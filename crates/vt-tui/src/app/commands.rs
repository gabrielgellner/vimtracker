//! The command line's commands, and the projects and samples they save, open and load.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::overlays::picker::explorer::Purpose;
use crate::services::folding::Folds;
use vt_engine::link::Command;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::action::Action;
use vt_model::constants::DEFAULT_TRACKS;
use vt_model::history::History;
use vt_model::project;
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
use vt_model::song::{Lane, Song};

use crate::overlays::command::parse::{Line, PatchLine, PresetLine};

use super::{App, READ_ONLY, folder_name, name_song};
use crate::pages::phrase::PhraseView;
use crate::pages::track::editing::{set_limiter, set_track_groove};
use crate::pages::track::patches::factory_root;
use crate::services::Cursor;
use crate::services::live_from;
use vt_model::preset::PresetKind;

impl App {
    /// Run a command line.
    pub fn line(&mut self, line: Line) {
        self.services.status.clear();
        match line {
            Line::Quit { force } => self.quit(force),
            Line::Write { path: None, .. } if self.services.folder.is_none() => {
                self.open_naming_explorer(Purpose::Save, "w", String::new());
            }
            Line::Write { path, force } => self.write(path, force),
            Line::Edit {
                path: Some(path),
                force,
            } => self.open(&path, force),
            Line::Edit { path: None, force } => self.open_project_explorer(force),
            Line::New {
                path: Some(path),
                force,
            } => self.new_project(&path, force),
            Line::New { path: None, force } if self.services.dirty && !force => {
                self.services.status = self.unsaved(":new! to discard them");
            }
            Line::New { path: None, force } => {
                let command = if force { "new!" } else { "new" };
                self.open_naming_explorer(Purpose::Save, command, String::new());
            }
            Line::Sample(Some(path)) => self.sample(&path),
            Line::Sample(None) => self.open_default_sample_picker(),
            Line::SampleChange(change) => self.services.change_sample(change),
            Line::Slice(change, range) => {
                let recording = self.sampling_shown();
                self.sample_page.change_slices(
                    change,
                    range,
                    self.modes.view,
                    recording,
                    &mut self.services,
                );
            }
            Line::Mark { letter, slice } => {
                self.sample_page
                    .set_mark(letter, slice, self.modes.view, &mut self.services);
            }
            Line::Marks => {
                self.modes.count = None;
                match self
                    .sample_page
                    .marks_picker(self.modes.view, &self.services)
                {
                    Ok(picker) => self.show_picker(picker),
                    Err(why) => self.services.status = why,
                }
            }
            Line::DelMarks(letters) => {
                self.sample_page.delete_marks(
                    letters.as_deref(),
                    self.modes.view,
                    &mut self.services,
                );
            }
            Line::Kit(Some(name)) => self.use_kit(&self.services.kit_path(&name)),
            Line::Bank => self.open_bank_picker(),
            Line::BankClean => self.clean_bank(),
            Line::Normalize(level) => self.services.normalize_sample(level),
            Line::Trim => self.services.trim_sample(),
            Line::Record { what, quantise } => self.record_command(what, quantise),
            Line::RecordStop => self.stop_sampling(),
            Line::RecordCancel => self.cancel_sampling(),
            Line::KitTracks(name) => self.load_kit(&self.services.kit_path(&name)),
            Line::Kit(None) => self.open_kit_explorer(),
            Line::Name(name) => self.name_track(name),
            Line::Bpm(millibpm) => self.services.apply(&Action::SetMilliBpm(millibpm)),
            Line::GrabTempo => self.services.grab_tempo(),
            Line::TrackAdd(count) => {
                let was = self.song().sound_tracks();
                self.services.apply(&Action::AddTracks(count));
                self.fit_levels();
                // A cursor on a fixed track stays on it.
                if self.services.cursor.track >= was {
                    self.services.cursor.track += self.song().sound_tracks() - was;
                }
            }
            Line::TrackDel => {
                let was = self.song().sound_tracks();
                self.services.apply(&Action::RemoveLastTrack);
                self.fit_levels();
                let now = self.song().sound_tracks();
                if self.services.cursor.track >= was {
                    self.services.cursor.track -= was - now;
                } else if self.services.cursor.track >= now {
                    self.services.cursor.track = now - 1;
                    self.services.cursor.slot = 0;
                    self.services.clamp_cursor(self.modes.view);
                }
            }
            Line::Chain(chain) => self.set_track_chain(chain),
            Line::Group { group, tracks } => self.set_group(group, tracks),
            Line::GroupName(name) => self.name_group(name),
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
            Line::GenApply => self.services.apply_generator_here(),
            Line::SetSkip(size) => self.services.apply(&Action::SetSkipSize(size)),
            Line::SetScale(kind) => self.services.set_scale(Scale {
                on: true,
                kind,
                ..self.services.live.scale
            }),
            Line::SetRoot(root) => self.services.set_scale(Scale {
                on: true,
                root,
                ..self.services.live.scale
            }),
            Line::ScaleOn(on) => self.services.set_scale(Scale {
                on,
                ..self.services.live.scale
            }),
            Line::PickScale => self.open_picker(self.services.scale_picker()),
            Line::FitToScale => self.fit_to_scale(),
            Line::SetLimiter(on) => set_limiter(&mut self.services, on),
            Line::ReadOnly(on) => self.set_read_only(on),
            Line::Seed(seed) => self.services.apply(&Action::SetSeed(seed)),
            Line::RollSeed => {
                let seed = crate::services::random_seed();
                self.services.apply(&Action::SetSeed(seed));
                self.services.status = format!("Seed {seed:08X}");
            }
            Line::Groove(groove) => {
                self.services.apply(&Action::SetGroove(groove));
                self.services.status = format!("The project plays with {groove}");
            }
            Line::TrackGroove(groove) => set_track_groove(&mut self.services, groove),
            Line::PickGroove { track } => self.open_picker(self.services.groove_picker(track)),
            Line::Reset(reset) => self.services.apply(&Action::SetReset {
                section: self.services.cursor.section,
                reset,
            }),
            Line::Loop => self
                .song_view
                .toggle_loop(self.modes.view, &mut self.services),
            Line::Launch(Some(launch)) => {
                self.services.apply(&Action::SetLaunch(launch));
                self.services.status = format!("Launches start {}", launch.when());
            }
            Line::Launch(None) => {
                let launch = self.song().launch;
                self.services.status = format!("Launches start {}", launch.when());
            }
            Line::Label(label) => {
                self.song_view
                    .label_chain(self.modes.view, label, &mut self.services);
            }
            Line::MacroName(name) => {
                let control = self.track_control();
                let open = self.overlays.mapping.as_ref().map(|view| view.at);
                self.services
                    .name_macro(&name, open, control, self.modes.view);
            }
            Line::Preset(line) => self.preset_line(line),
            Line::Patch(line) => self.patch_line(line),
            Line::TrackLevel(level) => self.services.apply(&Action::SetTrackLevel {
                track: self.services.cursor.track,
                level,
            }),
            Line::Mute => self.services.toggle_mute(),
            Line::Solo => self.services.toggle_solo(),
            Line::Len(length) if self.lane_slot().is_some() => {
                let slot = self.lane_slot().unwrap_or(0);
                let lane = self.lane_here(slot);
                self.set_lane(slot, Some(Lane { length, ..lane }));
            }
            Line::Lane(lane) => self.lane_line(lane),
            Line::Order(Some(order)) => self.services.set_order(order),
            Line::Order(None) => {
                self.settled(PhraseView::order_picker(&self.services), true);
            }
            Line::Bloom {
                branches: None,
                mutation: None,
                path: None,
            } => self.open_bloom_picker(),
            Line::Bloom {
                branches,
                mutation,
                path,
            } => self.grow_bloom(branches, mutation, path.unwrap_or_default()),
            Line::Len(length) => {
                if let Some(phrase) = self.services.phrase_id() {
                    self.services.apply(&Action::SetLength {
                        track: self.services.cursor.track,
                        phrase,
                        length,
                    });
                    self.services.clamp_cursor(self.modes.view);
                }
            }
        }
    }

