//! Tags in the explorer: what `T` gives a sample, a kit's sound or a preset, kept in the tag
//! library by the file's content or in the preset's own file, and the guess from how a sample's
//! sound ends once it's been read for its preview.

use std::path::Path;
use std::sync::Arc;

use vt_model::preset::{self, PresetKind};
use vt_model::tag::{self, Source, Tag};

use crate::overlays::picker::explorer::{self, Kind};
use crate::overlays::picker::exploring::Explorer;
use crate::overlays::picker::{KitChoice, Pick, Picker};

/// Tags as the status line lists them, each after a `#`.
pub(crate) fn listed(tags: &[Tag]) -> String {
    let names: Vec<String> = tags.iter().map(|tag| format!("#{}", tag.name)).collect();
    names.join(" ")
}

impl Explorer {
    /// Give an item in the explorer the tags typed after `T`: a sample or a kit's sound by its
    /// file, wherever it's listed, and a preset in its file. Returns what the status line says it
    /// has now, or nothing for an item that takes no tags.
    pub(crate) fn tag_item(
        &mut self,
        picker: &mut Picker,
        item: usize,
        typed: &[String],
    ) -> Option<String> {
        let chosen = picker.item(item);
        let name = chosen.label.clone();
        let done = match chosen.pick.clone() {
            Pick::Sample(path) => self.tag_file(picker, &path, "", typed),
            Pick::KitSound(KitChoice { path, label, .. }) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                self.tag_file(picker, &path, &label, typed).map(|_| name)
            }
            Pick::Preset { kind, path } => self.tag_preset(picker, item, kind, &path, typed),
            _ => return None,
        };
        Some(match done {
            Ok(name) => {
                let tags = picker.item(item).tags.clone();
                if tags.is_empty() {
                    format!("{name} has no tags")
                } else {
                    format!("{name}: {}", listed(&tags))
                }
            }
            Err(error) => format!("Couldn't tag {name}: {error}"),
        })
    }

    /// What's guessed for a sample file: from where it is, its name and a kit's label, then how
    /// its sound ends, if it's been read and nothing else said.
    fn file_guesses(&self, path: &Path, label: &str) -> Vec<&'static str> {
        let mut guessed = tag::guess(path, label);
        let said = guessed.contains(&tag::LOOP) || guessed.contains(&tag::ONE_SHOT);
        if let Some(heard) = self.heard_tags.get(path)
            && !said
        {
            guessed.push(heard);
        }
        guessed
    }

    /// Keep a file's tags as typed in the library: those not guessed as given by hand, and the
    /// guesses left out as taken off. Returns its name.
    fn tag_file(
        &mut self,
        picker: &mut Picker,
        path: &Path,
        label: &str,
        typed: &[String],
    ) -> Result<String, String> {
        let guessed = self.file_guesses(path, label);
        let given = typed
            .iter()
            .filter(|name| !guessed.contains(&name.as_str()))
            .cloned()
            .collect();
        let taken_off = guessed
            .iter()
            .filter(|guess| !typed.iter().any(|name| name == *guess))
            .map(|guess| (*guess).to_owned())
            .collect();
        Arc::make_mut(&mut self.tag_library)
            .set(path, given, taken_off)
            .map_err(|error| error.to_string())?;
        self.retag_file(picker, path);
        Ok(path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }

    /// Give a preset its own tags, those typed that it doesn't always have, in its file.
    fn tag_preset(
        &self,
        picker: &mut Picker,
        item: usize,
        kind: PresetKind,
        path: &Path,
        typed: &[String],
    ) -> Result<String, String> {
        let fixed = picker
            .item(item)
            .tags
            .iter()
            .filter(|tag| tag.source == Source::Fixed);
        let fixed: Vec<String> = fixed.map(|tag| tag.name.clone()).collect();
        let own = typed
            .iter()
            .filter(|name| !fixed.contains(name))
            .cloned()
            .collect();
        kind.set_tags(path, own)
            .map_err(|error| error.to_string())?;
        let (read, _) = explorer::item(path, Kind::Preset(kind), &self.tag_library);
        picker.set_tags(item, read.tags);
        Ok(preset::preset_name(path))
    }

    /// Work a file's tags out again wherever the explorer lists it, after its tags are given or
    /// its sound is heard.
    fn retag_file(&self, picker: &mut Picker, path: &Path) {
        let library = Arc::clone(&self.tag_library);
        let changed: Vec<(usize, Vec<Tag>)> = picker
            .items_of_file(path)
            .into_iter()
            .filter_map(|item| {
                let label = match &picker.item(item).pick {
                    Pick::KitSound(sound) => sound.label.as_str(),
                    _ => "",
                };
                let tags = tag::file_tags(&self.file_guesses(path, label), library.of_file(path));
                (tags != picker.item(item).tags).then_some((item, tags))
            })
            .collect();
        picker.set_library(library);
        for (item, tags) in changed {
            picker.set_tags(item, tags);
        }
    }

    /// A sample read for the explorer: what its sound says it is, a loop or a one-shot, joins its
    /// tags where its folders and name said neither.
    pub(crate) fn note_heard(
        &mut self,
        picker: Option<&mut Picker>,
        path: &Path,
        heard: Option<&'static str>,
    ) {
        let Some(heard) = heard else {
            return;
        };
        if self.heard_tags.get(path) != Some(&heard) {
            self.heard_tags.insert(path.to_path_buf(), heard);
        }
        if let Some(picker) = picker {
            self.retag_file(picker, path);
        }
    }
}
