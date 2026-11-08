//! A bank of resonant modes tuned to a note, from Mutable Instruments' Plaits, with Rings' limiter
//! and Elements' second pickup. Ported to Rust from `plaits/dsp/physical_modelling/resonator.cc`
//! and `resonator.h`, `rings/dsp/limiter.h`, and the pickups of `elements/dsp/resonator.cc` and
//! `elements/dsp/part.cc`. The `Modal` voice strikes the bank itself, and the `Reso` slot effect
//! strikes it with the track's sound.
//!
//! Copyright 2014, 2015, 2016 Emilie Gillet.
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

use vt_model::insert::{Resonator as Settings, shift_semitones};

use crate::dsp::drums::soft_clip;
use crate::dsp::plaits_parts::{CosineOscillator, STIFFNESS, interpolate, tan_fast};
use crate::dsp::smoother::Smoothed;

/// How many modes there are, as many as Plaits has.
pub(super) const MODES: usize = 24;
/// How many frames the slot effect plays between workings-out of its modes, as long as Plaits'
/// blocks at 48 kHz.
const BLOCK: usize = 24;
/// The rate Plaits', Rings' and Elements' per-sample constants are set for.
const MUTABLE_RATE: f32 = 48_000.0;
/// How hard what comes in strikes the bank, as Rings and Elements turn their input down.
const INPUT_GAIN: f32 = 0.125;
/// How loud what comes out is, before the limiter, so a drum comes out about as loud as it went
/// in.
const LEVEL: f32 = 3.8;
/// How long a hit the level is matched for, in seconds. A mode ringing for less than this passes
/// a hit as a filter would, and one ringing for longer gathers it up as it would a click.
const HIT_SECONDS: f32 = 0.02;
/// The lowest and highest the bank is tuned to.
const LOWEST_HZ: f32 = 16.0;
const HIGHEST_HZ: f32 = 12_000.0;
/// How fast Elements' second pickup wanders, and from where: a slow triangle taking it from a
/// tenth of the way along to past the middle and back, as Elements starts it.
const PICKUP_HZ: f32 = 0.5;
const PICKUP_OFFSET: f32 = 0.1;
/// How far `Width` spreads the second pickup across the sides at `FF`, as far as Elements does.
const MOST_SPREAD: f32 = 0.7;
/// How long it keeps going once nothing comes in and nothing's heard, in seconds. A low mode
/// near silence can spend a while near nothing as it crosses zero.
const QUIET_SECONDS: f32 = 0.1;

/// A ratio for an interval in semitones.
#[inline]
pub(super) fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// How much the third partial's stretch shifts the note, so it can be taken back out.
fn nth_harmonic_compensation(n: usize, mut stiffness: f32) -> f32 {
    let mut stretch = 1.0;
    for _ in 0..n - 1 {
        stretch += stiffness;
        stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
    }
    1.0 / stretch
}

/// Plaits' bank: a band-pass for each mode, its partials whole multiples of the note stretched by
/// how stiff it is, each higher one losing its ring faster as it gets darker. Its tuning is worked
/// out once in a while, and it's played a frame at a time.
#[derive(Debug, Clone, Copy)]
pub(super) struct Modes {
    g: [f32; MODES],
    r_plus_g: [f32; MODES],
    h: [f32; MODES],
    state_1: [f32; MODES],
    state_2: [f32; MODES],
    /// How much each mode is turned down for being near the top, as Plaits turns them down.
    pub(super) attenuation: [f32; MODES],
    /// Each mode's frequency, as a fraction of the rate, and its Q, for working out how loud it is.
    frequency: [f32; MODES],
    quality: [f32; MODES],
}

impl Default for Modes {
    fn default() -> Self {
        let mut modes = Self {
            g: [0.0; MODES],
            r_plus_g: [0.0; MODES],
            h: [0.0; MODES],
            state_1: [0.0; MODES],
            state_2: [0.0; MODES],
            attenuation: [0.0; MODES],
            frequency: [0.0; MODES],
            quality: [0.0; MODES],
        };
        modes.tune(0.01, 0.25, 0.5, 0.5, 1.0);
        modes
    }
}

