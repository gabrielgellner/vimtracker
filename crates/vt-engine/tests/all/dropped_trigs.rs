//! A note that doesn't play takes its row's trig with it (decision 429): a note dropped by a
//! failed `CHA`, the dice loop's too, or thinned by its row's `CDLT` no longer cuts or releases
//! the note sounding with its `KIL` or `OFF`, and its locks don't run, as a failed `CND` skips
//! them and as an Elektron trig's chance does. A row with no note has no trig to drop, so its FX
//! still act, and on `Pulse` the chance drops only a pulse and its note's timing, never the
//! row's locks.

use std::sync::Arc;

use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Lane, Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

/// A long flat sample, so a note's level shows for as long as it lasts.
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

/// One track playing a plain note and then `row`, its chances moved by `delta` and decided by
/// `target`, with its dice loop `dice` steps long.
fn song(row: &str, delta: u8, target: u8, dice: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.seed = 0x5EED;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.chance_delta = delta;
    track.chance_target = target;
    track.dice_loop = dice;
    let steps = ["C-3 -- ---", row]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The level halfway through the second row, each time round, for this many rounds.
fn levels(song: &Song, rounds: usize) -> Vec<f32> {
    let out = render_frames(version(song, flat_pool()), RATE, rounds * 2 * STEP);
    (0..rounds)
        .map(|round| out[((round * 2 + 1) * STEP + STEP / 2) * 2].abs())
        .collect()
}

/// The level halfway through the second row, the first time round.
fn level(song: &Song) -> f32 {
    levels(song, 1)[0]
}

#[test]
fn a_failed_cha_takes_its_kil_off_and_locks_with_it() {
    for row in [
        "C-3 -- cha:00 kil:00",
        "C-3 -- cha:00 off:00",
        "C-3 -- cha:00 vol:00",
    ] {
        assert!(
            level(&song(row, 0x80, 0, 0)) > 0.1,
            "{row} left the note sounding"
        );
    }
}

#[test]
fn a_passed_cha_keeps_its_kil_off_and_locks() {
    for row in [
        "C-3 -- cha:FF kil:00",
        "C-3 -- cha:FF off:00",
        "C-3 -- cha:FF vol:00",
    ] {
        assert!(level(&song(row, 0x80, 0, 0)) < 0.001, "{row} acted");
    }
}

#[test]
fn a_row_cdlt_thins_takes_its_kil_off_and_locks_with_it() {
    for row in ["C-3 -- kil:00", "C-3 -- off:00", "C-3 -- vol:00"] {
        // `00` thins every note away; at `80` none is.
        let thinned = format!("{row} chancedelta:00");
        assert!(
            level(&song(&thinned, 0x80, 0, 0)) > 0.1,
            "{row} thinned acted"
        );
        let kept = format!("{row} chancedelta:80");
        assert!(level(&song(&kept, 0x80, 0, 0)) < 0.001, "{row} kept didn't");
    }
}

#[test]
fn a_cha_drops_or_keeps_the_whole_trig_each_time_round() {
    // Each time round the row either plays and is cut at once, or is dropped with its `KIL` and
    // leaves the note before it sounding, with the dice loop off and on.
    for dice in [0, 16] {
        let heard = levels(&song("C-3 -- cha:80 kil:00", 0x80, 0, dice), 32);
        assert!(
            heard.iter().all(|level| *level < 0.001 || *level > 0.1),
            "{heard:?}"
        );
        let dropped = heard.iter().filter(|level| **level > 0.1).count();
        assert!((1..32).contains(&dropped), "dice {dice}: {dropped} dropped");
    }
}

#[test]
fn a_row_with_no_note_keeps_its_fx_whatever_its_cha() {
    for row in [
        "--- -- cha:00 kil:00",
        "--- -- cha:00 off:00",
        "--- -- cha:00 vol:00",
    ] {
        assert!(level(&song(row, 0x80, 0, 0)) < 0.001, "{row} didn't act");
        // `CDLT` thins notes, so a row without one isn't thinned.
        let thinned = format!("{row} chancedelta:00");
        assert!(level(&song(&thinned, 0x80, 0, 0)) < 0.001, "{row} thinned");
    }
}

#[test]
fn a_lanes_timing_goes_with_the_dropped_note_and_its_locks_keep_their_loop() {
    let laned = |row: &str| {
        let mut song = song(row, 0x80, 0, 0);
        let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
        phrase.set_lane(1, Some(Lane::new(0, 2)));
        song
    };
    // A `KIL` in a lane times the row's note, so it's dropped with it.
    assert!(level(&laned("C-3 -- cha:00 kil:00")) > 0.1);
    // A lock in a lane keeps to its own loop, as it does past a failed `CND`.
    assert!(level(&laned("C-3 -- cha:00 vol:00")) < 0.001);
}

#[test]
fn on_pulses_a_dropped_pulse_takes_its_timing_but_not_the_rows_locks() {
    // On `Pulse` the chance drops a pulse, not the row: its note's `KIL` and `OFF` go with it,
    // while the row's locks, which its later pulses play with, still run.
    assert!(level(&song("C-3 -- cha:00 kil:00", 0x80, 1, 0)) > 0.1);
    assert!(level(&song("C-3 -- cha:00 off:00", 0x80, 1, 0)) > 0.1);
    assert!(level(&song("C-3 -- cha:00 vol:00", 0x80, 1, 0)) < 0.001);
}
