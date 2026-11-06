//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::constants::{
    GROOVE_UNIT_TICKS, MAX_MILLIBPM, MAX_SWING, MAX_TRACKS, MIN_MILLIBPM, MIN_SWING, PREVIEW_MS,
    TICKS_PER_QUARTER, TICKS_PER_STEP,
};
use vt_model::song::{Track, level_gain};
use vt_model::step::{FxCommand, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note};

use crate::dsp::delay::Delay;
use crate::dsp::reverb::Reverb;
use crate::dsp::smoother::coefficient;
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
    transport: Transport,
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
    delay: Delay,
    reverb: Reverb,
    /// The master's levels since the last report.
    reading: Reading,
    /// Frames until LFOs are next worked out.
    lfo_countdown: u32,
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
            transport: Transport::Stopped,
            next_step: 0,
            place: 0,
            origin_tick: 0,
            frames_since_origin: 0,
            clock: 0,
            millibpm,
            smoothing: coefficient(rate),
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
        if self.section_steps() == 0 {
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
        let reset = self.current.song.sections.first().and_then(|s| s.reset);
        reset.is_some_and(|reset| place >= reset as u64)
    }

    /// Send every track back to its start from the next step, leaving the beat where it is.
    pub fn realign(&mut self) {
        self.place = 0;
    }

    fn section_steps(&self) -> usize {
        let song = &self.current.song;
        song.sections.first().map_or(0, |s| song.section_steps(s))
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
                    state.set_held(command, now);
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
        self.halt(Transport::Stopped);
        if step == 0 {
            self.reset_tracks();
        }
        self.next_step = step as u64;
        self.place = step as u64;
        self.origin_tick = step as u64 * TICKS_PER_STEP;
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

    /// Restart a track's LFOs that follow its notes, and put it on their new values.
    fn retrigger_lfos(&mut self, index: usize) {
        let now = self.song_time();
        let Some(track) = self.current.song.tracks.get(index) else {
            return;
        };
        let state = &mut self.tracks[index];
        for (lfo, start) in track.lfos.iter().zip(&mut state.lfo_starts) {
            if lfo.retrig {
                start.tick = now.tick;
                start.clock = now.clock;
            }
        }
        state.modulate(track, index, now, false);
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
        self.next_step = 0;
        self.place = 0;
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
    pub fn preview(&mut self, instrument: u32, note: u8, velocity: u8) {
        let gain = velocity_gain(velocity);
        let Some(on) = self.note_on(instrument, note, gain) else {
            return;
        };
        let frames = PREVIEW_MS * self.rate / 1000;
        self.preview = Voice::start(
            NoteOn {
                release_after: Some(frames),
                ..on
            },
            self.rate,
        );
    }

    fn note_on(&self, instrument: u32, note: u8, step_gain: f32) -> Option<NoteOn> {
        let sample = *self.current.instrument_samples.get(&instrument)?;
        let settings = self.current.song.instruments.get(&instrument)?;
        Some(NoteOn {
            sample,
            note,
            gain: step_gain * level_gain(f32::from(settings.volume)),
            attack_ms: settings.attack_ms,
            release_ms: settings.release_ms,
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
        if self.section_steps() == 0 {
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
                self.retrigger_lfos(index);
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
                self.tracks[index].cut_to_tail();
            }
            if self.tracks[index].off_at.is_some_and(|at| at <= clock) {
                self.tracks[index].off_at = None;
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
        if self.section_steps() == 0 {
            return;
        }
        if self.past_reset(self.place) {
            self.place = 0;
        }
        let section_step = self.place as usize;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        for index in 0..tracks {
            // A big tempo jump can bring this step before the last one's swing delay ended.
            if let Some((_, pending)) = self.tracks[index].swung.take() {
                self.fire_track(index, pending);
            }
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
        let Some(section) = song.sections.first() else {
            return;
        };
        let Some(Some(chain)) = section.chains.get(index) else {
            return;
        };
        let Some(position) = song.chain_position(*chain, section_step) else {
            return;
        };
        let Some(step) = song.step_at(position) else {
            return;
        };
        if step.instrument.is_some() {
            self.tracks[index].instrument = step.instrument;
        }
        let mut delay_units = None;
        let mut kill_units = None;
        let mut off_units = None;
        let mut retrigger = None;
        let mut plays = true;
        for slot in &step.fx {
            let Some(command) = slot.command else {
                continue;
            };
            match command {
                FxCommand::Del => delay_units = Some(slot.value),
                FxCommand::Kil => kill_units = Some(slot.value),
                FxCommand::Off => off_units = Some(slot.value),
                FxCommand::Ret => retrigger = Some(slot.value),
                FxCommand::Cha => {
                    plays &= self.tracks[index].rng.chance(slot.value);
                }
                held => self.tracks[index].set_held(held, slot.value),
            }
        }
        if plays && delay_units.is_none() && matches!(step.note, Note::On(_)) {
            self.retrigger_lfos(index);
        }
        if !self.tracks[index].sounding() {
            self.tracks[index].settle();
        }
        let start_frames = delay_units.map_or(0, |units| self.groove_units(units));
        let start_at = self.clock + start_frames;
        match step.note {
            Note::Empty => {}
            Note::Off if plays => {
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
                let on = self.tracks[index]
                    .instrument
                    .and_then(|i| self.note_on(i, note, gain));
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
    /// its instrument, velocity and transpose. Whatever an audition was sounding is cut first. Its
    /// FX don't run, and nothing that playback relies on changes.
    pub fn audition(&mut self, section_step: usize) {
        if self.transport == Transport::Playing {
            return;
        }
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some(section) = song.sections.first() else {
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
            let Some(position) = song.chain_position(*chain, section_step) else {
                continue;
            };
            let Some(step) = song.step_at(position) else {
                continue;
            };
            let Note::On(midi) = step.note else {
                continue;
            };
            let note = (i16::from(midi) + i16::from(position.transpose))
                .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
            let gain = step.velocity.map_or(1.0, velocity_gain);
            let instrument = step.instrument.or(self.tracks[index].instrument);
            let Some(on) = instrument.and_then(|i| self.note_on(i, note, gain)) else {
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
            for track in self.tracks.iter_mut() {
                if !track.sounding() {
                    continue;
                }
                let main = track.voice.next(samples.get(track.voice.sample()));
                let tail = track.tail.next(samples.get(track.tail.sample()));
                let shaped = track.shape(
                    [main[0] + tail[0], main[1] + tail[1]],
                    self.smoothing,
                    self.rate,
                );
                mix[0] += shaped.dry[0];
                mix[1] += shaped.dry[1];
                delay_in[0] += shaped.delay[0];
                delay_in[1] += shaped.delay[1];
                reverb_in[0] += shaped.reverb[0];
                reverb_in[1] += shaped.reverb[1];
            }
            let preview = self.preview.next(samples.get(self.preview.sample()));
            let delayed = self.delay.process(delay_in, delay_frames);
            let reverbed = self.reverb.process(reverb_in);
            let mixed = [
                mix[0] + preview[0] + delayed[0] + reverbed[0],
                mix[1] + preview[1] + delayed[1] + reverbed[1],
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
