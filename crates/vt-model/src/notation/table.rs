//! Which token wrote each row of a render, so the grid can light the rows a token writes and the
//! field the token playing (decision 460).
//!
//! A render keeps a byte a row beside its cells ([`render_tokens_into`]), up to
//! `MAX_PHRASE_STEPS` of them, allocating nothing: the place of the token of its setting that
//! wrote the row, as the fill counts its tokens, a `List` by its values and a random fill by its
//! notes' degrees. A grammar says which of those bytes each of its tokens is ([`Grammar::rows`]),
//! so the table and a field meet without either knowing the other's text.

use super::{Field, GrammarId, Kind, Token};
use crate::generator::Generator;
use crate::selection::StepRange;
use crate::shape::{
    FillKind, GenCell, NO_TOKEN, Shape, ShapeColumn, Texts, Where, render_tokens_into,
    rhythm_tokens_into,
};
use crate::step::Step;

/// What the rows line under a field says of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// A token writes it.
    Hit,
    /// A token with a chance, `?`, writes it some of the time.
    Chance,
    /// Nothing writes it.
    Nothing,
}

impl Mark {
    /// How the rows line shows it: `x`, `?` or `.`.
    pub fn glyph(self) -> char {
        match self {
            Self::Hit => 'x',
            Self::Chance => '?',
            Self::Nothing => '.',
        }
    }
}

/// A render's cells over its span with the token that wrote each, made on the edit thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    range: StepRange,
    cells: Vec<GenCell>,
    tokens: Vec<u8>,
    /// Whether every row written was written by a token, as a `List`'s are, so a byte of
    /// [`NO_TOKEN`] on one is its 256th value rather than none.
    every: bool,
    /// An `Own` rhythm's hit that chose each row, by its place among the hits, which its field
    /// reads in place of the fill's tokens (decision 461): empty for any other `Where`.
    hits: Vec<u8>,
}

impl Table {
    /// A shape's render over a range, as [`render_tokens_into`] keeps it.
    pub fn render(
        steps: &[Step],
        range: StepRange,
        column: ShapeColumn,
        shape: &Shape,
        texts: Texts,
        source: Option<&[Step]>,
    ) -> Self {
        let len = range.len().min(crate::constants::MAX_PHRASE_STEPS);
        let mut cells = vec![GenCell::Under; len];
        let mut tokens = vec![NO_TOKEN; len];
        render_tokens_into(
            steps,
            range,
            column,
            shape,
            texts,
            source,
            &mut cells,
            &mut tokens,
        );
        let mut hits = Vec::new();
        if matches!(shape.place, Where::Own(_)) {
            hits = vec![NO_TOKEN; len];
            rhythm_tokens_into(steps, range, column, shape, texts.rhythm, &mut hits);
        }
        Self {
            range,
            cells,
            tokens,
            every: shape.kind == FillKind::List,
            hits,
        }
    }

    /// A live generator's on a pass of its phrase, from 0, with its shape as heard: what the edit
    /// thread works out for itself of a pass the engine plays, as passes are positional, so
    /// nothing more crosses from the engine than the pass it reports. None while it covers no
    /// rows.
    pub fn of_generator(generator: &Generator, steps: &[Step], pass: i64) -> Option<Self> {
        let range = generator.rows()?;
        let under = generator.under(steps);
        let shape = generator.heard().on_pass(pass);
        Some(Self::render(
            &under,
            range,
            generator.column,
            &shape,
            generator.texts(),
            None,
        ))
    }

    /// The rows it's over.
    pub fn range(&self) -> StepRange {
        self.range
    }

    /// The token that wrote a stored row, as the fill counts them: none where nothing wrote it or
    /// no token did.
    pub fn token(&self, row: usize) -> Option<u8> {
        let at = row.checked_sub(self.range.first)?;
        if *self.cells.get(at)? == GenCell::Under {
            return None;
        }
        match self.tokens[at] {
            NO_TOKEN if !self.every => None,
            token => Some(token),
        }
    }

