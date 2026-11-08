//! The readable text form of a step, used on disk and in tests.
//!
//! `NOTE VEL GATE FX FX FX`, such as `C-4 64 HLD3 cut:40 --- ret:14`. A row without a gate cell
//! leaves it out, as every row did before gates: `C-4 64 cut:40 --- ret:14`.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::step::{
    FxCommand, FxSlot, Gate, GateKind, MAX_NOTE, MAX_PULSES, MAX_VELOCITY, MIN_NOTE, Mark, Note,
    Step,
};

/// Why a step's text could not be read.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum StepTextError {
    #[error("a step needs a note and a velocity: {0:?}")]
    TooFewFields(String),
    #[error("not a note: {0:?}")]
    Note(String),
    #[error("not a velocity: {0:?}")]
    Velocity(String),
    #[error("not an FX slot: {0:?}")]
    Slot(String),
    #[error("unknown FX command: {0:?}")]
    Command(String),
    #[error("not a gate: {0:?}, which is a gate and 0 to 8 pulses, like HLD3")]
    Gate(String),
}

impl FromStr for Gate {
    type Err = StepTextError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || StepTextError::Gate(text.to_owned());
        let (name, pulses) = text.split_at_checked(3).ok_or_else(bad)?;
        let kind = GateKind::from_name(name).ok_or_else(bad)?;
        let pulses = match pulses.as_bytes() {
            [digit @ b'0'..=b'9'] => digit - b'0',
            _ => return Err(bad()),
        };
        (pulses <= MAX_PULSES)
            .then_some(Self { kind, pulses })
            .ok_or_else(bad)
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.kind.name(), self.pulses)
    }
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

    /// A command's key, a colon, its value in hex and a value's mark, like `cut:40`,
    /// `string.decay:80g` or `del:03`, or `---` for an empty slot. A continuous setting's value
    /// can have a fine part, as in `cut:4080h`, and is written without it when it's `00`. The
    /// colon is there because a key can end in letters that read as hex.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || StepTextError::Slot(text.to_owned());
        if text == "---" {
            return Ok(Self::default());
        }
        let (key, rest) = text.split_once(':').ok_or_else(bad)?;
        let (digits, mark) = match rest.len() {
            2 | 4 => (rest, ""),
            3 | 5 => rest.split_at(rest.len() - 1),
            _ => return Err(bad()),
        };
        let mark = match mark {
            "" => Mark::Lock,
            "h" => Mark::Hold,
            "g" => Mark::Glide,
            _ => return Err(bad()),
        };
        let command =
            FxCommand::from_key(key).ok_or_else(|| StepTextError::Command(key.to_owned()))?;
        let coarse = digits.get(..2).and_then(hex_byte).ok_or_else(bad)?;
        let fine = match digits.get(2..) {
            Some("") => 0,
            Some(fine) if command.has_fine() => hex_byte(fine).ok_or_else(bad)?,
            _ => return Err(bad()),
        };
        let value = u16::from(coarse) << 8 | u16::from(fine);
        let marked = match mark {
            Mark::Lock => true,
            Mark::Hold => command.holds(),
            Mark::Glide => command.glides(),
        };
        if !marked || value > command.highest_value() {
            return Err(bad());
        }
        Ok(Self {
            command: Some(command),
            value,
            mark,
        })
    }
}

