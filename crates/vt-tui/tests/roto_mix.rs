//! The Roto-Control's MIX mode as vimtracker's mixer, with a fake Roto in place of the real one:
//! Level on the knobs and mutes on the buttons, SEL's Pan, sends and solos, FUNC's track focus,
//! the banks, the returns and the master, the meters, the transport, and the keys' edits sent
//! back.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::action::{Action, TrackControl};
use vt_model::constants::{MAX_GROUPS, MAX_LEVEL, MAX_SOUND_TRACKS};
use vt_model::samples::SamplePool;
use vt_model::song::Song;
use vt_roto::cc::{ControlEvent, Side};
use vt_roto::fake::{FakePorts, FakeRoto};
use vt_roto::link::Options;
use vt_roto::live::KNOB_TOP;
use vt_roto::serial::Mode;
use vt_roto::sysex::{AllTracks, FromRoto, KnobMode, SwitchMode, ToRoto};
use vt_tui::app::App;
use vt_tui::roto::RotoMode;
use vt_tui::roto::devices::knob;
use vt_tui::roto::mix::{
    LEVEL_NOTCH, METER_RED_DB, METER_YELLOW_DB, level_at, level_position, meter_level,
};
use vt_tui::track_page::Control;

use common::{block, press};

/// An app with this many sound tracks whose Roto is a fake one in MIX mode, plugged in and
/// connected, which the link looks for often.
fn mixing(tracks: usize) -> (App, Realtime, FakePorts) {
    let song = Song::new(tracks);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let mut fake = FakeRoto::new();
    fake.mode = Mode::Mix;
    let ports = FakePorts::new(fake, true);
    let shared = ports.clone();
    app.roto.use_ports(move || Box::new(shared.clone()));
    app.roto.options = Options {
        watch_every: Duration::from_millis(10),
        sysex_gap: Duration::ZERO,
        ..Options::default()
    };
    app.roto.gesture_pause = Duration::ZERO;
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .meter_thresholds
        .is_some()));
    (app, realtime, ports)
}

/// The Roto's own first view of MIX mode, as MODE chooses it: Level and mutes on the tracks.
const LEVELS: FromRoto = FromRoto::SetMixAllTracksMode {
    tracks: AllTracks::Audio,
    knobs: KnobMode::Level,
    switches: SwitchMode::Mute,
    send: 0,
};

