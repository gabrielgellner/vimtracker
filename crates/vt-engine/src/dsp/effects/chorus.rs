//! A stereo chorus and a string machine's ensemble, as in Mutable Instruments' Rings: copies of
//! the sound read from short delay lines whose length a sine wobbles. Rings' delays and wobbles
//! are tuned at 48 kHz and scaled here to the rate it runs at, and read between samples. Each
//! wobble is a point going round a circle rather than a sine worked out, as Surge's quadrature
//! oscillators are (`sst-basic-blocks/dsp/QuadratureOscillators.h:98-132`, GPL 3 or later), so
//! the copies' other phases are fixed turns of it.
//!
//! Adapted from `rings/dsp/fx/chorus.h` and `rings/dsp/fx/ensemble.h`:
//!
//! Copyright 2015 Emilie Gillet.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction,
//! including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
//! NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
//! OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::f32::consts::TAU;

use vt_model::insert::{Chorus as Settings, ChorusType};

use crate::dsp::blocks::smoother::{Smoothed, coefficient};

/// The rate Rings' delays and wobbles are tuned at.
const TUNING_RATE: f32 = 48_000.0;
/// The longest a copy is read back, at the tuning rate: the chorus's longer tap at full depth,
/// with room to read between samples.
const LONGEST: f32 = 1200.0 + 384.0 + 4.0;

/// Where the chorus's two taps sit, and how far a full depth moves them, at the tuning rate.
const CHORUS_TAPS: [f32; 2] = [1200.0, 800.0];
const CHORUS_DEPTH: f32 = 384.0;
/// How far round each of the chorus's two wobbles goes in a frame at the tuning rate, as Rings
/// has them: about 0.2 and 0.26 Hz.
const CHORUS_SPEEDS: [f32; 2] = [4.17e-06, 5.417e-06];

/// Where the ensemble's three copies sit, and how far a full depth moves them.
const ENSEMBLE_TAP: f32 = 1024.0;
const ENSEMBLE_DEPTH: f32 = 128.0;
/// Its slow and fast wobbles: about 0.75 and 6.6 Hz.
const ENSEMBLE_SPEEDS: [f32; 2] = [1.57e-05, 1.37e-04];
/// How much of the slow wobble the fast one adds.
const ENSEMBLE_FAST: f32 = 0.1;
/// The cosine and sine of the ensemble's copies' places, in sixths of a turn: Rings' three a
/// third apart on the left, and on the right those a sixth on.
const SIXTHS: [(f32, f32); 6] = [
    (1.0, 0.0),
    (0.5, 0.866_025_4),
    (-0.5, 0.866_025_4),
    (-1.0, 0.0),
    (-0.5, -0.866_025_4),
    (0.5, -0.866_025_4),
];

/// A wobble, as a point going round a circle: its sine and cosine without working either out.
#[derive(Debug, Clone, Copy)]
struct Wobble {
    cos: f32,
    sin: f32,
}

impl Wobble {
    const START: Self = Self { cos: 1.0, sin: 0.0 };

    /// Go round by some turns, under a hundredth of a radian.
    #[inline]
    fn turn(&mut self, turns: f32) {
        let angle = turns * TAU;
        let squared = angle * angle;
        // The series' first terms, which are as close as an f32 gets at angles this small.
        let (cos, sin) = (1.0 - squared * 0.5, angle * (1.0 - squared / 6.0));
        let (c, s) = (
            self.cos * cos - self.sin * sin,
            self.sin * cos + self.cos * sin,
        );
        // A step of Newton's back toward the circle, so rounding never moves it off.
        let back = 1.5 - 0.5 * (c * c + s * s);
        self.cos = c * back;
        self.sin = s * back;
    }

    /// The sine some of a turn further on, from that turn's cosine and sine.
    #[inline]
    fn sine_on(self, (cos, sin): (f32, f32)) -> f32 {
        self.sin * cos + self.cos * sin
    }
}

