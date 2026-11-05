//! The song and everything it refers to by ID.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::constants::{BEAT_STEPS, DEFAULT_MILLIBPM, DEFAULT_PHRASE_STEPS, FORMAT_VERSION};
use crate::label::is_named_slot;
use crate::step::Step;

/// One voice's lane through the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub mute: bool,
    pub solo: bool,
    /// This track's own swing, in tenths of a percent, instead of the project's.
    pub swing: Option<u16>,
    /// The track's level, set outside its patterns. It also scales what the track sends to the
    /// delay and reverb.
    #[serde(default = "full_level")]
    pub level: u8,
}

impl Default for Track {
    fn default() -> Self {
        Self {
            mute: false,
            solo: false,
            swing: None,
            level: full_level(),
        }
    }
}

fn full_level() -> u8 {
    0xFF
}

/// One row of the song, holding one chain per track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub chains: Vec<Option<u32>>,
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
/// Shortening a phrase hides steps, so they come back if it is lengthened in the session.
/// Hidden steps are dropped on save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PhraseFile", into = "PhraseFile")]
pub struct Phrase {
    steps: Vec<Step>,
    visible: usize,
}

#[derive(Serialize, Deserialize)]
struct PhraseFile {
    steps: Vec<Step>,
}

impl From<PhraseFile> for Phrase {
    fn from(file: PhraseFile) -> Self {
        let visible = file.steps.len();
        Self {
            steps: file.steps,
            visible,
        }
    }
}

impl From<Phrase> for PhraseFile {
    fn from(mut phrase: Phrase) -> Self {
        phrase.steps.truncate(phrase.visible);
        Self {
            steps: phrase.steps,
        }
    }
}

impl Phrase {
    /// A phrase of empty steps. A phrase always has at least one.
    pub fn new(length: usize) -> Self {
        let visible = length.max(1);
        Self {
            steps: vec![Step::default(); visible],
            visible,
        }
    }

    /// A phrase holding these steps.
    pub fn from_steps(steps: Vec<Step>) -> Self {
        let visible = steps.len();
        Self { steps, visible }
    }

    /// The steps that play.
    pub fn steps(&self) -> &[Step] {
        &self.steps[..self.visible]
    }

    /// The number of steps that play.
    pub fn len(&self) -> usize {
        self.visible
    }

    /// Always false, since a phrase has at least one step.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// One visible step, to change.
    pub fn step_mut(&mut self, index: usize) -> Option<&mut Step> {
        self.steps[..self.visible].get_mut(index)
    }

    /// Set the length, keeping any steps past it hidden. A length of 0 becomes 1.
    pub fn set_len(&mut self, length: usize) {
        let visible = length.max(1);
        if visible > self.steps.len() {
            self.steps.resize(visible, Step::default());
        }
        self.visible = visible;
    }
}

/// A sample and how it plays.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrument {
    /// Relative to the project folder.
    pub sample: Option<String>,
    pub volume: u8,
    pub attack_ms: u16,
    pub release_ms: u16,
    /// Whether this slot's name is a label rather than just its number.
    #[serde(default)]
    pub labelled: bool,
}

impl Default for Instrument {
    fn default() -> Self {
        Self {
            sample: None,
            volume: 0xFF,
            attack_ms: 1,
            release_ms: 50,
            labelled: false,
        }
    }
}

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
    pub chains: BTreeMap<u32, Arc<Chain>>,
    pub phrases: BTreeMap<u32, Arc<Phrase>>,
    pub instruments: BTreeMap<u32, Arc<Instrument>>,
}

