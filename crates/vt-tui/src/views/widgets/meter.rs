//! A meter's cells, a row at a time: a level rising from the bottom, as the main meter's and the
//! mixer's bars do, an amount hanging down from the top, as gain reduction does, or a level going
//! across, as the Sound page's Output meter does, each cell filled in eighths. The FX page's
//! reduction meters and the project page's groove use them too.

use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::meter::{FLOOR_DB, db_at, height_of};
use crate::theme;

/// Partial blocks, from one eighth of a cell up to a whole one.
pub const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A bar of one channel for one row, counting rows up from the bottom, some cells wide.
pub fn level_cell(
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
pub fn hanging_eighths(share: f32, row: usize, rows: usize) -> usize {
    let total = share.clamp(0.0, 1.0) * (rows * 8) as f32;
    (total.round() as usize).saturating_sub(row * 8).min(8)
}

/// One row of a bar hanging down from the top, filled this many eighths, on a background.
pub fn hanging_cell(filled: usize, color: Color, behind: Color, width: usize) -> Span<'static> {
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

/// A level as a bar across, a character for each `per_cell` dB up from `floor`, each filled in
/// eighths and in the level colour at its middle, with a faint mark in the cell of the highest it
/// was lately, `held`, while the bar is below it.
pub fn level_across(
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
