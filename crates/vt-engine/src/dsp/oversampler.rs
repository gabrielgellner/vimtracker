//! Going to twice the rate and back, for what would otherwise alias.

/// A chain of first-order allpass filters, `(a + z^-1) / (1 + a z^-1)`.
#[derive(Debug, Clone, Copy, Default)]
struct Allpass {
    state: [f32; 3],
}

impl Allpass {
    #[inline]
    fn process(&mut self, mut x: f32, coefficients: &[f32; 3]) -> f32 {
        for (state, a) in self.state.iter_mut().zip(coefficients) {
            let y = a * x + *state;
            *state = x - a * y;
            x = y;
        }
        x
    }
}

/// A polyphase halfband's two branches, for going to and from twice the rate. The coefficients are
/// from VCV Fundamental's VCF, GPL 3 or later.
#[allow(clippy::excessive_precision)]
const HALFBAND_A: [f32; 3] = [0.062_822_416_060, 0.424_380_855_72, 0.781_861_460_39];
#[allow(clippy::excessive_precision)]
const HALFBAND_B: [f32; 3] = [0.223_807_330_34, 0.616_534_435_04, 0.927_473_594_87];

/// Going to twice the rate and back, so that what saturates aliases less.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Oversampler {
    up: [Allpass; 2],
    down: [Allpass; 2],
}

impl Oversampler {
    /// One sample at the normal rate, with `step` run twice at the higher one.
    #[inline]
    pub(crate) fn process(&mut self, input: f32, mut step: impl FnMut(f32) -> f32) -> f32 {
        let first = step(self.up[0].process(input, &HALFBAND_A));
        let second = step(self.up[1].process(input, &HALFBAND_B));
        // The branches swap on the way down, for unity gain in the passband.
        let a = self.down[0].process(second, &HALFBAND_A);
        let b = self.down[1].process(first, &HALFBAND_B);
        0.5 * (a + b)
    }
}
