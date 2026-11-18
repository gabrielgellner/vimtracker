//! A frame for analog colour (#262): a static curve whose asymmetry and hardness are set apart,
//! between a pre-emphasis and a de-emphasis that are exact inverses, then a transformer's low end
//! and a DC blocker. The curve is anti-aliased with its second antiderivative (second-order ADAA,
//! after Bilbao, Esqueda, Parker and Välimäki, "Antiderivative Antialiasing for Memoryless
//! Nonlinearities", IEEE SPL 2017) at eight times the rate.
//!
//! Each curve is a circuit's, worked out once into a table:
//!
//! - **Warm:** `tanh`, a differential pair's.
//! - **Tube:** a 12AX7 common-cathode stage, by Koren's triode equations ("Improved VT models for
//!   SPICE simulations", 1996), on a 100 kΩ load line from 250 V, biased at -1.5 V, with the grid
//!   held near 0 V as it starts to conduct.
//! - **Diode:** a pair of silicon diodes back to back after a resistor, as a pedal's clipper,
//!   with the 1N914's Shockley constants.
//! - **Transistor:** a silicon transistor's common-emitter stage with emitter degeneration,
//!   cutting off softly on one side and saturating hard on the other (Ebers-Moll, forward active).
//!
//! Each is scaled so its slope at nothing is one, which makes the frame unity gain for a quiet
//! sound at no drive: the emphases cancel, the curve passes it as it is and the transformer only
//! bends what's loud and low. A curve is kept as what it takes from a straight line through
//! nothing, `curve(x) - x`, which is about nothing for a quiet sound, so its integrals there are
//! exact to the last bit rather than the small difference of two large ones.

use std::sync::LazyLock;

use super::clip::within;
use super::flush::{Tiny, flush};
use super::one_pole;
use super::oversampler::OctupleOversampler;
use super::sides::{SameBits, same_bits_by_fields};

/// The curves the frame has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Curve {
    Warm,
    Tube,
    Diode,
    Transistor,
}

/// How far either side of nothing a curve is tabled. Past it, it goes on in a straight line at
/// its slope there, which is about flat for every curve but the diodes', which still rise slowly.
const REACH: f64 = 8.0;
/// Knots a unit.
const DENSITY: f64 = 64.0;
const KNOTS: usize = (2.0 * REACH * DENSITY) as usize + 1;
const MIDDLE: usize = KNOTS / 2;
const STEP: f64 = 1.0 / DENSITY;

/// How close samples at the higher rate can be before a difference of antiderivatives over the
/// distance between them is taken as no better than the next simpler form.
const ADAA_CLOSEST: f64 = 1e-4;

/// The most the asymmetry moves the curve's working point, as a share of the way to its nearer
/// end.
const MOST_BIAS: f64 = 0.4;

/// How far what comes in can go, 24 dB over full scale.
const HEADROOM: f32 = 16.0;

/// Where the emphasis shelves turn, in Hz, and how far they lift or cut the highs at either end of
/// their range, in dB.
const EMPHASIS_HZ: f32 = 1_500.0;
const MOST_EMPHASIS_DB: f32 = vt_model::insert::MOST_COLOUR_EMPHASIS_DB;

/// Where the transformer's flux stops following the sound, in Hz: below it the flux is the
/// sound's, above it falls an octave at a time, so a low note saturates the core and a high one
/// doesn't.
const IRON_FLUX_HZ: f32 = 40.0;
/// How hard the core saturates at full `Iron`: its flux's knee, `tanh(k φ) / k`.
const MOST_IRON_KNEE: f32 = 2.0;
/// The transformer's bump: where, how wide and how high at full `Iron`, in dB.
const BUMP_HZ: f32 = 60.0;
const BUMP_Q: f32 = 0.9;
const MOST_BUMP_DB: f32 = 1.5;
/// Where the DC blocker after the asymmetric curve turns, in Hz.
const DC_HZ: f32 = 5.0;

/// A curve worked out once, as what it takes from `x`: at each knot that remainder, its slope,
/// and its first and second integrals from nothing. Between knots the remainder is the cubic
/// Hermite through theirs, and the integrals that cubic's, exactly, so all three always agree, as
/// ADAA needs.
pub(crate) struct Table {
    values: [f64; KNOTS],
    slopes: [f64; KNOTS],
    firsts: [f64; KNOTS],
    seconds: [f64; KNOTS],
    /// The second integral across each segment as a polynomial in how far along it is, from its
    /// constant up, for the one lookup every sample at the higher rate makes.
    second_terms: [[f64; 6]; KNOTS],
    /// The curve's own value at each end, where the hard clip it blends toward holds.
    low: f64,
    high: f64,
}

/// The Hermite basis at `t`, its integrals from 0, and theirs, each as the weights of the two
/// knots' values and slopes (the slopes already times `STEP`).
#[inline]
fn basis(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    [
        2.0 * t3 - 3.0 * t2 + 1.0,
        t3 - 2.0 * t2 + t,
        3.0 * t2 - 2.0 * t3,
        t3 - t2,
    ]
}

