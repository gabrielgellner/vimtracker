//! The session's sample pool.
//!
//! The pool only grows during a session and IDs stay stable, so a playing voice can keep its
//! sample until it ends. Replaced samples stay in memory until the project reloads.
//!
//! A sample being recorded is in the pool too, as frames that grow while they're played, until
//! it's finished and its sound takes their place under the same ID, so a voice playing it carries
//! on.
//!
//! It also keeps each file's analysis, by content, once it's read or worked out.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::analysis::Analysis;
use crate::bank::ContentHash;
use crate::growing::Growing;
use crate::sample::{SampleData, SampleError, load_sample_with_info};

/// A sample's stable place in the pool.
pub type SampleId = u32;

/// The rate a sample's file was recorded at and the rate it was converted to, so a point in the
/// file's own frames can be found in the pool's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    pub file: u32,
    pub loaded: u32,
}

impl Rates {
    /// The same rate on both sides, for a sample made at the program's rate.
    pub const SAME: Self = Self { file: 1, loaded: 1 };

    /// The pool's frame for a frame of the file, to the nearest, or the last there could be for
    /// one too far on to count.
    pub fn loaded_frame(self, frame: u64) -> u64 {
        let file = u128::from(self.file.max(1));
        let loaded = (u128::from(frame) * u128::from(self.loaded) + file / 2) / file;
        u64::try_from(loaded).unwrap_or(u64::MAX)
    }

    /// The file's frame for a frame of the pool's, to the nearest.
    pub fn file_frame(self, frame: u64) -> u64 {
        Rates {
            file: self.loaded,
            loaded: self.file,
        }
        .loaded_frame(frame)
    }
}

/// A sample's sound: loaded whole, or still being written.
#[derive(Debug, Clone)]
pub enum Sound {
    Loaded(Arc<SampleData>),
    Growing(Arc<Growing>),
}

/// Loaded sounds are equal by their frames, and growing ones only if they're the same.
impl PartialEq for Sound {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Loaded(one), Self::Loaded(other)) => one == other,
            (Self::Growing(one), Self::Growing(other)) => Arc::ptr_eq(one, other),
            _ => false,
        }
    }
}

impl Sound {
    /// Whether it's the very same sound as another, not only the same frames.
    pub fn same(&self, other: &Sound) -> bool {
        match (self, other) {
            (Self::Loaded(one), Self::Loaded(other)) => Arc::ptr_eq(one, other),
            (Self::Growing(one), Self::Growing(other)) => Arc::ptr_eq(one, other),
            _ => false,
        }
    }

    /// Its frames, to read.
    #[inline]
    pub(crate) fn frames(&self) -> Frames<'_> {
        match self {
            Self::Loaded(data) => Frames::Loaded(&data.frames),
            Self::Growing(growing) => Frames::Growing(growing),
        }
    }

    /// The whole sound, once it's loaded.
    pub fn loaded(&self) -> Option<&Arc<SampleData>> {
        match self {
            Self::Loaded(data) => Some(data),
            Self::Growing(_) => None,
        }
    }
}

/// A sample's frames as a voice reads them, loaded or growing.
#[derive(Debug, Clone, Copy)]
pub enum Frames<'a> {
    Loaded(&'a [[f32; 2]]),
    Growing(&'a Growing),
}

impl<'a> From<&'a SampleData> for Frames<'a> {
    fn from(data: &'a SampleData) -> Self {
        Self::Loaded(&data.frames)
    }
}

impl Frames<'_> {
    /// A frame and the one after it, for interpolating between them, if the first is there: the
    /// first again for the second at the end. A growing sample has only what's written.
    #[inline]
    pub fn pair(self, index: usize) -> Option<([f32; 2], [f32; 2])> {
        match self {
            Self::Loaded(frames) => {
                let first = *frames.get(index)?;
                Some((first, frames.get(index + 1).copied().unwrap_or(first)))
            }
            Self::Growing(growing) => growing.pair(index),
        }
    }
}

/// Every sample loaded this session.
#[derive(Debug, Clone, Default)]
pub struct SamplePool {
    samples: Vec<Sound>,
    /// Each sample's rates, in the same order.
    rates: Vec<Rates>,
    by_path: HashMap<String, SampleId>,
    analyses: HashMap<ContentHash, Arc<Analysis>>,
}

impl SamplePool {
    /// The analysis of a file's content, once it's in.
    pub fn analysis(&self, hash: ContentHash) -> Option<&Arc<Analysis>> {
        self.analyses.get(&hash)
    }

