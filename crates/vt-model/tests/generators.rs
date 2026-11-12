//! Live generators: the shape panel kept live on a phrase's column, through `edit`.

use std::fs;
use std::sync::Arc;

use vt_model::action::{Action, FieldValue, edit};
use vt_model::generator::{Span, applied};
use vt_model::lfo::Shape as Wave;
use vt_model::project::{load_song, save_song};
use vt_model::scale::Scale;
use vt_model::selection::StepRange;
use vt_model::shape::{DEFAULT_RATE, FillKind, Shape, ShapeColumn, Values, Where};
use vt_model::song::{Lane, Phrase, Song};
use vt_model::step::{FxCommand, Note, Step};

const NOTE: &str = "C-3 40 --- --- ---";
const EMPTY: &str = "--- -- --- --- ---";
const CUT: ShapeColumn = ShapeColumn::Fx {
    slot: 0,
    command: FxCommand::CUT,
};

fn song_with(steps: &[&str]) -> Arc<Song> {
    let mut song = Song::new(1);
    let steps: Vec<Step> = steps.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    Arc::new(song)
}

fn phrase(song: &Song) -> &Phrase {
    &song.tracks[0].phrases[&0]
}

fn texts(song: &Song) -> Vec<String> {
    phrase(song)
        .steps()
        .iter()
        .map(ToString::to_string)
        .collect()
}

/// Each row's velocity, `--` for none.
fn velocities(song: &Song) -> Vec<String> {
    texts(song)
        .iter()
        .map(|text| text[4..6].to_owned())
        .collect()
}

/// Each row's first FX slot.
fn slots(song: &Song) -> Vec<String> {
    phrase(song)
        .steps()
        .iter()
        .map(|step| step.fx[0].to_string())
        .collect()
}

fn ramp(from: u8, to: u8, place: Where) -> Shape {
    Shape {
        from,
        to,
        curve: 0,
        s: 0,
        kind: FillKind::Ramp,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place,
        seed: 7,
    }
}

fn all(rows: usize) -> Span {
    Span::rows(StepRange::between(0, rows - 1))
}

fn generate(song: &Arc<Song>, column: ShapeColumn, span: Span, shape: Shape) -> Arc<Song> {
    edit(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column,
            span,
            shape,
        },
    )
    .unwrap()
}

fn reshape(song: &Arc<Song>, generator: usize, shape: Shape) -> Arc<Song> {
    edit(
        song,
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator,
            shape,
        },
    )
    .unwrap()
}

fn set_velocity(song: &Arc<Song>, step: usize, velocity: u8) -> Arc<Song> {
    edit(
        song,
        &Action::SetField {
            track: 0,
            phrase: 0,
            step,
            value: FieldValue::Velocity(Some(velocity)),
        },
    )
    .unwrap()
}

fn set_note(song: &Arc<Song>, step: usize, note: Note) -> Arc<Song> {
    edit(
        song,
        &Action::SetNote {
            track: 0,
            phrase: 0,
            step,
            note,
        },
    )
    .unwrap()
}

#[test]
fn a_live_generator_writes_what_the_panel_would_and_stays_live() {
    let start = song_with(&[NOTE, NOTE, EMPTY, NOTE]);
    let shape = ramp(0x10, 0x40, Where::Notes);
    let live = generate(&start, ShapeColumn::Velocity, all(4), shape);
    let once = edit(
        &start,
        &Action::Shape {
            track: 0,
            phrase: 0,
            range: StepRange::between(0, 3),
            column: ShapeColumn::Velocity,
            shape,
        },
    )
    .unwrap();
    assert_eq!(texts(&live), texts(&once));
    assert_eq!(velocities(&live), ["10", "20", "--", "40"]);
    assert_eq!(phrase(&live).generators().len(), 1);
    assert!(phrase(&once).generators().is_empty());
}

