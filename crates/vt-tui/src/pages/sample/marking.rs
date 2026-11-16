//! Marks: letters naming slices of the sample the current track plays, as Vim's marks name lines,
//! so a step can lock a slice by its name, `SLI 'a`, and keep playing it as slices before it come
//! and go. `:mark a` names one, `:marks` lists them, and `:delmarks a-c` and `:delmarks!` take them
//! away, each one undo step on the bank entry. On the sample page, `m a` names the slice under the
//! cursor, `'a` goes to the slice named `a`, `]'` and `['` to the next and previous slice with a
//! name, `'<` and `'>` to the ends of the last selection, and `''` back to where the latest jump
//! left from, each a jump `C-o` comes back from.

use vt_model::action::Action;

use crate::input::mode::View;
use crate::overlays::picker::{Pick, Picker};
use crate::pages::sample::SamplePage;
use crate::services::Services;
use crate::services::banking::sample_text;

/// What a key after `'` or `m` on the sample page is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MarkWait {
    /// `'`: a letter's slice, `<` or `>` for the last selection's ends, or `'` for the latest
    /// jump's start.
    Jump,
    /// `m`: the letter to name the slice under the cursor with.
    Name,
}

impl SamplePage {
    /// The bank entry marks are named in: the one the page shows, while it's the view, or else
    /// the one the current track plays.
    pub(crate) fn marked_entry(&self, view: View, services: &Services) -> u8 {
        match self.pinned {
            Some((track, number)) if view == View::Sample && track == services.cursor.track => {
                number
            }
            _ => services.track_sample_number(),
        }
    }

    /// `:mark a`: name a slice with a letter, by its number, or the one here: the one the sample
    /// page's cursor is in, or off the page the one the track's `Slice` is on. The letter leaves
    /// any slice it named before.
    pub(crate) fn set_mark(
        &self,
        letter: char,
        slice: Option<u8>,
        view: View,
        services: &mut Services,
    ) {
        let number = self.marked_entry(view, services);
        let Some(mut entry) = services.song().bank.get(number).cloned() else {
            services.status = format!(
                "Track {:02} plays no sample to mark: K on Sample chooses one",
                services.cursor.track + 1
            );
            return;
        };
        let name = sample_text(services.song(), number);
        let points = &mut entry.edit.points;
        if points.slices.is_empty() {
            services.status = format!("{name} has no slices to name: :slice 16 cuts it into 16");
            return;
        }
        let count = points.slices.len();
        let at = match slice {
            Some(slice) if usize::from(slice) >= count => {
                services.status = format!("{name} has slices 00 to {:02X}", count - 1);
                return;
            }
            Some(slice) => usize::from(slice),
            None => self.slice_here(view, number, &entry, services),
        };
        let points = &mut entry.edit.points;
        let before = points.marked(letter).filter(|before| *before != at);
        points.set_mark(letter, at);
        services.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let moved = before.map_or(String::new(), |before| format!(", not {before:02X}"));
        services.status = format!("'{letter} names slice {at:02X} of {name}{moved}: u to undo");
    }

    /// `:marks`: the letters naming slices, in the order of their slices, with each slice's number
    /// and where it starts, to go to one on the page, or why there are none.
    pub(crate) fn marks_picker(&self, view: View, services: &Services) -> Result<Picker, String> {
        let number = self.marked_entry(view, services);
        let items = services.mark_items(number, Pick::Mark);
        if items.is_empty() {
            return Err(match services.song().bank.get(number) {
                Some(_) => format!(
                    "No slice of {} has a name: :mark a names the slice here",
                    sample_text(services.song(), number)
                ),
                None => format!("Track {:02} plays no sample", services.cursor.track + 1),
            });
        }
        Ok(Picker::new("Marks", items))
    }

    /// `:delmarks a-c` takes these letters off their slices, and `:delmarks!` every letter.
    pub(crate) fn delete_marks(
        &self,
        letters: Option<&[char]>,
        view: View,
        services: &mut Services,
    ) {
        let number = self.marked_entry(view, services);
        let Some(mut entry) = services.song().bank.get(number).cloned() else {
            services.status = format!("Track {:02} plays no sample", services.cursor.track + 1);
            return;
        };
        let marks = &mut entry.edit.points.marks;
        let taken: Vec<char> = marks
            .keys()
            .copied()
            .filter(|letter| letters.is_none_or(|letters| letters.contains(letter)))
            .collect();
        let name = sample_text(services.song(), number);
        if taken.is_empty() {
            services.status = format!("No such marks on {name}");
            return;
        }
        marks.retain(|letter, _| !taken.contains(letter));
        services.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let said: Vec<String> = taken.iter().map(|letter| format!("'{letter}")).collect();
        services.status = format!("Took {} off {name}: u to undo", said.join(" "));
    }
}
