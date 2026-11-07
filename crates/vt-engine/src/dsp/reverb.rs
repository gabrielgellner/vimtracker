//! A stereo reverb: the Griesinger topology, as in Mutable Instruments' Clouds, with its line
//! lengths scaled from 32 kHz to the rate it runs at, and pre-delay and cuts on what goes in.
//!
//! The core is adapted from `clouds/dsp/fx/reverb.h` and `fx_engine.h`:
//!
//! Copyright 2014 Emilie Gillet.
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

use vt_model::insert::Reverb as Settings;

use crate::dsp::filter::{Coefficients, Trim};
use crate::dsp::smoother::{Smoothed, coefficient};

/// The rate the line lengths and damping are written for.
const TUNING_RATE: f32 = 32_000.0;
/// How much the allpasses spread what goes through them.
const DIFFUSION: f32 = 0.625;
/// How much of what goes in reaches the lines. Tuned so its level is near the reverb before it.
const INPUT_GAIN: f32 = 0.25;
/// How much of what's in each loop comes out.
const WET: f32 = 2.0;
/// What each trip round a loop keeps at the top of `Decay`, just short of going on for ever.
const MOST_DECAY: f32 = 0.98;
/// The damping filter's coefficient at the top of `Damp`, at the tuning rate. At `00` it's open.
const MOST_DAMPING: f32 = 0.1;
/// The longest pre-delay, in seconds.
const LONGEST_PRE: f32 = 0.25;
/// How fast the two slow wobbles go, in hertz.
const WOBBLE_HERTZ: [f64; 2] = [0.5, 0.3];

/// One delay line's place in the shared buffer.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    length: usize,
}

/// The buffer all the lines share, each with its own place, moving together.
#[derive(Debug, Clone)]
struct Lines {
    buffer: Vec<f32>,
    mask: usize,
    write: usize,
}

impl Lines {
    #[inline]
    fn read(&self, line: Line, offset: usize) -> f32 {
        self.buffer[(self.write + line.base + offset) & self.mask]
    }

    #[inline]
    fn tail(&self, line: Line) -> f32 {
        self.read(line, line.length - 1)
    }

    #[inline]
    fn write(&mut self, line: Line, offset: usize, value: f32) {
        let at = (self.write + line.base + offset) & self.mask;
        self.buffer[at] = value;
    }

    /// Read between samples.
    #[inline]
    fn between(&self, line: Line, offset: f32) -> f32 {
        let whole = offset as usize;
        let fraction = offset - whole as f32;
        let a = self.read(line, whole);
        let b = self.read(line, whole + 1);
        a + (b - a) * fraction
    }

    /// A step through an allpass, going by what it read from the end of its line.
    #[inline]
    fn allpass(&mut self, line: Line, input: f32, into: f32) -> f32 {
        let read = self.tail(line);
        let sum = input + read * into;
        self.write(line, 0, sum);
        sum * -into + read
    }
}

/// The stereo reverb.
#[derive(Debug, Clone)]
pub struct Reverb {
    lines: Lines,
    diffusers: [Line; 4],
    /// Each side's loop: two allpasses and a delay.
    loops: [[Line; 3]; 2],
    /// Where the smear in the first diffuser reads and writes, and how far it moves.
    smear: [f32; 3],
    /// Where the second loop's delay reads, and how far it moves.
    swing: [f32; 2],
    damped: [f32; 2],
    wobble: [f32; 2],
    pre: Vec<f32>,
    pre_write: usize,
    pre_frames: Smoothed,
    decay: Smoothed,
    damping: Smoothed,
    smoothing: f32,
    /// What damping at the tuning rate takes to be the same at this one.
    rate_ratio: f32,
    trim: Trim,
    low_cut: Option<(u8, Coefficients)>,
    high_cut: Option<(u8, Coefficients)>,
    rate: u32,
}

