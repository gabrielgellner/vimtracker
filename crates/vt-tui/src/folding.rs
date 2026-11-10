//! Folding groups (decision 128): a folded group's tracks are put away into its own column in the
//! song view, its strip on the mixer and a column of the phrase view's overview, as Ableton folds a
//! group track, and an empty group folds away to nothing. The track order doesn't change, so a
//! group's tracks are never moved next to it, only hidden.
//!
//! As in Vim, moves between tracks step over a folded one, a cursor a fold closes over stands on
//! its group, and the track picker opens the fold of a track it goes to. Folds are how the screen
//! shows the song rather than part of it, so they're not saved, and last the session.

use std::sync::Arc;

use vt_model::constants::MAX_GROUPS;
use vt_model::song::{Song, TrackKind};

use crate::app::App;
use crate::input::mode::View;
use crate::song_view::COLUMNS_BEFORE_TRACKS;
use crate::views::widgets::group;

/// Which groups are folded. A group no key has folded or opened is folded while it's empty, so it
/// stays out of the way until a track joins it, and is open once one has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Folds {
    /// Each group's fold as `z a`, `zM` or `zR` last set it, by its number from 1, or none to go
    /// by whether it's empty.
    closed: [Option<bool>; MAX_GROUPS],
}

impl Folds {
    /// Whether a group, by its number from 1, is folded.
    pub fn is_closed(&self, song: &Song, number: u8) -> bool {
        let set = usize::from(number)
            .checked_sub(1)
            .and_then(|at| self.closed.get(at).copied().flatten());
        set.unwrap_or_else(|| {
            song.fixed_track(TrackKind::Group(number))
                .is_some_and(|track| song.group_members(track).next().is_none())
        })
    }

    /// Whether a track is out of sight: a sound track in a folded group, or a folded group with no
    /// tracks in it, which has nothing to show.
    pub fn hides(&self, song: &Song, track: usize) -> bool {
        let Some(number) = group::number(song, track) else {
            return false;
        };
        self.is_closed(song, number)
            && (song.tracks[track].is_sound() || song.group_members(track).next().is_none())
    }

    /// Whether a track is a group whose tracks are folded into it.
    pub fn holds(&self, song: &Song, track: usize) -> bool {
        song.tracks.get(track).is_some_and(|this| this.is_group())
            && !self.hides(song, track)
            && group::number(song, track).is_some_and(|number| self.is_closed(song, number))
    }

    /// The tracks shown, in order.
    pub fn shown<'a>(&'a self, song: &'a Song) -> impl Iterator<Item = usize> + 'a {
        (0..song.tracks.len()).filter(|&track| !self.hides(song, track))
    }

    /// The track as many shown tracks along from this one as `by`, stopping at the ends, so a move
    /// steps over the folded ones.
    pub fn step(&self, song: &Song, from: usize, by: isize) -> usize {
        let shown: Vec<usize> = self.shown(song).collect();
        if by == 0 {
            return from;
        }
        let Some(&last) = shown.last() else {
            return from;
        };
        // From a folded track, the first shown after it counts as the first step forward.
        let at = shown.partition_point(|&track| track < from);
        let to = if by >= 0 {
            let past = usize::from(shown.get(at) != Some(&from));
            at.saturating_add(by as usize).saturating_sub(past)
        } else {
            at.saturating_sub(by.unsigned_abs())
        };
        shown.get(to).copied().unwrap_or(last)
    }

    /// Where a cursor on a folded track stands: on its group, as Vim's stands on a closed fold, or
    /// for an empty group folded away, the nearest track shown after it, or else before it.
    pub fn stand_in(&self, song: &Song, track: usize) -> usize {
        if !self.hides(song, track) {
            return track;
        }
        if let Some(group) = song.group_track_of(track)
            && !self.hides(song, group)
        {
            return group;
        }
        let after = (track..song.tracks.len()).find(|&next| !self.hides(song, next));
        let before = (0..track).rev().find(|&next| !self.hides(song, next));
        after.or(before).unwrap_or(track)
    }

    /// Fold a group or open it.
    pub fn set(&mut self, number: u8, closed: bool) {
        if let Some(fold) = usize::from(number)
            .checked_sub(1)
            .and_then(|at| self.closed.get_mut(at))
        {
            *fold = Some(closed);
        }
    }

    /// Fold every group, or open every one, the empty ones too.
    pub fn set_all(&mut self, closed: bool) {
        self.closed = [Some(closed); MAX_GROUPS];
    }

    /// Open the fold a track is in, so it's shown.
    pub fn open_over(&mut self, song: &Song, track: usize) {
        if self.hides(song, track)
            && let Some(number) = group::number(song, track)
        {
            self.set(number, false);
        }
    }
}

impl App {
    /// The track the cursor is on in a view that folds: the song view's column, or the mixer's
    /// strip.
    fn folding_track(&self) -> Option<usize> {
        match self.view {
            View::Song => self.song_column.checked_sub(COLUMNS_BEFORE_TRACKS),
            View::Mixer => Some(self.cursor.track),
            _ => None,
        }
    }

