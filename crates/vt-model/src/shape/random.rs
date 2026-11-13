//! `Random`'s values shaped as Mutable Instruments' Marbles shapes its X outputs'. `Spread` takes
//! them from one value, through a bell narrowing round it, an even draw and a bowl, to the two
//! ends alone; `Bias` leans them toward the low end or the high. On the Note column, `Steps` lets
//! through only the most important of the scale's degrees, the root first, then the fifth, then
//! the third, and `Weights` gives each degree a chance of its own, as Vermona's meloDICER gives
//! each semitone a fader.
//!
//! Marbles draws from a beta distribution through tables worked out ahead
//! (`marbles/random/distributions.h`). Here an even draw is bent by Schlick's bias and gain, which
//! `Curve` and `S` bend a ramp with: gain pulls it toward the middle or out to the ends, and bias
//! moves it toward the bias point. Up to noon, bias moves the bell gain makes, so its middle, the
//! value as many draws fall below as above, is the bias point, and `Spread` 0 is the bias point
//! alone, as on Marbles; past noon, bias moves the draw before gain makes a bowl of it, so the
//! share falling in the top half is the bias point, and `Spread` 100 is the two ends, the high one
//! as often as the bias point is high, as on Marbles. At noon the two are the same, an even draw
//! leaned by bias alone. Bias and gain undo themselves with their parameters turned round, so the
//! chance of each value is worked out exactly, which weighing the notes needs, from a few
//! divisions, the same on every machine.
//!
//! How much each note matters in a key is adapted from Marbles' scales' degree weights
//! (`marbles/settings.cc`, `preset_scales`), whose major and minor scales give the root 255, the
//! fifth 192, the third and the seventh 128, the second and the sixth 96 and the fourth 64:
//!
//! > Copyright 2015 Emilie Gillet.
//! >
//! > Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! > and associated documentation files (the "Software"), to deal in the Software without
//! > restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! > distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! > Software is furnished to do so, subject to the following conditions:
//! >
//! > The above copyright notice and this permission notice shall be included in all copies or
//! > substantial portions of the Software.
//! >
//! > THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! > BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! > NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! > DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! > OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use serde::{Deserialize, Serialize};

use super::{bias, gain};
use crate::scale::{ROOTS, Scale};
use crate::step::MAX_NOTE;

/// How many weights a `Random` keeps: one for each semitone from the root, so enough for any
/// scale's degrees.
pub const WEIGHTS: usize = 12;
/// The heaviest weight, a hex digit's top.
pub const MAX_WEIGHT: u8 = 0xF;
/// The `Spread` of an even draw, and the widest.
pub const EVEN_SPREAD: u8 = 50;
pub const MAX_SPREAD: u8 = 100;
/// The furthest `Bias` goes either way.
pub const MAX_BIAS: i8 = 100;

/// How close to an end the bias point goes, so bias stays a curve. Near enough that `Spread` 0
/// at full `Bias` writes the end itself on any column, up to 256 values.
const NEAREST_END: f64 = 0.001;

/// How a `Random` fill draws its values. Saved with a live generator's shape, and left out while
/// it's as it starts, which draws as `Random` always has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Random {
    /// From 0, one value, at the bias point, through a bell, to `EVEN_SPREAD`, every value as
    /// likely, and on through a bowl to `MAX_SPREAD`, the two ends alone.
    pub spread: u8,
    /// Where the values lean, from `-MAX_BIAS`, the low end, to `MAX_BIAS`, the high.
    pub bias: i8,
    /// On the Note column, how many of the scale's degrees it lets through, the most important
    /// first, or of the twelve semitones with the scale off; 0 lets all of them through.
    pub steps: u8,
    /// On the Note column, each degree's weight, from the root up, or each semitone's with the
    /// scale off, from 0, never, to `MAX_WEIGHT`.
    pub weights: [u8; WEIGHTS],
}

impl Default for Random {
    fn default() -> Self {
        Self {
            spread: EVEN_SPREAD,
            bias: 0,
            steps: 0,
            weights: [MAX_WEIGHT; WEIGHTS],
        }
    }
}

impl Random {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether its draw is other than even, so it isn't drawn as `Random` always was.
    pub fn shaped(&self) -> bool {
        self.spread != EVEN_SPREAD || self.bias != 0
    }

