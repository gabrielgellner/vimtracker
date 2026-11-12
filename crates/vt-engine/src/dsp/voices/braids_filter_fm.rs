//! Models from Mutable Instruments' Braids that fake a resonant filter and bend sines by their own
//! feedback: its digital filters, the CZ-style resonant waves made by windowing a sine synced to
//! the note, and its feedback and chaotic FM. Ported to Rust from `digital_oscillator.cc`, at
//! twice the program's rate as Braids runs at 96 kHz, through [`super::braids_parts`].
//!
//! Copyright 2012 Emilie Gillet.
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

use super::braids_parts::{INNER, Macro, Model, Pitch, mix, sine};
use super::virtual_analog::{Moving, Settings};

/// A resonant filter's sound, faked as Casio's CZ did: a sine at the cutoff, restarted each cycle
/// of the note and windowed by a saw or a triangle, blended with a square made the same way.
pub type DigitalFilter = Macro<DigitalFilterModel>;
/// A sine bent by a second, which its own last output bends in turn.
pub type FeedbackFm = Macro<FeedbackFmModel>;
/// A sine bent by a second, whose speed its own output moves, for unstable, chaotic tones.
pub type ChaoticFm = Macro<ChaoticFmModel>;

/// Where each filter's sine restarts each cycle, and where its square's restarts each half, in
/// turns: low-pass, peak, band-pass and high-pass.
const FILTER_RESETS: [f32; 4] = [0.0, 0.5, 0.25, 0.5];
const SQUARE_RESETS: [f32; 2] = [0.25, 0.5];

#[derive(Debug, Clone, Copy)]
pub struct DigitalFilterModel {
    phase: f32,
    modulator: f32,
    square_modulator: f32,
    integrator: f32,
    polarity: bool,
    increment: Option<f32>,
}

impl Model for DigitalFilterModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self {
            phase: 0.0,
            modulator: 0.0,
            square_modulator: 0.0,
            integrator: 0.0,
            polarity: false,
            increment: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let kind = ((settings.at(0) * 255.0).round() as usize).min(FILTER_RESETS.len() - 1);
        let (cutoff, shape) = (settings.at(1), settings.at(2));
        let increment = pitch.increment(0.0);
        // The cutoff, from a little under the note to ten octaves over it.
        let shifted = (cutoff * 128.0 - 8.0).min(128.0 - pitch.note);
        let target = pitch.increment(shifted);
        let mut modulator_increments =
            Moving::across(self.increment.unwrap_or(target), target, INNER);
        self.increment = Some(target);
        // A saw's window below the middle and a triangle's above, with the square between.
        let balance = if shape < 0.5 {
            shape * 2.0
        } else {
            2.0 - shape * 2.0
        };
        let low_or_peak = kind & 2 == 0;
        for out in out.iter_mut() {
            let modulator_increment = modulator_increments.next();
            self.phase += increment;
            self.modulator = (self.modulator + modulator_increment).fract();
            self.square_modulator = (self.square_modulator + modulator_increment).fract();
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.modulator = FILTER_RESETS[kind];
            }
            if (2.0 * self.phase).fract() < 2.0 * increment {
                self.polarity = !self.polarity;
                self.square_modulator = SQUARE_RESETS[kind & 1];
            }
            let carrier = sine(self.modulator);
            let square_carrier = sine(self.square_modulator);
            let saw = 1.0 - self.phase;
            let double_saw = 1.0 - (2.0 * self.phase).fract();
            let triangle = if self.phase < 0.5 {
                2.0 * self.phase
            } else {
                2.0 - 2.0 * self.phase
            };
            let window = if shape < 0.5 { saw } else { triangle };
            let mut pulse = square_carrier * double_saw;
            if self.polarity {
                pulse = -pulse;
            }
            // The square's integrator gains with the cutoff, as an integrator does.
            self.integrator =
                (self.integrator + pulse * modulator_increment * 4.0).clamp(-1.0, 1.0);
            let (windowed, square) = if low_or_peak {
                let square = if kind == 1 {
                    0.5 * (pulse + self.integrator)
                } else {
                    self.integrator
                };
                (window * (carrier + 1.0) - 1.0, square)
            } else {
                (carrier * window, pulse)
            };
            *out = mix(windowed, square, balance);
        }
    }
}

/// Braids' FM ratios, which `Ratio` catches on: from an eighth to eight, with the simple ones
/// held longest, in 256ths of a semitone above 16 384 for 1:1.
const FM_QUANTIZER: [u16; 129] = [
    7168, 7168, 7168, 7360, 7552, 7744, 7936, 8128, 8320, 8512, 8704, 8896, 9088, 9280, 9472, 9664,
    9856, 10048, 10240, 10240, 10240, 10432, 10624, 10816, 11008, 11200, 11392, 11584, 11776,
    11968, 12160, 12352, 12544, 12736, 12928, 13312, 13312, 13312, 13352, 13352, 13352, 13726,
    14100, 14474, 14848, 14848, 14848, 15080, 15313, 15313, 15313, 15581, 15848, 16116, 16384,
    16384, 16384, 16424, 16424, 16424, 16798, 17172, 17546, 17920, 17920, 17920, 18152, 18385,
    18385, 18385, 18624, 18864, 18864, 18864, 19160, 19456, 19456, 19456, 19496, 19496, 19496,
    19737, 19978, 19978, 19978, 20200, 20422, 20645, 20867, 20867, 20867, 20992, 20992, 20992,
    21253, 21253, 21253, 21457, 21457, 21457, 21673, 21890, 21890, 21890, 22209, 22528, 22528,
    22528, 22789, 22789, 22789, 23021, 23254, 23254, 23254, 23516, 23516, 23516, 23790, 24064,
    24064, 24064, 24448, 24832, 25216, 25600, 25600, 25600, 25600,
];

