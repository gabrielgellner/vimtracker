//! The cursor: moving it, keeping it on what exists, and the phrase view's scroll following it.

use super::App;

impl App {
    /// Keep the cursor's row on screen, moving the phrase view rather than the cursor.
    /// Beside the grid with the keys in Gen's panel, it keeps the panel's device's rows in view
    /// instead (decision 468).
    pub fn follow_cursor(&mut self, height: usize) {
        match self.gen_lit() {
            Some((_, rows)) => self.phrase_view.show_rows(height, rows, &self.services),
            None => self.phrase_view.follow_cursor(height, &self.services),
        }
    }

    pub(crate) fn after_history(&mut self) {
        self.services.dirty = true;
        self.fit_levels();
        self.services.cursor.track = self.services.cursor.track.min(self.song().tracks.len() - 1);
        self.services.clamp_cursor(self.modes.view);
        self.services.publish();
    }

    pub(crate) fn slots(&self) -> usize {
        self.services
            .phrase_here()
            .and_then(|p| p.steps().get(self.services.cursor.step))
            .map_or(0, |step| step.fx.len())
    }
}