/// A chorus's delay lines and wobbles.
#[derive(Debug, Clone)]
pub struct Chorus {
    lines: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    wobbles: [Wobble; 2],
    kind: ChorusType,
    /// How much faster than Rings the wobbles go.
    speed: Smoothed,
    /// How far the copies move, from 0 to 1.
    depth: Smoothed,
    /// The tuning rate over this one, for the wobbles, and this one over the tuning rate, for the
    /// delays.
    slower: f32,
    longer: f32,
    smoothing: f32,
}

impl Chorus {
    /// Lines long enough for its longest read at this rate.
    pub fn new(rate: u32) -> Self {
        let longer = rate as f32 / TUNING_RATE;
        let size = ((LONGEST * longer).ceil() as usize + 2).next_power_of_two();
        Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            wobbles: [Wobble::START; 2],
            kind: ChorusType::Chorus,
            speed: Smoothed::at(1.0),
            depth: Smoothed::at(0.5),
            slower: TUNING_RATE / rate as f32,
            longer,
            smoothing: coefficient(rate),
        }
    }

    /// The longest anything stays in it, in frames.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.wobbles = [Wobble::START; 2];
    }

    /// Take what's set now. The speed is two octaves either way of Rings' at `80`.
    pub fn follow(&mut self, settings: Settings) {
        self.kind = settings.kind;
        self.speed.target = ((f32::from(settings.rate) - 128.0) / 64.0).exp2();
        self.depth.target = f32::from(settings.depth) / 255.0;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.speed.settle();
        self.depth.settle();
    }

    /// A line's sound a number of frames back, read between samples.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let frames = frames.max(1.0);
        let whole = frames as usize;
        let fraction = frames - whole as f32;
        let line = &self.lines[side];
        let newer = line[self.write.wrapping_sub(whole) & self.mask];
        let older = line[self.write.wrapping_sub(whole + 1) & self.mask];
        newer + (older - newer) * fraction
    }

    /// Feed one frame in and get the copies out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let speed = self.speed.next(self.smoothing) * self.slower;
        let depth = self.depth.next(self.smoothing);
        let speeds = match self.kind {
            ChorusType::Chorus => CHORUS_SPEEDS,
            ChorusType::Ensemble => ENSEMBLE_SPEEDS,
        };
        for (wobble, step) in self.wobbles.iter_mut().zip(speeds) {
            wobble.turn(step * speed);
        }
        self.write = (self.write + 1) & self.mask;
        let longer = self.longer;
        match self.kind {
            ChorusType::Chorus => {
                // Both sides go down one line, and two taps each wobble apart from the other.
                self.lines[0][self.write] = (input[0] + input[1]) * 0.5;
                let amount = depth * CHORUS_DEPTH;
                // The right side's are a quarter of a turn on: the cosines.
                let [first, second] = self.wobbles;
                let left = self.read(0, (first.sin * amount + CHORUS_TAPS[0]) * longer)
                    + self.read(0, (second.sin * amount + CHORUS_TAPS[1]) * longer);
                let right = self.read(0, (first.cos * amount + CHORUS_TAPS[1]) * longer)
                    + self.read(0, (second.cos * amount + CHORUS_TAPS[0]) * longer);
                [left * 0.5, right * 0.5]
            }
            ChorusType::Ensemble => {
                // Each side has its own line, and three copies a third of a turn apart cross over.
                self.lines[0][self.write] = input[0];
                self.lines[1][self.write] = input[1];
                let (slow, fast) = (
                    depth * ENSEMBLE_DEPTH,
                    depth * ENSEMBLE_DEPTH * ENSEMBLE_FAST,
                );
                let [first, second] = self.wobbles;
                let wobble = |sixths: usize| {
                    let on = SIXTHS[sixths];
                    (first.sine_on(on) * slow + second.sine_on(on) * fast + ENSEMBLE_TAP) * longer
                };
                // Rings took a stereo string, so each side's copies mirrored the other's. A track's
                // sound is mostly one, so the right side's copies are a sixth of a turn on from the
                // left's, which spreads even a single sound.
                let (one, two, three) = (wobble(0), wobble(2), wobble(4));
                let left = self.read(0, one) + self.read(0, two) + self.read(1, three);
                let (four, five, six) = (wobble(1), wobble(3), wobble(5));
                let right = self.read(1, four) + self.read(1, five) + self.read(0, six);
                [left * 0.33, right * 0.33]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn chorus(kind: ChorusType, depth: u8) -> Chorus {
        let mut chorus = Chorus::new(RATE);
        chorus.follow(Settings {
            kind,
            depth,
            ..Settings::DEFAULT
        });
        chorus.settle();
        chorus
    }

    /// A sine through it, with what comes out after it's filled.
    fn through(chorus: &mut Chorus, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|frame| {
                let sample = (frame as f32 / RATE as f32 * 440.0 * TAU).sin() * 0.5;
                chorus.process([sample, sample])
            })
            .skip(4_800)
            .collect()
    }

    #[test]
    fn its_copies_come_after_the_sound_and_as_loud() {
        for kind in ChorusType::ALL {
            let mut chorus = chorus(kind, 0x80);
            // Nothing comes out until the shortest copy has been round.
            let first = chorus.process([1.0, 1.0]);
            assert_eq!(first, [0.0, 0.0], "{kind:?}");
            let out = through(&mut chorus, 48_000);
            let peak = out
                .iter()
                .flatten()
                .fold(0.0_f32, |most, s| most.max(s.abs()));
            assert!(peak > 0.3 && peak <= 0.6, "{kind:?} {peak}");
        }
    }

    #[test]
    fn depth_moves_the_copies_and_none_leaves_them_still() {
        for kind in ChorusType::ALL {
            let still = through(&mut chorus(kind, 0x00), 24_000);
            let moving = through(&mut chorus(kind, 0xFF), 24_000);
            assert_ne!(still, moving, "{kind:?}");
            // With no depth, the copies are fixed delays, so a steady sine stays the same shape.
            let peak = |out: &[[f32; 2]]| out.iter().fold(0.0_f32, |most, s| most.max(s[0].abs()));
            let (early, late) = still.split_at(still.len() / 2);
            assert!((peak(early) - peak(late)).abs() < 0.01, "{kind:?}");
        }
    }

    #[test]
    fn the_two_sides_differ_so_it_spreads() {
        for kind in ChorusType::ALL {
            let out = through(&mut chorus(kind, 0xFF), 24_000);
            let apart = out
                .iter()
                .map(|[l, r]| (l - r).abs())
                .fold(0.0_f32, f32::max);
            assert!(apart > 0.01, "{kind:?} {apart}");
        }
    }

    #[test]
    fn it_reads_as_far_back_at_any_rate() {
        for rate in [44_100, 48_000, 96_000] {
            let mut chorus = Chorus::new(rate);
            chorus.follow(Settings {
                depth: 0xFF,
                ..Settings::DEFAULT
            });
            chorus.settle();
            assert!(chorus.frames() as f32 >= LONGEST * rate as f32 / TUNING_RATE);
            for _ in 0..rate {
                let out = chorus.process([0.5, -0.5]);
                assert!(out.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            }
        }
    }

    #[test]
    fn a_wobble_stays_on_its_sine_for_minutes() {
        // Its fastest, two octaves up, for five minutes.
        let step = ENSEMBLE_SPEEDS[1] * 4.0;
        let mut wobble = Wobble::START;
        let mut worst = 0.0_f64;
        for frame in 1..=RATE * 300 {
            wobble.turn(step);
            let exact = (f64::from(step) * f64::from(frame) * std::f64::consts::TAU).sin();
            worst = worst.max((f64::from(wobble.sin) - exact).abs());
        }
        assert!(worst < 1e-3, "{worst}");
        let radius = wobble.cos.hypot(wobble.sin);
        assert!((radius - 1.0).abs() < 1e-6, "{radius}");
    }
}
