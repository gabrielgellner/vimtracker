//! Clocked noise through a resonant filter that runs from low-pass to high-pass, from Mutable
//! Instruments' Plaits. Ported to Rust from `noise_engine.cc` and `noise/clocked_noise.h`, with its
//! main output, limited as Plaits limits it.
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

use super::virtual_analog::{Settings, next_blep, this_blep};
use crate::dsp::blocks::plaits_parts::{Limiter, Svf, random, tan_accurate};
use crate::rng::Pcg32;

/// How many frames are worked out at once, as in Plaits.
const BLOCK: usize = 24;
/// How loud it is against the other machines.
const LEVEL: f32 = 0.83;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// A MIDI note's frequency as a fraction of Plaits' 48 kHz, for settings it measures in notes.
#[inline]
fn note_frequency(note: f32) -> f32 {
    440.0 / 48_000.0 * semitones(note - 69.0)
}

/// Noise held for a clock's cycle, then jumping to the next value, with the jumps smoothed so
/// they don't alias. Above a quarter of the rate, it fades to plain noise.
#[derive(Debug, Clone, Copy)]
struct ClockedNoise {
    phase: f32,
    sample: f32,
    next_sample: f32,
    frequency: f32,
}

impl ClockedNoise {
    fn new() -> Self {
        Self {
            phase: 0.0,
            sample: 0.0,
            next_sample: 0.0,
            frequency: 0.001,
        }
    }

    fn render(&mut self, rng: &mut Pcg32, sync: bool, frequency: f32, out: &mut [f32; BLOCK]) {
        let frequency = frequency.clamp(0.0, 1.0);
        let step = (frequency - self.frequency) / BLOCK as f32;
        let mut next_sample = self.next_sample;
        let mut sample = self.sample;
        if sync {
            self.phase = 1.0;
        }
        for out in out.iter_mut() {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            self.frequency += step;
            let frequency = self.frequency;
            let raw_sample = random(rng) * 2.0 - 1.0;
            let raw_amount = (4.0 * (frequency - 0.25)).clamp(0.0, 1.0);
            self.phase += frequency;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = self.phase / frequency;
                let discontinuity = raw_sample - sample;
                this_sample += discontinuity * this_blep(t);
                next_sample += discontinuity * next_blep(t);
                sample = raw_sample;
            }
            next_sample += sample;
            *out = this_sample + raw_amount * (raw_sample - this_sample);
        }
        self.next_sample = next_sample;
        self.sample = sample;
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Noise {
    rng: Pcg32,
    clocked: ClockedNoise,
    filter: Svf,
    previous_f0: f32,
    previous_q: f32,
    previous_mode: f32,
    limiter: Limiter,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

impl Noise {
    pub fn new() -> Self {
        Self {
            rng: Pcg32::new(0, 0),
            clocked: ClockedNoise::new(),
            filter: Svf::default(),
            previous_f0: 0.0,
            previous_q: 0.0,
            previous_mode: 0.0,
            limiter: Limiter::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Choose its noise, so each note differs but a render repeats.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 4);
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
        let mode = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let morph = settings.morph.clamp(0.0, 1.0);
        // The clock runs from two octaves below the lowest note to the top of the range.
        let clock_f = note_frequency(timbre * 152.0 - 24.0);
        let q = 0.5 * semitones(morph * 120.0);
        let sync = self.fresh;
        if self.fresh {
            (self.previous_f0, self.previous_q, self.previous_mode) = (f0, q, mode);
            self.fresh = false;
        }
        let mut noise = [0.0; BLOCK];
        self.clocked
            .render(&mut self.rng, sync, clock_f, &mut noise);

        let f0_step = (f0 - self.previous_f0) / BLOCK as f32;
        let q_step = (q - self.previous_q) / BLOCK as f32;
        let mode_step = (mode - self.previous_mode) / BLOCK as f32;
        let mut block = [0.0; BLOCK];
        for (out, sample) in block.iter_mut().zip(noise) {
            self.previous_f0 += f0_step;
            self.previous_q += q_step;
            self.previous_mode += mode_step;
            let (f0, q) = (self.previous_f0, self.previous_q);
            let gain = 1.0 / ((0.5 + q) * 40.0 * f0).sqrt();
            self.filter.set_g_q(tan_accurate(f0), q);
            *out = self.filter.low_to_high(sample * gain, self.previous_mode);
        }
        for (out, sample) in self.output.iter_mut().zip(block) {
            *out = self.limiter.process(1.0, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Noise::new();
        voice.seed(7);
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.0,
            timbre: 0.9,
            morph: 0.2,
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

    #[test]
    fn it_keeps_sounding() {
        let out = run(880.0, plain(), 48_000);
        assert!(energy(&out[43_200..]) > 1e-4, "{}", energy(&out[43_200..]));
    }

    #[test]
    fn the_filter_runs_from_low_to_high_pass() {
        let low = run(880.0, plain(), 24_000);
        let high = run(
            880.0,
            Settings {
                harmonics: 1.0,
                ..plain()
            },
            24_000,
        );
        assert!(
            roughness(&high) / energy(&high) > roughness(&low) / energy(&low) * 4.0,
            "{} {}",
            roughness(&high) / energy(&high),
            roughness(&low) / energy(&low)
        );
    }

    #[test]
    fn a_slow_clock_steps() {
        let out = run(
            880.0,
            Settings {
                timbre: 0.3,
                ..plain()
            },
            4_800,
        );
        assert_ne!(out, run(880.0, plain(), 4_800));
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
}
