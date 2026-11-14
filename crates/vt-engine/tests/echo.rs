//! The Space Echo as a song plays it: a render repeats, its wobble follows the song's seed, a step's
//! lock of `Rate` bends what's on the tape, `Sync` keeps time with the song, and golden renders of
//! a few of its modes. When `VT_ECHO_DEMOS` names a folder, the demo test writes WAVs there to hear.

mod support;

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop, write_wav};
use vt_model::insert::{Echo, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// A plucked chord a quarter of a second long, then nothing.
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

fn song(insert: Insert, texts: &[&str], seed: u64) -> Song {
    let mut song = Song::new(1);
    song.seed = seed;
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

fn render(insert: Insert, texts: &[&str], seed: u64, frames: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song(insert, texts, seed)), pool()));
    render_frames(version, RATE, frames)
}

fn looped(insert: Insert, texts: &[&str]) -> Vec<f32> {
    render_loop(
        Arc::new(Published::new(Arc::new(song(insert, texts, 1)), pool())),
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

/// Wobbling hard, fed back, with the reverb.
const WOBBLY: Echo = Echo {
    wow: 0xFF,
    intensity: 0xB0,
    reverb: 0x80,
    ..Echo::DEFAULT
};

#[test]
fn a_render_repeats_and_its_wobble_follows_the_seed() {
    let echo = Insert::Echo(WOBBLY);
    let once = render(echo, &NOTES, 1, STEP * 32);
    assert!(once.iter().all(|sample| sample.is_finite()));
    assert_eq!(once, render(echo, &NOTES, 1, STEP * 32));
    assert_ne!(once, render(echo, &NOTES, 2, STEP * 32));
    // Without wobble, the seed changes nothing.
    let still = Insert::Echo(Echo { wow: 0, ..WOBBLY });
    assert_eq!(
        render(still, &NOTES, 1, STEP * 32),
        render(still, &NOTES, 2, STEP * 32)
    );
}

/// How many times a stretch of the left side crosses zero going up.
fn crossings(sound: &[f32], from: usize, to: usize) -> usize {
    let left: Vec<f32> = sound[from * 2..to * 2].iter().step_by(2).copied().collect();
    left.windows(2)
        .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
        .count()
}

/// A long tone, a quarter of a second a cycle of a 220 Hz sine, held for seconds.
fn tone_pool() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 8)
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
fn a_step_that_slows_the_tape_bends_the_repeats_down_and_they_come_back() {
    let echo = Insert::Echo(Echo {
        mix: 0xFF,
        intensity: 0,
        wow: 0,
        reverb: 0,
        ..Echo::DEFAULT
    });
    // On the eighth step, the tape slows from 150 ms to about 250.
    let texts = [
        "C-3 -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- --- --- ---",
        "--- -- 1.echo.rate:4E --- ---",
    ];
    let version = Arc::new(Published::new(Arc::new(song(echo, &texts, 1)), tone_pool()));
    let out = render_frames(version, RATE, STEP * 16);
    // 220 Hz for 50 ms is 11 crossings; just after the change it's far under, and later back.
    let before = crossings(&out, STEP * 6, STEP * 6 + 2400);
    let bent = crossings(&out, STEP * 7 + 600, STEP * 7 + 3000);
    let after = crossings(&out, STEP * 14, STEP * 14 + 2400);
    assert!(before.abs_diff(11) <= 1, "{before}");
    assert!(bent <= 8, "{bent}");
    assert!(after.abs_diff(11) <= 1, "{after}");
}

#[test]
fn synced_its_first_head_is_a_sixteenth_at_the_songs_tempo() {
    // `1/16` is the seventh division, after `Free`: a step at 120 BPM.
    let echo = Insert::Echo(Echo {
        sync: 7,
        mix: 0xFF,
        intensity: 0,
        wow: 0,
        reverb: 0,
        mode: 0,
        ..Echo::DEFAULT
    });
    let out = render(echo, &["C-3 -- --- --- ---"], 1, STEP * 3);
    let left: Vec<f32> = out.iter().step_by(2).copied().collect();
    let first = left.iter().position(|sample| sample.abs() > 1e-4).unwrap();
    assert!(first.abs_diff(STEP) < 16, "{first}");
}

#[test]
fn golden_echoes_stay_the_same() {
    // As it starts: the first head with its spring.
    golden("echo-default", &looped(Insert::Echo(Echo::DEFAULT), &NOTES));
    // All three heads, fed back near running away, darker and driven, wobbling.
    golden(
        "echo-three-heads",
        &looped(
            Insert::Echo(Echo {
                mode: 10,
                intensity: 0xC8,
                input: 0xB0,
                wow: 0x60,
                treble: 0x50,
                reverb: 0x60,
                ..Echo::DEFAULT
            }),
            &NOTES,
        ),
    );
    // Heads two and three on a dotted eighth at the song's tempo, a step slowing it.
    golden(
        "echo-synced",
        &looped(
            Insert::Echo(Echo {
                mode: 3,
                sync: 12,
                intensity: 0x90,
                inertia: 0x80,
                bass: 0xB0,
                ..Echo::DEFAULT
            }),
            &[
                NOTES[0],
                NOTES[1],
                NOTES[2],
                NOTES[3],
                "--- -- 1.echo.sync:0D --- ---",
            ],
        ),
    );
}

/// Demos to hear, against TAPE-201's renders: written only when `VT_ECHO_DEMOS` names a folder.
#[test]
fn demos_for_the_ear() {
    let Some(folder) = std::env::var_os("VT_ECHO_DEMOS") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    let modes: [(&str, Echo); 5] = [
        (
            "mode-01-head-1",
            Echo {
                mode: 0,
                reverb: 0,
                ..Echo::DEFAULT
            },
        ),
        (
            "mode-04-heads-2-3",
            Echo {
                mode: 3,
                reverb: 0,
                ..Echo::DEFAULT
            },
        ),
        (
            "mode-07-head-3-reverb",
            Echo {
                mode: 6,
                reverb: 0x80,
                ..Echo::DEFAULT
            },
        ),
        (
            "mode-11-all-heads-reverb",
            Echo {
                mode: 10,
                reverb: 0x80,
                ..Echo::DEFAULT
            },
        ),
        (
            "mode-12-reverb-only",
            Echo {
                mode: 11,
                reverb: 0xFF,
                ..Echo::DEFAULT
            },
        ),
    ];
    // The sound alone, which TAPE-201 is given for its own renders.
    let dry = render(Insert::Empty, &NOTES, 1, STEP * 64);
    write_wav(&folder.join("dry.wav"), RATE, &dry).unwrap();
    for (name, echo) in modes {
        let out = render(Insert::Echo(echo), &NOTES, 1, STEP * 64);
        write_wav(&folder.join(format!("ours-{name}.wav")), RATE, &out).unwrap();
    }
    // Played on the effect itself, half wet as the slot is, so its settings can move every 512
    // frames as TAPE-201's are moved between its buffers.
    let played = |settings: &dyn Fn(usize) -> Echo, frames: usize| -> Vec<f32> {
        let mut echo = vt_engine::dsp::effects::echo::Echo::new(RATE);
        echo.seed(vt_engine::rng::Pcg32::new(1, 0));
        echo.follow(settings(0), 25.0);
        echo.settle();
        let mut out = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            if frame % 512 == 0 {
                echo.follow(settings(frame), 25.0);
            }
            let input = [
                dry.get(frame * 2).copied().unwrap_or(0.0),
                dry.get(frame * 2 + 1).copied().unwrap_or(0.0),
            ];
            let wet = echo.process(input);
            out.push(input[0] * 0.5 + wet[0] * 0.5);
            out.push(input[1] * 0.5 + wet[1] * 0.5);
        }
        out
    };
    // `Rate` swept while it echoes: from 150 ms slowing to about 420 over two seconds, speeding up
    // to about 55 over two more, and back to 150, fed back well.
    let sweep = |frame: usize| {
        let seconds = frame as f32 / RATE as f32;
        let rate = match seconds {
            s if s < 2.0 => 0x69 as f32,
            s if s < 4.0 => 0x69 as f32 - (s - 2.0) / 2.0 * 0x39 as f32,
            s if s < 6.0 => 0x30 as f32 + (s - 4.0) / 2.0 * 0x70 as f32,
            s if s < 8.0 => 0xA0 as f32 - (s - 6.0) / 2.0 * 0x37 as f32,
            _ => 0x69 as f32,
        };
        Echo {
            mode: 0,
            rate: rate.round() as u8,
            intensity: 0xA8,
            reverb: 0,
            ..Echo::DEFAULT
        }
    };
    write_wav(
        &folder.join("ours-rate-swept.wav"),
        RATE,
        &played(&sweep, RATE as usize * 12),
    )
    .unwrap();
    // `Intensity` into self-oscillation: up from TAPE-201's 0.8 to its top, 1.2, over four seconds.
    let running = |frame: usize| {
        let seconds = frame as f32 / RATE as f32;
        let intensity = 0xAA as f32 + (seconds / 4.0).min(1.0) * (0xFF - 0xAA) as f32;
        Echo {
            mode: 0,
            intensity: intensity.round() as u8,
            reverb: 0,
            ..Echo::DEFAULT
        }
    };
    write_wav(
        &folder.join("ours-self-oscillation.wav"),
        RATE,
        &played(&running, RATE as usize * 14),
    )
    .unwrap();
}
