//! Every message of both protocols written and read back, each checked to cover every kind there
//! is, so a new command can't go untested.

use std::collections::BTreeSet;

use vt_roto::cc::{self, ControlDecoder, ControlEvent};
use vt_roto::serial::{
    Code, KnobHaptic, KnobMessage, MidiKnobConfig, MidiSwitchConfig, Mode, Notice,
    PluginKnobConfig, PluginSwitchConfig, Reply, Request, StoredKind, StoredPlugin, SwitchHaptic,
    SwitchMessage,
};
use vt_roto::sysex::{
    AllTracks, ControlType, Daw, FirmwareVersion, FromRoto, KnobMode, LearnParam, PluginDetails,
    PluginKind, SwitchMode, ToRoto, TrackDetails, TransportStatus,
};
use vt_roto::{Label, ParamHash, PluginHash, Rgb};

fn track(index: u16) -> TrackDetails {
    TrackDetails {
        index,
        name: Label::new("Bass"),
        colour: 23,
        grouped: index.is_multiple_of(2),
    }
}

fn names(count: usize) -> Vec<Label> {
    (0..count)
        .map(|at| Label::new(&format!("Step {at}")))
        .collect()
}

/// Which kind each is, by its place in its enum, so the samples can be checked to cover them all.
fn to_roto_kind(message: &ToRoto) -> usize {
    match message {
        ToRoto::DawStarted => 0,
        ToRoto::PingResponse(_) => 1,
        ToRoto::NumTracks(_) => 2,
        ToRoto::FirstTrack(_) => 3,
        ToRoto::TrackDetails(_) => 4,
        ToRoto::TrackDetailsEnd => 5,
        ToRoto::TransportStatus(_) => 6,
        ToRoto::RequestFirmwareVersion => 7,
        ToRoto::RequestApiVersion => 8,
        ToRoto::SetTrackDetails(_) => 9,
        ToRoto::ResetTrackDetails(_) => 10,
        ToRoto::SetTrackColour { .. } => 11,
        ToRoto::SetCurrentTrackName(_) => 12,
        ToRoto::SetCurrentTrackColour(_) => 13,
        ToRoto::ShowControlValue { .. } => 14,
        ToRoto::NumPlugins(_) => 15,
        ToRoto::FirstPlugin(_) => 16,
        ToRoto::PluginDetails(_) => 17,
        ToRoto::PluginDetailsEnd => 18,
        ToRoto::DawSelectPlugin { .. } => 19,
        ToRoto::LearnParam(_) => 20,
        ToRoto::UnmapControl { .. } => 21,
        ToRoto::SetMappedControlName { .. } => 22,
        ToRoto::SetPluginControlDetails { .. } => 23,
        ToRoto::ResetPluginControlDetails { .. } => 24,
        ToRoto::PluginParamSweep(_) => 25,
        ToRoto::PluginParamSweepRestart(_) => 26,
        ToRoto::NumSendTracks(_) => 27,
        ToRoto::DawSelectTrack(_) => 28,
        ToRoto::SendTrackNames { .. } => 29,
        ToRoto::RefreshMixMode => 30,
        ToRoto::DawSelectFocusTrack { .. } => 31,
        ToRoto::SetVuMeterThresholds { .. } => 32,
        ToRoto::SetVuMeterStates(_) => 33,
    }
}
const TO_ROTO_KINDS: usize = 34;

fn from_roto_kind(message: &FromRoto) -> usize {
    match message {
        FromRoto::PingDaw => 0,
        FromRoto::SetFirstTrack(_) => 1,
        FromRoto::RotoSelectTrack(_) => 2,
        FromRoto::RequestTransportStatus => 3,
        FromRoto::Connected => 4,
        FromRoto::FirmwareVersion(_) => 5,
        FromRoto::ApiVersion { .. } => 6,
        FromRoto::PageLeft => 7,
        FromRoto::PageRight => 8,
        FromRoto::SetPluginMode => 9,
        FromRoto::SetFirstPlugin(_) => 10,
        FromRoto::RotoSelectPlugin(_) => 11,
        FromRoto::SetDeviceLearn(_) => 12,
        FromRoto::ControlMapped { .. } => 13,
        FromRoto::SetPluginEnabled { .. } => 14,
        FromRoto::SetPluginsLock(_) => 15,
        FromRoto::MacroPluginPageIndex(_) => 16,
        FromRoto::ControlLearned { .. } => 17,
        FromRoto::ToggleRemotePage => 18,
        FromRoto::SetTrackSelectMode => 19,
        FromRoto::SetPluginEnableMode => 20,
        FromRoto::SetPluginSelectMode => 21,
        FromRoto::PluginParamSweepValue { .. } => 22,
        FromRoto::PluginParamSweepEnd => 23,
        FromRoto::SetMixAllTracksMode { .. } => 24,
        FromRoto::SetMixTrackMode(_) => 25,
        FromRoto::SetAllTracksMode(_) => 26,
        FromRoto::ToggleTrackGrouping(_) => 27,
        FromRoto::RequestSendTrackNames(_) => 28,
    }
}
const FROM_ROTO_KINDS: usize = 29;

