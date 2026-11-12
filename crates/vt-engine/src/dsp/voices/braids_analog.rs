//! Analog-style models from Mutable Instruments' Braids: the CS-80-like saw, the morph from
//! triangle to square through a fuzzy filter, the saw blended into a square, two waves in hard
//! sync, and three oscillators as a chord. Ported to Rust from `macro_oscillator.cc`, at twice
//! the program's rate as Braids runs at 96 kHz, on the oscillator in [`super::braids_parts`].
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

use super::braids_parts::{
    AnalogOscillator, INNER, Macro, Model, NO_SYNC, Pitch, Shape, VIOLENT_OVERDRIVE, mix, waveshape,
};
use super::virtual_analog::{Moving, Settings};

/// The CS-80-like saw: each cycle starts on a step, held for `Notch`, at a level `Depth` sets.
pub type CSaw = Macro<CSawModel>;
/// From a triangle through a saw and a square to a narrowing pulse, then a sine under it, through
/// a low-pass that `Fuzz` closes and overdrives.
pub type Morph = Macro<MorphModel>;
/// A saw with a second saw behind it, blended into a square of the same width.
pub type SawSquare = Macro<SawSquareModel>;
/// A square or a saw, and a second one hard synced to it, blended.
pub type HardSync = Macro<SyncModel>;

#[derive(Debug, Clone, Copy)]
pub struct CSawModel {
    oscillator: AnalogOscillator,
}

