//! Noise and texture models from Mutable Instruments' Braids: noise through two resonant peaks,
//! clocked noise that loops, a cloud of sine grains, and a modem's data tone. Ported to Rust from
//! `digital_oscillator.cc`, at twice the program's rate as Braids runs at 96 kHz, through
//! [`super::braids_parts`]. The noisy ones keep Braids' whole-number arithmetic, whose grain is
//! part of their sound, and each note's noise comes from a seed of its own so a render repeats.
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

use super::braids_parts::{
    BRAIDS_BLOCK, INNER, Lcg, MODERATE_OVERDRIVE, Macro, Model, Pitch, sine, waveshape, whole,
};
use super::virtual_analog::Settings;

/// Noise through two resonant peaks, one at the note and one up to five octaves either way.
pub type TwinPeaks = Macro<TwinPeaksModel>;
/// Noise held for each tick of a clock at the note, quantised, that loops after a while.
pub type ClockedNoise = Macro<ClockedNoiseModel>;
/// A cloud of sine grains around the note.
pub type GranularCloud = Macro<GranularCloudModel>;
/// A carrier keyed through four symbols, as a modem sends data.
pub type DigitalModulation = Macro<DigitalModulationModel>;

/// What a frame holds to in 16 bits.
#[inline]
fn clip16(x: i32) -> i32 {
    x.clamp(-32_768, 32_767)
}

