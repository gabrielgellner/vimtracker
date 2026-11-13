//! The song view: sections, their chains and labels, launches, and jamming chains on tracks.

use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{SectionLoop, Transport};
use vt_engine::link::Realtime;
use vt_model::song::{Launch, SectionStart};
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::View;

mod common;
use common::*;

#[test]
fn the_song_view_adds_copies_and_removes_sections() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    assert_eq!(app.view, View::Song);
    assert_eq!(app.song().sections.len(), 1);
    press(&mut app, "a");
    assert_eq!(app.song().sections.len(), 2);
    assert_eq!(app.cursor.section, 1);
    assert_eq!(
        app.song().sections[1].chains,
        vec![None; app.song().tracks.len()],
        "no chains"
    );
    assert_eq!(app.song().tracks[0].chains.len(), 1, "none made");
    press(&mut app, "gc");
    assert_eq!(app.song().sections.len(), 3);
    assert_eq!(app.cursor.section, 2);
    assert_eq!(
        app.song().sections[2].chains,
        app.song().sections[1].chains,
        "the same chains"
    );
    press(&mut app, "dd");
    assert_eq!(app.song().sections.len(), 2);
    assert_eq!(app.cursor.section, 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections.len(), 3);
    press(&mut app, "ddddddd");
    assert_eq!(app.song().sections.len(), 1, "one always stays");
    assert!(app.status.contains("at least one"), "{}", app.status);
    assert_eq!(app.cursor.section, 0);
    let text = song_text(&app);
    assert!(text[2].contains("Length") && text[2].contains("Start"));
    assert!(text[3].contains("16") && text[3].contains("new"));
}

#[test]
fn the_song_view_edits_length_start_and_chains() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    assert_eq!(column_name(&app), "Length");
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].length, Some(17));
    press(&mut app, "-_");
    assert_eq!(app.song().sections[0].length, Some(1));
    press(&mut app, "i32");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].length, Some(32));
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].length, None);
    press(&mut app, "i0");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].length, None, "{}", app.status);

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Rpt");
    assert!(song_text(&app)[2].contains("Rpt"));
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].repeat.get(), 2);
    press(&mut app, "+");
    assert_eq!(app.song().sections[0].repeat.get(), 6);
    press(&mut app, "__");
    assert_eq!(app.song().sections[0].repeat.get(), 1, "never below once");
    press(&mut app, "i12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].repeat.get(), 12);
    press(&mut app, "i0");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].repeat.get(), 12, "{}", app.status);
    assert!(app.status.contains("repeat"), "{}", app.status);
    press(&mut app, "gc");
    assert_eq!(app.song().sections[1].repeat.get(), 12, "a copy keeps it");
    press(&mut app, "u");
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].repeat.get(), 1);

    press(&mut app, "l");
    assert_eq!(column_name(&app), "Start");
    press(&mut app, "K");
    assert_eq!(app.song().sections[0].start, SectionStart::Reset);
    assert!(song_text(&app)[3].contains("reset"));
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].start, SectionStart::New);

    press(&mut app, "ll");
    assert_eq!(column_name(&app), "Track 02 chain");
    press(&mut app, "x");
    assert_eq!(app.song().sections[0].chains[1], None);
    assert!(song_text(&app)[3].contains("--"));
    press(&mut app, "i05");
    assert_eq!(app.song().sections[0].chains[1], Some(5));
    assert!(
        app.song().tracks[1].chains.contains_key(&5),
        "made as needed"
    );
    press(&mut app, "=");
    assert_eq!(app.song().sections[0].chains[1], Some(6));
    press(&mut app, "$");
    assert_eq!(column_name(&app), "Master chain");
    press(&mut app, "0");
    assert_eq!(column_name(&app), "Length");
}

#[test]
fn other_views_edit_the_section_under_the_cursor() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1alllgd 3");
    let two = app.song().sections[1].chains[0].unwrap();
    let phrase = app.song().tracks[0].chains[&two].entries[0].phrase;
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.song().tracks[0].phrases[&phrase].steps()[0].note,
        Note::On(60)
    );
    assert_eq!(
        app.song().tracks[0].phrases[&0].steps()[0].note,
        Note::Empty
    );
    command(&mut app, "reset 8");
    assert_eq!(app.song().sections[1].reset, Some(8));
    assert_eq!(app.song().sections[0].reset, None, "only this section");
    press(&mut app, " 1k");
    assert_eq!(app.cursor.section, 0);
    assert_eq!(app.phrase_id(), Some(0));
}

