//! A stereo hall: a feedback delay network after Jot and Chaigne (1991), laid out as Fons
//! Adriaensen's zita-rev1 lays its out (freeverb3's `zrev`). Each side goes through the cuts and a
//! pre-delay, then four all-passes that spread it; the left feeds four of eight lines and the
//! right the other four, as zita-rev1's do. Each line is an all-pass and a delay whose read
//! wobbles slowly and is read between samples by a cubic; what comes out of the eight is mixed by
//! an 8x8 Hadamard matrix, which keeps all of it, and goes back in through each line's absorption,
//! which keeps of the lows, the middle and the highs what each band's `Decay` asks for that line's
//! length. Early reflections are taps on the pre-delay, freeverb3's `earlyref` pattern, and the
//! late tail is four lines a side, taken with signs that don't cancel in mono (decision 440).
//!
//! The lines' lengths are prime numbers of frames at 48 kHz, spread over 1:1.5, scaled to the
//! rate and by `Size`, which moves no faster than its whole range in a second, as the plate's
//! does. Its memory comes from the effect pool, long enough for the biggest hall at the pool's
//! rate.
//!
//! The early reflections' times and gains are freeverb3's `earlyref.cpp` preset 11:
//!
//! Copyright (C) 2006-2018 Teru Kamogashira. Freeverb3 is free software, under the GNU General
//! Public License version 2 or any later version, so it's used here under version 3.

use vt_model::insert::{
    Hall as Settings, REVERB_DECAY_SECONDS, hall_lows, hall_size, reverb_decay_seconds,
};

use crate::dsp::blocks::coefficient::of_corner;
use crate::dsp::blocks::line::{SharedLines, hermite};
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::{REVERB_QUIET_SECONDS, SongPlace};
use crate::dsp::effects::reverb::widen;
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::tail::{self, Complex, TAIL_HZ, Tail};
use crate::dsp::filters::filter::{Coefficients, Trim};

/// The rate the lengths are written for.
const TUNING_RATE: f32 = 48_000.0;
/// How many lines go round.
const LINES: usize = 8;
/// Each line's delay at `Size` `80`, in frames at the tuning rate: primes from 60 to 92 ms, about
/// evenly in ratio over 1:1.5, put in the order that has each side's four spread over all of it.
const DELAYS: [usize; LINES] = [2879, 3457, 3907, 4409, 3061, 3253, 3673, 4153];
/// Each line's all-pass, the same way: primes from 9 to 21 ms.
const LOOP_ALLPASSES: [usize; LINES] = [443, 701, 859, 1031, 547, 619, 787, 953];
/// How much the lines' all-passes spread, alternately added and taken away, as zita-rev1's.
const LOOP_SPREAD: f32 = 0.6;
/// Each side's input all-passes, in frames at the tuning rate, and how much they spread.
const DIFFUSERS: [[usize; 4]; 2] = [[211, 157, 103, 67], [223, 163, 109, 71]];
const DIFFUSION: f32 = 0.7;
/// The early reflections at `Size` `80`, each side's, in seconds, and their gains: freeverb3's
/// `earlyref` preset 11.
const EARLY_SECONDS: [[f32; 11]; 2] = [
    [
        0.003568, 0.011703, 0.019526, 0.024870, 0.037740, 0.048089, 0.053948, 0.061333, 0.061344,
        0.070073, 0.077130,
    ],
    [
        0.002818, 0.009115, 0.017042, 0.023885, 0.033068, 0.042307, 0.051234, 0.059896, 0.067984,
        0.067995, 0.076458,
    ],
];
const EARLY_GAINS: [[f32; 11]; 2] = [
    [
        0.963333, -0.806667, 0.706667, -0.656667, -0.556667, 0.526667, -0.506667, 0.503333,
        0.450000, -0.470000, 0.456667,
    ],
    [
        0.980000, -0.850000, 0.726667, -0.660000, 0.583333, 0.553333, -0.503333, 0.486667,
        -0.473333, -0.466667, 0.463333,
    ],
];
/// How loud the early reflections are at the top of `Early`, against the tail.
const EARLY_LEVEL: f32 = 0.25;
/// Where the absorption's shelves turn, in hertz.
const LOW_CROSSOVER_HZ: f32 = 500.0;
const HIGH_CROSSOVER_HZ: f32 = 2_000.0;
/// Where the high-pass on what goes in takes away what's below hearing, as zita-rev1's does, so
/// DC can't build up in the lows' long tail.
const DC_CUT_HZ: f32 = 5.0;
/// How long the highs ring against the middle at the top of `Damp`. At `00` they ring as long.
const DARKEST: f32 = 0.1;
/// The biggest hall, as so many times the lengths, for the lines' room.
const LARGEST: f32 = 2.0;
/// How far `Size` moves in a second at most, as the plate's.
const SIZE_PER_SECOND: f32 = 1.5;
/// How far a line's read wobbles at the top of `Mod`, in seconds, and how fast each line's goes,
/// in hertz: slow, and never together, so the modes drift apart without a chorus. At the fastest
/// and deepest the pitch moves by under 2 cents.
const MOST_WOBBLE: f32 = 0.00035;
const WOBBLE_HZ: [f64; LINES] = [0.13, 0.41, 0.21, 0.31, 0.17, 0.47, 0.26, 0.36];
/// How far each wobble starts round its circle, so no two lines move together.
const WOBBLE_PHASES: [f64; LINES] = [0.0, 0.31, 0.62, 0.13, 0.84, 0.45, 0.27, 0.71];
/// The longest pre-delay, in seconds, as the reverbs'.
const LONGEST_PRE: f32 = 0.25;
/// How much of each side goes into the lines, and how loud the tail comes out.
const INPUT_GAIN: f32 = 0.5;
const WET: f32 = 0.58;
/// How much longer a trip round a line lasts than its delay and all-pass, as far as its tail goes,
/// as the reverb's `HELD`: its all-pass holds some frequencies back longer, and those last. From
/// measuring the middle at 2 s and 16 s, where it's 5% and 3% long without it.
const HELD: f32 = 1.04;
/// Which lines each side hears, and with what sign: four each, so their sum in mono is eight
/// different lines, which nothing cancels.
const OUT_SIGNS: [[f32; LINES]; 2] = [
    [1.0, -1.0, 1.0, -1.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, -1.0, -1.0],
];
/// The sign each side goes into its four lines with, as zita-rev1's.
const IN_SIGNS: [f32; 4] = [1.0, 1.0, -1.0, -1.0];
/// How often the lines' gains are worked out again at most while `Decay`, `Lows`, `Damp` or
/// `Size` glide, in frames: 24 powers a time, so not every frame.
const GAINS_EVERY: u32 = 32;

