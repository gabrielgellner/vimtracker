//! Filling a column across a selection with a shaped ramp, random values, a walk, pink noise, a
//! wave, two values in turn, or a short list of them in turn, or reworking the values already
//! there. On the Note column, the steps are the scale's degrees while it's on, `Acid` writes a 303
//! line, its notes, accents and slides together, and `Follow` mirrors another track's phrase.
//! `Multiply` reworks the rhythm there rather than its values, the bit transforms each value's
//! bits, and `Grids` writes a drum part from a map of real patterns, one part a track across a
//! kit's.

mod acid;
mod bits;
mod follow;
mod grids;
pub mod params;
mod render;
mod walk;

pub use acid::{Acid, MAX_ACID_NOTES, chance as acid_chance, weight as acid_weight};
pub use bits::{Bits, MAX_FACTOR, MAX_TURN};
pub(crate) use follow::copy_fx;
pub use follow::{Follow, MAX_TRANSPOSE, Source, first_note};
pub use grids::{ACCENT_LEVEL, DrumPart, GRIDS_STEPS, Grids, parts as drum_parts};
pub use params::{Context, GEN_COLUMNS, GEN_ROWS, GenParam, Starts};
pub use render::{GenCell, RowView, render_into};
use serde::{Deserialize, Serialize};
pub use walk::{Edge, MAX_DEPTH, MIN_DEPTH, Pink, Walk, default_step};

use crate::constants::{MAX_PHRASE_STEPS, TICKS_PER_QUARTER};
use crate::lfo::{Rate, Shape as Wave};
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::step::{FxCommand, MAX_NOTE, MAX_VELOCITY, MIDDLE_C, MIN_NOTE, Note, Step, nudge_text};

/// The furthest `curve` and `s` go either way.
pub const MAX_BEND: i8 = 100;
/// The waves a column can follow, in the order `+` goes through them.
pub const WAVES: [Wave; 6] = [
    Wave::Sine,
    Wave::Tri,
    Wave::Saw,
    Wave::Ramp,
    Wave::Square,
    Wave::Smooth,
];
/// How long a wave's cycle is unless told otherwise: a half note.
pub const DEFAULT_RATE: WaveRate = WaveRate::Time(Rate::Note(TICKS_PER_QUARTER as u32 * 2));
/// The most cycles a wave can make across a selection.
pub const MAX_CYCLES: u16 = 64;

/// How long a wave's cycle is: a length in time, as an LFO's, or a whole number of cycles across
/// the selection, so they fit it whatever its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveRate {
    Time(Rate),
    Cycles(u16),
}

impl WaveRate {
    /// Faster or slower by a small step: a time as an LFO's moves, or one cycle more or fewer.
    pub fn nudged(self, faster: bool) -> Self {
        match self {
            Self::Time(rate) => Self::Time(rate.nudged(faster)),
            Self::Cycles(cycles) => Self::Cycles(Self::moved(cycles, faster, 1)),
        }
    }

    /// Faster or slower by a large step: a time as an LFO's jumps, or four cycles more or fewer.
    pub fn jumped(self, faster: bool) -> Self {
        match self {
            Self::Time(rate) => Self::Time(rate.jumped(faster)),
            Self::Cycles(cycles) => Self::Cycles(Self::moved(cycles, faster, 4)),
        }
    }

    fn moved(cycles: u16, more: bool, by: u16) -> u16 {
        let moved = if more {
            cycles.saturating_add(by)
        } else {
            cycles.saturating_sub(by)
        };
        moved.clamp(1, MAX_CYCLES)
    }
}

/// A time as an LFO's rate shows, and cycles as `x3`.
impl std::fmt::Display for WaveRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Time(rate) => rate.fmt(f),
            Self::Cycles(cycles) => write!(f, "x{cycles}"),
        }
    }
}

/// Why typed text isn't a wave's rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaveRateError;

/// A time as an LFO's rate is typed, but in time with the rows, or `x` and a number of cycles.
impl std::str::FromStr for WaveRate {
    type Err = WaveRateError;

    fn from_str(text: &str) -> Result<Self, WaveRateError> {
        let text = text.trim();
        if let Some(count) = text.strip_prefix(['x', 'X']) {
            return match count.trim().parse::<u16>() {
                Ok(cycles) if (1..=MAX_CYCLES).contains(&cycles) => Ok(Self::Cycles(cycles)),
                _ => Err(WaveRateError),
            };
        }
        match text.parse::<Rate>() {
            Ok(Rate::Hertz(_)) | Err(_) => Err(WaveRateError),
            Ok(rate) => Ok(Self::Time(rate)),
        }
    }
}
/// How many hits a Euclidean `Where` starts with: in eight rows, the tresillo.
pub const EUCLID_HITS: u16 = 3;
const EUCLID: Where = Where::Euclid {
    hits: EUCLID_HITS,
    rotate: 0,
};
/// The most values a `List` fill goes through, so a shape stays plain data of a fixed size.
pub const MAX_VALUES: usize = 8;

/// What goes in the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FillKind {
    /// From one value to the other, along the curve.
    Ramp,
    /// Anywhere between the two values, new on each row.
    Random,
    /// From the first value, each a small step from the last, between the two: Brownian.
    Walk,
    /// Between the two values, wandering more than `Random` and less than `Walk`: pink, or 1/f.
    Pink,
    /// Swinging between the two values, by where each row falls in the wave's cycle.
    Wave,
    /// One value then the other, on the rows it fills, however they're spaced.
    Alternate,
    /// Each value of a short list in turn, on the rows it fills, then round again.
    List,
    /// A 303 line: notes from a small set, with accents, slides and octaves, and on an empty
    /// column a rhythm weighted toward the strong beats.
    Acid,
    /// A drum part from Grids' map of patterns, one of the kick's, the snare's and the hats'.
    Grids,
    /// Another phrase's notes, with their velocities and gates, turned on the way as a canon's
    /// voices are.
    Follow,
    /// The values there, from their own lowest and highest onto the two values, along the curve.
    Scale,
    /// The values there, each flipped within their own range.
    Invert,
    /// The values there, dealt to the same rows in a new order.
    Shuffle,
    /// The values there, in the opposite order.
    Reverse,
    /// The notes there, each moved to the nearest in the scale.
    Snap,
    /// The rhythm there, sixteen rows a number, multiplied, as Numeric Repetitor's is.
    Multiply,
    /// The values there as bits, some of them flipped, as Xaoc's Leibniz Lipsk does.
    InvertBits,
    /// The values there as bits, only some of them kept, as Leibniz's Gera does.
    MaskBits,
    /// The values there as bits, turned round.
    RotateBits,
}

impl FillKind {
    /// Every fill, in the order `+` goes through them.
    pub const ALL: [Self; 19] = [
        Self::Ramp,
        Self::Random,
        Self::Walk,
        Self::Pink,
        Self::Wave,
        Self::Alternate,
        Self::List,
        Self::Acid,
        Self::Grids,
        Self::Follow,
        Self::Scale,
        Self::Invert,
        Self::Shuffle,
        Self::Reverse,
        Self::Snap,
        Self::Multiply,
        Self::InvertBits,
        Self::MaskBits,
        Self::RotateBits,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ramp => "Ramp",
            Self::Random => "Random",
            Self::Walk => "Walk",
            Self::Pink => "1/f",
            Self::Wave => "Wave",
            Self::Alternate => "Alternate",
            Self::List => "List",
            Self::Acid => "Acid",
            Self::Grids => "Grids",
            Self::Follow => "Follow",
            Self::Scale => "Scale",
            Self::Invert => "Invert",
            Self::Shuffle => "Shuffle",
            Self::Reverse => "Reverse",
            Self::Snap => "Snap",
            Self::Multiply => "Multiply",
            Self::InvertBits => "Invert bits",
            Self::MaskBits => "Mask bits",
            Self::RotateBits => "Rotate bits",
        }
    }

    /// What it does, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Ramp => "From one value to the other",
            Self::Random => "Anywhere between the two",
            Self::Walk => "A small step at a time",
            Self::Pink => "Pink, between random and a walk",
            Self::Wave => "Swinging between the two",
            Self::Alternate => "One value, then the other",
            Self::List => "A few values in turn",
            Self::Acid => "A 303 line, with accents and slides",
            Self::Grids => "A drum part from Grids' map",
            Self::Follow => "Another track's phrase, kept in step",
            Self::Scale => "The values there onto the two",
            Self::Invert => "The values there, upside down",
            Self::Shuffle => "The values there, in a new order",
            Self::Reverse => "The values there, the other way",
            Self::Snap => "The notes there into the scale",
            Self::Multiply => "The rhythm there, times a number",
            Self::InvertBits => "The values there, some bits flipped",
            Self::MaskBits => "The values there, some bits kept",
            Self::RotateBits => "The values there, their bits turned",
        }
    }

    /// Whether it reworks the values already in the rows, rather than making new ones.
    pub fn reworks(self) -> bool {
        matches!(
            self,
            Self::Scale
                | Self::Invert
                | Self::Shuffle
                | Self::Reverse
                | Self::Snap
                | Self::Multiply
                | Self::InvertBits
                | Self::MaskBits
                | Self::RotateBits
        )
    }

    /// Whether it works on each value's bits.
    pub fn on_bits(self) -> bool {
        matches!(self, Self::InvertBits | Self::MaskBits | Self::RotateBits)
    }

    /// Whether it chooses its rows with a `Where`. A `Follow` writes every row from its source,
    /// `Multiply` reads and writes the rhythm itself, and `Grids` makes one of its own.
    pub fn has_where(self) -> bool {
        !matches!(self, Self::Follow | Self::Multiply | Self::Grids)
    }
}

