//! The settings only modulation and macros move, and the mod slots' own: each mod slot's, each
//! macro's, the accumulator's and the arpeggio's.

use super::{Entry, Kind, Place, Units};
use crate::accumulator::MOST_DEGREES;
use crate::arpeggio::{ArpRate, MOST_OCTAVES};
use crate::envelope::{Curve, MAX_RATCHET};
use crate::lfo::{LfoMode, Shape};
use crate::macros::MACROS;
use crate::steps::{MAX_JUMP, MAX_MUTATE, STEPS, StepsMode};

/// A mod slot's settings that steps can lock: its depth and polarity, whatever its type, and an
/// LFO's phase and deform, in [`MOD_LOCKED_FIELDS`](crate::modulation::MOD_LOCKED_FIELDS)' order.
pub(super) const MOD: [Entry; 4] = [
    Entry {
        meaning: "how far it moves its setting at most, a minus turning it over",
        key: "depth",
        name: "Depth",
        // With the slot's number after it, as `DEP1`.
        label: Some("DEP"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Depth,
        },
        default: 0x80,
        place: Place::Own,
    },
    Entry {
        meaning: "where in its cycle a note starts it",
        key: "phase",
        name: "Phase",
        label: Some("PHS"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Degrees,
        },
        default: 0x00,
        place: Place::Own,
    },
    Entry {
        meaning: "whether it moves what it reaches one way or both ways",
        key: "polarity",
        name: "Polarity",
        label: Some("POL"),
        kind: Kind::Stepped {
            choices: &POLARITY_NAMES,
            wraps: false,
        },
        // Each type has its own, which the track's value gives; this is an LFO's.
        default: 0x01,
        place: Place::Own,
    },
    Entry {
        meaning: "how its wave bends away from its plain shape",
        key: "deform",
        name: "Deform",
        label: Some("DFM"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Depth,
        },
        default: 0x80,
        place: Place::Own,
    },
];

/// A polarity's names, in the order of its values.
const POLARITY_NAMES: [&str; 2] = ["Uni", "Bi"];

/// How many of a mod slot's type's own settings the registry holds, and of the accumulator's and
/// the arpeggio's together.
pub const MOD_OWN_SETTINGS: usize = 20;
pub const PLAYED_SETTINGS: usize = ACCUMULATION.len() + ARPEGGIO.len();

/// A macro, the same entry for all eight but its name: where it's turned, from `00` to `FF` with a
/// fine part, as a cutoff is. With its number after it, as `MAC1`.
pub(super) const MACRO: Entry = Entry {
    meaning: "where the macro is turned, moving every setting it holds",
    key: "macro",
    name: "Macro",
    label: Some("MAC"),
    kind: Kind::Continuous {
        top: 0xFF,
        middle: false,
        units: Units::Percent,
    },
    default: 0,
    place: Place::Own,
};

/// What each macro is called in lists before it's named.
pub(super) const MACRO_NAMES: [&str; MACROS] = [
    "Macro 1", "Macro 2", "Macro 3", "Macro 4", "Macro 5", "Macro 6", "Macro 7", "Macro 8",
];

/// Whole numbers from 1, for a count chosen from a list, as Steps' `Length` is.
const COUNTS: [&str; MOST_DEGREES as usize] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17",
    "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28",
];

/// An envelope's time, unlabelled, so only modulation and macros move it.
const fn time(key: &'static str, name: &'static str) -> Entry {
    Entry {
        meaning: "",
        key,
        name,
        label: None,
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::EnvelopeTime,
        },
        default: 0,
        place: Place::Own,
    }
}

/// A percentage from none to all, unlabelled, as an envelope's sustain is.
const fn percentage(key: &'static str, name: &'static str) -> Entry {
    Entry {
        meaning: "",
        key,
        name,
        label: None,
        kind: Kind::Continuous {
            top: 100,
            middle: false,
            units: Units::Percent,
        },
        default: 0,
        place: Place::Own,
    }
}

