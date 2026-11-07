//! An 8 by 8 grid of waves in four banks, from Mutable Instruments' Plaits. Ported to Rust from
//! `wavetable_engine.cc` and `wavetable_oscillator.h`, with its main output. The waves are its
//! `wav_integrated_waves`, taken from the generated `resources.cc` into `tables/plaits_waves.bin`.
//! Each is stored integrated, and played back differentiated, which keeps high notes from
//! aliasing.
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

use super::virtual_analog::Settings;

/// How many frames are worked out at once, as in Plaits, whose smoothing is set per block.
const BLOCK: usize = 24;
/// Samples in one wave, and the four more each is stored with so reads past its end need no wrap.
const WAVE: usize = 128;
const STORED: usize = WAVE + 4;
const WAVES: usize = 192;
const PER_BANK: usize = 64;
/// How loud it is against the other machines.
const LEVEL: f32 = 0.5;

/// The waves, as little-endian 16-bit samples.
static DATA: &[u8; WAVES * STORED * 2] = include_bytes!("tables/plaits_waves.bin");

/// One stored sample of a wave.
#[inline]
fn stored(wave: usize, index: usize) -> f32 {
    let at = (wave * STORED + index) * 2;
    f32::from(i16::from_le_bytes([DATA[at], DATA[at + 1]]))
}

/// Which stored wave each place in the four banks plays. The fourth bank is the first three
/// shuffled, as Plaits has it with no waves of the user's own.
const fn wave_map() -> [u8; PER_BANK * 4] {
    let mut map = [0; PER_BANK * 4];
    let mut i = 0;
    while i < map.len() {
        map[i] = if i < PER_BANK * 3 {
            i as u8
        } else {
            (i * 101 % WAVES) as u8
        };
        i += 1;
    }
    map
}
const WAVE_MAP: [u8; PER_BANK * 4] = wave_map();

/// A wave at a place in the grid, read between its samples with a Hermite curve.
#[inline]
fn read(x: usize, y: usize, z: usize, index: usize, fraction: f32) -> f32 {
    let wave = usize::from(WAVE_MAP[x + y * 8 + z * PER_BANK]);
    let xm1 = stored(wave, index);
    let x0 = stored(wave, index + 1);
    let x1 = stored(wave, index + 2);
    let x2 = stored(wave, index + 3);
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b_neg = w + a;
    (((a * fraction) - b_neg) * fraction + c) * fraction + x0
}

/// Pulls a fraction towards 0 or 1, steeper for a larger amount.
#[inline]
fn pull(x: f32, amount: f32) -> f32 {
    ((x - 0.5) * amount).clamp(-0.5, 0.5) + 0.5
}

