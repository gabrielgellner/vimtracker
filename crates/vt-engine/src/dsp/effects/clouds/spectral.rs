//! Clouds' spectral mode, the module's phase vocoder. What comes in is cut into frames four times
//! as long as the hop between them, through the module's window, and each frame's spectrum is
//! taken apart into levels and phases. The levels are written into a few kept spectra, its
//! textures, which `Position` chooses between; what's played is read back from them, warped,
//! shifted and quantised, given the frame's phases, put back together, and added to the frames
//! around it. Frozen, nothing more is written, and the phases go on turning as they last did.
//!
//! The module's frame is 4096 samples at its 32 kHz, 128 ms. Here the frame holds the same time
//! at the rate it's given, 6144 samples at 48 kHz and 24576 at 192, through a real FFT of any
//! length, so each bin is the module's 7.8 Hz wide; and only the bins below the module's top,
//! 125 Hz under 16 kHz, are used, so the warp, the quantising and the shift work on the same
//! frequencies as the module's, and it's band-limited as the module is. The 8-bit qualities run
//! it at half the rate, as the module does, so their frames are 256 ms and their bins 3.9 Hz.
//!
//! A frame's work, the FFT, the transformation and the inverse, a side at a time, is spread over
//! the hop after it, one part every so many frames, where the module does it in its main loop as
//! time allows; so no one block of the callback takes it all. What comes out is a frame and a hop
//! late, 160 ms, as the module's is.
//!
//! Adapted from Clouds' `clouds/dsp/pvoc/phase_vocoder.cc`, `stft.cc` and
//! `frame_transformation.cc`, `granular_processor.cc`'s spectral mode, the sine window of
//! `resources/lookup_tables.py`, and stmlib's `fast_atan2r` and its table, MIT:
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

use std::f64::consts::TAU;
use std::fmt;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::rng::Pcg32;

/// The module's frame, in its samples, at its rate, and how many bins it leaves off the top.
const MODULE_FRAME: usize = 4096;
const MODULE_RATE: f64 = 32_000.0;
const MODULE_TRUNCATION: usize = 16;
/// How many hops make a frame.
const HOPS: usize = 4;
/// The most kept spectra a side has: six in mono, two in stereo, as the module's memory allows.
pub(super) const MOST_TEXTURES: usize = 6;
/// The parts of a frame's work for each side: the FFT, the transformation and the inverse.
const PARTS: usize = 3;
/// How many points stmlib's sine table has a turn, and its `atan_lut`.
const SINE_POINTS: usize = 1024;
const ATAN_POINTS: usize = 512;
/// The warp's cubics, from the bin played to the bin read as fractions of the band, which `Size`
/// blends between: the band squeezed up, reversed, as it is, half way, and reversed another way.
const WARPS: [[f32; 4]; 6] = [
    [10.5882, -14.8824, 5.29412, 0.0],
    [-7.3333, 9.0, -1.79167, 0.125],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.5, 0.5, 0.0],
    [-7.3333, 9.5, -2.416667, 0.25],
    [-7.3333, 9.5, -2.416667, 0.25],
];

/// What a frame's transformation is given, worked out from the knobs as the module does.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct SpectralControl {
    /// Which kept spectrum is written and read, from 0 to 1.
    pub position: f32,
    /// Where between the warp's cubics, 0 to 1, as it is at a half.
    pub warp: f32,
    /// How readily new levels replace the kept ones, the module's refresh rate.
    pub refresh: f32,
    pub randomization: f32,
    pub quantization: f32,
    /// How far it shifts, as a ratio of frequencies.
    pub ratio: f32,
    pub frozen: bool,
}

impl SpectralControl {
    /// From the module's knobs, each 0 to 1, and its shift in semitones.
    pub fn new(
        position: f32,
        size: f32,
        semitones: f32,
        density: f32,
        texture: f32,
        frozen: bool,
    ) -> Self {
        let warp = size - 0.5;
        let randomization = ((density - 0.5) * (density - 0.5) * 4.2 - 0.05).clamp(0.0, 1.0);
        Self {
            position: position.clamp(0.0, 1.0),
            warp: (4.0 * warp * warp * warp + 0.5).clamp(0.0, 1.0),
            refresh: 0.01 + 0.99 * density,
            randomization,
            quantization: texture,
            ratio: (semitones / 12.0).exp2(),
            frozen,
        }
    }
}

