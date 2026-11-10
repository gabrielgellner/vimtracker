//! Formants from segments of sine waves, from the granular formant model of Mutable Instruments'
//! Plaits: two grainlets an interval apart, its main output. Ported to Rust from `grain_engine.cc`
//! and `oscillator/grainlet_oscillator.h`. Its other output, the Z oscillator, isn't ported.
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

use super::virtual_analog::{BLOCK, Moving, Settings, next_blep, this_blep};
use crate::dsp::blocks::plaits_parts::OnePole;
use crate::dsp::blocks::sine_table::SINE;

/// The highest frequency an oscillator runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;
/// How loud it is against the other machines. Plaits plays this model at 0.7 and the virtual
/// analog one at 0.8, but its two grainlets add up louder than that saw and square: at 220 Hz,
/// with `Harm` 0.6 and `Timbre` and `Morph` in the middle, it measures 0.39 RMS to the virtual
/// analog voice's 0.24 at its defaults, so this brings the two level. Low `Timbre` puts the
/// formant below the note and is much quieter, as in Plaits.
const LEVEL: f32 = 0.62;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// A MIDI note's frequency as a fraction of Plaits' 48 kHz, for settings it measures in notes.
#[inline]
fn note_frequency(note: f32) -> f32 {
    440.0 / 48_000.0 * semitones(note - 69.0)
}

/// A sine of a phase in turns, read from Plaits' table, wrapping.
#[inline]
fn sine(phase: f32) -> f32 {
    let index = (phase - phase.floor()) * 512.0;
    let whole = (index as usize).min(511);
    let fraction = index - whole as f32;
    SINE[whole] + (SINE[whole + 1] - SINE[whole]) * fraction
}

/// A window shaped by `shape`, on `0..=0.5`, at a phase of the carrier's cycle.
#[inline]
fn carrier(phase: f32, shape: f32) -> f32 {
    let shape = shape * 3.0;
    let whole = shape as usize;
    let mut t = 1.0 - (shape - whole as f32);
    let phase = match whole {
        // Squeezed into the start of the cycle and silent after.
        0 => (phase * (1.0 + t * t * t * 15.0)).min(1.0) + 0.75,
        // A rise and fall with its peak moving.
        1 => {
            let breakpoint = 0.001 + 0.499 * t * t * t;
            let phase = if phase < breakpoint {
                phase * (0.5 / breakpoint)
            } else {
                0.5 + (phase - breakpoint) * 0.5 / (1.0 - breakpoint)
            };
            phase + 0.75
        }
        // Starting at its peak and falling.
        _ => {
            t = 1.0 - t;
            (0.25 + phase * (0.5 + t * t * t * 14.5)).min(0.75)
        }
    };
    (sine(phase) + 1.0) * 0.25
}

/// The formant's sine through the carrier's window, with some of the window bled in.
#[inline]
fn grainlet(carrier_phase: f32, formant_phase: f32, shape: f32, bleed: f32) -> f32 {
    carrier(carrier_phase, shape) * (sine(formant_phase) + bleed) / (1.0 + bleed)
}

/// A sine at the formant's frequency, restarted on each cycle of the carrier and windowed by it,
/// with the restart's jump smoothed.
#[derive(Debug, Clone, Copy)]
struct Grainlet {
    carrier_phase: f32,
    formant_phase: f32,
    next_sample: f32,
    carrier_frequency: f32,
    formant_frequency: f32,
    shape: f32,
    bleed: f32,
}

impl Grainlet {
    fn new() -> Self {
        Self {
            carrier_phase: 0.0,
            formant_phase: 0.0,
            next_sample: 0.0,
            carrier_frequency: 0.0,
            formant_frequency: 0.0,
            shape: 0.0,
            bleed: 0.0,
        }
    }

    /// Start its settings where they're asked for, rather than moving there from nothing.
    fn settle(&mut self, carrier: f32, formant: f32, shape: f32, bleed: f32) {
        self.carrier_frequency = carrier.min(MAX_FREQUENCY * 0.5);
        self.formant_frequency = formant.min(MAX_FREQUENCY);
        (self.shape, self.bleed) = (shape, bleed);
    }

