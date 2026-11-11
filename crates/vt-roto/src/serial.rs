//! The Roto's serial API, version 1.3, over its USB serial port at 115200 baud, 8 data bits, no
//! parity and one stop bit. It reads and writes what the Roto stores: its 64 MIDI setups and its
//! PLUGIN setups, each a device's knobs and switches found by the device's hash.
//!
//! A request is `5A <group> <command> <length:2> <data>`, its length the data's, high byte first,
//! and the Roto answers each with `A5 <code> <data>`, the data's size fixed by the request; an
//! error has no data. The Roto also sends a few notices of its own, in the requests' format. A
//! setup can only be changed between `START CONFIG UPDATE` and `END CONFIG UPDATE`.
//!
//! Where the PDF is inconsistent, this follows its overview table and its field lists:
//!
//! *   `ADD PLUGIN` is `03 06`, as the table has it, not `03 05`, its heading's bytes.
//! *   `GET PLUGIN` sends the hash alone, the 8 bytes its length says, without the `PT` its field
//!     list adds.
//! *   `GET KNOB CONTROL CONFIG` sends the setup and the control, 2 bytes, where its length says 1.
//! *   `SET PLUGIN KNOB CONFIG`'s fields come to `0028` bytes before the step names, where its
//!     length says `0027`. The length is always worked out from the bytes written; a real
//!     Roto's `GET PLUGIN KNOB CONFIG` reply has the macro byte the extra one is.
//! *   The PDF's values are hex, so a reply's "10 x 0D" step names are 16 names of 13 bytes.
//! *   Two-byte values other than lengths (an index, a minimum, a maximum, an NRPN address) are
//!     8-bit bytes, high first, as the lengths are, though the PDF doesn't say: a real Roto's
//!     stored knob reads index `00 16` and maximum `3F FF`, 16383 (firmware 3.3.0).

use crate::bytes::{DecodeError, Label, ParamHash, PluginHash, Reader, push_wide};
use crate::sysex::{ControlType, Daw, FirmwareVersion};

/// The port's speed. The rest is 8 data bits, no parity and one stop bit.
pub const BAUD: u32 = 115_200;

/// The first byte of a request or a notice, and of a reply.
pub const REQUEST: u8 = 0x5A;
pub const REPLY: u8 = 0xA5;

/// The groups of commands.
pub const GENERAL: u8 = 0x01;
pub const MIDI: u8 = 0x02;
pub const PLUGIN: u8 = 0x03;

/// How many PLUGIN setups the Roto stores, shared by every program that drives it.
pub const MAX_PLUGIN_SETUPS: usize = 64;

/// How many MIDI setups it has.
pub const MIDI_SETUPS: u8 = 64;

/// How many knobs, and as many switches, a PLUGIN setup has: eight pages of eight.
pub const PLUGIN_CONTROLS: u8 = 64;

/// How many knobs, and as many switches, a MIDI setup has: four pages of eight.
pub const MIDI_CONTROLS: u8 = 32;

/// How many step names a reply carries, used or not, and the most a stored control has.
pub const STEP_NAMES: usize = 16;

/// The most steps a stored control has.
pub const MAX_STORED_STEPS: u8 = 16;

/// How a reply went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Code {
    Success,
    /// `ADD PLUGIN` for a hash the Roto has.
    PluginExists,
    /// No plugin, or no control, with that hash or index.
    NoPlugin,
    Error(u8),
}

impl Code {
    fn byte(self) -> u8 {
        match self {
            Self::Success => 0x00,
            Self::PluginExists => 0xFC,
            Self::NoPlugin => 0xFD,
            Self::Error(code) => code,
        }
    }

    fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::Success,
            0xFC => Self::PluginExists,
            0xFD => Self::NoPlugin,
            code => Self::Error(code),
        }
    }
}

/// Which of its three modes the Roto is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Midi,
    Plugin,
    Mix,
}

impl Mode {
    fn code(self) -> u8 {
        match self {
            Self::Midi => 0,
            Self::Plugin => 1,
            Self::Mix => 2,
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Midi),
            1 => Ok(Self::Plugin),
            2 => Ok(Self::Mix),
            value => Err(DecodeError::Value {
                what: "a mode",
                value,
            }),
        }
    }
}

/// How a knob feels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnobHaptic {
    /// A smooth turn over 300 degrees, `KNOB_300`, with up to two notches at positions of 0 to 127.
    Smooth { indents: [Option<u8>; 2] },
    /// `KNOB_N_STEP`: 2 to 16 clicks, each of which can be named.
    Steps(u8),
    /// A smooth turn with a notch at the top, `KNOB_300_CENTRE_INDENT`.
    CentreIndent,
}

impl KnobHaptic {
    fn steps(self) -> u8 {
        match self {
            Self::Steps(steps) => steps,
            _ => 0,
        }
    }

