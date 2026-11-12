//! Low-frequency oscillators, one of a mod slot's types, which a track keeps running apart from its
//! steps, and the musical rates they and effects' sweeps run at.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::constants::{TICKS_PER_QUARTER, TICKS_PER_STEP};

const BAR_TICKS: u32 = TICKS_PER_QUARTER as u32 * 4;
const MAX_RATE_BARS: u32 = 64;
const MAX_RATE_STEPS: u32 = 4096;
const MIN_CENTIHERTZ: u32 = 1;
const MAX_CENTIHERTZ: u32 = 10_000;
/// The furthest either way an LFO's deform goes.
pub const MAX_DEFORM: i16 = 0xFF;

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

/// The whole bars a sweep can take, after the divisions.
const SWEEP_BARS: [(&str, u32); 4] = [
    ("2 bars", 2 * BAR_TICKS),
    ("4 bars", 4 * BAR_TICKS),
    ("8 bars", 8 * BAR_TICKS),
    ("16 bars", 16 * BAR_TICKS),
];
const SWEEPS: usize = DIVISIONS.len() + SWEEP_BARS.len();

/// The lengths an effect's sweep chooses between, as a phaser's or flanger's `Rate`: the note
/// divisions, then whole bars up to 16, as a slow sweep wants.
pub const SWEEP_NAMES: [&str; SWEEPS] = {
    let mut names = [""; SWEEPS];
    let mut at = 0;
    while at < SWEEPS {
        names[at] = if at < DIVISIONS.len() {
            DIVISIONS[at].0
        } else {
            SWEEP_BARS[at - DIVISIONS.len()].0
        };
        at += 1;
    }
    names
};

/// How many ticks each sweep takes.
pub const SWEEP_TICKS: [u32; SWEEPS] = {
    let mut ticks = [0; SWEEPS];
    let mut at = 0;
    while at < SWEEPS {
        ticks[at] = if at < DIVISIONS.len() {
            DIVISIONS[at].1
        } else {
            SWEEP_BARS[at - DIVISIONS.len()].1
        };
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
    deformed_wave(shape, cycles, key, 0.0)
}

/// A wave bent by an LFO's `Deform`, from -1 to 1 (#468): Surge's `bend1`, one bend for every
/// shape, which skews `Sine`, `Tri`, `Saw` and `Ramp` toward a log or an exp shape while keeping
/// their peaks where they are, and is the width of `Square`'s high part. The random shapes have
/// nothing to bend. At 0 it's the wave exactly.
#[inline]
pub fn deformed_wave(shape: Shape, cycles: f64, key: u64, deform: f32) -> f32 {
    let position = cycles.rem_euclid(1.0) as f32;
    let bend = |value: f32| bent(value, deform);
    match shape {
        Shape::Sine => bend((position * std::f32::consts::TAU).sin()),
        Shape::Tri => bend(if position < 0.25 {
            position * 4.0
        } else if position < 0.75 {
            2.0 - position * 4.0
        } else {
            position * 4.0 - 4.0
        }),
        Shape::Saw => bend(1.0 - position * 2.0),
        Shape::Ramp => bend(position * 2.0 - 1.0),
        Shape::Square => {
            if position < 0.5 + 0.5 * deform {
                1.0
            } else {
                -1.0
            }
        }
        Shape::Random => random(key, cycles.floor() as i64),
        // A Catmull-Rom curve through the values `Random` holds for this cycle and its neighbours,
        // so it glides through each with the slope its neighbours give rather than stopping there,
        // as Surge's `Noise` does. It's still worked out on the spot from the four values, and it
        // overshoots a little, so it's kept within the wave's range.
        Shape::Smooth => {
            let cycle = cycles.floor() as i64;
            let at = |offset: i64| random(key, cycle.wrapping_add(offset));
            catmull_rom([at(-1), at(0), at(1), at(2)], position).clamp(-1.0, 1.0)
        }
    }
}

/// Surge's `bend1` (`LFOModulationSource.cpp`, GPL-3.0-or-later): `x - a x^2 + a`, twice, with `a`
/// half the deform, which keeps -1 and 1 where they are and never turns back between them.
#[inline]
fn bent(value: f32, deform: f32) -> f32 {
    if deform == 0.0 {
        return value;
    }
    let a = 0.5 * deform.clamp(-1.0, 1.0);
    let once = value - a * value * value + a;
    once - a * once * once + a
}

/// The Catmull-Rom curve through `points[1]` at 0 and `points[2]` at 1.
#[inline]
pub(crate) fn catmull_rom([before, from, to, after]: [f32; 4], t: f32) -> f32 {
    let (t2, t3) = (t * t, t * t * t);
    0.5 * (2.0 * from
        + (to - before) * t
        + (2.0 * before - 5.0 * from + 4.0 * to - after) * t2
        + (3.0 * from - before - 3.0 * to + after) * t3)
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

/// How an LFO runs against the track's notes: the mode in its title, the Digitakt's five
/// (`docs/designs/mod-slots.md` §2, #86). Every one is still worked out from the song's position
/// and the time since the track's last note, so none is stepped along and a render repeats.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LfoMode {
    /// It runs from the song's position, and only playing from the top starts it again.
    #[default]
    Free,
    /// Each note on the track starts it again at its `Phase`.
    Trig,
    /// It runs free underneath, and each note holds the value it's at until the next: a sample
    /// and hold of the wave.
    Hold,
    /// Each note starts it at its `Phase`, and it plays one cycle, then stays where it ended.
    One,
    /// The same, for half a cycle.
    Half,
}

impl LfoMode {
    /// Every mode, in the order `+` goes through them.
    pub const ALL: [Self; 5] = [Self::Free, Self::Trig, Self::Hold, Self::One, Self::Half];

    /// The name in the title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Free => "Free",
            Self::Trig => "Trig",
            Self::Hold => "Hold",
            Self::One => "One",
            Self::Half => "Half",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Free => "runs from the song's position",
            Self::Trig => "each note starts it at its phase",
            Self::Hold => "runs free, and each note holds where it is",
            Self::One => "each note plays one cycle, then it stays",
            Self::Half => "each note plays half a cycle, then it stays",
        }
    }

    /// Whether each note starts its cycle again at its `Phase`.
    pub fn restarts(self) -> bool {
        matches!(self, Self::Trig | Self::One | Self::Half)
    }

    /// How many cycles a one-shot plays from each note before it stays where it ended.
    pub fn plays_for(self) -> Option<f64> {
        match self {
            Self::One => Some(1.0),
            Self::Half => Some(0.5),
            _ => None,
        }
    }
}

