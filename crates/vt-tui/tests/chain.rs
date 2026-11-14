//! The chain view and phrases in chains: adding, cloning and sharing them.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

#[test]
fn cuj_5_reuse_and_vary_a_phrase() {
    let (mut app, _realtime) = app();
    // Add the same phrase twice to this chain, then open the second use.
    press(&mut app, " 2");
    assert_eq!(app.view, View::Chain);
    press(&mut app, "yyp");
    assert_eq!(app.chain_entries().len(), 2);
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert!(app.status.contains("used 2 times"), "{}", app.status);
    // The clone stays here. An edit reaches only the clone.
    press(&mut app, "gc");
    assert_eq!(app.phrase_id(), Some(1));
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[0].phrases[&0].steps()[0].note,
        Note::Empty
    );
    assert_eq!(
        app.song().tracks[0].phrases[&1].steps()[0].note,
        Note::On(60)
    );
    assert_eq!(app.chain_entries()[0].phrase, 0);
    // Ctrl-O returns to where the jump started, and Ctrl-I goes forward again.
    let back = parse_sequence("C-o").unwrap()[0];
    app.key(back, Instant::now());
    assert_eq!(app.view, View::Chain);
    assert_eq!(app.cursor.slot, 1);
    app.key(parse_sequence("C-i").unwrap()[0], Instant::now());
    assert_eq!(app.view, View::Phrase);
    // Undo removes the clone's use again.
    press(&mut app, "uu");
    assert_eq!(app.chain_entries()[1].phrase, 0);
}

#[test]
fn gd_on_the_empty_row_adds_the_lowest_unused_phrase() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2G");
    assert_eq!(app.cursor.slot, 1);
    assert_eq!(app.phrase_id(), None);
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.phrase_id(), Some(1));
    assert_eq!(app.chain_entries().len(), 2);
    // Leaving the chain view never leaves the cursor on the empty row.
    press(&mut app, " 2G 3");
    assert_eq!(app.cursor.slot, 1);
    assert_eq!(app.phrase_id(), Some(1));
    assert_eq!(notes(&app).len(), 16);
}

#[test]
fn the_chain_view_edits_rows_and_keeps_one() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2");
    press(&mut app, "dd");
    assert!(app.status.contains("at least one"), "{}", app.status);
    // Typing a phrase on the empty row adds it, and a new ID is created.
    press(&mut app, "ji1f");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].phrase, 0x1F);
    assert!(app.song().tracks[0].phrases.contains_key(&0x1F));
    // Transpose by keys and by typing, and out of range is refused.
    press(&mut app, "l5=");
    assert_eq!(app.chain_entries()[1].transpose, 5);
    press(&mut app, "i-12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].transpose, -12);
    press(&mut app, "i60");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries()[1].transpose, -12);
    assert!(app.status.contains("48"), "{}", app.status);
    press(&mut app, "99-");
    assert_eq!(app.chain_entries()[1].transpose, -48);
    // Removing the row copies it, so it can be pasted back.
    press(&mut app, "dd");
    assert_eq!(app.chain_entries().len(), 1);
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "kp");
    assert_eq!(app.chain_entries()[1].phrase, 0x1F);
}

#[test]
fn phrase_view_keys_move_through_the_chain_and_grr_lists_uses() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2yyp 3");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "[P");
    assert_eq!(app.cursor.slot, 0);
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 1, "motions stop at the edge");
    press(&mut app, "[p]P");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "grr");
    assert_eq!(app.mode, Mode::Picker);
    let uses: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(uses, ["Section 01 slot 00", "Section 01 slot 01"]);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.cursor.slot, 0);
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn a_shared_chain_warns_on_clone_and_gc_big_separates_it() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1gc 3");
    assert_eq!(app.chain_uses(), 2);
    press(&mut app, "gc");
    assert!(app.status.contains("shared"), "{}", app.status);
    press(&mut app, "gC");
    assert_eq!(app.chain_uses(), 1);
    assert_ne!(app.chain_id(), Some(0));
}

#[test]
fn undo_keeps_the_cursor_on_a_row_that_exists() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2yyp 3");
    assert_eq!(app.cursor.slot, 1);
    press(&mut app, "u");
    assert_eq!(app.cursor.slot, 0);
    assert_eq!(app.phrase_id(), Some(0));
}

