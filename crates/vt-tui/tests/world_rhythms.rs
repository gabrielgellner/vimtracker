//! Named rhythms as a `Where`, by keys (decision 327): `S` across three tracks gives each the next
//! part of the rhythm, kept live as a generator a track; `Child` plays some of a part's hits; a
//! 12/8 rhythm's hint names the track speed it fits; and on the Gen page, the rhythm, its part
//! and `Child` are cells.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::generator::Generator;
use vt_model::samples::SamplePool;
use vt_model::shape::{GenParam, Role, Where, World};
use vt_model::song::{Phrase, Song};
use vt_model::speed::Speed;
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::track_page::{Control, Page};

mod common;
use common::*;

const E: &str = "--- -- --- --- ---";

/// A song of so many tracks at a speed, each with an empty phrase of 16 rows.
fn app_with(tracks: usize, speed: Speed) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in &mut song.tracks {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
        track.speed = speed;
    }
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

/// A track's rows as `x` for a note and `.` for none.
fn rhythm(app: &App, track: usize) -> String {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            Note::On(_) => 'x',
            _ => '.',
        })
        .collect()
}

fn generators(app: &App, track: usize) -> Vec<Generator> {
    app.song().tracks[track].phrases[&0].generators().to_vec()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(60, 18);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..18)
        .map(|y| {
            let line: String = (0..60).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Down the open panel to a setting.
fn down_to(app: &mut App, row: GenParam) {
    for _ in 0..GenParam::ALL.len() {
        if app.shape.as_ref().unwrap().row == row {
            return;
        }
        keys(app, "j");
    }
    panic!("no {row:?} row");
}

/// The setting the cursor is on, on the Gen page.
fn here(app: &App) -> Option<GenParam> {
    match app.track_control() {
        Control::Gen { param, .. } => Some(param),
        _ => None,
    }
}

fn son(part: Role, child: u8) -> Where {
    Where::Rhythm {
        rhythm: World::SonClave,
        part,
        child,
    }
}

#[test]
fn a_rhythm_across_three_tracks_gives_each_the_next_part() {
    let (mut app, _rt) = app_with(3, Speed::Sixteenth);
    let depth = app.history.undo_depth();
    keys(&mut app, "vGWWS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.place, son(Role::Bell, 100));
    for row in [GenParam::Rhythm, GenParam::Role, GenParam::Child] {
        assert!(panel.rows().contains(&row), "{row:?}");
    }
    let screen = drawn(&app);
    for expected in [
        "Where   Rhythm",
        "Rhythm  Son clave",
        "Part    Bell",
        "Child   100%",
        "Each track plays the next part",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // The clave, the son's bass and its conga.
    assert_eq!(rhythm(&app, 0), "x..x..x...x.x...");
    assert_eq!(rhythm(&app, 1), "...x..x....x..x.");
    assert_eq!(rhythm(&app, 2), "......xx......xx");
    // Another rhythm from the list, for all three.
    down_to(&mut app, GenParam::Rhythm);
    keys(&mut app, "K/gahu<Enter>");
    assert_eq!(rhythm(&app, 0), "x..x..x...x...x.");
    assert_eq!(rhythm(&app, 1), "x...x...x...x...");
    assert_eq!(rhythm(&app, 2), "..x...x...x...x.");
    // A child plays some of each part's hits, and none of its rests.
    down_to(&mut app, GenParam::Child);
    keys(&mut app, "i50<Enter>");
    let mothers = ["x..x..x...x...x.", "x...x...x...x...", "..x...x...x...x."];
    let mut played = 0;
    for (track, mother) in mothers.iter().enumerate() {
        let child = rhythm(&app, track);
        for (hit, of) in child.chars().zip(mother.chars()) {
            assert!(hit == '.' || of == 'x', "{child} from {mother}");
        }
        played += child.matches('x').count();
    }
    assert!(played > 0 && played < 13, "{played} of 13");
    keys(&mut app, "L");
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "one undo for all three"
    );
    let shapes: Vec<_> = (0..3)
        .map(|track| generators(&app, track)[0].shape)
        .collect();
    let parts: Vec<Where> = shapes.iter().map(|shape| shape.place).collect();
    let gahu = |part| Where::Rhythm {
        rhythm: World::Gahu,
        part,
        child: 50,
    };
    assert_eq!(parts, [gahu(Role::Bell), gahu(Role::Low), gahu(Role::High)]);
    assert_ne!(shapes[0].seed, shapes[1].seed, "a child of its own a track");
    assert!(shapes[0].draws(), "so it has Vary");
}

#[test]
fn a_12_8_rhythm_says_it_fits_a_bar_on_a_triplet_track() {
    let hint = "Its 12 rows make a bar on a 1/8T track";
    for (speed, says) in [(Speed::Sixteenth, true), (Speed::EighthTriplet, false)] {
        let (mut app, _rt) = app_with(1, speed);
        keys(&mut app, "vGS");
        down_to(&mut app, GenParam::Where);
        keys(&mut app, "K/rhythm<Enter>");
        down_to(&mut app, GenParam::Rhythm);
        keys(&mut app, "K/bemb<Enter>");
        assert_eq!(
            drawn(&app).contains(hint),
            says,
            "{speed:?}\n{}",
            drawn(&app)
        );
        // The bell goes round in 12 rows.
        assert_eq!(rhythm(&app, 0), "x.x.xx.x.x.xx.x.");
    }
}

#[test]
fn the_gen_page_has_the_rhythm_its_part_and_child() {
    let (mut app, _rt) = app_with(1, Speed::Sixteenth);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>L");
    assert_eq!(generators(&app, 0)[0].shape.place, son(Role::Bell, 100));
    keys(&mut app, " 5LLL");
    assert_eq!(app.track_page.page, Page::Gen);
    keys(&mut app, "}k");
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Ramp  Rhythm  Once"), "{text}");
    for expected in ["Son clave", "Bell", "100%"] {
        assert!(text.contains(expected), "{expected} in\n{text}");
    }
    // `Child` is the third column's last cell, as a `Chance`'s amount is, its part above it and
    // the rhythm above that.
    keys(&mut app, "jjj$");
    assert_eq!(here(&app), Some(GenParam::Child));
    let depth = app.history.undo_depth();
    keys(&mut app, "i0<Enter>");
    assert_eq!(generators(&app, 0)[0].shape.place, son(Role::Bell, 0));
    assert_eq!(rhythm(&app, 0), ".".repeat(16));
    assert_eq!(app.history.undo_depth(), depth + 1);
    // `x` puts it back to every hit.
    keys(&mut app, "x");
    assert_eq!(generators(&app, 0)[0].shape.place, son(Role::Bell, 100));
    keys(&mut app, "k");
    assert_eq!(here(&app), Some(GenParam::Role));
    keys(&mut app, "K/low<Enter>");
    assert_eq!(generators(&app, 0)[0].shape.place, son(Role::Low, 100));
    assert_eq!(rhythm(&app, 0), "...x..x....x..x.");
    keys(&mut app, "k");
    assert_eq!(here(&app), Some(GenParam::Rhythm));
    keys(&mut app, "K/motorik<Enter>");
    assert_eq!(
        generators(&app, 0)[0].shape.place,
        Where::Rhythm {
            rhythm: World::Motorik,
            part: Role::Low,
            child: 100
        }
    );
    // Motorik's kick.
    assert_eq!(rhythm(&app, 0), "x.x...x.x.x...x.");
}