/// The FFT and its inverse, planned once.
#[derive(Clone)]
struct Plans {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
}

impl fmt::Debug for Plans {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Plans")
    }
}

/// One side's frames: what came in and what goes out, in 16 bits as the module keeps them, its
/// kept spectra, and its bins' phases and how far each turned in the last hop.
#[derive(Debug, Clone)]
struct Side {
    analysis: Vec<i16>,
    synthesis: Vec<i16>,
    textures: Vec<f32>,
    phases: Vec<u16>,
    turns: Vec<u16>,
}

/// The phase vocoder, for one side or two.
#[derive(Debug, Clone)]
pub(super) struct Spectral {
    /// A frame's samples, a hop's, a frame and a hop, and the bins it uses.
    frame: usize,
    hop: usize,
    ring: usize,
    bins: usize,
    window: Vec<f32>,
    sides: [Side; 2],
    used_sides: usize,
    textures: usize,
    plans: Plans,
    /// The frame being worked on, its spectrum, and the FFT's own room.
    samples: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    /// The levels a frame took apart, played back, warped, then shifted, each with room past the
    /// end for reading between the last two; and the phases they're given.
    levels: Vec<f32>,
    replayed: Vec<f32>,
    warped: Vec<f32>,
    shifted: Vec<f32>,
    angles: Vec<u32>,
    sine: Vec<f32>,
    atan: Vec<u16>,
    /// Where the next sample goes in and comes out, where the frame being worked on starts, how
    /// far through the hop it is, and which part of the frame's work is next.
    at: usize,
    frame_start: usize,
    in_hop: usize,
    part: usize,
    started: bool,
}

/// The frame for a rate: the module's 128 ms, as near as an FFT of only small factors gets, so it
/// stays fast, and a whole number of hops.
fn frame_for(rate: f64) -> usize {
    let wanted = MODULE_FRAME as f64 * rate / MODULE_RATE;
    let smooth = |mut n: usize| {
        for factor in [2, 3, 5, 7] {
            while n.is_multiple_of(factor) {
                n /= factor;
            }
        }
        n == 1
    };
    let nearest = ((wanted / HOPS as f64).round() as usize).max(64) * HOPS;
    (0..nearest / 50)
        .flat_map(|by| [nearest + by * HOPS, nearest.saturating_sub(by * HOPS)])
        .find(|&n| n > 0 && smooth(n))
        .unwrap_or(nearest)
}

/// stmlib's `fast_atan2r`: a bin's phase, a whole turn being 65536, and its level.
#[inline]
fn polar(atan: &[u16], x: f32, y: f32) -> (u16, f32) {
    let squared = x * x + y * y;
    if squared == 0.0 {
        return (0, 0.0);
    }
    let inverse = 1.0 / squared.sqrt();
    let level = inverse * squared;
    let (across, up) = (x.abs(), y.abs());
    let mut angle = if up > across {
        16_384 - atan[(across * inverse * ATAN_POINTS as f32 + 0.5) as usize]
    } else {
        atan[(up * inverse * ATAN_POINTS as f32 + 0.5) as usize]
    };
    if x.is_sign_negative() != y.is_sign_negative() {
        angle = angle.wrapping_neg();
    }
    let half = if x.is_sign_negative() { 32_768 } else { 0 };
    (angle.wrapping_add(half), level)
}

/// Read between the samples of a table, as stmlib's `Interpolate`.
#[inline]
fn between(table: &[f32], index: f32) -> f32 {
    let index = index.max(0.0);
    let whole = (index as usize).min(table.len() - 2);
    let fraction = index - whole as f32;
    table[whole] + (table[whole + 1] - table[whole]) * fraction
}

#[inline]
fn clip16(x: i32) -> i16 {
    x.clamp(-32_768, 32_767) as i16
}

/// A number from its generator as stmlib's `Random::GetSample`, 16 bits.
#[inline]
fn sample(rng: &mut Pcg32) -> u16 {
    (rng.next_u32() >> 16) as u16
}

