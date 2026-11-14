//! `Tonnetz`: chords walking Euler's Tonnetz, the lattice of the twelve notes by fifths and thirds,
//! by the three neo-Riemannian moves, after Ornament & Crime's Harrington 1200 and Automatonnetz,
//! themselves after Noise Engineering's Tonnetz Sequent. Each move keeps two of a major or minor
//! triad's notes and moves the third by a semitone or a tone, so a walk of them voice-leads as
//! smoothly as chords can:
//!
//! - `P`, parallel: C major to C minor and back, the third a semitone.
//! - `L`, leading-tone exchange: C major to E minor, the root down a semitone, and E minor back to
//!   C major, the fifth up one.
//! - `R`, relative: C major to A minor, the fifth up a tone, and A minor back to C major, the root
//!   down one.
//!
//! Each undoes itself. The walk starts on the triad on `From`, major or minor, which the first
//! row the `Where` chooses plays, and each row it chooses after plays the chord one move on. The
//! moves come from a list, such as `P L R L`, going round; at random, each row drawn from the seed
//! by a weight for each move; or from a loop of so many moves drawn from the seed once, going
//! round. In scale, a move that would bring in a note outside the song's scale isn't taken: a list
//! or a loop goes on to its next move, and a random one draws from the others. In a major or minor
//! key that leaves `P` out, as it always brings in a note from outside, and `L` and `R` walk round
//! the key's six major and minor chords.
//!
//! What it writes on its Note column is its `Voice`: on the `Chord` voice, the chord's root and the
//! voice's `Chord` setting locked to major or minor, so one track plays the progression; the root
//! alone, for a bass; or one of three voices, each starting on a note of the first triad in root
//! position, the root, the third and the fifth, so three tracks of plain mono voices play it
//! between them, each moving by a semitone or a tone when its note is the one a move changes. A
//! voice stays within a tritone of where it started: past that, it takes its new note an octave the
//! other way, the one leap the walk makes, so a long walk keeps each track in its own register.

use serde::{Deserialize, Serialize};

use super::render::Rows;
use super::{GenCell, Shape, ShapeColumn, mix};
use crate::constants::FX_SLOTS;
use crate::scale::Scale;
use crate::selection::StepRange;
use crate::settings::SettingId;
use crate::song::SynthVoice;
use crate::step::{FxCommand, FxSlot, MAX_NOTE, MIN_NOTE, Mark, Note, Step};

/// The most moves a list holds.
pub const MAX_MOVES: usize = 8;
/// The heaviest a move's weight goes.
pub const MAX_MOVE_WEIGHT: u8 = 9;
/// The longest loop of moves drawn from the seed.
pub const MAX_LOOP: u8 = 16;
/// The dice of a random walk's moves, so they don't follow another fill's.
const DICE: u64 = 0x7044_0590;
/// The dice of a loop's moves.
const LOOPED: u64 = 0x100D_0590;
/// The setting a chord is locked on: the `Chord` voice's chord, its first.
pub const CHORD_SETTING: SettingId = match SettingId::voice(SynthVoice::Chord, 0) {
    Some(id) => id,
    None => panic!("the Chord voice has a chord"),
};
/// The places of major and minor in the `Chord` voice's list of chords, Plaits' `M` and `m`.
pub const MAJOR_CHORD: u8 = 10;
pub const MINOR_CHORD: u8 = 3;
/// The notes' names, sharps for the black keys, as a chord is named.
const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A neo-Riemannian move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Move {
    /// Parallel: the third moves a semitone, so major becomes minor on the same root.
    P,
    /// Leading-tone exchange: a major chord's root down a semitone, a minor's fifth up one.
    L,
    /// Relative: a major chord's fifth up a tone, a minor's root down one.
    R,
}

impl Move {
    /// Every move, in the order their weights are.
    pub const ALL: [Self; 3] = [Self::P, Self::L, Self::R];

    /// The letter it's written as.
    pub fn letter(self) -> char {
        match self {
            Self::P => 'P',
            Self::L => 'L',
            Self::R => 'R',
        }
    }

