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
fn the_buffer_is_drawn_past_its_tracks_grid_over_the_other_tracks() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 . [D-3 x] <E-3 3>*2<Esc>");
    let lines = screen_cells(&mut app);
    let title = lines
        .iter()
        .position(|line| line.contains("Note pattern  Step 4 rows"))
        .expect("the buffer's title");
    // The first row is whole in the grid, its FX columns too, up to the grid's right edge, and
    // the buffer's text starts right of that edge.
    let row = &lines[title + 1];
    let grid_end = row
        .find("││")
        .expect("the grid's edge against the buffer's");
    assert!(row[..grid_end].contains("C-3 64"), "{row}");
    assert!(
        row[grid_end..].starts_with("││ C-3 . [D-3 x] <E-3 3>*2"),
        "{row}"
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

/// The pattern's buffer completes with Neovim's keys, as every buffer does, and `Tab` plays while
/// it's typed (decision 477).
#[test]
fn the_patterns_buffer_completes_with_ctrl_n_and_tab_plays() {
    use ratatui::crossterm::event::KeyCode;
    use vt_engine::engine::Transport;
    let (mut app, mut rt) = app_of(1, 16);
    keys(&mut app, "!^n");
    let menu = |app: &App| panel(app).tokens.as_ref().unwrap().menu.is_some();
    assert!(menu(&app), "Ctrl-n lists what can go here");
    keys(&mut app, "^e");
    assert!(!menu(&app));
    special(&mut app, KeyCode::Tab);
    block(&mut rt, &mut app);
    assert_eq!(app.services.reported.transport, Transport::Playing);
    assert!(!menu(&app), "Tab lists nothing");
    keys(&mut app, "C-3<Enter>");
    assert_eq!(notes(&app, 0)[0], "C-3");
}

/// Recipes from the patterns guide write what it says they do.
#[test]
fn the_patterns_guides_recipes_write_what_it_says() {
    let on = |rows: &[usize]| -> String {
        (0..16)
            .map(|row| if rows.contains(&row) { "C-3" } else { "." })
            .collect::<Vec<_>>()
            .join(" ")
    };
    // Four on the floor, at `Step 4 rows`.
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3!4<Enter>");
    assert_eq!(line(&app, 0), on(&[0, 4, 8, 12]));
    // The tresillo at `Step 1 row`, eight rows and round again.
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3(3,8)@8<Esc>[s[s<Enter>");
    assert_eq!(line(&app, 0), on(&[0, 3, 6, 8, 11, 14]));
    // `[. x]` at `Step 4 rows` is the offbeat, two rows in.
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "![. C-3]<Enter>");
    assert_eq!(line(&app, 0), on(&[2, 6, 10, 14]));
}

/// `Enter` writes the pattern after `Esc` has stopped the typing, as it does while typing, so
/// `[s` and `]s` can set the `Step` before it's written.
#[test]
fn enter_after_esc_writes_the_pattern_too() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3<Esc><Enter>");
    assert_eq!(app.modes.mode, Mode::Normal);
    assert_eq!(notes(&app, 0)[0], "C-3");
    assert_eq!(notes(&app, 0)[4], "D-3");
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3<Esc>[s<Enter>");
    assert_eq!(notes(&app, 0)[2], "D-3", "at the Step it was set to");
}

/// `u` in the pattern's buffer takes back its last change, a stretch of typing as one, back to
/// the empty line it opened on, and `C-r` puts it back, the rows following the line each time.
#[test]
fn u_and_ctrl_r_undo_and_redo_in_the_line() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 D-3<Esc>");
    assert_eq!(line_text(&app), "C-3 D-3");
    keys(&mut app, "x");
    assert_eq!(line_text(&app), "C-3");
    keys(&mut app, "u");
    assert_eq!(line_text(&app), "C-3 D-3", "the x taken back");
    assert_eq!(notes(&app, 0)[4], "D-3", "and heard");
    keys(&mut app, "u");
    assert_eq!(line_text(&app), "", "the typing taken back, whole");
    keys(&mut app, "u");
    assert!(
        app.services.status.contains("Nothing to undo"),
        "{}",
        app.services.status
    );
    keys(&mut app, "^r^r");
    assert_eq!(line_text(&app), "C-3");
    assert!(app.overlays.shape.is_some(), "still open");
    keys(&mut app, "<Enter>");
    assert_eq!(notes(&app, 0)[0], "C-3");
}

/// The pattern's line as its buffer holds it.
fn line_text(app: &App) -> String {
    panel(app)
        .tokens
        .as_ref()
        .map(|tokens| tokens.field.text().to_owned())
        .unwrap_or_default()
}

/// The `CHA` a row of a track's phrase holds, if it has one.
fn chance_on(app: &App, row: usize) -> Option<u8> {
    app.song().tracks[0].phrases[&0].steps()[row]
        .fx
        .iter()
        .find(|slot| slot.command == Some(FxCommand::Cha))
        .map(|slot| (slot.value >> 8) as u8)
}

/// On the Note column a `?` writes its row's `CHA` rather than drawing there, so the track's
/// `CDLT` and `Thin` reach it; elsewhere, and where a hit is nudged off its row, it draws as it
/// always has (decision 480).
#[test]
fn a_question_mark_on_the_note_column_is_its_rows_chance() {
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3 C-3?30 C-3?70 C-3<Enter>");
    assert_eq!(
        line(&app, 0),
        "C-3 . . . C-3 . . . C-3 . . . C-3 . . .",
        "every hit lands"
    );
    assert_eq!(chance_on(&app, 0), None);
    assert_eq!(chance_on(&app, 4), Some(0x4D), "30%");
    assert_eq!(chance_on(&app, 8), Some(0xB3), "70%");
    assert_eq!(chance_on(&app, 12), None);
    // On Velocity there's no note to give a chance to: the draw decides, and no `CHA` is written.
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "!C-3<Esc>[s[s<Enter>l!7F?50<Esc>[s[s<Enter>");
    assert!((0..16).all(|row| chance_on(&app, row).is_none()));
    // A triplet's middle hit is nudged off its row and keeps its chance there too.
    let (mut app, _rt) = app_of(1, 16);
    keys(&mut app, "![C-3 D-3? E-3]<Enter>");
    let steps = app.song().tracks[0].phrases[&0].steps().to_vec();
    assert_eq!(steps[1].nudge, 8, "nudged");
    assert_eq!(chance_on(&app, 1), Some(0x80), "and chanced, 50%");
    // A row with every FX slot taken can't hold one: the draw decides it, and the status says so.
    let mut song = Song::new(1);
    let full = "--- -- ret:02 cut:40 del:03".parse::<Step>().unwrap();
    let steps: Vec<Step> = (0..16)
        .map(|row| {
            if row == 4 {
                full.clone()
            } else {
                E.parse().unwrap()
            }
        })
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, _rt) = app_for(song);
    keys(&mut app, "!C-3 C-3?30");
    assert!(
        app.services.status.contains("Row 04 has no FX slot free"),
        "{}",
        app.services.status
    );
}

