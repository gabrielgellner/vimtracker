//! FX commands chosen with `K`: their lists, step locks by label, destinations, lanes,
//! conditions, accumulators, arps and orders.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::{Mode, View};
use vt_tui::input::picker::Pick;

mod common;
use common::*;

#[test]
fn a_step_locks_the_gate_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "len");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "40");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("gate:40"), "{step}");
    assert_eq!(column_name(&app), "Machine: Gate");
    assert_eq!(value_shown(&app).as_deref(), Some("25 %"));
}

#[test]
fn a_chosen_command_shows_in_its_cell_while_its_value_is_typed() {
    let (mut app, _realtime) = app();
    let row = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 2)].symbol()).collect()
    };
    // From K on an empty cell, as from typing, the command waits for its value.
    press(&mut app, "lllllK");
    press(&mut app, "CUT");
    special(&mut app, KeyCode::Enter);
    assert!(row(&app).contains("CUT   00"), "{}", row(&app));
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        None,
        "not written yet"
    );
    press(&mut app, "4");
    assert!(row(&app).contains("CUT   4_"), "{}", row(&app));
    press(&mut app, "0");
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        Some(vt_model::step::FxCommand::CUT)
    );
    // Typing the name works the same way, over a value already there.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "khiRES");
    assert!(row(&app).contains("RES_"), "{}", row(&app));
    special(&mut app, KeyCode::Enter);
    assert!(row(&app).contains("RES   40"), "{}", row(&app));
    assert_eq!(
        app.phrase_here().unwrap().steps()[0].fx[0].command,
        Some(vt_model::step::FxCommand::CUT),
        "still the old one until the value"
    );
}

#[test]
fn a_value_shows_its_mark_and_events_and_empty_slots_show_none() {
    let mut song = Song::new(1);
    let step = |text: &str| text.parse::<Step>().unwrap();
    let phrase = Phrase::from_steps(vec![
        step("C-3 40 cut:40 sna:20h res:10g"),
        step("C-3 40 del:10 --- ---"),
    ]);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, _realtime) = connect(Engine::new(48_000, version));
    let app = App::new(song, None, samples, handle, 48_000);
    let buffer = screen(&app);
    let row = |y: u16| -> String { (0..80).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(
        row(2).contains(&format!(
            "CUT   4000 {} SNA   20   {} RES   1000 {}",
            vt_tui::glyphs::LOCK,
            vt_tui::glyphs::HOLD,
            vt_tui::glyphs::GLIDE
        )),
        "{}",
        row(2)
    );
    // The lock is dim, and a hold and a glide are in the value's fine colour.
    let at = |glyph: char| row(2).chars().position(|c| c == glyph).unwrap() as u16;
    let fine = vt_tui::theme::fine(vt_tui::theme::MAUVE);
    assert_eq!(
        buffer[(at(vt_tui::glyphs::LOCK), 2)].fg,
        vt_tui::theme::OVERLAY2
    );
    assert_eq!(buffer[(at(vt_tui::glyphs::HOLD), 2)].fg, fine);
    assert_eq!(buffer[(at(vt_tui::glyphs::GLIDE), 2)].fg, fine);
    assert!(
        row(3).contains("DEL   10     ···   ··     ···   ··"),
        "{}",
        row(3)
    );
}

#[test]
fn k_lists_commands_by_category_and_opens_searching() {
    use vt_tui::input::picker::{Focus, Row};
    let (mut app, _realtime) = app();
    press(&mut app, "lllllK");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.focus, Focus::Search);
    assert!(picker.query.is_empty());
    let names: Vec<&str> = picker
        .rows()
        .iter()
        .filter_map(|row| match row {
            Row::Category(category) => Some(picker.category(*category).0),
            Row::Item(_) => None,
        })
        .collect();
    assert_eq!(names[..2], ["Filter", "Output"]);
    assert_eq!(names.last().copied(), Some("Events"));
    assert_eq!(picker.current().unwrap().label, "---", "on the empty slot");
    // Typing searches long names as well as labels.
    press(&mut app, "send b");
    let current = app.picker.as_ref().unwrap().current().unwrap();
    assert_eq!(
        (current.label.as_str(), current.meaning.as_str()),
        ("SNB", "Send B")
    );
}

