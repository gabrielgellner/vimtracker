//! What every page shares (#320): the song and its history, which actions are applied to, the
//! program's cursor in it and which groups are folded, the engine and what it last reported, the
//! sample pool and its analyses, this machine's settings, the Roto and the status line. `App` holds them beside the pages, so a page running a key can
//! borrow them mutably while it borrows itself, and an edit on a page applies an action as one in
//! `App` does.

pub(crate) mod analysing;
pub mod audio;
pub(crate) mod automating;
pub(crate) mod autosave;
pub mod banking;
pub mod config;
pub(crate) mod folding;
pub(crate) mod generators;
pub(crate) mod heard;
pub(crate) mod loading;
pub mod meter;
pub mod sampling;
pub(crate) mod scope;

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use vt_engine::engine::{PLAYHEADS, Playhead, SectionLoop, TrackPlace, Transport};
use vt_engine::gens::{GenPass, GenRow};
use vt_engine::link::{Command, Handle, JamState};
use vt_engine::published::Published;
use vt_engine::track_state::{Faders, LaneRead, LiveValues, NotePitch};
use vt_model::action::{Action, edit};
use vt_model::constants::{DEFAULT_NOTE_VELOCITY, FX_SLOTS, MAX_TRACKS, TICKS_PER_STEP};
use vt_model::history::History;
use vt_model::lane::Heard;
use vt_model::order::Dealer;
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
use vt_model::selection::{Extent, Field};
use vt_model::song::{ChainEntry, Phrase, Section, Song, Track};

use crate::app::{Cursor, Momentary, TrackLive};
use crate::input::mode::View;
use crate::services::analysing::Analyst;
use crate::services::config::Settings;
use crate::services::folding::{Folds, Order};
use crate::services::meter::Meter;

/// How many versions sent are kept for a report to be of. The callback takes each within a block,
/// so only a few are ever waiting; these are for when it has stopped.
const SENT_KEPT: usize = 64;

