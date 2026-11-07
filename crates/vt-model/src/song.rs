//! The song and everything it refers to by ID.

use std::collections::BTreeMap;
use std::fmt;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::constants::{
    BAR_STEPS, BEAT_STEPS, DB_PER_LEVEL, DEFAULT_MILLIBPM, DEFAULT_PHRASE_STEPS, FIXED_TRACKS,
    FORMAT_VERSION, FX_SLOTS, MAX_LEVEL, MAX_PHRASE_STEPS, TRACK_ENVELOPES, UNITY_LEVEL,
};
use crate::envelope::{Envelope, default_envelopes};
use crate::insert::{INSERTS, Insert};
use crate::lfo::{LFOS_PER_TRACK, Lfo};
use crate::scale::Scale;
use crate::selection::Extent;
use crate::step::{FxCommand, FxSlot, Step};

/// The gain of a level value, on an even scale in dB like a fader's. `00` is silent, nothing goes
/// past `MAX_LEVEL`, and the value can be between steps, as an LFO moves it.
pub fn level_gain(level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    let level = level.min(f32::from(MAX_LEVEL));
    10.0_f32.powf((level - f32::from(UNITY_LEVEL)) * DB_PER_LEVEL / 20.0)
}

/// A track's filter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterType {
    #[default]
    LowPass,
    HighPass,
    BandPass,
    Notch,
    /// A saturating four-pole ladder, which self-oscillates and costs more.
    Ladder,
}

impl FilterType {
    /// Every type, in the order a list shows them.
    pub const ALL: [Self; 5] = [
        Self::LowPass,
        Self::HighPass,
        Self::BandPass,
        Self::Notch,
        Self::Ladder,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::LowPass => "LP",
            Self::HighPass => "HP",
            Self::BandPass => "BP",
            Self::Notch => "Notch",
            Self::Ladder => "Ladder",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::LowPass => "Low-pass, clean, 12 dB",
            Self::HighPass => "High-pass, clean, 12 dB",
            Self::BandPass => "Band-pass, clean",
            Self::Notch => "Notch, clean",
            Self::Ladder => "Saturating low-pass, 24 dB, self-oscillates",
        }
    }
}

/// What a track is. Only `Sound` tracks have a machine, and the rest are fixed, after them, in this
/// order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Sound,
    /// A return, which other tracks send to.
    ReturnA,
    ReturnB,
    /// What everything ends up in.
    Master,
}

impl TrackKind {
    /// The fixed tracks, in order.
    pub const FIXED: [Self; FIXED_TRACKS] = [Self::ReturnA, Self::ReturnB, Self::Master];

    /// The short name of a fixed track.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Sound => "",
            Self::ReturnA => "A",
            Self::ReturnB => "B",
            Self::Master => "M",
        }
    }

    /// What a fixed track is called in full.
    pub fn title(self) -> &'static str {
        match self {
            Self::Sound => "",
            Self::ReturnA => "Return A",
            Self::ReturnB => "Return B",
            Self::Master => "Master",
        }
    }
}

/// One voice's lane through the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Whether it plays sounds, or is one of the fixed tracks at the end.
    #[serde(default)]
    pub kind: TrackKind,
    /// Two characters, or empty for none.
    #[serde(default)]
    pub name: String,
    /// What makes its sound.
    #[serde(default)]
    pub machine: Machine,
    /// Which synth it plays, on the Plaits machine.
    #[serde(default)]
    pub synth_voice: SynthVoice,
    /// What it plays, relative to the project folder.
    #[serde(default)]
    pub sample: Option<String>,
    /// Where held `HAR` starts: a synth's first setting.
    #[serde(default = "centre")]
    pub harmonics: u8,
    /// Where held `TIM` starts: a synth's second setting.
    #[serde(default)]
    pub timbre: u8,
    /// Where held `MOR` starts: a synth's third setting.
    #[serde(default = "centre")]
    pub morph: u8,
    pub mute: bool,
    pub solo: bool,
    /// This track's own swing, in tenths of a percent, instead of the project's.
    pub swing: Option<u16>,
    /// The track's level, set outside its patterns. It also scales what the track sends to the
    /// returns.
    #[serde(default = "unity_level")]
    pub level: u8,
    /// What the track's held `CUT` starts from, and what its LFOs move around until a step sets it.
    #[serde(default = "open_cutoff")]
    pub cutoff: u8,
    /// Where held `RES` starts.
    #[serde(default)]
    pub resonance: u8,
    #[serde(default)]
    pub filter: FilterType,
    /// How hard the filter is driven.
    #[serde(default)]
    pub drive: u8,
    /// The utility filter after it: a high-pass, which `00` turns off.
    #[serde(default)]
    pub low_cut: u8,
    /// And a low-pass, which `FF` turns off.
    #[serde(default = "open_cutoff")]
    pub high_cut: u8,
    /// The effects after the filters, in order.
    #[serde(default)]
    pub inserts: [Insert; INSERTS],
    /// Where held `PAN` starts.
    #[serde(default = "centre")]
    pub pan: u8,
    /// How much it sends to return `A`, and where held `SNA` starts.
    #[serde(default)]
    pub send_a: u8,
    /// How much it sends to return `B`, and where held `SNB` starts.
    #[serde(default)]
    pub send_b: u8,
    #[serde(default)]
    pub lfos: [Lfo; LFOS_PER_TRACK],
    #[serde(default = "default_envelopes")]
    pub envelopes: [Envelope; TRACK_ENVELOPES],
    /// The chains this track can play, numbered within it.
    #[serde(default)]
    pub chains: BTreeMap<u32, Arc<Chain>>,
    /// The phrases its chains can play, numbered within it.
    #[serde(default)]
    pub phrases: BTreeMap<u32, Arc<Phrase>>,
}

impl Track {
    /// A fixed track: named for what it is, and with no chain until one is given to it, so it
    /// doesn't change how long a section is.
    fn fixed(kind: TrackKind) -> Self {
        Self {
            kind,
            name: kind.letter().to_owned(),
            inserts: match kind {
                TrackKind::ReturnA => [Insert::RETURN_DELAY, Insert::Empty, Insert::Empty],
                TrackKind::ReturnB => [Insert::RETURN_REVERB, Insert::Empty, Insert::Empty],
                TrackKind::Sound | TrackKind::Master => [Insert::Empty; INSERTS],
            },
            ..Self::default()
        }
    }