/// How many lengths a fade can take either way: the sweeps', from `1/64` to 16 bars.
const FADES: usize = SWEEPS;

/// How an LFO fades in or out after each of the track's notes (#86): Elektron's `FADE` as a
/// musical length rather than a speed, none, or in or out over a note division or whole bars, in
/// ticks of [`SWEEP_TICKS`]. It counts from the last note in every mode, so in `Free` only the fade
/// starts again, not the wave.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fade {
    #[default]
    Off,
    /// From nothing to its whole depth over this many ticks after each note.
    In(u32),
    /// From its whole depth to nothing over this many ticks after each note.
    Out(u32),
}

/// Why typed text isn't a fade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a fade is off, or + or - and a length like 1/4, 1/8d or 2 bars")]
pub struct FadeError;

impl Fade {
    /// Every fade in the order `+` goes through them: the longest fade out, shorter and shorter
    /// to none, then longer and longer fades in.
    pub const ALL: [Self; 2 * FADES + 1] = {
        let mut all = [Self::Off; 2 * FADES + 1];
        let mut at = 0;
        while at < FADES {
            all[FADES - 1 - at] = Self::Out(SWEEP_TICKS[at]);
            all[FADES + 1 + at] = Self::In(SWEEP_TICKS[at]);
            at += 1;
        }
        all
    };

    /// Whether this is a fade that can be set: none, or one of the sweeps' lengths.
    pub fn in_range(self) -> bool {
        match self {
            Self::Off => true,
            Self::In(ticks) | Self::Out(ticks) => SWEEP_TICKS.contains(&ticks),
        }
    }

    /// How much of the wave is heard this many ticks after a note, from 0 to 1. With no note
    /// since playing started, it's as though the note were long ago: a fade in is done, and a
    /// fade out is over.
    #[inline]
    pub fn gain(self, since_note: Option<f64>) -> f32 {
        let share = |ticks: u32| {
            since_note.map_or(1.0, |since| {
                (since / f64::from(ticks.max(1))).clamp(0.0, 1.0) as f32
            })
        };
        match self {
            Self::Off => 1.0,
            Self::In(ticks) => share(ticks),
            Self::Out(ticks) => 1.0 - share(ticks),
        }
    }