/// Ticks the app until the fake Roto has what's wanted, for up to five seconds.
fn tick_until(app: &mut App, ports: &FakePorts, done: impl Fn(&FakeRoto) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.tick();
        if ports.with(|fake| done(fake)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Ticks the app until it's as wanted, for up to five seconds.
fn tick_until_app(app: &mut App, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.tick();
        if done(app) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Runs the audio side a block at a time, ticking the app between, until the engine is as wanted,
/// for up to five seconds. An edit reaches the engine as a new version, which waits behind one
/// sent before it, such as the one a sample's analysis sends when it's done, so a test that hears
/// an edit waits for the engine to have it rather than for a number of blocks.
fn heard_until(app: &mut App, realtime: &mut Realtime, done: impl Fn(&Engine) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        block(realtime, app);
        if done(realtime.engine()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

/// Ticks the app for a while, for what's sent to settle.
fn settle(app: &mut App, for_ms: u64) {
    let until = Instant::now() + Duration::from_millis(for_ms);
    while Instant::now() < until {
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Something done on the Roto that it says by SysEx.
fn roto_says(ports: &FakePorts, message: FromRoto) {
    let bytes = ports.with(|fake| fake.press(message));
    ports.from_roto(vec![bytes]);
}

/// A knob, a button, a touch or a transport button used on the Roto.
fn roto_does(ports: &FakePorts, event: ControlEvent) {
    let ccs = ports.with(|fake| fake.control(event));
    ports.from_roto(ccs);
}

fn press_button(ports: &FakePorts, button: u8) {
    roto_does(
        ports,
        ControlEvent::Switch {
            index: button,
            value: 127,
        },
    );
    roto_does(
        ports,
        ControlEvent::Switch {
            index: button,
            value: 0,
        },
    );
}

/// Where the knob of a setting sits on a track.
fn position(app: &App, control: Control, track: usize) -> u16 {
    if control == Control::Level {
        return level_position(f64::from(app.song().tracks[track].level));
    }
    knob(control, &app.song().tracks[track], app.song())
        .unwrap()
        .position
}

/// Every value the Roto was told to show on a knob, in order.
fn values_shown(fake: &FakeRoto, knob: u8) -> Vec<String> {
    fake.received
        .iter()
        .filter_map(|message| match message {
            ToRoto::ShowControlValue { index, value, .. } if *index == knob => {
                Some(value.as_str().to_owned())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn mix_mode_shows_the_tracks_levels_on_the_knobs_and_mutes_on_the_buttons() {
    let (mut app, _realtime, ports) = mixing(8);
    // The meters turn yellow and red where the app's own do.
    ports.with(|fake| {
        assert_eq!(
            fake.meter_thresholds,
            Some((meter_level(METER_YELLOW_DB), meter_level(METER_RED_DB)))
        );
    });
    app.apply(&Action::SetTrackLevel {
        track: 3,
        level: 0x40,
    });
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    assert_eq!(app.roto.mode, RotoMode::Mix);
    ports.with(|fake| {
        // The sound tracks, then the groups, in banks of eight.
        assert_eq!(fake.tracks.len(), 8);
        assert_eq!(fake.tracks[&0].name.as_str(), "Track 01");
        assert_eq!(fake.tracks[&7].name.as_str(), "Track 08");
        assert!(
            fake.received
                .contains(&ToRoto::NumTracks(8 + MAX_GROUPS as u16))
        );
        assert_eq!(fake.first_track, 0);
        // Two sends, named as the returns are.
        assert_eq!(fake.sends, Some(2));
        assert_eq!(fake.send_names[&0].as_str(), "Return A");
        assert_eq!(fake.send_names[&1].as_str(), "Return B");
        // Every meter lit, every mute out.
        assert_eq!(fake.meter_states, Some([true; 8]));
        for button in 0..8 {
            assert_eq!(fake.light(button), Some(false));
        }
    });
    for track in 0..8 {
        let at = ports.with(|fake| fake.knob_position(track as u8));
        assert_eq!(at, Some(position(&app, Control::Level, track)), "{track}");
    }
}

#[test]
fn a_level_knob_edits_its_own_track_whatever_the_cursor_is_on_one_undo_step_a_touch() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    app.cursor.track = 6;
    let depth = app.history.undo_depth();
    roto_does(
        &ports,
        ControlEvent::Touch {
            index: 2,
            touched: true,
        },
    );
    for value in [9000, 10000, 12000] {
        roto_does(&ports, ControlEvent::Knob { index: 2, value });
        settle(&mut app, 40);
    }
    roto_does(
        &ports,
        ControlEvent::Touch {
            index: 2,
            touched: false,
        },
    );
    assert!(tick_until_app(&mut app, |app| {
        app.history.undo_depth() == depth + 1
            && position(app, Control::Level, 2).abs_diff(12000) < 100
    }));
    settle(&mut app, 50);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    assert_eq!(app.cursor.track, 6, "the cursor stays");
    // Its value, as the app writes it.
    assert!(tick_until(&mut app, &ports, |fake| {
        values_shown(fake, 2)
            .last()
            .is_some_and(|text| text.ends_with("dB"))
    }));
    // A knob it moved isn't sent its own position back.
    let sent = ports.with(|fake| {
        fake.received_ccs
            .iter()
            .filter(|cc| cc[1] == vt_roto::cc::KNOB_LOW + 2)
            .count()
    });
    assert_eq!(sent, 1, "only as MIX mode was shown");
}

#[test]
fn a_mute_button_mutes_its_track_lit_while_muted_and_the_keys_light_it_too() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake.light(7).is_some()));
    press_button(&ports, 3);
    assert!(tick_until_app(&mut app, |app| app.levels[3].mute));
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.light(3) == Some(true)
            && fake
                .meter_states
                .is_some_and(|states| !states[3] && states[2])
    }));
    press_button(&ports, 3);
    assert!(tick_until_app(&mut app, |app| !app.levels[3].mute));
    assert!(tick_until(&mut app, &ports, |fake| fake.light(3) == Some(false)));
    // The keys mute the cursor's track, and the Roto shows it.
    app.cursor.track = 5;
    press(&mut app, " m");
    assert!(app.levels[5].mute, "{}", app.status);
    assert!(tick_until(&mut app, &ports, |fake| fake.light(5) == Some(true)));
}

#[test]
fn sel_puts_pan_or_a_send_on_the_knobs_and_solos_on_the_buttons() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(
        &ports,
        FromRoto::SetMixAllTracksMode {
            tracks: AllTracks::Audio,
            knobs: KnobMode::Pan,
            switches: SwitchMode::Solo,
            send: 0,
        },
    );
    let pan = Control::Base(TrackControl::Pan);
    let centre = position(&app, pan, 0);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.knob_position(0) == Some(centre)
    }));
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 1,
            value: KNOB_TOP,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[1].pan == 0xFF));
    press_button(&ports, 4);
    assert!(tick_until_app(&mut app, |app| app.levels[4].solo));
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.light(4) == Some(true)
            && fake
                .meter_states
                .is_some_and(|states| states[4] && !states[0])
    }));
    // The second send, Send B.
    roto_says(
        &ports,
        FromRoto::SetMixAllTracksMode {
            tracks: AllTracks::Audio,
            knobs: KnobMode::Send,
            switches: SwitchMode::Mute,
            send: 1,
        },
    );
    settle(&mut app, 30);
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 6,
            value: KNOB_TOP,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[6].send_b == MAX_LEVEL));
    assert_eq!(app.song().tracks[6].send_a, 0, "not the first send");
}

