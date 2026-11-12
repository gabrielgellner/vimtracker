//! Four operators you build an FM sound from yourself, after the matrix of tilr's TetraOP
//! (`engine/FmMatrix.cpp`, GPL-3): every operator modulates through the one-sample-late outputs of
//! the others, so the four are worked out side by side, one to each lane of an `f32x4`, and an
//! algorithm is only which entries of the 4x4 matrix are on. Each operator has a ratio, a level, an
//! envelope, one of the TX81Z's eight waves made from a sine and a cosine by quadrant, as Surge's
//! Sine oscillator makes them (`SineOscillator.cpp`, GPL-3.0-or-later), and one of TetraOP's
//! phase-distortion functions (`engine/PhaseDist.h`). From TetraOP too is its ring modulation, a
//! crossfade from an operator's output to it times its modulators'; from Surge, squared feedback
//! when `Feedback` is below the middle (`SineOscillator.cpp`), and its FM2's offset in Hz between
//! two modulators, for beating (`FM2Oscillator.cpp`). Ours are kept where theirs are thinner: the
//! feedback is the average of the last two samples, as `six_op` has the DX7's, the index is tamed
//! on high notes as `fm` tames Plaits', and the whole voice runs at twice the rate through the
//! halfband `oversampler` has. This is written from them, not ported.

use wide::f32x4;

use super::virtual_analog::{BLOCK, Settings};
use crate::dsp::blocks::oversampler::Decimator;
use vt_model::settings::{FOUR_OP_ALGORITHMS, FOUR_OP_EACH, FOUR_OP_SHARED};

/// How many operators, one to a lane.
const OPERATORS: usize = 4;
/// The settings before the operators' own: the algorithm, feedback, ring modulation and beating.
const ALGORITHM: usize = 0;
const FEEDBACK: usize = 1;
const RING: usize = 2;
const BEAT: usize = 3;
/// Each operator's, after those, in this order.
const RATIO: usize = 0;
const FINE: usize = 1;
const LEVEL: usize = 2;
const WAVE: usize = 3;
const WARP: usize = 4;
const AMOUNT: usize = 5;
const ATTACK: usize = 6;
const DECAY: usize = 7;
const SUSTAIN: usize = 8;
const RELEASE: usize = 9;

/// Where an operator's setting is among the voice's.
#[inline]
const fn place(operator: usize, setting: usize) -> usize {
    FOUR_OP_SHARED + operator * FOUR_OP_EACH + setting
}

/// The ratios `Ratio` steps through, as the Digitone's do, as the model names them.
const RATIOS: [f32; 20] = [
    0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0,
    14.0, 15.0, 16.0,
];
/// How far `Fine` moves a ratio either way at its ends, in cents. It's squared on the way, so the
/// middle, where a slight detune lives, has most of the knob.
const FINE_CENTS: f32 = 600.0;
/// How far a modulator at full level moves its target's phase, in turns, before taming: about
/// 12.6 radians, as far as a DX7's go.
const INDEX: f32 = 2.0;
/// How far the feedback moves the operator's own phase at full, in turns, either way.
const FEEDBACK_TURNS: f32 = 0.25;
/// How far apart `Beat` sets two modulators at full, in Hz.
const BEAT_HERTZ: f32 = 10.0;
/// An envelope's shortest and longest times, in ms, `00` to `FF` evenly in ratio.
const SHORTEST_MS: f32 = 0.5;
const LONGEST_MS: f32 = 20_000.0;
/// How loud it is against the other machines.
const LEVEL_OUT: f32 = 0.5;
/// The highest frequency an operator runs at, as a fraction of the doubled rate.
const MAX_FREQUENCY: f32 = 0.24;

/// Each algorithm's connections, as which operators each one modulates, one bit an operator:
/// TetraOP's ten layouts, with operator 1 its A and 4 its D, and all four as carriers last. A
/// carrier is an operator that modulates nothing, and only carriers are heard.
const ALGORITHMS: [[u8; OPERATORS]; FOUR_OP_ALGORITHMS.len()] = [
    // 4>3>2>1
    [0, 0b0001, 0b0010, 0b0100],
    // 3>1 4>2
    [0, 0, 0b0001, 0b0010],
    // 4>2 1 3
    [0, 0, 0, 0b0010],
    // 4>1,2,3
    [0, 0, 0, 0b0111],
    // 2,3,4>1
    [0, 0b0001, 0b0001, 0b0001],
    // 4>3>2 1
    [0, 0, 0b0010, 0b0100],
    // 4>3>1,2
    [0, 0, 0b0011, 0b0100],
    // 4>2,3>1
    [0, 0b0001, 0b0001, 0b0110],
    // 3,4>2>1
    [0, 0b0001, 0b0010, 0b0010],
    // 1 2 3 4
    [0, 0, 0, 0],
];

