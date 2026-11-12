//! The project's bank: the samples a sampler track's `Sample` setting picks from, numbered `01` to
//! `FF`, after Elektron's project pool. A file joins it once, however many tracks and steps play it,
//! and each entry keeps how it plays, so a kit's sound plays the same on any track.
//!
//! An entry is metadata over audio that's never written: which file, found by its path and its
//! content hash, where it starts, ends, loops and is sliced, and how it plays. The rest of the
//! sample editor's work goes in its [`SampleEdit`] later.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::ops::Range;
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
    /// A peak level to bring the file to, in dBFS, from [`MIN_NORMALIZE`] to 0, as a gain worked
    /// out from its analysis rather than written into the audio, as Bitwig's Normalize writes a
    /// gain. The same gain scales every channel, and the entry's own gain comes on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalize: Option<f32>,
    /// Whether the sample page puts a point it places on the nearest zero crossing, so a slice
    /// starts without a click. It only changes where points go, never how anything plays.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub snap_zero: bool,
    /// How it keeps to the song's tempo when a track's `Play` is `Tempo`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub warp: Warp,
}

/// How an entry keeps to the song's tempo under `Play`'s `Tempo`, as Ableton's warp modes say how
/// a clip does. The live stretcher (#285) and the offline one (#284) come as modes of their own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Warp {
    /// Faster or slower, its pitch moving with its speed, as a record's does. Its note and tune
    /// don't change its speed, which the tempo has, as Ableton's Re-Pitch takes no transpose.
    #[default]
    Repitch,
    /// Each of its slices, or each sound its `Sens` finds if it has none, starts at its own time
    /// at the tempo and plays at its own speed, its note and tune moving its pitch: cut short with
    /// a fade if the next is due first, and leaving a gap if it ends before. After Ableton's
    /// Beats, for drums.
    Beats,
}

/// What each warp is called, in order, as the sample page lists them.
pub const WARP_NAMES: [&str; 2] = ["Re-pitch", "Beats"];

impl Warp {
    pub const ALL: [Self; 2] = [Self::Repitch, Self::Beats];

    pub fn name(self) -> &'static str {
        WARP_NAMES[self as usize]
    }
}

/// The most cents an entry's tune can be moved either way, short of a semitone, which is the
/// tune's.
pub const MAX_CENTS: i8 = 99;

/// The lowest level `:normalize` brings a file to, in dBFS.
pub const MIN_NORMALIZE: f32 = -60.0;

// The gain and the level normalised to are never NaNs, so they're equal to themselves.
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

    /// What normalising multiplies the sound by, given the file's peak, of whichever channel is
    /// louder: enough to bring that peak to the level asked for. It's 1 with nothing asked for,
    /// for a silent file, and while the peak isn't known yet.
    pub fn normalize_gain(self, peak: Option<f32>) -> f32 {
        match (self.normalize, peak) {
            (Some(level), Some(peak)) if peak > 0.0 => 10_f32.powf(level / 20.0) / peak,
            _ => 1.0,
        }
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

/// Where an entry starts, ends and loops and where its slices start, in frames at its file's own
/// rate, so a point stays on the same sound whatever rate the program runs at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Points {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub start: u64,
    /// One past its last frame, or none for the file's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
    #[serde(default, rename = "loop", skip_serializing_if = "Option::is_none")]
    pub loop_: Option<Loop>,
    /// Its slices, in order of where they start, numbered by their place, as Vim numbers lines.
    /// With none, the entry plays as one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<Slice>,
    /// The id the next new slice takes, so no id is used twice within the entry.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub next_id: SliceId,
    /// The slices named by a letter, `a` to `z`, as Vim's marks name lines, so a step can lock a
    /// slice by its name, `SLI 'a`, and keep playing it as slices before it come and go. One letter
    /// names one slice, and a slice can have several.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub marks: BTreeMap<char, SliceId>,
}

/// The letters that can name a slice, as Vim's marks within a file are `a` to `z`.
pub const MARK_LETTERS: std::ops::RangeInclusive<char> = 'a'..='z';

/// How many letters can name slices.
pub const MARK_COUNT: usize = 26;

/// Whether a letter can name a slice.
pub fn is_mark(letter: char) -> bool {
    MARK_LETTERS.contains(&letter)
}