    fn write(self, out: &mut Vec<u8>) {
        let (mode, indents) = match self {
            Self::Smooth { indents } => (0, indents),
            Self::Steps(_) => (1, [None; 2]),
            Self::CentreIndent => (2, [None; 2]),
        };
        out.push(mode);
        out.extend(indents.map(|indent| indent.unwrap_or(0xFF)));
        out.push(self.steps());
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        let mode = reader.byte()?;
        let indents = [reader.byte()?, reader.byte()?].map(|at| (at != 0xFF).then_some(at));
        let steps = reader.byte()?;
        match mode {
            0 => Ok(Self::Smooth { indents }),
            1 => Ok(Self::Steps(steps)),
            2 => Ok(Self::CentreIndent),
            value => Err(DecodeError::Value {
                what: "a knob haptic mode",
                value,
            }),
        }
    }
}

/// How a switch feels: back up when let go, or on and off with each press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitchHaptic {
    Push,
    Toggle,
}

impl SwitchHaptic {
    fn code(self) -> u8 {
        match self {
            Self::Push => 0,
            Self::Toggle => 1,
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Push),
            1 => Ok(Self::Toggle),
            value => Err(DecodeError::Value {
                what: "a switch haptic mode",
                value,
            }),
        }
    }
}

/// One knob of a PLUGIN setup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginKnobConfig {
    pub plugin: PluginHash,
    /// Which knob, 0 to 63: eight pages of eight.
    pub control: u8,
    pub param_index: u16,
    pub param_hash: ParamHash,
    pub macro_param: bool,
    pub min: u16,
    pub max: u16,
    pub name: Label,
    pub colour: u8,
    pub haptic: KnobHaptic,
    /// A name for each step of a stepped knob, as many as its steps; none otherwise.
    pub step_names: Vec<Label>,
}

/// One switch of a PLUGIN setup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PluginSwitchConfig {
    pub plugin: PluginHash,
    pub control: u8,
    pub param_index: u16,
    pub param_hash: ParamHash,
    pub min: u8,
    pub max: u8,
    pub name: Label,
    pub colour: u8,
    pub light_on: u8,
    pub light_off: u8,
    pub haptic: SwitchHaptic,
    /// 0 for a plain switch, or 2 to 16 positions, each with a name.
    pub steps: u8,
    pub step_names: Vec<Label>,
}

/// What a MIDI setup's knob sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KnobMessage {
    Cc7,
    Cc14,
    Nrpn7,
    Nrpn14,
}

/// What a MIDI setup's switch sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SwitchMessage {
    Cc7,
    Cc14,
    Nrpn7,
    Nrpn14,
    ProgramChange,
    Note,
}

fn knob_message_code(message: KnobMessage) -> u8 {
    match message {
        KnobMessage::Cc7 => 0,
        KnobMessage::Cc14 => 1,
        KnobMessage::Nrpn7 => 2,
        KnobMessage::Nrpn14 => 3,
    }
}

fn switch_message_code(message: SwitchMessage) -> u8 {
    match message {
        SwitchMessage::Cc7 => 0,
        SwitchMessage::Cc14 => 1,
        SwitchMessage::Nrpn7 => 2,
        SwitchMessage::Nrpn14 => 3,
        SwitchMessage::ProgramChange => 4,
        SwitchMessage::Note => 5,
    }
}

fn read_switch_message(reader: &mut Reader) -> Result<SwitchMessage, DecodeError> {
    match reader.byte()? {
        0 => Ok(SwitchMessage::Cc7),
        1 => Ok(SwitchMessage::Cc14),
        2 => Ok(SwitchMessage::Nrpn7),
        3 => Ok(SwitchMessage::Nrpn14),
        4 => Ok(SwitchMessage::ProgramChange),
        5 => Ok(SwitchMessage::Note),
        value => Err(DecodeError::Value {
            what: "a switch's message",
            value,
        }),
    }
}

/// One knob of a MIDI setup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MidiKnobConfig {
    pub setup: u8,
    /// Which knob, 0 to 31.
    pub control: u8,
    pub message: KnobMessage,
    /// The MIDI channel, 1 to 16.
    pub channel: u8,
    pub param: u8,
    pub nrpn: u16,
    pub min: u16,
    pub max: u16,
    pub name: Label,
    pub colour: u8,
    pub haptic: KnobHaptic,
    pub step_names: Vec<Label>,
}

/// One switch of a MIDI setup.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MidiSwitchConfig {
    pub setup: u8,
    pub control: u8,
    pub message: SwitchMessage,
    pub channel: u8,
    pub param: u8,
    pub nrpn: u16,
    pub min: u16,
    pub max: u16,
    pub name: Label,
    pub colour: u8,
    pub light_on: u8,
    pub light_off: u8,
    pub haptic: SwitchHaptic,
    pub steps: u8,
    pub step_names: Vec<Label>,
}

/// What kind a stored PLUGIN setup is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoredKind {
    Normal,
    MacroLearned,
    MacroAutoConfig,
    MacroDefault,
    ThirdParty,
}

