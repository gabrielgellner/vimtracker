//! Macros on the track page (decision 285): `M` maps the setting under the cursor to a macro,
//! moves it or takes it off, `gm` opens a macro's mapping and edits its ranges, `:macro` names
//! one, a setting a macro holds refuses a turn and shows where the macro puts it with its range
//! drawn on its bar, and the Macros band turns them.

use ratatui::crossterm::event::KeyCode;
use vt_model::macros::MacroTarget;
use vt_model::settings::SettingId;
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};
use vt_tui::track_page::Control;

mod common;
use common::*;

/// On `Cut`, the Filter's first setting.
fn on_cut() -> (App, vt_engine::link::Realtime) {
    let (mut app, realtime) = tracks_of_different_lengths();
    press(&mut app, " 5}j");
    assert_eq!(app.view, View::Track);
    assert_eq!(
        app.track_control(),
        Control::Base(vt_model::action::TrackControl::Cutoff)
    );
    (app, realtime)
}

/// The track page as lines of text, as big as it is on the author's screen.
fn page_text(app: &App) -> Vec<String> {
    let (width, height) = (150, 40);
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn cut() -> MacroTarget {
    MacroTarget::Setting(SettingId::CUT)
}

#[test]
fn m_and_a_digit_map_a_setting_to_a_macro_turned_to_where_it_is() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "i40");
    press(&mut app, "M");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().items().len(), 8);
    press(&mut app, "5");
    assert_eq!(app.mode, Mode::Normal);
    let track = &app.song().tracks[0];
    let (at, map) = track.macro_holding(cut()).unwrap();
    assert_eq!(at, 4);
    assert_eq!(
        (map.min, map.max),
        (0, 0xFF00),
        "its whole range to start with"
    );
    assert_eq!(
        track.macros[4].value >> 8,
        0x40,
        "turned to where Cut was, so it doesn't jump"
    );
    assert_eq!(track.macros[4].shown_name().as_deref(), Some("Cut"));
    assert!(app.status.contains("macro 5"), "{}", app.status);
    // One undo step takes it all back.
    press(&mut app, "u");
    assert!(app.song().tracks[0].macro_holding(cut()).is_none());
    assert_eq!(app.song().tracks[0].macros[4].value, 0);
}

#[test]
fn a_mapped_setting_refuses_a_turn_and_says_where_to_turn_it() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "M3");
    let before = app.song().clone();
    for keys in ["=", "-", "+", "x", "i"] {
        press(&mut app, keys);
        assert_eq!(app.mode, Mode::Normal, "{keys}");
        assert_eq!(
            app.song().tracks[0].cutoff,
            before.tracks[0].cutoff,
            "{keys}"
        );
        assert!(
            app.status.contains("is on macro 3"),
            "{keys}: {}",
            app.status
        );
    }
}

#[test]
fn m_on_a_mapped_setting_takes_it_off_its_macro_or_moves_it_to_another() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "M2");
    press(&mut app, "M");
    assert_eq!(
        app.picker.as_ref().unwrap().selected,
        1,
        "opened on its macro"
    );
    press(&mut app, "7");
    assert_eq!(app.song().tracks[0].macro_holding(cut()).unwrap().0, 6);
    assert!(app.song().tracks[0].macros[1].maps.is_empty());
    press(&mut app, "M7");
    assert!(app.song().tracks[0].macro_holding(cut()).is_none());
    assert!(app.status.contains("off macro 7"), "{}", app.status);
}