#[test]
fn a_slots_settings_are_on_the_dest_list_only_while_it_holds_something() {
    let (mut app, _realtime) = app();
    // Mod 4's Dest, the first LFO's, is on the Mod page, beside the envelopes.
    press(&mut app, &format!(" 5L{}$", to_lfo(0)));
    let list = |app: &mut App| {
        press(app, "K/");
        let labels: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        labels
    };
    assert!(!list(&mut app).iter().any(|label| label.starts_with("1 ")));

    press(&mut app, "L="); // Distort in slot 1.
    press(&mut app, &format!("H{}$", to_lfo(0)));
    let labels = list(&mut app);
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with("1 Amount  Distort in slot 1")),
        "{labels:?}"
    );

    // Choosing one saves it, and it reads back on the page.
    press(&mut app, "K/1 amount");
    special(&mut app, KeyCode::Enter);
    let target = SettingId::slot(0, vt_model::settings::Effect::Distort, 1).unwrap();
    assert_eq!(app.song().tracks[0].mods[3].destination, Some(target));
    let lfo = |app: &App| device_title(app, "Mod 4");
    assert!(lfo(&app).contains("1 Amount"), "{}", lfo(&app));

    // Emptying the slot leaves it chosen, doing nothing, and saying so.
    press(&mut app, "L0x");
    press(&mut app, "H");
    assert_eq!(app.song().tracks[0].mods[3].destination, Some(target));
    assert!(lfo(&app).contains("1 ---"), "{}", lfo(&app));
}

#[test]
fn stepping_dest_goes_through_a_slots_settings() {
    let (mut app, _realtime) = on_fx_page();
    press(&mut app, "=H");
    press(&mut app, &format!("{}$", to_lfo(0)));
    // Past the fifteen a sample track has, and `Pitch` in scale beside `Pitch`, as a synth's
    // settings aren't on it.
    for _ in 0..17 {
        press(&mut app, "=");
    }
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 0).unwrap()),
    );
}

#[test]
fn a_slots_setting_is_chosen_for_a_step_by_its_label_and_saved_by_its_key() {
    use vt_model::settings::{Effect, SettingId};
    use vt_model::step::FxCommand;
    let (mut app, _realtime) = app();
    press(&mut app, " 5LL= 3");
    let command = |app: &App, step: usize| app.phrase_here().unwrap().steps()[step].fx[0].command;
    let setting = |at| FxCommand::Set(SettingId::slot(0, Effect::Distort, at).unwrap());

    // The list has what's on the track, by label and then by group and name.
    press(&mut app, "lllllK");
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert!(
        labels.iter().any(|label| label.starts_with("1MIX  1 Mix")),
        "{labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label.starts_with("2")),
        "{labels:?}"
    );
    press(&mut app, "1amnt");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    assert_eq!(command(&app, 0), Some(setting(1)));
    assert_eq!(app.phrase_here().unwrap().steps()[0].fx[0].value, 0x8000);
    let text = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(
        text.contains("1.distort.amount:80"),
        "a file holds the setting's key: {text}"
    );

    // Typed, a slot's label needs its digit, and `Enter` takes it to the value.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggj0llllli1mix");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "40");
    assert_eq!(command(&app, 1), Some(setting(4)));

    // A label that isn't there says so, and opens the list searching for it.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "ggjj0llllliamnt");
    special(&mut app, KeyCode::Enter);
    assert_eq!(command(&app, 2), None);
    assert!(app.status.contains("isn't a label"), "{}", app.status);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.query, "amnt");
    assert_eq!(
        picker.current().map(listed).as_deref(),
        Some("1AMNT  1 Amount")
    );
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20");
    assert_eq!(command(&app, 2), Some(setting(1)));
}

#[test]
fn the_phrase_view_names_a_slots_setting_in_a_wide_column() {
    let (mut app, _realtime) = app();
    press(&mut app, " 5LL= 3lllllK1amnt");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    special(&mut app, KeyCode::Esc);
    let backend = ratatui::backend::TestBackend::new(120, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::phrase::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..12)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("1AMNT 80"), "{text}");
    assert!(text.contains("···   ··"), "{text}");
}

#[test]
fn the_lists_only_hold_the_loaded_machines_settings_by_their_names() {
    use vt_model::song::{Machine, SynthVoice};
    let (mut app, _realtime) = app();
    let labels = |app: &mut App, keys: &str| {
        press(app, keys);
        let found: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        found
    };
    let on = |labels: &[String], name: &str| {
        labels
            .iter()
            .any(|label| label.starts_with(&format!("{name}  ")))
    };
    // A sample track has no synth settings, in Dest or for a step.
    let dest = labels(&mut app, &format!(" 5L{}$K/", to_lfo(0)));
    assert!(!on(&dest, "Har") && on(&dest, "Pitch"), "{dest:?}");
    let steps = labels(&mut app, " 3lllllK");
    assert!(!on(&steps, "HAR") && on(&steps, "CUT"), "{steps:?}");

    // On a synth they're there under their names on this voice.
    press(&mut app, " 5H0Kfm");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].machine,
        Machine::Plaits(SynthVoice::Fm)
    );
    let dest = labels(&mut app, &format!("Lgg{}$K/", to_lfo(0)));
    assert!(on(&dest, "Ratio") && on(&dest, "Feedback"), "{dest:?}");
    let steps = labels(&mut app, " 3K");
    assert!(on(&steps, "INDX"), "{steps:?}");

    // One chosen stays through a change of machine, and says what happened.
    press(&mut app, " 6K/ratio");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::voice(SynthVoice::Fm, 0).unwrap())
    );
    press(&mut app, "Hgg0K");
    press(&mut app, "sa");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::voice(SynthVoice::Fm, 0).unwrap())
    );
    press(&mut app, "L");
    let line = device_title(&app, "Mod 4");
    assert!(line.contains("Ratio ---"), "{line}");
}