/// A mark's place among the letters, `a` first, if it's one.
pub fn mark_index(letter: char) -> Option<usize> {
    is_mark(letter).then(|| letter as usize - 'a' as usize)
}

/// A slice's own name within its entry, which it keeps as slices before it come and go and its
/// number changes, so a mark can name it (#271). It's never given to another slice of the entry.
pub type SliceId = u32;

/// The most slices an entry can have: as many as `SLI`'s two hex digits number, `00` to `FF`.
pub const MAX_SLICES: usize = 256;

/// One slice of an entry: where it starts, and how it plays where that isn't as the entry does.
/// It ends where the next one starts, or at the entry's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slice {
    pub id: SliceId,
    /// Its first frame, at the file's own rate.
    pub start: u64,
    #[serde(default, skip_serializing_if = "SliceParams::is_empty")]
    pub over: SliceParams,
}

/// How a slice plays where it differs from its entry, as each of Renoise's slices has its own
/// volume and panning. Each that's none plays as the entry does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SliceParams {
    /// In decibels, in place of the entry's gain. The entry's normalising still applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f32>,
    /// From [`MOST_PAN`] to the left, below 0, to as far to the right, where the entry is in the
    /// middle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pan: Option<i8>,
    /// In semitones, in place of the entry's tune, with the entry's cents on top.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tune: Option<i8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    /// In frames at the file's own rate, from the slice's start and to its end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_in: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_out: Option<u32>,
}

// The gain is never a NaN, as the entry's isn't.
impl Eq for SliceParams {}

/// How far a slice can be panned either way, as Ableton's and Renoise's pans go from 50 left to
/// 50 right.
pub const MOST_PAN: i8 = 50;

impl SliceParams {
    /// Whether the slice plays just as its entry does.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// How the slice plays: its entry's settings, with the slice's own in their place.
    pub fn over(&self, play: PlayParams) -> PlayParams {
        PlayParams {
            gain_db: self.gain_db.unwrap_or(play.gain_db),
            tune: self.tune.unwrap_or(play.tune),
            reverse: self.reverse.unwrap_or(play.reverse),
            fade_in: self.fade_in.unwrap_or(play.fade_in),
            fade_out: self.fade_out.unwrap_or(play.fade_out),
            ..play
        }
    }

    /// What its pan multiplies the left and the right by: exactly 1 for both in the middle, and
    /// keeping equal power either way, as a track's pan does.
    pub fn pan_gains(&self) -> [f32; 2] {
        let pan = self.pan.unwrap_or(0).clamp(-MOST_PAN, MOST_PAN);
        if pan == 0 {
            return [1.0, 1.0];
        }
        let position = (f32::from(pan) / f32::from(MOST_PAN) + 1.0) / 2.0;
        let angle = position * std::f32::consts::FRAC_PI_2;
        [
            angle.cos() * std::f32::consts::SQRT_2,
            angle.sin() * std::f32::consts::SQRT_2,
        ]
    }
}

impl Points {
    /// Whether they're the whole file, with no loop and no slices.
    pub fn is_whole(&self) -> bool {
        *self == Self::default()
    }

    /// One past the last frame that plays, in a file this long.
    pub fn end_in(&self, length: u64) -> u64 {
        self.end.unwrap_or(length).min(length)
    }

    /// How many slices there are to choose from: the entry as one, when it has none.
    pub fn slice_count(&self) -> usize {
        self.slices.len().max(1)
    }

    /// The frames a slice plays, by its number, clamped to the last, in a file this long: from its
    /// start to the next one's, or to the entry's end, inside the entry's start and end. With no
    /// slices, the entry is one, from its start to its end.
    pub fn slice_frames(&self, number: usize, length: u64) -> Range<u64> {
        let end = self.end_in(length);
        let start = self.start.min(end);
        if self.slices.is_empty() {
            return start..end;
        }
        let number = number.min(self.slices.len() - 1);
        let from = self.slices[number].start.clamp(start, end);
        let to = self
            .slices
            .get(number + 1)
            .map_or(end, |next| next.start.clamp(start, end));
        from..to.max(from)
    }

    /// The number of the slice a frame is in. One before the first slice is in the first.
    pub fn slice_at(&self, frame: u64) -> usize {
        self.slices
            .iter()
            .rposition(|slice| slice.start <= frame)
            .unwrap_or(0)
    }

