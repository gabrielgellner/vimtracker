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

const ACCENT: Entry = level("accent", "Accent", "ACC", DEFAULT_ACCENT)
    .means("how hard each note is struck, apart from how loud it is");
/// How far each note of an analog-style voice strays, from none at `00`: its oscillators' tuning,
/// a slow wander in pitch, its cutoff, level and envelope times, and, but for `Acid`'s, which
/// always runs free, where its oscillators start.
const DRIFT: Entry =
    level("drift", "Drift", "DRFT", 0x00).means("how far each note strays, as on an analog synth");

pub(crate) const VA: [Entry; 4] = [
    centred("detune", "Detune", "DTUN")
        .means("the saw's tuning against the note, up to two octaves either way"),
    level("square", "Square", "SQR", 0x00)
        .means("how much square: none at 00, then a pulse, a square and hard sync"),
    level("saw", "Saw", "SAW", 0x80)
        .means("the saw's shape: a triangle at 00, a plain saw at 80, then a notch"),
    DRIFT,
];
pub(crate) const FM: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80)
        .means("the second wave's pitch against the note's, in steps that catch"),
    level("index", "Index", "INDX", 0x00)
        .means("how hard the second wave bends the first: brighter going up"),
    centred("feedback", "Feedback", "FDBK")
        .means("the wave bending itself: gritty above 80, unstable pitch below"),
];
pub(crate) const VCF: [Entry; 4] = [
    centred("resonance", "Resonance", "RESO")
        .means("how much the filter rings: steep below 80, gentle above"),
    level("frequency", "Frequency", "FREQ", 0x80)
        .means("the filter's cutoff, from two octaves under the note to far above"),
    level("wave", "Wave", "WAVE", 0x40)
        .means("the wave: saw and sub, saw, square, pulse, then square and sub"),
    DRIFT,
];
pub(crate) const PD: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80)
        .means("the bending wave's pitch against the note's, an octave at 80"),
    level("amount", "Amount", "AMNT", 0x80)
        .means("how far the sine is bent: pure at 00, brighter going up"),
    level("skew", "Skew", "SKEW", 0x40)
        .means("which way the sine is bent, for an edge like a saw's or a pulse's"),
];
pub(crate) const FORMANT: [Entry; 3] = [
    Entry {
        default: 0x99,
        ..centred("interval", "Interval", "INTV")
            .means("the second formant against the first, up to two octaves")
    },
    level("formant", "Formant", "FRMT", 0x80)
        .means("where the formants sit, from low and muffled to bright"),
    level("width", "Width", "WDTH", 0x80).means("how wide and soft the formants are"),
];
pub(crate) const ADDITIVE: [Entry; 3] = [
    level("bumps", "Bumps", "BMPS", 0x00)
        .means("how many bumps of harmonics: one at 00, more going up"),
    level("peak", "Peak", "PEAK", 0x4D).means("which harmonic is loudest, from the fundamental up"),
    level("shape", "Shape", "SHPE", 0x80)
        .means("the bumps' shape, from wide and flat to narrow and peaked"),
];
pub(crate) const SWARM: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x66)
        .means("how far the saws' pitches spread, up to four octaves"),
    level("density", "Density", "DENS", 0x80)
        .means("how often grains start: sparse at 00, a cloud at FF"),
    level("overlap", "Overlap", "OVLP", 0x99)
        .means("how long the grains are, to held tones that glide near FF"),
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
        meaning: "every saw at the note, or every other a fifth up or an octave down",
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
    level("detune", "Detune", "DTUN", 0x60)
        .means("how far apart the saws are tuned, up to a whole tone either way"),
    centred("warp", "Warp", "WARP")
        .means("how the saws' tunings spread: even at 80, gathered above, pushed out below"),
    level("mix", "Mix", "MIX", 0xFF).means("how loud the outer saws are, or the moved ones"),
    Entry {
        meaning: "how many saws play",
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
    level("shape", "Shape", "SHPE", 0x00).means("saws at 00 to squares at FF, blending between"),
    level("width", "Width", "WDTH", 0x80)
        .means("how far the saws spread across the sides: mono at 00"),
    level("phase", "Phase", "PHAS", 0xFF)
        .means("where each saw starts a note: the top at 00, anywhere at FF"),
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
        meaning: "the saw, or the square a 303 makes from it",
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
    in_units("cutoff", "Cutoff", "FREQ", 0x80, Units::AcidCutoff)
        .means("where the diode filter sits, moving most near the top"),
    level("resonance", "Resonance", "RESO", 0x80)
        .means("how much the filter rings, thinning the bass as it rises"),
    level("envmod", "Env mod", "EMOD", 0x80)
        .means("how far each note's envelope sweeps the cutoff"),
    in_units("decay", "Decay", "DCAY", 0x80, Units::AcidDecay)
        .means("how long the envelope's sweep takes to fall"),
    level("accent", "Accent", "ACC", 0x80)
        .means("how much louder and brighter an accented note is"),
    in_units("slide", "Slide", "SLID", 0x00, Units::Slide)
        .means("how long a slide takes, unless the track's Glide is on"),
    in_units("drive", "Drive", "DRIV", 0x00, Units::AcidDrive)
        .means("how hard the saw drives the filter, for grit"),
    DRIFT,
];
/// The chords Chord and Chiptune play, in the order Plaits lists them.
const CHORD_NAMES: [&str; 11] = [
    "Octave", "5", "sus4", "m", "m7", "m9", "m11", "6/9", "M9", "M7", "M",
];
/// A chord, chosen from the list. It starts major.
const CHORD_TYPE: Entry = Entry {
    meaning: "which chord each note plays",
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
    level("terrain", "Terrain", "TERR", 0x00)
        .means("which surface the circle traces, blending from one to the next"),
    level("size", "Size", "SIZE", 0x80)
        .means("how wide the circle is: small and pure at 00, rich at FF"),
    level("offset", "Offset", "OFST", 0x80).means("where the circle sits on the surface"),
];
pub(crate) const CHORD: [Entry; 3] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66)
        .means("moves the chord's lowest note up an octave at a time"),
    level("wave", "Wave", "WAVE", 0x40)
        .means("organ and string sounds below 80, then waves above it"),
];
pub(crate) const CHIPTUNE: [Entry; 4] = [
    CHORD_TYPE,
    level("inversion", "Inversion", "INV", 0x66)
        .means("moves the chord's lowest note up an octave at a time"),
    centred("shape", "Shape", "SHPE")
        .means("a thin pulse below 80, a square at 80, and hard sync above"),
    DRIFT,
];
pub(crate) const FOLD: [Entry; 3] = [
    centred("shape", "Shape", "SHPE").means("the waveshaper before the fold, plain at 80"),
    level("fold", "Fold", "FOLD", 0x00)
        .means("how far the wave folds back on itself: smooth at 00, rich going up"),
    level("slope", "Slope", "SLPE", 0x80).means("the wave's tilt, from a triangle at 00 to a ramp"),
];
pub(crate) const WAVES: [Entry; 3] = [
    level("bank", "Bank", "BANK", 0x80)
        .means("which bank: blending up and down below 80, stepping above"),
    level("wave", "Wave", "WAVE", 0x00).means("which wave along a row of the grid"),
    level("row", "Row", "ROW", 0x80).means("which row of the grid"),
];
pub(crate) const SIX_OP: [Entry; 4] = [
    Entry {
        meaning: "which of the 96 DX7 patches plays, from the next note",
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
    centred("bright", "Bright", "BRT")
        .means("how hard the modulators bend, as the patch has it at 80"),
    centred("time", "Time", "TIME")
        .means("the patch's attack and decay: faster below 80, slower above"),
    ACCENT,
];
/// A string's or a bar's: Material starts where it's in tune.
pub(crate) const STRUCK: [Entry; 4] = [
    level("material", "Material", "MATL", 0x40)
        .means("what it's made of: plain at 40, buzzier below, more metal above"),
    level("bright", "Bright", "BRT", 0x00).means("how bright the strike is"),
    level("decay", "Decay", "DCAY", 0x80).means("how long it rings"),
    ACCENT,
];
pub(crate) const DUST: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x80)
        .means("how far each click's pitch strays from the note"),
    level("density", "Density", "DENS", 0x00).means("how many clicks: a few at 00, a hiss at FF"),
    centred("filter", "Filter", "FILT").means("echoing more below 80, ringing more above"),
];
pub(crate) const NOISE: [Entry; 3] = [
    centred("type", "Type", "TYPE")
        .means("the filter: low-pass at 00, band-pass at 80, high-pass at FF"),
    level("clock", "Clock", "CLCK", 0x00)
        .means("how fast the noise changes: grainy steps low, a hiss high"),
    level("ring", "Ring", "RING", 0x80).means("how much the filter rings at the note"),
];
pub(crate) const KICK: [Entry; 4] = [
    level("punch", "Punch", "PNCH", 0x80)
        .means("the attack's pitch sweep, then its self-modulation and drive"),
    level("tone", "Tone", "TONE", 0x00).means("how bright it is"),
    level("decay", "Decay", "DCAY", 0x80).means("how long it rings"),
    ACCENT,
];
pub(crate) const SNARE: [Entry; 4] = [
    level("snappy", "Snappy", "SNAP", 0x80).means("the mix of drum and noise: all drum at 00"),
    level("tone", "Tone", "TONE", 0x00).means("the drum's overtones, more of them near the top"),
    level("decay", "Decay", "DCAY", 0x80).means("how long it rings"),
    ACCENT,
];
/// Plaits' speech: its three knobs, then its prosody and speed, the attenuverters it takes them
/// from, none at `80`. It starts on the first bank of words.
pub(crate) const SPEECH: [Entry; 5] = [
    level("model", "Model", "MODL", 0x80)
        .means("formants, a SAM-like voice, the chips' vowels, then words from 70"),
    level("species", "Species", "SPCS", 0x80)
        .means("the voice's size: deep and slow below 80, small and quick above"),
    level("word", "Word", "WORD", 0x00).means("which vowel, or which word of the bank"),
    centred("prosody", "Prosody", "PROS")
        .means("how much of a word's own pitch it keeps: none at 80"),
    centred("speed", "Speed", "SPED")
        .means("how fast a word is said: as recorded at 80, faster above"),
];
pub(crate) const HAT: [Entry; 4] = [
    level("noise", "Noise", "NOIS", 0x80).means("how much plain noise replaces the metal"),
    level("tone", "Tone", "TONE", 0x00).means("how bright it is"),
    level("decay", "Decay", "DCAY", 0x80).means("how long it rings"),
    ACCENT,
];

