//! A diode ladder low-pass filter, the kind in Roland's TB-303, from Vital's `DiodeFilter`. Ported
//! to Rust from `src/synthesis/filters/diode_filter.{h,cpp}` and the one-pole filter it's built
//! from, `one_pole_filter.h`, with the `tanh` of `futils.h`, taken out of Vital's SIMD and processor types. It has Vital's 24 dB
//! style, whose blend moves the high-pass at its input from nine octaves under the cutoff, where
//! `Acid` keeps it, up to one under, so less bass reaches the ladder.
//!
//! Its first stage saturates, its last is clipped, and a high-pass sits in its feedback loop, which
//! is part of how a 303 sounds as the resonance rises: the resonance thins the bass rather than
//! only adding a peak.
//!
//! Copyright 2013-2019 Matt Tytel.
//!
//! vital is free software: you can redistribute it and/or modify it under the terms of the GNU
//! General Public License as published by the Free Software Foundation, either version 3 of the
//! License, or (at your option) any later version.
//!
//! vital is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even
//! the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
//! General Public License for more details.
//!
//! You should have received a copy of the GNU General Public License along with vital. If not, see
//! <http://www.gnu.org/licenses/>.

/// The feedback at no resonance and at full.
const LEAST_RESONANCE: f32 = 0.7;
const MOST_RESONANCE: f32 = 17.0;
/// The corner of the high-pass in the feedback loop, in Hz.
const FEEDBACK_HIGH_PASS: f32 = 20.0;
/// Where it resonates, as a share of the cutoff it's given. Its four stages load each other, so
/// their poles spread out: with no resonance, it starts to fall well under the cutoff, and the
/// resonance peaks at about this share of it.
pub const RESONANT_SHARE: f32 = 0.6;
/// How far under the cutoff the input's high-pass sits, as a ratio: Vital's lowest, `2^-9`.
pub const HIGH_PASS_RATIO: f32 = 1.0 / 512.0;

/// A one-pole low-pass in trapezoidal form, whose state can be saturated as it goes. Vital's
/// `Dirty` filter is built from it too.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OnePole {
    /// The output at this sample.
    pub(crate) current: f32,
    /// What it holds for the next sample, and the same through its saturation.
    state: f32,
    pub(crate) saturated: f32,
}

impl OnePole {
    /// One sample with no saturation.
    #[inline]
    pub(crate) fn tick_plain(&mut self, input: f32, coefficient: f32) -> f32 {
        let delta = coefficient * (input - self.state);
        self.state += delta;
        self.current = self.state;
        self.state += delta;
        self.current
    }

    /// One sample, with the state passed through `saturate`.
    #[inline]
    pub(crate) fn tick(&mut self, input: f32, coefficient: f32, saturate: impl Fn(f32) -> f32) {
        let delta = coefficient * (input - self.saturated);
        self.state += delta;
        self.current = saturate(self.state);
        self.state += delta;
        self.saturated = saturate(self.state);
    }
}

/// The coefficient for a one-pole at a frequency as a fraction of the sample rate, warped as Vital
/// warps it so it stays below a quarter turn however high it's asked to go.
#[inline]
fn coefficient(frequency: f32) -> f32 {
    let scaled = frequency * std::f32::consts::PI;
    (scaled / (scaled + 1.0))
        .min(0.499 * std::f32::consts::PI)
        .tan()
}

/// Vital's rational approximation of `tanh`, from `futils.h`, which is much cheaper. It runs a
/// hair over 1 for very large values.
#[inline]
pub(crate) fn tanh(value: f32) -> f32 {
    let magnitude = value.abs();
    let square = value * value;
    let numerator = value
        * (2.455_507_5
            + 2.455_507_5 * magnitude
            + square * (0.893_229_85 + 0.821_226_67 * magnitude));
    let denominator =
        2.445_066_3 + (2.445_066_3 + square) * (value + 0.814_642_7 * value * magnitude).abs();
    numerator / denominator
}

#[inline]
fn clip(value: f32) -> f32 {
    value.clamp(-1.0, 1.0)
}

/// The coefficient of the high-pass in the feedback loop, for a filter running at this rate, in Hz.
pub fn feedback_coefficient(rate: f32) -> f32 {
    coefficient(FEEDBACK_HIGH_PASS / rate)
}

