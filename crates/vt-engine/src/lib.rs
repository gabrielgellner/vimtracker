//! Sequencer and audio engine for vimtracker.

pub mod engine;
pub mod link;
pub mod mixer;
pub mod published;
pub mod render;
pub mod track_state;
pub mod voice;

#[cfg(test)]
mod rt_guard;

#[cfg(test)]
#[global_allocator]
static RT_GUARD: rt_guard::Guarded = rt_guard::Guarded;
