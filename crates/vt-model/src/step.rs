//! One position in a phrase, and the fields it holds.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::constants::FX_SLOTS;
use crate::settings::{Kind, SettingId};

/// Lowest note the format can write, `c_1`: the C in octave -1.
pub const MIN_NOTE: u8 = 12;
/// Highest note the format can write, `G-8`.
pub const MAX_NOTE: u8 = 127;
/// Middle C, MIDI note 60 at 261.63 Hz, which is `C-3`.
pub const MIDDLE_C: u8 = 60;
/// The octave MIDI note 0 is in. Octaves are numbered as Ableton Live, Logic and Cubase number
/// them, from -2 at MIDI 0, so middle C is `C-3` and a note reads the same here as in Live.
const OCTAVE_OF_NOTE_ZERO: i16 = -2;
/// The lowest octave a step's note can be in, the one [`MIN_NOTE`] starts.
pub const LOWEST_OCTAVE: i8 = -1;
/// The highest octave a step's note can be in, the one [`MAX_NOTE`] is in.
pub const HIGHEST_OCTAVE: i8 = 8;
/// The names of the notes in an octave from 0 up, before the octave's digit: a letter, then `-`
/// for a natural or `#` for a sharp.
pub(crate) const NOTE_NAMES: [&str; 12] = [
    "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
];
/// The names of the notes in an octave below 0, before the digit of how far below it is, so
/// octave -1 fits the same three characters: a small letter, then `_` for a natural or `+` for a
/// sharp, as Furnace writes them. `c_1` is the C an octave under `C-0`.
pub(crate) const NOTE_NAMES_BELOW_ZERO: [&str; 12] = [
    "c_", "c+", "d_", "d+", "e_", "f_", "f+", "g_", "g+", "a_", "a+", "b_",
];

/// The octave a MIDI note is in, as its name shows it: 3 for middle C.
pub fn octave_of(midi: u8) -> i8 {
    (i16::from(midi) / 12 + OCTAVE_OF_NOTE_ZERO) as i8
}

/// The MIDI number of the C an octave starts on: 60 for octave 3. It can be outside the notes a
/// step holds, for an octave out of their range.
pub fn octave_start(octave: i8) -> i16 {
    (i16::from(octave) - OCTAVE_OF_NOTE_ZERO) * 12
}
/// The highest velocity, full level, which is also MIDI's.
pub const MAX_VELOCITY: u8 = 0x7F;
/// The lowest velocity that accents a note on a voice with an accent of its own, like `Acid`.
/// It's above the velocity notes are written with, `64`, so one step up the ladder, to `74`,
/// accents a note, and so does `7F` or none.
pub const ACCENT_VELOCITY: u8 = 0x70;

/// A note field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Note {
    #[default]
    Empty,
    Off,
    /// A MIDI note from [`MIN_NOTE`] to [`MAX_NOTE`].
    On(u8),
}

/// What an FX slot does: an event, which acts on the note once, or a lock of one of the
/// settings in the registry, which holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FxCommand {
    Del,
    Kil,
    Off,
    Ret,
    Cha,
    /// Plays the row only when a trig condition holds, which its value names.
    Cnd,
    /// Moves the row's note on by its value each time the row plays, an accumulator.
    Acu,
    /// Goes round the row's note, the note and `x` above it, and the note and `y` above it, an
    /// arpeggio.
    Arp,
    /// Sets a setting, by its id, so it belongs to its owner: the track, a voice, or an effect in
    /// a slot.
    Set(SettingId),
}

impl FxCommand {
    pub const VOL: Self = Self::Set(SettingId::VOL);
    pub const PAN: Self = Self::Set(SettingId::PAN);
    pub const CUT: Self = Self::Set(SettingId::CUT);
    pub const RES: Self = Self::Set(SettingId::RES);
    pub const SEND_A: Self = Self::Set(SettingId::SEND_A);
    pub const SEND_B: Self = Self::Set(SettingId::SEND_B);

    /// Every event, in the order a list shows them.
    pub const EVENTS: [Self; 8] = [
        Self::Del,
        Self::Kil,
        Self::Off,
        Self::Ret,
        Self::Cha,
        Self::Cnd,
        Self::Acu,
        Self::Arp,
    ];

    /// What it does, for a list to show.
    pub fn meaning(self) -> String {
        match self {
            Self::Del => "Delay this step by xx units".to_owned(),
            Self::Kil => "Cut the note after xx units".to_owned(),
            Self::Off => "Start the release after xx units".to_owned(),
            Self::Ret => "Retrigger. x is the fade, y the units between".to_owned(),
            Self::Cha => "Play the note with a chance of xx out of FF".to_owned(),
            Self::Cnd => "Play the row only when condition xx holds".to_owned(),
            Self::Acu => "Move the note on xx scale degrees each time it plays".to_owned(),
            Self::Arp => "Arpeggio: the note, x up and y up".to_owned(),
            Self::Set(id) => format!("{}: {}", id.group(), id.name()),
        }
    }

