//! A stereo reverb: the Griesinger topology Mutable Instruments' Clouds has, a run of diffusers
//! and a figure of eight of two allpasses and a delay a side, with its line lengths scaled from
//! 32 kHz to the rate it runs at, and pre-delay and cuts on what goes in.
//!
//! It has gone its own way from Clouds' tank, which Clouds keeps as the module has it in
//! `clouds/reverb.rs` (decision 447). Its loops are 1.3 times as long, so more modes share the
//! work, and spread less, so no frequency rings on far longer than its neighbours; each half's
//! delay and first allpass wobble, slowly and out of step, so no mode stands still; six
//! diffusers spread what goes in rather than four; and each side of what comes out is five taps
//! along both loops' delays, as Dattorro's plate takes its own, rather than where one side's
//! allpasses end, so the start fills in at once and differs side to side.
//!
//! `Decay` sets how long its tail takes to fall 60 dB at 1 kHz. Each half of its loop keeps
//! `0.001` to the power of its own length over that each time round, as sst's Reverb 2 works its
//! decay out, so the rate leaves it as long, with its length counted as `held` says. A one-pole
//! high-pass beside each half's damping takes the lows as it rings, and its sides are spread or
//! brought together as mid and side on the way out (decision 439).
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

use vt_model::insert::{Reverb as Settings, low_damp_hz, reverb_decay_seconds};

use crate::dsp::blocks::coefficient::of_corner;
use crate::dsp::blocks::line::SharedLines;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::{REVERB_QUIET_SECONDS, SongPlace};
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::tail::{self, Complex, TAIL_HZ, Tail};
use crate::dsp::filters::filter::{Coefficients, Trim};

/// The rate the line lengths and damping are written for.
const TUNING_RATE: f32 = 32_000.0;
/// The diffusers' lengths at the tuning rate: Clouds' four, and two longer, so a hit is spread
/// over about 70 ms before the loops take it.
const DIFFUSERS: [f32; 6] = [113.0, 162.0, 241.0, 399.0, 577.0, 769.0];
/// How much the diffusers spread what goes through them.
const INPUT_DIFFUSION: f32 = 0.625;
/// Each half of the loop at the tuning rate: its two allpasses, and where its delay is read,
/// Clouds' lengths 1.3 times over.
const HALVES: [[f32; 3]; 2] = [[2149.0, 2649.0, 4434.0], [2487.0, 2162.0, 6084.0]];
/// How much the loop's allpasses spread. Clouds' 0.625 leaves the frequencies an allpass holds
/// back most ringing four times as long as those it holds back least, which stood out as modes.
const LOOP_DIFFUSION: f32 = 0.5;
/// How far each half's first allpass and its delay's read wobble either way, at the tuning rate.
const ALLPASS_WOBBLE: f32 = 30.0;
const DELAY_WOBBLE: f32 = 20.0;
/// How fast the three slow wobbles go, in hertz: the smear in the first diffuser's, the first
/// half's delay's, which the second half's first allpass shares, and the second half's delay's,
/// which the first half's first allpass shares.
const WOBBLE_HERTZ: [f64; 3] = [0.5, 0.37, 0.3];
/// How much of what goes in reaches the lines.
const INPUT_GAIN: f32 = 0.25;
/// What each tap is scaled by. Tuned so its level is near the reverb's before it, decision 439's.
const TAP_GAIN: f32 = 0.9;
/// The damping filter's coefficient at the top of `Damp`, at the tuning rate. At `00` it's open.
const MOST_DAMPING: f32 = 0.1;
/// The longest pre-delay, in seconds.
const LONGEST_PRE: f32 = 0.25;

/// Where a tap reads: which half's delay, how far along it as a share of where it's read, and
/// whether it's added or taken away.
type Tap = (usize, f32, f32);