#[test]
fn an_lfos_dest_list_only_reaches_the_lfos_after_it() {
    let (mut app, _realtime) = app();
    let labels = |app: &mut App, keys: &str| {
        press(app, keys);
        let found: Vec<String> = app
            .picker
            .as_ref()
            .unwrap()
            .items()
            .iter()
            .map(listed)
            .collect();
        // The first Esc leaves the search for the list, and the second closes it.
        special(app, KeyCode::Esc);
        special(app, KeyCode::Esc);
        found
    };
    let has = |labels: &[String], name: &str| {
        labels
            .iter()
            .any(|label| label.starts_with(&format!("{name}  ")))
    };
    let first = labels(&mut app, &format!(" 5L{}$K/", to_lfo(0)));
    assert!(
        has(&first, "M5 Depth") && has(&first, "M5 Phase") && has(&first, "M6 Phase"),
        "{first:?}"
    );
    assert!(
        !has(&first, "M4 Depth") && !has(&first, "M3 Depth"),
        "{first:?}"
    );
    let second = labels(&mut app, "}K/");
    assert!(
        !has(&second, "M4 Depth") && !has(&second, "M5 Depth") && has(&second, "M6 Depth"),
        "{second:?}"
    );
    let third = labels(&mut app, "}K/");
    assert!(
        !third
            .iter()
            .any(|label| label.starts_with("M") && label.contains(" Depth  ")),
        "{third:?}"
    );
    // Across to `Mod 3`, beside `Mod 6`: an envelope reaches every LFO.
    let envelope = labels(&mut app, "0hK/");
    assert!(
        has(&envelope, "M4 Depth") && has(&envelope, "M5 Phase") && has(&envelope, "M6 Phase"),
        "{envelope:?}"
    );
    assert!(!has(&envelope, "M2 Depth"), "{envelope:?}");

    press(&mut app, &format!("gg{}$K/m5 depth", to_lfo(0)));
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::modulator(4, 0).unwrap())
    );
    let line = device_title(&app, "Mod 4");
    assert!(line.contains("M5 Depth"), "{line}");
}

#[test]
fn dest_is_a_tree_of_categories_that_fold_and_a_search_flattens() {
    use vt_tui::input::picker::{Focus, Row};
    let (mut app, _realtime) = app();
    // Distort in slot 1, then Mod 4's Dest.
    press(&mut app, &format!(" 5LL=H{}$K", to_lfo(0)));
    let current = |app: &App| {
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.clone())
            .unwrap_or_default()
    };
    let picker = |app: &App| app.picker.as_ref().unwrap().clone();
    assert_eq!(picker(&app).focus, Focus::List);
    assert_eq!(current(&app), "---", "on what's chosen: nothing");
    assert!(
        picker(&app).rows().iter().all(|row| match row {
            Row::Category(category) => !picker(&app).category(*category).2,
            Row::Item(item) => !picker(&app).is_nested(*item),
        }),
        "every category folded, as none holds what's chosen"
    );

    // Down to Filter's heading, l into it, and down its list.
    press(&mut app, "jl");
    assert_eq!(current(&app), "Cut");
    press(&mut app, "j");
    assert_eq!(current(&app), "Res");
    assert_eq!(app.mode, Mode::Picker, "moving doesn't choose");

    // h closes it and goes up to it.
    press(&mut app, "h");
    assert_eq!(current(&app), "", "on the heading");
    let heading = picker(&app).rows()[picker(&app).selected];
    assert!(matches!(heading, Row::Category(_)));

    // A letter that isn't a key starts a search, which reaches every category.
    press(&mut app, "1 mi");
    assert_eq!(picker(&app).focus, Focus::Search);
    assert_eq!(current(&app), "1 Mix");
    // Esc goes back to the list, keeping what it found, as Esc leaves Insert mode.
    special(&mut app, KeyCode::Esc);
    assert_eq!(picker(&app).focus, Focus::List);
    assert_eq!(picker(&app).query, "1 mi");
    assert_eq!(current(&app), "1 Mix");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap())
    );

    // It opens on what's chosen, inside its category, and Esc leaves it.
    press(&mut app, "K");
    assert_eq!(current(&app), "1 Mix");
    press(&mut app, "hk");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::slot(0, vt_model::settings::Effect::Distort, 4).unwrap()),
        "changing nothing"
    );

    // On a sample track, Machine holds Pitch, Velocity, Gate and Quantise with the scale's pitch,
    // and l opens it on Pitch.
    press(&mut app, "Kgg");
    let at = picker(&app)
        .rows()
        .iter()
        .position(|row| matches!(row, Row::Category(category) if picker(&app).category(*category) == ("Machine", 5, false)))
        .unwrap();
    for _ in 0..at {
        press(&mut app, "j");
    }
    press(&mut app, "l");
    assert_eq!(current(&app), "Pitch");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().tracks[0].mods[3].destination,
        Some(SettingId::PITCH)
    );
}

