//! What a page is given to run its keys and draw with: the program's shared services, borrowed
//! (#320). A page, such as the track page or the mixer, keeps its own state, its cursor, its
//! images and where it last drew them, and reaches everything else through what it's given.
//!
//! - **[`CtxMut`]**, to run a key: the [`Services`], the song, the cursor and the rest, borrowed
//!   mutably apart from the page itself, so a key can change the page and apply an action at once,
//!   and the [`Modes`] that span the pages, the view and the mode among them, to read.
//! - **[`Ctx`]**, to be drawn: everything a page reads, the song, the cursor's place in it, the
//!   view and the modes that span pages, which groups are folded, and what the engine last
//!   reported, the tracks' meters among it. It only reads, and reads through `App`, as drawing is
//!   given `&App` and some of what's drawn, a live generator's sources, is worked out by `App`.
//!
//! - **[`Page`]**, what a page is as a component: its keys in the binding table, how it runs one,
//!   and its drawing and pictures. `App` holds one of each page and dispatches by `View` with a
//!   plain `match`, in [`App::run_page`], so there's no `dyn` and no downcast.
//!
//! They're the seam between a page and `App`: what a page may reach of the program is what's
//! here, and nothing else of `App`'s.

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_engine::engine::{PLAYHEADS, Playhead, SectionLoop, Transport};
use vt_engine::link::JamState;
use vt_engine::track_state::{Faders, LiveValues};
use vt_model::constants::FX_SLOTS;
use vt_model::generator::Generator;
use vt_model::order::Dealer;
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
use vt_model::selection::{Extent, Field};
use vt_model::song::{ChainEntry, Phrase, Section, Song, Track};
use vt_model::step::Step;

use crate::app::{App, AudioState, Cursor, Entry, Modes, Selection, TrackLive};
use crate::folding::Folds;
use crate::input::bindings::Cmd;
use crate::input::mode::{Mode, View};
use crate::jumping::FieldJump;
use crate::kitty::{PanelIds, PanelWant};
use crate::meter::Meter;
use crate::phrase_view::PhraseView;
use crate::roto::Roto;
use crate::sample_page::{Shown, is_sample_motion};
use crate::services::Services;
use crate::shaping::LiveGen;
use crate::track_page::{Control, ParamView};
use crate::views::widgets::setting::{CellLook, Focus};
use crate::wheel::WheelSetting;

/// What a key did, as far as the page alone can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The page did it.
    Done,
    /// It can't be done here, and the status line says why.
    Refused(&'static str),
    /// It isn't one of the page's own keys: the next in the order [`App::run`] gives keys in gets
    /// it.
    Passed,
}

/// One of the program's pages as a component: its own state, its keys, its drawing and its
/// pictures. The track page, the mixer, the project page's two sub-pages, the sample page, the
/// song view, the chain view and the phrase view are so far (#739); the other pages' keys and
/// drawing are still `App`'s.
pub(crate) trait Page {
    /// Its rows of the binding table, as `(keys, Cmd, description)`, in the order which-key and
    /// `Space k` list them. The table is still assembled in one place, `input::bindings`, under
    /// the page's mode.
    const KEYS: &'static [(&'static str, Cmd, &'static str)];

    /// What its drawing notes for after the frame, such as where it drew what the terminal places
    /// a dot on, or nothing.
    type Drawn;

    /// Run one of its keys `count` times, at least 1, changing its own state directly and the
    /// song through the services' actions, or pass it back.
    fn key(&mut self, cmd: Cmd, count: usize, ctx: &mut CtxMut) -> Outcome;

    /// Draw it.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) -> Self::Drawn;

    /// What pictures it shows this frame, as data, for `Graphics` to send before ratatui draws.
    /// Most pages have none.
    fn panels(&self, _ctx: &Ctx) -> Vec<PanelWant> {
        Vec::new()
    }

    /// Keep the ids `Graphics` gave the pictures it wanted, for `draw` to put placeholders for.
    fn take_panels(&mut self, _ids: PanelIds, _wants: &[PanelWant], _rate: u32) {}
}

/// What a page reads of the program, borrowed from it while the page is drawn.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    app: &'a App,
}

/// What a page is given to run a key: the shared services, to read and change, and the modes that
/// span the pages, to read, borrowed apart from the page.
pub struct CtxMut<'a> {
    pub services: &'a mut Services,
    pub modes: &'a Modes,
}

