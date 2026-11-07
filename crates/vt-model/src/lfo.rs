//! Low-frequency oscillators, which a track keeps running apart from its steps.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::constants::{TICKS_PER_QUARTER, TICKS_PER_STEP};
use crate::target::Target;

/// How many a track has.
pub const LFOS_PER_TRACK: usize = 2;
/// The furthest either way a depth goes, in the units of what it moves.
pub const MAX_LFO_DEPTH: i16 = 0xFF;

const BAR_TICKS: u32 = TICKS_PER_QUARTER as u32 * 4;
const MAX_RATE_BARS: u32 = 64;
const MAX_RATE_STEPS: u32 = 4096;
const MIN_CENTIHERTZ: u32 = 1;
const MAX_CENTIHERTZ: u32 = 10_000;

/// The note divisions `+` and `-` go through, shortest first, as ticks per cycle. Whole bars follow.
const DIVISIONS: [(&str, u32); 18] = [
    ("1/64", 60),
    ("1/32t", 80),
    ("1/64d", 90),
    ("1/32", 120),
    ("1/16t", 160),
    ("1/32d", 180),
    ("1/16", 240),
    ("1/8t", 320),
    ("1/16d", 360),
    ("1/8", 480),
    ("1/4t", 640),
    ("1/8d", 720),
    ("1/4", 960),
    ("1/2t", 1280),
    ("1/4d", 1440),
    ("1/2", 1920),
    ("1/2d", 2880),
    ("1 bar", BAR_TICKS),
];

/// The divisions' names, shortest first, for a setting to choose between.
pub const DIVISION_NAMES: [&str; DIVISIONS.len()] = {
    let mut names = [""; DIVISIONS.len()];
    let mut at = 0;
    while at < DIVISIONS.len() {
        names[at] = DIVISIONS[at].0;
        at += 1;
    }
    names
};

/// How many ticks each is.
pub const DIVISION_TICKS: [u32; DIVISIONS.len()] = {
    let mut ticks = [0; DIVISIONS.len()];
    let mut at = 0;
    while at < DIVISIONS.len() {
        ticks[at] = DIVISIONS[at].1;
        at += 1;
    }
    ticks
};

/// How long one cycle takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rate {
    /// A note division or whole bars, in ticks.
    Note(u32),
    /// A number of steps, for lengths that slide against the bar.
    Steps(u32),
    /// Free of the tempo, in hundredths of a hertz.
    Hertz(u32),
}

/// Why typed text isn't a rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a rate is like 1/4, 1/8d, 1/16t, 2 bars, 7st or 2.5hz")]
pub struct RateError;

impl Rate {
    /// Whether this is a rate that can be set.
    pub fn in_range(self) -> bool {
        match self {
            Self::Note(ticks) => (1..=MAX_RATE_BARS * BAR_TICKS).contains(&ticks),
            Self::Steps(steps) => (1..=MAX_RATE_STEPS).contains(&steps),
            Self::Hertz(hundredths) => (MIN_CENTIHERTZ..=MAX_CENTIHERTZ).contains(&hundredths),
        }
    }

    /// How many cycles pass in some ticks of the song and some seconds of playing.
    pub fn cycles(self, ticks: f64, seconds: f64) -> f64 {
        match self {
            Self::Note(length) => ticks / f64::from(length.max(1)),
            Self::Steps(steps) => ticks / (f64::from(steps.max(1)) * TICKS_PER_STEP as f64),
            Self::Hertz(hundredths) => seconds * f64::from(hundredths) / 100.0,
        }
    }

    /// The next faster or slower rate of the same kind.
    #[must_use]
    pub fn nudged(self, faster: bool) -> Self {
        match self {
            Self::Note(ticks) => Self::Note(next_length(ticks, !faster)),
            Self::Steps(steps) if faster => Self::Steps(steps.saturating_sub(1).max(1)),
            Self::Steps(steps) => Self::Steps((steps + 1).min(MAX_RATE_STEPS)),
            Self::Hertz(hundredths) => {
                let size = |value| match value {
                    0..=99 => 5,
                    100..=999 => 10,
                    _ => 100,
                };
                Self::Hertz(if faster {
                    (hundredths + size(hundredths)).min(MAX_CENTIHERTZ)
                } else {
                    hundredths
                        .saturating_sub(size(hundredths.saturating_sub(1)))
                        .max(MIN_CENTIHERTZ)
                })
            }
        }
    }
}

/// A rate as note lengths that can be chosen from a list: divisions, then doubling bars.
pub fn note_rates() -> Vec<Rate> {
    let bars = [2, 4, 8, 16, 32, MAX_RATE_BARS].map(|bars| bars * BAR_TICKS);
    DIVISIONS
        .iter()
        .map(|(_, ticks)| *ticks)
        .chain(bars)
        .map(Rate::Note)
        .collect()
}