#[test]
fn dests_window_draws_categories_as_dim_headings_with_counts() {
    let (mut app, _realtime) = app();
    press(&mut app, &format!(" 5L{}$Kjl", to_lfo(0)));
    let mut picker = app.picker.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(100, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..24)
        .map(|y| {
            (0..100)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    let total = picker.items().len();
    assert!(
        text.contains(&format!("Mod 4 destination {total}/{total}")),
        "{text}"
    );
    assert!(text.contains("│• ---"), "the chosen one is marked: {text}");
    assert!(text.contains("│▾ Filter  3"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.contains("│    Cut ") && line.contains(" Filter cutoff")),
        "in its category, with its meaning: {text}"
    );
    assert!(text.contains("▸ Output  "), "{text}");
    let dim = vt_tui::theme::dim().fg;
    let heading = text
        .lines()
        .position(|line| line.contains("▸ Output"))
        .unwrap();
    let column = text.lines().nth(heading).unwrap().find('▸').unwrap();
    let column = text.lines().nth(heading).unwrap()[..column].chars().count();
    assert_eq!(buffer[(column as u16, heading as u16)].fg, dim.unwrap());
}

#[test]
fn a_plain_list_opens_on_the_list_and_typing_searches() {
    // `K` then typing on Machine, and `/` goes to the search.
    let (mut app, _realtime) = app();
    press(&mut app, " 5Kpl");
    assert_eq!(app.picker.as_ref().unwrap().query, "pl");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal, "Esc clears the search, then closes");
    press(&mut app, "K/j");
    assert_eq!(app.picker.as_ref().unwrap().query, "j");
}

fn lane_of(app: &App, slot: usize) -> Option<vt_model::song::Lane> {
    app.song().tracks[0].phrases[&0].lane(slot)
}

#[test]
fn lane_makes_an_fx_column_a_lane_and_the_loop_keys_then_set_it() {
    use vt_model::song::Lane;
    let (mut app, _realtime) = app();
    press(&mut app, "llllljj");
    assert_eq!(app.cursor.field, vt_tui::app::Field::FxCommand(0));
    command(&mut app, "lane 3");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 3
        })
    );
    assert!(app.status.contains("FX 1"), "{}", app.status);
    // Now it has a lane, the loop keys set it rather than the phrase.
    press(&mut app, "jjjjge");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 5
        })
    );
    command(&mut app, "len 4");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 2,
            length: 4
        })
    );
    press(&mut app, "kgs");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 5,
            length: 4
        })
    );
    assert_eq!(
        app.song().tracks[0].phrases[&0].len(),
        16,
        "the phrase keeps its loop"
    );
    press(&mut app, "kkkkge");
    assert!(app.status.contains("above its start"), "{}", app.status);
    command(&mut app, "lane");
    assert_eq!(lane_of(&app, 0), None, "following the notes again");
    press(&mut app, "u");
    assert_eq!(
        lane_of(&app, 0),
        Some(Lane {
            start: 5,
            length: 4
        }),
        "one undo each"
    );
}

#[test]
fn the_loop_keys_on_an_fx_column_without_a_lane_set_the_phrases_loop() {
    let (mut app, _realtime) = app();
    press(&mut app, "lllll7jge");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 8);
    command(&mut app, "len 6");
    assert_eq!(app.song().tracks[0].phrases[&0].len(), 6);
    assert_eq!(lane_of(&app, 0), None, "the column still follows the notes");
    press(&mut app, "hhh");
    command(&mut app, "lane 3");
    assert!(app.status.contains("FX column"), "{}", app.status);
    assert_eq!(lane_of(&app, 0), None);
}

#[test]
fn a_lane_shows_its_length_and_where_it_is_playing() {
    let (mut app, _realtime) = app();
    press(&mut app, "llllljrSNA");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "20gg");
    command(&mut app, "lane 3");
    on_the_row_being_edited(&mut app);
    app.transport = Transport::Playing;
    // Place 4 is the notes' row 4, and the lane's row 4 % 3 = 1.
    app.set_playhead(4);
    app.top = 0;
    let buffer = screen(&app);
    let line = |y: u16| -> String { (0..80).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(line(0).contains("FX1:3"), "{}", line(0));
    let all: Vec<String> = (0..24).map(line).collect();
    let lane_row = all
        .iter()
        .find(|l| l.contains(">SNA"))
        .unwrap_or_else(|| panic!("no lane playhead:\n{}", all.join("\n")));
    assert!(lane_row.contains(" 01 "), "{lane_row}");
    let note_row = (0..24)
        .map(line)
        .find(|l| l.contains(" 04 >"))
        .unwrap_or_default();
    assert!(!note_row.is_empty(), "the notes' own playhead is on row 4");
}

