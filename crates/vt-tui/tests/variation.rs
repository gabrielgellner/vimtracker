//! The shape panel's `Variation`, by keys: `S` on a line writes another like it, learned from its
//! own notes, kept live with `L`, and a line of too few notes is refused with a message.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::shape::{FillKind, GenParam};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::keys::Key;

const E: &str = "--- -- --- --- ---";

/// A line of my own, every run of three rows of it followed by one row only.
const LINE: [&str; 16] = [
    "C-2 7F --- --- ---",
    E,
    "C-2 50 --- --- ---",
    "D#2 50 HLD1 --- --- ---",
    "G-2 50 --- --- ---",
    E,
    "C-2 7F --- --- ---",
    "C-3 50 --- --- ---",
    E,
    "A#2 50 HLD1 --- --- ---",
    "C-2 50 --- --- ---",
    "OFF -- --- --- ---",
    "C-2 7F --- --- ---",
    E,
    "G-2 50 --- --- ---",
    "D#2 7F --- --- ---",
];

/// A song of one track whose phrase is these rows, with the scale off.
fn app_with(rows: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    song.scale.on = false;
    let steps: Vec<Step> = rows.iter().map(|row| row.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Type keys, with `<Enter>` written out.
fn keys(app: &mut App, text: &str) {
    let now = Instant::now();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("<Enter>") {
            app.key(Key::new(KeyCode::Enter), now);
            rest = tail;
        } else {
            app.key(Key::char(c), now);
            rest = &rest[c.len_utf8()..];
        }
    }
}

fn texts(app: &App) -> Vec<String> {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn line() -> Vec<String> {
    LINE.iter()
        .map(|row| row.parse::<Step>().unwrap().to_string())
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(52, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..12)
        .map(|y| {
            let line: String = (0..52).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn variation_writes_a_line_like_the_one_there_and_order_3_cold_writes_it_back() {
    let (mut app, _rt) = app_with(&LINE);
    let depth = app.history.undo_depth();
    keys(&mut app, "vGSK/variation<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Variation);
    use GenParam::*;
    assert_eq!(panel.rows(), [Fill, Order, Temperature, Vary]);
    let screen = drawn(&app);
    for expected in [
        "Reshape",
        "Note:",
        "of 16 rows",
        "Order   1",
        "Temp    50%",
        "Vary    Once",
        "r another  Order 3 copies closely",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // Order 3 at temperature 0 hangs every row on the three before it, which the line has one
    // follower for each: it's written back as it was.
    keys(&mut app, "ji3<Enter>ji0<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.variation.order, 3);
    assert_eq!(panel.shape.variation.temperature, 0);
    assert_eq!(texts(&app), line());
    // Looser and hotter, it strays, and `L` keeps it live in one undo step.
    keys(&mut app, "ki1<Enter>ki100<Enter>");
    assert_ne!(texts(&app), line());
    let written = texts(&app);
    keys(&mut app, "L");
    assert!(app.shape.is_none());
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert_eq!(texts(&app), written);
    let generators = app.song().tracks[0].phrases[&0].generators();
    assert_eq!(generators.len(), 1);
    assert_eq!(generators[0].shape.kind, FillKind::Variation);
}

#[test]
fn a_line_of_too_few_notes_is_refused_with_a_message() {
    let mut rows = [E; 16];
    rows[0] = "C-3 40 --- --- ---";
    rows[4] = "E-3 40 --- --- ---";
    rows[8] = "G-3 40 --- --- ---";
    let (mut app, _rt) = app_with(&rows);
    let before = texts(&app);
    let depth = app.history.undo_depth();
    keys(&mut app, "vGSK/variation<Enter>");
    let message = "Variation needs 4 notes to learn from, and there are 3";
    assert!(
        drawn(&app).contains("Variation needs 4 notes"),
        "{}",
        drawn(&app)
    );
    assert_eq!(texts(&app), before, "nothing is written");
    keys(&mut app, "<Enter>");
    assert_eq!(app.status, message);
    assert!(app.shape.is_some(), "the panel stays open");
    keys(&mut app, "L");
    assert_eq!(app.status, message);
    assert_eq!(app.history.undo_depth(), depth);
}
