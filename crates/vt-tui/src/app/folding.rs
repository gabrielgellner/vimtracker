//! Folding a group's tracks away, `za`, `zM` and `zR`, and keeping the cursor on a track
//! that shows. It's `App`'s, as the song view's column and the view decide which track
//! folds; which groups are folded is the services' [`crate::services::folding::Folds`].

use std::sync::Arc;

use vt_model::song::TrackKind;

use crate::app::App;
use crate::input::mode::View;
use crate::pages::song::COLUMNS_BEFORE_TRACKS;
use crate::services::folding::{Folds, Order};
use crate::widgets::group;

impl App {
    /// The track the cursor is on, in a view with folding keys: the song view's column, or the
    /// track the others are on.
    fn folding_track(&self) -> Option<usize> {
        match self.modes.view {
            View::Song => self.song_view.column.checked_sub(COLUMNS_BEFORE_TRACKS),
            View::Mixer | View::Phrase | View::Chain | View::Track => {
                Some(self.services.cursor.track)
            }
            View::Audio | View::Project | View::Sample => None,
        }
    }

    /// Fold a group, by its number from 1, or open it, as a key would but saying nothing: for
    /// whatever makes a group, as loading a kit does. The fold is kept with the song, so it marks
    /// it changed, and it isn't an undo step. A song view or mixer cursor it closes over stands on
    /// the group.
    pub fn fold_group(&mut self, number: u8, closed: bool) {
        let was = self.services.folds;
        self.services.folds.set(number, closed);
        self.folds_changed(was);
        self.keep_cursor_shown();
    }

    /// Mark the song changed if the folds are, as they're saved with it.
    pub(crate) fn folds_changed(&mut self, was: Folds) {
        if self.services.folds != was {
            self.services.dirty = true;
        }
    }

    /// `z a`: fold the group the cursor is on, or the one its track is in, or open it, as Vim's
    /// `za` toggles the fold the cursor is in.
    pub(crate) fn toggle_fold(&mut self) {
        self.modes.count = None;
        let Some(track) = self.folding_track() else {
            self.services.status =
                "z a folds a group from its column, or from one of its tracks'".to_owned();
            return;
        };
        let song = Arc::clone(self.song());
        let Some(number) = group::number(&song, track) else {
            let name = match song.tracks[track].kind {
                TrackKind::Sound => format!("Track {:02}", track + 1),
                kind => kind.title(),
            };
            self.services.status = format!("{name} isn't in a group to fold");
            return;
        };
        let was = self.services.folds;
        let closed = !self.services.folds.is_closed(&song, number);
        self.services.folds.set(number, closed);
        self.folds_changed(was);
        self.stand_on_shown();
        let members: Vec<usize> = song
            .fixed_track(TrackKind::Group(number))
            .map_or_else(Vec::new, |at| song.group_members(at).collect());
        self.services.status = match (closed, members.is_empty()) {
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
        self.modes.count = None;
        let was = self.services.folds;
        self.services.folds.set_all(closed);
        self.folds_changed(was);
        self.stand_on_shown();
        self.services.status = if closed {
            "Every group folded, and the empty ones put away. zR opens them".to_owned()
        } else {
            "Every group open, the empty ones too. zM folds them".to_owned()
        };
    }

    /// `]g` and `[g`: go to the next or previous group, folded or open, as many along as `by`.
    pub(crate) fn move_to_group(&mut self, by: isize) {
        let times = self.modes.count.take().unwrap_or(1).max(1) as isize;
        let order = Order::of(self.modes.view);
        // The song view's cursor is its column, which may be one before the tracks'.
        let from = match self.modes.view {
            View::Song => self.folding_track().unwrap_or(0),
            _ => self.services.cursor.track,
        };
        match self
            .services
            .folds
            .step_to_group(self.song(), from, by * times, order)
        {
            Some(group) if self.modes.view == View::Song => {
                self.song_view.column = COLUMNS_BEFORE_TRACKS + group;
            }
            Some(group) => self.services.go_to_track_at(group, self.modes.view),
            None if by > 0 => self.services.status = "No group after this track".to_owned(),
            None => self.services.status = "No group before this track".to_owned(),
        }
    }

    /// Put the cursor, if it's on a folded track, on what stands in for it: the song view's
    /// column, or the track the other views are on. Only the view in front moves, so a phrase
    /// being edited stays in front of the phrase view when its group is folded on the mixer.
    pub(crate) fn stand_on_shown(&mut self) {
        let Some(track) = self.folding_track() else {
            return;
        };
        let to = self.services.folds.stand_in(self.song(), track);
        match self.modes.view {
            View::Song => self.song_view.column = COLUMNS_BEFORE_TRACKS + to,
            _ if to != track => self.services.go_to_track_at(to, self.modes.view),
            _ => {}
        }
    }

    /// After a key, in the song view and the mixer, keep the cursor off a folded track: one that a
    /// track joining a folded group, an undo or a jump has left there stands on its group. The
    /// other views show a folded track's phrase or page until the cursor leaves it.
    pub(crate) fn keep_cursor_shown(&mut self) {
        if !matches!(self.modes.view, View::Song | View::Mixer) {
            return;
        }
        let Some(track) = self.folding_track() else {
            return;
        };
        if self.services.folds.hides(self.song(), track) {
            self.stand_on_shown();
        }
    }
}