    /// Render what is in memory, with the live mutes and at the device's rate. The screen waits,
    /// and audio keeps playing on its own thread. It waits for the bank's analyses, so a
    /// normalised sample renders the same however far they'd got.
    fn render(&mut self, path: &Path, rows: Option<usize>, tail: bool) {
        self.services.finish_analysis();
        let version = Published::new(self.song_to_save(), Arc::clone(&self.services.samples));
        let options = RenderOptions {
            rows,
            tail,
            ..RenderOptions::default()
        };
        let samples = render_loop(Arc::new(version), self.services.rate, options);
        self.services.status = match write_wav(path, self.services.rate, &samples) {
            Ok(()) => format!(
                "Rendered {} ({:.2} s)",
                path.display(),
                samples.len() as f64 / 2.0 / f64::from(self.services.rate)
            ),
            Err(error) => format!("Couldn't write {}: {error}", path.display()),
        };
    }

    /// Render one file per group, and per track in none, that would be heard, into a folder, or
    /// one per track if asked.
    fn render_stems(&mut self, folder: &Path, options: RenderOptions) {
        self.services.finish_analysis();
        let version = Published::new(self.song_to_save(), Arc::clone(&self.services.samples));
        self.services.status =
            match crate::run::write_stems(&version, folder, self.services.rate, options) {
                Ok(count) => crate::run::stems_written(count, folder),
                Err(error) => format!("Couldn't write {}: {error}", folder.display()),
            };
    }

