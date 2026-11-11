//! The Roto's SysEx commands, version 1.6 of Melbourne Instruments' API, as typed messages each
//! way: [`ToRoto`] for what the program sends and [`FromRoto`] for what the Roto does. Both can be
//! written and read, so the fake Roto in [`crate::fake`] speaks the same code.
//!
//! Every message is `F0 00 22 03 02 <group> <command> <data> F7`. Where the PDF and the vendor's
//! scripts disagree, the scripts win, as they run against the real firmware:
//!
//! *   The PDF's section headings number the last PLUGIN commands in decimal, `20` to `23`; its
//!     table and the Logic script have them as `1A` to `1D`, which is what's used here.
//! *   `0C 0C`, the meters' states, isn't in the PDF; the Ableton script sends it.
//! *   An index in two bytes is the high seven bits first, as the scripts send it.
//! *   `SET PLUGIN MODE` (`0B 01`) has no data in the PDF, but the real Roto (firmware 3.3.0) sends
//!     one byte, `00`, which Bitwig's script reads and doesn't use. It's written as the Roto sends
//!     it, and read with or without it.

use crate::bytes::{DecodeError, Label, ParamHash, PluginHash, Reader, Rgb, push_pair};

/// The bytes before every message: the start of SysEx, Melbourne Instruments' ID and the Roto's.
pub const HEADER: [u8; 5] = [0xF0, 0x00, 0x22, 0x03, 0x02];

/// The byte that ends every message.
pub const END: u8 = 0xF7;

/// The groups of commands.
pub const GENERAL: u8 = 0x0A;
pub const PLUGIN: u8 = 0x0B;
pub const MIX: u8 = 0x0C;

/// How many names come with a step knob's details at most. A knob can have more steps, up to
/// [`MAX_STEPS`], but past this its steps have no names.
pub const MAX_NAMED_STEPS: u8 = 16;

/// The most steps a knob has.
pub const MAX_STEPS: u8 = 24;

/// How many controls of a kind the Roto has on a page, and how many tracks or plugins it shows.
pub const CONTROLS: usize = 8;

/// Which program says it's driving the Roto, in `DAW PING RESPONSE`. The Roto behaves a little
/// differently for each: see the design note on which vimtracker claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Daw {
    Ableton,
    Bitwig,
    Logic,
}

impl Daw {
    fn code(self) -> u8 {
        match self {
            Self::Ableton => 1,
            Self::Bitwig => 2,
            Self::Logic => 3,
        }
    }

    pub(crate) fn from_code(value: u8) -> Result<Self, DecodeError> {
        match value {
            1 => Ok(Self::Ableton),
            2 => Ok(Self::Bitwig),
            3 => Ok(Self::Logic),
            value => Err(DecodeError::Value {
                what: "a DAW",
                value,
            }),
        }
    }
}

/// A knob or a switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlType {
    Knob,
    Switch,
}

impl ControlType {
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Knob => 0,
            Self::Switch => 1,
        }
    }

    pub(crate) fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Knob),
            1 => Ok(Self::Switch),
            value => Err(DecodeError::Value {
                what: "a control type",
                value,
            }),
        }
    }
}

/// One track as the Roto shows it in MIX mode and in its track list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TrackDetails {
    pub index: u16,
    pub name: Label,
    /// An index into [`crate::colour::SCHEMES`].
    pub colour: u8,
    /// Whether it's a group, which the Roto can fold.
    pub grouped: bool,
}

impl TrackDetails {
    fn write(&self, out: &mut Vec<u8>) {
        push_pair(out, self.index);
        self.name.write(out);
        out.push(self.colour & 0x7F);
        out.push(u8::from(self.grouped));
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Self {
            index: reader.pair()?,
            name: reader.label()?,
            colour: reader.byte()?,
            grouped: reader.flag("grouped")?,
        })
    }
}

/// The transport's state, for the Roto's transport buttons' lights. Stop has no state; it's always
/// sent off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TransportStatus {
    pub playing: bool,
    pub recording: bool,
    pub session_recording: bool,
    pub looping: bool,
    pub punch_in: bool,
    pub punch_out: bool,
    pub automation_changed: bool,
}

/// What kind of plugin a device is, in `PLUGIN DETAILS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginKind {
    /// One whose controls the Roto keeps in a stored setup, found by its hash.
    Normal,
    /// One whose controls the program sends as they're needed, a page of eight at a time, and
    /// which isn't stored.
    Macro,
    ThirdParty,
}

