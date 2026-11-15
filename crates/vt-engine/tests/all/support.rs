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

/// Compare with a saved hash, and with what the render sounded like when it was saved (#841). The
/// hash catches any change at all; the sound, each of whose measures must stay in its band, says
/// how far it moved, so a change meant can be blessed with evidence and a broken one told apart
/// from it. `VT_BLESS=1` saves both after an intended change to the sound, and its diff shows the
/// sound's move. `VT_DUMP=folder` writes the samples there, to compare two builds by how far apart
/// they are.
///
/// These hold on one platform: the filter and reverb use library math that can differ elsewhere.
pub fn golden(name: &str, samples: &[f32]) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.hash"));
    let found = hash(samples);
    let sound = Sound::of(samples);
    if let Some(dump) = std::env::var_os("VT_DUMP") {
        let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
        std::fs::write(format!("{}/{name}.raw", dump.to_string_lossy()), bytes).unwrap();
    }
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::write(&path, format!("{found}\n{sound}")).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    let mut lines = saved.lines();
    let saved_hash = lines.next().unwrap_or_default().trim();
    let Some(was) = Sound::parse(lines) else {
        panic!("{name} has no saved sound beside its hash: run with VT_BLESS=1");
    };
    let rows = was.against(&sound);
    let out = rows.iter().filter(|row| !row.within).count();
    if saved_hash == found && out == 0 {
        return;
    }
    let mut message = format!("{name} changed: its hash is {found}, was {saved_hash}.\n");
    message += &format!("{:8}{:>11}{:>11}{:>11}\n", "", "was", "now", "band ±");
    for row in &rows {
        let flag = if row.within { "" } else { "  out" };
        message += &format!(
            "  {:<6}{:>11}{:>11}{:>11}{flag}\n",
            row.name, row.was, row.now, row.band
        );
    }
    message += if out == 0 {
        "Each is in its band, as after a small fix to the DSP. If the change was meant, run with \
         VT_BLESS=1."
    } else {
        "Out of its band, the sound has moved more than a small fix moves it. If that was meant, \
         run with VT_BLESS=1 and say in the commit why it moved."
    };
    panic!("{message}");
}

/// What a golden render sounds like, saved beside its hash. Every golden is interleaved stereo at
/// 48 kHz. Each measure means something for any render, so every golden keeps them all: a loop
/// that sounds to its end ends at 100%.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sound {
    /// The loudest sample, in dB of full scale.
    pub peak: f64,
    /// How loud on average, in dB of full scale.
    pub rms: f64,
    /// The mean, an offset from zero, at full scale 1.
    pub dc: f64,
    /// Where the sound falls for good to 60 dB under its peak, in percent of the render.
    pub ends: f64,
    /// How bright, in Hz: the sine that moves as much from one frame to the next, for its level.
    /// A filter's or a tone stage's move shows here when the level hardly moves.
    pub bright: f64,
}

/// How far each may move from the saved sound before it's out of its band, set from past changes
/// to the DSP and from bugs put in on purpose (#841). Fixes meant as small, such as flushing
/// filter states to zero, anti-aliasing the distortions and ramping the track filter, moved RMS
/// and a peak by 0.7 dB and the brightness by 0.09 of an octave at most. A voice's saw at half
/// its level, the plate's decay halved and the echo's Q doubled each took a golden out of a band;
/// so did refits blessed on purpose, the Overdrive's tone stage (0.9 of an octave darker) and the
/// Res taper (2.3 dB quieter). A change that hardly moves the sound stays in, whatever its cause,
/// and the hash still catches it.
const PEAK_BAND_DB: f64 = 2.0;
const RMS_BAND_DB: f64 = 1.5;
const DC_BAND: f64 = 0.01;
const ENDS_BAND_PERCENT: f64 = 5.0;
const BRIGHT_BAND_OCTAVES: f64 = 0.15;

/// One of a sound's measures, before and after, as the failure shows it.
pub struct Row {
    pub name: &'static str,
    was: String,
    now: String,
    band: String,
    pub within: bool,
}

