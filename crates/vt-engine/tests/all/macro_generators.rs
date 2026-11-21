//! Macros reaching live generators' settings as the engine plays (decision 300): a macro's turn,
//! a step's `MAC` lock and an LFO on it each hold the generator settings it maps where it has them
//! that block, so the engine renders those generators again; at the macro's turn, what's heard is
//! what's written. What each row was heard as is reported, and writing those rows in plays the
//! same.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::gens::{GenReport, GenRow};
use vt_model::action::{Action, edit};
use vt_model::bloom::Bloom;
use vt_model::generator::Span;
use vt_model::lfo::{Lfo, Rate};
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::project::{load_song, save_song};
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, GenCell, GenParam, Shape, ShapeColumn, Where};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// 5625 frames a step at 128 BPM.
const STEP: usize = 5625;

/// Three hits of `7F` spread over sixteen rows, `40` between.
fn tresillo() -> Shape {
    Shape {
        from: 0x7F,
        to: 0x7F,
        kind: FillKind::Alternate,
        place: Where::Euclid { hits: 3, rotate: 0 },
        seed: 7,
        ..Shape::default()
    }
}

fn apply(song: Song, action: &Action) -> Song {
    Arc::unwrap_or_clone(edit(&Arc::new(song), action).expect("the edit applies"))
}

/// One synth track of sixteen notes at `40`, these rows written as given, with the tresillo live
/// on their velocities and macro 1 holding its `Hits` from 1 to 16, turned to 3.
fn song(rows: &[(usize, &str)]) -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let mut steps: Vec<Step> = vec!["C-3 40 --- --- ---".parse().unwrap(); 16];
    for (row, text) in rows {
        steps[*row] = text.parse().unwrap();
    }
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Velocity,
            span: Span::rows(StepRange::between(0, 15)),
            shape: tresillo(),
            list: Box::default(),
            rhythm: Box::default(),
        },
    );
    let hits = MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Hits,
    };
    let song = apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits, (1, 16)),
        },
    );
    // 1 + 15 × 0x22 / 0xFF is 3.
    apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x2200,
        },
    )
}

/// The song with its rows as plain values, no generator and no macro: what's written.
fn plain(song: &Song, steps: Vec<Step>) -> Song {
    let mut plain = song.clone();
    let track = &mut plain.tracks[0];
    track.macros = Default::default();
    track.mods = Song::new(1).tracks[0].mods;
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    plain
}

fn written(song: &Song) -> Vec<Step> {
    song.tracks[0].phrases[&0].steps().to_vec()
}

/// Play a song from the top for so many steps, a step at a time, and what was heard of its
/// generators' rows, in order.
fn play(song: &Song, steps: usize) -> (Vec<f32>, Vec<GenRow>, Engine) {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = Vec::with_capacity(steps * STEP * 2);
    let mut rows = Vec::new();
    let mut block = vec![0.0; STEP * 2];
    for _ in 0..steps {
        engine.process(&mut block);
        out.extend_from_slice(&block);
        engine.take_gen_reports(|_, report| {
            if let GenReport::Row(row) = report {
                rows.push(row);
            }
        });
    }
    (out, rows, engine)
}

/// The rows as they were last heard: each written row with the cell heard on it laid over, as
/// `Space g a` keeps them.
fn as_heard(song: &Song, heard: &[GenRow]) -> Vec<Step> {
    let phrase = &song.tracks[0].phrases[&0];
    let mut steps = phrase.steps().to_vec();
    for row in heard {
        let generator = &phrase.generators()[row.generator];
        steps[row.row] = match row.cell {
            Some(cell) => generator.heard_row(phrase.steps(), row.row, cell).unwrap(),
            None => phrase.steps()[row.row].clone(),
        };
    }
    steps
}

#[test]
fn a_macro_at_its_turn_leaves_its_generator_at_rest() {
    let song = song(&[]);
    let hits = written(&song)
        .iter()
        .filter(|step| step.velocity == Some(0x7F))
        .count();
    assert_eq!(hits, 3, "the macro's turn is written");
    let (out, rows, engine) = play(&song, 32);
    let (as_written, ..) = play(&plain(&song, written(&song)), 32);
    assert!(out == as_written, "it plays sample for sample as written");
    assert!(engine.generators_heard(0).all(|run| run.renders == 0));
    assert_eq!(rows.len(), 32, "each row is reported");
    assert!(rows.iter().all(|row| row.cell.is_none()), "as written");
}

