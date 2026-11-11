//! A device's settings grid: up to four columns of three rows, with a blank line between rows, and
//! holes where the device has nothing. Each cell is a setting's name, then its value and its bar,
//! or a word or a choice across both. The grid reads what each cell shows through
//! [`CellSource`], so it knows nothing of the page it's on or the app around it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::kitty;
use crate::theme;
use crate::views::widgets::bar::{BAR_ENDS, ValueBar};

/// How far apart the grid's rows are: a row, then a blank line.
pub const ROW_STEP: u16 = 2;
/// How many lines a grid of three rows takes.
pub const GRID_HEIGHT: u16 = 2 * ROW_STEP + 1;

/// What each cell of a grid shows, for the setting it holds.
pub trait CellSource {
    /// What names a setting.
    type Id: Copy;
    /// Its name, dim, with any labels a jump has put on it, in this much room.
    fn name(&self, id: Self::Id, room: u16) -> Line<'static>;
    /// Its value, in the cursor's colour under the cursor, or what's being typed in its place.
    fn value(&self, id: Self::Id) -> Line<'static>;
    /// Its bar, or nothing for a word or a choice, whose value then takes the bar's room too.
    fn bar(&self, id: Self::Id) -> Option<ValueBar>;
}

/// The room each part of a cell has, so the columns line up down a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellWidths {
    /// A setting's name and the space after it.
    pub name: u16,
    /// Its value and the space after it.
    pub value: u16,
    /// The cells inside a bar, each split into eighths, or none for a compact cell with no bar.
    pub bar_cells: usize,
    /// The gap before the next column.
    pub gap: u16,
}

impl CellWidths {
    /// The FX page's: two hex digits, and the Sound page's 9-cell bars, so a bar looks the same on
    /// both. That's 27 columns a cell.
    pub const FX: Self = Self {
        name: 10,
        value: 3,
        bar_cells: 9,
        gap: 3,
    };

    /// The Sound page's: four digits, a value's coarse ones and its fine ones set back, as `A000`,
    /// with the same bars, so a column lines up with the FX page's but for its value. That's 29
    /// columns a cell.
    pub const SOUND: Self = Self {
        value: 5,
        ..Self::FX
    };

    /// Compact cells, for a half-width device, where a panel rather than a bar shows the values:
    /// a name, a value as long as `2 bars` or `8000 ms`, and no bar. That's 21 columns a cell.
    pub const COMPACT: Self = Self {
        name: 10,
        value: 8,
        bar_cells: 0,
        gap: 3,
    };

    /// The bar with its ends, or nothing in a compact cell, which has none.
    fn bar(self) -> u16 {
        match self.bar_cells {
            0 => 0,
            cells => cells as u16 + BAR_ENDS,
        }
    }

    /// A whole column, with its gap.
    pub fn column(self) -> u16 {
        self.name + self.value + self.bar() + self.gap
    }
}

/// A grid of settings, read through a source, a row of places for each of its rows.
pub struct SettingsGrid<'a, S: CellSource> {
    source: &'a S,
    rows: Vec<Vec<Option<S::Id>>>,
    widths: CellWidths,
    fade: Option<Style>,
}

impl<'a, S: CellSource> SettingsGrid<'a, S> {
    pub fn new(source: &'a S, rows: Vec<Vec<Option<S::Id>>>, widths: CellWidths) -> Self {
        Self {
            source,
            rows,
            widths,
            fade: None,
        }
    }

    /// The same grid faint, as a device switched out is, but for what's in this style, the
    /// cursor's.
    #[must_use]
    pub fn faded(self, keep: Option<Style>) -> Self {
        Self { fade: keep, ..self }
    }
}

impl<S: CellSource> Widget for SettingsGrid<'_, S> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let widths = self.widths;
        let fade = |line: Line<'static>| match self.fade {
            Some(keep) => faded(line, keep),
            None => line,
        };
        for (row, places) in self.rows.iter().enumerate() {
            let y = area.y + row as u16 * ROW_STEP;
            if y >= area.bottom() {
                break;
            }
            for (column, place) in places.iter().enumerate() {
                let Some(id) = *place else {
                    continue;
                };
                let x = area.x + column as u16 * widths.column();
                // A part of the cell, cut at the grid's right edge.
                let mut put = |line: Line<'static>, from: u16, width: u16| {
                    let left = x + from;
                    if left < area.right() {
                        let width = width.min(area.right() - left);
                        fade(line).render(Rect::new(left, y, width, 1), buf);
                    }
                };
                put(self.source.name(id, widths.name), 0, widths.name);
                let value = self.source.value(id);
                match self.source.bar(id) {
                    Some(bar) => {
                        put(value, widths.name, widths.value);
                        put(Line::from(bar), widths.name + widths.value, widths.bar());
                    }
                    // A word or a choice can run on into the gap, short of the next column's
                    // name, as a slice's time needs.
                    None => put(
                        value,
                        widths.name,
                        widths.value + widths.bar() + widths.gap.saturating_sub(1),
                    ),
                }
            }
        }
    }
}

/// A line faint, as a device switched out is, but for what's in the style to keep, the cursor's.
/// An image's placeholders keep their colour, which says which image they are.
pub fn faded(line: Line<'static>, keep: Style) -> Line<'static> {
    Line::from(
        line.spans
            .into_iter()
            .map(|span| {
                if span.style == keep || span.content.starts_with(kitty::PLACEHOLDER) {
                    span
                } else {
                    span.style(theme::faint())
                }
            })
            .collect::<Vec<_>>(),
    )
}