/// The taps, the left side's and then the right's: one near the start of the other half's delay,
/// so something comes out at once, and the rest spread along both. They were picked from many by
/// a model of the first 80 ms, for sides about as alike as a hall's early on (IACC 0.15), with few
/// taps on one delay close enough together to comb what comes out.
const TAPS: [[Tap; 5]; 2] = [
    [
        (1, 0.028, 1.0),
        (1, 0.289, 1.0),
        (1, 0.854, -1.0),
        (0, 0.249, 1.0),
        (0, 0.864, -1.0),
    ],
    [
        (0, 0.023, 1.0),
        (0, 0.352, 1.0),
        (0, 0.94, 1.0),
        (1, 0.159, -1.0),
        (1, 0.551, -1.0),
    ],
];

/// How many times faster than the tail the diffusers and the loop's allpasses may ring by
/// themselves: an allpass's echoes of itself go round it whatever the loop keeps, falling by what
/// it spreads each time, and at short tails they're most of what's heard, several of them one
/// after another, so each rings out in two thirds of the tail.
const RING_FASTER: f32 = 1.5;

/// How much longer each half of the loop lasts than its lines, as far as its tail goes, for a
/// tail of `seconds`. Its allpasses hold some frequencies back longer than others, and those fall
/// slowest and last (decision 439), but the wobbles mix them as they go round, so a tail of a few
/// seconds or more falls at the loop's own length. About a second, the mix is still young and the
/// loop's tail is the most of what's heard, so it counts up to 1.28 times as long; at the
/// shortest tails the allpasses' own ringing is what's heard, as `RING_FASTER` sets it. Measured
/// at 1 kHz against a click, within 4% from `00` to `C0` (decision 447).
fn held(seconds: f32) -> f32 {
    let octaves = seconds.ln() / 0.43;
    1.0 + 0.28 / (1.0 + octaves * octaves)
}

/// How long each half of the loop is at a rate, in frames, as far as its tail goes: its two
/// allpasses and its delay, as far as it's read, held back by `held`.
fn half_frames(seconds: f32, rate: u32) -> [f32; 2] {
    let scale = rate as f32 / TUNING_RATE;
    HALVES.map(|[a, b, read]| {
        ((a * scale) as usize as f32 + (b * scale) as usize as f32 + read * scale) * held(seconds)
    })
}

/// What each half of the loop keeps each time round, for a tail that falls 60 dB in `seconds`.
pub(crate) fn kept_for(seconds: f32, frames: f32, rate: u32) -> f32 {
    0.001_f32.powf(frames / (seconds * rate as f32))
}

/// The most an allpass this long at the tuning rate may spread for a tail of `seconds`: so it
/// rings by itself `RING_FASTER` times faster than the tail falls. Tails longer than about 1.5 s
/// leave every one as it's set.
fn most_spread(length: f32, seconds: f32, rate: u32) -> f32 {
    let frames = (length * rate as f32 / TUNING_RATE) as usize as f32;
    kept_for(seconds / RING_FASTER, frames, rate)
}

/// How much each of the loop's allpasses spreads for a tail of `seconds`.
fn loop_spread(seconds: f32, rate: u32) -> [f32; 4] {
    let [[a, b, _], [c, d, _]] = HALVES;
    [a, b, c, d].map(|length| LOOP_DIFFUSION.min(most_spread(length, seconds, rate)))
}

/// How much a one-pole low-pass in a loop moves each frame for `Low damp`'s corner, whose high-pass
/// is what comes in less it, or nothing where it's off.
pub(crate) fn low_damping_at(low_damp: u8, rate: u32) -> f32 {
    low_damp_hz(low_damp).map_or(0.0, |hertz| of_corner(hertz, rate as f32))
}

