//! Grooves: how late or early each step lands, its velocity and a little random timing, from the
//! project, a track, or a step's `GRV`.

use std::sync::Arc;

use vt_engine::engine::{Engine, sample_of_tick};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::constants::TICKS_PER_STEP;
use vt_model::groove::{Groove, GrooveId, SWING_PERCENTS};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Lane, Phrase, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 25 per tick.
const STEP: usize = 6000;
const TICK: usize = 25;
/// Below this, nothing's sounding.
const SILENT: f32 = 1e-4;
/// How near a note's end is to where it's due, with its fade out.
const NEAR: usize = 100;

/// How late swing put every other step before grooves, in ticks, for a swing in tenths of a
/// percent, written out again here so the swing grooves are checked against it.
fn old_swing_ticks(tenths: u64) -> u64 {
    (TICKS_PER_STEP * 2 * tenths + 500) / 1000 - TICKS_PER_STEP
}

/// A short hit, so each note's start shows.
fn hits_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; 10],
        },
    );
    Arc::new(pool)
}

/// A long flat sample, so a note's level and length show.
fn flat_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[0.5, 0.5]; RATE as usize * 8],
        },
    );
    Arc::new(pool)
}

/// Tracks playing these rows, each on its own, panned apart when there are two so each channel
/// is one track's.
fn song(tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.mods[0].adsr.attack_ms = 0;
        track.mods[0].adsr.release_ms = 0;
        let pan = match (tracks.len(), index) {
            (1, _) => "",
            (_, 0) => " pan:00",
            _ => " pan:FF",
        };
        let steps = rows
            .iter()
            .map(|text| format!("{text}{pan}").parse().unwrap())
            .collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

/// A row of notes this many steps long.
fn notes(steps: usize) -> Vec<&'static str> {
    vec!["C-3 --"; steps]
}

/// A project playing one of its own grooves.
fn with_own(mut song: Song, groove: Groove) -> Song {
    song.grooves = vec![groove];
    song.groove = GrooveId::custom(0).unwrap();
    song
}

fn version(song: &Song, pool: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool))
}

/// One channel of a render, for this many steps.
fn render(song: &Song, pool: Arc<SamplePool>, steps: usize, channel: usize) -> Vec<f32> {
    render_frames(version(song, pool), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[channel])
        .collect()
}

/// Where notes begin in one channel, for songs of short hits.
fn hits(song: &Song, steps: usize, channel: usize) -> Vec<usize> {
    let out = render(song, hits_pool(), steps, channel);
    let on = |at: usize| out[at].abs() > 0.001;
    (0..out.len())
        .filter(|&at| on(at) && (at == 0 || !on(at - 1)))
        .collect()
}

/// Where each step lands, for pushes in ticks.
fn landings(pushes: &[i64]) -> Vec<usize> {
    pushes
        .iter()
        .enumerate()
        .map(|(step, push)| (step * STEP).saturating_add_signed(*push as isize * TICK as isize))
        .collect()
}

#[test]
fn the_swing_grooves_land_where_swing_did() {
    for percent in SWING_PERCENTS {
        let mut swung = song(&[&notes(4)]);
        swung.groove = GrooveId::swing(percent).unwrap();
        let late = sample_of_tick(old_swing_ticks(u64::from(percent) * 10), RATE, 120_000);
        let late = late as usize;
        assert_eq!(
            hits(&swung, 4, 0),
            [0, STEP + late, STEP * 2, STEP * 3 + late],
            "Swing {percent}"
        );
    }
    // 50 is straight, as it was.
    let mut straight = song(&[&notes(4)]);
    straight.groove = GrooveId::swing(50).unwrap();
    assert_eq!(hits(&straight, 4, 0), [0, STEP, STEP * 2, STEP * 3]);
}

#[test]
fn swing_at_any_percent_plays_as_it_did_at_any_tempo() {
    // At a tempo where a step isn't a whole number of frames, the delay is still worked out on
    // its own, as it was.
    for tenths in [520, 667, 750] {
        let mut swung = with_own(song(&[&notes(2)]), Groove::swing(tenths));
        swung.millibpm = 137_630;
        let on_grid = sample_of_tick(TICKS_PER_STEP, RATE, 137_630) as usize;
        let late = sample_of_tick(old_swing_ticks(u64::from(tenths)), RATE, 137_630) as usize;
        // Two steps here are less than two at 120 BPM, so the render has the third's start too.
        assert_eq!(hits(&swung, 2, 0)[..2], [0, on_grid + late], "{tenths}");
    }
}

