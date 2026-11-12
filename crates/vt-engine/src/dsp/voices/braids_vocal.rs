//! Vocal models from Mutable Instruments' Braids: VOSIM, two sines in a bell-shaped window
//! restarted each cycle of the note, and its vowel model, a saw through five formant filters
//! that move from vowel to vowel and from a bass's voice to a soprano's. Ported to Rust from
//! `digital_oscillator.cc`, at twice the program's rate as Braids runs at 96 kHz, through
//! [`super::braids_parts`].
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

use super::braids_parts::{INNER, Macro, Model, Pitch, sine};
use super::virtual_analog::{Settings, next_blep, this_blep};

/// Two sines at fixed pitches, in a bell-shaped window that restarts each cycle of the note.
pub type Vosim = Macro<VosimModel>;
/// A saw through five formant filters, from vowel to vowel and voice to voice.
pub type Fof = Macro<FofModel>;

/// Braids' `lut_bell`: a window that rises over its first sixteenth and falls over the rest.
#[rustfmt::skip]
const BELL: [u16; 257] = [
    0, 670, 2655, 5873, 10191, 15434, 21387, 27805, 34427, 40980, 47198, 52824, 57630, 61417, 64032,
    65366, 65534, 65528, 65517, 65500, 65477, 65449, 65415, 65376, 65331, 65280, 65224, 65162,
    65095, 65022, 64944, 64860, 64770, 64675, 64574, 64468, 64357, 64240, 64118, 63990, 63857,
    63718, 63575, 63426, 63271, 63112, 62947, 62777, 62602, 62421, 62236, 62046, 61850, 61650,
    61444, 61234, 61018, 60798, 60573, 60343, 60109, 59870, 59626, 59377, 59124, 58866, 58604,
    58338, 58067, 57791, 57512, 57228, 56940, 56648, 56351, 56051, 55746, 55438, 55126, 54810,
    54490, 54166, 53839, 53508, 53173, 52835, 52494, 52149, 51801, 51449, 51094, 50736, 50375,
    50011, 49645, 49275, 48902, 48526, 48148, 47767, 47384, 46998, 46610, 46219, 45826, 45431,
    45033, 44633, 44232, 43828, 43423, 43015, 42606, 42195, 41783, 41369, 40953, 40537, 40118,
    39699, 39278, 38856, 38433, 38010, 37585, 37159, 36733, 36306, 35879, 35450, 35022, 34593,
    34163, 33734, 33304, 32874, 32445, 32015, 31585, 31156, 30727, 30298, 29869, 29442, 29014,
    28588, 28162, 27737, 27312, 26889, 26467, 26045, 25625, 25206, 24789, 24373, 23958, 23545,
    23133, 22723, 22315, 21908, 21504, 21101, 20700, 20302, 19905, 19511, 19119, 18730, 18343,
    17958, 17576, 17196, 16819, 16445, 16074, 15706, 15340, 14978, 14618, 14262, 13909, 13559,
    13212, 12869, 12529, 12193, 11860, 11531, 11206, 10884, 10566, 10252, 9941, 9635, 9332, 9034,
    8740, 8450, 8164, 7882, 7604, 7331, 7062, 6798, 6538, 6283, 6032, 5786, 5544, 5307, 5075, 4848,
    4625, 4407, 4195, 3987, 3784, 3586, 3393, 3205, 3022, 2844, 2671, 2504, 2342, 2185, 2033, 1887,
    1746, 1610, 1479, 1354, 1235, 1121, 1012, 909, 811, 719, 632, 550, 475, 405, 340, 281, 228, 180,
    138, 101, 70, 45, 25, 11, 2, 0, 0,
];

/// The window at a phase on `0..1`.
#[inline]
fn bell(phase: f32) -> f32 {
    let index = phase.clamp(0.0, 1.0) * 255.999;
    let whole = index as usize;
    let fraction = index - whole as f32;
    let (a, b) = (f32::from(BELL[whole]), f32::from(BELL[whole + 1]));
    (a + (b - a) * fraction) / 65_536.0
}

#[derive(Debug, Clone, Copy)]
pub struct VosimModel {
    phase: f32,
    formants: [f32; 2],
}

impl Model for VosimModel {
    const LEVEL: f32 = 0.6;

    fn new() -> Self {
        Self {
            phase: 0.0,
            formants: [0.0; 2],
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let increment = pitch.increment(0.0);
        // Each formant's pitch is its own, whatever the note, from MIDI's lowest to its highest.
        let formants =
            [settings.at(0), settings.at(1)].map(|at| pitch.at(at * 128.0).increment(0.0));
        for out in out.iter_mut() {
            self.phase += increment;
            let mut sample = 0.75;
            for ((phase, increment), gain) in
                self.formants.iter_mut().zip(formants).zip([0.5, 0.25])
            {
                *phase = (*phase + increment).fract();
                sample += sine(*phase) * gain;
            }
            sample *= bell(self.phase);
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.formants = [0.0; 2];
                sample = 0.0;
            }
            *out = sample - 0.75;
        }
    }
}

const FORMANTS: usize = 5;

