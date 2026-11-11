//! A Roto-Control in memory, which speaks both its protocols as the real one does, so the link and
//! everything built on it are tested with no hardware and no MIDI ports. It answers the handshake,
//! the version requests and every serial request, keeps PLUGIN setups up to the real one's 64, and
//! refuses changes outside a config update. Turning a knob or pressing a switch on it gives the
//! CCs the real one sends, and it keeps what the program sends it, to check.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::bytes::{Label, ParamHash, PluginHash};
use crate::cc::{self, ControlEvent};
use crate::serial::{
    self, Code, MAX_PLUGIN_SETUPS, MIDI_SETUPS, Mode, Notice, PluginKnobConfig, PluginSwitchConfig,
    Reply, Request, StoredKind, StoredPlugin,
};
use crate::sysex::{
    CONTROLS, ControlType, Daw, FirmwareVersion, FromRoto, LearnParam, PluginDetails, PluginKind,
    ToRoto, TrackDetails,
};
use crate::transport::{Deliver, LinkError, MidiSend, Ports};

/// What the fake says its firmware is, unless told otherwise.
pub fn default_firmware() -> FirmwareVersion {
    FirmwareVersion {
        major: 3,
        minor: 3,
        patch: 0,
        commit: "fake000".to_owned(),
    }
}

/// One stored PLUGIN setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSetup {
    pub plugin: StoredPlugin,
    pub knobs: BTreeMap<u8, PluginKnobConfig>,
    pub switches: BTreeMap<u8, PluginSwitchConfig>,
}

/// One knob of a live device as the fake shows it: what `LEARN PARAM` and `SET PLUGIN CONTROL
/// DETAILS` said of it, or nothing if it was unmapped.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LiveKnob {
    pub learned: Option<LearnParam>,
    pub name: Label,
    /// The real Roto's colour for a control nothing has coloured, scheme 21.
    pub colour: u8,
    pub unmapped: bool,
}

/// The colour the real Roto gives a live control until it's told one.
pub const DEFAULT_CONTROL_COLOUR: u8 = 21;

/// Where the fake's handshake is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    /// No program has answered its ping.
    Searching,
    /// Connected to a program, which said it was this one.
    Connected(Daw),
}

/// The fake Roto.
#[derive(Debug, Clone)]
pub struct FakeRoto {
    pub firmware: FirmwareVersion,
    pub api: (u8, u8),
    pub link: Link,
    pub mode: Mode,
    pub page: u8,
    pub setups: Vec<StoredSetup>,
    pub midi_setup: u8,
    pub midi_setup_names: Vec<Label>,
    /// Whether a config update is open.
    pub updating: bool,
    /// Where `GET NEXT PLUGIN` is in the list.
    next_plugin: usize,
    /// Every SysEx message the program sent, read.
    pub received: Vec<ToRoto>,
    /// Every CC the program sent, as knob positions, switch lights and meters.
    pub received_ccs: Vec<[u8; 3]>,
    /// Every message the fake couldn't read.
    pub unreadable: Vec<Vec<u8>>,
    /// Whether it says which mode it's in once connected. The real one doesn't (firmware 3.3.0):
    /// it says so only when MODE is pressed, and the serial API's `GET MODE` reads it.
    pub announces_mode: bool,
    /// The plugin list the program last sent, by index.
    pub plugins: BTreeMap<u8, PluginDetails>,
    /// The first plugin of the bank the program last said was shown.
    pub first_plugin: u8,
    /// The macro plugin shown in PLUGIN mode, and its page, once the program has chosen one.
    pub shown: Option<(u8, u8)>,
    /// The shown plugin's knobs, by the number the Roto gives each, from 1 across its pages.
    pub knobs: BTreeMap<u16, LiveKnob>,
    /// Every serial request that would change what it stores, whether or not it took it.
    pub serial_changes: Vec<Request>,
    /// The track list the program last sent, by index, which FUNC shows in PLUGIN mode.
    pub tracks: BTreeMap<u16, TrackDetails>,
    /// The track the program last said was chosen.
    pub selected_track: Option<TrackDetails>,
}

impl Default for FakeRoto {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeRoto {
    pub fn new() -> Self {
        Self {
            firmware: default_firmware(),
            api: (1, 6),
            link: Link::Searching,
            mode: Mode::Mix,
            page: 0,
            setups: Vec::new(),
            midi_setup: 0,
            midi_setup_names: (1..=MIDI_SETUPS)
                .map(|at| Label::new(&format!("SETUP {at:02}")))
                .collect(),
            updating: false,
            next_plugin: 0,
            received: Vec::new(),
            received_ccs: Vec::new(),
            unreadable: Vec::new(),
            announces_mode: false,
            plugins: BTreeMap::new(),
            first_plugin: 0,
            shown: None,
            knobs: BTreeMap::new(),
            serial_changes: Vec::new(),
            tracks: BTreeMap::new(),
            selected_track: None,
        }
    }

