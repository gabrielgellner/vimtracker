//! Rendering with no sound device.

use std::path::Path;
use std::sync::Arc;

use vt_model::constants::{MAX_MILLIBPM, MIN_MILLIBPM, TICKS_PER_STEP};

use crate::engine::{Engine, sample_of_tick};
use crate::published::Published;

/// The longest tail `--tail` adds, in seconds.
const MAX_TAIL_SECONDS: usize = 10;
/// A tail has ended once it stays below this level, about -80 dB, for `SILENT_SECONDS`.
const SILENCE: f32 = 0.0001;
const SILENT_SECONDS: f32 = 0.25;

/// What to capture.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    /// The length in steps, or the section's length if not set.
    pub rows: Option<usize>,
    /// Let sound ring out after the last step, ending at silence.
    pub tail: bool,
}

/// Play a version from the top and return interleaved stereo frames.
pub fn render_frames(version: Arc<Published>, rate: u32, frames: usize) -> Vec<f32> {
    let mut engine = Engine::new(rate, version);
    engine.play_from(0);
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out
}

/// Render a loop: one silent pass fills delay and reverb, then the next pass is captured from the
/// top, so the file is exactly the loop's length and its start hears its own end.
pub fn render_loop(version: Arc<Published>, rate: u32, options: RenderOptions) -> Vec<f32> {
    let song = Arc::clone(&version.song);
    let millibpm = song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
    let section = song.sections.first().map_or(0, |s| song.section_steps(s));
    if section == 0 {
        return Vec::new();
    }
    let at_step =
        |step: usize| sample_of_tick(step as u64 * TICKS_PER_STEP, rate, millibpm) as usize;
    let warm = at_step(section);
    let length = at_step(section + options.rows.unwrap_or(section)) - warm;

    let mut engine = Engine::new(rate, version);
    for (index, track) in song.tracks.iter().enumerate() {
        engine.set_mute(index, track.mute);
        engine.set_solo(index, track.solo);
    }
    engine.play_from(0);
    let mut discard = vec![0.0; warm * 2];
    engine.process(&mut discard);
    let mut out = vec![0.0; length * 2];
    engine.process(&mut out);
    if options.tail {
        append_tail(&mut engine, rate, &mut out);
    }
    out
}

/// Stop sequencing and keep what rings until it stays silent, up to the limit.
fn append_tail(engine: &mut Engine, rate: u32, out: &mut Vec<f32>) {
    engine.let_ring();
    let quiet_limit = (SILENT_SECONDS * rate as f32) as usize;
    let mut block = vec![0.0; 2 * 256];
    let mut tail: Vec<f32> = Vec::new();
    let mut last_loud = 0;
    let mut frames = 0;
    while frames < MAX_TAIL_SECONDS * rate as usize && frames - last_loud < quiet_limit {
        engine.process(&mut block);
        tail.extend_from_slice(&block);
        for frame in block.as_chunks::<2>().0 {
            frames += 1;
            if frame[0].abs() > SILENCE || frame[1].abs() > SILENCE {
                last_loud = frames;
            }
        }
    }
    out.extend_from_slice(&tail[..last_loud * 2]);
}

/// Write interleaved stereo frames as a 32-bit float WAV, replacing any file there.
pub fn write_wav(path: &Path, rate: u32, samples: &[f32]) -> Result<(), hound::Error> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples {
        writer.write_sample(*sample)?;
    }
    writer.finalize()
}
