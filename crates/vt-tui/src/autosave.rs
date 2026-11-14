//! Writing unsaved work aside now and then, and offering it back after a crash.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::AUTOSAVE_SECONDS;
use vt_model::project;

use crate::app::App;
use crate::input::keys::Key;
use crate::input::mode::Mode;

impl App {
    /// Write the autosave file if it's due and something changed since the last one. A project
    /// whose folder doesn't exist yet has nowhere to write until the first `:w`.
    pub fn autosave_if_due(&mut self, now: Instant) {
        let every = Duration::from_secs(AUTOSAVE_SECONDS);
        let started = *self.last_autosave.get_or_insert(now);
        if now.saturating_duration_since(started) < every {
            return;
        }
        self.last_autosave = Some(now);
        let Some(folder) = self.folder.clone().filter(|folder| folder.is_dir()) else {
            return;
        };
        if !self.dirty {
            return;
        }
        let song = self.song_to_save();
        if self.autosaved.as_ref() == Some(&song) {
            return;
        }
        match project::write_autosave(&folder, &song) {
            Ok(()) => self.autosaved = Some(song),
            Err(error) => self.status = format!("Autosave failed: {error}"),
        }
    }

    /// Remove the autosave file, once the work in it is saved or was thrown away on purpose.
    pub(crate) fn forget_autosave(&mut self) {
        self.autosaved = None;
        if let Some(folder) = &self.folder {
            project::remove_autosave(folder);
        }
    }

    /// If this project's autosave is newer than its song, ask whether to restore it.
    pub fn offer_restore(&mut self) {
        let Some(folder) = &self.folder else {
            return;
        };
        if project::newer_autosave(folder) {
            self.status = "Found newer unsaved work: y restores it, n deletes it".to_owned();
            self.mode = Mode::Restore;
        }
    }

    /// Only `y` and `n` answer, so a stray key can't throw work away.
    pub(crate) fn restore_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Char('y') => {
                self.mode = Mode::Normal;
                self.status.clear();
                self.restore_autosave();
            }
            KeyCode::Char('n') => {
                self.mode = Mode::Normal;
                self.status = "Deleted the unsaved work".to_owned();
                self.forget_autosave();
            }
            _ => {}
        }
    }

    /// A file that can't be read is left in place, so a newer build can't destroy it.
    fn restore_autosave(&mut self) {
        let Some(folder) = self.folder.clone() else {
            return;
        };
        match project::load_autosave(&folder) {
            Ok(song) => {
                self.replace_song(song, Some(folder));
                self.dirty = true;
                self.autosaved = Some(Arc::clone(self.song()));
                // Restoring only puts the work back unsaved, so a read-only song's file is left as
                // it is until it's written over on purpose (decision 313).
                if self.status.is_empty() {
                    self.status = if self.read_only {
                        "Restored unsaved work to a read-only song: :w path keeps a copy, :w! writes over it"
                    } else {
                        "Restored unsaved work: :w keeps it"
                    }
                    .to_owned();
                }
            }
            Err(error) => {
                self.status = format!("Couldn't restore, so the file was kept: {error}");
            }
        }
    }
}
