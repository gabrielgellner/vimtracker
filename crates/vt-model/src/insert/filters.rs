//! The effects that filter a sound: an EQ, a low-pass gate, a resonator, a comb and a vowel
//! filter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::pitch::{MOST_SHIFT, PITCH_PARAMS};
use super::{
    AMOUNT_PARAM, Columns, GAIN_PARAM, Kind, MIX_PARAM, Param, Place, SOURCE_NAMES, Units,
    hex_param, source_track,
};

/// An EQ's, in the order the page shows them.
pub(crate) const EQ_PARAMS: [Param; 8] = [
    hex_param(
        "Low freq",
        "lowfreq",
        "LFRQ",
        Equaliser::DEFAULT.low_frequency,
    )
    .units(Units::Hertz),
    hex_param("Low", "low", "LOW", Equaliser::DEFAULT.low)
        .units(Units::Boost)
        .centred(),
    hex_param(
        "Mid freq",
        "midfreq",
        "MFRQ",
        Equaliser::DEFAULT.mid_frequency,
    )
    .units(Units::Hertz),
    hex_param("Mid", "mid", "MID", Equaliser::DEFAULT.mid)
        .units(Units::Boost)
        .centred(),
    hex_param(
        "High freq",
        "highfreq",
        "HFRQ",
        Equaliser::DEFAULT.high_frequency,
    )
    .units(Units::Hertz),
    hex_param("High", "high", "HIGH", Equaliser::DEFAULT.high)
        .units(Units::Boost)
        .centred(),
    // After the bands, so the page shows each band's frequency over its boost.
    hex_param("Mid Q", "midq", "MIDQ", Equaliser::DEFAULT.mid_q).units(Units::Q),
    GAIN_PARAM,
];

/// Where its settings sit on the FX page. Its bands are its columns, then what comes out.
pub(super) const EQ_GRID: Columns = &[
    &["lowfreq", "low"],
    &["midfreq", "mid", "midq"],
    &["highfreq", "high"],
    &["gain"],
];

/// An EQ's settings. Each band's boost or cut is centred on `80`, where it leaves the sound as it
/// is. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Equaliser {
    /// Where each band sits, from 20 Hz at `00` to 20 kHz at `FF`, evenly in pitch, as the cutoff.
    pub low_frequency: u8,
    pub low: u8,
    pub mid_frequency: u8,
    pub mid: u8,
    /// How narrow the middle band is.
    pub mid_q: u8,
    pub high_frequency: u8,
    pub high: u8,
    pub gain: u8,
}

impl Default for Equaliser {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Equaliser {
    /// As it starts: flat, with the shelves at about 100 Hz and 8 kHz, and the middle at 1 kHz.
    pub const DEFAULT: Self = Self {
        low_frequency: 0x3B,
        low: 0x80,
        mid_frequency: 0x90,
        mid: 0x80,
        mid_q: 0x40,
        high_frequency: 0xDD,
        high: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        low_frequency,
        low,
        mid_frequency,
        mid,
        high_frequency,
        high,
        mid_q,
        gain,
    }
}

/// What a low-pass gate opens as what it hears gets louder, after Mutable Instruments' Streams.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateMode {
    /// Streams' vactrol: its level and its filter's cutoff together, opening quickly and closing
    /// slowly, as a low-pass gate does.
    #[default]
    Gate,
    /// Streams' follower on the cutoff alone, so it brightens as it gets louder: an envelope
    /// filter, or an auto-wah.
    Filter,
    /// Streams' follower on the level alone.
    Vca,
}

impl GateMode {
    pub const ALL: [Self; 3] = [Self::Gate, Self::Filter, Self::Vca];
}

const GATE_MODE_NAMES: [&str; 3] = ["Gate", "Filter", "VCA"];

