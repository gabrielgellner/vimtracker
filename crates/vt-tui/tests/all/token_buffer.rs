//! A text-like setting's buffer, opened from its cell (decision 463, #1106): on a token cell `h`
//! `l` `j` `k` move between settings as on any; `Enter` opens the buffer on the first token; `i`
//! and `a` open it typing at the end, keeping the text; `cc` and `S` type it all again; `K` stays
//! the setting's own; and a named rhythm's text edited turns it `Own` without moving the cell.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::{GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};

use crate::common::*;

const NAMED: &str = "x . . x . . x . . . x . x . . .";

/// A live generator on a named rhythm, the Gen page on screen.
fn live_rhythm() -> (App, Realtime) {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16)
        .map(|_| "--- -- --- --- ---".parse().unwrap())
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, realtime) = app_for(song);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>L 5LLL");
    (app, realtime)
}

/// The Gen page on the author's screen, a line a row.
fn page(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_view(frame, frame.area(), app, View::Track))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..42)
        .map(|y| (0..163).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// Where a cell's name is drawn on the page: its line and its character.
fn drawn_at(app: &App, name: &str) -> (usize, usize) {
    page(app)
        .iter()
        .enumerate()
        .find_map(|(y, line)| line.find(name).map(|x| (y, line[..x].chars().count())))
        .unwrap_or_else(|| panic!("no {name} on the page"))
}

fn generator(app: &App) -> vt_model::generator::Generator {
    app.song().tracks[0].phrases[&0].generators()[0].clone()
}

/// Where the cursor is on the Gen page, counted from its first setting in `h` `l` `j` `k`: the
/// rows down and the settings across.
fn place_of(app: &mut App, param: GenParam) -> (usize, usize) {
    keys(app, "gg");
    for row in 0..12 {
        keys(app, "0");
        for column in 0..6 {
            if here(app) == Some(param) {
                return (row, column);
            }
            keys(app, "l");
            assert_eq!(app.modes.mode, Mode::Normal, "l only moves");
        }
        keys(app, "j");
    }
    panic!("no {param:?} on the Gen page");
}

#[test]
fn h_l_j_k_on_a_text_cell_move_between_settings() {
    let (mut app, _rt) = live_rhythm();
    place_of(&mut app, GenParam::OwnText);
    for key in ["h", "l", "j", "k"] {
        let before = here(&app);
        keys(&mut app, key);
        assert_eq!(app.modes.mode, Mode::Normal, "{key}");
        assert!(app.track_page.tokens.is_none(), "{key}");
        // Back where it was, by the opposite key, for the next.
        let back = match key {
            "h" => "l",
            "l" => "h",
            "j" => "k",
            _ => "j",
        };
        if here(&app) != before {
            keys(&mut app, back);
        }
    }
}

#[test]
fn i_and_a_type_at_the_end_keeping_the_text() {
    let (mut app, _rt) = live_rhythm();
    place_of(&mut app, GenParam::OwnText);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "i");
    assert_eq!(app.modes.mode, Mode::Tokens);
    let field = &app.track_page.tokens.as_ref().unwrap().tokens.field;
    assert_eq!(field.text(), format!("{NAMED} "));
    assert!(field.inserting().is_some());
    keys(&mut app, "x<Enter>");
    assert_eq!(app.modes.mode, Mode::Normal, "Enter closes it");
    assert_eq!(generator(&app).rhythm(), format!("{NAMED} x"));
    assert!(matches!(generator(&app).shape.place, Where::Own(_)));
    assert_eq!(app.services.history.undo_depth(), depth + 1, "one undo step");
    keys(&mut app, "a[x x]<Enter>");
    assert_eq!(generator(&app).rhythm(), format!("{NAMED} x [x x]"));
    keys(&mut app, "u");
    assert_eq!(generator(&app).rhythm(), format!("{NAMED} x"));
}

#[test]
fn cc_and_capital_s_type_it_all_again() {
    let (mut app, _rt) = live_rhythm();
    place_of(&mut app, GenParam::OwnText);
    keys(&mut app, "cc");
    assert_eq!(app.modes.mode, Mode::Tokens);
    let field = &app.track_page.tokens.as_ref().unwrap().tokens.field;
    assert_eq!(field.text(), "");
    keys(&mut app, "x x<Enter>");
    assert_eq!(generator(&app).rhythm(), "x x");
    keys(&mut app, "Sx . x<Enter>");
    assert_eq!(generator(&app).rhythm(), "x . x");
    // What was there is kept for `p`.
    keys(&mut app, "Sx . x<Esc>$p<Enter>");
    assert_eq!(generator(&app).rhythm(), "x . x x . x");
}

#[test]
fn enter_opens_it_on_the_first_token_and_k_stays_the_settings_own() {
    let (mut app, _rt) = live_rhythm();
    place_of(&mut app, GenParam::OwnText);
    keys(&mut app, "K");
    assert!(app.track_page.tokens.is_none(), "K isn't the buffer's");
    assert_ne!(app.modes.mode, Mode::Tokens);
    keys(&mut app, "<Esc>");
    place_of(&mut app, GenParam::OwnText);
    keys(&mut app, "<Enter>");
    assert_eq!(app.modes.mode, Mode::Tokens);
    assert_eq!(app.track_page.tokens.as_ref().unwrap().at, 0);
    assert_eq!(app.services.status, "Hit: x");
    // Drawn whole, under the cell, as typed.
    let text = page(&app).join("\n");
    assert!(text.contains("╭ Text "), "{text}");
    assert!(text.contains(NAMED), "{text}");
    // On `Rhythm`, `Enter` still opens the shape panel.
    keys(&mut app, "<Esc>");
    place_of(&mut app, GenParam::Rhythm);
    keys(&mut app, "<Enter>");
    assert!(app.overlays.shape.is_some());
}

/// Editing a named rhythm's text turns it `Own`, and the cell stays where it was.
#[test]
fn a_named_rhythms_text_edited_stays_where_it_was() {
    let (mut app, _rt) = live_rhythm();
    place_of(&mut app, GenParam::OwnText);
    let named = drawn_at(&app, "Text ");
    keys(&mut app, "<Enter>x<Enter>");
    assert!(matches!(generator(&app).shape.place, Where::Own(_)));
    assert_eq!(here(&app), Some(GenParam::OwnText), "the cursor stays on it");
    assert_eq!(drawn_at(&app, "Text "), named);
}
