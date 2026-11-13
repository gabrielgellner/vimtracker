//! The readable text form of a step, used on disk and in tests.
//!
//! `NOTE VEL GATE FX FX FX`, such as `C-3 64 HLD3 cut:40 --- ret:14`. A row without a gate cell
//! leaves it out, as every row did before gates: `C-3 64 cut:40 --- ret:14`. A gate with a length
//! of its own has it after the pulses, as the grid shows it: `C-3 64 SGL1 80 cut:40`. A row with
//! a nudge has it after the velocity, a sign and one or two hex digits in 24ths of a sixteenth:
//! `C-3 64 -3 SGL1`, `C-3 64 -2F SGL1`.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;

use crate::settings::SettingId;
use crate::step::{
    FxCommand, FxSlot, Gate, GateKind, MAX_NOTE, MAX_PULSES, MOST_NUDGE, MAX_VELOCITY, MIN_NOTE,
    Mark, NOTE_NAMES, NOTE_NAMES_BELOW_ZERO, Note, Step, octave_start,
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
    #[error("not a nudge: {0:?}, which is + or - and 1 to 2F 24ths of a sixteenth, like -3")]
    Nudge(String),
    #[error("not a gate: {0:?}, which is a gate, 0 to 8 pulses and perhaps a length, like SGL1 80")]
    Gate(String),
}

impl FromStr for Gate {
    type Err = StepTextError;

    /// A gate's name and its pulses, like `HLD3`, then a space and a length in hex if it has one of
    /// its own, like `SGL1 80`.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || StepTextError::Gate(text.to_owned());
        let (cell, length) = match text.split_once(' ') {
            Some((cell, length)) => (cell, Some(hex_byte(length).ok_or_else(bad)?)),
            None => (text, None),
        };
        let (name, pulses) = cell.split_at_checked(3).ok_or_else(bad)?;
        let kind = GateKind::from_name(name).ok_or_else(bad)?;
        let pulses = match pulses.as_bytes() {
            [digit @ b'0'..=b'9'] => digit - b'0',
            _ => return Err(bad()),
        };
        (pulses <= MAX_PULSES)
            .then_some(Self {
                kind,
                pulses,
                length,
            })
            .ok_or_else(bad)
    }
}

impl fmt::Display for Gate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.kind.name(), self.pulses)?;
        match self.length {
            Some(length) => write!(f, " {length:02X}"),
            None => Ok(()),
        }
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

