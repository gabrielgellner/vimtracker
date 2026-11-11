//! Live devices against the fake Roto: the plugin list, each knob's name, feel, position and
//! colour sent as the Roto asks for it, the pages, and nothing ever stored.

use std::time::{Duration, Instant};

use vt_roto::Label;
use vt_roto::fake::{DEFAULT_CONTROL_COLOUR, FakePorts, FakeRoto};
use vt_roto::link::{Event, Link, Options};
use vt_roto::live::{Feel, KNOB_TOP, LiveControl, LiveDevice, PluginView};
use vt_roto::serial::Mode;
use vt_roto::session::{Change, Session};
use vt_roto::sysex::{Daw, FromRoto, PluginKind, ToRoto};

fn knob(name: &str, feel: Feel, position: u16, colour: u8) -> Option<LiveControl> {
    Some(LiveControl {
        name: Label::new(name),
        feel,
        position,
        colour,
    })
}

/// A filter of two pages, with holes, and a delay of one.
fn devices() -> Vec<LiveDevice> {
    let steps = |names: &[&str]| Feel::Steps(names.iter().map(|name| Label::new(name)).collect());
    let mut filter = vec![None; 16];
    filter[0] = knob("Cut", Feel::Smooth, 12000, 9);
    filter[1] = knob("Low cut", Feel::Smooth, 0, 9);
    filter[4] = knob("Res", Feel::Smooth, 3000, 9);
    filter[8] = knob("Drive", Feel::Smooth, 0, 15);
    filter[12] = knob("Filter", steps(&["LP", "HP", "BP"]), 0, 13);
    let mut delay = vec![None; 8];
    delay[0] = knob("Time", Feel::Smooth, 4000, 17);
    delay[1] = knob("Pan", Feel::CentreNotch, KNOB_TOP / 2, 20);
    delay[2] = knob("Ping-pong", Feel::Switch, KNOB_TOP, 13);
    vec![
        LiveDevice {
            key: "vimtracker/filter".to_owned(),
            name: Label::new("Filter"),
            enabled: true,
            controls: filter,
        },
        LiveDevice {
            key: "vimtracker/fx1/delay".to_owned(),
            name: Label::new("1 Delay"),
            enabled: false,
            controls: delay,
        },
    ]
}

/// A fake Roto in PLUGIN mode, connected to a session as Bitwig.
fn connected() -> FakeRoto {
    let mut fake = FakeRoto::new();
    fake.mode = Mode::Plugin;
    let mut session = Session::new(Daw::Bitwig);
    let mut to_roto = session.start();
    while !to_roto.is_empty() {
        let replies: Vec<Vec<u8>> = to_roto
            .drain(..)
            .flat_map(|message| fake.midi_in(&message.encode()))
            .collect();
        for reply in replies {
            to_roto.extend(session.receive(FromRoto::decode(&reply).unwrap()).0);
        }
    }
    assert!(session.is_connected());
    fake
}

/// Passes messages between the view and the fake until neither has more to say.
fn exchange(view: &mut PluginView, fake: &mut FakeRoto, first: Vec<ToRoto>) {
    let mut to_roto = first;
    while !to_roto.is_empty() {
        let replies: Vec<Vec<u8>> = to_roto
            .drain(..)
            .flat_map(|message| fake.midi_in(&message.encode()))
            .collect();
        for reply in replies {
            to_roto.extend(view.receive(&FromRoto::decode(&reply).unwrap()));
        }
    }
}

#[test]
fn each_device_is_a_macro_plugin_of_as_many_pages_as_its_knobs_need() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    assert_eq!(fake.plugins.len(), 2);
    let filter = &fake.plugins[&0];
    assert_eq!(filter.name.as_str(), "Filter");
    assert_eq!(filter.kind, PluginKind::Macro);
    assert_eq!(filter.macro_pages, 2);
    let delay = &fake.plugins[&1];
    assert_eq!((delay.macro_pages, delay.enabled), (1, false));
    assert_eq!(fake.shown, Some((0, 0)));
}