impl StoredKind {
    fn code(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::MacroLearned => 1,
            Self::MacroAutoConfig => 2,
            Self::MacroDefault => 3,
            Self::ThirdParty => 4,
        }
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        match reader.byte()? {
            0 => Ok(Self::Normal),
            1 => Ok(Self::MacroLearned),
            2 => Ok(Self::MacroAutoConfig),
            3 => Ok(Self::MacroDefault),
            4 => Ok(Self::ThirdParty),
            value => Err(DecodeError::Value {
                what: "a stored plugin's type",
                value,
            }),
        }
    }
}

/// A stored PLUGIN setup as the Roto lists it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoredPlugin {
    pub hash: PluginHash,
    pub name: Label,
    pub kind: StoredKind,
}

impl StoredPlugin {
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.hash.0);
        self.name.write(out);
        out.push(self.kind.code());
    }

    fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        Ok(Self {
            hash: PluginHash(reader.array()?),
            name: reader.label()?,
            kind: StoredKind::read(reader)?,
        })
    }
}

/// A request to the Roto.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Request {
    // GENERAL, 01.
    GetFirmwareVersion,
    GetMode,
    SetMode {
        mode: Mode,
        page: u8,
    },
    StartConfigUpdate,
    EndConfigUpdate,
    /// Wipes every setup. Nothing in vimtracker sends it; it's here so every command has a type.
    FactoryReset,
    // MIDI, 02.
    GetCurrentSetup,
    GetSetup(u8),
    SetSetup(u8),
    SetSetupName {
        setup: u8,
        name: Label,
    },
    GetKnobControlConfig {
        setup: u8,
        control: u8,
    },
    GetSwitchControlConfig {
        setup: u8,
        control: u8,
    },
    SetKnobControlConfig(MidiKnobConfig),
    SetSwitchControlConfig(MidiSwitchConfig),
    ClearControlConfig {
        setup: u8,
        control: ControlType,
        index: u8,
    },
    ClearMidiSetup(u8),
    // PLUGIN, 03.
    GetCurrentPlugin,
    GetFirstPlugin,
    GetNextPlugin,
    GetPlugin(PluginHash),
    AddPlugin {
        hash: PluginHash,
        name: Label,
    },
    SetPluginName {
        hash: PluginHash,
        name: Label,
    },
    ClearPlugin(PluginHash),
    GetPluginKnobConfig {
        plugin: PluginHash,
        control: u8,
    },
    GetPluginSwitchConfig {
        plugin: PluginHash,
        control: u8,
    },
    SetPluginKnobConfig(PluginKnobConfig),
    SetPluginSwitchConfig(PluginSwitchConfig),
    ClearPluginControlConfig {
        plugin: PluginHash,
        control: ControlType,
        index: u8,
    },
}

/// What the Roto answers a request with, when it succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reply {
    /// Success with nothing more, as every change answers.
    Done,
    FirmwareVersion(FirmwareVersion),
    Mode {
        mode: Mode,
        page: u8,
    },
    Setup {
        index: u8,
        name: Label,
    },
    MidiKnob(MidiKnobConfig),
    MidiSwitch(MidiSwitchConfig),
    CurrentPlugin {
        plugin: StoredPlugin,
        daw: Daw,
    },
    Plugin(StoredPlugin),
    PluginKnob(PluginKnobConfig),
    PluginSwitch(PluginSwitchConfig),
}

/// What the Roto says without being asked.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Notice {
    ModeSet {
        mode: Mode,
        page: u8,
    },
    SetupSelected(u8),
    MidiControlLearned {
        setup: u8,
        control: ControlType,
        index: u8,
    },
    /// A PLUGIN setup selected on the Roto, or none.
    PluginSelected(Option<PluginHash>),
    PluginControlLearned {
        plugin: PluginHash,
        control: ControlType,
        index: u8,
    },
}

/// A request's or a notice's whole frame from its group, command and data.
fn frame(group: u8, command: u8, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + data.len());
    out.extend_from_slice(&[REQUEST, group, command]);
    push_wide(&mut out, data.len() as u16);
    out.extend_from_slice(data);
    out
}

/// How long a request's or a notice's frame is, once its first five bytes are in.
pub fn frame_len(bytes: &[u8]) -> Option<usize> {
    match bytes {
        [REQUEST, _, _, high, low, ..] => Some(5 + usize::from(u16::from_be_bytes([*high, *low]))),
        _ => None,
    }
}

/// A frame's group, command and data, checking its length.
fn unframe(frame: &[u8]) -> Result<(u8, u8, &[u8]), DecodeError> {
    let [REQUEST, group, command, high, low, data @ ..] = frame else {
        return Err(if frame.first() == Some(&REQUEST) {
            DecodeError::Short
        } else {
            DecodeError::NotRoto
        });
    };
    let length = usize::from(u16::from_be_bytes([*high, *low]));
    match data.len().cmp(&length) {
        std::cmp::Ordering::Less => Err(DecodeError::Short),
        std::cmp::Ordering::Greater => Err(DecodeError::Trailing(data.len() - length)),
        std::cmp::Ordering::Equal => Ok((*group, *command, data)),
    }
}

fn write_names(out: &mut Vec<u8>, names: &[Label], count: usize) {
    let empty = Label::empty();
    for at in 0..count {
        names.get(at).unwrap_or(&empty).write(out);
    }
}

