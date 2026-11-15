//! What the tests share: golden renders, a sound's peak and RMS, a render that repeats, and the
//! samples, songs and renders that more than one file builds the same way. A file's own variants
//! stay in the file. The DSP unit tests' signals are in `src/dsp/signals.rs`.

use std::borrow::Borrow;
use std::path::PathBuf;
use std::sync::Arc;

use vt_engine::published::Published;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

/// FNV-1a over the samples' exact bits, so a golden catches any change at all.
fn hash(samples: &[f32]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for sample in samples {
        for byte in sample.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Compare with a saved hash. `VT_BLESS=1` updates it after an intended change to the sound, and
/// `VT_DUMP=folder` writes the samples there, to compare two builds by how far apart they are.
///
/// These hold on one platform: the filter and reverb use library math that can differ elsewhere.
pub fn golden(name: &str, samples: &[f32]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.hash"));
    let found = hash(samples);
    if let Some(dump) = std::env::var_os("VT_DUMP") {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(format!("{}/{name}.raw", dump.to_string_lossy()), bytes).unwrap();
    }
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::write(&path, format!("{found}\n")).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    assert_eq!(
        saved.trim(),
        found,
        "{name} changed: run with VT_BLESS=1 if that was intended"
    );
}

/// The loudest a sound gets, either side of zero.
pub fn peak(samples: impl IntoIterator<Item = impl Borrow<f32>>) -> f32 {
    samples
        .into_iter()
        .fold(0.0, |most: f32, s| most.max(s.borrow().abs()))
}

/// The root of the mean of the squares: how loud a sound is on average.
pub fn rms(samples: impl IntoIterator<Item = impl Borrow<f32>>) -> f32 {
    let (sum, count) = samples.into_iter().fold((0.0, 0), |(sum, count), s| {
        let s = *s.borrow();
        (sum + s * s, count + 1)
    });
    (sum / count as f32).sqrt()
}

/// Render twice and check the two are the same, sample for sample, so what's random follows the
/// song's seed and nothing carries over from one render to the next. Gives the render back.
pub fn repeats(what: &str, render: impl Fn() -> Vec<f32>) -> Vec<f32> {
    let (first, second) = (render(), render());
    if first != second {
        let at = first.iter().zip(&second).position(|(a, b)| a != b);
        panic!(
            "{what}: the second render differs, at sample {at:?}, {} samples against {}",
            first.len(),
            second.len()
        );
    }
    first
}

/// A pool of one sample, `s`, of these frames.
pub fn pool(frames: Vec<[f32; 2]>) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// A song with one track playing these step texts on `s`, and instant envelopes.
pub fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// A song published with these samples, for an engine to play.
pub fn version(song: &Song, pool: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool))
}

/// Songs that play no samples, such as synth tracks and what generators write.
pub mod no_samples {
    use super::*;

    pub fn version(song: &Song) -> Arc<Published> {
        Arc::new(Published::new(
            Arc::new(song.clone()),
            Arc::new(SamplePool::default()),
        ))
    }
}

/// A steady sample at full scale, `s`, twenty seconds long, so what's heard is what modulation
/// does to it.
pub mod steady {
    use super::*;

    pub fn flat() -> Arc<SamplePool> {
        let mut pool = SamplePool::default();
        pool.insert(
            "s",
            SampleData {
                frames: vec![[1.0, 1.0]; RATE as usize * 20],
            },
        );
        Arc::new(pool)
    }

    pub fn version(song: &Song) -> Arc<Published> {
        Arc::new(Published::new(Arc::new(song.clone()), flat()))
    }
}

/// A ramp from 0 to 1, `ramp`, so a note's pitch is how fast it climbs and a note that starts
/// again starts from 0.
pub mod ramp {
    use vt_engine::render::render_frames;

    use super::*;

    /// The ramp's length.
    pub const RAMP_FRAMES: usize = RATE as usize * 8;

    pub fn pool() -> Arc<SamplePool> {
        let mut pool = SamplePool::default();
        let frames = (0..RAMP_FRAMES)
            .map(|at| {
                let level = at as f32 / RAMP_FRAMES as f32;
                [level, level]
            })
            .collect();
        pool.insert("ramp", SampleData { frames });
        Arc::new(pool)
    }

    pub fn version(song: &Song) -> Arc<Published> {
        Arc::new(Published::new(Arc::new(song.clone()), pool()))
    }

    /// The left channel, for this many steps.
    pub fn render(song: &Song, steps: usize) -> Vec<f32> {
        render_frames(version(song), RATE, steps * STEP)
            .chunks(2)
            .map(|frame| frame[0])
            .collect()
    }
}

/// A long ramp from 0.25 to 0.75, `s`, so a note's level says how far into it the note is and
/// its velocity how high it starts.
pub mod rising {
    use super::*;