/// A song saved and read back from a scratch folder of its own.
fn loaded(song: &Song, name: &str) -> Song {
    let folder = std::env::temp_dir().join(format!("vt-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    save_song(&folder, song).unwrap();
    let loaded = load_song(&folder).unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    loaded
}

/// A Bloom branch on phrase `00`, a `Follow` of the trunk on `10` making three changes, with
/// macro 1 holding its `Mutation` from 0 to 32, turned to 8 (decision 307).
fn bloom_branch() -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let line: Vec<Step> = [
        "C-3 64 --- --- ---",
        "--- -- --- --- ---",
        "D#3 50 --- --- ---",
    ]
    .iter()
    .cycle()
    .take(16)
    .map(|row| row.parse().unwrap())
    .collect();
    song.tracks[0].phrases.clear();
    song.tracks[0]
        .phrases
        .insert(0x10, Arc::new(Phrase::from_steps(line)));
    Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap()).entries[0].phrase = 0x10;
    let song = apply(
        song,
        &Action::Bloom {
            track: 0,
            chain: 0,
            slot: 0,
            scale: vt_model::scale::Scale::default(),
            bloom: Bloom {
                branches: 1,
                mutation: 3,
                seed: 9,
                ..Bloom::default()
            },
        },
    );
    let mutation = MacroTarget::Gen {
        column: GenColumn::Note,
        param: GenParam::Mutation,
    };
    let song = apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(mutation, (0, 32)),
        },
    );
    apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x4000,
        },
    )
}

/// What a song's generators hold on a phrase of its first track, as they're heard.
fn heard_shapes(song: &Song, phrase: u32) -> Vec<Shape> {
    song.tracks[0].phrases[&phrase]
        .generators()
        .iter()
        .map(|generator| generator.heard())
        .collect()
}

/// A song just loaded has its generators held as its macros hold them, with no edit first, so the
/// engine has nothing of theirs to render at rest (#651): the held shapes aren't saved, and
/// without them it renders every mapped generator at the macro's turn each pass.
#[test]
fn a_song_loaded_holds_its_generators_and_leaves_them_at_rest() {
    // 1 + 15 × 0x44 / 0xFF is 5, so the macro holds `Hits` away from the shape's own 3.
    let song = apply(
        song(&[]),
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x4400,
        },
    );
    let hits = written(&song)
        .iter()
        .filter(|step| step.velocity == Some(0x7F))
        .count();
    assert_eq!(hits, 5, "the macro's turn is written");
    let loaded = loaded(&song, "held-on-load");
    let generator = &loaded.tracks[0].phrases[&0].generators()[0];
    let held = generator.held().expect("held as it loads, with no edit");
    assert!(matches!(held.place, Where::Euclid { hits: 5, .. }));
    assert!(matches!(
        generator.shape.place,
        Where::Euclid { hits: 3, .. }
    ));
    assert_eq!(heard_shapes(&loaded, 0), heard_shapes(&song, 0));
    assert_eq!(
        written(&loaded),
        written(&song),
        "loading writes nothing new"
    );
    let (out, rows, engine) = play(&loaded, 32);
    let (as_written, ..) = play(&plain(&song, written(&song)), 32);
    assert!(out == as_written, "it plays sample for sample as written");
    assert!(
        engine.generators_heard(0).all(|run| run.renders == 0),
        "nothing renders at rest"
    );
    assert!(rows.iter().all(|row| row.cell.is_none()), "as written");
}

/// A Bloom branch's `Mutation`, held by a macro, is held again as it loads, from the trunk it
/// follows.
#[test]
fn a_bloom_branch_loaded_holds_its_mutation_and_leaves_it_at_rest() {
    let song = bloom_branch();
    let branch = &song.tracks[0].phrases[&0].generators()[0];
    assert_eq!(branch.shape.follow.mutation, 3);
    assert_ne!(branch.heard().follow.mutation, 3, "the macro holds it");
    let loaded = loaded(&song, "bloom-held-on-load");
    assert!(loaded.tracks[0].phrases[&0].generators()[0].is_held());
    assert_eq!(heard_shapes(&loaded, 0), heard_shapes(&song, 0));
    assert_eq!(
        loaded.tracks[0].phrases[&0].steps(),
        song.tracks[0].phrases[&0].steps(),
        "loading writes nothing new"
    );
    let (_, _, engine) = play(&loaded, 32);
    assert!(
        engine.generators_heard(0).all(|run| run.renders == 0),
        "nothing renders at rest"
    );
}