    /// The move a letter names, in either case.
    pub fn from_letter(letter: char) -> Option<Self> {
        match letter.to_ascii_uppercase() {
            'P' => Some(Self::P),
            'L' => Some(Self::L),
            'R' => Some(Self::R),
            _ => None,
        }
    }

    /// Its place among the moves.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Whether a triad is major or minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Quality {
    #[default]
    Major,
    Minor,
}

impl Quality {
    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
        }
    }

    /// The other one.
    pub fn other(self) -> Self {
        match self {
            Self::Major => Self::Minor,
            Self::Minor => Self::Major,
        }
    }
}

/// A major or minor triad, by its root's place in the octave, `0` for C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Triad {
    pub root: u8,
    pub quality: Quality,
}

impl Triad {
    /// The triad on a root, any note, by its place in the octave.
    pub fn new(root: u8, quality: Quality) -> Self {
        Self {
            root: root % 12,
            quality,
        }
    }

    /// Its notes' places in the octave: the root, the third and the fifth.
    pub fn notes(self) -> [u8; 3] {
        let third = match self.quality {
            Quality::Major => 4,
            Quality::Minor => 3,
        };
        [self.root, (self.root + third) % 12, (self.root + 7) % 12]
    }

    /// The triad a move makes of it.
    pub fn moved(self, by: Move) -> Self {
        let (up, quality) = match (by, self.quality) {
            (Move::P, quality) => (0, quality.other()),
            // C major to E minor, and E minor back.
            (Move::L, Quality::Major) => (4, Quality::Minor),
            (Move::L, Quality::Minor) => (8, Quality::Major),
            // C major to A minor, and A minor back.
            (Move::R, Quality::Major) => (9, Quality::Minor),
            (Move::R, Quality::Minor) => (3, Quality::Major),
        };
        Self::new(self.root + up, quality)
    }

    /// The note a move takes out of it, and the one it brings in, by their places in the octave.
    pub fn changes(self, by: Move) -> (u8, u8) {
        let (before, after) = (self.notes(), self.moved(by).notes());
        let gone = before.into_iter().find(|note| !after.contains(note));
        let new = after.into_iter().find(|note| !before.contains(note));
        // Every move changes exactly one note, as the tests check.
        (gone.unwrap_or(0), new.unwrap_or(0))
    }

    /// Its name, as `C` for C major and `Am` for A minor.
    pub fn name(self) -> String {
        let root = NAMES[usize::from(self.root)];
        match self.quality {
            Quality::Major => root.to_owned(),
            Quality::Minor => format!("{root}m"),
        }
    }

    /// Its place in the `Chord` voice's list, which its lock holds.
    pub fn chord(self) -> u8 {
        match self.quality {
            Quality::Major => MAJOR_CHORD,
            Quality::Minor => MINOR_CHORD,
        }
    }
}

/// The moves a list goes through, up to [`MAX_MOVES`] of them. Saved as their letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    len: u8,
}

impl MoveList {
    /// The first `MAX_MOVES` of these moves.
    pub fn new(moves: &[Move]) -> Self {
        let mut list = Self {
            moves: [Move::P; MAX_MOVES],
            len: 0,
        };
        for (slot, each) in list.moves.iter_mut().zip(moves) {
            *slot = *each;
            list.len += 1;
        }
        list
    }

    pub fn as_slice(&self) -> &[Move] {
        &self.moves[..usize::from(self.len)]
    }

    /// Moves as they're typed, their letters in either case, with or without spaces: one to
    /// [`MAX_MOVES`] of them.
    pub fn parse(text: &str) -> Option<Self> {
        let moves: Option<Vec<Move>> = text
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(Move::from_letter)
            .collect();
        let moves = moves?;
        (!moves.is_empty() && moves.len() <= MAX_MOVES).then(|| Self::new(&moves))
    }
}

