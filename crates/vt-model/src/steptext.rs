//! The readable text form of a step, used on disk and in tests.
//!
//! `NOTE VEL INST FX FX FX`, such as `C-4 64 01 CUT40 ---00 RET14`.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::step::{FxCommand, FxSlot, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note, Step};

/// Why a step's text could not be read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StepTextError {
    #[error("a step needs a note, a velocity and an instrument: {0:?}")]
    TooFewFields(String),
    #[error("not a note: {0:?}")]
    Note(String),
    #[error("not a velocity: {0:?}")]
    Velocity(String),
    #[error("not an instrument: {0:?}")]
    Instrument(String),
    #[error("not an FX slot: {0:?}")]
    Slot(String),
    #[error("unknown FX command: {0:?}")]
    Command(String),
}

/// Read two uppercase hex digits.
fn hex_byte(text: &str) -> Option<u8> {
    let upper = text.len() == 2
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
    upper.then(|| u8::from_str_radix(text, 16).ok()).flatten()
}

impl FromStr for Note {
    type Err = StepTextError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || StepTextError::Note(text.to_owned());
        match text {
            "---" => return Ok(Self::Empty),
            "OFF" => return Ok(Self::Off),
            _ => {}
        }
        let bytes = text.as_bytes();
        if bytes.len() != 3 {
            return Err(bad());
        }
        let class: i16 = match (bytes[0], bytes[1]) {
            (b'C', b'-') => 0,
            (b'C', b'#') => 1,
            (b'D', b'-') => 2,
            (b'D', b'#') => 3,
            (b'E', b'-') => 4,
            (b'F', b'-') => 5,
            (b'F', b'#') => 6,
            (b'G', b'-') => 7,
            (b'G', b'#') => 8,
            (b'A', b'-') => 9,
            (b'A', b'#') => 10,
            (b'B', b'-') => 11,
            _ => return Err(bad()),
        };
        if !bytes[2].is_ascii_digit() {
            return Err(bad());
        }
        let octave = i16::from(bytes[2] - b'0');
        let midi = (octave + 1) * 12 + class;
        if midi > i16::from(MAX_NOTE) || midi < i16::from(MIN_NOTE) {
            return Err(bad());
        }
        Ok(Self::On(midi as u8))
    }
}

impl FromStr for FxSlot {
    type Err = StepTextError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || StepTextError::Slot(text.to_owned());
        if text.len() != 5 || !text.is_ascii() {
            return Err(bad());
        }
        let (name, value) = text.split_at(3);
        let value = hex_byte(value).ok_or_else(bad)?;
        if name == "---" {
            return if value == 0 {
                Ok(Self::default())
            } else {
                Err(bad())
            };
        }
        let command =
            FxCommand::from_name(name).ok_or_else(|| StepTextError::Command(name.to_owned()))?;
        Ok(Self {
            command: Some(command),
            value,
        })
    }
}

impl fmt::Display for FxSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.command {
            Some(command) => write!(f, "{}{:02X}", command.name(), self.value),
            None => f.write_str("---00"),
        }
    }
}

impl FromStr for Step {
    type Err = StepTextError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut fields = text.split(' ');
        let few = || StepTextError::TooFewFields(text.to_owned());
        let note = fields.next().ok_or_else(few)?.parse()?;
        let velocity = fields.next().ok_or_else(few)?;
        let velocity = match velocity {
            "--" => None,
            other => Some(
                hex_byte(other)
                    .filter(|velocity| *velocity <= MAX_VELOCITY)
                    .ok_or_else(|| StepTextError::Velocity(other.to_owned()))?,
            ),
        };
        let instrument = fields.next().ok_or_else(few)?;
        let instrument = match instrument {
            "--" => None,
            other => {
                let valid = other.len() >= 2
                    && other
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
                let parsed = valid.then(|| u32::from_str_radix(other, 16).ok()).flatten();
                Some(parsed.ok_or_else(|| StepTextError::Instrument(other.to_owned()))?)
            }
        };
        let fx = fields.map(str::parse).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            note,
            velocity,
            instrument,
            fx,
        })
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.note)?;
        match self.velocity {
            Some(velocity) => write!(f, " {velocity:02X}")?,
            None => f.write_str(" --")?,
        }
        match self.instrument {
            Some(instrument) => write!(f, " {instrument:02X}")?,
            None => f.write_str(" --")?,
        }
        for slot in &self.fx {
            write!(f, " {slot}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_form_round_trips() {
        for text in [
            "C-4 64 01 CUT40 ---00 RET14",
            "--- -- -- ---00 ---00 ---00",
            "OFF 7F 1A2B VOL00 PAN80 CHAFF",
            "C-0 00 00 DEL01 KIL02 OFF03",
            "G-9 -- -- RES10 DLY20 REV30",
            "C#3 -- 00",
        ] {
            let step: Step = text.parse().unwrap();
            assert_eq!(step.to_string(), text);
        }
    }

    #[test]
    fn notes_map_to_midi() {
        assert_eq!("C-4".parse(), Ok(Note::On(60)));
        assert_eq!("C-0".parse(), Ok(Note::On(12)));
        assert_eq!("G-9".parse(), Ok(Note::On(127)));
        assert_eq!(Note::On(61).to_string(), "C#4");
    }

    #[test]
    fn invalid_fields_give_errors() {
        for (text, expected) in [
            ("G#9 -- --", StepTextError::Note("G#9".into())),
            ("H-4 -- --", StepTextError::Note("H-4".into())),
            ("C-44 -- --", StepTextError::Note("C-44".into())),
            ("C-4 6 --", StepTextError::Velocity("6".into())),
            ("C-4 ab --", StepTextError::Velocity("ab".into())),
            ("C-4 80 --", StepTextError::Velocity("80".into())),
            ("C-4 -- 1", StepTextError::Instrument("1".into())),
            ("C-4 -- -- CUT4", StepTextError::Slot("CUT4".into())),
            ("C-4 -- -- ---01", StepTextError::Slot("---01".into())),
            ("C-4 -- -- XXX00", StepTextError::Command("XXX".into())),
            ("C-4", StepTextError::TooFewFields("C-4".into())),
        ] {
            assert_eq!(text.parse::<Step>(), Err(expected), "{text}");
        }
    }
}
