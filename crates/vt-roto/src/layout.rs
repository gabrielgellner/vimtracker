//! A device's layout on the Roto, declared once as data: pages of eight knobs and eight buttons,
//! each slot holding a setting, a gap left on purpose, or nothing. The program says what a
//! setting is (`S`), and this says where it goes. The same layout drives a device shown live,
//! whose buttons the Roto refuses, and one stored on the Roto, whose buttons it lights and labels
//! (`docs/designs/roto-control.md` §4.3).
//!
//! **The Roto plays and tweaks; the app builds.** A layout holds what's played and tweaked, never
//! what a device is (a machine, an effect, an effect's type), which is chosen in the program, as
//! it would swap the whole layout under the hand. A device keeps to one page unless it has more
//! than eight knobs or eight buttons, or a reason of its own; its off/on settings are its first
//! page's buttons, beside its main knobs.
//!
//! **Buttons on a live device** go on knobs instead, as two-step knobs: each on the knob above
//! its button if that's free, or the nearest free knob of its page, left first, and those that
//! still don't fit on a page of their own just after it, each above its button.

use crate::sysex::CONTROLS;

/// One slot of a page: a knob or a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slot<S> {
    /// Nothing here, nor anywhere after it on the page's knobs or buttons: the row ends early.
    Unused,
    /// Left empty on purpose, between settings, to keep a group on knobs of its own, as an
    /// envelope's times on knobs 1 to 4.
    Gap,
    Setting(S),
}

impl<S> Slot<S> {
    /// The setting here, if there's one.
    pub fn setting(&self) -> Option<&S> {
        match self {
            Self::Setting(setting) => Some(setting),
            Self::Unused | Self::Gap => None,
        }
    }

    /// Whether nothing is here, a gap or the end of a row.
    pub fn is_free(&self) -> bool {
        !matches!(self, Self::Setting(_))
    }
}

/// One page of a device: its name, which the guide and the design note use (the Roto has no way
/// to show a page's name), and its eight knobs over its eight buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Page<S: 'static> {
    pub name: &'static str,
    pub knobs: [Slot<S>; CONTROLS],
    pub buttons: [Slot<S>; CONTROLS],
}

/// A device's layout: its pages, in the order the Roto's arrows go through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Layout<S: 'static> {
    pub pages: &'static [Page<S>],
}

/// A knob as a device shown live has it: a knob's setting, or a button's moved onto a knob.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Placed<'a, S> {
    pub setting: &'a S,
    /// Whether it's a button's, on a knob as the Roto has no buttons for a live device.
    pub from_button: bool,
}

impl<S> Clone for Placed<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Placed<'_, S> {}

impl<S> Layout<S> {
    /// Every setting it places, knobs and buttons, page by page.
    pub fn settings(&self) -> impl Iterator<Item = &S> {
        self.pages.iter().flat_map(|page| {
            page.knobs
                .iter()
                .chain(&page.buttons)
                .filter_map(Slot::setting)
        })
    }

    /// Its knobs as a stored device has them, a page of eight after another.
    pub fn knobs(&self) -> Vec<Option<&S>> {
        self.pages
            .iter()
            .flat_map(|page| page.knobs.iter().map(Slot::setting))
            .collect()
    }

    /// Its buttons as a stored device has them, a page of eight after another, or none at all if
    /// it has none.
    pub fn buttons(&self) -> Vec<Option<&S>> {
        let buttons: Vec<Option<&S>> = self
            .pages
            .iter()
            .flat_map(|page| page.buttons.iter().map(Slot::setting))
            .collect();
        if buttons.iter().all(Option::is_none) {
            return Vec::new();
        }
        buttons
    }

    /// Its knobs as a device shown live has them, a page of eight after another, with its buttons
    /// moved onto knobs: each above its button if that's free, or the nearest free knob of its
    /// page, left first, and the rest on a page of their own just after it, each above its
    /// button.
    pub fn live_knobs(&self) -> Vec<Option<Placed<'_, S>>> {
        let mut knobs = Vec::with_capacity(self.pages.len() * CONTROLS);
        for page in self.pages {
            let mut row: [Option<Placed<'_, S>>; CONTROLS] = std::array::from_fn(|at| {
                page.knobs[at].setting().map(|setting| Placed {
                    setting,
                    from_button: false,
                })
            });
            let mut left: [Option<Placed<'_, S>>; CONTROLS] = [None; CONTROLS];
            for (at, button) in page.buttons.iter().enumerate() {
                let Some(setting) = button.setting() else {
                    continue;
                };
                let placed = Placed {
                    setting,
                    from_button: true,
                };
                match nearest_free(&row, at) {
                    Some(free) => row[free] = Some(placed),
                    None => left[at] = Some(placed),
                }
            }
            knobs.extend(row);
            if left.iter().any(Option::is_some) {
                knobs.extend(left);
            }
        }
        knobs
    }
}

