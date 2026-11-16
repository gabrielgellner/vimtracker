//! One setting described as anything that shows or edits it needs it (#873): a [`Descriptor`],
//! with its names, its range, how it steps ([`Steps`], [`Pace`]), what it lists ([`Choice`]), how
//! it's typed ([`Typing`], [`Reads`]) and how its values read ([`Reading`]). Each family of
//! settings produces one, so the pages, `K`, typing, the status line and the Roto read one
//! description, whatever the setting is.
//!
//! A descriptor holds no value: the program reads the value where the setting is and asks the
//! descriptor how it shows, steps and is kept. Nothing here names anything of a screen.

use std::borrow::Cow;

use crate::constants::BAR_STEPS;
use crate::lfo::Fade;
use crate::listed::Listed;
use crate::modulation::depth_text;
use crate::settings::{Kind, LONGEST_GLIDE_MS, Units, glide_value};
use crate::song::reset_named;
use crate::step::Note;
use crate::tracking::Source;

/// How many small steps an envelope time's large one is.
const TIME_JUMP: i32 = 10;
/// How many small steps a large one is, for a value in hex and for a list.
pub const HEX_JUMP: i32 = 0x10;

/// How far one step moves a value from where it is now, up and down. They differ where the step
/// depends on the value, as an envelope's time steps by 5 ms from 10 ms up but by 1 ms below it,
/// so `-` from 10 ms goes to 9 ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Steps {
    pub up: i32,
    pub down: i32,
}

impl Steps {
    /// A step of the same size either way.
    pub const fn even(by: i32) -> Self {
        Self { up: by, down: by }
    }

    /// No step: a setting moved by its own rule, as a rate through its kinds, or not at all.
    pub const NONE: Self = Self::even(0);
}

/// How a count's steps follow one another, as `3=` takes three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pace {
    /// Each the same, wherever it gets to: three steps go three times as far.
    #[default]
    Even,
    /// An envelope's time, whose steps grow as it gets longer.
    Time,
    /// An AHD's hold, which steps as a time and has `Note` below 0 ms.
    Hold,
    /// The track's `Reset`, whose large steps go from one whole bar to the next.
    Bars,
}

impl Pace {
    /// The small and the large step from a value, where they depend on it.
    pub fn steps(self, value: i32) -> Option<(Steps, Steps)> {
        let time = |value: i32| {
            let nudge = Steps {
                up: time_step(value),
                down: time_step(value - 1),
            };
            let jump = Steps {
                up: nudge.up * TIME_JUMP,
                down: nudge.down * TIME_JUMP,
            };
            (nudge, jump)
        };
        Some(match self {
            Self::Even => return None,
            Self::Time => time(value),
            // Up from `Note` is 0 ms, down from no time is `Note`, and a time steps down no
            // further than 0 ms.
            Self::Hold => match value {
                ..0 => (Steps::even(1), Steps::even(1)),
                0 => (
                    Steps::even(1),
                    Steps {
                        up: TIME_JUMP,
                        down: 1,
                    },
                ),
                _ => {
                    let (nudge, jump) = time(value);
                    let down = |steps: Steps| Steps {
                        down: steps.down.min(value),
                        ..steps
                    };
                    (down(nudge), down(jump))
                }
            },
            Self::Bars => {
                let bar = BAR_STEPS as i32;
                let jump = Steps {
                    up: (value / bar + 1) * bar - value,
                    down: value - ((value + bar - 1) / bar - 1).max(0) * bar,
                };
                (Steps::even(1), jump)
            }
        })
    }
}

/// How far a small step moves an envelope time. Short times need finer steps than long ones.
fn time_step(value: i32) -> i32 {
    match value {
        ..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}

/// How a setting's value is typed after `i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    /// It isn't: `i` lists its choices, as `K` does.
    Listed,
    /// It isn't: it's one of two, so `i` turns it over.
    Toggled,
    /// Two hex digits, kept as soon as the second is typed.
    Hex,
    /// Hex values, up to [`crate::shape::MAX_VALUES`], a space coming by itself after each two
    /// digits.
    HexList,
    /// A note's name or a nudge, alone or in a list with spaces between, kept with `Enter`.
    Named { list: bool },
    /// A rate, like `1/4`, `2 bars` or `x3`.
    Rate,
    /// A whole number of up to four digits, with a sign before it if `signed`.
    Whole { signed: bool },
    /// A hex digit for each of the scale's degrees, from the root, kept with `Enter`.
    Weights,
    /// The letters of a `Tonnetz`' moves, `P`, `L` and `R`, kept with `Enter`.
    Moves,
}

