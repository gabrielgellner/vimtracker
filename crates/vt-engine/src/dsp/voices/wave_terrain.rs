//! Wave terrain synthesis from Mutable Instruments' Plaits: a path traced round an ellipse over a
//! surface, whose height along the way is the sound. Ported to Rust from
//! `engine2/wave_terrain_engine.cc` and the `FastSineOscillator` of `sine_oscillator.h`, with its
//! main output. Five of its surfaces are functions worked out as it plays; the last three are its
//! three banks of waves, `wav_integrated_waves`, read side by side as a surface, from the same
//! `tables/plaits_waves.bin` the wavetable voice plays.
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

use super::virtual_analog::{BLOCK, Moving, Settings};
use super::wavetable::{PER_BANK, WAVE, stored};
use crate::dsp::blocks::plaits_parts::{DcBlocker, interpolate};
use crate::dsp::blocks::sine_table::SINE;

/// The path is traced at twice the rate, and each pair of heights averaged, to tame aliasing.
const OVERSAMPLING: usize = 2;
const PATH: usize = BLOCK * OVERSAMPLING;
/// Plaits' eight surfaces, leaving out the ninth it reads from the user's own data.
const TERRAINS: usize = 8;
/// How loud it is against the other machines. Its surfaces run about ±1, and across its settings
/// the main output is mostly 0.45 to 0.8 RMS, where the virtual analog voice's half-level saw is
/// about 0.24. This brings the middle of that range level with it: at 220 Hz with Timbre and
/// Morph halfway on the first surface it measures 0.21 against the virtual analog voice's 0.24,
/// about a decibel quieter.
const LEVEL: f32 = 0.45;
/// The DC blocker's pole: a corner of about 3 Hz at 48 kHz, below anything played.
const DC_POLE: f32 = 1.0 - 20.0 / 48_000.0;

/// A sine of a phase in turns. The phase must not be negative, which is why every surface adds a
/// few turns to its sines' arguments.
#[inline]
fn sine(phase: f32) -> f32 {
    interpolate(&SINE, phase - phase.floor(), 512.0)
}

/// Pushes `x` towards ±1, harder for a larger `a`.
#[inline]
fn squash(x: f32, a: f32) -> f32 {
    let x = x * a;
    x / (1.0 + x.abs())
}

/// A sample of a stored wave, differentiated back from its integrated form as it is read.
#[inline]
fn differentiated(wave: usize, index: usize, fraction: f32) -> f32 {
    let a = stored(wave, index);
    let b = stored(wave, index + 1);
    let c = stored(wave, index + 2);
    (b - a) + (c - b - b + a) * fraction
}

/// A bank of waves read as a surface: `x` picks between the bank's waves, `y` runs along them.
#[inline]
fn wave_surface(x: f32, y: f32, bank: usize) -> f32 {
    // The path can stray just past ±1, so both stop at the edge, where Plaits would read past its
    // table weighted by nothing.
    let sample = ((y + 1.0) * 0.5 * WAVE as f32).clamp(0.0, WAVE as f32);
    let place = ((x + 1.0) * 0.5 * (PER_BANK - 1) as f32).clamp(0.0, (PER_BANK - 1) as f32);
    let (index, fraction) = (sample as usize, sample.fract());
    let wave = (place as usize).min(PER_BANK - 2);
    let across = place - wave as f32;
    let wave = bank * PER_BANK + wave;
    let a = differentiated(wave, index, fraction);
    let b = differentiated(wave + 1, index, fraction);
    (a + (b - a) * across) / 1024.0
}

/// The height of a surface at a point, with `x` and `y` about ±1.
#[inline]
fn terrain(x: f32, y: f32, index: usize) -> f32 {
    const K: f32 = 4.0;
    match index {
        0 => (squash(sine(K + x * 1.273), 2.0) - sine(K + y * (x + 1.571) * 0.637)) * 0.57,
        1 => {
            let xy = x * y;
            sine(K + sine(K + (x + y) * 0.637) / (0.2 + xy * xy) * 0.159)
        }
        2 => {
            let xy = x * y;
            sine(K + sine(K + 2.387 * xy) / (0.350 + xy * xy) * 0.159)
        }
        3 => {
            let xy = x * y;
            let xys = (x - 0.25) * (y + 0.25);
            sine(K + xy / (2.0 + (5.0 * xys).abs()) * 6.366)
        }
        4 => sine(
            0.159 / (0.170 + (y - 0.25).abs())
                + 0.477 / (0.350 + ((x + 0.5) * (y + 1.5)).abs())
                + K,
        ),
        // The wave banks, last first.
        5..=7 => wave_surface(x, y, 7 - index),
        _ => 0.0,
    }
}

