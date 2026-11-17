//! Mutable Instruments' Beads as the module plays: what comes in, with what's fed back through
//! the module's limiter and saturation, is recorded all the time into a buffer of the module's
//! length for its quality, coded as the quality codes it, and either grains (`Granular`) or a
//! delay's taps (`Delay`) read it back. Then come the blend of what came in with that, the
//! cassette's wow and flutter, Beads' limiter, its reverb and its output limiter.
//!
//! It runs at the rate it's given rather than the module's 48 kHz, with every time the module
//! gives in samples scaled to it, so its buffers hold the module's seconds. Its four qualities
//! record at the module's rates for them, 48, 32 and 24 kHz scaled to this rate: the two tapes
//! through the module's own halving filter, as its `Sunny tape` does in its software, and
//! `Cold digital`, which the module gets by running its codec at 32 kHz, through a two-to-three
//! converter of this program's own. Each stores what it records as the module stores it, in 16
//! bits, in 12 or in 8-bit mu-law.
//!
//! Its grains are `Grains`', which are Beads' granular mode, played over this recording. Its
//! cassette's wow and flutter are `Tape`'s, which are Beads'. Its wavetable mode, for when
//! nothing is patched in, is left out.
//!
//! Adapted from Beads' `beads/dsp/processor.cc`, `granular_processor.h`, `delay_line.h`,
//! `delay_granular_player.h`, `delay_grain.h`, `granular_player.h`, `delay_controller.cc`,
//! `delay_clock.cc`, `multitap_delay.h`, `tap.h`, `trigger_generator.cc`,
//! `randomized_parameter.h`, `limiter.h`, `mu_law.h`, `sample_rate_conversion_filters.h`,
//! `fx/reverb.h`, `fx/fx_engine.h` and `fx/wow_and_flutter.h`, and stmlib's hysteresis filter and
//! quantizer, MIT:
//!
//! Copyright 2021 Emilie Gillet.
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

use vt_model::constants::{CUT_FADE_FRAMES, TICKS_PER_STEP};
use vt_model::insert::{Beads as Settings, BeadsMode, grain_length, shift_semitones};

use crate::dsp::blocks::clip::soft_clip;
use crate::dsp::blocks::line::{SharedLines, hermite};
use crate::dsp::blocks::plaits_parts::Svf;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::clouds::{mu_law_decode, mu_law_encode};
use crate::dsp::effects::granular::{GrainPlayer, Launch};
use crate::dsp::effects::memory::SongPlace;
use crate::dsp::effects::slot::{Mixing, Rung, SlotEffect};
use crate::dsp::effects::tape::Wobble;
use crate::rng::Pcg32;

/// The rate the module runs at, which its times in samples are for.
const BEADS_RATE: f32 = 48_000.0;
/// The rate the module records at in each quality, and how many stereo frames it holds in each:
/// 4 seconds at 16 bits, 8 at 12, about 10.7 at 12 and half the rate, and 16 at 8-bit mu-law.
const QUALITY_RATES: [f32; 4] = [48_000.0, 32_000.0, 24_000.0, 24_000.0];
const QUALITY_FRAMES: [usize; 4] = [192_000, 256_000, 256_000, 384_000];
const MOST_FRAMES: usize = 384_000;
/// How much the module's recording is of each quality's resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Resolution {
    Bits16,
    Bits12,
    MuLaw,
}
const RESOLUTIONS: [Resolution; 4] = [
    Resolution::Bits16,
    Resolution::Bits12,
    Resolution::Bits12,
    Resolution::MuLaw,
];
/// How near the recording's ends grains fade, and how far in from them they start, in the
/// module's samples; and how much of what comes in while frozen is kept to fade back in with.
const HEAD_FADE: f32 = 256.0;
const HEAD_SAFETY: f32 = 257.0;
const FREEZE_TAIL: f32 = 256.0;
/// How long the module takes to go between its modes, in seconds, and how long of that its wet
/// sound fades out and in.
const MODE_TRANSITION: f32 = 1.5;
const MODE_FADE: f32 = 0.75;
/// How loud the dry and wet sounds come out, as the module's, and what's taken off the whole so
/// the dry sound alone comes out as it went in.
const DRY_GAIN: f32 = 0.75;
const WET_GAIN: f32 = 1.5;
/// The fastest grains start, in Hz, and the clock's divisions below the middle of `Density`.
const MOST_GRAIN_HERTZ: f32 = 130.812_78;
const DIVISIONS: [u32; 8] = [1, 2, 3, 4, 6, 8, 12, 16];
/// How long without a step before the divider starts counting again, in seconds, and how close
/// steps come before it follows them as an oscillator.
const DIVIDER_RESET_SECONDS: f32 = 4.0;
const AUDIO_RATE_SECONDS: f32 = 1.0 / 48.0;
/// The delay's shortest, in the module's samples, its taps' pitch-shifting window, and the
/// ratios of the clock its `Density` chooses: powers of two below the middle, other divisions
/// above.
const MIN_DELAY: f32 = 68.0;
const TAP_WINDOW: f32 = 2048.0;
const RATIOS: [f32; 22] = [
    1.0 / 16.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0 / 2.0,
    1.0,
    1.0,
    1.0,
    1.0,
    1.0 / 2.0,
    1.0 / 3.0,
    1.0 / 4.0,
    1.0 / 6.0,
    1.0 / 8.0,
    1.0 / 10.0,
    1.0 / 12.0,
    1.0 / 16.0,
    1.0 / 16.0,
    1.0 / 16.0,
];
const INTERVALS: usize = 2;
const TAP_RATIOS: usize = 2;
const HEADS: usize = INTERVALS * TAP_RATIOS;
const TAPS: usize = HEADS * 2;
/// The cassette's wow and flutter, in the module's samples at its 24 kHz, its line's length, and
/// the low-pass after it there.
const WOW_SAMPLES: f32 = 105.0;
const FLUTTER_SAMPLES: f32 = 17.0;
const WOW_LINE: f32 = 128.0;
const WOW_LOW_PASS: f32 = 0.95;
const WOW_RATE: f32 = 24_000.0;
/// The reverb's lines in samples at the rate the module runs it, which is the quality's: four
/// diffusers, then each side's two allpasses and delay.
const REVERB_LENGTHS: [f32; 10] = [
    150.0, 214.0, 319.0, 527.0, 2182.0, 2690.0, 4501.0, 2525.0, 2197.0, 6312.0,
];
const REVERB_DIFFUSION: f32 = 0.625;
const REVERB_WOBBLE_HERTZ: [f64; 2] = [0.5, 0.3];
/// Beads' halving and doubling filters, `SRC_FIR<SRC_DOWN, 2, 48>` and `SRC_FIR<SRC_UP, 2, 48>`,
/// whose 48 taps are these and the same again backwards.
#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
const HALVING: [f32; 24] = [
    -2.143728787e-04,
    -4.113243578e-04,
    3.550026182e-04,
    1.123588805e-03,
    2.013790146e-04,
    -2.135889966e-03,
    -1.946536068e-03,
    2.421957824e-03,
    5.099674271e-03,
    -4.847193956e-04,
    -8.638296788e-03,
    -5.137744633e-03,
    1.000597441e-02,
    1.464501735e-02,
    -5.419339385e-03,
    -2.582247866e-02,
    -9.106955536e-03,
    3.335521514e-02,
    3.702172341e-02,
    -2.802414194e-02,
    -8.416883646e-02,
    -1.192656678e-02,
    1.955030744e-01,
    3.837045956e-01,
];
#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
const DOUBLING: [f32; 24] = [
    -4.287457573e-04,
    -8.226487157e-04,
    7.100052364e-04,
    2.247177610e-03,
    4.027580292e-04,
    -4.271779932e-03,
    -3.893072135e-03,
    4.843915649e-03,
    1.019934854e-02,
    -9.694387912e-04,
    -1.727659358e-02,
    -1.027548927e-02,
    2.001194882e-02,
    2.929003471e-02,
    -1.083867877e-02,
    -5.164495731e-02,
    -1.821391107e-02,
    6.671043028e-02,
    7.404344683e-02,
    -5.604828388e-02,
    -1.683376729e-01,
    -2.385313356e-02,
    3.910061488e-01,
    7.674091912e-01,
];
/// The two-to-three converter's taps, at twice the rate, and how far its Kaiser window bends.
const THIRDS_TAPS: usize = 192;
const THIRDS_BETA: f64 = 8.0;

/// What reaching a value at `per_sample` a sample takes a frame, with so many frames a sample.
fn per_frame(per_sample: f32, frames: f32) -> f32 {
    1.0 - (1.0 - per_sample).powf(1.0 / frames.max(1e-3))
}

/// A smooth step from 0 to 1.
#[inline]
fn smooth_step(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// stmlib's `Interpolate`: a table read between its entries, at a share of its length.
#[inline]
fn interpolate(table: &[f32], at: f32, size: f32) -> f32 {
    let at = at * size;
    let whole = (at as usize).min(table.len() - 2);
    let between = at - whole as f32;
    table[whole] + (table[whole + 1] - table[whole]) * between
}

/// The start of `-ln(1 - x)`'s series, as Beads uses for its random intervals.
#[inline]
fn log_one_minus(x: f32) -> f32 {
    -x * (1.0 + x * (0.5 + x * (0.3333 + x * 0.25)))
}

/// A sample as 16 bits, as the module's `Clip16` of it.
#[inline]
fn clip16(sample: f32) -> i16 {
    ((sample * 32_768.0) as i32).clamp(-32_768, 32_767) as i16
}

/// Moving towards a value faster up than down, as stmlib's `SLOPE`.
#[inline]
fn slope(value: &mut f32, to: f32, up: f32, down: f32) {
    let error = to - *value;
    *value += if error > 0.0 { up } else { down } * error;
}

/// Beads' limiter: a fast peak follower that lets go slowly turns it down past a ceiling.
#[derive(Debug, Clone, Copy)]
struct Limiter {
    peak: f32,
    peak_smoothed: f32,
    up: [f32; 2],
    down: f32,
    ceiling: f32,
}

impl Limiter {
    /// One that lets go over so many seconds, at a rate, where each of the module's samples is so
    /// many frames.
    fn new(seconds: f32, ceiling: f32, rate: f32, frames: f32) -> Self {
        Self {
            peak: 0.5,
            peak_smoothed: 0.5,
            up: [per_frame(0.1, frames), per_frame(0.5, frames)],
            down: 1.0 / (rate * seconds),
            ceiling,
        }
    }

    #[inline]
    fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let peak = frame[0].abs().max(frame[1].abs()).min(40.0);
        slope(&mut self.peak, peak, self.up[0], self.down);
        let smoothed = self.peak;
        slope(&mut self.peak_smoothed, smoothed, self.up[1], self.down);
        let gain = if self.peak_smoothed <= self.ceiling {
            1.0
        } else {
            self.ceiling / self.peak_smoothed
        };
        frame.map(|side| side * gain)
    }
}

