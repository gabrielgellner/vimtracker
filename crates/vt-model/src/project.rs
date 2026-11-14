//! Project folders: `song.ron` and a `samples/` folder.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Deserialize;
use thiserror::Error;

use crate::analysis::{AnalysisError, analyse};
use crate::bank::ContentHash;
use crate::constants::FORMAT_VERSION;
use crate::sample::{SampleError, is_sample_file};
use crate::samples::SamplePool;
use crate::song::Song;

const SONG_FILE: &str = "song.ron";
const AUTOSAVE_FILE: &str = "song.autosave.ron";
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
    let mut song: Song = ron::from_str(text).map_err(|e| parse(&e))?;
    song.ensure_fixed_tracks();
    // Generators' held shapes aren't saved: work them out once, as an edit would, so a song is
    // heard as held from the moment it loads. Its columns are already what that writes.
    crate::action::hold_generators(&mut song);
    Ok(song)
}

/// Whether a folder is a project's: whether it has a `song.ron`.
pub fn is_project(folder: &Path) -> bool {
    folder.join(SONG_FILE).is_file()
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

/// Write unsaved work beside the song, leaving the song itself alone.
pub fn write_autosave(folder: &Path, song: &Song) -> Result<(), ProjectError> {
    write_song(folder, AUTOSAVE_FILE, song)
}

/// Read the autosave file.
pub fn load_autosave(folder: &Path) -> Result<Song, ProjectError> {
    let path = folder.join(AUTOSAVE_FILE);
    let text = fs::read_to_string(&path).map_err(io_at(&path))?;
    parse_song(&text, &path)
}

/// Remove the autosave file, if there is one.
pub fn remove_autosave(folder: &Path) {
    let _ = fs::remove_file(folder.join(AUTOSAVE_FILE));
}

/// Whether there is an autosave newer than the song, or with no saved song at all.
pub fn newer_autosave(folder: &Path) -> bool {
    let modified = |name: &str| fs::metadata(folder.join(name)).and_then(|m| m.modified());
    match (modified(AUTOSAVE_FILE), modified(SONG_FILE)) {
        (Ok(autosave), Ok(song)) => autosave >= song,
        (Ok(_), Err(_)) => true,
        (Err(_), _) => false,
    }
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

/// Where a bank entry's file is. Paths in an unsaved project are absolute.
pub fn resolve_sample(folder: Option<&Path>, sample: &str) -> PathBuf {
    match folder {
        Some(folder) => folder.join(sample),
        None => PathBuf::from(sample),
    }
}

/// Save a song to a new folder, copying its bank's files into its `samples/` folder, each
/// content once, and return the song with its bank's paths rewritten. A folder that already has a
/// song is refused.
pub fn save_as(from: Option<&Path>, to: &Path, song: &Song) -> Result<Song, ProjectError> {
    if to.join(SONG_FILE).exists() {
        return Err(ProjectError::Exists(to.display().to_string()));
    }
    fs::create_dir_all(to).map_err(io_at(to))?;
    let mut saved = song.clone();
    let mut copied: HashMap<ContentHash, String> = HashMap::new();
    for (_, entry) in saved.bank.iter_mut() {
        for source in entry.sources_mut() {
            if let Some(stored) = copied.get(&source.hash) {
                source.path = stored.clone();
                continue;
            }
            source.path = import_sample(to, &resolve_sample(from, &source.path))?;
            if source.hash.is_known() {
                copied.insert(source.hash, source.path.clone());
            }
        }
    }
    save_song(to, &saved)?;
    Ok(saved)
}

/// Check the bank's files against their hashes: one that's gone is looked for by its content in
/// the project's `samples/` folder, and the bank pointed at it if it's there, and one whose content
/// has changed since it joined the bank is reported. What's said about each is given back.
pub fn find_sources(folder: Option<&Path>, song: &mut Song) -> Vec<String> {
    let mut notes = Vec::new();
    let mut in_project: Option<HashMap<ContentHash, String>> = None;
    for (number, entry) in song.bank.iter_mut() {
        for source in entry.sources_mut() {
            if !source.hash.is_known() {
                continue;
            }
            let path = resolve_sample(folder, &source.path);
            match ContentHash::of_file(&path) {
                Ok(hash) if hash == source.hash => {}
                Ok(_) => notes.push(format!(
                    "Sample {number:02X}: {} has changed since it joined the bank",
                    source.path
                )),
                Err(_) => {
                    let found = in_project.get_or_insert_with(|| hashes_in_project(folder));
                    if let Some(moved) = found.get(&source.hash) {
                        notes.push(format!(
                            "Sample {number:02X}: {} was missing, and is found at {moved}",
                            source.path
                        ));
                        source.path = moved.clone();
                    }
                }
            }
        }
    }
    notes
}

/// The sample files in a project's `samples/` folder, by their content, as the song refers to
/// them.
fn hashes_in_project(folder: Option<&Path>) -> HashMap<ContentHash, String> {
    let Some(folder) = folder else {
        return HashMap::new();
    };
    let Ok(files) = fs::read_dir(folder.join(SAMPLES_DIR)) else {
        return HashMap::new();
    };
    files
        .filter_map(Result::ok)
        .map(|file| file.path())
        .filter(|path| is_sample_file(path))
        .filter_map(|path| {
            let hash = ContentHash::of_file(&path).ok()?;
            let name = path.file_name()?.to_string_lossy().into_owned();
            Some((hash, format!("{SAMPLES_DIR}/{name}")))
        })
        .collect()
}

/// Load each of the bank's files that isn't already in the pool, once however many tracks play
/// it. Files that can't be read are reported, and the rest still load.
pub fn load_samples(
    folder: Option<&Path>,
    song: &Song,
    rate: u32,
    pool: &mut SamplePool,
) -> Vec<SampleError> {
    let mut errors = Vec::new();
    for source in song.bank.iter().flat_map(|(_, entry)| entry.sources()) {
        if pool.id_of(&source.path).is_some() {
            continue;
        }
        if let Err(error) = pool.load(&source.path, &resolve_sample(folder, &source.path), rate) {
            errors.push(error);
        }
    }
    errors
}

/// Read or work out the analysis of each of the bank's files whose content is known and whose
/// analysis isn't in the pool yet, keeping it in a cache folder if there is one. It waits for
/// each, as rendering a song must, so a render never depends on how far work in the background
/// had got. Files that can't be read are reported, and the rest are analysed.
pub fn analyse_samples(
    folder: Option<&Path>,
    song: &Song,
    cache: Option<&Path>,
    pool: &mut SamplePool,
) -> Vec<AnalysisError> {
    let mut errors = Vec::new();
    for source in song.bank.iter().flat_map(|(_, entry)| entry.sources()) {
        if !source.hash.is_known() || pool.analysis(source.hash).is_some() {
            continue;
        }
        match analyse(&resolve_sample(folder, &source.path), source.hash, cache) {
            Ok((analysis, _)) => pool.insert_analysis(Arc::new(analysis)),
            Err(error) => errors.push(error),
        }
    }
    errors
}
