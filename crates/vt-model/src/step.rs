//! One position in a phrase, and the fields it holds.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::constants::FX_SLOTS;

/// Lowest note the format can write, `C-0`.
pub const MIN_NOTE: u8 = 12;
/// Highest note the format can write, `G-9`.
pub const MAX_NOTE: u8 = 127;

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
    Dly,
    Rev,
    Del,
    Kil,
    Off,
    Ret,
    Cha,
}

impl FxCommand {
    /// The three-letter name used on screen and in files.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vol => "VOL",
            Self::Pan => "PAN",
            Self::Cut => "CUT",
            Self::Res => "RES",
            Self::Dly => "DLY",
            Self::Rev => "REV",
            Self::Del => "DEL",
            Self::Kil => "KIL",
            Self::Off => "OFF",
            Self::Ret => "RET",
            Self::Cha => "CHA",
        }
    }

    /// Look a command up by its three-letter name.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "VOL" => Self::Vol,
            "PAN" => Self::Pan,
            "CUT" => Self::Cut,
            "RES" => Self::Res,
            "DLY" => Self::Dly,
            "REV" => Self::Rev,
            "DEL" => Self::Del,
            "KIL" => Self::Kil,
            "OFF" => Self::Off,
            "RET" => Self::Ret,
            "CHA" => Self::Cha,
            _ => return None,
        })
    }

    /// Whether the value stays in force after this step.
    pub fn holds(self) -> bool {
        matches!(
            self,
            Self::Vol | Self::Pan | Self::Cut | Self::Res | Self::Dly | Self::Rev
        )
    }
}

/// One FX command and its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FxSlot {
    pub command: Option<FxCommand>,
    pub value: u8,
}

/// One position in a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub note: Note,
    /// `None` means full level.
    pub volume: Option<u8>,
    pub instrument: Option<u32>,
    pub fx: Vec<FxSlot>,
}

impl Default for Step {
    fn default() -> Self {
        Self {
            note: Note::Empty,
            volume: None,
            instrument: None,
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