/// A low-pass gate's, in the order the page shows them, as columns of two: what it opens and how
/// far, where it rests and its resonance, then how slowly it responds and what it hears.
pub(crate) const LPG_PARAMS: [Param; 8] = [
    Param {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &GATE_MODE_NAMES,
            wraps: true,
        },
        default: 0,
        place: Place::Type,
    },
    Param {
        default: LowPassGate::DEFAULT.amount,
        ..AMOUNT_PARAM
    },
    hex_param("Base", "base", "BASE", LowPassGate::DEFAULT.base),
    hex_param("Res", "res", "RESO", LowPassGate::DEFAULT.resonance),
    hex_param(
        "Response",
        "response",
        "RESP",
        LowPassGate::DEFAULT.response,
    ),
    Param {
        name: "Source",
        key: "source",
        // What it hears is part of how it's set up, so steps don't change it.
        label: None,
        kind: Kind::Stepped {
            choices: &SOURCE_NAMES,
            wraps: false,
        },
        default: LowPassGate::DEFAULT.source,
        place: Place::Own,
    },
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A low-pass gate's settings, after Mutable Instruments' Streams. One a file doesn't have starts
/// as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LowPassGate {
    pub mode: GateMode,
    /// How far what it hears opens it, from where it rests to all the way.
    pub amount: u8,
    /// Where it rests when it's closed: its filter's cutoff, or its level as a VCA.
    pub base: u8,
    /// Its filter's resonance.
    pub resonance: u8,
    /// Streams' shape knob: how slowly it opens and closes. As a gate, the vactrol's decay grows
    /// to its longest at `80`, then its attack slows as well. As a follower, both lengthen.
    pub response: u8,
    /// What it hears: `0` for its own input, a track's number, or a return after the tracks.
    pub source: u8,
    /// From `00`, all dry, to `FF`, all through the gate.
    pub mix: u8,
    pub gain: u8,
}

