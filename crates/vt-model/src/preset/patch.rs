//! A patch on its own (decision 311): what plays a sound without the sound, after Live's MIDI
//! Effect Rack presets, saved from a track and added to any other, which keeps its sound.
//!
//! Saving takes the macros that hold something and what they move: the mod slots they map to,
//! the slots those go to in turn and the slots that go to any of them, the accumulator and the
//! arpeggio if one is reached, and the live generators of the phrase under the cursor that a macro
//! holds by its column, with any slot or macro an FX one of those locks (decision 317). Nothing of
//! the sound: a map to the machine, the filter or an effect is kept by the setting's key, for a
//! track that has it.
//!
//! Loading adds the parts to free mod slots and free macros, in order, renumbering every
//! reference among them, its generators' locks too, and refuses when there are too few unless
//! told to put them over the last ones (the study's §5.3).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::{PRESET_FORMAT, Patch, PresetError, io_at, parse};
use crate::constants::FX_SLOTS;
use crate::generator::Generator;
use crate::macros::{GenColumn, MACROS, Macro, MacroTarget};
use crate::modulation::MOD_SLOTS;
use crate::settings::{Owner, SettingId};
use crate::shape::{FillKind, Shape, ShapeColumn};
use crate::song::{Phrase, Track};
use crate::step::FxCommand;

/// A live generator a patch carries: its column, an FX column's command by its key, and its
/// shape, which goes onto the phrase under the cursor when the patch is loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchGen {
    pub column: GenColumn,
    /// The command an FX generator writes, by the key songs store it by, as `cut`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub shape: Shape,
}

impl PatchGen {
    /// The command an FX generator writes, if it's one this build knows.
    pub fn fx_command(&self) -> Option<FxCommand> {
        self.command.as_deref().and_then(FxCommand::from_key)
    }

    /// The command an FX generator writes on a track, placed: a lock on one of the patch's slots
    /// or macros, as `DEP1` or `MAC2`, renumbered to where that part went, so it plays there.
    pub fn fx_command_on(&self, placement: &Placement) -> Option<FxCommand> {
        Some(match self.fx_command()? {
            FxCommand::Set(id) => FxCommand::Set(placement.setting(id)),
            command => command,
        })
    }

    /// What it is, for a list: `Note generator`, `FX1 generator`.
    fn summary(&self) -> String {
        format!("{} generator", self.column.name())
    }
}

/// A patch preset's file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchPreset {
    pub format: u32,
    pub patch: Patch,
    /// Its own tags, beside `patch`, which every patch has (decision 315).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

/// Where a patch's parts go on a track: the track's mod slot for each of the patch's, and its
/// macro for each of the patch's, in order, with those that weren't free and are put over; and
/// for an FX generator put on another FX column than it was saved on, which, so its macros follow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Placement {
    pub mods: Vec<usize>,
    pub macros: Vec<usize>,
    /// The slots and macros among those that held something, which the patch replaces.
    pub over_mods: Vec<usize>,
    pub over_macros: Vec<usize>,
    /// An FX generator's column as saved, and the one it's put on.
    pub fx: BTreeMap<usize, usize>,
}

impl Placement {
    /// A setting of the patch's on the track: one of its slots' or macros' renumbered to where
    /// that went, and any other as it is.
    pub fn setting(&self, id: SettingId) -> SettingId {
        let mods = |at: usize| self.mods.get(at).copied();
        let macros = |at: usize| self.macros.get(at).copied();
        renumber(id, mods, macros).unwrap_or(id)
    }
}

/// Why a patch doesn't fit: how many mod slots and macros it needs, and how many are free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Short {
    pub mods: (usize, usize),
    pub macros: (usize, usize),
}

/// The parts of a track a patch of it holds, by their places on the track.
#[derive(Debug, Default)]
struct Parts {
    mods: BTreeSet<usize>,
    macros: BTreeSet<usize>,
    accumulation: bool,
    arpeggio: bool,
}

