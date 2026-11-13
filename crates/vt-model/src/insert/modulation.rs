//! The effects that move: a chorus, a phaser, a flanger, a rotary speaker and Warps.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;
use crate::lfo::SWEEP_NAMES;

use super::pitch::{MOST_SHIFT, PITCH_PARAMS};
use super::{
    GAIN_PARAM, MIX_PARAM, OFF_ON, Param, SOURCE_NAMES, Setting, WET_MIX_PARAM, from_middle,
    hex_param, source_track,
};

/// How a chorus moves its copies: Mutable Instruments' Rings' two, then a Roland Juno's three.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChorusType {
    /// Two copies, slowly drifting apart from each other.
    #[default]
    Chorus,
    /// Three copies on a slow and a fast wobble, a third of a turn apart, as a string machine has.
    Ensemble,
    /// A Juno's chorus I: a short delay on each side swept by a slow triangle, the two sides in
    /// antiphase.
    JunoI,
    /// Its chorus II: the same, a little faster.
    JunoII,
    /// Both buttons down: a fast, shallow wobble, nearer a vibrato.
    JunoBoth,
}

impl ChorusType {
    pub const ALL: [Self; 5] = [
        Self::Chorus,
        Self::Ensemble,
        Self::JunoI,
        Self::JunoII,
        Self::JunoBoth,
    ];

    /// Whether it's one of the Juno's.
    pub fn is_juno(self) -> bool {
        matches!(self, Self::JunoI | Self::JunoII | Self::JunoBoth)
    }
}

const CHORUS_TYPE_NAMES: [&str; 5] = ["Chorus", "Ensemble", "Juno I", "Juno II", "Juno I+II"];