/// A diode ladder's state.
#[derive(Debug, Clone, Copy, Default)]
pub struct DiodeFilter {
    high_pass: [OnePole; 2],
    feedback_high_pass: OnePole,
    stages: [OnePole; 4],
}

impl DiodeFilter {
    /// One sample, at a cutoff as a fraction of its rate, with resonance from 0 to 1, a drive as a
    /// gain of at least 1, the [`feedback_coefficient`] for its rate, and where the input's
    /// high-pass sits as a share of the cutoff, [`HIGH_PASS_RATIO`] as Vital has it at its lowest.
    /// The output stays within ±1.
    #[inline]
    pub fn next(
        &mut self,
        input: f32,
        cutoff: f32,
        resonance: f32,
        drive: f32,
        feedback_coefficient: f32,
        low_cut: f32,
    ) -> f32 {
        let resonance = resonance.clamp(0.0, 1.0);
        let resonance = LEAST_RESONANCE + (MOST_RESONANCE - LEAST_RESONANCE) * resonance.powi(3);
        let drive_gain = drive.max(1.0);
        let drive = (resonance * 0.5 + 1.0) * drive_gain;
        let post = 1.0 / drive_gain.sqrt();
        let coefficient = coefficient(cutoff.clamp(0.0, 1.0));

        // A two-pole high-pass under the cutoff.
        let high = coefficient * low_cut;
        let high_squared = high * high;
        let feedback_mult = high * 2.0 - high_squared - 1.0;
        let normalizer = 1.0 / (high_squared - high + 1.0);
        let [first, second] = &mut self.high_pass;
        let feedback = feedback_mult * first.state + (1.0 - high) * second.state;
        let high_input = (input - feedback) * normalizer;
        let first_out = first.tick_plain(high_input, high);
        let second_out = second.tick_plain(first_out, high);
        let high_out = high_input - first_out * 2.0 + second_out;

        // The ladder, with its resonance fed back from the last stage through a saturation, and
        // its first stage's input high-passed.
        let [stage1, stage2, stage3, stage4] = &mut self.stages;
        let filter_input = (drive * high_out - resonance * stage4.saturated) * 0.5;
        let saturated = tanh(filter_input);
        let feedback_input = saturated + stage2.saturated;
        let feedback = self
            .feedback_high_pass
            .tick_plain(feedback_input, feedback_coefficient);
        stage1.tick(feedback_input - feedback, coefficient, tanh);
        stage2.tick(
            (stage1.current + stage3.saturated) * 0.5,
            coefficient,
            |value| value,
        );
        stage3.tick(
            (stage2.current + stage4.saturated) * 0.5,
            coefficient,
            |value| value,
        );
        stage4.tick(stage3.current, coefficient, clip);
        stage4.current * post
    }
}

/// A complex number, for working out the response.
#[derive(Clone, Copy)]
pub(crate) struct Complex(pub(crate) f32, pub(crate) f32);

impl Complex {
    pub(crate) fn add(self, other: Self) -> Self {
        Self(self.0 + other.0, self.1 + other.1)
    }

    pub(crate) fn scale(self, by: f32) -> Self {
        Self(self.0 * by, self.1 * by)
    }

    pub(crate) fn times(self, other: Self) -> Self {
        Self(
            self.0 * other.0 - self.1 * other.1,
            self.0 * other.1 + self.1 * other.0,
        )
    }

    pub(crate) fn over(self, other: Self) -> Self {
        let size = other.0 * other.0 + other.1 * other.1;
        Self(
            (self.0 * other.0 + self.1 * other.1) / size,
            (self.1 * other.0 - self.0 * other.1) / size,
        )
    }

    pub(crate) fn power(self) -> f32 {
        self.0 * self.0 + self.1 * self.1
    }
}