    /// The highest value this takes: a setting's own top, as a list stops at its last choice.
    pub fn highest(self) -> u8 {
        match self {
            Self::Set(id) => id.entry().kind.top(),
            // The last condition, `<31`.
            Self::Cnd => 0xCF,
            _ => 0xFF,
        }
    }

    /// Its label, as the grid shows it with nothing loaded to name it otherwise.
    pub fn name(self) -> String {
        match self {
            Self::Del => "DEL".to_owned(),
            Self::Kil => "KIL".to_owned(),
            Self::Off => "OFF".to_owned(),
            Self::Ret => "RET".to_owned(),
            Self::Cha => "CHA".to_owned(),
            Self::Cnd => "CND".to_owned(),
            Self::Acu => "ACU".to_owned(),
            Self::Arp => "ARP".to_owned(),
            Self::Set(id) => id.label().unwrap_or_else(|| id.key()),
        }
    }

    /// The name songs store it by: an event's own, or a setting's key.
    pub fn key(self) -> String {
        match self {
            Self::Set(id) => id.key(),
            event => event.name().to_lowercase(),
        }
    }

    /// The command a stored key names.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::EVENTS
            .into_iter()
            .find(|event| event.key() == key)
            .or_else(|| SettingId::from_key(key).map(Self::Set))
    }

    /// The setting this locks, if it locks one.
    pub fn setting(self) -> Option<SettingId> {
        match self {
            Self::Set(id) => Some(id),
            _ => None,
        }
    }

    /// Whether its value has a fine part: a continuous position's does, and one on the level scale,
    /// an event's or a list's doesn't (decision 175).
    pub fn has_fine(self) -> bool {
        matches!(self, Self::Set(id) if id.entry().kind.has_fine())
    }

    /// The highest value this takes with its fine part, which is `00` at the top.
    pub fn highest_value(self) -> u16 {
        u16::from(self.highest()) << 8
    }

    /// Whether its value can glide to the next: a continuous setting's can, the level scale's
    /// with no fine part too, and a list's jumps.
    pub fn glides(self) -> bool {
        matches!(
            self,
            Self::Set(id) if matches!(id.entry().kind, Kind::Continuous { .. })
        )
    }

    /// Whether the value stays in force after this step: a setting's does, an event's doesn't.
    pub fn holds(self) -> bool {
        matches!(self, Self::Set(_))
    }
}

/// How long a value lasts. Only values have one, so events and empty slots keep the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    /// Until the next note on the track, or the next value of its parameter.
    #[default]
    Lock,
    /// Replaces the base until playback next starts from the top.
    Hold,
    /// Slides from this row to the next value of its parameter in the slot.
    Glide,
}

impl Mark {
    /// The mark after this one, for `~`.
    pub fn next(self) -> Self {
        match self {
            Self::Lock => Self::Hold,
            Self::Hold => Self::Glide,
            Self::Glide => Self::Lock,
        }
    }

    /// The character shown beside a value.
    pub fn glyph(self) -> char {
        match self {
            Self::Lock => '.',
            Self::Hold => 'h',
            Self::Glide => 'g',
        }
    }
}

/// One FX command and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FxSlot {
    pub command: Option<FxCommand>,
    /// A coarse byte and a fine one, so `4080` is half a step above `40`. Only a continuous
    /// setting's has a fine part, and an event's or a list's is always `xx00`.
    pub value: u16,
    pub mark: Mark,
}

impl FxSlot {
    /// Its value's coarse byte, the two digits typed for it.
    pub fn coarse(self) -> u8 {
        (self.value >> 8) as u8
    }

    /// Its value's fine byte.
    pub fn fine(self) -> u8 {
        (self.value & 0xFF) as u8
    }

    /// An `SLI` locking a slice by the letter that names it, `SLI 'a`, rather than by its number,
    /// with a value's mark.
    pub fn slice_by_mark(letter: char, mark: Mark) -> Self {
        Self {
            command: Some(FxCommand::Set(SettingId::SLICE)),
            value: u16::from(letter as u8),
            mark,
        }
    }

    /// The letter an `SLI` locks its slice by, if it locks one by a letter rather than a number.
    pub fn slice_mark(self) -> Option<char> {
        slice_letter(self.value).filter(|_| self.command == Some(FxCommand::Set(SettingId::SLICE)))
    }

