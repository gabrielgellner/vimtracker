//! What the phrase view draws: its rows and the playhead, the overview of the other tracks, its
//! cells' colours, its title, the rows inspector, the shape panel, the rules under beats and bars,
//! and the status line's names for the column.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Chain, ChainEntry, Machine, Phrase, Song, SynthVoice};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn a_selection_keeps_its_text_readable_and_shows_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, "Vj");
    let buffer = screen(&app);
    // Past the step number and playhead gutter, on the row the selection started from.
    let selected = &buffer[(10, 2)];
    assert_eq!(selected.bg, vt_tui::theme::SURFACE1);
    assert_eq!(selected.fg, vt_tui::theme::TEXT);
    assert_ne!(selected.symbol(), " ");
    let cursor = &buffer[(9, 3)];
    assert_eq!(cursor.bg, vt_tui::theme::ROSEWATER);
    assert_eq!(cursor.fg, vt_tui::theme::BASE);
}

#[test]
fn each_row_shows_how_far_it_is_from_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, "5j");
    let buffer = screen(&app);
    // The gutter, up to the grid's frame.
    let row = |step: u16| -> String { (0..8).map(|x| buffer[(x, step + 2)].symbol()).collect() };
    assert_eq!(row(0), " 5  00  ");
    assert_eq!(row(4), " 1  04  ");
    assert_eq!(row(5), "    05  ", "the cursor's row is blank");
    assert_eq!(row(6), " 1  06  ");
    // Decimal, to match the count typed, beside a step number in hex.
    assert_eq!(row(15), "10  0F  ");
    assert_eq!(buffer[(1, 2)].fg, vt_tui::theme::OVERLAY0);
    assert_eq!(buffer[(4, 2)].fg, vt_tui::theme::YELLOW, "the beat marker");
    assert_eq!(buffer[(4, 3)].fg, vt_tui::theme::SUBTEXT1, "other steps");
}

#[test]
fn the_overview_shows_every_track_and_marks_the_one_being_edited() {
    let (mut app, _realtime) = app();
    press(&mut app, "W");
    let buffer = screen(&app);
    let labels: String = (68..100).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(
        labels.trim_end(),
        " 01   02",
        "the returns and master are left out"
    );
    assert_eq!(
        buffer[(74, 1)].bg,
        vt_tui::theme::GREEN,
        "track 02 is marked"
    );
    assert_ne!(buffer[(69, 1)].bg, vt_tui::theme::GREEN);
    let headings: String = (0..30).map(|x| buffer[(x, 1)].symbol()).collect();
    assert_eq!(headings.trim_end(), "        ╭Note ───────────── FX");
    assert_eq!(
        buffer[(28, 2)].bg,
        vt_tui::theme::FX_BAND,
        "the FX sit on a band"
    );
    assert_ne!(buffer[(9, 2)].bg, vt_tui::theme::FX_BAND);
}

#[test]
fn other_tracks_light_the_step_they_are_playing() {
    let song = Song::new(3);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    // The first track's column in the overview, its marker's cell and then its note's, on a step
    // two rows below the title.
    let color = |app: &App, x: u16, row: u16| screen(app)[(x, row + 2)].fg;
    let (marker, note) = (68, 69);
    let dimmed_green = vt_tui::theme::dimmed(vt_tui::theme::GREEN);
    on_the_row_being_edited(&mut app);
    app.set_playhead(2);
    // Its rows are empty, so faint, as an empty cell on the grid is.
    assert_eq!(color(&app, note, 2), vt_tui::theme::OVERLAY0, "stopped");
    app.transport = Transport::Playing;
    assert_eq!(color(&app, marker, 2), vt_tui::theme::GREEN);
    assert_eq!(
        color(&app, note, 2),
        dimmed_green,
        "an empty row playing is lit, set back"
    );
    assert_eq!(color(&app, note, 3), vt_tui::theme::OVERLAY0);
    app.transport = Transport::Paused;
    assert_eq!(color(&app, marker, 2), vt_tui::theme::GREEN);
}