#[test]
fn gm_opens_the_mapping_and_moves_swaps_types_and_unmaps_its_ends() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "M1");
    // Res, beside it, on the same macro.
    press(&mut app, "jM1");
    press(&mut app, "kgm");
    assert_eq!(app.mode, Mode::Mapping);
    let map = |app: &App, target| *app.song().tracks[0].macro_holding(target).unwrap().1;
    // On Cut's Min: up a large step and one, then to Max and type it.
    press(&mut app, "+=");
    assert_eq!(map(&app, cut()).min, 0x1100);
    press(&mut app, "li");
    press(&mut app, "A0");
    assert_eq!(map(&app, cut()).max, 0xA000);
    press(&mut app, "~");
    assert_eq!(
        (map(&app, cut()).min, map(&app, cut()).max),
        (0xA000, 0x1100)
    );
    assert!(
        app.status.contains("At 00 the macro sets Cut"),
        "{}",
        app.status
    );
    // Down to Res, and off the macro, and back with u.
    let res = MacroTarget::Setting(SettingId::RES);
    press(&mut app, "jdd");
    assert!(app.song().tracks[0].macro_holding(res).is_none());
    assert_eq!(app.mode, Mode::Mapping, "Cut is still on it");
    press(&mut app, "u");
    assert!(app.song().tracks[0].macro_holding(res).is_some());
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    // gm on a setting on no macro says so.
    press(&mut app, "jjgm");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("isn't on a macro"), "{}", app.status);
}

#[test]
fn the_page_shows_where_the_macro_puts_a_setting_and_its_range_on_its_bar() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "i40M1gm");
    press(&mut app, "i20li");
    press(&mut app, "C0");
    special(&mut app, KeyCode::Esc);
    // The macro at 40 of its 20 to C0: 20 + A0 * 40 / FF, about 48.
    let shown = |app: &App| {
        page_text(app)
            .into_iter()
            .find(|line| line.contains(" Cut "))
            .unwrap()
    };
    let line = shown(&app);
    assert!(
        line.contains("Cut       48"),
        "{line}\n{}",
        page_text(&app).join("\n")
    );
    assert!(line.contains('░'), "the range is drawn: {line}");
    let backend = ratatui::backend::TestBackend::new(163, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::status::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let status: String = (0..163).map(|x| buffer[(x, 0)].symbol()).collect();
    assert!(status.contains("on macro 1 Cut"), "{status}");
}

#[test]
fn the_macros_band_turns_a_macro_and_colon_macro_names_it() {
    let (mut app, _realtime) = on_cut();
    press(&mut app, "M1");
    // The Gen page, its first band the macros.
    press(&mut app, "LLL");
    assert_eq!(app.track_page.page, vt_tui::track_page::Page::Gen);
    press(&mut app, "gg0");
    assert_eq!(app.track_control(), Control::Macro(0));
    press(&mut app, "iFF");
    assert_eq!(app.song().tracks[0].macros[0].value, 0xFF00);
    press(&mut app, "-");
    assert_eq!(app.song().tracks[0].macros[0].value, 0xFE00);
    command(&mut app, "macro Bite");
    assert_eq!(app.song().tracks[0].macros[0].name.as_deref(), Some("Bite"));
    let text = page_text(&app).join("\n");
    assert!(text.contains("Bite"), "{text}");
    assert!(text.contains("Macros"), "{text}");
    assert!(text.contains("Bite → Cut"), "what it holds: {text}");
    command(&mut app, "macro");
    assert_eq!(app.song().tracks[0].macros[0].name, None);
    // gm on the band opens its mapping, and M refuses a macro.
    press(&mut app, "M");
    assert!(app.status.contains("can't hold a macro"), "{}", app.status);
    press(&mut app, "gm");
    assert_eq!(app.mode, Mode::Mapping);
}

/// Eight empty rows with a tresillo of notes kept live on them, then the Gen page, on its `Hits`.
fn on_hits() -> (App, vt_engine::link::Realtime) {
    use std::sync::Arc;
    let mut song = vt_model::song::Song::new(1);
    let steps: Vec<vt_model::step::Step> = (0..8)
        .map(|_| "--- -- --- --- ---".parse().unwrap())
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(vt_model::song::Phrase::from_steps(steps)));
    let samples = Arc::new(vt_model::samples::SamplePool::default());
    let version = Arc::new(vt_engine::published::Published::new(
        Arc::new(song.clone()),
        Arc::clone(&samples),
    ));
    let (handle, realtime) =
        vt_engine::link::connect(vt_engine::engine::Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    press(&mut app, "vGSjjjjjj===");
    special(&mut app, KeyCode::Char('L'));
    press(&mut app, " 4LLL}kjj$");
    assert!(matches!(
        app.track_control(),
        Control::Gen {
            param: vt_model::shape::GenParam::Hits,
            ..
        }
    ));
    (app, realtime)
}

