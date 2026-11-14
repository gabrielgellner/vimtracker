//! Beads as a song plays it, and `Grains`, whose grains it shares: golden renders of each.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop};
use vt_model::insert::{Grains, Insert, MOST_SHIFT};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;

/// A plucked chord a quarter of a second long, then nothing: a few sines and their octaves,
/// dying away.
fn pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize / 4)
        .map(|frame| {
            let t = frame as f32 / RATE as f32;
            let value = [130.8_f32, 196.0, 261.6, 329.6, 523.2]
                .iter()
                .map(|hertz| (t * hertz * std::f32::consts::TAU).sin() * 0.12)
                .sum::<f32>()
                * (-t * 6.0).exp();
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("pluck", SampleData { frames });
    Arc::new(pool)
}

fn song(insert: Insert, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "pluck");
    }
    song.tracks[0].inserts[0] = insert;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn looped(insert: Insert, texts: &[&str]) -> Vec<f32> {
    render_loop(
        Arc::new(Published::new(Arc::new(song(insert, texts)), pool())),
        RATE,
        RenderOptions::default(),
    )
}

const NOTES: [&str; 7] = [
    "C-3 -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "G-3 -- --- --- ---",
    "--- -- --- --- ---",
    "--- -- --- --- ---",
    "D#3 -- --- --- ---",
];

#[test]
fn grains_render_as_they_did_before_beads_shared_them() {
    // Random grains a fifth up, straying, spread wide, then frozen; and regular ones backwards.
    golden(
        "grains-random",
        &looped(
            Insert::Grains(Grains {
                pitch: MOST_SHIFT + 7,
                density: 0xE0,
                random: 0x60,
                spread: 0xFF,
                ..Grains::DEFAULT
            }),
            &[
                NOTES[0],
                NOTES[1],
                NOTES[2],
                NOTES[3],
                "--- -- 1.grains.freeze:01 --- ---",
            ],
        ),
    );
    golden(
        "grains-regular",
        &looped(
            Insert::Grains(Grains {
                size: 0x30,
                density: 0x30,
                time: 0x60,
                shape: 0x40,
                mix: 0xFF,
                ..Grains::DEFAULT
            }),
            &NOTES,
        ),
    );
}
