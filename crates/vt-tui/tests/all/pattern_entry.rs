//! Pattern entry (decision 474, #1127): `!` fills the cursor's column from a line in `Mini`'s
//! notation, counted from the phrase's row 00 and going round down the whole phrase, written
//! only on the rows a `v` selection holds; `Enter` writes plain rows as one undo step, `L` keeps
//! it live, `Esc` puts everything back; `x` on the Note column is the last note typed; `[s` and
//! `]s` halve and double its `Step`, remembered; `!!` opens on the last pattern and `.` drops it
//! on another column; and across a group's drum tracks it makes a kit line.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::FillKind;
use vt_model::song::{Phrase, Song};
use vt_model::step::{FxCommand, Note, Step};
use vt_tui::app::App;
use vt_tui::input::mode::Mode;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// So many tracks, each with an empty phrase of `rows` rows.
fn app_of(tracks: usize, rows: usize) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in &mut song.tracks {
        let steps: Vec<Step> = (0..rows).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    app_for(song)
}

/// The notes on a track's phrase, by name, `.` for none.
fn notes(app: &App, track: usize) -> Vec<String> {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            note @ Note::On(_) => note.to_string(),
            _ => ".".to_owned(),
        })
        .collect()
}

/// The same, as a line, for a short comparison.
fn line(app: &App, track: usize) -> String {
    notes(app, track).join(" ")
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the pattern's buffer")
}

/// `!` on the Note column with nothing selected: the buffer opens typing, at `Step 4`, rows
/// change as it's typed, the line goes round the whole phrase, and `Enter` writes plain rows as
/// one undo step that `u` takes off whole.
#[test]
fn a_pattern_fills_the_whole_phrase_from_row_00_as_one_undo_step() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "5j!");
    assert_eq!(app.modes.mode, Mode::Shape);
    let shown = panel(&app);
    assert!(shown.pattern.is_some());
    assert_eq!(shown.shape.kind, FillKind::Mini);
    assert_eq!(shown.shape.mini.step, 4, "four rows a token to start");
    assert!(
        shown
            .tokens
            .as_ref()
            .is_some_and(|t| t.field.inserting().is_some()),
        "typing"
    );
    let depth = app.services.history.undo_depth();
    keys(&mut app, "C-3 . D-3");
    // Heard as it's typed, counted from row 00 wherever the cursor was.
    assert_eq!(notes(&app, 0)[0], "C-3");
    assert_eq!(notes(&app, 0)[8], "D-3");
    assert_eq!(notes(&app, 0)[12], "C-3", "the line goes round");
    keys(&mut app, "<Enter>");
    assert!(app.overlays.shape.is_none());
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo step"
    );
    let phrase = &app.song().tracks[0].phrases[&0];
    assert!(phrase.generators().is_empty(), "plain rows");
    assert_eq!(phrase.steps().len(), 16, "the phrase's length stays");
    keys(&mut app, "u");
    assert!(notes(&app, 0).iter().all(|note| note == "."));
}

/// With a `v` selection only its rows are written, still counted from row 00, unlike `S`'s
/// line, which starts at its selection.
#[test]
fn a_selection_is_where_the_pattern_writes_counted_from_row_00() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "4jv3j!C-3 D-3<Enter>");
    // Rows 0 to 3 are C-3's place and 4 to 7 D-3's; only 4 to 7 are written.
    assert_eq!(line(&app, 0), ". . . . D-3 . . . . . . . . . . .");
}

/// A line longer than the phrase is cut at its end, the phrase's length and loop kept, and the
/// status line says how many rows didn't fit.
#[test]
fn a_pattern_longer_than_the_phrase_is_cut_and_said() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3 E-3 F-3 G-3");
    assert!(
        app.services
            .status
            .contains("4 rows didn't fit in the phrase's 16"),
        "{}",
        app.services.status
    );
    keys(&mut app, "<Enter>");
    assert!(
        app.services.status.contains("4 rows didn't fit"),
        "{}",
        app.services.status
    );
    let phrase = &app.song().tracks[0].phrases[&0];
    assert_eq!(phrase.steps().len(), 16);
    assert_eq!(notes(&app, 0)[0], "C-3", "not round again from G-3");
    assert_eq!(notes(&app, 0)[12], "F-3");
}

