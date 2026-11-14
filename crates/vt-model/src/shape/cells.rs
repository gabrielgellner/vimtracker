//! `Cells`: rhythms from an elementary cellular automaton, one of Stephen Wolfram's 256 rules, as
//! dan derks' *less concepts* for norns plays them. A row of cells, each on or off, makes its
//! next generation cell by cell from the cell and its two neighbours: the three are a number from
//! 0 to 7, left the highest bit, and the rule's bit of that number is the cell's next state. Rule
//! 30 is chaotic, 90 draws Sierpiński's triangle, 110 is complex and 184 moves like traffic. The
//! row is a ring, so the first cell's left neighbour is the last.
//!
//! It's a `Where`, choosing the rows a fill writes on, in one of two layouts. On one column, each
//! row is a cell and each pass a generation: the rows chosen are the generation's cells that are
//! on. Across tracks, each track is a cell and each row a generation, so the tracks play the
//! automaton's picture down the phrase. Kept live with `Vary`, each pass steps it on, one
//! generation on a column and on from the last row across tracks.
//!
//! A pass's generation is worked out from the seed row and how many generations it has stepped,
//! never carried from one pass to the next, so playing from anywhere hears what playing to there
//! would. The automaton starts again from its seed row after [`COLUMN_GENERATIONS`] on a column
//! and [`ACROSS_GENERATIONS`] across tracks, so a render never steps more than 255 generations of
//! up to 4096 cells, or 8191 of up to 64.

use serde::{Deserialize, Serialize};

use super::mix;
use super::render::{Rows, WORDS};
use crate::constants::MAX_PHRASE_STEPS;
use crate::selection::StepRange;
use crate::step::{Note, Step};

/// How many generations a column goes through before it starts again from its seed row: 256
/// passes, over eight minutes of a bar's phrase at 120 BPM.
pub const COLUMN_GENERATIONS: usize = 256;
/// How many generations the tracks across go through before they start again from their seed row:
/// the most rows a phrase has, so a 16-row phrase's 256 passes.
pub const ACROSS_GENERATIONS: usize = MAX_PHRASE_STEPS;
/// The most tracks across, each a cell of one row.
pub const MAX_CELLS: usize = 64;
/// The rule it starts with: Wolfram's rule 30.
pub const DEFAULT_RULE: u8 = 30;
/// What the seed row's cells are drawn with, so they don't follow another fill's.
const SEEDED: u64 = 0xCE11_0589;

/// Where the automaton starts: its seed row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Start {
    /// Each cell on or off at even odds, drawn from the seed.
    #[default]
    Seed,
    /// One cell on, in the middle, as Wolfram's pictures start.
    One,
    /// The rows with a note on them, or across tracks, the tracks with a note on the first row.
    Hits,
}

impl Start {
    /// Every start, in the order a list offers them.
    pub const ALL: [Self; 3] = [Self::Seed, Self::One, Self::Hits];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Seed => "Seed",
            Self::One => "One",
            Self::Hits => "Hits",
        }
    }

    /// What it starts from, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Seed => "Cells drawn from the seed",
            Self::One => "One cell in the middle",
            Self::Hits => "The notes already there",
        }
    }

    /// Its place among the starts.
    pub fn index(self) -> usize {
        self as usize
    }

    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The tracks a `Cells` plays across: which of them this one is, and how many there are, and the
/// row they all start from, worked out once for all of them as the panel shapes them, so they
/// play one automaton whatever each track's own seed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Across {
    pub cell: u8,
    pub cells: u8,
    pub row: u64,
}

/// A `Cells` `Where`'s rule, its seed row, and across tracks, which cell its track is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cells {
    pub rule: u8,
    #[serde(default, skip_serializing_if = "Start::is_default")]
    pub start: Start,
    /// None on one column, each row a cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub across: Option<Across>,
    /// How many generations a pass has stepped it on from its seed row, set as the engine plays a
    /// pass ([`Shape::on_pass`](super::Shape::on_pass)), going round at [`ACROSS_GENERATIONS`].
    /// Never saved, so what's written is the seed row's.
    #[serde(skip)]
    pub stepped: u16,
}

impl Default for Cells {
    fn default() -> Self {
        Self {
            rule: DEFAULT_RULE,
            start: Start::Seed,
            across: None,
            stepped: 0,
        }
    }
}

impl Cells {
    /// The same, stepped on by so many generations, as a pass plays it.
    pub fn stepped_by(self, generations: i64) -> Self {
        Self {
            stepped: generations.rem_euclid(ACROSS_GENERATIONS as i64) as u16,
            ..self
        }
    }

