//! Vital's `Dirty` filter, a Sallen-Key low-pass with saturating integrators in its feedback loop.
//! Ported to Rust from `src/synthesis/filters/dirty_filter.{h,cpp}`, with the one-pole filter and
//! `tanh` it shares with the diode ladder, taken out of Vital's SIMD and processor types. It has
//! Vital's 24 dB and 12 dB styles, with each two-pole's low-pass, band-pass and high-pass mixed as
//! the track's mode asks, rather than blended by a knob. Its one-poles take the exact
//! trapezoidal coefficient, `g / (1 + g)`, in place of Vital's cheaper warp of it, which runs flat
//! at high cutoffs.
//!
//! A two-pole low-pass comes first. Then the loop: two more one-poles feed a sum, which saturates
//! and is the output. The sum goes round through a saturating one-pole low-pass, and what that
//! takes out, through a second, which makes a band-pass, comes back into the sum, turned up by the
//! resonance. It self-oscillates at the cutoff from a resonance of about 2. `Drive` turns up only
//! what comes straight into the sum, not what comes round, and less as the resonance rises, so a
//! driven sound grits up while its peak survives, and drive adds a little resonance too.
//!
//! Vital's 24 dB style feeds its first two-pole back on itself through states its one-poles never
//! write, so that feedback is always zero there, and it's left out here, along with the gain that
//! was meant to balance it, which only lifted high cutoffs by a dB or two.
//!
//! Its loop's one-poles saturate, which slows them as it rings louder, so once it rang by itself it
//! was up to 60 cents flat. Here the loop is tuned higher by how far its resonance goes past where
//! it starts to ring, which keeps that within 3 cents of the cutoff. Under that, nothing changes.
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

use super::diode_filter::{Complex, OnePole, tanh};
use crate::dsp::blocks::flush::tiny_by_fields;

/// The loop's resonance at no `Res` and at full. It self-oscillates from about 2.
const LEAST_RESONANCE: f32 = 0.1;
const MOST_RESONANCE: f32 = 2.15;
/// How much `Drive` adds to the loop's resonance, at full.
pub const DRIVE_RESONANCE: f32 = 0.05;
/// How far the loop's input is turned up, to drive its saturation, and its output turned down.
const SATURATION_BOOST: f32 = 1.4;
/// What a quiet sound comes through at with no resonance, before this evens it out.
const QUIET_LEVEL: f32 = (1.0 / (1.0 + LEAST_RESONANCE * LEAST_RESONANCE * 0.5) + LEAST_RESONANCE)
    / (1.0 + LEAST_RESONANCE * LEAST_RESONANCE);

/// The slope of Vital's `tanh` at 0, which is a hair over 1.
const TANH_SLOPE: f32 = 2.455_507_5 / 2.445_066_3;

/// Vital's `quickTanh`: about `tanh` up to 2, and past that it keeps rising, slowly, so the loop's
/// one-poles squash rather than clip.
#[inline]
fn quick_tanh(value: f32) -> f32 {
    let square = value * value;
    value / (square / (3.0 + square * 0.2) + 1.0)
}

/// How much higher the loop is tuned for each fraction its resonance goes past where it starts to
/// ring, at the lowest cutoffs. Its one-poles saturate, which slows them, so it rings flat as it
/// grows louder, and more so at high cutoffs.
const RINGING_PULL: f32 = 0.4;

/// What the filter runs with that depends only on its cutoff and resonance, worked out as often
/// as the track's other filter coefficients are.
#[derive(Debug, Clone, Copy)]
pub struct Tuning {
    /// The one-poles' coefficient, `g / (1 + g)`, and its `g`.
    coefficient: f32,
    analog: f32,
    /// The loop's resonance before any drive.
    resonance: f32,
    /// One over where it starts to ring by itself.
    over_ringing: f32,
    /// How much higher the loop is tuned for each fraction past that.
    pull: f32,
}

impl Tuning {
    /// From the one-poles' coefficient, for a cutoff at twice the rate, and `Res` from 0 to 1. The
    /// loop's resonance is on a square root, as Vital has it, and eased off a little at high
    /// cutoffs, where the loop would otherwise ring more.
    pub fn new(coefficient: f32, resonance: f32) -> Self {
        let eased = resonance.clamp(0.0, 1.0).sqrt() / (0.5 * coefficient + 0.97).max(1.0);
        let rest = 1.0 - coefficient;
        Self {
            coefficient,
            analog: coefficient / rest,
            resonance: LEAST_RESONANCE + (MOST_RESONANCE - LEAST_RESONANCE) * eased.clamp(0.0, 1.0),
            over_ringing: 1.0 / ringing_resonance(coefficient),
            pull: RINGING_PULL / (rest * rest * rest),
        }
    }

