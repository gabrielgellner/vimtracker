//! The Roto-Control: vimtracker's devices as the Roto shows them live, for every kind of device,
//! and the setting that turns it on, with a fake Roto in place of the real one.

mod common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use common::{app, press};
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::settings::SettingId;
use vt_model::song::{FilterType, Machine, Song, Track};
use vt_roto::cc::ControlEvent;
use vt_roto::fake::{FakePorts, FakeRoto};
use vt_roto::link::Options;
use vt_roto::live::{Feel, KNOB_TOP};
use vt_roto::serial::Mode;
use vt_roto::sysex::{PluginKind, ToRoto};
use vt_tui::app::App;
use vt_tui::input::mode::View;
use vt_tui::roto::devices::{Family, knob, live_devices, name, placement};
use vt_tui::roto::edits::value_text;
use vt_tui::roto::{Connection, RotoSetting};
use vt_tui::track_page::{AccumulatorField, ArpField, Control, EnvelopeField, LfoField, Page};

/// A song with a sound track of every machine, each with a different effect in its slots and a
/// different filter, and its groups, returns and master, so every kind of device is in it.
fn every_kind_of_device() -> Song {
    let mut song = Song::new(Machine::ALL.len());
    let sound: Vec<usize> = (0..song.tracks.len())
        .filter(|at| song.tracks[*at].is_sound())
        .collect();
    let mut effects = Insert::ALL.iter().cycle();
    for (at, track) in sound.into_iter().enumerate() {
        let track = &mut song.tracks[track];
        track.machine = Machine::ALL[at];
        track.filter = FilterType::ALL[at % FilterType::ALL.len()];
        for slot in &mut track.inserts {
            *slot = *effects.next().unwrap();
        }
    }
    song
}

#[test]
fn every_knobs_name_fits_the_rotos_twelve_characters_uncut() {
    let song = every_kind_of_device();
    let mut too_long = BTreeSet::new();
    for track in &song.tracks {
        for page in Page::ALL {
            for spec in page.devices(track) {
                for control in spec.rows().flatten().flatten() {
                    let shown = name(control, track);
                    assert!(shown.as_str().chars().count() <= 12);
                    let full = match control {
                        Control::InsertParam(..) => control.name_on(track),
                        _ => control.label_on(track),
                    };
                    if full.chars().count() > 12 {
                        too_long.insert(full);
                    }
                }
            }
        }
        for device in live_devices(track, &song, false) {
            assert!(device.name.as_str().chars().count() <= 12);
            assert!(!device.name.is_empty());
        }
    }
    assert!(too_long.is_empty(), "cut on the Roto: {too_long:?}");
}

#[test]
fn every_position_is_on_the_knob_and_every_device_a_macro_of_its_pages() {
    let song = every_kind_of_device();
    for track in &song.tracks {
        for device in live_devices(track, &song, false) {
            assert!(
                (1..=vt_roto::live::MAX_PAGES).contains(&device.pages()),
                "{}",
                device.name
            );
            assert_eq!(device.controls.len(), usize::from(device.pages()) * 8);
            for control in device.controls.iter().flatten() {
                assert!(control.position <= KNOB_TOP);
                if let Feel::Steps(names) = &control.feel {
                    assert!((2..=24).contains(&names.len()), "{}", control.name);
                }
            }
        }
    }
}