#[test]
fn each_track_numbers_its_own_chains_and_phrases() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "W");
    assert_eq!(app.phrase_id(), Some(0));
    assert_eq!(
        notes(&app)[0],
        Note::Empty,
        "not the first track's phrase 00"
    );
    press(&mut app, " 2gc");
    assert_eq!(app.chain_entries()[0].phrase, 1);
    assert_eq!(
        app.song().tracks[0].phrases.len(),
        1,
        "the first track has none new"
    );
    command(&mut app, "chain FF");
    assert_eq!(app.chain_id(), Some(0xFF));
    assert!(!app.song().tracks[0].chains.contains_key(&0xFF));
    press(&mut app, " 1lll");
    assert!(
        song_text(&app)[3].contains("FF"),
        "{:?}",
        song_text(&app)[3]
    );
}

#[test]
fn an_empty_section_can_be_visited_from_every_view_and_filled() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1a");
    assert!(song_text(&app)[4].contains("--"));
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            vt_tui::views::chain::draw(frame, frame.area(), &app);
            vt_tui::views::phrase::draw(frame, frame.area(), &app);
            vt_tui::views::track::draw(frame, frame.area(), &app);
            vt_tui::views::mixer::draw(frame, frame.area(), &app);
        })
        .unwrap();
    press(&mut app, " 3ia");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.song().sections[1].chains[0], None);
    press(&mut app, " 1lllgd");
    assert_eq!(app.view, View::Phrase);
    assert!(app.song().sections[1].chains[0].is_some());
    assert_eq!(app.song().sections[1].chains[1], None, "only this track");
}

#[test]
fn gd_in_the_song_view_opens_a_new_chains_phrase_as_one_undo() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1gd");
    assert_eq!(app.view, View::Song, "{}", app.status);
    assert!(app.status.contains("track"));
    press(&mut app, "llllx");
    let depth = app.history.undo_depth();
    press(&mut app, "gd");
    assert_eq!(app.view, View::Phrase);
    assert_eq!(app.cursor.track, 1);
    assert!(app.chain_id().is_some());
    assert_eq!(app.chain_entries().len(), 1);
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn gd_in_the_song_view_goes_by_how_many_phrases_a_chain_has() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1lllgd");
    assert_eq!(app.view, View::Phrase, "one phrase");
    press(&mut app, " 2jgd");
    assert_eq!(app.chain_entries().len(), 2);
    press(&mut app, " 1gd");
    assert_eq!(app.view, View::Chain, "several");
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.view, View::Song, "C-o comes back");
}

#[test]
fn launch_sets_when_launches_start_and_shows_it() {
    let (mut app, _realtime) = app();
    assert_eq!(app.song().launch, Launch::every(16).unwrap());
    command(&mut app, "launch");
    assert!(app.status.contains("1bar"), "{}", app.status);
    let depth = app.history.undo_depth();
    command(&mut app, "launch 1/4");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
    assert!(app.status.contains("1/4"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1);
    command(&mut app, "launch 12");
    assert!(app.status.contains("12-step"), "{}", app.status);
    command(&mut app, "launch chain");
    assert_eq!(app.song().launch, Launch::ChainEnd);
    assert!(app.status.contains("chain ends"), "{}", app.status);
    press(&mut app, "uu");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
    command(&mut app, "launch soon");
    assert_eq!(app.song().launch, Launch::every(4).unwrap());
}

#[test]
fn a_playhead_follows_the_chain_a_track_is_on_whichever_row_shows_it() {
    let (mut app, mut realtime) = jamming();
    play_on(&mut app, &mut realtime, 3);
    assert_eq!(app.playing_section, 0);
    assert_eq!(app.cursor.section, 1);
    assert_eq!(
        app.chain_playhead(0, 1),
        None,
        "not playing this row's chain"
    );
    assert!(app.chain_playhead(0, 0).is_some(), "but this one");
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 16);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    assert_eq!(app.chain_playhead(0, 0), None, "replaced by the launch");
    let launched = app.chain_playhead(0, 1).unwrap();
    assert!(launched < 16, "from the launch's own start: {launched}");
    assert_eq!(app.chain_playhead(1, 0).map(|step| step < 16), Some(true));
}

