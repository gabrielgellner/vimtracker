//! The rows a token writes, and the token playing (decision 460, #1090): the shape panel's rows
//! line under `Weights` says each row of the span as `x` or `.`, the rows the digit under the
//! cursor writes are lit in it and in the phrase view's column, and while the phrase plays the
//! digit that wrote the row playing is in the playhead's colour, in the panel and on the Gen page.
//! A `Tonnetz`'s moves light the rows their chords are on, and the move playing (decision 473).

use ratatui::style::Modifier;
use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::shape::{FillKind, GenParam, degree_of};
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::mode::View;
use vt_tui::widgets::theme;

use crate::common::{app_with, command, down_to, here, keys, screen, shape_panel};

const E: &str = "--- -- --- --- ---";

fn pentatonic() -> Scale {
    Scale {
        on: true,
        kind: ScaleKind::MinorPentatonic,
        ..Scale::default()
    }
}

/// Sixteen empty rows, a random fill generating notes on them in C minor pentatonic, and the
/// cursor on `Weights`.
fn on_weights() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&[E; 16]);
    command(&mut app, "set scale=minor-pentatonic");
    keys(&mut app, "vGSKrandom<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Random);
    down_to(&mut app, GenParam::Weights);
    (app, realtime)
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

/// The degree from the root of each row's note, as the phrase holds it.
fn degrees(app: &App) -> Vec<Option<usize>> {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            Note::On(note) => degree_of(pentatonic(), note),
            _ => None,
        })
        .collect()
}

/// The rows the cursor's digit lights, as the panel works them out for the grid.
fn lit(app: &App) -> Vec<usize> {
    panel(app)
        .token_rows(0, 0)
        .map(|(_, rows)| rows)
        .unwrap_or_default()
}

#[test]
fn the_rows_line_says_each_row_and_lights_the_cursors() {
    let (mut app, _realtime) = on_weights();
    let drawn = shape_panel(&app, 60, 24);
    assert!(
        drawn
            .lines()
            .any(|line| line.contains("Rows") && line.contains(&["x"; 16].join(" "))),
        "every row written: {drawn}"
    );
    assert!(lit(&app).is_empty(), "nothing lit outside the tokens");
    // Along the digits, the light moves along the rows on each one's degree.
    let degrees = degrees(&app);
    let mut every = Vec::new();
    keys(&mut app, "l");
    for degree in 0..5 {
        let rows = lit(&app);
        let on: Vec<usize> = (0..16)
            .filter(|row| degrees[*row] == Some(degree))
            .collect();
        assert_eq!(rows, on, "the rows on degree {degree}");
        every.extend(rows);
        keys(&mut app, "l");
    }
    every.sort_unstable();
    assert_eq!(every, (0..16).collect::<Vec<_>>(), "each row a digit's");
}

/// The phrase view lights the rows the digit under the cursor writes, in the cursor's colour, as
/// the digit is, but for the row with the grid's own cursor, which stays the cursor.
#[test]
fn the_phrase_view_lights_the_rows_of_the_digit_under_the_cursor() {
    let (mut app, _realtime) = on_weights();
    // The rows of the grid with a cell in the cursor's colour, by their numbers.
    let lit_rows = |app: &App| {
        let buffer = screen(app);
        let lines = crate::common::drawn_lines(&buffer);
        (0..16)
            .filter(|row| {
                let number = format!("{row:02X}");
                let Some(y) = lines
                    .iter()
                    .position(|line| line.get(4..6) == Some(number.as_str()))
                else {
                    return false;
                };
                (0..buffer.area.width).any(|x| {
                    let cell = &buffer[(x, y as u16)];
                    cell.fg == theme::ROSEWATER && cell.modifier.contains(Modifier::BOLD)
                })
            })
            .collect::<Vec<usize>>()
    };
    assert!(lit_rows(&app).is_empty());
    let degrees = degrees(&app);
    for degree in 0..5 {
        keys(&mut app, "l");
        let on: Vec<usize> = (1..16)
            .filter(|row| degrees[*row] == Some(degree))
            .collect();
        assert_eq!(lit_rows(&app), on, "degree {degree}");
    }
}

/// The phrase playing, its playhead on a row.
fn playing_at(app: &mut App, row: usize) {
    app.services.reported.transport = Transport::Playing;
    app.services.reported.jam[0].chain = Some(0);
    app.set_playhead(row);
}