#[test]
fn a_mac_lock_moves_the_generators_it_holds_on_its_own_row() {
    let song = song(&[(8, "C-3 40 macro1:FF --- ---")]);
    assert_eq!(written(&song)[8].velocity, Some(0x40), "row 8 isn't a hit");
    let (out, rows, _) = play(&song, 16);
    // The lock puts `Hits` at 16, so row 8 is a hit, and the next note ends it.
    let mut wanted = written(&song);
    wanted[8].velocity = Some(0x7F);
    let (locked, ..) = play(&plain(&song, wanted.clone()), 16);
    assert!(out == locked, "row 8 plays as a hit, and only it");
    let moved: Vec<(usize, Option<GenCell>)> = rows
        .iter()
        .filter(|row| row.cell.is_some())
        .map(|row| (row.row, row.cell))
        .collect();
    // Row 9 is read once before the note there ends the lock.
    assert_eq!(moved[0], (8, Some(GenCell::Velocity(0x7F))));
    assert_eq!(as_heard(&song, &rows), wanted, "kept, it's what played");
}

/// The song with macro 1's mapping of `Hits` bent to move early, `Curve +60` (decision 314).
fn bent(song: Song) -> Song {
    let hits = MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Hits,
    };
    apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits, (1, 16)).bent(60, 0),
        },
    )
}

fn hits(steps: &[Step]) -> usize {
    steps
        .iter()
        .filter(|step| step.velocity == Some(0x7F))
        .count()
}

#[test]
fn a_bent_mapping_moves_its_generator_along_its_curve_as_it_plays() {
    // At the macro's turn, 22, straight is 3 hits and bent early 7, which is what's written.
    let turned = bent(song(&[]));
    assert_eq!(hits(&written(&song(&[]))), 3);
    assert_eq!(hits(&written(&turned)), 7);
    // The engine bends it as the edit does, so at the turn nothing renders.
    let (out, rows, engine) = play(&turned, 32);
    let (as_written, ..) = play(&plain(&turned, written(&turned)), 32);
    assert!(out == as_written, "it plays sample for sample as written");
    assert!(engine.generators_heard(0).all(|run| run.renders == 0));
    assert!(rows.iter().all(|row| row.cell.is_none()), "as written");
    // With the macro at 00, one hit, a lock to 22 on row 7 is 7 hits there, so row 7 is one,
    // where straight it isn't.
    assert_eq!(written(&turned)[7].velocity, Some(0x7F));
    assert_eq!(written(&song(&[]))[7].velocity, Some(0x40));
    let locked = apply(
        bent(song(&[(7, "C-3 40 macro1:22 --- ---")])),
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0,
        },
    );
    assert_eq!(hits(&written(&locked)), 1);
    let (out, rows, _) = play(&locked, 16);
    let mut wanted = written(&locked);
    wanted[7].velocity = Some(0x7F);
    let (as_locked, ..) = play(&plain(&locked, wanted.clone()), 16);
    assert!(out == as_locked, "row 7 plays as a hit, and only it");
    assert_eq!(as_heard(&locked, &rows), wanted, "kept, it's what played");
}

#[test]
fn an_lfo_on_a_macro_moves_its_generator_and_what_was_heard_plays_the_same_written() {
    let mut song = song(&[]);
    song.tracks[0].mods[0] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let (out, rows, engine) = play(&song, 16);
    let heard = as_heard(&song, &rows);
    assert_ne!(heard, written(&song), "the LFO moved the hits");
    assert!(engine.generators_heard(0).any(|run| run.renders > 1));
    let (kept, ..) = play(&plain(&song, heard), 16);
    assert!(out == kept, "the rows heard, written in, play the same");
}

/// Random velocities from `20` to `60` on sixteen notes, these rows written as given, with macro
/// 1 holding the fill's `Spread` from 0 to 100 and its `Bias` from -100 to 100, turned to its
/// middle, where they're as they start: an even draw.
fn random_song(rows: &[(usize, &str)]) -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let mut steps: Vec<Step> = vec!["C-3 40 --- --- ---".parse().unwrap(); 16];
    for (row, text) in rows {
        steps[*row] = text.parse().unwrap();
    }
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let random = Shape {
        from: 0x20,
        to: 0x60,
        kind: FillKind::Random,
        place: Where::Notes,
        ..tresillo()
    };
    let mut song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Velocity,
            span: Span::rows(StepRange::between(0, 15)),
            shape: random,
            list: Box::default(),
            rhythm: Box::default(),
        },
    );
    for (param, ends) in [(GenParam::Spread, (0, 100)), (GenParam::Bias, (-100, 100))] {
        let target = MacroTarget::Gen {
            column: GenColumn::Velocity,
            param,
        };
        song = apply(
            song,
            &Action::MapMacro {
                track: 0,
                at: 0,
                map: MacroMap::numbers(target, ends),
            },
        );
    }
    // 100 x 0x80 / 0xFF is 50, and -100 + 200 x 0x80 / 0xFF is 0.
    apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x8000,
        },
    )
}

