//! The song and everything it refers to by ID.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::constants::{
    BEAT_STEPS, DB_PER_LEVEL, DEFAULT_MILLIBPM, DEFAULT_PHRASE_STEPS, FORMAT_VERSION, MAX_LEVEL,
    MAX_PHRASE_STEPS, TRACK_ENVELOPES, UNITY_LEVEL,
};
use crate::envelope::{Envelope, default_envelopes};
use crate::lfo::{LFOS_PER_TRACK, Lfo};
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

/// One voice's lane through the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Two characters, or empty for none.
    #[serde(default)]
    pub name: String,
    /// What it plays, relative to the project folder.
    #[serde(default)]
    pub sample: Option<String>,
    pub mute: bool,
    pub solo: bool,
    /// This track's own swing, in tenths of a percent, instead of the project's.
    pub swing: Option<u16>,
    /// The track's level, set outside its patterns. It also scales what the track sends to the
    /// delay and reverb.
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
    /// Where held `PAN` starts.
    #[serde(default = "centre")]
    pub pan: u8,
    /// Where held `DLY` starts.
    #[serde(default)]
    pub delay: u8,
    /// Where held `REV` starts.
    #[serde(default)]
    pub reverb: u8,
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
    /// The total steps in a chain. A missing or empty chain has none.
    pub fn chain_steps(&self, chain: u32) -> usize {
        self.chains.get(&chain).map_or(0, |chain| {
            chain
                .entries
                .iter()
                .filter_map(|entry| self.phrases.get(&entry.phrase))
                .map(|phrase| phrase.len())
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
            if remaining < phrase.len() {
                return Some(ChainPosition {
                    slot,
                    phrase: entry.phrase,
                    step: remaining,
                    transpose: entry.transpose,
                });
            }
            remaining -= phrase.len();
        }
        None
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
            }),
        );
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            name: String::new(),
            sample: None,
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
            pan: centre(),
            delay: 0,
            reverb: 0,
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
    /// Tracks keep counting from where they are.
    #[default]
    Carry,
    /// Every track goes back to its start.
    Reset,
}

/// One row of the song, holding one chain per track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub chains: Vec<Option<u32>>,
    /// How many steps it plays. If not set, the length of its longest chain.
    #[serde(default)]
    pub length: Option<usize>,
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
}

/// One track's sequence of steps, reusable by ID.
///
/// It stores more than it plays, like a clip: a loop of `length` steps starting at `start`, with
/// anything either side of it kept but silent. Rows on screen count from the loop's start, so the
/// loop's first step is always beat 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PhraseFile", into = "PhraseFile")]
pub struct Phrase {
    steps: Vec<Step>,
    start: usize,
    length: usize,
}

#[derive(Serialize, Deserialize)]
struct PhraseFile {
    steps: Vec<Step>,
    #[serde(default)]
    start: usize,
    #[serde(default)]
    length: Option<usize>,
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
        }
    }

    /// A phrase holding these steps, looping over all of them.
    pub fn from_steps(steps: Vec<Step>) -> Self {
        let length = steps.len();
        Self {
            steps,
            start: 0,
            length,
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

    /// The number of steps that play.
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
        if to < 0 {
            let added = to.unsigned_abs().min(room);
            self.steps
                .splice(0..0, std::iter::repeat_n(Step::default(), added));
            self.start = 0;
        } else {
            let highest = MAX_PHRASE_STEPS
                .max(self.steps.len())
                .saturating_sub(self.length);
            self.start = (to as usize).min(highest);
        }
        self.store_up_to(self.start + self.length);
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
        self.steps.splice(end..end, copy);
        self.length *= 2;
    }
}

/// A sample and how it plays.
/// Where a position in a chain lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPosition {
    pub slot: usize,
    pub phrase: u32,
    pub step: usize,
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
    /// Whether the main output clips at full level. Off, a render keeps levels above it.
    #[serde(default = "default_limiter")]
    pub limiter: bool,
    /// Project swing, in tenths of a percent.
    pub swing: u16,
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
            limiter: default_limiter(),
            swing: 500,
            tracks: Vec::new(),
            sections: vec![Section::default()],
        };
        for _ in 0..tracks.max(1) {
            song.add_track();
        }
        song
    }

    /// Add a track at the end, with a new chain and phrase in the first section and none in the rest.
    pub fn add_track(&mut self) {
        let mut track = Track::default();
        track.add_chain(0);
        self.tracks.push(track);
        for (index, section) in self.sections.iter_mut().enumerate() {
            section.chains.push((index == 0).then_some(0));
        }
    }

    /// How many steps a section plays before the next one.
    pub fn section_length(&self, section: &Section) -> usize {
        section
            .length
            .unwrap_or_else(|| self.section_steps(section))
    }

    /// How many steps the whole song plays, once through.
    pub fn song_steps(&self) -> usize {
        self.sections.iter().map(|s| self.section_length(s)).sum()
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
        assert_eq!(song.tracks.len(), DEFAULT_TRACKS);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].chains.len(), DEFAULT_TRACKS);
        for track in &song.tracks {
            assert_eq!((track.chains.len(), track.phrases.len()), (1, 1));
        }
        assert_eq!(song.section_steps(&song.sections[0]), DEFAULT_PHRASE_STEPS);
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
