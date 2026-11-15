//! The cursor: moving it, keeping it on what exists, and the phrase view's scroll following it.

use super::App;

impl App {
    /// Keep the cursor's row on screen, moving the phrase view rather than the cursor.
    pub fn follow_cursor(&mut self, height: usize) {
        self.phrase_view.follow_cursor(height, &self.services);
    }

    pub(crate) fn after_history(&mut self) {
        self.services.dirty = true;
        self.fit_levels();
        self.services.cursor.track = self.services.cursor.track.min(self.song().tracks.len() - 1);
        self.clamp_cursor();
        self.services.publish();
    }

    pub(crate) fn slots(&self) -> usize {
        self.services
            .phrase_here()
            .and_then(|p| p.steps().get(self.services.cursor.step))
            .map_or(0, |step| step.fx.len())
    }

    pub(crate) fn move_step(&mut self, by: isize) {
        self.services.move_step(by);
    }

    /// Keep the cursor on something that exists. Only the chain view has a row past the end.
    pub(crate) fn clamp_cursor(&mut self) {
        self.services.clamp_cursor(self.modes.view);
    }
}
