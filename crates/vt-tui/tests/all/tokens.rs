//! The token editor (decision 456, #1086), on the shape panel's `Weights`, its first grammar: `~`
//! rests a digit and gives it back, `r` and `s` type over one, text that doesn't read is marked
//! with `✗` and a reason while the last that did keeps playing, `Enter` on it is refused and `Esc`
//! puts it back, and the panel's own keys still work from inside the tokens.

use vt_model::shape::{FillKind, GenParam};
use vt_tui::app::App;
use vt_tui::input::mode::Mode;
use vt_tui::screen::describe;

use crate::common::{app_with, command, down_to, keys, shape_panel};

const E: &str = "--- -- --- --- ---";

fn on_weights() -> App {
    let (mut app, _realtime) = app_with(&[E; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKrandom<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Random);
    down_to(&mut app, GenParam::Weights);
    app
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

fn weights(app: &App) -> Vec<u8> {
    panel(app).shape.random.weights[..5].to_vec()
}

#[test]
fn tilde_rests_a_weight_and_gives_it_back() {
    let mut app = on_weights();
    keys(&mut app, "ll-~");
    assert_eq!(weights(&app), [0xF, 0, 0xF, 0xF, 0xF]);
    assert_eq!(app.services.status, "Weight of D#: 0");
    keys(&mut app, "~");
    assert_eq!(weights(&app), [0xF, 0xE, 0xF, 0xF, 0xF]);
    // A weight of 0 that wasn't rested here has nothing to give back.
    keys(&mut app, "l0h~");
    assert_eq!(weights(&app), [0xF, 0xE, 0, 0xF, 0xF]);
    assert_eq!(
        app.services.status,
        "A rest already, with nothing to give back"
    );
}

#[test]
fn r_replaces_a_digit_and_stays_on_it() {
    let mut app = on_weights();
    keys(&mut app, "lr8");
    assert_eq!(weights(&app), [8, 0xF, 0xF, 0xF, 0xF]);
    assert_eq!(panel(&app).weight, Some(0));
    assert!(
        panel(&app)
            .tokens
            .as_ref()
            .unwrap()
            .field
            .inserting()
            .is_none()
    );
    keys(&mut app, "ls4");
    assert_eq!(weights(&app), [8, 4, 0xF, 0xF, 0xF]);
    assert_eq!(app.services.status, "Weight of D#: 4");
}

/// Text that doesn't read is marked, the reason said, and the column plays the last text that did;
/// `Enter` is refused and `Esc` puts it back.
#[test]
fn text_that_doesnt_read_is_marked_and_the_last_that_did_keeps_playing() {
    let mut app = on_weights();
    keys(&mut app, "l-lrg");
    assert_eq!(
        weights(&app),
        [0xE, 0xF, 0xF, 0xF, 0xF],
        "the last that read"
    );
    assert_eq!(
        app.services.status,
        "g isn't a weight, a hex digit from 0 to F; the column plays the last text that read. \
         Esc puts it back"
    );
    let drawn = shape_panel(&app, 60, 24);
    assert!(drawn.contains("Weights E  g  F  F  F"), "{drawn}");
    assert!(drawn.contains("        C  ✗  F  G  A#"), "{drawn}");
    keys(&mut app, "<Enter>");
    assert!(panel(&app).tokens.is_some(), "Enter is refused");
    assert!(
        app.services.status.starts_with("g isn't a weight"),
        "{}",
        app.services.status
    );
    keys(&mut app, "<Esc>");
    assert!(panel(&app).tokens.is_none());
    keys(&mut app, "l");
    let drawn = shape_panel(&app, 60, 24);
    assert!(drawn.contains("Weights E  F  F  F  F"), "{drawn}");
    assert_eq!(weights(&app), [0xE, 0xF, 0xF, 0xF, 0xF]);
}

/// The token editor's keys come first, in a mode of their own, and the panel's are there under
/// them: `j` leaves the tokens for the next row, and `Space d` describes the digit.
#[test]
fn the_panels_keys_still_work_from_the_tokens() {
    let mut app = on_weights();
    keys(&mut app, "l");
    assert_eq!(app.key_scopes(), [Mode::Tokens, Mode::Shape]);
    keys(&mut app, " d");
    assert_eq!(panel(&app).weight, Some(0));
    assert_eq!(
        describe::text(&app).expect("a line"),
        "Weight of C: F, how often C is chosen, from 0, never, to F, the most."
    );
    let next = panel(&app).rows();
    let below = next[next
        .iter()
        .position(|row| *row == GenParam::Weights)
        .unwrap()
        + 1];
    keys(&mut app, "j");
    assert!(panel(&app).tokens.is_none());
    assert_eq!(panel(&app).row, below);
    assert_eq!(app.key_scopes(), [Mode::Shape]);
}