    /// `z a` in the song view or the mixer: fold the group the cursor is on, or the one its track
    /// is in, or open it, as Vim's `za` toggles the fold the cursor is in.
    pub(crate) fn toggle_fold(&mut self) {
        self.count = None;
        let Some(track) = self.folding_track() else {
            self.status =
                "z a folds a group from its column, or from one of its tracks'".to_owned();
            return;
        };
        let song = Arc::clone(self.song());
        let Some(number) = group::number(&song, track) else {
            let name = match song.tracks[track].kind {
                TrackKind::Sound => format!("Track {:02}", track + 1),
                kind => kind.title(),
            };
            self.status = format!("{name} isn't in a group to fold");
            return;
        };
        let closed = !self.folds.is_closed(&song, number);
        self.folds.set(number, closed);
        self.stand_on_shown();
        let members: Vec<usize> = song
            .fixed_track(TrackKind::Group(number))
            .map_or_else(Vec::new, |at| song.group_members(at).collect());
        self.status = match (closed, members.is_empty()) {
            (true, true) => format!("G{number} is empty, and folded away. zR shows it"),
            (true, false) => format!(
                "G{number} folded: {} put away in it. z a opens it",
                group::runs(&members)
            ),
            (false, _) => format!("G{number} open"),
        };
    }

    /// `zM` and `zR`: fold every group, or open every one, as Vim's close and open every fold.
    pub(crate) fn fold_all(&mut self, closed: bool) {
        self.count = None;
        self.folds.set_all(closed);
        self.stand_on_shown();
        self.status = if closed {
            "Every group folded, and the empty ones put away. zR opens them".to_owned()
        } else {
            "Every group open, the empty ones too. zM folds them".to_owned()
        };
    }

    /// Put the cursor of the view in front, if it folds, on what stands in for a folded track it's
    /// on: the song view's column, or the mixer's strip. The other views' track is left where it
    /// is, so a phrase being edited stays in front of the phrase view.
    pub(crate) fn stand_on_shown(&mut self) {
        let Some(track) = self.folding_track() else {
            return;
        };
        let to = self.folds.stand_in(self.song(), track);
        match self.view {
            View::Song => self.song_column = COLUMNS_BEFORE_TRACKS + to,
            _ if to != track => self.go_to_track_at(to),
            _ => {}
        }
    }

    /// After a key, in the song view and the mixer, keep the cursor off a folded track: one that a
    /// track joining a folded group, an undo or a jump has left there stands on its group.
    pub(crate) fn keep_cursor_shown(&mut self) {
        let Some(track) = self.folding_track() else {
            return;
        };
        if self.folds.hides(self.song(), track) {
            self.stand_on_shown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A song of eight sound tracks, 01 to 03 in group 1 and 05 in group 2.
    fn song() -> Song {
        let mut song = Song::new(8);
        for track in [0, 1, 2] {
            song.tracks[track].group = Some(1);
        }
        song.tracks[4].group = Some(2);
        song
    }

    fn group(song: &Song, number: u8) -> usize {
        song.fixed_track(TrackKind::Group(number)).unwrap()
    }

    #[test]
    fn an_empty_group_is_folded_away_and_one_with_tracks_is_open() {
        let song = song();
        let folds = Folds::default();
        assert!(!folds.is_closed(&song, 1));
        assert!(folds.is_closed(&song, 3));
        assert!(!folds.hides(&song, 0));
        assert!(!folds.hides(&song, group(&song, 1)));
        assert!(folds.hides(&song, group(&song, 3)));
        assert!(!folds.holds(&song, group(&song, 3)));
    }

    #[test]
    fn a_folded_group_hides_its_tracks_and_holds_them() {
        let song = song();
        let mut folds = Folds::default();
        folds.set(1, true);
        assert!(folds.hides(&song, 0) && folds.hides(&song, 2));
        assert!(!folds.hides(&song, 3) && !folds.hides(&song, 4));
        assert!(folds.holds(&song, group(&song, 1)));
        assert!(!folds.holds(&song, group(&song, 2)));
        // A fold opened shows an empty group too.
        folds.set_all(false);
        assert!(!folds.hides(&song, group(&song, 3)));
    }

    #[test]
    fn a_step_goes_over_folded_tracks() {
        let song = song();
        let mut folds = Folds::default();
        folds.set(1, true);
        assert_eq!(folds.step(&song, 3, -1), 3, "01 to 03 are folded");
        assert_eq!(folds.step(&song, 3, 1), 4);
        // From 08, past the empty groups G3 and G4.
        let g2 = group(&song, 2);
        assert_eq!(folds.step(&song, 7, 2), g2);
        assert_eq!(folds.step(&song, g2, 1), g2 + 3, "to A");
        // From a folded track, a step forward is to the next shown.
        assert_eq!(folds.step(&song, 1, 1), 3);
        assert_eq!(folds.step(&song, 1, -1), 3);
        assert_eq!(folds.step(&song, 0, 100), song.tracks.len() - 1);
    }

    #[test]
    fn a_cursor_on_a_folded_track_stands_on_its_group_or_the_next_shown() {
        let song = song();
        let mut folds = Folds::default();
        folds.set(1, true);
        assert_eq!(folds.stand_in(&song, 1), group(&song, 1));
        assert_eq!(folds.stand_in(&song, 3), 3);
        assert_eq!(folds.stand_in(&song, group(&song, 3)), group(&song, 4) + 1);
        folds.open_over(&song, 1);
        assert!(!folds.hides(&song, 1));
    }
}
