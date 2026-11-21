//! A track's `Velocity range` (`VRNG`): how far below a full velocity's level velocity `01` plays,
//! even in dB between, as Ableton Live's Simpler and Surge have it, with `7F` at the voice's own
//! level and `00` silent (decision 478).

use std::sync::Arc;

use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{DEFAULT_VELOCITY_RANGE, MOST_VELOCITY_RANGE};
use vt_model::song::{Machine, Song, SynthVoice};

use crate::support::panned::{render, song};

/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// A sample at full scale, flat, so a note's level is its gain.
fn flat_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; 48_000 * 4],
        },
    );
    Arc::new(pool)
}

/// Each step's level in dB, halfway into it.
fn levels_db(song: &Song, steps: usize) -> Vec<f32> {
    let out = render(song, flat_pool(), steps, 0);
    (0..steps)
        .map(|step| 20.0 * out[step * STEP + STEP / 2].abs().log10())
        .collect()
}

/// A note a step at these velocities, on a track whose `Velocity range` is `range`.
fn ranged(velocities: &[&str], range: u8) -> Song {
    let rows: Vec<String> = velocities.iter().map(|v| format!("C-3 {v}")).collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut song = song(&[&rows]);
    song.tracks[0].velocity_range = range;
    song
}

#[track_caller]
fn near(heard: &[f32], expected: &[f32]) {
    assert_eq!(heard.len(), expected.len());
    for (at, (h, e)) in heard.iter().zip(expected).enumerate() {
        assert!(
            (h - e).abs() < 0.05,
            "step {at}: {heard:?}, not {expected:?}"
        );
    }
}

#[test]
fn it_starts_at_24_db_with_7f_at_the_voices_own_level() {
    let song = ranged(&["7F", "40", "01"], DEFAULT_VELOCITY_RANGE);
    assert_eq!(Song::new(1).tracks[0].velocity_range, 24);
    let half = -24.0 * 63.0 / 126.0;
    near(&levels_db(&song, 3), &[0.0, half, -24.0]);
}

#[test]
fn each_step_of_velocity_is_as_far_in_db_as_the_next() {
    let velocities = ["7F", "60", "40", "20", "01"];
    for range in [12, 24, 36, MOST_VELOCITY_RANGE] {
        let expected: Vec<f32> = velocities
            .iter()
            .map(|v| {
                let v = u8::from_str_radix(v, 16).unwrap();
                -f32::from(range) * f32::from(127 - v) / 126.0
            })
            .collect();
        near(&levels_db(&ranged(&velocities, range), 5), &expected);
    }
}

#[test]
fn a_range_of_00_leaves_the_level_alone_and_past_the_top_it_stops() {
    near(
        &levels_db(&ranged(&["7F", "40", "01"], 0), 3),
        &[0.0, 0.0, 0.0],
    );
    let mut past = ranged(&["01"], MOST_VELOCITY_RANGE);
    past.tracks[0].velocity_range = 0xFF;
    near(&levels_db(&past, 1), &[-48.0]);
}

#[test]
fn a_lock_on_a_notes_row_sets_it_for_that_note() {
    let rows = [
        "C-3 40 velocityrange:30",
        "C-3 40",
        "C-3 40 velocityrange:00",
    ];
    let song = song(&[&rows]);
    near(&levels_db(&song, 3), &[-24.0, -12.0, 0.0]);
}

#[test]
fn acid_plays_at_its_own_level_whatever_the_range() {
    let acid = |range: u8| {
        let mut song = ranged(&["40", "7F", "01"], range);
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        render(&song, flat_pool(), 3, 0)
    };
    assert_eq!(acid(0), acid(MOST_VELOCITY_RANGE));
}
