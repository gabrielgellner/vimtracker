//! Presets on the track page (decision 310): saving the cursor's track's sound, or on the FX page
//! the effect in the slot under the cursor, into the presets folder, and loading one from the
//! browser or by name, as one undo step that leaves the track's place in the song and its phrases
//! alone.

use std::path::{Path, PathBuf};

use vt_model::action::Action;
use vt_model::preset::{
    EffectPreset, PresetError, PresetKind, SoundPreset, load_effect, load_sound, preset_name,
    preset_path, save,
};

use crate::app::App;
use crate::config::home_folder;
use crate::explorer::{Purpose, place_name};
use crate::input::command::PresetLine;
use crate::input::mode::View;
use crate::track_page::{Control, Page};

/// What a preset command acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// A track's sound and patch.
    Sound(usize),
    /// The effect in one of a track's slots.
    Effect { track: usize, slot: usize },
}

impl Target {
    fn kind(self) -> PresetKind {
        match self {
            Self::Sound(_) => PresetKind::Sound,
            Self::Effect { .. } => PresetKind::Effect,
        }
    }

    /// What it is in a sentence, as `track 01's sound` or `track 01's FX 2`.
    fn said(self) -> String {
        match self {
            Self::Sound(track) => format!("track {:02}'s sound", track + 1),
            Self::Effect { track, slot } => format!("track {:02}'s FX {}", track + 1, slot + 1),
        }
    }
}

/// What's said where a sound preset is wanted on a track that doesn't play sounds.
const NOT_SOUND: &str =
    "Only a track that plays sounds has a sound preset: on the FX page, :preset is the slot's";

/// What's said with no presets folder.
const NO_FOLDER: &str =
    "There's no presets folder: set presets in settings.ron, in the config folder";

impl App {
    /// What `:preset` acts on here: on the FX page with the cursor on a slot, its effect, and
    /// anywhere else the cursor's track's sound.
    fn preset_target(&self) -> Target {
        let track = self.cursor.track;
        if self.view == View::Track
            && self.track_page.page == Page::Fx
            && let Control::Insert(slot) | Control::InsertParam(slot, _) = self.track_control()
        {
            return Target::Effect { track, slot };
        }
        Target::Sound(track)
    }

    /// The folder of the presets `:preset` lists here, `sounds` or `effects`, for completion.
    pub(crate) fn presets_here(&self) -> Option<PathBuf> {
        let kind = self.preset_target().kind();
        Some(self.settings.preset_folder()?.join(kind.folder()))
    }

    /// Whether the cursor's track plays sounds, saying why not if it doesn't, for a sound preset.
    fn takes_sound(&mut self, target: Target) -> bool {
        let Target::Sound(track) = target else {
            return true;
        };
        let sound = self
            .song()
            .tracks
            .get(track)
            .is_some_and(vt_model::song::Track::is_sound);
        if !sound {
            self.status = NOT_SOUND.to_owned();
        }
        sound
    }

    /// The presets folder, or none, saying so.
    pub(crate) fn presets_folder(&mut self) -> Option<PathBuf> {
        let folder = self.settings.preset_folder();
        if folder.is_none() {
            self.status = NO_FOLDER.to_owned();
        }
        folder
    }

    /// `:preset`, `:preset save` and `:preset load`.
    pub(crate) fn preset_line(&mut self, line: PresetLine) {
        match line {
            PresetLine::Browse => self.open_preset_explorer(),
            PresetLine::Save { name, force } => self.save_preset(&name, force),
            PresetLine::Load(name) => self.load_preset_named(&name),
        }
    }

    /// List the presets for what's under the cursor, in the presets folder's `sounds` or
    /// `effects`, to load one, as Ableton's hot-swap opens its browser on a device's presets.
    pub(crate) fn open_preset_explorer(&mut self) {
        let target = self.preset_target();
        if !self.takes_sound(target) {
            return;
        }
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let kind = target.kind();
        let folder = folder.join(kind.folder());
        if !folder.is_dir() {
            let home = home_folder();
            self.status = format!(
                "No {} presets yet: :preset save name saves {} in {}",
                kind.noun(),
                target.said(),
                place_name(&folder, home.as_deref())
            );
            return;
        }
        let purpose = Purpose::Preset(kind);
        let roots = self.places(purpose);
        self.open_explorer(purpose, roots, None);
    }

