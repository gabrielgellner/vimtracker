//! The pieces the Roto's messages are built from: names in 13 bytes, numbers in two 7-bit bytes,
//! hashes, colours, and a reader that says what was wrong with a message it can't read.

use thiserror::Error;
use xxhash_rust::xxh3::xxh3_64;

/// How many bytes a name takes in a message, its end marker included.
pub const LABEL_BYTES: usize = 13;

/// How many characters of a name the Roto shows.
pub const LABEL_CHARS: usize = LABEL_BYTES - 1;

/// Why a message couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    #[error("the message ends early")]
    Short,
    #[error("the message has {0} bytes more than its command takes")]
    Trailing(usize),
    #[error("not a Roto-Control message")]
    NotRoto,
    #[error("unknown command {group:02X} {command:02X}")]
    Unknown { group: u8, command: u8 },
    #[error("{what} can't be {value:02X}")]
    Value { what: &'static str, value: u8 },
}

/// A name as the Roto shows it: up to 12 printable ASCII characters, as its font has no others
/// (firmware 3.3.0 drew `°` as `?`). A character it hasn't is written as its nearest in ASCII,
/// such as `°` as `deg`, or else `?`, and anything past 12 is cut, so a label always reads back
/// as it was written.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash, PartialOrd, Ord)]
pub struct Label(String);

/// A character the Roto's font hasn't, in ASCII, if it has a near one.
fn in_ascii(character: char) -> Option<&'static str> {
    Some(match character {
        '°' => "deg",
        '×' => "x",
        '–' | '—' | '−' => "-",
        '·' | '•' => ".",
        '…' => "...",
        '‘' | '’' => "'",
        '“' | '”' => "\"",
        'µ' | 'μ' => "u",
        '♯' => "#",
        '♭' => "b",
        '½' => "1/2",
        '²' => "2",
        'á' | 'à' | 'â' | 'ä' => "a",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'í' | 'ì' | 'î' | 'ï' => "i",
        'ó' | 'ò' | 'ô' | 'ö' => "o",
        'ú' | 'ù' | 'û' | 'ü' => "u",
        _ => return None,
    })
}

impl Label {
    /// A label from any text, in ASCII the Roto can show and cut to 12 characters.
    pub fn new(text: &str) -> Self {
        let mut shown = String::new();
        for character in text.chars() {
            if character == ' ' || character.is_ascii_graphic() {
                shown.push(character);
            } else {
                shown.push_str(in_ascii(character).unwrap_or("?"));
            }
        }
        Self(shown.chars().take(LABEL_CHARS).collect())
    }

    /// Whether text shows on the Roto as it means, with nothing in it the Roto's font can't
    /// draw or write near enough.
    pub fn shows_whole(text: &str) -> bool {
        text.chars().all(|character| {
            character == ' ' || character.is_ascii_graphic() || in_ascii(character).is_some()
        })
    }

    /// No name, as a slot with nothing in it has.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The label in its 13 bytes, padded with zeros.
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        let mut bytes = [0u8; LABEL_BYTES];
        for (at, byte) in self.0.bytes().take(LABEL_CHARS).enumerate() {
            bytes[at] = byte;
        }
        out.extend_from_slice(&bytes);
    }

    /// A label from its 13 bytes: up to the first zero, with anything the Roto wouldn't send as
    /// `?`.
    pub(crate) fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        let bytes = reader.take(LABEL_BYTES)?;
        let end = bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(LABEL_CHARS);
        let text: String = bytes[..end.min(LABEL_CHARS)]
            .iter()
            .map(|&byte| {
                if byte == b' ' || byte.is_ascii_graphic() {
                    char::from(byte)
                } else {
                    '?'
                }
            })
            .collect();
        Ok(Self(text))
    }
}

impl std::fmt::Display for Label {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Which a PLUGIN setup is, as eight 7-bit bytes. The Roto finds a device's stored setup by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PluginHash(pub [u8; 8]);

impl PluginHash {
    /// A stable hash of a key, such as `vimtracker/fx/delay`, the same on every machine and in
    /// every build, so a setup written once is found again.
    pub fn of(key: &str) -> Self {
        let wide = xxh3_64(key.as_bytes()).to_be_bytes();
        Self(wide.map(|byte| byte & 0x7F))
    }

