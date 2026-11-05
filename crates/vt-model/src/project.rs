//! Project folders: `song.ron` and a `samples/` folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

use crate::constants::FORMAT_VERSION;
use crate::sample::{SampleError, load_wav};
use crate::samples::SamplePool;
use crate::song::{Instrument, Song};

const SONG_FILE: &str = "song.ron";
const SAMPLES_DIR: &str = "samples";

/// Why a project could not be read or written.
#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("{path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("{path}: {detail}")]
    Parse { path: String, detail: String },
    #[error(
        "{path} uses format {found}, and this version reads format {expected}. \
         Open it with the matching version to render it to WAV."
    )]
    Version {
        path: String,
        found: u32,
        expected: u32,
    },
    #[error("{0} already has a song in it")]
    Exists(String),
    #[error("could not write the song: {0}")]
    Serialize(String),
}

/// Attach a path to an I/O error.
fn io_at(path: &Path) -> impl Fn(io::Error) -> ProjectError + '_ {
    move |source| ProjectError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Only the version, so a changed format gives a clear message rather than a field error.
#[derive(Deserialize)]
struct VersionProbe {
    format_version: u32,
}

/// Parse a song's text, checking its version first.
fn parse_song(text: &str, path: &Path) -> Result<Song, ProjectError> {
    let parse = |error: &dyn std::fmt::Display| ProjectError::Parse {
        path: path.display().to_string(),
        detail: error.to_string(),
    };
    let probe: VersionProbe = ron::from_str(text).map_err(|e| parse(&e))?;
    if probe.format_version != FORMAT_VERSION {
        return Err(ProjectError::Version {
            path: path.display().to_string(),
            found: probe.format_version,
            expected: FORMAT_VERSION,
        });
    }
    ron::from_str(text).map_err(|e| parse(&e))
}

/// Read a project's song.
pub fn load_song(folder: &Path) -> Result<Song, ProjectError> {
    let path = folder.join(SONG_FILE);
    let text = fs::read_to_string(&path).map_err(io_at(&path))?;
    parse_song(&text, &path)
}

/// Write a project's song through a temporary file and rename, so a process crash or failed
/// write leaves the previous file intact.
pub fn save_song(folder: &Path, song: &Song) -> Result<(), ProjectError> {
    fs::create_dir_all(folder).map_err(io_at(folder))?;
    write_song(folder, SONG_FILE, song)
}

fn write_song(folder: &Path, name: &str, song: &Song) -> Result<(), ProjectError> {
    let text = ron::ser::to_string_pretty(song, ron::ser::PrettyConfig::default())
        .map_err(|e| ProjectError::Serialize(e.to_string()))?;
    let target = folder.join(name);
    let temporary = folder.join(format!(".{name}.tmp"));
    fs::write(&temporary, text).map_err(io_at(&temporary))?;
    fs::rename(&temporary, &target).map_err(io_at(&target))
}

/// Create a project folder for a new song. A folder that already has a song is refused.
pub fn create_project(folder: &Path, song: &Song) -> Result<(), ProjectError> {
    if folder.join(SONG_FILE).exists() {
        return Err(ProjectError::Exists(folder.display().to_string()));
    }
    save_song(folder, song)
}

/// Copy a sample into a project's `samples/` folder, returning its path relative to the
/// project. A different file with the same name gets a new name.
pub fn import_sample(folder: &Path, source: &Path) -> Result<String, ProjectError> {
    let samples = folder.join(SAMPLES_DIR);
    fs::create_dir_all(&samples).map_err(io_at(&samples))?;
    let bytes = fs::read(source).map_err(io_at(source))?;
    let stem = source
        .file_stem()
        .map_or_else(|| "sample".to_owned(), |s| s.to_string_lossy().into_owned());
    let extension = source
        .extension()
        .map_or_else(|| "wav".to_owned(), |e| e.to_string_lossy().into_owned());
    for attempt in 1u32.. {
        let name = if attempt == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{attempt}.{extension}")
        };
        let target = samples.join(&name);
        match fs::read(&target) {
            Ok(existing) if existing == bytes => return Ok(format!("{SAMPLES_DIR}/{name}")),
            Ok(_) => continue,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::write(&target, &bytes).map_err(io_at(&target))?;
                return Ok(format!("{SAMPLES_DIR}/{name}"));
            }
            Err(error) => return Err(io_at(&target)(error)),
        }
    }
    unreachable!("the import loop always returns")
}

/// Where an instrument's sample path points. Paths in an unsaved project are absolute.
pub fn resolve_sample(folder: Option<&Path>, sample: &str) -> PathBuf {
    match folder {
        Some(folder) => folder.join(sample),
        None => PathBuf::from(sample),
    }
}

/// Save a song to a new folder, copying its samples there, and return the song with its sample
/// paths rewritten. A folder that already has a song is refused.
pub fn save_as(from: Option<&Path>, to: &Path, song: &Song) -> Result<Song, ProjectError> {
    if to.join(SONG_FILE).exists() {
        return Err(ProjectError::Exists(to.display().to_string()));
    }
    fs::create_dir_all(to).map_err(io_at(to))?;
    let mut saved = song.clone();
    for instrument in saved.instruments.values_mut() {
        if let Some(sample) = &instrument.sample {
            let source = resolve_sample(from, sample);
            let path = import_sample(to, &source)?;
            *instrument = Arc::new(Instrument {
                sample: Some(path),
                ..(**instrument).clone()
            });
        }
    }
    save_song(to, &saved)?;
    Ok(saved)
}

/// Load every instrument's sample that isn't already in the pool. Samples that can't be read are
/// reported, and the rest still load.
pub fn load_samples(
    folder: Option<&Path>,
    song: &Song,
    rate: u32,
    pool: &mut SamplePool,
) -> Vec<SampleError> {
    let mut errors = Vec::new();
    for instrument in song.instruments.values() {
        let Some(sample) = &instrument.sample else {
            continue;
        };
        if pool.id_of(sample).is_some() {
            continue;
        }
        match load_wav(&resolve_sample(folder, sample), rate) {
            Ok(data) => {
                pool.insert(sample, data);
            }
            Err(error) => errors.push(error),
        }
    }
    errors
}