/// How long each line is at a rate and size, in frames: its delay and its all-pass.
fn line_frames(rate: u32, stretch: f32) -> [f32; LINES] {
    let scale = rate as f32 / TUNING_RATE * stretch;
    std::array::from_fn(|line| (DELAYS[line] + LOOP_ALLPASSES[line]) as f32 * scale)
}

/// The three bands' tails, in seconds: the lows, the middle and the highs.
/// The lows ring no longer than the longest `Decay`, so a sustained low note can't build up
/// without end.
fn band_seconds(settings: Settings) -> [f32; 3] {
    let middle = reverb_decay_seconds(f32::from(settings.decay));
    let highs = DARKEST.powf(f32::from(settings.damp) / 255.0);
    [
        (middle * hall_lows(f32::from(settings.lows))).min(REVERB_DECAY_SECONDS.1),
        middle,
        middle * highs,
    ]
}

/// What a line `frames` long keeps of each band each time round, for tails of `seconds`.
fn band_gains(seconds: [f32; 3], frames: f32, rate: u32) -> [f32; 3] {
    seconds.map(|seconds| 0.001_f32.powf(frames * HELD / (seconds * rate as f32)))
}

/// A biquad's coefficients, over its `a0`.
#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
}

impl Biquad {
    /// Robert Bristow-Johnson's shelf, of slope 1, its corner at `hertz`, `gain` times at its far
    /// end, the highs' for a high shelf and the lows' for a low one, as zita-rev1's lines have.
    fn shelf(high: bool, hertz: f32, gain: f32, rate: u32) -> Self {
        let a = gain.sqrt();
        let omega = std::f32::consts::TAU * hertz / rate as f32;
        let (sin, cos) = omega.sin_cos();
        let alpha = sin * std::f32::consts::FRAC_1_SQRT_2;
        let lift = 2.0 * a.sqrt() * alpha;
        // A high shelf is a low one with the cosine's sign turned, and its odd terms'.
        let (cos, odd) = if high { (-cos, -1.0) } else { (cos, 1.0) };
        let b0 = a * ((a + 1.0) - (a - 1.0) * cos + lift);
        let b1 = 2.0 * a * ((a - 1.0) - (a + 1.0) * cos) * odd;
        let b2 = a * ((a + 1.0) - (a - 1.0) * cos - lift);
        let a0 = (a + 1.0) + (a - 1.0) * cos + lift;
        let a1 = -2.0 * ((a - 1.0) + (a + 1.0) * cos) * odd;
        let a2 = (a + 1.0) + (a - 1.0) * cos - lift;
        Self {
            b: [b0 / a0, b1 / a0, b2 / a0],
            a: [a1 / a0, a2 / a0],
        }
    }

