//! The reverbs as a song plays them: golden renders of each, so the ones there are stay exactly as
//! they were when another comes.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::insert::{Insert, Plate, Reverb, Space};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;

/// A short decaying tone, computed so no audio file is needed.
fn pool() -> Arc<SamplePool> {
    let frames = (0..4800)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let value = (t * 220.0 * std::f32::consts::TAU).sin() * (1.0 - i as f32 / 4800.0) * 0.8;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("tone", SampleData { frames });
    Arc::new(pool)
}

/// A song with one track playing these step texts through an effect in its first slot.
fn song(insert: Insert, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "tone");
    }
    song.tracks[0].inserts[0] = insert;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// Two notes, a fifth apart, with room after each to ring.
const NOTES: [&str; 5] = [
    "C-3 -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "G-3 -- --- --- ---",
];

fn looped(insert: Insert, texts: &[&str]) -> Vec<f32> {
    render_loop(
        Arc::new(Published::new(Arc::new(song(insert, texts)), pool())),
        RATE,
        RenderOptions::default(),
    )
}

#[test]
fn golden_reverbs_stay_the_same() {
    golden(
        "reverb-clouds",
        &looped(Insert::Reverb(Reverb::DEFAULT), &NOTES),
    );
    golden(
        "reverb-plate",
        &looped(Insert::Plate(Plate::DEFAULT), &NOTES),
    );
    golden(
        "reverb-space",
        &looped(Insert::Space(Space::DEFAULT), &NOTES),
    );
}