#[test]
fn func_focuses_one_track_level_pan_and_sends_and_plugin_mode_shows_it_too() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    // Touching a knob chooses its track, as the Roto says.
    roto_says(&ports, FromRoto::RotoSelectTrack(3));
    roto_says(&ports, FromRoto::SetMixTrackMode(0));
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.selected_track
            .as_ref()
            .is_some_and(|track| track.index == 3)
    }));
    assert_eq!(app.roto.track, 3);
    let level = position(&app, Control::Level, 3);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.sends == Some(2) && fake.knob_position(0) == Some(level)
    }));
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 3,
            value: KNOB_TOP,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[3].send_b == MAX_LEVEL));
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 2,
            value: KNOB_TOP,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[3].send_a == MAX_LEVEL));
    // Its meter is the first, and nothing is on the buttons.
    ports.with(|fake| {
        assert_eq!(
            fake.meter_states,
            Some([true, false, false, false, false, false, false, false])
        );
    });
    press_button(&ports, 0);
    settle(&mut app, 30);
    assert!(!app.levels[3].mute);
    // PLUGIN mode shows the same track's devices.
    roto_says(&ports, FromRoto::SetPluginMode);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    assert_eq!(app.roto.mode, RotoMode::Plugin);
    assert_eq!(app.cursor.track, 0, "the cursor stays");
}

#[test]
fn the_arrows_page_the_banks_to_the_groups_and_both_reach_the_returns_and_master() {
    let (mut app, _realtime, ports) = mixing(12);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    roto_says(&ports, FromRoto::SetFirstTrack(8));
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.first_track == 8 && fake.tracks.contains_key(&15)
    }));
    ports.with(|fake| {
        assert_eq!(fake.tracks[&11].name.as_str(), "Track 12");
        assert_eq!(fake.tracks[&12].name.as_str(), "Group 1");
        assert!(fake.tracks[&12].grouped);
        assert_eq!(fake.tracks[&15].name.as_str(), "Group 4");
    });
    // A group's level, on its knob.
    let group = app.song().sound_tracks();
    roto_does(&ports, ControlEvent::Knob { index: 4, value: 0 });
    assert!(tick_until_app(&mut app, |app| app.song().tracks[group]
        .level
        == 0));
    // Both arrows: the returns and the master.
    roto_says(&ports, FromRoto::SetAllTracksMode(AllTracks::MasterReturn));
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.received.last()
            == Some(&ToRoto::SetVuMeterStates([
                true, true, true, false, false, false, false, false,
            ]))
    }));
    ports.with(|fake| {
        assert!(fake.received.contains(&ToRoto::NumTracks(3)));
        assert_eq!(fake.first_track, 0);
        assert_eq!(fake.tracks[&0].name.as_str(), "Return A");
        assert_eq!(fake.tracks[&2].name.as_str(), "Master");
        assert_eq!(fake.light(2), Some(false), "the master can't be muted");
    });
    let master = app.song().tracks.len() - 1;
    roto_does(&ports, ControlEvent::Knob { index: 2, value: 0 });
    assert!(tick_until_app(&mut app, |app| app.song().tracks[master]
        .level
        == 0));
    press_button(&ports, 2);
    settle(&mut app, 30);
    assert!(!app.levels[master].mute);
    // Back to the tracks, at the bank it was on.
    roto_says(&ports, FromRoto::SetAllTracksMode(AllTracks::Audio));
    assert!(tick_until(&mut app, &ports, |fake| fake.first_track == 8));
}

#[test]
fn every_track_and_group_is_counted_up_to_sixty_eight() {
    let (mut app, _realtime, ports) = mixing(MAX_SOUND_TRACKS);
    roto_says(&ports, LEVELS);
    let count = (MAX_SOUND_TRACKS + MAX_GROUPS) as u16;
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.received.contains(&ToRoto::NumTracks(count))
    }));
    roto_says(&ports, FromRoto::SetFirstTrack(64));
    assert!(tick_until(&mut app, &ports, |fake| fake
        .tracks
        .contains_key(&67)));
    ports.with(|fake| assert_eq!(fake.tracks[&67].name.as_str(), "Group 4"));
}