/// Braids' formant pitches in 128ths of a semitone, by voice from bass to soprano, then by vowel,
/// a, e, i, o and u.
const FORMANT_PITCHES: [[[i16; FORMANTS]; FORMANTS]; FORMANTS] = [
    [
        [9519, 10738, 12448, 12636, 12892],
        [8620, 11720, 12591, 12932, 13158],
        [7579, 11891, 12768, 13122, 13323],
        [8620, 10013, 12591, 12768, 13010],
        [8324, 9519, 12591, 12831, 13048],
    ],
    [
        [9696, 10821, 12810, 13010, 13263],
        [8620, 11827, 12768, 13228, 13477],
        [7908, 12038, 12932, 13263, 13452],
        [8620, 10156, 12768, 12932, 13085],
        [8324, 9519, 12852, 13010, 13296],
    ],
    [
        [9730, 10902, 12892, 13085, 13330],
        [8832, 11953, 12852, 13085, 13296],
        [7749, 12014, 13010, 13330, 13483],
        [8781, 10211, 12852, 13085, 13296],
        [8448, 9627, 12892, 13085, 13363],
    ],
    [
        [10156, 10960, 12932, 13427, 14195],
        [8620, 11692, 12852, 13296, 14195],
        [8324, 11827, 12852, 13550, 14195],
        [8881, 10156, 12956, 13427, 14195],
        [8160, 9860, 12708, 13427, 14195],
    ],
    [
        [10156, 10960, 13010, 13667, 14195],
        [8324, 12187, 12932, 13489, 14195],
        [7749, 12337, 13048, 13667, 14195],
        [8881, 10156, 12956, 13609, 14195],
        [8160, 9860, 12852, 13609, 14195],
    ],
];

/// A formant's pitch in semitones, between the table's voices and vowels.
fn formant_pitch(voice: f32, vowel: f32, formant: usize) -> f32 {
    let place = |value: f32| {
        let place = value.clamp(0.0, 1.0) * 4.0;
        let whole = (place as usize).min(FORMANTS - 2);
        (whole, place - whole as f32)
    };
    let ((x, x_mix), (y, y_mix)) = (place(voice), place(vowel));
    let at = |x: usize, y: usize| f32::from(FORMANT_PITCHES[x][y][formant]);
    let a = at(x, y) + (at(x + 1, y) - at(x, y)) * x_mix;
    let c = at(x, y + 1) + (at(x + 1, y + 1) - at(x, y + 1)) * x_mix;
    (a + (c - a) * y_mix) / 128.0
}

#[derive(Debug, Clone, Copy)]
pub struct FofModel {
    phase: f32,
    next_saw: f32,
    previous: f32,
    low: [f32; FORMANTS],
    band: [f32; FORMANTS],
}

impl Model for FofModel {
    const LEVEL: f32 = 1.7;

    fn new() -> Self {
        Self {
            phase: 0.0,
            next_saw: 0.0,
            previous: 0.0,
            low: [0.0; FORMANTS],
            band: [0.0; FORMANTS],
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (vowel, voice) = (settings.at(0), settings.at(1));
        // It runs at half the rate, so each filter is tuned an octave up for its frequency.
        let cutoffs: [f32; FORMANTS] = std::array::from_fn(|formant| {
            pitch.svf_cutoff(formant_pitch(voice, vowel, formant) + 12.0)
        });
        let increment = pitch.increment(0.0) * 2.0;
        let (pairs, _) = out.as_chunks_mut::<2>();
        for pair in pairs {
            let mut this_saw = self.next_saw;
            self.next_saw = 0.0;
            self.phase += increment;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = (self.phase / increment).min(1.0);
                this_saw -= this_blep(t);
                self.next_saw -= next_blep(t);
            }
            self.next_saw += self.phase;
            let mut sample = 0.0;
            for ((low, band), f) in self.low.iter_mut().zip(&mut self.band).zip(cutoffs) {
                let notch = this_saw - *band / 64.0;
                *low = (*low + f * *band).clamp(-1.0, 1.0);
                let high = notch - *low;
                *band = (*band + f * high).clamp(-1.0, 1.0);
                // Braids takes every formant at the first's level.
                sample += *band / 8.0;
            }
            let sample = sample.clamp(-1.0, 1.0);
            *pair = [0.5 * (sample + self.previous), sample];
            self.previous = sample;
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
        check("VOSIM", &|hertz, values, frames| {
            run::<VosimModel>(hertz, values, frames)
        });
        check("FOF", &|hertz, values, frames| {
            run::<FofModel>(hertz, values, frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.5, 0.5], [0.75, 0.25]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                assert!(decibels.abs() < 4.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            let base = run(220.0, [0.3, 0.3], 4_800);
            for values in [[0.9, 0.3], [0.3, 0.9]] {
                let other = run(220.0, values, 4_800);
                assert!(differs(&base, &other) > 0.01, "{name} {values:?}");
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

    #[test]
    fn the_formants_sit_between_the_tables_entries() {
        // At the corners, a formant is the table's own; between, it's between them.
        let bass_a = formant_pitch(0.0, 0.0, 0);
        assert_eq!(bass_a, 9519.0 / 128.0);
        let soprano_u = formant_pitch(1.0, 1.0, 4);
        assert_eq!(soprano_u, 14_195.0 / 128.0);
        let between = formant_pitch(0.125, 0.0, 0);
        let (bass, tenor) = (9519.0 / 128.0, 9696.0 / 128.0);
        assert!(between > bass && between < tenor, "{between}");
    }
}
