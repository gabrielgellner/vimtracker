//! Six-operator FM playing DX7 patches, from Mutable Instruments' Plaits. Ported to Rust from
//! `six_op_engine.cc` and `dsp/fm`: `patch.h`, `dx_units.h`, `envelope.h`, `lfo.h`,
//! `operator.h`, `algorithms.cc` and `voice.h`. The three banks of 32 patches are its
//! `syx_bank_0` to `syx_bank_2`, taken from the generated `resources.cc` into
//! `tables/plaits_fm_patches.bin`, each patch in the DX7's packed 128-byte form.
//!
//! It plays one note at a time. Plaits keeps a second voice so a release can ring under the next
//! note, which a track's own envelope does here instead. Its level is the main output's, soft
//! clipped as Plaits does.
//!
//! Copyright 2021 Emilie Gillet.
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

// The loops index by operator and stage, as Plaits does, so the port reads against it line by line.
#![allow(clippy::needless_range_loop)]

use super::sine_table::SINE;
use super::virtual_analog::Settings;

/// How many frames are worked out at once, as in Plaits, whose envelopes move once a block.
const BLOCK: usize = 24;
const OPERATORS: usize = 6;
const PATCH_SIZE: usize = 128;
/// Every patch, in bank order.
pub const PATCHES: usize = vt_model::song::SIX_OP_PATCHES as usize;
/// How loud it is against the other machines.
const LEVEL: f32 = 0.83;

static BANKS: &[u8; PATCHES * PATCH_SIZE] = include_bytes!("tables/plaits_fm_patches.bin");

/// The patch a `Harm` on `0..=1` chooses. Its value is the patch's number, so a step's `HAR20`
/// plays patch `20`, and anything past the last patch plays that.
pub fn patch_index(harmonics: f32) -> usize {
    ((harmonics.max(0.0) * 255.0).round() as usize).min(PATCHES - 1)
}

/// A patch's name, as the DX7 stores it, without its trailing spaces.
pub fn patch_name(index: usize) -> String {
    let at = index.min(PATCHES - 1) * PATCH_SIZE + 118;
    BANKS[at..at + 10]
        .iter()
        .map(|byte| match byte & 0x7f {
            b @ 0x20..=0x7e => b as char,
            _ => ' ',
        })
        .collect::<String>()
        .trim_end()
        .to_owned()
}

// Patches.

#[derive(Debug, Clone, Copy, Default)]
struct KeyboardScaling {
    break_point: u8,
    left_depth: u8,
    right_depth: u8,
    left_curve: u8,
    right_curve: u8,
}

#[derive(Debug, Clone, Copy, Default)]
struct OperatorPatch {
    rate: [u8; 4],
    level: [u8; 4],
    scaling: KeyboardScaling,
    rate_scaling: u8,
    amp_mod_sensitivity: u8,
    velocity_sensitivity: u8,
    level_total: u8,
    /// 0 follows the note, 1 is a fixed frequency.
    mode: u8,
    coarse: u8,
    fine: u8,
    detune: u8,
}

#[derive(Debug, Clone, Copy, Default)]
struct Modulations {
    rate: u8,
    delay: u8,
    pitch_mod_depth: u8,
    amp_mod_depth: u8,
    reset_phase: bool,
    waveform: u8,
    pitch_mod_sensitivity: u8,
}

/// A patch, with operator 6 first, as the DX7 stores them.
#[derive(Debug, Clone, Copy, Default)]
struct Patch {
    op: [OperatorPatch; OPERATORS],
    pitch_rate: [u8; 4],
    pitch_level: [u8; 4],
    algorithm: u8,
    feedback: u8,
    reset_phase: bool,
    modulations: Modulations,
}