impl Modes {
    /// Tune it to `f0`, a fraction of the rate, with Plaits' structure, brightness and damping
    /// from `0` to `1`. `q_scale` stretches every mode's Q, so it rings as long at a rate other
    /// than Plaits' 48 kHz.
    pub(super) fn tune(
        &mut self,
        f0: f32,
        structure: f32,
        mut brightness: f32,
        damping: f32,
        q_scale: f32,
    ) {
        let mut stiffness = interpolate(&STIFFNESS, structure, 64.0);
        let f0 = f0 * nth_harmonic_compensation(3, stiffness);
        let mut harmonic = f0;
        let mut stretch = 1.0;
        let q_sqrt = semitones(damping * 79.7);
        let mut q = 500.0 * q_sqrt * q_sqrt * q_scale;
        brightness *= 1.0 - structure * 0.3;
        brightness *= 1.0 - damping * 0.3;
        let q_loss = brightness * (2.0 - brightness) * 0.85 + 0.15;
        for mode in 0..MODES {
            let frequency = (harmonic * stretch).min(0.499);
            self.attenuation[mode] = 1.0 - frequency * 2.0;
            let g = tan_fast(frequency);
            let quality = 1.0 + frequency * q;
            let r = 1.0 / quality;
            self.frequency[mode] = frequency;
            self.quality[mode] = quality;
            self.g[mode] = g;
            self.r_plus_g[mode] = r + g;
            self.h[mode] = 1.0 / (1.0 + r * g + g * g);
            stretch += stiffness;
            // Partials mustn't fold back below nothing, and a few more fit at the top.
            stiffness *= if stiffness < 0.0 { 0.93 } else { 0.98 };
            harmonic += f0;
            q *= q_loss;
        }
    }

    /// One frame in, and the modes' band-passes summed with each of `P` sets of gains, one for
    /// each pickup.
    #[inline]
    pub(super) fn process<const P: usize>(
        &mut self,
        input: f32,
        gains: &[[f32; MODES]; P],
    ) -> [f32; P] {
        let mut out = [0.0; P];
        for mode in 0..MODES {
            let (g, state_1, state_2) = (self.g[mode], self.state_1[mode], self.state_2[mode]);
            let hp = (input - self.r_plus_g[mode] * state_1 - state_2) * self.h[mode];
            let bp = g * hp + state_1;
            self.state_1[mode] = g * hp + bp;
            let lp = g * bp + state_2;
            self.state_2[mode] = g * bp + lp;
            for (out, gains) in out.iter_mut().zip(gains) {
                *out += gains[mode] * bp;
            }
        }
        out
    }
}

/// A per-sample coefficient set for 48 kHz, changed to take as long at `rate`.
fn at_rate(coefficient: f32, rate: u32) -> f32 {
    1.0 - (1.0 - coefficient).powf(MUTABLE_RATE / rate as f32)
}

/// Rings' limiter: it follows the loudest of the two sides and their difference, rising quickly
/// and falling over about a second, turns both down together to keep that under full scale, and
/// clips softly what gets past it first.
#[derive(Debug, Clone, Copy)]
struct Limiter {
    peak: f32,
    attack: f32,
    release: f32,
}

impl Limiter {
    fn new(rate: u32) -> Self {
        Self {
            peak: 0.5,
            attack: at_rate(0.05, rate),
            release: at_rate(0.000_02, rate),
        }
    }

    /// How far it's turning down now, as a gain.
    fn gain(&self) -> f32 {
        if self.peak <= 1.0 {
            1.0
        } else {
            1.0 / self.peak
        }
    }

    #[inline]
    fn process(&mut self, [left, right]: [f32; 2]) -> [f32; 2] {
        let peak = left.abs().max(right.abs()).max((right - left).abs());
        let slope = if peak > self.peak {
            self.attack
        } else {
            self.release
        };
        self.peak += slope * (peak - self.peak);
        let gain = self.gain() * 0.8;
        [soft_clip(left * gain), soft_clip(right * gain)]
    }
}

