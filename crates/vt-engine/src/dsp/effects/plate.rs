//! A stereo plate: Jon Dattorro's, from "Effect Design, Part 1: Reverberator and Other Filters"
//! (J. Audio Eng. Soc., 1997). What goes in is band-limited, held back and spread by four
//! all-passes, then goes round a tank of two halves that feed each other, each a wobbling
//! all-pass, a delay, a damping low-pass, a second all-pass and a second delay. Each side of what
//! comes out is seven taps from all over the tank, as the paper gives them.
//!
//! The paper's lengths are for 29761 Hz, and are scaled here to the rate it runs at. `Size` scales
//! the tank's lengths and taps again, read between samples, and moves no faster than its whole
//! range in a second, so a change bends the tail's pitch a little rather than clicking. Its lines
//! share one buffer from the effect pool, long enough for the biggest plate at the pool's rate.
//!
//! `Decay` sets how long its tail takes to fall 60 dB, and each half of the tank keeps `0.001` to
//! the power of its own length over that each time round, so neither `Size` nor the rate changes
//! it. Beside each half's damping a one-pole high-pass takes the lows as it rings, and its sides
//! are spread or brought together as mid and side on the way out, as the reverb's (decision 439).
//!
//! Ported from el-visio's `verb.c` at <https://github.com/el-visio/dattorro-verb>, MIT, and checked
//! against the paper. That port steps its wobble one sample at a time and adds the last tap of
//! each side, where the paper takes it away; here the wobble is a sine read between samples, and
//! the taps' signs are the paper's.
//!
//! Copyright (c) 2022 Pauli Pölkki.
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

use vt_model::insert::{Plate as Settings, plate_size, reverb_decay_seconds};

use crate::dsp::blocks::coefficient::of_corner;
use crate::dsp::blocks::line::SharedLines;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::{REVERB_QUIET_SECONDS, SongPlace};
use crate::dsp::effects::reverb::{LowDamping, low_damping_at, widen};
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::tail::{self, Complex, TAIL_HZ, Tail};

/// The rate the paper's lengths are written for.
const TUNING_RATE: f32 = 29_761.0;
/// The input all-passes' lengths, and how much each spreads what goes through it.
const DIFFUSER_LENGTHS: [f32; 4] = [142.0, 107.0, 379.0, 277.0];
const DIFFUSIONS: [f32; 4] = [0.75, 0.75, 0.625, 0.625];
/// Each half of the tank: its wobbling all-pass, its delay before the damping, its second all-pass
/// and its delay after.
const TANK_LENGTHS: [[f32; 4]; 2] = [
    [672.0, 4453.0, 1800.0, 3720.0],
    [908.0, 4217.0, 2656.0, 3163.0],
];
/// How much the tank's first all-passes spread, taken away rather than added as the paper has it.
const DECAY_DIFFUSION: f32 = 0.7;
/// The most and least the second all-passes spread, which goes with `Decay`, as the paper has it.
const SECOND_DIFFUSION: (f32, f32) = (0.25, 0.5);
/// The damping filter's coefficient at the top of `Damp`, at the tuning rate, as the reverb's. At
/// `00` it's open.
const MOST_DAMPING: f32 = 0.1;
/// The longest pre-delay, in seconds, as the reverb's.
const LONGEST_PRE: f32 = 0.25;
/// The lowest `High cut`, in hertz, and how many times higher it goes, as the filters' cutoff.
const LOWEST_CUT_HZ: f32 = 20.0;
const CUT_SPAN: f32 = 1000.0;
/// How far the first all-passes wobble at the top of `Mod`, at the tuning rate. The paper's 16 is
/// at `80`.
const MOST_EXCURSION: f32 = 32.0;
/// How fast they wobble. The two halves are a quarter of a turn apart.
const WOBBLE_HZ: f64 = 1.0;
/// The biggest plate, as so many times the paper's.
const LARGEST: f32 = 2.0;
/// How far `Size` moves in a second at most, as so many times the paper's: its whole range. So
/// the lines' lengths change by at most about a quarter of a frame each frame, and the pitch of
/// what's in them bends by no more than a quarter as it moves.
const SIZE_PER_SECOND: f32 = 1.5;
/// How far the pre-delay moves in a frame at most, in frames, for the same reason: across its
/// whole range in a second.
const PRE_PER_FRAME: f32 = LONGEST_PRE;
/// What each tap is scaled by, as the paper has it.
const TAP_GAIN: f32 = 0.6;
/// How loud what comes out is. Tuned so it's near the reverb's at both their defaults.
const WET: f32 = 0.7;

