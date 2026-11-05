//! A TPT state-variable low-pass, which stays stable near the top of the range.

/// Filter coefficients. Cutoff is `0..=1`, exponential from 20 Hz to 20 kHz.
#[derive(Debug, Clone, Copy)]
pub struct Coefficients {
    g: f32,
    a1: f32,
}

impl Coefficients {
    /// Work out coefficients.
    pub fn new(cutoff: f32, resonance: f32, rate: u32) -> Self {
        let frequency = 20.0 * 1000_f32.powf(cutoff);
        let nyquist_safe = frequency.min(rate as f32 * 0.45);
        let g = (std::f32::consts::PI * nyquist_safe / rate as f32).tan();
        // Q runs from a flat response to a strong but stable peak.
        let q = 0.707 + resonance * 9.3;
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        Self { g, a1 }
    }
}

/// One channel's state.
#[derive(Debug, Clone, Copy, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Filter one sample.
    #[inline]
    pub fn low_pass(&mut self, input: f32, c: &Coefficients) -> f32 {
        let v3 = input - self.ic2;
        let v1 = c.a1 * self.ic1 + c.a1 * c.g * v3;
        let v2 = self.ic2 + c.g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}