    /// One frame through it, transposed direct form II, with its two states.
    #[inline]
    fn run(self, state: &mut [f32; 2], input: f32) -> f32 {
        let out = self.b[0] * input + state[0];
        state[0] = self.b[1] * input - self.a[0] * out + state[1];
        state[1] = self.b[2] * input - self.a[1] * out;
        out
    }

    /// Its response at `omega` radians a frame.
    fn response(self, omega: f64) -> Complex {
        let at = |c: [f64; 3]| {
            Complex(c[0], 0.0)
                .add(Complex::delay(omega, 1.0).scale(c[1]))
                .add(Complex::delay(omega, 2.0).scale(c[2]))
        };
        at(self.b.map(f64::from)).over(at([1.0, f64::from(self.a[0]), f64::from(self.a[1])]))
    }
}

/// A line's absorption, from its three bands' gains: what it keeps of the middle, then a low
/// shelf to what it keeps of the lows and a high shelf to the highs'.
#[derive(Debug, Clone, Copy, Default)]
struct Absorption {
    middle: f32,
    low: Biquad,
    high: Biquad,
}

impl Absorption {
    fn new([low, middle, high]: [f32; 3], rate: u32) -> Self {
        Self {
            middle,
            low: Biquad::shelf(false, LOW_CROSSOVER_HZ, low / middle, rate),
            high: Biquad::shelf(true, HIGH_CROSSOVER_HZ, high / middle, rate),
        }
    }

    /// One frame through it, with its shelves' states.
    #[inline]
    fn absorb(self, states: &mut [[f32; 2]; 2], input: f32) -> f32 {
        let low = self.low.run(&mut states[0], input * self.middle);
        self.high.run(&mut states[1], low)
    }

    /// How much it keeps at `hertz`, as a gain.
    /// How much it keeps at `omega` radians a frame, as a gain.
    fn kept_at(self, omega: f64) -> f64 {
        self.low
            .response(omega)
            .times(self.high.response(omega))
            .scale(f64::from(self.middle))
            .size()
    }
}

/// How much a line keeps at `hertz` with its band gains, as a gain.
#[cfg(test)]
fn kept_at(gains: [f32; 3], hertz: f32, rate: u32) -> f32 {
    let omega = std::f64::consts::TAU * f64::from(hertz) / f64::from(rate);
    Absorption::new(gains, rate).kept_at(omega) as f32
}

/// How a hall's tail dies away at its settings and a rate, for a drawing. Every line keeps of each
/// frequency what falls 60 dB in that band's time over its length, and the matrix keeps all of it,
/// so the tail is its first line's: what a band of it around a frequency measures, as
/// `tail::loop_seconds` works it out, as a shelf's corner is within an octave of where it's drawn.
pub fn tail(settings: Settings, rate: u32) -> Option<Tail> {
    let frames = line_frames(rate, hall_size(f32::from(settings.size)))[0];
    let absorption = Absorption::new(band_gains(band_seconds(settings), frames, rate), rate);
    let held = f64::from(frames * HELD);
    let seconds = TAIL_HZ
        .map(|hertz| tail::loop_seconds(hertz, rate, |omega| (absorption.kept_at(omega), held)));
    Some(Tail {
        pre: f32::from(settings.pre) / 255.0 * LONGEST_PRE,
        seconds,
    })
}

/// One line's place in the shared buffer: where its delay starts and its length at the tuning
/// rate, and where its all-pass starts and its length here.
#[derive(Debug, Clone, Copy)]
struct Line {
    delay: usize,
    delay_frames: f32,
    allpass: usize,
    allpass_frames: f32,
}