/// How much a tail that falls 60 dB in `seconds` falls each frame, as the natural log of what it
/// keeps.
fn fall_per_frame(seconds: f32, rate: u32) -> f32 {
    0.001_f32.ln() / (seconds * rate as f32)
}

/// How much longer a trip round the tank lasts than its lines, as far as its tail goes, as the
/// reverb's `HELD`: from `tail` at 250 Hz, undamped, within 2% at every `Size` and `Decay` but
/// the shortest tails of the biggest plate, which are over in a trip or two.
const HELD: f32 = 1.09;

/// What `Decay` makes of the tank, falling `fall` a frame with its lines stretched by `stretch`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Keeping {
    /// What each of a half's two decays keeps: half what the whole half keeps, in dB, as a trip
    /// through it takes as long as its lines, held back by `HELD`.
    kept: [f32; 2],
    /// How much each half's first and second all-passes spread. An all-pass's echoes of itself go
    /// round it whatever the tank keeps, so each spreads no more than the tail keeps over its
    /// length, and none rings on past the tail. The second's goes with `Decay` as the paper has
    /// it, from what the halves keep.
    spread: [[f32; 2]; 2],
}

impl Keeping {
    fn new(fall: f32, stretch: f32) -> Self {
        let kept =
            TANK_LENGTHS.map(|half| (fall * half.iter().sum::<f32>() * HELD * stretch * 0.5).exp());
        let second =
            ((kept[0] + kept[1]) * 0.5 + 0.15).clamp(SECOND_DIFFUSION.0, SECOND_DIFFUSION.1);
        let most = |length: f32| (fall * length * stretch).exp();
        let spread = TANK_LENGTHS.map(|[first, _, after_damping, _]| {
            [
                DECAY_DIFFUSION.min(most(first)),
                second.min(most(after_damping)),
            ]
        });
        Self { kept, spread }
    }
}