    /// The hash the fake gives a live knob, as the real one gives each a hash of its own.
    pub fn knob_hash(index: u16) -> ParamHash {
        ParamHash::of(&format!("roto macro {index}"))
    }

    /// A shown knob, by the number the Roto gives it.
    pub fn knob(&self, index: u16) -> Option<&LiveKnob> {
        self.knobs.get(&index)
    }

    /// What the real one sends when it shows a macro plugin's page: that it's in PLUGIN mode, then
    /// `CONTROL MAPPED` for each knob of the page, numbered from 1 across the pages.
    fn map_page(&mut self, page: u8) -> Vec<FromRoto> {
        let mut messages = vec![FromRoto::SetPluginMode];
        for control_index in 0..CONTROLS as u8 {
            let index = u16::from(page) * CONTROLS as u16 + u16::from(control_index) + 1;
            self.knobs.entry(index).or_insert_with(|| LiveKnob {
                colour: DEFAULT_CONTROL_COLOUR,
                ..LiveKnob::default()
            });
            messages.push(FromRoto::ControlMapped {
                index,
                hash: Self::knob_hash(index),
                control: ControlType::Knob,
                control_index,
                macro_param: true,
            });
        }
        messages
    }

    /// The numbers of the shown page's knobs.
    fn shown_page(&self) -> Option<std::ops::RangeInclusive<u16>> {
        let (_, page) = self.shown?;
        let first = u16::from(page) * CONTROLS as u16 + 1;
        Some(first..=first + CONTROLS as u16 - 1)
    }

    /// The arrows turned to another page of the shown macro plugin: it says so, and asks for that
    /// page's knobs.
    pub fn turn_page(&mut self, page: u8) -> Vec<Vec<u8>> {
        let Some((plugin, _)) = self.shown else {
            return Vec::new();
        };
        self.shown = Some((plugin, page));
        let mut messages = vec![FromRoto::MacroPluginPageIndex(page)];
        messages.extend(self.map_page(page).into_iter().skip(1));
        messages.iter().map(FromRoto::encode).collect()
    }

    /// Another plugin chosen with SEL, which the program answers by choosing it.
    pub fn choose_plugin(&mut self, index: u8) -> Vec<u8> {
        FromRoto::RotoSelectPlugin(index).encode()
    }

    /// Plugged into a computer: it starts looking for a program.
    pub fn plug_in(&mut self) -> Vec<Vec<u8>> {
        self.link = Link::Searching;
        vec![FromRoto::PingDaw.encode()]
    }

    /// The message that says which mode it's in, as it sends when MODE is pressed.
    pub fn mode_message(&self) -> FromRoto {
        match self.mode {
            Mode::Plugin => FromRoto::SetPluginMode,
            Mode::Mix | Mode::Midi => FromRoto::SetMixAllTracksMode {
                tracks: crate::sysex::AllTracks::Audio,
                knobs: crate::sysex::KnobMode::Level,
                switches: crate::sysex::SwitchMode::Mute,
                send: 0,
            },
        }
    }