#[test]
fn space_c_in_a_phrase_plays_from_that_step_of_its_chain() {
    let (mut app, mut realtime) = app();
    press(&mut app, "5j c");
    block(&mut realtime, &mut app);
    block(&mut realtime, &mut app);
    app.tick();
    assert_eq!(app.transport, Transport::Playing);
    assert_eq!(app.chain_playhead(0, 0), Some(5));
}

#[test]
fn enter_says_when_the_launch_will_start() {
    let (mut app, _realtime) = jamming();
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("on the 1bar grid"), "{}", app.status);
    command(&mut app, "launch chain");
    press(&mut app, " 1");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("when its chain ends"), "{}", app.status);
}

#[test]
fn a_label_names_the_chain_under_the_cursor() {
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = app();
    press(&mut app, " 1lll");
    assert_eq!(column_name(&app), "Track 01 chain");
    let depth = app.history.undo_depth();
    command(&mut app, "label verse bass");
    assert_eq!(app.song().tracks[0].chains[&0].label, "verse bass");
    assert_eq!(column_name(&app), "Track 01 chain: verse bass");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "gc");
    assert_eq!(
        column_name(&app),
        "Track 01 chain: verse bass",
        "same chain"
    );
    press(&mut app, "u");
    command(&mut app, "label");
    assert_eq!(column_name(&app), "Track 01 chain");
    press(&mut app, " 2");
    command(&mut app, "label from chain view");
    assert_eq!(
        app.song().tracks[0].chains[&0].label,
        "from chain view",
        "{}",
        app.status
    );
}

#[test]
fn label_is_refused_without_a_chain_to_name() {
    let (mut app, _realtime) = app();
    command(&mut app, "label nope");
    assert!(app.status.contains("song and chain"), "{}", app.status);
    press(&mut app, " 1");
    command(&mut app, "label nope");
    assert!(app.status.contains("track's column"), "{}", app.status);
    press(&mut app, "lllx");
    command(&mut app, "label nope");
    assert!(app.status.contains("no chain"), "{}", app.status);
    assert!(app.song().tracks[0].chains[&0].label.is_empty());
}

#[test]
fn k_chooses_a_cells_chain_by_number_or_label() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1aaj");
    press(&mut app, "lll");
    press(&mut app, "=");
    assert_eq!(app.song().sections[2].chains[0], Some(1));
    command(&mut app, "label bass");
    press(&mut app, "kK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.shown().count(), 3, "-- and both chains");
    press(&mut app, "bass");
    assert_eq!(app.picker.as_ref().unwrap().shown().count(), 1);
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[1].chains[0], Some(1));
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "K");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(listed)
            .as_deref(),
        Some("01  bass"),
        "the cell's own is highlighted"
    );
    special(&mut app, KeyCode::Esc);
    press(&mut app, "K");
    press(&mut app, "--");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[1].chains[0], None);
    press(&mut app, "0K");
    assert!(app.status.contains("track's column"), "{}", app.status);
    assert!(app.picker.is_none());
}

#[test]
fn g_upper_c_copies_the_chain_in_the_cursors_column_alone() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1llll");
    command(&mut app, "label bass");
    press(&mut app, "gc");
    assert_eq!(app.cursor.section, 1);
    let depth = app.history.undo_depth();
    let old = app.song().tracks[1].chains[&0].clone();
    press(&mut app, "gC");
    assert_eq!(app.song().sections[1].chains[1], Some(1), "this cell");
    assert_eq!(app.song().sections[0].chains[1], Some(0), "the other row");
    assert_eq!(app.song().sections[1].chains[0], Some(0), "the other track");
    assert_eq!(app.song().tracks[1].chains[&0], old);
    let copy = &app.song().tracks[1].chains[&1];
    assert_eq!(copy.label, "bass");
    assert_ne!(copy.entries[0].phrase, old.entries[0].phrase);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections[1].chains[1], Some(0));

    press(&mut app, "0gC");
    assert!(app.status.contains("track's column"), "{}", app.status);
    press(&mut app, "llllx");
    press(&mut app, "gC");
    assert!(app.status.contains("no chain"), "{}", app.status);
}

#[test]
fn a_chain_used_by_another_section_counts_as_shared() {
    let (mut app, _realtime) = app();
    assert_eq!(app.chain_uses(), 1);
    press(&mut app, " 1gc");
    assert_eq!(app.chain_uses(), 2);
    assert_eq!(app.phrase_uses(app.phrase_id().unwrap()).len(), 2);
}

