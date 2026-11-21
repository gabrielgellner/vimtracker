//! The settings that were text-like, on the token editor (decision 459, #1089): a `List`'s values a
//! token each, turned in the column's units, with the room on the status line and shown whole in
//! the buffer (decision 463); a `Tonnetz`'s moves over the chords they reach, going round `P` `L`
//! `R`; a mask as eight bits beside its byte; and the Gen page's cells in a grammar opening their
//! buffer with `Enter`, its changes one undo step.

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
    // `i` on the row opens the buffer typing at the end, keeping the values there.
    keys(&mut app, "<Esc>i");
    let field = &panel(&app).tokens.as_ref().unwrap().field;
    assert_eq!(field.text(), "D#3 C-3 G-3 ");
    assert!(field.inserting().is_some());
    keys(&mut app, "A-3<Enter>");
    assert!(panel(&app).tokens.is_none());
    assert_eq!(panel(&app).list, [63, 60, 67, 69]);
}

/// A list longer than its row shows whole in its buffer, over as many lines as it takes, and one
/// with all the room it has refuses another, saying so.
#[test]
fn a_long_list_shows_whole_in_its_buffer_and_the_257th_is_refused() {
    let mut app = on("list", GenParam::Values);
    keys(&mut app, "S");
    keys(&mut app, "C-3 D#3 ".repeat(20).trim_end());
    keys(&mut app, "<Enter>l$");
    let drawn = shape_panel(&app, 80, 30);
    let row = drawn
        .lines()
        .find(|line| line.contains("Values"))
        .unwrap()
        .to_owned();
    assert!(row.contains("+"), "the row is cut: {row}");
    let inside: Vec<&str> = drawn
        .lines()
        .skip_while(|line| !line.contains("╭ Values"))
        .skip(1)
        .take_while(|line| !line.contains('╰'))
        .collect();
    let values: usize = inside
        .iter()
        .map(|line| line.matches("C-3").count() + line.matches("D#3").count())
        .sum();
    assert_eq!(values, 40, "{drawn}");
    assert!(inside.len() > 2, "wrapped: {drawn}");
    assert_eq!(app.services.status, "Value 40: D#3, 40 of 256 values");
    // At its most, `a` and `p` are refused.
    let mut app = on("list", GenParam::Values);
    keys(&mut app, "S");
    keys(&mut app, "C-3 ".repeat(MAX_LIST).trim_end());
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

/// Kept live, the Gen page's `Weights` cell shows its digits as typed, and `h` `l` `j` `k` move
/// on from it as from any cell; `Enter` opens its buffer, the digits over their notes, which a key
/// it doesn't bind doesn't leave, and its changes are heard as they're made and one undo step
/// when it closes (decision 463).
#[test]
fn the_gen_pages_cells_in_a_grammar_open_their_buffer() {
    let (mut app, _realtime) = app_with(&["--- -- --- --- ---"; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKrandom<Enter>L 5LLL");
    to_cell(&mut app, GenParam::Weights);
    let text = fx_page_text(&app).join("
");
    assert!(text.contains("Weights   FFFFF"), "{text}");
    keys(&mut app, "h");
    assert_eq!(app.modes.mode, Mode::Normal, "h moves on");
    assert_ne!(here(&app), Some(GenParam::Weights));
    keys(&mut app, "l");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(here(&app), Some(GenParam::Weights));
    let depth = app.services.history.undo_depth();
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.mode, Mode::Tokens);
    assert_eq!(app.key_scopes(), [Mode::Tokens]);
    let text = fx_page_text(&app).join("
");
    assert!(text.contains("╭ Weights "), "{text}");
    assert!(text.contains("F  F  F  F  F"), "{text}");
    assert!(text.contains("C  D# F  G  A#"), "{text}");
    keys(&mut app, "l-");
    let weights = |app: &App| {
        app.song().tracks[0].phrases[&0].generators()[0]
            .shape
            .random
            .weights
    };
    assert_eq!(weights(&app)[..5], [0xF, 0xE, 0xF, 0xF, 0xF], "heard at once");
    assert_eq!(app.services.status, "Weight of D#: E");
    keys(&mut app, "l0");
    assert_eq!(weights(&app)[..5], [0xF, 0xE, 0, 0xF, 0xF]);
    // `j`, and `h` from the first, stay in the buffer.
    keys(&mut app, "jhhhhh");
    assert_eq!(app.modes.mode, Mode::Tokens);
    assert_eq!(app.track_page.tokens.as_ref().unwrap().at, 0);
    // `Esc` closes it on the cell, and `u` takes back its changes at once.
    keys(&mut app, "<Esc>");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(here(&app), Some(GenParam::Weights));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    keys(&mut app, "u");
    assert_eq!(weights(&app)[..5], [0xF; 5]);
}

/// A live `List`'s values on the Gen page, in their field while the cursor is in them.
#[test]
fn a_live_lists_values_are_turned_on_the_gen_page() {
    let (mut app, _realtime) = app_with(&[NOTES; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKlist<Enter>L 5LLL");
    to_cell(&mut app, GenParam::Values);
    keys(&mut app, "<Enter>l=");
    let list = |app: &App| {
        app.song().tracks[0].phrases[&0].generators()[0]
            .list()
            .to_vec()
    };
    assert_eq!(list(&app), [60, 75], "C-4 up to D#4");
    assert_eq!(app.services.status, "Value 2: D#4, 2 of 256 values");
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Values    C-3 D#4"), "{text}");
}
