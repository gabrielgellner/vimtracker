//! A vowel filter: a voice's five formants, band-passes fixed in pitch as a vowel's are, which the
//! track's sound goes through side by side, as Vital's formant filter does with its four. `Vowel`
//! morphs from one vowel to the next, so an LFO sweeping it makes the sound say "wah-yow", and
//! `Size` moves between voices, from a child's to a deep one, as a formant shift.
//!
//! The vowels and voices are Plaits' naive speech synth's, which plays them with a pulse: five
//! vowels, `A`, `E`, `I`, `O` and `U`, each in five registers, each with five formants' pitches and
//! levels, blended between in both directions. Its formants share one Q, 20; here `Res` sets it.
//! Adapted from `plaits/dsp/speech/naive_speech_synth.cc`, whose table this is.
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

use std::f32::consts::PI;

use vt_model::insert::{Vowel as Settings, vowel_blend};

use crate::dsp::blocks::flush::flush;
use crate::dsp::blocks::sides::Sides;
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::effects::resonator::{Limiter, semitones};
use crate::dsp::filters::filter::Svf;

/// How many formants a voice has, and how many vowels and voices there are, as Plaits has them.
const FORMANTS: usize = 5;
const VOWELS: usize = 5;
const VOICES: usize = 5;
/// How many frames it plays between workings-out of its formants, about two thirds of a
/// millisecond at 48 kHz.
const BLOCK: usize = 32;
/// The formants' Q at either end of `Res`, evenly in ratio between: broad enough to hear the sound
/// through at `00`, and past Plaits' 20, ringing, at `FF`.
const LEAST_Q: f32 = 2.0;
const MOST_Q: f32 = 30.0;
/// The lowest and highest a formant goes, the highest as a fraction of the rate.
const LOWEST_HZ: f32 = 20.0;
const HIGHEST: f32 = 0.45;
/// How much pink noise there is across what's heard, 20 Hz to 20 kHz, in natural-log units of
/// frequency, which is what the formants' power is matched against.
const HEARD: f32 = 6.907_755;
/// How loud what comes out is, before the limiter, so pink noise comes out about as loud as it
/// went in. The limiter itself turns everything down to 0.8.
const LEVEL: f32 = 1.33;

/// Plaits' table: for each vowel, from `A` to `U`, and each voice, from the deepest to the
/// smallest, each formant's pitch as a MIDI note and its level out of 256.
const TABLE: [[[[u8; 2]; FORMANTS]; VOICES]; VOWELS] = [
    [
        [[74, 255], [83, 114], [97, 90], [98, 90], [100, 25]],
        [[75, 255], [84, 128], [100, 114], [101, 101], [103, 20]],
        [[76, 255], [85, 128], [100, 18], [102, 16], [104, 3]],
        [[79, 255], [85, 161], [101, 25], [104, 4], [110, 0]],
        [[79, 255], [85, 128], [101, 6], [106, 25], [110, 0]],
    ],
    [
        [[67, 255], [91, 64], [98, 90], [101, 64], [102, 32]],
        [[67, 255], [92, 51], [99, 64], [103, 51], [105, 25]],
        [[69, 255], [93, 51], [100, 32], [102, 25], [103, 25]],
        [[67, 255], [91, 16], [100, 8], [103, 4], [110, 0]],
        [[65, 255], [95, 25], [101, 45], [105, 2], [110, 0]],
    ],
    [
        [[59, 255], [92, 8], [99, 40], [102, 20], [104, 10]],
        [[61, 255], [94, 45], [101, 32], [103, 25], [105, 8]],
        [[60, 255], [93, 16], [101, 16], [104, 4], [105, 4]],
        [[65, 255], [92, 25], [100, 8], [105, 4], [110, 0]],
        [[60, 255], [96, 64], [101, 12], [106, 12], [110, 1]],
    ],
    [
        [[67, 255], [78, 72], [98, 22], [99, 25], [101, 2]],
        [[67, 255], [79, 80], [99, 64], [101, 64], [102, 12]],
        [[68, 255], [79, 80], [100, 12], [102, 20], [103, 5]],
        [[69, 255], [79, 90], [101, 40], [104, 10], [110, 0]],
        [[69, 255], [79, 72], [101, 20], [106, 20], [110, 0]],
    ],
    [
        [[65, 255], [74, 25], [98, 6], [100, 10], [101, 4]],
        [[65, 255], [74, 25], [100, 36], [101, 51], [103, 12]],
        [[66, 255], [75, 25], [100, 18], [102, 8], [104, 5]],
        [[63, 255], [77, 64], [99, 8], [104, 2], [110, 0]],
        [[63, 255], [77, 40], [100, 4], [106, 2], [110, 0]],
    ],
];

