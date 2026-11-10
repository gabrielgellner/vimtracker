//! The project's bank: the samples a sampler track's `Sample` setting picks from, numbered `01` to
//! `FF`, after Elektron's project pool. A file joins it once, however many tracks and steps play it,
//! and each entry keeps how it plays, so a kit's sound plays the same on any track.
//!
//! An entry is metadata over audio that's never written: which file, found by its path and its
//! content hash, where it starts, ends and loops, and how it plays. Slices and the rest of the
//! sample editor's work go in its [`SampleEdit`] later.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// How many entries the bank holds, numbered `01` to `FF`. `00` is no sample, as a Digitakt's
/// `OFF` is, so a track or a step can choose silence and a new track plays nothing.
pub const BANK_SIZE: usize = 255;

/// A file's content hash: 128 bits of XXH3 over its bytes, so a file is known by what's in it as
/// well as where it is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash(pub u128);

impl ContentHash {
    /// Not worked out, as for a song built in code whose samples are put in the pool by name.
    pub const UNKNOWN: Self = Self(0);

    /// The hash of these bytes.
    pub fn of(bytes: &[u8]) -> Self {
        Self(xxhash_rust::xxh3::xxh3_128(bytes))
    }

    /// The hash of a file's bytes.
    pub fn of_file(path: &Path) -> io::Result<Self> {
        fs::read(path).map(|bytes| Self::of(&bytes))
    }

    /// Whether it's been worked out.
    pub fn is_known(self) -> bool {
        self != Self::UNKNOWN
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// Songs store a hash as 32 hex digits.
impl Serialize for ContentHash {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        u128::from_str_radix(&text, 16)
            .map(Self)
            .map_err(|_| serde::de::Error::custom(format!("{text:?} isn't a content hash")))
    }
}

/// A file an entry plays, where it is and what's in it. The path is relative to the project
/// folder if the file's inside it, and absolute if not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub path: String,
    #[serde(default, skip_serializing_if = "is_unknown")]
    pub hash: ContentHash,
}

fn is_unknown(hash: &ContentHash) -> bool {
    !hash.is_known()
}

/// A stretch of one file. For now it's always the whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub source: Source,
}

/// What plays, in order: one segment over one whole file for every entry, until cutting and
/// pasting come.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    pub segments: Vec<Segment>,
}

/// How an entry plays, whatever track plays it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayParams {
    /// In decibels, always a finite number, as a kit's gain is checked to be.
    #[serde(default, skip_serializing_if = "is_zero_gain")]
    pub gain_db: f32,
    /// In semitones, up to a kit's `MAX_TUNE` either way.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tune: i8,
    /// In cents, up to [`MAX_CENTS`] either way, on top of the tune.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cents: i8,
    /// Whether it plays from its end back to its start.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reverse: bool,
    /// How long it fades in from its start and out to its end, in frames at its file's own rate.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade_in: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub fade_out: u32,
}

/// The most cents an entry's tune can be moved either way, short of a semitone, which is the
/// tune's.
pub const MAX_CENTS: i8 = 99;

// The gain is never a NaN, so it's equal to itself.
impl Eq for PlayParams {}

fn is_zero_gain(gain: &f32) -> bool {
    *gain == 0.0
}

/// Whether a value is its type's default, so a song needn't store it.
fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

impl PlayParams {
    /// What the gain multiplies the sound by.
    pub fn gain(self) -> f32 {
        10_f32.powf(self.gain_db / 20.0)
    }

    /// How much faster than its note the sample plays, for its tune and cents.
    pub fn speed(self) -> f64 {
        let semitones = f64::from(self.tune) + f64::from(self.cents) / 100.0;
        2_f64.powf(semitones / 12.0)
    }
}

/// A loop inside an entry, from its start up to its end, in frames at the file's own rate. Whether
/// it loops forward or ping-pongs is the track's `Play`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loop {
    pub start: u64,
    /// One past its last frame.
    pub end: u64,
    /// How long the crossfade from its end into its start lasts, in frames at the file's own rate.
    /// With none, a forward loop still has a short declick.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub crossfade: u32,
}

