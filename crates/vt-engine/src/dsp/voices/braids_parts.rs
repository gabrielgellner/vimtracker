//! Shared parts of the voices ported from Mutable Instruments' Braids: how a model runs at twice
//! the rate, as Braids runs at 96 kHz, and comes back down; its band-limited analog oscillator;
//! and its waveshaper tables, taken from its generated `resources.cc`. Ported to Rust, in floating
//! point, from `macro_oscillator.cc`, `analog_oscillator.cc`, `parameter_interpolation.h` and
//! `resources/lookup_tables.py`.
//!
//! Copyright 2012 Emilie Gillet.
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

use std::f32::consts::{PI, TAU};

use super::virtual_analog::{BLOCK, Moving, Settings, next_blep, this_blep};
use crate::dsp::blocks::oversampler::Decimator;
use crate::dsp::blocks::plaits_parts::DcBlocker;
use crate::dsp::blocks::sine_table::SINE;

/// Braids renders at 96 kHz, and these at twice the program's rate, which at 48 kHz is the same.
pub(super) const OVERSAMPLING: usize = 2;
/// How many frames a model renders at once, at twice the rate.
pub(super) const INNER: usize = BLOCK * OVERSAMPLING;

/// Braids' highest note for its analog oscillators, as a MIDI note: about 13.3 kHz.
const HIGHEST_NOTE: f32 = 128.0;
/// Where the output's offset is taken out, as the module's output coupling does.
const DC_HZ: f32 = 10.0;

/// Whether, and where, an oscillator's cycle restarted during each frame: how far into the frame
/// after the restart it ended, as a share of a frame.
pub(super) type Restarts = [Option<f32>; INNER];
/// No restarts.
pub(super) const NO_SYNC: Restarts = [None; INNER];

/// A note as Braids takes it, in semitones as MIDI numbers them, at the rate a model runs at.
#[derive(Debug, Clone, Copy)]
pub struct Pitch {
    pub(super) note: f32,
    /// Twice the program's rate, in Hz.
    rate: f32,
}

impl Pitch {
    /// The phase a cycle moves each frame, at a note so many semitones from this one, no higher
    /// than Braids' highest.
    #[inline]
    pub(super) fn increment(self, semitones: f32) -> f32 {
        let note = (self.note + semitones).min(HIGHEST_NOTE);
        440.0 * ((note - 69.0) / 12.0).exp2() / self.rate
    }

    /// Braids' `lut_svf_cutoff` at a MIDI note: a state variable filter's `2 sin(pi f)`, with its
    /// cutoff no higher than an eighth of the rate.
    #[inline]
    pub(super) fn svf_cutoff(self, note: f32) -> f32 {
        let f = (440.0 * ((note - 69.0) / 12.0).exp2() / self.rate).min(0.125);
        2.0 * (PI * f).sin()
    }
}

/// A sine of a phase in turns, read from Plaits' table, starting at its trough as Braids'
/// `wav_sine` does.
#[inline]
pub(super) fn sine(phase: f32) -> f32 {
    let phase = phase + 0.75;
    let index = (phase - phase.floor()) * 512.0;
    let whole = (index as usize).min(511);
    let fraction = index - whole as f32;
    let (a, b) = (SINE[whole], SINE[whole + 1]);
    a + (b - a) * fraction
}

/// A table of 257 values over `-1..1` read between its entries, as stmlib's `Interpolate88`
/// reads a waveshaper.
#[inline]
pub(super) fn waveshape(table: &[i16; 257], x: f32) -> f32 {
    let index = ((x + 1.0) * 128.0).clamp(0.0, 255.999);
    let whole = index as usize;
    let fraction = index - whole as f32;
    let (a, b) = (f32::from(table[whole]), f32::from(table[whole + 1]));
    (a + (b - a) * fraction) / 32_768.0
}

