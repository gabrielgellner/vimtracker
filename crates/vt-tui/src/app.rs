//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::LoadReadout;
use crate::meter::Meter;
use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Transport;
use vt_engine::link::{Command, Handle};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::action::{Action, FieldValue, edit};
use vt_model::constants::{
    DEFAULT_NOTE_VELOCITY, DEFAULT_TRACKS, MAX_PHRASE_STEPS, TRANSPORT_GUARD_MS,
};
use vt_model::history::History;
use vt_model::project::{self, ProjectError};
use vt_model::sample::load_wav;
use vt_model::samples::SamplePool;
pub use vt_model::selection::Field;
use vt_model::selection::{Block, Extent, Scope, StepRange};
use vt_model::song::{ChainEntry, Song};
use vt_model::step::{FxCommand, FxSlot, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Mark, Note, Step};

use crate::grammar::Change;
use crate::guides::Guide;
use crate::input::bindings::{Cmd, Op, Table, note_key};
use crate::input::command::{self, Line};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::Picker;
use crate::shaping::ShapePanel;
use crate::track_page::Page;
use crate::tree::{ChainColumn, Place};

/// A run of steps and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub range: StepRange,
    pub scope: Scope,
}

/// How far `c` and `v` move the note velocity in Insert: its top digit.
const NOTE_VELOCITY_STEP: u8 = 0x10;

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
    /// The track page's row and setting.
    pub track_page: Page,
    pub track_row: usize,
    pub track_field: usize,
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
    pub octave: u8,
    /// The velocity new notes are written with, like the octave.
    pub note_velocity: u8,
    /// The field a replace started on, since typing an FX command moves on to its value.
    pub(crate) replacing: Option<Field>,
    /// The step last written in Insert, and where that left the cursor.
    pub(crate) last_entry: Option<(usize, usize)>,
    pub command_line: String,
    pub status: String,
    pub transport: Transport,
    pub playhead: usize,
    /// The main output's level.
    pub meter: Meter,
    /// The CPU figure on the status line.
    pub cpu: LoadReadout,
    last_tick: Instant,
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
    pub guide_width: usize,
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
    /// The open shape panel.
    pub shape: Option<ShapePanel>,
    /// Changes each time random values are wanted anew.
    pub(crate) shape_seed: u64,
    /// The selection a replace will fill, and the undo depth from before it started.
    pub(crate) replacing_selection: Option<(Selection, usize)>,
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
            track_page: Page::Sound,
            track_row: 0,
            track_field: 0,
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
            octave: 4,
            note_velocity: DEFAULT_NOTE_VELOCITY,
            replacing: None,
            last_entry: None,
            command_line: String::new(),
            status: String::new(),
            transport: Transport::Stopped,
            playhead: 0,
            meter: Meter::default(),
            cpu: LoadReadout::default(),
            last_tick: Instant::now(),
            should_quit: false,
            levels,
            entry: Entry::default(),
            top: 0,
            view_height: 16,
            help_scroll: 0,
            guide: None,
            guide_scroll: 0,
            guide_page: 16,
            guide_width: 76,
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
            shape: None,
            shape_seed: 0,
            replacing_selection: None,
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

    /// Where the cursor's phrase's loop sits.
    pub fn extent(&self) -> Extent {
        self.phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .map_or(Extent::whole(0), |phrase| phrase.extent())
    }

    /// The position in the section where the cursor's phrase starts playing.
    pub fn slot_start(&self) -> usize {
        let song = self.song();
        let Some(chain) = song
            .sections
            .first()
            .and_then(|s| s.chains.get(self.cursor.track).copied().flatten())
            .and_then(|id| song.chains.get(&id))
        else {
            return 0;
        };
        chain
            .entries
            .iter()
            .take(self.cursor.slot)
            .filter_map(|e| song.phrases.get(&e.phrase))
            .map(|p| p.len())
            .sum()
    }

    /// The cursor's position within the section. Only the loop has one, so a step outside it
    /// counts as the loop's first.
    pub fn section_step(&self) -> usize {
        let extent = self.extent();
        // The chain view has no step of its own, so it plays from the row's first.
        let row = if self.view != View::Chain && extent.plays(self.cursor.step) {
            self.cursor.step - extent.start
        } else {
            0
        };
        self.slot_start() + row
    }

    /// Put the cursor on the first step that plays, at the top of the screen.
    pub(crate) fn go_to_loop_start(&mut self) {
        let start = self.extent().start;
        self.cursor.step = start;
        self.top = start;
    }

    /// Send the current version to the audio thread.
    pub fn publish(&mut self) {
        let version = Published::new(Arc::clone(self.song()), Arc::clone(&self.samples));
        self.handle.publish(Arc::new(version));
    }

    /// Do the work due each frame: send waiting versions and read the callback's report.
    pub fn tick(&mut self) {
        self.handle.pump();
        let now = Instant::now();
        let seconds = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        let report = self.handle.latest();
        self.meter
            .update(report.map(|report| report.reading), seconds);
        if let Some(report) = report {
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
            | Mode::Track => {
                self.command_key_normal(key, now);
            }
            Mode::Insert => self.insert_key(key, now),
            Mode::Replace => self.replace_key(key),
            Mode::Command => self.command_key(key),
            Mode::Help => self.help_key(key),
            Mode::Guide => self.guide_key(key),
            Mode::Picker => self.picker_key(key),
            Mode::Shape => self.shape_key(key, now),
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
            (Mode::Normal, View::Track) => Mode::Track,
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

    /// Hear a row, unless playback is using the tracks.
    pub(crate) fn audition(&mut self, section_step: usize) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
        } else {
            self.handle.send(Command::Audition { section_step });
        }
    }

    /// Scroll an open guide, stopping when its last row reaches the bottom of the window.
    fn guide_key(&mut self, key: Key) {
        let Some(guide) = self.guide else {
            self.mode = Mode::Normal;
            return;
        };
        let last = guide
            .lines(self.guide_width)
            .len()
            .saturating_sub(self.guide_page);
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
            View::Track => self.run_track_page(cmd),
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
            Cmd::SlideLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.slide_loop(by * times as isize);
            }
            Cmd::RotateLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.rotate_loop(by * times as isize);
            }
            Cmd::StartLoopHere => self.start_loop_here(),
            Cmd::EndLoopHere => self.end_loop_here(),
            Cmd::ShowView(view) => self.jump_to_view(view),
            Cmd::Picker => self.open_view_picker(),
            Cmd::Help => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
            Cmd::Guides => self.open_guide_picker(),
            Cmd::PickValue => self.open_field_picker(),
            Cmd::Shape => self.open_shape(),
            Cmd::PickSample => self.open_default_sample_picker(),
            Cmd::Replace => self.begin_replace(),
            Cmd::Substitute => self.substitute(),
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.last_entry = None;
                self.mode = Mode::Insert;
            }
            Cmd::EnterCommand => {
                self.command_line.clear();
                self.mode = Mode::Command;
            }
            Cmd::PlayFromCursor => {
                let step = self.section_step();
                self.handle.send(Command::PlayFrom { step });
                self.guard = Some((key, now));
            }
            Cmd::PlayFromStart => {
                self.handle.send(Command::PlayFrom { step: 0 });
                self.guard = Some((key, now));
            }
            Cmd::Audition => {
                if self.extent().plays(self.cursor.step) {
                    let section_step = self.section_step();
                    self.audition(section_step);
                } else {
                    self.audition_outside_loop();
                }
            }
            Cmd::PauseOrResume => {
                self.handle.send(Command::PauseOrResume);
                self.guard = Some((key, now));
            }
            Cmd::Quit => self.quit(false),
            Cmd::NextTrack => self.move_track(1),
            Cmd::PreviousTrack => self.move_track(-1),
            Cmd::ToggleMute => self.toggle_mute(),
            Cmd::ToggleSolo => self.toggle_solo(),
            Cmd::ClearMutes => self.clear_live(),
            Cmd::Undo => {
                let before = self.loop_place();
                if self.history.undo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                } else {
                    self.status = "Nothing to undo".to_owned();
                }
            }
            Cmd::Redo => {
                let before = self.loop_place();
                if self.history.redo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                } else {
                    self.status = "Nothing to redo".to_owned();
                }
            }
            _ => {}
        }
    }

    /// Whether a phrase's length can change now. A new length moves where playback lands, so it
    /// has to wait until playback has stopped, unlike a slide.
    fn can_change_length(&mut self) -> bool {
        let allowed = self.transport != Transport::Playing;
        if !allowed {
            self.status = "Length changes wait until paused. Slides work now: ]l [l".to_owned();
        }
        allowed
    }

    fn double(&mut self) {
        if let Some(phrase) = self.phrase_id()
            && self.can_change_length()
        {
            self.apply(&Action::Double { phrase });
        }
    }

    /// The phrase under the cursor and where its loop starts.
    fn loop_place(&self) -> (Option<u32>, usize) {
        (self.phrase_id(), self.extent().start)
    }

    /// After the loop has moved, keep the cursor and the view on the same row numbers, so the
    /// notes scroll under them.
    fn keep_row(&mut self, before: (Option<u32>, usize)) {
        let (phrase, old) = before;
        if phrase != self.phrase_id() {
            return;
        }
        let by = self.extent().start as isize - old as isize;
        self.cursor.step = self.cursor.step.saturating_add_signed(by);
        self.top = self.top.saturating_add_signed(by);
    }

    fn slide_loop(&mut self, by: isize) {
        if let Some(phrase) = self.phrase_id() {
            let before = self.loop_place();
            self.apply(&Action::SlideLoop { phrase, by });
            self.keep_row(before);
            self.clamp_cursor();
        }
    }

    /// Rotate the loop's steps. The cursor stays on its row. Going all the way round changes
    /// nothing, so it doesn't leave an undo step.
    fn rotate_loop(&mut self, by: isize) {
        let length = self.extent().length as isize;
        if length > 0
            && by.rem_euclid(length) != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::RotateLoop { phrase, by });
        }
    }

    /// Start the loop on the cursor's step. The notes stay where they are and the numbers change.
    fn start_loop_here(&mut self) {
        let by = self.extent().row(self.cursor.step);
        if by != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::SlideLoop { phrase, by });
        }
    }

    /// End the loop on the cursor's step, so its row is the last that plays. Steps after it stay
    /// stored, and a dim step below the loop brings the steps before it in.
    fn end_loop_here(&mut self) {
        let extent = self.extent();
        let Ok(row) = usize::try_from(extent.row(self.cursor.step)) else {
            self.status = "The loop can't end above its start".to_owned();
            return;
        };
        let length = row + 1;
        if let Some(phrase) = self.phrase_id()
            && length != extent.length
            && self.can_change_length()
        {
            self.apply(&Action::SetLength { phrase, length });
        }
    }

    /// Hear the note on a row outside the loop. Other tracks have nothing to play with it, since
    /// only the loop has a place in the section.
    fn audition_outside_loop(&mut self) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
            return;
        }
        let Some(step) = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id))
            .and_then(|phrase| phrase.steps().get(self.cursor.step))
        else {
            return;
        };
        let Note::On(midi) = step.note else {
            return;
        };
        let velocity = step.velocity.unwrap_or(MAX_VELOCITY);
        let transpose = self
            .chain_entries()
            .get(self.cursor.slot)
            .map_or(0, |entry| entry.transpose);
        let note = (i16::from(midi) + i16::from(transpose))
            .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
        self.handle.send(Command::Preview {
            track: self.cursor.track,
            note,
            velocity,
        });
    }

    fn stored_steps(&self) -> usize {
        self.extent().stored
    }

    /// The furthest the view can scroll: to the end of what is stored, or with the loop's start at
    /// the top, so a slide can keep row `00` still on a screen with room to spare.
    fn last_top(&self) -> usize {
        let extent = self.extent();
        extent
            .stored
            .saturating_sub(self.view_height)
            .max(extent.start)
    }

    /// Keep the cursor on screen, moving the view rather than the cursor.
    pub fn follow_cursor(&mut self, height: usize) {
        self.view_height = height.max(1);
        let last_top = self.last_top();
        if self.cursor.step < self.top {
            self.top = self.cursor.step;
        } else if self.cursor.step >= self.top + self.view_height {
            self.top = self.cursor.step + 1 - self.view_height;
        }
        self.top = self.top.min(last_top);
    }

    fn view_cursor(&mut self, cmd: Cmd) {
        let height = self.view_height.min(self.stored_steps().max(1));
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
        self.top = self.top.saturating_add_signed(by).min(self.last_top());
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
    pub(crate) fn fit_levels(&mut self) {
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
        let extent = self.extent();
        // A row outside the loop has no place in the section, so it keeps its number instead.
        let outside = (!extent.plays(self.cursor.step)).then(|| extent.row(self.cursor.step));
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
                let row = outside.unwrap_or(position.step as isize);
                self.cursor.step = self.extent().step_of(row);
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

    pub(crate) fn move_step(&mut self, by: isize) {
        let Some(phrase) = self
            .phrase_id()
            .and_then(|id| self.song().phrases.get(&id).cloned())
        else {
            return;
        };
        let last = phrase.steps().len() as isize - 1;
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

    /// Typing here means what it does in Insert, for this one field, and `advance` ends it.
    fn replace_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => {
                self.cancel_entry();
                if let Some(field) = self.replacing.take() {
                    self.cursor.field = field;
                }
                if let Some((selection, _)) = self.replacing_selection.take() {
                    self.cursor.step = selection.range.first;
                }
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if self.entry.text.is_empty() => {}
            KeyCode::Char('c' | 'v') if self.cursor.field == Field::Note => {}
            _ => match self.cursor.field {
                Field::Note => self.insert_note(key),
                Field::Velocity | Field::FxValue(_) => self.insert_value(key),
                Field::FxCommand(slot) => self.insert_command(key, slot),
            },
        }
    }

    fn insert_key(&mut self, key: Key, now: Instant) {
        // You can start and pause while writing, as `Tab` and `Shift-Tab` are never typed.
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume, key, now),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart, key, now),
            _ => {}
        }
        let skip = self.song().skip as usize;
        let arrow = match (key.code, key.shift) {
            (KeyCode::Up, _) => Some(-1),
            (KeyCode::Down, _) => Some(1),
            (KeyCode::Left, true) => Some(snap_by(self.current_row(), skip, false)),
            (KeyCode::Right, true) => Some(snap_by(self.current_row(), skip, true)),
            (KeyCode::Left, false) => Some(self.skip_by(skip, false)),
            (KeyCode::Right, false) => Some(self.skip_by(skip, true)),
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
                    _ => self.insert_track_page(key.code),
                }
            }
            return;
        }
        match self.cursor.field {
            Field::Note => self.insert_note(key),
            Field::Velocity | Field::FxValue(_) => self.insert_value(key),
            Field::FxCommand(slot) => self.insert_command(key, slot),
        }
    }

    fn insert_note(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.clear_note();
                self.move_step(-1);
            }
            KeyCode::Char('z') => self.octave = self.octave.saturating_sub(1),
            KeyCode::Char('x') => self.octave = (self.octave + 1).min(9),
            KeyCode::Char('c') => {
                self.note_velocity = next_note_velocity(self.note_velocity, false)
            }
            KeyCode::Char('v') => self.note_velocity = next_note_velocity(self.note_velocity, true),
            KeyCode::Char('1') => {
                self.write_note(Note::Off);
                self.advance();
            }
            KeyCode::Char(c) if !key.ctrl => {
                if let Some(semitones) = note_key(c) {
                    let midi = (u16::from(self.octave) + 1) * 12 + u16::from(semitones);
                    let midi = midi.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8;
                    let on_step = self
                        .phrase_id()
                        .and_then(|id| self.song().phrases.get(&id).cloned())
                        .and_then(|phrase| phrase.steps().get(self.cursor.step).cloned());
                    let velocity = on_step
                        .as_ref()
                        .map_or(self.note_velocity, |step| self.played_with(step));
                    self.write_played_note(midi);
                    self.handle.send(Command::Preview {
                        track: self.cursor.track,
                        note: midi,
                        velocity,
                    });
                    self.advance();
                }
            }
            _ => {}
        }
    }

    fn current_row(&self) -> isize {
        self.extent().row(self.cursor.step)
    }

    /// How far a plain left or right arrow moves: one skip from the entry just written, so its
    /// offset within the beat is kept, or else from the cursor.
    fn skip_by(&self, skip: usize, forward: bool) -> isize {
        if skip == 0 {
            return 0;
        }
        let step = self.cursor.step as isize;
        let from = match self.last_entry {
            Some((written, landed)) if landed == self.cursor.step => written as isize,
            _ => step,
        };
        let skip = skip as isize;
        from + if forward { skip } else { -skip } - step
    }

    /// Move without writing, staying in Insert. Anything half typed is dropped.
    fn insert_move(&mut self, by: isize) {
        self.last_entry = None;
        if !self.entry.text.is_empty() || self.entry.command.is_some() {
            self.cancel_entry();
            self.mode = Mode::Insert;
        }
        self.move_step(by);
    }

    /// Write a played note with its velocity, as one change.
    fn write_played_note(&mut self, midi: u8) {
        let Some(id) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        let current = self.song().phrases.get(&id);
        let Some(step) = current.and_then(|p| p.steps().get(self.cursor.step).cloned()) else {
            return;
        };
        let velocity = self.played_with(&step);
        self.apply(&Action::SetStep {
            phrase: id,
            step: self.cursor.step,
            value: Step {
                note: Note::On(midi),
                velocity: Some(velocity),
                ..step
            },
        });
    }

    /// The velocity a played note takes. Replacing keeps what the step has.
    fn played_with(&self, step: &Step) -> u8 {
        if self.mode == Mode::Replace {
            step.velocity.unwrap_or(self.note_velocity)
        } else {
            self.note_velocity
        }
    }

    /// Clear a note along with the velocity it was written with.
    fn clear_note(&mut self) {
        let Some(id) = self.phrase_id() else {
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
                note: Note::Empty,
                velocity: None,
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

    pub(crate) fn set_field(&mut self, value: FieldValue) {
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
                        Field::Velocity => self.set_field(FieldValue::Velocity(None)),
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
            Field::Velocity => {
                self.set_field(FieldValue::Velocity(Some(value.min(MAX_VELOCITY))));
            }
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
                        mark: self.mark_for_typed_value(slot),
                    },
                ));
            }
            _ => {}
        }
        self.advance();
    }

    /// The mark a value typed here gets: the one it's typed over, or else the nearest value's
    /// above it in this column.
    pub(crate) fn mark_for_typed_value(&self, slot: usize) -> Mark {
        let song = self.song();
        let phrase = self.phrase_id().and_then(|id| song.phrases.get(&id));
        let Some(steps) = phrase.map(|phrase| phrase.steps()) else {
            return Mark::default();
        };
        let upward = steps[..=self.cursor.step.min(steps.len() - 1)].iter().rev();
        upward
            .filter_map(|step| step.fx.get(slot))
            .find(|fx| fx.is_value())
            .map_or(Mark::default(), |fx| fx.mark)
    }

    pub(crate) fn current_slot(&self, slot: usize) -> Option<FxSlot> {
        let id = self.phrase_id()?;
        let phrase = self.song().phrases.get(&id)?;
        phrase.steps().get(self.cursor.step)?.fx.get(slot).copied()
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
        if self.mode == Mode::Replace {
            self.finish_replace();
            return;
        }
        let written = self.cursor.step;
        let by = self.song().step as isize;
        self.move_step(by);
        self.last_entry = Some((written, self.cursor.step));
    }

    fn write_note(&mut self, note: Note) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        self.apply(&Action::SetNote {
            phrase,
            step: self.cursor.step,
            note,
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
            Line::Kit(Some(name)) => self.load_named_kit(&name),
            Line::Kit(None) => self.open_kit_picker(),
            Line::Name(name) => self.apply(&Action::SetTrackName {
                track: self.cursor.track,
                name: name.unwrap_or_default(),
            }),
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
            Line::Swing(swing) => self.apply(&Action::SetSwing(swing)),
            Line::TrackSwing(swing) => self.apply(&Action::SetTrackSwing {
                track: self.cursor.track,
                swing,
            }),
            Line::Reset(reset) => self.apply(&Action::SetReset(reset)),
            Line::TrackLevel(level) => self.apply(&Action::SetTrackLevel {
                track: self.cursor.track,
                level,
            }),
            Line::Mute => self.toggle_mute(),
            Line::Solo => self.toggle_solo(),
            Line::Len(length) => {
                if let Some(phrase) = self.phrase_id()
                    && self.can_change_length()
                {
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

    pub(crate) fn sample(&mut self, source: &Path) {
        if source.is_dir() {
            self.open_sample_picker(&[source.to_path_buf()]);
            return;
        }
        let stored = match self.store_sample(source) {
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
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample: Some(stored),
                });
                if self.status.is_empty() {
                    self.status = format!("Track {:02} sample loaded", self.cursor.track + 1);
                }
            }
            Err(error) => self.status = error.to_string(),
        }
    }
}

/// How far to move from a row to reach the next multiple of the skip, so a jump lands on the grid
/// wherever it starts.
fn snap_by(row: isize, skip: usize, forward: bool) -> isize {
    if skip == 0 {
        return 0;
    }
    let into = row.rem_euclid(skip as isize) as usize;
    match (forward, into) {
        (true, _) => (skip - into) as isize,
        (false, 0) => -(skip as isize),
        (false, _) => -(into as isize),
    }
}

/// The note velocity one press up or down from this one, changing only its top digit so that
/// pressing back always returns to the default. `7F` is the one value above that ladder, and going
/// down from it steps back on.
fn next_note_velocity(velocity: u8, up: bool) -> u8 {
    let step = NOTE_VELOCITY_STEP;
    let lowest = DEFAULT_NOTE_VELOCITY % step;
    let highest = MAX_VELOCITY - (MAX_VELOCITY - lowest) % step;
    if up {
        (velocity + step).min(MAX_VELOCITY)
    } else if velocity > highest {
        highest
    } else {
        velocity.saturating_sub(step).max(lowest)
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
