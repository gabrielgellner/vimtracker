//! Strange attractors for an LFO's `Lorenz` and `Rössler` shapes: smooth motion that never quite
//! repeats, as Ornament & Crime's Low-rents, Nonlinear Circuits' Sloth and Joranalogue's Orbit 3
//! give a patch, and as the easter egg in Mutable Instruments' Streams does
//! (`streams/lorenz_generator.cc`, Emilie Gillet, MIT; read, not copied: it steps Lorenz's
//! equations once a sample with Euler's method from `(0.1, 0, 0)` at ρ = 28, and swaps its two
//! outputs' axes on its second channel for variety).
//!
//! Each is three equations whose point goes round and round without ever retracing its path.
//! Here the point is stepped on by the fourth-order Runge-Kutta method, [`STEPS_PER_CYCLE`] steps
//! to a cycle of the LFO's rate, and a cycle is about one turn of the attractor: one turn round a
//! wing of Lorenz's butterfly, or round Rössler's loop, at any `Deform`. Between two steps it
//! follows the cubic through both with the slopes the equations give there, so it's smooth however
//! slow the rate. The steps are counted from where it was seeded, in the LFO's own cycles, so how
//! far it gets depends only on the song's position and never on the audio's blocks, and a render
//! repeats.
//!
//! `Deform` is the parameter that makes each chaotic: Lorenz's ρ, from 24.8 at `-FF`, where it
//! circles a wing longest, through the textbook 28 at `00`, then ever busier chaos and windows
//! round 100 and 160 where it loops, to 220 at `+FF`, where it has settled into a regular figure
//! of eight; and Rössler's c, from 2.5 at `-FF`, a plain loop, through its doublings into chaos to
//! the textbook 5.7 at `00`, and on to 18 at `+FF`, wilder still. Lorenz keeps its σ of 10 and β
//! of 8/3, and Rössler its a and b of 0.2.
//!
//! Each axis is scaled to -1..1 from the range it covers at that parameter, measured once over
//! 3,000 turns at every eighth of the way along `Deform` and kept in `LORENZ` and `ROSSLER`, then
//! kept within it, so its rare furthest reaches touch the ends and never pass them.

use crate::lfo::Shape;
use crate::random::splitmix64;

/// How many steps the point takes in a cycle of the LFO's rate.
pub const STEPS_PER_CYCLE: f64 = 128.0;

/// The most steps a working out takes the point on by. Further than this behind, as a slot that
/// sat still for many cycles or `Hold` between notes far apart, it's seeded again where it should
/// be, so the audio thread's work stays bounded: eight cycles.
pub const MAX_CATCH_UP: i64 = 1024;

/// How many cycles a seed runs before it's heard, so it's on the attractor: one, and up to three
/// more by the key, so tracks and slots start in different places.
const SETTLE_CYCLES: f64 = 1.0;
const KEYED_CYCLES: f64 = 3.0;

/// One of the attractors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Lorenz,
    Rossler,
}

/// How much wider than what was measured each axis is taken to go, for the places between those
/// measured, where it can go a little further than either side.
const MARGIN: f64 = 1.06;

/// What's measured of an attractor at one place along `Deform`: how long it takes to go round
/// once, and the lowest and highest each axis goes.
#[derive(Debug, Clone, Copy)]
struct Measured {
    turn: f64,
    axes: [(f64, f64); 3],
}

/// What's measured, in a line.
const fn measured(turn: f64, x: (f64, f64), y: (f64, f64), z: (f64, f64)) -> Measured {
    Measured {
        turn,
        axes: [x, y, z],
    }
}

/// Lorenz's, at every eighth of the way along `Deform`, from `-FF` to `+FF`: the time between the
/// highest points of `z`, once a turn, and how far each axis went over 3,000 turns, with `x` and
/// `y` the same either way, as the attractor is the same turned round its `z` axis.
const LORENZ: [Measured; 9] = [
    measured(0.8000, (-17.52, 17.52), (-23.41, 23.41), (2.02, 41.10)),
    measured(0.7888, (-17.93, 17.93), (-24.15, 24.15), (2.09, 42.57)),
    measured(0.7786, (-18.34, 18.34), (-24.89, 24.89), (2.18, 44.04)),
    measured(0.7654, (-18.99, 18.99), (-26.09, 26.09), (1.50, 46.00)),
    measured(0.7431, (-19.32, 19.32), (-26.70, 26.70), (1.76, 47.34)),
    measured(0.5500, (-26.35, 26.35), (-40.94, 40.94), (8.12, 78.99)),
    measured(0.4180, (-37.31, 37.31), (-68.34, 68.34), (13.08, 136.09)),
    measured(0.3203, (-46.09, 46.09), (-93.99, 93.99), (45.46, 211.97)),
    measured(0.2458, (-48.25, 48.25), (-98.68, 98.68), (140.86, 300.78)),
];

