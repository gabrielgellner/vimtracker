//! The reverbs as a song plays them: golden renders of each, so the ones there are stay exactly as
//! they were when another comes, and what a spring's knock does on a track with nothing playing.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{Insert, Plate, Reverb, Spring};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

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
        "reverb-spring",
        &looped(Insert::Spring(Spring::DEFAULT), &NOTES),
    );
    // Its chaos and knock take random numbers, which a render repeats.
    let mut knocked = NOTES.to_vec();
    knocked[2] = "--- -- 1.spring.knock:01 --- ---";
    let wild = Insert::Spring(Spring {
        chaos: 0xFF,
        decay: 0xE0,
        ..Spring::DEFAULT
    });
    golden("reverb-spring-knocked", &looped(wild, &knocked));
}

#[test]
fn a_knock_rings_a_spring_on_a_track_with_nothing_playing() {
    let render = |texts: &[&str]| {
        let song = song(Insert::Spring(Spring::DEFAULT), texts);
        let version = Arc::new(Published::new(Arc::new(song), pool()));
        render_frames(version, RATE, STEP * 4)
    };
    let quiet = render(&["--- -- --- --- ---"]);
    assert!(quiet.iter().all(|sample| *sample == 0.0));
    let knocked = render(&["--- -- --- --- ---", "--- -- 1.spring.knock:01 --- ---"]);
    assert!(knocked[..STEP * 2].iter().all(|sample| *sample == 0.0));
    let loudest = knocked[STEP * 2..]
        .iter()
        .fold(0.0_f32, |most, sample| most.max(sample.abs()));
    assert!(loudest > 0.02, "{loudest}");
}
