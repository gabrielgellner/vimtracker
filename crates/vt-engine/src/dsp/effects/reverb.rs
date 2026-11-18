//! A stereo reverb: the Griesinger topology, as in Mutable Instruments' Clouds, with its line
//! lengths scaled from 32 kHz to the rate it runs at, and pre-delay and cuts on what goes in.
//!
//! `Decay` sets how long its tail takes to fall 60 dB, and each half of its loop keeps `0.001` to
//! the power of its own length over that each time round, as sst's Reverb 2 works its decay out,
//! so the rate leaves it as long. A one-pole high-pass beside each half's damping takes the lows
//! as it rings, and its sides are spread or brought together as mid and side on the way out
//! (decision 439).
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
/// How much the allpasses spread what goes through them.
const DIFFUSION: f32 = 0.625;
/// How much of what goes in reaches the lines. Tuned so its level is near the reverb before it.
const INPUT_GAIN: f32 = 0.25;
/// How much of what's in each loop comes out.
const WET: f32 = 2.0;
/// The damping filter's coefficient at the top of `Damp`, at the tuning rate. At `00` it's open.
const MOST_DAMPING: f32 = 0.1;
/// The longest pre-delay, in seconds.
const LONGEST_PRE: f32 = 0.25;
/// How fast the two slow wobbles go, in hertz.
const WOBBLE_HERTZ: [f64; 2] = [0.5, 0.3];
/// Every line's length at the tuning rate: the four diffusers, then each loop's two allpasses and
/// its delay.
const LENGTHS: [usize; 10] = [113, 162, 241, 399, 1653, 2038, 3411, 1913, 1663, 4782];
/// Where the second loop's delay is read, at the tuning rate, and how far its wobble moves it.
const SWING: [f32; 2] = [4680.0, 100.0];

/// How much longer a trip round the loop lasts than its lines, as far as its tail goes. Its
/// allpasses hold some frequencies back longer than others, and those fall slowest and last, so
/// it falls as a plain loop this much longer would (`tail`'s note). Worked out from `tail` at
/// 250 Hz, undamped, where it's the same at every `Decay` and rate.
const HELD: f32 = 1.4;

/// How long each half of the loop is at a rate, in frames, as far as its tail goes: its two
/// allpasses and its delay, as far as it's read, held back by `HELD`.
fn half_frames(rate: u32) -> [f32; 2] {
    let scale = rate as f32 / TUNING_RATE;
    let line = |at: usize| (LENGTHS[at] as f32 * scale) as usize as f32;
    [
        line(4) + line(5) + line(6) - 1.0,
        line(7) + line(8) + SWING[0] * scale,
    ]
    .map(|frames| frames * HELD)
}

/// What each half of the loop keeps each time round, for a tail that falls 60 dB in `seconds`.
pub(crate) fn kept_for(seconds: f32, frames: f32, rate: u32) -> f32 {
    0.001_f32.powf(frames / (seconds * rate as f32))
}

/// The loop's allpasses, by their places in `LENGTHS`: the first side's two, then the second's.
const LOOP_ALLPASSES: [usize; 4] = [4, 5, 7, 8];

/// The most each of the loop's allpasses may spread, for a tail that falls 60 dB in `seconds`.
/// An allpass's echoes of itself go round it whatever the loop keeps, falling by what it spreads
/// each time, so a short tail can be no shorter than they ring; spreading no more than the tail
/// keeps over its length, they ring no longer than it. Tails longer than about 0.9 s leave them at
/// `DIFFUSION`.
fn most_spread(seconds: f32, rate: u32) -> [f32; 4] {
    let scale = rate as f32 / TUNING_RATE;
    LOOP_ALLPASSES.map(|at| {
        let frames = (LENGTHS[at] as f32 * scale) as usize as f32;
        kept_for(seconds, frames, rate)
    })
}

/// How much a one-pole low-pass in a loop moves each frame for `Low damp`'s corner, whose high-pass
/// is what comes in less it, or nothing where it's off.
pub(crate) fn low_damping_at(low_damp: u8, rate: u32) -> f32 {
    low_damp_hz(low_damp).map_or(0.0, |hertz| of_corner(hertz, rate as f32))
}