    pub fn pool() -> Arc<SamplePool> {
        let frames = RATE as usize * 8;
        let mut pool = SamplePool::default();
        pool.insert(
            "s",
            SampleData {
                frames: (0..frames)
                    .map(|at| {
                        let level = 0.25 + 0.5 * at as f32 / frames as f32;
                        [level, level]
                    })
                    .collect(),
            },
        );
        Arc::new(pool)
    }

    /// Tracks playing these rows, with notes that start and stop at once, in a section long
    /// enough that the launch grid isn't cut short by its repeats.
    pub fn song(tracks: &[&[&str]]) -> Song {
        let mut song = Song::new(tracks.len());
        song.limiter = false;
        song.sections[0].length = Some(64);
        for (index, rows) in tracks.iter().enumerate() {
            song.sample_on(index, "s");
            let track = &mut song.tracks[index];
            track.mods[0].adsr.attack_ms = 0;
            track.mods[0].adsr.release_ms = 0;
            let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
            track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
        }
        song
    }

    pub fn version(song: &Song) -> Arc<Published> {
        Arc::new(Published::new(Arc::new(song.clone()), pool()))
    }
}

/// A sample that holds one level, `flat`, so what's heard is the note's envelope and gain alone.
pub mod level {
    use vt_engine::render::render_frames;

    use super::*;

    pub fn pool() -> Arc<SamplePool> {
        let mut pool = SamplePool::default();
        let frames = vec![[0.5, 0.5]; RATE as usize * 8];
        pool.insert("flat", SampleData { frames });
        Arc::new(pool)
    }

    /// One sample track playing these rows, with notes that start and stop at once, so the
    /// moment one ends can be seen.
    pub fn song(rows: &[&str]) -> Song {
        let mut song = Song::new(1);
        song.limiter = false;
        song.sample_on(0, "flat");
        song.tracks[0].mods[0].adsr.attack_ms = 0;
        song.tracks[0].mods[0].adsr.release_ms = 0;
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
        song
    }

    /// The left channel, for this many steps.
    pub fn render(song: &Song, steps: usize) -> Vec<f32> {
        let version = Arc::new(Published::new(Arc::new(song.clone()), pool()));
        render_frames(version, RATE, steps * STEP)
            .chunks(2)
            .map(|frame| frame[0])
            .collect()
    }
}

/// Tracks on `s`, panned apart when there are two so each channel is one track's.
pub mod panned {
    use vt_engine::render::render_frames;

    use super::*;

    /// Tracks playing these rows, each on its own.
    pub fn song(tracks: &[&[&str]]) -> Song {
        let mut song = Song::new(tracks.len());
        song.limiter = false;
        for (index, rows) in tracks.iter().enumerate() {
            song.sample_on(index, "s");
            let track = &mut song.tracks[index];
            track.mods[0].adsr.attack_ms = 0;
            track.mods[0].adsr.release_ms = 0;
            let pan = match (tracks.len(), index) {
                (1, _) => "",
                (_, 0) => " pan:00",
                _ => " pan:FF",
            };
            let steps = rows
                .iter()
                .map(|text| format!("{text}{pan}").parse().unwrap())
                .collect();
            track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
        }
        song
    }

    /// One channel of a render, for this many steps.
    pub fn render(song: &Song, pool: Arc<SamplePool>, steps: usize, channel: usize) -> Vec<f32> {
        render_frames(version(song, pool), RATE, steps * STEP)
            .chunks(2)
            .map(|frame| frame[channel])
            .collect()
    }
}

/// A plucked chord a quarter of a second long, `pluck`, then nothing: a few sines and their
/// octaves, dying away.
pub mod pluck {
    use super::*;

    pub fn pool() -> Arc<SamplePool> {
        let frames = (0..RATE as usize / 4)
            .map(|frame| {
                let t = frame as f32 / RATE as f32;
                let value = [130.8_f32, 196.0, 261.6, 329.6, 523.2]
                    .iter()
                    .map(|hertz| (t * hertz * std::f32::consts::TAU).sin() * 0.12)
                    .sum::<f32>()
                    * (-t * 6.0).exp();
                [value, value]
            })
            .collect();
        let mut pool = SamplePool::default();
        pool.insert("pluck", SampleData { frames });
        Arc::new(pool)
    }
}

/// A short decaying tone, `tone`, computed so no audio file is needed.
pub mod tone {
    use super::*;

    pub fn pool() -> Arc<SamplePool> {
        let frames = (0..4800)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let value =
                    (t * 220.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 4800.0) * 0.8;
                [value, value]
            })
            .collect();
        let mut pool = SamplePool::default();
        pool.insert("tone", SampleData { frames });
        Arc::new(pool)
    }
}