/// How a reverb's tail dies away at its settings and a rate, for a drawing. Its figure of eight is
/// one loop: through each side's damping, two allpasses and a delay, kept by `Decay` on the way
/// into each side. The allpasses keep all of it but hold each frequency back by their own amount,
/// so the tail is the loop's as `tail::loop_seconds` works it out. The wobbles move the reads by
/// under 2%, which leaves the loop as long, but reading between samples takes a little of the
/// highs each time round, which it counts; the smear is in a diffuser, before the loop.
pub fn tail(settings: Settings, rate: u32) -> Option<Tail> {
    let scale = rate as f32 / TUNING_RATE;
    // Each allpass reads what was written a frame short of its length ago.
    let frames = |length: f32| f64::from((length * scale) as usize as f32 - 1.0);
    let seconds = reverb_decay_seconds(f32::from(settings.decay));
    let kept = half_frames(seconds, rate).map(|frames| f64::from(kept_for(seconds, frames, rate)));
    let spread = loop_spread(seconds, rate).map(f64::from);
    let damping = f64::from(damping_at(settings.damp, TUNING_RATE / rate as f32));
    let low = f64::from(low_damping_at(settings.low_damp, rate));
    let [[a, b, one_read], [c, d, two_read]] = HALVES;
    let response = |omega: f64| {
        // A read that wobbles lands anywhere between two samples, and read in a straight line
        // between them it keeps `1 - (1 - cos ω) / 3` of a frequency's power on average.
        let moving = (1.0 - (1.0 - omega.cos()) / 3.0).sqrt();
        let allpass = |frames: f64, into: f64, kept: f64| {
            let delayed = Complex::delay(omega, frames).scale(kept);
            delayed
                .add(Complex(-into, 0.0))
                .over(Complex::ONE.add(delayed.scale(-into)))
        };
        let one_pole = |by: f64| {
            Complex(by, 0.0).over(Complex::ONE.add(Complex::delay(omega, 1.0).scale(by - 1.0)))
        };
        let damped = one_pole(damping);
        let thinned = Complex::ONE.add(one_pole(low).scale(-1.0));
        let one = allpass(frames(a), -spread[0], moving)
            .times(allpass(frames(b), spread[1], 1.0))
            .times(Complex::delay(omega, f64::from(one_read * scale)).scale(moving));
        let two = allpass(frames(c), spread[2], moving)
            .times(allpass(frames(d), -spread[3], 1.0))
            .times(Complex::delay(omega, f64::from(two_read * scale)).scale(moving));
        one.times(two)
            .times(damped)
            .times(damped)
            .times(thinned)
            .times(thinned)
            .scale(kept[0] * kept[1])
    };
    let seconds = TAIL_HZ
        .map(|hertz| tail::loop_seconds(hertz, rate, |omega| tail::kept_and_held(omega, response)));
    Some(Tail {
        pre: f32::from(settings.pre) / 255.0 * LONGEST_PRE,
        seconds,
    })
}

/// The damping one-pole's coefficient at a rate, for `Damp`, from its coefficient at the tuning
/// rate.
fn damping_at(damp: u8, rate_ratio: f32) -> f32 {
    let at_tuning_rate = 1.0 - f32::from(damp) / 255.0 * (1.0 - MOST_DAMPING);
    1.0 - (1.0 - at_tuning_rate).powf(rate_ratio)
}

/// The high-pass beside a loop's damping, `Low damp`: a one-pole low-pass taken from what comes
/// in. Off, it lets everything through as it is; turned off, it glides there, and once it's too
/// slow to move anything it's left out and forgets what it held.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LowDamping {
    by: Smoothed,
    held: [f32; 2],
}

impl LowDamping {
    /// So slow a low-pass that it moves nothing a listener would hear, where it's left out.
    const STILL: f32 = 1e-6;

    pub(crate) fn clear(&mut self) {
        self.held = [0.0; 2];
    }

    pub(crate) fn set(&mut self, by: f32) {
        self.by.target = by;
    }

    pub(crate) fn settle(&mut self) {
        self.by.settle();
    }

    /// This frame's coefficient, or nothing while it's off.
    #[inline]
    pub(crate) fn next(&mut self, smoothing: f32) -> Option<f32> {
        if self.by.target == 0.0 && self.by.value < Self::STILL {
            if self.by.value != 0.0 {
                self.by.value = 0.0;
                self.held = [0.0; 2];
            }
            return None;
        }
        Some(self.by.next(smoothing))
    }