/// What a `Reso` slot keeps, from the effect pool, as its modes and their gains come to more than
/// a kilobyte.
#[derive(Debug, Clone)]
pub struct Resonator {
    modes: Modes,
    /// The MIDI note it's tuned to, gliding to the track's with `Tune` added.
    note: Smoothed,
    tune: f32,
    material: Smoothed,
    bright: Smoothed,
    decay: Smoothed,
    /// Where it's heard, as a fraction of a cycle of the pickup's cosine across the modes.
    position: Smoothed,
    width: Smoothed,
    /// What each mode adds to each side now, how much that moves each frame, and where it's
    /// going, so a change in where it's heard never steps.
    gains: [[f32; MODES]; 2],
    steps: [[f32; MODES]; 2],
    targets: [[f32; MODES]; 2],
    /// Where Elements' slow triangle is, as a fraction of its cycle, and how far it goes a block.
    phase: f32,
    increment: f32,
    limiter: Limiter,
    /// How many frames of this block are left before the modes are worked out again.
    left: usize,
    /// Whether it's only just started, so it starts at the note and what's set rather than
    /// gliding to them.
    fresh: bool,
    /// The settings last followed, so the first jumps to them.
    settings: Option<Settings>,
    q_scale: f32,
    rate: u32,
}

impl Resonator {
    pub fn new(rate: u32) -> Self {
        Self {
            modes: Modes::default(),
            note: Smoothed::at(60.0),
            tune: 0.0,
            material: Smoothed::default(),
            bright: Smoothed::default(),
            decay: Smoothed::default(),
            position: Smoothed::default(),
            width: Smoothed::default(),
            gains: [[0.0; MODES]; 2],
            steps: [[0.0; MODES]; 2],
            targets: [[0.0; MODES]; 2],
            phase: 0.0,
            increment: PICKUP_HZ * BLOCK as f32 / rate as f32,
            limiter: Limiter::new(rate),
            left: 0,
            fresh: true,
            settings: None,
            q_scale: rate as f32 / MUTABLE_RATE,
            rate,
        }
    }

    /// Forget everything it was playing, for a fresh start.
    pub fn clear(&mut self) {
        let settings = self.settings;
        *self = Self::new(self.rate);
        if let Some(settings) = settings {
            self.follow(settings);
        }
    }

    /// How many quiet frames in a row mean it's done.
    pub fn quiet_frames(&self) -> u32 {
        (QUIET_SECONDS * self.rate as f32) as u32
    }

    /// Take what's set now, jumping to it the first time.
    pub fn follow(&mut self, settings: Settings) {
        if self.settings == Some(settings) {
            return;
        }
        self.tune = shift_semitones(f32::from(settings.tune));
        self.material.target = f32::from(settings.material) / 255.0;
        self.bright.target = f32::from(settings.bright) / 255.0;
        self.decay.target = f32::from(settings.decay) / 255.0;
        // Its end to its middle: past that, a cosine's cycle comes back the way it went.
        self.position.target = f32::from(settings.position) / 255.0 * 0.5;
        self.width.target = f32::from(settings.width) / 255.0 * MOST_SPREAD;
        if self.settings.is_none() {
            self.settle();
        }
        self.settings = Some(settings);
    }

    /// Jump to what's set and the next note, rather than gliding, as when it's been resting.
    pub fn settle(&mut self) {
        self.fresh = true;
        self.left = 0;
    }

