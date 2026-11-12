//! Wavetable models from Mutable Instruments' Braids: a 16 by 16 map of its waves to wander
//! over, a line of them to scan with rough or smooth steps between, and a paraphonic chord of
//! four voices on them. Ported to Rust from `digital_oscillator.cc`, at twice the program's rate
//! as Braids runs at 96 kHz, through [`super::braids_parts`]. Its 256 waves, 129 bytes each, are
//! in `tables/braids_waves.bin`, taken from its generated `resources.cc`.
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

use super::braids_parts::{INNER, Lcg, Macro, Model, Pitch, whole};
use super::virtual_analog::Settings;

/// A map of waves to wander over, one setting across it and the other down.
pub type WaveMap = Macro<WaveMapModel>;
/// A line of waves to scan, with rough or smooth steps between them.
pub type WaveLine = Macro<WaveLineModel>;
/// A chord of four voices on a line of waves.
pub type Paraphonic = Macro<ParaphonicModel>;

/// Braids' waves: 256 of them, each a cycle of 128 bytes and the first again.
static WAVES: &[u8; 256 * WAVE_SIZE] = include_bytes!("tables/braids_waves.bin");
const WAVE_SIZE: usize = 129;

/// A wave, by its number.
#[inline]
fn wave(index: u8) -> &'static [u8] {
    let start = usize::from(index) * WAVE_SIZE;
    &WAVES[start..start + WAVE_SIZE]
}

/// stmlib's `Interpolate824` on a wave of bytes, at a phase of which the top 8 bits are the
/// place, as a 16-bit sample.
#[inline]
fn read(wave: &[u8], phase: u32) -> i32 {
    let at = (phase >> 24) as usize;
    let (a, b) = (i64::from(wave[at]), i64::from(wave[at + 1]));
    ((a << 8) + (((b - a) * i64::from(phase & 0xFF_FFFF)) >> 16) - 32_768) as i32
}

/// Two waves read at a phase and blended, from the first at 0 to the second at 65 535.
#[inline]
fn crossfade(a: &[u8], b: &[u8], phase: u32, balance: u32) -> i32 {
    let (a, b) = (read(a, phase), read(b, phase));
    a + (((b - a) as i64 * i64::from(balance & 0xFFFF)) >> 16) as i32
}

/// stmlib's `Mix` in whole numbers, which keeps only the bottom 16 bits of its balance.
#[inline]
fn mix16(a: i32, b: i32, balance: u32) -> i32 {
    let balance = i64::from(balance & 0xFFFF);
    ((i64::from(a) * (65_535 - balance) + i64::from(b) * balance) >> 16) as i32
}

/// A phase increment in whole turns of `u32`.
#[inline]
fn increment(pitch: Pitch, semitones: f32) -> u32 {
    (pitch.increment(semitones) * 4_294_967_296.0) as u32
}

/// Braids' map of waves, 16 by 16.
#[rustfmt::skip]
const MAP: [u8; 256] = [
    176, 255, 202, 193, 121, 122, 124, 123, 95, 197, 3, 8, 9, 35, 34, 32, 177, 30, 201, 185, 186,
    83, 80, 54, 195, 196, 42, 41, 36, 37, 38, 33, 178, 31, 199, 184, 165, 82, 79, 86, 78, 45, 46,
    47, 43, 44, 40, 39, 192, 198, 200, 183, 164, 81, 25, 26, 126, 125, 0, 1, 2, 6, 23, 22, 191, 179,
    227, 228, 166, 84, 85, 14, 127, 128, 129, 130, 131, 132, 7, 209, 190, 134, 226, 229, 253, 204,
    55, 12, 15, 10, 11, 163, 162, 167, 171, 208, 189, 133, 225, 230, 254, 216, 215, 214, 157, 158,
    159, 160, 161, 168, 169, 170, 174, 234, 233, 231, 218, 217, 206, 205, 213, 56, 244, 242, 116,
    115, 114, 243, 188, 235, 238, 232, 219, 207, 210, 211, 194, 145, 203, 245, 111, 112, 113, 108,
    63, 27, 156, 237, 5, 149, 148, 246, 143, 144, 212, 52, 110, 109, 104, 172, 187, 28, 17, 236,
    120, 150, 147, 50, 142, 141, 62, 60, 59, 102, 103, 57, 182, 29, 13, 239, 119, 151, 135, 138,
    139, 140, 248, 99, 98, 100, 101, 61, 21, 19, 4, 240, 118, 152, 136, 137, 250, 251, 249, 247, 96,
    53, 105, 106, 175, 24, 16, 241, 117, 153, 154, 155, 146, 51, 94, 93, 49, 97, 58, 107, 181, 252,
    18, 220, 221, 64, 89, 90, 92, 91, 88, 87, 65, 66, 67, 68, 173, 20, 224, 223, 222, 48, 180, 77,
    76, 75, 74, 73, 72, 71, 70, 69,
];

