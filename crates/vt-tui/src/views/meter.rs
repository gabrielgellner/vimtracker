//! The main output meter: a column on the right of the screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::meter::{FLOOR_DB, Meter, height_of};
use crate::theme;
use crate::views::widgets::image;
use crate::views::widgets::meter::{hanging_cell, hanging_eighths, level_cell};

/// The columns the meter takes: a gap, scale numbers, then L, R and G bars with gaps between them.
const WIDTH: u16 = 13;
/// With less room than this, the view keeps all of it.
const NARROWEST_SCREEN: u16 = 60;
/// With fewer rows than this, the bars wouldn't say much.
const LOWEST_METER: u16 = 6;
/// The rows that aren't bars: the heading, the readout and the bar names.
const ROWS_AROUND_BARS: u16 = 3;
/// Scale marks, most wanted first, since a short meter can't fit them all.
const MARKS: [i32; 9] = [0, 6, -12, -24, -6, -48, -18, -36, -60];
/// The most gain reduction the G bar can show, in dB.
const MOST_REDUCTION: f32 = 12.0;
/// The scope's size in characters. It sits above the meter, after the meter's gap on the left.
pub const SCOPE_WIDTH: usize = 12;
pub const SCOPE_HEIGHT: usize = 3;
/// The rows the scope takes from the meter's column: itself, and a gap below it.
const SCOPE_ROWS: u16 = SCOPE_HEIGHT as u16 + 1;

/// Split the space for a view into what the view gets and what the meter gets, if there is room.
pub fn split(body: Rect) -> (Rect, Option<Rect>) {
    if body.width < NARROWEST_SCREEN || body.height < LOWEST_METER {
        return (body, None);
    }
    let [view, meter] =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(WIDTH)]).areas(body);
    (view, Some(meter))
}

/// Split the meter's column into the scope at the top, if there's room for it, and the meter.
pub fn split_scope(column: Rect) -> (Option<Rect>, Rect) {
    if column.height < LOWEST_METER + SCOPE_ROWS {
        return (None, column);
    }
    let scope = Rect::new(
        column.x + 1,
        column.y,
        SCOPE_WIDTH as u16,
        SCOPE_HEIGHT as u16,
    );
    let meter = Rect {
        y: column.y + SCOPE_ROWS,
        height: column.height - SCOPE_ROWS,
        ..column
    };
    (Some(scope), meter)
}

