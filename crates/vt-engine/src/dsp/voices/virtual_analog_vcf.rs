//! A saw or square with a sub-oscillator through a resonant low-pass filter, from the filtered
//! virtual analog model of Mutable Instruments' Plaits. Ported to Rust from
//! `virtual_analog_vcf_engine.cc` and `stmlib/dsp/filter.h`, with its main output, the low-pass.
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

use super::drums::soft_clip;
use super::variable_shape::VariableShape;
use super::virtual_analog::{BLOCK, Moving, Settings};
use crate::dsp::blocks::plaits_parts::Svf;

/// How loud it is against the other machines. Its filter and clipping leave it near the virtual
/// analog voice's level already: at 220 Hz with the filter half open, a saw is a decibel under it
/// and a square two or three over, so this trims it a little. Resonance at either end of `Harm`
/// adds up to 10 dB more, as it does in Plaits.
const LEVEL: f32 = 0.9;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct VirtualAnalogVcf {
    oscillator: VariableShape,
    sub_oscillator: VariableShape,
    /// Two filters, the second making it four-pole when it's mixed in.
    filters: [Svf; 2],
    sub_gain: f32,
    cutoff: f32,
    stage2_gain: f32,
    q: f32,
    gain: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for VirtualAnalogVcf {
    fn default() -> Self {
        Self::new()
    }
}

impl VirtualAnalogVcf {
    pub fn new() -> Self {
        Self {
            oscillator: VariableShape::new(),
            sub_oscillator: VariableShape::new(),
            filters: [Svf::default(); 2],
            sub_gain: 0.0,
            cutoff: 0.0,
            stage2_gain: 0.0,
            q: 0.0,
            gain: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
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

    fn render(&mut self, f0: f32, settings: Settings) {
        let harmonics = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let morph = settings.morph.clamp(0.0, 1.0);

        // From a saw at a quarter, to a square at a half, to a narrow pulse and back to a square,
        // with the sub-oscillator coming in at either end.
        let shape = ((morph - 0.25) * 2.0 + 0.5).clamp(0.5, 1.0);
        let pw = if morph > 0.75 {
            2.5 - morph * 2.0
        } else {
            (morph - 0.5) * 2.0 + 0.5
        };
        let pw = pw.clamp(0.5, 0.98);
        let sub_gain = ((morph - 0.5).abs() - 0.3).max(0.0) * 5.0;

        let mut main = [0.0; BLOCK];
        let mut sub = [0.0; BLOCK];
        self.oscillator
            .render::<false, false>(0.0, f0, pw, shape, 0.0, &mut main);
        self.sub_oscillator
            .render::<false, false>(0.0, f0 * 0.501, 0.5, 1.0, 0.0, &mut sub);

        let cutoff = f0 * semitones((timbre - 0.2) * 120.0);
        // Four-pole to the left of the middle, two-pole to the right, resonant at either end.
        let stage2_gain = (1.0 - (harmonics - 0.4) * 4.0).clamp(0.0, 1.0);
        let resonance = 2.667 * ((harmonics - 0.5).abs() - 0.125).max(0.0);
        let resonance_sqr = resonance * resonance;
        let q = resonance_sqr * resonance_sqr * 48.0;
        let gain = ((harmonics - 0.7) + 0.85).clamp(0.7 - resonance_sqr * 0.3, 1.0);

        if self.fresh {
            self.sub_gain = sub_gain;
            self.cutoff = cutoff;
            self.stage2_gain = stage2_gain;
            self.q = q;
            self.gain = gain;
            self.fresh = false;
        }
        let mut sub_level = Moving::towards(self.sub_gain, sub_gain);
        let mut cutoff_moving = Moving::towards(self.cutoff, cutoff);
        let mut stage2_moving = Moving::towards(self.stage2_gain, stage2_gain);
        let mut q_moving = Moving::towards(self.q, q);
        let mut gain_moving = Moving::towards(self.gain, gain);

        let [first, second] = &mut self.filters;
        for ((out, main), sub) in self.output.iter_mut().zip(main).zip(sub) {
            let cutoff = cutoff_moving.next().min(0.25);
            let q = q_moving.next();
            let stage2_gain = stage2_moving.next();
            first.set_fast(cutoff, 0.5 + q);
            second.set_fast(cutoff, 0.5 + 0.025 * q);

            let gain = gain_moving.next();
            let input = soft_clip((main + sub * sub_level.next()) * gain);
            let lp = soft_clip(first.low_pass(input) * gain);
            *out = lp + stage2_gain * (soft_clip(second.low_pass(lp)) - lp);
        }
        self.sub_gain = sub_gain;
        self.cutoff = cutoff;
        self.stage2_gain = stage2_gain;
        self.q = q;
        self.gain = gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = VirtualAnalogVcf::new();
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
        // Not at the default settings, where `Timbre` at nothing closes the filter.
        for settings in [
            plain(),
            Settings {
                morph: 0.5,
                ..plain()
            },
        ] {
            let decibels = 20.0 * (rms(&run(220.0, settings, 48_000)) / rms(&va)).log10();
            assert!(decibels.abs() < 3.0, "{decibels} dB at {settings:?}");
        }
    }

    #[test]
    fn timbre_opens_the_filter() {
        let dark = run(
            220.0,
            Settings {
                timbre: 0.2,
                ..plain()
            },
            24_000,
        );
        let bright = run(
            220.0,
            Settings {
                timbre: 0.9,
                ..plain()
            },
            24_000,
        );
        assert!(
            roughness(&bright) / rms(&bright).powi(2) > roughness(&dark) / rms(&dark).powi(2) * 4.0,
            "{} {}",
            roughness(&bright),
            roughness(&dark)
        );
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let base = run(220.0, plain(), 4_800);
        for settings in [
            Settings {
                harmonics: 0.0,
                ..plain()
            },
            Settings {
                harmonics: 1.0,
                ..plain()
            },
            Settings {
                timbre: 0.8,
                ..plain()
            },
            Settings {
                morph: 0.5,
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
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
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
        let mut voice = VirtualAnalogVcf::new();
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
