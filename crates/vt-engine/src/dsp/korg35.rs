//! The Korg 35, the filter in Korg's MS-20, from sst-filters' K35. Ported to Rust from
//! `include/sst/filters/K35Filter.h`, which adapts Odin 2's `Korg35Filter.cpp` and follows Will
//! Pirkle's app note AN-5 and Zavalishin's *The Art of VA Filter Design*. It's taken out of sst's
//! SIMD and coefficient types, with its blend between clean and saturated left out, as it always
//! saturates here.
//!
//! It's a Sallen-Key loop. A one-pole on the input feeds a sum, which goes through a second one-pole
//! to the output. The output, turned up by the feedback, comes back to the sum through the loop's
//! other one-pole. The low-pass has low-passes forward and a high-pass back. The high-pass has the
//! mirror of that, and comes out of the sum. The feedback starts to self-oscillate at 2, right at the
//! cutoff. The sum saturates, as sst's low-pass has it, so the resonance screams and distorts rather
//! than growing.
//!
//! Copyright 2019-2025, various authors, as described in the GitHub transaction log.
//!
//! sst-filters is released under the Gnu General Public License version 3 or later. Some of the
//! filters in this package originated in the version of Surge open sourced in 2018.
//!
//! All source in sst-filters available at <https://github.com/surge-synthesizer/sst-filters>

use super::filter::soft_clip;

/// The feedback at full resonance. It self-oscillates from 2, and this goes as far past that as
/// Vital's Sallen-Key filter does, so it sings by itself near the top of `Res`, as the ladder does.
pub const MOST_FEEDBACK: f32 = 2.15;

/// A one-pole in trapezoidal form, whose coefficient `G` is `g / (1 + g)`.
#[derive(Debug, Clone, Copy, Default)]
struct OnePole {
    state: f32,
}

impl OnePole {
    #[inline]
    fn low_pass(&mut self, input: f32, coefficient: f32) -> f32 {
        let step = (input - self.state) * coefficient;
        let output = step + self.state;
        self.state = output + step;
        output
    }

    #[inline]
    fn high_pass(&mut self, input: f32, coefficient: f32) -> f32 {
        input - self.low_pass(input, coefficient)
    }
}

/// A Korg 35's state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Korg35 {
    /// The one-pole on the input, then the loop's two.
    input: OnePole,
    forward: OnePole,
    back: OnePole,
}

impl Korg35 {
    /// What comes out of the saturating sum, given what's known of what reaches it and how much of
    /// its own output comes straight back. As sst does, the loop is solved as if it were clean, and
    /// the answer saturated. That keeps the resonance in tune, within 5 cents up to 16 kHz.
    #[inline]
    fn sum(known: f32, comes_back: f32) -> f32 {
        soft_clip(known / (1.0 - comes_back))
    }

    /// One sample of the low-pass, with the coefficient `G` of its one-poles and the feedback, from
    /// 0 to [`MOST_FEEDBACK`].
    #[inline]
    pub fn low_pass(&mut self, input: f32, coefficient: f32, feedback: f32) -> f32 {
        let (g, rest) = (coefficient, 1.0 - coefficient);
        let entered = self.input.low_pass(input, g);
        // The loop's high-pass gives `rest * (feedback * low-pass) - rest * its state`, and its
        // low-pass `g * what it's fed + rest * its state`. What doesn't depend on this sample's sum
        // is known now.
        let known = entered + rest * (feedback * rest * self.forward.state - self.back.state);
        let summed = Self::sum(known, feedback * g * rest);
        let output = self.forward.low_pass(summed, g);
        self.back.high_pass(feedback * output, g);
        output
    }

    /// One sample of the high-pass, with the same coefficient and feedback.
    #[inline]
    pub fn high_pass(&mut self, input: f32, coefficient: f32, feedback: f32) -> f32 {
        let (g, rest) = (coefficient, 1.0 - coefficient);
        let entered = self.input.high_pass(input, g);
        // The loop's high-pass then low-pass give `g * rest * what's fed in`, plus what's known now.
        let known = entered + rest * (self.back.state - g * self.forward.state);
        let output = Self::sum(known, feedback * g * rest);
        let turned = self.forward.high_pass(feedback * output, g);
        self.back.low_pass(turned, g);
        output
    }
}

/// How much it lets through at a frequency, as a power, for a quiet signal: a two-pole with the
/// feedback taking from its damping, `1 / (s^2 + (2 - k) s + 1)` for the low-pass and
/// `s (s + 1) / (s^2 + (2 - k) s + 1)` for the high-pass, which falls at 6 dB an octave under its
/// peak. `along` is the frequency over the cutoff, both warped as the trapezoidal one-poles warp
/// them.
pub fn response(high_pass: bool, feedback: f32, along: f32) -> f32 {
    let squared = along * along;
    let real = 1.0 - squared;
    let imaginary = (2.0 - feedback) * along;
    let top = if high_pass {
        squared * squared + squared
    } else {
        1.0
    };
    top / (real * real + imaginary * imaginary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_signal_meets_the_worked_out_response() {
        // A trapezoidal filter is its analog model on a warped frequency axis, so a sine at the
        // cutoff comes out as the model says at `along` of 1.
        let rate = 96_000.0_f32;
        let cutoff = 1_000.0_f32;
        let g = (std::f32::consts::PI * cutoff / rate).tan();
        let coefficient = g / (1.0 + g);
        for high_pass in [false, true] {
            for feedback in [0.0, 1.0, 1.8] {
                let mut filter = Korg35::default();
                let (mut power_in, mut power_out) = (0.0_f64, 0.0_f64);
                let frames = rate as usize;
                for frame in 0..frames {
                    let input =
                        0.001 * (std::f32::consts::TAU * cutoff * frame as f32 / rate).sin();
                    let output = if high_pass {
                        filter.high_pass(input, coefficient, feedback)
                    } else {
                        filter.low_pass(input, coefficient, feedback)
                    };
                    if frame > frames / 2 {
                        power_in += f64::from(input * input);
                        power_out += f64::from(output * output);
                    }
                }
                let measured = 10.0 * (power_out / power_in).log10() as f32;
                let worked_out = 10.0 * response(high_pass, feedback, 1.0).log10();
                assert!(
                    (measured - worked_out).abs() < 0.1,
                    "{high_pass} {feedback}: {measured} and {worked_out}"
                );
            }
        }
    }
}
