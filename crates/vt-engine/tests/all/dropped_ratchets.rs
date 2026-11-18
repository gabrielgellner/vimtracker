//! A `RET` goes with its note (decision 427): a note written on a row that doesn't play, as a
//! failed `CHA` or `CND` or `CDLT`'s thinning drops it, takes its `RET` with it rather than
//! striking the note before it again, as an Elektron trig that fails takes its retrigs. A row
//! with no note still strikes the one sounding.

use std::sync::Arc;

use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

use crate::support::version;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A sixth of a step, `RET`'s unit.
const UNIT: usize = STEP / 6;
/// How many times round the two rows play.
const ROUNDS: usize = 48;

/// A short hit, so each note's start shows, and the note before a row has ended by its hits.
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
    let steps = ["C-3 --", row]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn render(song: &Song, steps: usize) -> Vec<f32> {
    render_frames(version(song, hits_pool()), RATE, steps * STEP)
}

/// Whether a note starts at a frame, in the left channel.
fn struck(out: &[f32], frame: usize) -> bool {
    out[frame * 2].abs() > 0.001
}

/// For each pulse of the second row, one a step, each time round, whether its note and each of
/// its `RET 02` hits after it, two units apart, struck.
fn pulses(row: &str, delta: u8, target: u8, dice: u8, length: usize) -> Vec<[bool; 3]> {
    let period = 1 + length;
    let out = render(&song(row, delta, target, dice), ROUNDS * period);
    (0..ROUNDS)
        .flat_map(|round| (1..period).map(move |pulse| round * period + pulse))
        .map(|step| [0, 1, 2].map(|hit| struck(&out, step * STEP + hit * 2 * UNIT)))
        .collect()
}

/// That every pulse whose note is dropped is silent through its hits, and on `Row` every one
/// whose note plays has them all, with some of each. How many were dropped.
fn dropped_pulses_are_silent(heard: &[[bool; 3]], whole: bool) -> usize {
    let dropped = heard.iter().filter(|pulse| !pulse[0]).count();
    for (number, pulse) in heard.iter().enumerate() {
        if !pulse[0] {
            assert_eq!(pulse, &[false; 3], "pulse {number} dropped but struck");
        } else if whole {
            assert_eq!(pulse, &[true; 3], "pulse {number} played without its hits");
        }
    }
    dropped
}

#[test]
fn a_failed_cha_drops_its_ret() {
    let heard = pulses("C-3 -- cha:80 ret:02", 0x80, 0, 0, 1);
    let dropped = dropped_pulses_are_silent(&heard, true);
    assert!((8..=40).contains(&dropped), "about half: {dropped}");
    // `CHA 00` never plays, so nothing of the row is heard.
    let heard = pulses("C-3 -- cha:00 ret:02", 0x80, 0, 0, 1);
    assert_eq!(dropped_pulses_are_silent(&heard, true), ROUNDS);
}

#[test]
fn a_dropped_row_plays_as_it_would_with_no_ret() {
    for row in ["C-3 -- cha:00", "C-3 -- cnd:09", "C-3 -- cnd:D0"] {
        let without = render(&song(row, 0x80, 0, 0), 2 * 8);
        let with = render(&song(&format!("{row} ret:02"), 0x80, 0, 0), 2 * 8);
        assert!(with == without, "{row}");
    }
}

#[test]
fn a_failed_cha_on_the_dice_loop_drops_its_ret() {
    // The loop holds its dice, so the row lands on the same few places each time round.
    let heard = pulses("C-3 -- cha:80 ret:02", 0x80, 0, 4, 1);
    assert!(dropped_pulses_are_silent(&heard, true) > 0);
}

#[test]
fn a_failed_percent_drops_its_ret() {
    // `E8` is 50%.
    let heard = pulses("C-3 -- cnd:E8 ret:02", 0x80, 0, 0, 1);
    let dropped = dropped_pulses_are_silent(&heard, true);
    assert!((8..=40).contains(&dropped), "about half: {dropped}");
}