fn to_roto_samples() -> Vec<ToRoto> {
    let hash = ParamHash::of("vimtracker/cut");
    vec![
        ToRoto::DawStarted,
        ToRoto::PingResponse(Daw::Ableton),
        ToRoto::PingResponse(Daw::Bitwig),
        ToRoto::PingResponse(Daw::Logic),
        ToRoto::NumTracks(300),
        ToRoto::FirstTrack(8),
        ToRoto::TrackDetails(track(130)),
        ToRoto::TrackDetailsEnd,
        ToRoto::TransportStatus(TransportStatus {
            playing: true,
            recording: false,
            session_recording: true,
            looping: true,
            punch_in: false,
            punch_out: true,
            automation_changed: false,
        }),
        ToRoto::RequestFirmwareVersion,
        ToRoto::RequestApiVersion,
        ToRoto::SetTrackDetails(track(3)),
        ToRoto::ResetTrackDetails(200),
        ToRoto::SetTrackColour {
            index: 9,
            colour: Rgb::new(0xFF, 0x80, 0x01),
        },
        ToRoto::SetCurrentTrackName(Label::new("Lead")),
        ToRoto::SetCurrentTrackColour(Rgb::new(0, 0xC0, 0xFF)),
        ToRoto::ShowControlValue {
            control: ControlType::Knob,
            index: 7,
            value: Label::new("-14.8 dB"),
        },
        ToRoto::ShowControlValue {
            control: ControlType::Switch,
            index: 0,
            value: Label::new("Overdrive"),
        },
        ToRoto::NumPlugins(11),
        ToRoto::FirstPlugin(8),
        ToRoto::PluginDetails(PluginDetails {
            index: 2,
            hash: PluginHash::of("vimtracker/fx/delay"),
            enabled: false,
            name: Label::new("Delay"),
            kind: PluginKind::Macro,
            macro_pages: 2,
        }),
        ToRoto::PluginDetails(PluginDetails {
            index: 0,
            hash: PluginHash::of("vimtracker/machine/acid"),
            enabled: true,
            name: Label::new("Acid"),
            kind: PluginKind::Normal,
            macro_pages: 0,
        }),
        ToRoto::PluginDetailsEnd,
        ToRoto::DawSelectPlugin {
            index: 3,
            macro_page: 1,
            force: true,
        },
        ToRoto::LearnParam(LearnParam {
            index: 200,
            hash,
            macro_param: true,
            centre_indent: true,
            steps: 0,
            position: 8192,
            name: Label::new("Pan"),
            step_names: Vec::new(),
        }),
        ToRoto::LearnParam(LearnParam {
            index: 4,
            hash,
            macro_param: false,
            centre_indent: false,
            steps: 16,
            position: 16383,
            name: Label::new("Type"),
            step_names: names(16),
        }),
        ToRoto::LearnParam(LearnParam {
            index: 5,
            hash,
            macro_param: true,
            centre_indent: false,
            steps: 24,
            position: 0,
            name: Label::new("Division"),
            step_names: Vec::new(),
        }),
        ToRoto::UnmapControl {
            control: ControlType::Switch,
            index: 5,
        },
        ToRoto::SetMappedControlName {
            index: 129,
            hash,
            name: Label::new("Feedback"),
        },
        ToRoto::SetPluginControlDetails {
            control: ControlType::Knob,
            index: 0x3F,
            name: Label::new("Mix"),
            colour: 0x52,
        },
        ToRoto::ResetPluginControlDetails {
            control: ControlType::Knob,
            index: 1,
        },
        ToRoto::PluginParamSweep(1000),
        ToRoto::PluginParamSweepRestart(2),
        ToRoto::NumSendTracks(2),
        ToRoto::DawSelectTrack(track(0)),
        ToRoto::SendTrackNames {
            first: 8,
            names: std::array::from_fn(|at| {
                if at < 2 {
                    Label::new(&format!("Return {at}"))
                } else {
                    Label::empty()
                }
            }),
        },
        ToRoto::RefreshMixMode,
        ToRoto::DawSelectFocusTrack {
            index: 17,
            colour: Rgb::new(10, 20, 30),
        },
        ToRoto::SetVuMeterThresholds {
            yellow: 87,
            red: 113,
        },
        ToRoto::SetVuMeterStates([true, false, true, true, false, false, true, false]),
    ]
}