/// `x` on the Note column is the last note typed in Insert, and a line kept live holds the
/// note's name, never an `x`; on another column `x` isn't a value.
#[test]
fn x_is_the_last_note_typed() {
    let (mut app, _rt) = app_of(1, 16);
    // `d` in Insert is E-3.
    keys(&mut app, "id<Esc>u");
    assert_eq!(app.services.live.insert_note, Some(64));
    keys(&mut app, "!x . x C-3");
    assert_eq!(notes(&app, 0)[0], "E-3");
    assert_eq!(notes(&app, 0)[8], "E-3");
    assert_eq!(panel(&app).pattern.as_ref().unwrap().typed, "x . x C-3");
    keys(&mut app, "<Esc>L");
    let generator = &app.song().tracks[0].phrases[&0].generators()[0];
    assert_eq!(generator.rhythm(), "E-3 . E-3 C-3");
    assert_eq!(generator.shape.kind, FillKind::Mini);
    // On the Velocity column, `x` doesn't read.
    keys(&mut app, "l!x");
    assert!(
        app.services.status.contains("x isn't a hex value"),
        "{}",
        app.services.status
    );
}

/// A degree counts from the root in the octave of the last note typed, as `x` is that note.
#[test]
fn degrees_count_from_the_last_note_typeds_octave() {
    let (mut app, _rt) = app_of(1, 16);
    // `d` in Insert, an octave down with `z`, is E-2.
    keys(&mut app, "izd<Esc>u!1 x 8");
    assert_eq!(notes(&app, 0)[0], "C-2");
    assert_eq!(notes(&app, 0)[4], "E-2");
    assert_eq!(
        notes(&app, 0)[8],
        "G-2",
        "the 8th semitone up, with the scale off"
    );
}

/// `Esc` while typing stops typing, and `Esc` again puts everything back.
#[test]
fn esc_puts_everything_back() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3");
    assert_eq!(notes(&app, 0)[0], "C-3");
    keys(&mut app, "<Esc>");
    assert!(app.overlays.shape.is_some(), "still open, not typing");
    keys(&mut app, "<Esc>");
    assert!(app.overlays.shape.is_none());
    assert!(notes(&app, 0).iter().all(|note| note == "."));
    assert_eq!(app.services.history.undo_depth(), 0);
}

/// `L` keeps it live, a `Mini` generator over the rows selected, its line counted from row 00.
#[test]
fn l_keeps_a_pattern_live_counted_from_row_00() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "4jv3j!C-3 D-3<Esc>L");
    let phrase = &app.song().tracks[0].phrases[&0];
    let generator = &phrase.generators()[0];
    assert_eq!(generator.shape.kind, FillKind::Mini);
    assert_eq!(generator.shape.mini.skip, 4, "counted from row 00");
    assert_eq!(generator.rhythm(), "C-3 D-3");
    assert_eq!(line(&app, 0), ". . . . D-3 . . . . . . . . . . .");
}

/// `[s` and `]s` halve and double the `Step`, shown in the buffer's title, and the next `!`
/// starts from it.
#[test]
fn step_halves_and_doubles_and_is_remembered() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3<Esc>[s");
    assert_eq!(panel(&app).shape.mini.step, 2);
    assert_eq!(notes(&app, 0)[2], "D-3");
    let drawn = screen_cells(&mut app).join("\n");
    assert!(drawn.contains("Note pattern  Step 2 rows"), "{drawn}");
    keys(&mut app, "]s]s]s");
    assert_eq!(panel(&app).shape.mini.step, 16);
    keys(&mut app, "]s");
    assert_eq!(panel(&app).shape.mini.step, 16, "16 at most");
    keys(&mut app, "[s[s<Enter>");
    assert_eq!(app.services.live.pattern_step, 4);
    keys(&mut app, "!");
    assert_eq!(panel(&app).shape.mini.step, 4);
}

/// `!!` opens on the last pattern, and `.` drops it on another track's column, as kept.
#[test]
fn bang_bang_and_dot_bring_back_the_last_pattern() {
    let (mut app, _rt) = app_of(2, 16);
    keys(&mut app, "!C-3 . D-3<Enter>");
    keys(&mut app, "W");
    assert_eq!(app.services.cursor.track, 1);
    keys(&mut app, "!!");
    assert_eq!(panel(&app).pattern.as_ref().unwrap().typed, "C-3 . D-3");
    assert_eq!(notes(&app, 1)[8], "D-3", "heard at once");
    keys(&mut app, "<Esc><Esc>");
    assert!(notes(&app, 1).iter().all(|note| note == "."));
    let depth = app.services.history.undo_depth();
    keys(&mut app, ".");
    assert_eq!(line(&app, 1), line(&app, 0));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    assert!(app.overlays.shape.is_none());
}

