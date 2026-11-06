//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::constants::{TRACK_ENVELOPES, UNITY_LEVEL};
use vt_model::envelope::Envelope;
use vt_model::lfo::{Destination, LFOS_PER_TRACK};
use vt_model::song::{FilterType, Track, level_gain};
use vt_model::step::FxCommand;

use crate::dsp::filter::{Channel, Coefficients, Trim};
use crate::dsp::lfo::wave;
use crate::dsp::smoother::Smoothed;
use crate::rng::Pcg32;
use crate::voice::{NoteOn, Voice};

/// How often filter coefficients are recalculated while the cutoff moves, in frames.
const FILTER_UPDATE_FRAMES: u32 = 32;
/// How close to wide open the utility filter counts as out of the way, on `0..=1`.
const TRIM_OPEN: f32 = 0.001;

/// Where an LFO's cycles count from.
#[derive(Debug, Clone, Copy, Default)]
pub struct LfoStart {
    pub tick: f64,
    pub clock: u64,
}

/// Where one of a track's envelopes has got to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum EnvelopeRun {
    #[default]
    Idle,
    Held {
        at: u64,
        from: f32,
    },
    Released {
        at: u64,
        from: f32,
    },
}

impl EnvelopeRun {
    fn level(self, envelope: &Envelope, clock: u64, rate: u32) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Held { at, from } => envelope.held(clock.saturating_sub(at), from, rate),
            Self::Released { at, from } => envelope.released(clock.saturating_sub(at), from, rate),
        }
    }
}

/// Where the song is, for working out LFOs.
#[derive(Debug, Clone, Copy)]
pub struct SongTime {
    pub tick: f64,
    pub clock: u64,
    pub rate: u32,
    pub seed: u64,
}

/// A note waiting for its delayed start.
#[derive(Debug, Clone, Copy)]
pub struct DelayedNote {
    pub at: u64,
    pub note: NoteOn,
}

/// Retriggering the current note.
#[derive(Debug, Clone, Copy)]
pub struct Retrigger {
    pub next_at: u64,
    pub interval: u64,
    /// Multiplies the level on each retrigger.
    pub factor: f32,
}

/// One track's voices, held FX values and pending events.
#[derive(Debug, Clone, Copy)]
pub struct TrackState {
    pub voice: Voice,
    pub tail: Voice,
    /// The instrument a note with no instrument keeps playing.
    pub instrument: Option<u32>,
    /// Live controls, kept here rather than in the song so undo can't change them.
    pub mute: bool,
    pub solo: bool,
    pub rng: Pcg32,
    pub last_note: Option<NoteOn>,
    pub delayed: Option<DelayedNote>,
    pub kill_at: Option<u64>,
    pub off_at: Option<u64>,
    pub retrigger: Option<Retrigger>,
    /// A step waiting for its swing delay.
    pub swung: Option<(u64, usize)>,
    /// Held `VOL`, `DLY` and `REV`, as level values, so `E0` is exactly 0 dB. What's smoothed is the gain they come to, so the
    /// curve is only worked out when they or an LFO change.
    vol: f32,
    delay: f32,
    reverb: f32,
    gain: Smoothed,
    /// The track's level from the song, which playback going back to the top leaves alone.
    pub level: Smoothed,
    pan: Smoothed,
    cutoff: Smoothed,
    resonance: Smoothed,
    delay_send: Smoothed,
    reverb_send: Smoothed,
    filter: [Channel; 2],
    filter_type: FilterType,
    drive: Smoothed,
    trim: [Trim; 2],
    low_cut: Smoothed,
    high_cut: Smoothed,
    /// The coefficients in use, and the value each was worked out for.
    low_cut_coefficients: Option<(f32, Coefficients)>,
    high_cut_coefficients: Option<(f32, Coefficients)>,
    trim_countdown: u32,
    coefficients: Option<Coefficients>,
    filter_countdown: u32,
    pub lfo_starts: [LfoStart; LFOS_PER_TRACK],
    /// What the LFOs add to each held value, in `Destination::ALL`'s order. Separate from the held
    /// values, so a step's FX moves the centre and the LFO carries on around it.
    offsets: [f32; Destination::ALL.len()],
    cut_offset: Smoothed,
    res_offset: Smoothed,
    pan_offset: Smoothed,
    /// What envelopes add, which moves fast and needs to keep its snap.
    cut_envelope: Smoothed,
    res_envelope: Smoothed,
    pan_envelope: Smoothed,
    envelopes: [EnvelopeRun; TRACK_ENVELOPES],
    /// What modulation multiplies a note's speed by.
    pitch: f64,
    /// Whether an LFO moves the filter, so it stays running at the top of its range.
    filter_moving: bool,
}

