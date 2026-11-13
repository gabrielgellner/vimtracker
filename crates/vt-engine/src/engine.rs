//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::constants::{
    MAX_GROUPS, MAX_MILLIBPM, MAX_TRACKS, MIN_MILLIBPM, TICKS_PER_QUARTER, TICKS_PER_STEP,
};
use vt_model::scale::Scale;
use vt_model::song::{Song, trim_gain};

use crate::dsp::memory::MemoryPool;
use crate::dsp::smoother::{coefficient, fast_coefficient};
use crate::gens::{GenHeard, GenScratch};
use crate::mixer::Reading;
use crate::preview::{Files, Previews};
use crate::published::Published;
use crate::timing::Stopwatch;
use crate::track_state::{Keys, MasterRest, SongTime, TrackState};

mod block;
mod controls;
mod modulation;
mod previews;
mod scheduling;
mod steps;
mod transport;
mod versions;

/// Where a track is heard, as its own steps count.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TrackPlace {
    /// The step of its running count heard.
    pub count: i64,
    /// How far through that step the transport is, from 0 to 1.
    pub progress: f32,
    /// How long that step is, in ticks.
    pub ticks: u32,
    /// Whether the row heard skipped its note, as its `CHA` or `CND` decided.
    pub skipped: bool,
    /// The note the row heard was played at, before its chain's transpose, when an accumulator
    /// moved it from the note as written.
    pub accumulated: Option<u8>,
}

/// How many voices of one track the sample page can show a playhead for: the track's note and
/// its tail, and two of the preview voices playing through it.
pub const PLAYHEADS: usize = 4;

/// Where a voice playing a sample through a track is in it, for the sample page's playhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Playhead {
    /// The bank entry it plays, or none for a one-off file, as the sample page plays.
    pub entry: Option<u8>,
    /// The frame it's at, in the pool's frames, counted from the first whichever way it plays.
    pub frame: u64,
}

/// Sections kept playing in turn, `:loop`'s, rather than the song moving on: from the first to the
/// last, in order, and back to the first. One section loops on itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionLoop {
    pub first: usize,
    pub last: usize,
}

impl SectionLoop {
    /// The sections from one to the other, whichever way round they're given.
    pub fn new(one: usize, other: usize) -> Self {
        Self {
            first: one.min(other),
            last: one.max(other),
        }
    }

    /// One section, looping on itself.
    pub fn one(section: usize) -> Self {
        Self::new(section, section)
    }

    /// Whether a section is one of these.
    pub fn holds(self, section: usize) -> bool {
        (self.first..=self.last).contains(&section)
    }

    /// The section after this one, and whether that went back round to the first: the next of
    /// these with a length, or from outside them, the first with one. If none has, the first.
    fn next(self, song: &Song, from: usize) -> (usize, bool) {
        let last = self.last.min(song.sections.len().saturating_sub(1));
        let first = self.first.min(last);
        let long = |at: &usize| song.section_length(&song.sections[*at]) > 0;
        if (first..=last).contains(&from) {
            if let Some(at) = (from + 1..=last).find(long) {
                return (at, false);
            }
            if let Some(at) = (first..=from).find(long) {
                return (at, at != from);
            }
        } else if let Some(at) = (first..=last).find(long) {
            return (at, false);
        }
        (first, false)
    }
}

/// Whether the sequencer is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

/// The sample an event falls on, counted from the tick where the current tempo began.
///
/// Always calculated from that origin and never accumulated, so rounding can't build up.
pub fn sample_of_tick(tick_from_origin: u64, rate: u32, millibpm: u32) -> u64 {
    let numerator = u128::from(tick_from_origin) * 60 * u128::from(rate) * 1000;
    let denominator = u128::from(TICKS_PER_QUARTER) * u128::from(millibpm);
    (numerator / denominator) as u64
}