/// Moves a state a share of the way to a value, as Plaits' `ONE_POLE`.
#[inline]
fn one_pole(state: &mut f32, value: f32, coefficient: f32) {
    *state += coefficient * (value - *state);
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Wavetable {
    phase: f32,
    /// The settings as grid positions, smoothed once a block and then once a frame.
    pre: [f32; 3],
    smoothed: [f32; 3],
    previous: [f32; 3],
    previous_frequency: f32,
    /// The differentiator, which turns an integrated wave back into the wave.
    slope: f32,
    last: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Wavetable {
    fn default() -> Self {
        Self::new()
    }
}

impl Wavetable {
    pub fn new() -> Self {
        Self {
            phase: 0.0,
            pre: [0.0; 3],
            smoothed: [0.0; 3],
            previous: [0.0; 3],
            previous_frequency: 0.0,
            slope: 0.0,
            last: 0.0,
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

    /// Where the three settings put the grid: Timbre across a row, Morph down the rows, and
    /// Harm through the banks and back.
    fn targets(settings: Settings) -> [f32; 3] {
        [
            settings.timbre.clamp(0.0, 1.0) * 6.9999,
            settings.morph.clamp(0.0, 1.0) * 6.9999,
            settings.harmonics.clamp(0.0, 1.0) * 6.9999,
        ]
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let frequency = frequency.clamp(1e-6, 0.45);
        let targets = Self::targets(settings);
        if self.fresh {
            // A new note starts where the settings are, rather than sweeping from the first wave.
            self.pre = targets;
            self.previous_frequency = frequency;
        }
        one_pole(&mut self.pre[0], targets[0], 0.2);
        one_pole(&mut self.pre[1], targets[1], 0.2);
        one_pole(&mut self.pre[2], targets[2], 0.05);
        // Past the third bank, Harm snaps more and more to whole waves.
        let quantization = (self.pre[2] - 3.0).clamp(0.0, 1.0);
        let coefficient = (2.0 * frequency * (4.0 - 3.0 * quantization)).clamp(0.01, 0.1);
        let mut aims = [0.0; 3];
        for (aim, pre) in aims.iter_mut().zip(self.pre) {
            let whole = pre.trunc();
            let fraction = pre - whole;
            *aim = whole + fraction + quantization * (pull(fraction, 16.0) - fraction);
        }
        if self.fresh {
            self.previous = aims;
            self.smoothed = aims;
        }
        let steps: [f32; 3] = std::array::from_fn(|i| (aims[i] - self.previous[i]) / BLOCK as f32);
        let frequency_step = (frequency - self.previous_frequency) / BLOCK as f32;
        for frame in 0..BLOCK {
            let f0 = self.previous_frequency + frequency_step * (frame + 1) as f32;
            let gain = (1.0 / (f0 * 131_072.0)) * (0.95 - f0);
            let cutoff = (WAVE as f32 * f0).min(1.0);
            for ((smoothed, previous), step) in
                self.smoothed.iter_mut().zip(self.previous).zip(steps)
            {
                one_pole(smoothed, previous + step * (frame + 1) as f32, coefficient);
            }
            // Snapping can land a place on 7, past the last row or bank, which Plaits reads just
            // outside its table, weighted by nothing. Stopping just short of it sounds the same.
            let [x, y, z] = self.smoothed.map(|place| place.clamp(0.0, 6.9999));
            let (x0, xf) = (x as usize, x.fract());
            let (y0, yf) = (y as usize, y.fract());
            let (z_whole, zf) = (z as usize, z.fract());
            // The banks run there and back: 0 1 2 3 3 2 1 0.
            let mirror = |z: usize| if z >= 4 { 7 - z } else { z };
            let (z0, z1) = (mirror(z_whole), mirror(z_whole + 1));

            self.phase += f0;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let p = self.phase * WAVE as f32;
            let index = (p as usize).min(WAVE - 1);
            let pf = p - index as f32;

            let layer = |z: usize| {
                let row = |y: usize| {
                    let a = read(x0, y, z, index, pf);
                    let b = read(x0 + 1, y, z, index, pf);
                    a + (b - a) * xf
                };
                let (a, b) = (row(y0), row(y0 + 1));
                a + (b - a) * yf
            };
            let (a, b) = (layer(z0), layer(z1));
            let mix = a + (b - a) * zf;
            if self.fresh {
                self.last = mix;
                self.fresh = false;
            }
            one_pole(&mut self.slope, mix - self.last, cutoff);
            self.last = mix;
            self.output[frame] = self.slope * gain;
        }
        self.previous = aims;
        self.previous_frequency = frequency;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Wavetable::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn at(harmonics: f32, timbre: f32, morph: f32) -> Settings {
        Settings {
            harmonics,
            timbre,
            morph,
            ..Settings::default()
        }
    }

    #[test]
    fn the_first_wave_is_a_sine_at_the_note() {
        let out = run(110.0, at(0.0, 0.0, 0.0), 48_000);
        let upward = out[2_400..]
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        let expected = 110.0 * (48_000.0 - 2_400.0) / 48_000.0;
        assert!((upward as f32 - expected).abs() <= 2.0, "{upward}");
        let peak = out[2_400..]
            .iter()
            .fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!(peak > 0.2 && peak <= 1.0, "{peak}");
    }

    #[test]
    fn each_setting_changes_the_wave() {
        let plain = run(110.0, at(0.1, 0.1, 0.1), 4_800);
        for other in [at(0.6, 0.1, 0.1), at(0.1, 0.6, 0.1), at(0.1, 0.1, 0.6)] {
            assert_ne!(run(110.0, other, 4_800), plain, "{other:?}");
        }
    }

    #[test]
    fn a_new_note_starts_on_its_wave_without_a_click() {
        let out = run(220.0, at(0.7, 0.9, 0.4), 480);
        let jump = out
            .windows(2)
            .fold(0.0_f32, |most, pair| most.max((pair[1] - pair[0]).abs()));
        assert!(jump < 0.5, "{jump}");
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.3, 0.5, 0.8, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = at(harmonics, timbre, morph);
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