#[test]
fn a_rerender_changes_its_own_cells_only_and_stays_live() {
    let start = song_with(&[
        "C-3 40 cut:10 --- ---",
        "D-3 40 cut:20 --- ---",
        "E-3 40 cut:30 --- ---",
        "F-3 40 cut:40 --- ---",
    ]);
    let live = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x00, 0x30, Where::Notes),
    );
    let reshaped = reshape(&live, 0, ramp(0x70, 0x10, Where::Notes));
    assert_eq!(velocities(&reshaped), ["70", "50", "30", "10"]);
    let others = |song: &Song| -> Vec<String> {
        texts(song)
            .iter()
            .map(|text| format!("{}{}", &text[..3], &text[6..]))
            .collect()
    };
    assert_eq!(others(&reshaped), others(&start));
    assert_eq!(phrase(&reshaped).generators().len(), 1);
    assert!(applied(&live, &reshaped).is_empty());
}

#[test]
fn a_hand_edit_to_its_cells_applies_it_and_to_another_column_does_not() {
    let start = song_with(&[NOTE, NOTE, NOTE, NOTE]);
    let live = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Notes),
    );
    // A note's pitch is read, not owned, and changes nothing.
    let other = set_note(&live, 1, Note::On(64));
    assert_eq!(phrase(&other).generators().len(), 1);
    assert_eq!(velocities(&other), velocities(&live));
    // A velocity typed over one of its cells applies it: every value stays as it is.
    let edited = set_velocity(&live, 2, 0x7F);
    assert!(phrase(&edited).generators().is_empty());
    assert_eq!(velocities(&edited), ["10", "20", "7F", "40"]);
    assert_eq!(applied(&live, &edited), [ShapeColumn::Velocity]);
    // Undo is the version before, where it's live.
    assert_eq!(phrase(&live).generators().len(), 1);
}

#[test]
fn a_note_added_under_a_velocity_generator_rerenders_it_in_the_same_edit() {
    let start = song_with(&[NOTE, EMPTY, NOTE, EMPTY, NOTE]);
    let live = generate(
        &start,
        ShapeColumn::Velocity,
        all(5),
        ramp(0x00, 0x40, Where::Notes),
    );
    assert_eq!(velocities(&live), ["00", "--", "20", "--", "40"]);
    // Adding a note as Insert does, with a velocity of its own, gives it a generated one.
    let added = edit(
        &live,
        &Action::SetStep {
            track: 0,
            phrase: 0,
            step: 1,
            value: "C-3 7F --- --- ---".parse().unwrap(),
        },
    )
    .unwrap();
    assert_eq!(phrase(&added).generators().len(), 1);
    assert_eq!(velocities(&added), ["00", "10", "20", "--", "40"]);
    // Taking it away again puts the ramp back as it was, with the note's own velocity under it.
    let gone = set_note(&added, 1, Note::Empty);
    assert_eq!(phrase(&gone).generators().len(), 1);
    assert_eq!(velocities(&gone), ["00", "7F", "20", "--", "40"]);
}

#[test]
fn a_euclidean_rotate_moves_the_hits_and_gives_the_rows_left_back_their_values() {
    let start = song_with(&[NOTE; 8]);
    let tresillo = |rotate| {
        let mut shape = ramp(0x7F, 0x7F, Where::Euclid { hits: 3, rotate });
        shape.kind = FillKind::Alternate;
        shape
    };
    let live = generate(&start, ShapeColumn::Velocity, all(8), tresillo(0));
    assert_eq!(
        velocities(&live),
        ["7F", "40", "40", "7F", "40", "40", "7F", "40"]
    );
    let rotated = reshape(&live, 0, tresillo(1));
    assert_eq!(
        velocities(&rotated),
        ["40", "7F", "40", "40", "7F", "40", "40", "7F"]
    );
    // A transform reworks what's under it each time, so it doesn't drift.
    let mut reverse = tresillo(0);
    reverse.kind = FillKind::Reverse;
    let once = reshape(&rotated, 0, reverse);
    let twice = reshape(&once, 0, reverse);
    assert_eq!(texts(&once), texts(&twice));
    assert_eq!(texts(&once), texts(&start));
}

