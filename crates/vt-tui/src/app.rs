//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::{Command, Handle};
use vt_engine::published::Published;
use vt_model::action::{Action, edit};
use vt_model::constants::{DEFAULT_TRACKS, TRANSPORT_GUARD_MS};
use vt_model::history::History;
use vt_model::project::{self, ProjectError};
use vt_model::sample::load_wav;
use vt_model::samples::SamplePool;
use vt_model::song::Song;
use vt_model::step::{MAX_NOTE, MIN_NOTE, Note};

use crate::input::bindings::{Cmd, Resolved, Table, note_key};
use crate::input::command::{self, Line};
use crate::input::keys::Key;
use crate::input::mode::Mode;

/// One editable part of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Note,
    Volume,
    Instrument,
    FxCommand(usize),
    FxValue(usize),
}

/// Where the cursor is: a step in a chain slot on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub track: usize,
    pub slot: usize,
    pub step: usize,
    pub field: Field,
}

/// A track's live controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackLive {
    pub mute: bool,
    pub solo: bool,
}

/// The whole program's state.
pub struct App {
    pub history: History,
    pub folder: Option<PathBuf>,
    pub dirty: bool,
    pub samples: Arc<SamplePool>,
    pub handle: Handle,
    pub rate: u32,
    pub mode: Mode,
    pub cursor: Cursor,
    pub instrument: u32,
    pub octave: u8,
    pub command_line: String,
    pub status: String,
    pub transport: Transport,
    pub playhead: usize,
    pub should_quit: bool,
    /// Live mute and solo per track, kept outside undo and written to the song on save.
    pub levels: Vec<TrackLive>,
    table: Table,
    pending: Vec<Key>,
    guard: Option<(Key, Instant)>,
}

impl App {
    /// Start with a song, its project folder if it has one, and a connection to the engine.
    pub fn new(
        song: Song,
        folder: Option<PathBuf>,
        samples: Arc<SamplePool>,
        handle: Handle,
        rate: u32,
    ) -> Self {
        let levels = live_from(&song);
        let mut app = Self {
            history: History::new(Arc::new(song)),
            folder,
            dirty: false,
            samples,
            handle,
            rate,
            mode: Mode::Normal,
            cursor: Cursor {
                track: 0,
                slot: 0,
                step: 0,
                field: Field::Note,
            },
            instrument: 0,
            octave: 4,
            command_line: String::new(),
            status: String::new(),
            transport: Transport::Stopped,
            playhead: 0,
            should_quit: false,
            levels,
            table: Table::default(),
            pending: Vec::new(),
            guard: None,
        };
        app.send_live();
        app
    }

    /// Tell the engine every track's live controls, after loading a song.
    fn send_live(&mut self) {
        for (track, live) in self.levels.clone().into_iter().enumerate() {
            self.handle.send(Command::SetMute {
                track,
                mute: live.mute,
            });
            self.handle.send(Command::SetSolo {
                track,
                solo: live.solo,
            });
        }
    }

    /// The song as saved: with live mute and solo written into it.
    fn song_to_save(&self) -> Arc<Song> {
        let mut song = Song::clone(self.song());
        for (track, live) in song.tracks.iter_mut().zip(&self.levels) {
            track.mute = live.mute;
            track.solo = live.solo;
        }
        Arc::new(song)
    }

    /// The current song.
    pub fn song(&self) -> &Arc<Song> {
        self.history.current()
    }

    /// The phrase under the cursor.
    pub fn phrase_id(&self) -> Option<u32> {
        let song = self.song();
        let chain = (*song.sections.first()?.chains.get(self.cursor.track)?)?;
        Some(
            song.chains
                .get(&chain)?
                .entries
                .get(self.cursor.slot)?
                .phrase,
        )
    }

    /// The keys typed so far in a sequence.
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    /// The cursor's position within the section, counting the phrases before its slot.
    pub fn section_step(&self) -> usize {
        let song = self.song();
        let Some(chain) = song
            .sections
            .first()
            .and_then(|s| s.chains.get(self.cursor.track).copied().flatten())
            .and_then(|id| song.chains.get(&id))
        else {
            return self.cursor.step;
        };
        let before: usize = chain
            .entries
            .iter()
            .take(self.cursor.slot)
            .filter_map(|e| song.phrases.get(&e.phrase))
            .map(|p| p.len())
            .sum();
        before + self.cursor.step
    }