    /// One frame in, with no note, for the pool's own use.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input, 60.0, 1.0)
    }

    /// One frame of the slot's sound striking the bank, tuned to `note` and its `Tune` above it,
    /// and the frame out, all wet and within full scale.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], note: f32, smoothing: f32) -> [f32; 2] {
        if self.left == 0 {
            self.work_out(note, smoothing);
            self.left = BLOCK;
        }
        self.left -= 1;
        let struck = (input[0] + input[1]) * 0.5 * INPUT_GAIN;
        let out = self.modes.process(struck, &self.gains);
        for (gains, steps) in self.gains.iter_mut().zip(&self.steps) {
            for (gain, step) in gains.iter_mut().zip(steps) {
                *gain += step;
            }
        }
        self.limiter.process(out)
    }

    /// Move the settings and the note on a block, tune the modes, and set each one's gains to
    /// move to where the pickups now hear it over the block.
    fn work_out(&mut self, note: f32, smoothing: f32) {
        let smoothing = 1.0 - (1.0 - smoothing).powi(BLOCK as i32);
        self.note.target = note + self.tune;
        let smoothed = [
            &mut self.note,
            &mut self.material,
            &mut self.bright,
            &mut self.decay,
            &mut self.position,
            &mut self.width,
        ];
        let [note, material, bright, decay, position, width] = smoothed.map(|value| {
            if self.fresh {
                value.settle();
            }
            value.next(smoothing)
        });
        let hertz = (440.0 * semitones(note - 69.0)).clamp(LOWEST_HZ, HIGHEST_HZ);
        let f0 = (hertz / self.rate as f32).min(0.45);
        self.modes.tune(f0, material, bright, decay, self.q_scale);

        // Elements' second pickup wanders on a slow triangle, and what it hears apart from the
        // first is spread across the sides.
        self.phase += self.increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let wander = if self.phase > 0.5 {
            1.0 - self.phase
        } else {
            self.phase
        };
        let mut centre = CosineOscillator::new(position);
        let mut side = CosineOscillator::new(PICKUP_OFFSET + wander);
        // A band-pass passes noise as loud as its frequency times its Q, and a hit as loud as
        // its frequency times as many cycles as ring through it, so the level is matched for
        // both by turning down what all the modes together would pass.
        let hit_cycles = std::f32::consts::PI * HIT_SECONDS * self.rate as f32;
        let mut heard = [(0.0, 0.0); MODES];
        let mut power = 0.0;
        for (mode, heard) in heard.iter_mut().enumerate() {
            let attenuation = self.modes.attenuation[mode];
            *heard = (centre.next() * attenuation, side.next() * attenuation);
            let frequency = self.modes.frequency[mode];
            let quality = self.modes.quality[mode].min(hit_cycles * frequency);
            power += heard.0 * heard.0 * frequency * quality;
        }
        let level = LEVEL / power.max(1e-12).sqrt();
        let previous = self.targets;
        let [left, right] = &mut self.targets;
        for ((left, right), (centre, side)) in left.iter_mut().zip(right).zip(heard) {
            let centre = centre * level;
            let spread = width * (side * level - centre);
            *left = centre + spread;
            *right = centre - spread;
        }
        if self.fresh {
            self.gains = self.targets;
            self.steps = [[0.0; MODES]; 2];
            self.fresh = false;
        } else {
            self.gains = previous;
            let moves = self.steps.iter_mut().zip(&self.targets).zip(&previous);
            for ((steps, targets), previous) in moves {
                for ((step, target), previous) in steps.iter_mut().zip(targets).zip(previous) {
                    *step = (target - previous) / BLOCK as f32;
                }
            }
        }
    }

    /// How far its limiter is turning down now, as a gain, for its tests.
    #[cfg(test)]
    fn limiting(&self) -> f32 {
        self.limiter.gain()
    }
}

#[cfg(test)]
mod tests {
    use vt_model::constants::UNITY_LEVEL;
    use vt_model::insert::MOST_SHIFT;

    use super::*;
    use crate::rng::Pcg32;

    const RATE: u32 = 48_000;
    /// Settings take effect at once, as the smoother has nothing to do.
    const NO_SMOOTHING: f32 = 1.0;

    /// A drum: a burst of noise falling away over about 20 ms, every quarter of a second.
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

    /// One drum hit, then nothing.
    fn hit(frames: usize, rate: u32) -> Vec<f32> {
        let mut out = drums(frames, 0.5, rate);
        out[rate as usize / 10..].fill(0.0);
        out
    }

    fn played(settings: Settings, note: f32, input: &[f32], rate: u32) -> Vec<[f32; 2]> {
        let mut resonator = Resonator::new(rate);
        resonator.follow(settings);
        input
            .iter()
            .map(|&sample| resonator.process([sample; 2], note, NO_SMOOTHING))
            .collect()
    }