/// While the phrase plays, the digit that wrote the row playing is the panel's token playing, in
/// the playhead's colour.
#[test]
fn the_digit_that_wrote_the_row_playing_is_lit_in_the_panel() {
    let (mut app, _realtime) = on_weights();
    let field = panel(&app).field(GenParam::Weights).unwrap();
    let row = 5;
    playing_at(&mut app, row);
    let playing = panel(&app).playing_token(&app.ctx(), &field);
    assert_eq!(playing, degrees(&app)[row], "row {row}");
}

/// Kept live, the Gen page's `Weights` device names the weight that wrote the row playing in the
/// playhead's colour.
#[test]
fn the_gen_page_names_the_weight_playing_in_the_playheads_colour() {
    let (mut app, _realtime) = on_weights();
    keys(&mut app, "L");
    assert!(app.overlays.shape.is_none(), "kept live");
    keys(&mut app, " 8");
    assert_eq!(app.modes.view, View::Gen);
    for _ in 0..12 {
        if here(&app) == Some(GenParam::Weight1) {
            break;
        }
        keys(&mut app, "j");
    }
    assert_eq!(here(&app), Some(GenParam::Weight1));
    let degrees = degrees(&app);
    let row = 3;
    playing_at(&mut app, row);
    let degree = degrees[row].expect("a note");
    let name = ["C", "D#", "F", "G", "A#"][degree];
    let buffer = crate::common::track_page_buffer(&app);
    let lines = crate::common::drawn_lines(&buffer);
    let title = lines
        .iter()
        .position(|line| line.contains("Weights ·"))
        .expect("the device");
    // The names of the device's cells, under its title: the one playing alone is green.
    let green: Vec<String> = (title..title + 6)
        .flat_map(|y| {
            let buffer = &buffer;
            (0..buffer.area.width).filter_map(move |x| {
                let cell = &buffer[(x, y as u16)];
                (cell.fg == theme::GREEN && cell.symbol() != " ").then(|| cell.symbol().to_owned())
            })
        })
        .collect();
    assert_eq!(green.concat(), name, "row {row} is on {name}: {lines:#?}");
}

/// The digit playing is drawn in the playhead's colour, among the others.
#[test]
fn the_panel_draws_the_digit_playing_in_the_playheads_colour() {
    let (mut app, _realtime) = on_weights();
    let degrees = degrees(&app);
    let row = 4;
    playing_at(&mut app, row);
    // Off the row, where its digits aren't lit together as one value.
    keys(&mut app, "k");
    let panel = app.overlays.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(60, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::overlays::shape::draw::draw(frame, frame.area(), &app.ctx(), &panel))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let lines = crate::common::drawn_lines(buffer);
    let y = lines
        .iter()
        .position(|line| line.contains("Weights"))
        .expect("the digits");
    let before: String = lines[y].split("Weights").next().unwrap().to_owned();
    let start = before.chars().count() + "Weights".len();
    // The digits by their place, and which of them is green.
    let digits: Vec<bool> = (start as u16..buffer.area.width)
        .filter(|x| !matches!(buffer[(*x, y as u16)].symbol(), " " | "│"))
        .map(|x| buffer[(x, y as u16)].fg == theme::GREEN)
        .collect();
    assert_eq!(digits.len(), 5, "{lines:#?}");
    assert_eq!(digits.iter().position(|green| *green), degrees[row]);
    assert_eq!(digits.iter().filter(|green| **green).count(), 1);
}