#[test]
fn insert_turns_the_cursor_and_the_grids_frame_green_without_moving_anything() {
    let (mut app, _realtime) = app();
    let row = |buffer: &ratatui::buffer::Buffer, y: u16| -> String {
        (0..40).map(|x| buffer[(x, y)].symbol()).collect()
    };

    // The grid is framed as a device is, dim, with its parts named in the top edge.
    let normal = screen(&app);
    assert_eq!(normal[(9, 2)].bg, vt_tui::theme::ROSEWATER);
    assert_eq!(normal[(8, 1)].symbol(), "╭");
    assert_eq!(normal[(8, 2)].symbol(), "│");
    assert_eq!(normal[(8, 18)].symbol(), "╰");
    assert_eq!(normal[(8, 2)].fg, vt_tui::theme::SURFACE1);
    assert_eq!(normal[(15, 1)].fg, vt_tui::theme::SURFACE1, "the edge");
    let headings: String = (8..67).map(|x| normal[(x, 1)].symbol()).collect();
    assert_eq!(
        headings,
        "╭Note ───────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮"
    );
    assert_eq!(normal[(9, 1)].fg, vt_tui::theme::OVERLAY2, "a heading");

    press(&mut app, "i");
    let insert = screen(&app);
    assert_eq!(insert[(9, 2)].bg, vt_tui::theme::GREEN);
    assert_eq!(insert[(9, 2)].fg, vt_tui::theme::BASE);
    assert_eq!(insert[(8, 1)].symbol(), "╭");
    assert_eq!(insert[(8, 2)].symbol(), "│");
    assert_eq!(insert[(8, 18)].symbol(), "╰");
    assert_eq!(insert[(8, 2)].fg, vt_tui::theme::GREEN);
    // Each heading is set into the top edge over its column, a space either side.
    let edge: String = (8..67).map(|x| insert[(x, 1)].symbol()).collect();
    assert_eq!(
        edge,
        "╭Note ───────────── FX 1 ─────── FX 2 ─────── FX 3 ───────╮"
    );
    assert_eq!(insert[(66, 18)].symbol(), "╯");

    // Only the frame's colour changes, and the steps are where they were.
    assert_eq!(row(&normal, 2), row(&insert, 2));
    let step_text_start = |b: &ratatui::buffer::Buffer| b[(12, 2)].symbol().to_owned();
    assert_eq!(step_text_start(&normal), step_text_start(&insert));

    special(&mut app, KeyCode::Esc);
    let back = screen(&app);
    assert_eq!(back[(8, 1)].fg, vt_tui::theme::SURFACE1);
    assert_eq!(back[(9, 2)].bg, vt_tui::theme::ROSEWATER);
}

#[test]
fn replace_shows_its_mode_and_a_red_cursor_with_the_frame_dim() {
    let (mut app, _realtime) = app();
    press(&mut app, "r");
    assert_eq!(app.mode, Mode::Replace);
    let buffer = screen(&app);
    assert_eq!(buffer[(9, 2)].bg, vt_tui::theme::RED);
    assert_eq!(
        buffer[(8, 2)].fg,
        vt_tui::theme::SURFACE1,
        "not lit, as in Insert"
    );
    press(&mut app, "a");
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(screen(&app)[(9, 2)].bg, vt_tui::theme::ROSEWATER);
}

#[test]
fn the_row_playing_lights_what_is_written_and_sets_back_what_is_empty() {
    let mut song = Song::new(1);
    let steps: Vec<Step> = ["C-3 40 --- --- ---", "--- -- --- --- ---"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    let (green, set_back) = (
        vt_tui::theme::GREEN,
        vt_tui::theme::dimmed(vt_tui::theme::GREEN),
    );
    // Row 00: its note and velocity lit, its empty gate and FX set back.
    app.set_playhead(0);
    let buffer = screen(&app);
    assert_eq!(buffer[(7, 2)].fg, green, "the marker");
    assert_eq!(buffer[(13, 2)].fg, green, "the velocity");
    assert_eq!(buffer[(20, 2)].fg, set_back, "the empty gate");
    assert_eq!(buffer[(28, 2)].fg, set_back, "the empty FX slot");
    // Row 01, empty, is set back throughout, and row 00 goes back to how it was.
    app.set_playhead(1);
    let buffer = screen(&app);
    assert_eq!(buffer[(7, 3)].fg, green, "the marker");
    for x in [9, 13, 20, 28] {
        assert_eq!(buffer[(x, 3)].fg, set_back, "cell {x}");
    }
    assert_eq!(buffer[(20, 2)].fg, vt_tui::theme::OVERLAY0);
}

#[test]
fn rows_count_from_the_loop_start_and_dim_outside_it() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]l");
    // A slide keeps row 00 where it was on screen, so scroll up to see what is above it.
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(row_number_at(&buffer, 2), "-02");
    assert_eq!(row_number_at(&buffer, 3), "-01");
    assert_eq!(row_number_at(&buffer, 4), " 00");
    assert_eq!(row_number_at(&buffer, 7), " 03");
    assert_eq!(row_number_at(&buffer, 8), " 04");
    assert_eq!(buffer[(5, 4)].fg, vt_tui::theme::YELLOW, "the loop start");
    assert_eq!(buffer[(5, 5)].fg, vt_tui::theme::SUBTEXT1);
    assert_eq!(buffer[(5, 2)].fg, vt_tui::theme::OVERLAY0, "not played");
    assert_eq!(
        buffer[(5, 8)].fg,
        vt_tui::theme::OVERLAY2,
        "a beat, not played"
    );
    assert_eq!(
        buffer[(9, 8)].fg,
        vt_tui::theme::OVERLAY0,
        "the step's text"
    );
    assert_eq!(
        buffer[(9, 6)].fg,
        vt_tui::theme::OVERLAY0,
        "an empty cell is faint where it plays too"
    );
}