impl Patch {
    fn unpack(data: &[u8]) -> Self {
        let mut patch = Self::default();
        for (i, o) in patch.op.iter_mut().enumerate() {
            let d = &data[i * 17..i * 17 + 17];
            for j in 0..4 {
                o.rate[j] = (d[j] & 0x7f).min(99);
                o.level[j] = (d[4 + j] & 0x7f).min(99);
            }
            o.scaling = KeyboardScaling {
                break_point: (d[8] & 0x7f).min(99),
                left_depth: (d[9] & 0x7f).min(99),
                right_depth: (d[10] & 0x7f).min(99),
                left_curve: d[11] & 0x3,
                right_curve: (d[11] >> 2) & 0x3,
            };
            o.rate_scaling = d[12] & 0x7;
            o.amp_mod_sensitivity = d[13] & 0x3;
            o.velocity_sensitivity = (d[13] >> 2) & 0x7;
            o.level_total = (d[14] & 0x7f).min(99);
            o.mode = d[15] & 0x1;
            o.coarse = (d[15] >> 1) & 0x1f;
            o.fine = (d[16] & 0x7f).min(99);
            o.detune = ((d[12] >> 3) & 0xf).min(14);
        }
        for j in 0..4 {
            patch.pitch_rate[j] = (data[102 + j] & 0x7f).min(99);
            patch.pitch_level[j] = (data[106 + j] & 0x7f).min(99);
        }
        patch.algorithm = data[110] & 0x1f;
        patch.feedback = data[111] & 0x7;
        patch.reset_phase = (data[111] >> 3) & 0x1 != 0;
        patch.modulations = Modulations {
            rate: (data[112] & 0x7f).min(99),
            delay: (data[113] & 0x7f).min(99),
            pitch_mod_depth: (data[114] & 0x7f).min(99),
            amp_mod_depth: (data[115] & 0x7f).min(99),
            reset_phase: data[116] & 0x1 != 0,
            waveform: ((data[116] >> 1) & 0x7).min(5),
            pitch_mod_sensitivity: data[116] >> 4,
        };
        patch
    }

    fn load(index: usize) -> Self {
        let at = index.min(PATCHES - 1) * PATCH_SIZE;
        Self::unpack(&BANKS[at..at + PATCH_SIZE])
    }
}

// The DX7's units, from `dx_units.h`.

#[allow(clippy::excessive_precision)]
const COARSE: [f32; 32] = [
    -12.000000, 0.000000, 12.000000, 19.019550, 24.000000, 27.863137, 31.019550, 33.688259,
    36.000000, 38.039100, 39.863137, 41.513180, 43.019550, 44.405276, 45.688259, 46.882687,
    48.000000, 49.049554, 50.039100, 50.975130, 51.863137, 52.707809, 53.513180, 54.282743,
    55.019550, 55.726274, 56.405276, 57.058650, 57.688259, 58.295772, 58.882687, 59.450356,
];
const AMP_MOD_SENSITIVITY: [f32; 4] = [0.0, 0.2588, 0.4274, 1.0];
const PITCH_MOD_SENSITIVITY: [f32; 8] = [
    0.0, 0.078125, 0.15625, 0.2578125, 0.4296875, 0.71875, 1.1953125, 2.0,
];
#[allow(clippy::excessive_precision)]
const CUBE_ROOT: [f32; 17] = [
    0.0,
    0.39685062976,
    0.5,
    0.57235744065,
    0.62996081605,
    0.67860466725,
    0.72112502092,
    0.75914745216,
    0.79370070937,
    0.82548197054,
    0.85498810729,
    0.88258719406,
    0.90856038354,
    0.93312785379,
    0.95646563396,
    0.97871693135,
    1.0,
];
const MIN_LFO_FREQUENCY: f32 = 0.005865;

fn operator_level(level: u8) -> i32 {
    let tlc = i32::from(level);
    if tlc < 20 {
        if tlc < 15 {
            (tlc * (36 - tlc)) >> 3
        } else {
            27 + tlc
        }
    } else {
        tlc + 28
    }
}

fn pitch_envelope_level(level: u8) -> f32 {
    let l = (f32::from(level) - 50.0) / 32.0;
    let tail = ((l + 0.02).abs() - 1.0).max(0.0);
    l * (1.0 + tail * tail * 5.3056)
}

fn operator_envelope_increment(rate: u8) -> f32 {
    let scaled = (i32::from(rate) * 41) >> 6;
    let mantissa = 4 + (scaled & 3);
    let exponent = 2 + (scaled >> 2);
    (mantissa << exponent) as f32 / (1 << 24) as f32
}

fn pitch_envelope_increment(rate: u8) -> f32 {
    let r = f32::from(rate) * 0.01;
    (1.0 + 192.0 * r * (r * r * r * r + 0.3333)) / (21.3 * 44_100.0)
}

fn lfo_frequency(rate: u8) -> f32 {
    let mut scaled = if rate == 0 {
        1
    } else {
        (i32::from(rate) * 165) >> 6
    };
    scaled *= if scaled < 160 {
        11
    } else {
        11 + ((scaled - 160) >> 4)
    };
    scaled as f32 * MIN_LFO_FREQUENCY
}