    /// The bias point, from 0 to 1, just short of the ends.
    fn centre(&self) -> f64 {
        (0.5 + f64::from(self.bias) / 200.0).clamp(NEAREST_END, 1.0 - NEAREST_END)
    }

    /// Gain's parameter for a spread between the two ends: 1 would be the bias point alone, a half
    /// even, and 0 the ends alone. Below noon it goes by the spread's square, so the bell's width,
    /// how far its values stray, grows about evenly with it, as it does above noon without.
    fn bend(&self) -> f64 {
        let spread = f64::from(self.spread);
        let even = f64::from(EVEN_SPREAD);
        if spread < even {
            1.0 - 0.5 * (spread / even).powi(2)
        } else {
            1.0 - spread / f64::from(MAX_SPREAD)
        }
    }

    /// Where an even draw `u`, from 0 to just below 1, lands, from 0 to 1: a bell moved to the
    /// bias point up to noon, and past it the draw moved before it's made a bowl.
    pub fn draw(&self, u: f64) -> f64 {
        let centre = self.centre();
        match self.spread {
            0 => centre,
            1..EVEN_SPREAD => bias(gain(u, self.bend()), centre),
            MAX_SPREAD.. if u >= 1.0 - centre => 1.0,
            MAX_SPREAD.. => 0.0,
            _ => gain(bias(u, centre), self.bend()),
        }
    }

    /// The chance a draw lands below `t`, from 0 to 1: `draw` undone, as bias and gain undo
    /// themselves with their parameters turned round.
    pub fn below(&self, t: f64) -> f64 {
        let centre = self.centre();
        let t = t.clamp(0.0, 1.0);
        match self.spread {
            0 if t > centre => 1.0,
            0 => 0.0,
            1..EVEN_SPREAD => gain(bias(t, 1.0 - centre), 1.0 - self.bend()),
            MAX_SPREAD.. if t <= 0.0 => 0.0,
            MAX_SPREAD.. => 1.0 - centre,
            _ => bias(gain(t, 1.0 - self.bend()), 1.0 - centre),
        }
    }

    /// Which of `span` whole values an even draw lands on, from the lowest.
    pub fn pick(&self, u: f64, span: usize) -> usize {
        ((self.draw(u) * span as f64) as usize).min(span.saturating_sub(1))
    }

    /// How many of a scale's degrees `Steps` lets through.
    pub fn kept(&self, scale: Scale) -> usize {
        let degrees = degree_count(scale);
        match usize::from(self.steps) {
            0 => degrees,
            steps => steps.min(degrees),
        }
    }

    /// Whether it weighs the notes: `Steps` leaves a degree out, or the degrees' weights differ.
    pub fn weighs(&self, scale: Scale) -> bool {
        let weights = &self.weights[..degree_count(scale)];
        self.kept(scale) < weights.len() || weights.iter().any(|weight| *weight != weights[0])
    }

    /// A note's weight: its degree's, or none if it isn't one `Steps` lets through.
    fn weight_of(&self, scale: Scale, order: &Degrees, note: u8) -> u8 {
        let Some(degree) = degree_of(scale, note) else {
            return 0;
        };
        if order.rank(degree) < self.kept(scale) {
            self.weights[degree]
        } else {
            0
        }
    }

    /// The weights as typed, a hex digit for each of a scale's degrees from the root.
    pub fn weights_text(&self, scale: Scale) -> String {
        self.weights[..degree_count(scale)]
            .iter()
            .map(|weight| format!("{:X}", weight.min(&MAX_WEIGHT)))
            .collect()
    }
}

/// How much each note matters in a key, by semitones above its root: Marbles' weights for its
/// major and minor scales, made each a little different where they tie, so the major third comes
/// before the minor, a seventh before a sixth, a sixth before the second, the major ones first,
/// and a minor second and a tritone, which neither scale has, last.
const IN_KEY: [u8; WEIGHTS] = [255, 16, 90, 127, 128, 64, 8, 192, 95, 96, 111, 112];

/// The twelve semitones above the root by how consonant each is with it, the simplest ratio
/// first, as Tenney's harmonic distance orders the just intervals: the fifth (3:2), the fourth
/// (4:3), the major sixth (5:3), the major third (5:4), the minor third (6:5), the minor sixth
/// (8:5), the minor seventh (9:5), the major second (9:8), the major seventh (15:8), the minor
/// second (16:15) and the tritone (45:32).
const CONSONANCE: [u8; WEIGHTS] = [0, 7, 5, 9, 4, 3, 8, 10, 2, 11, 1, 6];

