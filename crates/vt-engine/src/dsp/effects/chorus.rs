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
//!
//! The Juno's three types are this program's own, written from how the DSP parts review describes
//! a Roland Juno's chorus (decision 198): a bucket-brigade delay on each side, a few ms long,
//! swept by one triangle, the sides in antiphase; its filters as a low-pass; and a DC block. TAL-NoiseMaker's chorus is GPL 2 only, so
//! none of its code is used. Each delay is read through a first-order all-pass, as Dattorro's
//! "Effect Design" describes, so its top end doesn't dull and brighten as the sweep moves it
//! between samples.

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

/// How fast the Juno's triangle goes in I, II and I+II, in Hz, at `Rate` `80`.
const JUNO_HERTZ: [f32; 3] = [0.5, 0.83, 8.0];
/// Each Juno type's shortest delay, and how far above it the sweep reaches at `Depth` `80`, in
/// ms: I and II from 0.7 to 4.9 ms, and I+II a narrow sweep around 3.4 ms.
const JUNO_SHORTEST_MS: [f32; 3] = [0.7, 0.7, 3.2];
const JUNO_SWEEP_MS: [f32; 3] = [4.2, 4.2, 0.35];
/// Where the Juno's low-pass, standing in for its bucket brigade's filters, starts to take the
/// top off, and where its DC block is.
const JUNO_TONE_HZ: f32 = 7_000.0;
const JUNO_DC_HZ: f32 = 10.0;
/// The least of the all-pass's fraction of a frame, so its coefficient stays small and it never
/// rings at the top: the fraction runs from here to one more, the whole frames one fewer.
const LEAST_FRACTION: f32 = 0.618;

/// A triangle from -1 to 1, at a place in its turn.
#[inline]
fn triangle(turns: f32) -> f32 {
    let turns = turns - turns.floor();
    4.0 * (turns - 0.5).abs() - 1.0
}

/// A Juno type's place among its three.
fn juno_mode(kind: ChorusType) -> usize {
    match kind {
        ChorusType::JunoII => 1,
        ChorusType::JunoBoth => 2,
        _ => 0,
    }
}