    /// Visit each number alongside another tuning's, so a track can ramp from one to the next.
    #[inline]
    pub(crate) fn each(&mut self, other: &Self, mut visit: impl FnMut(&mut f32, f32)) {
        visit(&mut self.coefficient, other.coefficient);
        visit(&mut self.analog, other.analog);
        visit(&mut self.resonance, other.resonance);
        visit(&mut self.over_ringing, other.over_ringing);
        visit(&mut self.pull, other.pull);
    }
}

/// Where the loop starts to ring by itself, as a resonance, at a one-pole coefficient. Its poles'
/// distance from the centre, squared, is `a^2 + 2 G B (1 - 2G + 2G^2)`, which is 1 here.
fn ringing_resonance(coefficient: f32) -> f32 {
    let g = coefficient;
    let feedback = 2.0 * (1.0 - g) / ((1.0 - 2.0 * g + 2.0 * g * g) * TANH_SLOPE);
    let solve = feedback * (1.0 + g);
    solve / (1.0 + solve * g * (1.0 - g))
}

/// What the filter runs with for a sample.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// The coefficient of the one-poles before the loop, and of the loop's, which is raised once it
    /// rings by itself.
    coefficient: f32,
    loop_coefficient: f32,
    /// What the loop's input is turned up by, so a high resonance doesn't overload it.
    entry: f32,
    /// How much of what comes straight in reaches the sum.
    drive: f32,
    /// How much of what comes round does.
    feedback: f32,
}

impl Settings {
    /// From the tuning, `Drive` from 0 to 1, and the gain it turns what comes straight in up by.
    #[inline]
    pub fn new(tuning: &Tuning, drive: f32, gain: f32) -> Self {
        let resonance = tuning.resonance + drive * DRIVE_RESONANCE;
        let past = (resonance * tuning.over_ringing - 1.0).max(0.0);
        let raised = tuning.analog * (1.0 + tuning.pull * past);
        let g = raised / (1.0 + raised);
        let squared = resonance * resonance;
        // Vital's solve for the feedback within the sample, close to exact for low cutoffs.
        let solve = 1.0 / ((1.0 - resonance * (g - g * g)) * (1.0 + g));
        Self {
            coefficient: tuning.coefficient,
            loop_coefficient: g,
            entry: SATURATION_BOOST / (squared + 1.0),
            drive: gain.max(1.0) / (squared * 0.5 + 1.0),
            feedback: resonance * solve,
        }
    }
}

/// A `Dirty` filter's state.
#[derive(Debug, Clone, Copy, Default)]
pub struct DirtyFilter {
    /// The two-pole before the loop, then the two at its input.
    before: [OnePole; 2],
    entry: [OnePole; 2],
    /// The loop's low-pass, and the one after it that makes a band-pass.
    low: OnePole,
    band: OnePole,
}

tiny_by_fields!(DirtyFilter {
    before,
    entry,
    low,
    band
});

/// What a two-pole gives out: its input, its first one-pole and its second, `1`, `L` and `L^2`,
/// weighted as a mode has them.
#[inline]
fn mixed(weights: &[f32; 3], input: f32, first: f32, second: f32) -> f32 {
    weights[0] * input + weights[1] * first + weights[2] * second
}

impl DirtyFilter {
    /// One sample, with the two-pole before the loop and the one where it goes in each mixed by
    /// its weights. A quiet sound with no resonance or drive comes out at the level it went in
    /// through the low-pass, and the output stays within ±0.8 there.
    #[inline]
    pub fn next(
        &mut self,
        input: f32,
        settings: &Settings,
        before: &[f32; 3],
        entry: &[f32; 3],
    ) -> f32 {
        let g = settings.coefficient;
        let [first, second] = &mut self.before;
        let one = first.tick_plain(input, g);
        let two = second.tick_plain(one, g);
        let before = mixed(before, input, one, two);
        let [first, second] = &mut self.entry;
        let entered = settings.entry * before;
        let one = first.tick_plain(entered, g);
        let two = second.tick_plain(one, g);
        let passed = mixed(entry, entered, one, two);
        // What comes round, from the loop's states, and what's known of this sample's input.
        let g = settings.loop_coefficient;
        let round = self.band.saturated + passed + g * (passed - self.low.saturated);
        let sum = tanh(settings.drive * passed + settings.feedback * round);
        self.low.tick(sum, g, quick_tanh);
        self.band.tick(sum - self.low.current, g, quick_tanh);
        sum / (SATURATION_BOOST * QUIET_LEVEL)
    }
}