/// stmlib's `HysteresisQuantizer2`: a value from 0 to 1 as one of so many steps, which sticks a
/// little either side of where it changes.
#[derive(Debug, Clone, Copy)]
struct Quantizer {
    steps: usize,
    hysteresis: f32,
    value: usize,
}

impl Quantizer {
    fn new(steps: usize, hysteresis: f32) -> Self {
        Self {
            steps: steps.max(1),
            hysteresis,
            value: 0,
        }
    }

    #[inline]
    fn process(&mut self, value: f32) -> usize {
        let value = value * self.steps as f32 - 0.5;
        let sign = if value > self.value as f32 { -1.0 } else { 1.0 };
        let quantized = (value + sign * self.hysteresis + 0.5) as i32;
        self.value = quantized.clamp(0, self.steps as i32 - 1) as usize;
        self.value
    }
}

/// stmlib's `HysteresisFilter`: a value that only moves once it's this far from where it is.
#[derive(Debug, Clone, Copy)]
struct Hysteresis {
    value: f32,
    threshold: f32,
}

impl Hysteresis {
    fn new(threshold: f32) -> Self {
        Self {
            value: 0.0,
            threshold,
        }
    }

    #[inline]
    fn process(&mut self, value: f32) -> f32 {
        let error = value - self.value;
        if error > self.threshold {
            self.value = value - self.threshold;
        } else if error < -self.threshold {
            self.value = value + self.threshold;
        }
        self.value
    }
}

/// Beads' random walk, which its attenuverters give a knob in its delay mode: a smooth step to a
/// new place at random each time round.
#[derive(Debug, Clone, Copy)]
struct Walk {
    phase: f32,
    hertz: f32,
    from: f32,
    to: f32,
}

impl Walk {
    fn new(hertz: f32) -> Self {
        Self {
            phase: 0.0,
            hertz,
            from: 0.0,
            to: 1.0,
        }
    }

    /// Where it is a frame on, at a rate, faster as there's some of it.
    #[inline]
    fn next(&mut self, amount: f32, rate: f32, rng: &mut Pcg32) -> f32 {
        self.phase += if amount <= 0.0 { 0.25 } else { 4.0 } * self.hertz / rate;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.from = self.to;
            self.to = 2.0 * rng.unit() - 1.0;
        }
        self.from + (self.to - self.from) * smooth_step(self.phase)
    }
}

/// A converter from one rate to another `up` over `down` times it, which puts zeros between what
/// comes in, filters it at the rate between, and keeps every so many: `up` and `down` are 1 and 2
/// to halve, 2 and 1 to double, 2 and 3 for two thirds and 3 and 2 back. The filter's taps add up
/// to `up`, so it keeps its level. Its taps are kept a phase at a time, and what it's heard twice
/// over, so each frame out is one run along each.
#[derive(Debug, Clone)]
struct Converter {
    /// Each phase's taps, `held` of them, one phase after another.
    banks: Vec<f32>,
    held: usize,
    up: usize,
    down: usize,
    history: Vec<[f32; 2]>,
    newest: usize,
    /// How far, at the rate between, the next frame out is from the newest in.
    phase: usize,
}

impl Converter {
    /// One with nothing in it, which makes no frames.
    const EMPTY: Self = Self {
        banks: Vec::new(),
        held: 0,
        up: 0,
        down: 0,
        history: Vec::new(),
        newest: 0,
        phase: 0,
    };

    fn new(taps: &[f32], up: usize, down: usize) -> Self {
        let held = taps.len().div_ceil(up);
        let banks = (0..up)
            .flat_map(|phase| {
                (0..held).map(move |at| taps.get(phase + at * up).copied().unwrap_or(0.0))
            })
            .collect();
        Self {
            banks,
            held,
            up,
            down,
            history: vec![[0.0; 2]; 2 * held],
            newest: 0,
            phase: 0,
        }
    }

    /// Beads' 48 taps from their first half.
    fn symmetric(half: &[f32]) -> Vec<f32> {
        half.iter().chain(half.iter().rev()).copied().collect()
    }

    fn clear(&mut self) {
        self.history.fill([0.0; 2]);
        self.newest = 0;
        self.phase = 0;
    }

    /// Take a frame in, and hand on each frame that's due out because of it.
    #[inline]
    fn push(&mut self, frame: [f32; 2], mut out: impl FnMut([f32; 2])) {
        let held = self.held;
        self.newest = if self.newest == 0 {
            held - 1
        } else {
            self.newest - 1
        };
        self.history[self.newest] = frame;
        self.history[self.newest + held] = frame;
        while self.phase < self.up {
            let bank = &self.banks[self.phase * held..(self.phase + 1) * held];
            let heard = &self.history[self.newest..self.newest + held];
            let mut sum = [0.0; 2];
            for (tap, frame) in bank.iter().zip(heard) {
                sum[0] += frame[0] * tap;
                sum[1] += frame[1] * tap;
            }
            out(sum);
            self.phase += self.down;
        }
        self.phase -= self.up;
    }
}