    /// A MIDI message from the program, and what the fake sends back.
    pub fn midi_in(&mut self, message: &[u8]) -> Vec<Vec<u8>> {
        if let [status, _, _] = *message
            && status == cc::CONTROL_CHANGE
        {
            self.received_ccs.push([message[0], message[1], message[2]]);
            return Vec::new();
        }
        let message = match ToRoto::decode(message) {
            Ok(message) => message,
            Err(_) => {
                self.unreadable.push(message.to_vec());
                return Vec::new();
            }
        };
        self.received.push(message.clone());
        let replies = match message {
            ToRoto::DawStarted => {
                self.link = Link::Searching;
                vec![FromRoto::PingDaw]
            }
            ToRoto::PingResponse(daw) => {
                self.link = Link::Connected(daw);
                // A new session shows its plugins afresh.
                self.shown = None;
                let mut replies = vec![FromRoto::Connected];
                if self.announces_mode {
                    replies.push(self.mode_message());
                }
                replies
            }
            // The real one ignores what comes before the handshake is done.
            _ if !matches!(self.link, Link::Connected(_)) => Vec::new(),
            ToRoto::RequestFirmwareVersion => {
                vec![FromRoto::FirmwareVersion(self.firmware.clone())]
            }
            ToRoto::RequestApiVersion => vec![FromRoto::ApiVersion {
                major: self.api.0,
                minor: self.api.1,
            }],
            ToRoto::PluginDetails(details) => {
                self.plugins.insert(details.index, details);
                Vec::new()
            }
            ToRoto::NumPlugins(count) => {
                self.plugins.retain(|index, _| *index < count);
                Vec::new()
            }
            ToRoto::FirstPlugin(first) => {
                self.first_plugin = first;
                Vec::new()
            }
            ToRoto::NumTracks(count) => {
                self.tracks.retain(|index, _| *index < count);
                Vec::new()
            }
            ToRoto::TrackDetails(details) => {
                self.tracks.insert(details.index, details);
                Vec::new()
            }
            ToRoto::DawSelectTrack(details) => {
                self.selected_track = Some(details);
                Vec::new()
            }
            // The real one answers only a change of what's shown, in PLUGIN mode, of a macro
            // plugin.
            ToRoto::DawSelectPlugin {
                index, macro_page, ..
            } if self.mode == Mode::Plugin
                && self.shown != Some((index, macro_page))
                && self
                    .plugins
                    .get(&index)
                    .is_some_and(|plugin| plugin.kind == PluginKind::Macro) =>
            {
                if self.shown.is_none_or(|(shown, _)| shown != index) {
                    self.knobs.clear();
                }
                self.shown = Some((index, macro_page));
                self.map_page(macro_page)
            }
            // Only for a knob of the page shown, as the real one drops the rest.
            ToRoto::LearnParam(param)
                if self
                    .shown_page()
                    .is_some_and(|page| page.contains(&param.index)) =>
            {
                let knob = self.knobs.entry(param.index).or_default();
                knob.name = param.name.clone();
                knob.unmapped = false;
                knob.learned = Some(param);
                Vec::new()
            }
            ToRoto::SetPluginControlDetails {
                control: ControlType::Knob,
                index,
                name,
                colour,
            } if self.shown.is_some() => {
                let knob = self.knobs.entry(u16::from(index)).or_default();
                knob.name = name;
                knob.colour = colour;
                Vec::new()
            }
            ToRoto::UnmapControl {
                control: ControlType::Knob,
                index,
            } => {
                if let Some(page) = self.shown_page() {
                    let knob = self
                        .knobs
                        .entry(page.start() + u16::from(index))
                        .or_default();
                    *knob = LiveKnob {
                        unmapped: true,
                        colour: DEFAULT_CONTROL_COLOUR,
                        ..LiveKnob::default()
                    };
                }
                Vec::new()
            }
            _ => Vec::new(),
        };
        replies.iter().map(FromRoto::encode).collect()
    }

    /// Something done on the Roto that it tells the program of by SysEx, such as choosing a mode
    /// or a page.
    pub fn press(&mut self, message: FromRoto) -> Vec<u8> {
        match message {
            FromRoto::SetPluginMode => self.mode = Mode::Plugin,
            FromRoto::SetMixAllTracksMode { .. } | FromRoto::SetMixTrackMode(_) => {
                self.mode = Mode::Mix;
            }
            _ => {}
        }
        message.encode()
    }

    /// A knob, a switch or a touch on the Roto, as the CCs it sends.
    pub fn control(&mut self, event: ControlEvent) -> Vec<Vec<u8>> {
        cc::encode(event)
            .into_iter()
            .map(|cc| cc.to_vec())
            .collect()
    }

    /// Where the program last put a knob, from the CCs it sent, if it has.
    pub fn knob_position(&self, index: u8) -> Option<u16> {
        let mut high = None;
        let mut value = None;
        for &[_, number, data] in &self.received_ccs {
            if number == cc::KNOB_HIGH + index {
                high = Some(data);
            } else if number == cc::KNOB_LOW + index {
                value = Some((u16::from(high.unwrap_or(0)) << 7) | u16::from(data));
            }
        }
        value
    }

    fn setup_at(&self, hash: PluginHash) -> Option<usize> {
        self.setups
            .iter()
            .position(|setup| setup.plugin.hash == hash)
    }