/// How a plate's tail dies away at its settings and a rate, for a drawing. Its tank's two halves
/// are one loop, kept by `Decay` twice in each half, darkened and thinned once, through four
/// all-passes that keep all of it but hold each frequency back by their own amount, so the tail
/// is the loop's as `tail::loop_seconds` works it out. The first all-passes' wobble moves them by
/// a few frames in hundreds, so it's left out.
pub fn tail(settings: Settings, rate: u32) -> Option<Tail> {
    let stretch = plate_size(f32::from(settings.size)) * rate as f32 / TUNING_RATE;
    let fall = fall_per_frame(reverb_decay_seconds(f32::from(settings.decay)), rate);
    let keeping = Keeping::new(fall, stretch);
    let damping = f64::from(damping_at(settings.damp, rate));
    let low = f64::from(low_damping_at(settings.low_damp, rate));
    let stretch = f64::from(stretch);
    let response = |omega: f64| {
        let allpass = |frames: f64, diffusion: f32| {
            let diffusion = f64::from(diffusion);
            let delayed = Complex::read(omega, frames);
            delayed
                .add(Complex(diffusion, 0.0))
                .over(Complex::ONE.add(delayed.scale(diffusion)))
        };
        let one_pole = |by: f64| {
            Complex(by, 0.0).over(Complex::ONE.add(Complex::delay(omega, 1.0).scale(by - 1.0)))
        };
        let damped = one_pole(damping);
        let thinned = Complex::ONE.add(one_pole(low).scale(-1.0));
        (0..2).fold(Complex::ONE, |round, half| {
            let [first, before, after_damping, after] =
                TANK_LENGTHS[half].map(|length| f64::from(length) * stretch);
            let [first_spread, second_spread] = keeping.spread[half];
            let kept = f64::from(keeping.kept[half]);
            round
                .times(allpass(first, -first_spread))
                .times(Complex::read(omega, before))
                .times(Complex::read(omega, after))
                .times(damped)
                .times(thinned)
                .times(allpass(after_damping, second_spread))
                .scale(kept * kept)
        })
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
fn damping_at(damp: u8, rate: u32) -> f32 {
    let at_tuning_rate = 1.0 - f32::from(damp) / 255.0 * (1.0 - MOST_DAMPING);
    // Over the scale every length is stretched by, so it rounds as it always has.
    1.0 - (1.0 - at_tuning_rate).powf(1.0 / (rate as f32 / TUNING_RATE))
}

/// Where in the tank a tap reads: which half, which of its four lines, how far back at the tuning
/// rate, and whether it's added or taken away.
type Tap = (usize, usize, f32, f32);

/// The paper's taps: the left side's, then the right's.
const TAPS: [[Tap; 7]; 2] = [
    [
        (1, 1, 266.0, 1.0),
        (1, 1, 2974.0, 1.0),
        (1, 2, 1913.0, -1.0),
        (1, 3, 1996.0, 1.0),
        (0, 1, 1990.0, -1.0),
        (0, 2, 187.0, -1.0),
        (0, 3, 1066.0, -1.0),
    ],
    [
        (0, 1, 353.0, 1.0),
        (0, 1, 3627.0, 1.0),
        (0, 2, 1228.0, -1.0),
        (0, 3, 2673.0, 1.0),
        (1, 1, 2111.0, -1.0),
        (1, 2, 335.0, -1.0),
        (1, 3, 121.0, -1.0),
    ],
];

/// One delay line's place in the shared buffer: where it starts, and how long it is at the tuning
/// rate and the paper's size.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    length: f32,
}

/// A step through an all-pass this many frames long, read between samples, in the shared lines.
#[inline]
fn allpass(lines: &mut SharedLines, line: Line, frames: f32, input: f32, diffusion: f32) -> f32 {
    let delayed = lines.between(line.base, frames);
    let sum = input - delayed * diffusion;
    lines.write(line.base, 0, sum);
    delayed + sum * diffusion
}

/// The stereo plate.
#[derive(Debug, Clone)]
pub(crate) struct Plate {
    lines: SharedLines,
    diffusers: [Line; 4],
    /// How long each input all-pass is at this rate, in frames.
    diffuser_frames: [f32; 4],
    tank: [[Line; 4]; 2],
    /// The band-limiting low-pass on what goes in, and each half's damping.
    band: f32,
    damped: [f32; 2],
    /// The wobble, as a point going round a circle, and how far it turns each frame.
    wobble: (f32, f32),
    turn: (f32, f32),
    pre: Vec<f32>,
    pre_write: usize,
    pre_frames: Smoothed,
    /// How far back the pre-delay is read now, which follows `pre_frames` no faster than it may
    /// move.
    pre_now: f32,
    /// How much the tail falls each frame, as the natural log of what it keeps.
    fall: Smoothed,
    /// What the tank keeps and spreads, and the fall and stretch it was worked out for, as they
    /// seldom move.
    keeping: Keeping,
    keeping_for: (f32, f32),
    damping: Smoothed,
    /// How much of what goes in the band-limiting low-pass lets through each frame.
    bandwidth: Smoothed,
    size: Smoothed,
    /// The size the lines are read at now, which follows `size` no faster than it may move.
    size_now: f32,
    excursion: Smoothed,
    low_damping: LowDamping,
    width: Smoothed,
    smoothing: f32,
    /// This rate over the tuning rate, which every length is scaled by.
    scale: f32,
    rate: u32,
}

impl Plate {
    /// Make a plate for a sample rate, with lines long enough for the biggest.
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let mut base = 0;
        let mut place = |longest: f32| {
            let line = Line { base, length: 0.0 };
            // Room to read between samples past the end.
            base += (longest * scale).ceil() as usize + 3;
            line
        };
        let diffusers = DIFFUSER_LENGTHS.map(|length| Line {
            length,
            ..place(length)
        });
        let tank = TANK_LENGTHS.map(|half| {
            let mut first = true;
            half.map(|length| {
                let wobble = if first { MOST_EXCURSION } else { 0.0 };
                first = false;
                Line {
                    length,
                    ..place(length * LARGEST + wobble)
                }
            })
        });
        let size = base.next_power_of_two();
        let pre_frames = (LONGEST_PRE * rate as f32) as usize + 2;
        let turning = std::f64::consts::TAU * WOBBLE_HZ / f64::from(rate);
        let mut plate = Self {
            lines: SharedLines::new(size),
            diffusers,
            diffuser_frames: DIFFUSER_LENGTHS.map(|length| (length * scale).round()),
            tank,
            band: 0.0,
            damped: [0.0; 2],
            wobble: (1.0, 0.0),
            turn: (turning.cos() as f32, turning.sin() as f32),
            pre: vec![0.0; pre_frames],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            pre_now: 0.0,
            fall: Smoothed::at(0.0),
            keeping: Keeping::new(0.0, 1.0),
            keeping_for: (f32::NAN, f32::NAN),
            damping: Smoothed::at(1.0),
            bandwidth: Smoothed::at(1.0),
            size: Smoothed::at(1.0),
            size_now: 1.0,
            excursion: Smoothed::at(0.0),
            low_damping: LowDamping::default(),
            width: Smoothed::at(1.0),
            smoothing: coefficient(rate),
            scale,
            rate,
        };
        plate.follow(Settings::DEFAULT, 0);
        plate.settle();
        plate
    }

    /// Forget what's in it.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.pre.fill(0.0);
        self.band = 0.0;
        self.damped = [0.0; 2];
        self.low_damping.clear();
    }

    /// How many frames its buffer holds, for the tests.
    #[cfg(test)]
    fn frames(&self) -> usize {
        self.lines.len()
    }

    /// Take the settings, and move the wobble to where it is this many frames in. It goes by the
    /// frame rather than by what the plate has done, so one that's slept is where it would have
    /// been.
    pub(crate) fn follow(&mut self, settings: Settings, frames: u64) {
        self.keep_time(frames);
        self.pre_frames.target = f32::from(settings.pre) / 255.0 * LONGEST_PRE * self.rate as f32;
        self.fall.target =
            fall_per_frame(reverb_decay_seconds(f32::from(settings.decay)), self.rate);
        self.damping.target = damping_at(settings.damp, self.rate);
        self.bandwidth.target = if settings.high_cut == 0xFF {
            1.0
        } else {
            let hertz = LOWEST_CUT_HZ * CUT_SPAN.powf(f32::from(settings.high_cut) / 255.0);
            of_corner(hertz, self.rate as f32)
        };
        self.size.target = plate_size(f32::from(settings.size));
        self.excursion.target = f32::from(settings.excursion) / 255.0 * MOST_EXCURSION * self.scale;
        self.low_damping
            .set(low_damping_at(settings.low_damp, self.rate));
        self.width.target = f32::from(settings.width) / 255.0;
    }

    /// Move the wobble to where it is this many frames in.
    pub(crate) fn keep_time(&mut self, frames: u64) {
        let turns = frames as f64 / f64::from(self.rate) * WOBBLE_HZ;
        let angle = std::f64::consts::TAU * (turns - turns.floor());
        self.wobble = (angle.cos() as f32, angle.sin() as f32);
    }

    /// Jump to the settings, for starting afresh.
    pub(crate) fn settle(&mut self) {
        self.pre_frames.settle();
        self.pre_now = self.pre_frames.value;
        self.fall.settle();
        self.damping.settle();
        self.bandwidth.settle();
        self.size.settle();
        self.size_now = self.size.value;
        self.excursion.settle();
        self.low_damping.settle();
        self.width.settle();
    }

    /// What goes in: both sides together, through the pre-delay and the band-limiting low-pass.
    #[inline]
    fn before_lines(&mut self, input: [f32; 2]) -> f32 {
        let length = self.pre.len();
        self.pre[self.pre_write] = (input[0] + input[1]) * 0.5;
        let wanted = self.pre_frames.next(self.smoothing);
        self.pre_now += (wanted - self.pre_now).clamp(-PRE_PER_FRAME, PRE_PER_FRAME);
        let back = self.pre_now;
        let whole = back as usize;
        let fraction = back - whole as f32;
        let a = self.pre[(self.pre_write + length - whole) % length];
        let b = self.pre[(self.pre_write + length - whole - 1) % length];
        self.pre_write = (self.pre_write + 1) % length;
        let held = if fraction == 0.0 {
            a
        } else {
            a + (b - a) * fraction
        };
        let bandwidth = self.bandwidth.next(self.smoothing);
        self.band += (held - self.band) * bandwidth;
        self.band
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let incoming = self.before_lines(input);
        let fall = self.fall.next(self.smoothing);
        let damping = self.damping.next(self.smoothing);
        let excursion = self.excursion.next(self.smoothing);
        let wanted = self.size.next(self.smoothing);
        let most = SIZE_PER_SECOND / self.rate as f32;
        self.size_now += (wanted - self.size_now).clamp(-most, most);
        let stretch = self.size_now * self.scale;
        if (fall, stretch) != self.keeping_for {
            self.keeping = Keeping::new(fall, stretch);
            self.keeping_for = (fall, stretch);
        }
        let keeping = self.keeping;
        let low_damping = self.low_damping.next(self.smoothing);
        let width = self.width.next(self.smoothing);
        let (cos, sin) = self.wobble;
        self.wobble = (
            cos * self.turn.0 - sin * self.turn.1,
            sin * self.turn.0 + cos * self.turn.1,
        );

        let lines = &mut self.lines;
        lines.advance();
        let mut diffused = incoming;
        for ((line, frames), diffusion) in self
            .diffusers
            .iter()
            .zip(self.diffuser_frames)
            .zip(DIFFUSIONS)
        {
            diffused = allpass(lines, *line, frames, diffused, diffusion);
        }

        // Each half takes what the other's last delay gives, so it all goes round.
        let wobbles = [cos, sin];
        let mut ends = [0.0; 2];
        for (half, end) in ends.iter_mut().enumerate() {
            let [_, _, _, other_end] = self.tank[1 - half];
            *end = lines.between(other_end.base, other_end.length * stretch);
        }
        for half in 0..2 {
            let [first, before, second, after] = self.tank[half];
            // Each half keeps its share twice: once after its damping, and once on its way
            // into the other half.
            let mut sample = diffused + ends[1 - half] * keeping.kept[1 - half];
            let [first_spread, second_spread] = keeping.spread[half];
            let wobbled = first.length * stretch + excursion * wobbles[half];
            sample = allpass(lines, first, wobbled, sample, -first_spread);
            lines.write(before.base, 0, sample);
            sample = lines.between(before.base, before.length * stretch);
            self.damped[half] += (sample - self.damped[half]) * damping;
            sample = self.damped[half];
            if let Some(by) = low_damping {
                sample = self.low_damping.thin(half, sample, by);
            }
            sample *= keeping.kept[half];
            sample = allpass(
                lines,
                second,
                second.length * stretch,
                sample,
                second_spread,
            );
            lines.write(after.base, 0, sample);
        }

        let mut out = [0.0; 2];
        for (side, taps) in out.iter_mut().zip(&TAPS) {
            *side = taps
                .iter()
                .map(|&(half, line, frames, sign)| {
                    lines.between(self.tank[half][line].base, frames * stretch) * sign
                })
                .sum::<f32>()
                * TAP_GAIN
                * WET;
        }
        widen(out, width)
    }
}

