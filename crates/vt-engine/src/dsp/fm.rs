//! Two-operator FM, from Mutable Instruments' Plaits. Ported to Rust from `fm_engine.cc`,
//! `sine_oscillator.h` and `4x_downsampler.h`, with its main output, and a sine worked out rather
//! than read from a table. Its ratios come from `resources/lookup_tables.py`.
//!
//! Copyright 2016, 2021 Emilie Gillet.
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

use std::f32::consts::TAU;

use super::virtual_analog::{BLOCK, Moving, Settings};

const OVERSAMPLING: usize = 4;
#[allow(clippy::excessive_precision)]
const DOWNSAMPLER_FIR: [f32; OVERSAMPLING] =
    [0.024_424_15, 0.092_973_15, 0.167_129_38, 0.215_473_32];
/// How far a modulator's frequency is from its carrier's for each value of `Harm`, in semitones.
/// Each ratio has a flat stretch, so it's easy to land on.
#[allow(clippy::excessive_precision)]
const RATIOS: [f32; 130] = [
    -12.000000, -12.000000, -12.000000, -11.840000, -11.840000, -11.840000, -11.110000, -10.380000,
    -9.650000, -8.920000, -8.190000, -7.460000, -6.730000, -6.000000, -6.000000, -6.000000,
    -5.545512, -5.091023, -4.636535, -4.182046, -4.182046, -4.182046, -3.659291, -3.136535,
    -2.613779, -2.091023, -1.568267, -1.045512, -0.522756, 0.000000, 0.000000, 0.000000, 0.160000,
    0.160000, 0.160000, 0.890000, 1.620000, 2.350000, 3.080000, 3.810000, 4.540000, 5.270000,
    6.000000, 6.000000, 6.000000, 6.454488, 6.908977, 7.363465, 7.817954, 7.817954, 7.817954,
    8.285530, 8.753106, 9.220683, 9.688259, 9.688259, 9.688259, 10.266194, 10.844130, 11.422065,
    12.000000, 12.000000, 12.000000, 12.160000, 12.160000, 12.160000, 12.629775, 13.099550,
    13.569325, 14.039100, 14.039100, 14.039100, 14.907620, 15.776140, 16.644660, 17.513179,
    17.513179, 17.513179, 18.000000, 18.000000, 18.000000, 18.509775, 19.019550, 19.019550,
    19.019550, 19.817954, 19.817954, 19.817954, 20.663864, 21.509775, 21.509775, 21.509775,
    22.132331, 22.754888, 23.377444, 24.000000, 24.000000, 24.000000, 24.509775, 25.019550,
    25.019550, 25.019550, 25.474038, 25.928527, 26.383015, 26.837504, 26.837504, 26.837504,
    27.350320, 27.863137, 27.863137, 27.863137, 28.397353, 28.931569, 29.465784, 30.000000,
    30.000000, 30.000000, 30.750000, 31.500000, 32.250000, 33.000000, 33.750000, 34.500000,
    35.250000, 36.000000, 36.000000, 36.000000, 36.000000, 36.000000,
];
/// How loud it is against the other machines.
const LEVEL: f32 = 0.5;

/// A sine at a phase, moved by a number of turns.
#[inline]
fn sine_pm(phase: u32, modulation: f32) -> f32 {
    (TAU * (phase as f32 / 4_294_967_296.0 + modulation)).sin()
}

