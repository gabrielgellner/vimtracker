//! Physical models from Mutable Instruments' Braids: a bowed string, a reed blown into a bore, a
//! flute's jet and bore, and its struck bell and drum, made of decaying partials. Ported to Rust
//! from `digital_oscillator.cc`, at twice the program's rate as Braids runs at 96 kHz, through
//! [`super::braids_parts`]. The waveguides keep Braids' whole-number arithmetic and its 8- and
//! 16-bit delay lines, whose grain is part of their sound, and are tuned from the note at the
//! rate they run at.
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

use super::braids_parts::{BRAIDS_BLOCK, INNER, Lcg, Macro, Model, Pitch, sine, whole};
use super::virtual_analog::Settings;

/// A bowed string: a bow's friction against a string between a bridge and a nut.
pub type Bowed = Macro<BowedModel>;
/// A reed blown into a bore, as a clarinet's.
pub type Blown = Macro<BlownModel>;
/// A jet of air across a flute's bore.
pub type Fluted = Macro<FlutedModel>;
/// A bell of eleven partials, struck and dying away.
pub type StruckBell = Macro<BellModel>;
/// A drum of six partials and filtered noise, struck and dying away.
pub type StruckDrum = Macro<DrumModel>;

/// The delay of one cycle of the note, in frames at the rate it runs at, in 65 536ths: Braids'
/// `ComputeDelay`, no shorter than its highest note's.
#[inline]
fn cycle_delay(pitch: Pitch) -> u32 {
    (65_536.0 / pitch.increment(0.0)) as u32
}

/// stmlib's `Mix` in whole numbers: from `a` at 0 to `b` at 65 535.
#[inline]
fn mix16(a: i32, b: i32, balance: i32) -> i32 {
    (a * (65_535 - balance) + b * balance) >> 16
}

/// What a frame holds to in 16 bits.
#[inline]
fn clip16(x: i32) -> i32 {
    x.clamp(-32_768, 32_767)
}

const BRIDGE_LENGTH: usize = 1024;
const NECK_LENGTH: usize = 4096;
const BRIDGE_LP_GAIN: i32 = 14_008;
const BRIDGE_LP_POLE: i32 = 18_022;
const BIQUAD_GAIN: i32 = 6553;
const BIQUAD_POLE_1: i32 = 6948;
const BIQUAD_POLE_2: i32 = -2959;

#[derive(Debug, Clone, Copy)]
pub struct BowedModel {
    bridge: [i8; BRIDGE_LENGTH],
    neck: [i8; NECK_LENGTH],
    at: usize,
    excitation: usize,
    low_pass: i32,
    biquad: [i32; 2],
    previous: i32,
}

impl Model for BowedModel {
    const LEVEL: f32 = 1.4;

