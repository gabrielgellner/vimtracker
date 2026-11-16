//! Edits to tracks, and how each one plays and sits in the mix.

use crate::constants::{FX_SLOTS, MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS, MAX_TRACK_RESET};
use crate::label::is_group_name;
use crate::lane::LANE_RATES;
use crate::song::Song;

use super::{Action, EditError, check_choke, track_mut};

/// Apply an edit to tracks, and how each one plays and sits in the mix.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetTrackGroove { track, groove } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .groove = *groove;
        }
        Action::SetTrackBase {
            track,
            control,
            value,
        } => {
            let track = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            track.set_control(*control, *value);
            // At the top, a fine part goes.
            if let Some(id) = control.setting() {
                track.set_fine(id, track.fine(id));
            }
        }
        Action::SetTrackLevel { track, level } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .level = (*level).min(MAX_LEVEL);
        }
        Action::SetTrackTrim { track, trim } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .trim = *trim;
        }
        Action::SetTrackRole { track, role } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .role = *role;
        }
        Action::SetTrackChoke { track, choke } => {
            check_choke(*choke)?;
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .choke = *choke;
        }
        Action::SetTrackGroup { first, last, group } => {
            if let Some(group) = *group
                && !(1..=MAX_GROUPS).contains(&usize::from(group))
            {
                return Err(EditError::Group(group));
            }
            if first > last || *last >= next.sound_tracks() {
                return Err(EditError::GroupMember);
            }
            for track in *first..=*last {
                next.tracks[track].group = *group;
            }
        }
        Action::SetTrackGateLength { track, gate_length } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .gate_length = *gate_length;
        }
        Action::SetTrackSpeed { track, speed } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .speed = *speed;
        }
        Action::SetTrackPhase { track, phase } => {
            track_mut(next, *track)?.phase = *phase;
        }
        Action::SetTrackVoiceMode { track, mode } => {
            track_mut(next, *track)?.voice_mode = *mode;
        }
        Action::SetTrackGlideMode { track, mode } => {
            track_mut(next, *track)?.glide_mode = *mode;
        }
        Action::SetTrackReset { track, reset } => {
            if reset.is_some_and(|steps| !(1..=MAX_TRACK_RESET).contains(&steps)) {
                return Err(EditError::TrackReset);
            }
            track_mut(next, *track)?.reset = *reset;
        }
        Action::SetTrackGlide { track, glide } => {
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .glide = *glide;
        }
        Action::SetTrackSafe { track, on } => track_mut(next, *track)?.safe = *on,
        Action::SetAccumulation {
            track,
            accumulation,
        } => {
            if !accumulation.in_range() {
                return Err(EditError::Accumulation);
            }
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .accumulation = *accumulation;
        }
        Action::SetArpeggio { track, arpeggio } => {
            if !arpeggio.in_range() {
                return Err(EditError::Arpeggio);
            }
            next.tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?
                .arpeggio = *arpeggio;
        }
        Action::SetLaneMove { track, slot, moved } => {
            if *slot >= FX_SLOTS || usize::from(moved.rate) >= LANE_RATES.len() {
                return Err(EditError::LaneMove);
            }
            track_mut(next, *track)?.lanes[*slot] = *moved;
        }
        Action::AddTracks(count) => {
            if next.sound_tracks() + count > MAX_SOUND_TRACKS {
                return Err(EditError::TooManyTracks);
            }
            for _ in 0..*count {
                next.add_track();
            }
        }
        Action::RemoveLastTrack => {
            if next.sound_tracks() <= 1 {
                return Err(EditError::LastTrack);
            }
            next.remove_last_sound_track();
        }
        Action::SetTrackName { track, name } => {
            let owner = next
                .tracks
                .get_mut(*track)
                .ok_or(EditError::NoTrack(*track))?;
            if owner.is_group() && !name.is_empty() && !is_group_name(name) {
                return Err(EditError::GroupName);
            }
            // A fixed track without a name goes back to its letter.
            owner.name = if name.is_empty() {
                owner.kind.letter()
            } else {
                name.clone()
            };
        }
        _ => unreachable!("not an edit to tracks, and how each one plays and sits in the mix"),
    }
    Ok(())
}