/// What `Wave` chooses: the TX81Z's eight, from a sine and a cosine by quadrant, as Surge's Sine
/// oscillator has its first eight shapes.
const WAVES: usize = 8;
/// What `Warp` chooses: none, then TetraOP's phase distortions but `Bend`, whose power a sample
/// costs too much here.
const WARPS: usize = 8;
const SKEW: usize = 1;
const BIAS: usize = 2;
const PULSE: usize = 3;
const SYNC: usize = 4;
const FOLD: usize = 5;
const STEPS: usize = 6;
const FORMANT: usize = 7;

/// The numbers the lanes use. Every other constant in the kernel is made in a `const` block, as
/// made with `splat` in the loop some come out as a call to fill memory each sample.
const ONE: f32x4 = f32x4::ONE;
const HALF: f32x4 = f32x4::splat(0.5);
const QUARTER: f32x4 = f32x4::splat(0.25);
const ALMOST_ONE: f32x4 = f32x4::splat(0.999_999);
const SIGN: f32x4 = f32x4::splat(-0.0);

/// A sine of a phase in turns, within about 1e-6: `sin 2πx` is `cos 2π(|x| - 1/4)` with the sign
/// of `x`, for `x` on `-1/2..1/2`, and the cosine is its series to the tenth power, in pairs
/// (Estrin's scheme), so each sample waits on fewer multiplies in a row. `round` and `copysign`
/// make their constants each call, so they're written out.
#[inline(always)]
fn sine(phase: f32x4) -> f32x4 {
    let x = phase - (phase + HALF).floor();
    let w = (x.abs() - QUARTER) * const { f32x4::splat(std::f32::consts::TAU) };
    let v = w * w;
    let v2 = v * v;
    let v4 = v2 * v2;
    let a = ONE - HALF * v;
    let b = const { f32x4::splat(1.0 / 24.0) } - const { f32x4::splat(1.0 / 720.0) } * v;
    let c = const { f32x4::splat(1.0 / 40_320.0) } - const { f32x4::splat(1.0 / 3_628_800.0) } * v;
    (a + b * v2 + c * v4) ^ (x & SIGN)
}

/// The waves, from the sine and cosine of the phase: a sine; a sine squared, signed; its first
/// half; that squared; a sine at twice the rate in the first half; that squared, signed; and the
/// last two with their second halves turned up.
#[inline(always)]
fn wave(kind: usize, s: f32x4, c: f32x4) -> f32x4 {
    let first = s.simd_ge(f32x4::ZERO);
    let twice = const { f32x4::splat(2.0) } * s * c;
    match kind {
        0 => s,
        1 => s * s.abs(),
        2 => first & s,
        3 => first & (s * s),
        4 => first & twice,
        5 => first & (twice * twice.abs()),
        6 => first & twice.abs(),
        _ => first & (twice * twice),
    }
}

/// What each phase distortion works out from its amount, on `-1..1`, once a block rather than a
/// sample: TetraOP's breakpoints, and what its divisions by them come to.
#[derive(Debug, Clone, Copy)]
struct Bends {
    amount: f32x4,
    /// `Skew`'s two breakpoints and the slopes of its three lines.
    skew: [f32x4; 5],
    /// `Bias`'s split and the slopes either side of it.
    bias: [f32x4; 3],
    /// `Pulse`'s stretch, TetraOP's `exp(|a| ln 5)`, and how many steps a turn has for `Steps`,
    /// and one over that.
    pulse: f32x4,
    steps: [f32x4; 2],
    /// How many times `Sync` and `Formant` run through a cycle.
    sync: f32x4,
}