    fn new() -> Self {
        Self {
            bridge: [0; BRIDGE_LENGTH],
            neck: [0; NECK_LENGTH],
            at: 0,
            excitation: 0,
            low_pass: 0,
            biquad: [0; 2],
            previous: 0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        // Harder bow pressure going up, and the bow nearer the middle of the string.
        let pressure = 172 - (whole(settings.at(0)) >> 8);
        let position = 6 + (whole(settings.at(1)) >> 9);
        // It runs at half the rate, less a frame each way for the filter's own delay.
        let mut delay = (cycle_delay(pitch) >> 1).saturating_sub(2 << 16);
        let mut bridge_delay = (delay >> 8) * position as u32;
        // An octave up when the note is too low for the lines.
        while delay - bridge_delay > ((NECK_LENGTH as u32 - 1) << 16)
            || bridge_delay > ((BRIDGE_LENGTH as u32 - 1) << 16)
        {
            delay >>= 1;
            bridge_delay >>= 1;
        }
        let neck_delay = delay - bridge_delay;
        let (bridge_whole, bridge_fraction) = (
            (bridge_delay >> 16) as usize,
            (bridge_delay & 0xFFFF) as i32,
        );
        let (neck_whole, neck_fraction) =
            ((neck_delay >> 16) as usize, (neck_delay & 0xFFFF) as i32);
        let (pairs, _) = out.as_chunks_mut::<2>();
        for pair in pairs {
            let bridge_at = self.at + 2 * BRIDGE_LENGTH - bridge_whole;
            let neck_at = self.at + 2 * NECK_LENGTH - neck_whole;
            let bridge_a = i32::from(self.bridge[bridge_at % BRIDGE_LENGTH]);
            let bridge_b = i32::from(self.bridge[(bridge_at - 1) % BRIDGE_LENGTH]);
            let nut_a = i32::from(self.neck[neck_at % NECK_LENGTH]);
            let nut_b = i32::from(self.neck[(neck_at - 1) % NECK_LENGTH]);
            let bridge_value = mix16(bridge_a, bridge_b, bridge_fraction) << 8;
            let nut_value = mix16(nut_a, nut_b, neck_fraction) << 8;
            self.low_pass = (bridge_value * BRIDGE_LP_GAIN + self.low_pass * BRIDGE_LP_POLE) >> 15;
            let bridge_reflection = -self.low_pass;
            let nut_reflection = -nut_value;
            let string_velocity = bridge_reflection + nut_reflection;
            let envelope =
                |at: usize| i32::from(BOWING_ENVELOPE[at.min(BOWING_ENVELOPE.len() - 1)]);
            let bow_velocity =
                (envelope(self.excitation >> 1) + envelope((self.excitation + 1) >> 1)) >> 1;
            let velocity_delta = bow_velocity - string_velocity;
            let friction = ((velocity_delta * pressure) >> 5).abs().min((1 << 17) - 1);
            let friction = i32::from(BOWING_FRICTION[(friction >> 9) as usize]);
            let new_velocity = friction.wrapping_mul(velocity_delta) >> 15;
            self.neck[self.at % NECK_LENGTH] = ((bridge_reflection + new_velocity) >> 8) as i8;
            self.bridge[self.at % BRIDGE_LENGTH] = ((nut_reflection + new_velocity) >> 8) as i8;
            self.at += 1;

            // The body, a resonant filter.
            let [y0, y1] = self.biquad;
            let temp = ((bridge_value * BIQUAD_GAIN) >> 15)
                + ((y0 * BIQUAD_POLE_1) >> 12)
                + ((y1 * BIQUAD_POLE_2) >> 12);
            let sample = clip16(temp - y1);
            self.biquad = [temp, y0];
            *pair = [
                ((sample + self.previous) >> 1) as f32 / 32_768.0,
                sample as f32 / 32_768.0,
            ];
            self.previous = sample;
            self.excitation += 1;
        }
        // The bow holds at the end of its attack.
        let held = (BOWING_ENVELOPE.len() - 32) << 1;
        self.excitation = self.excitation.min(held);
        self.at %= NECK_LENGTH;
    }
}

const BORE_LENGTH: usize = 2048;
const BREATH_PRESSURE: i32 = 26_214;
const REFLECTION: i32 = -3891;
const REED_SLOPE: i32 = -1229;
const REED_OFFSET: i32 = 22_938;

#[derive(Debug, Clone, Copy)]
pub struct BlownModel {
    bore: [i16; BORE_LENGTH],
    at: usize,
    low_pass: i32,
    body: i32,
    noise: Lcg,
}

impl Model for BlownModel {
    const LEVEL: f32 = 1.0;

    fn new() -> Self {
        Self {
            bore: [0; BORE_LENGTH],
            at: 0,
            low_pass: 0,
            body: 0,
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let mut delay = (cycle_delay(pitch) >> 1).saturating_sub(1 << 16);
        while delay > ((BORE_LENGTH as u32 - 1) << 16) {
            delay >>= 1;
        }
        let (bore_whole, bore_fraction) = ((delay >> 16) as usize, (delay & 0xFFFF) as i32);
        // Less noise in the breath going up.
        let breath_noise = 28_000 - (whole(settings.at(0)) >> 1);
        let tone = ((pitch.note * 128.0) as i32 - 8192 + (whole(settings.at(1)) >> 1)) >> 7;
        let coefficient = i32::from(FLUTE_BODY_FILTER[tone.clamp(0, 127) as usize]);
        for out in out.iter_mut() {
            let mut breath = (i32::from(self.noise.sample()) * breath_noise) >> 15;
            breath = (breath * BREATH_PRESSURE) >> 15;
            breath += BREATH_PRESSURE;
            let bore_at = self.at + 2 * BORE_LENGTH - bore_whole;
            let a = i32::from(self.bore[bore_at % BORE_LENGTH]);
            let b = i32::from(self.bore[(bore_at - 1) % BORE_LENGTH]);
            let value = mix16(a, b, bore_fraction);
            let mut pressure = (value >> 1) + self.low_pass;
            self.low_pass = value >> 1;
            pressure = (REFLECTION * pressure) >> 12;
            pressure -= breath;
            let reed = clip16(((pressure * REED_SLOPE) >> 12) + REED_OFFSET);
            let sample = clip16(((pressure * reed) >> 15) + breath);
            self.bore[self.at % BORE_LENGTH] = sample as i16;
            self.at += 1;
            self.body =
                ((coefficient * sample + (4096 - coefficient) * self.body) >> 12) as i16 as i32;
            *out = self.body as f32 / 32_768.0;
        }
        self.at %= BORE_LENGTH;
    }
}

const JET_LENGTH: usize = 1024;
const FLUTE_BORE_LENGTH: usize = 4096;
const DC_BLOCKING_POLE: i32 = 4055;

#[derive(Debug, Clone, Copy)]
pub struct FlutedModel {
    jet: [i8; JET_LENGTH],
    bore: [i8; FLUTE_BORE_LENGTH],
    at: usize,
    excitation: usize,
    low_pass: i32,
    dc: [i32; 2],
    noise: Lcg,
}

impl Model for FlutedModel {
    const LEVEL: f32 = 0.55;

