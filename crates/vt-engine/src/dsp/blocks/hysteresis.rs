//! Tape's magnetic hysteresis (#259): the Jiles–Atherton model ("Theory of ferromagnetic
//! hysteresis", 1986) as Jatin Chowdhury's ChowTape has it ("Real-time Physical Modelling for
//! Analog Tape Machines", DAFx 2019): the tape's magnetisation `M` follows the head's field `H`
//! along a loop, so it saturates, and lags the field by how it moved before.
//!
//! The model is an ordinary differential equation in `M`, solved a sample at a time by the
//! trapezoidal rule. ChowTape solves it with Runge–Kutta or with four or eight Newton–Raphson
//! iterations, each working the model out afresh; here it takes one Newton step, from the last
//! sample's magnetisation, with the last sample's slope carried over rather than worked out again
//! (its value at the last step's start, moved by the step along the derivative already known), so
//! each sample works the model out once. Measured against Runge–Kutta at sixteen times the rate,
//! that's as close as ChowTape's own second-order Runge–Kutta, at half its cost (decision 443).
//!
//! In double precision, as the model needs: the Langevin function's `coth(Q) - 1/Q` loses its
//! digits near nothing in single.
//!
//! Adapted from `Plugin/Source/Processors/Hysteresis/HysteresisOps.h` and
//! `HysteresisProcessing.{h,cpp}` in ChowTape (AnalogTapeModel), GPL-3.0:
//!
//! Copyright (c) 2019 Jatin Chowdhury. ChowTape's state-transition network solver and its
//! weights are left out.

use super::clip::within;
use super::flush::TINY;

/// The Jiles–Atherton constants ChowTape keeps fixed: the coupling between domains, and the
/// pinning that sets the loop's width.
const ALPHA: f64 = 1.6e-3;
const PINNING: f64 = 0.478_75;
/// Below this a field is near enough nothing for the Langevin function's series.
const NEAR_NOTHING: f64 = 1e-3;
/// How far ChowTape's derivative of the field leans back on the last one, the alpha transform's
/// alpha: 1 is the bilinear transform, 0 the backward difference.
const DERIVATIVE_ALPHA: f64 = 0.75;
/// The trapezoidal rule's step, as a share of a sample: a little under a half, as ChowTape's
/// `T / 1.9`, which damps what the solver would otherwise ring at near the top.
const TRAPEZOID: f64 = 1.0 / 1.9;
/// How far past full saturation the magnetisation may go before a step is taken as having
/// diverged and held where it was.
const MOST_MAGNETISATION: f64 = 1.5;
/// The field the head can give, past which what comes in is held, as ChowTape's clip before its
/// Newton–Raphson solvers.
pub(crate) const MOST_FIELD: f32 = 12.5;

/// The tape's magnetic constants, from its settings, with what the model works out from them
/// every sample worked out once.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Magnetics {
    /// The saturation magnetisation, and one over the anhysteretic curve's shape.
    saturation: f64,
    per_shape: f64,
    /// The irreversible share of the magnetisation, from about 0, a thin loop, to about 1, a wide
    /// one, and it times the pinning.
    irreversible: f64,
    pinning: f64,
    /// The reversible share times `M_s / a`, and that times `alpha`.
    reversible: f64,
    coupled: f64,
    /// `M_s alpha / a`, how far the curve's own input moves with `M`.
    feedback: f64,
    /// What `M` is multiplied by to come out at about the level of the field that went in.
    pub(crate) makeup: f64,
}

impl Magnetics {
    /// For `drive` as a gain, and `saturation` and `bias` from 0 to 1, as ChowTape's knobs. More
    /// drive narrows the anhysteretic curve, so the same field takes it further toward saturation,
    /// as a field that much stronger would; a drive of one is ChowTape's half way. More saturation
    /// lowers where it saturates, and more bias makes more of it reversible, so its loop thins.
    ///
    /// ChowTape's makeup divides by the saturation magnetisation, which leaves a quiet sound
    /// louder the more `Sat` is turned up and the more it's driven. Here the makeup divides by the
    /// drive instead, keeping ChowTape's share for the bias, so a quiet sound comes out within
    /// about a dB at any drive and saturation, and turning either up saturates sooner rather than
    /// turning it up (decision 443). `Reel` has since taken [`Self::biased`]; this stays for the
    /// loop's own tests.
    #[cfg(test)]
    pub(crate) fn new(drive: f64, saturation: f64, bias: f64) -> Self {
        let saturation = 0.5 + 1.5 * (1.0 - saturation);
        let reversible = bias.max(0.0).sqrt() - 0.01;
        Self::of(
            drive,
            saturation,
            reversible,
            (1.0 + 0.6 * (1.0 - bias)) / drive,
        )
    }

