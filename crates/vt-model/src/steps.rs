//! Steps, one of a mod slot's types: up to sixteen values played in turn, in time or one a note, as
//! the M8's tables, Bitwig's `Steps` and the Metropolix's mod lanes step through theirs. Like an
//! LFO, where it is is worked out from the song's position or a count, and from where it was at
//! each change of its rate on, by the song's ticks rather than the blocks, so it can't drift and a
//! render repeats (decision 301).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::lfo::{DIVISION_NAMES, DIVISION_TICKS};
use crate::random::{GOLDEN_GAMMA, splitmix64};
use crate::shape::{EVEN_SPREAD, MAX_BIAS, MAX_SPREAD, Random};

/// How many values a slot keeps.
pub const STEPS: usize = 16;
/// The furthest either way a value goes, in its destination's units at a depth of `+FF`.
pub const MAX_STEP: i16 = 0xFF;
/// The most `Mutate` goes, in percent.
pub(crate) const MAX_MUTATE: u8 = 100;
/// The most `Jump` goes, in percent.
pub(crate) const MAX_JUMP: u8 = 100;
/// How many of a track's steps a `Tick` is a sixth of, as `DEL` counts.
pub(crate) const TICKS_PER_STEP_SHARE: u64 = 6;

/// How a Steps slot moves through its values against the track's notes: its mode, in its title.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepsMode {
    /// Each note starts it at its `Start`.
    #[default]
    Trig,
    /// It runs from the song's position, as an LFO in `Free` does.
    Free,
    /// It moves on one step with each note, the M8's `TIC00` and Bitwig's Note/Advance.
    Note,
}

impl StepsMode {
    /// Every mode, in the order `+` goes through them.
    pub const ALL: [Self; 3] = [Self::Trig, Self::Free, Self::Note];

    /// The name in the title.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Trig => "Trig",
            Self::Free => "Free",
            Self::Note => "Note",
        }
    }

    /// What it does, for a list to choose from.
    pub(crate) fn meaning(self) -> &'static str {
        match self {
            Self::Trig => "each note starts it at its start",
            Self::Free => "runs from the song's position",
            Self::Note => "one step on with each note",
        }
    }
}

crate::listed::listed!(StepsMode: meaning);

/// How long each step lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepRate {
    /// A sixth of the track's step, as `DEL` counts, the M8's table speed.
    Tick,
    /// A note division, in ticks.
    Note(u32),
    /// One step a pass of the track's chain, counted as `CND` counts passes, so it starts again
    /// with the chain (after the Cirklon's accumulators).
    Pass,
}

impl Default for StepRate {
    fn default() -> Self {
        Self::Note(240)
    }
}

/// How many rates there are: `Tick`, the divisions, then `Pass`.
const RATES: usize = DIVISION_TICKS.len() + 2;

/// Why typed text isn't a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a Steps rate is tick, a division like 1/16 or 1/8d, or pass")]
pub struct StepRateError;

impl StepRate {
    /// Every rate, shortest first: `Tick`, the divisions from `1/64` to a bar, then `Pass`.
    pub const ALL: [Self; RATES] = {
        let mut all = [Self::Tick; RATES];
        let mut at = 0;
        while at < DIVISION_TICKS.len() {
            all[at + 1] = Self::Note(DIVISION_TICKS[at]);
            at += 1;
        }
        all[RATES - 1] = Self::Pass;
        all
    };

    /// Whether this is a rate that can be set.
    pub(crate) fn in_range(self) -> bool {
        Self::ALL.contains(&self)
    }

    /// Where it is on the registry's `00` to `FF` scale, with a coarse byte and a fine one, for
    /// macros and modulation to move: faster up the scale, as `=` is, stepping evenly through the
    /// divisions from a bar at `00` to `1/64`, then `Tick` at `FF`. `Pass` isn't a speed, so it has
    /// no place.
    pub fn place(self) -> Option<u16> {
        let timed = RATES - 1;
        let at = Self::ALL[..timed].iter().position(|rate| *rate == self)?;
        let along = 1.0 - at as f64 / (timed - 1) as f64;
        Some((along * 255.0 * 256.0).round() as u16)
    }

    /// The rate at a place on the registry's scale, with any fine part as the fraction, the
    /// inverse of [`StepRate::place`]: the nearest there. `Pass` stays as it is.
    #[must_use]
    pub(crate) fn placed(self, value: f32) -> Self {
        if self == Self::Pass {
            return self;
        }
        let timed = RATES - 1;
        let along = (f64::from(value) / 255.0).clamp(0.0, 1.0);
        Self::ALL[timed - 1 - (along * (timed - 1) as f64).round() as usize]
    }