/// Rössler's, the same way, its turn the time between the highest points of `x`. At `+FF` its `z`
/// went to 91.37, less than at `+C0`, so it keeps that one's, which it reaches in between.
const ROSSLER: [Measured; 9] = [
    measured(5.7489, (-3.71, 4.73), (-4.43, 3.09), (0.033, 3.20)),
    measured(5.7653, (-4.86, 6.18), (-5.79, 4.15), (0.025, 6.20)),
    measured(5.7835, (-5.94, 7.56), (-7.06, 5.10), (0.021, 9.25)),
    measured(5.8271, (-7.42, 9.37), (-8.80, 6.38), (0.017, 15.13)),
    measured(5.8594, (-9.11, 11.43), (-10.79, 7.84), (0.014, 22.85)),
    measured(5.8993, (-12.08, 14.97), (-14.31, 10.41), (0.010, 39.08)),
    measured(5.8131, (-15.99, 19.47), (-18.92, 13.78), (0.008, 63.97)),
    measured(5.6638, (-21.11, 25.18), (-24.98, 18.21), (0.006, 100.96)),
    measured(6.0762, (-23.38, 29.38), (-27.67, 20.17), (0.005, 100.96)),
];

impl System {
    /// The attractor a shape follows, if it's one.
    pub fn of(shape: Shape) -> Option<Self> {
        match shape {
            Shape::Lorenz => Some(Self::Lorenz),
            Shape::Rossler => Some(Self::Rossler),
            _ => None,
        }
    }

    /// Its chaotic parameter at a deform from -1 to 1: Lorenz's ρ, or Rössler's c. It goes evenly
    /// in ratio either side of the textbook value at none.
    pub fn parameter(self, deform: f64) -> f64 {
        let deform = deform.clamp(-1.0, 1.0);
        let (low, middle, high) = match self {
            Self::Lorenz => (24.8, 28.0, 220.0),
            Self::Rossler => (2.5, 5.7, 18.0),
        };
        let end: f64 = if deform < 0.0 { low } else { high };
        middle * (end / middle).powf(deform.abs())
    }

    /// Which way the point moves, and how fast, from `at` with this parameter.
    #[inline]
    fn slope(self, [x, y, z]: [f64; 3], parameter: f64) -> [f64; 3] {
        match self {
            Self::Lorenz => [
                10.0 * (y - x),
                x * (parameter - z) - y,
                x * y - 8.0 / 3.0 * z,
            ],
            Self::Rossler => [-y - z, x + 0.2 * y, 0.2 + z * (x - parameter)],
        }
    }

    /// What was measured nearest a deform, between the two places either side of it.
    fn measured(self, deform: f64) -> Measured {
        let table = match self {
            Self::Lorenz => &LORENZ,
            Self::Rossler => &ROSSLER,
        };
        let along = (deform.clamp(-1.0, 1.0) + 1.0) * 4.0;
        let low = (along.floor() as usize).min(table.len() - 2);
        let share = along - low as f64;
        let (a, b) = (table[low], table[low + 1]);
        let mix = |from: f64, to: f64| from + (to - from) * share;
        Measured {
            turn: mix(a.turn, b.turn),
            axes: std::array::from_fn(|axis| {
                (
                    mix(a.axes[axis].0, b.axes[axis].0),
                    mix(a.axes[axis].1, b.axes[axis].1),
                )
            }),
        }
    }

    /// A point on its attractor at none, where every seed starts before it settles: where 50 of
    /// Lorenz's own seconds from `(1, 1, 1)` and 500 of Rössler's from `(1, 1, 0)` took them. It's
    /// well away from the points each spirals out from, which it would take many cycles to leave.
    fn start(self) -> [f64; 3] {
        match self {
            Self::Lorenz => [-4.41, -3.32, 24.1],
            Self::Rossler => [-5.51, 6.66, 0.019],
        }
    }

