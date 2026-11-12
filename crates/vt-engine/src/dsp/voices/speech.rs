//! Speech, from Mutable Instruments' Plaits: a formant filter bank, a SAM-like formant synth and
//! an LPC synth playing the vowels and the words of Texas Instruments' speech chips. Ported to
//! Rust from `speech_engine.cc`, `naive_speech_synth.cc`, `sam_speech_synth.cc`,
//! `lpc_speech_synth.cc`, `lpc_speech_synth_controller.cc`, `lpc_speech_synth_phonemes.cc` and
//! `lpc_speech_synth_words.cc`, with its main output. The words are in
//! `tables/plaits_speech_words.bin`, as Plaits has them, LPC-10 frames packed into bits.
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

use super::virtual_analog::{BLOCK, Moving, Settings, next_blep, this_blep};
use crate::dsp::blocks::plaits_parts::{DcBlocker, HysteresisQuantizer, Limiter, Svf, tan_dirty};
use crate::dsp::blocks::sine_table::SINE;
use crate::rng::Pcg32;

/// How loud it is against the other machines, through Plaits' limiter as Plaits has it. Plaits
/// takes it at 0.7; this is a little louder, as its formant models are quiet beside `VA`.
const GAIN: f32 = 1.0;

/// A sine at a phase in whole turns of `u32`, read from the table without interpolating, as
/// Plaits' `SineRaw` is.
#[inline]
fn sine_raw(phase: u32) -> f32 {
    SINE[(phase >> 23) as usize]
}

/// The naive model: a glottal pulse through five band-passes at a phoneme's formants.
const NAIVE_FORMANTS: usize = 5;
/// Each phoneme's formants for each register, as pitches and levels out of 256.
#[rustfmt::skip]
const NAIVE_PHONEMES: [[[(u8, u8); NAIVE_FORMANTS]; 5]; 5] = [
    [
        [(74, 255), (83, 114), (97, 90), (98, 90), (100, 25)],
        [(75, 255), (84, 128), (100, 114), (101, 101), (103, 20)],
        [(76, 255), (85, 128), (100, 18), (102, 16), (104, 3)],
        [(79, 255), (85, 161), (101, 25), (104, 4), (110, 0)],
        [(79, 255), (85, 128), (101, 6), (106, 25), (110, 0)],
    ],
    [
        [(67, 255), (91, 64), (98, 90), (101, 64), (102, 32)],
        [(67, 255), (92, 51), (99, 64), (103, 51), (105, 25)],
        [(69, 255), (93, 51), (100, 32), (102, 25), (103, 25)],
        [(67, 255), (91, 16), (100, 8), (103, 4), (110, 0)],
        [(65, 255), (95, 25), (101, 45), (105, 2), (110, 0)],
    ],
    [
        [(59, 255), (92, 8), (99, 40), (102, 20), (104, 10)],
        [(61, 255), (94, 45), (101, 32), (103, 25), (105, 8)],
        [(60, 255), (93, 16), (101, 16), (104, 4), (105, 4)],
        [(65, 255), (92, 25), (100, 8), (105, 4), (110, 0)],
        [(60, 255), (96, 64), (101, 12), (106, 12), (110, 1)],
    ],
    [
        [(67, 255), (78, 72), (98, 22), (99, 25), (101, 2)],
        [(67, 255), (79, 80), (99, 64), (101, 64), (102, 12)],
        [(68, 255), (79, 80), (100, 12), (102, 20), (103, 5)],
        [(69, 255), (79, 90), (101, 40), (104, 10), (110, 0)],
        [(69, 255), (79, 72), (101, 20), (106, 20), (110, 0)],
    ],
    [
        [(65, 255), (74, 25), (98, 6), (100, 10), (101, 4)],
        [(65, 255), (74, 25), (100, 36), (101, 51), (103, 12)],
        [(66, 255), (75, 25), (100, 18), (102, 8), (104, 5)],
        [(63, 255), (77, 64), (99, 8), (104, 2), (110, 0)],
        [(63, 255), (77, 40), (100, 4), (106, 2), (110, 0)],
    ],
];

/// Plaits' impulse train: a band-limited saw, differentiated and smoothed into a pulse.
#[derive(Debug, Clone, Copy)]
struct ImpulseTrain {
    phase: f32,
    next_sample: f32,
    lp: f32,
    hp: f32,
    frequency: f32,
}

impl ImpulseTrain {
    fn new() -> Self {
        Self {
            phase: 0.5,
            next_sample: 0.0,
            lp: 1.0,
            hp: 0.0,
            frequency: 0.001,
        }
    }