impl PluginKind {
    fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Macro => 1,
            Self::ThirdParty => 2,
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Macro),
            2 => Ok(Self::ThirdParty),
            value => Err(DecodeError::Value {
                what: "a plugin type",
                value,
            }),
        }
    }
}

/// One device in the Roto's plugin list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginDetails {
    /// Its place in the track's list, from the first.
    pub index: u8,
    pub hash: PluginHash,
    pub enabled: bool,
    pub name: Label,
    pub kind: PluginKind,
    /// How many pages of eight controls a macro plugin has, or 0.
    pub macro_pages: u8,
}

/// A setting's details for the control it's on: its name, where it is, and how the knob feels.
/// The program sends it when the Roto says a control is mapped, and for a macro plugin's
/// controls as each page comes up.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LearnParam {
    pub index: u16,
    pub hash: ParamHash,
    pub macro_param: bool,
    /// A notch at the top of a knob, for a centred setting. The steps are ignored with it.
    pub centre_indent: bool,
    /// 0 for a smooth knob, or 2 to [`MAX_STEPS`] steps.
    pub steps: u8,
    /// Where the setting is, from 0 to 16383.
    pub position: u16,
    pub name: Label,
    /// A name for each step, when there are no more than [`MAX_NAMED_STEPS`]. Written with
    /// exactly `steps` names, padded with empty ones.
    pub step_names: Vec<Label>,
}

impl LearnParam {
    /// Whether its steps carry names in the message.
    fn named_steps(steps: u8) -> usize {
        if (1..=MAX_NAMED_STEPS).contains(&steps) {
            usize::from(steps)
        } else {
            0
        }
    }
}

/// Which tracks MIX mode's "all tracks" shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AllTracks {
    /// The tracks that play, as Ableton's audio and MIDI tracks.
    Audio,
    /// The returns and the master.
    MasterReturn,
}

impl AllTracks {
    fn code(self) -> u8 {
        match self {
            Self::Audio => 0,
            Self::MasterReturn => 1,
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Audio),
            1 => Ok(Self::MasterReturn),
            value => Err(DecodeError::Value {
                what: "a track set",
                value,
            }),
        }
    }
}

/// What MIX mode's knobs move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnobMode {
    Level,
    Pan,
    Send,
}

/// What MIX mode's switches do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitchMode {
    Mute,
    Solo,
    Arm,
}

/// A message to the Roto.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToRoto {
    // GENERAL, 0A.
    /// A session starts. Nothing else is sent until the Roto answers with `PING DAW`.
    DawStarted,
    /// The answer to `PING DAW`, saying which program this is.
    PingResponse(Daw),
    NumTracks(u16),
    /// The first track of the page shown, a multiple of 8.
    FirstTrack(u16),
    TrackDetails(TrackDetails),
    TrackDetailsEnd,
    TransportStatus(TransportStatus),
    RequestFirmwareVersion,
    RequestApiVersion,
    /// As `TrackDetails`, but shown at once.
    SetTrackDetails(TrackDetails),
    ResetTrackDetails(u16),
    SetTrackColour {
        index: u16,
        colour: Rgb,
    },
    SetCurrentTrackName(Label),
    SetCurrentTrackColour(Rgb),
    /// A control's value as text, shown on its screen until the Roto times it out.
    ShowControlValue {
        control: ControlType,
        index: u8,
        value: Label,
    },
    // PLUGIN, 0B.
    NumPlugins(u8),
    FirstPlugin(u8),
    PluginDetails(PluginDetails),
    PluginDetailsEnd,
    DawSelectPlugin {
        index: u8,
        macro_page: u8,
        /// Select it even if the Roto has its plugin locked.
        force: bool,
    },
    LearnParam(LearnParam),
    UnmapControl {
        control: ControlType,
        index: u8,
    },
    SetMappedControlName {
        index: u16,
        hash: ParamHash,
        name: Label,
    },
    SetPluginControlDetails {
        control: ControlType,
        index: u8,
        name: Label,
        colour: u8,
    },
    ResetPluginControlDetails {
        control: ControlType,
        index: u8,
    },
    /// Logic Pro's parameter sweep, kept so every command has a type.
    PluginParamSweep(u16),
    PluginParamSweepRestart(u16),
    // MIX, 0C.
    NumSendTracks(u8),
    DawSelectTrack(TrackDetails),
    /// Eight sends' names, from the first given, a multiple of 8.
    SendTrackNames {
        first: u8,
        names: [Label; CONTROLS],
    },
    RefreshMixMode,
    DawSelectFocusTrack {
        index: u16,
        colour: Rgb,
    },
    /// Where MIX mode's meters turn from green to yellow and from yellow to red, each of 0 to 127.
    SetVuMeterThresholds {
        yellow: u8,
        red: u8,
    },
    /// Whether each meter on the page is lit, as a muted track's isn't. Not in the PDF: the
    /// Ableton script sends it, as `0C 0C`.
    SetVuMeterStates([bool; CONTROLS]),
}

