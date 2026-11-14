//! The timeline: rows of different lengths laid out in time, a line for each pulse.

use std::sync::Arc;

use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;

mod common;
use common::*;

/// A first track whose rows last these pulses, each with a note, beside a second track of this
/// many rows at this speed, numbered by their notes from `C-3`: the shape of an acid line of 7
/// rows over 16 pulses beside drums of 16 rows.
fn timeline_song(pulses: &[u8], rows: usize, speed: vt_model::speed::Speed) -> (App, Realtime) {
    use vt_model::step::{Gate, GateKind};
    let mut song = Song::new(2);
    let steps = pulses
        .iter()
        .map(|pulses| Step {
            note: Note::On(48),
            gate: Some(Gate {
                kind: GateKind::Single,
                pulses: *pulses,
                length: None,
            }),
            ..Step::default()
        })
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let mut phrase = Phrase::new(rows);
    for row in 0..rows {
        phrase.step_mut(row).unwrap().note = Note::On(60 + row as u8);
    }
    song.tracks[1].phrases.insert(0, Arc::new(phrase));
    song.tracks[1].speed = speed;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// An acid line's rows of uneven lengths, 16 pulses in all.
const UNEVEN: [u8; 7] = [2, 1, 3, 4, 3, 1, 2];

/// The second track's note on a row, as its rows are numbered from `C-3`.
fn note_name(row: usize) -> String {
    Note::On(60 + row as u8).to_string()
}

#[test]
fn the_timeline_gives_a_row_a_line_for_each_pulse() {
    let (app, _realtime) = timeline_song(&UNEVEN, 16, vt_model::speed::Speed::Sixteenth);
    let buffer = screen(&app);
    // Each row's own line has its number, and its continuation lines none.
    let numbered: Vec<(u16, String)> = (0..16)
        .map(|line| (line, row_number_at(&buffer, line + 2)))
        .filter(|(_, number)| !number.trim().is_empty())
        .collect();
    let expected: Vec<(u16, String)> = [0, 2, 3, 6, 10, 13, 14]
        .iter()
        .enumerate()
        .map(|(row, line)| (*line, format!(" {row:02}")))
        .collect();
    assert_eq!(numbered, expected);
    // A continuation line is dim: a line under the note it holds, and nothing else.
    let text = |y: u16| -> String { (9..30).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(text(3).trim_end(), " │");
    assert_eq!(Some(buffer[(10, 3)].fg), vt_tui::theme::faint().fg);
    // The FX band carries on through it.
    assert_eq!(buffer[(28, 3)].bg, vt_tui::theme::FX_BAND);
    // Beats are lines that start on a beat. Row 02 lasts steps 3 to 5, so the rule before the
    // beat at step 4 is under its own line, and row 03's second line is before step 8's.
    let beat = Some(vt_tui::theme::SURFACE1);
    assert_eq!(rule_under(&buffer, 5), beat);
    assert_eq!(rule_under(&buffer, 9), beat);
    assert_eq!(rule_under(&buffer, 2), None);
    // Only row 00 starts on a beat: the others fall between, row 04 on step 10.
    assert_eq!(
        buffer[(5, 2)].fg,
        vt_tui::theme::YELLOW,
        "row 00 is on a beat"
    );
    assert_ne!(buffer[(5, 12)].fg, vt_tui::theme::YELLOW, "row 04 isn't");
}

#[test]
fn other_tracks_line_up_by_time_in_the_timeline() {
    let (app, _realtime) = timeline_song(&UNEVEN, 16, vt_model::speed::Speed::Sixteenth);
    let buffer = screen(&app);
    // The drums' sixteenths go down beside the acid's lines, one a line, rather than one a row.
    for line in 0..16 {
        assert_eq!(
            beside(&buffer, 0, line).0,
            format!(" {}", note_name(line as usize))
        );
    }
}

#[test]
fn a_slower_track_leaves_lines_blank_and_a_faster_one_marks_them() {
    let (app, _realtime) = timeline_song(&[1; 16], 8, vt_model::speed::Speed::Eighth);
    let buffer = screen(&app);
    for line in 0..16 {
        let expected = if line % 2 == 0 {
            format!(" {}", note_name(line as usize / 2))
        } else {
            "    ".to_owned()
        };
        assert_eq!(beside(&buffer, 0, line).0, expected, "line {line}");
    }
    // At a 32nd, two of its rows start in each sixteenth: the first shows, marked.
    let (app, _realtime) = timeline_song(&[1; 16], 32, vt_model::speed::Speed::ThirtySecond);
    let buffer = screen(&app);
    for line in 0..16 {
        let expected = format!(
            "{}{}",
            vt_tui::views::phrase::MORE_STARTS,
            note_name(line as usize * 2)
        );
        assert_eq!(beside(&buffer, 0, line).0, expected, "line {line}");
    }
}

#[test]
fn j_and_k_skip_continuation_lines_and_counts_count_rows() {
    let (mut app, _realtime) = timeline_song(&UNEVEN, 16, vt_model::speed::Speed::Sixteenth);
    press(&mut app, "j");
    assert_eq!(app.cursor.step, 1);
    press(&mut app, "2j");
    assert_eq!(app.cursor.step, 3, "a count counts rows");
    let buffer = screen(&app);
    // Relative numbers count rows too, and continuation lines have none.
    let gutter = |y: u16| -> String { (0..6).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(gutter(2), " 3  00");
    assert_eq!(gutter(3).trim(), "");
    assert_eq!(gutter(4), " 2  01");
    assert_eq!(gutter(5), " 1  02");
    assert_eq!(gutter(8), "    03");
    assert_eq!(
        Some(buffer[(9, 8)].bg),
        vt_tui::theme::cursor(app.mode).bg,
        "the cursor is on row 03's own line"
    );
    press(&mut app, "3k");
    assert_eq!(app.cursor.step, 0);
    // A selection covers rows, and their continuation lines with them.
    press(&mut app, "Vj");
    let buffer = screen(&app);
    assert_eq!(Some(buffer[(10, 3)].bg), vt_tui::theme::selected().bg);
}

#[test]
fn the_view_scrolls_by_the_lines_rows_take() {
    // Rows of 8 pulses, 128 lines, on a screen of 21.
    let (mut app, _realtime) = timeline_song(&[8; 16], 16, vt_model::speed::Speed::Sixteenth);
    app.follow_cursor(21);
    press(&mut app, "3j");
    app.follow_cursor(21);
    assert_eq!(
        app.top, 2,
        "rows 02 and 03 are the 16 lines that fit with row 03"
    );
    press(&mut app, "L");
    assert_eq!(
        app.cursor.step, 3,
        "L goes to the last row whose lines all fit"
    );
    press(&mut app, "H");
    assert_eq!(app.cursor.step, 2);
    press(&mut app, "G");
    app.follow_cursor(21);
    assert_eq!(app.top, 14, "the last two rows fill 16 of the lines");
    // Collapsed, a row a line again, so every row fits.
    press(&mut app, "zi");
    app.follow_cursor(21);
    assert_eq!(app.top, 0);
}

#[test]
fn the_playheads_follow_the_timeline() {
    let (mut app, _realtime) = timeline_song(&UNEVEN, 16, vt_model::speed::Speed::Sixteenth);
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    // Step 5 is row 02's third pulse, on line 5, and the drums' row 05.
    app.set_playhead(5);
    let buffer = screen(&app);
    assert_eq!(buffer[(7, 7)].symbol(), ">", "on row 02's last line");
    assert_eq!(buffer[(7, 5)].symbol(), " ", "not on row 02's own line");
    assert_eq!(beside(&buffer, 0, 5).0, format!(">{}", note_name(5)));
}

#[test]
fn z_i_collapses_the_timeline_to_a_line_a_row_and_opens_it_again() {
    let (mut app, _realtime) = timeline_song(&UNEVEN, 16, vt_model::speed::Speed::Sixteenth);
    assert!(app.timeline_on(), "the timeline is the default");
    press(&mut app, "zi");
    assert!(!app.timeline_on());
    assert!(app.status.contains("z i"), "{}", app.status);
    let buffer = screen(&app);
    for row in 0..7 {
        assert_eq!(row_number_at(&buffer, row + 2), format!(" {row:02}"));
    }
    // Collapsed, the other tracks' columns go by row number, as before the timeline.
    assert_eq!(beside(&buffer, 0, 3).0, format!(" {}", note_name(3)));
    press(&mut app, "zi");
    assert!(app.timeline_on());
    assert_eq!(row_number_at(&screen(&app), 5), " 02");
}

#[test]
fn a_view_with_nothing_to_lay_out_in_time_looks_as_it_did() {
    let (mut app, _realtime) = timeline_song(&[1; 16], 16, vt_model::speed::Speed::Sixteenth);
    let timeline = screen(&app);
    press(&mut app, "zi");
    assert_eq!(screen(&app), timeline);
}
