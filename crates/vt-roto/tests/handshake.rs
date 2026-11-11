//! The handshake, the link's thread and the serial API, against the fake Roto, with no hardware
//! and no MIDI ports.

use std::time::{Duration, Instant};

use vt_roto::cc::ControlEvent;
use vt_roto::fake::{FakePorts, FakeRoto, Link as FakeLink};
use vt_roto::link::{Event, Link, Options};
use vt_roto::serial::{
    Code, KnobHaptic, MAX_PLUGIN_SETUPS, Mode, PluginKnobConfig, Reply, Request,
};
use vt_roto::session::{Change, Session, State};
use vt_roto::sysex::{Daw, FromRoto, ToRoto};
use vt_roto::{Label, ParamHash, PluginHash};

/// Passes each message between the session and the fake until neither has more to say, and
/// gives back what the session reported.
fn run_to_quiet(session: &mut Session, fake: &mut FakeRoto, first: Vec<ToRoto>) -> Vec<Change> {
    let mut to_roto = first;
    let mut changes = Vec::new();
    while !to_roto.is_empty() {
        let mut from_roto = Vec::new();
        for message in to_roto.drain(..) {
            from_roto.extend(fake.midi_in(&message.encode()));
        }
        for bytes in from_roto {
            let (replies, change) = session.receive(FromRoto::decode(&bytes).unwrap());
            to_roto.extend(replies);
            changes.extend(change);
        }
    }
    changes
}

#[test]
fn the_handshake_goes_as_the_vendors_scripts_do() {
    let mut session = Session::new(Daw::Bitwig);
    let mut fake = FakeRoto::new();
    let start = session.start();
    assert_eq!(start, [ToRoto::DawStarted]);
    assert_eq!(session.state(), State::Waiting);
    let changes = run_to_quiet(&mut session, &mut fake, start);
    assert_eq!(session.state(), State::Connected);
    assert_eq!(fake.link, FakeLink::Connected(Daw::Bitwig));
    assert_eq!(changes[0], Change::Connected);
    assert!(changes.contains(&Change::FirmwareVersion(fake.firmware.clone())));
    assert!(changes.contains(&Change::ApiVersion { major: 1, minor: 6 }));
    assert_eq!(session.api_version(), Some((1, 6)));
    // What went to the Roto, in order: started, the answer to its ping, then the two requests.
    assert_eq!(
        fake.received,
        [
            ToRoto::DawStarted,
            ToRoto::PingResponse(Daw::Bitwig),
            ToRoto::RequestFirmwareVersion,
            ToRoto::RequestApiVersion,
        ]
    );
}

#[test]
fn a_roto_plugged_in_pings_and_is_answered() {
    let mut session = Session::new(Daw::Bitwig);
    let mut fake = FakeRoto::new();
    let ping = fake.plug_in();
    let (replies, change) = session.receive(FromRoto::decode(&ping[0]).unwrap());
    assert_eq!(replies, [ToRoto::PingResponse(Daw::Bitwig)]);
    assert_eq!(change, None);
    run_to_quiet(&mut session, &mut fake, replies);
    assert!(session.is_connected());
}

#[test]
fn nothing_but_the_handshake_is_sent_or_acted_on_before_connecting() {
    let mut session = Session::new(Daw::Bitwig);
    session.start();
    assert!(!session.may_send(&ToRoto::NumTracks(4)));
    assert!(session.may_send(&ToRoto::PingResponse(Daw::Bitwig)));
    let (replies, change) = session.receive(FromRoto::SetPluginMode);
    assert!(replies.is_empty());
    assert_eq!(change, None);
}

#[test]
fn a_ping_while_connected_starts_the_handshake_again() {
    let mut session = Session::new(Daw::Bitwig);
    let mut fake = FakeRoto::new();
    let start = session.start();
    run_to_quiet(&mut session, &mut fake, start);
    // As when another program sends DAW STARTED and the Roto looks again.
    let (replies, _) = session.receive(FromRoto::PingDaw);
    assert_eq!(replies, [ToRoto::PingResponse(Daw::Bitwig)]);
    assert_eq!(session.state(), State::Waiting);
}

#[test]
fn once_connected_the_rotos_choices_reach_the_program() {
    let mut session = Session::new(Daw::Bitwig);
    let mut fake = FakeRoto::new();
    let start = session.start();
    run_to_quiet(&mut session, &mut fake, start);
    let pressed = fake.press(FromRoto::SetPluginMode);
    let (_, change) = session.receive(FromRoto::decode(&pressed).unwrap());
    assert_eq!(change, Some(Change::Message(FromRoto::SetPluginMode)));
    assert_eq!(fake.mode, Mode::Plugin);
}

fn quick() -> Options {
    Options {
        daw: Daw::Bitwig,
        watch_every: Duration::from_millis(10),
        sysex_gap: Duration::ZERO,
        handshake_timeout: Duration::from_millis(500),
    }
}

/// Waits for an event the test is looking for, gathering the rest.
fn wait_for(link: &Link, wanted: impl Fn(&Event) -> bool) -> Vec<Event> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut seen = Vec::new();
    while Instant::now() < deadline {
        if let Some(event) = link.wait(Duration::from_millis(50)) {
            let found = wanted(&event);
            seen.push(event);
            if found {
                return seen;
            }
        }
    }
    panic!("never came; saw {seen:?}");
}

