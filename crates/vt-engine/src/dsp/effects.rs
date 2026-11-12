//! The slot effects that aren't filters or dynamics, and what plays a slot: [`distort`] keeps a
//! slot's state and plays whichever effect it has, and [`memory`] keeps the long buffers some
//! effects take. The table Warps reads is in `effects/tables/`.

pub mod chorus;
pub mod delay;
pub mod distort;
pub mod exciter;
pub mod flanger;
pub mod freq_shifter;
pub mod granular;
pub mod memory;
pub mod pedal;
pub mod phaser;
pub mod pitch_shifter;
pub mod plate;
pub mod resonator;
pub mod reverb;
pub mod space;
pub mod spring;
pub mod tape;
pub mod warps;