#[test]
fn other_tracks_are_blank_above_row_00_and_keep_going_past_this_loop() {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    command(&mut app, "len 4");
    press(&mut app, "]L");
    app.top = 0;
    let buffer = screen(&app);
    let neighbour = |y: u16| -> String { (74..77).map(|x| buffer[(x, y)].symbol()).collect() };
    assert_eq!(neighbour(2), "   ", "above the loop");
    assert_eq!(neighbour(6), "---", "row 00 has the other track's row 00");
    assert_eq!(
        neighbour(10),
        "---",
        "the other track is longer than this loop"
    );
}

#[test]
fn the_playhead_marks_where_the_loop_is_playing() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]L");
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(1);
    app.top = 0;
    let buffer = screen(&app);
    // The loop starts at stored step 8, so row 01 is on the screen's row 11.
    assert_eq!(row_number_at(&buffer, 11), " 01");
    assert_eq!(buffer[(7, 11)].symbol(), ">");
}

#[test]
fn the_overview_shows_empty_notes_faint_and_written_ones_dim() {
    let mut song = Song::new(2);
    let steps: Vec<Step> = ["C-3 40 --- --- ---", "--- -- --- --- ---"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    assert_eq!(
        beside(&buffer, 0, 0),
        (" C-3".to_owned(), vt_tui::theme::OVERLAY2)
    );
    assert_eq!(
        beside(&buffer, 0, 1),
        (" ---".to_owned(), vt_tui::theme::OVERLAY0)
    );
}

#[test]
fn empty_cells_are_faint_dots_and_an_empty_slot_has_no_value() {
    let (app, _realtime) = app();
    let buffer = screen(&app);
    // Row 01 of a new phrase, past the cursor's.
    let row: String = (9..66).map(|x| buffer[(x, 3)].symbol()).collect();
    assert_eq!(
        row,
        "--- ··     ···· ·· ···   ··     ···   ··     ···   ··    "
    );
    assert_eq!(buffer[(13, 3)].fg, vt_tui::theme::OVERLAY0);
    assert_eq!(buffer[(34, 3)].fg, vt_tui::theme::OVERLAY0);
}

#[test]
fn the_phrase_title_is_in_groups_and_the_track_labels_head_their_columns() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let buffer = screen(&app);
    let lines = drawn_lines(&buffer);
    assert!(lines[0].contains("Track 01  Chain 00  00"), "{}", lines[0]);
    assert!(!lines[0].contains("01  02"), "the labels left the title");
    // Each label over its column's note, past the marker's cell.
    let headings: String = lines[1].chars().skip(68).collect();
    assert!(headings.starts_with(" 01   02   03 "), "{headings}");
    assert_eq!(buffer[(69, 1)].bg, vt_tui::theme::GREEN);
    // In Insert they sit beside the box's top edge, in the same columns.
    app.key(Key::char('i'), Instant::now());
    let insert = drawn_lines(&screen(&app))[1].clone();
    let after: String = insert.chars().skip(66).collect();
    assert!(after.starts_with("╮  01   02   03 "), "{after}");
}

#[test]
fn a_shared_phrase_and_chain_are_linked_rather_than_starred() {
    use vt_tui::glyphs::{ICON_SPACE, LINK};
    let (mut app, _realtime) = app();
    // A second row of the chain plays phrase 00 again.
    press(&mut app, " 2yyp");
    let chain = drawn_lines(&drawn(&app, vt_tui::views::chain::draw));
    assert!(
        chain[2].contains(&format!("00{LINK}{ICON_SPACE}")),
        "{}",
        chain[2]
    );
    assert!(!chain.iter().any(|line| line.contains('*')), "{chain:?}");
    press(&mut app, " 3");
    let title = drawn_lines(&screen(&app))[0].clone();
    assert!(
        title.contains(&format!("Chain 00  00{LINK}{ICON_SPACE}00{LINK}")),
        "{title}"
    );
}

