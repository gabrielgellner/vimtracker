//! A comb tuned to the track's note, after Vital's comb filter. What comes in goes into a line a
//! cycle of the note long, and what comes back out is fed in again through a band of the loop's
//! own, so it rings at the note and its harmonics. As a comb, it's heard as what goes into the
//! line. As a flanger, it's heard as the sound with what comes back out of the line, so feedback
//! sharpens the flanger's comb into peaks on the harmonics. Negative feedback reads half as far
//! back and turns what it reads over, so its peaks sit on the note's odd harmonics alone, as
//! Vital's `Flange-` has them, for a hollower ring at the same pitch.
//!
//! The band in the loop is Vital's: a one-pole low-pass and a one-pole high-pass, turned up so its
//! peak passes at unity. Vital's comb has two styles of band, a blend of low-pass and high-pass at
//! a cutoff, and a band spread around a centre. Here `Tone` sets the low-pass and `Spread` brings
//! the high-pass up under it, which covers Vital's band spread and its low-pass and high-pass ends.
//! Vital hard-clips what's fed back, so nothing runs away at full feedback, and so does this.
//!
//! Vital reads its line a cycle back, so the band's delay pulls its tuning off. As Rings' string
//! does, the line here is read that much the shorter, so the loop's peaks sit on the note's
//! harmonics. Rings takes its damping filter's delay at the note from a table. Here the band's is
//! worked out where each harmonic's pole is, a little inside the circle as it decays, weighed by
//! how long each one rings, and so is the delay of reading between samples. Rings is by Emilie
//! Gillet, MIT, Copyright 2015 Emilie Gillet, and only its way of tuning is followed here, not its
//! code.
//!
//! With feedback positive, nothing at all, which doesn't move, is one of the loop's peaks too, and
//! what's well under the note builds up slowly however it's high-passed within the loop, as
//! Vital's does. So what goes into the line is high-passed first, outside the loop, where it can't
//! pull the tuning, and the high-pass within the loop is kept low.
//!
//! Adapted from Vital's `synthesis/filters/comb_filter.cpp` and `comb_filter.h`, with its
//! Catmull-Rom reads from `synthesis/lookups/memory.h`, GPL 3 or later:
//!
//! Copyright 2013-2019 Matt Tytel.
//!
//! Vital is free software: you can redistribute it and/or modify it under the terms of the GNU
//! General Public License as published by the Free Software Foundation, either version 3 of the
//! License, or (at your option) any later version.

use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

use vt_model::insert::{Comb as Settings, CombMode, from_middle, shift_semitones};

use crate::dsp::resonator::{Limiter, semitones};
use crate::dsp::smoother::Smoothed;

/// How many frames it plays between workings-out of its loop, about a millisecond and a half at
/// 48 kHz.
const BLOCK: usize = 64;
/// The lowest and highest it's tuned to, and the fewest frames a cycle can take, so a half cycle
/// is still long enough to read between samples with the band's delay taken off.
const LOWEST_HZ: f32 = 16.0;
const HIGHEST_HZ: f32 = 12_000.0;
const SHORTEST_CYCLE: f32 = 8.0;
/// The highest the band's edges go, as a fraction of the rate.
const HIGHEST_CUTOFF: f32 = 0.45;
/// How far under the note, in semitones, what goes into the line is cleaned by two high-passes,
/// as a comb and as a flanger. As the loop's peaks sit on every harmonic with the feedback
/// positive, and nothing at all is one of them, sound well under the note builds up slowly as it
/// goes round, however it's high-passed within the loop. Cleaned before, it only reaches the loop
/// much quieter, and what's heard keeps all of it. A flanger's notches need what comes out of the
/// line to match the sound, so it's cleaned less.
const COMB_CLEANING: f32 = -24.0;
const FLANGE_CLEANING: f32 = -48.0;
/// How far above the note `Tone` takes the band's low-pass at `FF`, in semitones, which is far
/// enough to leave everything through.
const HIGHEST_TONE: f32 = 96.0;
/// The lowest the band's high-pass goes, in semitones from the note, so what doesn't move never
/// builds up. Vital's is four octaves down, but a high-pass moves the note's harmonics by
/// different amounts, and so out of tune with each other: by about 17 cents there, and about one
/// here, eight octaves down.
const LOWEST_CUT: f32 = -96.0;
/// The most feedback, as Vital has it: the loop's gain at the band's peak.
const MOST_FEEDBACK: f32 = 1.0;
/// How much of what comes in a flanger puts into its line, as Vital does.
const FLANGE_SCALE: f32 = FRAC_1_SQRT_2;
/// How far below nothing the feedback goes in turning over to read half as far back, so it never
/// clicks.
const POLARITY_SPAN: f32 = 1.0 / 16.0;
/// How long a hit the level is matched for, in seconds, as the resonator's is. A loop ringing for
/// longer gathers a hit up and builds on it.
const HIT_SECONDS: f32 = 0.02;
/// How loud what comes out is, before the limiter, so a drum comes out about as loud as it went
/// in.
const LEVEL: f32 = 1.25;
/// How long it keeps going once nothing comes in and nothing's heard, in seconds. That's longer
/// than its longest cycle, so nothing's still on its way round.
const QUIET_SECONDS: f32 = 0.1;
/// How many frequencies, evenly across the band, the level is worked out from.
const BANDS: usize = 8;
/// The harmonics the tuning is weighed across, each with how many harmonics around it it stands
/// for: every harmonic, and the odd ones alone for negative feedback.
const HARMONICS: [(f32, f32); 12] = [
    (1.0, 1.0),
    (2.0, 1.0),
    (3.0, 1.0),
    (4.0, 1.5),
    (6.0, 2.0),
    (8.0, 3.0),
    (12.0, 4.0),
    (16.0, 6.0),
    (24.0, 8.0),
    (32.0, 12.0),
    (48.0, 16.0),
    (64.0, 16.0),
];
const ODD_HARMONICS: [(f32, f32); 10] = [
    (1.0, 1.0),
    (3.0, 1.0),
    (5.0, 1.0),
    (7.0, 1.5),
    (11.0, 2.0),
    (15.0, 3.0),
    (23.0, 4.0),
    (31.0, 6.0),
    (47.0, 8.0),
    (63.0, 8.0),
];

