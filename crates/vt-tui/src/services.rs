//! What every page shares (#320): the song and its history, which actions are applied to, the
//! engine and what it last reported, the sample pool and its analyses, this machine's settings,
//! the Roto and the status line. `App` holds them beside the pages, so a page running a key can
//! borrow them mutably while it borrows itself, and an edit on a page applies an action as one in
//! `App` does.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use vt_engine::engine::{PLAYHEADS, Playhead, TrackPlace, Transport};
use vt_engine::gens::{GenPass, GenRow};
use vt_engine::link::{Handle, JamState};
use vt_engine::published::Published;
use vt_engine::track_state::{Faders, LiveValues, NotePitch};
use vt_model::action::{Action, edit};
use vt_model::constants::MAX_TRACKS;
use vt_model::history::History;
use vt_model::order::Dealer;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::analysing::Analyst;
use crate::app::TrackLive;
use crate::config::Settings;
use crate::meter::Meter;

/// How many versions sent are kept for a report to be of. The callback takes each within a block,
/// so only a few are ever waiting; these are for when it has stopped.
const SENT_KEPT: usize = 64;

/// The program's shared services.
pub struct Services {
    pub history: History,
    pub folder: Option<PathBuf>,
    pub dirty: bool,
    pub status: String,
    pub samples: Arc<SamplePool>,
    /// Analyses the bank's files, off this thread, for normalising, `:trim` and the waveform.
    pub analyst: Analyst,
    /// Where this machine keeps its kits and samples.
    pub settings: Settings,
    /// The Roto-Control: whether it's on, the link to it, and what it shows.
    pub roto: crate::roto::Roto,
    pub handle: Handle,
    pub rate: u32,
    /// The version last sent to the audio thread, whose bank entries the next keeps where they're
    /// the same.
    published: Option<Arc<Published>>,
    /// The songs of the versions sent that a report could still be of, by their ids, oldest
    /// first, and the song the live values were worked out from, so a bar draws what's playing
    /// against the setting it was reported with (#421). The engine starts on the song the app
    /// starts with, before any version is sent.
    pub(crate) sent: VecDeque<(u64, Arc<Song>)>,
    pub(crate) live_song: Option<Arc<Song>>,
    /// The track the engine is asked to report the live values of.
    pub(crate) watched: usize,
    /// The time of the frame or key being handled, which the playhead is carried on to from the
    /// engine's last report, so what's recorded goes by the times the app is given and never
    /// reads the clock itself.
    pub(crate) now: Instant,
    pub(crate) last_tick: Instant,
    pub transport: Transport,
    pub playhead: usize,
    /// How far through the playhead's step the engine last said it was, and when that was read.
    pub progress: f32,
    /// Where each track is heard, as its own steps count, which a track at another speed than the
    /// song's needs.
    pub places: [TrackPlace; MAX_TRACKS],
    pub(crate) report_at: Instant,
    /// The section the engine is on.
    pub playing_section: usize,
    /// Whether the `:loop` is waiting for the launch grid's next line, as the engine last said.
    pub loop_waiting: bool,
    /// Whether a realign is waiting for the launch grid, as the engine last said.
    pub realigning: bool,
    /// Whether each track's own realign is waiting for the launch grid, as the engine last said.
    pub realigning_tracks: [bool; MAX_TRACKS],
    /// What each track is launched onto and cued to, as the engine last said.
    pub jam: Vec<JamState>,
    /// Live mute and solo per track, kept outside undo and written to the song on save.
    pub levels: Vec<TrackLive>,
    /// What the cursor's track is playing with right now, and the track the engine reports.
    pub live: Option<LiveValues>,
    /// What each mod slot moved each row's note by on `Pitch` the last time it was heard, and
    /// what the row's own `RND`s moved it by, by track, phrase and row, in semitones, as the engine
    /// reports them, for `Space g A` (#569, decision 323).
    pub(crate) heard_pitches: HashMap<(usize, u32, usize), NotePitch>,
    /// The pass each live generator that varies was last heard on and the seeds it played, by
    /// track, phrase and its place in the phrase's list, as the engine reports them, for
    /// `Space g a` on the Gen page to keep (decision 293).
    pub(crate) heard_passes: HashMap<(usize, u32, usize), GenPass>,
    /// Each row of a live generator as it was last heard, by track, phrase, the generator's place
    /// in the phrase's list and the stored row, where something moved it from what's written,
    /// as the engine reports them: for the playhead's row, the overview, the phrase's `~` and the
    /// status line to show, and `Space g a` and `Space g A` to keep (decision 300).
    pub(crate) heard_rows: HashMap<(usize, u32, usize, usize), GenRow>,
    /// Every track's faders as they're heard: where the steps have them, and which a hand has
    /// taken over, as the engine last said, with what's been asked of it since (decision 174).
    pub(crate) faders: [Faders; MAX_TRACKS],
    pub(crate) fader_wishes: Vec<crate::automating::Wish>,
    /// Where each voice playing a sample through the cursor's track is in it, for the sample
    /// page's playheads.
    pub playheads: [Option<Playhead>; PLAYHEADS],
    /// The main output's level.
    pub meter: Meter,
    /// The main output's recent sound, for the scope above the meter.
    pub scope: crate::scope::Scope,
    /// Each track's level, for the mixer.
    pub track_meters: Vec<Meter>,
}