fn lfo_delay(delay: u8) -> [f32; 2] {
    if delay == 0 {
        [100_000.0; 2]
    } else {
        let d = 99 - i32::from(delay);
        let d = (16 + (d & 15)) << (1 + (d >> 4));
        [
            d as f32 * MIN_LFO_FREQUENCY,
            (d & 0xff80).max(0x80) as f32 * MIN_LFO_FREQUENCY,
        ]
    }
}

fn normalize_velocity(velocity: f32) -> f32 {
    let at = velocity.clamp(0.0, 1.0) * 16.0;
    let whole = (at as usize).min(15);
    let fraction = at - whole as f32;
    let cube_root = CUBE_ROOT[whole] + (CUBE_ROOT[whole + 1] - CUBE_ROOT[whole]) * fraction;
    16.0 * (cube_root - 0.918)
}

fn rate_scaling(note: f32, rate_scaling: u8) -> f32 {
    (f32::from(rate_scaling) * (note * 0.33333 - 7.0) * 0.03125).exp2()
}

fn keyboard_scaling(note: f32, ks: KeyboardScaling) -> f32 {
    let x = note - f32::from(ks.break_point) - 15.0;
    let curve = if x > 0.0 {
        ks.right_curve
    } else {
        ks.left_curve
    };
    let mut t = x.abs();
    if curve == 1 || curve == 2 {
        t = (t * 0.010467).min(1.0);
        t = t * t * t * 96.0;
    }
    if curve < 2 {
        t = -t;
    }
    let depth = f32::from(if x > 0.0 {
        ks.right_depth
    } else {
        ks.left_depth
    });
    t * depth * 0.02677
}

fn frequency_ratio(op: &OperatorPatch) -> f32 {
    let detune = if op.mode == 0 && op.fine != 0 {
        1.0 + 0.01 * f32::from(op.fine)
    } else {
        1.0
    };
    let mut base = if op.mode == 0 {
        COARSE[usize::from(op.coarse)]
    } else {
        f32::from(u16::from(op.coarse & 3) * 100 + u16::from(op.fine)) * 0.39864
    };
    base += (f32::from(op.detune) - 7.0) * 0.015;
    (base / 12.0).exp2() * detune
}

// Envelopes, from `envelope.h`.

/// A level that means "where the stage before ended".
const PREVIOUS_LEVEL: f32 = -100.0;

/// A four-stage envelope: three while the note is held, and its release.
#[derive(Debug, Clone, Copy)]
struct Envelope {
    stage: usize,
    phase: f32,
    start: f32,
    increment: [f32; 4],
    level: [f32; 4],
    scale: f32,
    /// Whether rising stages curve, as an operator's do.
    reshape: bool,
}

impl Envelope {
    fn new(scale: f32, reshape: bool) -> Self {
        Self {
            stage: 3,
            phase: 1.0,
            start: 0.0,
            increment: [0.001; 4],
            level: [1.0, 0.5, 0.25, 0.0],
            scale,
            reshape,
        }
    }

    fn set_operator(&mut self, rate: [u8; 4], level: [u8; 4], global_level: i32) {
        for i in 0..4 {
            let scaled = (operator_level(level[i]) & !1) + global_level - 133;
            self.level[i] = 0.125 * if scaled < 1 { 0.5 } else { scaled as f32 };
        }
        for i in 0..4 {
            let mut increment = operator_envelope_increment(rate[i]);
            let mut from = self.level[(i + 3) % 4];
            let mut to = self.level[i];
            if from == to {
                // A plateau moves at a scaled rate, and the attack's is faster still.
                increment *= 0.6;
                if i == 0 && level[i] == 0 {
                    increment *= 20.0;
                }
            } else if from < to {
                from = from.max(6.7);
                to = to.max(6.7);
                if from == to {
                    // The jump to the curve's floor can leave no attack at all.
                    increment = 1.0;
                } else {
                    increment *= 7.2 / (to - from);
                }
            } else {
                increment *= 1.0 / (from - to);
            }
            self.increment[i] = increment * self.scale;
        }
    }

    fn set_pitch(&mut self, rate: [u8; 4], level: [u8; 4]) {
        for i in 0..4 {
            self.level[i] = pitch_envelope_level(level[i]);
        }
        for i in 0..4 {
            let from = self.level[(i + 3) % 4];
            let to = self.level[i];
            let mut increment = pitch_envelope_increment(rate[i]);
            if from != to {
                increment *= 1.0 / (from - to).abs();
            } else if i != 3 {
                increment = 0.2;
            }
            self.increment[i] = increment * self.scale;
        }
    }

