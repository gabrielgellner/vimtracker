//! The effects that move a sound's pitch: a pitch shifter, a granular effect, Clouds, Beads and
//! a frequency shifter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::{
    Columns, Entry, GAIN_PARAM, Kind, MIX_PARAM, OFF_ON, Place, Units, WET_MIX_PARAM, from_middle,
    hex_param,
};

/// How far a pitch shifter goes either way, in semitones.
pub const MOST_SHIFT: u8 = 24;
/// The shifts it chooses between, from two octaves down to two up, written as the track's `Pitch`
/// shows semitones.
pub(super) const SHIFT_NAMES: [&str; (MOST_SHIFT * 2 + 1) as usize] = [
    "-24 st", "-23 st", "-22 st", "-21 st", "-20 st", "-19 st", "-18 st", "-17 st", "-16 st",
    "-15 st", "-14 st", "-13 st", "-12 st", "-11 st", "-10 st", "-9 st", "-8 st", "-7 st", "-6 st",
    "-5 st", "-4 st", "-3 st", "-2 st", "-1 st", "+0 st", "+1 st", "+2 st", "+3 st", "+4 st",
    "+5 st", "+6 st", "+7 st", "+8 st", "+9 st", "+10 st", "+11 st", "+12 st", "+13 st", "+14 st",
    "+15 st", "+16 st", "+17 st", "+18 st", "+19 st", "+20 st", "+21 st", "+22 st", "+23 st",
    "+24 st",
];
/// How far a pitch shifter's `Fine` goes either way, in cents.
pub(crate) const MOST_FINE_CENTS: f32 = 50.0;
/// A pitch shifter's shortest and longest grain, in ms.
pub const SHORTEST_GRAIN_MS: f32 = 10.0;
pub const LONGEST_GRAIN_MS: f32 = 100.0;

/// How many semitones a pitch shifter's `Pitch` shifts by, for a place in its list.
pub fn shift_semitones(value: f32) -> f32 {
    value - f32::from(MOST_SHIFT)
}

/// A pitch shifter's fine shift in cents, for a value on the `00` to `FF` scale: from -50, through
/// none at `80`, to +50.
pub fn fine_cents(value: f32) -> f32 {
    from_middle(value) * MOST_FINE_CENTS
}

/// How long a pitch shifter's grains are in ms, for a value on the `00` to `FF` scale: from 10 ms
/// to 100 ms, evenly in ratio.
pub fn grain_ms(value: f32) -> f32 {
    SHORTEST_GRAIN_MS * (LONGEST_GRAIN_MS / SHORTEST_GRAIN_MS).powf(value / 255.0)
}

