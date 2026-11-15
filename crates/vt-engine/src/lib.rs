//! Sequencer and audio engine for vimtracker.
//!
//! The sound itself, in `dsp`, is the crate's own. What others draw of it is in [`pictures`], and
//! the parts the tests play on their own are in [`parts`].

#![warn(unreachable_pub)]

pub(crate) mod dsp;
pub mod engine;
pub mod gens;
pub mod link;
pub mod mixer;
pub mod parts;
pub mod pictures;
mod preview;
pub mod published;
mod queues;
pub(crate) mod reader;
pub mod recorder;
pub mod render;
pub(crate) mod rng;
pub mod slowdown;
pub mod timing;
pub mod track_state;
pub(crate) mod voice;

#[cfg(test)]
mod rt_guard;

#[cfg(test)]
#[global_allocator]
static RT_GUARD: rt_guard::Guarded = rt_guard::Guarded;