    /// Whether this track plays sounds.
    pub fn is_sound(&self) -> bool {
        self.kind == TrackKind::Sound
    }

    /// The total steps a chain lasts: the pulses of every row in its phrases' loops. A missing or
    /// empty chain has none, and so does one whose rows are all skipped.
    pub fn chain_steps(&self, chain: u32) -> usize {
        self.chains.get(&chain).map_or(0, |chain| {
            chain
                .entries
                .iter()
                .filter_map(|entry| self.phrases.get(&entry.phrase))
                .map(|phrase| phrase.time())
                .sum()
        })
    }

    /// Find a step within a chain, looping the chain if the position is past its end.
    pub fn chain_position(&self, chain: u32, step: usize) -> Option<ChainPosition> {
        let total = self.chain_steps(chain);
        if total == 0 {
            return None;
        }
        let mut remaining = step % total;
        let chain = self.chains.get(&chain)?;
        for (slot, entry) in chain.entries.iter().enumerate() {
            let Some(phrase) = self.phrases.get(&entry.phrase) else {
                continue;
            };
            let time = phrase.time();
            if remaining < time {
                let (step, pulse) = phrase.row_at(remaining)?;
                return Some(ChainPosition {
                    slot,
                    phrase: entry.phrase,
                    step,
                    pulse,
                    transpose: entry.transpose,
                });
            }
            remaining -= time;
        }
        None
    }

    /// Where a slot and a row of its phrase's loop fall in a chain, if the chain still has them.
    pub fn chain_place(&self, chain: u32, slot: usize, row: usize) -> Option<usize> {
        let entries = &self.chains.get(&chain)?.entries;
        let phrase = self.phrases.get(&entries.get(slot)?.phrase)?;
        if row >= phrase.len() {
            return None;
        }
        let before: usize = entries[..slot]
            .iter()
            .filter_map(|entry| self.phrases.get(&entry.phrase))
            .map(|phrase| phrase.time())
            .sum();
        Some(before + phrase.time_to(row))
    }

    /// The step at a position, once a chain position has been found.
    pub fn step_at(&self, position: ChainPosition) -> Option<&Step> {
        self.phrases
            .get(&position.phrase)?
            .loop_steps()
            .get(position.step)
    }

    /// Make a chain with this ID and one new phrase.
    pub fn add_chain(&mut self, chain: u32) {
        let phrase = lowest_unused(&self.phrases);
        self.phrases
            .insert(phrase, Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
        self.chains.insert(
            chain,
            Arc::new(Chain {
                entries: vec![ChainEntry {
                    phrase,
                    transpose: 0,
                }],
                label: String::new(),
            }),
        );
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            kind: TrackKind::default(),
            name: String::new(),
            machine: Machine::default(),
            synth_voice: SynthVoice::default(),
            sample: None,
            harmonics: centre(),
            timbre: 0,
            morph: centre(),
            mute: false,
            solo: false,
            swing: None,
            level: unity_level(),
            cutoff: open_cutoff(),
            resonance: 0,
            filter: FilterType::default(),
            drive: 0,
            low_cut: 0,
            high_cut: open_cutoff(),
            inserts: Default::default(),
            pan: centre(),
            send_a: 0,
            send_b: 0,
            lfos: Default::default(),
            envelopes: default_envelopes(),
            chains: BTreeMap::new(),
            phrases: BTreeMap::new(),
        }
    }
}

fn unity_level() -> u8 {
    UNITY_LEVEL
}

fn open_cutoff() -> u8 {
    0xFF
}

fn centre() -> u8 {
    0x80
}

/// What a section does to the tracks when it starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionStart {
    /// A track given a different chain starts it from its top, and one given the same keeps going.
    #[default]
    New,
    /// Every track goes back to its start, each time the section's length comes round.
    Reset,
}

/// What makes a track's sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Machine {
    /// Plays its sample, once, at the note's speed.
    #[default]
    Sample,
    /// One of the synth voices, after Mutable Instruments' Plaits.
    Plaits,
}

impl Machine {
    /// Every machine, in the order a list shows them.
    pub const ALL: [Self; 2] = [Self::Sample, Self::Plaits];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sample => "Sample",
            Self::Plaits => "Plaits",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Sample => "One file, at the speed of its note",
            Self::Plaits => "A synth, with a choice of voices",
        }
    }
}

/// How many patches 6-op has, which its `Patch` counts from `00`.
pub const SIX_OP_PATCHES: u8 = 96;

/// Which synth the Plaits machine plays.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthVoice {
    /// A square and a saw.
    #[default]
    VirtualAnalog,
    /// Two-operator FM.
    Fm,
    /// A slope through a waveshaper and a wavefolder.
    Waveshaping,
    /// A grid of waves in four banks.
    Wavetable,
    /// Six-operator FM, playing DX7 patches.
    SixOp,
    /// A plucked string.
    String,
    /// A struck resonator.
    Modal,
    /// Random impulses through resonant filters.
    Dust,
    /// Clocked noise through a resonant filter.
    Noise,
    /// An 808-style bass drum.
    Kick,
    /// An 808-style snare drum.
    Snare,
    /// An 808-style hi-hat.
    Hat,
}