    /// A serial request's whole frame, and the fake's reply's bytes.
    pub fn serial_in(&mut self, frame: &[u8]) -> Vec<u8> {
        let Ok(request) = Request::decode(frame) else {
            self.unreadable.push(frame.to_vec());
            return serial::encode_failure(Code::Error(0x01));
        };
        if !request.is_read_only() {
            self.serial_changes.push(request.clone());
        }
        if request.needs_config_update() && !self.updating {
            return serial::encode_failure(Code::Error(0x02));
        }
        match self.answer(&request) {
            Ok(reply) => reply.encode(),
            Err(code) => serial::encode_failure(code),
        }
    }

    fn answer(&mut self, request: &Request) -> Result<Reply, Code> {
        let listed = |setup: &StoredSetup| Reply::Plugin(setup.plugin.clone());
        match request {
            Request::GetFirmwareVersion => Ok(Reply::FirmwareVersion(self.firmware.clone())),
            Request::GetMode => Ok(Reply::Mode {
                mode: self.mode,
                page: self.page,
            }),
            Request::SetMode { mode, page } => {
                self.mode = *mode;
                self.page = *page;
                Ok(Reply::Done)
            }
            Request::StartConfigUpdate => {
                self.updating = true;
                Ok(Reply::Done)
            }
            Request::EndConfigUpdate => {
                self.updating = false;
                Ok(Reply::Done)
            }
            Request::FactoryReset => {
                self.setups.clear();
                Ok(Reply::Done)
            }
            Request::GetCurrentSetup => Ok(Reply::Setup {
                index: self.midi_setup,
                name: self.midi_setup_names[usize::from(self.midi_setup)].clone(),
            }),
            Request::GetSetup(index) => {
                let name = self
                    .midi_setup_names
                    .get(usize::from(*index))
                    .ok_or(Code::Error(3))?;
                Ok(Reply::Setup {
                    index: *index,
                    name: name.clone(),
                })
            }
            Request::SetSetup(index) if *index < MIDI_SETUPS => {
                self.midi_setup = *index;
                Ok(Reply::Done)
            }
            Request::SetSetupName { setup, name } if *setup < MIDI_SETUPS => {
                self.midi_setup_names[usize::from(*setup)] = name.clone();
                Ok(Reply::Done)
            }
            // MIDI setups' controls aren't kept: vimtracker never writes them.
            Request::SetSetup(_)
            | Request::SetSetupName { .. }
            | Request::GetKnobControlConfig { .. }
            | Request::GetSwitchControlConfig { .. } => Err(Code::Error(3)),
            Request::SetKnobControlConfig(_)
            | Request::SetSwitchControlConfig(_)
            | Request::ClearControlConfig { .. }
            | Request::ClearMidiSetup(_) => Ok(Reply::Done),
            Request::GetCurrentPlugin => Err(Code::NoPlugin),
            Request::GetFirstPlugin => {
                self.next_plugin = 1;
                self.setups.first().map(listed).ok_or(Code::NoPlugin)
            }
            Request::GetNextPlugin => {
                let setup = self.setups.get(self.next_plugin).ok_or(Code::NoPlugin)?;
                self.next_plugin += 1;
                Ok(listed(setup))
            }
            Request::GetPlugin(hash) => {
                let at = self.setup_at(*hash).ok_or(Code::NoPlugin)?;
                Ok(listed(&self.setups[at]))
            }
            Request::AddPlugin { hash, name } => {
                if self.setup_at(*hash).is_some() {
                    return Err(Code::PluginExists);
                }
                if self.setups.len() >= MAX_PLUGIN_SETUPS {
                    return Err(Code::Error(0x04));
                }
                self.setups.push(StoredSetup {
                    plugin: StoredPlugin {
                        hash: *hash,
                        name: name.clone(),
                        kind: StoredKind::Normal,
                    },
                    knobs: BTreeMap::new(),
                    switches: BTreeMap::new(),
                });
                Ok(Reply::Done)
            }
            Request::SetPluginName { hash, name } => {
                let at = self.setup_at(*hash).ok_or(Code::NoPlugin)?;
                self.setups[at].plugin.name = name.clone();
                Ok(Reply::Done)
            }
            Request::ClearPlugin(hash) => {
                let at = self.setup_at(*hash).ok_or(Code::NoPlugin)?;
                self.setups.remove(at);
                Ok(Reply::Done)
            }
            Request::GetPluginKnobConfig { plugin, control } => {
                let at = self.setup_at(*plugin).ok_or(Code::NoPlugin)?;
                let knob = self.setups[at].knobs.get(control).ok_or(Code::NoPlugin)?;
                Ok(Reply::PluginKnob(knob.clone()))
            }
            Request::GetPluginSwitchConfig { plugin, control } => {
                let at = self.setup_at(*plugin).ok_or(Code::NoPlugin)?;
                let switch = self.setups[at]
                    .switches
                    .get(control)
                    .ok_or(Code::NoPlugin)?;
                Ok(Reply::PluginSwitch(switch.clone()))
            }
            Request::SetPluginKnobConfig(config) => {
                let at = self.setup_at(config.plugin).ok_or(Code::NoPlugin)?;
                if config.control >= serial::PLUGIN_CONTROLS {
                    return Err(Code::NoPlugin);
                }
                self.setups[at].knobs.insert(config.control, config.clone());
                Ok(Reply::Done)
            }
            Request::SetPluginSwitchConfig(config) => {
                let at = self.setup_at(config.plugin).ok_or(Code::NoPlugin)?;
                if config.control >= serial::PLUGIN_CONTROLS {
                    return Err(Code::NoPlugin);
                }
                self.setups[at]
                    .switches
                    .insert(config.control, config.clone());
                Ok(Reply::Done)
            }
            Request::ClearPluginControlConfig {
                plugin,
                control,
                index,
            } => {
                let at = self.setup_at(*plugin).ok_or(Code::NoPlugin)?;
                let setup = &mut self.setups[at];
                let removed = match control {
                    crate::sysex::ControlType::Knob => setup.knobs.remove(index).is_some(),
                    crate::sysex::ControlType::Switch => setup.switches.remove(index).is_some(),
                };
                if removed {
                    Ok(Reply::Done)
                } else {
                    Err(Code::NoPlugin)
                }
            }
        }
    }