    fn render(
        &mut self,
        carrier: f32,
        formant: f32,
        shape: f32,
        bleed: f32,
        out: &mut [f32; BLOCK],
    ) {
        let carrier = carrier.min(MAX_FREQUENCY * 0.5);
        let formant = formant.min(MAX_FREQUENCY);
        let mut f0s = Moving::towards(self.carrier_frequency, carrier);
        let mut f1s = Moving::towards(self.formant_frequency, formant);
        let mut shapes = Moving::towards(self.shape, shape);
        let mut bleeds = Moving::towards(self.bleed, bleed);
        let mut next_sample = self.next_sample;
        for out in out.iter_mut() {
            let this_sample = next_sample;
            next_sample = 0.0;
            let f0 = f0s.next();
            let f1 = f1s.next();
            self.carrier_phase += f0;
            if self.carrier_phase >= 1.0 {
                self.carrier_phase -= 1.0;
                let reset_time = self.carrier_phase / f0;
                let before = grainlet(
                    1.0,
                    self.formant_phase + (1.0 - reset_time) * f1,
                    shapes.subsample(1.0 - reset_time),
                    bleeds.subsample(1.0 - reset_time),
                );
                let after = grainlet(0.0, 0.0, shapes.subsample(1.0), bleeds.subsample(1.0));
                let discontinuity = after - before;
                *out = this_sample + discontinuity * this_blep(reset_time);
                next_sample += discontinuity * next_blep(reset_time);
                self.formant_phase = reset_time * f1;
            } else {
                *out = this_sample;
                self.formant_phase += f1;
                if self.formant_phase >= 1.0 {
                    self.formant_phase -= 1.0;
                }
            }
            next_sample += grainlet(
                self.carrier_phase,
                self.formant_phase,
                shapes.next(),
                bleeds.next(),
            );
        }
        self.next_sample = next_sample;
        (self.carrier_frequency, self.formant_frequency) = (carrier, formant);
        (self.shape, self.bleed) = (shape, bleed);
    }
}

/// One voice of the model. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Grain {
    grainlets: [Grainlet; 2],
    dc_blocker: OnePole,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Grain {
    fn default() -> Self {
        Self::new()
    }
}

impl Grain {
    pub fn new() -> Self {
        Self {
            grainlets: [Grainlet::new(); 2],
            dc_blocker: OnePole::default(),
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
        // Plaits clips what's left as it turns it into 16-bit samples.
        (sample * LEVEL).clamp(-1.0, 1.0)
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let harmonics = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let morph = settings.morph.clamp(0.0, 1.0);
        let f1 = note_frequency(24.0 + 84.0 * timbre);
        let ratio = semitones(-24.0 + 48.0 * harmonics);
        // Below the middle of `Harm`, the bare window bleeds in alongside the formants.
        let bleed = if harmonics < 0.5 {
            1.0 - 2.0 * harmonics
        } else {
            0.0
        };
        let bleed = bleed * (2.0 - bleed);
        // High notes leave too little of a cycle to shape, so the shape settles in the middle.
        let shape = 0.33 + (morph - 0.33) * (1.0 - f0 * 24.0).max(0.0);
        if self.fresh {
            self.grainlets[0].settle(f0, f1, shape, bleed);
            self.grainlets[1].settle(f0, f1 * ratio, shape, bleed);
            self.fresh = false;
        }

        let mut second = [0.0; BLOCK];
        self.grainlets[0].render(f0, f1, shape, bleed, &mut self.output);
        self.grainlets[1].render(f0, f1 * ratio, shape, bleed, &mut second);
        self.dc_blocker.set_dirty(0.3 * f0);
        for (out, second) in self.output.iter_mut().zip(second) {
            *out = self.dc_blocker.high_pass(*out + second);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Grain::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.6,
            timbre: 0.5,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// How far apart two sounds are, against how loud they are.
    fn difference(a: &[f32], b: &[f32]) -> f32 {
        let apart: Vec<f32> = a.iter().zip(b).map(|(a, b)| a - b).collect();
        energy(&apart) / energy(a).max(energy(b))
    }

    #[test]
    fn it_keeps_sounding() {
        let out = run(220.0, plain(), 48_000);
        assert!(energy(&out[43_200..]) > 1e-3, "{}", energy(&out[43_200..]));
    }

    #[test]
    fn harmonics_timbre_and_morph_each_change_the_sound() {
        let base = run(220.0, plain(), 9_600);
        for settings in [
            Settings {
                harmonics: 0.9,
                ..plain()
            },
            Settings {
                timbre: 0.8,
                ..plain()
            },
            Settings {
                morph: 0.1,
                ..plain()
            },
        ] {
            let other = run(220.0, settings, 9_600);
            assert!(
                difference(&base, &other) > 0.1,
                "{} at {settings:?}",
                difference(&base, &other)
            );
        }
    }

    #[test]
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        use crate::dsp::voices::virtual_analog::VirtualAnalog;
        let grain = run(220.0, plain(), 48_000);
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE, Settings::default()))
            .collect();
        let decibels = 10.0 * (energy(&grain) / energy(&va)).log10();
        assert!(decibels.abs() < 3.0, "{decibels} dB");
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.33, 0.5, 0.67, 1.0] {
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
}