/// A low-pass for going between a rate and two thirds of it at twice the rate, through a Kaiser
/// window: it passes up to about 0.28 of the rate, 13.5 kHz at 48 kHz, and stops by a third of
/// it, the slower rate's highest, as a codec running at 32 kHz does. Its taps add up to `gain`.
fn thirds_filter(gain: f32) -> Vec<f32> {
    let bessel = |x: f64| {
        let (mut sum, mut term) = (1.0, 1.0);
        for k in 1..32 {
            term *= (x / 2.0) / f64::from(k);
            sum += term * term;
        }
        sum
    };
    // A third of the rate is a sixth of twice it; the cutoff sits half the transition below.
    let transition = (80.0 - 7.95) / (14.36 * THIRDS_TAPS as f64);
    let cutoff = 1.0 / 6.0 - transition / 2.0;
    let middle = (THIRDS_TAPS - 1) as f64 / 2.0;
    let taps: Vec<f64> = (0..THIRDS_TAPS)
        .map(|at| {
            let x = at as f64 - middle;
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (std::f64::consts::TAU * cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            let window = (1.0 - (x / middle).powi(2)).max(0.0).sqrt();
            sinc * bessel(THIRDS_BETA * window) / bessel(THIRDS_BETA)
        })
        .collect();
    let sum: f64 = taps.iter().sum();
    taps.iter()
        .map(|tap| (tap / sum * f64::from(gain)) as f32)
        .collect()
}

/// A rate and back: each frame in goes down to the recording's rate, which may hand on none, one
/// or two to be played, and what's played comes back up, a frame late so there's always one
/// ready.
#[derive(Debug, Clone)]
struct Rates {
    down: Converter,
    up: Converter,
    ready: [[f32; 2]; 8],
    first: usize,
    count: usize,
}

impl Rates {
    fn new(down: Converter, up: Converter) -> Self {
        let mut rates = Self {
            down,
            up,
            ready: [[0.0; 2]; 8],
            first: 0,
            count: 0,
        };
        rates.clear();
        rates
    }

    fn clear(&mut self) {
        self.down.clear();
        self.up.clear();
        self.ready = [[0.0; 2]; 8];
        self.first = 0;
        self.count = 1;
    }
}

/// One frame of where a delay tap reads and how loud, in the recording's frames.
#[derive(Debug, Clone, Copy, Default)]
struct Tap {
    delay: f32,
    gain: f32,
}

/// One of the delay's play heads, which reads a slice of the recording while it's frozen.
#[derive(Debug, Clone, Copy, Default)]
struct PlayHead {
    start: f32,
    remaining: u32,
    middle: u32,
    phase: f32,
    step: f32,
    gain: f32,
}

/// A triangle from 0 up to 1 at `width` and back down.
#[inline]
fn triangle(phase: f32, width: f32) -> f32 {
    if phase < width {
        phase / width
    } else {
        1.0 - (phase - width) / (1.0 - width)
    }
}

/// The widths of the delay's envelope's triangle along its `Shape`.
const ENVELOPE_WIDTHS: [f32; 6] = [0.0, 0.001, 0.01, 0.5, 0.75, 0.999];

impl PlayHead {
    fn done(&self) -> bool {
        self.remaining == 0
    }

    fn half_way(&self) -> bool {
        self.remaining != 0 && self.remaining == self.middle
    }

    fn start(&mut self, start: f32, frames: u32, gain: f32) {
        self.start = start;
        self.remaining = frames;
        self.middle = frames >> 1;
        self.phase = 0.0;
        self.step = 1.0 / frames.max(1) as f32;
        self.gain = gain;
    }

    #[inline]
    fn render(&mut self, shape: f32, follower: bool) -> Tap {
        if self.done() {
            return Tap::default();
        }
        let envelope = if follower {
            let shapes = [0.0, 0.0, 0.0, triangle(self.phase, 0.5), 0.0, 0.0];
            interpolate(&shapes, shape, 5.0)
        } else if shape < 0.2 {
            let square = (100.0 * triangle(self.phase, 0.5)).min(1.0);
            let ramp = triangle(self.phase, ENVELOPE_WIDTHS[1]);
            square + (ramp - square) * shape * 5.0
        } else {
            triangle(self.phase, interpolate(&ENVELOPE_WIDTHS, shape, 5.0))
        };
        let tap = Tap {
            delay: self.start + self.remaining as f32,
            gain: self.gain * envelope,
        };
        self.remaining -= 1;
        self.phase += self.step;
        tap
    }
}

/// The envelope on each of the delay's repeats, which goes round once a repeat.
#[derive(Debug, Clone, Copy, Default)]
struct TapEnvelope {
    phase: f32,
    step: f32,
}

impl TapEnvelope {
    #[inline]
    fn render(&mut self, shape: f32) -> f32 {
        let envelope = if shape < 0.2 {
            let ramp = triangle(self.phase, ENVELOPE_WIDTHS[1]);
            1.0 + (ramp - 1.0) * shape * 5.0
        } else {
            triangle(self.phase, interpolate(&ENVELOPE_WIDTHS, shape, 5.0))
        };
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        envelope
    }
}

/// Beads' delay clock: the time between the clock's pulses, the one before and the fade from it,
/// or, going by itself, the time `Density` sets.
#[derive(Debug, Clone, Copy)]
struct DelayClock {
    intervals: [f32; 2],
    average: f32,
    fade: f32,
    fade_step: f32,
    fastest_fade: f32,
    slowest_fade: f32,
    jitter: f32,
    sensitivity: f32,
    change: bool,
    since: u32,
    longest: u32,
    rotate: usize,
}

impl DelayClock {
    fn new(rate: f32) -> Self {
        Self {
            intervals: [rate * 0.25; 2],
            average: rate * 0.25,
            fade: 1.0,
            fade_step: 1.0 / (rate * 0.02),
            fastest_fade: 1.0 / (rate * 0.02),
            slowest_fade: 1.0 / (rate * 0.25),
            jitter: rate * 0.001,
            sensitivity: 1.0,
            change: false,
            since: 0,
            longest: (rate * 4.0) as u32,
            rotate: 0,
        }
    }

    fn update(&mut self, interval: f32, coefficient: f32) {
        self.intervals[0] += coefficient * (interval - self.intervals[0]);
        self.intervals[1] = self.intervals[0];
        self.fade = 1.0;
        self.rotate = 0;
    }

    fn interval(&self, at: usize) -> f32 {
        self.intervals[(at + self.rotate) % 2]
    }

    fn gain(&self, at: usize) -> f32 {
        if (at + self.rotate) % 2 == 1 {
            self.fade
        } else {
            1.0 - self.fade
        }
    }

    fn retrig(&self) -> bool {
        self.fade == 0.0 && self.since == 0
    }

    fn new_interval(&self) -> usize {
        (self.rotate + 1) % 2
    }

    /// A frame on, with or without a pulse of the clock.
    fn process(&mut self, tick: bool) {
        self.since += 1;
        self.fade = (self.fade + self.fade_step).min(1.0);
        if tick {
            if self.since < self.longest {
                let interval = self.since as f32;
                let big = (interval - self.intervals[1]).abs() > self.jitter;
                let small =
                    (self.average - self.intervals[1]).abs() > self.sensitivity * self.jitter;
                if big {
                    self.sensitivity = 1.0;
                    self.average = interval;
                } else {
                    self.average += 0.1 * (interval - self.average);
                    self.sensitivity = (self.sensitivity * 0.95).max(0.05);
                }
                self.change = self.change || big || small;
            }
            self.since = 0;
        }
        if self.change && self.fade >= 1.0 {
            self.rotate += 1;
            self.intervals[0] = self.intervals[1];
            self.intervals[1] = self.average;
            self.fade = 0.0;
            self.fade_step = (2.0 / self.average).clamp(self.slowest_fade, self.fastest_fade);
            self.change = false;
        }
    }
}

/// Beads' delay controller: where each of its eight taps reads and how loud, from its clock,
/// `Density`, `Time` and `Shape`, and what loops while it's frozen.
#[derive(Debug, Clone)]
struct DelayController {
    longest: f32,
    shortest: f32,
    clock: DelayClock,
    heads: [PlayHead; HEADS],
    followers: [PlayHead; HEADS],
    slices: [Quantizer; HEADS],
    envelopes: [TapEnvelope; TAPS],
    position_walk: Walk,
    shape_walk: Walk,
    /// The slow wobble of its odd tap's level, its phase and how far it goes each frame.
    odd_phase: f32,
    odd_step: f32,
    density_filter: Hysteresis,
    slice_filter: Hysteresis,
    multiplier: f32,
    multiplier_coefficient: f32,
    shape: f32,
    frozen: bool,
    taps: [Tap; TAPS],
}

impl DelayController {
    /// At the recording's rate, with its frames, and so many frames for each of the module's
    /// samples.
    fn new(rate: f32, longest: f32, scale: f32) -> Self {
        Self {
            longest,
            shortest: MIN_DELAY * scale,
            clock: DelayClock::new(rate),
            heads: [PlayHead::default(); HEADS],
            followers: [PlayHead::default(); HEADS],
            slices: [Quantizer::new(2, 0.05); HEADS],
            envelopes: [TapEnvelope::default(); TAPS],
            position_walk: Walk::new(0.1),
            shape_walk: Walk::new(0.1),
            odd_phase: 0.0,
            odd_step: 0.17 / rate,
            density_filter: Hysteresis::new(0.0025),
            slice_filter: Hysteresis::new(0.0025),
            multiplier: 0.0,
            multiplier_coefficient: per_frame(0.01, scale),
            shape: 0.0,
            frozen: false,
            taps: [Tap::default(); TAPS],
        }
    }

    /// The taps for a frame: following the clock's pulses or going by itself, frozen or not.
    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        knobs: &Knobs,
        clocked: bool,
        tick: bool,
        frozen: bool,
        rate: f32,
        rng: &mut Pcg32,
    ) {
        let random = knobs.random;
        let slice = 1.005
            * (knobs.time + random * self.position_walk.next(random, rate, rng)).clamp(0.0, 1.0);
        let shape = (knobs.shape + random * self.shape_walk.next(random, rate, rng))
            .clamp(0.0, 1.0)
            * 0.995;
        self.shape = shape;
        let density = self.density_filter.process(knobs.density.clamp(0.0, 1.0));

        let mut ratios = [0.0; TAP_RATIOS];
        let mut gains = [0.0; TAP_RATIOS];
        if clocked {
            self.clock.process(tick);
            let ratio = density * 20.0;
            let whole = (ratio as usize).min(RATIOS.len() - 2);
            let fraction = ((ratio - whole as f32 - 0.5) * if density < 0.5 { 4.0 } else { 1.5 }
                + 0.5)
                .clamp(0.0, 1.0);
            let flip = whole & 1;
            ratios[flip] = RATIOS[whole];
            ratios[1 - flip] = RATIOS[whole + 1];
            gains[flip] = 1.0 - fraction;
            gains[1 - flip] = fraction;
        } else {
            let interval =
                (self.longest * (-(density - 0.5).abs() * 18.0).exp2()).max(self.shortest);
            let coefficient = (10.0 / interval).clamp(0.005, 0.7);
            self.clock.update(interval, coefficient);
            ratios = [1.0, 0.77];
            self.odd_phase += self.odd_step;
            if self.odd_phase >= 1.0 {
                self.odd_phase -= 1.0;
            }
            let wobble = 0.5 + 0.5 * (self.odd_phase * TAU).sin();
            let odd = ((density - 0.5) * 25.0).clamp(0.0, 1.0) * (0.25 + 0.75 * wobble);
            gains = [1.0 - odd * 0.2, odd * 0.8];
        }

        if frozen {
            if !self.frozen {
                self.heads = [PlayHead::default(); HEADS];
                self.followers = [PlayHead::default(); HEADS];
            }
            if self.clock.retrig() {
                let first = self.clock.new_interval() * TAP_RATIOS;
                for head in first..first + TAP_RATIOS {
                    self.heads[head] = PlayHead::default();
                    self.followers[head] = PlayHead::default();
                }
            }
            let slice = self.slice_filter.process(slice);
            for head in 0..HEADS {
                let interval = head / TAP_RATIOS;
                let ratio = head % TAP_RATIOS;
                let starting = if self.heads[head].done() {
                    Some(false)
                } else if self.heads[head].half_way() {
                    Some(true)
                } else {
                    None
                };
                if let Some(follower) = starting {
                    let frames = ((self.clock.interval(interval) * ratios[ratio]) as u32).max(1);
                    let count = ((self.longest as u32 / frames) as usize).max(1);
                    if self.slices[head].steps != count {
                        self.slices[head] = Quantizer::new(count, 0.05 / 5.0 * count as f32);
                    }
                    let index = self.slices[head].process(slice) as f32;
                    let gain = gains[ratio] * self.clock.gain(interval);
                    let start = frames as f32 * index + self.shortest;
                    if follower {
                        self.followers[head].start(start, frames, gain);
                    } else {
                        self.heads[head].start(start, frames, gain);
                    }
                }
                self.taps[2 * head] = self.heads[head].render(shape, false);
                self.taps[2 * head + 1] = self.followers[head].render(shape, true);
            }
        } else {
            if self.frozen {
                self.envelopes = [TapEnvelope::default(); TAPS];
            }
            if self.clock.retrig() {
                let first = self.clock.new_interval() * HEADS;
                for envelope in &mut self.envelopes[first..first + HEADS] {
                    *envelope = TapEnvelope::default();
                }
            }
            self.multiplier += self.multiplier_coefficient * (slice - self.multiplier);
            let multiplier = (self.multiplier * 15.0 + 1.0).clamp(1.0, 16.0);
            let whole = multiplier as usize;
            let fraction = (0.5 + 8.0 * (multiplier - whole as f32 - 0.5)).clamp(0.0, 1.0);
            for t in 0..TAPS {
                let interval = t / HEADS;
                let ratio = (t / INTERVALS) % TAP_RATIOS;
                let tap = t & 1;
                let at = 2 * ratio + if whole & 1 == 1 { tap } else { 1 - tap } + interval * HEADS;
                let delay = (self.clock.interval(interval) * ratios[ratio] * (whole + tap) as f32)
                    .clamp(self.shortest, self.longest);
                self.envelopes[at].step = 1.0 / delay;
                let fade = if tap == 1 { fraction } else { 1.0 - fraction };
                let gain = gains[ratio] * fade * self.clock.gain(interval);
                self.taps[at] = Tap {
                    delay,
                    gain: gain * self.envelopes[at].render(shape),
                };
            }
        }
        self.frozen = frozen;
    }
}

/// When grains start, as Beads' trigger generator does: by itself, evenly or at random, or from
/// the clock's pulses, a division of them or a chance of each.
#[derive(Debug, Clone, Copy)]
struct Seed {
    phase: f32,
    /// How much longer or shorter than the average the interval is now, when it's at random.
    ratio: f32,
    /// Following the clock: how fast it thinks pulses come, where that's going, and how fast it
    /// gets there.
    frequency: f32,
    target: f32,
    glide: f32,
    division: u32,
    audio_rate: bool,
    divider: Quantizer,
    accept: bool,
    count: u32,
    since: u32,
    reset_after: u32,
    default_reset: u32,
    audio_rate_interval: u32,
    one_hertz: f32,
}

/// A grain to start, a share of a frame late, and how regularly they're coming, from 0 to 1.
#[derive(Debug, Clone, Copy)]
struct Fire {
    late: f32,
    regularity: f32,
}

impl Seed {
    fn new(rate: f32) -> Self {
        let since = (rate / 2.0) as u32;
        Self {
            phase: 0.0,
            ratio: 1.0,
            frequency: 1.0 / since as f32,
            target: 1.0 / since as f32,
            glide: 1.0,
            division: 1,
            audio_rate: false,
            divider: Quantizer::new(DIVISIONS.len(), 0.075),
            accept: true,
            count: 0,
            since,
            reset_after: (rate * DIVIDER_RESET_SECONDS) as u32,
            default_reset: (rate * DIVIDER_RESET_SECONDS) as u32,
            audio_rate_interval: (rate * AUDIO_RATE_SECONDS) as u32,
            one_hertz: 1.0 / rate,
        }
    }