#[test]
fn gg_and_g_on_a_column_with_a_lane_go_by_the_lane() {
    let (mut app, _realtime) = app();
    // FX 1's lane is steps 4 to 6 of the phrase's 16.
    press(&mut app, "lllll4j");
    command(&mut app, "lane 3");
    press(&mut app, "G");
    assert_eq!(app.cursor.step, 6, "the lane's last step");
    press(&mut app, "gg");
    assert_eq!(app.cursor.step, 4, "the lane's first");
    press(&mut app, "2G");
    assert_eq!(app.cursor.step, 5, "a count goes by the lane too");
    press(&mut app, "hhG");
    assert_eq!(app.cursor.step, 15, "the notes go by the phrase's loop");
}

#[test]
fn k_on_the_gate_column_chooses_a_gate_and_keeps_its_pulses() {
    let (mut app, _realtime) = app();
    press(&mut app, "lllrMLT3ggK");
    assert_eq!(app.mode, Mode::Picker);
    // `h` is a key in the list, so `/` starts the search.
    press(&mut app, "/hld");
    special(&mut app, KeyCode::Enter);
    let gate = app.song().tracks[0].phrases[&0].steps()[0].gate.unwrap();
    assert_eq!(
        (gate.kind, gate.pulses),
        (vt_model::step::GateKind::Hold, 3)
    );
}

#[test]
fn an_lfos_depth_is_locked_by_its_label_and_reads_in_its_units() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "dep4");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "9F");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("mod4.depth:9F"), "{step}");
    assert_eq!(column_name(&app), "Mod 4: Depth");
    // Fine-tuned to a depth of exactly +40.
    press(&mut app, "l");
    for _ in 0..0xE0 {
        press(&mut app, "=");
    }
    assert_eq!(value_shown(&app).as_deref(), Some("+40"));
    press(&mut app, "l");
    press(&mut app, "i");
    press(&mut app, "ptch");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "87");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    assert_eq!(value_shown(&app).as_deref(), Some("+7 st"));
}

#[test]
fn a_step_locks_the_speed_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "rate");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "04");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("speed:04"), "{step}");
    assert_eq!(column_name(&app), "Machine: Speed");
    assert_eq!(value_shown(&app).as_deref(), Some("1/8"));
}

#[test]
fn k_on_a_gate_lists_gates_and_on_its_length_lists_lengths() {
    use vt_model::step::GateKind;
    let (mut app, _realtime) = app();
    let gate = |app: &App| app.phrase_here().unwrap().steps()[0].gate;
    // On the gate, the gate types.
    press(&mut app, "lllK");
    assert_eq!(app.cursor.field, vt_tui::app::Field::Gate);
    assert_eq!(app.picker.as_ref().unwrap().title, "Gates");
    special(&mut app, KeyCode::Esc);
    assert!(app.picker.is_none());

    // On its length, some lengths, with the track's first and marked on an empty cell.
    press(&mut app, "lK");
    assert_eq!(app.cursor.field, vt_tui::app::Field::GateLength);
    let picker = app.picker.as_ref().expect("the lengths are listed");
    assert_eq!(picker.title, "Lengths");
    let labels: Vec<&str> = picker
        .items()
        .iter()
        .map(|item| item.label.as_str())
        .collect();
    assert_eq!(labels, ["--", "08", "20", "40", "80", "C0", "E6", "FF"]);
    assert_eq!(picker.items()[0].meaning, "The track's Gate, now 100%");
    assert_eq!(
        picker.items()[4].meaning,
        "50%, half, as a 303's gate closes"
    );
    assert_eq!(picker.current().unwrap().label, "--");
    // `Esc` leaves it as it was.
    special(&mut app, KeyCode::Esc);
    assert!(app.picker.is_none());
    assert_eq!(gate(&app), None);

    // Choosing one sets it, in one change, on one pulse of `SGL` for an empty cell.
    let depth = app.history.undo_depth();
    press(&mut app, "K80");
    special(&mut app, KeyCode::Enter);
    let set = gate(&app).expect("a gate to hold the length");
    assert_eq!(
        (set.kind, set.pulses, set.length),
        (GateKind::Single, 1, Some(0x80))
    );
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");

    // Typing filters, and the gate keeps its type and pulses.
    press(&mut app, "hKHLD");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "==lK");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "80");
    press(&mut app, "C0");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.shown().next().unwrap().label, "C0");
    assert!(picker.shown().count() < picker.items().len());
    special(&mut app, KeyCode::Enter);
    let set = gate(&app).unwrap();
    assert_eq!(
        (set.kind, set.pulses, set.length),
        (GateKind::Hold, 3, Some(0xC0))
    );

    // `--` gives it back to the track's `Gate`.
    press(&mut app, "Kgg");
    special(&mut app, KeyCode::Enter);
    assert_eq!(gate(&app).unwrap().length, None);
    assert_eq!(gate(&app).unwrap().kind, GateKind::Hold);
}

