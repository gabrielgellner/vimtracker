//! Gate cells: each row's gate type and its length in pulses.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Lane, Phrase, Song};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note that starts again starts from 0, and a faster one climbs
/// faster.
const RAMP_FRAMES: usize = RATE as usize * 8;

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("ramp", SampleData { frames });
    Arc::new(pool)
}

/// One track playing these rows, with notes that start and stop at once.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.sample_on(0, "ramp");
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    render_frames(version(song), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// How far the ramp has climbed a little into a step, past the fade of a note cut short: near 0
/// if a note started there.
fn into(out: &[f32], step: usize) -> f32 {
    out[step * STEP + 500]
}

/// How fast the ramp climbs in a step, which doubles an octave up.
fn slope(out: &[f32], step: usize) -> f32 {
    out[step * STEP + 2010] - out[step * STEP + 10]
}

/// Below this, a note started in the step. A note carried on from the step before is at least
/// a step's climb, `6000 / RAMP_FRAMES`, about 0.016.
const STARTED: f32 = 0.005;

#[test]
fn single_plays_one_note_across_its_pulses() {
    let out = render(&song(&["C-3 -- SGL3", "C-3 --"]), 4);
    assert!(into(&out, 0) < STARTED);
    assert!(into(&out, 1) > STARTED, "it carries on");
    assert!(into(&out, 2) > STARTED);
    assert!(into(&out, 3) < STARTED, "the next row, after 3 pulses");
}

#[test]
fn multi_plays_a_note_on_every_pulse() {
    let out = render(&song(&["C-3 -- MLT3", "--- --"]), 4);
    for step in 0..3 {
        assert!(into(&out, step) < STARTED, "pulse {step}");
    }
    assert!(into(&out, 3) > STARTED, "the empty row lets it ring");
}

#[test]
fn hold_ties_into_the_next_note_with_no_new_attack() {
    let out = render(&song(&["C-3 -- HLD2", "C-4 --", "--- --"]), 3);
    assert!(into(&out, 2) > STARTED, "no new attack");
    let ratio = slope(&out, 2) / slope(&out, 1);
    assert!((ratio - 2.0).abs() < 0.05, "an octave up: {ratio}");
    // Without the hold, the next note starts again.
    let out = render(&song(&["C-3 -- SGL2", "C-4 --"]), 3);
    assert!(into(&out, 2) < STARTED);
}

#[test]
fn a_hold_with_nothing_to_tie_into_carries_on_as_a_single() {
    let out = render(&song(&["C-3 -- HLD2", "--- --", "C-4 --"]), 4);
    assert!(into(&out, 2) > STARTED, "rings on through the empty row");
    assert!(
        into(&out, 3) < STARTED,
        "and the note after that starts again"
    );
}

#[test]
fn rest_plays_nothing_and_ends_what_is_sounding() {
    let out = render(&song(&["C-3 --", "C-3 -- RST2", "C-3 --"]), 4);
    assert!(into(&out, 0) < STARTED);
    let quiet = out[STEP + 200..3 * STEP]
        .iter()
        .all(|level| level.abs() < 1e-6);
    assert!(quiet, "silent for its 2 pulses");
    assert!(into(&out, 3) < STARTED, "the next row starts on time");
    assert!(out[3 * STEP + 1000] > 0.0);
}

#[test]
fn a_row_of_0_pulses_is_skipped_and_takes_no_time() {
    let out = render(&song(&["--- --", "C-4 -- SGL0", "C-3 --"]), 3);
    // Row 2 plays on step 1, and the skipped row never plays.
    assert_eq!(out[..STEP].iter().copied().fold(0.0, f32::max), 0.0);
    assert!(into(&out, 1) < STARTED && out[STEP + 2000] > 0.0);
}

#[test]
fn a_loop_of_skipped_rows_is_silent_and_doesnt_hang() {
    let out = render(&song(&["C-3 -- SGL0", "C-3 -- HLD0"]), 4);
    assert!(out.iter().all(|level| *level == 0.0));
}

#[test]
fn an_empty_row_with_a_gate_still_takes_its_time() {
    let out = render(&song(&["C-3 --", "--- -- RST3", "C-3 --"]), 5);
    assert!(
        into(&out, 1) > STARTED,
        "the first note rings through the empty rest"
    );
    assert!(into(&out, 4) < STARTED, "the next note is 1 + 3 steps in");
}

#[test]
fn a_lane_runs_on_steps_and_a_column_without_one_follows_the_rows() {
    let rows = [
        "C-3 -- MLT4 sna:10h --- ---",
        "--- -- sna:20h --- ---",
        "--- -- sna:30h --- ---",
    ];
    let send = |song: &Song, place: usize| {
        let mut engine = Engine::new(RATE, version(song));
        engine.play_from(0);
        let mut out = vec![0.0; (place * STEP + 1) * 2];
        engine.process(&mut out);
        engine.live(0).send_a
    };
    let following = song(&rows);
    // Row 0 lasts steps 0 to 3, so its FX hold all that time.
    assert_eq!(send(&following, 2), 0x10);
    assert_eq!(send(&following, 4), 0x20);
    let mut drifting = following.clone();
    Arc::make_mut(drifting.tracks[0].phrases.get_mut(&0).unwrap()).set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 3,
        }),
    );
    // The lane moves on each step, inside the long row.
    assert_eq!(send(&drifting, 1), 0x20);
    assert_eq!(send(&drifting, 2), 0x30);
    assert_eq!(send(&drifting, 3), 0x10);
}

#[test]
fn a_rows_pulses_changed_while_it_plays_keep_its_note_going() {
    let long = song(&["C-3 -- HLD4", "--- --"]);
    let mut engine = Engine::new(RATE, version(&long));
    engine.play_from(0);
    let mut out = vec![0.0; (2 * STEP + 1) * 2];
    engine.process(&mut out);
    let longer = song(&["C-3 -- HLD6", "--- --"]);
    engine.swap(version(&longer));
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert!(
        out[500 * 2] > STARTED,
        "still the same note, not started again"
    );
}