/// The stereo hall.
#[derive(Debug, Clone)]
pub(crate) struct Hall {
    lines: SharedLines,
    tank: [Line; LINES],
    /// Each side's input all-passes: where each starts and how long it is here.
    diffusers: [[(usize, usize); 4]; 2],
    /// Each line's shelves' states.
    absorbing: [[[f32; 2]; 2]; LINES],
    /// The bands' tails, and each line's absorption for them at the size it was worked out for.
    bands: [Smoothed; 3],
    absorption: [Absorption; LINES],
    spreads: [f32; LINES],
    gains_for: ([f32; 3], f32),
    /// How many frames until they may be worked out again.
    gains_wait: u32,
    /// Each line's wobble, as a point going round a circle, and how far it turns each frame.
    wobble: [(f32, f32); LINES],
    turn: [(f32, f32); LINES],
    depth: Smoothed,
    /// Each side's pre-delay, which the early reflections read from too.
    pre: [Vec<f32>; 2],
    pre_write: usize,
    pre_frames: Smoothed,
    early: Smoothed,
    size: Smoothed,
    /// The size the lines are read at now, which follows `size` no faster than it may move.
    size_now: f32,
    width: Smoothed,
    trims: [Trim; 2],
    /// The DC cut's low-pass on each side, and what it moves each frame.
    dc: [f32; 2],
    dc_by: f32,
    low_cut: Option<(u8, Coefficients)>,
    high_cut: Option<(u8, Coefficients)>,
    smoothing: f32,
    /// This rate over the tuning rate.
    scale: f32,
    rate: u32,
}