#[test]
fn a_bar_of_its_own_lands_each_step_where_it_says() {
    let pushes: [i64; 16] = [
        0, 12, -24, 36, 0, -120, 120, 5, -5, 60, -60, 1, -1, 0, 90, -90,
    ];
    let groove = Groove::pattern(&pushes.map(|push| push as i16));
    let played = with_own(song(&[&notes(16)]), groove);
    assert_eq!(hits(&played, 16, 0), landings(&pushes));
}

#[test]
fn an_early_step_after_a_late_one_still_plays_in_order() {
    // The latest a step can land and the earliest the next can land are the same moment, and
    // the later step's note is the one left sounding.
    let groove = Groove::pattern(&[0, 120, -120, 0]);
    let rows = ["C-3 20", "C-3 40", "C-3 7F", "C-3 40"];
    let played = with_own(song(&[&rows]), groove);
    let out = render(&played, flat_pool(), 4, 0);
    let level = |velocity: u8| 0.5 * f32::from(velocity) / f32::from(0x7F_u8);
    let at = STEP + 120 * TICK;
    assert!((out[at - 1] - level(0x20)).abs() < 1e-3, "{}", out[at - 1]);
    assert!(
        (out[at + 200] - level(0x7F)).abs() < 1e-3,
        "{}",
        out[at + 200]
    );
}

#[test]
fn no_groove_changes_how_long_a_loop_takes_or_where_it_starts() {
    let straight = song(&[&notes(16)]);
    let loop_length = render_loop(
        version(&straight, hits_pool()),
        RATE,
        RenderOptions::default(),
    )
    .len();
    for id in GrooveId::all() {
        let mut grooved = straight.clone();
        grooved.groove = id;
        let groove = grooved.groove(id);
        let pushes: i64 = (0..16).map(|step| groove.length(step) - 240).sum();
        assert_eq!(pushes, 0, "{id}'s steps add up to a bar");
        let out = render_loop(
            version(&grooved, hits_pool()),
            RATE,
            RenderOptions::default(),
        );
        assert_eq!(out.len(), loop_length, "{id}");
        // Every pass starts on the grid.
        let found = hits(&grooved, 33, 0);
        for pass in [0, 16, 32] {
            assert!(found.contains(&(pass * STEP)), "{id}: pass at {pass}");
        }
    }
}

#[test]
fn a_render_of_an_early_groove_starts_its_pass_on_the_grid() {
    // Push lands steps early, so the engine works ahead, and the render's pass starts again from
    // the top all the same.
    let mut pushed = song(&[&notes(16)]);
    pushed.groove = GrooveId::named("Push").unwrap();
    let out = render_loop(
        version(&pushed, hits_pool()),
        RATE,
        RenderOptions::default(),
    );
    assert_eq!(out.len(), 16 * STEP * 2);
    let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    assert!(left[0].abs() > 0.5, "the first step opens it");
    // The third step lands 12 ticks early.
    let third = 2 * STEP - 12 * TICK;
    assert!(left[third].abs() > 0.5 && left[third - 1].abs() < SILENT);
}

#[test]
fn the_playhead_stays_on_the_step_heard_while_the_next_is_worked_out_early() {
    let mut pushed = song(&[&notes(16)]);
    pushed.groove = GrooveId::named("Push").unwrap();
    let mut engine = Engine::new(RATE, version(&pushed, hits_pool()));
    engine.play_from(0);
    // Just before the second step's place: it's been worked out, but the first is still heard.
    let mut out = vec![0.0; (STEP - 10) * 2];
    engine.process(&mut out);
    assert_eq!(engine.position(), 0);
    assert!(engine.step_progress() > 0.99);
    let mut out = vec![0.0; 20 * 2];
    engine.process(&mut out);
    assert_eq!(engine.position(), 1);
}