impl SynthVoice {
    /// Every voice, in the order a list shows them.
    pub const ALL: [Self; 12] = [
        Self::VirtualAnalog,
        Self::Fm,
        Self::Waveshaping,
        Self::Wavetable,
        Self::SixOp,
        Self::String,
        Self::Modal,
        Self::Dust,
        Self::Noise,
        Self::Kick,
        Self::Snare,
        Self::Hat,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::VirtualAnalog => "VA",
            Self::Fm => "FM",
            Self::Waveshaping => "Fold",
            Self::Wavetable => "Waves",
            Self::SixOp => "6-op",
            Self::String => "String",
            Self::Modal => "Modal",
            Self::Dust => "Dust",
            Self::Noise => "Noise",
            Self::Kick => "Kick",
            Self::Snare => "Snare",
            Self::Hat => "Hat",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::VirtualAnalog => "A square and a saw, for bass and leads",
            Self::Fm => "Two-operator FM, for bass, bells and keys",
            Self::Waveshaping => "A wave folded into harmonics, for bass and leads",
            Self::Wavetable => "Waves in four banks to sweep, for pads and leads",
            Self::SixOp => "Six-op FM with 96 DX7 patches, for keys and bells",
            Self::String => "A plucked string, from a sitar's buzz to metal",
            Self::Modal => "A struck resonator, for bells, bars and wood",
            Self::Dust => "Random clicks, filtered, for crackle and rain",
            Self::Noise => "Clocked noise through a filter, for hiss and wind",
            Self::Kick => "An 808-style bass drum, tuned by its note",
            Self::Snare => "An 808-style snare drum, tuned by its note",
            Self::Hat => "An 808-style hi-hat, tuned by its note",
        }
    }

    /// What its three settings are called, which `HAR`, `TIM` and `MOR` set in this order.
    pub fn settings(self) -> [&'static str; 3] {
        match self {
            Self::VirtualAnalog => ["Harm", "Timbre", "Morph"],
            Self::Fm => ["Ratio", "Index", "Feedback"],
            Self::Waveshaping => ["Shape", "Fold", "Slope"],
            Self::Wavetable => ["Bank", "Wave", "Row"],
            Self::SixOp => ["Patch", "Bright", "Time"],
            Self::String | Self::Modal => ["Material", "Bright", "Decay"],
            Self::Dust => ["Spread", "Density", "Filter"],
            Self::Noise => ["Type", "Clock", "Res"],
            Self::Kick => ["Punch", "Tone", "Decay"],
            Self::Snare => ["Snappy", "Tone", "Decay"],
            Self::Hat => ["Noise", "Tone", "Decay"],
        }
    }

    /// Which of its settings has a middle of its own, so shows where it is around that.
    pub fn centred(self) -> [bool; 3] {
        match self {
            Self::VirtualAnalog => [true, false, false],
            Self::Fm => [false, false, true],
            Self::Waveshaping => [true, false, false],
            Self::Wavetable => [false, false, false],
            Self::SixOp => [false, true, true],
            Self::String | Self::Modal => [false, false, false],
            Self::Dust => [false, false, true],
            Self::Noise => [true, false, false],
            Self::Kick | Self::Snare | Self::Hat => [false, false, false],
        }
    }

    /// The highest value `Harm` takes. 6-op's is a patch number, so it stops at the last patch.
    pub fn harmonics_top(self) -> u8 {
        match self {
            Self::SixOp => SIX_OP_PATCHES - 1,
            _ => 0xFF,
        }
    }

    /// Whether a note keeps sounding until something ends it.
    pub fn holds(self) -> bool {
        matches!(
            self,
            Self::VirtualAnalog
                | Self::Fm
                | Self::Waveshaping
                | Self::Wavetable
                | Self::SixOp
                | Self::Dust
                | Self::Noise
        )
    }
}

/// When a launched chain starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Launch {
    /// When the track's own chain ends.
    ChainEnd,
    /// On the next step this many into the row, so every row's start is one.
    Every(NonZeroU32),
}

/// The lengths `:launch` has names for.
const LAUNCH_NAMES: [(&str, u32); 7] = [
    ("step", 1),
    ("1/4", BEAT_STEPS as u32),
    ("1/2", BEAT_STEPS as u32 * 2),
    ("1bar", BAR_STEPS as u32),
    ("2bar", BAR_STEPS as u32 * 2),
    ("4bar", BAR_STEPS as u32 * 4),
    ("8bar", BAR_STEPS as u32 * 8),
];

impl Launch {
    /// Launch on a grid of this many steps, if it's 1 or more.
    pub fn every(steps: usize) -> Option<Self> {
        let steps = u32::try_from(steps).ok()?;
        NonZeroU32::new(steps).map(Self::Every)
    }

    /// The setting with this name in `:launch`.
    pub fn named(name: &str) -> Option<Self> {
        if name == "chain" {
            return Some(Self::ChainEnd);
        }
        let (_, steps) = LAUNCH_NAMES.iter().find(|(known, _)| *known == name)?;
        NonZeroU32::new(*steps).map(Self::Every)
    }

    /// When a launch starts, to finish a sentence on the status line.
    pub fn when(self) -> String {
        match self {
            Self::ChainEnd => "when its chain ends".to_owned(),
            Self::Every(_) => format!("on the {self} grid"),
        }
    }
}

impl Default for Launch {
    fn default() -> Self {
        Self::every(BAR_STEPS).unwrap_or(Self::ChainEnd)
    }
}

impl fmt::Display for Launch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Every(steps) = self else {
            return f.write_str("chain");
        };
        match LAUNCH_NAMES.iter().find(|(_, known)| *known == steps.get()) {
            Some((name, _)) => f.write_str(name),
            None => write!(f, "{steps}-step"),
        }
    }
}

/// How many times a section plays before moving on. Saved songs can't hold 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Repeat(NonZeroU16);

impl Repeat {
    /// As many times as this, from 1 to `u16::MAX`.
    pub fn new(times: i64) -> Self {
        let times = times.clamp(1, i64::from(u16::MAX)) as u16;
        Self(NonZeroU16::new(times).unwrap_or(NonZeroU16::MIN))
    }

    /// How many times it is.
    pub fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for Repeat {
    fn default() -> Self {
        Self(NonZeroU16::MIN)
    }
}

/// One row of the song, holding one chain per track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub chains: Vec<Option<u32>>,
    /// How many steps it plays. If not set, the length of its longest chain.
    #[serde(default)]
    pub length: Option<usize>,
    /// How many times it plays before moving on.
    #[serde(default)]
    pub repeat: Repeat,
    /// What it does to the tracks when it starts.
    #[serde(default)]
    pub start: SectionStart,
    /// Send every track back to its start after this many steps. Without it, each loops on its own.
    #[serde(default)]
    pub reset: Option<usize>,
}

