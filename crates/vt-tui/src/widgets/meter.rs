//! A meter's cells, a row at a time: a level rising from the bottom, as the main meter's and the
//! mixer's bars do, an amount hanging down from the top, as gain reduction does, or a level going
//! across, as the Sound page's Output meter does, each cell filled in eighths. The FX page's
//! reduction meters and the project page's groove use them too.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::services::meter::{FLOOR_DB, db_at, height_of};
use crate::widgets::theme;

/// Partial blocks, from one eighth of a cell up to a whole one.
pub(crate) const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A bar of one channel for one row, counting rows up from the bottom, some cells wide.
pub(crate) fn level_cell(
    row: usize,
    rows: usize,
    average: f32,
    peak: f32,
    top: f32,
    width: usize,
) -> Span<'static> {
    let eighths = |db: f32| (height_of(db) * (rows * 8) as f32).round() as usize;
    let start = row * 8;
    let solid = eighths(average).saturating_sub(start).min(8);
    let light = eighths(peak).saturating_sub(start).min(8);
    let height = (row as f32 + 0.5) / rows as f32;
    let bright = theme::level(db_at(height));
    let dimmed = theme::dimmed(bright);
    let held = eighths(top);
    let (symbol, fg, bg) = if solid == 8 {
        ('█', bright, theme::MANTLE)
    } else if light == 0 {
        // Only the marked top goes above the bar.
        let marked = top > FLOOR_DB && held.saturating_sub(1) / 8 == row;
        (if marked { '▔' } else { ' ' }, bright, theme::MANTLE)
    } else if solid > 0 {
        let behind = if light == 8 { dimmed } else { theme::MANTLE };
        (EIGHTHS[solid], bright, behind)
    } else {
        (EIGHTHS[light], dimmed, theme::MANTLE)
    };
    Span::styled(symbol.to_string().repeat(width), Style::new().fg(fg).bg(bg))
}

/// How much of a row a bar hanging down from the top fills, in eighths, for a share of its whole
/// height from 0 to 1, counting rows down from the top.
pub(crate) fn hanging_eighths(share: f32, row: usize, rows: usize) -> usize {
    let total = share.clamp(0.0, 1.0) * (rows * 8) as f32;
    (total.round() as usize).saturating_sub(row * 8).min(8)
}

/// One row of a bar hanging down from the top, filled this many eighths, on a background.
pub(crate) fn hanging_cell(
    filled: usize,
    color: Color,
    behind: Color,
    width: usize,
) -> Span<'static> {
    // There are no blocks that hang from the top, so a partial one is drawn the other way around.
    let (symbol, fg, bg): (char, Color, Color) = match filled {
        0 => (' ', color, behind),
        8 => ('█', color, behind),
        part => (EIGHTHS[8 - part], behind, color),
    };
    Span::styled(symbol.to_string().repeat(width), Style::new().fg(fg).bg(bg))
}

/// Partial blocks filling a cell from the left, from one eighth of it to seven.
const LEFT_EIGHTHS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

/// An amount as a bar going right in one colour, filled in eighths, for a share of its whole length
/// from 0 to 1, this many cells long: a gain reduction meter laid on its side.
pub(crate) fn amount_across(share: f32, cells: usize, style: Style) -> Span<'static> {
    let total = (share.clamp(0.0, 1.0) * (cells * 8) as f32).round() as usize;
    let text: String = (0..cells)
        .map(|cell| match total.saturating_sub(cell * 8).min(8) {
            0 => ' ',
            8 => '█',
            part => LEFT_EIGHTHS[part - 1],
        })
        .collect();
    Span::styled(text, style)
}

/// A level as a bar across, a character for each `per_cell` dB up from `floor`, each filled in
/// eighths and in the level colour at its middle, with a faint mark in the cell of the highest it
/// was lately, `held`, while the bar is below it.
pub(crate) fn level_across(
    db: f32,
    held: f32,
    floor: f32,
    per_cell: f32,
    cells: usize,
) -> Vec<Span<'static>> {
    let eighths = |db: f32| ((db - floor) / per_cell * 8.0).round().max(0.0) as usize;
    let filled = eighths(db);
    let marked = (held > floor).then(|| (eighths(held).max(1) - 1) / 8);
    (0..cells)
        .map(|cell| {
            let color = theme::level(floor + per_cell * (cell as f32 + 0.5));
            match filled.saturating_sub(cell * 8).min(8) {
                8 => Span::styled("█", Style::new().fg(color)),
                0 if marked == Some(cell) => Span::styled("▏", theme::faint()),
                0 => Span::raw(" "),
                part => Span::styled(LEFT_EIGHTHS[part - 1].to_string(), Style::new().fg(color)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use ratatui::text::{Line, Text};

    use super::*;
    use crate::widgets::pin::{assert_drawn, drawn};

    #[test]
    fn a_level_rises_from_the_bottom_with_its_peak_dimmer_and_its_top_marked() {
        // Four rows, top first: the held top, the peak above the average, the average.
        let rows: Vec<Line> = (0..4)
            .rev()
            .map(|row| Line::from(level_cell(row, 4, -30.0, -12.0, -3.0, 2)))
            .collect();
        assert_drawn(
            &drawn(Text::from(rows), 2, 4),
            &[
                "▔▔",
                "aa",
                "▄▄",
                "bb",
                "▁▁",
                "cc",
                "██",
                "dd",
                "a fg RED bg MANTLE",
                "b fg #737865 bg MANTLE",
                "c fg GREEN bg #657a64",
                "d fg GREEN bg MANTLE",
            ],
        );
    }

    #[test]
    fn an_amount_hangs_from_the_top_in_eighths() {
        let rows: Vec<Line> = (0..3)
            .map(|row| {
                let filled = hanging_eighths(0.5, row, 3);
                Line::from(hanging_cell(filled, theme::PEACH, theme::MANTLE, 1))
            })
            .collect();
        assert_drawn(
            &drawn(Text::from(rows), 1, 3),
            &[
                "█",
                "a",
                "▄",
                "b",
                " ",
                "a",
                "a fg PEACH bg MANTLE",
                "b fg MANTLE bg PEACH",
            ],
        );
    }

    #[test]
    fn an_amount_and_a_level_go_across_in_eighths() {
        let lines = vec![
            Line::from(amount_across(0.45, 6, Style::new().fg(theme::PEACH))),
            Line::from(level_across(-20.0, -6.0, -48.0, 6.0, 8)),
        ];
        assert_drawn(
            &drawn(Text::from(lines), 8, 2),
            &[
                "██▊     ",
                "aaaaaa..",
                "████▋ ▏ ",
                "bbbbb.c.",
                "a fg PEACH",
                "b fg GREEN",
                "c fg OVERLAY0",
            ],
        );
    }
}
