//! A column's values as bars, one a row, beside the column on the phrase grid, as the Cirklon
//! draws a lane's values: what each row holds now solid, filled in eighths of a cell, and what it
//! held before faint behind it, so a change across the rows can be seen at a glance. A note's
//! pitch is a contour instead, a mark a row placed by pitch, over faint lines on the scale's notes.
//! Over them, the column's name and its scale, and a line saying the values' lowest, highest and
//! average, and how many rows changed.
//!
//! They go right of the column, over whatever is there, or left of it where the right has no room;
//! [`Beside`] works out where.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Widget};
use vt_model::scale::Scale;
use vt_model::selection::StepRange;
use vt_model::shape::ShapeColumn;
use vt_model::step::Step;

use crate::widgets::theme;

/// How many cells the bars take, each split into eighths: 176 steps across a column's range,
/// wide enough for the summary over them to say how many rows changed in words.
pub(crate) const BARS: u16 = 22;
/// The whole box: the bars with a clear cell either side, so they don't touch the column.
pub(crate) const WIDTH: u16 = BARS + 2;
/// The rows over the bars: the name and scale, then the summary.
const HEADER_ROWS: u16 = 2;
/// The characters for a cell partly full, from one eighth to seven.
const EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];
/// A row's note on the contour.
const MARK: &str = "●";
/// The faint line on the contour at each of the scale's roots, and the dot on its other notes.
const ROOT_LINE: &str = "┊";
const SCALE_DOT: &str = "·";

/// What's now: the bright colour a value's bar has on the pages.
const NOW: Color = theme::MAUVE;

/// What was before: the same colour set back.
fn before_colour() -> Color {
    theme::dimmed(NOW)
}

/// One row of the column: its step, and its value before and now, if it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BarRow {
    pub step: usize,
    pub before: Option<u8>,
    pub now: Option<u8>,
}

/// A column's values as bars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValueBars {
    /// The column's name as the grid shows it: `VEL`, a command's label.
    pub label: String,
    pub column: ShapeColumn,
    /// The rows that have bars, in order. Any other row is left blank.
    pub rows: Vec<BarRow>,
}

impl ValueBars {
    /// The bars for rows of a column, or none for a column whose values aren't amounts.
    pub(crate) fn new(label: String, column: ShapeColumn, rows: Vec<BarRow>) -> Option<Self> {
        column.measured().then_some(Self {
            label,
            column,
            rows,
        })
    }

    /// The bars of a column over a span of rows, each its value in `before` and in `now`, the
    /// phrase's rows as they were and are: a selection's, and Gen's device beside the grid
    /// (decision 468). None for a column whose values aren't amounts.
    pub(crate) fn of_steps(
        column: ShapeColumn,
        range: StepRange,
        before: &[Step],
        now: &[Step],
    ) -> Option<Self> {
        let rows = (range.first..=range.last)
            .filter_map(|step| {
                Some(BarRow {
                    step,
                    before: before.get(step).and_then(|was| column.value(was)),
                    now: column.value(now.get(step)?),
                })
            })
            .collect();
        Self::new(Self::label_of(column), column, rows)
    }

    /// The name a column has over its bars: the grid's for an FX command, and otherwise the
    /// column's own, short.
    pub(crate) fn label_of(column: ShapeColumn) -> String {
        match column {
            ShapeColumn::Note { .. } => "NOTE".to_owned(),
            ShapeColumn::Velocity => "VEL".to_owned(),
            ShapeColumn::Nudge { .. } => "NDG".to_owned(),
            ShapeColumn::Length => "GATE".to_owned(),
            ShapeColumn::Fx { command, .. } => command.name(),
        }
    }

