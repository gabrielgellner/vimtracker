//! The track's `Phrase` settings on the Gen page (decision 400): `Order`, `Reverse`, `Order X`
//! and `Order Y`, which move the walk of every phrase the track plays, and the title and playhead
//! that follow them.

use vt_engine::engine::Transport;
use vt_engine::link::Realtime;
use vt_model::action::TrackControl;
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::Mode;
use vt_tui::input::mode::View;

use crate::common::*;

const ROWS: [&str; 8] = [
    "C-3 40 --- --- ---",
    "D-3 40 --- --- ---",
    "E-3 40 --- --- ---",
    "F-3 40 --- --- ---",
    "G-3 40 --- --- ---",
    "A-3 40 --- --- ---",
    "B-3 40 --- --- ---",
    "C-4 40 --- --- ---",
];

/// The Gen page of a phrase with no live columns, on `Phrase`'s `Order`.
fn on_phrase_device() -> (App, Realtime) {
    let (mut app, realtime) = app_with(&ROWS);
    keys(&mut app, " 8gg");
    assert_eq!(app.modes.view, View::Gen);
    assert_eq!(app.track_control(), Control::Base(TrackControl::Order));
    (app, realtime)
}

/// The phrase view's top line.
fn title(app: &App) -> String {
    let buffer = screen(app);
    (0..100).map(|x| buffer[(x, 0)].symbol()).collect()
}

#[test]
fn order_turns_through_the_list_and_the_title_says_what_plays() {
    let (mut app, _realtime) = on_phrase_device();
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Phrase · here Forward, as written"), "{text}");
    keys(&mut app, "==");
    assert_eq!(app.song().tracks[0].order, 0x82);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("· here Up-down, its own Forward"), "{text}");
    let line = status_line(&app);
    assert!(
        line.contains("+2 on: each phrase plays in the order 2 places after its own"),
        "{line}"
    );
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].order, 0x81, "one undo step a press");
    keys(&mut app, "x");
    assert_eq!(app.song().tracks[0].order, 0x80);
    keys(&mut app, "i-1<Enter>");
    assert_eq!(app.song().tracks[0].order, 0x7F, "typed in places");
    keys(&mut app, "x");
    // `Reverse` is a switch.
    keys(&mut app, "l");
    assert_eq!(app.track_control(), Control::Base(TrackControl::Reverse));
    keys(&mut app, "=");
    assert_eq!(app.song().tracks[0].reverse, 1);
    let text = fx_page_text(&app).join("\n");
    assert!(
        text.contains("· here Forward reversed, its own Forward"),
        "{text}"
    );
    keys(&mut app, "-");
    assert_eq!(app.song().tracks[0].reverse, 0);
    // `Order X` lists `Own` and 1 to 16.
    keys(&mut app, "l");
    assert_eq!(app.track_control(), Control::Base(TrackControl::OrderX));
    keys(&mut app, "K");
    assert_eq!(app.modes.mode, Mode::Picker);
    let picker = app.overlays.picker.as_ref().unwrap();
    assert_eq!(picker.items().len(), 17);
    assert_eq!(picker.current().unwrap().label, "Own");
}

#[test]
fn m_maps_the_order_to_a_macro() {
    let (mut app, _realtime) = on_phrase_device();
    keys(&mut app, "M1");
    let order = vt_model::macros::MacroTarget::Setting(vt_model::settings::SettingId::ORDER);
    let track = &app.song().tracks[0];
    assert_eq!(track.macro_holding(order).map(|(at, _)| at), Some(0));
}

#[test]
fn the_phrase_title_and_playhead_follow_the_walk_the_track_plays() {
    let (mut app, _realtime) = app_with(&ROWS);
    let moved = vt_model::action::Action::SetTrackBase {
        track: 0,
        control: TrackControl::Order,
        value: 0x81,
    };
    app.services.apply(&moved);
    on_the_row_being_edited(&mut app);
    assert!(title(&app).contains("Reverse (Forward)"), "{}", title(&app));
    // One step in, reverse plays the seventh row.
    app.services.reported.transport = Transport::Playing;
    app.services.reported.jam[0].chain = Some(0);
    app.set_playhead(1);
    app.phrase_view.top = 0;
    let buffer = screen(&app);
    let lines: Vec<String> = (0..24)
        .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let playing = lines
        .iter()
        .find(|line| line.contains(">B-3"))
        .unwrap_or_else(|| panic!("no playhead on the seventh row:\n{}", lines.join("\n")));
    assert!(playing.contains("06"), "{playing}");
}

#[test]
fn length_and_start_set_the_window_and_the_titles_say_it() {
    let (mut app, _realtime) = on_phrase_device();
    keys(&mut app, "j");
    assert_eq!(
        app.track_control(),
        Control::Base(TrackControl::PhraseLength)
    );
    keys(&mut app, "i-4<Enter>");
    assert_eq!(app.song().tracks[0].phrase_length, 0x7C, "typed in rows");
    let line = status_line(&app);
    assert!(
        line.contains("-4 rows: each phrase's order walks over 4 fewer of its rows"),
        "{line}"
    );
    keys(&mut app, "l");
    assert_eq!(
        app.track_control(),
        Control::Base(TrackControl::PhraseStart)
    );
    keys(&mut app, "==");
    assert_eq!(app.song().tracks[0].phrase_start, 2);
    let text = fx_page_text(&app).join("\n");
    assert!(
        text.contains("· here Forward, 4 rows from 3, its own Forward"),
        "{text}"
    );
    on_the_row_being_edited(&mut app);
    assert!(
        title(&app).contains("Forward, 4 rows from 3 (Forward)"),
        "{}",
        title(&app)
    );
}

#[test]
fn cue_is_a_switch_beside_the_window() {
    let (mut app, _realtime) = on_phrase_device();
    keys(&mut app, "jll");
    assert_eq!(app.track_control(), Control::Base(TrackControl::Cue));
    let line = status_line(&app);
    assert!(
        line.contains("Off: a new length, start or lane rate is heard at once"),
        "{line}"
    );
    keys(&mut app, "=");
    assert_eq!(app.song().tracks[0].cue, 1);
    let line = status_line(&app);
    assert!(
        line.contains("On: a new length, start or lane rate waits for its loop to come round"),
        "{line}"
    );
    keys(&mut app, "-");
    assert_eq!(app.song().tracks[0].cue, 0);
}

#[test]
fn the_chains_order_and_jump_sit_at_the_foot_of_phrase() {
    let (mut app, _realtime) = on_phrase_device();
    keys(&mut app, "jj");
    assert_eq!(app.track_control(), Control::Base(TrackControl::ChainOrder));
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Chain"), "{text}");
    keys(&mut app, "=");
    assert_eq!(app.song().tracks[0].chain_order, 1);
    let line = status_line(&app);
    assert!(line.contains("Reverse: the chain's phrases"), "{line}");
    // Stepping stops at the list's end.
    for _ in 0..40 {
        keys(&mut app, "=");
    }
    assert_eq!(app.song().tracks[0].chain_order, 27);
    keys(&mut app, "l");
    assert_eq!(app.track_control(), Control::Base(TrackControl::ChainJump));
    keys(&mut app, "=");
    assert!(app.song().tracks[0].chain_jump > 0);
}