/// Reads `count` names and keeps the first `keep`.
fn read_names(reader: &mut Reader, count: usize, keep: usize) -> Result<Vec<Label>, DecodeError> {
    let mut names = Vec::with_capacity(keep);
    for at in 0..count {
        let name = reader.label()?;
        if at < keep {
            names.push(name);
        }
    }
    Ok(names)
}

/// How many of a reply's 16 names are a stepped control's.
fn kept_names(steps: u8) -> usize {
    usize::from(steps).min(STEP_NAMES)
}

impl PluginKnobConfig {
    /// Its fields, with `names` step names after them: its steps' for a change, 16 for a reply.
    fn write(&self, out: &mut Vec<u8>, names: usize) {
        out.extend_from_slice(&self.plugin.0);
        out.push(self.control);
        push_wide(out, self.param_index);
        out.extend_from_slice(&self.param_hash.0);
        out.push(u8::from(self.macro_param));
        push_wide(out, self.min);
        push_wide(out, self.max);
        self.name.write(out);
        out.push(self.colour);
        self.haptic.write(out);
        write_names(out, &self.step_names, names);
    }

    fn read(reader: &mut Reader, reply: bool) -> Result<Self, DecodeError> {
        let plugin = PluginHash(reader.array()?);
        let control = reader.byte()?;
        let param_index = reader.wide()?;
        let param_hash = ParamHash(reader.array()?);
        let macro_param = reader.flag("macro")?;
        let min = reader.wide()?;
        let max = reader.wide()?;
        let name = reader.label()?;
        let colour = reader.byte()?;
        let haptic = KnobHaptic::read(reader)?;
        let steps = haptic.steps();
        let count = if reply {
            STEP_NAMES
        } else {
            usize::from(steps)
        };
        let step_names = read_names(reader, count, kept_names(steps))?;
        Ok(Self {
            plugin,
            control,
            param_index,
            param_hash,
            macro_param,
            min,
            max,
            name,
            colour,
            haptic,
            step_names,
        })
    }
}

impl PluginSwitchConfig {
    fn write(&self, out: &mut Vec<u8>, names: usize) {
        out.extend_from_slice(&self.plugin.0);
        out.push(self.control);
        push_wide(out, self.param_index);
        out.extend_from_slice(&self.param_hash.0);
        out.push(self.min);
        out.push(self.max);
        self.name.write(out);
        out.extend_from_slice(&[
            self.colour,
            self.light_on,
            self.light_off,
            self.haptic.code(),
            self.steps,
        ]);
        write_names(out, &self.step_names, names);
    }

    fn read(reader: &mut Reader, reply: bool) -> Result<Self, DecodeError> {
        let plugin = PluginHash(reader.array()?);
        let control = reader.byte()?;
        let param_index = reader.wide()?;
        let param_hash = ParamHash(reader.array()?);
        let min = reader.byte()?;
        let max = reader.byte()?;
        let name = reader.label()?;
        let colour = reader.byte()?;
        let light_on = reader.byte()?;
        let light_off = reader.byte()?;
        let haptic = SwitchHaptic::read(reader)?;
        let steps = reader.byte()?;
        let count = if reply {
            STEP_NAMES
        } else {
            usize::from(steps)
        };
        let step_names = read_names(reader, count, kept_names(steps))?;
        Ok(Self {
            plugin,
            control,
            param_index,
            param_hash,
            min,
            max,
            name,
            colour,
            light_on,
            light_off,
            haptic,
            steps,
            step_names,
        })
    }
}

impl MidiKnobConfig {
    fn write(&self, out: &mut Vec<u8>, names: usize) {
        out.extend_from_slice(&[
            self.setup,
            self.control,
            knob_message_code(self.message),
            self.channel,
            self.param,
        ]);
        push_wide(out, self.nrpn);
        push_wide(out, self.min);
        push_wide(out, self.max);
        self.name.write(out);
        out.push(self.colour);
        self.haptic.write(out);
        write_names(out, &self.step_names, names);
    }

    fn read(reader: &mut Reader, reply: bool) -> Result<Self, DecodeError> {
        let setup = reader.byte()?;
        let control = reader.byte()?;
        let message = match reader.byte()? {
            0 => KnobMessage::Cc7,
            1 => KnobMessage::Cc14,
            2 => KnobMessage::Nrpn7,
            3 => KnobMessage::Nrpn14,
            value => {
                return Err(DecodeError::Value {
                    what: "a knob's message",
                    value,
                });
            }
        };
        let channel = reader.byte()?;
        let param = reader.byte()?;
        let nrpn = reader.wide()?;
        let min = reader.wide()?;
        let max = reader.wide()?;
        let name = reader.label()?;
        let colour = reader.byte()?;
        let haptic = KnobHaptic::read(reader)?;
        let steps = haptic.steps();
        let count = if reply {
            STEP_NAMES
        } else {
            usize::from(steps)
        };
        let step_names = read_names(reader, count, kept_names(steps))?;
        Ok(Self {
            setup,
            control,
            message,
            channel,
            param,
            nrpn,
            min,
            max,
            name,
            colour,
            haptic,
            step_names,
        })
    }
}

