//! Edits to a track's sound: its machine, settings, inserts, mods and macros.

use crate::insert::Insert;
use crate::macros::{MACROS, valid_name};
use crate::settings::Owner;
use crate::song::Song;

use super::{Action, EditError, track_mut};

/// Apply an edit to a track's sound: its machine, settings, inserts, mods and macros.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetInsert {
            track,
            slot,
            insert,
        } => {
            if let Some(held) = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .inserts
                .get_mut(*slot)
            {
                *held = *insert;
            }
            // An empty slot has nothing to switch out, so what goes in next starts in.
            if *insert == Insert::Empty
                && let Some(bypassed) = next.tracks[*track].bypassed.get_mut(*slot)
            {
                *bypassed = false;
            }
        }
        Action::LoadSound {
            track,
            preset,
            sample,
        } => {
            let owner = track_mut(next, *track)?;
            if !owner.is_sound() {
                return Err(EditError::NotSound);
            }
            preset.apply(owner, *sample);
        }
        Action::LoadPatch {
            track,
            patch,
            placement,
        } => {
            let owner = track_mut(next, *track)?;
            if !owner.is_sound() {
                return Err(EditError::NotSound);
            }
            patch.add_to(owner, placement);
        }
        Action::LoadEffect {
            track,
            slot,
            insert,
            fine,
        } => {
            let owner = track_mut(next, *track)?;
            if let Some(held) = owner.inserts.get_mut(*slot) {
                *held = *insert;
                owner.fine.retain(
                    |id, _| !matches!(id.owner().0, Owner::Slot { slot: at, .. } if at == *slot),
                );
                for (id, value) in fine {
                    owner.set_fine(*id, *value);
                }
            }
        }
        Action::SetBypass {
            track,
            slot,
            bypassed,
        } => {
            if let Some(held) = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .bypassed
                .get_mut(*slot)
            {
                *held = *bypassed;
            }
        }
        Action::SetMod {
            track,
            slot,
            modulator,
        } => {
            if !modulator.in_range() {
                return Err(EditError::Mod);
            }
            *next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .mods
                .get_mut(*slot)
                .ok_or(EditError::Mod)? = *modulator;
        }
        Action::SetMacro { track, at, value } => {
            let held = track_mut(next, *track)?
                .macros
                .get_mut(*at)
                .ok_or(EditError::Macro)?;
            held.value = (*value).min(0xFF00);
        }
        Action::MapMacro { track, at, map } => {
            if *at >= MACROS || !map.in_range() {
                return Err(EditError::Macro);
            }
            let macros = &mut track_mut(next, *track)?.macros;
            for (place, held) in macros.iter_mut().enumerate() {
                match held.map_of(map.target) {
                    Some(found) if place == *at => held.maps[found] = *map,
                    Some(found) => {
                        held.maps.remove(found);
                    }
                    None if place == *at => held.maps.push(*map),
                    None => {}
                }
            }
        }
        Action::UnmapMacro { track, target } => {
            for held in &mut track_mut(next, *track)?.macros {
                held.maps.retain(|map| map.target != *target);
            }
        }
        Action::NameMacro { track, at, name } => {
            if name.as_deref().is_some_and(|name| !valid_name(name)) {
                return Err(EditError::Macro);
            }
            track_mut(next, *track)?
                .macros
                .get_mut(*at)
                .ok_or(EditError::Macro)?
                .name
                .clone_from(name);
        }
        Action::SetTrackMachine { track, machine } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .machine = *machine;
        }
        Action::SetTrackSetting {
            track,
            setting,
            value,
        } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .set_setting(*setting, *value);
            let track = &mut next.tracks[*track];
            track.set_fine(*setting, track.fine(*setting));
        }
        Action::SetTrackFine {
            track,
            setting,
            fine,
        } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .set_fine(*setting, *fine);
        }
        Action::SetFilterType { track, filter } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .filter = *filter;
        }
        Action::SetFilterMode { track, mode } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .filter_mode = *mode;
        }
        _ => unreachable!(
            "not an edit to a track's sound: its machine, settings, inserts, mods and macros"
        ),
    }
    Ok(())
}