/// A macro on a `Random`'s `Spread` and `Bias` (decision 302) reaches them as the engine plays: a
/// `MAC1 00` lock on row 8 puts `Spread` at 0, one value at the bias point, which `Bias` -100
/// puts at the bottom, `20`, on that row alone.
#[test]
fn a_mac_lock_narrows_and_leans_a_random_fill_on_its_own_row() {
    let song = random_song(&[(8, "C-3 40 macro1:00 --- ---")]);
    let written_rows = written(&song);
    assert_ne!(
        written_rows[8].velocity,
        Some(0x20),
        "row 8 isn't the bottom"
    );
    let (out, rows, _) = play(&song, 16);
    let mut wanted = written_rows;
    wanted[8].velocity = Some(0x20);
    let (locked, ..) = play(&plain(&song, wanted.clone()), 16);
    assert!(out == locked, "row 8 plays at the bottom, and only it");
    assert_eq!(as_heard(&song, &rows), wanted, "kept, it's what played");
}

/// An LFO on that macro sweeps `Spread` and `Bias` together, and the velocities heard, written
/// in, play the same.
#[test]
fn an_lfo_on_a_macro_sweeps_a_random_fills_spread_and_bias() {
    let mut song = random_song(&[]);
    song.tracks[0].mods[0] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let (out, rows, engine) = play(&song, 16);
    let heard = as_heard(&song, &rows);
    assert_ne!(heard, written(&song), "the LFO moved the velocities");
    assert!(engine.generators_heard(0).any(|run| run.renders > 1));
    let (kept, ..) = play(&plain(&song, heard), 16);
    assert!(out == kept, "the rows heard, written in, play the same");
}

/// A live generator on a Note column whose scale was changed after it was made (decision 360):
/// the version the engine is handed has its notes written through the new scale, and moving, as
/// with its `To` held at another number, the engine renders it through the new scale too.
#[test]
fn a_moving_generator_on_notes_renders_through_the_scale_it_was_given() {
    use vt_model::action::rescale_generators;
    use vt_model::scale::{Scale, ScaleKind};
    use vt_model::step::Note;
    let major = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    let minor = Scale {
        kind: ScaleKind::Minor,
        ..major
    };
    let mut song = Song::new(1);
    song.saved_scale = major;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let rests = Phrase::from_steps(vec![Step::default(); 16]);
    song.tracks[0].phrases.insert(0, Arc::new(rests));
    let column = ShapeColumn::Note {
        scale: major,
        writes: Some(0x64),
    };
    let song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column,
            span: Span::rows(StepRange::between(0, 15)),
            shape: Shape {
                from: 60,
                to: 72,
                seed: 7,
                ..Shape::default()
            },
            list: Box::default(),
            rhythm: Box::default(),
        },
    );
    let song = rescale_generators(&song, minor).expect("it was in major");
    let pitches = |steps: &[Step]| -> Vec<u8> {
        steps
            .iter()
            .filter_map(|step| match step.note {
                Note::On(note) => Some(note),
                _ => None,
            })
            .collect()
    };
    // At rest, what's written, in minor with notes major hasn't.
    let at_rest = pitches(&written(&song));
    assert_eq!(at_rest.len(), 16);
    assert!(
        at_rest.iter().all(|note| minor.contains(*note)),
        "{at_rest:?}"
    );
    assert!(
        at_rest.iter().any(|note| !major.contains(*note)),
        "{at_rest:?}"
    );
    // Moving, up two octaves, not one.
    let mut engine = Engine::new(RATE, version(&song));
    engine.set_scale(minor);
    engine.hold_generator(0, column, GenParam::To, Some(84));
    engine.play_from(0);
    let mut rows = Vec::new();
    let mut block = vec![0.0; STEP * 2];
    for _ in 0..16 {
        engine.process(&mut block);
        engine.take_gen_reports(|_, report| {
            if let GenReport::Row(row) = report {
                rows.push(row);
            }
        });
    }
    let moving = pitches(&as_heard(&song, &rows));
    assert_ne!(moving, at_rest, "it moved");
    assert!(
        moving.iter().all(|note| minor.contains(*note)),
        "{moving:?}"
    );
    assert!(
        moving.iter().any(|note| !major.contains(*note)),
        "{moving:?}"
    );
}