    /// By itself: a pulse train, evenly below the middle of `Density` and at random above it,
    /// none in the middle.
    #[inline]
    fn free(&mut self, density: f32, rate: f32, rng: &mut Pcg32) -> Option<Fire> {
        let most = MOST_GRAIN_HERTZ / rate;
        let (frequency, random) = if density < 0.48 {
            (most * (-density * 16.0).exp2(), false)
        } else if density > 0.52 {
            (0.5 * most * (-(1.0 - density) * 16.0).exp2(), true)
        } else {
            return None;
        };
        if !random {
            self.ratio = 1.0;
        }
        let step = (frequency * self.ratio).clamp(0.000_001, 0.25);
        self.phase += step;
        if self.phase < 1.0 {
            return None;
        }
        self.phase -= 1.0;
        let late = self.phase / step;
        if random {
            self.ratio = -1.0 / log_one_minus(0.1 + 0.9 * rng.unit());
        }
        Some(Fire {
            late,
            regularity: if random { 0.0 } else { 1.0 },
        })
    }

    /// Following the clock's pulses: below the middle of `Density` every so many of them, from
    /// every one to every sixteenth, and above it each one by a chance that grows to all of them,
    /// none in the middle. A pulse that's a share of a frame late starts its grain as late.
    #[inline]
    fn clocked(&mut self, pulse: Option<f32>, density: f32, rng: &mut Pcg32) -> Option<Fire> {
        let division = if density < 0.48 {
            DIVISIONS[self.divider.process(density * 2.15)]
        } else {
            1
        };
        let chance = if density < 0.48 {
            1.01
        } else {
            ((density - 0.52) * 2.15).max(0.0)
        };
        let chance = chance * chance;
        let regularity = if chance >= 1.0 { 1.0 } else { 0.0 };
        self.frequency += self.glide * (self.target - self.frequency);
        self.phase += self.frequency;
        if let Some(_late) = pulse {
            if self.since >= self.reset_after {
                self.count = 0;
                self.phase = 0.0;
            }
            self.reset_after = self.default_reset.max(4 * self.since);
            let threshold = self.audio_rate_interval + if self.audio_rate { 8 } else { 0 };
            self.audio_rate = self.since < threshold && chance >= 1.0;
            self.accept = rng.unit() < chance && self.count.is_multiple_of(division);
            if self.accept {
                let frequency = 1.0 / self.since.max(1) as f32;
                self.target = if chance >= 1.0 {
                    frequency / division as f32
                } else {
                    frequency * chance
                };
                if self.audio_rate {
                    let mut no_glide = division != self.division;
                    self.division = division;
                    let up = (1.02 + 2.0 * frequency) * self.frequency;
                    let down = (0.98 - 2.0 * frequency) * self.frequency;
                    no_glide |= self.target > up || self.target < down;
                    // Jitter is heard more at high frequencies, so the faster pulses come, the
                    // slower it glides.
                    self.glide = if no_glide {
                        1.0
                    } else {
                        self.since as f32 * self.one_hertz
                    };
                } else {
                    self.phase = 0.0;
                    self.glide = 1.0;
                }
            } else if chance < 1.0 {
                self.target *= 1.0 - chance;
            }
            self.count = (self.count + 1) % 48;
            self.since = 0;
        }
        self.since += 1;
        if self.audio_rate {
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                let late = self.phase / self.frequency.max(1e-9);
                return Some(Fire { late, regularity });
            }
            None
        } else {
            self.phase = self.phase.min(1.0);
            match pulse {
                Some(late) if self.accept => Some(Fire { late, regularity }),
                _ => None,
            }
        }
    }
}

/// The song's steps as the clock Beads follows, worked out from where the song is at the start of
/// each block and moved on frame by frame between: a pulse on each step.
#[derive(Debug, Clone, Copy)]
struct Steps {
    tick: f64,
    /// How far the song goes a frame at the recording's rate.
    ticks_per_frame: f64,
    playing: bool,
}

impl Steps {
    const STOPPED: Self = Self {
        tick: -1.0,
        ticks_per_frame: 0.0,
        playing: false,
    };

    /// Where the song is now, as its tick, whether it's playing, and how far it goes a frame. A
    /// jump goes there, just before it, so a song played from a step starts on that step.
    #[allow(clippy::float_cmp, reason = "both are whole numbers of steps")]
    fn sync(&mut self, tick: f64, playing: bool, ticks_per_frame: f64) {
        self.playing = playing;
        self.ticks_per_frame = ticks_per_frame;
        let step = TICKS_PER_STEP as f64;
        if (tick - self.tick).abs() > ticks_per_frame.max(1e-9) * 4.0 + 1e-6 {
            self.tick = tick - 1e-6;
        } else if (tick / step).floor() == (self.tick / step).floor() {
            self.tick = tick;
        }
    }

    /// A frame on: whether a step starts in it, and how far through the frame.
    #[inline]
    fn next(&mut self) -> Option<f32> {
        if !self.playing {
            return None;
        }
        let step = TICKS_PER_STEP as f64;
        let before = self.tick;
        self.tick += self.ticks_per_frame;
        ((self.tick / step).floor() > (before / step).floor()).then(|| {
            let into = self.tick - (self.tick / step).floor() * step;
            (into / self.ticks_per_frame).clamp(0.0, 1.0) as f32
        })
    }
}

/// The settings for a frame of the recording, from 0 to 1 but `semitones`.
#[derive(Debug, Clone, Copy, Default)]
struct Knobs {
    time: f32,
    size: f32,
    shape: f32,
    semitones: f32,
    density: f32,
    random: f32,
    feedback: f32,
}

/// Beads' buffer of what it's recorded, both sides, stored as what each plays back as in its
/// quality's resolution. It's sized for the longest any quality records, and a new quality starts
/// a new recording in it; what hasn't been recorded since reads as silence.
#[derive(Debug, Clone)]
struct Recording {
    frames: Vec<[i16; 2]>,
    /// How many it holds now, past which grains can't read, and the line's length, a little more.
    size: usize,
    length: usize,
    resolution: Resolution,
    /// Where the newest is, and how many have been written since it started, at most all of them.
    write: usize,
    filled: usize,
    /// What came in just after it froze, and how much of it is left to fade the recording back
    /// in with, of how much.
    tail: Vec<[f32; 2]>,
    tail_written: usize,
    tail_fade: usize,
    was_frozen: bool,
}

impl Recording {
    fn new(most: usize, tail: usize) -> Self {
        Self {
            frames: vec![[0; 2]; most + 4],
            size: most,
            length: most + 4,
            resolution: Resolution::Bits16,
            write: 0,
            filled: 0,
            tail: vec![[0.0; 2]; tail.max(1)],
            tail_written: 0,
            tail_fade: 0,
            was_frozen: false,
        }
    }

    fn reset(&mut self, size: usize, resolution: Resolution) {
        self.size = size.min(self.frames.len() - 4);
        self.length = self.size + 4;
        self.resolution = resolution;
        self.write = 0;
        self.filled = 0;
        self.tail_written = 0;
        self.tail_fade = 0;
        self.was_frozen = false;
    }

    /// A sample as it plays back once it's stored.
    #[inline]
    fn stored(&self, sample: f32) -> i16 {
        let pcm = clip16(sample);
        match self.resolution {
            Resolution::Bits16 => pcm,
            Resolution::Bits12 => (pcm >> 4) << 4,
            Resolution::MuLaw => mu_law_decode(mu_law_encode(pcm)),
        }
    }

    /// Record a frame, unless it's frozen, as `Grains` does: just after freezing it keeps a little
    /// of what comes in, and fades from that into what's coming in when it records again.
    #[inline]
    fn record(&mut self, mut input: [f32; 2], frozen: bool) {
        if frozen {
            if !self.was_frozen {
                self.tail_written = 0;
            }
            if self.tail_written < self.tail.len() {
                self.tail[self.tail_written] = input;
                self.tail_written += 1;
            }
        } else {
            if self.was_frozen && self.tail_written > 0 {
                self.tail_fade = self.tail_written;
            }
            if self.tail_written > 0 {
                let kept = self.tail[self.tail_fade - self.tail_written];
                let gain = smooth_step(self.tail_written as f32 / self.tail_fade as f32);
                for (sample, kept) in input.iter_mut().zip(kept) {
                    *sample += (kept - *sample) * gain;
                }
                self.tail_written -= 1;
            }
            self.write += 1;
            if self.write >= self.length {
                self.write = 0;
            }
            self.frames[self.write] = input.map(|sample| self.stored(sample));
            self.filled = (self.filled + 1).min(self.length);
        }
        self.was_frozen = frozen;
    }

    /// How many frames back grains can reach: what's been recorded, at most all it holds.
    fn duration(&self) -> f64 {
        self.filled.min(self.size) as f64
    }

    /// The frame so many back from the newest, or silence for one not recorded yet.
    #[inline]
    fn at(&self, back: usize) -> [f32; 2] {
        if back >= self.filled {
            return [0.0; 2];
        }
        let index = if back > self.write {
            self.write + self.length - back
        } else {
            self.write - back
        };
        self.frames[index].map(|sample| f32::from(sample) / 32_768.0)
    }

    /// The recording a number of frames behind the newest, read between samples through four of
    /// them with the cubic `Grains` reads its line with.
    #[inline]
    fn read(&self, back: f64) -> [f32; 2] {
        let back = back.clamp(1.0, self.size as f64 + 1.0);
        let whole = back as usize;
        let between = (back - whole as f64) as f32;
        // Most reads are of four recorded frames in a row, which need no wrapping round.
        let (newer, current, older, oldest) = if whole + 2 < self.filled && whole + 2 <= self.write
        {
            let at = self.write - whole - 2;
            let four = &self.frames[at..at + 4];
            let read = |frame: [i16; 2]| frame.map(|sample| f32::from(sample) / 32_768.0);
            (read(four[3]), read(four[2]), read(four[1]), read(four[0]))
        } else {
            (
                self.at(whole - 1),
                self.at(whole),
                self.at(whole + 1),
                self.at(whole + 2),
            )
        };
        std::array::from_fn(|side| {
            hermite(
                newer[side],
                current[side],
                older[side],
                oldest[side],
                between,
            )
        })
    }
}

/// One of the delay's taps, which reads the recording through two heads a window apart moving at
/// the pitch's speed, crossfading between them, as a rotary-head pitch shifter does.
#[derive(Debug, Clone, Copy, Default)]
struct Shifter {
    phase: f32,
}

impl Shifter {
    #[inline]
    fn read(&mut self, recording: &Recording, delay: f32, speed: f32, window: f32) -> [f32; 2] {
        self.phase += speed - 1.0;
        if self.phase >= window {
            self.phase -= window;
        } else if self.phase < 0.0 {
            self.phase += window;
        }
        let half = window * 0.5;
        let delay = (delay + half).max(window + 1.0);
        let phase = self.phase / window;
        let blend = if phase < 0.5 {
            2.0 * phase
        } else {
            2.0 - 2.0 * phase
        };
        let one = recording.read(f64::from(delay - self.phase));
        let two = recording.read(f64::from(delay - (self.phase + half) % window));
        [0, 1].map(|side| two[side] + (one[side] - two[side]) * blend)
    }
}