/// How `i`'s text reads as a value. Where it doesn't, or the value is outside its bounds, it's
/// refused with what the descriptor says why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Reads {
    /// Not at all: `i` lists it or turns it over, or it's typed its own way.
    #[default]
    No,
    /// Two hex digits, kept to its bounds, each worth `1 << shift` of it, as a macro's are its
    /// coarse byte.
    Hex { shift: u32 },
    /// Hex refused outside its bounds, with a sign before it where `signed`.
    HexWithin { signed: bool },
    /// A whole number, with any of the letter `before` it and any of the text `after` it, `off`
    /// for its lowest where `off` (a number then starting above it), counted from 1 where
    /// `from_one`.
    Number {
        before: Option<char>,
        after: &'static str,
        off: bool,
        from_one: bool,
    },
    /// A glide's time in ms, or `off`.
    Glide,
    /// A track's `Reset`: steps, bars like `2bar`, or `off`.
    Reset,
    /// An AHD's hold in ms, or `note`.
    Hold,
    /// A note by its name.
    Note,
}

/// A whole number as it's typed, nothing before it or after it.
pub const NUMBER: Reads = Reads::Number {
    before: None,
    after: "",
    off: false,
    from_one: false,
};

/// The same followed by a percent sign, or not.
pub const PERCENT: Reads = Reads::Number {
    before: None,
    after: "%",
    off: false,
    from_one: false,
};

impl Reads {
    /// What `text` sets a value within `bounds` to, or why not, which is `why_not`. Text that only
    /// hex digits are typed into, which can't be wrong, is refused with nothing to say.
    pub fn parse(self, text: &str, (low, high): (i32, i32), why_not: &str) -> Result<i32, String> {
        let refused = || why_not.to_owned();
        let kept = |value: Option<i32>, lowest: i32| {
            value
                .filter(|value| (lowest..=high).contains(value))
                .ok_or_else(refused)
        };
        match self {
            Self::No => Err(String::new()),
            Self::Hex { shift } => u8::from_str_radix(text, 16)
                .map(|byte| (i32::from(byte) << shift).clamp(low, high))
                .map_err(|_| String::new()),
            Self::HexWithin { signed } => {
                let (sign, digits) = match text.strip_prefix('-') {
                    Some(digits) if signed => (-1, digits),
                    _ if signed => (1, text.strip_prefix('+').unwrap_or(text)),
                    _ => (1, text.trim()),
                };
                let magnitude = u32::from_str_radix(digits, 16).ok();
                let value = magnitude.and_then(|magnitude| i32::try_from(magnitude).ok());
                kept(value.map(|value| sign * value), low)
            }
            Self::Number {
                before,
                after,
                off,
                from_one,
            } => {
                let text = text.trim();
                if off && text.eq_ignore_ascii_case("off") {
                    return Ok(low);
                }
                let digits = match before {
                    Some(letter) => text.trim_start_matches([
                        letter.to_ascii_uppercase(),
                        letter.to_ascii_lowercase(),
                    ]),
                    None => text,
                };
                let digits = match after.is_empty() {
                    true => digits,
                    false => digits.trim_end_matches(after).trim(),
                };
                let number = digits.parse::<i32>().ok();
                let number = number.map(|number| if from_one { number - 1 } else { number });
                // A number starts above `off`.
                kept(number, if off { low + 1 } else { low })
            }
            Self::Glide => {
                let text = text.trim();
                if text.eq_ignore_ascii_case("off") {
                    return Ok(0);
                }
                let ms = text.trim_end_matches("ms").trim().parse::<f32>().ok();
                ms.filter(|ms| (0.0..=LONGEST_GLIDE_MS).contains(ms))
                    .map(|ms| i32::from(glide_value(ms)))
                    .ok_or_else(refused)
            }
            Self::Reset => reset_named(text)
                .map(|reset| reset.map_or(0, i32::from))
                .ok_or_else(refused),
            // `Note` is below no time.
            Self::Hold => {
                let text = text.trim();
                if text.eq_ignore_ascii_case("note") {
                    return Ok(-1);
                }
                let ms = text.trim_end_matches("ms").trim().parse::<i32>().ok();
                kept(ms, 0)
            }
            Self::Note => {
                let midi = match text.trim().parse::<Note>() {
                    Ok(Note::On(midi)) => Some(i32::from(midi)),
                    _ => None,
                };
                kept(midi, low)
            }
        }
    }
}