impl Spectral {
    /// Frames of the module's length at this rate, and everything they need.
    pub fn new(rate: u32) -> Self {
        let frame = frame_for(f64::from(rate));
        let hop = frame / HOPS;
        let ring = frame + hop;
        // The module's 2032 bins, at this rate's widths.
        let module_width = MODULE_RATE / MODULE_FRAME as f64;
        let width = f64::from(rate) / frame as f64;
        let top = frame / 2 - (MODULE_TRUNCATION * frame / MODULE_FRAME).max(1);
        let bins = (((MODULE_FRAME / 2 - MODULE_TRUNCATION) as f64 * module_width / width).round()
            as usize)
            .min(top);
        // The module's window: a power of a parabola, scaled so that it and the window half a
        // frame on square to one, so four of them a hop apart square to two.
        let power = |n: usize| {
            let t = n as f64 / frame as f64;
            (1.0 - (2.0 * t - 1.0).powi(2)).powf(1.25)
        };
        let window = (0..frame)
            .map(|n| {
                let half = n % (frame / 2);
                let compensation = (power(half).powi(2) + power(half + frame / 2).powi(2)).sqrt();
                (power(n) / compensation) as f32
            })
            .collect();
        let mut planner = RealFftPlanner::<f32>::new();
        let plans = Plans {
            forward: planner.plan_fft_forward(frame),
            inverse: planner.plan_fft_inverse(frame),
        };
        let scratch = plans
            .forward
            .get_scratch_len()
            .max(plans.inverse.get_scratch_len());
        let side = Side {
            analysis: vec![0; ring],
            synthesis: vec![0; ring],
            textures: vec![0.0; MOST_TEXTURES * bins],
            phases: vec![0; bins],
            turns: vec![0; bins],
        };
        let mut spectral = Self {
            frame,
            hop,
            ring,
            bins,
            window,
            sides: [side.clone(), side],
            used_sides: 2,
            textures: 2,
            samples: vec![0.0; frame],
            spectrum: vec![Complex::default(); frame / 2 + 1],
            scratch: vec![Complex::default(); scratch],
            plans,
            levels: vec![0.0; bins],
            replayed: vec![0.0; bins + 2],
            warped: vec![0.0; bins + 2],
            shifted: vec![0.0; bins + 2],
            angles: vec![0; bins],
            sine: (0..=SINE_POINTS + SINE_POINTS / 4)
                .map(|at| (at as f64 / SINE_POINTS as f64 * TAU).sin() as f32)
                .collect(),
            atan: (0..=ATAN_POINTS)
                .map(|at| (65_536.0 / TAU * (at as f64 / ATAN_POINTS as f64).asin()) as u16)
                .collect(),
            at: 0,
            frame_start: 0,
            in_hop: 0,
            part: 0,
            started: false,
        };
        spectral.reset(2, 2);
        spectral
    }

    /// Start afresh with nothing in it, for one side or two and so many kept spectra.
    pub fn reset(&mut self, sides: usize, textures: usize) {
        self.used_sides = sides.clamp(1, 2);
        self.textures = textures.clamp(1, MOST_TEXTURES);
        for side in &mut self.sides {
            side.analysis.fill(0);
            side.synthesis.fill(0);
            side.textures.fill(0.0);
            side.phases.fill(0);
            side.turns.fill(0);
        }
        self.at = 0;
        self.frame_start = 2 * self.hop % self.ring;
        self.in_hop = 0;
        self.part = 0;
        self.started = false;
    }

    /// How many frames late what comes out is, at the rate it runs at.
    pub fn latency(&self) -> usize {
        self.ring
    }

    /// The frame's samples and how many bins it uses, for the tests.
    #[cfg(test)]
    pub fn sizes(&self) -> (usize, usize) {
        (self.frame, self.bins)
    }