    /// Send the current version to the audio thread.
    pub fn publish(&mut self) {
        let version = Published::new(Arc::clone(self.song()), Arc::clone(&self.samples));
        self.handle.publish(Arc::new(version));
    }

    /// Do the work due each frame: send waiting versions and read the callback's report.
    pub fn tick(&mut self) {
        self.handle.pump();
        if let Some(report) = self.handle.latest() {
            self.playhead = report.position;
            self.transport = report.transport;
        }
    }

    /// Handle one key.
    pub fn key(&mut self, key: Key, now: Instant) {
        if let Some((guarded, seen)) = self.guard {
            let window = Duration::from_millis(TRANSPORT_GUARD_MS);
            if guarded == key && now.saturating_duration_since(seen) < window {
                // A held key looks like repeated presses, so keep ignoring it while it repeats.
                self.guard = Some((key, now));
                return;
            }
            self.guard = None;
        }
        match self.mode {
            Mode::Normal => self.normal_key(key, now),
            Mode::Insert => self.insert_key(key),
            Mode::Command => self.command_key(key),
        }
    }

    fn normal_key(&mut self, key: Key, now: Instant) {
        if key.code == KeyCode::Esc {
            self.pending.clear();
            return;
        }
        self.pending.push(key);
        match self.table.resolve(Mode::Normal, &self.pending) {
            Resolved::Prefix => {}
            Resolved::Unbound => {
                if self.pending.len() > 1 {
                    let keys: Vec<String> = self.pending.iter().map(Key::to_string).collect();
                    self.status = format!("{} isn't bound", keys.join(" "));
                }
                self.pending.clear();
            }
            Resolved::Command(cmd) => {
                self.pending.clear();
                self.status.clear();
                self.run(cmd, key, now);
            }
        }
    }

    fn run(&mut self, cmd: Cmd, key: Key, now: Instant) {
        match cmd {
            Cmd::MoveLeft => self.move_field(false),
            Cmd::MoveRight => self.move_field(true),
            Cmd::MoveDown => self.move_step(1),
            Cmd::MoveUp => self.move_step(-1),
            Cmd::EnterInsert => {
                if self.cursor.field == Field::Note {
                    self.mode = Mode::Insert;
                } else {
                    self.status = "Insert only edits notes for now".to_owned();
                }
            }
            Cmd::EnterCommand => {
                self.command_line.clear();
                self.mode = Mode::Command;
            }
            Cmd::PlayOrStop => {
                let from = self.section_step();
                self.handle.send(Command::PlayOrStop { from });
                self.guard = Some((key, now));
            }
            Cmd::PauseOrResume => {
                self.handle.send(Command::PauseOrResume);
                self.guard = Some((key, now));
            }
            Cmd::ReturnToStart => {
                self.handle.send(Command::Return);
                self.guard = Some((key, now));
            }
            Cmd::Quit => self.quit(false),
            Cmd::NextTrack => self.move_track(1),
            Cmd::PreviousTrack => self.move_track(-1),
            Cmd::ToggleMute => self.toggle_mute(),
            Cmd::ToggleSolo => self.toggle_solo(),
            Cmd::ClearMutes => self.clear_live(),
            Cmd::Undo => {
                if self.history.undo().is_some() {
                    self.after_history();
                } else {
                    self.status = "Nothing to undo".to_owned();
                }
            }
            Cmd::Redo => {
                if self.history.redo().is_some() {
                    self.after_history();
                } else {
                    self.status = "Nothing to redo".to_owned();
                }
            }
        }
    }

    fn after_history(&mut self) {
        self.dirty = true;
        self.fit_levels();
        self.cursor.track = self.cursor.track.min(self.song().tracks.len() - 1);
        self.clamp_cursor();
        self.publish();
    }

    /// Keep one set of live controls per track as tracks come and go.
    fn fit_levels(&mut self) {
        let tracks = self.song().tracks.len();
        for removed in tracks..self.levels.len() {
            self.handle.send(Command::SetMute {
                track: removed,
                mute: false,
            });
            self.handle.send(Command::SetSolo {
                track: removed,
                solo: false,
            });
        }
        self.levels.resize(tracks, TrackLive::default());
    }

    /// Move to another track, staying at the same position in the section.
    fn move_track(&mut self, by: isize) {
        let last = self.song().tracks.len() as isize - 1;
        let track = (self.cursor.track as isize + by).clamp(0, last) as usize;
        if track == self.cursor.track {
            return;
        }
        let section_step = self.section_step();
        self.cursor.track = track;
        let song = Arc::clone(self.song());
        let position = song
            .sections
            .first()
            .and_then(|s| s.chains.get(track).copied().flatten())
            .and_then(|chain| song.chain_position(chain, section_step));
        match position {
            Some(position) => {
                self.cursor.slot = position.slot;
                self.cursor.step = position.step;
            }
            None => self.cursor.slot = 0,
        }
    }

