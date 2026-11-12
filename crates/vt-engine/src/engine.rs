//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::accumulator::{Adds, MOST_DEGREES, Reset, amount, moved_note};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::condition::{Condition, Situation};
use vt_model::constants::{
    BAR_STEPS, DEFAULT_NOTE_VELOCITY, FIXED_TRACKS, FX_SLOTS, GROOVE_UNIT_TICKS, MAX_GROUPS,
    MAX_MILLIBPM, MAX_SOUND_TRACKS, MAX_TRACKS, MIN_MILLIBPM, NUDGE_TICKS, PREVIEW_MS,
    TICKS_PER_QUARTER, TICKS_PER_STEP,
};
use vt_model::envelope::{Adsr, Envelope};
use vt_model::groove::{FULL_VELOCITY, Groove, GrooveId, MAX_PUSH};
use vt_model::insert::Insert;
use vt_model::modulation::{ModSlot, note_shape_slot};
use vt_model::order::Dealer;
use vt_model::sample::SampleData;
use vt_model::scale::Scale;
use vt_model::settings::{PlayMode, SettingId};
use vt_model::song::{
    ChainPosition, Launch, Machine, Section, SectionStart, Song, SynthVoice, Track, trim_gain,
};
use vt_model::speed::Speed;
use vt_model::step::{
    ACCENT_VELOCITY, FxCommand, FxSlot, GateKind, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Mark, Note,
};

use crate::dsp::memory::{Memory, MemoryCount, MemoryPool};
use crate::dsp::smoother::{coefficient, fast_coefficient};
use crate::link::JamState;
use crate::mixer::{Reading, master};
use crate::preview::{FILE_SAMPLES, Files, LetGo, Previews};
use crate::published::{Bank, Published, Span};
use crate::timing::{Part as TimedPart, PartTimes, SlotTimes, Stopwatch};
use crate::track_state::{
    Accumulating, Arpeggiating, Clock, Decided, DelayedNote, Epoch, Faders, Keys, LiveValues, Part,
    PendingStep, Retrigger, RowKey, SongTime, TrackState, Worked, base_of,
};
use crate::voice::{Cut, NoteOn, Voice};
use vt_model::samples::SampleId;

/// How often LFOs are worked out, in frames. Their values are smoothed in between.
const LFO_UPDATE_FRAMES: u32 = 32;

/// The velocity every `Acid` note plays at, whatever its own: 99, where decision 117 fitted its
/// level, so a note there sounds as it did before velocity was taken away from it (decision 121).
/// An accented note plays at it too, as all an accent adds to the level is the voice's own accent
/// circuit's (decision 127).
const ACID_VELOCITY: u8 = 99;

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
}

/// The next row with a length after this one, in order, and whether that went past the last. If no
/// other has one, it's this row.
fn next_row(song: &Song, from: usize) -> (usize, bool) {
    let count = song.sections.len();
    let mut wrapped = false;
    for ahead in 1..=count {
        let at = (from + ahead) % count;
        wrapped |= at == 0;
        if song.section_length(&song.sections[at]) > 0 {
            return (at, wrapped);
        }
    }
    (from, false)
}