/// How many degrees a scale has for `Steps` and `Weights`: its own while it's on, and the twelve
/// semitones while it's off.
pub fn degree_count(scale: Scale) -> usize {
    if scale.on {
        scale.kind.intervals().len()
    } else {
        WEIGHTS
    }
}

/// The degree a note is, counted from the root: its place in the scale while it's on, if it's in
/// it, and otherwise its semitones above the root.
fn degree_of(scale: Scale, note: u8) -> Option<usize> {
    let above = (usize::from(note) + WEIGHTS - usize::from(scale.root) % WEIGHTS) % WEIGHTS;
    if scale.on {
        scale
            .kind
            .intervals()
            .iter()
            .position(|interval| usize::from(*interval) == above)
    } else {
        Some(above)
    }
}

/// A scale's degrees, the most important first: in key, by Marbles' weights; with the scale off,
/// the semitones by consonance with the root.
#[derive(Debug, Clone, Copy)]
pub struct Degrees {
    order: [u8; WEIGHTS],
    len: usize,
}

impl Degrees {
    pub fn of(scale: Scale) -> Self {
        if !scale.on {
            return Self {
                order: CONSONANCE,
                len: WEIGHTS,
            };
        }
        let intervals = scale.kind.intervals();
        let mut order = [0; WEIGHTS];
        for (degree, place) in order.iter_mut().enumerate() {
            *place = degree as u8;
        }
        let len = intervals.len().min(WEIGHTS);
        // Each weight differs, so the order is the same however it's sorted, and an unstable sort
        // allocates nothing.
        order[..len].sort_unstable_by_key(|degree| {
            std::cmp::Reverse(IN_KEY[usize::from(intervals[usize::from(*degree)]) % WEIGHTS])
        });
        Self { order, len }
    }

    /// The degrees, the most important first.
    pub fn order(&self) -> &[u8] {
        &self.order[..self.len]
    }

    /// Where a degree comes among them, from 0 for the most important.
    fn rank(&self, degree: usize) -> usize {
        self.order()
            .iter()
            .position(|each| usize::from(*each) == degree)
            .unwrap_or(self.len)
    }
}

/// A degree's name: the note it is in the song's key, as `G` for the fifth of C.
pub fn degree_name(scale: Scale, degree: usize) -> &'static str {
    let above = if scale.on {
        scale.kind.intervals().get(degree).copied().unwrap_or(0)
    } else {
        degree as u8
    };
    ROOTS[(usize::from(scale.root) + usize::from(above)) % WEIGHTS]
}

/// How many notes a column can step through, the most `Odds` holds.
const NOTES: usize = MAX_NOTE as usize + 1;

/// The chance of each note a weighed `Random` can write, worked out once a render, held in place
/// so it allocates nothing: each note's chance from `Spread` and `Bias`, times its degree's weight.
pub(super) struct Odds {
    /// The weighed chances of each note and those below it.
    up_to: [f64; NOTES],
    /// Each note's own weight, for a draw whose odds miss every note let through.
    weight: [u8; NOTES],
    len: usize,
}

impl Odds {
    /// The odds of `notes`, the ones between the two ends, from the lowest.
    pub(super) fn new(random: &Random, scale: Scale, notes: &[u8]) -> Self {
        let order = Degrees::of(scale);
        let len = notes.len().min(NOTES);
        let mut odds = Self {
            up_to: [0.0; NOTES],
            weight: [0; NOTES],
            len,
        };
        let (mut sum, mut below) = (0.0, 0.0);
        for (at, note) in notes.iter().take(len).enumerate() {
            let weight = random.weight_of(scale, &order, *note);
            // The last note takes the top end itself.
            let up_to = if at + 1 == len {
                1.0
            } else {
                random.below((at + 1) as f64 / len as f64)
            };
            sum += (up_to - below).max(0.0) * f64::from(weight);
            below = up_to;
            odds.up_to[at] = sum;
            odds.weight[at] = weight;
        }
        odds
    }