impl SlotEffect for Plate {
    type Settings = Settings;

    /// A plate's, long enough for the biggest.
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

impl Rung for Plate {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn plate(settings: Settings, rate: u32) -> Plate {
        let mut plate = Plate::new(rate);
        plate.follow(settings, 0);
        plate.settle();
        plate
    }

    fn settings(decay: u8) -> Settings {
        Settings {
            decay,
            ..Settings::DEFAULT
        }
    }

    /// A single frame at full scale, and then what comes out until this many frames.
    fn impulse(plate: &mut Plate, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| {
                let level = if at == 0 { 1.0 } else { 0.0 };
                plate.process([level, level])
            })
            .collect()
    }

    /// How much comes out in each slice of 50 ms.
    fn energies(out: &[[f32; 2]], rate: u32) -> Vec<f32> {
        out.chunks(rate as usize / 20)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect()
    }

    /// How long until it's stayed 60 dB below its loudest, in seconds.
    fn decay_seconds(out: &[[f32; 2]], rate: u32) -> f32 {
        let energies = energies(out, rate);
        let loudest = energies.iter().fold(0.0_f32, |most, e| most.max(*e));
        let last = energies
            .iter()
            .rposition(|e| *e > loudest * 1e-6)
            .unwrap_or(0);
        (last + 1) as f32 * 0.05
    }