/// Beads' reverb: the Griesinger topology Clouds' has, with Beads' lines, its own wobbles and
/// mix, run as the module runs it, at its quality's rate, so the dirtier ones ring longer.
#[derive(Debug, Clone)]
struct BeadsReverb {
    shared: SharedLines,
    /// Each line's place and length, in frames.
    lines: [(usize, usize); 10],
    /// How many frames each of the module's samples is at the quality it's tuned for.
    scale: f32,
    wobble: [f32; 2],
    wobble_phase: [f64; 2],
    damped: [f32; 2],
    send: Smoothed,
    dry: Smoothed,
    wet: Smoothed,
    time: f32,
    damping: f32,
    smoothing: f32,
    rate: u32,
}

impl BeadsReverb {
    /// Room for its lines at the slowest rate any quality runs it at.
    fn new(rate: u32) -> Self {
        let longest = rate as f32 / QUALITY_RATES[3];
        let size = REVERB_LENGTHS
            .iter()
            .map(|length| (length * longest) as usize + 1)
            .sum::<usize>()
            .next_power_of_two();
        let mut reverb = Self {
            shared: SharedLines::new(size),
            lines: [(0, 1); 10],
            scale: 1.0,
            wobble: [1.0; 2],
            wobble_phase: [0.0; 2],
            damped: [0.0; 2],
            send: Smoothed::at(0.0),
            dry: Smoothed::at(1.0),
            wet: Smoothed::at(0.0),
            time: 0.0,
            damping: 0.0,
            smoothing: coefficient(rate),
            rate,
        };
        reverb.tune_for(QUALITY_RATES[0]);
        reverb
    }

    /// Lay its lines out as the module runs it at a rate.
    fn tune_for(&mut self, module_rate: f32) {
        self.scale = self.rate as f32 / module_rate;
        let mut base = 0;
        for (line, length) in self.lines.iter_mut().zip(REVERB_LENGTHS) {
            let length = ((length * self.scale) as usize).max(2);
            *line = (base, length);
            base += length + 1;
        }
    }

    fn clear(&mut self) {
        self.shared.clear();
        self.damped = [0.0; 2];
    }

    /// Move its wobbles to where they are this many frames in, as `Reverb` does.
    fn keep_time(&mut self, frames: u64) {
        let seconds = frames as f64 / f64::from(self.rate);
        self.wobble_phase = REVERB_WOBBLE_HERTZ.map(|hertz| (hertz * seconds).fract());
    }

    /// Its mix, time and damping, as the module sets them for its quality, the damping as a
    /// one-pole's coefficient at the module's rate.
    fn set(&mut self, mix: f32, time: f32, damping: f32) {
        let input_gain = 0.2 * (1.0 - 0.333 * time);
        let send = mix * (2.0 - mix);
        self.send.target = send * (2.0 - send) * input_gain;
        self.dry.target = (1.0 - mix) * (1.0 + mix);
        self.wet.target = 0.4 / input_gain;
        self.time = time;
        self.damping = per_frame(damping, self.scale);
    }

    fn settle(&mut self) {
        self.send.settle();
        self.dry.settle();
        self.wet.settle();
    }

    /// What the module's 16-bit lines keep of a value: all of it, but clipped at full scale.
    #[inline]
    fn kept(value: f32) -> f32 {
        value.clamp(-1.0, 32_767.0 / 32_768.0)
    }

    #[inline]
    fn put(&mut self, line: usize, offset: usize, value: f32) {
        let (base, _) = self.lines[line];
        self.shared.write(base, offset, Self::kept(value));
    }

    #[inline]
    fn between(&self, line: usize, offset: f32) -> f32 {
        let (base, _) = self.lines[line];
        self.shared.between(base, offset.max(0.0))
    }

    /// A step through an allpass, by what it read from the end of its line.
    #[inline]
    fn allpass(&mut self, line: usize, input: f32, into: f32) -> f32 {
        let (base, length) = self.lines[line];
        self.shared
            .allpass_keeping(base, length, input, into, Self::kept)
    }

    #[inline]
    fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let send = self.send.next(self.smoothing);
        let dry = self.dry.next(self.smoothing);
        let wet = self.wet.next(self.smoothing);
        self.shared.advance();
        for (phase, hertz) in self.wobble_phase.iter_mut().zip(REVERB_WOBBLE_HERTZ) {
            *phase = (*phase + hertz / f64::from(self.rate)).fract();
        }
        if self.shared.turned() & 31 == 0 {
            self.wobble = self
                .wobble_phase
                .map(|phase| (phase * std::f64::consts::TAU).cos() as f32);
        }
        let scale = self.scale;
        let kap = REVERB_DIFFUSION;

        // Smear the first allpass inside itself. The module reads from 10 samples in, 80 either
        // way, which goes out of the line below 0; here it reads from 50, 40 either way, so it
        // reaches as far in and stays in.
        let smeared = self.between(0, (50.0 + 40.0 * self.wobble[0]) * scale);
        self.put(0, (100.0 * scale) as usize, smeared);

        let mut diffused = (frame[0] + frame[1]) * send;
        for line in 0..4 {
            diffused = self.allpass(line, diffused, kap);
        }

        let damping = self.damping;
        let from_two = self.between(9, (6261.0 + 50.0 * self.wobble[1]) * scale);
        let mut left = diffused + from_two * self.time;
        self.damped[0] += damping * (left - self.damped[0]);
        left = self.damped[0];
        left = self.allpass(4, left, -kap);
        left = self.allpass(5, left, kap);
        self.put(6, 0, left);

        let from_one = self.between(6, (4460.0 + 40.0 * self.wobble[0]) * scale);
        let mut right = diffused + from_one * self.time;
        self.damped[1] += damping * (right - self.damped[1]);
        right = self.damped[1];
        right = self.allpass(7, right, kap);
        right = self.allpass(8, right, -kap);
        self.put(9, 0, right);

        [frame[0] * dry + left * wet, frame[1] * dry + right * wet]
    }
}

/// The cassette's wow and flutter: `Tape`'s, which are Beads', moving a short line, then the
/// module's low-pass after it.
#[derive(Debug, Clone)]
struct Cassette {
    wobble: Wobble,
    line: Vec<[f32; 2]>,
    write: usize,
    wow: f32,
    flutter: f32,
    low_pass: f32,
    smoothed: [f32; 2],
    rate: u32,
}

impl Cassette {
    fn new(rate: u32) -> Self {
        let scale = rate as f32 / WOW_RATE;
        Self {
            wobble: Wobble::new(rate),
            line: vec![[0.0; 2]; (WOW_LINE * scale) as usize + 4],
            write: 0,
            wow: WOW_SAMPLES * scale,
            flutter: FLUTTER_SAMPLES * scale,
            low_pass: per_frame(WOW_LOW_PASS, scale),
            smoothed: [0.0; 2],
            rate,
        }
    }

    fn clear(&mut self) {
        self.line.fill([0.0; 2]);
        self.smoothed = [0.0; 2];
        self.write = 0;
    }

    #[inline]
    fn process(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let delay = self
            .wobble
            .next(self.wow, self.flutter, self.rate)
            .clamp(1.0, (self.line.len() - 3) as f32);
        let length = self.line.len();
        self.write = (self.write + 1) % length;
        self.line[self.write] = frame;
        let whole = delay as usize;
        let between = delay - whole as f32;
        let at = |back: usize| self.line[(self.write + length - back) % length];
        let (a, b) = (at(whole), at(whole + 1));
        for side in 0..2 {
            let read = a[side] + (b[side] - a[side]) * between;
            self.smoothed[side] += self.low_pass * (read - self.smoothed[side]);
        }
        self.smoothed
    }
}

/// Beads: its recording and what plays it, what it does after, and what moves them.
#[derive(Debug, Clone)]
pub(crate) struct Beads {
    recording: Recording,
    quality: u8,
    /// The quality it's going to, once what comes out has dipped to nothing, and how far back up
    /// from that dip it is: a new quality starts a new recording, so it dips over
    /// [`CUT_FADE_FRAMES`] each way rather than cut (decision 414).
    next_quality: u8,
    restarted: f32,
    /// Whether anything has come in since it was emptied, so it may hold something to play.
    heard: bool,
    target_mode: BeadsMode,
    /// How far between its modes it is, from `-1.5` seconds, granular, to `+1.5`, the delay.
    transition: f32,
    frozen: bool,
    clocked: bool,
    grains: GrainPlayer,
    seed: Seed,
    delay: DelayController,
    shifters: [Shifter; TAPS],
    tone: [Svf; 2],
    /// The `Size` the tone's filters were last set for, and how much of them is heard.
    tone_for: f32,
    tone_amount: f32,
    steps: Steps,
    /// What comes back round, a frame late, and its high-pass; the limiter on what goes in.
    previous: [f32; 2],
    blocker: [Svf; 2],
    input_limiter: Limiter,
    /// The one-pole on what comes back, a frame of the recording, which the quality sets.
    feedback_low_pass: f32,
    /// The rates its qualities record at but its cleanest, each down and back up.
    halves: Rates,
    thirds: Rates,
    cassette: Cassette,
    pre_reverb: Limiter,
    reverb: BeadsReverb,
    output: Limiter,
    time: Smoothed,
    size: Smoothed,
    shape: Smoothed,
    semitones: Smoothed,
    density: Smoothed,
    random: Smoothed,
    feedback: Smoothed,
    dry: Smoothed,
    wet: Smoothed,
    reverb_amount: f32,
    /// The rate its recording runs at now, how many of its frames each of the module's samples
    /// is, and what smooths its settings a frame of it.
    core_rate: f32,
    scale: f32,
    core_smoothing: f32,
    smoothing: f32,
    rng: Pcg32,
    rate: u32,
}

