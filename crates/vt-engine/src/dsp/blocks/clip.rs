//! A clamp that's safe in a feedback loop. `f32::clamp` gives a NaN back for a NaN, and panics if
//! either end is one, so a NaN that reaches a loop through it stays there for good and silences
//! the tail. `max` and `min` give the other value when one is a NaN, so here a NaN comes out at
//! the low end instead, and goes round as a click that dies away. sst clips this way on purpose
//! (`sst-basic-blocks/dsp/Clippers.h:115-129`).

/// `x` held between `low` and `high`, with a NaN held to `low` (#461).
#[inline]
#[allow(clippy::manual_clamp)]
pub(crate) fn within(x: f32, low: f32, high: f32) -> f32 {
    x.max(low).min(high)
}