/// Which rows get a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Where {
    /// Rows with a note.
    Notes,
    /// Rows without a note.
    NoNotes,
    /// Rows already holding this command.
    Commands,
    /// Every row.
    AllRows,
    /// Every so many rows, starting from the first.
    Every(u16),
    /// Each row with this percent chance.
    Chance(u8),
    /// `hits` spread as evenly as they go among the rows it can change, then moved `rotate` of
    /// those rows later, going round.
    Euclid { hits: u16, rotate: u16 },
}

impl Where {
    /// The name on screen, without an amount.
    pub fn name(self) -> &'static str {
        match self {
            Self::Notes => "Notes",
            Self::NoNotes => "No notes",
            Self::Commands => "Commands",
            Self::AllRows => "All rows",
            Self::Every(_) => "Every",
            Self::Chance(_) => "Chance",
            Self::Euclid { .. } => "Euclid",
        }
    }
}

/// `hits` spread as evenly as they go over `rows`, by Bjorklund's algorithm, so 3 in 8 is the
/// tresillo `x..x..x.` and 5 in 8 the cinquillo `x.xx.xx.`, as Toussaint gives them. A render
/// works it out in place, over at most `MAX_PHRASE_STEPS` rows.
pub fn euclid(hits: usize, rows: usize) -> Vec<bool> {
    let pattern = render::euclid(hits, rows);
    (0..rows.min(MAX_PHRASE_STEPS))
        .map(|row| pattern.has(row))
        .collect()
}

/// The column being filled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeColumn {
    /// The notes, re-pitched where they are, through the song's scale as it was when the panel
    /// opened: in its degrees while it's on, and in semitones while it's off. With `writes`, it's
    /// a column that had no notes: it writes notes of its own on the rows without one, with that
    /// velocity and the track's gate.
    Note {
        scale: Scale,
        writes: Option<u8>,
    },
    Velocity,
    /// A note's nudge, held as `0` for the earliest to twice `most` for the latest, so it's a plain
    /// range. `most` is the furthest its track's nudges went when the panel opened, `B` at a
    /// sixteenth, which is kept with a live generator as a Note column's scale is.
    Nudge {
        most: i8,
    },
    /// A note's length, the gate cell's last two digits.
    Length,
    Fx {
        slot: usize,
        command: FxCommand,
    },
}

impl ShapeColumn {
    /// The highest value this column takes.
    pub fn highest(self) -> u8 {
        match self {
            Self::Note { .. } => MAX_NOTE,
            Self::Velocity => MAX_VELOCITY,
            Self::Nudge { most } => 2 * most.max(0) as u8,
            Self::Length => u8::MAX,
            Self::Fx { command, .. } => command.highest(),
        }
    }

    /// The lowest value this column takes.
    pub fn lowest(self) -> u8 {
        match self {
            Self::Note { .. } => MIN_NOTE,
            _ => 0,
        }
    }

    /// Which rows it fills unless told otherwise: the ones already holding such a value, or every
    /// row for notes of its own.
    pub fn default_where(self) -> Where {
        match self {
            Self::Note {
                writes: Some(_), ..
            } => Where::AllRows,
            Self::Fx { .. } => Where::Commands,
            _ => Where::Notes,
        }
    }

    /// The fills it offers: `Snap`, `Acid`, `Grids` and `Follow` only mean something for notes,
    /// `Multiply` for notes and FX commands, whose rows are a rhythm, and a transform nothing for
    /// notes of its own, as there are none to rework.
    pub fn fills(self) -> Vec<FillKind> {
        FillKind::ALL
            .into_iter()
            .filter(|kind| {
                !matches!(
                    kind,
                    FillKind::Snap | FillKind::Acid | FillKind::Grids | FillKind::Follow
                ) || matches!(self, Self::Note { .. })
            })
            .filter(|kind| {
                *kind != FillKind::Multiply || matches!(self, Self::Note { .. } | Self::Fx { .. })
            })
            .filter(|kind| !(kind.reworks() && self.writes_notes()))
            .collect()
    }

    /// Whether it writes notes of its own, on a column that had none.
    pub fn writes_notes(self) -> bool {
        matches!(
            self,
            Self::Note {
                writes: Some(_),
                ..
            }
        )
    }

    /// The nudge a value held on the Nudge column is, kept to the column's range, or 0 elsewhere.
    pub fn nudge_of(self, value: u8) -> i8 {
        match self {
            Self::Nudge { most } => {
                (i16::from(value).min(2 * i16::from(most)) - i16::from(most)) as i8
            }
            _ => 0,
        }
    }

    /// A value as the panel shows it: a note's name, a nudge with its sign as the grid shows it,
    /// and anything else in hex.
    pub fn show(self, value: u8) -> String {
        match self {
            Self::Note { .. } => Note::On(value.clamp(MIN_NOTE, MAX_NOTE)).to_string(),
            Self::Nudge { most } => match self.nudge_of(value) {
                0 => "0".to_owned(),
                nudge => nudge_text(nudge, most),
            },
            _ => format!("{value:02X}"),
        }
    }

    /// A list of values as the panel shows it, with a space between each.
    pub fn show_list(self, values: &Values) -> String {
        let shown: Vec<String> = values.as_slice().iter().map(|v| self.show(*v)).collect();
        shown.join(" ")
    }

    /// A value typed for this column, kept in its range: a note's name, a nudge as a sign and one
    /// or two hex digits, or anything else in hex.
    pub fn parse(self, text: &str) -> Option<u8> {
        let text = text.trim();
        match self {
            Self::Note { .. } => parse_note(text),
            Self::Nudge { most } => {
                let (sign, digits) = match text.as_bytes().first()? {
                    b'-' => (-1, &text[1..]),
                    b'+' => (1, &text[1..]),
                    _ => (1, text),
                };
                let size = u8::from_str_radix(digits, 16).ok()?.min(most.max(0) as u8) as i8;
                Some((sign * size + most) as u8)
            }
            _ => u8::from_str_radix(text, 16)
                .ok()
                .map(|v| v.min(self.highest())),
        }
    }

    /// The choices of rows. A note's velocity, nudge or length means nothing without it, so those
    /// only narrow them, as they do for the notes themselves.
    pub fn wheres(self) -> Vec<Where> {
        match self {
            // Notes of its own go on rows without one, so the rows are only a rhythm.
            Self::Note {
                writes: Some(_), ..
            } => vec![Where::AllRows, Where::Every(4), Where::Chance(50), EUCLID],
            Self::Note { .. } | Self::Velocity | Self::Nudge { .. } | Self::Length => {
                vec![Where::Notes, Where::Every(4), Where::Chance(50), EUCLID]
            }
            Self::Fx { .. } => vec![
                Where::Commands,
                Where::AllRows,
                Where::Notes,
                Where::NoNotes,
                Where::Every(4),
                Where::Chance(50),
                EUCLID,
            ],
        }
    }

    /// This column's value on a step, if it has one: an FX value's coarse digits. A row with no
    /// length of its own has none.
    fn value(self, step: &Step) -> Option<u8> {
        let note = matches!(step.note, Note::On(_));
        match self {
            Self::Note { .. } => match step.note {
                Note::On(note) => Some(note),
                _ => None,
            },
            Self::Velocity => note.then(|| step.velocity.unwrap_or(MAX_VELOCITY)),
            Self::Nudge { most } => note.then(|| (step.nudge.clamp(-most, most) + most) as u8),
            Self::Length => step.length().filter(|_| note),
            Self::Fx { slot, command } => step
                .fx
                .get(slot)
                .filter(|fx| fx.command == Some(command))
                .map(|fx| fx.coarse()),
        }
    }

    /// This column's value on a step with its fine part, if it has one. A velocity has none.
    fn exact(self, step: &Step) -> Option<u16> {
        match self {
            Self::Fx { slot, command } => step
                .fx
                .get(slot)
                .filter(|fx| fx.command == Some(command))
                .map(|fx| fx.value),
            _ => self.value(step).map(|value| u16::from(value) << 8),
        }
    }

    /// Whether a step could take a value at all. A row holding another command keeps it, and notes
    /// of its own go round an `OFF`.
    fn can_take(self, step: &Step) -> bool {
        match self {
            Self::Note {
                writes: Some(_), ..
            } => step.note == Note::Empty,
            Self::Note { .. } | Self::Velocity | Self::Nudge { .. } | Self::Length => {
                matches!(step.note, Note::On(_))
            }
            Self::Fx { slot, command } => step
                .fx
                .get(slot)
                .is_some_and(|fx| fx.command.is_none() || fx.command == Some(command)),
        }
    }

    /// Write a value to a step, with no fine part.
    pub fn write(self, step: &mut Step, value: u8) {
        self.write_exact(step, u16::from(value) << 8);
    }