    /// The next fade one way, stopping at either end. A large step skips the dotted and triplet
    /// lengths, as a rate's does.
    #[must_use]
    pub fn nudged(self, up: bool, large: bool) -> Self {
        let at = Self::ALL
            .iter()
            .position(|fade| *fade == self)
            .unwrap_or(FADES);
        let straight = |fade: &&Self| match **fade {
            Self::Off => true,
            Self::In(ticks) | Self::Out(ticks) => ticks % 60 == 0 && (ticks / 60).is_power_of_two(),
        };
        let found = if up {
            Self::ALL[at + 1..]
                .iter()
                .find(|fade| !large || straight(fade))
        } else {
            Self::ALL[..at]
                .iter()
                .rfind(|fade| !large || straight(fade))
        };
        found.copied().unwrap_or(self)
    }
}

impl fmt::Display for Fade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = |ticks: u32| {
            SWEEP_TICKS
                .iter()
                .position(|length| *length == ticks)
                .map_or("?", |at| SWEEP_NAMES[at])
        };
        match *self {
            Self::Off => f.write_str("Off"),
            Self::In(ticks) => write!(f, "+{}", name(ticks)),
            Self::Out(ticks) => write!(f, "-{}", name(ticks)),
        }
    }
}

impl FromStr for Fade {
    type Err = FadeError;

    fn from_str(text: &str) -> Result<Self, FadeError> {
        let text: String = text
            .to_ascii_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if text == "off" || text == "0" {
            return Ok(Self::Off);
        }
        let (out, length) = match text.strip_prefix('-') {
            Some(length) => (true, length),
            None => (false, text.strip_prefix('+').unwrap_or(&text)),
        };
        // `2bar` and `1bars` read as the names have them.
        let length = length.trim_end_matches('s');
        let at = SWEEP_NAMES
            .iter()
            .position(|name| name.replace(' ', "").trim_end_matches('s') == length)
            .ok_or(FadeError)?;
        let ticks = SWEEP_TICKS[at];
        Ok(if out {
            Self::Out(ticks)
        } else {
            Self::In(ticks)
        })
    }
}

/// An LFO's own settings: its wave, how fast, where it starts and how notes start it. Where it goes
/// and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lfo {
    pub shape: Shape,
    pub rate: Rate,
    /// Where in the cycle it starts, in degrees.
    pub phase: u16,
    pub mode: LfoMode,
    /// How far its wave is bent, from `-FF` to `+FF`: a skew toward a log or an exp shape, or a
    /// square's width (#468).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub deform: i16,
    /// How it fades in or out after each note (#86).
    #[serde(default, skip_serializing_if = "is_off")]
    pub fade: Fade,
}

fn is_zero(value: &i16) -> bool {
    *value == 0
}

fn is_off(fade: &Fade) -> bool {
    *fade == Fade::Off
}

impl Default for Lfo {
    fn default() -> Self {
        Self {
            shape: Shape::default(),
            rate: Rate::Note(TICKS_PER_QUARTER as u32),
            phase: 0,
            mode: LfoMode::Free,
            deform: 0,
            fade: Fade::Off,
        }
    }
}

impl Lfo {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.rate.in_range()
            && self.phase < 360
            && self.deform.abs() <= MAX_DEFORM
            && self.fade.in_range()
    }
}

/// A phase in degrees as a setting's value, where `FF00` is a whole turn.
pub fn phase_value(degrees: u16) -> u16 {
    (f64::from(degrees % 360) * f64::from(0xFF00_u16) / 360.0).round() as u16
}

