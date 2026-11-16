//! Folding groups: a folded group's tracks are put away into its own column in the song view, its
//! strip on the mixer and a column of the phrase view's overview, as Ableton folds a group track,
//! and an empty group folds away to nothing. The track order doesn't change, so a group's tracks
//! are never moved next to it, only hidden.
//!
//! As in Vim, moves between tracks step over a folded one, a folded group is one stop for `W` and
//! `B`, a cursor a fold closes over stands on its group, and the track picker opens the fold of a
//! track it goes to. Folds are saved with the song, in its `view`, but they're how the screen shows
//! it rather than part of it, so they aren't undone.

use vt_model::constants::MAX_GROUPS;
use vt_model::song::{Song, TrackKind};
use vt_model::view::ViewState;

use crate::input::mode::View;
use crate::widgets::group;

/// Which groups are folded. A group no key has folded or opened is folded while it's empty, so it
/// stays out of the way until a track joins it, and is open once one has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Folds {
    /// Each group's fold as `z a`, `zM` or `zR` last set it, by its number from 1, or none to go
    /// by whether it's empty.
    closed: [Option<bool>; MAX_GROUPS],
}

/// The order tracks are gone through in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Each in its own place, the groups after the sound tracks, as the song view's columns and
    /// the mixer's strips are.
    Columns,
    /// A folded group in its tracks' place, just before where its first one would be, as the
    /// phrase view's overview has it: the order of the views that show one track.
    Stops,
}

impl Order {
    /// The order `W`, `B`, `]g` and `[g` go through the tracks in on a view: the song view's columns
    /// and the mixer's strips as they're drawn, and elsewhere with a folded group in its tracks'
    /// place, as the phrase view's overview draws it.
    pub(crate) fn of(view: View) -> Self {
        match view {
            View::Song | View::Mixer => Self::Columns,
            _ => Self::Stops,
        }
    }
}

impl Folds {
    /// The folds a song was saved with.
    pub(crate) fn from_view(view: &ViewState) -> Self {
        let mut folds = Self::default();
        for &number in &view.opened {
            folds.set(number, false);
        }
        for &number in &view.folded {
            folds.set(number, true);
        }
        folds
    }

    /// The folds to save with the song: those a key or [`App::fold_group`] has set.
    pub fn view(&self) -> ViewState {
        let set = |closed: bool| -> Vec<u8> {
            (1..)
                .zip(self.closed)
                .filter(|&(_, fold)| fold == Some(closed))
                .map(|(number, _)| number)
                .collect()
        };
        ViewState {
            folded: set(true),
            opened: set(false),
        }
    }

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

    /// The tracks shown, each in its own place.
    pub fn shown<'a>(&'a self, song: &'a Song) -> impl Iterator<Item = usize> + 'a {
        (0..song.tracks.len()).filter(|&track| !self.hides(song, track))
    }

    /// Where a track comes in an order, to sort by: its own place, or in [`Order::Stops`] a folded
    /// group's just before its first track's.
    pub fn place(&self, song: &Song, track: usize, order: Order) -> (usize, bool) {
        let first = (order == Order::Stops && self.holds(song, track))
            .then(|| song.group_members(track).next())
            .flatten();
        match first {
            Some(first) => (first, false),
            None => (track, true),
        }
    }

    /// The tracks shown, in an order.
    pub(crate) fn ordered(&self, song: &Song, order: Order) -> Vec<usize> {
        let mut tracks: Vec<usize> = self.shown(song).collect();
        tracks.sort_by_key(|&track| self.place(song, track, order));
        tracks
    }

    /// The track as many shown tracks along from this one as `by` in an order, stopping at the
    /// ends, so a move steps over the folded ones and makes a folded group one stop.
    pub fn step(&self, song: &Song, from: usize, by: isize, order: Order) -> usize {
        let shown = self.ordered(song, order);
        if by == 0 {
            return from;
        }
        let Some(&last) = shown.last() else {
            return from;
        };
        // From a folded track, the first shown after it counts as the first step forward.
        let at = self.position(song, &shown, from, order);
        let to = if by >= 0 {
            let past = usize::from(shown.get(at) != Some(&from));
            at.saturating_add(by as usize).saturating_sub(past)
        } else {
            at.saturating_sub(by.unsigned_abs())
        };
        shown.get(to).copied().unwrap_or(last)
    }

