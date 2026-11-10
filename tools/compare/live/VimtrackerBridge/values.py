"""Setting a Live device's parameter to a value as Live shows it, such as `-20 dB` or `10 ms`, and
reading back what it shows.

A parameter's `value` is in Live's own units, which are often not the ones on screen: Glue
Compressor's Threshold may run from 0 to 1, say, while it shows `-20.0 dB`. The only published way
from one to the other is `str_for_value`, which gives the text Live would show for a value. So a
continuous parameter is set by searching its range for the values that show the one wanted, and
taking the middle of them; a quantized one, a switch or a list, by finding the wanted item among
its `value_items`.

This runs inside Live, in its own Python (3.11 in Live 12), and in the harness, which uses
`matches` to check what Live read back. It uses nothing outside the standard library.
"""

import math
import re

# A shown number, then its unit: "-20.0 dB", "10.0 ms", "1.20 s", "100 %", "1.00 kHz", "-inf dB".
SHOWN = re.compile(r"^\s*([+-]?(?:inf|\d+(?:\.\d*)?|\.\d+))\s*([^\d\s][^\s]*)?\s*$", re.IGNORECASE)
# Each unit with a prefix, as its unit without one and how many of that it is.
SCALED = {
    "ms": ("s", 1e-3),
    "us": ("s", 1e-6),
    "µs": ("s", 1e-6),
    "khz": ("hz", 1e3),
}
# How far apart two numbers can be and still be the same, relative to their size, as `100 ms`
# read as seconds isn't exactly 0.1.
EPSILON = 1e-9
# How many times a search halves the range it looks in: 2^-60 of it is far finer than any display.
HALVINGS = 60


class Unsettable(Exception):
    """A value a parameter can't be set to, with what it can be."""


def parse(text):
    """A shown value as (number, unit, number in the unscaled unit), the unit in lower case and
    `None` when there's none, or None when it isn't a number, such as `A` or `Off`."""
    found = SHOWN.match(str(text))
    if not found:
        return None
    number = float(found.group(1))
    unit = (found.group(2) or "").strip().lower() or None
    if unit in SCALED:
        base, scale = SCALED[unit]
        return number, unit, base, number * scale
    return number, unit, unit, number


def decimals(text):
    """How many decimals a shown number has, which says how finely it's rounded."""
    found = SHOWN.match(str(text))
    if not found or "." not in found.group(1):
        return 0
    return len(found.group(1).split(".")[1])


def target_of(wanted):
    """What a spec asks for, as (number, unit or None), or None for a name such as `Overdrive`."""
    if isinstance(wanted, bool):
        return None
    if isinstance(wanted, (int, float)):
        return float(wanted), None
    parsed = parse(wanted)
    if parsed is None:
        return None
    number, unit, base, scaled = parsed
    return (scaled, base) if unit is not None else (number, None)


def shown_number(text, unit):
    """A shown value as a number in the spec's terms: in its unscaled unit when the spec gave a unit,
    so `100 ms` meets `0.10 s`, or as the number shown when it gave a bare number."""
    parsed = parse(text)
    if parsed is None:
        return None
    number, shown_unit, base, scaled = parsed
    if unit is None:
        return number
    if base != unit:
        return None
    return scaled


def matches(wanted, shown):
    """Whether what Live shows is what the spec asked for, to the shown value's own rounding."""
    if isinstance(wanted, bool):
        return normal(shown) in (("on", "1", "true") if wanted else ("off", "0", "false"))
    target = target_of(wanted)
    if target is None:
        return item_matches(str(wanted), str(shown))
    number, unit = target
    got = shown_number(shown, unit)
    if got is None:
        return False
    if math.isinf(number) or math.isinf(got):
        return number == got
    parsed = parse(shown)
    scale = SCALED[parsed[1]][1] if unit is not None and parsed[1] in SCALED else 1.0
    # Half the last shown digit, and a hair more for the arithmetic.
    tolerance = 0.5 * 10.0 ** -decimals(shown) * scale * (1 + 1e-9) + EPSILON * abs(number)
    return abs(got - number) <= tolerance


def normal(text):
    return "".join(str(text).lower().split())