impl MidiSwitchConfig {
    fn write(&self, out: &mut Vec<u8>, names: usize) {
        out.extend_from_slice(&[
            self.setup,
            self.control,
            switch_message_code(self.message),
            self.channel,
            self.param,
        ]);
        push_wide(out, self.nrpn);
        push_wide(out, self.min);
        push_wide(out, self.max);
        self.name.write(out);
        out.extend_from_slice(&[
            self.colour,
            self.light_on,
            self.light_off,
            self.haptic.code(),
            self.steps,
        ]);
        write_names(out, &self.step_names, names);
    }

    fn read(reader: &mut Reader, reply: bool) -> Result<Self, DecodeError> {
        let setup = reader.byte()?;
        let control = reader.byte()?;
        let message = read_switch_message(reader)?;
        let channel = reader.byte()?;
        let param = reader.byte()?;
        let nrpn = reader.wide()?;
        let min = reader.wide()?;
        let max = reader.wide()?;
        let name = reader.label()?;
        let colour = reader.byte()?;
        let light_on = reader.byte()?;
        let light_off = reader.byte()?;
        let haptic = SwitchHaptic::read(reader)?;
        let steps = reader.byte()?;
        let count = if reply {
            STEP_NAMES
        } else {
            usize::from(steps)
        };
        let step_names = read_names(reader, count, kept_names(steps))?;
        Ok(Self {
            setup,
            control,
            message,
            channel,
            param,
            nrpn,
            min,
            max,
            name,
            colour,
            light_on,
            light_off,
            haptic,
            steps,
            step_names,
        })
    }
}

/// The kind of reply a request has, to read it by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplyShape {
    Done,
    FirmwareVersion,
    Mode,
    Setup,
    MidiKnob,
    MidiSwitch,
    CurrentPlugin,
    Plugin,
    PluginKnob,
    PluginSwitch,
}

impl ReplyShape {
    /// How many bytes follow the code on success.
    fn len(self) -> usize {
        // A name is 13 bytes, a reply's step names 16 of them.
        let names = STEP_NAMES * 13;
        match self {
            Self::Done => 0,
            Self::FirmwareVersion => 3 + FirmwareVersion::COMMIT_BYTES,
            Self::Mode => 2,
            Self::Setup => 1 + 13,
            Self::MidiKnob => 5 + 6 + 13 + 5 + names,
            Self::MidiSwitch => 5 + 6 + 13 + 5 + names,
            Self::CurrentPlugin => 8 + 13 + 1 + 1,
            Self::Plugin => 8 + 13 + 1,
            Self::PluginKnob => 8 + 1 + 2 + 6 + 1 + 4 + 13 + 5 + names,
            Self::PluginSwitch => 8 + 1 + 2 + 6 + 2 + 13 + 5 + names,
        }
    }
}

impl Request {
    fn reply_shape(&self) -> ReplyShape {
        match self {
            Self::GetFirmwareVersion => ReplyShape::FirmwareVersion,
            Self::GetMode => ReplyShape::Mode,
            Self::GetCurrentSetup | Self::GetSetup(_) => ReplyShape::Setup,
            Self::GetKnobControlConfig { .. } => ReplyShape::MidiKnob,
            Self::GetSwitchControlConfig { .. } => ReplyShape::MidiSwitch,
            Self::GetCurrentPlugin => ReplyShape::CurrentPlugin,
            Self::GetFirstPlugin | Self::GetNextPlugin | Self::GetPlugin(_) => ReplyShape::Plugin,
            Self::GetPluginKnobConfig { .. } => ReplyShape::PluginKnob,
            Self::GetPluginSwitchConfig { .. } => ReplyShape::PluginSwitch,
            _ => ReplyShape::Done,
        }
    }

    /// How many bytes its reply has after `A5` and the code, when it succeeds.
    pub fn reply_len(&self) -> usize {
        self.reply_shape().len()
    }

