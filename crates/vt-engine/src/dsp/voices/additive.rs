//! Additive synthesis of 24 harmonics whose levels follow a spectrum with bumps, from Mutable
//! Instruments' Plaits. Ported to Rust from `additive_engine.cc` and
//! `oscillator/harmonic_oscillator.h`, with its main output. Its other output, the organ's eight
//! drawbar harmonics, is left out.
//!
//! Copyright 2016 Emilie Gillet.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use super::virtual_analog::{BLOCK, Moving, Settings};
use crate::dsp::blocks::plaits_parts::interpolate;
use crate::dsp::blocks::sine_table::SINE;

/// How many harmonics are summed.
const HARMONICS: usize = 24;
/// The recurrence that makes each harmonic from the last drifts, so it starts afresh here.
const BATCH: usize = 12;
/// How quickly the spectrum follows the settings, each block. Plaits uses 0.001 over its blocks of
/// 12 frames, so this keeps its timing over ours.
const SMOOTHING: f32 = 0.001 * BLOCK as f32 / 12.0;
/// How loud it is against the other machines. The harmonics' levels always add up to one, so a
/// single sine is as loud as it gets, at -3 dB RMS, and a wide spectrum is quieter. At 220 Hz with
/// harmonics 0, timbre 0.3 and morph 0.5 it measures -6.5 dB unscaled, against -12.3 dB for the
/// virtual analog voice at its defaults, so halving it brings the two within a decibel.
const LEVEL: f32 = 0.5;

/// A sine of a phase in turns, wrapped to one turn.
#[inline]
fn sine(phase: f32) -> f32 {
    interpolate(&SINE, phase - phase.floor(), 512.0)
}