/// One phrase in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainEntry {
    pub phrase: u32,
    pub transpose: i8,
}

/// A list of phrases, each with a transpose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chain {
    pub entries: Vec<ChainEntry>,
    /// What to call it on screen, or empty for just its number.
    #[serde(default)]
    pub label: String,
}

/// Where an FX column loops on its own, among a phrase's stored steps. It drifts against the
/// notes, which follow the phrase's loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    pub start: usize,
    pub length: usize,
}

impl Lane {
    /// Whether a stored step is in it.
    pub fn contains(self, step: usize) -> bool {
        (self.start..self.start + self.length).contains(&step)
    }

    /// The stored step it reads at this count, which can be below 0 after playing from a step.
    pub fn step_at(self, count: i64) -> usize {
        self.start + count.rem_euclid(self.length.max(1) as i64) as usize
    }
}

/// One track's sequence of steps, reusable by ID.
///
/// It stores more than it plays, like a clip: a loop of `length` steps starting at `start`, with
/// anything either side of it kept but silent. Rows on screen count from the loop's start, so the
/// loop's first step is always beat 1. Each FX column can have a lane of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PhraseFile", into = "PhraseFile")]
pub struct Phrase {
    steps: Vec<Step>,
    start: usize,
    length: usize,
    lanes: [Option<Lane>; FX_SLOTS],
}

#[derive(Serialize, Deserialize)]
struct PhraseFile {
    steps: Vec<Step>,
    #[serde(default)]
    start: usize,
    #[serde(default)]
    length: Option<usize>,
    #[serde(default)]
    lanes: [Option<Lane>; FX_SLOTS],
}

/// A lane that fits in this many stored steps, or none if it can't.
fn fitted(lane: Option<Lane>, stored: usize) -> Option<Lane> {
    let lane = lane?;
    (lane.start < stored).then(|| Lane {
        start: lane.start,
        length: lane.length.clamp(1, stored - lane.start),
    })
}

impl From<PhraseFile> for Phrase {
    fn from(file: PhraseFile) -> Self {
        let stored = file.steps.len();
        let start = file.start.min(stored.saturating_sub(1));
        let length = file
            .length
            .unwrap_or(stored)
            .min(stored - start.min(stored));
        Self {
            lanes: file.lanes.map(|lane| fitted(lane, stored)),
            steps: file.steps,
            start,
            length,
        }
    }
}

impl From<Phrase> for PhraseFile {
    fn from(phrase: Phrase) -> Self {
        Self {
            steps: phrase.steps,
            start: phrase.start,
            length: Some(phrase.length),
            lanes: phrase.lanes,
        }
    }
}

impl Phrase {
    /// A phrase of empty steps. A phrase always has at least one.
    pub fn new(length: usize) -> Self {
        let length = length.max(1);
        Self {
            steps: vec![Step::default(); length],
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
        }
    }

    /// A phrase holding these steps, looping over all of them.
    pub fn from_steps(steps: Vec<Step>) -> Self {
        let length = steps.len();
        Self {
            steps,
            start: 0,
            length,
            lanes: [None; FX_SLOTS],
        }
    }

    /// Every stored step, in or out of the loop.
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The steps that play.
    pub fn loop_steps(&self) -> &[Step] {
        &self.steps[self.start..self.start + self.length]
    }

    /// The next value with this command in a slot, looking forward from a step of the loop and
    /// wrapping, with how many steps away it is. The step itself doesn't count.
    pub fn next_value(
        &self,
        step: usize,
        slot: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let steps = self.loop_steps();
        (1..steps.len()).find_map(|distance| {
            let fx = steps[(step + distance) % steps.len()].fx.get(slot)?;
            (fx.command == Some(command)).then_some((distance, *fx))
        })
    }

    /// An FX column's own lane, if it has one. One without follows the phrase's loop.
    pub fn lane(&self, slot: usize) -> Option<Lane> {
        self.lanes.get(slot).copied().flatten()
    }

    /// Give an FX column a lane of its own, or none to follow the phrase's loop again. Steps are
    /// stored for all of it, up to the most a phrase keeps.
    pub fn set_lane(&mut self, slot: usize, lane: Option<Lane>) {
        if slot >= FX_SLOTS {
            return;
        }
        if let Some(lane) = lane {
            self.store_up_to((lane.start + lane.length.max(1)).min(MAX_PHRASE_STEPS));
        }
        self.lanes[slot] = fitted(lane, self.steps.len());
    }

    /// The next value with this command in a lane, looking forward from one of its steps and
    /// wrapping within it, with how many steps away it is.
    pub fn next_in_lane(
        &self,
        slot: usize,
        step: usize,
        command: FxCommand,
    ) -> Option<(usize, FxSlot)> {
        let lane = self.lane(slot)?;
        let at = step.checked_sub(lane.start)?;
        (1..lane.length).find_map(|distance| {
            let index = lane.start + (at + distance) % lane.length;
            let fx = self.steps.get(index)?.fx.get(slot)?;
            (fx.command == Some(command)).then_some((distance, *fx))
        })
    }

    /// How many steps its loop lasts: each row's pulses, so a skipped row takes none.
    pub fn time(&self) -> usize {
        self.loop_steps().iter().map(Step::pulses).sum()
    }

    /// How many steps into the loop a row of it starts.
    pub fn time_to(&self, row: usize) -> usize {
        self.loop_steps().iter().take(row).map(Step::pulses).sum()
    }

    /// The row of the loop playing this many steps in, and which of its pulses. Skipped rows are
    /// passed over, and a loop that takes no time has none.
    pub fn row_at(&self, time: usize) -> Option<(usize, usize)> {
        let mut left = time;
        for (row, step) in self.loop_steps().iter().enumerate() {
            let pulses = step.pulses();
            if left < pulses {
                return Some((row, left));
            }
            left -= pulses;
        }
        None
    }