/// The pan gains for a `0..=1` position. The centre is unity, and it keeps equal power.
#[inline]
fn pan_gains(position: f32) -> (f32, f32) {
    let angle = position * std::f32::consts::FRAC_PI_2;
    (
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    )
}

/// Map a `PAN` value to a `0..=1` position, with `80` as exact centre.
fn pan_position(value: u8) -> f32 {
    let value = f32::from(value);
    if value >= 128.0 {
        0.5 + (value - 128.0) / 127.0 * 0.5
    } else {
        value / 128.0 * 0.5
    }
}

impl TrackState {
    /// A silent track with default held values.
    pub fn new(seed: u64, index: usize) -> Self {
        Self {
            voice: Voice::default(),
            tail: Voice::default(),
            instrument: None,
            mute: false,
            solo: false,
            rng: Pcg32::new(seed, index as u64),
            last_note: None,
            delayed: None,
            kill_at: None,
            off_at: None,
            retrigger: None,
            swung: None,
            vol: f32::from(UNITY_LEVEL),
            delay: 0.0,
            reverb: 0.0,
            gain: Smoothed::at(1.0),
            level: Smoothed::at(1.0),
            pan: Smoothed::at(0.5),
            cutoff: Smoothed::at(1.0),
            resonance: Smoothed::at(0.0),
            delay_send: Smoothed::at(0.0),
            reverb_send: Smoothed::at(0.0),
            filter: [Channel::default(); 2],
            filter_type: FilterType::LowPass,
            drive: Smoothed::at(0.0),
            trim: [Trim::default(); 2],
            low_cut: Smoothed::at(0.0),
            high_cut: Smoothed::at(1.0),
            low_cut_coefficients: None,
            high_cut_coefficients: None,
            trim_countdown: 0,
            coefficients: None,
            filter_countdown: 0,
            lfo_starts: [LfoStart::default(); LFOS_PER_TRACK],
            offsets: [0.0; Destination::ALL.len()],
            cut_offset: Smoothed::at(0.0),
            res_offset: Smoothed::at(0.0),
            pan_offset: Smoothed::at(0.0),
            cut_envelope: Smoothed::at(0.0),
            res_envelope: Smoothed::at(0.0),
            pan_envelope: Smoothed::at(0.0),
            envelopes: [EnvelopeRun::Idle; TRACK_ENVELOPES],
            pitch: 1.0,
            filter_moving: false,
        }
    }

    /// Reset held values, the random generator and LFOs, when playback goes back to the top. Values
    /// glide back, so anything still sounding doesn't click.
    pub fn reset_held(&mut self, seed: u64, index: usize, track: &Track, clock: u64) {
        self.rng = Pcg32::new(seed, index as u64);
        self.instrument = None;
        self.last_note = None;
        self.set_held(FxCommand::Vol, UNITY_LEVEL);
        self.set_held(FxCommand::Pan, track.pan);
        self.set_held(FxCommand::Cut, track.cutoff);
        self.set_held(FxCommand::Res, track.resonance);
        self.set_held(FxCommand::Dly, track.delay);
        self.set_held(FxCommand::Rev, track.reverb);
        self.lfo_starts = [LfoStart { tick: 0.0, clock }; LFOS_PER_TRACK];
    }