impl Bends {
    fn new(amounts: [f32; OPERATORS]) -> Self {
        let amount = f32x4::new(amounts);
        let skewed = amount * f32x4::splat(0.99);
        let p1 = QUARTER + skewed * QUARTER;
        let p2 = f32x4::splat(0.75) - skewed * QUARTER;
        let split = (HALF + amount * f32x4::splat(0.49))
            .max(f32x4::splat(0.001))
            .min(f32x4::splat(0.999));
        let steps = f32x4::new(amounts.map(|amount| {
            ((1.0 - amount.abs().sqrt()) * 0.95 * 16.0 * std::f32::consts::LN_2).exp()
        }));
        Self {
            amount,
            skew: [p1, p2, QUARTER / p1, HALF / (p2 - p1), QUARTER / (ONE - p2)],
            bias: [split, HALF / split, HALF / (ONE - split)],
            pulse: f32x4::new(amounts.map(|amount| (amount.abs() * 5_f32.ln()).exp())),
            steps: [steps, ONE / steps],
            sync: ONE + amount * amount * f32x4::splat(15.0),
        }
    }
}

/// A phase distortion of `kind`, from TetraOP's, its amount worked out in `bends`.
#[inline(always)]
fn warp(kind: usize, phase: f32x4, bends: &Bends) -> f32x4 {
    match kind {
        SKEW => {
            let [p1, p2, k1, k2, k3] = bends.skew;
            let r1 = phase * k1;
            let r2 = (phase - p1) * k2 + QUARTER;
            let r3 = (phase - p2) * k3 + const { f32x4::splat(0.75) };
            let r23 = phase.simd_lt(p2).select(r2, r3);
            phase.simd_lt(p1).select(r1, r23).min(ALMOST_ONE)
        }
        BIAS => {
            let [split, low, high] = bends.bias;
            let below = phase * low;
            let above = (phase - split) * high + HALF;
            phase.simd_lt(split).select(below, above).min(ALMOST_ONE)
        }
        PULSE => {
            let negative = bends.amount.simd_lt(f32x4::ZERO);
            let turned = negative.select(ONE - phase, phase);
            let pulsed = (turned * bends.pulse).min(ALMOST_ONE);
            negative.select(ONE - pulsed, pulsed).min(ALMOST_ONE)
        }
        SYNC | FORMANT => {
            let synced = phase * bends.sync;
            synced - synced.floor()
        }
        FOLD => {
            let y = phase
                + phase
                    * (ONE - phase)
                    * (HALF - phase)
                    * bends.amount.abs()
                    * const { f32x4::splat(16.0) };
            let y = y.abs();
            y - y.trunc()
        }
        STEPS => {
            let [count, step] = bends.steps;
            ((phase * count + HALF).trunc() * step).min(ALMOST_ONE)
        }
        _ => phase,
    }
}

/// An operator's envelope, worked out once a block: an attack toward a little over full, so it
/// gets there, then a decay to the sustain while the note's held, and a release to nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Attack,
    Decay,
    Release,
}

/// How much of the way a one-pole goes each block for a time on `0..=1`.
#[inline]
fn coefficient(time: f32, rate: f32) -> f32 {
    let ms = SHORTEST_MS * (LONGEST_MS / SHORTEST_MS).powf(time.clamp(0.0, 1.0));
    1.0 - (-(BLOCK as f32) * 1000.0 / (ms * rate)).exp()
}

/// How far a modulator is let bend its target on a note: Plaits' taming in `fm`, by how far above
/// the bottom of the keyboard the modulator's pitch is.
#[inline]
fn taming(note: f32, ratio: f32) -> f32 {
    let semitones = 12.0 * ratio.max(1e-3).log2();
    (1.0 - (note - 24.0 + semitones - 72.0) * 0.025).clamp(0.0, 1.0)
}

/// What the settings and the note make of the operators, worked out again only when they move.
#[derive(Debug, Clone, Copy)]
struct Prepared {
    frequency: f32x4,
    /// How far each operator's level reaches: how hard a modulator bends at full, tamed, or how
    /// loud a carrier is.
    reach: [f32; OPERATORS],
    /// Its envelope's attack, decay and release, as how far each goes a block, and its sustain.
    rates: [[f32; 3]; OPERATORS],
    sustain: [f32; OPERATORS],
    bends: Bends,
    feedback: f32,
    ring: f32,
    columns: [f32x4; OPERATORS],
    carriers: f32x4,
    modulated: f32x4,
    waves: [f32x4; WAVES],
    waves_used: u16,
    warps: [f32x4; WARPS],
    warps_used: u16,
}

