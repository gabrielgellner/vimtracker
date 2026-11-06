//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::constants::{
    GROOVE_UNIT_TICKS, MAX_MILLIBPM, MAX_SWING, MAX_TRACKS, MIN_MILLIBPM, MIN_SWING, PREVIEW_MS,
    TICKS_PER_QUARTER, TICKS_PER_STEP,
};
use vt_model::envelope::Envelope;
use vt_model::lfo::Destination;
use vt_model::song::{Launch, Section, SectionStart, Song, Track, level_gain};
use vt_model::step::{FxCommand, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Mark, Note};

use crate::dsp::delay::Delay;
use crate::dsp::reverb::Reverb;
use crate::dsp::smoother::{coefficient, fast_coefficient};
use crate::link::JamState;
use crate::mixer::{Reading, master};
use crate::published::Published;
use crate::track_state::{DelayedNote, Retrigger, SongTime, TrackState};
use crate::voice::{NoteOn, Voice};

/// Delay time: 3/16 of a note, in ticks.
const DELAY_TICKS: u64 = TICKS_PER_QUARTER * 3 / 4;
/// How often LFOs are worked out, in frames. Their values are smoothed in between.
const LFO_UPDATE_FRAMES: u32 = 32;

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

/// How many ticks swing delays odd steps by, for a swing in tenths of a percent.
pub fn swing_ticks(swing: u16) -> u64 {
    let swing = u64::from(swing.clamp(MIN_SWING, MAX_SWING));
    (TICKS_PER_STEP * 2 * swing + 500) / 1000 - TICKS_PER_STEP
}