    pub(super) fn quit(&mut self, force: bool) {
        if self.services.dirty && !force {
            self.services.status = self.unsaved(":q! to quit anyway");
        } else {
            self.forget_autosave();
            self.drop_unkept_recordings();
            self.should_quit = true;
        }
    }

    /// Save, or save a copy to a new folder, which isn't read-only, so it can be carried on in. A
    /// read-only song is written over in its own folder only when forced, as Vim's `:w!`, and stays
    /// read-only.
    fn write(&mut self, path: Option<PathBuf>, force: bool) {
        let same = path
            .as_deref()
            .is_none_or(|p| Some(p) == self.services.folder.as_deref());
        if same && self.services.live.read_only && !force {
            self.services.status = READ_ONLY.to_owned();
            return;
        }
        let result = if same {
            match self.services.folder.clone() {
                Some(folder) => {
                    project::save_song(&folder, &self.song_to_save()).map(|()| (folder, None))
                }
                None => {
                    self.services.status = "This project has no folder yet: use :w path".to_owned();
                    return;
                }
            }
        } else {
            // A project saved to a new folder is named after it, which the line saving it starts
            // with, so the two only differ if another name is typed there.
            let target = path.expect("a different path is present");
            let mut song = Song::clone(&self.song_to_save());
            song.name = folder_name(&target);
            song.saved_read_only = false;
            project::save_as(self.services.folder.as_deref(), &target, &song)
                .map(|saved| (target, Some(saved)))
        };
        match result {
            Ok((folder, saved)) => {
                if let Some(saved) = saved {
                    self.replace_song(saved, Some(folder.clone()));
                }
                self.services.folder = Some(folder.clone());
                self.services.dirty = false;
                self.forget_autosave();
                self.services.status = if self.services.live.read_only {
                    format!("Saved {}, which stays read-only", folder.display())
                } else {
                    format!("Saved {}", folder.display())
                };
            }
            Err(error) => self.services.status = error.to_string(),
        }
    }

    /// `:set readonly` and `:set readwrite`. Like the scale, it isn't an edit to undo, but it's
    /// saved with the song, so switching it marks the song changed.
    fn set_read_only(&mut self, on: bool) {
        if self.services.live.read_only != on {
            self.services.live.read_only = on;
            self.services.dirty = true;
        }
        self.services.status = if on {
            "Read-only: :w path saves a copy, :w! writes over it".to_owned()
        } else {
            "Not read-only: :w saves over it again".to_owned()
        };
    }

    /// Open a project. Forced, unsaved changes are thrown away, autosave and all, as `:q!` does.
    fn open(&mut self, path: &Path, force: bool) {
        if self.services.dirty && !force {
            self.services.status = self.unsaved(":e! to discard them");
            return;
        }
        match project::load_song(path) {
            Ok(song) => {
                self.discard_changes();
                self.services.handle.send(Command::Return);
                self.services.give_back_on_opening();
                self.services.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.services.dirty = false;
                if self.services.status.is_empty() {
                    self.services.status = format!("Opened {}", path.display());
                }
                self.offer_restore();
            }
            Err(error) => self.services.status = error.to_string(),
        }
    }

    /// Start a project. Forced, unsaved changes are thrown away, autosave and all.
    fn new_project(&mut self, path: &Path, force: bool) {
        if self.services.dirty && !force {
            self.services.status = self.unsaved(":new! to discard them");
            return;
        }
        let song = Song {
            name: folder_name(path),
            ..crate::services::new_song(DEFAULT_TRACKS)
        };
        match project::create_project(path, &song) {
            Ok(()) => {
                self.discard_changes();
                self.services.handle.send(Command::Return);
                self.services.give_back_on_opening();
                self.services.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.services.dirty = false;
                self.services.status = format!("Created {}", path.display());
            }
            Err(error) => self.services.status = error.to_string(),
        }
    }