    /// One frame of a side through it.
    #[inline]
    pub(crate) fn thin(&mut self, side: usize, sample: f32, by: f32) -> f32 {
        self.held[side] += (sample - self.held[side]) * by;
        sample - self.held[side]
    }
}

/// Mid and side on the way out: the side scaled by `Width`, which leaves both sides as they are at
/// its top.
#[inline]
pub(crate) fn widen([left, right]: [f32; 2], width: f32) -> [f32; 2] {
    if width.to_bits() == 1.0_f32.to_bits() {
        return [left, right];
    }
    let mid = (left + right) * 0.5;
    let side = (left - right) * 0.5 * width;
    [mid + side, mid - side]
}

/// One delay line's place in the shared buffer.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    length: usize,
}

/// A tap at a rate: where its line starts, how many frames back it reads, and its sign.
#[derive(Debug, Clone, Copy)]
struct Reading {
    base: usize,
    back: usize,
    sign: f32,
}

/// The stereo reverb.
#[derive(Debug, Clone)]
pub(crate) struct Reverb {
    lines: SharedLines,
    diffusers: [Line; 6],
    /// Each half of the loop: two allpasses and a delay.
    loops: [[Line; 3]; 2],
    taps: [[Reading; 5]; 2],
    /// Where the smear in the first diffuser reads and writes, and how far it moves.
    smear: [f32; 3],
    /// Where each half's first allpass reads, and how far it moves.
    allpass_wobble: [[f32; 2]; 2],
    /// Where each half's delay is read, and how far it moves.
    swing: [[f32; 2]; 2],
    damped: [f32; 2],
    low_damping: LowDamping,
    /// The three wobbles, each a point going round a circle, and how far each turns a frame.
    wobble: [(f32, f32); 3],
    turn: [(f32, f32); 3],
    pre: Vec<f32>,
    pre_write: usize,
    pre_frames: Smoothed,
    /// What each half of the loop keeps each time round.
    decay: [Smoothed; 2],
    /// How much each of the loop's allpasses spreads, for `Decay`, as `loop_spread` gives it.
    loop_spread: [Smoothed; 4],
    /// How much the diffusers spread, for `Decay`: what the longest may.
    input_spread: Smoothed,
    damping: Smoothed,
    width: Smoothed,
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
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let mut base = 0;
        let mut line = |frames: f32| {
            let line = Line {
                base,
                length: (frames * scale) as usize,
            };
            base += line.length + 1;
            line
        };
        let diffusers = DIFFUSERS.map(&mut line);
        // Room for the wobbles either way, and the frame after them to read between.
        let room = 3.0 / scale;
        let loops = HALVES.map(|[a, b, read]| {
            [
                line(a + ALLPASS_WOBBLE + room),
                line(b),
                line(read + DELAY_WOBBLE + room),
            ]
        });
        let size = base.next_power_of_two();
        let taps = TAPS.map(|side| {
            side.map(|(half, along, sign)| Reading {
                base: loops[half][2].base,
                back: ((HALVES[half][2] * along * scale) as usize).max(1),
                sign,
            })
        });
        // A power of two long, so reading round it is a mask.
        let pre_frames = ((LONGEST_PRE * rate as f32) as usize + 2).next_power_of_two();
        let mut reverb = Self {
            lines: SharedLines::new(size),
            diffusers,
            loops,
            taps,
            smear: [50.0 * scale, 100.0 * scale, 22.5 * scale],
            // Each allpass reads what was written a frame short of its length ago.
            allpass_wobble: HALVES.map(|[a, ..]| [a * scale - 1.0, ALLPASS_WOBBLE * scale]),
            swing: HALVES.map(|[.., read]| [read * scale, DELAY_WOBBLE * scale]),
            damped: [0.0; 2],
            low_damping: LowDamping::default(),
            wobble: [(1.0, 0.0); 3],
            turn: WOBBLE_HERTZ.map(|hertz| {
                let turning = std::f64::consts::TAU * hertz / f64::from(rate);
                (turning.cos() as f32, turning.sin() as f32)
            }),
            pre: vec![0.0; pre_frames],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            decay: [Smoothed::at(0.0); 2],
            loop_spread: [Smoothed::at(0.0); 4],
            input_spread: Smoothed::at(0.0),
            damping: Smoothed::at(0.0),
            width: Smoothed::at(1.0),
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
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.pre.fill(0.0);
        self.damped = [0.0; 2];
        self.low_damping.clear();
        self.trim = Trim::default();
    }