/// A mod slot's type's own settings that a macro or modulation can move, by what they are, so an
/// ADSR's, an AHD's and an LPG's `Attack` are one. The times, levels and amounts first are
/// unlabelled, as a step's lock of one would be heard only from the next note; the lists, the
/// hold, the peak, the fade and the ratchet after them are labelled, so steps lock them too, with
/// the slot's number after the label, as `SHP4`. Each is on the slot only while its type has it.
/// They're in [`MOD_OWN_FIELDS`](crate::modulation::MOD_OWN_FIELDS)' order, which names them.
pub(super) const MOD_OWN: [Entry; MOD_OWN_SETTINGS] = [
    time("attack", "Attack").means("how long it takes to rise to full, or the gate to open"),
    time("decay", "Decay").means("how long it takes to fall back"),
    percentage("sustain", "Sustain").means("the level it holds at while the note does"),
    time("release", "Release").means("how long it takes to fall away once the note ends"),
    percentage("body", "Body").means("the level the strike falls to before it decays"),
    Entry {
        meaning: "how far the gate's fall closes the filter as well as the level",
        key: "colour",
        name: "Colour",
        label: None,
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "how many of its values it plays",
        key: "length",
        name: "Length",
        label: None,
        kind: Kind::Stepped {
            choices: COUNTS.split_at(STEPS).0,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "the chance each value is drawn again as it comes round",
        key: "mutate",
        name: "Mutate",
        label: None,
        kind: Kind::Continuous {
            top: MAX_MUTATE,
            middle: false,
            units: Units::Percent,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "how it goes from one value to the next, gliding or jumping",
        key: "smooth",
        name: "Smooth",
        label: None,
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Depth,
        },
        default: 0x80,
        place: Place::Own,
    },
    // An LFO's and Steps' `Rate`, faster up the scale, through its own kind of rate; the default is
    // a new LFO's `1/4`.
    Entry {
        meaning: "how fast it cycles, or how long each value lasts",
        key: "rate",
        name: "Rate",
        label: None,
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default: 0x7A,
        place: Place::Own,
    },
    Entry {
        meaning: "the chance each step goes to one of its values at random",
        key: "jump",
        name: "Jump",
        label: None,
        kind: Kind::Continuous {
            top: MAX_JUMP,
            middle: false,
            units: Units::Percent,
        },
        default: 0,
        place: Place::Own,
    },
    // An LFO's wave, swapped where it is in its cycle; the default is a new LFO's `Tri`.
    listed("shape", "Shape", "SHP", &SHAPE_NAMES, 1).means("the wave the LFO cycles through"),
    // Steps' end, from where it's counted to; the default is a new slot's `from 1`.
    listed("loop", "Loop", "LOOP", &LOOP_NAMES, 1)
        .means("what Steps does once it reaches its last value"),
    // An envelope's mode, an ADSR's, an AHD's and a drum's alike.
    listed("curve", "Curve", "CRV", &CURVE_NAMES, 0).means("how the envelope's rise and fall bend"),
    // An LFO's and Steps' modes, each a list of its own, as they mean different things.
    listed("lfomode", "Mode", "LMOD", &LFO_MODE_NAMES, 0)
        .means("how notes start the LFO's cycle, or leave it running"),
    listed("stepsmode", "Mode", "SMOD", &STEPS_MODE_NAMES, 0)
        .means("whether Steps' values go by in time or one a note"),
    // An AHD's hold, on a scale of its own (`hold_at`); the default is a new AHD's 30 ms.
    Entry {
        meaning: "how long the AHD stays at full, or, at the top, for as long as the note",
        key: "hold",
        name: "Hold",
        label: Some("HOLD"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::HoldTime,
        },
        default: 0x72,
        place: Place::Own,
    },
    // A drum's peak, on the envelopes' time scale; the default is a new drum's 10 ms.
    Entry {
        label: Some("PEAK"),
        default: 0x42,
        ..time("peak", "Peak").means("how long the drum's first strike lasts before its body")
    },
    // An LFO's fade, from the longest out through none to the longest in, as `+` goes; the
    // default is none.
    listed(
        "fade",
        "Fade",
        "FADE",
        &FADE_NAMES,
        FADE_NAMES.len() as u8 / 2,
    )
    .means("how the LFO fades in or out after each note starts it"),
    // How many times a drum strikes, from 1.
    listed(
        "ratchet",
        "Ratchet",
        "RTCH",
        COUNTS.split_at(MAX_RATCHET as usize).0,
        0,
    )
    .means("how many times the drum strikes on each note"),
];

/// A list of a mod slot's type's own that a step can lock, by its names.
const fn listed(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    choices: &'static [&'static str],
    default: u8,
) -> Entry {
    Entry {
        meaning: "",
        key,
        name,
        label: Some(label),
        kind: Kind::Stepped {
            choices,
            wraps: false,
        },
        default,
        place: Place::Own,
    }
}