    /// The next rate one way, `=` being faster, as it is for an LFO, stopping at either end. A
    /// large step skips the dotted and triplet divisions.
    #[must_use]
    pub fn nudged(self, faster: bool, large: bool) -> Self {
        let at = Self::ALL.iter().position(|rate| *rate == self).unwrap_or(7);
        let straight = |rate: &&Self| match **rate {
            Self::Note(ticks) => ticks % 60 == 0 && (ticks / 60).is_power_of_two(),
            _ => true,
        };
        let found = if faster {
            Self::ALL[..at]
                .iter()
                .rfind(|rate| !large || straight(rate))
        } else {
            Self::ALL[at + 1..]
                .iter()
                .find(|rate| !large || straight(rate))
        };
        found.copied().unwrap_or(self)
    }

    /// How many ticks each step lasts on a track whose steps are this many ticks long, or none
    /// for one a pass.
    pub fn ticks(self, step_ticks: u64) -> Option<f64> {
        match self {
            Self::Tick => Some(step_ticks.max(1) as f64 / TICKS_PER_STEP_SHARE as f64),
            Self::Note(ticks) => Some(f64::from(ticks.max(1))),
            Self::Pass => None,
        }
    }
}

impl fmt::Display for StepRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tick => f.write_str("Tick"),
            Self::Note(ticks) => match DIVISION_TICKS.iter().position(|length| length == ticks) {
                Some(at) => f.write_str(DIVISION_NAMES[at]),
                None => write!(f, "{ticks}t"),
            },
            Self::Pass => f.write_str("Pass"),
        }
    }
}

impl FromStr for StepRate {
    type Err = StepRateError;

    fn from_str(text: &str) -> Result<Self, StepRateError> {
        let text: String = text
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        match text.as_str() {
            "tick" => return Ok(Self::Tick),
            "pass" => return Ok(Self::Pass),
            _ => {}
        }
        DIVISION_NAMES
            .iter()
            .position(|name| name.replace(' ', "") == text)
            .map(|at| Self::Note(DIVISION_TICKS[at]))
            .ok_or(StepRateError)
    }
}

/// What a Steps slot does at the end of its `Length`. All but `Once` go round from its `From`,
/// the steps before it played once as an intro (decision 428).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Loop {
    /// Plays to the end and holds the last, the Cirklon's `clip`.
    Once,
    /// Plays once, then loops from `From`: the M8's `HOP`, and from the first step the Cirklon's
    /// `rtz`.
    #[default]
    Loop,
    /// Goes back and forth between the end and `From`, the Cirklon's `rvtz`.
    Bounce,
    /// Plays the steps from `From` on in a new order each time round, dealt from the project's
    /// seed, so a render repeats.
    Shuffle,
}

impl Loop {
    /// Every way, in the order `+` goes through them.
    pub const ALL: [Self; 4] = [Self::Once, Self::Loop, Self::Bounce, Self::Shuffle];

    /// The name in the title.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Once => "Once",
            Self::Loop => "Loop",
            Self::Bounce => "Bounce",
            Self::Shuffle => "Shuffle",
        }
    }

    /// What it does, for a list to choose from.
    pub(crate) fn meaning(self) -> &'static str {
        match self {
            Self::Once => "plays to the end and holds the last",
            Self::Loop => "plays once, then loops from its From",
            Self::Bounce => "goes back and forth, down to its From",
            Self::Shuffle => "a new order each time round, from its From",
        }
    }

    /// Whether it goes round from `From`, which `Once` ignores.
    pub const fn uses_from(self) -> bool {
        !matches!(self, Self::Once)
    }
}

crate::listed::listed!(Loop: meaning);