    /// Write a value with a fine part to a step, as its [`cell`](Self::cell). A velocity takes the
    /// whole value.
    pub fn write_exact(self, step: &mut Step, value: u16) {
        self.cell(step, value).apply(self, step);
    }
}

/// A note's name as the grid shows it, like `C-3` or `C#3`, or more loosely, like `c3`, `c-3` or
/// `c#3`.
fn parse_note(text: &str) -> Option<u8> {
    if let Ok(Note::On(note)) = text.parse() {
        return Some(note);
    }
    let mut chars = text.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let rest = chars.as_str();
    let (mark, octave) = match rest.strip_prefix('#') {
        Some(octave) => ('#', octave),
        None => ('-', rest.strip_prefix('-').unwrap_or(rest)),
    };
    match format!("{letter}{mark}{octave}").parse() {
        Ok(Note::On(note)) => Some(note),
        _ => None,
    }
}

/// The notes a Note column steps through: every one in range, or only those in the scale while
/// it's on, so a step along it is a degree. Held in place, so a render makes one without
/// allocating.
struct Ladder {
    scale: Scale,
    held: [u8; MAX_NOTE as usize + 1],
    len: usize,
}

impl Ladder {
    fn new(scale: Scale) -> Self {
        let mut ladder = Self {
            scale,
            held: [0; MAX_NOTE as usize + 1],
            len: 0,
        };
        for note in (MIN_NOTE..=MAX_NOTE).filter(|note| !scale.on || scale.contains(*note)) {
            ladder.held[ladder.len] = note;
            ladder.len += 1;
        }
        ladder
    }

    /// Its notes, from the lowest.
    fn notes(&self) -> &[u8] {
        &self.held[..self.len]
    }

    /// Its last step.
    fn top(&self) -> f64 {
        (self.len - 1) as f64
    }

    /// The step a note given as an end is on: the nearest note in the scale's, the lower if two
    /// are as near.
    fn step(&self, note: u8) -> u8 {
        let note = if self.scale.on {
            self.scale.fitted(note)
        } else {
            note
        };
        self.position(note).round() as u8
    }

    /// Where a note falls along it: on a step, or between two for a note outside the scale, so
    /// moving it and back finds the same note.
    fn position(&self, note: u8) -> f64 {
        match self.notes().binary_search(&note) {
            Ok(step) => step as f64,
            Err(0) => 0.0,
            Err(above) if above == self.len => self.top(),
            Err(above) => {
                let (below, over) = (self.held[above - 1], self.held[above]);
                (above - 1) as f64 + f64::from(note - below) / f64::from(over - below)
            }
        }
    }

    /// The note at a position along it.
    fn note(&self, position: f64) -> u8 {
        let position = position.clamp(0.0, self.top());
        let step = position.floor() as usize;
        let between = position - step as f64;
        match self.notes().get(step + 1) {
            Some(over) if between > 1e-9 => {
                let below = self.held[step];
                below + (f64::from(over - below) * between).round() as u8
            }
            _ => self.held[step],
        }
    }
}

/// The values a `List` fill goes through, up to `MAX_VALUES` of them. Saved as a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "Vec<u8>", into = "Vec<u8>")]
pub struct Values {
    values: [u8; MAX_VALUES],
    len: u8,
}

impl Values {
    /// The first `MAX_VALUES` of these values.
    pub fn new(values: &[u8]) -> Self {
        let mut list = Self::default();
        for (slot, value) in list.values.iter_mut().zip(values) {
            *slot = *value;
            list.len += 1;
        }
        list
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.values[..usize::from(self.len)]
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The same values the other way round.
    pub fn reversed(&self) -> Self {
        let mut list = *self;
        list.values[..usize::from(self.len)].reverse();
        list
    }
}

impl From<Vec<u8>> for Values {
    fn from(values: Vec<u8>) -> Self {
        Self::new(&values)
    }
}

impl From<Values> for Vec<u8> {
    fn from(values: Values) -> Self {
        values.as_slice().to_vec()
    }
}

/// The values in hex with a space between each, as they're typed.
impl std::fmt::Display for Values {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, value) in self.as_slice().iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{value:02X}")?;
        }
        Ok(())
    }
}

/// How to fill a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shape {
    pub from: u8,
    pub to: u8,
    /// Bends the line: above 0 it moves early, and below 0 late.
    pub curve: i8,
    /// Eases both ends: above 0 an S, and below 0 the reverse.
    pub s: i8,
    pub kind: FillKind,
    /// The wave a `Wave` follows, its length, and where in the cycle the first row selected falls,
    /// in degrees, as an LFO's are.
    pub wave: Wave,
    pub rate: WaveRate,
    pub phase: u16,
    /// How many of the rows it fills in a row take each value: `Alternate` and `List` give each
    /// value that many, and a ramp, wave or random fill holds a value for that many.
    pub run: u16,
    /// What a `List` goes through, in place of `from` and `to`.
    pub values: Values,
    pub place: Where,
    /// Picks the random values and the chance rows, so a change elsewhere doesn't reroll them.
    pub seed: u64,
    /// What an `Acid` fill writes besides its pitches. Saved only once it's changed.
    #[serde(default, skip_serializing_if = "Acid::is_default")]
    pub acid: Acid,
    /// What a `Follow` fill reads and how it turns it. Saved only once it's changed.
    #[serde(default, skip_serializing_if = "Follow::is_default")]
    pub follow: Follow,
    /// How far a `Walk` moves and what it does at the ends. Saved only once it's changed.
    #[serde(default, skip_serializing_if = "Walk::is_default")]
    pub walk: Walk,
    /// How many values a `1/f` fill sums. Saved only once it's changed.
    #[serde(default, skip_serializing_if = "Pink::is_default")]
    pub pink: Pink,
    /// What `Multiply` multiplies by, and what the bit transforms flip, keep and turn by. Saved
    /// only once it's changed.
    #[serde(default, skip_serializing_if = "Bits::is_default")]
    pub bits: Bits,
    /// Where a `Grids` fill reads its map, and the part it plays. Saved only once it's changed.
    #[serde(default, skip_serializing_if = "Grids::is_default")]
    pub grids: Grids,
    /// How many passes each of its variations lasts, a new seed each time: `0` for none, the
    /// seed every pass, as written. Only a shape that [draws](Shape::draws) varies. Saved only
    /// once it's set.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub vary: u8,
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

/// The choices of `Vary`, as passes each variation lasts: none, then a new one each pass, every
/// two, four or eight.
pub const VARIES: [u8; 5] = [0, 1, 2, 4, 8];

/// Mixed into a pass's seed, so it isn't any track's (`track_seed`).
const PASS_KEY: u64 = 0x7A55_0000;

impl Shape {
    /// Whether anything it writes is drawn from its seed, so another seed would write something
    /// else: a random fill, a walk, `1/f`, `Acid`, `Shuffle`, `Grids`' chaos or a random wave, or
    /// rows chosen by chance.
    pub fn draws(&self) -> bool {
        let random_wave =
            self.kind == FillKind::Wave && matches!(self.wave, Wave::Random | Wave::Smooth);
        random_wave
            || matches!(self.place, Where::Chance(_)) && self.kind.has_where()
            || matches!(
                self.kind,
                FillKind::Random
                    | FillKind::Walk
                    | FillKind::Pink
                    | FillKind::Acid
                    | FillKind::Shuffle
                    | FillKind::Grids
            )
    }

    /// Whether it plays a variation of its own on some passes, so the engine renders it.
    pub fn varies(&self) -> bool {
        self.vary > 0 && self.draws()
    }

    /// The shape a pass of its phrase plays, counted from 0: itself unless it [varies], and
    /// then with each `vary` passes a seed of their own, mixed from its seed and which of them
    /// it is, as `r` would roll it (with `Acid`'s pitches kept, only the second die). The first
    /// is the seed as written, so what's written is what the first pass plays, and since it's
    /// worked out from the pass alone, a render repeats and playing from anywhere hears what
    /// playing to there would.
    ///
    /// [varies]: Shape::varies
    pub fn on_pass(&self, pass: i64) -> Shape {
        if !self.varies() {
            return *self;
        }
        let which = pass.div_euclid(i64::from(self.vary));
        if which == 0 {
            return *self;
        }
        // Mixed rather than added, as `track_seed` is, so the next pass isn't the rows moved on.
        let new = |seed: u64| seed ^ mix(PASS_KEY, which as usize);
        let mut shape = *self;
        if shape.kind == FillKind::Acid && shape.acid.keep {
            shape.acid.roll = new(shape.acid.roll);
        } else {
            shape.seed = new(shape.seed);
        }
        shape
    }
}

/// Bias: bends `t` toward 1 for `a` above 0.5.
fn bias(t: f64, a: f64) -> f64 {
    t / ((1.0 / a - 2.0) * (1.0 - t) + 1.0)
}

/// Schlick's gain built from bias: an S for `g` below 0.5.
fn gain(t: f64, g: f64) -> f64 {
    if t < 0.5 {
        bias(2.0 * t, g) / 2.0
    } else {
        1.0 - bias(2.0 - 2.0 * t, g) / 2.0
    }
}