#[test]
fn on_an_fx_column_with_a_lane_it_owns_the_lanes_steps_and_follows_the_lane() {
    let start = song_with(&[
        "C-3 40 cut:00 --- ---",
        NOTE,
        NOTE,
        "C-3 40 cut:55 --- ---",
        NOTE,
        NOTE,
    ]);
    let laned = edit(
        &start,
        &Action::SetLane {
            track: 0,
            phrase: 0,
            slot: 0,
            lane: Some(Lane {
                start: 0,
                length: 3,
            }),
        },
    )
    .unwrap();
    let live = generate(&laned, CUT, Span::Lane, ramp(0x00, 0x40, Where::AllRows));
    assert_eq!(
        phrase(&live).generators()[0].rows(),
        Some(StepRange::between(0, 2))
    );
    assert_eq!(
        slots(&live),
        ["cut:00", "cut:20", "cut:40", "cut:55", "---", "---"]
    );
    // A longer lane is filled again, as a loop of its own.
    let longer = edit(
        &live,
        &Action::SetLane {
            track: 0,
            phrase: 0,
            slot: 0,
            lane: Some(Lane {
                start: 0,
                length: 5,
            }),
        },
    )
    .unwrap();
    assert_eq!(
        slots(&longer),
        ["cut:00", "cut:10", "cut:20", "cut:30", "cut:40", "---"]
    );
    // A shorter one gives the rows it leaves back what was under them.
    let shorter = edit(
        &longer,
        &Action::SetLane {
            track: 0,
            phrase: 0,
            slot: 0,
            lane: Some(Lane {
                start: 2,
                length: 2,
            }),
        },
    )
    .unwrap();
    assert_eq!(
        slots(&shorter),
        ["cut:00", "---", "cut:00", "cut:40", "---", "---"]
    );
    // Following the notes again applies it.
    let gone = edit(
        &shorter,
        &Action::SetLane {
            track: 0,
            phrase: 0,
            slot: 0,
            lane: None,
        },
    )
    .unwrap();
    assert!(phrase(&gone).generators().is_empty());
    assert_eq!(slots(&gone), slots(&shorter));
}

#[test]
fn two_generators_on_different_columns_are_independent() {
    let start = song_with(&["C-3 40 cut:10 --- ---"; 4]);
    let one = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Notes),
    );
    let two = generate(&one, CUT, all(4), ramp(0x00, 0x30, Where::Commands));
    assert_eq!(phrase(&two).generators().len(), 2);
    let reshaped = reshape(&two, 1, ramp(0x30, 0x00, Where::Commands));
    assert_eq!(velocities(&reshaped), velocities(&one));
    assert_eq!(slots(&reshaped), ["cut:30", "cut:20", "cut:10", "cut:00"]);
    // Applying one by hand leaves the other live.
    let edited = set_velocity(&reshaped, 0, 0x01);
    assert_eq!(phrase(&edited).generators().len(), 1);
    assert_eq!(phrase(&edited).generators()[0].column, CUT);
    let moved = reshape(&edited, 0, ramp(0x00, 0x30, Where::Commands));
    assert_eq!(slots(&moved), ["cut:00", "cut:10", "cut:20", "cut:30"]);
    assert_eq!(velocities(&moved)[0], "01");
}

#[test]
fn a_new_generator_overlapping_one_on_its_column_applies_it_first() {
    let start = song_with(&[NOTE; 4]);
    let one = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Notes),
    );
    let two = generate(
        &one,
        ShapeColumn::Velocity,
        Span::rows(StepRange::between(2, 3)),
        ramp(0x70, 0x70, Where::Notes),
    );
    assert_eq!(phrase(&two).generators().len(), 1);
    assert_eq!(velocities(&two), ["10", "20", "70", "70"]);
    // What's under the new one is what the old one left.
    let back = reshape(&two, 0, ramp(0x00, 0x00, Where::Every(8)));
    assert_eq!(velocities(&back), ["10", "20", "00", "40"]);
}