#[derive(Debug, Clone, Copy)]
pub struct WaveMapModel {
    phase: u32,
}

impl Model for WaveMapModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self { phase: 0 }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        // The grid is 16 by 16, so there are 15 squares to blend across each way.
        let p = [whole(settings.at(0)), whole(settings.at(1))].map(|p| ((p * 15) >> 4) as u32);
        let fade = p.map(|p| (p << 5) & 0xFFFF);
        let at = p.map(|p| (p >> 11) as usize);
        let waves: [[&[u8]; 2]; 2] = std::array::from_fn(|i| {
            std::array::from_fn(|j| wave(MAP[(at[0] + i) * 16 + at[1] + j]))
        });
        let half = increment(pitch, 0.0) >> 1;
        for out in out.iter_mut() {
            let mut sample = 0;
            // Worked out twice a frame, plainly.
            for _ in 0..2 {
                self.phase = self.phase.wrapping_add(half);
                let phase = self.phase >> 1;
                let first = crossfade(waves[0][0], waves[0][1], phase, fade[1]);
                let second = crossfade(waves[1][0], waves[1][1], phase, fade[1]);
                sample += mix16(first, second, fade[0]) >> 1;
            }
            *out = sample as f32 / 32_768.0;
        }
    }
}

/// The waves `Wave Line` scans along, the last read past its end as Braids reads it.
#[rustfmt::skip]
const LINE: [u8; 65] = [
    187, 179, 154, 155, 135, 134, 137, 19, 24, 3, 8, 66, 79, 25, 180, 174, 64, 127, 198, 15, 10, 7,
    11, 0, 191, 192, 115, 238, 237, 236, 241, 47, 70, 76, 235, 26, 133, 208, 34, 175, 183, 146, 147,
    148, 150, 151, 152, 153, 117, 138, 32, 33, 35, 125, 199, 201, 30, 31, 193, 27, 29, 21, 18, 182,
    157,
];
/// A shorter line, for the paraphonic chord.
#[rustfmt::skip]
const MINI_LINE: [u8; 33] = [
    157, 161, 171, 188, 189, 191, 192, 193, 196, 198, 201, 234, 232, 229, 226, 224, 1, 2, 3, 4, 5, 8,
    12, 32, 36, 42, 47, 252, 254, 141, 139, 135, 174,
];

/// Keep only the top 7 bits of a phase's place, then the top 5, for rough steps.
const ROUGH: u32 = 0xFE00_0000;
const ROUGHER: u32 = 0xF800_0000;

#[derive(Debug, Clone, Copy)]
pub struct WaveLineModel {
    phase: u32,
    smoothed: Option<u32>,
    previous: u32,
}

impl Model for WaveLineModel {
    const LEVEL: f32 = 0.55;