#[test]
fn loop_and_transport_keys_reach_the_section_being_edited() {
    let (mut app, mut realtime) = app();
    with_a_sound(&mut app);
    press(&mut app, " 1a");
    block(&mut realtime, &mut app);
    command(&mut app, "loop");
    assert_eq!(app.looping, Some(SectionLoop::one(1)));
    assert!(app.status.contains("section 02"), "{}", app.status);
    press(&mut app, " c");
    block(&mut realtime, &mut app);
    block(&mut realtime, &mut app);
    app.tick();
    assert_eq!(app.playing_section, 1);
    press(&mut app, " 1k");
    assert!(song_text(&app)[4].contains(">02"), "{:?}", song_text(&app));
    command(&mut app, "loop");
    assert_eq!(app.looping, Some(SectionLoop::one(0)));
    command(&mut app, "loop");
    assert_eq!(app.looping, None);
}

#[test]
fn capital_v_over_sections_then_loop_loops_them_from_the_next_launch_line() {
    use vt_tui::glyphs::{PLAY, WAITING};
    let (mut app, mut rt) = app();
    with_a_sound(&mut app);
    // Three sections, and the cursor on the second.
    press(&mut app, " 1aak");
    press(&mut app, "V");
    assert!(
        status_line(&app).starts_with(" VISUAL "),
        "{}",
        status_line(&app)
    );
    press(&mut app, "j");
    command(&mut app, "loop");
    assert_eq!(app.looping, Some(SectionLoop::new(1, 2)));
    assert_eq!(app.status, "Looping sections 02-03");
    assert_eq!(app.section_anchor, None, "the command ends the selection");
    assert!(status_line(&app).contains("Sec 3/3 loop 2-3"));
    block(&mut rt, &mut app);
    assert_eq!(rt.engine().looping(), Some(SectionLoop::new(1, 2)));
    let rows = song_text(&app);
    assert!(rows[3].starts_with(" 01  "), "{rows:?}");
    assert!(rows[4].starts_with(" 02\u{2502} ") && rows[5].starts_with(" 03\u{2502} "));

    // Set while playing a section outside it, it waits for the bar, and its first section
    // starts there.
    special(&mut app, KeyCode::BackTab);
    block(&mut rt, &mut app);
    press(&mut app, "kVj");
    command(&mut app, "loop");
    assert_eq!(app.status, "Looping sections 02-03 on the 1bar grid");
    block(&mut rt, &mut app);
    assert!(app.loop_waiting);
    assert!(status_line(&app).contains(&format!(" {PLAY} {WAITING} LOOP  ")));
    assert_eq!(app.playing_section, 0);
    for _ in 0..800 {
        block(&mut rt, &mut app);
    }
    assert!(!app.loop_waiting, "landed on the bar");
    assert_eq!(app.playing_section, 1);

    // One that holds the section playing only changes what comes next, so it's at once.
    press(&mut app, "kkVj");
    command(&mut app, "loop");
    block(&mut rt, &mut app);
    assert!(!app.loop_waiting);
    assert_eq!(rt.engine().looping(), Some(SectionLoop::new(0, 1)));

    // `:loop` alone on one of them goes back to the song.
    command(&mut app, "loop");
    assert_eq!(app.looping, None);
    assert_eq!(app.status, "Playing the song");
    block(&mut rt, &mut app);
    assert_eq!(rt.engine().looping(), None);
}

#[test]
fn esc_or_v_again_ends_a_selection_of_sections_and_so_does_leaving_the_view() {
    let (mut app, _rt) = app();
    press(&mut app, " 1aV");
    assert!(app.section_anchor.is_some());
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.section_anchor, None);
    press(&mut app, "VV");
    assert_eq!(app.section_anchor, None);
    press(&mut app, "V 3");
    assert_eq!(app.section_anchor, None);
    press(&mut app, " 1V:");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.section_anchor, None, "a command line left ends it too");
}

#[test]
fn the_status_line_counts_sections_when_there_are_several() {
    let (mut app, _realtime) = app();
    let status = |app: &App| {
        let backend = ratatui::backend::TestBackend::new(120, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..120)
            .map(|x| buffer[(x, 0)].symbol())
            .collect::<String>()
    };
    assert!(!status(&app).contains("Sec "));
    press(&mut app, " 1a");
    assert!(status(&app).contains("Sec 2/2"), "{}", status(&app));
}