#[test]
fn the_title_shows_the_chain_with_this_phrase_in_the_cursors_colour_cut_to_its_room() {
    // A chain of 40 phrases, more than the title has room for.
    let mut song = Song::new(1);
    for id in 1..40 {
        song.tracks[0].phrases.insert(id, Arc::new(Phrase::new(4)));
    }
    let entries = (0..40)
        .map(|phrase| ChainEntry {
            phrase,
            transpose: 0,
        })
        .collect();
    let chain = song.sections[0].chains[0].unwrap();
    song.tracks[0].chains.insert(
        chain,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 1);
    let buffer = screen_wide(&app);
    let title = drawn_lines(&buffer)[0].clone();
    assert!(title.contains("Chain 00  00 01 02 03"), "{title}");
    assert!(
        title.trim_end().ends_with('\u{2026}'),
        "cut, with a sign: {title}"
    );
    assert!(
        !title.trim_end().contains("\u{2026} "),
        "not cut before the start: {title}"
    );
    let at = title.find("Chain 00  ").unwrap();
    let first = (title[..at].chars().count() + "Chain 00  ".len()) as u16;
    assert_eq!(buffer[(first, 0)].fg, vt_tui::theme::OVERLAY2);
    assert_eq!(
        buffer[(first + 3, 0)].fg,
        vt_tui::theme::ROSEWATER,
        "{title}"
    );
    // `]p` moves along it.
    press(&mut app, "]p");
    assert_eq!(app.cursor.slot, 2);
    let buffer = screen_wide(&app);
    assert_eq!(buffer[(first + 3, 0)].fg, vt_tui::theme::OVERLAY2);
    assert_eq!(buffer[(first + 6, 0)].fg, vt_tui::theme::ROSEWATER);
    // At the end, it's cut before the start instead, and the last phrase is still shown.
    for _ in 0..40 {
        press(&mut app, "]p");
    }
    let title = drawn_lines(&screen_wide(&app))[0].clone();
    assert!(title.contains("Chain 00  \u{2026} "), "{title}");
    assert!(title.trim_end().ends_with("27"), "{title}");
}

/// A song of some tracks whose first plays a row with a note, a gate and a cutoff held.
fn inspected(tracks: usize) -> App {
    let mut song = Song::new(tracks);
    let steps: Vec<Step> = ["G-2 7F HLD1 cut:20h --- ---", "--- -- --- --- ---"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    App::new(song, None, samples, handle, 48_000)
}

#[test]
fn space_i_shows_the_rows_inspector_beside_the_overview_and_hides_it() {
    let mut app = inspected(8);
    let text = |app: &App| drawn_lines(&screen_wide(app)).join("\n");
    assert!(!text(&app).contains("Row 00"), "hidden at first");
    press(&mut app, " i");
    assert!(app.inspector);
    let lines = drawn_lines(&screen_wide(&app));
    // Its frame level with the grid's, past the eight tracks' columns.
    let top = &lines[1];
    let at = top.find("Row 00").expect("its title");
    assert!(top[..at].contains("08"), "{top}");
    let shown = lines.join("\n");
    for part in [
        "Note     G-2      MIDI 55",
        "Velocity 7F       \u{2595}",
        "Gate     HLD1     tied on, 1 pulse",
        "Length   \u{b7}\u{b7}       100 %, the track's",
        "FX 1     Cut      48 Hz  hold",
        "FX 2     \u{b7}\u{b7}\u{b7}      empty",
    ] {
        assert!(shown.contains(part), "no {part}:\n{shown}");
    }
    // It follows the cursor's row.
    press(&mut app, "j");
    let shown = text(&app);
    assert!(shown.contains("Row 01"), "{shown}");
    assert!(shown.contains("Note     ---      nothing plays"), "{shown}");
    press(&mut app, " i");
    assert!(!text(&app).contains("Row 01"));
}

#[test]
fn the_rows_inspector_gives_way_when_the_overview_needs_the_room() {
    let mut app = inspected(16);
    press(&mut app, " i");
    let lines = drawn_lines(&screen_wide(&app));
    assert!(!lines.join("\n").contains("Row 00"), "it gives way");
    // And every track keeps its column.
    assert!(lines[1].contains("16"), "{}", lines[1]);
    assert!(app.inspector, "it comes back when there's room");
}

/// The phrase view at the author's 163 columns, less the meter beside it.
fn screen_wide(app: &App) -> ratatui::buffer::Buffer {
    let backend = ratatui::backend::TestBackend::new(148, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), app))
        .unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn other_tracks_show_their_own_length_and_playhead() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let (dim, green) = (vt_tui::theme::OVERLAY2, vt_tui::theme::GREEN);
    let note = |text: &str, color| (text.to_owned(), color);

    // Stopped, nothing is marked. The 2-step track ends early, and the 8-step one has rows past
    // the selected track's 4.
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 0, 1), note(" C#3", dim));
    assert_eq!(beside(&buffer, 0, 2).0, "    ");
    assert_eq!(beside(&buffer, 1, 3), note(" D#3", dim));
    assert_eq!(beside(&buffer, 1, 7), note(" G-3", dim));

    on_the_row_being_edited(&mut app);
    // Each track marks where it is in its own loop.
    app.transport = Transport::Playing;
    app.set_playhead(5);
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 0, 1), note(">C#3", green));
    assert_eq!(beside(&buffer, 0, 3).0, "    ");
    assert_eq!(beside(&buffer, 1, 5), note(">F-3", green));
    assert_eq!(beside(&buffer, 1, 1), note(" C#3", dim));

    app.transport = Transport::Paused;
    assert_eq!(beside(&screen(&app), 1, 5), note(">F-3", green));
}