/// A pitch shifter's, in the order the page shows them.
pub(crate) const PITCH_PARAMS: [Entry; 6] = [
    Entry {
        name: "Pitch",
        key: "pitch",
        label: Some("PTCH"),
        kind: Kind::Stepped {
            choices: &SHIFT_NAMES,
            wraps: false,
        },
        default: Pitch::DEFAULT.shift,
        place: Place::Own,
    },
    hex_param("Fine", "fine", "FINE", Pitch::DEFAULT.fine)
        .units(Units::Cents)
        .centred(),
    hex_param("Size", "size", "SIZE", Pitch::DEFAULT.size).units(Units::Grain),
    hex_param("Feedback", "feedback", "FDBK", Pitch::DEFAULT.feedback),
    Entry {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A pitch shifter's settings, after Clouds'. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pitch {
    /// How far it shifts, as a place in the list from two octaves down to two up, none in the
    /// middle.
    pub shift: u8,
    /// How far it shifts besides, from 50 cents down to 50 up, none at `80`.
    pub fine: u8,
    /// How long each of its two grains is, from 10 ms to 100 ms.
    pub size: u8,
    /// How much of what comes out goes back in to be shifted again.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Pitch {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Pitch {
    /// As it starts: no shift, Clouds' grain of about 64 ms, no feedback, and all wet.
    pub const DEFAULT: Self = Self {
        shift: MOST_SHIFT,
        fine: 0x80,
        size: 0xCE,
        feedback: 0,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        shift,
        fine,
        size,
        feedback,
        mix,
        gain,
    }
}

/// A granular effect's, in the order the page shows them, as columns of two: where and how long,
/// how often and how wide, freezing and straying, then pitch and envelope, whose short names fit
/// the narrow fourth column.
pub(crate) const GRAINS_PARAMS: [Entry; 10] = [
    hex_param("Time", "time", "TIME", Grains::DEFAULT.time).units(Units::GrainTime),
    hex_param("Size", "size", "SIZE", Grains::DEFAULT.size).units(Units::GrainLength),
    hex_param("Density", "density", "DENS", Grains::DEFAULT.density).units(Units::Density),
    hex_param("Spread", "spread", "SPRD", Grains::DEFAULT.spread),
    Entry {
        name: "Freeze",
        key: "freeze",
        label: Some("FRZ"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Grains::DEFAULT.freeze,
        place: Place::Own,
    },
    hex_param("Random", "random", "RAND", Grains::DEFAULT.random),
    Entry {
        default: Grains::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    hex_param("Shape", "shape", "SHAP", Grains::DEFAULT.shape),
    WET_MIX_PARAM,
    GAIN_PARAM,
];

/// How much a granular effect records, in seconds, as Beads' does at its best quality.
pub const GRAIN_BUFFER_SECONDS: f32 = 4.0;
/// How often a granular effect's grains can start at most, in Hz, as Beads': the C an octave below
/// middle C. At random they start half as often at most, on average.
pub(crate) const MOST_GRAIN_HERTZ: f32 = 130.812_78;

/// How far back a granular effect's grains read, in seconds, for a value on the `00` to `FF` scale:
/// from just behind what's being recorded to the whole recording, evenly.
pub(crate) fn grain_time_seconds(value: f32) -> f32 {
    value / 255.0 * GRAIN_BUFFER_SECONDS
}

/// How long a granular effect's grains are, in seconds, and whether they play backwards, for a
/// value on the `00` to `FF` scale, as Beads' `Size` knob: from the whole recording backwards at
/// `00`, shortening to about 8 ms at a third, then lengthening forwards to the whole of it at `FF`.
pub fn grain_length(value: f32) -> (f32, bool) {
    let size = value / 255.0;
    let reversed = size < 1.0 / 3.0;
    let shortness = if reversed {
        size * 3.0 * (2.0 - size * 3.0)
    } else {
        1.0 - 1.5 * (size - 1.0 / 3.0)
    };
    let shortness = (shortness * 1.025).min(1.0);
    ((shortness * -9.0).exp2() * GRAIN_BUFFER_SECONDS, reversed)
}

/// How a granular effect's grains start, for a `Density`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Triggering {
    /// Evenly, so many a second.
    Regular(f32),
    /// Not at all.
    Off,
    /// At random, about so many a second.
    Random(f32),
}

/// How a granular effect's grains start, for a value on the `00` to `FF` scale, as Beads' `Density`
/// knob: evenly and fastest at `00`, slowing to none in the middle, then at random, faster
/// towards `FF`. Each side spans 16 octaves.
pub fn grain_density(value: f32) -> Triggering {
    let density = value / 255.0;
    if density < 0.48 {
        Triggering::Regular(MOST_GRAIN_HERTZ * (-density * 16.0).exp2())
    } else if density > 0.52 {
        Triggering::Random(0.5 * MOST_GRAIN_HERTZ * (-(1.0 - density) * 16.0).exp2())
    } else {
        Triggering::Off
    }
}

/// A granular effect's settings, after Beads'. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grains {
    /// How far back the grains start reading, from just behind what's being recorded to four
    /// seconds back.
    pub time: u8,
    /// How long each grain is, and below a third whether it plays backwards, as Beads' `Size`.
    pub size: u8,
    /// How far each grain is shifted, as a place in the list from two octaves down to two up.
    pub pitch: u8,
    /// Each grain's envelope, as Beads' `Shape`: from a square, through a decay and a bell, to a
    /// reversed decay.
    pub shape: u8,
    /// How often grains start, as Beads' `Density`: evenly below `80`, at random above it.
    pub density: u8,
    /// How far the grains spread across the sides. `Spectral` doesn't use it.
    pub spread: u8,
    /// Whether it stops recording, `1`, so the grains keep playing what's held.
    pub freeze: u8,
    /// How far each grain's time, size and shape stray at random from what's set.
    pub random: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for Grains {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Grains {
    /// As it starts: half a second back, grains of about 150 ms in a bell, no shift, about seven
    /// a second at random, spread half way, recording, nothing straying, and half wet.
    pub const DEFAULT: Self = Self {
        time: 0x20,
        size: 0xA8,
        pitch: MOST_SHIFT,
        shape: 0xAA,
        density: 0xCC,
        spread: 0x80,
        freeze: 0,
        random: 0,
        mix: 0x80,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        time,
        size,
        density,
        spread,
        freeze,
        random,
        pitch,
        shape,
        mix,
        gain,
    }
}

/// What Clouds does with what it records, as the module's modes do.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CloudsMode {
    /// Grains read the recording, overlapping as `Density` says, shaped and then diffused by
    /// `Texture`.
    #[default]
    Granular,
    /// Two windows read the recording, each lined up with the one before where the two match
    /// best, so it can be slowed, frozen or shifted without grains' blur.
    Stretch,
    /// A delay as long as `Position` says, its repeats shifted by `Pitch`; frozen, a loop of what
    /// it holds as long as `Size` says.
    Looping,
    /// The module's phase vocoder: what comes in is taken apart into frames of its spectrum, which
    /// are kept, warped, shifted and quantised, then put back together, about 160 ms later.
    Spectral,
}

impl CloudsMode {
    pub const ALL: [Self; 4] = [Self::Granular, Self::Stretch, Self::Looping, Self::Spectral];
}

const CLOUDS_MODE_NAMES: [&str; 4] = ["Granular", "Stretch", "Looping", "Spectral"];

/// Clouds' four qualities, the module's: its recording in stereo or mono, in 16 bits or in 8-bit
/// mu-law at half the rate, each holding a longer recording than the one before.
pub const QUALITY_NAMES: [&str; 4] = ["Stereo 16", "Mono 16", "Stereo 8", "Mono 8"];

/// Clouds', in the order the slot keeps them: its mode, which the title shows, the module's six
/// knobs, `Blend`'s other three jobs as settings of their own, `Freeze` and `Quality`, then the
/// gain every slot has.
pub(crate) const CLOUDS_PARAMS: [Entry; 13] = [
    Entry {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &CLOUDS_MODE_NAMES,
            wraps: true,
        },
        default: 0,
        place: Place::Type,
    },
    hex_param("Position", "position", "POS", Clouds::DEFAULT.position),
    hex_param("Size", "size", "SIZE", Clouds::DEFAULT.size),
    Entry {
        default: Clouds::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    hex_param("Density", "density", "DENS", Clouds::DEFAULT.density),
    hex_param("Texture", "texture", "TEXT", Clouds::DEFAULT.texture),
    Entry {
        name: "Blend",
        key: "blend",
        label: Some("BLND"),
        default: Clouds::DEFAULT.blend,
        ..MIX_PARAM
    },
    hex_param("Spread", "spread", "SPRD", Clouds::DEFAULT.spread),
    hex_param("Feedback", "feedback", "FDBK", Clouds::DEFAULT.feedback),
    hex_param("Reverb", "reverb", "VERB", Clouds::DEFAULT.reverb),
    Entry {
        name: "Freeze",
        key: "freeze",
        label: Some("FRZ"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Clouds::DEFAULT.freeze,
        place: Place::Own,
    },
    Entry {
        name: "Quality",
        key: "quality",
        // A new quality starts a new recording, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &QUALITY_NAMES,
            wraps: true,
        },
        default: Clouds::DEFAULT.quality,
        place: Place::Own,
    },
    GAIN_PARAM,
];

/// Where its settings sit on the FX page. The module's big knobs, then its small ones with its freeze button, then what its
/// `Blend` knob also does, then its quality over what comes out.
pub(super) const CLOUDS_GRID: Columns = &[
    &["position", "size", "pitch"],
    &["density", "texture", "freeze"],
    &["spread", "feedback", "reverb"],
    &["quality", "blend", "gain"],
];

/// Clouds' settings: Mutable Instruments' texture synthesizer as the module plays, with its modes,
/// its six knobs, `Blend`'s four jobs and its qualities. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clouds {
    pub mode: CloudsMode,
    /// Where in the recording it reads, from what's just come in at `00` to the oldest at `FF`; in
    /// `Looping`, how long its delay is; in `Spectral`, which of its kept spectra it writes and
    /// plays, blending between them.
    pub position: u8,
    /// How long its grains, windows or loop are; in `Spectral`, how it warps the spectrum, none at
    /// `80`.
    pub size: u8,
    /// How far it shifts, as a place in the list from two octaves down to two up.
    pub pitch: u8,
    /// In `Granular`, how many grains overlap and how they start: regularly below `80`, none in
    /// the middle, and at random above, as the module's manual has it. In `Stretch` and `Looping`,
    /// how far the diffuser smears what comes out. In `Spectral`, how often its spectrum is
    /// updated: bins left at random below `80`, frames merged above, random phases at both ends.
    pub density: u8,
    /// In `Granular`, the grains' envelope, from a square through a triangle to a bell, then the
    /// diffuser above `C0`. In `Stretch` and `Looping`, a low-pass below `80` and a high-pass above
    /// it. In `Spectral`, its levels quantised below `80`, and the strongest bins weakened above.
    pub texture: u8,
    /// From `00`, all dry, to `FF`, all wet: `Blend`'s first job on the module.
    pub blend: u8,
    /// How far the grains spread across the sides. `Spectral` doesn't use it.
    pub spread: u8,
    /// How much of what comes out goes back in to be recorded again.
    pub feedback: u8,
    /// How much of Clouds' own reverb is on what comes out.
    pub reverb: u8,
    /// Whether it stops recording, `1`, so it keeps playing what it holds.
    pub freeze: u8,
    /// Its recording's quality, a place in [`QUALITY_NAMES`].
    pub quality: u8,
    pub gain: u8,
}

impl Default for Clouds {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Clouds {
    /// As it starts: grains a quarter of the way back, about 130 ms long, no shift, a few at a
    /// time at random, a smooth envelope, half wet, spread half way, no feedback, a little reverb,
    /// recording, in stereo at 16 bits.
    pub const DEFAULT: Self = Self {
        mode: CloudsMode::Granular,
        position: 0x40,
        size: 0x80,
        pitch: MOST_SHIFT,
        density: 0xC0,
        texture: 0x80,
        blend: 0x80,
        spread: 0x80,
        feedback: 0,
        reverb: 0x40,
        freeze: 0,
        quality: 0,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        mode: CloudsMode,
        position,
        size,
        pitch,
        density,
        texture,
        blend,
        spread,
        feedback,
        reverb,
        freeze,
        quality,
        gain,
    }
}

/// What Beads does with what it records, as the module's modes do: grains, or a delay whose taps
/// play it back. Its third, a wavetable synth for when nothing's patched in, is left out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeadsMode {
    /// Grains read the recording, as many and as often as `Density` says, each from where `Time`
    /// says, as long as `Size` says, under `Shape`'s envelope.
    #[default]
    Granular,
    /// The module with `Size` all the way up: taps read the recording at a delay `Density` sets
    /// and `Time` multiplies, shaped by `Shape`, shifted by `Pitch`, filtered by `Size`; frozen, a
    /// slice of it loops.
    Delay,
}

impl BeadsMode {
    pub const ALL: [Self; 2] = [Self::Granular, Self::Delay];
}

const BEADS_MODE_NAMES: [&str; 2] = ["Granular", "Delay"];

/// Whether Beads' grains or delay go by themselves or follow the song's steps, as the module does
/// with a clock in its `SEED` input.
pub(crate) const CLOCK_NAMES: [&str; 2] = ["Free", "Steps"];

/// Beads' four qualities, the module's, from the cleanest to the dirtiest, each holding a longer
/// recording than the one before: 16 bits at 48 kHz, 12 bits at 32 kHz, 12 bits at 24 kHz, and
/// 8-bit mu-law at 24 kHz with wow and flutter. The manual names the last three.
pub const BEADS_QUALITY_NAMES: [&str; 4] =
    ["Hi-fi", "Cold digital", "Sunny tape", "Scorched cassette"];

/// Beads', in the order the slot keeps them: its mode, which the title shows, the module's four
/// big knobs, its density, whether that follows the steps, the randomness its attenuverters
/// give, its freeze button, its feedback, dry/wet and reverb knobs, its quality, which the title
/// shows too, then the gain every slot has.
pub(crate) const BEADS_PARAMS: [Entry; 14] = [
    Entry {
        name: "Mode",
        key: "mode",
        // Choosing a mode chooses a sound, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &BEADS_MODE_NAMES,
            wraps: true,
        },
        default: 0,
        place: Place::Type,
    },
    hex_param("Time", "time", "TIME", Beads::DEFAULT.time),
    hex_param("Size", "size", "SIZE", Beads::DEFAULT.size).units(Units::GrainLength),
    hex_param("Shape", "shape", "SHAP", Beads::DEFAULT.shape),
    Entry {
        default: Beads::DEFAULT.pitch,
        ..PITCH_PARAMS[0]
    },
    hex_param("Density", "density", "DENS", Beads::DEFAULT.density),
    Entry {
        name: "Clock",
        key: "clock",
        label: Some("CLK"),
        kind: Kind::Stepped {
            choices: &CLOCK_NAMES,
            wraps: true,
        },
        default: Beads::DEFAULT.clock,
        place: Place::Own,
    },
    hex_param("Random", "random", "RAND", Beads::DEFAULT.random),
    Entry {
        name: "Freeze",
        key: "freeze",
        label: Some("FRZ"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: true,
        },
        default: Beads::DEFAULT.freeze,
        place: Place::Own,
    },
    hex_param("Feedback", "feedback", "FDBK", Beads::DEFAULT.feedback),
    Entry {
        name: "Blend",
        key: "blend",
        label: Some("BLND"),
        default: Beads::DEFAULT.blend,
        ..MIX_PARAM
    },
    hex_param("Reverb", "reverb", "VERB", Beads::DEFAULT.reverb),
    Entry {
        name: "Quality",
        key: "quality",
        // A new quality starts a new recording, so steps don't lock it.
        label: None,
        kind: Kind::Stepped {
            choices: &BEADS_QUALITY_NAMES,
            wraps: true,
        },
        default: Beads::DEFAULT.quality,
        place: Place::Title,
    },
    GAIN_PARAM,
];

/// Where its settings sit on the FX page. The module's big knobs; its pitch, its attenuverters' randomness and its freeze
/// button; its density, the clock it follows and its feedback; then its reverb over what comes
/// out. Its mode and quality are in the title.
pub(super) const BEADS_GRID: Columns = &[
    &["time", "size", "shape"],
    &["pitch", "random", "freeze"],
    &["density", "clock", "feedback"],
    &["reverb", "blend", "gain"],
];

/// Beads' settings: Mutable Instruments' granular texture synthesizer as the module plays, with its
/// granular and delay modes, its knobs, its buttons and its qualities. One a file doesn't have
/// starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Beads {
    pub mode: BeadsMode,
    /// Where in the recording grains start, from what's just come in at `00` to the oldest at
    /// `FF`; in `Delay`, how many times the delay `Density` sets the taps are.
    pub time: u8,
    /// How long each grain is, and below a third whether it plays backwards, as `Grains`' `Size`.
    /// In `Delay`, the taps' tone: a low-pass below `80`, a high-pass above, open at `80`.
    pub size: u8,
    /// Each grain's envelope, from a square through a decay and a bell to a reversed decay; in
    /// `Delay`, an envelope on each repeat, none at `00`.
    pub shape: u8,
    /// How far it shifts, as a place in the list from two octaves down to two up.
    pub pitch: u8,
    /// How often grains start: evenly below `80`, none in the middle, at random above. Following
    /// the steps, a division of them below `80` and a chance of each above. In `Delay`, how long
    /// it is.
    pub density: u8,
    /// Whether `Density` goes by itself, `0`, or follows the steps, `1`.
    pub clock: u8,
    /// How far each grain's time, size and shape stray at random, as the module's attenuverters do
    /// with nothing patched; in `Delay`, how far its time and shape wander.
    pub random: u8,
    /// Whether it stops recording, `1`, so it keeps playing what it holds.
    pub freeze: u8,
    /// How much of what comes out goes back in to be recorded again.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub blend: u8,
    /// How much of Beads' own reverb is on what comes out.
    pub reverb: u8,
    /// Its recording's quality, a place in [`BEADS_QUALITY_NAMES`].
    pub quality: u8,
    pub gain: u8,
}

