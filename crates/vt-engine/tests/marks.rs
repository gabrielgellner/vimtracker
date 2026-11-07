//! What locks, holds and glides do. `VOL` is the probe throughout: with a flat sample and instant
//! envelopes, what comes out is the gain of the value being heard.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::constants::UNITY_LEVEL;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Phrase, Song, level_gain};
use vt_model::target::Target;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Far enough into a step for the smoothers to have arrived.
const SETTLED: usize = 4000;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = vec![[1.0, 1.0]; RATE as usize * 20];
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in &mut song.tracks {
        track.sample = Some("s".to_owned());
    }
    song.limiter = false;
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    let steps = texts.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), flat()))
}

fn render(song: Song, frames: usize) -> Vec<f32> {
    let out = render_frames(version(song), RATE, frames);
    out.as_chunks::<2>().0.iter().map(|f| f[0]).collect()
}

/// The left channel once a step has settled.
fn at_step(out: &[f32], step: usize) -> f32 {
    out[step * STEP + SETTLED]
}

#[track_caller]
fn close(actual: f32, value: f32) {
    let expected = level_gain(value);
    assert!(
        (actual - expected).abs() < 0.001,
        "{actual} should be the gain of {value:X?}, {expected}"
    );
}

/// Close to a value that's still moving, where the smoother is a little behind it.
#[track_caller]
fn near(actual: f32, value: f32) {
    let (low, high) = (level_gain(value - 5.0), level_gain(value + 5.0));
    assert!(
        (low..=high).contains(&actual),
        "{actual} should be near the gain of {value}, {low} to {high}"
    );
}

const UNITY: f32 = UNITY_LEVEL as f32;

// Locks.