/// Braids' models have its `Timbre` and `Color` first, named for what they do on each.
pub(crate) const CSAW: [Entry; 2] = [
    level("notch", "Notch", "NTCH", 0x40)
        .means("how long each cycle holds its step: a plain saw at 00"),
    level("depth", "Depth", "DPTH", 0x80)
        .means("how low the step sits: deep at 00, near the saw's middle at FF"),
];
pub(crate) const MORPH: [Entry; 2] = [
    level("shape", "Shape", "SHPE", 0x55)
        .means("triangle at 00, saw at 55, square at AA, a thin pulse at FF"),
    level("fuzz", "Fuzz", "FUZZ", 0x00)
        .means("closes the filter towards the note and overdrives it"),
];
pub(crate) const SAW_SQUARE: [Entry; 2] = [
    level("width", "Width", "WDTH", 0x00)
        .means("how far behind the second saw is, and how narrow the square"),
    level("mix", "Mix", "MIX", 0x80).means("all saw at 00, all square at FF"),
];
/// The waves `Sync` plays both of its oscillators with.
const SYNC_WAVES: [&str; 2] = ["Square", "Saw"];
pub(crate) const SYNC: [Entry; 3] = [
    Entry {
        meaning: "the wave both oscillators play",
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
    level("interval", "Interval", "INTV", 0x40)
        .means("how far above the note the synced wave is, up to 64 semitones"),
    level("mix", "Mix", "MIX", 0xC0).means("the first wave at 00, the synced one at FF"),
];
/// The waves `Triple` plays all three of its oscillators with.
const TRIPLE_WAVES: [&str; 4] = ["Saw", "Square", "Triangle", "Sine"];
/// Its second and third oscillators start a fifth and an octave up.
pub(crate) const TRIPLE: [Entry; 3] = [
    Entry {
        meaning: "the wave all three oscillators play",
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
    level("second", "Second", "SCND", 0xA8)
        .means("the second oscillator's pitch, two octaves either way of the note"),
    level("third", "Third", "THRD", 0xC0)
        .means("the third oscillator's pitch, two octaves either way of the note"),
];
pub(crate) const RING: [Entry; 2] = [
    level("first", "First", "FRST", 0xA0)
        .means("the first modulator's pitch, up to 32 semitones either way"),
    level("second", "Second", "SCND", 0xB0)
        .means("the second modulator's pitch, up to 32 semitones either way"),
];
pub(crate) const SAW_COMB: [Entry; 2] = [
    level("tune", "Tune", "TUNE", 0xA0)
        .means("the comb's pitch, five octaves either way of the note"),
    Entry {
        default: 0xC0,
        ..centred("feedback", "Feedback", "FDBK")
            .means("how much it rings: hollow below 80, none at 80, ringing above")
    },
];
/// The filters `Z Filter` sounds like.
const Z_FILTER_TYPES: [&str; 4] = ["LP", "Peak", "BP", "HP"];
pub(crate) const Z_FILTER: [Entry; 3] = [
    Entry {
        meaning: "the filter it sounds like",
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
    level("cutoff", "Cutoff", "FREQ", 0x60)
        .means("where it rings, from just under the note to ten octaves over"),
    level("shape", "Shape", "SHPE", 0x40)
        .means("the window: a saw's at 00, the square at 80, a triangle's at FF"),
];
/// Feedback and chaotic FM start at 1:1, where `Ratio` holds from `6C` to `6F`.
pub(crate) const FEEDBACK_FM: [Entry; 2] = [
    level("amount", "Amount", "AMNT", 0x60)
        .means("how hard it bends: a pure sine at 00, harsher going up"),
    level("ratio", "Ratio", "RAT", 0x6E).means("the second sine's pitch, catching on ratios"),
];
/// `VOSIM`'s sines sit at pitches of their own, as MIDI notes from `00` to `FF`.
pub(crate) const VOSIM: [Entry; 2] = [
    level("formant", "Formant", "FRMT", 0xA0).means("the first sine's pitch, whatever the note"),
    level("second", "Second", "SCND", 0xC0)
        .means("the second, quieter sine's pitch, whatever the note"),
];
pub(crate) const FOF: [Entry; 2] = [
    level("vowel", "Vowel", "VOWL", 0x00)
        .means("the vowel, from a at 00 through e, i and o to u at FF"),
    level("voice", "Voice", "VOIC", 0x40).means("the singer, from a bass at 00 to a soprano at FF"),
];
pub(crate) const BOWED: [Entry; 2] = [
    level("pressure", "Pressure", "PRES", 0x80)
        .means("how hard the bow grips: airy at 00, scratchy going up"),
    level("position", "Position", "POSN", 0x40)
        .means("where the bow sits, from near the bridge towards the middle"),
];
/// A reed speaks most readily with little noise in its breath.
pub(crate) const BLOWN: [Entry; 2] = [
    level("breath", "Breath", "BRTH", 0xC0).means("the breath's noise: breathy at 00, pure at FF"),
    level("tone", "Tone", "TONE", 0x40).means("the bore's tone: dark at 00, brighter going up"),
];
pub(crate) const FLUTED: [Entry; 2] = [
    level("breath", "Breath", "BRTH", 0x80).means("the breath's noise: breathy at 00, pure at FF"),
    level("jet", "Jet", "JET", 0x80)
        .means("how long the jet is against the bore, which overblows it"),
];
/// The struck ones ring on without dying away from `FA` up.
pub(crate) const STRUCK_BELL: [Entry; 2] = [
    level("decay", "Decay", "DCAY", 0xA0).means("how long it rings, on without dying away near FF"),
    level("detune", "Detune", "DTUN", 0x20)
        .means("how far its pairs of partials are pulled apart, to beat"),
];
pub(crate) const STRUCK_DRUM: [Entry; 2] = [
    level("decay", "Decay", "DCAY", 0x80).means("how long it rings, on without dying away near FF"),
    level("tone", "Tone", "TONE", 0x40)
        .means("from a dull thud through brighter partials to a noisy skin"),
];
pub(crate) const WAVE_MAP: [Entry; 2] = [
    level("across", "Across", "ACRS", 0x40).means("where on the map of waves, across it"),
    level("down", "Down", "DOWN", 0x40).means("where on the map of waves, down it"),
];
pub(crate) const WAVE_LINE: [Entry; 2] = [
    level("scan", "Scan", "SCAN", 0x40).means("where along the line of waves"),
    level("steps", "Steps", "STEP", 0x60)
        .means("stepped jumps low, smooth sweeps in the middle, coarse steps at FF"),
];
pub(crate) const PARAPHONIC: [Entry; 2] = [
    level("wave", "Wave", "WAVE", 0x40).means("which wave along the line, blending between them"),
    level("chord", "Chord", "CHRD", 0x88)
        .means("a detuned unison at 00, then fifths, minor, major and octaves"),
];
pub(crate) const TWIN_PEAKS: [Entry; 2] = [
    level("sharpness", "Sharpness", "SHRP", 0xA0)
        .means("how narrow the peaks are: wide at 00, whistling at FF"),
    level("spacing", "Spacing", "SPAC", 0xA0)
        .means("the second peak: five octaves under at 00, at the note at 80"),
];
pub(crate) const CLOCKED: [Entry; 2] = [
    level("loop", "Loop", "LOOP", 0x00)
        .means("how soon the noise repeats: hardly ever at 00, every few ticks at FF"),
    level("steps", "Steps", "STEP", 0xFF)
        .means("how many levels the noise takes: two at 00, up to 32 at FF"),
];
pub(crate) const GRAIN_CLOUD: [Entry; 2] = [
    level("length", "Length", "LENG", 0x80)
        .means("how long each grain is: long at 00, short and dense at FF"),
    level("spread", "Spread", "SPRD", 0x40).means("how far each grain strays from the note"),
];
pub(crate) const MODEM: [Entry; 2] = [
    level("baud", "Baud", "BAUD", 0xC0)
        .means("how fast the symbols go: slow at 00, an octave under the note at FF"),
    level("data", "Data", "DATA", 0x80).means("the byte it sends over and over"),
];
pub(crate) const TOY: [Entry; 2] = [
    level("clock", "Clock", "CLCK", 0xC0)
        .means("how fast it's worked out: gritty at 00, cleaner at FF"),
    level("bend", "Bend", "BEND", 0x00)
        .means("which bits are bent: plain at 00, more broken going up"),
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
            )
            .means(concat!("operator ", $n, "'s pitch against the note's")),
            centred(
                concat!("fine", $n),
                concat!("Fine ", $n),
                concat!("FIN", $n),
            )
            .means(concat!(
                "moves operator ",
                $n,
                "'s ratio up to six semitones either way"
            )),
            level(
                concat!("level", $n),
                concat!("Level ", $n),
                concat!("LVL", $n),
                $level,
            )
            .means(concat!(
                "how hard operator ",
                $n,
                " bends, or how loud it is if heard"
            )),
            listed(
                concat!("wave", $n),
                concat!("Wave ", $n),
                concat!("WAV", $n),
                &FOUR_OP_WAVES,
                0,
            )
            .means(concat!("the wave operator ", $n, " plays")),
            listed(
                concat!("warp", $n),
                concat!("Warp ", $n),
                concat!("WRP", $n),
                &FOUR_OP_WARPS,
                0,
            )
            .means(concat!("what bends operator ", $n, "'s cycle")),
            centred(
                concat!("amount", $n),
                concat!("Amount ", $n),
                concat!("AMT", $n),
            )
            .means(concat!(
                "how far its warp bends operator ",
                $n,
                "'s cycle, either way of 80"
            )),
            level(
                concat!("attack", $n),
                concat!("Attack ", $n),
                concat!("ATK", $n),
                0x00,
            )
            .means(concat!("how long operator ", $n, "'s level takes to rise")),
            level(
                concat!("decay", $n),
                concat!("Decay ", $n),
                concat!("DEC", $n),
                0x80,
            )
            .means(concat!(
                "how long operator ",
                $n,
                "'s level takes to fall to its sustain"
            )),
            level(
                concat!("sustain", $n),
                concat!("Sustain ", $n),
                concat!("SUS", $n),
                0xFF,
            )
            .means(concat!(
                "where operator ",
                $n,
                "'s level holds while the note does"
            )),
            level(
                concat!("release", $n),
                concat!("Release ", $n),
                concat!("REL", $n),
                0x60,
            )
            .means(concat!(
                "how long operator ",
                $n,
                "'s level takes to fall once the note ends"
            )),
        ]
    };
}

/// `4-op`'s: its algorithm, feedback, ring modulation and beating, then each operator's. It
/// starts as a stack with only operator 2 bending 1, a little, for a plain, warm tone.
pub(crate) const FOUR_OP: [Entry; MAX_MACHINE_SETTINGS] = {
    let shared = [
        listed("algorithm", "Algorithm", "ALGO", &FOUR_OP_ALGORITHMS, 0)
            .means("which operators bend which, and which are heard"),
        centred("feedback", "Feedback", "FDBK")
            .means("operator 4 bending itself: brighter above 80, rougher below"),
        level("ring", "Ring", "RING", 0x00)
            .means("how much each bent operator is multiplied by what bends it"),
        level("beat", "Beat", "BEAT", 0x00)
            .means("how far the modulators are pulled apart, one up and the next down"),
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