    /// Take a frame in and give the one a frame and a hop ago out; and every so often, a part of
    /// the last hop's frame's work.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        control: &SpectralControl,
        rng: &mut Pcg32,
    ) -> [f32; 2] {
        let mut out = [0.0; 2];
        for ((side, out), input) in self.sides[..self.used_sides]
            .iter_mut()
            .zip(&mut out)
            .zip(input)
        {
            side.analysis[self.at] = clip16((input * 32_768.0) as i32);
            *out = f32::from(side.synthesis[self.at]) / 16_384.0;
        }
        if self.used_sides == 1 {
            out[1] = out[0];
        }
        self.at += 1;
        if self.at == self.ring {
            self.at = 0;
        }
        self.in_hop += 1;
        if self.in_hop == self.hop {
            self.in_hop = 0;
            self.part = 0;
            self.started = true;
        }
        let parts = PARTS * self.used_sides;
        if self.started && self.part < parts && self.in_hop == self.part * self.hop / parts {
            let side = self.part / PARTS;
            match self.part % PARTS {
                0 => self.analyse(side),
                1 => self.transform(side, control, rng),
                _ => self.synthesise(side),
            }
            self.part += 1;
            if self.part == parts {
                self.frame_start = (self.frame_start + self.hop) % self.ring;
            }
        }
        out
    }

    /// A side's frame through the window and the FFT.
    fn analyse(&mut self, side: usize) {
        let analysis = &self.sides[side].analysis;
        let mut from = self.frame_start;
        for (sample, weight) in self.samples.iter_mut().zip(&self.window) {
            *sample = weight * f32::from(analysis[from]);
            from += 1;
            if from == self.ring {
                from = 0;
            }
        }
        let done = self.plans.forward.process_with_scratch(
            &mut self.samples,
            &mut self.spectrum,
            &mut self.scratch,
        );
        debug_assert!(done.is_ok());
    }

    /// Back through the inverse and the window, added to the frames before it.
    fn synthesise(&mut self, side: usize) {
        let done = self.plans.inverse.process_with_scratch(
            &mut self.spectrum,
            &mut self.samples,
            &mut self.scratch,
        );
        debug_assert!(done.is_ok());
        // The FFT there and back scales by its length, and four windows a hop apart square to two.
        let scale = 1.0 / (2 * self.frame) as f32;
        let fresh = self.frame - self.hop;
        let synthesis = &mut self.sides[side].synthesis;
        let mut to = self.frame_start;
        for (n, (sample, weight)) in self.samples.iter().zip(&self.window).enumerate() {
            let mut x = (sample * weight * scale) as i32;
            if n < fresh {
                x += i32::from(synthesis[to]);
            }
            synthesis[to] = clip16(x);
            to += 1;
            if to == self.ring {
                to = 0;
            }
        }
    }

    /// The module's `FrameTransformation::Process`, less its glitches, which want a trigger.
    fn transform(&mut self, side: usize, control: &SpectralControl, rng: &mut Pcg32) {
        let bins = self.bins;
        let textures = self.textures;
        let state = &mut self.sides[side];
        if !control.frozen {
            // The module's spectrum has the other sign on its imaginary parts.
            self.levels[0] = 0.0;
            for bin in 1..bins {
                let value = self.spectrum[bin];
                let (angle, level) = polar(&self.atan, value.re, -value.im);
                state.turns[bin] = angle.wrapping_sub(state.phases[bin]);
                state.phases[bin] = angle;
                self.levels[bin] = level;
            }
            store(
                &mut state.textures,
                &self.levels,
                bins,
                textures,
                control.position,
                control.refresh,
                rng,
            );
        }
        replay(
            &state.textures,
            &mut self.replayed[..bins],
            textures,
            control.position,
        );
        warp(&self.replayed, &mut self.warped, bins, control.warp);
        shift(&self.warped, &mut self.shifted, bins, control.ratio);
        quantize(
            &mut self.shifted[..bins],
            control.quantization,
            self.frame as f32,
        );

        // The phases: each bin's own, turned on by its last turn times the shift, so frozen they
        // keep turning, then scattered at random as far as `Density` says.
        for ((angle, phase), turn) in self
            .angles
            .iter_mut()
            .zip(&mut state.phases)
            .zip(&state.turns)
        {
            *angle = u32::from(*phase);
            *phase = phase.wrapping_add((f32::from(*turn) * control.ratio) as u32 as u16);
        }
        let r = ((control.randomization - 0.05) * 1.06).clamp(0.0, 1.0);
        let amount = (r * r * 32_768.0) as i32;
        if amount > 0 {
            for angle in &mut self.angles {
                let random = i32::from(sample(rng) as i16);
                *angle = angle.wrapping_add(((random * amount) >> 14) as u32);
            }
        }

        self.spectrum[0] = Complex::default();
        for bin in 1..bins {
            let level = self.shifted[bin];
            let at = usize::from(self.angles[bin] as u16 >> 6);
            self.spectrum[bin] = Complex::new(
                level * self.sine[at + SINE_POINTS / 4],
                -level * self.sine[at],
            );
        }
        self.spectrum[bins..].fill(Complex::default());
    }
}