#[test]
fn a_column_shows_the_phrase_its_chain_is_on() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(4)));
    for (id, midi) in [(1, 60), (2, 64)] {
        let mut phrase = Phrase::new(2);
        *phrase.step_mut(0).unwrap() = Step {
            note: Note::On(midi),
            ..Step::default()
        };
        song.tracks[1].phrases.insert(id, Arc::new(phrase));
    }
    let entry = |phrase| ChainEntry {
        phrase,
        transpose: 0,
    };
    let entries = vec![entry(1), entry(2)];
    song.tracks[1].chains.insert(
        1,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    song.sections[0].chains[1] = Some(1);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    assert_eq!(beside(&screen(&app), 0, 0).0, " C-3");

    // Stopped, it follows the cursor, as that's where playing would start.
    app.cursor.step = 2;
    assert_eq!(beside(&screen(&app), 0, 0).0, " E-3");
    app.cursor.step = 0;

    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(2);
    assert_eq!(beside(&screen(&app), 0, 0).0, ">E-3");
}

#[test]
fn rows_past_a_short_phrase_keep_the_box_and_have_nothing_above_the_loop() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let time = Instant::now();
    app.key(Key::char('i'), time);
    let buffer = screen(&app);
    // Row 04 is past the selected track's end: it closes the box beside the longer track.
    let text = |row: u16| -> String {
        (0..66)
            .map(|x| buffer[(x, row + 2)].symbol())
            .collect::<String>()
    };
    assert!(text(3).contains('│'), "{}", text(3));
    assert!(text(4).contains('╰'), "{}", text(4));
    assert!(text(5).contains(" 05"), "{}", text(5));
    assert_eq!(beside(&buffer, 1, 4).0, " E-3");
    assert_eq!(beside(&buffer, 1, 5).0, " F-3");
    app.key(Key::new(KeyCode::Esc), time);

    // A row above the selected track's loop start is above every other track's too.
    press(&mut app, "]l");
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(beside(&buffer, 1, 0).0, "    ");
    assert_eq!(beside(&buffer, 1, 1).0, " C-3");
}

#[test]
fn the_shape_panel_draws_its_settings_beside_the_grid() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, "lvGS");
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), &app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: Vec<String> = (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let all = text.join("\n");
    for expected in [
        "Reshape",
        "Velocity: 4 of 4 rows",
        "From",
        "Curve",
        "Ramp",
        "Notes",
        "Enter keep",
    ] {
        assert!(all.contains(expected), "{expected} in {all}");
    }
    // The grid's own columns are left showing.
    assert!(
        text.iter()
            .all(|line| line.chars().take(38).all(|c| c == ' '))
    );
}

#[test]
fn a_list_fill_draws_its_values_in_place_of_the_ends() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    press(&mut app, "lvGSK/list");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "ji704055");
    special(&mut app, KeyCode::Enter);
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), &app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let all: String = (0..16)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in ["List", "Values  70 40 55", "Run", "~ reverse the list"] {
        assert!(all.contains(expected), "{expected} in {all}");
    }
    assert!(!all.contains("From"), "{all}");
}

#[test]
fn the_status_line_names_the_column() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    assert_eq!(column_name(&app), "Note");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Velocity");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Nudge");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gate");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Gate length, the track's");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 command");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "FX1 value");
    press(&mut app, " 2");
    assert_eq!(column_name(&app), "Phrase");
}