#[inline]
fn basis_slope(t: f64) -> [f64; 4] {
    let t2 = t * t;
    [
        6.0 * t2 - 6.0 * t,
        3.0 * t2 - 4.0 * t + 1.0,
        6.0 * t - 6.0 * t2,
        3.0 * t2 - 2.0 * t,
    ]
}

#[inline]
fn basis_first(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    [
        0.5 * t4 - t3 + t,
        0.25 * t4 - 2.0 / 3.0 * t3 + 0.5 * t2,
        t3 - 0.5 * t4,
        0.25 * t4 - t3 / 3.0,
    ]
}

#[inline]
fn basis_second(t: f64) -> [f64; 4] {
    let t2 = t * t;
    let t3 = t2 * t;
    let t4 = t3 * t;
    let t5 = t4 * t;
    [
        0.1 * t5 - 0.25 * t4 + 0.5 * t2,
        0.05 * t5 - t4 / 6.0 + t3 / 6.0,
        0.25 * t4 - 0.1 * t5,
        0.05 * t5 - t4 / 12.0,
    ]
}

impl Table {
    fn build(curve: impl Fn(f64) -> f64) -> Self {
        let mut values = [0.0; KNOTS];
        let mut slopes = [0.0; KNOTS];
        // Scaled to a slope of one at nothing, and to nothing there.
        let centre = curve(0.0);
        let unit = slope_of(&curve, 0.0);
        let scaled = |x: f64| (curve(x) - centre) / unit;
        for knot in 0..KNOTS {
            let x = knot as f64 * STEP - REACH;
            values[knot] = if knot == MIDDLE { 0.0 } else { scaled(x) - x };
            slopes[knot] = slope_of(scaled, x) - 1.0;
        }
        let mut table = Self {
            low: values[0] - REACH,
            high: values[KNOTS - 1] + REACH,
            values,
            slopes,
            firsts: [0.0; KNOTS],
            seconds: [0.0; KNOTS],
            second_terms: [[0.0; 6]; KNOTS],
        };
        // Each integral from nothing, a segment at a time out from the middle either way.
        for knot in MIDDLE + 1..KNOTS {
            table.firsts[knot] = table.firsts[knot - 1] + table.first_within(knot - 1, 1.0);
            table.seconds[knot] = table.seconds[knot - 1] + table.second_within(knot - 1, 1.0);
        }
        for knot in (0..MIDDLE).rev() {
            table.firsts[knot] = table.firsts[knot + 1] - table.first_within(knot, 1.0);
            table.seconds[knot] = table.seconds[knot + 1] - table.second_within(knot, 1.0);
        }
        // `second_within`'s basis, gathered by power.
        for knot in 0..KNOTS - 1 {
            let [v0, m0, v1, m1] = table.knots(knot);
            let squared = STEP * STEP;
            table.second_terms[knot] = [
                table.seconds[knot],
                STEP * table.firsts[knot],
                squared * 0.5 * v0,
                squared * m0 / 6.0,
                squared * (0.25 * (v1 - v0) - m0 / 6.0 - m1 / 12.0),
                squared * (0.1 * (v0 - v1) + 0.05 * (m0 + m1)),
            ];
        }
        table
    }

    /// The four weights of a segment's knots, the slopes taken to its length.
    #[inline]
    fn knots(&self, knot: usize) -> [f64; 4] {
        [
            self.values[knot],
            self.slopes[knot] * STEP,
            self.values[knot + 1],
            self.slopes[knot + 1] * STEP,
        ]
    }

    /// The first integral across a segment from its knot, to `t` of the way.
    #[inline]
    fn first_within(&self, knot: usize, t: f64) -> f64 {
        STEP * dot(self.knots(knot), basis_first(t))
    }

    /// The second integral across a segment from its knot, to `t` of the way, with the first
    /// integral's level at the knot carried along it.
    #[inline]
    fn second_within(&self, knot: usize, t: f64) -> f64 {
        STEP * (self.firsts[knot] * t + STEP * dot(self.knots(knot), basis_second(t)))
    }

    /// The knot at or below `x`, and how far on from it toward the next, from 0 to 1. `None`
    /// past either end.
    #[inline]
    fn place(x: f64) -> Option<(usize, f64)> {
        let at = (x + REACH) * DENSITY;
        if !(at >= 0.0 && at < (KNOTS - 1) as f64) {
            return None;
        }
        let knot = at as usize;
        Some((knot, at - knot as f64))
    }

    /// The end `x` is past, and how far past: the knot, and the distance, negative below.
    #[inline]
    fn past(x: f64) -> (usize, f64) {
        if x < 0.0 {
            (0, x + REACH)
        } else {
            (KNOTS - 1, x - REACH)
        }
    }

    /// What the curve takes from `x`, at `x`.
    #[inline]
    pub(crate) fn value(&self, x: f64) -> f64 {
        match Self::place(x) {
            Some((knot, t)) => dot(self.knots(knot), basis(t)),
            None => {
                let (end, p) = Self::past(x);
                self.values[end] + self.slopes[end] * p
            }
        }
    }