/// A formant's pitch, as a MIDI note, and its level.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Formant {
    note: f32,
    level: f32,
}

/// The formants for `Vowel` and `Size` on the `00` to `FF` scale, blended between the four
/// nearest in Plaits' table, as Plaits blends them: their pitches in semitones and their levels.
fn formants(vowel: f32, size: f32) -> [Formant; FORMANTS] {
    let (from, to, by) = vowel_blend(vowel);
    // `Size` goes from the smallest voice at `00` to the deepest at `FF`, which is the table's
    // first.
    let (smaller, bigger, towards) = vowel_blend(size);
    let [smaller, bigger] = [smaller, bigger].map(|place| VOICES - 1 - place);
    std::array::from_fn(|formant| {
        let at = |vowel: usize, voice: usize| TABLE[vowel][voice][formant].map(f32::from);
        let blend = |vowel: usize, part: usize| {
            let (one, other) = (at(vowel, smaller)[part], at(vowel, bigger)[part]);
            one + (other - one) * towards
        };
        let between = |part: usize| blend(from, part) + (blend(to, part) - blend(from, part)) * by;
        Formant {
            note: between(0),
            level: between(1) / 256.0,
        }
    })
}

/// The formants' Q for `Res` on the `00` to `FF` scale.
fn formant_q(resonance: f32) -> f32 {
    LEAST_Q * (MOST_Q / LEAST_Q).powf((resonance / 255.0).clamp(0.0, 1.0))
}

/// How much of pink noise a band-pass of Q `q`, passing its centre at unity, lets through, in the
/// same natural-log units of frequency as `HEARD`: the integral of its power over the log of
/// frequency, which is `pi / 2q` as it narrows.
fn band_power(q: f32) -> f32 {
    let b = 2.0 - 1.0 / (q * q);
    let root = (4.0 - b * b).sqrt();
    (PI / 2.0 + (b / root).atan()) / (root * q * q)
}

/// Each formant's tuning, damping and how much of it is heard, which move together.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Bank {
    /// `tan(pi f)` for each formant's frequency `f` as a fraction of the rate.
    g: [f32; FORMANTS],
    /// One over each one's Q.
    k: [f32; FORMANTS],
    /// How much of each one's band-pass goes out, its level and the bank's together, with `k` so
    /// its centre passes at its level.
    gain: [f32; FORMANTS],
}

impl Bank {
    /// How far each part moves a frame to get to `to` within a block.
    fn steps_to(&self, to: &Self) -> Self {
        let step = |from: [f32; FORMANTS], to: [f32; FORMANTS]| {
            std::array::from_fn(|at| (to[at] - from[at]) / BLOCK as f32)
        };
        Self {
            g: step(self.g, to.g),
            k: step(self.k, to.k),
            gain: step(self.gain, to.gain),
        }
    }

    fn add(&mut self, step: &Self) {
        for at in 0..FORMANTS {
            self.g[at] += step.g[at];
            self.k[at] += step.k[at];
            self.gain[at] += step.gain[at];
        }
    }
}