#[test]
fn each_kind_of_setting_has_its_feel() {
    let mut song = Song::new(1);
    let track = &mut song.tracks[0];
    track.inserts[0] = Insert::ALL[2]; // a delay, with its ping-pong switch
    let track = song.tracks[0].clone();
    let master = song
        .tracks
        .iter()
        .find(|track| track.kind == vt_model::song::TrackKind::Master)
        .unwrap()
        .clone();
    let feel = |control: Control, track: &Track| knob(control, track, &song).unwrap().feel;
    let steps = |control: Control, track: &Track| match feel(control, track) {
        Feel::Steps(names) => names.len(),
        other => panic!("{control:?} is {other:?}"),
    };
    // A list of up to 16, each named.
    assert_eq!(steps(Control::FilterType, &track), FilterType::ALL.len());
    assert_eq!(steps(Control::Lfo(0, LfoField::Shape), &track), 7);
    assert_eq!(steps(Control::Arpeggio(ArpField::Mode), &track), 5);
    // Of 17 to 24, stepped all the same, with names for the first 16 as the Roto shows them.
    assert_eq!(steps(Control::Choke, &track), 17);
    assert_eq!(steps(Control::Insert(0), &track), Insert::ALL.len());
    assert_eq!(steps(Control::Machine, &track), Machine::ALL.len());
    // A longer list, or one that changes with the track, turns smoothly.
    assert_eq!(
        feel(Control::Accumulator(AccumulatorField::Up), &track),
        Feel::Smooth
    );
    assert_eq!(
        feel(Control::Lfo(0, LfoField::Destination), &track),
        Feel::Smooth
    );
    assert_eq!(
        feel(Control::Envelope(0, EnvelopeField::Destination), &track),
        Feel::Smooth
    );
    assert_eq!(feel(Control::Sample, &track), Feel::Smooth);
    // Off and on is a switch.
    assert_eq!(
        feel(Control::Lfo(0, LfoField::Retrig), &track),
        Feel::Switch
    );
    assert_eq!(
        feel(Control::Envelope(0, EnvelopeField::Reset), &track),
        Feel::Switch
    );
    assert_eq!(feel(Control::Limiter, &master), Feel::Switch);
    let ping_pong = Insert::ALL[2]
        .params()
        .iter()
        .position(|param| param.is_toggle())
        .unwrap();
    assert_eq!(
        feel(Control::InsertParam(0, ping_pong), &track),
        Feel::Switch
    );
    // A centred value has its notch.
    assert_eq!(
        feel(Control::Base(TrackControl::Pan), &track),
        Feel::CentreNotch
    );
    assert_eq!(
        feel(Control::Base(TrackControl::Pitch), &track),
        Feel::CentreNotch
    );
    assert_eq!(feel(Control::Trim, &track), Feel::CentreNotch);
    assert_eq!(
        feel(Control::Lfo(0, LfoField::Depth), &track),
        Feel::CentreNotch
    );
    // Anything else turns smoothly.
    assert_eq!(
        feel(Control::Base(TrackControl::Cutoff), &track),
        Feel::Smooth
    );
    assert_eq!(feel(Control::Level, &track), Feel::Smooth);
    assert_eq!(
        feel(Control::Envelope(0, EnvelopeField::Attack), &track),
        Feel::Smooth
    );
}

#[test]
fn a_knob_sits_where_its_setting_is() {
    let mut song = Song::new(1);
    let track = &mut song.tracks[0];
    track.pan = 0x80;
    track.cutoff = 0xFF;
    track.filter = FilterType::ALL[3];
    let track = song.tracks[0].clone();
    let at = |control: Control| knob(control, &track, &song).unwrap().position;
    assert_eq!(at(Control::Base(TrackControl::Cutoff)), KNOB_TOP);
    let pan = at(Control::Base(TrackControl::Pan));
    assert!(pan.abs_diff(KNOB_TOP / 2) < 70, "{pan}");
    let filter = at(Control::FilterType);
    assert_eq!(
        filter,
        vt_roto::live::position_of(3, FilterType::ALL.len()),
        "a step's place"
    );
}

#[test]
fn a_family_has_one_colour_wherever_it_is_and_the_families_differ() {
    let song = Song::new(1);
    let track = &song.tracks[0];
    let colour = |control: Control| knob(control, track, &song).unwrap().colour;
    assert_eq!(colour(Control::Level), Family::Level.scheme());
    assert_eq!(
        colour(Control::Base(TrackControl::SendA)),
        Family::Level.scheme()
    );
    assert_eq!(
        colour(Control::Base(TrackControl::Cutoff)),
        Family::Tone.scheme()
    );
    assert_eq!(colour(Control::FilterType), Family::Tone.scheme());
    assert_eq!(
        colour(Control::Base(TrackControl::Pan)),
        Family::Space.scheme()
    );
    assert_eq!(
        colour(Control::Lfo(0, LfoField::Depth)),
        Family::Modulation.scheme()
    );
    let families = [
        Family::Level,
        Family::Tone,
        Family::Space,
        Family::Time,
        Family::Modulation,
        Family::Character,
        Family::Choice,
    ];
    let schemes: BTreeSet<u8> = families.iter().map(|family| family.scheme()).collect();
    assert_eq!(schemes.len(), families.len(), "each family looks different");
}

/// An app whose Roto is a fake one in PLUGIN mode, plugged in, which the link looks for often.
fn app_with_a_fake_roto() -> (App, vt_engine::link::Realtime, FakePorts) {
    let (mut app, realtime) = app();
    let mut fake = FakeRoto::new();
    fake.mode = Mode::Plugin;
    let ports = FakePorts::new(fake, true);
    let shared = ports.clone();
    app.roto.use_ports(move || Box::new(shared.clone()));
    app.roto.options = Options {
        watch_every: Duration::from_millis(10),
        sysex_gap: Duration::ZERO,
        ..Options::default()
    };
    (app, realtime, ports)
}

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