    fn new() -> Self {
        Self {
            phase: 0,
            smoothed: None,
            previous: 0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (scan, style) = (whole(settings.at(0)) as u32, whole(settings.at(1)) as u32);
        let smoothed = match self.smoothed {
            Some(was) => (3 * was + (scan << 1)) >> 2,
            None => {
                self.previous = scan;
                scan << 1
            }
        };
        self.smoothed = Some(smoothed);
        let scan = smoothed & 0xFFFF;
        let wave_0 = wave(LINE[(self.previous >> 9) as usize]);
        let wave_1 = wave(LINE[(scan >> 10) as usize]);
        let wave_2 = wave(LINE[(scan >> 10) as usize + 1]);
        let smooth_fade = (scan << 6) & 0xFFFF;
        let mut rough_fade = 0_u32;
        let rough_step = 32_768 / INNER as u32;
        let balance = style << 3;
        let half = increment(pitch, 0.0) >> 1;
        for out in out.iter_mut() {
            let mut sample = 0;
            for _ in 0..2 {
                let phase = self.phase >> 1;
                // Four ways along `Steps`: from the last wave to this one, stepped or smooth, then
                // between the two either side of the scan, smooth or stepped, then more stepped.
                let (a, b) = if style < 8192 {
                    let rough = crossfade(wave_0, wave_1, phase & ROUGH, rough_fade);
                    let smooth = crossfade(wave_0, wave_1, phase, rough_fade);
                    (rough, smooth)
                } else if style < 16_384 {
                    let rough = crossfade(wave_0, wave_1, phase, rough_fade);
                    let smooth = crossfade(wave_1, wave_2, phase, smooth_fade);
                    (rough, smooth)
                } else if style < 24_576 {
                    let smooth = crossfade(wave_1, wave_2, phase, smooth_fade);
                    let rough = crossfade(wave_1, wave_2, phase & ROUGH, smooth_fade);
                    (smooth, rough)
                } else {
                    let smooth = crossfade(wave_1, wave_2, phase & ROUGH, smooth_fade);
                    let rough = crossfade(wave_1, wave_2, phase & ROUGHER, smooth_fade);
                    (smooth, rough)
                };
                sample += mix16(a, b, balance);
                self.phase = self.phase.wrapping_add(half);
                if style < 16_384 {
                    rough_fade += rough_step;
                }
            }
            *out = (sample >> 1) as f32 / 32_768.0;
        }
        self.previous = smoothed >> 1;
    }
}

/// The chords `Paraphonic`'s three upper voices play, in 128ths of a semitone over the note, from
/// a slight detune through fifths, minor and major chords to octaves.
const SEMI: i32 = 128;
#[rustfmt::skip]
const CHORDS: [[i32; 3]; 17] = [
    [2, 4, 6],
    [16, 32, 48],
    [2 * SEMI, 7 * SEMI, 12 * SEMI],
    [3 * SEMI, 7 * SEMI, 10 * SEMI],
    [3 * SEMI, 7 * SEMI, 12 * SEMI],
    [3 * SEMI, 7 * SEMI, 14 * SEMI],
    [3 * SEMI, 7 * SEMI, 17 * SEMI],
    [7 * SEMI, 12 * SEMI, 19 * SEMI],
    [7 * SEMI, 3 + 12 * SEMI, 5 + 19 * SEMI],
    [4 * SEMI, 7 * SEMI, 17 * SEMI],
    [4 * SEMI, 7 * SEMI, 14 * SEMI],
    [4 * SEMI, 7 * SEMI, 12 * SEMI],
    [4 * SEMI, 7 * SEMI, 11 * SEMI],
    [5 * SEMI, 7 * SEMI, 12 * SEMI],
    [4, 7 * SEMI, 12 * SEMI],
    [4, 4 + 12 * SEMI, 12 * SEMI],
    [4, 4 + 12 * SEMI, 12 * SEMI],
];

#[derive(Debug, Clone, Copy)]
pub struct ParaphonicModel {
    phases: [u32; 4],
    struck: bool,
    noise: Lcg,
}

impl Model for ParaphonicModel {
    const LEVEL: f32 = 1.1;

    fn new() -> Self {
        Self {
            phases: [0; 4],
            struck: false,
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        if !self.struck {
            // Each voice starts anywhere in its cycle.
            for phase in &mut self.phases {
                *phase = self.noise.word();
            }
            self.struck = true;
        }
        let (scan, chord) = (whole(settings.at(0)) as u32, whole(settings.at(1)));
        let index = (chord >> 11) as usize;
        // Catch on each chord for most of its stretch, then slide to the next.
        let fraction = ((chord << 5) & 0xFFFF) as u32;
        let fraction = if fraction < 30_720 {
            0
        } else if fraction >= 34_816 {
            65_535
        } else {
            (fraction - 30_720) * 16
        } as i32;
        let mut increments = [increment(pitch, 0.0); 4];
        for (voice, increment_at) in increments.iter_mut().skip(1).enumerate() {
            let (a, b) = (CHORDS[index][voice], CHORDS[index + 1][voice]);
            let detune = a + (((b - a) * fraction) >> 16);
            *increment_at = increment(pitch, detune as f32 / SEMI as f32);
        }
        let wave_1 = wave(MINI_LINE[(scan >> 10) as usize]);
        let wave_2 = wave(MINI_LINE[(scan >> 10) as usize + 1]);
        let fade = (scan << 6) & 0xFFFF;
        for out in out.iter_mut() {
            let mut sample = 0;
            for (phase, increment) in self.phases.iter_mut().zip(increments) {
                *phase = phase.wrapping_add(increment);
                sample += crossfade(wave_1, wave_2, *phase >> 1, fade);
            }
            *out = (sample >> 2) as f32 / 32_768.0;
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
        voice.seed(7);
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
        check("Map", &|hertz, values, frames| {
            run::<WaveMapModel>(hertz, values, frames)
        });
        check("Line", &|hertz, values, frames| {
            run::<WaveLineModel>(hertz, values, frames)
        });
        check("Paraphonic", &|hertz, values, frames| {
            run::<ParaphonicModel>(hertz, values, frames)
        });
    }

    #[test]
    fn the_waves_are_whole() {
        // Each of the 256 waves is a cycle of 128 bytes and its first again, so it loops.
        for index in 0..=255 {
            let wave = wave(index);
            assert_eq!(wave.len(), WAVE_SIZE);
            assert_eq!(wave[0], wave[128], "wave {index}");
        }
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.25, 0.25], [0.75, 0.6]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                assert!(decibels.abs() < 6.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            let base = run(220.0, [0.3, 0.3], 9_600);
            for values in [[0.9, 0.3], [0.3, 0.9]] {
                let other = run(220.0, values, 9_600);
                assert!(differs(&base, &other) > 0.005, "{name} {values:?}");
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