    /// The token of a field's grammar that wrote a stored row: an `Own` rhythm's hit, for its
    /// text, and the fill's token for any other.
    fn token_for(&self, field: &Field, row: usize) -> Option<u8> {
        if !std::ptr::eq(field.grammar(), GrammarId::Rhythm.grammar()) {
            return self.token(row);
        }
        let at = row.checked_sub(self.range.first)?;
        if *self.cells.get(at)? == GenCell::Under {
            return None;
        }
        self.hits.get(at).copied().filter(|hit| *hit != NO_TOKEN)
    }

    /// Whether a stored row is written.
    pub fn written(&self, row: usize) -> bool {
        row.checked_sub(self.range.first)
            .and_then(|at| self.cells.get(at))
            .is_some_and(|cell| *cell != GenCell::Under)
    }

    /// The stored rows the token at a place of a field wrote, in order. None while the field's
    /// text doesn't read, as the table is of the last text that did.
    pub fn rows(&self, field: &Field, at: usize) -> Vec<usize> {
        let Some(token) = written_as(field, at) else {
            return Vec::new();
        };
        (self.range.first..=self.range.last)
            .filter(|row| self.token_for(field, *row) == Some(token))
            .collect()
    }

    /// The token of a field that wrote a stored row, by its place, while the text reads.
    pub fn token_of(&self, field: &Field, row: usize) -> Option<usize> {
        let token = self.token_for(field, row)?;
        (0..field.tokens().len()).find(|at| written_as(field, *at) == Some(token))
    }

    /// The rows line: each row of the span, written by a token, by one with a chance, or by
    /// nothing.
    pub fn marks(&self, field: &Field) -> Vec<Mark> {
        (self.range.first..=self.range.last)
            .map(|row| {
                if !self.written(row) {
                    return Mark::Nothing;
                }
                match self.token_of(field, row) {
                    Some(at) if chance(field, at) => Mark::Chance,
                    _ => Mark::Hit,
                }
            })
            .collect()
    }
}

/// The byte a field's token at a place writes as, by its grammar, while its text reads.
fn written_as(field: &Field, at: usize) -> Option<u8> {
    if field.bad().is_some() {
        return None;
    }
    (field.grammar().rows)(field.tokens(), at)
}

/// Whether the token at a place of a field has a chance after it, `?` or `?30`, among the
/// modifiers that follow it.
pub fn chance(field: &Field, at: usize) -> bool {
    field.tokens()[at.saturating_add(1).min(field.tokens().len())..]
        .iter()
        .enumerate()
        .take_while(|(_, token)| token.kind == Kind::Modifier)
        .any(|(after, _)| {
            field
                .token(at + 1 + after)
                .is_some_and(|text| text.starts_with('?'))
        })
}