    fn render(&mut self, gate: bool, rate: f32, ad_scale: f32, release_scale: f32) -> f32 {
        if gate {
            if self.stage == 3 {
                self.start = self.value();
                self.stage = 0;
                self.phase = 0.0;
            }
        } else if self.stage != 3 {
            self.start = self.value();
            self.stage = 3;
            self.phase = 0.0;
        }
        let scale = if self.stage == 3 {
            release_scale
        } else {
            ad_scale
        };
        self.phase += self.increment[self.stage] * rate * scale;
        if self.phase >= 1.0 {
            if self.stage >= 2 {
                self.phase = 1.0;
            } else {
                self.phase = 0.0;
                self.stage += 1;
            }
            self.start = PREVIOUS_LEVEL;
        }
        self.value()
    }

    fn value(&self) -> f32 {
        let mut from = if self.start == PREVIOUS_LEVEL {
            self.level[(self.stage + 3) % 4]
        } else {
            self.start
        };
        let mut to = self.level[self.stage];
        let mut phase = self.phase;
        if self.reshape && from < to {
            from = from.max(6.7);
            to = to.max(6.7);
            phase *= (2.5 - phase) * 0.666_667;
        }
        phase * (to - from) + from
    }
}

// The patch's LFO, from `lfo.h`.

#[derive(Debug, Clone, Copy)]
struct Lfo {
    phase: f32,
    frequency: f32,
    delay_phase: f32,
    delay_increment: [f32; 2],
    value: f32,
    random_value: f32,
    random: u32,
    amp_mod_depth: f32,
    pitch_mod_depth: f32,
    waveform: u8,
    reset_phase: bool,
}

impl Lfo {
    fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 0.1,
            delay_phase: 0.0,
            delay_increment: [0.1; 2],
            value: 0.0,
            random_value: 0.0,
            random: 0x2545_f491,
            amp_mod_depth: 0.0,
            pitch_mod_depth: 0.0,
            waveform: 0,
            reset_phase: false,
        }
    }

    fn set(&mut self, modulations: Modulations, one_hz: f32) {
        self.frequency = lfo_frequency(modulations.rate) * one_hz;
        self.delay_increment = lfo_delay(modulations.delay).map(|increment| increment * one_hz);
        self.waveform = modulations.waveform;
        self.reset_phase = modulations.reset_phase;
        self.amp_mod_depth = f32::from(modulations.amp_mod_depth) * 0.01;
        self.pitch_mod_depth = f32::from(modulations.pitch_mod_depth)
            * 0.01
            * PITCH_MOD_SENSITIVITY[usize::from(modulations.pitch_mod_sensitivity & 7)];
    }

    fn reset(&mut self) {
        if self.reset_phase {
            self.phase = 0.0;
        }
        self.delay_phase = 0.0;
    }

    fn step(&mut self, scale: f32) {
        self.phase += scale * self.frequency;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.random ^= self.random << 13;
            self.random ^= self.random >> 17;
            self.random ^= self.random << 5;
            self.random_value = self.random as f32 / 4_294_967_296.0;
        }
        self.value = self.wave();
        let increment = self.delay_increment[usize::from(self.delay_phase >= 0.5)];
        self.delay_phase = (self.delay_phase + scale * increment).min(1.0);
    }

    fn wave(&self) -> f32 {
        let phase = self.phase;
        match self.waveform {
            0 => {
                2.0 * if phase < 0.5 {
                    0.5 - phase
                } else {
                    phase - 0.5
                }
            }
            1 => 1.0 - phase,
            2 => phase,
            3 => {
                if phase < 0.5 {
                    0.0
                } else {
                    1.0
                }
            }
            4 => 0.5 + 0.5 * (std::f32::consts::TAU * (phase + 0.5)).sin(),
            _ => self.random_value,
        }
    }

    fn delay_ramp(&self) -> f32 {
        if self.delay_phase < 0.5 {
            0.0
        } else {
            (self.delay_phase - 0.5) * 2.0
        }
    }

    fn pitch_mod(&self) -> f32 {
        (self.value - 0.5) * self.delay_ramp() * self.pitch_mod_depth
    }

    fn amp_mod(&self) -> f32 {
        (1.0 - self.value) * self.delay_ramp() * self.amp_mod_depth
    }
}

// Algorithms, from `algorithms.cc`. Each operator's opcode says where its modulation comes from
// and which buffer it writes to, or adds to.

const DESTINATION_MASK: u8 = 0x03;
const SOURCE_MASK: u8 = 0x30;
const SOURCE_FEEDBACK: u8 = 0x30;
const ADDITIVE: u8 = 0x04;
const FEEDBACK_SOURCE: u8 = 0x40;

