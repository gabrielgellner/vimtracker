//! One position in a phrase, and the fields it holds.

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
    Dly,
    Rev,
    Har,
    Tim,
    Mor,
    Del,
    Kil,
    Off,
    Ret,
    Cha,
}

impl FxCommand {
    /// Every command, in the order a list shows them.
    pub const ALL: [Self; 14] = [
        Self::Vol,
        Self::Pan,
        Self::Cut,
        Self::Res,
        Self::Dly,
        Self::Rev,
        Self::Har,
        Self::Tim,
        Self::Mor,
        Self::Del,
        Self::Kil,
        Self::Off,
        Self::Ret,
        Self::Cha,
    ];

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Vol => "Track gain, holds",
            Self::Pan => "Pan, holds. 00 left, 80 centre, FF right",
            Self::Cut => "Filter cutoff, holds",
            Self::Res => "Filter resonance, holds",
            Self::Dly => "Delay send, holds",
            Self::Rev => "Reverb send, holds",
            Self::Har => "Synth detune, holds. 80 in tune",
            Self::Tim => "Synth square, holds",
            Self::Mor => "Synth saw, holds. 80 a plain saw",
            Self::Del => "Delay this step by xx units",
            Self::Kil => "Cut the note after xx units",
            Self::Off => "Start the release after xx units",
            Self::Ret => "Retrigger. x is the fade, y the units between",
            Self::Cha => "Play the note with a chance of xx out of FF",
        }
    }

    /// The highest value this takes. Those on the level scale stop at its top.
    pub fn highest(self) -> u8 {
        match self {
            Self::Vol | Self::Dly | Self::Rev => MAX_LEVEL,
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
            Self::Dly => "DLY",
            Self::Rev => "REV",
            Self::Har => "HAR",
            Self::Tim => "TIM",
            Self::Mor => "MOR",
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
            "HAR" => Self::Har,
            "TIM" => Self::Tim,
            "MOR" => Self::Mor,
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
            Self::Vol
                | Self::Pan
                | Self::Cut
                | Self::Res
                | Self::Dly
                | Self::Rev
                | Self::Har
                | Self::Tim
                | Self::Mor
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
