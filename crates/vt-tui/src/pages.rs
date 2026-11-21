//! What a page is given to run its keys and draw with: the program's shared services, borrowed. A
//! page, such as the track page or the mixer, keeps its own state, its cursor, its images and where
//! it last drew them, and reaches everything else through what it's given.
//!
//! - **[`CtxMut`]**, to run a key: the [`Services`], the song, the cursor and the rest, borrowed
//!   mutably apart from the page itself, so a key can change the page and apply an action at once,
//!   and the [`Modes`] that span the pages, the view and the mode among them, to read.
//! - **[`Ctx`]**, to be drawn: everything a page reads, the song, the cursor's place in it, the
//!   view and the modes that span pages, which groups are folded, and what the engine last
//!   reported, the tracks' meters among it. It only reads: the [`Services`] and the [`Modes`],
//!   and the little of `App`'s own a page shows, a [`PageState`] `App` fills as it's drawn.
//!
//! - **[`Page`]**, what a page is as a component: its keys in the binding table, how it runs one,
//!   and its drawing and pictures. `App` holds one of each page and dispatches by `View` with a
//!   plain `match`, in [`App::run_page`], so there's no `dyn` and no downcast.
//!
//! They're the seam between a page and `App`: what a page may reach of the program is what's
//! here, and nothing else of `App`'s.

pub mod audio;
pub mod chain;
pub mod mixer;
pub mod phrase;
pub mod project;
pub mod sample;
pub mod song;
pub mod track;

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_engine::engine::{PLAYHEADS, Playhead, SectionLoop, Transport};
use vt_engine::link::JamState;
use vt_engine::track_state::{Drawn, Faders, LiveValues};
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::order::{Dealer, WalkMove};
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
use vt_model::selection::{Extent, Field, StepRange};
use vt_model::shape::ShapeColumn;
use vt_model::song::{ChainEntry, Phrase, Section, Song, Track};
use vt_model::step::{FxSlot, Gate, Step};

use crate::app::{App, Entry, Modes, Selection, which_key_due};
use crate::controls::Control;
use crate::controls::param::ParamView;
use crate::graphics::{PanelIds, PanelWant};
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::input::wheel::{Wheel, WheelSetting};
use crate::overlays::command::parse::Line;
use crate::overlays::field_jump::FieldJump;
use crate::overlays::help::{Help, POPUP_DELAY};
use crate::overlays::mapping::MappingView;
use crate::overlays::picker::exploring::Naming;
use crate::overlays::picker::{Pick, Picker};
use crate::overlays::shape::ShapePanel;
use crate::pages::audio::AudioState;
use crate::pages::sample::{SamplePage, Shown, is_sample_motion};
use crate::pages::track::TrackPage;
use crate::roto::Roto;
use crate::services::Services;
use crate::services::audio::SlowdownReadout;
use crate::services::folding::Folds;
use crate::services::generators::LiveGen;
use crate::services::meter::Meter;
use crate::services::{Cursor, Momentary, TrackLive};
use crate::widgets::setting::{CellLook, Focus};

/// What a key did, as far as the page or the overlay it went to alone can tell, and what it asks
/// for that only `App` can do: open the picker, show another view or run a `:` line, and start Insert or another mode,
/// open the explorer to name a file, hold what lasts while the key is down, or start the phrase
/// view afresh. [`App::settled`] does what it asks once the page has given its borrows back, so a
/// page needn't pass a key back to reach them.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The page did it.
    Done,
    /// It can't be done here, and the status line says why.
    Refused(&'static str),
    /// It isn't one of the page's own keys: the next in the order [`App::run`] gives keys in gets
    /// it.
    Passed,
    /// Done, and this picker opens over the page, with its preview.
    Open(Box<Picker>),
    /// Done, and this view shows, as [`App::show_view`] shows one.
    Show(View),
    /// Done, and this view shows as a jump, which `Ctrl-O` comes back from.
    Jump(View),
    /// Done, and this command line runs, as if it had been typed.
    Run(Line),
    /// Done, and Insert starts, with this typed already.
    Insert(Entry),
    /// Done, and the keys go into this mode of the page's, such as the sample page's pads.
    Enter(Mode),
    /// Done, and the file explorer opens to name a file, for the command it then runs, as `:w`
    /// opens it for a song not yet saved.
    Explore(Naming),
    /// Done, and what it started lasts while the key is down, to be ended or undone when it's
    /// let go.
    Hold(Momentary),
    /// Done, and the cursor moved onto another phrase, so the phrase view starts it afresh, with
    /// the cursor's step at its top. Only a move that means to start a phrase over asks for it,
    /// such as going to another section or chain row: a jump back, or going to another track,
    /// leaves the phrase view's top where it was.
    Moved,
    /// Done, and what's open over the page closes, with nothing chosen.
    Close,
    /// Done, and the picker closes with this chosen, which `App` then does.
    Choose(Pick),
    /// Done, and an item in the explorer is given these tags, as typed after `T`. The explorer
    /// stays open.
    Tag { item: usize, tags: Vec<String> },
    /// Done, and `Space k`'s list of keys opens at this mode's, over what's open, which it goes
    /// back to when it closes, as `?` in a picker opens it at the picker's.
    Keys(Mode),
    /// Done, and what's open closes and the cursor lands on this setting of the track page or
    /// the mixer, by row and field, as a jump `Ctrl-O` comes back from, as `/` lands.
    Land { row: usize, field: usize },
    /// Done, and this command of the table's runs as `App::run` runs it, the mode staying as it
    /// is, as `Tab` plays from a field jump.
    Do(Cmd),
    /// Done, and the shape panel opens with this, heard as it opens, as `gl` opens it again on a
    /// live generator.
    Shape(Box<ShapePanel>),
    /// Done, and the phrase view shows with the cursor on this field and step, and the shape
    /// panel opens again on the live generator there, as `Enter` on the Gen page opens it.
    Reopen { field: Field, step: usize },
    /// Done, and this macro's mapping view opens, as `gm` opens it.
    Mapping(MappingView),
    /// Done, and the search for a setting by its name starts, as `/` on the track page starts
    /// it.
    FieldJump,
}