/// What a block plays: each lane's frequency, gain, envelope, warp and wave, where they start and
/// how they move a sub-sample.
#[derive(Debug, Clone, Copy)]
struct Plan {
    frequency: f32x4,
    frequency_step: f32x4,
    gain: f32x4,
    gain_step: f32x4,
    bends: Bends,
    feedback: f32,
    feedback_step: f32,
    ring: f32,
    ring_step: f32,
    /// Which operator each modulates, a lane a source, and which are carriers or modulated.
    columns: [f32x4; OPERATORS],
    carriers: f32x4,
    modulated: f32x4,
    /// Which lanes have each wave and each warp, and which are in use at all.
    waves: [f32x4; WAVES],
    waves_used: u16,
    warps: [f32x4; WARPS],
    warps_used: u16,
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct FourOp {
    rate: f32,
    phase: f32x4,
    /// Each operator's last two outputs, its level in them, and its wave alone, for ring
    /// modulation.
    output: f32x4,
    previous: f32x4,
    shape: f32x4,
    stage: [Stage; OPERATORS],
    envelope: [f32; OPERATORS],
    frequency: f32x4,
    gain: f32x4,
    feedback: f32,
    ring: f32,
    decimator: Decimator,
    /// What it last prepared, and for which note and settings.
    prepared: Prepared,
    prepared_for: Option<(f32, Settings)>,
    frames: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl FourOp {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1) as f32,
            phase: f32x4::ZERO,
            output: f32x4::ZERO,
            previous: f32x4::ZERO,
            shape: f32x4::ZERO,
            stage: [Stage::Attack; OPERATORS],
            envelope: [0.0; OPERATORS],
            frequency: f32x4::ZERO,
            gain: f32x4::ZERO,
            feedback: 0.0,
            ring: 0.0,
            decimator: Decimator::default(),
            prepared: Prepared {
                frequency: f32x4::ZERO,
                reach: [0.0; OPERATORS],
                rates: [[0.0; 3]; OPERATORS],
                sustain: [0.0; OPERATORS],
                bends: Bends::new([0.0; OPERATORS]),
                feedback: 0.0,
                ring: 0.0,
                columns: [f32x4::ZERO; OPERATORS],
                carriers: f32x4::ZERO,
                modulated: f32x4::ZERO,
                waves: [f32x4::ZERO; WAVES],
                waves_used: 1,
                warps: [f32x4::ZERO; WARPS],
                warps_used: 1,
            },
            prepared_for: None,
            frames: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Let go of the note: every operator's envelope goes to its release.
    pub fn release(&mut self) {
        self.stage = [Stage::Release; OPERATORS];
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            let plan = self.plan(frequency, settings);
            self.play(&plan);
            self.at = 0;
        }
        let sample = self.frames[self.at];
        self.at += 1;
        sample * LEVEL_OUT
    }

    /// Work out what the settings and the note make of each operator, which holds until either
    /// moves: its pitch, how far its level reaches, its envelope's rates, its wave and warp.
    fn prepare(&self, frequency: f32, settings: Settings) -> Prepared {
        let operator = |op: usize, setting: usize| settings.at(place(op, setting)).clamp(0.0, 1.0);
        let algorithm =
            ((settings.at(ALGORITHM) * 255.0).round() as usize).min(ALGORITHMS.len() - 1);
        let routes = ALGORITHMS[algorithm];
        let carrier: [bool; OPERATORS] = std::array::from_fn(|op| routes[op] == 0);
        let carriers = carrier.iter().filter(|carrier| **carrier).count().max(1) as f32;
        let note = 69.0 + 12.0 * (frequency * self.rate / 440.0).max(1e-3).log2();
        let beat = settings.at(BEAT).clamp(0.0, 1.0);
        let beat = BEAT_HERTZ * beat * beat;
        let mut beating = 1.0;
        let mut frequencies = [0.0; OPERATORS];
        let mut reach = [0.0; OPERATORS];
        let mut rates = [[0.0; 3]; OPERATORS];
        let mut sustain = [0.0; OPERATORS];
        let mut amounts = [0.0; OPERATORS];
        let mut waves = [0usize; OPERATORS];
        let mut warps = [0usize; OPERATORS];
        for op in 0..OPERATORS {
            let step = ((operator(op, RATIO) * 255.0).round() as usize).min(RATIOS.len() - 1);
            let fine = operator(op, FINE) * 2.0 - 1.0;
            let ratio = RATIOS[step] * (FINE_CENTS * fine * fine.abs() / 1200.0).exp2();
            let mut hertz = frequency * self.rate * ratio;
            // Modulators beat against each other, one up and the next down.
            if !carrier[op] {
                hertz += beating * beat;
                beating = -beating;
            }
            frequencies[op] = (hertz / self.rate * 0.5).clamp(0.0, MAX_FREQUENCY);
            let level = operator(op, LEVEL);
            reach[op] = level
                * level
                * if carrier[op] {
                    1.0 / carriers
                } else {
                    let tamed = taming(note, ratio);
                    INDEX * tamed * tamed
                };
            rates[op] =
                [ATTACK, DECAY, RELEASE].map(|time| coefficient(operator(op, time), self.rate));
            sustain[op] = operator(op, SUSTAIN);
            waves[op] = ((operator(op, WAVE) * 255.0).round() as usize).min(WAVES - 1);
            warps[op] = ((operator(op, WARP) * 255.0).round() as usize).min(WARPS - 1);
            let amount = operator(op, AMOUNT) * 2.0 - 1.0;
            amounts[op] = amount;
        }
        let lanes = |on: [bool; OPERATORS]| {
            f32x4::new(on.map(|on| if on { f32::from_bits(u32::MAX) } else { 0.0 }))
        };
        let mut waves_used = 0;
        let mut warps_used = 0;
        for op in 0..OPERATORS {
            waves_used |= 1 << waves[op];
            warps_used |= 1 << warps[op];
        }
        Prepared {
            frequency: f32x4::new(frequencies),
            reach,
            rates,
            sustain,
            bends: Bends::new(amounts),
            feedback: settings.at(FEEDBACK).clamp(0.0, 1.0) * 2.0 - 1.0,
            ring: settings.at(RING).clamp(0.0, 1.0),
            columns: std::array::from_fn(|source| {
                f32x4::new(std::array::from_fn(|target| {
                    f32::from(routes[source] >> target & 1 == 1)
                }))
            }),
            carriers: lanes(carrier),
            modulated: lanes(std::array::from_fn(|target| {
                routes.iter().any(|route| route >> target & 1 == 1)
            })),
            waves: std::array::from_fn(|kind| lanes(waves.map(|wave| wave == kind))),
            waves_used,
            warps: std::array::from_fn(|kind| lanes(warps.map(|warp| warp == kind))),
            warps_used,
        }
    }