impl Services {
    /// Start with a song, its project folder if it has one, and a connection to the engine.
    pub(crate) fn new(
        song: Song,
        folder: Option<PathBuf>,
        samples: Arc<SamplePool>,
        handle: Handle,
        rate: u32,
    ) -> Self {
        let levels = live_from(&song);
        Self {
            history: History::new(Arc::new(song)),
            folder,
            dirty: false,
            status: String::new(),
            samples,
            analyst: Analyst::new(None),
            settings: Settings::none(),
            roto: crate::roto::Roto::default(),
            handle,
            rate,
            published: None,
            sent: VecDeque::new(),
            live_song: None,
            watched: 0,
            now: Instant::now(),
            last_tick: Instant::now(),
            transport: Transport::Stopped,
            playhead: 0,
            progress: 0.0,
            places: [TrackPlace::default(); MAX_TRACKS],
            report_at: Instant::now(),
            playing_section: 0,
            loop_waiting: false,
            realigning: false,
            realigning_tracks: [false; MAX_TRACKS],
            jam: vec![JamState::default(); MAX_TRACKS],
            levels,
            live: None,
            heard_pitches: HashMap::new(),
            heard_passes: HashMap::new(),
            heard_rows: HashMap::new(),
            faders: [Faders::default(); MAX_TRACKS],
            fader_wishes: Vec::new(),
            playheads: [None; PLAYHEADS],
            meter: Meter::default(),
            scope: crate::scope::Scope::new(rate),
            track_meters: vec![Meter::default(); MAX_TRACKS],
        }
    }

    /// The current song.
    pub fn song(&self) -> &Arc<Song> {
        self.history.current()
    }

    /// Run an action and make the result current. A fader it moves that the steps drive is
    /// taken over by the hand that moved it.
    pub fn apply(&mut self, action: &Action) {
        match edit(self.song(), action) {
            Ok(next) => {
                self.say_applied(&next);
                self.take_over_moved(action);
                self.history.commit(next);
                self.dirty = true;
                self.publish();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Send the current version to the audio thread. Only the bank entries that changed are
    /// worked out again.
    pub fn publish(&mut self) {
        let version = Arc::new(Published::after(
            Arc::clone(self.song()),
            Arc::clone(&self.samples),
            self.published.as_deref(),
        ));
        self.published = Some(Arc::clone(&version));
        self.sent.push_back((version.id, Arc::clone(&version.song)));
        if self.sent.len() > SENT_KEPT {
            self.sent.pop_front();
        }
        self.handle.publish(version);
        self.analyse_bank();
    }

    /// Note which version a report is of: the versions before it are done with, as the callback
    /// never goes back to one, and its song is what the live values were worked out from. A
    /// report of a version before any still kept, such as the one the engine started on, keeps
    /// the song it was last of.
    pub(crate) fn follow_version(&mut self, version: u64) {
        while self.sent.front().is_some_and(|(id, _)| *id < version) {
            self.sent.pop_front();
        }
        if let Some((_, song)) = self.sent.front().filter(|(id, _)| *id == version) {
            self.live_song = Some(Arc::clone(song));
        }
    }

    /// How far a track is through this chain, if it's playing it, counting every time round it
    /// since it started, as the engine does to deal a dealt order, for finding the row playing.
    pub fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
        let moving = matches!(self.transport, Transport::Playing | Transport::Paused);
        let jam = self.jam.get(track)?;
        if !moving || jam.chain != Some(chain) {
            return None;
        }
        let length = (self.song().tracks.get(track)?.chain_steps(chain) as i64).max(1);
        let place = self.places.get(track)?.count - jam.started;
        let laps = place.div_euclid(length).max(0);
        Some((laps * length + place.rem_euclid(length)) as usize)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub fn dealer(&self, track: usize) -> Dealer {
        Dealer::new(self.song().seed, track)
    }
}

/// Each track's live mute and solo, as a song has them saved.
pub(crate) fn live_from(song: &Song) -> Vec<TrackLive> {
    song.tracks
        .iter()
        .map(|t| TrackLive {
            mute: t.mute,
            solo: t.solo,
        })
        .collect()
}
