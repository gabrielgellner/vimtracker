//! The jump list, after Vim's: the places `Ctrl-O` goes back to and `Ctrl-I` forward again, and
//! `''`, back to where the latest jump left from.

use crate::app::{App, Cursor};
use crate::input::mode::View;
use crate::pages::track::Page;

/// How many places the jump list remembers.
const JUMP_LIMIT: usize = 100;

/// Somewhere `Ctrl-O` can go back to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
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
        self.jumped_from = Some(self.here());
        self.jumps.truncate(self.jump_index);
        self.jumps.push(self.here());
        if self.jumps.len() > JUMP_LIMIT {
            self.jumps.remove(0);
        }
        self.jump_index = self.jumps.len();
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
        let Some(place) = self.jumped_from else {
            return false;
        };
        self.record_jump();
        self.restore(place);
        true
    }

    pub(super) fn jump_back(&mut self) {
        if self.jump_index == 0 {
            self.services.status = "Nothing to jump back to".to_owned();
            return;
        }
        if self.jump_index == self.jumps.len() {
            // Keep where we are, so `Tab` can come back.
            self.jumps.push(self.here());
        }
        self.jump_index -= 1;
        self.restore(self.jumps[self.jump_index]);
    }

    pub(super) fn jump_forward(&mut self) {
        if self.jump_index + 1 >= self.jumps.len() {
            self.services.status = "Nothing to jump forward to".to_owned();
            return;
        }
        self.jump_index += 1;
        self.restore(self.jumps[self.jump_index]);
    }
}