impl Hall {
    /// Make a hall for a sample rate, with lines long enough for the biggest.
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let mut base = 0;
        let mut place = |frames: f32| {
            let at = base;
            // Room for the cubic's frames either side, and the wobble.
            base += frames.ceil() as usize + 4;
            at
        };
        let most_wobble = MOST_WOBBLE * rate as f32;
        let tank = std::array::from_fn(|line| {
            let delay_frames = DELAYS[line] as f32;
            let allpass_frames = LOOP_ALLPASSES[line] as f32;
            Line {
                delay: place(delay_frames * scale * LARGEST + most_wobble),
                delay_frames,
                allpass: place(allpass_frames * scale * LARGEST),
                allpass_frames,
            }
        });
        let diffusers = DIFFUSERS.map(|side| {
            side.map(|frames| {
                let length = ((frames as f32 * scale) as usize).max(1);
                (place(length as f32), length)
            })
        });
        let longest_early = EARLY_SECONDS
            .iter()
            .flatten()
            .fold(0.0_f32, |most, seconds| most.max(*seconds));
        let pre_frames = ((LONGEST_PRE + longest_early * LARGEST) * rate as f32) as usize + 4;
        let turn = WOBBLE_HZ.map(|hertz| {
            let angle = std::f64::consts::TAU * hertz / f64::from(rate);
            (angle.cos() as f32, angle.sin() as f32)
        });
        let mut hall = Self {
            lines: SharedLines::new(base),
            tank,
            diffusers,
            absorbing: [[[0.0; 2]; 2]; LINES],
            bands: [Smoothed::at(1.0); 3],
            absorption: [Absorption::default(); LINES],
            spreads: [LOOP_SPREAD; LINES],
            gains_for: ([f32::NAN; 3], f32::NAN),
            gains_wait: 0,
            wobble: [(1.0, 0.0); LINES],
            turn,
            depth: Smoothed::at(0.0),
            pre: [vec![0.0; pre_frames], vec![0.0; pre_frames]],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            early: Smoothed::at(0.0),
            size: Smoothed::at(1.0),
            size_now: 1.0,
            width: Smoothed::at(1.0),
            trims: [Trim::default(), Trim::default()],
            dc: [0.0; 2],
            dc_by: of_corner(DC_CUT_HZ, rate as f32),
            low_cut: None,
            high_cut: None,
            smoothing: coefficient(rate),
            scale,
            rate,
        };
        hall.follow(Settings::DEFAULT, 0);
        hall.settle();
        hall
    }

    /// Forget what's in it.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        for side in &mut self.pre {
            side.fill(0.0);
        }
        self.absorbing = [[[0.0; 2]; 2]; LINES];
        self.trims = [Trim::default(), Trim::default()];
        self.dc = [0.0; 2];
    }

    /// How many frames its buffer holds, for the tests.
    #[cfg(test)]
    fn frames(&self) -> usize {
        self.lines.len()
    }

    /// Take the settings, and move the wobbles to where they are this many frames in. They go by
    /// the frame rather than by what the hall has done, so one that's slept is where it would
    /// have been.
    pub(crate) fn follow(&mut self, settings: Settings, frames: u64) {
        self.keep_time(frames);
        let rate = self.rate as f32;
        self.pre_frames.target = f32::from(settings.pre) / 255.0 * LONGEST_PRE * rate;
        for (band, seconds) in self.bands.iter_mut().zip(band_seconds(settings)) {
            band.target = seconds;
        }
        self.size.target = hall_size(f32::from(settings.size));
        self.early.target = f32::from(settings.early) / 255.0 * EARLY_LEVEL;
        self.depth.target = f32::from(settings.excursion) / 255.0 * MOST_WOBBLE * rate;
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
        for ((wobble, hertz), phase) in self.wobble.iter_mut().zip(WOBBLE_HZ).zip(WOBBLE_PHASES) {
            let turns = seconds * hertz + phase;
            let angle = std::f64::consts::TAU * (turns - turns.floor());
            *wobble = (angle.cos() as f32, angle.sin() as f32);
        }
    }

    /// Jump to the settings, for starting afresh.
    pub(crate) fn settle(&mut self) {
        self.pre_frames.settle();
        for band in &mut self.bands {
            band.settle();
        }
        self.size.settle();
        self.size_now = self.size.value;
        self.early.settle();
        self.depth.settle();
        self.width.settle();
        self.gains_wait = 0;
    }

    /// Each line's band gains for the bands' tails and the size now, worked out again only when
    /// either has moved, and then at most once every `GAINS_EVERY` frames, as they glide. With
    /// them, how much each line's all-pass spreads: `LOOP_SPREAD`, or less where the shortest band
    /// is short. An all-pass's echoes of itself go round it whatever the line keeps, so it spreads
    /// no more than the shortest band keeps over twice its length; at once its length, the
    /// all-passes' ringing and the band's together measured twice as long as the band.
    #[inline]
    fn gains(&mut self, bands: [f32; 3], stretch: f32) -> ([Absorption; LINES], [f32; LINES]) {
        self.gains_wait = self.gains_wait.saturating_sub(1);
        if self.gains_wait == 0 && (bands, stretch) != self.gains_for {
            let frames = line_frames(self.rate, stretch);
            self.absorption = frames
                .map(|frames| Absorption::new(band_gains(bands, frames, self.rate), self.rate));
            let shortest = bands
                .iter()
                .fold(f32::INFINITY, |least, band| least.min(*band));
            let scale = self.scale * stretch;
            self.spreads = LOOP_ALLPASSES.map(|frames| {
                let frames = (frames as f32 * scale) as usize as f32;
                LOOP_SPREAD.min(0.001_f32.powf(2.0 * frames / (shortest * self.rate as f32)))
            });
            self.gains_for = (bands, stretch);
            self.gains_wait = GAINS_EVERY;
        }
        (self.absorption, self.spreads)
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let smoothing = self.smoothing;
        let bands = self.bands.each_mut().map(|band| band.next(smoothing));
        let wanted = self.size.next(smoothing);
        let most = SIZE_PER_SECOND / self.rate as f32;
        self.size_now += (wanted - self.size_now).clamp(-most, most);
        let stretch = self.size_now;
        let (absorption, spreads) = self.gains(bands, stretch);
        let depth = self.depth.next(smoothing);
        let early_level = self.early.next(smoothing);
        let width = self.width.next(smoothing);
        let scale = self.scale * stretch;

        // The cuts, then each side into its pre-delay.
        let length = self.pre[0].len();
        let mut cut = input;
        for (side, sample) in cut.iter_mut().enumerate() {
            if let Some((_, c)) = &self.low_cut {
                *sample = self.trims[side].low_cut(*sample, c);
            }
            if let Some((_, c)) = &self.high_cut {
                *sample = self.trims[side].high_cut(*sample, c);
            }
            self.dc[side] += (*sample - self.dc[side]) * self.dc_by;
            *sample -= self.dc[side];
            self.pre[side][self.pre_write] = *sample;
        }
        let back = self.pre_frames.next(smoothing);
        let read = |line: &[f32], back: f32| {
            let whole = back as usize;
            let fraction = back - whole as f32;
            let a = line[(self.pre_write + length - whole) % length];
            let b = line[(self.pre_write + length - whole - 1) % length];
            a + (b - a) * fraction
        };
        let held = [read(&self.pre[0], back), read(&self.pre[1], back)];
        let mut early = [0.0; 2];
        if early_level > 0.0 {
            let rate = self.rate as f32;
            for (side, out) in early.iter_mut().enumerate() {
                *out = EARLY_SECONDS[side]
                    .iter()
                    .zip(EARLY_GAINS[side])
                    .map(|(seconds, gain)| {
                        read(&self.pre[side], back + seconds * stretch * rate) * gain
                    })
                    .sum::<f32>()
                    * early_level;
            }
        }
        self.pre_write = (self.pre_write + 1) % length;

        let lines = &mut self.lines;
        lines.advance();
        // Each side spread by its all-passes.
        let mut diffused = held.map(|sample| sample * INPUT_GAIN);
        for (sample, side) in diffused.iter_mut().zip(self.diffusers) {
            for (base, length) in side {
                *sample = lines.allpass(base, length, *sample, DIFFUSION);
            }
        }

        // What each line gives now, read between samples where its wobble has it, with each
        // side's input added and through its all-pass.
        let mut x = [0.0_f32; LINES];
        for (line, out) in x.iter_mut().enumerate() {
            let Line {
                delay,
                delay_frames,
                allpass,
                allpass_frames,
            } = self.tank[line];
            let (cos, sin) = self.wobble[line];
            let (turn_cos, turn_sin) = self.turn[line];
            self.wobble[line] = (
                cos * turn_cos - sin * turn_sin,
                sin * turn_cos + cos * turn_sin,
            );
            let back = (delay_frames * scale + depth * sin).max(2.0);
            let whole = back as usize;
            let between = back - whole as f32;
            let given = hermite(
                lines.read(delay, whole - 1),
                lines.read(delay, whole),
                lines.read(delay, whole + 1),
                lines.read(delay, whole + 2),
                between,
            );
            let side = line / 4;
            let sample = given + diffused[side] * IN_SIGNS[line % 4];
            let spread = if line % 2 == 0 {
                spreads[line]
            } else {
                -spreads[line]
            };
            let frames = (allpass_frames * scale) as usize;
            *out = lines.allpass(allpass, frames.max(1), sample, spread);
        }

        // Hadamard's matrix, as zita-rev1 does it, by butterflies.
        for span in [1, 2, 4] {
            for start in (0..LINES).step_by(span * 2) {
                for at in start..start + span {
                    let (a, b) = (x[at], x[at + span]);
                    x[at] = a + b;
                    x[at + span] = a - b;
                }
            }
        }
        let norm = (LINES as f32).sqrt().recip();
        let mut out = early;
        for (side, signs) in out.iter_mut().zip(OUT_SIGNS) {
            *side += x.iter().zip(signs).map(|(x, sign)| x * sign).sum::<f32>() * norm * WET;
        }
        for line in 0..LINES {
            let kept = absorption[line].absorb(&mut self.absorbing[line], x[line] * norm);
            lines.write(self.tank[line].delay, 0, kept);
        }
        widen(out, width)
    }
}

