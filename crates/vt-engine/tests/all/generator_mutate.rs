//! A generator's `Mutate` (decision 430): the share of its rows each new variation draws again,
//! the rest kept as they were last heard, after Marbles' Déjà vu. At `100`, every row, each pass
//! plays its own seed as before; at `0` the first pass heard plays on; between, the rows drawn
//! again are picked from the generator's seed, so a render repeats. Playing from the middle starts
//! from a whole variation, and a macro, its lock and its modulation reach it.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::gens::GenReport;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::{Lfo, Rate};
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::project::{load_song, save_song};
use vt_model::selection::StepRange;
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, GenCell, GenParam, Shape, ShapeColumn, Where, render_into};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// 5625 frames a step at 128 BPM.
const STEP: usize = 5625;
const ROWS: usize = 16;
const PASSES: usize = 9;

fn apply(song: Song, action: &Action) -> Song {
    Arc::unwrap_or_clone(edit(&Arc::new(song), action).expect("the edit applies"))
}

/// One synth track playing a phrase of sixteen notes nine times over, with random velocities in
/// clusters live on it, a new variation every `vary` passes, drawing `mutate` percent of its rows
/// again.
fn song(vary: u8, mutate: u8) -> Song {
    let mut song = Song::new(1);
    song.millibpm = 128_000;
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let hit: Step = "C-3 40 --- --- ---".parse().unwrap();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(vec![hit; ROWS])));
    let chain = Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap());
    let entry = chain.entries[0];
    chain.entries = vec![entry; PASSES];
    apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Velocity,
            span: Span::rows(StepRange::between(0, ROWS - 1)),
            shape: Shape {
                from: 0x10,
                to: 0x7F,
                kind: FillKind::Random,
                place: Where::Clusters(70),
                seed: 7,
                vary,
                mutate,
                ..Shape::default()
            },
        },
    )
}

fn shape(song: &Song) -> Shape {
    song.tracks[0].phrases[&0].generators()[0].shape
}

/// What a pass of the generator renders whole, from its seed alone.
fn whole(song: &Song, pass: i64) -> Vec<GenCell> {
    let phrase = &song.tracks[0].phrases[&0];
    let generator = &phrase.generators()[0];
    let under = generator.under(phrase.steps());
    let mut cells = vec![GenCell::Under; ROWS];
    let rows = generator.rows().unwrap();
    let heard = generator.shape.on_pass(pass);
    render_into(&under, rows, generator.column, &heard, None, &mut cells);
    cells
}

/// Play a song from `from` a step at a time for `steps`, and each row heard on each pass, as the
/// engine reported it: a cell, or none where it played what's written.
fn play_from(song: &Song, from: usize, steps: usize) -> (Vec<f32>, Vec<Vec<Option<GenCell>>>) {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(from);
    let mut out = Vec::with_capacity(steps * STEP * 2);
    let mut passes = vec![vec![None; ROWS]; PASSES];
    let mut block = vec![0.0; STEP * 2];
    for _ in 0..steps {
        engine.process(&mut block);
        out.extend_from_slice(&block);
        engine.take_gen_reports(|_, report| {
            if let GenReport::Row(row) = report {
                passes[row.pass as usize][row.row] = row.cell;
            }
        });
    }
    (out, passes)
}

fn play(song: &Song) -> (Vec<f32>, Vec<Vec<Option<GenCell>>>) {
    play_from(song, 0, PASSES * ROWS)
}

/// The rows heard on each pass, every one of them reported as a cell of its own.
fn cells(passes: &[Vec<Option<GenCell>>]) -> Vec<Vec<GenCell>> {
    passes
        .iter()
        .map(|rows| rows.iter().map(|cell| cell.expect("a row heard")).collect())
        .collect()
}

/// At `100`, as it starts, each pass plays its own seed whole, as before `Mutate` was: the first
/// what's written, with nothing rendered, and each later one what its seed renders, so a song is
/// sample for sample what it was whether `Mutate` is left alone, written at 100 or held there.
#[test]
fn at_every_row_each_pass_plays_its_own_seed_whole() {
    let song = song(1, 100);
    assert_eq!(shape(&song).mutate, 100);
    let (out, passes) = play(&song);
    assert_eq!(passes[0], vec![None; ROWS], "the first is as written");
    for (pass, rows) in passes.iter().enumerate().skip(1) {
        let heard: Vec<GenCell> = rows.iter().map(|cell| cell.unwrap()).collect();
        assert_eq!(
            heard,
            whole(&song, pass as i64),
            "pass {pass} is its seed's"
        );
    }
    let mut engine = Engine::new(RATE, version(&song));
    engine.hold_generator(0, ShapeColumn::Velocity, GenParam::Mutate, Some(100));
    engine.play_from(0);
    let mut held = vec![0.0; PASSES * ROWS * STEP * 2];
    engine.process(&mut held);
    assert!(out == held, "held at 100, it plays the same");
}

/// At `0`, the first variation heard plays every pass after it, whatever `Vary` says: each pass
/// is the first, row for row and sample for sample.
#[test]
fn at_none_the_first_pass_heard_repeats_exactly() {
    for vary in [1, 2, 4] {
        let song = song(vary, 0);
        let (out, passes) = play(&song);
        let passes = cells(&passes);
        let first = whole(&song, 0);
        assert_eq!(passes[0], first, "the first is the written seed's");
        for (pass, rows) in passes.iter().enumerate() {
            assert_eq!(*rows, passes[0], "Vary {vary}: pass {pass} is the first");
        }
        // Sample for sample each pass is the first: the written one with `Vary` off.
        let mut written = shape(&song);
        written.vary = 0;
        let once = apply(
            song,
            &Action::Reshape {
                track: 0,
                phrase: 0,
                generator: 0,
                shape: written,
            },
        );
        assert!(play(&once).0 == out, "Vary {vary}: as if it never varied");
    }
}