/// A sample track with a cutoff lock, a lock for String's Decay, which it doesn't play, and an
/// event.
fn app_with_a_lock_for_another_voice() -> (App, Realtime) {
    let mut song = Song::new(1);
    let decay = SettingId::voice(SynthVoice::String, 2).unwrap();
    assert_eq!(decay.name(), "Decay");
    let text = format!("C-3 40 cut:40 {}:20 del:10", decay.key());
    let phrase = Phrase::from_steps(vec![text.parse::<Step>().unwrap()]);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

#[test]
fn the_status_line_names_the_setting_a_cell_locks() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app_with_a_lock_for_another_voice();
    press(&mut app, "lllll");
    assert_eq!(column_name(&app), "Filter: Cut");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Filter: Cut", "on its value too");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "Filter: Cut", "and its fine digits");
    press(&mut app, "l");
    assert_eq!(column_name(&app), "String: Decay, not on this track");
    press(&mut app, "lll");
    assert_eq!(column_name(&app), vt_model::step::FxCommand::Del.meaning());
}

#[test]
fn the_status_line_shows_a_value_in_its_units() {
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = app_with_a_lock_for_another_voice();
    press(&mut app, "l");
    assert_eq!(value_shown(&app), None, "a velocity has no units");
    press(&mut app, "l");
    assert_eq!(value_shown(&app).as_deref(), Some("on the grid"), "a nudge");
    press(&mut app, "l");
    assert_eq!(value_shown(&app), None, "a gate has no units");
    press(&mut app, "l");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("100 %"),
        "the track's Gate"
    );
    press(&mut app, "l");
    // `cut:40` is a quarter of the way up ten octaves from 20 Hz.
    assert_eq!(value_shown(&app).as_deref(), Some("113 Hz"));
    press(&mut app, "lll");
    assert_eq!(value_shown(&app), None, "not on this track");
    press(&mut app, "lll");
    assert_eq!(value_shown(&app), None, "an event has no units");
    press(&mut app, " 5");
    let mut rows = Vec::new();
    for _ in 0..12 {
        rows.extend(value_shown(&app));
        press(&mut app, "j");
    }
    assert!(
        rows.contains(&"20.0 kHz".to_owned()),
        "the open cutoff: {rows:?}"
    );
}

#[test]
fn each_kind_of_cell_has_its_own_colours() {
    use ratatui::style::Modifier;
    use vt_tui::theme;
    let (app, _realtime) = app_with_a_lock_for_another_voice();
    let buffer = screen(&app);
    let row: String = (0..80).map(|x| buffer[(x, 2)].symbol()).collect();
    // Counted in cells, as a value's lock mark is a glyph of more than one byte.
    let style_at = |text: &str, past: usize| {
        let at = row.find(text).unwrap_or_else(|| panic!("no {text}: {row}"));
        let at = row[..at].chars().count();
        buffer[((at + past) as u16, 2)].style()
    };
    // A lock: its coarse digits bold in the value colour, and its fine ones set back.
    let coarse = style_at("CUT   4000", 6);
    assert_eq!(coarse.fg, Some(theme::MAUVE), "{row}");
    assert!(coarse.add_modifier.contains(Modifier::BOLD));
    assert_eq!(
        style_at("CUT   4000", 8).fg,
        Some(theme::fine(theme::MAUVE))
    );
    // An event's value is in its own colour, with no fine digits.
    assert_eq!(style_at("DEL   10", 6).fg, Some(theme::PEACH));
    assert!(row.contains("DEL   10    │"), "{row}");
    // A lock the track doesn't have is grey all over, brighter than an empty cell.
    assert_eq!(style_at("DCAY", 0).fg, theme::dim().fg);
    assert_eq!(style_at("DCAY  2000", 6).fg, theme::dim().fg);
    assert_eq!(theme::dim().fg, Some(vt_tui::theme::OVERLAY2));
}

#[test]
fn a_gate_cell_shows_its_own_length_dim_and_the_tracks_as_faint_dots() {
    use vt_tui::theme;
    use vt_tui::views::status::{column_name, value_shown};
    let mut song = Song::new(1);
    let steps = [
        "C-3 40 SGL1 80 cut:40 --- ---",
        "C-3 40 HLD2 --- --- ---",
        "C-3 40 --- --- ---",
    ]
    .iter()
    .map(|text| text.parse().unwrap())
    .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (9..40).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).starts_with("C-3 40     SGL1 80 CUT   4000"),
        "{}",
        row(2)
    );
    assert!(row(3).starts_with("C-3 40     HLD2 ·· ···"), "{}", row(3));
    // The `HLD2` lasts two lines of the timeline.
    assert_eq!(row(4).trim_end(), " │", "{}", row(4));
    assert!(row(5).starts_with("C-3 40     ···· ·· ···"), "{}", row(5));
    // The length is the cell's third part, after the gate and its pulses.
    let length = 9 + 16;
    assert_eq!(Some(buffer[(length, 2)].fg), theme::dim().fg);
    assert_ne!(
        Some(buffer[(length - 3, 2)].fg),
        theme::dim().fg,
        "the gate isn't dim"
    );
    assert_eq!(Some(buffer[(length, 3)].fg), theme::faint().fg);
    assert_eq!(Some(buffer[(length, 5)].fg), theme::faint().fg);

    press(&mut app, "llll");
    assert_eq!(column_name(&app), "Gate length");
    assert_eq!(value_shown(&app).as_deref(), Some("50 %"));
    press(&mut app, "j");
    assert_eq!(column_name(&app), "Gate length, the track's");
    assert_eq!(value_shown(&app).as_deref(), Some("100 %"));
}

