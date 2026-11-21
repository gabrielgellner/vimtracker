//! The shape panel's `Ring`, by keys: a noise ring's rhythm and pitch on an empty column, its
//! `Change`, `Chance` and `Length`, kept live; and on the Gen page, `Space g a` keeping the loop a
//! pass heard began with.

use std::sync::Arc;

use vt_engine::link::{Command, Realtime};
use vt_model::generator::Generator;
use vt_model::shape::{FillKind, GenParam, written};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::View;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// A song of one track of empty rows.
fn app_with(rows: usize) -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..rows).map(|_| E.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    app_for(song)
}

fn generators(app: &App) -> Vec<Generator> {
    app.song().tracks[0].phrases[&0].generators().to_vec()
}

fn notes(app: &App) -> Vec<Note> {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.note)
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    shape_panel(app, 60, 30)
}

/// A live ring over 16 empty rows, `Change` up to 60%, from the panel.
fn ringing() -> (App, Realtime) {
    let (mut app, realtime) = app_with(16);
    keys(&mut app, "vGSK/ring<Enter>");
    // Fill, From, To, then Change.
    keys(&mut app, "jjji60<Enter>");
    keys(&mut app, "L");
    assert_eq!(generators(&app).len(), 1);
    (app, realtime)
}

#[test]
fn the_panel_writes_a_rings_rhythm_and_pitch_and_keeps_it_live() {
    let (mut app, _rt) = app_with(16);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "vGSK/ring<Enter>");
    let panel = app.overlays.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Ring);
    {
        use GenParam::*;
        assert_eq!(panel.rows(), [Fill, From, To, Change, Ones, RingLength]);
    }
    let screen = drawn(&app);
    for expected in [
        "Ring",
        "Change",
        "10%",
        "Chance",
        "50%",
        "Length",
        "r a new ring",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // `Chance` at 100 plays every row, `Enter` writing it as one undo step.
    keys(&mut app, "jjjji100<Enter>");
    keys(&mut app, "<Enter>");
    assert!(notes(&app).iter().all(|note| matches!(note, Note::On(_))));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(app.services.history.undo_depth(), depth);
    // Kept live with `L`, as one undo step.
    let (app, _rt) = ringing();
    let played = notes(&app);
    assert!(played.iter().any(|note| matches!(note, Note::On(_))));
    assert!(played.contains(&Note::Empty));
    let generator = &generators(&app)[0];
    assert_eq!(generator.shape.kind, FillKind::Ring);
    assert_eq!(generator.shape.ring.change, 60);
    assert_eq!(app.services.history.undo_depth(), depth + 1);
}

#[test]
fn space_g_a_on_the_gen_page_keeps_the_loop_a_pass_heard_began_with() {
    let (mut app, mut realtime) = ringing();
    let written_shape = generators(&app)[0].shape;
    let first = notes(&app);
    keys(&mut app, " 8");
    assert_eq!(app.modes.view, View::Gen);
    assert!(matches!(app.track_control(), Control::Gen { .. }));
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Ring · Note"), "{text}");
    for expected in ["Change", "60%", "Chance", "Length"] {
        assert!(text.contains(expected), "{expected} in\n{text}");
    }
    // Nothing's been heard, so there's nothing to keep.
    keys(&mut app, " ga");
    assert_eq!(generators(&app)[0].shape, written_shape);
    assert!(
        app.services.status.contains("hasn't been heard"),
        "{}",
        app.services.status
    );

    // Two and a half times round its 16 rows.
    for _ in 0..4 {
        block(&mut realtime, &mut app);
    }
    app.services.handle.send(Command::PlayFrom { step: 0 });
    let pass = 16 * 6000;
    for _ in 0..(pass * 5 / 2) / 256 {
        block(&mut realtime, &mut app);
    }
    let depth = app.services.history.undo_depth();
    keys(&mut app, " ga");
    let kept = generators(&app)[0].shape;
    let third = written_shape.on_pass(2);
    assert_eq!(
        kept.ring.kept,
        Some(third.ring.kept_on(third.seed, 16)),
        "the register the third pass began with"
    );
    assert_eq!(kept.ring.change, 60, "Change stays where it is");
    // The phrase holds what the third pass played.
    let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
    let column = generators(&app)[0].column;
    let range = vt_model::selection::StepRange::between(0, 15);
    let mut heard = vec![Note::Empty; 16];
    for (row, step) in written(&steps, range, column, &third) {
        heard[row] = step.note;
    }
    assert_eq!(notes(&app), heard);
    assert_ne!(notes(&app), first, "a pass of its own");
    assert_eq!(generators(&app).len(), 1, "still live");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    assert!(
        app.services.status.contains("pass 3"),
        "{}",
        app.services.status
    );
    keys(&mut app, "u");
    assert_eq!(generators(&app)[0].shape, written_shape);
    // `r` starts a new ring, letting the loop kept go.
    keys(&mut app, " ga");
    assert!(generators(&app)[0].shape.ring.kept.is_some());
    keys(&mut app, "r");
    assert_eq!(generators(&app)[0].shape.ring.kept, None);
}