#[test]
fn the_roto_is_off_until_turned_on_on_the_audio_page() {
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    assert!(!app.roto.setting.on);
    app.tick();
    assert_eq!(app.roto.connection, Connection::Off);
    std::thread::sleep(Duration::from_millis(50));
    app.tick();
    assert!(
        ports.with(|fake| fake.received.is_empty()),
        "nothing is sent while it's off"
    );
    press(&mut app, " 7G");
    assert_eq!(app.view, View::Audio);
    assert_eq!(vt_tui::views::status::column_name(&app), "Roto");
    press(&mut app, "=");
    assert!(app.roto.setting.on);
    assert!(app.status.contains("Live"), "{}", app.status);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    assert!(matches!(app.roto.connection, Connection::Connected { .. }));
}

#[test]
fn turned_on_the_roto_shows_the_tracks_devices_live_and_stores_nothing() {
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.set_roto(true);
    let devices = live_devices(&app.song().tracks[0], app.song(), false);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.knob(1).is_some_and(|knob| knob.learned.is_some())
    }));
    ports.with(|fake| {
        let names: Vec<&str> = fake.plugins.values().map(|p| p.name.as_str()).collect();
        let expected: Vec<&str> = devices.iter().take(8).map(|d| d.name.as_str()).collect();
        assert_eq!(names, expected);
        assert_eq!(names[0], "Sampler");
        assert!(fake.plugins.values().all(|p| p.kind == PluginKind::Macro));
    });
    // The machine's first page, as its device describes it.
    let machine = &devices[0];
    assert!(tick_until(&mut app, &ports, |fake| {
        (1..=8).all(|index| {
            fake.knob(index)
                .is_some_and(|k| k.learned.is_some() || k.unmapped)
        })
    }));
    ports.with(|fake| {
        for index in 1..=8u16 {
            let shown = fake.knob(index).unwrap();
            match machine.control(index) {
                Some(control) => {
                    assert_eq!(shown.name, control.name);
                    assert_eq!(shown.colour, control.colour);
                    assert_eq!(shown.learned.as_ref().unwrap().position, control.position);
                }
                None => assert!(shown.unmapped, "knob {index} is a hole"),
            }
        }
        assert!(fake.setups.is_empty());
        assert!(fake.serial_changes.is_empty());
    });
    // The filter, chosen on the Roto.
    ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(1))]);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.shown == Some((1, 0)) && fake.knob(1).is_some_and(|knob| knob.name.as_str() == "Cut")
    }));
}

#[test]
fn a_stepped_knob_turned_on_the_roto_shows_its_steps_name() {
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    // The Play device, whose fifth knob is `Speed`, seven steps from `1/32` to `1/4`.
    ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(PLAY))]);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.shown == Some((PLAY, 0))
            && fake
                .knob(u16::from(SPEED_KNOB) + 1)
                .is_some_and(|knob| knob.learned.is_some())
    }));
    let turned = ports.with(|fake| {
        fake.control(ControlEvent::Knob {
            index: SPEED_KNOB,
            value: KNOB_TOP,
        })
    });
    ports.from_roto(turned);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.received.iter().any(|message| {
            matches!(message, ToRoto::ShowControlValue { index: SPEED_KNOB, value, .. }
                if value.as_str() == "1/4")
        })
    }));
}

#[test]
fn turning_the_roto_off_hands_it_back_and_closes_the_link() {
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    press(&mut app, " 7G-");
    assert!(!app.roto.setting.on);
    assert!(!app.roto.is_linked());
    assert_eq!(app.roto.connection, Connection::Off);
    assert!(app.status.contains("given back"), "{}", app.status);
    let sent = ports.with(|fake| fake.received.len());
    assert_eq!(
        ports.with(|fake| fake.received.last().cloned()),
        Some(ToRoto::DawStarted)
    );
    // Nothing more once it's off, even when the Roto pings.
    ports.from_roto(ports.with(|fake| fake.plug_in()));
    std::thread::sleep(Duration::from_millis(50));
    app.tick();
    assert_eq!(ports.with(|fake| fake.received.len()), sent);
}