/// One of the program's pages as a component: its own state, its keys, its drawing and its
/// pictures. Every page is one: the song view, the chain view, the phrase view, the track page,
/// the mixer, the project page's two sub-pages and the sample page.
pub(crate) trait Page {
    /// Its rows of the binding table, as `(keys, Cmd, description)`, in the order which-key and
    /// `Space k` list them. The table is assembled in one place, `input::bindings`, under the
    /// page's mode.
    const KEYS: &'static [(&'static str, Cmd, &'static str)];

    /// What its drawing notes for after the frame, such as where it drew what the terminal places
    /// a dot on, or nothing.
    type Drawn;

    /// Run one of its keys `count` times, at least 1, changing its own state directly and the
    /// song through the services' actions, or pass it back.
    fn key(&mut self, cmd: Cmd, count: usize, ctx: &mut CtxMut) -> Outcome;

    /// Draw it.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Self::Drawn;

    /// The name of what the cursor is on, which the status line shows.
    fn column(&self, ctx: &Ctx) -> String;

    /// What the status line says under it while nothing else is said there, such as what the keys
    /// do in a mode of the page's own and how to leave it, or nothing.
    fn hint(&self, _ctx: &Ctx) -> Option<String> {
        None
    }

    /// What pictures it shows this frame, as data, for `Graphics` to send before ratatui draws.
    /// Most pages have none.
    fn panels(&self, _ctx: &Ctx) -> Vec<PanelWant> {
        Vec::new()
    }

    /// Keep the ids `Graphics` gave the pictures it wanted, for `draw` to put placeholders for.
    fn take_panels(&mut self, _ids: PanelIds, _wants: &[PanelWant], _rate: u32) {}
}

/// What a page reads of the program, borrowed from it while the page is drawn: the shared
/// services, the modes that span the pages, and what of `App`'s own the pages show.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    services: &'a Services,
    modes: &'a Modes,
    page: PageState<'a>,
}

/// What of `App`'s own state a page shows, which isn't the services': what's drawn over the
/// pages, the selection a replace or the shape panel holds, the recording, and the audio's
/// readouts. `App` fills it each time a page is given a [`Ctx`].
#[derive(Clone, Copy)]
pub(crate) struct PageState<'a> {
    /// Whether a picker, a guide, the shape panel, help or the command line's list is drawn over
    /// the page.
    covered: bool,
    /// When the keys typed so far started, while more keys can follow them, for which-key's list
    /// that opens over the page a moment after.
    keys_since: Option<Instant>,
    /// The selection a replace across it or the shape panel is filling.
    held_selection: Option<Selection>,
    /// The entry being recorded into, and its seconds so far once it has started.
    recording: Option<(u8, Option<f64>)>,
    /// The audio output as it plays, and what the audio page chooses from.
    audio: &'a AudioState,
    wheel: WheelSetting,
    cpu_percent: u64,
    slowdown: &'a SlowdownReadout,
    /// The search for a setting under way, whose labels go over the settings it finds.
    field_jump: Option<&'a FieldJump>,
    /// The shape panel, while it's open, whose token under the cursor lights the rows it writes.
    shape: Option<&'a ShapePanel>,
    /// Beside the grid with the keys in Gen's panel, the rows its device lights in the grid, and
    /// their column (decision 468).
    gen_lit: Option<(ShapeColumn, StepRange)>,
    /// The buffer open on a setting of Gen's panel beside the grid, whose token under the cursor
    /// lights the rows it writes there too (decision 470).
    gen_tokens: Option<&'a crate::pages::track::GenTokens>,
}

