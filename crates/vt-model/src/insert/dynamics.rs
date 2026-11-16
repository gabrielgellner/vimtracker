//! The effects that shape a sound's level: a compressor and a limiter.

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;

use super::{
    Entry, GAIN_PARAM, Kind, MIX_PARAM, Mode, OFF_ON, Place, SOURCE_NAMES, Shown, Units, hex_param,
    place_of, places_of, source_track,
};

/// A compressor's character, each modelled on its own reference and tuned against it. `VCA`, `FET`
/// and `Diode` come after these, in this order, so a song's types keep their places.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressorType {
    /// Transparent and digital, with an exact ratio and knee, as Ableton's Compressor is.
    #[default]
    Clean,
    /// An SSL console's bus compressor, fitted to the one in Arturia's Bus FORCE: set ratios,
    /// attacks and releases, an automatic release, and its detector hearing what it's already
    /// turned down.
    Console,
    /// Ableton Live's Glue Compressor, another model of the same SSL, fitted to it: its own
    /// switches, a knee that softens as the ratio falls, and a limit on how far it turns down.
    Glue,
}

impl CompressorType {
    /// Every type, in the order `+` goes through them.
    pub const ALL: [Self; 3] = [Self::Clean, Self::Console, Self::Glue];
}

const COMPRESSOR_TYPE_NAMES: [&str; 3] = ["Clean", "Console", "Glue"];

/// The console compressor's ratios, as its switch has them.
pub const CONSOLE_RATIOS: [f32; 3] = [2.0, 4.0, 10.0];
const CONSOLE_RATIO_NAMES: [&str; 3] = ["2:1", "4:1", "10:1"];
const CONSOLE_ATTACK_NAMES: [&str; 6] = ["0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms"];
/// The console compressor's releases in ms, as its switch has them, before `Auto`.
pub const CONSOLE_RELEASES_MS: [f32; 4] = [100.0, 300.0, 600.0, 1200.0];
/// Where `Auto` is among the console compressor's releases: after the set times.
pub const CONSOLE_AUTO_RELEASE: u8 = CONSOLE_RELEASES_MS.len() as u8;
const CONSOLE_RELEASE_NAMES: [&str; 5] = ["0.1 s", "0.3 s", "0.6 s", "1.2 s", "Auto"];

/// Glue's ratios, attacks and releases, as Live shows its switches. Its attacks are in ms, and
/// its releases are named in seconds, as the SSL's are, though they don't take that long: what
/// each takes is the engine's.
pub const GLUE_RATIOS: [f32; 3] = [2.0, 4.0, 10.0];
const GLUE_RATIO_NAMES: [&str; 3] = ["2:1", "4:1", "10:1"];
pub(super) const GLUE_ATTACK_NAMES: [&str; 7] = [
    "0.01 ms", "0.1 ms", "0.3 ms", "1 ms", "3 ms", "10 ms", "30 ms",
];
const GLUE_RELEASE_NAMES: [&str; 7] =
    ["0.1 s", "0.2 s", "0.4 s", "0.6 s", "0.8 s", "1.2 s", "Auto"];
/// Where `Auto` is among Glue's releases: after the set ones.
pub const GLUE_AUTO_RELEASE: u8 = GLUE_RELEASE_NAMES.len() as u8 - 1;
/// The most Glue's `Range` lets it turn down, in dB, where it starts, as Live's does.
pub(crate) const MOST_RANGE_DB: f32 = 70.0;

/// Glue's `Range` in dB, for a value on the `00` to `FF` scale: from 0 dB, where it doesn't turn
/// down at all, to [`MOST_RANGE_DB`].
pub fn range_db(value: f32) -> f32 {
    value / 255.0 * MOST_RANGE_DB
}

/// A compressor's threshold in dB, for a value on the `00` to `FF` scale: from -60 dB to 0 dB.
pub fn threshold_db(value: f32) -> f32 {
    value / 255.0 * 60.0 - 60.0
}

/// A compressor's ratio for a value on the `00` to `FF` scale: 1:1 at `00`, rising faster as it
/// goes, to 20:1 just below the top, and as hard as a limiter at `FF`.
pub fn ratio(value: f32) -> f32 {
    if value >= 255.0 {
        f32::INFINITY
    } else {
        1.0 + (value / 255.0).powi(2) * 19.0
    }
}

/// A compressor's attack in ms: from 0.1 ms at `00` to 100 ms at `FF`, evenly in ratio.
pub fn attack_ms(value: f32) -> f32 {
    0.1 * 1000_f32.powf(value / 255.0)
}

/// A compressor's release in ms: from 10 ms at `00` to a second at `FF`, evenly in ratio.
pub fn release_ms(value: f32) -> f32 {
    10.0 * 100_f32.powf(value / 255.0)
}

