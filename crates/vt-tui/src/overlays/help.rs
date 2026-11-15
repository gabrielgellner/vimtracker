//! The which-key popup and `Space k`, both generated from the binding table.

use std::time::{Duration, Instant};

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::input::bindings::Cmd;
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::overlays::Overlay;
use crate::pages::{Ctx, CtxMut, Outcome};
use crate::widgets::popup;

/// How long a prefix waits before showing what can come next.
pub const POPUP_DELAY: Duration = Duration::from_millis(300);

/// Draw the list of next keys, if a prefix has waited long enough.
pub fn draw_popup(frame: &mut Frame, area: Rect, app: &App, now: Instant) {
    let rows = app.popup_rows(now, POPUP_DELAY);
    if rows.is_empty() {
        return;
    }
    let lines: Vec<Line> = rows
        .iter()
        .map(|(keys, description)| Line::raw(format!("{keys:<6} {description}")))
        .collect();
    // As wide as its longest row, so no description is cut off.
    let widest = lines.iter().map(Line::width).max().unwrap_or(0);
    let window = popup::bottom_right(area, widest as u16 + 2, lines.len() as u16 + 2);
    let inside = popup::draw(frame, window, popup::border());
    frame.render_widget(Paragraph::new(lines), inside);
}

/// `Space k`: every key, from the binding table. `j` and `k` scroll, and any other key closes it.
#[derive(Debug, Clone, Default)]
pub struct Help {
    /// Its rows, as they were when it opened.
    rows: Vec<String>,
    /// How far it has scrolled.
    pub scroll: usize,
}

/// A key of `Space k`'s list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpKey {
    Down,
    Up,
}

impl Help {
    /// The list from its top, over these rows.
    pub(crate) fn over(rows: Vec<String>) -> Self {
        Self { rows, scroll: 0 }
    }
}

impl Overlay for Help {
    const MODE: Mode = Mode::Help;

    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("j", Cmd::InHelp(HelpKey::Down), "Scroll down"),
        ("Down", Cmd::InHelp(HelpKey::Down), "Scroll down"),
        ("k", Cmd::InHelp(HelpKey::Up), "Scroll up"),
        ("Up", Cmd::InHelp(HelpKey::Up), "Scroll up"),
    ];

    fn key(&mut self, cmd: Cmd, _ctx: &mut CtxMut) -> Outcome {
        match cmd {
            Cmd::InHelp(HelpKey::Down) => self.scroll += 1,
            Cmd::InHelp(HelpKey::Up) => self.scroll = self.scroll.saturating_sub(1),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// It scrolls whatever is held with `j` or `k`.
    fn reads(&self, key: Key) -> Key {
        Key::new(key.code)
    }

    /// Any other key closes it.
    fn text(&mut self, _key: Key, _ctx: &mut CtxMut) -> Outcome {
        Outcome::Close
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect, _ctx: &Ctx) {
        let lines: Vec<Line> = self
            .rows
            .iter()
            .skip(self.scroll)
            .map(|row| Line::raw(row.as_str()))
            .collect();
        let border = popup::border()
            .title("Keys: j and k scroll, any other key closes")
            .title_style(Style::new().add_modifier(Modifier::BOLD));
        let inside = popup::draw(frame, area, border);
        frame.render_widget(Paragraph::new(lines), inside);
    }
}