#[test]
fn the_link_connects_on_its_own_thread_and_passes_on_the_knobs() {
    let ports = FakePorts::new(FakeRoto::new(), true);
    let link = Link::spawn(ports.clone(), quick()).unwrap();
    let seen = wait_for(&link, |event| {
        matches!(event, Event::Session(Change::ApiVersion { .. }))
    });
    assert_eq!(seen[0], Event::Opened);
    assert!(seen.contains(&Event::Session(Change::Connected)));

    let turn = ControlEvent::Knob {
        index: 2,
        value: 12_345,
    };
    let ccs = ports.with(|fake| fake.control(turn));
    ports.from_roto(ccs);
    wait_for(&link, |event| *event == Event::Control(turn));

    // Knob positions sent back reach the Roto.
    link.send_ccs(vt_roto::cc::knob_position(5, 9000).to_vec());
    let deadline = Instant::now() + Duration::from_secs(5);
    while ports.with(|fake| fake.knob_position(5)) != Some(9000) {
        assert!(Instant::now() < deadline, "the position never came");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn unplugged_and_plugged_back_in_the_link_connects_again() {
    let ports = FakePorts::new(FakeRoto::new(), false);
    let link = Link::spawn(ports.clone(), quick()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert!(link.poll().is_empty(), "nothing to find yet");

    ports.plug_in();
    wait_for(&link, |event| *event == Event::Session(Change::Connected));
    ports.unplug();
    wait_for(&link, |event| matches!(event, Event::Lost(_)));
    ports.plug_in();
    wait_for(&link, |event| *event == Event::Session(Change::Connected));
    // The fake saw two whole handshakes.
    let started = ports.with(|fake| {
        fake.received
            .iter()
            .filter(|message| **message == ToRoto::DawStarted)
            .count()
    });
    assert_eq!(started, 2);
}

#[test]
fn a_message_sent_once_connected_reaches_the_roto() {
    let ports = FakePorts::new(FakeRoto::new(), true);
    let link = Link::spawn(ports.clone(), quick()).unwrap();
    wait_for(&link, |event| *event == Event::Session(Change::Connected));
    link.send(ToRoto::NumTracks(3));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ports.with(|fake| fake.received.contains(&ToRoto::NumTracks(3))) {
        assert!(Instant::now() < deadline, "never sent once connected");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn knob(plugin: PluginHash, control: u8) -> PluginKnobConfig {
    PluginKnobConfig {
        plugin,
        control,
        param_index: u16::from(control),
        param_hash: ParamHash::of("time"),
        macro_param: false,
        min: 0,
        max: 16383,
        name: Label::new("Time"),
        colour: 9,
        haptic: KnobHaptic::Smooth {
            indents: [None, None],
        },
        step_names: Vec::new(),
    }
}

/// Sends a request to the fake as bytes and reads its reply, as the serial link would.
fn ask(fake: &mut FakeRoto, request: &Request) -> Result<Reply, Code> {
    let reply = fake.serial_in(&request.encode());
    request.decode_reply(&reply).unwrap()
}

#[test]
fn a_setup_is_written_in_a_config_update_and_read_back() {
    let mut fake = FakeRoto::new();
    let hash = PluginHash::of("vimtracker/fx/delay");
    let add = Request::AddPlugin {
        hash,
        name: Label::new("Delay"),
    };
    assert!(
        matches!(ask(&mut fake, &add), Err(Code::Error(_))),
        "refused outside an update"
    );
    assert_eq!(ask(&mut fake, &Request::StartConfigUpdate), Ok(Reply::Done));
    assert_eq!(ask(&mut fake, &add), Ok(Reply::Done));
    assert_eq!(ask(&mut fake, &add), Err(Code::PluginExists));
    let written = knob(hash, 9);
    assert_eq!(
        ask(&mut fake, &Request::SetPluginKnobConfig(written.clone())),
        Ok(Reply::Done)
    );
    assert_eq!(ask(&mut fake, &Request::EndConfigUpdate), Ok(Reply::Done));
    assert_eq!(
        ask(
            &mut fake,
            &Request::GetPluginKnobConfig {
                plugin: hash,
                control: 9
            }
        ),
        Ok(Reply::PluginKnob(written))
    );
    let Ok(Reply::Plugin(listed)) = ask(&mut fake, &Request::GetFirstPlugin) else {
        panic!("not listed");
    };
    assert_eq!(listed.name.as_str(), "Delay");
    assert_eq!(ask(&mut fake, &Request::GetNextPlugin), Err(Code::NoPlugin));
}

#[test]
fn the_store_holds_sixty_four_plugin_setups() {
    let mut fake = FakeRoto::new();
    ask(&mut fake, &Request::StartConfigUpdate).unwrap();
    for at in 0..MAX_PLUGIN_SETUPS {
        let add = Request::AddPlugin {
            hash: PluginHash::of(&format!("device {at}")),
            name: Label::new(&format!("Device {at}")),
        };
        assert_eq!(ask(&mut fake, &add), Ok(Reply::Done));
    }
    let one_more = Request::AddPlugin {
        hash: PluginHash::of("one more"),
        name: Label::new("One more"),
    };
    assert!(matches!(ask(&mut fake, &one_more), Err(Code::Error(_))));
}

#[test]
fn the_fake_doesnt_say_its_mode_on_connecting_as_the_real_one_doesnt() {
    let mut session = Session::new(Daw::Bitwig);
    let mut fake = FakeRoto::new();
    let start = session.start();
    let changes = run_to_quiet(&mut session, &mut fake, start);
    assert!(
        !changes
            .iter()
            .any(|change| matches!(change, Change::Message(_)))
    );
    // The serial API's GET MODE says it instead.
    assert_eq!(
        ask(&mut fake, &Request::GetMode),
        Ok(Reply::Mode {
            mode: Mode::Mix,
            page: 0
        })
    );
}