    /// Tape biased right, as a studio machine is aligned: the bias's high-frequency field leaves
    /// the tape on its anhysteretic curve, so the loop closes and all of the magnetisation is
    /// reversible (decision 449). `drive` is a gain and `saturation` the saturation magnetisation
    /// itself; the makeup brings a quiet sound out at the level it went in, taking off what the
    /// trapezoidal rule's short step adds.
    pub(crate) fn biased(drive: f64, saturation: f64) -> Self {
        Self::of(drive, saturation, 1.0, 1.0 / (2.0 * TRAPEZOID * drive))
    }

    fn of(drive: f64, saturation: f64, reversible: f64, makeup: f64) -> Self {
        let shape = saturation / (3.0 * drive);
        let irreversible = (1.0 - reversible).max(0.0);
        let per_shape = 1.0 / shape;
        Self {
            saturation,
            per_shape,
            irreversible,
            // With nothing irreversible the pinning only divides what it then multiplies by
            // nothing, so it's kept at one, away from where it would divide by nothing.
            pinning: if irreversible > 0.0 {
                irreversible * PINNING
            } else {
                1.0
            },
            reversible: reversible * saturation * per_shape,
            coupled: reversible * saturation * per_shape * ALPHA,
            feedback: saturation * per_shape * ALPHA,
            makeup,
        }
    }
}

/// The model's slope `dM/dt` at a magnetisation, a field and the field's rate of change, and its
/// derivative by `M`, for the Newton step: ChowTape's `hysteresisFunc` and `hysteresisFuncPrime`
/// together, with three divisions where they have eight.
#[inline]
fn slope(m: f64, h: f64, h_rate: f64, tape: &Magnetics) -> (f64, f64) {
    let q = (h + ALPHA * m) * tape.per_shape;
    // The Langevin function `coth(Q) - 1/Q`, and its first two derivatives.
    let (langevin, langevin_slope, langevin_curve) = if q.abs() < NEAR_NOTHING {
        (q / 3.0, 1.0 / 3.0, -2.0 / 15.0 * q)
    } else {
        // `coth` by one exponential, which is cheaper than `tanh`, and it and `1/Q` by one
        // division.
        let falling = (-2.0 * q.abs()).exp();
        let shared = 1.0 / ((1.0 - falling) * q);
        let coth = ((1.0 + falling) * q * shared).copysign(q);
        let over = (1.0 - falling) * shared;
        let squared = over * over;
        (
            coth - over,
            squared - coth * coth + 1.0,
            2.0 * coth * (coth * coth - 1.0) - 2.0 * squared * over,
        )
    };
    let off = tape.saturation * langevin - m;
    let rising = if h_rate >= 0.0 { 1.0 } else { -1.0 };
    // The irreversible part moves only while the field pulls the magnetisation toward the curve.
    let moving = if off != 0.0 && (off > 0.0) == (rising > 0.0) {
        tape.irreversible
    } else {
        0.0
    };
    let pinned = rising * tape.pinning - ALPHA * off;
    let denominator = 1.0 - langevin_slope * tape.coupled;
    let both = 1.0 / (pinned * denominator);
    let (over_pinned, over_denominator) = (denominator * both, pinned * both);
    let value =
        h_rate * (moving * off * over_pinned + langevin_slope * tape.reversible) * over_denominator;
    // Its derivative by `M`, through `off`, `L'` and the denominator.
    let off_by_m = langevin_slope * tape.feedback - 1.0;
    let irreversible = moving * off_by_m * over_pinned * (1.0 + off * ALPHA * over_pinned);
    let reversible = langevin_curve * tape.coupled * tape.per_shape;
    let denominator_by_m = -reversible * ALPHA;
    let by_m = (h_rate * (irreversible + reversible) - value * denominator_by_m) * over_denominator;
    (value, by_m)
}