/// A token's place among those of its kind, as a fill that counts only its values counts it,
/// for a grammar's [`rows`](super::Grammar::rows): none for a token of another kind.
pub fn nth_of(tokens: &[Token], at: usize, kind: Kind) -> Option<u8> {
    if tokens.get(at)?.kind != kind {
        return None;
    }
    let nth = tokens[..at]
        .iter()
        .filter(|token| token.kind == kind)
        .count();
    u8::try_from(nth).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notation::GrammarId;
    use crate::notation::tests::WORDS;
    use crate::scale::{Scale, ScaleKind};
    use crate::shape::{Where, degree_name};

    fn steps(count: usize) -> Vec<Step> {
        vec![Step::default(); count]
    }

    fn minor() -> Scale {
        Scale {
            on: true,
            kind: ScaleKind::Minor,
            ..Scale::default()
        }
    }

    /// A random fill's table holds each row's degree, and `Weights`' digits read it: the rows a
    /// digit lights are those whose note is on its degree, and a row's token is its note's digit.
    #[test]
    fn a_random_fills_rows_are_lit_by_their_degrees_weight() {
        let scale = minor();
        let column = ShapeColumn::Note {
            scale,
            writes: None,
        };
        let shape = Shape {
            kind: FillKind::Random,
            place: Where::AllRows,
            from: 48,
            to: 60,
            seed: 7,
            ..Shape::default()
        };
        let range = StepRange::between(0, 15);
        let table = Table::render(&steps(16), range, column, &shape, Texts::NONE, None);
        let field = Field::new(GrammarId::Weights.grammar(), "FFFFFFF", column);
        let written =
            crate::shape::written_from(&steps(16), range, column, &shape, Texts::NONE, None);
        assert_eq!(written.len(), 16);
        let mut lit = 0;
        for degree in 0..7 {
            let rows = table.rows(&field, degree);
            for row in &rows {
                assert_eq!(table.token_of(&field, *row), Some(degree));
                let crate::step::Note::On(note) = written[*row].1.note else {
                    panic!("a note on every row");
                };
                assert_eq!(
                    crate::shape::degree_of(scale, note),
                    Some(degree),
                    "{} on row {row}",
                    degree_name(scale, degree)
                );
            }
            lit += rows.len();
        }
        assert_eq!(lit, 16, "every row lit by one digit");
        assert!(table.marks(&field).iter().all(|mark| *mark == Mark::Hit));
        // Text that doesn't read lights nothing, as the table is of the last that did.
        let bad = Field::new(GrammarId::Weights.grammar(), "FFF", column);
        assert!(table.rows(&bad, 0).is_empty());
    }

    /// A `List`'s table holds each row's value's place, so two values the same are told apart,
    /// and a list shorter than the rows goes round.
    #[test]
    fn a_lists_rows_are_lit_by_their_values_place() {
        let column = ShapeColumn::Velocity;
        let shape = Shape {
            kind: FillKind::List,
            place: Where::AllRows,
            ..Shape::default()
        };
        let mut steps = steps(6);
        for step in &mut steps {
            step.note = crate::step::Note::On(60);
        }
        let range = StepRange::between(0, 5);
        let table = Table::render(
            &steps,
            range,
            column,
            &shape,
            Texts::list(&[0x40, 0x40, 0x7F]),
            None,
        );
        let field = Field::new(&WORDS, "40 40 7F", column);
        assert_eq!(table.rows(&field, 0), [0, 3]);
        assert_eq!(table.rows(&field, 1), [1, 4]);
        assert_eq!(table.rows(&field, 2), [2, 5]);
        assert_eq!(table.token_of(&field, 5), Some(2));
    }

    /// A full list's 256th value is told from no token, as a `List` writes every row from one.
    #[test]
    fn a_full_lists_last_value_is_lit() {
        let shape = Shape {
            kind: FillKind::List,
            place: Where::AllRows,
            ..Shape::default()
        };
        let list: Vec<u8> = (0..=255).collect();
        let mut steps = steps(256);
        for step in &mut steps {
            step.note = crate::step::Note::On(60);
        }
        let range = StepRange::between(0, 255);
        let table = Table::render(
            &steps,
            range,
            ShapeColumn::Velocity,
            &shape,
            Texts::list(&list),
            None,
        );
        assert_eq!(table.token(255), Some(255));
        assert_eq!(table.token(254), Some(254));
    }

    /// The rows line marks a row a token with a chance writes with `?`, and an unwritten one `.`.
    #[test]
    fn the_rows_line_says_hits_chances_and_nothing() {
        let column = ShapeColumn::Velocity;
        let shape = Shape {
            kind: FillKind::List,
            place: Where::AllRows,
            ..Shape::default()
        };
        let mut steps = steps(4);
        for step in &mut steps[..3] {
            step.note = crate::step::Note::On(60);
        }
        // On the notes alone, so row 3 is left alone.
        let shape = Shape {
            place: Where::Notes,
            ..shape
        };
        let range = StepRange::between(0, 3);
        let table = Table::render(
            &steps,
            range,
            column,
            &shape,
            Texts::list(&[0x10, 0x20]),
            None,
        );
        let field = Field::new(&WORDS, "10 20?30", column);
        assert!(chance(&field, 1));
        assert!(!chance(&field, 0));
        let marks: String = table
            .marks(&field)
            .iter()
            .map(|mark| mark.glyph())
            .collect();
        assert_eq!(marks, "x?x.");
    }

    /// A live generator that varies has a table of its own for each pass, worked out from the
    /// pass alone, as the engine plays it.
    #[test]
    fn a_pass_after_the_first_is_rendered_from_its_number() {
        let column = ShapeColumn::Note {
            scale: minor(),
            writes: None,
        };
        let shape = Shape {
            kind: FillKind::Random,
            place: Where::AllRows,
            from: 48,
            to: 72,
            seed: 11,
            vary: 1,
            ..Shape::default()
        };
        let range = StepRange::between(0, 15);
        let mut phrase = crate::song::Phrase::from_steps(steps(16));
        let generator = Generator::new(column, crate::generator::Span::rows(range), shape);
        assert!(phrase.add_generator(generator, None));
        let generator = &phrase.generators()[0];
        let under = generator.under(phrase.steps());
        let first = Table::of_generator(generator, phrase.steps(), 0).unwrap();
        assert_eq!(
            first,
            Table::render(&under, range, column, &shape, Texts::NONE, None)
        );
        let second = Table::of_generator(generator, phrase.steps(), 1).unwrap();
        let heard = shape.on_pass(1);
        assert_ne!(heard.seed, shape.seed, "it varies");
        assert_eq!(
            second,
            Table::render(&under, range, column, &heard, Texts::NONE, None)
        );
        assert_ne!(first, second);
    }

    /// A `Tonnetz`'s table holds the place in its list of the move that reached each row's chord
    /// (decision 473): the first row, the chord it starts on, none; a list going round lights each
    /// move's rows; a move left out for the scale, none, its rows the next move's; and a walk
    /// whose moves are drawn, none, as its list isn't what writes them.
    #[test]
    fn a_tonnetzs_rows_are_lit_by_the_move_that_reached_them() {
        use crate::shape::{MoveList, Moves, Tonnetz, TonnetzMove};
        let column = ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        };
        let walk = |list: &[TonnetzMove], moves: Moves| Shape {
            kind: FillKind::Tonnetz,
            place: Where::AllRows,
            from: 60,
            tonnetz: Tonnetz {
                list: MoveList::new(list),
                moves,
                ..Tonnetz::default()
            },
            ..Shape::default()
        };
        let range = StepRange::between(0, 8);
        let (p, l, r) = (TonnetzMove::P, TonnetzMove::L, TonnetzMove::R);
        let shape = walk(&[p, l, r, l], Moves::List);
        let table = Table::render(&steps(9), range, column, &shape, Texts::NONE, None);
        let field = Field::new(GrammarId::Moves.grammar(), "P L R L", column);
        assert_eq!(table.token_of(&field, 0), None, "the chord it starts on");
        assert_eq!(table.rows(&field, 0), [1, 5]);
        assert_eq!(table.rows(&field, 1), [2, 6]);
        assert_eq!(table.rows(&field, 2), [3, 7]);
        assert_eq!(table.rows(&field, 3), [4, 8]);
        let marks: String = table
            .marks(&field)
            .iter()
            .map(|mark| mark.glyph())
            .collect();
        assert_eq!(marks, "xxxxxxxxx");
        // In C major `P` is never taken, so `L` and `R` write its rows.
        let major = ShapeColumn::Note {
            scale: Scale {
                on: true,
                kind: ScaleKind::Major,
                ..Scale::default()
            },
            writes: None,
        };
        let in_scale = Shape {
            tonnetz: Tonnetz {
                in_scale: true,
                ..shape.tonnetz
            },
            ..shape
        };
        let table = Table::render(&steps(9), range, major, &in_scale, Texts::NONE, None);
        let field = Field::new(GrammarId::Moves.grammar(), "P L R L", major);
        assert!(table.rows(&field, 0).is_empty(), "P is left out");
        let lit: usize = (0..4).map(|at| table.rows(&field, at).len()).sum();
        assert_eq!(lit, 8, "every row after the first by L or R");
        let random = walk(&[p, l, r, l], Moves::Random);
        let table = Table::render(&steps(9), range, column, &random, Texts::NONE, None);
        assert!((0..4).all(|at| table.rows(&field, at).is_empty()));
    }

    #[test]
    fn a_tokens_place_among_its_kind() {
        let field = Field::new(&WORDS, "10 [. 20*2] 30", ShapeColumn::Velocity);
        let tokens = field.tokens();
        assert_eq!(nth_of(tokens, 0, Kind::Value), Some(0));
        assert_eq!(nth_of(tokens, 1, Kind::Value), None, "a bracket");
        assert_eq!(nth_of(tokens, 3, Kind::Value), Some(1));
        assert_eq!(nth_of(tokens, 6, Kind::Value), Some(2));
    }
}
