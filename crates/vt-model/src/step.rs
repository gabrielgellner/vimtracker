//! One position in a phrase, and the fields it holds.

use crate::insert::{INSERTS, MAX_PARAMS};
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::constants::{FX_SLOTS, MAX_LEVEL};

/// Lowest note the format can write, `C-0`.
pub const MIN_NOTE: u8 = 12;
/// Highest note the format can write, `G-9`.
pub const MAX_NOTE: u8 = 127;
/// The highest velocity, full level, which is also MIDI's.
pub const MAX_VELOCITY: u8 = 0x7F;

/// A note field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Note {
    #[default]
    Empty,
    Off,
    /// A MIDI note from [`MIN_NOTE`] to [`MAX_NOTE`].
    On(u8),
}

/// An FX command name. Mixer and filter values hold until changed, and the rest apply once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxCommand {
    Vol,
    Pan,
    Cut,
    Res,
    SendA,
    SendB,
    Har,
    Tim,
    Mor,
    Del,
    Kil,
    Off,
    Ret,
    Cha,
    /// Sets a setting of what's in a slot, by its place in the effect's list. It reads as the
    /// setting's name on screen, and a file holds the slot and place, like `1.2`.
    Slot {
        slot: u8,
        param: u8,
    },
}

/// How a file names the commands for slots' settings.
const SLOT_NAMES: [[&str; MAX_PARAMS]; INSERTS] = [
    [
        "1.0", "1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "1.7", "1.8",
    ],
    [
        "2.0", "2.1", "2.2", "2.3", "2.4", "2.5", "2.6", "2.7", "2.8",
    ],
    [
        "3.0", "3.1", "3.2", "3.3", "3.4", "3.5", "3.6", "3.7", "3.8",
    ],
];

impl FxCommand {
    /// Every command, in the order a list shows them.
    pub const ALL: [Self; 14 + INSERTS * MAX_PARAMS] = {
        const FIRST: [FxCommand; 14] = [
            FxCommand::Vol,
            FxCommand::Pan,
            FxCommand::Cut,
            FxCommand::Res,
            FxCommand::SendA,
            FxCommand::SendB,
            FxCommand::Har,
            FxCommand::Tim,
            FxCommand::Mor,
            FxCommand::Del,
            FxCommand::Kil,
            FxCommand::Off,
            FxCommand::Ret,
            FxCommand::Cha,
        ];
        let mut all = [Self::Vol; 14 + INSERTS * MAX_PARAMS];
        let mut at = 0;
        while at < FIRST.len() {
            all[at] = FIRST[at];
            at += 1;
        }
        let mut slot = 0;
        while slot < INSERTS {
            let mut param = 0;
            while param < MAX_PARAMS {
                all[at] = Self::slot(slot as u8, param as u8);
                at += 1;
                param += 1;
            }
            slot += 1;
        }
        all
    };

    const fn slot(slot: u8, param: u8) -> Self {
        Self::Slot { slot, param }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Vol => "Track gain, holds",
            Self::Pan => "Pan, holds. 00 left, 80 centre, FF right",
            Self::Cut => "Filter cutoff, holds",
            Self::Res => "Filter resonance, holds",
            Self::SendA => "Send to return A, holds",
            Self::SendB => "Send to return B, holds",
            Self::Har => "A synth's first setting, holds",
            Self::Tim => "A synth's second setting, holds",
            Self::Mor => "A synth's third setting, holds",
            Self::Del => "Delay this step by xx units",
            Self::Kil => "Cut the note after xx units",
            Self::Off => "Start the release after xx units",
            Self::Ret => "Retrigger. x is the fade, y the units between",
            Self::Cha => "Play the note with a chance of xx out of FF",
            Self::Slot { .. } => "A setting of what's in a slot on the FX page",
        }
    }

    /// The highest value this takes. Those on the level scale stop at its top.
    pub fn highest(self) -> u8 {
        match self {
            Self::Vol | Self::SendA | Self::SendB => MAX_LEVEL,
            _ => 0xFF,
        }
    }

    /// The three-letter name used on screen and in files.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vol => "VOL",
            Self::Pan => "PAN",
            Self::Cut => "CUT",
            Self::Res => "RES",
            Self::SendA => "SNA",
            Self::SendB => "SNB",
            Self::Har => "HAR",
            Self::Tim => "TIM",
            Self::Mor => "MOR",
            Self::Del => "DEL",
            Self::Kil => "KIL",
            Self::Off => "OFF",
            Self::Ret => "RET",
            Self::Cha => "CHA",
            Self::Slot { slot, param } => SLOT_NAMES[slot as usize][param as usize],
        }
    }

    /// Look a command up by its three-letter name.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "VOL" => Self::Vol,
            "PAN" => Self::Pan,
            "CUT" => Self::Cut,
            "RES" => Self::Res,
            "SNA" => Self::SendA,
            "SNB" => Self::SendB,
            "HAR" => Self::Har,
            "TIM" => Self::Tim,
            "MOR" => Self::Mor,
            "DEL" => Self::Del,
            "KIL" => Self::Kil,
            "OFF" => Self::Off,
            "RET" => Self::Ret,
            "CHA" => Self::Cha,
            _ => return Self::ALL.into_iter().find(|command| command.name() == name),
        })
    }

    /// Whether the value stays in force after this step.
    pub fn holds(self) -> bool {
        matches!(
            self,
            Self::Vol
                | Self::Pan
                | Self::Cut
                | Self::Res
                | Self::SendA
                | Self::SendB
                | Self::Har
                | Self::Tim
                | Self::Mor
                | Self::Slot { .. }
        )
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
    pub value: u8,
    pub mark: Mark,
}

impl FxSlot {
    /// Whether this is a value, which has a mark, rather than an event or nothing.
    pub fn is_value(self) -> bool {
        self.command.is_some_and(FxCommand::holds)
    }

    /// This slot, with the default mark unless it's a value.
    pub fn with_valid_mark(self) -> Self {
        Self {
            mark: if self.is_value() {
                self.mark
            } else {
                Mark::default()
            },
            ..self
        }
    }
}

/// One position in a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub note: Note,
    /// `None` means full level.
    pub velocity: Option<u8>,
    pub fx: Vec<FxSlot>,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            note: Note::Empty,
            velocity: None,
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [&str; 12] = [
            "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
        ];
        match self {
            Self::Empty => f.write_str("---"),
            Self::Off => f.write_str("OFF"),
            Self::On(midi) => {
                let name = NAMES[usize::from(midi % 12)];
                let octave = i16::from(*midi) / 12 - 1;
                write!(f, "{name}{octave}")
            }
        }
    }
}