    fn toggle_mute(&mut self) {
        let track = self.cursor.track;
        if let Some(live) = self.levels.get_mut(track) {
            live.mute = !live.mute;
            let mute = live.mute;
            self.handle.send(Command::SetMute { track, mute });
            self.dirty = true;
        }
    }

    fn toggle_solo(&mut self) {
        let track = self.cursor.track;
        if let Some(live) = self.levels.get_mut(track) {
            live.solo = !live.solo;
            let solo = live.solo;
            self.handle.send(Command::SetSolo { track, solo });
            self.dirty = true;
        }
    }

    fn clear_live(&mut self) {
        self.levels.fill(TrackLive::default());
        self.send_live();
        self.dirty = true;
    }

    fn slots(&self) -> usize {
        self.phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .and_then(|p| p.steps().get(self.cursor.step))
            .map_or(0, |step| step.fx.len())
    }

    fn move_field(&mut self, forward: bool) {
        let slots = self.slots();
        let order = |field: Field| match field {
            Field::Note => 0,
            Field::Volume => 1,
            Field::Instrument => 2,
            Field::FxCommand(slot) => 3 + slot * 2,
            Field::FxValue(slot) => 4 + slot * 2,
        };
        let last = 2 + slots * 2;
        let current = order(self.cursor.field);
        let next = if forward {
            (current + 1).min(last)
        } else {
            current.saturating_sub(1)
        };
        self.cursor.field = match next {
            0 => Field::Note,
            1 => Field::Volume,
            2 => Field::Instrument,
            n if (n - 3) % 2 == 0 => Field::FxCommand((n - 3) / 2),
            n => Field::FxValue((n - 4) / 2),
        };
    }