impl Reverb {
    /// Make a reverb for a sample rate.
    pub fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let lengths = [113, 162, 241, 399, 1653, 2038, 3411, 1913, 1663, 4782];
        let mut base = 0;
        let lines = lengths.map(|length| {
            let line = Line {
                base,
                length: (length as f32 * scale) as usize,
            };
            base += line.length + 1;
            line
        });
        let size = base.next_power_of_two();
        let pre_frames = (LONGEST_PRE * rate as f32) as usize + 2;
        let mut reverb = Self {
            lines: Lines {
                buffer: vec![0.0; size],
                mask: size - 1,
                write: 0,
            },
            diffusers: [lines[0], lines[1], lines[2], lines[3]],
            loops: [
                [lines[4], lines[5], lines[6]],
                [lines[7], lines[8], lines[9]],
            ],
            smear: [50.0 * scale, 100.0 * scale, 45.0 * scale],
            swing: [4680.0 * scale, 100.0 * scale],
            damped: [0.0; 2],
            wobble: [1.0; 2],
            pre: vec![0.0; pre_frames],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            decay: Smoothed::at(0.0),
            damping: Smoothed::at(0.0),
            smoothing: coefficient(rate),
            rate_ratio: TUNING_RATE / rate as f32,
            trim: Trim::default(),
            low_cut: None,
            high_cut: None,
            rate,
        };
        reverb.follow(Settings::DEFAULT, 0);
        reverb.settle();
        reverb
    }

    /// Forget what's in it.
    pub fn clear(&mut self) {
        self.lines.buffer.fill(0.0);
        self.pre.fill(0.0);
        self.damped = [0.0; 2];
        self.trim = Trim::default();
    }

    /// Take the settings, and move the wobbles to where they are this many frames in. They go by
    /// the frame rather than by what the reverb has done, so one that's slept is where it would
    /// have been.
    pub fn follow(&mut self, settings: Settings, frames: u64) {
        let seconds = frames as f64 / f64::from(self.rate);
        self.wobble =
            WOBBLE_HERTZ.map(|hertz| (std::f64::consts::TAU * hertz * seconds).cos() as f32);
        self.pre_frames.target = f32::from(settings.pre) / 255.0 * LONGEST_PRE * self.rate as f32;
        self.decay.target = f32::from(settings.decay) / 255.0 * MOST_DECAY;
        let at_tuning_rate = 1.0 - f32::from(settings.damp) / 255.0 * (1.0 - MOST_DAMPING);
        self.damping.target = 1.0 - (1.0 - at_tuning_rate).powf(self.rate_ratio);
        let rate = self.rate;
        let update = |slot: &mut Option<(u8, Coefficients)>, value: u8, on: bool| {
            if !on {
                *slot = None;
            } else if slot.as_ref().is_none_or(|(at, _)| *at != value) {
                *slot = Some((
                    value,
                    Coefficients::new(f32::from(value) / 255.0, 0.0, rate),
                ));
            }
        };
        update(&mut self.low_cut, settings.low_cut, settings.low_cut > 0);
        update(
            &mut self.high_cut,
            settings.high_cut,
            settings.high_cut < 0xFF,
        );
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.pre_frames.settle();
        self.decay.settle();
        self.damping.settle();
    }

    /// What goes in: both sides together, through the cuts and the pre-delay.
    #[inline]
    fn before_lines(&mut self, input: [f32; 2]) -> f32 {
        let mut sample = input[0] + input[1];
        if let Some((_, c)) = &self.low_cut {
            sample = self.trim.low_cut(sample, c);
        }
        if let Some((_, c)) = &self.high_cut {
            sample = self.trim.high_cut(sample, c);
        }
        let length = self.pre.len();
        self.pre[self.pre_write] = sample;
        let back = self.pre_frames.next(self.smoothing);
        let whole = back as usize;
        let fraction = back - whole as f32;
        let a = self.pre[(self.pre_write + length - whole) % length];
        let b = self.pre[(self.pre_write + length - whole - 1) % length];
        self.pre_write = (self.pre_write + 1) % length;
        if fraction == 0.0 {
            a
        } else {
            a + (b - a) * fraction
        }
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let incoming = self.before_lines(input);
        let decay = self.decay.next(self.smoothing);
        let damping = self.damping.next(self.smoothing);
        let lines = &mut self.lines;
        lines.write = lines.write.wrapping_sub(1) & lines.mask;

        // Smear the first allpass inside itself. In Clouds this reads below 0, out of the line and
        // into old memory, which would go round again whatever `Decay` is, so here it stays in.
        let [first, _, _, _] = self.diffusers;
        let [read_at, write_at, moves] = self.smear;
        let smeared = lines.between(first, read_at + moves * self.wobble[0]);
        lines.write(first, write_at as usize, smeared);

        let mut diffused = incoming * INPUT_GAIN;
        for line in self.diffusers {
            diffused = lines.allpass(line, diffused, DIFFUSION);
        }

        let [[one_a, one_b, one_delay], [two_a, two_b, two_delay]] = self.loops;
        let [swing_at, swing_moves] = self.swing;
        let from_two = lines.between(two_delay, swing_at + swing_moves * self.wobble[1]);
        let mut left = diffused + from_two * decay;
        self.damped[0] += damping * (left - self.damped[0]);
        left = self.damped[0];
        left = lines.allpass(one_a, left, -DIFFUSION);
        left = lines.allpass(one_b, left, DIFFUSION);
        lines.write(one_delay, 0, left);

        let from_one = lines.tail(one_delay);
        let mut right = diffused + from_one * decay;
        self.damped[1] += damping * (right - self.damped[1]);
        right = self.damped[1];
        right = lines.allpass(two_a, right, DIFFUSION);
        right = lines.allpass(two_b, right, -DIFFUSION);
        lines.write(two_delay, 0, right);

        [left * WET, right * WET]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn reverb(settings: Settings, rate: u32) -> Reverb {
        let mut reverb = Reverb::new(rate);
        reverb.follow(settings, 0);
        reverb.settle();
        reverb
    }

    fn settings(decay: u8) -> Settings {
        Settings {
            decay,
            ..Settings::DEFAULT
        }
    }

    /// A short burst of something broad, and then what comes out until this many frames.
    fn burst(reverb: &mut Reverb, frames: usize) -> Vec<[f32; 2]> {
        let mut noise = 1_u32;
        (0..frames)
            .map(|at| {
                noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let sample = (noise >> 16) as f32 / 32_768.0 - 1.0;
                let level = if at < 480 { sample * 0.5 } else { 0.0 };
                reverb.process([level, level])
            })
            .collect()
    }

    /// How long until it's stayed 60 dB below its loudest, in seconds, from slices of 50 ms.
    fn decay_seconds(out: &[[f32; 2]], rate: u32) -> f32 {
        let slice = rate as usize / 20;
        let energies: Vec<f32> = out
            .chunks(slice)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect();
        let loudest = energies.iter().fold(0.0_f32, |most, e| most.max(*e));
        let last = energies
            .iter()
            .rposition(|e| *e > loudest * 1e-6)
            .unwrap_or(0);
        (last + 1) as f32 * 0.05
    }

    #[test]
    fn decay_makes_its_tail_longer_and_stays_stable_at_the_top() {
        let time = |decay| {
            decay_seconds(
                &burst(&mut reverb(settings(decay), RATE), RATE as usize * 30),
                RATE,
            )
        };
        let (short, middle) = (time(0x40), time(0x80));
        assert!(short > 0.1 && short < 2.0, "{short}");
        assert!(middle > short * 1.5, "{short} {middle}");
        let top = burst(&mut reverb(settings(0xFF), RATE), RATE as usize * 30);
        assert!(top.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        let peak = |part: &[[f32; 2]]| part.iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
        let (start, end) = (&top[..RATE as usize], &top[RATE as usize * 29..]);
        assert!(peak(end) < peak(start), "{} {}", peak(end), peak(start));
        assert!(peak(end) > 0.0, "still going");
    }

    #[test]
    fn its_decay_is_about_the_same_at_any_rate() {
        let time = |rate| {
            decay_seconds(
                &burst(&mut reverb(settings(0x80), rate), rate as usize * 12),
                rate,
            )
        };
        let (low, mid, high) = (time(44_100), time(48_000), time(96_000));
        assert!((low / mid - 1.0).abs() < 0.2, "{low} {mid}");
        assert!((high / mid - 1.0).abs() < 0.2, "{high} {mid}");
    }

    #[test]
    fn pre_delay_holds_back_what_comes_out() {
        let first = |pre| {
            let settings = Settings {
                pre,
                ..Settings::DEFAULT
            };
            let out = burst(&mut reverb(settings, RATE), RATE as usize);
            out.iter().position(|f| f[0] != 0.0 || f[1] != 0.0).unwrap()
        };
        let (none, most) = (first(0), first(0xFF));
        let held = (most - none) as f32 / RATE as f32;
        assert!((held - LONGEST_PRE).abs() < 0.002, "{held}");
    }

    /// How much of a tone comes out while it plays and rings.
    fn energy(settings: Settings, hertz: f32) -> f32 {
        let mut reverb = reverb(settings, RATE);
        let mut total = 0.0;
        for at in 0..RATE as usize {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            let input = if at < RATE as usize / 4 { tone } else { 0.0 };
            let out = reverb.process([input, input]);
            total += out[0] * out[0] + out[1] * out[1];
        }
        total
    }

    #[test]
    fn damp_takes_highs_from_the_tail() {
        let open = Settings {
            damp: 0,
            ..settings(0xC0)
        };
        let dark = Settings { damp: 0xFF, ..open };
        assert!(energy(dark, 6000.0) < energy(open, 6000.0) * 0.1);
    }

    #[test]
    fn the_cuts_shape_what_goes_in() {
        let open = Settings::DEFAULT;
        let high_cut = Settings {
            high_cut: 0x80,
            ..open
        };
        assert!(energy(high_cut, 8000.0) < energy(open, 8000.0) * 0.1);
        assert!(energy(high_cut, 200.0) > energy(open, 200.0) * 0.5);
        let low_cut = Settings {
            low_cut: 0x80,
            ..open
        };
        assert!(energy(low_cut, 60.0) < energy(open, 60.0) * 0.1);
        assert!(energy(low_cut, 8000.0) > energy(open, 8000.0) * 0.5);
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut reverb = reverb(Settings::DEFAULT, RATE);
        burst(&mut reverb, 1000);
        reverb.clear();
        for _ in 0..RATE {
            assert_eq!(reverb.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