    fn render(&mut self, frequency: f32, out: &mut [f32; BLOCK]) {
        let frequency = frequency.clamp(0.000_001, 0.25);
        let mut frequencies = Moving::towards(self.frequency, frequency);
        self.frequency = frequency;
        let mut next = self.next_sample;
        for out in out.iter_mut() {
            let mut this = next;
            next = 0.0;
            let frequency = frequencies.next();
            self.phase += frequency;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = self.phase / frequency;
                this -= this_blep(t);
                next -= next_blep(t);
            }
            next += self.phase;
            self.lp += 0.25 * ((self.hp - this) - self.lp);
            *out = 4.0 * self.lp;
            self.hp = this;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct NaiveSpeech {
    pulse: ImpulseTrain,
    click: usize,
    filters: [Svf; NAIVE_FORMANTS],
    coloration: Svf,
    rate: f32,
}

impl NaiveSpeech {
    fn new(rate: f32) -> Self {
        let mut coloration = Svf::default();
        coloration.set_g_q(tan_dirty(800.0 / rate), 0.5);
        Self {
            pulse: ImpulseTrain::new(),
            click: 0,
            filters: [Svf::default(); NAIVE_FORMANTS],
            coloration,
            rate,
        }
    }

    fn render(
        &mut self,
        click: bool,
        frequency: f32,
        phoneme: f32,
        register: f32,
        out: &mut [f32; BLOCK],
    ) {
        if click {
            self.click = (self.rate * 0.05) as usize;
        }
        self.click -= self.click.min(BLOCK);
        let frequency = if self.click > 0 {
            frequency * 0.5
        } else {
            frequency
        };
        let mut excitation = [0.0; BLOCK];
        self.pulse.render(frequency, &mut excitation);
        for sample in &mut excitation {
            *sample = self.coloration.band_pass(*sample) * 4.0;
        }
        let p = phoneme.clamp(0.0, 1.0) * (5.0 - 1.001);
        let r = register.clamp(0.0, 1.0) * (5.0 - 1.001);
        let (pi, pf) = (p as usize, p.fract());
        let (ri, rf) = (r as usize, r.fract());
        // A note's pitches here are MIDI's, from Plaits' A at 55 Hz an octave under its lowest.
        let a0 = 440.0 / 8.0 / self.rate;
        out.fill(0.0);
        for (at, filter) in self.filters.iter_mut().enumerate() {
            let corner = |p: usize, r: usize| NAIVE_PHONEMES[p][r][at];
            let blend = |field: fn((u8, u8)) -> u8| {
                let along = |p: usize| {
                    let (a, b) = (
                        f32::from(field(corner(p, ri))),
                        f32::from(field(corner(p, ri + 1))),
                    );
                    a + (b - a) * rf
                };
                along(pi) + (along(pi + 1) - along(pi)) * pf
            };
            let pitch = blend(|formant| formant.0).min(160.0);
            let level = blend(|formant| formant.1) / 256.0;
            let mut f = a0 * ((pitch - 33.0) / 12.0).exp2();
            if self.click > 0 && at == 0 {
                f *= 0.5;
            }
            filter.set_g_q(tan_dirty(f.min(0.497)), 20.0);
            for (out, input) in out.iter_mut().zip(excitation) {
                *out += filter.band_pass(input) * level;
            }
        }
    }
}

/// The SAM-like model: three sines at a phoneme's formants, restarted each cycle of the note.
const SAM_FORMANTS: usize = 3;
const SAM_VOWELS: usize = 9;
const SAM_CONSONANTS: usize = 8;
/// Each phoneme's formants, as pitches in eighths of Hz and levels on the table below, with the
/// last phoneme twice as a guard for reading between them.
#[rustfmt::skip]
const SAM_PHONEMES: [[(u8, u8); SAM_FORMANTS]; SAM_VOWELS + SAM_CONSONANTS + 1] = [
    [(60, 15), (90, 13), (200, 1)],
    [(40, 13), (114, 12), (139, 6)],
    [(33, 14), (155, 12), (209, 7)],
    [(22, 13), (189, 10), (247, 8)],
    [(51, 15), (99, 12), (195, 1)],
    [(29, 13), (65, 8), (180, 0)],
    [(13, 12), (103, 3), (182, 0)],
    [(20, 15), (114, 3), (213, 0)],
    [(13, 7), (164, 3), (222, 14)],
    [(13, 9), (121, 9), (254, 0)],
    [(40, 12), (112, 10), (114, 5)],
    [(24, 13), (54, 8), (157, 0)],
    [(33, 14), (155, 12), (166, 7)],
    [(36, 14), (83, 8), (249, 1)],
    [(40, 14), (114, 12), (139, 6)],
    [(13, 5), (58, 5), (182, 5)],
    [(13, 7), (164, 10), (222, 14)],
    [(13, 7), (164, 10), (222, 14)],
];
#[allow(clippy::excessive_precision)]
const SAM_LEVELS: [f32; 16] = [
    0.03125000, 0.03756299, 0.04515131, 0.05427259, 0.06523652, 0.07841532, 0.09425646, 0.11329776,
    0.13618570, 0.16369736, 0.19676682, 0.23651683, 0.28429697, 0.34172946, 0.41076422, 0.49374509,
];

#[derive(Debug, Clone, Copy)]
struct SamSpeech {
    phase: f32,
    frequency: f32,
    next_sample: f32,
    lp: f32,
    formant_phases: [u32; SAM_FORMANTS],
    consonant: usize,
    consonant_index: f32,
    rate: f32,
}

impl SamSpeech {
    fn new(rate: f32) -> Self {
        Self {
            phase: 0.0,
            frequency: 0.0,
            next_sample: 0.0,
            lp: 0.0,
            formant_phases: [0; SAM_FORMANTS],
            consonant: 0,
            consonant_index: 0.0,
            rate,
        }
    }

    fn render(
        &mut self,
        consonant: bool,
        frequency: f32,
        vowel: f32,
        formant_shift: f32,
        out: &mut [f32; BLOCK],
    ) {
        let frequency = frequency.min(0.0625);
        if consonant {
            self.consonant = (self.rate * 0.05) as usize;
            let r = ((vowel + 3.0 * frequency + 7.0 * formant_shift) * 8.0) as i32;
            self.consonant_index = r.rem_euclid(SAM_CONSONANTS as i32) as f32;
        }
        self.consonant -= self.consonant.min(BLOCK);
        let phoneme = if self.consonant > 0 {
            self.consonant_index + SAM_VOWELS as f32
        } else {
            vowel.clamp(0.0, 1.0) * (SAM_VOWELS as f32 - 1.0001)
        };
        let (whole, fraction) = (phoneme as usize, phoneme.fract());
        let shift = 1.0 + formant_shift * 2.5;
        let mut increments = [0_u32; SAM_FORMANTS];
        let mut levels = [0.0; SAM_FORMANTS];
        for at in 0..SAM_FORMANTS {
            let (a, b) = (SAM_PHONEMES[whole][at], SAM_PHONEMES[whole + 1][at]);
            let f = f32::from(a.0) + (f32::from(b.0) - f32::from(a.0)) * fraction;
            increments[at] = (f * 8.0 * shift * 4_294_967_296.0 / self.rate) as u32;
            let (la, lb) = (SAM_LEVELS[usize::from(a.1)], SAM_LEVELS[usize::from(b.1)]);
            levels[at] = la + (lb - la) * fraction;
        }
        let mut frequencies = Moving::towards(self.frequency, frequency);
        self.frequency = frequency;
        let mut next = self.next_sample;
        for out in out.iter_mut() {
            let mut this = next;
            next = 0.0;
            let frequency = frequencies.next();
            self.phase += frequency;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let t = self.phase / frequency;
                for (phase, increment) in self.formant_phases.iter_mut().zip(increments) {
                    *phase = (t * increment as f32) as u32;
                }
                this -= this_blep(t);
                next -= next_blep(t);
            } else {
                for (phase, increment) in self.formant_phases.iter_mut().zip(increments) {
                    *phase = phase.wrapping_add(increment);
                }
            }
            next += self.phase;
            // Plaits' excitation, which only its second output plays, still moves its filter.
            let d = this - 0.5 - self.lp;
            self.lp += (16.0 * frequency).min(1.0) * d;
            let mut s = 0.0;
            for (phase, level) in self.formant_phases.iter().zip(levels) {
                s += sine_raw(*phase) * level;
            }
            *out = s * (1.0 - self.phase);
        }
        self.next_sample = next;
    }
}

/// One LPC-10 frame: its loudness, its pitch's period at 8 kHz, none for noise, and its ten
/// reflection coefficients.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Frame {
    energy: u8,
    period: u8,
    k0: i16,
    k1: i16,
    k: [i8; 8],
}