    fn new() -> Self {
        Self {
            jet: [0; JET_LENGTH],
            bore: [0; FLUTE_BORE_LENGTH],
            at: 0,
            excitation: 0,
            low_pass: 0,
            dc: [0; 2],
            noise: Lcg::new(0),
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let mut bore_delay = (cycle_delay(pitch) << 1).saturating_sub(2 << 16);
        // The jet's share of the length, from 48 to 79 256ths.
        let mut jet_delay = (bore_delay >> 8) * (48 + (whole(settings.at(1)) >> 10)) as u32;
        bore_delay -= jet_delay;
        while bore_delay > ((FLUTE_BORE_LENGTH as u32 - 1) << 16)
            || jet_delay > ((JET_LENGTH as u32 - 1) << 16)
        {
            bore_delay >>= 1;
            jet_delay >>= 1;
        }
        let (bore_whole, bore_fraction) =
            ((bore_delay >> 16) as usize, (bore_delay & 0xFFFF) as i32);
        let (jet_whole, jet_fraction) = ((jet_delay >> 16) as usize, (jet_delay & 0xFFFF) as i32);
        // Less noise in the breath going up.
        let intensity = 2100 - (whole(settings.at(0)) >> 4);
        let note = (pitch.note as i32).clamp(0, 127) as usize;
        let coefficient = i32::from(FLUTE_BODY_FILTER[note]);
        for (index, out) in out.iter_mut().enumerate() {
            let bore_at = self.at + 2 * FLUTE_BORE_LENGTH - bore_whole;
            let jet_at = self.at + 2 * JET_LENGTH - jet_whole;
            let bore_a = i32::from(self.bore[bore_at % FLUTE_BORE_LENGTH]);
            let bore_b = i32::from(self.bore[(bore_at - 1) % FLUTE_BORE_LENGTH]);
            let jet_a = i32::from(self.jet[jet_at % JET_LENGTH]);
            let jet_b = i32::from(self.jet[(jet_at - 1) % JET_LENGTH]);
            let bore_value = mix16(bore_a, bore_b, bore_fraction) << 9;
            let jet_value = mix16(jet_a, jet_b, jet_fraction) << 9;

            let excitation = self.excitation.min(BLOWING_ENVELOPE.len() - 1);
            let mut breath = i32::from(BLOWING_ENVELOPE[excitation]) << 1;
            let mut random = (i32::from(self.noise.sample()) * intensity) >> 12;
            random = (random * breath) >> 15;
            breath += random;

            self.low_pass =
                (-coefficient * bore_value + (4096 - coefficient) * self.low_pass) >> 12;
            let reflection = self.low_pass;
            let [x0, y0] = self.dc;
            let y0 = ((DC_BLOCKING_POLE * y0) >> 12) + reflection - x0;
            self.dc = [reflection, y0];
            let reflection = y0;

            let pressure = breath - (reflection >> 1);
            self.jet[self.at % JET_LENGTH] = (pressure >> 9) as i8;
            let jet_index = jet_value.clamp(0, 65_535);
            let pressure = i32::from(BLOWING_JET[(jet_index >> 8) as usize]) + (reflection >> 1);
            self.bore[self.at % FLUTE_BORE_LENGTH] = (pressure >> 9) as i8;
            self.at += 1;

            *out = clip16(bore_value >> 1) as f32 / 32_768.0;
            // Braids moves its breath on for three frames in four.
            if (INNER - 1 - index) & 3 != 0 {
                self.excitation += 1;
            }
        }
        self.excitation = self.excitation.min(BLOWING_ENVELOPE.len() - 32);
        self.at %= FLUTE_BORE_LENGTH;
    }
}

/// How a struck partial's level falls each of Braids' blocks of 24 frames, from its long decay at
/// `Decay`'s top to its short one at the bottom, and none at all at the very top.
fn decay(decay: f32, long: u16, short: u16) -> f32 {
    let decay = whole(decay);
    if decay >= 32_000 {
        return 1.0;
    }
    let balance = (32_767 - decay) >> 8;
    let balance = (balance * balance) >> 7;
    let (long, short) = (i32::from(long), i32::from(short));
    let per_block = (long - (((long - short) * balance) >> 7)) as f32 / 65_536.0;
    // This renders blocks of a different size, so its fall is spread over them.
    per_block.powf(INNER as f32 / BRAIDS_BLOCK as f32)
}

const BELL_PARTIALS: usize = 11;
/// Each partial's pitch from the note, in 128ths of a semitone, its level, and how fast it falls.
const BELL_PITCHES: [i16; BELL_PARTIALS] = [
    -1284, -1283, -184, -183, 385, 1175, 1536, 2233, 2434, 2934, 3110,
];
const BELL_LEVELS: [i32; BELL_PARTIALS] = [
    8192, 5488, 8192, 14745, 21872, 13680, 11960, 10895, 10895, 6144, 10895,
];
const BELL_LONG: [u16; BELL_PARTIALS] = [
    65533, 65533, 65533, 65532, 65531, 65531, 65530, 65529, 65527, 65523, 65519,
];
const BELL_SHORT: [u16; BELL_PARTIALS] = [
    65308, 65283, 65186, 65123, 64839, 64889, 64632, 64409, 64038, 63302, 62575,
];

#[derive(Debug, Clone, Copy)]
pub struct BellModel {
    phases: [f32; BELL_PARTIALS],
    levels: [f32; BELL_PARTIALS],
    previous: f32,
}

impl Model for BellModel {
    const LEVEL: f32 = 1.0;