/// How much the filter lets through at a frequency, as a power, for a quiet signal with no drive.
/// The cutoff and frequency are fractions of its rate, and the feedback coefficient and low cut are
/// the ones it runs with. Its saturations do nothing to a quiet signal, so it's the high-pass at its
/// input, then four one-poles, each fed half of its neighbours, with the last fed back through the
/// feedback's high-pass.
pub fn response(
    cutoff: f32,
    resonance: f32,
    feedback_coefficient: f32,
    low_cut: f32,
    frequency: f32,
) -> f32 {
    // A trapezoidal one-pole with Vital's coefficient `c` is an analog one with a cutoff of
    // `c / (1 - c)`, on a frequency axis warped as the bilinear transform warps it.
    let analog = |coefficient: f32| coefficient / (1.0 - coefficient);
    let along = (std::f32::consts::PI * frequency.min(0.499)).tan();
    let resonance = resonance.clamp(0.0, 1.0);
    let resonance = LEAST_RESONANCE + (MOST_RESONANCE - LEAST_RESONANCE) * resonance.powi(3);
    let drive = resonance * 0.5 + 1.0;
    let one = Complex(1.0, 0.0);
    let coefficient = coefficient(cutoff.clamp(0.0, 1.0));
    // Each stage's `1 + s / w`, and the feedback high-pass's `s / (s + w)`.
    let stage = Complex(1.0, along / analog(coefficient));
    let q = Complex(0.0, along / analog(feedback_coefficient));
    let high_pass = q.over(one.add(q));
    // Working back from the last stage, the out over the in is
    // `h d / 2 / (4b^4 - 3b^2 - h (2b^2 - 1) + h r / 2)`.
    let squared = stage.times(stage);
    let below = squared
        .times(squared)
        .scale(4.0)
        .add(squared.scale(-3.0))
        .add(
            high_pass
                .times(squared.scale(2.0).add(Complex(-1.0, 0.0)))
                .scale(-1.0),
        )
        .add(high_pass.scale(resonance * 0.5));
    let ladder = high_pass.scale(drive * 0.5).over(below).power();
    ladder * input_high_pass(coefficient * low_cut, frequency)
}