/// Sequencer and voices. It owns all its state ahead of time, and `process` doesn't allocate.
pub struct Engine {
    rate: u32,
    current: Arc<Published>,
    tracks: Box<[TrackState; MAX_TRACKS]>,
    preview: Voice,
    /// The track the preview voice goes through.
    preview_track: usize,
    transport: Transport,
    /// The section playing, and how many steps of it have fired.
    section: usize,
    section_place: u64,
    /// A section to keep playing rather than moving on.
    loop_section: Option<usize>,
    /// The next step to fire, for timing. It never wraps.
    next_step: u64,
    /// Where the next step falls for the tracks, which each loop at their own length. Separate from
    /// `next_step` so a reset moves the tracks but not the beat.
    place: u64,
    /// The tick the tempo began at.
    origin_tick: u64,
    /// Frames played since `origin_tick`.
    frames_since_origin: u64,
    /// Frames played in total, for events on a track's own schedule.
    clock: u64,
    millibpm: u32,
    smoothing: f32,
    fast_smoothing: f32,
    delay: Delay,
    reverb: Reverb,
    /// The master's levels since the last report.
    reading: Reading,
    /// Frames until LFOs are next worked out.
    lfo_countdown: u32,
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
        let delay_frames = sample_of_tick(DELAY_TICKS, rate, MIN_MILLIBPM) as usize + 1;
        let mut engine = Self {
            rate,
            current,
            tracks: Box::new(std::array::from_fn(|index| TrackState::new(seed, index))),
            preview: Voice::default(),
            preview_track: 0,
            transport: Transport::Stopped,
            section: 0,
            section_place: 0,
            loop_section: None,
            next_step: 0,
            place: 0,
            origin_tick: 0,
            frames_since_origin: 0,
            clock: 0,
            millibpm,
            smoothing: coefficient(rate),
            fast_smoothing: fast_coefficient(rate),
            delay: Delay::new(delay_frames),
            reverb: Reverb::new(rate),
            reading: Reading::default(),
            lfo_countdown: 0,
        };
        engine.follow_levels();
        engine.reset_tracks();
        for track in engine.tracks.iter_mut() {
            track.settle();
        }
        engine
    }

    /// Move each track's level toward the current version's.
    fn follow_levels(&mut self) {
        for (state, track) in self.tracks.iter_mut().zip(&self.current.song.tracks) {
            state.level.target = level_gain(f32::from(track.level));
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
        let place = self.place.saturating_sub(u64::from(fired));
        if self.past_reset(place) {
            0
        } else {
            place as usize
        }
    }

    /// Whether a place is one the section's reset would already have sent back to the start.
    fn past_reset(&self, place: u64) -> bool {
        let reset = self.playing_section().and_then(|s| s.reset);
        reset.is_some_and(|reset| place >= reset as u64)
    }

    /// Send every track back to its start from the next step, leaving the beat where it is.
    pub fn realign(&mut self) {
        self.place = 0;
        for track in self.tracks.iter_mut() {
            track.started = 0;
        }
    }

    /// Bring a section in: a track given a different chain starts it from its top.
    fn start_row(&mut self, section: &Section) {
        let place = self.place as i64;
        for (track, chain) in self.tracks.iter_mut().zip(&section.chains) {
            if track.playing != *chain {
                track.playing = *chain;
                track.started = place;
            }
        }
    }

    /// Work out the running count and each track's start from the rows before this one, as if the
    /// song had played to this step of it, and give the step within the section.
    fn walk_to(&mut self, section: usize, step: usize) -> u64 {
        self.realign();
        for track in self.tracks.iter_mut() {
            track.playing = None;
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
        for (at, row) in rows.iter().enumerate().skip(first) {
            let length = song.section_length(row) as u64;
            if length == 0 {
                continue;
            }
            self.start_row(row);
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
                self.realign();
            }
            self.place = place;
        }
        within
    }

    /// Put a track's chain at this step, leaving the others where they are.
    pub fn place_track(&mut self, track: usize, step: usize) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.started = self.place as i64 - step as i64;
        }
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
        self.loop_section = Some(section);
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

    /// The chain a track is on, and how far into it a place falls.
    fn track_place(&self, index: usize, section_step: usize) -> Option<(u32, usize)> {
        let chain = self.track_chain(index)?;
        let length = self.current.song.tracks.get(index)?.chain_steps(chain) as i64;
        let place = section_step as i64 - self.tracks[index].started;
        Some((chain, place.rem_euclid(length.max(1)) as usize))
    }

    /// Start what a track has cued, if this is a step its launch timing starts one on.
    fn start_cued(&mut self, index: usize, section_step: usize) {
        if self.tracks[index].cued.is_none() {
            return;
        }
        let due = match self.current.song.launch {
            Launch::Every(steps) => {
                // Each repeat of a section starts the grid again.
                let length = self
                    .playing_section()
                    .map_or(0, |s| self.current.song.section_length(s));
                (self.section_place % (length as u64).max(1)).is_multiple_of(u64::from(steps.get()))
            }
            Launch::ChainEnd => {
                self.track_place(index, section_step)
                    .is_none_or(|(current, place)| {
                        let length = self.current.song.tracks[index].chain_steps(current);
                        length == 0 || place.is_multiple_of(length)
                    })
            }
        };
        if due {
            self.begin_cued(index, section_step);
        }
    }

    /// Start what a track has cued now, if it has anything cued.
    fn begin_cued(&mut self, index: usize, section_step: usize) {
        let Some(chain) = self.tracks[index].cued.take() else {
            return;
        };
        if chain.is_none() {
            self.tracks[index].stop();
        }
        self.tracks[index].launched = Some(chain);
        self.tracks[index].started = section_step as i64;
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

    /// Keep playing this section rather than moving on, or go back to playing the song.
    pub fn set_loop_section(&mut self, section: Option<usize>) {
        self.loop_section = section;
    }

    /// Go back to the first section's start without touching anything sounding, for a render.
    pub fn rewind(&mut self) {
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
        let from = self.section;
        let (next, wrapped) = match self.loop_section {
            Some(section) if section < count => (section, false),
            _ => next_row(&song, from),
        };
        self.section = next;
        self.section_place = 0;
        let section = &song.sections[next];
        if song.section_length(section) == 0 {
            return false;
        }
        if next != from {
            self.end_launches();
        }
        if section.start == SectionStart::Reset {
            self.realign();
        } else if next != from && wrapped {
            let place = self.place as i64;
            for track in self.tracks.iter_mut() {
                track.started = place;
            }
        } else if next != from {
            self.start_row(section);
        }
        // A cue left waiting isn't lost to the song moving on.
        if next != from {
            for index in 0..self.tracks.len() {
                self.begin_cued(index, self.place as usize);
            }
        }
        true
    }

    /// Switch to a new version, returning the old one so it can be freed elsewhere.
    ///
    /// A shorter phrase wraps by the modulo in sequencing. An ID that no longer exists is silent,
    /// and a removed track's voices fade.
    pub fn swap(&mut self, version: Arc<Published>) -> Arc<Published> {
        let old = std::mem::replace(&mut self.current, version);
        self.follow_base(&old);
        for track in self.tracks.iter_mut().skip(self.current.song.tracks.len()) {
            track.stop();
            track.launched = None;
            track.cued = None;
        }
        self.follow_levels();
        self.retune();
        old
    }

    /// Hear a change to a track's page straight away. A held value a step has set since stays
    /// until something changes it, so this only acts on what actually changed.
    fn follow_base(&mut self, old: &Published) {
        let fresh = Track::default();
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            let before = old.song.tracks.get(index).unwrap_or(&fresh);
            let controls = [
                (FxCommand::Cut, before.cutoff, track.cutoff),
                (FxCommand::Res, before.resonance, track.resonance),
                (FxCommand::Pan, before.pan, track.pan),
                (FxCommand::Dly, before.delay, track.delay),
                (FxCommand::Rev, before.reverb, track.reverb),
            ];
            for (command, was, now) in controls {
                if was != now {
                    state.set_base(command, now);
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
            for start in &mut track.lfo_starts {
                start.tick = 0.0;
            }
        }
        self.update_lfos();
    }

    /// Where the song is now, worked out from the tempo's origin so rounding can't build up.
    fn song_time(&self) -> SongTime {
        let ticks_per_frame = TICKS_PER_QUARTER as f64 * f64::from(self.millibpm)
            / (60.0 * f64::from(self.rate) * 1000.0);
        SongTime {
            tick: self.origin_tick as f64 + self.frames_since_origin as f64 * ticks_per_frame,
            clock: self.clock,
            rate: self.rate,
            seed: self.current.song.seed,
        }
    }

    fn update_lfos(&mut self) {
        let now = self.song_time();
        let stopped = self.transport == Transport::Stopped;
        let tracks = self.tracks.iter_mut().enumerate();
        for ((index, state), track) in tracks.zip(&self.current.song.tracks) {
            state.modulate(track, index, now, stopped);
        }
    }

    /// Start a note's modulation: the track's envelopes, and LFOs that follow its notes.
    fn start_modulation(&mut self, index: usize) {
        let now = self.song_time();
        let Some(track) = self.current.song.tracks.get(index) else {
            return;
        };
        let state = &mut self.tracks[index];
        state.trigger_envelopes(track, now);
        for (lfo, start) in track.lfos.iter().zip(&mut state.lfo_starts) {
            if lfo.retrig {
                start.tick = now.tick;
                start.clock = now.clock;
            }
        }
        state.modulate(track, index, now, false);
    }

    /// Begin the release of a track's envelopes, when its note ends.
    fn release_modulation(&mut self, index: usize) {
        let now = self.song_time();
        if let Some(track) = self.current.song.tracks.get(index) {
            self.tracks[index].release_envelopes(track, now);
        }
    }

    /// The shape of a track's notes: its first envelope if that's on `Vol`.
    fn note_shape(&self, index: usize) -> Envelope {
        self.current
            .song
            .tracks
            .get(index)
            .map(|track| track.envelopes[0])
            .filter(|envelope| envelope.destination == Some(Destination::Vol))
            .unwrap_or_default()
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
        self.reset_tracks();
    }

    fn reset_tracks(&mut self) {
        let seed = self.current.song.seed;
        let fresh = Track::default();
        let tracks = &self.current.song.tracks;
        for (index, state) in self.tracks.iter_mut().enumerate() {
            let track = tracks.get(index).unwrap_or(&fresh);
            state.reset_held(seed, index, track, self.clock);
        }
    }

    /// Stop sequencing but let everything sounding finish on its own, for a render's tail.
    pub fn let_ring(&mut self) {
        self.transport = Transport::Stopped;
    }

    /// Stop moving. Voices fade so stopping doesn't click, and effect tails keep ringing.
    fn halt(&mut self, to: Transport) {
        for track in self.tracks.iter_mut() {
            track.stop();
        }
        self.transport = to;
    }

    /// Set whether a track is muted. A muted track's sounding note fades.
    pub fn set_mute(&mut self, track: usize, mute: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute = mute;
        }
        self.apply_audibility();
    }

    /// Set whether a track is soloed. While any track is soloed, only soloed tracks sound.
    pub fn set_solo(&mut self, track: usize, solo: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo = solo;
        }
        self.apply_audibility();
    }

    fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    fn apply_audibility(&mut self) {
        let any_solo = self.any_solo();
        for track in self.tracks.iter_mut() {
            if track.mute || (any_solo && !track.solo) {
                track.cut_to_tail();
            }
        }
    }

    /// Play a note on the preview voice. It never cuts a sequenced note, or is cut by one.
    pub fn preview(&mut self, track: usize, note: u8, velocity: u8) {
        let gain = velocity_gain(velocity);
        let shape = self.note_shape(track);
        let Some(on) = self.note_on(track, note, gain, shape) else {
            return;
        };
        let frames = PREVIEW_MS * self.rate / 1000;
        if let Some(state) = self.tracks.get_mut(track)
            && !state.sounding()
        {
            state.settle();
        }
        self.preview_track = track;
        self.preview = Voice::start(
            NoteOn {
                release_after: Some(frames),
                ..on
            },
            self.rate,
            0.0,
            1.0,
        );
    }

    fn note_on(&self, track: usize, note: u8, step_gain: f32, shape: Envelope) -> Option<NoteOn> {
        let sample = (*self.current.track_samples.get(track)?)?;
        Some(NoteOn {
            sample,
            note,
            gain: step_gain,
            shape,
            release_after: None,
        })
    }

    /// Fill interleaved stereo output. This doesn't allocate, lock or print.
    /// The master's levels since this was last asked.
    pub fn take_reading(&mut self) -> Reading {
        std::mem::take(&mut self.reading)
    }

    pub fn process(&mut self, out: &mut [f32]) {
        let mut written = 0;
        let total = out.len() / 2;
        while written < total {
            let playing = self.transport == Transport::Playing;
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
            } else {
                u64::MAX
            };
            let run = until_event
                .min((total - written) as u64)
                .min(u64::from(self.lfo_countdown))
                .max(1) as usize;
            self.lfo_countdown -= run as u32;
            self.render(&mut out[written * 2..(written + run) * 2]);
            if playing {
                self.frames_since_origin += run as u64;
                self.clock += run as u64;
            }
            written += run;
        }
    }

    /// Frames until the next step fires.
    fn frames_until_next_step(&self) -> u64 {
        if !self.has_steps() {
            return u64::MAX;
        }
        let tick = (self.next_step * TICKS_PER_STEP).saturating_sub(self.origin_tick);
        sample_of_tick(tick, self.rate, self.millibpm).saturating_sub(self.frames_since_origin)
    }

    /// Frames until a track's delayed, cut, release or retrigger event.
    fn frames_until_track_event(&self) -> u64 {
        let mut soonest = u64::MAX;
        for track in self.tracks.iter() {
            let events = [
                track.swung.map(|(at, _)| at),
                track.delayed.map(|d| d.at),
                track.kill_at,
                track.off_at,
                track.retrigger.map(|r| r.next_at),
            ];
            for at in events.into_iter().flatten() {
                soonest = soonest.min(at.saturating_sub(self.clock));
            }
        }
        soonest
    }

    fn groove_units(&self, units: u8) -> u64 {
        sample_of_tick(
            u64::from(units) * GROOVE_UNIT_TICKS,
            self.rate,
            self.millibpm,
        )
    }

    fn audible(&self, index: usize) -> bool {
        let state = &self.tracks[index];
        !state.mute && (!self.any_solo() || state.solo)
    }

    /// Run anything due now on each track's own schedule.
    fn fire_due_events(&mut self) {
        let clock = self.clock;
        for index in 0..MAX_TRACKS {
            if let Some((at, section_step)) = self.tracks[index].swung
                && at <= clock
            {
                self.tracks[index].swung = None;
                self.fire_track(index, section_step);
            }
            if let Some(delayed) = self.tracks[index].delayed
                && delayed.at <= clock
            {
                self.tracks[index].delayed = None;
                self.start_modulation(index);
                if self.audible(index) {
                    let (kill, off, retrigger) = {
                        let t = &self.tracks[index];
                        (t.kill_at, t.off_at, t.retrigger)
                    };
                    self.tracks[index].start(delayed.note, self.rate);
                    let t = &mut self.tracks[index];
                    t.kill_at = kill;
                    t.off_at = off;
                    t.retrigger = retrigger;
                }
            }
            if self.tracks[index].kill_at.is_some_and(|at| at <= clock) {
                self.tracks[index].kill_at = None;
                self.release_modulation(index);
                self.tracks[index].cut_to_tail();
            }
            if self.tracks[index].off_at.is_some_and(|at| at <= clock) {
                self.tracks[index].off_at = None;
                self.release_modulation(index);
                self.tracks[index].voice.release();
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
        let audible = self.audible(index);
        if self.tracks[index].last_note.is_some() {
            self.start_modulation(index);
        }
        let track = &mut self.tracks[index];
        let Some(note) = track.last_note else {
            track.retrigger = None;
            return;
        };
        let gain = (note.gain * retrigger.factor).clamp(0.0, 1.0);
        let next = NoteOn { gain, ..note };
        let (kill, off) = (track.kill_at, track.off_at);
        if audible {
            track.start(next, rate);
        }
        track.kill_at = kill;
        track.off_at = off;
        track.last_note = Some(next);
        track.retrigger = Some(Retrigger {
            next_at: retrigger.next_at + retrigger.interval,
            ..retrigger
        });
    }

    /// Fire every track for the next step.
    fn fire_step(&mut self) {
        if !self.enter_section_if_due() {
            return;
        }
        if self.past_reset(self.place) {
            self.realign();
        }
        let section_step = self.place as usize;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        for index in 0..tracks {
            // A big tempo jump can bring this step before the last one's swing delay ended.
            if let Some((_, pending)) = self.tracks[index].swung.take() {
                self.fire_track(index, pending);
            }
            self.start_cued(index, section_step);
            self.tracks[index].playing = self.track_chain(index);
            let delay = self.swing_frames(index, section_step);
            if delay > 0 {
                self.tracks[index].swung = Some((self.clock + delay, section_step));
            } else {
                self.fire_track(index, section_step);
            }
        }
    }

    /// How long a track's step waits for swing. Pairs count from the start or the last reset, not
    /// from each track's own loop, so an odd length stays on the same grid as everything else.
    fn swing_frames(&self, index: usize, section_step: usize) -> u64 {
        if section_step.is_multiple_of(2) {
            return 0;
        }
        let song = &self.current.song;
        let swing = song.tracks[index].swing.unwrap_or(song.swing);
        sample_of_tick(swing_ticks(swing), self.rate, self.millibpm)
    }

    /// Start or stop a track's note for a step, and run its FX left to right.
    fn fire_track(&mut self, index: usize, section_step: usize) {
        self.tracks[index].retrigger = None;
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some((chain, place)) = self.track_place(index, section_step) else {
            return;
        };
        let Some(owner) = song.tracks.get(index) else {
            return;
        };
        let Some(position) = owner.chain_position(chain, place) else {
            return;
        };
        let Some(step) = owner.step_at(position) else {
            return;
        };
        let mut delay_units = None;
        let mut kill_units = None;
        let mut off_units = None;
        let mut retrigger = None;
        // `CHA` goes first, as whether the note plays decides whether locks end.
        let mut plays = true;
        for slot in step.fx.iter().filter(|s| s.command == Some(FxCommand::Cha)) {
            plays &= self.tracks[index].rng.chance(slot.value);
        }
        if plays && matches!(step.note, Note::On(_)) {
            self.tracks[index].end_locks();
        }
        for (number, slot) in step.fx.iter().enumerate() {
            let Some(command) = slot.command else {
                continue;
            };
            match command {
                FxCommand::Del => delay_units = Some(slot.value),
                FxCommand::Kil => kill_units = Some(slot.value),
                FxCommand::Off => off_units = Some(slot.value),
                FxCommand::Ret => retrigger = Some(slot.value),
                FxCommand::Cha => {}
                value if slot.mark == Mark::Glide => {
                    let to = owner.phrases.get(&position.phrase).and_then(|phrase| {
                        phrase
                            .next_value(position.step, number, value)
                            .map(|(steps, to)| (to, steps as f64 * TICKS_PER_STEP as f64))
                    });
                    let now = self.song_time();
                    self.tracks[index].start_glide(value, slot.value, to, now);
                }
                value => self.tracks[index].set_value(value, slot.value, slot.mark),
            }
        }
        if plays && delay_units.is_none() && matches!(step.note, Note::On(_)) {
            self.start_modulation(index);
        }
        if !self.tracks[index].sounding() {
            self.tracks[index].settle();
        }
        let start_frames = delay_units.map_or(0, |units| self.groove_units(units));
        let start_at = self.clock + start_frames;
        match step.note {
            Note::Empty => {}
            Note::Off if plays => {
                self.release_modulation(index);
                self.tracks[index].voice.release();
                self.tracks[index].retrigger = None;
            }
            Note::Off => {}
            Note::On(_) if !plays => {}
            Note::On(midi) => {
                let note = (i16::from(midi) + i16::from(position.transpose))
                    .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE))
                    as u8;
                let gain = step.velocity.map_or(1.0, velocity_gain);
                let on = self.note_on(index, note, gain, self.note_shape(index));
                match on {
                    Some(on) if start_frames > 0 => {
                        self.tracks[index].last_note = Some(on);
                        self.tracks[index].delayed = Some(DelayedNote {
                            at: start_at,
                            note: on,
                        });
                    }
                    Some(on) if self.audible(index) => self.tracks[index].start(on, self.rate),
                    Some(on) => {
                        self.tracks[index].last_note = Some(on);
                        self.tracks[index].cut_to_tail();
                    }
                    None => self.tracks[index].cut_to_tail(),
                }
            }
        }
        if let Some(units) = kill_units {
            self.tracks[index].kill_at = Some(start_at + self.groove_units(units));
        }
        if let Some(units) = off_units {
            self.tracks[index].off_at = Some(start_at + self.groove_units(units));
        }
        if let Some(value) = retrigger {
            let units = value & 0x0F;
            if units > 0 {
                let interval = self.groove_units(units).max(1);
                self.tracks[index].retrigger = Some(Retrigger {
                    next_at: start_at + interval,
                    interval,
                    factor: retrigger_factor(value >> 4),
                });
            }
        }
    }

    /// Hear a row while not playing: every audible track's note at this step of the section, with
    /// its velocity and transpose. Whatever an audition was sounding is cut first. Its
    /// FX don't run, and nothing that playback relies on changes.
    pub fn audition(&mut self, section_step: usize) {
        self.audition_section(self.section(), section_step);
    }

    /// Hear a row of a given section while not playing.
    pub fn audition_section(&mut self, section: usize, section_step: usize) {
        if self.transport == Transport::Playing {
            return;
        }
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some(section) = song.sections.get(section) else {
            return;
        };
        for index in 0..song.tracks.len().min(self.tracks.len()) {
            self.tracks[index].cut_to_tail();
            if !self.audible(index) {
                continue;
            }
            let Some(Some(chain)) = section.chains.get(index) else {
                continue;
            };
            let owner = &song.tracks[index];
            let Some(position) = owner.chain_position(*chain, section_step) else {
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
            let gain = step.velocity.map_or(1.0, velocity_gain);
            let shape = self.note_shape(index);
            let Some(on) = self.note_on(index, note, gain, shape) else {
                continue;
            };
            let track = &mut self.tracks[index];
            if !track.sounding() {
                track.settle();
            }
            let last_note = track.last_note;
            track.start(on, self.rate);
            track.last_note = last_note;
        }
    }

    /// Render voices and effects into a run of frames with no events in it.
    fn render(&mut self, out: &mut [f32]) {
        let samples = Arc::clone(&self.current.samples);
        let delay_frames = sample_of_tick(DELAY_TICKS, self.rate, self.millibpm) as usize;
        let limiter = self.current.song.limiter;
        for frame in out.as_chunks_mut::<2>().0 {
            let mut mix = [0.0_f32; 2];
            let mut delay_in = [0.0_f32; 2];
            let mut reverb_in = [0.0_f32; 2];
            for (index, track) in self.tracks.iter_mut().enumerate() {
                let previewing = index == self.preview_track && self.preview.active();
                if !track.sounding() && !previewing {
                    continue;
                }
                let main = track.voice.next(samples.get(track.voice.sample()));
                let tail = track.tail.next(samples.get(track.tail.sample()));
                let preview = if previewing {
                    self.preview.next(samples.get(self.preview.sample()))
                } else {
                    [0.0; 2]
                };
                let shaped = track.shape(
                    [
                        main[0] + tail[0] + preview[0],
                        main[1] + tail[1] + preview[1],
                    ],
                    self.smoothing,
                    self.fast_smoothing,
                    self.rate,
                );
                mix[0] += shaped.dry[0];
                mix[1] += shaped.dry[1];
                delay_in[0] += shaped.delay[0];
                delay_in[1] += shaped.delay[1];
                reverb_in[0] += shaped.reverb[0];
                reverb_in[1] += shaped.reverb[1];
            }
            let delayed = self.delay.process(delay_in, delay_frames);
            let reverbed = self.reverb.process(reverb_in);
            let mixed = [
                mix[0] + delayed[0] + reverbed[0],
                mix[1] + delayed[1] + reverbed[1],
            ];
            let out = master(mixed, limiter);
            self.reading.add(mixed, out);
            *frame = out;
        }
    }
}

/// The gain of a step's velocity.
fn velocity_gain(velocity: u8) -> f32 {
    f32::from(velocity) / f32::from(MAX_VELOCITY)
}

/// How much `RET`'s first digit changes the level on each retrigger. `0` is no change, 1-7 are
/// 10% to 70% quieter, and 8-F are 10% to 80% louder, compounding.
fn retrigger_factor(digit: u8) -> f32 {
    match digit {
        0 => 1.0,
        1..=7 => 1.0 - f32::from(digit) * 0.1,
        _ => 1.0 + f32::from(digit - 7) * 0.1,
    }
}