/// Sequencer and voices. It owns all its state ahead of time, and `process` doesn't allocate.
pub struct Engine {
    rate: u32,
    current: Arc<Published>,
    tracks: Box<[TrackState; MAX_TRACKS]>,
    /// How many track states, from the first, the song's versions have had tracks in since tracks
    /// were last reset: as tracks come and go, the fixed tracks' states move with them, but never
    /// past the most there have been. The rest have the settings that reset gave them.
    used: usize,
    /// The voices the keyboard plays through, each tied to the note its key started.
    previews: Previews,
    /// The tracks the last audition started a note on, for letting go of them when its key is.
    auditioned: [bool; MAX_TRACKS],
    /// One-off files the preview voices play, such as the one the sample browser's cursor is on,
    /// and the place the newest went in.
    files: Files,
    file_place: usize,
    /// Each track's sound after its slots, for compressors and Warps that hear another track.
    keys: Keys,
    /// Which tracks a compressor or Warps somewhere hears, which keep playing while muted so it
    /// still does.
    listened: [bool; MAX_TRACKS],
    transport: Transport,
    /// The section playing, and how many steps of it have fired.
    section: usize,
    section_place: u64,
    /// Sections to keep playing rather than moving on.
    looping: Option<SectionLoop>,
    /// Sections to loop from the next line of the launch grid, as `:loop` asks while playing.
    loop_cued: Option<SectionLoop>,
    /// The next step to fire, for timing. It never wraps.
    next_step: u64,
    /// Where the next step falls for the tracks, which each loop at their own length. Separate from
    /// `next_step` so a reset moves the tracks but not the beat.
    place: u64,
    /// Where the step before the last one worked out fell, which is still the one heard while a
    /// groove's lead has worked the next out early.
    heard_place: u64,
    /// The tick the tempo began at.
    origin_tick: u64,
    /// Frames played since `origin_tick`.
    frames_since_origin: u64,
    /// Frames played in total, for events on a track's own schedule.
    clock: u64,
    millibpm: u32,
    smoothing: f32,
    fast_smoothing: f32,
    /// The long buffers no slot is using.
    memory: MemoryPool,
    /// Frames rendered in all, playing or not, for what moves on its own.
    rendered: u64,
    /// The master's levels since the last report.
    reading: Reading,
    /// Frames until LFOs are next worked out.
    lfo_countdown: u32,
    /// Whether fill is on, which `CND`'s `FILL` and `!FILL` read.
    fill: bool,
    /// The scale an `ACU` moves notes through, a live control as it's outside undo.
    scale: Scale,
    /// Whether every track goes back to its start at the next line of the realign grid, as a
    /// key asks while playing.
    realign_waiting: bool,
    /// Where its time goes, while `vimtracker bench --tracks` asks.
    stopwatch: Stopwatch,
    /// Whether a track at rest is left as it is rather than worked out again.
    resting: bool,
    /// Which buses the last run left out, for tests.
    rested: RestingBuses,
    /// What moving generators render into, taken from the version playing.
    gens: GenScratch,
}

impl Engine {
    /// A stopped engine at the top of the section.
    pub fn new(rate: u32, current: Arc<Published>) -> Self {
        let millibpm = current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        let seed = current.song.seed;
        let scale = current.song.scale;
        let memory = MemoryPool::for_song(&current.song, rate);
        let mut engine = Self {
            rate,
            current,
            // Built on the heap a track at a time: each holds two voices with room for any synth,
            // so the whole array is too big to build on the stack first.
            tracks: (0..MAX_TRACKS)
                .map(|index| TrackState::new(seed, index))
                .collect::<Box<[TrackState]>>()
                .try_into()
                .unwrap_or_else(|_| unreachable!("one state for each track")),
            // None has been reset yet.
            used: MAX_TRACKS,
            previews: Previews::default(),
            auditioned: [false; MAX_TRACKS],
            files: [None, None],
            file_place: 0,
            keys: Keys::default(),
            listened: [false; MAX_TRACKS],
            transport: Transport::Stopped,
            section: 0,
            section_place: 0,
            looping: None,
            loop_cued: None,
            next_step: 0,
            place: 0,
            heard_place: 0,
            origin_tick: 0,
            frames_since_origin: 0,
            clock: 0,
            millibpm,
            smoothing: coefficient(rate),
            fast_smoothing: fast_coefficient(rate),
            memory,
            rendered: 0,
            reading: Reading::default(),
            lfo_countdown: 0,
            fill: false,
            scale,
            realign_waiting: false,
            stopwatch: Stopwatch::default(),
            resting: true,
            rested: RestingBuses::default(),
            gens: GenScratch::default(),
        };
        engine.current.scratch.exchange(&mut engine.gens);
        engine.follow_levels();
        engine.reset_tracks();
        engine.find_listened();
        for track in engine.tracks.iter_mut() {
            track.settle();
        }
        engine
    }

    /// Move each track's trim toward the current version's. Its level is `LEV`'s base, which
    /// follows the version as every setting does.
    fn follow_levels(&mut self) {
        for (state, track) in self.tracks.iter_mut().zip(&self.current.song.tracks) {
            state.input_trim.target = trim_gain(track.trim);
        }
    }

    /// The output sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The transport state.
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// The current version.
    pub fn current(&self) -> &Arc<Published> {
        &self.current
    }