    #[test]
    fn an_impulse_gives_a_dense_tail_that_dies_away() {
        let out = impulse(&mut plate(Settings::DEFAULT, RATE), RATE as usize * 6);
        // Dense: once it's built up, most frames have something in them, not a few echoes.
        let tail = &out[RATE as usize / 10..RATE as usize / 2];
        let loud = tail.iter().fold(0.0_f32, |most, f| most.max(f[0].abs()));
        let busy = tail.iter().filter(|f| f[0].abs() > loud * 0.01).count();
        assert!(busy > tail.len() * 3 / 4, "{busy} of {}", tail.len());
        // And it dies away.
        let energies = energies(&out, RATE);
        let (first, last) = (energies[2], energies[energies.len() - 1]);
        assert!(last < first * 1e-4, "{first} {last}");
    }

    #[test]
    fn decay_makes_its_tail_longer_and_still_dies_away_at_the_top() {
        let time = |decay| {
            decay_seconds(
                &impulse(&mut plate(settings(decay), RATE), RATE as usize * 20),
                RATE,
            )
        };
        let (short, middle, long) = (time(0x40), time(0x80), time(0xC0));
        assert!(short > 0.2 && short < 3.0, "{short}");
        assert!(middle > short * 1.5, "{short} {middle}");
        assert!(long > middle * 1.5, "{middle} {long}");
        // At the top it rings for a long time, but each second it's quieter than the last.
        let top = impulse(&mut plate(settings(0xFF), RATE), RATE as usize * 20);
        assert!(top.iter().flatten().all(|s| s.is_finite()));
        let seconds: Vec<f32> = top
            .chunks(RATE as usize)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect();
        for pair in seconds[1..].windows(2) {
            assert!(pair[1] < pair[0], "{seconds:?}");
        }
        assert!(seconds[19] > seconds[1] * 1e-4, "it's long: {seconds:?}");
        assert!(seconds[19] < seconds[1] * 0.5, "it dies away: {seconds:?}");
    }

