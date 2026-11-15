//! A clamp that's safe in a feedback loop, and stmlib's soft clip built on it. `f32::clamp` gives
//! a NaN back for a NaN, and panics if either end is one, so a NaN that reaches a loop through it
//! stays there for good and silences the tail. `max` and `min` give the other value when one is a
//! NaN, so here a NaN comes out at the low end instead, and goes round as a click that dies away.
//! sst clips this way on purpose (`sst-basic-blocks/dsp/Clippers.h:115-129`).

/// `x` held between `low` and `high`, with a NaN held to `low` (#461).
#[inline]
#[allow(clippy::manual_clamp)]
pub(crate) fn within(x: f32, low: f32, high: f32) -> f32 {
    x.max(low).min(high)
}

/// stmlib's `SoftLimit`: a gentle curve near unity for quiet signals, reaching 1 at 3.
#[inline]
pub(crate) fn soft_limit(x: f32) -> f32 {
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// stmlib's `SoftClip`, which holds a sample within -1 to 1: [`soft_limit`] up to 3, and 1 past
/// it. It's exactly ±1 at ±3, so holding what goes in to that range does what stmlib's branches
/// do, but a NaN comes out at an end: a NaN that went round a feedback loop would stay there for
/// good.
#[inline]
pub(crate) fn soft_clip(x: f32) -> f32 {
    soft_limit(within(x, -3.0, 3.0))
}
