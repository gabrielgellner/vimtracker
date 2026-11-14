//! Clouds as a song plays it: a step's lock of `Freeze` holds what it recorded after the sound has
//! gone, a render repeats, and golden renders of each mode.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{Clouds, CloudsMode, Insert, MOST_SHIFT};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

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

fn song(clouds: Clouds, texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "pluck");
    }
    song.tracks[0].inserts[0] = Insert::Clouds(clouds);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn render(clouds: Clouds, texts: &[&str], frames: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song(clouds, texts)), pool()));
    render_frames(version, RATE, frames)
}

/// How loud the left side is over a stretch of frames, as its RMS.
fn level(sound: &[f32], from: usize, to: usize) -> f32 {
    let left: Vec<f32> = sound[from * 2..to * 2].iter().step_by(2).copied().collect();
    (left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32).sqrt()
}

const WET: Clouds = Clouds {
    blend: 0xFF,
    reverb: 0,
    ..Clouds::DEFAULT
};

#[test]
fn a_lock_of_freeze_holds_what_it_recorded_after_the_sound_has_gone() {
    for mode in CloudsMode::ALL {
        // Reading what's just come in, so what's held when it freezes is the pluck.
        let clouds = Clouds {
            mode,
            position: 0,
            ..WET
        };
        let free = render(clouds, &["C-3 -- --- --- ---"], STEP * 8);
        let frozen = render(
            clouds,
            &[
                "C-3 -- --- --- ---",
                "--- -- --- --- ---",
                "--- -- 1.clouds.freeze:01 --- ---",
            ],
            STEP * 8,
        );
        assert!(frozen.iter().all(|sample| sample.is_finite()), "{mode:?}");
        // The same until the lock, then most of a second after the pluck the frozen one still
        // plays it and the free one has let it go.
        assert_eq!(free[..STEP * 4], frozen[..STEP * 4], "{mode:?}");
        let (held, gone) = (
            level(&frozen, STEP * 6, STEP * 8),
            level(&free, STEP * 6, STEP * 8),
        );
        assert!(held > 0.005, "{mode:?}: {held}");
        assert!(gone < held * 0.1, "{mode:?}: {gone} {held}");
    }
}

#[test]
fn a_render_repeats() {
    for mode in CloudsMode::ALL {
        let clouds = Clouds {
            mode,
            density: 0xE0,
            feedback: 0x80,
            ..Clouds::DEFAULT
        };
        let texts = [
            "C-3 -- --- --- ---",
            "--- -- --- --- ---",
            "G-3 -- --- --- ---",
        ];
        assert_eq!(
            render(clouds, &texts, STEP * 16),
            render(clouds, &texts, STEP * 16),
            "{mode:?}"
        );
    }
}

#[test]
fn golden_clouds_stay_the_same() {
    let looped = |clouds: Clouds, texts: &[&str]| {
        render_loop(
            Arc::new(Published::new(Arc::new(song(clouds, texts)), pool())),
            RATE,
            RenderOptions::default(),
        )
    };
    let notes = [
        "C-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "G-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "D#3 -- --- --- ---",
    ];
    // Random grains a fifth up, spread wide, with some feedback.
    golden(
        "clouds-granular",
        &looped(
            Clouds {
                pitch: MOST_SHIFT + 7,
                density: 0xE0,
                spread: 0xFF,
                feedback: 0x60,
                ..Clouds::DEFAULT
            },
            &notes,
        ),
    );
    // Stretched an octave down through long windows, darkened.
    golden(
        "clouds-stretch",
        &looped(
            Clouds {
                mode: CloudsMode::Stretch,
                pitch: MOST_SHIFT - 12,
                size: 0xE0,
                texture: 0x50,
                density: 0x40,
                ..Clouds::DEFAULT
            },
            &notes,
        ),
    );
    // A delay a fifth up, frozen into a loop on the fifth step.
    golden(
        "clouds-looping",
        &looped(
            Clouds {
                mode: CloudsMode::Looping,
                position: 0x30,
                size: 0x90,
                pitch: MOST_SHIFT + 7,
                feedback: 0x50,
                ..Clouds::DEFAULT
            },
            &[
                notes[0],
                notes[1],
                notes[2],
                notes[3],
                "--- -- 1.clouds.freeze:01 --- ---",
            ],
        ),
    );
    // Regular grains in mono at 8 bits, a long recording's grit.
    golden(
        "clouds-mono-8",
        &looped(
            Clouds {
                quality: 3,
                density: 0x20,
                position: 0x80,
                reverb: 0xA0,
                ..Clouds::DEFAULT
            },
            &notes,
        ),
    );
}
