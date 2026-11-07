//! A chain's length changing while it plays: each track stays on the row it was going to play.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Launch, Phrase, Section, Song};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ))
}

/// One track whose chain plays phrases of these lengths, one after another.
fn song(lengths: &[usize]) -> Song {
    let mut song = Song::new(1);
    let entries = lengths
        .iter()
        .enumerate()
        .map(|(at, length)| {
            let id = at as u32 + 10;
            song.tracks[0]
                .phrases
                .insert(id, Arc::new(Phrase::new(*length)));
            ChainEntry {
                phrase: id,
                transpose: 0,
            }
        })
        .collect();
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    song
}

/// Play until these places have fired, from 0.
fn played_to(song: &Song, last: usize) -> Engine {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; (last * STEP + 1) * 2];
    engine.process(&mut out);
    assert_eq!(engine.position(), last);
    engine
}

/// Play one more step.
fn step(engine: &mut Engine) {
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
}

/// Where in its chain the step last fired was.
fn row(engine: &Engine, song: &Song) -> usize {
    let length = song.tracks[0].chain_steps(0) as i64;
    (engine.position() as i64 - engine.jam(0).started).rem_euclid(length) as usize
}

fn with_length(song: &Song, slot: usize, length: usize) -> Song {
    let mut changed = song.clone();
    let id = slot as u32 + 10;
    Arc::make_mut(changed.tracks[0].phrases.get_mut(&id).unwrap()).set_len(length);
    changed
}

#[test]
fn shortening_below_the_place_starts_the_chain_again() {
    let long = song(&[16]);
    let mut engine = played_to(&long, 12);
    let short = with_length(&long, 0, 8);
    engine.swap(version(&short));
    step(&mut engine);
    assert_eq!(row(&engine, &short), 0);
    step(&mut engine);
    assert_eq!(row(&engine, &short), 1);
}

#[test]
fn shortening_above_the_place_keeps_the_row() {
    let long = song(&[16]);
    let mut engine = played_to(&long, 5);
    let short = with_length(&long, 0, 8);
    engine.swap(version(&short));
    step(&mut engine);
    assert_eq!(row(&engine, &short), 6);
}

#[test]
fn lengthening_keeps_the_row() {
    let short = song(&[16]);
    let mut engine = played_to(&short, 5);
    let started = engine.jam(0).started;
    let long = with_length(&short, 0, 32);
    engine.swap(version(&long));
    assert_eq!(engine.jam(0).started, started, "nothing to move");
    step(&mut engine);
    assert_eq!(row(&engine, &long), 6);
}

#[test]
fn shortening_an_earlier_phrase_keeps_the_row_in_a_later_one() {
    let both = song(&[8, 8]);
    // Place 12 is the second phrase's row 4, so the next is its row 5.
    let mut engine = played_to(&both, 12);
    let shorter = with_length(&both, 0, 4);
    engine.swap(version(&shorter));
    step(&mut engine);
    assert_eq!(row(&engine, &shorter), 4 + 5);
}

#[test]
fn a_track_that_didnt_change_keeps_its_count() {
    let plain = song(&[16]);
    let mut engine = played_to(&plain, 20);
    let started = engine.jam(0).started;
    let mut renamed = plain.clone();
    renamed.tracks[0].name = "BD".to_owned();
    engine.swap(version(&renamed));
    assert_eq!(engine.jam(0).started, started);
}

#[test]
fn a_change_back_undoes_itself() {
    let long = song(&[16]);
    let mut engine = played_to(&long, 5);
    let short = with_length(&long, 0, 8);
    engine.swap(version(&short));
    engine.swap(version(&long));
    step(&mut engine);
    assert_eq!(row(&engine, &long), 6, "an undo keeps the row too");
}

#[test]
fn a_section_with_no_length_of_its_own_moves_on_when_shortened_below_its_place() {
    let mut two = song(&[16]);
    two.tracks[0].phrases.insert(20, Arc::new(Phrase::new(16)));
    two.tracks[0].chains.insert(
        1,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 20,
                transpose: 0,
            }],
            label: String::new(),
        }),
    );
    two.sections.push(Section {
        chains: vec![Some(1), None, None, None],
        ..Section::default()
    });
    let mut engine = played_to(&two, 10);
    assert_eq!(engine.section(), 0);
    let short = with_length(&two, 0, 8);
    engine.swap(version(&short));
    step(&mut engine);
    assert_eq!(engine.section(), 1, "its length is now 8, below its place");
}

#[test]
fn a_launch_on_a_grid_follows_the_new_length() {
    let mut grid = song(&[16]);
    grid.launch = Launch::every(4).unwrap();
    grid.tracks[0].phrases.insert(20, Arc::new(Phrase::new(16)));
    grid.tracks[0].chains.insert(
        1,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 20,
                transpose: 0,
            }],
            label: String::new(),
        }),
    );
    // Place 10 has fired, so on the old grid the launch would wait for 12.
    let mut engine = played_to(&grid, 10);
    engine.launch(0, Some(1));
    let short = with_length(&grid, 0, 8);
    engine.swap(version(&short));
    step(&mut engine);
    // The section's length is now 8, so it starts again here, and that's on the grid.
    assert_eq!(engine.jam(0).chain, Some(1));
}