/// Catmull-Rom's weights for the four frames around a read, the newest first, at a fraction of
/// the way from the second to the third.
#[inline]
fn weights(between: f32) -> [f32; 4] {
    let t = between;
    [
        t * (-0.5 + t * (1.0 - 0.5 * t)),
        1.0 + t * t * (-2.5 + 1.5 * t),
        t * (0.5 + t * (2.0 - 1.5 * t)),
        t * t * (-0.5 + 0.5 * t),
    ]
}

/// How far back to read, in frames, for a read between samples to be `delay` frames late at
/// `omega` radians a frame. A cubic read is a little late or early between samples, by an amount
/// that changes with frequency, so the read is moved by however far off it is, twice over.
fn read_for(delay: f32, omega: f32) -> f32 {
    let (sin, cos) = omega.sin_cos();
    let (sin_2, cos_2) = (2.0 * omega).sin_cos();
    let mut read = delay;
    for _ in 0..2 {
        let whole = read.floor();
        let [newer, current, older, oldest] = weights(read - whole);
        // The newest frame is one less late than the current, and the older two more.
        let real = (newer + older) * cos + current + oldest * cos_2;
        let imaginary = (newer - older) * sin - oldest * sin_2;
        let late = whole - imaginary.atan2(real) / omega;
        read += delay - late;
    }
    read
}

/// The first `count` powers of `x`, added up, given `x` to that power.
#[inline]
fn geometric(x: f32, power: f32, count: f32) -> f32 {
    if x > 0.9999 {
        count
    } else {
        (1.0 - power) / (1.0 - x)
    }
}

/// A trapezoidal one-pole low-pass, with its gain `g / (1 + g)`.
#[inline]
fn low_pass(state: &mut f32, input: f32, gain: f32) -> f32 {
    let step = (input - *state) * gain;
    let out = step + *state;
    *state = out + step;
    out
}

/// How much the band in the loop passes at `tan(pi f)`, from the trapezoidal one-poles' `g`s.
#[inline]
fn band_gain(warped: f32, low: f32, high: f32, boost: f32) -> f32 {
    let (above_low, below_high) = (warped / low, high / warped);
    boost / ((1.0 + above_low * above_low) * (1.0 + below_high * below_high)).sqrt()
}

/// How much the band passes and how far it moves the phase, in radians, at a frequency whose
/// sine and cosine a frame are given, and `radius` inside the circle, where a decaying pole is.
fn band_response([sin, cos]: [f32; 2], radius: f32, low: f32, high: f32, boost: f32) -> (f32, f32) {
    // A frame's delay there, and its square.
    let (real, imaginary) = (cos / radius, -sin / radius);
    let (square_real, square_imaginary) =
        (real * real - imaginary * imaginary, 2.0 * real * imaginary);
    // The one-poles are `g (1 + d) / ((1 + g) + (g - 1) d)` and `(1 - d) / ((1 + g) + (g - 1) d)`,
    // so together they're `g (1 - d^2)` over the two denominators.
    let (low_real, low_imaginary) = (1.0 + low + (low - 1.0) * real, (low - 1.0) * imaginary);
    let (high_real, high_imaginary) = (1.0 + high + (high - 1.0) * real, (high - 1.0) * imaginary);
    let under_real = low_real * high_real - low_imaginary * high_imaginary;
    let under_imaginary = low_real * high_imaginary + low_imaginary * high_real;
    let (over_real, over_imaginary) = (1.0 - square_real, -square_imaginary);
    // The top times the bottom turned back, over the bottom's size squared.
    let real = over_real * under_real + over_imaginary * under_imaginary;
    let imaginary = over_imaginary * under_real - over_real * under_imaginary;
    let under = under_real * under_real + under_imaginary * under_imaginary;
    (
        boost * low * real.hypot(imaginary) / under,
        imaginary.atan2(real),
    )
}

/// Where the loop's peak at `omega` radians a frame sits, as how far back to read for it to sit
/// there, with `period` the frames it would take with nothing in the loop, from the band's
/// response there on the circle. The loop's pole is a little inside the circle, as far as it loses
/// each time round, so the band is worked out there: its phase decides how far back to read, and
/// how much it passes decides how far inside the pole is, so the two are worked out in turn until
/// they settle.
fn pole_delay(
    omega: f32,
    turn: [f32; 2],
    on_circle: (f32, f32),
    period: f32,
    feedback: f32,
    [low, high, boost]: [f32; 3],
) -> f32 {
    let (mut gain, mut phase) = on_circle;
    // Ringing long, it's as good as on the circle.
    let turns = if feedback * gain > 0.99 { 0 } else { 3 };
    for _ in 0..turns {
        let delay = period + phase / omega;
        let radius = ((feedback * gain).max(1e-6).ln() / delay).exp().max(0.9);
        (gain, phase) = band_response(turn, radius, low, high, boost);
    }
    period + phase / omega
}