/// One side's tape: its magnetisation, the field and its rate of change a sample ago, and the
/// slope carried over.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Hysteresis {
    m: f64,
    h: f64,
    h_rate: f64,
    slope: f64,
}

impl Hysteresis {
    /// The magnetisation for the next field, a sample of `period` seconds on from the last.
    #[inline]
    pub(crate) fn process(&mut self, h: f32, period: f64, tape: &Magnetics) -> f64 {
        let h = f64::from(within(h, -MOST_FIELD, MOST_FIELD));
        let h_rate =
            (1.0 + DERIVATIVE_ALPHA) / period * (h - self.h) - DERIVATIVE_ALPHA * self.h_rate;
        let step = period * TRAPEZOID;
        // One Newton step on `M - M[n-1] - step (f(M) + f[n-1]) = 0`, from `M[n-1]`.
        let (value, by_m) = slope(self.m, h, h_rate, tape);
        let moved = step * (value + self.slope) / (1.0 - step * by_m);
        let m = self.m + moved;
        let bound = MOST_MAGNETISATION * tape.saturation;
        if m.is_finite() && m.abs() <= bound && moved.is_finite() {
            self.m = m;
            self.slope = value + by_m * moved;
            self.h_rate = h_rate;
        } else {
            // Diverged: the magnetisation holds and the field stops moving, as ChowTape does.
            self.slope = 0.0;
            self.h_rate = 0.0;
        }
        // In silence the field's rate falls away by a quarter a sample, and is let go before it
        // reaches the denormals. The magnetisation stays, as tape's does.
        if h == 0.0 && self.h_rate.abs() < f64::from(TINY) {
            self.h_rate = 0.0;
            self.slope = 0.0;
        }
        self.h = h;
        self.m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f64 = 48_000.0;

    /// The model solved with fourth-order Runge–Kutta, as ChowTape's `RK4Solver`, for a
    /// reference at a high rate.
    #[derive(Default)]
    struct Reference {
        m: f64,
        h: f64,
        h_rate: f64,
    }

    impl Reference {
        fn process(&mut self, h: f64, period: f64, tape: &Magnetics) -> f64 {
            let h_rate =
                (1.0 + DERIVATIVE_ALPHA) / period * (h - self.h) - DERIVATIVE_ALPHA * self.h_rate;
            let (h_half, rate_half) = ((h + self.h) * 0.5, (h_rate + self.h_rate) * 0.5);
            let k1 = slope(self.m, self.h, self.h_rate, tape).0 * period;
            let k2 = slope(self.m + k1 * 0.5, h_half, rate_half, tape).0 * period;
            let k3 = slope(self.m + k2 * 0.5, h_half, rate_half, tape).0 * period;
            let k4 = slope(self.m + k3, h, h_rate, tape).0 * period;
            self.m += k1 / 6.0 + k2 / 3.0 + k3 / 3.0 + k4 / 6.0;
            (self.h, self.h_rate) = (h, h_rate);
            self.m
        }
    }

    /// The field and magnetisation over a 10 Hz sine's last cycle, of a second, at `level`.
    fn loop_of(tape: &Magnetics, level: f64) -> Vec<(f64, f64)> {
        let mut side = Hysteresis::default();
        let frames = RATE as usize;
        let mut traced = Vec::new();
        for frame in 0..frames {
            let h = level * (std::f64::consts::TAU * 10.0 * frame as f64 / RATE).sin();
            let m = side.process(h as f32, 1.0 / RATE, tape);
            if frame >= frames - (RATE / 10.0) as usize {
                traced.push((f64::from(h as f32), m));
            }
        }
        traced
    }

    /// A loop's area, by the shoelace formula, and the magnetisation at its widest.
    fn area(traced: &[(f64, f64)]) -> f64 {
        let mut sum = 0.0;
        for pair in traced.windows(2) {
            sum += pair[0].0 * pair[1].1 - pair[1].0 * pair[0].1;
        }
        (sum * 0.5).abs()
    }

    #[test]
    fn a_slow_sine_traces_a_loop_that_widens_and_saturates_with_drive() {
        let gentle = loop_of(&Magnetics::new(0.5, 0.5, 0.5), 1.0);
        let driven = loop_of(&Magnetics::new(2.0, 0.5, 0.5), 1.0);
        // Rising and falling through nothing, the magnetisation differs: a loop, not a curve.
        let at_nothing = |traced: &[(f64, f64)]| {
            let mut found = Vec::new();
            for pair in traced.windows(2) {
                if (pair[0].0 < 0.0) != (pair[1].0 < 0.0) {
                    found.push(pair[1].1);
                }
            }
            found
        };
        let crossings = at_nothing(&driven);
        assert_eq!(crossings.len(), 2, "{crossings:?}");
        assert!(
            (crossings[0] - crossings[1]).abs() > 0.05,
            "remanence {crossings:?}"
        );
        assert!(
            area(&driven) > area(&gentle) * 2.0,
            "{} {}",
            area(&gentle),
            area(&driven)
        );
        // Driven, it's flatter at the top: the magnetisation at full field is a smaller share of
        // its slope through nothing.
        let flattening = |traced: &[(f64, f64)]| {
            let top = traced.iter().map(|(_, m)| m.abs()).fold(0.0, f64::max);
            let near = traced
                .windows(2)
                .filter(|pair| pair[0].0.abs() < 0.1 && pair[1].0 > pair[0].0)
                .map(|pair| (pair[1].1 - pair[0].1) / (pair[1].0 - pair[0].0))
                .fold(0.0, f64::max);
            top / near
        };
        assert!(flattening(&driven) < flattening(&gentle) * 0.6);
        // More bias thins the loop.
        let biased = loop_of(&Magnetics::new(2.0, 0.5, 0.95), 1.0);
        assert!(
            area(&biased) < area(&driven) * 0.8,
            "{} {}",
            area(&biased),
            area(&driven)
        );
    }

    #[test]
    fn one_newton_step_with_its_slope_carried_stays_near_runge_kutta_at_sixteen_times() {
        // A sum of sines up to 5 kHz at 2x, compared with RK4 at 32x, read every sixteenth.
        let tape = Magnetics::new(1.6, 0.5, 0.5);
        let (rate, many) = (2.0 * RATE, 16usize);
        let input = |t: f64| {
            let tau = std::f64::consts::TAU;
            2.0 * (tau * 110.0 * t).sin()
                + 0.8 * (tau * 1_300.0 * t).sin()
                + 0.4 * (tau * 5_000.0 * t).sin()
        };
        let mut side = Hysteresis::default();
        let mut reference = Reference::default();
        let (mut off, mut level) = (0.0, 0.0);
        for frame in 0..(rate as usize / 4) {
            let m = side.process(input(frame as f64 / rate) as f32, 1.0 / rate, &tape);
            let mut r = 0.0;
            for step in 0..many {
                let t = (frame as f64 - 1.0 + (step + 1) as f64 / many as f64) / rate;
                r = reference.process(
                    f64::from(input(t) as f32),
                    1.0 / (rate * many as f64),
                    &tape,
                );
            }
            if frame > rate as usize / 8 {
                off += (m - r) * (m - r);
                level += r * r;
            }
        }
        let error_db = 10.0 * (off / level).log10();
        assert!(error_db < -30.0, "{error_db}");
    }

    #[test]
    fn steps_of_dc_and_silence_never_blow_it_up() {
        for drive in [0.1, 1.0, 8.0] {
            for bias in [0.0, 1.0] {
                let tape = Magnetics::new(drive, 1.0, bias);
                let mut side = Hysteresis::default();
                for frame in 0..48_000 {
                    let h = match frame / 3000 % 4 {
                        0 => 100.0,
                        1 => 0.0,
                        2 => -MOST_FIELD,
                        _ => {
                            if frame % 2 == 0 {
                                MOST_FIELD
                            } else {
                                -MOST_FIELD
                            }
                        }
                    };
                    let m = side.process(h, 1.0 / (2.0 * RATE), &tape);
                    assert!(m.is_finite() && m.abs() <= MOST_MAGNETISATION * tape.saturation);
                }
            }
        }
    }
}