    /// How many steps there are from the start of a row of the loop to a row this many after it,
    /// wrapping round, for a glide's time.
    pub fn time_between(&self, row: usize, rows: usize) -> usize {
        let steps = self.loop_steps();
        (0..rows)
            .map(|at| steps[(row + at) % steps.len()].pulses())
            .sum()
    }

    /// The number of rows that play.
    pub fn len(&self) -> usize {
        self.length
    }

    /// Always false, since a phrase has at least one step.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Where the loop starts among the stored steps.
    pub fn start(&self) -> usize {
        self.start
    }

    /// Where the loop sits, for working out rows and beats.
    pub fn extent(&self) -> Extent {
        Extent {
            start: self.start,
            length: self.length,
            stored: self.steps.len(),
        }
    }

    /// One stored step, to change.
    pub fn step_mut(&mut self, index: usize) -> Option<&mut Step> {
        self.steps.get_mut(index)
    }

    /// Set the loop's length, keeping every step. A length of 0 becomes 1.
    pub fn set_len(&mut self, length: usize) {
        self.length = length.max(1);
        self.store_up_to(self.start + self.length);
    }

    /// Store at least this many steps, up to the most a phrase keeps. The loop stays where it is.
    pub fn grow_to(&mut self, stored: usize) {
        self.store_up_to(stored.min(MAX_PHRASE_STEPS));
    }

    fn store_up_to(&mut self, stored: usize) {
        if stored > self.steps.len() {
            self.steps.resize(stored, Step::default());
        }
    }

    /// Move the loop without changing its length, adding empty steps if it runs off either end.
    pub fn slide(&mut self, by: isize) {
        let room = MAX_PHRASE_STEPS.saturating_sub(self.steps.len());
        let to = (self.start as isize).saturating_add(by);
        // Where the loop started, counted in the steps stored after any are added in front.
        let mut was = self.start;
        if to < 0 {
            let added = to.unsigned_abs().min(room);
            self.steps
                .splice(0..0, std::iter::repeat_n(Step::default(), added));
            self.start = 0;
            was += added;
            for lane in self.lanes.iter_mut().flatten() {
                lane.start += added;
            }
        } else {
            let highest = MAX_PHRASE_STEPS
                .max(self.steps.len())
                .saturating_sub(self.length);
            self.start = (to as usize).min(highest);
        }
        self.store_up_to(self.start + self.length);
        // Lanes move with the loop, as they're set where it is. One that would go above the first
        // step stops there.
        let moved = self.start as isize - was as isize;
        let mut lanes = self.lanes;
        for lane in lanes.iter_mut().flatten() {
            lane.start = lane.start.saturating_add_signed(moved);
        }
        for (slot, lane) in lanes.into_iter().enumerate() {
            self.set_lane(slot, lane);
        }
    }

    /// Move the loop's steps later, wrapping round within it. Negative moves them earlier, and steps
    /// outside the loop stay where they are.
    pub fn rotate(&mut self, by: isize) {
        let (start, length) = (self.start, self.length);
        if length > 0 {
            let by = by.rem_euclid(length as isize) as usize;
            self.steps[start..start + length].rotate_right(by);
        }
    }

    /// Double the loop by repeating it. Steps after it move down rather than being overwritten.
    pub fn double(&mut self) {
        let copy = self.loop_steps().to_vec();
        let end = self.start + self.length;
        let added = copy.len();
        self.steps.splice(end..end, copy);
        self.length *= 2;
        // Lanes after the loop move down with their steps.
        for lane in self.lanes.iter_mut().flatten() {
            if lane.start >= end {
                lane.start += added;
            }
        }
    }
}

/// A sample and how it plays.
/// Where a position in a chain lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPosition {
    pub slot: usize,
    pub phrase: u32,
    /// The row of the phrase's loop.
    pub step: usize,
    /// Which of the row's pulses, from 0 for its first.
    pub pulse: usize,
    pub transpose: i8,
}

fn default_limiter() -> bool {
    true
}

fn default_skip() -> u32 {
    BEAT_STEPS as u32
}

/// Everything saved in `song.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub format_version: u32,
    /// Tempo, in thousandths of a BPM.
    pub millibpm: u32,
    pub seed: u64,
    /// How far Insert moves down after a note.
    pub step: u32,
    /// How far the left and right arrows move in Insert.
    #[serde(default = "default_skip")]
    pub skip: u32,
    /// The scale for entering and moving notes, kept with the song and not used to play it.
    #[serde(default)]
    pub scale: Scale,
    /// Whether the main output clips at full level. Off, a render keeps levels above it.
    #[serde(default = "default_limiter")]
    pub limiter: bool,
    /// Project swing, in tenths of a percent.
    pub swing: u16,
    /// When launched chains start.
    #[serde(default)]
    pub launch: Launch,
    pub tracks: Vec<Track>,
    pub sections: Vec<Section>,
}

impl Song {
    /// A new project: one section, one chain and one phrase per track.
    pub fn new(tracks: usize) -> Self {
        let mut song = Self {
            format_version: FORMAT_VERSION,
            millibpm: DEFAULT_MILLIBPM,
            seed: 0,
            step: 1,
            skip: default_skip(),
            scale: Scale::default(),
            limiter: default_limiter(),
            swing: 500,
            launch: Launch::default(),
            tracks: Vec::new(),
            sections: vec![Section::default()],
        };
        song.ensure_fixed_tracks();
        for _ in 0..tracks.max(1) {
            song.add_track();
        }
        song
    }

    /// Give a song without them its fixed tracks, as a song from before them has none.
    pub fn ensure_fixed_tracks(&mut self) {
        if self.tracks.iter().all(Track::is_sound) {
            self.tracks.extend(TrackKind::FIXED.map(Track::fixed));
            for section in &mut self.sections {
                section.chains.resize(self.tracks.len(), None);
            }
        }
    }

    /// How many tracks play sounds. They come first, and then the fixed tracks.
    pub fn sound_tracks(&self) -> usize {
        self.tracks.len().saturating_sub(FIXED_TRACKS)
    }