/// Random notes from C-3 to B-3 on sixteen rows of C-3, the scale off so each of the twelve
/// semitones has a weight, every weight `1`, a new draw each pass, and macro 1 holding the root's
/// weight, `Weight 1`, from 0 to `F` (decision 425), turned to `turn`.
fn weighted_song(turn: u16) -> Song {
    use vt_model::scale::Scale;
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let steps: Vec<Step> = vec!["C-3 64 --- --- ---".parse().unwrap(); 16];
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let mut random = Shape {
        from: 60,
        to: 71,
        kind: FillKind::Random,
        place: Where::Notes,
        seed: 11,
        vary: 1,
        ..Shape::default()
    };
    random.random.weights = [1; 12];
    let column = ShapeColumn::Note {
        scale: Scale {
            on: false,
            ..Scale::default()
        },
        writes: None,
    };
    let song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column,
            span: Span::rows(StepRange::between(0, 15)),
            shape: random,
            list: Box::default(),
            rhythm: Box::default(),
        },
    );
    let root = MacroTarget::Gen {
        column: GenColumn::Note,
        param: GenParam::Weight1,
    };
    let song = apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(root, (0, 15)),
        },
    );
    apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: turn,
        },
    )
}

/// The notes each row played, as the engine reported them, over every pass.
fn notes_heard(song: &Song, heard: &[GenRow]) -> Vec<u8> {
    let phrase = &song.tracks[0].phrases[&0];
    heard
        .iter()
        .filter_map(|row| {
            let step = match row.cell {
                Some(cell) => phrase.generators()[row.generator]
                    .heard_row(phrase.steps(), row.row, cell)
                    .unwrap(),
                None => phrase.steps()[row.row].clone(),
            };
            match step.note {
                vt_model::step::Note::On(note) => Some(note),
                _ => None,
            }
        })
        .collect()
}

/// A macro on one of a `Random`'s weights moves how often its note comes as the engine plays,
/// over many draws: at `0` the root never plays, and at `F`, against `1` for each of the eleven
/// others, it plays more than a third of the time, where at the turn each note comes about one
/// time in twelve.
#[test]
fn a_macro_on_one_weight_moves_how_often_its_note_comes() {
    let share = |turn: u16| {
        let song = weighted_song(turn);
        let (_, rows, _) = play(&song, 16 * 16);
        let notes = notes_heard(&song, &rows);
        assert_eq!(notes.len(), 16 * 16, "each row of each pass is heard");
        notes.iter().filter(|note| **note == 60).count() as f64 / notes.len() as f64
    };
    // 15 × 0x11 / 0xFF is 1, the weight it's written with.
    let (never, even, most) = (share(0), share(0x1100), share(0xFF00));
    assert_eq!(never, 0.0, "a weight of 0 never plays the root");
    assert!(even > 0.0 && even < 0.2, "about one in twelve: {even}");
    assert!(most > 0.35, "the root comes most: {most}");
}

/// An LFO on the macro sweeps the root's weight: the notes heard move from those written, the
/// generator renders again as it plays, and the rows heard, written in, play the same.
#[test]
fn an_lfo_on_a_macro_sweeps_one_weight() {
    let mut song = weighted_song(0x1100);
    let shape = song.tracks[0].phrases[&0].generators()[0].shape;
    song = apply(
        song,
        &Action::Reshape {
            track: 0,
            phrase: 0,
            generator: 0,
            shape: Shape { vary: 0, ..shape },
            list: None,
            rhythm: None,
        },
    );
    song.tracks[0].mods[0] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let (out, rows, engine) = play(&song, 16);
    let heard = as_heard(&song, &rows);
    assert_ne!(heard, written(&song), "the LFO moved the notes");
    assert!(engine.generators_heard(0).any(|run| run.renders > 1));
    let (kept, ..) = play(&plain(&song, heard), 16);
    assert!(out == kept, "the rows heard, written in, play the same");
}

/// The engine renders the same weights the same way every time: played again, a song with a
/// weight on a macro and an LFO plays sample for sample as it did and is heard row for row so,
/// and so does it saved and read back.
#[test]
fn a_weight_on_a_macro_renders_the_same_each_time() {
    let mut song = weighted_song(0xC000);
    song.tracks[0].mods[0] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(8),
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    let (first, first_rows, _) = play(&song, 64);
    let (again, again_rows, _) = play(&song, 64);
    assert!(first == again, "it plays the same");
    assert_eq!(first_rows, again_rows, "each row is heard the same");
    let (read_back, ..) = play(&loaded(&song, "weights"), 64);
    assert!(first == read_back, "saved and read back, it plays the same");
}
