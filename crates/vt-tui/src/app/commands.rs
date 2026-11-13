//! The command line's commands, and the projects and samples they save, open and load.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::explorer::Purpose;
use crate::folding::Folds;
use vt_engine::link::Command;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::action::Action;
use vt_model::constants::DEFAULT_TRACKS;
use vt_model::history::History;
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
use vt_model::song::{Lane, Song};

use crate::input::command::Line;

use super::{App, Cursor, UNSAVED_NEW, UNSAVED_OPEN, folder_name, live_from, name_song};

impl App {
    /// Run a command line.
    pub fn line(&mut self, line: Line) {
        self.status.clear();
        match line {
            Line::Quit { force } => self.quit(force),
            Line::Write(None) if self.folder.is_none() => {
                self.open_naming_explorer(Purpose::Save, "w", String::new());
            }
            Line::Write(path) => self.write(path),
            Line::Edit {
                path: Some(path),
                force,
            } => self.open(&path, force),
            Line::Edit { path: None, force } => self.open_project_explorer(force),
            Line::New {
                path: Some(path),
                force,
            } => self.new_project(&path, force),
            Line::New { path: None, force } if self.dirty && !force => {
                self.status = UNSAVED_NEW.to_owned();
            }
            Line::New { path: None, force } => {
                let command = if force { "new!" } else { "new" };
                self.open_naming_explorer(Purpose::Save, command, String::new());
            }
            Line::Sample(Some(path)) => self.sample(&path),
            Line::Sample(None) => self.open_default_sample_picker(),
            Line::SampleChange(change) => self.change_sample(change),
            Line::Slice(change, range) => self.change_slices(change, range),
            Line::Mark { letter, slice } => self.set_mark(letter, slice),
            Line::Marks => self.open_marks_picker(),
            Line::DelMarks(letters) => self.delete_marks(letters.as_deref()),
            Line::Kit(Some(name)) => self.use_kit(&self.kit_path(&name)),
            Line::Bank => self.open_bank_picker(),
            Line::BankClean => self.clean_bank(),
            Line::Normalize(level) => self.normalize_sample(level),
            Line::Trim => self.trim_sample(),
            Line::KitTracks(name) => self.load_kit(&self.kit_path(&name)),
            Line::Kit(None) => self.open_kit_picker(),
            Line::Name(name) => self.apply(&Action::SetTrackName {
                track: self.cursor.track,
                name: name.unwrap_or_default(),
            }),
            Line::Bpm(millibpm) => self.apply(&Action::SetMilliBpm(millibpm)),
            Line::GrabTempo => self.grab_tempo(),
            Line::TrackAdd(count) => {
                let was = self.song().sound_tracks();
                self.apply(&Action::AddTracks(count));
                self.fit_levels();
                // A cursor on a fixed track stays on it.
                if self.cursor.track >= was {
                    self.cursor.track += self.song().sound_tracks() - was;
                }
            }
            Line::TrackDel => {
                let was = self.song().sound_tracks();
                self.apply(&Action::RemoveLastTrack);
                self.fit_levels();
                let now = self.song().sound_tracks();
                if self.cursor.track >= was {
                    self.cursor.track -= was - now;
                } else if self.cursor.track >= now {
                    self.cursor.track = now - 1;
                    self.cursor.slot = 0;
                    self.clamp_cursor();
                }
            }
            Line::Chain(chain) => self.set_track_chain(chain),
            Line::Group { group, tracks } => self.set_group(group, tracks),
            Line::Render {
                path: None,
                rows,
                tail,
                stems,
                each_track,
            } => {
                // The flags are kept, after the name.
                let mut after = String::new();
                if let Some(rows) = rows {
                    after.push_str(&format!(" --rows {rows}"));
                }
                if tail {
                    after.push_str(" --tail");
                }
                if stems {
                    after.push_str(" --stems");
                }
                if each_track {
                    after.push_str(" --split");
                }
                self.open_naming_explorer(Purpose::Render, "render", after);
            }
            Line::Render {
                path: Some(path),
                rows,
                tail,
                stems: true,
                each_track,
            } => self.render_stems(
                &path,
                RenderOptions {
                    rows,
                    tail,
                    each_track,
                },
            ),
            Line::Render {
                path: Some(path),
                rows,
                tail,
                ..
            } => self.render(&path, rows, tail),
            Line::Double => self.double(),
            Line::GenApply => self.apply_generator_here(),
            Line::SetSkip(size) => self.apply(&Action::SetSkipSize(size)),
            Line::SetScale(kind) => self.set_scale(Scale {
                on: true,
                kind,
                ..self.scale
            }),
            Line::SetRoot(root) => self.set_scale(Scale {
                on: true,
                root,
                ..self.scale
            }),
            Line::ScaleOn(on) => self.set_scale(Scale { on, ..self.scale }),
            Line::PickScale => self.open_scale_picker(),
            Line::FitToScale => self.fit_to_scale(),
            Line::SetLimiter(on) => {
                self.apply(&Action::SetLimiter(on));
                self.status = if on {
                    "Limiter on".to_owned()
                } else {
                    "Limiter off: renders keep levels over 0, and the output still clips at 0"
                        .to_owned()
                };
            }
            Line::Seed(seed) => self.apply(&Action::SetSeed(seed)),
            Line::Groove(groove) => {
                self.apply(&Action::SetGroove(groove));
                self.status = format!("The project plays with {groove}");
            }
            Line::TrackGroove(groove) => self.set_track_groove(groove),
            Line::PickGroove { track } => self.open_groove_picker(track),
            Line::Reset(reset) => self.apply(&Action::SetReset {
                section: self.cursor.section,
                reset,
            }),
            Line::Loop => self.toggle_loop(),
            Line::Launch(Some(launch)) => {
                self.apply(&Action::SetLaunch(launch));
                self.status = format!("Launches start {}", launch.when());
            }
            Line::Launch(None) => {
                let launch = self.song().launch;
                self.status = format!("Launches start {}", launch.when());
            }
            Line::Label(label) => self.label_chain(label),
            Line::MacroName(name) => self.name_macro(&name),
            Line::TrackLevel(level) => self.apply(&Action::SetTrackLevel {
                track: self.cursor.track,
                level,
            }),
            Line::Mute => self.toggle_mute(),
            Line::Solo => self.toggle_solo(),
            Line::Len(length) if self.lane_slot().is_some() => {
                let slot = self.lane_slot().unwrap_or(0);
                let start = self.lane_here(slot).start;
                self.set_lane(slot, Some(Lane { start, length }));
            }
            Line::Lane(length) => match self.fx_column() {
                Some(slot) => {
                    let start = self.cursor.step;
                    self.set_lane(slot, length.map(|length| Lane { start, length }));
                }
                None => self.status = ":lane works on an FX column".to_owned(),
            },
            Line::Order(Some(order)) => self.set_order(order),
            Line::Order(None) => self.open_order_picker(),
            Line::Len(length) => {
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::SetLength {
                        track: self.cursor.track,
                        phrase,
                        length,
                    });
                    self.clamp_cursor();
                }
            }
        }
    }

    /// Render what is in memory, with the live mutes and at the device's rate. The screen waits,
    /// and audio keeps playing on its own thread. It waits for the bank's analyses, so a
    /// normalised sample renders the same however far they'd got.
    fn render(&mut self, path: &Path, rows: Option<usize>, tail: bool) {
        self.finish_analysis();
        let version = Published::new(self.song_to_save(), Arc::clone(&self.samples));
        let options = RenderOptions {
            rows,
            tail,
            ..RenderOptions::default()
        };
        let samples = render_loop(Arc::new(version), self.rate, options);
        self.status = match write_wav(path, self.rate, &samples) {
            Ok(()) => format!(
                "Rendered {} ({:.2} s)",
                path.display(),
                samples.len() as f64 / 2.0 / f64::from(self.rate)
            ),
            Err(error) => format!("Couldn't write {}: {error}", path.display()),
        };
    }

    /// Render one file per group, and per track in none, that would be heard, into a folder, or
    /// one per track if asked.
    fn render_stems(&mut self, folder: &Path, options: RenderOptions) {
        self.finish_analysis();
        let version = Published::new(self.song_to_save(), Arc::clone(&self.samples));
        self.status = match crate::run::write_stems(&version, folder, self.rate, options) {
            Ok(count) => crate::run::stems_written(count, folder),
            Err(error) => format!("Couldn't write {}: {error}", folder.display()),
        };
    }

    pub(super) fn quit(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = "Unsaved changes: use :w to save, or :q! to quit anyway".to_owned();
        } else {
            self.forget_autosave();
            self.should_quit = true;
        }
    }

    fn write(&mut self, path: Option<PathBuf>) {
        let same = path
            .as_deref()
            .is_none_or(|p| Some(p) == self.folder.as_deref());
        let result = if same {
            match self.folder.clone() {
                Some(folder) => {
                    project::save_song(&folder, &self.song_to_save()).map(|()| (folder, None))
                }
                None => {
                    self.status = "This project has no folder yet: use :w path".to_owned();
                    return;
                }
            }
        } else {
            // A project saved to a new folder is named after it, which the line saving it starts
            // with, so the two only differ if another name is typed there.
            let target = path.expect("a different path is present");
            let mut song = Song::clone(&self.song_to_save());
            song.name = folder_name(&target);
            project::save_as(self.folder.as_deref(), &target, &song)
                .map(|saved| (target, Some(saved)))
        };
        match result {
            Ok((folder, saved)) => {
                if let Some(saved) = saved {
                    self.replace_song(saved, Some(folder.clone()));
                }
                self.folder = Some(folder.clone());
                self.dirty = false;
                self.forget_autosave();
                self.status = format!("Saved {}", folder.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Open a project. Forced, unsaved changes are thrown away, autosave and all, as `:q!` does.
    fn open(&mut self, path: &Path, force: bool) {
        if self.dirty && !force {
            self.status = UNSAVED_OPEN.to_owned();
            return;
        }
        match project::load_song(path) {
            Ok(song) => {
                self.discard_changes();
                self.handle.send(Command::Return);
                self.give_back_on_opening();
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                if self.status.is_empty() {
                    self.status = format!("Opened {}", path.display());
                }
                self.offer_restore();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Start a project. Forced, unsaved changes are thrown away, autosave and all.
    fn new_project(&mut self, path: &Path, force: bool) {
        if self.dirty && !force {
            self.status = UNSAVED_NEW.to_owned();
            return;
        }
        let song = Song {
            name: folder_name(path),
            ..Song::new(DEFAULT_TRACKS)
        };
        match project::create_project(path, &song) {
            Ok(()) => {
                self.discard_changes();
                self.handle.send(Command::Return);
                self.give_back_on_opening();
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                self.status = format!("Created {}", path.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Throw away unsaved changes before another project replaces them, so their autosave isn't
    /// offered back the next time this project opens.
    fn discard_changes(&mut self) {
        if self.dirty {
            self.forget_autosave();
        }
    }

    /// Replace the song and undo history, loading any samples it needs.
    pub(crate) fn replace_song(&mut self, mut song: Song, folder: Option<PathBuf>) {
        name_song(&mut song, folder.as_deref(), &mut self.names);
        let notes = project::find_sources(folder.as_deref(), &mut song);
        let mut pool = SamplePool::clone(&self.samples);
        let errors = project::load_samples(folder.as_deref(), &song, self.rate, &mut pool);
        self.samples = Arc::new(pool);
        self.levels = live_from(&song);
        self.scale = song.scale;
        self.auto_jump = song.auto_jump;
        self.send_live();
        self.history = History::new(Arc::new(song));
        // Another song's rows aren't these.
        self.heard_pitches.clear();
        self.folder = folder;
        self.folds = Folds::default();
        self.cursor = Cursor {
            step: self.cursor.step,
            ..self.cursor
        };
        if self.phrase_id().is_none() {
            self.cursor.track = 0;
            self.cursor.slot = 0;
        }
        self.clamp_cursor();
        if let Some(first) = notes.first() {
            self.status = first.clone();
        }
        if let Some(first) = errors.first() {
            self.status = first.to_string();
        }
        self.publish();
    }

    /// How a song refers to a sample: where it is, or relative to the project if it's inside,
    /// so a bundled project can be moved. `:w` to a new folder is what copies samples.
    pub(crate) fn store_sample(&self, source: &Path) -> Result<String, ProjectError> {
        let full = source.canonicalize().map_err(|error| ProjectError::Io {
            path: source.display().to_string(),
            source: error,
        })?;
        let project = self.folder.as_deref().and_then(|f| f.canonicalize().ok());
        let inside = project.as_deref().and_then(|f| full.strip_prefix(f).ok());
        Ok(inside.unwrap_or(&full).display().to_string())
    }

    /// Put a file in the bank and have the current track play it.
    pub(crate) fn sample(&mut self, source: &Path) {
        self.sample_named(source, None);
    }

    /// Put a file in the bank, have the current track play it, and name the track after it, if
    /// there's a name, as one undo step.
    pub(crate) fn sample_named(&mut self, source: &Path, name: Option<&str>) {
        let track = self.cursor.track;
        let more = name
            .map(|name| Action::SetTrackName {
                track,
                name: name.to_owned(),
            })
            .into_iter()
            .collect();
        self.sample_with(source, None, more);
    }
}