/// How many of the phrase's rows have a note.
fn notes(app: &App) -> usize {
    app.song().tracks[0].phrases[&0]
        .steps()
        .iter()
        .filter(|step| matches!(step.note, vt_model::step::Note::On(_)))
        .count()
}

#[test]
fn m_maps_a_generators_hits_and_turning_the_macro_writes_the_column_again() {
    let (mut app, _realtime) = on_hits();
    assert_eq!(notes(&app), 3);
    press(&mut app, "M2");
    let target = MacroTarget::Gen {
        column: vt_model::macros::GenColumn::Note,
        param: vt_model::shape::GenParam::Hits,
    };
    let (at, map) = app.song().tracks[0].macro_holding(target).unwrap();
    let map = *map;
    assert_eq!(at, 1);
    assert_eq!(
        (map.number_at(0.0), map.number_at(255.0)),
        (0, 8),
        "its bounds"
    );
    assert_eq!(
        notes(&app),
        3,
        "the macro turned to where Hits is, so nothing jumps"
    );
    // Its own turn is refused, and the band turns it.
    press(&mut app, "=");
    assert!(app.status.contains("is on macro 2"), "{}", app.status);
    press(&mut app, "gg0l");
    assert_eq!(app.track_control(), Control::Macro(1));
    press(&mut app, "iFF");
    assert_eq!(notes(&app), 8);
    press(&mut app, "i00");
    assert_eq!(notes(&app), 0);
    // The generator keeps its own underneath.
    let generator = &app.song().tracks[0].phrases[&0].generators()[0];
    assert_eq!(
        generator.shape.place,
        vt_model::shape::Where::Euclid { hits: 3, rotate: 0 }
    );
    // Its mapping says it's by hand.
    press(&mut app, "gm");
    assert!(app.status.contains("by hand"), "{}", app.status);
    let backend = ratatui::backend::TestBackend::new(150, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let view = app.mapping.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::views::mapping::draw(frame, frame.area(), &app, &view))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..40)
        .map(|y| {
            (0..150)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("Note Hits"), "{text}");
    assert!(text.contains("by hand"), "{text}");
}

#[test]
fn the_roto_shows_the_macros_first_each_knob_named_by_its_macro_and_turns_them() {
    use vt_model::action::Action;
    use vt_tui::roto::devices::live_devices;
    use vt_tui::roto::edits::turned;
    let (mut app, _realtime) = on_cut();
    press(&mut app, "M3");
    command(&mut app, "macro A long macro name");
    assert!(app.status.contains("12"), "{}", app.status);
    command(&mut app, "macro Twelve chars");
    let song = app.song().clone();
    let devices = live_devices(&song.tracks[0], None, &song, false);
    assert_eq!(devices[0].name.as_str(), "Macros");
    assert_eq!(
        devices[1].name.as_str(),
        "Sampler",
        "the machine after them"
    );
    let names: Vec<&str> = (1..=8)
        .map(|knob| devices[0].control(knob).unwrap().name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Macro 1",
            "Macro 2",
            "Twelve chars",
            "Macro 4",
            "Macro 5",
            "Macro 6",
            "Macro 7",
            "Macro 8"
        ]
    );
    // A knob turned to its bottom turns its macro to `00`, one action: it's at `FF`, where Cut
    // was as it was mapped.
    assert_eq!(song.tracks[0].macros[2].value, 0xFF00);
    let action = turned(Control::Macro(2), 0, &song, 0).unwrap();
    assert_eq!(
        action,
        Action::SetMacro {
            track: 0,
            at: 2,
            value: 0
        }
    );
}