impl Frame {
    const fn new(energy: u8, period: u8, k0: i16, k1: i16, k: [i8; 8]) -> Self {
        Self {
            energy,
            period,
            k0,
            k1,
            k,
        }
    }
}

const VOWELS: usize = 5;
const CONSONANTS: usize = 10;
/// Five vowels to sweep through, then ten consonants a note can start with.
#[rustfmt::skip]
const PHONEMES: [Frame; VOWELS + CONSONANTS] = [
    Frame::new(192, 80, -18368, 11584, [52, 29, 23, 14, -17, 79, 37, 4]),
    Frame::new(192, 80, -14528, 1536, [38, 29, 11, 14, -41, 79, 57, 4]),
    Frame::new(192, 80, 14528, 9216, [25, -54, -70, 36, 19, 79, 57, 22]),
    Frame::new(192, 80, -14528, -13440, [38, 57, 57, 14, -53, 7, 37, 77]),
    Frame::new(192, 80, -26368, 4160, [11, 15, -1, 36, -41, 31, 77, 22]),
    Frame::new(15, 0, 5184, 9216, [-29, -12, 0, 0, 0, 0, 0, 0]),
    Frame::new(10, 0, 27968, 17856, [25, 43, -24, -20, -53, 55, -4, -51]),
    Frame::new(128, 160, 14528, -3712, [-43, -26, -24, -20, -53, 55, -4, -51]),
    Frame::new(128, 160, 10048, 11584, [-16, 15, 0, 0, 0, 0, 0, 0]),
    Frame::new(224, 100, 18368, -13440, [-97, -26, -12, -53, -41, 7, 57, 32]),
    Frame::new(192, 80, -10048, 9216, [-70, 15, 34, -20, -17, 31, -24, 22]),
    Frame::new(96, 160, -18368, 17856, [-29, -12, -35, 3, -5, 7, 37, 22]),
    Frame::new(64, 80, -21632, -6272, [-83, 29, 57, 3, -5, 7, 16, 32]),
    Frame::new(192, 80, 0, -1088, [11, -26, -24, -9, -5, 55, 37, 22]),
    Frame::new(64, 80, 21632, -17536, [-97, 85, 57, -20, -17, 31, -4, 59]),
];

