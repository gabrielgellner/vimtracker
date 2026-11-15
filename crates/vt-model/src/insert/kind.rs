//! Each effect apart from its settings, described once: what its settings are stored under, its
//! name, what it does, every setting it keeps, and which of them it shows and where.

use std::ops::Index;

use super::{
    BEADS_GRID, BEADS_PARAMS, CHORUS_PARAMS, CLOUDS_GRID, CLOUDS_PARAMS, COMB_PARAMS,
    COMPRESSOR_PARAMS, COMPRESSOR_SHOWN, DELAY_GRID, DELAY_PARAMS, DISTORT_PARAMS, DISTORT_SHOWN,
    ECHO_GRID, ECHO_PARAMS, EQ_GRID, EQ_PARAMS, EXCITER_GRID, EXCITER_PARAMS, FLANGER_PARAMS,
    FREQ_SHIFT_PARAMS, GRAINS_PARAMS, Insert, LIMIT_PARAMS, LPG_PARAMS, MAX_KEPT_PARAMS,
    PHASER_PARAMS, PITCH_PARAMS, PLATE_PARAMS, Param, RESO_PARAMS, REVERB_PARAMS, ROTARY_GRID,
    ROTARY_PARAMS, SPACE_PARAMS, SPRING_GRID, SPRING_PARAMS, STUTTER_GRID, STUTTER_PARAMS,
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

crate::listed::listed!(EffectKind: key);

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
    /// Which of them it shows: all of them, or those of the mode its type is in.
    pub shown: Shown,
    /// Where they sit on the FX page when it shows them all, or nothing for the grid most effects
    /// have. A mode has its own.
    pub grid: Option<Columns>,
}

/// The FX page's columns, left to right, each from the top, by the keys of the settings in them.
/// A key the effect doesn't show in its mode leaves its place empty.
pub type Columns = &'static [&'static [&'static str]];

/// Which of its settings an effect shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// Every one it keeps, in the order it keeps them.
    All,
    /// Those of a mode: the value of its type, the setting at `type_at` among those it keeps, is
    /// the place of its mode in `modes`.
    ByType { type_at: u8, modes: &'static [Mode] },
}

/// What an effect shows in one of its types or modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mode {
    /// Its settings, by their places among those the effect keeps, in the order the page lists
    /// them.
    pub shown: &'static [u8],
    /// Where they sit on the FX page, or nothing for the grid most effects have.
    pub grid: Option<Columns>,
}

/// The place of the setting stored as `key` among `params`. It's for the tables, so one that isn't
/// there doesn't build.
pub(super) const fn place_of(params: &[Param], key: &str) -> u8 {
    let mut at = 0;
    while at < params.len() {
        if same(params[at].key.as_bytes(), key.as_bytes()) {
            return at as u8;
        }
        at += 1;
    }
    panic!("an effect has no setting of that key")
}

/// The places of the settings stored as `keys` among `params`, in order.
pub(super) const fn places_of<const N: usize>(params: &[Param], keys: [&str; N]) -> [u8; N] {
    let mut places = [0; N];
    let mut at = 0;
    while at < N {
        places[at] = place_of(params, keys[at]);
        at += 1;
    }
    places
}

const fn same(one: &[u8], other: &[u8]) -> bool {
    if one.len() != other.len() {
        return false;
    }
    let mut at = 0;
    while at < one.len() {
        if one[at] != other[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// Every place among an effect's settings, for one that shows them all.
const EVERY: [u8; MAX_KEPT_PARAMS] = {
    let mut every = [0; MAX_KEPT_PARAMS];
    let mut at = 0;
    while at < MAX_KEPT_PARAMS {
        every[at] = at as u8;
        at += 1;
    }
    every
};

/// The settings an effect shows now, in the order the page lists them: some or all of those it
/// keeps. It reads as a list of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    kept: &'static [Param],
    shown: &'static [u8],
}

impl Params {
    /// An empty slot's.
    pub const NONE: Self = Self {
        kept: &[],
        shown: &[],
    };

    /// All of `kept`.
    pub(super) const fn all(kept: &'static [Param]) -> Self {
        Self {
            kept,
            shown: EVERY.split_at(kept.len()).0,
        }
    }

    /// Those `mode` shows of `kept`.
    pub(super) const fn of(kept: &'static [Param], mode: &Mode) -> Self {
        Self {
            kept,
            shown: mode.shown,
        }
    }

    pub const fn len(self) -> usize {
        self.shown.len()
    }

    pub const fn is_empty(self) -> bool {
        self.shown.is_empty()
    }

    /// The one at `at` among those shown.
    pub fn get(self, at: usize) -> Option<&'static Param> {
        self.kept.get(self.kept_at(at)?)
    }

    /// The place among those the effect keeps of the one at `at` among those shown.
    pub fn kept_at(self, at: usize) -> Option<usize> {
        self.shown.get(at).map(|&kept| usize::from(kept))
    }

    /// Whether the one at `kept` among those the effect keeps is shown.
    pub fn shows(self, kept: usize) -> bool {
        self.shown.iter().any(|&at| usize::from(at) == kept)
    }

    pub fn iter(self) -> ParamsIter {
        ParamsIter {
            kept: self.kept,
            shown: self.shown.iter(),
        }
    }
}

impl Index<usize> for Params {
    type Output = Param;

    fn index(&self, at: usize) -> &Param {
        &self.kept[usize::from(self.shown[at])]
    }
}

impl IntoIterator for Params {
    type Item = &'static Param;
    type IntoIter = ParamsIter;

    fn into_iter(self) -> ParamsIter {
        self.iter()
    }
}

/// The settings an effect shows, one by one.
#[derive(Debug, Clone)]
pub struct ParamsIter {
    kept: &'static [Param],
    shown: std::slice::Iter<'static, u8>,
}

impl Iterator for ParamsIter {
    type Item = &'static Param;

    fn next(&mut self) -> Option<&'static Param> {
        let kept = self.kept;
        self.shown.next().map(|&at| &kept[usize::from(at)])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.shown.size_hint()
    }
}