#[test]
fn each_knob_the_roto_asks_for_is_learned_with_its_name_feel_position_and_colour() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    let cut = fake.knob(1).unwrap();
    assert_eq!(cut.name.as_str(), "Cut");
    assert_eq!(cut.colour, 9);
    let learned = cut.learned.as_ref().unwrap();
    assert!(learned.macro_param);
    assert_eq!((learned.steps, learned.centre_indent), (0, false));
    assert_eq!(learned.position, 12000);
    // The answer gives back the Roto's own number and hash for the knob.
    assert_eq!(learned.index, 1);
    assert_eq!(learned.hash, FakeRoto::knob_hash(1));
    assert_eq!(fake.knob(5).unwrap().name.as_str(), "Res");
    // Holes are unmapped and keep the Roto's own colour.
    for hole in [3, 4, 6, 7, 8] {
        let knob = fake.knob(hole).unwrap();
        assert!(knob.unmapped, "knob {hole}");
        assert_eq!(knob.colour, DEFAULT_CONTROL_COLOUR);
    }
    // The second page isn't asked for until it's shown.
    assert!(fake.knob(9).is_none_or(|knob| knob.learned.is_none()));
}

#[test]
fn turning_the_page_learns_its_knobs_numbered_on_from_the_first_page() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    let turned: Vec<FromRoto> = fake
        .turn_page(1)
        .iter()
        .map(|bytes| FromRoto::decode(bytes).unwrap())
        .collect();
    let answers: Vec<ToRoto> = turned.iter().flat_map(|m| view.receive(m)).collect();
    assert_eq!(view.page(), 1);
    exchange(&mut view, &mut fake, answers);
    assert_eq!(fake.knob(9).unwrap().name.as_str(), "Drive");
    let filter = fake.knob(13).unwrap();
    assert_eq!(filter.name.as_str(), "Filter");
    let learned = filter.learned.as_ref().unwrap();
    assert_eq!(learned.steps, 3);
    assert_eq!(
        learned.step_names,
        [Label::new("LP"), Label::new("HP"), Label::new("BP")]
    );
    assert!(fake.knob(16).unwrap().unmapped);
}

#[test]
fn choosing_a_device_on_the_roto_shows_its_knobs() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    let chosen = fake.choose_plugin(1);
    let answers = view.receive(&FromRoto::decode(&chosen).unwrap());
    exchange(&mut view, &mut fake, answers);
    assert_eq!(fake.shown, Some((1, 0)));
    let pan = fake.knob(2).unwrap().learned.clone().unwrap();
    assert!(pan.centre_indent, "a centred value has its notch");
    assert_eq!(pan.position, KNOB_TOP / 2);
    let ping_pong = fake.knob(3).unwrap().learned.clone().unwrap();
    assert_eq!(ping_pong.steps, 2, "a switch is two steps");
    assert_eq!(ping_pong.step_names, [Label::new("Off"), Label::new("On")]);
    assert_eq!(ping_pong.position, KNOB_TOP);
}

#[test]
fn showing_the_same_again_is_answered_with_nothing_new() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    let learned = fake.received.len();
    let again = view.receive(&FromRoto::SetPluginMode);
    exchange(&mut view, &mut fake, again);
    let since: Vec<&ToRoto> = fake.received[learned..].iter().collect();
    assert!(
        since
            .iter()
            .all(|message| !matches!(message, ToRoto::LearnParam(_))),
        "{since:?}"
    );
}

