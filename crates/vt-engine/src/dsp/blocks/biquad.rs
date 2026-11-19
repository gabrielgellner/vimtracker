//! A biquad in transposed direct form II, with the RBJ cookbook's peak, high-pass and high shelf:
//! the colour's transformer bump, the reel's and deck's head bumps and the reel's playback head.

use std::f32::consts::TAU;

use super::sides::same_bits_by_fields;

/// `(b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`; its state is the caller's.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

same_bits_by_fields!(Biquad { b0, b1, b2, a1, a2 });

impl Biquad {
    /// No filter at all, to the bit.
    pub(crate) const FLAT: Self = Self {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
    };

    /// A high-pass at `hertz`, `q` sharp.
    pub(crate) fn high_pass(hertz: f32, q: f32, rate: f32) -> Self {
        let omega = TAU * hertz / rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: 0.5 * (1.0 + cos) / a0,
            b1: -(1.0 + cos) / a0,
            b2: 0.5 * (1.0 + cos) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// A peak at `hertz`, `q` wide, lifting by `db` (cutting below nothing).
    pub(crate) fn peak(hertz: f32, q: f32, db: f32, rate: f32) -> Self {
        let a = 10.0_f32.powf(db / 40.0);
        let omega = TAU * hertz / rate;
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

    /// A high shelf from about `hertz`, `q` sharp, lifting the top by `db` (cutting below
    /// nothing).
    pub(crate) fn high_shelf(hertz: f32, q: f32, db: f32, rate: f32) -> Self {
        let a = 10.0_f32.powf(db / 40.0);
        let omega = TAU * hertz / rate;
        let alpha = omega.sin() / (2.0 * q);
        let cos = omega.cos();
        let root = 2.0 * a.sqrt() * alpha;
        let a0 = (a + 1.0) - (a - 1.0) * cos + root;
        Self {
            b0: a * ((a + 1.0) + (a - 1.0) * cos + root) / a0,
            b1: -2.0 * a * ((a - 1.0) + (a + 1.0) * cos) / a0,
            b2: a * ((a + 1.0) + (a - 1.0) * cos - root) / a0,
            a1: 2.0 * ((a - 1.0) - (a + 1.0) * cos) / a0,
            a2: ((a + 1.0) - (a - 1.0) * cos - root) / a0,
        }
    }

    #[inline]
    pub(crate) fn process(&self, state: &mut [f32; 2], x: f32) -> f32 {
        let y = self.b0 * x + state[0];
        state[0] = self.b1 * x - self.a1 * y + state[1];
        state[1] = self.b2 * x - self.a2 * y;
        y
    }
}
