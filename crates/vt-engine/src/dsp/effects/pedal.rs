//! The `Distort` effect's guitar pedals (#361), each modelled on a pedal's circuit and tuned
//! against a reference by its output alone, through `tools/compare`.
//!
//! - `Overdrive` is an Ibanez TS-808 Tube Screamer's clipping and tone stages, from ElectroSmash's
//!   "Tube Screamer Analysis" and from D. T. Yeh, J. S. Abel and J. O. Smith, "Simplified,
//!   physically-informed models of distortion and overdrive guitar effects pedals" (DAFx-07). An
//!   op-amp's gain branch, 4.7k and 47 nF to ground, only lets through what's over 720 Hz, and the
//!   gain it gets, the feedback's 51k and `Drive`'s 500k over the 4.7k, is clipped by a pair of
//!   1N914 diodes across that feedback, with its 51 pF taking off the highs as `Drive` goes up.
//!   What's clipped is added to the clean sound, so the bass passes clean under clipped mids: the
//!   Tube Screamer's mid hump. A 723 Hz low-pass, 1k and 220 nF, and the tone stage after it then
//!   put back as much of the highs as `Tone` asks.
//! - `Distortion` is an op-amp distortion's, as ElectroSmash's "ProCo RAT Analysis" lays out:
//!   high gain, then clipping to ground, then a tone low-pass. Its gain, its clip's knee and offset,
//!   and its level are fitted to Arturia's Dist OPAMP-21, after Tech 21's SansAmp, in its `Modern` mode,
//!   which has no EQ of its own: a clip much harder than a diode's, which stays flat at the
//!   ceiling however hard it's driven. `Tone` is a 12 dB an octave low-pass, as the RAT's `Filter`
//!   is a low-pass, though a gentler one.
//!
//! Both run their clipping at four times the rate. The circuit values are the published
//! schematics'; everything else, the scaling, the fitting and the code, is this program's own.

use std::f32::consts::TAU;

use vt_model::insert::DistortType;

use crate::dsp::blocks::oversampler::QuadrupleOversampler;
use crate::dsp::blocks::plaits_parts::Svf;
use crate::dsp::blocks::smoother::Smoothed;

/// How many volts full scale stands for in the Tube Screamer: about what a TB-303 puts out, a volt
/// from peak to peak.
const VOLTS_AT_FULL_SCALE: f32 = 0.5;
/// How many times the rate the clipping runs at.
const OVERSAMPLING: f32 = 4.0;

/// A 1N914's saturation current, in amps, and its thermal voltage times its ideality, in volts.
const DIODE_SATURATION_CURRENT: f32 = 2.52e-9;
const DIODE_THERMAL_VOLTAGE: f32 = 1.752 * 0.025_85;
/// How many Newton steps solve a diode pair's voltage. Each starts from above and closes in, and
/// five are within a microvolt for anything a pedal is given.
const DIODE_STEPS: usize = 5;

/// The Tube Screamer's gain branch to ground: 4.7k and 47 nF, so only what's over 720 Hz is
/// clipped.
const OVERDRIVE_GAIN_RESISTOR: f32 = 4_700.0;
const OVERDRIVE_GAIN_CAPACITOR: f32 = 47e-9;
/// Its feedback: 51k, `Drive`'s 500k log pot, and 51 pF across them.
const OVERDRIVE_FEEDBACK_RESISTOR: f32 = 51_000.0;
const OVERDRIVE_DRIVE_POT: f32 = 500_000.0;
const OVERDRIVE_FEEDBACK_CAPACITOR: f32 = 51e-12;
/// The low-pass before its tone stage: 1k and 220 nF.
const OVERDRIVE_TONE_HZ: f32 = 723.0;
/// How much of the highs over that the tone stage puts back, from `Tone` at `00` to `FF`: none, to
/// a little more than were there.
const OVERDRIVE_MOST_TREBLE: f32 = 1.5;
/// Where its output's highs roll off.
const OVERDRIVE_TOP_HZ: f32 = 12_000.0;
/// Its `Level`, set so a 303 line at about -16 dBFS RMS, as the riff decision 130 fitted it on was,
/// comes out as loud as it went in at `Amount`'s and `Tone`'s defaults.
const OVERDRIVE_LEVEL: f32 = 0.43;