/// `[a|b|c]` picks one of its choices at random each time round, a choice being a whole run, as
/// Tidal's `|` (decision 481).
#[test]
fn a_bar_picks_one_choice_at_random_each_time_round() {
    // On Velocity over 32 rows, a token a row: each row one of three velocities.
    let (mut app, _rt) = app_of(1, 32);
    keys(&mut app, "!C-3<Esc>[s[s<Enter>l![7F|40|20]<Esc>[s[s<Enter>");
    let velocities: std::collections::BTreeSet<u8> = app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .filter_map(|step| step.velocity)
        .collect();
    assert!(
        velocities.iter().all(|v| [0x7F, 0x40, 0x20].contains(v)),
        "{velocities:?}"
    );
    assert!(velocities.len() > 1, "more than one picked: {velocities:?}");
    // On Note, a choice is a run: each beat is two notes or three, never a mix.
    let (mut app, _rt) = app_of(1, 32);
    keys(&mut app, "![C-3 C-3|C-3 C-3 C-3]<Enter>");
    let notes = notes(&app, 0);
    for beat in notes.chunks(4) {
        let count = beat.iter().filter(|note| *note == "C-3").count();
        assert!(count == 2 || count == 3, "{beat:?}");
    }
    // A `|` outside a `[ ]`, or with nothing beside it, doesn't read, and says why.
    let why = |typed: &str| {
        let (mut app, _rt) = app_of(1, 16);
        keys(&mut app, typed);
        panel(&app)
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.field.bad().map(|bad| bad.why.clone()))
            .unwrap_or_default()
    };
    assert!(why("!C-3|D-3").contains("between the choices of a [ ]"));
    assert!(why("![C-3|").contains("has nothing in it"));
}

/// A kit line's buffer goes at the foot, so the overview's columns of the group's tracks stay in
/// sight, and each lights the rows the token under the cursor writes on its track (decision 482).
#[test]
fn a_kit_lines_buffer_leaves_the_overview_in_sight_and_lights_its_rows() {
    let (mut app, _rt) = app_of(3, 16);
    let mut song = (**app.song()).clone();
    for (track, name) in song.tracks.iter_mut().zip(["BD", "SN", "HH"]) {
        track.name = name.to_owned();
        track.group = Some(1);
    }
    app.services.history.replace_current(Arc::new(song));
    // Back to `sn` from the end, so it's the token under the cursor.
    keys(&mut app, "vGWW!bd . sn .<Esc>h");
    let lines = screen_cells(&mut app);
    let title = lines
        .iter()
        .position(|line| line.contains("Note kit line"))
        .expect("the buffer");
    assert!(title > 30, "at the foot, line {title}");
    // The overview's heading and SN's note on row 08, in sight.
    let heading = lines
        .iter()
        .position(|line| line.contains("BD   SN   HH"))
        .unwrap();
    let row_08 = heading + 9;
    // By characters, as the frame's are wider than a byte.
    let sn = lines[heading][..lines[heading].find("SN").unwrap()]
        .chars()
        .count();
    let cell = |line: usize| lines[line].chars().skip(sn).take(3).collect::<String>();
    assert_eq!(cell(row_08), "C-3", "{}", lines[row_08]);
    // Lit in the cursor's colour, where `bd`'s row on BD isn't.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(163, 42)).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, std::time::Instant::now()))
        .unwrap();
    let screen = terminal.backend().buffer().clone();
    let x = sn as u16;
    let rosewater = vt_tui::widgets::theme::ROSEWATER;
    assert_eq!(screen[(x, row_08 as u16)].fg, rosewater);
    let bd = lines[heading].find("BD").unwrap();
    let bx = lines[heading][..bd].chars().count() as u16;
    assert_ne!(screen[(bx, (heading + 1) as u16)].fg, rosewater);
}