/// How much it lets through at a frequency, as a power, for a quiet signal, from its settings with
/// no drive's gain, its two-poles' weights, and the frequency as a fraction of the filter's rate.
/// Its saturations do nothing to a quiet signal, so it's two two-poles of trapezoidal one-poles,
/// `L = G (z + 1) / (z - a)` with `a = 1 - 2G`, each mixed from `1`, `L` and `L^2`, into a loop
/// that hears its states a sample late:
/// `A (z - a)^2 / ((z - a)^2 - 2 G B ((1 - 2G) z - (1 - 2G + 2G^2)))`, with `B` what comes round
/// and `A` what comes straight in, both through the slope of the sum's `tanh`.
pub fn response(settings: &Settings, before: &[f32; 3], entry: &[f32; 3], frequency: f32) -> f32 {
    let turn = std::f32::consts::TAU * frequency.min(0.499);
    let z = Complex(turn.cos(), turn.sin());
    let one_pole = |g: f32| {
        let z_less_a = z.add(Complex(2.0 * g - 1.0, 0.0));
        (z.add(Complex(1.0, 0.0)).scale(g).over(z_less_a), z_less_a)
    };
    let (passed, _) = one_pole(settings.coefficient);
    let squared = passed.times(passed);
    let two_pole = |weights: &[f32; 3]| {
        Complex(weights[0], 0.0)
            .add(passed.scale(weights[1]))
            .add(squared.scale(weights[2]))
    };
    let stages = two_pole(before).times(two_pole(entry));
    let g = settings.loop_coefficient;
    let (_, z_less_a) = one_pole(g);
    let (drive, feedback) = (settings.drive * TANH_SLOPE, settings.feedback * TANH_SLOPE);
    let straight = drive + feedback * (1.0 + g);
    let back = z
        .scale(1.0 - 2.0 * g)
        .add(Complex(-(1.0 - 2.0 * g + 2.0 * g * g), 0.0))
        .scale(2.0 * g * feedback);
    let around = z_less_a.times(z_less_a);
    let looped = around.scale(straight).over(around.add(back.scale(-1.0)));
    let gain = settings.entry / (SATURATION_BOOST * QUIET_LEVEL);
    stages.times(looped).scale(gain).power()
}

/// How much louder its low-pass is in its bass than at its cutoff, with no drive, where a
/// band-pass's peak is. Each of its four one-poles passes 0.71 at the cutoff.
pub fn band_makeup(tuning: &Tuning) -> f32 {
    let settings = Settings::new(tuning, 0.0, 1.0);
    let at_cutoff = tuning.analog.atan() / std::f32::consts::PI;
    let through = [1.0, 0.0, 0.0];
    let bass = response(&settings, &through, &through, 0.0);
    let peak = response(&settings, &through, &through, at_cutoff);
    4.0 * (bass / peak.max(1e-12)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each two-pole's low-pass, as Vital's 24 dB low-pass has it.
    const LOW: [f32; 3] = [0.0, 0.0, 1.0];

    #[test]
    fn a_quiet_signal_meets_the_worked_out_response() {
        let rate = 96_000.0_f32;
        for cutoff in [200.0, 1_000.0, 8_000.0] {
            let g = (std::f32::consts::PI * cutoff / rate).tan();
            let coefficient = g / (1.0 + g);
            // From about `Res DB` it rings by itself.
            for res in [0.0, 0.5, 0.8] {
                let settings = Settings::new(&Tuning::new(coefficient, res), 0.0, 1.0);
                for hertz in [cutoff * 0.5, cutoff, cutoff * 2.0] {
                    let mut filter = DirtyFilter::default();
                    let (mut power_in, mut power_out) = (0.0_f64, 0.0_f64);
                    let frames = rate as usize;
                    for frame in 0..frames {
                        let input =
                            0.001 * (std::f32::consts::TAU * hertz * frame as f32 / rate).sin();
                        let output = filter.next(input, &settings, &LOW, &LOW);
                        if frame > frames / 2 {
                            power_in += f64::from(input * input);
                            power_out += f64::from(output * output);
                        }
                    }
                    let measured = 10.0 * (power_out / power_in).log10() as f32;
                    let worked_out = 10.0 * response(&settings, &LOW, &LOW, hertz / rate).log10();
                    assert!(
                        (measured - worked_out).abs() < 0.1,
                        "{cutoff} {res} {hertz}: {measured} and {worked_out}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_quiet_sound_comes_through_at_its_level_with_no_resonance() {
        let g = (std::f32::consts::PI * 1_000.0 / 96_000.0).tan();
        let coefficient = g / (1.0 + g);
        let settings = Settings::new(&Tuning::new(coefficient, 0.0), 0.0, 1.0);
        let low = response(&settings, &LOW, &LOW, 10.0 / 96_000.0);
        assert!((10.0 * low.log10()).abs() < 0.05, "{low}");
    }
}
