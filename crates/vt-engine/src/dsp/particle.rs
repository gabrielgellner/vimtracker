//! Random impulses through resonant filters, then a diffuser, for crackle, rain and droplets, from
//! Mutable Instruments' Plaits. Ported to Rust from `particle_engine.cc`, `noise/particle.h`,
//! `fx/diffuser.h` and `fx/fx_engine.h`, with its main output, limited as Plaits limits it.
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

use super::plaits_parts::{CosineOscillator, Limiter, Svf, random, tan_dirty};
use super::virtual_analog::Settings;
use crate::rng::Pcg32;

/// How many frames are worked out at once, as in Plaits.
const BLOCK: usize = 24;
const PARTICLES: usize = 6;
/// How loud it is against the other machines, after Plaits' limiter with its pre-gain of 2.
const LEVEL: f32 = 0.83;
const PRE_GAIN: f32 = 2.0;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// A MIDI note's frequency as a fraction of Plaits' 48 kHz, for settings it measures in notes.
#[inline]
fn note_frequency(note: f32) -> f32 {
    440.0 / 48_000.0 * semitones(note - 69.0)
}

/// One stream of impulses through a band-pass at a random pitch near the note.
#[derive(Debug, Clone, Copy, Default)]
struct Particle {
    pre_gain: f32,
    filter: Svf,
}

impl Particle {
    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        rng: &mut Pcg32,
        sync: bool,
        density: f32,
        gain: f32,
        frequency: f32,
        spread: f32,
        q: f32,
        out: &mut [f32; BLOCK],
    ) {
        let mut u = if sync { density } else { random(rng) };
        let mut can_randomize_frequency = true;
        for out in out.iter_mut() {
            let mut s = 0.0;
            if u <= density {
                s = u * gain;
                if can_randomize_frequency {
                    let v = 2.0 * random(rng) - 1.0;
                    let f = (semitones(spread * v) * frequency).min(0.25);
                    self.pre_gain = 0.5 / (q * f * density.sqrt()).sqrt();
                    self.filter.set_g_q(tan_dirty(f), q);
                    // The cutoff stays put for the rest of the block.
                    can_randomize_frequency = false;
                }
            }
            *out += self.filter.band_pass(self.pre_gain * s);
            u = random(rng);
        }
    }
}

/// The lengths of the diffuser's delays, in the order they sit in its memory.
const LENGTHS: [usize; 7] = [126, 180, 269, 444, 1653, 2010, 3411];
const MEMORY: usize = 8192;

/// Where each delay starts in the memory, one past the end of the last.
const fn bases() -> [usize; 7] {
    let mut bases = [0; 7];
    let mut i = 1;
    while i < 7 {
        bases[i] = bases[i - 1] + LENGTHS[i - 1] + 1;
        i += 1;
    }
    bases
}
const BASES: [usize; 7] = bases();

/// A chain of all-pass filters and a modulated delay, as Plaits' `Diffuser`, in its 12-bit memory.
#[derive(Debug, Clone, Copy)]
struct Diffuser {
    buffer: [i16; MEMORY],
    write: usize,
    lfo: CosineOscillator,
    lfo_value: f32,
    lp_decay: f32,
    accumulator: f32,
    previous_read: f32,
}

impl Diffuser {
    fn new() -> Self {
        Self {
            buffer: [0; MEMORY],
            write: 0,
            lfo: CosineOscillator::new(0.3 / 48_000.0 * 32.0),
            lfo_value: 0.0,
            lp_decay: 0.0,
            accumulator: 0.0,
            previous_read: 0.0,
        }
    }

    #[inline]
    fn at(&self, line: usize, offset: usize) -> usize {
        (self.write + BASES[line] + offset) & (MEMORY - 1)
    }

    #[inline]
    fn load(&self, index: usize) -> f32 {
        f32::from(self.buffer[index]) / 4096.0
    }

    /// Add the end of a delay, times `scale`.
    #[inline]
    fn read_tail(&mut self, line: usize, scale: f32) {
        let value = self.load(self.at(line, LENGTHS[line] - 1));
        self.previous_read = value;
        self.accumulator += value * scale;
    }

    /// Write into the start of a delay, then scale what's left and add what was last read.
    #[inline]
    fn write_all_pass(&mut self, line: usize, scale: f32) {
        self.write_line(line, scale);
        self.accumulator += self.previous_read;
    }

    #[inline]
    fn write_line(&mut self, line: usize, scale: f32) {
        let index = self.at(line, 0);
        self.buffer[index] = (self.accumulator * 4096.0).clamp(-32768.0, 32767.0) as i16;
        self.accumulator *= scale;
    }

    /// Add a delay read between its samples, swayed by the LFO.
    #[inline]
    fn interpolate(&mut self, line: usize, offset: f32, amplitude: f32, scale: f32) {
        let offset = offset + amplitude * self.lfo_value;
        let whole = offset as usize;
        let fraction = offset - whole as f32;
        let a = self.load(self.at(line, whole));
        let b = self.load(self.at(line, whole + 1));
        let x = a + (b - a) * fraction;
        self.previous_read = x;
        self.accumulator += x * scale;
    }