impl Beads {
    /// A recording long enough for any quality at this rate, and everything after it.
    pub(crate) fn new(rate: u32) -> Self {
        let rate_f = rate as f32;
        let scale = rate_f / BEADS_RATE;
        let most = (MOST_FRAMES as f32 * scale).ceil() as usize;
        let tail = (FREEZE_TAIL * scale).ceil() as usize;
        let halves = Rates::new(
            Converter::new(&Converter::symmetric(&HALVING), 1, 2),
            Converter::new(&Converter::symmetric(&DOUBLING), 2, 1),
        );
        let thirds = Rates::new(
            Converter::new(&thirds_filter(2.0), 2, 3),
            Converter::new(&thirds_filter(3.0), 3, 2),
        );
        let smoothing = coefficient(rate);
        let mut beads = Self {
            recording: Recording::new(most, tail),
            quality: 0,
            next_quality: 0,
            restarted: 1.0,
            heard: false,
            target_mode: BeadsMode::Granular,
            transition: -MODE_TRANSITION,
            frozen: false,
            clocked: false,
            grains: GrainPlayer::new(rate_f, 1.0, 1.0),
            seed: Seed::new(rate_f),
            delay: DelayController::new(rate_f, 1.0, scale),
            shifters: [Shifter::default(); TAPS],
            tone: [Svf::default(); 2],
            tone_for: f32::NAN,
            tone_amount: 0.0,
            steps: Steps::STOPPED,
            previous: [0.0; 2],
            blocker: [Svf::default(); 2],
            input_limiter: Limiter::new(1.0, 0.95, rate_f, scale),
            feedback_low_pass: 0.0,
            halves,
            thirds,
            cassette: Cassette::new(rate),
            pre_reverb: Limiter::new(4.0, 0.9, rate_f, scale),
            reverb: BeadsReverb::new(rate),
            output: Limiter::new(6.0, 0.9, rate_f, scale),
            time: Smoothed::default(),
            size: Smoothed::default(),
            shape: Smoothed::default(),
            semitones: Smoothed::default(),
            density: Smoothed::default(),
            random: Smoothed::default(),
            feedback: Smoothed::default(),
            dry: Smoothed::default(),
            wet: Smoothed::default(),
            reverb_amount: 0.0,
            core_rate: rate_f,
            scale,
            core_smoothing: smoothing,
            smoothing,
            rng: Pcg32::new(0, 0),
            rate,
        };
        beads.start_recording(0);
        beads.follow(Settings::DEFAULT, SongPlace::STILL, 0);
        beads.settle();
        beads
    }

    /// How many seconds its recording holds now.
    pub(crate) fn seconds(&self) -> f32 {
        self.recording.size as f32 / self.core_rate
    }

    /// How many quiet frames in a row mean nothing's left in it: its recording and its reverb's
    /// tail. Frozen, what it holds can always come back, so it never is.
    pub(crate) fn quiet_frames(&self) -> u32 {
        if self.frozen {
            u32::MAX
        } else {
            ((self.seconds() + 4.0) * self.rate as f32) as u32
        }
    }

    /// Draw its grains' places and pans, and its random intervals, from a generator of its own,
    /// so a song renders the same each time.
    pub(crate) fn seed(&mut self, rng: Pcg32) {
        self.rng = rng;
    }

    pub(crate) fn clear(&mut self) {
        self.heard = false;
        self.switch_quality();
        self.restarted = 1.0;
        self.start_recording(self.quality);
        self.reverb.clear();
        self.cassette.clear();
        self.previous = [0.0; 2];
        for filter in self.blocker.iter_mut().chain(&mut self.tone) {
            filter.reset();
        }
    }

    /// Start a new recording in a quality, with everything that plays it starting afresh, as the
    /// module does when its quality changes.
    fn start_recording(&mut self, quality: u8) {
        self.quality = quality.min(3);
        let at = usize::from(self.quality);
        let rate = self.rate as f32;
        self.core_rate = rate * QUALITY_RATES[at] / BEADS_RATE;
        let size = (QUALITY_FRAMES[at] as f32 * self.scale) as usize;
        self.recording.reset(size, RESOLUTIONS[at]);
        self.grains = GrainPlayer::new(
            self.core_rate,
            size as f64,
            f64::from(HEAD_FADE * self.scale),
        );
        self.seed = Seed::new(self.core_rate);
        self.delay = DelayController::new(self.core_rate, size as f32, self.scale);
        self.shifters = [Shifter::default(); TAPS];
        self.input_limiter = Limiter::new(1.0, 0.95, self.core_rate, self.scale);
        self.core_smoothing = coefficient(self.core_rate as u32);
        let low_pass = match RESOLUTIONS[at] {
            Resolution::MuLaw => 0.75,
            Resolution::Bits12 => 0.9,
            Resolution::Bits16 => 0.999,
        };
        self.feedback_low_pass = per_frame(low_pass, self.scale);
        // The module runs its limiters and reverb at its quality's rate.
        let module = rate / QUALITY_RATES[at];
        self.pre_reverb = Limiter::new(4.0, 0.9, rate, module);
        self.output = Limiter::new(6.0, 0.9, rate, module);
        self.reverb.tune_for(QUALITY_RATES[at]);
        self.halves.clear();
        self.thirds.clear();
        let fade = 40.0 / self.core_rate;
        for filter in &mut self.blocker {
            filter.reset();
            filter.set_exact(fade, 0.5);
        }
    }

    /// Take what's set now, where the song is and how fast it goes, and move the reverb's wobbles
    /// to where they are this many frames in.
    pub(crate) fn follow(&mut self, settings: Settings, song: SongPlace, frames: u64) {
        self.keep_time(frames, song);
        self.next_quality = settings.quality.min(3);
        self.target_mode = settings.mode;
        self.frozen = settings.freeze != 0;
        self.clocked = settings.clock != 0;
        self.time.target = f32::from(settings.time) / 255.0;
        self.size.target = f32::from(settings.size) / 255.0;
        self.shape.target = f32::from(settings.shape) / 255.0;
        self.semitones.target = shift_semitones(f32::from(settings.pitch));
        self.density.target = f32::from(settings.density) / 255.0;
        self.random.target = f32::from(settings.random) / 255.0;
        self.feedback.target = f32::from(settings.feedback) / 255.0;
        let blend = f32::from(settings.blend) / 255.0;
        self.dry.target = (1.0 - blend) * (1.0 + blend) * DRY_GAIN;
        self.wet.target = blend * (2.0 - blend) * WET_GAIN;
        self.reverb_amount = f32::from(settings.reverb) / 255.0;
        self.tune_reverb();
    }

    /// Set its reverb as the module does for its `Reverb` knob, in its quality.
    fn tune_reverb(&mut self) {
        let reverb = self.reverb_amount;
        let digital = self.quality == 0;
        let time = reverb * reverb * reverb;
        self.reverb.set(
            reverb * reverb * 0.9,
            if digital {
                0.3 + 0.6 * time
            } else {
                0.45 + 0.5 * time
            },
            if digital { 0.6 } else { 0.4 },
        );
    }

    /// Move the reverb's wobbles to where they are this many frames in, and follow the song's
    /// steps from where it is.
    pub(crate) fn keep_time(&mut self, frames: u64, song: SongPlace) {
        self.reverb.keep_time(frames);
        let per_frame = if song.frames_per_tick > 0.0 {
            f64::from(self.rate) / f64::from(self.core_rate) / song.frames_per_tick
        } else {
            0.0
        };
        self.steps.sync(song.tick, song.playing, per_frame);
    }

    /// Jump to what's set, as it's only just started.
    pub(crate) fn settle(&mut self) {
        self.switch_quality();
        self.restarted = 1.0;
        for setting in [
            &mut self.time,
            &mut self.size,
            &mut self.shape,
            &mut self.semitones,
            &mut self.density,
            &mut self.random,
            &mut self.feedback,
            &mut self.dry,
            &mut self.wet,
        ] {
            setting.settle();
        }
        self.transition = match self.target_mode {
            BeadsMode::Granular => -MODE_TRANSITION,
            BeadsMode::Delay => MODE_TRANSITION,
        };
        self.reverb.settle();
    }

    /// Start a new recording in the quality it's going to, if that's another, with its reverb
    /// set for it.
    fn switch_quality(&mut self) {
        if self.next_quality != self.quality {
            self.start_recording(self.next_quality);
            self.tune_reverb();
        }
    }

    /// Move a frame through a change of quality: down to nothing, the new recording, then back.
    #[inline]
    fn restart(&mut self) {
        let by = 1.0 / CUT_FADE_FRAMES as f32;
        if self.next_quality == self.quality {
            self.restarted = (self.restarted + by).min(1.0);
        } else {
            self.restarted = (self.restarted - by).max(0.0);
            if self.restarted == 0.0 {
                // The reverb's lines are laid out again for the new quality's rate, and the
                // cassette plays nothing of the last time it ran, so both start empty.
                self.reverb.clear();
                if self.next_quality == 3 {
                    self.cassette.clear();
                }
                self.switch_quality();
            }
        }
    }

    fn delay_mode(&self) -> bool {
        self.transition > 0.0
    }

    /// The knobs a frame of the recording on.
    #[inline]
    fn knobs(&mut self) -> Knobs {
        let by = self.core_smoothing;
        Knobs {
            time: self.time.next(by),
            size: self.size.next(by),
            shape: self.shape.next(by),
            semitones: self.semitones.next(by),
            density: self.density.next(by),
            random: self.random.next(by),
            feedback: self.feedback.next(by),
        }
    }

    /// A setting strayed at random by `Random`'s share towards anywhere in its range, as the
    /// module's attenuverters do with nothing patched.
    #[inline]
    fn strayed(&mut self, value: f32, random: f32) -> f32 {
        if random <= 0.0 {
            return value;
        }
        (value + (self.rng.unit() - value) * random).clamp(0.0, 1.0)
    }

    /// Start a grain, as Beads' granular player does.
    fn start_grain(&mut self, knobs: &Knobs, fire: Fire) {
        let random = knobs.random;
        let time = self.strayed(knobs.time, random);
        let size = self.strayed(knobs.size, random);
        let shape = self.strayed(knobs.shape, random);
        let (seconds, reversed) = grain_length(size * 255.0);
        let frames = f64::from(seconds * self.core_rate);
        let ratio = f64::from((knobs.semitones / 12.0).exp2());
        let speed = if reversed { -ratio } else { ratio };
        // As loud as one over the root of how many there'll be, and spread wider the more there
        // are and the more at random they come.
        let playing = self.grains.playing() + 1;
        let amplitude = 1.0 / (playing as f32).sqrt();
        let spread = (3.0 * random + 1.0 - fire.regularity).min(1.0);
        let spread = 0.5 * playing as f32 * spread;
        let (u, v) = (self.rng.unit(), self.rng.unit());
        let pan = 0.5 + (u - 0.5) * (spread * v).min(1.0);
        let safety = f64::from(HEAD_SAFETY * self.scale);
        let reach = self.recording.duration() - 2.0 * safety;
        self.grains.launch(
            Launch {
                delay: f64::from(time) * reach + safety,
                speed,
                frames,
                shape,
                late: fire.late,
                amplitude,
                pan,
            },
            self.frozen,
        );
    }