impl Parts {
    /// Take in the part a setting belongs to, if it's one a patch holds; whether it's new.
    fn reach(&mut self, id: SettingId) -> bool {
        match id.owner().0 {
            Owner::Mod(slot) | Owner::ModOwn(slot) => self.mods.insert(slot),
            Owner::Macro(at) => self.macros.insert(at),
            Owner::Accumulation => !std::mem::replace(&mut self.accumulation, true),
            Owner::Arpeggio => !std::mem::replace(&mut self.arpeggio, true),
            _ => false,
        }
    }

    /// Whether a setting belongs to a part already held.
    fn holds(&self, id: SettingId) -> bool {
        match id.owner().0 {
            Owner::Mod(slot) | Owner::ModOwn(slot) => self.mods.contains(&slot),
            Owner::Macro(at) => self.macros.contains(&at),
            Owner::Accumulation => self.accumulation,
            Owner::Arpeggio => self.arpeggio,
            _ => false,
        }
    }

    /// The macros holding something and everything they move, with the settings in `locked`,
    /// which their generators lock, and every slot going to any of that, until nothing more is
    /// reached.
    fn of(track: &Track, locked: impl IntoIterator<Item = SettingId>) -> Self {
        let mut parts = Self {
            macros: (0..MACROS)
                .filter(|at| !track.macros[*at].maps.is_empty())
                .collect(),
            ..Self::default()
        };
        for id in locked {
            parts.reach(id);
        }
        loop {
            let mut grew = false;
            for at in parts.macros.clone() {
                for map in &track.macros[at].maps {
                    if let Some(id) = map.target.setting() {
                        grew |= parts.reach(id);
                    }
                }
            }
            for (slot, held) in track.mods.iter().enumerate() {
                let Some(id) = held.destination else {
                    continue;
                };
                if parts.mods.contains(&slot) {
                    grew |= parts.reach(id);
                } else if parts.holds(id) {
                    grew |= parts.mods.insert(slot);
                }
            }
            if !grew {
                return parts;
            }
        }
    }
}

/// A setting with its mod slot or macro renumbered, or none if it's on one the numbering hasn't.
/// Any other setting stays as it is.
fn renumber(
    id: SettingId,
    mods: impl Fn(usize) -> Option<usize>,
    macros: impl Fn(usize) -> Option<usize>,
) -> Option<SettingId> {
    match id.owner() {
        (Owner::Mod(slot), at) => SettingId::modulator(mods(slot)?, at),
        (Owner::ModOwn(slot), at) => SettingId::mod_own(mods(slot)?, at),
        (Owner::Macro(at), _) => SettingId::macro_at(macros(at)?),
        _ => Some(id),
    }
}

/// Where a number is in a sorted list.
fn place_in(list: &[usize]) -> impl Fn(usize) -> Option<usize> + '_ {
    |at| list.iter().position(|found| *found == at)
}

/// The places to put this many parts, of those `free` says are free: the first free ones, or,
/// when there are too few and it may, those and the last that aren't, in order.
fn choose(needed: usize, free: &[bool], over: bool) -> Option<(Vec<usize>, Vec<usize>)> {
    let open: Vec<usize> = (0..free.len()).filter(|at| free[*at]).collect();
    if open.len() >= needed {
        return Some((open[..needed].to_vec(), Vec::new()));
    }
    if !over || needed > free.len() {
        return None;
    }
    let taken: Vec<usize> = (0..free.len()).filter(|at| !free[*at]).collect();
    let replaced = taken[taken.len() - (needed - open.len())..].to_vec();
    let mut chosen: Vec<usize> = open.into_iter().chain(replaced.iter().copied()).collect();
    chosen.sort_unstable();
    Some((chosen, replaced))
}