    /// Take the settings, and move the wobbles to where they are this many frames in. They go by
    /// the frame rather than by what the reverb has done, so one that's slept is where it would
    /// have been.
    pub(crate) fn follow(&mut self, settings: Settings, frames: u64) {
        self.keep_time(frames);
        self.pre_frames.target = f32::from(settings.pre) / 255.0 * LONGEST_PRE * self.rate as f32;
        let seconds = reverb_decay_seconds(f32::from(settings.decay));
        for (decay, frames) in self.decay.iter_mut().zip(half_frames(seconds, self.rate)) {
            decay.target = kept_for(seconds, frames, self.rate);
        }
        for (spread, target) in self
            .loop_spread
            .iter_mut()
            .zip(loop_spread(seconds, self.rate))
        {
            spread.target = target;
        }
        self.input_spread.target =
            INPUT_DIFFUSION.min(most_spread(DIFFUSERS[5], seconds, self.rate));
        self.damping.target = damping_at(settings.damp, self.rate_ratio);
        self.low_damping
            .set(low_damping_at(settings.low_damp, self.rate));
        self.width.target = f32::from(settings.width) / 255.0;
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

    /// Move the wobbles to where they are this many frames in.
    pub(crate) fn keep_time(&mut self, frames: u64) {
        let seconds = frames as f64 / f64::from(self.rate);
        self.wobble = WOBBLE_HERTZ.map(|hertz| {
            let turns = seconds * hertz;
            let angle = std::f64::consts::TAU * (turns - turns.floor());
            (angle.cos() as f32, angle.sin() as f32)
        });
    }

    /// Jump to the settings, for starting afresh.
    pub(crate) fn settle(&mut self) {
        self.pre_frames.settle();
        for decay in &mut self.decay {
            decay.settle();
        }
        for spread in &mut self.loop_spread {
            spread.settle();
        }
        self.input_spread.settle();
        self.damping.settle();
        self.low_damping.settle();
        self.width.settle();
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
        let mask = self.pre.len() - 1;
        self.pre[self.pre_write] = sample;
        let back = self.pre_frames.next(self.smoothing);
        let whole = back as usize;
        let fraction = back - whole as f32;
        let a = self.pre[self.pre_write.wrapping_sub(whole) & mask];
        let b = self.pre[self.pre_write.wrapping_sub(whole + 1) & mask];
        self.pre_write = (self.pre_write + 1) & mask;
        if fraction == 0.0 {
            a
        } else {
            a + (b - a) * fraction
        }
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let incoming = self.before_lines(input);
        let wet = self.tank(incoming);
        widen(wet, self.width.next(self.smoothing))
    }

    /// What goes in through the diffusers and the loop's two halves, and the taps along them.
    #[inline]
    fn tank(&mut self, incoming: f32) -> [f32; 2] {
        let [one, two] = &mut self.decay;
        let decay = [one.next(self.smoothing), two.next(self.smoothing)];
        let damping = self.damping.next(self.smoothing);
        let low_damping = self.low_damping.next(self.smoothing);
        let spread = self
            .loop_spread
            .each_mut()
            .map(|spread| spread.next(self.smoothing));
        let input_spread = self.input_spread.next(self.smoothing);
        for ((cos, sin), (by_cos, by_sin)) in self.wobble.iter_mut().zip(self.turn) {
            (*cos, *sin) = (*cos * by_cos - *sin * by_sin, *sin * by_cos + *cos * by_sin);
        }
        let wobble = self.wobble.map(|(cos, _)| cos);
        let lines = &mut self.lines;
        lines.advance();

        // Smear the first diffuser inside itself, staying in its line, as Clouds' reverb does.
        let [first, ..] = self.diffusers;
        let [read_at, write_at, moves] = self.smear;
        let smeared = lines.between(first.base, read_at + moves * wobble[0]);
        lines.write(first.base, write_at as usize, smeared);

        let mut diffused = incoming * INPUT_GAIN;
        for line in self.diffusers {
            diffused = lines.allpass(line.base, line.length, diffused, input_spread);
        }

        let [[one_a, one_b, one_delay], [two_a, two_b, two_delay]] = self.loops;
        let [[one_wobble_at, one_wobble], [two_wobble_at, two_wobble]] = self.allpass_wobble;
        let [[one_at, one_moves], [two_at, two_moves]] = self.swing;
        let from_two = lines.between(two_delay.base, two_at + two_moves * wobble[2]);
        let mut left = diffused + from_two * decay[1];
        self.damped[0] += damping * (left - self.damped[0]);
        left = self.damped[0];
        if let Some(by) = low_damping {
            left = self.low_damping.thin(0, left, by);
        }
        let at = one_wobble_at + one_wobble * wobble[2];
        left = moving_allpass(lines, one_a.base, at, left, -spread[0]);
        left = lines.allpass(one_b.base, one_b.length, left, spread[1]);
        lines.write(one_delay.base, 0, left);

        let from_one = lines.between(one_delay.base, one_at + one_moves * wobble[1]);
        let mut right = diffused + from_one * decay[0];
        self.damped[1] += damping * (right - self.damped[1]);
        right = self.damped[1];
        if let Some(by) = low_damping {
            right = self.low_damping.thin(1, right, by);
        }
        let at = two_wobble_at + two_wobble * wobble[1];
        right = moving_allpass(lines, two_a.base, at, right, spread[2]);
        right = lines.allpass(two_b.base, two_b.length, right, -spread[3]);
        lines.write(two_delay.base, 0, right);

        self.taps.map(|side| {
            side.iter()
                .map(|tap| lines.read(tap.base, tap.back) * tap.sign)
                .sum::<f32>()
                * TAP_GAIN
        })
    }
}

/// A step through an allpass whose read moves, `at` frames back, read between samples.
#[inline]
fn moving_allpass(lines: &mut SharedLines, base: usize, at: f32, input: f32, into: f32) -> f32 {
    let read = lines.between(base, at);
    let sum = input + read * into;
    lines.write(base, 0, sum);
    sum * -into + read
}

impl SlotEffect for Reverb {
    type Settings = Settings;

