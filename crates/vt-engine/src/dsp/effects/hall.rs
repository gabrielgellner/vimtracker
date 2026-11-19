//! A stereo hall: a feedback delay network after Jot and Chaigne (1991), laid out as Fons
//! Adriaensen's zita-rev1 lays its out (freeverb3's `zrev`). Each side goes through the cuts and a
//! pre-delay, then seven all-passes that spread it; the left feeds four of eight lines and the
//! right the other four, as zita-rev1's do. Each line is an all-pass and a delay, both read
//! between samples by a cubic where a slow random wobble of their own has them; what comes out of
//! the eight is mixed by an 8x8 Hadamard matrix, which keeps all of it, and goes back in through
//! each line's absorption, which keeps of the lows, the middle and the highs what each band's
//! `Decay` asks for that line's length. Each side hears its own four lines' reads, as zita-rev1's
//! outputs do, and early reflections before them: 64 taps a side on the pre-delay, at times
//! scattered rather than evenly spaced, smoothed by five short all-passes, the two sides sharing
//! what's under 300 Hz (decision 440).
//!
//! The lines' lengths are prime numbers of frames at 48 kHz, spread over 1:1.5, scaled to the
//! rate and by `Size`, which moves no faster than its whole range in a second, as the plate's
//! does. Its memory comes from the effect pool, long enough for the biggest hall at the pool's
//! rate.

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
/// Each side's input all-passes, in frames at the tuning rate, and how much they spread: primes
/// from 1.6 to 19 ms, about evenly in ratio. Four short ones, as zita-rev1 has, left the tail's
/// first trips round the lines as clumps a round trip apart, which a snare heard as a roll.
const DIFFUSERS: [[usize; 7]; 2] = [
    [887, 619, 421, 283, 191, 127, 79],
    [907, 601, 433, 277, 197, 131, 83],
];
const DIFFUSION: f32 = 0.7;
/// How many early reflections each side has, from when to when at `Size` `80`, in seconds, and
/// how fast they fall away: in this time they're 1/e as loud. Most of their energy is in the first
/// 30 ms, as a snare's attack in a hall has it.
const EARLY_TAPS: usize = 64;
const EARLY_FIRST: f32 = 0.001;
const EARLY_LAST: f32 = 0.09;
const EARLY_FALL: f32 = 0.035;
/// Which of the hashed patterns the taps are: the 34th, which neither repeats nor leaves a hole in
/// a snare's body.
const EARLY_SEED: i64 = 34;
/// Each side's all-passes on the early reflections, in frames at the tuning rate, and how much
/// they spread: primes under 3.2 ms, which smear each tap into a short burst, too short to be
/// heard as an echo of it.
const EARLY_DIFFUSERS: [[usize; 5]; 2] = [[149, 113, 83, 61, 43], [151, 109, 79, 67, 41]];
const EARLY_DIFFUSION: f32 = 0.7;
/// How loud the early reflections are at the top of `Early`, against the tail, their taps' gains
/// adding to 1 in energy.
const EARLY_LEVEL: f32 = 1.75;
/// Under this, in hertz, the two sides' early reflections are one: their middle, as loud as either
/// side, by a one-pole's share. Each side's own taps cancelled the other's in the lows, so a snare's
/// body went missing from the mono sum.
const EARLY_SHARED_HZ: f32 = 300.0;
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
/// How far a line's delay and its all-pass are moved at the top of `Mod`, in seconds, and how
/// often each of the sixteen takes a new random place to glide to, in hertz: the lines' first,
/// then the all-passes'. Each wanders on its own, at rates no two share, so the modes drift apart
/// without the tail sweeping as a flanger does; sine wobbles at the lines alone, over twice as
/// deep, were heard as one.
const MOST_WOBBLE: f32 = 0.0003;
const WOBBLE_HZ: [f64; WOBBLES] = [
    2.0, 2.26, 1.74, 2.58, 1.86, 2.14, 2.42, 1.62, 2.5, 2.23, 2.93, 2.08, 2.78, 2.38, 3.08, 2.63,
];
const WOBBLES: usize = LINES * 2;
/// The cubic through four random points can overshoot them by a quarter, so the lines have room
/// for that.
const OVERSHOOT: f32 = 1.25;
/// The longest pre-delay, in seconds, as the reverbs'.
const LONGEST_PRE: f32 = 0.25;
/// How much of each side goes into the lines, and how loud the tail comes out.
const INPUT_GAIN: f32 = 0.5;
const WET: f32 = 0.78;
/// How much longer a trip round a line lasts than its delay and all-pass, as far as its tail goes,
/// as the reverb's `HELD`: its all-pass holds some frequencies back longer, and those last. From
/// measuring the middle at 2 s and 16 s, where it's 5% and 3% long without it.
const HELD: f32 = 1.04;
/// The sign each side goes into its four lines with, as zita-rev1's.
const IN_SIGNS: [f32; 4] = [1.0, 1.0, -1.0, -1.0];
/// How often the lines' gains are worked out again at most while `Decay`, `Lows`, `Damp` or
/// `Size` glide, in frames: 24 powers a time, so not every frame.
const GAINS_EVERY: u32 = 32;