/// The words, five banks of LPC-10 frames packed into bits, each word ending with an energy of
/// 15, as Texas Instruments' speech chips read them. Extracted by Plaits from their ROMs.
static WORDS: &[u8; 11_011] = include_bytes!("tables/plaits_speech_words.bin");
/// Where each bank ends in [`WORDS`].
const BANK_ENDS: [usize; 5] = [1233, 2133, 3685, 6209, 11_011];
const MAX_FRAMES: usize = 1024;
const MAX_WORDS: usize = 32;

const ENERGIES: [u8; 16] = [
    0x00, 0x02, 0x03, 0x04, 0x05, 0x07, 0x0a, 0x0f, 0x14, 0x20, 0x29, 0x39, 0x51, 0x72, 0xa1, 0xff,
];
#[rustfmt::skip]
const PERIODS: [u8; 64] = [
    0, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38,
    39, 40, 41, 42, 43, 45, 47, 49, 51, 53, 54, 57, 59, 61, 63, 66, 69, 71, 73, 77, 79, 81, 85, 87,
    92, 95, 99, 102, 106, 110, 115, 119, 123, 128, 133, 138, 143, 149, 154, 160,
];
#[rustfmt::skip]
const K0: [i16; 32] = [
    -32064, -31872, -31808, -31680, -31552, -31424, -31232, -30848, -30592, -30336, -30016, -29696,
    -29376, -28928, -28480, -27968, -26368, -24256, -21632, -18368, -14528, -10048, -5184, 0, 5184,
    10048, 14528, 18368, 21632, 24256, 26368, 27968,
];
#[rustfmt::skip]
const K1: [i16; 32] = [
    -20992, -19328, -17536, -15552, -13440, -11200, -8768, -6272, -3712, -1088, 1536, 4160, 6720,
    9216, 11584, 13824, 15936, 17856, 19648, 21248, 22656, 24000, 25152, 26176, 27072, 27840, 28544,
    29120, 29632, 30080, 30464, 32384,
];
/// The other coefficients' tables, with how many bits each takes.
#[rustfmt::skip]
const K_REST: [(&[i8], u32); 8] = [
    (&[-110, -97, -83, -70, -56, -43, -29, -16, -2, 11, 25, 38, 52, 65, 79, 92], 4),
    (&[-82, -68, -54, -40, -26, -12, 1, 15, 29, 43, 57, 71, 85, 99, 113, 126], 4),
    (&[-82, -70, -59, -47, -35, -24, -12, -1, 11, 23, 34, 46, 57, 69, 81, 92], 4),
    (&[-64, -53, -42, -31, -20, -9, 3, 14, 25, 36, 47, 58, 69, 80, 91, 102], 4),
    (&[-77, -65, -53, -41, -29, -17, -5, 7, 19, 31, 43, 55, 67, 79, 90, 102], 4),
    (&[-64, -40, -16, 7, 31, 55, 79, 102], 3),
    (&[-64, -44, -24, -4, 16, 37, 57, 77], 3),
    (&[-51, -33, -15, 4, 22, 32, 59, 77], 3),
];

/// Reads bits from the start of each byte's lowest, as the speech chips do.
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    available: u32,
    bits: u16,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            at: 0,
            available: 0,
            bits: 0,
        }
    }

    fn get(&mut self, count: u32) -> u8 {
        let mut shift = count;
        if count > self.available {
            self.bits <<= self.available;
            shift -= self.available;
            self.bits |= u16::from(self.data.get(self.at).copied().unwrap_or(0).reverse_bits());
            self.at += 1;
            self.available += 8;
        }
        self.bits <<= shift;
        let result = (self.bits >> 8) as u8;
        self.bits &= 0xFF;
        self.available -= count;
        result
    }

    /// Skip to the next whole byte.
    fn flush(&mut self) {
        while self.available > 0 {
            self.get(1);
        }
    }
}

/// The bank of words being played, its frames unpacked as a word plays them.
#[derive(Debug, Clone, Copy)]
struct WordBank {
    frames: [Frame; MAX_FRAMES],
    frame_count: usize,
    boundaries: [usize; MAX_WORDS + 1],
    word_count: usize,
    loaded: Option<usize>,
}

impl WordBank {
    fn new() -> Self {
        Self {
            frames: [Frame::default(); MAX_FRAMES],
            frame_count: 0,
            boundaries: [0; MAX_WORDS + 1],
            word_count: 0,
            loaded: None,
        }
    }

    /// Unpack a bank, if it isn't the one loaded, saying whether it was.
    fn load(&mut self, bank: usize) -> bool {
        if self.loaded == Some(bank) || bank >= BANK_ENDS.len() {
            return false;
        }
        let start = if bank == 0 { 0 } else { BANK_ENDS[bank - 1] };
        let mut data = &WORDS[start..BANK_ENDS[bank]];
        self.frame_count = 0;
        self.word_count = 0;
        while !data.is_empty() && self.word_count < MAX_WORDS {
            self.boundaries[self.word_count] = self.frame_count;
            let used = self.load_word(data);
            data = &data[used.min(data.len())..];
            self.word_count += 1;
        }
        self.boundaries[self.word_count] = self.frame_count;
        self.loaded = Some(bank);
        true
    }