impl SlotEffect for Hall {
    type Settings = Settings;

    /// A hall's, long enough for the biggest.
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

impl Rung for Hall {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::tail::measure::{click_seconds, ratios};

    const RATE: u32 = 48_000;

    fn hall(settings: Settings, rate: u32) -> Hall {
        let mut hall = Hall::new(rate);
        hall.follow(settings, 0);
        hall.settle();
        hall
    }

    /// No early reflections, and every band as long as the middle.
    const FLAT: Settings = Settings {
        lows: 0x80,
        damp: 0,
        early: 0,
        ..Settings::DEFAULT
    };

    fn impulse(hall: &mut Hall, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| hall.process([if at == 0 { 1.0 } else { 0.0 }; 2]))
            .collect()
    }

    #[test]
    fn its_lines_are_prime_and_spread_over_one_and_a_half() {
        let prime = |n: usize| {
            (2..n)
                .take_while(|d| d * d <= n)
                .all(|d| !n.is_multiple_of(d))
        };
        for frames in DELAYS.iter().chain(&LOOP_ALLPASSES) {
            assert!(prime(*frames), "{frames}");
        }
        let (least, most) = (DELAYS.iter().min().unwrap(), DELAYS.iter().max().unwrap());
        let spread = *most as f32 / *least as f32;
        assert!((spread - 1.5).abs() < 0.05, "{spread}");
    }