const fn modulation(n: u8) -> u8 {
    n << 4
}
const fn add(n: u8) -> u8 {
    n | ADDITIVE
}
const fn out(n: u8) -> u8 {
    n
}
const FB_SRC: u8 = FEEDBACK_SOURCE;
const FB_DST: u8 = modulation(3);
const FB: u8 = FB_SRC | FB_DST;
const NO_MOD: u8 = modulation(0);
const OUTPUT: u8 = add(0);

/// The DX7's 32 algorithms, operator 6 first.
#[rustfmt::skip]
const OPCODES: [[u8; OPERATORS]; 32] = [
    [FB | out(1), modulation(1) | out(1), modulation(1) | out(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), modulation(1) | out(1), modulation(1) | out(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | out(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | out(1), modulation(1) | add(0)],
    [FB_DST | NO_MOD | out(1), modulation(1) | out(1), FB_SRC | modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0), NO_MOD | out(1), modulation(1) | add(0)],
    [FB_DST | NO_MOD | out(1), FB_SRC | modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0), NO_MOD | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), modulation(1) | out(1), FB | add(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), modulation(1) | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | out(1), modulation(1) | add(0)],
    [FB | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), NO_MOD | add(1), NO_MOD | add(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0)],
    [FB | out(1), NO_MOD | add(1), NO_MOD | add(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0)],
    [FB | out(1), NO_MOD | add(1), modulation(1) | out(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), NO_MOD | add(1), modulation(1) | out(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | out(1), NO_MOD | out(2), modulation(2) | add(1), NO_MOD | add(1), modulation(1) | OUTPUT],
    [NO_MOD | out(1), modulation(1) | out(1), NO_MOD | out(2), modulation(2) | add(1), FB | add(1), modulation(1) | OUTPUT],
    [NO_MOD | out(1), modulation(1) | out(1), modulation(1) | out(1), FB | add(1), NO_MOD | add(1), modulation(1) | OUTPUT],
    [FB | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), NO_MOD | out(1), modulation(1) | out(1), modulation(1) | add(0)],
    [NO_MOD | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0), modulation(1) | add(0)],
    [NO_MOD | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), FB | out(1), modulation(1) | add(0), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), modulation(1) | add(0), NO_MOD | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), NO_MOD | out(1), modulation(1) | add(0), NO_MOD | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), modulation(1) | add(0), NO_MOD | add(0), NO_MOD | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, modulation(1) | add(0), NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0)],
    [FB | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0), NO_MOD | add(0)],
    [NO_MOD | out(1), NO_MOD | add(1), modulation(1) | OUTPUT, FB | out(1), modulation(1) | add(0), NO_MOD | add(0)],
    [NO_MOD | OUTPUT, FB | out(1), modulation(1) | out(1), modulation(1) | add(0), NO_MOD | out(1), modulation(1) | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, NO_MOD | out(1), modulation(1) | add(0), NO_MOD | add(0), NO_MOD | add(0)],
    [NO_MOD | OUTPUT, FB | out(1), modulation(1) | out(1), modulation(1) | add(0), NO_MOD | add(0), NO_MOD | add(0)],
    [FB | out(1), modulation(1) | OUTPUT, NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0)],
    [FB | OUTPUT, NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0), NO_MOD | add(0)],
];

/// Where an operator's modulation comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    /// A buffer another operator wrote.
    Buffer,
    None,
    /// The chain's own output, fed back from this operator in it.
    Feedback(usize),
    /// Fed back from an operator outside the chain, which nothing can render.
    Unreachable,
}

/// A run of operators each modulating the next, rendered together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Call {
    n: usize,
    source: Source,
    additive: bool,
    input: usize,
    output: usize,
}

/// The chains Plaits has renderers for. Others are split into single operators.
fn has_renderer(n: usize, source: Source, additive: bool) -> bool {
    match (n, source) {
        (1, Source::Buffer | Source::None | Source::Feedback(0)) => true,
        (3, Source::Feedback(2)) | (2, Source::Feedback(1)) => additive,
        _ => false,
    }
}

