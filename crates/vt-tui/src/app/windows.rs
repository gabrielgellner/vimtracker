//! Gen beside the grid (decision 467): the phrase view's grid and Gen as Vim's two windows, the
//! side form with the panel at the grid's right, or the full form with Gen the whole page, and
//! `Ctrl-w` with `h` `l` `w` `o` `v` `c` between them. The form is kept for the session in
//! `gen_page.beside`, and whether the panel shows beside the grid in `phrase_view.gen_beside`.

use std::sync::Arc;

use crate::app::App;
use crate::input::bindings::Window;
use crate::input::mode::View;

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
        // Beside the grid, on the device the grid's cursor chooses (decision 468).
        if self.gen_page.beside {
            self.follow_grid_cursor();
        }
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
}