    /// The delay's taps, a frame of the recording: each read straight and through its pitch
    /// shifter, crossfaded by how far the pitch is from none, then filtered by `Size`.
    #[inline]
    #[allow(
        clippy::float_cmp,
        reason = "the tone is worked out again only when `Size` moves at all"
    )]
    fn play_taps(&mut self, knobs: &Knobs) -> [f32; 2] {
        let speed = (knobs.semitones / 12.0).exp2();
        let shifted = (100.0 * ((speed - 1.0).abs() - 0.01)).clamp(0.0, 1.0);
        let window = TAP_WINDOW * self.scale;
        let mut mix = [0.0; 2];
        for (tap, shifter) in self.delay.taps.iter().zip(&mut self.shifters) {
            if tap.delay == 0.0 || tap.gain == 0.0 {
                continue;
            }
            if shifted > 0.0 {
                let read = shifter.read(&self.recording, tap.delay, speed, window);
                for side in 0..2 {
                    mix[side] += read[side] * tap.gain * shifted;
                }
            }
            if shifted < 1.0 {
                let read = self.recording.read(f64::from(tap.delay));
                for side in 0..2 {
                    mix[side] += read[side] * tap.gain * (1.0 - shifted);
                }
            }
        }
        // `Size` sets the tone: a low-pass from 7 to 2 octaves below the module's highest, open,
        // then a high-pass from 8 to 3 below.
        let bandwidth = knobs.size;
        if bandwidth != self.tone_for {
            self.tone_for = bandwidth;
            let amount = (2.0 * bandwidth - 1.0).abs();
            let octave = if bandwidth < 0.5 {
                -7.0 + bandwidth * 10.0
            } else {
                -8.0 + (bandwidth - 0.5) * 10.0
            };
            let f = 0.49 * octave.exp2();
            let q = 1.0 + 16.0 * amount * f;
            let wet_root = amount * (2.0 - amount);
            self.tone_amount = wet_root * (2.0 - wet_root);
            let to_here = BEADS_RATE / self.rate as f32;
            for filter in &mut self.tone {
                filter.set_exact((f * to_here).min(0.49), q);
            }
        }
        let wet_amount = self.tone_amount;
        let mut out = [0.0; 2];
        for side in 0..2 {
            let (high, _, low) = self.tone[side].process_all(mix[side]);
            let wet = if bandwidth < 0.5 { low } else { high };
            out[side] = mix[side] + (wet - mix[side]) * wet_amount;
        }
        out
    }

    /// One frame at the recording's rate: what's fed back joins what comes in, it's recorded, and
    /// the grains or the taps play it.
    #[inline]
    fn core(&mut self, input: [f32; 2]) -> [f32; 2] {
        let knobs = self.knobs();
        let delay_mode = self.delay_mode();
        let pulse = self.steps.next();
        if delay_mode {
            self.delay.render(
                &knobs,
                self.clocked,
                pulse.is_some(),
                self.frozen,
                self.core_rate,
                &mut self.rng,
            );
        } else {
            let fire = if self.clocked {
                self.seed.clocked(pulse, knobs.density, &mut self.rng)
            } else {
                self.seed.free(knobs.density, self.core_rate, &mut self.rng)
            };
            if let Some(fire) = fire {
                self.start_grain(&knobs, fire);
            }
        }

        // What came out goes back in, through a limiter and, but at 16 bits, a saturation.
        let resolution = self.recording.resolution;
        let most = if delay_mode { 1.5 } else { 3.0 };
        let saturation = match resolution {
            Resolution::MuLaw => 1.75,
            Resolution::Bits12 => 1.3,
            Resolution::Bits16 => 0.0,
        };
        let feedback = knobs.feedback;
        let mut sound = input;
        for side in 0..2 {
            sound[side] += if saturation > 0.0 {
                self.previous[side] * feedback * most / saturation
            } else {
                self.previous[side] * feedback * feedback * most
            };
        }
        sound = self.input_limiter.process(sound);
        if saturation > 0.0 {
            for side in 0..2 {
                let saturated = soft_clip(sound[side] * saturation);
                sound[side] = input[side] + feedback * (saturated - input[side]);
            }
        }

        self.recording.record(sound, self.frozen);
        let out = if delay_mode {
            self.play_taps(&knobs)
        } else {
            let recording = &self.recording;
            self.grains.render(self.frozen, |back| recording.read(back))
        };

        let low_pass = self.feedback_low_pass;
        for side in 0..2 {
            let blocked = self.blocker[side].process_all(out[side]).0;
            self.previous[side] += low_pass * (blocked - self.previous[side]);
        }
        out
    }

    /// A frame through the recording's rate, down to it and back for all but the cleanest.
    #[inline]
    fn through_core(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut rates = match self.quality {
            0 => return self.core(input),
            1 => std::mem::replace(&mut self.thirds, Rates::placeholder()),
            _ => std::mem::replace(&mut self.halves, Rates::placeholder()),
        };
        let Rates {
            down,
            up,
            ready,
            first,
            count,
        } = &mut rates;
        down.push(input, |frame| {
            let played = self.core(frame);
            up.push(played, |out| {
                if *count < ready.len() {
                    ready[(*first + *count) % ready.len()] = out;
                    *count += 1;
                }
            });
        });
        let out = if *count > 0 {
            let out = ready[*first];
            *first = (*first + 1) % ready.len();
            *count -= 1;
            out
        } else {
            [0.0; 2]
        };
        match self.quality {
            1 => self.thirds = rates,
            _ => self.halves = rates,
        }
        out
    }

    /// The sound as the module's dry path codes it in each quality.
    #[inline]
    fn dry_sound(&self, input: [f32; 2]) -> [f32; 2] {
        match self.quality {
            1 => input.map(|sample| ((sample * 4096.0) as i32) as f32 / 4096.0),
            3 => input
                .map(|sample| f32::from(mu_law_decode(mu_law_encode(clip16(sample)))) / 32_768.0),
            _ => input,
        }
    }

    /// Feed one frame in and get what comes out, the sound blended with what Beads makes of it.
    #[inline]
    #[allow(clippy::float_cmp, reason = "its dip lands on its ends exactly")]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.heard |= input != [0.0, 0.0];
        let restarting = self.next_quality != self.quality || self.restarted != 1.0;
        if restarting {
            self.restart();
        }
        let toward = match self.target_mode {
            BeadsMode::Granular => -1.0,
            BeadsMode::Delay => 1.0,
        };
        self.transition =
            (self.transition + toward / self.rate as f32).clamp(-MODE_TRANSITION, MODE_TRANSITION);
        let wet_fade = smooth_step((self.transition.abs() / MODE_FADE).min(1.0));

        // Through a change of quality what's recorded dips too, and so does what goes into the
        // cassette and the reverb, so the new recording and what they hold rise from nothing
        // rather than start on a step.
        let restarted = smooth_step(self.restarted);
        let played = if restarting {
            self.through_core(input.map(|side| side * restarted))
        } else {
            self.through_core(input)
        };
        let dry_sound = self.dry_sound(input);
        let dry = self.dry.next(self.smoothing);
        let wet = self.wet.next(self.smoothing) * wet_fade;
        let mut out = [0, 1].map(|side| dry * dry_sound[side] + wet * played[side]);
        if restarting {
            out = out.map(|side| side * restarted);
        }
        if self.quality == 3 {
            out = self.cassette.process(out);
        }
        out = self.pre_reverb.process(out);
        out = self.reverb.process(out);
        out = self.output.process(out);
        // The codec's full scale, then back up from the module's dry level.
        let out = out.map(|side| side.clamp(-1.0, 1.0) / DRY_GAIN);
        if restarting {
            return out.map(|side| side * restarted);
        }
        out
    }
}

impl Rates {
    /// Nothing, while the real one is borrowed for a frame. It holds no buffers, so making one
    /// allocates nothing.
    fn placeholder() -> Self {
        Self {
            down: Converter::EMPTY,
            up: Converter::EMPTY,
            ready: [[0.0; 2]; 8],
            first: 0,
            count: 0,
        }
    }
}

impl SlotEffect for Beads {
    type Settings = Settings;
    /// It blends the sound and what it made itself, as the module does.
    const MIXING: Mixing = Mixing::Inside;

    /// Beads', which records the longest of the module's qualities too.
    fn new(rate: u32) -> Self {
        Self::new(rate)
    }

    fn follow(&mut self, settings: Settings, song: SongPlace, frames: u64) {
        self.follow(settings, song, frames);
    }

    fn settle(&mut self) {
        self.settle();
    }

    /// Its mode and quality, while nothing has come in since it was emptied, so a lock on a
    /// song's first row is heard from its first frame; nothing else. With `Blend` all dry, what
    /// it feeds back and its reverb go on with none of them coming out, so the quiet its slot
    /// hears doesn't show it holds nothing, and a jump of a mode or a level could step what's
    /// still in it. It jumps all it has only as it's taken from the pool.
    fn settle_quiet(&mut self) {
        if !self.heard {
            if self.next_quality != self.quality {
                self.switch_quality();
                self.reverb.settle();
            }
            self.restarted = 1.0;
            self.transition = match self.target_mode {
                BeadsMode::Granular => -MODE_TRANSITION,
                BeadsMode::Delay => MODE_TRANSITION,
            };
        }
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn quiet_frames(&self, _rate: u32) -> u32 {
        self.quiet_frames()
    }

    fn keep_time(&mut self, frames: u64, song: SongPlace) {
        self.keep_time(frames, song);
    }

    fn seed(&mut self, rng: Pcg32) {
        self.seed(rng);
    }
}

impl Rung for Beads {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::signals::{rms, sine};
    use vt_model::insert::MOST_SHIFT;

    const RATE: u32 = 48_000;

    fn beads(settings: Settings, rate: u32) -> Beads {
        let mut beads = Beads::new(rate);
        beads.follow(settings, SongPlace::STILL, 0);
        beads.settle();
        beads
    }

    /// All wet and dry of reverb; in `Delay`, repeating every 37 ms or so.
    fn wet(mode: BeadsMode) -> Settings {
        let delay = mode == BeadsMode::Delay;
        Settings {
            mode,
            blend: 0xFF,
            reverb: 0,
            time: if delay { 0 } else { Settings::DEFAULT.time },
            density: if delay {
                0x20
            } else {
                Settings::DEFAULT.density
            },
            ..Settings::DEFAULT
        }
    }

    /// How strong a frequency is in a sound, as a sine's amplitude, by a single bin of a DFT over
    /// each 40 ms through a Hann window, on average.
    fn strength(sound: &[f32], hertz: f32) -> f32 {
        let chunks = sound.as_chunks::<{ RATE as usize / 25 }>().0;
        let count = chunks.len() as f32;
        chunks
            .iter()
            .map(|chunk| {
                let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
                let length = chunk.len() as f64;
                for (frame, sample) in chunk.iter().enumerate() {
                    let at = frame as f64;
                    let hann = 0.5 - 0.5 * (std::f64::consts::TAU * at / length).cos();
                    let angle = f64::from(TAU * hertz / RATE as f32) * at;
                    real += f64::from(*sample) * hann * angle.cos();
                    imaginary += f64::from(*sample) * hann * angle.sin();
                }
                ((real * real + imaginary * imaginary).sqrt() * 4.0 / length) as f32
            })
            .sum::<f32>()
            / count
    }