/// An algorithm's chains, in the order they're rendered, as Plaits' `Compile` makes them.
fn compile(algorithm: usize) -> ([Call; OPERATORS], usize) {
    let opcodes = OPCODES[algorithm];
    let mut calls = [Call {
        n: 1,
        source: Source::None,
        additive: true,
        input: 0,
        output: 0,
    }; OPERATORS];
    let mut count = 0;
    let mut i = 0;
    while i < OPERATORS {
        let opcode = opcodes[i];
        let mut n = 1;
        while i + n < OPERATORS {
            let from = opcodes[i + n - 1];
            let to = (opcodes[i + n] & SOURCE_MASK) >> 4;
            let has_additive = from & ADDITIVE != 0;
            let broken = (from & DESTINATION_MASK) != to;
            if has_additive || broken {
                // A modulation reused by later operators, as in algorithms 19 to 25, breaks the
                // chain.
                if to == opcode & DESTINATION_MASK {
                    n = 1;
                }
                break;
            }
            n += 1;
        }
        for _ in 0..2 {
            let out_opcode = opcodes[i + n - 1];
            let additive = out_opcode & ADDITIVE != 0;
            let source = if opcode & SOURCE_MASK == 0 {
                Source::None
            } else if opcode & SOURCE_MASK != SOURCE_FEEDBACK {
                Source::Buffer
            } else {
                (0..n)
                    .rev()
                    .find(|j| opcodes[i + j] & FEEDBACK_SOURCE != 0)
                    .map_or(Source::Unreachable, Source::Feedback)
            };
            if has_renderer(n, source, additive) {
                calls[count] = Call {
                    n,
                    source,
                    additive,
                    input: usize::from((opcode & SOURCE_MASK) >> 4),
                    output: usize::from(out_opcode & DESTINATION_MASK),
                };
                count += 1;
                break;
            }
            n = 1;
        }
        i += n;
    }
    (calls, count)
}

/// Whether an operator modulates another rather than being heard.
fn is_modulator(algorithm: usize, op: usize) -> bool {
    OPCODES[algorithm][op] & DESTINATION_MASK != 0
}

// Operators, from `operator.h`.

/// A sine at a phase moved by a number of turns, up to 32 either way.
#[inline]
fn sine_pm(phase: u32, pm: f32) -> f32 {
    const SCALE: f32 = 4_294_967_296.0 / 64.0;
    let offset = ((pm + 32.0) * SCALE) as u32;
    let phase = phase.wrapping_add(offset.wrapping_mul(64));
    let index = (phase >> 23) as usize;
    let fraction = (phase << 9) as f32 / 4_294_967_296.0;
    let a = SINE[index];
    let b = SINE[index + 1];
    a + (b - a) * fraction
}