#[test]
fn lst_with_no_last_pass_drops_its_ret() {
    // A phrase looping in its chain has no last pass, so `LST` never holds.
    let heard = pulses("C-3 -- cnd:09 ret:02", 0x80, 0, 0, 1);
    assert_eq!(dropped_pulses_are_silent(&heard, true), ROUNDS);
}

#[test]
fn a_row_cdlt_thins_drops_its_ret() {
    let heard = pulses("C-3 -- ret:02", 0x40, 0, 0, 1);
    let dropped = dropped_pulses_are_silent(&heard, true);
    assert!((8..=40).contains(&dropped), "about half: {dropped}");
    let heard = pulses("C-3 -- ret:02", 0x00, 0, 0, 1);
    assert_eq!(dropped_pulses_are_silent(&heard, true), ROUNDS);
}

#[test]
fn a_ret_on_a_row_with_no_note_strikes_the_one_sounding() {
    for delta in [0x00, 0x80] {
        let heard = pulses("--- -- ret:02", delta, 0, 0, 1);
        if delta == 0x00 {
            // Nothing plays, so nothing is sounding to strike.
            assert!(heard.iter().all(|pulse| pulse == &[false; 3]));
        } else {
            assert!(
                heard.iter().all(|pulse| pulse == &[false, true, true]),
                "{heard:?}"
            );
        }
    }
    // A `CHA` on it has no note to skip, so it still strikes.
    let heard = pulses("--- -- cha:00 ret:02", 0x80, 0, 0, 1);
    assert!(heard.iter().all(|pulse| pulse == &[false, true, true]));
}

#[test]
fn a_multis_dropped_pulses_drop_their_ret() {
    // On `Row`, a dropped `MLT` row is silent through all its pulses and their hits.
    let heard = pulses("C-3 -- MLT2 cha:80 ret:02", 0x80, 0, 0, 2);
    let dropped = dropped_pulses_are_silent(&heard, true);
    assert!((16..=80).contains(&dropped), "about half: {dropped}");
    for row in heard.chunks(2) {
        assert_eq!(row[0][0], row[1][0], "the row's pulses go together");
    }
}

#[test]
fn on_pulses_each_pulse_takes_its_ret_with_it() {
    // On `Pulse`, a pulse whose note is dropped, the first by the row's chance or a later one by
    // its own roll, takes its hits with it; the row's later pulses still roll their own.
    let heard = pulses("C-3 -- MLT2 cha:80 ret:02", 0x80, 1, 0, 2);
    let dropped = dropped_pulses_are_silent(&heard, false);
    assert!((16..=80).contains(&dropped), "about half: {dropped}");
    let after_a_dropped_first = heard
        .chunks(2)
        .filter(|row| !row[0][0] && row[1][0])
        .count();
    assert!(after_a_dropped_first > 0, "a dropped pulse leaves the rest");
    // A row whose first pulse is dropped strikes nothing until its next pulse.
    let heard = pulses("C-3 -- cha:80 ret:02", 0x80, 1, 0, 1);
    dropped_pulses_are_silent(&heard, false);
}

#[test]
fn renders_repeat() {
    for (row, target) in [
        ("C-3 -- MLT2 cha:80 ret:02", 1),
        ("C-3 -- cha:80 ret:02", 0),
    ] {
        let song = song(row, 0x60, target, 0);
        assert!(render(&song, 64) == render(&song, 64), "{row}");
    }
}

#[test]
fn on_the_acid_a_dropped_row_plays_as_it_would_with_no_ret() {
    let acid = |row: &str| {
        let mut song = Song::new(1);
        song.limiter = false;
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        let steps = ["C-1 64", row, "C-1 7F MLT2", "--- --"]
            .iter()
            .map(|text| text.parse().unwrap())
            .collect();
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
        render_frames(
            version(&song, Arc::new(SamplePool::default())),
            RATE,
            10 * STEP,
        )
    };
    assert!(acid("C-1 64 cha:00 ret:02") == acid("C-1 64 cha:00"));
    assert!(acid("C-1 64 MLT2 cha:00 ret:02") == acid("C-1 64 MLT2 cha:00"));
    // A row with no note still strikes the 303's note again.
    assert!(acid("--- -- ret:02") != acid("--- --"));
}
