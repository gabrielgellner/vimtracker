//! The voices, which make a track's sound from its notes. Most are ported from Mutable Instruments'
//! Plaits, and share its settings, blocks and band-limited steps through [`virtual_analog`]; the
//! `braids_` ones are from Braids, its older macro-oscillator, and run at twice the rate through
//! `braids_parts`. The tables they read are in `voices/tables/`.

pub(crate) mod acid;
pub(crate) mod additive;
pub(crate) mod braids_analog;
pub(crate) mod braids_digital;
pub(crate) mod braids_filter_fm;
pub(crate) mod braids_noise;
mod braids_parts;
pub(crate) mod braids_physical;
pub(crate) mod braids_vocal;
pub(crate) mod braids_waves;
pub(crate) mod chiptune;
pub(crate) mod chord;
mod chord_bank;
pub(crate) mod drums;
pub(crate) mod fm;
pub(crate) mod four_op;
pub(crate) mod grain;
pub(crate) mod kick;
pub(crate) mod modal;
pub(crate) mod noise;
pub(crate) mod particle;
pub(crate) mod phase_distortion;
pub(crate) mod six_op;
pub(crate) mod speech;
pub(crate) mod string_model;
pub(crate) mod supersaw;
pub(crate) mod swarm;
mod variable_shape;
pub(crate) mod virtual_analog;
pub(crate) mod virtual_analog_vcf;
pub(crate) mod wave_terrain;
pub(crate) mod waveshaping;
mod waveshaping_tables;
pub(crate) mod wavetable;
