//! The stutter as a song plays it: a step's lock repeats a piece of the track's sound on the grid,
//! each lock starting it again, and golden renders of it.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{HOLD_REPEATS, Insert, Stutter};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, a sixteenth.
const STEP: usize = 6000;

/// A sine sweeping up, so no piece of it is like any other.
fn pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 2)
        .map(|frame| {
            let t = frame as f32 / RATE as f32;
            let value = (t * (150.0 + 300.0 * t) * std::f32::consts::TAU).sin() * 0.5;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("sweep", SampleData { frames });
    Arc::new(pool)
}

/// A song with one track playing these step texts through a stutter in its first slot.
fn song(stutter: Stutter, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "sweep");
    }
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    song.tracks[0].inserts[0] = Insert::Stutter(stutter);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// The left side of a render.
fn render(stutter: Stutter, texts: &[&str], frames: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song(stutter, texts)), pool()));
    render_frames(version, RATE, frames)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// How far back in `dry` the sound in `out` over these frames came from, if it's exactly some of
/// it, looking within a step either side of `about`.
fn came_from(
    out: &[f32],
    dry: &[f32],
    frames: std::ops::Range<usize>,
    about: usize,
) -> Option<usize> {
    (about.saturating_sub(700)..about + 700).find(|back| {
        frames
            .clone()
            .all(|frame| (out[frame] - dry[frame - back]).abs() < 1e-5)
    })
}

#[test]
fn a_lock_repeats_the_piece_after_it_on_the_grid() {
    let notes = ["C-3 -- --- --- ---"];
    let dry = render(Stutter::DEFAULT, &notes, STEP * 6);
    let out = render(
        Stutter::DEFAULT,
        &["C-3 -- 1.stutter.trig:01 --- ---"],
        STEP * 6,
    );
    // The first sixteenth is the sound as it comes; then it's the same sixteenth three more
    // times, each a sixteenth later; then the sound again, where it's got to.
    assert!((0..STEP).all(|frame| out[frame] == dry[frame]));
    for repeat in 1..4 {
        let frames = repeat * STEP + 1000..(repeat + 1) * STEP - 1000;
        assert_eq!(came_from(&out, &dry, frames.clone(), 0), None);
        assert!(
            came_from(&out, &dry, frames, repeat * STEP).is_some(),
            "{repeat}"
        );
    }
    assert!((STEP * 5..STEP * 6).all(|frame| out[frame] == dry[frame]));
}

#[test]
fn each_lock_starts_it_again_even_where_trig_is_on_already() {
    let notes = ["C-3 -- --- --- ---", "D-3 -- --- --- ---"];
    let dry = render(Stutter::DEFAULT, &notes, STEP * 4);
    let out = render(
        Stutter::DEFAULT,
        &[
            "C-3 -- 1.stutter.trig:01 --- ---",
            "D-3 -- 1.stutter.trig:01 --- ---",
        ],
        STEP * 4,
    );
    // The second step is the second note as it comes, and the sixteenth after it repeats it.
    let frames = STEP + 1000..STEP * 2 - 1000;
    assert!(frames.clone().all(|frame| out[frame] == dry[frame]));
    let frames = STEP * 2 + 1000..STEP * 3 - 1000;
    assert!(came_from(&out, &dry, frames, STEP).is_some());
}

#[test]
fn a_lock_on_an_empty_step_stutters_what_is_playing() {
    let notes = ["C-3 -- --- --- ---", "--- -- --- --- ---"];
    let dry = render(Stutter::DEFAULT, &notes, STEP * 4);
    let out = render(
        Stutter {
            repeats: 0,
            ..Stutter::DEFAULT
        },
        &["C-3 -- --- --- ---", "--- -- 1.stutter.trig:01 --- ---"],
        STEP * 4,
    );
    assert!((0..STEP * 2).all(|frame| out[frame] == dry[frame]));
    let frames = STEP * 2 + 1000..STEP * 3 - 1000;
    assert!(came_from(&out, &dry, frames, STEP).is_some());
    assert!((STEP * 3 + 1000..STEP * 4).all(|frame| out[frame] == dry[frame]));
}

fn looped(stutter: Stutter, texts: &[&str]) -> Vec<f32> {
    render_loop(
        Arc::new(Published::new(Arc::new(song(stutter, texts)), pool())),
        RATE,
        RenderOptions::default(),
    )
}

#[test]
fn golden_stutters_stay_the_same() {
    // Rolls of different lengths, fading, and one held until the next note.
    golden(
        "stutter",
        &looped(
            Stutter {
                decay: 0x60,
                ..Stutter::DEFAULT
            },
            &[
                "C-3 -- 1.stutter.trig:01 --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "G-3 -- 1.stutter.trig:01 1.stutter.length:03 1.stutter.repeats:07",
                "--- -- --- --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
    );
    golden(
        "stutter-held",
        &looped(
            Stutter {
                repeats: HOLD_REPEATS,
                length: 4,
                ..Stutter::DEFAULT
            },
            &[
                "C-3 -- 1.stutter.trig:01 --- ---",
                "--- -- --- --- ---",
                "--- -- --- --- ---",
                "D-3 -- --- --- ---",
            ],
        ),
    );
}
