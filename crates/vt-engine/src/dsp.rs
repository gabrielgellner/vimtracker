//! The sound itself: voices, filters, effects and dynamics, one per file so each can be tested
//! alone, on shared building blocks.
//!
//! - [`voices`] make a track's sound from its notes.
//! - [`filters`] shape it: the track's filter and the character filters it switches between,
//!   and the slot effects that are filters.
//! - [`effects`] are the other slot effects, with what plays a slot and the long buffers they take.
//! - [`dynamics`] are the mixing tools: compressor, limiter and EQ.
//! - [`blocks`] are the small parts the others share, which use nothing else here.

pub mod blocks;
pub mod dynamics;
pub mod effects;
pub mod filters;
pub mod voices;

// What the rest of the engine and vt-tui use, at the paths they had before the folder was grouped.
pub use blocks::smoother;
pub use dynamics::{compressor, eq};
pub use effects::{distort, memory};
pub use filters::filter;
pub use voices::{
    acid, additive, braids_analog, braids_digital, braids_filter_fm, braids_noise, braids_physical,
    braids_vocal, braids_waves, chiptune, chord, drums, fm, grain, kick, modal, noise, particle,
    phase_distortion, six_op, speech, string_model, supersaw, swarm, virtual_analog,
    virtual_analog_vcf, wave_terrain, waveshaping, wavetable,
};