    /// Save what's under the cursor as a preset of this name, in its kind's folder, which is made
    /// if it isn't there. A name that's taken is refused unless forced.
    pub(crate) fn save_preset(&mut self, name: &str, force: bool) {
        let target = self.preset_target();
        if !self.takes_sound(target) {
            return;
        }
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let path = match preset_path(&folder, target.kind(), name) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let written = match target {
            Target::Sound(track) => {
                let sample = self.preset_sample(track);
                let preset = SoundPreset::of(&self.song().tracks[track], sample);
                save(&path, &preset, force)
            }
            Target::Effect { track, slot } => {
                let Some(preset) = EffectPreset::of(&self.song().tracks[track], slot) else {
                    self.status = format!("FX {} is empty, so there's nothing to save", slot + 1);
                    return;
                };
                save(&path, &preset, force)
            }
        };
        match written {
            Ok(()) => {
                if let Some(saved_in) = path.parent() {
                    self.note_place(Purpose::Preset(target.kind()), saved_in);
                }
                let home = home_folder();
                self.status = format!(
                    "Saved {} as {name}, in {}",
                    target.said(),
                    place_name(&folder.join(target.kind().folder()), home.as_deref())
                );
            }
            Err(PresetError::Taken(_)) => {
                self.status = format!(
                    "There's a {} preset called {name} already: :preset! save {name} writes over it",
                    target.kind().noun()
                );
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// The bank entry a track's sampler plays, with its files by their full paths, so a preset
    /// finds them from any song.
    fn preset_sample(&self, track: usize) -> Option<vt_model::bank::BankEntry> {
        let number = self.song().tracks.get(track)?.sample();
        let mut entry = self.song().bank.get(number)?.clone();
        for source in entry.sources_mut() {
            let resolved = vt_model::project::resolve_sample(self.folder.as_deref(), &source.path);
            let full = resolved.canonicalize().unwrap_or(resolved);
            source.path = full.display().to_string();
        }
        Some(entry)
    }

    /// `:preset load name`: the preset of that name for what's under the cursor.
    fn load_preset_named(&mut self, name: &str) {
        let target = self.preset_target();
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let path = match preset_path(&folder, target.kind(), name) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if !path.is_file() {
            let home = home_folder();
            self.status = format!(
                "There's no {} preset called {name} in {}",
                target.kind().noun(),
                place_name(&folder.join(target.kind().folder()), home.as_deref())
            );
            return;
        }
        self.load_preset(target.kind(), &path);
    }

    /// Load a preset on what's under the cursor, as one undo step: a sound on its track, keeping
    /// the track's level, pan, sends, name and phrases, or an effect into the slot under it.
    pub(crate) fn load_preset(&mut self, kind: PresetKind, path: &Path) {
        if kind == PresetKind::Patch {
            let over = std::mem::take(&mut self.patch_over);
            self.add_patch(path, over);
            return;
        }
        let target = self.preset_target();
        let name = preset_name(path);
        match (kind, target) {
            (PresetKind::Sound, Target::Sound(track)) => self.load_sound_preset(track, path, &name),
            (PresetKind::Effect, Target::Effect { track, slot }) => {
                self.load_effect_preset(track, slot, path, &name);
            }
            (PresetKind::Effect, Target::Sound(_)) => {
                self.status =
                    "An effect preset loads into an FX slot: put the cursor on one on the FX page"
                        .to_owned();
            }
            (PresetKind::Sound, Target::Effect { track, .. }) => {
                self.load_sound_preset(track, path, &name);
            }
            (PresetKind::Patch, _) => {}
        }
    }

    fn load_sound_preset(&mut self, track: usize, path: &Path, name: &str) {
        if !self.takes_sound(Target::Sound(track)) {
            return;
        }
        let preset = match load_sound(path) {
            Ok(preset) => preset,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let mut actions = Vec::new();
        let sample = match &preset.sound.sample {
            None => 0,
            Some(entry) => match self.bank_preset_sample(entry) {
                Ok((number, add)) => {
                    actions.extend(add);
                    number
                }
                Err(error) => {
                    self.status = format!("{name} can't load: {error}");
                    return;
                }
            },
        };
        actions.push(Action::LoadSound {
            track,
            preset: Box::new(preset),
            sample,
        });
        self.status.clear();
        self.apply(&Action::Batch(actions));
        if self.status.is_empty() {
            self.status = format!(
                "Track {:02} plays {name}, keeping its phrases, level and sends: u undoes it",
                track + 1
            );
        }
    }

    fn load_effect_preset(&mut self, track: usize, slot: usize, path: &Path, name: &str) {
        let preset = match load_effect(path) {
            Ok(preset) => preset,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let fine = match preset.fine_in(slot) {
            Ok(fine) => fine,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        self.status.clear();
        self.apply(&Action::LoadEffect {
            track,
            slot,
            insert: preset.effect,
            fine,
        });
        if self.status.is_empty() {
            self.status = format!(
                "FX {} of track {:02} is {name}, a {}: u undoes it",
                slot + 1,
                track + 1,
                preset.effect.name()
            );
        }
    }
}