/// A number from -1 to 1 for a stream and a step along it, the same every time: `SplitMix64`'s
/// mixer over both.
fn hashed(stream: u64, step: i64) -> f32 {
    let mut z = stream.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (step as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1_u64 << 23) as f32 - 1.0
}

/// Each side's early reflections at `Size` `80`: when, in seconds, and how loud. One a stretch of
/// the span divided evenly, somewhere in it, so they're spread out but no two gaps alike; louder
/// earlier, by a little over or under the fall at random, and of either sign; the gains adding to
/// 1 in energy.
fn early_taps() -> [[(f32, f32); EARLY_TAPS]; 2] {
    std::array::from_fn(|side| {
        let stream = |what: u64| 100 * what + side as u64;
        let mut taps: [(f32, f32); EARLY_TAPS] = std::array::from_fn(|tap| {
            let step = EARLY_SEED * 1000 + tap as i64;
            let within = (hashed(stream(1), step) + 1.0) * 0.5;
            let seconds = EARLY_FIRST
                + (EARLY_LAST - EARLY_FIRST) * (tap as f32 + within) / EARLY_TAPS as f32;
            let sign = if hashed(stream(2), step) > 0.0 {
                1.0
            } else {
                -1.0
            };
            let level = (-(seconds - EARLY_FIRST) / EARLY_FALL).exp()
                * (0.7 + 0.3 * hashed(stream(3), step).abs());
            (seconds, sign * level)
        });
        let energy: f32 = taps.iter().map(|(_, gain)| gain * gain).sum();
        for tap in &mut taps {
            tap.1 /= energy.sqrt();
        }
        taps
    })
}

/// One of the sixteen wobbles: a cubic through random points, one every `1 / WOBBLE_HZ` seconds,
/// worked out from where it is along them, so a hall that's slept is where it would have been.
#[derive(Debug, Clone, Copy, Default)]
struct Wobble {
    /// The four points around now, the step of the second, and how far past it now is.
    points: [f32; 4],
    step: i64,
    past: f64,
    /// How far it goes each frame, in steps.
    by: f64,
}

impl Wobble {
    /// The wobble `which`, this many seconds in.
    fn at(which: usize, by: f64, seconds: f64) -> Self {
        let steps = seconds * WOBBLE_HZ[which];
        let step = steps.floor() as i64;
        Self {
            points: std::array::from_fn(|at| hashed(which as u64, step - 1 + at as i64)),
            step,
            past: steps - step as f64,
            by,
        }
    }

    /// Where it is now, from -1.25 to 1.25 at most, and on a frame.
    #[inline]
    fn next(&mut self, which: usize) -> f32 {
        self.past += self.by;
        if self.past >= 1.0 {
            self.past -= 1.0;
            self.step += 1;
            let [_, a, b, c] = self.points;
            self.points = [a, b, c, hashed(which as u64, self.step + 2)];
        }
        let [a, b, c, d] = self.points;
        hermite(a, b, c, d, self.past as f32)
    }
}

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