    #[test]
    fn decay_is_how_long_its_middle_takes_to_fall_60_db_at_any_size_and_rate() {
        // Undamped, every band alike.
        for (decay, size, rate) in [
            (0x00, 0x80, 48_000),
            (0x30, 0x80, 48_000),
            (0x60, 0x80, 48_000),
            (0x90, 0x80, 48_000),
            (0x60, 0x00, 48_000),
            (0x60, 0xFF, 48_000),
            (0x60, 0x80, 44_100),
            (0x60, 0x80, 96_000),
        ] {
            let settings = Settings {
                decay,
                size,
                ..FLAT
            };
            let set = reverb_decay_seconds(f32::from(decay));
            let mut hall = hall(settings, rate);
            let measured = click_seconds(|x| hall.process(x), set, rate, 1000.0);
            // The input's all-passes and the lines' own spread out a short tail's start, so the
            // shortest measure a little longer.
            let tolerance = match set {
                set if set < 0.5 => 0.25,
                set if set < 1.0 => 0.15,
                _ => 0.08,
            };
            assert!(
                (measured / set - 1.0).abs() < tolerance,
                "{decay:02X} {size:02X} at {rate}: {measured} for {set}"
            );
        }
    }

    #[test]
    fn each_band_falls_in_its_own_time_as_its_tail_says() {
        // Lows, middle and highs set apart, measured at 125 Hz, 1 kHz and 8 kHz, well inside
        // their bands, against `band_seconds`; and the tail drawn against a click through it.
        // Seen: the bands within 11%, the drawing within 8%.
        for (lows, damp) in [(0x40, 0x00), (0x80, 0x80), (0xC0, 0xC0), (0xFF, 0xFF)] {
            let settings = Settings { lows, damp, ..FLAT };
            let [low, middle, high] = band_seconds(settings);
            for (hertz, set) in [(125.0, low), (1000.0, middle), (8000.0, high)] {
                let mut hall = hall(settings, RATE);
                let measured = click_seconds(|x| hall.process(x), set.max(middle), RATE, hertz);
                assert!(
                    (measured / set - 1.0).abs() < 0.15,
                    "{lows:02X} {damp:02X} at {hertz} Hz: {measured} for {set}"
                );
            }
            let said = tail(settings, RATE).unwrap();
            let mut hall = hall(settings, RATE);
            let ratios = ratios(said, |x| hall.process([x, x]), RATE);
            assert!(
                ratios.iter().all(|ratio| (ratio - 1.0).abs() < 0.12),
                "{lows:02X} {damp:02X}: {said:?} measured {ratios:?}"
            );
        }
    }

    /// Abel and Huang's normalised echo density of the left side around `at` seconds, over 20 ms:
    /// how many samples stand further from nothing than the window's spread, over how many would
    /// for noise. About 1 for a tail as dense as noise.
    fn echo_density(out: &[[f32; 2]], at: f32) -> f32 {
        let window = RATE as usize / 50;
        let start = (at * RATE as f32) as usize - window / 2;
        let part: Vec<f32> = out[start..start + window].iter().map(|f| f[0]).collect();
        let spread = (part.iter().map(|x| x * x).sum::<f32>() / window as f32).sqrt();
        let outside = part.iter().filter(|x| x.abs() > spread).count();
        outside as f32 / window as f32 / 0.3173
    }