    /// Work out what the LFOs add now. A stopped track sits at its held values, so an audition
    /// hears those rather than wherever the LFO stopped.
    pub fn modulate(&mut self, track: &Track, index: usize, now: SongTime, stopped: bool) {
        let mut offsets = [0.0_f32; Destination::ALL.len()];
        self.filter_moving = false;
        for (slot, (lfo, start)) in track.lfos.iter().zip(&self.lfo_starts).enumerate() {
            let Some(destination) = lfo.destination else {
                continue;
            };
            if lfo.depth == 0 {
                continue;
            }
            self.filter_moving |= matches!(destination, Destination::Cut | Destination::Res);
            if stopped {
                continue;
            }
            let seconds = now.clock.wrapping_sub(start.clock) as f64 / f64::from(now.rate);
            let cycles = lfo.rate.cycles(now.tick - start.tick, seconds);
            let key = now.seed ^ ((index as u64) << 8 | slot as u64);
            let value = wave(lfo.shape, cycles + f64::from(lfo.phase) / 360.0, key);
            offsets[destination as usize] += value * f32::from(lfo.depth) / 255.0;
        }
        // A different filter starts from silence rather than from another's state.
        if self.filter_type != track.filter {
            self.filter_type = track.filter;
            self.filter = [Channel::default(); 2];
            self.coefficients = None;
        }
        self.drive.target = f32::from(track.drive) / 255.0;
        self.low_cut.target = f32::from(track.low_cut) / 255.0;
        self.high_cut.target = f32::from(track.high_cut) / 255.0;
        self.cut_offset.target = offsets[Destination::Cut as usize];
        self.res_offset.target = offsets[Destination::Res as usize];
        self.pan_offset.target = offsets[Destination::Pan as usize];
        let mut moved = [0.0_f32; Destination::ALL.len()];
        for (slot, (envelope, run)) in track.envelopes.iter().zip(self.envelopes).enumerate() {
            let Some(destination) = envelope.destination else {
                continue;
            };
            // The first one on `Vol` is each note's own shape, which the voice follows itself.
            if envelope.depth == 0 || (slot == 0 && destination == Destination::Vol) {
                continue;
            }
            self.filter_moving |= matches!(destination, Destination::Cut | Destination::Res);
            if stopped {
                continue;
            }
            let level = run.level(envelope, now.clock, now.rate);
            moved[destination as usize] += level * f32::from(envelope.depth) / 255.0;
        }
        self.cut_envelope.target = moved[Destination::Cut as usize];
        self.res_envelope.target = moved[Destination::Res as usize];
        self.pan_envelope.target = moved[Destination::Pan as usize];
        for (offset, envelope) in offsets.iter_mut().zip(moved) {
            *offset += envelope;
        }
        self.offsets = offsets;
        let semitones = offsets[Destination::Pitch as usize] * 255.0;
        self.pitch = 2_f64.powf(f64::from(semitones.clamp(-96.0, 96.0)) / 12.0);
        self.voice.set_pitch(self.pitch);
        self.update_gains();
    }

    /// Start the track's envelopes for a note.
    pub fn trigger_envelopes(&mut self, track: &Track, now: SongTime) {
        for (envelope, run) in track.envelopes.iter().zip(&mut self.envelopes) {
            let from = if envelope.reset {
                0.0
            } else {
                run.level(envelope, now.clock, now.rate)
            };
            *run = EnvelopeRun::Held {
                at: now.clock,
                from,
            };
        }
    }