/// On the Gen page, the cursor on a weight marks the rows it writes on the generator's preview, so
/// its picture is sent again as the cursor goes from one weight to the next.
#[test]
fn the_gen_pages_preview_marks_the_rows_of_the_weight_under_the_cursor() {
    use vt_tui::graphics::Graphics;
    let (mut app, _realtime) = on_weights();
    keys(&mut app, "L");
    // The whole page, whose previews show (decision 467).
    keys(&mut app, " 8^wo");
    let mut graphics = Graphics::new();
    let sent = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        app.send_pictures(graphics, &mut out).unwrap();
        crate::common::screen_cells(app);
        String::from_utf8_lossy(&out).matches("a=T").count()
    };
    sent(&mut app, &mut graphics);
    assert_eq!(sent(&mut app, &mut graphics), 0, "nothing new");
    for _ in 0..12 {
        if here(&app) == Some(GenParam::Weight1) {
            break;
        }
        keys(&mut app, "j");
    }
    assert_eq!(here(&app), Some(GenParam::Weight1));
    assert_eq!(sent(&mut app, &mut graphics), 1, "C's rows marked");
    // A picture that changes again within two frames waits, so a frame goes by first.
    assert_eq!(sent(&mut app, &mut graphics), 0, "nothing new");
    keys(&mut app, "l");
    assert_eq!(here(&app), Some(GenParam::Weight2));
    assert_eq!(sent(&mut app, &mut graphics), 1, "D#'s rows marked");
}

const NOTES: &str = "C-3 40 --- --- ---";

/// Sixteen notes, a `Tonnetz` walking `P L R L` on them, and the cursor in its moves' buffer on
/// the first, `P` (decision 473).
fn on_moves() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&[NOTES; 16]);
    keys(&mut app, "vGSKtonnetz<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Tonnetz);
    down_to(&mut app, GenParam::MoveList);
    keys(&mut app, "l");
    (app, realtime)
}

/// A `Tonnetz`'s move lights the rows whose chord it reached, its buffer has the rows line, and
/// the row playing names the move that reached its chord; the first row, the chord it starts on,
/// is no move's (decision 473).
#[test]
fn a_move_lights_the_rows_its_chord_is_on_and_the_move_playing() {
    let (mut app, _realtime) = on_moves();
    assert_eq!(lit(&app), [1, 5, 9, 13], "P's rows");
    keys(&mut app, "l");
    assert_eq!(lit(&app), [2, 6, 10, 14], "L's rows");
    keys(&mut app, "ll");
    assert_eq!(lit(&app), [4, 8, 12], "the second L's rows");
    let drawn = shape_panel(&app, 80, 30);
    eprintln!("{drawn}");
    assert!(
        drawn
            .lines()
            .any(|line| line.contains("Rows") && line.contains(&["x"; 16].join(" "))),
        "{drawn}"
    );
    let field = panel(&app).field(GenParam::MoveList).unwrap();
    playing_at(&mut app, 6);
    assert_eq!(panel(&app).playing_token(&app.ctx(), &field), Some(1));
    playing_at(&mut app, 0);
    assert_eq!(panel(&app).playing_token(&app.ctx(), &field), None);
}

/// Kept live, on the whole Gen page, the move that reached the chord of the row playing is green
/// in the `Tonnetz`'s `List` cell, and none while the first row, the chord it starts on, plays
/// (decision 473).
#[test]
fn the_gen_page_draws_the_move_playing_in_the_playheads_colour() {
    let (mut app, _realtime) = on_moves();
    keys(&mut app, "<Esc>L");
    assert!(app.overlays.shape.is_none(), "kept live");
    keys(&mut app, " 8^wo");
    assert_eq!(app.modes.view, View::Gen);
    'found: for _ in 0..12 {
        keys(&mut app, "0");
        for _ in 0..6 {
            if here(&app) == Some(GenParam::MoveList) {
                break 'found;
            }
            keys(&mut app, "l");
        }
        keys(&mut app, "j");
    }
    assert_eq!(here(&app), Some(GenParam::MoveList));
    // Off the cell, where the cursor doesn't light it as one value.
    keys(&mut app, "k");
    let green = |app: &App| {
        let buffer = crate::common::track_page_buffer(app);
        let lines = crate::common::drawn_lines(&buffer);
        let (y, at) = lines
            .iter()
            .enumerate()
            .find_map(|(y, line)| line.find("P L R L").map(|at| (y, at)))
            .unwrap_or_else(|| panic!("the moves: {lines:#?}"));
        let start = lines[y][..at].chars().count() as u16;
        // The four moves, a cell apart, and which of them is green.
        (0..4).position(|nth| buffer[(start + 2 * nth, y as u16)].fg == theme::GREEN)
    };
    playing_at(&mut app, 7);
    assert_eq!(green(&app), Some(2), "row 07 is R's chord");
    playing_at(&mut app, 0);
    assert_eq!(green(&app), None, "the chord it starts on");
}