/// Where an entry starts, ends and loops, in frames at its file's own rate, so a point stays on
/// the same sound whatever rate the program runs at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Points {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: u64,
    /// One past its last frame, or none for the file's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    #[serde(default, rename = "loop", skip_serializing_if = "Option::is_none")]
    pub loop_: Option<Loop>,
}

impl Points {
    /// Whether they're the whole file, with no loop.
    pub fn is_whole(&self) -> bool {
        *self == Self::default()
    }
}

/// Everything about how a sample plays, apart from the audio, which is never written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleEdit {
    pub timeline: Timeline,
    #[serde(default, skip_serializing_if = "Points::is_whole")]
    pub points: Points,
    #[serde(default, skip_serializing_if = "is_default_play")]
    pub play: PlayParams,
}

fn is_default_play(play: &PlayParams) -> bool {
    *play == PlayParams::default()
}

/// One sample in the bank.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BankEntry {
    /// A short name, two characters as a track's are, such as a kit's `BD`, or empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// What it is, such as "Bass Drum 909", or its file's name.
    pub name: String,
    pub edit: SampleEdit,
}

impl BankEntry {
    /// An entry playing a whole file as it is, named after the file.
    pub fn of_file(path: impl Into<String>, hash: ContentHash) -> Self {
        let path = path.into();
        let name = file_stem(&path);
        Self {
            label: String::new(),
            name,
            edit: SampleEdit {
                timeline: Timeline {
                    segments: vec![Segment {
                        source: Source { path, hash },
                    }],
                },
                points: Points::default(),
                play: PlayParams::default(),
            },
        }
    }

    /// The file it plays from first, which every entry has for now.
    pub fn source(&self) -> Option<&Source> {
        self.sources().next()
    }

    /// Every file it plays from.
    pub fn sources(&self) -> impl Iterator<Item = &Source> {
        self.edit
            .timeline
            .segments
            .iter()
            .map(|segment| &segment.source)
    }

    /// Every file it plays from, to change where they are.
    pub fn sources_mut(&mut self) -> impl Iterator<Item = &mut Source> {
        self.edit
            .timeline
            .segments
            .iter_mut()
            .map(|segment| &mut segment.source)
    }

    /// Whether it plays this file: one at the same path, or with the same content.
    pub fn plays(&self, source: &Source) -> bool {
        self.sources()
            .any(|own| own.path == source.path || (own.hash.is_known() && own.hash == source.hash))
    }

    /// Its name as a list shows it: its label and name, as `BD Bass Drum 909`.
    pub fn title(&self) -> String {
        match self.label.as_str() {
            "" => self.name.clone(),
            label => format!("{label} {}", self.name),
        }
    }
}

