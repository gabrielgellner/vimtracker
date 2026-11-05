//! A one-pole smoother for filter, volume, pan and send changes.

use vt_model::constants::SMOOTHING_MS;

/// How much of the remaining distance to cover each frame.
pub fn coefficient(rate: u32) -> f32 {
    let frames = SMOOTHING_MS as f32 / 1000.0 * rate as f32;
    1.0 - (-1.0 / frames.max(1.0)).exp()
}

/// A value moving toward its target.
#[derive(Debug, Clone, Copy, Default)]
pub struct Smoothed {
    pub target: f32,
    pub value: f32,
}

impl Smoothed {
    /// Start at a value with nothing to move toward.
    pub fn at(value: f32) -> Self {
        Self {
            target: value,
            value,
        }
    }

    /// Jump straight to the target.
    pub fn settle(&mut self) {
        self.value = self.target;
    }

    /// Move one frame.
    #[inline]
    pub fn next(&mut self, coefficient: f32) -> f32 {
        self.value += (self.target - self.value) * coefficient;
        self.value
    }
}