/// What the loop plays with. It moves in a straight line across each block, so nothing steps.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Loop {
    /// How far back it reads, in frames, for feedback either way up.
    rising: f32,
    falling: f32,
    /// How much of what's read is from a cycle back, rather than half of one and turned over.
    polarity: f32,
    /// The band's low-pass and high-pass, as trapezoidal one-poles' gains, and how much it's
    /// turned up so its peak passes at unity.
    low: f32,
    high: f32,
    boost: f32,
    feedback: f32,
    /// How much of what comes in goes into the line, and how much straight out.
    input: f32,
    dry: f32,
    /// The high-passes' gain that what goes into the line is cleaned with first.
    cleaning: f32,
    /// How far it's a flanger rather than a comb, from 0 to 1.
    flange: f32,
    level: f32,
}

const FIELDS: usize = 12;

impl Loop {
    fn fields(self) -> [f32; FIELDS] {
        [
            self.rising,
            self.falling,
            self.polarity,
            self.low,
            self.high,
            self.boost,
            self.feedback,
            self.input,
            self.dry,
            self.cleaning,
            self.flange,
            self.level,
        ]
    }

    fn from_fields(fields: [f32; FIELDS]) -> Self {
        let [
            rising,
            falling,
            polarity,
            low,
            high,
            boost,
            feedback,
            input,
            dry,
            cleaning,
            flange,
            level,
        ] = fields;
        Self {
            rising,
            falling,
            polarity,
            low,
            high,
            boost,
            feedback,
            input,
            dry,
            cleaning,
            flange,
            level,
        }
    }

    /// How far each moves in a frame, to get from here to `to` in a block.
    fn steps_to(self, to: Self) -> Self {
        let (from, to) = (self.fields(), to.fields());
        Self::from_fields(std::array::from_fn(|at| (to[at] - from[at]) / BLOCK as f32))
    }

    fn plus(self, step: Self) -> Self {
        let (value, step) = (self.fields(), step.fields());
        Self::from_fields(std::array::from_fn(|at| value[at] + step[at]))
    }
}

/// What a `Comb` slot keeps, from the effect pool, as its lines are a cycle of the lowest note
/// long.
#[derive(Debug, Clone)]
pub struct Comb {
    lines: [Vec<f32>; 2],
    mask: usize,
    /// Where the newest frame is in the lines.
    write: usize,
    /// The band's low-pass and high-pass, on each side, and the two high-passes cleaning what
    /// goes into the line.
    low: [f32; 2],
    high: [f32; 2],
    cleaned: [[f32; 2]; 2],
    /// The MIDI note it's tuned to, gliding to the track's with `Tune` added.
    note: Smoothed,
    tune: f32,
    /// From -1 to 1, as set, before Vital's curve.
    feedback: Smoothed,
    tone: Smoothed,
    spread: Smoothed,
    flange: Smoothed,
    /// The loop now, how much it moves each frame, and where it's going by the end of the block.
    now: Loop,
    step: Loop,
    target: Loop,
    /// What the loop was last worked out from, so nothing's worked out while nothing moves.
    worked_from: [f32; 5],
    /// `tan(pi f)` at frequencies evenly across the band, for working out how loud it is.
    warped: [f32; BANDS],
    limiter: Limiter,
    /// How many frames of this block are left before the loop is worked out again.
    left: usize,
    /// Whether it's only just started, so it starts at the note and what's set rather than
    /// gliding to them.
    fresh: bool,
    /// The settings last followed, so the first jumps to them.
    settings: Option<Settings>,
    rate: u32,
}

