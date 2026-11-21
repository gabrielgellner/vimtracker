//! The focus colour (decision 475): where another area on screen could take the keys, the frame
//! of the one with them is in `theme::FOCUS`, and the other's cursor sits back, dimmed rather
//! than hidden. Gen beside the grid, the macro panel, the shape panel and a buffer of tokens.

use std::ops::Range;
use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use vt_engine::link::Realtime;
use vt_model::shape::GenParam;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};
use vt_tui::widgets::theme;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// Two tracks, the first's eight empty rows with a tresillo of notes kept live on them, then
/// Gen, which opens beside the grid, the keys in it.
fn beside_the_grid() -> (App, Realtime) {
    let mut song = Song::new(2);
    let empty: Step = E.parse().unwrap();
    for track in &mut song.tracks {
        track
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(vec![empty.clone(); 8])));
    }
    let (mut app, realtime) = app_for(song);
    keys(&mut app, "vGSjjjjjj===L");
    keys(&mut app, " 8");
    assert_eq!(app.modes.view, View::Gen);
    (app, realtime)
}

/// The screen as drawn at 163x42, with its colours.
fn drawn(app: &mut App) -> Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, app, None, std::time::Instant::now()))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Whether a cell between two columns, on the rows above the status lines, has this background.
fn has_bg(buffer: &Buffer, columns: Range<u16>, bg: Color) -> bool {
    columns
        .clone()
        .any(|x| (1..40).any(|y| buffer[(x, y)].bg == bg))
}

/// The background of the cursor sitting back.
fn away() -> Color {
    theme::cursor_away().bg.unwrap()
}

/// The first cell of a row's line that has this symbol, from the left.
fn find(buffer: &Buffer, y: u16, symbol: &str) -> Option<u16> {
    (0..buffer.area.width).find(|x| buffer[(*x, y)].symbol() == symbol)
}

/// In the panel, its device's frame is in the focus colour and its cursor lit, the grid's frame
/// as it was and its cursor sitting back; in the grid, the other way round, the panel's cursor
/// dimmed rather than hidden as it was (decision 467).
#[test]
fn beside_the_grid_the_side_with_the_keys_is_framed_and_the_others_cursor_sits_back() {
    let (mut app, _rt) = beside_the_grid();
    let screen = drawn(&mut app);
    assert_eq!(screen[(68, 1)].symbol(), "╭");
    assert_eq!(screen[(68, 1)].fg, theme::FOCUS, "the panel's device");
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1, "the grid's frame");
    assert_eq!(screen[(66, 3)].fg, theme::SURFACE1);
    assert!(has_bg(&screen, 68..163, theme::ROSEWATER));
    assert!(
        has_bg(&screen, 0..67, away()),
        "the grid's cursor sits back"
    );
    assert!(!has_bg(&screen, 0..67, theme::ROSEWATER));
    keys(&mut app, "^wh");
    let screen = drawn(&mut app);
    assert_eq!(screen[(8, 1)].fg, theme::FOCUS, "the grid's frame");
    assert_eq!(screen[(66, 3)].fg, theme::FOCUS);
    assert_eq!(screen[(8, 10)].symbol(), "╰");
    assert_eq!(screen[(8, 10)].fg, theme::FOCUS, "its foot");
    assert_ne!(screen[(68, 1)].fg, theme::FOCUS, "the panel's device");
    assert!(has_bg(&screen, 0..67, theme::ROSEWATER));
    assert!(
        has_bg(&screen, 68..163, away()),
        "the panel's cursor sits back"
    );
    assert!(!has_bg(&screen, 68..163, theme::ROSEWATER));
}

/// Insert lights the grid's frame green as it always has, which Insert in the panel doesn't.
#[test]
fn insert_keeps_its_green_on_the_grid_alone() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^whi");
    assert_eq!(app.modes.mode, Mode::Insert);
    assert_eq!(drawn(&mut app)[(8, 1)].fg, theme::GREEN);
    keys(&mut app, "<Esc>^wlji");
    assert_eq!(app.modes.mode, Mode::Insert);
    let screen = drawn(&mut app);
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1, "Insert in the panel");
    assert_eq!(screen[(68, 1)].fg, theme::FOCUS);
}

