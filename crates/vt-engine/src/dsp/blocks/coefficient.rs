//! A one-pole's coefficient, from a time constant or a corner, as the smoothers, the dynamics'
//! followers and the effects' tone filters each work it out. Each formula is written in one
//! order, so a site moves here only where its own was the same to the bit. Those that count in
//! milliseconds as `1000 / (ms rate)`, leave out the `max(1)`, or work in `f64` keep their own.

use std::f32::consts::TAU;

/// How much of the rest of the way a one-pole covers each frame, for a time constant in seconds.
/// One shorter than a frame covers all of it.
#[inline]
pub(crate) fn of_time(seconds: f32, rate: u32) -> f32 {
    1.0 - kept_over(seconds, rate)
}

/// How much of the last value a one-pole keeps each frame, for a time constant in seconds.
#[inline]
pub(crate) fn kept_over(seconds: f32, rate: u32) -> f32 {
    (-1.0 / (seconds * rate as f32).max(1.0)).exp()
}

/// How far a one-pole low-pass moves toward its input each frame, for its corner in Hz.
#[inline]
pub(crate) fn of_corner(hertz: f32, rate: f32) -> f32 {
    1.0 - (-TAU * hertz / rate).exp()
}
