//! A stack of up to eight detuned saws, or squares, or a blend: the supersaw. Each is our own
//! band-limited oscillator, Plaits' polyBLEP as [`virtual_analog`](super::virtual_analog) has it,
//! rather than a port of Surge's Classic or Modern.
//!
//! The stack is laid out as sst's `UnisonSetup` lays out Surge's unison
//! (`OscillatorDriftUnisonCharacter.h`): the saws' detunings evenly spread on `-1..1`, their gains
//! together as loud as one saw, `1/√N` each when they're alike, and its alternating pan. From
//! TetraOP's `Unison.h` come the warp of that spread, the Fifths and Sub stacks, which take every
//! other saw up a fifth or down an octave, and the mix of the middle saws against the rest. From
//! Surge's Sine oscillator (`SineOscillator.cpp`, `prepare_unison`) comes ramping a saw in or out
//! over about 50 frames at 44.1 kHz, so changing how many there are doesn't click. sst and Surge
//! are GPL-3.0-or-later and TetraOP (copyright tilr 2026) is GPL-3; this is written from them.
//!
//! The saws run four at a time, one to each lane of an `f32x4` from the `wide` crate, as they all
//! do the same thing each frame: eight cost about what two plain saws do. A stack of four lanes
//! with nothing to play is skipped. The tests check the lanes against the same steps worked out
//! one saw at a time.

use wide::f32x4;

use super::virtual_analog::{BLOCK, Settings};
use crate::dsp::blocks::drift::Drift;

/// The most saws it stacks, and how many go in each four lanes.
pub const MAX_SAWS: usize = 8;
const LANES: usize = 4;
const STACKS: usize = MAX_SAWS / LANES;
/// The highest frequency a saw runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;
/// How far the outermost saws go either way at `Detune` full, in cents, as TetraOP's do. `Detune`
/// is squared on the way, so the fine settings, where a supersaw lives, have most of the knob.
const MAX_DETUNE_CENTS: f32 = 200.0;
/// How loud a square is against a saw: theirs are 1 and `1/√3` RMS, so it comes down to a saw's.
const SQUARE_GAIN: f32 = 0.577_350_3;
/// How loud it is against the other machines: about `VA`'s at their defaults.
const LEVEL: f32 = 0.5;
/// How long a saw takes to come in or go, in frames at 44.1 kHz, as Surge's do.
const RAMP_FRAMES: f32 = 50.0;
/// How many stacks `Stack` chooses from: `Unison`, every saw on its own pitch, then `Fifths` and
/// `Sub`, every other one a fifth up or an octave down (TetraOP's `kFifths` and `kSub`).
const MODES: usize = 3;

/// Where each setting is among its own.
const MODE: usize = 0;
const DETUNE: usize = 1;
const WARP: usize = 2;
const MIX: usize = 3;
const VOICES: usize = 4;
const SHAPE: usize = 5;
const WIDTH: usize = 6;
const PHASE: usize = 7;
const DRIFT: usize = 8;

/// How the saws sit in the stack: how far each is detuned, in semitones, what its stack
/// multiplies its pitch by, its gain, and where sst's pan law puts it, on `-1..1`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Layout {
    semitones: [f32; MAX_SAWS],
    ratio: [f32; MAX_SAWS],
    gain: [f32; MAX_SAWS],
    pan: [f32; MAX_SAWS],
}