/// The grid alone has no other area to tell it from, so its frame is as it was and its cursor
/// lit.
#[test]
fn the_grid_alone_keeps_its_frame() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc");
    let screen = drawn(&mut app);
    assert_eq!(screen[(8, 1)].symbol(), "╭");
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1);
    assert!(has_bg(&screen, 0..163, theme::ROSEWATER), "its cursor");
}

/// The row of the macro panel's rule, the line with its title.
fn macro_rule(screen: &Buffer) -> u16 {
    (0..screen.area.height)
        .find(|y| {
            (0..screen.area.width)
                .map(|x| screen[(x, *y)].symbol())
                .collect::<String>()
                .starts_with(" Macros  ")
        })
        .expect("the macro panel")
}

/// With the macro panel showing, the keys in it put its rule in the focus colour and the grid's
/// cursor sits back; given back, the grid's frame takes the colour; hidden, the grid is alone.
#[test]
fn the_macro_panel_takes_the_focus_colour_from_the_grid() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc T");
    assert_eq!(app.modes.mode, Mode::Macros);
    let screen = drawn(&mut app);
    let rule = macro_rule(&screen);
    assert_eq!(screen[(162, rule)].fg, theme::FOCUS, "the panel's rule");
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1, "the grid's frame");
    assert!(
        has_bg(&screen, 0..163, away()),
        "the grid's cursor sits back"
    );
    keys(&mut app, "<Esc>");
    let screen = drawn(&mut app);
    assert_ne!(screen[(162, macro_rule(&screen))].fg, theme::FOCUS);
    assert_eq!(screen[(8, 1)].fg, theme::FOCUS, "the grid's frame");
    assert!(!has_bg(&screen, 0..163, away()));
    keys(&mut app, " t");
    assert_eq!(drawn(&mut app)[(8, 1)].fg, theme::SURFACE1, "alone again");
}

/// The shape panel always has the keys, so it's framed in the focus colour, over the grid whose
/// cursor sits back.
#[test]
fn the_shape_panel_is_framed_in_the_focus_colour() {
    let (mut app, _rt) = beside_the_grid();
    keys(&mut app, "^wc");
    keys(&mut app, "llggvjjjS");
    assert_eq!(app.modes.mode, Mode::Shape);
    let screen = drawn(&mut app);
    let corner = (1..40)
        .find_map(|y| {
            let x = find(&screen, y, "╭").filter(|x| *x > 8)?;
            Some((x, y))
        })
        .expect("the shape panel");
    assert_eq!(screen[corner].fg, theme::FOCUS);
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1, "the grid's frame");
    assert!(
        has_bg(&screen, 0..163, away()),
        "the grid's cursor sits back"
    );
}

/// A buffer of tokens open on Gen's panel is framed in the focus colour, over the panel whose
/// device it's open on, which isn't.
#[test]
fn a_buffer_is_framed_in_the_focus_colour_over_its_page() {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, _rt) = app_for(song);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>");
    down_to(&mut app, GenParam::Rhythm);
    keys(&mut app, "K/own<Enter>L 8");
    'found: for _ in 0..12 {
        keys(&mut app, "0");
        for _ in 0..6 {
            if here(&app) == Some(GenParam::OwnText) {
                break 'found;
            }
            keys(&mut app, "l");
        }
        keys(&mut app, "j");
    }
    assert_eq!(here(&app), Some(GenParam::OwnText));
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.mode, Mode::Tokens);
    let screen = drawn(&mut app);
    let lines = screen_cells(&mut app);
    let (x, y) = lines
        .iter()
        .enumerate()
        .find_map(|(y, line)| {
            line.find("╭ Text ")
                .map(|at| (line[..at].chars().count() as u16, y as u16))
        })
        .expect("the buffer");
    assert_eq!(screen[(x, y)].fg, theme::FOCUS, "the buffer's frame");
    assert_ne!(screen[(68, 1)].fg, theme::FOCUS, "the panel's device");
    assert_eq!(screen[(8, 1)].fg, theme::SURFACE1, "the grid's frame");
    assert!(
        has_bg(&screen, 68..163, away()),
        "the panel's cursor sits back"
    );
}