    /// Which note an even draw `u` lands on, from the lowest. Where the draw's own odds miss every
    /// note let through, as `Spread` 0 on one left out does, it's the nearest one let through to
    /// where it lands, the lower of two as near, and with none let through, where it lands.
    pub(super) fn pick(&self, random: &Random, u: f64) -> usize {
        let len = self.len.max(1);
        let total = self.up_to[len - 1];
        if total > 0.0 {
            let target = u * total;
            return self.up_to[..len]
                .partition_point(|sum| *sum <= target)
                .min(len - 1);
        }
        let at = random.pick(u, len);
        (0..len)
            .flat_map(|distance| [at.checked_sub(distance), at.checked_add(distance)])
            .flatten()
            .find(|near| *near < len && self.weight[*near] > 0)
            .unwrap_or(at)
    }
}

/// An even draw from 0 to just below 1, from a row's mixed number.
pub(super) fn unit(mixed: u64) -> f64 {
    (mixed >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    fn spread(spread: u8, bias: i8) -> Random {
        Random {
            spread,
            bias,
            ..Random::default()
        }
    }

    fn even(at: usize, of: usize) -> f64 {
        (at as f64 + 0.5) / of as f64
    }

    #[test]
    fn noon_is_an_even_draw_and_below_undoes_draw() {
        let random = Random::default();
        for at in 0..100 {
            let u = even(at, 100);
            assert!((random.draw(u) - u).abs() < 1e-12);
        }
        for spread_at in [1, 10, 25, 50, 75, 90, 99] {
            for bias_at in [-90, -30, 0, 40, 100] {
                let random = spread(spread_at, bias_at);
                for at in 0..50 {
                    let u = even(at, 50);
                    let back = random.below(random.draw(u));
                    assert!((back - u).abs() < 1e-6, "{spread_at} {bias_at}: {u} {back}");
                }
            }
        }
    }

    #[test]
    fn the_ends_of_spread_are_the_bias_point_and_the_two_ends() {
        let one = spread(0, 40);
        assert!((0..20).all(|at| (one.draw(even(at, 20)) - 0.7).abs() < 1e-12));
        let ends = spread(MAX_SPREAD, 40);
        let high = (0..1000)
            .filter(|at| ends.draw(even(*at, 1000)) == 1.0)
            .count();
        assert!((0..1000).all(|at| matches!(ends.draw(even(at, 1000)), 0.0 | 1.0)));
        assert_eq!(high, 700, "the high end as often as the bias point is high");
        // At full bias, the end itself, on a column of 256 values.
        assert_eq!(spread(0, MAX_BIAS).pick(0.3, 256), 255);
        assert_eq!(spread(0, -MAX_BIAS).pick(0.3, 256), 0);
    }

    #[test]
    fn in_key_the_root_then_the_fifth_then_the_third() {
        let scale = |kind| Scale {
            on: true,
            root: 0,
            kind,
        };
        let names = |scale: Scale| -> Vec<&str> {
            Degrees::of(scale)
                .order()
                .iter()
                .map(|degree| degree_name(scale, usize::from(*degree)))
                .collect()
        };
        assert_eq!(
            names(scale(ScaleKind::Major)),
            ["C", "G", "E", "B", "A", "D", "F"]
        );
        assert_eq!(
            names(scale(ScaleKind::Minor)),
            ["C", "G", "D#", "A#", "G#", "D", "F"]
        );
        assert_eq!(
            names(scale(ScaleKind::MinorPentatonic)),
            ["C", "G", "D#", "A#", "F"]
        );
        assert_eq!(
            names(scale(ScaleKind::Blues)),
            ["C", "G", "D#", "A#", "F", "F#"]
        );
        let off = Scale {
            on: false,
            root: 9,
            kind: ScaleKind::Major,
        };
        assert_eq!(
            names(off),
            [
                "A", "E", "D", "F#", "C#", "C", "F", "G", "B", "G#", "A#", "D#"
            ]
        );
    }

    #[test]
    fn weighing_starts_once_steps_or_weights_say() {
        let scale = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::MajorPentatonic,
        };
        let mut random = Random::default();
        assert!(!random.weighs(scale));
        random.weights[7] = 0;
        assert!(!random.weighs(scale), "past the scale's five degrees");
        random.weights = [3; WEIGHTS];
        assert!(!random.weighs(scale), "all alike is even");
        random.weights[2] = 9;
        assert!(random.weighs(scale));
        let mut steps = Random {
            steps: 5,
            ..Random::default()
        };
        assert!(!steps.weighs(scale), "all five let through");
        steps.steps = 4;
        assert!(steps.weighs(scale));
        assert_eq!(steps.kept(scale), 4);
        assert_eq!(random.weights_text(scale), "33933");
    }
}
