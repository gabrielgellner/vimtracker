//! A rhythm of your own (decision 461, #1091): `K` on `Rhythm` gains `Own`, which starts from the
//! named rhythm's text; typing or editing the text of a named one makes it `Own`; `Step` makes a
//! token a beat; the field's tokens are lit with the rows each hit writes; and kept live, the text
//! is the generator's, saved with it and played by the engine.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::{GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// One track with an empty phrase of 16 rows, the panel open on it at `Where`'s `Rhythm`.
fn on_rhythm() -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, realtime) = app_for(song);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>");
    (app, realtime)
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

/// The phrase's rows as the panel shapes them now: `x` a note, `.` none.
fn shaped(app: &App) -> String {
    let panel = panel(app);
    let part = panel.parts[0];
    let own = panel.own();
    let texts = own.texts(&panel.list);
    let steps = app.song().tracks[0].phrases[&0].steps().to_vec();
    let written =
        vt_model::shape::written_from(&steps, part.range, part.column, &panel.shape, texts, None);
    (0..16)
        .map(|row| {
            let note = written
                .iter()
                .find(|(at, _)| *at == row)
                .map(|(_, step)| step.note);
            if matches!(note, Some(Note::On(_))) {
                'x'
            } else {
                '.'
            }
        })
        .collect()
}

#[test]
fn own_starts_from_the_named_rhythm_and_plays_what_is_typed() {
    let (mut app, _rt) = on_rhythm();
    let drawn = shape_panel(&app, 80, 30);
    assert!(
        drawn.contains("Text    x . . x . . x . . . x . x . . ."),
        "{drawn}"
    );
    down_to(&mut app, GenParam::Rhythm);
    keys(&mut app, "K/own<Enter>");
    assert!(matches!(panel(&app).shape.place, Where::Own(_)));
    assert_eq!(panel(&app).rhythm, "x . . x . . x . . . x . x . . .");
    assert_eq!(shaped(&app), "x..x..x...x.x...");
    down_to(&mut app, GenParam::OwnText);
    keys(&mut app, "Sx . [. x] x(3,8)<Enter>");
    assert_eq!(panel(&app).rhythm, "x . [. x] x(3,8)");
    // A token a beat.
    down_to(&mut app, GenParam::OwnStep);
    keys(&mut app, "i4<Enter>");
    assert_eq!(shaped(&app), "x.........x.xx.x");
    let drawn = shape_panel(&app, 80, 30);
    assert!(drawn.contains("Step    4 rows"), "{drawn}");
    // Text that doesn't read keeps the last that did playing, and `Enter` keeps the buffer open
    // on it, saying why.
    keys(&mut app, "k");
    // A `]` with no `[` stops the text reading there.
    keys(&mut app, "ax ]<Enter>");
    assert_eq!(
        panel(&app).rhythm,
        "x . [. x] x(3,8) x",
        "the last that read"
    );
    assert!(panel(&app).tokens.is_some());
    assert!(
        app.services
            .status
            .contains("the column plays the last text that read"),
        "{}",
        app.services.status
    );
    keys(&mut app, "<Esc>");
    assert!(panel(&app).tokens.is_none());
    assert_eq!(panel(&app).rhythm, "x . [. x] x(3,8) x");
}

/// Editing a named rhythm's text a token at a time makes it `Own`, from its text; the cursor's
/// hit lights the rows it writes, and the rows line shows them.
#[test]
fn editing_a_named_rhythm_makes_it_own_and_lights_its_hits() {
    let (mut app, _rt) = on_rhythm();
    down_to(&mut app, GenParam::OwnText);
    keys(&mut app, "l");
    assert_eq!(app.services.status, "Hit: x");
    keys(&mut app, "lll~");
    assert!(matches!(panel(&app).shape.place, Where::Own(_)));
    assert_eq!(panel(&app).rhythm, "x . . . . . x . . . x . x . . .");
    assert_eq!(shaped(&app), "x.....x...x.x...");
    let drawn = shape_panel(&app, 80, 30);
    assert!(
        drawn.contains("Rows  x . . . . . x . . . x . x . . ."),
        "{drawn}"
    );
    // On the second hit: the rows it writes.
    keys(&mut app, "3l");
    let rows = panel(&app).token_rows(0, 0).map(|(_, rows)| rows);
    assert_eq!(rows, Some(vec![6]));
}

/// Kept live, the text is the generator's: saved with it and read back, and the Gen page's cell
/// takes a new one typed as one undo step.
#[test]
fn kept_live_the_text_is_the_generators_and_saved_with_it() {
    let (mut app, _rt) = on_rhythm();
    down_to(&mut app, GenParam::OwnText);
    keys(&mut app, "Sx x [. x]<Enter>L");
    let generator = &app.song().tracks[0].phrases[&0].generators()[0];
    assert_eq!(generator.rhythm(), "x x [. x]");
    let folder = std::env::temp_dir().join(format!("vt-own-rhythm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    vt_model::project::save_song(&folder, app.song()).expect("saves");
    let saved = std::fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(saved.contains("rhythm: \"x x [. x]\""), "{saved}");
    let loaded = vt_model::project::load_song(&folder).expect("loads");
    let _ = std::fs::remove_dir_all(&folder);
    let again = &loaded.tracks[0].phrases[&0].generators()[0];
    assert_eq!(again.rhythm(), "x x [. x]");
    assert_eq!(again.shape, generator.shape);
    assert_eq!(
        loaded.tracks[0].phrases[&0].steps(),
        app.song().tracks[0].phrases[&0].steps()
    );
}

/// Down the Gen page's rows, and across each, to a generator's setting.
fn to_cell(app: &mut App, param: GenParam) {
    for _ in 0..8 {
        keys(app, "0");
        for _ in 0..4 {
            if here(app) == Some(param) {
                return;
            }
            keys(app, "l");
        }
        keys(app, "j");
    }
    panic!("no {param:?} on the Gen page");
}

/// On the Gen page, `K` on `Rhythm` gives `Own` from the named one, and the `Text` cell's tokens
/// are edited as the panel's are, each change one undo step.
#[test]
fn the_gen_pages_text_cell_takes_the_token_editor() {
    let (mut app, _rt) = on_rhythm();
    keys(&mut app, "L 8");
    to_cell(&mut app, GenParam::Rhythm);
    keys(&mut app, "K/own<Enter>");
    let generator = |app: &App| app.song().tracks[0].phrases[&0].generators()[0].clone();
    assert!(matches!(generator(&app).shape.place, Where::Own(_)));
    assert_eq!(generator(&app).rhythm(), "x . . x . . x . . . x . x . . .");
    to_cell(&mut app, GenParam::OwnText);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "<Enter>x<Enter>");
    assert_eq!(generator(&app).rhythm(), ". . x . . x . . . x . x . . .");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
}