/// How far round a cycle a value on the `00` to `FF` scale is, from 0 to 1.
pub fn phase_of(value: f32) -> f32 {
    value / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_goes_there_and_back_as_a_value() {
        assert_eq!(phase_value(0), 0);
        assert_eq!(phase_value(180), 0x7F80);
        assert!((phase_of(f32::from(phase_value(90)) / 256.0) - 0.25).abs() < 1e-6);
    }

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
        // It stays in range, and never jumps: the steepest a Catmull-Rom path through values
        // at most 2 apart can be is under 4 per cycle.
        let steps = 10_000;
        let at = |step: i32| wave(Shape::Smooth, f64::from(step) / f64::from(steps), 5);
        for step in -steps..steps * 100 {
            let (here, next) = (at(step), at(step + 1));
            assert!((-1.0..=1.0).contains(&here), "{here}");
            assert!((next - here).abs() <= 4.0 / steps as f32, "{here} {next}");
        }
    }

    #[test]
    fn smooth_glides_through_each_value_with_its_neighbours_slope_and_follows_its_key() {
        // At a value it moves on at the slope from the value before to the one after, rather than
        // stopping there as an eased path does, so it doesn't read as stop and go.
        let mut moving = 0;
        for cycle in 0..50 {
            let at = f64::from(cycle);
            let slope = (wave(Shape::Smooth, at + 1e-3, 5) - wave(Shape::Smooth, at, 5)) / 1e-3;
            let expected =
                (wave(Shape::Random, at + 1.0, 5) - wave(Shape::Random, at - 1.0, 5)) / 2.0;
            if wave(Shape::Smooth, at, 5).abs() < 0.9 {
                assert!((slope - expected).abs() < 0.05, "{slope} {expected}");
                moving += usize::from(expected.abs() > 0.1);
            }
        }
        assert!(moving > 10, "{moving}");
        // Its slope is the same either side of a value.
        let left = (wave(Shape::Smooth, 3.0, 5) - wave(Shape::Smooth, 3.0 - 1e-3, 5)) / 1e-3;
        let right = (wave(Shape::Smooth, 3.0 + 1e-3, 5) - wave(Shape::Smooth, 3.0, 5)) / 1e-3;
        assert!((left - right).abs() < 0.05, "{left} {right}");
        assert_ne!(wave(Shape::Smooth, 2.5, 5), wave(Shape::Smooth, 2.5, 6));
    }

    #[test]
    fn deform_bends_every_shape_between_its_peaks_and_is_a_squares_width() {
        for shape in [Shape::Sine, Shape::Tri, Shape::Saw, Shape::Ramp] {
            for deform in [-1.0, -0.4, 0.3, 1.0] {
                for i in 0..=200 {
                    let cycles = f64::from(i) / 200.0 * 0.999;
                    let plain = wave(shape, cycles, 0);
                    let bent = deformed_wave(shape, cycles, 0, deform);
                    assert!((-1.0..=1.0).contains(&bent), "{shape:?} {bent}");
                    // The peaks stay where they are, and it bends one way throughout.
                    if (plain.abs() - 1.0).abs() < 1e-6 {
                        close(bent, plain);
                    } else if plain.abs() < 0.99 {
                        assert!((bent - plain) * deform > 0.0, "{shape:?} {plain} {bent}");
                    }
                }
            }
        }
        // Raised, a tri spends longer high; lowered, longer low.
        let high = |deform: f32| {
            (0..1000)
                .filter(|i| deformed_wave(Shape::Tri, f64::from(*i) / 1000.0, 0, deform) > 0.0)
                .count()
        };
        assert!(
            high(0.5) > 600 && high(-0.5) < 400,
            "{} {}",
            high(0.5),
            high(-0.5)
        );
        // A square's high part is as wide as it says.
        let wide = |deform: f32| {
            (0..1000)
                .filter(|i| deformed_wave(Shape::Square, f64::from(*i) / 1000.0, 0, deform) > 0.0)
                .count()
        };
        assert_eq!(wide(0.0), 500);
        assert_eq!(wide(0.5), 750);
        assert_eq!(wide(-0.5), 250);
        // At none it's the wave to the bit, and the random shapes have nothing to bend.
        for shape in Shape::ALL {
            for i in 0..50 {
                let cycles = f64::from(i) * 0.137;
                assert_eq!(
                    deformed_wave(shape, cycles, 3, 0.0).to_bits(),
                    wave(shape, cycles, 3).to_bits()
                );
            }
        }
        assert_eq!(
            deformed_wave(Shape::Random, 2.3, 3, 0.8),
            wave(Shape::Random, 2.3, 3)
        );
    }

    #[test]
    fn a_deform_is_saved_only_once_set() {
        let plain = ron::to_string(&Lfo::default()).unwrap();
        assert!(!plain.contains("deform"), "{plain}");
        let bent = Lfo {
            deform: -0x40,
            ..Lfo::default()
        };
        let text = ron::to_string(&bent).unwrap();
        assert!(text.contains("deform:-64"), "{text}");
        assert_eq!(ron::from_str::<Lfo>(&text).unwrap(), bent);
        assert!(
            !Lfo {
                deform: 0x100,
                ..bent
            }
            .in_range()
        );
    }

    #[test]
    fn a_fade_reads_and_shows_as_a_signed_length() {
        let fade = |text: &str| text.parse::<Fade>().unwrap();
        assert_eq!(fade("off"), Fade::Off);
        assert_eq!(fade("+1 bar"), Fade::In(BAR_TICKS));
        assert_eq!(fade("1bar"), Fade::In(BAR_TICKS));
        assert_eq!(fade("-1/16"), Fade::Out(240));
        assert_eq!(fade("+2 bar"), Fade::In(2 * BAR_TICKS));
        assert_eq!(fade("-16 bars"), Fade::Out(16 * BAR_TICKS));
        for fade in Fade::ALL {
            assert!(fade.in_range());
            assert_eq!(fade.to_string().parse::<Fade>(), Ok(fade), "{fade}");
        }
        assert_eq!(Fade::In(BAR_TICKS).to_string(), "+1 bar");
        assert_eq!(Fade::Out(120).to_string(), "-1/32");
        for text in ["", "1/3", "+3 bars", "32 bars", "fast"] {
            assert!(text.parse::<Fade>().is_err(), "{text}");
        }
        assert!(!Fade::In(100).in_range());
    }

    #[test]
    fn a_fade_steps_through_out_none_and_in_and_stops_at_the_ends() {
        assert_eq!(Fade::Off.nudged(true, false), Fade::In(60));
        assert_eq!(Fade::Off.nudged(false, false), Fade::Out(60));
        assert_eq!(Fade::In(60).nudged(false, false), Fade::Off);
        assert_eq!(Fade::Out(60).nudged(true, false), Fade::Off);
        let longest = Fade::In(16 * BAR_TICKS);
        assert_eq!(longest.nudged(true, false), longest);
        assert_eq!(
            Fade::Out(16 * BAR_TICKS).nudged(false, true),
            Fade::Out(16 * BAR_TICKS)
        );
        // A large step goes by straight lengths.
        assert_eq!(Fade::In(240).nudged(true, true), Fade::In(480));
        assert_eq!(Fade::In(360).nudged(false, true), Fade::In(240));
        assert_eq!(Fade::In(60).nudged(false, true), Fade::Off);
        assert_eq!(Fade::Off.nudged(false, true), Fade::Out(60));
    }

    #[test]
    fn a_fade_in_rises_and_a_fade_out_falls_over_its_length_after_a_note() {
        let bar = f64::from(BAR_TICKS);
        let fade_in = Fade::In(BAR_TICKS);
        assert_eq!(fade_in.gain(Some(0.0)), 0.0);
        assert_eq!(fade_in.gain(Some(bar / 4.0)), 0.25);
        assert_eq!(fade_in.gain(Some(bar * 3.0)), 1.0);
        let fade_out = Fade::Out(BAR_TICKS);
        assert_eq!(fade_out.gain(Some(0.0)), 1.0);
        assert_eq!(fade_out.gain(Some(bar / 4.0)), 0.75);
        assert_eq!(fade_out.gain(Some(bar * 3.0)), 0.0);
        // With no note yet, it's as though the note were long ago.
        assert_eq!(fade_in.gain(None), 1.0);
        assert_eq!(fade_out.gain(None), 0.0);
        assert_eq!(Fade::Off.gain(Some(0.0)), 1.0);
    }

    #[test]
    fn the_modes_say_which_restart_and_which_play_once() {
        let restarts = LfoMode::ALL.map(LfoMode::restarts);
        assert_eq!(restarts, [false, true, false, true, true]);
        assert_eq!(LfoMode::One.plays_for(), Some(1.0));
        assert_eq!(LfoMode::Half.plays_for(), Some(0.5));
        assert_eq!(LfoMode::Hold.plays_for(), None);
        let lfo = Lfo {
            mode: LfoMode::Half,
            fade: Fade::Out(480),
            ..Lfo::default()
        };
        let text = ron::to_string(&lfo).unwrap();
        assert!(
            text.contains("mode:Half") && text.contains("fade:Out(480)"),
            "{text}"
        );
        assert_eq!(ron::from_str::<Lfo>(&text).unwrap(), lfo);
        assert!(!ron::to_string(&Lfo::default()).unwrap().contains("fade"));
    }
}