impl Patch {
    /// A track's behaviour as a patch: its macros that hold something and what they move, with
    /// the live generators of `phrase` they hold by column, numbered from the first part in the
    /// patch. A slot or macro an FX generator locks comes too, as `DEP4` or `MAC2`, so its lock
    /// follows the part when the patch is added. None if no macro holds anything, as a patch is
    /// its macros' device.
    pub fn of_track(track: &Track, phrase: Option<&Phrase>) -> Option<Self> {
        let columns: BTreeSet<GenColumn> = track
            .macros
            .iter()
            .flat_map(|held| &held.maps)
            .filter_map(|map| match map.target {
                MacroTarget::Gen { column, .. } => Some(column),
                MacroTarget::Setting(_) => None,
            })
            .collect();
        // A `Follow` reads another phrase, which a patch can't carry.
        let held: Vec<&Generator> = phrase
            .map(|phrase| phrase.generators())
            .unwrap_or_default()
            .iter()
            .filter(|found| columns.contains(&GenColumn::of(found.column)))
            .filter(|found| found.shape.kind != FillKind::Follow)
            .collect();
        let locked = held.iter().filter_map(|found| match found.column {
            ShapeColumn::Fx { command, .. } => command.setting(),
            _ => None,
        });
        let parts = Parts::of(track, locked);
        if parts.macros.is_empty() {
            return None;
        }
        let mods: Vec<usize> = parts.mods.iter().copied().collect();
        let macros: Vec<usize> = parts.macros.iter().copied().collect();
        let renumbered = |id: SettingId| renumber(id, place_in(&mods), place_in(&macros));
        let patch_mods = mods
            .iter()
            .map(|slot| {
                let mut held = track.mods[*slot];
                held.destination = held.destination.and_then(renumbered);
                held
            })
            .collect();
        let patch_macros: Vec<Macro> = macros
            .iter()
            .map(|at| {
                let mut held = track.macros[*at].clone();
                for map in &mut held.maps {
                    if let MacroTarget::Setting(id) = map.target
                        && let Some(id) = renumbered(id)
                    {
                        map.target = MacroTarget::Setting(id);
                    }
                }
                held
            })
            .collect();
        let generators = held
            .into_iter()
            .map(|found| PatchGen {
                column: GenColumn::of(found.column),
                command: match found.column {
                    ShapeColumn::Fx {
                        command: FxCommand::Set(id),
                        ..
                    } => Some(FxCommand::Set(renumbered(id).unwrap_or(id)).key()),
                    ShapeColumn::Fx { command, .. } => Some(command.key()),
                    _ => None,
                },
                shape: found.shape,
            })
            .collect();
        Some(Self {
            mods: patch_mods,
            accumulation: if parts.accumulation {
                track.accumulation
            } else {
                Default::default()
            },
            arpeggio: if parts.arpeggio {
                track.arpeggio
            } else {
                Default::default()
            },
            macros: patch_macros,
            generators,
        })
    }