    /// Where `count` slices of the same length start, from the entry's start to its end in a file
    /// this long, as Polyend's `Evenly` and Renoise's equal slicing make them. Fewer come back if
    /// the entry has fewer frames than that.
    pub fn even_starts(&self, count: usize, length: u64) -> Vec<u64> {
        let end = self.end_in(length);
        let start = self.start.min(end);
        let count = count.clamp(1, MAX_SLICES) as u128;
        let mut starts: Vec<u64> = (0..count)
            .map(|at| start + (u128::from(end - start) * at / count) as u64)
            .collect();
        starts.dedup();
        starts
    }

    /// Cut the entry into slices starting at these frames, or into none, keeping the id and the
    /// settings of each slice that starts where a new one does and giving the rest new ids. It
    /// gives the number each old slice has now, that of the slice holding the frame it started on,
    /// so steps that chose it keep playing the same sound, as Vim's marks follow their lines. With
    /// no slices before or after, there's no numbering to follow, and it gives none.
    ///
    /// Each mark moves the same way, to the slice holding its old slice's start. So a split keeps
    /// the marks on the part that keeps the start, a join gives the slice that's left the marks of
    /// the one it took in, and cutting into none drops them all.
    pub fn reslice(&mut self, starts: &[u64]) -> Option<Vec<u8>> {
        let mut starts = starts.to_vec();
        starts.sort_unstable();
        starts.dedup();
        starts.truncate(MAX_SLICES);
        let old = std::mem::take(&mut self.slices);
        for start in starts {
            let slice = match old.iter().find(|slice| slice.start == start) {
                Some(kept) => *kept,
                None => {
                    let id = self.next_id;
                    self.next_id += 1;
                    Slice {
                        id,
                        start,
                        over: SliceParams::default(),
                    }
                }
            };
            self.slices.push(slice);
        }
        let marks = std::mem::take(&mut self.marks);
        if !self.slices.is_empty() {
            self.marks = marks
                .into_iter()
                .filter_map(|(letter, id)| {
                    let start = old.iter().find(|slice| slice.id == id)?.start;
                    Some((letter, self.slices[self.slice_at(start)].id))
                })
                .collect();
        }
        if old.is_empty() || self.slices.is_empty() {
            return None;
        }
        Some(
            old.iter()
                .map(|slice| self.slice_at(slice.start) as u8)
                .collect(),
        )
    }

    /// Where the slices would start with those starting inside a stretch of frames, from its first
    /// to before its end, put in place of these, as a range re-slices only the slices inside it.
    /// Those outside stay, and the stretch's end starts one if it's inside the entry, so what
    /// comes after plays as it did. An entry with no slices is one from its start.
    pub fn restarted(&self, within: Range<u64>, inside: &[u64], length: u64) -> Vec<u64> {
        let end = self.end_in(length);
        let start = self.start.min(end);
        let mut starts: Vec<u64> = match self.slices.is_empty() {
            true => vec![start],
            false => self.slices.iter().map(|slice| slice.start).collect(),
        };
        starts.retain(|at| !within.contains(at));
        starts.extend(inside.iter().filter(|at| within.contains(at)));
        if within.end < end {
            starts.push(within.end);
        }
        starts.retain(|at| (start..end).contains(at));
        starts.sort_unstable();
        starts.dedup();
        starts
    }

    /// Where the slices would start with one more starting at this frame, splitting the slice it's
    /// in: none if a slice already starts there or it's outside the entry. An entry with no slices
    /// is one from its start, which the split cuts in two.
    pub fn split_starts(&self, at: u64) -> Option<Vec<u64>> {
        let inside = at > self.start && self.end.is_none_or(|end| at < end);
        let mut starts: Vec<u64> = match self.slices.is_empty() {
            true => vec![self.start],
            false => self.slices.iter().map(|slice| slice.start).collect(),
        };
        if !inside || starts.contains(&at) {
            return None;
        }
        starts.push(at);
        starts.sort_unstable();
        Some(starts)
    }