impl fmt::Display for Loop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Where a Steps slot's laps are counted from once its `From` has changed while it went round:
/// the count a lap starts at, and which lap that is, for a `Shuffle`'s deal. Without one, its laps
/// are counted from the end of its intro, as they always were (decision 428).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lap {
    pub at: i64,
    pub number: i64,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// A Steps slot's own settings. Where it goes and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Steps {
    /// Its values, `-FF` to `+FF`, in the destination's units at a depth of `+FF`. Those past
    /// `Length` are kept, so lengthening it finds them.
    #[serde(skip_serializing_if = "is_default")]
    pub values: [i16; STEPS],
    #[serde(skip_serializing_if = "is_default")]
    pub mode: StepsMode,
    #[serde(skip_serializing_if = "is_eighth")]
    pub length: u8,
    /// The step a note starts it at, counted from 0.
    #[serde(skip_serializing_if = "is_default")]
    pub start: u8,
    #[serde(skip_serializing_if = "is_default")]
    pub rate: StepRate,
    #[serde(rename = "loop", skip_serializing_if = "is_default")]
    pub looping: Loop,
    /// The step `Loop`, `Bounce` and `Shuffle` go round from, counted from 0, the steps before it
    /// played once as an intro; kept within `Length` as it plays, and ignored by `Once`. A change
    /// while it goes round is taken up as the next lap starts (decision 428).
    #[serde(skip_serializing_if = "is_default")]
    pub from: u8,
    /// The chance each step's value is drawn again as it comes round, in percent: `0` plays the
    /// values as written, `100` a new one each time, and between them a loop that drifts, as the
    /// Turing Machine's knob does. Drawn from the project's seed, so a render repeats.
    #[serde(skip_serializing_if = "is_default")]
    pub mutate: u8,
    /// The chance each step it moves on to is any of its `Length` instead, in percent, as
    /// Marbles' Déjà vu past noon replays its loop out of order: `0` walks as `Loop` says, `100`
    /// is a step at random every time. Drawn from the project's seed, so a render repeats.
    #[serde(skip_serializing_if = "is_default")]
    pub jump: u8,
    /// How `Mutate` spreads the values it draws, as Marbles' X `Spread` and a `Random` fill's do:
    /// from 0, the bias point alone, through a bell, to `50`, every value as likely, as it always
    /// drew, and on through a bowl to `100`, the two ends alone (decision 403).
    #[serde(skip_serializing_if = "is_even_spread")]
    pub spread: u8,
    /// Which way the values `Mutate` draws lean, from `-100`, the low end, to `+100`, the high, as
    /// Marbles' X `Bias` and a `Random` fill's do.
    #[serde(skip_serializing_if = "is_default")]
    pub bias: i8,
    /// How it goes from step to step, `-FF` to `+FF`, after Surge's step sequencer's deform: none
    /// steps; up to `+80` glides from the step before over the first part of each step, then on
    /// to a curve through the steps by `+FF`; down to `-80` falls to nothing over the last part of
    /// each, a pluck a step, sharper toward `-FF`.
    #[serde(skip_serializing_if = "is_default")]
    pub smooth: i16,
}

fn is_even_spread(spread: &u8) -> bool {
    *spread == EVEN_SPREAD
}

fn is_eighth(length: &u8) -> bool {
    *length == DEFAULT_LENGTH
}

/// How many steps a new Steps slot plays.
const DEFAULT_LENGTH: u8 = 8;

impl Default for Steps {
    fn default() -> Self {
        Self {
            values: [0; STEPS],
            mode: StepsMode::Trig,
            length: DEFAULT_LENGTH,
            start: 0,
            rate: StepRate::default(),
            looping: Loop::default(),
            from: 0,
            mutate: 0,
            jump: 0,
            spread: EVEN_SPREAD,
            bias: 0,
            smooth: 0,
        }
    }
}

impl Steps {
    /// Whether these are settings that can be set.
    pub(crate) fn in_range(&self) -> bool {
        self.values.iter().all(|value| value.abs() <= MAX_STEP)
            && (1..=STEPS as u8).contains(&self.length)
            && usize::from(self.start) < STEPS
            && self.rate.in_range()
            && self.mutate <= MAX_MUTATE
            && self.jump <= MAX_JUMP
            && self.spread <= MAX_SPREAD
            && self.bias.unsigned_abs() <= MAX_BIAS.unsigned_abs()
            && self.smooth.abs() <= MAX_STEP
            && usize::from(self.from) < STEPS
    }

    /// The value `Mutate` draws from `draw`, the next number of its stream, `-FF` to `+FF`: at an
    /// even `Spread` and no `Bias`, `draw` over the values evenly, as it always was, and otherwise
    /// through `Random`'s bent distribution, so one shape serves both. It allocates nothing, for
    /// the audio thread.
    pub fn mutated(&self, draw: u32) -> i16 {
        let span = 2 * MAX_STEP as u32 + 1;
        let shape = Random {
            spread: self.spread,
            bias: self.bias,
            ..Random::default()
        };
        if !shape.shaped() {
            return (draw % span) as i16 - MAX_STEP;
        }
        let even = f64::from(draw) / (f64::from(u32::MAX) + 1.0);
        shape.pick(even, span as usize) as i16 - MAX_STEP
    }

    /// How many steps it plays, from 1 to 16.
    pub fn played_count(&self) -> usize {
        usize::from(self.length).clamp(1, STEPS)
    }

    /// Its values played, never empty.
    pub fn played(&self) -> &[i16] {
        &self.values[..self.played_count()]
    }

    /// Which step plays `count` steps after it started at `start`, counted from 0, under its
    /// `Loop` and `From`. `key` deals a `Shuffle`'s orders.
    pub fn step_at(&self, start: usize, count: i64, key: u64) -> usize {
        self.step_on(start, count, key, self.from, None)
    }

