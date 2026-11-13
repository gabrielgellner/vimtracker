//! The OB-X's two-pole filter, after OB-Xd's. Ported to Rust from OB-Xf's `src/engine/Filter.h`
//! (`diodePairResistanceApprox`, `resolveFeedback2Pole` and `apply2Pole`), the scalar form of
//! sst-filters' `OBXDFilter.h`. It's taken out of OB-Xf's voice, with its own cutoff's `tan`
//! worked out by the track, as the other filters' are, and its mode morph made a choice of four.
//!
//! It's a trapezoidal state-variable filter whose damping depends on how loud it is: a Taylor fit
//! of a slightly mismatched diode pair, worked out from the band state a sample before, adds to the
//! damping as the band grows. So its resonance limits itself with no clipper, and the fit's odd
//! terms make it lopsided, which adds a little of the even harmonics. OB-Xd's "push" takes 0.035
//! more from the damping, so at full resonance a quiet filter is a little under no damping at all,
//! and it rings by itself, in tune, as loud as the diode pair lets it. Here the last of `Res`
//! pushes it.
//!
//! OB-Xd was originally written by Vadim Filatov, and then a version was released under the GPL3
//! at <https://github.com/reales/OB-Xd>. Subsequently, the product was continued by DiscoDSP and
//! the copyright holders as an excellent closed source product.
//!
//! OB-Xf is a successor to OB-Xd version 2.11. Copyright 2013-2025 by the authors as indicated in
//! the original release, and subsequent authors as per GitHub transaction log.
//!
//! OB-Xf is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later). Source
//! code is available at <https://github.com/surge-synthesizer/OB-Xf>

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::flush::tiny_by_fields;

/// The resonance at full `Res`: 1 is where a quiet filter has no damping left, and the rest is
/// OB-Xd's push past it, so it rings by itself.
pub const MOST_RESONANCE: f32 = 1.035;
/// How the band state is scaled into the diode pair's fit, as OB-Xd has it.
const DIODE_SCALE: f32 = 0.0876;
/// The furthest the fit is read, where it's already 60 times its quiet value, which no sound
/// reaches. Past it the damping holds, so a runaway can't take it to an infinity.
const DIODE_RANGE: f32 = 8.0;

/// The diode pair's resistance, as a share of its quiet value, at `x`.
#[inline]
fn diode_pair(x: f32) -> f32 {
    (((0.010_359_2 * x + 0.009_208_33) * x + 0.185) * x + 0.05) * x + 1.0
}

/// The filter's state: its two integrators.
#[derive(Debug, Clone, Copy, Default)]
pub struct ObX {
    band: f32,
    low: f32,
}

tiny_by_fields!(ObX { band, low });

impl ObX {
    /// One sample's low-pass, band-pass and high-pass, tuned by `g`, `tan(pi f)` for its cutoff `f`
    /// as a fraction of the rate, with `resonance` from 0 to [`MOST_RESONANCE`]. A NaN or an
    /// infinity going in is taken as silence, as it would stay in the loop for good.
    #[inline]
    pub fn next(&mut self, input: f32, g: f32, resonance: f32) -> [f32; 3] {
        let input = if input.is_finite() { input } else { 0.0 };
        let level = within(self.band * DIODE_SCALE, -DIODE_RANGE, DIODE_RANGE);
        let damping = 2.0 * (diode_pair(level) - resonance);
        // The loop solved for the high-pass within the sample, with the damping held.
        let high = (input - (damping + g) * self.band - self.low) / (1.0 + g * (damping + g));
        let band = g * high + self.band;
        self.band = g * high + band;
        let low = g * band + self.low;
        self.low = g * band + low;
        [low, band, high]
    }
}

/// How much the band-pass is turned up at a resonance: by its damping while that's over 1, so with
/// no resonance it peaks at 1, and not at all past that, where it peaks as loud as the low-pass is
/// at `Cut`.
#[inline]
pub fn band_makeup(resonance: f32) -> f32 {
    (2.0 * (1.0 - resonance)).max(1.0)
}

