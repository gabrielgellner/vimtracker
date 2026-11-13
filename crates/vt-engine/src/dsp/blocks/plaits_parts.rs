//! Shared parts of Mutable Instruments' Plaits and its library stmlib, for the voices ported from
//! it: the state variable filter and its tangent approximations, a one-pole filter, a DC blocker,
//! a limiter, a cosine oscillator, Plaits' delay line, and two of its tables, taken from its
//! generated `resources.cc`. Ported to Rust from `stmlib/dsp/filter.h`, `stmlib/dsp/limiter.h`,
//! `stmlib/dsp/cosine_oscillator.h` and `plaits/dsp/physical_modelling/delay_line.h`.
//!
//! Copyright 2014, 2016 Emilie Gillet.
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

use std::f32::consts::PI;

use crate::rng::Pcg32;

use super::flush::Tiny;
use super::sides::SameBits;

/// Noise on `0..1`.
#[inline]
pub(in crate::dsp) fn random(rng: &mut Pcg32) -> f32 {
    rng.next_u32() as f32 / 4_294_967_296.0
}

/// `tan(pi f)` closely enough for frequencies below about 8 kHz.
#[inline]
pub(in crate::dsp) fn tan_dirty(f: f32) -> f32 {
    let a = 3.736e-01 * PI.powi(3);
    f * (PI + a * f * f)
}

/// `tan(pi f)` closely enough across the audio range, at 48 kHz.
#[inline]
pub(in crate::dsp) fn tan_fast(f: f32) -> f32 {
    let a = 3.260e-01 * PI.powi(3);
    let b = 1.823e-01 * PI.powi(5);
    let f2 = f * f;
    f * (PI + f2 * (a + b * f2))
}

/// `tan(pi f)` closely, at any rate.
#[inline]
#[allow(clippy::excessive_precision)]
pub(in crate::dsp) fn tan_accurate(f: f32) -> f32 {
    let a = 3.333314036e-01 * PI.powi(3);
    let b = 1.333923995e-01 * PI.powi(5);
    let c = 5.462117983e-02 * PI.powi(7);
    let d = 2.900525356e-02 * PI.powi(9);
    let f2 = f * f;
    f * (PI + f2 * (a + f2 * (b + f2 * (c + f2 * d))))
}

/// A state variable filter.
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::dsp) struct Svf {
    g: f32,
    r: f32,
    h: f32,
    state_1: f32,
    state_2: f32,
}

impl Svf {
    pub(in crate::dsp) fn set_g_q(&mut self, g: f32, resonance: f32) {
        self.g = g;
        self.r = 1.0 / resonance;
        self.h = 1.0 / (1.0 + self.r * g + g * g);
    }

    /// Set by a quick approximation, for frequencies a resonator would use.
    pub(in crate::dsp) fn set_fast(&mut self, f: f32, resonance: f32) {
        self.set_g_q(tan_fast(f), resonance);
    }

    pub(in crate::dsp) fn set_exact(&mut self, f: f32, resonance: f32) {
        self.set_g_q((PI * f.min(0.497)).tan(), resonance);
    }

    /// The next high-pass and band-pass outputs.
    #[inline]
    pub(in crate::dsp) fn process(&mut self, input: f32) -> (f32, f32) {
        let (hp, bp, _) = self.process_all(input);
        (hp, bp)
    }

    /// The next high-pass, band-pass and low-pass outputs.
    #[inline]
    pub(in crate::dsp) fn process_all(&mut self, input: f32) -> (f32, f32, f32) {
        let hp = (input - self.r * self.state_1 - self.g * self.state_1 - self.state_2) * self.h;
        let bp = self.g * hp + self.state_1;
        self.state_1 = self.g * hp + bp;
        let lp = self.g * bp + self.state_2;
        self.state_2 = self.g * bp + lp;
        (hp, bp, lp)
    }

    /// Start its state again from nothing once all of it is far below anything heard, for a frame
    /// with nothing coming in, so it falls to an exact zero rather than trading denormals (#260).
    #[inline]
    pub(in crate::dsp) fn let_go(&mut self) {
        if [self.state_1, self.state_2].tiny() {
            (self.state_1, self.state_2) = (0.0, 0.0);
        }
    }

    #[inline]
    pub(in crate::dsp) fn low_pass(&mut self, input: f32) -> f32 {
        self.process_all(input).2
    }

