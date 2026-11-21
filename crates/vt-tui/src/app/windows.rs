//! Gen beside the grid (decision 467): the phrase view's grid and Gen as Vim's two windows, the
//! side form with the panel at the grid's right, or the full form with Gen the whole page, and
//! `Ctrl-w` with `h` `l` `w` `o` `v` `c` between them. The form is kept for the session in
//! `gen_page.beside`, and whether the panel shows beside the grid in `phrase_view.gen_beside`.
//! With the panel open, `gl` moves it to the cursor's live column, `gL` puts the keys there too,
//! and `L` in the shape panel lands in the new generator's device (decision 469).

use std::sync::Arc;

use crate::app::App;
use crate::input::bindings::Window;
use crate::input::mode::View;
use crate::overlays::shape::ShapePanel;
use crate::services::generators::generator_label;

/// What `gl` and `gL` say off a live column.
const NOT_LIVE: &str = "No live column here. Select rows, press S, then L to keep it live";

impl App {
    /// Whether Gen's panel is on screen beside the grid: the side form, the keys in either.
    pub fn gen_beside_shown(&self) -> bool {
        self.phrase_view.gen_beside && matches!(self.modes.view, View::Phrase | View::Gen)
    }

    /// Gen showing, in the form it's in, as `Space 8` shows it: beside the grid, the panel
    /// opening there, or the whole page, the panel closed.
    pub(super) fn gen_form_shown(&mut self) {
        self.phrase_view.gen_beside = self.gen_page.beside;
        self.gen_page.phrase = self.services.phrase_id();
        let song = Arc::clone(self.song());
        self.gen_page
            .settle(song.tracks.get(self.services.cursor.track));
    }

    /// A window key, `Ctrl-w` and a letter, or `Esc` in the panel. Moving the keys between the
    /// grid and Gen isn't a jump, as moving between Vim's windows isn't.
    pub(crate) fn window(&mut self, key: Window) {
        self.modes.count = None;
        let in_gen = self.modes.view == View::Gen;
        match key {
            Window::Grid if in_gen => self.show_view(View::Phrase),
            Window::Gen if !in_gen => self.show_view(View::Gen),
            Window::Other => match in_gen {
                true => self.show_view(View::Phrase),
                false => self.show_view(View::Gen),
            },
            Window::Grid | Window::Gen => {}
            Window::Only => {
                self.gen_page.beside = false;
                self.show_view(View::Gen);
            }
            // The form changes and the keys stay where they are, as a split leaves them.
            Window::Split => {
                self.gen_page.beside = true;
                match in_gen {
                    true => self.show_view(View::Gen),
                    false => self.phrase_view.gen_beside = true,
                }
            }
            Window::Close => {
                self.phrase_view.gen_beside = false;
                if in_gen {
                    self.show_view(View::Phrase);
                }
            }
        }
    }

    /// Gen's panel on the device of a live generator in the phrase under the cursor, by its place
    /// in the phrase's list, if it has one there.
    fn gen_on(&mut self, generator: usize) -> bool {
        self.gen_page.phrase = self.services.phrase_id();
        let song = Arc::clone(self.song());
        self.gen_page
            .to_generator(song.tracks.get(self.services.cursor.track), generator)
    }

    /// `gl` with Gen's panel beside the grid: the panel on the cursor's live column's device, the
    /// keys staying in the grid, as there's nothing the shape panel would show that it doesn't.
    /// Returns false with no panel, for `gl` to open the shape panel as it does.
    pub(crate) fn gen_to_here(&mut self) -> bool {
        if !(self.gen_beside_shown() && self.modes.view == View::Phrase) {
            return false;
        }
        self.modes.count = None;
        match self.services.generator_here() {
            Some((phrase, generator)) if self.gen_on(generator) => {
                let track = &self.song().tracks[self.services.cursor.track];
                let name = generator_label(&track.phrases[&phrase].generators()[generator]);
                self.services.status =
                    format!("{name} in Gen beside: Ctrl-w l puts the keys in it");
            }
            _ => self.services.status = NOT_LIVE.to_owned(),
        }
        true
    }

    /// `gL`, the wider `gl`: Gen beside the grid on the cursor's live column's device, the keys in
    /// it, in the side form whichever form was used last.
    pub(crate) fn open_gen_here(&mut self) {
        self.modes.count = None;
        let Some((_, generator)) = self.services.generator_here() else {
            self.services.status = NOT_LIVE.to_owned();
            return;
        };
        self.gen_page.beside = true;
        self.gen_on(generator);
        self.show_view(View::Gen);
    }

    /// After `L` in the shape panel, with Gen's panel beside the grid: the new generator's device
    /// in the panel, the keys in it. After `Enter` or `Esc`, the keys stay in the grid.
    pub(crate) fn land_kept(&mut self, panel: &ShapePanel) {
        if !(self.gen_beside_shown() && self.modes.view == View::Phrase) {
            return;
        }
        if let Some(generator) = panel.kept_generator(&self.services)
            && self.gen_on(generator)
        {
            self.show_view(View::Gen);
        }
    }
}