/// Read a nudge: `+` or `-`, then one or two uppercase hex digits from 1 to the most any track's
/// nudge goes. Whether it goes that far on its own track is the track's to say.
fn nudge(text: &str) -> Result<i8, StepTextError> {
    let bad = || StepTextError::Nudge(text.to_owned());
    let (sign, digits) = match text.as_bytes() {
        [b'+', digits @ ..] => (1, digits),
        [b'-', digits @ ..] => (-1, digits),
        _ => return Err(bad()),
    };
    let upper = |digit: &u8| digit.is_ascii_digit() || (b'A'..=b'F').contains(digit);
    if !(1..=2).contains(&digits.len()) || !digits.iter().all(upper) {
        return Err(bad());
    }
    let amount = std::str::from_utf8(digits)
        .ok()
        .and_then(|digits| i8::from_str_radix(digits, 16).ok())
        .ok_or_else(bad)?;
    (1..=MOST_NUDGE)
        .contains(&amount)
        .then_some(sign * amount)
        .ok_or_else(bad)
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
        // A name from the octaves from 0 up, or from those below it, which count down.
        let name = &bytes[..2];
        let is = |known: &&str| known.as_bytes() == name;
        let (class, sign) = match NOTE_NAMES.iter().position(is) {
            Some(class) => (class, 1),
            None => (
                NOTE_NAMES_BELOW_ZERO.iter().position(is).ok_or_else(bad)?,
                -1,
            ),
        };
        if !bytes[2].is_ascii_digit() || (sign < 0 && bytes[2] == b'0') {
            return Err(bad());
        }
        let octave = sign * (bytes[2] - b'0') as i8;
        let midi = octave_start(octave) + class as i16;
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
        // An `SLI` can lock a slice by the letter naming it, as `slice:'a`.
        if let Some(letter) = digits.strip_prefix('\'') {
            let mut letters = letter.chars();
            return match (letters.next(), letters.next(), mark) {
                (Some(letter), None, Mark::Lock | Mark::Hold)
                    if crate::bank::is_mark(letter)
                        && command == FxCommand::Set(SettingId::SLICE) =>
                {
                    Ok(FxSlot::slice_by_mark(letter, mark))
                }
                _ => Err(bad()),
            };
        }
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
        match (self.command, self.slice_mark()) {
            (Some(command), Some(letter)) => write!(f, "{}:'{letter}", command.key())?,
            (Some(command), None) => write!(f, "{}:{:02X}", command.key(), self.coarse())?,
            (None, _) => f.write_str("---")?,
        }
        if self.fine() != 0 && self.slice_mark().is_none() {
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
        // A nudge is a sign and one or two digits, which neither a gate nor an FX slot is.
        // An empty slot's `---` isn't one.
        let is_nudge = |field: &&str| {
            let signed = field.starts_with('+') || field.starts_with('-');
            let digits = field.get(1..).unwrap_or_default();
            signed
                && (field.len() == 2
                    || (field.len() == 3 && digits.bytes().all(|b| b.is_ascii_alphanumeric())))
        };
        let nudge = match fields.next_if(is_nudge) {
            Some(field) => self::nudge(field)?,
            None => 0,
        };
        // A gate cell is a gate's name and one digit, where an FX slot is five or six characters.
        let is_gate = |field: &&str| {
            field.len() == 4 && field.get(..3).and_then(GateKind::from_name).is_some()
        };
        let gate = match fields.next_if(is_gate) {
            Some(field) => {
                let gate: Gate = field.parse()?;
                // Its length is two hex digits, which no FX slot is.
                let length = fields.next_if(|field| hex_byte(field).is_some());
                Some(Gate {
                    length: length.and_then(hex_byte),
                    ..gate
                })
            }
            None => None,
        };
        let fx = fields.map(str::parse).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            note,
            velocity,
            nudge,
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
        if self.nudge != 0 {
            let sign = if self.nudge > 0 { '+' } else { '-' };
            write!(f, " {sign}{:X}", self.nudge.unsigned_abs())?;
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
    use crate::step::{HIGHEST_OCTAVE, LOWEST_OCTAVE, MIDDLE_C, octave_of};

    #[test]
    fn every_field_form_round_trips() {
        for text in [
            "C-3 64 --- ret:14",
            "--- -- --- --- ---",
            "OFF 7F pan:80 cha:FF",
            "c_1 00 kil:02 off:03",
            "G-8 -- sna:20 snb:30",
            "C#2 --",
            "C-3 64 cut:40h sna:20g pan:80",
            "C-3 64 cut:4080h 1.distort.bits:03 res:FF",
            "C-3 64 HLD3 cut:40 --- ---",
            "--- -- RST4 --- --- ---",
            "E-3 -- MLT0",
            "C-3 64 SGL1 80 cut:40 --- ---",
            "--- -- HLD3 FF",
            "E-3 -- MLT2 00 ---",
            "C-3 64 -3 SGL1 80 cut:40",
            "C-3 -- +B",
            "OFF -- -1 --- ---",
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
    fn a_value_on_the_level_scale_has_no_fine_part_and_still_glides() {
        for text in ["sna:2080", "vol:E001", "level:8040h"] {
            assert!(text.parse::<FxSlot>().is_err(), "{text} should be refused");
        }
        let slot: FxSlot = "level:80g".parse().unwrap();
        assert_eq!(slot.command, Some(FxCommand::Set(SettingId::LEVEL)));
        assert_eq!((slot.value, slot.mark), (0x8000, Mark::Glide));
        assert_eq!(slot.to_string(), "level:80g");
        assert!(
            "pan:8040".parse::<FxSlot>().is_ok(),
            "a position keeps its fine part"
        );
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
                pulses: 3,
                length: None,
            })
        );
        assert_eq!(gate("SGL1 80").map(|gate| gate.length), Ok(Some(0x80)));
        for text in [
            "HLD9", "XYZ1", "HLD", "HLD10", "hld3", "SGL1 8", "SGL1 ff", "SGL1 --",
        ] {
            assert!(gate(text).is_err(), "{text} should be refused");
        }
        let step: Step = "C-3 64 --- --- ---".parse().unwrap();
        assert_eq!(step.gate, None, "a row from before gates has none");
        assert_eq!(step.pulses(), 1);
    }

    #[test]
    fn a_nudge_is_a_sign_and_up_to_2f_24ths() {
        let nudge = |text: &str| text.parse::<Step>().map(|step| step.nudge);
        assert_eq!(nudge("C-3 64 -3 SGL1"), Ok(-3));
        assert_eq!(nudge("C-3 -- +B cut:40"), Ok(11));
        assert_eq!(nudge("C-3 -- -2F SGL1"), Ok(-47));
        assert_eq!(nudge("C-3 -- +0B"), Ok(11), "with a leading 0, as a slow track shows it");
        let far: Step = "C-3 -- -2F".parse().unwrap();
        assert_eq!(far.to_string(), "C-3 -- -2F");
        assert_eq!(
            nudge("C-3 64 SGL1"),
            Ok(0),
            "a row without one is on the grid"
        );
        for text in ["C-3 -- +30", "C-3 -- -0", "C-3 -- -00", "C-3 -- +b", "C-3 -- ++"] {
            let field = text.split(' ').nth(2).unwrap();
            assert_eq!(
                text.parse::<Step>(),
                Err(StepTextError::Nudge(field.into())),
                "{text}"
            );
        }
    }

    #[test]
    fn notes_map_to_midi() {
        assert_eq!("C-4".parse(), Ok(Note::On(72)));
        assert_eq!("C-0".parse(), Ok(Note::On(24)));
        assert_eq!("c_1".parse(), Ok(Note::On(12)));
        assert_eq!("b_1".parse(), Ok(Note::On(23)));
        assert_eq!("G-8".parse(), Ok(Note::On(127)));
        assert_eq!(Note::On(61).to_string(), "C#3");
        assert_eq!(Note::On(13).to_string(), "c+1");
    }

    #[test]
    fn middle_c_is_c3_as_in_ableton_live() {
        assert_eq!("C-3".parse(), Ok(Note::On(MIDDLE_C)));
        assert_eq!(MIDDLE_C, 60);
        assert_eq!(Note::On(60).to_string(), "C-3");
        let hertz = 440.0 * ((f64::from(MIDDLE_C) - 69.0) / 12.0).exp2();
        assert!((hertz - 261.63).abs() < 0.005, "{hertz}");
        assert_eq!("A-3".parse(), Ok(Note::On(69)), "A440");
    }

    #[test]
    fn every_note_in_range_round_trips_in_three_characters() {
        for midi in MIN_NOTE..=MAX_NOTE {
            let name = Note::On(midi).to_string();
            assert_eq!(name.len(), 3, "{midi} is {name}");
            assert_eq!(name.parse(), Ok(Note::On(midi)), "{name}");
            assert_eq!(octave_start(octave_of(midi)), i16::from(midi / 12 * 12));
        }
        assert_eq!(Note::On(MIN_NOTE).to_string(), "c_1");
        assert_eq!(Note::On(23).to_string(), "b_1");
        assert_eq!(Note::On(24).to_string(), "C-0");
        assert_eq!(Note::On(MAX_NOTE).to_string(), "G-8");
        assert_eq!(octave_of(MIN_NOTE), LOWEST_OCTAVE);
        assert_eq!(octave_of(MAX_NOTE), HIGHEST_OCTAVE);
        // Out of range either way, and a sharp written with the other octaves' mark.
        for name in ["b_2", "G#8", "c-1", "c#1", "C_1", "C+1", "c_0"] {
            assert_eq!(name.parse::<Note>(), Err(StepTextError::Note(name.into())));
        }
    }

    #[test]
    fn invalid_fields_give_errors() {
        for (text, expected) in [
            ("G#8 --", StepTextError::Note("G#8".into())),
            ("H-4 -- --", StepTextError::Note("H-4".into())),
            ("C-44 -- --", StepTextError::Note("C-44".into())),
            ("C-3 6 --", StepTextError::Velocity("6".into())),
            ("C-3 ab --", StepTextError::Velocity("ab".into())),
            ("C-3 80", StepTextError::Velocity("80".into())),
            ("C-3 -- cut:4", StepTextError::Slot("cut:4".into())),
            ("C-3 -- CUT40", StepTextError::Slot("CUT40".into())),
            ("C-3 -- ---01", StepTextError::Slot("---01".into())),
            ("C-3 -- xyz:00", StepTextError::Command("xyz".into())),
            ("C-3 -- del:00h", StepTextError::Slot("del:00h".into())),
            ("C-3 -- cut:FF80", StepTextError::Slot("cut:FF80".into())),
            ("C-3 -- vol:F900", StepTextError::Slot("vol:F900".into())),
            ("C-3 -- del:1080", StepTextError::Slot("del:1080".into())),
            (
                "C-3 -- 1.distort.bits:0B",
                StepTextError::Slot("1.distort.bits:0B".into()),
            ),
            (
                "C-3 -- 1.distort.bits:0380",
                StepTextError::Slot("1.distort.bits:0380".into()),
            ),
            ("C-3 -- cut:408", StepTextError::Slot("cut:408".into())),
            (
                "C-3 -- 1.distort.bits:03g",
                StepTextError::Slot("1.distort.bits:03g".into()),
            ),
            ("C-3", StepTextError::TooFewFields("C-3".into())),
            ("C-3 -- SGL1 8", StepTextError::Slot("8".into())),
            ("C-3 -- 80", StepTextError::Slot("80".into())),
        ] {
            assert_eq!(text.parse::<Step>(), Err(expected), "{text}");
        }
    }
}