    fn rms(samples: impl Iterator<Item = f32>) -> f32 {
        let (sum, count) = samples.fold((0.0, 0), |(sum, count), s| (sum + s * s, count + 1));
        (sum / count as f32).sqrt()
    }

    fn peak(samples: impl Iterator<Item = f32>) -> f32 {
        samples.fold(0.0, |most: f32, s| most.max(s.abs()))
    }

    /// The frequency of what's ringing, in Hz, from where it crosses zero going up.
    fn frequency(samples: &[f32], rate: u32) -> f32 {
        let crossings: Vec<f32> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0)
            .map(|(at, pair)| at as f32 + pair[0] / (pair[0] - pair[1]))
            .collect();
        let cycles = crossings.len() - 1;
        cycles as f32 / (crossings[cycles] - crossings[0]) * rate as f32
    }

    fn cents(found: f32, wanted: f32) -> f32 {
        1200.0 * (found / wanted).log2()
    }

    fn hertz_of(note: f32) -> f32 {
        440.0 * semitones(note - 69.0)
    }

    /// One mode of the bank on its own, struck once and left to ring.
    fn mode_rings_at(structure: f32, f0: f32, mode: usize, rate: u32) -> f32 {
        let mut modes = Modes::default();
        modes.tune(f0, structure, 0.5, 0.9, rate as f32 / MUTABLE_RATE);
        let mut gains = [[0.0; MODES]];
        gains[0][mode] = 1.0;
        let out: Vec<f32> = (0..rate as usize / 2)
            .map(|frame| modes.process(if frame == 0 { 1.0 } else { 0.0 }, &gains)[0])
            .collect();
        frequency(&out[rate as usize / 100..], rate)
    }

    /// The frequency within a semitone of `near` that's strongest in some samples, to a cent.
    fn strongest_near(samples: &[f32], near: f32, rate: u32) -> f32 {
        (-100..=100)
            .map(|cents| near * (cents as f32 / 1200.0).exp2())
            .max_by(|a, b| strength(samples, *a, rate).total_cmp(&strength(samples, *b, rate)))
            .unwrap()
    }

    /// How loud both sides are together.
    fn loudness(frames: &[[f32; 2]]) -> f32 {
        rms(frames.iter().flatten().copied())
    }

    /// How strong a frequency is in some samples.
    fn strength(samples: &[f32], hertz: f32, rate: u32) -> f32 {
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for (frame, sample) in samples.iter().enumerate() {
            let phase = frame as f64 / f64::from(rate) * f64::from(hertz) * std::f64::consts::TAU;
            real += f64::from(*sample) * phase.cos();
            imaginary += f64::from(*sample) * phase.sin();
        }
        ((real * real + imaginary * imaginary).sqrt() * 2.0 / samples.len() as f64) as f32
    }

    #[test]
    fn its_lowest_mode_sits_at_the_note() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            for note in [24.0, 36.0, 48.0, 60.0, 72.0, 84.0, 96.0] {
                let wanted = hertz_of(note);
                // `Material` from `40` to `4C` is in tune.
                for structure in [0.25, 0.29] {
                    let found = mode_rings_at(structure, wanted / rate as f32, 0, rate);
                    assert!(
                        cents(found, wanted).abs() < 3.0,
                        "{rate} {note}: {found} for {wanted}"
                    );
                }
            }
        }
    }

    #[test]
    fn material_spreads_the_partials_and_keeps_the_third_in_tune() {
        let f0 = 220.0 / RATE as f32;
        let ratio = |structure| {
            mode_rings_at(structure, f0, 1, RATE) / mode_rings_at(structure, f0, 0, RATE)
        };
        // Whole multiples where it's in tune, bunched below, and spread wider above, as a
        // bell's are.
        assert!((ratio(0.25) - 2.0).abs() < 0.005, "{}", ratio(0.25));
        assert!(ratio(0.0) < 1.95, "{}", ratio(0.0));
        assert!(ratio(1.0) > 2.2, "{}", ratio(1.0));
        assert!(ratio(0.75) > 2.02 && ratio(0.75) < ratio(1.0));
        // Plaits keeps the third partial at three times the note however they spread.
        for structure in [0.0, 0.5, 0.75, 1.0] {
            let third = mode_rings_at(structure, f0, 2, RATE);
            assert!(cents(third, 660.0).abs() < 5.0, "{structure}: {third}");
        }
    }

    #[test]
    fn it_rings_at_the_note_and_tune_above_it() {
        // Dark, so the lowest mode outlasts the others, and long, so it's still ringing.
        let settings = |tune| Settings {
            tune,
            bright: 0x00,
            decay: 0xC0,
            width: 0x00,
            ..Settings::DEFAULT
        };
        for (tune, note) in [
            (MOST_SHIFT, 57.0),
            (MOST_SHIFT + 7, 57.0),
            (MOST_SHIFT - 12, 69.0),
        ] {
            let out = played(settings(tune), note, &hit(RATE as usize, RATE), RATE);
            let left: Vec<f32> = out[RATE as usize / 2..]
                .iter()
                .map(|frame| frame[0])
                .collect();
            let wanted = hertz_of(note + shift_semitones(f32::from(tune)));
            let found = strongest_near(&left, wanted, RATE);
            assert!(
                cents(found, wanted).abs() < 3.0,
                "{tune} {note}: {found} for {wanted}"
            );
        }
    }

    #[test]
    fn decay_makes_it_ring_longer_and_as_long_at_any_rate() {
        let tail = |decay, rate: u32| {
            let settings = Settings {
                decay,
                ..Settings::DEFAULT
            };
            let out = played(settings, 60.0, &hit(rate as usize, rate), rate);
            // A tenth of a second around half a second in, well after the hit.
            let around = rate as usize / 2;
            rms(out[around..around + rate as usize / 10]
                .iter()
                .map(|frame| frame[0]))
        };
        let (short, middle, long) = (tail(0x40, RATE), tail(0x80, RATE), tail(0xC0, RATE));
        assert!(short < 1e-4, "{short}");
        assert!(
            middle > short * 10.0 && long > middle * 5.0,
            "{short} {middle} {long}"
        );
        for decay in [0x80, 0xC0] {
            let (at_48, at_96) = (tail(decay, RATE), tail(decay, 96_000));
            let db = 20.0 * (at_96 / at_48).log10();
            assert!(db.abs() < 2.0, "{decay:x}: {db} dB");
        }
    }

    #[test]
    fn a_drum_comes_out_about_as_loud_as_it_went_in() {
        let input = drums(RATE as usize * 2, 0.5, RATE);
        let out = played(Settings::DEFAULT, 60.0, &input, RATE);
        let db = |wet: f32, dry: f32| 20.0 * (wet / dry).log10();
        let level = db(loudness(&out), rms(input.iter().copied()));
        assert!(level.abs() < 1.5, "{level} dB");
        let loudest = db(
            peak(out.iter().flatten().copied()),
            peak(input.iter().copied()),
        );
        assert!(loudest.abs() < 1.5, "{loudest} dB");
        // And within a few dB of that across the notes it's likely to play, while it rings.
        for note in [36.0, 48.0, 72.0, 84.0] {
            let out = played(Settings::DEFAULT, note, &input, RATE);
            let level = db(loudness(&out), rms(input.iter().copied()));
            assert!(level.abs() < 3.0, "{note}: {level} dB");
        }
    }

    #[test]
    fn position_in_the_middle_leaves_out_the_even_partials() {
        let second = |position| {
            let settings = Settings {
                position,
                width: 0x00,
                ..Settings::DEFAULT
            };
            let out = played(settings, 48.0, &drums(RATE as usize, 0.5, RATE), RATE);
            let left: Vec<f32> = out.iter().map(|frame| frame[0]).collect();
            let hertz = hertz_of(48.0);
            strength(&left, hertz * 2.0, RATE) / strength(&left, hertz, RATE)
        };
        let (end, middle) = (second(0x00), second(0xFF));
        assert!(middle < end * 0.1, "{end} {middle}");
    }

    #[test]
    fn width_spreads_it_across_the_sides() {
        let input = drums(RATE as usize, 0.5, RATE);
        let mono = played(
            Settings {
                width: 0x00,
                ..Settings::DEFAULT
            },
            60.0,
            &input,
            RATE,
        );
        assert!(mono.iter().all(|frame| frame[0] == frame[1]));
        let wide = played(
            Settings {
                width: 0xFF,
                ..Settings::DEFAULT
            },
            60.0,
            &input,
            RATE,
        );
        let sides = rms(wide.iter().map(|frame| frame[0] - frame[1]));
        let middle = rms(wide.iter().map(|frame| frame[0] + frame[1]));
        assert!(sides > middle * 0.1, "{sides} {middle}");
    }

    #[test]
    fn moving_its_settings_and_note_glides_without_a_step() {
        let smoothing = crate::dsp::smoother::coefficient(RATE);
        let mut resonator = Resonator::new(RATE);
        let mut last = [0.0; 2];
        let mut rng = Pcg32::new(3, 1);
        let mut largest = 0.0_f32;
        for frame in 0..RATE as usize * 4 {
            // A jump every 64 frames, as a knob or an LFO steps it block by block, and a new
            // note every 64 as well.
            let step = (frame / 64 % 2) as u8;
            resonator.follow(Settings {
                tune: MOST_SHIFT + step * 7,
                material: 0x20 + step * 0xC0,
                bright: 0x20 + step * 0xC0,
                decay: 0x60 + step * 0x60,
                position: step * 0xFF,
                width: step * 0xFF,
                mix: 0xFF,
                gain: UNITY_LEVEL,
            });
            let note = 36.0 + (frame / 192 % 3) as f32 * 5.0;
            // A quiet hiss, so what moves is the resonator and not the sound.
            let noise = (rng.next_u32() as f32 / 2_147_483_648.0 - 1.0) * 0.02;
            let out = resonator.process([noise; 2], note, smoothing);
            if frame > RATE as usize / 2 {
                for side in 0..2 {
                    largest = largest.max((out[side] - last[side]).abs());
                }
            }
            last = out;
        }
        assert!(largest < 0.05, "{largest}");
    }

    #[test]
    fn it_stays_finite_and_within_full_scale_at_every_extreme() {
        for rate in [44_100, 192_000] {
            for material in [0x00, 0x40, 0xFF] {
                for bright in [0x00, 0xFF] {
                    for decay in [0x00, 0xFF] {
                        for position in [0x00, 0xFF] {
                            let mut resonator = Resonator::new(rate);
                            resonator.follow(Settings {
                                material,
                                bright,
                                decay,
                                position,
                                width: 0xFF,
                                ..Settings::DEFAULT
                            });
                            for frame in 0..rate as usize / 2 {
                                // A loud square at the note, then the note jumping from one end
                                // to the other with every frame.
                                let input = if frame / 92 % 2 == 0 { 4.0 } else { -4.0 };
                                let note = if frame < rate as usize / 4 {
                                    60.0
                                } else if frame % 2 == 0 {
                                    -24.0
                                } else {
                                    151.0
                                };
                                let out = resonator.process([input, -input], note, 1.0);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                                    "{rate} {material:x} {bright:x} {decay:x}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_long_loud_tone_at_its_note_is_held_down_and_let_go() {
        let mut resonator = Resonator::new(RATE);
        resonator.follow(Settings {
            decay: 0xFF,
            bright: 0xFF,
            ..Settings::DEFAULT
        });
        let hertz = hertz_of(60.0);
        let mut loudest = 0.0_f32;
        for frame in 0..RATE as usize * 10 {
            let tone = (frame as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 2.0;
            let out = resonator.process([tone; 2], 60.0, NO_SMOOTHING);
            loudest = loudest.max(out[0].abs());
        }
        assert!(loudest <= 1.0, "{loudest}");
        assert!(resonator.limiting() < 0.01, "{}", resonator.limiting());
        // Once it's stopped ringing, the limiter lets go over a few seconds.
        resonator.follow(Settings {
            decay: 0x00,
            ..Settings::DEFAULT
        });
        for _ in 0..RATE as usize * 8 {
            resonator.process([0.0; 2], 60.0, NO_SMOOTHING);
        }
        assert!(resonator.limiting() > 0.1, "{}", resonator.limiting());
    }
}