/// How long a Juno type's delay is in ms, with its triangle at `wave`, from -1 to 1, and `Depth`
/// from 0 to 1: from its shortest, up as far as its sweep goes at the middle, twice as far at
/// full, and not at all at none.
fn juno_delay_ms(mode: usize, depth: f32, wave: f32) -> f32 {
    JUNO_SHORTEST_MS[mode] + JUNO_SWEEP_MS[mode] * depth * 2.0 * (wave + 1.0) * 0.5
}

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
    /// The Juno's triangle, in turns; each side's all-pass read's last output; and its low-pass
    /// and DC block, its last in and out, on each side.
    juno_phase: f32,
    juno_held: [f32; 2],
    juno_tone: [f32; 2],
    juno_dc: [[f32; 2]; 2],
    frames_per_ms: f32,
    tone_coefficient: f32,
    dc_coefficient: f32,
    seconds_per_frame: f32,
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
            juno_phase: 0.0,
            juno_held: [0.0; 2],
            juno_tone: [0.0; 2],
            juno_dc: [[0.0; 2]; 2],
            frames_per_ms: rate as f32 / 1000.0,
            tone_coefficient: 1.0 - (-TAU * JUNO_TONE_HZ / rate as f32).exp(),
            dc_coefficient: 1.0 - TAU * JUNO_DC_HZ / rate as f32,
            seconds_per_frame: 1.0 / rate as f32,
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
        self.juno_phase = 0.0;
        self.juno_held = [0.0; 2];
        self.juno_tone = [0.0; 2];
        self.juno_dc = [[0.0; 2]; 2];
    }

    /// Take what's set now. The speed is two octaves either way of Rings' or the Juno's at `80`.
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

    /// A line's sound a number of frames back, read through a first-order all-pass, which passes
    /// every frequency at the same level wherever between two samples it reads. It keeps its last
    /// output, so it's for one read of each side a frame, moving slowly.
    #[inline]
    fn read_through(&mut self, side: usize, frames: f32) -> f32 {
        let frames = frames.max(1.0 + LEAST_FRACTION);
        let whole = (frames - LEAST_FRACTION) as usize;
        let fraction = frames - whole as f32;
        // An all-pass's delay at low frequencies is `(1 - a) / (1 + a)` frames.
        let a = (1.0 - fraction) / (1.0 + fraction);
        let line = &self.lines[side];
        let newer = line[self.write.wrapping_sub(whole) & self.mask];
        let older = line[self.write.wrapping_sub(whole + 1) & self.mask];
        let out = a * (newer - self.juno_held[side]) + older;
        self.juno_held[side] = out;
        out
    }

    /// Feed one frame in and get the copies out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let speed = self.speed.next(self.smoothing);
        let depth = self.depth.next(self.smoothing);
        if self.kind.is_juno() {
            return self.juno(input, speed, depth);
        }
        self.rings(input, speed * self.slower, depth)
    }

    /// One frame through a Juno's chorus: each side down its own line, read a few ms back where
    /// the triangle puts it, the right where the left would be half a turn on, then darkened and
    /// with no DC.
    #[inline]
    fn juno(&mut self, input: [f32; 2], speed: f32, depth: f32) -> [f32; 2] {
        let mode = juno_mode(self.kind);
        self.juno_phase += JUNO_HERTZ[mode] * speed * self.seconds_per_frame;
        if self.juno_phase >= 1.0 {
            self.juno_phase -= 1.0;
        }
        self.write = (self.write + 1) & self.mask;
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            self.lines[side][self.write] = sample;
            let wave = triangle(self.juno_phase + side as f32 * 0.5);
            let read =
                self.read_through(side, juno_delay_ms(mode, depth, wave) * self.frames_per_ms);
            self.juno_tone[side] += (read - self.juno_tone[side]) * self.tone_coefficient;
            let toned = self.juno_tone[side];
            let [last_in, last_out] = self.juno_dc[side];
            let heard = toned - last_in + self.dc_coefficient * last_out;
            self.juno_dc[side] = [toned, heard];
            out[side] = heard;
        }
        out
    }

    /// One frame through Rings' chorus or ensemble.
    #[inline]
    fn rings(&mut self, input: [f32; 2], speed: f32, depth: f32) -> [f32; 2] {
        let speeds = match self.kind {
            ChorusType::Chorus => CHORUS_SPEEDS,
            _ => ENSEMBLE_SPEEDS,
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
            _ => {
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

    #[test]
    fn the_junos_sweep_where_theirs_do_with_the_sides_in_antiphase() {
        // I and II from 0.7 to 4.9 ms at the middle `Depth`, and I+II a narrow sweep.
        for (mode, shortest, longest) in [(0, 0.7, 4.9), (1, 0.7, 4.9), (2, 3.2, 3.55)] {
            assert!((juno_delay_ms(mode, 0.5, -1.0) - shortest).abs() < 1e-5);
            assert!((juno_delay_ms(mode, 0.5, 1.0) - longest).abs() < 1e-5);
            // With no depth it stays at its shortest.
            assert!((juno_delay_ms(mode, 0.0, 1.0) - shortest).abs() < 1e-5);
        }
        // Half a turn on, the triangle is turned over, so one side is longest as the other is
        // shortest.
        for at in 0..100 {
            let turns = at as f32 / 100.0;
            assert!((triangle(turns + 0.5) + triangle(turns)).abs() < 1e-5);
        }
        // Each type's triangle goes at its own rate.
        for (kind, hertz) in [
            (ChorusType::JunoI, 0.5),
            (ChorusType::JunoII, 0.83),
            (ChorusType::JunoBoth, 8.0),
        ] {
            let mut chorus = chorus(kind, 0x80);
            for _ in 0..RATE / 10 {
                chorus.process([0.0; 2]);
            }
            let turns = (hertz / 10.0_f32).fract();
            assert!((chorus.juno_phase - turns).abs() < 1e-3, "{kind:?}");
        }
    }

    #[test]
    fn a_juno_with_no_depth_holds_its_shortest_delay() {
        let mut chorus = chorus(ChorusType::JunoI, 0x00);
        let out: Vec<f32> = (0..200)
            .map(|frame| {
                let click = if frame == 0 { 1.0 } else { 0.0 };
                chorus.process([click; 2])[0]
            })
            .collect();
        let loudest = (0..out.len())
            .max_by(|a, b| out[*a].abs().total_cmp(&out[*b].abs()))
            .unwrap();
        // 0.7 ms is 33.6 frames, and the low-pass smears it a little later.
        assert!((33..=37).contains(&loudest), "{loudest}");
        assert!(out[..32].iter().all(|sample| sample.abs() < 1e-6));
    }

    #[test]
    fn the_all_pass_read_keeps_the_top_between_samples() {
        // A high sine read half a frame between samples: a straight line between them would lose
        // 3 dB of it, and the all-pass none.
        let mut chorus = Chorus::new(RATE);
        let hertz = 12_000.0;
        let (mut power, mut count) = (0.0_f32, 0);
        for frame in 0..RATE as usize / 10 {
            chorus.write = (chorus.write + 1) & chorus.mask;
            chorus.lines[0][chorus.write] = (frame as f32 / RATE as f32 * hertz * TAU).sin();
            let read = chorus.read_through(0, 40.5);
            if frame > RATE as usize / 20 {
                power += read * read;
                count += 1;
            }
        }
        // A sine of full scale has half a unit of power.
        let level = (power / count as f32 * 2.0).sqrt();
        assert!((level - 1.0).abs() < 0.01, "{level}");
    }
}
