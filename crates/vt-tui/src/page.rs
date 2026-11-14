//! What a page is given to run its keys and draw with: the program's shared services, borrowed
//! (#320). A page, such as the track page, keeps its own state, its cursor, its images and where
//! it last drew them, and reads everything else through a [`Ctx`]: the song, the cursor's place in
//! it, the view and the modes that span pages, and what the engine last reported. `Ctx` is the
//! seam between a page and `App`: what a page may read of the program is what's here, and nothing
//! else of `App`'s. It reads them through `App` for now, as `App` still holds the pages, and only
//! reads: a page's edits, which apply actions, are still `App`'s.

use std::sync::Arc;

use vt_engine::engine::Transport;
use vt_engine::link::JamState;
use vt_engine::track_state::{Faders, LiveValues};
use vt_model::order::Dealer;
use vt_model::song::{Song, Track};

use crate::app::{App, Cursor, Entry, TrackLive};
use crate::input::mode::{Mode, View};
use crate::jumping::FieldJump;
use crate::shaping::LiveGen;
use crate::track_page::Control;

/// The shared services a page reads, borrowed from the program while it runs a key or is drawn.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    app: &'a App,
}

impl App {
    /// The shared services, for a page to run its keys and draw with.
    pub fn ctx(&self) -> Ctx<'_> {
        Ctx { app: self }
    }
}

impl<'a> Ctx<'a> {
    /// The current song.
    pub fn song(&self) -> &'a Arc<Song> {
        self.app.song()
    }

    /// Where the cursor is in the song: its section, track, chain slot, step and field.
    pub fn cursor(&self) -> Cursor {
        self.app.cursor
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&'a Track> {
        self.app.track_here()
    }

    /// The view on screen.
    pub fn view(&self) -> View {
        self.app.view
    }

    /// The mode the keys are in.
    pub fn mode(&self) -> Mode {
        self.app.mode
    }

    /// What's been typed in Insert and not yet written.
    pub fn entry(&self) -> &'a Entry {
        &self.app.entry
    }

    /// The search for a setting under way, whose labels go over the settings it finds.
    pub fn field_jump(&self) -> Option<&'a FieldJump> {
        self.app.field_jump.as_ref()
    }

    /// Whether the song plays, as the engine last said.
    pub fn transport(&self) -> Transport {
        self.app.transport
    }

    /// What the cursor's track is playing with right now, as the engine last said.
    pub fn live(&self) -> Option<LiveValues> {
        self.app.live
    }

    /// The cursor's track as the latest report's live values were worked out from.
    pub fn reported_track(&self) -> Option<&'a Track> {
        self.app.reported_track()
    }

    /// What each track is launched onto and cued to, as the engine last said.
    pub fn jam(&self) -> &'a [JamState] {
        &self.app.jam
    }

    /// Each track's live mute and solo.
    pub fn levels(&self) -> &'a [TrackLive] {
        &self.app.levels
    }

    /// How far a track is through this chain, if it's playing it, counting every time round it.
    pub fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
        self.app.playing_place(track, chain)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub fn dealer(&self, track: usize) -> Dealer {
        self.app.dealer(track)
    }

    /// A track's faders as they're heard.
    pub fn faders(&self, track: usize) -> Faders {
        self.app.faders(track)
    }

    /// Whether a control on a track is a fader the steps drive now, so it shows their value.
    pub fn follows_steps(&self, track: usize, control: Control) -> bool {
        self.app.follows_steps(track, control)
    }

    /// Whether a control on a track is a fader a hand has taken over from the steps.
    pub fn taken_over(&self, track: usize, control: Control) -> bool {
        self.app.taken_over(track, control)
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

    /// Which slice of its sample a track plays, as the Sound page's `Slice` says it after its
    /// number.
    pub fn slice_choice(&self, track: &Track) -> String {
        self.app.slice_choice(track)
    }
}