#[test]
fn a_groove_shapes_each_steps_velocity() {
    let groove = Groove::pattern(&[0, 0]).with_velocity(1, 50);
    let played = with_own(song(&[&["C-3 60", "C-3 60"]]), groove);
    let out = render(&played, flat_pool(), 2, 0);
    let (first, second) = (out[100].abs(), out[STEP + 100].abs());
    assert!(
        (second / first - 0.5).abs() < 1e-3,
        "{second} against {first}"
    );
    // Over 100% it's louder, up to the most a velocity is.
    let louder = with_own(
        song(&[&["C-3 40", "C-3 40"]]),
        Groove::pattern(&[0, 0]).with_velocity(1, 150),
    );
    let out = render(&louder, flat_pool(), 2, 0);
    assert!((out[STEP + 100] / out[100] - 1.5).abs() < 1e-2);
}

#[test]
fn humanise_is_random_within_its_ticks_and_repeats_from_the_seed() {
    let groove = Groove::pattern(&[0, 0]).with_humanise(10);
    let played = with_own(song(&[&notes(16)]), groove);
    let first = hits(&played, 16, 0);
    assert_eq!(first, hits(&played, 16, 0), "a render repeats");
    assert_eq!(first.len(), 16);
    for (step, at) in first.iter().enumerate() {
        assert!(
            at.abs_diff(step * STEP) <= 10 * TICK,
            "step {step} at {at} is within 10 ticks"
        );
    }
    let off_grid = first
        .iter()
        .enumerate()
        .filter(|(step, at)| **at != step * STEP)
        .count();
    assert!(off_grid > 8, "most steps move: {first:?}");
    let mut seeded = played.clone();
    seeded.seed = 99;
    assert_ne!(
        first,
        hits(&seeded, 16, 0),
        "another seed moves them elsewhere"
    );
}

#[test]
fn humanise_leaves_chance_rolling_as_it_did() {
    let rows = vec!["C-3 -- cha:80"; 16];
    let plain = song(&[&rows]);
    let humanised = with_own(plain.clone(), Groove::pattern(&[0]).with_humanise(4));
    let played = |song: &Song| {
        hits(song, 16, 0)
            .iter()
            .map(|at| (at + STEP / 2) / STEP)
            .collect::<Vec<_>>()
    };
    assert_eq!(played(&plain), played(&humanised));
}

#[test]
fn a_tracks_groove_replaces_the_projects_and_leaves_others_alone() {
    let rows = notes(4);
    let mut two = song(&[&rows, &rows]);
    two.groove = GrooveId::swing(70).unwrap();
    two.tracks[1].groove = Some(GrooveId::STRAIGHT);
    let late = sample_of_tick(old_swing_ticks(700), RATE, 120_000) as usize;
    assert_eq!(
        hits(&two, 4, 0),
        [0, STEP + late, STEP * 2, STEP * 3 + late]
    );
    assert_eq!(hits(&two, 4, 1), [0, STEP, STEP * 2, STEP * 3]);
}

#[test]
fn a_steps_groove_lock_plays_that_step_with_it() {
    let late = sample_of_tick(old_swing_ticks(700), RATE, 120_000) as usize;
    // `01` is Straight, so the second step lands on the grid, and the fourth is swung again, as
    // the third's note ended the lock.
    let rows = ["C-3 --", "C-3 -- groove:01", "C-3 --", "C-3 --"];
    let mut locked = song(&[&rows]);
    locked.groove = GrooveId::swing(70).unwrap();
    assert_eq!(hits(&locked, 4, 0), [0, STEP, STEP * 2, STEP * 3 + late]);
    // Held, it stays until something changes it: a fill goes straight.
    let rows = ["C-3 --", "C-3 -- groove:01h", "C-3 --", "C-3 --"];
    let mut held = song(&[&rows]);
    held.groove = GrooveId::swing(70).unwrap();
    assert_eq!(hits(&held, 4, 0), [0, STEP, STEP * 2, STEP * 3]);
    // `00` is the project's, from a track with its own.
    let rows = ["C-3 --", "C-3 -- groove:00", "C-3 --", "C-3 --"];
    let mut project = song(&[&rows]);
    project.groove = GrooveId::swing(70).unwrap();
    project.tracks[0].groove = Some(GrooveId::STRAIGHT);
    assert_eq!(hits(&project, 4, 0), [0, STEP + late, STEP * 2, STEP * 3]);
}