    #[test]
    fn its_decay_is_about_the_same_at_any_rate() {
        let time = |rate| {
            decay_seconds(
                &impulse(&mut plate(settings(0x80), rate), rate as usize * 8),
                rate,
            )
        };
        let (low, mid, high) = (time(44_100), time(48_000), time(96_000));
        assert!((low / mid - 1.0).abs() < 0.2, "{low} {mid}");
        assert!((high / mid - 1.0).abs() < 0.2, "{high} {mid}");
    }

    /// How much of a tone comes out while it plays and rings.
    fn energy(settings: Settings, hertz: f32) -> f32 {
        let mut plate = plate(settings, RATE);
        let mut total = 0.0;
        for at in 0..RATE as usize * 2 {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            let input = if at < RATE as usize / 4 { tone } else { 0.0 };
            let out = plate.process([input, input]);
            // Only the tail, once what went in has stopped.
            if at > RATE as usize / 2 {
                total += out[0] * out[0] + out[1] * out[1];
            }
        }
        total
    }

    #[test]
    fn damp_darkens_the_tail() {
        let open = Settings {
            damp: 0,
            ..settings(0xC0)
        };
        let dark = Settings { damp: 0xFF, ..open };
        assert!(energy(dark, 6000.0) < energy(open, 6000.0) * 0.01);
        // The lows ring on.
        assert!(energy(dark, 150.0) > energy(open, 150.0) * 0.2);
    }

    #[test]
    fn high_cut_takes_highs_from_what_goes_in() {
        let open = Settings::DEFAULT;
        let cut = Settings {
            high_cut: 0x80,
            ..open
        };
        assert!(energy(cut, 8000.0) < energy(open, 8000.0) * 0.05);
        assert!(energy(cut, 200.0) > energy(open, 200.0) * 0.5);
    }

    #[test]
    fn its_sides_differ_so_it_spreads() {
        let out = impulse(&mut plate(Settings::DEFAULT, RATE), RATE as usize * 2);
        let tail = &out[RATE as usize / 10..];
        let (mut both, mut left, mut right) = (0.0_f64, 0.0_f64, 0.0_f64);
        for [l, r] in tail {
            both += f64::from(l * r);
            left += f64::from(l * l);
            right += f64::from(r * r);
        }
        let correlation = both / (left * right).sqrt();
        assert!(correlation.abs() < 0.2, "{correlation}");
        // About as loud on each side.
        assert!((left / right - 1.0).abs() < 0.5, "{left} {right}");
    }

    #[test]
    fn pre_delay_holds_back_what_comes_out() {
        let first = |pre| {
            let settings = Settings {
                pre,
                ..Settings::DEFAULT
            };
            let out = impulse(&mut plate(settings, RATE), RATE as usize);
            out.iter().position(|f| f[0] != 0.0 || f[1] != 0.0).unwrap()
        };
        let (none, most) = (first(0), first(0xFF));
        let held = (most - none) as f32 / RATE as f32;
        assert!((held - LONGEST_PRE).abs() < 0.002, "{held}");
    }

    #[test]
    fn size_makes_the_plate_bigger_and_leaves_its_tail_as_long() {
        let out = |size| {
            let settings = Settings {
                size,
                ..Settings::DEFAULT
            };
            impulse(&mut plate(settings, RATE), RATE as usize * 12)
        };
        let (small, big) = (out(0x00), out(0xFF));
        let first = |out: &[[f32; 2]]| out.iter().position(|f| f[0] != 0.0).unwrap();
        assert!(
            first(&big) > first(&small) * 3,
            "{} {}",
            first(&small),
            first(&big)
        );
        let (short, long) = (decay_seconds(&small, RATE), decay_seconds(&big, RATE));
        assert!((long / short - 1.0).abs() < 0.25, "{short} {long}");
    }

