//! 808-style snare and hi-hat, from Mutable Instruments' Plaits. Ported to Rust from
//! `drums/analog_snare_drum.h` and `drums/hi_hat.h` with its square noise, and the state variable
//! filter in `stmlib/dsp/filter.h`.
//!
//! Copyright 2014, 2016 Emilie Gillet.
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

use std::f32::consts::PI;

use super::virtual_analog::{BLOCK, Settings};
use crate::rng::Pcg32;

/// How hard every note hits, as Plaits does with nothing patched to its level. A step's velocity
/// sets its loudness.
const ACCENT: f32 = 0.8;

#[inline]
fn semitones(interval: f32) -> f32 {
    2_f32.powf(interval / 12.0)
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

/// Noise on `0..1`.
#[inline]
fn noise(rng: &mut Pcg32) -> f32 {
    rng.next_u32() as f32 / 4_294_967_296.0
}

/// A state variable filter.
#[derive(Debug, Clone, Copy, Default)]
struct Svf {
    g: f32,
    r: f32,
    h: f32,
    state_1: f32,
    state_2: f32,
}

impl Svf {
    fn set_g_q(&mut self, g: f32, resonance: f32) {
        self.g = g;
        self.r = 1.0 / resonance;
        self.h = 1.0 / (1.0 + self.r * g + g * g);
    }

    /// Set by a quick approximation, for frequencies a resonator would use.
    fn set_fast(&mut self, f: f32, resonance: f32) {
        let a = 3.260e-01 * PI.powi(3);
        let b = 1.823e-01 * PI.powi(5);
        let f2 = f * f;
        self.set_g_q(f * (PI + f2 * (a + b * f2)), resonance);
    }

    fn set_exact(&mut self, f: f32, resonance: f32) {
        self.set_g_q((PI * f.min(0.497)).tan(), resonance);
    }

    /// The next high-pass and band-pass outputs.
    #[inline]
    fn process(&mut self, input: f32) -> (f32, f32) {
        let hp = (input - self.r * self.state_1 - self.g * self.state_1 - self.state_2) * self.h;
        let bp = self.g * hp + self.state_1;
        self.state_1 = self.g * hp + bp;
        let lp = self.g * bp + self.state_2;
        self.state_2 = self.g * bp + lp;
        (hp, bp)
    }
}

const SNARE_MODES: usize = 5;
const SNARE_RATIOS: [f32; SNARE_MODES] = [1.00, 2.00, 3.18, 4.16, 5.62];

/// One snare hit. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Snare {
    rate: f32,
    rng: Pcg32,
    pulse_remaining: i32,
    pulse: f32,
    pulse_height: f32,
    pulse_lp: f32,
    noise_envelope: f32,
    resonators: [Svf; SNARE_MODES],
    noise_filter: Svf,
    output: [f32; BLOCK],
    at: usize,
}

impl Snare {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate as f32,
            rng: Pcg32::new(0, 0),
            pulse_remaining: (1.0e-3 * rate as f32) as i32,
            pulse: 0.0,
            pulse_height: 3.0 + 7.0 * ACCENT,
            pulse_lp: 0.0,
            noise_envelope: 2.0,
            resonators: [Svf::default(); SNARE_MODES],
            noise_filter: Svf::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// Choose this hit's noise.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 1);
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.max(1e-5), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let Settings {
            harmonics: snappy,
            timbre: tone,
            morph: decay,
        } = settings;
        let decay_xt = decay * (1.0 + decay * (decay - 1.0));
        let pulse_decay = 1.0 - 1.0 / (0.1e-3 * self.rate);
        let q = 2000.0 * semitones(decay_xt * 84.0);
        let noise_decay = 1.0 - 0.0017 * semitones(-decay * (50.0 + snappy * 10.0));
        let exciter_leak = snappy * (2.0 - snappy) * 0.1;
        let snappy = (snappy * 1.1 - 0.05).clamp(0.0, 1.0);

