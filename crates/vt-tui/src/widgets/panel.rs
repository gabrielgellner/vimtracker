//! A device's graphic panel: its pictures and meters side by side, each in a faint square box, as
//! part of the page's grid rather than a window over it, with a caption in its bottom border if it
//! has one, such as `GR`, and a meter's scale as text beside its box. A device puts it against its
//! right side, with
//! [`Device::panel_area`](crate::widgets::device::Device::panel_area), and the panel sits
//! in the middle of the rows it's given.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::widgets::theme;

/// The room between a panel's boxes.
const GAP: u16 = 2;

/// One part of a panel: its lines, an image's placeholders, a meter's cells or a scale's numbers,
/// and whether it's in a box, with its caption.
struct Part {
    lines: Vec<Line<'static>>,
    width: u16,
    boxed: bool,
    caption: Option<String>,
}

impl Part {
    /// How wide it is with its box's edges, if it has them.
    fn outer(&self) -> u16 {
        self.width + if self.boxed { 2 } else { 0 }
    }
}

/// A panel's parts, left to right.
#[derive(Default)]
pub(crate) struct GraphicPanel {
    parts: Vec<Part>,
}

impl GraphicPanel {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The same with a box after the others, round these lines, this wide inside, with a caption
    /// in its bottom border or not.
    #[must_use]
    pub(crate) fn boxed(
        mut self,
        lines: Vec<Line<'static>>,
        width: u16,
        caption: Option<&str>,
    ) -> Self {
        self.parts.push(Part {
            lines,
            width,
            boxed: true,
            caption: caption.map(str::to_owned),
        });
        self
    }

    /// The same with lines right after the last box, against it, in line with the rows inside it,
    /// such as a meter's scale.
    #[must_use]
    pub(crate) fn beside(mut self, lines: Vec<Line<'static>>, width: u16) -> Self {
        self.parts.push(Part {
            lines,
            width,
            boxed: false,
            caption: None,
        });
        self
    }

    /// Whether it has nothing to show.
    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The room before each part: a gap before every box but the first, and none before lines
    /// beside a box.
    fn gaps(&self) -> impl Iterator<Item = u16> + '_ {
        self.parts
            .iter()
            .enumerate()
            .map(|(index, part)| if index > 0 && part.boxed { GAP } else { 0 })
    }

    /// How wide it is, with its boxes' edges and the gaps between them.
    pub(crate) fn width(&self) -> u16 {
        let parts: u16 = self.parts.iter().map(Part::outer).sum();
        parts + self.gaps().sum::<u16>()
    }

    /// How tall it is, its tallest box with its edges.
    pub(crate) fn height(&self) -> u16 {
        let lines = self.parts.iter().map(|part| part.lines.len());
        lines.max().map_or(0, |lines| lines as u16 + 2)
    }

    /// Its top row, in the middle of an area's rows.
    fn top(&self, area: Rect) -> u16 {
        area.y + area.height.saturating_sub(self.height()) / 2
    }

    /// Where the inside of its first box starts, drawn in an area: the character at the top left
    /// of a picture in it, for the terminal to place something over it.
    pub(crate) fn inside_at(&self, area: Rect) -> (u16, u16) {
        (area.x + 1, self.top(area) + 1)
    }
}

impl Part {
    /// The part a line for each row: in its box, or blank above and below where a box has edges.
    fn drawn(self, height: usize) -> Vec<Line<'static>> {
        let width = usize::from(self.width);
        let blank = || Line::raw(" ".repeat(width));
        let mut inside = self.lines;
        inside.resize_with(height, blank);
        if !self.boxed {
            let mut lines = vec![blank()];
            lines.extend(inside);
            lines.push(blank());
            return lines;
        }
        let edge = theme::faint();
        let rule = "─".repeat(width);
        let mut lines = vec![Line::styled(format!("┌{rule}┐"), edge)];
        for line in inside {
            let mut spans = vec![Span::styled("│", edge)];
            spans.extend(line.spans);
            spans.push(Span::styled("│", edge));
            lines.push(Line::from(spans));
        }
        let caption = self.caption.as_deref().unwrap_or("");
        let rest = "─".repeat(width.saturating_sub(caption.chars().count()));
        lines.push(Line::styled(format!("└{caption}{rest}┘"), edge));
        lines
    }
}

impl Widget for GraphicPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let height = self.height();
        let top = self.top(area);
        let inside = usize::from(height.saturating_sub(2));
        let gaps: Vec<u16> = self.gaps().collect();
        let mut x = area.x;
        for (part, gap) in self.parts.into_iter().zip(gaps) {
            x += gap;
            let width = part.outer();
            for (row, line) in part.drawn(inside).into_iter().enumerate() {
                let y = top + row as u16;
                if y < area.bottom() && x < area.right() {
                    let room = width.min(area.right() - x);
                    line.render(Rect::new(x, y, room, 1), buf);
                }
            }
            x += width;
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use super::*;
    use crate::widgets::image;
    use crate::widgets::pin::{assert_drawn, drawn};

    /// A picture three cells wide and two high, a meter of two rows captioned `GR`, and its scale
    /// beside it.
    fn panel() -> GraphicPanel {
        let meter = vec![
            Line::from(Span::styled("█", Style::new().fg(theme::PEACH))),
            Line::from(Span::styled("▄", Style::new().fg(theme::PEACH))),
        ];
        let scale = vec![
            Line::styled("0", theme::faint()),
            Line::styled("-6", theme::faint()),
        ];
        GraphicPanel::new()
            .boxed(
                image::rows(3, 3, 2).into_iter().map(Line::from).collect(),
                3,
                None,
            )
            .boxed(meter, 1, Some("GR"))
            .beside(scale, 2)
    }

    #[test]
    fn a_panel_boxes_its_parts_side_by_side_in_the_middle_of_its_rows() {
        let panel = panel();
        assert_eq!((panel.width(), panel.height()), (12, 4));
        assert_eq!(panel.inside_at(Rect::new(4, 10, 12, 7)), (5, 12));
        assert_drawn(
            &drawn(panel, 12, 6),
            &[
                "            ",
                "............",
                "┌───┐  ┌─┐  ",
                "aaaaa..aaa..",
                "│▒▒▒│  │█│0 ",
                "abbba..acaaa",
                "│▒▒▒│  │▄│-6",
                "abbba..acaaa",
                "└───┘  └GR  ",
                "aaaaa..aaa..",
                "            ",
                "............",
                "a fg OVERLAY0",
                "b fg #000003",
                "c fg PEACH",
            ],
        );
    }

    #[test]
    fn a_panel_is_cut_at_its_areas_right_edge() {
        assert_drawn(
            &drawn(panel(), 7, 4),
            &[
                "┌───┐  ",
                "aaaaa..",
                "│▒▒▒│  ",
                "abbba..",
                "│▒▒▒│  ",
                "abbba..",
                "└───┘  ",
                "aaaaa..",
                "a fg OVERLAY0",
                "b fg #000003",
            ],
        );
    }

    #[test]
    fn an_empty_panel_takes_no_room() {
        let panel = GraphicPanel::new();
        assert!(panel.is_empty());
        assert_eq!((panel.width(), panel.height()), (0, 0));
    }
}
