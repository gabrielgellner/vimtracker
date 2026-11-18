//! An FX column's lane's `Hold` (decision 422): the chance each of the lane's rows keeps the row it
//! played before instead of its own, so its value holds, as Kria's per-step probability does. It's
//! decided as each row starts, for all its steps, rolled from the seed on a stream of its own, so
//! a render repeats and its `Jump` rolls as it did; at `00` every lane plays as before.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Phrase, Song};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The rows of the lane, each a send of its own.
const ROWS: usize = 8;

/// One track playing a phrase of 64 rows whose FX 1 lane is its first eight, `division` steps a
/// row, each a send of its own, `10` to `80`, with that lane's `Jump` and `Hold`.
fn song(division: usize, jump: u8, hold: u8) -> Song {
    let mut phrase = Phrase::new(64);
    for row in 0..ROWS {
        let text = format!("--- -- sna:{:02X}h --- ---", (row + 1) * 0x10);
        *phrase.step_mut(row).unwrap() = text.parse::<Step>().unwrap();
    }
    phrase.set_lane(
        0,
        Some(Lane {
            division,
            ..Lane::new(0, ROWS)
        }),
    );
    let mut song = Song::new(1);
    song.seed = 0x5EED;
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song.tracks[0].lanes[0].jump = jump;
    song.tracks[0].lanes[0].hold = hold;
    song
}

/// The stored row FX 1's lane reads at each of this many steps, and the send it leaves.
fn heard(song: &Song, steps: usize) -> Vec<(usize, u8)> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    (0..steps)
        .map(|_| {
            engine.process(&mut out);
            let row = engine.track_heard(0).lanes[0].unwrap().row;
            (row, engine.live(0).send_a)
        })
        .collect()
}

fn rows(song: &Song, steps: usize) -> Vec<usize> {
    heard(song, steps).iter().map(|(row, _)| *row).collect()
}

/// The rows the lane counts to with nothing moving it, one a step.
fn counted(steps: usize) -> Vec<usize> {
    (0..steps).map(|step| step % ROWS).collect()
}

#[test]
fn at_none_it_counts_and_at_full_it_keeps_its_first_row() {
    assert_eq!(rows(&song(1, 0, 0), 64), counted(64));
    // At full, every row after the first keeps the one before, so the first is held throughout.
    assert_eq!(rows(&song(1, 0, 0xFF), 64), vec![0; 64]);
    // Its value holds with it.
    assert!(
        heard(&song(1, 0, 0xFF), 16)
            .iter()
            .all(|(_, send)| *send == 0x10)
    );
}

#[test]
fn halfway_about_half_the_rows_keep_the_one_before() {
    let heard = rows(&song(1, 0, 0x80), 512);
    let counted = counted(512);
    let mut held = 0;
    for step in 1..512 {
        if heard[step] != counted[step] {
            // Anything not counted is the row before, kept.
            assert_eq!(heard[step], heard[step - 1], "step {step}: {heard:?}");
            held += 1;
        }
    }
    // Half hold, though one kept on the row it would count to anyway isn't seen.
    assert!((180..290).contains(&held), "{held} of 512");
    assert_eq!(heard, rows(&song(1, 0, 0x80), 512), "a render repeats");
}

#[test]
fn a_slow_row_holds_once_for_all_its_steps() {
    let heard = rows(&song(3, 0, 0x80), 96);
    for row in heard.chunks(3) {
        assert!(row.iter().all(|at| *at == row[0]), "{heard:?}");
    }
    let rows: Vec<usize> = heard.iter().step_by(3).copied().collect();
    assert_ne!(rows, counted(32));
}

#[test]
fn it_keeps_a_jumped_row_and_leaves_the_jumps_as_they_were() {
    // A held row plays what the row before played, a jump's row too, and the jumps that aren't
    // held land where they did without it, as the two roll apart.
    let jumping = rows(&song(1, 0x80, 0), 256);
    let both = rows(&song(1, 0x80, 0x80), 256);
    let mut held = 0;
    for step in 1..256 {
        if both[step] != jumping[step] {
            assert_eq!(both[step], both[step - 1], "step {step}");
            held += 1;
        }
    }
    assert!(held > 64, "{held} of 256");
    assert!(
        (1..256)
            .any(|step| both[step] == both[step - 1] && both[step - 1] != counted(256)[step - 1]),
        "a jumped row is kept: {both:?}"
    );
}

#[test]
fn a_lock_turns_it_from_the_next_row_on() {
    // An `lane1.hold:FF` on the phrase's tenth row, outside the lane, holds from then on.
    let mut locked = song(1, 0, 0);
    let phrase = Arc::make_mut(locked.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(9).unwrap() = "--- -- --- lane1.hold:FF ---".parse().unwrap();
    let heard = rows(&locked, 64);
    assert_eq!(heard[..10], counted(10)[..], "{heard:?}");
    assert!(heard[11..].iter().all(|row| *row == heard[10]), "{heard:?}");
    assert_eq!(SettingId::lane(0, 4).unwrap().key(), "lane1.hold");
}

#[test]
fn cue_never_holds_it_back() {
    let mut cued = song(1, 0, 0x80);
    cued.tracks[0].cue = 1;
    assert_eq!(rows(&cued, 64), rows(&song(1, 0, 0x80), 64));
}