#[test]
fn the_setting_is_saved_for_this_machine_and_read_back() {
    let folder = common::folder();
    let path = folder.join("vimtracker").join(vt_tui::roto::SETTING_FILE);
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.roto.setting_path = Some(path.clone());
    assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: false });
    app.set_roto(true);
    assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: true });
    // The next start reads it and connects.
    let (mut next, _realtime, next_ports) = app_with_a_fake_roto();
    next.roto.setting = RotoSetting::load_from(&path);
    next.start_roto_if_on();
    assert!(next.roto.is_linked());
    assert!(tick_until(&mut next, &next_ports, |fake| fake
        .shown
        .is_some()));
    app.set_roto(false);
    assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: false });
    drop(ports);
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn the_audio_page_says_whether_the_roto_is_on_and_connected() {
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    let text = |app: &App| -> String {
        let backend = ratatui::backend::TestBackend::new(100, 14);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::audio::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        text(&app).contains("Off, so Live can have it"),
        "{}",
        text(&app)
    );
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    for _ in 0..20 {
        app.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
    let shown = text(&app);
    assert!(shown.contains("On, connected, firmware 3.3.0"), "{shown}");
}

/// An app connected to a fake Roto in PLUGIN mode, showing the `device`th device of the cursor's
/// track, its first page's knobs learned.
fn showing(device: u8) -> (App, vt_engine::link::Realtime, FakePorts) {
    let (mut app, realtime, ports) = app_with_a_fake_roto();
    app.roto.gesture_pause = Duration::ZERO;
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(device))]);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.shown == Some((device, 0))
            && (1..=8).all(|index| {
                fake.knob(index)
                    .is_some_and(|knob| knob.learned.is_some() || knob.unmapped)
            })
    }));
    (app, realtime, ports)
}

fn turn(ports: &FakePorts, knob: u8, value: u16) {
    let ccs = ports.with(|fake| fake.control(ControlEvent::Knob { index: knob, value }));
    ports.from_roto(ccs);
}

fn touch(ports: &FakePorts, knob: u8, touched: bool) {
    let ccs = ports.with(|fake| {
        fake.control(ControlEvent::Touch {
            index: knob,
            touched,
        })
    });
    ports.from_roto(ccs);
}

/// Ticks the app until the song is as wanted, for up to five seconds.
fn tick_until_song(app: &mut App, done: impl Fn(&Song) -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        app.tick();
        if done(app.song()) {
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

/// The Output device, the third, has `Level` on its first knob and `Trim` on its fifth.
const OUTPUT: u8 = 2;
const TRIM_KNOB: u8 = 4;
const LEVEL_KNOB: u8 = 0;

/// The Play device, the fourth, has `Speed` on its fifth knob.
const PLAY: u8 = 3;
const SPEED_KNOB: u8 = 4;

#[test]
fn a_knob_edits_its_own_track_whatever_the_cursor_is_on() {
    let (mut app, _realtime, ports) = showing(OUTPUT);
    assert_eq!(app.roto.track, 0);
    // The cursor goes to the second track, and to the phrase view.
    app.cursor.track = 1;
    press(&mut app, " 3");
    let other = app.song().tracks[1].level;
    let top = SettingId::VOL.entry().kind.top();
    turn(&ports, LEVEL_KNOB, KNOB_TOP);
    assert!(tick_until_song(&mut app, |song| song.tracks[0].level == top));
    assert_eq!(app.song().tracks[1].level, other, "not the cursor's track");
    assert_eq!(app.cursor.track, 1, "the cursor stays");
    // Its value, as the app writes it.
    let text = value_text(Control::Level, &app.song().tracks[0], app.song());
    assert!(text.ends_with("dB"), "{text}");
    assert!(tick_until(&mut app, &ports, |fake| {
        values_shown(fake, LEVEL_KNOB).last() == Some(&text)
    }));
}

#[test]
fn touching_a_knob_to_letting_it_go_is_one_undo_step() {
    let (mut app, _realtime, ports) = showing(OUTPUT);
    let was = app.song().tracks[0].level;
    let depth = app.history.undo_depth();
    touch(&ports, LEVEL_KNOB, true);
    for value in [2000, 5000, 8000, 11_000] {
        turn(&ports, LEVEL_KNOB, value);
        let level = |song: &Song| song.tracks[0].level;
        let before = level(app.song());
        assert!(tick_until_song(&mut app, |song| level(song) != before));
        settle(&mut app, 10);
    }
    assert!(
        app.history.undo_depth() > depth + 1,
        "each frame's move is an edit"
    );
    touch(&ports, LEVEL_KNOB, false);
    settle(&mut app, 30);
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "one step for the touch"
    );
    press(&mut app, "u");
    assert_eq!(app.song().tracks[0].level, was);
    // Turned without a touch, as a hard twist can be, it's a step of its own once it settles.
    turn(&ports, LEVEL_KNOB, 3000);
    turn(&ports, LEVEL_KNOB, 4000);
    assert!(tick_until_song(&mut app, |song| song.tracks[0].level != was));
    settle(&mut app, 30);
    assert_eq!(app.history.undo_depth(), depth + 1);
}

#[test]
fn a_stepped_knob_moves_through_its_list() {
    use vt_model::speed::Speed;
    use vt_roto::live::position_of;
    let (mut app, _realtime, ports) = showing(PLAY);
    for (at, speed) in Speed::ALL.iter().enumerate().rev() {
        turn(&ports, SPEED_KNOB, position_of(at, Speed::ALL.len()));
        assert!(tick_until_song(&mut app, |song| song.tracks[0].speed == *speed));
        assert!(tick_until(&mut app, &ports, |fake| {
            values_shown(fake, SPEED_KNOB).last().map(String::as_str) == Some(speed.name())
        }));
    }
}

#[test]
fn a_knob_being_touched_is_never_sent_its_own_value_back() {
    let (mut app, _realtime, ports) = showing(OUTPUT);
    let positions = |ports: &FakePorts, knob: u8| ports.with(|fake| fake.knob_position(knob));
    let was = app.song().tracks[0].level;
    touch(&ports, LEVEL_KNOB, true);
    turn(&ports, LEVEL_KNOB, 3000);
    assert!(tick_until_song(&mut app, |song| song.tracks[0].level != was));
    settle(&mut app, 30);
    assert_eq!(
        positions(&ports, LEVEL_KNOB),
        None,
        "not sent its own value"
    );
    // Another knob's setting moved by the keys is sent: its motor follows.
    app.apply(&vt_model::action::Action::SetTrackTrim {
        track: 0,
        trim: 0xC0,
    });
    let trim = knob(Control::Trim, &app.song().tracks[0], app.song()).unwrap();
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.knob_position(TRIM_KNOB) == Some(trim.position)
    }));
    // And shows its value as it goes, as a knob turned by hand does.
    let text = value_text(Control::Trim, &app.song().tracks[0], app.song());
    assert!(tick_until(&mut app, &ports, |fake| {
        values_shown(fake, TRIM_KNOB).last() == Some(&text)
    }));
    // The touched knob's own setting moved by the keys waits for the hand to let go.
    app.apply(&vt_model::action::Action::SetTrackLevel {
        track: 0,
        level: 0x10,
    });
    settle(&mut app, 30);
    assert_eq!(
        positions(&ports, LEVEL_KNOB),
        None,
        "not moved under the hand"
    );
    touch(&ports, LEVEL_KNOB, false);
    let level = knob(Control::Level, &app.song().tracks[0], app.song()).unwrap();
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.knob_position(LEVEL_KNOB) == Some(level.position)
    }));
    // Turned and let go, with nothing else moving it, it's left where the hand put it.
    let sent = ports.with(|fake| fake.received_ccs.len());
    touch(&ports, LEVEL_KNOB, true);
    turn(&ports, LEVEL_KNOB, 9000);
    touch(&ports, LEVEL_KNOB, false);
    settle(&mut app, 50);
    assert_eq!(ports.with(|fake| fake.received_ccs.len()), sent, "no echo");
}