impl Model for CSawModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self {
            oscillator: AnalogOscillator::new(),
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (notch, depth) = (settings.at(0), settings.at(1));
        self.oscillator.render(
            Shape::CSaw,
            pitch.increment(0.0),
            notch,
            depth,
            &NO_SYNC,
            out,
            None,
        );
        // Braids shifts it back towards the middle as the step drops, and turns it up a little.
        let shift = (1.0 - depth) / 16.0;
        for sample in out.iter_mut() {
            *sample = (*sample + shift) * 1.625;
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MorphModel {
    oscillators: [AnalogOscillator; 2],
    low_pass: f32,
}

impl Model for MorphModel {
    const LEVEL: f32 = 0.36;

    fn new() -> Self {
        Self {
            oscillators: [AnalogOscillator::new(); 2],
            low_pass: 0.0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (shape, fuzz) = (settings.at(0), settings.at(1));
        let (first, width, second, balance) = if shape <= 1.0 / 3.0 {
            (Shape::Triangle, 0.0, Shape::Saw, shape * 3.0)
        } else if shape <= 2.0 / 3.0 {
            (
                Shape::Square,
                0.0,
                Shape::Saw,
                1.0 - (shape - 1.0 / 3.0) * 3.0,
            )
        } else {
            (Shape::Square, (shape - 2.0 / 3.0) * 3.0, Shape::Sine, 0.0)
        };
        let increment = pitch.increment(0.0);
        let mut other = [0.0; INNER];
        let [a, b] = &mut self.oscillators;
        a.render(first, increment, width, 0.0, &NO_SYNC, out, None);
        b.render(second, increment, 0.0, 0.0, &NO_SYNC, &mut other, None);

        // The filter closes from far above the note down to it.
        let cutoff = (pitch.note + 128.0 - fuzz * 128.0).clamp(0.0, 256.0);
        let f = pitch.svf_cutoff(cutoff);
        // Less overdrive on high notes, where it would alias.
        let fuzz = (fuzz - (pitch.note - 80.0).max(0.0) / 32.0).max(0.0);
        for (out, other) in out.iter_mut().zip(other) {
            let sample = mix(*out, other, balance);
            self.low_pass = (self.low_pass + (sample - self.low_pass) * f).clamp(-1.0, 1.0);
            let fuzzed = waveshape(&VIOLENT_OVERDRIVE, self.low_pass);
            *out = mix(sample, fuzzed, fuzz);
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SawSquareModel {
    oscillators: [AnalogOscillator; 2],
    mix: Option<f32>,
}

impl Model for SawSquareModel {
    const LEVEL: f32 = 0.62;

    fn new() -> Self {
        Self {
            oscillators: [AnalogOscillator::new(); 2],
            mix: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (width, balance) = (settings.at(0), settings.at(1));
        let increment = pitch.increment(0.0);
        let mut square = [0.0; INNER];
        let [saw, other] = &mut self.oscillators;
        saw.render(
            Shape::VariableSaw,
            increment,
            width,
            0.0,
            &NO_SYNC,
            out,
            None,
        );
        other.render(
            Shape::Square,
            increment,
            width,
            0.0,
            &NO_SYNC,
            &mut square,
            None,
        );
        let mut balances = Moving::across(self.mix.unwrap_or(balance), balance, INNER);
        for (out, square) in out.iter_mut().zip(square) {
            // Braids turns the square down to about as loud as the saw.
            *out = mix(*out, square * 148.0 / 256.0, balances.next());
        }
        self.mix = Some(balance);
    }
}

/// Which wave both of `Sync`'s oscillators play.
const SYNC_WAVES: [Shape; 2] = [Shape::Square, Shape::Saw];
/// How much each wave is turned down, as a square is about 5 dB louder than a saw.
const SYNC_GAINS: [f32; 2] = [0.58, 1.0];

#[derive(Debug, Clone, Copy)]
pub struct SyncModel {
    oscillators: [AnalogOscillator; 2],
    mix: Option<f32>,
}

impl Model for SyncModel {
    const LEVEL: f32 = 0.7;

    fn new() -> Self {
        Self {
            oscillators: [AnalogOscillator::new(); 2],
            mix: None,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let wave = (settings.at(0) * 255.0).round() as usize;
        let wave = wave.min(SYNC_WAVES.len() - 1);
        let (shape, gain) = (SYNC_WAVES[wave], SYNC_GAINS[wave] * 0.75);
        let (interval, balance) = (settings.at(1), settings.at(2));
        let mut synced = [0.0; INNER];
        let mut sync = NO_SYNC;
        let [master, slave] = &mut self.oscillators;
        master.render(
            shape,
            pitch.increment(0.0),
            0.0,
            0.0,
            &NO_SYNC,
            out,
            Some(&mut sync),
        );
        // The synced one up to 64 semitones over the note.
        let increment = pitch.increment(interval * 64.0);
        slave.render(shape, increment, 0.0, 0.0, &sync, &mut synced, None);
        let mut balances = Moving::across(self.mix.unwrap_or(balance), balance, INNER);
        for (out, synced) in out.iter_mut().zip(synced) {
            *out = mix(*out, synced, balances.next()) * gain;
        }
        self.mix = Some(balance);
    }
}

/// Three oscillators of one wave, the second and third detuned by intervals that catch on
/// chords.
pub type Triple = Macro<TripleModel>;

/// The intervals `Triple`'s second and third oscillators catch on, in Braids' 128ths of a
/// semitone: unisons a little apart, then every semitone over two octaves either way, with the
/// fifths and octaves a little wide.
const SEMI: i32 = 128;
#[rustfmt::skip]
const INTERVALS: [i32; 65] = [
    -24 * SEMI, -24 * SEMI, -24 * SEMI + 4,
    -23 * SEMI, -22 * SEMI, -21 * SEMI, -20 * SEMI, -19 * SEMI, -18 * SEMI,
    -17 * SEMI - 4, -17 * SEMI,
    -16 * SEMI, -15 * SEMI, -14 * SEMI, -13 * SEMI,
    -12 * SEMI - 4, -12 * SEMI,
    -11 * SEMI, -10 * SEMI, -9 * SEMI, -8 * SEMI,
    -7 * SEMI - 4, -7 * SEMI,
    -6 * SEMI, -5 * SEMI, -4 * SEMI, -3 * SEMI, -2 * SEMI, -SEMI,
    -24, -8, -4, 0, 4, 8, 24,
    SEMI, 2 * SEMI, 3 * SEMI, 4 * SEMI, 5 * SEMI, 6 * SEMI,
    7 * SEMI, 7 * SEMI + 4,
    8 * SEMI, 9 * SEMI, 10 * SEMI, 11 * SEMI,
    12 * SEMI, 12 * SEMI + 4,
    13 * SEMI, 14 * SEMI, 15 * SEMI, 16 * SEMI,
    17 * SEMI, 17 * SEMI + 4,
    18 * SEMI, 19 * SEMI, 20 * SEMI, 21 * SEMI, 22 * SEMI, 23 * SEMI,
    24 * SEMI - 4, 24 * SEMI, 24 * SEMI,
];

/// How far a setting detunes an oscillator, in semitones: it holds on each interval for the first
/// half of its stretch, then slides to the next, as Braids works it out in its whole numbers.
fn interval(value: f32) -> f32 {
    let value = (value.clamp(0.0, 1.0) * 32_767.0) as i32;
    let first = INTERVALS[(value >> 9) as usize];
    let second = INTERVALS[(((value >> 8) + 1) >> 1) as usize];
    let fade = (value << 8) & 0xFFFF;
    (first + (((second - first) * fade) >> 16)) as f32 / SEMI as f32
}

/// The waves `Triple` plays, and how much each is turned down to about as loud as the saw.
const TRIPLE_WAVES: [(Shape, f32); 4] = [
    (Shape::Saw, 1.0),
    (Shape::Square, 0.58),
    (Shape::Triangle, 1.0),
    (Shape::Sine, 0.82),
];

#[derive(Debug, Clone, Copy)]
pub struct TripleModel {
    oscillators: [AnalogOscillator; 3],
}

impl Model for TripleModel {
    const LEVEL: f32 = 0.5;

    fn new() -> Self {
        Self {
            oscillators: [AnalogOscillator::new(); 3],
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let wave = ((settings.at(0) * 255.0).round() as usize).min(TRIPLE_WAVES.len() - 1);
        let (shape, gain) = TRIPLE_WAVES[wave];
        let detunes = [0.0, interval(settings.at(1)), interval(settings.at(2))];
        out.fill(0.0);
        let mut one = [0.0; INNER];
        for (oscillator, detune) in self.oscillators.iter_mut().zip(detunes) {
            let increment = pitch.increment(detune);
            oscillator.render(shape, increment, 0.0, 0.0, &NO_SYNC, &mut one, None);
            for (out, one) in out.iter_mut().zip(one) {
                // Braids takes each at about a third.
                *out += one * 21.0 / 64.0 * gain;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: u32 = 48_000;

    fn settings(values: [f32; 3]) -> Settings {
        let mut places = [0.0; vt_model::settings::MAX_MACHINE_SETTINGS];
        places[..3].copy_from_slice(&values);
        Settings::from_places(places)
    }

    fn run<M: Model>(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Macro::<M>::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn differs(a: &[f32], b: &[f32]) -> f32 {
        let total: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
        (total / a.len() as f32).sqrt()
    }

    /// Each model, with its settings as a track starts them.
    fn each(mut check: impl FnMut(&str, &dyn Fn(f32, [f32; 3], usize) -> Vec<f32>)) {
        check("CSaw", &|hertz, values, frames| {
            run::<CSawModel>(hertz, settings(values), frames)
        });
        check("Morph", &|hertz, values, frames| {
            run::<MorphModel>(hertz, settings(values), frames)
        });
        check("SawSq", &|hertz, values, frames| {
            run::<SawSquareModel>(hertz, settings(values), frames)
        });
        check("Sync", &|hertz, values, frames| {
            // Its first setting is a list of two waves.
            let [first, second, third] = values;
            let wave = if first < 0.5 { 0.0 } else { 1.0 / 255.0 };
            run::<SyncModel>(hertz, settings([wave, second, third]), frames)
        });
        check("Triple", &|hertz, values, frames| {
            // Its first setting is a list of four waves.
            let [first, second, third] = values;
            let wave = (first * 3.0).round() / 255.0;
            run::<TripleModel>(hertz, settings([wave, second, third]), frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.5, 0.5, 0.5], [0.25, 0.25, 0.25]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                assert!(decibels.abs() < 3.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            let base = run(220.0, [0.3, 0.3, 0.3], 4_800);
            for values in [[0.9, 0.3, 0.3], [0.3, 0.9, 0.3]] {
                let other = run(220.0, values, 4_800);
                assert!(differs(&base, &other) > 0.02, "{name} {values:?}");
            }
        });
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        each(|name, run| {
            for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
                for first in [0.0, 0.5, 1.0] {
                    for second in [0.0, 0.5, 1.0] {
                        for third in [0.0, 1.0] {
                            let values = [first, second, third];
                            for sample in run(hertz, values, 2_400) {
                                assert!(
                                    sample.is_finite() && sample.abs() <= 1.0,
                                    "{name}: {sample} at {hertz} Hz {values:?}"
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    /// How loud a sound is at one frequency, by Goertzel's algorithm, as an RMS.
    fn at_frequency(samples: &[f32], hertz: f32) -> f32 {
        let w = std::f32::consts::TAU * hertz / RATE as f32;
        let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
        for sample in samples {
            let s = sample + 2.0 * w.cos() * s1 - s2;
            (s2, s1) = (s1, s);
        }
        let power = s1 * s1 + s2 * s2 - 2.0 * w.cos() * s1 * s2;
        power.sqrt() * std::f32::consts::SQRT_2 / samples.len() as f32
    }

    #[test]
    fn a_high_saw_aliases_little() {
        // A saw at 13 kHz has nothing under 24 kHz but its fundamental, so all the rest is
        // aliasing. Plain, it would fold its harmonics all over the spectrum.
        let out = run::<SyncModel>(13_000.0, settings([1.0 / 255.0, 0.0, 0.0]), 9_600);
        let out = &out[2_400..];
        let fundamental = at_frequency(out, 13_000.0);
        let aliased = (rms(out).powi(2) - fundamental.powi(2)).max(0.0).sqrt();
        assert!(
            aliased < fundamental * 0.1,
            "{aliased} against {fundamental}"
        );
    }

    #[test]
    fn the_synced_wave_follows_the_master() {
        // Hard sync keeps the master's period whatever the interval, so the sound repeats at the
        // note.
        let period = 48_000 / 200;
        let out = run::<SyncModel>(200.0, settings([0.0, 0.37, 1.0]), period * 40);
        let late = &out[period * 30..];
        let shifted = &out[period * 31..];
        assert!(differs(&late[..period * 8], &shifted[..period * 8]) < 0.02);
    }

    #[test]
    fn triple_intervals_catch_on_the_notes_of_a_chord() {
        // The defaults, a fifth and an octave, and the middle, in tune with the note.
        assert_eq!(interval(f32::from(0xA8_u8) / 255.0), 7.0);
        assert_eq!(interval(f32::from(0xC0_u8) / 255.0), 12.0);
        assert!(interval(0.5).abs() < 0.05, "{}", interval(0.5));
        assert_eq!(interval(0.0), -24.0);
        assert_eq!(interval(1.0), 24.0);
        // Going up, it never goes down.
        let mut last = -24.0;
        for at in 0..=1000 {
            let next = interval(at as f32 / 1000.0);
            assert!(next >= last, "{at}");
            last = next;
        }
    }
}