    /// Keep an analysis, by the content it's of.
    pub fn insert_analysis(&mut self, analysis: Arc<Analysis>) {
        self.analyses.insert(analysis.hash, analysis);
    }

    /// Keep every analysis another pool has, as one loaded again at another rate can, since an
    /// analysis is of the file at its own rate.
    pub fn keep_analyses(&mut self, other: &SamplePool) {
        for analysis in other.analyses.values() {
            self.analyses
                .entry(analysis.hash)
                .or_insert_with(|| Arc::clone(analysis));
        }
    }
}

impl SamplePool {
    /// Add a sample made at the program's rate, or return its ID if this path is already loaded.
    pub fn insert(&mut self, path: &str, data: SampleData) -> SampleId {
        self.insert_converted(path, data, Rates::SAME)
    }

    /// Add a sample converted from its file's rate, or return its ID if this path is already
    /// loaded.
    pub fn insert_converted(&mut self, path: &str, data: SampleData, rates: Rates) -> SampleId {
        if let Some(id) = self.by_path.get(path) {
            return *id;
        }
        self.insert_sound(path, Sound::Loaded(Arc::new(data)), rates)
    }

    /// Add a sample that's still being written, at the program's rate, or return the ID of what's
    /// already loaded at this path.
    pub fn insert_growing(&mut self, path: &str, growing: Arc<Growing>) -> SampleId {
        self.insert_sound(path, Sound::Growing(growing), Rates::SAME)
    }

    fn insert_sound(&mut self, path: &str, sound: Sound, rates: Rates) -> SampleId {
        if let Some(id) = self.by_path.get(path) {
            return *id;
        }
        let id = self.samples.len() as SampleId;
        self.samples.push(sound);
        self.rates.push(rates);
        self.by_path.insert(path.to_owned(), id);
        id
    }

    /// Put a growing sample's finished sound in its place, under the same ID, so a voice playing
    /// it carries on. Returns false if the ID isn't a growing sample.
    pub fn finish_growing(&mut self, id: SampleId, data: SampleData) -> bool {
        match self.samples.get_mut(id as usize) {
            Some(sound @ Sound::Growing(_)) => {
                *sound = Sound::Loaded(Arc::new(data));
                true
            }
            _ => false,
        }
    }

    /// Forget a path, as for a recording dropped, so nothing new finds it. Its sound stays under
    /// its ID, as a replaced sample's does, until the pool is made again.
    pub fn forget(&mut self, path: &str) {
        self.by_path.remove(path);
    }

    /// Read a file at the program's rate and add it under a path, keeping its own rate, unless the
    /// path is already loaded.
    pub fn load(&mut self, path: &str, file: &Path, rate: u32) -> Result<SampleId, SampleError> {
        if let Some(id) = self.id_of(path) {
            return Ok(id);
        }
        let (data, info) = load_sample_with_info(file, rate)?;
        let rates = Rates {
            file: info.rate,
            loaded: rate,
        };
        Ok(self.insert_converted(path, data, rates))
    }

    /// The rates a sample was converted between, or the same rate on both sides for one not
    /// loaded.
    pub fn rates(&self, id: SampleId) -> Rates {
        self.rates.get(id as usize).copied().unwrap_or(Rates::SAME)
    }

    /// The ID for a path, if loaded.
    pub fn id_of(&self, path: &str) -> Option<SampleId> {
        self.by_path.get(path).copied()
    }

    /// A sample by ID, once it's loaded whole.
    pub fn get(&self, id: SampleId) -> Option<&SampleData> {
        self.shared(id).map(Arc::as_ref)
    }

    /// A sample by ID, shared, so whoever holds it can tell it from another later given the same
    /// ID, once it's loaded whole.
    pub fn shared(&self, id: SampleId) -> Option<&Arc<SampleData>> {
        self.samples.get(id as usize)?.loaded()
    }

    /// A sample's sound by ID, loaded or growing.
    pub fn sound(&self, id: SampleId) -> Option<&Sound> {
        self.samples.get(id as usize)
    }

    /// A sample's frames by ID, loaded or growing, as a voice reads them.
    #[inline]
    pub fn frames(&self, id: SampleId) -> Option<Frames<'_>> {
        self.samples.get(id as usize).map(Sound::frames)
    }

    /// The number of samples loaded.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Whether no samples are loaded.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