#[test]
fn values_are_shown_at_most_twelve_times_a_second_and_the_last_always() {
    let (mut app, _realtime, ports) = showing(OUTPUT);
    let started = Instant::now();
    for step in 0..40u16 {
        turn(&ports, LEVEL_KNOB, 400 * step);
        app.tick();
        std::thread::sleep(Duration::from_millis(3));
    }
    let elapsed = started.elapsed();
    let shown = ports.with(|fake| values_shown(fake, LEVEL_KNOB).len());
    let most = 1 + elapsed.as_millis() / vt_tui::roto::VALUE_EVERY.as_millis();
    assert!(
        (1..=most as usize).contains(&shown),
        "{shown} values in {elapsed:?}"
    );
    settle(&mut app, 200);
    let last = value_text(Control::Level, &app.song().tracks[0], app.song());
    assert_eq!(
        ports.with(|fake| values_shown(fake, LEVEL_KNOB).last().cloned()),
        Some(last),
        "the last value is shown"
    );
}

#[test]
fn sel_switches_an_effect_or_a_modulator_and_b_shows_on_the_roto() {
    use vt_model::action::Action;
    use vt_roto::sysex::FromRoto;
    let (mut app, _realtime, ports) = showing(OUTPUT);
    let tap = |ports: &FakePorts, index: u8, enabled: bool| {
        let bytes = ports.with(|fake| fake.press(FromRoto::SetPluginEnabled { index, enabled }));
        ports.from_roto(vec![bytes]);
    };
    let enabled = |fake: &FakeRoto, device: u8| fake.plugins.get(&device).map(|p| p.enabled);
    app.apply(&Action::SetInsert {
        track: 0,
        slot: 0,
        insert: Insert::ALL[2],
    });
    assert!(tick_until(&mut app, &ports, |fake| enabled(fake, 4) == Some(true)));
    // Tapping SEL and the slot bypasses it.
    tap(&ports, 4, false);
    assert!(tick_until_song(&mut app, |song| song.tracks[0].bypassed[0]));
    assert!(tick_until(&mut app, &ports, |fake| enabled(fake, 4) == Some(false)));
    // `b` in the app, as its action, brings it back, which shows on the Roto.
    app.apply(&Action::SetBypass {
        track: 0,
        slot: 0,
        bypassed: false,
    });
    assert!(tick_until(&mut app, &ports, |fake| enabled(fake, 4) == Some(true)));
    // On the second bank, counted from it as the Roto counts: its second device is LFO 1.
    let page = ports.with(|fake| fake.press(FromRoto::SetFirstPlugin(8)));
    ports.from_roto(vec![page]);
    assert!(tick_until(&mut app, &ports, |fake| fake.first_plugin == 8));
    tap(&ports, 1, false);
    assert!(tick_until_song(&mut app, |song| song.tracks[0].lfos[0].bypassed));
    assert!(tick_until(&mut app, &ports, |fake| enabled(fake, 9) == Some(false)));
    assert_eq!(ports.with(|fake| fake.first_plugin), 8, "the bank stays");
    // The accumulator has no switch: it stays on, and is sent so again.
    let (sent, undo) = (
        ports.with(|fake| fake.received.len()),
        app.history.undo_depth(),
    );
    tap(&ports, 6, false);
    assert!(tick_until(&mut app, &ports, |fake| fake.received.len() > sent));
    assert_eq!(ports.with(|fake| enabled(fake, 14)), Some(true));
    assert_eq!(app.history.undo_depth(), undo, "and nothing changed");
}