    /// One step on with this parameter, `seconds` of the attractor's own time: the fourth-order
    /// Runge-Kutta method.
    #[inline]
    fn step(self, at: [f64; 3], seconds: f64, parameter: f64) -> [f64; 3] {
        let along = |from: [f64; 3], slope: [f64; 3], by: f64| {
            std::array::from_fn(|axis| from[axis] + slope[axis] * by)
        };
        let k1 = self.slope(at, parameter);
        let k2 = self.slope(along(at, k1, seconds / 2.0), parameter);
        let k3 = self.slope(along(at, k2, seconds / 2.0), parameter);
        let k4 = self.slope(along(at, k3, seconds), parameter);
        std::array::from_fn(|axis| {
            at[axis] + seconds / 6.0 * (k1[axis] + 2.0 * k2[axis] + 2.0 * k3[axis] + k4[axis])
        })
    }
}

/// How the point steps at one deform: its parameter, and how much of the attractor's own time a
/// step is.
#[derive(Debug, Clone, Copy)]
struct Pace {
    parameter: f64,
    seconds: f64,
}

impl Pace {
    fn of(system: System, deform: f32) -> Self {
        let deform = f64::from(deform);
        Self {
            parameter: system.parameter(deform),
            seconds: system.measured(deform).turn / STEPS_PER_CYCLE,
        }
    }
}

/// A point the attractor has stepped to, and its slope there per step.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Point {
    at: [f64; 3],
    slope: [f64; 3],
}

impl Point {
    fn of(system: System, at: [f64; 3], pace: Pace) -> Self {
        let slope = system.slope(at, pace.parameter);
        Self {
            at,
            slope: slope.map(|slope| slope * pace.seconds),
        }
    }

    /// The next step on.
    #[inline]
    fn next(self, system: System, pace: Pace) -> Self {
        let at = system.step(self.at, pace.seconds, pace.parameter);
        Self::of(system, at, pace)
    }
}

/// An attractor as an LFO runs it: where it was seeded, in the LFO's cycles, how many steps it has
/// taken since, and the points either side of where it is. It's plain numbers, so the engine keeps
/// one in each slot and nothing is allocated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attractor {
    pub system: System,
    origin: f64,
    steps: i64,
    from: Point,
    to: Point,
}

impl Attractor {
    /// Start from its seed, `cycles` into the LFO's count: the same start point for every key,
    /// settled on the attractor for one cycle and up to three more by the key, at this deform.
    pub fn seeded(system: System, key: u64, deform: f32, cycles: f64) -> Self {
        let pace = Pace::of(system, deform);
        // The key mixed, so neighbouring keys settle for very different times.
        let keyed = (splitmix64(key) >> 11) as f64 / (1_u64 << 53) as f64;
        let settle = ((SETTLE_CYCLES + KEYED_CYCLES * keyed) * STEPS_PER_CYCLE) as usize;
        let mut point = Point::of(system, system.start(), pace);
        for _ in 0..settle {
            point = point.next(system, pace);
        }
        Self {
            system,
            origin: cycles,
            steps: 0,
            from: point,
            to: point.next(system, pace),
        }
    }

    /// Step on to `cycles` into the LFO's count at this deform, if it can: not if that's before
    /// where it is, or more than [`MAX_CATCH_UP`] steps on, when it should be seeded again there.
    #[must_use]
    pub fn reach(&mut self, cycles: f64, deform: f32) -> bool {
        let target = self.steps_to(cycles);
        if target < self.steps || target - self.steps > MAX_CATCH_UP || !self.is_finite() {
            return false;
        }
        let pace = Pace::of(self.system, deform);
        while self.steps < target {
            self.from = self.to;
            self.to = self.to.next(self.system, pace);
            self.steps += 1;
        }
        true
    }

    /// How many whole steps `cycles` is from its seed.
    fn steps_to(&self, cycles: f64) -> i64 {
        ((cycles - self.origin) * STEPS_PER_CYCLE).floor() as i64
    }

    fn is_finite(&self) -> bool {
        self.to.at.iter().all(|value| value.is_finite())
    }

