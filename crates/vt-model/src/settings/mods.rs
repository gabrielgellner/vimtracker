//! The settings only modulation and macros move, and the mod slots' own: each mod slot's, each
//! macro's, the accumulator's and the arpeggio's.

use super::{Entry, Kind, Place, Units};
use crate::accumulator::MOST_DEGREES;
use crate::arpeggio::{ArpRate, MOST_OCTAVES};
use crate::macros::MACROS;
use crate::steps::{MAX_JUMP, MAX_MUTATE, STEPS};

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
pub const MOD_OWN_SETTINGS: usize = 11;
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
/// ADSR's, an AHD's and an LPG's `Attack` are one: unlabelled, as steps can't lock them. Each is on
/// the slot only while its type has it. They're in
/// [`MOD_OWN_FIELDS`](crate::modulation::MOD_OWN_FIELDS)' order, which names them.
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