#[test]
fn a_nudge_shows_dim_by_the_velocity_only_on_a_row_that_has_one() {
    use vt_tui::theme;
    use vt_tui::views::status::{column_name, value_shown};
    let mut song = Song::new(1);
    let steps: Vec<Step> = [
        "C-3 40 -3 SGL1 80 cut:40 --- ---",
        "C-3 40 --- --- ---",
        "--- -- +B --- --- ---",
    ]
    .iter()
    .map(|text| text.parse().unwrap())
    .collect();
    let phrase = Phrase::from_steps(steps);
    song.tracks[0].phrases.insert(0, Arc::new(phrase.clone()));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (9..40).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).starts_with("C-3 40 -3  SGL1 80 CUT   4000"),
        "{}",
        row(2)
    );
    assert!(row(3).starts_with("C-3 40     ···· ·· ···"), "{}", row(3));
    assert!(row(4).starts_with("--- ·· +B  ···· ·· ···"), "{}", row(4));
    let nudge = 9 + 7;
    assert_eq!(Some(buffer[(nudge, 2)].fg), theme::dim().fg);
    assert_eq!(Some(buffer[(nudge + 1, 4)].fg), theme::dim().fg);
    assert_ne!(
        Some(buffer[(nudge - 3, 2)].fg),
        theme::dim().fg,
        "the velocity isn't dim"
    );

    // It's a stop of its own, after the velocity.
    press(&mut app, "ll");
    assert_eq!(app.cursor.field, vt_tui::app::Field::Nudge);
    assert_eq!(column_name(&app), "Nudge");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("3/24 of a sixteenth early")
    );
    press(&mut app, "j");
    assert_eq!(value_shown(&app).as_deref(), Some("on the grid"));
    press(&mut app, "j");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("11/24 of a sixteenth late")
    );

    // A sign typed first shows in the cell until its digit comes.
    press(&mut app, "ki-");
    let buffer = screen(&app);
    let typed: String = (nudge..nudge + 2)
        .map(|x| buffer[(x, 3)].symbol())
        .collect();
    assert_eq!(typed, "-_");
    press(&mut app, "2");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.phrase_here().unwrap().steps()[1].to_string(),
        "C-3 40 -2 --- --- ---"
    );

    // It's saved in the step's text, and a row without one is written as before.
    let saved = ron::to_string(&phrase).unwrap();
    assert!(saved.contains("C-3 40 -3 SGL1 80 cut:40"), "{saved}");
    assert!(saved.contains("\"C-3 40 --- --- ---\""), "{saved}");
    assert_eq!(ron::from_str::<Phrase>(&saved).unwrap(), phrase);
}

#[test]
fn a_selection_lights_up_the_neighbour_columns_it_reaches() {
    let (mut app, _realtime) = tracks_of_different_lengths();
    let selected = vt_tui::theme::selected().bg;

    press(&mut app, "lVW");
    assert!(app.mode.is_visual());
    let buffer = screen(&app);
    assert_eq!(buffer[(42, 2)].bg, selected.unwrap());
    assert_ne!(
        buffer[(42, 3)].bg,
        selected.unwrap(),
        "only the rows selected"
    );
}

#[test]
fn a_beat_row_is_one_that_starts_on_a_beat_in_time() {
    let (mut app, _realtime) = app();
    // Rows 00 and 01 last 2 pulses each, so row 02 starts on the second beat.
    // Collapsed, a row a line, as the timeline would put rows 00 and 01 over two lines each.
    press(&mut app, "lllrSGL2jrSGL2ggzi");
    app.top = 0;
    let buffer = screen(&app);
    let number_colour = |row: u16| buffer[(5, row + 2)].fg;
    let beat = number_colour(0);
    assert_ne!(number_colour(1), beat, "row 01 starts at step 2");
    assert_eq!(number_colour(2), beat, "row 02 starts at step 4");
    assert_ne!(number_colour(4), beat, "row 04 starts at step 6");
    assert_eq!(number_colour(6), beat, "row 06 starts at step 8");
}