/// How far the modulator is from the carrier for a `Harm` on `0..=1`, in semitones.
pub(super) fn ratio(harmonics: f32) -> f32 {
    let at = harmonics.clamp(0.0, 1.0) * 128.0;
    let whole = at as usize;
    RATIOS[whole] + (RATIOS[whole + 1] - RATIOS[whole]) * (at - whole as f32)
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Fm {
    rate: f32,
    carrier_phase: u32,
    modulator_phase: u32,
    carrier_frequency: f32,
    modulator_frequency: f32,
    amount: f32,
    feedback: f32,
    previous_sample: f32,
    fir: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Fm {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate as f32,
            carrier_phase: 0,
            modulator_phase: 0,
            carrier_frequency: 0.0,
            modulator_frequency: 0.0,
            amount: 0.0,
            feedback: 0.0,
            previous_sample: 0.0,
            fir: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency, settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let Settings {
            harmonics,
            timbre,
            morph,
            ..
        } = settings;
        let semitones = ratio(harmonics);
        let carrier = frequency / OVERSAMPLING as f32;
        let modulator = (carrier * 2_f32.powf(semitones / 12.0)).clamp(0.0, 0.5);
        // Less FM for high notes, which would otherwise alias.
        let note = 69.0 + 12.0 * (frequency * self.rate / 440.0).max(1e-3).log2();
        let taming = (1.0 - (note - 24.0 + semitones - 72.0) * 0.025).clamp(0.0, 1.0);
        let amount = 2.0 * timbre * timbre * taming * taming;
        let feedback = 2.0 * morph - 1.0;
        if self.fresh {
            self.carrier_frequency = carrier;
            self.modulator_frequency = modulator;
            self.amount = amount;
            self.feedback = feedback;
            self.fresh = false;
        }
        let mut carrier_fm = Moving::towards(self.carrier_frequency, carrier);
        let mut modulator_fm = Moving::towards(self.modulator_frequency, modulator);
        let mut amount_moving = Moving::towards(self.amount, amount);
        let mut feedback_moving = Moving::towards(self.feedback, feedback);
        let mut head = self.fir;
        for out in &mut self.output {
            let amount = amount_moving.next();
            let feedback = feedback_moving.next();
            let phase_feedback = if feedback < 0.0 {
                0.5 * feedback * feedback
            } else {
                0.0
            };
            let increment = (4_294_967_296.0 * carrier_fm.next()) as u32;
            let modulator_frequency = modulator_fm.next();
            let mut tail = 0.0;
            for j in 0..OVERSAMPLING {
                let step = 4_294_967_296.0
                    * modulator_frequency
                    * (1.0 + self.previous_sample * phase_feedback);
                self.modulator_phase = self.modulator_phase.wrapping_add(step as u32);
                self.carrier_phase = self.carrier_phase.wrapping_add(increment);
                let modulator_fb = if feedback > 0.0 {
                    0.25 * feedback * feedback
                } else {
                    0.0
                };
                let modulator = sine_pm(self.modulator_phase, modulator_fb * self.previous_sample);
                let carrier = sine_pm(self.carrier_phase, amount * modulator);
                self.previous_sample += (carrier - self.previous_sample) * 0.05;
                head += carrier * DOWNSAMPLER_FIR[OVERSAMPLING - 1 - j];
                tail += carrier * DOWNSAMPLER_FIR[j];
            }
            *out = head;
            head = tail;
        }
        self.fir = head;
        self.carrier_frequency = carrier;
        self.modulator_frequency = modulator;
        self.amount = amount;
        self.feedback = feedback;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Fm::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.0,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        total / samples.len() as f32
    }

    #[test]
    fn no_index_is_a_sine_at_the_note() {
        let out = run(110.0, plain(), 48_000);
        let upward = out
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        assert!((upward as i32 - 110).abs() <= 1, "{upward}");
        let peak = out.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!((peak - LEVEL).abs() < 0.05, "{peak}");
    }

    #[test]
    fn the_ratios_run_from_half_to_eight_with_flat_stretches() {
        let as_ratio = |harmonics: f32| 2_f32.powf(ratio(harmonics) / 12.0);
        assert!((as_ratio(0.0) - 0.5).abs() < 0.001);
        assert!((as_ratio(1.0) - 8.0).abs() < 0.001);
        assert!(
            as_ratio(0.5) > 1.9 && as_ratio(0.5) < 2.1,
            "{}",
            as_ratio(0.5)
        );
        assert_eq!(ratio(0.0), ratio(1.0 / 128.0));
    }

    #[test]
    fn more_index_is_brighter_and_ratio_and_feedback_change_it() {
        let dull = roughness(&run(110.0, plain(), 4_800));
        let index = Settings {
            timbre: 0.7,
            ..plain()
        };
        let bright = run(110.0, index, 4_800);
        assert!(roughness(&bright) > dull * 2.0);
        let other_ratio = Settings {
            harmonics: 0.9,
            ..index
        };
        assert_ne!(run(110.0, other_ratio, 4_800), bright);
        for morph in [0.0, 1.0] {
            assert_ne!(run(110.0, Settings { morph, ..index }, 4_800), bright);
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.3, 0.5, 0.8, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                            ..Settings::default()
                        };
                        for sample in run(hertz, settings, 2_000) {
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
}