/// How much it lets through at a frequency, as a power, for a quiet signal: the state-variable
/// filter with damping `2 (1 - r)`, its low-pass `1 / (s^2 + k s + 1)`, band-pass `s / (...)` and
/// high-pass `s^2 / (...)` mixed by `weights`, the band-pass made up. `along` is the frequency over
/// the cutoff, both warped as the trapezoidal integrators warp them.
pub fn response(weights: [f32; 3], resonance: f32, along: f32) -> f32 {
    let squared = along * along;
    let damping = 2.0 * (1.0 - resonance);
    let real = weights[0] - weights[2] * squared;
    let imaginary = weights[1] * band_makeup(resonance) * along;
    let below = 1.0 - squared;
    let across = damping * along;
    (real * real + imaginary * imaginary) / (below * below + across * across)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 96_000.0;

    fn g(hertz: f32) -> f32 {
        (std::f32::consts::PI * hertz / RATE).tan()
    }

    #[test]
    fn a_quiet_signal_meets_the_worked_out_response() {
        let cutoff = 1_000.0;
        for (weights, name) in [
            ([1.0, 0.0, 0.0], "low"),
            ([0.0, 1.0, 0.0], "band"),
            ([0.0, 0.0, 1.0], "high"),
        ] {
            for resonance in [0.0, 0.5, 0.9] {
                for hertz in [250.0, 1_000.0, 4_000.0] {
                    let mut filter = ObX::default();
                    let (mut power_in, mut power_out) = (0.0_f64, 0.0_f64);
                    let frames = RATE as usize;
                    for frame in 0..frames {
                        let input =
                            0.001 * (std::f32::consts::TAU * hertz * frame as f32 / RATE).sin();
                        let out = filter.next(input, g(cutoff), resonance);
                        let makeup = band_makeup(resonance);
                        let output = weights[0] * out[0]
                            + weights[1] * makeup * out[1]
                            + weights[2] * out[2];
                        if frame > frames / 2 {
                            power_in += f64::from(input * input);
                            power_out += f64::from(output * output);
                        }
                    }
                    let measured = 10.0 * (power_out / power_in).log10() as f32;
                    let along = g(hertz) / g(cutoff);
                    let worked_out = 10.0 * response(weights, resonance, along).log10();
                    assert!(
                        (measured - worked_out).abs() < 0.05,
                        "{name} at {resonance}, {hertz} Hz: {measured} and {worked_out}"
                    );
                }
            }
        }
    }

    /// Where it rings at full resonance after a click, in cents off `cutoff`, and how loud.
    fn ringing(cutoff: f32) -> (f32, f32) {
        let mut filter = ObX::default();
        filter.next(1e-3, g(cutoff), MOST_RESONANCE);
        let frames = RATE as usize * 2;
        let out: Vec<f32> = (0..frames)
            .map(|_| filter.next(0.0, g(cutoff), MOST_RESONANCE)[0])
            .collect();
        let last = &out[frames / 2..];
        let rising: Vec<f32> = last
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] <= 0.0 && pair[1] > 0.0)
            .map(|(frame, pair)| frame as f32 + pair[0] / (pair[0] - pair[1]))
            .collect();
        let span = rising[rising.len() - 1] - rising[0];
        let pitch = (rising.len() - 1) as f32 * RATE / span;
        let peak = last.iter().fold(0.0_f32, |high, x| high.max(x.abs()));
        (1200.0 * (pitch / cutoff).log2(), peak)
    }

    #[test]
    fn at_full_resonance_it_rings_in_tune_by_itself() {
        // As the DSP parts review simulated it at 96 kHz: -1.3 cents at 100 Hz, 0 at 1 kHz, +5.3
        // at 5 kHz, +11.4 at 10 kHz and +17.3 at 16 kHz, at a level of about 5.8.
        for (cutoff, expected) in [
            (100.0, -1.3),
            (1_000.0, 0.0),
            (5_000.0, 5.3),
            (10_000.0, 11.4),
            (16_000.0, 17.3),
        ] {
            let (cents, peak) = ringing(cutoff);
            assert!(
                (cents - expected).abs() < 1.0,
                "{cutoff} Hz rang {cents} cents off"
            );
            assert!((5.0..6.5).contains(&peak), "{cutoff} Hz rang at {peak}");
        }
    }

    #[test]
    fn without_the_push_it_dies_away() {
        let mut filter = ObX::default();
        filter.next(1.0, g(1_000.0), 1.0);
        let mut peak = 0.0_f32;
        for frame in 0..RATE as usize * 4 {
            let low = filter.next(0.0, g(1_000.0), 1.0)[0];
            if frame > RATE as usize * 3 {
                peak = peak.max(low.abs());
            }
        }
        assert!(peak < 1.0, "{peak}");
    }

    #[test]
    fn a_nan_going_in_is_taken_as_silence() {
        let mut filter = ObX::default();
        for frame in 0..1_000 {
            let input = if frame == 100 { f32::NAN } else { 0.1 };
            let out = filter.next(input, g(1_000.0), MOST_RESONANCE);
            assert!(out.iter().all(|x| x.is_finite()), "{frame}: {out:?}");
        }
    }
}
