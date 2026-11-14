//! Beads as a song plays it: a step's lock of `Freeze` holds what it recorded, a render repeats,
//! grains following the steps start on them, and golden renders of each mode and quality; and
//! `Grains`, whose grains it shares, renders as it did.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::insert::{Beads, BeadsMode, Grains, Insert, MOST_SHIFT};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::support::golden;
use crate::support::pluck::pool;

const RATE: u32 = 48_000;

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

fn render(insert: Insert, texts: &[&str], frames: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song(insert, texts)), pool()));
    render_frames(version, RATE, frames)
}

/// How loud the left side is over a stretch of frames, as its RMS.
fn level(sound: &[f32], from: usize, to: usize) -> f32 {
    let left: Vec<f32> = sound[from * 2..to * 2].iter().step_by(2).copied().collect();
    (left.iter().map(|s| s * s).sum::<f32>() / left.len() as f32).sqrt()
}

/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

const WET: Beads = Beads {
    blend: 0xFF,
    reverb: 0,
    ..Beads::DEFAULT
};

/// Repeats every 37 ms or so in `Delay`, and grains of what's just come in in `Granular`.
fn wet(mode: BeadsMode) -> Beads {
    match mode {
        BeadsMode::Granular => Beads { time: 0, ..WET },
        BeadsMode::Delay => Beads {
            mode,
            time: 0,
            density: 0x20,
            ..WET
        },
    }
}

#[test]
fn a_lock_of_freeze_holds_what_it_recorded_after_the_sound_has_gone() {
    for mode in BeadsMode::ALL {
        let beads = Insert::Beads(Beads {
            density: if mode == BeadsMode::Granular {
                0x30
            } else {
                0x20
            },
            ..wet(mode)
        });
        let free = render(beads, &["C-3 -- --- --- ---"], STEP * 16);
        let frozen = render(
            beads,
            &["C-3 -- --- --- ---", "--- -- 1.beads.freeze:01 --- ---"],
            STEP * 16,
        );
        assert!(frozen.iter().all(|sample| sample.is_finite()), "{mode:?}");
        assert_eq!(free[..STEP * 2], frozen[..STEP * 2], "{mode:?}");
        // Over a second after the pluck, the frozen one still plays it and the free one has let
        // it go.
        let (held, gone) = (
            level(&frozen, STEP * 12, STEP * 16),
            level(&free, STEP * 12, STEP * 16),
        );
        assert!(held > 0.005, "{mode:?}: {held}");
        assert!(gone < held * 0.1, "{mode:?}: {gone} {held}");
    }
}

#[test]
fn a_render_repeats() {
    for mode in BeadsMode::ALL {
        for clock in 0..2 {
            let beads = Insert::Beads(Beads {
                mode,
                clock,
                density: 0xE0,
                random: 0x60,
                feedback: 0x80,
                quality: 3,
                ..Beads::DEFAULT
            });
            let texts = [
                "C-3 -- --- --- ---",
                "--- -- --- --- ---",
                "G-3 -- --- --- ---",
            ];
            assert_eq!(
                render(beads, &texts, STEP * 16),
                render(beads, &texts, STEP * 16),
                "{mode:?} {clock}"
            );
        }
    }
}

/// A tone four seconds long, so grains have something to read wherever they start.
fn held_pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 4)
        .map(|frame| {
            let value = (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.3;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("pluck", SampleData { frames });
    Arc::new(pool)
}

#[test]
fn clocked_grains_land_on_the_steps() {
    // Short, square grains on every step, in the cleanest quality and on tape, whose recording
    // runs at half the rate through filters. Each is heard a little after its step, as its attack
    // of 2 ms rises, and later through the filters, but never before it. The cassette's wow
    // delays all of it by a few ms more, as the module's does.
    for (quality, late) in [(0, 100), (2, 160)] {
        let beads = Insert::Beads(Beads {
            clock: 1,
            density: 0x00,
            size: 0x60,
            shape: 0x00,
            time: 0x00,
            quality,
            ..WET
        });
        let version = Arc::new(Published::new(
            Arc::new(song(beads, &["C-3 -- --- --- ---"])),
            held_pool(),
        ));
        let out = render_frames(version, RATE, STEP * 12);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        // Where each burst starts: loud after a long stretch of quiet.
        let loud = |at: usize| left[at].abs() > 0.02;
        let starts: Vec<usize> = (2000..left.len())
            .filter(|&at| loud(at) && !(at - 2000..at).any(loud))
            .collect();
        assert!(starts.len() >= 10, "{quality}: {starts:?}");
        for start in starts {
            assert!(start % STEP <= late, "{quality}: {start}");
        }
    }
}

#[test]
fn golden_beads_stay_the_same() {
    let notes = [
        "C-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "G-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "D#3 -- --- --- ---",
    ];
    // Random grains a fifth up, straying, in the cleanest quality.
    golden(
        "beads-granular",
        &looped(
            Insert::Beads(Beads {
                pitch: MOST_SHIFT + 7,
                density: 0xE0,
                random: 0x60,
                feedback: 0x60,
                ..Beads::DEFAULT
            }),
            &notes,
        ),
    );
    // A delay of three sixteenths an octave up, on tape.
    golden(
        "beads-delay",
        &looped(
            Insert::Beads(Beads {
                mode: BeadsMode::Delay,
                clock: 1,
                density: 0x5A,
                time: 0x30,
                pitch: MOST_SHIFT + 12,
                feedback: 0x80,
                quality: 2,
                ..Beads::DEFAULT
            }),
            &notes,
        ),
    );
    // A delay in 12 bits, frozen into a loop on the fifth step.
    golden(
        "beads-delay-frozen",
        &looped(
            Insert::Beads(Beads {
                mode: BeadsMode::Delay,
                density: 0x30,
                size: 0x50,
                shape: 0x60,
                quality: 1,
                reverb: 0xA0,
                ..Beads::DEFAULT
            }),
            &[
                notes[0],
                notes[1],
                notes[2],
                notes[3],
                "--- -- 1.beads.freeze:01 --- ---",
            ],
        ),
    );
    // Grains on the steps, on the cassette, its wow and flutter and grit.
    golden(
        "beads-cassette-steps",
        &looped(
            Insert::Beads(Beads {
                clock: 1,
                density: 0x10,
                size: 0x90,
                time: 0x80,
                quality: 3,
                reverb: 0x80,
                ..Beads::DEFAULT
            }),
            &notes,
        ),
    );
}