/// The issue's example, `P L R L`: a walk that goes from C major through C minor, A♭ major and F
/// minor to D♭ major, and on.
impl Default for MoveList {
    fn default() -> Self {
        Self::new(&[Move::P, Move::L, Move::R, Move::L])
    }
}

/// Their letters with a space between each, as `P L R L`.
impl std::fmt::Display for MoveList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (index, each) in self.as_slice().iter().enumerate() {
            if index > 0 {
                f.write_str(" ")?;
            }
            write!(f, "{}", each.letter())?;
        }
        Ok(())
    }
}

impl TryFrom<String> for MoveList {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        Self::parse(&text).ok_or_else(|| format!("{text:?} isn't a list of P, L and R moves"))
    }
}

impl From<MoveList> for String {
    fn from(list: MoveList) -> Self {
        list.to_string()
    }
}

/// Where a walk's moves come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Moves {
    /// The list, going round.
    #[default]
    List,
    /// Each drawn from the seed, by the moves' weights.
    Random,
    /// A loop of so many drawn from the seed by their weights, going round.
    Loop,
}

impl Moves {
    /// Every one, in the order a list offers them.
    pub const ALL: [Self; 3] = [Self::List, Self::Random, Self::Loop];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::List => "List",
            Self::Random => "Random",
            Self::Loop => "Loop",
        }
    }

    /// What it does, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::List => "The moves as typed, going round",
            Self::Random => "Each move drawn by its weight",
            Self::Loop => "A loop of moves drawn by weight",
        }
    }

    /// Its place among them.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// What a track writes of the chords.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Voice {
    /// The root, with the chord locked on the `Chord` voice.
    Chord,
    /// The root alone.
    Root,
    /// The voice that starts on the first triad's root, its third, or its fifth.
    Low,
    Mid,
    High,
}

impl Voice {
    /// Every voice, in the order a list offers them.
    pub const ALL: [Self; 5] = [Self::Chord, Self::Root, Self::Low, Self::Mid, Self::High];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Chord => "Chord",
            Self::Root => "Root",
            Self::Low => "Low",
            Self::Mid => "Mid",
            Self::High => "High",
        }
    }

    /// What it writes, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Chord => "The root, and the chord for the Chord voice",
            Self::Root => "The chord's root alone",
            Self::Low => "The voice that starts on the root",
            Self::Mid => "The voice that starts on the third",
            Self::High => "The voice that starts on the fifth",
        }
    }

    /// Its place among the voices.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The voice the `nth` of the tracks shaped at once plays: the low, the middle and the high,
    /// going round.
    pub fn across(nth: usize) -> Self {
        [Self::Low, Self::Mid, Self::High][nth % 3]
    }

    /// Which of the three voices it is, if it's one.
    fn place(self) -> Option<usize> {
        match self {
            Self::Low => Some(0),
            Self::Mid => Some(1),
            Self::High => Some(2),
            Self::Chord | Self::Root => None,
        }
    }
}

/// A `Tonnetz` fill's settings, saved with a live generator's shape and left out while they're as
/// they start. The first chord's root is the shape's `From`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tonnetz {
    /// Whether the first chord is major or minor.
    pub quality: Quality,
    /// Where its moves come from, and the list, the weights and the loop's length they come from.
    pub moves: Moves,
    pub list: MoveList,
    /// Each move's weight, `P`, `L` and `R`'s, from 0 to [`MAX_MOVE_WEIGHT`].
    pub weights: [u8; 3],
    pub length: u8,
    /// What it writes, or none while the panel shapes several tracks, each taking a voice of its
    /// own as it goes on them.
    pub voice: Option<Voice>,
    /// Whether it only takes moves that keep to the song's scale.
    pub in_scale: bool,
}

impl Default for Tonnetz {
    /// From a major chord, through `P L R L`, writing chords for the `Chord` voice, free of the
    /// scale; at random, each move as likely, and a loop of four.
    fn default() -> Self {
        Self {
            quality: Quality::Major,
            moves: Moves::List,
            list: MoveList::default(),
            weights: [3, 3, 3],
            length: 4,
            voice: Some(Voice::Chord),
            in_scale: false,
        }
    }
}

