//! `Walk` and `1/f`: lines that wander between the two values. `Walk` is Brownian, each value a
//! small step from the last, as Max's `drunk` is. `1/f` is pink, between `Random`'s white and a
//! walk's brown, which R. F. Voss and J. Clarke found listeners liked best ("1/f noise in music:
//! Music from 1/f noise", J. Acoust. Soc. Am. 63, 1978). It sums `depth` random values, the first
//! new on every turn and each after it half as often as the one before, staggered so only one of
//! those changes on a turn: Voss's algorithm as James McCartney staggered it.
//!
//! Both are worked out turn by turn from the seed alone, without allocating, so a live generator
//! can run them again on every pass.

use serde::{Deserialize, Serialize};

use super::{ShapeColumn, mix};

/// The fewest and most random values a `1/f` fill sums.
pub const MIN_DEPTH: u8 = 2;
pub const MAX_DEPTH: u8 = 8;

/// What a walk does at an end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Edge {
    /// Bounces back off it, as far as it would have gone past.
    #[default]
    Reflect,
    /// Comes round from the other end.
    Wrap,
}

impl Edge {
    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Reflect => "Reflect",
            Self::Wrap => "Wrap",
        }
    }

    /// The other one.
    pub fn other(self) -> Self {
        match self {
            Self::Reflect => Self::Wrap,
            Self::Wrap => Self::Reflect,
        }
    }
}

/// What a `Walk` fill moves by. Saved with a live generator's shape, and left out while it's as
/// it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Walk {
    /// The largest move from one value to the next, either way: on the Note column, in the
    /// scale's degrees while it's on and in semitones while it's off.
    pub step: u8,
    pub edge: Edge,
}

impl Default for Walk {
    fn default() -> Self {
        Self {
            step: 2,
            edge: Edge::Reflect,
        }
    }
}

impl Walk {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// How many random values a `1/f` fill sums. Saved with a live generator's shape, and left out
/// while it's as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Pink {
    /// From `MIN_DEPTH` to `MAX_DEPTH`: the more, the longer it takes to wander far.
    pub depth: u8,
}

impl Default for Pink {
    fn default() -> Self {
        Self { depth: 4 }
    }
}

impl Pink {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A walk's `Step` when the panel opens on a column: two degrees or semitones on the Note column,
/// and an eighth of the column's range on the others, such as `10` on Velocity, so a step can be
/// heard; on Nudge that's two 24ths at a sixteenth, and more on a slower track, whose range is
/// wider.
pub fn default_step(column: ShapeColumn) -> u8 {
    match column {
        ShapeColumn::Note { .. } => Walk::default().step,
        column => ((u16::from(column.highest()) + 1) / 8).max(1) as u8,
    }
}

/// What each draw is for, so one draw doesn't follow another.
const WALK: u64 = 0x3A1C_0001;
const PINK: u64 = 0x3A1C_0002;

/// A walk through whole values from `low` to `high`, a turn at a time, starting at `from` on turn
/// 0.
pub(super) struct Walker {
    walk: Walk,
    seed: u64,
    low: i32,
    high: i32,
    turn: usize,
    at: i32,
}

impl Walker {
    pub(super) fn new(walk: Walk, seed: u64, from: i32, low: i32, high: i32) -> Self {
        Self {
            walk,
            seed,
            low,
            high,
            turn: 0,
            at: from.clamp(low, high),
        }
    }