    /// Its slope.
    #[inline]
    pub(crate) fn slope(&self, x: f64) -> f64 {
        match Self::place(x) {
            Some((knot, t)) => dot(self.knots(knot), basis_slope(t)) / STEP,
            None => self.slopes[Self::past(x).0],
        }
    }

    /// Its integral from nothing.
    #[inline]
    pub(crate) fn first(&self, x: f64) -> f64 {
        match Self::place(x) {
            Some((knot, t)) => self.firsts[knot] + self.first_within(knot, t),
            None => {
                let (end, p) = Self::past(x);
                self.firsts[end] + self.values[end] * p + 0.5 * self.slopes[end] * p * p
            }
        }
    }

    /// That integral's integral from nothing.
    #[inline]
    pub(crate) fn second(&self, x: f64) -> f64 {
        match Self::place(x) {
            Some((knot, t)) => {
                let [c0, c1, c2, c3, c4, c5] = self.second_terms[knot];
                c0 + t * (c1 + t * (c2 + t * (c3 + t * (c4 + t * c5))))
            }
            None => {
                let (end, p) = Self::past(x);
                self.seconds[end]
                    + self.firsts[end] * p
                    + 0.5 * self.values[end] * p * p
                    + self.slopes[end] * p * p * p / 6.0
            }
        }
    }
}

#[inline]
fn dot(a: [f64; 4], b: [f64; 4]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]
}

/// A slope worked out from either side, for a curve that's only known by its values.
fn slope_of(curve: impl Fn(f64) -> f64, x: f64) -> f64 {
    const APART: f64 = 1e-5;
    (curve(x + APART) - curve(x - APART)) / (2.0 * APART)
}

/// Where a rising function crosses `target` between `low` and `high`, by halving.
fn crossing(f: impl Fn(f64) -> f64, target: f64, mut low: f64, mut high: f64) -> f64 {
    for _ in 0..200 {
        let middle = 0.5 * (low + high);
        if middle <= low || middle >= high {
            break;
        }
        if f(middle) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    0.5 * (low + high)
}

/// `ln(1 + e^x)`, without overflowing.
fn softplus(x: f64) -> f64 {
    if x > 30.0 { x } else { x.exp().ln_1p() }
}

/// A 12AX7's plate current, in amps, by Koren's equations, for its grid and plate voltages.
fn koren_plate_current(grid: f64, plate: f64) -> f64 {
    const MU: f64 = 100.0;
    const EX: f64 = 1.4;
    const KG1: f64 = 1060.0;
    const KP: f64 = 600.0;
    const KVB: f64 = 300.0;
    let e1 = plate / KP * softplus(KP * (1.0 / MU + grid / (KVB + plate * plate).sqrt()));
    2.0 * e1.powf(EX) / KG1
}

/// The tube stage's plate voltage for an input `x`, a unit of which is 1.5 V at the grid, so one
/// takes it from its bias to where it starts to conduct. Turned over, as the stage inverts, so
/// the curve rises.
fn tube(x: f64) -> f64 {
    const SUPPLY: f64 = 250.0;
    const LOAD: f64 = 100e3;
    const BIAS: f64 = -1.5;
    /// How sharply the grid is held as it conducts, per volt.
    const CONDUCTION: f64 = 10.0;
    let driven = BIAS + 1.5 * x;
    let grid = -softplus(-CONDUCTION * driven) / CONDUCTION;
    // The plate sits where its current through the load leaves it.
    let plate = crossing(
        |plate| plate + koren_plate_current(grid, plate) * LOAD,
        SUPPLY,
        0.0,
        SUPPLY,
    );
    -plate
}

/// The diode clipper's output for an input `x`, a unit of which is half a volt.
fn diode(x: f64) -> f64 {
    const SATURATION: f64 = 2.52e-9;
    const THERMAL: f64 = 1.752 * 0.025_85;
    const SERIES: f64 = 4.7e3;
    let input = 0.5 * x;
    // What's through the resistor goes through the diodes.
    crossing(
        |out| out + SERIES * 2.0 * SATURATION * (out / THERMAL).sinh(),
        input,
        -input.abs() - 1.0,
        input.abs() + 1.0,
    )
}

/// The transistor stage's collector current for an input `x`, a unit of which is about how far
/// the base moves from its bias to saturation, about 0.46 V.
fn transistor(x: f64) -> f64 {
    const SATURATION: f64 = 1e-14;
    const THERMAL: f64 = 0.025_85;
    const SUPPLY: f64 = 9.0;
    const COLLECTOR: f64 = 4.7e3;
    const EMITTER: f64 = 470.0;
    const AT_REST: f64 = 0.8e-3;
    /// How far short of the supply the collector stops, saturated, and how sharply, in amps.
    const LEAST_ACROSS: f64 = 0.1;
    const KNEE: f64 = 3e-5;
    let base_at_rest = THERMAL * (AT_REST / SATURATION).ln() + AT_REST * EMITTER;
    let base = base_at_rest + 0.46 * x;
    // In its active region, the base sits a diode over the emitter, whose resistor carries the
    // current: worked out on the current's log, as it spans many decades.
    let log_current = crossing(
        |log| THERMAL * (log - SATURATION.ln()) + log.exp() * EMITTER,
        base,
        -80.0,
        0.0,
    );
    let active = log_current.exp();
    // Saturated, it can't pass more than the supply over both resistors.
    let most = (SUPPLY - LEAST_ACROSS) / (COLLECTOR + EMITTER);
    active - KNEE * softplus((active - most) / KNEE)
}

static WARM: LazyLock<Table> = LazyLock::new(|| Table::build(f64::tanh));
static TUBE: LazyLock<Table> = LazyLock::new(|| Table::build(tube));
static DIODE: LazyLock<Table> = LazyLock::new(|| Table::build(diode));
static TRANSISTOR: LazyLock<Table> = LazyLock::new(|| Table::build(transistor));

impl Curve {
    /// Its table, worked out the first time it's asked for.
    #[inline]
    pub(crate) fn table(self) -> &'static Table {
        match self {
            Self::Warm => &WARM,
            Self::Tube => &TUBE,
            Self::Diode => &DIODE,
            Self::Transistor => &TRANSISTOR,
        }
    }

    /// Work out every curve's table, off the audio thread, so none is first asked for on it.
    pub(crate) fn prepare() {
        for curve in [Self::Warm, Self::Tube, Self::Diode, Self::Transistor] {
            LazyLock::force(match curve {
                Self::Warm => &WARM,
                Self::Tube => &TUBE,
                Self::Diode => &DIODE,
                Self::Transistor => &TRANSISTOR,
            });
        }
    }
}