/// A chorus's, in the order the page shows them.
pub(crate) const CHORUS_PARAMS: [Param; 5] = [
    Param {
        name: "Type",
        key: "type",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        setting: Setting::ChorusType,
        choices: &CHORUS_TYPE_NAMES,
        wraps: true,
        default: 0,
    },
    hex_param("Rate", "rate", "RATE", Setting::Rate, Chorus::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Chorus::DEFAULT.depth,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A chorus's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chorus {
    pub kind: ChorusType,
    /// How fast the copies move: as Rings has it at `80`, two octaves slower at `00` and faster at
    /// `FF`.
    pub rate: u8,
    /// How far they move.
    pub depth: u8,
    /// From `00`, all dry, to `FF`, all wet. Half is the classic chorus.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Chorus {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Chorus {
    /// As it starts.
    pub const DEFAULT: Self = Self {
        kind: ChorusType::Chorus,
        rate: 0x80,
        depth: 0x80,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::ChorusType => self.kind as u8,
            Setting::Rate => self.rate,
            Setting::Depth => self.depth,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut chorus = self;
        match setting {
            Setting::ChorusType => {
                chorus.kind = ChorusType::ALL[usize::from(value).min(ChorusType::ALL.len() - 1)];
            }
            Setting::Rate => chorus.rate = value,
            Setting::Depth => chorus.depth = value,
            Setting::Mix => chorus.mix = value,
            Setting::Gain => chorus.gain = value,
            _ => {}
        }
        chorus
    }
}

/// How many semitones a phaser's `Depth` sweeps either way of its centre, at full.
pub const MOST_PHASER_SEMITONES: f32 = 48.0;
/// A flanger's shortest and longest delay at the middle of its sweep, in ms.
pub const SHORTEST_FLANGE_MS: f32 = 0.5;
pub const LONGEST_FLANGE_MS: f32 = 10.0;

/// A flanger's delay in ms at the middle of its sweep, for a value on the `00` to `FF` scale: from
/// 0.5 ms to 10 ms, evenly in ratio.
pub fn flanger_delay_ms(value: f32) -> f32 {
    SHORTEST_FLANGE_MS * (LONGEST_FLANGE_MS / SHORTEST_FLANGE_MS).powf(value / 255.0)
}

/// A flanger's feedback from -1 to 1 for a value on the `00` to `FF` scale, with none at `80`.
pub fn flanger_feedback(value: f32) -> f32 {
    from_middle(value)
}

/// How many all-passes a phaser can have, as Vital's does: each four make two notches.
pub const STAGE_NAMES: [&str; 3] = ["4", "8", "12"];

/// The `Rate` of a phaser or flanger: how long its sweep takes, in note lengths.
const fn sweep_param(default: u8) -> Param {
    Param {
        name: "Rate",
        key: "rate",
        label: Some("RATE"),
        setting: Setting::SweepRate,
        choices: &SWEEP_NAMES,
        wraps: false,
        default,
    }
}

/// A phaser's, in the order the page shows them.
pub(crate) const PHASER_PARAMS: [Param; 7] = [
    sweep_param(Phaser::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Phaser::DEFAULT.depth,
    ),
    hex_param(
        "Centre",
        "centre",
        "CNTR",
        Setting::Centre,
        Phaser::DEFAULT.centre,
    ),
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::Feedback,
        Phaser::DEFAULT.feedback,
    ),
    Param {
        name: "Stages",
        key: "stages",
        label: Some("STGS"),
        setting: Setting::Stages,
        choices: &STAGE_NAMES,
        wraps: false,
        default: Phaser::DEFAULT.stages,
    },
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A phaser's settings, after Vital's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Phaser {
    /// How long a sweep takes, as a place in the list of sweeps.
    pub rate: u8,
    /// How far it sweeps either way of its centre: up to 48 semitones at `FF`.
    pub depth: u8,
    /// The frequency it sweeps around, from 20 Hz to 20 kHz as the filter's cutoff.
    pub centre: u8,
    /// How much of what comes out goes round again, which sharpens the notches into peaks.
    pub feedback: u8,
    /// How many all-passes, as a place among 4, 8 and 12.
    pub stages: u8,
    /// From `00`, all dry, to `FF`, all through the all-passes. Half has the deepest notches.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Phaser {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Phaser {
    /// As it starts: a sweep every 2 bars, 24 semitones either way of about 640 Hz, half
    /// feedback and 8 stages, as Vital's starts, and half wet.
    pub const DEFAULT: Self = Self {
        rate: 18,
        depth: 0x80,
        centre: 0x80,
        feedback: 0x80,
        stages: 1,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::SweepRate => self.rate,
            Setting::Depth => self.depth,
            Setting::Centre => self.centre,
            Setting::Feedback => self.feedback,
            Setting::Stages => self.stages,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut phaser = self;
        match setting {
            Setting::SweepRate => phaser.rate = value,
            Setting::Depth => phaser.depth = value,
            Setting::Centre => phaser.centre = value,
            Setting::Feedback => phaser.feedback = value,
            Setting::Stages => phaser.stages = value,
            Setting::Mix => phaser.mix = value,
            Setting::Gain => phaser.gain = value,
            _ => {}
        }
        phaser
    }
}

/// A flanger's, in the order the page shows them.
pub(crate) const FLANGER_PARAMS: [Param; 6] = [
    sweep_param(Flanger::DEFAULT.rate),
    hex_param(
        "Depth",
        "depth",
        "DPTH",
        Setting::Depth,
        Flanger::DEFAULT.depth,
    ),
    hex_param(
        "Delay",
        "delay",
        "DLAY",
        Setting::FlangerDelay,
        Flanger::DEFAULT.delay,
    ),
    hex_param(
        "Feedback",
        "feedback",
        "FDBK",
        Setting::FlangerFeedback,
        Flanger::DEFAULT.feedback,
    ),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// A flanger's settings, after Vital's. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Flanger {
    /// How long a sweep takes, as a place in the list of sweeps.
    pub rate: u8,
    /// How far the delay sweeps: at `FF`, from 0.5 ms to twice the middle and back.
    pub depth: u8,
    /// The delay at the middle of the sweep, from 0.5 ms to 10 ms.
    pub delay: u8,
    /// How much of the delay goes round again, from all of it upside down at `00`, through none at
    /// `80`, to all of it at `FF`. It never quite reaches all, so it can't run away.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all delayed. Half has the deepest comb.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Flanger {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Flanger {
    /// As it starts: a sweep every bar, half depth around about 2.2 ms, and half feedback, as
    /// Vital's starts, and half wet.
    pub const DEFAULT: Self = Self {
        rate: 17,
        depth: 0x80,
        delay: 0x80,
        feedback: 0xC0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::SweepRate => self.rate,
            Setting::Depth => self.depth,
            Setting::FlangerDelay => self.delay,
            Setting::FlangerFeedback => self.feedback,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut flanger = self;
        match setting {
            Setting::SweepRate => flanger.rate = value,
            Setting::Depth => flanger.depth = value,
            Setting::FlangerDelay => flanger.delay = value,
            Setting::FlangerFeedback => flanger.feedback = value,
            Setting::Mix => flanger.mix = value,
            Setting::Gain => flanger.gain = value,
            _ => {}
        }
        flanger
    }
}

/// A rotary speaker's `Fast` switched on, where its rotors spin fast.
pub const FAST_SPEED: u8 = 1;

/// A rotary speaker's, in the order the page shows them, as columns: its speed switch and how
/// long its rotors take to change; then the horn's Doppler and swell; then the amplifier's drive
/// and the width; then the mix and gain.
pub(crate) const ROTARY_PARAMS: [Param; 8] = [
    Param {
        name: "Fast",
        key: "fast",
        label: Some("FAST"),
        setting: Setting::Fast,
        choices: &OFF_ON,
        wraps: true,
        default: Rotary::DEFAULT.fast,
    },
    hex_param(
        "Inertia",
        "inertia",
        "INRT",
        Setting::Inertia,
        Rotary::DEFAULT.inertia,
    ),
    hex_param(
        "Doppler",
        "doppler",
        "DOPL",
        Setting::Doppler,
        Rotary::DEFAULT.doppler,
    ),
    hex_param(
        "Tremolo",
        "tremolo",
        "TREM",
        Setting::Tremolo,
        Rotary::DEFAULT.tremolo,
    ),
    hex_param(
        "Drive",
        "drive",
        "DRIV",
        Setting::Drive,
        Rotary::DEFAULT.drive,
    ),
    hex_param(
        "Width",
        "width",
        "WDTH",
        Setting::Width,
        Rotary::DEFAULT.width,
    ),
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A rotary speaker's settings: a Leslie's horn and drum, after sst's, with rotors that take time
/// to change speed. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rotary {
    /// Switched on, the rotors spin fast, the Leslie's tremolo; off, slow, its chorale.
    pub fast: u8,
    /// How long the rotors take to get to a new speed: at once at `00`, a Leslie's at `80`, twice
    /// as long at `FF`.
    pub inertia: u8,
    /// How far the horn's pitch wavers as it comes and goes.
    pub doppler: u8,
    /// How far its loudness swells as it points at each side and away.
    pub tremolo: u8,
    /// How hard the amplifier is driven, from clean at `00`.
    pub drive: u8,
    /// How far apart its two microphones are heard, from mono at `00` to as they are at `FF`.
    pub width: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Rotary {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Rotary {
    /// As it starts: slow, a Leslie's inertia, half Doppler and swell, clean, wide and all wet.
    pub const DEFAULT: Self = Self {
        fast: 0,
        inertia: 0x80,
        doppler: 0x80,
        tremolo: 0x80,
        drive: 0x00,
        width: 0xFF,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Fast => self.fast,
            Setting::Inertia => self.inertia,
            Setting::Doppler => self.doppler,
            Setting::Tremolo => self.tremolo,
            Setting::Drive => self.drive,
            Setting::Width => self.width,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut rotary = self;
        match setting {
            Setting::Fast => rotary.fast = value,
            Setting::Inertia => rotary.inertia = value,
            Setting::Doppler => rotary.doppler = value,
            Setting::Tremolo => rotary.tremolo = value,
            Setting::Drive => rotary.drive = value,
            Setting::Width => rotary.width = value,
            Setting::Mix => rotary.mix = value,
            Setting::Gain => rotary.gain = value,
            _ => {}
        }
        rotary
    }
}

/// What Warps can take as its modulator: its own oscillator, a track by its number, or a return,
/// as the compressor's `Source` but with the oscillator in place of `Self`.
pub const WARPS_SOURCE_NAMES: [&str; SOURCE_NAMES.len()] = {
    let mut names = SOURCE_NAMES;
    names[0] = "Osc";
    names
};

/// The waveforms Warps' oscillator plays, as Warps' own: a sine, a triangle, a saw, a square, and
/// noise low-passed at four times its pitch.
pub const WAVE_NAMES: [&str; 5] = ["Sine", "Tri", "Saw", "Square", "Noise"];

/// What Warps' `Algo` goes through from `00` to `FF`, as the status line names them: its six
/// cross-modulations, the modulator on its own as it fades over, and the vocoder.
pub const ALGORITHM_NAMES: [&str; 8] = [
    "Xfade", "Fold", "Ring", "Digital", "XOR", "Compare", "Mod", "Vocoder",
];

/// Where Warps' `Algo` starts fading from its cross-modulations to the bare modulator, where it's
/// only the modulator, and where it's only the vocoder, on its `0` to `1` scale.
pub const ALGORITHM_FADE_START: f32 = 0.675;
pub const ALGORITHM_MODULATOR: f32 = 0.7;
pub const ALGORITHM_VOCODER: f32 = 0.725;

/// Where Warps' algorithm knob is, from `0` to `1`, for `Algo` on the `00` to `FF` scale. It's
/// out of 256, so each cross-modulation, an eighth of the way from the last, lands on a value:
/// `00`, `20`, `40`, `60`, `80` and `A0`.
pub fn algorithm_knob(value: f32) -> f32 {
    (value / 256.0).clamp(0.0, 1.0)
}

/// Where Warps' `Algo` is, for a value on the `00` to `FF` scale, as the two of `ALGORITHM_NAMES`
/// it's between and how far it is from the first to the second. Each cross-modulation is an
/// eighth of the way from the last, as Warps' knob has them, then it fades through the modulator
/// into the vocoder.
pub fn algorithm_blend(value: f32) -> (usize, usize, f32) {
    let at = algorithm_knob(value);
    if at < 0.625 {
        let place = at * 8.0;
        let from = place as usize;
        (from, from + 1, place - from as f32)
    } else if at < ALGORITHM_MODULATOR {
        (5, 6, (at - 0.625) / (ALGORITHM_MODULATOR - 0.625))
    } else if at < ALGORITHM_VOCODER {
        (
            6,
            7,
            (at - ALGORITHM_MODULATOR) / (ALGORITHM_VOCODER - ALGORITHM_MODULATOR),
        )
    } else {
        (7, 7, 0.0)
    }
}

/// Warps', in the order the page shows them, as columns of two: the algorithm and its setting,
/// the modulator and how hard it's driven, then the oscillator's pitch and waveform.
pub(crate) const WARPS_PARAMS: [Param; 8] = [
    hex_param(
        "Algo",
        "algo",
        "ALGO",
        Setting::Algorithm,
        Warps::DEFAULT.algorithm,
    ),
    hex_param(
        "Timbre",
        "timbre",
        "TMBR",
        Setting::Timbre,
        Warps::DEFAULT.timbre,
    ),
    Param {
        name: "Source",
        key: "source",
        // What it hears is part of how it's set up, so steps don't change it.
        label: None,
        setting: Setting::Source,
        choices: &WARPS_SOURCE_NAMES,
        wraps: false,
        default: Warps::DEFAULT.source,
    },
    hex_param(
        "Level",
        "level",
        "LEVL",
        Setting::ModulatorLevel,
        Warps::DEFAULT.level,
    ),
    Param {
        default: Warps::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    Param {
        name: "Shape",
        key: "shape",
        label: Some("SHAP"),
        setting: Setting::Wave,
        choices: &WAVE_NAMES,
        wraps: true,
        default: Warps::DEFAULT.shape,
    },
    Param {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// Warps' settings, after Mutable Instruments' Warps. The slot's own sound is the carrier, Warps'
/// first input. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Warps {
    /// Warps' algorithm knob, from crossfading, through folding, ring modulation, XOR and the
    /// comparator, to the vocoder, each blending into the next.
    pub algorithm: u8,
    /// Warps' timbre knob: what each algorithm does with it, from how far it crossfades to the
    /// vocoder's formant shift.
    pub timbre: u8,
    /// The modulator, Warps' second input: `0` for its own oscillator, a track's number, or a
    /// return after the tracks.
    pub source: u8,
    /// How hard the modulator is driven, Warps' second level knob.
    pub level: u8,
    /// How far the oscillator is from the track's note, as a place in the list from two octaves
    /// down to two up.
    pub pitch: u8,
    /// The oscillator's waveform, as a place among `WAVE_NAMES`.
    pub shape: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Warps {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Warps {
    /// As it starts: the digital ring modulation of the sound by a sine an octave above its note,
    /// driven a little over unity, all wet.
    pub const DEFAULT: Self = Self {
        algorithm: 0x60,
        timbre: 0x80,
        source: 0,
        level: 0xA0,
        pitch: MOST_SHIFT + 12,
        shape: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    /// The track it takes as its modulator, by its place among the tracks, if not its own
    /// oscillator.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    /// A setting's value. One it doesn't have reads as nothing.
    pub(super) fn read(self, setting: Setting) -> u8 {
        match setting {
            Setting::Algorithm => self.algorithm,
            Setting::Timbre => self.timbre,
            Setting::Source => self.source,
            Setting::ModulatorLevel => self.level,
            Setting::Shift => self.pitch,
            Setting::Wave => self.shape,
            Setting::Mix => self.mix,
            Setting::Gain => self.gain,
            _ => 0,
        }
    }

    /// The same with a setting changed, within its range.
    pub(super) fn written(self, setting: Setting, value: u8) -> Self {
        let value = value.min(setting.highest());
        let mut warps = self;
        match setting {
            Setting::Algorithm => warps.algorithm = value,
            Setting::Timbre => warps.timbre = value,
            Setting::Source => warps.source = value,
            Setting::ModulatorLevel => warps.level = value,
            Setting::Shift => warps.pitch = value,
            Setting::Wave => warps.shape = value,
            Setting::Mix => warps.mix = value,
            Setting::Gain => warps.gain = value,
            _ => {}
        }
        warps
    }
}