    #[inline]
    pub(in crate::dsp) fn band_pass(&mut self, input: f32) -> f32 {
        self.process_all(input).1
    }

    /// A low-pass at `mode` 0, through a band-pass in the middle, to a high-pass at 1.
    #[inline]
    pub(in crate::dsp) fn low_to_high(&mut self, input: f32, mode: f32) -> f32 {
        let hp_gain = (-mode * 2.0 + 1.0).min(0.0);
        let bp_gain = 1.0 - 2.0 * (mode - 0.5).abs();
        let lp_gain = (1.0 - mode * 2.0).max(0.0);
        let (hp, bp, lp) = self.process_all(input);
        hp_gain * hp + bp_gain * bp + lp_gain * lp
    }
}

impl SameBits for Svf {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        [self.g, self.r, self.h, self.state_1, self.state_2].same_bits(&[
            other.g,
            other.r,
            other.h,
            other.state_1,
            other.state_2,
        ])
    }
}

/// Takes out a constant offset, which feedback would otherwise build up.
#[derive(Debug, Clone, Copy)]
pub(in crate::dsp) struct DcBlocker {
    pole: f32,
    x: f32,
    y: f32,
}

impl DcBlocker {
    pub(in crate::dsp) fn new(pole: f32) -> Self {
        Self {
            pole,
            x: 0.0,
            y: 0.0,
        }
    }

    #[inline]
    pub(in crate::dsp) fn process(&mut self, input: f32) -> f32 {
        let previous = self.x;
        self.x = input;
        self.y = self.y * self.pole + input - previous;
        self.y
    }
}

/// A one-pole filter, as stmlib's `OnePole` with its quick tangent.
#[derive(Debug, Clone, Copy, Default)]
pub(in crate::dsp) struct OnePole {
    gain: f32,
    state: f32,
}

impl OnePole {
    pub(in crate::dsp) fn set_dirty(&mut self, f: f32) {
        let g = tan_dirty(f);
        self.gain = g / (1.0 + g);
    }

    #[inline]
    pub(in crate::dsp) fn high_pass(&mut self, input: f32) -> f32 {
        let low = (input - self.state) * self.gain + self.state;
        self.state = (input - self.state) * self.gain + low;
        input - low
    }
}

/// Holds a level within about 1, following its peaks, as Plaits does for its louder voices.
#[derive(Debug, Clone, Copy)]
pub(in crate::dsp) struct Limiter {
    peak: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self { peak: 0.5 }
    }
}

impl Limiter {
    #[inline]
    pub(in crate::dsp) fn process(&mut self, pre_gain: f32, input: f32) -> f32 {
        let s = input * pre_gain;
        let error = s.abs() - self.peak;
        self.peak += if error > 0.0 { 0.05 } else { 0.00002 } * error;
        let gain = if self.peak <= 1.0 {
            1.0
        } else {
            1.0 / self.peak
        };
        // Plaits clips what's left as it turns it into 16-bit samples.
        soft_limit(s * gain * 0.8).clamp(-1.0, 1.0)
    }
}

