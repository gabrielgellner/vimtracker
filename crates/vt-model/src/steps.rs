//! Steps, one of a mod slot's types (#235): up to sixteen values played in turn, in time or one a
//! note, as the M8's tables, Bitwig's `Steps` and the Metropolix's mod lanes step through theirs.
//! Like an LFO, where it is is worked out from the song's position or a count, never stepped
//! along, so it can't drift and a render repeats.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::lfo::{DIVISION_NAMES, DIVISION_TICKS};

/// How many values a slot keeps.
pub const STEPS: usize = 16;
/// The furthest either way a value goes, in its destination's units at a depth of `+FF`.
pub const MAX_STEP: i16 = 0xFF;
/// How many of a track's steps a `Tick` is a sixth of, as `DEL` counts.
pub const TICKS_PER_STEP_SHARE: u64 = 6;

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
    pub fn name(self) -> &'static str {
        match self {
            Self::Trig => "Trig",
            Self::Free => "Free",
            Self::Note => "Note",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Trig => "each note starts it at its start",
            Self::Free => "runs from the song's position",
            Self::Note => "one step on with each note",
        }
    }
}

/// How long each step lasts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepRate {
    /// A sixth of the track's step, as `DEL` counts, the M8's table speed.
    Tick,
    /// A note division, in ticks.
    Note(u32),
    /// One step a pass of the track's chain, counted as `CND` counts passes, so it starts again
    /// with the chain (#235's comment, after the Cirklon's accumulators).
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
    pub fn in_range(self) -> bool {
        Self::ALL.contains(&self)
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

/// What a Steps slot does at the end of its `Length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Loop {
    /// Plays to the end and holds the last, the Cirklon's `clip`.
    Once,
    /// Plays once, then loops from this step, counted from 0: the M8's `HOP`, and from the first
    /// step the Cirklon's `rtz`.
    From(u8),
    /// Goes back and forth, the Cirklon's `rvtz`.
    Bounce,
    /// Plays each time round in a new order, dealt from the project's seed, so a render repeats.
    Shuffle,
}

impl Default for Loop {
    fn default() -> Self {
        Self::From(0)
    }
}

impl Loop {
    /// Every way, in the order `+` goes through them.
    pub const ALL: [Self; STEPS + 3] = {
        let mut all = [Self::Once; STEPS + 3];
        let mut at = 0;
        while at < STEPS {
            all[at + 1] = Self::From(at as u8);
            at += 1;
        }
        all[STEPS + 1] = Self::Bounce;
        all[STEPS + 2] = Self::Shuffle;
        all
    };

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Once => "plays to the end and holds the last",
            Self::From(_) => "plays once, then loops from a step",
            Self::Bounce => "goes back and forth",
            Self::Shuffle => "a new order each time round",
        }
    }
}

impl fmt::Display for Loop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Once => f.write_str("Once"),
            Self::From(step) => write!(f, "from {}", step + 1),
            Self::Bounce => f.write_str("Bounce"),
            Self::Shuffle => f.write_str("Shuffle"),
        }
    }
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
        }
    }
}

impl Steps {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.values.iter().all(|value| value.abs() <= MAX_STEP)
            && (1..=STEPS as u8).contains(&self.length)
            && usize::from(self.start) < STEPS
            && self.rate.in_range()
            && match self.looping {
                Loop::From(step) => usize::from(step) < STEPS,
                _ => true,
            }
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
    /// `Loop`. `key` deals a `Shuffle`'s orders.
    pub fn step_at(&self, start: usize, count: i64, key: u64) -> usize {
        let length = self.played_count() as i64;
        let start = (start as i64).min(length - 1);
        let count = count.max(0);
        let first = length - start;
        match self.looping {
            Loop::Once => (start + count).min(length - 1) as usize,
            Loop::From(from) => {
                let from = i64::from(from).min(length - 1);
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
}

/// The order a `Shuffle` plays some steps in, its `lap`th time round: a Fisher-Yates shuffle from
/// a mix of the key and the lap, so each time round is dealt afresh and a render deals the same.
fn dealt(length: usize, lap: i64, key: u64) -> [u8; STEPS] {
    let mut order: [u8; STEPS] = std::array::from_fn(|at| at as u8);
    let mut state = key ^ (lap as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for at in (1..length).rev() {
        state = mix(state);
        let other = (state % (at as u64 + 1)) as usize;
        order.swap(at, other);
    }
    order
}

/// SplitMix64's mix, a stateless step from one value to the next.
fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

impl Steps {
    /// Its start as the phase setting's value, with a coarse byte and a fine one, where a step's
    /// share of `FF00` picks it: the middle of its share of the length, so the value comes back
    /// as the same step.
    pub fn start_value(&self) -> u16 {
        let length = self.played_count() as f64;
        let start = f64::from(self.start).min(length - 1.0);
        ((start + 0.5) / length * f64::from(0xFF00_u16)).round() as u16
    }

    /// The step a phase on the `00` to `FF` scale picks, as a step's lock of it does: the value
    /// times the length over 256, so `PHS` picks where each note's run starts (#235).
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

    fn walk(steps: &Steps, start: usize, count: i64) -> Vec<usize> {
        (0..count).map(|at| steps.step_at(start, at, 7)).collect()
    }

    #[test]
    fn each_loop_goes_through_the_steps_its_way() {
        assert_eq!(walk(&steps(4, Loop::Once), 0, 6), [0, 1, 2, 3, 3, 3]);
        assert_eq!(walk(&steps(4, Loop::From(0)), 0, 6), [0, 1, 2, 3, 0, 1]);
        // `from 3` plays once, then loops the last two, as the M8's `HOP` does.
        assert_eq!(walk(&steps(4, Loop::From(2)), 0, 7), [0, 1, 2, 3, 2, 3, 2]);
        assert_eq!(
            walk(&steps(4, Loop::Bounce), 0, 8),
            [0, 1, 2, 3, 2, 1, 0, 1]
        );
        assert_eq!(walk(&steps(1, Loop::Bounce), 0, 3), [0, 0, 0]);
        // From a later start, the first pass begins there.
        assert_eq!(walk(&steps(4, Loop::From(0)), 2, 4), [2, 3, 0, 1]);
        assert_eq!(walk(&steps(4, Loop::Once), 2, 4), [2, 3, 3, 3]);
        // A start or a loop past the length keeps within it.
        assert_eq!(walk(&steps(3, Loop::From(9)), 5, 4), [2, 2, 2, 2]);
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
    }
}
