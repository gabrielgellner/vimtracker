//! The small parts the voices, filters and effects share. Nothing here uses anything else in
//! `dsp`, so a block can go wherever it's needed.

pub mod oversampler;
pub(super) mod plaits_parts;
pub(super) mod sine_table;
pub mod smoother;