#[derive(Debug, Clone, Copy, Default)]
struct Operator {
    phase: u32,
    amplitude: f32,
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct SixOp {
    one_hz: f32,
    rate: f32,
    patch: Option<Patch>,
    calls: [Call; OPERATORS],
    call_count: usize,
    algorithm: usize,
    operators: [Operator; OPERATORS],
    envelopes: [Envelope; OPERATORS],
    pitch_envelope: Envelope,
    ratios: [f32; OPERATORS],
    headroom: [f32; OPERATORS],
    feedback: [f32; 2],
    lfo: Lfo,
    /// Whether the note is held, and whether it was a block ago.
    gate: bool,
    was_gated: bool,
    note: f32,
    velocity: f32,
    buffers: [[f32; BLOCK]; 3],
    output: [f32; BLOCK],
    at: usize,
}

impl SixOp {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32;
        // Its envelopes count in the DX7's own rate.
        let scale = 44_100.0 / rate;
        Self {
            one_hz: 1.0 / rate,
            rate,
            patch: None,
            calls: compile(0).0,
            call_count: 0,
            algorithm: 0,
            operators: [Operator::default(); OPERATORS],
            envelopes: [Envelope::new(scale, true); OPERATORS],
            pitch_envelope: Envelope::new(scale, false),
            ratios: [0.0; OPERATORS],
            headroom: [0.0; OPERATORS],
            feedback: [0.0; 2],
            lfo: Lfo::new(),
            gate: true,
            was_gated: false,
            note: 48.0,
            velocity: 10.0,
            buffers: [[0.0; BLOCK]; 3],
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// Let go of the note, so its envelopes start their release.
    pub fn release(&mut self) {
        self.gate = false;
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
        sample * LEVEL
    }

    /// Load the patch a note starts with, and work out what stays the same while it plays.
    fn load(&mut self, index: usize) {
        let patch = Patch::load(index);
        self.pitch_envelope
            .set_pitch(patch.pitch_rate, patch.pitch_level);
        for (i, op) in patch.op.iter().enumerate() {
            let level = operator_level(op.level_total);
            self.envelopes[i].set_operator(op.rate, op.level, level);
            // Scaling and velocity can't raise an operator past what a level of 99 would.
            self.headroom[i] = (127 - level) as f32;
            // The sign says whether it follows the note or holds a fixed frequency.
            let sign = if op.mode == 0 { 1.0 } else { -1.0 };
            self.ratios[i] = sign * frequency_ratio(op);
        }
        self.algorithm = usize::from(patch.algorithm);
        (self.calls, self.call_count) = compile(self.algorithm);
        self.lfo.set(patch.modulations, self.one_hz);
        self.lfo.reset();
        self.patch = Some(patch);
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        if self.patch.is_none() {
            self.load(patch_index(settings.harmonics));
        }
        let Some(patch) = self.patch else {
            return;
        };
        let control = settings.morph.clamp(0.0, 1.0);
        let brightness = settings.timbre.clamp(0.0, 1.0);
        self.lfo.step(BLOCK as f32);
        let pitch_mod_lfo = self.lfo.pitch_mod();
        let amp_mod = self.lfo.amp_mod();

        let envelope_rate = BLOCK as f32;
        // Morph speeds the envelopes up or slows them down, around its middle.
        let ad_scale = ((0.5 - control) * 8.0).exp2();
        let r_scale = (-(control - 0.3).abs() * 8.0).exp2();
        let pitch_envelope =
            self.pitch_envelope
                .render(self.gate, envelope_rate, ad_scale, r_scale);
        let f0 = frequency * (pitch_envelope + pitch_mod_lfo).exp2();

        let note_on = self.gate && !self.was_gated;
        self.was_gated = self.gate;
        if note_on {
            self.velocity = normalize_velocity(settings.accent);
            self.note = 69.0 + 12.0 * (frequency * self.rate / 440.0).max(1e-6).log2();
            if patch.reset_phase {
                for op in &mut self.operators {
                    op.phase = 0;
                }
            }
        }

        let mut f = [0.0; OPERATORS];
        let mut a = [0.0; OPERATORS];
        for i in 0..OPERATORS {
            let op = &patch.op[i];
            f[i] = self.ratios[i]
                * if self.ratios[i] < 0.0 {
                    -self.one_hz
                } else {
                    f0
                };
            let speed = rate_scaling(self.note, op.rate_scaling);
            let mut level =
                self.envelopes[i].render(self.gate, envelope_rate * speed, ad_scale, r_scale);
            let kb_scaling = keyboard_scaling(self.note, op.scaling);
            let velocity_scaling = self.velocity * f32::from(op.velocity_sensitivity);
            let brightness = if is_modulator(self.algorithm, i) {
                (brightness - 0.5) * 32.0
            } else {
                0.0
            };
            level += 0.125 * (kb_scaling + velocity_scaling + brightness).min(self.headroom[i]);
            let sensitivity = AMP_MOD_SENSITIVITY[usize::from(op.amp_mod_sensitivity)];
            let level_mod = 1.0 - (6.4 * (sensitivity * amp_mod - 1.0)).exp2();
            a[i] = (-14.0 + level * level_mod).exp2();
        }

        self.buffers[0] = [0.0; BLOCK];
        let mut i = 0;
        let calls = self.calls;
        for call in calls[..self.call_count].iter().copied() {
            self.render_operators(i, call, &f, &a, patch.feedback);
            i += call.n;
        }
        for (out, sample) in self.output.iter_mut().zip(self.buffers[0]) {
            *out = soft_clip(sample * 0.25);
        }
    }

    /// Render a chain of operators starting at `first`.
    fn render_operators(
        &mut self,
        first: usize,
        call: Call,
        f: &[f32; OPERATORS],
        a: &[f32; OPERATORS],
        feedback_amount: u8,
    ) {
        let n = call.n;
        let mut frequency = [0_u32; 3];
        let mut phase = [0_u32; 3];
        let mut amplitude = [0.0_f32; 3];
        let mut amplitude_step = [0.0_f32; 3];
        for j in 0..n {
            let op = &self.operators[first + j];
            frequency[j] = (f[first + j].min(0.5) * 4_294_967_296.0) as u32;
            phase[j] = op.phase;
            amplitude[j] = op.amplitude;
            amplitude_step[j] = (a[first + j].min(4.0) - op.amplitude) / BLOCK as f32;
        }
        let feedback_scale = if feedback_amount == 0 {
            0.0
        } else {
            (1 << feedback_amount) as f32 / 512.0
        };
        let [mut previous_0, mut previous_1] = self.feedback;
        // A feedback source reads no buffer, and buffer 3 is the same as buffer 2.
        let input = call.input.min(2);
        let output = call.output;
        for k in 0..BLOCK {
            let mut pm = match call.source {
                Source::Feedback(_) => (previous_0 + previous_1) * feedback_scale,
                Source::Buffer => self.buffers[input][k],
                Source::None | Source::Unreachable => 0.0,
            };
            for j in 0..n {
                phase[j] = phase[j].wrapping_add(frequency[j]);
                pm = sine_pm(phase[j], pm) * amplitude[j];
                amplitude[j] += amplitude_step[j];
                if call.source == Source::Feedback(j) {
                    previous_1 = previous_0;
                    previous_0 = pm;
                }
            }
            if call.additive {
                self.buffers[output][k] += pm;
            } else {
                self.buffers[output][k] = pm;
            }
        }
        for j in 0..n {
            self.operators[first + j] = Operator {
                phase: phase[j],
                amplitude: amplitude[j],
            };
        }
        if matches!(call.source, Source::Feedback(_)) {
            self.feedback = [previous_0, previous_1];
        }
    }
}

/// Stmlib's soft clip, which holds a sample within -1 to 1.
#[inline]
fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn settings(harmonics: f32) -> Settings {
        Settings {
            harmonics,
            timbre: 0.5,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn run(voice: &mut SixOp, hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()))
    }