    /// Throw away unsaved changes before another project replaces them, so their autosave isn't
    /// offered back the next time this project opens.
    fn discard_changes(&mut self) {
        if self.services.dirty {
            self.forget_autosave();
        }
    }

    /// Replace the song and undo history, loading any samples it needs.
    pub(crate) fn replace_song(&mut self, mut song: Song, folder: Option<PathBuf>) {
        name_song(&mut song, folder.as_deref(), &mut self.services.names);
        let notes = project::find_sources(folder.as_deref(), &mut song);
        let mut pool = SamplePool::clone(&self.services.samples);
        let errors = project::load_samples(folder.as_deref(), &song, self.services.rate, &mut pool);
        self.services.samples = Arc::new(pool);
        self.services.live.levels = live_from(&song);
        self.services.live.scale = song.saved_scale;
        self.services.live.auto_jump = song.saved_auto_jump;
        self.services.live.read_only = song.saved_read_only;
        self.services.folds = Folds::from_view(&song.saved_view);
        self.send_live();
        self.services.history = History::new(Arc::new(song));
        // A song whose generators were saved in another scale than its own takes its own.
        self.services.follow_scale();
        // Another song's rows and generators aren't these.
        self.services.reported.heard_pitches.clear();
        self.services.reported.heard_passes.clear();
        self.services.reported.heard_rows.clear();
        self.services.folder = folder;
        self.services.cursor = Cursor {
            step: self.services.cursor.step,
            ..self.services.cursor
        };
        if self.services.phrase_id().is_none() {
            self.services.cursor.track = 0;
            self.services.cursor.slot = 0;
        }
        self.services.clamp_cursor(self.modes.view);
        if let Some(first) = notes.first() {
            self.services.status = first.clone();
        }
        if let Some(first) = errors.first() {
            self.services.status = first.to_string();
        }
        self.services.publish();
    }

    /// `:preset`, `:preset save` and `:preset load`, on what's under the cursor on the track
    /// page. It's `App`'s, as listing the presets opens the explorer, and a save keeps its folder
    /// among the explorer's recent places.
    fn preset_line(&mut self, line: PresetLine) {
        let view = self.modes.view;
        let saved = match line {
            PresetLine::Browse => {
                let kind = self.track_page.presets_to_browse(view, &mut self.services);
                if let Some(kind) = kind {
                    let purpose = Purpose::Preset(kind);
                    let roots = self.places(purpose);
                    self.open_explorer(purpose, roots, None);
                }
                None
            }
            PresetLine::Save { name, force } => {
                self.track_page
                    .save_preset(&name, force, view, &mut self.services)
            }
            PresetLine::Load(name) => {
                self.track_page
                    .load_preset_named(&name, view, &mut self.services);
                None
            }
        };
        if let Some(saved) = saved {
            saved.keep(
                &mut self.overlays.explorer.recent_places,
                &mut self.services,
            );
        }
    }

    /// `:patch`, `:patch save` and `:patch load`, and their `!`, on the cursor's track. It's
    /// `App`'s for the same reasons as `:preset`: the list of patches is the explorer, which
    /// keeps whether one chosen goes over the last mod slots and macros.
    fn patch_line(&mut self, line: PatchLine) {
        match line {
            PatchLine::Browse { over } => {
                if self.services.patch_track().is_none() {
                    return;
                }
                self.overlays.explorer.patch_over = over;
                let purpose = Purpose::Preset(PresetKind::Patch);
                let mut roots = self.places(purpose);
                roots.push(factory_root());
                self.open_explorer(purpose, roots, None);
            }
            PatchLine::Save { name, force } => {
                if let Some(saved) = self.services.save_patch(&name, force) {
                    saved.keep(
                        &mut self.overlays.explorer.recent_places,
                        &mut self.services,
                    );
                }
            }
            PatchLine::Load { name, over } => {
                self.services.add_patch_named(&name, over, self.modes.view);
            }
        }
    }

    /// Put a file in the bank and have the current track play it.
    pub(crate) fn sample(&mut self, source: &Path) {
        self.sample_named(source, None);
    }

    /// Put a file in the bank, have the current track play it, and name the track after it, if
    /// there's a name, as one undo step.
    pub(crate) fn sample_named(&mut self, source: &Path, name: Option<&str>) {
        let track = self.services.cursor.track;
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