/// What a `Vowel` slot keeps: its formants on each side and where they're going. It's small
/// enough to keep in every slot.
#[derive(Debug, Clone, Copy)]
pub struct Formants {
    /// Each side's formants, worked out once for both while a sound is the same on both.
    filters: Sides<[Svf; FORMANTS]>,
    /// The formants now, how much they move each frame, and where they're going by the end of the
    /// block.
    now: Bank,
    step: Bank,
    target: Bank,
    /// `1 / (1 + g (g + k))` for each formant, worked out each frame while they move.
    a1: [f32; FORMANTS],
    moving: bool,
    /// `Vowel`, `Size` and `Res` on the `00` to `FF` scale, and `Track` from 0 to 1.
    vowel: Smoothed,
    size: Smoothed,
    resonance: Smoothed,
    track: Smoothed,
    /// The track's note, gliding.
    note: Smoothed,
    /// What the formants were last worked out from, so nothing's worked out while nothing moves.
    worked_from: [f32; 5],
    limiter: Limiter,
    /// How many frames of this block are left before the formants are worked out again.
    left: usize,
    /// Whether it's only just started, so it starts at what's set rather than gliding to it.
    fresh: bool,
    rate: u32,
}

impl Default for Formants {
    fn default() -> Self {
        let mut formants = Self {
            filters: Sides::default(),
            now: Bank::default(),
            step: Bank::default(),
            target: Bank::default(),
            a1: [1.0; FORMANTS],
            moving: false,
            vowel: Smoothed::default(),
            size: Smoothed::default(),
            resonance: Smoothed::default(),
            track: Smoothed::default(),
            note: Smoothed::at(60.0),
            worked_from: [f32::NAN; 5],
            limiter: Limiter::new(48_000),
            left: 0,
            fresh: true,
            rate: 0,
        };
        formants.follow(Settings::DEFAULT);
        formants
    }
}