/// Braids' `lut_resonator_scale`, by MIDI note: the gain that keeps a resonator from clipping.
const RESONATOR_SCALE: [u16; 129] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 3, 3, 3, 4, 4, 4,
    5, 5, 6, 6, 7, 8, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 20, 22, 24, 27, 29, 32, 35, 38, 41, 45,
    49, 54, 58, 64, 70, 76, 83, 90, 98, 107, 117, 128, 139, 152, 166, 181, 197, 215, 234, 255, 256,
    256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256,
    256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256,
    256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256, 256,
];
/// Braids' `lut_granular_envelope`: a grain's window, a Hann window, then silence.
const GRANULAR_ENVELOPE: [u16; 513] = [
    0, 4, 19, 44, 78, 123, 177, 241, 314, 398, 490, 593, 705, 826, 957, 1097, 1247, 1405, 1572,
    1749, 1934, 2128, 2330, 2541, 2761, 2988, 3224, 3467, 3718, 3977, 4244, 4517, 4798, 5086, 5381,
    5682, 5989, 6303, 6623, 6949, 7281, 7618, 7960, 8308, 8660, 9017, 9378, 9744, 10113, 10487,
    10864, 11244, 11627, 12013, 12402, 12793, 13187, 13582, 13979, 14377, 14777, 15178, 15579,
    15981, 16383, 16785, 17187, 17588, 17989, 18389, 18787, 19184, 19579, 19973, 20364, 20753,
    21139, 21522, 21902, 22279, 22653, 23022, 23388, 23749, 24106, 24458, 24806, 25148, 25485,
    25817, 26143, 26463, 26777, 27084, 27385, 27680, 27968, 28249, 28522, 28789, 29048, 29299,
    29542, 29778, 30005, 30225, 30436, 30638, 30832, 31017, 31194, 31361, 31519, 31669, 31809,
    31940, 32061, 32173, 32276, 32368, 32452, 32525, 32589, 32643, 32688, 32722, 32747, 32762,
    32767, 32762, 32747, 32722, 32688, 32643, 32589, 32525, 32452, 32368, 32276, 32173, 32061,
    31940, 31809, 31669, 31519, 31361, 31194, 31017, 30832, 30638, 30436, 30225, 30005, 29778,
    29542, 29299, 29048, 28789, 28522, 28249, 27968, 27680, 27385, 27084, 26777, 26463, 26143,
    25817, 25485, 25148, 24806, 24458, 24106, 23749, 23388, 23022, 22653, 22279, 21902, 21522,
    21139, 20753, 20364, 19973, 19579, 19184, 18787, 18389, 17989, 17588, 17187, 16785, 16383,
    15981, 15579, 15178, 14777, 14377, 13979, 13582, 13187, 12793, 12402, 12013, 11627, 11244,
    10864, 10487, 10113, 9744, 9378, 9017, 8660, 8308, 7960, 7618, 7281, 6949, 6623, 6303, 5989,
    5682, 5381, 5086, 4798, 4517, 4244, 3977, 3718, 3467, 3224, 2988, 2761, 2541, 2330, 2128, 1934,
    1749, 1572, 1405, 1247, 1097, 957, 826, 705, 593, 490, 398, 314, 241, 177, 123, 78, 44, 19, 4,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0,
];
/// Braids' `lut_granular_envelope_rate`: how fast a grain's window goes, by `Length`.
const GRANULAR_ENVELOPE_RATE: [u16; 257] = [
    2048, 2070, 2092, 2115, 2138, 2161, 2185, 2209, 2233, 2257, 2282, 2307, 2332, 2357, 2383, 2409,
    2435, 2462, 2488, 2515, 2543, 2571, 2599, 2627, 2655, 2684, 2714, 2743, 2773, 2803, 2834, 2865,
    2896, 2927, 2959, 2991, 3024, 3057, 3090, 3124, 3158, 3192, 3227, 3262, 3298, 3334, 3370, 3407,
    3444, 3481, 3519, 3558, 3596, 3635, 3675, 3715, 3756, 3796, 3838, 3880, 3922, 3965, 4008, 4051,
    4096, 4140, 4185, 4231, 4277, 4323, 4371, 4418, 4466, 4515, 4564, 4614, 4664, 4715, 4766, 4818,
    4870, 4924, 4977, 5031, 5086, 5142, 5198, 5254, 5311, 5369, 5428, 5487, 5547, 5607, 5668, 5730,
    5792, 5855, 5919, 5983, 6049, 6114, 6181, 6248, 6316, 6385, 6455, 6525, 6596, 6668, 6741, 6814,
    6888, 6963, 7039, 7116, 7193, 7271, 7351, 7431, 7512, 7593, 7676, 7760, 7844, 7930, 8016, 8103,
    8192, 8281, 8371, 8462, 8554, 8647, 8742, 8837, 8933, 9030, 9129, 9228, 9328, 9430, 9533, 9637,
    9741, 9848, 9955, 10063, 10173, 10284, 10396, 10509, 10623, 10739, 10856, 10974, 11094, 11214,
    11336, 11460, 11585, 11711, 11838, 11967, 12098, 12229, 12363, 12497, 12633, 12771, 12910,
    13051, 13193, 13336, 13482, 13628, 13777, 13927, 14078, 14232, 14387, 14543, 14702, 14862,
    15024, 15187, 15353, 15520, 15689, 15860, 16032, 16207, 16384, 16562, 16742, 16925, 17109,
    17295, 17484, 17674, 17866, 18061, 18258, 18456, 18657, 18861, 19066, 19274, 19483, 19696,
    19910, 20127, 20346, 20568, 20792, 21018, 21247, 21478, 21712, 21949, 22188, 22429, 22673,
    22920, 23170, 23422, 23677, 23935, 24196, 24459, 24726, 24995, 25267, 25542, 25820, 26102,
    26386, 26673, 26964, 27257, 27554, 27854, 28157, 28464, 28774, 29087, 29404, 29724, 30048,
    30375, 30706, 31040, 31378, 31720, 32065, 32415, 32768,
];

/// A resonator's coefficient at a MIDI note, `2 cos(2 pi f)` for a filter run at half the rate,
/// in 32 768ths, as Braids' `lut_resonator_coefficient` is at 96 kHz.
fn resonator_coefficient(pitch: Pitch, note: f32) -> i32 {
    let f = (440.0 * ((note - 69.0) / 12.0).exp2() / (pitch.rate() / 2.0)).min(0.25);
    ((2.0 * (std::f32::consts::TAU * f).cos()) * 32_768.0).min(65_535.0) as i32
}

/// The gain that keeps a resonator at a MIDI note from clipping, from Braids' table.
fn resonator_scale(note: f32) -> i32 {
    let index = note.clamp(0.0, 127.999);
    let at = index as usize;
    let fraction = index - at as f32;
    let (a, b) = (
        f32::from(RESONATOR_SCALE[at]),
        f32::from(RESONATOR_SCALE[at + 1]),
    );
    (a + (b - a) * fraction) as i32
}

