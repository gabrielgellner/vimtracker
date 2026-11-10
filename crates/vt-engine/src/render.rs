//! Rendering with no sound device.

use std::path::Path;
use std::sync::Arc;

use vt_model::constants::{MAX_MILLIBPM, MIN_MILLIBPM, TICKS_PER_STEP};
use vt_model::song::Song;

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
    /// The length in steps. If not set, the section's reset, or else its longest chain.
    pub rows: Option<usize>,
    /// Let sound ring out after the last step, ending at silence.
    pub tail: bool,
    /// Render a stem of each track, rather than each group as one.
    pub each_track: bool,
}

/// Play a version from the top and return interleaved stereo frames.
pub fn render_frames(version: Arc<Published>, rate: u32, frames: usize) -> Vec<f32> {
    let mut engine = Engine::new(rate, version);
    engine.play_from(0);
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out
}

/// Render a loop: one silent pass fills delay and reverb, then every track goes back to its start
/// and the next pass is captured, so the file is exactly the loop's length and its start hears its
/// own end. Tracks whose lengths don't divide the file's won't join up when it repeats.
pub fn render_loop(version: Arc<Published>, rate: u32, options: RenderOptions) -> Vec<f32> {
    let song = Arc::clone(&version.song);
    let millibpm = song.millibpm.clamp(MIN_MILLIBPM, MAX_MILLIBPM);
    let longest = song.song_steps();
    if longest == 0 {
        return Vec::new();
    }
    // A song with sections is rendered through once, and one section as its loop.
    let mut playing = song.sections.iter().filter(|s| song.section_length(s) > 0);
    let section = match (playing.next(), playing.next()) {
        (Some(only), None) => only.reset.unwrap_or(longest),
        _ => longest,
    };
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
    engine.rewind();
    let mut out = vec![0.0; length * 2];
    engine.process(&mut out);
    if options.tail {
        append_tail(&mut engine, rate, &mut out);
    }
    out
}

/// One part of a song: a track's, or a group's with its tracks.
#[derive(Debug)]
pub struct Stem {
    /// The track it's of: a sound track, or a group's track.
    pub track: usize,
    pub samples: Vec<f32>,
}

/// The tracks a stem render makes a stem of, in order: each sound track that would be heard, or,
/// unless asked for each track, each group with a track that would be heard in place of its
/// tracks.
pub fn stem_tracks(song: &Song, each_track: bool) -> Vec<usize> {
    let sounds = song.sound_tracks();
    let mut tracks: Vec<usize> = (0..sounds)
        .filter(|track| song.heard(*track))
        .map(|track| match song.group_track_of(track) {
            Some(group) if !each_track => group,
            _ => track,
        })
        .collect();
    tracks.sort_unstable();
    tracks.dedup();
    tracks
}

/// Render each stem on its own, with the limiter off so the stems add up to the whole: each track
/// that plays sounds and would be heard, or by default each group as one, in place of its tracks.
/// Each keeps what it sends to the returns, and they all end together.
pub fn render_stems(version: &Published, rate: u32, options: RenderOptions) -> Vec<Stem> {
    let song = &version.song;
    let mut stems: Vec<Stem> = stem_tracks(song, options.each_track)
        .into_iter()
        .map(|index| {
            let mut alone = Song::clone(song);
            alone.limiter = false;
            // Soloed, it plays, through its group if it's in one, and a group plays its tracks
            // that aren't muted themselves. The returns stay as they are, as what it sends goes
            // through them.
            let group = song.group_track_of(index);
            for (other, track) in alone.tracks.iter_mut().enumerate() {
                if other == index || Some(other) == group {
                    track.mute = false;
                }
                track.solo = other == index;
            }
            let alone = Published::new(Arc::new(alone), Arc::clone(&version.samples));
            Stem {
                track: index,
                samples: render_loop(Arc::new(alone), rate, options),
            }
        })
        .collect();
    let longest = stems
        .iter()
        .map(|stem| stem.samples.len())
        .max()
        .unwrap_or(0);
    for stem in &mut stems {
        stem.samples.resize(longest, 0.0);
    }
    stems
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