def item_matches(wanted, item):
    """Whether a list's item is the one wanted: the same but for case and spaces, or one the start
    of the other, so `Distort` finds `Distortion`."""
    a, b = normal(wanted), normal(item)
    return bool(a) and bool(b) and (a == b or a.startswith(b) or b.startswith(a))


def choose_item(name, wanted, items):
    """The index of the item wanted among a quantized parameter's items: an exact match first,
    then one that starts the same, then by number for a bare or unit-bearing number."""
    items = [str(item) for item in items]
    for index, item in enumerate(items):
        if normal(item) == normal(wanted):
            return index
    if isinstance(wanted, bool):
        if len(items) == 2:
            return 1 if wanted else 0
        raise Unsettable(f"{name} isn't a switch; its values are {', '.join(items)}")
    target = target_of(wanted)
    if target is not None:
        for index, item in enumerate(items):
            if matches(wanted, item):
                return index
    else:
        found = [index for index, item in enumerate(items) if item_matches(str(wanted), item)]
        if len(found) == 1:
            return found[0]
    raise Unsettable(f"{name} can't be {wanted}; its values are {', '.join(items)}")


def value_for(parameter, wanted):
    """The value to give a parameter so Live shows what's wanted: a number with or without its
    unit, a list's item by name, `true` or `false` for a switch, or `min`, `max` or `default`."""
    name = parameter.name
    low, high = float(parameter.min), float(parameter.max)
    if isinstance(wanted, str) and normal(wanted) in ("min", "max", "default"):
        word = normal(wanted)
        if word == "min":
            return low
        if word == "max":
            return high
        if parameter.is_quantized:
            raise Unsettable(f"{name} is a list, which has no default to ask for")
        return float(parameter.default_value)
    if parameter.is_quantized:
        items = list(parameter.value_items) if parameter.value_items else []
        if items:
            return low + choose_item(name, wanted, items)
        if isinstance(wanted, bool):
            return high if wanted else low
    if isinstance(wanted, bool):
        raise Unsettable(f"{name} isn't a switch; give it a number")
    target = target_of(wanted)
    if target is None:
        raise Unsettable(f"{name} takes a number, such as {parameter.str_for_value(low)}, not {wanted}")
    return search(parameter, target[0], target[1])


def search(parameter, number, unit):
    """The middle of the values a continuous parameter shows `number` at, found by halving, as
    what's shown only grows, or only falls, over its range."""
    name = parameter.name
    low, high = float(parameter.min), float(parameter.max)

    def shown(value):
        return shown_number(parameter.str_for_value(value), unit)

    bottom, top = shown(low), shown(high)
    if bottom is None or top is None:
        raise Unsettable(
            f"{name} shows {parameter.str_for_value(low)} to {parameter.str_for_value(high)}, "
            f"which can't be read as {number:g}{' ' + unit if unit else ''}"
        )
    rising = top >= bottom
    least, most = (bottom, top) if rising else (top, bottom)
    if number < least or number > most:
        raise Unsettable(
            f"{name} runs from {parameter.str_for_value(low)} to {parameter.str_for_value(high)}, "
            f"so it can't be {number:g}{' ' + unit if unit else ''}"
        )

    def under(value, inclusive):
        """Whether a value shows less than the number, in the direction the parameter runs."""
        got = shown(value)
        if got is None:
            return True
        if not rising:
            got, target = -got, -number
        else:
            target = number
        slack = EPSILON * max(abs(target), 1e-12)
        return got <= target + slack if inclusive else got < target - slack

    # The first value that shows the number or more, and the last that shows it or less.
    a, b = low, high
    for _ in range(HALVINGS):
        middle = (a + b) / 2.0
        if under(middle, inclusive=False):
            a = middle
        else:
            b = middle
    first = b
    a, b = low, high
    for _ in range(HALVINGS):
        middle = (a + b) / 2.0
        if under(middle, inclusive=True):
            a = middle
        else:
            b = middle
    last = a
    if first <= last:
        return (first + last) / 2.0
    # Nothing shows the number exactly: the nearer of the values either side.
    near = min((last, first), key=lambda value: abs((shown(value) or 0.0) - number))
    return min(max(near, low), high)