    /// Start the release of the track's envelopes, when a note ends.
    pub fn release_envelopes(&mut self, track: &Track, now: SongTime) {
        for (envelope, run) in track.envelopes.iter().zip(&mut self.envelopes) {
            if matches!(run, EnvelopeRun::Held { .. }) {
                let from = run.level(envelope, now.clock, now.rate);
                *run = EnvelopeRun::Released {
                    at: now.clock,
                    from,
                };
            }
        }
    }

    /// Work out the gains of the held values that are heard on a level scale.
    fn update_gains(&mut self) {
        let gain = |held: f32, destination: Destination| {
            level_gain((held + self.offsets[destination as usize] * 255.0).clamp(0.0, 255.0))
        };
        let (vol, delay, reverb) = (
            gain(self.vol, Destination::Vol),
            gain(self.delay, Destination::Dly),
            gain(self.reverb, Destination::Rev),
        );
        self.gain.target = vol;
        self.delay_send.target = delay;
        self.reverb_send.target = reverb;
    }

    /// Start a new note, cancelling anything pending for the old one.
    pub fn start(&mut self, note: NoteOn, rate: u32) {
        let from = if note.shape.reset {
            0.0
        } else {
            self.voice.level()
        };
        self.cut_to_tail();
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.last_note = Some(note);
        self.voice = Voice::start(note, rate, from, self.pitch);
    }

    /// Fade the playing note.
    pub fn cut_to_tail(&mut self) {
        if self.voice.active() {
            self.tail = self.voice;
            self.tail.fade();
            self.voice = Voice::default();
        }
    }

    /// Stop sounding and drop anything pending, when the transport halts.
    pub fn stop(&mut self) {
        self.cut_to_tail();
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.retrigger = None;
        self.swung = None;
        self.envelopes = [EnvelopeRun::Idle; TRACK_ENVELOPES];
    }

    /// Set a held value from an FX command.
    pub fn set_held(&mut self, command: FxCommand, value: u8) {
        let scaled = f32::from(value) / 255.0;
        match command {
            FxCommand::Vol => self.vol = f32::from(value),
            FxCommand::Pan => self.pan.target = pan_position(value),
            FxCommand::Cut => self.cutoff.target = scaled,
            FxCommand::Res => self.resonance.target = scaled,
            FxCommand::Dly => self.delay = f32::from(value),
            FxCommand::Rev => self.reverb = f32::from(value),
            _ => {}
        }
        self.update_gains();
    }

    /// Whether anything is sounding.
    #[inline]
    pub fn sounding(&self) -> bool {
        self.voice.active() || self.tail.active()
    }

    /// Settle held values, so a new sound doesn't ramp from an old setting while idle.
    pub fn settle(&mut self) {
        self.gain.settle();
        self.level.settle();
        self.pan.settle();
        self.cutoff.settle();
        self.resonance.settle();
        self.delay_send.settle();
        self.reverb_send.settle();
        self.cut_offset.settle();
        self.res_offset.settle();
        self.pan_offset.settle();
        self.cut_envelope.settle();
        self.res_envelope.settle();
        self.pan_envelope.settle();
        self.drive.settle();
        self.low_cut.settle();
        self.high_cut.settle();
        self.coefficients = None;
        self.filter_countdown = 0;
    }

