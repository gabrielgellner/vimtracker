//! A device's settings grid: up to four columns of three rows, with a blank line between rows, and
//! holes where the device has nothing. Each cell is a setting's name, then its value and its bar,
//! or a word or a choice across both. The grid reads what each cell shows through
//! [`CellSource`], so it knows nothing of the page it's on or the app around it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Widget;

use crate::graphics;
use crate::widgets::bar::{BAR_ENDS, ValueBar};
use crate::widgets::theme;

/// How far apart the grid's rows are: a row, then a blank line.
pub const ROW_STEP: u16 = 2;
/// How many lines a grid of three rows takes.
pub const GRID_HEIGHT: u16 = 2 * ROW_STEP + 1;

/// What each cell of a grid shows, for the setting it holds. It's one method, so a cell's parts
/// are worked out together, once.
pub trait CellSource {
    /// What names a setting.
    type Id: Copy;
    /// Its name, value and bar, in cells of these widths.
    fn cell(&self, id: Self::Id, widths: CellWidths) -> CellParts;
}

/// The parts of a setting's cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellParts {
    /// Its name, dim, with any labels a jump has put on it, in the name's room.
    pub name: Line<'static>,
    /// Its value, in the cursor's colour under the cursor, or what's being typed in its place.
    pub value: Line<'static>,
    /// Its bar, or nothing for a word or a choice, whose value then takes the bar's room too.
    pub bar: Option<ValueBar>,
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

    /// The FX page's zoomed in on a slot: the same, with bars nearly twice as long, 17 cells, so
    /// four columns fill the device's width in the keyboard's box. That's 35 columns a cell.
    pub const ZOOM: Self = Self {
        bar_cells: 17,
        ..Self::FX
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
                let CellParts { name, value, bar } = self.source.cell(id, widths);
                put(name, 0, widths.name);
                match bar {
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
                if span.style == keep || span.content.starts_with(graphics::PLACEHOLDER) {
                    span
                } else {
                    span.style(theme::faint())
                }
            })
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use ratatui::text::Span;

    use super::*;
    use crate::input::mode::Mode;
    use crate::widgets::pin::{assert_drawn, drawn};

    /// Made-up cells: 0 under the cursor, 1 dim at its default, 2 a word with no bar, 3 a centred
    /// bar playing away from where it's set, 4 a picture's placeholder.
    struct Made;

    impl CellSource for Made {
        type Id = u8;

        fn cell(&self, id: u8, _widths: CellWidths) -> CellParts {
            CellParts {
                name: Line::styled(format!("Name {id}"), theme::dim()),
                value: Self::value(id),
                bar: Self::bar(id),
            }
        }
    }

    impl Made {
        fn value(id: u8) -> Line<'static> {
            match id {
                0 => Line::from(Span::styled("A0", theme::cursor(Mode::Normal))),
                1 => Line::from(Span::styled("00", theme::dim())),
                2 => Line::raw("One-shot and on"),
                4 => Line::from(Span::styled(
                    graphics::PLACEHOLDER.to_string(),
                    Style::new().fg(graphics::id_color(7)),
                )),
                _ => Line::raw("C0"),
            }
        }

        fn bar(id: u8) -> Option<ValueBar> {
            match id {
                0 => Some(ValueBar::new(0xA0, 9)),
                1 => Some(ValueBar::new(0, 9)),
                3 => Some(ValueBar::new(0xC0, 9).centred(true).playing(0x40)),
                _ => None,
            }
        }
    }

    fn grid(widths: CellWidths) -> SettingsGrid<'static, Made> {
        let rows = vec![
            vec![Some(0), Some(1)],
            vec![None, Some(2)],
            vec![Some(3), Some(4)],
        ];
        SettingsGrid::new(&Made, rows, widths)
    }

    #[test]
    fn each_width_makes_a_column_this_wide() {
        let columns = [
            CellWidths::FX,
            CellWidths::ZOOM,
            CellWidths::SOUND,
            CellWidths::COMPACT,
        ]
        .map(CellWidths::column);
        assert_eq!(columns, [27, 35, 29, 21]);
    }

    #[test]
    fn a_grid_puts_each_cells_name_value_and_bar_in_its_column_and_steps_over_holes() {
        let width = 2 * CellWidths::FX.column();
        assert_drawn(
            &drawn(grid(CellWidths::FX), width, GRID_HEIGHT),
            &[
                "Name 0    A0 ▕█████▋   ▏   Name 1    00 ▕         ▏   ",
                "aaaaaaaaaabb.cdddddddddc...aaaaaaaaaaaa.cdddddddddc...",
                "                                                      ",
                "......................................................",
                "                           Name 2    One-shot and on  ",
                "...........................aaaaaaaaaa.................",
                "                                                      ",
                "......................................................",
                "Name 3    C0 ▕  │ ┊ │  ▏   Name 4    ▒                ",
                "aaaaaaaaaa...cddddcdcddc...aaaaaaaaaae................",
                "a fg OVERLAY2",
                "b fg BASE bg ROSEWATER",
                "c fg OVERLAY0",
                "d fg MAUVE",
                "e fg #000007",
            ],
        );
    }

    #[test]
    fn a_grid_is_cut_at_its_right_edge_and_its_bottom() {
        assert_drawn(
            &drawn(grid(CellWidths::FX), 40, 3),
            &[
                "Name 0    A0 ▕█████▋   ▏   Name 1    00 ",
                "aaaaaaaaaabb.cdddddddddc...aaaaaaaaaaaa.",
                "                                        ",
                "........................................",
                "                           Name 2    One",
                "...........................aaaaaaaaaa...",
                "a fg OVERLAY2",
                "b fg BASE bg ROSEWATER",
                "c fg OVERLAY0",
                "d fg MAUVE",
            ],
        );
    }

    #[test]
    fn a_compact_grid_has_no_bars_and_a_word_runs_on_short_of_the_next_column() {
        let width = 2 * CellWidths::COMPACT.column();
        assert_drawn(
            &drawn(grid(CellWidths::COMPACT), width, GRID_HEIGHT),
            &[
                "Name 0    A0         Name 1    00         ",
                "aaaaaaaaaabb.........aaaaaaaaaaaa.........",
                "                                          ",
                "..........................................",
                "                     Name 2    One-shot a ",
                ".....................aaaaaaaaaa...........",
                "                                          ",
                "..........................................",
                "Name 3    C0         Name 4    ▒          ",
                "aaaaaaaaaa...........aaaaaaaaaac..........",
                "a fg OVERLAY2",
                "b fg BASE bg ROSEWATER",
                "c fg #000007",
            ],
        );
    }

    #[test]
    fn a_faded_grid_keeps_the_cursor_and_a_pictures_colour() {
        let faded = grid(CellWidths::FX).faded(Some(theme::cursor(Mode::Normal)));
        let width = 2 * CellWidths::FX.column();
        assert_drawn(
            &drawn(faded, width, GRID_HEIGHT),
            &[
                "Name 0    A0 ▕█████▋   ▏   Name 1    00 ▕         ▏   ",
                "aaaaaa....bb.aaaaaaaaaaa...aaaaaa....aa.aaaaaaaaaaa...",
                "                                                      ",
                "......................................................",
                "                           Name 2    One-shot and on  ",
                "...........................aaaaaa....aaaaaaaaaaaaaaa..",
                "                                                      ",
                "......................................................",
                "Name 3    C0 ▕  │ ┊ │  ▏   Name 4    ▒                ",
                "aaaaaa....aa.aaaaaaaaaaa...aaaaaa....c................",
                "a fg OVERLAY0",
                "b fg BASE bg ROSEWATER",
                "c fg #000007",
            ],
        );
    }
}
