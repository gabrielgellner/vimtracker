//! The effects that filter a sound: an EQ, a low-pass gate, a resonator, a comb and a vowel
//! filter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::pitch::{MOST_SHIFT, PITCH_PARAMS};
use super::{
    AMOUNT_PARAM, GAIN_PARAM, MIX_PARAM, Param, SOURCE_NAMES, Setting, hex_param, source_track,
};

/// An EQ's, in the order the page shows them.
pub(crate) const EQ_PARAMS: [Param; 8] = [
    hex_param(
        "Low freq",
        "lowfreq",
        "LFRQ",
        Setting::LowFrequency,
        Equaliser::DEFAULT.low_frequency,
    ),
    hex_param("Low", "low", "LOW", Setting::Low, Equaliser::DEFAULT.low),
    hex_param(
        "Mid freq",
        "midfreq",
        "MFRQ",
        Setting::MidFrequency,
        Equaliser::DEFAULT.mid_frequency,
    ),
    hex_param("Mid", "mid", "MID", Setting::Mid, Equaliser::DEFAULT.mid),
    hex_param(
        "High freq",
        "highfreq",
        "HFRQ",
        Setting::HighFrequency,
        Equaliser::DEFAULT.high_frequency,
    ),
    hex_param(
        "High",
        "high",
        "HIGH",
        Setting::High,
        Equaliser::DEFAULT.high,
    ),
    // After the bands, so the page shows each band's frequency over its boost.
    hex_param(
        "Mid Q",
        "midq",
        "MIDQ",
        Setting::MidQ,
        Equaliser::DEFAULT.mid_q,
    ),
    GAIN_PARAM,
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::LowFrequency => self.low_frequency,
            Setting::Low => self.low,
            Setting::MidFrequency => self.mid_frequency,
            Setting::Mid => self.mid,
            Setting::MidQ => self.mid_q,
            Setting::HighFrequency => self.high_frequency,
            Setting::High => self.high,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut eq = self;
        match setting {
            Setting::LowFrequency => eq.low_frequency = value,
            Setting::Low => eq.low = value,
            Setting::MidFrequency => eq.mid_frequency = value,
            Setting::Mid => eq.mid = value,
            Setting::MidQ => eq.mid_q = value,
            Setting::HighFrequency => eq.high_frequency = value,
            Setting::High => eq.high = value,
            Setting::Gain => eq.gain = value,
            _ => {}
        }
        eq
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
        setting: Setting::GateMode,
        choices: &GATE_MODE_NAMES,
        wraps: true,
        default: 0,
    },
    Param {
        default: LowPassGate::DEFAULT.amount,
        ..AMOUNT_PARAM
    },
    hex_param(
        "Base",
        "base",
        "BASE",
        Setting::Base,
        LowPassGate::DEFAULT.base,
    ),
    hex_param(
        "Res",
        "res",
        "RESO",
        Setting::Resonance,
        LowPassGate::DEFAULT.resonance,
    ),
    hex_param(
        "Response",
        "response",
        "RESP",
        Setting::Response,
        LowPassGate::DEFAULT.response,
    ),
    Param {
        name: "Source",
        key: "source",
        // What it hears is part of how it's set up, so steps don't change it.
        label: None,
        setting: Setting::Source,
        choices: &SOURCE_NAMES,
        wraps: false,
        default: LowPassGate::DEFAULT.source,
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::GateMode => self.mode as u8,
            Setting::Amount => self.amount,
            Setting::Base => self.base,
            Setting::Resonance => self.resonance,
            Setting::Response => self.response,
            Setting::Source => self.source,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut gate = self;
        match setting {
            Setting::GateMode => gate.mode = GateMode::ALL[usize::from(value)],
            Setting::Amount => gate.amount = value,
            Setting::Base => gate.base = value,
            Setting::Resonance => gate.resonance = value,
            Setting::Response => gate.response = value,
            Setting::Source => gate.source = value,
            Setting::Mix => gate.mix = value,
            Setting::Gain => gate.gain = value,
            _ => {}
        }
        gate
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
    hex_param(
        "Material",
        "material",
        "MATL",
        Setting::Material,
        Resonator::DEFAULT.material,
    ),
    hex_param(
        "Bright",
        "bright",
        "BRT",
        Setting::Bright,
        Resonator::DEFAULT.bright,
    ),
    hex_param(
        "Decay",
        "decay",
        "DCAY",
        Setting::Decay,
        Resonator::DEFAULT.decay,
    ),
    hex_param(
        "Position",
        "position",
        "POS",
        Setting::Pickup,
        Resonator::DEFAULT.position,
    ),
    hex_param(
        "Width",
        "width",
        "WDTH",
        Setting::Width,
        Resonator::DEFAULT.width,
    ),
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Shift => self.tune,
            Setting::Material => self.material,
            Setting::Bright => self.bright,
            Setting::Decay => self.decay,
            Setting::Pickup => self.position,
            Setting::Width => self.width,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut resonator = self;
        match setting {
            Setting::Shift => resonator.tune = value,
            Setting::Material => resonator.material = value,
            Setting::Bright => resonator.bright = value,
            Setting::Decay => resonator.decay = value,
            Setting::Pickup => resonator.position = value,
            Setting::Width => resonator.width = value,
            Setting::Mix => resonator.mix = value,
            Setting::Gain => resonator.gain = value,
            _ => {}
        }
        resonator
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
        setting: Setting::CombMode,
        choices: &COMB_MODE_NAMES,
        wraps: true,
        default: 0,
    },
    Param {
        name: "Tune",
        key: "tune",
        label: Some("TUNE"),
        default: Comb::DEFAULT.tune,
        ..PITCH_PARAMS[0]
    },
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::FlangerFeedback,
        Comb::DEFAULT.feedback,
    ),
    hex_param("Tone", "tone", "TONE", Setting::Bright, Comb::DEFAULT.tone),
    hex_param(
        "Spread",
        "spread",
        "SPRD",
        Setting::Spread,
        Comb::DEFAULT.spread,
    ),
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::CombMode => self.mode as u8,
            Setting::Shift => self.tune,
            Setting::FlangerFeedback => self.feedback,
            Setting::Bright => self.tone,
            Setting::Spread => self.spread,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut comb = self;
        match setting {
            Setting::CombMode => comb.mode = CombMode::ALL[usize::from(value)],
            Setting::Shift => comb.tune = value,
            Setting::FlangerFeedback => comb.feedback = value,
            Setting::Bright => comb.tone = value,
            Setting::Spread => comb.spread = value,
            Setting::Mix => comb.mix = value,
            Setting::Gain => comb.gain = value,
            _ => {}
        }
        comb
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
    hex_param(
        "Vowel",
        "vowel",
        "VOWL",
        Setting::Vowel,
        Vowel::DEFAULT.vowel,
    ),
    hex_param(
        "Size",
        "size",
        "SIZE",
        Setting::VoiceSize,
        Vowel::DEFAULT.size,
    ),
    hex_param(
        "Res",
        "res",
        "RESO",
        Setting::Resonance,
        Vowel::DEFAULT.resonance,
    ),
    hex_param(
        "Track",
        "track",
        "TRK",
        Setting::KeyTrack,
        Vowel::DEFAULT.track,
    ),
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

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Vowel => self.vowel,
            Setting::VoiceSize => self.size,
            Setting::Resonance => self.resonance,
            Setting::KeyTrack => self.track,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut vowel = self;
        match setting {
            Setting::Vowel => vowel.vowel = value,
            Setting::VoiceSize => vowel.size = value,
            Setting::Resonance => vowel.resonance = value,
            Setting::KeyTrack => vowel.track = value,
            Setting::Mix => vowel.mix = value,
            Setting::Gain => vowel.gain = value,
            _ => {}
        }
        vowel
    }
}