/// The two kept spectra either side of `position`, and how far between them it is.
fn either_side(textures: usize, position: f32) -> (usize, usize, f32) {
    let index = position * (textures - 1) as f32;
    let whole = (index as usize).min(textures - 1);
    let next = if position >= 1.0 {
        whole
    } else {
        (whole + 1).min(textures - 1)
    };
    (whole, next, index - whole as f32)
}

/// Write a frame's levels into the kept spectra either side of `position`: below a `refresh` of a
/// half, each bin at random, less often the lower it is; above, blended in, then more and more
/// added to what's there.
fn store(
    kept: &mut [f32],
    levels: &[f32],
    bins: usize,
    textures: usize,
    position: f32,
    refresh: f32,
    rng: &mut Pcg32,
) {
    let (a, b, fraction) = either_side(textures, position);
    let (mut gain_a, mut gain_b) = (1.0 - fraction, fraction);
    let both = a != b;
    let (a, b) = (a * bins, b * bins);
    if refresh >= 0.5 {
        let feedback = 2.0 * (refresh - 0.5);
        if feedback < 0.5 {
            gain_a *= 1.0 - feedback;
            gain_b *= 1.0 - feedback;
            for (bin, &x) in levels.iter().enumerate() {
                kept[a + bin] += (x - kept[a + bin]) * gain_a;
                if both {
                    kept[b + bin] += (x - kept[b + bin]) * gain_b;
                }
            }
        } else {
            let t = (feedback - 0.5) * 0.7 + 0.5;
            let new = (t - 0.5) * (t - 0.5) * 2.0 + 0.5;
            let (new_a, new_b) = (gain_a * new, gain_b * new);
            let (old_a, old_b) = (1.0 - gain_a * (1.0 - t), 1.0 - gain_b * (1.0 - t));
            for (bin, &x) in levels.iter().enumerate() {
                kept[a + bin] = kept[a + bin] * old_a + x * new_a;
                if both {
                    kept[b + bin] = kept[b + bin] * old_b + x * new_b;
                }
            }
        }
    } else {
        let feedback = refresh * 2.0;
        let threshold = (feedback * feedback * 65_535.0) as u16;
        for (bin, &x) in levels.iter().enumerate() {
            if sample(rng) <= threshold {
                kept[a + bin] += (x - kept[a + bin]) * gain_a;
                if both {
                    kept[b + bin] += (x - kept[b + bin]) * gain_b;
                }
            }
        }
    }
}

/// The kept spectra either side of `position`, blended.
fn replay(kept: &[f32], out: &mut [f32], textures: usize, position: f32) {
    let bins = out.len();
    let (a, b, fraction) = either_side(textures, position);
    let (a, b) = (&kept[a * bins..][..bins], &kept[b * bins..][..bins]);
    for ((out, a), b) in out.iter_mut().zip(a).zip(b) {
        *out = a + (b - a) * fraction;
    }
}

/// Each bin played reads the bin the warp's cubic gives.
fn warp(from: &[f32], to: &mut [f32], bins: usize, amount: f32) {
    let amount = amount * 4.0;
    let whole = (amount as usize).min(4);
    let fraction = amount - whole as f32;
    let [a, b, c, d]: [f32; 4] = std::array::from_fn(|k| {
        WARPS[whole][k] + (WARPS[whole + 1][k] - WARPS[whole][k]) * fraction
    });
    let width = 1.0 / bins as f32;
    let mut f = 0.0;
    to[0] = 0.0;
    for to in &mut to[1..bins] {
        f += width;
        let read = (d + f * (c + f * (b + a * f))) * bins as f32;
        *to = between(from, read);
    }
}

/// Move the levels up or down by a ratio: read between bins going up, and share each between two
/// going down.
fn shift(from: &[f32], to: &mut [f32], bins: usize, ratio: f32) {
    if ratio == 1.0 {
        to[..bins].copy_from_slice(&from[..bins]);
    } else if ratio > 1.0 {
        let step = 1.0 / ratio;
        let mut index = 1.0;
        to[0] = 0.0;
        for to in &mut to[1..bins] {
            *to = between(from, index);
            index += step;
        }
    } else {
        to.fill(0.0);
        let mut index = 1.0_f32;
        for &level in &from[1..bins] {
            let whole = index as usize;
            let fraction = index - whole as f32;
            to[whole] += (1.0 - fraction) * level;
            to[whole + 1] += fraction * level;
            index += ratio;
        }
    }
}