    /// Whether it only asks, changing nothing the Roto stores or shows. `GET NEXT PLUGIN` moves
    /// the Roto's place in its list, which nothing else reads.
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::GetFirmwareVersion
                | Self::GetMode
                | Self::GetCurrentSetup
                | Self::GetSetup(_)
                | Self::GetKnobControlConfig { .. }
                | Self::GetSwitchControlConfig { .. }
                | Self::GetCurrentPlugin
                | Self::GetFirstPlugin
                | Self::GetNextPlugin
                | Self::GetPlugin(_)
                | Self::GetPluginKnobConfig { .. }
                | Self::GetPluginSwitchConfig { .. }
        )
    }

    /// Whether the Roto only takes it inside a config update.
    pub fn needs_config_update(&self) -> bool {
        matches!(
            self,
            Self::SetSetupName { .. }
                | Self::SetKnobControlConfig(_)
                | Self::SetSwitchControlConfig(_)
                | Self::ClearControlConfig { .. }
                | Self::ClearMidiSetup(_)
                | Self::AddPlugin { .. }
                | Self::SetPluginName { .. }
                | Self::ClearPlugin(_)
                | Self::SetPluginKnobConfig(_)
                | Self::SetPluginSwitchConfig(_)
                | Self::ClearPluginControlConfig { .. }
        )
    }

    /// The whole frame, ready to send.
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let (group, command) = match self {
            Self::GetFirmwareVersion => (GENERAL, 0x01),
            Self::GetMode => (GENERAL, 0x02),
            Self::SetMode { mode, page } => {
                data.extend_from_slice(&[mode.code(), *page]);
                (GENERAL, 0x03)
            }
            Self::StartConfigUpdate => (GENERAL, 0x04),
            Self::EndConfigUpdate => (GENERAL, 0x05),
            Self::FactoryReset => (GENERAL, 0x06),
            Self::GetCurrentSetup => (MIDI, 0x01),
            Self::GetSetup(setup) => {
                data.push(*setup);
                (MIDI, 0x02)
            }
            Self::SetSetup(setup) => {
                data.push(*setup);
                (MIDI, 0x03)
            }
            Self::SetSetupName { setup, name } => {
                data.push(*setup);
                name.write(&mut data);
                (MIDI, 0x04)
            }
            Self::GetKnobControlConfig { setup, control } => {
                data.extend_from_slice(&[*setup, *control]);
                (MIDI, 0x05)
            }
            Self::GetSwitchControlConfig { setup, control } => {
                data.extend_from_slice(&[*setup, *control]);
                (MIDI, 0x06)
            }
            Self::SetKnobControlConfig(config) => {
                config.write(&mut data, usize::from(config.haptic.steps()));
                (MIDI, 0x07)
            }
            Self::SetSwitchControlConfig(config) => {
                config.write(&mut data, usize::from(config.steps));
                (MIDI, 0x08)
            }
            Self::ClearControlConfig {
                setup,
                control,
                index,
            } => {
                data.extend_from_slice(&[*setup, control.code(), *index]);
                (MIDI, 0x09)
            }
            Self::ClearMidiSetup(setup) => {
                data.push(*setup);
                (MIDI, 0x0A)
            }
            Self::GetCurrentPlugin => (PLUGIN, 0x01),
            Self::GetFirstPlugin => (PLUGIN, 0x02),
            Self::GetNextPlugin => (PLUGIN, 0x03),
            Self::GetPlugin(hash) => {
                data.extend_from_slice(&hash.0);
                (PLUGIN, 0x04)
            }
            Self::AddPlugin { hash, name } => {
                data.extend_from_slice(&hash.0);
                name.write(&mut data);
                (PLUGIN, 0x06)
            }
            Self::SetPluginName { hash, name } => {
                data.extend_from_slice(&hash.0);
                name.write(&mut data);
                (PLUGIN, 0x07)
            }
            Self::ClearPlugin(hash) => {
                data.extend_from_slice(&hash.0);
                (PLUGIN, 0x08)
            }
            Self::GetPluginKnobConfig { plugin, control } => {
                data.extend_from_slice(&plugin.0);
                data.push(*control);
                (PLUGIN, 0x09)
            }
            Self::GetPluginSwitchConfig { plugin, control } => {
                data.extend_from_slice(&plugin.0);
                data.push(*control);
                (PLUGIN, 0x0A)
            }
            Self::SetPluginKnobConfig(config) => {
                config.write(&mut data, usize::from(config.haptic.steps()));
                (PLUGIN, 0x0B)
            }
            Self::SetPluginSwitchConfig(config) => {
                config.write(&mut data, usize::from(config.steps));
                (PLUGIN, 0x0C)
            }
            Self::ClearPluginControlConfig {
                plugin,
                control,
                index,
            } => {
                data.extend_from_slice(&plugin.0);
                data.extend_from_slice(&[control.code(), *index]);
                (PLUGIN, 0x0D)
            }
        };
        frame(group, command, &data)
    }

    /// A request from its whole frame, as the fake Roto reads it.
    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let (group, command, data) = unframe(frame)?;
        let mut reader = Reader::new(data);
        let reader = &mut reader;
        let request = match (group, command) {
            (GENERAL, 0x01) => Self::GetFirmwareVersion,
            (GENERAL, 0x02) => Self::GetMode,
            (GENERAL, 0x03) => Self::SetMode {
                mode: Mode::read(reader)?,
                page: reader.byte()?,
            },
            (GENERAL, 0x04) => Self::StartConfigUpdate,
            (GENERAL, 0x05) => Self::EndConfigUpdate,
            (GENERAL, 0x06) => Self::FactoryReset,
            (MIDI, 0x01) => Self::GetCurrentSetup,
            (MIDI, 0x02) => Self::GetSetup(reader.byte()?),
            (MIDI, 0x03) => Self::SetSetup(reader.byte()?),
            (MIDI, 0x04) => Self::SetSetupName {
                setup: reader.byte()?,
                name: reader.label()?,
            },
            (MIDI, 0x05) => Self::GetKnobControlConfig {
                setup: reader.byte()?,
                control: reader.byte()?,
            },
            (MIDI, 0x06) => Self::GetSwitchControlConfig {
                setup: reader.byte()?,
                control: reader.byte()?,
            },
            (MIDI, 0x07) => Self::SetKnobControlConfig(MidiKnobConfig::read(reader, false)?),
            (MIDI, 0x08) => Self::SetSwitchControlConfig(MidiSwitchConfig::read(reader, false)?),
            (MIDI, 0x09) => Self::ClearControlConfig {
                setup: reader.byte()?,
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (MIDI, 0x0A) => Self::ClearMidiSetup(reader.byte()?),
            (PLUGIN, 0x01) => Self::GetCurrentPlugin,
            (PLUGIN, 0x02) => Self::GetFirstPlugin,
            (PLUGIN, 0x03) => Self::GetNextPlugin,
            (PLUGIN, 0x04) => Self::GetPlugin(PluginHash(reader.array()?)),
            (PLUGIN, 0x06) => Self::AddPlugin {
                hash: PluginHash(reader.array()?),
                name: reader.label()?,
            },
            (PLUGIN, 0x07) => Self::SetPluginName {
                hash: PluginHash(reader.array()?),
                name: reader.label()?,
            },
            (PLUGIN, 0x08) => Self::ClearPlugin(PluginHash(reader.array()?)),
            (PLUGIN, 0x09) => Self::GetPluginKnobConfig {
                plugin: PluginHash(reader.array()?),
                control: reader.byte()?,
            },
            (PLUGIN, 0x0A) => Self::GetPluginSwitchConfig {
                plugin: PluginHash(reader.array()?),
                control: reader.byte()?,
            },
            (PLUGIN, 0x0B) => Self::SetPluginKnobConfig(PluginKnobConfig::read(reader, false)?),
            (PLUGIN, 0x0C) => Self::SetPluginSwitchConfig(PluginSwitchConfig::read(reader, false)?),
            (PLUGIN, 0x0D) => Self::ClearPluginControlConfig {
                plugin: PluginHash(reader.array()?),
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (group, command) => return Err(DecodeError::Unknown { group, command }),
        };
        let left = reader.remaining();
        if left != 0 {
            return Err(DecodeError::Trailing(left));
        }
        Ok(request)
    }

    /// Its reply from the bytes the Roto sent, `A5` first: the reply, or the code it failed with.
    pub fn decode_reply(&self, bytes: &[u8]) -> Result<Result<Reply, Code>, DecodeError> {
        let mut reader = Reader::new(bytes);
        if reader.byte()? != REPLY {
            return Err(DecodeError::NotRoto);
        }
        let code = Code::from_byte(reader.byte()?);
        if code != Code::Success {
            reader.finish()?;
            return Ok(Err(code));
        }
        let reader = &mut reader;
        let reply = match self.reply_shape() {
            ReplyShape::Done => Reply::Done,
            ReplyShape::FirmwareVersion => Reply::FirmwareVersion(FirmwareVersion::read(reader)?),
            ReplyShape::Mode => Reply::Mode {
                mode: Mode::read(reader)?,
                page: reader.byte()?,
            },
            ReplyShape::Setup => Reply::Setup {
                index: reader.byte()?,
                name: reader.label()?,
            },
            ReplyShape::MidiKnob => Reply::MidiKnob(MidiKnobConfig::read(reader, true)?),
            ReplyShape::MidiSwitch => Reply::MidiSwitch(MidiSwitchConfig::read(reader, true)?),
            ReplyShape::CurrentPlugin => Reply::CurrentPlugin {
                plugin: StoredPlugin::read(reader)?,
                daw: Daw::from_code(reader.byte()?)?,
            },
            ReplyShape::Plugin => Reply::Plugin(StoredPlugin::read(reader)?),
            ReplyShape::PluginKnob => Reply::PluginKnob(PluginKnobConfig::read(reader, true)?),
            ReplyShape::PluginSwitch => {
                Reply::PluginSwitch(PluginSwitchConfig::read(reader, true)?)
            }
        };
        let left = reader.remaining();
        if left != 0 {
            return Err(DecodeError::Trailing(left));
        }
        Ok(Ok(reply))
    }
}