    /// The cells the `nth` of several tracks shaped at once plays: its own cell of one row,
    /// started from this `start` drawn from `seed`, or from `hits`, whether each track has a note
    /// on its first row. On one track, it's a column of its own. Past [`MAX_CELLS`] tracks, the
    /// rest go round the cells again.
    pub fn for_track(self, seed: u64, nth: usize, hits: &[bool]) -> Self {
        let cells = hits.len().min(MAX_CELLS);
        if cells < 2 {
            return Self {
                across: None,
                ..self
            };
        }
        let mut row = 0u64;
        seeded(
            self.start,
            seed,
            cells,
            |cell| hits[cell],
            |cell| {
                row |= 1u64 << cell;
            },
        );
        Self {
            across: Some(Across {
                cell: (nth % cells) as u8,
                cells: cells as u8,
                row,
            }),
            ..self
        }
    }

    /// The rows it chooses among `open`, the rows of the span the column could change, by their
    /// offset in it: the cells on in its generation, or across tracks, the generations its own
    /// cell is on in.
    pub(super) fn chosen(
        self,
        steps: &[Step],
        range: StepRange,
        seed: u64,
        open: &Rows,
        len: usize,
    ) -> Rows {
        let on = match self.across {
            Some(across) => self.down(across, len),
            None => self.along(steps, range, seed, len),
        };
        let mut hit = Rows::new(len);
        for offset in open.ones() {
            if on.has(offset) {
                hit.set(offset);
            }
        }
        hit
    }

    /// On one column: the generation this pass plays, a row a cell.
    fn along(self, steps: &[Step], range: StepRange, seed: u64, len: usize) -> Rows {
        let mut row = Rows::new(len);
        let hit = |offset: usize| {
            steps
                .get(range.first + offset)
                .is_some_and(|step| matches!(step.note, Note::On(_)))
        };
        seeded(self.start, seed, len, hit, |cell| row.set(cell));
        for _ in 0..usize::from(self.stepped) % COLUMN_GENERATIONS {
            row = generation(&row, self.rule);
        }
        row
    }

    /// Across tracks: the generations its cell is on in, a row a generation, carrying on from the
    /// last row of the pass before.
    fn down(self, across: Across, len: usize) -> Rows {
        let cells = u32::from(across.cells.clamp(1, MAX_CELLS as u8));
        let start = across.row & mask(cells);
        let first = usize::from(self.stepped) * len % ACROSS_GENERATIONS;
        let mut state = start;
        for _ in 0..first {
            state = next(state, self.rule, cells);
        }
        let mut on = Rows::new(len);
        let mut at = first;
        for offset in 0..len {
            if (state >> across.cell.min(63)) & 1 != 0 {
                on.set(offset);
            }
            at += 1;
            if at == ACROSS_GENERATIONS {
                at = 0;
                state = start;
            } else {
                state = next(state, self.rule, cells);
            }
        }
        on
    }
}

/// Lay a seed row of `width` cells by `set`, from the seed, one cell in the middle, or `hit`.
fn seeded(
    start: Start,
    seed: u64,
    width: usize,
    hit: impl Fn(usize) -> bool,
    mut set: impl FnMut(usize),
) {
    if width == 0 {
        return;
    }
    match start {
        Start::Seed => (0..width)
            .filter(|cell| mix(seed ^ SEEDED, *cell) & 1 != 0)
            .for_each(set),
        Start::One => set(width / 2),
        Start::Hits => (0..width).filter(|cell| hit(*cell)).for_each(set),
    }
}

/// The lowest `cells` bits.
fn mask(cells: u32) -> u64 {
    if cells >= 64 {
        u64::MAX
    } else {
        (1 << cells) - 1
    }
}

/// Each cell's next state from its left neighbour, itself and its right neighbour, bit by bit:
/// the rule's bit for each neighbourhood of three it's on.
fn ruled(left: u64, centre: u64, right: u64, rule: u8) -> u64 {
    let mut next = 0;
    for pattern in 0..8 {
        if (rule >> pattern) & 1 == 0 {
            continue;
        }
        let pick = |bits: u64, on: bool| if on { bits } else { !bits };
        next |= pick(left, pattern & 4 != 0)
            & pick(centre, pattern & 2 != 0)
            & pick(right, pattern & 1 != 0);
    }
    next
}

/// A ring of up to 64 cells' next generation, a cell a bit, the first cell the lowest.
fn next(row: u64, rule: u8, cells: u32) -> u64 {
    let cells = cells.clamp(1, 64);
    let mask = mask(cells);
    // A cell's left neighbour is the one before it, and the first's is the last.
    let left = ((row << 1) | (row >> (cells - 1))) & mask;
    let right = ((row >> 1) | (row << (cells - 1))) & mask;
    ruled(left, row, right, rule) & mask
}