/// A message from the Roto.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FromRoto {
    // GENERAL, 0A.
    /// The Roto looks for a program, every second for a minute after it's plugged in or after
    /// `DAW STARTED`.
    PingDaw,
    SetFirstTrack(u16),
    RotoSelectTrack(u16),
    RequestTransportStatus,
    /// The handshake is done; the program may now send anything.
    Connected,
    FirmwareVersion(FirmwareVersion),
    ApiVersion {
        major: u8,
        minor: u8,
    },
    /// The arrows, sent only to Logic Pro in MIX mode's focus.
    PageLeft,
    PageRight,
    // PLUGIN, 0B.
    /// PLUGIN mode is chosen, or the device or page it shows changed. The real Roto sends a byte
    /// the PDF leaves out, `00` so far.
    SetPluginMode,
    SetFirstPlugin(u8),
    RotoSelectPlugin(u8),
    SetDeviceLearn(bool),
    ControlMapped {
        index: u16,
        hash: ParamHash,
        control: ControlType,
        control_index: u8,
        macro_param: bool,
    },
    SetPluginEnabled {
        index: u8,
        enabled: bool,
    },
    SetPluginsLock(bool),
    MacroPluginPageIndex(u8),
    ControlLearned {
        control: ControlType,
        index: u8,
    },
    ToggleRemotePage,
    SetTrackSelectMode,
    SetPluginEnableMode,
    SetPluginSelectMode,
    PluginParamSweepValue {
        channel: u8,
        param: u16,
        value: u16,
    },
    PluginParamSweepEnd,
    // MIX, 0C.
    SetMixAllTracksMode {
        tracks: AllTracks,
        knobs: KnobMode,
        switches: SwitchMode,
        send: u8,
    },
    /// MIX mode on one track, its controls from this page, a multiple of 8.
    SetMixTrackMode(u8),
    SetAllTracksMode(AllTracks),
    ToggleTrackGrouping(u16),
    RequestSendTrackNames(u8),
}

/// The Roto's firmware version, with its short git commit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FirmwareVersion {
    pub major: u8,
    pub minor: u8,
    pub patch: u8,
    pub commit: String,
}

impl FirmwareVersion {
    /// How many bytes the commit takes.
    pub const COMMIT_BYTES: usize = 7;

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&[self.major, self.minor, self.patch]);
        let mut commit = [0u8; Self::COMMIT_BYTES];
        for (at, byte) in self.commit.bytes().take(Self::COMMIT_BYTES).enumerate() {
            commit[at] = byte & 0x7F;
        }
        out.extend_from_slice(&commit);
    }

    pub(crate) fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        let major = reader.byte()?;
        let minor = reader.byte()?;
        let patch = reader.byte()?;
        let commit = reader
            .take(Self::COMMIT_BYTES)?
            .iter()
            .take_while(|&&byte| byte != 0)
            .map(|&byte| char::from(byte & 0x7F))
            .collect();
        Ok(Self {
            major,
            minor,
            patch,
            commit,
        })
    }
}

impl std::fmt::Display for FirmwareVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{}.{}.{} ({})",
            self.major, self.minor, self.patch, self.commit
        )
    }
}

/// A whole message, header and end included, from its group, command and data.
fn frame(group: u8, command: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER.len() + 3 + data.len());
    out.extend_from_slice(&HEADER);
    out.push(group);
    out.push(command);
    // SysEx data is seven bits; anything wider would end the message early.
    out.extend(data.iter().map(|&byte| byte & 0x7F));
    out.push(END);
    out
}

/// A message's group, command and data, if it's the Roto's.
pub fn unframe(message: &[u8]) -> Result<(u8, u8, &[u8]), DecodeError> {
    let inner = message
        .strip_prefix(&HEADER)
        .and_then(|rest| rest.strip_suffix(&[END]))
        .ok_or(DecodeError::NotRoto)?;
    match inner {
        [group, command, data @ ..] => Ok((*group, *command, data)),
        _ => Err(DecodeError::Short),
    }
}