/// What a hard clip at `low` and `high` takes from `x`, and its integrals from nothing.
#[inline]
fn clip_remainders(x: f64, low: f64, high: f64) -> [f64; 3] {
    let over = if x > high {
        x - high
    } else if x < low {
        x - low
    } else {
        return [0.0; 3];
    };
    [-over, -0.5 * over * over, -over * over * over / 6.0]
}

/// The curve as set, blended toward the hard clip: what it takes from `x`, and its integrals.
#[derive(Clone, Copy)]
struct Blend {
    table: &'static Table,
    soft: f64,
    hard: f64,
}

impl Blend {
    #[inline]
    fn value(self, x: f64) -> f64 {
        self.soft * self.table.value(x)
            + self.hard * clip_remainders(x, self.table.low, self.table.high)[0]
    }

    #[inline]
    fn first(self, x: f64) -> f64 {
        self.soft * self.table.first(x)
            + self.hard * clip_remainders(x, self.table.low, self.table.high)[1]
    }

    #[inline]
    fn second(self, x: f64) -> f64 {
        self.soft * self.table.second(x)
            + self.hard * clip_remainders(x, self.table.low, self.table.high)[2]
    }

    /// The mean of the first integral's divided difference between two points, or the first
    /// integral halfway once they're too close.
    #[inline]
    fn divided(self, a: f64, b: f64, second_a: f64, second_b: f64) -> f64 {
        let apart = a - b;
        if apart.abs() < ADAA_CLOSEST {
            self.first(0.5 * (a + b))
        } else {
            (second_a - second_b) / apart
        }
    }

    /// The remainder through second-order ADAA, for the newest of three samples and the two
    /// before it: the curve's mean over the two intervals between them, weighted by a triangle,
    /// which belongs to the middle sample. `kept` is the second integral at the middle sample
    /// and the divided difference between it and the oldest, which it gives back for the newest,
    /// for the next.
    #[inline]
    fn antialiased(self, [newest, middle, oldest]: [f64; 3], kept: &mut [f64; 2]) -> f64 {
        let [at_middle, before] = *kept;
        let at_newest = self.second(newest);
        let now = self.divided(newest, middle, at_newest, at_middle);
        *kept = [at_newest, now];
        let ends = newest - oldest;
        if ends.abs() >= ADAA_CLOSEST {
            return 2.0 * (now - before) / ends;
        }
        // The two ends are about the same: the triangle's two halves fold together.
        let mean = 0.5 * (newest + oldest);
        let from_middle = mean - middle;
        if from_middle.abs() < ADAA_CLOSEST {
            self.value(0.5 * (mean + middle))
        } else {
            2.0 / from_middle * (self.first(mean) + (at_middle - self.second(mean)) / from_middle)
        }
    }

    /// What `antialiased` keeps, worked out afresh for the last two samples.
    fn kept(self, [middle, oldest]: [f64; 2]) -> [f64; 2] {
        let [at_middle, at_oldest] = [middle, oldest].map(|x| self.second(x));
        [
            at_middle,
            self.divided(middle, oldest, at_middle, at_oldest),
        ]
    }
}

/// What the frame is set to, as eased for a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Colouring {
    pub curve: Curve,
    /// How hard it drives the curve, as a gain: one at none.
    pub drive: f32,
    /// How far the working point moves off the curve's middle, from 0 to 1, toward its nearer
    /// end, for even harmonics.
    pub even: f32,
    /// How far the curve is blended toward a hard clip at its own ends, from 0 to 1, for a
    /// sharper knee and more odd harmonics.
    pub hard: f32,
}