    /// The scale the bars are drawn on, its lowest value and its highest: the column's own, or
    /// for a note's contour, the notes there before and now, at least an octave.
    fn range(&self) -> (u8, u8) {
        let ShapeColumn::Note { .. } = self.column else {
            return (self.column.lowest(), self.column.highest());
        };
        let notes = self.rows.iter().flat_map(|row| [row.before, row.now]);
        let (low, high) = notes
            .flatten()
            .fold(None, |range: Option<(u8, u8)>, note| {
                Some(range.map_or((note, note), |(low, high)| (low.min(note), high.max(note))))
            })
            .unwrap_or((60, 72));
        // Widened about its middle to an octave, so a small move doesn't look a large one.
        let short = 12_u8.saturating_sub(high - low);
        let low = low.saturating_sub(short / 2).max(self.column.lowest());
        let high = (low + 12).max(high).min(self.column.highest());
        (low, high)
    }

    /// The name and the scale: `VEL 00 ─ 7F`.
    fn header(&self) -> Line<'static> {
        let (low, high) = self.range();
        Line::from(vec![
            Span::styled(
                self.label.clone(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" {} ─ {}", self.column.show(low), self.column.show(high)),
                theme::dim(),
            ),
        ])
    }

    /// The values now, lowest to highest, their average, and how many rows changed:
    /// `20─7F avg 4C 6 changed`. Where that's too long, the average is `~4C`, and then it's left
    /// out.
    fn summary(&self) -> String {
        let changed = self.rows.iter().filter(|row| row.before != row.now).count();
        let changed = format!("{changed} changed");
        let now: Vec<u8> = self.rows.iter().filter_map(|row| row.now).collect();
        let (Some(low), Some(high)) = (now.iter().min(), now.iter().max()) else {
            return format!("none {changed}");
        };
        let sum: usize = now.iter().map(|value| usize::from(*value)).sum();
        let average = ((sum + now.len() / 2) / now.len()) as u8;
        let show = |value: u8| self.column.show(value);
        let range = format!("{}─{}", show(*low), show(*high));
        let average = show(average);
        [
            format!("{range} avg {average} {changed}"),
            format!("{range} ~{average} {changed}"),
        ]
        .into_iter()
        .find(|line| line.chars().count() <= usize::from(BARS))
        .unwrap_or_else(|| format!("{range} {changed}"))
    }

    /// A row's bar: what's now solid, and what was before set back behind it where it's longer.
    fn level(&self, row: BarRow) -> Vec<Span<'static>> {
        let (low, high) = self.range();
        let eighths = |value: Option<u8>| value.map_or(0, |value| eighths(value, low, high));
        let (now, before) = (eighths(row.now), eighths(row.before));
        let bright = Style::new().fg(NOW);
        let faint = Style::new().fg(before_colour());
        (0..usize::from(BARS))
            .map(|cell| {
                let part = |eighths: usize| eighths.saturating_sub(cell * 8).min(8);
                let (now, before) = (part(now), part(before));
                match (now, before) {
                    (8, _) => Span::styled("█", bright),
                    (0, 8) => Span::styled("█", faint),
                    (0, 0) => Span::raw(" "),
                    (0, before) => Span::styled(EIGHTHS[before - 1].to_string(), faint),
                    (now, before) if before > now => {
                        Span::styled(EIGHTHS[now - 1].to_string(), bright.bg(before_colour()))
                    }
                    (now, _) => Span::styled(EIGHTHS[now - 1].to_string(), bright),
                }
            })
            .collect()
    }

    /// A row's note on the contour: a mark where it is now, one set back where it was, over the
    /// faint lines of the scale.
    fn contour(&self, row: BarRow, scale: Scale) -> Vec<Span<'static>> {
        let (low, high) = self.range();
        let cells = usize::from(BARS);
        let mut spans: Vec<Span<'static>> = vec![Span::raw(" "); cells];
        // A semitone a cell while they fit, centred, and squeezed in when they don't.
        let squeezed = usize::from(high - low) >= cells;
        let place = |note: u8| -> usize {
            let above = usize::from(note - low);
            if squeezed {
                above * (cells - 1) / usize::from(high - low)
            } else {
                above + (cells - 1 - usize::from(high - low)) / 2
            }
        };
        let root = if scale.on { scale.root } else { 0 };
        for note in low..=high {
            let line = if (note % 12) == root % 12 {
                ROOT_LINE
            } else if scale.on && !squeezed && scale.contains(note) {
                SCALE_DOT
            } else {
                continue;
            };
            spans[place(note)] = Span::styled(line, theme::faint());
        }
        if let Some(before) = row.before {
            spans[place(before)] = Span::styled(MARK, Style::new().fg(before_colour()));
        }
        if let Some(now) = row.now {
            spans[place(now)] = Span::styled(MARK, Style::new().fg(NOW));
        }
        spans
    }

    /// A row's cells between the box's clear edges.
    fn row_spans(&self, row: BarRow) -> Vec<Span<'static>> {
        match self.column {
            ShapeColumn::Note { scale, .. } => self.contour(row, scale),
            _ => self.level(row),
        }
    }
}

