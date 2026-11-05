//! The main output meter: a column on the right of the screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::meter::{FLOOR_DB, Meter, db_at, height_of};
use crate::theme;

/// The columns the meter takes: a gap, scale numbers, then L, R and G bars with gaps between them.
const WIDTH: u16 = 13;
/// With less room than this, the view keeps all of it.
const NARROWEST_SCREEN: u16 = 60;
/// With fewer rows than this, the bars wouldn't say much.
const LOWEST_METER: u16 = 6;
/// The rows that aren't bars: the heading, the readout and the bar names.
const ROWS_AROUND_BARS: u16 = 3;
/// Scale marks, most wanted first, since a short meter can't fit them all.
const MARKS: [i32; 9] = [0, -12, -6, -24, -3, -48, -18, -36, -60];
/// The most gain reduction the G bar can show, in dB.
const MOST_REDUCTION: f32 = 12.0;
/// How much gain reduction turns the G readout red, in dB. The limiter is always curving a little,
/// so this is where it starts to be doing real work.
const LIMITING: f32 = 1.0;
/// Partial blocks, from one eighth of a cell up to a whole one.
const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Split the space for a view into what the view gets and what the meter gets, if there is room.
pub fn split(body: Rect) -> (Rect, Option<Rect>) {
    if body.width < NARROWEST_SCREEN || body.height < LOWEST_METER {
        return (body, None);
    }
    let [view, meter] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(WIDTH)]).areas(body);
    (view, Some(meter))
}

/// A bar of one channel for one row, counting rows up from the bottom.
fn level_cell(row: usize, rows: usize, average: f32, peak: f32, top: f32) -> Span<'static> {
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
    Span::styled(symbol.to_string().repeat(2), Style::new().fg(fg).bg(bg))
}

/// The G bar for one row, hanging down from the top, counting rows down from the top.
fn reduction_cell(row: usize, rows: usize, reduction: f32) -> Span<'static> {
    let total = (reduction / MOST_REDUCTION).min(1.0) * (rows * 8) as f32;
    let filled = (total.round() as usize).saturating_sub(row * 8).min(8);
    // There are no blocks that hang from the top, so a partial one is drawn the other way around.
    let color = reduction_color(reduction);
    let (symbol, fg, bg): (char, Color, Color) = match filled {
        0 => (' ', color, theme::MANTLE),
        8 => ('█', color, theme::MANTLE),
        part => (EIGHTHS[8 - part], theme::MANTLE, color),
    };
    Span::styled(symbol.to_string().repeat(2), Style::new().fg(fg).bg(bg))
}

fn reduction_color(reduction: f32) -> Color {
    if reduction >= LIMITING {
        theme::RED
    } else {
        theme::PEACH
    }
}

/// The scale's mark for each row, counting rows up from the bottom.
fn marks(rows: usize) -> Vec<Option<i32>> {
    let mut labels = vec![None; rows];
    for mark in MARKS {
        let row = ((height_of(mark as f32) * rows as f32) as usize).min(rows - 1);
        if labels[row].is_none() {
            labels[row] = Some(mark);
        }
    }
    labels
}