#[test]
fn doubling_applies_its_generators_and_rotating_those_on_the_rows_it_moves() {
    let start = song_with(&[NOTE; 4]);
    let live = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Notes),
    );
    let doubled = edit(
        &live,
        &Action::Double {
            track: 0,
            phrase: 0,
        },
    )
    .unwrap();
    assert!(phrase(&doubled).generators().is_empty());
    assert_eq!(velocities(&doubled)[4..], ["10", "20", "30", "40"]);
    let rotated = edit(
        &live,
        &Action::RotateLoop {
            track: 0,
            phrase: 0,
            by: 1,
        },
    )
    .unwrap();
    assert!(phrase(&rotated).generators().is_empty());
    assert_eq!(velocities(&rotated), ["40", "10", "20", "30"]);
    // A longer loop keeps it, as every step is kept.
    let longer = edit(
        &live,
        &Action::SetLength {
            track: 0,
            phrase: 0,
            length: 8,
        },
    )
    .unwrap();
    assert_eq!(phrase(&longer).generators().len(), 1);
}

#[test]
fn sliding_the_loop_back_past_the_first_step_keeps_it_on_its_rows() {
    let start = song_with(&[NOTE; 4]);
    let live = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Notes),
    );
    let slid = edit(
        &live,
        &Action::SlideLoop {
            track: 0,
            phrase: 0,
            by: -2,
        },
    )
    .unwrap();
    assert_eq!(phrase(&slid).generators().len(), 1);
    assert_eq!(
        phrase(&slid).generators()[0].rows(),
        Some(StepRange::between(2, 5))
    );
    let reshaped = reshape(&slid, 0, ramp(0x40, 0x10, Where::Notes));
    assert_eq!(velocities(&reshaped)[2..], ["40", "30", "20", "10"]);
}

#[test]
fn a_live_generator_round_trips_through_saving() {
    let start = song_with(&[
        "C-3 40 cut:00 --- ---",
        "D#3 40 --- --- ---",
        "C-3 40 cut:40 --- ---",
        NOTE,
    ]);
    let note = ShapeColumn::Note {
        scale: Scale::default(),
    };
    let mut wave = ramp(0x30, 0x50, Where::Notes);
    wave.kind = FillKind::List;
    wave.values = Values::new(&[0x30, 0x40, 0x48]);
    let one = generate(
        &start,
        ShapeColumn::Velocity,
        all(4),
        ramp(0x10, 0x40, Where::Euclid { hits: 3, rotate: 1 }),
    );
    let two = generate(&one, CUT, all(4), ramp(0x00, 0x40, Where::Commands));
    let three = generate(&two, note, all(4), wave);
    let folder = std::env::temp_dir().join(format!("vt-generators-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    save_song(&folder, &three).unwrap();
    let text = fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(text.contains("generators"), "{text}");
    let loaded = load_song(&folder).unwrap();
    let _ = fs::remove_dir_all(&folder);
    assert_eq!(phrase(&loaded), phrase(&three));
    // Still live after a reload: a reshape works, and a hand edit applies it.
    let loaded = Arc::new(loaded);
    let reshaped = reshape(
        &loaded,
        0,
        ramp(0x40, 0x10, Where::Euclid { hits: 3, rotate: 1 }),
    );
    assert_eq!(phrase(&reshaped).generators().len(), 3);
    let edited = set_velocity(&reshaped, 0, 0x01);
    assert_eq!(phrase(&edited).generators().len(), 2);
    // A song with none saves no generators at all.
    assert!(!ron::ser::to_string(&*start).unwrap().contains("generators"));
}