impl Default for LowPassGate {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl LowPassGate {
    /// As it starts: a gate on its own sound, opening most of the way. Let go, its cutoff falls at
    /// once while its level holds a moment, then fades out over about a second, as a vactrol's
    /// does.
    pub const DEFAULT: Self = Self {
        mode: GateMode::Gate,
        amount: 0xC0,
        base: 0x00,
        resonance: 0x00,
        response: 0x10,
        source: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// The track it hears, by its place among the tracks, if not its own input.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    effect_settings! {
        mode: GateMode,
        amount,
        base,
        resonance,
        response,
        source,
        mix,
        gain,
    }
}

/// A resonator's, in the order the page shows them, as columns of two: its pitch and how its
/// partials spread, how bright and how long it rings, then where it's struck and how wide it is.
pub(crate) const RESO_PARAMS: [Param; 8] = [
    Param {
        name: "Tune",
        key: "tune",
        label: Some("TUNE"),
        default: Resonator::DEFAULT.tune,
        ..PITCH_PARAMS[0]
    },
    hex_param("Material", "material", "MATL", Resonator::DEFAULT.material),
    hex_param("Bright", "bright", "BRT", Resonator::DEFAULT.bright),
    hex_param("Decay", "decay", "DCAY", Resonator::DEFAULT.decay),
    hex_param("Position", "position", "POS", Resonator::DEFAULT.position),
    hex_param("Width", "width", "WDTH", Resonator::DEFAULT.width),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A resonator's settings: a bank of modes tuned to the track's note, after Mutable Instruments'
/// Plaits, Rings and Elements, which the slot's sound strikes. One a file doesn't have starts as
/// new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resonator {
    /// How far its pitch is from the track's note, as a place in the list from two octaves down to
    /// two up.
    pub tune: u8,
    /// Plaits' structure: its partials bunched low, in tune from `40`, then spread wider, as a
    /// bell's are.
    pub material: u8,
    /// How long its high partials ring against its lowest.
    pub bright: u8,
    /// How long it rings, Plaits' damping.
    pub decay: u8,
    /// Where it's struck and heard, from its end at `00` to its middle at `FF`.
    pub position: u8,
    /// How far a second pickup, wandering slowly along it, spreads it across the sides, as
    /// Elements' does.
    pub width: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Resonator {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Resonator {
    /// As it starts: in tune with the note, its partials whole multiples of it, ringing for about
    /// a third of a second at middle C, struck near its end as Plaits strikes its own, a little
    /// wide, and all wet.
    pub const DEFAULT: Self = Self {
        tune: MOST_SHIFT,
        material: 0x40,
        bright: 0x80,
        decay: 0x80,
        position: 0x08,
        width: 0x80,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        tune,
        material,
        bright,
        decay,
        position,
        width,
        mix,
        gain,
    }
}

/// What a comb does with what it delays, after Vital's comb filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CombMode {
    /// Feedback alone: the sound and its echoes, each a cycle of the note apart, so it rings.
    #[default]
    Comb,
    /// The sound with a delayed copy, which feedback sharpens: notches between the note's
    /// harmonics, and peaks on them.
    Flange,
}

impl CombMode {
    pub const ALL: [Self; 2] = [Self::Comb, Self::Flange];
}

const COMB_MODE_NAMES: [&str; 2] = ["Comb", "Flange"];

/// A comb's, in the order the page shows them, as columns of two: what it is and its pitch, how
/// long it rings and how bright, then how wide a band it keeps and the mix, then its gain.
pub(crate) const COMB_PARAMS: [Param; 7] = [
    Param {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &COMB_MODE_NAMES,
            wraps: true,
        },
        default: 0,
        place: Place::Type,
    },
    Param {
        name: "Tune",
        key: "tune",
        label: Some("TUNE"),
        default: Comb::DEFAULT.tune,
        ..PITCH_PARAMS[0]
    },
    hex_param("Feedback", "feedback", "FDBK", Comb::DEFAULT.feedback).centred(),
    hex_param("Tone", "tone", "TONE", Comb::DEFAULT.tone),
    hex_param("Spread", "spread", "SPRD", Comb::DEFAULT.spread),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A comb's settings: a delay tuned to the track's note, fed back through a filter, after Vital's
/// comb filter. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Comb {
    pub mode: CombMode,
    /// How far its pitch is from the track's note, as a place in the list from two octaves down to
    /// two up.
    pub tune: u8,
    /// How long it rings, from none at `80`: above, its peaks sit on all the note's harmonics, and
    /// below, on its odd ones, for a hollower ring at the same pitch.
    pub feedback: u8,
    /// How bright what goes round is: its low-pass, from the note at `00` to everything at `FF`.
    pub tone: u8,
    /// How far down from `Tone` what goes round reaches, from a narrow band at `00` to everything
    /// under it at `FF`.
    pub spread: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Comb {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Comb {
    /// As it starts: a comb in tune with the note, ringing for about half a second at middle C,
    /// losing what's over four octaves up as it rings, and all wet.
    pub const DEFAULT: Self = Self {
        mode: CombMode::Comb,
        tune: MOST_SHIFT,
        feedback: 0xF0,
        tone: 0x80,
        spread: 0xFF,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        mode: CombMode,
        tune,
        feedback,
        tone,
        spread,
        mix,
        gain,
    }
}

/// The vowels a vowel filter moves through, in order along its `Vowel`.
pub const VOWEL_NAMES: [&str; 5] = ["A", "E", "I", "O", "U"];

/// Where a vowel filter's `Vowel` or `Size` is among its five vowels or voices, for a value on the
/// `00` to `FF` scale: the one it's coming from, the next, and how far it is from one to the other.
/// They sit at `00`, `40`, `80`, `C0` and `FF`, so `A` is at `00`, `E` at `40`, `I` at `80`, `O` at
/// `C0` and `U` at `FF`.
pub fn vowel_blend(value: f32) -> (usize, usize, f32) {
    let value = value.clamp(0.0, 255.0);
    let from = ((value / 64.0) as usize).min(VOWEL_NAMES.len() - 2);
    // The last is a step shorter, as `FF` is one short of `100`.
    let width = if from == VOWEL_NAMES.len() - 2 {
        63.0
    } else {
        64.0
    };
    (from, from + 1, (value - from as f32 * 64.0) / width)
}

/// A vowel filter's, in the order the page shows them, as columns of two: which vowel and how big
/// a voice, how sharp its formants are and how far they follow the note, then the mix and gain.
pub(crate) const VOWEL_PARAMS: [Param; 6] = [
    hex_param("Vowel", "vowel", "VOWL", Vowel::DEFAULT.vowel).units(Units::Vowel),
    hex_param("Size", "size", "SIZE", Vowel::DEFAULT.size),
    hex_param("Res", "res", "RESO", Vowel::DEFAULT.resonance),
    hex_param("Track", "track", "TRK", Vowel::DEFAULT.track),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A vowel filter's settings: a voice's formants, fixed in pitch as a vowel's are, morphing from
/// one vowel to the next, after Plaits' speech synth and Vital's formant filter. One a file
/// doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Vowel {
    /// Which vowel, from `A` at `00` through `E`, `I` and `O` to `U` at `FF`, blending between.
    pub vowel: u8,
    /// How big a voice, from a child's at `00` to a deep one at `FF`: its formants lower as it
    /// grows.
    pub size: u8,
    /// How sharp its formants are, from broad at `00` to ringing at `FF`.
    pub resonance: u8,
    /// How far its formants follow the track's note from middle C, from not at all at `00` to as
    /// far as the note moves at `FF`.
    pub track: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Vowel {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Vowel {
    /// As it starts: an `A` in a voice of middle size, its formants fixed, and all wet.
    pub const DEFAULT: Self = Self {
        vowel: 0x00,
        size: 0x80,
        resonance: 0x80,
        track: 0x00,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        vowel,
        size,
        resonance,
        track,
        mix,
        gain,
    }
}
