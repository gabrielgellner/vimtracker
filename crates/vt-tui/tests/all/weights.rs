//! A random fill's `Weights` in the shape panel, digit by digit (decision 388, #972): `l` goes into
//! them on the first, `h` and `l` go across, `=` `-` `+` `_` and a hex digit change the one under
//! the cursor, each heard as it changes, and each digit has its note under it.

use vt_model::shape::{FillKind, GenParam};
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::screen::describe;

use crate::common::{app_with, command, down_to, keys, press, shape_panel};

const E: &str = "--- -- --- --- ---";

/// An empty Note column, a random fill on it in this scale, and the cursor on `Weights`.
fn on_weights(scale: &str) -> App {
    let (mut app, _realtime) = app_with(&[E; 16]);
    command(&mut app, &format!("set scale={scale}"));
    keys(&mut app, "vGSKrandom<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Random);
    down_to(&mut app, GenParam::Weights);
    app
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

fn weights(app: &App, count: usize) -> Vec<u8> {
    panel(app).shape.random.weights[..count].to_vec()
}

/// The notes the column plays now, by their pitch class from C.
fn pitches(app: &App) -> Vec<u8> {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .filter_map(|step| match step.note {
            Note::On(note) => Some(note % 12),
            _ => None,
        })
        .collect()
}

#[test]
fn l_goes_into_the_weights_on_the_first_digit_and_names_its_note() {
    let mut app = on_weights("minor-pentatonic");
    assert_eq!(panel(&app).weight, None);
    keys(&mut app, "l");
    assert_eq!(panel(&app).weight, Some(0));
    assert_eq!(panel(&app).weight_here(), Some((0, "C", 0xF)));
    assert_eq!(app.services.status, "Weight of C: F");
    // Each digit over its note, the scale's degrees from its root, spelled as the grid spells them.
    let drawn = shape_panel(&app, 60, 24);
    assert!(drawn.contains("Weights F  F  F  F  F"), "{drawn}");
    assert!(drawn.contains("        C  D# F  G  A#"), "{drawn}");
    // `h` from the first leaves them, and the panel stays open.
    keys(&mut app, "h");
    assert_eq!(panel(&app).weight, None);
    assert_eq!(panel(&app).row, GenParam::Weights);
}

#[test]
fn with_the_scale_off_there_are_twelve_digits_a_semitone_each() {
    let mut app = on_weights("off");
    assert!(!app.services.live.scale.on);
    keys(&mut app, "l$");
    assert_eq!(panel(&app).weight_here(), Some((11, "B", 0xF)));
    let drawn = shape_panel(&app, 60, 24);
    assert!(
        drawn.contains("        C  C# D  D# E  F  F# G  G# A  A# B"),
        "{drawn}"
    );
    // The panel grows to fit them, on the author's screen.
    assert!(
        drawn
            .lines()
            .any(|line| line.contains("Weights F  F  F  F  F  F  F  F  F  F  F  F"))
    );
}

#[test]
fn h_l_and_the_adjusting_keys_change_the_digit_under_the_cursor_as_its_heard() {
    let mut app = on_weights("minor-pentatonic");
    keys(&mut app, "lll-");
    assert_eq!(weights(&app, 5), [0xF, 0xF, 0xE, 0xF, 0xF]);
    assert_eq!(app.services.status, "Weight of F: E");
    keys(&mut app, "h__");
    assert_eq!(weights(&app, 5), [0xF, 0x7, 0xE, 0xF, 0xF]);
    keys(&mut app, "+=");
    assert_eq!(weights(&app, 5), [0xF, 0xC, 0xE, 0xF, 0xF]);
    // `x` puts it back to `F`; `^` and `$` go to the ends; it never goes past them.
    keys(&mut app, "x");
    press(&mut app, "^-$-lll-");
    assert_eq!(weights(&app, 5), [0xE, 0xF, 0xE, 0xF, 0xD]);
    keys(&mut app, "=======");
    assert_eq!(weights(&app, 5)[4], 0xF);
    // A hex digit sets it and moves on: only the root and the fifth let through, the preview
    // follows at once.
    press(&mut app, "^f00f0");
    assert_eq!(weights(&app, 5), [0xF, 0, 0, 0xF, 0]);
    assert_eq!(panel(&app).weight, Some(4));
    let heard = pitches(&app);
    assert!(!heard.is_empty());
    assert!(
        heard.iter().all(|pitch| [0, 7].contains(pitch)),
        "{heard:?}"
    );
    // `Esc` leaves the digits, not the panel, and a second closes it as ever.
    keys(&mut app, "<Esc>");
    assert_eq!(panel(&app).weight, None);
    assert!(app.overlays.shape.is_some());
}

#[test]
fn i_types_them_all_starting_from_the_digits_there() {
    let mut app = on_weights("minor-pentatonic");
    keys(&mut app, "l-<Esc>i");
    let typing = panel(&app).typing.clone().expect("typing");
    assert_eq!(typing, "EFFFF");
    keys(&mut app, "<BS><BS>00<Enter>");
    assert_eq!(weights(&app, 5), [0xE, 0xF, 0xF, 0, 0]);
    // From inside the digits too. Typed at once, the digits start afresh, as a selected field's
    // text does, and those after the digits typed keep theirs.
    keys(&mut app, "li");
    assert_eq!(panel(&app).weight, None);
    assert_eq!(panel(&app).typing.clone().expect("typing"), "EFF00");
    keys(&mut app, "f0");
    assert_eq!(panel(&app).typing.clone().expect("typing"), "F0");
    keys(&mut app, "<Enter>");
    assert_eq!(weights(&app, 5), [0xF, 0, 0xF, 0, 0]);
}

#[test]
fn space_d_describes_the_digit_under_the_cursor() {
    let mut app = on_weights("minor-pentatonic");
    keys(&mut app, " dllll-");
    assert_eq!(panel(&app).weight, Some(3));
    assert_eq!(
        describe::text(&app).expect("a line"),
        "Weight of G: E, how often G is chosen, from 0, never, to F, the most."
    );
}

#[test]
fn the_digits_changed_are_kept_or_put_back_as_one_undo() {
    let mut app = on_weights("minor-pentatonic");
    let depth = app.services.history.undo_depth();
    keys(&mut app, "lf0000");
    // Changes are heard but not yet kept.
    assert_eq!(app.services.history.undo_depth(), depth);
    assert!(pitches(&app).iter().all(|pitch| *pitch == 0));
    // `Enter` leaves the digits; a second keeps the panel, as one undo.
    keys(&mut app, "<Enter><Enter>");
    assert!(app.overlays.shape.is_none());
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert!(pitches(&app).is_empty(), "the column is empty again");
}