    fn process(&mut self, amount: f32, rt: f32, block: &mut [f32; BLOCK]) {
        const KAP: f32 = 0.625;
        const KLP: f32 = 0.75;
        let mut lp = self.lp_decay;
        for sample in block.iter_mut() {
            self.write = (self.write + MEMORY - 1) % MEMORY;
            self.accumulator = 0.0;
            self.previous_read = 0.0;
            self.lfo_value = if self.write & 31 == 0 {
                self.lfo.next()
            } else {
                self.lfo.value()
            };
            self.accumulator += *sample;
            for line in 0..3 {
                self.read_tail(line, KAP);
                self.write_all_pass(line, -KAP);
            }
            self.interpolate(3, 400.0, 43.0, KAP);
            self.write_all_pass(3, -KAP);
            self.interpolate(6, 3070.0, 340.0, rt);
            lp += KLP * (self.accumulator - lp);
            self.accumulator = lp;
            self.read_tail(4, -KAP);
            self.write_all_pass(4, KAP);
            self.read_tail(5, KAP);
            self.write_all_pass(5, -KAP);
            self.write_line(6, 2.0);
            let wet = self.accumulator;
            *sample += amount * (wet - *sample);
        }
        self.lp_decay = lp;
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct ParticleNoise {
    rng: Pcg32,
    particles: [Particle; PARTICLES],
    post_filter: Svf,
    diffuser: Diffuser,
    limiter: Limiter,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for ParticleNoise {
    fn default() -> Self {
        Self::new()
    }
}

impl ParticleNoise {
    pub fn new() -> Self {
        Self {
            rng: Pcg32::new(0, 0),
            particles: [Particle::default(); PARTICLES],
            post_filter: Svf::default(),
            diffuser: Diffuser::new(),
            limiter: Limiter::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Choose its noise, so each note differs but a render repeats.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 3);
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
        let density_sqrt = note_frequency(60.0 + timbre * timbre * 72.0);
        let density = density_sqrt * density_sqrt * (1.0 / PARTICLES as f32);
        let gain = 1.0 / density;
        let q_sqrt = semitones(if morph >= 0.5 {
            (morph - 0.5) * 120.0
        } else {
            0.0
        });
        let q = 0.5 + q_sqrt * q_sqrt;
        let spread = 48.0 * harmonics * harmonics;
        let raw_diffusion_sqrt = 2.0 * (morph - 0.5).abs();
        let raw_diffusion = raw_diffusion_sqrt * raw_diffusion_sqrt;
        let diffusion = if morph < 0.5 { raw_diffusion } else { 0.0 };
        let sync = self.fresh;
        self.fresh = false;

        let mut block = [0.0; BLOCK];
        for particle in &mut self.particles {
            particle.render(
                &mut self.rng,
                sync,
                density,
                gain,
                f0,
                spread,
                q,
                &mut block,
            );
        }
        self.post_filter.set_g_q(tan_dirty(f0.min(0.49)), 0.5);
        for sample in &mut block {
            *sample = self.post_filter.low_pass(*sample);
        }
        self.diffuser.process(
            0.8 * diffusion * diffusion,
            0.5 * diffusion + 0.25,
            &mut block,
        );
        for (out, sample) in self.output.iter_mut().zip(block) {
            *out = self.limiter.process(PRE_GAIN, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = ParticleNoise::new();
        voice.seed(7);
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.3,
            timbre: 0.5,
            morph: 0.7,
        }
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    #[test]
    fn the_diffusers_delays_fit_its_memory() {
        assert_eq!(BASES[1], 127);
        assert!(BASES[6] + LENGTHS[6] <= MEMORY);
    }

    #[test]
    fn it_keeps_crackling() {
        let busy = Settings {
            timbre: 0.8,
            ..plain()
        };
        let out = run(880.0, busy, 96_000);
        assert!(energy(&out[..24_000]) > 1e-5, "{}", energy(&out[..24_000]));
        assert!(energy(&out[72_000..]) > 1e-5, "{}", energy(&out[72_000..]));
    }

    #[test]
    fn more_density_is_busier() {
        let sparse = run(
            880.0,
            Settings {
                timbre: 0.1,
                ..plain()
            },
            24_000,
        );
        let dense = run(
            880.0,
            Settings {
                timbre: 0.9,
                ..plain()
            },
            24_000,
        );
        let impulses = |out: &[f32]| out.iter().filter(|s| s.abs() > 0.01).count();
        assert!(
            impulses(&dense) > impulses(&sparse),
            "{} {}",
            impulses(&dense),
            impulses(&sparse)
        );
    }

    #[test]
    fn diffusion_changes_it() {
        let out = run(880.0, plain(), 4_800);
        let diffused = run(
            880.0,
            Settings {
                morph: 0.1,
                ..plain()
            },
            4_800,
        );
        assert_ne!(out, diffused);
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.5, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
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