/// How many semitones `Ratio` puts the modulator from the note.
fn ratio_semitones(ratio: f32) -> f32 {
    let index = ratio.clamp(0.0, 1.0) * 127.996;
    let whole = index as usize;
    let fraction = index - whole as f32;
    let (a, b) = (
        f32::from(FM_QUANTIZER[whole]),
        f32::from(FM_QUANTIZER[whole + 1]),
    );
    (a + (b - a) * fraction - 16_384.0) / 256.0
}

#[derive(Debug, Clone, Copy)]
pub struct FeedbackFmModel {
    phase: f32,
    modulator: f32,
    previous: f32,
    amount: Option<f32>,
}

impl Model for FeedbackFmModel {
    const LEVEL: f32 = 0.35;

    fn new() -> Self {
        Self {
            phase: 0.0,
            modulator: 0.0,
            previous: 0.0,
            amount: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (amount, ratio) = (settings.at(0), ratio_semitones(settings.at(1)));
        // Less feedback on high notes and high modulators, where it would turn to noise.
        let attenuation = (1.0 - (pitch.note - 72.0 + ratio) / 64.0).clamp(0.0, 1.0);
        let increment = pitch.increment(0.0);
        let modulator_increment = pitch.increment(12.0 + ratio) * 0.5;
        let mut amounts = Moving::across(self.amount.unwrap_or(amount), amount, INNER);
        self.amount = Some(amount);
        for out in out.iter_mut() {
            self.phase = (self.phase + increment).fract();
            self.modulator = (self.modulator + modulator_increment).fract();
            let index = amounts.next() * attenuation;
            let bent = sine(self.modulator + self.previous * 0.125) * index * 0.5;
            self.previous = sine(self.phase + bent);
            *out = self.previous;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChaoticFmModel {
    phase: f32,
    modulator: f32,
    amount: Option<f32>,
}

impl Model for ChaoticFmModel {
    const LEVEL: f32 = 0.35;

    fn new() -> Self {
        Self {
            phase: 0.0,
            modulator: 0.0,
            amount: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (amount, ratio) = (settings.at(0), ratio_semitones(settings.at(1)));
        let increment = pitch.increment(0.0);
        let modulator_increment = pitch.increment(12.0 + ratio) * 0.5;
        let mut amounts = Moving::across(self.amount.unwrap_or(amount), amount, INNER);
        self.amount = Some(amount);
        for out in out.iter_mut() {
            self.phase = (self.phase + increment).fract();
            let bent = sine(self.modulator) * amounts.next() * 0.5;
            let sample = sine(self.phase + bent);
            // The output speeds the modulator up or slows it down, by up to a half.
            let speed = (129.0 + sample * 64.0) / 256.0;
            self.modulator = (self.modulator + modulator_increment * speed).fract();
            *out = sample;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: u32 = 48_000;

    fn settings(values: [f32; 3]) -> Settings {
        let mut places = [0.0; vt_model::settings::MAX_MACHINE_SETTINGS];
        places[..3].copy_from_slice(&values);
        Settings::from_places(places)
    }

    fn run<M: Model>(hertz: f32, values: [f32; 3], frames: usize) -> Vec<f32> {
        let mut voice = Macro::<M>::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings(values)))
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn differs(a: &[f32], b: &[f32]) -> f32 {
        let total: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
        (total / a.len() as f32).sqrt()
    }

    /// Each model, given the two settings Braids has, and the filter in each of its types.
    fn each(mut check: impl FnMut(&str, &dyn Fn(f32, [f32; 2], usize) -> Vec<f32>)) {
        for kind in 0..4 {
            let name = ["LP", "Peak", "BP", "HP"][kind];
            check(name, &|hertz, [first, second], frames| {
                run::<DigitalFilterModel>(hertz, [kind as f32 / 255.0, first, second], frames)
            });
        }
        check("FB FM", &|hertz, [first, second], frames| {
            run::<FeedbackFmModel>(hertz, [first, second, 0.0], frames)
        });
        check("Chaos", &|hertz, [first, second], frames| {
            run::<ChaoticFmModel>(hertz, [first, second, 0.0], frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.5, 0.25], [0.75, 0.6]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                // The filters are quieter as their cutoff rises, as a resonance thins a sound.
                assert!(decibels.abs() < 5.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            let base = run(220.0, [0.3, 0.3], 4_800);
            for values in [[0.9, 0.3], [0.3, 0.9]] {
                let other = run(220.0, values, 4_800);
                assert!(differs(&base, &other) > 0.02, "{name} {values:?}");
            }
        });
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        each(|name, run| {
            for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
                for first in [0.0, 0.5, 1.0] {
                    for second in [0.0, 0.5, 1.0] {
                        for sample in run(hertz, [first, second], 2_400) {
                            assert!(
                                sample.is_finite() && sample.abs() <= 1.0,
                                "{name}: {sample} at {hertz} Hz {first} {second}"
                            );
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn the_ratio_catches_on_simple_ratios() {
        // The ends are an eighth and eight times, and 1:1 holds for a stretch below the middle.
        assert!((ratio_semitones(0.0) + 36.0).abs() < 0.01);
        assert!((ratio_semitones(1.0) - 36.0).abs() < 0.01);
        for at in [0.425, 0.43, 0.435] {
            assert_eq!(ratio_semitones(at), 0.0, "{at}");
        }
    }
}
