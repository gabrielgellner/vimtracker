//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::constants::{
    GROOVE_UNIT_TICKS, MAX_MILLIBPM, MAX_TRACKS, MIN_MILLIBPM, PREVIEW_MS, TICKS_PER_QUARTER,
    TICKS_PER_STEP,
};
use vt_model::step::{FxCommand, MAX_NOTE, MIN_NOTE, Note};

use crate::dsp::delay::Delay;
use crate::dsp::reverb::Reverb;
use crate::dsp::smoother::coefficient;
use crate::mixer::master;
use crate::published::Published;
use crate::track_state::{DelayedNote, Retrigger, TrackState};
use crate::voice::{NoteOn, Voice};

/// Delay time: 3/16 of a note, in ticks.
const DELAY_TICKS: u64 = TICKS_PER_QUARTER * 3 / 4;

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
    preview: Voice,
    transport: Transport,
    /// The next step to fire, counted from the section start without wrapping.
    next_step: u64,
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
}

impl Engine {
    /// A stopped engine at the top of the section.
    pub fn new(rate: u32, current: Arc<Published>) -> Self {
        let millibpm = current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        let seed = current.song.seed;
        let delay_frames = sample_of_tick(DELAY_TICKS, rate, MIN_MILLIBPM) as usize + 1;
        Self {
            rate,
            current,
            tracks: Box::new(std::array::from_fn(|index| TrackState::new(seed, index))),
            preview: Voice::default(),
            transport: Transport::Stopped,
            next_step: 0,
            origin_tick: 0,
            frames_since_origin: 0,
            clock: 0,
            millibpm,
            smoothing: coefficient(rate),
            delay: Delay::new(delay_frames),
            reverb: Reverb::new(rate),
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

    /// The last step fired, within the section, or the next to fire if none has yet.
    pub fn position(&self) -> usize {
        let length = self.section_steps();
        if length == 0 {
            return 0;
        }
        let fired = self.transport == Transport::Playing && self.frames_since_origin > 0;
        (self.next_step.saturating_sub(u64::from(fired)) % length as u64) as usize
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
        for track in self.tracks.iter_mut().skip(self.current.song.tracks.len()) {
            track.stop();
        }
        self.retune();
        old
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

    /// Play from a step. Enter stops if already playing.
    pub fn play_or_stop(&mut self, from_step: usize) {
        if self.transport == Transport::Playing {
            self.halt(Transport::Stopped);
        } else {
            self.play_from(from_step);
        }
    }

    /// Start at a step within the section. Starting from the top resets held values.
    pub fn play_from(&mut self, step: usize) {
        if step == 0 {
            self.reset_tracks();
        }
        self.next_step = step as u64;
        self.origin_tick = step as u64 * TICKS_PER_STEP;
        self.frames_since_origin = 0;
        self.millibpm = self.current.song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
        self.transport = Transport::Playing;
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
        self.origin_tick = 0;
        self.frames_since_origin = 0;
        self.reset_tracks();
    }

    fn reset_tracks(&mut self) {
        let seed = self.current.song.seed;
        for (index, track) in self.tracks.iter_mut().enumerate() {
            track.reset_held(seed, index);
        }
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
    pub fn preview(&mut self, instrument: u32, note: u8) {
        let Some(on) = self.note_on(instrument, note, 1.0) else {
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
            gain: step_gain * f32::from(settings.volume) / 255.0,
            attack_ms: settings.attack_ms,
            release_ms: settings.release_ms,
            release_after: None,
        })
    }

    /// Fill interleaved stereo output. This doesn't allocate, lock or print.
    pub fn process(&mut self, out: &mut [f32]) {
        let mut written = 0;
        let total = out.len() / 2;
        while written < total {
            let playing = self.transport == Transport::Playing;
            if playing && self.frames_until_next_step() == 0 {
                self.fire_step();
                self.next_step += 1;
                continue;
            }
            if playing {
                self.fire_due_events();
            }
            let until_event = if playing {
                self.frames_until_next_step()
                    .min(self.frames_until_track_event())
            } else {
                u64::MAX
            };
            let run = until_event.min((total - written) as u64).max(1) as usize;
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
        let length = self.section_steps();
        if length == 0 {
            return;
        }
        let section_step = (self.next_step % length as u64) as usize;
        let tracks = self.current.song.tracks.len().min(MAX_TRACKS);
        for index in 0..tracks {
            self.tracks[index].retrigger = None;
            self.fire_track(index, section_step);
        }
    }

    /// Start or stop a track's note for a step, and run its FX left to right.
    fn fire_track(&mut self, index: usize, section_step: usize) {
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
                let gain = step.volume.map_or(1.0, |v| f32::from(v) / 255.0);
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

    /// Render voices and effects into a run of frames with no events in it.
    fn render(&mut self, out: &mut [f32]) {
        let samples = Arc::clone(&self.current.samples);
        let delay_frames = sample_of_tick(DELAY_TICKS, self.rate, self.millibpm) as usize;
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
            *frame = master([
                mix[0] + preview[0] + delayed[0] + reverbed[0],
                mix[1] + preview[1] + delayed[1] + reverbed[1],
            ]);
        }
    }
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
