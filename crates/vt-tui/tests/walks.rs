//! The shape panel's `Walk` and `1/f` fills, by keys.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::shape::{Edge, FillKind, GenParam};
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

fn steps(app: &App) -> Vec<String> {
    let id = app.phrase_id().unwrap();
    app.song().tracks[0].phrases[&id]
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn velocities(app: &App) -> Vec<u8> {
    steps(app)
        .iter()
        .map(|s| u8::from_str_radix(&s[4..6], 16).unwrap())
        .collect()
}

fn notes(app: &App) -> Vec<u8> {
    steps(app)
        .iter()
        .map(|s| match s[..3].parse() {
            Ok(Note::On(note)) => note,
            _ => panic!("{s}"),
        })
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> Vec<String> {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(42, 14);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..14)
        .map(|y| {
            let line: String = (0..42).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect()
}

#[test]
fn walk_steps_the_velocities_from_from_and_1_f_sums_its_depth() {
    let (mut app, _rt) = app_with(&[NOTE; 16]);
    let depth = app.history.undo_depth();
    keys(&mut app, "lvGSKwalk<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Walk);
    assert_eq!(
        panel.rows(),
        [
            GenParam::Fill,
            GenParam::From,
            GenParam::To,
            GenParam::Step,
            GenParam::Edge,
            GenParam::Run,
            GenParam::Where,
        ]
    );
    // On Velocity a step starts at `10`, an eighth of the range, so it can be heard.
    assert_eq!(panel.shape.walk.step, 0x10);
    keys(&mut app, "ji20ji60");
    let walked = velocities(&app);
    assert_eq!(walked[0], 0x20);
    assert!(walked.iter().all(|v| (0x20..=0x60).contains(v)));
    assert!(
        walked
            .windows(2)
            .all(|pair| pair[0].abs_diff(pair[1]) <= 0x10)
    );
    // `+` jumps the step by `10`, `i` types it in hex, `x` puts it back, and `=` turns `Edge`.
    keys(&mut app, "j+");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 0x20);
    keys(&mut app, "i08");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 0x08);
    let small = velocities(&app);
    assert!(
        small
            .windows(2)
            .all(|pair| pair[0].abs_diff(pair[1]) <= 0x08)
    );
    keys(&mut app, "x");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 0x10);
    keys(&mut app, "j=");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.edge, Edge::Wrap);
    keys(&mut app, "i");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.edge, Edge::Reflect);
    // `r` walks another way.
    let before = velocities(&app);
    keys(&mut app, "r");
    assert_ne!(velocities(&app), before);
    // The next fill along is `1/f`, with its `Depth` in place of the walk's settings.
    keys(&mut app, "kkkk=");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.row, GenParam::Fill);
    assert_eq!(panel.shape.kind, FillKind::Pink);
    assert_eq!(panel.shape.kind.name(), "1/f");
    assert_eq!(
        panel.rows(),
        [
            GenParam::Fill,
            GenParam::From,
            GenParam::To,
            GenParam::Depth,
            GenParam::Run,
            GenParam::Where,
        ]
    );
    assert_eq!(panel.shape.pink.depth, 4);
    assert!(velocities(&app).iter().all(|v| (0x20..=0x60).contains(v)));
    keys(&mut app, "jjji9<Enter>");
    assert_eq!(app.status, "Depth runs from 2 to 8");
    keys(&mut app, "i6<Enter>++");
    assert_eq!(app.shape.as_ref().unwrap().shape.pink.depth, 8);
    keys(&mut app, "_");
    assert_eq!(app.shape.as_ref().unwrap().shape.pink.depth, 6);
    keys(&mut app, "<Enter>");
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn a_walk_of_notes_steps_in_degrees_and_says_so() {
    let (mut app, _rt) = app_with(&[E; 16]);
    app.scale.on = true;
    keys(&mut app, "vGSKwalk<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert_eq!(panel.shape.walk.step, 2);
    // From the root, in the major scale, never more than two degrees, a major third, at a time.
    let walked = notes(&app);
    assert_eq!(walked[0], panel.shape.from);
    assert!(walked.iter().all(|note| app.scale.contains(*note)));
    assert!(walked.windows(2).all(|pair| pair[0].abs_diff(pair[1]) <= 4));
    let screen = drawn(&app);
    let text = screen.join("\n");
    for expected in [
        "Generate",
        "Note: 16 of 16 rows",
        "Fill    Walk",
        "Step    2 degrees",
        "Edge    Reflect",
        "~ swap  r reroll",
    ] {
        assert!(text.contains(expected), "{expected} in\n{text}");
    }
    // `+` on Step adds an octave of degrees, and the most is two octaves.
    keys(&mut app, "jjj+");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 9);
    keys(&mut app, "+++");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 24);
    keys(&mut app, "i3<Enter>");
    assert_eq!(app.shape.as_ref().unwrap().shape.walk.step, 3);
    // With the scale off, it's semitones.
    keys(&mut app, "<Enter>");
    app.scale.on = false;
    keys(&mut app, "vGSKwalk<Enter>");
    assert!(drawn(&app).join("\n").contains("Step    2 semitones"));
}

#[test]
fn dot_walks_another_selection_from_its_own_ends() {
    let (mut app, _rt) = app_with(&[NOTE; 8]);
    keys(&mut app, "lvjjjSKwalk<Enter>ji10ji30<Enter>");
    let first = velocities(&app);
    assert_eq!(first[0], 0x10);
    // Four rows on, the notes there are all `40`, so the walk goes from `00` to `7F`.
    keys(&mut app, "4jvjjj.");
    let after = velocities(&app);
    assert_eq!(after[..4], first[..4]);
    assert_eq!(after[4], 0x00);
    assert_ne!(after[4..], [0x40; 4]);
}