    /// Where the slices would start with a slice joined to the next, by its number: the same
    /// without the next one's start, or none if there's no next.
    pub fn join_starts(&self, number: usize) -> Option<Vec<u64>> {
        self.slices.get(number + 1)?;
        let mut starts: Vec<u64> = self.slices.iter().map(|slice| slice.start).collect();
        starts.remove(number + 1);
        Some(starts)
    }

    /// The number of the slice a letter names, if it names one that's still there.
    pub fn marked(&self, letter: char) -> Option<usize> {
        let id = *self.marks.get(&letter)?;
        self.slices.iter().position(|slice| slice.id == id)
    }

    /// The letters naming a slice, by its number, in order.
    pub fn marks_of(&self, number: usize) -> Vec<char> {
        let Some(slice) = self.slices.get(number) else {
            return Vec::new();
        };
        self.marks
            .iter()
            .filter(|(_, id)| **id == slice.id)
            .map(|(letter, _)| *letter)
            .collect()
    }

    /// Every mark naming a slice that's there, in the order the slices come and then by letter,
    /// each with its slice's number.
    pub fn marks_in_order(&self) -> Vec<(char, usize)> {
        let mut marks: Vec<(char, usize)> = self
            .marks
            .keys()
            .filter_map(|letter| Some((*letter, self.marked(*letter)?)))
            .collect();
        marks.sort_by_key(|(letter, number)| (*number, *letter));
        marks
    }

    /// Name a slice, by its number, with a letter, which leaves whatever slice it named before.
    /// Whether there's such a slice and the letter can name it.
    pub fn set_mark(&mut self, letter: char, number: usize) -> bool {
        match self.slices.get(number) {
            Some(slice) if is_mark(letter) => {
                self.marks.insert(letter, slice.id);
                true
            }
            _ => false,
        }
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
    #[serde(default, skip_serializing_if = "Hints::is_default")]
    pub hints: Hints,
}

fn is_default_play(play: &PlayParams) -> bool {
    *play == PlayParams::default()
}

/// What an entry's sound is like, for finding its sounds and slicing it, which never changes how
/// it plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hints {
    /// How readily a rise counts as a sound starting, from 0, the strongest only, to
    /// [`MOST_SENS`](crate::onsets::MOST_SENS), nearly every one, as Simpler's and Renoise's
    /// slicing sensitivity. `w` and `b` on the sample page and `:slice transients` go by it.
    #[serde(default = "default_sens", skip_serializing_if = "is_default_sens")]
    pub sens: u8,
    /// How many bars long it is, from its start to its end, for `:slice beat` to cut it by rather
    /// than the song's tempo, and for `Play`'s `Tempo` to fit to. None to go by the tempo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bars: Option<u16>,
    /// How many of the song's steps it lasts, where that isn't a whole number of bars, in place of
    /// `bars`: one or the other says how long it is, never both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u16>,
}

fn default_sens() -> u8 {
    crate::onsets::DEFAULT_SENS
}

fn is_default_sens(sens: &u8) -> bool {
    *sens == default_sens()
}

impl Default for Hints {
    fn default() -> Self {
        Self {
            sens: default_sens(),
            bars: None,
            steps: None,
        }
    }
}

/// How many of the song's steps there are in a bar.
const BAR_STEPS: u32 = crate::constants::BAR_STEPS as u32;

/// The most steps an entry can be said to last: as many as the most bars `:sample bars` takes.
pub const MOST_STEPS: u32 = 256 * BAR_STEPS;

impl Hints {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// How many of the song's steps it lasts, from its start to its end, if it says.
    pub fn length_steps(&self) -> Option<u32> {
        match (self.steps, self.bars) {
            (Some(steps), _) => Some(u32::from(steps)),
            (None, Some(bars)) => Some(u32::from(bars) * BAR_STEPS),
            (None, None) => None,
        }
    }

    /// Say how many steps it lasts, or that it doesn't say: in bars if it's a whole number of
    /// them, so `:sample bars` says the same.
    pub fn set_length_steps(&mut self, steps: Option<u32>) {
        let steps = steps.map(|steps| steps.clamp(1, MOST_STEPS));
        (self.bars, self.steps) = match steps {
            None => (None, None),
            Some(steps) if steps.is_multiple_of(BAR_STEPS) => {
                (Some((steps / BAR_STEPS) as u16), None)
            }
            Some(steps) => (None, Some(steps as u16)),
        };
    }
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
                hints: Hints::default(),
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
