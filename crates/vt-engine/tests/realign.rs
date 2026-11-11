//! Realigning by hand: a key sends every track back to its start at the next line of the launch
//! grid, which is a reset that comes once, when it's played, rather than on a schedule.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::accumulator::{Accumulation, Reset};
use vt_model::groove::GrooveId;
use vt_model::order::Order;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Launch, Phrase, Song};
use vt_model::speed::Speed;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Halfway into a step, where its level is read.
const HALF: usize = STEP / 2;

/// A long ramp, so a note's pitch is how fast it climbs and its velocity how high it starts.
fn pool() -> Arc<SamplePool> {
    let frames = RATE as usize * 8;
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: (0..frames)
                .map(|at| {
                    let level = 0.25 + 0.5 * at as f32 / frames as f32;
                    [level, level]
                })
                .collect(),
        },
    );
    Arc::new(pool)
}

/// Tracks playing these rows, with notes that start and stop at once, in a section long enough
/// that the launch grid isn't cut short by its repeats.
fn song(tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    song.sections[0].length = Some(64);
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.mods[0].adsr.attack_ms = 0;
        track.mods[0].adsr.release_ms = 0;
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// Three rows, each its own velocity, so a step's level says which row played.
const ROWS: [&str; 3] = ["C-3 40", "C-3 20", "C-3 10"];

/// Play from the top, ask for a realign partway into this step, and give the level in each step
/// up to this many.
fn levels(song: &Song, cue_in: usize, steps: usize) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut before = vec![0.0; (cue_in * STEP + HALF) * 2];
    engine.process(&mut before);
    engine.toggle_realign();
    let mut after = vec![0.0; (steps * STEP - cue_in * STEP - HALF) * 2];
    engine.process(&mut after);
    before.extend(after);
    (0..steps)
        .map(|step| before[(step * STEP + 100) * 2])
        .collect()
}

/// The row each step played, from its level, by the velocity [`ROWS`] gives it.
fn rows_played(levels: &[f32]) -> Vec<usize> {
    let loudest = levels.iter().copied().fold(0.0, f32::max);
    levels
        .iter()
        .map(|level| match level / loudest {
            ratio if ratio > 0.75 => 0,
            ratio if ratio > 0.375 => 1,
            _ => 2,
        })
        .collect()
}

#[test]
fn a_realign_waits_for_the_next_line_of_the_launch_grid() {
    let mut three = song(&[&ROWS]);
    three.launch = Launch::every(4).unwrap();
    // Asked for in step 1, it lands on step 4, where the track would have played its second row.
    let played = rows_played(&levels(&three, 1, 9));
    assert_eq!(played, [0, 1, 2, 0, 0, 1, 2, 0, 1]);
}

#[test]
fn on_a_grid_of_a_step_it_lands_on_the_next() {
    let mut three = song(&[&ROWS]);
    three.launch = Launch::named("step").unwrap();
    let played = rows_played(&levels(&three, 1, 5));
    assert_eq!(played, [0, 1, 0, 1, 2]);
}

#[test]
fn with_launches_at_a_chains_end_it_waits_for_the_bar() {
    let mut three = song(&[&ROWS]);
    three.launch = Launch::ChainEnd;
    let played = rows_played(&levels(&three, 1, 18));
    // Without it, step 16 would play the second row.
    assert_eq!(played[15..], [0, 0, 1]);
}

#[test]
fn asking_again_takes_it_back_and_stopping_forgets_it() {
    let three = song(&[&ROWS]);
    let mut engine = Engine::new(RATE, version(&three));
    engine.toggle_realign();
    assert!(
        !engine.realign_waiting(),
        "nothing to realign while stopped"
    );
    engine.play_from(0);
    engine.process(&mut vec![0.0; HALF * 2]);
    engine.toggle_realign();
    assert!(engine.realign_waiting());
    engine.toggle_realign();
    assert!(!engine.realign_waiting(), "a second press takes it back");
    engine.toggle_realign();
    engine.pause_or_resume();
    assert!(!engine.realign_waiting(), "pausing forgets it");
}

#[test]
fn it_waits_until_its_line_and_then_is_done() {
    let three = song(&[&ROWS]);
    let mut engine = Engine::new(RATE, version(&three));
    engine.play_from(0);
    engine.process(&mut vec![0.0; HALF * 2]);
    engine.toggle_realign();
    engine.process(&mut vec![0.0; 15 * STEP * 2]);
    assert!(engine.realign_waiting(), "still waiting in step 15");
    engine.process(&mut vec![0.0; STEP * 2]);
    assert!(!engine.realign_waiting(), "landed on the bar");
}

/// Tracks that drift apart, with everything that counts from a chain's start: a swing that counts
/// pairs from it, a condition on its first pass, an accumulator that starts again with its chain,
/// an up-down walk, and a track at its own speed.
fn drifting() -> Song {
    let mut song = song(&[
        &[
            "C-3 40 cnd:07",
            "C-3 20 acu:01",
            "C-3 30",
            "--- --",
            "C-3 10",
        ],
        &[
            "C-3 40", "D-3 20", "E-3 30", "F-3 10", "G-3 40", "A-3 20", "B-3 30",
        ],
        &["C-3 40 acu:02", "--- --", "E-3 20"],
    ]);
    song.groove = GrooveId::swing(60).unwrap();
    song.launch = Launch::every(16).unwrap();
    song.tracks[0].accumulation = Accumulation {
        reset: Reset::Chain,
        ..Accumulation::default()
    };
    let mut walk = (*song.tracks[1].phrases[&0]).clone();
    walk.set_order(Order::UpDown);
    song.tracks[1].phrases.insert(0, Arc::new(walk));
    song.tracks[2].speed = Speed::EighthTriplet;
    song
}

/// The whole of the first 32 steps, asking for a realign partway into step 5 if `realign`.
fn played(song: &Song, realign: bool) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; (5 * STEP + HALF) * 2];
    engine.process(&mut out);
    if realign {
        engine.toggle_realign();
    }
    let mut rest = vec![0.0; (27 * STEP - HALF) * 2];
    engine.process(&mut rest);
    out.extend(rest);
    out
}

#[test]
fn a_realign_starts_everything_again_as_a_reset_does() {
    let free = drifting();
    let mut reset = drifting();
    reset.sections[0].reset = Some(16);
    let realigned = played(&free, true);
    assert_ne!(realigned, played(&free, false), "the realign is heard");
    // A reset every 16 steps first comes at step 16, where the realign lands, and the next is
    // past the end, so the two play the same, sample for sample.
    assert_eq!(realigned, played(&reset, false));
}