    /// The utility filter. It's out of the way when wide open, and coefficients are only worked out
    /// while it moves.
    #[inline]
    fn trimmed(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let low_cut = self.low_cut.next(smoothing);
        let high_cut = self.high_cut.next(smoothing);
        let (low_on, high_on) = (low_cut > TRIM_OPEN, high_cut < 1.0 - TRIM_OPEN);
        if !low_on && !high_on {
            self.low_cut_coefficients = None;
            self.high_cut_coefficients = None;
            return input;
        }
        if self.trim_countdown == 0 {
            self.trim_countdown = FILTER_UPDATE_FRAMES;
            let update = |slot: &mut Option<(f32, Coefficients)>, value: f32| {
                let current = slot.is_some_and(|(at, _)| (at - value).abs() < TRIM_OPEN);
                if !current {
                    *slot = Some((value, Coefficients::new(value, 0.0, rate)));
                }
            };
            if low_on {
                if self.low_cut_coefficients.is_none() {
                    self.trim[0].clear_low_cut();
                    self.trim[1].clear_low_cut();
                }
                update(&mut self.low_cut_coefficients, low_cut);
            } else {
                self.low_cut_coefficients = None;
            }
            if high_on {
                if self.high_cut_coefficients.is_none() {
                    self.trim[0].prime_high_cut(input[0]);
                    self.trim[1].prime_high_cut(input[1]);
                }
                update(&mut self.high_cut_coefficients, high_cut);
            } else {
                self.high_cut_coefficients = None;
            }
        }
        self.trim_countdown -= 1;
        let mut output = input;
        for (sample, trim) in output.iter_mut().zip(&mut self.trim) {
            if let Some((_, c)) = &self.low_cut_coefficients {
                *sample = trim.low_cut(*sample, c);
            }
            if let Some((_, c)) = &self.high_cut_coefficients {
                *sample = trim.high_cut(*sample, c);
            }
        }
        output
    }

    /// Filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub fn shape(&mut self, input: [f32; 2], smoothing: f32, fast: f32, rate: u32) -> ShapedFrame {
        let cutoff = (self.cutoff.next(smoothing)
            + self.cut_offset.next(smoothing)
            + self.cut_envelope.next(fast))
        .clamp(0.0, 1.0);
        let resonance = (self.resonance.next(smoothing)
            + self.res_offset.next(smoothing)
            + self.res_envelope.next(fast))
        .clamp(0.0, 1.0);
        let position = (self.pan.next(smoothing)
            + self.pan_offset.next(smoothing)
            + self.pan_envelope.next(fast))
        .clamp(0.0, 1.0);
        let drive = self.drive.next(smoothing);
        let open = self.filter_type == FilterType::LowPass
            && cutoff > 0.999
            && resonance < 0.001
            && drive < 0.001
            && !self.filter_moving;
        let filtered = if open {
            self.coefficients = None;
            input
        } else {
            if self.filter_countdown == 0 || self.coefficients.is_none() {
                self.coefficients = Some(Coefficients::new(cutoff, resonance, rate));
                self.filter_countdown = FILTER_UPDATE_FRAMES;
            }
            self.filter_countdown -= 1;
            // A drive this small takes the clean path rather than a very slight clip.
            let drive = if drive < 0.001 { 0.0 } else { drive };
            match &self.coefficients {
                Some(c) => [
                    self.filter[0].process(input[0], self.filter_type, drive, c),
                    self.filter[1].process(input[1], self.filter_type, drive, c),
                ],
                None => input,
            }
        };
        let filtered = self.trimmed(filtered, smoothing, rate);
        let gain = self.gain.next(smoothing) * self.level.next(smoothing);
        let (left, right) = pan_gains(position);
        let dry = [filtered[0] * gain * left, filtered[1] * gain * right];
        let delay = self.delay_send.next(smoothing);
        let reverb = self.reverb_send.next(smoothing);
        ShapedFrame {
            dry,
            delay: [dry[0] * delay, dry[1] * delay],
            reverb: [dry[0] * reverb, dry[1] * reverb],
        }
    }
}

/// One track's frame, and what it sends to the effects.
#[derive(Debug, Clone, Copy)]
pub struct ShapedFrame {
    pub dry: [f32; 2],
    pub delay: [f32; 2],
    pub reverb: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pan_is_exactly_centred_at_80_and_keeps_power() {
        assert_eq!(pan_position(0x80), 0.5);
        assert_eq!(pan_position(0x00), 0.0);
        assert_eq!(pan_position(0xFF), 1.0);
        let (l, r) = pan_gains(0.5);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = pan_gains(0.2);
        assert!((l * l + r * r - 2.0).abs() < 1e-5);
    }
}