/// Draw the scope: the placeholders for its image.
pub fn draw_scope(frame: &mut Frame, area: Rect, id: u32) {
    let lines: Vec<Line> = image::rows(id, SCOPE_WIDTH, SCOPE_HEIGHT)
        .into_iter()
        .map(Line::from)
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// The G bar for one row, hanging down from the top, counting rows down from the top.
fn reduction_cell(row: usize, rows: usize, reduction: f32) -> Span<'static> {
    let filled = hanging_eighths(reduction / MOST_REDUCTION, row, rows);
    hanging_cell(filled, theme::RED, theme::MANTLE, 2)
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
pub fn draw(frame: &mut Frame, area: Rect, meter: &Meter, limiter: bool) {
    let rows = usize::from(area.height.saturating_sub(ROWS_AROUND_BARS));
    if rows == 0 {
        return;
    }
    let peak = if meter.top() > FLOOR_DB {
        // Adding zero turns a rounded -0 into 0.
        let top = meter.top().round() + 0.0;
        if top > 0.0 {
            format!("{top:+.0}")
        } else {
            format!("{top:.0}")
        }
    } else {
        "--".to_owned()
    };
    let reduction = meter.reduction.top;
    // The limiter should never be working, so any amount at all is a warning and reads at least 1.
    let reduction_style = if reduction > 0.0 {
        theme::warning()
    } else {
        theme::dim()
    };
    let mut lines = vec![
        Line::styled("     Main", Style::new().add_modifier(Modifier::BOLD)),
        Line::from(vec![
            Span::raw(format!("     {peak:>5} ")),
            Span::styled(
                if limiter && reduction > 0.0 {
                    format!("{:>2.0}", reduction.round().max(1.0))
                } else if limiter {
                    " 0".to_owned()
                } else {
                    "--".to_owned()
                },
                reduction_style,
            ),
        ]),
    ];
    let labels = marks(rows);
    for row in (0..rows).rev() {
        let label = labels[row].map_or(String::new(), |mark| mark.to_string());
        let [left, right] = meter
            .channels
            .map(|c| level_cell(row, rows, c.average(), c.peak.now, c.peak.top, 2));
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
    use crate::meter::TOP_DB;

    const HEIGHT: u16 = 22;

    fn screen(meter: &Meter) -> ratatui::buffer::Buffer {
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), meter, true))
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
                    power: [before * before; 2],
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
        assert!(text(&buffer, 2).starts_with("   6 "));
        for y in 2..HEIGHT - 1 {
            assert!(!text(&buffer, y).contains('█'), "row {y}");
        }
    }

    #[test]
    fn a_loud_signal_fills_up_the_scale_and_shows_the_limiter() {
        let buffer = screen(&played(1.0, 0.76));
        assert_eq!(text(&buffer, 1).trim(), "0  2");
        assert_eq!(buffer[(12, 1)].fg, theme::RED);
        // The bars reach 0, with room above it, and the G bar starts from the top.
        assert_eq!(buffer[(5, 5)].symbol(), "█");
        assert_ne!(buffer[(5, 2)].symbol(), "█");
        assert_eq!(buffer[(11, 2)].symbol(), "█");
        assert_eq!(buffer[(11, HEIGHT - 2)].symbol(), " ");
    }

    #[test]
    fn the_readout_shows_how_far_over_full_level_it_is() {
        let buffer = screen(&played(4.0, 1.0));
        assert_eq!(text(&buffer, 1).trim(), "+12 12");
        assert_eq!(buffer[(5, 2)].symbol(), "█", "the bar is at the top");
        assert_eq!(buffer[(5, 2)].fg, theme::level(TOP_DB));
        assert_eq!(theme::level(TOP_DB), theme::RED);
        let buffer = screen(&played(0.998, 0.998));
        assert_eq!(text(&buffer, 1).trim(), "0  0", "not -0");
    }

    #[test]
    fn any_limiting_at_all_is_a_warning() {
        let buffer = screen(&played(1.01, 1.0));
        assert_eq!(text(&buffer, 1).trim(), "0  1", "under 1 dB still shows");
        assert_eq!(buffer[(12, 1)].fg, theme::RED);
    }

    #[test]
    fn with_the_limiter_off_g_says_so() {
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, HEIGHT)).unwrap();
        terminal
            .draw(|frame| draw(frame, frame.area(), &played(2.0, 2.0), false))
            .unwrap();
        let buffer = terminal.backend().buffer();
        assert_eq!(text(buffer, 1).trim(), "+6 --");
    }

    #[test]
    fn a_clean_signal_keeps_the_g_readout_quiet() {
        let buffer = screen(&played(0.25, 0.25));
        assert_eq!(text(&buffer, 1).trim(), "-12  0");
        assert_eq!(buffer[(12, 1)].fg, theme::OVERLAY2);
        assert_eq!(buffer[(5, HEIGHT - 2)].symbol(), "█");
        assert_eq!(buffer[(5, 2)].symbol(), " ");
    }

    #[test]
    fn the_bars_change_colour_up_the_scale() {
        let buffer = screen(&played(2.0, 1.0));
        assert_eq!(buffer[(5, 2)].fg, theme::RED);
        assert_eq!(buffer[(5, HEIGHT - 2)].fg, theme::GREEN);
    }

    #[test]
    fn it_gives_way_on_a_small_screen() {
        assert!(split(Rect::new(0, 0, 100, 24)).1.is_some());
        let small = Rect::new(0, 0, 50, 24);
        assert_eq!(split(small), (small, None));
    }

    #[test]
    fn the_scope_sits_above_the_meter_when_theres_room() {
        let column = Rect::new(107, 0, WIDTH, 22);
        let (scope, meter) = split_scope(column);
        assert_eq!(scope, Some(Rect::new(108, 0, 12, 3)));
        assert_eq!(meter, Rect::new(107, 4, WIDTH, 18));
        let short = Rect::new(107, 0, WIDTH, 9);
        assert_eq!(split_scope(short), (None, short));
    }

    #[test]
    fn every_row_has_one_mark_at_most_and_the_top_is_six() {
        let labels = marks(18);
        assert_eq!(labels[17], Some(6));
        assert!(labels.contains(&Some(0)));
        assert!(labels.iter().flatten().count() >= 7);
        assert_eq!(marks(1), [Some(0)], "0 is the mark worth keeping");
    }
}
