//! Kits: a named list of sounds to put on tracks, in order. A kit refers to its sounds where
//! they are, so making one copies nothing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::constants::{DB_PER_LEVEL, MAX_LEVEL, UNITY_LEVEL};
use crate::label::is_name;
use crate::project::ProjectError;

/// The choke groups a sound can be in, numbered from 1 as Ableton's are.
pub const CHOKE_GROUPS: u8 = 16;
/// How far a sound can be tuned, in semitones either way, as far as Ableton's Simpler goes.
pub const MAX_TUNE: i8 = 48;

/// What a kit file holds, as it's written. Everything but the sounds can be left out.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KitFile {
    /// What it's called, or empty to go by the file's name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Where it came from, such as "Ableton Core Library".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// What kind of kit it is, such as "Drum Machines", for the kit picker to group by.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    pub sounds: Vec<SoundFile>,
}

/// One sound in a kit file. Paths can be absolute, start with `~/`, or be relative to the kit.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoundFile {
    /// The name its track takes, such as `BD`, or empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// What the sound is, such as "Bass Drum 909", or empty to go by its file's name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub path: String,
    /// Other tones of the same sound, to choose between later.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    /// Velocity layers, soft to hard.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
    /// The group of sounds that cut each other off, as an open hat and a closed one do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choke: Option<u8>,
    /// In decibels.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gain: f32,
    /// In semitones.
    #[serde(default, skip_serializing_if = "is_zero_tune")]
    pub tune: i8,
}

fn is_zero(gain: &f32) -> bool {
    *gain == 0.0
}

fn is_zero_tune(tune: &i8) -> bool {
    *tune == 0
}

/// A kit as it's used, with its sounds' paths found.
#[derive(Debug, Clone, PartialEq)]
pub struct Kit {
    pub name: String,
    pub source: String,
    pub category: String,
    pub sounds: Vec<KitEntry>,
}

/// A sound from a kit.
#[derive(Debug, Clone, PartialEq)]
pub struct KitEntry {
    pub path: PathBuf,
    /// The name its track takes, in capitals, or empty for none.
    pub label: String,
    /// What the sound is, or its file's name if the kit doesn't say.
    pub name: String,
    pub variants: Vec<PathBuf>,
    pub layers: Vec<PathBuf>,
    pub choke: Option<u8>,
    pub gain: f32,
    pub tune: i8,
}

impl KitEntry {
    /// The level its track takes: unity, moved by its gain, a quarter of a dB a step.
    pub fn level(&self) -> u8 {
        let steps = (self.gain / DB_PER_LEVEL).round();
        (f32::from(UNITY_LEVEL) + steps).clamp(0.0, f32::from(MAX_LEVEL)) as u8
    }

    /// The pitch its track takes: `80`, moved by its tune, a semitone a step.
    pub fn pitch(&self) -> u8 {
        (0x80 + i16::from(self.tune)).clamp(0, 0xFF) as u8
    }
}

/// Read a kit file, its sounds in file order. A sound whose label can't be a name, or whose
/// choke group, gain or tune is out of range, stops it.
pub fn load_kit(path: &Path) -> Result<Kit, ProjectError> {
    let text = fs::read_to_string(path).map_err(|source| ProjectError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let refuse = |detail: String| ProjectError::Parse {
        path: path.display().to_string(),
        detail,
    };
    let file: KitFile = ron::from_str(&text).map_err(|error| refuse(error.to_string()))?;
    let folder = path.parent().unwrap_or_else(|| Path::new(""));
    let mut sounds: Vec<KitEntry> = Vec::with_capacity(file.sounds.len());
    for sound in &file.sounds {
        check(sound).map_err(refuse)?;
        let found = resolve(folder, &sound.path);
        let name = if sound.name.is_empty() {
            file_stem(&found)
        } else {
            sound.name.clone()
        };
        let all = |paths: &[String]| paths.iter().map(|p| resolve(folder, p)).collect();
        sounds.push(KitEntry {
            label: sound.label.to_ascii_uppercase(),
            name,
            variants: all(&sound.variants),
            layers: all(&sound.layers),
            choke: sound.choke,
            gain: sound.gain,
            tune: sound.tune,
            path: found,
        });
    }
    let name = if file.name.is_empty() {
        file_stem(path)
    } else {
        file.name
    };
    Ok(Kit {
        name,
        source: file.source,
        category: file.category,
        sounds,
    })
}

/// Why a sound can't be used, if it can't.
fn check(sound: &SoundFile) -> Result<(), String> {
    let label = sound.label.as_str();
    if !label.is_empty() && !is_name(label) {
        return Err(format!(
            "{label:?} can't be a name: it needs two characters, the first a letter"
        ));
    }
    let who = || {
        if label.is_empty() {
            sound.path.clone()
        } else {
            label.to_owned()
        }
    };
    if let Some(group) = sound.choke
        && !(1..=CHOKE_GROUPS).contains(&group)
    {
        return Err(format!(
            "{}: choke group {group} isn't one of 1 to {CHOKE_GROUPS}",
            who()
        ));
    }
    if !sound.gain.is_finite() {
        return Err(format!(
            "{}: gain {} isn't a number of dB",
            who(),
            sound.gain
        ));
    }
    if sound.tune.unsigned_abs() > MAX_TUNE.unsigned_abs() {
        return Err(format!(
            "{}: tune {} is more than {MAX_TUNE} semitones",
            who(),
            sound.tune
        ));
    }
    Ok(())
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
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