/// Every SysEx message a whole session sends: the handshake, the list, both pages and a device
/// chosen, through the link's thread.
#[test]
fn the_link_shows_devices_live_and_never_writes_a_stored_setup() {
    let mut fake = FakeRoto::new();
    fake.mode = Mode::Plugin;
    let ports = FakePorts::new(fake, true);
    let options = Options {
        watch_every: Duration::from_millis(10),
        sysex_gap: Duration::ZERO,
        ..Options::default()
    };
    let link = Link::spawn(ports.clone(), options).unwrap();
    let mut view = PluginView::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut turned = false;
    let mut chose = false;
    while Instant::now() < deadline {
        for event in link.poll() {
            let messages = match event {
                Event::Session(Change::Connected) => view.show(devices()),
                Event::Session(Change::Message(message)) => view.receive(&message),
                _ => Vec::new(),
            };
            for message in messages {
                link.send(message);
            }
        }
        let filter_learned = ports.with(|fake| {
            fake.knob(1)
                .is_some_and(|knob| knob.learned.is_some() && fake.shown == Some((0, 0)))
        });
        if filter_learned && !turned {
            turned = true;
            ports.from_roto(ports.with(|fake| fake.turn_page(1)));
        }
        let page_two = ports.with(|fake| fake.knob(13).is_some_and(|knob| knob.learned.is_some()));
        if page_two && !chose {
            chose = true;
            ports.from_roto(vec![ports.with(|fake| fake.choose_plugin(1))]);
        }
        let delay_learned = ports.with(|fake| {
            fake.shown == Some((1, 0)) && fake.knob(3).is_some_and(|knob| knob.learned.is_some())
        });
        if delay_learned {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        chose,
        "both pages of the filter and then the delay were shown"
    );
    link.hand_back();
    ports.with(|fake| {
        assert!(
            fake.knob(3).unwrap().learned.is_some(),
            "the delay's knobs were learned"
        );
        assert_eq!(
            fake.received.last(),
            Some(&ToRoto::DawStarted),
            "handing it back says DAW STARTED last"
        );
        assert!(fake.setups.is_empty(), "no setup was stored");
        assert!(fake.serial_changes.is_empty(), "no serial request was sent");
        assert!(fake.unreadable.is_empty(), "{:?}", fake.unreadable);
        for message in &fake.received {
            if let ToRoto::PluginDetails(details) = message {
                assert_eq!(
                    details.kind,
                    PluginKind::Macro,
                    "a macro plugin keeps nothing on the Roto"
                );
            }
        }
    });
}

#[test]
fn a_stepped_knob_turned_shows_its_steps_name_and_a_smooth_one_nothing() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    // The Roto draws no value of its own, so each step's name is sent as the knob turns.
    assert!(view.knob_turned(0, 9000).is_empty(), "Cut is smooth");
    let chosen = fake.choose_plugin(1);
    let answers = view.receive(&FromRoto::decode(&chosen).unwrap());
    exchange(&mut view, &mut fake, answers);
    assert_eq!(
        view.knob_turned(2, KNOB_TOP),
        [ToRoto::ShowControlValue {
            control: vt_roto::sysex::ControlType::Knob,
            index: 2,
            value: Label::new("On"),
        }]
    );
    assert!(matches!(
        &view.knob_turned(2, 100)[..],
        [ToRoto::ShowControlValue { value, .. }] if value.as_str() == "Off"
    ));
    assert!(view.knob_turned(3, 0).is_empty(), "a hole says nothing");
}

#[test]
fn the_page_shown_follows_the_knobs_the_roto_asks_for_and_its_arrows() {
    let mut fake = connected();
    let mut view = PluginView::new();
    let push = view.show(devices());
    exchange(&mut view, &mut fake, push);
    // The real Roto's arrows send PAGE RIGHT, then ask for the second page's knobs, 9 to 16.
    assert!(view.receive(&FromRoto::PageRight).is_empty());
    assert_eq!(view.page(), 1);
    let turned: Vec<FromRoto> = fake
        .turn_page(1)
        .iter()
        .skip(1)
        .map(|bytes| FromRoto::decode(bytes).unwrap())
        .collect();
    let answers: Vec<ToRoto> = turned.iter().flat_map(|m| view.receive(m)).collect();
    exchange(&mut view, &mut fake, answers);
    assert_eq!(view.page(), 1);
    // The filter's type is the fifth knob of its second page, of three steps.
    assert!(matches!(
        &view.knob_turned(4, KNOB_TOP / 2)[..],
        [ToRoto::ShowControlValue { value, .. }] if value.as_str() == "HP"
    ));
    assert!(view.receive(&FromRoto::PageLeft).is_empty());
    assert_eq!(view.page(), 0);
    assert!(view.receive(&FromRoto::PageLeft).is_empty());
    assert_eq!(view.page(), 0, "no page before the first");
}
