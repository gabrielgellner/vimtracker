//! The rotary speaker as a song plays it: a step's lock of `Fast` speeds it up from that step, and
//! golden renders of it.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{Insert, Rotary};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// An organ-like chord held for two seconds: a few sines and their octaves.
fn pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 2)
        .map(|frame| {
            let t = frame as f32 / RATE as f32;
            let value = [130.8_f32, 164.8, 196.0, 261.6, 523.2, 1046.5]
                .iter()
                .map(|hertz| (t * hertz * std::f32::consts::TAU).sin() * 0.1)
                .sum::<f32>();
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("organ", SampleData { frames });
    Arc::new(pool)
}

fn song(rotary: Rotary, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "organ");
    }
    song.tracks[0].inserts[0] = Insert::Rotary(rotary);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn render(texts: &[&str]) -> Vec<f32> {
    let version = Arc::new(Published::new(
        Arc::new(song(Rotary::DEFAULT, texts)),
        pool(),
    ));
    render_frames(version, RATE, STEP * 8)
}

#[test]
fn a_lock_of_fast_speeds_it_up_from_its_step() {
    let slow = render(&["C-3 -- --- --- ---"]);
    let fast = render(&[
        "C-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- 1.rotary.fast:01 --- ---",
    ]);
    assert!(fast.iter().all(|sample| sample.is_finite()));
    // Two steps the same, both sides, then not.
    let at = STEP * 2 * 2;
    assert_eq!(slow[..at], fast[..at]);
    assert_ne!(slow[at + 2000..], fast[at + 2000..]);
}

#[test]
fn golden_rotaries_stay_the_same() {
    let looped = |rotary: Rotary, texts: &[&str]| {
        render_loop(
            Arc::new(Published::new(Arc::new(song(rotary, texts)), pool())),
            RATE,
            RenderOptions::default(),
        )
    };
    golden(
        "rotary",
        &looped(
            Rotary::DEFAULT,
            &["C-3 -- --- --- ---", "--- -- --- --- ---"],
        ),
    );
    // Sped up on a step and slowed at the next note, driven.
    golden(
        "rotary-fast",
        &looped(
            Rotary {
                drive: 0x60,
                ..Rotary::DEFAULT
            },
            &[
                "C-3 -- --- --- ---",
                "--- -- 1.rotary.fast:01 --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "G-3 -- --- --- ---",
                "--- -- --- --- ---",
            ],
        ),
    );
}