impl Tonnetz {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether its moves are drawn from the seed.
    pub fn draws(&self) -> bool {
        self.moves != Moves::List
    }

    /// It as it goes on the `nth` of the tracks shaped at once: with a voice of its own if the
    /// panel gave it none.
    pub fn on_track(self, nth: usize) -> Self {
        Self {
            voice: self.voice.or(Some(Voice::across(nth))),
            ..self
        }
    }

    /// The chords its first `count` rows play, from the triad on `from`, through a scale that a
    /// walk in scale keeps to, with this seed: for the panel to name them.
    pub fn chords(&self, from: u8, seed: u64, scale: Scale, count: usize) -> Vec<Triad> {
        let mut walk = Walk::new(*self, from, seed, scale);
        (0..count)
            .map(|nth| {
                if nth > 0 {
                    walk.step();
                }
                walk.triad
            })
            .collect()
    }
}

/// A walk on its way: the chord it's on and its three voices' notes, and how far along its moves
/// it is. It's plain data, so a render keeps one without allocating.
struct Walk {
    tonnetz: Tonnetz,
    seed: u64,
    /// The scale its moves keep to, if they keep to one.
    scale: Option<Scale>,
    from: i16,
    triad: Triad,
    /// The three voices' notes, and where each started.
    voices: [i16; 3],
    starts: [i16; 3],
    /// How many moves it has made, held or not, and its place in its list or loop.
    moved: usize,
    place: usize,
}

impl Walk {
    fn new(tonnetz: Tonnetz, from: u8, seed: u64, scale: Scale) -> Self {
        let triad = Triad::new(from, tonnetz.quality);
        let from = i16::from(from);
        let third = match tonnetz.quality {
            Quality::Major => 4,
            Quality::Minor => 3,
        };
        let voices = [from, from + third, from + 7];
        Self {
            tonnetz,
            seed,
            scale: (tonnetz.in_scale && scale.on).then_some(scale),
            from,
            triad,
            voices,
            starts: voices,
            moved: 0,
            place: 0,
        }
    }

    /// Whether a move keeps to the scale, if it keeps to one.
    fn allowed(&self, by: Move) -> bool {
        let (_, new) = self.triad.changes(by);
        self.scale.is_none_or(|scale| scale.contains(new))
    }

    /// A move by the weights, from a die, among those `allowed` lets through; none if their
    /// weights come to nothing.
    fn weighed(&self, die: u64, allowed: impl Fn(Move) -> bool) -> Option<Move> {
        let weight = |by: Move| u64::from(self.tonnetz.weights[by.index()].min(MAX_MOVE_WEIGHT));
        let total: u64 = Move::ALL
            .into_iter()
            .filter(|by| allowed(*by))
            .map(weight)
            .sum();
        if total == 0 {
            return None;
        }
        let mut at = die % total;
        for by in Move::ALL.into_iter().filter(|by| allowed(*by)) {
            if at < weight(by) {
                return Some(by);
            }
            at -= weight(by);
        }
        None
    }

    /// The `at`th move of its list or its loop, going round, and how many it has.
    fn listed(&self, at: usize) -> (Option<Move>, usize) {
        match self.tonnetz.moves {
            Moves::Loop => {
                let len = usize::from(self.tonnetz.length.clamp(1, MAX_LOOP));
                let die = mix(self.seed ^ LOOPED, at % len);
                (self.weighed(die, |_| true), len)
            }
            _ => {
                let list = self.tonnetz.list.as_slice();
                let len = list.len();
                (list.get(at % len.max(1)).copied(), len)
            }
        }
    }