/// Multiply a sample by a scale in 65 536ths, rounding towards nought as Braids does.
#[inline]
fn scaled(sample: i32, scale: i32) -> i32 {
    if sample > 0 {
        (sample * scale) >> 16
    } else {
        -((-sample * scale) >> 16)
    }
}

/// A two-pole resonator's last two outputs.
#[derive(Debug, Clone, Copy, Default)]
struct Resonator {
    y1: i32,
    y2: i32,
}

impl Resonator {
    #[inline]
    fn process(&mut self, input: i32, coefficient: i32, q_squared: i32) -> i32 {
        let y = clip16(
            input + (self.y1.wrapping_mul(coefficient) >> 15)
                - (self.y2.wrapping_mul(q_squared) >> 15),
        );
        (self.y2, self.y1) = (self.y1, y);
        y
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TwinPeaksModel {
    resonators: [Resonator; 2],
    noise: Lcg,
}

impl Model for TwinPeaksModel {
    const LEVEL: f32 = 0.8;

    fn new() -> Self {
        Self {
            resonators: [Resonator::default(); 2],
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (sharpness, spacing) = (whole(settings.at(0)), whole(settings.at(1)));
        // In 64 bits, as Braids works these out unsigned, past what 32 signed bits hold.
        let q = i64::from(65_240 + (sharpness >> 7));
        let q_squared = ((q * q) >> 17) as i32;
        // The second peak up to five octaves either way of the note.
        let second = pitch.note + (spacing - 16_384) as f32 / 256.0;
        let notes = [pitch.note.clamp(0.0, 128.0), second.clamp(0.0, 128.0)];
        let coefficients =
            notes.map(|note| ((i64::from(resonator_coefficient(pitch, note)) * q) >> 16) as i32);
        let scales = notes.map(resonator_scale);
        // Narrower peaks are quieter, so they're turned up.
        let makeup = 8191 - (sharpness >> 2);
        let (pairs, _) = out.as_chunks_mut::<2>();
        for pair in pairs {
            let sample = i32::from(self.noise.sample()) >> 1;
            let [first, second] = &mut self.resonators;
            let mut sum = first.process(scaled(sample, scales[0]), coefficients[0], q_squared)
                + second.process(scaled(sample, scales[1]), coefficients[1], q_squared);
            sum = clip16(sum + ((sum * makeup) >> 13));
            let shaped = waveshape(&MODERATE_OVERDRIVE, sum as f32 / 32_768.0);
            *pair = [shaped; 2];
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ClockedNoiseModel {
    phase: u32,
    cycle: u32,
    state: u32,
    seed: u32,
    sample: i16,
    held: Option<[i32; 2]>,
    noise: Lcg,
}

impl Model for ClockedNoiseModel {
    const LEVEL: f32 = 0.4;

    fn new() -> Self {
        Self {
            phase: 0,
            cycle: 0,
            state: 0,
            seed: 0,
            sample: 0,
            held: None,
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
        // Each note starts its loop from a seed of its own, as Braids' strike does.
        self.seed = self.noise.word();
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        // A little hysteresis, so a setting resting between two values doesn't flicker.
        let (length, steps) = (whole(settings.at(0)), whole(settings.at(1)));
        let [mut held_length, mut held_steps] = self.held.unwrap_or([length, steps]);
        if (steps - held_steps).abs() > 64 {
            held_steps = steps;
        }
        if (length - held_length).abs() > 16 {
            held_length = length;
        }
        self.held = Some([held_length, held_steps]);

        // The clock runs up to three octaves over the note, short of the rate.
        let mut increment = (pitch.increment(0.0) * 4_294_967_296.0) as u32;
        for _ in 0..3 {
            if increment < 1 << 31 {
                increment <<= 1;
            }
        }
        // How many clocks the noise runs before it loops, by its own pitch.
        let cycle_note = (held_length - 16_384) as f32 / 128.0;
        let cycle_increment = pitch.at(cycle_note).increment(0.0) * 2.0;
        let cycle_increment = (cycle_increment * 4_294_967_296.0).min(u32::MAX as f32) as u32;
        let steps = (1 + (held_steps >> 10)).max(2) as u32;
        let divider = 65_536 / steps;
        for out in out.iter_mut() {
            self.phase = self.phase.wrapping_add(increment);
            if self.phase < increment {
                self.state = self
                    .state
                    .wrapping_mul(1_664_525)
                    .wrapping_add(1_013_904_223);
                self.cycle = self.cycle.wrapping_add(cycle_increment);
                if self.cycle < cycle_increment {
                    self.state = self.seed;
                    self.cycle = cycle_increment;
                }
                let mut sample = self.state & 0xFFFF;
                sample -= sample % divider;
                sample += divider >> 1;
                self.sample = sample as u16 as i16;
                // So the clock divides the rate exactly.
                self.phase = increment;
            }
            *out = f32::from(self.sample) / 32_768.0;
        }
    }
}

const GRAINS: usize = 4;

#[derive(Debug, Clone, Copy, Default)]
struct Grain {
    phase: u32,
    increment: u32,
    envelope: u32,
    envelope_increment: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct GranularCloudModel {
    grains: [Grain; GRAINS],
    noise: Lcg,
}

impl Model for GranularCloudModel {
    const LEVEL: f32 = 1.0;

    fn new() -> Self {
        Self {
            grains: [Grain::default(); GRAINS],
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (length, spread) = (whole(settings.at(0)), whole(settings.at(1)));
        let increment = (pitch.increment(0.0) * 4_294_967_296.0) as u32;
        // Braids gives each idle grain a quarter's chance to start each of its blocks, and these
        // blocks are longer, so the chance is too.
        let chance = (0x4000 * INNER / BRAIDS_BLOCK) as u32;
        for grain in &mut self.grains {
            if grain.envelope > 1 << 24 || grain.envelope_increment == 0 {
                grain.envelope_increment = 0;
                if self.noise.word() & 0xFFFF < chance {
                    grain.envelope_increment =
                        u32::from(GRANULAR_ENVELOPE_RATE[(length >> 7) as usize]) << 3;
                    grain.envelope = 0;
                    // Each grain strays from the note by up to a little over an octave.
                    let stray = (i32::from(self.noise.sample()) * spread) >> 16;
                    let step = (increment >> 8) as i32;
                    let moved = if stray < 0 {
                        step * (stray >> 8)
                    } else {
                        step * (stray >> 7)
                    };
                    grain.increment = increment.wrapping_add_signed(moved);
                }
            }
        }
        for out in out.iter_mut() {
            let mut sample = 0.0;
            for grain in &mut self.grains {
                grain.phase = grain.phase.wrapping_add(grain.increment);
                grain.envelope = grain.envelope.wrapping_add(grain.envelope_increment);
                let window = GRANULAR_ENVELOPE[((grain.envelope >> 16) as usize).min(512)];
                sample +=
                    sine(grain.phase as f32 / 4_294_967_296.0) * f32::from(window) / 131_072.0;
            }
            *out = sample.clamp(-1.0, 1.0);
        }
    }
}

/// The four symbols a carrier's phase and quadrature can take, as a modem's constellation.
const CONSTELLATION_Q: [f32; 4] = [0.705, -0.705, -0.705, 0.705];
const CONSTELLATION_I: [f32; 4] = [0.705, 0.705, -0.705, -0.705];

#[derive(Debug, Clone, Copy)]
pub struct DigitalModulationModel {
    phase: f32,
    symbol_phase: f32,
    count: u16,
    filter: i32,
    data: u8,
}

impl Model for DigitalModulationModel {
    const LEVEL: f32 = 0.35;

    fn new() -> Self {
        Self {
            phase: 0.0,
            symbol_phase: 0.0,
            count: 0,
            filter: 0,
            data: 0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (rate, data) = (settings.at(0), whole(settings.at(1)));
        let increment = pitch.increment(0.0);
        // Symbols from an octave under the note, down to three more octaves under.
        let symbol_increment = pitch.increment(-12.0 - (1.0 - rate) * 32.0);
        for out in out.iter_mut() {
            self.phase = (self.phase + increment).fract();
            self.symbol_phase += symbol_increment;
            if self.symbol_phase >= 1.0 {
                self.symbol_phase -= 1.0;
                self.count += 1;
                if self.count & 3 == 0 {
                    if self.count >= 64 + 4 * 256 {
                        self.count = 0;
                    }
                    // A preamble, then the data, one byte four symbols long, again and again.
                    self.data = if self.count < 32 {
                        0x00
                    } else if self.count < 48 {
                        0x99
                    } else if self.count < 64 {
                        0xCC
                    } else {
                        self.filter = (self.filter * 3 + data) >> 2;
                        (self.filter >> 7) as u8
                    };
                } else {
                    self.data >>= 2;
                }
            }
            let symbol = usize::from(self.data & 3);
            let (i, q) = (sine(self.phase), sine(self.phase + 0.25));
            *out = CONSTELLATION_Q[symbol] * q + CONSTELLATION_I[symbol] * i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::voices::virtual_analog::VirtualAnalog;

    const RATE: u32 = 48_000;

    fn settings(values: [f32; 2]) -> Settings {
        let mut places = [0.0; vt_model::settings::MAX_MACHINE_SETTINGS];
        places[..2].copy_from_slice(&values);
        Settings::from_places(places)
    }

    fn run<M: Model>(hertz: f32, values: [f32; 2], frames: usize) -> Vec<f32> {
        let mut voice = Macro::<M>::new(RATE);
        voice.seed(7);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings(values)))
            .collect()
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    fn differs(a: &[f32], b: &[f32]) -> f32 {
        let total: f32 = a.iter().zip(b).map(|(a, b)| (a - b).powi(2)).sum();
        (total / a.len() as f32).sqrt()
    }

    fn each(mut check: impl FnMut(&str, &dyn Fn(f32, [f32; 2], usize) -> Vec<f32>)) {
        check("Twin Peaks", &|hertz, values, frames| {
            run::<TwinPeaksModel>(hertz, values, frames)
        });
        check("Clocked", &|hertz, values, frames| {
            run::<ClockedNoiseModel>(hertz, values, frames)
        });
        check("Cloud", &|hertz, values, frames| {
            run::<GranularCloudModel>(hertz, values, frames)
        });
        check("Modem", &|hertz, values, frames| {
            run::<DigitalModulationModel>(hertz, values, frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            for values in [[0.5, 0.5], [0.75, 0.25]] {
                let out = run(220.0, values, 48_000);
                let decibels = 20.0 * (rms(&out[4_800..]) / rms(&va)).log10();
                assert!(decibels.abs() < 6.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            // A second, so the modem is past its preamble.
            let base = run(220.0, [0.8, 0.3], 48_000);
            for values in [[0.95, 0.3], [0.8, 0.9]] {
                let other = run(220.0, values, 48_000);
                assert!(differs(&base, &other) > 0.005, "{name} {values:?}");
            }
        });
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        each(|name, run| {
            for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
                for first in [0.0, 0.5, 1.0] {
                    for second in [0.0, 0.5, 1.0] {
                        for sample in run(hertz, [first, second], 2_400) {
                            assert!(
                                sample.is_finite() && sample.abs() <= 1.0,
                                "{name}: {sample} at {hertz} Hz {first} {second}"
                            );
                        }
                    }
                }
            }
        });
    }

    #[test]
    fn each_note_has_its_own_noise_and_a_render_repeats() {
        let with = |seed| {
            let mut voice = Macro::<TwinPeaksModel>::new(RATE);
            voice.seed(seed);
            (0..4_800)
                .map(|_| voice.next(220.0 / RATE as f32, settings([0.5, 0.5])))
                .collect::<Vec<f32>>()
        };
        assert_eq!(with(1), with(1));
        assert_ne!(with(1), with(2));
    }

    #[test]
    fn clocked_noise_loops() {
        // With the shortest loop, the noise repeats over and over, once the filters it comes down
        // through have settled.
        let out = run::<ClockedNoiseModel>(220.0, [1.0, 1.0], 48_000);
        let late = &out[24_000..];
        let period =
            (1..4_800).find(|lag| differs(&late[..4_800], &late[*lag..*lag + 4_800]) < 1e-4);
        assert!(period.is_some(), "it never repeats");
    }
}