fn from_roto_samples() -> Vec<FromRoto> {
    let hash = ParamHash::of("vimtracker/res");
    vec![
        FromRoto::PingDaw,
        FromRoto::SetFirstTrack(16),
        FromRoto::RotoSelectTrack(129),
        FromRoto::RequestTransportStatus,
        FromRoto::Connected,
        FromRoto::FirmwareVersion(FirmwareVersion {
            major: 3,
            minor: 3,
            patch: 0,
            commit: "eaa37b0".to_owned(),
        }),
        FromRoto::ApiVersion { major: 1, minor: 6 },
        FromRoto::PageLeft,
        FromRoto::PageRight,
        FromRoto::SetPluginMode,
        FromRoto::SetFirstPlugin(8),
        FromRoto::RotoSelectPlugin(3),
        FromRoto::SetDeviceLearn(true),
        FromRoto::ControlMapped {
            index: 300,
            hash,
            control: ControlType::Switch,
            control_index: 7,
            macro_param: true,
        },
        FromRoto::SetPluginEnabled {
            index: 1,
            enabled: false,
        },
        FromRoto::SetPluginsLock(true),
        FromRoto::MacroPluginPageIndex(1),
        FromRoto::ControlLearned {
            control: ControlType::Knob,
            index: 0x3F,
        },
        FromRoto::ToggleRemotePage,
        FromRoto::SetTrackSelectMode,
        FromRoto::SetPluginEnableMode,
        FromRoto::SetPluginSelectMode,
        FromRoto::PluginParamSweepValue {
            channel: 15,
            param: 1000,
            value: 16000,
        },
        FromRoto::PluginParamSweepEnd,
        FromRoto::SetMixAllTracksMode {
            tracks: AllTracks::MasterReturn,
            knobs: KnobMode::Send,
            switches: SwitchMode::Solo,
            send: 1,
        },
        FromRoto::SetMixAllTracksMode {
            tracks: AllTracks::Audio,
            knobs: KnobMode::Pan,
            switches: SwitchMode::Arm,
            send: 0,
        },
        FromRoto::SetMixAllTracksMode {
            tracks: AllTracks::Audio,
            knobs: KnobMode::Level,
            switches: SwitchMode::Mute,
            send: 0,
        },
        FromRoto::SetMixTrackMode(8),
        FromRoto::SetAllTracksMode(AllTracks::Audio),
        FromRoto::ToggleTrackGrouping(2),
        FromRoto::RequestSendTrackNames(9),
    ]
}

#[test]
fn every_message_to_the_roto_reads_back_as_it_was_written() {
    let samples = to_roto_samples();
    let kinds: BTreeSet<usize> = samples.iter().map(to_roto_kind).collect();
    assert_eq!(
        kinds.len(),
        TO_ROTO_KINDS,
        "a kind of message has no sample"
    );
    for message in samples {
        let bytes = message.encode();
        assert!(
            bytes[1..bytes.len() - 1].iter().all(|&byte| byte < 0x80),
            "{message:?} has a byte SysEx can't carry"
        );
        assert_eq!(ToRoto::decode(&bytes), Ok(message.clone()), "{message:?}");
    }
}

#[test]
fn every_message_from_the_roto_reads_back_as_it_was_written() {
    let samples = from_roto_samples();
    let kinds: BTreeSet<usize> = samples.iter().map(from_roto_kind).collect();
    assert_eq!(
        kinds.len(),
        FROM_ROTO_KINDS,
        "a kind of message has no sample"
    );
    for message in samples {
        let bytes = message.encode();
        assert!(bytes[1..bytes.len() - 1].iter().all(|&byte| byte < 0x80));
        assert_eq!(FromRoto::decode(&bytes), Ok(message.clone()), "{message:?}");
    }
}

