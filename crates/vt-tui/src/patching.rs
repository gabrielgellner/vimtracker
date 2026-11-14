//! Patches on the track page (decision 311): the cursor's track's macros and what they move saved
//! with `:patch save` into the presets folder's `patches`, and added to a track from the browser
//! or by name, into its free mod slots and macros, with their generators on the phrase under the
//! cursor, as one undo step that keeps the track's sound.

use std::path::Path;

use vt_model::action::Action;
use vt_model::constants::FX_SLOTS;
use vt_model::macros::GenColumn;
use vt_model::preset::{
    PatchGen, PatchPreset, Placement, PresetError, PresetKind, Short, load_patch, preset_name,
    preset_path, save,
};
use vt_model::selection::Field;
use vt_model::shape::ShapeColumn;
use vt_model::step::FxCommand;

use crate::app::App;
use crate::config::home_folder;
use crate::explorer::{Purpose, place_name};
use crate::input::command::PatchLine;

/// What's said where a patch is wanted on a track that doesn't play sounds.
const NOT_SOUND: &str = "Only a track that plays sounds takes a patch";

/// Numbers as a sentence lists them, from 1: `3`, `3 and 5`, `2, 3 and 4`.
fn listed(places: &[usize]) -> String {
    let names: Vec<String> = places.iter().map(|at| (at + 1).to_string()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    }
}

/// Mod slots or macros by number, as `mod slot 3` or `macros 2, 3 and 4`, or nothing for none.
fn parts(one: &str, many: &str, places: &[usize]) -> Option<String> {
    match places.len() {
        0 => None,
        1 => Some(format!("{one} {}", listed(places))),
        _ => Some(format!("{many} {}", listed(places))),
    }
}

/// So many of something, as `1 free mod slot` or `3 free macros`.
fn counted(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 free {one}")
    } else {
        format!("{n} free {many}")
    }
}

/// Why a patch doesn't fit a track, and what `!` does.
fn short_of(name: &str, track: usize, short: Short) -> String {
    let (mods, macros) = (short.mods, short.macros);
    let mut needs = Vec::new();
    let mut has = Vec::new();
    if mods.0 > 0 {
        needs.push(counted(mods.0, "mod slot", "mod slots"));
        has.push(counted(mods.1, "slot", "slots"));
    }
    if macros.0 > 0 {
        needs.push(counted(macros.0, "macro", "macros"));
        has.push(counted(macros.1, "macro", "macros"));
    }
    format!(
        "{name} needs {}; track {:02} has {}: :patch! puts it over the last ones",
        needs.join(" and "),
        track + 1,
        has.join(" and ")
    )
}

impl App {
    /// `:patch`, `:patch save` and `:patch load`, and their `!`.
    pub(crate) fn patch_line(&mut self, line: PatchLine) {
        match line {
            PatchLine::Browse { over } => self.open_patch_explorer(over),
            PatchLine::Save { name, force } => self.save_patch(&name, force),
            PatchLine::Load { name, over } => self.add_patch_named(&name, over),
        }
    }

    /// The cursor's track, if it plays sounds, saying so if it doesn't.
    fn patch_track(&mut self) -> Option<usize> {
        let track = self.cursor.track;
        let sound = self
            .song()
            .tracks
            .get(track)
            .is_some_and(vt_model::song::Track::is_sound);
        if !sound {
            self.status = NOT_SOUND.to_owned();
        }
        sound.then_some(track)
    }

    /// List the patches in the presets folder, to add one to the cursor's track; `over` puts it
    /// over the last mod slots and macros where too few are free.
    pub(crate) fn open_patch_explorer(&mut self, over: bool) {
        let Some(track) = self.patch_track() else {
            return;
        };
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let folder = folder.join(PresetKind::Patch.folder());
        if !folder.is_dir() {
            let home = home_folder();
            self.status = format!(
                "No patches yet: :patch save name saves track {:02}'s macros and what they move in {}",
                track + 1,
                place_name(&folder, home.as_deref())
            );
            return;
        }
        self.patch_over = over;
        let purpose = Purpose::Preset(PresetKind::Patch);
        let roots = self.places(purpose);
        self.open_explorer(purpose, roots, None);
    }