const COMPRESSOR_TYPE_PARAM: Entry = Entry {
    name: "Type",
    key: "type",
    // Choosing a type chooses a sound, so steps don't lock it.
    label: None,
    kind: Kind::Stepped {
        choices: &COMPRESSOR_TYPE_NAMES,
        wraps: true,
    },
    default: 0,
    place: Place::Type,
};
const SOURCE_PARAM: Entry = Entry {
    name: "Source",
    key: "source",
    // What it hears is part of how it's set up, so steps don't change it.
    label: None,
    kind: Kind::Stepped {
        choices: &SOURCE_NAMES,
        wraps: false,
    },
    default: 0,
    place: Place::Own,
};
const THRESHOLD_PARAM: Entry = hex_param(
    "Threshold",
    "threshold",
    "THRS",
    Compressor::DEFAULT.threshold,
)
.units(Units::Threshold);
const RATIO_PARAM: Entry =
    hex_param("Ratio", "ratio", "RTIO", Compressor::DEFAULT.ratio).units(Units::Ratio);
const ATTACK_PARAM: Entry =
    hex_param("Attack", "attack", "ATK", Compressor::DEFAULT.attack).units(Units::Attack);
const RELEASE_PARAM: Entry =
    hex_param("Release", "release", "REL", Compressor::DEFAULT.release).units(Units::Release);
const AUTO_GAIN_PARAM: Entry = Entry {
    name: "Auto",
    key: "auto",
    label: Some("AUTO"),
    kind: Kind::Stepped {
        choices: &OFF_ON,
        wraps: true,
    },
    default: 0,
    place: Place::Own,
};
/// Every type's makeup, from 0 dB at `00` to [`MOST_MAKEUP_DB`] at `FF`, on top of what `Auto`
/// gives back.
const MAKEUP_PARAM: Entry =
    hex_param("Makeup", "makeup", "MKUP", Compressor::DEFAULT.makeup).units(Units::Makeup);