/// How a reverb's tail dies away at its settings and a rate, for a drawing. Its figure of eight is
/// one loop: through each side's damping, two allpasses and a delay, kept by `Decay` on the way
/// into each side. The allpasses keep all of it but hold each frequency back by their own amount,
/// so the tail is the loop's as `tail::loop_seconds` works it out. The second loop's wobble
/// moves its read by 2%, and the smear is in a diffuser, before the loop, so neither changes it.
pub fn tail(settings: Settings, rate: u32) -> Option<Tail> {
    let scale = rate as f32 / TUNING_RATE;
    // Each line reads what was written a frame short of its length ago, and the second loop's
    // delay reads at its swing.
    let frames = |at: usize| f64::from((LENGTHS[at] as f32 * scale) as usize as f32 - 1.0);
    let seconds = reverb_decay_seconds(f32::from(settings.decay));
    let kept = half_frames(rate).map(|frames| f64::from(kept_for(seconds, frames, rate)));
    let spread = most_spread(seconds, rate).map(|most| f64::from(DIFFUSION.min(most)));
    let damping = f64::from(damping_at(settings.damp, TUNING_RATE / rate as f32));
    let low = f64::from(low_damping_at(settings.low_damp, rate));
    let swing = f64::from(SWING[0] * scale);
    let response = |omega: f64| {
        let allpass = |frames: f64, into: f64| {
            let delayed = Complex::delay(omega, frames);
            delayed
                .add(Complex(-into, 0.0))
                .over(Complex::ONE.add(delayed.scale(-into)))
        };
        let one_pole = |by: f64| {
            Complex(by, 0.0).over(Complex::ONE.add(Complex::delay(omega, 1.0).scale(by - 1.0)))
        };
        let damped = one_pole(damping);
        let thinned = Complex::ONE.add(one_pole(low).scale(-1.0));
        let one = allpass(frames(4), -spread[0])
            .times(allpass(frames(5), spread[1]))
            .times(Complex::delay(omega, frames(6)));
        let two = allpass(frames(7), spread[2])
            .times(allpass(frames(8), -spread[3]))
            .times(Complex::read(omega, swing));
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
    if width == 1.0 {
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

/// The stereo reverb.
#[derive(Debug, Clone)]
pub(crate) struct Reverb {
    lines: SharedLines,
    diffusers: [Line; 4],
    /// Each side's loop: two allpasses and a delay.
    loops: [[Line; 3]; 2],
    /// Where the smear in the first diffuser reads and writes, and how far it moves.
    smear: [f32; 3],
    /// Where the second loop's delay reads, and how far it moves.
    swing: [f32; 2],
    damped: [f32; 2],
    low_damping: LowDamping,
    wobble: [f32; 2],
    pre: Vec<f32>,
    pre_write: usize,
    pre_frames: Smoothed,
    /// What each half of the loop keeps each time round.
    decay: [Smoothed; 2],
    /// How long each half is, in frames, for working that out.
    half_frames: [f32; 2],
    /// The most each of the loop's allpasses spreads, for `Decay`, as `most_spread` gives it.
    most_spread: [Smoothed; 4],
    damping: Smoothed,
    width: Smoothed,
    /// How much the allpasses spread what goes through them, and how much of what goes in reaches
    /// the lines: the reverb's own, unless Clouds tunes them as the module does.
    diffusion: f32,
    input_gain: f32,
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
        let lines = LENGTHS.map(|length| {
            let line = Line {
                base,
                length: (length as f32 * scale) as usize,
            };
            base += line.length + 1;
            line
        });
        let size = base.next_power_of_two();
        // A power of two long, so reading round it is a mask.
        let pre_frames = ((LONGEST_PRE * rate as f32) as usize + 2).next_power_of_two();
        let mut reverb = Self {
            lines: SharedLines::new(size),
            diffusers: [lines[0], lines[1], lines[2], lines[3]],
            loops: [
                [lines[4], lines[5], lines[6]],
                [lines[7], lines[8], lines[9]],
            ],
            smear: [50.0 * scale, 100.0 * scale, 45.0 * scale],
            swing: SWING.map(|frames| frames * scale),
            damped: [0.0; 2],
            low_damping: LowDamping::default(),
            wobble: [1.0; 2],
            pre: vec![0.0; pre_frames],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            decay: [Smoothed::at(0.0); 2],
            half_frames: half_frames(rate),
            most_spread: [Smoothed::at(1.0); 4],
            damping: Smoothed::at(0.0),
            width: Smoothed::at(1.0),
            diffusion: DIFFUSION,
            input_gain: INPUT_GAIN,
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
        for (decay, frames) in self.decay.iter_mut().zip(self.half_frames) {
            decay.target = kept_for(seconds, frames, self.rate);
        }
        for (most, target) in self
            .most_spread
            .iter_mut()
            .zip(most_spread(seconds, self.rate))
        {
            most.target = target;
        }
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
        self.wobble =
            WOBBLE_HERTZ.map(|hertz| (std::f64::consts::TAU * hertz * seconds).cos() as f32);
    }

    /// Jump to the settings, for starting afresh.
    pub(crate) fn settle(&mut self) {
        self.pre_frames.settle();
        for decay in &mut self.decay {
            decay.settle();
        }
        for most in &mut self.most_spread {
            most.settle();
        }
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

    /// Tune the tank as Clouds tunes its own reverb, block by block: what each trip round its loops
    /// keeps, its damping as a one-pole's coefficient at 32 kHz, its allpasses' diffusion and how
    /// much of what goes in reaches the lines. The decay and damping glide there as `Decay` and
    /// `Damp` do.
    pub(crate) fn tune(
        &mut self,
        decay: f32,
        damping_at_tuning_rate: f32,
        diffusion: f32,
        gain: f32,
    ) {
        for half in &mut self.decay {
            half.target = decay;
        }
        // Clouds spreads as it does whatever its decay, at once.
        for most in &mut self.most_spread {
            *most = Smoothed::at(1.0);
        }
        self.damping.target = 1.0 - (1.0 - damping_at_tuning_rate).powf(self.rate_ratio);
        self.diffusion = diffusion;
        self.input_gain = gain;
    }

    /// Feed one frame into the tank alone, with no cuts or pre-delay, as Clouds' reverb has none,
    /// and get the wet signal out.
    #[inline]
    pub(crate) fn process_tank(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.tank(input[0] + input[1])
    }

    /// Both sides together through the diffusers and the tank's two loops.
    #[inline]
    fn tank(&mut self, incoming: f32) -> [f32; 2] {
        let diffusion = self.diffusion;
        let [one, two] = &mut self.decay;
        let decay = [one.next(self.smoothing), two.next(self.smoothing)];
        let damping = self.damping.next(self.smoothing);
        let low_damping = self.low_damping.next(self.smoothing);
        let spread = self
            .most_spread
            .each_mut()
            .map(|most| most.next(self.smoothing));
        let lines = &mut self.lines;
        lines.advance();

        // Smear the first allpass inside itself. In Clouds this reads below 0, out of the line and
        // into old memory, which would go round again whatever `Decay` is, so here it stays in.
        let [first, _, _, _] = self.diffusers;
        let [read_at, write_at, moves] = self.smear;
        let smeared = lines.between(first.base, read_at + moves * self.wobble[0]);
        lines.write(first.base, write_at as usize, smeared);

        let mut diffused = incoming * self.input_gain;
        for line in self.diffusers {
            diffused = lines.allpass(line.base, line.length, diffused, diffusion);
        }

        let [[one_a, one_b, one_delay], [two_a, two_b, two_delay]] = self.loops;
        let [swing_at, swing_moves] = self.swing;
        let from_two = lines.between(two_delay.base, swing_at + swing_moves * self.wobble[1]);
        let mut left = diffused + from_two * decay[1];
        self.damped[0] += damping * (left - self.damped[0]);
        left = self.damped[0];
        if let Some(by) = low_damping {
            left = self.low_damping.thin(0, left, by);
        }
        left = lines.allpass(one_a.base, one_a.length, left, -diffusion.min(spread[0]));
        left = lines.allpass(one_b.base, one_b.length, left, diffusion.min(spread[1]));
        lines.write(one_delay.base, 0, left);

        let from_one = lines.read(one_delay.base, one_delay.length - 1);
        let mut right = diffused + from_one * decay[0];
        self.damped[1] += damping * (right - self.damped[1]);
        right = self.damped[1];
        if let Some(by) = low_damping {
            right = self.low_damping.thin(1, right, by);
        }
        right = lines.allpass(two_a.base, two_a.length, right, diffusion.min(spread[2]));
        right = lines.allpass(two_b.base, two_b.length, right, -diffusion.min(spread[3]));
        lines.write(two_delay.base, 0, right);

        [left * WET, right * WET]
    }
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
        // go round, so long tails fall a little faster than worked out, and the diffusers spread
        // a click's start over tens of milliseconds, so short dark ones a little slower. Seen:
        // within 9%, and 15% for highs darkened hard.
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
    fn decay_is_how_long_its_tail_takes_to_fall_60_db_at_any_rate() {
        // Undamped, under its damping. The diffusers before the loop ring for about a fifth of a
        // second themselves, so the shortest tails are a little longer; the wobbles mix the modes
        // as they go round, so the longest a little shorter. Seen: within 16%.
        for (decay, rate) in [
            (0x00, 48_000),
            (0x20, 48_000),
            (0x40, 48_000),
            (0x60, 48_000),
            (0x80, 48_000),
            (0xA0, 48_000),
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
                250.0,
            );
            assert!(
                (measured / set - 1.0).abs() < 0.2,
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
        assert!(mono.iter().all(|[l, r]| l == r));
        assert!(wide.iter().any(|[l, r]| (l - r).abs() > 0.01));
        for ((w, h), m) in wide.iter().zip(&half).zip(&mono) {
            let sums = [w, h, m].map(|f| f[0] + f[1]);
            assert!((sums[0] - sums[1]).abs() < 1e-5 && (sums[0] - sums[2]).abs() < 1e-5);
        }
    }
}