#[test]
fn a_lock_on_an_early_step_is_worked_out_before_it_lands() {
    // Pushed early by its own groove, which only the lock chooses.
    let pushed = GrooveId::named("Push").unwrap();
    let lock = format!("C-3 -- groove:{:02X}", pushed.index() + 1);
    let rows = ["C-3 --", "C-3 --", lock.as_str(), "C-3 --"];
    let locked = song(&[&rows]);
    assert_eq!(
        hits(&locked, 4, 0),
        [0, STEP, STEP * 2 - 12 * TICK, STEP * 3]
    );
}

#[test]
fn each_pulse_of_a_row_is_a_grooved_step() {
    let mut pulses = song(&[&["C-3 -- MLT4", "--- --", "--- --", "--- --"]]);
    pulses.groove = GrooveId::swing(70).unwrap();
    let late = sample_of_tick(old_swing_ticks(700), RATE, 120_000) as usize;
    assert_eq!(
        hits(&pulses, 4, 0),
        [0, STEP + late, STEP * 2, STEP * 3 + late]
    );
}

#[test]
fn a_lane_plays_on_the_grooved_steps() {
    // A lane of volume, three long, under one long note: its second step turns it down late.
    let mut laned = song(&[&["C-3 --", "--- -- vol:00", "--- --", "--- --"]]);
    let phrase = Arc::make_mut(laned.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 3,
        }),
    );
    laned.grooves = vec![Groove::swing(750)];
    laned.groove = GrooveId::custom(0).unwrap();
    let out = render(&laned, flat_pool(), 3, 0);
    // 120 ticks is 3000 frames: the second step's volume hasn't come in just before.
    assert!(out[STEP + 2999].abs() > 0.1);
    assert!(out[STEP + 3000 + 4000].abs() < 0.001);
}

#[test]
fn a_delay_adds_to_where_the_groove_lands_a_step() {
    let mut delayed = song(&[&["C-3 --", "C-3 -- del:02"]]);
    delayed.grooves = vec![Groove::swing(750)];
    delayed.groove = GrooveId::custom(0).unwrap();
    // 120 ticks late, then two units of 40.
    assert_eq!(hits(&delayed, 2, 0), [0, STEP + (120 + 80) * TICK]);
}

#[test]
fn a_notes_length_follows_how_long_the_groove_makes_its_step() {
    let mut gated = song(&[&["C-3 --", "C-3 --", "--- --"]]);
    gated.tracks[0].gate_length = Some(0x80);
    gated.groove = GrooveId::swing(66).unwrap();
    let out = render(&gated, flat_pool(), 3, 0);
    let heard_for = |from: usize| {
        (from..out.len())
            .find(|at| out[*at].abs() < SILENT)
            .unwrap()
            - from
    };
    let share = 0x80 as f64 / 255.0;
    // The first step is 240 + 77 ticks long, and the second 240 - 77.
    let first = (share * (240.0 + 77.0) * TICK as f64).round() as usize;
    let second_start = STEP + 77 * TICK;
    let second = (share * (240.0 - 77.0) * TICK as f64).round() as usize;
    assert!(
        heard_for(0).abs_diff(first) < NEAR,
        "{} vs {first}",
        heard_for(0)
    );
    assert!(
        heard_for(second_start).abs_diff(second) < NEAR,
        "{} vs {second}",
        heard_for(second_start)
    );
}

#[test]
fn a_long_notes_length_runs_to_where_its_last_pulse_lands() {
    // A note of two pulses, with the second pushed 40 ticks late and the third on the grid.
    let mut gated = song(&[&["C-3 -- SGL2", "--- --", "--- --"]]);
    gated.tracks[0].gate_length = Some(0x80);
    let gated = with_own(gated, Groove::pattern(&[0, 40, 0]));
    let out = render(&gated, flat_pool(), 3, 0);
    let ends = (0..out.len()).find(|at| out[*at].abs() < SILENT).unwrap();
    let share = 0x80 as f64 / 255.0;
    let expected = ((240.0 + 40.0 + share * 200.0) * TICK as f64).round() as usize;
    assert!(ends.abs_diff(expected) < NEAR, "{ends} vs {expected}");
}
