//! A device: one thing in a page's rack, such as an FX slot, in a rounded frame 9 rows tall with
//! its title on the top border. The title says which device it is, dim, then has a place for its
//! switch, then the controls that change its character, such as an effect and its type. Inside
//! are its settings grid on the left and its graphic panel, if it has one, on the right. The
//! device with the cursor has a brighter frame and its title's controls bold.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::theme;
use crate::views::widgets;
use crate::views::widgets::grid::GRID_HEIGHT;

/// How tall every device is, whatever it holds, so nothing moves when what's in it changes: its
/// frame, a blank line, three rows of settings with a blank line after each, and its frame.
pub const DEVICE_HEIGHT: u16 = GRID_HEIGHT + 4;
/// The room between the frame and what's inside it, at each side.
const PADDING: u16 = 2;
/// Where the title starts along the top border, after the corner and a line.
const TITLE_INDENT: u16 = 2;
/// The room between the title's parts.
const TITLE_GAP: &str = "  ";

/// A device's frame and title.
pub struct Device {
    identity: String,
    switch: Option<Span<'static>>,
    controls: Vec<Line<'static>>,
    focused: bool,
}

impl Device {
    /// A device named by this, such as a slot's digit.
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            identity: identity.into(),
            switch: None,
            controls: Vec::new(),
            focused: false,
        }
    }

    /// The same with its switch after its name, or the room for one.
    #[must_use]
    pub fn switch(self, switch: Span<'static>) -> Self {
        Self {
            switch: Some(switch),
            ..self
        }
    }

    /// The same with these controls in its title, each already in its own style.
    #[must_use]
    pub fn controls(self, controls: Vec<Line<'static>>) -> Self {
        Self { controls, ..self }
    }

    /// The same with the cursor in it, or not.
    #[must_use]
    pub fn focused(self, focused: bool) -> Self {
        Self { focused, ..self }
    }

    /// Where a device's settings go in its area: in from its frame, after a blank line.
    pub fn settings_area(area: Rect) -> Rect {
        let inner = inner(area);
        let x = inner.x + PADDING;
        Rect::new(
            x,
            inner.y + 1,
            inner.right().saturating_sub(x),
            GRID_HEIGHT.min(inner.height.saturating_sub(1)),
        )
    }

    /// Where a graphic panel this wide goes in a device's area: against its right side, in from
    /// its frame, as tall as all inside it.
    pub fn panel_area(area: Rect, width: u16) -> Rect {
        let inner = inner(area);
        let right = inner.right().saturating_sub(PADDING);
        let width = width.min(right.saturating_sub(inner.x));
        Rect::new(right - width, inner.y, width, inner.height)
    }
}

/// What's inside a device's frame.
fn inner(area: Rect) -> Rect {
    widgets::window_border().inner(area)
}

impl Widget for Device {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let frame = if self.focused {
            theme::OVERLAY2
        } else {
            theme::SURFACE1
        };
        widgets::window_border()
            .border_style(Style::new().fg(frame))
            .render(area, buf);
        let mut title = vec![Span::raw(" "), Span::styled(self.identity, theme::dim())];
        if let Some(switch) = self.switch {
            title.extend([Span::raw(TITLE_GAP), switch]);
        }
        for control in self.controls {
            title.push(Span::raw(TITLE_GAP));
            title.extend(control.spans.into_iter().map(|span| {
                if self.focused {
                    span.patch_style(Modifier::BOLD)
                } else {
                    span
                }
            }));
        }
        title.push(Span::raw(" "));
        let left = area.x + TITLE_INDENT;
        // The title stops short of the far corner and the line before it.
        let room = area.right().saturating_sub(left + TITLE_INDENT);
        // In the text's colour, not the frame's under it, and only as wide as it is, so the frame
        // after it keeps its own.
        let title = Line::from(title).style(Style::new().fg(theme::TEXT));
        let width = (title.width() as u16).min(room);
        if width > 0 && area.height > 0 {
            title.render(Rect::new(left, area.y, width, 1), buf);
        }
    }
}