    fn load_word(&mut self, data: &[u8]) -> usize {
        let mut bits = Bits::new(data);
        let mut frame = Frame::default();
        loop {
            let energy = bits.get(4);
            if energy == 0 {
                frame.energy = 0;
            } else if energy == 0xF {
                bits.flush();
                break;
            } else {
                frame.energy = ENERGIES[usize::from(energy)];
                let repeat = bits.get(1) == 1;
                frame.period = PERIODS[usize::from(bits.get(6))];
                if !repeat {
                    frame.k0 = K0[usize::from(bits.get(5))];
                    frame.k1 = K1[usize::from(bits.get(5))];
                    // Unvoiced frames have only the first four coefficients.
                    let count = if frame.period > 0 { 8 } else { 2 };
                    for (k, (table, width)) in frame.k.iter_mut().zip(K_REST).take(count) {
                        *k = table[usize::from(bits.get(width))];
                    }
                }
            }
            if self.frame_count < MAX_FRAMES {
                self.frames[self.frame_count] = frame;
                self.frame_count += 1;
            }
            if bits.at >= data.len() {
                break;
            }
        }
        bits.at
    }

    /// The first and last frames of the word at a place on `0..1` along the bank.
    fn word(&self, address: f32) -> Option<(usize, usize)> {
        if self.word_count == 0 {
            return None;
        }
        let word = ((address * self.word_count as f32) as usize).min(self.word_count - 1);
        Some((
            self.boundaries[word],
            self.boundaries[word + 1].saturating_sub(1),
        ))
    }
}

/// The LPC synth's pitch when a word has none of its own, in Hz, as every word was recorded to
/// average it.
const LPC_F0: f32 = 100.0;
const ORDER: usize = 10;

/// A lattice filter of ten coefficients over a pulse or noise, at 8 kHz, as the speech chips
/// play LPC-10.
#[derive(Debug, Clone, Copy)]
struct LpcSynth {
    phase: f32,
    frequency: f32,
    noise_energy: f32,
    pulse_energy: f32,
    next_sample: f32,
    pulse_at: usize,
    k: [f32; ORDER],
    s: [f32; ORDER + 1],
}

impl LpcSynth {
    fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 0.0125,
            noise_energy: 0.0,
            pulse_energy: 0.0,
            next_sample: 0.0,
            pulse_at: 0,
            k: [0.0; ORDER],
            s: [0.0; ORDER + 1],
        }
    }

    /// One sample at 8 kHz.
    fn render(&mut self, prosody: f32, pitch_shift: f32, rng: &mut Pcg32) -> f32 {
        let base = LPC_F0 / 8000.0;
        let f = ((base + (self.frequency - base) * prosody) * pitch_shift).clamp(0.0, 0.5);
        self.phase += f;
        let mut this = self.next_sample;
        self.next_sample = 0.0;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            let reset_time = self.phase / f;
            let reset_sample = (32.0 * reset_time) as usize;
            let mut discontinuity = 0.0;
            if self.pulse_at < EXCITATION_PULSE.len() {
                self.pulse_at = self.pulse_at.saturating_sub(reset_sample);
                let s = EXCITATION_PULSE[self.pulse_at];
                discontinuity = f32::from(s) / 128.0 * self.pulse_energy;
            }
            this -= discontinuity * this_blep(reset_time);
            self.next_sample -= discontinuity * next_blep(reset_time);
            self.pulse_at = reset_sample;
        }
        // Plaits takes the sign of its noise from a 16-bit sample, positive or not.
        let noisy = (rng.next_u32() >> 16) as u16 as i16 > 0;
        let mut e = if noisy {
            self.noise_energy
        } else {
            -self.noise_energy
        };
        if self.pulse_at < EXCITATION_PULSE.len() {
            let s = EXCITATION_PULSE[self.pulse_at];
            self.next_sample += f32::from(s) / 128.0 * self.pulse_energy;
            self.pulse_at += 32;
        }
        e += this;
        e *= 1.5;
        let mut stages = [0.0; ORDER + 1];
        stages[ORDER] = e;
        for at in (0..ORDER).rev() {
            stages[at] = stages[at + 1] - self.k[at] * self.s[at];
        }
        stages[0] = stages[0].clamp(-2.0, 2.0);
        for at in (1..ORDER).rev() {
            self.s[at] = self.s[at - 1] + self.k[at - 1] * stages[at - 1];
        }
        self.s[0] = stages[0];
        stages[0]
    }

    fn play(&mut self, frames: &[Frame], frame: f32, interpolate: bool) {
        let whole = frame as usize;
        let blend = if interpolate { frame.fract() } else { 0.0 };
        let (a, b) = (frames[whole], frames[(whole + 1).min(frames.len() - 1)]);
        self.play_pair(a, b, blend);
    }

    fn play_pair(&mut self, a: Frame, b: Frame, blend: f32) {
        let lerp = |x: f32, y: f32| x + (y - x) * blend;
        let frequency = |frame: Frame| {
            if frame.period == 0 {
                self.frequency
            } else {
                1.0 / f32::from(frame.period)
            }
        };
        self.frequency = lerp(frequency(a), frequency(b));
        let energy = |frame: Frame| f32::from(frame.energy) / 256.0;
        let noise = |frame: Frame| {
            if frame.period == 0 {
                energy(frame)
            } else {
                0.0
            }
        };
        let pulse = |frame: Frame| {
            if frame.period != 0 {
                energy(frame)
            } else {
                0.0
            }
        };
        self.noise_energy = lerp(noise(a), noise(b));
        self.pulse_energy = lerp(pulse(a), pulse(b));
        self.k[0] = lerp(f32::from(a.k0), f32::from(b.k0)) / 32_768.0;
        self.k[1] = lerp(f32::from(a.k1), f32::from(b.k1)) / 32_768.0;
        for at in 0..8 {
            self.k[at + 2] = lerp(f32::from(a.k[at]), f32::from(b.k[at])) / 128.0;
        }
    }
}