/// A sine and cosine pair from the "magic circle", which needs no table: the path round the
/// surface. Its pitch and size move in a line across a block.
#[derive(Debug, Clone, Copy)]
struct Path {
    x: f32,
    y: f32,
    epsilon: f32,
    amplitude: f32,
}

impl Path {
    fn new() -> Self {
        Self {
            x: 1.0,
            y: 0.0,
            epsilon: 0.0,
            amplitude: 0.0,
        }
    }

    /// Twice the sine of a half turn's `frequency`, by a polynomial good to a few cents.
    #[inline]
    fn fast_2_sin(frequency: f32) -> f32 {
        let f_pi = frequency * std::f32::consts::PI;
        f_pi * (2.0 - (2.0 * 0.96 / 6.0) * f_pi * f_pi)
    }

    /// The targets for a frequency and size, fading to nothing towards a quarter of the rate.
    fn targets(frequency: f32, amplitude: f32) -> (f32, f32) {
        if frequency >= 0.25 {
            (Self::fast_2_sin(0.25), 0.0)
        } else {
            (
                Self::fast_2_sin(frequency),
                amplitude * (1.0 - frequency * 4.0),
            )
        }
    }

    fn jump_to(&mut self, frequency: f32, amplitude: f32) {
        (self.epsilon, self.amplitude) = Self::targets(frequency, amplitude);
    }