#[test]
fn a_lock_lasts_until_the_next_note() {
    let out = render(
        song(&[
            "C-4 -- VOL80 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]),
        STEP * 3,
    );
    close(at_step(&out, 0), 128.0);
    close(at_step(&out, 1), 128.0);
    close(at_step(&out, 2), UNITY);
}

#[test]
fn a_lock_on_a_row_with_no_note_changes_the_note_and_ends_at_the_next() {
    let out = render(
        song(&[
            "C-4 -- ---00 ---00 ---00",
            "--- -- VOL80 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]),
        STEP * 4,
    );
    close(at_step(&out, 0), UNITY);
    close(at_step(&out, 1), 128.0);
    close(at_step(&out, 2), 128.0);
    close(at_step(&out, 3), UNITY);
}

#[test]
fn a_lock_on_a_notes_own_row_survives_that_note() {
    let out = render(
        song(&["C-4 -- VOL80 ---00 ---00", "C-4 -- VOL40 ---00 ---00"]),
        STEP * 2,
    );
    close(at_step(&out, 1), 64.0);
}

#[test]
fn the_next_value_of_a_parameter_replaces_its_lock() {
    let out = render(
        song(&["C-4 -- VOL80 ---00 ---00", "--- -- VOL40 ---00 ---00"]),
        STEP * 2,
    );
    close(at_step(&out, 1), 64.0);
}

#[test]
fn another_parameters_value_leaves_a_lock_alone() {
    let out = render(
        song(&["C-4 -- VOL80 ---00 ---00", "--- -- CUTF0 ---00 ---00"]),
        STEP * 2,
    );
    assert!((at_step(&out, 1) - level_gain(128.0)).abs() < 0.05);
}

#[test]
fn a_note_a_failed_chance_skips_ends_nothing() {
    let out = render(
        song(&["C-4 -- VOL80 ---00 ---00", "C-4 -- CHA00 ---00 ---00"]),
        STEP * 2,
    );
    close(at_step(&out, 1), 128.0);
}

#[test]
fn an_off_ends_nothing() {
    let mut start = song(&["C-4 -- VOL80 ---00 ---00", "OFF -- ---00 ---00 ---00"]);
    start.tracks[0].envelopes[0].release_ms = 2000;
    let out = render(start, STEP + 100);
    // Just into the release, it's still at the lock's level and hasn't jumped up.
    let level = out[STEP + 99];
    assert!(level < level_gain(128.0) * 1.01, "{level}");
    assert!(level > level_gain(128.0) * 0.9, "{level}");
}

#[test]
fn a_delayed_note_ends_locks_at_its_row() {
    let out = render(
        song(&["C-4 -- VOL80 ---00 ---00", "C-4 -- DEL05 ---00 ---00"]),
        STEP * 2,
    );
    // Before the delayed note starts, the old one is already back at the base.
    near(out[STEP + 4500], UNITY);
}

#[test]
fn the_rightmost_slot_wins() {
    let out = render(song(&["C-4 -- VOL40 VOL80 ---00"]), STEP);
    close(at_step(&out, 0), 128.0);
    let out = render(song(&["C-4 -- VOL80h VOL40 ---00"]), STEP);
    close(at_step(&out, 0), 64.0);
}

#[test]
fn an_lfo_carries_on_around_a_lock_as_around_a_hold() {
    let with = |text: &str| {
        let mut start = song(&[text]);
        start.tracks[0].lfos[0] = Lfo {
            destination: Some(Target::Vol),
            shape: Shape::Sine,
            rate: Rate::Hertz(400),
            depth: 0x20,
            ..Lfo::default()
        };
        render(start, STEP)
    };
    let locked = with("C-4 -- VOL80 ---00 ---00");
    assert_eq!(locked, with("C-4 -- VOL80h ---00 ---00"));
    assert!(locked[1000] != locked[3000], "the LFO is moving it");
}

#[test]
fn wrapping_the_loop_leaves_a_lock_until_a_note() {
    let out = render(
        song(&[
            "--- -- ---00 ---00 ---00",
            "C-4 -- VOL80 ---00 ---00",
            "--- -- ---00 ---00 ---00",
        ]),
        STEP * 5,
    );
    close(at_step(&out, 3), 128.0);
    close(at_step(&out, 4), 128.0);
}

#[test]
fn realigning_leaves_a_lock_alone() {
    let mut engine = Engine::new(
        RATE,
        version(song(&[
            "--- -- ---00 ---00 ---00",
            "C-4 -- VOL80 ---00 ---00",
            "--- -- ---00 ---00 ---00",
        ])),
    );
    engine.play_from(0);
    let mut out = vec![0.0; 2 * (STEP * 2 + SETTLED)];
    engine.process(&mut out);
    engine.realign();
    let mut more = vec![0.0; 2 * STEP / 2];
    engine.process(&mut more);
    close(more[more.len() - 2], 128.0);
}

#[test]
fn a_page_edit_waits_under_a_lock_and_replaces_a_hold() {
    let hear = |text: &str, after_note: bool| {
        let start = song(&[text, "C-4 -- ---00 ---00 ---00"]);
        let mut engine = Engine::new(RATE, version(start.clone()));
        engine.play_from(0);
        let mut out = vec![0.0; 2 * SETTLED];
        engine.process(&mut out);
        let mut edited = start;
        edited.tracks[0].pan = 0xFF;
        engine.swap(version(edited));
        engine.process(&mut vec![0.0; 2 * 1900]);
        if after_note {
            engine.process(&mut vec![0.0; 2 * SETTLED]);
        }
        let mut last = vec![0.0; 2];
        engine.process(&mut last);
        (last[0], last[1])
    };
    // Locked hard left, the edit to hard right isn't heard yet.
    let (left, right) = hear("C-4 -- PAN00 ---00 ---00", false);
    assert!(left > 0.5 && right.abs() < 0.01, "{left} {right}");
    // The next note ends the lock, and there it is.
    let (left, right) = hear("C-4 -- PAN00 ---00 ---00", true);
    assert!(left.abs() < 0.1 && right > 1.3, "{left} {right}");
    // A hold gives way to the edit straight away.
    let (left, right) = hear("C-4 -- PAN00h ---00 ---00", false);
    assert!(left.abs() < 0.1 && right > 1.3, "{left} {right}");
}

// Holds.

#[test]
fn a_hold_outlasts_notes_and_a_lock_returns_to_it() {
    let out = render(
        song(&[
            "C-4 -- VOL80h ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "C-4 -- VOL40 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]),
        STEP * 4,
    );
    close(at_step(&out, 1), 128.0);
    close(at_step(&out, 2), 64.0);
    close(at_step(&out, 3), 128.0);
}

#[test]
fn a_hold_stays_until_playing_from_the_top() {
    let held = song(&["C-4 -- VOL80h ---00 ---00", "C-4 -- ---00 ---00 ---00"]);
    let mut engine = Engine::new(RATE, version(held));
    engine.play_from(0);
    engine.process(&mut vec![0.0; 2 * STEP]);
    let plain = song(&["C-4 -- ---00 ---00 ---00"]);
    engine.swap(version(plain));
    engine.play_from(0);
    let mut out = vec![0.0; 2 * SETTLED];
    engine.process(&mut out);
    close(out[out.len() - 2], UNITY);
}

// Glides.

#[test]
fn a_glide_moves_in_a_line_and_lands_exactly() {
    let out = render(
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- VOLC0h ---00 ---00",
            "--- -- ---00 ---00 ---00",
        ]),
        STEP * 6,
    );
    near(out[STEP], 0x60 as f32);
    near(out[STEP * 2], 0x80 as f32);
    near(out[STEP * 3], 0xA0 as f32);
    close(at_step(&out, 4), 192.0);
    close(at_step(&out, 5), 192.0);
}

#[test]
fn a_glides_arrival_takes_the_mark_of_its_row() {
    let start = |end: &str| {
        let arrival = format!("--- -- {end} ---00 ---00");
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
            arrival.as_str(),
            "C-4 -- ---00 ---00 ---00",
        ])
    };
    let locked = render(start("VOLC0"), STEP * 4);
    close(at_step(&locked, 2), 192.0);
    close(at_step(&locked, 3), UNITY);
    let held = render(start("VOLC0h"), STEP * 4);
    close(at_step(&held, 3), 192.0);
}