#[test]
fn k_chooses_a_chain_rows_phrase_and_large_steps_work_there() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2Ggd 2Ggd 2ggjdddd 2gg");
    assert_eq!(app.chain_entries().len(), 1);
    let row = |app: &App, slot: usize| app.chain_entries()[slot];
    let start = row(&app, 0).phrase;

    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    let count = picker.shown().count();
    assert!(count >= 2);
    assert_eq!(picker.selected, start as usize);
    press(&mut app, "02");
    special(&mut app, KeyCode::Enter);
    assert_eq!(row(&app, 0).phrase, 2);

    // The empty row past the end adds a row.
    press(&mut app, "jK");
    press(&mut app, "01");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries().len(), 2);
    assert_eq!(row(&app, 1).phrase, 1);

    // Transpose isn't a list, and moves by octaves.
    press(&mut app, "lK");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Phrase"), "{}", app.status);
    press(&mut app, "+");
    assert_eq!(row(&app, 1).transpose, 12);
    press(&mut app, "=");
    assert_eq!(row(&app, 1).transpose, 13);
    press(&mut app, "_");
    assert_eq!(row(&app, 1).transpose, 1);
}

#[test]
fn zero_and_dollar_pick_the_chain_columns() {
    use vt_tui::tree::ChainColumn;
    let (mut app, _realtime) = app();
    press(&mut app, " 2$");
    assert_eq!(app.chain_column, ChainColumn::Transpose);
    press(&mut app, "0");
    assert_eq!(app.chain_column, ChainColumn::Phrase);
}

#[test]
fn x_in_a_chain_only_resets_transpose() {
    let (mut app, _realtime) = app();
    press(&mut app, " 2x");
    assert!(app.status.contains("dd"), "{}", app.status);
    assert_eq!(app.chain_entries().len(), 1);
    press(&mut app, "l+x");
    assert_eq!(app.chain_entries()[0].transpose, 0);
}

/// `:bloom` grows branches from the phrase here, each following the one before it, and writes
/// their path in the row's place, as one undo step (decision 307).
#[test]
fn bloom_grows_branches_from_this_phrase_and_writes_their_path_into_the_chain() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "jjjjid");
    special(&mut app, KeyCode::Esc);
    let depth = app.history.undo_depth();
    command(&mut app, "bloom 3 1 trunk");
    let phrases: Vec<(u32, i8)> = app
        .chain_entries()
        .iter()
        .map(|entry| (entry.phrase, entry.transpose))
        .collect();
    assert_eq!(phrases, [(0, 0), (1, 0), (0, 0), (2, 0), (0, 0), (3, 0)]);
    assert!(
        app.status.contains("00 grew 3 branches, 01 02 03"),
        "{}",
        app.status
    );
    assert!(app.status.contains("played trunk"), "{}", app.status);
    let track = &app.song().tracks[0];
    for branch in 1..=3 {
        let generator = &track.phrases[&branch].generators()[0];
        assert_eq!(generator.source().unwrap().phrase, branch - 1);
        assert_eq!(generator.shape.follow.mutation, 1);
    }
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    press(&mut app, "u");
    assert_eq!(app.chain_entries().len(), 1);
    assert_eq!(app.song().tracks[0].phrases.len(), 1);
    // The trunk's notes reach every branch, in one edit.
    command(&mut app, "bloom 2");
    assert_eq!(app.chain_entries().len(), 3);
    press(&mut app, "ggig");
    special(&mut app, KeyCode::Esc);
    let track = &app.song().tracks[0];
    let firsts = (1..=2)
        .filter(|branch| track.phrases[branch].steps()[0].note == Note::On(67))
        .count();
    assert!(firsts >= 1, "a branch plays the trunk's new G");
}

#[test]
fn bloom_alone_lists_the_paths_and_refuses_what_it_cant_grow() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "bloom");
    let picker = app.picker.as_ref().unwrap();
    let items: Vec<String> = picker.shown().map(listed).collect();
    assert_eq!(items.len(), 5);
    assert_eq!(items[1], "trunk  The trunk before each branch: T 1 T 2 T 3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.chain_entries().len(), 4, "three branches, forward");
    command(&mut app, "bloom 8");
    assert!(app.status.contains(":bloom takes"), "{}", app.status);
    command(&mut app, "bloom 2 sideways");
    assert!(app.status.contains(":bloom takes"), "{}", app.status);
    assert_eq!(app.chain_entries().len(), 4);
}