/// The program's shared services.
pub struct Services {
    pub history: History,
    /// Where the cursor is in the song, which every page reads and some move.
    pub cursor: Cursor,
    /// Which groups are folded, read from the song's `view` when it opens and written into it
    /// when it's saved, not undone (decisions 128 and 305).
    pub folds: Folds,
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
    /// Whether the key being handled is the terminal repeating one that's held, for the sample
    /// page's `h` and `l`, which speed up as they're held.
    pub(crate) repeating: bool,
    pub(crate) last_tick: Instant,
    pub transport: Transport,
    pub playhead: usize,
    /// How far through the playhead's step the engine last said it was, and when that was read.
    pub(crate) progress: f32,
    /// Where each track is heard, as its own steps count, which a track at another speed than the
    /// song's needs.
    pub places: [TrackPlace; MAX_TRACKS],
    pub(crate) report_at: Instant,
    /// The section the engine is on.
    pub playing_section: usize,
    /// The sections being kept playing, set with `:loop`, and by the song view's `Enter` and `C`.
    pub looping: Option<SectionLoop>,
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
    pub(crate) fader_wishes: Vec<crate::services::automating::Wish>,
    /// Where each voice playing a sample through the cursor's track is in it, for the sample
    /// page's playheads.
    pub playheads: [Option<Playhead>; PLAYHEADS],
    /// The main output's level.
    pub meter: Meter,
    /// The main output's recent sound, for the scope above the meter.
    pub scope: crate::services::scope::Scope,
    /// Each track's level, for the mixer.
    pub track_meters: Vec<Meter>,
    /// The scale, kept outside undo and written to the song on save as the live mutes and solos
    /// are, as it's for entering notes and isn't part of what's undone.
    pub scale: Scale,
    /// Whether entering in Insert moves on by the grid, likewise.
    pub auto_jump: bool,
    /// Whether the song is read-only, so `:w` to its own folder refuses (decision 313), likewise.
    pub read_only: bool,
    /// Whether fill is on, for `CND`'s `FILL` and `!FILL`. It's a live control, never saved.
    pub fill: bool,
    /// The octave the keys play and enter notes in, as note names number it: 3, where `a` is
    /// `C-3`, middle C.
    pub octave: i8,
    /// The velocity new notes are written with, like the octave.
    pub note_velocity: u8,
    /// Changes each time random values are wanted anew: a shape's seed, a roll, a bloom's.
    pub(crate) shape_seed: u64,
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
        let folds = Folds::from_view(&song.view);
        let (scale, auto_jump, read_only) = (song.scale, song.auto_jump, song.read_only);
        Self {
            history: History::new(Arc::new(song)),
            cursor: Cursor {
                section: 0,
                track: 0,
                slot: 0,
                step: 0,
                field: Field::Note,
            },
            folds,
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
            repeating: false,
            last_tick: Instant::now(),
            transport: Transport::Stopped,
            playhead: 0,
            progress: 0.0,
            places: [TrackPlace::default(); MAX_TRACKS],
            report_at: Instant::now(),
            playing_section: 0,
            looping: None,
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
            scope: crate::services::scope::Scope::new(rate),
            track_meters: vec![Meter::default(); MAX_TRACKS],
            scale,
            auto_jump,
            read_only,
            fill: false,
            octave: 3,
            note_velocity: DEFAULT_NOTE_VELOCITY,
            shape_seed: 0,
        }
    }

    /// The current song.
    pub fn song(&self) -> &Arc<Song> {
        self.history.current()
    }

    /// Run an action and make the result current. A fader it moves that the steps drive is
    /// taken over by the hand that moved it. One that changes nothing leaves no undo step, keeps
    /// the song as saved and isn't sent again (decision 343); the take-over still happens, as
    /// the hand moved the fader off what the steps had it at.
    pub fn apply(&mut self, action: &Action) {
        match edit(self.song(), action) {
            Ok(next) => {
                self.say_applied(&next);
                self.take_over_moved(action);
                if self.history.commit(next) {
                    self.dirty = true;
                    self.publish();
                }
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
    pub(crate) fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
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

    /// Which step of the cursor's phrase the playhead is on, if it's on this phrase.
    pub(crate) fn playhead_step(&self) -> Option<usize> {
        self.playhead_place().map(|(step, _)| step)
    }

    /// Which step of the cursor's phrase the playhead is on, and which of the step's pulses, if
    /// it's on this phrase.
    pub(crate) fn playhead_place(&self) -> Option<(usize, usize)> {
        let owner = self.track_here()?;
        let chain = (*self.section_here()?.chains.get(self.cursor.track)?)?;
        let playhead = self.playing_place(self.cursor.track, chain)?;
        let position = owner.chain_position(chain, playhead, self.dealer(self.cursor.track))?;
        let start = owner.phrases.get(&position.phrase)?.start();
        (position.slot == self.cursor.slot && Some(position.phrase) == self.phrase_id())
            .then_some((start + position.step, position.pulse))
    }

    /// The note the cursor's track's row playing was heard at, before its chain's transpose,
    /// where an accumulator or an `RND` moved it.
    pub(crate) fn heard_note(&self) -> Option<u8> {
        self.places.get(self.cursor.track)?.accumulated
    }

    /// Where each FX column of the cursor's phrase with a lane of its own is reading, if it's the
    /// one playing: the stored row, as the engine reports it read, so on a lane heard longer than
    /// its own, the row it's copying (decision 348).
    pub(crate) fn lane_playheads(&self, phrase: &Phrase) -> [Option<usize>; FX_SLOTS] {
        let mut playheads = [None; FX_SLOTS];
        if self.playhead_step().is_none() {
            return playheads;
        }
        let Some(place) = self.places.get(self.cursor.track) else {
            return playheads;
        };
        for (slot, playhead) in playheads.iter_mut().enumerate() {
            *playhead = phrase
                .lane(slot)
                .and(place.lanes[slot])
                .map(|read| read.row);
        }
        playheads
    }

    /// Where each FX column's lane of the phrase a track plays at the place it's at reads, worked out
    /// from its count at the track's `Length` and `Rate`, as the engine places a lane: for drawing
    /// without playing.
    pub(crate) fn placed_lanes(&self, track: usize) -> [Option<LaneRead>; FX_SLOTS] {
        let found = || {
            let owner = self.song().tracks.get(track)?;
            let chain = (*self.section_here()?.chains.get(track)?)?;
            let place = self.playing_place(track, chain)?;
            let position = owner.chain_position(chain, place, self.dealer(track))?;
            let phrase = owner.phrases.get(&position.phrase)?;
            let count = self.places.get(track)?.count - self.jam.get(track)?.started;
            Some(std::array::from_fn(|slot| {
                let lane = phrase.lane(slot)?;
                Some(LaneRead::placed(lane, lane.heard(owner.lanes[slot]), count))
            }))
        };
        found().unwrap_or([None; FX_SLOTS])
    }

    /// How each FX column's lane of the cursor's phrase is heard, for those with one: as the engine
    /// last read it while this phrase plays, or as the track's `Length` and `Rate` have it.
    pub(crate) fn heard_lanes(&self, phrase: &Phrase) -> [Option<Heard>; FX_SLOTS] {
        let playing = self.playhead_step().is_some();
        let place = self.places.get(self.cursor.track);
        let track = self.track_here();
        std::array::from_fn(|slot| {
            let lane = phrase.lane(slot)?;
            match place
                .and_then(|place| place.lanes[slot])
                .filter(|_| playing)
            {
                Some(read) => Some(read.heard),
                None => Some(lane.heard(track.map(|track| track.lanes[slot]).unwrap_or_default())),
            }
        })
    }

    /// A track's name, or its number if it has none.
    pub(crate) fn track_name(&self, track: usize) -> String {
        self.song()
            .tracks
            .get(track)
            .map_or_else(|| format!("{:02}", track + 1), |found| found.label(track))
    }

    /// The cursor's track as the latest report's live values were worked out from, which is a
    /// frame or so behind the song while it's being changed.
    pub(crate) fn reported_track(&self) -> Option<&Track> {
        let song = self.live_song.as_ref().unwrap_or_else(|| self.song());
        song.tracks.get(self.cursor.track)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub(crate) fn dealer(&self, track: usize) -> Dealer {
        Dealer::new(self.song().seed, track)
    }

    /// The section being edited.
    pub fn section_here(&self) -> Option<&Section> {
        let sections = &self.song().sections;
        sections.get(self.cursor.section).or(sections.last())
    }

    /// The step of its own that one track is at when another is at this one of its own, at their
    /// own speeds: the step playing then.
    pub(crate) fn step_on(&self, track: usize, from: usize, step: usize) -> usize {
        let ticks = |index: usize| {
            self.song()
                .tracks
                .get(index)
                .map_or(TICKS_PER_STEP, |track| track.speed.ticks())
        };
        (step as u64 * ticks(from) / ticks(track)) as usize
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&Track> {
        self.song().tracks.get(self.cursor.track)
    }

    /// The chain on the cursor's track.
    pub fn chain_id(&self) -> Option<u32> {
        let section = self.section_here()?;
        section.chains.get(self.cursor.track).copied().flatten()
    }

    /// The rows in the cursor's chain.
    pub fn chain_entries(&self) -> &[ChainEntry] {
        self.chain_id()
            .and_then(|id| self.track_here()?.chains.get(&id))
            .map_or(&[], |chain| chain.entries.as_slice())
    }

    /// The phrase under the cursor.
    pub fn phrase_id(&self) -> Option<u32> {
        let chain = (*self.section_here()?.chains.get(self.cursor.track)?)?;
        Some(
            self.track_here()?
                .chains
                .get(&chain)?
                .entries
                .get(self.cursor.slot)?
                .phrase,
        )
    }

    /// The contents of the phrase under the cursor.
    pub fn phrase_here(&self) -> Option<&Arc<Phrase>> {
        self.track_here()?.phrases.get(&self.phrase_id()?)
    }

    /// Where the cursor's phrase's loop sits.
    pub(crate) fn extent(&self) -> Extent {
        self.phrase_here()
            .map_or(Extent::whole(0), |phrase| phrase.extent())
    }

    /// Put the cursor on the first step its phrase plays.
    pub(crate) fn go_to_loop_start(&mut self) {
        self.cursor.step = self.extent().start;
    }

    /// Hear the row the cursor's track is on at a step of the section, with the row each other
    /// track plays then, unless playback is using the tracks. What it starts, which lasts while
    /// the key is down, if it sounds.
    pub(crate) fn audition(&mut self, section_step: usize) -> Option<Momentary> {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
            return None;
        }
        self.handle.send(Command::Audition {
            section: self.cursor.section,
            section_step,
            track: Some(self.cursor.track),
        });
        Some(Momentary::Audition)
    }

    /// The position in the section where the cursor's phrase starts playing.
    pub(crate) fn slot_start(&self) -> usize {
        let Some(track) = self.track_here() else {
            return 0;
        };
        let Some(chain) = self
            .section_here()
            .and_then(|s| s.chains.get(self.cursor.track).copied().flatten())
            .and_then(|id| track.chains.get(&id))
        else {
            return 0;
        };
        chain
            .entries
            .iter()
            .take(self.cursor.slot)
            .filter_map(|e| track.phrases.get(&e.phrase))
            .map(|p| p.time())
            .sum()
    }

    /// The cursor's position within the section, on a view. Only the loop has one, so a step
    /// outside it counts as the loop's first.
    pub fn section_step(&self, view: View) -> usize {
        if view == View::Song {
            return 0;
        }
        let extent = self.extent();
        // The chain view has no step of its own, so it plays from the row's first.
        let row = if view != View::Chain && extent.plays(self.cursor.step) {
            self.cursor.step - extent.start
        } else {
            0
        };
        // Rows above it last their pulses. In another order, the walk is wherever it is at that
        // step, worked out from the count as everything else is.
        let time = self.phrase_here().map_or(row, |phrase| phrase.time_to(row));
        self.slot_start() + time
    }

    /// Move the cursor along its phrase's steps, staying on one.
    pub(crate) fn move_step(&mut self, by: isize) {
        let Some(phrase) = self.phrase_here().cloned() else {
            return;
        };
        let last = phrase.steps().len() as isize - 1;
        self.cursor.step = (self.cursor.step as isize + by).clamp(0, last) as usize;
    }

    /// Keep the cursor on something that exists, as a view has it. Only the chain view has a row
    /// past the end.
    pub(crate) fn clamp_cursor(&mut self, view: View) {
        let sections = self.song().sections.len();
        self.cursor.section = self.cursor.section.min(sections.saturating_sub(1));
        let rows = self.chain_entries().len();
        let last = if view == View::Chain {
            rows
        } else {
            rows.saturating_sub(1)
        };
        self.cursor.slot = self.cursor.slot.min(last);
        self.move_step(0);
    }

    /// Move to the track as many along as `by`, stepping over those folded away, a folded group
    /// one stop on a view that has them (decisions 128 and 305).
    pub(crate) fn move_track(&mut self, by: isize, view: View) {
        let order = Order::of(view);
        let track = self.folds.step(self.song(), self.cursor.track, by, order);
        self.go_to_track_at(track, view);
    }

    /// Go to another track, staying at the same position in the section as a view has it.
    pub(crate) fn go_to_track_at(&mut self, track: usize, view: View) {
        let track = track.min(self.song().tracks.len() - 1);
        if track == self.cursor.track {
            return;
        }
        let section_step = self.section_step(view);
        let extent = self.extent();
        // A row outside the loop has no place in the section, so it keeps its number instead.
        let outside = (!extent.plays(self.cursor.step)).then(|| extent.row(self.cursor.step));
        let from = self.cursor.track;
        self.cursor.track = track;
        let song = Arc::clone(self.song());
        let position = song
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.chains.get(track).copied().flatten())
            .and_then(|chain| {
                let place = self.step_on(track, from, section_step);
                song.tracks[track].chain_position(chain, place, self.dealer(track))
            });
        match position {
            Some(position) => {
                self.cursor.slot = position.slot;
                let row = outside.unwrap_or(position.step as isize);
                self.cursor.step = self.extent().step_of(row);
            }
            None => self.cursor.slot = 0,
        }
    }

    /// The cursor's track, or a fresh one if there's none, as the track page edits it.
    pub(crate) fn page_track(&self) -> Track {
        self.song()
            .tracks
            .get(self.cursor.track)
            .cloned()
            .unwrap_or_default()
    }

    /// The cursor's track as it's heard, with its faders where the steps have them while they
    /// drive them.
    pub(crate) fn page_track_as_heard(&self) -> Track {
        self.track_as_heard(self.cursor.track)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default()
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
