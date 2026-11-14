//! What a filter does to a steady sine, worked out from its own coefficients rather than measured,
//! for drawing the slot effects' curves on the FX page's zoom (decision 333). It's worked out in
//! double precision: a comb's line is thousands of frames long, and a sine's phase that far back
//! has to be exact for its narrow peaks to land where they ring.
//!
//! The track filter's, the vowel filter's, the resonator's and the low-pass gate's state-variable
//! filters, and the comb's one-poles, are all trapezoidal, which is the bilinear transform of an
//! analog filter: what one does at a frequency is what its analog filter does at `tan(pi f) / g`
//! of its cutoff, `g` being the `tan` it was tuned with, however closely that was worked out.

use std::f64::consts::{PI, TAU};

/// A complex gain: how much a sine comes out, and how far it's turned.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Gain {
    pub(crate) re: f64,
    pub(crate) im: f64,
}

impl Gain {
    pub(crate) const ONE: Self = Self { re: 1.0, im: 0.0 };
    pub(crate) const ZERO: Self = Self { re: 0.0, im: 0.0 };

    /// A gain that doesn't turn.
    pub(crate) fn real(re: f64) -> Self {
        Self { re, im: 0.0 }
    }

    pub(crate) fn plus(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    pub(crate) fn minus(self, other: Self) -> Self {
        Self {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }

    pub(crate) fn scale(self, by: f64) -> Self {
        Self {
            re: self.re * by,
            im: self.im * by,
        }
    }

    pub(crate) fn times(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    pub(crate) fn over(self, other: Self) -> Self {
        let size = other.re * other.re + other.im * other.im;
        Self {
            re: (self.re * other.re + self.im * other.im) / size,
            im: (self.im * other.re - self.re * other.im) / size,
        }
    }

    /// How much it lets through.
    pub(crate) fn size(self) -> f64 {
        self.re.hypot(self.im)
    }

    /// The same in dB, with nothing at all as far down as an `f32` goes.
    pub(crate) fn db(self) -> f32 {
        (20.0 * self.size().max(1e-12).log10()) as f32
    }

    /// A sine `frames` frames late, at `turn` cycles a frame: `e^(-2 pi j turn frames)`, with the
    /// cycles it goes through taken off first, so it's exact however far back.
    pub(crate) fn late(turn: f64, frames: f64) -> Self {
        let cycles = turn * frames;
        let (sin, cos) = (TAU * (cycles - cycles.round())).sin_cos();
        Self { re: cos, im: -sin }
    }
}

/// Where a frequency `turn`, in cycles a frame, is on a trapezoidal filter's analog axis:
/// `tan(pi f)`, which a filter tuned with `g` hears as `tan(pi f) / g` of its cutoff.
pub(crate) fn warped(turn: f64) -> f64 {
    (PI * turn.min(0.4999)).tan()
}

/// A state-variable filter's low-pass, band-pass and high-pass at a frequency it hears as `w`, a
/// multiple of its cutoff, with damping `r`, one over its Q: `1`, `s` and `s^2` over
/// `s^2 + r s + 1` with `s = j w`. The band-pass passes `1 / r` at its centre.
pub(crate) fn svf(w: f64, r: f64) -> [Gain; 3] {
    let below = Gain {
        re: 1.0 - w * w,
        im: r * w,
    };
    let s = Gain { re: 0.0, im: w };
    [
        Gain::ONE.over(below),
        s.over(below),
        Gain::real(-w * w).over(below),
    ]
}

/// A one-pole low-pass and high-pass at a frequency each hears as `w` of its cutoff: `1 / (1 + s)`
/// and `s / (1 + s)`.
pub(crate) fn one_pole(w: f64) -> [Gain; 2] {
    let below = Gain { re: 1.0, im: w };
    [Gain::ONE.over(below), Gain { re: 0.0, im: w }.over(below)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_band_pass_passes_its_q_at_its_centre_and_nothing_far_away() {
        let [low, band, high] = svf(1.0, 0.1);
        assert!((band.size() - 10.0).abs() < 1e-9);
        assert!((low.size() - 10.0).abs() < 1e-9 && (high.size() - 10.0).abs() < 1e-9);
        assert!(svf(1e-4, 0.1)[1].size() < 1e-3);
        assert!(svf(1e4, 0.1)[1].size() < 1e-3);
        let [low, high] = one_pole(1.0);
        assert!((low.size() - 0.5_f64.sqrt()).abs() < 1e-12);
        assert!((high.size() - 0.5_f64.sqrt()).abs() < 1e-12);
    }

    #[test]
    fn far_back_is_as_exact_as_one_frame_back() {
        // A quarter of a cycle a frame, a million and one frames back, is a quarter turn late.
        let far = Gain::late(0.25, 1_000_001.0);
        assert!(
            far.re.abs() < 1e-9 && (far.im + 1.0).abs() < 1e-9,
            "{far:?}"
        );
        let near = Gain::late(0.25, 1.0);
        assert!((far.re - near.re).abs() < 1e-9 && (far.im - near.im).abs() < 1e-9);
    }
}