/// Plays the LPC synth at 8 kHz, through vowels or a bank's words, and brings it up to the rate.
#[derive(Debug, Clone, Copy)]
struct LpcController {
    clock: f32,
    sample: f32,
    next_sample: f32,
    gain: f32,
    synth: LpcSynth,
    playing: Option<usize>,
    last: usize,
    remaining: usize,
    rate: f32,
}

impl LpcController {
    fn new(rate: f32) -> Self {
        Self {
            clock: 0.0,
            sample: 0.0,
            next_sample: 0.0,
            gain: 0.0,
            synth: LpcSynth::new(),
            playing: None,
            last: 0,
            remaining: 0,
            rate,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        bank: Option<&mut WordBank>,
        trigger: bool,
        frequency: f32,
        prosody: f32,
        speed: f32,
        address: f32,
        formant_shift: f32,
        gain: f32,
        rng: &mut Pcg32,
        out: &mut [f32; BLOCK],
    ) {
        let rate_ratio = ((formant_shift - 0.5) * 36.0 / 12.0).exp2();
        // The synth runs at 8 kHz, moved by the formant shift.
        let rate = rate_ratio * 8000.0 / self.rate;
        // Every word was recorded to average 100 Hz.
        let pitch_shift = frequency / (rate_ratio * LPC_F0 / self.rate);
        let stretch = if formant_shift < 0.4 {
            (formant_shift - 0.4) * -45.0
        } else if formant_shift > 0.6 {
            (formant_shift - 0.6) * -45.0
        } else {
            0.0
        };
        let time_stretch = ((-speed * 24.0 + stretch) / 12.0).exp2();
        let words = bank.is_some();
        let (frames, bank_words): (&[Frame], Option<&WordBank>) = match bank {
            Some(bank) => (&bank.frames[..bank.frame_count.max(1)], Some(&*bank)),
            None => (&PHONEMES[..], None),
        };
        let frame_count = if words {
            bank_words.map_or(0, |b| b.frame_count)
        } else {
            VOWELS
        };
        if trigger {
            match bank_words {
                None => {
                    // A consonant, chosen from the settings, for a frame.
                    let r = ((address + 3.0 * formant_shift + 7.0 * frequency) * 8.0) as i32;
                    let consonant = VOWELS + r.rem_euclid(CONSONANTS as i32) as usize;
                    self.playing = Some(consonant);
                    self.last = consonant + 1;
                }
                Some(bank) => match bank.word(address) {
                    Some((start, end)) => {
                        self.playing = Some(start);
                        self.last = end;
                    }
                    None => self.playing = None,
                },
            }
            self.remaining = 0;
        }
        match self.playing {
            None if self.remaining == 0 => {
                let place = address * (frame_count.max(1) as f32 - 1.0001);
                self.synth.play(frames, place.max(0.0), true);
            }
            _ => {
                if self.remaining == 0
                    && let Some(frame) = self.playing
                {
                    self.synth
                        .play(frames, frame.min(frames.len() - 1) as f32, false);
                    // Forty frames a second, as the speech chips play them.
                    self.remaining = (self.rate / 40.0 * time_stretch) as usize;
                    let next = frame + 1;
                    // Vowels go back to sweeping; a word stays on its last frame.
                    self.playing = Some(next);
                    if next >= self.last {
                        self.playing = if words { Some(self.last) } else { None };
                    }
                }
                self.remaining -= self.remaining.min(BLOCK);
            }
        }
        let mut gains = Moving::towards(self.gain, gain);
        self.gain = gain;
        for out in out.iter_mut() {
            let mut this = self.next_sample;
            self.next_sample = 0.0;
            self.clock += rate;
            if self.clock >= 1.0 {
                self.clock -= 1.0;
                let reset_time = self.clock / rate;
                let new_sample = self.synth.render(prosody, pitch_shift, rng);
                let discontinuity = new_sample - self.sample;
                this += discontinuity * this_blep(reset_time);
                self.next_sample += discontinuity * next_blep(reset_time);
                self.sample = new_sample;
            }
            self.next_sample += self.sample;
            *out = this * gains.next();
        }
    }
}

/// The glottal pulse the voiced frames are made of, at 32 times the synth's 8 kHz.
#[rustfmt::skip]
const EXCITATION_PULSE: [i8; 640] = [
    0, 0, 0, 0, 1, 1, 1, 2, 3, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 15, 17, 19, 21, 23, 25, 28, 30, 33,
    36, 38, 41, 44, 48, 51, 54, 57, 61, 64, 68, 71, 75, 78, 82, 85, 89, 92, 95, 99, 102, 105, 108,
    110, 113, 115, 118, 120, 121, 123, 124, 125, 126, 127, 127, 127, 127, 126, 125, 124, 122, 121,
    118, 116, 113, 110, 107, 104, 100, 96, 92, 87, 83, 78, 73, 68, 63, 58, 53, 47, 42, 37, 31, 26,
    21, 16, 11, 6, 1, -3, -8, -12, -16, -20, -23, -26, -29, -32, -35, -37, -39, -40, -42, -43, -43,
    -44, -44, -44, -44, -44, -43, -42, -41, -40, -39, -38, -36, -35, -33, -32, -30, -29, -27, -26,
    -24, -23, -22, -20, -19, -18, -18, -17, -16, -16, -16, -16, -16, -16, -17, -17, -18, -19, -20,
    -21, -23, -24, -26, -27, -29, -30, -32, -34, -35, -37, -39, -40, -42, -43, -44, -46, -47, -48,
    -48, -49, -49, -50, -50, -50, -49, -49, -48, -47, -46, -45, -44, -42, -41, -39, -37, -35, -33,
    -31, -29, -27, -25, -23, -20, -18, -16, -14, -13, -11, -9, -8, -7, -6, -5, -4, -3, -3, -3, -3,
    -3, -4, -5, -6, -7, -9, -10, -12, -14, -16, -18, -21, -23, -26, -29, -32, -34, -37, -40, -43,
    -46, -49, -51, -54, -56, -59, -61, -63, -65, -67, -68, -70, -71, -72, -72, -73, -73, -73, -73,
    -72, -72, -71, -70, -69, -67, -66, -64, -63, -61, -59, -57, -54, -52, -50, -48, -45, -43, -41,
    -39, -37, -35, -33, -31, -29, -27, -26, -25, -23, -22, -21, -20, -20, -19, -19, -19, -18, -18,
    -18, -19, -19, -19, -20, -20, -21, -21, -22, -22, -23, -23, -24, -24, -25, -25, -25, -26, -26,
    -26, -25, -25, -25, -24, -24, -23, -22, -21, -20, -19, -18, -16, -15, -13, -12, -10, -8, -6, -4,
    -3, -1, 1, 3, 5, 7, 8, 10, 12, 13, 15, 16, 17, 18, 19, 20, 21, 21, 22, 22, 22, 22, 22, 22, 22,
    22, 21, 21, 20, 19, 19, 18, 17, 16, 15, 14, 14, 13, 12, 11, 10, 10, 9, 9, 8, 8, 8, 8, 8, 8, 8,
    8, 8, 9, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23, 24, 26, 27, 28, 29, 30, 31, 32,
    33, 33, 34, 35, 35, 35, 36, 36, 36, 36, 35, 35, 35, 34, 34, 33, 33, 32, 31, 30, 30, 29, 28, 27,
    26, 25, 25, 24, 23, 22, 22, 21, 20, 20, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 20, 20, 21, 21,
    22, 22, 23, 23, 24, 25, 25, 26, 27, 27, 28, 29, 29, 29, 30, 30, 30, 31, 31, 31, 31, 30, 30, 30,
    29, 29, 28, 28, 27, 26, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 15, 14, 13, 12, 12, 11,
    10, 10, 10, 9, 9, 9, 9, 9, 9, 9, 9, 10, 10, 10, 11, 11, 12, 12, 12, 13, 13, 14, 14, 14, 15, 15,
    15, 15, 15, 15, 15, 15, 15, 15, 14, 14, 14, 13, 12, 12, 11, 10, 9, 8, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    -1, -2, -3, -4, -4, -5, -6, -6, -7, -7, -8, -8, -8, -9, -9, -9, -9, -9, -9, -9, -9, -8, -8, -8,
    -7, -7, -7, -6, -6, -5, -5, -5, -4, -4, -4, -3, -3, -3, -2, -2, -2, -2, -1, -1, -1, -1, -1, -1,
    -1, -1, -1, -1, -1, -1, -1, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2, -2,
    -2, -2, -2, -2, -2, -2, -1, -1, -1, -1, -1, -1, 0, 0, 0, 0, 0,
];

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Speech {
    naive: NaiveSpeech,
    sam: SamSpeech,
    lpc: LpcController,
    words: WordBank,
    bank: HysteresisQuantizer,
    /// SAM's formants sit a little off centre, which this takes out, as Terrain's and Chip's are.
    dc_blocker: DcBlocker,
    limiter: Limiter,
    rng: Pcg32,
    struck: bool,
    output: [f32; BLOCK],
    at: usize,
}

impl Speech {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32;
        Self {
            naive: NaiveSpeech::new(rate),
            sam: SamSpeech::new(rate),
            lpc: LpcController::new(rate),
            words: WordBank::new(),
            bank: HysteresisQuantizer::new(BANK_ENDS.len() + 1, 0.1, false),
            dc_blocker: DcBlocker::new(1.0 - std::f32::consts::TAU * 10.0 / rate),
            limiter: Limiter::default(),
            rng: Pcg32::new(0, 0),
            struck: false,
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// Seed the noise of its unvoiced sounds, so each note differs but a render repeats.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 5);
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
        sample
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        // Each note is a trigger, as Plaits takes one with its trigger patched.
        let trigger = !self.struck;
        self.struck = true;
        let (model, species, word) = (
            settings.at(0).clamp(0.0, 1.0),
            settings.at(1).clamp(0.0, 1.0),
            settings.at(2).clamp(0.0, 1.0),
        );
        // Prosody and speed are either side of their middles, as Plaits' attenuverters are.
        let prosody = settings.at(3).clamp(0.0, 1.0) * 2.0 - 1.0;
        let speed = settings.at(4).clamp(0.0, 1.0) * 2.0 - 1.0;
        let group = model * 6.0;
        let mut out = [0.0; BLOCK];
        if group <= 2.0 {
            // From the naive model through SAM to the LPC vowels.
            let mut blend = group;
            if group <= 1.0 {
                self.naive
                    .render(trigger, frequency, word, species, &mut out);
            } else {
                self.lpc.render(
                    None,
                    trigger,
                    frequency,
                    0.0,
                    0.0,
                    word,
                    species,
                    1.0,
                    &mut self.rng,
                    &mut out,
                );
                blend = 2.0 - blend;
            }
            let mut sam = [0.0; BLOCK];
            self.sam.render(trigger, frequency, word, species, &mut sam);
            blend = blend * blend * (3.0 - 2.0 * blend);
            blend = blend * blend * (3.0 - 2.0 * blend);
            for (out, sam) in out.iter_mut().zip(sam) {
                *out += (sam - *out) * blend;
            }
        } else {
            // Then the banks of words, one a note.
            let bank = self.bank.process((group - 2.0) * 0.275);
            let words = match bank.checked_sub(1) {
                Some(bank) => {
                    if self.words.load(bank) {
                        self.lpc.playing = None;
                    }
                    Some(&mut self.words)
                }
                None => None,
            };
            self.lpc.render(
                words,
                trigger,
                frequency,
                prosody,
                speed,
                word,
                species,
                1.0,
                &mut self.rng,
                &mut out,
            );
        }
        for (output, sample) in self.output.iter_mut().zip(out) {
            *output = self.limiter.process(GAIN, self.dc_blocker.process(sample));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: u32 = 48_000;

    fn settings(values: [f32; 5]) -> Settings {
        let mut places = [0.0; vt_model::settings::MAX_MACHINE_SETTINGS];
        places[..5].copy_from_slice(&values);
        Settings::from_places(places)
    }

    fn run(hertz: f32, values: [f32; 5], frames: usize) -> Vec<f32> {
        let mut voice = Speech::new(RATE);
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

    /// Each of its models: the naive one, SAM, the LPC vowels, and each bank of words.
    const MODELS: [f32; 8] = [0.0, 1.0 / 6.0, 2.0 / 6.0, 0.45, 0.55, 0.7, 0.85, 1.0];

    #[test]
    fn every_bank_unpacks_into_words() {
        let mut bank = WordBank::new();
        for at in 0..BANK_ENDS.len() {
            assert!(bank.load(at));
            assert!(bank.word_count > 1, "bank {at}: {} words", bank.word_count);
            assert!(bank.frame_count > bank.word_count, "bank {at}");
            assert!(bank.frame_count < MAX_FRAMES, "bank {at}");
            // Each word has frames, in order.
            for word in 0..bank.word_count {
                assert!(
                    bank.boundaries[word] < bank.boundaries[word + 1],
                    "{at} {word}"
                );
            }
            assert!(!bank.load(at), "loading it again does nothing");
        }
    }

    #[test]
    fn each_model_speaks() {
        for model in MODELS {
            let out = run(110.0, [model, 0.5, 0.3, 0.5, 0.5], 24_000);
            assert!(rms(&out) > 0.01, "{model}: {}", rms(&out));
        }
    }

    #[test]
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..24_000)
            .map(|_| va.next(110.0 / RATE as f32, Settings::default()))
            .collect();
        // The formants, SAM and the vowels; a word's loudness is its own.
        for model in [0.0, 1.0 / 6.0, 2.0 / 6.0] {
            let out = run(110.0, [model, 0.5, 0.3, 0.5, 0.5], 24_000);
            let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va[4_800..])).log10();
            assert!(decibels.abs() < 7.0, "{model}: {decibels} dB");
        }
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let base = [0.7, 0.5, 0.3, 0.5, 0.5];
        let plain = run(110.0, base, 24_000);
        for (at, value) in [(0, 0.9), (1, 0.8), (2, 0.8), (3, 1.0), (4, 0.0)] {
            let mut values = base;
            values[at] = value;
            let other = run(110.0, values, 24_000);
            assert!(differs(&plain, &other) > 0.001, "setting {at}");
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 110.0, 880.0, 6_000.0, 20_000.0] {
            for model in MODELS {
                for other in [0.0, 0.5, 1.0] {
                    let values = [model, other, other, other, 1.0 - other];
                    for sample in run(hertz, values, 4_800) {
                        assert!(
                            sample.is_finite() && sample.abs() <= 1.0,
                            "{sample} at {hertz} Hz {values:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_word_plays_once_and_rests_on_its_last_frame() {
        // A long note on a word: it speaks, then settles.
        let out = run(110.0, [1.0, 0.5, 0.0, 0.5, 0.5], 96_000);
        assert!(rms(&out[..24_000]) > 0.01);
    }
}
