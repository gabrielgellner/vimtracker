//! Macros reaching live generators' settings as the engine plays (decision 300): a macro's turn,
//! a step's `MAC` lock and an LFO on it each hold the generator settings it maps where it has them
//! that block, so the engine renders those generators again; at the macro's turn, what's heard is
//! what's written. What each row was heard as is reported, and writing those rows in plays the
//! same.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::gens::{GenReport, GenRow};
use vt_engine::published::Published;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::{Lfo, Rate, Shape as Wave};
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::samples::SamplePool;
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, GenCell, GenParam, Pink, Shape, ShapeColumn,
    Values, Walk, Where,
};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::Step;

const RATE: u32 = 48_000;
/// 5625 frames a step at 128 BPM.
const STEP: usize = 5625;

/// Three hits of `7F` spread over sixteen rows, `40` between.
fn tresillo() -> Shape {
    Shape {
        from: 0x7F,
        to: 0x7F,
        curve: 0,
        s: 0,
        kind: FillKind::Alternate,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::Euclid { hits: 3, rotate: 0 },
        seed: 7,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: vt_model::shape::Grids::default(),
        vary: 0,
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

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ))
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