    /// The hash the serial API sends for no plugin, all `FF`.
    pub const NONE: Self = Self([0xFF; 8]);
}

/// Which setting a control is mapped to, as six 7-bit bytes, beside its index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParamHash(pub [u8; 6]);

impl ParamHash {
    /// A stable hash of a key, such as a setting's key in the registry.
    pub fn of(key: &str) -> Self {
        let wide = xxh3_64(key.as_bytes()).to_be_bytes();
        Self(std::array::from_fn(|at| wide[at] & 0x7F))
    }
}

/// A colour in RGB, which the Roto takes for a track and the current track. Elsewhere it takes an
/// index into its own table, [`crate::colour::SCHEMES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub(crate) fn write(self, out: &mut Vec<u8>) {
        for part in [self.red, self.green, self.blue] {
            push_pair(out, u16::from(part));
        }
    }

    pub(crate) fn read(reader: &mut Reader) -> Result<Self, DecodeError> {
        let mut part = || -> Result<u8, DecodeError> {
            let value = reader.pair()?;
            u8::try_from(value).map_err(|_| DecodeError::Value {
                what: "a colour part",
                value: (value >> 7) as u8,
            })
        };
        Ok(Self {
            red: part()?,
            green: part()?,
            blue: part()?,
        })
    }
}

/// The most a number in two 7-bit bytes can be.
pub const PAIR_MAX: u16 = 0x3FFF;

/// A number up to 16383 as two 7-bit bytes, the high one first, as SysEx sends track indices and
/// positions. Past 16383 it's the top.
pub(crate) fn push_pair(out: &mut Vec<u8>, value: u16) {
    let value = value.min(PAIR_MAX);
    out.push((value >> 7) as u8);
    out.push((value & 0x7F) as u8);
}

/// A number as two 8-bit bytes, the high one first, as the serial API sends lengths.
pub(crate) fn push_wide(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Reads a message's bytes in order, and fails rather than panics when they run out.
#[derive(Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub fn byte(&mut self) -> Result<u8, DecodeError> {
        let byte = *self.bytes.get(self.at).ok_or(DecodeError::Short)?;
        self.at += 1;
        Ok(byte)
    }

    pub fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.at.checked_add(count).ok_or(DecodeError::Short)?;
        let bytes = self.bytes.get(self.at..end).ok_or(DecodeError::Short)?;
        self.at = end;
        Ok(bytes)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let bytes = self.take(N)?;
        Ok(std::array::from_fn(|at| bytes[at]))
    }

    /// A number in two 7-bit bytes, the high one first.
    pub fn pair(&mut self) -> Result<u16, DecodeError> {
        let high = self.byte()?;
        let low = self.byte()?;
        Ok((u16::from(high & 0x7F) << 7) | u16::from(low & 0x7F))
    }

    /// A number in two 8-bit bytes, the high one first.
    pub fn wide(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    /// A yes or no, sent as `00` or `01`.
    pub fn flag(&mut self, what: &'static str) -> Result<bool, DecodeError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(DecodeError::Value { what, value }),
        }
    }

    pub fn label(&mut self) -> Result<Label, DecodeError> {
        Label::read(self)
    }

    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }

    /// Fails if any bytes are left, as a command that's longer than it should be was misread.
    pub fn finish(self) -> Result<(), DecodeError> {
        match self.remaining() {
            0 => Ok(()),
            more => Err(DecodeError::Trailing(more)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_label_is_cut_to_twelve_printable_characters() {
        assert_eq!(Label::new("Overdrive").as_str(), "Overdrive");
        assert_eq!(Label::new("A very long name").as_str(), "A very long ");
        assert_eq!(Label::new("Délai").as_str(), "Delai");
        assert_eq!(Label::new("90°").as_str(), "90deg");
        assert_eq!(Label::new("a☃").as_str(), "a?");
    }

    #[test]
    fn a_label_takes_thirteen_bytes_and_reads_back() {
        let mut out = Vec::new();
        Label::new("Cutoff").write(&mut out);
        assert_eq!(out.len(), LABEL_BYTES);
        assert_eq!(&out[..7], b"Cutoff\0");
        let mut reader = Reader::new(&out);
        assert_eq!(reader.label().unwrap(), Label::new("Cutoff"));
        reader.finish().unwrap();
    }

    #[test]
    fn a_full_label_has_its_end_marker() {
        let mut out = Vec::new();
        Label::new("ABCDEFGHIJKLMNOP").write(&mut out);
        assert_eq!(out[LABEL_CHARS], 0);
    }

    #[test]
    fn a_pair_is_high_then_low_seven_bits() {
        let mut out = Vec::new();
        push_pair(&mut out, 300);
        assert_eq!(out, [2, 44]);
        assert_eq!(Reader::new(&out).pair().unwrap(), 300);
    }

    #[test]
    fn hashes_are_seven_bit_and_stable() {
        let hash = PluginHash::of("vimtracker/fx/delay");
        assert!(hash.0.iter().all(|&byte| byte < 0x80));
        assert_eq!(hash, PluginHash::of("vimtracker/fx/delay"));
        assert_ne!(hash, PluginHash::of("vimtracker/fx/reverb"));
        assert!(ParamHash::of("cut").0.iter().all(|&byte| byte < 0x80));
    }

    #[test]
    fn a_short_message_fails_rather_than_panics() {
        let mut reader = Reader::new(&[1]);
        assert_eq!(reader.pair(), Err(DecodeError::Short));
    }
}
