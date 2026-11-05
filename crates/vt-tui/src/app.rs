//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::{Command, Handle};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::action::{Action, FieldValue, edit};
use vt_model::constants::{DEFAULT_TRACKS, TRANSPORT_GUARD_MS};
use vt_model::history::History;
use vt_model::project::{self, ProjectError};
use vt_model::sample::load_wav;
use vt_model::samples::SamplePool;
pub use vt_model::selection::Field;
use vt_model::selection::{Block, Scope, StepRange};
use vt_model::song::{ChainEntry, Song};
use vt_model::step::{FxCommand, FxSlot, MAX_NOTE, MIN_NOTE, Note, Step};

use crate::grammar::Change;
use crate::guides::Guide;
use crate::input::bindings::{Cmd, Op, Table, note_key};
use crate::input::command::{self, Line};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::Picker;
use crate::instruments::Row;
use crate::tree::{ChainColumn, Place};

/// A run of steps and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub range: StepRange,
    pub scope: Scope,
}

/// How far `c` and `v` change the note volume in Insert, and the highest they set.
const NOTE_VOLUME_STEP: u8 = 0x20;
const NOTE_VOLUME_TOP: u8 = 0xE0;

/// Typed characters not yet written, in Insert mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub text: String,
    /// A command name waiting for its value, so the command doesn't take effect until then.
    pub command: Option<FxCommand>,
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
    pub view: View,
    pub instrument_row: Row,
    /// Most recently used first, starting with the view on screen.
    pub recent_views: Vec<View>,
    pub picker: Option<Picker>,
    pub chain_column: ChainColumn,
    pub(crate) yanked_row: Option<ChainEntry>,
    pub(crate) jumps: Vec<Place>,
    pub(crate) jump_index: usize,
    pub(crate) last_autosave: Option<Instant>,
    pub(crate) autosaved: Option<Arc<Song>>,
    pub cursor: Cursor,
    pub instrument: u32,
    pub octave: u8,
    /// The volume new notes are written with. `None` leaves the Volume column alone.
    pub note_volume: Option<u8>,
    pub command_line: String,
    pub status: String,
    pub transport: Transport,
    pub playhead: usize,
    pub should_quit: bool,
    /// Live mute and solo per track, kept outside undo and written to the song on save.
    pub levels: Vec<TrackLive>,
    pub entry: Entry,
    /// The first step on screen, and how many fit.
    pub top: usize,
    pub view_height: usize,
    pub help_scroll: usize,
    /// The open guide, how far it has scrolled, and how many of its rows fit.
    pub guide: Option<Guide>,
    pub guide_scroll: usize,
    pub guide_page: usize,
    pub(crate) table: Table,
    pub(crate) pending: Vec<Key>,
    pub(crate) pending_since: Option<Instant>,
    pub(crate) count: Option<usize>,
    pub(crate) operator: Option<Op>,
    pub(crate) operator_count: Option<usize>,
    pub(crate) register: Option<char>,
    pub(crate) awaiting_register: bool,
    pub(crate) registers: HashMap<char, Block>,
    pub(crate) anchor: Option<(usize, Scope)>,
    pub(crate) selection_scope: Option<Scope>,
    pub(crate) last_selection: Option<Selection>,
    pub(crate) last_change: Option<Change>,
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
            view: View::Phrase,
            instrument_row: Row::Sample,
            recent_views: View::ALL.to_vec(),
            picker: None,
            chain_column: ChainColumn::Phrase,
            yanked_row: None,
            jumps: Vec::new(),
            jump_index: 0,
            last_autosave: None,
            autosaved: None,
            cursor: Cursor {
                track: 0,
                slot: 0,
                step: 0,
                field: Field::Note,
            },
            instrument: 0,
            octave: 4,
            note_volume: None,
            command_line: String::new(),
            status: String::new(),
            transport: Transport::Stopped,
            playhead: 0,
            should_quit: false,
            levels,
            entry: Entry::default(),
            top: 0,
            view_height: 16,
            help_scroll: 0,
            guide: None,
            guide_scroll: 0,
            guide_page: 16,
            table: Table::default(),
            pending: Vec::new(),
            pending_since: None,
            count: None,
            operator: None,
            operator_count: None,
            register: None,
            awaiting_register: false,
            registers: HashMap::new(),
            anchor: None,
            selection_scope: None,
            last_selection: None,
            last_change: None,
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
    pub(crate) fn song_to_save(&self) -> Arc<Song> {
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
        // The chain view has no step of its own, so it plays from the row's first.
        before
            + if self.view == View::Chain {
                0
            } else {
                self.cursor.step
            }
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
            Mode::Normal
            | Mode::OperatorPending
            | Mode::VisualField
            | Mode::VisualStep
            | Mode::Chain
            | Mode::Instrument => {
                self.command_key_normal(key, now);
            }
            Mode::Insert => self.insert_key(key),
            Mode::Command => self.command_key(key),
            Mode::Help => self.help_key(key),
            Mode::Guide => self.guide_key(key),
            Mode::Picker => self.picker_key(key),
            Mode::Restore => self.restore_key(key),
        }
    }

    /// What the popup should list, once the delay has passed.
    pub fn popup_rows(&self, now: Instant, delay: Duration) -> Vec<(String, &'static str)> {
        if self.pending.is_empty() || !which_key_due(self.pending_since, now, delay) {
            return Vec::new();
        }
        self.table.continuations(self.key_scope(), &self.pending)
    }

    /// Which rows of the binding table apply now. Normal mode's keys depend on the view.
    pub fn key_scope(&self) -> Mode {
        match (self.mode, self.view) {
            (Mode::Normal, View::Chain) => Mode::Chain,
            (Mode::Normal, View::Instrument) => Mode::Instrument,
            (mode, _) => mode,
        }
    }

    pub(crate) fn show_view(&mut self, view: View) {
        self.count = None;
        self.view = view;
        self.note_view(view);
        self.clamp_cursor();
    }

    /// Show a view, as a jump that `Ctrl-O` can undo.
    pub(crate) fn jump_to_view(&mut self, view: View) {
        if view != self.view {
            self.record_jump();
        }
        self.show_view(view);
    }

    /// Every key, for `Space k`.
    pub fn help_rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        let mut last = None;
        for binding in self.table.rows() {
            if last != Some(binding.mode) {
                rows.push(String::new());
                rows.push(format!("{:?}", binding.mode));
                last = Some(binding.mode);
            }
            rows.push(format!("  {:<12} {}", binding.keys, binding.description));
        }
        rows
    }

    fn help_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            _ => {
                self.help_scroll = 0;
                self.mode = Mode::Normal;
            }
        }
    }

    /// Scroll an open guide, stopping when its last row reaches the bottom of the window.
    fn guide_key(&mut self, key: Key) {
        let Some(guide) = self.guide else {
            self.mode = Mode::Normal;
            return;
        };
        let last = guide.lines().len().saturating_sub(self.guide_page);
        let half = (self.guide_page / 2).max(1);
        self.guide_scroll = match key.code {
            KeyCode::Char('d') if key.ctrl => self.guide_scroll + half,
            KeyCode::Char('u') if key.ctrl => self.guide_scroll.saturating_sub(half),
            KeyCode::Char('j') | KeyCode::Down => self.guide_scroll + 1,
            KeyCode::Char('k') | KeyCode::Up => self.guide_scroll.saturating_sub(1),
            KeyCode::Char('g') => 0,
            KeyCode::Char('G') => last,
            KeyCode::Char('?') => {
                self.guide = None;
                self.open_guide_picker();
                return;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.guide = None;
                self.mode = Mode::Normal;
                return;
            }
            _ => self.guide_scroll,
        }
        .min(last);
    }

    pub(crate) fn run(&mut self, cmd: Cmd, key: Key, now: Instant) {
        let handled = match self.view {
            View::Chain => self.run_chain(cmd),
            View::Instrument => self.run_instrument(cmd),
            View::Phrase => false,
        };
        if handled || self.run_tree(cmd) || self.run_grammar(cmd) {
            return;
        }
        match cmd {
            Cmd::ViewTop | Cmd::ViewMiddle | Cmd::ViewBottom => self.view_cursor(cmd),
            Cmd::HalfPageDown => self.half_page(1),
            Cmd::HalfPageUp => self.half_page(-1),
            Cmd::ScrollDown => self.scroll(3),
            Cmd::ScrollUp => self.scroll(-3),
            Cmd::Double => self.double(),
            Cmd::ShowView(view) => self.jump_to_view(view),
            Cmd::Picker => self.open_view_picker(),
            Cmd::Help => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
            Cmd::Guides => self.open_guide_picker(),
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
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
            _ => {}
        }
    }

    fn double(&mut self) {
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::Double { phrase });
        }
    }

    fn phrase_len(&self) -> usize {
        self.phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .map_or(0, |p| p.len())
    }

    /// Keep the cursor on screen, moving the view rather than the cursor.
    pub fn follow_cursor(&mut self, height: usize) {
        self.view_height = height.max(1);
        let last_top = self.phrase_len().saturating_sub(self.view_height);
        if self.cursor.step < self.top {
            self.top = self.cursor.step;
        } else if self.cursor.step >= self.top + self.view_height {
            self.top = self.cursor.step + 1 - self.view_height;
        }
        self.top = self.top.min(last_top);
    }

    fn view_cursor(&mut self, cmd: Cmd) {
        let height = self.view_height.min(self.phrase_len().max(1));
        self.cursor.step = self.top
            + match cmd {
                Cmd::ViewTop => 0,
                Cmd::ViewMiddle => height / 2,
                _ => height - 1,
            };
        self.clamp_cursor();
    }

    fn half_page(&mut self, sign: isize) {
        let half = (self.view_height / 2).max(1) as isize;
        self.move_step(sign * half);
        self.top = self.cursor.step.saturating_sub(self.view_height / 2);
    }

    /// Scroll the view. The cursor only moves if it would leave the screen.
    fn scroll(&mut self, by: isize) {
        let last_top = self.phrase_len().saturating_sub(self.view_height);
        self.top = self.top.saturating_add_signed(by).min(last_top);
        let bottom = self.top + self.view_height.saturating_sub(1);
        self.cursor.step = self.cursor.step.clamp(self.top, bottom);
        self.clamp_cursor();
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

    pub(crate) fn slots(&self) -> usize {
        self.phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .and_then(|p| p.steps().get(self.cursor.step))
            .map_or(0, |step| step.fx.len())
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

    /// Keep the cursor on something that exists. Only the chain view has a row past the end.
    pub(crate) fn clamp_cursor(&mut self) {
        let rows = self.chain_entries().len();
        let last = if self.view == View::Chain {
            rows
        } else {
            rows.saturating_sub(1)
        };
        self.cursor.slot = self.cursor.slot.min(last);
        self.move_step(0);
    }

    fn insert_key(&mut self, key: Key) {
        let skip = self.song().skip as isize;
        let arrow = match key.code {
            KeyCode::Up => Some(-1),
            KeyCode::Down => Some(1),
            KeyCode::Left => Some(-skip),
            KeyCode::Right => Some(skip),
            _ => None,
        };
        if let Some(by) = arrow {
            if self.view == View::Phrase {
                self.insert_move(by);
            }
            return;
        }
        if self.view != View::Phrase {
            if !key.ctrl {
                match self.view {
                    View::Chain => self.insert_chain_row(key.code),
                    _ => self.insert_instrument_row(key.code),
                }
            }
            return;
        }
        match self.cursor.field {
            Field::Note => self.insert_note(key),
            Field::Volume | Field::FxValue(_) => self.insert_value(key),
            Field::Instrument => self.insert_instrument(key),
            Field::FxCommand(slot) => self.insert_command(key, slot),
        }
    }

    fn insert_note(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.write_note(Note::Empty, None);
                self.move_step(-1);
            }
            KeyCode::Char('z') => self.octave = self.octave.saturating_sub(1),
            KeyCode::Char('x') => self.octave = (self.octave + 1).min(9),
            KeyCode::Char('c') => self.note_volume = next_note_volume(self.note_volume, false),
            KeyCode::Char('v') => self.note_volume = next_note_volume(self.note_volume, true),
            KeyCode::Char('1') => {
                self.write_note(Note::Off, None);
                self.advance();
            }
            KeyCode::Char(c) if !key.ctrl => {
                if let Some(semitones) = note_key(c) {
                    let midi = (u16::from(self.octave) + 1) * 12 + u16::from(semitones);
                    let midi = midi.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8;
                    self.write_played_note(midi);
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

    /// Move without writing, staying in Insert. Anything half typed is dropped.
    fn insert_move(&mut self, by: isize) {
        if !self.entry.text.is_empty() || self.entry.command.is_some() {
            self.cancel_entry();
            self.mode = Mode::Insert;
        }
        self.move_step(by);
    }

    /// Write a played note, with the note volume if one is set, as one change.
    fn write_played_note(&mut self, midi: u8) {
        let note = Note::On(midi);
        let Some(volume) = self.note_volume else {
            self.write_note(note, Some(self.instrument));
            return;
        };
        let Some(id) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        let current = self.song().phrases.get(&id);
        let Some(step) = current.and_then(|p| p.steps().get(self.cursor.step).cloned()) else {
            return;
        };
        self.apply(&Action::SetStep {
            phrase: id,
            step: self.cursor.step,
            value: Step {
                note,
                instrument: Some(self.instrument),
                volume: Some(volume),
                ..step
            },
        });
    }

    /// Leave Insert, dropping anything typed but not written.
    fn cancel_entry(&mut self) {
        if let Some(command) = self.entry.command
            && let Field::FxValue(slot) = self.cursor.field
        {
            self.status = format!("{} needs a value, so nothing was written", command.name());
            self.cursor.field = Field::FxCommand(slot);
        }
        self.entry = Entry::default();
        self.mode = Mode::Normal;
    }

    fn set_field(&mut self, value: FieldValue) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        self.apply(&Action::SetField {
            phrase,
            step: self.cursor.step,
            value,
        });
    }

    /// Two hex digits commit and move down by the step setting. A command typed just before
    /// takes effect here, so typing `CUT` during playback doesn't close the filter.
    fn insert_value(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    match self.cursor.field {
                        Field::Volume => self.set_field(FieldValue::Volume(None)),
                        Field::FxValue(slot) if self.entry.command.is_none() => {
                            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                        }
                        _ => {}
                    }
                    self.move_step(-1);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    self.commit_value();
                }
            }
            _ => {}
        }
    }

    fn commit_value(&mut self) {
        let text = std::mem::take(&mut self.entry.text);
        let Ok(value) = u8::from_str_radix(&text, 16) else {
            return;
        };
        match self.cursor.field {
            Field::Volume => self.set_field(FieldValue::Volume(Some(value))),
            Field::FxValue(slot) => {
                let existing = self.current_slot(slot).and_then(|s| s.command);
                let Some(command) = self.entry.command.take().or(existing) else {
                    self.status = "Type an FX command first".to_owned();
                    return;
                };
                self.set_field(FieldValue::Slot(
                    slot,
                    FxSlot {
                        command: Some(command),
                        value,
                    },
                ));
            }
            _ => {}
        }
        self.advance();
    }

    fn current_slot(&self, slot: usize) -> Option<FxSlot> {
        let id = self.phrase_id()?;
        let phrase = self.song().phrases.get(&id)?;
        phrase.steps().get(self.cursor.step)?.fx.get(slot).copied()
    }

    /// Instrument IDs can have more digits, so `Enter` commits them.
    fn insert_instrument(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.set_field(FieldValue::Instrument(None));
                    self.move_step(-1);
                }
            }
            KeyCode::Enter => {
                let text = std::mem::take(&mut self.entry.text);
                if let Ok(id) = u32::from_str_radix(&text, 16) {
                    self.instrument = id;
                    self.set_field(FieldValue::Instrument(Some(id)));
                    self.advance();
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() && self.entry.text.len() < 8 => {
                self.entry.text.push(c.to_ascii_uppercase());
            }
            _ => {}
        }
    }

    /// The third letter of a valid name moves to the value. An invalid name is refused, and the
    /// field is unchanged.
    fn insert_command(&mut self, key: Key, slot: usize) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                    self.move_step(-1);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_alphabetic() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 3 {
                    let name = std::mem::take(&mut self.entry.text);
                    match FxCommand::from_name(&name) {
                        Some(command) => {
                            self.entry.command = Some(command);
                            self.cursor.field = Field::FxValue(slot);
                        }
                        None => self.status = format!("{name} isn't an FX command"),
                    }
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
            Line::Chain(chain) => self.set_track_chain(chain),
            Line::Render { path, rows, tail } => self.render(&path, rows, tail),
            Line::Double => self.double(),
            Line::SetStep(size) => self.apply(&Action::SetStepSize(size)),
            Line::SetSkip(size) => self.apply(&Action::SetSkipSize(size)),
            Line::Seed(seed) => self.apply(&Action::SetSeed(seed)),
            Line::Swing(swing) => self.apply(&Action::SetSwing(swing)),
            Line::TrackSwing(swing) => self.apply(&Action::SetTrackSwing {
                track: self.cursor.track,
                swing,
            }),
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

    /// Render what is in memory, with the live mutes and at the device's rate. The screen waits,
    /// and audio keeps playing on its own thread.
    fn render(&mut self, path: &Path, rows: Option<usize>, tail: bool) {
        let version = Published::new(self.song_to_save(), Arc::clone(&self.samples));
        let samples = render_loop(Arc::new(version), self.rate, RenderOptions { rows, tail });
        self.status = match write_wav(path, self.rate, &samples) {
            Ok(()) => format!(
                "Rendered {} ({:.2} s)",
                path.display(),
                samples.len() as f64 / 2.0 / f64::from(self.rate)
            ),
            Err(error) => format!("Couldn't write {}: {error}", path.display()),
        };
    }

    fn quit(&mut self, force: bool) {
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
                self.forget_autosave();
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
                self.offer_restore();
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
    pub(crate) fn replace_song(&mut self, song: Song, folder: Option<PathBuf>) {
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

    pub(crate) fn sample(&mut self, source: &Path) {
        if source.is_dir() {
            self.open_sample_picker(&[source.to_path_buf()]);
            return;
        }
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

/// The note volume one press up or down from this one. Above the top it stops being set, and it
/// stops at the lowest step rather than writing silent notes.
fn next_note_volume(volume: Option<u8>, up: bool) -> Option<u8> {
    match (volume, up) {
        (None, true) => None,
        (None, false) => Some(NOTE_VOLUME_TOP),
        (Some(value), true) if value >= NOTE_VOLUME_TOP => None,
        (Some(value), true) => Some(value + NOTE_VOLUME_STEP),
        (Some(value), false) => Some(value.saturating_sub(NOTE_VOLUME_STEP).max(NOTE_VOLUME_STEP)),
    }
}

/// Whether a delay since a time has passed. Copied from gmuse's `src/ui/mod.rs`.
fn which_key_due(since: Option<Instant>, now: Instant, delay: Duration) -> bool {
    match since {
        Some(started) => now.saturating_duration_since(started) >= delay,
        None => false,
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