#[test]
fn the_documented_bytes_of_a_few_messages() {
    let header = [0xF0, 0x00, 0x22, 0x03, 0x02];
    let with = |data: &[u8]| [&header[..], data, &[0xF7]].concat();
    assert_eq!(FromRoto::PingDaw.encode(), with(&[0x0A, 0x02]));
    assert_eq!(FromRoto::Connected.encode(), with(&[0x0A, 0x0C]));
    assert_eq!(
        ToRoto::NumTracks(130).encode(),
        with(&[0x0A, 0x04, 0x01, 0x02])
    );
    assert_eq!(
        ToRoto::SetVuMeterThresholds {
            yellow: 87,
            red: 113
        }
        .encode(),
        with(&[0x0C, 0x0B, 87, 113])
    );
}

fn plugin_knob(haptic: KnobHaptic, step_names: Vec<Label>) -> PluginKnobConfig {
    PluginKnobConfig {
        plugin: PluginHash::of("vimtracker/fx/delay"),
        control: 9,
        param_index: 22,
        param_hash: ParamHash::of("feedback"),
        macro_param: false,
        min: 0,
        max: 16383,
        name: Label::new("Feedback"),
        colour: 0x40,
        haptic,
        step_names,
    }
}

fn plugin_switch(steps: u8) -> PluginSwitchConfig {
    PluginSwitchConfig {
        plugin: PluginHash::of("vimtracker/fx/delay"),
        control: 63,
        param_index: 1,
        param_hash: ParamHash::of("sync"),
        min: 0,
        max: 1,
        name: Label::new("Sync"),
        colour: 3,
        light_on: 13,
        light_off: 70,
        haptic: SwitchHaptic::Toggle,
        steps,
        step_names: names(usize::from(steps)),
    }
}

fn midi_knob() -> MidiKnobConfig {
    MidiKnobConfig {
        setup: 63,
        control: 31,
        message: KnobMessage::Nrpn14,
        channel: 16,
        param: 0,
        nrpn: 0x1234,
        min: 0,
        max: 16383,
        name: Label::new("Cutoff"),
        colour: 1,
        haptic: KnobHaptic::Steps(4),
        step_names: names(4),
    }
}

fn midi_switch() -> MidiSwitchConfig {
    MidiSwitchConfig {
        setup: 0,
        control: 0,
        message: SwitchMessage::ProgramChange,
        channel: 1,
        param: 12,
        nrpn: 0xFFFF,
        min: 0,
        max: 127,
        name: Label::new("Patch"),
        colour: 1,
        light_on: 2,
        light_off: 3,
        haptic: SwitchHaptic::Push,
        steps: 0,
        step_names: Vec::new(),
    }
}

fn request_kind(request: &Request) -> usize {
    match request {
        Request::GetFirmwareVersion => 0,
        Request::GetMode => 1,
        Request::SetMode { .. } => 2,
        Request::StartConfigUpdate => 3,
        Request::EndConfigUpdate => 4,
        Request::FactoryReset => 5,
        Request::GetCurrentSetup => 6,
        Request::GetSetup(_) => 7,
        Request::SetSetup(_) => 8,
        Request::SetSetupName { .. } => 9,
        Request::GetKnobControlConfig { .. } => 10,
        Request::GetSwitchControlConfig { .. } => 11,
        Request::SetKnobControlConfig(_) => 12,
        Request::SetSwitchControlConfig(_) => 13,
        Request::ClearControlConfig { .. } => 14,
        Request::ClearMidiSetup(_) => 15,
        Request::GetCurrentPlugin => 16,
        Request::GetFirstPlugin => 17,
        Request::GetNextPlugin => 18,
        Request::GetPlugin(_) => 19,
        Request::AddPlugin { .. } => 20,
        Request::SetPluginName { .. } => 21,
        Request::ClearPlugin(_) => 22,
        Request::GetPluginKnobConfig { .. } => 23,
        Request::GetPluginSwitchConfig { .. } => 24,
        Request::SetPluginKnobConfig(_) => 25,
        Request::SetPluginSwitchConfig(_) => 26,
        Request::ClearPluginControlConfig { .. } => 27,
    }
}
const REQUEST_KINDS: usize = 28;