#[inline]
fn soft_limit(x: f32) -> f32 {
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// Picks one of a number of steps from a value on `0..=1`, leaving a step only once the value is
/// a margin past its edge, so a setting near an edge doesn't flicker between two. stmlib's
/// `HysteresisQuantizer2`.
#[derive(Debug, Clone, Copy)]
pub(in crate::dsp) struct HysteresisQuantizer {
    steps: i32,
    hysteresis: f32,
    scale: f32,
    offset: f32,
    value: i32,
}

impl HysteresisQuantizer {
    /// A symmetric one puts the first and last steps' middles at 0 and 1; otherwise each step
    /// gets an equal share of the range.
    pub(in crate::dsp) fn new(steps: usize, hysteresis: f32, symmetric: bool) -> Self {
        Self {
            steps: steps as i32,
            hysteresis,
            scale: if symmetric { steps - 1 } else { steps } as f32,
            offset: if symmetric { 0.0 } else { -0.5 },
            value: 0,
        }
    }

    #[inline]
    pub(in crate::dsp) fn process(&mut self, value: f32) -> usize {
        let value = value * self.scale + self.offset;
        let sign = if value > self.value as f32 { -1.0 } else { 1.0 };
        // Truncating towards zero, as C++ does.
        self.value = ((value + sign * self.hysteresis + 0.5) as i32).clamp(0, self.steps - 1);
        self.value as usize
    }

    #[inline]
    pub(in crate::dsp) fn value(&self) -> usize {
        self.value as usize
    }
}

/// A cosine on `0..1` worked out by a recurrence, from a quick approximation of its start, as
/// stmlib's `COSINE_OSCILLATOR_APPROXIMATE`. It starts at its peak.
#[derive(Debug, Clone, Copy)]
pub(in crate::dsp) struct CosineOscillator {
    coefficient: f32,
    y0: f32,
    y1: f32,
}

impl CosineOscillator {
    pub(in crate::dsp) fn new(frequency: f32) -> Self {
        let mut sign = 16.0;
        let mut frequency = frequency - 0.25;
        if frequency < 0.0 {
            frequency = -frequency;
        } else if frequency > 0.5 {
            frequency -= 0.5;
        } else {
            sign = -16.0;
        }
        let coefficient = sign * frequency * (1.0 - 2.0 * frequency);
        Self {
            coefficient,
            y0: 0.5,
            y1: coefficient * 0.25,
        }
    }

    #[inline]
    pub(in crate::dsp) fn next(&mut self) -> f32 {
        let value = self.y0;
        self.y0 = self.coefficient * self.y0 - self.y1;
        self.y1 = value;
        value + 0.5
    }

    #[inline]
    pub(in crate::dsp) fn value(&self) -> f32 {
        self.y1 + 0.5
    }
}

/// A delay line of `N` samples, written backwards as Plaits' is, so a read is the delay ahead.
#[derive(Debug, Clone, Copy)]
pub(in crate::dsp) struct DelayLine<const N: usize> {
    line: [f32; N],
    write: usize,
}

impl<const N: usize> DelayLine<N> {
    pub(in crate::dsp) fn new() -> Self {
        Self {
            line: [0.0; N],
            write: 0,
        }
    }

    #[inline]
    pub(in crate::dsp) fn write(&mut self, sample: f32) {
        self.line[self.write] = sample;
        self.write = (self.write + N - 1) % N;
    }

    #[inline]
    pub(in crate::dsp) fn allpass(&mut self, sample: f32, delay: usize, coefficient: f32) -> f32 {
        let read = self.line[(self.write + delay) % N];
        let write = sample + coefficient * read;
        self.write(write);
        -write * coefficient + read
    }

    #[inline]
    pub(in crate::dsp) fn read(&self, delay: f32) -> f32 {
        let whole = delay as usize;
        let fraction = delay - whole as f32;
        let a = self.line[(self.write + whole) % N];
        let b = self.line[(self.write + whole + 1) % N];
        a + (b - a) * fraction
    }

    #[inline]
    pub(in crate::dsp) fn read_hermite(&self, delay: f32) -> f32 {
        let whole = delay as usize;
        let fraction = delay - whole as f32;
        let t = self.write + whole + N;
        let xm1 = self.line[(t - 1) % N];
        let x0 = self.line[t % N];
        let x1 = self.line[(t + 1) % N];
        let x2 = self.line[(t + 2) % N];
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b_neg = w + a;
        (((a * fraction) - b_neg) * fraction + c) * fraction + x0
    }
}

/// A table read between its entries, at `index` times `size`.
#[inline]
pub(in crate::dsp) fn interpolate(table: &[f32], index: f32, size: f32) -> f32 {
    let index = (index * size).clamp(0.0, (table.len() - 2) as f32);
    let whole = index as usize;
    let fraction = index - whole as f32;
    table[whole] + (table[whole + 1] - table[whole]) * fraction
}

#[allow(clippy::excessive_precision)]
/// How far a state variable filter's resonance shifts a string's pitch, by its cutoff in semitones.
pub(in crate::dsp) const SVF_SHIFT: [f32; 257] = [
    7.500000000e-01,
    7.591880421e-01,
    7.683455389e-01,
    7.774424499e-01,
    7.864497239e-01,
    7.953397451e-01,
    8.040867240e-01,
    8.126670211e-01,
    8.210593968e-01,
    8.292451828e-01,
    8.372083767e-01,
    8.449356653e-01,
    8.524163823e-01,
    8.596424124e-01,
    8.666080494e-01,
    8.733098228e-01,
    8.797462999e-01,
    8.859178746e-01,
    8.918265520e-01,
    8.974757332e-01,
    9.028700082e-01,
    9.080149595e-01,
    9.129169800e-01,
    9.175831064e-01,
    9.220208696e-01,
    9.262381615e-01,
    9.302431183e-01,
    9.340440198e-01,
    9.376492031e-01,
    9.410669892e-01,
    9.443056230e-01,
    9.473732226e-01,
    9.502777394e-01,
    9.530269262e-01,
    9.556283121e-01,
    9.580891845e-01,
    9.604165758e-01,
    9.626172547e-01,
    9.646977214e-01,
    9.666642057e-01,
    9.685226683e-01,
    9.702788030e-01,
    9.719380415e-01,
    9.735055596e-01,
    9.749862835e-01,
    9.763848977e-01,
    9.777058532e-01,
    9.789533760e-01,
    9.801314757e-01,
    9.812439545e-01,
    9.822944161e-01,
    9.832862746e-01,
    9.842227627e-01,
    9.851069409e-01,
    9.859417053e-01,
    9.867297957e-01,
    9.874738036e-01,
    9.881761792e-01,
    9.888392394e-01,
    9.894651739e-01,
    9.900560522e-01,
    9.906138300e-01,
    9.911403551e-01,
    9.916373732e-01,
    9.921065333e-01,
    9.925493929e-01,
    9.929674233e-01,
    9.933620136e-01,
    9.937344761e-01,
    9.940860496e-01,
    9.944179041e-01,
    9.947311441e-01,
    9.950268127e-01,
    9.953058946e-01,
    9.955693195e-01,
    9.958179650e-01,
    9.960526598e-01,
    9.962741861e-01,
    9.964832825e-01,
    9.966806459e-01,
    9.968669346e-01,
    9.970427696e-01,
    9.972087374e-01,
    9.973653916e-01,
    9.975132546e-01,
    9.976528197e-01,
    9.977845524e-01,
    9.979088923e-01,
    9.980262540e-01,
    9.981370293e-01,
    9.982415876e-01,
    9.983402778e-01,
    9.984334293e-01,
    9.985213529e-01,
    9.986043419e-01,
    9.986826732e-01,
    9.987566083e-01,
    9.988263939e-01,
    9.988922628e-01,
    9.989544348e-01,
    9.990131175e-01,
    9.990685067e-01,
    9.991207871e-01,
    9.991701333e-01,
    9.992167099e-01,
    9.992606725e-01,
    9.993021676e-01,
    9.993413338e-01,
    9.993783018e-01,
    9.994131950e-01,
    9.994461297e-01,
    9.994772160e-01,
    9.995065576e-01,
    9.995342523e-01,
    9.995603927e-01,
    9.995850659e-01,
    9.996083544e-01,
    9.996303357e-01,
    9.996510834e-01,
    9.996706665e-01,
    9.996891506e-01,
    9.997065972e-01,
    9.997230647e-01,
    9.997386078e-01,
    9.997532786e-01,
    9.997671260e-01,
    9.997801962e-01,
    9.997925329e-01,
    9.998041771e-01,
    9.998151678e-01,
    9.998255416e-01,
    9.998353332e-01,
    9.998445753e-01,
    9.998532986e-01,
    9.998615323e-01,
    9.998693039e-01,
    9.998766393e-01,
    9.998835630e-01,
    9.998900981e-01,
    9.998962664e-01,
    9.999020885e-01,
    9.999075839e-01,
    9.999127708e-01,
    9.999176666e-01,
    9.999222876e-01,
    9.999266493e-01,
    9.999307661e-01,
    9.999346519e-01,
    9.999383196e-01,
    9.999417815e-01,
    9.999450491e-01,
    9.999481332e-01,
    9.999510443e-01,
    9.999537919e-01,
    9.999563854e-01,
    9.999588333e-01,
    9.999611438e-01,
    9.999633246e-01,
    9.999653831e-01,
    9.999673260e-01,
    9.999691598e-01,
    9.999708907e-01,
    9.999725245e-01,
    9.999740666e-01,
    9.999755221e-01,
    9.999768960e-01,
    9.999781927e-01,
    9.999794167e-01,
    9.999805719e-01,
    9.999816623e-01,
    9.999826915e-01,
    9.999836630e-01,
    9.999845799e-01,
    9.999854454e-01,
    9.999862623e-01,
    9.999870333e-01,
    9.999877611e-01,
    9.999884480e-01,
    9.999890964e-01,
    9.999897083e-01,
    9.999902860e-01,
    9.999908312e-01,
    9.999913458e-01,
    9.999918315e-01,
    9.999922900e-01,
    9.999927227e-01,
    9.999931311e-01,
    9.999935167e-01,
    9.999938805e-01,
    9.999942240e-01,
    9.999945482e-01,
    9.999948542e-01,
    9.999951430e-01,
    9.999954156e-01,
    9.999956729e-01,
    9.999959157e-01,
    9.999961450e-01,
    9.999963613e-01,
    9.999965656e-01,
    9.999967583e-01,
    9.999969403e-01,
    9.999971120e-01,
    9.999972741e-01,
    9.999974271e-01,
    9.999975715e-01,
    9.999977078e-01,
    9.999978364e-01,
    9.999979579e-01,
    9.999980725e-01,
    9.999981807e-01,
    9.999982828e-01,
    9.999983792e-01,
    9.999984701e-01,
    9.999985560e-01,
    9.999986370e-01,
    9.999987135e-01,
    9.999987857e-01,
    9.999988539e-01,
    9.999989182e-01,
    9.999989789e-01,
    9.999990362e-01,
    9.999990903e-01,
    9.999991414e-01,
    9.999991896e-01,
    9.999992351e-01,
    9.999992780e-01,
    9.999993185e-01,
    9.999993568e-01,
    9.999993929e-01,
    9.999994269e-01,
    9.999994591e-01,
    9.999994895e-01,
    9.999995181e-01,
    9.999995452e-01,
    9.999995707e-01,
    9.999995948e-01,
    9.999996175e-01,
    9.999996390e-01,
    9.999996593e-01,
    9.999996784e-01,
    9.999996964e-01,
    9.999997135e-01,
    9.999997296e-01,
    9.999997447e-01,
    9.999997591e-01,
    9.999997726e-01,
    9.999997853e-01,
    9.999997974e-01,
    9.999998088e-01,
    9.999998195e-01,
    9.999998296e-01,
    9.999998392e-01,
    9.999998482e-01,
    9.999998567e-01,
    9.999998648e-01,
    9.999998724e-01,
    9.999998795e-01,
];

#[allow(clippy::excessive_precision)]
/// How a resonator's partials stretch, by `Harm`.
pub(in crate::dsp) const STIFFNESS: [f32; 65] = [
    -6.250000000e-02,
    -5.859375000e-02,
    -5.468750000e-02,
    -5.078125000e-02,
    -4.687500000e-02,
    -4.296875000e-02,
    -3.906250000e-02,
    -3.515625000e-02,
    -3.125000000e-02,
    -2.734375000e-02,
    -2.343750000e-02,
    -1.953125000e-02,
    -1.562500000e-02,
    -1.171875000e-02,
    -7.812500000e-03,
    -3.906250000e-03,
    0.000000000e+00,
    0.000000000e+00,
    0.000000000e+00,
    0.000000000e+00,
    1.009582073e-03,
    2.416076364e-03,
    4.002252878e-03,
    5.791066350e-03,
    7.808404022e-03,
    1.008346028e-02,
    1.264915914e-02,
    1.554263074e-02,
    1.880574864e-02,
    2.248573583e-02,
    2.663584813e-02,
    3.131614488e-02,
    3.659435812e-02,
    4.254687278e-02,
    4.925983210e-02,
    5.683038428e-02,
    6.536808837e-02,
    7.499649981e-02,
    8.585495846e-02,
    9.810060511e-02,
    1.119106556e-01,
    1.274849653e-01,
    1.450489216e-01,
    1.648567056e-01,
    1.871949702e-01,
    2.123869891e-01,
    2.407973346e-01,
    2.728371538e-01,
    3.089701187e-01,
    3.497191360e-01,
    3.956739150e-01,
    4.474995013e-01,
    5.059459012e-01,
    5.718589358e-01,
    6.461924814e-01,
    7.300222738e-01,
    8.245614757e-01,
    9.311782340e-01,
    1.000037649e+00,
    1.005639154e+00,
    1.048005353e+00,
    1.183990632e+00,
    1.457101344e+00,
    2.000000000e+00,
    2.000000000e+00,
];
