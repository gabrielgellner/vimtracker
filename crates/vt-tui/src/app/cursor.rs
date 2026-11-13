//! The cursor: moving it, keeping it on what exists, and the view's scroll following it.

use crate::timeline;

use crate::input::bindings::Cmd;
use crate::input::mode::View;

use super::App;

impl App {
    fn stored_steps(&self) -> usize {
        self.extent().stored
    }

    /// The furthest the view can scroll: to the end of what is stored, or with the loop's start at
    /// the top, so a slide can keep row `00` still on a screen with room to spare. Rows taking
    /// several lines in the timeline count each of them.
    fn last_top(&self) -> usize {
        let counts = self.row_line_counts();
        timeline::row_lines_before(&counts, counts.len(), self.view_height).max(self.extent().start)
    }

    /// Keep the cursor's row on screen, moving the view rather than the cursor.
    pub fn follow_cursor(&mut self, height: usize) {
        self.view_height = height.max(1);
        let last_top = self.last_top();
        let counts = self.row_line_counts();
        if self.cursor.step < self.top {
            self.top = self.cursor.step;
        } else {
            let end = self.cursor.step + 1;
            self.top = self
                .top
                .max(timeline::row_lines_before(&counts, end, self.view_height))
                .min(self.cursor.step);
        }
        self.top = self.top.min(last_top);
    }

    pub(super) fn view_cursor(&mut self, cmd: Cmd) {
        let counts = self.row_line_counts();
        let all = timeline::lines_over(&counts, 0..self.stored_steps().max(1));
        let height = self.view_height.min(all);
        // The bottom is the last row whose lines all fit, so the view doesn't move to show it.
        self.cursor.step = match cmd {
            Cmd::ViewTop => self.top,
            Cmd::ViewMiddle => timeline::row_at_line(&counts, self.top, height / 2),
            _ => timeline::row_at_line(&counts, self.top, height)
                .saturating_sub(1)
                .max(self.top),
        };
        self.clamp_cursor();
    }

    pub(super) fn half_page(&mut self, sign: isize) {
        let counts = self.row_line_counts();
        let half = (self.view_height / 2).max(1);
        let rows = timeline::rows_over(&counts, self.cursor.step, half, sign > 0).max(1);
        self.move_step(sign * rows as isize);
        let counts = self.row_line_counts();
        self.top = timeline::row_lines_before(&counts, self.cursor.step, self.view_height / 2);
    }

    /// Scroll the view. The cursor only moves if it would leave the screen.
    pub(super) fn scroll(&mut self, by: isize) {
        self.top = self.top.saturating_add_signed(by).min(self.last_top());
        let counts = self.row_line_counts();
        // The last row whose lines all fit, or the top's own if it doesn't.
        let bottom = timeline::row_at_line(&counts, self.top, self.view_height)
            .saturating_sub(1)
            .max(self.top);
        self.cursor.step = self.cursor.step.clamp(self.top, bottom);
        self.clamp_cursor();
    }

    pub(super) fn after_history(&mut self) {
        self.dirty = true;
        self.fit_levels();
        self.cursor.track = self.cursor.track.min(self.song().tracks.len() - 1);
        self.clamp_cursor();
        self.publish();
    }

    pub(crate) fn slots(&self) -> usize {
        self.phrase_here()
            .and_then(|p| p.steps().get(self.cursor.step))
            .map_or(0, |step| step.fx.len())
    }

    pub(crate) fn move_step(&mut self, by: isize) {
        let Some(phrase) = self.phrase_here().cloned() else {
            return;
        };
        let last = phrase.steps().len() as isize - 1;
        self.cursor.step = (self.cursor.step as isize + by).clamp(0, last) as usize;
    }

    /// Keep the cursor on something that exists. Only the chain view has a row past the end.
    pub(crate) fn clamp_cursor(&mut self) {
        let sections = self.song().sections.len();
        self.cursor.section = self.cursor.section.min(sections.saturating_sub(1));
        let rows = self.chain_entries().len();
        let last = if self.view == View::Chain {
            rows
        } else {
            rows.saturating_sub(1)
        };
        self.cursor.slot = self.cursor.slot.min(last);
        self.move_step(0);
    }
}