/// One of the choices `K` lists for a setting: its name, what it does and the heading it's
/// listed under, either of them empty for none.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub name: Cow<'static, str>,
    pub meaning: Cow<'static, str>,
    pub group: Cow<'static, str>,
}

impl Choice {
    /// A choice with only a name.
    pub fn named(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            meaning: Cow::Borrowed(""),
            group: Cow::Borrowed(""),
        }
    }

    /// With what it does.
    pub fn meaning(self, meaning: impl Into<Cow<'static, str>>) -> Self {
        Self {
            meaning: meaning.into(),
            ..self
        }
    }

    /// One of a [`Listed`] kind, as its list names it, says what it does and heads it.
    pub fn of<X: Listed>(each: X) -> Self {
        Self {
            group: Cow::Borrowed(each.category()),
            ..Self::named(each.name()).meaning(each.meaning())
        }
    }
}

/// How a setting's values read, the one vocabulary every family's settings share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reading {
    /// Two hex digits.
    Hex,
    /// A registry setting's byte: two hex digits on a bar, and in real units on the status line
    /// with its fine part, as [`Kind::shown`] says them.
    Byte {
        units: Units,
        top: u8,
        middle: bool,
    },
    /// One of its choices, by its place among them: the choice's name.
    Listed,
    /// Off or on, as `on` and `off`.
    Switch,
    /// A whole number.
    Number,
    /// Milliseconds, whose steps grow as it gets longer.
    Time,
    /// An AHD's hold: milliseconds, or `Note` below none.
    Hold,
    Percent,
    /// How many, `what` saying of what, shown counted from 1 where `from_one`.
    Count {
        what: &'static str,
        from_one: bool,
    },
    /// Hex either side of none, with its sign, as a depth is.
    SignedHex,
    /// Degrees round a cycle, read as the axis heard at each third of the way round where it has
    /// `axes`, as an attractor's phase does.
    Degrees {
        axes: bool,
    },
    /// A division or hertz, or Steps' own rate, which isn't one number and reads as its holder
    /// says.
    Rate,
    /// An LFO's fade, by its place among [`Fade::ALL`].
    Fade,
    /// What a Track mod slot's map follows: a note, or hex, as its source reads.
    Mapped(Source),
}

/// How a value shows: in its cell, and on the status line after its name, where that says more
/// than the cell.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shown {
    pub cell: String,
    pub text: String,
}

/// One setting where it is, as anything that shows or edits it needs it described: its names, its
/// range, how it steps, how it's typed and how its values read. It holds no value.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    /// What songs and the Roto's layouts name it by.
    pub key: &'static str,
    /// Its full name, for lists and the status line.
    pub name: &'static str,
    /// The grid's short label, where it has one.
    pub label: Option<&'static str>,
    pub reading: Reading,
    pub bounds: (i32, i32),
    /// What `x` puts back.
    pub default: i32,
    /// How far `=` and `-`, and `+` and `_`, move it, where its pace doesn't say otherwise.
    pub nudge: i32,
    pub jump: i32,
    /// How a count's steps follow one another.
    pub pace: Pace,
    /// Goes round past either end: a phase, a list that wraps.
    pub wraps: bool,
    /// Its bar draws from the middle: pan, trim, a band's boost.
    pub centred: bool,
    /// What `K` lists for it, in order, or none for a setting that isn't a list.
    pub choices: Vec<Choice>,
    /// How `i` takes it, and how what it takes reads as a value.
    pub typing: Typing,
    pub reads: Reads,
    /// What the status line says when typing doesn't read.
    pub why_not: Cow<'static, str>,
    /// Whether it has a bar beside its value.
    pub barred: bool,
}

