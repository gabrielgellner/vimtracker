//! A row's nudge: its note part landing up to just under half a step early or late, on top of
//! where the groove lands its step, while its lanes stay on the grid.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::groove::{Groove, GrooveId};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Lane, Phrase, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 25 per tick.
const STEP: usize = 6000;
const TICK: usize = 25;
/// A nudge's unit, a 24th of a step, in frames.
const NUDGE: usize = 10 * TICK;
/// Below this, nothing's sounding.
const SILENT: f32 = 1e-4;
/// How near a note's end is to where it's due, with its fade out.
const NEAR: usize = 100;

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

/// Tracks playing these rows, panned apart when there are two so each channel is one track's.
fn song(tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.envelopes[0].attack_ms = 0;
        track.envelopes[0].release_ms = 0;
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

/// A project playing one of its own grooves.
fn with_own(mut song: Song, groove: Groove) -> Song {
    song.grooves = vec![groove];
    song.groove = GrooveId::custom(0).unwrap();
    song
}

/// The first FX column made a lane of its own, this many steps long.
fn with_lane(mut song: Song, length: usize) -> Song {
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(0, Some(Lane { start: 0, length }));
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

/// The level a flat note plays at with this velocity.
fn level(velocity: u8) -> f32 {
    0.5 * f32::from(velocity) / f32::from(0x7F_u8)
}

#[test]
fn a_nudge_lands_a_note_that_many_24ths_early_or_late() {
    let nudged = song(&[&["C-3 --", "C-3 -- +5", "C-3 -- -3", "C-3 -- -B"]]);
    assert_eq!(
        hits(&nudged, 4, 0),
        [
            0,
            STEP + 5 * NUDGE,
            2 * STEP - 3 * NUDGE,
            3 * STEP - 11 * NUDGE
        ]
    );
}

#[test]
fn a_nudge_adds_to_where_the_groove_lands_a_step() {
    // Grid, then groove, then nudge: half a step late and just under half again, and the same
    // early, which is as far as either goes.
    let late = with_own(
        song(&[&["C-3 --", "C-3 -- +B", "C-3 --"]]),
        Groove::pattern(&[0, 120, 0]),
    );
    assert_eq!(
        hits(&late, 3, 0),
        [0, STEP + 120 * TICK + 11 * NUDGE, 2 * STEP]
    );
    let early = with_own(
        song(&[&["C-3 --", "C-3 -- -B", "C-3 --"]]),
        Groove::pattern(&[0, -120, 0]),
    );
    assert_eq!(
        hits(&early, 3, 0),
        [0, STEP - 120 * TICK - 11 * NUDGE, 2 * STEP]
    );
    // On a swung step, as Swing 66 lands it.
    let mut swung = song(&[&["C-3 --", "C-3 -- -2"]]);
    swung.groove = GrooveId::swing(66).unwrap();
    assert_eq!(hits(&swung, 2, 0), [0, STEP + 77 * TICK - 2 * NUDGE]);
}

#[test]
fn a_rows_note_never_lands_before_the_last_rows() {
    // The second row lands as late as it can and the third as early, which would put the third
    // first. It waits for the second instead, and is the note left sounding.
    let groove = Groove::pattern(&[0, 120, -120, 0]);
    let rows = ["C-3 20", "C-3 40 +B", "C-3 7F -B", "--- --"];
    let crossed = with_own(song(&[&rows]), groove);
    let out = render(&crossed, flat_pool(), 4, 0);
    let at = STEP + 120 * TICK + 11 * NUDGE;
    assert!((out[at - 1] - level(0x20)).abs() < 1e-3, "{}", out[at - 1]);
    assert!(
        (out[at + 200] - level(0x7F)).abs() < 1e-3,
        "{}",
        out[at + 200]
    );
    // Without a groove, the furthest nudges either way still leave rows in order.
    let apart = song(&[&["C-3 --", "C-3 -- +B", "C-3 -- -B"]]);
    assert_eq!(
        hits(&apart, 3, 0),
        [0, STEP + 11 * NUDGE, 2 * STEP - 11 * NUDGE]
    );
}

#[test]
fn a_rows_own_fx_move_with_its_nudge() {
    // A row with no note, nudged late, turns the long note before it down where it lands.
    let nudged = song(&[&["C-3 --", "--- -- +6 vol:00", "--- --"]]);
    let out = render(&nudged, flat_pool(), 3, 0);
    let lands = STEP + 6 * NUDGE;
    assert!(out[lands - 100].abs() > 0.4, "{}", out[lands - 100]);
    assert!(out[lands + 3000].abs() < 0.001, "{}", out[lands + 3000]);
}

#[test]
fn an_early_note_starts_with_the_lanes_value_from_before() {
    // The first column is a lane: its second step turns the track down on the grid, after the
    // note nudged early on that step has started at the level before.
    let laned = with_lane(song(&[&["C-3 20", "C-3 7F -6 vol:00", "--- --"]]), 2);
    let out = render(&laned, flat_pool(), 3, 0);
    let starts = STEP - 6 * NUDGE;
    assert!((out[starts - 100] - level(0x20)).abs() < 1e-3);
    assert!((out[starts + 200] - level(0x7F)).abs() < 1e-3);
    assert!((out[STEP - 100] - level(0x7F)).abs() < 1e-3);
    assert!(out[STEP + 3000].abs() < 0.001, "{}", out[STEP + 3000]);
}

#[test]
fn a_lanes_lock_on_a_late_notes_step_lasts_that_note() {
    // The lane's lock lands on the grid and turns down the note before, and the note nudged late
    // on its step starts under it rather than ending it.
    let laned = with_lane(song(&[&["C-3 --", "C-3 -- +6 vol:00", "--- --"]]), 2);
    let out = render(&laned, flat_pool(), 3, 0);
    let starts = STEP + 6 * NUDGE;
    assert!(out[STEP - 100].abs() > 0.4);
    assert!(out[starts - 100].abs() < 0.05, "{}", out[starts - 100]);
    assert!(out[starts + 3000].abs() < 0.001, "{}", out[starts + 3000]);
}

#[test]
fn del_counts_from_where_the_nudge_lands_the_note() {
    let delayed = song(&[&["C-3 --", "C-3 -- +6 del:02"]]);
    assert_eq!(hits(&delayed, 2, 0), [0, STEP + 6 * NUDGE + 80 * TICK]);
    // Early, too: a sixth late from a quarter early.
    let delayed = song(&[&["C-3 --", "C-3 -- -6 del:01"]]);
    assert_eq!(hits(&delayed, 2, 0), [0, STEP - 6 * NUDGE + 40 * TICK]);
}

#[test]
fn every_pulse_of_a_multi_moves_with_its_rows_nudge() {
    let multi = song(&[&["C-3 -- +3 MLT3", "--- --"]]);
    assert_eq!(
        hits(&multi, 4, 0),
        [3 * NUDGE, STEP + 3 * NUDGE, 2 * STEP + 3 * NUDGE]
    );
}

#[test]
fn a_nudged_note_keeps_its_length() {
    // As Elektron's micro-timing moves a whole trig, the note's end moves with its start.
    let mut gated = song(&[&["--- --", "C-3 -- +6", "--- --", "--- --"]]);
    gated.tracks[0].gate_length = Some(0x80);
    let out = render(&gated, flat_pool(), 4, 0);
    let starts = STEP + 6 * NUDGE;
    assert!(out[starts - 10].abs() < SILENT);
    let ends = (starts + 10..out.len())
        .find(|at| out[*at].abs() < SILENT)
        .unwrap();
    let length = (0x80 as f64 / 255.0 * STEP as f64).round() as usize;
    assert!(
        (ends - starts).abs_diff(length) < NEAR,
        "{} vs {length}",
        ends - starts
    );
}

#[test]
fn only_an_early_nudge_makes_the_engine_work_ahead() {
    let lead = |rows: &[&str]| {
        let published = version(&song(&[rows]), hits_pool());
        (published.lead, published.nudges)
    };
    assert_eq!(lead(&["C-3 --", "C-3 --"]), (0, false));
    assert_eq!(lead(&["C-3 --", "C-3 -- +B"]), (0, true));
    assert_eq!(lead(&["C-3 -- -3", "C-3 -- -1"]), (30, true));
    // On top of the groove's own.
    let pushed = with_own(song(&[&["C-3 -- -2"]]), Groove::pattern(&[0, -40]));
    assert_eq!(version(&pushed, hits_pool()).lead, 60);
}

#[test]
fn an_early_nudge_on_one_track_leaves_the_others_as_they_were() {
    let plain = ["C-3 --", "C-3 40", "C-3 --", "D-3 --"];
    let without = song(&[&plain, &["C-3 --", "C-3 --"]]);
    let with = song(&[&plain, &["C-3 --", "C-3 -- -B"]]);
    // The other track's events split the blocks rendered differently, which moves nothing by
    // more than a float's rounding.
    let (without, with) = (
        render(&without, flat_pool(), 4, 0),
        render(&with, flat_pool(), 4, 0),
    );
    let most = without
        .iter()
        .zip(&with)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(most < 1e-5, "{most}");
}

#[test]
fn nudges_never_change_how_long_a_loop_takes() {
    let rows: Vec<&str> = (0..16)
        .map(|step| match step % 4 {
            1 => "C-3 -- -B",
            3 => "C-3 -- +B",
            _ => "C-3 --",
        })
        .collect();
    let straight = song(&[&vec!["C-3 --"; 16]]);
    let nudged = song(&[&rows]);
    let length =
        |song: &Song| render_loop(version(song, hits_pool()), RATE, RenderOptions::default()).len();
    assert_eq!(length(&nudged), length(&straight));
    assert_eq!(length(&nudged), 16 * STEP * 2);
}
