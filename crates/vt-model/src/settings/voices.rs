//! Each synth voice's settings, in the order it takes them, which its
//! [`VoiceSpec`](crate::machine::VoiceSpec) in [`VOICES`](crate::machine::VOICES) names.

use super::{Entry, Kind, Place, Units, centred, level, same_key};
use crate::machine::{DEFAULT_ACCENT, SynthVoice};
use crate::patch_names::PATCH_NAMES;

/// The most settings a machine has: `4-op`'s, four it shares and ten for each of its four
/// operators.
pub const MAX_MACHINE_SETTINGS: usize = 44;
/// How many settings `4-op` has before its operators', and how many each operator has.
pub const FOUR_OP_SHARED: usize = 4;
pub const FOUR_OP_EACH: usize = 10;

const ACCENT: Entry = level("accent", "Accent", "ACC", DEFAULT_ACCENT);
/// How far each note of an analog-style voice strays, from none at `00`: its oscillators' tuning,
/// a slow wander in pitch, its cutoff, level and envelope times, and, but for `Acid`'s, which
/// always runs free, where its oscillators start.
const DRIFT: Entry = level("drift", "Drift", "DRFT", 0x00);

pub(crate) const VA: [Entry; 4] = [
    centred("detune", "Detune", "DTUN"),
    level("square", "Square", "SQR", 0x00),
    level("saw", "Saw", "SAW", 0x80),
    DRIFT,
];
pub(crate) const FM: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80),
    level("index", "Index", "INDX", 0x00),
    centred("feedback", "Feedback", "FDBK"),
];
pub(crate) const VCF: [Entry; 4] = [
    centred("resonance", "Resonance", "RESO"),
    level("frequency", "Frequency", "FREQ", 0x80),
    level("wave", "Wave", "WAVE", 0x40),
    DRIFT,
];
pub(crate) const PD: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80),
    level("amount", "Amount", "AMNT", 0x80),
    level("skew", "Skew", "SKEW", 0x40),
];
pub(crate) const FORMANT: [Entry; 3] = [
    Entry {
        default: 0x99,
        ..centred("interval", "Interval", "INTV")
    },
    level("formant", "Formant", "FRMT", 0x80),
    level("width", "Width", "WDTH", 0x80),
];
pub(crate) const ADDITIVE: [Entry; 3] = [
    level("bumps", "Bumps", "BMPS", 0x00),
    level("peak", "Peak", "PEAK", 0x4D),
    level("shape", "Shape", "SHPE", 0x80),
];
pub(crate) const SWARM: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x66),
    level("density", "Density", "DENS", 0x80),
    level("overlap", "Overlap", "OVLP", 0x99),
];
/// How many saws `Supersaw` stacks.
const SAW_COUNTS: [&str; 8] = ["1", "2", "3", "4", "5", "6", "7", "8"];
/// What `Supersaw`'s stack is: every saw on its own pitch, or every other one a fifth up or an
/// octave down.
const SUPERSAW_MODES: [&str; 3] = ["Unison", "Fifths", "Sub"];
/// `Supersaw`'s: seven saws a little apart, as Roland's has, all as loud, half wide, each starting
/// anywhere in its cycle.
pub(crate) const SUPERSAW: [Entry; 9] = [
    Entry {
        key: "stack",
        name: "Stack",
        label: Some("STCK"),
        kind: Kind::Stepped {
            choices: &SUPERSAW_MODES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    level("detune", "Detune", "DTUN", 0x60),
    centred("warp", "Warp", "WARP"),
    level("mix", "Mix", "MIX", 0xFF),
    Entry {
        key: "saws",
        name: "Saws",
        label: Some("SAWS"),
        kind: Kind::Stepped {
            choices: &SAW_COUNTS,
            wraps: false,
        },
        default: 6,
        place: Place::Own,
    },
    level("shape", "Shape", "SHPE", 0x00),
    level("width", "Width", "WDTH", 0x80),
    level("phase", "Phase", "PHAS", 0xFF),
    DRIFT,
];
/// A continuous setting from `00` to `FF` that reads in these units.
const fn in_units(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    default: u8,
    units: Units,
) -> Entry {
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units,
        },
        ..level(key, name, label, default)
    }
}
/// The acid voice's waves: the saw, and the square a TB-303 shapes from it.
const ACID_WAVES: [&str; 2] = ["Saw", "Square"];
/// The acid voice's, in the order of a TB-303's knobs, then two of the Devil Fish's, then `Drift`.
pub(crate) const ACID: [Entry; 9] = [
    Entry {
        key: "wave",
        name: "Wave",
        label: Some("WAVE"),
        kind: Kind::Stepped {
            choices: &ACID_WAVES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    in_units("cutoff", "Cutoff", "FREQ", 0x80, Units::AcidCutoff),
    level("resonance", "Resonance", "RESO", 0x80),
    level("envmod", "Env mod", "EMOD", 0x80),
    in_units("decay", "Decay", "DCAY", 0x80, Units::AcidDecay),
    level("accent", "Accent", "ACC", 0x80),
    in_units("slide", "Slide", "SLID", 0x00, Units::Slide),
    in_units("drive", "Drive", "DRIV", 0x00, Units::AcidDrive),
    DRIFT,
];
/// The chords Chord and Chiptune play, in the order Plaits lists them.
const CHORD_NAMES: [&str; 11] = [
    "Octave", "5", "sus4", "m", "m7", "m9", "m11", "6/9", "M9", "M7", "M",
];
/// A chord, chosen from the list. It starts major.
const CHORD_TYPE: Entry = Entry {
    key: "chord",
    name: "Chord",
    label: Some("CHRD"),
    kind: Kind::Stepped {
        choices: &CHORD_NAMES,
        wraps: false,
    },
    default: 10,
    place: Place::Own,
};
pub(crate) const TERRAIN: [Entry; 3] = [
    level("terrain", "Terrain", "TERR", 0x00),
    level("size", "Size", "SIZE", 0x80),
    level("offset", "Offset", "OFST", 0x80),
];
pub(crate) const CHORD: [Entry; 3] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66),
    level("wave", "Wave", "WAVE", 0x40),
];
pub(crate) const CHIPTUNE: [Entry; 4] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66),
    centred("shape", "Shape", "SHPE"),
    DRIFT,
];
pub(crate) const FOLD: [Entry; 3] = [
    centred("shape", "Shape", "SHPE"),
    level("fold", "Fold", "FOLD", 0x00),
    level("slope", "Slope", "SLPE", 0x80),
];
pub(crate) const WAVES: [Entry; 3] = [
    level("bank", "Bank", "BANK", 0x80),
    level("wave", "Wave", "WAVE", 0x00),
    level("row", "Row", "ROW", 0x80),
];
pub(crate) const SIX_OP: [Entry; 4] = [
    Entry {
        key: "patch",
        name: "Patch",
        label: Some("PAT"),
        kind: Kind::Stepped {
            choices: &PATCH_NAMES,
            wraps: false,
        },
        // E.PIANO 1.
        default: 0x20,
        place: Place::Own,
    },
    centred("bright", "Bright", "BRT"),
    centred("time", "Time", "TIME"),
    ACCENT,
];
/// A string's or a bar's: Material starts where it's in tune.
pub(crate) const STRUCK: [Entry; 4] = [
    level("material", "Material", "MATL", 0x40),
    level("bright", "Bright", "BRT", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
pub(crate) const DUST: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x80),
    level("density", "Density", "DENS", 0x00),
    centred("filter", "Filter", "FILT"),
];
pub(crate) const NOISE: [Entry; 3] = [
    centred("type", "Type", "TYPE"),
    level("clock", "Clock", "CLCK", 0x00),
    level("ring", "Ring", "RING", 0x80),
];
pub(crate) const KICK: [Entry; 4] = [
    level("punch", "Punch", "PNCH", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
pub(crate) const SNARE: [Entry; 4] = [
    level("snappy", "Snappy", "SNAP", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
/// Plaits' speech: its three knobs, then its prosody and speed, the attenuverters it takes them
/// from, none at `80`. It starts on the first bank of words.
pub(crate) const SPEECH: [Entry; 5] = [
    level("model", "Model", "MODL", 0x80),
    level("species", "Species", "SPCS", 0x80),
    level("word", "Word", "WORD", 0x00),
    centred("prosody", "Prosody", "PROS"),
    centred("speed", "Speed", "SPED"),
];
pub(crate) const HAT: [Entry; 4] = [
    level("noise", "Noise", "NOIS", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];

/// Braids' models have its `Timbre` and `Color` first, named for what they do on each.
pub(crate) const CSAW: [Entry; 2] = [
    level("notch", "Notch", "NTCH", 0x40),
    level("depth", "Depth", "DPTH", 0x80),
];
pub(crate) const MORPH: [Entry; 2] = [
    level("shape", "Shape", "SHPE", 0x55),
    level("fuzz", "Fuzz", "FUZZ", 0x00),
];
pub(crate) const SAW_SQUARE: [Entry; 2] = [
    level("width", "Width", "WDTH", 0x00),
    level("mix", "Mix", "MIX", 0x80),
];
/// The waves `Sync` plays both of its oscillators with.
const SYNC_WAVES: [&str; 2] = ["Square", "Saw"];
pub(crate) const SYNC: [Entry; 3] = [
    Entry {
        key: "wave",
        name: "Wave",
        label: Some("WAVE"),
        kind: Kind::Stepped {
            choices: &SYNC_WAVES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    level("interval", "Interval", "INTV", 0x40),
    level("mix", "Mix", "MIX", 0xC0),
];
/// The waves `Triple` plays all three of its oscillators with.
const TRIPLE_WAVES: [&str; 4] = ["Saw", "Square", "Triangle", "Sine"];
/// Its second and third oscillators start a fifth and an octave up.
pub(crate) const TRIPLE: [Entry; 3] = [
    Entry {
        key: "wave",
        name: "Wave",
        label: Some("WAVE"),
        kind: Kind::Stepped {
            choices: &TRIPLE_WAVES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    level("second", "Second", "SCND", 0xA8),
    level("third", "Third", "THRD", 0xC0),
];
pub(crate) const RING: [Entry; 2] = [
    level("first", "First", "FRST", 0xA0),
    level("second", "Second", "SCND", 0xB0),
];
pub(crate) const SAW_COMB: [Entry; 2] = [
    level("tune", "Tune", "TUNE", 0xA0),
    Entry {
        default: 0xC0,
        ..centred("feedback", "Feedback", "FDBK")
    },
];
/// The filters `Z Filter` sounds like.
const Z_FILTER_TYPES: [&str; 4] = ["LP", "Peak", "BP", "HP"];
pub(crate) const Z_FILTER: [Entry; 3] = [
    Entry {
        key: "type",
        name: "Type",
        label: Some("TYPE"),
        kind: Kind::Stepped {
            choices: &Z_FILTER_TYPES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    level("cutoff", "Cutoff", "FREQ", 0x60),
    level("shape", "Shape", "SHPE", 0x40),
];
/// Feedback and chaotic FM start at 1:1, where `Ratio` holds from `6C` to `6F`.
pub(crate) const FEEDBACK_FM: [Entry; 2] = [
    level("amount", "Amount", "AMNT", 0x60),
    level("ratio", "Ratio", "RAT", 0x6E),
];
/// `VOSIM`'s sines sit at pitches of their own, as MIDI notes from `00` to `FF`.
pub(crate) const VOSIM: [Entry; 2] = [
    level("formant", "Formant", "FRMT", 0xA0),
    level("second", "Second", "SCND", 0xC0),
];
pub(crate) const FOF: [Entry; 2] = [
    level("vowel", "Vowel", "VOWL", 0x00),
    level("voice", "Voice", "VOIC", 0x40),
];
pub(crate) const BOWED: [Entry; 2] = [
    level("pressure", "Pressure", "PRES", 0x80),
    level("position", "Position", "POSN", 0x40),
];
/// A reed speaks most readily with little noise in its breath.
pub(crate) const BLOWN: [Entry; 2] = [
    level("breath", "Breath", "BRTH", 0xC0),
    level("tone", "Tone", "TONE", 0x40),
];
pub(crate) const FLUTED: [Entry; 2] = [
    level("breath", "Breath", "BRTH", 0x80),
    level("jet", "Jet", "JET", 0x80),
];
/// The struck ones ring on without dying away from `FA` up.
pub(crate) const STRUCK_BELL: [Entry; 2] = [
    level("decay", "Decay", "DCAY", 0xA0),
    level("detune", "Detune", "DTUN", 0x20),
];
pub(crate) const STRUCK_DRUM: [Entry; 2] = [
    level("decay", "Decay", "DCAY", 0x80),
    level("tone", "Tone", "TONE", 0x40),
];
pub(crate) const WAVE_MAP: [Entry; 2] = [
    level("across", "Across", "ACRS", 0x40),
    level("down", "Down", "DOWN", 0x40),
];
pub(crate) const WAVE_LINE: [Entry; 2] = [
    level("scan", "Scan", "SCAN", 0x40),
    level("steps", "Steps", "STEP", 0x60),
];
pub(crate) const PARAPHONIC: [Entry; 2] = [
    level("wave", "Wave", "WAVE", 0x40),
    level("chord", "Chord", "CHRD", 0x88),
];
pub(crate) const TWIN_PEAKS: [Entry; 2] = [
    level("sharpness", "Sharpness", "SHRP", 0xA0),
    level("spacing", "Spacing", "SPAC", 0xA0),
];
pub(crate) const CLOCKED: [Entry; 2] = [
    level("loop", "Loop", "LOOP", 0x00),
    level("steps", "Steps", "STEP", 0xFF),
];
pub(crate) const GRAIN_CLOUD: [Entry; 2] = [
    level("length", "Length", "LENG", 0x80),
    level("spread", "Spread", "SPRD", 0x40),
];
pub(crate) const MODEM: [Entry; 2] = [
    level("baud", "Baud", "BAUD", 0xC0),
    level("data", "Data", "DATA", 0x80),
];
pub(crate) const TOY: [Entry; 2] = [
    level("clock", "Clock", "CLCK", 0xC0),
    level("bend", "Bend", "BEND", 0x00),
];

/// `4-op`'s algorithms, by which operators bend which: `4>3>2>1` is a stack heard from operator
/// 1, and those with nothing after them are heard together. They're TetraOP's ten layouts, with
/// its A as operator 1 and its D as 4, the last all four heard.
pub const FOUR_OP_ALGORITHMS: [&str; 10] = [
    "4>3>2>1", "3>1 4>2", "4>2 1 3", "4>1,2,3", "2,3,4>1", "4>3>2 1", "4>3>1,2", "4>2,3>1",
    "3,4>2>1", "1 2 3 4",
];
/// The ratios an operator's `Ratio` steps through, as the Digitone's do.
const FOUR_OP_RATIOS: [&str; 20] = [
    "1/4", "1/2", "3/4", "1", "3/2", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
    "13", "14", "15", "16",
];
/// An operator's waves, the TX81Z's eight.
const FOUR_OP_WAVES: [&str; 8] = [
    "Sine", "Sine²", "Half", "Half²", "Twice", "Twice²", "Bumps", "Bumps²",
];
/// What bends an operator's cycle: TetraOP's phase distortions, but its `Bend`.
const FOUR_OP_WARPS: [&str; 8] = [
    "Off", "Skew", "Bias", "Pulse", "Sync", "Fold", "Steps", "Formant",
];

/// A choice from a list, starting at `default`.
const fn listed(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    choices: &'static [&'static str],
    default: u8,
) -> Entry {
    Entry {
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

/// One of `4-op`'s operators, `$n` its number: its pitch, its level, its sound and its envelope,
/// starting at a ratio and level of its own.
macro_rules! operator {
    ($n:literal, $ratio:expr, $level:expr) => {
        [
            listed(
                concat!("ratio", $n),
                concat!("Ratio ", $n),
                concat!("RAT", $n),
                &FOUR_OP_RATIOS,
                $ratio,
            ),
            centred(
                concat!("fine", $n),
                concat!("Fine ", $n),
                concat!("FIN", $n),
            ),
            level(
                concat!("level", $n),
                concat!("Level ", $n),
                concat!("LVL", $n),
                $level,
            ),
            listed(
                concat!("wave", $n),
                concat!("Wave ", $n),
                concat!("WAV", $n),
                &FOUR_OP_WAVES,
                0,
            ),
            listed(
                concat!("warp", $n),
                concat!("Warp ", $n),
                concat!("WRP", $n),
                &FOUR_OP_WARPS,
                0,
            ),
            centred(
                concat!("amount", $n),
                concat!("Amount ", $n),
                concat!("AMT", $n),
            ),
            level(
                concat!("attack", $n),
                concat!("Attack ", $n),
                concat!("ATK", $n),
                0x00,
            ),
            level(
                concat!("decay", $n),
                concat!("Decay ", $n),
                concat!("DEC", $n),
                0x80,
            ),
            level(
                concat!("sustain", $n),
                concat!("Sustain ", $n),
                concat!("SUS", $n),
                0xFF,
            ),
            level(
                concat!("release", $n),
                concat!("Release ", $n),
                concat!("REL", $n),
                0x60,
            ),
        ]
    };
}

/// `4-op`'s: its algorithm, feedback, ring modulation and beating, then each operator's. It
/// starts as a stack with only operator 2 bending 1, a little, for a plain, warm tone.
pub(crate) const FOUR_OP: [Entry; MAX_MACHINE_SETTINGS] = {
    let shared = [
        listed("algorithm", "Algorithm", "ALGO", &FOUR_OP_ALGORITHMS, 0),
        centred("feedback", "Feedback", "FDBK"),
        level("ring", "Ring", "RING", 0x00),
        level("beat", "Beat", "BEAT", 0x00),
    ];
    // Ratios 1, 1, 2 and 1; operator 1 heard in full, 2 bending it a little, 3 and 4 off.
    let operators = [
        operator!("1", 3, 0xFF),
        operator!("2", 3, 0x60),
        operator!("3", 5, 0x00),
        operator!("4", 3, 0x00),
    ];
    let mut all = [shared[0]; MAX_MACHINE_SETTINGS];
    let mut at = 0;
    while at < FOUR_OP_SHARED {
        all[at] = shared[at];
        at += 1;
    }
    let mut operator = 0;
    while operator < 4 {
        let mut setting = 0;
        while setting < FOUR_OP_EACH {
            all[FOUR_OP_SHARED + operator * FOUR_OP_EACH + setting] = operators[operator][setting];
            setting += 1;
        }
        operator += 1;
    }
    all
};

/// A voice's settings, in the order it takes them.
pub const fn voice_entries(voice: SynthVoice) -> &'static [Entry] {
    voice.spec().params
}

/// The place of the setting stored as `key` among a voice's, which the engine's voices read their
/// settings by. It's for their constants, so a key the voice doesn't have doesn't build.
pub const fn voice_place(voice: SynthVoice, key: &str) -> usize {
    let entries = voice_entries(voice);
    let mut at = 0;
    while at < entries.len() {
        if same_key(entries[at].key, key) {
            return at;
        }
        at += 1;
    }
    panic!("a voice has no setting of that key")
}

/// The name a voice's settings are stored under.
pub const fn voice_key(voice: SynthVoice) -> &'static str {
    voice.spec().key
}