    /// Save the cursor's track's macros, the parts they move and the live generators they hold on
    /// the phrase under the cursor, as a patch of this name. A name that's taken is refused unless
    /// forced, and a track whose macros hold nothing has no patch.
    pub(crate) fn save_patch(&mut self, name: &str, force: bool) {
        let Some(track) = self.patch_track() else {
            return;
        };
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let path = match preset_path(&folder, PresetKind::Patch, name) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let phrase = self.phrase_here().map(|phrase| &**phrase);
        let Some(preset) = PatchPreset::of(&self.song().tracks[track], phrase) else {
            self.status = format!(
                "No macro of track {:02} holds anything, so there's no patch to save: M on a setting maps one",
                track + 1
            );
            return;
        };
        match save(&path, &preset, force) {
            Ok(()) => {
                if let Some(saved_in) = path.parent() {
                    self.note_place(Purpose::Preset(PresetKind::Patch), saved_in);
                }
                let home = home_folder();
                self.status = format!(
                    "Saved track {:02}'s patch as {name}, {}, in {}",
                    track + 1,
                    preset.summary(),
                    place_name(&folder.join(PresetKind::Patch.folder()), home.as_deref())
                );
            }
            Err(PresetError::Taken(_)) => {
                self.status = format!(
                    "There's a patch called {name} already: :patch! save {name} writes over it"
                );
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// `:patch load name`: the patch of that name, added to the cursor's track.
    fn add_patch_named(&mut self, name: &str, over: bool) {
        let Some(folder) = self.presets_folder() else {
            return;
        };
        let path = match preset_path(&folder, PresetKind::Patch, name) {
            Ok(path) => path,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        if !path.is_file() {
            let home = home_folder();
            self.status = format!(
                "There's no patch called {name} in {}",
                place_name(&folder.join(PresetKind::Patch.folder()), home.as_deref())
            );
            return;
        }
        self.add_patch(&path, over);
    }

    /// Add a patch to the cursor's track, as one undo step: its mod slots and macros into free
    /// ones, or with `over` the last ones where too few are free, and its generators onto the
    /// phrase under the cursor. The track's sound is left alone. If any part can't go on, none
    /// does, and the status line says why.
    pub(crate) fn add_patch(&mut self, path: &Path, over: bool) {
        let Some(track) = self.patch_track() else {
            return;
        };
        let name = preset_name(path);
        let patch = match load_patch(path) {
            Ok(preset) => preset.patch,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let mut placement = match patch.place(&self.song().tracks[track], over) {
            Ok(placement) => placement,
            Err(short) => {
                self.status = short_of(&name, track, short);
                return;
            }
        };
        let mut generators = Vec::new();
        let mut taken = Vec::new();
        for generator in &patch.generators {
            let made = self
                .patch_column(generator, &placement, &mut taken)
                .and_then(|(field, command, live)| {
                    if let (GenColumn::Fx(saved), Field::FxCommand(slot)) =
                        (generator.column, field)
                    {
                        placement.fx.insert(saved, slot);
                    }
                    self.patch_generator(field, command, generator.shape, live)
                });
            match made {
                Ok(action) => generators.push(action),
                Err(why) => {
                    self.status = format!("{name} isn't added: {why}");
                    return;
                }
            }
        }
        let said = self.patch_added(&name, track, &patch.generators, &placement);
        let mut actions = vec![Action::LoadPatch {
            track,
            patch: Box::new(patch),
            placement,
        }];
        actions.extend(generators);
        self.status.clear();
        self.apply(&Action::Batch(actions));
        if self.status.is_empty() {
            self.status = said;
        }
    }

    /// Where a patch's generator goes on the phrase under the cursor: its column, an FX one's
    /// command, and the generator already live there, if one is. An FX generator goes on the
    /// first FX column live with its command, or holding it, or else empty, and none other of
    /// the patch's has taken; its command is the one `placement` makes it, a lock on one of the
    /// patch's slots or macros on the one that part went to.
    fn patch_column(
        &self,
        generator: &PatchGen,
        placement: &Placement,
        taken: &mut Vec<usize>,
    ) -> Result<(Field, Option<FxCommand>, Option<usize>), String> {
        let phrase = self
            .phrase_here()
            .ok_or("this track has no phrase here for its generators")?;
        let live = phrase.generators();
        let field = match generator.column {
            GenColumn::Note => Field::Note,
            GenColumn::Velocity => Field::Velocity,
            GenColumn::Nudge => Field::Nudge,
            GenColumn::Length => Field::GateLength,
            GenColumn::Fx(_) => {
                let command = generator
                    .fx_command_on(placement)
                    .ok_or("its FX generator's command isn't known")?;
                let open = |slot: &usize| !taken.contains(slot);
                let live_with = (0..FX_SLOTS).filter(open).find_map(|slot| {
                    live.iter()
                        .position(|found| found.column == ShapeColumn::Fx { slot, command })
                        .map(|at| (slot, at))
                });
                if let Some((slot, at)) = live_with {
                    taken.push(slot);
                    return Ok((Field::FxCommand(slot), Some(command), Some(at)));
                }
                let unlive = |slot: &usize| {
                    !live
                        .iter()
                        .any(|found| matches!(found.column, ShapeColumn::Fx { slot: at, .. } if at == *slot))
                };
                let holds = |slot: &usize, wanted: Option<FxCommand>| {
                    let mut commands = phrase
                        .steps()
                        .iter()
                        .filter_map(|step| step.fx.get(*slot).and_then(|fx| fx.command));
                    match wanted {
                        Some(command) => commands.any(|found| found == command),
                        None => commands.next().is_none(),
                    }
                };
                let free = || (0..FX_SLOTS).filter(open).filter(unlive);
                let slot = free()
                    .find(|slot| holds(slot, Some(command)))
                    .or_else(|| free().find(|slot| holds(slot, None)))
                    .ok_or_else(|| {
                        format!("no FX column is free for its {} generator", command.name())
                    })?;
                taken.push(slot);
                return Ok((Field::FxCommand(slot), Some(command), None));
            }
        };
        let at = live
            .iter()
            .position(|found| GenColumn::of(found.column) == generator.column);
        Ok((field, None, at))
    }

    /// What the status line says once a patch is added: where its parts went, and what it put
    /// over.
    fn patch_added(
        &self,
        name: &str,
        track: usize,
        generators: &[PatchGen],
        placement: &Placement,
    ) -> String {
        let mut went: Vec<String> = parts("mod slot", "mod slots", &placement.mods)
            .into_iter()
            .chain(parts("macro", "macros", &placement.macros))
            .collect();
        if !generators.is_empty() {
            let columns: Vec<String> = generators
                .iter()
                .map(|generator| {
                    let column = match generator.column {
                        GenColumn::Fx(saved) => {
                            GenColumn::Fx(placement.fx.get(&saved).copied().unwrap_or(saved))
                        }
                        column => column,
                    };
                    column.name()
                })
                .collect();
            let phrase = self
                .phrase_id()
                .map_or_else(String::new, |phrase| format!(" on phrase {phrase:02X}"));
            went.push(format!("live {}{phrase}", columns.join(" ")));
        }
        let over: Vec<String> = parts("mod slot", "mod slots", &placement.over_mods)
            .into_iter()
            .chain(parts("macro", "macros", &placement.over_macros))
            .collect();
        let over = if over.is_empty() {
            String::new()
        } else {
            format!(", over {}", over.join(" and "))
        };
        let went = match went.split_last() {
            None => String::new(),
            Some((last, [])) => format!(" in {last}"),
            Some((last, rest)) => format!(" in {} and {last}", rest.join(", ")),
        };
        format!(
            "Track {:02} has {name}{went}{over}, keeping its sound: u undoes it",
            track + 1
        )
    }
}
