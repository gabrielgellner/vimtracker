//! The chords Mutable Instruments' Plaits shares between its chord and chiptune models, and how
//! it spreads one across five voices for an inversion. Ported to Rust from `chords/chord_bank.h`
//! and `chords/chord_bank.cc`, with Plaits' own table of chords rather than the alternative one.
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

use super::plaits_parts::HysteresisQuantizer;

/// The notes in a chord, and the voices they're spread over: one more, so a note moving up an
/// octave can fade from one voice into the next.
pub(super) const NOTES: usize = 4;
pub(super) const VOICES: usize = NOTES + 1;
const COUNT: usize = 11;

/// Each chord in semitones above its root. The octave and fifth double their notes, a hundredth
/// of a semitone apart so they beat slowly.
const CHORDS: [[f32; NOTES]; COUNT] = [
    [0.00, 0.01, 11.99, 12.00], // Octave
    [0.00, 7.00, 7.01, 12.00],  // 5
    [0.00, 5.00, 7.00, 12.00],  // sus4
    [0.00, 3.00, 7.00, 12.00],  // m
    [0.00, 3.00, 7.00, 10.00],  // m7
    [0.00, 3.00, 10.00, 14.00], // m9
    [0.00, 3.00, 10.00, 17.00], // m11
    [0.00, 2.00, 9.00, 16.00],  // 69
    [0.00, 4.00, 11.00, 14.00], // M9
    [0.00, 4.00, 7.00, 11.00],  // M7
    [0.00, 4.00, 7.00, 12.00],  // M
];

/// The chord chosen, and every chord's notes as frequency ratios.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChordBank {
    quantizer: HysteresisQuantizer,
    ratios: [[f32; NOTES]; COUNT],
}

impl ChordBank {
    pub(super) fn new() -> Self {
        Self {
            quantizer: HysteresisQuantizer::new(COUNT, 0.075, false),
            ratios: CHORDS.map(|chord| chord.map(|note| (note / 12.0).exp2())),
        }
    }

    /// Choose a chord from a setting on `0..=1`. It's stretched a little so the last chord gets a
    /// full share of the range.
    #[inline]
    pub(super) fn set_chord(&mut self, parameter: f32) {
        self.quantizer.process(parameter * 1.02);
    }

    #[inline]
    pub(super) fn chord_index(&self) -> usize {
        self.quantizer.value()
    }

    /// The chord's ratios and levels across the voices, for an inversion on `0..=1`. Each step up
    /// moves the lowest note an octave up, fading it from one voice into the next, so the chord
    /// climbs through its inversions and five octaves, from two octaves under the root.
    pub(super) fn inversion(&self, inversion: f32) -> ([f32; VOICES], [f32; VOICES]) {
        const BASE_GAIN: f32 = 0.25;
        let base = &self.ratios[self.chord_index()];
        let inversion = inversion.clamp(0.0, 1.0) * (NOTES * VOICES) as f32;
        let integral = inversion as usize;
        let fractional = inversion - integral as f32;
        let rotations = integral / NOTES;
        let rotated_note = integral % NOTES;

        let mut ratios = [0.0; VOICES];
        let mut amplitudes = [0.0; VOICES];
        for (i, &ratio) in base.iter().enumerate() {
            let transposition = 0.25 * (1 << ((NOTES - 1 + integral - i) / NOTES)) as f32;
            let target = (i + VOICES - rotations) % VOICES;
            let previous = (target + VOICES - 1) % VOICES;
            if i == rotated_note {
                ratios[target] = ratio * transposition;
                ratios[previous] = ratios[target] * 2.0;
                amplitudes[previous] = BASE_GAIN * fractional;
                amplitudes[target] = BASE_GAIN * (1.0 - fractional);
            } else if i < rotated_note {
                ratios[previous] = ratio * transposition;
                amplitudes[previous] = BASE_GAIN;
            } else {
                ratios[target] = ratio * transposition;
                amplitudes[target] = BASE_GAIN;
            }
        }
        (ratios, amplitudes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmonics_steps_through_every_chord_in_order() {
        let mut bank = ChordBank::new();
        let mut seen = Vec::new();
        for step in 0..=1000 {
            bank.set_chord(step as f32 / 1000.0);
            if seen.last() != Some(&bank.chord_index()) {
                seen.push(bank.chord_index());
            }
        }
        assert_eq!(seen, (0..COUNT).collect::<Vec<_>>());
    }

    #[test]
    fn a_setting_near_an_edge_does_not_flicker() {
        let mut bank = ChordBank::new();
        // The edge between the first two chords.
        let edge = 1.0 / (COUNT as f32 * 1.02);
        bank.set_chord(edge - 0.001);
        let first = bank.chord_index();
        for wobble in [0.004, -0.004, 0.004, -0.004] {
            bank.set_chord(edge + wobble);
            assert_eq!(bank.chord_index(), first);
        }
    }

    #[test]
    fn the_middle_of_its_inversions_is_the_chord_at_its_root() {
        let mut bank = ChordBank::new();
        bank.set_chord(1.0);
        let (ratios, amplitudes) = bank.inversion(0.4);
        let mut sounding: Vec<f32> = (0..VOICES)
            .filter(|&v| amplitudes[v] > 0.0)
            .map(|v| ratios[v])
            .collect();
        sounding.sort_by(f32::total_cmp);
        let expected = [0.0, 4.0, 7.0, 12.0].map(|n: f32| (n / 12.0).exp2());
        for (got, want) in sounding.iter().zip(expected) {
            assert!((got - want).abs() < 1e-4, "{sounding:?}");
        }
        assert!((amplitudes.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }
}
