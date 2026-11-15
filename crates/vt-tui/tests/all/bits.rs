//! The shape panel's `Multiply` and bit transforms, by keys.

use vt_model::shape::{FillKind, GenParam};
use vt_model::step::Note;
use vt_tui::app::App;

use crate::common::{app_with, keys};

const E: &str = "--- -- --- --- ---";
const NOTE: &str = "C-3 40 --- --- ---";

/// Each row as `x` for a note and `.` for none.
fn rhythm(app: &App) -> String {
    let id = app.services.phrase_id().unwrap();
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
    let panel = app.overlays.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(52, 14);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::overlays::shape::draw::draw(frame, frame.area(), app, &panel))
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
    let depth = app.services.history.undo_depth();
    keys(&mut app, "vGSKmultiply<Enter>");
    let panel = app.overlays.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Multiply);
    assert_eq!(
        panel.rows(),
        [GenParam::Fill, GenParam::Factor, GenParam::Masks]
    );
    // It starts at 3.
    assert_eq!(rhythm(&app), "xx..xx..xx..xx..");
    let screen = drawn(&app);
    for expected in [
        "Note: 8 of 16 rows",
        "Factor  3",
        "Masks   Off",
        "16 rows a number, row 1 its lowest bit",
    ] {
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
    assert_eq!(app.overlays.shape.as_ref().unwrap().shape.bits.factor, 8);
    keys(&mut app, "i17<Enter>");
    assert_eq!(app.services.status, "Factor runs from 1 to 16");
    keys(&mut app, "i5<Enter>");
    assert_eq!(rhythm(&app), "x.x.x.x.x.x.x.x.");
    keys(&mut app, "i1<Enter>");
    assert_eq!(rhythm(&app), "x...x...x...x...", "1 leaves it as it is");
    keys(&mut app, "x");
    assert_eq!(app.overlays.shape.as_ref().unwrap().shape.bits.factor, 3);
    // Masks keep rows 1, 2 and 13 to 16 before 3 multiplies.
    keys(&mut app, "j=");
    assert!(app.overlays.shape.as_ref().unwrap().shape.bits.masks);
    assert_eq!(rhythm(&app), "xx..........xx..");
    keys(&mut app, "<Enter>");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    assert_eq!(rhythm(&app), "xx..........xx..");
}

#[test]
fn multiply_is_only_offered_on_a_rhythm() {
    let (mut app, _rt) = app_with(&four_on_the_floor());
    keys(&mut app, "lvGS");
    let panel = app.overlays.shape.clone().unwrap();
    assert!(!panel.context().fills().contains(&FillKind::Multiply));
    let (mut app, _rt) = app_with(&[E; 16]);
    keys(&mut app, "vGS");
    let panel = app.overlays.shape.clone().unwrap();
    assert_eq!(panel.verb(), "Generate");
    assert!(!panel.context().fills().contains(&FillKind::Multiply));
}

/// Each row's velocity.
fn velocities(app: &App) -> Vec<String> {
    let id = app.services.phrase_id().unwrap();
    app.song().tracks[0].phrases[&id]
        .steps()
        .iter()
        .map(|step| step.to_string()[4..6].to_owned())
        .collect()
}

#[test]
fn the_bit_transforms_flip_keep_and_turn_a_columns_bits() {
    let (mut app, _rt) = app_with(&[
        "C-3 35 --- --- ---",
        "C-3 00 --- --- ---",
        "C-3 41 --- --- ---",
    ]);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "lvjjSKflipped<Enter>");
    let panel = app.overlays.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::InvertBits);
    assert_eq!(
        panel.rows(),
        [GenParam::Fill, GenParam::Flip, GenParam::Where]
    );
    // Flipping all seven of a velocity's bits.
    assert_eq!(velocities(&app), ["4A", "7F", "3E"]);
    let screen = drawn(&app);
    for expected in [
        "Fill    Invert bits",
        "Flip    FF",
        "The values there, as bits",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // `i` types the bits in hex.
    keys(&mut app, "ji0f");
    assert_eq!(velocities(&app), ["3A", "0F", "4E"]);
    // `Mask bits` keeps some, and `Rotate bits` turns them.
    keys(&mut app, "k=");
    assert_eq!(
        app.overlays.shape.as_ref().unwrap().shape.kind,
        FillKind::MaskBits
    );
    assert_eq!(velocities(&app), ["30", "00", "40"]);
    keys(&mut app, "=");
    assert_eq!(
        app.overlays.shape.as_ref().unwrap().shape.kind,
        FillKind::RotateBits
    );
    assert_eq!(velocities(&app), ["6A", "00", "03"]);
    keys(&mut app, "j=");
    assert!(drawn(&app).contains("Turn    2 bits"));
    assert_eq!(velocities(&app), ["55", "00", "06"]);
    keys(&mut app, "<Enter>");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
}
