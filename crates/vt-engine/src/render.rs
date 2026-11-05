//! Rendering with no sound device.

use std::sync::Arc;

use crate::engine::Engine;
use crate::published::Published;

/// Play a version from the top and return interleaved stereo frames.
pub fn render_frames(version: Arc<Published>, rate: u32, frames: usize) -> Vec<f32> {
    let mut engine = Engine::new(rate, version);
    engine.play_from(0);
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out
}
