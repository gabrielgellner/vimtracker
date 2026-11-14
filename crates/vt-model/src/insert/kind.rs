//! Each effect apart from its settings, described once: what its settings are stored under, its
//! name, what it does and every setting it keeps.

use super::{
    BEADS_PARAMS, CHORUS_PARAMS, CLOUDS_PARAMS, COMB_PARAMS, COMPRESSOR_PARAMS, DELAY_PARAMS,
    DISTORT_PARAMS, ECHO_PARAMS, EQ_PARAMS, EXCITER_PARAMS, FLANGER_PARAMS, FREQ_SHIFT_PARAMS,
    GRAINS_PARAMS, Insert, LIMIT_PARAMS, LPG_PARAMS, PHASER_PARAMS, PITCH_PARAMS, PLATE_PARAMS,
    Param, RESO_PARAMS, REVERB_PARAMS, ROTARY_PARAMS, SPACE_PARAMS, SPRING_PARAMS, STUTTER_PARAMS,
    TAPE_PARAMS, VOWEL_PARAMS, WARPS_PARAMS,
};

/// An effect a slot can hold, apart from its settings: a dense index for tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum EffectKind {
    Distort,
    Delay,
    Reverb,
    Chorus,
    Eq,
    Compressor,
    Limit,
    Phaser,
    Flanger,
    Tape,
    Plate,
    Pitch,
    Grains,
    Warps,
    Lpg,
    Reso,
    Comb,
    Vowel,
    FreqShift,
    Spring,
    Space,
    Rotary,
    Exciter,
    Stutter,
    Clouds,
    Beads,
    Echo,
}

impl EffectKind {
    /// In the order [`SettingId`](crate::settings::SettingId) numbers their settings, which is
    /// [`Insert::ALL`]'s without `Empty`. Never reordered: a track's saved `settings` map is in
    /// this order.
    pub const ALL: [Self; 27] = [
        Self::Distort,
        Self::Delay,
        Self::Reverb,
        Self::Chorus,
        Self::Eq,
        Self::Compressor,
        Self::Limit,
        Self::Phaser,
        Self::Flanger,
        Self::Tape,
        Self::Plate,
        Self::Pitch,
        Self::Grains,
        Self::Warps,
        Self::Lpg,
        Self::Reso,
        Self::Comb,
        Self::Vowel,
        Self::FreqShift,
        Self::Spring,
        Self::Space,
        Self::Rotary,
        Self::Exciter,
        Self::Stutter,
        Self::Clouds,
        Self::Beads,
        Self::Echo,
    ];
    pub const COUNT: usize = Self::ALL.len();

    /// Its description.
    #[inline]
    pub const fn spec(self) -> &'static EffectSpec {
        &EFFECTS[self as usize]
    }

    /// Every setting it has in any mode, in the order its slot keeps them. Distort's modes and
    /// Comp's types each show only their own.
    pub const fn params(self) -> &'static [Param] {
        self.spec().params
    }

    /// What its settings are stored under, as `2.delay.feedback`.
    pub const fn key(self) -> &'static str {
        self.spec().key
    }

    pub const fn name(self) -> &'static str {
        self.spec().name
    }
}

/// One effect, described once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSpec {
    /// What its settings are stored under, which never changes.
    pub key: &'static str,
    /// On screen and in lists.
    pub name: &'static str,
    /// What it does, for a list to choose from.
    pub meaning: &'static str,
    /// Every setting it keeps, in any mode, in the order its slot keeps them.
    pub params: &'static [Param],
}