/// An empty FX column lists its commands first, as `S` and `Mini` do, and the pattern writes
/// that command's values.
#[test]
fn an_empty_fx_column_asks_for_its_command_first() {
    let (mut app, _rt) = app_of(1, 16);
    // A note on every row, for the FX to go with.
    keys(&mut app, "!C-3<Enter>");
    keys(&mut app, "w!");
    assert!(app.overlays.picker.is_some(), "the commands listed");
    keys(&mut app, "CHA<Enter>");
    assert!(
        panel(&app).pattern.is_some(),
        "the pattern's buffer, once chosen"
    );
    keys(&mut app, "40 80<Enter>");
    let steps = app.song().tracks[0].phrases[&0].steps().to_vec();
    let fx = steps[4].fx[0];
    assert_eq!(fx.command, Some(FxCommand::Cha));
    assert_eq!(fx.value >> 8, 0x80);
}

/// Across a `W` selection of a group's drum tracks, a kit line, each track playing its name.
#[test]
fn across_a_group_a_pattern_is_a_kit_line() {
    let (mut app, _rt) = app_of(3, 16);
    let mut song = (**app.song()).clone();
    for (track, name) in song.tracks.iter_mut().zip(["BD", "SN", "HH"]) {
        track.name = name.to_owned();
        track.group = Some(1);
    }
    app.services.history.replace_current(Arc::new(song));
    keys(&mut app, "vGWW!");
    assert!(panel(&app).shape.kit.is_some(), "a kit line");
    keys(&mut app, "bd sn . sn<Enter>");
    assert_ne!(notes(&app, 0)[0], ".");
    assert_eq!(notes(&app, 0)[4], ".");
    assert_ne!(notes(&app, 1)[4], ".");
    assert_ne!(notes(&app, 1)[12], ".");
    assert!(notes(&app, 2).iter().all(|note| note == "."));
}

/// The buffer is drawn beside the column it fills, at 163x42, so the rows it writes stay in
/// sight, its rows line lighting the token under the cursor's rows.
#[test]
fn the_buffer_is_drawn_beside_its_column() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 . [D-3 x] <E-3 3>*2<Esc>");
    let lines = screen_cells(&mut app);
    let title = lines
        .iter()
        .position(|line| line.contains("Note pattern  Step 4 rows"))
        .expect("the buffer's title");
    // The first row's note is in the grid, and the buffer's text right of it.
    assert!(
        lines[title + 1].contains("│C-3│ C-3 . [D-3 x] <E-3 3>*2"),
        "{}",
        lines[title + 1]
    );
    assert!(lines[title + 3].contains("Rows  x . . . . . . . x . x . x . x ."));
    assert!(lines[title + 4].contains("Enter write  L keep live  [s ]s Step"));
}

/// A group's tracks without names make no kit line, as `S` refuses one (decision 472).
#[test]
fn a_kit_line_on_unnamed_tracks_is_refused() {
    let (mut app, _rt) = app_of(2, 16);
    let mut song = (**app.song()).clone();
    for track in &mut song.tracks {
        track.group = Some(1);
    }
    app.services.history.replace_current(Arc::new(song));
    keys(&mut app, "vGW!");
    assert!(app.overlays.shape.is_none());
    assert!(
        app.services.status.contains("no names"),
        "{}",
        app.services.status
    );
}

/// The buffer has the keys, so its frame is in the focus colour (decision 475).
#[test]
fn the_buffer_is_framed_in_the_focus_colour() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3");
    let lines = screen_cells(&mut app);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, std::time::Instant::now()))
        .unwrap();
    let screen = terminal.backend().buffer().clone();
    let y = lines
        .iter()
        .position(|line| line.contains("Note pattern  Step 4 rows"))
        .expect("the buffer's title");
    // Its corner, the first after the grid's own frame.
    let x = lines[y]
        .chars()
        .enumerate()
        .skip(10)
        .find(|(_, c)| *c == '╭')
        .map(|(x, _)| x)
        .expect("the buffer's corner");
    assert_eq!(
        screen[(x as u16, y as u16)].fg,
        vt_tui::widgets::theme::FOCUS
    );
}