impl Default for Beads {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Beads {
    /// As it starts: grains a quarter of the way back, about 150 ms long in a bell, no shift,
    /// about seven a second at random, nothing straying, recording, no feedback, half wet, a
    /// little reverb, in its cleanest quality.
    pub const DEFAULT: Self = Self {
        mode: BeadsMode::Granular,
        time: 0x40,
        size: 0xA8,
        shape: 0xAA,
        pitch: MOST_SHIFT,
        density: 0xCC,
        clock: 0,
        random: 0,
        freeze: 0,
        feedback: 0,
        blend: 0x80,
        reverb: 0x40,
        quality: 0,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        mode: BeadsMode,
        time,
        size,
        shape,
        pitch,
        density,
        clock,
        random,
        freeze,
        feedback,
        blend,
        reverb,
        quality,
        gain,
    }
}

/// How far a frequency shifter's `Shift` goes either way, and its `Spread` at most, in Hz, as far
/// as sst's and Surge's shifters go.
pub(crate) const MOST_SHIFT_HERTZ: f32 = 1000.0;
/// The longest delay a frequency shifter has in its loop, in ms.
pub const LONGEST_SHIFT_DELAY_MS: f32 = 1000.0;

/// A frequency shifter's shift in Hz, for a value on the `00` to `FF` scale: none at `80`, and
/// from there out to 1 kHz either way on a cube, so the slow shifts of a few Hz that beat and
/// spiral have room near the middle: `90` is about 2 Hz, `A0` 16 Hz and `C0` 128 Hz.
pub fn shift_hertz(value: f32) -> f32 {
    from_middle(value).powi(3) * MOST_SHIFT_HERTZ
}

/// How far apart a frequency shifter's two sides are shifted, in Hz, for a value on the `00` to
/// `FF` scale: none at `00`, and on a cube to 1 kHz at `FF`, so `40` is about 16 Hz.
pub fn spread_hertz(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0).powi(3) * MOST_SHIFT_HERTZ
}