impl Comb {
    /// Lines long enough for a cycle of the lowest note, with the band's delay put back.
    pub fn new(rate: u32) -> Self {
        let longest = 1.25 * rate as f32 / LOWEST_HZ;
        let size = (longest.ceil() as usize + 8).next_power_of_two();
        Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            low: [0.0; 2],
            high: [0.0; 2],
            cleaned: [[0.0; 2]; 2],
            note: Smoothed::at(60.0),
            tune: 0.0,
            feedback: Smoothed::default(),
            tone: Smoothed::default(),
            spread: Smoothed::default(),
            flange: Smoothed::default(),
            now: Loop::default(),
            step: Loop::default(),
            target: Loop::default(),
            worked_from: [f32::NAN; 5],
            warped: std::array::from_fn(|band| {
                (PI * (band as f32 + 0.5) / (2 * BANDS) as f32).tan()
            }),
            limiter: Limiter::new(rate),
            left: 0,
            fresh: true,
            settings: None,
            rate,
        }
    }

    /// Forget everything it was playing, for a fresh start.
    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.low = [0.0; 2];
        self.high = [0.0; 2];
        self.cleaned = [[0.0; 2]; 2];
        self.limiter = Limiter::new(self.rate);
        self.settle();
    }

    /// How many quiet frames in a row mean it's done.
    pub fn quiet_frames(&self) -> u32 {
        (QUIET_SECONDS * self.rate as f32) as u32
    }

    /// Take what's set now, jumping to it the first time.
    pub fn follow(&mut self, settings: Settings) {
        if self.settings == Some(settings) {
            return;
        }
        self.tune = shift_semitones(f32::from(settings.tune));
        self.feedback.target = from_middle(f32::from(settings.feedback));
        self.tone.target = f32::from(settings.tone) / 255.0;
        self.spread.target = f32::from(settings.spread) / 255.0;
        self.flange.target = match settings.mode {
            CombMode::Comb => 0.0,
            CombMode::Flange => 1.0,
        };
        if self.settings.is_none() {
            self.settle();
        }
        self.settings = Some(settings);
    }

    /// Jump to what's set and the next note, rather than gliding, as when it's been resting.
    pub fn settle(&mut self) {
        self.fresh = true;
        self.left = 0;
    }

    /// One frame in, with no note, for the pool's own use.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input, 60.0, 1.0)
    }

    /// A line's sound a number of frames back, read between samples through four of them.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let whole = frames as usize;
        let [newer, current, older, oldest] = weights(frames - whole as f32);
        let line = &self.lines[side];
        // The frame `back` frames ago, a frame back being the newest in the line.
        let at = |back: usize| line[(self.write + 1).wrapping_sub(back) & self.mask];
        newer * at(whole - 1) + current * at(whole) + older * at(whole + 1) + oldest * at(whole + 2)
    }

    /// One frame of the slot's sound through the comb, tuned to `note` and its `Tune` above it,
    /// and the frame out, all wet and within full scale.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], note: f32, smoothing: f32) -> [f32; 2] {
        if self.left == 0 {
            self.work_out(note, smoothing);
            self.left = BLOCK;
        }
        self.left -= 1;
        let now = self.now;
        let write = (self.write + 1) & self.mask;
        let mut out = [0.0; 2];
        for (side, out) in out.iter_mut().enumerate() {
            let read = if now.polarity >= 1.0 {
                self.read(side, now.rising)
            } else if now.polarity <= 0.0 {
                -self.read(side, now.falling)
            } else {
                now.polarity * self.read(side, now.rising)
                    - (1.0 - now.polarity) * self.read(side, now.falling)
            };
            let low = low_pass(&mut self.low[side], read, now.low);
            let band = (low - low_pass(&mut self.high[side], low, now.high)) * now.boost;
            // Vital's clip, so however long it rings, what goes round never runs away.
            let fed = (band * now.feedback).clamp(-1.0, 1.0);
            let [first, second] = &mut self.cleaned[side];
            let cleaned = input[side] - low_pass(first, input[side], now.cleaning);
            let cleaned = cleaned - low_pass(second, cleaned, now.cleaning);
            self.lines[side][write] = cleaned * now.input + fed;
            // A comb is heard as what goes into its line, and a flanger as what comes out of it
            // with the sound, but the sound as it came in.
            let wet = fed + (band - fed) * now.flange;
            *out = (input[side] * now.dry + wet) * now.level;
        }
        self.write = write;
        self.now = now.plus(self.step);
        self.limiter.process(out)
    }

    /// Move the settings and the note on a block, and set the loop to move to what they now make
    /// over the block.
    fn work_out(&mut self, note: f32, smoothing: f32) {
        let smoothing = 1.0 - (1.0 - smoothing).powi(BLOCK as i32);
        self.note.target = note + self.tune;
        let smoothed = [
            &mut self.note,
            &mut self.feedback,
            &mut self.tone,
            &mut self.spread,
            &mut self.flange,
        ];
        let values = smoothed.map(|value| {
            if self.fresh {
                value.settle();
            }
            value.next(smoothing)
        });
        let mut previous = self.target;
        // Nothing's worked out again for less than can be heard.
        let moved = values
            .iter()
            .zip(&self.worked_from)
            .any(|(now, then)| (now - then).abs() >= 1e-4 || then.is_nan());
        if moved {
            self.worked_from = values;
            self.target = self.worked_out(values);
            // A read that wasn't heard starts where it's now going.
            if previous.polarity >= 1.0 {
                previous.falling = self.target.falling;
            }
            if previous.polarity <= 0.0 {
                previous.rising = self.target.rising;
            }
        }
        if self.fresh {
            self.now = self.target;
            self.step = Loop::default();
            self.fresh = false;
        } else {
            // Exactly where the last block was going, however the frames added up.
            self.now = previous;
            self.step = previous.steps_to(self.target);
        }
    }

    /// The loop for a note and settings.
    fn worked_out(&self, [note, feedback, tone, spread, flange]: [f32; 5]) -> Loop {
        let rate = self.rate as f32;
        let hertz = (440.0 * semitones(note - 69.0))
            .clamp(LOWEST_HZ, HIGHEST_HZ.min(rate / SHORTEST_CYCLE));
        // As a fraction of the rate, and the frames in a cycle.
        let f0 = hertz / rate;
        let cycle = 1.0 / f0;

        // The band's edges, in semitones from the note: `Tone` takes its low-pass from the note
        // to eight octaves up, and `Spread` takes its high-pass from there down to eight octaves
        // under the note.
        let top = tone * HIGHEST_TONE;
        let bottom = top + (LOWEST_CUT - top) * spread;
        let warped =
            |semitones_up: f32| (PI * (f0 * semitones(semitones_up)).min(HIGHEST_CUTOFF)).tan();
        let low = warped(top);
        let high = warped(bottom);
        let cleaning = warped(COMB_CLEANING + (FLANGE_CLEANING - COMB_CLEANING) * flange);
        // Vital's: the two one-poles together pass `1 / (1 + high / low)` at their peak.
        let boost = 1.0 + high / low;

        // Vital's curve, so more of `Feedback` is spent where it rings longest.
        let amount = feedback.abs().sqrt() * MOST_FEEDBACK;
        let polarity = (1.0 + feedback / POLARITY_SPAN).clamp(0.0, 1.0);

        // The loop's peaks sit where it goes round in a whole number of cycles: a cycle less the
        // band's delay, or half a cycle turned over for the odd harmonics alone. Each harmonic's
        // turn a frame is found from the note's.
        let (sin, cos) = (TAU * f0).sin_cos();
        let tuned = |period: f32, harmonics: &[(f32, f32)]| {
            // How long each harmonic rings, raised high enough that what rings longest decides.
            let mut found = [([0.0; 2], (0.0, 0.0), 0.0); HARMONICS.len()];
            let (mut turn, mut at) = ([sin, cos], 1.0);
            for (found, &(harmonic, stands_for)) in found.iter_mut().zip(harmonics) {
                if f0 * harmonic > HIGHEST_CUTOFF {
                    break;
                }
                // Turning by the note's turn until it's at the harmonic's.
                while at < harmonic {
                    let [turn_sin, turn_cos] = turn;
                    turn = [
                        turn_sin * cos + turn_cos * sin,
                        turn_cos * cos - turn_sin * sin,
                    ];
                    at += 1.0;
                }
                let response = band_response(turn, 1.0, low, high, boost);
                let rings = response.0 / (1.001 - amount * response.0);
                *found = (
                    turn,
                    response,
                    stands_for * (rings * rings) * (rings * rings),
                );
            }
            let most = found.iter().fold(0.0_f32, |most, found| most.max(found.2));
            let (mut delay, mut frequency, mut total) = (0.0, 0.0, 0.0);
            for (&(turn, response, weight), &(harmonic, _)) in found.iter().zip(harmonics) {
                // Those that hardly ring make little difference, and cost as much.
                if weight > most * 1e-3 {
                    let omega = TAU * f0 * harmonic;
                    let band = [low, high, boost];
                    delay += weight * pole_delay(omega, turn, response, period, amount, band);
                    frequency += weight * omega;
                    total += weight;
                }
            }
            read_for(delay / total, frequency / total).clamp(2.0, (self.mask - 3) as f32)
        };
        // A read that's not heard isn't worked out, and starts where it's first needed.
        let rising = if polarity > 0.0 || self.target.polarity > 0.0 {
            tuned(cycle, &HARMONICS)
        } else {
            self.target.rising
        };
        let falling = if polarity < 1.0 || self.target.polarity < 1.0 {
            tuned(cycle * 0.5, &ODD_HARMONICS)
        } else {
            self.target.falling
        };

        // How loud a hit comes out, worked out from the power it passes across the band, with as
        // many of its echoes as go round within a hit.
        let echoes =
            (HIT_SECONDS * rate / (rising * polarity + falling * (1.0 - polarity))).max(1.0);
        let (mut combed, mut flanged) = (0.0, 0.0);
        for &warped in &self.warped {
            let gain = band_gain(warped, low, high, boost);
            let passed = gain * gain;
            // What's left of it after as many times round as fit in a hit, and one fewer.
            let round = (amount * amount * passed).max(1e-12);
            let left = round.powf(echoes);
            combed += geometric(round, left, echoes);
            flanged += passed * geometric(round, left / round, echoes - 1.0);
        }
        // Vital turns down what goes into a comb as its feedback rises, and what comes straight
        // out of a flanger.
        let comb_input = 1.0 - amount * amount * 0.5;
        let flange_direct = 1.0 / (1.0 + amount);
        let combed = comb_input * comb_input * combed / BANDS as f32;
        let flanged = 0.5 * (flange_direct * flange_direct + flanged / BANDS as f32);
        let power = combed + (flanged - combed) * flange;

        Loop {
            rising,
            falling,
            polarity,
            low: low / (1.0 + low),
            high: high / (1.0 + high),
            boost,
            feedback: amount,
            input: comb_input + (FLANGE_SCALE - comb_input) * flange,
            dry: comb_input + (FLANGE_SCALE * flange_direct - comb_input) * flange,
            cleaning: cleaning / (1.0 + cleaning),
            flange,
            level: LEVEL / power.sqrt(),
        }
    }

    /// How far its limiter is turning down now, as a gain, for its tests.
    #[cfg(test)]
    fn limiting(&self) -> f32 {
        self.limiter.gain()
    }
}

