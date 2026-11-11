//! The Roto-Control: vimtracker's devices as the Roto shows them live, for every kind of device,
//! and the setting that turns it on, with a fake Roto in place of the real one.

mod common;

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use common::{app, press};
use vt_model::action::TrackControl;
use vt_model::insert::Insert;
use vt_model::song::{FilterType, Machine, Song, Track};
use vt_roto::fake::{FakePorts, FakeRoto};
use vt_roto::link::Options;
use vt_roto::live::{Feel, KNOB_TOP};
use vt_roto::serial::Mode;
use vt_roto::sysex::{PluginKind, ToRoto};
use vt_tui::app::App;
use vt_tui::input::mode::View;
use vt_tui::roto::devices::{Family, knob, knob_order, live_devices, name};
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

/// Whether a control should have a knob on this track: every setting it has, but a filter's
/// `Mode` when its type has no modes.
fn has_knob(control: Control, track: &Track) -> bool {
    control.exists_on(track) && !(control == Control::FilterMode && !track.filter.has_modes())
}

#[test]
fn every_device_places_every_setting_once_row_by_row_with_holes_kept() {
    let song = every_kind_of_device();
    let mut effects_seen = BTreeSet::new();
    for track in &song.tracks {
        for page in Page::ALL {
            for spec in page.devices(track) {
                let order = knob_order(&spec);
                assert!(order.len() == 8 || order.len() == 16, "{}", spec.identity);
                // Row by row through the grid, then the title.
                let mut expected: Vec<Option<Control>> =
                    spec.grid.iter().flatten().copied().collect();
                expected.extend(spec.title);
                assert_eq!(order[..], expected[..order.len()], "{}", spec.identity);
                assert!(expected[order.len()..].iter().all(Option::is_none));
                for control in spec.rows().flatten().flatten() {
                    assert_eq!(
                        order.iter().filter(|at| **at == Some(control)).count(),
                        1,
                        "{control:?} in {}",
                        spec.identity
                    );
                    let found = knob(control, track, &song);
                    assert_eq!(
                        found.is_some(),
                        has_knob(control, track),
                        "{control:?} on {:?}",
                        track.kind
                    );
                    if let Control::InsertParam(slot, _) = control {
                        effects_seen.insert(track.inserts[slot].name());
                    }
                }
            }
        }
    }
    assert_eq!(
        effects_seen.len(),
        Insert::ALL.len() - 1,
        "every effect was seen: {effects_seen:?}"
    );
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
        for device in live_devices(track, &song) {
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
        for device in live_devices(track, &song) {
            assert!(
                device.pages() == 1 || device.pages() == 2,
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
    let devices = live_devices(&app.song().tracks[0], app.song());
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
    use vt_roto::cc::ControlEvent;
    let (mut app, _realtime, ports) = app_with_a_fake_roto();
    app.set_roto(true);
    assert!(tick_until(&mut app, &ports, |fake| fake.shown.is_some()));
    // The Play device, whose second knob is `Speed`, seven steps from `1/32` to `1/4`.
    ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(3))]);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.shown == Some((3, 0)) && fake.knob(2).is_some_and(|knob| knob.learned.is_some())
    }));
    let turned = ports.with(|fake| {
        fake.control(ControlEvent::Knob {
            index: 1,
            value: KNOB_TOP,
        })
    });
    ports.from_roto(turned);
    assert!(tick_until(&mut app, &ports, |fake| {
        fake.received.iter().any(|message| {
            matches!(message, ToRoto::ShowControlValue { index: 1, value, .. }
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