/// Every effect's description, by [`EffectKind`].
pub const EFFECTS: [EffectSpec; EffectKind::COUNT] = [
    EffectSpec {
        key: "distort",
        name: "Distort",
        meaning: "Clips, folds or crushes the sound, or drives it as a pedal",
        params: &DISTORT_PARAMS,
    },
    EffectSpec {
        key: "delay",
        name: "Delay",
        meaning: "Repeats the sound, bouncing side to side",
        params: &DELAY_PARAMS,
    },
    EffectSpec {
        key: "reverb",
        name: "Reverb",
        meaning: "Puts the sound in a room",
        params: &REVERB_PARAMS,
    },
    EffectSpec {
        key: "chorus",
        name: "Chorus",
        meaning: "Thickens it with drifting copies, an ensemble or a Juno's",
        params: &CHORUS_PARAMS,
    },
    EffectSpec {
        key: "eq3",
        name: "EQ3",
        meaning: "Shapes its tone: a low and a high shelf, and a middle band",
        params: &EQ_PARAMS,
    },
    EffectSpec {
        key: "comp",
        name: "Comp",
        meaning: "Evens out its level, or ducks it under another track",
        params: &COMPRESSOR_PARAMS,
    },
    EffectSpec {
        key: "limit",
        name: "Limit",
        meaning: "Keeps its peaks under a ceiling, looking 2 ms ahead",
        params: &LIMIT_PARAMS,
    },
    EffectSpec {
        key: "phaser",
        name: "Phaser",
        meaning: "Sweeps notches through it with all-passes",
        params: &PHASER_PARAMS,
    },
    EffectSpec {
        key: "flanger",
        name: "Flanger",
        meaning: "Sweeps a comb through it with a short delay",
        params: &FLANGER_PARAMS,
    },
    EffectSpec {
        key: "tape",
        name: "Tape",
        meaning: "Saturates, warms and wobbles it as tape does",
        params: &TAPE_PARAMS,
    },
    EffectSpec {
        key: "plate",
        name: "Plate",
        meaning: "Puts the sound on a bright, dense plate",
        params: &PLATE_PARAMS,
    },
    EffectSpec {
        key: "pitch",
        name: "Pitch",
        meaning: "Shifts its pitch up or down, keeping its time",
        params: &PITCH_PARAMS,
    },
    EffectSpec {
        key: "grains",
        name: "Grains",
        meaning: "Records it and plays it back as a cloud of grains",
        params: &GRAINS_PARAMS,
    },
    EffectSpec {
        key: "warps",
        name: "Warps",
        meaning: "Ring mods, folds or vocodes it with another track",
        params: &WARPS_PARAMS,
    },
    EffectSpec {
        key: "lpg",
        name: "LPG",
        meaning: "Gates or filters it by its level, or another track's",
        params: &LPG_PARAMS,
    },
    EffectSpec {
        key: "reso",
        name: "Reso",
        meaning: "Rings it like a bell or bar tuned to the note",
        params: &RESO_PARAMS,
    },
    EffectSpec {
        key: "comb",
        name: "Comb",
        meaning: "Rings it through a comb or flanger tuned to the note",
        params: &COMB_PARAMS,
    },
    EffectSpec {
        key: "vowel",
        name: "Vowel",
        meaning: "Makes it say a vowel, morphing from A to U",
        params: &VOWEL_PARAMS,
    },
    EffectSpec {
        key: "freqshift",
        name: "Freq Shift",
        meaning: "Moves every frequency by the same Hz: bells and spirals",
        params: &FREQ_SHIFT_PARAMS,
    },
    EffectSpec {
        key: "spring",
        name: "Spring",
        meaning: "Puts the sound in a spring tank, as a guitar amp has: drips and boings",
        params: &SPRING_PARAMS,
    },
    EffectSpec {
        key: "space",
        name: "Space",
        meaning: "Washes the sound into a huge, nearly endless pad",
        params: &SPACE_PARAMS,
    },
    EffectSpec {
        key: "rotary",
        name: "Rotary",
        meaning: "Spins it in a Leslie's cabinet, slow or fast, for organs and more",
        params: &ROTARY_PARAMS,
    },
    EffectSpec {
        key: "exciter",
        name: "Exciter",
        meaning: "Adds air: harmonics made from the highs, more as they get louder",
        params: &EXCITER_PARAMS,
    },
    EffectSpec {
        key: "stutter",
        name: "Stutter",
        meaning: "Repeats a piece of the sound when a step says: rolls and glitches",
        params: &STUTTER_PARAMS,
    },
    EffectSpec {
        key: "clouds",
        name: "Clouds",
        meaning: "Mutable's Clouds: grains, stretching or a looping delay, with reverb",
        params: &CLOUDS_PARAMS,
    },
    EffectSpec {
        key: "beads",
        name: "Beads",
        meaning: "Mutable's Beads: grains or a delay on tape or digital, with reverb",
        params: &BEADS_PARAMS,
    },
    EffectSpec {
        key: "echo",
        name: "Space Echo",
        meaning: "A Roland RE-201's tape loop: three heads, wow, darkening repeats, springs",
        params: &ECHO_PARAMS,
    },
];

impl Insert {
    /// The effect it holds, if any. The only match on every effect in the model besides serde's
    /// and the settings' own.
    pub const fn kind(self) -> Option<EffectKind> {
        Some(match self {
            Self::Empty => return None,
            Self::Distort(_) => EffectKind::Distort,
            Self::Delay(_) => EffectKind::Delay,
            Self::Reverb(_) => EffectKind::Reverb,
            Self::Chorus(_) => EffectKind::Chorus,
            Self::Eq(_) => EffectKind::Eq,
            Self::Compressor(_) => EffectKind::Compressor,
            Self::Limit(_) => EffectKind::Limit,
            Self::Phaser(_) => EffectKind::Phaser,
            Self::Flanger(_) => EffectKind::Flanger,
            Self::Tape(_) => EffectKind::Tape,
            Self::Plate(_) => EffectKind::Plate,
            Self::Pitch(_) => EffectKind::Pitch,
            Self::Grains(_) => EffectKind::Grains,
            Self::Warps(_) => EffectKind::Warps,
            Self::Lpg(_) => EffectKind::Lpg,
            Self::Reso(_) => EffectKind::Reso,
            Self::Comb(_) => EffectKind::Comb,
            Self::Vowel(_) => EffectKind::Vowel,
            Self::FreqShift(_) => EffectKind::FreqShift,
            Self::Spring(_) => EffectKind::Spring,
            Self::Space(_) => EffectKind::Space,
            Self::Rotary(_) => EffectKind::Rotary,
            Self::Exciter(_) => EffectKind::Exciter,
            Self::Stutter(_) => EffectKind::Stutter,
            Self::Clouds(_) => EffectKind::Clouds,
            Self::Beads(_) => EffectKind::Beads,
            Self::Echo(_) => EffectKind::Echo,
        })
    }
}
