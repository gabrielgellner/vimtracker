//! A device's graphic panel: its pictures and meters side by side, each in a faint square box, as
//! part of the page's grid rather than a window over it, with a caption in its bottom border if it
//! has one, such as `GR`. A device puts it against its right side, with
//! [`Device::panel_area`](crate::views::widgets::device::Device::panel_area), and the panel sits
//! in the middle of the rows it's given.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::theme;

/// The room between a panel's boxes.
const GAP: u16 = 2;

/// One box of a panel: its lines, an image's placeholders or a meter's cells, and its caption.
struct Part {
    lines: Vec<Line<'static>>,
    width: u16,
    caption: Option<&'static str>,
}

/// A panel's boxes, left to right.
#[derive(Default)]
pub struct GraphicPanel {
    parts: Vec<Part>,
}

impl GraphicPanel {
    pub fn new() -> Self {
        Self::default()
    }

    /// The same with a box after the others, round these lines, this wide inside, with a caption
    /// in its bottom border or not.
    #[must_use]
    pub fn boxed(
        mut self,
        lines: Vec<Line<'static>>,
        width: u16,
        caption: Option<&'static str>,
    ) -> Self {
        self.parts.push(Part {
            lines,
            width,
            caption,
        });
        self
    }

    /// Whether it has nothing to show.
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// How wide it is, with its boxes' edges and the gaps between them.
    pub fn width(&self) -> u16 {
        let boxes: u16 = self.parts.iter().map(|part| part.width + 2).sum();
        boxes + GAP * (self.parts.len() as u16).saturating_sub(1)
    }

    /// How tall it is, its tallest box with its edges.
    pub fn height(&self) -> u16 {
        let lines = self.parts.iter().map(|part| part.lines.len());
        lines.max().map_or(0, |lines| lines as u16 + 2)
    }
}

impl Part {
    /// The box round its lines, a line for each row.
    fn boxed(self, height: usize) -> Vec<Line<'static>> {
        let width = usize::from(self.width);
        let edge = theme::faint();
        let rule = "─".repeat(width);
        let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
        let blank = || Line::raw(" ".repeat(width));
        let mut inside = self.lines;
        inside.resize_with(height, blank);
        for line in inside {
            let mut spans = vec![Span::styled("│", edge)];
            spans.extend(line.spans);
            spans.push(Span::styled("│", edge));
            lines.push(Line::from(spans));
        }
        let caption = self.caption.unwrap_or("");
        let rest = "─".repeat(width.saturating_sub(caption.chars().count()));
        lines.push(Line::styled(format!("└{caption}{rest}┘"), edge));
        lines
    }
}

impl Widget for GraphicPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = self.height();
        let top = area.y + area.height.saturating_sub(height) / 2;
        let inside = usize::from(height.saturating_sub(2));
        let mut x = area.x;
        for part in self.parts {
            let width = part.width + 2;
            for (row, line) in part.boxed(inside).into_iter().enumerate() {
                let y = top + row as u16;
                if y < area.bottom() && x < area.right() {
                    let room = width.min(area.right() - x);
                    line.render(Rect::new(x, y, room, 1), buf);
                }
            }
            x += width + GAP;
        }
    }
}