    /// Work out a block's targets, moving the envelopes on a block.
    fn plan(&mut self, frequency: f32, settings: Settings) -> Plan {
        let frequency = frequency.clamp(1e-6, 0.49);
        if self.prepared_for != Some((frequency, settings)) {
            self.prepared = self.prepare(frequency, settings);
            self.prepared_for = Some((frequency, settings));
        }
        let prepared = &self.prepared;
        let mut gains = [0.0; OPERATORS];
        for (op, gain) in gains.iter_mut().enumerate() {
            let [attack, decay, release] = prepared.rates[op];
            let envelope = &mut self.envelope[op];
            match self.stage[op] {
                Stage::Attack => {
                    *envelope += (1.3 - *envelope) * attack;
                    if *envelope >= 1.0 {
                        *envelope = 1.0;
                        self.stage[op] = Stage::Decay;
                    }
                }
                Stage::Decay => *envelope += (prepared.sustain[op] - *envelope) * decay,
                Stage::Release => *envelope -= *envelope * release,
            }
            *gain = *envelope * prepared.reach[op];
        }
        let (frequency, feedback, ring) = (prepared.frequency, prepared.feedback, prepared.ring);
        let gain = f32x4::new(gains);
        if self.fresh {
            self.frequency = frequency;
            self.gain = gain;
            self.feedback = feedback;
            self.ring = ring;
            self.fresh = false;
        }
        let steps = const { f32x4::splat((2 * BLOCK) as f32) };
        let plan = Plan {
            frequency: self.frequency,
            frequency_step: (frequency - self.frequency) / steps,
            gain: self.gain,
            gain_step: (gain - self.gain) / steps,
            bends: prepared.bends,
            feedback: self.feedback,
            feedback_step: (feedback - self.feedback) / (2 * BLOCK) as f32,
            ring: self.ring,
            ring_step: (ring - self.ring) / (2 * BLOCK) as f32,
            columns: prepared.columns,
            carriers: prepared.carriers,
            modulated: prepared.modulated,
            waves: prepared.waves,
            waves_used: prepared.waves_used,
            warps: prepared.warps,
            warps_used: prepared.warps_used,
        };
        self.frequency = frequency;
        self.gain = gain;
        self.feedback = feedback;
        self.ring = ring;
        plan
    }