#[test]
fn the_keys_move_the_knobs_but_never_one_being_touched() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    app.apply(&Action::SetTrackLevel {
        track: 0,
        level: 0x30,
    });
    let moved = position(&app, Control::Level, 0);
    assert!(tick_until(&mut app, &ports, |fake| fake.knob_position(0)
        == Some(moved)));
    assert!(tick_until(&mut app, &ports, |fake| {
        values_shown(fake, 0)
            .last()
            .is_some_and(|text| text.ends_with("dB"))
    }));
    // Touched, it shows its value, which says the app has seen the touch.
    let shown = ports.with(|fake| values_shown(fake, 1).len());
    roto_does(
        &ports,
        ControlEvent::Touch {
            index: 1,
            touched: true,
        },
    );
    assert!(tick_until(&mut app, &ports, |fake| values_shown(fake, 1)
        .len()
        > shown));
    let before = ports.with(|fake| fake.knob_position(1));
    app.apply(&Action::SetTrackLevel {
        track: 1,
        level: 0x30,
    });
    settle(&mut app, 50);
    assert_eq!(ports.with(|fake| fake.knob_position(1)), before);
    roto_does(
        &ports,
        ControlEvent::Touch {
            index: 1,
            touched: false,
        },
    );
    let moved = position(&app, Control::Level, 1);
    assert!(tick_until(&mut app, &ports, |fake| fake.knob_position(1)
        == Some(moved)));
    // An undo moves it back.
    press(&mut app, "u");
    let back = position(&app, Control::Level, 1);
    assert_ne!(back, moved);
    assert!(tick_until(&mut app, &ports, |fake| fake.knob_position(1) == Some(back)));
}

#[test]
fn the_meters_show_the_engines_levels_on_the_tracks_shown() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    for channel in &mut app.track_meters[2].channels {
        channel.peak.now = 0.0;
    }
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.meter(2, Side::Left).is_some_and(|level| level > 100)
            && fake.meter(2, Side::Right).is_some_and(|level| level > 100)
    }));
    assert!(ports.with(|fake| fake.meter(0, Side::Left).is_none_or(|level| level < 10)));
}

#[test]
fn the_default_asked_for_puts_a_fader_back_where_a_new_track_has_it() {
    let (mut app, _realtime, ports) = mixing(8);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(7)
        .is_some()));
    let unity = app.song().tracks[1].level;
    app.apply(&Action::SetTrackLevel {
        track: 1,
        level: 0x20,
    });
    roto_does(&ports, ControlEvent::Default { index: 1 });
    assert!(tick_until_app(&mut app, |app| app.song().tracks[1].level == unity));
}

#[test]
fn the_transport_buttons_play_stop_and_light_as_it_plays() {
    let (mut app, mut realtime, ports) = mixing(2);
    roto_says(&ports, FromRoto::RequestTransportStatus);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.received
            .iter()
            .any(|message| matches!(message, ToRoto::TransportStatus(status) if !status.playing))
    }));
    let transport = |ports: &FakePorts, index: u8| {
        roto_does(ports, ControlEvent::Transport { index, value: 127 });
        roto_does(ports, ControlEvent::Transport { index, value: 0 });
    };
    transport(&ports, 0);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.transport != Transport::Playing && Instant::now() < deadline {
        block(&mut realtime, &mut app);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.transport, Transport::Playing);
    assert!(tick_until(&mut app, &ports, |fake| fake.transport_light(0)
        == Some(true)));
    transport(&ports, 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.transport == Transport::Playing && Instant::now() < deadline {
        block(&mut realtime, &mut app);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.transport, Transport::Paused);
    assert!(tick_until(&mut app, &ports, |fake| fake.transport_light(0)
        == Some(false)));
}