/// A first-order filter in transposed direct form II: `(b0 + b1 z⁻¹) / (1 + a1 z⁻¹)`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct FirstOrder {
    b0: f32,
    b1: f32,
    a1: f32,
}

impl FirstOrder {
    #[inline]
    fn process(&self, state: &mut f32, x: f32) -> f32 {
        let y = self.b0 * x + *state;
        *state = self.b1 * x - self.a1 * y;
        y
    }
}

/// A peaking biquad, the RBJ cookbook's, in transposed direct form II.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Peak {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Peak {
    fn new(hertz: f32, q: f32, db: f32, rate: f32) -> Self {
        let a = 10.0_f32.powf(db / 40.0);
        let omega = std::f32::consts::TAU * hertz / rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos = omega.cos();
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: -2.0 * cos / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    #[inline]
    fn process(&self, state: &mut [f32; 2], x: f32) -> f32 {
        let y = self.b0 * x + state[0];
        state[0] = self.b1 * x - self.a1 * y + state[1];
        state[1] = self.b2 * x - self.a2 * y;
        y
    }
}

same_bits_by_fields!(FirstOrder { b0, b1, a1 });
same_bits_by_fields!(Peak { b0, b1, b2, a1, a2 });

/// The filters round the curve, worked out for the emphasis and the transformer as set: the same
/// for both sides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tuning {
    pre: FirstOrder,
    de: FirstOrder,
    /// The transformer's flux low-pass, its knee and its bump.
    flux: f32,
    knee: f32,
    bump: Peak,
    /// The DC blocker's low-pass.
    dc: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self::new(0.0, 0.0, 48_000)
    }
}

same_bits_by_fields!(Tuning {
    pre,
    de,
    flux,
    knee,
    bump,
    dc
});

impl Tuning {
    /// For `emphasis` from -1, the highs cut before the curve and lifted after, so they saturate
    /// later, through 0, none, to 1, lifted before and cut after, so they saturate first; and
    /// `iron` from 0 to 1.
    pub(crate) fn new(emphasis: f32, iron: f32, rate: u32) -> Self {
        let rate = rate as f32;
        // A first-order high shelf, `(G s + w) / (s + w)`, through the bilinear transform, and
        // its inverse, `(s + w) / (G s + w)`, which is the same with its two halves swapped, so
        // together they're exactly flat.
        let lift = 10.0_f32.powf(emphasis * MOST_EMPHASIS_DB / 20.0);
        let k = (std::f32::consts::PI * EMPHASIS_HZ / rate).tan();
        let pre = FirstOrder {
            b0: (lift + k) / (1.0 + k),
            b1: (k - lift) / (1.0 + k),
            a1: (k - 1.0) / (1.0 + k),
        };
        let de = FirstOrder {
            b0: (1.0 + k) / (lift + k),
            b1: (k - 1.0) / (lift + k),
            a1: (k - lift) / (lift + k),
        };
        let warp = |hertz: f32| one_pole::gain((std::f32::consts::PI * hertz / rate).tan());
        Self {
            pre,
            de,
            flux: warp(IRON_FLUX_HZ),
            knee: iron * MOST_IRON_KNEE,
            bump: Peak::new(BUMP_HZ, BUMP_Q, iron * MOST_BUMP_DB, rate),
            dc: warp(DC_HZ),
        }
    }
}

/// One side's state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Saturator {
    oversampler: OctupleOversampler,
    /// The last two samples at the higher rate, driven, before the working point moved them, and
    /// what the ADAA keeps of them, as moved: the second integral at the last, and the divided
    /// difference between them. These are worked out again when the curve, the working point or
    /// the hardness has moved since, which `held` keeps, so moving any of them can't jump.
    last: [f64; 2],
    kept: [f64; 2],
    held: [f64; 3],
    pre: f32,
    de: f32,
    flux: f32,
    bump: [f32; 2],
    dc: f32,
}

/// What it keeps of the sound, leaving out the integrals worked out from it, which a fresh state
/// works out again.
impl Tiny for Saturator {
    #[inline]
    fn tiny(&self) -> bool {
        let Self {
            oversampler,
            last,
            kept: _,
            held: _,
            pre,
            de,
            flux,
            bump,
            dc,
        } = self;
        oversampler.tiny()
            && last.tiny()
            && pre.tiny()
            && de.tiny()
            && flux.tiny()
            && bump.tiny()
            && dc.tiny()
    }
}

same_bits_by_fields!(Saturator {
    oversampler,
    last,
    kept,
    held,
    pre,
    de,
    flux,
    bump,
    dc
});