/// The delay in a frequency shifter's loop in ms, for a value on the `00` to `FF` scale: none at
/// `00`, and on a cube to a second at `FF`, so `40` is about 16 ms and `80` 125 ms.
pub fn shift_delay_ms(value: f32) -> f32 {
    (value / 255.0).clamp(0.0, 1.0).powi(3) * LONGEST_SHIFT_DELAY_MS
}

/// A frequency shifter's, in the order the page shows them, as columns: the shift, how far its
/// sides are apart and how far it follows the note; then the loop's delay and feedback; then the
/// mix and gain.
pub(crate) const FREQ_SHIFT_PARAMS: [Entry; 7] = [
    hex_param("Shift", "shift", "SHFT", FreqShift::DEFAULT.shift)
        .units(Units::ShiftHertz)
        .centred(),
    hex_param("Spread", "spread", "SPRD", FreqShift::DEFAULT.spread).units(Units::SpreadHertz),
    hex_param("Track", "track", "TRK", FreqShift::DEFAULT.track),
    hex_param("Time", "time", "TIME", FreqShift::DEFAULT.time).units(Units::LoopTime),
    hex_param("Feedback", "feedback", "FDBK", FreqShift::DEFAULT.feedback),
    Entry {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A frequency shifter's settings: every frequency in the sound moved up or down by the same Hz,
/// with a delay and feedback in its loop for spirals, after sst's and Surge's. One a file doesn't
/// have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FreqShift {
    /// How far it shifts, from 1 kHz down at `00`, through none at `80`, to 1 kHz up at `FF`.
    pub shift: u8,
    /// How far apart the sides are shifted, the left further up and the right further down by
    /// half of it each, from none at `00`.
    pub spread: u8,
    /// How far the shift follows the track's note from middle C, from not at all at `00` to in
    /// proportion at `FF`, where an octave up shifts twice as far.
    pub track: u8,
    /// The delay in its loop, from none at `00` to a second at `FF`.
    pub time: u8,
    /// How much of what comes out goes round again, shifted again each time.
    pub feedback: u8,
    /// From `00`, all dry, to `FF`, all wet.
    pub mix: u8,
    pub gain: u8,
}

impl Default for FreqShift {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl FreqShift {
    /// As it starts: 16 Hz up, the sides together, no loop, and all wet.
    pub const DEFAULT: Self = Self {
        shift: 0xA0,
        spread: 0x00,
        track: 0x00,
        time: 0x00,
        feedback: 0x00,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        shift,
        spread,
        track,
        time,
        feedback,
        mix,
        gain,
    }
}