impl Descriptor {
    /// A setting with only its names, at 0, that doesn't step and reads as a number.
    fn named(key: &'static str, name: &'static str) -> Self {
        Self {
            key,
            name,
            label: None,
            reading: Reading::Number,
            bounds: (0, 0),
            default: 0,
            nudge: 0,
            jump: 0,
            pace: Pace::Even,
            wraps: false,
            centred: false,
            choices: Vec::new(),
            typing: Typing::Listed,
            reads: Reads::No,
            why_not: Cow::Borrowed(""),
            barred: false,
        }
    }

    /// Two hex digits from `00` to `top`, with a bar.
    pub fn hex(key: &'static str, name: &'static str, top: u8, default: u8) -> Self {
        Self {
            reading: Reading::Hex,
            bounds: (0, top as i32),
            default: default as i32,
            nudge: 1,
            jump: HEX_JUMP,
            typing: Typing::Hex,
            reads: Reads::Hex { shift: 0 },
            barred: true,
            ..Self::named(key, name)
        }
    }

    /// One of a list, one step at a time either way, going round past the ends if it `wraps`.
    pub fn listed(
        key: &'static str,
        name: &'static str,
        choices: impl IntoIterator<Item = Choice>,
        default: usize,
        wraps: bool,
    ) -> Self {
        let choices: Vec<Choice> = choices.into_iter().collect();
        Self {
            reading: Reading::Listed,
            bounds: (0, choices.len().saturating_sub(1) as i32),
            default: default as i32,
            nudge: 1,
            jump: HEX_JUMP,
            wraps,
            choices,
            ..Self::named(key, name)
        }
    }

    /// Off or on: `=` puts it on and `-` takes it off.
    pub fn switch(key: &'static str, name: &'static str, default: bool) -> Self {
        Self {
            reading: Reading::Switch,
            bounds: (0, 1),
            default: default as i32,
            nudge: 1,
            jump: 1,
            typing: Typing::Toggled,
            ..Self::named(key, name)
        }
    }

    /// A whole number from `low` to `high`, typed in decimal.
    pub fn whole(
        key: &'static str,
        name: &'static str,
        (low, high): (i32, i32),
        default: i32,
    ) -> Self {
        Self {
            bounds: (low, high),
            default,
            nudge: 1,
            jump: 1,
            typing: Typing::Whole { signed: low < 0 },
            reads: NUMBER,
            ..Self::named(key, name)
        }
    }

    /// How a value shows, with its fine byte where it has one.
    pub fn shown(&self, value: i32, fine: Option<u8>) -> Shown {
        let text = match self.reading {
            Reading::Byte { units, top, middle } => {
                let byte = u16::from(value.clamp(0, 0xFF) as u8);
                Kind::Continuous { top, middle, units }
                    .shown(byte << 8 | u16::from(fine.unwrap_or(0)))
            }
            _ => String::new(),
        };
        let cell = match self.reading {
            Reading::Hex | Reading::Byte { .. } => format!("{value:02X}"),
            Reading::Listed => usize::try_from(value)
                .ok()
                .and_then(|at| self.choices.get(at))
                .map_or_else(String::new, |choice| choice.name.clone().into_owned()),
            Reading::Switch => if value != 0 { "on" } else { "off" }.to_owned(),
            Reading::Number => value.to_string(),
            Reading::Hold if value < 0 => "Note".to_owned(),
            Reading::Time | Reading::Hold => format!("{value} ms"),
            Reading::Percent => format!("{value}%"),
            Reading::Count { from_one, .. } => (value + i32::from(from_one)).to_string(),
            Reading::SignedHex => depth_text(value as i16),
            Reading::Degrees { axes } => match value {
                0 if axes => "x".to_owned(),
                120 if axes => "y".to_owned(),
                240 if axes => "z".to_owned(),
                _ => format!("{value}°"),
            },
            Reading::Rate => String::new(),
            Reading::Fade => usize::try_from(value)
                .ok()
                .and_then(|at| Fade::ALL.get(at))
                .map_or_else(String::new, ToString::to_string),
            Reading::Mapped(source) => source.text(value as u8),
        };
        Shown { cell, text }
    }

    /// Where a value sits on its bar, 0 to 255, or none for a setting with no bar.
    pub fn bar(&self, value: i32) -> Option<u8> {
        match self.reading {
            _ if !self.barred => None,
            Reading::Hex | Reading::Byte { .. } => Some(value.clamp(0, 0xFF) as u8),
            _ => None,
        }
    }