    fn index_of(name: &str) -> f32 {
        let index = (0..PATCHES).find(|i| patch_name(*i) == name).unwrap();
        index as f32 / 255.0
    }

    #[test]
    fn the_banks_hold_96_named_patches() {
        assert_eq!(patch_name(0), "SOLID BASS");
        assert_eq!(patch_name(32), "E.PIANO 1");
        assert_eq!(patch_name(95), "BR TRUMPET");
        assert_eq!(patch_index(0.0), 0);
        assert_eq!(patch_index(1.0 / 255.0), 1);
        assert_eq!(patch_index(95.0 / 255.0), 95);
        assert_eq!(patch_index(1.0), 95, "past the last plays the last");
        assert_eq!(patch_name(patch_index(index_of("MARIMBA"))), "MARIMBA");
        for (index, name) in vt_model::patch_names::PATCH_NAMES.iter().enumerate() {
            assert_eq!(patch_name(index), *name, "the model's names are the bank's");
        }
    }

    #[test]
    fn every_algorithm_renders_each_operator_once() {
        for algorithm in 0..32 {
            let (calls, count) = compile(algorithm);
            let covered: usize = calls[..count].iter().map(|call| call.n).sum();
            assert_eq!(covered, OPERATORS, "algorithm {}", algorithm + 1);
        }
        // Algorithm 4 feeds operator 4 back to operator 6, across a chain of three.
        let (calls, _) = compile(3);
        assert_eq!(calls[0].n, 3);
        assert_eq!(calls[0].source, Source::Feedback(2));
    }

    #[test]
    fn a_piano_sounds_at_its_note_then_dies_away_once_released() {
        let mut voice = SixOp::new(RATE);
        let piano = settings(index_of("E.PIANO 1"));
        let held = run(&mut voice, 220.0, piano, 24_000);
        let loud = peak(&held[..4_800]);
        assert!(loud > 0.05 && loud <= 1.0, "{loud}");
        voice.release();
        let released = run(&mut voice, 220.0, piano, 96_000);
        assert!(peak(&released[90_000..]) < loud * 0.05, "it rings on");
    }

    #[test]
    fn brightness_and_patch_change_the_sound() {
        let render = |s: Settings| run(&mut SixOp::new(RATE), 110.0, s, 4_800);
        let plain = render(settings(index_of("E.PIANO 1")));
        assert_ne!(
            render(Settings {
                timbre: 0.9,
                ..settings(index_of("E.PIANO 1"))
            }),
            plain
        );
        assert_ne!(render(settings(index_of("BRASS   1"))), plain);
    }

    #[test]
    fn every_patch_at_any_note_stays_finite_and_in_range() {
        for index in 0..PATCHES {
            for hertz in [30.0, 440.0, 4_000.0] {
                for (timbre, morph) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0)] {
                    let s = Settings {
                        harmonics: index as f32 / 255.0,
                        timbre,
                        morph,
                        ..Settings::default()
                    };
                    let mut voice = SixOp::new(RATE);
                    for sample in run(&mut voice, hertz, s, 2_400) {
                        assert!(sample.is_finite() && sample.abs() <= 1.0, "{index} {hertz}");
                    }
                }
            }
        }
    }
}
