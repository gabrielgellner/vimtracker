//! Phase distortion, from Mutable Instruments' Plaits: a sine read through a phase that a synced,
//! leaning triangle bends, for resonant, Casio-like sweeps. Ported to Rust from
//! `phase_distortion_engine.cc`, with its main output, the synced one.
//!
//! Copyright 2021 Emilie Gillet.
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

use super::fm::ratio;
use super::sine_table::SINE;
use super::variable_shape::VariableShape;
use super::virtual_analog::{BLOCK, Settings};

/// It runs at twice the rate, then drops every other frame, as Plaits does.
const OVERSAMPLING: usize = 2;
/// How loud it is against the other machines. Its output is a sine however bent, so about 0.7
/// RMS, where the virtual analog voice's half-level saw is about a third of that; this brings it
/// to within a decibel at 220 Hz.
const LEVEL: f32 = 0.35;

/// A sine of a phase in turns, read from Plaits' table. Any phase wraps.
#[inline]
fn sine(phase: f32) -> f32 {
    let index = (phase - phase.floor()) * 512.0;
    let whole = (index as usize).min(511);
    let fraction = index - whole as f32;
    let (a, b) = (SINE[whole], SINE[whole + 1]);
    a + (b - a) * fraction
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct PhaseDistortion {
    shaper: VariableShape,
    output: [f32; BLOCK],
    at: usize,
}

impl Default for PhaseDistortion {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseDistortion {
    pub fn new() -> Self {
        Self {
            shaper: VariableShape::new(),
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.clamp(1e-5, 0.49), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let harmonics = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let morph = settings.morph.clamp(0.0, 1.0);
        let f0 = frequency / OVERSAMPLING as f32;
        let modulator_f = (f0 * (ratio(harmonics) / 12.0).exp2()).min(0.25);
        let pw = 0.5 + morph * 0.49;
        // Less distortion as the modulator rises, which would otherwise alias.
        let amount = 8.0 * timbre * timbre * (1.0 - modulator_f * 3.8);

        let mut phase = [0.0; BLOCK * OVERSAMPLING];
        self.shaper
            .render::<true, true>(f0, modulator_f, pw, 0.0, amount, &mut phase);
        let (pairs, _) = phase.as_chunks::<OVERSAMPLING>();
        for (out, pair) in self.output.iter_mut().zip(pairs) {
            // Plaits drops the extra frames plainly, by averaging each pair.
            *out = 0.5 * sine(pair[0] + 0.25) + 0.5 * sine(pair[1] + 0.25);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::virtual_analog::VirtualAnalog;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = PhaseDistortion::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.5,
            morph: 0.25,
            ..Settings::default()
        }
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// A rough measure of energy near the top of the spectrum: how much each frame jumps.
    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        total / samples.len() as f32
    }

    fn differs(a: &[f32], b: &[f32]) -> f32 {
        let total: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
        (total / a.len() as f32).sqrt()
    }

    #[test]
    fn it_sounds() {
        let out = run(220.0, plain(), 48_000);
        assert!(rms(&out[24_000..]) > 0.1, "{}", rms(&out[24_000..]));
    }

    #[test]
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE, Settings::default()))
            .collect();
        for settings in [plain(), Settings::default()] {
            let decibels = 20.0 * (rms(&run(220.0, settings, 48_000)) / rms(&va)).log10();
            assert!(decibels.abs() < 3.0, "{decibels} dB at {settings:?}");
        }
    }

    #[test]
    fn with_no_timbre_it_is_a_plain_sine() {
        let smooth = run(
            220.0,
            Settings {
                timbre: 0.0,
                ..plain()
            },
            4_800,
        );
        assert!(roughness(&smooth) < roughness(&run(220.0, plain(), 4_800)));
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let base = run(220.0, plain(), 4_800);
        for settings in [
            Settings {
                harmonics: 0.8,
                ..plain()
            },
            Settings {
                timbre: 0.9,
                ..plain()
            },
            Settings {
                morph: 0.9,
                ..plain()
            },
        ] {
            let other = run(220.0, settings, 4_800);
            assert!(differs(&base, &other) > 0.05, "{settings:?}");
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 0.75, 1.0] {
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
    fn moving_settings_while_it_plays_stays_in_range() {
        let mut voice = PhaseDistortion::new();
        for frame in 0..48_000 {
            let sweep = (frame as f32 / 48_000.0 * 20.0).sin() * 0.5 + 0.5;
            let settings = Settings {
                harmonics: sweep,
                timbre: sweep,
                morph: 1.0 - sweep,
                ..Settings::default()
            };
            let sample = voice.next((55.0 + sweep * 2_000.0) / RATE, settings);
            assert!(sample.is_finite() && sample.abs() <= 1.0, "{sample}");
        }
    }
}
