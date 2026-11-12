//! The exciter as a song plays it: what it adds to a track's sound, and golden renders of it.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{Exciter, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;

/// A short, bright, decaying noise, as a hat, from a fixed seed so no audio file is needed.
fn pool() -> Arc<SamplePool> {
    let mut seed = 0x1234_5678_u32;
    let frames = (0..4800)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let noise = seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
            let value = noise * (1.0 - i as f32 / 4800.0).powi(3) * 0.5;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("hat", SampleData { frames });
    Arc::new(pool)
}

fn song(insert: Insert, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "hat");
    }
    song.tracks[0].inserts[0] = insert;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

const HATS: [&str; 4] = [
    "C-3 -- --- --- ---",
    "C-3 40 --- --- ---",
    "C-3 -- --- --- ---",
    "C-3 70 --- --- ---",
];

#[test]
fn it_adds_to_the_sound_and_at_no_mix_leaves_it_be() {
    let render = |insert: Insert| {
        let version = Arc::new(Published::new(Arc::new(song(insert, &HATS)), pool()));
        render_frames(version, RATE, 6000 * 4)
    };
    let dry = render(Insert::Empty);
    let none = render(Insert::Exciter(Exciter {
        mix: 0x00,
        ..Exciter::DEFAULT
    }));
    assert_eq!(dry, none);
    let excited = render(Insert::Exciter(Exciter::DEFAULT));
    assert!(excited.iter().all(|sample| sample.is_finite()));
    let energy = |samples: &[f32]| samples.iter().map(|s| s * s).sum::<f32>();
    let added: Vec<f32> = excited.iter().zip(&dry).map(|(a, b)| a - b).collect();
    assert!(energy(&added) > energy(&dry) * 0.01);
}

#[test]
fn golden_exciter_stays_the_same() {
    let looped = |insert: Insert| {
        render_loop(
            Arc::new(Published::new(Arc::new(song(insert, &HATS)), pool())),
            RATE,
            RenderOptions::default(),
        )
    };
    golden("exciter", &looped(Insert::Exciter(Exciter::DEFAULT)));
    golden(
        "exciter-driven",
        &looped(Insert::Exciter(Exciter {
            drive: 0xFF,
            tone: 0xC0,
            attack: 0x00,
            release: 0xFF,
            mix: 0xFF,
            ..Exciter::DEFAULT
        })),
    );
}