    /// A reverb's.
    fn new(rate: u32) -> Self {
        Self::new(rate)
    }

    fn follow(&mut self, settings: Settings, _song: SongPlace, frames: u64) {
        self.follow(settings, frames);
    }

    fn settle(&mut self) {
        self.settle();
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn quiet_frames(&self, rate: u32) -> u32 {
        (REVERB_QUIET_SECONDS * rate as f32) as u32
    }

    fn keep_time(&mut self, frames: u64, _song: SongPlace) {
        self.keep_time(frames);
    }
}

impl Rung for Reverb {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::random::NumericalRecipesLcg;

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
        let mut rng = NumericalRecipesLcg::new(1);
        (0..frames)
            .map(|at| {
                let sample = (rng.next_u32() >> 16) as f32 / 32_768.0 - 1.0;
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

    #[test]
    fn its_tail_falls_as_fast_as_a_click_through_it_measures() {
        // Each frequency's modes fall at their own rate, and the tail is what a band of them
        // measures, so it's within the measurement's spread: the wobbles mix the modes as they
        // go round, which `held` counts only for 1 kHz, so the lows of tails of a few seconds
        // fall a little faster than worked out, and highs darkened hard a little slower. Seen:
        // within 11%.
        use crate::dsp::effects::tail::measure;
        let cases = [
            (0x57, 0x00, 48_000),
            (0x76, 0x00, 48_000),
            (0x9D, 0x00, 48_000),
            (0x76, 0x80, 48_000),
            (0x9D, 0xFF, 48_000),
            (0xBD, 0x40, 48_000),
            (0x76, 0x40, 44_100),
            (0x76, 0x40, 96_000),
        ];
        for (decay, damp, rate) in cases {
            let settings = Settings {
                decay,
                damp,
                ..settings(decay)
            };
            let said = tail(settings, rate).unwrap();
            let mut reverb = reverb(settings, rate);
            let ratios = measure::ratios(said, |x| reverb.process([x, x]), rate);
            for (ratio, hertz) in ratios.iter().zip(TAIL_HZ) {
                assert!(
                    (ratio - 1.0).abs() < 0.18,
                    "{decay:02X} {damp:02X} at {rate}, {hertz} Hz: {said:?} measured {ratio}"
                );
            }
        }
        // Darker damping takes the highs sooner, and leaves the lows.
        let [open, dark] = [0x00, 0xFF].map(|damp| {
            tail(
                Settings {
                    damp,
                    ..settings(0x9D)
                },
                RATE,
            )
            .unwrap()
            .seconds
        });
        assert!(dark[1] < open[1] * 0.5 && dark[0] > open[0] * 0.7);
    }

    #[test]
    fn its_tail_starts_after_its_pre_delay() {
        use crate::dsp::effects::tail::measure;
        for pre in [0x00, 0x80, 0xFF] {
            let settings = Settings {
                pre,
                ..Settings::DEFAULT
            };
            let said = tail(settings, RATE).unwrap();
            let mut reverb = reverb(settings, RATE);
            let response: Vec<f32> = (0..RATE as usize / 2)
                .map(|at| reverb.process([if at == 0 { 1.0 } else { 0.0 }; 2])[0])
                .collect();
            // At once, through the diffusers and the loops, to a frame, as it reads between them.
            let heard = measure::onset(&response, RATE);
            assert!(
                heard >= said.pre - 1.0 / RATE as f32 && heard < said.pre + 0.05,
                "{pre:02X}: {heard} against {}",
                said.pre
            );
        }
    }

    #[test]
    fn decay_is_how_long_its_tail_takes_to_fall_60_db_at_1_khz_at_any_rate() {
        // Undamped, an octave around 1 kHz, as `held` was measured (decision 447). Seen: within
        // 4% at 48 kHz.
        for (decay, rate) in [
            (0x00, 48_000),
            (0x20, 48_000),
            (0x40, 48_000),
            (0x60, 48_000),
            (0x80, 48_000),
            (0xC0, 48_000),
            (0x60, 44_100),
            (0x60, 96_000),
        ] {
            let settings = Settings {
                damp: 0,
                ..settings(decay)
            };
            let set = reverb_decay_seconds(f32::from(decay));
            let mut reverb = reverb(settings, rate);
            let measured = crate::dsp::effects::tail::measure::click_seconds(
                |x| reverb.process(x),
                set,
                rate,
                1000.0,
            );
            assert!(
                (measured / set - 1.0).abs() < 0.1,
                "{decay:02X} at {rate}: {measured} for {set}"
            );
        }
    }

    #[test]
    fn low_damp_takes_the_lows_from_the_tail_and_leaves_the_rest() {
        let open = Settings {
            damp: 0,
            ..settings(0x80)
        };
        let thin = Settings {
            low_damp: 0x80,
            ..open
        };
        let at = |settings, hertz| {
            let mut reverb = reverb(settings, RATE);
            crate::dsp::effects::tail::measure::click_seconds(
                |x| reverb.process(x),
                4.0,
                RATE,
                hertz,
            )
        };
        // At about 140 Hz an octave around 100 Hz falls in under two thirds the time; seen: 0.60.
        let (thin_low, open_low) = (at(thin, 100.0), at(open, 100.0));
        assert!(thin_low < open_low * 0.7, "{thin_low} {open_low}");
        assert!(at(thin, 2000.0) > at(open, 2000.0) * 0.85);
        // Worked out as it measures.
        let said = tail(thin, RATE).unwrap();
        let mut reverb = reverb(thin, RATE);
        let ratios =
            crate::dsp::effects::tail::measure::ratios(said, |x| reverb.process([x, x]), RATE);
        assert!(
            ratios.iter().all(|ratio| (ratio - 1.0).abs() < 0.18),
            "{ratios:?}"
        );
    }

    #[test]
    fn low_damp_turned_off_is_left_out() {
        let mut reverb = reverb(
            Settings {
                low_damp: 0xC0,
                ..Settings::DEFAULT
            },
            RATE,
        );
        burst(&mut reverb, RATE as usize / 2);
        assert!(reverb.low_damping.next(reverb.smoothing).is_some());
        reverb.follow(Settings::DEFAULT, 0);
        burst(&mut reverb, RATE as usize);
        assert!(reverb.low_damping.next(reverb.smoothing).is_none());
        assert_eq!(reverb.low_damping.held, [0.0; 2]);
    }

    /// A click through it at its defaults, both sides.
    fn click(frames: usize) -> Vec<[f32; 2]> {
        let mut reverb = reverb(Settings::DEFAULT, RATE);
        (0..frames)
            .map(|at| reverb.process([if at == 0 { 1.0 } else { 0.0 }; 2]))
            .collect()
    }

    #[test]
    fn its_sides_differ_from_the_start() {
        // How alike its sides are over the first 80 ms, at their most alike within a millisecond
        // either way, as IACC is measured: a hall's are about 0.15 to 0.2, and this tank's were
        // 0.52 when each side came from one place in it. Seen: 0.15.
        let out = click(RATE as usize / 2);
        let start = out
            .iter()
            .position(|[l, r]| l.abs() + r.abs() > 1e-4)
            .unwrap();
        let early = &out[start..start + RATE as usize * 80 / 1000];
        let energy = |side: usize| early.iter().map(|f| f[side] * f[side]).sum::<f32>();
        let norm = (energy(0) * energy(1)).sqrt();
        let lag = RATE as isize / 1000;
        let most = (-lag..=lag)
            .map(|by| {
                let mut sum = 0.0;
                for at in 0..early.len() as isize {
                    let other = at + by;
                    if (0..early.len() as isize).contains(&other) {
                        sum += early[at as usize][0] * early[other as usize][1];
                    }
                }
                (sum / norm).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(most < 0.25, "{most}");
    }

    #[test]
    fn a_click_comes_out_spread_with_no_echo_standing_out() {
        // A snare through it shouldn't flam: over its first 350 ms no 3 ms stands out from the
        // 50 ms around it. Seen: 1.1 dB at most, where one tap a side gave 3.3 dB.
        let out = click(RATE as usize / 2);
        let energy: Vec<f32> = out.iter().map(|[l, r]| l * l + r * r).collect();
        let start = energy.iter().position(|e| *e > 1e-8).unwrap();
        let mean = |at: usize, frames: usize| {
            energy[at - frames / 2..at + frames / 2].iter().sum::<f32>() / frames as f32
        };
        let (short, long) = (RATE as usize * 3 / 1000, RATE as usize / 20);
        let most = (start + RATE as usize / 40..start + RATE as usize * 35 / 100)
            .map(|at| 10.0 * (mean(at, short) / mean(at, long)).log10())
            .fold(f32::MIN, f32::max);
        assert!(most < 2.0, "{most} dB");
    }

    #[test]
    fn width_brings_its_sides_together_and_leaves_their_sum() {
        let out = |width| {
            let settings = Settings {
                width,
                ..Settings::DEFAULT
            };
            burst(&mut reverb(settings, RATE), RATE as usize)
        };
        let (wide, half, mono) = (out(0xFF), out(0x80), out(0x00));
        assert!(mono.iter().all(|[l, r]| l.to_bits() == r.to_bits()));
        assert!(wide.iter().any(|[l, r]| (l - r).abs() > 0.01));
        for ((w, h), m) in wide.iter().zip(&half).zip(&mono) {
            let sums = [w, h, m].map(|f| f[0] + f[1]);
            assert!((sums[0] - sums[1]).abs() < 1e-5 && (sums[0] - sums[2]).abs() < 1e-5);
        }
    }
}