/// Each request with a reply it could have.
fn exchanges() -> Vec<(Request, Reply)> {
    let hash = PluginHash::of("vimtracker/fx/delay");
    let stored = StoredPlugin {
        hash,
        name: Label::new("Delay"),
        kind: StoredKind::MacroAutoConfig,
    };
    vec![
        (
            Request::GetFirmwareVersion,
            Reply::FirmwareVersion(FirmwareVersion {
                major: 3,
                minor: 3,
                patch: 0,
                commit: "eaa37b0".to_owned(),
            }),
        ),
        (
            Request::GetMode,
            Reply::Mode {
                mode: Mode::Plugin,
                page: 8,
            },
        ),
        (
            Request::SetMode {
                mode: Mode::Midi,
                page: 0,
            },
            Reply::Done,
        ),
        (Request::StartConfigUpdate, Reply::Done),
        (Request::EndConfigUpdate, Reply::Done),
        (Request::FactoryReset, Reply::Done),
        (
            Request::GetCurrentSetup,
            Reply::Setup {
                index: 2,
                name: Label::new("SETUP 03"),
            },
        ),
        (
            Request::GetSetup(63),
            Reply::Setup {
                index: 63,
                name: Label::new("SETUP 64"),
            },
        ),
        (Request::SetSetup(1), Reply::Done),
        (
            Request::SetSetupName {
                setup: 4,
                name: Label::new("Synths"),
            },
            Reply::Done,
        ),
        (
            Request::GetKnobControlConfig {
                setup: 63,
                control: 31,
            },
            Reply::MidiKnob(midi_knob()),
        ),
        (
            Request::GetSwitchControlConfig {
                setup: 0,
                control: 0,
            },
            Reply::MidiSwitch(midi_switch()),
        ),
        (Request::SetKnobControlConfig(midi_knob()), Reply::Done),
        (Request::SetSwitchControlConfig(midi_switch()), Reply::Done),
        (
            Request::ClearControlConfig {
                setup: 1,
                control: ControlType::Switch,
                index: 31,
            },
            Reply::Done,
        ),
        (Request::ClearMidiSetup(5), Reply::Done),
        (
            Request::GetCurrentPlugin,
            Reply::CurrentPlugin {
                plugin: stored.clone(),
                daw: Daw::Bitwig,
            },
        ),
        (Request::GetFirstPlugin, Reply::Plugin(stored.clone())),
        (Request::GetNextPlugin, Reply::Plugin(stored.clone())),
        (Request::GetPlugin(hash), Reply::Plugin(stored)),
        (
            Request::AddPlugin {
                hash,
                name: Label::new("Delay"),
            },
            Reply::Done,
        ),
        (
            Request::SetPluginName {
                hash,
                name: Label::new("Echo"),
            },
            Reply::Done,
        ),
        (Request::ClearPlugin(hash), Reply::Done),
        (
            Request::GetPluginKnobConfig {
                plugin: hash,
                control: 9,
            },
            Reply::PluginKnob(plugin_knob(KnobHaptic::Steps(16), names(16))),
        ),
        (
            Request::GetPluginKnobConfig {
                plugin: hash,
                control: 9,
            },
            Reply::PluginKnob(plugin_knob(
                KnobHaptic::Smooth {
                    indents: [Some(64), None],
                },
                Vec::new(),
            )),
        ),
        (
            Request::GetPluginKnobConfig {
                plugin: hash,
                control: 9,
            },
            Reply::PluginKnob(plugin_knob(KnobHaptic::CentreIndent, Vec::new())),
        ),
        (
            Request::GetPluginSwitchConfig {
                plugin: hash,
                control: 63,
            },
            Reply::PluginSwitch(plugin_switch(3)),
        ),
        (
            Request::SetPluginKnobConfig(plugin_knob(KnobHaptic::Steps(3), names(3))),
            Reply::Done,
        ),
        (
            Request::SetPluginKnobConfig(plugin_knob(KnobHaptic::CentreIndent, Vec::new())),
            Reply::Done,
        ),
        (
            Request::SetPluginSwitchConfig(plugin_switch(0)),
            Reply::Done,
        ),
        (
            Request::SetPluginSwitchConfig(plugin_switch(16)),
            Reply::Done,
        ),
        (
            Request::ClearPluginControlConfig {
                plugin: hash,
                control: ControlType::Knob,
                index: 63,
            },
            Reply::Done,
        ),
    ]
}

#[test]
fn every_serial_request_and_its_reply_read_back() {
    let exchanges = exchanges();
    let kinds: BTreeSet<usize> = exchanges
        .iter()
        .map(|(request, _)| request_kind(request))
        .collect();
    assert_eq!(
        kinds.len(),
        REQUEST_KINDS,
        "a kind of request has no sample"
    );
    for (request, reply) in exchanges {
        let frame = request.encode();
        assert_eq!(
            vt_roto::serial::frame_len(&frame),
            Some(frame.len()),
            "{request:?}'s length"
        );
        assert_eq!(Request::decode(&frame), Ok(request.clone()), "{request:?}");
        let bytes = reply.encode();
        assert_eq!(
            bytes.len(),
            2 + request.reply_len(),
            "{request:?}'s reply length"
        );
        assert_eq!(request.decode_reply(&bytes), Ok(Ok(reply)), "{request:?}");
    }
}

