//! Marks: letters naming slices of the sample the current track plays, as Vim's marks name lines,
//! so a step can lock a slice by its name, `SLI 'a`, and keep playing it as slices before it come
//! and go. `:mark a` names one, `:marks` lists them, and `:delmarks a-c` and `:delmarks!` take them
//! away, each one undo step on the bank entry. On the sample page, `'a` goes to the slice named
//! `a`, and `]'` and `['` to the next and previous slice with a name, as jumps `C-o` comes back
//! from.

use vt_model::action::Action;

use crate::app::App;
use crate::banking::sample_text;
use crate::input::mode::View;
use crate::input::picker::{Pick, Picker};

impl App {
    /// The bank entry marks are named in: the one the sample page shows, while it's the view, or
    /// else the one the current track plays.
    pub(crate) fn marked_entry(&self) -> u8 {
        match self.sample_page.pinned {
            Some((track, number)) if self.view == View::Sample && track == self.cursor.track => {
                number
            }
            _ => self.track_sample_number(),
        }
    }

    /// `:mark a`: name a slice with a letter, by its number, or the one here: the one the sample
    /// page's cursor is in, or off the page the one the track's `Slice` is on. The letter leaves
    /// any slice it named before.
    pub(crate) fn set_mark(&mut self, letter: char, slice: Option<u8>) {
        let number = self.marked_entry();
        let Some(mut entry) = self.song().bank.get(number).cloned() else {
            self.status = format!(
                "Track {:02} plays no sample to mark: K on Sample chooses one",
                self.cursor.track + 1
            );
            return;
        };
        let name = sample_text(self.song(), number);
        let points = &mut entry.edit.points;
        if points.slices.is_empty() {
            self.status = format!("{name} has no slices to name: :slice 16 cuts it into 16");
            return;
        }
        let count = points.slices.len();
        let at = match slice {
            Some(slice) if usize::from(slice) >= count => {
                self.status = format!("{name} has slices 00 to {:02X}", count - 1);
                return;
            }
            Some(slice) => usize::from(slice),
            None => self.slice_here(number, &entry),
        };
        let points = &mut entry.edit.points;
        let before = points.marked(letter).filter(|before| *before != at);
        points.set_mark(letter, at);
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let moved = before.map_or(String::new(), |before| format!(", not {before:02X}"));
        self.status = format!("'{letter} names slice {at:02X} of {name}{moved}: u to undo");
    }

    /// `:marks`: list the letters naming slices, in the order of their slices, with each slice's
    /// number and where it starts, to go to one on the sample page.
    pub(crate) fn open_marks_picker(&mut self) {
        self.count = None;
        let number = self.marked_entry();
        let items = self.mark_items(number, Pick::Mark);
        if items.is_empty() {
            self.status = match self.song().bank.get(number) {
                Some(_) => format!(
                    "No slice of {} has a name: :mark a names the slice here",
                    sample_text(self.song(), number)
                ),
                None => format!("Track {:02} plays no sample", self.cursor.track + 1),
            };
            return;
        }
        self.show_picker(Picker::new("Marks", items));
    }

    /// `:delmarks a-c` takes these letters off their slices, and `:delmarks!` every letter.
    pub(crate) fn delete_marks(&mut self, letters: Option<&[char]>) {
        let number = self.marked_entry();
        let Some(mut entry) = self.song().bank.get(number).cloned() else {
            self.status = format!("Track {:02} plays no sample", self.cursor.track + 1);
            return;
        };
        let marks = &mut entry.edit.points.marks;
        let taken: Vec<char> = marks
            .keys()
            .copied()
            .filter(|letter| letters.is_none_or(|letters| letters.contains(letter)))
            .collect();
        let name = sample_text(self.song(), number);
        if taken.is_empty() {
            self.status = format!("No such marks on {name}");
            return;
        }
        marks.retain(|letter, _| !taken.contains(letter));
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let said: Vec<String> = taken.iter().map(|letter| format!("'{letter}")).collect();
        self.status = format!("Took {} off {name}: u to undo", said.join(" "));
    }

    /// `'a`: the sample page on the slice a letter names, as a jump `C-o` comes back from.
    pub(crate) fn jump_to_mark(&mut self, letter: char) {
        let number = self.marked_entry();
        let marked = self
            .song()
            .bank
            .get(number)
            .and_then(|entry| entry.edit.points.marked(letter));
        let Some(slice) = marked else {
            self.status = format!(
                "No slice of {} is named {letter}: :marks lists them",
                sample_text(self.song(), number)
            );
            return;
        };
        self.go_to_slice(number, slice);
        self.status = format!("'{letter}: slice {slice:02X}");
    }

    /// `]'` and `['`: the sample page on the next or previous slice with a letter naming it, from
    /// the one the cursor is in, `times` over, as a jump.
    pub(crate) fn jump_to_next_mark(&mut self, later: bool, times: usize) {
        let number = self.marked_entry();
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
                self.status = format!("{}: slice {slice:02X}", letters.join(" "));
            }
            None if named.is_empty() => {
                self.status = "No slice has a name: :mark a names the slice here".to_owned();
            }
            None => self.status = "No named slice further that way".to_owned(),
        }
    }

    /// Put the sample page's cursor on a slice's start, as a jump.
    fn go_to_slice(&mut self, number: u8, slice: usize) {
        let track = self.cursor.track;
        self.record_jump();
        self.show_view(View::Sample);
        self.sample_page.pinned = (number != self.track_sample_number()).then_some((track, number));
        match self.sample_page_here() {
            Ok(shown) => {
                let start = shown.slice(slice).start;
                self.sample_page.show_frame(start, shown.length());
            }
            Err(why) => self.status = why,
        }
    }
}
