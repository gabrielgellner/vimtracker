//! A struck resonator of 24 modes, for bells, bars, plates and woodblocks, from Mutable
//! Instruments' Plaits. Ported to Rust from `modal_engine.cc`,
//! `physical_modelling/modal_voice.cc` and `physical_modelling/resonator.cc`, with its main
//! output, limited as Plaits limits it.
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

use super::plaits_parts::{CosineOscillator, Limiter, STIFFNESS, interpolate, tan_fast};
use super::virtual_analog::Settings;

/// How many frames are worked out at once, as in Plaits.
const BLOCK: usize = 24;
const MODES: usize = 24;
/// How loud it is against the other machines.
const LEVEL: f32 = 0.83;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// How much the third partial's stretch shifts the note, so it can be taken back out.
fn nth_harmonic_compensation(n: usize, mut stiffness: f32) -> f32 {
    let mut stretch = 1.0;
    for _ in 0..n - 1 {
        stretch += stiffness;
        stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
    }
    1.0 / stretch
}

/// One band-pass or low-pass mode, as Plaits' `ResonatorSvf` with its frequency and Q set once a
/// block.
#[derive(Debug, Clone, Copy, Default)]
struct Mode {
    state_1: f32,
    state_2: f32,
}

impl Mode {
    /// Filter a block, adding `gain` times the band-pass, or the low-pass, into `out`.
    fn process(
        &mut self,
        f: f32,
        q: f32,
        gain: f32,
        low_pass: bool,
        input: &[f32; BLOCK],
        out: &mut [f32; BLOCK],
    ) {
        let g = tan_fast(f);
        let r = 1.0 / q;
        let h = 1.0 / (1.0 + r * g + g * g);
        let r_plus_g = r + g;
        let (mut state_1, mut state_2) = (self.state_1, self.state_2);
        for (sample, out) in input.iter().zip(out.iter_mut()) {
            let hp = (sample - r_plus_g * state_1 - state_2) * h;
            let bp = g * hp + state_1;
            state_1 = g * hp + bp;
            let lp = g * bp + state_2;
            state_2 = g * bp + lp;
            *out += gain * if low_pass { lp } else { bp };
        }
        (self.state_1, self.state_2) = (state_1, state_2);
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Modal {
    harmonics_lp: f32,
    excitation: Mode,
    amplitudes: [f32; MODES],
    modes: [Mode; MODES],
    limiter: Limiter,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Modal {
    fn default() -> Self {
        Self::new()
    }
}

impl Modal {
    pub fn new() -> Self {
        // Where it's struck, which sets how loud each mode is.
        let mut amplitudes = CosineOscillator::new(0.015);
        Self {
            harmonics_lp: 0.0,
            excitation: Mode::default(),
            amplitudes: std::array::from_fn(|_| amplitudes.next() * 0.25),
            modes: [Mode::default(); MODES],
            limiter: Limiter::default(),
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
        let trigger = self.fresh;
        let harmonics = settings.harmonics.clamp(0.0, 1.0);
        if self.fresh {
            self.harmonics_lp = harmonics;
            self.fresh = false;
        }
        self.harmonics_lp += 0.01 * (harmonics - self.harmonics_lp);
        let structure = self.harmonics_lp;
        let mut brightness = settings.timbre.clamp(0.0, 1.0);
        let mut damping = settings.morph.clamp(0.0, 1.0);
        let accent = settings.accent.clamp(0.0, 1.0);
        brightness += 0.25 * accent * (1.0 - brightness);
        damping += 0.25 * accent * (1.0 - damping);

        // The strike: one impulse, through a low-pass set by how bright it is.
        let cutoff =
            (2.0 * f0 * semitones((brightness * (2.0 - brightness) - 0.5) * 60.0)).min(0.499);
        let mut strike = [0.0; BLOCK];
        if trigger {
            let attenuation = 1.0 - damping * 0.5;
            let amplitude = (0.12 + 0.08 * accent) * attenuation;
            strike[0] = amplitude * semitones(cutoff * cutoff * 24.0) / cutoff;
        }
        let mut excitation = [0.0; BLOCK];
        self.excitation
            .process(cutoff, 1.5, 1.0, true, &strike, &mut excitation);

        // The resonator.
        let mut stiffness = interpolate(&STIFFNESS, structure, 64.0);
        let f0 = f0 * nth_harmonic_compensation(3, stiffness);
        let mut harmonic = f0;
        let mut stretch = 1.0;
        let q_sqrt = semitones(damping * 79.7);
        let mut q = 500.0 * q_sqrt * q_sqrt;
        brightness *= 1.0 - structure * 0.3;
        brightness *= 1.0 - damping * 0.3;
        let q_loss = brightness * (2.0 - brightness) * 0.85 + 0.15;
        let mut output = [0.0; BLOCK];
        for (mode, amplitude) in self.modes.iter_mut().zip(self.amplitudes) {
            let frequency = (harmonic * stretch).min(0.499);
            let attenuation = 1.0 - frequency * 2.0;
            mode.process(
                frequency,
                1.0 + frequency * q,
                amplitude * attenuation,
                false,
                &excitation,
                &mut output,
            );
            stretch += stiffness;
            // Partials mustn't fold back below nothing, and a few more fit at the top.
            stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
            harmonic += f0;
            q *= q_loss;
        }
        for (out, sample) in self.output.iter_mut().zip(output) {
            *out = self.limiter.process(1.0, sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Modal::new();
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

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn the_partials_start_loud_and_fall_away_across_the_bar() {
        let voice = Modal::new();
        assert!((voice.amplitudes[0] - 0.25).abs() < 0.01);
        assert!(voice.amplitudes[MODES - 1] < voice.amplitudes[0]);
    }

    #[test]
    fn a_strike_rings_and_dies_away() {
        let out = run(440.0, plain(), 96_000);
        let loud = peak(&out[..4_800]);
        assert!(loud > 0.05, "{loud}");
        assert!(peak(&out[86_400..]) < loud * 0.1, "it rings on");
    }

    #[test]
    fn more_decay_rings_longer() {
        let short = run(440.0, plain(), 48_000);
        let long = run(
            440.0,
            Settings {
                morph: 0.9,
                ..plain()
            },
            48_000,
        );
        assert!(peak(&long[38_400..]) > peak(&short[38_400..]) * 2.0);
    }

    #[test]
    fn each_setting_changes_it() {
        let out = run(440.0, plain(), 4_800);
        for other in [
            Settings {
                harmonics: 0.0,
                ..plain()
            },
            Settings {
                timbre: 0.9,
                ..plain()
            },
        ] {
            assert_ne!(run(440.0, other, 4_800), out, "{other:?}");
        }
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
