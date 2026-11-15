//! The trapezoidal one-pole, from Zavalishin's *The Art of VA Filter Design*: the low-pass that
//! the Korg 35, the diode and dirty filters, the comb's band, the phaser's all-passes, Echo's
//! shelves and stmlib's `OnePole` are built from. Its gain `G` is `g / (1 + g)`, where `g` is the
//! warped frequency, `tan(pi f / rate)` or an approximation of it. Each user warps its own way, so
//! only the last step is here.

/// A trapezoidal one-pole's gain `G`, from its warped frequency `g`.
#[inline]
pub(crate) fn gain(warped: f32) -> f32 {
    warped / (1.0 + warped)
}

/// One sample of a trapezoidal one-pole low-pass, whose state is `state` and gain `gain`. Its
/// high-pass is what comes in less this.
#[inline]
pub(crate) fn low_pass(state: &mut f32, input: f32, gain: f32) -> f32 {
    let step = (input - *state) * gain;
    let out = step + *state;
    *state = out + step;
    out
}
