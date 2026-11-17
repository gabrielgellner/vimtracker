//! An FX column's lane's `Jump` (decision 415): the chance each of the lane's rows plays any of its
//! own rows instead of the next it counts to, as `JMP` is a phrase's (decision 391). It's read as
//! each of the lane's rows starts, as its `Start` is, so a lock, an LFO or a macro moves it; it's
//! rolled from the seed, so a render repeats; and the track's `Cue` never holds it back.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::lane::Lane;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::macros::{MacroMap, MacroTarget, Zone};
use vt_model::modulation::ModSlot;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The rows of the lane, each a send of its own.
const ROWS: usize = 8;

/// One track playing a phrase of 64 rows whose FX 1 lane is its first eight, `division` steps a
/// row, each a send of its own, `10` to `80`, and whose FX 1 lane `Jump` is `jump`.
fn song(division: usize, jump: u8) -> Song {
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
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song.tracks[0].lanes[0].jump = jump;
    song
}

/// The stored row FX 1's lane reads at each of this many steps.
fn rows(song: &Song, steps: usize) -> Vec<usize> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    (0..steps)
        .map(|_| {
            engine.process(&mut out);
            engine.track_heard(0).lanes[0].unwrap().row
        })
        .collect()
}

/// The rows the lane counts to with no `Jump`, one a step.
fn counted(steps: usize) -> Vec<usize> {
    (0..steps).map(|step| step % ROWS).collect()
}

/// How many of the steps read another row than the count's.
fn jumps(heard: &[usize]) -> usize {
    heard
        .iter()
        .zip(counted(heard.len()))
        .filter(|(heard, counted)| **heard != *counted)
        .count()
}

#[test]
fn at_none_a_lane_counts_and_at_full_each_row_is_any_of_its_own() {
    assert_eq!(rows(&song(1, 0), 64), counted(64));
    let full = song(1, 0xFF);
    let heard = rows(&full, 128);
    assert!(heard.iter().all(|row| *row < ROWS), "{heard:?}");
    let mut seen = heard.clone();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), ROWS, "any of its rows: {heard:?}");
    // A jump lands on the row counted to an eighth of the time.
    assert!(jumps(&heard) > 128 * 3 / 4, "{heard:?}");
    assert_eq!(heard, rows(&full, 128), "a render repeats");
    // The sends follow the rows read.
    let mut engine = Engine::new(RATE, version(&full));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    for _ in 0..16 {
        engine.process(&mut out);
        let row = engine.track_heard(0).lanes[0].unwrap().row;
        assert_eq!(usize::from(engine.live(0).send_a), (row + 1) * 0x10);
    }
}

#[test]
fn halfway_about_half_the_rows_jump_and_the_count_carries_on() {
    let heard = rows(&song(1, 0x80), 512);
    // Half jump, and an eighth of those land on the row counted to anyway.
    let jumped = jumps(&heard);
    assert!((160..290).contains(&jumped), "{jumped} of 512");
    // A row that doesn't jump is the count's, so a jump replaces one row and the lane carries on.
    let counted = counted(512);
    let kept = heard
        .iter()
        .zip(&counted)
        .filter(|(heard, counted)| heard == counted)
        .count();
    assert_eq!(kept, 512 - jumped);
}

#[test]
fn a_slow_row_jumps_once_for_all_its_steps() {
    let heard = rows(&song(3, 0xFF), 96);
    for row in heard.chunks(3) {
        assert!(row.iter().all(|at| *at == row[0]), "{heard:?}");
    }
    assert_ne!(
        heard.iter().step_by(3).copied().collect::<Vec<_>>(),
        counted(32)
    );
}

#[test]
fn each_column_rolls_its_own() {
    // The same lane on FX 1 and FX 2, both at full: they jump apart.
    let mut song = song(1, 0xFF);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(1, Some(Lane::new(0, ROWS)));
    song.tracks[0].lanes[1].jump = 0xFF;
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    let mut apart = 0;
    for _ in 0..32 {
        engine.process(&mut out);
        let lanes = engine.track_heard(0).lanes;
        apart += usize::from(lanes[0].unwrap().row != lanes[1].unwrap().row);
    }
    assert!(apart > 16, "{apart} of 32");
}

