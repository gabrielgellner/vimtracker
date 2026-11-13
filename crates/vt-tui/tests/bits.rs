//! The shape panel's `Multiply`, by keys.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::shape::{FillKind, GenParam};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;

const E: &str = "--- -- --- --- ---";
const NOTE: &str = "C-3 40 --- --- ---";

fn app_with(steps: &[&str]) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|s| s.parse().unwrap()).collect();
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

/// Each row as `x` for a note and `.` for none.
fn rhythm(app: &App) -> String {
    let id = app.phrase_id().unwrap();
    app.song().tracks[0].phrases[&id]
        .steps()
        .iter()
        .map(|step| {
            if matches!(step.note, Note::On(_)) {
                'x'
            } else {
                '.'
            }
        })
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(52, 14);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..14)
        .map(|y| {
            let line: String = (0..52).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Four on the floor over 16 rows.
fn four_on_the_floor() -> Vec<&'static str> {
    (0..16)
        .map(|row| if row % 4 == 0 { NOTE } else { E })
        .collect()
}

#[test]
fn multiply_turns_four_on_the_floor_by_its_factor() {
    let (mut app, _rt) = app_with(&four_on_the_floor());
    let depth = app.history.undo_depth();
    keys(&mut app, "vGSKmultiply<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Multiply);
    assert_eq!(
        panel.rows(),
        [GenParam::Fill, GenParam::Factor, GenParam::Masks]
    );
    // It starts at 3.
    assert_eq!(rhythm(&app), "xx..xx..xx..xx..");
    let screen = drawn(&app);
    for expected in ["Factor  3", "Masks   Off", "16 rows a number, row 1 its lowest bit"] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // `=` and `-` move it by one, `+` by 4, `i` types it, and `x` puts it back.
    keys(&mut app, "j=");
    assert_eq!(
        rhythm(&app),
        "..x...x...x...x.",
        "times 4 is two rows later"
    );
    keys(&mut app, "+");
    assert_eq!(app.shape.as_ref().unwrap().shape.bits.factor, 8);
    keys(&mut app, "i17<Enter>");
    assert_eq!(app.status, "Factor runs from 1 to 16");
    keys(&mut app, "i5<Enter>");
    assert_eq!(rhythm(&app), "x.x.x.x.x.x.x.x.");
    keys(&mut app, "i1<Enter>");
    assert_eq!(rhythm(&app), "x...x...x...x...", "1 leaves it as it is");
    keys(&mut app, "x");
    assert_eq!(app.shape.as_ref().unwrap().shape.bits.factor, 3);
    // Masks keep rows 1, 2 and 13 to 16 before 3 multiplies.
    keys(&mut app, "j=");
    assert!(app.shape.as_ref().unwrap().shape.bits.masks);
    assert_eq!(rhythm(&app), "xx..........xx..");
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), depth + 1);
    assert_eq!(rhythm(&app), "xx..........xx..");
}

#[test]
fn multiply_is_only_offered_on_a_rhythm() {
    let (mut app, _rt) = app_with(&four_on_the_floor());
    keys(&mut app, "lvGS");
    let panel = app.shape.clone().unwrap();
    assert!(!panel.context().fills().contains(&FillKind::Multiply));
    let (mut app, _rt) = app_with(&[E; 16]);
    keys(&mut app, "vGS");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert!(!panel.context().fills().contains(&FillKind::Multiply));
}
