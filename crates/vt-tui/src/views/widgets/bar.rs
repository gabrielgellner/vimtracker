//! A value's bar, as the settings pages draw one beside a value: filled in eighths of a cell, or a
//! mark either side of a centre for pan and pitch, between faint lines at each end of its range.
//! While something moves the value, the fill follows what's playing and a dim mark stays where it's
//! set.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::theme;

/// The lines at each end of a bar's range, touching the cells between them.
const BAR_START: &str = "▕";
const BAR_END: &str = "▏";
/// How many cells the ends take.
pub const BAR_ENDS: u16 = 2;
/// The characters for a cell partly full, from one eighth to seven.
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// A value's bar between its ends, in a number of cells, each split into eighths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValueBar {
    /// What the value is set to.
    set: u8,
    /// What it's playing with now, which differs from what's set while something moves it.
    live: u8,
    /// Whether it's a mark either side of a centre rather than a fill.
    centred: bool,
    /// The cells between the ends.
    cells: usize,
}

impl ValueBar {
    /// A bar for a value as it's set, in this many cells.
    pub fn new(set: u8, cells: usize) -> Self {
        Self {
            set,
            live: set,
            centred: false,
            cells,
        }
    }

    /// The same bar showing what's playing, with a dim mark for what's set if that differs.
    #[must_use]
    pub fn playing(self, live: u8) -> Self {
        Self { live, ..self }
    }

    /// The same bar as a mark either side of a centre, like pan.
    #[must_use]
    pub fn centred(self, centred: bool) -> Self {
        Self { centred, ..self }
    }

    /// The cells between the ends for a value, as text: a fill, or a mark around a centre.
    fn text(&self, value: u8) -> String {
        if self.centred {
            centre_bar(value, self.cells)
        } else {
            level_bar(value, self.cells)
        }
    }

    /// Where a value's edge or mark is along the bar, in eighths of a cell.
    fn eighths(&self, value: u8) -> usize {
        if self.centred {
            usize::from(value) * self.cells * 8 / 256
        } else {
            level_eighths(value, self.cells)
        }
    }

    /// The cells between the ends. The fill or bright mark follows what's playing, with a dim mark
    /// for what's set once they're a cell or more apart. Over a filled cell, that mark is cut out
    /// of the fill. Nearer, the bar is drawn as what's playing alone, as the mark couldn't be told
    /// from the edge, and a value moving about its setting would have it come and go each frame
    /// as a thin line flickering at the edge (#421).
    pub fn spans(&self) -> Vec<Span<'static>> {
        let bright = Style::new().fg(theme::MAUVE);
        let bar = self.text(self.live);
        if self.eighths(self.set).abs_diff(self.eighths(self.live)) < 8 {
            return vec![Span::styled(bar, bright)];
        }
        let mark = bar_cell(self.set, self.cells);
        bar.chars()
            .enumerate()
            .map(|(cell, shown)| match shown {
                '█' if cell == mark => {
                    Span::styled("│", Style::new().fg(theme::BASE).bg(theme::MAUVE))
                }
                ' ' if cell == mark => Span::styled("│", theme::faint()),
                _ => Span::styled(shown.to_string(), bright),
            })
            .collect()
    }
}

/// The bar with its ends, to go in a table's cell.
impl From<ValueBar> for Line<'static> {
    fn from(bar: ValueBar) -> Self {
        let mut spans = vec![Span::styled(BAR_START, theme::faint())];
        spans.extend(bar.spans());
        spans.push(Span::styled(BAR_END, theme::faint()));
        Line::from(spans)
    }
}

/// Drawn at the left of an area's top row, cut at its right edge.
impl Widget for ValueBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Line::from(self).render(area, buf);
    }
}

/// A bar for a value, full at `FF`, filling all of its cells. Only `00` leaves them empty.
pub fn level_bar(value: u8, cells: usize) -> String {
    let eighths = level_eighths(value, cells);
    let mut bar = "█".repeat(eighths / 8);
    if !eighths.is_multiple_of(8) {
        bar.push(EIGHTHS[eighths % 8 - 1]);
    }
    format!("{bar:<cells$}")
}

/// How far a level bar is filled, in eighths of a cell.
fn level_eighths(value: u8, cells: usize) -> usize {
    (usize::from(value) * cells * 8 / 255).max(usize::from(value > 0))
}

/// A mark for a value either side of a centre, like pan, filling all of the bar's cells.
pub fn centre_bar(value: u8, cells: usize) -> String {
    format!(
        "{:<cells$}",
        format!("{}│", " ".repeat(bar_cell(value, cells)))
    )
}

/// What a value is playing with, as a report says, moved by as much as its setting has moved
/// since the version the report was of: what the setting was then, and is now. A report is a
/// frame or so behind a setting being changed, so the fill would trail the hand and a mark for
/// the setting would flicker at its edge; this way it follows the hand at once, and what
/// modulation adds keeps moving it (#421).
pub fn moved_since(live: u8, then: u8, now: u8) -> u8 {
    (i16::from(live) + i16::from(now) - i16::from(then)).clamp(0, 255) as u8
}

/// The cell of a bar a value's mark goes in.
fn bar_cell(value: u8, cells: usize) -> usize {
    usize::from(value) * cells / 256
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_draws_its_ends_and_fill_into_an_area() {
        let area = Rect::new(0, 0, 12, 1);
        let mut buf = Buffer::empty(area);
        ValueBar::new(0x80, 9).render(area, &mut buf);
        let text: String = (0..12).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(text, "▕████▌    ▏ ");
        assert_eq!(buf[(0, 0)].fg, theme::OVERLAY0);
        assert_eq!(buf[(1, 0)].fg, theme::MAUVE);
    }

    #[test]
    fn a_narrow_area_cuts_a_bar_at_its_edge() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        ValueBar::new(0xFF, 5).render(area, &mut buf);
        let text: String = (0..4).map(|x| buf[(x, 0)].symbol()).collect();
        assert_eq!(text, "▕███");
    }
}