/// The bus type's switches, shown by the same names as the clean type's knobs. Their labels are
/// their own, as a slot's labels name one setting each.
const CONSOLE_RATIO_PARAM: Entry = Entry {
    name: "Ratio",
    key: "consoleratio",
    label: Some("CRTO"),
    kind: Kind::Stepped {
        choices: &CONSOLE_RATIO_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.console_ratio,
    place: Place::Own,
};
const CONSOLE_ATTACK_PARAM: Entry = Entry {
    name: "Attack",
    key: "consoleattack",
    label: Some("CATK"),
    kind: Kind::Stepped {
        choices: &CONSOLE_ATTACK_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.console_attack,
    place: Place::Own,
};
const CONSOLE_RELEASE_PARAM: Entry = Entry {
    name: "Release",
    key: "consolerelease",
    label: Some("CREL"),
    kind: Kind::Stepped {
        choices: &CONSOLE_RELEASE_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.console_release,
    place: Place::Own,
};
/// Glue's switches, shown by the same names again, with labels of their own.
const GLUE_RATIO_PARAM: Entry = Entry {
    name: "Ratio",
    key: "glueratio",
    label: Some("GRTO"),
    kind: Kind::Stepped {
        choices: &GLUE_RATIO_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.glue_ratio,
    place: Place::Own,
};
const GLUE_ATTACK_PARAM: Entry = Entry {
    name: "Attack",
    key: "glueattack",
    label: Some("GATK"),
    kind: Kind::Stepped {
        choices: &GLUE_ATTACK_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.glue_attack,
    place: Place::Own,
};
const GLUE_RELEASE_PARAM: Entry = Entry {
    name: "Release",
    key: "gluerelease",
    label: Some("GREL"),
    kind: Kind::Stepped {
        choices: &GLUE_RELEASE_NAMES,
        wraps: false,
    },
    default: Compressor::DEFAULT.glue_release,
    place: Place::Own,
};
/// Glue's `Range`, the most it turns down, from none at `00` to [`MOST_RANGE_DB`] at `FF`.
const RANGE_PARAM: Entry =
    hex_param("Range", "range", "RNGE", Compressor::DEFAULT.range).units(Units::Range);
/// Glue's soft clip at the very end, after its makeup and its mix, as Live's `Peak Clip In` is:
/// off, as Live loads it, or on.
const PEAK_CLIP_PARAM: Entry = Entry {
    name: "Peak Clip",
    key: "peakclip",
    label: Some("PCLP"),
    kind: Kind::Stepped {
        choices: &OFF_ON,
        wraps: true,
    },
    default: Compressor::DEFAULT.peak_clip,
    place: Place::Own,
};
/// A compressor's whole mix, which it starts at, for compressing in parallel when turned down.
const COMPRESSOR_MIX_PARAM: Entry = Entry {
    default: 0xFF,
    ..MIX_PARAM
};
/// Every setting a compressor has in any of its types, in the order songs number them. Each type
/// shows only its own.
pub(crate) const COMPRESSOR_PARAMS: [Entry; 17] = [
    COMPRESSOR_TYPE_PARAM,
    SOURCE_PARAM,
    THRESHOLD_PARAM,
    RATIO_PARAM,
    ATTACK_PARAM,
    RELEASE_PARAM,
    CONSOLE_RATIO_PARAM,
    CONSOLE_ATTACK_PARAM,
    CONSOLE_RELEASE_PARAM,
    GLUE_RATIO_PARAM,
    GLUE_ATTACK_PARAM,
    GLUE_RELEASE_PARAM,
    RANGE_PARAM,
    AUTO_GAIN_PARAM,
    COMPRESSOR_MIX_PARAM,
    MAKEUP_PARAM,
    PEAK_CLIP_PARAM,
];
/// `Clean`'s, in the order the page lists them: the type over what it hears, then the threshold
/// over the ratio, the attack over the release, and Auto over the mix, with the makeup last. On
/// the page its detector, its times and what comes out each have a column.
const CLEAN: Mode = Mode {
    shown: &places_of(
        &COMPRESSOR_PARAMS,
        [
            "type",
            "source",
            "threshold",
            "ratio",
            "attack",
            "release",
            "auto",
            "mix",
            "makeup",
        ],
    ),
    grid: Some(&[
        &["threshold", "ratio"],
        &["attack", "release", "source"],
        &["makeup", "auto"],
        &["mix"],
    ]),
};
/// `Console`'s: its switches in place of the clean type's knobs, and the rest as `Clean` has them.
const CONSOLE: Mode = Mode {
    shown: &places_of(
        &COMPRESSOR_PARAMS,
        [
            "type",
            "source",
            "threshold",
            "consoleratio",
            "consoleattack",
            "consolerelease",
            "auto",
            "mix",
            "makeup",
        ],
    ),
    grid: Some(&[
        &["threshold", "consoleratio"],
        &["consoleattack", "consolerelease", "source"],
        &["makeup", "auto"],
        &["mix"],
    ]),
};
/// `Glue`'s: its switches as `Console` has its own, and its `Range` over the makeup, so the rest
/// keep the places they have in the other types, then its `Peak Clip` in a column of its own, last
/// as it's last in what Glue does.
const GLUE: Mode = Mode {
    shown: &places_of(
        &COMPRESSOR_PARAMS,
        [
            "type",
            "source",
            "threshold",
            "glueratio",
            "glueattack",
            "gluerelease",
            "auto",
            "mix",
            "range",
            "makeup",
            "peakclip",
        ],
    ),
    grid: Some(&[
        &["threshold", "glueratio", "range"],
        &["glueattack", "gluerelease", "source"],
        &["makeup", "auto"],
        &["mix", "peakclip"],
    ]),
};
/// Each type's, in the order of [`CompressorType::ALL`].
pub(super) const COMPRESSOR_SHOWN: Shown = Shown::ByType {
    type_at: place_of(&COMPRESSOR_PARAMS, "type"),
    modes: &[CLEAN, CONSOLE, GLUE],
};

/// The most a compressor's `Makeup` turns up what it's turned down, in dB, as a limiter's `Input`
/// turns up what goes in.
pub const MOST_MAKEUP_DB: f32 = 24.0;

/// A compressor's makeup in dB, for a value on the `00` to `FF` scale: from 0 dB to +24 dB.
pub fn makeup_db(value: f32) -> f32 {
    value / 255.0 * MOST_MAKEUP_DB
}

/// A compressor's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Compressor {
    /// Its character. Each type keeps its own settings, so changing type and back finds them as
    /// they were; those that mean the same in each are shared.
    pub kind: CompressorType,
    pub threshold: u8,
    /// `Clean`'s ratio, attack and release, on the `00` to `FF` scale.
    pub ratio: u8,
    pub attack: u8,
    pub release: u8,
    /// `Console`'s ratio, attack and release, each a place among its switch's.
    pub console_ratio: u8,
    pub console_attack: u8,
    pub console_release: u8,
    /// `Glue`'s, the same, and the most it turns down, from 0 dB at `00` to 70 dB at `FF`, as
    /// [`range_db`] has it.
    pub glue_ratio: u8,
    pub glue_attack: u8,
    pub glue_release: u8,
    pub range: u8,
    /// `1` for `Glue` to soft clip what comes out, after its makeup and its mix, to -0.5 dBFS.
    pub peak_clip: u8,
    /// What it hears: `0` for its own input, a track's number, or a return after the tracks.
    pub source: u8,
    /// `1` to make up by itself how much quieter it comes out than it went in, each measured over
    /// about a second, so it comes out as loud as it went in. Off for ducking, which shouldn't be louder unducked.
    pub auto: u8,
    /// From `00`, all dry, to `FF`, all compressed, for compressing in parallel.
    pub mix: u8,
    /// How much it turns up what it's turned down, before the mix and on top of `Auto`: from
    /// 0 dB at `00` to +24 dB at `FF`, as [`makeup_db`] has it.
    pub makeup: u8,
}

impl Default for Compressor {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Compressor {
    /// As it starts: `Clean`, at -15 dB, about 3.7:1, a quick attack and a 100 ms release,
    /// hearing itself. `Console` starts at 4:1 with a 10 ms attack and `Auto` release, the G-bus's
    /// usual place on a drum bus, and `Glue` where Live loads it: 4:1, a 1 ms attack, a 0.6 s
    /// release, all of its range and no clip.
    pub const DEFAULT: Self = Self {
        kind: CompressorType::Clean,
        threshold: 0xC0,
        ratio: 0x60,
        attack: 0x55,
        release: 0x80,
        console_ratio: 1,
        console_attack: 4,
        console_release: CONSOLE_AUTO_RELEASE,
        glue_ratio: 1,
        glue_attack: 3,
        glue_release: 3,
        range: 0xFF,
        peak_clip: 0,
        source: 0,
        auto: 0,
        mix: 0xFF,
        makeup: 0,
    };

    /// The track it hears, by its place among the tracks, if not its own input. The returns come
    /// after the sound tracks, at `sounds` and the one after.
    pub fn source_track(self, sounds: usize) -> Option<usize> {
        source_track(self.source, sounds)
    }

    effect_settings! {
        kind: CompressorType,
        source,
        threshold,
        ratio,
        attack,
        release,
        console_ratio,
        console_attack,
        console_release,
        glue_ratio,
        glue_attack,
        glue_release,
        range,
        auto,
        mix,
        makeup,
        peak_clip,
    }
}

/// How much a limiter's `Input` turns up what goes in at most, in dB.
pub(crate) const MOST_INPUT_DB: f32 = 24.0;
/// How far below 0 dB a limiter's `Ceiling` goes.
pub(crate) const LOWEST_CEILING_DB: f32 = -24.0;

/// A limiter's drive in dB, for a value on the `00` to `FF` scale: from 0 dB to +24 dB.
pub fn input_db(value: f32) -> f32 {
    value / 255.0 * MOST_INPUT_DB
}

/// A limiter's ceiling in dB, for a value on the `00` to `FF` scale: from -24 dB to 0 dB.
pub fn ceiling_db(value: f32) -> f32 {
    LOWEST_CEILING_DB * (1.0 - value / 255.0)
}

/// A limiter's release in ms: from 1 ms at `00` to a second at `FF`, evenly in ratio, so `AA` is
/// 100 ms.
pub fn limit_release_ms(value: f32) -> f32 {
    1000_f32.powf(value / 255.0)
}

/// A limiter's, in the order the page shows them.
pub(crate) const LIMIT_PARAMS: [Entry; 5] = [
    hex_param("Input", "input", "INPT", Limiter::DEFAULT.input).units(Units::Input),
    hex_param("Ceiling", "ceiling", "CEIL", Limiter::DEFAULT.ceiling).units(Units::Ceiling),
    hex_param("Release", "release", "REL", Limiter::DEFAULT.release).units(Units::LimitRelease),
    Entry {
        default: 0xFF,
        ..MIX_PARAM
    },
    GAIN_PARAM,
];

/// A look-ahead limiter's settings. One a file doesn't have starts as new.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limiter {
    /// How much it turns up what goes in, from 0 dB at `00` to +24 dB at `FF`, to push it harder.
    pub input: u8,
    /// The most that comes out, from -24 dB at `00` to 0 dB at `FF`.
    pub ceiling: u8,
    /// How fast it lets go once a peak has passed.
    pub release: u8,
    /// From `00`, all dry, to `FF`, all limited. The dry sound is as late as the limited one.
    pub mix: u8,
    /// What comes out, on the level scale.
    pub gain: u8,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Limiter {
    /// As it starts: nothing added going in, a ceiling just under 0 dB at about -0.3 dB, and a
    /// 100 ms release.
    pub const DEFAULT: Self = Self {
        input: 0x00,
        ceiling: 0xFC,
        release: 0xAA,
        mix: 0xFF,
        gain: UNITY_LEVEL,
    };

    effect_settings! {
        input,
        ceiling,
        release,
        mix,
        gain,
    }
}