impl DoubleEndedIterator for ParamsIter {
    fn next_back(&mut self) -> Option<&'static Param> {
        let kept = self.kept;
        self.shown.next_back().map(|&at| &kept[usize::from(at)])
    }
}

impl ExactSizeIterator for ParamsIter {}

/// Every effect's description, by [`EffectKind`].
pub const EFFECTS: [EffectSpec; EffectKind::COUNT] = [
    EffectSpec {
        key: "distort",
        name: "Distort",
        meaning: "Clips, folds or crushes the sound, or drives it as a pedal",
        params: &DISTORT_PARAMS,
        shown: DISTORT_SHOWN,
        grid: None,
    },
    EffectSpec {
        key: "delay",
        name: "Delay",
        meaning: "Repeats the sound, bouncing side to side",
        params: &DELAY_PARAMS,
        shown: Shown::All,
        grid: Some(DELAY_GRID),
    },
    EffectSpec {
        key: "reverb",
        name: "Reverb",
        meaning: "Puts the sound in a room",
        params: &REVERB_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "chorus",
        name: "Chorus",
        meaning: "Thickens it with drifting copies, an ensemble or a Juno's",
        params: &CHORUS_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "eq3",
        name: "EQ3",
        meaning: "Shapes its tone: a low and a high shelf, and a middle band",
        params: &EQ_PARAMS,
        shown: Shown::All,
        grid: Some(EQ_GRID),
    },
    EffectSpec {
        key: "comp",
        name: "Comp",
        meaning: "Evens out its level, or ducks it under another track",
        params: &COMPRESSOR_PARAMS,
        shown: COMPRESSOR_SHOWN,
        grid: None,
    },
    EffectSpec {
        key: "limit",
        name: "Limit",
        meaning: "Keeps its peaks under a ceiling, looking 2 ms ahead",
        params: &LIMIT_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "phaser",
        name: "Phaser",
        meaning: "Sweeps notches through it with all-passes",
        params: &PHASER_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "flanger",
        name: "Flanger",
        meaning: "Sweeps a comb through it with a short delay",
        params: &FLANGER_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "tape",
        name: "Tape",
        meaning: "Saturates, warms and wobbles it as tape does",
        params: &TAPE_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "plate",
        name: "Plate",
        meaning: "Puts the sound on a bright, dense plate",
        params: &PLATE_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "pitch",
        name: "Pitch",
        meaning: "Shifts its pitch up or down, keeping its time",
        params: &PITCH_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "grains",
        name: "Grains",
        meaning: "Records it and plays it back as a cloud of grains",
        params: &GRAINS_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "warps",
        name: "Warps",
        meaning: "Ring mods, folds or vocodes it with another track",
        params: &WARPS_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "lpg",
        name: "LPG",
        meaning: "Gates or filters it by its level, or another track's",
        params: &LPG_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "reso",
        name: "Reso",
        meaning: "Rings it like a bell or bar tuned to the note",
        params: &RESO_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "comb",
        name: "Comb",
        meaning: "Rings it through a comb or flanger tuned to the note",
        params: &COMB_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "vowel",
        name: "Vowel",
        meaning: "Makes it say a vowel, morphing from A to U",
        params: &VOWEL_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "freqshift",
        name: "Freq Shift",
        meaning: "Moves every frequency by the same Hz: bells and spirals",
        params: &FREQ_SHIFT_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "spring",
        name: "Spring",
        meaning: "Puts the sound in a spring tank, as a guitar amp has: drips and boings",
        params: &SPRING_PARAMS,
        shown: Shown::All,
        grid: Some(SPRING_GRID),
    },
    EffectSpec {
        key: "space",
        name: "Space",
        meaning: "Washes the sound into a huge, nearly endless pad",
        params: &SPACE_PARAMS,
        shown: Shown::All,
        grid: None,
    },
    EffectSpec {
        key: "rotary",
        name: "Rotary",
        meaning: "Spins it in a Leslie's cabinet, slow or fast, for organs and more",
        params: &ROTARY_PARAMS,
        shown: Shown::All,
        grid: Some(ROTARY_GRID),
    },
    EffectSpec {
        key: "exciter",
        name: "Exciter",
        meaning: "Adds air: harmonics made from the highs, more as they get louder",
        params: &EXCITER_PARAMS,
        shown: Shown::All,
        grid: Some(EXCITER_GRID),
    },
    EffectSpec {
        key: "stutter",
        name: "Stutter",
        meaning: "Repeats a piece of the sound when a step says: rolls and glitches",
        params: &STUTTER_PARAMS,
        shown: Shown::All,
        grid: Some(STUTTER_GRID),
    },
    EffectSpec {
        key: "clouds",
        name: "Clouds",
        meaning: "Mutable's Clouds: grains, stretching or a looping delay, with reverb",
        params: &CLOUDS_PARAMS,
        shown: Shown::All,
        grid: Some(CLOUDS_GRID),
    },
    EffectSpec {
        key: "beads",
        name: "Beads",
        meaning: "Mutable's Beads: grains or a delay on tape or digital, with reverb",
        params: &BEADS_PARAMS,
        shown: Shown::All,
        grid: Some(BEADS_GRID),
    },
    EffectSpec {
        key: "echo",
        name: "Space Echo",
        meaning: "A Roland RE-201's tape loop: three heads, wow, darkening repeats, springs",
        params: &ECHO_PARAMS,
        shown: Shown::All,
        grid: Some(ECHO_GRID),
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