    /// One move on: the next of the list or loop that keeps to the scale, or a random one that
    /// does. With none, it holds the chord.
    fn step(&mut self) {
        let next = match self.tonnetz.moves {
            Moves::Random => {
                let die = mix(self.seed ^ DICE, self.moved);
                self.weighed(die, |by| self.allowed(by))
            }
            Moves::List | Moves::Loop => {
                let (_, len) = self.listed(self.place);
                let found = (0..len).find_map(|ahead| {
                    let (by, _) = self.listed(self.place + ahead);
                    by.filter(|by| self.allowed(*by)).map(|by| (ahead, by))
                });
                self.place += found.map_or(1, |(ahead, _)| ahead + 1);
                found.map(|(_, by)| by)
            }
        };
        self.moved += 1;
        if let Some(by) = next {
            self.take(by);
        }
    }

    /// Make a move: the voice on the note it takes out moves to the nearest of the note it brings
    /// in, a semitone or a tone, unless that's more than a tritone from where the voice started,
    /// when it takes the one an octave the other way.
    fn take(&mut self, by: Move) {
        let (gone, new) = self.triad.changes(by);
        for (voice, start) in self.voices.iter_mut().zip(self.starts) {
            if voice.rem_euclid(12) as u8 == gone {
                let up = (i16::from(new) - i16::from(gone) + 6).rem_euclid(12) - 6;
                *voice = start + (*voice + up - start + 6).rem_euclid(12) - 6;
            }
        }
        self.triad = self.triad.moved(by);
    }

    /// The note a voice writes now: the root within a tritone of `From` for the chord and the root,
    /// or a voice's own note, kept within the notes a step holds.
    fn note(&self, voice: Voice) -> u8 {
        let note = match voice.place() {
            Some(at) => self.voices[at],
            None => {
                let up = (i16::from(self.triad.root) - self.from + 6).rem_euclid(12) - 6;
                self.from + up
            }
        };
        let (low, high) = (i16::from(MIN_NOTE), i16::from(MAX_NOTE));
        let mut note = note;
        while note < low {
            note += 12;
        }
        while note > high {
            note -= 12;
        }
        note as u8
    }
}

/// A row's cell on the `Chord` voice: the chord's root, over the note there with its velocity and
/// gate or as a note of its own, and the row's FX with the chord locked in the slot that already
/// locks it or the first free one. A row with every slot taken by something else gets the root
/// alone.
fn chord_cell(column: ShapeColumn, step: &Step, root: u8, triad: Triad) -> GenCell {
    let pitched = column.cell(step, u16::from(root) << 8);
    let (velocity, gate) = match pitched {
        GenCell::Note { velocity, gate, .. } => (velocity, gate),
        _ => (step.velocity, step.gate),
    };
    let lock = FxCommand::Set(CHORD_SETTING);
    let mut fx = [FxSlot::default(); FX_SLOTS];
    for (slot, cell) in fx.iter_mut().zip(&step.fx) {
        *slot = *cell;
    }
    let slots = step.fx.len().min(FX_SLOTS);
    let at = (0..slots)
        .find(|&slot| fx[slot].command == Some(lock))
        .or_else(|| (0..slots).find(|&slot| fx[slot].command.is_none()));
    let Some(at) = at else {
        return pitched;
    };
    fx[at] = FxSlot {
        command: Some(lock),
        value: u16::from(triad.chord()) << 8,
        mark: Mark::Lock,
    };
    GenCell::NoteFx {
        note: Note::On(root),
        velocity,
        gate,
        fx,
    }
}