#[cfg(test)]
mod tests {
    use vt_model::constants::UNITY_LEVEL;
    use vt_model::insert::MOST_SHIFT;

    use super::*;
    use crate::rng::Pcg32;

    const RATE: u32 = 48_000;
    /// Settings take effect at once, as the smoother has nothing to do.
    const NO_SMOOTHING: f32 = 1.0;

    /// A drum: a burst of noise falling away over about 20 ms, every quarter of a second.
    fn drums(frames: usize, level: f32, rate: u32) -> Vec<f32> {
        let mut rng = Pcg32::new(7, 1);
        (0..frames)
            .map(|frame| {
                let since = (frame % (rate as usize / 4)) as f32 / rate as f32;
                let noise = rng.next_u32() as f32 / 2_147_483_648.0 - 1.0;
                noise * level * (-since / 0.02).exp()
            })
            .collect()
    }

    /// A click, then nothing.
    fn click(frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames];
        out[0] = 0.5;
        out
    }

    /// A quiet sine, which the limiter leaves alone.
    fn sine(frames: usize, hertz: f32, rate: u32) -> Vec<f32> {
        (0..frames)
            .map(|frame| (frame as f32 / rate as f32 * hertz * TAU).sin() * 0.01)
            .collect()
    }

    fn played(settings: Settings, note: f32, input: &[f32], rate: u32) -> Vec<[f32; 2]> {
        let mut comb = Comb::new(rate);
        comb.follow(settings);
        input
            .iter()
            .map(|&sample| comb.process([sample; 2], note, NO_SMOOTHING))
            .collect()
    }

    fn left(frames: &[[f32; 2]]) -> Vec<f32> {
        frames.iter().map(|frame| frame[0]).collect()
    }

    fn rms(samples: impl Iterator<Item = f32>) -> f32 {
        let (sum, count) = samples.fold((0.0, 0), |(sum, count), s| (sum + s * s, count + 1));
        (sum / count as f32).sqrt()
    }

    fn peak(samples: impl Iterator<Item = f32>) -> f32 {
        samples.fold(0.0, |most: f32, s| most.max(s.abs()))
    }

    fn db(wet: f32, dry: f32) -> f32 {
        20.0 * (wet / dry).log10()
    }

    fn cents(found: f32, wanted: f32) -> f32 {
        1200.0 * (found / wanted).log2()
    }

    fn hertz_of(note: f32) -> f32 {
        440.0 * semitones(note - 69.0)
    }

    /// How strong a frequency is in some samples.
    fn strength(samples: &[f32], hertz: f32, rate: u32) -> f32 {
        let turn = f64::from(hertz) / f64::from(rate) * std::f64::consts::TAU;
        let (step_sin, step_cos) = turn.sin_cos();
        let (mut sin, mut cos) = (0.0_f64, 1.0_f64);
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for sample in samples {
            real += f64::from(*sample) * cos;
            imaginary += f64::from(*sample) * sin;
            (sin, cos) = (
                sin * step_cos + cos * step_sin,
                cos * step_cos - sin * step_sin,
            );
        }
        ((real * real + imaginary * imaginary).sqrt() * 2.0 / samples.len() as f64) as f32
    }

    /// The frequency within about half a semitone of `near` that's strongest in some samples, to
    /// a fraction of a cent: the strongest every ten cents, then every cent around it, then a
    /// parabola through the three around the strongest.
    fn strongest_near(samples: &[f32], near: f32, rate: u32) -> f32 {
        let at = |cents: f32| near * (cents / 1200.0).exp2();
        let best = |from: f32, step: f32, count: i32| {
            (-count..=count)
                .map(|step_at| from + step_at as f32 * step)
                .max_by(|a, b| {
                    strength(samples, at(*a), rate).total_cmp(&strength(samples, at(*b), rate))
                })
                .unwrap()
        };
        let coarse = best(0.0, 10.0, 5);
        let fine = best(coarse, 1.0, 10);
        let [below, middle, above] =
            [fine - 1.0, fine, fine + 1.0].map(|cents| strength(samples, at(cents), rate));
        at(fine + 0.5 * (below - above) / (below - 2.0 * middle + above))
    }

    /// What rings after a click at some settings, from four cycles of the note after it, for
    /// forty.
    fn ring(settings: Settings, note: f32, rate: u32) -> Vec<f32> {
        let cycle = (rate as f32 / hertz_of(note)) as usize;
        left(&played(settings, note, &click(cycle * 44), rate))[cycle * 4..].to_vec()
    }

    /// The harmonic of the note that rings the loudest, and how far off it is, in cents.
    fn loudest_harmonic(settings: Settings, note: f32, rate: u32) -> (usize, f32) {
        let tail = ring(settings, note, rate);
        let hertz = hertz_of(note);
        let harmonic = (1..=24)
            .filter(|harmonic| hertz * (*harmonic as f32) < rate as f32 * 0.4)
            .max_by(|a, b| {
                strength(&tail, hertz * *a as f32, rate).total_cmp(&strength(
                    &tail,
                    hertz * *b as f32,
                    rate,
                ))
            })
            .unwrap();
        let wanted = hertz * harmonic as f32;
        (harmonic, cents(strongest_near(&tail, wanted, rate), wanted))
    }

    #[test]
    fn it_rings_on_the_notes_harmonics_at_every_rate_and_tone() {
        // Feedback near full either way, so it rings long enough to tell.
        let (rising, falling) = (0xFC, 0x04);
        let at = |feedback, tone, spread| Settings {
            feedback,
            tone,
            spread,
            ..Settings::DEFAULT
        };
        let settings = [
            // A low-pass, from on the note to wide open.
            at(rising, 0x00, 0xFF),
            at(rising, 0x30, 0xFF),
            at(rising, 0x60, 0xFF),
            at(rising, 0x80, 0xFF),
            at(rising, 0xC0, 0xFF),
            at(rising, 0xFF, 0xFF),
            // The same, on the odd harmonics alone. Darker than this, it's gone in a few
            // cycles, too soon to tell its pitch to a few cents.
            at(falling, 0x30, 0xFF),
            at(falling, 0x60, 0xFF),
            at(falling, 0x80, 0xFF),
            at(falling, 0xC0, 0xFF),
            // A narrow band on the note and two octaves up, and a high-pass on the note and two
            // octaves up, where a higher harmonic rings longest.
            at(rising, 0x00, 0x00),
            at(rising, 0x40, 0x00),
            at(rising, 0xFF, 0x80),
            at(rising, 0xFF, 0x60),
        ];
        for rate in [44_100, 48_000, 96_000, 192_000] {
            for note in [24.0, 36.0, 48.0, 60.0, 72.0, 84.0, 96.0] {
                for settings in settings {
                    let (harmonic, off) = loudest_harmonic(settings, note, rate);
                    assert!(
                        off.abs() < 3.0,
                        "{rate} {note} {:x} {:x} {:x}: harmonic {harmonic} {off} cents",
                        settings.feedback,
                        settings.tone,
                        settings.spread
                    );
                }
            }
        }
    }

    #[test]
    fn it_rings_at_the_note_and_tune_above_it() {
        for (tune, note) in [
            (MOST_SHIFT, 57.0),
            (MOST_SHIFT + 7, 57.0),
            (MOST_SHIFT - 12, 69.0),
            (MOST_SHIFT + 24, 48.0),
        ] {
            let settings = Settings {
                tune,
                feedback: 0xFC,
                ..Settings::DEFAULT
            };
            let wanted = hertz_of(note + shift_semitones(f32::from(tune)));
            let found = strongest_near(&ring(settings, note, RATE), wanted, RATE);
            assert!(
                cents(found, wanted).abs() < 3.0,
                "{tune} {note}: {found} for {wanted}"
            );
        }
    }

    #[test]
    fn negative_feedback_rings_on_the_odd_harmonics_alone() {
        let hertz = hertz_of(48.0);
        let second_against_third = |feedback| {
            let settings = Settings {
                feedback,
                tone: 0xFF,
                ..Settings::DEFAULT
            };
            let tail = ring(settings, 48.0, RATE);
            strength(&tail, hertz * 2.0, RATE) / strength(&tail, hertz * 3.0, RATE)
        };
        let (rising, falling) = (second_against_third(0xF0), second_against_third(0x10));
        assert!(rising > 0.5, "{rising}");
        assert!(falling < 0.05, "{falling}");
    }

    #[test]
    fn feedback_makes_it_ring_longer() {
        // A tenth of a second, from a fifth of a second after a click.
        let tail = |feedback| {
            let settings = Settings {
                feedback,
                ..Settings::DEFAULT
            };
            let out = played(settings, 48.0, &click(RATE as usize / 2), RATE);
            rms(out[RATE as usize / 5..RATE as usize * 3 / 10]
                .iter()
                .map(|frame| frame[0]))
        };
        let (none, some, more, most) = (tail(0x80), tail(0xC0), tail(0xE0), tail(0xFC));
        assert!(none < 1e-6, "{none}");
        assert!(
            some > none && more > some * 10.0 && most > more * 3.0,
            "{none} {some} {more} {most}"
        );
        // And either way up.
        assert!(
            (tail(0x04) / most - 1.0).abs() < 0.5,
            "{} {most}",
            tail(0x04)
        );
    }

    #[test]
    fn tone_darkens_what_rings_and_spread_narrows_it_to_a_band() {
        let hertz = hertz_of(36.0);
        let harmonics = |tone, spread| {
            let settings = Settings {
                tone,
                spread,
                ..Settings::DEFAULT
            };
            let tail = ring(settings, 36.0, RATE);
            [1.0, 4.0, 8.0].map(|harmonic| strength(&tail, hertz * harmonic, RATE))
        };
        let ([dark_first, _, dark_eighth], [bright_first, _, bright_eighth]) =
            (harmonics(0x10, 0xFF), harmonics(0xFF, 0xFF));
        assert!(
            dark_eighth / dark_first < bright_eighth / bright_first * 0.01,
            "{dark_first} {dark_eighth} {bright_first} {bright_eighth}"
        );
        // A narrow band two octaves up keeps the fourth harmonic over the first and the eighth.
        let [first, fourth, eighth] = harmonics(0x40, 0x00);
        assert!(
            fourth > first * 3.0 && fourth > eighth * 3.0,
            "{first} {fourth} {eighth}"
        );
    }

    #[test]
    fn a_flanger_notches_between_the_harmonics_even_with_no_feedback() {
        let hertz = hertz_of(48.0);
        let through = |mode, at: f32| {
            let settings = Settings {
                mode,
                feedback: 0x80,
                tone: 0xFF,
                ..Settings::DEFAULT
            };
            let out = played(settings, 48.0, &sine(RATE as usize / 2, at, RATE), RATE);
            rms(out[RATE as usize / 4..].iter().map(|frame| frame[0]))
        };
        let (on, between) = (
            through(CombMode::Flange, hertz),
            through(CombMode::Flange, hertz * 1.5),
        );
        assert!(on > between * 20.0, "{on} {between}");
        // A comb with no feedback is only the sound.
        let (on, between) = (
            through(CombMode::Comb, hertz),
            through(CombMode::Comb, hertz * 1.5),
        );
        assert!((db(on, between)).abs() < 0.1, "{on} {between}");
    }

    #[test]
    fn a_drum_comes_out_about_as_loud_as_it_went_in() {
        let input = drums(RATE as usize * 2, 0.5, RATE);
        let (dry, dry_peak) = (rms(input.iter().copied()), peak(input.iter().copied()));
        for mode in CombMode::ALL {
            for note in [36.0, 48.0, 60.0, 72.0, 84.0] {
                let settings = Settings {
                    mode,
                    ..Settings::DEFAULT
                };
                let out = played(settings, note, &input, RATE);
                let level = db(loudness(&out), dry);
                let loudest = db(peak(out.iter().flatten().copied()), dry_peak);
                assert!(level.abs() < 2.0, "{mode:?} {note}: {level} dB");
                assert!(loudest.abs() < 2.0, "{mode:?} {note}: {loudest} dB");
            }
        }
    }

    /// How loud both sides are together.
    fn loudness(frames: &[[f32; 2]]) -> f32 {
        rms(frames.iter().flatten().copied())
    }

    #[test]
    fn moving_its_settings_and_note_glides_without_a_step() {
        let smoothing = crate::dsp::smoother::coefficient(RATE);
        let mut comb = Comb::new(RATE);
        let mut last = [0.0; 2];
        let mut largest = 0.0_f32;
        for frame in 0..RATE as usize * 4 {
            // A jump every 64 frames, as a knob or an LFO steps it block by block, with the
            // feedback turning over and the mode changing, and a new note every 192.
            let step = (frame / 64 % 2) as u8;
            comb.follow(Settings {
                mode: CombMode::ALL[usize::from(step)],
                tune: MOST_SHIFT + step * 7,
                feedback: 0x08 + step * 0xF0,
                tone: 0x20 + step * 0xC0,
                spread: 0x20 + step * 0xC0,
                mix: 0xFF,
                gain: UNITY_LEVEL,
            });
            let note = 36.0 + (frame / 192 % 3) as f32 * 5.0;
            // A low, quiet sine, so what moves is the comb and not the sound.
            let low = (frame as f32 / RATE as f32 * 55.0 * TAU).sin() * 0.05;
            let out = comb.process([low; 2], note, smoothing);
            if frame > RATE as usize / 2 {
                for side in 0..2 {
                    largest = largest.max((out[side] - last[side]).abs());
                }
            }
            last = out;
        }
        // Unsmoothed, the largest step is about 0.044.
        assert!(largest < 0.005, "{largest}");
    }

    #[test]
    fn it_stays_finite_and_within_full_scale_at_every_extreme() {
        for rate in [44_100, 192_000] {
            for mode in CombMode::ALL {
                for feedback in [0x00, 0x80, 0xFF] {
                    for tone in [0x00, 0xFF] {
                        for spread in [0x00, 0xFF] {
                            let mut comb = Comb::new(rate);
                            comb.follow(Settings {
                                mode,
                                feedback,
                                tone,
                                spread,
                                ..Settings::DEFAULT
                            });
                            for frame in 0..rate as usize / 2 {
                                // A loud square at the note, then the note jumping from one end
                                // to the other with every frame.
                                let input = if frame / 92 % 2 == 0 { 4.0 } else { -4.0 };
                                let note = if frame < rate as usize / 4 {
                                    60.0
                                } else if frame % 2 == 0 {
                                    -24.0
                                } else {
                                    151.0
                                };
                                let out = comb.process([input, -input], note, 1.0);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() <= 1.0),
                                    "{rate} {mode:?} {feedback:x} {tone:x} {spread:x}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_long_loud_tone_at_its_note_is_held_down_and_let_go() {
        let mut comb = Comb::new(RATE);
        comb.follow(Settings {
            feedback: 0xFF,
            tone: 0xFF,
            ..Settings::DEFAULT
        });
        let hertz = hertz_of(60.0);
        let mut loudest = 0.0_f32;
        for frame in 0..RATE as usize * 4 {
            let tone = (frame as f32 / RATE as f32 * hertz * TAU).sin() * 2.0;
            let out = comb.process([tone; 2], 60.0, NO_SMOOTHING);
            loudest = loudest.max(out[0].abs());
        }
        assert!(loudest <= 1.0, "{loudest}");
        assert!(comb.limiting() < 0.5, "{}", comb.limiting());
        // Once it's stopped ringing, the limiter lets go.
        comb.follow(Settings {
            feedback: 0x80,
            ..Settings::DEFAULT
        });
        for _ in 0..RATE as usize * 8 {
            comb.process([0.0; 2], 60.0, NO_SMOOTHING);
        }
        assert!(comb.limiting() > 0.9, "{}", comb.limiting());
    }

    #[test]
    fn its_lines_hold_a_cycle_of_the_lowest_note_with_room_to_spare() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let comb = Comb::new(rate);
            let longest = rate as f32 / LOWEST_HZ * 1.25;
            assert!(comb.lines[0].len() as f32 > longest + 4.0, "{rate}");
            assert!(comb.lines[0].len() as f32 <= longest * 2.0 + 16.0, "{rate}");
        }
    }
}