    /// Where it is at `cycles`, which it has reached, each axis from -1 to 1 by the range it
    /// covers at this deform: on the cubic from the step before to the step after, with the
    /// slopes the equations give at each.
    pub fn point(&self, cycles: f64, deform: f32) -> [f32; 3] {
        let t = ((cycles - self.origin) * STEPS_PER_CYCLE - self.steps as f64).clamp(0.0, 1.0);
        let (t2, t3) = (t * t, t * t * t);
        let (h00, h10, h01, h11) = (
            2.0 * t3 - 3.0 * t2 + 1.0,
            t3 - 2.0 * t2 + t,
            -2.0 * t3 + 3.0 * t2,
            t3 - t2,
        );
        let axes = self.system.measured(f64::from(deform)).axes;
        std::array::from_fn(|axis| {
            let value = h00 * self.from.at[axis]
                + h10 * self.from.slope[axis]
                + h01 * self.to.at[axis]
                + h11 * self.to.slope[axis];
            let (low, high) = axes[axis];
            let (middle, half) = (low.midpoint(high), (high - low) / 2.0 * MARGIN);
            (((value - middle) / half) as f32).clamp(-1.0, 1.0)
        })
    }
}

/// What's heard of a point, each axis from -1 to 1, looking at it from `turns` round: `x` at 0, `y`
/// a third of the way round and `z` two thirds, and between two, a blend of them, so an LFO's
/// `Phase` picks the axis, 0° `x`, 120° `y` and 240° `z`, and moves smoothly between them.
#[inline]
pub fn view(point: [f32; 3], turns: f64) -> f32 {
    let along = turns.rem_euclid(1.0) * 3.0;
    let axis = (along.floor() as usize).min(2);
    let share = (along - axis as f64) as f32;
    point[axis] * (1.0 - share) + point[(axis + 1) % 3] * share
}