        for (index, resonator) in self.resonators.iter_mut().enumerate() {
            let f = (f0 * SNARE_RATIOS[index]).min(0.499);
            let width = if index == 0 { q } else { q * 0.25 };
            resonator.set_fast(f, 1.0 + f * width);
        }

        let mut gain = [0.0; SNARE_MODES];
        if tone < 0.666_667 {
            // 808-style, with two modes.
            let tone = tone * 1.5;
            gain[0] = 1.5 + (1.0 - tone) * (1.0 - tone) * 4.5;
            gain[1] = 2.0 * tone + 0.15;
        } else {
            // What the 808 could have been with more.
            let mut tone = (tone - 0.666_667) * 3.0;
            gain[0] = 1.5 - tone * 0.5;
            gain[1] = 2.15 - tone * 0.7;
            for level in &mut gain[2..] {
                *level = tone;
                tone *= tone;
            }
        }

        let f_noise = (f0 * 16.0).clamp(0.0, 0.499);
        self.noise_filter.set_fast(f_noise, 1.0 + f_noise * 1.5);

        for out in &mut self.output {
            let pulse = if self.pulse_remaining > 0 {
                self.pulse_remaining -= 1;
                self.pulse = if self.pulse_remaining > 0 {
                    self.pulse_height
                } else {
                    self.pulse_height - 1.0
                };
                self.pulse
            } else {
                self.pulse *= pulse_decay;
                self.pulse
            };
            self.pulse_lp += (pulse - self.pulse_lp) * 0.75;

            let mut shell = 0.0;
            for (index, resonator) in self.resonators.iter_mut().enumerate() {
                let excitation = if index == 0 {
                    (pulse - self.pulse_lp) + 0.006 * pulse
                } else {
                    0.026 * pulse
                };
                let (_, band) = resonator.process(excitation);
                shell += gain[index] * (band + excitation * exciter_leak);
            }
            let shell = soft_clip(shell);

            let hiss = (2.0 * noise(&mut self.rng) - 1.0).max(0.0);
            self.noise_envelope *= noise_decay;
            let hiss = hiss * self.noise_envelope * snappy * 2.0;
            let (_, hiss) = self.noise_filter.process(hiss);
            *out = hiss + shell * (1.0 - snappy);
        }
    }
}

const HAT_RATIOS: [f32; 6] = [1.0, 1.304, 1.466, 1.787, 1.932, 2.536];

/// One hi-hat hit. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Hat {
    rate: f32,
    rng: Pcg32,
    envelope: f32,
    fresh: bool,
    noise_clock: f32,
    noise_sample: f32,
    phases: [u32; 6],
    colour: Svf,
    high_pass: Svf,
    output: [f32; BLOCK],
    at: usize,
}

impl Hat {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate as f32,
            rng: Pcg32::new(0, 0),
            envelope: 0.0,
            fresh: true,
            noise_clock: 0.0,
            noise_sample: 0.0,
            phases: [0; 6],
            colour: Svf::default(),
            high_pass: Svf::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// Choose this hit's noise.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 2);
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.max(1e-5), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let Settings {
            harmonics: noisiness,
            timbre: tone,
            morph: decay,
        } = settings;
        let envelope_decay = 1.0 - 0.003 * semitones(-decay * 84.0);
        if self.fresh {
            self.envelope = (1.5 + 0.5 * (1.0 - decay)) * (0.3 + 0.7 * ACCENT);
            self.fresh = false;
        }

        // Six square waves make the metallic noise.
        let mut increments = [0_u32; 6];
        for (increment, ratio) in increments.iter_mut().zip(HAT_RATIOS) {
            *increment = ((2.0 * f0 * ratio).min(0.499) * 4_294_967_296.0) as u32;
        }
        let cutoff = (150.0 / self.rate * semitones(tone * 72.0)).clamp(0.0, 16_000.0 / self.rate);
        self.colour.set_exact(cutoff, 3.0 + 3.0 * tone);
        self.high_pass.set_exact(cutoff, 0.5);