/// A sine of a phase in turns already in `0..1.25`.
#[inline]
fn sine_no_wrap(phase: f32) -> f32 {
    interpolate(&SINE, phase, 512.0)
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Additive {
    phase: f32,
    frequency: f32,
    /// Each harmonic's share of the sound, smoothed and adding up to one.
    spectrum: [f32; HARMONICS],
    /// Where each harmonic's level was left at the end of the last block.
    levels: [f32; HARMONICS],
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Additive {
    fn default() -> Self {
        Self::new()
    }
}

impl Additive {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 0.0,
            spectrum: [0.0; HARMONICS],
            levels: [0.0; HARMONICS],
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            // Written so a bad frequency falls to silence rather than through.
            let f0 = if frequency > 0.0 {
                frequency.min(0.5)
            } else {
                0.0
            };
            self.render(f0, settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample * LEVEL
    }

    /// Moves the spectrum towards a peak at `centroid`, falling away by `slope` either side, with
    /// `bumps` ripples in it. The smoothing runs on the normalised levels, which isn't a proper
    /// low-pass filter, but Plaits says it sounds better than the proper ones it tried.
    fn update_spectrum(&mut self, centroid: f32, slope: f32, bumps: f32) {
        let n = HARMONICS as f32 - 1.0;
        let margin = (1.0 / slope - 1.0) / (1.0 + bumps);
        let center = centroid * (n + margin) - 0.5 * margin;
        let mut sum = 0.001;
        for (i, level) in self.spectrum.iter_mut().enumerate() {
            let order = (i as f32 - center).abs() * slope;
            let mut gain = 1.0 - order;
            gain += gain.abs();
            gain *= gain;
            gain *= 1.0 + sine(0.25 + order * bumps);
            gain *= gain;
            gain *= gain;
            *level += SMOOTHING * (gain - *level);
            sum += *level;
        }
        let norm = 1.0 / sum;
        for level in self.spectrum.iter_mut() {
            *level *= norm;
        }
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let raw_bumps = settings.harmonics.clamp(0.0, 1.0);
        let centroid = settings.timbre.clamp(0.0, 1.0);
        let raw_slope = (1.0 - 0.6 * raw_bumps) * settings.morph.clamp(0.0, 1.0);
        let slope = 0.01 + 1.99 * raw_slope * raw_slope * raw_slope;
        let bumps = 16.0 * raw_bumps * raw_bumps;
        self.update_spectrum(centroid, slope, bumps);

        if self.fresh {
            self.frequency = f0;
            self.fresh = false;
        }
        let mut fm = Moving::towards(self.frequency, f0);
        // Each harmonic fades out as it nears half the rate, and is gone above it.
        let mut targets = [0.0; HARMONICS];
        for (i, (target, level)) in targets.iter_mut().zip(self.spectrum).enumerate() {
            let f = (f0 * (i + 1) as f32).min(0.5);
            *target = level * (1.0 - 2.0 * f);
        }
        let mut am = [Moving::towards(0.0, 0.0); HARMONICS];
        for ((am, &state), &target) in am.iter_mut().zip(&self.levels).zip(&targets) {
            *am = Moving::towards(state, target);
        }

        for out in self.output.iter_mut() {
            self.phase += fm.next();
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let phase = self.phase;
            let two_x = 2.0 * sine_no_wrap(phase);
            let (low, high) = am.split_at_mut(BATCH);

            // Each harmonic from the two below it, by Chebyshev's recurrence.
            let (mut previous, mut current) = (1.0, two_x * 0.5);
            let mut sum = 0.0;
            for am in low {
                sum += am.next() * current;
                (previous, current) = (current, two_x * current - previous);
            }
            let k = (BATCH + 1) as f32;
            let (mut previous, mut current) = (sine(phase * (k - 1.0) + 0.25), sine(phase * k));
            for am in high {
                sum += am.next() * current;
                (previous, current) = (current, two_x * current - previous);
            }
            *out = sum;
        }
        self.levels = targets;
        self.frequency = f0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Additive::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.0,
            timbre: 0.3,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        total / samples.len() as f32
    }

    /// How much two renders differ, against the first's own energy.
    fn difference(a: &[f32], b: &[f32]) -> f32 {
        let apart: Vec<f32> = a.iter().zip(b).map(|(a, b)| a - b).collect();
        energy(&apart) / energy(a)
    }

    #[test]
    fn it_keeps_sounding() {
        let out = run(220.0, plain(), 48_000);
        assert!(energy(&out[43_200..]) > 1e-3, "{}", energy(&out[43_200..]));
    }

    #[test]
    fn timbre_moves_the_peak_up_the_harmonics() {
        let low = run(220.0, plain(), 24_000);
        let high = run(
            220.0,
            Settings {
                timbre: 0.9,
                ..plain()
            },
            24_000,
        );
        let brightness = |s: &[f32]| roughness(&s[12_000..]) / energy(&s[12_000..]);
        assert!(
            brightness(&high) > brightness(&low) * 4.0,
            "{} {}",
            brightness(&high),
            brightness(&low)
        );
    }

    #[test]
    fn harmonics_and_morph_change_the_sound() {
        let base = run(220.0, plain(), 24_000);
        for changed in [
            Settings {
                harmonics: 0.7,
                ..plain()
            },
            Settings {
                morph: 0.9,
                ..plain()
            },
        ] {
            let out = run(220.0, changed, 24_000);
            let apart = difference(&base[12_000..], &out[12_000..]);
            assert!(apart > 0.1, "{apart} for {changed:?}");
        }
    }

    #[test]
    fn harmonics_above_half_the_rate_are_dropped_rather_than_folded_back() {
        // A narrow peak on the top harmonic of a high note, far above half the rate.
        let settings = Settings {
            timbre: 1.0,
            morph: 1.0,
            ..plain()
        };
        let low = run(
            6_000.0,
            Settings {
                timbre: 0.0,
                ..settings
            },
            24_000,
        );
        let high = run(6_000.0, settings, 24_000);
        assert!(
            energy(&high[12_000..]) < energy(&low[12_000..]) * 0.01,
            "{} {}",
            energy(&high[12_000..]),
            energy(&low[12_000..])
        );
        assert!(high.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.5, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                            ..Settings::default()
                        };
                        for sample in run(hertz, settings, 4_800) {
                            assert!(
                                sample.is_finite() && sample.abs() <= 1.0,
                                "{sample} at {hertz} {settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn settings_moving_while_it_plays_stay_in_range() {
        let mut voice = Additive::new();
        for frame in 0..48_000 {
            let t = frame as f32 / 48_000.0;
            let settings = Settings {
                harmonics: t,
                timbre: 1.0 - t,
                morph: (t * 7.0).fract(),
                ..Settings::default()
            };
            let sample = voice.next((50.0 + t * 4_000.0) / RATE, settings);
            assert!(sample.is_finite() && sample.abs() <= 1.0, "{sample}");
        }
    }
}