#[test]
fn the_rotos_play_level_and_mute_reach_what_the_engine_plays() {
    let (mut app, mut realtime, ports) = mixing(2);
    common::with_a_sound(&mut app);
    press(&mut app, "ia");
    common::special(&mut app, ratatui::crossterm::event::KeyCode::Esc);
    block(&mut realtime, &mut app);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake
        .knob_position(1)
        .is_some()));
    // PLAY on the Roto plays it.
    roto_does(
        &ports,
        ControlEvent::Transport {
            index: 0,
            value: 127,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.transport != Transport::Playing && Instant::now() < deadline {
        block(&mut realtime, &mut app);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(app.transport, Transport::Playing);
    // Two seconds: a whole loop of the phrase, and its note.
    const LOOP: usize = 2 * 96_000;
    let heard = common::loudest(&mut realtime, LOOP);
    assert!(heard > 0.05, "{heard}");
    // The level all the way down on the Roto: silent, once the engine has it.
    let level = |engine: &Engine| engine.current().song.tracks[0].level;
    roto_does(&ports, ControlEvent::Knob { index: 0, value: 0 });
    assert!(tick_until_app(&mut app, |app| app.song().tracks[0].level == 0));
    assert!(heard_until(&mut app, &mut realtime, |engine| level(engine) == 0));
    let _ = common::loudest(&mut realtime, 4800);
    let down = common::loudest(&mut realtime, LOOP);
    assert!(down < 1e-4, "{down}");
    // Back up, and heard again; then muted with its button, and silent.
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 0,
            value: KNOB_TOP / 8 * 7,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[0].level > 0));
    assert!(heard_until(&mut app, &mut realtime, |engine| level(engine) > 0));
    let _ = common::loudest(&mut realtime, 4800);
    let up = common::loudest(&mut realtime, LOOP);
    assert!(up > 0.05, "{up}");
    press_button(&ports, 0);
    assert!(tick_until_app(&mut app, |app| app.levels[0].mute));
    assert!(heard_until(&mut app, &mut realtime, |engine| engine
        .jam(0)
        .mute));
    let _ = common::loudest(&mut realtime, 4800);
    let muted = common::loudest(&mut realtime, LOOP);
    assert!(muted < 1e-4, "{muted}");
}

#[test]
fn a_second_vimtracker_leaves_the_roto_to_the_one_that_has_it_and_says_so() {
    use vt_tui::roto::Connection;
    let folder = common::folder();
    let lock = folder.join(vt_tui::roto::LOCK_FILE);
    let (mut first, _realtime, first_ports) = mixing(2);
    first.set_roto(false);
    first.roto.lock_path = Some(lock.clone());
    first.set_roto(true);
    assert!(tick_until(&mut first, &first_ports, |fake| {
        fake.meter_thresholds.is_some()
    }));
    let (mut second, _second_realtime, second_ports) = mixing(2);
    second.set_roto(false);
    second.roto.lock_path = Some(lock);
    let sent = second_ports.with(|fake| fake.received.len());
    second.set_roto(true);
    assert_eq!(second.roto.connection, Connection::Taken);
    assert!(
        second.status.contains("another vimtracker"),
        "{}",
        second.status
    );
    assert!(
        second
            .roto
            .connection
            .describe()
            .contains("another vimtracker")
    );
    settle(&mut second, 50);
    assert_eq!(
        second_ports.with(|fake| fake.received.len()),
        sent,
        "nothing sent while the other has it"
    );
    // The first lets it go, and the second takes it.
    first.set_roto(false);
    assert!(tick_until_app(&mut second, |app| {
        matches!(app.roto.connection, Connection::Connected { .. })
    }));
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn the_level_knobs_notch_is_zero_db_with_the_ends_at_silence_and_plus_six() {
    use vt_model::constants::{MAX_LEVEL, UNITY_LEVEL};
    assert_eq!(level_position(f64::from(UNITY_LEVEL)), LEVEL_NOTCH);
    assert_eq!(level_at(LEVEL_NOTCH), UNITY_LEVEL);
    assert_eq!(level_position(0.0), 0);
    assert_eq!(level_at(0), 0);
    assert_eq!(level_position(f64::from(MAX_LEVEL)), KNOB_TOP);
    assert_eq!(level_at(KNOB_TOP), MAX_LEVEL);
    for level in 0..=MAX_LEVEL {
        assert_eq!(
            level_at(level_position(f64::from(level))),
            level,
            "{level:02X}"
        );
    }
    // Turned into the notch on the Roto, a track is at 0 dB; the keys' 0 dB puts it there.
    let (mut app, _realtime, ports) = mixing(2);
    roto_says(&ports, LEVELS);
    assert!(tick_until(&mut app, &ports, |fake| fake.knob_position(0)
        == Some(LEVEL_NOTCH)));
    app.apply(&Action::SetTrackLevel {
        track: 1,
        level: 0x80,
    });
    roto_does(
        &ports,
        ControlEvent::Knob {
            index: 1,
            value: LEVEL_NOTCH,
        },
    );
    assert!(tick_until_app(&mut app, |app| app.song().tracks[1].level == UNITY_LEVEL));
}
