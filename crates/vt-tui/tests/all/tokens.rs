//! The token editor (decision 456, #1086), on the shape panel's `Weights`, its first grammar: `~`
//! rests a digit and gives it back, `r` and `s` type over one, text that doesn't read is marked
//! with `✗` and a reason while the last that did keeps playing, `Enter` on it is refused and `Esc`
//! puts it back, and the buffer is a dialog over the panel (decision 463).

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::shape::{FillKind, GenParam};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
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
    // The row is the last that read, and the buffer what's typed.
    let drawn = shape_panel(&app, 60, 30);
    assert!(drawn.contains("Weights E  F  F  F  F"), "{drawn}");
    assert!(drawn.contains(" E  g  F  F  F"), "{drawn}");
    assert!(drawn.contains(" C  ✗  F  G  A#"), "{drawn}");
    assert!(drawn.contains("still the last text that read"), "{drawn}");
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

/// The buffer's keys come first, in a mode of their own; of the panel's, `Space d` describes the
/// digit, and the rest do nothing while it's open, as it's a dialog: `j` doesn't leave it.
#[test]
fn the_buffer_is_a_dialog_over_the_panel() {
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
    keys(&mut app, "jL");
    assert!(panel(&app).tokens.is_some());
    assert_eq!(panel(&app).row, GenParam::Weights);
    assert!(app.overlays.shape.is_some(), "L doesn't keep it");
    keys(&mut app, "<Esc>j");
    assert!(panel(&app).tokens.is_none());
    assert_eq!(panel(&app).row, below);
    assert_eq!(app.key_scopes(), [Mode::Shape]);
}

fn press(app: &mut App, code: KeyCode) {
    app.key(Key::new(code), Instant::now());
}

/// The list open in the field, its title and what's highlighted.
fn listed(app: &App) -> Option<(&'static str, String)> {
    let menu = panel(app).tokens.as_ref()?.menu.as_ref()?;
    Some((menu.picker.title, menu.highlighted()?.to_owned()))
}

/// `K` on a weight lists the weights, each with what it means, opened on the weight there, under
/// the digit; `j` `k` move through them and `Enter` puts one in its place (decision 458).
#[test]
fn k_on_a_weight_lists_the_weights_and_enter_puts_one_in_its_place() {
    let mut app = on_weights();
    keys(&mut app, "llK");
    assert_eq!(listed(&app), Some(("In its place", "F".to_owned())));
    let drawn = shape_panel(&app, 80, 30);
    assert!(drawn.contains("In its place 16"), "{drawn}");
    // Under the digit, scrolled to the weight there.
    assert!(drawn.contains("Weights F  F  F  F  F"), "{drawn}");
    assert!(drawn.contains("│  E  93% as often as F │"), "{drawn}");
    assert!(drawn.contains("│  F  the most          │"), "{drawn}");
    keys(&mut app, "kk");
    assert_eq!(listed(&app).unwrap().1, "D");
    assert_eq!(weights(&app), [0xF; 5], "nothing changes until Enter");
    keys(&mut app, "<Enter>");
    assert_eq!(listed(&app), None);
    assert_eq!(weights(&app), [0xF, 0xD, 0xF, 0xF, 0xF]);
    assert_eq!(panel(&app).weight, Some(1));
    assert_eq!(app.services.status, "Weight of D#: D");
    // `Esc` closes it and changes nothing, and the field's keys work again.
    keys(&mut app, "K<Esc>");
    assert_eq!(listed(&app), None);
    assert!(panel(&app).tokens.is_some());
    keys(&mut app, "-");
    assert_eq!(weights(&app), [0xF, 0xC, 0xF, 0xF, 0xF]);
}

/// `Ctrl-n` and `Ctrl-p` move through `K`'s list beside `j` and `k`, and `Ctrl-y` puts one in, as
/// `Enter` does (decision 477).
#[test]
fn ctrl_n_and_ctrl_p_move_through_ks_list() {
    let mut app = on_weights();
    keys(&mut app, "llK^p^p");
    assert_eq!(listed(&app).unwrap().1, "D");
    keys(&mut app, "^n");
    assert_eq!(listed(&app).unwrap().1, "E");
    keys(&mut app, "^y");
    assert_eq!(listed(&app), None);
    assert_eq!(weights(&app), [0xF, 0xE, 0xF, 0xF, 0xF]);
}

/// `Ctrl-e` closes the list and puts back what was typed, still typing; and `Tab` plays while
/// typing, completing nothing (decision 477).
#[test]
fn ctrl_e_closes_the_list_and_tab_plays_while_typing() {
    use vt_engine::engine::Transport;
    let (mut app, mut realtime) = app_with(&[E; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKrandom<Enter>");
    down_to(&mut app, GenParam::Weights);
    keys(&mut app, "ls^n^n");
    assert_eq!(listed(&app).unwrap().1, "1");
    keys(&mut app, "^e");
    assert_eq!(listed(&app), None);
    let tokens = panel(&app).tokens.as_ref().unwrap();
    assert_eq!(
        tokens.field.typed_word(),
        Some(""),
        "what was typed is back"
    );
    assert_eq!(weights(&app), [0xF; 5]);
    press(&mut app, KeyCode::Tab);
    crate::common::block(&mut realtime, &mut app);
    assert_eq!(app.services.reported.transport, Transport::Playing);
    assert_eq!(listed(&app), None, "Tab lists nothing");
    let tokens = panel(&app).tokens.as_ref().unwrap();
    assert!(tokens.field.inserting().is_some(), "and typing goes on");
    keys(&mut app, "3");
    assert_eq!(weights(&app), [3, 0xF, 0xF, 0xF, 0xF]);
}

/// `Ctrl-n` while typing lists what can go, as the `:` line's does, `Ctrl-n` and `Ctrl-p` going
/// through it, and `Ctrl-y` or `Enter` types the one highlighted, which on `Weights` ends the
/// typing; `K` at the start of what's typed lists the same (decision 477).
#[test]
fn ctrl_n_while_typing_completes_from_the_same_list() {
    let mut app = on_weights();
    keys(&mut app, "ls");
    assert_eq!(
        app.services.status,
        "K lists what can go here, Ctrl-n completes a word"
    );
    keys(&mut app, "^n");
    assert_eq!(listed(&app), Some(("What can go here", "0".to_owned())));
    keys(&mut app, "^p");
    assert_eq!(
        listed(&app).unwrap().1,
        "F",
        "round from the first to the last"
    );
    keys(&mut app, "^p^y");
    assert_eq!(weights(&app), [0xE, 0xF, 0xF, 0xF, 0xF]);
    let tokens = panel(&app).tokens.as_ref().unwrap();
    assert!(tokens.field.inserting().is_none(), "a weight is one digit");
    assert!(!tokens.completes());
    keys(&mut app, "sK");
    assert_eq!(listed(&app), Some(("What can go here", "0".to_owned())));
    keys(&mut app, "<Esc>");
    assert_eq!(listed(&app), None);
    let tokens = panel(&app).tokens.as_ref().unwrap();
    assert!(
        tokens.field.inserting().is_some(),
        "Esc closes only the list"
    );
    keys(&mut app, "3");
    assert_eq!(weights(&app), [3, 0xF, 0xF, 0xF, 0xF]);
}