/// A failed reply's bytes, as the fake Roto sends one.
pub fn encode_failure(code: Code) -> Vec<u8> {
    vec![REPLY, code.byte()]
}

impl Reply {
    /// A successful reply's bytes, `A5 00` first, as the fake Roto sends it.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![REPLY, Code::Success.byte()];
        match self {
            Self::Done => {}
            Self::FirmwareVersion(version) => version.write(&mut out),
            Self::Mode { mode, page } => out.extend_from_slice(&[mode.code(), *page]),
            Self::Setup { index, name } => {
                out.push(*index);
                name.write(&mut out);
            }
            Self::MidiKnob(config) => config.write(&mut out, STEP_NAMES),
            Self::MidiSwitch(config) => config.write(&mut out, STEP_NAMES),
            Self::CurrentPlugin { plugin, daw } => {
                plugin.write(&mut out);
                out.push(match daw {
                    Daw::Ableton => 1,
                    Daw::Bitwig => 2,
                    Daw::Logic => 3,
                });
            }
            Self::Plugin(plugin) => plugin.write(&mut out),
            Self::PluginKnob(config) => config.write(&mut out, STEP_NAMES),
            Self::PluginSwitch(config) => config.write(&mut out, STEP_NAMES),
        }
        out
    }
}

impl Notice {
    pub fn encode(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let (group, command) = match self {
            Self::ModeSet { mode, page } => {
                data.extend_from_slice(&[mode.code(), *page]);
                (GENERAL, 0x03)
            }
            Self::SetupSelected(setup) => {
                data.push(*setup);
                (MIDI, 0x03)
            }
            Self::MidiControlLearned {
                setup,
                control,
                index,
            } => {
                data.extend_from_slice(&[*setup, control.code(), *index]);
                (MIDI, 0x0B)
            }
            Self::PluginSelected(hash) => {
                data.extend_from_slice(&hash.unwrap_or(PluginHash::NONE).0);
                (PLUGIN, 0x05)
            }
            Self::PluginControlLearned {
                plugin,
                control,
                index,
            } => {
                data.extend_from_slice(&plugin.0);
                data.extend_from_slice(&[control.code(), *index]);
                (PLUGIN, 0x0E)
            }
        };
        frame(group, command, &data)
    }