    /// Hold a setting of the generators on a track's column at a number, as heard, or give it back
    /// to their shapes with none: each of them on that column, on whichever of the track's phrases,
    /// is rendered again from its shape with it set, within its bounds, and heard so from the next
    /// row read. A list setting, such as the fill or the `Where`, is left to the shape. It's what
    /// a macro holding a generator's setting will move (#623), and it outlasts versions.
    pub fn hold_generator(
        &mut self,
        track: usize,
        column: vt_model::shape::ShapeColumn,
        param: vt_model::shape::GenParam,
        number: Option<i32>,
    ) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.gen_holds.hold(column, param, number);
            if !state.gen_holds.any() {
                self.gens.rest(track);
            }
        }
    }

    /// A track's live generators, on the phrase it last read a row of while one was held: which
    /// pass each was read on, how often it has rendered, and whether it's moving.
    pub fn generators_heard(&self, track: usize) -> impl Iterator<Item = GenHeard> + '_ {
        self.gens.heard(track)
    }

    /// The place last fired, or the next to fire if none has yet.
    pub fn position(&self) -> usize {
        if !self.has_steps() {
            return 0;
        }
        let fired = self.transport == Transport::Playing && self.frames_since_origin > 0;
        let place = if fired && self.worked_out_early(false) {
            self.heard_place
        } else {
            self.place.saturating_sub(u64::from(fired))
        };
        if self.past_reset(place) {
            0
        } else {
            place as usize
        }
    }

    /// How far the transport is from the place last fired to the next, from 0 to 1, so the edit
    /// thread can tell where in a step a key went down. It's 0 until a step has fired.
    pub fn step_progress(&self) -> f32 {
        let fired = self.transport == Transport::Playing && self.frames_since_origin > 0;
        if !self.has_steps() || !fired || self.next_step == 0 {
            return 0.0;
        }
        let back = if self.worked_out_early(false) { 2 } else { 1 };
        let last = self.next_step.saturating_sub(back);
        let progress =
            (self.song_time().tick - (last * TICKS_PER_STEP) as f64) / TICKS_PER_STEP as f64;
        progress.clamp(0.0, 1.0) as f32
    }

    /// Where a track is heard: the step of its own running count last on the grid, how far
    /// through it the transport is, and how long it is, which the playhead and recording follow.
    /// Before a step has played, it's the next to.
    pub fn track_heard(&self, index: usize) -> TrackPlace {
        let Some(state) = self.tracks.get(index) else {
            return TrackPlace::default();
        };
        let ticks = self.speed_ticks(index);
        let waiting = TrackPlace {
            count: state.clock.count,
            progress: 0.0,
            ticks: ticks as u32,
            skipped: false,
            accumulated: None,
        };
        let moved = self.transport != Transport::Stopped && self.frames_since_origin > 0;
        if !moved {
            return waiting;
        }
        let now = self.song_time().tick;
        state.heard_at(now).map_or(waiting, |worked| TrackPlace {
            count: worked.count,
            progress: ((now - worked.grid as f64) / worked.ticks.max(1) as f64).clamp(0.0, 1.0)
                as f32,
            ticks: worked.ticks as u32,
            skipped: !state.row.note,
            accumulated: state.accumulated,
        })
    }

    /// Whether the last step worked out has its place on the grid still to come, or just here
    /// if `or_now`, as a groove that lands steps early has them worked out before their place.
    fn worked_out_early(&self, or_now: bool) -> bool {
        if self.current.lead == 0 || self.next_step == 0 {
            return false;
        }
        let place = ((self.next_step - 1) * TICKS_PER_STEP).saturating_sub(self.origin_tick);
        let at = sample_of_tick(place, self.rate, self.millibpm);
        self.frames_since_origin < at || or_now && self.frames_since_origin == at
    }

    /// Whether a place is one the section's reset would already have sent back to the start.
    fn past_reset(&self, place: u64) -> bool {
        let reset = self.playing_section().and_then(|s| s.reset);
        reset.is_some_and(|reset| place >= reset as u64)
    }

    /// Where the song is now, worked out from the tempo's origin so rounding can't build up.
    fn song_time(&self) -> SongTime {
        let ticks_per_frame = TICKS_PER_QUARTER as f64 * f64::from(self.millibpm)
            / (60.0 * f64::from(self.rate) * 1000.0);
        SongTime {
            scale: self.current.song.scale,
            tick: self.origin_tick as f64 + self.frames_since_origin as f64 * ticks_per_frame,
            clock: self.clock,
            rate: self.rate,
            seed: self.current.song.seed,
            frames_per_tick: 1.0 / ticks_per_frame,
            rendered: self.rendered,
        }
    }
}

/// The groups, returns and master that rest through a run.
#[derive(Debug, Clone, Copy, Default)]
struct RestingBuses {
    groups: [bool; MAX_GROUPS],
    returns: [bool; 2],
    master: Option<MasterRest>,
}