/// The free knob nearest `at` in a row: `at` itself, then one either side, the left first, and so
/// on out.
fn nearest_free<T>(row: &[Option<T>; CONTROLS], at: usize) -> Option<usize> {
    (0..CONTROLS)
        .flat_map(|distance| [at.checked_sub(distance), Some(at + distance)])
        .flatten()
        .filter(|place| *place < CONTROLS)
        .find(|place| row[*place].is_none())
}

/// Whether a row keeps to its rule: nothing set after an [`Slot::Unused`].
pub fn row_ends_cleanly<S>(row: &[Slot<S>; CONTROLS]) -> bool {
    let first_unused = row
        .iter()
        .position(|slot| matches!(slot, Slot::Unused))
        .unwrap_or(CONTROLS);
    row[first_unused..].iter().all(Slot::is_free)
}

#[cfg(test)]
mod tests {
    use super::*;

    use Slot::{Gap, Setting, Unused};

    const fn row(settings: [Slot<u8>; CONTROLS]) -> [Slot<u8>; CONTROLS] {
        settings
    }

    #[test]
    fn a_button_goes_on_the_knob_above_it_if_that_is_free() {
        static PAGES: [Page<u8>; 1] = [Page {
            name: "Envelope",
            knobs: row([
                Setting(1),
                Setting(2),
                Setting(3),
                Setting(4),
                Setting(5),
                Setting(6),
                Gap,
                Unused,
            ]),
            buttons: row([
                Unused,
                Unused,
                Unused,
                Unused,
                Unused,
                Unused,
                Unused,
                Setting(9),
            ]),
        }];
        let layout = Layout { pages: &PAGES };
        let knobs = layout.live_knobs();
        assert_eq!(knobs.len(), CONTROLS);
        let settings: Vec<Option<u8>> = knobs.iter().map(|k| k.map(|k| *k.setting)).collect();
        assert_eq!(
            settings,
            [
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                Some(6),
                None,
                Some(9)
            ]
        );
        assert!(knobs[7].unwrap().from_button);
        assert!(!knobs[0].unwrap().from_button);
    }

    #[test]
    fn a_button_under_a_set_knob_takes_the_nearest_free_one_and_the_rest_a_page_of_their_own() {
        static PAGES: [Page<u8>; 1] = [Page {
            name: "Glue",
            knobs: row([
                Setting(1),
                Setting(2),
                Setting(3),
                Setting(4),
                Setting(5),
                Setting(6),
                Setting(7),
                Unused,
            ]),
            buttons: row([
                Setting(10),
                Setting(11),
                Unused,
                Unused,
                Unused,
                Unused,
                Unused,
                Unused,
            ]),
        }];
        let layout = Layout { pages: &PAGES };
        let settings: Vec<Option<u8>> = layout
            .live_knobs()
            .iter()
            .map(|k| k.map(|k| *k.setting))
            .collect();
        assert_eq!(settings.len(), 2 * CONTROLS);
        assert_eq!(settings[7], Some(10), "the only free knob");
        assert_eq!(
            settings[CONTROLS + 1],
            Some(11),
            "over its button, a page on"
        );
        assert_eq!(
            settings.iter().flatten().count(),
            9,
            "every setting placed once"
        );
    }

    #[test]
    fn a_row_ends_cleanly_with_nothing_set_after_its_end() {
        assert!(row_ends_cleanly(&row([
            Setting(1),
            Gap,
            Setting(2),
            Unused,
            Unused,
            Unused,
            Unused,
            Unused
        ])));
        assert!(!row_ends_cleanly(&row([
            Setting(1),
            Unused,
            Setting(2),
            Unused,
            Unused,
            Unused,
            Unused,
            Unused
        ])));
    }
}
