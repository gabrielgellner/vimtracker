//! A generator's `Where` is a selection of the span's rows (decision 448): `Notes`, `NoNotes` and
//! `Commands` choose by what's there, and every other is a pattern counted over the rows its
//! `Over` says, every row on the Note column and an FX column and the notes on Velocity, Nudge and
//! Length. A row outside the selection is never touched; a row inside is written, a note of its
//! own where there was none, unless the column can't change it.

use vt_model::scale::Scale;
use vt_model::selection::StepRange;
use vt_model::shape::{
    BEATS, CellStart, Cells, FillKind, Over, Role, Shape, ShapeColumn, Where, World, written,
};
use vt_model::step::{Note, Step};

const NOTES: ShapeColumn = ShapeColumn::Note {
    scale: Scale {
        on: false,
        root: 0,
        kind: vt_model::scale::ScaleKind::Major,
    },
    writes: Some(0x64),
};

fn steps(texts: &[&str]) -> Vec<Step> {
    texts.iter().map(|text| text.parse().unwrap()).collect()
}

/// `count` empty rows with a note on each of `notes`.
fn with_notes(count: usize, notes: &[usize]) -> Vec<Step> {
    let mut rows = vec![Step::default(); count];
    for &row in notes {
        rows[row] = "C-3 40 --- --- ---".parse().unwrap();
    }
    rows
}

fn all(steps: &[Step]) -> StepRange {
    StepRange::between(0, steps.len() - 1)
}

fn shape(kind: FillKind, place: Where, over: Option<Over>) -> Shape {
    Shape {
        from: 48,
        to: 60,
        kind,
        place,
        over,
        seed: 7,
        ..Shape::default()
    }
}

/// The rows a shape writes.
fn rows_of(steps: &[Step], column: ShapeColumn, shape: &Shape) -> Vec<usize> {
    written(steps, all(steps), column, shape)
        .into_iter()
        .filter(|(row, step)| steps[*row] != *step)
        .map(|(row, _)| row)
        .collect()
}

/// The rows a shape writes or leaves, whatever it writes there.
fn touched(steps: &[Step], column: ShapeColumn, shape: &Shape) -> Vec<usize> {
    written(steps, all(steps), column, shape)
        .into_iter()
        .map(|(row, _)| row)
        .collect()
}

const EUCLID_3: Where = Where::Euclid { hits: 3, rotate: 0 };

/// The report that started it: `Acid` on `Euclid` on a Note column with notes changed only the
/// notes. Now it plays on Euclid's hits over every row, a note of its own where there was none,
/// and leaves the notes off the hits as they were, none taken away.
#[test]
fn acid_on_euclid_over_a_column_with_notes_plays_on_the_hits() {
    let before = with_notes(8, &[1, 3, 4, 6]);
    let acid = shape(FillKind::Acid, EUCLID_3, None);
    let out = written(&before, all(&before), NOTES, &acid);
    let rows: Vec<usize> = out.iter().map(|(row, _)| *row).collect();
    // E(3,8) over the 8 rows: x..x..x.
    assert_eq!(rows, [0, 3, 6]);
    for (row, step) in &out {
        assert!(matches!(step.note, Note::On(_)), "a note on {row}");
    }
    // Row 0 had none and takes one at the column's velocity or an accent's.
    let first = &out[0].1;
    assert!(matches!(first.velocity, Some(0x64 | 0x7F)), "{first}");
    // The notes off the hits are as they were, and nothing is cleared.
    let mut after = before.clone();
    for (row, step) in out {
        after[row] = step;
    }
    for row in [1, 4] {
        assert_eq!(
            after[row], before[row],
            "row {row} is outside the selection"
        );
    }
    let notes = after
        .iter()
        .filter(|step| matches!(step.note, Note::On(_)))
        .count();
    assert_eq!(notes, 5, "the four, and one of its own on row 0");
}

/// On an empty column a pattern writes where it always did: the rows it chooses there are the same
/// whether the column has notes or not, as it counts every row either way.
#[test]
fn a_pattern_counts_every_row_on_the_note_column_whatever_is_on_them() {
    let empty = vec![Step::default(); 16];
    let notes = with_notes(16, &[1, 2, 5, 9, 10, 13]);
    let cells = Cells {
        rule: 90,
        start: CellStart::One,
        across: None,
        stepped: 0,
    };
    let places = [
        Where::AllRows,
        Where::Every(3),
        Where::Chance(40),
        EUCLID_3,
        Where::Euclid { hits: 5, rotate: 2 },
        Where::Clusters(40),
        Where::Markov(70),
        Where::States(80),
        Where::Complementary {
            share: 60,
            second: false,
        },
        Where::Rhythm {
            rhythm: World::SonClave,
            part: Role::Bell,
            child: 100,
        },
        Where::Cells(cells),
        BEATS,
    ];
    for place in places {
        let ramp = shape(FillKind::Ramp, place, None);
        assert_eq!(
            touched(&notes, NOTES, &ramp),
            touched(&empty, NOTES, &ramp),
            "{place:?}"
        );
    }
    // Euclid over the 16: 3 hits at 0, 5 and 10, whatever is there.
    assert_eq!(
        touched(&empty, NOTES, &shape(FillKind::Ramp, EUCLID_3, None)),
        [0, 5, 10]
    );
    assert_eq!(
        touched(&notes, NOTES, &shape(FillKind::Ramp, EUCLID_3, None)),
        [0, 5, 10]
    );
}

