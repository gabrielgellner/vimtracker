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
