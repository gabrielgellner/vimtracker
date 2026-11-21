//! The settings that were text-like, on the token editor (decision 459, #1089): a `List`'s values a
//! token each, turned in the column's units, with the room on the status line and scrolling past
//! the field's width; a `Tonnetz`'s moves over the chords they reach, going round `P` `L` `R`; a
//! mask as eight bits beside its byte; and the Gen page's cells in a grammar gone into with `l`,
//! each change one undo step.

use vt_model::shape::{FillKind, GenParam, MAX_LIST};
use vt_tui::app::App;
use vt_tui::input::mode::Mode;

use crate::common::{app_with, command, down_to, fx_page_text, here, keys, shape_panel};

const NOTES: &str = "C-3 40 --- --- ---";

/// A note column of 16 rows, in C minor pentatonic, the panel open on a fill and on a row.
fn on(fill: &str, row: GenParam) -> App {
    let (mut app, _realtime) = app_with(&[NOTES; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, &format!("vGSK{fill}<Enter>"));
    down_to(&mut app, row);
    app
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

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

/// A list's values are tokens: `=` walks a note up the scale, `+` an octave, `x` takes one out
/// and `p` puts it back after another, and the status line says how much room there is.
#[test]
fn a_lists_values_are_turned_one_at_a_time_in_the_scale() {
    let mut app = on("list", GenParam::Values);
    assert_eq!(panel(&app).list, [60, 72], "C-3 and C-4");
    keys(&mut app, "l=");
    assert_eq!(panel(&app).list, [63, 72], "D#3, a degree up");
    assert_eq!(app.services.status, "Value 1: D#3, 2 of 256 values");
    keys(&mut app, "l_");
    assert_eq!(panel(&app).list, [63, 60]);
    keys(&mut app, "yitp");
    assert_eq!(panel(&app).list, [63, 60, 60]);
    keys(&mut app, "x");
    assert_eq!(panel(&app).list, [63, 60]);
    keys(&mut app, "aG-3<Esc>");
    assert_eq!(panel(&app).list, [63, 60, 67]);
    assert!(app.services.status.ends_with("3 of 256 values"));
    // `i` on the row starts from the values there.
    keys(&mut app, "<Esc>i");
    assert_eq!(panel(&app).typing.as_ref().unwrap().as_str(), "D#3 C-3 G-3");
}

/// A list longer than the field shows the values round the cursor, `…` where it's cut, and one
/// with all the room it has refuses another, saying so.
#[test]
fn a_long_list_scrolls_by_token_and_the_257th_is_refused() {
    let mut app = on("list", GenParam::Values);
    keys(&mut app, "i");
    keys(&mut app, &"C-3 D#3 ".repeat(20));
    keys(&mut app, "<Enter>l$");
    let drawn = shape_panel(&app, 80, 24);
    let row = drawn
        .lines()
        .find(|line| line.contains("Values"))
        .unwrap()
        .to_owned();
    assert!(row.contains("Values  …  "), "{row}");
    assert_eq!(app.services.status, "Value 40: D#3, 40 of 256 values");
    // At its most, `a` and `p` are refused.
    let mut app = on("list", GenParam::Values);
    keys(&mut app, "i");
    keys(&mut app, &"C-3 ".repeat(MAX_LIST));
    keys(&mut app, "<Enter>");
    assert_eq!(panel(&app).list.len(), MAX_LIST);
    keys(&mut app, "la");
    assert_eq!(
        app.services.status,
        "A list has up to 256 values: x takes one out"
    );
    keys(&mut app, "yitp");
    assert_eq!(panel(&app).list.len(), MAX_LIST);
}

/// Each of a `Tonnetz`'s moves is over the chord it reaches; `=` goes round `P`, `L` and `R`,
/// and `K` lists the three with the chord each would reach.
#[test]
fn moves_show_their_chords_and_turn_round_p_l_and_r() {
    let mut app = on("tonnetz", GenParam::MoveList);
    let drawn = shape_panel(&app, 80, 24);
    assert!(drawn.contains("List    P   L   R   L"), "{drawn}");
    assert!(drawn.contains("        Cm  G#  Fm  C#"), "{drawn}");
    keys(&mut app, "ll");
    assert_eq!(app.services.status, "Move 2: L, Cm to G#");
    keys(&mut app, "=");
    assert_eq!(panel(&app).shape.tonnetz.list.to_string(), "P R R L");
    assert_eq!(app.services.status, "Move 2: R, Cm to D#");
    let drawn = shape_panel(&app, 80, 24);
    assert!(drawn.contains("        Cm  D#  Cm  G#"), "{drawn}");
    keys(&mut app, "K");
    let drawn = shape_panel(&app, 80, 30);
    assert!(drawn.contains("to G#, leading-tone"), "{drawn}");
}

/// A mask is drawn as its eight bits, the lowest first, with its byte beside them; `=` in the
/// bits switches the one under the cursor, and on the row still steps the byte.
#[test]
fn a_mask_is_eight_bits_switched_one_at_a_time() {
    let mut app = on("mask", GenParam::Keep);
    assert_eq!(panel(&app).shape.kind, FillKind::MaskBits);
    let drawn = shape_panel(&app, 80, 24);
    assert!(drawn.contains("Keep    . . . . x x x x   F0"), "{drawn}");
    keys(&mut app, "l=");
    assert_eq!(panel(&app).shape.bits.keep, 0xF1);
    assert_eq!(app.services.status, "Bit 1: on, F1 in hex");
    keys(&mut app, "$-");
    assert_eq!(panel(&app).shape.bits.keep, 0x71, "{}", app.services.status);
    keys(&mut app, "6h1");
    assert_eq!(panel(&app).shape.bits.keep, 0x73);
    keys(&mut app, "<Esc>=");
    assert_eq!(panel(&app).shape.bits.keep, 0x74, "the row steps the byte");
}

/// Kept live, the Gen page's `Weights` cell is its digits over their notes, and `l` goes into
/// them as the panel's row does: each change is one undo step, and a key the field doesn't bind
/// leaves it and does what it does on the page.
#[test]
fn the_gen_pages_cells_in_a_grammar_take_the_token_editor() {
    let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKrandom<Enter>L 5LLL");
    to_cell(&mut app, GenParam::Weights);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Weights   F  F  F  F  F"), "{text}");
    assert!(text.contains("C  D# F  G  A#"), "{text}");
    let depth = app.services.history.undo_depth();
    keys(&mut app, "l");
    assert_eq!(app.modes.mode, Mode::Tokens);
    assert_eq!(app.key_scopes(), [Mode::Tokens]);
    keys(&mut app, "l-");
    let weights = |app: &App| {
        app.song().tracks[0].phrases[&0].generators()[0]
            .shape
            .random
            .weights
    };
    assert_eq!(weights(&app)[..5], [0xF, 0xE, 0xF, 0xF, 0xF]);
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    assert_eq!(app.services.status, "Weight of D#: E");
    keys(&mut app, "l0");
    assert_eq!(weights(&app)[..5], [0xF, 0xE, 0, 0xF, 0xF]);
    // `Esc` goes back to the cell, and `u` takes back a change at a time.
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(here(&app), Some(GenParam::Weights));
    keys(&mut app, "u");
    assert_eq!(weights(&app)[..5], [0xF, 0xE, 0xF, 0xF, 0xF]);
    // `j` leaves the field for the setting below.
    keys(&mut app, "lj");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert!(app.track_page.tokens.is_none());
    assert_ne!(here(&app), Some(GenParam::Weights));
}

/// A live `List`'s values on the Gen page, in their field while the cursor is in them.
#[test]
fn a_live_lists_values_are_turned_on_the_gen_page() {
    let (mut app, _realtime) = app_with(&[NOTES; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKlist<Enter>L 5LLL");
    to_cell(&mut app, GenParam::Values);
    keys(&mut app, "ll=");
    let list = |app: &App| {
        app.song().tracks[0].phrases[&0].generators()[0]
            .list()
            .to_vec()
    };
    assert_eq!(list(&app), [60, 75], "C-4 up to D#4");
    assert_eq!(app.services.status, "Value 2: D#4, 2 of 256 values");
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Values    C-3  D#4"), "{text}");
}