/// The next length a note rate can take, going through the divisions and then doubling bars.
fn next_length(ticks: u32, longer: bool) -> u32 {
    let mut lengths = DIVISIONS
        .iter()
        .map(|(_, length)| *length)
        .chain([2, 4, 8, 16, 32, MAX_RATE_BARS].map(|bars| bars * BAR_TICKS));
    if longer {
        lengths
            .clone()
            .find(|length| *length > ticks)
            .unwrap_or(MAX_RATE_BARS * BAR_TICKS)
    } else {
        lengths
            .rfind(|length| *length < ticks)
            .unwrap_or(DIVISIONS[0].1)
    }
}

impl Rate {
    /// A large step faster or slower: straight note lengths only, skipping dotted and triplet, four
    /// steps, or a doubling of the hertz.
    #[must_use]
    pub fn jumped(self, faster: bool) -> Self {
        match self {
            Self::Note(ticks) => {
                let straight: Vec<u32> = note_rates()
                    .into_iter()
                    .filter_map(|rate| match rate {
                        Self::Note(length)
                            if length % 240 == 0 || length % 60 == 0 && length < 240 =>
                        {
                            Some(length)
                        }
                        _ => None,
                    })
                    .filter(|length| (length / 60).is_power_of_two())
                    .collect();
                let found = if faster {
                    straight.iter().rfind(|length| **length < ticks)
                } else {
                    straight.iter().find(|length| **length > ticks)
                };
                Self::Note(found.copied().unwrap_or(ticks))
            }
            Self::Steps(steps) if faster => Self::Steps(steps.saturating_sub(4).max(1)),
            Self::Steps(steps) => Self::Steps((steps + 4).min(MAX_RATE_STEPS)),
            Self::Hertz(hundredths) if faster => Self::Hertz((hundredths * 2).min(MAX_CENTIHERTZ)),
            Self::Hertz(hundredths) => Self::Hertz((hundredths / 2).max(MIN_CENTIHERTZ)),
        }
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Note(ticks) => match DIVISIONS.iter().find(|(_, length)| length == ticks) {
                Some((name, _)) => f.write_str(name),
                None => write!(f, "{} bars", ticks / BAR_TICKS),
            },
            Self::Steps(steps) => write!(f, "{steps}st"),
            Self::Hertz(hundredths) => {
                let text = format!("{}.{:02}", hundredths / 100, hundredths % 100);
                let text = text.trim_end_matches('0').trim_end_matches('.');
                write!(f, "{text}hz")
            }
        }
    }
}

impl FromStr for Rate {
    type Err = RateError;

    fn from_str(text: &str) -> Result<Self, RateError> {
        let text: String = text
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let rate = if let Some(number) = text.strip_suffix("hz") {
            let hertz: f64 = number.parse().map_err(|_| RateError)?;
            if !hertz.is_finite() || hertz < 0.0 {
                return Err(RateError);
            }
            Self::Hertz((hertz * 100.0).round() as u32)
        } else if let Some(number) = text.strip_suffix("st") {
            Self::Steps(number.parse().map_err(|_| RateError)?)
        } else if let Some(number) = text
            .strip_suffix("bars")
            .or_else(|| text.strip_suffix("bar"))
        {
            let bars: u32 = number.parse().map_err(|_| RateError)?;
            Self::Note(bars.checked_mul(BAR_TICKS).ok_or(RateError)?)
        } else {
            let (_, ticks) = DIVISIONS
                .iter()
                .find(|(name, _)| *name == text)
                .ok_or(RateError)?;
            Self::Note(*ticks)
        };
        rate.in_range().then_some(rate).ok_or(RateError)
    }
}

/// The wave an LFO follows. `Saw` falls and `Ramp` rises.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shape {
    Sine,
    #[default]
    Tri,
    Saw,
    Ramp,
    Square,
    Random,
    /// Random values with a smooth path between them.
    Smooth,
}

impl Shape {
    /// Every shape, in the order `+` goes through them.
    pub const ALL: [Self; 7] = [
        Self::Sine,
        Self::Tri,
        Self::Saw,
        Self::Ramp,
        Self::Square,
        Self::Random,
        Self::Smooth,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sine => "Sine",
            Self::Tri => "Tri",
            Self::Saw => "Saw",
            Self::Ramp => "Ramp",
            Self::Square => "Square",
            Self::Random => "Random",
            Self::Smooth => "Smooth",
        }
    }
}

