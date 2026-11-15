//! Edits to the sample bank, and the samples tracks play from it.

use crate::bank::Points;
use crate::constants::MAX_SOUND_TRACKS;
use crate::settings::SettingId;
use crate::song::{Machine, Song};

use super::{Action, EditError, check_choke};

/// Apply an edit to the sample bank, and the samples tracks play from it.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetTrackSample { track, sample } => {
            let owner = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            owner.set_setting(SettingId::SAMPLE, *sample);
            // Choosing a sample means wanting to hear it.
            if *sample != 0 {
                owner.machine = Machine::Sampler;
            }
        }
        Action::SetBankEntry { number, entry } => {
            next.bank.set(*number, entry.clone());
        }
        Action::SetSlices { number, starts } => {
            reslice(next, *number, |_| Ok(starts.clone()))?;
        }
        Action::SplitSlice { number, at } => reslice(next, *number, |points| {
            points.split_starts(*at).ok_or(EditError::Split)
        })?,
        Action::JoinSlices { number, slice } => reslice(next, *number, |points| {
            points
                .join_starts(usize::from(*slice))
                .ok_or(EditError::Join(*slice))
        })?,
        Action::LoadKit(sounds) => {
            if sounds.len() > MAX_SOUND_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            while next.sound_tracks() < sounds.len() {
                next.add_track();
            }
            for (track, sound) in next.tracks.iter_mut().zip(sounds) {
                check_choke(sound.choke)?;
                track.set_setting(SettingId::SAMPLE, sound.sample);
                track.machine = Machine::Sampler;
                track.name = sound.name.clone();
                track.choke = sound.choke;
            }
        }
        _ => unreachable!("not an edit to the sample bank, and the samples tracks play from it"),
    }
    Ok(())
}

/// Cut a bank entry into slices starting where `starts` says from its points, moving its marks
/// and the steps' `SLI` locks with their sound, as one change.
fn reslice(
    song: &mut Song,
    number: u8,
    starts: impl FnOnce(&Points) -> Result<Vec<u64>, EditError>,
) -> Result<(), EditError> {
    let mut entry = song
        .bank
        .get(number)
        .cloned()
        .ok_or(EditError::NoSample(number))?;
    let starts = starts(&entry.edit.points)?;
    let renumbered = entry.edit.points.reslice(&starts);
    song.bank.set(number, Some(entry));
    if let Some(renumbered) = renumbered {
        song.renumber_slices(number, &renumbered);
    }
    Ok(())
}