#[test]
fn choosing_a_device_while_sels_switches_are_up_never_switches_it() {
    use vt_roto::sysex::{ControlType, FromRoto};
    // The real Roto says it's chosen with `ROTO SELECT PLUGIN` sometimes, and sometimes only by
    // asking for the device's knobs.
    for says_so in [true, false] {
        let (mut app, _realtime, ports) = showing(OUTPUT);
        let depth = app.history.undo_depth();
        // Tapping SEL, paging to the second bank, then holding SEL and pressing LFO 1, the
        // tenth device: the Roto switches it as the button goes down.
        let messages = ports.with(|fake| {
            vec![
                fake.press(FromRoto::SetFirstPlugin(8)),
                fake.press(FromRoto::SetPluginEnabled {
                    index: 1,
                    enabled: false,
                }),
            ]
        });
        ports.from_roto(messages);
        assert!(tick_until_song(&mut app, |song| song.tracks[0].lfos[0].bypassed));
        // SEL let go: it asks for the knobs of the device it now shows, and may say so.
        let mut messages = ports.with(|fake| {
            vec![fake.press(FromRoto::ControlMapped {
                index: 1,
                hash: FakeRoto::knob_hash(1),
                control: ControlType::Knob,
                control_index: 0,
                macro_param: true,
            })]
        });
        if says_so {
            messages.push(ports.with(|fake| fake.choose_plugin(1)));
        }
        ports.from_roto(messages);
        let lfo = live_devices(&app.song().tracks[0], app.song(), false)[9].clone();
        let first = lfo.control(1).unwrap().name.clone();
        assert!(tick_until(&mut app, &ports, |fake| {
            fake.shown == Some((9, 0)) && fake.knob(1).is_some_and(|knob| knob.name == first)
        }));
        assert_eq!(
            app.roto.view.selected_index(),
            9,
            "LFO 1's knobs, {says_so}"
        );
        assert!(
            !app.song().tracks[0].lfos[0].bypassed,
            "chosen, not switched"
        );
        assert_eq!(app.history.undo_depth(), depth, "and no undo step left");
        assert_eq!(
            ports.with(|fake| fake.plugins.get(&9).map(|p| p.enabled)),
            Some(true)
        );
    }
}

#[test]
fn func_chooses_the_track_the_roto_shows_and_the_cursor_stays() {
    use vt_model::action::Action;
    use vt_roto::sysex::FromRoto;
    let (mut app, _realtime, ports) = showing(OUTPUT);
    ports.with(|fake| {
        assert_eq!(fake.tracks[&0].name.as_str(), "Track 01");
        assert_eq!(fake.tracks[&1].name.as_str(), "Track 02");
        assert_eq!(
            fake.selected_track.as_ref().map(|track| track.index),
            Some(0)
        );
    });
    let machine = Machine::ALL[3];
    app.apply(&Action::SetTrackMachine { track: 1, machine });
    let choose = ports.with(|fake| fake.press(FromRoto::RotoSelectTrack(1)));
    ports.from_roto(vec![choose]);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.plugins
            .get(&0)
            .is_some_and(|p| p.name.as_str() == machine.name())
    }));
    assert_eq!(app.roto.track, 1);
    assert_eq!(app.cursor.track, 0, "the cursor stays");
    assert_eq!(app.roto.view.selected_index(), OUTPUT, "on the same device");
    // Its knobs now edit that track.
    let top = SettingId::VOL.entry().kind.top();
    let first = app.song().tracks[0].level;
    turn(&ports, LEVEL_KNOB, KNOB_TOP);
    assert!(tick_until_song(&mut app, |song| song.tracks[1].level == top));
    assert_eq!(app.song().tracks[0].level, first);
    // The arrows page through the tracks: the groups, the returns and the master after them.
    let page = ports.with(|fake| fake.press(FromRoto::SetFirstTrack(8)));
    ports.from_roto(vec![page]);
    let last = app.song().tracks.len() - 1;
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.tracks
            .get(&(last as u16))
            .is_some_and(|track| track.name.as_str() == "Master")
    }));
    ports.with(|fake| {
        let group = fake
            .tracks
            .values()
            .find(|track| track.name.as_str() == "Group 1");
        assert!(group.is_some_and(|group| group.grouped));
    });
}