#[test]
fn a_rule_under_a_row_marks_where_the_next_beat_or_bar_begins() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 20");
    app.top = 0;
    let buffer = screen(&app);
    let beat = Some(vt_tui::theme::SURFACE1);
    let bar = Some(vt_tui::theme::OVERLAY0);
    assert_eq!(rule_under(&buffer, 2), None, "row 01 is a step");
    assert_eq!(rule_under(&buffer, 5), beat, "row 04 starts a beat");
    assert_eq!(rule_under(&buffer, 13), beat, "row 0C starts a beat");
    assert_eq!(rule_under(&buffer, 17), bar, "row 10 starts a bar");
    assert_eq!(
        rule_under(&buffer, 21),
        None,
        "nothing follows the last row"
    );
    // The rule runs under the whole row, its FX too.
    let fx = &buffer[(40, 17)];
    assert!(fx.modifier.contains(ratatui::style::Modifier::UNDERLINED));
    assert_eq!(fx.underline_color, vt_tui::theme::OVERLAY0);
}

#[test]
fn the_rules_follow_rows_that_last_several_pulses() {
    let (mut app, _realtime) = app();
    // Rows 00 and 01 last 2 pulses each, so row 02 starts on the second beat.
    // Collapsed, a row a line, as the timeline would put rows 00 and 01 over two lines each.
    press(&mut app, "lllrSGL2jrSGL2ggzi");
    app.top = 0;
    let buffer = screen(&app);
    assert_eq!(rule_under(&buffer, 2), None, "row 01 starts at step 2");
    assert_eq!(
        rule_under(&buffer, 3),
        Some(vt_tui::theme::SURFACE1),
        "row 02 starts at step 4"
    );
    assert_eq!(rule_under(&buffer, 4), None, "row 03 starts at step 5");
}

#[test]
fn the_loops_last_row_is_followed_by_a_bar() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    press(&mut app, "2]l");
    app.top = 0;
    let buffer = screen(&app);
    // Rows -02 and -01 come before the loop, which plays rows 00 to 03 of those kept.
    assert_eq!(row_number_at(&buffer, 7), " 03");
    let bar = Some(vt_tui::theme::OVERLAY0);
    assert_eq!(
        rule_under(&buffer, 3),
        bar,
        "row -01 is followed by the loop"
    );
    assert_eq!(
        rule_under(&buffer, 7),
        bar,
        "the loop starts again after row 03"
    );
    assert_eq!(rule_under(&buffer, 4), None);
}

#[test]
fn a_tracks_heading_has_its_machine_and_mute_or_solo_glyphs() {
    use vt_tui::glyphs::{DRUM, MUTE, SAMPLER, SOLO, SYNTH};
    let (mut app, _rt) = app();
    let heading = |app: &App, y: u16| -> String {
        let buffer = screen(app);
        (0..100).map(|x| buffer[(x, y)].symbol()).collect()
    };
    let machine = app.song().tracks[0].machine;
    let glyph = vt_tui::glyphs::machine(machine);
    assert!(heading(&app, 0).starts_with(&format!("{glyph} Track 01  Chain")));
    tap(&mut app, " m");
    assert!(heading(&app, 0).starts_with(&format!("{glyph} Track 01 {MUTE}  Chain")));
    // Over its column in the overview too, in place of the number's last cell.
    assert!(heading(&app, 1).contains(&format!(" 01{MUTE}")));
    tap(&mut app, " m");
    app.levels[0].solo = true;
    assert!(heading(&app, 0).contains(&format!("Track 01 {SOLO}  Chain")));
    assert_eq!(vt_tui::glyphs::machine(Machine::Sampler), SAMPLER);
    assert_eq!(
        vt_tui::glyphs::machine(Machine::Plaits(SynthVoice::Kick)),
        DRUM
    );
    assert_eq!(
        vt_tui::glyphs::machine(Machine::Plaits(SynthVoice::Fm)),
        SYNTH
    );
}

#[test]
fn a_track_at_an_eighth_has_its_own_playhead() {
    let mut song = Song::new(2);
    song.tracks[0].phrases.insert(0, Arc::new(Phrase::new(16)));
    song.tracks[1].phrases.insert(0, Arc::new(Phrase::new(4)));
    song.tracks[1].speed = vt_model::speed::Speed::Eighth;
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    app.set_playhead(6);
    assert_eq!(app.chain_playhead(0, 0), Some(6));
    assert_eq!(app.chain_playhead(1, 0), Some(3));
    app.set_playhead(9);
    assert_eq!(
        app.chain_playhead(1, 0),
        Some(0),
        "its fifth eighth is its first row again"
    );
}