/// stmlib's `Mix`: from `a` at a balance of 0 to `b` at 1.
#[inline]
pub(super) fn mix(a: f32, b: f32, balance: f32) -> f32 {
    a + (b - a) * balance
}

/// One of Braids' models, rendering at twice the rate.
pub trait Model: Copy {
    /// How loud it comes out against the other machines.
    const LEVEL: f32;
    fn new() -> Self;
    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]);
}

/// A voice playing one of Braids' models, at twice the rate and brought back down. It's plain
/// data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Macro<M> {
    model: M,
    /// Twice the program's rate, in Hz.
    rate: f32,
    down: Decimator,
    dc_blocker: DcBlocker,
    output: [f32; BLOCK],
    at: usize,
}

impl<M: Model> Macro<M> {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32 * OVERSAMPLING as f32;
        Self {
            model: M::new(),
            rate,
            down: Decimator::default(),
            dc_blocker: DcBlocker::new(1.0 - TAU * DC_HZ / rate * OVERSAMPLING as f32),
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency, settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let hertz = frequency.max(1e-6) * self.rate / OVERSAMPLING as f32;
        let pitch = Pitch {
            note: 69.0 + 12.0 * (hertz / 440.0).log2(),
            rate: self.rate,
        };
        let mut inner = [0.0; INNER];
        self.model.render(pitch, &settings, &mut inner);
        let (pairs, _) = inner.as_chunks::<OVERSAMPLING>();
        for (out, pair) in self.output.iter_mut().zip(pairs) {
            let down = self.down.process(pair[0], pair[1]);
            *out = self.dc_blocker.process(down) * M::LEVEL;
        }
    }
}

/// The waves Braids' analog oscillator makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shape {
    Saw,
    /// A saw with a second, half-height saw a width behind it.
    VariableSaw,
    /// A saw that starts each cycle on a step, held for a width, as the CS-80's does.
    CSaw,
    /// A square, narrowing to a pulse.
    Square,
    Triangle,
    Sine,
}

/// Where a `CSaw`'s cycle starts, by its `aux`.
#[inline]
fn csaw_depth(aux: f32) -> f32 {
    -0.0625 + aux * 0.25
}

/// Braids' analog oscillator: a band-limited saw, square, triangle or sine, with hard sync in and
/// out.
#[derive(Debug, Clone, Copy)]
pub(super) struct AnalogOscillator {
    phase: f32,
    previous_increment: f32,
    high: bool,
    /// The level a `CSaw`'s cycle starts at.
    discontinuity_depth: f32,
    next_sample: f32,
    shape: Option<Shape>,
}

impl AnalogOscillator {
    pub(super) fn new() -> Self {
        Self {
            phase: 0.0,
            previous_increment: 0.0,
            high: false,
            discontinuity_depth: -0.5,
            next_sample: 0.0,
            shape: None,
        }
    }