#[test]
fn k_chooses_cnd_then_its_condition_from_a_list() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "CND");
    special(&mut app, KeyCode::Enter);
    // Choosing `CND` lists the conditions, with Elektron's names.
    let picker = app.picker.as_ref().expect("the conditions are listed");
    assert_eq!(picker.title, "Conditions");
    assert_eq!(picker.focus, vt_tui::input::picker::Focus::Search);
    press(&mut app, "1:4");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "1:4");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).command, Some(FxCommand::Cnd));
    assert_eq!(slot(&app).coarse(), 0x14);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(
        column_name(&app),
        "Play the row only when condition xx holds"
    );
    assert_eq!(value_shown(&app).as_deref(), Some("1:4, pass 1 of every 4"));

    // `K` on its value lists them again, on its own.
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Conditions");
    assert_eq!(picker.current().unwrap().label, "1:4");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);

    // `=` steps through the list, and `+` to a longer cycle.
    press(&mut app, "=");
    assert_eq!(slot(&app).coarse(), 0x24);
    press(&mut app, "+");
    assert_eq!(slot(&app).coarse(), 0x25);
    press(&mut app, "--");
    assert_eq!(slot(&app).coarse(), 0x44, "back through 1:5 to 4:4");

    // A value that isn't a condition isn't taken.
    press(&mut app, "r09");
    assert_eq!(slot(&app).coarse(), 0x44);
    assert!(app.status.contains("isn't a condition"), "{}", app.status);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "r07");
    assert_eq!(slot(&app).coarse(), 0x07);
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("1ST, the first time round only")
    );

    // A build is in the list too, and `+` moves its pass by four.
    press(&mut app, "K>5");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, ">5");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).coarse(), 0x95);
    assert_eq!(value_shown(&app).as_deref(), Some(">5, from pass 5 on"));
    press(&mut app, "+");
    assert_eq!(slot(&app).coarse(), 0x99);
    press(&mut app, "rA3");
    assert_eq!(value_shown(&app).as_deref(), Some("<3, before pass 3 only"));
}

#[test]
fn space_l_turns_fill_on_and_held_it_goes_back() {
    let (mut app, _realtime) = app();
    press(&mut app, " ");
    hold_for(&mut app, 'l', 100);
    assert!(app.fill, "tapped, it stays on");
    assert_eq!(app.status, "Fill on");
    let backend = ratatui::backend::TestBackend::new(120, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let status: String = (0..120).map(|x| buffer[(x, 0)].symbol()).collect();
    let fill = format!("{} FILL", vt_tui::glyphs::STOP);
    assert!(status.contains(&fill), "{status}");

    press(&mut app, " ");
    hold_for(&mut app, 'l', 100);
    assert!(!app.fill, "tapped again, it's off");

    // Held, it's on until it's let go.
    press(&mut app, " ");
    let now = Instant::now();
    app.key(Key::char('l'), now);
    assert!(app.fill);
    app.key_repeated(Key::char('l'), now + Duration::from_millis(300));
    assert!(app.fill, "a repeat doesn't switch it back");
    app.key_released(Key::char('l'), now + Duration::from_millis(600));
    assert!(!app.fill, "held, it goes back");
    assert!(!app.dirty, "fill isn't part of the song");
}

#[test]
fn k_chooses_acu_then_its_amount_from_a_list() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "ACU");
    special(&mut app, KeyCode::Enter);
    // Choosing `ACU` lists how far it can move the note, up and then down.
    let picker = app.picker.as_ref().expect("the amounts are listed");
    assert_eq!(picker.title, "Accumulate");
    assert_eq!(picker.current().unwrap().label, "+1", "it opens on one up");
    assert_eq!(
        listed(picker.current().unwrap()),
        "+1  01  Up 1 semitone",
        "the scale is off"
    );
    press(&mut app, "-2");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "-2");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).command, Some(FxCommand::Acu));
    assert_eq!(slot(&app).coarse(), 0xFE);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(
        column_name(&app),
        "Move the note on xx scale degrees each time it plays"
    );
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("down 2 semitones each time it plays, wrap on this row")
    );

    // `K` on its value lists them again, on its own.
    press(&mut app, "K");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "-2");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);

    // `=` goes up through `00`, as the amount is signed, and `+` by seven.
    press(&mut app, "==");
    assert_eq!(slot(&app).coarse(), 0x00);
    assert_eq!(value_shown(&app).as_deref(), Some("doesn't move the note"));
    press(&mut app, "=+");
    assert_eq!(slot(&app).coarse(), 0x08);
    press(&mut app, "+++++");
    assert_eq!(slot(&app).coarse(), 0x1C, "it stops at 28");

    // A value past 28 isn't taken.
    press(&mut app, "r1D");
    assert_eq!(slot(&app).coarse(), 0x1C);
    assert!(app.status.contains("is past 28"), "{}", app.status);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "rE4");
    assert_eq!(slot(&app).coarse(), 0xE4);

    // With the scale on, it's degrees.
    command(&mut app, "set scale=on");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("down 28 scale degrees each time it plays, wrap on this row")
    );
}

