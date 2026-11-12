//! The voices, which make a track's sound from its notes. Most are ported from Mutable Instruments'
//! Plaits, and share its settings, blocks and band-limited steps through [`virtual_analog`]; the
//! `braids_` ones are from Braids, its older macro-oscillator, and run at twice the rate through
//! `braids_parts`. The tables they read are in `voices/tables/`.

pub mod acid;
pub mod additive;
pub mod braids_analog;
mod braids_parts;
pub mod chiptune;
pub mod chord;
mod chord_bank;
pub mod drums;
pub mod fm;
pub mod grain;
pub mod kick;
pub mod modal;
pub mod noise;
pub mod particle;
pub mod phase_distortion;
pub mod six_op;
pub mod string_model;
pub mod swarm;
mod variable_shape;
pub mod virtual_analog;
pub mod virtual_analog_vcf;
pub mod wave_terrain;
pub mod waveshaping;
mod waveshaping_tables;
pub mod wavetable;