/// What a page is given to run a key: the shared services, to read and change, the modes that
/// span the pages, to read, and the mouse wheel, borrowed apart from the page.
pub(crate) struct CtxMut<'a> {
    pub services: &'a mut Services,
    pub modes: &'a Modes,
    /// The mouse wheel, whose setting the audio page's rows turn on and off. Its ticks are
    /// `App`'s, as they press keys.
    pub wheel: &'a mut Wheel,
}

impl App {
    /// What a page reads, to be drawn with.
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx {
            services: &self.services,
            modes: &self.modes,
            page: self.page_state(),
        }
    }

    /// What of its own state the pages show, as it is now.
    fn page_state(&self) -> PageState<'_> {
        let covered = self.overlays.picker.is_some()
            || self.overlays.guide.open.is_some()
            || self.overlays.shape.is_some()
            || matches!(self.modes.mode, Mode::Help | Mode::Picker | Mode::Guide)
            || (self.modes.mode == Mode::Command && self.overlays.command.menu.is_some());
        // Only worked out while keys are typed, as the list of what follows them is.
        let continued = |_: &Instant| {
            !self.grammar.pending.is_empty()
                && !self
                    .table
                    .continuations_in(&self.key_scopes(), &self.grammar.pending)
                    .is_empty()
        };
        PageState {
            covered,
            keys_since: self.grammar.pending_since.filter(continued),
            held_selection: self
                .grammar
                .replacing_selection
                .map(|(selection, _)| selection)
                .or(self
                    .overlays
                    .shape
                    .as_ref()
                    .filter(|panel| panel.pattern.as_ref().is_none_or(|p| p.selected))
                    .map(|panel| panel.selection)),
            recording: self.sampling_shown(),
            audio: &self.audio_page.audio,
            wheel: self.wheel.setting,
            cpu_percent: self.readouts.cpu.percent(),
            slowdown: &self.readouts.slowdown,
            field_jump: self.overlays.field_jump.as_ref(),
            shape: self.overlays.shape.as_ref(),
            gen_lit: self.gen_lit(),
            gen_tokens: self
                .gen_page
                .tokens
                .as_ref()
                .filter(|_| self.gen_beside_shown()),
        }
    }

    /// Run a command on the page on screen, the second place a key goes in [`App::run`]'s order:
    /// through its [`Page::key`], and then what it passes back through `App`'s own for the page.
    /// Returns false if it's none of theirs.
    pub(crate) fn run_page(&mut self, cmd: Cmd) -> bool {
        match self.modes.view {
            View::Track | View::Mixer | View::Gen => self.run_track_pages(cmd),
            View::Chain => self.run_chain_view(cmd),
            View::Audio | View::Project => self.run_project_pages(cmd),
            View::Sample => self.run_sample_page(cmd),
            View::Song => self.run_song_view(cmd),
            View::Phrase => self.run_phrase_view(cmd),
        }
    }

    /// What the cursor is on, on the track page, the mixer or Gen.
    pub fn track_control(&self) -> Control {
        if self.modes.view == View::Mixer {
            return self.mixer_page.control_here(&self.ctx());
        }
        self.rack().control(self.services.track_here())
    }

    /// The rack the keys work on: Gen's while it shows, and the track page's otherwise, which the
    /// mixer's keys reach too.
    pub fn rack(&self) -> &TrackPage {
        if self.modes.view == View::Gen {
            &self.gen_page
        } else {
            &self.track_page
        }
    }

    /// The rack the keys work on, to change, as [`App::rack`] chooses it.
    pub(crate) fn rack_mut(&mut self) -> &mut TrackPage {
        if self.modes.view == View::Gen {
            &mut self.gen_page
        } else {
            &mut self.track_page
        }
    }

    /// Run a command on the track page, the mixer or Gen: the page's own key, and what a key it
    /// passes back means across pages, [`TrackPage::across`]. The mixer turns the track page's
    /// controls, so a key it passes back goes on to the track page's own keys first. A key that
    /// means what it does elsewhere, such as going to another track, leaves the page's cursor
    /// where it was kept, and the count for what takes it.
    fn run_track_pages(&mut self, cmd: Cmd) -> bool {
        self.gen_page.phrase = self.services.phrase_id();
        let kept = self.rack().clone();
        let count = self.modes.count.take();
        let times = count.unwrap_or(1).max(1);
        let view = self.modes.view;
        let rack = if view == View::Gen {
            &mut self.gen_page
        } else {
            &mut self.track_page
        };
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = match view {
            View::Mixer => match self.mixer_page.key(cmd, times, &mut ctx) {
                Outcome::Passed => rack.key(cmd, times, &mut ctx),
                outcome => outcome,
            },
            _ => rack.key(cmd, times, &mut ctx),
        };
        if self.settled(outcome, true) {
            return true;
        }
        let control = self.track_control();
        let rack = if view == View::Gen {
            &mut self.gen_page
        } else {
            &mut self.track_page
        };
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = rack.across(cmd, times, control, &mut ctx);
        // The count was taken before the page had the key.
        if self.settled(outcome, false) {
            return true;
        }
        *self.rack_mut() = kept;
        self.modes.count = count;
        false
    }

    /// Run a command on the project page's Song or Audio sub-page: the page's own key and what it
    /// asks for. The count stays for what takes it, as `gg` and `G` go to the row it names.
    fn run_project_pages(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = match self.modes.view {
            View::Audio => self.audio_page.key(cmd, times, &mut ctx),
            _ => self.project_page.key(cmd, times, &mut ctx),
        };
        self.settled(outcome, true)
    }

    /// Run a command on the sample page: the page's own key, and what it asks for, once the page
    /// is up to date with what it shows. With no sample to show, `K` lists the bank to choose one
    /// for the track, as `:bank` does, and a key that moves over the sample says why there's none,
    /// as there's nothing it can do, while any other goes on to mean what it does on every page.
    /// The count is taken by what takes it, and left for what passes it on.
    fn run_sample_page(&mut self, cmd: Cmd) -> bool {
        match self.sample_page_here() {
            Ok(_) => {}
            Err(_) if cmd == Cmd::PickValue => {
                return self.settled(Outcome::Run(Line::Bank), true);
            }
            Err(why) if is_sample_motion(cmd) => {
                self.services.status = why;
                self.modes.count = None;
                return true;
            }
            Err(_) => return false,
        }
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = self.sample_page.key(cmd, times, &mut ctx);
        self.settled(outcome, true)
    }

    /// Run a command in the song view: the view's own key, and what it asks for. What it passes
    /// back, the tree's `gd` and what means the same anywhere, leaves the count for what takes it.
    fn run_song_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = self.song_view.key(cmd, times, &mut ctx);
        self.settled(outcome, true)
    }

    /// Run a command in the chain view: the view's own key, and what it asks for. What it passes
    /// back goes on to the tree, with the count for what takes it.
    fn run_chain_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = self.chain_view.key(cmd, times, &mut ctx);
        self.settled(outcome, true)
    }

    /// Run a command in the phrase view: the view's own key, a move or one over the view, or
    /// pass it back for what's `App`'s, the tree's keys, the grammar's operators, edits and Visual
    /// modes, Insert, the loop's keys, recording and the pickers. A key it does takes the count,
    /// and one it passes back leaves it for what takes it.
    fn run_phrase_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
            wheel: &mut self.wheel,
        };
        let outcome = self.phrase_view.key(cmd, times, &mut ctx);
        self.settled(outcome, true)
    }

    /// Settle what a page's key did, or an overlay's: a key it did takes the count if
    /// `takes_count` says so, and one it refused takes it and says why on the status line. Then
    /// what it asks for happens: the picker opens or closes, with what was chosen in it done, the
    /// view shows or the line runs. Returns false for a key it passed back,
    /// for the next in the order to have.
    pub(crate) fn settled(&mut self, outcome: Outcome, takes_count: bool) -> bool {
        match outcome {
            Outcome::Passed => return false,
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                return true;
            }
            _ if takes_count => self.modes.count = None,
            _ => {}
        }
        match outcome {
            Outcome::Open(picker) => self.show_picker(*picker),
            Outcome::Show(view) => self.show_view(view),
            Outcome::Jump(view) => self.jump_to_view(view),
            Outcome::Run(line) => self.line(line),
            Outcome::Insert(entry) => {
                self.modes.entry = entry;
                self.modes.mode = Mode::Insert;
            }
            Outcome::Enter(mode) => self.modes.mode = mode,
            Outcome::Explore(naming) => {
                self.open_naming_explorer(naming.purpose, naming.command, naming.after);
            }
            Outcome::Hold(started) => self.held.started = Some(started),
            Outcome::Moved => self.phrase_view.top = self.services.cursor.step,
            Outcome::Close => match self.modes.mode {
                Mode::Help => {
                    self.overlays.help.scroll = 0;
                    self.modes.mode = self.overlays.help.back;
                }
                Mode::Guide => {
                    self.overlays.guide.open = None;
                    // Back to the shape panel, if it opened the guide or the list.
                    self.modes.mode = self.mode_after_picker();
                }
                Mode::Command => self.modes.mode = Mode::Normal,
                Mode::Jump => {
                    self.overlays.field_jump = None;
                    self.modes.mode = Mode::Normal;
                }
                Mode::Mapping => {
                    self.overlays.mapping = None;
                    self.modes.mode = Mode::Normal;
                }
                Mode::Macros => {
                    let panel = &mut self.overlays.macro_panel;
                    panel.typing = None;
                    self.modes.mode = panel.back;
                }
                Mode::Shape => {
                    self.overlays.shape = None;
                    self.modes.mode = Mode::Normal;
                }
                Mode::Tokens if self.overlays.shape.is_none() => self.leave_gen_tokens(),
                _ => {
                    self.overlays.picker = None;
                    // A shape waiting for its command is given up.
                    self.overlays.shape_command = None;
                    self.modes.mode = self.mode_after_picker();
                    self.overlays.explorer.leave(&mut self.services);
                }
            },
            Outcome::Choose(pick) => {
                let naming = self.overlays.explorer.naming.take();
                self.overlays.picker = None;
                self.modes.mode = self.mode_after_picker();
                self.overlays.explorer.leave(&mut self.services);
                self.overlays.explorer.naming = naming;
                self.choose(pick);
            }
            Outcome::Land { row, field } => {
                self.overlays.field_jump = None;
                self.modes.mode = Mode::Normal;
                self.record_jump();
                if self.modes.view == View::Mixer {
                    self.mixer_page.row = field;
                } else {
                    let track = self.services.track_here().cloned();
                    self.rack_mut().move_to(track.as_ref(), row, field);
                }
            }
            Outcome::Do(cmd) => self.run(cmd),
            Outcome::Shape(panel) => {
                panel.preview(&mut self.services);
                self.overlays.shape = Some(*panel);
                self.modes.mode = Mode::Shape;
            }
            Outcome::Reopen { field, step } => {
                self.show_view(View::Phrase);
                self.services.cursor.field = field;
                self.services.cursor.step = step;
                let outcome = ShapePanel::reopen(&mut self.services, self.modes.view);
                self.settled(outcome, false);
            }
            Outcome::Mapping(view) => {
                self.overlays.mapping = Some(view);
                self.modes.mode = Mode::Mapping;
            }
            Outcome::FieldJump => {
                self.modes.count = None;
                self.overlays.field_jump = Some(FieldJump::default());
                self.modes.mode = Mode::Jump;
            }
            Outcome::Tag { item, tags } => {
                if let Some(picker) = self.overlays.picker.as_mut()
                    && let Some(said) = self.overlays.explorer.tag_item(picker, item, &tags)
                {
                    self.services.status = said;
                }
                self.overlays.refresh_preview(self.services.rate);
            }
            Outcome::Keys(mode) => {
                self.overlays.help = Help::at(self.help_rows(), mode, self.modes.mode);
                self.modes.mode = Mode::Help;
            }
            Outcome::Done | Outcome::Refused(_) | Outcome::Passed => {}
        }
        true
    }
}