    /// Play a block at twice the rate and bring it down.
    fn play(&mut self, plan: &Plan) {
        let mut frequency = plan.frequency;
        let mut gain = plan.gain;
        let mut feedback = plan.feedback;
        let mut ring = plan.ring;
        let mut phase = self.phase;
        let mut output = self.output;
        let mut previous = self.previous;
        let mut shape = self.shape;
        let last = f32x4::new([0.0, 0.0, 0.0, f32::from_bits(u32::MAX)]);
        let quarter = QUARTER;
        let mut twice = [0.0; 2];
        for frame in 0..BLOCK {
            for half in &mut twice {
                frequency += plan.frequency_step;
                gain += plan.gain_step;
                feedback += plan.feedback_step;
                ring += plan.ring_step;
                // Every connection is one sample late, so the order doesn't matter.
                let [a, b, c, d] = output.to_array();
                let modulation = plan.columns[0] * f32x4::splat(a)
                    + plan.columns[1] * f32x4::splat(b)
                    + plan.columns[2] * f32x4::splat(c)
                    + plan.columns[3] * f32x4::splat(d);
                // Operator 4 feeds back the average of its last two, squared below the middle.
                let average = 0.5 * (d + previous.to_array()[3]);
                let fed = if feedback < 0.0 {
                    -feedback * average * average.abs()
                } else {
                    feedback * average
                };
                let modulation = modulation + (last & f32x4::splat(FEEDBACK_TURNS * fed));
                phase += frequency;
                phase -= phase.floor();
                let mut read = phase + modulation;
                read -= read.floor();
                // `Formant` windows the cycle as it was before it was bent.
                let unwarped = read;
                if plan.warps_used & !1 != 0 {
                    let mut warped = read;
                    for kind in 1..WARPS {
                        if plan.warps_used & (1 << kind) != 0 {
                            let this = warp(kind, read, &plan.bends);
                            warped = plan.warps[kind].select(this, warped);
                        }
                    }
                    read = warped;
                }
                let s = sine(read);
                let mut wave_out = s;
                if plan.waves_used != 1 {
                    let cosine = sine(read + quarter);
                    for kind in 0..WAVES {
                        if plan.waves_used & (1 << kind) != 0 {
                            wave_out = plan.waves[kind].select(wave(kind, s, cosine), wave_out);
                        }
                    }
                }
                if plan.warps_used & (1 << FORMANT) != 0 {
                    let window = sine(unwarped * HALF);
                    wave_out = plan.warps[FORMANT].select(wave_out * window, wave_out);
                }
                // Ring modulation: each modulated operator times what its modulators make,
                // crossfaded in.
                if ring > 0.0 {
                    let [a, b, c, d] = shape.to_array();
                    let ringing = plan.columns[0] * f32x4::splat(a)
                        + plan.columns[1] * f32x4::splat(b)
                        + plan.columns[2] * f32x4::splat(c)
                        + plan.columns[3] * f32x4::splat(d);
                    let r = f32x4::splat(ring);
                    let rung = wave_out * ((ONE - r) + r * ringing);
                    wave_out = plan.modulated.select(rung, wave_out);
                }
                shape = wave_out;
                previous = output;
                output = wave_out * gain;
                *half = (plan.carriers & output).reduce_add();
            }
            self.frames[frame] = self.decimator.process(twice[0], twice[1]);
        }
        self.phase = phase;
        self.output = output;
        self.previous = previous;
        self.shape = shape;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::virtual_analog::VirtualAnalog;
    use vt_model::settings::{MAX_MACHINE_SETTINGS, SettingId};
    use vt_model::song::SynthVoice;

    const RATE: u32 = 48_000;

    /// The settings at their defaults, as the model has them.
    fn plain() -> Settings {
        Settings::from_places(std::array::from_fn(|at| {
            SettingId::voice(SynthVoice::FourOp, at)
                .map_or(0.0, |id| f32::from(id.entry().default) / 255.0)
        }))
    }

    fn with(settings: Settings, changes: &[(usize, f32)]) -> Settings {
        let mut all: [f32; MAX_MACHINE_SETTINGS] = std::array::from_fn(|at| settings.at(at));
        for (at, value) in changes {
            all[*at] = *value;
        }
        Settings::from_places(all)
    }

    /// A list's choice as a setting's value.
    fn choice(index: usize) -> f32 {
        index as f32 / 255.0
    }

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = FourOp::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    fn difference(a: &[f32], b: &[f32]) -> f32 {
        let apart: Vec<f32> = a.iter().zip(b).map(|(a, b)| a - b).collect();
        energy(&apart) / energy(a).max(1e-12)
    }

    /// Energy near the top of the spectrum against all of it: how much each frame jumps.
    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
        total / energy(samples).max(1e-12) / samples.len() as f32
    }