impl Saturator {
    /// One sample through the frame.
    #[inline]
    pub(crate) fn process(&mut self, input: f32, colouring: Colouring, tuning: &Tuning) -> f32 {
        // With nothing coming in, all it keeps goes once it's tiny, so it falls to an exact zero
        // rather than trading denormals for good.
        if input == 0.0 {
            flush(self);
        }
        let table = colouring.curve.table();
        let hard = f64::from(colouring.hard);
        let blend = Blend {
            table,
            soft: 1.0 - hard,
            hard,
        };
        // The working point, toward the curve's nearer end, and what the curve takes from a line
        // there and its slope, so what comes out is nothing for nothing and rises at one: the even
        // harmonics without a change of level or DC.
        let nearer = if table.high <= -table.low {
            table.high
        } else {
            table.low
        };
        let bias = f64::from(colouring.even) * MOST_BIAS * nearer;
        let at_rest = blend.value(bias);
        let rising = 1.0 + blend.soft * table.slope(bias);
        let drive = f64::from(colouring.drive);

        // A fresh state's `held` is nothing, which a curve's can be, so its first sample works
        // them out too.
        let held = [colouring.curve as u8 as f64 + 1.0, bias, hard];
        if !held.same_bits(&self.held) {
            self.held = held;
            self.kept = blend.kept(self.last.map(|x| x + bias));
        }
        // Held far past full scale, and a NaN with it, so neither can stay in the filters.
        let input = within(input, -HEADROOM, HEADROOM);
        let emphasised = tuning.pre.process(&mut self.pre, input);
        let Self {
            oversampler,
            last,
            kept,
            ..
        } = self;
        let across = 1.0 / rising;
        let shaped = oversampler.process(emphasised, |x| {
            // The straight line through nothing needs nothing to keep it from aliasing, only the
            // same triangle the rest is weighted by, which centres it on the middle sample too.
            let newest = f64::from(x) * drive;
            let [middle, oldest] = *last;
            let line = (newest + middle + oldest) * (1.0 / 3.0);
            let rest = blend.antialiased([newest + bias, middle + bias, oldest + bias], kept);
            *last = [newest, middle];
            ((line + rest - at_rest) * across) as f32
        });
        let out = tuning.de.process(&mut self.de, shaped);

        // The transformer: its core's flux follows the lows, and saturates as they get loud.
        let flux = one_pole::low_pass(&mut self.flux, out, tuning.flux);
        let out = if tuning.knee > 0.0 {
            out + (tuning.knee * flux).tanh() / tuning.knee - flux
        } else {
            out
        };
        let out = tuning.bump.process(&mut self.bump, out);
        out - one_pole::low_pass(&mut self.dc, out, tuning.dc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const CURVES: [Curve; 4] = [Curve::Warm, Curve::Tube, Curve::Diode, Curve::Transistor];

    fn colouring(curve: Curve, drive_db: f32, even: f32, hard: f32) -> Colouring {
        Colouring {
            curve,
            drive: 10.0_f32.powf(drive_db / 20.0),
            even,
            hard,
        }
    }

    /// A second of sines at `hertz` and their `levels` in dBFS, summed, through the frame; the
    /// second half of what comes out.
    fn played(colouring: Colouring, tuning: &Tuning, tones: &[(f32, f32)]) -> Vec<f32> {
        let mut side = Saturator::default();
        let frames = RATE as usize;
        (0..frames)
            .map(|frame| {
                let t = frame as f64 / f64::from(RATE);
                let input: f64 = tones
                    .iter()
                    .map(|&(hertz, db)| {
                        10.0_f64.powf(f64::from(db) / 20.0)
                            * (t * f64::from(hertz) * std::f64::consts::TAU).sin()
                    })
                    .sum();
                side.process(input as f32, colouring, tuning)
            })
            .skip(frames / 2)
            .collect()
    }

    /// How much of a frequency there is, as a sine's amplitude. Every frequency asked for is a
    /// whole number of cycles in half a second.
    fn level_at(out: &[f32], hertz: f32) -> f64 {
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for (frame, &sample) in out.iter().enumerate() {
            let phase = frame as f64 / f64::from(RATE) * f64::from(hertz) * std::f64::consts::TAU;
            real += f64::from(sample) * phase.cos();
            imaginary += f64::from(sample) * phase.sin();
        }
        2.0 * (real * real + imaginary * imaginary).sqrt() / out.len() as f64
    }

    fn db(ratio: f64) -> f64 {
        20.0 * ratio.max(1e-30).log10()
    }

    /// The 2nd and 3rd harmonics of a sine, and its THD to the 9th, all under the fundamental, in
    /// dB.
    fn harmonics(colouring: Colouring, tuning: &Tuning, hertz: f32, level: f32) -> (f64, f64, f64) {
        let out = played(colouring, tuning, &[(hertz, level)]);
        let fundamental = level_at(&out, hertz);
        let of = |n: f32| level_at(&out, hertz * n) / fundamental;
        let total: f64 = (2..10)
            .map(|n| n as f32)
            .filter(|n| hertz * n < 24_000.0)
            .map(|n| of(n).powi(2))
            .sum();
        (db(of(2.0)), db(of(3.0)), db(total.sqrt()))
    }

    #[test]
    fn every_table_agrees_with_itself() {
        // The integral's slope is the value, and the value's the slope, everywhere, past the ends
        // too, and each curve rises at one through nothing.
        for curve in CURVES {
            let table = curve.table();
            assert_eq!(table.value(0.0), 0.0, "{curve:?}");
            assert_eq!(table.first(0.0), 0.0, "{curve:?}");
            assert_eq!(table.second(0.0), 0.0, "{curve:?}");
            assert!(table.slope(0.0).abs() < 1e-6, "{curve:?}");
            for at in -1100..1100 {
                let x = f64::from(at) * 0.0097;
                let apart = 1e-6;
                let slope = |f: &dyn Fn(f64) -> f64| (f(x + apart) - f(x - apart)) / 2e-6;
                let second = slope(&|x| table.second(x));
                let first = slope(&|x| table.first(x));
                let value = slope(&|x| table.value(x));
                assert!((second - table.first(x)).abs() < 1e-6, "{curve:?} at {x}");
                assert!((first - table.value(x)).abs() < 1e-6, "{curve:?} at {x}");
                assert!(
                    (value - table.slope(x)).abs() < 1e-4,
                    "{curve:?} at {x}: {value} {}",
                    table.slope(x)
                );
                assert!(table.slope(x) > -1.0 - 1e-9, "{curve:?} rises at {x}");
            }
        }
    }

    #[test]
    fn the_tables_are_the_curves_between_their_knots() {
        let worst = (0..4000)
            .map(|at| f64::from(at) * 0.003_7 - 7.4)
            .map(|x| (WARM.value(x) + x - x.tanh()).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-9, "{worst}");
    }

    #[test]
    fn at_no_drive_a_quiet_sound_comes_out_as_it_went_in() {
        // Unity at 1 kHz, flat within half a dB, whatever the emphasis, as its two shelves cancel.
        for curve in CURVES {
            for emphasis in [-1.0, 0.0, 1.0] {
                let tuning = Tuning::new(emphasis, 0.0, RATE);
                let colouring = colouring(curve, 0.0, 0.0, 0.0);
                for hertz in [30.0, 100.0, 1_000.0, 5_000.0, 10_000.0, 18_000.0] {
                    let out = played(colouring, &tuning, &[(hertz, -40.0)]);
                    let change = db(level_at(&out, hertz) / 0.01);
                    let most = if hertz < 2_000.0 && hertz > 500.0 {
                        0.05
                    } else {
                        0.5
                    };
                    assert!(
                        change.abs() < most,
                        "{curve:?} {emphasis} at {hertz} Hz: {change} dB"
                    );
                }
            }
        }
    }

    #[test]
    fn distortion_grows_with_level() {
        // From -40 to +12 dBFS, at 100 Hz, 1 kHz and 5 kHz, a moderate drive.
        for curve in CURVES {
            let tuning = Tuning::new(0.0, 0.0, RATE);
            let colouring = colouring(curve, 6.0, 0.3, 0.0);
            for hertz in [100.0, 1_000.0, 5_000.0] {
                let thd: Vec<f64> = [-40.0, -20.0, -10.0, 0.0, 12.0]
                    .iter()
                    .map(|&level| harmonics(colouring, &tuning, hertz, level).2)
                    .collect();
                assert!(thd[0] < -40.0, "{curve:?} at {hertz}: {thd:?}");
                assert!(
                    thd.windows(2).all(|p| p[1] > p[0] - 1.0),
                    "{curve:?} at {hertz}: {thd:?}"
                );
                assert!(thd[4] > -20.0, "{curve:?} at {hertz}: {thd:?}");
            }
        }
    }

    #[test]
    fn even_brings_up_the_second_harmonic_against_the_third() {
        let tuning = Tuning::new(0.0, 0.0, RATE);
        for curve in CURVES {
            let ratio = |even| {
                let (second, third, _) =
                    harmonics(colouring(curve, 6.0, even, 0.0), &tuning, 1_000.0, -6.0);
                second - third
            };
            let (none, half, full) = (ratio(0.0), ratio(0.5), ratio(1.0));
            assert!(
                none < half && half < full,
                "{curve:?}: {none} {half} {full}"
            );
        }
        // Warm is symmetric: no even harmonics at all without it.
        let (second, _, _) = harmonics(
            colouring(Curve::Warm, 6.0, 0.0, 0.0),
            &tuning,
            1_000.0,
            -6.0,
        );
        assert!(second < -100.0, "{second}");
    }

    #[test]
    fn hard_sharpens_the_knee() {
        // Below the knee a hard clip leaves the sound alone where the soft curve already bends
        // it; driven past it, the hard clip's harmonics fall away more slowly.
        let tuning = Tuning::new(0.0, 0.0, RATE);
        let below = |hard| {
            harmonics(
                colouring(Curve::Warm, 0.0, 0.0, hard),
                &tuning,
                1_000.0,
                -6.0,
            )
            .2
        };
        assert!(
            below(1.0) < below(0.0) - 40.0,
            "{} {}",
            below(0.0),
            below(1.0)
        );
        let spread = |hard| {
            let out = played(
                colouring(Curve::Warm, 18.0, 0.0, hard),
                &tuning,
                &[(1_000.0, -6.0)],
            );
            db(level_at(&out, 9_000.0) / level_at(&out, 3_000.0))
        };
        assert!(
            spread(1.0) > spread(0.0) + 3.0,
            "{} {}",
            spread(0.0),
            spread(1.0)
        );
    }

    #[test]
    fn emphasis_saturates_the_highs_first_or_last() {
        let thd = |emphasis| {
            let tuning = Tuning::new(emphasis, 0.0, RATE);
            harmonics(
                colouring(Curve::Warm, 6.0, 0.0, 0.0),
                &tuning,
                6_000.0,
                -10.0,
            )
            .2
        };
        let (later, none, first) = (thd(-1.0), thd(0.0), thd(1.0));
        assert!(
            later + 6.0 < none && none + 6.0 < first,
            "{later} {none} {first}"
        );
    }

    #[test]
    fn iron_saturates_the_lows_and_lifts_them_a_little() {
        let quiet = |iron, hertz| {
            let tuning = Tuning::new(0.0, iron, RATE);
            let out = played(
                colouring(Curve::Warm, 0.0, 0.0, 0.0),
                &tuning,
                &[(hertz, -40.0)],
            );
            db(level_at(&out, hertz) / 0.01)
        };
        let bump = quiet(1.0, BUMP_HZ);
        assert!((bump - f64::from(MOST_BUMP_DB)).abs() < 0.1, "{bump}");
        assert!(quiet(1.0, 1_000.0).abs() < 0.05);
        // Loud and low, it distorts; loud and high, hardly at all.
        let thd = |hertz| {
            let tuning = Tuning::new(0.0, 1.0, RATE);
            harmonics(colouring(Curve::Warm, 0.0, 0.0, 1.0), &tuning, hertz, -1.0).2
        };
        let (low, high) = (thd(40.0), thd(1_000.0));
        assert!(low > -40.0 && high < low - 30.0, "{low} {high}");
    }

    #[test]
    fn intermodulation_grows_with_level() {
        // SMPTE: 60 Hz and 7 kHz, four to one, and the sidebands either side of 7 kHz.
        let tuning = Tuning::new(0.0, 0.0, RATE);
        for curve in CURVES {
            let imd = |level: f32| {
                let out = played(
                    colouring(curve, 6.0, 0.3, 0.0),
                    &tuning,
                    &[(60.0, level), (7_000.0, level - 12.0)],
                );
                let carrier = level_at(&out, 7_000.0);
                let sidebands: f64 = [-120.0, -60.0, 60.0, 120.0]
                    .iter()
                    .map(|offset| level_at(&out, 7_000.0 + offset).powi(2))
                    .sum();
                db(sidebands.sqrt() / carrier)
            };
            let (quiet, loud) = (imd(-30.0), imd(-3.0));
            assert!(
                quiet < -30.0 && loud > quiet + 20.0,
                "{curve:?}: {quiet} {loud}"
            );
        }
    }

    #[test]
    fn a_loud_10_khz_sine_hardly_aliases() {
        // A 10 kHz sine's harmonics past 24 kHz fold back to multiples of 2 kHz, between its own:
        // all of them under -80 dB, driven as far as it goes, soft or hard, the highs lifted too.
        for curve in CURVES {
            for (drive, hard, emphasis) in [
                (12.0, 0.0, 0.0),
                (24.0, 0.0, 0.0),
                (24.0, 1.0, 0.0),
                (24.0, 1.0, 1.0),
            ] {
                let tuning = Tuning::new(emphasis, 0.0, RATE);
                let out = played(
                    colouring(curve, drive, 0.3, hard),
                    &tuning,
                    &[(10_000.0, 0.0)],
                );
                let fundamental = level_at(&out, 10_000.0);
                let worst = [2e3, 4e3, 6e3, 8e3, 12e3, 14e3, 16e3, 18e3]
                    .iter()
                    .map(|&alias| db(level_at(&out, alias) / fundamental))
                    .fold(f64::MIN, f64::max);
                assert!(
                    worst < -80.0,
                    "{curve:?} {drive} {hard} {emphasis}: {worst} dB"
                );
            }
        }
    }

    #[test]
    fn it_stays_bounded_and_falls_to_nothing() {
        let tuning = Tuning::new(1.0, 1.0, RATE);
        for curve in CURVES {
            let out = played(colouring(curve, 30.0, 1.0, 0.5), &tuning, &[(50.0, 12.0)]);
            let peak = out.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
            assert!(peak.is_finite() && peak < 4.0, "{curve:?}: {peak}");
            // A NaN or an infinity is a click, and no more.
            let mut side = Saturator::default();
            for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                assert!(
                    side.process(x, colouring(curve, 30.0, 1.0, 0.5), &tuning)
                        .is_finite()
                );
            }
            for _ in 0..1000 {
                let out = side.process(1.0, colouring(curve, 30.0, 1.0, 0.5), &tuning);
                assert!(out.is_finite(), "{curve:?}");
            }
            let mut last = 1.0;
            for _ in 0..RATE * 5 {
                last = side.process(0.0, colouring(curve, 30.0, 1.0, 0.5), &tuning);
            }
            assert_eq!(last, 0.0, "{curve:?}");
        }
    }
}