    fn new() -> Self {
        Self {
            // A quarter of a turn in, so each starts at nought.
            phases: [0.25; BELL_PARTIALS],
            levels: BELL_LEVELS.map(|level| level as f32 / 131_072.0),
            previous: 0.0,
        }
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (length, detune) = (settings.at(0), settings.at(1));
        // Pairs of partials pulled apart, up to two semitones each way.
        let spread = (whole(detune) >> 7) as f32 / 128.0;
        let increments: [f32; BELL_PARTIALS] = std::array::from_fn(|at| {
            let offset = f32::from(BELL_PITCHES[at]) / 128.0;
            let offset = if at & 1 == 1 {
                offset + spread
            } else {
                offset - spread
            };
            pitch.increment(offset) * 2.0
        });
        // Each block's fall comes before it plays, as Braids has it.
        for (at, level) in self.levels.iter_mut().enumerate() {
            *level *= decay(length, BELL_LONG[at], BELL_SHORT[at]);
        }
        let (pairs, _) = out.as_chunks_mut::<2>();
        for pair in pairs {
            let mut sample = 0.0;
            for ((phase, increment), level) in
                self.phases.iter_mut().zip(increments).zip(self.levels)
            {
                *phase = (*phase + increment).fract();
                sample += sine(*phase) * level;
            }
            let sample = sample.clamp(-1.0, 1.0);
            *pair = [0.5 * (sample + self.previous), sample];
            self.previous = sample;
        }
    }
}

const DRUM_PARTIALS: usize = 6;
const DRUM_PITCHES: [i16; DRUM_PARTIALS] = [0, 0, 1041, 1747, 1846, 3072];
const DRUM_LEVELS: [i32; DRUM_PARTIALS] = [16986, 2654, 3981, 5308, 3981, 2985];
const DRUM_LONG: [u16; DRUM_PARTIALS] = [65533, 65531, 65531, 65531, 65531, 65516];
const DRUM_SHORT: [u16; DRUM_PARTIALS] = [65083, 64715, 64715, 64715, 64715, 62312];

#[derive(Debug, Clone, Copy)]
pub struct DrumModel {
    phases: [f32; DRUM_PARTIALS],
    levels: [f32; DRUM_PARTIALS],
    struck: bool,
    noise: Lcg,
    low_pass: [f32; 3],
    previous: f32,
}

impl Model for DrumModel {
    const LEVEL: f32 = 0.8;

    fn new() -> Self {
        Self {
            phases: [0.25; DRUM_PARTIALS],
            levels: [0.0; DRUM_PARTIALS],
            struck: false,
            noise: Lcg::new(0),
            low_pass: [0.0; 3],
            previous: 0.0,
        }
    }

    fn seed(&mut self, seed: u32) {
        self.noise = Lcg::new(seed);
    }

