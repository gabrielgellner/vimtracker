//! A track's `Swing` (`SWG`): every other step later, on top of its groove, from 50.0% to 75.0% a
//! tenth of a percent a step, read once a pair of steps, from the track, a lock, a lane, a macro
//! or a mod slot (decision 420).

use std::sync::Arc;

use vt_model::groove::{GrooveId, MOST_SWING, SWING_PERCENTS, swing_ticks, swing_value};
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

use crate::support::panned::{render, song};

/// 6000 frames per step at 120 BPM, and 25 per tick.
const STEP: usize = 6000;
const TICK: usize = 25;

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

/// A long sample that changes every frame, so where a note starts and ends and how loud it is
/// all show in the render.
fn ramp_pool() -> Arc<SamplePool> {
    let frames = (0..48_000 * 2)
        .map(|frame| {
            let value = ((frame % 977) as f32 / 977.0) - 0.5;
            [value, -value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// Where notes begin, for this many steps.
fn hits(song: &Song, steps: usize) -> Vec<usize> {
    let out = render(song, hits_pool(), steps, 0);
    let on = |at: usize| out[at].abs() > 0.001;
    (0..out.len())
        .filter(|&at| on(at) && (at == 0 || !on(at - 1)))
        .collect()
}

/// A track playing a note a step, with its `Swing` at this.
fn swung(rows: &[&str], swing: u8) -> Song {
    let mut song = song(&[rows]);
    song.tracks[0].swing = swing;
    song
}

/// Eight notes on a track whose gate is half, so a swung pair's lengths show too, then a velocity
/// and a nudge that the swing goes under.
const PLAYED: [&str; 8] = [
    "C-3 --",
    "D-3 --",
    "C-3 40",
    "E-3 --",
    "C-3 --",
    "C-3 -- +4",
    "G-3 7F",
    "C-3 -- -2",
];

#[test]
fn every_swing_groove_renders_bit_for_bit_as_its_swing() {
    for percent in SWING_PERCENTS {
        let mut grooved = song(&[&PLAYED]);
        grooved.tracks[0].gate_length = Some(0x80);
        grooved.groove = GrooveId::swing(percent).unwrap();
        let mut own = song(&[&PLAYED]);
        own.tracks[0].gate_length = Some(0x80);
        own.tracks[0].swing = swing_value(percent);
        for channel in 0..2 {
            assert_eq!(
                render(&grooved, ramp_pool(), 9, channel),
                render(&own, ramp_pool(), 9, channel),
                "Swing {percent} and SWG {:02X}",
                swing_value(percent),
            );
        }
    }
}

#[test]
fn a_swing_between_the_grooves_places_the_off_beat_in_proportion() {
    let rows = ["C-3 --"; 4];
    assert_eq!(
        hits(&swung(&rows, 0x00), 4),
        [0, STEP, 2 * STEP, 3 * STEP],
        "50.0 % is straight"
    );
    // 57.0 %: the second of a pair 57 % of the pair's way along, 274 of its 480 ticks.
    assert_eq!(swing_ticks(0x46), 34);
    assert_eq!(
        hits(&swung(&rows, 0x46), 4),
        [0, STEP + 34 * TICK, 2 * STEP, 3 * STEP + 34 * TICK]
    );
    // Every tenth further on is never earlier, and the top is 75.0 %, half a step late.
    let mut last = 0;
    for value in 0..=MOST_SWING {
        let ticks = swing_ticks(value);
        let percent = 50.0 + f64::from(value) / 10.0;
        let exact = 480.0 * percent / 100.0 - 240.0;
        assert!((f64::from(ticks) - exact).abs() <= 0.5, "{value:02X}");
        assert!(ticks >= last, "{value:02X}");
        last = ticks;
    }
    assert_eq!(swing_ticks(MOST_SWING), 120);
    assert_eq!(swing_ticks(0xFF), 120, "past the top stays there");
    let offbeat = |value: u8| hits(&swung(&rows, value), 2)[1];
    assert_eq!(offbeat(0xFA), STEP + 120 * TICK);
    assert!(offbeat(0x46) < offbeat(0x48) && offbeat(0x48) < offbeat(0x50));
}

#[test]
fn it_goes_on_top_of_the_groove() {
    let rows = ["C-3 --"; 4];
    let mut both = swung(&rows, 0x46);
    both.groove = GrooveId::swing(60).unwrap();
    // `Swing 60`'s 48 ticks and 57.0 %'s 34.
    assert_eq!(
        hits(&both, 4),
        [0, STEP + 82 * TICK, 2 * STEP, 3 * STEP + 82 * TICK]
    );
    // No further than half a step, however much the two add up to.
    let mut most = swung(&rows, MOST_SWING);
    most.groove = GrooveId::swing(70).unwrap();
    assert_eq!(hits(&most, 2), [0, STEP + 120 * TICK]);
}

#[test]
fn a_lock_on_a_pairs_first_row_swings_the_pair() {
    // The lock sets the first pair; the next pair has the track's own, straight.
    let rows = ["C-3 -- swing:A0", "C-3 --", "C-3 --", "C-3 --"];
    assert_eq!(
        hits(&swung(&rows, 0x00), 4),
        [0, STEP + 77 * TICK, 2 * STEP, 3 * STEP]
    );
    // A lock on the second row of a pair comes too late for it: the pair's swing was read as
    // it started. Held, it swings the pairs after.
    let rows = ["C-3 --", "C-3 -- swing:A0h", "C-3 --", "C-3 --"];
    assert_eq!(
        hits(&swung(&rows, 0x00), 4),
        [0, STEP, 2 * STEP, 3 * STEP + 77 * TICK]
    );
}

#[test]
fn an_lfo_on_it_moves_the_swing_and_each_pair_keeps_what_it_read() {
    let rows = ["C-3 --"; 8];
    // A square over four steps, a quarter of the way round as it starts, so it's up through the
    // first pair's first step and down through the second's.
    let mut moved = swung(&rows, 0x80);
    moved.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::SWING),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(4),
            phase: 90,
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    let heard = hits(&moved, 8);
    let own = swing_ticks(0x80) as usize * TICK;
    assert!(heard[1] - STEP > own, "{heard:?}");
    assert!(heard[3] - 3 * STEP < own, "{heard:?}");
    // A square over two steps, started the same way, is up on each pair's first step and down on
    // its second. The pair reads it on its first, so every off-beat is swung as far as it was up,
    // where reading it on the off-beat would have put them all on the grid.
    let mut quick = swung(&rows, 0x00);
    quick.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::SWING),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(2),
            phase: 90,
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    let heard = hits(&quick, 8);
    for pair in 0..4 {
        assert!(heard[2 * pair + 1] > (2 * pair + 1) * STEP, "{heard:?}");
        assert_eq!(heard[2 * pair + 1], heard[1] + 2 * pair * STEP, "{heard:?}");
    }
}

#[test]
fn a_swung_render_repeats() {
    let mut moved = swung(&PLAYED, 0x5A);
    moved.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::SWING),
        Lfo {
            shape: Shape::Sine,
            rate: Rate::Steps(3),
            ..Lfo::default()
        },
    )
    .with_depth(0x50);
    let first = render(&moved, ramp_pool(), 9, 0);
    assert_eq!(first, render(&moved, ramp_pool(), 9, 0));
}