    /// The last second of three of a tone through it, on the left.
    fn through(settings: Settings, hertz: f32) -> Vec<f32> {
        let mut beads = beads(settings, RATE);
        (0..RATE as usize * 3)
            .map(|frame| beads.process([sine(frame, hertz, 0.5, RATE); 2])[0])
            .skip(RATE as usize * 2)
            .collect()
    }

    #[test]
    fn each_quality_records_the_modules_seconds_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            for (quality, seconds) in [4.0, 8.0, 10.667, 16.0].into_iter().enumerate() {
                let beads = beads(
                    Settings {
                        quality: quality as u8,
                        ..Settings::DEFAULT
                    },
                    rate,
                );
                assert!(
                    (beads.seconds() - seconds).abs() < 0.01,
                    "{rate} {quality}: {}",
                    beads.seconds()
                );
            }
        }
    }

    #[test]
    fn every_mode_and_quality_plays_what_it_recorded() {
        for mode in BeadsMode::ALL {
            for quality in 0..4 {
                let out = through(
                    Settings {
                        quality,
                        density: if mode == BeadsMode::Delay { 0x20 } else { 0x30 },
                        ..wet(mode)
                    },
                    330.0,
                );
                let level = rms(&out);
                assert!(level > 0.05 && level < 1.4, "{mode:?} {quality}: {level}");
                assert!(strength(&out, 330.0) > 0.05, "{mode:?} {quality}");
            }
        }
    }

    #[test]
    fn granular_has_no_grains_with_density_in_the_middle() {
        let out = through(
            Settings {
                density: 0x80,
                ..wet(BeadsMode::Granular)
            },
            330.0,
        );
        assert!(out.iter().all(|s| s.abs() < 1e-6), "{}", rms(&out));
    }

    #[test]
    fn blend_at_none_passes_the_sound_through() {
        let mut beads = beads(
            Settings {
                blend: 0,
                reverb: 0,
                ..Settings::DEFAULT
            },
            RATE,
        );
        for frame in 0..RATE as usize {
            let sample = sine(frame, 440.0, 0.5, RATE) * 0.4;
            let out = beads.process([sample; 2]);
            assert!((out[0] - sample).abs() < 1e-5, "{frame}: {out:?} {sample}");
        }
    }

    #[test]
    fn pitch_shifts_each_mode_an_octave() {
        for mode in BeadsMode::ALL {
            let up = through(
                Settings {
                    pitch: MOST_SHIFT + 12,
                    size: if mode == BeadsMode::Delay { 0x80 } else { 0xE0 },
                    density: 0x20,
                    ..wet(mode)
                },
                220.0,
            );
            let (octave, left) = (strength(&up, 440.0), strength(&up, 220.0));
            assert!(octave > 0.03, "{mode:?}: {octave}");
            assert!(left < octave * 0.5, "{mode:?}: {octave} {left}");
        }
    }

    #[test]
    fn the_8_bit_and_tape_qualities_are_band_limited_as_the_modules() {
        // A tone above what a 24 kHz recording holds, and one above a 32 kHz one.
        let level = |quality: u8, hertz: f32| {
            let out = through(
                Settings {
                    quality,
                    ..wet(BeadsMode::Delay)
                },
                hertz,
            );
            rms(&out)
        };
        assert!(level(0, 14_000.0) > 0.1, "{}", level(0, 14_000.0));
        for quality in [2, 3] {
            assert!(
                level(quality, 14_000.0) < level(0, 14_000.0) * 0.05,
                "{quality}: {}",
                level(quality, 14_000.0)
            );
        }
        assert!(level(1, 18_000.0) < level(0, 18_000.0) * 0.05);
        assert!(level(1, 9_000.0) > level(0, 9_000.0) * 0.5);
    }

    #[test]
    fn frozen_every_mode_keeps_playing_after_the_sound_stops() {
        for mode in BeadsMode::ALL {
            let settings = Settings {
                density: if mode == BeadsMode::Delay { 0x20 } else { 0x30 },
                ..wet(mode)
            };
            let mut free = beads(settings, RATE);
            for frame in 0..RATE as usize * 2 {
                free.process([sine(frame, 330.0, 0.5, RATE); 2]);
            }
            let mut frozen = free.clone();
            frozen.follow(
                Settings {
                    freeze: 1,
                    ..settings
                },
                SongPlace::STILL,
                0,
            );
            let after = |beads: &mut Beads| -> Vec<f32> {
                (0..RATE as usize * 6)
                    .map(|_| beads.process([0.0; 2])[0])
                    .collect()
            };
            let (held, gone) = (after(&mut frozen), after(&mut free));
            let late = &held[RATE as usize * 5..];
            assert!(
                strength(late, 330.0) > 0.03,
                "{mode:?}: {}",
                strength(late, 330.0)
            );
            assert!(
                gone[RATE as usize * 5..].iter().all(|s| s.abs() < 1e-3),
                "{mode:?}"
            );
            assert_eq!(frozen.quiet_frames(), u32::MAX);
        }
    }

    #[test]
    fn the_same_seed_plays_the_same_and_another_differs() {
        let play = |seed: u64| -> Vec<f32> {
            let mut beads = beads(Settings::DEFAULT, RATE);
            beads.seed(Pcg32::new(seed, 3));
            (0..RATE as usize)
                .map(|frame| beads.process([sine(frame, 220.0, 0.5, RATE); 2])[0])
                .collect()
        };
        assert_eq!(play(1), play(1));
        assert_ne!(play(1), play(2));
    }

    #[test]
    fn mu_law_and_12_bits_store_what_the_module_stores() {
        let mut recording = Recording::new(16, 4);
        recording.reset(8, Resolution::Bits12);
        assert_eq!(recording.stored(0.123_456), (clip16(0.123_456) >> 4) << 4);
        recording.reset(8, Resolution::MuLaw);
        assert_eq!(recording.stored(0.5), mu_law_decode(mu_law_encode(16_384)));
        recording.reset(8, Resolution::Bits16);
        assert_eq!(recording.stored(2.0), 32_767);
    }

    #[test]
    fn the_converters_keep_a_low_tone_and_its_level() {
        for quality in [1, 2] {
            let mut beads = beads(Settings::DEFAULT, RATE);
            beads.start_recording(quality);
            let rates = if quality == 1 {
                &mut beads.thirds
            } else {
                &mut beads.halves
            };
            let mut out = Vec::new();
            for frame in 0..RATE as usize {
                let sample = sine(frame, 1000.0, 0.5, RATE);
                rates.down.push([sample; 2], |frame| {
                    rates.up.push(frame, |frame| out.push(frame[0]));
                });
            }
            let level = rms(&out[RATE as usize / 2..]);
            assert!(
                (level - 0.5 / 2.0_f32.sqrt()).abs() < 0.01,
                "{quality}: {level}"
            );
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        // `cargo test --release` plays every mode, quality, rate, clock and corner. A test build,
        // with its overflow checks, takes each mode and quality once at a rate of their own, with
        // every corner.
        let all = !cfg!(debug_assertions);
        let rates = [44_100, 192_000];
        for (m, mode) in BeadsMode::ALL.into_iter().enumerate() {
            for quality in 0..4_u8 {
                for (r, rate) in rates.into_iter().enumerate() {
                    if !all && (m + usize::from(quality) + r) % 2 == 1 {
                        continue;
                    }
                    for (c, (low, high)) in [(0x00, 0x00), (0xFF, 0xFF), (0x00, 0xFF), (0xFF, 0x00)]
                        .into_iter()
                        .enumerate()
                    {
                        let settings = Settings {
                            mode,
                            quality,
                            time: low,
                            size: high,
                            shape: low,
                            pitch: if low == 0 { 0 } else { MOST_SHIFT * 2 },
                            density: high,
                            clock: (c % 2) as u8,
                            random: high,
                            feedback: 0xFF,
                            blend: 0xFF,
                            reverb: high,
                            ..Settings::DEFAULT
                        };
                        let mut beads = beads(settings, rate);
                        let quarter = rate as usize / 4;
                        let frames_per_tick = f64::from(rate) / 240.0 / 8.0;
                        for frame in 0..rate as usize * 2 {
                            if frame % 256 == 0 {
                                let tick = frame as f64 / frames_per_tick;
                                let freeze = u8::from(frame >= rate as usize);
                                let song = SongPlace {
                                    tick,
                                    playing: true,
                                    frames_per_tick,
                                };
                                beads.follow(Settings { freeze, ..settings }, song, frame as u64);
                            }
                            // Full-scale noise, a full-scale square, DC and silence.
                            let level = match frame / quarter % 4 {
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
                            let out = beads.process([level, -level * 0.5]);
                            // What it made, before the blend and the limiters, which would hide
                            // its running away.
                            assert!(
                                beads
                                    .previous
                                    .iter()
                                    .all(|s| s.is_finite() && s.abs() < 8.0),
                                "{settings:?} {rate}: {:?}",
                                beads.previous
                            );
                            assert!(
                                out.iter()
                                    .all(|s| s.is_finite() && s.abs() <= 1.0 / DRY_GAIN + 1e-4),
                                "{settings:?} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn clocked_grains_start_on_the_steps() {
        // Every step, at 120 BPM, 6000 frames a step: the grains start where the steps fall.
        let settings = Settings {
            clock: 1,
            density: 0x00,
            size: 0x60,
            ..wet(BeadsMode::Granular)
        };
        for quality in 0..4 {
            let mut beads = beads(
                Settings {
                    quality,
                    ..settings
                },
                RATE,
            );
            let frames_per_tick = 6000.0 / 240.0;
            let mut starts = Vec::new();
            let mut playing = 0;
            for frame in 0..60_000 {
                if frame % 128 == 0 {
                    let song = SongPlace {
                        tick: frame as f64 / frames_per_tick,
                        playing: true,
                        frames_per_tick,
                    };
                    beads.follow(
                        Settings {
                            quality,
                            ..settings
                        },
                        song,
                        frame as u64,
                    );
                }
                beads.process([0.3; 2]);
                let now = beads.grains.playing();
                if now > playing {
                    starts.push(frame);
                }
                playing = now;
            }
            assert!(starts.len() >= 9, "{quality}: {starts:?}");
            for start in starts {
                let off = start % 6000;
                assert!(off <= 4 || off >= 5996, "{quality}: {start}");
            }
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        for mode in BeadsMode::ALL {
            let mut beads = beads(wet(mode), RATE);
            for frame in 0..RATE as usize {
                beads.process([sine(frame, 440.0, 0.5, RATE); 2]);
            }
            beads.clear();
            beads.settle();
            for _ in 0..RATE {
                let out = beads.process([0.0, 0.0]);
                assert!(out.iter().all(|s| s.abs() < 1e-6), "{mode:?}: {out:?}");
            }
        }
    }
}