    /// Fill `out` with a shape at a phase increment, its settings on `0..=1`, with its increment
    /// moving in a line from the last call's. `sync_in` restarts its cycle, and `sync_out` says
    /// where its own restarted. A new shape starts it over, as Braids does.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render(
        &mut self,
        shape: Shape,
        increment: f32,
        parameter: f32,
        aux: f32,
        sync_in: &Restarts,
        out: &mut [f32; INNER],
        sync_out: Option<&mut Restarts>,
    ) {
        if self.shape != Some(shape) {
            let previous_increment = self.previous_increment;
            *self = Self::new();
            self.previous_increment = previous_increment;
            if previous_increment == 0.0 {
                self.previous_increment = increment;
            }
            // Braids starts a `CSaw` on a step lower than any `aux` sets, which overflows it, so
            // here it starts on its own.
            self.discontinuity_depth = csaw_depth(aux);
            self.shape = Some(shape);
        }
        let mut increments = Moving::across(self.previous_increment, increment, INNER);
        let mut unused = NO_SYNC;
        let sync_out = sync_out.unwrap_or(&mut unused);
        match shape {
            Shape::Saw => self.saw(&mut increments, sync_in, out, sync_out),
            Shape::VariableSaw => {
                self.variable_saw(&mut increments, parameter, sync_in, out, sync_out);
            }
            Shape::CSaw => self.csaw(&mut increments, parameter, aux, sync_in, out, sync_out),
            Shape::Square => self.square(&mut increments, parameter, sync_in, out, sync_out),
            Shape::Triangle => self.triangle(&mut increments, sync_in, out),
            Shape::Sine => self.sine(&mut increments, sync_in, out),
        }
        self.previous_increment = increment;
    }

    /// Where a sync's restart came in this frame's cycle, and whether the cycle had ended by then.
    #[inline]
    fn at_reset(&self, t: f32, increment: f32) -> (f32, bool) {
        let at = self.phase + (1.0 - t) * increment;
        if at >= 1.0 {
            (at - 1.0, true)
        } else {
            (at, false)
        }
    }

    /// Move on a frame, saying whether the cycle ended, and where if `sync_out` wants it.
    #[inline]
    fn advance(&mut self, increment: f32, sync_out: &mut Option<f32>) -> bool {
        self.phase += increment;
        let wrapped = self.phase >= 1.0;
        if wrapped {
            self.phase -= 1.0;
        }
        *sync_out = wrapped.then(|| (self.phase / increment).min(1.0));
        wrapped
    }

    fn saw(
        &mut self,
        increments: &mut Moving,
        sync_in: &Restarts,
        out: &mut [f32; INNER],
        sync_out: &mut Restarts,
    ) {
        let mut next = self.next_sample;
        for ((out, sync), sync_out) in out.iter_mut().zip(sync_in).zip(sync_out) {
            let increment = increments.next();
            let mut this = next;
            next = 0.0;
            let mut transition_during_reset = false;
            if let Some(t) = *sync {
                let (at, wrapped) = self.at_reset(t, increment);
                transition_during_reset = wrapped;
                this -= at * this_blep(t);
                next -= at * next_blep(t);
            }
            let self_reset = self.advance(increment, sync_out);
            if (transition_during_reset || sync.is_none()) && self_reset {
                let t = (self.phase / increment).min(1.0);
                this -= this_blep(t);
                next -= next_blep(t);
            }
            if let Some(t) = *sync {
                self.phase = t * increment;
                self.high = false;
            }
            next += self.phase;
            *out = 2.0 * this - 1.0;
        }
        self.next_sample = next;
    }

    fn variable_saw(
        &mut self,
        increments: &mut Moving,
        parameter: f32,
        sync_in: &Restarts,
        out: &mut [f32; INNER],
        sync_out: &mut Restarts,
    ) {
        // The second saw's place, from a sixty-fourth of a cycle to a half.
        let pw = parameter.max(1.0 / 32.0) * 0.5;
        let naive = |phase: f32| 0.5 * phase + 0.5 * (phase - pw).rem_euclid(1.0);
        let mut next = self.next_sample;
        for ((out, sync), sync_out) in out.iter_mut().zip(sync_in).zip(sync_out) {
            let increment = increments.next();
            let mut this = next;
            next = 0.0;
            let mut transition_during_reset = false;
            if let Some(t) = *sync {
                let (at, wrapped) = self.at_reset(t, increment);
                transition_during_reset = wrapped || (!self.high && at >= pw);
                let discontinuity = naive(0.0) - naive(at);
                this += discontinuity * this_blep(t);
                next += discontinuity * next_blep(t);
            }
            let mut self_reset = self.advance(increment, sync_out);
            while transition_during_reset || sync.is_none() {
                if !self.high {
                    if self.phase < pw {
                        break;
                    }
                    let t = ((self.phase - pw) / increment).clamp(0.0, 1.0);
                    this -= 0.5 * this_blep(t);
                    next -= 0.5 * next_blep(t);
                    self.high = true;
                }
                if self.high {
                    if !self_reset {
                        break;
                    }
                    self_reset = false;
                    let t = (self.phase / increment).min(1.0);
                    this -= 0.5 * this_blep(t);
                    next -= 0.5 * next_blep(t);
                    self.high = false;
                }
            }
            if let Some(t) = *sync {
                self.phase = t * increment;
                self.high = false;
            }
            next += naive(self.phase);
            *out = 2.0 * this - 1.0;
        }
        self.next_sample = next;
    }

    fn csaw(
        &mut self,
        increments: &mut Moving,
        parameter: f32,
        aux: f32,
        sync_in: &Restarts,
        out: &mut [f32; INNER],
        sync_out: &mut Restarts,
    ) {
        let depth = csaw_depth(aux);
        let mut next = self.next_sample;
        for ((out, sync), sync_out) in out.iter_mut().zip(sync_in).zip(sync_out) {
            let increment = increments.next();
            // How long each cycle holds its step, up to three eighths of it.
            let pw = (parameter * 0.375).max(8.0 * increment);
            let mut this = next;
            next = 0.0;
            let mut transition_during_reset = false;
            if let Some(t) = *sync {
                let (at, wrapped) = self.at_reset(t, increment);
                transition_during_reset = wrapped || (!self.high && at >= pw);
                if self.phase >= pw {
                    self.discontinuity_depth = depth;
                    let discontinuity = depth - 0.5 * at;
                    this += discontinuity * this_blep(t);
                    next += discontinuity * next_blep(t);
                }
            }
            let mut self_reset = self.advance(increment, sync_out);
            while transition_during_reset || sync.is_none() {
                if !self.high {
                    if self.phase < pw {
                        break;
                    }
                    let t = ((self.phase - pw) / increment).clamp(0.0, 1.0);
                    let discontinuity = 0.5 * self.phase - self.discontinuity_depth;
                    this += discontinuity * this_blep(t);
                    next += discontinuity * next_blep(t);
                    self.high = true;
                }
                if self.high {
                    if !self_reset {
                        break;
                    }
                    self_reset = false;
                    self.discontinuity_depth = depth;
                    let t = (self.phase / increment).min(1.0);
                    let discontinuity = self.discontinuity_depth - 0.5;
                    this += discontinuity * this_blep(t);
                    next += discontinuity * next_blep(t);
                    self.high = false;
                }
            }
            if let Some(t) = *sync {
                self.phase = t * increment;
                self.high = false;
            }
            next += if self.phase < pw {
                self.discontinuity_depth
            } else {
                0.5 * self.phase
            };
            *out = 2.0 * this - 0.5;
        }
        self.next_sample = next;
    }

    fn square(
        &mut self,
        increments: &mut Moving,
        parameter: f32,
        sync_in: &Restarts,
        out: &mut [f32; INNER],
        sync_out: &mut Restarts,
    ) {
        // A square at 0, narrowing to a pulse a hundredth of the cycle wide.
        let pw = 0.5 - parameter.min(32_000.0 / 32_768.0) * 0.5;
        let mut next = self.next_sample;
        for ((out, sync), sync_out) in out.iter_mut().zip(sync_in).zip(sync_out) {
            let increment = increments.next();
            let mut this = next;
            next = 0.0;
            let mut transition_during_reset = false;
            if let Some(t) = *sync {
                let (at, wrapped) = self.at_reset(t, increment);
                transition_during_reset = wrapped || (!self.high && at >= pw);
                if at >= pw {
                    this -= this_blep(t);
                    next -= next_blep(t);
                }
            }
            let mut self_reset = self.advance(increment, sync_out);
            while transition_during_reset || sync.is_none() {
                if !self.high {
                    if self.phase < pw {
                        break;
                    }
                    let t = ((self.phase - pw) / increment).clamp(0.0, 1.0);
                    this += this_blep(t);
                    next += next_blep(t);
                    self.high = true;
                }
                if self.high {
                    if !self_reset {
                        break;
                    }
                    self_reset = false;
                    let t = (self.phase / increment).min(1.0);
                    this -= this_blep(t);
                    next -= next_blep(t);
                    self.high = false;
                }
            }
            if let Some(t) = *sync {
                self.phase = t * increment;
                self.high = false;
            }
            next += if self.phase < pw { 0.0 } else { 1.0 };
            *out = 2.0 * this - 1.0;
        }
        self.next_sample = next;
    }

    /// A triangle, worked out twice a frame and averaged, with no correction for a restart.
    fn triangle(&mut self, increments: &mut Moving, sync_in: &Restarts, out: &mut [f32; INNER]) {
        let triangle = |phase: f32| {
            if phase < 0.5 {
                4.0 * phase - 1.0
            } else {
                3.0 - 4.0 * phase
            }
        };
        for (out, sync) in out.iter_mut().zip(sync_in) {
            let increment = increments.next();
            if sync.is_some() {
                self.phase = 0.0;
            }
            self.phase = (self.phase + 0.5 * increment).fract();
            let first = triangle(self.phase);
            self.phase = (self.phase + 0.5 * increment).fract();
            *out = 0.5 * (first + triangle(self.phase));
        }
    }

    fn sine(&mut self, increments: &mut Moving, sync_in: &Restarts, out: &mut [f32; INNER]) {
        for (out, sync) in out.iter_mut().zip(sync_in) {
            self.phase = (self.phase + increments.next()).fract();
            if sync.is_some() {
                self.phase = 0.0;
            }
            *out = sine(self.phase);
        }
    }
}

