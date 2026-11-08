//! A swarm of eight sawtooths, each under its own grain envelope, from the swarm model of Mutable
//! Instruments' Plaits, with its main output, limited as Plaits limits it. Ported to Rust from
//! `swarm_engine.h` and `swarm_engine.cc`.
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

use super::plaits_parts::{Limiter, interpolate, random};
use super::sine_table::SINE;
use super::virtual_analog::{BLOCK, Moving, Settings, next_blep, this_blep};
use crate::rng::Pcg32;

const VOICES: usize = 8;
/// The highest frequency a saw runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;
/// Plaits runs this model into its limiter with a pre-gain of 3.
const PRE_GAIN: f32 = 3.0;
/// How loud it is against the other machines. Out of the limiter it already sits within about a
/// decibel of the virtual analog voice at 220 Hz, measured over a second: 1 dB quieter with the
/// default settings, level with timbre at the middle, up to 2 dB louder with harmonics at none,
/// when the saws line up. So it's left as it comes.
const LEVEL: f32 = 1.0;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// A MIDI note's frequency as a fraction of Plaits' 48 kHz, for settings it measures in notes.
#[inline]
fn note_frequency(note: f32) -> f32 {
    440.0 / 48_000.0 * semitones(note - 69.0)
}

/// Plaits' table sine of a phase in turns, wrapping.
#[inline]
fn sine(phase: f32) -> f32 {
    interpolate(&SINE, phase.rem_euclid(1.0), 512.0)
}

/// A grain's shape and the pitch it glides across. Short grains fade in and out; once they
/// overlap enough, it's a held tone gliding from one random pitch to the next.
#[derive(Debug, Clone, Copy)]
struct GrainEnvelope {
    from: f32,
    interval: f32,
    phase: f32,
    fm: f32,
    amplitude: f32,
    previous_size_ratio: f32,
    filter_coefficient: f32,
}

impl GrainEnvelope {
    fn new() -> Self {
        Self {
            from: 0.0,
            interval: 1.0,
            phase: 1.0,
            fm: 0.0,
            amplitude: 0.5,
            previous_size_ratio: 0.0,
            filter_coefficient: 0.0,
        }
    }

    /// Move on by a block. A burst starts every grain at once and fast, then each one slows.
    fn step(&mut self, rng: &mut Pcg32, rate: f32, burst_mode: bool, start_burst: bool) {
        let mut randomize = false;
        if start_burst {
            self.phase = 0.5;
            self.fm = 16.0;
            randomize = true;
        } else {
            self.phase += rate * self.fm;
            if self.phase >= 1.0 {
                self.phase -= self.phase.floor();
                randomize = true;
            }
        }
        if randomize {
            self.from += self.interval;
            self.interval = random(rng) - self.from;
            // A random length for the next grain.
            if burst_mode {
                self.fm *= 0.8 + 0.2 * random(rng);
            } else {
                self.fm = 0.5 + 1.5 * random(rng);
            }
        }
    }

    /// Where the pitch is. Two overlapping grains are heard as one tone gliding between them.
    #[inline]
    fn frequency(&self, size_ratio: f32) -> f32 {
        if size_ratio < 1.0 {
            2.0 * (self.from + self.interval * self.phase) - 1.0
        } else {
            self.from
        }
    }

    fn amplitude(&mut self, size_ratio: f32) -> f32 {
        let mut target = 1.0;
        if size_ratio >= 1.0 {
            let phase = ((self.phase - 0.5) * size_ratio).clamp(-1.0, 1.0);
            target = 0.5 * (sine(0.5 * phase + 1.25) + 1.0);
        }
        // Crossing between grains and a held tone smooths the level for a while.
        if (size_ratio >= 1.0) != (self.previous_size_ratio >= 1.0) {
            self.filter_coefficient = 0.5;
        }
        self.filter_coefficient *= 0.95;
        self.previous_size_ratio = size_ratio;
        self.amplitude += (0.5 - self.filter_coefficient) * (target - self.amplitude);
        self.amplitude
    }
}

/// A plain saw with smoothed edges, added into a block at a moving level.
#[derive(Debug, Clone, Copy)]
struct AdditiveSaw {
    phase: f32,
    next_sample: f32,
    frequency: f32,
    gain: f32,
}

