//! Kits: a named list of sounds to fill instrument slots with. A kit refers to its sounds where
//! they are, so making one copies nothing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

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
}

/// Read a kit file, returning where its sounds are, in slot order. Paths can be absolute, start
/// with `~/`, or be relative to the kit file.
pub fn load_kit(path: &Path) -> Result<Vec<PathBuf>, ProjectError> {
    let text = fs::read_to_string(path).map_err(|source| ProjectError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let kit: KitFile = ron::from_str(&text).map_err(|error| ProjectError::Parse {
        path: path.display().to_string(),
        detail: error.to_string(),
    })?;
    let folder = path.parent().unwrap_or_else(|| Path::new(""));
    Ok(kit
        .sounds
        .iter()
        .map(|sound| resolve(folder, &sound.path))
        .collect())
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