/// Draw the meter.
pub fn draw(frame: &mut Frame, area: Rect, meter: &Meter) {
    let rows = usize::from(area.height.saturating_sub(ROWS_AROUND_BARS));
    if rows == 0 {
        return;
    }
    let peak = if meter.top() > FLOOR_DB {
        // Adding zero turns a rounded -0 into 0.
        format!("{:.0}", meter.top().round() + 0.0)
    } else {
        "--".to_owned()
    };
    let reduction = meter.reduction.top;
    let reduction_style = if reduction >= LIMITING {
        Style::new().fg(theme::RED).add_modifier(Modifier::BOLD)
    } else {
        theme::dim()
    };
    let mut lines = vec![
        Line::styled("     Main", Style::new().add_modifier(Modifier::BOLD)),
        Line::from(vec![
            Span::raw(format!("     {peak:>5} ")),
            Span::styled(format!("{reduction:>2.0}"), reduction_style),
        ]),
    ];
    let labels = marks(rows);
    for row in (0..rows).rev() {
        let label = labels[row].map_or(String::new(), |mark| mark.to_string());
        let [left, right] = meter
            .channels
            .map(|c| level_cell(row, rows, c.average(), c.peak.now, c.peak.top));
        lines.push(Line::from(vec![
            Span::styled(format!(" {label:>3} "), theme::dim()),
            left,
            Span::raw(" "),
            right,
            Span::raw(" "),
            reduction_cell(rows - 1 - row, rows, meter.reduction.now),
        ]));
    }
    lines.push(Line::styled("     L  R  G", theme::dim()));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use vt_engine::mixer::Reading;

    use super::*;

    const HEIGHT: u16 = 22;

    fn screen(meter: &Meter) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), meter))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..WIDTH).map(|x| buffer[(x, y)].symbol()).collect()
    }

    fn played(before: f32, after: f32) -> Meter {
        let mut meter = Meter::default();
        for _ in 0..100 {
            meter.update(
                Some(Reading {
                    before: [before; 2],
                    after: [after; 2],
                    power: [after * after; 2],
                    frames: 1,
                }),
                0.033,
            );
        }
        meter
    }

    #[test]
    fn silence_draws_empty_bars_and_a_blank_readout() {
        let buffer = screen(&Meter::default());
        assert_eq!(text(&buffer, 1).trim(), "--  0");
        assert!(text(&buffer, 2).starts_with("   0 "));
        for y in 2..HEIGHT - 1 {
            assert!(!text(&buffer, y).contains('█'), "row {y}");
        }
    }

    #[test]
    fn a_loud_signal_fills_up_the_scale_and_shows_the_limiter() {
        let buffer = screen(&played(1.0, 0.76));
        assert_eq!(text(&buffer, 1).trim(), "-2  2");
        assert_eq!(buffer[(11, 1)].fg, theme::RED, "2 dB off is real limiting");
        // The bars reach near the top, and the G bar starts from it.
        assert_eq!(buffer[(5, 4)].symbol(), "█");
        assert_eq!(buffer[(11, 2)].symbol(), "█");
        assert_eq!(buffer[(11, HEIGHT - 2)].symbol(), " ");
    }

    #[test]
    fn just_under_full_level_reads_zero() {
        let buffer = screen(&played(4.0, 0.99));
        assert_eq!(text(&buffer, 1).trim(), "0 12");
    }

    #[test]
    fn a_clean_signal_keeps_the_g_readout_quiet() {
        let buffer = screen(&played(0.25, 0.2449));
        assert_eq!(buffer[(12, 1)].fg, theme::OVERLAY2);
        assert_eq!(buffer[(5, HEIGHT - 2)].symbol(), "█");
        assert_eq!(buffer[(5, 2)].symbol(), " ");
    }

    #[test]
    fn the_bars_change_colour_up_the_scale() {
        let buffer = screen(&played(1.0, 1.0));
        assert_eq!(buffer[(5, 2)].fg, theme::level(db_at(1.0 - 0.5 / 19.0)));
        assert_eq!(buffer[(5, HEIGHT - 2)].fg, theme::GREEN);
    }

    #[test]
    fn it_gives_way_on_a_small_screen() {
        assert!(split(Rect::new(0, 0, 100, 24)).1.is_some());
        let small = Rect::new(0, 0, 50, 24);
        assert_eq!(split(small), (small, None));
    }

    #[test]
    fn every_mark_row_is_one_mark_and_the_top_is_zero() {
        let labels = marks(18);
        assert_eq!(labels[17], Some(0));
        assert!(labels.iter().flatten().count() >= 7);
        assert_eq!(marks(1), [Some(0)]);
    }
}