/// Where a ramp is at `t` from 0 to 1. It always starts at 0 and ends at 1, and 0 and 0 is a line.
pub fn curved(t: f64, curve: i8, s: i8) -> f64 {
    // Just short of the limits, where these curves stop moving at all.
    let amount = |bend: i8| (f64::from(bend) / 200.0).clamp(-0.495, 0.495);
    gain(bias(t, 0.5 + amount(curve)), 0.5 - amount(s))
}

/// A stateless number for a row, so a row's result doesn't depend on any other's.
fn mix(seed: u64, row: usize) -> u64 {
    let mut mixed = seed
        .wrapping_add((row as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add(0x9E37_79B9_7F4A_7C15);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

/// The seed a shape uses on the `nth` of the tracks it shapes at once, counted from 0, so each
/// track's random values, chance rows and shuffle are its own, and the first keeps the seed as it
/// is.
pub fn track_seed(seed: u64, nth: usize) -> u64 {
    // Mixed rather than added, as a seed one step on would only move the rows along.
    if nth == 0 {
        seed
    } else {
        seed ^ mix(0xC0FF_EE00, nth)
    }
}

/// The rows that get a value.
pub fn targets(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    place: Where,
    seed: u64,
) -> Vec<usize> {
    let len = range.len().min(MAX_PHRASE_STEPS);
    render::chosen(steps, range, column, place, seed, len)
        .ones()
        .map(|offset| range.first + offset)
        .collect()
}

/// Whether a column has nothing on these rows to reshape: no note, or no FX command in its slot,
/// so a shape is generated there rather than reshaping what's there. A note's velocity, nudge
/// and length always have their notes.
pub fn is_empty(steps: &[Step], range: StepRange, column: ShapeColumn) -> bool {
    let mut rows = (range.first..=range.last).filter_map(|row| steps.get(row));
    match column {
        ShapeColumn::Note { .. } => !rows.any(|step| matches!(step.note, Note::On(_))),
        ShapeColumn::Fx { slot, .. } => {
            !rows.any(|step| step.fx.get(slot).is_some_and(|fx| fx.command.is_some()))
        }
        _ => false,
    }
}

/// How many rows of a range a column could change, which a Euclidean `Where` spreads its hits
/// over.
pub fn open_rows(steps: &[Step], range: StepRange, column: ShapeColumn) -> usize {
    (range.first..=range.last)
        .filter(|row| steps.get(*row).is_some_and(|step| column.can_take(step)))
        .count()
}

/// The rows a shape changes, each with its step as the shape leaves it: a value of its column, or
/// for `Acid`, a note with its velocity and gate. `Follow` needs what it reads, so it writes
/// nothing here: see [`written_from`].
pub fn written(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
) -> Vec<(usize, Step)> {
    written_from(steps, range, column, shape, None)
}

/// The same, given the loop's steps of the phrase a `Follow` reads, if it has one: the cells
/// [`render_into`] leaves, laid over copies of their steps.
pub fn written_from(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    source: Option<&[Step]>,
) -> Vec<(usize, Step)> {
    let mut cells = vec![GenCell::Under; range.len().min(MAX_PHRASE_STEPS)];
    render_into(steps, range, column, shape, source, &mut cells);
    cells
        .into_iter()
        .enumerate()
        .filter(|(_, cell)| *cell != GenCell::Under)
        .filter_map(|(offset, cell)| {
            let row = range.first + offset;
            let mut step = steps.get(row)?.clone();
            cell.apply(column, &mut step);
            Some((row, step))
        })
        .collect()
}

/// The values a shape writes, by row. A ramp's ends are the first and last rows it fills, and rows
/// keep their spacing along it.
pub fn values(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
) -> Vec<(usize, u8)> {
    shaped(steps, range, column, shape)
        .into_iter()
        .map(|(row, value)| (row, value.whole))
        .collect()
}

/// The same with each value's fine part, where its column has one, so a ramp between two values
/// a step apart still moves on every row. A column without one gets the whole values.
pub fn exact_values(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
) -> Vec<(usize, u16)> {
    shaped(steps, range, column, shape)
        .into_iter()
        .map(|(row, value)| (row, render::exact(column, value)))
        .collect()
}

/// Each row's value as the render writes it, from the first row.
fn shaped(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
) -> Vec<(usize, render::Shaped)> {
    let mut cells = vec![GenCell::Under; range.len().min(MAX_PHRASE_STEPS)];
    let mut found = Vec::new();
    render::render(
        steps,
        range,
        column,
        shape,
        None,
        &mut cells,
        |row, value| {
            found.push((row, *value));
        },
    );
    found.sort_by_key(|(row, _)| *row);
    found
}

/// Where a shape starts and ends unless told otherwise: the first and last values in the selection,
/// or if those match, the lowest and highest, or if those match, the column's whole range. For
/// notes, that's an octave up from the one note there.
pub fn default_ends(steps: &[Step], range: StepRange, column: ShapeColumn) -> (u8, u8) {
    let present: Vec<u8> = (range.first..=range.last)
        .filter_map(|row| steps.get(row).and_then(|step| column.value(step)))
        .collect();
    let whole_range = || match column {
        ShapeColumn::Note { .. } => {
            let low = present
                .first()
                .copied()
                .unwrap_or(MIDDLE_C)
                .min(MAX_NOTE - 12);
            (low, low + 12)
        }
        _ => (0, column.highest()),
    };
    let (Some(&first), Some(&last)) = (present.first(), present.last()) else {
        return whole_range();
    };
    if first != last {
        return (first, last);
    }
    let lowest = present.iter().copied().min().unwrap_or(first);
    let highest = present.iter().copied().max().unwrap_or(first);
    if lowest != highest {
        (lowest, highest)
    } else {
        whole_range()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_a_value_keeps_a_mark_already_there_and_otherwise_locks() {
        let column = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::RES,
        };
        let mut step: Step = "--- -- cut:40h".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- res:20h");
        let mut step: Step = "--- -- ---".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- res:20");
        let mut step: Step = "--- -- del:10".parse().unwrap();
        column.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "--- -- res:20");
    }

    const VEL: ShapeColumn = ShapeColumn::Velocity;
    const CUT: ShapeColumn = ShapeColumn::Fx {
        slot: 0,
        command: FxCommand::CUT,
    };

    fn rows(steps: &[&str]) -> Vec<Step> {
        steps.iter().map(|text| text.parse().unwrap()).collect()
    }

    fn all(steps: &[Step]) -> StepRange {
        StepRange::between(0, steps.len() - 1)
    }

    #[test]
    fn a_ramp_on_a_setting_fills_its_fine_part() {
        let steps: Vec<Step> = (0..4)
            .map(|_| "C-3 -- cut:40 --- ---".parse().unwrap())
            .collect();
        let column = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::CUT,
        };
        let out: Vec<u16> = exact_values(&steps, all(&steps), column, &ramp(0x40, 0x41, 0, 0))
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        assert_eq!(out, [0x4000, 0x4055, 0x40AB, 0x4100]);
        let events = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::Del,
        };
        let steps: Vec<Step> = (0..4)
            .map(|_| "C-3 -- del:40 --- ---".parse().unwrap())
            .collect();
        let whole: Vec<u16> = exact_values(&steps, all(&steps), events, &ramp(0x40, 0x41, 0, 0))
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        assert!(whole.iter().all(|value| value & 0xFF == 0), "{whole:X?}");
    }

    fn ramp(from: u8, to: u8, curve: i8, s: i8) -> Shape {
        Shape {
            from,
            to,
            curve,
            s,
            kind: FillKind::Ramp,
            wave: Wave::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::Notes,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: Grids::default(),
            vary: 0,
        }
    }

    fn notes(count: usize) -> Vec<Step> {
        rows(&vec!["C-3 -- --- --- ---"; count])
    }

    fn written(steps: &[Step], shape: Shape) -> Vec<u8> {
        values(steps, all(steps), VEL, &shape)
            .iter()
            .map(|(_, value)| *value)
            .collect()
    }

    #[test]
    fn a_curve_reaches_the_values_the_design_gives() {
        // Over 9 steps from 00 to 7F. Curve +60 is bias 0.8, and S +60 is gain 0.2.
        assert_eq!(
            written(&notes(9), ramp(0, 0x7F, 60, 0)),
            [0x00, 0x2E, 0x49, 0x5A, 0x66, 0x6E, 0x75, 0x7B, 0x7F]
        );
        assert_eq!(
            written(&notes(9), ramp(0, 0x7F, 0, 60)),
            [0x00, 0x05, 0x0D, 0x1B, 0x40, 0x64, 0x72, 0x7A, 0x7F]
        );
    }

    #[test]
    fn with_no_bend_it_is_a_straight_line() {
        assert_eq!(
            written(&notes(5), ramp(0x10, 0x50, 0, 0)),
            [0x10, 0x20, 0x30, 0x40, 0x50]
        );
    }

    #[test]
    fn the_ends_are_exact_and_the_ramp_never_turns_back() {
        for curve in [-100, -60, -1, 0, 1, 60, 100] {
            for s in [-100, -60, 0, 60, 100] {
                for (from, to) in [(0, 0x7F), (0x7F, 0), (0x10, 0x11), (0x40, 0x40)] {
                    let out = written(&notes(33), ramp(from, to, curve, s));
                    assert_eq!(out[0], from, "{curve} {s}");
                    assert_eq!(out[32], to, "{curve} {s}");
                    let rising = out.windows(2).all(|pair| pair[0] <= pair[1]);
                    let falling = out.windows(2).all(|pair| pair[0] >= pair[1]);
                    assert!(rising || falling, "{curve} {s} {out:?}");
                }
            }
        }
    }

    #[test]
    fn falling_mirrors_rising() {
        let rising = written(&notes(9), ramp(0, 0x7F, 0, 0));
        let falling = written(&notes(9), ramp(0x7F, 0, 0, 0));
        let mirrored: Vec<u8> = rising.iter().map(|value| 0x7F - value).collect();
        assert_eq!(falling, mirrored);
    }

    #[test]
    fn rows_keep_their_spacing_along_a_ramp() {
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
        ]);
        let out = values(&steps, all(&steps), VEL, &ramp(0, 0x40, 0, 0));
        assert_eq!(out, [(0, 0x00), (1, 0x10), (4, 0x40)]);
    }

    #[test]
    fn the_ends_are_the_first_and_last_rows_filled() {
        let steps = rows(&[
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
        ]);
        let out = values(&steps, all(&steps), VEL, &ramp(0x10, 0x20, 0, 0));
        assert_eq!(out, [(1, 0x10), (2, 0x20)]);
    }

    #[test]
    fn velocity_only_ever_goes_on_notes() {
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
        ]);
        let picked = |place| targets(&steps, all(&steps), VEL, place, 1);
        assert_eq!(picked(Where::Notes), [0, 2, 4]);
        assert_eq!(picked(Where::Every(2)), [0, 2, 4]);
        assert_eq!(picked(Where::Every(4)), [0, 4]);
    }

    #[test]
    fn fx_rows_can_be_narrowed_and_other_commands_are_kept() {
        let steps = rows(&[
            "C-3 -- cut:10 --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- res:10 --- ---",
            "--- -- cut:30 --- ---",
        ]);
        let picked = |place| targets(&steps, all(&steps), CUT, place, 1);
        assert_eq!(picked(Where::Commands), [0, 4]);
        assert_eq!(picked(Where::AllRows), [0, 1, 2, 4], "RES stays");
        assert_eq!(picked(Where::Notes), [0, 2]);
        assert_eq!(picked(Where::NoNotes), [1, 4]);
        assert_eq!(picked(Where::Every(2)), [0, 2, 4]);
    }

    #[test]
    fn chance_is_about_right_and_stays_put_for_a_seed() {
        let steps = notes(1000);
        let picked =
            |percent, seed| targets(&steps, all(&steps), VEL, Where::Chance(percent), seed);
        let half = picked(50, 1);
        assert!((400..600).contains(&half.len()), "{}", half.len());
        assert_eq!(half, picked(50, 1));
        assert_ne!(half, picked(50, 2));
        assert_eq!(picked(0, 1).len(), 0);
        assert_eq!(picked(100, 1).len(), 1000);
    }

    /// A pattern as `x` and `.`.
    fn drawn(pattern: &[bool]) -> String {
        pattern
            .iter()
            .map(|hit| if *hit { 'x' } else { '.' })
            .collect()
    }

    #[test]
    fn euclid_gives_toussaints_patterns() {
        assert_eq!(drawn(&euclid(3, 8)), "x..x..x.");
        assert_eq!(drawn(&euclid(5, 8)), "x.xx.xx.");
        assert_eq!(drawn(&euclid(5, 16)), "x..x..x..x..x...");
        assert_eq!(drawn(&euclid(4, 12)), "x..x..x..x..");
        assert_eq!(drawn(&euclid(7, 16)), "x..x.x.x..x.x.x.");
        assert_eq!(drawn(&euclid(0, 4)), "....");
        assert_eq!(drawn(&euclid(4, 4)), "xxxx");
        assert_eq!(drawn(&euclid(9, 4)), "xxxx");
        assert!(euclid(3, 0).is_empty());
    }

    #[test]
    fn a_euclidean_where_rotates_later_and_goes_round() {
        let steps = notes(8);
        let picked = |hits, rotate| {
            let rows = targets(&steps, all(&steps), VEL, Where::Euclid { hits, rotate }, 1);
            let mut pattern = vec![false; 8];
            for row in rows {
                pattern[row] = true;
            }
            drawn(&pattern)
        };
        assert_eq!(picked(3, 0), "x..x..x.");
        assert_eq!(picked(3, 1), ".x..x..x");
        assert_eq!(picked(3, 2), "x.x..x..");
        assert_eq!(picked(3, 8), "x..x..x.", "a whole turn");
        assert_eq!(picked(5, 0), "x.xx.xx.");
        assert_eq!(picked(5, 3), "xx.x.xx.");
    }

    #[test]
    fn a_euclidean_where_counts_among_the_notes() {
        // Notes on 6 of 16 rows: 3 hits among them fall on every other note.
        let mut steps = rows(&vec!["--- -- --- --- ---"; 16]);
        for row in [0, 3, 4, 8, 11, 14] {
            steps[row] = "C-3 -- --- --- ---".parse().unwrap();
        }
        let place = Where::Euclid { hits: 3, rotate: 0 };
        assert_eq!(targets(&steps, all(&steps), VEL, place, 1), [0, 4, 11]);
        assert_eq!(open_rows(&steps, all(&steps), VEL), 6);
        // On an FX column, among the rows that could hold the command: not the one with another.
        let steps = rows(&[
            "--- -- --- --- ---",
            "--- -- res:10 --- ---",
            "--- -- --- --- ---",
            "--- -- --- --- ---",
            "--- -- --- --- ---",
        ]);
        let place = Where::Euclid { hits: 2, rotate: 0 };
        assert_eq!(targets(&steps, all(&steps), CUT, place, 1), [0, 3]);
    }

    #[test]
    fn random_stays_in_range_either_way_round_and_reaches_both_ends() {
        let steps = notes(500);
        for (from, to) in [(0x10, 0x20), (0x20, 0x10)] {
            let shape = Shape {
                kind: FillKind::Random,
                ..ramp(from, to, 0, 0)
            };
            let out = written(&steps, shape);
            assert!(out.iter().all(|value| (0x10..=0x20).contains(value)));
            assert!(out.contains(&0x10) && out.contains(&0x20));
            assert_eq!(out, written(&steps, shape));
            assert_ne!(out, written(&steps, Shape { seed: 2, ..shape }));
        }
    }

    fn waving(wave: Wave, from: u8, to: u8) -> Shape {
        Shape {
            kind: FillKind::Wave,
            wave,
            ..ramp(from, to, 0, 0)
        }
    }

    #[test]
    fn a_wave_swings_between_the_values_at_its_rate() {
        // A half note is 8 rows.
        assert_eq!(
            written(&notes(9), waving(Wave::Tri, 0, 0x40)),
            [0x20, 0x30, 0x40, 0x30, 0x20, 0x10, 0, 0x10, 0x20]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Sine, 0, 0x40)),
            [0x20, 0x37, 0x40, 0x37, 0x20]
        );
        assert_eq!(
            written(&notes(8), waving(Wave::Square, 0x10, 0x70)),
            [0x70, 0x70, 0x70, 0x70, 0x10, 0x10, 0x10, 0x10]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Ramp, 0, 0x40)),
            [0, 0x08, 0x10, 0x18, 0x20]
        );
        assert_eq!(
            written(&notes(5), waving(Wave::Saw, 0, 0x40)),
            [0x40, 0x38, 0x30, 0x28, 0x20]
        );
    }

    #[test]
    fn a_waves_rate_and_phase_move_it_and_swapped_ends_flip_it() {
        let quarter = Shape {
            rate: "1/4".parse().unwrap(),
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(
            written(&notes(5), quarter),
            [0x20, 0x40, 0x20, 0, 0x20],
            "a quarter note is 4 rows"
        );
        let steps = Shape {
            rate: "2st".parse().unwrap(),
            ..waving(Wave::Square, 0, 0x40)
        };
        assert_eq!(written(&notes(4), steps), [0x40, 0, 0x40, 0]);
        let phased = Shape {
            phase: 90,
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(written(&notes(3), phased), [0x40, 0x30, 0x20]);
        assert_eq!(
            written(&notes(3), waving(Wave::Tri, 0x40, 0)),
            [0x20, 0x10, 0]
        );
    }

    #[test]
    fn a_wave_goes_by_where_a_row_falls_counting_from_the_first_selected() {
        let steps = rows(&[
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "C-3 -- --- --- ---",
        ]);
        let out = values(&steps, all(&steps), VEL, &waving(Wave::Tri, 0, 0x40));
        assert_eq!(out, [(1, 0x30), (3, 0x30), (4, 0x20)]);
        let later = StepRange::between(1, 4);
        let out = values(&steps, later, VEL, &waving(Wave::Tri, 0, 0x40));
        assert_eq!(out, [(1, 0x20), (3, 0x40), (4, 0x30)]);
    }

    #[test]
    fn a_run_holds_a_ramp_a_wave_and_a_random_fill_in_stairs() {
        let stairs = Shape {
            run: 2,
            ..ramp(0, 0x30, 0, 0)
        };
        assert_eq!(
            written(&notes(8), stairs),
            [0, 0, 0x10, 0x10, 0x20, 0x20, 0x30, 0x30]
        );
        // An odd count ends on a short stair, which still reaches To.
        let stairs = Shape {
            run: 3,
            ..ramp(0, 0x30, 0, 0)
        };
        assert_eq!(
            written(&notes(7), stairs),
            [0, 0, 0, 0x18, 0x18, 0x18, 0x30]
        );
        // A wave is sampled on each run's first row and held.
        let held = Shape {
            run: 2,
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(
            written(&notes(8), held),
            [0x20, 0x20, 0x40, 0x40, 0x20, 0x20, 0, 0]
        );
        let random = Shape {
            kind: FillKind::Random,
            run: 4,
            ..ramp(0, 0x7F, 0, 0)
        };
        let out = written(&notes(64), random);
        assert!(out.chunks(4).all(|run| run.iter().all(|v| *v == run[0])));
        assert!(out.chunks(4).any(|run| run[0] != out[0]));
    }

    #[test]
    fn a_rate_in_cycles_fits_that_many_into_the_selection() {
        let twice = Shape {
            rate: WaveRate::Cycles(2),
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(
            written(&notes(16), twice),
            [
                0x20, 0x30, 0x40, 0x30, 0x20, 0x10, 0, 0x10, 0x20, 0x30, 0x40, 0x30, 0x20, 0x10, 0,
                0x10
            ]
        );
        // Across the rows selected, not only those filled.
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
        ]);
        let once = Shape {
            rate: WaveRate::Cycles(1),
            ..waving(Wave::Tri, 0, 0x40)
        };
        assert_eq!(
            values(&steps, all(&steps), VEL, &once),
            [(0, 0x20), (2, 0x20)]
        );
        assert_eq!("x3".parse(), Ok(WaveRate::Cycles(3)));
        assert_eq!("1/4".parse(), Ok(WaveRate::Time("1/4".parse().unwrap())));
        assert!("x0".parse::<WaveRate>().is_err());
        assert!("2hz".parse::<WaveRate>().is_err());
        assert_eq!(WaveRate::Cycles(3).to_string(), "x3");
        assert_eq!(WaveRate::Cycles(1).nudged(false), WaveRate::Cycles(1));
        assert_eq!(WaveRate::Cycles(3).jumped(true), WaveRate::Cycles(7));
    }

    #[test]
    fn a_smooth_wave_wanders_the_same_way_for_a_seed() {
        let smooth = |seed| Shape {
            seed,
            rate: "1/4".parse().unwrap(),
            ..waving(Wave::Smooth, 0, 0x7F)
        };
        let out = written(&notes(64), smooth(1));
        assert_eq!(out, written(&notes(64), smooth(1)));
        assert_ne!(out, written(&notes(64), smooth(2)));
        // It eases from one value to the next, so neighbours stay close.
        assert!(
            out.windows(2).all(|pair| pair[0].abs_diff(pair[1]) < 0x40),
            "{out:X?}"
        );
    }

    fn alternating(run: u16, from: u8, to: u8) -> Shape {
        Shape {
            kind: FillKind::Alternate,
            run,
            ..ramp(from, to, 0, 0)
        }
    }

    #[test]
    fn alternate_takes_turns_for_its_run() {
        assert_eq!(
            written(&notes(5), alternating(1, 0x70, 0x40)),
            [0x70, 0x40, 0x70, 0x40, 0x70]
        );
        assert_eq!(
            written(&notes(6), alternating(2, 0x70, 0x40)),
            [0x70, 0x70, 0x40, 0x40, 0x70, 0x70]
        );
    }

    #[test]
    fn alternate_flips_on_every_note_however_they_are_spaced() {
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
        ]);
        let out = values(&steps, all(&steps), VEL, &alternating(1, 0x70, 0x40));
        assert_eq!(out, [(0, 0x70), (3, 0x40), (4, 0x70), (6, 0x40)]);
    }

    fn listing(run: u16, list: &[u8]) -> Shape {
        Shape {
            kind: FillKind::List,
            run,
            values: Values::new(list),
            ..ramp(0, 0x7F, 0, 0)
        }
    }

    #[test]
    fn a_list_goes_round_its_values_for_its_run() {
        assert_eq!(
            written(&notes(7), listing(1, &[0x70, 0x40, 0x55])),
            [0x70, 0x40, 0x55, 0x70, 0x40, 0x55, 0x70]
        );
        assert_eq!(
            written(&notes(7), listing(2, &[0x70, 0x40, 0x55])),
            [0x70, 0x70, 0x40, 0x40, 0x55, 0x55, 0x70]
        );
        assert_eq!(written(&notes(3), listing(1, &[0x30])), [0x30; 3]);
    }

    #[test]
    fn a_list_counts_only_the_rows_it_fills_and_stops_at_the_columns_top() {
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- --- --- ---",
        ]);
        let out = values(&steps, all(&steps), VEL, &listing(1, &[0x70, 0xFF, 0x20]));
        assert_eq!(out, [(0, 0x70), (2, 0x7F), (3, 0x20), (5, 0x70)]);
    }

    #[test]
    fn a_list_keeps_at_most_eight_values_and_shows_them_in_hex() {
        let list = Values::new(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(list.as_slice(), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Values::new(&[0x70, 0x40, 0x5]).to_string(), "70 40 05");
        assert_eq!(
            Values::new(&[0x70, 0x40, 0x5]).reversed().as_slice(),
            [0x5, 0x40, 0x70]
        );
        assert!(Values::default().is_empty());
        assert!(written(&notes(2), listing(1, &[])).iter().all(|v| *v == 0));
    }

    #[test]
    fn a_wave_and_alternate_stop_at_the_columns_top() {
        let steps = notes(4);
        assert_eq!(written(&steps, alternating(1, 0xFF, 0)), [0x7F, 0, 0x7F, 0]);
        let out = written(&notes(3), waving(Wave::Tri, 0, 0xFF));
        assert!(out.iter().all(|value| *value <= 0x7F), "{out:?}");
    }

    #[test]
    fn values_stop_at_the_columns_top() {
        let steps = notes(2);
        assert_eq!(written(&steps, ramp(0, 0xFF, 0, 0)), [0, 0x7F]);
        let vol = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::VOL,
        };
        let shape = Shape {
            place: Where::AllRows,
            ..ramp(0, 0xFF, 0, 0)
        };
        let out = values(&steps, all(&steps), vol, &shape);
        assert_eq!(out[1].1, 0xF8);
    }

    fn reworking(kind: FillKind, from: u8, to: u8) -> Shape {
        Shape {
            kind,
            ..ramp(from, to, 0, 0)
        }
    }

    fn on_cut(values: &[u8]) -> Vec<Step> {
        values
            .iter()
            .map(|value| format!("C-3 -- cut:{value:02X} --- ---").parse().unwrap())
            .collect()
    }

    fn cut_values(steps: &[Step], shape: Shape) -> Vec<u8> {
        let shape = Shape {
            place: Where::Commands,
            ..shape
        };
        values(steps, all(steps), CUT, &shape)
            .iter()
            .map(|(_, value)| *value)
            .collect()
    }

    #[test]
    fn scale_maps_the_values_own_range_onto_the_ends_along_the_curve() {
        let steps = on_cut(&[0x20, 0x40, 0x60]);
        let scaled = |from, to, curve| {
            cut_values(
                &steps,
                Shape {
                    curve,
                    ..reworking(FillKind::Scale, from, to)
                },
            )
        };
        assert_eq!(scaled(0, 0xFF, 0), [0x00, 0x80, 0xFF]);
        assert_eq!(scaled(0x40, 0x60, 0), [0x40, 0x50, 0x60]);
        assert_eq!(scaled(0xFF, 0, 0), [0xFF, 0x7F, 0x00], "upside down");
        let bent = scaled(0, 0xFF, 60);
        assert_eq!((bent[0], bent[2]), (0, 0xFF));
        assert!(bent[1] > 0x80, "{bent:X?}");
        // All one value has no range, so it goes to From.
        assert_eq!(
            cut_values(&on_cut(&[0x30; 3]), reworking(FillKind::Scale, 0x10, 0x70)),
            [0x10; 3]
        );
    }

    #[test]
    fn invert_flips_each_value_within_their_range() {
        let steps = on_cut(&[0x10, 0x30, 0x70]);
        assert_eq!(
            cut_values(&steps, reworking(FillKind::Invert, 0, 0)),
            [0x70, 0x50, 0x10]
        );
        // A fine part is flipped too.
        let steps: Vec<Step> = ["cut:1080", "cut:2000", "cut:3000"]
            .iter()
            .map(|fx| format!("C-3 -- {fx} --- ---").parse().unwrap())
            .collect();
        let shape = Shape {
            place: Where::Commands,
            ..reworking(FillKind::Invert, 0, 0)
        };
        let out: Vec<u16> = exact_values(&steps, all(&steps), CUT, &shape)
            .into_iter()
            .map(|(_, value)| value)
            .collect();
        assert_eq!(out, [0x3000, 0x2080, 0x1080]);
    }

    #[test]
    fn shuffle_deals_the_same_values_again_from_its_seed() {
        let list: Vec<u8> = (0..16).map(|value| value * 8).collect();
        let steps = on_cut(&list);
        let dealt = |seed| {
            cut_values(
                &steps,
                Shape {
                    seed,
                    ..reworking(FillKind::Shuffle, 0, 0)
                },
            )
        };
        let first = dealt(1);
        assert_ne!(first, list);
        assert_eq!(first, dealt(1));
        assert_ne!(first, dealt(2));
        let mut sorted = first.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, list, "the same values");
    }

    #[test]
    fn reverse_gives_the_rows_their_values_the_other_way_and_keeps_to_where() {
        let steps = rows(&[
            "C-3 10 --- --- ---",
            "--- -- --- --- ---",
            "C-3 20 --- --- ---",
            "C-3 30 --- --- ---",
            "--- -- --- --- ---",
        ]);
        let out = values(
            &steps,
            all(&steps),
            VEL,
            &reworking(FillKind::Reverse, 0, 0),
        );
        assert_eq!(out, [(0, 0x30), (2, 0x20), (3, 0x10)]);
        // On an FX column, the rows chosen without the command keep nothing.
        let steps = rows(&[
            "C-3 -- cut:10 --- ---",
            "--- -- --- --- ---",
            "C-3 -- cut:20 --- ---",
            "--- -- cut:30 --- ---",
        ]);
        let shape = Shape {
            place: Where::Notes,
            ..reworking(FillKind::Reverse, 0, 0)
        };
        assert_eq!(
            values(&steps, all(&steps), CUT, &shape),
            [(0, 0x20), (2, 0x10)]
        );
        let shape = Shape {
            place: Where::AllRows,
            ..shape
        };
        assert_eq!(
            values(&steps, all(&steps), CUT, &shape),
            [(0, 0x30), (2, 0x20), (3, 0x10)]
        );
    }

    fn in_key(kind: crate::scale::ScaleKind, root: u8) -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale {
                on: true,
                root,
                kind,
            },
            writes: None,
        }
    }

    const CHROMATIC: ShapeColumn = ShapeColumn::Note {
        scale: Scale {
            on: false,
            root: 0,
            kind: crate::scale::ScaleKind::Major,
        },
        writes: None,
    };

    /// The notes written, by name.
    fn named(steps: &[Step], column: ShapeColumn, shape: Shape) -> Vec<String> {
        values(steps, all(steps), column, &shape)
            .iter()
            .map(|(_, note)| Note::On(*note).to_string())
            .collect()
    }

    const C3: u8 = MIDDLE_C;

    #[test]
    fn notes_of_its_own_take_their_rhythm_from_where_and_land_in_key() {
        let ShapeColumn::Note { scale, .. } = in_key(crate::scale::ScaleKind::Major, 0) else {
            unreachable!()
        };
        let writing = ShapeColumn::Note {
            scale,
            writes: Some(0x50),
        };
        assert_eq!(writing.default_where(), Where::AllRows);
        assert!(!writing.wheres().contains(&Where::Notes));
        assert!(writing.fills().iter().all(|kind| !kind.reworks()));
        let empty = rows(&["--- -- --- --- ---"; 8]);
        let rows_of = |place: Where| -> Vec<usize> {
            let shape = Shape {
                place,
                ..ramp(C3, C3 + 12, 0, 0)
            };
            values(&empty, all(&empty), writing, &shape)
                .iter()
                .map(|(row, _)| *row)
                .collect()
        };
        assert_eq!(rows_of(Where::AllRows), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(rows_of(Where::Every(4)), [0, 4]);
        assert_eq!(rows_of(Where::Euclid { hits: 3, rotate: 0 }), [0, 3, 6]);
        let all_rows = Shape {
            place: Where::AllRows,
            ..ramp(C3, C3 + 12, 0, 0)
        };
        assert_eq!(
            named(&empty, writing, all_rows),
            ["C-3", "D-3", "E-3", "F-3", "G-3", "A-3", "B-3", "C-4"]
        );
        // Each is written with its velocity, and an `OFF` is gone round.
        let mut steps = rows(&[
            "--- -- --- --- ---",
            "OFF -- --- --- ---",
            "--- -- --- --- ---",
        ]);
        let range = all(&steps);
        for (row, value) in exact_values(&steps, range, writing, &all_rows) {
            writing.write_exact(&mut steps[row], value);
        }
        let texts: Vec<String> = steps.iter().map(ToString::to_string).collect();
        assert_eq!(
            texts,
            [
                "C-3 50 --- --- ---",
                "OFF -- --- --- ---",
                "C-4 50 --- --- ---"
            ]
        );
    }

    #[test]
    fn a_ramp_of_notes_walks_the_scale_or_semitones_with_it_off() {
        let major = in_key(crate::scale::ScaleKind::Major, 0);
        assert_eq!(
            named(&notes(8), major, ramp(C3, C3 + 12, 0, 0)),
            ["C-3", "D-3", "E-3", "F-3", "G-3", "A-3", "B-3", "C-4"]
        );
        assert_eq!(
            named(&notes(8), CHROMATIC, ramp(C3, C3 + 12, 0, 0)),
            ["C-3", "D-3", "D#3", "F-3", "G-3", "A-3", "A#3", "C-4"]
        );
        // Down, and from an end outside the scale, which counts from the nearest degree.
        assert_eq!(
            named(&notes(3), major, ramp(C3 + 5, C3 + 1, 0, 0)),
            ["F-3", "D-3", "C-3"]
        );
    }

    #[test]
    fn random_notes_are_seeded_and_all_in_the_scale() {
        let minor = in_key(crate::scale::ScaleKind::Minor, 9);
        let ShapeColumn::Note { scale, .. } = minor else {
            unreachable!()
        };
        let random = |seed| Shape {
            seed,
            kind: FillKind::Random,
            ..ramp(C3, C3 + 24, 0, 0)
        };
        let out = values(&notes(200), all(&notes(200)), minor, &random(1));
        assert!(out.iter().all(|(_, note)| scale.contains(*note)));
        assert!(out.iter().all(|(_, note)| (C3..=C3 + 24).contains(note)));
        assert_eq!(
            out,
            values(&notes(200), all(&notes(200)), minor, &random(1))
        );
        assert_ne!(
            out,
            values(&notes(200), all(&notes(200)), minor, &random(2))
        );
    }

    #[test]
    fn a_wave_and_a_list_of_notes() {
        let major = in_key(crate::scale::ScaleKind::Major, 0);
        let wave = Shape {
            rate: "1/4".parse().unwrap(),
            ..waving(Wave::Tri, C3, C3 + 7)
        };
        // Four degrees either side of E, the middle of C to G.
        assert_eq!(named(&notes(4), major, wave), ["E-3", "G-3", "E-3", "C-3"]);
        // A motif is written as it's given, in the scale or not.
        let motif = Shape {
            values: Values::new(&[C3, C3 + 3, C3 + 7]),
            ..listing(1, &[])
        };
        assert_eq!(named(&notes(4), major, motif), ["C-3", "D#3", "G-3", "C-3"]);
    }

    #[test]
    fn re_pitching_keeps_the_rhythm_and_leaves_offs_and_rests() {
        let steps = rows(&[
            "C-3 40 --- --- ---",
            "--- -- --- --- ---",
            "OFF -- --- --- ---",
            "C-3 20 SGL2 40 --- --- ---",
        ]);
        let major = in_key(crate::scale::ScaleKind::Major, 0);
        let out = values(&steps, all(&steps), major, &ramp(C3, C3 + 2, 0, 0));
        assert_eq!(out, [(0, C3), (3, C3 + 2)]);
        let mut step = steps[3].clone();
        major.write(&mut step, C3 + 2);
        assert_eq!(step.to_string(), "D-3 20 SGL2 40 --- --- ---");
    }

    #[test]
    fn transforms_on_notes_invert_in_degrees_and_move_notes_as_they_are() {
        let major = in_key(crate::scale::ScaleKind::Major, 0);
        let line = |names: &[&str]| -> Vec<Step> {
            names
                .iter()
                .map(|name| format!("{name} -- --- --- ---").parse().unwrap())
                .collect()
        };
        let tune = line(&["C-3", "E-3", "D-3", "G-3"]);
        assert_eq!(
            named(&tune, major, reworking(FillKind::Invert, 0, 0)),
            ["G-3", "E-3", "F-3", "C-3"]
        );
        assert_eq!(
            named(&tune, CHROMATIC, reworking(FillKind::Invert, 0, 0)),
            ["G-3", "D#3", "F-3", "C-3"]
        );
        let chromatic = line(&["C-3", "C#3", "E-3"]);
        assert_eq!(
            named(&chromatic, major, reworking(FillKind::Reverse, 0, 0)),
            ["E-3", "C#3", "C-3"]
        );
        let shuffled = named(&tune, major, reworking(FillKind::Shuffle, 0, 0));
        let mut sorted = shuffled.clone();
        sorted.sort();
        assert_eq!(sorted, ["C-3", "D-3", "E-3", "G-3"]);
        // Scale lands on degrees between the two ends.
        assert_eq!(
            named(&tune, major, reworking(FillKind::Scale, C3, C3 + 12)),
            ["C-3", "G-3", "E-3", "C-4"]
        );
    }

    #[test]
    fn snap_moves_a_note_to_the_nearest_degree_and_the_lower_of_two() {
        let pentatonic = in_key(crate::scale::ScaleKind::MajorPentatonic, 0);
        let steps: Vec<Step> = ["C#3", "F-3", "B-3", "G-3", "F#3"]
            .iter()
            .map(|name| format!("{name} -- --- --- ---").parse().unwrap())
            .collect();
        let snap = reworking(FillKind::Snap, 0, 0);
        // C# is as near C as D, so it goes down; F is nearer E, B nearer C and F# nearer G.
        assert_eq!(
            named(&steps, pentatonic, snap),
            ["C-3", "E-3", "C-4", "G-3", "G-3"]
        );
        let names: Vec<String> = steps.iter().map(|step| step.note.to_string()).collect();
        assert_eq!(named(&steps, CHROMATIC, snap), names, "with the scale off");
        assert!(CHROMATIC.fills().contains(&FillKind::Snap));
        assert!(!VEL.fills().contains(&FillKind::Snap));
    }

    #[test]
    fn multiply_is_offered_where_the_rows_are_a_rhythm() {
        let fx = ShapeColumn::Fx {
            slot: 0,
            command: FxCommand::RES,
        };
        assert!(CHROMATIC.fills().contains(&FillKind::Multiply));
        assert!(fx.fills().contains(&FillKind::Multiply));
        for column in [VEL, ShapeColumn::Nudge { most: 11 }, ShapeColumn::Length] {
            assert!(!column.fills().contains(&FillKind::Multiply));
        }
    }

    #[test]
    fn a_nudge_and_a_length_fill_the_notes_and_keep_their_ranges() {
        let steps = rows(&[
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "C-3 -- +2 --- --- ---",
            "C-3 -- SGL1 40 --- --- ---",
        ]);
        let nudge = ShapeColumn::Nudge { most: 11 };
        let out = values(&steps, all(&steps), nudge, &ramp(8, 14, 0, 0));
        assert_eq!(out, [(0, 8), (2, 12), (3, 14)]);
        assert_eq!(nudge.show(8), "-3");
        assert_eq!(nudge.show(11), "0");
        assert_eq!(nudge.show(22), "+B");
        assert_eq!(nudge.parse("-3"), Some(8));
        assert_eq!(nudge.parse("+b"), Some(22));
        assert_eq!(nudge.parse("+C"), Some(22), "kept at B");
        assert_eq!(nudge.parse("x"), None);
        let mut step = steps[0].clone();
        nudge.write(&mut step, 8);
        assert_eq!(step.nudge, -3);
        assert_eq!(default_ends(&steps, all(&steps), nudge), (11, 13));

        // On a track at a quarter, a nudge goes to `2F` either way, shown with two digits.
        let slow = ShapeColumn::Nudge { most: 47 };
        assert_eq!(slow.highest(), 94);
        assert_eq!(slow.show(47 - 11), "-0B");
        assert_eq!(slow.show(94), "+2F");
        assert_eq!(slow.parse("-2f"), Some(0));
        assert_eq!(slow.parse("+30"), Some(94), "kept at 2F");
        assert_eq!(slow.parse("+B"), Some(58));
        let mut step = steps[2].clone();
        assert_eq!(
            slow.value(&step),
            Some(49),
            "+2 is the same time on every track"
        );
        slow.write(&mut step, 0);
        assert_eq!(step.nudge, -47);
        assert_eq!(nudge.value(&step), Some(0), "kept to a sixteenth's range");

        // A length fills every note, and a transform leaves `--` as it is.
        let length = ShapeColumn::Length;
        let out = values(&steps, all(&steps), length, &ramp(0x20, 0x80, 0, 0));
        assert_eq!(out, [(0, 0x20), (2, 0x60), (3, 0x80)]);
        let mut step = steps[0].clone();
        length.write(&mut step, 0x20);
        assert_eq!(step.to_string(), "C-3 -- SGL1 20 --- --- ---");
        let lengths = rows(&[
            "C-3 -- SGL1 20 --- --- ---",
            "C-3 -- --- --- ---",
            "C-3 -- SGL1 80 --- --- ---",
        ]);
        let out = values(
            &lengths,
            all(&lengths),
            length,
            &reworking(FillKind::Reverse, 0, 0),
        );
        assert_eq!(out, [(0, 0x80), (2, 0x20)]);
    }

    #[test]
    fn notes_are_shown_and_typed_by_name() {
        assert_eq!(CHROMATIC.show(61), "C#3");
        for (typed, note) in [
            ("C-3", 60),
            ("c3", 60),
            ("c-3", 60),
            ("c#3", 61),
            ("G-8", 127),
        ] {
            assert_eq!(CHROMATIC.parse(typed), Some(note), "{typed}");
        }
        for typed in ["H3", "e#3", "C-9", ""] {
            assert_eq!(CHROMATIC.parse(typed), None, "{typed}");
        }
        assert_eq!(
            CHROMATIC.show_list(&Values::new(&[60, 64, 67])),
            "C-3 E-3 G-3"
        );
        let one = rows(&["G-3 -- --- --- ---"; 2]);
        assert_eq!(default_ends(&one, all(&one), CHROMATIC), (67, 79));
    }

    #[test]
    fn default_ends_prefer_the_first_and_last_values() {
        let steps = rows(&[
            "C-3 50 --- --- ---",
            "C-3 7F --- --- ---",
            "C-3 20 --- --- ---",
        ]);
        assert_eq!(default_ends(&steps, all(&steps), VEL), (0x50, 0x20));

        let same_ends = rows(&[
            "C-3 40 --- --- ---",
            "C-3 7F --- --- ---",
            "C-3 40 --- --- ---",
        ]);
        assert_eq!(default_ends(&same_ends, all(&same_ends), VEL), (0x40, 0x7F));

        let one_value = rows(&["C-3 40 --- --- ---"; 3]);
        assert_eq!(default_ends(&one_value, all(&one_value), VEL), (0, 0x7F));

        let empty = rows(&["--- -- --- --- ---"; 2]);
        assert_eq!(default_ends(&empty, all(&empty), VEL), (0, 0x7F));
    }

    #[test]
    fn default_ends_count_a_note_with_no_velocity_as_full() {
        let steps = rows(&["C-3 -- --- --- ---", "C-3 20 --- --- ---"]);
        assert_eq!(default_ends(&steps, all(&steps), VEL), (0x7F, 0x20));
    }

    #[test]
    fn default_ends_for_fx_use_only_that_command() {
        let steps = rows(&[
            "--- -- res:70 --- ---",
            "--- -- cut:10 --- ---",
            "--- -- cut:30 --- ---",
        ]);
        assert_eq!(default_ends(&steps, all(&steps), CUT), (0x10, 0x30));
    }

    /// Each `vary` passes play a seed of their own, the first the one written, worked out from the
    /// pass alone; a shape that draws nothing from its seed, or has `Vary` off, plays as written.
    #[test]
    fn a_varying_shape_has_a_seed_for_each_group_of_passes_the_first_its_own() {
        let random = Shape {
            kind: FillKind::Random,
            vary: 1,
            ..ramp(0x10, 0x70, 0, 0)
        };
        assert_eq!(random.on_pass(0), random);
        let seeds: Vec<u64> = (0..8).map(|pass| random.on_pass(pass).seed).collect();
        for (pass, seed) in seeds.iter().enumerate() {
            let others = seeds.iter().filter(|other| *other == seed).count();
            assert_eq!(others, 1, "pass {pass}'s seed is its own");
            assert_eq!(
                random.on_pass(pass as i64).seed,
                *seed,
                "and the same again"
            );
        }
        let fours = Shape { vary: 4, ..random };
        let groups: Vec<u64> = (0..12).map(|pass| fours.on_pass(pass).seed).collect();
        assert!(groups[..4].iter().all(|seed| *seed == random.seed));
        assert!(groups[4..8].iter().all(|seed| *seed == groups[4]));
        assert_ne!(groups[4], random.seed);
        assert_ne!(groups[8], groups[4]);
        assert_eq!(
            Shape { vary: 0, ..random }.on_pass(5),
            Shape { vary: 0, ..random }
        );
        // Another written seed varies elsewhere.
        assert_ne!(
            Shape { seed: 9, ..random }.on_pass(3).seed,
            random.on_pass(3).seed
        );
        let ramp = Shape {
            vary: 1,
            ..ramp(0x10, 0x70, 0, 0)
        };
        assert!(!ramp.draws());
        assert_eq!(ramp.on_pass(3), ramp, "a ramp has nothing to vary");
        let chance = Shape {
            place: Where::Chance(50),
            ..ramp
        };
        assert_ne!(chance.on_pass(3), chance, "rows chosen by chance vary");
        // With its pitches kept, `Acid` varies only its second die, as `r` rolls only that.
        let mut acid = Shape {
            kind: FillKind::Acid,
            ..random
        };
        acid.acid.keep = true;
        let varied = acid.on_pass(2);
        assert_eq!(varied.seed, acid.seed);
        assert_ne!(varied.acid.roll, acid.acid.roll);
    }
}
