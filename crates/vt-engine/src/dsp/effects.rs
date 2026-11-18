//! The slot effects that aren't filters or dynamics, and what plays a slot: [`slot`] keeps a
//! slot's state and plays whichever effect it has, and [`memory`] keeps the long buffers some
//! effects take. The table Warps reads is in `effects/tables/`.

pub(crate) mod beads;
pub(crate) mod chorus;
pub(crate) mod clouds;
pub(crate) mod colour;
pub(crate) mod delay;
pub(crate) mod distort;
pub(crate) mod echo;
pub(crate) mod exciter;
pub(crate) mod flanger;
pub(crate) mod freq_shifter;
pub(crate) mod granular;
pub(crate) mod memory;
pub(crate) mod pedal;
pub(crate) mod phaser;
pub(crate) mod pitch_shifter;
pub(crate) mod plate;
pub(crate) mod resonator;
pub(crate) mod reverb;
pub(crate) mod rotary;
pub(crate) mod slot;
pub(crate) mod space;
pub(crate) mod spring;
pub(crate) mod stutter;
#[cfg(test)]
pub(crate) mod sweep;
pub(crate) mod tail;
pub(crate) mod tape;
pub(crate) mod warps;