/// How far a value's bar reaches, in eighths of a cell. Any value over the lowest shows at least
/// an eighth.
fn eighths(value: u8, low: u8, high: u8) -> usize {
    let span = usize::from(high.saturating_sub(low)).max(1);
    let above = usize::from(value.clamp(low, high) - low);
    let cells = usize::from(BARS) * 8;
    ((above * cells + span / 2) / span).max(usize::from(above > 0))
}

/// Which side of its column the bars go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Right,
    Left,
}

/// The bars beside a column on screen: the area the grid is drawn in, the columns the shaped
/// column's cells take, and where each row on screen is, its line's top and its step.
#[derive(Debug, Clone)]
pub(crate) struct Beside {
    pub area: Rect,
    pub column: (u16, u16),
    pub lines: Vec<(u16, usize)>,
}

impl Beside {
    /// The box on one side of the column, from two rows over the first of the bars' rows on
    /// screen, for the header, to the last of them, so the rest of the grid stays in view; or none
    /// where it doesn't fit inside the area. With none of them on screen, it's the header alone,
    /// at the top.
    pub(crate) fn place(&self, bars: &ValueBars, side: Side) -> Option<Rect> {
        let x = match side {
            Side::Right => self.column.1,
            Side::Left => self.column.0.checked_sub(WIDTH)?,
        };
        if x < self.area.x || x + WIDTH > self.area.right() {
            return None;
        }
        let shown: Vec<u16> = self
            .lines
            .iter()
            .filter(|(_, step)| bars.rows.iter().any(|row| row.step == *step))
            .map(|(y, _)| *y)
            .collect();
        let (Some(first), Some(last)) = (shown.iter().min(), shown.iter().max()) else {
            return Some(Rect::new(x, self.area.y, WIDTH, HEADER_ROWS));
        };
        let top = first.saturating_sub(HEADER_ROWS).max(self.area.y);
        let bottom = (last + 1).min(self.area.bottom());
        Some(Rect::new(x, top, WIDTH, bottom.saturating_sub(top)))
    }

