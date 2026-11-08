//! The session's sample pool.
//!
//! The pool only grows during a session and IDs stay stable, so a playing voice can keep its
//! sample until it ends. Replaced samples stay in memory until the project reloads.

use std::collections::HashMap;
use std::sync::Arc;

use crate::sample::SampleData;

/// A sample's stable place in the pool.
pub type SampleId = u32;

/// Every sample loaded this session.
#[derive(Debug, Clone, Default)]
pub struct SamplePool {
    samples: Vec<Arc<SampleData>>,
    by_path: HashMap<String, SampleId>,
}

impl SamplePool {
    /// Add a sample, or return its ID if this path is already loaded.
    pub fn insert(&mut self, path: &str, data: SampleData) -> SampleId {
        if let Some(id) = self.by_path.get(path) {
            return *id;
        }
        let id = self.samples.len() as SampleId;
        self.samples.push(Arc::new(data));
        self.by_path.insert(path.to_owned(), id);
        id
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
