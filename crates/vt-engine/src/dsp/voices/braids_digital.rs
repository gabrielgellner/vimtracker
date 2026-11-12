//! Digital models from Mutable Instruments' Braids: three sines ring modulating each other, a
//! saw through a tuned comb filter, and a low-resolution, circuit-bent tone. Ported to Rust from
//! `digital_oscillator.cc` and `macro_oscillator.cc`, at twice the program's rate as Braids runs
//! at 96 kHz, through [`super::braids_parts`].
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

use super::braids_parts::{
    AnalogOscillator, INNER, MODERATE_OVERDRIVE, Macro, Model, NO_SYNC, Pitch, Shape, sine,
    waveshape,
};
use super::virtual_analog::Settings;

/// A sine ring modulated by two more, each up to 32 semitones either way of the note.
pub type Ring = Macro<RingModel>;
/// A saw through a comb filter tuned up to five octaves either way of the note.
pub type SawComb = Macro<SawCombModel>;
/// A tone worked out at a low rate in few bits, with its bits bent.
pub type Toy = Macro<ToyModel>;

#[derive(Debug, Clone, Copy)]
pub struct RingModel {
    phases: [f32; 3],
}

impl Model for RingModel {
    const LEVEL: f32 = 1.0;

    fn new() -> Self {
        Self { phases: [0.0; 3] }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let increments = [
            pitch.increment(0.0),
            pitch.increment((settings.at(0) - 0.5) * 64.0),
            pitch.increment((settings.at(1) - 0.5) * 64.0),
        ];
        for out in out.iter_mut() {
            for (phase, increment) in self.phases.iter_mut().zip(increments) {
                *phase = (*phase + increment).fract();
            }
            let [carrier, first, second] = self.phases;
            // The carrier a quarter of a turn on, as Braids reads it.
            let ring = sine(carrier + 0.25) * sine(first) * sine(second);
            // Braids halves each product to keep it in its 16 bits.
            *out = waveshape(&MODERATE_OVERDRIVE, 0.25 * ring);
        }
    }
}

/// How long the comb's delay can be, in frames at twice the rate: down to about 12 Hz at 48 kHz.
const COMB_LENGTH: usize = 8192;

#[derive(Debug, Clone, Copy)]
pub struct SawCombModel {
    saw: AnalogOscillator,
    /// The comb's line, in Braids' 16 bits to keep the voice small.
    line: [i16; COMB_LENGTH],
    at: usize,
    /// The comb's pitch, gliding to where it's set so its delay doesn't jump.
    pitch: Option<f32>,
}

impl Model for SawCombModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self {
            saw: AnalogOscillator::new(),
            line: [0; COMB_LENGTH],
            at: 0,
            pitch: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (tuning, feedback) = (settings.at(0), settings.at(1));
        let increment = pitch.increment(0.0);
        self.saw
            .render(Shape::Saw, increment, 0.0, 0.0, &NO_SYNC, out, None);

        let target = (tuning - 0.5) * 128.0;
        let glided = self
            .pitch
            .map_or(target, |was| (15.0 * was + target) / 16.0);
        self.pitch = Some(glided);
        let delay = (1.0 / pitch.increment(glided)).min((COMB_LENGTH - 2) as f32);
        let (whole, fraction) = (delay as usize, delay.fract());
        // More exact near either end, where it rings longest.
        let resonance = waveshape(&MODERATE_OVERDRIVE, feedback * 2.0 - 1.0);
        for sample in out.iter_mut() {
            let input = *sample;
            let read = |back: usize| {
                f32::from(self.line[(self.at + COMB_LENGTH - back) % COMB_LENGTH]) / 32_768.0
            };
            let (a, b) = (read(whole), read(whole + 1));
            let delayed = a + (b - a) * fraction;
            let back = (delayed * resonance + 0.5 * input).clamp(-1.0, 1.0);
            self.line[self.at] = (back * 32_767.0) as i16;
            *sample = (0.5 * input + delayed).clamp(-1.0, 1.0);
            self.at = (self.at + 1) % COMB_LENGTH;
        }
    }
}

/// The filter Braids runs over each of `Toy`'s frames, worked out four times over, and the offset
/// its sum is moved down by.
const TOY_FIR: [u32; 4] = [10_530, 14_751, 16_384, 14_751];
const TOY_OFFSET: i32 = 28_208;

#[derive(Debug, Clone, Copy)]
pub struct ToyModel {
    phase: u32,
    held: u8,
    counter: u16,
}

impl Model for ToyModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self {
            phase: 0,
            held: 0,
            counter: 0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        // In Braids' own whole numbers, as it's made of their bits.
        let increment = (pitch.increment(0.0) * 4_294_967_296.0 / 4.0) as u32;
        let rate = (settings.at(0) * 32_767.0) as u16;
        let bend = ((settings.at(1) * 32_767.0) as u16 >> 8) as u8;
        // How many of its four steps a frame each held value lasts.
        let count = 512 - (rate >> 6);
        for out in out.iter_mut() {
            let mut filtered = 0;
            for tap in TOY_FIR {
                self.phase = self.phase.wrapping_add(increment);
                if self.counter >= count {
                    let top = (self.phase >> 24) as u8;
                    self.held = ((top ^ (bend << 1)) & !bend).wrapping_add(bend >> 1);
                    self.counter = 0;
                }
                filtered += tap * u32::from(self.held);
                self.counter += 1;
            }
            *out = ((filtered >> 8) as i32 - TOY_OFFSET) as f32 / 32_768.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: u32 = 48_000;

    fn settings(values: [f32; 2]) -> Settings {
        let mut places = [0.0; vt_model::settings::MAX_MACHINE_SETTINGS];
        places[..2].copy_from_slice(&values);
        Settings::from_places(places)
    }

    fn run<M: Model>(hertz: f32, values: [f32; 2], frames: usize) -> Vec<f32> {
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

    fn each(mut check: impl FnMut(&str, &dyn Fn(f32, [f32; 2], usize) -> Vec<f32>)) {
        check("Ring", &|hertz, values, frames| {
            run::<RingModel>(hertz, values, frames)
        });
        check("SawComb", &|hertz, values, frames| {
            run::<SawCombModel>(hertz, values, frames)
        });
        check("Toy", &|hertz, values, frames| {
            run::<ToyModel>(hertz, values, frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.6, 0.6], [0.75, 0.75]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                assert!(decibels.abs() < 3.0, "{name}: {decibels} dB at {values:?}");
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
}