#[test]
fn the_row_playing_shows_and_says_the_note_an_accumulator_moved_it_to() {
    use vt_tui::views::status::value_shown;
    let rows = vec!["C-3 -- acu:02".parse().unwrap(), "D-3 --".parse().unwrap()];
    let (mut app, mut rt) = recording_app(rows);
    let row_text = |app: &App, row: u16| {
        let buffer = screen(app);
        (0..40)
            .map(|x| buffer[(x, row + 2)].symbol())
            .collect::<String>()
    };
    let mut at = play_from_the_top(&mut app, &mut rt);
    // The first time round, it plays as written.
    play_to(&mut app, &mut rt, &mut at, 0.5);
    assert!(row_text(&app, 0).contains("C-3"), "{}", row_text(&app, 0));
    assert_eq!(value_shown(&app), None);
    // The second time, two semitones up, as the scale is off.
    play_to(&mut app, &mut rt, &mut at, 2.5);
    assert!(row_text(&app, 0).contains(">"), "{}", row_text(&app, 0));
    assert!(row_text(&app, 0).contains("D-3"), "{}", row_text(&app, 0));
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the row playing is heard as D-3")
    );
    // The row after, which has none, plays as written.
    play_to(&mut app, &mut rt, &mut at, 3.5);
    assert!(row_text(&app, 1).contains("D-3"), "{}", row_text(&app, 1));
    assert_eq!(value_shown(&app), None);
}

#[test]
fn k_chooses_arp_and_its_value_is_typed_as_two_digits() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "ARP");
    let picker = app.picker.as_ref().expect("the commands are listed");
    assert_eq!(
        listed(picker.current().unwrap()),
        "ARP  Arpeggio: the note, x up and y up"
    );
    special(&mut app, KeyCode::Enter);
    // As `RET`'s, its two digits are typed, and nothing goes in until they are.
    assert_eq!(app.mode, Mode::Replace);
    assert_eq!(slot(&app).command, None);
    let depth = app.history.undo_depth();
    press(&mut app, "47");
    assert_eq!(slot(&app).command, Some(FxCommand::Arp));
    assert_eq!(slot(&app).coarse(), 0x47);
    assert_eq!(app.history.undo_depth(), depth + 1, "one change");
    assert_eq!(column_name(&app), "Arpeggio: the note, x up and y up");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note, 4 and 7 semitones up, Up at Tick, 1 octave, legato")
    );

    // `=` and `-` move the second note and `+` and `_` the first, neither carrying into the other.
    press(&mut app, "===");
    assert_eq!(slot(&app).coarse(), 0x4A);
    press(&mut app, "______");
    assert_eq!(slot(&app).coarse(), 0x0A, "it stops at 0");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note and 10 semitones up, Up at Tick, 1 octave, legato")
    );
    press(&mut app, "r00");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("only the note's octaves, Up at Tick, 1 octave, legato")
    );

    // With the scale on, they're degrees.
    press(&mut app, "r24");
    command(&mut app, "set scale=on");
    assert_eq!(
        value_shown(&app).as_deref(),
        Some("the note, 2 and 4 scale degrees up, Up at Tick, 1 octave, legato")
    );
}

#[test]
fn order_plays_the_phrases_rows_in_an_order_from_a_word_or_a_list() {
    use vt_model::order::Order;
    use vt_tui::input::command::Line;
    let (mut app, _realtime) = app();
    let order = |app: &App| app.phrase_here().unwrap().order();
    let heading = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 0)].symbol()).collect()
    };
    assert!(!heading(&app).contains("Forward"), "forward isn't named");
    command(&mut app, "order up-down");
    assert_eq!(order(&app), Order::UpDown);
    assert!(app.status.contains("the ends once"), "{}", app.status);
    assert!(heading(&app).contains("Up-down"), "{}", heading(&app));
    command(&mut app, "order pendulum");
    assert_eq!(order(&app), Order::UpDown, "the old names are gone");
    assert!(app.status.contains(":order takes"), "{}", app.status);
    command(&mut app, "order sideways");
    assert_eq!(order(&app), Order::UpDown);
    assert!(app.status.contains(":order takes"), "{}", app.status);
    // With nothing after it, a list opens on the order it plays in.
    command(&mut app, "order");
    let picker = app.picker.as_ref().expect("the list is open");
    assert_eq!(
        picker.current().map(|item| item.pick.clone()),
        Some(Pick::Line(Line::Order(Some(Order::UpDown))))
    );
    assert_eq!(picker.shown().count(), Order::ALL.len());
    press(&mut app, "shuf");
    special(&mut app, KeyCode::Enter);
    assert_eq!(order(&app), Order::Shuffle);
    assert_eq!(
        app.song().tracks[1].phrases[&0].order(),
        Order::Forward,
        "only this phrase's"
    );
    press(&mut app, "u");
    assert_eq!(order(&app), Order::UpDown);
    press(&mut app, "u");
    assert_eq!(order(&app), Order::Forward);
}