    fn render(&mut self, pitch: Pitch, settings: &Settings, out: &mut [f32; INNER]) {
        let (length, tone) = (settings.at(0), settings.at(1));
        // Struck, each partial rises to its level over the first block; then each block falls.
        let targets: [f32; DRUM_PARTIALS] = if self.struck {
            std::array::from_fn(|at| self.levels[at] * decay(length, DRUM_LONG[at], DRUM_SHORT[at]))
        } else {
            self.struck = true;
            DRUM_LEVELS.map(|level| level as f32 / 65_536.0)
        };
        let increments: [f32; DRUM_PARTIALS] =
            std::array::from_fn(|at| pitch.increment(f32::from(DRUM_PITCHES[at]) / 128.0) * 2.0);
        let tone = whole(tone);
        // Braids reads its cutoff table at half the note here, which sets the noise's colour.
        let cutoff = ((pitch.note - 12.0 + (tone >> 2) as f32 / 128.0) / 2.0).clamp(0.0, 128.0);
        let f = pitch.svf_cutoff(cutoff);
        let harmonics_gain = if tone < 12_888 { tone + 4096 } else { 16_384 } as f32 / 16_384.0;
        let noise_gain = ((tone - 16_384).max(0) * 12_888) >> 14;
        let (first_noise, second_noise) = (
            (12_288 - noise_gain) as f32 / 16_384.0,
            noise_gain as f32 / 16_384.0,
        );
        let (pairs, _) = out.as_chunks_mut::<2>();
        let steps = pairs.len() as f32;
        for (step, pair) in pairs.iter_mut().enumerate() {
            let fade = (step + 1) as f32 / steps;
            let noise = (f32::from(self.noise.sample()) / 32_768.0).clamp(-0.5, 0.5);
            let [a, b, c] = &mut self.low_pass;
            *a += (noise - *a) * f;
            *b += (*a - *b) * f;
            *c += (*b - *c) * f;
            let mut partials = [0.0; DRUM_PARTIALS];
            for (at, partial) in partials.iter_mut().enumerate() {
                self.phases[at] = (self.phases[at] + increments[at]).fract();
                let level = self.levels[at] + (targets[at] - self.levels[at]) * fade;
                *partial = sine(self.phases[at]) * level;
            }
            let harmonics: f32 = partials.iter().sum();
            // Two of the partials ring the filtered noise, for the drum's skin.
            let first = partials[1] * *c * 128.0;
            let second = partials[3] * *c * 64.0;
            let sample = (partials[0]
                + first * first_noise
                + second * second_noise
                + harmonics * harmonics_gain)
                .clamp(-1.0, 1.0);
            *pair = [0.5 * (sample + self.previous), sample];
            self.previous = sample;
        }
        self.levels = targets;
    }
}

const BOWING_ENVELOPE: [u16; 752] = [
    0, 10, 21, 32, 43, 54, 65, 76, 87, 98, 109, 120, 131, 142, 153, 164, 175, 185, 196, 207, 218,
    229, 240, 251, 262, 273, 284, 295, 306, 317, 328, 339, 350, 361, 371, 382, 393, 404, 415, 426,
    437, 448, 459, 470, 481, 492, 503, 514, 525, 536, 547, 557, 568, 579, 590, 601, 612, 623, 634,
    645, 656, 667, 678, 689, 700, 711, 722, 733, 743, 754, 765, 776, 787, 798, 809, 820, 831, 842,
    853, 864, 875, 886, 897, 908, 919, 929, 940, 951, 962, 973, 984, 995, 1006, 1017, 1028, 1039,
    1050, 1061, 1072, 1083, 1094, 1105, 1115, 1126, 1137, 1148, 1159, 1170, 1181, 1192, 1203, 1214,
    1225, 1236, 1247, 1258, 1269, 1280, 1291, 1301, 1312, 1323, 1334, 1345, 1356, 1367, 1378, 1389,
    1400, 1411, 1422, 1433, 1444, 1455, 1466, 1477, 1487, 1498, 1509, 1520, 1531, 1542, 1553, 1564,
    1575, 1586, 1597, 1608, 1619, 1630, 1641, 1652, 1663, 1673, 1684, 1695, 1706, 1717, 1728, 1739,
    1750, 1761, 1772, 1783, 1794, 1805, 1816, 1827, 1838, 1849, 1859, 1870, 1881, 1892, 1903, 1914,
    1925, 1936, 1947, 1958, 1969, 1980, 1991, 2002, 2013, 2024, 2035, 2045, 2056, 2067, 2078, 2089,
    2100, 2111, 2122, 2133, 2144, 2155, 2166, 2177, 2188, 2199, 2210, 2221, 2231, 2242, 2253, 2264,
    2275, 2286, 2297, 2308, 2319, 2330, 2341, 2352, 2363, 2374, 2385, 2396, 2406, 2417, 2428, 2439,
    2450, 2461, 2472, 2483, 2494, 2505, 2516, 2527, 2538, 2549, 2560, 2571, 2582, 2592, 2603, 2614,
    2625, 2636, 2647, 2658, 2669, 2680, 2691, 2702, 2713, 2724, 2735, 2746, 2757, 2768, 2778, 2789,
    2800, 2811, 2822, 2833, 2844, 2855, 2866, 2877, 2888, 2899, 2910, 2921, 2932, 2943, 2954, 2964,
    2975, 2986, 2997, 3008, 3019, 3030, 3041, 3052, 3063, 3074, 3085, 3096, 3107, 3118, 3129, 3140,
    3150, 3161, 3172, 3183, 3194, 3205, 3216, 3227, 3238, 3249, 3260, 3271, 3282, 3293, 3304, 3315,
    3326, 3336, 3347, 3358, 3369, 3380, 3391, 3402, 3413, 3424, 3435, 3446, 3457, 3468, 3479, 3490,
    3501, 3512, 3522, 3533, 3544, 3555, 3566, 3577, 3588, 3599, 3610, 3621, 3632, 3643, 3654, 3665,
    3676, 3687, 3698, 3708, 3719, 3730, 3741, 3752, 3763, 3774, 3785, 3796, 3807, 3818, 3829, 3840,
    3851, 3862, 3873, 3884, 3894, 3905, 3916, 3927, 3938, 3949, 3960, 3971, 3982, 3993, 4004, 4015,
    4026, 4037, 4048, 4059, 4070, 4080, 4091, 4102, 4113, 4124, 4135, 4146, 4157, 4168, 4179, 4190,
    4201, 4212, 4223, 4234, 4245, 4256, 4266, 4277, 4288, 4299, 4310, 4321, 4332, 4343, 4354, 4365,
    4376, 4387, 4398, 4409, 4420, 4431, 4442, 4452, 4463, 4474, 4485, 4496, 4507, 4518, 4529, 4540,
    4551, 4562, 4573, 4584, 4595, 4606, 4617, 4628, 4638, 4649, 4660, 4671, 4682, 4693, 4704, 4715,
    4726, 4737, 4748, 4759, 4770, 4781, 4792, 4803, 4813, 4824, 4835, 4846, 4857, 4868, 4879, 4890,
    4901, 4912, 4923, 4934, 4945, 4956, 4967, 4978, 4989, 4999, 5010, 5021, 5032, 5043, 5054, 5065,
    5076, 5087, 5098, 5109, 5120, 5131, 5142, 5153, 5164, 5175, 5185, 5196, 5207, 5218, 5229, 5240,
    5251, 5262, 5273, 5284, 5295, 5306, 5317, 5328, 5339, 5350, 5361, 5371, 5382, 5393, 5404, 5415,
    5426, 5437, 5448, 5459, 5470, 5481, 5492, 5503, 5514, 5525, 5536, 5547, 5557, 5568, 5579, 5590,
    5601, 5612, 5623, 5634, 5645, 5656, 5667, 5678, 5689, 5700, 5711, 5722, 5733, 5743, 5754, 5765,
    5776, 5787, 5798, 5809, 5820, 5831, 5842, 5853, 5864, 5875, 5886, 5897, 5908, 5919, 5929, 5940,
    5951, 5962, 5973, 5984, 5995, 6006, 6017, 6028, 6039, 6050, 6061, 6072, 6083, 6094, 6105, 6115,
    6126, 6137, 6148, 6159, 6170, 6181, 6192, 6203, 6214, 6225, 6236, 6247, 6258, 6269, 6280, 6291,
    6301, 6312, 6323, 6334, 6345, 6356, 6367, 6378, 6389, 6400, 6411, 6422, 6433, 6444, 6455, 6466,
    6477, 6487, 6498, 6509, 6520, 6531, 6542, 6553, 6553, 6542, 6531, 6520, 6509, 6498, 6487, 6476,
    6465, 6454, 6443, 6432, 6421, 6410, 6399, 6388, 6377, 6366, 6355, 6344, 6333, 6322, 6311, 6300,
    6289, 6278, 6267, 6256, 6245, 6234, 6223, 6212, 6201, 6190, 6179, 6168, 6157, 6146, 6135, 6124,
    6113, 6102, 6090, 6079, 6068, 6057, 6046, 6035, 6024, 6013, 6002, 5991, 5980, 5969, 5958, 5947,
    5936, 5925, 5914, 5903, 5892, 5881, 5870, 5859, 5848, 5837, 5826, 5815, 5804, 5793, 5782, 5771,
    5760, 5749, 5738, 5727, 5716, 5705, 5694, 5683, 5672, 5661, 5650, 5639, 5628, 5617, 5606, 5595,
    5584, 5573, 5562, 5551, 5540, 5529, 5518, 5507, 5496, 5485, 5474, 5463, 5452, 5441, 5430, 5419,
    5408, 5397, 5386, 5375, 5364, 5353, 5342, 5330, 5319, 5308, 5297, 5286, 5275, 5264, 5253, 5242,
    5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242,
    5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242, 5242,
];
const BOWING_FRICTION: [u16; 257] = [
    32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768, 32768,
    32768, 32768, 32768, 32768, 30797, 28973, 27281, 25711, 24253, 22896, 21633, 20456, 19358,
    18333, 17374, 16478, 15638, 14852, 14114, 13421, 12771, 12159, 11583, 11042, 10531, 10050,
    9596, 9167, 8762, 8379, 8016, 7673, 7349, 7041, 6749, 6472, 6209, 5960, 5723, 5497, 5283, 5078,
    4884, 4699, 4522, 4354, 4194, 4040, 3894, 3754, 3621, 3493, 3371, 3254, 3143, 3036, 2933, 2835,
    2741, 2651, 2564, 2481, 2401, 2325, 2251, 2181, 2113, 2048, 1985, 1924, 1866, 1810, 1756, 1705,
    1655, 1606, 1560, 1515, 1472, 1431, 1390, 1352, 1314, 1278, 1243, 1209, 1177, 1145, 1115, 1085,
    1057, 1029, 1003, 977, 952, 928, 904, 882, 860, 838, 818, 798, 778, 759, 741, 723, 706, 690,
    673, 658, 642, 628, 613, 599, 586, 572, 560, 547, 535, 523, 512, 501, 490, 479, 469, 459, 449,
    440, 430, 421, 413, 404, 396, 388, 380, 372, 365, 357, 350, 343, 336, 330, 323, 317, 311, 305,
    299, 293, 288, 282, 277, 272, 267, 262, 257, 252, 247, 243, 238, 234, 230, 226, 222, 218, 214,
    210, 207, 203, 199, 196, 193, 189, 186, 183, 180, 177, 174, 171, 168, 165, 162, 160, 157, 155,
    152, 150, 147, 145, 143, 140, 138, 136, 134, 132, 130, 128, 126, 124, 122, 120, 118, 116, 114,
    113, 111, 109, 108, 106, 104, 103, 101, 100, 98, 97, 96, 94, 93, 92, 90, 89, 88, 86, 85, 84,
    83, 82, 81, 79, 78, 77, 76, 75, 74, 73, 72, 71, 70, 69, 68, 67, 66, 66, 65, 64,
];
const BLOWING_ENVELOPE: [u16; 392] = [
    0, 178, 357, 536, 715, 894, 1073, 1252, 1431, 1610, 1789, 1968, 2147, 2326, 2505, 2684, 2863,
    3042, 3221, 3400, 3579, 3758, 3937, 4116, 4295, 4474, 4653, 4832, 5011, 5190, 5369, 5548, 5727,
    5906, 6085, 6264, 6443, 6622, 6801, 6980, 7159, 7338, 7517, 7696, 7875, 8054, 8233, 8412, 8591,
    8770, 8949, 9128, 9307, 9486, 9665, 9844, 10023, 10202, 10381, 10560, 10739, 10918, 11097,
    11276, 11455, 11634, 11813, 11991, 12170, 12349, 12528, 12707, 12886, 13065, 13244, 13423,
    13602, 13781, 13960, 14139, 14318, 14497, 14676, 14855, 15034, 15213, 15392, 15571, 15750,
    15929, 16108, 16287, 16466, 16645, 16824, 17003, 17182, 17361, 17540, 17719, 17898, 18077,
    18256, 18435, 18614, 18793, 18972, 19151, 19330, 19509, 19688, 19867, 20046, 20225, 20404,
    20583, 20762, 20941, 21120, 21299, 21299, 21281, 21263, 21245, 21227, 21210, 21192, 21174,
    21156, 21138, 21120, 21103, 21085, 21067, 21049, 21031, 21014, 20996, 20978, 20960, 20942,
    20924, 20907, 20889, 20871, 20853, 20835, 20817, 20800, 20782, 20764, 20746, 20728, 20711,
    20693, 20675, 20657, 20639, 20621, 20604, 20586, 20568, 20550, 20532, 20514, 20497, 20479,
    20461, 20443, 20425, 20408, 20390, 20372, 20354, 20336, 20318, 20301, 20283, 20265, 20247,
    20229, 20211, 20194, 20176, 20158, 20140, 20122, 20105, 20087, 20069, 20051, 20033, 20015,
    19998, 19980, 19962, 19944, 19926, 19908, 19891, 19873, 19855, 19837, 19819, 19802, 19784,
    19766, 19748, 19730, 19712, 19695, 19677, 19659, 19641, 19623, 19605, 19588, 19570, 19552,
    19534, 19516, 19499, 19481, 19463, 19445, 19427, 19409, 19392, 19374, 19356, 19338, 19320,
    19302, 19285, 19267, 19249, 19231, 19213, 19196, 19178, 19160, 19142, 19124, 19106, 19089,
    19071, 19053, 19035, 19017, 18999, 18982, 18964, 18946, 18928, 18910, 18893, 18875, 18857,
    18839, 18821, 18803, 18786, 18768, 18750, 18732, 18714, 18696, 18679, 18661, 18643, 18625,
    18607, 18590, 18572, 18554, 18536, 18518, 18500, 18483, 18465, 18447, 18429, 18411, 18393,
    18376, 18358, 18340, 18322, 18304, 18287, 18269, 18251, 18233, 18215, 18197, 18180, 18162,
    18144, 18126, 18108, 18090, 18073, 18055, 18037, 18019, 18001, 17984, 17966, 17948, 17930,
    17912, 17894, 17877, 17859, 17841, 17823, 17805, 17787, 17770, 17752, 17734, 17716, 17698,
    17681, 17663, 17645, 17627, 17609, 17591, 17574, 17556, 17538, 17520, 17502, 17484, 17467,
    17449, 17431, 17413, 17395, 17378, 17360, 17342, 17324, 17306, 17288, 17271, 17253, 17235,
    17217, 17199, 17181, 17164, 17146, 17128, 17110, 17092, 17075, 17057, 17039, 17039, 17039,
    17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039,
    17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039, 17039,
    17039, 17039, 17039, 17039,
];
const BLOWING_JET: [i16; 257] = [
    0, -255, -511, -767, -1022, -1278, -1532, -1786, -2039, -2292, -2544, -2795, -3044, -3293,
    -3541, -3787, -4031, -4275, -4516, -4756, -4994, -5231, -5465, -5697, -5927, -6155, -6381,
    -6604, -6824, -7042, -7257, -7470, -7679, -7886, -8089, -8289, -8486, -8680, -8870, -9056,
    -9239, -9418, -9594, -9765, -9932, -10095, -10254, -10409, -10559, -10705, -10846, -10982,
    -11114, -11241, -11363, -11480, -11591, -11698, -11799, -11894, -11984, -12069, -12147, -12220,
    -12287, -12348, -12403, -12452, -12494, -12530, -12560, -12583, -12599, -12609, -12611, -12607,
    -12596, -12578, -12552, -12519, -12479, -12431, -12376, -12313, -12242, -12163, -12077, -11982,
    -11879, -11768, -11649, -11521, -11384, -11239, -11085, -10923, -10751, -10571, -10381, -10182,
    -9974, -9757, -9530, -9293, -9047, -8791, -8526, -8250, -7964, -7668, -7362, -7046, -6719,
    -6382, -6034, -5676, -5306, -4926, -4535, -4133, -3719, -3295, -2859, -2411, -1952, -1482,
    -1000, -506, 0, 517, 1048, 1590, 2144, 2711, 3291, 3883, 4487, 5105, 5735, 6378, 7034, 7704,
    8386, 9082, 9791, 10514, 11250, 12000, 12764, 13542, 14333, 15139, 15959, 16793, 17642, 18504,
    19382, 20274, 21181, 22102, 23039, 23990, 24957, 25939, 26936, 27948, 28976, 30019, 31079,
    32153, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
    32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767, 32767,
];
const FLUTE_BODY_FILTER: [u16; 128] = [
    30, 32, 34, 36, 38, 40, 43, 45, 48, 51, 54, 57, 60, 64, 68, 72, 76, 81, 86, 91, 96, 102, 108,
    114, 121, 129, 136, 144, 153, 162, 172, 182, 193, 204, 216, 229, 243, 258, 273, 289, 306, 325,
    344, 364, 386, 409, 433, 459, 487, 516, 546, 579, 613, 650, 688, 729, 773, 819, 867, 919, 974,
    1032, 1093, 1158, 1227, 1300, 1377, 1459, 1546, 1638, 1735, 1839, 1948, 2064, 2187, 2317, 2454,
    2600, 2755, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867,
    2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867,
    2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867, 2867,
    2867, 2867, 2867,
];

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
        check("Bowed", &|hertz, values, frames| {
            run::<BowedModel>(hertz, values, frames)
        });
        check("Blown", &|hertz, values, frames| {
            run::<BlownModel>(hertz, values, frames)
        });
        check("Fluted", &|hertz, values, frames| {
            run::<FlutedModel>(hertz, values, frames)
        });
        check("Bell", &|hertz, values, frames| {
            run::<BellModel>(hertz, values, frames)
        });
        check("Drum", &|hertz, values, frames| {
            run::<DrumModel>(hertz, values, frames)
        });
    }

