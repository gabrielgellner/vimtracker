//! Kits: a named list of sounds to put on tracks, in order. A kit refers to its sounds where
//! they are, so making one copies nothing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::label::is_name;
use crate::project::ProjectError;

/// What a kit file holds.
#[derive(Debug, Deserialize)]
struct KitFile {
    sounds: Vec<KitSound>,
}

/// One sound, as its own record so a kit can later say more about it.
#[derive(Debug, Deserialize)]
struct KitSound {
    path: String,
    /// The name its track takes, or empty for none.
    #[serde(default)]
    label: String,
}

/// A sound from a kit, and the name its track takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitEntry {
    pub path: PathBuf,
    pub name: String,
}

/// Read a kit file, in file order. Paths can be absolute, start with `~/`, or be relative to the
/// kit file. A label that can't be a name stops it.
pub fn load_kit(path: &Path) -> Result<Vec<KitEntry>, ProjectError> {
    let text = fs::read_to_string(path).map_err(|source| ProjectError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let refuse = |detail: String| ProjectError::Parse {
        path: path.display().to_string(),
        detail,
    };
    let kit: KitFile = ron::from_str(&text).map_err(|error| refuse(error.to_string()))?;
    let folder = path.parent().unwrap_or_else(|| Path::new(""));
    let mut entries: Vec<KitEntry> = Vec::with_capacity(kit.sounds.len());
    for sound in &kit.sounds {
        let label = sound.label.as_str();
        if !label.is_empty() && !is_name(label) {
            return Err(refuse(format!(
                "{label:?} can't be a name: it needs two characters, the first a letter"
            )));
        }
        entries.push(KitEntry {
            path: resolve(folder, &sound.path),
            name: label.to_ascii_uppercase(),
        });
    }
    Ok(entries)
}

fn resolve(folder: &Path, sound: &str) -> PathBuf {
    if let Some(rest) = sound.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    folder.join(sound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_paths_are_relative_to_the_kit_unless_absolute() {
        assert_eq!(
            resolve(Path::new("/kits"), "../drums/kick.wav"),
            Path::new("/kits/../drums/kick.wav")
        );
        assert_eq!(
            resolve(Path::new("/kits"), "/drums/kick.wav"),
            Path::new("/drums/kick.wav")
        );
    }
}
