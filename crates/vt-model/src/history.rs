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

    /// Take back the last version as if it was never made, as for an edit that turns out not to
    /// have been meant: the one before becomes current and the one taken back is dropped, not kept
    /// to redo. What was already there to redo stays, so call it straight after the commit it
    /// takes back, when there's nothing to redo.
    pub fn take_back(&mut self) -> Option<&Arc<Song>> {
        self.current = self.past.pop()?;
        Some(&self.current)
    }

    /// Change the current version without adding an undo step, while something is being previewed.
    pub fn replace_current(&mut self, song: Arc<Song>) {
        self.current = song;
    }

    /// Change every version, past, current and to come, that a change says it changes, without
    /// adding an undo step: for what's learned about something after the edit that made it, as a
    /// recording's length is once it stops, so going back and forth never finds it half made.
    pub fn amend(&mut self, mut change: impl FnMut(&Song) -> Option<Song>) {
        let versions = self
            .past
            .iter_mut()
            .chain(std::iter::once(&mut self.current))
            .chain(self.future.iter_mut());
        for version in versions {
            if let Some(changed) = change(version) {
                *version = Arc::new(changed);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amending_changes_every_version_it_says_without_an_undo_step() {
        let mut song = Song::new(1);
        song.millibpm = 90_000;
        let mut history = History::new(Arc::new(song.clone()));
        for millibpm in [100_000, 110_000, 120_000] {
            song.millibpm = millibpm;
            history.commit(Arc::new(song.clone()));
        }
        history.undo();
        let depth = history.undo_depth();
        // Every version faster than 100 BPM goes 5 faster.
        history.amend(|song| {
            (song.millibpm > 100_000).then(|| {
                let mut faster = song.clone();
                faster.millibpm += 5000;
                faster
            })
        });
        assert_eq!(history.undo_depth(), depth);
        assert_eq!(history.current().millibpm, 115_000);
        assert_eq!(history.redo().unwrap().millibpm, 125_000);
        history.undo();
        history.undo();
        assert_eq!(history.current().millibpm, 100_000);
    }
}
