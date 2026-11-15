//! A popup: a window over the screen, such as the picker, the guides, the keys, the shape panel and
//! the mapping view. [`draw`] clears where it goes, so nothing under it shows through, and draws
//! the rounded border every window has in the window's colours, with its titles; the rest work out
//! where it goes in an area, never bigger than the area: [`centred`] in its middle, [`top_right`]
//! and [`bottom_right`] in a corner, [`top_left`] at its top, and [`above`] ending just above a
//! row, as the command line's list of matches does.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear};

use crate::widgets;
use crate::widgets::theme;

/// A popup's border, in the window's colours, for its titles to go on.
pub(crate) fn border() -> Block<'static> {
    widgets::window_border().style(theme::window())
}

/// Draw a popup's border over what's under it, clearing its area first, and give the area inside
/// the border, for what's in it.
pub(crate) fn draw(frame: &mut Frame, area: Rect, border: Block) -> Rect {
    let inside = border.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(border, area);
    inside
}

/// A size no bigger than an area's.
fn within(area: Rect, width: u16, height: u16) -> (u16, u16) {
    (width.min(area.width), height.min(area.height))
}

/// A window this big in the middle of an area, or as much of it as fits.
pub(crate) fn centred(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = within(area, width, height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// A window this big in an area's top left corner, or as much of it as fits.
pub(crate) fn top_left(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = within(area, width, height);
    Rect {
        width,
        height,
        ..area
    }
}

/// A window this big in an area's top right corner, or as much of it as fits.
pub(crate) fn top_right(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = within(area, width, height);
    Rect {
        x: area.right() - width,
        y: area.y,
        width,
        height,
    }
}

/// A window this big in an area's bottom right corner, or as much of it as fits.
pub(crate) fn bottom_right(area: Rect, width: u16, height: u16) -> Rect {
    let (width, height) = within(area, width, height);
    Rect {
        x: area.right() - width,
        y: area.bottom() - height,
        width,
        height,
    }
}

/// A window this big ending just above a row of the screen, from a column into an area, or as
/// much of it as fits between the area's top and the row, moved left to stay inside it.
pub(crate) fn above(area: Rect, row: u16, column: u16, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(row.saturating_sub(area.y));
    Rect {
        x: area.x + column.min(area.width - width),
        y: row - height,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyCode;

    use super::*;
    use crate::app::App;
    use crate::input::keys::Key;
    use crate::widgets::pin::{app, press, special};

    #[test]
    fn a_window_in_the_middle_or_a_corner_is_no_bigger_than_its_area() {
        let area = Rect::new(10, 5, 40, 20);
        assert_eq!(centred(area, 20, 10), Rect::new(20, 10, 20, 10));
        assert_eq!(
            centred(area, 21, 11),
            Rect::new(19, 9, 21, 11),
            "an odd row or column over goes below or to the right"
        );
        assert_eq!(centred(area, 60, 30), area);
        assert_eq!(top_left(area, 20, 30), Rect::new(10, 5, 20, 20));
        assert_eq!(top_right(area, 20, 10), Rect::new(30, 5, 20, 10));
        assert_eq!(top_right(area, 60, 10), Rect::new(10, 5, 40, 10));
        assert_eq!(bottom_right(area, 20, 10), Rect::new(30, 15, 20, 10));
        assert_eq!(bottom_right(area, 60, 30), area);
    }

    #[test]
    fn a_window_above_a_row_ends_on_the_row_before_it_and_stays_in_its_area() {
        let area = Rect::new(0, 0, 100, 42);
        assert_eq!(above(area, 41, 10, 30, 12), Rect::new(10, 29, 30, 12));
        assert_eq!(
            above(area, 41, 90, 30, 12),
            Rect::new(70, 29, 30, 12),
            "moved left"
        );
        assert_eq!(
            above(area, 8, 10, 30, 12),
            Rect::new(10, 0, 30, 8),
            "cut at the top"
        );
    }

    /// The whole screen at the author's size, a row at a time, with a character for each cell.
    fn screen(app: &mut App, now: Instant) -> Vec<Vec<char>> {
        let mut terminal = Terminal::new(TestBackend::new(163, 42)).unwrap();
        terminal
            .draw(|frame| crate::run::draw_screen(frame, app, None, now))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..42)
            .map(|y| {
                (0..163)
                    .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                    .collect()
            })
            .collect()
    }

    /// The window on a row of the screen whose top border has this in it: from its `╭` to its
    /// `╮`, and down to its `╰`.
    fn window(screen: &[Vec<char>], title: &str) -> Rect {
        let wanted: Vec<char> = title.chars().collect();
        for (y, row) in screen.iter().enumerate() {
            let Some(at) = row.windows(wanted.len()).position(|run| run == wanted) else {
                continue;
            };
            let Some(left) = row[..at].iter().rposition(|c| *c == '╭') else {
                continue;
            };
            let right = at + row[at..].iter().position(|c| *c == '╮').unwrap();
            let bottom = (y..screen.len()).find(|y| screen[*y][left] == '╰').unwrap();
            let rect = |n: usize| u16::try_from(n).unwrap();
            return Rect::new(
                rect(left),
                rect(y),
                rect(right + 1 - left),
                rect(bottom + 1 - y),
            );
        }
        panic!("no window with {title:?}");
    }

    /// The window whose bottom right corner is here, found from it.
    fn window_ending(screen: &[Vec<char>], right: usize, bottom: usize) -> Rect {
        assert_eq!(screen[bottom][right], '╯');
        let left = screen[bottom][..right]
            .iter()
            .rposition(|c| *c == '╰')
            .unwrap();
        let top = (0..bottom)
            .rev()
            .find(|y| screen[*y][right] == '╮')
            .unwrap();
        let rect = |n: usize| u16::try_from(n).unwrap();
        Rect::new(
            rect(left),
            rect(top),
            rect(right + 1 - left),
            rect(bottom + 1 - top),
        )
    }

    /// Where the picker's window is, by its title.
    fn picker_window(app: &mut App) -> Rect {
        let title = format!(
            " {} ",
            app.overlays.picker.as_ref().expect("a picker").title
        );
        window(&screen(app, Instant::now()), &title)
    }

    #[test]
    fn the_picker_is_in_the_middle_of_the_screen() {
        let (mut app, _realtime) = app();
        press(&mut app, " 5LLK");
        assert_eq!(picker_window(&mut app), Rect::new(44, 10, 74, 19));
        special(&mut app, KeyCode::Esc);
        press(&mut app, " ?");
        assert_eq!(picker_window(&mut app), Rect::new(63, 11, 37, 17));
    }

    #[test]
    fn the_command_lines_matches_sit_above_it_under_the_word_they_finish() {
        let (mut app, _realtime) = app();
        press(&mut app, ":set ");
        app.key(Key::new(KeyCode::Tab), Instant::now());
        let title = format!(
            " {} ",
            app.overlays
                .command
                .menu
                .as_ref()
                .expect("a list")
                .picker
                .title
        );
        assert_eq!(
            window(&screen(&mut app, Instant::now()), &title),
            Rect::new(2, 33, 66, 8)
        );
    }

    #[test]
    fn a_guide_is_as_tall_as_the_screen_in_its_middle_and_the_keys_fill_it() {
        let (mut app, _realtime) = app();
        press(&mut app, " ?");
        special(&mut app, KeyCode::Enter);
        let title = format!(" {} ", app.overlays.guide.open.expect("a guide").title());
        assert_eq!(
            window(&screen(&mut app, Instant::now()), &title),
            Rect::new(31, 0, 100, 40)
        );
        press(&mut app, "q k");
        let keys = window(&screen(&mut app, Instant::now()), "Keys: j and k");
        assert_eq!(keys, Rect::new(0, 0, 163, 40));
    }

    #[test]
    fn the_next_keys_are_in_the_bottom_right_corner() {
        let (mut app, _realtime) = app();
        press(&mut app, " ");
        let later = Instant::now() + Duration::from_secs(1);
        let shown = screen(&mut app, later);
        let bottom = (0..shown.len())
            .rev()
            .find(|y| shown[*y][162] == '╯')
            .unwrap();
        assert_eq!(window_ending(&shown, 162, bottom), Rect::new(88, 3, 75, 37));
    }

    #[test]
    fn the_shape_panel_is_at_the_right_under_the_cpu_figure() {
        let (mut app, _realtime) = app();
        press(&mut app, "vjS");
        let title = format!(
            " {} ",
            app.overlays.shape.as_ref().expect("the panel").verb()
        );
        assert_eq!(
            window(&screen(&mut app, Instant::now()), &title),
            Rect::new(108, 2, 42, 11)
        );
    }

    #[test]
    fn the_mapping_view_is_in_the_middle_of_the_page() {
        let (mut app, _realtime) = app();
        press(&mut app, " 5}jM1gm");
        assert!(app.overlays.mapping.is_some());
        let shown = screen(&mut app, Instant::now());
        assert_eq!(window(&shown, " Macro 1"), Rect::new(34, 14, 82, 11));
    }

    #[test]
    fn the_rows_inspector_is_as_tall_as_its_row_needs() {
        let (mut app, _realtime) = app();
        press(&mut app, " i");
        assert!(app.phrase_view.inspector);
        let shown = screen(&mut app, Instant::now());
        assert_eq!(window(&shown, "Row 00"), Rect::new(79, 1, 60, 11));
    }
}