/// Braids' `ws_violent_overdrive`: `tanh(8x)`, over `-1..1`.
pub(super) const VIOLENT_OVERDRIVE: [i16; 257] = [
    -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766,
    -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766,
    -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32766, -32765, -32765,
    -32765, -32765, -32765, -32765, -32765, -32765, -32765, -32764, -32764, -32764, -32764, -32763,
    -32763, -32763, -32762, -32762, -32761, -32760, -32760, -32759, -32758, -32757, -32756, -32754,
    -32753, -32751, -32749, -32747, -32744, -32741, -32738, -32734, -32730, -32725, -32720, -32713,
    -32706, -32698, -32689, -32679, -32668, -32655, -32640, -32623, -32604, -32582, -32558, -32531,
    -32499, -32464, -32424, -32379, -32327, -32269, -32204, -32130, -32046, -31951, -31844, -31724,
    -31587, -31434, -31260, -31065, -30845, -30598, -30320, -30008, -29658, -29266, -28828, -28340,
    -27795, -27189, -26518, -25774, -24954, -24053, -23064, -21985, -20811, -19541, -18172, -16705,
    -15142, -13486, -11742, -9919, -8025, -6073, -4075, -2045, 0, 2045, 4075, 6073, 8025, 9919,
    11742, 13486, 15142, 16705, 18172, 19541, 20811, 21985, 23064, 24053, 24954, 25774, 26518,
    27189, 27795, 28340, 28828, 29266, 29658, 30008, 30320, 30598, 30845, 31065, 31260, 31434,
    31587, 31724, 31844, 31951, 32046, 32130, 32204, 32269, 32327, 32379, 32424, 32464, 32499,
    32531, 32558, 32582, 32604, 32623, 32640, 32655, 32668, 32679, 32689, 32698, 32706, 32713,
    32720, 32725, 32730, 32734, 32738, 32741, 32744, 32747, 32749, 32751, 32753, 32754, 32756,
    32757, 32758, 32759, 32760, 32760, 32761, 32762, 32762, 32763, 32763, 32763, 32764, 32764,
    32764, 32764, 32765, 32765, 32765, 32765, 32765, 32765, 32765, 32765, 32765, 32766, 32766,
    32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766,
    32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766, 32766,
    32766, 32766, 32766, 32766, 32766, 32766,
];