    #[test]
    fn each_is_about_as_loud_as_the_virtual_analog_voice_as_it_starts() {
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..12_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        each(|name, run| {
            // With little noise in the breath, where the blown ones speak most readily.
            for values in [[0.75, 0.25], [0.9, 0.4]] {
                let out = run(220.0, values, 12_000);
                let decibels = 20.0 * (rms(&out) / rms(&va)).log10();
                assert!(decibels.abs() < 6.0, "{name}: {decibels} dB at {values:?}");
            }
        });
    }

    #[test]
    fn each_setting_changes_the_sound() {
        each(|name, run| {
            let base = run(220.0, [0.3, 0.3], 9_600);
            for values in [[0.9, 0.3], [0.3, 0.9]] {
                let other = run(220.0, values, 9_600);
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
                        for sample in run(hertz, [first, second], 4_800) {
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
    fn the_bowed_and_blown_ones_keep_sounding() {
        for (name, out) in [
            ("Bowed", run::<BowedModel>(220.0, [0.5, 0.5], 96_000)),
            ("Blown", run::<BlownModel>(220.0, [0.5, 0.5], 96_000)),
            ("Fluted", run::<FlutedModel>(220.0, [0.5, 0.5], 96_000)),
        ] {
            let (early, late) = (rms(&out[9_600..19_200]), rms(&out[86_400..]));
            assert!(late > early * 0.3, "{name}: {late} against {early}");
        }
    }

    #[test]
    fn the_struck_ones_die_away_unless_decay_is_at_its_top() {
        for (name, short, long) in [
            (
                "Bell",
                run::<BellModel>(220.0, [0.3, 0.5], 96_000),
                run::<BellModel>(220.0, [1.0, 0.5], 96_000),
            ),
            (
                "Drum",
                run::<DrumModel>(220.0, [0.3, 0.5], 96_000),
                run::<DrumModel>(220.0, [1.0, 0.5], 96_000),
            ),
        ] {
            let start = rms(&short[..4_800]);
            assert!(rms(&short[86_400..]) < start * 0.05, "{name} rings on");
            assert!(rms(&long[86_400..]) > start * 0.5, "{name} at the top dies");
        }
    }

    #[test]
    fn a_seed_changes_the_breath_and_a_render_repeats() {
        let with = |seed| {
            let mut voice = Macro::<BlownModel>::new(RATE);
            voice.seed(seed);
            (0..4_800)
                .map(|_| voice.next(220.0 / RATE as f32, settings([0.5, 0.5])))
                .collect::<Vec<f32>>()
        };
        assert_eq!(with(1), with(1));
        assert_ne!(with(1), with(2));
    }
}
