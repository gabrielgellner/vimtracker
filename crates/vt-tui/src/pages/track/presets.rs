//! Presets on the track page (decision 310): saving the cursor's track's sound, or on the FX page
//! the effect in the slot under the cursor, into the presets folder, and loading one from the
//! browser or by name, as one undo step that leaves the track's place in the song and its phrases
//! alone.

use std::path::{Path, PathBuf};

use vt_model::action::Action;
use vt_model::preset::{
    EffectPreset, Preset, PresetError, PresetKind, SoundPreset, is_preset_name, load, preset_name,
    preset_path, save,
};

use crate::app::App;
use crate::input::mode::View;
use crate::overlays::command::parse::PresetLine;
use crate::overlays::picker::explorer::{Purpose, place_name};
use crate::pages::track::{Control, Page};
use crate::services::config::home_folder;

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

/// What a preset of this kind is called in a sentence, as `sound preset` or `patch`, and the
/// command that saves one.
fn called(kind: PresetKind) -> (String, &'static str) {
    match kind {
        PresetKind::Patch => ("patch".to_owned(), ":patch"),
        kind => (format!("{} preset", kind.noun()), ":preset"),
    }
}

/// Where a kind's presets are kept in the presets folder, as the status line names it.
pub(crate) fn kind_place(folder: &Path, kind: PresetKind) -> String {
    let home = home_folder();
    place_name(&folder.join(kind.folder()), home.as_deref())
}

/// The file of the preset of this kind and name in the presets folder, if there's a folder and
/// the preset is in it, or why the name can't be one.
pub(crate) fn named_file(
    folder: Option<&Path>,
    kind: PresetKind,
    name: &str,
) -> Result<Option<PathBuf>, PresetError> {
    if !is_preset_name(name) {
        return Err(PresetError::Name);
    }
    Ok(folder
        .and_then(|folder| preset_path(folder, kind, name).ok())
        .filter(|path| path.is_file()))
}

impl App {
    /// What `:preset` acts on here: on the FX page with the cursor on a slot, its effect, and
    /// anywhere else the cursor's track's sound.
    fn preset_target(&self) -> Target {
        let track = self.services.cursor.track;
        if self.modes.view == View::Track
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
        Some(self.services.settings.preset_folder()?.join(kind.folder()))
    }

    /// Whether this track plays sounds, saying `not_sound` if it doesn't.
    pub(crate) fn plays_sounds(&mut self, track: usize, not_sound: &str) -> bool {
        let sound = self
            .song()
            .tracks
            .get(track)
            .is_some_and(vt_model::song::Track::is_sound);
        if !sound {
            not_sound.clone_into(&mut self.services.status);
        }
        sound
    }

    /// Whether the cursor's track plays sounds, saying why not if it doesn't, for a sound preset.
    fn takes_sound(&mut self, target: Target) -> bool {
        match target {
            Target::Sound(track) => self.plays_sounds(track, NOT_SOUND),
            Target::Effect { .. } => true,
        }
    }