/// Where the running count is after some steps from a place, under a section's `:reset`, and
/// whether a reset came within them. A count at or past the reset goes back on its next step.
fn after_steps(place: u64, steps: u64, reset: Option<usize>) -> (u64, bool) {
    let Some(reset) = reset.map(|reset| reset.max(1) as u64) else {
        return (place + steps, false);
    };
    let until = reset.saturating_sub(place);
    if until >= steps {
        return (place + steps, false);
    }
    (((steps - until - 1) % reset) + 1, true)
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
        };
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

    /// Send every track back to its start from the next step, leaving the beat where it is.
    pub fn realign(&mut self) {
        self.realign_at(self.next_step * TICKS_PER_STEP);
    }

    /// Send every track back to its start at the next line of the realign grid, or take back a
    /// realign waiting for it. Only while playing, as playing starts every track at its start.
    pub fn toggle_realign(&mut self) {
        self.realign_waiting = self.transport == Transport::Playing && !self.realign_waiting;
    }

    /// Whether a realign is waiting for its line of the grid.
    pub fn realign_waiting(&self) -> bool {
        self.realign_waiting
    }

    /// Start one track's chain again at the next line of the realign grid, or take back one
    /// waiting for it. Only while playing, as an every-track realign is.
    pub fn toggle_realign_track(&mut self, index: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(track) = self.tracks.get_mut(index) {
            track.realign_waiting = playing && !track.realign_waiting;
        }
    }

    /// Whether a track's own realign is waiting for its line of the grid.
    pub fn track_realign_waiting(&self, index: usize) -> bool {
        self.tracks
            .get(index)
            .is_some_and(|track| track.realign_waiting)
    }

    /// How far apart a track's own resets are on the grid, in ticks, if it has them.
    fn reset_ticks(&self, index: usize) -> Option<u64> {
        let steps = self.current.song.tracks.get(index)?.reset?;
        Some(u64::from(steps.max(1)) * TICKS_PER_STEP)
    }

    /// Whether a track's own reset falls on the song's step at this place on the grid: the first
    /// of the song's steps at or after its reset's length from where its chain last started.
    fn reset_due(&self, index: usize, grid: u64) -> bool {
        let Some(every) = self.reset_ticks(index) else {
            return false;
        };
        let (every, step) = (every as i64, TICKS_PER_STEP as i64);
        let since = grid as i64 - self.tracks[index].clock.anchor;
        since >= every && since.div_euclid(every) > (since - step).div_euclid(every)
    }

    /// Start each track's chain again at the last of its own resets before this place on the
    /// grid, as playing up to it would have, for working out where playing from a place starts.
    /// Each track's clock is no more than a step of its own past the last place this was asked.
    fn reset_tracks_before(&mut self, at: u64) {
        for index in 0..MAX_TRACKS {
            let Some(every) = self.reset_ticks(index) else {
                continue;
            };
            let anchor = self.tracks[index].clock.anchor;
            let first = ((anchor + every as i64).max(0) as u64).next_multiple_of(TICKS_PER_STEP);
            if first >= at {
                continue;
            }
            let last = first + (at - 1 - first) / every * every;
            let ticks = self.speed_ticks(index);
            let track = &mut self.tracks[index];
            track.clock.skip_to(last, ticks);
            track.restart_at(last);
        }
    }

    /// How many steps apart the lines are that a realign waits for: the launch grid's, or a bar's
    /// when launches wait for a chain's end, as every track's chain ends at a different place.
    fn realign_grid(&self) -> u64 {
        match self.current.song.launch {
            Launch::Every(steps) => u64::from(steps.get()),
            Launch::ChainEnd => BAR_STEPS as u64,
        }
    }

    /// Whether the song's step about to fire is a line of a grid this many steps apart. Each
    /// repeat of a section starts the grid again, so a section shorter than the grid has a line
    /// only at its start.
    fn on_grid_line(&self, steps: u64) -> bool {
        let length = self
            .playing_section()
            .map_or(0, |s| self.current.song.section_length(s));
        (self.section_place % (length as u64).max(1)).is_multiple_of(steps.max(1))
    }

    /// Send every track back to its start at this place on the grid, counting its steps again
    /// from there.
    fn realign_at(&mut self, at: u64) {
        self.place = 0;
        for track in self.tracks.iter_mut() {
            track.clock.count = 0;
            track.restart_at(at);
        }
    }

    /// Bring a section in at this place on the grid: a track given a different chain starts it
    /// from its top there, lining its steps up with the section's start.
    fn start_row(&mut self, section: &Section, at: u64) {
        for (track, chain) in self.tracks.iter_mut().zip(&section.chains) {
            if track.playing != *chain {
                track.playing = *chain;
                track.restart_at(at);
            }
        }
    }

    /// How long a track's steps are at its own speed, in ticks.
    fn speed_ticks(&self, index: usize) -> u64 {
        self.current
            .song
            .tracks
            .get(index)
            .map_or(TICKS_PER_STEP, |track| track.speed.ticks())
    }

    /// Count every track on to the first of its steps at or after this place on the grid, as if
    /// it had played at its own speed.
    fn skip_tracks_to(&mut self, at: u64) {
        for index in 0..MAX_TRACKS {
            let ticks = self.speed_ticks(index);
            self.tracks[index].clock.skip_to(at, ticks);
        }
    }

    /// Work out the running count and each track's start from the rows before this one, as if the
    /// song had played to this step of it, and give the step within the section.
    ///
    /// Each track's steps are walked at its own speed on a grid counted from the walk's start, and
    /// moved to where playing starts once that's known. A track partway through a longer step
    /// there comes in on its next.
    fn walk_to(&mut self, section: usize, step: usize) -> u64 {
        self.place = 0;
        for track in self.tracks.iter_mut() {
            track.playing = None;
            track.clock = Clock::default();
            track.restart(0);
            track.forget_worked(0);
            track.forget_conditions();
            track.forget_accumulators();
        }
        let song = Arc::clone(&self.current.song);
        let Some(rows) = song.sections.get(..=section) else {
            return 0;
        };
        // A row that resets makes what came before it not matter.
        let first = rows
            .iter()
            .rposition(|row| row.start == SectionStart::Reset)
            .unwrap_or(0);
        let mut within = 0;
        let mut tick = 0;
        for (at, row) in rows.iter().enumerate().skip(first) {
            let length = song.section_length(row) as u64;
            if length == 0 {
                continue;
            }
            self.reset_tracks_before(tick);
            self.skip_tracks_to(tick);
            self.start_row(row, tick);
            let plays = song.section_plays(row) as u64;
            let mut steps = if at == section {
                within = step as u64 % plays;
                within
            } else {
                plays
            };
            // A resetting section's earlier repeats leave nothing behind.
            if row.start == SectionStart::Reset && steps > length {
                steps = (steps - 1) % length + 1;
            }
            let (place, reset) = after_steps(self.place, steps, row.reset);
            if reset {
                // The last reset was as many steps back as the count has come since.
                let reset_at = tick + (steps - place) * TICKS_PER_STEP;
                self.skip_tracks_to(reset_at);
                self.realign_at(reset_at);
            }
            self.place = place;
            tick += steps * TICKS_PER_STEP;
        }
        self.reset_tracks_before(tick);
        self.skip_tracks_to(tick);
        let by = (within * TICKS_PER_STEP) as i64 - tick as i64;
        for track in self.tracks.iter_mut() {
            track.clock.shift(by);
        }
        within
    }

    /// Put a track's chain at this step of its own, starting it there at the next step, leaving
    /// the others where they are.
    pub fn place_track(&mut self, track: usize, step: usize) {
        let at = self.next_step * TICKS_PER_STEP;
        let ticks = self.speed_ticks(track) as i64;
        if let Some(state) = self.tracks.get_mut(track) {
            state.clock.line_up(at);
            state.restart(state.clock.count - step as i64);
            // Its own reset counts from where its chain would have started, on the same grid.
            state.clock.anchor -= step as i64 * ticks;
        }
    }

    /// Start at a step within a section, or at a step of a track's own with that track's chain
    /// there: the song starts at the step of its own that the track's falls in.
    pub fn play_section_at(&mut self, section: usize, step: usize, track: Option<usize>) {
        let Some(track) = track else {
            self.play_section(section, step);
            return;
        };
        let song_step = step as u64 * self.speed_ticks(track) / TICKS_PER_STEP;
        self.play_section(section, song_step as usize);
        self.place_track(track, step);
    }

    /// Play a chain on one track when its launch timing allows, or stop it then.
    pub fn launch(&mut self, track: usize, chain: Option<u32>) {
        if let Some(track) = self.tracks.get_mut(track) {
            track.cued = Some(chain);
        }
    }

    /// Carry on in another section as if nothing changed, with launched chains still running. It
    /// has this length, so the place in it is the place in the last section's current repeat.
    pub fn continue_in(&mut self, section: usize, length: usize) {
        self.section = section;
        self.section_place %= (length as u64).max(1);
        self.looping = Some(SectionLoop::one(section));
        self.loop_cued = None;
    }

    /// What a track is launched onto and cued to, while jamming.
    pub fn jam(&self, track: usize) -> JamState {
        self.tracks
            .get(track)
            .map_or_else(JamState::default, |track| JamState {
                playing: track.launched,
                cued: track.cued,
                chain: track.playing,
                started: track.started,
                lane_offsets: track.lane_offsets,
                mute: track.mute,
                solo: track.solo,
                mute_waiting: track.mute_waiting,
                solo_waiting: track.solo_waiting,
                switches_landed: track.switches_landed,
            })
    }

    /// Back to every track playing its section's chain, forgetting what's cued.
    fn clear_launches(&mut self) {
        self.end_launches();
        for track in self.tracks.iter_mut() {
            track.cued = None;
        }
    }

    /// Back to every track playing its section's chain, keeping what's cued.
    fn end_launches(&mut self) {
        for track in self.tracks.iter_mut() {
            track.launched = None;
        }
    }

    /// The chain a track is on: one launched, or its section's.
    fn track_chain(&self, index: usize) -> Option<u32> {
        if let Some(launched) = self.tracks[index].launched {
            return launched;
        }
        let section = self.current.song.sections.get(self.section)?;
        *section.chains.get(index)?
    }

    /// The chain a track is on, and how far into it a step of its running count falls, counting
    /// every time round it since it started, as which time round deals a dealt order. A step
    /// before it started, as playing from a step can count, is in its first time round.
    fn track_place(&self, index: usize, count: i64) -> Option<(u32, usize)> {
        let chain = self.track_chain(index)?;
        let length = (self.current.song.tracks.get(index)?.chain_steps(chain) as i64).max(1);
        let place = count - self.tracks[index].started;
        let laps = place.div_euclid(length).max(0);
        Some((chain, (laps * length + place.rem_euclid(length)) as usize))
    }

    /// What a track's dealt orders are dealt from.
    fn dealer(&self, index: usize) -> Dealer {
        Dealer::new(self.current.song.seed, index)
    }

    /// Start what a track has cued, if this place on the grid is one its launch timing starts one
    /// on: one of the song's steps for a launch on the bar's grid, which a track partway through
    /// a longer step starts on there, or the track's own step at its chain's end.
    fn start_cued(&mut self, index: usize, at: u64, song_step: bool) {
        if self.tracks[index].cued.is_none() {
            return;
        }
        let due = match self.current.song.launch {
            Launch::Every(steps) => song_step && self.on_grid_line(u64::from(steps.get())),
            Launch::ChainEnd => {
                let state = &self.tracks[index];
                state.clock.beat == at
                    && self
                        .track_place(index, state.clock.count)
                        .is_none_or(|(current, place)| {
                            let length = self.current.song.tracks[index].chain_steps(current);
                            length == 0 || place.is_multiple_of(length)
                        })
            }
        };
        if due {
            self.begin_cued(index, at);
        }
    }

    /// Start what a track has cued now, at this place on the grid, if it has anything cued.
    fn begin_cued(&mut self, index: usize, at: u64) {
        let Some(chain) = self.tracks[index].cued.take() else {
            return;
        };
        if chain.is_none() {
            self.tracks[index].stop();
        }
        self.tracks[index].launched = Some(chain);
        self.tracks[index].restart_at(at);
    }

    /// Whether any section has a length, so there's a next step to fire. One with a length and no
    /// chains is a rest, and one with neither is skipped.
    fn has_steps(&self) -> bool {
        let song = &self.current.song;
        song.sections.iter().any(|s| song.section_length(s) > 0)
    }

    /// The section playing. Removing one while playing moves to whichever takes its place.
    fn playing_section(&self) -> Option<&Section> {
        let sections = &self.current.song.sections;
        sections.get(self.section).or(sections.last())
    }

    /// The section playing, by position.
    pub fn section(&self) -> usize {
        self.section
            .min(self.current.song.sections.len().saturating_sub(1))
    }

    /// Keep playing this section rather than moving on, from now, or go back to playing the song.
    pub fn set_loop_section(&mut self, section: Option<usize>) {
        self.looping = section.map(SectionLoop::one);
        self.loop_cued = None;
    }

    /// Keep playing these sections in turn rather than moving on, or go back to playing the song.
    /// While playing a section that isn't one of them, a loop waits for the next line of the
    /// launch grid, where the first of them starts; a section ending is a line too. Otherwise it's
    /// at once, as nothing moves until the section playing ends, which is a line anyway, and so is
    /// going back to the song, as the section playing then plays to its end and moves on.
    pub fn loop_sections(&mut self, sections: Option<SectionLoop>) {
        self.loop_cued = None;
        let playing = self.transport == Transport::Playing;
        match sections {
            Some(sections) if playing && !sections.holds(self.section()) => {
                self.loop_cued = Some(sections);
            }
            _ => self.looping = sections,
        }
    }

    /// Whether a loop is waiting for its line of the grid.
    pub fn loop_waiting(&self) -> bool {
        self.loop_cued.is_some()
    }

    /// The sections being kept playing, if any.
    pub fn looping(&self) -> Option<SectionLoop> {
        self.looping
    }

    /// Go back to the first section's start without touching anything sounding, for a render. A
    /// step a groove's lead worked out early is worked out again from the top.
    pub fn rewind(&mut self) {
        if self.worked_out_early(true) {
            self.next_step -= 1;
            let from = self.next_step * TICKS_PER_STEP;
            for track in self.tracks.iter_mut() {
                track.forget_steps(from);
            }
        }
        self.section = 0;
        self.section_place = 0;
        self.realign();
        self.clear_launches();
    }

    /// Move to the next section if this one has played out. False if nothing has any length.
    fn enter_section_if_due(&mut self) -> bool {
        let song = Arc::clone(&self.current.song);
        let count = song.sections.len();
        if count == 0 {
            return false;
        }
        self.section = self.section.min(count - 1);
        let here = &song.sections[self.section];
        if self.section_place < song.section_plays(here) as u64 {
            // A loop cued lands on the grid's next line, and its first section starts there if
            // the one playing isn't one of its.
            if self.loop_cued.is_some() && self.on_grid_line(self.realign_grid()) {
                let sections = self.loop_cued.take().unwrap_or_else(|| unreachable!());
                self.looping = Some(sections);
                if !sections.holds(self.section) {
                    let (from, first) = (self.section, sections.first.min(count - 1));
                    return self.enter_section(&song, first, from, false);
                }
            }
            // Each repeat of a resetting section starts it over.
            let length = song.section_length(here) as u64;
            if here.start == SectionStart::Reset
                && self.section_place > 0
                && self.section_place.is_multiple_of(length.max(1))
            {
                self.realign();
            }
            return true;
        }
        // A section's start is a line of the grid, so a loop cued lands here.
        if let Some(sections) = self.loop_cued.take() {
            self.looping = Some(sections);
        }
        let from = self.section;
        let (next, wrapped) = match self.looping {
            Some(sections) if sections.first < count => sections.next(&song, from),
            _ => next_row(&song, from),
        };
        self.enter_section(&song, next, from, wrapped)
    }

    /// Start a section, coming from another or the same one, and going back round to the first if
    /// `wrapped`: it runs its `Start`, ends launches if it's another, and starts what's cued. False
    /// if it has no length.
    fn enter_section(&mut self, song: &Song, next: usize, from: usize, wrapped: bool) -> bool {
        self.section = next;
        self.section_place = 0;
        let section = &song.sections[next];
        if song.section_length(section) == 0 {
            return false;
        }
        if next != from {
            self.end_launches();
        }
        let at = self.next_step * TICKS_PER_STEP;
        if section.start == SectionStart::Reset {
            self.realign();
        } else if next != from && wrapped {
            for track in self.tracks.iter_mut() {
                track.restart_at(at);
            }
        } else if next != from {
            self.start_row(section, at);
        }
        // A cue left waiting isn't lost to the song moving on.
        if next != from {
            for index in 0..self.tracks.len() {
                self.begin_cued(index, at);
            }
        }
        true
    }

    /// Keep a long buffer the edit thread has sent, for a version it's about to send.
    pub fn receive_memory(&mut self, memory: Memory) {
        self.memory.put(memory);
    }

    /// The edit thread wants this many back, as they're more than its versions need.
    pub fn release_memory(&mut self, count: MemoryCount) {
        self.memory.owe(count);
    }

    /// The next long buffer to send back to be freed.
    pub fn next_freed_memory(&mut self) -> Option<Memory> {
        self.memory.next_owed()
    }

    /// How many long buffers are held that no slot is using.
    pub fn spare_memory(&self) -> usize {
        self.memory.spare()
    }

    /// The id of the version playing, [`Published::id`].
    pub fn version(&self) -> u64 {
        self.current.id
    }

    /// Switch to a new version, returning the old one so it can be freed elsewhere.
    ///
    /// A track whose chain changed length stays on the row it was going to play next. An ID that
    /// no longer exists is silent, and a removed track's voices fade.
    pub fn swap(&mut self, version: Arc<Published>) -> Arc<Published> {
        let old = std::mem::replace(&mut self.current, version);
        self.used = self
            .used
            .max(self.current.song.tracks.len().min(MAX_TRACKS));
        self.keep_fixed_tracks(old.song.sound_tracks());
        self.line_up_new_tracks(old.song.sound_tracks());
        self.keep_places(&old.song);
        self.follow_base(&old);
        for track in self.tracks.iter_mut().skip(self.current.song.tracks.len()) {
            track.stop();
            track.launched = None;
            track.cued = None;
            track.mute = false;
            track.solo = false;
            track.mute_waiting = false;
            track.solo_waiting = false;
            track.give_back_all();
            track.give_back_memory(&mut self.memory);
        }
        self.follow_levels();
        self.retune();
        self.find_listened();
        // A track that's joined or left a group is heard by that group's mute and solo now.
        let was = old.song.tracks.iter().map(|track| track.group);
        if was.ne(self.current.song.tracks.iter().map(|track| track.group)) {
            self.apply_audibility();
        }
        if !Arc::ptr_eq(&old.bank, &self.current.bank) {
            self.follow_bank();
        }
        old
    }

    /// Have every note playing one of the bank's entries follow its changes, heard at once, as
    /// moving a point is: notes sounding, and those waiting to sound or to sound again. One whose
    /// entry now plays another file, or nothing, keeps playing what it was.
    fn follow_bank(&mut self) {
        let bank = &self.current.bank;
        for track in self.tracks.iter_mut() {
            follow_voice(&mut track.voice, bank);
            follow_voice(&mut track.tail, bank);
            if let Some(note) = &mut track.last_note {
                follow_note(note, bank);
            }
            if let Some(delayed) = &mut track.delayed {
                follow_note(&mut delayed.note, bank);
            }
        }
        self.previews.follow(
            |voice| follow_voice(voice, bank),
            |note| follow_note(note, bank),
        );
    }

    /// Note which tracks a compressor, Warps or a low-pass gate hears. One that's muted keeps playing, silently, so
    /// a muted kick can still duck a bass, and one that no longer is heard stops. A group that's
    /// heard has its tracks heard too, so a muted kit can still duck a bass through its group.
    fn find_listened(&mut self) {
        let song = &self.current.song;
        let sounds = song.sound_tracks();
        let mut listened = [false; MAX_TRACKS];
        for insert in song.tracks.iter().flat_map(|track| track.inserts) {
            let heard = match insert {
                Insert::Compressor(compressor) => compressor.source_track(sounds),
                Insert::Warps(warps) => warps.source_track(sounds),
                Insert::Lpg(gate) => gate.source_track(sounds),
                _ => None,
            };
            if let Some(track) = heard
                && let Some(listened) = listened.get_mut(track)
            {
                *listened = true;
            }
        }
        for track in 0..sounds.min(MAX_TRACKS) {
            if let Some(group) = song.group_track_of(track)
                && listened.get(group) == Some(&true)
            {
                listened[track] = true;
            }
        }
        if listened != self.listened {
            self.listened = listened;
            self.apply_audibility();
        }
    }

    /// Whether a track plays its notes: when it's heard, or when a compressor or Warps hears it.
    fn plays(&self, index: usize) -> bool {
        self.audible(index) || self.listened[index]
    }

    /// A note has started on a track, so what's sounding on the others in its choke group stops,
    /// fading out as a new note fades the old one.
    fn choke(&mut self, index: usize) {
        let sounds = self.sounds();
        let tracks = &self.current.song.tracks;
        let Some(group) = tracks.get(index).and_then(|track| track.choke) else {
            return;
        };
        for (other, track) in tracks.iter().enumerate().take(sounds) {
            if other != index && track.choke == Some(group) {
                self.tracks[other].cut_to_tail();
            }
        }
    }

    /// The fixed tracks come after the others, so as those come and go, what the fixed tracks keep
    /// moves with them.
    fn keep_fixed_tracks(&mut self, was: usize) {
        let now = self.sounds();
        let was = was.min(MAX_SOUND_TRACKS);
        let moved = &mut self.tracks[was.min(now)..was.max(now) + FIXED_TRACKS];
        if now > was {
            moved.rotate_right(now - was);
        } else {
            moved.rotate_left(was - now);
        }
    }

    /// A track that's new comes in on the song's next step, counting its steps as if it had played
    /// at its speed since the last reset.
    fn line_up_new_tracks(&mut self, was: usize) {
        let at = self.next_step * TICKS_PER_STEP;
        for index in was.min(MAX_SOUND_TRACKS)..self.sounds() {
            let ticks = self.speed_ticks(index);
            let clock = &mut self.tracks[index].clock;
            clock.line_up(at);
            clock.count = (self.place * TICKS_PER_STEP).div_ceil(ticks) as i64;
        }
    }

    /// Keep each playing track on the row it was going to play next, as its chain's length changes:
    /// the same slot of its chain and row of that phrase's loop, or its chain's first row if those
    /// have gone. Only `started` moves, and a track whose chain didn't change is left alone, so its
    /// count since its chain started carries on.
    fn keep_places(&mut self, old: &Song) {
        if self.transport == Transport::Stopped {
            return;
        }
        let (now, was) = (self.sounds(), old.sound_tracks());
        for index in 0..self.current.song.tracks.len().min(MAX_TRACKS) {
            // A fixed track is compared with what it was, wherever that was, and a new one is new.
            let then = match index {
                _ if index >= now => index - now + was,
                _ if index < was => index,
                _ => continue,
            };
            let old_chain = match self.tracks[index].launched {
                Some(launched) => launched,
                None => old
                    .sections
                    .get(self.section)
                    .and_then(|s| s.chains.get(then).copied().flatten()),
            };
            let (Some(chain), Some(before)) = (self.track_chain(index), old.tracks.get(then))
            else {
                continue;
            };
            if old_chain != Some(chain) {
                continue;
            }
            let current = Arc::clone(&self.current);
            let track = &current.song.tracks[index];
            let (old_length, length) = (
                before.chain_steps(chain) as i64,
                track.chain_steps(chain) as i64,
            );
            if old_length == 0 || length == 0 {
                continue;
            }
            let dealer = self.dealer(index);
            let state = &mut self.tracks[index];
            let place = state.clock.count;
            let count = place - state.started;
            let next = count.rem_euclid(old_length);
            let laps = count.div_euclid(old_length);
            let lapped = |length: i64, at: i64| (laps.max(0) * length + at) as usize;
            let Some(was_at) = before.chain_position(chain, lapped(old_length, next), dealer)
            else {
                continue;
            };
            // The same pulse of the row too, as far as the row still lasts, where this pass of
            // its phrase's walk plays it nearest to where it was, or where it was if the pass
            // doesn't play it.
            let kept = track
                .chain_place(chain, was_at.row_place(), laps.max(0) as usize, dealer)
                .unwrap_or(0) as i64;
            let moved = length != old_length || kept != next;
            if moved {
                state.started = place - (laps * length + kept);
            }
            let (Some(old_phrase), Some(at)) = (
                before.phrases.get(&was_at.phrase),
                track.chain_position(chain, lapped(length, kept), dealer),
            ) else {
                continue;
            };
            let Some(phrase) = track.phrases.get(&at.phrase) else {
                continue;
            };
            let new_count = place - state.started;
            for slot in 0..FX_SLOTS {
                let (old_lane, lane) = (old_phrase.lane(slot), phrase.lane(slot));
                if !moved && old_lane == lane && was_at.phrase == at.phrase {
                    continue;
                }
                let Some(lane) = lane else {
                    state.lane_offsets[slot] = 0;
                    continue;
                };
                // The step this column was going to read, which it keeps if its lane still has it.
                let was_reading = match old_lane {
                    Some(old_lane) => old_lane.step_at(count + state.lane_offsets[slot]),
                    None => old_phrase.start() + was_at.step,
                };
                let target = if was_at.phrase == at.phrase && lane.contains(was_reading) {
                    was_reading
                } else {
                    lane.start
                };
                let offset = (target - lane.start) as i64 - new_count;
                state.lane_offsets[slot] = offset.rem_euclid(lane.length.max(1) as i64);
            }
        }
    }

    /// How many tracks play sounds, and so where the fixed tracks start.
    fn sounds(&self) -> usize {
        self.current.song.sound_tracks().min(MAX_SOUND_TRACKS)
    }

    /// Hear a change to a track's page straight away. A held value a step has set since stays
    /// until something changes it, so this only acts on what actually changed.
    fn follow_base(&mut self, old: &Published) {
        let fresh = Track::default();
        let (now, was) = (self.sounds(), old.song.sound_tracks());
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            // A fixed track is compared with what it was, wherever that was.
            let then = if index >= now {
                index - now + was
            } else {
                index
            };
            let before = if index >= now || index < was {
                old.song.tracks.get(then).unwrap_or(&fresh)
            } else {
                &fresh
            };
            for id in SettingId::all() {
                let (was, now) = (base_of(id, before), base_of(id, track));
                if was != now {
                    state.set_base(id, now);
                }
            }
        }
    }

    /// Move the tempo origin here if the tempo changed, so the next step's time stays right.
    fn retune(&mut self) {
        let millibpm = self.current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        if millibpm == self.millibpm {
            return;
        }
        let numerator = u128::from(self.frames_since_origin)
            * u128::from(TICKS_PER_QUARTER)
            * u128::from(self.millibpm);
        let denominator = 60 * u128::from(self.rate) * 1000;
        self.origin_tick += (numerator / denominator) as u64;
        self.frames_since_origin = 0;
        self.millibpm = millibpm;
    }

    /// Start at a step within the section, whatever is happening now. Anything sounding fades, and
    /// starting from the top resets held values.
    pub fn play_from(&mut self, step: usize) {
        self.play_section(0, step);
    }

    /// Start at a step within a section. Only the song's very start resets held values.
    pub fn play_section(&mut self, section: usize, step: usize) {
        self.halt(Transport::Stopped);
        self.clear_launches();
        if section == 0 && step == 0 {
            self.reset_tracks();
        }
        self.section = section;
        let within = self.walk_to(section, step);
        self.section_place = within;
        self.next_step = within;
        self.origin_tick = within * TICKS_PER_STEP;
        self.frames_since_origin = 0;
        self.millibpm = self.current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        self.transport = Transport::Playing;
        for track in self.tracks.iter_mut() {
            track.count_cycles_from_the_top();
        }
        self.update_lfos();
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

    fn update_lfos(&mut self) {
        let now = self.song_time();
        let stopped = self.transport == Transport::Stopped;
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            state.modulate(track, index, now, stopped, &mut self.memory);
        }
    }

    /// Start a note's modulation: the track's envelopes, and LFOs that follow its notes.
    fn start_modulation(&mut self, index: usize) {
        let now = self.song_time();
        let Some(track) = self.current.song.tracks.get(index) else {
            return;
        };
        let state = &mut self.tracks[index];
        state.trigger_mods(track, now);
        state.modulate(track, index, now, false, &mut self.memory);
    }

    /// Begin the release of a track's envelopes, when its note ends.
    fn release_modulation(&mut self, index: usize) {
        let now = self.song_time();
        if let Some(track) = self.current.song.tracks.get(index) {
            self.tracks[index].release_mods(track, now);
        }
    }

    /// What a track is playing with right now. A sound track with nothing going through it isn't
    /// run, so its slots turn nothing down, whatever they last did.
    pub fn live(&self, index: usize) -> LiveValues {
        match (self.tracks.get(index), self.current.song.tracks.get(index)) {
            (Some(state), Some(track)) => {
                let mut live = state.live();
                let run = !track.is_sound()
                    || state.sounding()
                    || state.ringing(self.rate)
                    || self.previews.on_track(&self.previews.sounding(), index);
                if !run {
                    live.reduction = Default::default();
                }
                // A note heard while stopped works the LFOs out once, but they aren't moving.
                if self.transport == Transport::Stopped {
                    live.mods = Default::default();
                }
                live
            }
            _ => LiveValues::default(),
        }
    }

    /// The shape of a track's notes: its first envelope on `Vol`, whichever slot that is, if it's
    /// in. Switched out, or with none, its notes have the plain shape a new track's have, from
    /// the next one.
    fn note_shape(&self, index: usize) -> Envelope {
        self.current
            .song
            .tracks
            .get(index)
            .and_then(|track| track.mods.get(note_shape_slot(&track.mods)?))
            .filter(|slot| !slot.bypassed)
            .map_or(Envelope::note_shape(), ModSlot::envelope)
    }

    /// Pause in place, or resume from the stored position.
    pub fn pause_or_resume(&mut self) {
        if self.transport == Transport::Playing {
            self.halt(Transport::Paused);
        } else {
            self.transport = Transport::Playing;
        }
    }

    /// Go back to the start, resetting held values and random generators. If playing, stop there.
    pub fn return_to_start(&mut self) {
        self.halt(Transport::Stopped);
        self.clear_launches();
        self.section = 0;
        self.section_place = 0;
        self.next_step = 0;
        self.realign();
        self.origin_tick = 0;
        self.frames_since_origin = 0;
        for track in self.tracks.iter_mut() {
            track.forget_worked(0);
        }
        self.reset_tracks();
    }

    /// Reset every track state's held values, random generators and LFOs, those past the song's
    /// tracks as an empty track's. Going through every setting is most of the time this takes, so
    /// a state past the tracks of every version since the last reset, and silent, has only the rest
    /// reset: its settings are still the empty track's that reset gave it, as only a track the song
    /// has changes them.
    fn reset_tracks(&mut self) {
        let seed = self.current.song.seed;
        let fresh = Track::default();
        let tracks = &self.current.song.tracks;
        for (index, state) in self.tracks.iter_mut().enumerate() {
            let used = index < self.used || state.sounding() || state.ringing(self.rate);
            if used {
                let track = tracks.get(index).unwrap_or(&fresh);
                state.reset_held(seed, index, track, self.clock);
            } else {
                state.reset_all_but_settings(seed, index, self.clock);
            }
        }
        self.used = tracks.len().min(MAX_TRACKS);
    }

    /// Stop sequencing but let everything sounding finish on its own, for a render's tail.
    pub fn let_ring(&mut self) {
        self.transport = Transport::Stopped;
        for track in self.tracks.iter_mut() {
            track.voice.release_if_held();
        }
    }

    /// Stop moving. Voices fade so stopping doesn't click, and effect tails keep ringing.
    fn halt(&mut self, to: Transport) {
        for track in self.tracks.iter_mut() {
            track.stop();
        }
        self.transport = to;
        self.realign_waiting = false;
        // A loop waiting for the grid is the loop from now, as there's no line coming round.
        if let Some(sections) = self.loop_cued.take() {
            self.looping = Some(sections);
        }
        for track in self.tracks.iter_mut() {
            track.realign_waiting = false;
            track.mute_waiting = false;
            track.solo_waiting = false;
        }
    }

    /// Turn fill on or off, which `CND`'s `FILL` and `!FILL` read as each row is decided.
    pub fn set_fill(&mut self, on: bool) {
        self.fill = on;
    }

    /// Whether fill is on.
    pub fn fill(&self) -> bool {
        self.fill
    }

    /// Set the scale an `ACU` moves notes through, from the next note it moves.
    pub fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
    }

    /// A track's faders as they're heard, for the mixer, the Output device and the Roto.
    pub fn faders(&self, index: usize) -> Faders {
        self.tracks
            .get(index)
            .map(TrackState::faders)
            .unwrap_or_default()
    }

    /// A hand moved one of a track's faders that the steps drive: it's the hand's from now, until
    /// it's given back (decision 174).
    pub fn take_over(&mut self, track: usize, fader: usize) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.take_over(fader);
        }
    }

    /// Give a track's faders back to the steps, or every track's, as `Space e` and `Space E` do.
    pub fn give_back(&mut self, track: Option<usize>) {
        match track {
            Some(track) => {
                if let Some(state) = self.tracks.get_mut(track) {
                    state.give_back_all();
                }
            }
            None => {
                for state in &mut self.tracks {
                    state.give_back_all();
                }
            }
        }
    }

    /// Set whether a track is muted, taking back a switch of its mute waiting for the grid. A
    /// muted track's sounding note fades.
    pub fn set_mute(&mut self, track: usize, mute: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute = mute;
            state.mute_waiting = false;
        }
        self.apply_audibility();
    }

    /// Switch a track's mute at the next line of the launch grid, or take back a switch waiting
    /// for it. Only while playing, as there's no grid coming round otherwise.
    pub fn toggle_mute_on_grid(&mut self, track: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute_waiting = playing && !state.mute_waiting;
        }
    }

    /// Switch a track's solo at the next line of the launch grid, or take back a switch waiting
    /// for it, as [`Engine::toggle_mute_on_grid`] does a mute.
    pub fn toggle_solo_on_grid(&mut self, track: usize) {
        let playing = self.transport == Transport::Playing;
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo_waiting = playing && !state.solo_waiting;
        }
    }

    /// Whether a track's mute switches at the next line of the launch grid.
    pub fn mute_waiting(&self, track: usize) -> bool {
        self.tracks.get(track).is_some_and(|t| t.mute_waiting)
    }

    /// Whether a track's solo switches at the next line of the launch grid.
    pub fn solo_waiting(&self, track: usize) -> bool {
        self.tracks.get(track).is_some_and(|t| t.solo_waiting)
    }

    /// Switch every mute and solo waiting for this line of the grid, together, so they're heard
    /// from the same sample.
    fn switch_waiting(&mut self) {
        for track in self.tracks.iter_mut() {
            if !(track.mute_waiting || track.solo_waiting) {
                continue;
            }
            track.mute ^= track.mute_waiting;
            track.solo ^= track.solo_waiting;
            track.mute_waiting = false;
            track.solo_waiting = false;
            track.switches_landed = track.switches_landed.wrapping_add(1);
        }
        self.apply_audibility();
    }

    /// Set whether a track is soloed. While a track that plays sounds or a group is soloed, only
    /// soloed ones play, with the groups of soloed tracks and the tracks of soloed groups, and the
    /// returns stay. While a return is soloed, only soloed returns are heard, and what sends to
    /// them keeps playing without being heard itself.
    pub fn set_solo(&mut self, track: usize, solo: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo = solo;
            state.solo_waiting = false;
        }
        self.apply_audibility();
    }

    /// Whether a sound track or a group is soloed.
    fn any_solo(&self) -> bool {
        self.tracks[..self.returns()].iter().any(|t| t.solo)
    }

    /// Whether a return is soloed.
    fn return_soloed(&self) -> bool {
        self.tracks[self.returns()..self.returns() + 2]
            .iter()
            .any(|t| t.solo)
    }

    /// Where the returns start, after the sound tracks and the groups.
    fn returns(&self) -> usize {
        self.sounds() + MAX_GROUPS
    }

    /// The group track a sound track plays through, if it's in one.
    fn group_of(&self, index: usize) -> Option<usize> {
        self.current
            .song
            .group_track_of(index)
            .filter(|group| *group < MAX_TRACKS)
    }

    /// Whether a sound track is heard, given whether anything's soloed: not muted, nor in a muted
    /// group, and, while something is soloed, soloed itself or in a soloed group.
    fn heard_with(&self, index: usize, any_solo: bool) -> bool {
        let state = &self.tracks[index];
        let group = self.group_of(index).map(|group| &self.tracks[group]);
        !state.mute
            && !group.is_some_and(|group| group.mute)
            && (!any_solo || state.solo || group.is_some_and(|group| group.solo))
    }

    /// Whether a sound track plays into its group, as it's heard but leaving out what the group
    /// decides for itself: its own mute, and solos outside it. Only a track soloed in the same
    /// group, and the group not soloed, keeps its other tracks out. With the group's own fade
    /// after, what's heard is the same as by `heard_with`.
    fn feeds_with(&self, index: usize, group: usize, any_solo: bool) -> bool {
        let state = &self.tracks[index];
        let lit =
            || (0..self.sounds()).any(|t| self.tracks[t].solo && self.group_of(t) == Some(group));
        !state.mute && (!any_solo || state.solo || self.tracks[group].solo || !lit())
    }

    /// Whether a group is heard, given whether anything's soloed: not muted, and, while something
    /// is soloed, soloed itself or holding a soloed track.
    fn group_heard_with(&self, group: usize, any_solo: bool) -> bool {
        let state = &self.tracks[group];
        let lit =
            || (0..self.sounds()).any(|t| self.tracks[t].solo && self.group_of(t) == Some(group));
        !state.mute && (!any_solo || state.solo || lit())
    }

    fn apply_audibility(&mut self) {
        let any_solo = self.any_solo();
        let return_soloed = self.return_soloed();
        let (sounds, returns) = (self.sounds(), self.returns());
        for index in 0..sounds {
            let heard = self.heard_with(index, any_solo);
            // A group a compressor or Warps hears decides for itself whether it's heard, and its
            // tracks play into it all the same.
            let feeds = match self.group_of(index) {
                Some(group) if self.listened[group] => self.feeds_with(index, group, any_solo),
                _ => heard,
            };
            let track = &mut self.tracks[index];
            // One a compressor or Warps hears keeps playing, faded out of the mix and the sends.
            track.heard.target = if heard { 1.0 } else { 0.0 };
            track.feeds.target = if feeds { 1.0 } else { 0.0 };
            if !heard && !self.listened[index] {
                track.cut_to_tail();
            }
        }
        for group in sounds..returns {
            let heard = self.group_heard_with(group, any_solo);
            self.tracks[group].heard.target = if heard { 1.0 } else { 0.0 };
        }
        for track in self.tracks[returns..returns + 2].iter_mut() {
            let heard = !track.mute && (!return_soloed || track.solo);
            track.heard.target = if heard { 1.0 } else { 0.0 };
        }
    }

    /// Play a note on a preview voice for a short while. It never cuts a sequenced note, or is
    /// cut by one.
    pub fn preview(&mut self, track: usize, note: u8, velocity: u8) {
        let frames = PREVIEW_MS * self.rate / 1000;
        self.start_preview(track, note, velocity, Some(frames));
    }

    /// Play a note on a preview voice until [`Engine::release_preview`] lets it go, for a key
    /// held down. Each key down has a voice of its own, so they play a chord.
    pub fn hold_preview(&mut self, track: usize, note: u8, velocity: u8) {
        self.start_preview(track, note, velocity, None);
    }

    /// Let go of a note held through a track, leaving any others sounding.
    pub fn release_preview(&mut self, track: usize, note: u8) {
        self.previews.let_go(track, note, LetGo::Now);
    }

    /// Let go of a note held through a track once it has sounded as long as a preview does: an
    /// audition's key tapped rather than held.
    pub fn finish_preview(&mut self, track: usize, note: u8) {
        let frames = PREVIEW_MS * self.rate / 1000;
        self.previews
            .let_go(track, note, LetGo::OnceSounded(frames));
    }

    /// Play a one-off file through a track on a preview voice, to its end or until another is
    /// played or it's stopped, as a sample browser plays the file its cursor moves onto. With a
    /// span, only that much of it plays, the way the span goes, with its gain, pan, tune and fades,
    /// as the sample page plays a slice or from its cursor. It's handed over whole, so nothing is
    /// loaded or freed here. What's given back is for the edit thread to free: the file it
    /// replaces, or this one if the track can't play it.
    pub fn play_file(
        &mut self,
        track: usize,
        file: Arc<SampleData>,
        span: Option<Span>,
    ) -> Option<Arc<SampleData>> {
        self.play_one_off(
            track,
            file,
            span,
            velocity_gain(DEFAULT_NOTE_VELOCITY),
            Envelope::note_shape(),
        )
    }

    /// Play audio through a track as it is, at full level from its first frame to its last with no
    /// fade in or out, so what comes out is what the track's filter and effects make of it, for
    /// `vimtracker probe`. Otherwise it goes as [`Engine::play_file`] does.
    pub fn play_input(&mut self, track: usize, input: Arc<SampleData>) -> Option<Arc<SampleData>> {
        let unshaped = Adsr {
            attack_ms: 0,
            release_ms: 0,
            ..Adsr::note_shape()
        };
        self.play_one_off(track, input, None, 1.0, Envelope::Adsr(unshaped))
    }

    fn play_one_off(
        &mut self,
        track: usize,
        file: Arc<SampleData>,
        span: Option<Span>,
        gain: f32,
        shape: Envelope,
    ) -> Option<Arc<SampleData>> {
        let plays = self
            .current
            .song
            .tracks
            .get(track)
            .is_some_and(Track::is_sound);
        if !plays {
            return Some(file);
        }
        for id in FILE_SAMPLES {
            self.previews.fade_sample(id);
        }
        // A span is of this file, whatever length it was worked out for, so it turns round at
        // the file's own end when it's reversed.
        let frames = file.frames.len() as u64;
        let span = span.map(|span| Span { frames, ..span });
        // The other place, as the last file may still be fading out of this one.
        let place = 1 - self.file_place;
        self.file_place = place;
        let old = self.files[place].replace(file);
        let on = NoteOn {
            sample: FILE_SAMPLES[place],
            synth: None,
            note: 60,
            gain,
            entry: None,
            slice: 0,
            span,
            cut: Cut::default(),
            accent: false,
            shape,
            release_after: None,
        };
        let sounding = self.previews.sounding();
        let previewing = self.previews.on_track(&sounding, track);
        let state = &mut self.tracks[track];
        if !state.sounding() && !previewing {
            state.settle();
        }
        self.previews
            .start(track, on, state.synth_settings(), self.rate);
        old
    }

    /// Let go of any one-off file playing.
    pub fn stop_file(&mut self) {
        for id in FILE_SAMPLES {
            self.previews.release_sample(id);
        }
    }

    /// How many preview voices are sounding.
    pub fn previews_sounding(&self) -> usize {
        self.previews.count()
    }

    /// Where each voice playing a sample through a track is in it, for the sample page's
    /// playheads: the track's own note and its tail, then the preview voices, as many as there's
    /// room for. A synth's note has no place in a sample, and a one-off file has no entry.
    pub fn playheads(&self, track: usize) -> [Option<Playhead>; PLAYHEADS] {
        let mut found = [None; PLAYHEADS];
        let Some(state) = self.tracks.get(track) else {
            return found;
        };
        let voices = [&state.voice, &state.tail]
            .into_iter()
            .chain(self.previews.on(track));
        let heads = voices.filter_map(|voice| {
            let frame = voice.frame()?;
            let file = FILE_SAMPLES.contains(&voice.sample());
            let entry = if file { None } else { Some(voice.entry()?) };
            Some(Playhead { entry, frame })
        });
        for (slot, head) in found.iter_mut().zip(heads) {
            *slot = Some(head);
        }
        found
    }

    fn start_preview(&mut self, track: usize, note: u8, velocity: u8, release_after: Option<u32>) {
        let shape = self.note_shape(track);
        let Some(on) = self.note_on(track, note, velocity, shape) else {
            return;
        };
        let sounding = self.previews.sounding();
        let previewing = self.previews.on_track(&sounding, track);
        let Some(state) = self.tracks.get_mut(track) else {
            return;
        };
        if !state.sounding() && !previewing {
            state.settle();
        }
        let on = NoteOn {
            release_after,
            ..on
        };
        self.previews
            .start(track, on, state.synth_settings(), self.rate);
    }

    /// A note at a velocity, which sets its level and whether it's accented. `Acid` has no velocity,
    /// as a 303 has none: its velocity only says whether it's accented, and every plain note it
    /// plays sounds the same, as does every accent (decision 121). A sampler plays the bank entry
    /// its `SMP` is on as the note starts, the slice of it its `SLI` is on, as its `STA`, `SLEN`
    /// and `PLY` cut it then, and has nothing to play with no entry.
    fn note_on(&self, track: usize, note: u8, velocity: u8, shape: Envelope) -> Option<NoteOn> {
        let owner = self.current.song.tracks.get(track)?;
        if !owner.is_sound() {
            return None;
        }
        let (sample, entry, slice, span, cut, synth) = match owner.machine {
            Machine::Sampler => {
                let state = self.tracks.get(track)?;
                let number = state.sample_number();
                let playable = self.current.bank[usize::from(number)].as_deref()?;
                let cut = state.sample_cut();
                let slice = state.slice_number(playable, owner.setting(SettingId::SLICE));
                (
                    playable.sample,
                    Some(number),
                    slice,
                    Some(playable.span(slice, cut.mode == PlayMode::Thru)),
                    cut,
                    None,
                )
            }
            Machine::Plaits(voice) => (0, None, 0, None, Cut::default(), Some(voice)),
        };
        let accent = velocity >= ACCENT_VELOCITY;
        let heard = match synth {
            Some(SynthVoice::Acid) => ACID_VELOCITY,
            _ => velocity,
        };
        Some(NoteOn {
            sample,
            synth,
            note,
            gain: velocity_gain(heard),
            entry,
            slice,
            span,
            cut,
            accent,
            shape,
            release_after: None,
        })
    }

    /// Fill interleaved stereo output. This doesn't allocate, lock or print.
    /// The master's levels since this was last asked.
    pub fn take_reading(&mut self) -> Reading {
        std::mem::take(&mut self.reading)
    }

    /// Each track's levels over the last so many frames. A silent track skips adding to its reading,
    /// so the frames are given here, and an average counts the silence.
    pub fn take_track_readings(&mut self, frames: u32) -> [Reading; MAX_TRACKS] {
        let mut readings = [Reading::default(); MAX_TRACKS];
        for (reading, track) in readings.iter_mut().zip(self.tracks.iter_mut()) {
            *reading = std::mem::take(&mut track.reading);
            reading.frames = frames;
        }
        readings
    }

    /// The highest level of a track's sides since this was last asked, right after its trim and
    /// before its filter, for the Output device's meter. Every track's starts again from silence,
    /// so one watched next doesn't begin with what it played while it wasn't.
    pub fn take_after_trim(&mut self, index: usize) -> [f32; 2] {
        let taken = self
            .tracks
            .get(index)
            .map_or([0.0; 2], |track| track.after_trim);
        for track in &mut self.tracks {
            track.after_trim = [0.0; 2];
        }
        taken
    }

    pub fn process(&mut self, out: &mut [f32]) {
        self.stopwatch.restart_lap();
        let mut written = 0;
        let total = out.len() / 2;
        while written < total {
            let playing = self.transport == Transport::Playing;
            // A track's own step that falls before the song's next is worked out first, so every
            // step is worked out in the order it falls.
            let track_step = if playing {
                self.next_track_step()
            } else {
                None
            };
            if let Some((index, 0)) = track_step {
                let at = self.tracks[index].clock.beat;
                self.track_step(index, at, false);
                continue;
            }
            if playing && self.frames_until_next_step() == 0 {
                self.fire_step();
                self.next_step += 1;
                self.place += 1;
                self.section_place += 1;
                continue;
            }
            if playing {
                self.fire_due_events();
            }
            if self.lfo_countdown == 0 {
                self.update_lfos();
                self.lfo_countdown = LFO_UPDATE_FRAMES;
            }
            let until_event = if playing {
                self.frames_until_next_step()
                    .min(self.frames_until_track_event())
                    .min(track_step.map_or(u64::MAX, |(_, frames)| frames))
            } else {
                u64::MAX
            };
            let run = until_event
                .min((total - written) as u64)
                .min(u64::from(self.lfo_countdown))
                .max(1) as usize;
            self.lfo_countdown -= run as u32;
            self.render(&mut out[written * 2..(written + run) * 2]);
            self.rendered += run as u64;
            if playing {
                self.frames_since_origin += run as u64;
                self.clock += run as u64;
            }
            written += run;
        }
        self.stopwatch.lap(TimedPart::Sequencing);
    }

    /// Start timing where the engine's time goes, a track and a slot at a time, from now. This
    /// allocates, so it's done before audio starts.
    pub fn time_parts(&mut self) {
        self.stopwatch.start();
        for track in self.tracks.iter_mut() {
            track.slot_times = SlotTimes::running();
        }
    }

    /// Where the engine's time has gone since [`Engine::time_parts`], if it was asked.
    pub fn part_times(&mut self) -> Option<PartTimes> {
        for (index, track) in self.tracks.iter_mut().enumerate() {
            self.stopwatch.add_slots(index, &mut track.slot_times);
        }
        self.stopwatch.times().cloned()
    }

    /// Frames until the next step is worked out: its place on the grid, or as far before it as
    /// a groove can land a step early.
    fn frames_until_next_step(&self) -> u64 {
        if !self.has_steps() {
            return u64::MAX;
        }
        self.frames_until_worked_out(self.next_step * TICKS_PER_STEP)
    }

    /// Frames until a step at this place on the grid is worked out.
    fn frames_until_worked_out(&self, grid: u64) -> u64 {
        let tick = grid
            .saturating_sub(self.current.lead)
            .saturating_sub(self.origin_tick);
        sample_of_tick(tick, self.rate, self.millibpm).saturating_sub(self.frames_since_origin)
    }

    /// The track whose own step falls soonest before the song's next step, and frames until it's
    /// worked out. One whose next step falls with the song's is worked out with it.
    fn next_track_step(&self) -> Option<(usize, u64)> {
        let next = self.next_step * TICKS_PER_STEP;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        let (index, beat) = (0..tracks)
            .map(|index| (index, self.tracks[index].clock.beat))
            .filter(|(_, beat)| *beat < next)
            .min_by_key(|(index, beat)| (*beat, *index))?;
        if !self.has_steps() {
            return None;
        }
        Some((index, self.frames_until_worked_out(beat)))
    }

    /// Frames in so many ticks of a sixteenth, taken as shares of a step this many ticks long, so
    /// a push, a nudge or a `DEL` on a slower track is that much longer. At a sixteenth it's the
    /// frames in the ticks, exactly.
    fn frames_of(&self, ticks: u64, step: u64) -> u64 {
        let numerator = u128::from(ticks) * u128::from(step) * 60 * u128::from(self.rate) * 1000;
        let denominator =
            u128::from(TICKS_PER_STEP) * u128::from(TICKS_PER_QUARTER) * u128::from(self.millibpm);
        (numerator / denominator) as u64
    }

    /// Where on the clock a step lands: its place on the grid, moved by its push in ticks of a
    /// step this many ticks long. Each part is worked out from the tempo's origin, so a step on
    /// the grid lands where it always did, and swing's delay is what it always was.
    fn landing(&self, grid: u64, push: i64, step: u64) -> u64 {
        let on_grid = sample_of_tick(
            grid.saturating_sub(self.origin_tick),
            self.rate,
            self.millibpm,
        );
        let moved = self.frames_of(push.unsigned_abs(), step);
        let lands = if push >= 0 {
            on_grid + moved
        } else {
            on_grid.saturating_sub(moved)
        };
        (self.clock + lands).saturating_sub(self.frames_since_origin)
    }

    /// Frames until a track's delayed, cut, release or retrigger event.
    fn frames_until_track_event(&self) -> u64 {
        let mut soonest = u64::MAX;
        for track in self.tracks.iter() {
            soonest = soonest.min(track.soonest().saturating_sub(self.clock));
            let events = [
                track.delayed.map(|d| d.at),
                track.kill_at,
                track.off_at,
                track.gate_at,
                track.retrigger.map(|r| r.next_at),
                track.arp.map(|arp| arp.next_at),
            ];
            for at in events.into_iter().flatten() {
                soonest = soonest.min(at.saturating_sub(self.clock));
            }
        }
        soonest
    }

    /// How long a note of so many pulses is held for, from its start, at its row's own length or
    /// else the track's `Gate`, or `None` at 100%, which holds it until something ends it, as
    /// before there was a `Gate`. Its pulses are steps as the groove lands them, so it runs to
    /// where its last pulse lands, and then its share of how long the groove makes that pulse,
    /// each as long as the track's speed makes a step.
    fn gate_frames(
        &self,
        index: usize,
        pulses: usize,
        own: Option<u8>,
        step: &PendingStep,
    ) -> Option<u64> {
        let share = self.tracks[index].gate_share(own);
        if share >= 1.0 {
            return None;
        }
        let (groove, first) = (&step.groove, step.running);
        let last = first + pulses.saturating_sub(1);
        let ticks = TICKS_PER_STEP as f64;
        let moved = f64::from(groove.push(last)) - f64::from(groove.push(first));
        let length = groove.length(last) as f64 / ticks;
        let pulses = pulses.saturating_sub(1) as f64 + moved / ticks + f64::from(share) * length;
        let step = step.ticks as f64 * 60.0 * f64::from(self.rate) * 1000.0
            / (TICKS_PER_QUARTER as f64 * f64::from(self.millibpm));
        Some((pulses * step).round() as u64)
    }

    /// Frames in so many of `DEL`'s units, sixths of a step this many ticks long.
    fn groove_units(&self, units: u8, step: u64) -> u64 {
        self.frames_of(u64::from(units) * GROOVE_UNIT_TICKS, step)
    }

    /// Whether a track is heard by the mutes and solos: a sound track by its group's too, and a
    /// group by its tracks'.
    fn audible(&self, index: usize) -> bool {
        let any_solo = self.any_solo();
        match index {
            _ if index < self.sounds() => self.heard_with(index, any_solo),
            _ if index < self.returns() => self.group_heard_with(index, any_solo),
            _ => {
                let state = &self.tracks[index];
                !state.mute && (!any_solo || state.solo)
            }
        }
    }

    /// Run anything due now on each track's own schedule.
    fn fire_due_events(&mut self) {
        let clock = self.clock;
        for index in 0..MAX_TRACKS {
            while let Some(part) = self.tracks[index].take_due(clock) {
                self.fire_track(index, part);
            }
            if let Some(delayed) = self.tracks[index].delayed
                && delayed.at <= clock
            {
                self.tracks[index].delayed = None;
                self.start_modulation(index);
                if self.plays(index) {
                    let (kill, off, gate, retrigger) = {
                        let t = &self.tracks[index];
                        (t.kill_at, t.off_at, t.gate_at, t.retrigger)
                    };
                    self.tracks[index].start(delayed.note, self.rate);
                    self.choke(index);
                    let t = &mut self.tracks[index];
                    t.kill_at = kill;
                    t.off_at = off;
                    t.gate_at = gate;
                    t.retrigger = retrigger;
                }
            }
            // Whatever ends the note ends its arpeggio.
            if self.tracks[index].kill_at.is_some_and(|at| at <= clock) {
                self.tracks[index].kill_at = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].cut_to_tail();
            }
            if self.tracks[index].off_at.is_some_and(|at| at <= clock) {
                self.tracks[index].off_at = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].voice.release();
            }
            // The gate closing lets the note go as `OFF` does, and ends its ratchets, which only
            // repeat while the gate's open.
            if self.tracks[index].gate_at.is_some_and(|at| at <= clock) {
                self.tracks[index].gate_at = None;
                self.tracks[index].retrigger = None;
                self.tracks[index].arp = None;
                self.release_modulation(index);
                self.tracks[index].voice.release();
            }
            // An arpeggio's note goes before a ratchet due with it, so the ratchet strikes it.
            if let Some(arp) = self.tracks[index].arp
                && arp.next_at <= clock
            {
                self.fire_arp(index, arp);
            }
            if let Some(retrigger) = self.tracks[index].retrigger
                && retrigger.next_at <= clock
            {
                self.fire_retrigger(index, retrigger);
            }
        }
    }

    fn fire_retrigger(&mut self, index: usize, retrigger: Retrigger) {
        let rate = self.rate;
        let audible = self.plays(index);
        if self.tracks[index].last_note.is_some() {
            self.start_modulation(index);
        }
        // With the track's `ACU`s adding on each hit, each moves the row's note on.
        let moved = match (retrigger.accumulates, self.tracks[index].last_note) {
            (Some(row), Some(_)) => {
                let heard =
                    self.accumulate(index, row.epoch, row.row, row.written, Some(row.amount));
                Some(
                    (i16::from(heard) + i16::from(row.transpose))
                        .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8,
                )
            }
            _ => None,
        };
        let track = &mut self.tracks[index];
        let Some(note) = track.last_note else {
            track.retrigger = None;
            return;
        };
        // `Acid` has no level but its accent, as a 303 has none, so each hit plays at the note's
        // own level, plain or accented, and `RET`'s fade and swell leave it alone (decision 124).
        // The accent carries on to every hit with the rest of the note.
        let gain = if note.synth == Some(SynthVoice::Acid) {
            note.gain
        } else {
            (note.gain * retrigger.factor).clamp(0.0, 1.0)
        };
        let next = NoteOn {
            gain,
            note: moved.unwrap_or(note.note),
            ..note
        };
        let (kill, off, gate) = (track.kill_at, track.off_at, track.gate_at);
        if audible {
            track.start(next, rate);
        }
        track.kill_at = kill;
        track.off_at = off;
        track.gate_at = gate;
        track.last_note = Some(next);
        track.retrigger = Some(Retrigger {
            next_at: retrigger.next_at + retrigger.interval,
            ..retrigger
        });
    }

    /// Play an arpeggio's next note: as a tie, which changes only the pitch, or with `Retrig` on,
    /// starting the note and its envelopes again. Each note is counted from the first, so a long
    /// note's don't drift.
    fn fire_arp(&mut self, index: usize, arp: Arpeggiating) {
        let rate = self.rate;
        let audible = self.plays(index);
        let Some(note) = self.tracks[index].last_note else {
            self.tracks[index].arp = None;
            return;
        };
        let roll = if arp.mode == ArpMode::Random {
            self.tracks[index].arp_rng.next_u32()
        } else {
            0
        };
        let place = arp.cycle.place(arp.mode, arp.played, roll);
        let next = NoteOn {
            note: arp.cycle.note(place),
            ..note
        };
        if arp.retrig {
            self.start_modulation(index);
            let track = &mut self.tracks[index];
            let (kill, off, gate) = (track.kill_at, track.off_at, track.gate_at);
            if audible {
                track.restrike(next, rate);
            }
            track.kill_at = kill;
            track.off_at = off;
            track.gate_at = gate;
            track.last_note = Some(next);
        } else {
            self.tracks[index].tie(next);
        }
        let played = arp.played + 1;
        let next_at = arp.from + self.frames_of(played * arp.note_ticks, arp.step_ticks);
        self.tracks[index].arp = Some(Arpeggiating {
            played,
            place,
            next_at,
            ..arp
        });
    }

    /// Move the song on a step: into the next section if this one has played out, and back to the
    /// start at a reset. Then work out the step of every track whose own step falls here, and
    /// start what's cued on any partway through a longer step, which then comes in here.
    fn fire_step(&mut self) {
        // The step last worked out, which is still the one heard until this one's place comes.
        self.heard_place = self.place.saturating_sub(1);
        if !self.enter_section_if_due() {
            return;
        }
        if self.past_reset(self.place) {
            self.realign();
        }
        // A realign asked for, of every track or of one, lands here as a reset would, after the
        // section has moved on, and so do mutes and solos waiting for the grid, before the line's
        // notes, which they're heard or silenced with.
        let switching = self.tracks.iter().any(|t| t.mute_waiting || t.solo_waiting);
        let waiting =
            self.realign_waiting || switching || self.tracks.iter().any(|t| t.realign_waiting);
        let line = waiting && self.on_grid_line(self.realign_grid());
        if switching && line {
            self.switch_waiting();
        }
        if self.realign_waiting && line {
            self.realign_waiting = false;
            self.realign();
            // Every track's own realign waiting for this line comes with it.
            for track in self.tracks.iter_mut() {
                track.realign_waiting = false;
            }
        }
        let grid = self.next_step * TICKS_PER_STEP;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        for index in 0..tracks {
            // A track's own realign, or its own reset, starts its chain again here, as a section
            // bringing it in would, a longer step cut short. A reset or realign of every track
            // just now has started it here already.
            let realigns = line && self.tracks[index].realign_waiting;
            if realigns {
                self.tracks[index].realign_waiting = false;
            }
            if realigns || self.reset_due(index, grid) {
                self.tracks[index].restart_at(grid);
            }
            if self.tracks[index].clock.beat != grid {
                self.start_cued(index, grid, true);
                if self.tracks[index].clock.beat != grid {
                    continue;
                }
            }
            self.track_step(index, grid, true);
        }
    }

    /// Work out a track's next step, which falls here on the grid, after playing what's waiting
    /// and starting what's cued, as the track's launch timing allows on the song's step or the
    /// track's own.
    fn track_step(&mut self, index: usize, at: u64, song_step: bool) {
        // A big tempo jump can bring this step before the last one landed. With nudges, a row
        // nudged late can land after the next step is worked out, and with speeds a fast track's
        // steps can be worked out ahead of a slow one's lead, so only what's due goes now.
        let by = if self.current.nudges || self.current.speeds {
            self.clock
        } else {
            u64::MAX
        };
        while let Some(part) = self.tracks[index].take_due(by) {
            self.fire_track(index, part);
        }
        self.start_cued(index, at, song_step);
        self.work_out(index);
    }

    /// Work out a track's next step, and play it now or when its groove lands it. A row with a
    /// nudge lands its note part that much off where the groove lands its lanes. The step lasts as
    /// long as its speed says, until the next of that speed's steps.
    fn work_out(&mut self, index: usize) {
        let (lead, groove_lead) = (self.current.lead, self.current.groove_lead as i64);
        let nudges = self.current.nudges;
        let clock = self.tracks[index].clock;
        let (grid, running) = (clock.beat, clock.count);
        self.tracks[index].playing = self.track_chain(index);
        // The groove counts the track's own steps from the start or the last reset, not its
        // loop, so an odd length stays on the beat, and a track at an eighth swings its eighths.
        let (groove, ticks) = self.step_choices(index, running);
        let humanise = self.tracks[index].humanise(groove.humanise());
        let at = running.max(0) as usize;
        let push = (i64::from(groove.push(at)) + humanise).clamp(-groove_lead, i64::from(MAX_PUSH));
        let place = self.track_place(index, running);
        let step = PendingStep {
            at: self.landing(grid, push, ticks),
            running: at,
            groove,
            ticks,
            part: Part::Whole,
            step: grid,
            leads: true,
            place,
            count: running - self.tracks[index].started,
            plays: None,
            start: self.tracks[index].chain_starts,
        };
        // Grid, then groove, then nudge. A track's notes play in order, so a row's note part
        // never lands before the last row's.
        let row_at = if nudges {
            let nudge = self.row_nudge(index, place);
            let earliest = (lead * TICKS_PER_STEP / ticks) as i64;
            let at = self.landing(grid, (push + nudge).max(-earliest), ticks);
            self.tracks[index]
                .last_row_at()
                .map_or(at, |last| at.max(last))
        } else {
            step.at
        };
        let parts = if row_at == step.at {
            [Some(step), None]
        } else {
            let lanes = PendingStep {
                part: Part::Lanes,
                leads: step.at < row_at,
                ..step
            };
            let row = PendingStep {
                at: row_at,
                part: Part::Row,
                leads: row_at < step.at,
                ..step
            };
            [Some(lanes), Some(row)]
        };
        let state = &mut self.tracks[index];
        let next = state.clock.after(ticks);
        state.note_worked(Worked {
            grid,
            count: running,
            ticks: next - grid,
        });
        state.clock.advance(ticks);
        for part in parts.into_iter().flatten() {
            if let Some(earliest) = self.tracks[index].wait_for(part) {
                self.fire_track(index, earliest);
            }
        }
        while let Some(part) = self.tracks[index].take_due(self.clock) {
            self.fire_track(index, part);
        }
    }

    /// How far a row's nudge moves its note part, in ticks.
    fn row_nudge(&self, index: usize, place: Option<(u32, usize)>) -> i64 {
        let Some((chain, place)) = place else {
            return 0;
        };
        let Some(owner) = self.current.song.tracks.get(index) else {
            return 0;
        };
        owner
            .chain_position(chain, place, self.dealer(index))
            .and_then(|position| owner.step_at(position))
            .map_or(0, |step| i64::from(step.nudge) * NUDGE_TICKS as i64)
    }

    /// The groove a track's step plays with and how long it is, in ticks, worked out before it's
    /// played, as each says when the step lands: a `GRV` or `RATE` on its row or in a lane at it,
    /// or else what the track holds, which a note on the row's first pulse ends any lock of, as
    /// playing it will. A `GRV` of `00` is the project's.
    fn step_choices(&self, index: usize, running: i64) -> (Groove, u64) {
        let song = &self.current.song;
        let state = &self.tracks[index];
        // With no `GRV` or `RATE` in the song, there's no row to look on, and the track holds its
        // own.
        let row = self
            .track_place(index, running)
            .filter(|_| self.current.groove_steps || self.current.speed_steps)
            .and_then(|(chain, place)| {
                let owner = song.tracks.get(index)?;
                let position = owner.chain_position(chain, place, self.dealer(index))?;
                let phrase = owner.phrases.get(&position.phrase)?;
                Some((position, owner.step_at(position)?, phrase))
            });
        let choose = |id: SettingId| match row {
            Some((position, step, phrase)) => {
                let first = position.pulse == 0;
                let count = running - state.started;
                // The last column's wins, as they run left to right.
                let chosen = (0..FX_SLOTS)
                    .rev()
                    .filter_map(|number| match phrase.lane(number) {
                        Some(lane) => {
                            let at = lane.step_at(count + state.lane_offsets[number]);
                            phrase.steps().get(at)?.fx.get(number)
                        }
                        None if first => step.fx.get(number),
                        None => None,
                    })
                    .find(|slot| slot.command == Some(FxCommand::Set(id)))
                    .map(|slot| slot.coarse());
                let note =
                    first && matches!(step.note, Note::On(_)) && step.gate_kind() != GateKind::Rest;
                chosen.unwrap_or_else(|| state.held_choice(id, note))
            }
            None => state.held_choice(id, false),
        };
        let groove = match usize::from(choose(SettingId::GROOVE)).checked_sub(1) {
            Some(at) => GrooveId::from_index(at).unwrap_or(GrooveId::STRAIGHT),
            None => song.groove,
        };
        let speed = Speed::from_index(usize::from(choose(SettingId::SPEED)));
        (song.groove(groove), speed.ticks())
    }

    /// Start or stop a track's note for a step, and run its FX left to right: all of it, or the
    /// part a nudge has it land apart, its lanes' values or its row's note part.
    fn fire_track(&mut self, index: usize, pending: PendingStep) {
        let running = pending.running;
        let row = pending.part.plays_row();
        if row {
            self.tracks[index].retrigger = None;
        }
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some((chain, place)) = pending.place else {
            return;
        };
        let Some(owner) = song.tracks.get(index) else {
            return;
        };
        // Which time round its chain the track is, for a Steps slot that moves on a pass.
        if pending.leads {
            self.tracks[index].chain_pass = place as i64 / owner.chain_steps(chain).max(1) as i64;
        }
        let Some(position) = owner.chain_position(chain, place, self.dealer(index)) else {
            return;
        };
        let Some(step) = owner.step_at(position) else {
            return;
        };
        // A row's later pulses repeat its note only on a `Multi`, and run only the note's own timing.
        let first = position.pulse == 0;
        let gate = step.gate_kind();
        let row_fx = |slot: &FxSlot| {
            first
                || gate == GateKind::Multi
                    && matches!(
                        slot.command,
                        Some(
                            FxCommand::Del
                                | FxCommand::Kil
                                | FxCommand::Off
                                | FxCommand::Ret
                                | FxCommand::Acu
                                | FxCommand::Arp
                        )
                    )
        };
        // Each FX column reads its own lane's step if it has one, and the note's row if not.
        let phrase = owner.phrases.get(&position.phrase);
        let count = pending.count;
        let mut lane_steps = [None; FX_SLOTS];
        let mut fx = [FxSlot::default(); FX_SLOTS];
        for (number, read) in fx.iter_mut().enumerate() {
            let lane = phrase.and_then(|phrase| Some((phrase, phrase.lane(number)?)));
            let found = match lane {
                Some((phrase, lane)) => {
                    let at = lane.step_at(count + self.tracks[index].lane_offsets[number]);
                    lane_steps[number] = Some(at);
                    phrase.steps().get(at).and_then(|step| step.fx.get(number))
                }
                None => step.fx.get(number).filter(|slot| row_fx(slot)),
            };
            *read = found.copied().unwrap_or_default();
        }
        let mut delay_units = None;
        let mut kill_units = None;
        let mut off_units = None;
        let mut retrigger = None;
        let mut accumulates: Option<i8> = None;
        let mut arpeggiates = None;
        // `CHA` and `CND` go first, as whether the note plays decides whether locks end. A row's
        // own are decided once, on its first pulse, by the step's first part to land. Every `CHA`
        // is rolled whatever the conditions say, so its rolls are the same with or without them.
        let decided = match pending.plays {
            Some(decided) => decided,
            None => {
                let mut decided = if first {
                    Decided::PLAYS
                } else {
                    self.tracks[index].row
                };
                let mut chance = None;
                for slot in fx.iter().filter(|s| s.command == Some(FxCommand::Cha)) {
                    let rolled = self.tracks[index].rng.chance(slot.coarse());
                    decided.note &= rolled;
                    chance = Some(chance.unwrap_or(true) && rolled);
                }
                if !self.conditions_hold(index, &fx, &lane_steps, &pending, position, chance) {
                    decided = Decided {
                        note: false,
                        fx: false,
                    };
                }
                if first {
                    self.tracks[index].row = decided;
                }
                if pending.part != Part::Whole {
                    self.tracks[index].decide(pending.step, decided);
                }
                decided
            }
        };
        let plays = decided.note;
        // What the row's note does on this pulse.
        let event = match (step.note, gate) {
            (Note::On(_), GateKind::Rest) if first => Note::Off,
            (Note::On(_), GateKind::Rest) => Note::Empty,
            (Note::On(midi), GateKind::Multi) => Note::On(midi),
            (note, _) if first => note,
            _ => Note::Empty,
        };
        let tying = first && self.tracks[index].tied && self.tracks[index].voice.active();
        // The last note's locks end where the step's first part lands, so a lane's lock on a step
        // whose note is nudged late lasts that note.
        if pending.leads && plays && first && matches!(event, Note::On(_)) && !tying {
            self.tracks[index].end_locks();
        }
        // A lane's values stay where the groove lands the step. What times the note goes with it,
        // wherever it's written. A failed `CND` skips the row's own FX, as Elektron's skips a
        // trig's locks, while a lane keeps to its own loop.
        let runs = |number: usize, slot: &FxSlot| match slot.command {
            Some(
                FxCommand::Del
                | FxCommand::Kil
                | FxCommand::Off
                | FxCommand::Ret
                | FxCommand::Cha
                | FxCommand::Cnd
                | FxCommand::Acu
                | FxCommand::Arp,
            ) => row && decided.fx,
            _ if lane_steps[number].is_some() => pending.part.runs_lanes(),
            _ => row && decided.fx,
        };
        for (number, slot) in fx.iter().enumerate() {
            let Some(command) = slot.command.filter(|_| runs(number, slot)) else {
                continue;
            };
            match command {
                FxCommand::Del => delay_units = Some(slot.coarse()),
                FxCommand::Kil => kill_units = Some(slot.coarse()),
                FxCommand::Off => off_units = Some(slot.coarse()),
                FxCommand::Ret => retrigger = Some(slot.coarse()),
                FxCommand::Cha | FxCommand::Cnd => {}
                // Two on a row add up, as far as an `ACU` goes at once. A value past that is left
                // out, as if it wasn't there.
                FxCommand::Acu => {
                    if let Some(amount) = amount(slot.coarse()) {
                        let most = MOST_DEGREES as i8;
                        let sum = accumulates.unwrap_or(0).saturating_add(amount);
                        accumulates = Some(sum.clamp(-most, most));
                    }
                }
                // The last column's wins, as they run left to right.
                FxCommand::Arp => arpeggiates = Some(slot.coarse()),
                FxCommand::Set(id) if slot.mark == Mark::Glide => {
                    let value = command;
                    let to = phrase.and_then(|phrase| {
                        match lane_steps[number] {
                            Some(at) => phrase.next_in_lane(number, at, value),
                            None => phrase.glide_to(position.visit, position.deal, number, value),
                        }
                        .map(|(steps, to)| (to, steps as f64 * pending.ticks as f64))
                    });
                    let now = self.song_time();
                    self.tracks[index].start_glide(id, slot.value, to, now);
                }
                FxCommand::Set(id) => self.tracks[index].set_value(id, slot.value, slot.mark),
            }
        }
        if !row {
            if !self.tracks[index].sounding() {
                self.tracks[index].settle();
            }
            return;
        }
        let tie = tying && plays && delay_units.is_none() && matches!(event, Note::On(_));
        if plays && delay_units.is_none() && matches!(event, Note::On(_)) && !tie {
            self.start_modulation(index);
        }
        if !self.tracks[index].sounding() {
            self.tracks[index].settle();
        }
        let start_frames = delay_units.map_or(0, |units| self.groove_units(units, pending.ticks));
        let start_at = self.clock + start_frames;
        if first {
            self.tracks[index].tied =
                gate == GateKind::Hold && plays && matches!(event, Note::On(_));
            self.tracks[index].accumulated = None;
        }
        // A row's `ACU` adds once as it plays, or on each pulse of a `MLT` and each hit of a `RET`
        // as the track says. A note that's skipped, or rests, adds nothing.
        let accumulation = owner.accumulation;
        let adds = first || accumulation.adds != Adds::Row;
        let key = RowKey {
            phrase: position.phrase,
            row: position.step,
        };
        let epoch = self.epoch(index, &pending, chain);
        let mut accumulating = None;
        match event {
            Note::Empty => {}
            Note::Off if plays => {
                self.release_modulation(index);
                self.tracks[index].voice.release();
                self.tracks[index].retrigger = None;
                self.tracks[index].arp = None;
            }
            Note::Off => {}
            Note::On(_) if !plays => {}
            Note::On(midi) => {
                let heard = self.accumulate(index, epoch, key, midi, accumulates.filter(|_| adds));
                if accumulation.adds == Adds::Retrig {
                    accumulating = accumulates.map(|amount| Accumulating {
                        row: key,
                        amount,
                        written: midi,
                        transpose: position.transpose,
                        epoch,
                    });
                }
                let note = (i16::from(heard) + i16::from(position.transpose))
                    .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE))
                    as u8;
                let played = (heard, position.transpose);
                let arp = self.arpeggiate(index, &pending, arpeggiates, played, tie, start_at);
                self.tracks[index].arp = arp;
                let note = arp.map_or(note, |arp| arp.cycle.note(arp.place));
                // `VDLT` moves the note's own, after the FX have run so a lock on this step
                // counts, and the groove's velocity for the step scales that.
                let own = self.tracks[index].velocity(step.velocity.unwrap_or(MAX_VELOCITY));
                let own = u32::from(own);
                let percent = u32::from(pending.groove.velocity(running));
                let velocity = if percent == u32::from(FULL_VELOCITY) {
                    own
                } else {
                    (own * percent + 50) / 100
                };
                let velocity = velocity.min(u32::from(MAX_VELOCITY)) as u8;
                let on = self.note_on(index, note, velocity, self.note_shape(index));
                match on {
                    // A `Hold` before it ties into this note, with no new attack.
                    Some(on) if tie => self.tracks[index].tie(on),
                    Some(on) if start_frames > 0 => {
                        self.tracks[index].last_note = Some(on);
                        self.tracks[index].delayed = Some(DelayedNote {
                            at: start_at,
                            note: on,
                        });
                    }
                    Some(on) if self.plays(index) => {
                        self.tracks[index].start(on, self.rate);
                        self.choke(index);
                    }
                    Some(on) => {
                        self.tracks[index].last_note = Some(on);
                        self.tracks[index].cut_to_tail();
                    }
                    None => self.tracks[index].cut_to_tail(),
                }
            }
        }
        // A note's gate closes its length of the way through its last pulse: a `Single`'s is its
        // row's last, a `Multi`'s note is one pulse long, and a `Hold` ties past it into the next
        // row. It's worked out after the FX have run, so a `LEN` or `LDLT` on this step counts,
        // and so does a lane's.
        if plays && matches!(event, Note::On(_)) && gate != GateKind::Hold {
            let pulses = if gate == GateKind::Multi {
                1
            } else {
                step.pulses()
            };
            self.tracks[index].gate_at = self
                .gate_frames(index, pulses, step.length(), &pending)
                .map(|frames| start_at + frames);
        }
        if let Some(units) = kill_units {
            self.tracks[index].kill_at = Some(start_at + self.groove_units(units, pending.ticks));
        }
        if let Some(units) = off_units {
            self.tracks[index].off_at = Some(start_at + self.groove_units(units, pending.ticks));
        }
        if let Some(value) = retrigger {
            let units = value & 0x0F;
            if units > 0 {
                let interval = self.groove_units(units, pending.ticks).max(1);
                self.tracks[index].retrigger = Some(Retrigger {
                    next_at: start_at + interval,
                    interval,
                    factor: retrigger_factor(value >> 4),
                    accumulates: accumulating,
                });
            }
        }
    }

    /// The arpeggio a note starting now plays, from the note as it's heard before its chain's
    /// transpose, which goes on after, an accumulator's move included, so the arpeggio moves with
    /// it. A `HLD` carries the one sounding into the note it ties into, keeping its place and its
    /// time, with that row's own `ARP` if it has one. Any other note starts one of its own, or has
    /// none.
    fn arpeggiate(
        &mut self,
        index: usize,
        pending: &PendingStep,
        value: Option<u8>,
        (heard, transpose): (u8, i8),
        tie: bool,
        start_at: u64,
    ) -> Option<Arpeggiating> {
        let carried = self.tracks[index].arp.filter(|_| tie);
        let value = value.or(carried.map(|arp| arp.value))?;
        let settings = self.current.song.tracks.get(index)?.arpeggio;
        let cycle = Cycle::new(heard, value, settings.octaves, self.scale, transpose);
        if !cycle.moves() {
            return None;
        }
        if let Some(carried) = carried {
            return Some(Arpeggiating {
                cycle,
                value,
                ..carried
            });
        }
        let roll = if settings.mode == ArpMode::Random {
            self.tracks[index].arp_rng.next_u32()
        } else {
            0
        };
        let note_ticks = settings.rate.ticks();
        Some(Arpeggiating {
            cycle,
            value,
            mode: settings.mode,
            retrig: settings.retrig,
            from: start_at,
            played: 1,
            place: cycle.place(settings.mode, 0, roll),
            note_ticks,
            step_ticks: pending.ticks,
            next_at: start_at + self.frames_of(note_ticks, pending.ticks),
        })
    }

    /// Which epoch a step's accumulators count in: the start of its chain it was worked out after,
    /// and with a reset every so many loops, which group of them its count is in. With a reset only
    /// when playing starts, it's always the same one.
    fn epoch(&self, index: usize, pending: &PendingStep, chain: u32) -> Epoch {
        let Some(owner) = self.current.song.tracks.get(index) else {
            return (0, 0);
        };
        match owner.accumulation.reset {
            Reset::Play => (0, 0),
            Reset::Chain => (pending.start, 0),
            Reset::Loops(loops) => {
                let length = owner.chain_steps(chain).max(1) as i64 * i64::from(loops.max(1));
                (pending.start, pending.count.div_euclid(length))
            }
        }
    }

    /// The note a row plays at, before its chain's transpose: the note as written, moved on by the
    /// track's accumulators. Its own `ACU`, if it adds now, adds first, unless this is its first
    /// play since they started again.
    fn accumulate(
        &mut self,
        index: usize,
        epoch: Epoch,
        key: RowKey,
        written: u8,
        amount: Option<i8>,
    ) -> u8 {
        let Some(owner) = self.current.song.tracks.get(index) else {
            return written;
        };
        let settings = owner.accumulation;
        let state = &mut self.tracks[index];
        state.accumulators.enter(epoch);
        if let Some(amount) = amount.filter(|amount| *amount != 0) {
            state.accumulators.add(key, amount, settings);
        }
        let offset = state.accumulators.offset(key, settings.moves);
        let heard = moved_note(written, offset, self.scale);
        state.accumulated = (heard != written).then_some(heard);
        heard
    }

    /// Decide a row's conditions, left to right, as its first pulse lands, and keep what they and
    /// its `CHA` came to for `PRE` here and `NEI` on the track to the right to read next. The row
    /// plays only if every one holds. A value that isn't a condition is left out, as if it wasn't
    /// there.
    fn conditions_hold(
        &mut self,
        index: usize,
        fx: &[FxSlot; FX_SLOTS],
        lane_steps: &[Option<usize>; FX_SLOTS],
        pending: &PendingStep,
        position: ChainPosition,
        chance: Option<bool>,
    ) -> bool {
        let previous = self.tracks[index].passed;
        let neighbour = index
            .checked_sub(1)
            .is_some_and(|left| self.tracks[left].passed);
        let mut holds = true;
        let mut decided = chance;
        for (number, slot) in fx.iter().enumerate() {
            if slot.command != Some(FxCommand::Cnd) {
                continue;
            }
            let Some(condition) = Condition::from_value(slot.coarse()) else {
                continue;
            };
            let pass = match lane_steps[number] {
                Some(_) => self.lane_pass(index, number, pending, position),
                None => self.row_pass(index, pending, position),
            };
            let result = condition.holds(Situation {
                pass,
                fill: self.fill,
                previous,
                neighbour,
            });
            holds &= result;
            if !condition.reads_last() {
                decided = Some(decided.unwrap_or(true) && result);
            }
        }
        if let Some(passed) = decided {
            self.tracks[index].passed = passed;
        }
        holds
    }

    /// How many times the track has played this row before, since its chain started: once each
    /// time round the chain for each time its phrase is in it, so a phrase played four times in
    /// a row counts four passes, and the first is 0.
    fn row_pass(&self, index: usize, pending: &PendingStep, position: ChainPosition) -> i64 {
        let Some((chain, _)) = pending.place else {
            return 0;
        };
        let Some(owner) = self.current.song.tracks.get(index) else {
            return 0;
        };
        let Some(entries) = owner.chains.get(&chain).map(|chain| &chain.entries) else {
            return 0;
        };
        let length = owner.chain_steps(chain).max(1) as i64;
        let times = |entries: &[vt_model::song::ChainEntry]| {
            entries
                .iter()
                .filter(|entry| entry.phrase == position.phrase)
                .count() as i64
        };
        let before = times(entries.get(..position.slot).unwrap_or_default());
        pending.count.div_euclid(length) * times(entries) + before
    }

    /// How many times round its own loop a lane has been since the track's chain started.
    fn lane_pass(
        &self,
        index: usize,
        number: usize,
        pending: &PendingStep,
        position: ChainPosition,
    ) -> i64 {
        let lane = self
            .current
            .song
            .tracks
            .get(index)
            .and_then(|owner| owner.phrases.get(&position.phrase))
            .and_then(|phrase| phrase.lane(number));
        let Some(lane) = lane else {
            return 0;
        };
        let count = pending.count + self.tracks[index].lane_offsets[number];
        count.div_euclid(lane.length.max(1) as i64)
    }

    /// Hear a row while not playing: every audible track's note at this step of the section, with
    /// its velocity and transpose. Whatever an audition was sounding is cut first. Its
    /// FX don't run, and nothing that playback relies on changes.
    pub fn audition(&mut self, section_step: usize) {
        self.audition_section(self.section(), section_step);
    }

    /// Hear a row of a given section while not playing.
    pub fn audition_section(&mut self, section: usize, section_step: usize) {
        self.audition_from(section, section_step, None);
    }

    /// Hear a row of a given section while not playing: a step of the song's, or one of a
    /// track's own, and on every other track the row that's playing then at its own speed.
    pub fn audition_from(&mut self, section: usize, step: usize, track: Option<usize>) {
        if self.transport == Transport::Playing {
            return;
        }
        let tick = step as u64 * track.map_or(TICKS_PER_STEP, |track| self.speed_ticks(track));
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some(section) = song.sections.get(section) else {
            return;
        };
        self.auditioned = [false; MAX_TRACKS];
        for index in 0..song.tracks.len().min(self.tracks.len()) {
            self.tracks[index].cut_to_tail();
            if !self.plays(index) {
                continue;
            }
            let Some(Some(chain)) = section.chains.get(index) else {
                continue;
            };
            let owner = &song.tracks[index];
            // A track with a reset of its own has started its chain again since the section's.
            let tick = owner.reset.map_or(tick, |steps| {
                tick % (u64::from(steps.max(1)) * TICKS_PER_STEP)
            });
            let place = (tick / owner.speed.ticks()) as usize;
            let Some(position) = owner.chain_position(*chain, place, self.dealer(index)) else {
                continue;
            };
            let Some(step) = owner.step_at(position) else {
                continue;
            };
            let Note::On(midi) = step.note else {
                continue;
            };
            let note = (i16::from(midi) + i16::from(position.transpose))
                .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
            let velocity = self.tracks[index].velocity(step.velocity.unwrap_or(MAX_VELOCITY));
            let shape = self.note_shape(index);
            let Some(on) = self.note_on(index, note, velocity, shape) else {
                continue;
            };
            let track = &mut self.tracks[index];
            if !track.sounding() {
                track.settle();
            }
            let last_note = track.last_note;
            track.start(on, self.rate);
            track.last_note = last_note;
            self.auditioned[index] = true;
            self.choke(index);
        }
    }

    /// The audition's key was let go. After a hold, every note it started is let go. After a tap,
    /// a sample plays out, and a synth's note that would otherwise sound until something ends it
    /// is let go once it has sounded as long as a preview does. Once playing has started, the
    /// tracks are playback's and are left alone.
    pub fn end_audition(&mut self, held: bool) {
        let auditioned = std::mem::replace(&mut self.auditioned, [false; MAX_TRACKS]);
        if self.transport == Transport::Playing {
            return;
        }
        let preview = PREVIEW_MS * self.rate / 1000;
        for (track, _) in self
            .tracks
            .iter_mut()
            .zip(auditioned)
            .filter(|(_, auditioned)| *auditioned)
        {
            if held {
                track.voice.release();
            } else if track.voice.holds() {
                track.voice.release_once_sounded(preview);
            }
        }
    }

    /// Render voices and effects into a run of frames with no events in it.
    fn render(&mut self, out: &mut [f32]) {
        self.stopwatch.lap(TimedPart::Sequencing);
        let samples = Arc::clone(&self.current.samples);
        let limiter = self.current.song.limiter;
        let groups = self.sounds();
        let return_a = self.returns();
        let (return_b, master_track) = (return_a + 1, return_a + 2);
        let wet_only = self.return_soloed();
        self.keys.sounds = groups;
        // A note or a preview only starts between runs, and a skipped track doesn't change, so a
        // track that's silent at the start of this one stays silent through it and is only asked
        // once. The others are asked each frame, as one can fall silent part way.
        // The fixed tracks have their turn after the rest: the groups with what their tracks play,
        // then the returns with what's sent to them.
        // The preview voices sounding are found once too, so with none there's nothing to ask.
        let previews = self.previews.sounding();
        let mut playing = [0_usize; MAX_TRACKS];
        // Where each playing track goes: its group's place among the groups, or the master.
        let mut routes = [None; MAX_TRACKS];
        // While a return is soloed, a group is heard only if it or one of its tracks is soloed.
        let mut lit = [false; MAX_GROUPS];
        for (group, lit) in lit.iter_mut().enumerate() {
            *lit = self.tracks[groups + group].solo;
        }
        let mut count = 0;
        for index in 0..self.tracks.len() {
            if (groups..=master_track).contains(&index) {
                continue;
            }
            let route = self.group_of(index).map(|group| group - groups);
            let track = &self.tracks[index];
            if let Some(group) = route
                && track.solo
            {
                lit[group] = true;
            }
            let previewing = self.previews.on_track(&previews, index);
            if !track.sounding() && !previewing && !track.ringing(self.rate) {
                self.keys.frames[index] = [0.0; 2];
                // Silent, a mute's fade has nothing to fade, so a note it plays later while a
                // compressor hears it doesn't start with the end of one.
                let track = &mut self.tracks[index];
                track.heard.settle();
                track.feeds.settle();
            } else {
                playing[count] = index;
                routes[count] = route;
                count += 1;
            }
        }
        for frame in out.as_chunks_mut::<2>().0 {
            self.stopwatch.lap(TimedPart::Mixing);
            let mut mix = [0.0_f32; 2];
            let mut buses = [[0.0_f32; 2]; MAX_GROUPS];
            let mut send_a = [0.0_f32; 2];
            let mut send_b = [0.0_f32; 2];
            for (&index, &route) in playing[..count].iter().zip(&routes[..count]) {
                let track = &mut self.tracks[index];
                let preview = self.previews.next(&previews, index, &samples, &self.files);
                if !track.sounding() && preview.is_none() && !track.ringing(self.rate) {
                    self.keys.frames[index] = [0.0; 2];
                    self.stopwatch.lap(TimedPart::Voice(index));
                    continue;
                }
                let main = track.voice.next(samples.get(track.voice.sample()));
                let tail = track.tail.next(samples.get(track.tail.sample()));
                self.stopwatch.lap(TimedPart::Voice(index));
                let preview = preview.unwrap_or([0.0; 2]);
                let shaped = track.shape(
                    [
                        main[0] + tail[0] + preview[0],
                        main[1] + tail[1] + preview[1],
                    ],
                    self.smoothing,
                    self.fast_smoothing,
                    self.rate,
                    &self.keys,
                );
                self.keys.frames[index] = track.key;
                // Muted but heard by a compressor or Warps, it plays on, faded out of the mix and
                // sends.
                let heard = track.heard.next(self.smoothing);
                let feeds = track.feeds.next(self.smoothing);
                let dry = shaped.dry.map(|sample| sample * heard);
                track.reading.add(dry, dry);
                // A track in a group always goes to it, and the group decides what's heard. What
                // it gives the group may be more than what's heard of it, while the group's heard
                // by a compressor and is silent itself.
                match route {
                    Some(group) => {
                        buses[group][0] += shaped.dry[0] * feeds;
                        buses[group][1] += shaped.dry[1] * feeds;
                    }
                    None if !wet_only || track.solo => {
                        mix[0] += dry[0];
                        mix[1] += dry[1];
                    }
                    None => {}
                }
                send_a[0] += shaped.send_a[0] * heard;
                send_a[1] += shaped.send_a[1] * heard;
                send_b[0] += shaped.send_b[0] * heard;
                send_b[1] += shaped.send_b[1] * heard;
                self.stopwatch.lap(TimedPart::Shaping(index));
            }
            let (smoothing, fast, rate) = (self.smoothing, self.fast_smoothing, self.rate);
            let keys = &mut self.keys;
            for (group, bus) in buses.into_iter().enumerate() {
                let index = groups + group;
                let shaped = self.tracks[index].process_group(bus, smoothing, fast, rate, keys);
                keys.frames[index] = self.tracks[index].key;
                if !wet_only || lit[group] {
                    mix[0] += shaped.dry[0];
                    mix[1] += shaped.dry[1];
                }
                send_a[0] += shaped.send_a[0];
                send_a[1] += shaped.send_a[1];
                send_b[0] += shaped.send_b[0];
                send_b[1] += shaped.send_b[1];
                self.stopwatch.lap(TimedPart::Shaping(index));
            }
            let from_a = self.tracks[return_a].process_return(send_a, smoothing, fast, rate, keys);
            keys.frames[return_a] = self.tracks[return_a].key;
            self.stopwatch.lap(TimedPart::Shaping(return_a));
            let from_b = self.tracks[return_b].process_return(send_b, smoothing, fast, rate, keys);
            keys.frames[return_b] = self.tracks[return_b].key;
            self.stopwatch.lap(TimedPart::Shaping(return_b));
            let mixed = [
                mix[0] + from_a[0] + from_b[0],
                mix[1] + from_a[1] + from_b[1],
            ];
            let mastered =
                self.tracks[master_track].process_master(mixed, smoothing, fast, rate, &self.keys);
            self.stopwatch.lap(TimedPart::Shaping(master_track));
            let out = master(mastered, limiter);
            self.tracks[master_track].reading.add(mastered, out);
            self.reading.add(mastered, out);
            *frame = out;
            self.stopwatch.lap(TimedPart::Limiting);
        }
        self.stopwatch.lap(TimedPart::Mixing);
    }
}