impl Formants {
    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.vowel.target = f32::from(settings.vowel);
        self.size.target = f32::from(settings.size);
        self.resonance.target = f32::from(settings.resonance);
        self.track.target = f32::from(settings.track) / 255.0;
    }

    /// Jump to what's set and the note, rather than gliding, as it's only just started.
    pub fn settle(&mut self) {
        self.fresh = true;
        self.left = 0;
    }

    /// One frame of the slot's sound through the formants, with `note` the track's for `Track`,
    /// and the frame out, all wet and within full scale. `same` says its two sides are the same,
    /// bit for bit.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        same: bool,
        note: f32,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        if rate != self.rate {
            self.rate = rate;
            self.limiter = Limiter::new(rate);
            self.worked_from = [f32::NAN; 5];
            self.settle();
        }
        if self.left == 0 {
            self.work_out(note, smoothing);
            self.left = BLOCK;
        }
        self.left -= 1;
        if self.moving {
            self.work_out_a1();
        }
        let (now, a1) = (&self.now, &self.a1);
        let out = self.filters.process(input, same, |filters, sample| {
            // With nothing coming in, each formant's state goes once it's tiny (#260).
            if sample == 0.0 {
                filters.iter_mut().for_each(flush);
            }
            let mut out = 0.0;
            for (at, filter) in filters.iter_mut().enumerate() {
                let (band, _) = filter.tick_tuned(sample, now.g[at], a1[at]);
                out += band * now.gain[at];
            }
            out
        });
        if self.moving {
            self.now.add(&self.step);
        }
        self.limiter.process(out)
    }

    /// Move the settings and the note on a block, and set the formants to move to what they now
    /// make over the block.
    fn work_out(&mut self, note: f32, smoothing: f32) {
        let smoothing = 1.0 - (1.0 - smoothing).powi(BLOCK as i32);
        self.note.target = note;
        let smoothed = [
            &mut self.vowel,
            &mut self.size,
            &mut self.resonance,
            &mut self.track,
            &mut self.note,
        ];
        let values = smoothed.map(|value| {
            if self.fresh {
                value.settle();
            }
            value.next(smoothing)
        });
        let previous = self.target;
        // Nothing's worked out again for less than can be heard.
        let moved = values
            .iter()
            .zip(&self.worked_from)
            .any(|(now, then)| (now - then).abs() >= 1e-4 || then.is_nan());
        if moved {
            self.worked_from = values;
            self.target = self.worked_out(values);
        }
        if self.fresh {
            self.now = self.target;
            self.step = Bank::default();
            self.fresh = false;
        } else {
            // Exactly where the last block was going, however the frames added up.
            self.now = previous;
            self.step = previous.steps_to(&self.target);
        }
        self.moving = self.step != Bank::default();
        // Standing still, it's worked out once for the block.
        self.work_out_a1();
    }

    /// Each formant's `1 / (1 + g (g + k))`, for where it is now.
    #[inline]
    fn work_out_a1(&mut self) {
        for at in 0..FORMANTS {
            let (g, k) = (self.now.g[at], self.now.k[at]);
            self.a1[at] = 1.0 / (1.0 + g * (g + k));
        }
    }

    /// The formants for some settings and a note.
    fn worked_out(&self, [vowel, size, resonance, track, note]: [f32; 5]) -> Bank {
        let rate = self.rate as f32;
        let shift = track * (note - 60.0);
        let q = formant_q(resonance);
        let k = 1.0 / q;
        let formants = formants(vowel, size);
        // Matched to pink noise: what the formants together pass of it, against all there is.
        let passed: f32 = formants.iter().map(|f| f.level * f.level).sum::<f32>() * band_power(q);
        let level = LEVEL * (HEARD / passed).sqrt();
        let mut bank = Bank::default();
        for (at, formant) in formants.iter().enumerate() {
            let hertz =
                (440.0 * semitones(formant.note + shift - 69.0)).clamp(LOWEST_HZ, HIGHEST * rate);
            bank.g[at] = (PI * hertz / rate).tan();
            bank.k[at] = k;
            bank.gain[at] = formant.level * k * level;
        }
        bank
    }

    /// How far its limiter is turning down now, as a gain, for its tests.
    #[cfg(test)]
    fn limiting(&self) -> f32 {
        self.limiter.gain()
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use vt_model::constants::UNITY_LEVEL;

    use super::*;
    use crate::rng::Pcg32;

    const RATE: u32 = 48_000;
    /// Settings take effect at once, as the smoother has nothing to do.
    const NO_SMOOTHING: f32 = 1.0;

    /// Where each vowel sits on `Vowel`, and each of the table's voices on `Size`, from the
    /// deepest.
    const VOWEL_AT: [u8; VOWELS] = [0x00, 0x40, 0x80, 0xC0, 0xFF];
    const SIZE_AT: [u8; VOICES] = [0xFF, 0xC0, 0x80, 0x40, 0x00];

    fn at(vowel: u8, size: u8, resonance: u8) -> Settings {
        Settings {
            vowel,
            size,
            resonance,
            ..Settings::DEFAULT
        }
    }

    fn played(settings: Settings, note: f32, input: &[f32], rate: u32) -> Vec<[f32; 2]> {
        let mut formants = Formants::default();
        formants.follow(settings);
        formants.settle();
        input
            .iter()
            .map(|&sample| formants.process([sample; 2], true, note, NO_SMOOTHING, rate))
            .collect()
    }

    /// Pink noise, by Paul Kellet's filter on white noise: as much in every octave.
    fn pink(frames: usize, level: f32) -> Vec<f32> {
        let mut rng = Pcg32::new(7, 1);
        let mut b = [0.0_f32; 3];
        (0..frames)
            .map(|_| {
                let white = rng.next_u32() as f32 / 2_147_483_648.0 - 1.0;
                b[0] = 0.99765 * b[0] + white * 0.099_046;
                b[1] = 0.963 * b[1] + white * 0.296_516_4;
                b[2] = 0.57 * b[2] + white * 1.052_691_3;
                (b[0] + b[1] + b[2] + white * 0.1848) * level * 0.2
            })
            .collect()
    }

    /// A drum: a burst of white noise falling away over about 20 ms, every quarter of a second.
    fn drums(frames: usize, level: f32, rate: u32) -> Vec<f32> {
        let mut rng = Pcg32::new(7, 1);
        (0..frames)
            .map(|frame| {
                let since = (frame % (rate as usize / 4)) as f32 / rate as f32;
                let noise = rng.next_u32() as f32 / 2_147_483_648.0 - 1.0;
                noise * level * (-since / 0.02).exp()
            })
            .collect()
    }

    /// A saw at a note, as a synth plays it, band-limited by adding its harmonics.
    fn saw(frames: usize, note: f32, level: f32, rate: u32) -> Vec<f32> {
        let hertz = 440.0 * semitones(note - 69.0);
        let harmonics = (rate as f32 * 0.45 / hertz) as usize;
        (0..frames)
            .map(|frame| {
                let phase = frame as f32 / rate as f32 * hertz * TAU;
                (1..=harmonics)
                    .map(|harmonic| (phase * harmonic as f32).sin() / harmonic as f32)
                    .sum::<f32>()
                    * level
                    * 0.55
            })
            .collect()
    }

    fn rms(samples: impl Iterator<Item = f32>) -> f32 {
        let (sum, count) = samples.fold((0.0, 0), |(sum, count), s| (sum + s * s, count + 1));
        (sum / count as f32).sqrt()
    }

    fn db(wet: f32, dry: f32) -> f32 {
        20.0 * (wet / dry).log10()
    }

    fn hertz_of(note: f32) -> f32 {
        440.0 * semitones(note - 69.0)
    }

    /// How loud it leaves each of some frequencies, from what a quiet click does: the click's
    /// response at each, by its Fourier sum.
    fn response(settings: Settings, hertz: &[f32]) -> Vec<f32> {
        let mut click = vec![0.0; RATE as usize / 4];
        click[0] = 1e-3;
        let out: Vec<f32> = played(settings, 60.0, &click, RATE)
            .iter()
            .map(|frame| frame[0] / 1e-3)
            .collect();
        hertz
            .iter()
            .map(|&hertz| {
                let turn = f64::from(hertz) / f64::from(RATE) * std::f64::consts::TAU;
                let (step_sin, step_cos) = turn.sin_cos();
                let (mut sin, mut cos) = (0.0_f64, 1.0_f64);
                let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
                for sample in &out {
                    real += f64::from(*sample) * cos;
                    imaginary += f64::from(*sample) * sin;
                    (sin, cos) = (
                        sin * step_cos + cos * step_sin,
                        cos * step_cos - sin * step_sin,
                    );
                }
                (real * real + imaginary * imaginary).sqrt() as f32
            })
            .collect()
    }

    /// What the table says one formant leaves a frequency at, as a complex gain: a band-pass
    /// passing its centre at its level, as an analog one through the bilinear transform.
    fn formant_says(formant: Formant, q: f32, hertz: f32) -> [f32; 2] {
        let warp = |hertz: f32| (PI * hertz / RATE as f32).tan();
        // `k w j / (1 - w^2 + k w j)`, with `w` the frequency over the formant's.
        let w = warp(hertz) / warp(hertz_of(formant.note));
        let (k, real, imaginary) = (1.0 / q, 1.0 - w * w, w / q);
        let below = real * real + imaginary * imaginary;
        [
            formant.level * k * w * imaginary / below,
            formant.level * k * w * real / below,
        ]
    }

    /// The same for them all, added together.
    fn table_says(formants: &[Formant; FORMANTS], q: f32, hertz: f32) -> [f32; 2] {
        formants.iter().fold([0.0; 2], |sum, &formant| {
            let one = formant_says(formant, q, hertz);
            [sum[0] + one[0], sum[1] + one[1]]
        })
    }

    fn size_of(complex: [f32; 2]) -> f32 {
        complex[0].hypot(complex[1])
    }

    #[test]
    fn each_vowels_formants_peak_where_plaits_table_puts_them() {
        let resonance = 0xFF;
        let q = formant_q(f32::from(resonance));
        let mut checked = 0;
        for vowel in 0..VOWELS {
            for voice in 0..VOICES {
                let settings = at(VOWEL_AT[vowel], SIZE_AT[voice], resonance);
                let formants = formants(f32::from(settings.vowel), f32::from(settings.size));
                // It's the table's own row, at each vowel and voice.
                for (formant, &[note, level]) in formants.iter().zip(&TABLE[vowel][voice]) {
                    assert_eq!(formant.note, f32::from(note));
                    assert_eq!(formant.level, f32::from(level) / 256.0);
                }
                // Each formant that stands out from the others' skirts peaks where it says, found
                // every 5 cents within a semitone of it, then by a parabola through the three
                // around the strongest.
                for (at, formant) in formants.iter().enumerate() {
                    let wanted = hertz_of(formant.note);
                    // However their phases fall, the most the others could add here.
                    let others: f32 = formants
                        .iter()
                        .enumerate()
                        .filter(|(other, _)| *other != at)
                        .map(|(_, &other)| size_of(formant_says(other, q, wanted)))
                        .sum();
                    if formant.level < 6.0 * others || wanted > RATE as f32 * 0.4 {
                        continue;
                    }
                    let cents: Vec<f32> = (-20..=20).map(|step| step as f32 * 5.0).collect();
                    let hertz: Vec<f32> = cents
                        .iter()
                        .map(|cents| wanted * (cents / 1200.0).exp2())
                        .collect();
                    let heard = response(settings, &hertz);
                    let best = (1..heard.len() - 1)
                        .max_by(|a, b| heard[*a].total_cmp(&heard[*b]))
                        .unwrap();
                    let [below, middle, above] = [heard[best - 1], heard[best], heard[best + 1]];
                    let found =
                        cents[best] + 2.5 * (below - above) / (below - 2.0 * middle + above);
                    assert!(
                        found.abs() < 10.0,
                        "vowel {vowel} voice {voice} formant {at} at {wanted} Hz: {found} cents"
                    );
                    checked += 1;
                }
                // And what comes out is the table's shape all through, at each formant and between
                // them, against a gain that's the same for all of it.
                let mut hertz: Vec<f32> = formants.iter().map(|f| hertz_of(f.note)).collect();
                for pair in formants.windows(2) {
                    hertz.push(hertz_of((pair[0].note + pair[1].note) / 2.0));
                }
                hertz.extend([150.0, 7_000.0]);
                let heard = response(settings, &hertz);
                let ratios: Vec<f32> = hertz
                    .iter()
                    .zip(&heard)
                    .map(|(&hertz, &heard)| db(heard, size_of(table_says(&formants, q, hertz))))
                    .collect();
                let (least, most) = ratios
                    .iter()
                    .fold((f32::MAX, f32::MIN), |(least, most), &r| {
                        (least.min(r), most.max(r))
                    });
                assert!(
                    most - least < 0.1,
                    "vowel {vowel} voice {voice}: {ratios:?} dB off the table at {hertz:?}"
                );
            }
        }
        // Most formants stand out at full `Res`: at least the first two of every vowel.
        assert!(checked >= 2 * VOWELS * VOICES, "{checked}");
    }

    #[test]
    fn the_morph_moves_smoothly_from_vowel_to_vowel_and_voice_to_voice() {
        // In quarter steps along `Vowel` and `Size`, no formant moves far or jumps in level.
        let (mut furthest, mut most) = (0.0_f32, 0.0_f32);
        for size in (0..=1020).step_by(17) {
            let size = size as f32 / 4.0;
            let mut last = formants(0.0, size);
            for vowel in 1..=1020 {
                let now = formants(vowel as f32 / 4.0, size);
                for (now, last) in now.iter().zip(&last) {
                    furthest = furthest.max((now.note - last.note).abs());
                    most = most.max((now.level - last.level).abs());
                }
                last = now;
            }
        }
        for vowel in (0..=1020).step_by(17) {
            let vowel = vowel as f32 / 4.0;
            let mut last = formants(vowel, 0.0);
            for size in 1..=1020 {
                let now = formants(vowel, size as f32 / 4.0);
                for (now, last) in now.iter().zip(&last) {
                    furthest = furthest.max((now.note - last.note).abs());
                    most = most.max((now.level - last.level).abs());
                }
                last = now;
            }
        }
        // The table's widest step between neighbours is 17 semitones, over 63 values.
        assert!(furthest < 0.07, "{furthest} semitones");
        assert!(most < 0.003, "{most}");
        // Between two vowels it's on the way from one to the other.
        let [a, e, half] = [0.0, 64.0, 32.0].map(|vowel| formants(vowel, 128.0));
        for ((a, e), half) in a.iter().zip(&e).zip(&half) {
            assert!((half.note - (a.note + e.note) / 2.0).abs() < 1e-4);
        }
    }

    #[test]
    fn pink_noise_comes_out_about_as_loud_as_it_went_in() {
        let input = pink(RATE as usize, 0.5);
        let dry = rms(input.iter().copied());
        for resonance in [0x00, 0x80, 0xFF] {
            for (voice, &size) in SIZE_AT.iter().enumerate() {
                for (vowel, &place) in VOWEL_AT.iter().enumerate() {
                    let settings = at(place, size, resonance);
                    let out = played(settings, 60.0, &input, RATE);
                    let level = db(rms(out.iter().flatten().copied()), dry);
                    assert!(
                        level.abs() < 1.5,
                        "{resonance:02X} vowel {vowel} voice {voice}: {level} dB"
                    );
                }
            }
        }
    }

    #[test]
    fn a_saw_and_a_drum_stay_within_a_few_db() {
        // A saw's harmonics fall on its formants or between them, depending on the note, and a
        // drum of white noise has most of its power above them, so neither is matched as well as
        // pink noise, but neither goes far.
        for (vowel, &place) in VOWEL_AT.iter().enumerate() {
            let settings = at(place, 0x80, 0x80);
            for note in [36.0, 48.0, 60.0, 72.0] {
                let input = saw(RATE as usize / 2, note, 0.5, RATE);
                let out = played(settings, note, &input, RATE);
                let level = db(
                    rms(out.iter().flatten().copied()),
                    rms(input.iter().copied()),
                );
                assert!(level.abs() < 9.0, "vowel {vowel} at {note}: {level} dB");
            }
            let input = drums(RATE as usize, 0.5, RATE);
            let out = played(settings, 60.0, &input, RATE);
            let level = db(
                rms(out.iter().flatten().copied()),
                rms(input.iter().copied()),
            );
            assert!((-13.0..0.0).contains(&level), "vowel {vowel}: {level} dB");
        }
    }

    #[test]
    fn track_moves_the_formants_with_the_note_and_none_leaves_them_fixed() {
        let tone = |hertz: f32| -> Vec<f32> {
            (0..RATE as usize / 2)
                .map(|frame| (frame as f32 / RATE as f32 * hertz * TAU).sin() * 0.01)
                .collect()
        };
        // `A`'s first formant, in the middle voice, at `F#4`.
        let first = hertz_of(TABLE[0][2][0][0].into());
        let fixed = at(0x00, 0x80, 0xFF);
        let tracking = Settings {
            track: 0xFF,
            ..fixed
        };
        let loudness = |settings: Settings, note: f32, hertz: f32| {
            rms(
                played(settings, note, &tone(hertz), RATE)[RATE as usize / 4..]
                    .iter()
                    .map(|frame| frame[0]),
            )
        };
        // With none, the note changes nothing.
        assert_eq!(
            played(fixed, 48.0, &tone(first), RATE),
            played(fixed, 84.0, &tone(first), RATE)
        );
        // With all of it, an octave up takes the formant an octave up.
        let up = loudness(tracking, 72.0, first * 2.0);
        assert!((db(up, loudness(fixed, 60.0, first))).abs() < 1.0);
        assert!(up > loudness(tracking, 72.0, first) * 4.0);
        // And halfway, half as far.
        let half = Settings {
            track: 0x80,
            ..fixed
        };
        let halfway = first * std::f32::consts::SQRT_2;
        assert!(loudness(half, 72.0, halfway) > loudness(half, 72.0, first) * 2.0);
    }

    #[test]
    fn sweeping_its_settings_and_note_glides_without_a_step() {
        let largest = |smoothing: f32| {
            let mut formants = Formants::default();
            let mut last = [0.0; 2];
            let mut largest = 0.0_f32;
            for frame in 0..RATE as usize * 2 {
                // A jump every 64 frames, as a knob or an LFO steps it block by block, and a new
                // note every 192.
                let step = (frame / 64 % 2) as u8;
                formants.follow(Settings {
                    vowel: step * 0xFF,
                    size: 0xFF - step * 0xFF,
                    resonance: 0x40 + step * 0xBF,
                    track: step * 0xFF,
                    mix: 0xFF,
                    gain: UNITY_LEVEL,
                });
                let note = 48.0 + (frame / 192 % 3) as f32 * 7.0;
                // A quiet tone among the formants, so what moves is the filter and not the sound.
                let tone = (frame as f32 / RATE as f32 * 500.0 * TAU).sin() * 0.05;
                let out = formants.process([tone; 2], true, note, smoothing, RATE);
                if frame > RATE as usize / 2 {
                    for side in 0..2 {
                        largest = largest.max((out[side] - last[side]).abs());
                    }
                }
                last = out;
            }
            largest
        };
        let (smoothed, unsmoothed) = (
            largest(crate::dsp::blocks::smoother::coefficient(RATE)),
            largest(NO_SMOOTHING),
        );
        assert!(smoothed < 0.02, "{smoothed} against {unsmoothed}");
        assert!(
            smoothed * 4.0 < unsmoothed,
            "{smoothed} against {unsmoothed}"
        );
    }

    #[test]
    fn it_stays_finite_and_within_full_scale_at_every_extreme() {
        for rate in [44_100, 192_000] {
            for vowel in [0x00, 0xFF] {
                for size in [0x00, 0xFF] {
                    for resonance in [0x00, 0xFF] {
                        for track in [0x00, 0xFF] {
                            let mut formants = Formants::default();
                            formants.follow(Settings {
                                vowel,
                                size,
                                resonance,
                                track,
                                ..Settings::DEFAULT
                            });
                            for frame in 0..rate as usize / 2 {
                                // A loud square, then the note jumping from one end to the other
                                // with every frame.
                                let input = if frame / 92 % 2 == 0 { 4.0 } else { -4.0 };
                                let note = if frame < rate as usize / 4 {
                                    60.0
                                } else if frame % 2 == 0 {
                                    -24.0
                                } else {
                                    151.0
                                };
                                let out = formants.process([input, -input], false, note, 1.0, rate);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                                    "{rate} {vowel:x} {size:x} {resonance:x} {track:x}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_loud_tone_on_a_formant_is_held_down_and_let_go() {
        let mut formants = Formants::default();
        formants.follow(at(0x00, 0x80, 0xFF));
        let hertz = hertz_of(TABLE[0][2][0][0].into());
        let mut loudest = 0.0_f32;
        for frame in 0..RATE as usize * 2 {
            let tone = (frame as f32 / RATE as f32 * hertz * TAU).sin();
            let out = formants.process([tone; 2], true, 60.0, NO_SMOOTHING, RATE);
            loudest = loudest.max(out[0].abs());
        }
        assert!(loudest <= 1.0, "{loudest}");
        assert!(formants.limiting() < 0.5, "{}", formants.limiting());
        for _ in 0..RATE as usize * 8 {
            formants.process([0.0; 2], true, 60.0, NO_SMOOTHING, RATE);
        }
        assert!(formants.limiting() > 0.9, "{}", formants.limiting());
    }

    #[test]
    fn it_takes_little_room_in_a_slot() {
        // Every slot keeps one, whatever it holds, as it does an EQ's bands.
        let size = std::mem::size_of::<Formants>();
        assert!(size <= 384, "{size} bytes");
    }
}