impl App {
    /// What a page reads, to be drawn with.
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx { app: self }
    }

    /// Run a command on the page on screen, the second place a key goes in [`App::run`]'s order:
    /// through [`Page::key`] on a page that's a component, and then `App`'s commands across
    /// pages for what it passed back, or the page's own `run_*` on the pages that aren't yet.
    /// Returns false if it's none of theirs.
    pub(crate) fn run_page(&mut self, cmd: Cmd) -> bool {
        match self.modes.view {
            View::Track | View::Mixer => self.run_track_pages(cmd),
            View::Chain => self.run_chain_view(cmd),
            View::Audio | View::Project => self.run_project_pages(cmd),
            View::Sample => self.run_sample_page(cmd),
            View::Song => self.run_song_view(cmd),
            View::Phrase => self.run_phrase_view(cmd),
        }
    }

    /// Run a command on the track page or the mixer: the page's own key, and what a key it passes
    /// back means across pages, [`App::run_across_pages`]. The mixer turns the track page's
    /// controls, so a key it passes back goes on to the track page's own keys first. A key that
    /// means what it does elsewhere, such as going to another track, leaves the track page's
    /// cursor where it was kept, and the count for what takes it.
    fn run_track_pages(&mut self, cmd: Cmd) -> bool {
        self.track_page.phrase = self.services.phrase_id();
        let kept = self.track_page.clone();
        let count = self.modes.count.take();
        let times = count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        let outcome = match self.modes.view {
            View::Mixer => match self.mixer_page.key(cmd, times, &mut ctx) {
                Outcome::Passed => self.track_page.key(cmd, times, &mut ctx),
                outcome => outcome,
            },
            _ => self.track_page.key(cmd, times, &mut ctx),
        };
        match outcome {
            Outcome::Done => true,
            Outcome::Refused(why) => {
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed if self.run_across_pages(cmd, times) => true,
            Outcome::Passed => {
                self.track_page = kept;
                self.modes.count = count;
                false
            }
        }
    }

    /// Run a command on the project page's Song or Audio sub-page: the page's own key, a move,
    /// and then what it passes back, an edit or `H` and `L`, which are `App`'s, as an edit runs
    /// its row's command or opens another output. The count stays for what takes it, as `gg` and
    /// `G` go to the row it names.
    fn run_project_pages(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        let outcome = match self.modes.view {
            View::Audio => self.audio_page.key(cmd, times, &mut ctx),
            _ => self.project_page.key(cmd, times, &mut ctx),
        };
        match outcome {
            Outcome::Done => {
                self.modes.count = None;
                true
            }
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed if self.modes.view == View::Audio => self.run_audio_page(cmd),
            Outcome::Passed => self.run_project_page(cmd),
        }
    }

    /// Run a command on the sample page: the page's own key, and then what it passes back, which
    /// is `App`'s, as it plays, starts the pads, waits for a mark's letter or opens the picker. A
    /// key that moves over the sample says why there's none to show, as there's nothing it can
    /// do, while any other goes on to mean what it does on every page. The count is taken by
    /// what takes it, and left for what passes it on.
    fn run_sample_page(&mut self, cmd: Cmd) -> bool {
        let shown = match self.sample_page_here() {
            Ok(shown) => shown,
            Err(why) if is_sample_motion(cmd) => {
                self.services.status = why;
                self.modes.count = None;
                return true;
            }
            Err(_) => return false,
        };
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        match self.sample_page.key(cmd, times, &mut ctx) {
            Outcome::Done => {
                self.modes.count = None;
                true
            }
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed => self.run_sample_passed(cmd, &shown),
        }
    }

    /// Run a command in the song view: the view's own key, and then what it passes back, a move
    /// to another section and what ends on one, a picker, Insert, `gC` and `;`, which are `App`'s.
    /// The count stays for what takes it.
    fn run_song_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        match self.song_view.key(cmd, times, &mut ctx) {
            Outcome::Done => {
                self.modes.count = None;
                true
            }
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed => self.run_song_passed(cmd),
        }
    }

    /// Run a command in the chain view: the view's own key, and then what it passes back, a move
    /// to another row, `dd`, the phrase picker, Insert and `;`, which are `App`'s. What neither
    /// takes goes on to the tree. The count stays for what takes it.
    fn run_chain_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        match self.chain_view.key(cmd, times, &mut ctx) {
            Outcome::Done => {
                self.modes.count = None;
                true
            }
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed => self.run_chain_passed(cmd),
        }
    }

    /// Run a command in the phrase view: the view's own key, a move or one over the view, or
    /// pass it back for what's `App`'s, the tree's keys, the grammar's operators, edits and Visual
    /// modes, Insert, the loop's keys, recording and the pickers. A move takes the count, and a
    /// key it passes back leaves it for what takes it.
    fn run_phrase_view(&mut self, cmd: Cmd) -> bool {
        let times = self.modes.count.unwrap_or(1).max(1);
        let mut ctx = CtxMut {
            services: &mut self.services,
            modes: &self.modes,
        };
        match self.phrase_view.key(cmd, times, &mut ctx) {
            Outcome::Done => {
                if PhraseView::takes_count(cmd) {
                    self.modes.count = None;
                }
                true
            }
            Outcome::Refused(why) => {
                self.modes.count = None;
                self.services.status = why.to_owned();
                true
            }
            Outcome::Passed => false,
        }
    }
}

