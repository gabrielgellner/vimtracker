//! Tags in the explorer (decision 315): what `T` gives a sample, a kit's sound or a preset, kept
//! in the tag library by the file's content or in the preset's own file, and the guess from how a
//! sample's sound ends once it's been read for its preview.

use std::path::Path;
use std::sync::Arc;

use vt_model::preset::{self, PresetKind};
use vt_model::tag::{self, Source, Tag};

use crate::app::App;
use crate::explorer::{self, Kind};
use crate::input::picker::{KitChoice, Pick};

/// Tags as the status line lists them, each after a `#`.
pub fn listed(tags: &[Tag]) -> String {
    let names: Vec<String> = tags.iter().map(|tag| format!("#{}", tag.name)).collect();
    names.join(" ")
}

impl App {
    /// Give an item in the explorer the tags typed after `T`: a sample or a kit's sound by its
    /// file, wherever it's listed, and a preset in its file. The status line says what it has now.
    pub(crate) fn tag_item(&mut self, item: usize, typed: Vec<String>) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let chosen = picker.item(item);
        let name = chosen.label.clone();
        let done = match chosen.pick.clone() {
            Pick::Sample(path) => self.tag_file(&path, "", &typed),
            Pick::KitSound(KitChoice { path, label, .. }) => {
                let name = path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                );
                self.tag_file(&path, &label, &typed).map(|_| name)
            }
            Pick::Preset { kind, path } => self.tag_preset(item, kind, &path, &typed),
            _ => return,
        };
        self.status = match done {
            Ok(name) => {
                let tags = self
                    .picker
                    .as_ref()
                    .map(|picker| picker.item(item).tags.clone())
                    .unwrap_or_default();
                if tags.is_empty() {
                    format!("{name} has no tags")
                } else {
                    format!("{name}: {}", listed(&tags))
                }
            }
            Err(error) => format!("Couldn't tag {name}: {error}"),
        };
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
    fn tag_file(&mut self, path: &Path, label: &str, typed: &[String]) -> Result<String, String> {
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
        self.retag_file(path);
        Ok(path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        ))
    }

    /// Give a preset its own tags, those typed that it doesn't always have, in its file.
    fn tag_preset(
        &mut self,
        item: usize,
        kind: PresetKind,
        path: &Path,
        typed: &[String],
    ) -> Result<String, String> {
        let picker = self.picker.as_ref().ok_or_else(String::new)?;
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
        preset::set_tags(path, kind, own).map_err(|error| error.to_string())?;
        let (read, _) = explorer::item(path, Kind::Preset(kind), &self.tag_library);
        if let Some(picker) = self.picker.as_mut() {
            picker.set_tags(item, read.tags);
        }
        Ok(preset::preset_name(path))
    }

    /// Work a file's tags out again wherever the explorer lists it, after its tags are given or
    /// its sound is heard.
    fn retag_file(&mut self, path: &Path) {
        let library = Arc::clone(&self.tag_library);
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
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
        if let Some(picker) = self.picker.as_mut() {
            picker.set_library(library);
            for (item, tags) in changed {
                picker.set_tags(item, tags);
            }
        }
    }

    /// A sample read for the explorer: what its sound says it is, a loop or a one-shot, joins its
    /// tags where its folders and name said neither.
    pub(crate) fn note_heard(&mut self, path: &Path, heard: Option<&'static str>) {
        let Some(heard) = heard else {
            return;
        };
        if self.heard_tags.get(path) != Some(&heard) {
            self.heard_tags.insert(path.to_path_buf(), heard);
        }
        self.retag_file(path);
    }
}