/// Below `0.48`, round each level down to a coarser and coarser step; above `0.52`, bend the levels
/// so the strongest weaken and the weakest grow.
fn quantize(levels: &mut [f32], amount: f32, frame: f32) {
    if amount <= 0.48 {
        let amount = amount * 2.0;
        let down = 0.5 * (-9.0 * (1.0 - amount * amount)).exp2() / frame;
        let up = 1.0 / down;
        for level in levels {
            *level = up * ((down * *level) as i32) as f32;
        }
    } else if amount >= 0.52 {
        let amount = (amount - 0.52) * 2.0;
        let norm = levels.iter().copied().fold(0.0, f32::max);
        let inverse = 1.0 / (norm + 0.0001);
        for level in &mut levels[1..] {
            let x = *level * inverse;
            let bent = 4.0 * x * (1.0 - x) * (1.0 - x) * (1.0 - x);
            *level = (x + (bent - x) * amount) * norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_frames_hold_the_modules_time_and_bins_at_any_rate() {
        for (rate, frame) in [
            (32_000, 4096),
            (44_100, 5600),
            (48_000, 6144),
            (96_000, 12_288),
            (192_000, 24_576),
        ] {
            let spectral = Spectral::new(rate);
            let (found, bins) = spectral.sizes();
            assert_eq!(found, frame, "{rate}");
            // The module's 2032 bins up to 15.875 kHz.
            let top = bins as f64 * f64::from(rate) / frame as f64;
            assert!((top - 15_875.0).abs() < 10.0, "{rate}: {bins} {top}");
            // A frame and a hop, the module's 160 ms.
            let late = spectral.latency() as f64 / f64::from(rate);
            assert!((late - 0.16).abs() < 0.002, "{rate}: {late}");
        }
    }

    #[test]
    fn the_window_is_the_modules_and_four_a_hop_apart_square_to_two() {
        let spectral = Spectral::new(32_000);
        // A few of `lut_sine_window_4096`'s entries.
        let window = &spectral.window;
        assert!(window[0].abs() < 1e-6);
        assert!((window[2048] - 1.0).abs() < 1e-6);
        let hop = spectral.hop;
        for n in [0, 100, 777, hop - 1] {
            let sum: f32 = (0..HOPS).map(|k| window[n + k * hop].powi(2)).sum();
            assert!((sum - 2.0).abs() < 1e-4, "{n}: {sum}");
        }
    }

    #[test]
    fn its_phases_are_stmlibs() {
        let spectral = Spectral::new(32_000);
        // `atan_lut`'s first entries and its last.
        assert_eq!(&spectral.atan[..4], &[0, 20, 40, 61]);
        assert_eq!(spectral.atan[512], 16_384);
        let turn = 65_536.0 / std::f32::consts::TAU;
        for angle in [0.1_f32, 1.0, 2.0, 3.0, -0.4, -2.5] {
            let (found, level) = polar(&spectral.atan, 3.0 * angle.cos(), 3.0 * angle.sin());
            let wanted = (angle.rem_euclid(std::f32::consts::TAU) * turn) as i32;
            let error = (i32::from(found) - wanted + 32_768).rem_euclid(65_536) - 32_768;
            assert!(error.abs() < 80, "{angle}: {found} {wanted}");
            assert!((level - 3.0).abs() < 1e-5);
        }
    }

    #[test]
    fn left_alone_it_gives_back_what_came_in_twice_as_loud_a_frame_and_a_hop_late() {
        // The module's synthesis is read back at half its scale, so it comes out 6 dB up.
        let mut spectral = Spectral::new(48_000);
        spectral.reset(1, 2);
        let control = SpectralControl::new(0.0, 0.5, 0.0, 0.5, 0.5, false);
        let mut rng = Pcg32::new(1, 1);
        let late = spectral.latency();
        let tone = |n: usize| (n as f32 * 0.03).sin() * 0.25;
        let out: Vec<f32> = (0..late * 3)
            .map(|n| spectral.process([tone(n); 2], &control, &mut rng)[0])
            .collect();
        for (n, out) in out.iter().enumerate().skip(late * 2) {
            assert!((out - 2.0 * tone(n - late)).abs() < 0.01, "{n}: {out}");
        }
    }
}