/// The content Wheres choose as they did.
#[test]
fn notes_no_notes_and_commands_choose_by_what_is_there() {
    let rows = steps(&[
        "C-3 40 cut:10 --- ---",
        "--- -- --- --- ---",
        "OFF -- cut:20 --- ---",
        "D-3 -- res:30 --- ---",
        "--- -- --- --- ---",
    ]);
    let ramp = |place| shape(FillKind::Ramp, place, None);
    assert_eq!(touched(&rows, NOTES, &ramp(Where::Notes)), [0, 3]);
    assert_eq!(
        touched(&rows, NOTES, &ramp(Where::NoNotes)),
        [1, 4],
        "the OFF stays"
    );
    assert_eq!(touched(&rows, NOTES, &ramp(Where::AllRows)), [0, 1, 3, 4]);
    let cut = ShapeColumn::Fx {
        slot: 0,
        command: vt_model::step::FxCommand::Set(vt_model::settings::SettingId::CUT),
    };
    assert_eq!(touched(&rows, cut, &ramp(Where::Commands)), [0, 2]);
    assert_eq!(touched(&rows, cut, &ramp(Where::Notes)), [0], "res is kept");
    assert_eq!(touched(&rows, cut, &ramp(Where::NoNotes)), [1, 2, 4]);
    assert_eq!(
        touched(&rows, ShapeColumn::Velocity, &ramp(Where::Notes)),
        [0, 3]
    );
}

/// The user's example: 7 notes in 16 rows and `Euclid 3`. Over the notes it's E(3,7), `x.x.x..`,
/// on the first, third and fifth notes; over every row it's E(3,16) on the rows, and on Velocity
/// only the hits that land on a note change one.
#[test]
fn over_notes_counts_the_notes_and_over_all_rows_the_rows() {
    let notes = [0, 2, 3, 7, 9, 12, 14];
    let rows = with_notes(16, &notes);
    let over = |over| shape(FillKind::Ramp, EUCLID_3, over);
    let on_notes = touched(&rows, NOTES, &over(Some(Over::Notes)));
    assert_eq!(on_notes, [notes[0], notes[2], notes[4]]);
    assert_eq!(touched(&rows, NOTES, &over(None)), [0, 5, 10]);
    assert_eq!(
        touched(&rows, NOTES, &over(Some(Over::AllRows))),
        [0, 5, 10]
    );
    // Velocity counts the notes unless told otherwise, and over every row skips the hits
    // without a note: 5 and 10 have none.
    let velocity = ShapeColumn::Velocity;
    assert_eq!(touched(&rows, velocity, &over(None)), on_notes);
    assert_eq!(touched(&rows, velocity, &over(Some(Over::AllRows))), [0]);
    // `Every` and a named rhythm count the same way.
    let every = |over| shape(FillKind::Ramp, Where::Every(2), over);
    assert_eq!(
        touched(&rows, NOTES, &every(Some(Over::Notes))),
        [0, 3, 9, 14]
    );
    assert_eq!(
        touched(&rows, NOTES, &every(None)),
        [0, 2, 4, 6, 8, 10, 12, 14]
    );
    assert_eq!(
        touched(&rows, velocity, &every(Some(Over::AllRows))),
        [0, 2, 12, 14]
    );
    let clave = |over| {
        shape(
            FillKind::Ramp,
            Where::Rhythm {
                rhythm: World::SonClave,
                part: Role::Bell,
                child: 100,
            },
            over,
        )
    };
    let by_rows = touched(&vec![Step::default(); 16], NOTES, &clave(None));
    let by_notes = touched(&rows, NOTES, &clave(Some(Over::Notes)));
    // The clave's pulses, counted a note at a time, land on the notes in its place.
    let mapped: Vec<usize> = by_rows
        .iter()
        .filter(|&&pulse| pulse < notes.len())
        .map(|&pulse| notes[pulse])
        .collect();
    assert_eq!(by_notes, mapped);
    // The Marbles' rhythms and Cells run among the notes alone.
    for place in [Where::Clusters(50), Where::Markov(60), Where::States(80)] {
        let chain = touched(
            &rows,
            NOTES,
            &shape(FillKind::Ramp, place, Some(Over::Notes)),
        );
        assert!(chain.iter().all(|row| notes.contains(row)), "{place:?}");
    }
}

/// On a column of a note's own setting, a row selected without a note has nothing to change and is
/// passed over.
#[test]
fn velocity_skips_the_rows_it_selects_without_a_note() {
    let rows = with_notes(8, &[0, 5]);
    let ramp = shape(FillKind::Ramp, Where::AllRows, None);
    assert_eq!(rows_of(&rows, ShapeColumn::Velocity, &ramp), [0, 5]);
    let every = shape(FillKind::Ramp, Where::Every(1), Some(Over::AllRows));
    assert_eq!(touched(&rows, ShapeColumn::Velocity, &every), [0, 5]);
    assert_eq!(
        touched(&rows, ShapeColumn::Length, &every),
        [0, 5],
        "a length needs its note too"
    );
}

/// `Beats` is `Acid`'s old rhythm for any fill: at 100% every row, at 0% none, the strong beats
/// most of all between.
#[test]
fn beats_chooses_about_its_share_of_the_rows_for_any_fill() {
    let empty = vec![Step::default(); 64];
    let at = |percent| {
        touched(
            &empty,
            ShapeColumn::Velocity,
            &shape(FillKind::Ramp, Where::Beats(percent), None),
        )
        .len()
    };
    assert_eq!(at(0), 0);
    let notes = with_notes(64, &(0..64).collect::<Vec<_>>());
    let on = |percent| {
        touched(
            &notes,
            ShapeColumn::Velocity,
            &shape(FillKind::Ramp, Where::Beats(percent), None),
        )
        .len()
    };
    assert_eq!(on(100), 64);
    assert_eq!(on(0), 0);
    let half = on(50);
    assert!((22..=42).contains(&half), "{half}");
}