/// `Distortion`'s gain before it clips, in dB, at `Amount` `00`, and how much more `FF` adds:
/// Dist OPAMP-21's `Drive` from about 5% to 100%, as `Amount`'s place is that `Drive`'s.
const DISTORTION_LEAST_GAIN_DB: f32 = 17.5;
const DISTORTION_GAIN_RANGE_DB: f32 = 25.6;
/// How hard its clip's knee is, as the power in `x / (1 + |x|^n)^(1/n)`: a hard clip with only a
/// little rounding, fitted to Dist OPAMP-21's harmonics as it starts to clip.
const DISTORTION_KNEE: f32 = 10.0;
/// How far off centre its clip sits, as a share of its ceiling, as an op-amp's two rails or a pair
/// of diodes never quite match: one side clips a little before the other, which gives Dist
/// OPAMP-21's 2nd harmonic, about 28 dB under the fundamental just over the clip and falling as
/// it's driven harder.
const DISTORTION_OFFSET: f32 = 0.086;
/// What its clip's ceiling comes out at: Dist OPAMP-21's, 0.157, raised 3 dB so a 303 line
/// comes out as loud as it went in at the defaults.
const DISTORTION_CEILING: f32 = 0.22;
/// Its two high-passes going in, as Dist OPAMP-21's bass falls.
const DISTORTION_HIGH_PASS_HZ: f32 = 22.0;
/// Where `Tone` puts its low-pass, at `00` and at `FF`, evenly in octaves between.
const DISTORTION_DARKEST_HZ: f32 = 500.0;
const DISTORTION_BRIGHTEST_HZ: f32 = 12_000.0;
/// A Butterworth's resonance, for a tone low-pass with no peak.
const BUTTERWORTH: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The coupling capacitor out of each pedal, as a high-pass, which takes out the offset an uneven
/// sound's clipping leaves: as low as Dist OPAMP-21's, whose offset falls back over about 120 ms
/// once a note stops.
const OUTPUT_COUPLING_HZ: f32 = 1.3;

/// Below this, with nothing going in, the pedal has rung out and rests at an exact zero, so its
/// filters never decay into denormals.
const QUIET: f32 = 1e-9;

/// How far a one-pole low-pass moves toward its input each sample.
#[inline]
fn one_pole(hertz: f32, rate: f32) -> f32 {
    1.0 - (-TAU * hertz / rate).exp()
}

/// A log pot's share of its resistance at a place from `0` to `1`: a tenth at the middle.
#[inline]
fn audio_taper(place: f32) -> f32 {
    (100_f32.powf(place) - 1.0) / 99.0
}

/// The voltage across a pair of diodes, back to back, with a resistor across them or before them,
/// for the voltage `w` the resistor alone would have: it solves `w = v + k sinh(v / vt)`, where `k`
/// is twice the diodes' saturation current times the resistor.
#[inline]
pub(crate) fn diode_pair(w: f32, k: f32) -> f32 {
    let size = w.abs();
    // Both the resistor's straight line and the diodes' curve are over the solution, so starting
    // from the lower of the two, Newton's steps come down onto it and never past it.
    let mut v = size.min(DIODE_THERMAL_VOLTAGE * (size / k).asinh());
    for _ in 0..DIODE_STEPS {
        let rising = (v / DIODE_THERMAL_VOLTAGE).exp();
        let falling = 1.0 / rising;
        let error = v + k * 0.5 * (rising - falling) - size;
        let slope = 1.0 + k / DIODE_THERMAL_VOLTAGE * 0.5 * (rising + falling);
        v = (v - error / slope).max(0.0);
    }
    v.copysign(w)
}

/// A clip that's straight through, then flat at 1, with a knee.
#[inline]
fn hard_knee(x: f32) -> f32 {
    x / (1.0 + x.abs().powf(DISTORTION_KNEE)).powf(1.0 / DISTORTION_KNEE)
}

/// `Distortion`'s clip: the knee, a little off centre, with nothing out for nothing in.
#[inline]
fn off_centre_clip(x: f32) -> f32 {
    hard_knee(x + DISTORTION_OFFSET) - hard_knee(DISTORTION_OFFSET)
}

/// One side's filters and oversampling.
#[derive(Debug, Clone, Copy, Default)]
struct Channel {
    oversampler: QuadrupleOversampler,
    /// The low-passes whose difference from what goes in is a high-pass: the Tube Screamer's gain
    /// branch at the higher rate, or `Distortion`'s two going in.
    high_pass: [f32; 2],
    /// The Tube Screamer's feedback capacitor, at the higher rate.
    feedback: f32,
    /// Its tone stage's low-pass and its output's.
    tone: f32,
    top: f32,
    /// `Distortion`'s tone low-pass.
    low_pass: Svf,
    /// The output's coupling capacitor, as the low-pass its high-pass is the difference from.
    coupling: f32,
}