    /// The biggest jump between frames from a tone through it, after a second, with the settings
    /// switching between two every 64 frames, as a knob or an LFO steps them block by block, or
    /// held at the first.
    fn biggest_jump(from: Settings, to: Settings, moving: bool) -> f32 {
        let tone =
            |frame: usize| (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
        let mut plate = plate(from, RATE);
        let mut last = [0.0_f32; 2];
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize * 3 {
            let late = frame > RATE as usize;
            if late {
                let now = if moving && frame / 64 % 2 == 1 {
                    to
                } else {
                    from
                };
                plate.follow(now, frame as u64);
            }
            let out = plate.process([tone(frame); 2]);
            if late {
                most = most.max((out[0] - last[0]).abs().max((out[1] - last[1]).abs()));
            }
            last = out;
        }
        most
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let plain = Settings::DEFAULT;
        let pairs = [
            ("Pre", plain, Settings { pre: 0xFF, ..plain }),
            ("Decay", settings(0x40), settings(0xFF)),
            (
                "Damp",
                plain,
                Settings {
                    damp: 0xFF,
                    ..plain
                },
            ),
            (
                "High cut",
                Settings {
                    high_cut: 0x60,
                    ..plain
                },
                plain,
            ),
            (
                "Size",
                Settings { size: 0, ..plain },
                Settings {
                    size: 0xFF,
                    ..plain
                },
            ),
            (
                "Mod",
                Settings {
                    excursion: 0,
                    ..plain
                },
                Settings {
                    excursion: 0xFF,
                    ..plain
                },
            ),
            (
                "Low damp",
                plain,
                Settings {
                    low_damp: 0xFF,
                    ..plain
                },
            ),
            ("Width", Settings { width: 0, ..plain }, plain),
            (
                "all",
                Settings {
                    decay: 0x40,
                    high_cut: 0x60,
                    size: 0,
                    excursion: 0,
                    ..plain
                },
                Settings {
                    pre: 0xFF,
                    decay: 0xFF,
                    damp: 0xFF,
                    size: 0xFF,
                    excursion: 0xFF,
                    ..plain
                },
            ),
        ];
        for (name, from, to) in pairs {
            // It jumps no further moving than it does held at either end.
            let still = biggest_jump(from, from, false).max(biggest_jump(to, to, false));
            let moving = biggest_jump(from, to, true);
            assert!(
                moving < still * 1.25,
                "{name}: {moving} moving, {still} still"
            );
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for decay in [0x00, 0xFF] {
            for damp in [0x00, 0xFF] {
                for size in [0x00, 0xFF] {
                    for (rate, excursion, high_cut) in [(44_100, 0x00, 0xFF), (192_000, 0xFF, 0x00)]
                    {
                        let mut plate = plate(
                            Settings {
                                pre: excursion,
                                decay,
                                damp,
                                high_cut,
                                size,
                                excursion,
                                ..Settings::DEFAULT
                            },
                            rate,
                        );
                        for frame in 0..rate as usize * 3 {
                            // Full-scale noise, a full-scale square, DC and silence in turn.
                            let level = match frame / (rate as usize / 4) % 4 {
                                0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                1 => {
                                    if frame / 50 % 2 == 0 {
                                        1.0
                                    } else {
                                        -1.0
                                    }
                                }
                                2 => 1.0,
                                _ => 0.0,
                            };
                            let out = plate.process([level, -level * 0.5]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                                "{decay:x} {damp:x} {size:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn its_lines_hold_the_biggest_plate_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let plate = Plate::new(rate);
            let scale = rate as f32 / TUNING_RATE;
            let needed: f32 = DIFFUSER_LENGTHS.iter().sum::<f32>() * scale
                + TANK_LENGTHS.iter().flatten().sum::<f32>() * LARGEST * scale;
            assert!(plate.frames() as f32 >= needed, "{rate}");
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut plate = plate(Settings::DEFAULT, RATE);
        impulse(&mut plate, 1000);
        plate.clear();
        for _ in 0..RATE {
            assert_eq!(plate.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }

    #[test]
    fn its_tail_falls_as_fast_as_a_click_through_it_measures() {
        // Each frequency's modes fall at their own rate, and the tail is what a band of them
        // measures. The wobble mixes the modes a little as they go round, which isn't worked
        // out, and a big plate has only some hundreds of modes in the band at its lows, so how
        // they fall differs a little from the many it's worked out from. Seen: within 11%.
        use crate::dsp::effects::tail::measure;
        let cases = [
            (0x57, 0x00, 0x80, 48_000),
            (0x76, 0x00, 0x80, 48_000),
            (0x9D, 0x00, 0x80, 48_000),
            (0x76, 0x80, 0x00, 48_000),
            (0x9D, 0xFF, 0xFF, 48_000),
            (0xBD, 0x40, 0x80, 48_000),
            (0x76, 0x40, 0xC0, 44_100),
            (0x76, 0x40, 0xC0, 48_000),
            (0x76, 0x40, 0x40, 96_000),
        ];
        for (decay, damp, size, rate) in cases {
            let settings = Settings {
                decay,
                damp,
                size,
                ..Settings::DEFAULT
            };
            let said = tail(settings, rate).unwrap();
            let mut plate = plate(settings, rate);
            let ratios = measure::ratios(said, |x| plate.process([x, x]), rate);
            for (ratio, hertz) in ratios.iter().zip(TAIL_HZ) {
                assert!(
                    (ratio - 1.0).abs() < 0.15,
                    "{decay:02X} {damp:02X} {size:02X} at {rate}, {hertz} Hz: {said:?} measured {ratio}"
                );
            }
        }
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
            let response: Vec<f32> = impulse(&mut plate(settings, RATE), RATE as usize / 2)
                .iter()
                .map(|frame| frame[0])
                .collect();
            // Then its taps, the nearest some milliseconds into the tank.
            let heard = measure::onset(&response, RATE);
            assert!(
                heard >= said.pre - 1.0 / RATE as f32 && heard < said.pre + 0.05,
                "{pre:02X}: {heard} against {}",
                said.pre
            );
        }
    }

    #[test]
    fn decay_is_how_long_its_tail_takes_to_fall_60_db_at_any_size_and_rate() {
        // Undamped, under its damping. Seen: within 10%.
        use crate::dsp::effects::tail::measure::click_seconds;
        for (decay, size, rate) in [
            (0x00, 0x80, 48_000),
            (0x20, 0x80, 48_000),
            (0x40, 0x80, 48_000),
            (0x60, 0x80, 48_000),
            (0x80, 0x80, 48_000),
            (0xA0, 0x80, 48_000),
            (0x60, 0x00, 48_000),
            (0x60, 0xFF, 48_000),
            (0x60, 0x80, 44_100),
            (0x60, 0x80, 96_000),
        ] {
            let settings = Settings {
                damp: 0,
                size,
                ..settings(decay)
            };
            let set = reverb_decay_seconds(f32::from(decay));
            let mut plate = plate(settings, rate);
            let measured = click_seconds(|x| plate.process(x), set, rate, 250.0);
            assert!(
                (measured / set - 1.0).abs() < 0.15,
                "{decay:02X} {size:02X} at {rate}: {measured} for {set}"
            );
        }
    }

    #[test]
    fn low_damp_takes_the_lows_from_the_tail_and_leaves_the_rest() {
        use crate::dsp::effects::tail::measure::{click_seconds, ratios};
        let open = Settings {
            damp: 0,
            ..settings(0x80)
        };
        let thin = Settings {
            low_damp: 0x80,
            ..open
        };
        let at = |settings, hertz| {
            let mut plate = plate(settings, RATE);
            click_seconds(|x| plate.process(x), 4.0, RATE, hertz)
        };
        let (thin_low, open_low) = (at(thin, 100.0), at(open, 100.0));
        assert!(thin_low < open_low * 0.7, "{thin_low} {open_low}");
        assert!(at(thin, 2000.0) > at(open, 2000.0) * 0.85);
        let said = tail(thin, RATE).unwrap();
        let mut plate = plate(thin, RATE);
        let ratios = ratios(said, |x| plate.process([x, x]), RATE);
        assert!(
            ratios.iter().all(|ratio| (ratio - 1.0).abs() < 0.15),
            "{ratios:?}"
        );
    }

    #[test]
    fn width_brings_its_sides_together_and_leaves_their_sum() {
        let out = |width| {
            let settings = Settings {
                width,
                ..Settings::DEFAULT
            };
            impulse(&mut plate(settings, RATE), RATE as usize)
        };
        let (wide, half, mono) = (out(0xFF), out(0x80), out(0x00));
        assert!(mono.iter().all(|[l, r]| l == r));
        for ((w, h), m) in wide.iter().zip(&half).zip(&mono) {
            let sums = [w, h, m].map(|f| f[0] + f[1]);
            assert!((sums[0] - sums[1]).abs() < 1e-5 && (sums[0] - sums[2]).abs() < 1e-5);
        }
    }
}