#[test]
fn every_knob_left_where_it_is_edits_nothing_and_each_end_is_an_edit_that_holds() {
    use vt_model::action::edit;
    use vt_tui::roto::devices::{control_at, track_devices};
    use vt_tui::roto::edits::turned;
    let song = std::sync::Arc::new(every_kind_of_device());
    let mut checked = 0;
    for (at, track) in song.tracks.iter().enumerate() {
        for (device, (_, _, spec)) in track_devices(track).into_iter().enumerate() {
            let knobs = placement(&spec, track, false).knobs;
            for (place, control) in knobs.into_iter().enumerate() {
                let Some((control, _)) = control else {
                    continue;
                };
                assert_eq!(
                    control_at(track, device, place as u16 + 1, false),
                    Some(control)
                );
                let Some(shown) = knob(control, track, &song) else {
                    continue;
                };
                let what = format!("{control:?} in {} on {:?}", spec.identity, track.kind);
                assert_eq!(turned(control, at, &song, shown.position), None, "{what}");
                for end in [0, KNOB_TOP / 3, KNOB_TOP] {
                    let Some(action) = turned(control, at, &song, end) else {
                        continue;
                    };
                    let next = edit(&song, &action).unwrap_or_else(|e| panic!("{what}: {e}"));
                    assert_eq!(turned(control, at, &next, end), None, "{what} at {end}");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 500, "{checked}");
}

#[test]
fn a_smooth_knob_sets_the_fine_digits_and_values_read_as_the_app_writes_them() {
    use vt_model::action::edit;
    use vt_tui::roto::edits::turned;
    let mut song = Song::new(1);
    song.tracks[0].inserts[0] = Insert::ALL[1];
    let song = std::sync::Arc::new(song);
    let cut = Control::Base(TrackControl::Cutoff);
    let next = edit(&song, &turned(cut, 0, &song, 5000).unwrap()).unwrap();
    let track = &next.tracks[0];
    assert_ne!(track.fine(SettingId::CUT), 0, "the fine digits are set");
    let back = knob(cut, track, &next).unwrap().position;
    assert!(back.abs_diff(5000) <= 1, "{back}");
    assert!(value_text(cut, track, &next).ends_with("Hz"));
    let level = value_text(Control::Level, track, &next);
    assert!(level.ends_with(" dB"), "{level}");
    assert_eq!(value_text(Control::Speed, track, &next), "1/16");
    assert_eq!(value_text(Control::Insert(0), track, &next), "Distort");
    // Distort's `Type` turned to its seventh of eight.
    let kind = Insert::ALL[1]
        .params()
        .iter()
        .position(|param| param.name == "Type")
        .unwrap();
    let control = Control::InsertParam(0, kind);
    let at = vt_roto::live::position_of(6, 8);
    let next = edit(&next, &turned(control, 0, &next, at).unwrap()).unwrap();
    assert_eq!(value_text(control, &next.tracks[0], &next), "Overdrive");
}

/// An app whose fake Roto takes vimtracker's setups over its serial port too, and stores one of
/// Ableton Live's already.
fn app_with_stored_setups() -> (App, vt_engine::link::Realtime, FakePorts) {
    use vt_roto::bytes::PluginHash;
    use vt_roto::fake::StoredSetup;
    use vt_roto::serial::{StoredKind, StoredPlugin};
    use vt_roto::store::Serial;
    let (mut app, realtime, ports) = app_with_a_fake_roto();
    ports.with(|fake| {
        fake.setups.push(StoredSetup {
            plugin: StoredPlugin {
                hash: PluginHash::of("ableton drift"),
                name: vt_roto::Label::new("Drift"),
                kind: StoredKind::Normal,
            },
            knobs: Default::default(),
            switches: Default::default(),
        });
    });
    let serial = ports.clone();
    app.roto.serial = Some(std::sync::Arc::new(move || {
        Ok(Box::new(serial.clone()) as Box<dyn Serial + Send>)
    }));
    (app, realtime, ports)
}

fn press_button(ports: &FakePorts, button: u8) {
    for value in [0x7F, 0x00] {
        let ccs = ports.with(|fake| {
            fake.control(ControlEvent::Switch {
                index: button,
                value,
            })
        });
        ports.from_roto(ccs);
    }
}

#[test]
fn a_device_with_buttons_is_stored_and_its_buttons_switch_its_settings_lit_from_the_song() {
    use vt_model::action::Action;
    use vt_roto::bytes::PluginHash;
    use vt_tui::roto::devices::stored_setups;
    let (mut app, _realtime, ports) = app_with_stored_setups();
    let comp = Insert::ALL[6];
    let glue = comp.with_value(comp.type_param().unwrap(), 2);
    app.apply(&Action::SetInsert {
        track: 0,
        slot: 0,
        insert: glue,
    });
    let auto = glue.params().iter().position(|p| p.key == "auto").unwrap();
    let auto_on = |song: &Song| song.tracks[0].inserts[0].value(auto) != 0;
    app.set_roto(true);
    // vimtracker's setups are written beside Live's, under its own hashes alone.
    let wanted = stored_setups().len();
    assert!(tick_until(&mut app, &ports, |fake| fake.setups.len() == wanted + 1));
    let ours: BTreeSet<PluginHash> = stored_setups().iter().map(|s| s.hash()).collect();
    ports.with(|fake| {
        assert_eq!(fake.setups[0].plugin.name.as_str(), "Drift");
        for request in &fake.serial_changes {
            if let Some(hash) = request.plugin_changed() {
                assert!(ours.contains(&hash), "{request:?}");
            }
        }
    });
    // The slot is then listed as the stored Glue, and chosen it maps its buttons, lit as the
    // song has them.
    let glue_hash = PluginHash::of("vimtracker:fx/comp-glue");
    assert!(tick_until(&mut app, &ports, |fake| fake
        .plugins
        .get(&4)
        .is_some_and(
            |p| p.kind == PluginKind::Normal && p.hash == glue_hash
        )));
    ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(4))]);
    let was = auto_on(app.song());
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.shown == Some((4, 0)) && fake.light(4) == Some(was)
    }));
    ports.with(|fake| {
        let threshold = fake.knob(1).unwrap().learned.clone().unwrap();
        assert_eq!(threshold.name.as_str(), "Threshold");
        assert!(!threshold.macro_param, "a stored device's knob");
    });
    // Auto's button, under the makeup, switches it, one undo step a press, and its light follows.
    let depth = app.history.undo_depth();
    press_button(&ports, 4);
    assert!(tick_until_song(&mut app, |song| auto_on(song) != was));
    assert!(tick_until(&mut app, &ports, |fake| fake.light(4) == Some(!was)));
    assert_eq!(app.history.undo_depth(), depth + 1);
    press_button(&ports, 4);
    assert!(tick_until_song(&mut app, |song| auto_on(song) == was));
    assert!(tick_until(&mut app, &ports, |fake| fake.light(4) == Some(was)));
    // Switched from the keys, the light follows too.
    app.apply(&Action::SetInsert {
        track: 0,
        slot: 0,
        insert: glue.with_value(auto, u8::from(!was)),
    });
    assert!(tick_until(&mut app, &ports, |fake| fake.light(4) == Some(!was)));
    // A button with nothing on it changes nothing.
    let depth = app.history.undo_depth();
    press_button(&ports, 0);
    settle(&mut app, 30);
    assert_eq!(app.history.undo_depth(), depth);
}

#[test]
fn without_vimtrackers_setups_a_devices_buttons_are_two_step_knobs() {
    use vt_roto::store::Serial;
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.roto.serial = Some(std::sync::Arc::new(|| {
        Err::<Box<dyn Serial + Send>, _>(vt_roto::transport::LinkError::NotFound)
    }));
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    // The serial port is tried a few times, a second apart, before it gives up.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !app.status.contains("knobs for now") && Instant::now() < deadline {
        settle(&mut app, 20);
    }
    assert!(app.status.contains("knobs for now"), "{}", app.status);
    assert!(!app.roto.stored_on_roto);
    ports.with(|fake| {
        assert!(fake.plugins.values().all(|p| p.kind == PluginKind::Macro));
        assert!(fake.serial_changes.is_empty());
    });
    // The envelope's reset is on its sixth knob, as two steps.
    let envelope = live_devices(&app.song().tracks[0], app.song(), false)
        .into_iter()
        .find(|device| device.name.as_str() == "Env 1")
        .unwrap();
    assert_eq!(envelope.controls[5].as_ref().unwrap().feel, Feel::Switch);
}