    /// Whether this is a value, which has a mark, rather than an event or nothing.
    pub fn is_value(self) -> bool {
        self.command.is_some_and(FxCommand::holds)
    }

    /// This slot, with the default mark unless it's a value, and no glide on a list.
    pub fn with_valid_mark(self) -> Self {
        let mark = match self.command {
            Some(command) if !command.holds() => Mark::default(),
            Some(command) if self.mark == Mark::Glide && !command.glides() => Mark::Lock,
            Some(_) => self.mark,
            None => Mark::default(),
        };
        Self { mark, ..self }
    }
}

/// The letter an `SLI` value locks its slice by, if it's one by a letter: it's in the fine byte,
/// which `SLI`, a list, otherwise never has.
pub fn slice_letter(value: u16) -> Option<char> {
    let letter = char::from((value & 0xFF) as u8);
    crate::bank::is_mark(letter).then_some(letter)
}

/// How a row's note behaves over its pulses, as on Metropolix.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GateKind {
    /// One note, on the first pulse.
    #[default]
    Single,
    /// A note on each pulse.
    Multi,
    /// One note, tied into the next row's note with no new attack.
    Hold,
    /// No note, and it ends one that's sounding.
    Rest,
}

impl GateKind {
    /// Every kind, in the order a list shows them.
    pub const ALL: [Self; 4] = [Self::Single, Self::Multi, Self::Hold, Self::Rest];

    /// Its three letters, as a gate cell shows them.
    pub fn name(self) -> &'static str {
        match self {
            Self::Single => "SGL",
            Self::Multi => "MLT",
            Self::Hold => "HLD",
            Self::Rest => "RST",
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Single => "One note, on the first pulse",
            Self::Multi => "A note on every pulse",
            Self::Hold => "One note, tied into the next row's",
            Self::Rest => "No note, and it ends one that's sounding",
        }
    }

    /// The kind with these three letters.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// A row's gate cell: what its note does, how many pulses, or steps, the row lasts, and how much
/// of its last pulse the note lasts. Every gate command's argument is its length, and `0` skips
/// the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    pub kind: GateKind,
    pub pulses: u8,
    /// How much of its last pulse the row's note lasts, on the `00` to `FF` scale, or `None` for
    /// the track's `Gate`. What moves `Gate` moves this up or down from where it is.
    pub length: Option<u8>,
}

/// The most pulses a row can last.
pub const MAX_PULSES: u8 = 8;

/// The furthest a row's nudge moves it either way, in 24ths of a step: just under half a step, as
/// Elektron's micro-timing goes, so a row's note can't pass the next row's.
pub const MAX_NUDGE: i8 = 11;

impl Default for Gate {
    fn default() -> Self {
        Self {
            kind: GateKind::Single,
            pulses: 1,
            length: None,
        }
    }
}

/// One position in a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub note: Note,
    /// `None` means full level.
    pub velocity: Option<u8>,
    /// How far the row's note part lands from its place, in 24ths of a step: late above 0, early
    /// below, and on the grid at 0. It moves the note and the row's own FX together.
    pub nudge: i8,
    /// `None` plays as one pulse of `Single`, as every row did before gates.
    pub gate: Option<Gate>,
    pub fx: Vec<FxSlot>,
}

impl Step {
    /// How many steps the row lasts.
    pub fn pulses(&self) -> usize {
        usize::from(self.gate.map_or(1, |gate| gate.pulses))
    }

    /// What its note does over its pulses.
    pub fn gate_kind(&self) -> GateKind {
        self.gate.map_or(GateKind::Single, |gate| gate.kind)
    }

    /// How much of its last pulse its note lasts, if the row says, rather than the track's `Gate`.
    pub fn length(&self) -> Option<u8> {
        self.gate.and_then(|gate| gate.length)
    }
}

impl Default for Step {
    fn default() -> Self {
        Self {
            note: Note::Empty,
            velocity: None,
            nudge: 0,
            gate: None,
            fx: vec![FxSlot::default(); FX_SLOTS],
        }
    }
}

impl Serialize for Step {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Step {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Step::from_str(&text).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for Note {
    /// Three characters: `---`, `OFF`, or a note's name and octave, from `c_1` to `G-8`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("---"),
            Self::Off => f.write_str("OFF"),
            Self::On(midi) => {
                let class = usize::from(midi % 12);
                match octave_of(*midi) {
                    octave if octave < 0 => {
                        write!(f, "{}{}", NOTE_NAMES_BELOW_ZERO[class], -octave)
                    }
                    octave => write!(f, "{}{octave}", NOTE_NAMES[class]),
                }
            }
        }
    }
}