    /// The presets folder, or none, saying so.
    pub(crate) fn presets_folder(&mut self) -> Option<PathBuf> {
        let folder = self.services.settings.preset_folder();
        if folder.is_none() {
            self.services.status = NO_FOLDER.to_owned();
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
            self.services.status = format!(
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
        let saved = format!("{} as {name}", target.said());
        match target {
            Target::Sound(track) => self.save_named(
                name,
                force,
                |app| {
                    let sample = app.preset_sample(track);
                    Ok(SoundPreset::of(&app.song().tracks[track], sample))
                },
                |_| saved,
            ),
            Target::Effect { track, slot } => self.save_named(
                name,
                force,
                |app| {
                    EffectPreset::of(&app.song().tracks[track], slot).ok_or_else(|| {
                        format!("FX {} is empty, so there's nothing to save", slot + 1)
                    })
                },
                |_| saved,
            ),
        }
    }

    /// Save a preset of this name in its kind's folder, which is made if it isn't there: what
    /// `make` makes, or else why there's nothing to save. A name that's taken is refused unless
    /// forced. `saved` says what's saved as what, for the status line.
    pub(crate) fn save_named<P: Preset>(
        &mut self,
        name: &str,
        force: bool,
        make: impl FnOnce(&Self) -> Result<P, String>,
        saved: impl FnOnce(&P) -> String,
    ) {
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let path = match preset_path(&folder, P::KIND, name) {
            Ok(path) => path,
            Err(error) => {
                self.services.status = error.to_string();
                return;
            }
        };
        let preset = match make(self) {
            Ok(preset) => preset,
            Err(why) => {
                self.services.status = why;
                return;
            }
        };
        match save(&path, &preset, force) {
            Ok(()) => {
                if let Some(saved_in) = path.parent() {
                    self.note_place(Purpose::Preset(P::KIND), saved_in);
                }
                self.services.status = format!(
                    "Saved {}, in {}",
                    saved(&preset),
                    kind_place(&folder, P::KIND)
                );
            }
            Err(PresetError::Taken(_)) => {
                let (called, command) = called(P::KIND);
                self.services.status = format!(
                    "There's a {called} called {name} already: {command}! save {name} writes over it"
                );
            }
            Err(error) => self.services.status = error.to_string(),
        }
    }

    /// The bank entry a track's sampler plays, with its files by their full paths, so a preset
    /// finds them from any song.
    fn preset_sample(&self, track: usize) -> Option<vt_model::bank::BankEntry> {
        let number = self.song().tracks.get(track)?.sample();
        let mut entry = self.song().bank.get(number)?.clone();
        for source in entry.sources_mut() {
            let resolved =
                vt_model::project::resolve_sample(self.services.folder.as_deref(), &source.path);
            let full = resolved.canonicalize().unwrap_or(resolved);
            source.path = full.display().to_string();
        }
        Some(entry)
    }

    /// `:preset load name`: the preset of that name for what's under the cursor.
    fn load_preset_named(&mut self, name: &str) {
        let kind = self.preset_target().kind();
        let Some(folder) = self.presets_folder() else {
            return;
        };
        match named_file(Some(&folder), kind, name) {
            Ok(Some(path)) => self.load_preset(kind, &path),
            Ok(None) => {
                self.services.status = format!(
                    "There's no {} preset called {name} in {}",
                    kind.noun(),
                    kind_place(&folder, kind)
                );
            }
            Err(error) => self.services.status = error.to_string(),
        }
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
                self.services.status =
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
        let preset = match load::<SoundPreset>(path) {
            Ok(preset) => preset,
            Err(error) => {
                self.services.status = error.to_string();
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
                    self.services.status = format!("{name} can't load: {error}");
                    return;
                }
            },
        };
        actions.push(Action::LoadSound {
            track,
            preset: Box::new(preset),
            sample,
        });
        self.services.status.clear();
        self.services.apply(&Action::Batch(actions));
        if self.services.status.is_empty() {
            self.services.status = format!(
                "Track {:02} plays {name}, keeping its phrases, level and sends: u undoes it",
                track + 1
            );
        }
    }

    fn load_effect_preset(&mut self, track: usize, slot: usize, path: &Path, name: &str) {
        let preset = match load::<EffectPreset>(path) {
            Ok(preset) => preset,
            Err(error) => {
                self.services.status = error.to_string();
                return;
            }
        };
        let fine = match preset.fine_in(slot) {
            Ok(fine) => fine,
            Err(error) => {
                self.services.status = error;
                return;
            }
        };
        self.services.status.clear();
        self.services.apply(&Action::LoadEffect {
            track,
            slot,
            insert: preset.effect,
            fine,
        });
        if self.services.status.is_empty() {
            self.services.status = format!(
                "FX {} of track {:02} is {name}, a {}: u undoes it",
                slot + 1,
                track + 1,
                preset.effect.name()
            );
        }
    }
}