    /// Every setting its parts reach, by their slots' destinations, their macros' maps and what
    /// its FX generators lock.
    fn reaches(&self) -> impl Iterator<Item = SettingId> + '_ {
        let dests = self.mods.iter().filter_map(|held| held.destination);
        let maps = self
            .macros
            .iter()
            .flat_map(|held| &held.maps)
            .filter_map(|map| map.target.setting());
        let locks = self
            .generators
            .iter()
            .filter_map(|generator| generator.fx_command()?.setting());
        dests.chain(maps).chain(locks)
    }

    /// Whether it holds the accumulator, as something in it reaches it.
    pub fn holds_accumulation(&self) -> bool {
        self.reaches().any(|id| id.owner().0 == Owner::Accumulation)
    }

    /// Whether it holds the arpeggio, as something in it reaches it.
    pub fn holds_arpeggio(&self) -> bool {
        self.reaches().any(|id| id.owner().0 == Owner::Arpeggio)
    }

    /// Where its slots and macros go on a track: into free ones, a free slot being one going
    /// nowhere that nothing reaches, and a free macro one holding nothing that no slot goes to.
    /// With too few, it's refused, saying how many are needed and free, unless `over` says to put
    /// the rest over the last that aren't free.
    pub fn place(&self, track: &Track, over: bool) -> Result<Placement, Short> {
        let reached: Vec<SettingId> = track
            .mods
            .iter()
            .filter_map(|held| held.destination)
            .chain(
                track
                    .macros
                    .iter()
                    .flat_map(|held| &held.maps)
                    .filter_map(|map| map.target.setting()),
            )
            .collect();
        let reaches_slot = |slot: usize| {
            reached
                .iter()
                .any(|id| matches!(id.owner().0, Owner::Mod(at) | Owner::ModOwn(at) if at == slot))
        };
        let free_mods: Vec<bool> = (0..MOD_SLOTS)
            .map(|slot| track.mods[slot].destination.is_none() && !reaches_slot(slot))
            .collect();
        let free_macros: Vec<bool> = (0..MACROS)
            .map(|at| {
                track.macros[at].maps.is_empty()
                    && !reached.contains(&SettingId::macro_at(at).expect("a macro"))
            })
            .collect();
        let count = |free: &[bool]| free.iter().filter(|free| **free).count();
        let short = Short {
            mods: (self.mods.len(), count(&free_mods)),
            macros: (self.macros.len(), count(&free_macros)),
        };
        let mods = choose(self.mods.len(), &free_mods, over);
        let macros = choose(self.macros.len(), &free_macros, over);
        let (Some((mods, over_mods)), Some((macros, over_macros))) = (mods, macros) else {
            return Err(short);
        };
        Ok(Placement {
            mods,
            macros,
            over_mods,
            over_macros,
            fx: BTreeMap::new(),
        })
    }

    /// Add it to a track where `placement` says: its slots and macros in theirs, every reference
    /// among them renumbered, a setting its macros map taken off any of the track's own as a
    /// setting is on one macro at most, and the accumulator and arpeggio if it holds them. What
    /// reached a slot or macro it puts over is let go, as that's another part now. The sound
    /// is left alone.
    pub fn add_to(&self, track: &mut Track, placement: &Placement) {
        let mods = |at: usize| placement.mods.get(at).copied();
        let macros = |at: usize| placement.macros.get(at).copied();
        let replaced = |id: SettingId| match id.owner().0 {
            Owner::Mod(slot) | Owner::ModOwn(slot) => placement.mods.contains(&slot),
            Owner::Macro(at) => placement.macros.contains(&at),
            _ => false,
        };
        let targets: Vec<MacroTarget> = self
            .macros
            .iter()
            .flat_map(|held| &held.maps)
            .map(|map| self.target_on(map.target, placement))
            .collect();
        for (at, held) in track.macros.iter_mut().enumerate() {
            if !placement.macros.contains(&at) {
                held.maps.retain(|map| {
                    !map.target.setting().is_some_and(replaced) && !targets.contains(&map.target)
                });
            }
        }
        for (slot, held) in track.mods.iter_mut().enumerate() {
            if !placement.mods.contains(&slot) && held.destination.is_some_and(replaced) {
                held.destination = None;
            }
        }
        for (held, slot) in self.mods.iter().zip(&placement.mods) {
            let mut held = *held;
            held.destination = held.destination.and_then(|id| renumber(id, mods, macros));
            track.mods[*slot] = held;
        }
        for (held, at) in self.macros.iter().zip(&placement.macros) {
            let mut held = held.clone();
            for map in &mut held.maps {
                map.target = self.target_on(map.target, placement);
            }
            track.macros[*at] = held;
        }
        if self.holds_accumulation() {
            track.accumulation = self.accumulation;
        }
        if self.holds_arpeggio() {
            track.arpeggio = self.arpeggio;
        }
    }

    /// What a map of its reaches on a track, placed: a slot's or macro's setting renumbered, and
    /// an FX generator's column where its generator was put.
    fn target_on(&self, target: MacroTarget, placement: &Placement) -> MacroTarget {
        match target {
            MacroTarget::Setting(id) => MacroTarget::Setting(placement.setting(id)),
            MacroTarget::Gen {
                column: GenColumn::Fx(slot),
                param,
            } => MacroTarget::Gen {
                column: GenColumn::Fx(placement.fx.get(&slot).copied().unwrap_or(slot)),
                param,
            },
            target => target,
        }
    }

    /// What it needs, for a list: its generators, then how many mod slots and macros, as
    /// `1 slot, 3 macros` or `Note generator, 2 macros`.
    pub fn needs(&self) -> String {
        let count = |n: usize, one: &str, many: &str| match n {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {many}")),
        };
        self.generators
            .iter()
            .map(PatchGen::summary)
            .chain(count(self.mods.len(), "slot", "slots"))
            .chain(count(self.macros.len(), "macro", "macros"))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Check what it holds fits a track and refers only to its own parts, beyond what reading it
    /// checks.
    fn check_alone(&self) -> Result<(), String> {
        if self.mods.len() > MOD_SLOTS {
            return Err(format!(
                "it has {} mod slots, and a track has {MOD_SLOTS}",
                self.mods.len()
            ));
        }
        if self.macros.len() > MACROS {
            return Err(format!(
                "it has {} macros, and a track has {MACROS}",
                self.macros.len()
            ));
        }
        let (mods, macros) = (self.mods.len(), self.macros.len());
        for id in self.reaches() {
            let outside = match id.owner().0 {
                Owner::Mod(slot) | Owner::ModOwn(slot) => slot >= mods,
                Owner::Macro(at) => at >= macros,
                _ => false,
            };
            if outside {
                return Err(format!(
                    "it reaches {}, and it has {mods} mod slots and {macros} macros",
                    id.key()
                ));
            }
        }
        let mut columns = BTreeSet::new();
        for generator in &self.generators {
            if !columns.insert(generator.column) {
                return Err(format!(
                    "it has two generators on {}",
                    generator.column.name()
                ));
            }
            match (generator.column, &generator.command) {
                (GenColumn::Fx(slot), _) if slot >= FX_SLOTS => {
                    return Err(format!(
                        "it has a generator on FX{}, and a phrase has {FX_SLOTS}",
                        slot + 1
                    ));
                }
                (GenColumn::Fx(_), Some(key)) if FxCommand::from_key(key).is_none() => {
                    return Err(format!("there's no FX command called {key}"));
                }
                (GenColumn::Fx(slot), None) => {
                    return Err(format!("its FX{} generator has no command", slot + 1));
                }
                (GenColumn::Fx(_), Some(_)) | (_, None) => {}
                (column, Some(_)) => {
                    return Err(format!(
                        "its {} generator has a command, which only an FX one has",
                        column.name()
                    ));
                }
            }
        }
        Ok(())
    }
}