    fn measured_pitch(samples: &[f32]) -> f32 {
        let crossings: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0)
            .map(|(at, _)| at)
            .collect();
        let (first, last) = (crossings[1], crossings[crossings.len() - 1]);
        (crossings.len() - 2) as f32 * RATE as f32 / (last - first) as f32
    }

    #[test]
    fn the_sine_is_a_sine() {
        let mut worst = 0.0_f32;
        for step in 0..4_000 {
            let phase = step as f32 / 1_000.0 - 2.0;
            let ours = sine(f32x4::splat(phase)).to_array()[0];
            worst = worst.max((ours - (phase * std::f32::consts::TAU).sin()).abs());
        }
        assert!(worst < 1e-5, "{worst}");
    }

    #[test]
    fn it_plays_the_note_and_is_about_as_loud_as_the_virtual_analog_voice() {
        // A plain sine crosses the middle once a cycle, where a bent one may not.
        let sine = run(110.0, with(plain(), &[(place(1, LEVEL), 0.0)]), 48_000);
        let pitch = measured_pitch(&sine);
        assert!((pitch - 110.0).abs() < 0.5, "{pitch}");
        let out = run(110.0, plain(), 48_000);
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(110.0 / RATE as f32, Settings::default()))
            .collect();
        let decibels = 10.0 * (energy(&out) / energy(&va)).log10();
        assert!(decibels.abs() < 3.0, "{decibels} dB");
    }

    #[test]
    fn a_modulator_brightens_and_feedback_ring_and_beat_change_the_sound() {
        let soft = run(220.0, with(plain(), &[(place(1, LEVEL), 0.0)]), 24_000);
        let bright = run(220.0, with(plain(), &[(place(1, LEVEL), 1.0)]), 24_000);
        assert!(roughness(&bright) > 2.0 * roughness(&soft));
        // The whole stack bending, so operator 4's feedback is heard.
        let base = with(plain(), &[(place(3, LEVEL), 0.6), (place(2, LEVEL), 0.6)]);
        let plain_out = run(220.0, base, 24_000);
        let up = run(220.0, with(base, &[(FEEDBACK, 1.0)]), 24_000);
        let down = run(220.0, with(base, &[(FEEDBACK, 0.0)]), 24_000);
        assert!(difference(&plain_out, &up) > 0.01);
        assert!(difference(&plain_out, &down) > 0.01);
        assert!(difference(&up, &down) > 0.01);
        let rung = run(220.0, with(base, &[(RING, 1.0)]), 24_000);
        assert!(difference(&plain_out, &rung) > 0.01);
        let beating = run(220.0, with(base, &[(BEAT, 1.0)]), 24_000);
        assert!(difference(&plain_out, &beating) > 0.01);
    }

    #[test]
    fn every_algorithm_wave_and_warp_sounds_its_own() {
        let base = with(
            plain(),
            &[
                (place(1, LEVEL), 0.7),
                (place(2, LEVEL), 0.7),
                (place(3, LEVEL), 0.7),
                (place(2, RATIO), choice(5)),
                (place(3, RATIO), choice(7)),
            ],
        );
        let algorithms: Vec<Vec<f32>> = (0..ALGORITHMS.len())
            .map(|at| run(220.0, with(base, &[(ALGORITHM, choice(at))]), 9_600))
            .collect();
        for (at, a) in algorithms.iter().enumerate() {
            assert!(energy(a) > 1e-4, "{at}");
            for (other, b) in algorithms.iter().enumerate().skip(at + 1) {
                assert!(difference(a, b) > 1e-3, "{at} {other}");
            }
        }
        let carrier = |changes: &[(usize, f32)]| run(220.0, with(plain(), changes), 9_600);
        let waves: Vec<Vec<f32>> = (0..WAVES)
            .map(|wave| carrier(&[(place(0, WAVE), choice(wave))]))
            .collect();
        for (at, a) in waves.iter().enumerate() {
            for b in &waves[at + 1..] {
                assert!(difference(a, b) > 1e-3, "wave {at}");
            }
        }
        let off = carrier(&[(place(0, AMOUNT), 0.9)]);
        assert_eq!(off, carrier(&[]), "Off ignores Amount");
        for warp in 1..WARPS {
            let warped = carrier(&[(place(0, WARP), choice(warp)), (place(0, AMOUNT), 0.9)]);
            assert!(difference(&off, &warped) > 1e-3, "warp {warp}");
        }
    }

    #[test]
    fn an_operators_envelope_shapes_it_and_lets_go() {
        // A slow attack on the carrier starts quiet; a release lets it fall away.
        let slow = run(220.0, with(plain(), &[(place(0, ATTACK), 0.8)]), 2_400);
        assert!(energy(&slow) < 0.2 * energy(&run(220.0, plain(), 2_400)));
        let mut voice = FourOp::new(RATE);
        let settings = with(plain(), &[(place(0, RELEASE), 0.3)]);
        for _ in 0..4_800 {
            voice.next(220.0 / RATE as f32, settings);
        }
        voice.release();
        let after: Vec<f32> = (0..24_000)
            .map(|_| voice.next(220.0 / RATE as f32, settings))
            .collect();
        assert!(energy(&after[20_000..]) < 1e-3 * energy(&after[..1_000]));
    }

    #[test]
    fn high_notes_bend_less() {
        let bright = with(plain(), &[(place(1, LEVEL), 1.0)]);
        let at = |hertz: f32| {
            roughness(&run(hertz, bright, 9_600)) / roughness(&run(hertz, plain(), 9_600))
        };
        let (low, high) = (at(110.0), at(3_520.0));
        assert!(high < low, "{high} {low}");
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for algorithm in 0..ALGORITHMS.len() {
                for value in [0.0, 0.5, 1.0] {
                    let settings = with(
                        Settings::from_places(std::array::from_fn(|_| value)),
                        &[(ALGORITHM, choice(algorithm))],
                    );
                    for sample in run(hertz, settings, 1_200) {
                        assert!(
                            sample.is_finite() && sample.abs() < 2.0,
                            "{sample} at {hertz} {algorithm} {value}"
                        );
                    }
                }
            }
        }
        // Every wave and warp with every operator at full.
        for wave in 0..WAVES {
            for warp in 0..WARPS {
                let mut changes = vec![(ALGORITHM, 0.0), (FEEDBACK, 0.0), (RING, 1.0)];
                for op in 0..OPERATORS {
                    changes.extend([
                        (place(op, LEVEL), 1.0),
                        (place(op, WAVE), choice(wave)),
                        (place(op, WARP), choice(warp)),
                        (place(op, AMOUNT), 1.0),
                    ]);
                }
                for sample in run(55.0, with(plain(), &changes), 2_400) {
                    assert!(sample.is_finite() && sample.abs() < 2.0, "{wave} {warp}");
                }
            }
        }
    }

    /// How long it takes to play a frame at 48 kHz, in ns, as it starts and with all four
    /// operators bending and a warp and a second wave in use, against `VA`. Each is the fastest of
    /// many short runs. Run with
    /// `cargo test --release -p vt-engine four_op::tests::cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cost() {
        use std::hint::black_box;
        use std::time::Instant;
        fn time(name: &str, mut play: impl FnMut() -> f32) {
            let mut fastest = f64::MAX;
            for _ in 0..200 {
                let started = Instant::now();
                let mut sum = 0.0;
                for _ in 0..9_600 {
                    sum += play();
                }
                black_box(sum);
                fastest = fastest.min(started.elapsed().as_nanos() as f64 / 9_600.0);
            }
            println!("{name}: {fastest:.1} ns a frame");
        }
        let settings = plain();
        let mut voice = FourOp::new(RATE);
        time("4-op, as it starts", || {
            voice.next(black_box(0.004_6), settings)
        });
        let busy = with(
            plain(),
            &[
                (place(1, LEVEL), 0.7),
                (place(2, LEVEL), 0.7),
                (place(3, LEVEL), 0.7),
                (FEEDBACK, 0.8),
                (RING, 0.3),
                (place(2, WAVE), choice(4)),
                (place(1, WARP), choice(SKEW)),
            ],
        );
        let mut voice = FourOp::new(RATE);
        time("4-op, all bending, with a warp and a wave", || {
            voice.next(black_box(0.004_6), busy)
        });
        let mut fm = crate::dsp::fm::Fm::new(RATE);
        let bent = Settings {
            timbre: 0.5,
            ..Settings::default()
        };
        time("FM", || fm.next(black_box(0.004_6), bent));
        let mut six_op = crate::dsp::six_op::SixOp::new(RATE);
        time("6-op", || {
            six_op.next(black_box(0.004_6), Settings::default())
        });
        let mut va = VirtualAnalog::new();
        time("VA", || va.next(black_box(0.004_6), Settings::default()));
    }
}
