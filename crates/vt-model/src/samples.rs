//! The session's sample pool.
//!
//! The pool only grows during a session and IDs stay stable, so a playing voice can keep its
//! sample until it ends. Replaced samples stay in memory until the project reloads.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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

    /// The pool's frame for a frame of the file, to the nearest.
    pub fn loaded_frame(self, frame: u64) -> u64 {
        let file = u128::from(self.file.max(1));
        ((u128::from(frame) * u128::from(self.loaded) + file / 2) / file) as u64
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

/// Every sample loaded this session.
#[derive(Debug, Clone, Default)]
pub struct SamplePool {
    samples: Vec<Arc<SampleData>>,
    /// Each sample's rates, in the same order.
    rates: Vec<Rates>,
    by_path: HashMap<String, SampleId>,
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
        let id = self.samples.len() as SampleId;
        self.samples.push(Arc::new(data));
        self.rates.push(rates);
        self.by_path.insert(path.to_owned(), id);
        id
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

    /// A sample by ID.
    pub fn get(&self, id: SampleId) -> Option<&SampleData> {
        self.samples.get(id as usize).map(Arc::as_ref)
    }

    /// A sample by ID, shared, so whoever holds it can tell it from another later given the same ID.
    pub fn shared(&self, id: SampleId) -> Option<&Arc<SampleData>> {
        self.samples.get(id as usize)
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