#[test]
fn a_failed_reply_reads_back_with_its_code() {
    for code in [Code::NoPlugin, Code::PluginExists, Code::Error(7)] {
        let bytes = vt_roto::serial::encode_failure(code);
        assert_eq!(Request::GetFirstPlugin.decode_reply(&bytes), Ok(Err(code)));
    }
}

#[test]
fn every_serial_notice_reads_back() {
    let notices = [
        Notice::ModeSet {
            mode: Mode::Mix,
            page: 8,
        },
        Notice::SetupSelected(12),
        Notice::MidiControlLearned {
            setup: 1,
            control: ControlType::Knob,
            index: 31,
        },
        Notice::PluginSelected(Some(PluginHash::of("x"))),
        Notice::PluginSelected(None),
        Notice::PluginControlLearned {
            plugin: PluginHash::of("x"),
            control: ControlType::Switch,
            index: 2,
        },
    ];
    for notice in notices {
        assert_eq!(
            Notice::decode(&notice.encode()),
            Ok(notice.clone()),
            "{notice:?}"
        );
    }
}

/// A real Roto's reply to `GET PLUGIN KNOB CONFIG` for a stored Ableton setup's first knob
/// (firmware 3.3.0), read with `vimtracker roto-probe`: its first 60 bytes, as the probe printed
/// them, padded with zeros to the 248 the reply had, as the rest wasn't printed.
#[test]
fn a_real_rotos_stored_knob_reads() {
    let start = [
        0xA5, 0x00, 0x48, 0x76, 0x77, 0x39, 0x67, 0x79, 0x2C, 0x29, 0x00, 0x00, 0x16, 0x47, 0x54,
        0x4D, 0x18, 0x46, 0x16, 0x00, 0x00, 0x00, 0x3F, 0xFF, 0x4F, 0x73, 0x63, 0x20, 0x31, 0x20,
        0x4F, 0x63, 0x74, 0x00, 0x00, 0x00, 0x00, 0x40, 0x01, 0xFF, 0xFF, 0x06, 0x2D, 0x32, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2D, 0x31, 0x00, 0x00, 0x00,
    ];
    let mut reply = start.to_vec();
    reply.resize(2 + 248, 0);
    let request = Request::GetPluginKnobConfig {
        plugin: PluginHash([0x48, 0x76, 0x77, 0x39, 0x67, 0x79, 0x2C, 0x29]),
        control: 0,
    };
    let Ok(Ok(Reply::PluginKnob(knob))) = request.decode_reply(&reply) else {
        panic!("couldn't read the real reply");
    };
    assert_eq!(knob.name.as_str(), "Osc 1 Oct");
    assert_eq!(knob.param_index, 22);
    assert_eq!((knob.min, knob.max), (0, 16383));
    assert_eq!(knob.colour, 0x40);
    assert_eq!(knob.haptic, KnobHaptic::Steps(6));
    assert_eq!(knob.step_names[0].as_str(), "-2");
    assert_eq!(knob.step_names[1].as_str(), "-1");
}

#[test]
fn every_control_reads_back_from_its_ccs() {
    let mut events = Vec::new();
    for index in 0..cc::KNOBS {
        events.push(ControlEvent::Knob {
            index,
            value: 16383 - u16::from(index) * 1000,
        });
        events.push(ControlEvent::Switch {
            index,
            value: cc::ON,
        });
        events.push(ControlEvent::Switch {
            index,
            value: cc::OFF,
        });
        events.push(ControlEvent::Touch {
            index,
            touched: true,
        });
        events.push(ControlEvent::Touch {
            index,
            touched: false,
        });
        events.push(ControlEvent::Default { index });
    }
    for index in 0..cc::TRANSPORT_BUTTONS {
        events.push(ControlEvent::Transport {
            index,
            value: cc::ON,
        });
    }
    let mut decoder = ControlDecoder::new();
    for event in events {
        let read: Vec<ControlEvent> = cc::encode(event)
            .iter()
            .filter_map(|message| decoder.feed(message))
            .collect();
        assert_eq!(read, [event], "{event:?}");
    }
}