    /// Which step plays `count` steps after it started at `start` under its `Loop`, going round
    /// from `from`, counted from 0 and kept within its length, with its laps counted from `lap`
    /// if `From` has changed as it went round, or else from the end of its intro. The intro plays
    /// in order from `start`: to the end for `Loop` and `Bounce`, which then come back to `from`,
    /// and up to `from` for `Shuffle`, which deals the rest. From the first step, each is as it
    /// always was (decision 428).
    pub fn step_on(&self, start: usize, count: i64, key: u64, from: u8, lap: Option<Lap>) -> usize {
        let length = self.played_count() as i64;
        let start = (start as i64).min(length - 1);
        let count = count.max(0);
        let from = i64::from(from).min(length - 1);
        let top = length - 1;
        if self.looping == Loop::Once {
            return (start + count).min(top) as usize;
        }
        let first = self.first_lap(start, from);
        if lap.is_none() && count < first {
            return (start + count) as usize;
        }
        let Lap { at, number } = lap.unwrap_or(Lap {
            at: first,
            number: 0,
        });
        let along = count - at;
        match self.looping {
            Loop::Once | Loop::Loop => (from + along.rem_euclid(length - from)) as usize,
            // Each lap starts at the end, goes down to `from` and comes back.
            Loop::Bounce if top == from => top as usize,
            Loop::Bounce => {
                let down = top - from;
                let turn = along.rem_euclid(2 * down);
                (if turn <= down {
                    top - turn
                } else {
                    turn - down + from
                }) as usize
            }
            Loop::Shuffle => {
                let count = length - from;
                let deal = number + along.div_euclid(count);
                (from
                    + i64::from(dealt(count as usize, deal, key)[along.rem_euclid(count) as usize]))
                    as usize
            }
        }
    }

    /// The count its first lap starts at, from `start` and going round from `from`, both kept
    /// within its length: the end of the intro. A `Loop`'s starts once the intro has reached the
    /// end, a `Bounce`'s at the end, and a `Shuffle`'s at `from`, before the count starts if
    /// `start` is past it, as its first deal is then played from part way in.
    fn first_lap(&self, start: i64, from: i64) -> i64 {
        let length = self.played_count() as i64;
        match self.looping {
            Loop::Once | Loop::Loop => length - start,
            Loop::Bounce => length - 1 - start,
            Loop::Shuffle => from - start,
        }
    }

    /// How many counts a lap lasts going round from `from`, kept within its length: at least one.
    fn lap_length(&self, from: u8) -> i64 {
        let length = self.played_count() as i64;
        let from = i64::from(from).min(length - 1);
        match self.looping {
            Loop::Bounce => (2 * (length - 1 - from)).max(1),
            _ => length - from,
        }
    }

    /// The lap after the one `count` is in, going round from `from` with laps counted from `lap`
    /// or the intro's end, where a new `From` is taken up. None for `Once`, or while it's in its
    /// intro, where `From` is taken at once as the intro doesn't play it.
    pub fn next_lap(&self, start: usize, count: i64, from: u8, lap: Option<Lap>) -> Option<Lap> {
        if self.looping == Loop::Once {
            return None;
        }
        let length = self.played_count() as i64;
        let start = (start as i64).min(length - 1);
        let first = self.first_lap(start, i64::from(from).min(length - 1));
        let Lap { at, number } = match lap {
            Some(lap) => lap,
            None if count < first => return None,
            None => Lap {
                at: first,
                number: 0,
            },
        };
        let each = self.lap_length(from);
        let laps = (count.max(0) - at).div_euclid(each) + 1;
        Some(Lap {
            at: at + laps * each,
            number: number + laps,
        })
    }
}

/// What a step sounds as, `at` from 0 to 1 of the way through it, under a `Smooth` from -1 to 1,
/// from the values of the two steps before it, its own and the one after (Surge's step sequencer
/// deform, `LFOModulationSource.cpp`, GPL-3.0-or-later, adapted): its own at none; a glide from the
/// one before over the first `2 smooth` of it up to a half, so it arrives as the step ends there;
/// beyond, that straight line blended into a Catmull-Rom curve through all four by full; and below
/// none, its own value falling to nothing over the last `-2 smooth` of it, a pluck, which past
/// half falls on a sharper curve.
#[inline]
pub fn smoothed([earlier, before, own, after]: [f32; 4], at: f32, smooth: f32) -> f32 {
    let at = at.clamp(0.0, 1.0);
    match smooth.clamp(-1.0, 1.0) {
        0.0 => own,
        smooth if smooth > 0.5 => {
            let line = before + (own - before) * at;
            let curve = crate::lfo::catmull_rom([earlier, before, own, after], at);
            (2.0 - 2.0 * smooth) * line + (2.0 * smooth - 1.0) * curve
        }
        smooth if smooth > 0.0 => before + (own - before) * (at / (2.0 * smooth)).min(1.0),
        smooth if smooth >= -0.5 => own * ((1.0 - at) / (-2.0 * smooth)).min(1.0),
        smooth => own * (1.0 - at).powf(1.0 + 6.0 * (-0.5 - smooth)),
    }
}