        // Clocked noise, which isn't in the 808, adds variety.
        let noisiness = noisiness * noisiness;
        let noise_f = (f0 * (16.0 + 16.0 * (1.0 - noisiness))).clamp(0.0, 0.5);

        for out in &mut self.output {
            let mut high = 0;
            for (phase, increment) in self.phases.iter_mut().zip(increments) {
                *phase = phase.wrapping_add(increment);
                high += *phase >> 31;
            }
            let metallic = 0.33 * high as f32 - 1.0;
            let (_, coloured) = self.colour.process(metallic);

            self.noise_clock += noise_f;
            if self.noise_clock >= 1.0 {
                self.noise_clock -= 1.0;
                self.noise_sample = noise(&mut self.rng) - 0.5;
            }
            let mixed = coloured + noisiness * (self.noise_sample - coloured);

            self.envelope *= envelope_decay;
            let swung = mixed * if mixed > 0.0 { 4.0 } else { 0.1 };
            let swung = swung / (1.0 + swung.abs());
            let (high_passed, _) = self.high_pass.process((swung + 0.1) * self.envelope);
            *out = high_passed;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.4,
            morph: 0.4,
        }
    }

    fn snare(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Snare::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn hat(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Hat::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |peak: f32, s| peak.max(s.abs()))
    }

    #[test]
    fn a_snare_hits_and_dies_away() {
        let out = snare(200.0, plain(), 48_000);
        assert!(peak(&out[..2_400]) > 0.1, "{}", peak(&out[..2_400]));
        assert!(peak(&out[38_400..]) < peak(&out[..2_400]) * 0.05);
    }

    #[test]
    fn a_hat_hits_and_dies_away() {
        let out = hat(500.0, plain(), 48_000);
        assert!(peak(&out[..2_400]) > 0.05, "{}", peak(&out[..2_400]));
        assert!(peak(&out[38_400..]) < peak(&out[..2_400]) * 0.05);
    }

    #[test]
    fn morph_is_how_long_each_rings() {
        let tail = |run: fn(f32, Settings, usize) -> Vec<f32>, hertz: f32, morph: f32| {
            let settings = Settings { morph, ..plain() };
            peak(&run(hertz, settings, 24_000)[9_600..])
        };
        assert!(tail(snare, 200.0, 0.8) > tail(snare, 200.0, 0.1) * 2.0);
        assert!(tail(hat, 500.0, 0.8) > tail(hat, 500.0, 0.1) * 2.0);
    }

    #[test]
    fn snappy_mixes_noise_into_a_snare() {
        // With none it's only the shell, and the same whatever the noise.
        let hit = |harmonics: f32, seed: u32| {
            let mut voice = Snare::new(RATE);
            voice.seed(seed);
            let settings = Settings {
                harmonics,
                ..plain()
            };
            (0..4_800)
                .map(|_| voice.next(200.0 / RATE as f32, settings))
                .collect::<Vec<f32>>()
        };
        assert_eq!(hit(0.0, 1), hit(0.0, 2));
        assert_ne!(hit(0.8, 1), hit(0.8, 2));
        assert_eq!(hit(0.8, 1), hit(0.8, 1), "repeatable");
    }

    #[test]
    fn tone_changes_a_hat() {
        let dark = Settings {
            timbre: 0.1,
            ..plain()
        };
        let bright = Settings {
            timbre: 0.9,
            ..plain()
        };
        assert_ne!(hat(500.0, dark, 4_800), hat(500.0, bright, 4_800));
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 100.0, 400.0, 2_000.0, 12_000.0, 30_000.0] {
            for harmonics in [0.0, 0.5, 1.0] {
                for timbre in [0.0, 0.5, 0.7, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                        };
                        for run in [snare, hat] {
                            for sample in run(hertz, settings, 4_800) {
                                assert!(
                                    sample.is_finite() && sample.abs() < 3.0,
                                    "{sample} at {hertz} {settings:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
