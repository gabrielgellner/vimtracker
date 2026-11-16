//! The project's bank of samples: adding files to it from the explorer, `:sample` and kits,
//! listing it for a track's `Sample` and a step's `SMP`, and removing what nothing plays.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use vt_model::action::Action;
use vt_model::bank::{BankEntry, ContentHash, PlayParams};
use vt_model::label::GROUP_NAME_CHARS;
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::services::Services;

/// What a kit says about one of its sounds, for the entry it becomes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Seed {
    pub label: String,
    pub name: String,
    pub play: PlayParams,
}

/// What's said when the bank has no room.
pub(crate) const FULL: &str =
    "The bank is full, at 255 samples: :bank clean removes those nothing plays";

/// A bank entry's number and name as a list or a row shows it: `03 BD Bass Drum`, or `Off`.
pub(crate) fn sample_text(song: &Song, number: u8) -> String {
    match (number, song.bank.get(number)) {
        (0, _) => "Off".to_owned(),
        (number, Some(entry)) => format!("{number:02X} {}", entry.title()),
        (number, None) => format!("{number:02X} ---"),
    }
}

/// What the status line says of a `Sample` value: its entry and the file it plays.
pub fn sample_shown(song: &Song, number: u8) -> String {
    match (number, song.bank.get(number)) {
        (0, _) => "Off: no sample".to_owned(),
        (number, Some(entry)) => {
            let file = entry
                .source()
                .map_or("", |source| source.path.rsplit('/').next().unwrap_or(""));
            format!("{number:02X} {}: {file}", entry.title())
        }
        (number, None) => format!("{number:02X}: nothing in the bank there, so it's silent"),
    }
}

/// Words that say a kit is a kit rather than which one, so they don't name its group.
const GENERIC_WORDS: [&str; 12] = [
    "kit", "kits", "drum", "drums", "core", "pack", "packs", "rack", "racks", "set", "the", "vol",
];

/// What a group whose kit's name says nothing better is called.
pub(crate) const KIT_GROUP_NAME: &str = "DRUMS";

/// A name's words, split at spaces and punctuation, leaving out those that only say it's a kit.
fn telling_words(name: &str) -> impl Iterator<Item = &str> {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .filter(|word| !GENERIC_WORDS.contains(&word.to_ascii_lowercase().as_str()))
}

/// A group's name from its kit's: the first word of its name, in capitals, up to the five
/// characters a group's name holds, so `909 Core Kit` names it `909` and `Rytm Analog` `RYTM`.
/// A first word that's generic, as `Kit-808`'s `Kit` is, gives way to the kit's category, by its
/// first word that isn't and fits whole, or else `DRUMS`, as `Drum Machines` gives.
pub fn group_name(kit: &str, category: &str) -> String {
    let first = kit
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| !word.is_empty());
    let name = match first {
        Some(word) if telling_words(word).next().is_some() => word,
        _ => telling_words(category)
            .find(|word| word.len() <= GROUP_NAME_CHARS)
            .unwrap_or(KIT_GROUP_NAME),
    };
    name.chars()
        .take(GROUP_NAME_CHARS)
        .collect::<String>()
        .to_ascii_uppercase()
}

impl Services {
    /// How a song refers to a sample: where it is, or relative to the project if it's inside,
    /// so a bundled project can be moved. `:w` to a new folder is what copies samples.
    pub(crate) fn store_sample(&self, source: &Path) -> Result<String, ProjectError> {
        let full = source.canonicalize().map_err(|error| ProjectError::Io {
            path: source.display().to_string(),
            source: error,
        })?;
        let project = self.folder.as_deref().and_then(|f| f.canonicalize().ok());
        let inside = project.as_deref().and_then(|f| full.strip_prefix(f).ok());
        Ok(inside.unwrap_or(&full).display().to_string())
    }

    /// Find a sound preset's bank entry in the bank, or load its file to add it: the number it's at,
    /// and the edit that adds it if it's new. Its file is kept as a sample added from where it is
    /// would be, inside the project by its place there, and an entry the same in every way is the
    /// same entry, so loading a preset twice adds it once.
    pub(crate) fn bank_preset_sample(
        &mut self,
        entry: &BankEntry,
    ) -> Result<(u8, Option<Action>), String> {
        let mut entry = entry.clone();
        for source in entry.sources_mut() {
            let file = Path::new(&source.path);
            let bytes = fs::read(file).map_err(|error| format!("{}: {error}", file.display()))?;
            source.hash = ContentHash::of(&bytes);
            source.path = self.store_sample(file).map_err(|error| error.to_string())?;
        }
        if let Some((number, _)) = self.song().bank.iter().find(|(_, own)| **own == entry) {
            return Ok((number, None));
        }
        let number = self.song().bank.free().ok_or_else(|| FULL.to_owned())?;
        let mut pool = SamplePool::clone(&self.samples);
        for source in entry.sources() {
            let resolved = project::resolve_sample(self.folder.as_deref(), &source.path);
            pool.load(&source.path, &resolved, self.rate)
                .map_err(|error| error.to_string())?;
        }
        self.samples = Arc::new(pool);
        let add = Action::SetBankEntry {
            number,
            entry: Some(entry),
        };
        Ok((number, Some(add)))
    }
}