/// A wave's value from -1 to 1, some number of cycles from its start. It's worked out from where
/// the song is rather than stepped along, so it can't drift, and the same project gives the same
/// result. `key` picks a random wave's
/// values, so tracks and LFOs differ.
#[inline]
pub fn wave(shape: Shape, cycles: f64, key: u64) -> f32 {
    let position = cycles.rem_euclid(1.0) as f32;
    match shape {
        Shape::Sine => (position * std::f32::consts::TAU).sin(),
        Shape::Tri => {
            if position < 0.25 {
                position * 4.0
            } else if position < 0.75 {
                2.0 - position * 4.0
            } else {
                position * 4.0 - 4.0
            }
        }
        Shape::Saw => 1.0 - position * 2.0,
        Shape::Ramp => position * 2.0 - 1.0,
        Shape::Square => {
            if position < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        Shape::Random => random(key, cycles.floor() as i64),
        // It starts each cycle on the value `Random` holds for it, and eases to the next one.
        Shape::Smooth => {
            let cycle = cycles.floor() as i64;
            let eased = position * position * (3.0 - 2.0 * position);
            let from = random(key, cycle);
            from + (random(key, cycle.wrapping_add(1)) - from) * eased
        }
    }
}

/// A value from -1 to 1 for one cycle, from a stateless mix so nothing has to be stepped.
fn random(key: u64, cycle: i64) -> f32 {
    let mut mixed = key
        .wrapping_add((cycle as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^= mixed >> 31;
    (mixed >> 40) as f32 / (1_u64 << 23) as f32 - 1.0
}

/// One LFO's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lfo {
    pub destination: Option<Target>,
    pub shape: Shape,
    pub rate: Rate,
    /// How far it swings either way, in the destination's units. Negative flips the shape.
    pub depth: i16,
    /// Where in the cycle it starts, in degrees.
    pub phase: u16,
    /// Whether each note on the track restarts it. Otherwise only playing from the top does.
    pub retrig: bool,
}

impl Default for Lfo {
    fn default() -> Self {
        Self {
            destination: None,
            shape: Shape::default(),
            rate: Rate::Note(TICKS_PER_QUARTER as u32),
            depth: 0,
            phase: 0,
            retrig: false,
        }
    }
}

impl Lfo {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.rate.in_range() && self.depth.abs() <= MAX_LFO_DEPTH && self.phase < 360
    }
}

/// A depth as text, with its sign, in hex like the values it moves.
pub fn depth_text(depth: i16) -> String {
    match depth {
        0 => "00".to_owned(),
        depth if depth < 0 => format!("-{:02X}", depth.unsigned_abs()),
        depth => format!("+{depth:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(text: &str) -> Rate {
        text.parse().unwrap()
    }

    #[test]
    fn rates_read_as_note_divisions_in_exact_ticks() {
        assert_eq!(rate("1/4"), Rate::Note(960));
        assert_eq!(rate("1/8d"), Rate::Note(720));
        assert_eq!(rate("1/16T"), Rate::Note(160));
        assert_eq!(rate("1 bar"), Rate::Note(3840));
        assert_eq!(rate("2bars"), Rate::Note(7680));
        assert_eq!(rate("3 bars"), Rate::Note(11_520));
        assert_eq!(rate("7st"), Rate::Steps(7));
        assert_eq!(rate("2.5hz"), Rate::Hertz(250));
        assert_eq!(rate("0.05 Hz"), Rate::Hertz(5));
    }

    #[test]
    fn a_rate_shows_as_it_can_be_typed() {
        for text in [
            "1/4", "1/16t", "1/8d", "1 bar", "2 bars", "3 bars", "7st", "2.5hz", "12hz",
        ] {
            assert_eq!(rate(text).to_string(), text);
            assert_eq!(rate(&rate(text).to_string()), rate(text));
        }
    }

    #[test]
    fn nonsense_and_out_of_range_rates_are_refused() {
        for text in [
            "", "4", "1/3", "fast", "0st", "0hz", "-1hz", "101hz", "0 bars", "65 bars",
        ] {
            assert!(text.parse::<Rate>().is_err(), "{text}");
        }
    }

    #[test]
    fn nudging_walks_the_divisions_and_stops_at_the_ends() {
        assert_eq!(rate("1/4").nudged(false), rate("1/2t"));
        assert_eq!(rate("1/4").nudged(true), rate("1/8d"));
        assert_eq!(rate("1/2d").nudged(false), rate("1 bar"));
        assert_eq!(rate("1 bar").nudged(false), rate("2 bars"));
        assert_eq!(rate("3 bars").nudged(false), rate("4 bars"));
        assert_eq!(rate("3 bars").nudged(true), rate("2 bars"));
        assert_eq!(rate("1/64").nudged(true), rate("1/64"));
        assert_eq!(rate("64 bars").nudged(false), rate("64 bars"));
        assert_eq!(rate("7st").nudged(false), rate("8st"));
        assert_eq!(rate("1st").nudged(true), rate("1st"));
        assert_eq!(rate("0.05hz").nudged(false), rate("0.01hz"));
        assert_eq!(rate("1hz").nudged(false), rate("0.95hz"));
        assert_eq!(rate("1hz").nudged(true), rate("1.1hz"));
        assert_eq!(rate("100hz").nudged(true), rate("100hz"));
    }

    #[test]
    fn a_large_step_skips_dotted_and_triplet_rates() {
        assert_eq!(rate("1/4").jumped(true), rate("1/8"));
        assert_eq!(rate("1/4").jumped(false), rate("1/2"));
        assert_eq!(rate("1/8d").jumped(true), rate("1/8"));
        assert_eq!(rate("1/8d").jumped(false), rate("1/4"));
        assert_eq!(rate("1/2").jumped(false), rate("1 bar"));
        assert_eq!(rate("1 bar").jumped(false), rate("2 bars"));
        assert_eq!(rate("3 bars").jumped(false), rate("4 bars"));
        assert_eq!(rate("1/64").jumped(true), rate("1/64"));
        assert_eq!(rate("64 bars").jumped(false), rate("64 bars"));
        assert_eq!(rate("7st").jumped(false), rate("11st"));
        assert_eq!(rate("3st").jumped(true), rate("1st"));
        assert_eq!(rate("2hz").jumped(true), rate("4hz"));
        assert_eq!(rate("2hz").jumped(false), rate("1hz"));
        assert_eq!(rate("0.01hz").jumped(false), rate("0.01hz"));
        assert_eq!(rate("60hz").jumped(true), rate("100hz"));
    }

    #[test]
    fn cycles_follow_the_song_or_the_clock() {
        assert_eq!(rate("1/4").cycles(1920.0, 99.0), 2.0);
        assert_eq!(rate("3st").cycles(1440.0, 99.0), 2.0);
        assert_eq!(rate("2hz").cycles(99.0, 1.5), 3.0);
    }

    #[test]
    fn depth_shows_its_sign_in_hex() {
        assert_eq!(depth_text(0), "00");
        assert_eq!(depth_text(0x40), "+40");
        assert_eq!(depth_text(-0xFF), "-FF");
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-5, "{a} and {b}");
    }

    #[test]
    fn each_shape_goes_where_its_name_says() {
        close(wave(Shape::Sine, 0.25, 0), 1.0);
        close(wave(Shape::Sine, 0.75, 0), -1.0);
        close(wave(Shape::Tri, 0.0, 0), 0.0);
        close(wave(Shape::Tri, 0.25, 0), 1.0);
        close(wave(Shape::Tri, 0.75, 0), -1.0);
        close(wave(Shape::Saw, 0.0, 0), 1.0);
        close(wave(Shape::Saw, 0.5, 0), 0.0);
        close(wave(Shape::Ramp, 0.0, 0), -1.0);
        close(wave(Shape::Ramp, 0.5, 0), 0.0);
        close(wave(Shape::Square, 0.25, 0), 1.0);
        close(wave(Shape::Square, 0.75, 0), -1.0);
    }

    #[test]
    fn a_wave_repeats_and_runs_before_its_start() {
        close(wave(Shape::Tri, 3.25, 0), 1.0);
        close(wave(Shape::Tri, -0.75, 0), 1.0);
    }

    #[test]
    fn random_holds_for_a_cycle_and_changes_by_cycle_and_key() {
        let held = wave(Shape::Random, 2.1, 5);
        assert_eq!(held, wave(Shape::Random, 2.9, 5));
        assert_ne!(held, wave(Shape::Random, 3.1, 5));
        assert_ne!(held, wave(Shape::Random, 2.1, 6));
        for cycle in 0..1000 {
            let value = wave(Shape::Random, f64::from(cycle), 1);
            assert!((-1.0..=1.0).contains(&value), "{value}");
        }
    }

    #[test]
    fn smooth_goes_through_randoms_values_without_jumping() {
        for cycle in -3..100 {
            let cycles = f64::from(cycle);
            close(
                wave(Shape::Smooth, cycles, 5),
                wave(Shape::Random, cycles, 5),
            );
        }
        // The steepest an eased path between values 2 apart can be is 3 per cycle.
        let steps = 10_000;
        let at = |step: i32| wave(Shape::Smooth, f64::from(step) / f64::from(steps), 5);
        for step in -steps..steps * 100 {
            let (here, next) = (at(step), at(step + 1));
            assert!((-1.0..=1.0).contains(&here), "{here}");
            assert!((next - here).abs() <= 3.01 / steps as f32, "{here} {next}");
        }
    }

    #[test]
    fn smooth_is_flat_at_each_value_and_follows_its_key() {
        let start = wave(Shape::Smooth, 2.0, 5);
        assert!((wave(Shape::Smooth, 2.001, 5) - start).abs() < 1e-4);
        assert_ne!(wave(Shape::Smooth, 2.5, 5), start);
        assert_ne!(wave(Shape::Smooth, 2.5, 5), wave(Shape::Smooth, 2.5, 6));
    }
}