    /// Its small and its large step from a value, by its pace.
    pub fn steps(&self, value: i32) -> (Steps, Steps) {
        self.pace
            .steps(value)
            .unwrap_or((Steps::even(self.nudge), Steps::even(self.jump)))
    }

    /// What `i`'s text sets it to, or why not.
    pub fn parse(&self, text: &str) -> Result<i32, String> {
        self.reads.parse(text, self.bounds, &self.why_not)
    }

    /// A value kept within its bounds, or taken round them where it wraps.
    pub fn kept(&self, value: i32) -> i32 {
        kept(i64::from(value), self.bounds, self.wraps)
    }
}

/// A value kept within `bounds`, or taken round them where it goes `round`.
pub fn kept(value: i64, (low, high): (i32, i32), round: bool) -> i32 {
    let (low, high) = (i64::from(low), i64::from(high));
    let kept = match round && high > low {
        true => low + (value - low).rem_euclid(high - low + 1),
        false => value.clamp(low, high),
    };
    kept as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_steps_finer_while_its_short() {
        let (nudge, jump) = Pace::Time.steps(10).unwrap();
        assert_eq!(nudge, Steps { up: 5, down: 1 });
        assert_eq!(jump, Steps { up: 50, down: 10 });
        assert_eq!(Pace::Even.steps(10), None);
    }

    #[test]
    fn a_hold_steps_from_no_time_to_the_note() {
        let (nudge, jump) = Pace::Hold.steps(0).unwrap();
        assert_eq!(nudge, Steps::even(1));
        assert_eq!(jump, Steps { up: 10, down: 1 });
        let (_, jump) = Pace::Hold.steps(5).unwrap();
        assert_eq!(jump.down, 5, "no further than 0 ms");
    }

    #[test]
    fn a_reset_jumps_to_the_next_whole_bar() {
        let bar = BAR_STEPS as i32;
        let (_, jump) = Pace::Bars.steps(bar + 3).unwrap();
        assert_eq!(
            jump,
            Steps {
                up: bar - 3,
                down: 3
            }
        );
    }

    #[test]
    fn a_hex_setting_reads_steps_and_is_kept() {
        let hex = Descriptor::hex("cut", "Cut", 0xE0, 0x80);
        assert_eq!(hex.shown(0x2A, None).cell, "2A");
        assert_eq!(hex.bar(0x2A), Some(0x2A));
        assert_eq!(hex.steps(0x2A), (Steps::even(1), Steps::even(0x10)));
        assert_eq!(hex.parse("ff"), Ok(0xE0), "kept to its top");
        assert_eq!(hex.parse("g"), Err(String::new()));
        assert_eq!(hex.kept(0x100), 0xE0);
    }

    #[test]
    fn a_list_reads_its_choice_and_goes_round() {
        let listed = Descriptor::listed(
            "mode",
            "Mode",
            ["Up", "Down", "Both"].map(Choice::named),
            0,
            true,
        );
        assert_eq!(listed.shown(2, None).cell, "Both");
        assert_eq!(listed.shown(3, None).cell, "");
        assert_eq!(listed.bar(1), None);
        assert_eq!(listed.kept(3), 0);
        assert_eq!(listed.kept(-1), 2);
    }

    #[test]
    fn a_number_is_refused_outside_its_bounds_in_its_own_words() {
        let whole = Descriptor {
            reads: Reads::Number {
                before: Some('G'),
                after: "",
                off: true,
                from_one: false,
            },
            why_not: Cow::Borrowed("Group is a group from 1 to 4, or off"),
            ..Descriptor::whole("group", "Group", (0, 4), 0)
        };
        assert_eq!(whole.parse("off"), Ok(0));
        assert_eq!(whole.parse("g3"), Ok(3));
        assert_eq!(
            whole.parse("0"),
            Err("Group is a group from 1 to 4, or off".to_owned())
        );
        assert_eq!(whole.shown(3, None).cell, "3");
    }

    #[test]
    fn a_switch_reads_in_words() {
        let switch = Descriptor::switch("safe", "Safe", true);
        assert_eq!(switch.shown(1, None).cell, "on");
        assert_eq!(switch.shown(0, None).cell, "off");
        assert_eq!(switch.default, 1);
        assert_eq!(switch.typing, Typing::Toggled);
    }
}