    /// The group as many groups along from this track as `by` in an order, folded or open, or none
    /// if there aren't that many that way. Empty groups folded away aren't counted.
    pub(crate) fn step_to_group(
        &self,
        song: &Song,
        from: usize,
        by: isize,
        order: Order,
    ) -> Option<usize> {
        let shown = self.ordered(song, order);
        let at = self.position(song, &shown, from, order);
        let times = by.unsigned_abs().checked_sub(1)?;
        let group = |track: &&usize| **track != from && song.tracks[**track].is_group();
        if by > 0 {
            shown[at..].iter().filter(group).nth(times).copied()
        } else {
            shown[..at].iter().rev().filter(group).nth(times).copied()
        }
    }

    /// Where a track is among those shown in an order, or for one folded away, where the first
    /// after it is.
    fn position(&self, song: &Song, shown: &[usize], track: usize, order: Order) -> usize {
        let here = self.place(song, track, order);
        shown.partition_point(|&other| self.place(song, other, order) < here)
    }

    /// Where a cursor on a folded track stands: on its group, as Vim's stands on a closed fold, or
    /// for an empty group folded away, the nearest track shown after it, or else before it.
    pub(crate) fn stand_in(&self, song: &Song, track: usize) -> usize {
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
    pub(crate) fn open_over(&mut self, song: &Song, track: usize) {
        if self.hides(song, track)
            && let Some(number) = group::number(song, track)
        {
            self.set(number, false);
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
        let columns = |from, by| folds.step(&song, from, by, Order::Columns);
        assert_eq!(columns(3, -1), 3, "01 to 03 are folded");
        assert_eq!(columns(3, 1), 4);
        // From 08, past the empty groups G3 and G4.
        let g2 = group(&song, 2);
        assert_eq!(columns(7, 2), g2);
        assert_eq!(columns(g2, 1), g2 + 3, "to A");
        // From a folded track, a step forward is to the next shown.
        assert_eq!(columns(1, 1), 3);
        assert_eq!(columns(1, -1), 3);
        assert_eq!(columns(0, 100), song.tracks.len() - 1);
    }

    #[test]
    fn a_folded_group_is_one_stop_in_its_tracks_place() {
        let song = song();
        let mut folds = Folds::default();
        folds.set(1, true);
        let (g1, g2) = (group(&song, 1), group(&song, 2));
        let stops = folds.ordered(&song, Order::Stops);
        assert_eq!(stops[..3], [g1, 3, 4]);
        assert_eq!(stops[stops.len() - 4..], [g2, g2 + 3, g2 + 4, g2 + 5]);
        let step = |from, by| folds.step(&song, from, by, Order::Stops);
        assert_eq!(
            step(g1, 1),
            3,
            "from the group to the track after its tracks"
        );
        assert_eq!(step(3, -1), g1);
        assert_eq!(step(7, 1), g2, "G1 isn't come to again after the tracks");
        // From a track folded into it, back to the group, or on past it.
        assert_eq!(step(1, -1), g1);
        assert_eq!(step(1, 1), 3);
        // Groups, folded or open.
        let to_group = |from, by| folds.step_to_group(&song, from, by, Order::Stops);
        assert_eq!(to_group(3, 1), Some(g2));
        assert_eq!(to_group(3, -1), Some(g1));
        assert_eq!(to_group(g1, 1), Some(g2));
        assert_eq!(to_group(g1, 2), None, "G3 and G4 are folded away");
        assert_eq!(to_group(g2, -1), Some(g1));
    }

    #[test]
    fn folds_are_saved_as_set_and_read_back() {
        let song = song();
        let mut folds = Folds::default();
        assert!(
            folds.view().is_empty(),
            "nothing is kept until a fold is set"
        );
        folds.set(1, true);
        folds.set(3, false);
        let view = folds.view();
        assert_eq!(
            (view.folded.as_slice(), view.opened.as_slice()),
            (&[1][..], &[3][..])
        );
        let back = Folds::from_view(&view);
        assert_eq!(back, folds);
        assert!(back.hides(&song, 0) && !back.hides(&song, group(&song, 3)));
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