#[test]
fn order_takes_a_snake_and_cartesian_numbers_and_the_title_shows_them() {
    use vt_model::order::{Cartesian, Order, Snake};
    use vt_tui::input::command::Line;
    let (mut app, _realtime) = app();
    let order = |app: &App| app.phrase_here().unwrap().order();
    let heading = |app: &App| -> String {
        let buffer = screen(app);
        (0..80).map(|x| buffer[(x, 0)].symbol()).collect()
    };
    command(&mut app, "order snake spiral");
    assert_eq!(order(&app), Order::Snake(Snake::Spiral));
    assert!(heading(&app).contains("Snake spiral"), "{}", heading(&app));
    assert!(app.status.contains("Round the edge"), "{}", app.status);
    command(&mut app, "order snake 2");
    assert_eq!(order(&app), Order::Snake(Snake::Columns));
    command(&mut app, "order cartesian 1 3 4 2");
    let cartesian = Order::Cartesian(Cartesian::new(1, 3).sized(4, 2));
    assert_eq!(order(&app), cartesian);
    assert!(
        heading(&app).contains("Cartesian 1 3 4 2"),
        "{}",
        heading(&app)
    );
    assert!(
        app.status
            .contains("X moves every row over 4, Y every 3 rows over 2"),
        "{}",
        app.status
    );
    command(&mut app, "order cartesian 0 3");
    assert_eq!(order(&app), cartesian, "a rate of 0 is refused");
    assert!(app.status.contains(":order takes"), "{}", app.status);
    // The list shows the phrase's own Cartesian numbers, and opens on them.
    command(&mut app, "order");
    let picker = app.picker.as_ref().expect("the list is open");
    assert_eq!(
        picker.current().map(|item| item.pick.clone()),
        Some(Pick::Line(Line::Order(Some(cartesian))))
    );
    assert_eq!(picker.shown().count(), Order::ALL.len());
}

#[test]
fn playing_from_a_row_starts_at_its_step_with_the_walk_where_it_is_then() {
    let (mut app, _realtime) = app();
    command(&mut app, "len 4");
    command(&mut app, "order reverse");
    // A row's step is where it's written, whatever the order, and the walk plays what it plays
    // there, as it would playing up to it: in reverse, the bottom row first.
    assert_eq!(app.section_step(), 0);
    press(&mut app, "j");
    assert_eq!(app.section_step(), 1);
}

#[test]
fn k_lists_a_slots_bypass_with_its_effect_and_a_step_locks_it() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, " 5LLK");
    press(&mut app, "delay");
    special(&mut app, KeyCode::Enter);
    press(&mut app, " 3");
    assert_eq!(app.view, View::Phrase);
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "1byp");
    let picker = app.picker.as_ref().expect("the commands are listed");
    let item = picker.current().unwrap();
    assert_eq!(listed(item), "1BYP  1 Bypass");
    assert_eq!(item.category, "FX 1 Delay", "with the slot's effect");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Replace);
    press(&mut app, "01");
    let bypass = SettingId::bypass(0).unwrap();
    assert_eq!(slot(&app).command, Some(FxCommand::Set(bypass)));
    assert_eq!(slot(&app).coarse(), 0x01);
    assert_eq!(column_name(&app), "FX 1: Bypass");
    assert_eq!(value_shown(&app).as_deref(), Some("Bypassed"));
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("1.bypass:01"), "{step}");
    // It stops at the last choice, and back down is the slot heard.
    press(&mut app, "=");
    assert_eq!(slot(&app).coarse(), 0x01);
    press(&mut app, "-");
    assert_eq!(value_shown(&app).as_deref(), Some("In"));
    // Typed by its label too, and an LFO or envelope can't move it.
    let track = &app.song().tracks[0];
    assert_eq!(
        FxCommand::typed("1byp", track),
        Some(FxCommand::Set(bypass))
    );
    assert!(!SettingId::for_mod(track, 3).contains(&bypass));
    assert!(!SettingId::for_mod(track, 0).contains(&bypass));
    // With nothing in a slot, there's nothing to bypass.
    assert!(FxCommand::on_track(track).contains(&FxCommand::Set(bypass)));
    let empty = FxCommand::Set(SettingId::bypass(1).unwrap());
    assert!(!FxCommand::on_track(track).contains(&empty));
}

#[test]
fn a_step_locks_the_glide_by_its_label() {
    use vt_tui::views::status::{column_name, value_shown};
    let (mut app, _realtime) = app();
    press(&mut app, "llllli");
    press(&mut app, "gld");
    special(&mut app, KeyCode::Enter);
    press(&mut app, "80");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "k");
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("glide:80"), "{step}");
    assert_eq!(column_name(&app), "Machine: Glide");
    assert_eq!(value_shown(&app).as_deref(), Some("63 ms"));
}
