//! Playing from the top resets every track's held values, its random generators and its LFOs. Only
//! the track states a song has used since the last reset have settings to put back, so only those
//! are gone through; what comes out is the same as when every state was, as tracks come and go
//! between plays, and while a removed track still rings.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::constants::UNITY_LEVEL;
use vt_model::insert::{Distort, DistortType, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song, level_gain};

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Far enough into a step for the smoothers to have arrived.
const SETTLED: usize = 4000;

/// A flat sample, so what comes out is the gain heard, and a decaying tone with something to
/// distort.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "flat",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 20],
        },
    );
    let tone = (0..RATE as usize)
        .map(|at| {
            let time = at as f32 / RATE as f32;
            let value = (time * 110.0 * std::f32::consts::TAU).sin() * (1.0 - time) * 0.6;
            [value, value * 0.8]
        })
        .collect();
    pool.insert("tone", SampleData { frames: tone });
    Arc::new(pool)
}

/// Tracks playing these rows of a sample, with notes that start and stop at once. A track with no
/// rows plays nothing.
fn song(sample: &str, tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    song.sections[0].length = Some(16);
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, sample);
        let track = &mut song.tracks[index];
        track.envelopes[0].attack_ms = 0;
        track.envelopes[0].release_ms = 0;
        if !rows.is_empty() {
            let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
            track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
        }
    }
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

fn play(engine: &mut Engine, steps: usize, out: &mut Vec<f32>) {
    let mut block = vec![0.0; steps * STEP * 2];
    engine.process(&mut block);
    out.extend(block);
}

const HELD: &[&str] = &[
    "C-3 -- vol:A0h cut:50h ---",
    "E-3 -- res:90h --- ---",
    "G-3 -- cha:80 --- ---",
];
const PLAIN: &[&str] = &["C-3 -- cha:80 --- ---", "D-3 -- --- --- ---"];

/// Five tracks holding values, the last two through an `Overdrive` that rings on after them.
fn five(seed: u64) -> Song {
    let mut song = song("tone", &[HELD, HELD, HELD, HELD, HELD]);
    song.seed = seed;
    for track in &mut song.tracks[3..5] {
        track.inserts[0] = Insert::Distort(Distort {
            kind: DistortType::Overdrive,
            amount: 0xC0,
            mix: 0xFF,
            ..Distort::default()
        });
    }
    song
}

/// The first three of them.
fn three(seed: u64) -> Song {
    let mut song = five(seed);
    song.remove_last_sound_track();
    song.remove_last_sound_track();
    song
}

/// The first three, then two new tracks with nothing held, as if added again.
fn added_back(seed: u64) -> Song {
    let mut song = three(seed);
    song.add_track();
    song.add_track();
    for index in 3..5 {
        song.sample_on(index, "tone");
        let track = &mut song.tracks[index];
        track.envelopes[0].attack_ms = 0;
        track.envelopes[0].release_ms = 0;
        let steps = PLAIN.iter().map(|text| text.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

#[test]
fn golden_starts_of_play_stay_the_same() {
    let mut out = Vec::new();
    let mut engine = Engine::new(RATE, version(&three(7)));
    engine.play_from(0);
    play(&mut engine, 1, &mut out);
    // Two tracks added while playing, holding values of their own, then from the top again with
    // everything held.
    engine.swap(version(&five(7)));
    play(&mut engine, 3, &mut out);
    engine.play_from(0);
    play(&mut engine, 3, &mut out);
    // Those two removed while they sound, so their notes and their `Overdrive` ring on in states
    // past the song's tracks, and playing from the top straight away.
    engine.swap(version(&three(7)));
    engine.play_from(0);
    play(&mut engine, 2, &mut out);
    // Added and removed again between two plays, and from the top once they've faded.
    engine.swap(version(&five(7)));
    play(&mut engine, 2, &mut out);
    engine.swap(version(&three(7)));
    play(&mut engine, 2, &mut out);
    engine.play_from(0);
    play(&mut engine, 1, &mut out);
    // A new seed, then from the top again, and the tracks added back while playing, with their
    // chances drawn from what that reset left them.
    engine.swap(version(&three(9)));
    engine.play_from(0);
    play(&mut engine, 1, &mut out);
    engine.swap(version(&added_back(9)));
    play(&mut engine, 4, &mut out);
    // And fewer tracks than ever, from the top.
    engine.swap(version(&song("tone", &[HELD])));
    engine.play_from(0);
    play(&mut engine, 3, &mut out);
    golden("play-start", &out);
}

#[test]
fn a_track_added_after_playing_from_the_top_hears_its_own_settings() {
    // A track added while playing holds a value.
    let alone = song("flat", &[&[]]);
    let mut engine = Engine::new(RATE, version(&alone));
    engine.play_from(0);
    let held = song("flat", &[&[], &["C-3 -- vol:40h --- ---"]]);
    engine.swap(version(&held));
    play(&mut engine, 2, &mut Vec::new());
    // It goes, its state moving past the song's tracks, where none was at the last reset, and
    // once it's faded, playing from the top resets it there.
    engine.swap(version(&alone));
    play(&mut engine, 1, &mut Vec::new());
    engine.play_from(0);
    play(&mut engine, 1, &mut Vec::new());
    // A new track while playing takes that state, and plays at its own level, not the old hold.
    let plain = song("flat", &[&[], &["C-3 -- --- --- ---"]]);
    engine.swap(version(&plain));
    let mut out = Vec::new();
    play(&mut engine, 3, &mut out);
    let heard = out[2 * (2 * STEP + SETTLED)];
    let unity = level_gain(f32::from(UNITY_LEVEL));
    assert!(
        (heard - unity).abs() < 0.001,
        "{heard} should be the gain of unity, {unity}"
    );
}