impl Channel {
    /// Whether anything is left ringing in it.
    fn rung_out(&self, last: f32) -> bool {
        last.abs() < QUIET
            && self.high_pass.iter().all(|state| state.abs() < QUIET)
            && self.feedback.abs() < QUIET
            && self.tone.abs() < QUIET
            && self.top.abs() < QUIET
            && self.coupling.abs() < QUIET
    }
}

/// What the pedals keep from frame to frame, small enough to keep in every slot. Nothing here
/// allocates.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pedal {
    channels: [Channel; 2],
    tone: Smoothed,
    /// The settings and rate the coefficients below were worked out for.
    worked_out: (f32, f32, f32),
    /// The Tube Screamer's gain, its diodes' `k`, and its one-poles: the gain branch and feedback
    /// at the higher rate, and its tone and output at the normal rate.
    gain: f32,
    diodes: f32,
    gain_branch: f32,
    feedback: f32,
    tone_low_pass: f32,
    top: f32,
    treble: f32,
    /// `Distortion`'s gain going in, and its high-passes' one-pole.
    drive: f32,
    distortion_high_pass: f32,
    /// The output coupling's one-pole.
    coupling: f32,
    /// Whether it's resting at zero, with nothing going in.
    resting: bool,
}

impl Default for Pedal {
    fn default() -> Self {
        Self {
            channels: [Channel::default(); 2],
            tone: Smoothed::at(0.5),
            worked_out: (f32::NAN, f32::NAN, f32::NAN),
            gain: 0.0,
            diodes: 1.0,
            gain_branch: 0.0,
            feedback: 0.0,
            tone_low_pass: 0.0,
            top: 0.0,
            treble: 0.0,
            drive: 1.0,
            distortion_high_pass: 0.0,
            coupling: 0.0,
            resting: true,
        }
    }
}

impl Pedal {
    /// Follow the slot's `Tone`, from `0` to `1`.
    pub(crate) fn follow(&mut self, tone: f32) {
        self.tone.target = tone;
    }

    /// Jump to the settings, for playing from the start or a pedal just chosen.
    pub(crate) fn settle(&mut self) {
        self.tone.settle();
    }

    /// Work out the coefficients for these settings, if they've moved.
    fn work_out(&mut self, amount: f32, tone: f32, rate: f32) {
        if self.worked_out == (amount, tone, rate) {
            return;
        }
        self.worked_out = (amount, tone, rate);
        let higher = rate * OVERSAMPLING;
        // The Tube Screamer.
        let feedback = OVERDRIVE_FEEDBACK_RESISTOR + OVERDRIVE_DRIVE_POT * audio_taper(amount);
        self.gain = feedback / OVERDRIVE_GAIN_RESISTOR;
        self.diodes = 2.0 * DIODE_SATURATION_CURRENT * feedback;
        let branch_hz = 1.0 / (TAU * OVERDRIVE_GAIN_RESISTOR * OVERDRIVE_GAIN_CAPACITOR);
        self.gain_branch = one_pole(branch_hz, higher);
        let feedback_hz = 1.0 / (TAU * feedback * OVERDRIVE_FEEDBACK_CAPACITOR);
        self.feedback = one_pole(feedback_hz.min(higher * 0.45), higher);
        self.tone_low_pass = one_pole(OVERDRIVE_TONE_HZ, rate);
        self.top = one_pole(OVERDRIVE_TOP_HZ.min(rate * 0.45), rate);
        self.treble = tone * OVERDRIVE_MOST_TREBLE;
        // The op-amp distortion.
        self.drive =
            10_f32.powf((DISTORTION_LEAST_GAIN_DB + amount * DISTORTION_GAIN_RANGE_DB) / 20.0);
        self.distortion_high_pass = one_pole(DISTORTION_HIGH_PASS_HZ, rate);
        self.coupling = one_pole(OUTPUT_COUPLING_HZ, rate);
        let tone_hz =
            DISTORTION_DARKEST_HZ * (DISTORTION_BRIGHTEST_HZ / DISTORTION_DARKEST_HZ).powf(tone);
        for channel in &mut self.channels {
            channel.low_pass.set_exact(tone_hz / rate, BUTTERWORTH);
        }
    }