impl Sound {
    pub fn of(samples: &[f32]) -> Sound {
        let (mut most, mut squares, mut sum, mut moves) = (0.0_f64, 0.0, 0.0, 0.0);
        for (at, &s) in samples.iter().enumerate() {
            let s = f64::from(s);
            // `max` would pass over a NaN; this lets it through, so a NaN is out of every band.
            most = if s.abs() > most || s.is_nan() {
                s.abs()
            } else {
                most
            };
            squares += s * s;
            sum += s;
            // From the same side's sample a frame before.
            if let Some(before) = at.checked_sub(2) {
                moves += (s - f64::from(samples[before])).powi(2);
            }
        }
        let count = samples.len().max(1) as f64;
        let quiet = most * 1e-3;
        let last = samples.iter().rposition(|s| f64::from(s.abs()) > quiet);
        // A sine at `f` moves by `2 sin(pi f / rate)` of its level a frame, as an RMS.
        let moved = if squares > 0.0 {
            (moves / squares).sqrt()
        } else {
            0.0
        };
        Sound {
            peak: decibels(most),
            rms: decibels((squares / count).sqrt()),
            dc: sum / count,
            ends: last.map_or(0.0, |at| (at + 1) as f64 / count * 100.0),
            bright: f64::from(RATE) / std::f64::consts::PI * (moved / 2.0).min(1.0).asin(),
        }
    }

    /// The lines after a saved hash, as `Display` writes them.
    pub fn parse<'a>(lines: impl Iterator<Item = &'a str>) -> Option<Sound> {
        let (mut peak, mut rms, mut dc, mut ends, mut bright) = (None, None, None, None, None);
        for line in lines {
            let mut words = line.split_whitespace();
            let (Some(key), Some(value)) = (words.next(), words.next()) else {
                continue;
            };
            let value = value.trim_end_matches('%').parse::<f64>().ok()?;
            match key {
                "peak" => peak = Some(value),
                "rms" => rms = Some(value),
                "dc" => dc = Some(value),
                "ends" => ends = Some(value),
                "bright" => bright = Some(value),
                _ => return None,
            }
        }
        Some(Sound {
            peak: peak?,
            rms: rms?,
            dc: dc?,
            ends: ends?,
            bright: bright?,
        })
    }

    /// Each measure of this saved sound beside the same of a new one, and whether it's in its band.
    pub fn against(&self, now: &Sound) -> [Row; 5] {
        let decibel_row = |name, was: f64, now: f64, band| Row {
            name,
            was: format!("{was:.2} dB"),
            now: format!("{now:.2} dB"),
            band: format!("{band} dB"),
            // Silence stays silence; a NaN is in no band.
            within: was == now || (now - was).abs() <= band,
        };
        [
            decibel_row("peak", self.peak, now.peak, PEAK_BAND_DB),
            decibel_row("rms", self.rms, now.rms, RMS_BAND_DB),
            Row {
                name: "dc",
                was: format!("{:+.5}", self.dc),
                now: format!("{:+.5}", now.dc),
                band: format!("{DC_BAND}"),
                within: (now.dc - self.dc).abs() <= DC_BAND,
            },
            Row {
                name: "ends",
                was: format!("{:.1}%", self.ends),
                now: format!("{:.1}%", now.ends),
                band: format!("{ENDS_BAND_PERCENT}%"),
                within: (now.ends - self.ends).abs() <= ENDS_BAND_PERCENT,
            },
            Row {
                name: "bright",
                was: format!("{:.0} Hz", self.bright),
                now: format!("{:.0} Hz", now.bright),
                band: format!("{BRIGHT_BAND_OCTAVES} oct"),
                within: self.bright == now.bright
                    || (now.bright / self.bright).log2().abs() <= BRIGHT_BAND_OCTAVES,
            },
        ]
    }
}

impl std::fmt::Display for Sound {
    /// Rounded so a bless's diff shows only moves worth seeing.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "peak {:.2} dB", self.peak)?;
        writeln!(f, "rms {:.2} dB", self.rms)?;
        writeln!(f, "dc {:+.5}", self.dc)?;
        writeln!(f, "ends {:.1}%", self.ends)?;
        writeln!(f, "bright {:.0} Hz", self.bright)
    }
}

/// A level in dB of full scale: silence is minus infinity.
fn decibels(level: f64) -> f64 {
    20.0 * level.log10()
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