    fn render(
        &mut self,
        frequency: f32,
        amplitude: f32,
        xs: &mut [f32; PATH],
        ys: &mut [f32; PATH],
    ) {
        let (epsilon_target, amplitude_target) = Self::targets(frequency, amplitude);
        let mut epsilon = Moving::across(self.epsilon, epsilon_target, PATH);
        let mut size = Moving::across(self.amplitude, amplitude_target, PATH);
        let (mut x, mut y) = (self.x, self.y);
        // The circle drifts in size as rounding builds up, so it is put back now and then.
        let norm = x * x + y * y;
        if norm <= 0.5 || norm >= 2.0 {
            let scale = 1.0 / norm.sqrt();
            x *= scale;
            y *= scale;
        }
        for (out_x, out_y) in xs.iter_mut().zip(ys.iter_mut()) {
            let e = epsilon.next();
            x += e * y;
            y -= e * x;
            let amplitude = size.next();
            *out_x = x * amplitude;
            *out_y = y * amplitude;
        }
        (self.x, self.y) = (x, y);
        (self.epsilon, self.amplitude) = (epsilon_target, amplitude_target);
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct WaveTerrain {
    path: Path,
    offset: f32,
    terrain: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
    /// Plaits' engine leaves a large offset on some paths, which its output's coupling takes away
    /// in hardware. Here it would thump as the track's envelope opens and closes.
    dc_blocker: DcBlocker,
}

impl Default for WaveTerrain {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveTerrain {
    pub fn new() -> Self {
        Self {
            path: Path::new(),
            offset: 0.0,
            terrain: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
            dc_blocker: DcBlocker::new(DC_POLE),
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.clamp(1e-5, 0.49), settings);
            self.at = 0;
        }
        let sample = self.dc_blocker.process(self.output[self.at]);
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let harmonics = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let morph = settings.morph.clamp(0.0, 1.0);
        // The path grows with Timbre, but less so for high notes, whose wider paths would alias.
        let attenuation = (1.0 - 8.0 * f0).max(0.0);
        let radius = 0.1 + 0.9 * timbre * attenuation * (2.0 - attenuation);
        let path_frequency = f0 / OVERSAMPLING as f32;
        // Morph slides the path's centre from the left edge to near the right.
        let offset_target = 1.9 * morph - 1.0;
        let terrain_target = (harmonics * 1.05).min(1.0) * (TERRAINS as f32 - 1.0001);
        if self.fresh {
            // A new note starts where the settings are, rather than sweeping from the first ones.
            self.path.jump_to(path_frequency, radius);
            (self.offset, self.terrain) = (offset_target, terrain_target);
            self.fresh = false;
        }

        let mut xs = [0.0; PATH];
        let mut ys = [0.0; PATH];
        self.path.render(path_frequency, radius, &mut xs, &mut ys);

        let mut offset = Moving::towards(self.offset, offset_target);
        let mut place = Moving::towards(self.terrain, terrain_target);
        for (frame, out) in self.output.iter_mut().enumerate() {
            let x_offset = offset.next();
            let z = place.next().max(0.0);
            let (index, fraction) = (z as usize, z.fract());
            let mut sum = 0.0;
            for j in 0..OVERSAMPLING {
                let at = frame * OVERSAMPLING + j;
                let x = xs[at] * (1.0 - x_offset.abs()) + x_offset;
                let y = ys[at];
                let z0 = terrain(x, y, index);
                let z1 = terrain(x, y, index + 1);
                sum += z0 + (z1 - z0) * fraction;
            }
            *out = sum / OVERSAMPLING as f32;
        }
        (self.offset, self.terrain) = (offset_target, terrain_target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = WaveTerrain::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.0,
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
        energy(&apart) / (energy(a) + energy(b))
    }

    /// The harmonics setting that lands on a surface.
    fn on_terrain(index: usize) -> f32 {
        index as f32 / ((TERRAINS as f32 - 1.0001) * 1.05)
    }

    #[test]
    fn it_has_no_offset_once_it_settles() {
        for (harmonics, timbre, morph) in [(0.0, 0.5, 0.0), (0.0, 0.5, 1.0), (0.55, 0.1, 0.5)] {
            let settings = Settings {
                harmonics,
                timbre,
                morph,
                ..Settings::default()
            };
            let mut voice = WaveTerrain::new();
            let out: Vec<f32> = (0..96_000)
                .map(|_| voice.next(220.0 / 48_000.0, settings))
                .collect();
            let mean = out[48_000..].iter().sum::<f32>() / 48_000.0;
            assert!(mean.abs() < 0.01, "{mean} at {settings:?}");
        }
    }

    #[test]
    fn it_sounds() {
        let out = run(220.0, plain(), 48_000);
        assert!(energy(&out[43_200..]) > 1e-3, "{}", energy(&out[43_200..]));
    }

    #[test]
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        let ours = energy(&run(220.0, plain(), 48_000)).sqrt();
        let mut va = VirtualAnalog::new();
        let theirs: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE, Settings::default()))
            .collect();
        let theirs = energy(&theirs).sqrt();
        let decibels = 20.0 * (ours / theirs).log10();
        assert!(decibels.abs() < 3.0, "{ours} {theirs} {decibels}");
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let base = run(220.0, plain(), 4_800);
        for changed in [
            Settings {
                harmonics: 0.3,
                ..plain()
            },
            Settings {
                timbre: 1.0,
                ..plain()
            },
            Settings {
                morph: 0.1,
                ..plain()
            },
        ] {
            let out = run(220.0, changed, 4_800);
            assert!(difference(&base, &out) > 0.05, "{changed:?}");
        }
    }

    #[test]
    fn each_terrain_sounds_different() {
        let sounds: Vec<Vec<f32>> = (0..TERRAINS)
            .map(|index| {
                let settings = Settings {
                    harmonics: on_terrain(index),
                    timbre: 0.8,
                    ..plain()
                };
                run(220.0, settings, 4_800)
            })
            .collect();
        for (i, a) in sounds.iter().enumerate() {
            assert!(energy(a) > 1e-3, "terrain {i} is silent");
            for (j, b) in sounds.iter().enumerate().skip(i + 1) {
                assert!(difference(a, b) > 0.05, "terrains {i} and {j}");
            }
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 0.6, 0.75, 0.9, 1.0] {
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
        let mut voice = WaveTerrain::new();
        for frame in 0..48_000 {
            let sweep = (frame as f32 / 48_000.0 * 20.0).sin() * 0.5 + 0.5;
            let settings = Settings {
                harmonics: sweep,
                timbre: 1.0 - sweep,
                morph: sweep,
                ..Settings::default()
            };
            let sample = voice.next(220.0 / RATE, settings);
            assert!(sample.is_finite() && sample.abs() <= 1.0, "{sample}");
        }
    }
}