impl CtxMut<'_> {
    /// The current song.
    pub fn song(&self) -> &Arc<Song> {
        self.services.song()
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&Track> {
        self.song().tracks.get(self.services.cursor.track)
    }
}

impl<'a> Ctx<'a> {
    /// The current song.
    pub fn song(&self) -> &'a Arc<Song> {
        self.app.song()
    }

    /// Where the cursor is in the song: its section, track, chain slot, step and field.
    pub fn cursor(&self) -> Cursor {
        self.app.services.cursor
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&'a Track> {
        self.app.services.track_here()
    }

    /// A track's name, or its number if it has none.
    pub fn track_name(&self, track: usize) -> String {
        crate::views::phrase::track_name(self.app, track)
    }

    /// The chain on the cursor's track.
    pub fn chain_id(&self) -> Option<u32> {
        self.app.services.chain_id()
    }

    /// The rows in the cursor's chain.
    pub fn chain_entries(&self) -> &'a [ChainEntry] {
        self.app.services.chain_entries()
    }

    /// How many sections play the cursor's chain.
    pub fn chain_uses(&self) -> usize {
        self.app.chain_uses()
    }

    /// Which sections and slots play a phrase of the cursor's track.
    pub fn phrase_uses(&self, phrase: u32) -> Vec<(usize, usize)> {
        self.app.phrase_uses(phrase)
    }

    /// Which groups are folded, so their tracks are left out where tracks are drawn side by side.
    pub fn folds(&self) -> &'a Folds {
        &self.app.services.folds
    }

    /// The view on screen.
    pub fn view(&self) -> View {
        self.app.modes.view
    }

    /// The mode the keys are in.
    pub fn mode(&self) -> Mode {
        self.app.modes.mode
    }

    /// What's been typed in Insert and not yet written.
    pub fn entry(&self) -> &'a Entry {
        &self.app.modes.entry
    }

    /// The search for a setting under way, whose labels go over the settings it finds.
    pub fn field_jump(&self) -> Option<&'a FieldJump> {
        self.app.modes.field_jump.as_ref()
    }

    /// How the modes show a setting's cell with the cursor on it this way: in the mode's colour,
    /// with what Insert has typed while the cursor's on it, and with no jump's labels, for a
    /// setting a jump doesn't reach.
    pub fn look(&self, focus: Focus) -> CellLook<'a> {
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
    pub fn look_on(&self, control: Control, focus: Focus) -> CellLook<'a> {
        let jump = self.field_jump();
        CellLook {
            name_labels: jump.map_or("", |jump| jump.label_text(control)),
            value_label: jump.and_then(|jump| jump.label(control)),
            ..self.look(focus)
        }
    }

    /// One of the track page's settings on a track of the song, as a widget draws it, with an
    /// accumulator's limits counted as the scale lock has them.
    pub fn param_view(&self, control: Control, track: &Track) -> Option<ParamView> {
        crate::track_page::view(control, track, self.song(), self.app.scale.on)
    }

    /// Whether the song plays, as the engine last said.
    pub fn transport(&self) -> Transport {
        self.app.services.transport
    }

    /// The section the engine is on.
    pub fn playing_section(&self) -> usize {
        self.app.services.playing_section
    }

    /// The sections being kept playing, set with `:loop`.
    pub fn looping(&self) -> Option<SectionLoop> {
        self.app.services.looping
    }

    /// Whether the `:loop` is waiting for the launch grid's next line, as the engine last said.
    pub fn loop_waiting(&self) -> bool {
        self.app.services.loop_waiting
    }

    /// What the cursor's track is playing with right now, as the engine last said.
    pub fn live(&self) -> Option<LiveValues> {
        self.app.services.live
    }

    /// The cursor's track as the latest report's live values were worked out from.
    pub fn reported_track(&self) -> Option<&'a Track> {
        self.app.reported_track()
    }

    /// What each track is launched onto and cued to, as the engine last said.
    pub fn jam(&self) -> &'a [JamState] {
        &self.app.services.jam
    }

    /// Each track's live mute and solo.
    pub fn levels(&self) -> &'a [TrackLive] {
        &self.app.services.levels
    }

    /// How far a track is through this chain, if it's playing it, counting every time round it.
    pub fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
        self.app.services.playing_place(track, chain)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub fn dealer(&self, track: usize) -> Dealer {
        self.app.services.dealer(track)
    }

    /// A track's faders as they're heard.
    pub fn faders(&self, track: usize) -> Faders {
        self.app.services.faders(track)
    }

    /// A track as the pages draw it: where its macros put what they hold, then its faders where
    /// the steps have them while they drive them.
    pub fn track_shown(&self, track: usize) -> Option<Cow<'a, Track>> {
        self.app.services.track_shown(track)
    }

    /// A track's level, as its meter holds it.
    pub fn track_meter(&self, track: usize) -> Meter {
        self.app
            .services
            .track_meters
            .get(track)
            .copied()
            .unwrap_or_default()
    }

    /// Whether a control on a track is a fader the steps drive now, so it shows their value.
    pub fn follows_steps(&self, track: usize, control: Control) -> bool {
        self.app.services.follows_steps(track, control)
    }

    /// Whether a control on a track is a fader a hand has taken over from the steps.
    pub fn taken_over(&self, track: usize, control: Control) -> bool {
        self.app.services.taken_over(track, control)
    }

    /// A live generator of a track's phrase, by its place in the phrase's list, with what it
    /// works from.
    pub(crate) fn live_generator(
        &self,
        track: usize,
        phrase: u32,
        index: usize,
    ) -> Option<LiveGen> {
        self.app.live_generator(track, phrase, index)
    }

    /// The row of its span a live generator is playing, counted from its first, while the phrase
    /// it's on is the one under the cursor and is playing.
    pub(crate) fn gen_playing_row(&self, generator: &Generator) -> Option<usize> {
        crate::views::track::gen_playing_row(self.app, generator)
    }

    /// The sample pool, with each file's analysis and frames, for a sampler's thumbnail.
    pub fn samples(&self) -> &'a SamplePool {
        &self.app.services.samples
    }

    /// The program's sample rate, which the pictures of filters and tails are worked out at.
    pub fn rate(&self) -> u32 {
        self.app.services.rate
    }

    /// The project's name, which is the song's own.
    pub fn project_name(&self) -> String {
        self.app.project_name()
    }

    /// The folder the project is saved in, if it's been saved.
    pub fn folder(&self) -> Option<&'a Path> {
        self.app.services.folder.as_deref()
    }

    /// Whether the project has changed since it was last saved.
    pub fn dirty(&self) -> bool {
        self.app.services.dirty
    }

    /// The section the cursor is in, or the last if it's past them.
    pub fn section_here(&self) -> Option<&'a Section> {
        self.app.services.section_here()
    }

    /// The scale notes are entered in.
    pub fn scale(&self) -> Scale {
        self.app.scale
    }

    /// Whether entering in Insert moves on by the grid.
    pub fn auto_jump(&self) -> bool {
        self.app.auto_jump
    }

    /// Whether `:w` refuses to write over the project.
    pub fn read_only(&self) -> bool {
        self.app.read_only
    }

    /// The audio output as it plays, and what the audio page chooses from.
    pub fn audio(&self) -> &'a AudioState {
        &self.app.audio
    }

    /// The Roto-Control: whether it's on, and the link to it.
    pub fn roto(&self) -> &'a Roto {
        &self.app.services.roto
    }

    /// Whether the mouse wheel turns the cursor's setting, and which way.
    pub fn wheel(&self) -> WheelSetting {
        self.app.wheel.setting
    }

    /// The CPU the audio takes, as the system line shows it, in percent.
    pub fn cpu_percent(&self) -> u64 {
        self.app.cpu.percent()
    }

    /// Whether power saving has slowed the CPU, so the figure is more than it would be.
    pub fn slowed(&self) -> bool {
        self.app.slowdown.slowed()
    }

    /// What the sample page shows for the cursor's track, or why it can't show it.
    pub fn sample_shown(&self) -> Result<Shown, String> {
        self.app.sample_shown()
    }

    /// Where each voice playing a sample through the cursor's track is in it, as the engine last
    /// said.
    pub fn playheads(&self) -> &'a [Option<Playhead>; PLAYHEADS] {
        &self.app.services.playheads
    }

    /// Whether something is drawn over the page, which a picture placed above the text would show
    /// through.
    pub fn overlaid(&self, now: Instant) -> bool {
        crate::views::sample::overlaid(self.app, now)
    }

    /// Which slice of its sample a track plays, as the Sound page's `Slice` says it after its
    /// number.
    pub fn slice_choice(&self, track: &Track) -> String {
        self.app.slice_choice(track)
    }

    /// The phrase under the cursor.
    pub fn phrase_id(&self) -> Option<u32> {
        self.app.services.phrase_id()
    }

    /// The contents of the phrase under the cursor.
    pub fn phrase_here(&self) -> Option<&'a Arc<Phrase>> {
        self.app.services.phrase_here()
    }

    /// Where the cursor's phrase's loop sits.
    pub fn extent(&self) -> Extent {
        self.app.services.extent()
    }

    /// The cursor's position within the section, on the view on screen.
    pub fn section_step(&self) -> usize {
        self.app.services.section_step(self.view())
    }

    /// The step of its own that one track is at when another is at this one of its own.
    pub fn step_on(&self, track: usize, from: usize, step: usize) -> usize {
        self.app.services.step_on(track, from, step)
    }

    /// The selection a replace across it or the shape panel is filling, which shows while Visual
    /// has ended.
    pub fn held_selection(&self) -> Option<Selection> {
        let app = self.app;
        app.replacing_selection
            .map(|(selection, _)| selection)
            .or(app.shape.as_ref().map(|panel| panel.selection))
    }

    /// Which step of the cursor's phrase the playhead is on, and which of the step's pulses, if
    /// it's on this phrase.
    pub fn playhead_place(&self) -> Option<(usize, usize)> {
        crate::views::phrase::playhead_place(self.app)
    }

    /// Where each FX column of the cursor's phrase with a lane of its own is reading, if it's the
    /// one playing.
    pub fn lane_playheads(&self, phrase: &Phrase) -> [Option<usize>; FX_SLOTS] {
        crate::views::phrase::lane_playheads(self.app, phrase)
    }

    /// The note the cursor's track's row playing was heard at, where an accumulator or an `RND`
    /// moved it.
    pub fn heard_note(&self) -> Option<u8> {
        crate::views::phrase::heard_note(self.app)
    }

    /// Whether the row the cursor's track is playing was skipped this time, by its `CHA` or `CND`.
    pub fn row_skipped(&self) -> bool {
        let services = &self.app.services;
        services
            .places
            .get(services.cursor.track)
            .is_some_and(|place| place.skipped)
    }

    /// A row with each live generator on it laid as last heard, if any was heard otherwise than
    /// written.
    pub fn heard_step(&self, track: usize, phrase: u32, row: usize) -> Option<Step> {
        self.app.heard_step(track, phrase, row)
    }

    /// A row of a live generator as it was last heard, if that wasn't what's written: the step
    /// as heard, and on which pass.
    pub fn heard_cell(
        &self,
        track: usize,
        phrase: u32,
        generator: usize,
        row: usize,
    ) -> Option<(Step, i64)> {
        self.app.heard_cell(track, phrase, generator, row)
    }

    /// Whether a live generator was heard otherwise than written on any of its rows.
    pub fn generator_moving(&self, track: usize, phrase: u32, generator: usize) -> bool {
        self.app.generator_moving(track, phrase, generator)
    }

    /// A field of the row under the cursor in its setting's units, where it has them, as the
    /// status line and the row inspector say it.
    pub fn phrase_value(&self, field: Field) -> Option<String> {
        crate::views::status::phrase_value(self.app, field)
    }

    /// The furthest the cursor's track's nudges go at its speed.
    pub fn most_nudge_here(&self) -> i8 {
        self.app.most_nudge_here()
    }
}
