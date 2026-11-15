//! The jump list, after Vim's: the places `Ctrl-O` goes back to and `Ctrl-I` forward again, and
//! `''`, back to where the latest jump left from.

use crate::app::{App, Cursor};
use crate::input::mode::View;
use crate::pages::track::Page;

/// How many places the jump list remembers.
const JUMP_LIMIT: usize = 100;

/// Somewhere `Ctrl-O` can go back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Place {
    view: View,
    cursor: Cursor,
    /// Where on the track page or the mixer, as a jump can land on another of their settings.
    track_page: Page,
    track_row: usize,
    track_field: usize,
    mixer_row: usize,
    /// Where on the sample page, and the entry it showed in place of the track's own, as a jump
    /// can land on another slice.
    sample_cursor: u64,
    sample_pinned: Option<(usize, u8)>,
}

/// The jump list: the places jumped from, where `Ctrl-O` and `Ctrl-I` are in it, and where the
/// latest jump left from.
#[derive(Debug, Default)]
pub struct Jumps {
    pub(crate) places: Vec<Place>,
    pub(crate) index: usize,
    /// Where the latest jump left from, for `''` to go back to, as Vim's previous context mark.
    pub(crate) jumped_from: Option<Place>,
}

impl App {
    fn here(&self) -> Place {
        Place {
            view: self.modes.view,
            cursor: self.services.cursor,
            track_page: self.track_page.page,
            track_row: self.track_page.row,
            track_field: self.track_page.field,
            mixer_row: self.mixer_page.row,
            sample_cursor: self.sample_page.cursor,
            sample_pinned: self.sample_page.pinned,
        }
    }

    /// Remember this place before a jump away from it.
    pub(crate) fn record_jump(&mut self) {
        self.jumps.jumped_from = Some(self.here());
        self.jumps.places.truncate(self.jumps.index);
        self.jumps.places.push(self.here());
        if self.jumps.places.len() > JUMP_LIMIT {
            self.jumps.places.remove(0);
        }
        self.jumps.index = self.jumps.places.len();
    }

    fn restore(&mut self, place: Place) {
        self.modes.view = place.view;
        self.note_view(place.view);
        self.services.cursor = place.cursor;
        self.services.clamp_cursor(self.modes.view);
        if place.view == View::Track {
            self.track_page.show(place.track_page);
            self.move_control(place.track_row, place.track_field);
        }
        if place.view == View::Mixer {
            self.mixer_page.row = place.mixer_row;
        }
        if place.view == View::Sample {
            self.sample_page.pinned = place.sample_pinned;
            if let Ok(shown) = self.sample_page_here() {
                self.sample_page.go(place.sample_cursor, shown.length());
            }
        }
    }

    /// `''`: back to where the latest jump left from, as a jump itself, so a second `''` comes
    /// back again, as Vim's previous context mark does. Whether there was one.
    pub(crate) fn jump_to_previous_context(&mut self) -> bool {
        let Some(place) = self.jumps.jumped_from else {
            return false;
        };
        self.record_jump();
        self.restore(place);
        true
    }

    pub(super) fn jump_back(&mut self) {
        if self.jumps.index == 0 {
            self.services.status = "Nothing to jump back to".to_owned();
            return;
        }
        if self.jumps.index == self.jumps.places.len() {
            // Keep where we are, so `Tab` can come back.
            self.jumps.places.push(self.here());
        }
        self.jumps.index -= 1;
        self.restore(self.jumps.places[self.jumps.index]);
    }

    pub(super) fn jump_forward(&mut self) {
        if self.jumps.index + 1 >= self.jumps.places.len() {
            self.services.status = "Nothing to jump forward to".to_owned();
            return;
        }
        self.jumps.index += 1;
        self.restore(self.jumps.places[self.jumps.index]);
    }
}