impl fmt::Display for FxSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.command {
            Some(command) => write!(f, "{}:{:02X}", command.key(), self.coarse())?,
            None => f.write_str("---")?,
        }
        if self.fine() != 0 {
            write!(f, "{:02X}", self.fine())?;
        }
        match self.mark {
            Mark::Lock => Ok(()),
            mark => write!(f, "{}", mark.glyph()),
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
        let mut fields = fields.peekable();
        // A gate cell is a gate's name and one digit, where an FX slot is five or six characters.
        let is_gate = |field: &&str| {
            field.len() == 4 && field.get(..3).and_then(GateKind::from_name).is_some()
        };
        let gate = match fields.next_if(is_gate) {
            Some(field) => Some(field.parse()?),
            None => None,
        };
        let fx = fields.map(str::parse).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            note,
            velocity,
            gate,
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
        if let Some(gate) = self.gate {
            write!(f, " {gate}")?;
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
            "C-4 64 --- ret:14",
            "--- -- --- --- ---",
            "OFF 7F pan:80 cha:FF",
            "C-0 00 kil:02 off:03",
            "G-9 -- sna:20 snb:30",
            "C#3 --",
            "C-4 64 cut:40h sna:20g pan:80",
            "C-4 64 cut:4080h 1.distort.bits:03 res:FF",
            "C-4 64 HLD3 cut:40 --- ---",
            "--- -- RST4 --- --- ---",
            "E-4 -- MLT0",
        ] {
            let step: Step = text.parse().unwrap();
            assert_eq!(step.to_string(), text);
        }
    }

    #[test]
    fn a_slot_reads_its_mark_with_lock_the_plain_form() {
        let slot = |text: &str| text.parse::<FxSlot>().map(|slot| slot.mark);
        assert_eq!(slot("cut:40"), Ok(Mark::Lock));
        assert_eq!(slot("cut:40h"), Ok(Mark::Hold));
        assert_eq!(slot("cut:40g"), Ok(Mark::Glide));
    }

    #[test]
    fn only_values_can_have_a_mark() {
        for text in ["del:10h", "ret:14g", "---h", "CUT40x", "cut:40."] {
            assert!(text.parse::<FxSlot>().is_err(), "{text} should be refused");
        }
    }

    #[test]
    fn a_gate_is_a_kind_and_up_to_8_pulses() {
        let gate = |text: &str| text.parse::<Gate>();
        assert_eq!(
            gate("HLD3"),
            Ok(Gate {
                kind: GateKind::Hold,
                pulses: 3
            })
        );
        for text in ["HLD9", "XYZ1", "HLD", "HLD10", "hld3"] {
            assert!(gate(text).is_err(), "{text} should be refused");
        }
        let step: Step = "C-4 64 --- --- ---".parse().unwrap();
        assert_eq!(step.gate, None, "a row from before gates has none");
        assert_eq!(step.pulses(), 1);
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
            ("G#9 --", StepTextError::Note("G#9".into())),
            ("H-4 -- --", StepTextError::Note("H-4".into())),
            ("C-44 -- --", StepTextError::Note("C-44".into())),
            ("C-4 6 --", StepTextError::Velocity("6".into())),
            ("C-4 ab --", StepTextError::Velocity("ab".into())),
            ("C-4 80", StepTextError::Velocity("80".into())),
            ("C-4 -- cut:4", StepTextError::Slot("cut:4".into())),
            ("C-4 -- CUT40", StepTextError::Slot("CUT40".into())),
            ("C-4 -- ---01", StepTextError::Slot("---01".into())),
            ("C-4 -- xyz:00", StepTextError::Command("xyz".into())),
            ("C-4 -- del:00h", StepTextError::Slot("del:00h".into())),
            ("C-4 -- cut:FF80", StepTextError::Slot("cut:FF80".into())),
            ("C-4 -- vol:F900", StepTextError::Slot("vol:F900".into())),
            ("C-4 -- del:1080", StepTextError::Slot("del:1080".into())),
            (
                "C-4 -- 1.distort.bits:0B",
                StepTextError::Slot("1.distort.bits:0B".into()),
            ),
            (
                "C-4 -- 1.distort.bits:0380",
                StepTextError::Slot("1.distort.bits:0380".into()),
            ),
            ("C-4 -- cut:408", StepTextError::Slot("cut:408".into())),
            (
                "C-4 -- 1.distort.bits:03g",
                StepTextError::Slot("1.distort.bits:03g".into()),
            ),
            ("C-4", StepTextError::TooFewFields("C-4".into())),
        ] {
            assert_eq!(text.parse::<Step>(), Err(expected), "{text}");
        }
    }
}
