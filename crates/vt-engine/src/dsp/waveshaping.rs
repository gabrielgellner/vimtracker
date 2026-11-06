//! A slope through a waveshaper and a wavefolder, from Mutable Instruments' Plaits. Ported to Rust
//! from `engine/waveshaping_engine.cc` and the slope of `oscillator/oscillator.h`, with its main
//! output.
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

use super::virtual_analog::{BLOCK, Moving, Settings, next_integrated_blep, this_integrated_blep};
use super::waveshaping_tables::{FOLD, SHAPES};

const MAX_FREQUENCY: f32 = 0.25;
const MIN_FREQUENCY: f32 = 0.000_001;
/// How loud it is against the other voices.
const LEVEL: f32 = 0.5;
/// The steps the fold table covers, leaving a point before and two after for interpolating.
const FOLD_SIZE: f32 = 512.0;

/// How far to back off a setting so a rich sound doesn't alias on a high note.
fn tame(f0: f32, harmonics: f32, order: f32) -> f32 {
    let f0 = f0 * harmonics;
    let max_f = 0.5 / order;
    let amount = (1.0 - (f0 - max_f) / (0.5 - max_f)).clamp(0.0, 1.0);
    amount * amount * amount
}

/// A value from the fold table, at a place on `0..=1`, with a smooth curve between points.
#[inline]
fn fold(index: f32) -> f32 {
    let at = index.clamp(0.0, 1.0) * FOLD_SIZE;
    let whole = at as usize;
    let f = at - whole as f32;
    // The table starts a point early, for the point before the first.
    let [before, here, next, after] = [
        FOLD[whole],
        FOLD[whole + 1],
        FOLD[whole + 2],
        FOLD[whole + 3],
    ];
    let c = (next - before) * 0.5;
    let v = here - next;
    let w = c + v;
    let a = w + v + (after - here) * 0.5;
    let b_neg = w + a;
    (((a * f) - b_neg) * f + c) * f + here
}

/// A value from between two waveshapers, for a place on `-1..=1`.
#[inline]
fn shaped(input: f32, shape: f32) -> f32 {
    let shape = shape * 3.9999;
    let table = shape as usize;
    let between = shape - table as f32;
    let place = 127.0 * input + 128.0;
    let index = place as i32;
    let f = place - index as f32;
    let index = (index & 255) as usize;
    let read = |table: &[i16; 257]| {
        let x0 = f32::from(table[index]) / 32768.0;
        let x1 = f32::from(table[index + 1]) / 32768.0;
        x0 + (x1 - x0) * f
    };
    let x = read(&SHAPES[table]);
    let y = read(&SHAPES[table + 1]);
    x + (y - x) * between
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Waveshaping {
    phase: f32,
    next_sample: f32,
    high: bool,
    frequency: f32,
    pw: f32,
    shape: f32,
    gain: f32,
    fresh: bool,
    output: [f32; BLOCK],
    at: usize,
}

impl Waveshaping {
    pub fn new() -> Self {
        Self {
            phase: 0.5,
            next_sample: 0.0,
            high: true,
            frequency: 0.001,
            pw: 0.5,
            shape: 0.0,
            gain: 0.0,
            fresh: true,
            output: [0.0; BLOCK],
            at: BLOCK,
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

    /// A slope, with its corners smoothed so they don't alias.
    fn render_slope(&mut self, frequency: f32, pw: f32) {
        let frequency = frequency.clamp(MIN_FREQUENCY, MAX_FREQUENCY);
        let pw = pw.clamp(
            frequency * 2.0,
            (1.0 - 2.0 * frequency).max(frequency * 2.0),
        );
        if self.fresh {
            self.frequency = frequency;
            self.pw = pw;
        }
        let mut fm = Moving::towards(self.frequency, frequency);
        let mut pwm = Moving::towards(self.pw, pw);
        let mut next_sample = self.next_sample;
        for out in &mut self.output {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let frequency = fm.next();
            let pw = pwm.next();
            self.phase += frequency;
            let up = 1.0 / pw;
            let down = 1.0 / (1.0 - pw);
            let corner = (up + down) * frequency;
            if self.high ^ (self.phase < pw) {
                let t = (self.phase - pw) / frequency;
                this_sample -= this_integrated_blep(t) * corner;
                next_sample -= next_integrated_blep(t) * corner;
                self.high = self.phase < pw;
            }
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = self.phase / frequency;
                this_sample += this_integrated_blep(t) * corner;
                next_sample += next_integrated_blep(t) * corner;
                self.high = true;
            }
            next_sample += if self.high {
                self.phase * up
            } else {
                1.0 - (self.phase - pw) * down
            };
            *out = 2.0 * this_sample - 1.0;
        }
        self.next_sample = next_sample;
        self.frequency = frequency;
        self.pw = pw;
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let Settings {
            harmonics,
            timbre,
            morph,
        } = settings;
        self.render_slope(f0, morph * 0.45 + 0.5);

        let slope = 3.0 + (morph - 0.5).abs() * 5.0;
        let shape_amount = (harmonics - 0.5).abs() * 2.0;
        let shape_taming = tame(f0, slope, 16.0);
        let gain_taming = tame(f0, slope * (3.0 + shape_amount * shape_taming * 5.0), 12.0);
        let shape = 0.5 + (harmonics - 0.5) * shape_taming;
        let gain = 0.03 + 0.46 * timbre * gain_taming;
        if self.fresh {
            self.shape = shape;
            self.gain = gain;
            self.fresh = false;
        }
        let mut shape_moving = Moving::towards(self.shape, shape);
        let mut gain_moving = Moving::towards(self.gain, gain);
        for out in &mut self.output {
            let mix = shaped(*out, shape_moving.next());
            *out = fold(mix * gain_moving.next() + 0.5);
        }
        self.shape = shape;
        self.gain = gain;
    }
}

impl Default for Waveshaping {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Waveshaping::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.0,
            morph: 0.0,
        }
    }

    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples.windows(2).map(|p| (p[1] - p[0]).powi(2)).sum();
        total / samples.len() as f32
    }

    #[test]
    fn it_plays_the_note_it_is_asked_for() {
        let out = run(110.0, plain(), 48_000);
        let upward = out
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        assert!((upward as i32 - 110).abs() <= 2, "{upward}");
    }

    #[test]
    fn its_tables_are_as_their_formulas_say() {
        // Linear goes straight through the middle, to full at each end.
        assert_eq!(SHAPES[2][128], 0);
        assert_eq!(SHAPES[2][256], 32767);
        assert_eq!(SHAPES[2][0], -32767);
        let peak = FOLD
            .iter()
            .fold(0.0_f32, |peak, value| peak.max(value.abs()));
        assert!((peak - 1.0).abs() < 0.001);
        assert_eq!(FOLD[FOLD.len() - 1], FOLD[FOLD.len() - 2]);
    }

    #[test]
    fn fold_makes_it_brighter_and_shape_and_slope_change_it() {
        let dull = run(110.0, plain(), 4_800);
        let folded = Settings {
            timbre: 0.9,
            ..plain()
        };
        assert!(roughness(&run(110.0, folded, 4_800)) > roughness(&dull) * 2.0);
        let shape = Settings {
            harmonics: 0.9,
            ..plain()
        };
        assert_ne!(run(110.0, shape, 4_800), dull);
        let slope = Settings {
            morph: 1.0,
            ..plain()
        };
        assert_ne!(run(110.0, slope, 4_800), dull);
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                        };
                        for sample in run(hertz, settings, 2_000) {
                            assert!(
                                sample.is_finite() && sample.abs() < 1.0,
                                "{sample} at {hertz} {settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