impl Song {
    /// A new project: one section, one chain and one phrase per track, and instrument `00`.
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
            chains: BTreeMap::new(),
            phrases: BTreeMap::new(),
            instruments: BTreeMap::from([(0, Arc::new(Instrument::default()))]),
        };
        for _ in 0..tracks.max(1) {
            song.add_track();
        }
        song
    }

    /// Whether a slot's name is a label.
    pub fn is_labelled(&self, slot: u32) -> bool {
        is_named_slot(slot) || self.instruments.get(&slot).is_some_and(|i| i.labelled)
    }

    /// The slot after or before this one. Numbers go one at a time. Past `FF`, and among slots
    /// only a label can name, it goes to the next instrument that exists.
    pub fn next_instrument(&self, slot: u32, forward: bool) -> u32 {
        if forward {
            if slot < 0xFF {
                return slot + 1;
            }
            let later = self.instruments.range(slot + 1..).next();
            return later.map_or(slot, |(id, _)| *id);
        }
        if !is_named_slot(slot) {
            return slot.saturating_sub(1);
        }
        let earlier = self.instruments.range(..slot).next_back();
        earlier.map_or(0xFF, |(id, _)| *id)
    }

    /// Add a track at the end, with its own new chain and phrase in every section.
    pub fn add_track(&mut self) {
        self.tracks.push(Track::default());
        for index in 0..self.sections.len() {
            let phrase = lowest_unused(&self.phrases);
            self.phrases
                .insert(phrase, Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
            let chain = lowest_unused(&self.chains);
            self.chains.insert(
                chain,
                Arc::new(Chain {
                    entries: vec![ChainEntry {
                        phrase,
                        transpose: 0,
                    }],
                }),
            );
            self.sections[index].chains.push(Some(chain));
        }
    }

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

    /// A section's length: its longest chain.
    pub fn section_steps(&self, section: &Section) -> usize {
        section
            .chains
            .iter()
            .flatten()
            .map(|chain| self.chain_steps(*chain))
            .max()
            .unwrap_or(0)
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
            .steps()
            .get(position.step)
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
    use crate::constants::DEFAULT_TRACKS;

    #[test]
    fn a_track_saved_without_a_level_loads_at_full() {
        let track: Track = ron::from_str("(mute: false, solo: true, swing: None)").unwrap();
        assert_eq!(track.level, 0xFF);
        assert!(track.solo);
    }

    #[test]
    fn a_new_song_has_a_chain_and_phrase_per_track() {
        let song = Song::new(DEFAULT_TRACKS);
        assert_eq!(song.tracks.len(), DEFAULT_TRACKS);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].chains.len(), DEFAULT_TRACKS);
        assert_eq!(song.chains.len(), DEFAULT_TRACKS);
        assert_eq!(song.section_steps(&song.sections[0]), DEFAULT_PHRASE_STEPS);
        assert!(song.instruments.contains_key(&0));
    }

    #[test]
    fn chain_positions_cross_phrases_and_loop() {
        let mut song = Song::new(1);
        song.phrases.insert(7, Arc::new(Phrase::new(4)));
        song.chains.insert(
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
        assert_eq!(song.chain_steps(0), 20);
        let position = song.chain_position(0, 17).unwrap();
        assert_eq!((position.slot, position.phrase, position.step), (1, 7, 1));
        assert_eq!(position.transpose, 3);
        assert_eq!(song.chain_position(0, 20).unwrap().step, 0);
        assert_eq!(song.chain_position(99, 0), None);
    }

    #[test]
    fn hidden_steps_return_in_the_session_and_drop_on_save() {
        let mut phrase = Phrase::new(4);
        phrase.step_mut(3).unwrap().velocity = Some(1);
        phrase.set_len(2);
        assert_eq!(phrase.len(), 2);
        let saved = ron::to_string(&phrase).unwrap();
        let loaded: Phrase = ron::from_str(&saved).unwrap();
        assert_eq!(loaded.len(), 2);
        phrase.set_len(4);
        assert_eq!(phrase.steps()[3].velocity, Some(1));
        phrase.set_len(0);
        assert_eq!(phrase.len(), 1);
    }

    #[test]
    fn the_lowest_unused_id_fills_gaps() {
        let items = BTreeMap::from([(0, ()), (1, ()), (3, ())]);
        assert_eq!(lowest_unused(&items), 2);
        assert_eq!(lowest_unused(&BTreeMap::<u32, ()>::new()), 0);
    }
}