/// A file's name without its folder or extension.
fn file_stem(path: &str) -> String {
    Path::new(path).file_stem().map_or_else(
        || path.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// The project's samples, by number. Numbers stay where they are when an entry is removed, as an
/// Elektron's slots do, so steps that lock one keep playing what they did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bank {
    entries: BTreeMap<u8, BankEntry>,
}

impl Bank {
    /// The entry with this number, if there is one. `00` is never one.
    pub fn get(&self, number: u8) -> Option<&BankEntry> {
        self.entries.get(&number)
    }

    /// Every entry, in order of number.
    pub fn iter(&self) -> impl Iterator<Item = (u8, &BankEntry)> {
        self.entries.iter().map(|(number, entry)| (*number, entry))
    }

    /// Every entry, in order of number, to change.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u8, &mut BankEntry)> {
        self.entries
            .iter_mut()
            .map(|(number, entry)| (*number, entry))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry already playing this file, if one is.
    pub fn find(&self, source: &Source) -> Option<u8> {
        self.iter()
            .find(|(_, entry)| entry.plays(source))
            .map(|(number, _)| number)
    }

    /// The lowest number with no entry, if the bank isn't full.
    pub fn free(&self) -> Option<u8> {
        (1..=BANK_SIZE as u8).find(|number| !self.entries.contains_key(number))
    }

    /// Put an entry at a number, replacing any there, or empty it. `00` stays empty.
    pub fn set(&mut self, number: u8, entry: Option<BankEntry>) {
        match entry {
            Some(entry) if number != 0 => {
                self.entries.insert(number, entry);
            }
            _ => {
                self.entries.remove(&number);
            }
        }
    }

    /// Add an entry at the lowest free number, unless one already plays its file, and give the
    /// number it's at. A full bank gives none.
    pub fn add(&mut self, entry: BankEntry) -> Option<u8> {
        if let Some(found) = entry.source().and_then(|source| self.find(source)) {
            return Some(found);
        }
        let number = self.free()?;
        self.entries.insert(number, entry);
        Some(number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_joins_the_bank_once_found_by_its_path_or_its_content() {
        let mut bank = Bank::default();
        let kick = ContentHash::of(b"kick");
        assert_eq!(bank.add(BankEntry::of_file("/a/kick.wav", kick)), Some(1));
        assert_eq!(
            bank.add(BankEntry::of_file("/a/snare.wav", ContentHash::of(b"sn"))),
            Some(2)
        );
        assert_eq!(bank.add(BankEntry::of_file("/a/kick.wav", kick)), Some(1));
        assert_eq!(
            bank.add(BankEntry::of_file("/b/moved-kick.wav", kick)),
            Some(1),
            "the same content somewhere else"
        );
        let unknown = ContentHash::UNKNOWN;
        assert_eq!(bank.add(BankEntry::of_file("s", unknown)), Some(3));
        assert_eq!(
            bank.add(BankEntry::of_file("t", unknown)),
            Some(4),
            "an unknown hash matches nothing"
        );
        assert_eq!(bank.get(1).map(|entry| entry.name.as_str()), Some("kick"));
    }

    #[test]
    fn numbers_stay_put_and_a_gap_is_filled_first() {
        let mut bank = Bank::default();
        for name in ["a", "b", "c"] {
            bank.add(BankEntry::of_file(name, ContentHash::UNKNOWN));
        }
        bank.set(2, None);
        assert_eq!(bank.get(3).map(|entry| entry.name.as_str()), Some("c"));
        assert_eq!(bank.free(), Some(2));
        bank.set(0, Some(BankEntry::of_file("z", ContentHash::UNKNOWN)));
        assert!(bank.get(0).is_none(), "00 is no sample");
    }

    #[test]
    fn a_full_bank_takes_no_more() {
        let mut bank = Bank::default();
        for at in 0..BANK_SIZE {
            assert!(
                bank.add(BankEntry::of_file(at.to_string(), ContentHash::UNKNOWN))
                    .is_some()
            );
        }
        assert_eq!(bank.free(), None);
        assert_eq!(
            bank.add(BankEntry::of_file("more", ContentHash::UNKNOWN)),
            None
        );
        assert_eq!(bank.iter().last().map(|(number, _)| number), Some(0xFF));
    }

    #[test]
    fn a_hash_reads_back_from_its_hex() {
        let hash = ContentHash::of(b"some audio");
        let text = ron::to_string(&hash).unwrap();
        assert_eq!(text.len(), 32 + 2);
        assert_eq!(ron::from_str::<ContentHash>(&text).unwrap(), hash);
        assert!(ron::from_str::<ContentHash>("\"xyz\"").is_err());
    }

    #[test]
    fn a_play_settings_gain_and_tune() {
        let play = PlayParams {
            gain_db: -6.0,
            tune: 12,
            ..PlayParams::default()
        };
        assert!((play.gain() - 0.501).abs() < 1e-3);
        assert_eq!(play.speed(), 2.0);
        let flat = PlayParams {
            tune: 1,
            cents: -50,
            ..PlayParams::default()
        };
        assert!((flat.speed() - 2_f64.powf(0.5 / 12.0)).abs() < 1e-12);
        assert_eq!(PlayParams::default().gain(), 1.0);
        assert_eq!(PlayParams::default().speed(), 1.0);
    }
}