/// The names of every one of a type that has a list of them, in its order, made as the program is
/// built.
macro_rules! names {
    ($ty:ty) => {{
        let mut names = [""; <$ty>::ALL.len()];
        let mut at = 0;
        while at < names.len() {
            names[at] = <$ty>::ALL[at].name();
            at += 1;
        }
        names
    }};
}

const SHAPE_NAMES: [&str; Shape::ALL.len()] = names!(Shape);
const CURVE_NAMES: [&str; Curve::ALL.len()] = names!(Curve);
const LFO_MODE_NAMES: [&str; LfoMode::ALL.len()] = names!(LfoMode);
const STEPS_MODE_NAMES: [&str; StepsMode::ALL.len()] = names!(StepsMode);

/// Steps' ends, in [`Loop::ALL`](crate::steps::Loop::ALL)'s order, as they show.
pub(crate) const LOOP_NAMES: [&str; STEPS + 3] = [
    "Once", "from 1", "from 2", "from 3", "from 4", "from 5", "from 6", "from 7", "from 8",
    "from 9", "from 10", "from 11", "from 12", "from 13", "from 14", "from 15", "from 16",
    "Bounce", "Shuffle",
];

/// An LFO's fades, in [`Fade::ALL`](crate::lfo::Fade::ALL)'s order, as they show: out over the
/// longest to the shortest, none, then in over the shortest to the longest.
pub(crate) const FADE_NAMES: [&str; 45] = [
    "-16 bars", "-8 bars", "-4 bars", "-2 bars", "-1 bar", "-1/2d", "-1/2", "-1/4d", "-1/2t",
    "-1/4", "-1/8d", "-1/4t", "-1/8", "-1/16d", "-1/8t", "-1/16", "-1/32d", "-1/16t", "-1/32",
    "-1/64d", "-1/32t", "-1/64", "Off", "+1/64", "+1/32t", "+1/64d", "+1/32", "+1/16t", "+1/32d",
    "+1/16", "+1/8t", "+1/16d", "+1/8", "+1/4t", "+1/8d", "+1/4", "+1/2t", "+1/4d", "+1/2",
    "+1/2d", "+1 bar", "+2 bars", "+4 bars", "+8 bars", "+16 bars",
];

/// The accumulator's settings a macro or modulation can move: how far up and down it goes.
pub(super) const ACCUMULATION: [Entry; 2] = [
    Entry {
        meaning: "how far above the note as written the accumulator can go",
        key: "up",
        name: "Up",
        label: None,
        kind: Kind::Stepped {
            choices: &COUNTS,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "how far below the note as written the accumulator can go",
        key: "down",
        name: "Down",
        label: None,
        kind: Kind::Stepped {
            choices: &COUNTS,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
];

/// The arpeggio's rates' names, in [`ArpRate::ALL`]'s order.
const ARP_RATE_NAMES: [&str; ArpRate::ALL.len()] = {
    let mut names = [""; ArpRate::ALL.len()];
    let mut at = 0;
    while at < names.len() {
        names[at] = ArpRate::ALL[at].name();
        at += 1;
    }
    names
};

/// The arpeggio's settings a macro or modulation can move: how fast and over how many octaves.
pub(super) const ARPEGGIO: [Entry; 2] = [
    Entry {
        meaning: "how fast the arpeggio plays its notes",
        key: "rate",
        name: "Rate",
        label: None,
        kind: Kind::Stepped {
            choices: &ARP_RATE_NAMES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "how many octaves the arpeggio climbs before it starts again",
        key: "octaves",
        name: "Octaves",
        label: None,
        kind: Kind::Stepped {
            choices: COUNTS.split_at(MOST_OCTAVES as usize).0,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
];