    /// Where it is on a turn, walking on from where it was. Turns are asked for in order.
    pub(super) fn at(&mut self, turn: usize) -> i32 {
        while self.turn < turn {
            self.turn += 1;
            let step = i32::from(self.walk.step);
            let moved = (mix(self.seed ^ WALK, self.turn) % (2 * step as u64 + 1)) as i32 - step;
            self.at = edged(self.walk.edge, self.at + moved, self.low, self.high);
        }
        self.at
    }
}

/// A value a move may have taken past an end, brought back between `low` and `high`.
fn edged(edge: Edge, value: i32, low: i32, high: i32) -> i32 {
    let span = high - low;
    if span <= 0 {
        return low;
    }
    match edge {
        Edge::Reflect => {
            // Folded back and forth, so even a step longer than the range lands inside it.
            let along = (value - low).rem_euclid(2 * span);
            low + if along > span {
                2 * span - along
            } else {
                along
            }
        }
        Edge::Wrap => low + (value - low).rem_euclid(span + 1),
    }
}

/// A `1/f` fill's value on a turn, from 0 to 1: the mean of `depth` random values, the first new
/// on every turn and the `k`th once every `2^k` turns, on the turns whose lowest set bit is the
/// `k - 1`th, so only one of those changes on each. A mean of more values keeps nearer the middle,
/// so its distance from the middle is widened to spread as one value does, and the few that would
/// go past an end, about one in twelve, stop there: at any depth it reaches as far as `Random`.
pub(super) fn pink(pink: Pink, seed: u64, turn: usize) -> f64 {
    let depth = pink.depth.clamp(MIN_DEPTH, MAX_DEPTH);
    let sum: f64 = (0..depth)
        .map(|k| {
            let changes = if k == 0 {
                turn
            } else {
                (turn + (1 << (k - 1))) >> k
            };
            let drawn = mix(mix(seed ^ PINK, usize::from(k)), changes);
            (drawn >> 11) as f64 / (1u64 << 53) as f64
        })
        .sum();
    let mean = sum / f64::from(depth);
    (0.5 + (mean - 0.5) * f64::from(depth).sqrt()).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lfo::Shape as Wave;
    use crate::scale::{Scale, ScaleKind};
    use crate::selection::StepRange;
    use crate::shape::{Acid, Bits, DEFAULT_RATE, FillKind, Follow, Shape, Values, Where, values};
    use crate::step::Step;

    fn walking(from: u8, to: u8, step: u8, edge: Edge, seed: u64) -> Shape {
        Shape {
            from,
            to,
            curve: 0,
            s: 0,
            kind: FillKind::Walk,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::Notes,
            seed,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk { step, edge },
            pink: Pink::default(),
            bits: Bits::default(),
            grids: crate::shape::Grids::default(),
            random: crate::shape::Random::default(),
            vary: 0,
            variation: crate::shape::Variation::default(),
        }
    }

    fn notes(count: usize) -> Vec<Step> {
        vec!["C-3 -- --- --- ---".parse().unwrap(); count]
    }

    fn all(steps: &[Step]) -> StepRange {
        StepRange::between(0, steps.len() - 1)
    }

    fn written(steps: &[Step], column: ShapeColumn, shape: &Shape) -> Vec<u8> {
        values(steps, all(steps), column, shape)
            .into_iter()
            .map(|(_, value)| value)
            .collect()
    }

    #[test]
    fn a_walk_starts_at_from_and_never_steps_further_than_step() {
        let steps = notes(400);
        for step in [1, 2, 5, 0x20] {
            for seed in 0..20 {
                let shape = walking(0x40, 0x20, step, Edge::Reflect, seed);
                let out = written(&steps, ShapeColumn::Velocity, &shape);
                assert_eq!(out[0], 0x40);
                assert!(out.iter().all(|value| (0x20..=0x40).contains(value)));
                assert!(
                    out.windows(2).all(|pair| pair[0].abs_diff(pair[1]) <= step),
                    "a step of {step} on seed {seed}: {out:?}"
                );
            }
        }
        // It wanders: over many rows it reaches both ends, and doesn't sit still.
        let out = written(
            &steps,
            ShapeColumn::Velocity,
            &walking(0x40, 0x20, 4, Edge::Reflect, 1),
        );
        assert!(out.contains(&0x20) && out.contains(&0x40), "{out:?}");
        assert!(out.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn reflect_bounces_off_an_end_and_wrap_comes_round() {
        assert_eq!(edged(Edge::Reflect, 12, 0, 10), 8);
        assert_eq!(edged(Edge::Reflect, -3, 0, 10), 3);
        assert_eq!(edged(Edge::Reflect, 25, 0, 10), 5);
        assert_eq!(edged(Edge::Wrap, 11, 0, 10), 0);
        assert_eq!(edged(Edge::Wrap, -1, 0, 10), 10);
        assert_eq!(edged(Edge::Wrap, 4, 0, 10), 4);
        assert_eq!(edged(Edge::Reflect, 4, 3, 3), 3);
        // Wrapped, a move is never more than `Step` going round the range.
        let steps = notes(400);
        let out = written(
            &steps,
            ShapeColumn::Velocity,
            &walking(0x10, 0x18, 3, Edge::Wrap, 7),
        );
        assert!(out.iter().all(|value| (0x10..=0x18).contains(value)));
        assert!(out.windows(2).all(|pair| {
            let apart = pair[0].abs_diff(pair[1]);
            apart.min(9 - apart) <= 3
        }));
        assert!(
            out.windows(2).any(|pair| pair[0].abs_diff(pair[1]) > 3),
            "it went round"
        );
    }

    #[test]
    fn a_walk_of_notes_steps_in_degrees_with_the_scale_on() {
        let scale = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        let column = ShapeColumn::Note {
            scale,
            writes: None,
        };
        let steps = notes(300);
        let shape = walking(60, 84, 1, Edge::Reflect, 3);
        let out = written(&steps, column, &shape);
        assert!(out.iter().all(|note| scale.contains(*note)));
        assert!(out.iter().all(|note| (60..=84).contains(note)));
        // One degree is a tone or a semitone, never more.
        assert!(out.windows(2).all(|pair| pair[0].abs_diff(pair[1]) <= 2));
        assert!(out.windows(2).any(|pair| pair[0].abs_diff(pair[1]) == 2));
        // Off, a step is a semitone.
        let chromatic = ShapeColumn::Note {
            scale: Scale { on: false, ..scale },
            writes: None,
        };
        let out = written(&steps, chromatic, &shape);
        assert!(out.windows(2).all(|pair| pair[0].abs_diff(pair[1]) <= 1));
        assert!(out.iter().any(|note| !scale.contains(*note)));
    }

    #[test]
    fn a_seed_repeats_and_another_wanders_elsewhere() {
        let steps = notes(64);
        for kind in [FillKind::Walk, FillKind::Pink] {
            let shape = Shape {
                kind,
                ..walking(0x00, 0x7F, 0x10, Edge::Reflect, 9)
            };
            let out = written(&steps, ShapeColumn::Velocity, &shape);
            assert_eq!(out, written(&steps, ShapeColumn::Velocity, &shape));
            let other = Shape { seed: 10, ..shape };
            assert_ne!(out, written(&steps, ShapeColumn::Velocity, &other));
        }
    }

    #[test]
    fn a_run_holds_each_value_of_a_walk_and_of_1_f() {
        let steps = notes(32);
        for kind in [FillKind::Walk, FillKind::Pink] {
            let shape = Shape {
                kind,
                run: 4,
                ..walking(0x00, 0x7F, 0x10, Edge::Reflect, 2)
            };
            let out = written(&steps, ShapeColumn::Velocity, &shape);
            assert!(out.chunks(4).all(|run| run.iter().all(|v| *v == run[0])));
        }
    }

    #[test]
    fn one_over_f_reaches_both_ends_at_any_depth_either_way_round() {
        let steps = notes(500);
        for (from, to) in [(0x10, 0x60), (0x60, 0x10)] {
            for depth in MIN_DEPTH..=MAX_DEPTH {
                let shape = Shape {
                    kind: FillKind::Pink,
                    pink: Pink { depth },
                    ..walking(from, to, 1, Edge::Reflect, u64::from(depth))
                };
                let out = written(&steps, ShapeColumn::Velocity, &shape);
                assert!(out.iter().all(|value| (0x10..=0x60).contains(value)));
                assert!(out.contains(&0x10) && out.contains(&0x60), "depth {depth}");
                // About one in twelve stops at an end, whatever the depth, over many seeds.
                let ends: usize = (0..20)
                    .map(|seed| {
                        let out = written(&steps, ShapeColumn::Velocity, &Shape { seed, ..shape });
                        out.iter().filter(|v| **v == 0x10 || **v == 0x60).count()
                    })
                    .sum();
                let share = ends as f64 / (20 * steps.len()) as f64;
                assert!(
                    (0.06..=0.12).contains(&share),
                    "{share} at the ends at depth {depth}"
                );
            }
        }
    }

    /// The power in each octave of a signal's spectrum, by a plain DFT, from the octave of bins
    /// `1..2` up to the one ending at half the turns.
    fn octaves(signal: &[f64]) -> Vec<f64> {
        let count = signal.len();
        let mean = signal.iter().sum::<f64>() / count as f64;
        let power = |bin: usize| {
            let (mut re, mut im) = (0.0, 0.0);
            for (turn, value) in signal.iter().enumerate() {
                let angle = std::f64::consts::TAU * (bin * turn) as f64 / count as f64;
                re += (value - mean) * angle.cos();
                im -= (value - mean) * angle.sin();
            }
            re * re + im * im
        };
        let mut bands = Vec::new();
        let mut low = 1;
        while low * 2 <= count / 2 {
            let band: f64 = (low..low * 2).map(power).sum::<f64>() / low as f64;
            bands.push(band);
            low *= 2;
        }
        bands
    }

    /// The least-squares slope of a line through these, one a unit apart.
    fn slope(values: &[f64]) -> f64 {
        let count = values.len() as f64;
        let mean_x = (count - 1.0) / 2.0;
        let mean_y = values.iter().sum::<f64>() / count;
        let (mut across, mut spread) = (0.0, 0.0);
        for (x, y) in values.iter().enumerate() {
            across += (x as f64 - mean_x) * (y - mean_y);
            spread += (x as f64 - mean_x).powi(2);
        }
        across / spread
    }

    #[test]
    fn one_over_f_falls_about_3_db_an_octave_where_walk_falls_6_and_random_none() {
        const TURNS: usize = 1024;
        const SEEDS: u64 = 48;
        let averaged = |signal: &dyn Fn(u64, usize) -> f64| -> Vec<f64> {
            let mut total = Vec::new();
            for seed in 0..SEEDS {
                let one: Vec<f64> = (0..TURNS).map(|turn| signal(seed, turn)).collect();
                let bands = octaves(&one);
                total.resize(bands.len(), 0.0);
                for (sum, band) in total.iter_mut().zip(bands) {
                    *sum += band;
                }
            }
            total.iter().map(|power| 10.0 * power.log10()).collect()
        };
        // Its depth's generators cover eight octaves, so it's pink from the lowest it reaches up.
        let pinked = averaged(&|seed, turn| pink(Pink { depth: 8 }, seed, turn));
        let falls = slope(&pinked[1..]);
        assert!(
            (-3.5..=-2.5).contains(&falls),
            "1/f falls {falls:.2} dB an octave: {pinked:?}"
        );
        // Against white, which stays flat, and a walk, which falls about 6.
        let white = averaged(&|seed, turn| (mix(seed, turn) >> 11) as f64);
        let flat = slope(&white[1..]);
        assert!(flat.abs() < 1.0, "white falls {flat:.2} dB an octave");
        let brown = averaged(&|seed, turn| {
            let mut walker = Walker::new(
                Walk {
                    step: 4,
                    edge: Edge::Reflect,
                },
                seed,
                5000,
                0,
                10_000,
            );
            f64::from(walker.at(turn))
        });
        let steep = slope(&brown[2..]);
        assert!(
            (-7.5..=-4.5).contains(&steep),
            "a walk falls {steep:.2} dB an octave"
        );
    }

    #[test]
    fn more_depth_wanders_more_slowly() {
        // The mean move from one turn to the next shrinks as more values are summed.
        let moves = |depth| {
            let mut total = 0.0;
            for seed in 0..20 {
                for turn in 1..512 {
                    let depth = Pink { depth };
                    total += (pink(depth, seed, turn) - pink(depth, seed, turn - 1)).abs();
                }
            }
            total
        };
        assert!(moves(2) > moves(4) && moves(4) > moves(8));
    }

    #[test]
    fn default_steps_suit_their_columns() {
        let scale = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        assert_eq!(
            default_step(ShapeColumn::Note {
                scale,
                writes: None
            }),
            2
        );
        assert_eq!(default_step(ShapeColumn::Nudge { most: 11 }), 2);
        // A slower track's range is wider, and so is its first step.
        let steps: Vec<u8> = crate::speed::Speed::ALL
            .iter()
            .map(|speed| {
                default_step(ShapeColumn::Nudge {
                    most: speed.most_nudge(),
                })
            })
            .collect();
        assert_eq!(steps, [1, 1, 2, 3, 5, 7, 11]);
        assert_eq!(default_step(ShapeColumn::Velocity), 0x10);
        assert_eq!(default_step(ShapeColumn::Length), 0x20);
    }
}