#[test]
fn playing_from_a_step_jumps_as_playing_from_the_top() {
    let full = song(1, 0xFF);
    let top = rows(&full, 40);
    for place in [5, 13, 27] {
        let mut from = Engine::new(RATE, version(&full));
        from.play_from(place);
        let mut out = vec![0.0; STEP * 2];
        from.process(&mut out);
        assert_eq!(
            from.track_heard(0).lanes[0].unwrap().row,
            top[place],
            "from step {place}"
        );
    }
}

#[test]
fn jumps_stay_within_its_own_rows_whatever_its_start_and_length() {
    // Started three rows on and heard for four, the count plays rows 3 to 6; a jump any of the
    // eight, as `JMP` plays any row of a phrase's loop whatever its window.
    let mut moved = song(1, 0x80);
    moved.tracks[0].lanes[0].start = 3;
    moved.tracks[0].lanes[0].length = 0x7C;
    let heard = rows(&moved, 256);
    assert!(heard.iter().all(|row| *row < ROWS), "{heard:?}");
    let outside = heard.iter().filter(|row| !(3..7).contains(*row)).count();
    assert!(outside > 20, "jumps reach its other rows: {heard:?}");
    // At none, the same lane plays its window.
    let mut still = moved;
    still.tracks[0].lanes[0].jump = 0;
    let window: Vec<usize> = (0..32).map(|step| 3 + step % 4).collect();
    assert_eq!(rows(&still, 32), window);
}

#[test]
fn a_lock_sets_the_chance_from_the_next_row_on() {
    // An `lane1.jump:FF` on the phrase's tenth row, outside the lane, holds from then on.
    let mut locked = song(1, 0);
    let phrase = Arc::make_mut(locked.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(9).unwrap() = "--- -- --- lane1.jump:FF ---".parse().unwrap();
    let heard = rows(&locked, 64);
    assert_eq!(heard[..10], counted(10)[..], "{heard:?}");
    assert!(jumps(&heard[..]) > 32, "{heard:?}");
}

#[test]
fn an_lfo_on_jump_scrambles_its_high_half_alone() {
    let mut lfo = song(1, 0);
    lfo.tracks[0].mods[3] = ModSlot::lfo(
        SettingId::lane(0, 3),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(32),
            ..Lfo::default()
        },
    )
    .with_depth(0xFF);
    let heard = rows(&lfo, 64);
    let counted = counted(64);
    assert_ne!(heard[1..15], counted[1..15], "{heard:?}");
    assert_eq!(heard[17..31], counted[17..31], "{heard:?}");
    assert_eq!(heard, rows(&lfo, 64), "a render repeats");
}

#[test]
fn a_macro_turns_jump() {
    let mut turned = song(1, 0);
    turned.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::lane(0, 3).unwrap()),
        min: 0,
        max: 0xFF00,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    }];
    assert_eq!(rows(&turned, 32), counted(32));
    turned.tracks[0].macros[0].value = 0xFF00;
    assert!(jumps(&rows(&turned, 32)) > 16);
}

#[test]
fn cue_never_holds_a_jump_back() {
    // With `Cue` on, `Jump` turned on the lane's second row is heard from its third, not from its
    // next lap, as how a loop is walked never waits (decision 402).
    let mut still = song(1, 0);
    still.tracks[0].cue = 1;
    let mut jumping = still.clone();
    jumping.tracks[0].lanes[0].jump = 0xFF;
    let mut engine = Engine::new(RATE, version(&still));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    let mut heard = Vec::new();
    for step in 0..ROWS {
        if step == 2 {
            engine.swap(version(&jumping));
        }
        engine.process(&mut out);
        heard.push(engine.track_heard(0).lanes[0].unwrap().row);
    }
    assert_eq!(heard[..2], [0, 1]);
    let moved = heard[2..]
        .iter()
        .zip(&counted(ROWS)[2..])
        .filter(|(heard, counted)| heard != counted)
        .count();
    assert!(moved >= 3, "{heard:?}");
    // It rolls as it does with `Cue` off.
    let mut off = jumping.clone();
    off.tracks[0].cue = 0;
    assert_eq!(rows(&jumping, 64), rows(&off, 64));
}