    pub fn decode(frame: &[u8]) -> Result<Self, DecodeError> {
        let (group, command, data) = unframe(frame)?;
        let mut reader = Reader::new(data);
        let reader = &mut reader;
        let notice = match (group, command) {
            (GENERAL, 0x03) => Self::ModeSet {
                mode: Mode::read(reader)?,
                page: reader.byte()?,
            },
            (MIDI, 0x03) => Self::SetupSelected(reader.byte()?),
            (MIDI, 0x0B) => Self::MidiControlLearned {
                setup: reader.byte()?,
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (PLUGIN, 0x05) => {
                let hash = PluginHash(reader.array()?);
                Self::PluginSelected((hash != PluginHash::NONE).then_some(hash))
            }
            (PLUGIN, 0x0E) => Self::PluginControlLearned {
                plugin: PluginHash(reader.array()?),
                control: ControlType::read(reader)?,
                index: reader.byte()?,
            },
            (group, command) => return Err(DecodeError::Unknown { group, command }),
        };
        let left = reader.remaining();
        if left != 0 {
            return Err(DecodeError::Trailing(left));
        }
        Ok(notice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_firmware_version_is_the_documented_bytes() {
        assert_eq!(
            Request::GetFirmwareVersion.encode(),
            [0x5A, 0x01, 0x01, 0x00, 0x00]
        );
    }

    #[test]
    fn set_mode_carries_its_length() {
        assert_eq!(
            Request::SetMode {
                mode: Mode::Mix,
                page: 8
            }
            .encode(),
            [0x5A, 0x01, 0x03, 0x00, 0x02, 0x02, 0x08]
        );
    }

    #[test]
    fn the_documented_lengths_of_the_changes() {
        // `SET SETUP NAME` is 000E, `ADD PLUGIN` 0015, `CLEAR PLUGIN CONTROL CONFIG` 000A.
        let hash = PluginHash::of("x");
        let name = Label::new("Delay");
        let cases = [
            (
                Request::SetSetupName {
                    setup: 0,
                    name: name.clone(),
                },
                0x0E,
            ),
            (Request::AddPlugin { hash, name }, 0x15),
            (
                Request::ClearPluginControlConfig {
                    plugin: hash,
                    control: ControlType::Knob,
                    index: 0,
                },
                0x0A,
            ),
        ];
        for (request, length) in cases {
            assert_eq!(
                frame_len(&request.encode()),
                Some(5 + length),
                "{request:?}"
            );
        }
    }

    #[test]
    fn a_switch_change_is_0025_bytes_and_its_names() {
        let config = PluginSwitchConfig {
            plugin: PluginHash::of("x"),
            control: 0,
            param_index: 0,
            param_hash: ParamHash::of("on"),
            min: 0,
            max: 1,
            name: Label::new("On"),
            colour: 0,
            light_on: 0,
            light_off: 0,
            haptic: SwitchHaptic::Toggle,
            steps: 2,
            step_names: vec![Label::new("Off"), Label::new("On")],
        };
        let frame = Request::SetPluginSwitchConfig(config).encode();
        assert_eq!(frame_len(&frame), Some(5 + 0x25 + 2 * 13));
    }

    #[test]
    fn an_error_reply_has_no_data() {
        let reply = Request::GetCurrentPlugin.decode_reply(&[0xA5, 0xFD]);
        assert_eq!(reply, Ok(Err(Code::NoPlugin)));
    }

    #[test]
    fn only_requests_that_ask_are_read_only() {
        assert!(Request::GetFirstPlugin.is_read_only());
        assert!(!Request::StartConfigUpdate.is_read_only());
        assert!(!Request::FactoryReset.is_read_only());
        assert!(
            !Request::SetMode {
                mode: Mode::Mix,
                page: 0
            }
            .is_read_only()
        );
    }
}