    /// One frame through the pedal `kind`, driven by `amount` from `0` to `1`. A kind that isn't a
    /// pedal leaves it as it is.
    #[inline]
    pub(crate) fn process(
        &mut self,
        kind: DistortType,
        input: [f32; 2],
        amount: f32,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        let nothing_in = input == [0.0, 0.0];
        if nothing_in && self.resting {
            return [0.0; 2];
        }
        self.resting = false;
        let tone = self.tone.next(smoothing);
        self.work_out(amount, tone, rate as f32);
        let coupling = self.coupling;
        let out = [0, 1].map(|side| {
            let pedal = match kind {
                DistortType::Overdrive => self.overdrive(side, input[side]),
                DistortType::Distortion => self.distortion(side, input[side]),
                _ => return input[side],
            };
            let channel = &mut self.channels[side];
            channel.coupling += (pedal - channel.coupling) * coupling;
            pedal - channel.coupling
        });
        if nothing_in
            && self
                .channels
                .iter()
                .zip(out)
                .all(|(channel, last)| channel.rung_out(last))
        {
            // Start again from nothing, so no filter decays into denormals.
            self.channels = [Channel::default(); 2];
            self.worked_out = (f32::NAN, f32::NAN, f32::NAN);
            self.resting = true;
            return [0.0; 2];
        }
        out
    }

    /// One side through the Tube Screamer.
    #[inline]
    fn overdrive(&mut self, side: usize, input: f32) -> f32 {
        let (gain, diodes, branch, feedback) =
            (self.gain, self.diodes, self.gain_branch, self.feedback);
        let channel = &mut self.channels[side];
        let Channel {
            oversampler,
            high_pass,
            feedback: held,
            ..
        } = &mut *channel;
        let volts = input * VOLTS_AT_FULL_SCALE;
        let driven = oversampler.process(volts, |x| {
            // The current through the gain branch, as a voltage across the feedback's resistance.
            high_pass[0] += (x - high_pass[0]) * branch;
            let through = (x - high_pass[0]) * gain;
            *held += (through - *held) * feedback;
            x + diode_pair(*held, diodes)
        });
        channel.tone += (driven - channel.tone) * self.tone_low_pass;
        let toned = channel.tone + (driven - channel.tone) * self.treble;
        channel.top += (toned - channel.top) * self.top;
        channel.top * OVERDRIVE_LEVEL / VOLTS_AT_FULL_SCALE
    }

    /// One side through the op-amp distortion.
    #[inline]
    fn distortion(&mut self, side: usize, input: f32) -> f32 {
        let (drive, high_pass) = (self.drive, self.distortion_high_pass);
        let channel = &mut self.channels[side];
        let mut x = input * drive;
        for state in &mut channel.high_pass {
            *state += (x - *state) * high_pass;
            x -= *state;
        }
        let clipped = channel.oversampler.process(x, off_centre_clip);
        channel.low_pass.low_pass(clipped) * DISTORTION_CEILING
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The diode pair's voltage by bisection, in double precision.
    fn bisected(w: f64, k: f64) -> f64 {
        let vt = f64::from(DIODE_THERMAL_VOLTAGE);
        let (mut low, mut high) = (0.0, w.abs());
        for _ in 0..200 {
            let middle = 0.5 * (low + high);
            if middle + k * (middle / vt).sinh() > w.abs() {
                high = middle;
            } else {
                low = middle;
            }
        }
        (0.5 * (low + high)).copysign(w)
    }

    #[test]
    fn the_diodes_are_solved_to_within_a_microvolt() {
        for k in [2.6e-4_f32, 2.8e-3, 5e-6] {
            for w in [
                -40.0_f32, -3.0, -0.5, -0.01, 0.0, 1e-4, 0.05, 0.3, 0.8, 2.0, 25.0, 300.0,
            ] {
                let solved = diode_pair(w, k);
                let exact = bisected(f64::from(w), f64::from(k));
                assert!(
                    (f64::from(solved) - exact).abs() < 1e-6,
                    "k {k}, w {w}: {solved} against {exact}"
                );
            }
        }
    }

    #[test]
    fn the_diodes_follow_a_small_voltage_and_hold_a_large_one_under_a_volt() {
        let k = 2.0 * DIODE_SATURATION_CURRENT * OVERDRIVE_FEEDBACK_RESISTOR;
        assert!((diode_pair(0.01, k) - 0.01).abs() < 1e-4);
        assert!(diode_pair(100.0, k) < 0.9);
        assert!(
            diode_pair(100.0, k) > diode_pair(10.0, k),
            "it keeps rising, slowly"
        );
        assert_eq!(diode_pair(-0.4, k), -diode_pair(0.4, k));
    }

    #[test]
    fn the_hard_knee_is_straight_then_flat() {
        assert!((hard_knee(0.1) - 0.1).abs() < 1e-6);
        assert!((hard_knee(5.0) - 1.0).abs() < 1e-6);
        assert!((hard_knee(1.0) - 0.933).abs() < 1e-3);
        assert_eq!(hard_knee(-3.0), -hard_knee(3.0));
    }
}
