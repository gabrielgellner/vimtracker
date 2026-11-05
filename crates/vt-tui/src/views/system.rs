//! How the program itself is running: CPU and audio dropouts, at the top right of the view.

use std::sync::atomic::Ordering;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::audio::AudioStats;
use crate::theme;

/// The text, and whether something has gone wrong and needs noticing.
pub fn text(cpu: u64, late: u64, xruns: u64) -> (String, bool) {
    (
        format!("CPU {cpu:>2}%  late {late}  xruns {xruns} "),
        late > 0 || xruns > 0,
    )
}

/// The row to draw on. The top one if it is free, or else the next, as a phrase with many tracks
/// fills the top row with their names.
fn free_row(frame: &mut Frame, area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let at = |y: u16| Rect::new(area.right() - width, y, width, 1);
    let buffer = frame.buffer_mut();
    let top = at(area.y);
    let free = top
        .positions()
        .all(|position| buffer[position].symbol() == " ");
    if free || area.height < 2 {
        top
    } else {
        at(area.y + 1)
    }
}

/// Draw it in the area a view has. Call this after drawing the view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, stats: Option<&AudioStats>) {
    let (late, xruns) = stats.map_or((0, 0), |s| {
        (
            s.late.load(Ordering::Relaxed),
            s.errors.load(Ordering::Relaxed),
        )
    });
    let (text, trouble) = text(app.cpu.percent(), late, xruns);
    let style = if trouble {
        theme::warning()
    } else {
        theme::dim()
    };
    let row = free_row(frame, area, text.len() as u16);
    frame.render_widget(Paragraph::new(text).style(style), row);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::text::Line;

    use super::*;

    const WIDTH: u16 = 40;

    /// Which row is free after some text is drawn on the top row.
    fn row_after(title: &str) -> u16 {
        let mut terminal = Terminal::new(TestBackend::new(WIDTH, 3)).unwrap();
        let mut row = 0;
        terminal
            .draw(|frame| {
                frame.render_widget(Line::raw(title), frame.area());
                row = free_row(frame, frame.area(), 10).y;
            })
            .unwrap();
        row
    }

    #[test]
    fn it_uses_the_top_row_unless_the_view_needs_it() {
        assert_eq!(row_after("A short title"), 0);
        assert_eq!(row_after(&"x".repeat(35)), 1);
    }

    #[test]
    fn dropouts_are_flagged_but_a_busy_cpu_alone_isnt() {
        assert_eq!(
            text(7, 0, 0),
            ("CPU  7%  late 0  xruns 0 ".to_owned(), false)
        );
        assert!(text(95, 0, 0).0.starts_with("CPU 95%"));
        assert!(!text(95, 0, 0).1);
        assert!(text(1, 1, 0).1);
        assert!(text(1, 0, 1).1);
    }
}