#[test]
fn a_glide_wraps_through_the_loop() {
    let out = render(
        song(&[
            "C-4 -- VOLC0 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
        ]),
        STEP * 4,
    );
    near(out[STEP * 3], 0x80 as f32);
}

#[test]
fn a_glide_skips_other_commands_in_its_slot() {
    let out = render(
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- CHAFF ---00 ---00",
            "--- -- VOLC0 ---00 ---00",
        ]),
        STEP * 3,
    );
    near(out[STEP], 0x80 as f32);
}

#[test]
fn a_value_in_another_slot_ends_a_glide() {
    let out = render(
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 VOL80 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- VOLC0 ---00 ---00",
        ]),
        STEP * 4,
    );
    close(at_step(&out, 1), 128.0);
    close(at_step(&out, 2), 128.0);
}

#[test]
fn with_no_other_value_a_glide_stays_put_like_a_lock() {
    let out = render(
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
        ]),
        STEP * 3,
    );
    close(at_step(&out, 1), 64.0);
    close(at_step(&out, 2), UNITY);
}

#[test]
fn a_note_doesnt_interrupt_a_glide() {
    let out = render(
        song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- VOLC0 ---00 ---00",
        ]),
        STEP * 5,
    );
    near(out[STEP * 3], 0xA0 as f32);
}

#[test]
fn a_glide_keeps_up_with_a_tempo_change() {
    let start = song(&[
        "C-4 -- VOL40g ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- VOLC0 ---00 ---00",
    ]);
    let mut engine = Engine::new(RATE, version(start.clone()));
    engine.play_from(0);
    engine.process(&mut vec![0.0; 2 * STEP * 2]);
    let mut faster = start;
    faster.millibpm *= 2;
    engine.swap(version(faster));
    // Half the frames now cover a whole step, so that's three of the four.
    let mut out = vec![0.0; 2 * STEP / 2];
    engine.process(&mut out);
    near(out[out.len() - 2], 0xA0 as f32);
}

#[test]
fn a_glide_stops_with_a_pause() {
    let mut engine = Engine::new(
        RATE,
        version(song(&[
            "C-4 -- VOL40g ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "--- -- ---00 ---00 ---00",
            "C-4 -- ---00 ---00 ---00",
            "--- -- VOLC0 ---00 ---00",
        ])),
    );
    engine.play_from(0);
    engine.process(&mut vec![0.0; 2 * STEP]);
    engine.pause_or_resume();
    engine.process(&mut vec![0.0; 2 * STEP * 3]);
    engine.pause_or_resume();
    // A pause silences the note, so listen at the next one: three steps of the four have played.
    let mut out = vec![0.0; 2 * (STEP * 2 + 200)];
    engine.process(&mut out);
    near(out[out.len() - 2], 0xA0 as f32);
}

#[test]
fn playing_from_the_top_ends_a_glide() {
    let gliding = song(&[
        "C-4 -- VOL40g ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- VOLC0 ---00 ---00",
    ]);
    let mut engine = Engine::new(RATE, version(gliding));
    engine.play_from(0);
    engine.process(&mut vec![0.0; 2 * STEP]);
    engine.swap(version(song(&["C-4 -- ---00 ---00 ---00"])));
    engine.play_from(0);
    let mut out = vec![0.0; 2 * SETTLED];
    engine.process(&mut out);
    close(out[out.len() - 2], UNITY);
}

/// A chain's second phrase has no value, so the glide runs out and takes its target's place.
#[test]
fn a_glide_a_chain_moves_past_finishes_on_time() {
    let mut start = song(&[
        "C-4 -- VOLC0 ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- VOL40g ---00 ---00",
        "--- -- ---00 ---00 ---00",
    ]);
    let quiet = Phrase::from_steps(vec![
        "--- -- ---00 ---00 ---00".parse().unwrap(),
        "C-4 -- ---00 ---00 ---00".parse().unwrap(),
    ]);
    start.tracks[0].phrases.insert(1, Arc::new(quiet));
    let entry = |phrase| ChainEntry {
        phrase,
        transpose: 0,
    };
    let entries = vec![entry(0), entry(1)];
    start.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    let out = render(start, STEP * 6);
    close(at_step(&out, 4), 192.0);
    close(at_step(&out, 5), UNITY);
}

#[test]
fn renders_with_marks_are_repeatable() {
    let start = song(&[
        "C-4 -- VOL40g CUT40 ---00",
        "--- -- CHA80 ---00 ---00",
        "C-4 -- VOLC0h ---00 ---00",
    ]);
    assert_eq!(render(start.clone(), STEP * 8), render(start, STEP * 8));
}