    fn move_step(&mut self, by: isize) {
        let Some(phrase) = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id).cloned())
        else {
            return;
        };
        let last = phrase.len() as isize - 1;
        self.cursor.step = (self.cursor.step as isize + by).clamp(0, last) as usize;
    }

    fn clamp_cursor(&mut self) {
        self.move_step(0);
    }

    fn insert_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.write_note(Note::Empty, None);
                self.move_step(-1);
            }
            KeyCode::Char('[') => self.octave = self.octave.saturating_sub(1),
            KeyCode::Char(']') => self.octave = (self.octave + 1).min(9),
            KeyCode::Char('1') => {
                self.write_note(Note::Off, None);
                self.advance();
            }
            KeyCode::Char(c) if !key.ctrl => {
                if let Some(semitones) = note_key(c) {
                    let midi = (u16::from(self.octave) + 1) * 12 + u16::from(semitones);
                    let midi = midi.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8;
                    self.write_note(Note::On(midi), Some(self.instrument));
                    self.handle.send(Command::Preview {
                        instrument: self.instrument,
                        note: midi,
                    });
                    self.advance();
                }
            }
            _ => {}
        }
    }

    fn advance(&mut self) {
        let by = self.song().step as isize;
        self.move_step(by);
    }

    fn write_note(&mut self, note: Note, instrument: Option<u32>) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        self.apply(&Action::SetNote {
            phrase,
            step: self.cursor.step,
            note,
            instrument,
        });
    }

    /// Run an action and make the result current.
    pub fn apply(&mut self, action: &Action) {
        match edit(self.song(), action) {
            Ok(next) => {
                self.history.commit(next);
                self.dirty = true;
                self.publish();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn command_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                self.mode = Mode::Normal;
                let text = std::mem::take(&mut self.command_line);
                match command::parse(&text) {
                    Ok(line) => self.line(line),
                    Err(error) => self.status = error.to_string(),
                }
            }
            KeyCode::Backspace => {
                if self.command_line.pop().is_none() {
                    self.mode = Mode::Normal;
                }
            }
            KeyCode::Char(c) if !key.ctrl => self.command_line.push(c),
            _ => {}
        }
    }

    /// Run a command line.
    pub fn line(&mut self, line: Line) {
        self.status.clear();
        match line {
            Line::Quit { force } => self.quit(force),
            Line::Write(path) => self.write(path),
            Line::Edit(path) => self.open(&path),
            Line::New(path) => self.new_project(&path),
            Line::Sample(path) => self.sample(&path),
            Line::Inst(id) => {
                self.instrument = id;
                self.status = format!("Instrument {id:02X}");
            }
            Line::Bpm(millibpm) => self.apply(&Action::SetMilliBpm(millibpm)),
            Line::TrackAdd(count) => {
                self.apply(&Action::AddTracks(count));
                self.fit_levels();
            }
            Line::TrackDel => {
                self.apply(&Action::RemoveLastTrack);
                self.fit_levels();
                let last = self.song().tracks.len() - 1;
                if self.cursor.track > last {
                    self.cursor.track = last;
                    self.cursor.slot = 0;
                    self.clamp_cursor();
                }
            }
            Line::Mute => self.toggle_mute(),
            Line::Solo => self.toggle_solo(),
            Line::Len(length) => {
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::SetLength { phrase, length });
                    self.clamp_cursor();
                }
            }
        }
    }

    fn quit(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = "Unsaved changes: use :w to save, or :q! to quit anyway".to_owned();
        } else {
            self.should_quit = true;
        }
    }

    fn write(&mut self, path: Option<PathBuf>) {
        let same = path
            .as_deref()
            .is_none_or(|p| Some(p) == self.folder.as_deref());
        let result = if same {
            match self.folder.clone() {
                Some(folder) => project::save_song(&folder, self.song()).map(|()| (folder, None)),
                None => {
                    self.status = "This project has no folder yet: use :w path".to_owned();
                    return;
                }
            }
        } else {
            let target = path.expect("a different path is present");
            project::save_as(self.folder.as_deref(), &target, &self.song_to_save())
                .map(|saved| (target, Some(saved)))
        };
        match result {
            Ok((folder, saved)) => {
                if let Some(saved) = saved {
                    self.replace_song(saved, Some(folder.clone()));
                }
                self.folder = Some(folder.clone());
                self.dirty = false;
                self.status = format!("Saved {}", folder.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn open(&mut self, path: &Path) {
        if self.dirty {
            self.status = "Unsaved changes: use :w to save first".to_owned();
            return;
        }
        match project::load_song(path) {
            Ok(song) => {
                self.handle.send(Command::Return);
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                if self.status.is_empty() {
                    self.status = format!("Opened {}", path.display());
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn new_project(&mut self, path: &Path) {
        if self.dirty {
            self.status = "Unsaved changes: use :w to save first".to_owned();
            return;
        }
        let song = Song::new(DEFAULT_TRACKS);
        match project::create_project(path, &song) {
            Ok(()) => {
                self.handle.send(Command::Return);
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                self.status = format!("Created {}", path.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Replace the song and undo history, loading any samples it needs.
    fn replace_song(&mut self, song: Song, folder: Option<PathBuf>) {
        let mut pool = SamplePool::clone(&self.samples);
        let errors = project::load_samples(folder.as_deref(), &song, self.rate, &mut pool);
        self.samples = Arc::new(pool);
        self.levels = live_from(&song);
        self.send_live();
        self.history = History::new(Arc::new(song));
        self.folder = folder;
        self.cursor = Cursor {
            step: self.cursor.step,
            ..self.cursor
        };
        if self.phrase_id().is_none() {
            self.cursor.track = 0;
            self.cursor.slot = 0;
        }
        self.clamp_cursor();
        if let Some(first) = errors.first() {
            self.status = first.to_string();
        }
        self.publish();
    }

    fn sample(&mut self, source: &Path) {
        let stored = match &self.folder {
            Some(folder) => project::import_sample(folder, source),
            None => source
                .canonicalize()
                .map(|p| p.display().to_string())
                .map_err(|error| ProjectError::Io {
                    path: source.display().to_string(),
                    source: error,
                }),
        };
        let stored = match stored {
            Ok(stored) => stored,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
        match load_wav(&resolved, self.rate) {
            Ok(data) => {
                let mut pool = SamplePool::clone(&self.samples);
                pool.insert(&stored, data);
                self.samples = Arc::new(pool);
                self.apply(&Action::SetSample {
                    instrument: self.instrument,
                    sample: stored,
                });
                if self.status.is_empty() {
                    self.status = format!("Instrument {:02X} loaded", self.instrument);
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }
}

fn live_from(song: &Song) -> Vec<TrackLive> {
    song.tracks
        .iter()
        .map(|t| TrackLive {
            mute: t.mute,
            solo: t.solo,
        })
        .collect()
}