/// The span a voice's slice of its entry plays now, if the entry still plays the voice's sample.
/// A slice gone since it started plays the last, as `SLI` past the last does.
fn span_now(
    bank: &Bank,
    entry: Option<u8>,
    (slice, thru): (u8, bool),
    sample: SampleId,
) -> Option<Span> {
    let playable = bank.get(usize::from(entry?))?.as_deref()?;
    (playable.sample == sample).then(|| playable.span(slice, thru))
}

/// Have a voice follow its entry's changes.
fn follow_voice(voice: &mut Voice, bank: &Bank) {
    if let Some(span) = span_now(bank, voice.entry(), voice.slice(), voice.sample()) {
        voice.respan(span);
    }
}

/// Have a note waiting to start follow its entry's changes.
fn follow_note(note: &mut NoteOn, bank: &Bank) {
    let slice = (note.slice, note.cut.mode == PlayMode::Thru);
    if let Some(span) = span_now(bank, note.entry, slice, note.sample) {
        note.span = Some(span);
    }
}

/// The gain of a step's velocity.
fn velocity_gain(velocity: u8) -> f32 {
    f32::from(velocity) / f32::from(MAX_VELOCITY)
}

/// How much `RET`'s first digit changes the level on each retrigger. `0` is no change, 1-7 are
/// 10% to 70% quieter, and 8-F are 10% to 80% louder, compounding. `Acid` ignores it.
fn retrigger_factor(digit: u8) -> f32 {
    match digit {
        0 => 1.0,
        1..=7 => 1.0 - f32::from(digit) * 0.1,
        _ => 1.0 + f32::from(digit - 7) * 0.1,
    }
}