/// A ring of up to `MAX_PHRASE_STEPS` cells' next generation, a word of 64 at a time.
fn generation(row: &Rows, rule: u8) -> Rows {
    let width = row.len;
    let mut out = Rows::new(width);
    if width == 0 {
        return out;
    }
    let words = width.div_ceil(64).min(WORDS);
    let first = row.words[0] & 1;
    let last = u64::from(row.has(width - 1));
    for word in 0..words {
        let centre = row.words[word];
        let before = if word == 0 {
            last
        } else {
            row.words[word - 1] >> 63
        };
        let after = if word + 1 < words {
            row.words[word + 1] << 63
        } else {
            0
        };
        let left = (centre << 1) | before;
        let mut right = (centre >> 1) | after;
        if word == words - 1 {
            right |= first << ((width - 1) % 64);
        }
        out.words[word] = ruled(left, centre, right, rule);
    }
    // Nothing past the last cell.
    if !width.is_multiple_of(64) {
        out.words[words - 1] &= mask((width % 64) as u32);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row of `width` cells as text, `x` for on.
    fn shown(row: &Rows, width: usize) -> String {
        (0..width)
            .map(|cell| if row.has(cell) { 'x' } else { '.' })
            .collect()
    }

    /// The first generations of a rule from one cell in the middle of a ring this wide.
    fn generations(rule: u8, width: usize, count: usize) -> Vec<String> {
        let mut row = Rows::new(width);
        row.set(width / 2);
        (0..count)
            .map(|_| {
                let text = shown(&row, width);
                row = generation(&row, rule);
                text
            })
            .collect()
    }

    /// The same, a word at a time.
    fn words(rule: u8, width: u32, count: usize) -> Vec<String> {
        let mut row = 1u64 << (width / 2);
        (0..count)
            .map(|_| {
                let text = (0..width)
                    .map(|cell| if (row >> cell) & 1 != 0 { 'x' } else { '.' })
                    .collect();
                row = next(row, rule, width);
                text
            })
            .collect()
    }

    /// As Wolfram's *A New Kind of Science* draws them from one cell: rule 30's first
    /// generations, rule 90's Sierpiński triangle, rule 110's growing to the left, and rule 184's
    /// single car driving right.
    #[test]
    fn known_rules_make_their_published_first_generations() {
        assert_eq!(
            generations(30, 15, 6),
            [
                ".......x.......",
                "......xxx......",
                ".....xx..x.....",
                "....xx.xxxx....",
                "...xx..x...x...",
                "..xx.xxxx.xxx..",
            ]
        );
        assert_eq!(
            generations(90, 15, 5),
            [
                ".......x.......",
                "......x.x......",
                ".....x...x.....",
                "....x.x.x.x....",
                "...x.......x...",
            ]
        );
        assert_eq!(
            generations(110, 15, 5),
            [
                ".......x.......",
                "......xx.......",
                ".....xxx.......",
                "....xx.x.......",
                "...xxxxx.......",
            ]
        );
        assert_eq!(
            generations(184, 15, 3),
            [".......x.......", "........x......", ".........x....."]
        );
    }

    #[test]
    fn a_word_steps_as_the_rows_do_and_the_ring_wraps() {
        for rule in [30, 45, 90, 110, 150, 184, 255, 0] {
            for width in [1, 2, 7, 15, 16, 63, 64] {
                assert_eq!(
                    words(rule, width as u32, 40),
                    generations(rule, width, 40),
                    "rule {rule} over {width}"
                );
            }
        }
        // Rule 184 drives its car round the ring: off the end and back in at the start.
        let mut row = Rows::new(4);
        row.set(3);
        assert_eq!(shown(&generation(&row, 184), 4), "x...");
        assert_eq!(next(0b1000, 184, 4), 0b0001);
    }

    #[test]
    fn a_long_ring_steps_across_its_words() {
        // Rule 90 from one cell at the end of the first word reaches into the second.
        let mut row = Rows::new(130);
        row.set(63);
        let row = generation(&row, 90);
        assert!(row.has(62) && row.has(64) && !row.has(63));
        assert_eq!(row.count(), 2);
        // And from the last cell, round to the first.
        let mut row = Rows::new(130);
        row.set(129);
        let row = generation(&row, 90);
        assert!(row.has(128) && row.has(0));
        assert_eq!(row.count(), 2);
        // A width of whole words.
        let mut row = Rows::new(128);
        row.set(127);
        let row = generation(&row, 90);
        assert!(row.has(126) && row.has(0));
        assert_eq!(row.count(), 2);
    }

    #[test]
    fn a_seed_row_is_the_seeds_the_middle_or_the_notes() {
        let mut steps = vec![Step::default(); 8];
        steps[1].note = Note::On(60);
        steps[6].note = Note::On(62);
        let range = StepRange::between(0, 7);
        let mut all = Rows::new(8);
        (0..8).for_each(|row| all.set(row));
        let rows = |cells: Cells, seed| shown(&cells.chosen(&steps, range, seed, &all, 8), 8);
        let one = Cells {
            start: Start::One,
            ..Cells::default()
        };
        assert_eq!(rows(one, 1), "....x...");
        let hits = Cells {
            start: Start::Hits,
            ..Cells::default()
        };
        assert_eq!(rows(hits, 1), ".x....x.");
        let seeded = Cells::default();
        assert_eq!(rows(seeded, 7), rows(seeded, 7), "a seed repeats");
        let differ = (0..16)
            .filter(|seed| rows(seeded, *seed) != rows(seeded, 7))
            .count();
        assert!(differ > 8, "another seed starts elsewhere");
        // Only the rows it could change.
        let mut open = Rows::new(8);
        open.set(1);
        assert_eq!(
            shown(&hits.chosen(&steps, range, 1, &open, 8), 8),
            ".x......"
        );
    }

    #[test]
    fn each_pass_on_a_column_steps_a_generation_and_goes_round() {
        let steps = vec![Step::default(); 15];
        let range = StepRange::between(0, 14);
        let mut all = Rows::new(15);
        (0..15).for_each(|row| all.set(row));
        let cells = Cells {
            rule: 30,
            start: Start::One,
            ..Cells::default()
        };
        let pass = |pass: i64| {
            shown(
                &cells.stepped_by(pass).chosen(&steps, range, 0, &all, 15),
                15,
            )
        };
        assert_eq!(
            &(0..6).map(pass).collect::<Vec<_>>(),
            &generations(30, 15, 6)
        );
        assert_eq!(
            pass(COLUMN_GENERATIONS as i64),
            pass(0),
            "back to the seed row"
        );
        assert_eq!(pass(COLUMN_GENERATIONS as i64 + 3), pass(3));
        assert_eq!(pass(-1), pass(COLUMN_GENERATIONS as i64 - 1));
    }

    #[test]
    fn across_tracks_each_is_a_cell_and_each_row_a_generation() {
        let across = |nth| {
            Cells {
                rule: 90,
                start: Start::One,
                ..Cells::default()
            }
            .for_track(0, nth, &[false; 7])
        };
        let mut open = Rows::new(4);
        (0..4).for_each(|row| open.set(row));
        let tracks: Vec<String> = (0..7)
            .map(|nth| {
                shown(
                    &across(nth).chosen(&[], StepRange::between(0, 3), 0, &open, 4),
                    4,
                )
            })
            .collect();
        // Read down the tracks, row by row, it's rule 90's picture on a ring of seven.
        let picture: Vec<String> = (0..4)
            .map(|row| tracks.iter().map(|track| &track[row..=row]).collect())
            .collect();
        assert_eq!(picture, generations(90, 7, 4));
        // The next pass carries on from the last row.
        let next: Vec<String> = (0..7)
            .map(|nth| {
                let cells = across(nth).stepped_by(1);
                shown(&cells.chosen(&[], StepRange::between(0, 3), 0, &open, 4), 4)
            })
            .collect();
        let picture: Vec<String> = (0..4)
            .map(|row| next.iter().map(|track| &track[row..=row]).collect())
            .collect();
        assert_eq!(picture, generations(90, 7, 8)[4..]);
        // One track alone is a column of its own.
        assert_eq!(Cells::default().for_track(0, 0, &[true]).across, None);
    }

    #[test]
    fn across_tracks_starts_from_their_hits_or_one_seed() {
        let hits = Cells {
            start: Start::Hits,
            ..Cells::default()
        };
        let row = |cells: Cells| cells.across.map(|across| across.row);
        assert_eq!(row(hits.for_track(0, 1, &[true, false, true])), Some(0b101));
        let seeded = Cells::default();
        let rows: Vec<_> = (0..4)
            .map(|nth| row(seeded.for_track(5, nth, &[false; 4])))
            .collect();
        assert!(
            rows.iter().all(|found| *found == rows[0]),
            "one row for all"
        );
    }

    #[test]
    fn across_tracks_goes_round_after_its_generations() {
        let cells = Cells {
            rule: 30,
            start: Start::One,
            ..Cells::default()
        }
        .for_track(0, 2, &[false; 5]);
        let len = 16;
        let mut open = Rows::new(len);
        (0..len).for_each(|row| open.set(row));
        let pass = |pass: i64| {
            shown(
                &cells
                    .stepped_by(pass)
                    .chosen(&[], StepRange::between(0, len - 1), 0, &open, len),
                len,
            )
        };
        let passes = (ACROSS_GENERATIONS / len) as i64;
        assert_eq!(pass(passes), pass(0));
        assert_eq!(pass(passes + 1), pass(1));
    }
}