    /// A PLUGIN setup chosen on the Roto, as the serial notice it sends.
    pub fn select_plugin(&mut self, hash: Option<PluginHash>) -> Vec<u8> {
        Notice::PluginSelected(hash).encode()
    }
}

/// The fake Roto as the link's ports, to plug in and unplug, shared with the test that drives it.
#[derive(Clone)]
pub struct FakePorts {
    shared: Arc<Mutex<Shared>>,
}

struct Shared {
    roto: FakeRoto,
    plugged: bool,
    deliver: Option<Box<dyn Deliver>>,
}

struct FakeSend {
    shared: Arc<Mutex<Shared>>,
}

impl MidiSend for FakeSend {
    fn send(&mut self, message: &[u8]) -> Result<(), LinkError> {
        let mut shared = self.shared.lock().expect("the fake's lock");
        if !shared.plugged {
            return Err(LinkError::NotFound);
        }
        let replies = shared.roto.midi_in(message);
        if let Some(deliver) = &shared.deliver {
            for reply in replies {
                deliver.deliver(&reply);
            }
        }
        Ok(())
    }
}

impl FakePorts {
    /// A fake Roto, plugged in or not.
    pub fn new(roto: FakeRoto, plugged: bool) -> Self {
        Self {
            shared: Arc::new(Mutex::new(Shared {
                roto,
                plugged,
                deliver: None,
            })),
        }
    }

    /// Plugs it in: the link finds it the next time it looks.
    pub fn plug_in(&self) {
        self.shared.lock().expect("the fake's lock").plugged = true;
    }

    /// Unplugs it: what's sent fails, and the link finds it gone the next time it looks.
    pub fn unplug(&self) {
        let mut shared = self.shared.lock().expect("the fake's lock");
        shared.plugged = false;
        shared.deliver = None;
    }

    /// Sends the program what the fake does when used, such as a knob's CCs.
    pub fn from_roto(&self, messages: Vec<Vec<u8>>) {
        let shared = self.shared.lock().expect("the fake's lock");
        if let Some(deliver) = &shared.deliver {
            for message in messages {
                deliver.deliver(&message);
            }
        }
    }

    /// Works on the fake Roto, for a test to use it or look at what it received.
    pub fn with<T>(&self, act: impl FnOnce(&mut FakeRoto) -> T) -> T {
        act(&mut self.shared.lock().expect("the fake's lock").roto)
    }
}

impl Ports for FakePorts {
    fn present(&mut self) -> bool {
        self.shared.lock().expect("the fake's lock").plugged
    }

    fn open(&mut self, incoming: Box<dyn Deliver>) -> Result<Box<dyn MidiSend>, LinkError> {
        let mut shared = self.shared.lock().expect("the fake's lock");
        if !shared.plugged {
            return Err(LinkError::NotFound);
        }
        shared.deliver = Some(incoming);
        Ok(Box::new(FakeSend {
            shared: self.shared.clone(),
        }))
    }
}