/// The stack of `count` saws. The detunings are sst's, `2v/(N-1) - 1`, each then raised to
/// TetraOP's power, `1 + 3w` for a `warp` above the middle, which gathers the saws in the middle,
/// and `1/(1 - 3w)` below it, which pushes them out, its sign kept. In `Unison` the middle saw, or
/// the middle two, play at full and the rest at `mix`, as TetraOP's do; in `Fifths` and `Sub` it's
/// every other saw, those a fifth up or an octave down, that plays at `mix`, so `Mix` sets how
/// loud the stack's second pitch is. Either way their powers add up to one.
fn layout(count: usize, cents: f32, warp: f32, mode: usize, mix: f32) -> Layout {
    let mut layout = Layout {
        semitones: [0.0; MAX_SAWS],
        ratio: [1.0; MAX_SAWS],
        gain: [0.0; MAX_SAWS],
        pan: [0.0; MAX_SAWS],
    };
    let count = count.clamp(1, MAX_SAWS);
    if count == 1 {
        layout.gain[0] = 1.0;
        return layout;
    }
    let exponent = if warp > 0.0 {
        1.0 + 3.0 * warp
    } else {
        1.0 / (1.0 - 3.0 * warp)
    };
    let middle = (count - 1) as f32 * 0.5;
    let half = count / 2;
    let odd = count % 2 == 1;
    for saw in 0..count {
        let position = 2.0 * saw as f32 / (count - 1) as f32 - 1.0;
        let warped = position.abs().powf(exponent).copysign(position);
        layout.semitones[saw] = warped * cents / 100.0;
        if saw % 2 == 1 {
            layout.ratio[saw] = match mode {
                1 => 1.5,
                2 => 0.5,
                _ => 1.0,
            };
        }
        let centre = if mode == 0 {
            (saw as f32 - middle).abs() < 1.0
        } else {
            saw % 2 == 0
        };
        layout.gain[saw] = if centre || count == 2 { 1.0 } else { mix };
        // sst's `panLaw`: out from the middle, every other saw to the other side.
        let mut pan = (saw as f32 - middle).abs() / middle;
        if odd && saw >= half {
            pan = -pan;
        }
        if saw % 2 == 1 {
            pan = -pan;
        }
        layout.pan[saw] = pan;
    }
    let power: f32 = layout.gain.iter().map(|gain| gain * gain).sum();
    let norm = 1.0 / power.sqrt();
    for gain in &mut layout.gain {
        *gain *= norm;
    }
    layout
}

/// The numbers the lanes use, made once when the program is built: made with `splat` in the
/// loop, some came out as a call to fill memory each frame.
const ONE: f32x4 = f32x4::ONE;
const HALF: f32x4 = f32x4::splat(0.5);
const TWO: f32x4 = f32x4::splat(2.0);

#[inline(always)]
fn this_blep(t: f32x4) -> f32x4 {
    HALF * t * t
}

#[inline(always)]
fn next_blep(t: f32x4) -> f32x4 {
    let t = ONE - t;
    -(HALF * t * t)
}

/// Four saws side by side: where each is in its cycle, and the part of its next frame its last
/// edge left.
#[derive(Debug, Clone, Copy)]
struct Stack {
    phase: f32x4,
    next: f32x4,
}

impl Stack {
    fn new() -> Self {
        Self {
            phase: f32x4::ZERO,
            next: f32x4::ZERO,
        }
    }
}

/// What a block plays: each lane's frequency and its gains on the left and the right, where they
/// start and how far they move each frame, and the shape's, as `Moving` has them.
#[derive(Debug, Clone, Copy)]
struct Plan {
    frequency: [f32x4; STACKS],
    frequency_step: [f32x4; STACKS],
    left: [f32x4; STACKS],
    left_step: [f32x4; STACKS],
    right: [f32x4; STACKS],
    right_step: [f32x4; STACKS],
    shape: f32,
    shape_step: f32,
    /// Whether a stack has anything to play this block.
    active: [bool; STACKS],
}

/// A saw's level at a phase, at a `shape` from saw to square: a ramp from 0 to 1, and the square,
/// 0 then `SQUARE_GAIN` from half way.
#[inline]
fn naive(phase: f32x4, shape: f32) -> f32x4 {
    let square = phase.simd_ge(HALF) & f32x4::splat(shape * SQUARE_GAIN);
    f32x4::splat(1.0 - shape) * phase + square
}

/// Play a block, four saws at a time, with only the stacks that have something to play.
fn play(stacks: &mut [Stack; STACKS], plan: &Plan, out: &mut [[f32; 2]; BLOCK]) {
    match plan.active {
        [true, true] => play_stacks::<2>(stacks, 0, plan, out),
        [true, false] => play_stacks::<1>(stacks, 0, plan, out),
        [false, true] => play_stacks::<1>(stacks, 1, plan, out),
        [false, false] => *out = [[0.0; 2]; BLOCK],
    }
}