impl CtxMut<'_> {
    /// The current song.
    pub(crate) fn song(&self) -> &Arc<Song> {
        self.services.song()
    }

    /// The track under the cursor.
    pub(crate) fn track_here(&self) -> Option<&Track> {
        self.song().tracks.get(self.services.cursor.track)
    }
}

impl<'a> Ctx<'a> {
    /// The current song.
    pub fn song(&self) -> &'a Arc<Song> {
        self.services.song()
    }

    /// Where the cursor is in the song: its section, track, chain slot, step and field.
    pub fn cursor(&self) -> Cursor {
        self.services.cursor
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&'a Track> {
        self.services.track_here()
    }

    /// A track's name, or its number if it has none.
    pub(crate) fn track_name(&self, track: usize) -> String {
        self.services.track_name(track)
    }

    /// The chain on the cursor's track.
    pub fn chain_id(&self) -> Option<u32> {
        self.services.chain_id()
    }

    /// The rows in the cursor's chain.
    pub fn chain_entries(&self) -> &'a [ChainEntry] {
        self.services.chain_entries()
    }

    /// How many sections play the cursor's chain.
    pub fn chain_uses(&self) -> usize {
        self.services.chain_uses()
    }

    /// Which sections and slots play a phrase of the cursor's track.
    pub fn phrase_uses(&self, phrase: u32) -> Vec<(usize, usize)> {
        self.services.phrase_uses(phrase)
    }

    /// Which groups are folded, so their tracks are left out where tracks are drawn side by side.
    pub fn folds(&self) -> &'a Folds {
        &self.services.folds
    }

    /// The view on screen.
    pub fn view(&self) -> View {
        self.modes.view
    }

    /// The mode the keys are in.
    pub fn mode(&self) -> Mode {
        self.modes.mode
    }

    /// What's been typed in Insert and not yet written.
    pub fn entry(&self) -> &'a Entry {
        &self.modes.entry
    }

    /// The search for a setting under way, whose labels go over the settings it finds.
    pub fn field_jump(&self) -> Option<&'a FieldJump> {
        self.page.field_jump
    }

    /// How the modes show a setting's cell with the cursor on it this way: in the mode's colour,
    /// with what Insert has typed while the cursor's on it, and with no jump's labels, for a
    /// setting a jump doesn't reach.
    pub(crate) fn look(&self, focus: Focus) -> CellLook<'a> {
        let mode = self.mode();
        let typing = mode == Mode::Insert && focus != Focus::Off;
        CellLook {
            focus,
            mode,
            typed: typing.then(|| self.entry().text.as_str()),
            query: self.field_jump().map(|jump| jump.query.as_str()),
            name_labels: "",
            value_label: None,
        }
    }

    /// How the modes show a setting's cell, as `look`, with the labels a jump puts on it.
    pub(crate) fn look_on(&self, control: Control, focus: Focus) -> CellLook<'a> {
        let jump = self.field_jump();
        CellLook {
            name_labels: jump.map_or("", |jump| jump.label_text(control)),
            value_label: jump.and_then(|jump| jump.label(control)),
            ..self.look(focus)
        }
    }

    /// One of the track page's settings on a track of the song, as a widget draws it, with an
    /// accumulator's limits counted as the scale lock has them.
    pub(crate) fn param_view(&self, control: Control, track: &Track) -> Option<ParamView> {
        crate::controls::view(control, track, self.song(), self.services.scale_here().on)
    }

    /// Whether the song plays, as the engine last said.
    pub fn transport(&self) -> Transport {
        self.services.reported.transport
    }

    /// The section the engine is on.
    pub fn playing_section(&self) -> usize {
        self.services.reported.playing_section
    }

    /// The sections being kept playing, set with `:loop`.
    pub fn looping(&self) -> Option<SectionLoop> {
        self.services.reported.looping
    }

    /// Whether the `:loop` is waiting for the launch grid's next line, as the engine last said.
    pub fn loop_waiting(&self) -> bool {
        self.services.reported.loop_waiting
    }

    /// What the cursor's track is playing with right now, as the engine last said.
    pub fn live(&self) -> Option<LiveValues> {
        self.services.reported.live
    }

    /// What a Steps slot on the cursor's track has drawn with `Mutate`, as it's heard.
    pub(crate) fn drawn(&self, slot: usize) -> Drawn {
        self.services.drawn(slot)
    }

    /// The cursor's track as the latest report's live values were worked out from.
    pub(crate) fn reported_track(&self) -> Option<&'a Track> {
        self.services.reported_track()
    }

    /// What each track is launched onto and cued to, as the engine last said.
    pub fn jam(&self) -> &'a [JamState] {
        &self.services.reported.jam
    }

    /// Each track's live mute and solo.
    pub fn levels(&self) -> &'a [TrackLive] {
        &self.services.live.levels
    }

    /// How far a track is through this chain, if it's playing it, counting every time round it.
    pub(crate) fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
        self.services.playing_place(track, chain)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub(crate) fn dealer(&self, track: usize) -> Dealer {
        self.services.dealer(track)
    }

    /// How a track's settings move its phrases' walks, as the engine last read them while it plays.
    pub(crate) fn walk(&self, track: usize) -> WalkMove {
        self.services.walk(track)
    }

    /// The row a playing track's `Jump` played in place of the one walked to, if it jumped.
    pub(crate) fn jumped_row(&self, track: usize) -> Option<usize> {
        self.services.jumped_row(track)
    }

    /// A track's faders as they're heard.
    pub fn faders(&self, track: usize) -> Faders {
        self.services.faders(track)
    }

    /// A track as the pages draw it: where its macros put what they hold, then its faders where
    /// the steps have them while they drive them.
    pub(crate) fn track_shown(&self, track: usize) -> Option<Cow<'a, Track>> {
        self.services.track_shown(track)
    }

    /// A track's level, as its meter holds it.
    pub(crate) fn track_meter(&self, track: usize) -> Meter {
        self.services
            .track_meters
            .get(track)
            .copied()
            .unwrap_or_default()
    }

    /// Whether a control on a track is a fader the steps drive now, so it shows their value.
    pub fn follows_steps(&self, track: usize, control: Control) -> bool {
        self.services.follows_steps(track, control)
    }

    /// Whether a control on a track is a fader a hand has taken over from the steps.
    pub fn taken_over(&self, track: usize, control: Control) -> bool {
        self.services.taken_over(track, control)
    }

    /// A live generator of a track's phrase, by its place in the phrase's list, with what it
    /// works from.
    pub(crate) fn live_generator(
        &self,
        track: usize,
        phrase: u32,
        index: usize,
    ) -> Option<LiveGen> {
        self.services
            .live_generator(track, phrase, index, self.modes.view)
    }

    /// The shape panel, while it's open.
    pub(crate) fn shape_panel(&self) -> Option<&'a ShapePanel> {
        self.page.shape
    }

    /// The rows Gen's device lights in the grid beside it, while the keys are in the panel, and
    /// their column (decision 468).
    pub(crate) fn gen_lit(&self) -> Option<(ShapeColumn, StepRange)> {
        self.page.gen_lit
    }

    /// The buffer open on a setting of Gen's panel, while the panel's beside the grid.
    pub(crate) fn gen_tokens(&self) -> Option<&'a crate::pages::track::GenTokens> {
        self.page.gen_tokens
    }

    /// The pass of its phrase a live generator was last heard on, from 0.
    pub(crate) fn heard_pass(&self, track: usize, phrase: u32, generator: usize) -> i64 {
        self.services.heard_pass(track, phrase, generator)
    }

    /// The row of its span a live generator is playing, counted from its first, while the phrase
    /// it's on is the one under the cursor and is playing.
    pub(crate) fn gen_playing_row(&self, generator: &Generator) -> Option<usize> {
        crate::pages::track::draw::gen_playing_row(self.services, generator)
    }

    /// The sample pool, with each file's analysis and frames, for a sampler's thumbnail.
    pub fn samples(&self) -> &'a SamplePool {
        &self.services.samples
    }

    /// The program's sample rate, which the pictures of filters and tails are worked out at.
    pub fn rate(&self) -> u32 {
        self.services.rate
    }

    /// The project's name, which is the song's own.
    pub fn project_name(&self) -> String {
        self.services.project_name()
    }

    /// The folder the project is saved in, if it's been saved.
    pub fn folder(&self) -> Option<&'a Path> {
        self.services.folder.as_deref()
    }

    /// Whether the project has changed since it was last saved.
    pub fn dirty(&self) -> bool {
        self.services.dirty
    }

    /// The section the cursor is in, or the last if it's past them.
    pub fn section_here(&self) -> Option<&'a Section> {
        self.services.section_here()
    }

    /// The scale notes are entered in, the section under the cursor's or the song's.
    pub fn scale(&self) -> Scale {
        self.services.scale_here()
    }

    /// Whether entering in Insert moves on by the grid.
    pub fn auto_jump(&self) -> bool {
        self.services.live.auto_jump
    }

    /// Whether `:w` refuses to write over the project.
    pub fn read_only(&self) -> bool {
        self.services.live.read_only
    }

    /// The audio output as it plays, and what the audio page chooses from.
    pub fn audio(&self) -> &'a AudioState {
        self.page.audio
    }

    /// The Roto-Control: whether it's on, and the link to it.
    pub fn roto(&self) -> &'a Roto {
        &self.services.roto
    }

    /// Whether the mouse wheel turns the cursor's setting, and which way.
    pub fn wheel(&self) -> WheelSetting {
        self.page.wheel
    }

    /// The CPU the audio takes, as the system line shows it, in percent.
    pub(crate) fn cpu_percent(&self) -> u64 {
        self.page.cpu_percent
    }

    /// Whether power saving has slowed the CPU, so the figure is more than it would be.
    pub fn slowed(&self) -> bool {
        self.page.slowdown.slowed()
    }

    /// What the leaf before the CPU figure means, while it shows, as `slowed ×3.6 by power saving,
    /// about 8% at full speed`.
    pub(crate) fn slowed_meaning(&self) -> Option<String> {
        self.page.slowdown.explanation(self.page.cpu_percent)
    }

    /// What the sample page shows for the cursor's track, or why it can't show it: the entry the
    /// page is on, which a jump may have chosen, unless it's still recording.
    pub fn sample_shown(&self, page: &SamplePage) -> Result<Shown, String> {
        page.shown_while(self.services, self.page.recording)
    }

    /// Where each voice playing a sample through the cursor's track is in it, as the engine last
    /// said.
    pub fn playheads(&self) -> &'a [Option<Playhead>; PLAYHEADS] {
        &self.services.reported.playheads
    }

    /// Whether something is drawn over the page, which a picture placed above the text would show
    /// through.
    pub(crate) fn overlaid(&self, now: Instant) -> bool {
        self.page.covered || which_key_due(self.page.keys_since, now, POPUP_DELAY)
    }

    /// Which slice of its sample a track plays, as the Sound page's `Slice` says it after its
    /// number.
    pub(crate) fn slice_choice(&self, track: &Track) -> String {
        self.services.slice_choice(track)
    }

    /// The phrase under the cursor.
    pub fn phrase_id(&self) -> Option<u32> {
        self.services.phrase_id()
    }

    /// The contents of the phrase under the cursor.
    pub fn phrase_here(&self) -> Option<&'a Arc<Phrase>> {
        self.services.phrase_here()
    }

    /// An FX slot of the row under the cursor.
    pub(crate) fn slot_here(&self, slot: usize) -> Option<FxSlot> {
        self.services.current_slot(slot)
    }

    /// The gate cell under the cursor.
    pub(crate) fn gate_here(&self) -> Option<Gate> {
        self.services.current_gate()
    }

    /// Where the cursor's phrase's loop sits.
    pub(crate) fn extent(&self) -> Extent {
        self.services.extent()
    }

    /// The cursor's position within the section, on the view on screen.
    pub fn section_step(&self) -> usize {
        self.services.section_step(self.view())
    }

    /// The step of its own that one track is at when another is at this one of its own.
    pub(crate) fn step_on(&self, track: usize, from: usize, step: usize) -> usize {
        self.services.step_on(track, from, step)
    }

    /// The selection a replace across it or the shape panel is filling, which shows while Visual
    /// has ended.
    pub(crate) fn held_selection(&self) -> Option<Selection> {
        self.page.held_selection
    }

    /// Which step of the cursor's phrase the playhead is on, and which of the step's pulses, if
    /// it's on this phrase.
    pub(crate) fn playhead_place(&self) -> Option<(usize, usize)> {
        self.services.playhead_place()
    }

    /// Where each FX column of the cursor's phrase with a lane of its own is reading, if it's the
    /// one playing.
    pub(crate) fn lane_playheads(&self, phrase: &Phrase) -> [Option<usize>; FX_SLOTS] {
        self.services.lane_playheads(phrase)
    }

    /// How each FX column of the cursor's phrase with a lane of its own is heard, as the track's
    /// `Length` and `Rate` move it.
    pub(crate) fn heard_lanes(&self, phrase: &Phrase) -> [Option<vt_model::lane::Heard>; FX_SLOTS] {
        self.services.heard_lanes(phrase)
    }

    /// The note the cursor's track's row playing was heard at, where an accumulator or an `RND`
    /// moved it.
    pub(crate) fn heard_note(&self) -> Option<u8> {
        self.services.heard_note()
    }

    /// Whether the row the cursor's track is playing was skipped this time, by its `CHA` or `CND`.
    pub(crate) fn row_skipped(&self) -> bool {
        let services = self.services;
        services
            .reported
            .places
            .get(services.cursor.track)
            .is_some_and(|place| place.skipped)
    }

    /// A row with each live generator on it laid as last heard, if any was heard otherwise than
    /// written.
    pub(crate) fn heard_step(&self, track: usize, phrase: u32, row: usize) -> Option<Step> {
        self.services.heard_step(track, phrase, row)
    }

    /// A row of a live generator as it was last heard, if that wasn't what's written: the step
    /// as heard, and on which pass.
    pub(crate) fn heard_cell(
        &self,
        track: usize,
        phrase: u32,
        generator: usize,
        row: usize,
    ) -> Option<(Step, i64)> {
        self.services.heard_cell(track, phrase, generator, row)
    }

    /// Whether a live generator was heard otherwise than written on any of its rows.
    pub(crate) fn generator_moving(&self, track: usize, phrase: u32, generator: usize) -> bool {
        self.services.generator_moving(track, phrase, generator)
    }

    /// A field of the row under the cursor in its setting's units, where it has them, as the
    /// status line and the row inspector say it.
    pub(crate) fn phrase_value(&self, field: Field) -> Option<String> {
        crate::screen::status::phrase_value(self.services, field)
    }

    /// The furthest the cursor's track's nudges go at its speed.
    pub(crate) fn most_nudge_here(&self) -> i8 {
        self.services.most_nudge_here()
    }
}