/// Where an attractor seeded at the LFO's start is `cycles` on, worked out from its seed every
/// time: what an engine running one from the top of the song gives at a steady deform. It takes a
/// step for every 128th of a cycle, so it's for drawings and tests, not the audio thread.
pub fn attractor_at(system: System, cycles: f64, key: u64, deform: f32) -> [f32; 3] {
    let mut attractor = Attractor::seeded(system, key, deform, 0.0);
    let cycles = cycles.max(0.0);
    let mut reached = 0.0;
    while reached < cycles {
        reached = (reached + MAX_CATCH_UP as f64 / STEPS_PER_CYCLE).min(cycles);
        assert!(attractor.reach(reached, deform), "it can always go on");
    }
    attractor.point(cycles, deform)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEMS: [System; 2] = [System::Lorenz, System::Rossler];
    const DEFORMS: [f32; 13] = [
        -1.0, -0.9, -0.7, -0.6, -0.4, -0.2, 0.0, 0.1, 0.3, 0.45, 0.7, 0.85, 1.0,
    ];

    /// Each point a sixteenth of a cycle apart, for this many cycles.
    fn run(system: System, key: u64, deform: f32, cycles: usize) -> Vec<[f32; 3]> {
        let mut attractor = Attractor::seeded(system, key, deform, 0.0);
        (0..cycles * 16)
            .map(|at| {
                let cycles = at as f64 / 16.0;
                assert!(attractor.reach(cycles, deform));
                attractor.point(cycles, deform)
            })
            .collect()
    }

    fn axis(points: &[[f32; 3]], axis: usize) -> Vec<f32> {
        points.iter().map(|point| point[axis]).collect()
    }

    #[test]
    fn every_axis_stays_in_range_and_reaches_most_of_it_at_any_deform() {
        for system in SYSTEMS {
            for deform in DEFORMS {
                let points = run(system, 3, deform, 300);
                for at in 0..3 {
                    let values = axis(&points, at);
                    assert!(
                        values.iter().all(|value| (-1.0..=1.0).contains(value)),
                        "{system:?} {deform} {at}"
                    );
                    // Its furthest reaches are rare enough that keeping within the range hardly
                    // ever holds it at an end, and most of the range is used.
                    #[allow(clippy::float_cmp, reason = "counts the values held at an end")]
                    let held = values.iter().filter(|value| value.abs() == 1.0).count();
                    assert!(held * 100 < values.len(), "{system:?} {deform} {at} {held}");
                    let low = values.iter().copied().fold(1.0, f32::min);
                    let high = values.iter().copied().fold(-1.0, f32::max);
                    assert!(
                        high > 0.6 && low < -0.6,
                        "{system:?} {deform} {at} {low} {high}"
                    );
                }
                // The axes that swing either way are about centred.
                for at in [0, 1] {
                    let values = axis(&points, at);
                    let mean = values.iter().sum::<f32>() / values.len() as f32;
                    assert!(mean.abs() < 0.2, "{system:?} {deform} {at} {mean}");
                }
            }
        }
    }

    #[test]
    fn a_cycle_is_about_a_turn_at_any_deform() {
        // A turn is a highest point of Lorenz's `z`, or of Rössler's `x`.
        for (system, at) in [(System::Lorenz, 2), (System::Rossler, 0)] {
            for deform in DEFORMS {
                let mut attractor = Attractor::seeded(system, 3, deform, 0.0);
                let values: Vec<f32> = (0..200 * 128)
                    .map(|step| {
                        let cycles = f64::from(step) / 128.0;
                        assert!(attractor.reach(cycles, deform));
                        attractor.point(cycles, deform)[at]
                    })
                    .collect();
                let turns = values
                    .windows(3)
                    .filter(|three| three[1] > three[0] && three[1] >= three[2])
                    .count();
                assert!((185..=215).contains(&turns), "{system:?} {deform} {turns}");
            }
        }
    }

    #[test]
    fn a_seed_and_a_start_repeat_however_its_reached_and_keys_differ() {
        for system in SYSTEMS {
            let once = run(system, 5, 0.2, 40);
            assert_eq!(once, run(system, 5, 0.2, 40));
            assert_ne!(once, run(system, 6, 0.2, 40));
            // Reached in steps of any size, it's where working it out from the seed puts it.
            let mut attractor = Attractor::seeded(system, 5, 0.2, 0.0);
            let mut cycles = 0.0;
            for step in 0..400 {
                cycles += f64::from(step % 13) * 0.0137;
                assert!(attractor.reach(cycles, 0.2));
                if step % 37 == 0 {
                    assert_eq!(
                        attractor.point(cycles, 0.2),
                        attractor_at(system, cycles, 5, 0.2)
                    );
                }
            }
            // Seeded later, it goes the same way from there.
            let mut later = Attractor::seeded(system, 5, 0.2, 10.0);
            assert!(later.reach(10.0 + 3.25, 0.2));
            assert_eq!(
                later.point(10.0 + 3.25, 0.2),
                attractor_at(system, 3.25, 5, 0.2)
            );
        }
    }

    #[test]
    fn it_never_quite_repeats_but_rossler_at_its_lowest_is_a_loop() {
        // Once a cycle for 300, then how far each is from the one some cycles on: far from none.
        let apart = |system: System, deform: f32, lag: usize| {
            let points = run(system, 3, deform, 300);
            let x: Vec<f32> = axis(&points, 0).into_iter().step_by(16).collect();
            let pairs = x.len() - lag;
            (0..pairs)
                .map(|at| (x[at] - x[at + lag]).abs())
                .sum::<f32>()
                / pairs as f32
        };
        for system in SYSTEMS {
            for lag in 1..40 {
                let apart = apart(system, 0.0, lag);
                assert!(apart > 0.05, "{system:?} {lag} {apart}");
            }
        }
        assert!(apart(System::Rossler, -1.0, 1) < 0.05);
    }

    #[test]
    fn it_wont_go_back_or_too_far_at_once() {
        let mut attractor = Attractor::seeded(System::Lorenz, 1, 0.0, 4.0);
        assert!(attractor.reach(5.0, 0.0));
        assert!(!attractor.reach(4.5, 0.0));
        let far = 5.0 + (MAX_CATCH_UP + 1) as f64 / STEPS_PER_CYCLE;
        let mut copy = attractor;
        assert!(!copy.reach(far, 0.0));
        assert!(attractor.reach(far - 2.0 / STEPS_PER_CYCLE, 0.0));
    }

    #[test]
    fn its_view_picks_an_axis_or_blends_the_next() {
        let point = [0.5, -0.25, 1.0];
        assert_eq!(view(point, 0.0), 0.5);
        assert_eq!(view(point, 1.0 / 3.0), -0.25);
        assert_eq!(view(point, 2.0 / 3.0), 1.0);
        assert_eq!(view(point, 1.0), 0.5);
        assert!((view(point, 1.0 / 6.0) - 0.125).abs() < 1e-6);
        assert!((view(point, 5.0 / 6.0) - 0.75).abs() < 1e-6);
        assert_eq!(view(point, -1.0 / 3.0), 1.0);
    }
}