/// Play `N` stacks from `first`, held in registers across the block. A saw and a square both
/// fall at the end of the cycle, by `1 - shape` and by the square's height, so one polyBLEP state
/// carries the blend, which also rises by the square's height half way.
#[inline(always)]
fn play_stacks<const N: usize>(
    stacks: &mut [Stack; STACKS],
    first: usize,
    plan: &Plan,
    out: &mut [[f32; 2]; BLOCK],
) {
    let pick =
        |values: &[f32x4; STACKS]| -> [f32x4; N] { std::array::from_fn(|at| values[first + at]) };
    let mut phase: [f32x4; N] = std::array::from_fn(|at| stacks[first + at].phase);
    let mut next: [f32x4; N] = std::array::from_fn(|at| stacks[first + at].next);
    let (mut frequency, frequency_step) = (pick(&plan.frequency), pick(&plan.frequency_step));
    let (mut left, left_step) = (pick(&plan.left), pick(&plan.left_step));
    let (mut right, right_step) = (pick(&plan.right), pick(&plan.right_step));
    let mut shape = plan.shape;
    for frame in out.iter_mut() {
        shape += plan.shape_step;
        let rise = f32x4::splat(shape * SQUARE_GAIN);
        let fall = f32x4::splat(1.0 - shape + shape * SQUARE_GAIN);
        let saw = f32x4::splat(1.0 - shape);
        let mut sum_left = f32x4::ZERO;
        let mut sum_right = f32x4::ZERO;
        for at in 0..N {
            frequency[at] += frequency_step[at];
            left[at] += left_step[at];
            right[at] += right_step[at];
            let f = frequency[at];
            let mut this = next[at];
            let mut following = f32x4::ZERO;
            phase[at] += f;
            let wrapped = phase[at].simd_ge(ONE);
            phase[at] -= wrapped & ONE;
            let inverse = ONE / f;
            let t = phase[at] * inverse;
            this -= wrapped & (fall * this_blep(t));
            following -= wrapped & (fall * next_blep(t));
            let high = phase[at].simd_ge(HALF);
            let rose = high & (phase[at] - f).simd_lt(HALF);
            let t = (phase[at] - HALF) * inverse;
            this += rose & (rise * this_blep(t));
            following += rose & (rise * next_blep(t));
            following += saw * phase[at] + (high & rise);
            next[at] = following;
            let sample = TWO * this - fall;
            sum_left += sample * left[at];
            sum_right += sample * right[at];
        }
        *frame = [sum_left.reduce_add(), sum_right.reduce_add()];
    }
    for at in 0..N {
        stacks[first + at] = Stack {
            phase: phase[at],
            next: next[at],
        };
    }
}

/// One voice of it. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Supersaw {
    stacks: [Stack; STACKS],
    drift: Drift,
    /// Each saw's own drift in tuning, on `-1..1`.
    tuning: [f32; MAX_SAWS],
    /// The stack as the settings it's made of last laid it out, and each saw's pitch from it, as
    /// a ratio to the note's.
    key: (usize, f32, f32, usize, f32),
    layout: Layout,
    pitch: [f32; MAX_SAWS],
    /// Where each saw's frequency, gain and pan and the shape were left after the last block.
    frequency: [f32; MAX_SAWS],
    gain: [f32; MAX_SAWS],
    pan: [[f32; 2]; MAX_SAWS],
    shape: f32,
    /// How far each saw has come in, from 0 to 1, as Surge's `playingramp`, and how far that
    /// moves in a block.
    playing: [f32; MAX_SAWS],
    ramp: f32,
    output: [[f32; 2]; BLOCK],
    at: usize,
    fresh: bool,
}