    /// Add a track after the last that plays sounds, with a new chain and phrase in the first
    /// section and none in the rest.
    pub fn add_track(&mut self) {
        let mut track = Track::default();
        track.add_chain(0);
        let at = self.sound_tracks();
        self.tracks.insert(at, track);
        for (index, section) in self.sections.iter_mut().enumerate() {
            section
                .chains
                .insert(at.min(section.chains.len()), (index == 0).then_some(0));
        }
    }

    /// Remove the last track that plays sounds.
    pub fn remove_last_sound_track(&mut self) {
        let Some(at) = self.sound_tracks().checked_sub(1) else {
            return;
        };
        self.tracks.remove(at);
        for section in &mut self.sections {
            if at < section.chains.len() {
                section.chains.remove(at);
            }
        }
    }

    /// How many steps a section plays before the next one.
    pub fn section_length(&self, section: &Section) -> usize {
        section
            .length
            .unwrap_or_else(|| self.section_steps(section))
    }

    /// How many steps a section plays before moving on, with its repeats.
    pub fn section_plays(&self, section: &Section) -> usize {
        self.section_length(section) * usize::from(section.repeat.get())
    }

    /// How many steps the whole song plays, once through.
    pub fn song_steps(&self) -> usize {
        self.sections.iter().map(|s| self.section_plays(s)).sum()
    }

    /// Add a section after this position, with no chains.
    pub fn insert_section(&mut self, at: usize) {
        let section = Section {
            chains: vec![None; self.tracks.len()],
            ..Section::default()
        };
        self.sections.insert(at.min(self.sections.len()), section);
    }

    /// A section's length: its longest chain.
    pub fn section_steps(&self, section: &Section) -> usize {
        self.tracks
            .iter()
            .zip(&section.chains)
            .filter_map(|(track, chain)| chain.map(|chain| track.chain_steps(chain)))
            .max()
            .unwrap_or(0)
    }
}