impl PatchPreset {
    /// A track's behaviour as a patch preset, if a macro of it holds something.
    pub fn of(track: &Track, phrase: Option<&Phrase>) -> Option<Self> {
        Some(Self {
            format: PRESET_FORMAT,
            patch: Patch::of_track(track, phrase)?,
            tags: Vec::new(),
        })
    }

    /// The tag it always has: `patch`, so `#patch` lists them all, beside `sound` and `effect`.
    pub fn fixed_tags(&self) -> Vec<String> {
        vec![super::PresetKind::Patch.noun().to_owned()]
    }

    /// What it is, for a list.
    pub fn summary(&self) -> String {
        self.patch.needs()
    }
}

/// Read a patch preset.
pub fn load_patch(path: &Path) -> Result<PatchPreset, PresetError> {
    let text = fs::read_to_string(path).map_err(io_at(path))?;
    let preset: PatchPreset = parse(&text, path)?;
    preset
        .patch
        .check_alone()
        .map_err(|detail| PresetError::Holds {
            path: path.display().to_string(),
            detail,
        })?;
    Ok(preset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::MacroMap;
    use crate::modulation::{ModKind, ModSlot, default_mods};
    use crate::song::Song;

    /// The tutorial's Turing Machine: a Steps slot fourth, on `Pitch` in scale, with `Change`,
    /// `Length` and `Range` on its `Mutate`, `Length` and `Depth`.
    fn turing() -> Track {
        let mut track = Song::new(1).tracks[0].clone();
        let mut steps = track.mods[3];
        steps.kind = ModKind::Steps;
        steps.destination = Some(SettingId::PITCH);
        steps.in_scale = true;
        steps.depth = 12;
        track.mods[3] = steps;
        let own = |name: &str| SettingId::from_key(&format!("mod4.{name}")).unwrap();
        for (at, (name, target)) in [
            ("Change", own("mutate")),
            ("Length", own("length")),
            ("Range", own("depth")),
        ]
        .into_iter()
        .enumerate()
        {
            track.macros[at] = Macro {
                name: Some(name.to_owned()),
                value: 0x4000,
                maps: vec![MacroMap::whole(MacroTarget::Setting(target))],
            };
        }
        track
    }

    fn key_of(target: MacroTarget) -> String {
        target.setting().unwrap().key()
    }

    #[test]
    fn the_turing_machine_is_one_slot_and_three_macros_numbered_from_the_first() {
        let patch = Patch::of_track(&turing(), None).unwrap();
        assert_eq!(patch.mods.len(), 1);
        assert_eq!(patch.mods[0].kind, ModKind::Steps);
        assert_eq!(patch.mods[0].destination, Some(SettingId::PITCH));
        let keys: Vec<String> = patch
            .macros
            .iter()
            .map(|m| key_of(m.maps[0].target))
            .collect();
        assert_eq!(keys, ["mod1.mutate", "mod1.length", "mod1.depth"]);
        assert_eq!(patch.needs(), "1 slot, 3 macros");
        assert!(!patch.holds_accumulation());
        assert!(Patch::of_track(&Song::new(1).tracks[0], None).is_none());
    }

    #[test]
    fn a_patch_goes_into_free_slots_and_macros_keeping_the_sound() {
        let patch = Patch::of_track(&turing(), None).unwrap();
        let mut track = Song::new(2).tracks[1].clone();
        track.cutoff = 0x33;
        track.macros[0].maps = vec![MacroMap::whole(MacroTarget::Setting(SettingId::CUT))];
        let placement = patch.place(&track, false).unwrap();
        // Slots 1 and 2 shape the note and the cutoff; 3 is the first free.
        assert_eq!(placement.mods, [2]);
        assert_eq!(placement.macros, [1, 2, 3]);
        assert!(placement.over_mods.is_empty());
        patch.add_to(&mut track, &placement);
        assert_eq!(track.mods[2].kind, ModKind::Steps);
        assert_eq!(track.mods[3], default_mods()[3]);
        assert_eq!(key_of(track.macros[1].maps[0].target), "mod3.mutate");
        assert_eq!(key_of(track.macros[3].maps[0].target), "mod3.depth");
        assert_eq!(track.macros[1].name.as_deref(), Some("Change"));
        assert_eq!(track.cutoff, 0x33, "the sound is kept");
        assert_eq!(track.macros[0].maps.len(), 1, "its own macro is kept");
    }

    #[test]
    fn too_few_free_is_refused_and_over_takes_the_last() {
        let patch = Patch::of_track(&turing(), None).unwrap();
        let mut track = turing();
        // Macros 4 to 7 hold settings of the track's own, so only the last is free.
        let own = [
            SettingId::CUT,
            SettingId::PAN,
            SettingId::SEND_A,
            SettingId::SEND_B,
        ];
        for (at, id) in (3..).zip(own) {
            track.macros[at].maps = vec![MacroMap::whole(MacroTarget::Setting(id))];
        }
        let short = patch.place(&track, false).unwrap_err();
        assert_eq!(short.macros, (3, 1));
        let placement = patch.place(&track, true).unwrap();
        assert_eq!(placement.macros, [5, 6, 7]);
        assert_eq!(placement.over_macros, [5, 6]);
        patch.add_to(&mut track, &placement);
        assert_eq!(track.macros[5].name.as_deref(), Some("Change"));
    }

    #[test]
    fn a_slot_going_to_a_part_of_it_comes_too_and_points_at_its_new_place() {
        let mut track = turing();
        // An LFO in slot 6 on the Steps slot's depth, and the arpeggio's rate on a macro.
        track.mods[5] = ModSlot {
            destination: SettingId::from_key("mod4.depth"),
            ..track.mods[5]
        };
        let rate = SettingId::arpeggio(0).unwrap();
        track.macros[4].maps = vec![MacroMap::whole(MacroTarget::Setting(rate))];
        track.arpeggio.octaves = 2;
        let patch = Patch::of_track(&track, None).unwrap();
        assert_eq!(patch.mods.len(), 2);
        assert_eq!(patch.mods[1].destination, SettingId::from_key("mod1.depth"));
        assert!(patch.holds_arpeggio());
        assert_eq!(patch.macros.len(), 4);

        let mut other = Song::new(1).tracks[0].clone();
        let placement = patch.place(&other, false).unwrap();
        assert_eq!(placement.mods, [2, 3]);
        patch.add_to(&mut other, &placement);
        assert_eq!(other.mods[3].destination, SettingId::from_key("mod3.depth"));
        assert_eq!(other.arpeggio.octaves, 2);
    }

    #[test]
    fn a_macro_on_an_fx_generator_follows_it_to_the_column_it_goes_on() {
        let lane = MacroTarget::Gen {
            column: GenColumn::Fx(0),
            param: crate::shape::GenParam::Hits,
        };
        let mut track = Song::new(1).tracks[0].clone();
        track.macros[0].maps = vec![MacroMap::numbers(lane, (0, 8))];
        let patch = Patch::of_track(&track, None).unwrap();
        let mut other = Song::new(1).tracks[0].clone();
        let mut placement = patch.place(&other, false).unwrap();
        placement.fx.insert(0, 2);
        patch.add_to(&mut other, &placement);
        assert_eq!(
            other.macros[0].maps[0].target,
            MacroTarget::Gen {
                column: GenColumn::Fx(2),
                param: crate::shape::GenParam::Hits,
            }
        );
    }

    /// A ramp from `A0` to `A0` over every row, as an FX generator's shape.
    fn level() -> Shape {
        use crate::shape::{
            Acid, Bits, DEFAULT_RATE, Follow, Grids, Pink, Random, Values, Variation, Walk, Where,
        };
        Shape {
            from: 0xA0,
            to: 0xA0,
            curve: 0,
            s: 0,
            kind: FillKind::Ramp,
            wave: crate::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::AllRows,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: Grids::default(),
            random: Random::default(),
            vary: 0,
            variation: Variation::default(),
        }
    }

    #[test]
    fn a_slot_or_macro_a_generator_locks_comes_along_and_the_lock_follows_it() {
        use crate::generator::Span;
        // Slot 4 is only reached by the `DEP4` FX1's generator writes, and macro 5, holding
        // nothing, only by the `MAC5` FX2's writes; macro 1 holds both generators.
        let depth = SettingId::modulator(3, 0).unwrap();
        let mac5 = SettingId::macro_at(4).unwrap();
        let mut track = Song::new(1).tracks[0].clone();
        track.mods[3].destination = Some(SettingId::CUT);
        let gen_on = |slot: usize| MacroTarget::Gen {
            column: GenColumn::Fx(slot),
            param: crate::shape::GenParam::To,
        };
        track.macros[0].maps = vec![
            MacroMap::numbers(gen_on(0), (0xA0, 0xA0)),
            MacroMap::numbers(gen_on(1), (0xA0, 0xA0)),
        ];
        let steps: Vec<crate::step::Step> = (0..8)
            .map(|_| "C-3 -- --- --- ---".parse().unwrap())
            .collect();
        let mut phrase = Phrase::from_steps(steps);
        for (slot, id) in [(0, depth), (1, mac5)] {
            let column = ShapeColumn::Fx {
                slot,
                command: FxCommand::Set(id),
            };
            let generator = Generator::new(column, Span::Rows { first: 0, last: 7 }, level());
            assert!(phrase.add_generator(generator, None));
        }
        let patch = Patch::of_track(&track, Some(&phrase)).unwrap();
        assert_eq!(patch.mods.len(), 1, "slot 4 comes along");
        assert_eq!(patch.mods[0].destination, Some(SettingId::CUT));
        assert_eq!(patch.macros.len(), 2, "macro 5 comes along");
        let commands: Vec<_> = patch.generators.iter().map(|g| g.command.clone()).collect();
        assert_eq!(
            commands,
            [Some("mod1.depth".to_owned()), Some("macro2".to_owned())]
        );
        assert_eq!(patch.check_alone(), Ok(()));

        // On a track whose macro 1 holds something, slot 1 lands in `Mod 3` and macro 2 in
        // macro 3, and the locks with them.
        let mut other = Song::new(1).tracks[0].clone();
        other.macros[0].maps = vec![MacroMap::whole(MacroTarget::Setting(SettingId::PAN))];
        let placement = patch.place(&other, false).unwrap();
        assert_eq!(
            (placement.mods.as_slice(), placement.macros.as_slice()),
            (&[2][..], &[1, 2][..])
        );
        let placed: Vec<String> = patch
            .generators
            .iter()
            .map(|g| g.fx_command_on(&placement).unwrap().name())
            .collect();
        assert_eq!(placed, ["DEP3", "MAC3"]);

        // A patch file whose generator locks a slot it hasn't is named.
        let mut outside = patch.clone();
        outside.generators[0].command = Some("mod2.depth".to_owned());
        let error = outside.check_alone().unwrap_err();
        assert!(error.contains("mod2.depth"), "{error}");
    }

    #[test]
    fn a_patch_reads_back_and_one_reaching_outside_itself_is_named() {
        let root = std::env::temp_dir().join(format!("vt-patch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let path =
            super::super::preset_path(&root, super::super::PresetKind::Patch, "Turing").unwrap();
        // `Range` bent, so its curve is seen to come along, added and renumbered (decision 314).
        let mut bent = turing();
        bent.macros[2].maps[0] = bent.macros[2].maps[0].bent(-60, 30);
        let preset = PatchPreset::of(&bent, None).unwrap();
        super::super::save(&path, &preset, false).unwrap();
        let loaded = load_patch(&path).unwrap();
        assert_eq!(loaded, preset);
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("cutoff"), "{text}");
        assert!(text.contains("curve: -60"), "{text}");
        assert_eq!(loaded.fixed_tags(), ["patch"]);
        super::super::set_tags(&path, super::super::PresetKind::Patch, vec!["pitch".into()])
            .unwrap();
        assert_eq!(load_patch(&path).unwrap().tags, ["pitch"]);
        let mut track = Song::new(2).tracks[1].clone();
        let placement = loaded.patch.place(&track, false).unwrap();
        loaded.patch.add_to(&mut track, &placement);
        let range = track.macros[2].maps[0];
        assert_eq!(key_of(range.target), "mod3.depth");
        assert_eq!((range.curve, range.s), (-60, 30));
        fs::write(
            &path,
            r#"(format: 1, patch: (macros: [(maps: [(target: Setting("mod2.depth"), min: 0, max: 1)])]))"#,
        )
        .unwrap();
        let error = load_patch(&path).unwrap_err().to_string();
        assert!(error.contains("mod2.depth"), "{error}");
        fs::remove_dir_all(&root).unwrap();
    }
}
