//! The jump list, after Vim's: the places `Ctrl-O` goes back to and `Ctrl-I` forward again, and
//! `''`, back to where the latest jump left from.

use ratatui::crossterm::event::KeyCode;
use vt_model::bank::is_mark;

use crate::app::App;
use crate::controls::{Control, Page};
use crate::input::keys::Key;
use crate::input::mode::View;
use crate::overlays::mapping::MappingView;
use crate::pages::sample::marking::MarkWait;
use crate::services::Cursor;
use crate::services::banking::sample_text;

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
            self.track_page.move_to(
                self.services.track_here(),
                place.track_row,
                place.track_field,
            );
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

    /// The key after `'` or `m` on the sample page.
    pub(crate) fn mark_key(&mut self, wait: MarkWait, key: Key) {
        let KeyCode::Char(c) = key.code else {
            self.services.status = "Marks are a to z: :marks lists them".to_owned();
            return;
        };
        if key.ctrl || key.alt {
            self.services.status = "Marks are a to z: :marks lists them".to_owned();
            return;
        }
        match (wait, c) {
            (MarkWait::Name, c) if is_mark(c) => {
                self.sample_page
                    .set_mark(c, None, self.modes.view, &mut self.services);
            }
            (MarkWait::Name, _) => {
                self.services.status =
                    "m takes a letter, a to z, to name the slice with".to_owned();
            }
            (MarkWait::Jump, c) if is_mark(c) => self.jump_to_mark(c),
            (MarkWait::Jump, '\'') => {
                if !self.jump_to_previous_context() {
                    self.services.status = "No jump to go back from yet".to_owned();
                }
            }
            (MarkWait::Jump, '<' | '>') => self.jump_to_selection_end(c == '>'),
            (MarkWait::Jump, _) => {
                self.services.status =
                    "Marks are a to z, '< '> the last selection and '' the last jump".to_owned();
            }
        }
    }

    /// `'<` and `'>`: the first or last frame of the last selection, as a jump. As in Vim, they're
    /// the selection made last, not one being made.
    pub(crate) fn jump_to_selection_end(&mut self, last: bool) {
        let Some((first, end)) = self.sample_page.last_selection else {
            self.services.status = "No selection yet: v starts one".to_owned();
            return;
        };
        let Ok(shown) = self.sample_page_here() else {
            return;
        };
        self.record_jump();
        let frame = if last { end } else { first };
        self.sample_page.go(frame, shown.length());
    }

    /// `'a`: the sample page on the slice a letter names, as a jump `C-o` comes back from.
    pub(crate) fn jump_to_mark(&mut self, letter: char) {
        let number = self
            .sample_page
            .marked_entry(self.modes.view, &self.services);
        let marked = self
            .song()
            .bank
            .get(number)
            .and_then(|entry| entry.edit.points.marked(letter));
        let Some(slice) = marked else {
            self.services.status = format!(
                "No slice of {} is named {letter}: :marks lists them",
                sample_text(self.song(), number)
            );
            return;
        };
        self.go_to_slice(number, slice);
        self.services.status = format!("'{letter}: slice {slice:02X}");
    }

    /// `]'` and `['`: the sample page on the next or previous slice with a letter naming it, from
    /// the one the cursor is in, `times` over, as a jump.
    pub(crate) fn jump_to_next_mark(&mut self, later: bool, times: usize) {
        let number = self
            .sample_page
            .marked_entry(self.modes.view, &self.services);
        let Some(entry) = self.song().bank.get(number) else {
            return;
        };
        let points = &entry.edit.points;
        let mut named: Vec<usize> = points
            .marks_in_order()
            .into_iter()
            .map(|(_, slice)| slice)
            .collect();
        named.dedup();
        let here = points.slice_at(self.sample_page.cursor);
        let found = if later {
            named.iter().filter(|slice| **slice > here).nth(times - 1)
        } else {
            named
                .iter()
                .rev()
                .filter(|slice| **slice < here)
                .nth(times - 1)
        };
        match found {
            Some(slice) => {
                let slice = *slice;
                let letters: Vec<String> = points
                    .marks_of(slice)
                    .into_iter()
                    .map(|letter| format!("'{letter}"))
                    .collect();
                self.go_to_slice(number, slice);
                self.services.status = format!("{}: slice {slice:02X}", letters.join(" "));
            }
            None if named.is_empty() => {
                self.services.status =
                    "No slice has a name: :mark a names the slice here".to_owned();
            }
            None => self.services.status = "No named slice further that way".to_owned(),
        }
    }

    /// Put the sample page's cursor on a slice's start, as a jump.
    fn go_to_slice(&mut self, number: u8, slice: usize) {
        let track = self.services.cursor.track;
        self.record_jump();
        self.show_view(View::Sample);
        self.sample_page.pinned =
            (number != self.services.track_sample_number()).then_some((track, number));
        match self.sample_page_here() {
            Ok(shown) => {
                let start = shown.slice(slice).start;
                self.sample_page.show_frame(start, shown.length());
            }
            Err(why) => self.services.status = why,
        }
    }

    /// `gd` on the track page or the mixer: on a macro, its mapping view; on a setting or a
    /// fader a macro holds, that macro in the Gen page's band, ready to turn, as a jump `C-o`
    /// comes back from (`gm` there opens its mapping); and on one more than one macro holds, a
    /// list of them to go to. Whether the cursor was on a macro or a setting one holds. It's
    /// `App`'s, as going to a macro is a jump.
    pub(crate) fn open_macro_here(&mut self) -> bool {
        let control = self.track_control();
        if let Control::Macro(at) = control {
            let outcome = MappingView::open(at, 0, &mut self.services);
            self.settled(outcome, false);
            return true;
        }
        let held = self.services.macros_holding(control);
        match held.as_slice() {
            [] => false,
            [(at, _)] => {
                self.go_to_macro(*at);
                true
            }
            _ => {
                let picker = self.services.holding_picker(&held);
                self.show_picker(picker);
                true
            }
        }
    }

    /// Go to a macro in the Gen page's band, as a jump, with no overlay open, so the next `=`
    /// turns it.
    pub(crate) fn go_to_macro(&mut self, at: usize) {
        let track = self.services.page_track();
        self.record_jump();
        self.track_page.show(Page::Gen);
        self.track_page.phrase = self.services.phrase_id();
        let rows = self.track_page.rows(Some(&track));
        let place = rows.iter().enumerate().find_map(|(row, places)| {
            let field = places
                .iter()
                .position(|place| *place == Some(Control::Macro(at)))?;
            Some((row, field))
        });
        if let Some((row, field)) = place {
            self.track_page
                .move_to(self.services.track_here(), row, field);
        }
        self.show_view(View::Track);
    }
}
