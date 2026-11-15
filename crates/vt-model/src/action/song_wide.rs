//! Edits to the settings of the whole song.

use crate::constants::{MAX_MILLIBPM, MIN_MILLIBPM};
use crate::groove::{CUSTOM_GROOVES, Groove};
use crate::names::is_project_name;
use crate::song::Song;

use super::{Action, EditError};

/// Apply an edit to the settings of the whole song.
pub(super) fn edit(next: &mut Song, action: &Action) -> Result<(), EditError> {
    match action {
        Action::SetSkipSize(size) => next.skip = *size,
        Action::SetLimiter(on) => next.limiter = *on,
        Action::SetSongName(name) => {
            if !is_project_name(name) {
                return Err(EditError::SongName);
            }
            next.name.clone_from(name);
        }
        Action::SetSeed(seed) => next.seed = *seed,
        Action::SetLaunch(launch) => next.launch = *launch,
        Action::SetGroove(groove) => next.groove = *groove,
        Action::SetCustomGroove { custom, groove } => {
            if *custom >= CUSTOM_GROOVES {
                return Err(EditError::CustomGroove);
            }
            // The ones before it are made too, on the grid, as they'd have played.
            if next.grooves.len() <= *custom {
                next.grooves.resize(*custom + 1, Groove::default());
            }
            next.grooves[*custom] = *groove;
        }
        Action::SetMilliBpm(millibpm) => {
            if !(MIN_MILLIBPM..=MAX_MILLIBPM).contains(millibpm) {
                return Err(EditError::Tempo);
            }
            next.millibpm = *millibpm;
        }
        _ => unreachable!("not an edit to the settings of the whole song"),
    }
    Ok(())
}