/// Whether a MIDI message is one of the Roto's SysEx messages.
pub fn is_roto_sysex(message: &[u8]) -> bool {
    message.starts_with(&HEADER) && message.last() == Some(&END)
}

fn knob_mode_code(mode: KnobMode) -> u8 {
    match mode {
        KnobMode::Level => 0,
        KnobMode::Pan => 1,
        KnobMode::Send => 2,
    }
}

fn switch_mode_code(mode: SwitchMode) -> u8 {
    match mode {
        SwitchMode::Mute => 0,
        SwitchMode::Solo => 1,
        SwitchMode::Arm => 2,
    }
}

impl ToRoto {
    /// The whole message, ready to send.
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let (group, command) = match self {
            Self::DawStarted => (GENERAL, 0x01),
            Self::PingResponse(daw) => {
                data.push(daw.code());
                (GENERAL, 0x03)
            }
            Self::NumTracks(count) => {
                push_pair(&mut data, *count);
                (GENERAL, 0x04)
            }
            Self::FirstTrack(first) => {
                push_pair(&mut data, *first);
                (GENERAL, 0x05)
            }
            Self::TrackDetails(details) => {
                details.write(&mut data);
                (GENERAL, 0x07)
            }
            Self::TrackDetailsEnd => (GENERAL, 0x08),
            Self::TransportStatus(status) => {
                data.extend_from_slice(&[
                    u8::from(status.playing),
                    0,
                    u8::from(status.recording),
                    u8::from(status.session_recording),
                    u8::from(status.looping),
                    u8::from(status.punch_in),
                    u8::from(status.punch_out),
                    u8::from(status.automation_changed),
                ]);
                (GENERAL, 0x0B)
            }
            Self::RequestFirmwareVersion => (GENERAL, 0x0D),
            Self::RequestApiVersion => (GENERAL, 0x0F),
            Self::SetTrackDetails(details) => {
                details.write(&mut data);
                (GENERAL, 0x11)
            }
            Self::ResetTrackDetails(index) => {
                push_pair(&mut data, *index);
                (GENERAL, 0x12)
            }
            Self::SetTrackColour { index, colour } => {
                push_pair(&mut data, *index);
                colour.write(&mut data);
                (GENERAL, 0x13)
            }
            Self::SetCurrentTrackName(name) => {
                name.write(&mut data);
                (GENERAL, 0x16)
            }
            Self::SetCurrentTrackColour(colour) => {
                colour.write(&mut data);
                (GENERAL, 0x17)
            }
            Self::ShowControlValue {
                control,
                index,
                value,
            } => {
                data.push(control.code());
                data.push(*index);
                value.write(&mut data);
                (GENERAL, 0x18)
            }
            Self::NumPlugins(count) => {
                data.push(*count);
                (PLUGIN, 0x02)
            }
            Self::FirstPlugin(first) => {
                data.push(*first);
                (PLUGIN, 0x03)
            }
            Self::PluginDetails(details) => {
                data.push(details.index);
                data.extend_from_slice(&details.hash.0);
                data.push(u8::from(details.enabled));
                details.name.write(&mut data);
                data.push(details.kind.code());
                data.push(details.macro_pages);
                (PLUGIN, 0x05)
            }
            Self::PluginDetailsEnd => (PLUGIN, 0x06),
            Self::DawSelectPlugin {
                index,
                macro_page,
                force,
            } => {
                data.extend_from_slice(&[*index, *macro_page, u8::from(*force)]);
                (PLUGIN, 0x08)
            }
            Self::LearnParam(param) => {
                push_pair(&mut data, param.index);
                data.extend_from_slice(&param.hash.0);
                data.push(u8::from(param.macro_param));
                data.push(u8::from(param.centre_indent));
                data.push(param.steps);
                push_pair(&mut data, param.position);
                param.name.write(&mut data);
                let empty = Label::empty();
                for at in 0..LearnParam::named_steps(param.steps) {
                    param.step_names.get(at).unwrap_or(&empty).write(&mut data);
                }
                (PLUGIN, 0x0A)
            }
            Self::UnmapControl { control, index } => {
                data.extend_from_slice(&[control.code(), *index]);
                (PLUGIN, 0x0E)
            }
            Self::SetMappedControlName { index, hash, name } => {
                push_pair(&mut data, *index);
                data.extend_from_slice(&hash.0);
                name.write(&mut data);
                (PLUGIN, 0x0F)
            }
            Self::SetPluginControlDetails {
                control,
                index,
                name,
                colour,
            } => {
                data.extend_from_slice(&[control.code(), *index]);
                name.write(&mut data);
                data.push(*colour);
                (PLUGIN, 0x13)
            }
            Self::ResetPluginControlDetails { control, index } => {
                data.extend_from_slice(&[control.code(), *index]);
                (PLUGIN, 0x14)
            }
            Self::PluginParamSweep(index) => {
                push_pair(&mut data, *index);
                (PLUGIN, 0x19)
            }
            Self::PluginParamSweepRestart(index) => {
                push_pair(&mut data, *index);
                (PLUGIN, 0x1D)
            }
            Self::NumSendTracks(count) => {
                data.push(*count);
                (MIX, 0x03)
            }
            Self::DawSelectTrack(details) => {
                details.write(&mut data);
                (MIX, 0x04)
            }
            Self::SendTrackNames { first, names } => {
                data.push(*first);
                for name in names {
                    name.write(&mut data);
                }
                (MIX, 0x08)
            }
            Self::RefreshMixMode => (MIX, 0x09),
            Self::DawSelectFocusTrack { index, colour } => {
                push_pair(&mut data, *index);
                colour.write(&mut data);
                (MIX, 0x0A)
            }
            Self::SetVuMeterThresholds { yellow, red } => {
                data.extend_from_slice(&[*yellow, *red]);
                (MIX, 0x0B)
            }
            Self::SetVuMeterStates(states) => {
                data.extend(states.iter().map(|&lit| u8::from(lit)));
                (MIX, 0x0C)
            }
        };
        frame(group, command, &data)
    }

    /// A message to the Roto from its bytes, as the fake Roto reads them.
    pub fn decode(message: &[u8]) -> Result<Self, DecodeError> {
        let (group, command, data) = unframe(message)?;
        let mut reader = Reader::new(data);
        let reader = &mut reader;
        let message = match (group, command) {
            (GENERAL, 0x01) => Self::DawStarted,
            (GENERAL, 0x03) => Self::PingResponse(Daw::from_code(reader.byte()?)?),
            (GENERAL, 0x04) => Self::NumTracks(reader.pair()?),
            (GENERAL, 0x05) => Self::FirstTrack(reader.pair()?),
            (GENERAL, 0x07) => Self::TrackDetails(TrackDetails::read(reader)?),
            (GENERAL, 0x08) => Self::TrackDetailsEnd,
            (GENERAL, 0x0B) => {
                let playing = reader.flag("play")?;
                reader.flag("stop")?;
                Self::TransportStatus(TransportStatus {
                    playing,
                    recording: reader.flag("record")?,
                    session_recording: reader.flag("session record")?,
                    looping: reader.flag("loop")?,
                    punch_in: reader.flag("punch in")?,
                    punch_out: reader.flag("punch out")?,
                    automation_changed: reader.flag("automation")?,
                })
            }
            (GENERAL, 0x0D) => Self::RequestFirmwareVersion,
            (GENERAL, 0x0F) => Self::RequestApiVersion,
            (GENERAL, 0x11) => Self::SetTrackDetails(TrackDetails::read(reader)?),
            (GENERAL, 0x12) => Self::ResetTrackDetails(reader.pair()?),
            (GENERAL, 0x13) => Self::SetTrackColour {
                index: reader.pair()?,
                colour: Rgb::read(reader)?,
            },
            (GENERAL, 0x16) => Self::SetCurrentTrackName(reader.label()?),
            (GENERAL, 0x17) => Self::SetCurrentTrackColour(Rgb::read(reader)?),
            (GENERAL, 0x18) => Self::ShowControlValue {
                control: ControlType::read(reader)?,
                index: reader.byte()?,
                value: reader.label()?,
            },
            (PLUGIN, 0x02) => Self::NumPlugins(reader.byte()?),
            (PLUGIN, 0x03) => Self::FirstPlugin(reader.byte()?),
            (PLUGIN, 0x05) => Self::PluginDetails(PluginDetails {
                index: reader.byte()?,
                hash: PluginHash(reader.array()?),
                enabled: reader.flag("enabled")?,
                name: reader.label()?,
                kind: PluginKind::read(reader)?,
                macro_pages: reader.byte()?,
            }),
            (PLUGIN, 0x06) => Self::PluginDetailsEnd,
            (PLUGIN, 0x08) => Self::DawSelectPlugin {
                index: reader.byte()?,
                macro_page: reader.byte()?,
                force: reader.flag("force")?,
            },
            (PLUGIN, 0x0A) => {
                let index = reader.pair()?;
                let hash = ParamHash(reader.array()?);
                let macro_param = reader.flag("macro")?;
                let centre_indent = reader.flag("centre indent")?;
                let steps = reader.byte()?;
                let position = reader.pair()?;
                let name = reader.label()?;
                let step_names = (0..LearnParam::named_steps(steps))
                    .map(|_| reader.label())
                    .collect::<Result<_, _>>()?;
                Self::LearnParam(LearnParam {
                    index,
                    hash,
                    macro_param,
                    centre_indent,
                    steps,
                    position,
                    name,
                    step_names,
                })
            }
            (PLUGIN, 0x0E) => Self::UnmapControl {
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (PLUGIN, 0x0F) => Self::SetMappedControlName {
                index: reader.pair()?,
                hash: ParamHash(reader.array()?),
                name: reader.label()?,
            },
            (PLUGIN, 0x13) => Self::SetPluginControlDetails {
                control: ControlType::read(reader)?,
                index: reader.byte()?,
                name: reader.label()?,
                colour: reader.byte()?,
            },
            (PLUGIN, 0x14) => Self::ResetPluginControlDetails {
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (PLUGIN, 0x19) => Self::PluginParamSweep(reader.pair()?),
            (PLUGIN, 0x1D) => Self::PluginParamSweepRestart(reader.pair()?),
            (MIX, 0x03) => Self::NumSendTracks(reader.byte()?),
            (MIX, 0x04) => Self::DawSelectTrack(TrackDetails::read(reader)?),
            (MIX, 0x08) => {
                let first = reader.byte()?;
                let mut names: [Label; CONTROLS] = Default::default();
                for name in &mut names {
                    *name = reader.label()?;
                }
                Self::SendTrackNames { first, names }
            }
            (MIX, 0x09) => Self::RefreshMixMode,
            (MIX, 0x0A) => Self::DawSelectFocusTrack {
                index: reader.pair()?,
                colour: Rgb::read(reader)?,
            },
            (MIX, 0x0B) => Self::SetVuMeterThresholds {
                yellow: reader.byte()?,
                red: reader.byte()?,
            },
            (MIX, 0x0C) => {
                let mut states = [false; CONTROLS];
                for state in &mut states {
                    *state = reader.flag("a meter's state")?;
                }
                Self::SetVuMeterStates(states)
            }
            (group, command) => return Err(DecodeError::Unknown { group, command }),
        };
        // Some commands' data has a fixed size, so anything after it is a misreading.
        let left = reader.remaining();
        if left != 0 {
            return Err(DecodeError::Trailing(left));
        }
        Ok(message)
    }
}

impl FromRoto {
    /// The whole message, as the fake Roto sends it.
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let (group, command) = match self {
            Self::PingDaw => (GENERAL, 0x02),
            Self::SetFirstTrack(first) => {
                push_pair(&mut data, *first);
                (GENERAL, 0x06)
            }
            Self::RotoSelectTrack(index) => {
                push_pair(&mut data, *index);
                (GENERAL, 0x09)
            }
            Self::RequestTransportStatus => (GENERAL, 0x0A),
            Self::Connected => (GENERAL, 0x0C),
            Self::FirmwareVersion(version) => {
                version.write(&mut data);
                (GENERAL, 0x0E)
            }
            Self::ApiVersion { major, minor } => {
                data.extend_from_slice(&[*major, *minor]);
                (GENERAL, 0x10)
            }
            Self::PageLeft => (GENERAL, 0x14),
            Self::PageRight => (GENERAL, 0x15),
            Self::SetPluginMode => {
                data.push(0);
                (PLUGIN, 0x01)
            }
            Self::SetFirstPlugin(first) => {
                data.push(*first);
                (PLUGIN, 0x04)
            }
            Self::RotoSelectPlugin(index) => {
                data.push(*index);
                (PLUGIN, 0x07)
            }
            Self::SetDeviceLearn(on) => {
                data.push(u8::from(*on));
                (PLUGIN, 0x09)
            }
            Self::ControlMapped {
                index,
                hash,
                control,
                control_index,
                macro_param,
            } => {
                push_pair(&mut data, *index);
                data.extend_from_slice(&hash.0);
                data.extend_from_slice(&[control.code(), *control_index, u8::from(*macro_param)]);
                (PLUGIN, 0x0B)
            }
            Self::SetPluginEnabled { index, enabled } => {
                data.extend_from_slice(&[*index, u8::from(*enabled)]);
                (PLUGIN, 0x0C)
            }
            Self::SetPluginsLock(locked) => {
                data.push(u8::from(*locked));
                (PLUGIN, 0x0D)
            }
            Self::MacroPluginPageIndex(page) => {
                data.push(*page);
                (PLUGIN, 0x10)
            }
            Self::ControlLearned { control, index } => {
                data.extend_from_slice(&[control.code(), *index]);
                (PLUGIN, 0x11)
            }
            Self::ToggleRemotePage => (PLUGIN, 0x12),
            Self::SetTrackSelectMode => (PLUGIN, 0x15),
            Self::SetPluginEnableMode => (PLUGIN, 0x16),
            Self::SetPluginSelectMode => (PLUGIN, 0x17),
            Self::PluginParamSweepValue {
                channel,
                param,
                value,
            } => {
                // Low seven bits first, unlike every other pair.
                data.push(*channel);
                data.extend_from_slice(&[(param & 0x7F) as u8, (param >> 7) as u8]);
                data.extend_from_slice(&[(value & 0x7F) as u8, (value >> 7) as u8]);
                (PLUGIN, 0x1A)
            }
            Self::PluginParamSweepEnd => (PLUGIN, 0x1C),
            Self::SetMixAllTracksMode {
                tracks,
                knobs,
                switches,
                send,
            } => {
                data.extend_from_slice(&[
                    tracks.code(),
                    knob_mode_code(*knobs),
                    switch_mode_code(*switches),
                    *send,
                ]);
                (MIX, 0x01)
            }
            Self::SetMixTrackMode(page) => {
                data.push(*page);
                (MIX, 0x02)
            }
            Self::SetAllTracksMode(tracks) => {
                data.push(tracks.code());
                (MIX, 0x05)
            }
            Self::ToggleTrackGrouping(index) => {
                push_pair(&mut data, *index);
                (MIX, 0x06)
            }
            Self::RequestSendTrackNames(index) => {
                data.push(*index);
                (MIX, 0x07)
            }
        };
        frame(group, command, &data)
    }

    /// A message from the Roto from its bytes.
    pub fn decode(message: &[u8]) -> Result<Self, DecodeError> {
        let (group, command, data) = unframe(message)?;
        let mut reader = Reader::new(data);
        let reader = &mut reader;
        let message = match (group, command) {
            (GENERAL, 0x02) => Self::PingDaw,
            (GENERAL, 0x06) => Self::SetFirstTrack(reader.pair()?),
            (GENERAL, 0x09) => Self::RotoSelectTrack(reader.pair()?),
            (GENERAL, 0x0A) => Self::RequestTransportStatus,
            (GENERAL, 0x0C) => Self::Connected,
            (GENERAL, 0x0E) => Self::FirmwareVersion(FirmwareVersion::read(reader)?),
            (GENERAL, 0x10) => Self::ApiVersion {
                major: reader.byte()?,
                minor: reader.byte()?,
            },
            (GENERAL, 0x14) => Self::PageLeft,
            (GENERAL, 0x15) => Self::PageRight,
            (PLUGIN, 0x01) => {
                if reader.remaining() == 1 {
                    reader.byte()?;
                }
                Self::SetPluginMode
            }
            (PLUGIN, 0x04) => Self::SetFirstPlugin(reader.byte()?),
            (PLUGIN, 0x07) => Self::RotoSelectPlugin(reader.byte()?),
            (PLUGIN, 0x09) => Self::SetDeviceLearn(reader.flag("learn")?),
            (PLUGIN, 0x0B) => Self::ControlMapped {
                index: reader.pair()?,
                hash: ParamHash(reader.array()?),
                control: ControlType::read(reader)?,
                control_index: reader.byte()?,
                macro_param: reader.flag("macro")?,
            },
            (PLUGIN, 0x0C) => Self::SetPluginEnabled {
                index: reader.byte()?,
                enabled: reader.flag("enabled")?,
            },
            (PLUGIN, 0x0D) => Self::SetPluginsLock(reader.flag("lock")?),
            (PLUGIN, 0x10) => Self::MacroPluginPageIndex(reader.byte()?),
            (PLUGIN, 0x11) => Self::ControlLearned {
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (PLUGIN, 0x12) => Self::ToggleRemotePage,
            (PLUGIN, 0x15) => Self::SetTrackSelectMode,
            (PLUGIN, 0x16) => Self::SetPluginEnableMode,
            (PLUGIN, 0x17) => Self::SetPluginSelectMode,
            (PLUGIN, 0x1A) => {
                let channel = reader.byte()?;
                let [low, high] = reader.array()?;
                let param = u16::from(low) | (u16::from(high) << 7);
                let [low, high] = reader.array()?;
                let value = u16::from(low) | (u16::from(high) << 7);
                Self::PluginParamSweepValue {
                    channel,
                    param,
                    value,
                }
            }
            (PLUGIN, 0x1C) => Self::PluginParamSweepEnd,
            (MIX, 0x01) => {
                let tracks = AllTracks::read(reader)?;
                let knobs = match reader.byte()? {
                    0 => KnobMode::Level,
                    1 => KnobMode::Pan,
                    2 => KnobMode::Send,
                    value => {
                        return Err(DecodeError::Value {
                            what: "a knob mode",
                            value,
                        });
                    }
                };
                let switches = match reader.byte()? {
                    0 => SwitchMode::Mute,
                    1 => SwitchMode::Solo,
                    2 => SwitchMode::Arm,
                    value => {
                        return Err(DecodeError::Value {
                            what: "a switch mode",
                            value,
                        });
                    }
                };
                Self::SetMixAllTracksMode {
                    tracks,
                    knobs,
                    switches,
                    send: reader.byte()?,
                }
            }
            (MIX, 0x02) => Self::SetMixTrackMode(reader.byte()?),
            (MIX, 0x05) => Self::SetAllTracksMode(AllTracks::read(reader)?),
            (MIX, 0x06) => Self::ToggleTrackGrouping(reader.pair()?),
            (MIX, 0x07) => Self::RequestSendTrackNames(reader.byte()?),
            (group, command) => return Err(DecodeError::Unknown { group, command }),
        };
        let left = reader.remaining();
        if left != 0 {
            return Err(DecodeError::Trailing(left));
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daw_started_is_the_documented_bytes() {
        assert_eq!(
            ToRoto::DawStarted.encode(),
            [0xF0, 0x00, 0x22, 0x03, 0x02, 0x0A, 0x01, 0xF7]
        );
    }

    #[test]
    fn the_ping_response_carries_the_daw() {
        assert_eq!(
            ToRoto::PingResponse(Daw::Bitwig).encode(),
            [0xF0, 0x00, 0x22, 0x03, 0x02, 0x0A, 0x03, 0x02, 0xF7]
        );
    }

    #[test]
    fn a_message_from_another_maker_is_not_the_rotos() {
        assert_eq!(
            FromRoto::decode(&[0xF0, 0x7E, 0x00, 0x06, 0x01, 0xF7]),
            Err(DecodeError::NotRoto)
        );
    }

    #[test]
    fn an_unknown_command_says_which() {
        let message = [0xF0, 0x00, 0x22, 0x03, 0x02, 0x0A, 0x7F, 0xF7];
        assert_eq!(
            FromRoto::decode(&message),
            Err(DecodeError::Unknown {
                group: 0x0A,
                command: 0x7F
            })
        );
    }

    #[test]
    fn extra_bytes_are_a_misreading() {
        let mut message = FromRoto::Connected.encode();
        message.insert(message.len() - 1, 0);
        assert_eq!(FromRoto::decode(&message), Err(DecodeError::Trailing(1)));
    }

    #[test]
    fn set_plugin_mode_reads_as_the_real_roto_sends_it_and_as_the_pdf_has_it() {
        let real = [0xF0, 0x00, 0x22, 0x03, 0x02, 0x0B, 0x01, 0x00, 0xF7];
        assert_eq!(FromRoto::decode(&real), Ok(FromRoto::SetPluginMode));
        assert_eq!(FromRoto::SetPluginMode.encode(), real);
        let documented = [0xF0, 0x00, 0x22, 0x03, 0x02, 0x0B, 0x01, 0xF7];
        assert_eq!(FromRoto::decode(&documented), Ok(FromRoto::SetPluginMode));
    }

    #[test]
    fn a_learn_param_past_sixteen_steps_has_no_names() {
        let param = LearnParam {
            index: 3,
            hash: ParamHash::of("x"),
            macro_param: true,
            centre_indent: false,
            steps: 20,
            position: 0,
            name: Label::new("Many"),
            step_names: Vec::new(),
        };
        let bytes = ToRoto::LearnParam(param).encode();
        // Header, group, command, then 2 + 6 + 1 + 1 + 1 + 2 + 13 bytes, then the end.
        assert_eq!(bytes.len(), HEADER.len() + 2 + 26 + 1);
    }
}
