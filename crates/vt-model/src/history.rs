//! Undo and redo, as a list of song versions that share unchanged data.

use std::sync::Arc;

use crate::constants::HISTORY_LIMIT;
use crate::song::Song;

/// The current song and the versions around it.
#[derive(Debug, Clone)]
pub struct History {
    current: Arc<Song>,
    past: Vec<Arc<Song>>,
    future: Vec<Arc<Song>>,
    limit: usize,
}

impl History {
    /// Start with a song and no undo steps.
    pub fn new(song: Arc<Song>) -> Self {
        Self::with_limit(song, HISTORY_LIMIT)
    }

    /// Start with a chosen number of undo steps kept.
    pub fn with_limit(song: Arc<Song>, limit: usize) -> Self {
        Self {
            current: song,
            past: Vec::new(),
            future: Vec::new(),
            limit,
        }
    }

    /// The current version.
    pub fn current(&self) -> &Arc<Song> {
        &self.current
    }

    /// Make a new version current, dropping the oldest past the limit, and clearing redo.
    pub fn commit(&mut self, song: Arc<Song>) {
        self.past.push(std::mem::replace(&mut self.current, song));
        if self.past.len() > self.limit {
            self.past.remove(0);
        }
        self.future.clear();
    }

    /// Go back one version, if there is one.
    pub fn undo(&mut self) -> Option<&Arc<Song>> {
        let previous = self.past.pop()?;
        self.future
            .push(std::mem::replace(&mut self.current, previous));
        Some(&self.current)
    }

    /// Go forward one version, if there is one.
    pub fn redo(&mut self) -> Option<&Arc<Song>> {
        let next = self.future.pop()?;
        self.past.push(std::mem::replace(&mut self.current, next));
        Some(&self.current)
    }

    /// Take back the last version as if it was never made, leaving nothing to redo, as for an edit
    /// that turns out not to have been meant.
    pub fn take_back(&mut self) -> Option<&Arc<Song>> {
        self.current = self.past.pop()?;
        Some(&self.current)
    }

    /// Change the current version without adding an undo step, while something is being previewed.
    pub fn replace_current(&mut self, song: Arc<Song>) {
        self.current = song;
    }

    /// Make everything since an earlier undo depth a single undo step, keeping the current version.
    pub fn squash_to(&mut self, depth: usize) {
        self.past.truncate(depth + 1);
    }

    /// How many undo steps are held.
    pub fn undo_depth(&self) -> usize {
        self.past.len()
    }
}