/// The order a `Shuffle` plays some steps in, its `lap`th time round: a Fisher-Yates shuffle from
/// a mix of the key and the lap, so each time round is dealt afresh and a render deals the same.
fn dealt(length: usize, lap: i64, key: u64) -> [u8; STEPS] {
    let mut order: [u8; STEPS] = std::array::from_fn(|at| at as u8);
    let mut state = key ^ (lap as u64).wrapping_mul(GOLDEN_GAMMA);
    for at in (1..length).rev() {
        state = splitmix64(state);
        let other = (state % (at as u64 + 1)) as usize;
        order.swap(at, other);
    }
    order
}

impl Steps {
    /// Its start as the phase setting's value, with a coarse byte and a fine one, where a step's
    /// share of `FF00` picks it: the middle of its share of the length, so the value comes back
    /// as the same step.
    pub(crate) fn start_value(&self) -> u16 {
        let length = self.played_count() as f64;
        let start = f64::from(self.start).min(length - 1.0);
        ((start + 0.5) / length * f64::from(0xFF00_u16)).round() as u16
    }

    /// The step a phase on the `00` to `FF` scale picks, as a step's lock of it does: the value
    /// times the length over 256, so `PHS` picks where each note's run starts.
    pub fn start_of(&self, value: f32) -> usize {
        let length = self.played_count();
        ((value / 256.0).rem_euclid(1.0) * length as f32).floor() as usize % length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(length: u8, looping: Loop) -> Steps {
        Steps {
            length,
            looping,
            ..Steps::default()
        }
    }

    #[test]
    fn a_rate_on_the_registrys_scale_steps_through_the_divisions_to_a_tick() {
        let rate = |text: &str| text.parse::<StepRate>().unwrap();
        assert_eq!(rate("1 bar").place(), Some(0));
        assert_eq!(rate("tick").place(), Some(0xFF00));
        assert_eq!(rate("pass").place(), None, "a pass isn't a speed");
        assert_eq!(rate("pass").placed(255.0), rate("pass"));
        let mut last = None;
        for rate in StepRate::ALL.iter().rev().skip(1) {
            let place = rate.place().unwrap();
            assert_eq!(rate.placed(f32::from(place) / 256.0), *rate, "{rate}");
            assert!(last.is_none_or(|last| place > last), "{rate} is faster");
            last = Some(place);
        }
        assert_eq!(
            rate("1/16").placed(300.0),
            rate("tick"),
            "kept to the scale"
        );
    }

    fn walk(steps: &Steps, start: usize, count: i64) -> Vec<usize> {
        (0..count).map(|at| steps.step_at(start, at, 7)).collect()
    }

    fn from(length: u8, looping: Loop, from: u8) -> Steps {
        Steps {
            from,
            ..steps(length, looping)
        }
    }

    #[test]
    fn each_loop_goes_through_the_steps_its_way() {
        assert_eq!(walk(&steps(4, Loop::Once), 0, 6), [0, 1, 2, 3, 3, 3]);
        assert_eq!(walk(&steps(4, Loop::Loop), 0, 6), [0, 1, 2, 3, 0, 1]);
        // `From 3` plays once, then loops the last two, as the M8's `HOP` does.
        assert_eq!(walk(&from(4, Loop::Loop, 2), 0, 7), [0, 1, 2, 3, 2, 3, 2]);
        assert_eq!(
            walk(&steps(4, Loop::Bounce), 0, 8),
            [0, 1, 2, 3, 2, 1, 0, 1]
        );
        assert_eq!(walk(&steps(1, Loop::Bounce), 0, 3), [0, 0, 0]);
        // From a later start, the first pass begins there.
        assert_eq!(walk(&steps(4, Loop::Loop), 2, 4), [2, 3, 0, 1]);
        assert_eq!(walk(&steps(4, Loop::Once), 2, 4), [2, 3, 3, 3]);
        // A start or a `From` past the length keeps within it.
        assert_eq!(walk(&from(3, Loop::Loop, 9), 5, 4), [2, 2, 2, 2]);
        assert_eq!(walk(&from(3, Loop::Bounce, 9), 0, 6), [0, 1, 2, 2, 2, 2]);
        assert_eq!(walk(&from(3, Loop::Shuffle, 9), 0, 6), [0, 1, 2, 2, 2, 2]);
        // `Once` ignores `From`.
        assert_eq!(walk(&from(4, Loop::Once, 2), 0, 6), [0, 1, 2, 3, 3, 3]);
    }

    /// How a step was found before `From` was a setting of its own, `from N` one of the loop's
    /// list, kept to show each is found as it was.
    fn as_it_was(looping: Loop, from: i64, length: i64, start: i64, count: i64, key: u64) -> usize {
        let start = start.min(length - 1);
        let first = length - start;
        match looping {
            Loop::Once => (start + count).min(length - 1) as usize,
            Loop::Loop => {
                let from = from.min(length - 1);
                if count < first {
                    (start + count) as usize
                } else {
                    (from + (count - first) % (length - from)) as usize
                }
            }
            Loop::Bounce if length == 1 => 0,
            Loop::Bounce => {
                let period = 2 * (length - 1);
                let at = (start + count) % period;
                (if at < length { at } else { period - at }) as usize
            }
            Loop::Shuffle => {
                let at = start + count;
                dealt(length as usize, at / length, key)[(at % length) as usize] as usize
            }
        }
    }

    #[test]
    fn each_old_from_and_each_loop_from_the_first_step_walk_as_they_did() {
        for length in 1..=16_u8 {
            for start in 0..16 {
                // Every old `from N` is `Loop` with `From N`.
                for at in 0..16 {
                    let steps = from(length, Loop::Loop, at);
                    for count in 0..80 {
                        let was = as_it_was(
                            Loop::Loop,
                            i64::from(at),
                            i64::from(length),
                            start,
                            count,
                            9,
                        );
                        assert_eq!(steps.step_at(start as usize, count, 9), was);
                    }
                }
                // `Bounce` and `Shuffle` from the first step, their turns and deals as they were.
                for looping in [Loop::Once, Loop::Bounce, Loop::Shuffle] {
                    let steps = steps(length, looping);
                    for count in 0..80 {
                        let was = as_it_was(looping, 0, i64::from(length), start, count, 9);
                        assert_eq!(
                            steps.step_at(start as usize, count, 9),
                            was,
                            "{looping} {length} {start} {count}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bounce_and_shuffle_from_a_later_step_keep_the_ones_before_it_as_an_intro() {
        // `From 5` on eight: 1 to 4 play once, then it bounces between the end and 5.
        assert_eq!(
            walk(&from(8, Loop::Bounce, 4), 0, 14),
            [0, 1, 2, 3, 4, 5, 6, 7, 6, 5, 4, 5, 6, 7]
        );
        let shuffled = from(8, Loop::Shuffle, 4);
        let walked = walk(&shuffled, 0, 4 + 4 * 6);
        assert_eq!(walked[..4], [0, 1, 2, 3]);
        for deal in walked[4..].chunks(4) {
            let mut sorted = deal.to_vec();
            sorted.sort_unstable();
            assert_eq!(sorted, [4, 5, 6, 7], "{walked:?}");
        }
        assert_ne!(walked[4..8], walked[8..12], "dealt afresh each time round");
        // A start past `From` has no intro, its first deal played from part way in.
        let late = walk(&shuffled, 6, 2 + 4);
        let mut deal = late[2..].to_vec();
        deal.sort_unstable();
        assert_eq!(deal, [4, 5, 6, 7], "{late:?}");
    }

    #[test]
    fn laps_counted_from_a_change_of_from_go_round_from_there() {
        // Looping the last four of eight, then from the third: the lap after count 9 starts at 12.
        let steps = from(8, Loop::Loop, 4);
        let lap = steps.next_lap(0, 9, 4, None).unwrap();
        assert_eq!(lap, Lap { at: 12, number: 1 });
        let walked: Vec<usize> = (12..20)
            .map(|count| steps.step_on(0, count, 7, 2, Some(lap)))
            .collect();
        assert_eq!(walked, [2, 3, 4, 5, 6, 7, 2, 3]);
        // In the intro there's no lap to wait for, and `Once` has none.
        assert_eq!(steps.next_lap(0, 3, 4, None), None);
        assert_eq!(from(8, Loop::Once, 4).next_lap(0, 30, 4, None), None);
        // A bounce's lap starts back at the end.
        let bounce = from(8, Loop::Bounce, 4);
        let lap = bounce.next_lap(0, 8, 4, None).unwrap();
        assert_eq!(lap.at, 13);
        assert_eq!(bounce.step_on(0, lap.at, 7, 0, Some(lap)), 7);
    }

    #[test]
    fn a_shuffle_plays_every_step_once_each_time_round_in_a_dealt_order() {
        let shuffled = steps(8, Loop::Shuffle);
        let laps: Vec<Vec<usize>> = (0..4)
            .map(|lap| {
                (0..8)
                    .map(|at| shuffled.step_at(0, lap * 8 + at, 3))
                    .collect()
            })
            .collect();
        for lap in &laps {
            let mut sorted = lap.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..8).collect::<Vec<_>>());
        }
        assert_ne!(laps[0], laps[1], "dealt afresh each time round");
        // The same key deals the same, so a render repeats, and another deals another.
        assert_eq!(walk(&shuffled, 0, 32), walk(&shuffled, 0, 32));
        let other: Vec<usize> = (0..8).map(|at| shuffled.step_at(0, at, 4)).collect();
        assert_ne!(other, laps[0]);
    }

    #[test]
    fn a_rate_reads_shows_and_steps_toward_tick_when_faster() {
        let rate = |text: &str| text.parse::<StepRate>().unwrap();
        assert_eq!(rate("tick"), StepRate::Tick);
        assert_eq!(rate("1/16"), StepRate::Note(240));
        assert_eq!(rate("Pass"), StepRate::Pass);
        for each in StepRate::ALL {
            assert_eq!(each.to_string().parse::<StepRate>(), Ok(each));
        }
        assert!("2 bars".parse::<StepRate>().is_err());
        assert_eq!(StepRate::Note(240).nudged(true, false), StepRate::Note(180));
        assert_eq!(StepRate::Note(60).nudged(true, false), StepRate::Tick);
        assert_eq!(StepRate::Tick.nudged(true, false), StepRate::Tick);
        assert_eq!(StepRate::Note(3840).nudged(false, false), StepRate::Pass);
        assert_eq!(StepRate::Note(240).nudged(true, true), StepRate::Note(120));
        // A tick is a sixth of the track's step, as `DEL` counts.
        assert_eq!(StepRate::Tick.ticks(240), Some(40.0));
        assert_eq!(StepRate::Pass.ticks(240), None);
    }

    #[test]
    fn a_start_goes_there_and_back_as_a_phase() {
        for length in 1..=16 {
            for start in 0..length {
                let at = Steps {
                    length,
                    start,
                    ..Steps::default()
                };
                let value = f32::from(at.start_value()) / 256.0;
                assert_eq!(at.start_of(value), usize::from(start), "{length} {start}");
            }
        }
        let eight = Steps::default();
        assert_eq!(eight.start_of(0.0), 0);
        assert_eq!(eight.start_of(255.0), 7);
        assert_eq!(eight.start_of(128.0), 4);
    }

    #[test]
    fn steps_save_only_what_differs_from_a_new_ones() {
        assert_eq!(ron::to_string(&Steps::default()).unwrap(), "()");
        let mut steps = Steps::default();
        steps.values[1] = 7;
        steps.looping = Loop::Once;
        steps.rate = StepRate::Tick;
        let text = ron::to_string(&steps).unwrap();
        assert!(
            text.contains("loop:Once") && text.contains("rate:Tick"),
            "{text}"
        );
        assert_eq!(ron::from_str::<Steps>(&text).unwrap(), steps);
        assert!(steps.in_range());
        steps.values[3] = 0x100;
        assert!(!steps.in_range());
        let drifting = Steps {
            mutate: 30,
            jump: 20,
            spread: 20,
            bias: -40,
            smooth: -0x40,
            ..Steps::default()
        };
        let text = ron::to_string(&drifting).unwrap();
        assert_eq!(text, "(mutate:30,jump:20,spread:20,bias:-40,smooth:-64)");
        assert_eq!(ron::from_str::<Steps>(&text).unwrap(), drifting);
        assert!(
            !Steps {
                mutate: 101,
                ..drifting
            }
            .in_range()
        );
        assert!(
            !Steps {
                jump: 101,
                ..drifting
            }
            .in_range()
        );
        for (spread, bias) in [(101, 0), (50, 101), (50, -101)] {
            assert!(
                !Steps {
                    spread,
                    bias,
                    ..drifting
                }
                .in_range()
            );
        }
    }

    /// The values `Mutate` draws from an even stream of numbers, `count` of them.
    fn drawn(steps: &Steps, count: u32) -> Vec<i16> {
        let mut state = 7u64;
        (0..count)
            .map(|_| {
                state = splitmix64(state.wrapping_add(GOLDEN_GAMMA));
                steps.mutated((state >> 32) as u32)
            })
            .collect()
    }

    #[test]
    fn mutate_draws_evenly_as_it_always_did_at_an_even_spread_and_no_bias() {
        let steps = Steps::default();
        for draw in [0, 1, 254, 255, 510, 511, 12_345, u32::MAX - 1, u32::MAX] {
            assert_eq!(
                steps.mutated(draw),
                (draw % 511) as i16 - MAX_STEP,
                "{draw}"
            );
        }
    }

    #[test]
    fn spread_narrows_to_a_bell_widens_to_the_ends_and_bias_leans_them() {
        const DRAWS: u32 = 20_000;
        let shaped = |spread: u8, bias: i8| Steps {
            spread,
            bias,
            ..Steps::default()
        };
        // How many draws fall within a quarter of the middle, and in the outer eighth each side.
        let shares = |values: &[i16]| {
            let near = values.iter().filter(|value| value.abs() < 64).count();
            let far = values.iter().filter(|value| value.abs() > 224).count();
            let mean =
                values.iter().map(|value| f64::from(*value)).sum::<f64>() / values.len() as f64;
            (
                near as f64 / values.len() as f64,
                far as f64 / values.len() as f64,
                mean,
            )
        };
        let (even_near, even_far, even_mean) = shares(&drawn(&shaped(50, 0), DRAWS));
        assert!((even_near - 0.25).abs() < 0.02, "{even_near}");
        assert!((even_far - 0.12).abs() < 0.02, "{even_far}");
        assert!(even_mean.abs() < 6.0, "{even_mean}");
        // Low: a bell round the middle, nothing out at the ends.
        let (near, far, mean) = shares(&drawn(&shaped(10, 0), DRAWS));
        assert!(near > 0.9 && far < 0.01, "{near} {far}");
        assert!(mean.abs() < 3.0, "{mean}");
        assert!(drawn(&shaped(0, 0), 100).iter().all(|value| *value == 0));
        // Mid-low: still a bell, wider.
        let (near, far, _) = shares(&drawn(&shaped(35, 0), DRAWS));
        assert!(near > even_near + 0.1 && far < even_far, "{near} {far}");
        // High: a bowl, most of them out at the ends, and only the ends at 100.
        let (near, far, mean) = shares(&drawn(&shaped(90, 0), DRAWS));
        assert!(far > 4.0 * even_far && near < 0.1, "{near} {far}");
        assert!(mean.abs() < 10.0, "{mean}");
        let ends = drawn(&shaped(100, 0), DRAWS);
        assert!(ends.iter().all(|value| value.abs() == MAX_STEP));
        let high = ends.iter().filter(|value| **value > 0).count() as f64 / f64::from(DRAWS);
        assert!((high - 0.5).abs() < 0.02, "{high}");
        // Bias leans them: an even draw toward the high end, a bell centred on the bias point, and
        // the two ends the high one as often as the bias point is high.
        let (_, _, mean) = shares(&drawn(&shaped(50, 60), DRAWS));
        assert!(mean > 80.0, "{mean}");
        let (_, _, mean) = shares(&drawn(&shaped(50, -60), DRAWS));
        assert!(mean < -80.0, "{mean}");
        let mut bell = drawn(&shaped(10, -50), DRAWS);
        bell.sort_unstable();
        let middle = bell[bell.len() / 2];
        assert!(
            (middle + 128).abs() < 8,
            "the bias point, a quarter up: {middle}"
        );
        let ends = drawn(&shaped(100, 60), DRAWS);
        let high = ends.iter().filter(|value| **value > 0).count() as f64 / f64::from(DRAWS);
        assert!((high - 0.8).abs() < 0.02, "{high}");
    }

    #[test]
    fn smooth_steps_glides_curves_and_plucks() {
        let four = [0.2, -0.4, 0.8, 0.0];
        let at = |through: f32, smooth: f32| smoothed(four, through, smooth);
        let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-5, "{a} {b}");
        // None is the step's own, all through it.
        for through in [0.0, 0.3, 0.99] {
            close(at(through, 0.0), 0.8);
        }
        // A quarter glides in from the one before over the first half of the step.
        close(at(0.0, 0.25), -0.4);
        close(at(0.25, 0.25), 0.2);
        close(at(0.5, 0.25), 0.8);
        close(at(0.9, 0.25), 0.8);
        // A half glides across the whole step, and full is the curve through all four, which
        // passes each value as a Catmull-Rom does.
        close(at(0.5, 0.5), 0.2);
        close(at(0.0, 1.0), -0.4);
        close(at(1.0, 1.0), 0.8);
        assert!(
            (at(0.5, 1.0) - at(0.5, 0.5)).abs() > 0.01,
            "a curve, not the line"
        );
        // Below none, a pluck: its own value, falling to nothing over the last share of the step.
        close(at(0.0, -0.25), 0.8);
        close(at(0.5, -0.25), 0.8);
        close(at(0.75, -0.25), 0.4);
        close(at(1.0, -0.25), 0.0);
        close(at(0.5, -0.5), 0.4);
        // Past half, sharper.
        assert!(at(0.5, -1.0) < at(0.5, -0.5));
        assert!(at(0.5, -1.0) > 0.0);
    }
}