    #[test]
    fn its_tail_is_as_dense_as_noise_within_100_ms() {
        for settings in [Settings::DEFAULT, FLAT] {
            let out = impulse(&mut hall(settings, RATE), RATE as usize / 2);
            for at in [0.1, 0.15, 0.2, 0.3, 0.4] {
                let density = echo_density(&out, at);
                assert!(density > 0.85, "{settings:?} at {at} s: {density}");
            }
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for (decay, lows, size) in [(0xFF, 0xFF, 0xFF), (0xFF, 0xFF, 0x00), (0x00, 0x00, 0x80)] {
            for (rate, excursion, high_cut) in [(44_100, 0x00, 0xFF), (192_000, 0xFF, 0x00)] {
                let mut hall = hall(
                    Settings {
                        pre: excursion,
                        decay,
                        lows,
                        damp: 0,
                        size,
                        early: 0xFF,
                        excursion,
                        high_cut,
                        ..Settings::DEFAULT
                    },
                    rate,
                );
                // What comes out each second, once nothing comes in.
                let mut seconds = [0.0_f32; 3];
                for frame in 0..rate as usize * 6 {
                    // Full-scale noise, a full-scale square and DC for a second each, then
                    // silence.
                    let level = match frame / rate as usize {
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
                    let out = hall.process([level, -level * 0.5]);
                    assert!(
                        out.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                        "{decay:x} {lows:x} {size:x} {rate}: {out:?}"
                    );
                    if frame >= rate as usize * 3 {
                        seconds[frame / rate as usize - 3] += out[0] * out[0] + out[1] * out[1];
                    }
                }
                // It dies away once nothing comes in, if very slowly at the longest.
                assert!(seconds[2] < seconds[0], "{seconds:?}");
            }
        }
    }

    #[test]
    fn every_line_keeps_less_than_all_of_every_frequency() {
        // The matrix keeps all of it, so the loop keeps less than all only if every line's
        // absorption does, at every frequency, at the longest of every band.
        let settings = Settings {
            decay: 0xFF,
            lows: 0xFF,
            ..FLAT
        };
        for size in [0x00_u8, 0xFF] {
            let stretch = hall_size(f32::from(size));
            for frames in line_frames(RATE, stretch) {
                let gains = band_gains(band_seconds(settings), frames, RATE);
                for step in 0..400 {
                    let hertz = 10.0 * 2400.0_f32.powf(step as f32 / 400.0);
                    let kept = kept_at(gains, hertz, RATE);
                    assert!(kept < 1.0, "{size:02X} {frames} at {hertz}: {kept}");
                }
            }
        }
    }

    #[test]
    fn it_sums_to_mono_without_cancelling() {
        let out = impulse(&mut hall(Settings::DEFAULT, RATE), RATE as usize);
        let energy =
            |pick: &dyn Fn(&[f32; 2]) -> f32| out.iter().map(|f| pick(f).powi(2)).sum::<f32>();
        let (left, right) = (energy(&|f| f[0]), energy(&|f| f[1]));
        let mono = energy(&|f| (f[0] + f[1]) * 0.5);
        // Uncorrelated sides sum to half of each's energy; cancelling would give much less.
        let half = (left + right) * 0.25;
        assert!(
            mono > half * 0.7 && mono < half * 1.5,
            "{mono} against {half}"
        );
        // And they differ, so it's wide.
        let both: f32 = out.iter().map(|f| f[0] * f[1]).sum();
        assert!((both / (left * right).sqrt()).abs() < 0.3);
    }

    #[test]
    fn its_wobble_moves_pitch_by_under_two_cents() {
        // A read moving by `depth` frames at `hertz` bends the pitch by up to depth times the
        // wobble's angular speed, as a share of the frequency.
        let depth = MOST_WOBBLE;
        let fastest = WOBBLE_HZ
            .iter()
            .fold(0.0_f64, |most, hertz| most.max(*hertz));
        let bend = f64::from(depth) * std::f64::consts::TAU * fastest;
        let cents = 1200.0 * (1.0 + bend).log2();
        assert!(cents < 2.0, "{cents}");
    }

    #[test]
    fn renders_repeat_and_clearing_forgets() {
        let render = |hall: &mut Hall| {
            (0..RATE as usize)
                .map(|at| {
                    let x = ((at * 7919) % 2003) as f32 / 1001.5 - 1.0;
                    hall.process([x, x * 0.3])
                })
                .collect::<Vec<_>>()
        };
        let mut first = hall(Settings::DEFAULT, RATE);
        let one = render(&mut first);
        let two = render(&mut hall(Settings::DEFAULT, RATE));
        assert!(
            one.iter()
                .zip(&two)
                .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits))
        );
        first.clear();
        first.keep_time(0);
        assert!(
            render(&mut first)
                .iter()
                .zip(&one)
                .all(|(a, b)| a.map(f32::to_bits) == b.map(f32::to_bits))
        );
        first.clear();
        for _ in 0..RATE {
            assert_eq!(first.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }

    #[test]
    fn its_lines_hold_the_biggest_hall_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let hall = Hall::new(rate);
            let scale = rate as f32 / TUNING_RATE;
            let needed: f32 = DELAYS
                .iter()
                .chain(&LOOP_ALLPASSES)
                .map(|frames| *frames as f32 * scale * LARGEST)
                .sum();
            assert!(hall.frames() as f32 >= needed, "{rate}");
        }
    }

    #[test]
    fn pre_delay_holds_back_what_comes_out() {
        use crate::dsp::effects::tail::measure::onset;
        for pre in [0x00, 0x80, 0xFF] {
            let settings = Settings { pre, ..FLAT };
            let said = tail(settings, RATE).unwrap();
            let response: Vec<f32> = impulse(&mut hall(settings, RATE), RATE as usize / 2)
                .iter()
                .map(|frame| frame[0])
                .collect();
            let heard = onset(&response, RATE);
            assert!(
                heard >= said.pre - 1.0 / RATE as f32 && heard < said.pre + 0.1,
                "{pre:02X}: {heard} against {}",
                said.pre
            );
        }
    }
}