/// The lowest ID not in use.
pub fn lowest_unused<T>(items: &BTreeMap<u32, T>) -> u32 {
    let mut candidate = 0;
    for id in items.keys() {
        if *id != candidate {
            break;
        }
        candidate += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::{Distort, DistortType};

    #[test]
    fn a_lane_saves_fits_its_steps_and_follows_them() {
        let mut phrase = Phrase::new(16);
        phrase.set_lane(
            1,
            Some(Lane {
                start: 4,
                length: 3,
            }),
        );
        assert_eq!(
            phrase.lane(1),
            Some(Lane {
                start: 4,
                length: 3
            })
        );
        assert_eq!(phrase.lane(0), None, "others follow the phrase");
        let text = ron::to_string(&phrase).unwrap();
        assert_eq!(ron::from_str::<Phrase>(&text).unwrap(), phrase);

        // A lane past the end stores the steps it needs.
        phrase.set_lane(
            2,
            Some(Lane {
                start: 14,
                length: 6,
            }),
        );
        assert_eq!(phrase.steps().len(), 20);
        // Sliding the loop moves it with the loop, whether steps are added in front or not.
        phrase.slide(-2);
        assert_eq!(
            phrase.lane(1).unwrap().start,
            4,
            "on the same step of the loop"
        );
        assert_eq!(phrase.start(), 0);
        phrase.slide(3);
        assert_eq!(phrase.lane(1).unwrap().start, 7);
        // Doubling the loop moves one after it down with its steps.
        let mut doubled = Phrase::new(4);
        doubled.grow_to(8);
        doubled.set_lane(
            0,
            Some(Lane {
                start: 5,
                length: 2,
            }),
        );
        doubled.double();
        assert_eq!(doubled.lane(0).unwrap().start, 9);
    }

    #[test]
    fn a_saved_phrase_without_lanes_has_none_and_a_bad_one_is_fitted() {
        let mut old = ron::to_string(&Phrase::new(2)).unwrap();
        old = old.replace(",lanes:(None,None,None)", "");
        assert!(!old.contains("lanes"), "{old}");
        let phrase: Phrase = ron::from_str(&old).unwrap();
        assert_eq!(phrase.lane(0), None);
        let file = PhraseFile {
            steps: vec![Step::default(); 2],
            start: 0,
            length: Some(2),
            lanes: [
                Some(Lane {
                    start: 1,
                    length: 9,
                }),
                Some(Lane {
                    start: 5,
                    length: 1,
                }),
                None,
            ],
        };
        let phrase = Phrase::from(file);
        assert_eq!(
            phrase.lane(0),
            Some(Lane {
                start: 1,
                length: 1
            })
        );
        assert_eq!(phrase.lane(1), None, "it starts past the steps");
    }

    #[test]
    fn a_lane_reads_by_count_and_a_glide_looks_within_it() {
        let lane = Lane {
            start: 2,
            length: 3,
        };
        let read: Vec<usize> = (0..7).map(|count| lane.step_at(count)).collect();
        assert_eq!(read, [2, 3, 4, 2, 3, 4, 2]);
        assert_eq!(lane.step_at(-1), 4);
        let steps = [
            "--- -- CUT10 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- CUT20 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- CUT30 ---00 ---00",
        ];
        let mut phrase = Phrase::from_steps(steps.iter().map(|t| t.parse().unwrap()).collect());
        phrase.set_lane(0, Some(lane));
        let next = |step| {
            phrase
                .next_in_lane(0, step, FxCommand::Cut)
                .map(|(distance, fx)| (distance, fx.value))
        };
        assert_eq!(
            next(4),
            Some((1, 0x20)),
            "wraps to the lane's start, not row 0"
        );
        assert_eq!(next(2), Some((2, 0x30)));
        assert_eq!(next(0), None, "not in the lane");
    }

    #[test]
    fn next_value_looks_forward_in_the_slot_and_wraps() {
        let steps = [
            "--- -- CUT10 ---00",
            "--- -- RES20 ---00",
            "--- -- CUT30 CUT90",
            "--- -- ---00 ---00",
        ];
        let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
        let phrase = Phrase::from_steps(steps);
        let found = |step, slot, command| {
            phrase
                .next_value(step, slot, command)
                .map(|(distance, fx)| (distance, fx.value))
        };
        assert_eq!(found(0, 0, FxCommand::Cut), Some((2, 0x30)));
        assert_eq!(found(2, 0, FxCommand::Cut), Some((2, 0x10)));
        assert_eq!(found(1, 0, FxCommand::Res), None, "not itself");
        assert_eq!(found(0, 1, FxCommand::Cut), Some((2, 0x90)));
        assert_eq!(found(0, 0, FxCommand::Vol), None);
    }
    use crate::constants::DEFAULT_TRACKS;

    #[test]
    fn a_track_saved_without_a_level_loads_at_0_db() {
        let track: Track = ron::from_str("(mute: false, solo: true, swing: None)").unwrap();
        assert_eq!(track.level, UNITY_LEVEL);
        assert!(track.solo);
    }

    #[test]
    fn a_new_song_has_a_chain_and_phrase_per_track() {
        let song = Song::new(DEFAULT_TRACKS);
        assert_eq!(song.sound_tracks(), DEFAULT_TRACKS);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].chains.len(), song.tracks.len());
        for track in &song.tracks[..DEFAULT_TRACKS] {
            assert_eq!((track.chains.len(), track.phrases.len()), (1, 1));
        }
        assert_eq!(song.section_steps(&song.sections[0]), DEFAULT_PHRASE_STEPS);
    }

    #[test]
    fn the_fixed_tracks_stay_last_and_dont_set_a_sections_length() {
        let mut song = Song::new(2);
        let kinds = |song: &Song| song.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
        use TrackKind::{Master, ReturnA, ReturnB, Sound};
        assert_eq!(kinds(&song), [Sound, Sound, ReturnA, ReturnB, Master]);
        let names: Vec<&str> = song.tracks[2..].iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["A", "B", "M"]);
        assert_eq!(song.sections[0].chains[2..], [None, None, None]);

        song.add_track();
        assert_eq!(
            kinds(&song),
            [Sound, Sound, Sound, ReturnA, ReturnB, Master]
        );
        assert_eq!(song.sections[0].chains[2], Some(0), "the new one's chain");
        assert_eq!(song.sections[0].chains[3], None, "A's stays A's");

        song.remove_last_sound_track();
        assert_eq!(kinds(&song), [Sound, Sound, ReturnA, ReturnB, Master]);
        assert_eq!(song.sections[0].chains.len(), 5);
    }

    #[test]
    fn a_song_from_before_the_fixed_tracks_gets_them_once() {
        let mut song = Song::new(2);
        song.tracks.truncate(2);
        song.sections[0].chains.truncate(2);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 5);
        assert_eq!(song.sound_tracks(), 2);
        assert_eq!(song.sections[0].chains.len(), 5);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 5);
    }

    fn gated(gates: &[&str]) -> Phrase {
        Phrase::from_steps(
            gates
                .iter()
                .map(|gate| format!("C-4 -- {gate} ---00 ---00 ---00").parse().unwrap())
                .collect(),
        )
    }

    #[test]
    fn a_row_lasts_its_pulses_and_a_skipped_one_none() {
        let phrase = gated(&["SGL1", "HLD3", "MLT0", "RST2"]);
        assert_eq!(phrase.time(), 6);
        let rows: Vec<(usize, usize)> = (0..6).filter_map(|t| phrase.row_at(t)).collect();
        assert_eq!(rows, [(0, 0), (1, 0), (1, 1), (1, 2), (3, 0), (3, 1)]);
        assert_eq!(phrase.row_at(6), None);
        assert_eq!(phrase.time_to(3), 4, "the skipped row takes no time");
        assert_eq!(phrase.time_between(3, 2), 2 + 1, "wrapping to row 0");
        let skipped = gated(&["SGL0", "HLD0"]);
        assert_eq!(skipped.time(), 0);
        assert_eq!(skipped.row_at(0), None, "nothing to play, and no hang");
    }

    #[test]
    fn a_chain_lasts_its_rows_pulses_and_finds_them() {
        let mut song = Song::new(1);
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(gated(&["HLD3", "SGL1"])));
        song.tracks[0].phrases.insert(7, Arc::new(gated(&["MLT2"])));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        assert_eq!(track.chain_steps(0), 6);
        let at = |step| {
            let found = track.chain_position(0, step).unwrap();
            (found.slot, found.step, found.pulse)
        };
        assert_eq!(at(2), (0, 0, 2));
        assert_eq!(at(3), (0, 1, 0));
        assert_eq!(at(5), (1, 0, 1));
        assert_eq!(at(6), (0, 0, 0), "and round again");
        assert_eq!(track.chain_place(0, 1, 0), Some(4));
        assert_eq!(track.chain_place(0, 0, 1), Some(3));
    }

    #[test]
    fn a_slot_and_row_find_their_place_in_a_chain_if_they_are_still_there() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        assert_eq!(track.chain_place(0, 1, 2), Some(16 + 2));
        assert_eq!(track.chain_place(0, 1, 4), None, "past its loop");
        assert_eq!(track.chain_place(0, 2, 0), None, "no third slot");
        assert_eq!(track.chain_place(9, 0, 0), None, "no such chain");
    }

    #[test]
    fn chain_positions_cross_phrases_and_loop() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![
                    ChainEntry {
                        phrase: 0,
                        transpose: 0,
                    },
                    ChainEntry {
                        phrase: 7,
                        transpose: 3,
                    },
                ],
                label: String::new(),
            }),
        );
        assert_eq!(song.tracks[0].chain_steps(0), 20);
        let position = song.tracks[0].chain_position(0, 17).unwrap();
        assert_eq!((position.slot, position.phrase, position.step), (1, 7, 1));
        assert_eq!(position.transpose, 3);
        assert_eq!(song.tracks[0].chain_position(0, 20).unwrap().step, 0);
        assert_eq!(song.tracks[0].chain_position(99, 0), None);
    }

    /// A phrase whose steps can be told apart by velocity, which is the step's stored position.
    fn numbered(length: usize) -> Phrase {
        let steps = (0..length)
            .map(|index| Step {
                velocity: Some(index as u8),
                ..Step::default()
            })
            .collect();
        Phrase::from_steps(steps)
    }

    fn played(phrase: &Phrase) -> Vec<u8> {
        phrase
            .loop_steps()
            .iter()
            .map(|step| step.velocity.unwrap())
            .collect()
    }

    #[test]
    fn steps_outside_the_loop_stay_stored_and_saved() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        assert_eq!(played(&phrase), [1, 2]);
        assert_eq!(phrase.steps().len(), 4);
        let saved = ron::to_string(&phrase).unwrap();
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded, phrase);
        phrase.set_len(0);
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn a_file_from_before_loops_plays_all_its_steps() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 0,
            lanes: [None; FX_SLOTS],
            length: None,
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (0, 3));
    }

    #[test]
    fn a_loop_past_the_end_of_a_file_is_pulled_back() {
        let file = PhraseFile {
            steps: vec![Step::default(); 3],
            start: 9,
            lanes: [None; FX_SLOTS],
            length: Some(9),
        };
        let phrase = Phrase::from(file);
        assert_eq!((phrase.start(), phrase.len()), (2, 1));
        assert_eq!(phrase.loop_steps().len(), 1);
    }

    #[test]
    fn sliding_keeps_the_length_and_adds_steps_at_either_end() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(3);
        assert_eq!(phrase.start(), 3);
        assert_eq!(phrase.steps().len(), 5, "one more step, so the loop fits");
        phrase.slide(-5);
        assert_eq!(phrase.start(), 0);
        assert_eq!(phrase.len(), 2);
        assert_eq!(phrase.steps().len(), 7, "two more in front");
        assert_eq!(phrase.steps()[2].velocity, Some(0), "what was first moved");
    }

    #[test]
    fn a_huge_slide_stops_at_the_most_a_phrase_keeps() {
        let mut phrase = numbered(4);
        phrase.slide(isize::MAX);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.loop_steps().len(), 4);
        let mut phrase = numbered(4);
        phrase.slide(isize::MIN);
        assert_eq!(phrase.steps().len(), MAX_PHRASE_STEPS);
        assert_eq!(phrase.start(), 0);
    }

    #[test]
    fn doubling_repeats_the_loop_and_keeps_what_is_after_it() {
        let mut phrase = numbered(4);
        phrase.set_len(2);
        phrase.slide(1);
        phrase.double();
        assert_eq!(played(&phrase), [1, 2, 1, 2]);
        assert_eq!(
            phrase.steps()[5].velocity,
            Some(3),
            "moved, not overwritten"
        );
    }

    #[test]
    fn a_track_saved_before_its_page_loads_with_the_old_sound() {
        let track: Track = ron::from_str("(mute: false, solo: false, swing: None)").unwrap();
        assert_eq!(track, Track::default());
        assert_eq!((track.cutoff, track.resonance, track.pan), (0xFF, 0, 0x80));
        assert_eq!(track.lfos, [Lfo::default(); LFOS_PER_TRACK]);
    }

    #[test]
    fn a_tracks_inserts_save_and_load_and_start_empty() {
        let mut track = Track::default();
        assert_eq!(track.inserts, [Insert::Empty; INSERTS]);
        track.inserts[1] = Insert::Distort(Distort {
            kind: DistortType::Fold,
            amount: 0x80,
            mix: 0x40,
            gain: 0xD0,
        });
        let text = ron::to_string(&track).unwrap();
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded, track);
    }

    #[test]
    fn level_values_are_even_steps_in_db() {
        let db = |level: f32| 20.0 * level_gain(level).log10();
        let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} and {b}");
        assert_eq!(level_gain(0xE0 as f32), 1.0);
        close(db(0xF0 as f32), 4.0);
        close(db(0xF8 as f32), 6.0);
        close(db(0xFF as f32), 6.0);
        close(db(0xD0 as f32), -4.0);
        close(db(0xC0 as f32), -8.0);
        close(db(0x80 as f32), -24.0);
        close(db(0x40 as f32), -40.0);
        close(db(0x01 as f32), -55.75);
        assert_eq!(level_gain(0.0), 0.0);
        assert_eq!(level_gain(-3.0), 0.0);
    }

    #[test]
    fn a_section_saved_without_a_reset_loads_with_none() {
        let section: Section = ron::from_str("(chains: [Some(0)])").unwrap();
        assert_eq!(section.reset, None);
        let saved = ron::to_string(&Section {
            reset: Some(64),
            ..section
        })
        .unwrap();
        assert_eq!(ron::from_str::<Section>(&saved).unwrap().reset, Some(64));
    }

    #[test]
    fn rotating_stays_inside_the_loop() {
        use crate::step::Note;

        let note = |midi| Step {
            note: Note::On(midi),
            ..Step::default()
        };
        let mut phrase = Phrase::from_steps((60..66).map(note).collect());
        phrase.set_len(3);
        phrase.slide(1);
        let notes =
            |phrase: &Phrase| -> Vec<Note> { phrase.steps().iter().map(|s| s.note).collect() };

        phrase.rotate(1);
        let on = |midi: [u8; 6]| midi.map(Note::On).to_vec();
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        phrase.rotate(-1);
        assert_eq!(notes(&phrase), on([60, 61, 62, 63, 64, 65]));
        phrase.rotate(4);
        assert_eq!(notes(&phrase), on([60, 63, 61, 62, 64, 65]));
        assert_eq!((phrase.start(), phrase.len()), (1, 3));
    }

    #[test]
    fn step_at_reads_within_the_loop() {
        let mut song = Song::new(1);
        let mut phrase = numbered(8);
        phrase.set_len(4);
        phrase.slide(2);
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        assert_eq!(song.tracks[0].chain_steps(0), 4);
        let position = song.tracks[0].chain_position(0, 5).unwrap();
        assert_eq!(position.step, 1, "the chain loops over the loop");
        assert_eq!(song.tracks[0].step_at(position).unwrap().velocity, Some(3));
    }

    #[test]
    fn the_lowest_unused_id_fills_gaps() {
        let items = BTreeMap::from([(0, ()), (1, ()), (3, ())]);
        assert_eq!(lowest_unused(&items), 2);
        assert_eq!(lowest_unused(&BTreeMap::<u32, ()>::new()), 0);
    }
}