/// A `Tonnetz` fill's cells for `range`, one for each row, the first in `cells[0]`: on each row
/// `rows` chooses, the walk's next chord, as its voice writes it, from the triad on the shape's
/// `From`. It writes notes; on any other column it leaves the rows alone.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    shape: &Shape,
    rows: &Rows,
    cells: &mut [GenCell],
) {
    let ShapeColumn::Note { scale, .. } = column else {
        return;
    };
    let tonnetz = shape.tonnetz;
    let voice = tonnetz.voice.unwrap_or(Voice::Low);
    let mut walk = Walk::new(tonnetz, shape.from, shape.seed, scale);
    for (nth, offset) in rows.ones().enumerate() {
        let (Some(cell), Some(step)) = (cells.get_mut(offset), steps.get(range.first + offset))
        else {
            break;
        };
        if nth > 0 {
            walk.step();
        }
        let note = walk.note(voice);
        *cell = match voice {
            Voice::Chord => chord_cell(column, step, note, walk.triad),
            _ => column.cell(step, u16::from(note) << 8),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;
    use crate::settings::Kind;
    use crate::shape::{FillKind, Shape, Where, on_track, written};

    const C_MAJOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };

    fn triad(root: u8, quality: Quality) -> Triad {
        Triad::new(root, quality)
    }

    fn every_triad() -> impl Iterator<Item = Triad> {
        (0..12).flat_map(|root| [triad(root, Quality::Major), triad(root, Quality::Minor)])
    }

    /// A Tonnetz on the Note column, from C-3.
    fn shape(tonnetz: Tonnetz, seed: u64) -> Shape {
        Shape {
            kind: FillKind::Tonnetz,
            from: 60,
            to: 72,
            place: Where::AllRows,
            seed,
            tonnetz,
            ..Shape::following(Default::default(), seed)
        }
    }

    fn writing(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: Some(0x64),
        }
    }

    /// What a shape writes on an empty column of `rows`, row by row.
    fn notes(shape: &Shape, column: ShapeColumn, rows: usize) -> Vec<Step> {
        let steps = vec![Step::default(); rows];
        let range = StepRange::between(0, rows - 1);
        let mut out = steps.clone();
        for (row, step) in written(&steps, range, column, shape) {
            out[row] = step;
        }
        out
    }

    fn pitch(step: &Step) -> u8 {
        match step.note {
            Note::On(note) => note,
            _ => panic!("no note on {step}"),
        }
    }

    /// P, L and R as Cohn and Riemann have them: C major's parallel is C minor, its leading-tone
    /// exchange E minor and its relative A minor, and A minor's are A major, F major and C major.
    #[test]
    fn p_l_and_r_are_as_defined() {
        let c = triad(0, Quality::Major);
        assert_eq!(c.notes(), [0, 4, 7]);
        assert_eq!(c.moved(Move::P), triad(0, Quality::Minor));
        assert_eq!(c.moved(Move::L), triad(4, Quality::Minor));
        assert_eq!(c.moved(Move::R), triad(9, Quality::Minor));
        let a = triad(9, Quality::Minor);
        assert_eq!(a.notes(), [9, 0, 4]);
        assert_eq!(a.moved(Move::P), triad(9, Quality::Major));
        assert_eq!(a.moved(Move::L), triad(5, Quality::Major));
        assert_eq!(a.moved(Move::R), triad(0, Quality::Major));
        // The note each changes: P the third, L a major chord's root and a minor's fifth, R a
        // major chord's fifth and a minor's root.
        assert_eq!(c.changes(Move::P), (4, 3));
        assert_eq!(c.changes(Move::L), (0, 11));
        assert_eq!(c.changes(Move::R), (7, 9));
        assert_eq!(a.changes(Move::L), (4, 5));
        assert_eq!(a.changes(Move::R), (9, 7));
        assert_eq!(c.name(), "C");
        assert_eq!(triad(8, Quality::Minor).name(), "G#m");
    }

    /// On every one of the 24 triads, each move undoes itself, keeps two notes and moves the third
    /// by a semitone, or for `R` a tone.
    #[test]
    fn each_move_undoes_itself_and_changes_one_note_by_a_step() {
        for chord in every_triad() {
            for by in Move::ALL {
                let moved = chord.moved(by);
                assert_eq!(moved.moved(by), chord, "{by:?} on {}", chord.name());
                assert_ne!(moved.quality, chord.quality);
                let kept = chord
                    .notes()
                    .into_iter()
                    .filter(|note| moved.notes().contains(note))
                    .count();
                assert_eq!(kept, 2, "{by:?} on {}", chord.name());
                let (gone, new) = chord.changes(by);
                let apart = (i16::from(new) - i16::from(gone)).rem_euclid(12);
                let apart = apart.min(12 - apart);
                assert_eq!(apart, if by == Move::R { 2 } else { 1 });
            }
        }
    }

    /// `P L R L` from C major goes through C minor, A♭ major, F minor and D♭ major, then round.
    #[test]
    fn a_list_goes_round() {
        let tonnetz = Tonnetz::default();
        let names: Vec<String> = tonnetz
            .chords(60, 1, Scale::default(), 9)
            .into_iter()
            .map(Triad::name)
            .collect();
        assert_eq!(names, ["C", "Cm", "G#", "Fm", "C#", "C#m", "A", "F#m", "D"]);
    }

    /// On the `Chord` voice, each row is the chord's root, within a tritone of `From`, with the
    /// voice's chord locked to `M` or `m` in a free slot, the row's other FX kept.
    #[test]
    fn the_chord_voice_gets_the_root_and_the_chord_locked() {
        let Kind::Stepped { choices, .. } = CHORD_SETTING.entry().kind else {
            panic!("a chord is a list");
        };
        assert_eq!(choices[usize::from(MAJOR_CHORD)], "M");
        assert_eq!(choices[usize::from(MINOR_CHORD)], "m");
        let shape = shape(Tonnetz::default(), 1);
        let mut steps: Vec<Step> = vec![Step::default(); 4];
        steps[1].fx[0] = FxSlot {
            command: Some(FxCommand::CUT),
            value: 0x4000,
            mark: Mark::Lock,
        };
        let range = StepRange::between(0, 3);
        let mut out = steps.clone();
        for (row, step) in written(&steps, range, writing(Scale::default()), &shape) {
            out[row] = step;
        }
        let lock = FxCommand::Set(CHORD_SETTING);
        // C, Cm, A♭ and Fm, each root near C-3.
        let roots: Vec<u8> = out.iter().map(pitch).collect();
        assert_eq!(roots, [60, 60, 56, 65]);
        assert_eq!(out[0].fx[0].command, Some(lock));
        assert_eq!(out[0].fx[0].coarse(), MAJOR_CHORD);
        assert_eq!(out[1].fx[0].command, Some(FxCommand::CUT), "kept");
        assert_eq!(out[1].fx[1].command, Some(lock));
        assert_eq!(out[1].fx[1].coarse(), MINOR_CHORD);
        assert_eq!(out[1].velocity, Some(0x64));
    }

    /// Across three tracks, each plays a voice of the chord, and from row to row exactly one of
    /// them moves, by a semitone or a tone, or an octave the other way where it would go more than
    /// a tritone from where it started; the three are always the chord's notes.
    #[test]
    fn across_three_tracks_each_move_changes_one_note_by_a_step() {
        let rows = 256;
        for moves in Moves::ALL {
            let tonnetz = Tonnetz {
                moves,
                voice: None,
                weights: [2, 3, 4],
                length: 7,
                ..Tonnetz::default()
            };
            let base = shape(tonnetz, 0x5EED);
            let column = writing(Scale::default());
            let tracks: Vec<Vec<u8>> = (0..3)
                .map(|nth| {
                    notes(&on_track(&base, nth), column, rows)
                        .iter()
                        .map(pitch)
                        .collect()
                })
                .collect();
            assert_eq!([tracks[0][0], tracks[1][0], tracks[2][0]], [60, 64, 67]);
            let chords = tonnetz.chords(60, 0x5EED, Scale::default(), rows);
            for row in 1..rows {
                let moved: Vec<i16> = tracks
                    .iter()
                    .map(|track| i16::from(track[row]) - i16::from(track[row - 1]))
                    .filter(|by| *by != 0)
                    .collect();
                let held = chords[row] == chords[row - 1];
                assert_eq!(moved.len(), usize::from(!held), "{moves:?} row {row}");
                for by in moved {
                    assert!(
                        matches!(by.abs(), 1 | 2 | 10 | 11),
                        "{moves:?} row {row} by {by}"
                    );
                }
                let mut heard: Vec<u8> = tracks.iter().map(|track| track[row] % 12).collect();
                let mut wanted = chords[row].notes().to_vec();
                heard.sort();
                wanted.sort();
                assert_eq!(heard, wanted, "{moves:?} row {row}");
            }
            // Each stays within a tritone of where it started.
            for (track, start) in tracks.iter().zip([60, 64, 67]) {
                assert!(track.iter().all(|note| note.abs_diff(start) <= 6));
            }
        }
    }

    /// A seed walks the same way every time, and another seed another way.
    #[test]
    fn a_seed_repeats() {
        for moves in [Moves::Random, Moves::Loop] {
            let tonnetz = Tonnetz {
                moves,
                ..Tonnetz::default()
            };
            let walk = |seed| tonnetz.chords(60, seed, Scale::default(), 64);
            assert_eq!(walk(7), walk(7), "{moves:?}");
            assert_ne!(walk(7), walk(8), "{moves:?}");
            let column = writing(Scale::default());
            assert_eq!(
                notes(&shape(tonnetz, 7), column, 64),
                notes(&shape(tonnetz, 7), column, 64)
            );
        }
        // A list draws nothing.
        assert!(!Tonnetz::default().draws());
        let list = |seed| Tonnetz::default().chords(60, seed, Scale::default(), 16);
        assert_eq!(list(7), list(8));
    }

    /// A loop of so many moves goes round: without a scale to keep to, its chords after a loop
    /// are its moves again from there.
    #[test]
    fn a_loop_repeats_its_moves() {
        let tonnetz = Tonnetz {
            moves: Moves::Loop,
            length: 3,
            ..Tonnetz::default()
        };
        let chords = tonnetz.chords(60, 99, Scale::default(), 13);
        let moves: Vec<Move> = chords
            .windows(2)
            .map(|pair| {
                Move::ALL
                    .into_iter()
                    .find(|by| pair[0].moved(*by) == pair[1])
                    .expect("one move apart")
            })
            .collect();
        assert_eq!(moves[..3], moves[3..6]);
        assert_eq!(moves[..3], moves[9..12]);
    }

    /// In scale, every chord keeps to it: in C major, `P` never comes, and `L` and `R` walk the
    /// key's six major and minor chords.
    #[test]
    fn in_scale_it_keeps_to_the_key() {
        for moves in Moves::ALL {
            let tonnetz = Tonnetz {
                moves,
                in_scale: true,
                ..Tonnetz::default()
            };
            let chords = tonnetz.chords(60, 3, C_MAJOR, 64);
            for chord in &chords {
                assert!(
                    chord.notes().into_iter().all(|note| C_MAJOR.contains(note)),
                    "{moves:?}: {}",
                    chord.name()
                );
            }
            let names: std::collections::BTreeSet<String> =
                chords.into_iter().map(Triad::name).collect();
            assert!(names.len() >= 2, "{moves:?} moves: {names:?}");
        }
        // With the scale off, it's free.
        let tonnetz = Tonnetz {
            in_scale: true,
            ..Tonnetz::default()
        };
        assert_eq!(
            tonnetz.chords(60, 3, Scale::default(), 3)[1],
            triad(0, Quality::Minor)
        );
    }

    #[test]
    fn a_list_is_typed_as_letters() {
        let list = MoveList::parse("p lr L").unwrap();
        assert_eq!(list.as_slice(), [Move::P, Move::L, Move::R, Move::L]);
        assert_eq!(list.to_string(), "P L R L");
        assert!(MoveList::parse("").is_none());
        assert!(MoveList::parse("PLX").is_none());
        assert!(MoveList::parse("PLRPLRPLR").is_none(), "nine is too many");
        let saved = ron::to_string(&list).unwrap();
        assert_eq!(saved, "\"P L R L\"");
        assert_eq!(ron::from_str::<MoveList>(&saved).unwrap(), list);
    }
}