fn play_on(app: &mut App, realtime: &mut Realtime, steps: usize) {
    let mut out = vec![0.0; steps * 6000 * 2];
    realtime.process(&mut out);
    app.tick();
}

/// A song view on a second section whose chains differ from the first's.
fn jamming() -> (App, Realtime) {
    let (mut app, mut realtime) = app();
    press(&mut app, " 1alllgd 1lgd 1");
    assert_eq!(app.song().sections[1].chains[..2], [Some(1), Some(1)]);
    press(&mut app, "\t");
    special(&mut app, KeyCode::Tab);
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.transport, Transport::Playing);
    (app, realtime)
}

#[test]
fn enter_cues_this_chain_on_one_track_and_holds_the_section() {
    let (mut app, mut realtime) = jamming();
    assert_eq!(app.cursor.section, 1);
    assert_eq!(app.playing_section, 0);
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.looping, Some(SectionLoop::one(0)), "the jam stays here");
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.jam[0].cued, Some(Some(1)));
    assert!(
        song_text(&app)[4].contains("01~"),
        "{:?}",
        song_text(&app)[4]
    );
    play_on(&mut app, &mut realtime, 16);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    assert_eq!(app.jam[0].cued, None);
    assert_eq!(app.jam[1].playing, None, "the other track is untouched");
    assert!(
        song_text(&app)[4].contains("01>"),
        "{:?}",
        song_text(&app)[4]
    );
    assert_eq!(
        app.song().sections[0].chains[0],
        Some(0),
        "the song is unchanged"
    );
}

#[test]
fn enter_and_c_need_the_song_to_be_playing_and_enter_a_track() {
    let (mut app, _realtime) = app();
    press(&mut app, " 1");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("while playing"), "{}", app.status);
    press(&mut app, "C");
    assert!(app.status.contains("start the song"), "{}", app.status);
    assert_eq!(app.song().sections.len(), 1);
    let (mut app, _realtime) = jamming();
    press(&mut app, "0");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("track's column"), "{}", app.status);
}

#[test]
fn c_captures_what_each_track_is_playing_as_one_undo() {
    let (mut app, mut realtime) = jamming();
    let depth = app.history.undo_depth();
    press(&mut app, "0lll");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    assert_eq!(app.jam[0].playing, Some(Some(1)));
    press(&mut app, "C");
    assert_eq!(app.song().sections.len(), 3);
    assert_eq!(app.song().sections[1].chains[0], Some(1), "launched");
    assert_eq!(app.song().sections[1].chains[1], Some(0), "from its row");
    assert_eq!(
        app.song().sections[1].length,
        Some(16),
        "the row it came from"
    );
    assert_eq!(app.cursor.section, 1);
    assert_eq!(app.looping, Some(SectionLoop::one(1)));
    play_on(&mut app, &mut realtime, 1);
    assert_eq!(app.playing_section, 1, "it carries on in the new row");
    assert_eq!(app.jam[0].playing, Some(Some(1)), "without restarting");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(app.song().sections.len(), 2);
}

#[test]
fn c_copies_the_settings_of_the_row_it_came_from() {
    let (mut app, mut realtime) = jamming();
    press(&mut app, "k0");
    command(&mut app, "reset 8");
    press(&mut app, "llK");
    assert_eq!(app.song().sections[0].start, SectionStart::Reset);
    play_on(&mut app, &mut realtime, 1);
    press(&mut app, "C");
    assert_eq!(app.song().sections[1].reset, Some(8));
    assert_eq!(app.song().sections[1].start, SectionStart::Reset);
}

#[test]
fn a_stopped_track_captures_as_empty_and_nothing_playing_is_refused() {
    let (mut app, mut realtime) = jamming();
    press(&mut app, "0lll");
    press(&mut app, "x");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    assert_eq!(app.jam[0].playing, Some(None));
    press(&mut app, "C");
    assert_eq!(app.song().sections[1].chains[0], None);
    assert!(app.song().sections[1].chains[1].is_some());

    let (mut app, mut realtime) = jamming();
    press(&mut app, "0lllx");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "lx");
    special(&mut app, KeyCode::Enter);
    play_on(&mut app, &mut realtime, 17);
    press(&mut app, "C");
    assert!(app.status.contains("Nothing is playing"), "{}", app.status);
    assert_eq!(app.song().sections.len(), 2);
}
