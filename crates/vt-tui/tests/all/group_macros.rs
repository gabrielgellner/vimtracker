//! A group track's macros on the tracks of its group (decision 408): `M` on a track in a group
//! lists the group's macros after its own and maps the setting there by the track's place, the
//! group's mapping view names the track each mapping is on, and a setting the group holds refuses
//! a turn of its own and `gd` goes to the group's macro.

use ratatui::crossterm::event::KeyCode;
use vt_model::macros::{MACROS, MacroTarget};
use vt_model::settings::SettingId;
use vt_model::song::{Song, TrackKind};
use vt_tui::app::App;
use vt_tui::controls::Control;
use vt_tui::input::mode::{Mode, View};

use crate::common::*;

fn cut() -> MacroTarget {
    MacroTarget::Setting(SettingId::CUT)
}

/// Three named tracks, Kick and Snare in group 1, on Snare's `Cut`, and the group's track.
fn on_snares_cut() -> (App, vt_engine::link::Realtime, usize) {
    let mut song = Song::new(3);
    for (at, name) in ["Kick", "Snare", "Hats"].into_iter().enumerate() {
        song.tracks[at].name = name.to_owned();
    }
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    let (mut app, realtime) = app_for(song);
    app.services.cursor.track = 1;
    press(&mut app, " 5}j");
    assert_eq!(app.modes.view, View::Track);
    assert_eq!(
        app.track_control(),
        Control::Base(vt_model::action::TrackControl::Cutoff)
    );
    (app, realtime, group)
}

#[test]
fn m_on_a_track_in_a_group_lists_the_groups_macros_after_its_own() {
    let (mut app, _realtime, group) = on_snares_cut();
    press(&mut app, "M");
    assert_eq!(app.modes.mode, Mode::Picker);
    let items = app.overlays.picker.as_ref().unwrap().items();
    assert_eq!(items.len(), 2 * MACROS);
    assert_eq!(items[MACROS + 1].label, "G1 2 Macro 2");
    // The second of the group's.
    press(&mut app, &"j".repeat(MACROS + 1));
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.modes.mode, Mode::Normal);
    let song = app.song();
    assert!(song.tracks[1].macro_holding(cut()).is_none(), "not its own");
    let (at, map) = song.group_holding(1, cut()).unwrap();
    assert_eq!(
        (at, map.member),
        (1, Some(1)),
        "the group's, by Snare's place"
    );
    assert_eq!(song.tracks[group].macros[1].maps.len(), 1);
    assert!(
        app.services.status.contains("G1's macro 2"),
        "{}",
        app.services.status
    );
    // It refuses a turn of its own, saying where it's turned.
    press(&mut app, "=");
    assert!(
        app.services.status.contains("on G1's macro 2"),
        "{}",
        app.services.status
    );
    // And `M` again opens on it, and Enter there takes it off.
    press(&mut app, "M");
    special(&mut app, KeyCode::Enter);
    assert!(app.song().group_holding(1, cut()).is_none());
    // Put on its own macro 3, then moved to the group's 1, it comes off its own.
    press(&mut app, "M3");
    assert_eq!(app.song().tracks[1].macro_holding(cut()).unwrap().0, 2);
    press(&mut app, "M");
    press(&mut app, &"j".repeat(MACROS - 2));
    special(&mut app, KeyCode::Enter);
    let song = app.song();
    assert!(song.tracks[1].macro_holding(cut()).is_none());
    assert_eq!(song.group_holding(1, cut()).unwrap().0, 0);
    assert!(
        app.services.status.contains("moved"),
        "{}",
        app.services.status
    );
}

#[test]
fn the_groups_mapping_view_names_the_track_each_mapping_is_on() {
    let (mut app, _realtime, group) = on_snares_cut();
    press(&mut app, "M");
    press(&mut app, &"j".repeat(MACROS));
    special(&mut app, KeyCode::Enter);
    // `gd` goes to the group's macro 1, on the group's track, and `gm` there opens it.
    press(&mut app, "gd");
    assert_eq!(app.services.cursor.track, group);
    assert_eq!(app.track_control(), Control::Macro(0));
    press(&mut app, "gm");
    assert_eq!(app.modes.mode, Mode::Mapping);
    let backend = ratatui::backend::TestBackend::new(150, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let view = app.overlays.mapping.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::overlays::mapping::draw::draw(frame, frame.area(), &app.ctx(), &view))
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
    assert!(text.contains("Snare Cut"), "{text}");
}
