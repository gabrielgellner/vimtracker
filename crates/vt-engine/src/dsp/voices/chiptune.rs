//! Four-note chords of square waves, from the chiptune model of Mutable Instruments' Plaits, with
//! its main output. Ported to Rust from `chiptune_engine.cc` and `super_square_oscillator.h`, as
//! Plaits plays it with nothing patched into its trigger: the whole chord held, with no
//! arpeggiator, and without the triangle bass of its second output.
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

use super::chord_bank::{ChordBank, VOICES};
use super::virtual_analog::{BLOCK, Moving, Settings, SyncedSquare};
use crate::dsp::blocks::plaits_parts::DcBlocker;

/// How loud it is against the other machines. A major chord of plain squares at its root comes
/// out of the engine 6 dB louder than the virtual analog voice's defaults, over a second at
/// 220 Hz, and Plaits turns it down to match: its output gain is 0.5, against the virtual analog
/// model's 0.8. A half brings it to within a decibel. Thin pulses, at the bottom of Morph, are
/// quieter, as they are in Plaits.
const LEVEL: f32 = 0.5;
/// The DC blocker's pole: a corner of about 3 Hz at 48 kHz, below anything played.
const DC_POLE: f32 = 1.0 - 20.0 / 48_000.0;

/// One voice of the model. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Chiptune {
    chords: ChordBank,
    /// Plaits' super square is the virtual analog voice's synced square at an even width.
    squares: [SyncedSquare; VOICES],
    amplitudes: [f32; VOICES],
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
    /// Thin pulses leave an offset, which Plaits' output coupling takes away in hardware. Here it
    /// would thump as the track's envelope opens and closes.
    dc_blocker: DcBlocker,
}

impl Default for Chiptune {
    fn default() -> Self {
        Self::new()
    }
}

impl Chiptune {
    pub fn new() -> Self {
        Self {
            chords: ChordBank::new(),
            squares: [SyncedSquare::new(); VOICES],
            amplitudes: [0.0; VOICES],
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
            self.render(frequency.clamp(0.0, 0.5), settings);
            self.at = 0;
        }
        let sample = self.dc_blocker.process(self.output[self.at]);
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let shape = settings.morph.clamp(0.0, 1.0) * 0.995;
        // The square runs at a ratio of the note and is reset by it: below the middle slower,
        // which narrows it to a thin pulse; at the middle a plain square; above, faster, for hard
        // sync.
        let slave_ratio = if shape < 0.5 {
            0.51 + 0.98 * shape
        } else {
            1.0 + 16.0 * (shape - 0.5) * (shape - 0.5)
        };
        // Harmonics is a place in the list of chords.
        self.chords.set_listed(settings.harmonics.clamp(0.0, 1.0));
        let (ratios, mut amplitudes) = self.chords.inversion(settings.timbre);
        // Plaits turns every other voice upside down.
        for amplitude in amplitudes.iter_mut().skip(1).step_by(2) {
            *amplitude = -*amplitude;
        }
        if self.fresh {
            self.amplitudes = amplitudes;
            self.fresh = false;
        }

        self.output = [0.0; BLOCK];
        let mut square = [0.0; BLOCK];
        for voice in 0..VOICES {
            let voice_f0 = f0 * ratios[voice];
            self.squares[voice].render(voice_f0, voice_f0 * slave_ratio, 0.5, &mut square);
            // Plaits sets each level once a block. Moving it across the block instead keeps an
            // inversion fading between voices from stepping.
            let mut level = Moving::towards(self.amplitudes[voice], amplitudes[voice]);
            for (out, sample) in self.output.iter_mut().zip(square) {
                *out += sample * level.next();
            }
        }
        self.amplitudes = amplitudes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Chiptune::new();
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

    /// A major chord at its root, on plain squares.
    fn plain() -> Settings {
        at(1.0, 0.4, 0.5)
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// How strongly a frequency sounds, by correlating with a sine and cosine at it.
    fn strength(samples: &[f32], hertz: f32) -> f32 {
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        for (n, &s) in samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * f64::from(hertz) * n as f64 / f64::from(RATE);
            re += f64::from(s) * phase.cos();
            im += f64::from(s) * phase.sin();
        }
        ((re * re + im * im).sqrt() / samples.len() as f64) as f32
    }

    #[test]
    fn it_has_no_offset_once_it_settles() {
        for (timbre, morph) in [(0.5, 0.0), (0.7, 0.0), (0.4, 0.25)] {
            let settings = Settings {
                harmonics: 1.0,
                timbre,
                morph,
                ..Settings::default()
            };
            let mut voice = Chiptune::new();
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
        assert!(energy(&out[24_000..]) > 1e-3, "{}", energy(&out[24_000..]));
    }

    #[test]
    fn a_major_chord_sounds_its_third_and_fifth_as_well_as_its_root() {
        let out = run(110.0, plain(), 48_000);
        let out = &out[4_800..];
        let semitone = |n: f32| 110.0 * (n / 12.0).exp2();
        let root = strength(out, semitone(0.0));
        let third = strength(out, semitone(4.0));
        let fifth = strength(out, semitone(7.0));
        let between = strength(out, semitone(3.0)).max(strength(out, semitone(5.0)));
        for note in [root, third, fifth] {
            assert!(note > between * 10.0, "{root} {third} {fifth} {between}");
        }
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let plain = run(220.0, plain(), 9_600);
        for other in [
            at(3.0 / 255.0, 0.4, 0.5),
            at(1.0, 0.7, 0.5),
            at(1.0, 0.4, 0.2),
        ] {
            let changed = run(220.0, other, 9_600);
            let difference: Vec<f32> = plain.iter().zip(&changed).map(|(a, b)| a - b).collect();
            assert!(
                energy(&difference) > energy(&plain) * 0.1,
                "{other:?} {}",
                energy(&difference)
            );
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.5, 1.0] {
                for timbre in [0.0, 0.3, 0.5, 0.8, 1.0] {
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
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