    /// Draw the bars in a box [`place`](Self::place) gave.
    pub(crate) fn draw(&self, buf: &mut Buffer, bars: &ValueBars, at: Rect) {
        Clear.render(at, buf);
        buf.set_style(at, theme::window());
        let inside = Rect::new(at.x + 1, at.y, BARS, at.height);
        let mut header = Rect {
            height: 1,
            ..inside
        };
        bars.header().render(header, buf);
        header.y += 1;
        if header.y < at.bottom() {
            Line::styled(bars.summary(), theme::dim()).render(header, buf);
        }
        for (y, step) in &self.lines {
            if *y < at.y + HEADER_ROWS || *y >= at.bottom() {
                continue;
            }
            let Some(row) = bars.rows.iter().find(|row| row.step == *step) else {
                continue;
            };
            let line = Rect { y: *y, ..header };
            Line::from(bars.row_spans(*row)).render(line, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::pin::assert_drawn;

    fn velocity(rows: &[(Option<u8>, Option<u8>)]) -> ValueBars {
        let rows = rows
            .iter()
            .enumerate()
            .map(|(step, (before, now))| BarRow {
                step,
                before: *before,
                now: *now,
            })
            .collect();
        ValueBars::new("VEL".to_owned(), ShapeColumn::Velocity, rows).unwrap()
    }

    fn drawn(bars: &ValueBars, beside: &Beside, side: Side) -> Buffer {
        let mut buf = Buffer::empty(beside.area);
        let at = beside.place(bars, side).unwrap();
        beside.draw(&mut buf, bars, at);
        crate::widgets::pin::cut(&buf, at)
    }

    #[test]
    fn a_velocity_now_is_solid_and_before_is_set_back_behind_it() {
        let bars = velocity(&[
            (Some(0x40), Some(0x7F)),
            (Some(0x40), Some(0x20)),
            (Some(0x40), Some(0x40)),
            (None, Some(0x10)),
        ]);
        let beside = Beside {
            area: Rect::new(0, 0, 40, 8),
            column: (4, 6),
            lines: vec![(2, 0), (3, 1), (4, 2), (5, 3)],
        };
        assert_drawn(
            &drawn(&bars, &beside, Side::Right),
            &[
                " VEL 00 ─ 7F            ",
                "abbbccccccccaaaaaaaaaaaa",
                " 10─7F avg 3C 3 changed ",
                "acccccccccccccccccccccca",
                " ██████████████████████ ",
                "adddddddddddddddddddddda",
                " █████▌█████▏           ",
                "adddddeffffffaaaaaaaaaaa",
                " ███████████▏           ",
                "addddddddddddaaaaaaaaaaa",
                " ██▊                    ",
                "adddaaaaaaaaaaaaaaaaaaaa",
                "a fg TEXT bg MANTLE",
                "b fg TEXT bg MANTLE +BOLD",
                "c fg OVERLAY2 bg MANTLE",
                "d fg MAUVE bg MANTLE",
                "e fg MAUVE bg #75638e",
                "f fg #75638e bg MANTLE",
            ],
        );
    }

    #[test]
    fn the_summary_says_how_many_rows_changed_in_words_and_shortens_the_average_to_fit() {
        assert_eq!(
            velocity(&[(Some(0x40), Some(0x7F)), (Some(0x20), Some(0x20))]).summary(),
            "20─7F avg 50 1 changed"
        );
        let many = vec![(Some(0x00), Some(0x40)); 100];
        assert_eq!(velocity(&many).summary(), "40─40 ~40 100 changed");
        assert_eq!(velocity(&[(Some(0x40), None)]).summary(), "none 1 changed");
    }

    #[test]
    fn a_column_that_isnt_an_amount_has_no_bars() {
        let column = ShapeColumn::Fx {
            slot: 0,
            command: vt_model::step::FxCommand::Cnd,
        };
        assert_eq!(ValueBars::new("CND".to_owned(), column, Vec::new()), None);
    }

    #[test]
    fn the_box_goes_left_of_the_column_or_nowhere_when_it_doesnt_fit() {
        let bars = velocity(&[(Some(0x40), Some(0x7F))]);
        let beside = Beside {
            area: Rect::new(0, 0, 40, 8),
            column: (24, 26),
            lines: vec![(2, 0)],
        };
        assert_eq!(
            beside.place(&bars, Side::Left),
            Some(Rect::new(0, 0, 24, 3))
        );
        let lower = Beside {
            lines: vec![(5, 0)],
            ..beside
        };
        assert_eq!(
            lower.place(&bars, Side::Left),
            Some(Rect::new(0, 3, 24, 3)),
            "from two rows over its first row"
        );
        assert_eq!(beside.place(&bars, Side::Right), None, "26 + 24 is past 40");
    }
}