impl Supersaw {
    pub fn new(rate: u32) -> Self {
        Self {
            stacks: [Stack::new(); STACKS],
            drift: Drift::new(),
            tuning: [0.0; MAX_SAWS],
            key: (0, 0.0, 0.0, 0, 0.0),
            layout: layout(1, 0.0, 0.0, 0, 0.0),
            pitch: [1.0; MAX_SAWS],
            frequency: [0.0; MAX_SAWS],
            gain: [0.0; MAX_SAWS],
            pan: [[1.0; 2]; MAX_SAWS],
            shape: 0.0,
            playing: [0.0; MAX_SAWS],
            ramp: BLOCK as f32 * 44_100.0 / (RAMP_FRAMES * rate.max(1) as f32),
            output: [[0.0; 2]; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Roll a note's drift and where its saws start from its seed, so each note differs but a
    /// render repeats. `Phase` as the note starts says how far into their cycles they may start: all
    /// at the top at `00`, anywhere at `FF`.
    pub fn seed(&mut self, seed: u32, settings: Settings) {
        self.drift.seed(seed);
        for tuning in &mut self.tuning {
            *tuning = self.drift.tuning();
        }
        let spread = settings.at(PHASE).clamp(0.0, 1.0);
        let shape = settings.at(SHAPE).clamp(0.0, 1.0);
        for stack in &mut self.stacks {
            let phase = f32x4::new(std::array::from_fn(|_| spread * self.drift.phase()));
            stack.phase = phase;
            stack.next = naive(phase, shape);
        }
    }

    /// What its note's envelope times are multiplied by.
    pub fn time(&self, settings: Settings) -> f32 {
        self.drift.time(settings.at(DRIFT))
    }

    /// The next stereo frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> [f32; 2] {
        if self.at == BLOCK {
            let plan = self.plan(frequency, settings);
            play(&mut self.stacks, &plan, &mut self.output);
            self.at = 0;
        }
        let frame = self.output[self.at];
        self.at += 1;
        frame
    }

    /// Work out the block's targets from the settings, and leave them as where the next block
    /// starts.
    fn plan(&mut self, frequency: f32, settings: Settings) -> Plan {
        let count = (settings.at(VOICES) * 255.0).round().clamp(0.0, 7.0) as usize + 1;
        let detune = settings.at(DETUNE).clamp(0.0, 1.0);
        let warp = settings.at(WARP).clamp(0.0, 1.0) * 2.0 - 1.0;
        let mode = ((settings.at(MODE) * 255.0).round() as usize).min(MODES - 1);
        let mix = settings.at(MIX).clamp(0.0, 1.0);
        let shape = settings.at(SHAPE).clamp(0.0, 1.0);
        let width = settings.at(WIDTH).clamp(0.0, 1.0);
        let drift = settings.at(DRIFT).clamp(0.0, 1.0);
        self.drift.step();
        // The layout and its pitches are only worked out again when what they're made of moves.
        let key = (count, detune, warp, mode, mix);
        if key != self.key {
            self.key = key;
            self.layout = layout(count, MAX_DETUNE_CENTS * detune * detune, warp, mode, mix);
            let stack = self.layout;
            self.pitch =
                std::array::from_fn(|saw| stack.ratio[saw] * (stack.semitones[saw] / 12.0).exp2());
        }
        let stack = self.layout;
        let level = if drift > 0.0 {
            LEVEL * self.drift.level(drift)
        } else {
            LEVEL
        };
        let frequency = frequency.clamp(1e-5, 0.49);

        // A saw going out keeps its pitch, gain and pan as it fades.
        let mut target_frequency = self.frequency;
        let mut target_gain = self.gain;
        let mut target_pan = self.pan;
        for saw in 0..count {
            let mut pitch = self.pitch[saw];
            if drift > 0.0 {
                pitch *= (self.drift.semitones(self.tuning[saw], drift) / 12.0).exp2();
            }
            target_frequency[saw] = (frequency * pitch).min(MAX_FREQUENCY);
            target_gain[saw] = stack.gain[saw] * level;
            let pan = width * stack.pan[saw];
            target_pan[saw] = [1.0 - pan, 1.0 + pan];
        }
        let wanted: [f32; MAX_SAWS] = std::array::from_fn(|saw| f32::from(saw < count));
        if self.fresh {
            self.frequency = target_frequency;
            self.gain = target_gain;
            self.pan = target_pan;
            self.shape = shape;
            self.playing = wanted;
            self.fresh = false;
        }
        // A saw coming in or going moves only so far a block; the rest move across it.
        let mut playing = self.playing;
        for (playing, wanted) in playing.iter_mut().zip(wanted) {
            *playing += (wanted - *playing).clamp(-self.ramp, self.ramp);
        }
        let from: [f32; MAX_SAWS] = std::array::from_fn(|saw| self.gain[saw] * self.playing[saw]);
        let to: [f32; MAX_SAWS] = std::array::from_fn(|saw| target_gain[saw] * playing[saw]);

        let lanes = |values: [f32; MAX_SAWS]| -> [f32x4; STACKS] {
            std::array::from_fn(|at| {
                f32x4::new(std::array::from_fn(|lane| values[at * LANES + lane]))
            })
        };
        let side = |gains: [f32; MAX_SAWS], pans: [[f32; 2]; MAX_SAWS], side: usize| {
            lanes(std::array::from_fn(|saw| gains[saw] * pans[saw][side]))
        };
        let moving = |from: [f32x4; STACKS], to: [f32x4; STACKS]| -> [f32x4; STACKS] {
            std::array::from_fn(|at| (to[at] - from[at]) / f32x4::splat(BLOCK as f32))
        };
        let (frequency_from, frequency_to) = (lanes(self.frequency), lanes(target_frequency));
        let (left_from, left_to) = (side(from, self.pan, 0), side(to, target_pan, 0));
        let (right_from, right_to) = (side(from, self.pan, 1), side(to, target_pan, 1));
        let silent = |gains: [f32x4; STACKS], at: usize| gains[at] == f32x4::ZERO;
        let plan = Plan {
            frequency: frequency_from,
            frequency_step: moving(frequency_from, frequency_to),
            left: left_from,
            left_step: moving(left_from, left_to),
            right: right_from,
            right_step: moving(right_from, right_to),
            shape: self.shape,
            shape_step: (shape - self.shape) / BLOCK as f32,
            active: std::array::from_fn(|at| {
                !(silent(left_from, at)
                    && silent(left_to, at)
                    && silent(right_from, at)
                    && silent(right_to, at))
            }),
        };
        self.frequency = target_frequency;
        self.gain = target_gain;
        self.playing = playing;
        self.pan = target_pan;
        self.shape = shape;
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::swarm::Swarm;
    use crate::dsp::virtual_analog::{BLOCK, VirtualAnalog};
    use vt_model::settings::MAX_MACHINE_SETTINGS;

    const RATE: u32 = 48_000;

    /// The settings at their defaults: seven saws a little apart, all as loud, half wide, starting
    /// anywhere.
    fn plain() -> Settings {
        settings(&[
            (VOICES, 6.0 / 255.0),
            (DETUNE, 0.375),
            (WARP, 0.5),
            (MIX, 1.0),
        ])
        .with(WIDTH, 0.5)
        .with(PHASE, 1.0)
    }

    fn settings(places: &[(usize, f32)]) -> Settings {
        let mut all = [0.0; MAX_MACHINE_SETTINGS];
        for (at, value) in places {
            all[*at] = *value;
        }
        Settings::from_places(all)
    }

    trait With {
        fn with(self, at: usize, value: f32) -> Self;
    }

    impl With for Settings {
        fn with(self, at: usize, value: f32) -> Self {
            let mut all: [f32; MAX_MACHINE_SETTINGS] = std::array::from_fn(|place| self.at(place));
            all[at] = value;
            Settings::from_places(all)
        }
    }

    fn count(saws: usize) -> f32 {
        (saws - 1) as f32 / 255.0
    }

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<[f32; 2]> {
        let mut voice = Supersaw::new(RATE);
        voice.seed(7, settings);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn mono(frames: &[[f32; 2]]) -> Vec<f32> {
        frames.iter().map(|[l, r]| 0.5 * (l + r)).collect()
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// The same block worked out a saw at a time, the reference the lanes are checked against.
    fn play_one_at_a_time(stacks: &mut [Stack; STACKS], plan: &Plan, out: &mut [[f32; 2]; BLOCK]) {
        let saws = |lanes: [f32x4; STACKS]| -> [f32; MAX_SAWS] {
            std::array::from_fn(|saw| lanes[saw / LANES].to_array()[saw % LANES])
        };
        let mut phase = saws(stacks.map(|stack| stack.phase));
        let mut next = saws(stacks.map(|stack| stack.next));
        let (mut frequency, frequency_step) = (saws(plan.frequency), saws(plan.frequency_step));
        let (mut left, left_step) = (saws(plan.left), saws(plan.left_step));
        let (mut right, right_step) = (saws(plan.right), saws(plan.right_step));
        let mut shape = plan.shape;
        for frame in out.iter_mut() {
            shape += plan.shape_step;
            let rise = shape * SQUARE_GAIN;
            let fall = 1.0 - shape + shape * SQUARE_GAIN;
            let mut sum_left = [0.0_f32; LANES];
            let mut sum_right = [0.0_f32; LANES];
            for saw in 0..MAX_SAWS {
                if !plan.active[saw / LANES] {
                    continue;
                }
                frequency[saw] += frequency_step[saw];
                left[saw] += left_step[saw];
                right[saw] += right_step[saw];
                let f = frequency[saw];
                let mut this = next[saw];
                let mut following = 0.0;
                phase[saw] += f;
                let inverse = 1.0 / f;
                if phase[saw] >= 1.0 {
                    phase[saw] -= 1.0;
                    let t = phase[saw] * inverse;
                    this -= fall * (0.5 * t * t);
                    following -= fall * (-0.5 * (1.0 - t) * (1.0 - t));
                }
                let high = phase[saw] >= 0.5;
                if high && phase[saw] - f < 0.5 {
                    let t = (phase[saw] - 0.5) * inverse;
                    this += rise * (0.5 * t * t);
                    following += rise * (-0.5 * (1.0 - t) * (1.0 - t));
                }
                following += (1.0 - shape) * phase[saw] + if high { rise } else { 0.0 };
                next[saw] = following;
                let sample = 2.0 * this - fall;
                sum_left[saw % LANES] += sample * left[saw];
                sum_right[saw % LANES] += sample * right[saw];
            }
            *frame = [sum_left.iter().sum(), sum_right.iter().sum()];
        }
        for (at, stack) in stacks.iter_mut().enumerate() {
            stack.phase = f32x4::new(std::array::from_fn(|lane| phase[at * LANES + lane]));
            stack.next = f32x4::new(std::array::from_fn(|lane| next[at * LANES + lane]));
        }
    }

    #[test]
    fn the_lanes_play_what_one_saw_at_a_time_does() {
        // Every count, both shapes and between, with the count, shape and pitch moving.
        let mut lanes = Supersaw::new(RATE);
        lanes.seed(3, plain());
        let mut reference = lanes;
        let mut worst = 0.0_f32;
        for block in 0..3_000 {
            let along = block as f32 / 3_000.0;
            let settings = plain()
                .with(VOICES, count(1 + block / 40 % 8))
                .with(SHAPE, (along * 7.0).fract())
                .with(WIDTH, along);
            let hertz = 40.0 + 9_000.0 * along * along;
            let frequency = hertz / RATE as f32;
            let plan = lanes.plan(frequency, settings);
            assert_eq!(reference.plan(frequency, settings).active, plan.active);
            let (mut a, mut b) = ([[0.0; 2]; BLOCK], [[0.0; 2]; BLOCK]);
            play(&mut lanes.stacks, &plan, &mut a);
            play_one_at_a_time(&mut reference.stacks, &plan, &mut b);
            for (a, b) in a.iter().zip(&b) {
                worst = worst.max((a[0] - b[0]).abs()).max((a[1] - b[1]).abs());
            }
        }
        assert!(worst < 1e-6, "{worst}");
    }

    #[test]
    fn the_stack_is_spread_evenly_and_as_loud_as_one_saw() {
        for saws in 1..=MAX_SAWS {
            let stack = layout(saws, 100.0, 0.0, 0, 1.0);
            let power: f32 = stack.gain.iter().map(|gain| gain * gain).sum();
            assert!((power - 1.0).abs() < 1e-5, "{saws}: {power}");
            for saw in 0..saws {
                assert!((stack.gain[saw] - 1.0 / (saws as f32).sqrt()).abs() < 1e-6);
                if saws > 1 {
                    let even = 2.0 * saw as f32 / (saws - 1) as f32 - 1.0;
                    assert!((stack.semitones[saw] - even).abs() < 1e-5, "{saws} {saw}");
                }
            }
            assert!(stack.gain[saws..].iter().all(|gain| *gain == 0.0));
        }
        // Alternate saws go to alternate sides, the outermost the furthest.
        let pans = layout(7, 100.0, 0.0, 0, 1.0).pan;
        assert_eq!(
            pans[..7],
            [1.0, -2.0 / 3.0, 1.0 / 3.0, 0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0]
        );
        for saws in [7, 8] {
            let sides: f32 = layout(saws, 100.0, 0.0, 0, 1.0).pan.iter().sum();
            assert!(sides.abs() < 1e-6, "{sides}");
        }
    }

    #[test]
    fn warp_gathers_or_spreads_the_saws_and_keeps_the_outermost() {
        let linear = layout(7, 100.0, 0.0, 0, 1.0).semitones;
        let gathered = layout(7, 100.0, 1.0, 0, 1.0).semitones;
        let spread = layout(7, 100.0, -1.0, 0, 1.0).semitones;
        for saw in [1, 2] {
            assert!(gathered[saw].abs() < linear[saw].abs());
            assert!(spread[saw].abs() > linear[saw].abs());
        }
        for stack in [gathered, spread] {
            assert_eq!(stack[0], -1.0);
            assert_eq!(stack[6], 1.0);
            assert_eq!(stack[3], 0.0);
        }
    }

    #[test]
    fn fifths_and_sub_take_every_other_saw_away_and_mix_sets_how_loud() {
        let fifths = layout(6, 0.0, 0.0, 1, 0.5);
        let sub = layout(6, 0.0, 0.0, 2, 0.5);
        for saw in 0..6 {
            let (up, down) = if saw % 2 == 1 { (1.5, 0.5) } else { (1.0, 1.0) };
            assert_eq!(fifths.ratio[saw], up);
            assert_eq!(sub.ratio[saw], down);
            let share = if saw % 2 == 1 { 0.5 } else { 1.0 };
            assert!((sub.gain[saw] / sub.gain[0] - share).abs() < 1e-6);
        }
        // In unison, Mix turns the saws round the middle one down.
        let centred = layout(7, 0.0, 0.0, 0, 0.0);
        assert_eq!(centred.gain[..7], [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        let pair = layout(8, 0.0, 0.0, 0, 0.0);
        assert!(pair.gain[3] > 0.7 && pair.gain[4] > 0.7 && pair.gain[0] == 0.0);
    }

    #[test]
    fn it_is_about_as_loud_as_the_virtual_analog_voice() {
        let stack = energy(&mono(&run(220.0, plain(), 48_000)));
        let mut va = VirtualAnalog::new();
        let va: Vec<f32> = (0..48_000)
            .map(|_| va.next(220.0 / RATE as f32, Settings::default()))
            .collect();
        let decibels = 10.0 * (stack / energy(&va)).log10();
        assert!(decibels.abs() < 3.0, "{decibels} dB");
    }

    /// The frequency from upward crossings of the middle, past the start.
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
    fn undetuned_it_plays_the_note_and_sub_plays_an_octave_under_it() {
        let together = plain().with(DETUNE, 0.0).with(PHASE, 0.0);
        let pitch = measured_pitch(&mono(&run(110.0, together, 48_000)));
        assert!((pitch - 110.0).abs() < 0.5, "{pitch}");
        // Sub adds the octave under the note, which plain unison hasn't.
        let at = |settings: Settings, hertz: f32| {
            let samples = mono(&run(110.0, settings, 48_000));
            let turn = std::f32::consts::TAU * hertz / RATE as f32;
            let (sin, cos) = samples
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(s, c), (at, x)| {
                    (
                        s + x * (turn * at as f32).sin(),
                        c + x * (turn * at as f32).cos(),
                    )
                });
            (sin * sin + cos * cos).sqrt() / samples.len() as f32
        };
        let sub = together.with(MODE, 2.0 / 255.0);
        assert!(at(sub, 55.0) > 0.05, "{}", at(sub, 55.0));
        assert!(at(together, 55.0) < 0.001, "{}", at(together, 55.0));
        assert!(at(sub, 110.0) > 0.05);
    }

    #[test]
    fn at_no_width_both_sides_are_the_same_and_wide_they_part_but_sum_as_mono() {
        let narrow = run(220.0, plain().with(WIDTH, 0.0), 9_600);
        assert!(narrow.iter().all(|[l, r]| l.to_bits() == r.to_bits()));
        let wide = run(220.0, plain().with(WIDTH, 1.0), 9_600);
        let apart: Vec<f32> = wide.iter().map(|[l, r]| l - r).collect();
        assert!(energy(&apart) > 0.1 * energy(&mono(&wide)));
        for (narrow, wide) in narrow.iter().zip(&wide) {
            assert!((narrow[0] - 0.5 * (wide[0] + wide[1])).abs() < 1e-5);
        }
    }

    #[test]
    fn a_saw_comes_in_or_goes_over_about_fifty_frames() {
        let mut voice = Supersaw::new(44_100);
        let one = plain().with(VOICES, count(1)).with(PHASE, 0.0);
        let eight = one.with(VOICES, count(8));
        voice.plan(0.01, one);
        let mut frames = 0;
        while voice.playing[7] < 1.0 {
            voice.plan(0.01, eight);
            frames += BLOCK;
        }
        assert!((48..=64).contains(&frames), "{frames}");
        let mut frames = 0;
        while voice.playing[7] > 0.0 {
            voice.plan(0.01, one);
            frames += BLOCK;
        }
        assert!((48..=64).contains(&frames), "{frames}");
        // It goes on with only the first four lanes once the others have gone.
        assert_eq!(voice.plan(0.01, one).active, [true, false]);
    }

    #[test]
    fn the_same_seed_renders_the_same_and_phase_none_starts_every_saw_at_the_top() {
        assert_eq!(run(220.0, plain(), 4_800), run(220.0, plain(), 4_800));
        let mut other = Supersaw::new(RATE);
        other.seed(8, plain());
        let other: Vec<[f32; 2]> = (0..4_800)
            .map(|_| other.next(220.0 / RATE as f32, plain()))
            .collect();
        assert_ne!(run(220.0, plain(), 4_800), other);
        let together = plain().with(PHASE, 0.0);
        let mut seeded = Supersaw::new(RATE);
        seeded.seed(8, together);
        let seeded: Vec<[f32; 2]> = (0..4_800)
            .map(|_| seeded.next(220.0 / RATE as f32, together))
            .collect();
        assert_eq!(run(220.0, together, 4_800), seeded);
    }

    #[test]
    fn drift_tunes_each_saw_its_own_way() {
        let still = plain().with(DETUNE, 0.0).with(PHASE, 0.0);
        let drifting = still.with(DRIFT, 1.0);
        let one = energy(&mono(&run(1_760.0, still, 48_000)));
        let mut voice = Supersaw::new(RATE);
        voice.seed(1, drifting);
        let apart: Vec<f32> = mono(
            &(0..48_000)
                .map(|_| voice.next(1_760.0 / RATE as f32, drifting))
                .collect::<Vec<_>>(),
        );
        // In tune, seven like saws add up; drifting apart, they beat and come out quieter.
        assert!(energy(&apart) < 0.5 * one, "{} {one}", energy(&apart));
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for saws in [1, 4, 5, 8] {
                for mode in 0..3 {
                    for value in [0.0, 0.5, 1.0] {
                        let settings = Settings::from_places(std::array::from_fn(|_| value))
                            .with(VOICES, count(saws))
                            .with(MODE, mode as f32 / 255.0);
                        for [l, r] in run(hertz, settings, 2_400) {
                            assert!(
                                l.is_finite() && r.is_finite() && l.abs() < 2.0 && r.abs() < 2.0,
                                "{l} {r} at {hertz} {settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// How long each takes to play a frame at 48 kHz, in ns: the stack of eight in its lanes, the
    /// same eight one saw at a time, four in its lanes, `Swarm`'s eight saws and `VA`. Each is the
    /// fastest of many short runs, so a busy machine's other work counts as little as it can. Run
    /// with `cargo test --release -p vt-engine supersaw::tests::cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cost() {
        use std::hint::black_box;
        use std::time::Instant;
        const FRAMES: usize = 9_600;
        const RUNS: usize = 200;
        fn time(name: &str, mut play: impl FnMut() -> f32) {
            let mut fastest = f64::MAX;
            for _ in 0..RUNS {
                let started = Instant::now();
                let mut sum = 0.0;
                for _ in 0..FRAMES {
                    sum += play();
                }
                black_box(sum);
                fastest = fastest.min(started.elapsed().as_nanos() as f64 / FRAMES as f64);
            }
            println!("{name}: {fastest:.1} ns a frame");
        }
        let eight = plain().with(VOICES, count(8));
        let four = eight.with(VOICES, count(4));
        let one = eight.with(VOICES, count(1));
        let mut lanes = Supersaw::new(RATE);
        lanes.seed(1, eight);
        time("Supersaw, 8 saws in lanes", || {
            lanes.next(black_box(0.004_6), eight)[0]
        });
        let mut one_at_a_time = Supersaw::new(RATE);
        one_at_a_time.seed(1, eight);
        let mut block = [[0.0; 2]; BLOCK];
        let mut at = BLOCK;
        time("Supersaw, 8 saws one at a time", || {
            if at == BLOCK {
                let plan = one_at_a_time.plan(black_box(0.004_6), eight);
                play_one_at_a_time(&mut one_at_a_time.stacks, &plan, &mut block);
                at = 0;
            }
            at += 1;
            block[at - 1][0]
        });
        for (name, settings) in [("4 saws", four), ("1 saw", one)] {
            let mut lanes = Supersaw::new(RATE);
            lanes.seed(1, settings);
            time(&format!("Supersaw, {name} in lanes"), || {
                lanes.next(black_box(0.004_6), settings)[0]
            });
        }
        let mut swarm = Swarm::new();
        swarm.seed(1);
        time("Swarm", || {
            swarm.next(black_box(0.004_6), Settings::default())
        });
        let mut va = VirtualAnalog::new();
        time("VA", || va.next(black_box(0.004_6), Settings::default()));
    }
}
