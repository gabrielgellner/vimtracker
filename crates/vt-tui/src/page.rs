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
//! They're the seam between a page and `App`: what a page may reach of the program is what's
//! here, and nothing else of `App`'s.

use std::borrow::Cow;
use std::sync::Arc;
use std::time::Instant;

use vt_engine::engine::Transport;
use vt_engine::link::JamState;
use vt_engine::track_state::{Faders, LiveValues};
use vt_model::generator::Generator;
use vt_model::order::Dealer;
use vt_model::samples::SamplePool;
use vt_model::song::{Song, Track};

use crate::app::{App, Cursor, Entry, Modes, TrackLive};
use crate::folding::Folds;
use crate::input::mode::{Mode, View};
use crate::jumping::FieldJump;
use crate::meter::Meter;
use crate::services::Services;
use crate::shaping::LiveGen;
use crate::track_page::Control;
use crate::views::widgets::setting::{CellLook, Focus};

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

    /// Whether the song plays, as the engine last said.
    pub fn transport(&self) -> Transport {
        self.app.services.transport
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
}