/// The line at `base` read `back` frames back, between samples by the four-point cubic.
#[inline]
fn read_between(lines: &SharedLines, base: usize, back: f32) -> f32 {
    let back = back.max(1.0);
    let whole = back as usize;
    let between = back - whole as f32;
    hermite(
        lines.read(base, whole - 1),
        lines.read(base, whole),
        lines.read(base, whole + 1),
        lines.read(base, whole + 2),
        between,
    )
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
    /// Each side's input all-passes and its early reflections' all-passes: where each starts and
    /// how long it is here.
    diffusers: [[(usize, usize); 7]; 2],
    early_diffusers: [[(usize, usize); 5]; 2],
    /// Each side's early reflections, when at `Size` `80`, in frames here, and how loud.
    early_taps: [[(f32, f32); EARLY_TAPS]; 2],
    /// What each side's early reflections take of the middle's lows, and what it moves each frame.
    early_shared: [f32; 2],
    early_shared_by: f32,
    /// Each line's shelves' states.
    absorbing: [[[f32; 2]; 2]; LINES],
    /// The bands' tails, and each line's absorption for them at the size it was worked out for.
    bands: [Smoothed; 3],
    absorption: [Absorption; LINES],
    spreads: [f32; LINES],
    /// How much each side's input all-passes spread, by the same rule as the lines' all-passes.
    diffusions: [[f32; 7]; 2],
    gains_for: ([f32; 3], f32),
    /// How many frames until they may be worked out again.
    gains_wait: u32,
    /// The lines' delays' wobbles, then their all-passes', and how far they go at most, in frames.
    wobbles: [Wobble; WOBBLES],
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
        let most_wobble = MOST_WOBBLE * OVERSHOOT * rate as f32;
        let tank = std::array::from_fn(|line| {
            let delay_frames = DELAYS[line] as f32;
            let allpass_frames = LOOP_ALLPASSES[line] as f32;
            Line {
                delay: place(delay_frames * scale * LARGEST + most_wobble),
                delay_frames,
                allpass: place(allpass_frames * scale * LARGEST + most_wobble),
                allpass_frames,
            }
        });
        let mut diffuser = |frames: usize| {
            let length = ((frames as f32 * scale) as usize).max(1);
            (place(length as f32), length)
        };
        let diffusers = DIFFUSERS.map(|side| side.map(&mut diffuser));
        let early_diffusers = EARLY_DIFFUSERS.map(|side| side.map(&mut diffuser));
        // A power of two long, so a read wraps round by a mask: the early reflections read it
        // 128 times a frame.
        let pre_frames =
            (((LONGEST_PRE + EARLY_LAST * LARGEST) * rate as f32) as usize + 4).next_power_of_two();
        let early_taps =
            early_taps().map(|side| side.map(|(seconds, gain)| (seconds * rate as f32, gain)));
        let mut hall = Self {
            lines: SharedLines::new(base),
            tank,
            diffusers,
            early_diffusers,
            early_taps,
            early_shared: [0.0; 2],
            early_shared_by: of_corner(EARLY_SHARED_HZ, rate as f32),
            absorbing: [[[0.0; 2]; 2]; LINES],
            bands: [Smoothed::at(1.0); 3],
            absorption: [Absorption::default(); LINES],
            spreads: [LOOP_SPREAD; LINES],
            diffusions: [[DIFFUSION; 7]; 2],
            gains_for: ([f32::NAN; 3], f32::NAN),
            gains_wait: 0,
            wobbles: [Wobble::default(); WOBBLES],
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
        self.early_shared = [0.0; 2];
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
        let rate = f64::from(self.rate);
        let seconds = frames as f64 / rate;
        self.wobbles =
            std::array::from_fn(|which| Wobble::at(which, WOBBLE_HZ[which] / rate, seconds));
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
    /// all-passes' ringing and the band's together measured twice as long as the band. The input's
    /// all-passes, which don't scale with `Size`, no more than it keeps over four times theirs:
    /// seven in a row ring longer than one, and at `DIFFUSION` the longest alone rings for a third
    /// of a second; all seven at it measured a half-second tail a sixth long.
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
            let rate = self.rate as f32;
            self.diffusions = self.diffusers.map(|side| {
                side.map(|(_, frames)| {
                    DIFFUSION.min(0.001_f32.powf(4.0 * frames as f32 / (shortest * rate)))
                })
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
        let mask = self.pre[0].len() - 1;
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
            let a = line[self.pre_write.wrapping_sub(whole) & mask];
            let b = line[self.pre_write.wrapping_sub(whole + 1) & mask];
            a + (b - a) * fraction
        };
        let held = [read(&self.pre[0], back), read(&self.pre[1], back)];
        let mut early = [0.0; 2];
        if early_level > 0.0 {
            for (side, out) in early.iter_mut().enumerate() {
                *out = self.early_taps[side]
                    .iter()
                    .map(|(frames, gain)| read(&self.pre[side], back + frames * stretch) * gain)
                    .sum::<f32>()
                    * early_level;
            }
        }
        self.pre_write = (self.pre_write + 1) & mask;
        let wobbles: [f32; WOBBLES] =
            std::array::from_fn(|which| self.wobbles[which].next(which) * depth);

        let lines = &mut self.lines;
        lines.advance();
        // Each side spread by its all-passes, and its early reflections by theirs.
        let mut diffused = held.map(|sample| sample * INPUT_GAIN);
        for ((sample, side), spreads) in
            diffused.iter_mut().zip(self.diffusers).zip(self.diffusions)
        {
            for ((base, length), spread) in side.into_iter().zip(spreads) {
                *sample = lines.allpass(base, length, *sample, spread);
            }
        }
        for (sample, side) in early.iter_mut().zip(self.early_diffusers) {
            for (base, length) in side {
                *sample = lines.allpass(base, length, *sample, EARLY_DIFFUSION);
            }
        }
        // Each side's lows swapped for their middle's: a side plus the low-pass of what it lacks.
        let middle = (early[0] + early[1]) * std::f32::consts::FRAC_1_SQRT_2;
        for (sample, shared) in early.iter_mut().zip(&mut self.early_shared) {
            *shared += (middle - *sample - *shared) * self.early_shared_by;
            *sample += *shared;
        }

        // What each line gives now, read between samples where its wobble has it, with each
        // side's input added and through its all-pass, read where its own wobble has it. Each
        // side hears its own four lines' reads: what goes in cancels in their sum, so the tail
        // starts with the shortest line, every trip round already spread by the all-passes.
        let mut x = [0.0_f32; LINES];
        let mut out = early;
        for (line, into) in x.iter_mut().enumerate() {
            let Line {
                delay,
                delay_frames,
                allpass,
                allpass_frames,
            } = self.tank[line];
            let given = read_between(lines, delay, delay_frames * scale + wobbles[line]);
            let side = line / 4;
            out[side] += given * WET;
            let sample = given + diffused[side] * IN_SIGNS[line % 4];
            let spread = if line % 2 == 0 {
                spreads[line]
            } else {
                -spreads[line]
            };
            // An all-pass `frames` long reads `frames - 1` back before it writes.
            let read = read_between(
                lines,
                allpass,
                allpass_frames * scale - 1.0 + wobbles[LINES + line],
            );
            let sum = sample + read * spread;
            lines.write(allpass, 0, sum);
            *into = read - sum * spread;
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
    use crate::dsp::effects::tail::measure::{click_seconds, seconds};

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

    /// A click through a hall, its two sides added, `seconds` long.
    fn click(hall: &mut Hall, seconds: f32) -> Vec<f32> {
        impulse(hall, (seconds * RATE as f32) as usize)
            .iter()
            .map(|[left, right]| left + right)
            .collect()
    }

    /// When the early reflections are over at `Size` `80`, their all-passes' smear with them.
    const AFTER_EARLY: f32 = EARLY_LAST + 0.01;

    #[test]
    fn each_band_falls_in_its_own_time_as_its_tail_says() {
        // Lows, middle and highs set apart, measured at 125 Hz, 1 kHz and 8 kHz, well inside
        // their bands, against `band_seconds`; and the tail drawn against the same click at
        // `TAIL_HZ`. With early reflections, as it's heard: without them the tail starts with the
        // shortest line, 60 ms in, and a band that holds two times, as 1 kHz between long lows
        // and short highs, is fitted from 5 to 35 dB down later on its curve (2.8 s for 2.0 at
        // `Lows` and `Damp` `FF`). The highs are measured from when the early reflections are
        // over: most of their energy is in the first 30 ms, and against a highs' tail of 0.2 to
        // 0.4 s they're its first 15 dB, which the fit took as the tail (up to 17% long).
        // Seen: the bands within 11%, the drawing within 6%.
        for (lows, damp) in [(0x40, 0x00), (0x80, 0x80), (0xC0, 0xC0), (0xFF, 0xFF)] {
            let settings = Settings {
                lows,
                damp,
                early: Settings::DEFAULT.early,
                ..FLAT
            };
            let [low, middle, high] = band_seconds(settings);
            let response = click(&mut hall(settings, RATE), low.max(middle) * 1.3 + 0.5);
            let after_early = &response[(AFTER_EARLY * RATE as f32) as usize..];
            for (hertz, set, heard) in [
                (125.0, low, &response[..]),
                (1000.0, middle, &response[..]),
                (8000.0, high, after_early),
            ] {
                let measured = seconds(heard, RATE, hertz, 35.0);
                assert!(
                    (measured / set - 1.0).abs() < 0.15,
                    "{lows:02X} {damp:02X} at {hertz} Hz: {measured} for {set}"
                );
            }
            let said = tail(settings, RATE).unwrap();
            for (hertz, set) in TAIL_HZ.into_iter().zip(said.seconds) {
                let measured = seconds(&response, RATE, hertz, 35.0);
                assert!(
                    (measured / set - 1.0).abs() < 0.12,
                    "{lows:02X} {damp:02X} drawn at {hertz} Hz: {measured} for {set}"
                );
            }
        }
    }

    fn echo_density(out: &[[f32; 2]], at: f32) -> f32 {
        let window = RATE as usize / 50;
        let start = (at * RATE as f32) as usize - window / 2;
        let part: Vec<f32> = out[start..start + window].iter().map(|f| f[0]).collect();
        let spread = (part.iter().map(|x| x * x).sum::<f32>() / window as f32).sqrt();
        let outside = part.iter().filter(|x| x.abs() > spread).count();
        outside as f32 / window as f32 / 0.3173
    }

    #[test]
    fn it_is_as_dense_as_noise_soon_after_it_starts() {
        // From 50 ms with its early reflections; without them, from 60 ms after its shortest line
        // first comes round, 60 ms in.
        for (settings, from) in [(Settings::DEFAULT, 0.05), (FLAT, 0.12)] {
            let out = impulse(&mut hall(settings, RATE), RATE as usize / 2);
            for at in [from, 0.15, 0.2, 0.3, 0.4] {
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
                    // The early reflections at the top of `Early` and the smallest size add up
                    // to under 8 on full-scale noise.
                    assert!(
                        out.iter().all(|s| s.is_finite() && s.abs() < 12.0),
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
    fn its_wobble_moves_pitch_by_under_five_cents() {
        // A read moving bends the pitch by how fast it moves, in seconds a second, as a share of
        // the frequency: at the top of `Mod`, the steepest any of the sixteen goes in 200 s.
        let mut steepest = 0.0_f32;
        for (which, hertz) in WOBBLE_HZ.iter().enumerate() {
            let mut wobble = Wobble::at(which, hertz / f64::from(RATE), 0.0);
            let mut was = wobble.next(which);
            for _ in 0..RATE * 200 {
                let now = wobble.next(which);
                steepest = steepest.max((now - was).abs() * RATE as f32);
                was = now;
            }
        }
        let cents = 1200.0 * (1.0 + steepest * MOST_WOBBLE).log2();
        // Seen: 4.4.
        assert!(cents < 5.0, "{cents}");
    }

    #[test]
    fn its_wobbles_are_where_the_frame_count_says() {
        // One that's slept, moved on by the count, is where one that ran all along is.
        let mut ran = Wobble::at(3, WOBBLE_HZ[3] / f64::from(RATE), 0.0);
        for _ in 0..RATE * 7 {
            ran.next(3);
        }
        let mut slept = Wobble::at(3, WOBBLE_HZ[3] / f64::from(RATE), 7.0);
        for _ in 0..1000 {
            assert!((ran.next(3) - slept.next(3)).abs() < 1e-3);
        }
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
