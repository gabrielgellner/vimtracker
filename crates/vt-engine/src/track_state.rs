//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::step::FxCommand;

use crate::dsp::filter::{Coefficients, Svf};
use crate::dsp::smoother::Smoothed;
use crate::rng::Pcg32;
use crate::voice::{NoteOn, Voice};

/// How often filter coefficients are recalculated while the cutoff moves, in frames.
const FILTER_UPDATE_FRAMES: u32 = 32;

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
    gain: Smoothed,
    pan: Smoothed,
    cutoff: Smoothed,
    resonance: Smoothed,
    delay_send: Smoothed,
    reverb_send: Smoothed,
    filter: [Svf; 2],
    coefficients: Option<Coefficients>,
    filter_countdown: u32,
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
            gain: Smoothed::at(1.0),
            pan: Smoothed::at(0.5),
            cutoff: Smoothed::at(1.0),
            resonance: Smoothed::at(0.0),
            delay_send: Smoothed::at(0.0),
            reverb_send: Smoothed::at(0.0),
            filter: [Svf::default(); 2],
            coefficients: None,
            filter_countdown: 0,
        }
    }

    /// Reset held values and the random generator, when playback goes back to the top. Values
    /// glide back, so anything still sounding doesn't click.
    pub fn reset_held(&mut self, seed: u64, index: usize) {
        self.rng = Pcg32::new(seed, index as u64);
        self.instrument = None;
        self.last_note = None;
        self.gain.target = 1.0;
        self.pan.target = 0.5;
        self.cutoff.target = 1.0;
        self.resonance.target = 0.0;
        self.delay_send.target = 0.0;
        self.reverb_send.target = 0.0;
    }

    /// Start a new note, cancelling anything pending for the old one.
    pub fn start(&mut self, note: NoteOn, rate: u32) {
        self.cut_to_tail();
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.last_note = Some(note);
        self.voice = Voice::start(note, rate);
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
    }

    /// Set a held value from an FX command.
    pub fn set_held(&mut self, command: FxCommand, value: u8) {
        let scaled = f32::from(value) / 255.0;
        match command {
            FxCommand::Vol => self.gain.target = scaled,
            FxCommand::Pan => self.pan.target = pan_position(value),
            FxCommand::Cut => self.cutoff.target = scaled,
            FxCommand::Res => self.resonance.target = scaled,
            FxCommand::Dly => self.delay_send.target = scaled,
            FxCommand::Rev => self.reverb_send.target = scaled,
            _ => {}
        }
    }

    /// Whether anything is sounding.
    #[inline]
    pub fn sounding(&self) -> bool {
        self.voice.active() || self.tail.active()
    }

    /// Settle held values, so a new sound doesn't ramp from an old setting while idle.
    pub fn settle(&mut self) {
        self.gain.settle();
        self.pan.settle();
        self.cutoff.settle();
        self.resonance.settle();
        self.delay_send.settle();
        self.reverb_send.settle();
        self.coefficients = None;
        self.filter_countdown = 0;
    }

    /// Filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub fn shape(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> ShapedFrame {
        let cutoff = self.cutoff.next(smoothing);
        let resonance = self.resonance.next(smoothing);
        let filtered = if cutoff > 0.999 && resonance < 0.001 {
            self.coefficients = None;
            input
        } else {
            if self.filter_countdown == 0 || self.coefficients.is_none() {
                self.coefficients = Some(Coefficients::new(cutoff, resonance, rate));
                self.filter_countdown = FILTER_UPDATE_FRAMES;
            }
            self.filter_countdown -= 1;
            match &self.coefficients {
                Some(c) => [
                    self.filter[0].low_pass(input[0], c),
                    self.filter[1].low_pass(input[1], c),
                ],
                None => input,
            }
        };
        let gain = self.gain.next(smoothing);
        let (left, right) = pan_gains(self.pan.next(smoothing));
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