impl AdditiveSaw {
    fn new() -> Self {
        Self {
            phase: 0.0,
            next_sample: 0.0,
            frequency: 0.01,
            gain: 0.0,
        }
    }

    fn render(&mut self, frequency: f32, level: f32, out: &mut [f32; BLOCK]) {
        let frequency = frequency.min(MAX_FREQUENCY);
        let mut fm = Moving::towards(self.frequency, frequency);
        let mut gain = Moving::towards(self.gain, level);
        let mut next_sample = self.next_sample;
        for out in out.iter_mut() {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let frequency = fm.next();
            self.phase += frequency;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = self.phase / frequency;
                this_sample -= this_blep(t);
                next_sample -= next_blep(t);
            }
            next_sample += self.phase;
            *out += (2.0 * this_sample - 1.0) * gain.next();
        }
        self.next_sample = next_sample;
        (self.frequency, self.gain) = (frequency, level);
    }
}

/// One of the swarm, at its own place in the spread.
#[derive(Debug, Clone, Copy)]
struct SwarmVoice {
    rank: f32,
    envelope: GrainEnvelope,
    saw: AdditiveSaw,
}

impl SwarmVoice {
    fn new(rank: f32) -> Self {
        Self {
            rank,
            envelope: GrainEnvelope::new(),
            saw: AdditiveSaw::new(),
        }
    }
}

/// One voice of the model. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Swarm {
    rng: Pcg32,
    voices: [SwarmVoice; VOICES],
    limiter: Limiter,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Swarm {
    fn default() -> Self {
        Self::new()
    }
}

impl Swarm {
    pub fn new() -> Self {
        // Plaits works out the middle in whole numbers, so the ranks run from -1 to 4/3.
        let middle = ((VOICES - 1) / 2) as f32;
        let voices = std::array::from_fn(|i| SwarmVoice::new((i as f32 - middle) / middle));
        Self {
            rng: Pcg32::new(0, 0),
            voices,
            limiter: Limiter::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Choose its grains, so each note differs but a render repeats.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 5);
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
        let density = note_frequency(timbre * 120.0) * 0.025 * BLOCK as f32;
        let spread = harmonics * harmonics * harmonics;
        let mut size_ratio = 0.25 * semitones((1.0 - morph) * 84.0);
        // Each note is a trigger, so it bursts as Plaits does with its trigger patched.
        let start_burst = self.fresh;
        self.fresh = false;

        let mut block = [0.0; BLOCK];
        for voice in &mut self.voices {
            voice
                .envelope
                .step(&mut self.rng, density, true, start_burst);
            let amplitude = voice.envelope.amplitude(size_ratio) / VOICES as f32;
            let expo_amount = voice.envelope.frequency(size_ratio);
            let rank = voice.rank;
            let linear_amount = rank * (rank + 0.01) * spread * 0.25;
            let f = f0 * semitones(48.0 * expo_amount * spread * rank) * (1.0 + linear_amount);
            voice.saw.render(f, amplitude, &mut block);
            size_ratio *= 0.97;
        }
        for (out, sample) in self.output.iter_mut().zip(block) {
            *out = self.limiter.process(PRE_GAIN, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::virtual_analog::VirtualAnalog;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Swarm::new();
        voice.seed(7);
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.5,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// How far apart two renders are, against the first one's energy.
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
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        let swarm = energy(&run(220.0, Settings::default(), 48_000));
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE, Settings::default()))
            .collect();
        let decibels = 10.0 * (swarm / energy(&va)).log10();
        assert!(decibels.abs() < 3.0, "{decibels} dB");
    }

    #[test]
    fn harmonics_timbre_and_morph_each_change_the_sound() {
        let base = run(220.0, plain(), 24_000);
        for settings in [
            Settings {
                harmonics: 0.9,
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
            let changed = difference(&base, &run(220.0, settings, 24_000));
            assert!(changed > 0.1, "{changed} at {settings:?}");
        }
    }

    #[test]
    fn the_same_seed_renders_the_same_samples() {
        assert_eq!(run(220.0, plain(), 4_800), run(220.0, plain(), 4_800));
        let mut other = Swarm::new();
        other.seed(8);
        let other: Vec<f32> = (0..4_800)
            .map(|_| other.next(220.0 / RATE, plain()))
            .collect();
        assert_ne!(run(220.0, plain(), 4_800), other);
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