/// Between, each new variation draws its rows again where the shape's seed picks them, about
/// `Mutate`'s share of them, from its own seed, and keeps the rest as the last pass heard them; a
/// render repeats, and another seed picks other rows.
#[test]
fn a_share_draws_about_that_many_rows_again_and_repeats() {
    let song = song(1, 40);
    let written = shape(&song);
    let (out, passes) = play(&song);
    let passes = cells(&passes);
    assert_eq!(passes[0], whole(&song, 0));
    let mut drawn = 0;
    for pass in 1..PASSES {
        let fresh = whole(&song, pass as i64);
        for row in 0..ROWS {
            if written.draws_again(pass as i64, row) {
                drawn += 1;
                assert_eq!(passes[pass][row], fresh[row], "pass {pass} draws row {row}");
            } else {
                let kept = passes[pass - 1][row];
                assert_eq!(passes[pass][row], kept, "pass {pass} keeps row {row}");
            }
        }
    }
    let share = f64::from(drawn) / ((PASSES - 1) * ROWS) as f64;
    assert!((0.25..0.55).contains(&share), "about 40%: {share}");
    // Some rows have drifted from the first pass, and some kept it.
    let last = &passes[PASSES - 1];
    assert_ne!(*last, passes[0]);
    assert!((0..ROWS).any(|row| last[row] == passes[0][row]));
    let (again, heard_again) = play(&song);
    assert!(out == again, "a render repeats");
    assert_eq!(cells(&heard_again), passes);
    // Saved and read back, it plays the same.
    let folder = std::env::temp_dir().join(format!("vt-mutate-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    save_song(&folder, &song).unwrap();
    let loaded = load_song(&folder).unwrap();
    let _ = std::fs::remove_dir_all(&folder);
    assert_eq!(shape(&loaded).mutate, 40);
    assert!(play(&loaded).0 == out, "read back, it plays the same");
    // Another seed picks other rows.
    let other = Shape { seed: 8, ..written };
    assert!((0..ROWS).any(|row| other.draws_again(1, row) != written.draws_again(1, row)));
    // A higher share draws again every row a lower one does.
    let more = Shape {
        mutate: 70,
        ..written
    };
    for row in 0..ROWS {
        assert!(!written.draws_again(3, row) || more.draws_again(3, row));
    }
}

/// Played from the middle, it starts from a whole variation, the pass's own seed's, as Steps'
/// `Mutate` does, and keeps it from there.
#[test]
fn played_from_the_middle_it_starts_from_a_whole_variation() {
    let song = song(1, 0);
    let (_, passes) = play_from(&song, 4 * ROWS, 3 * ROWS);
    for (pass, heard) in passes.iter().enumerate().take(7).skip(4) {
        let rows: Vec<GenCell> = heard.iter().map(|cell| cell.unwrap()).collect();
        assert_eq!(rows, whole(&song, 4), "pass {pass} is the fifth's whole");
    }
}

/// A new version while it plays, as an edit makes, keeps what was heard: at `0`, the first pass
/// plays on after it.
#[test]
fn an_edit_keeps_what_was_heard() {
    let song = song(1, 0);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut passes = vec![vec![None; ROWS]; PASSES];
    let mut block = vec![0.0; STEP * 2];
    for step in 0..4 * ROWS {
        if step % 5 == 3 {
            let mut edited = song.clone();
            edited.millibpm = 128_000 + step as u32;
            engine.swap(version(&edited));
        }
        engine.process(&mut block);
        engine.take_gen_reports(|_, report| {
            if let GenReport::Row(row) = report {
                passes[row.pass as usize][row.row] = row.cell;
            }
        });
    }
    let passes = cells(&passes[..4]);
    for (pass, rows) in passes.iter().enumerate() {
        assert_eq!(*rows, whole(&song, 0), "pass {pass} is the first");
    }
}

/// Macro 1 holding `Mutate` from 0 to 100, turned to 0 over a generator written at 100: the
/// macro's hold reaches it, so the first pass plays on; then with an LFO on the macro, what it
/// adds draws rows again as it plays, which the turn alone never does, and the render repeats.
#[test]
fn modulation_reaches_it() {
    let song = song(1, 100);
    let target = MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Mutate,
    };
    let song = apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(target, (0, 100)),
        },
    );
    let mut song = apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0,
        },
    );
    assert_eq!(shape(&song).mutate, 100, "written at every row");
    let (_, still) = play(&song);
    let still = cells(&still);
    assert_eq!(still[0], whole(&song, 0));
    assert!(
        still.iter().all(|rows| *rows == still[0]),
        "the macro's turn holds it at none"
    );
    song.tracks[0].mods[0] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(32),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let (out, moved) = play(&song);
    let moved = cells(&moved);
    assert_eq!(moved[0], still[0], "the first pass is whole either way");
    assert!(
        moved.iter().any(|rows| *rows != moved[0]),
        "the LFO draws rows again"
    );
    let (again, moved_again) = play(&song);
    assert!(out == again, "a render repeats");
    assert_eq!(cells(&moved_again), moved);
}