/// What the high-pass at the input lets through at a frequency, as a power, from its one-poles'
/// coefficient `h`. Each is `L = h (1 + z^-1) / (1 - (1 - 2h) z^-1)`, and it hears its own states a
/// sample late, so it's `n (1 - L)^2 / (1 + n z^-1 p (f + (1 - h) L))`, with `p = 2h / (1 - (1 - 2h)
/// z^-1)`, `n` its normaliser and `f` its first state's feedback.
fn input_high_pass(h: f32, frequency: f32) -> f32 {
    let turn = std::f32::consts::TAU * frequency.min(0.499);
    let back = Complex(turn.cos(), -turn.sin());
    let one = Complex(1.0, 0.0);
    let pole = one.add(back.scale(-(1.0 - 2.0 * h)));
    let p = Complex(2.0 * h, 0.0).over(pole);
    let low = one.add(back).scale(h).over(pole);
    let normalizer = 1.0 / (h * h - h + 1.0);
    let feedback = -(1.0 - h) * (1.0 - h);
    let rest = one.add(low.scale(-1.0));
    let loop_gain = back
        .times(p)
        .times(Complex(feedback, 0.0).add(low.scale(1.0 - h)))
        .scale(normalizer);
    rest.times(rest)
        .scale(normalizer)
        .over(one.add(loop_gain))
        .power()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [f32; 2] = [44_100.0, 192_000.0];

    /// How loud a quiet sine comes out, in dB, after it settles.
    fn gain_db(rate: f32, hertz: f32, cutoff: f32, resonance: f32) -> f32 {
        let mut filter = DiodeFilter::default();
        let feedback = feedback_coefficient(rate);
        let frames = (rate * 0.5) as usize;
        let mut peak = 0.0_f32;
        let mut input_peak = 0.0_f32;
        for frame in 0..frames {
            let input = 0.01 * (std::f32::consts::TAU * hertz * frame as f32 / rate).sin();
            let output = filter.next(
                input,
                cutoff / rate,
                resonance,
                1.0,
                feedback,
                HIGH_PASS_RATIO,
            );
            if frame > frames / 2 {
                peak = peak.max(output.abs());
                input_peak = input_peak.max(input.abs());
            }
        }
        20.0 * (peak / input_peak).log10()
    }

    #[test]
    fn it_passes_the_bass_and_falls_steeply_above_the_cutoff() {
        for rate in RATES {
            let pass = gain_db(rate, 50.0, 1_000.0, 0.0);
            let above = gain_db(rate, 2_000.0, 1_000.0, 0.0);
            let further = gain_db(rate, 4_000.0, 1_000.0, 0.0);
            assert!(pass > -3.0, "{pass} dB at {rate}");
            assert!(pass - above > 40.0, "{pass} {above} at {rate}");
            // Its four poles are spread out, so it falls about 20 dB an octave here, and more
            // further on.
            assert!(above - further > 18.0, "{above} {further} at {rate}");
        }
    }

    #[test]
    fn resonance_raises_a_peak_under_the_cutoff() {
        for rate in RATES {
            let peak = 1_000.0 * RESONANT_SHARE;
            let flat = gain_db(rate, peak, 1_000.0, 0.0);
            let peaked = gain_db(rate, peak, 1_000.0, 0.8);
            assert!(peaked > flat + 12.0, "{flat} {peaked} at {rate}");
        }
    }

    #[test]
    fn resonance_thins_the_bass() {
        for rate in RATES {
            let flat = gain_db(rate, 50.0, 1_000.0, 0.0);
            let resonant = gain_db(rate, 50.0, 1_000.0, 0.8);
            assert!(resonant < flat - 3.0, "{flat} {resonant} at {rate}");
        }
    }

    #[test]
    fn full_resonance_rings_on_by_itself_near_its_peak() {
        for rate in RATES {
            let mut filter = DiodeFilter::default();
            let feedback = feedback_coefficient(rate);
            let frames = rate as usize;
            let out: Vec<f32> = (0..frames)
                .map(|frame| {
                    let kick = if frame < 10 { 0.5 } else { 0.0 };
                    filter.next(kick, 1_000.0 / rate, 1.0, 1.0, feedback, HIGH_PASS_RATIO)
                })
                .collect();
            let tail = &out[frames / 2..];
            let rising = tail
                .windows(2)
                .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
                .count() as f32;
            let hertz = rising * 2.0;
            let expected = 1_000.0 * RESONANT_SHARE;
            assert!(
                (hertz - expected).abs() < expected * 0.1,
                "{hertz} at {rate}"
            );
        }
    }

    #[test]
    fn full_resonance_stays_bounded_with_loud_input_and_after_it() {
        for rate in RATES {
            for cutoff in [20.0, 200.0, 2_000.0, 12_000.0, 40_000.0] {
                for drive in [1.0, 16.0] {
                    let mut filter = DiodeFilter::default();
                    let feedback = feedback_coefficient(rate);
                    for frame in 0..(rate as usize / 2) {
                        // A loud square, then silence for it to ring into.
                        let input = if frame < rate as usize / 4 {
                            if (frame / 100) % 2 == 0 { 1.0 } else { -1.0 }
                        } else {
                            0.0
                        };
                        let output = filter.next(
                            input,
                            cutoff / rate,
                            1.0,
                            drive,
                            feedback,
                            HIGH_PASS_RATIO,
                        );
                        assert!(
                            output.is_finite() && output.abs() <= 1.0,
                            "{output} at {cutoff} Hz, {rate}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sweeping_the_cutoff_at_full_resonance_stays_bounded() {
        for rate in RATES {
            let mut filter = DiodeFilter::default();
            let feedback = feedback_coefficient(rate);
            let frames = rate as usize * 2;
            for frame in 0..frames {
                // Up and down the whole range eight times a second, with a saw in.
                let along = frame as f32 / rate;
                let sweep = (along * 8.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                let cutoff = 20.0 * 1_000_f32.powf(sweep);
                let input = (along * 55.0).fract() * 2.0 - 1.0;
                let output = filter.next(input, cutoff / rate, 1.0, 1.0, feedback, HIGH_PASS_RATIO);
                assert!(output.is_finite() && output.abs() <= 1.0, "{output}");
            }
        }
    }

    #[test]
    fn it_falls_silent_after_its_input_stops() {
        for rate in RATES {
            let mut filter = DiodeFilter::default();
            let feedback = feedback_coefficient(rate);
            for frame in 0..(rate as usize) {
                let input = if frame < 1000 { 0.5 } else { 0.0 };
                filter.next(input, 500.0 / rate, 0.5, 1.0, feedback, HIGH_PASS_RATIO);
            }
            let tail: f32 = (0..100)
                .map(|_| {
                    filter
                        .next(0.0, 500.0 / rate, 0.5, 1.0, feedback, HIGH_PASS_RATIO)
                        .abs()
                })
                .fold(0.0, f32::max);
            assert!(tail < 1e-3, "{tail} at {rate}");
        }
    }
}
