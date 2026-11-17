//! A track's `Jump` (decision 391): the chance each step plays any row of its phrase instead of
//! the one its order walks to, scrambling the order by degrees, as Marbles' Déjà vu knob does past
//! noon. It's read before the row plays, as `QNT` is, so a `JMP` on a row, an LFO or a macro
//! moves it, and it's rolled from the seed, so a render repeats.

use std::sync::Arc;

use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::order::Order;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Chain, ChainEntry, Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;

fn ramp_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// The eight rows of C major from `C-3` up, each a note that starts and stops at once.
const SCALE: [&str; 8] = [
    "C-3 --", "D-3 --", "E-3 --", "F-3 --", "G-3 --", "A-3 --", "B-3 --", "C-4 --",
];

/// What the rows of [`SCALE`] play, in order.
const WALKED: [i32; 8] = [60, 62, 64, 65, 67, 69, 71, 72];

/// One track playing these rows in this order, with its `Jump` at `jump`.
fn song(rows: &[&str], order: Order, jump: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.jump = jump;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    let mut phrase = Phrase::from_steps(steps);
    phrase.set_order(order);
    track.phrases.insert(0, Arc::new(phrase));
    song
}

/// The note each step starts with, by how fast the ramp climbs, for this many steps.
fn notes(song: &Song, steps: usize) -> Vec<i32> {
    let out: Vec<f32> = render_frames(version(song, ramp_pool()), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect();
    let own = 1000.0 / RAMP_FRAMES as f32;
    (0..steps)
        .map(|step| {
            let frame = step * STEP;
            let climb = out[frame + 1600] - out[frame + 600];
            C3 + (12.0 * (climb / own).log2()).round() as i32
        })
        .collect()
}

#[test]
fn at_none_the_order_plays_as_it_is_and_at_full_any_row_each_step() {
    let walked = notes(&song(&SCALE, Order::Forward, 0), 16);
    assert_eq!(walked, WALKED.repeat(2));
    let jumping = song(&SCALE, Order::Forward, 0xFF);
    let jumped = notes(&jumping, 32);
    assert!(
        jumped.iter().all(|note| WALKED.contains(note)),
        "{jumped:?}"
    );
    assert_ne!(jumped, WALKED.repeat(4), "it jumps");
    let mut heard = jumped.clone();
    heard.sort_unstable();
    heard.dedup();
    assert!(heard.len() >= 6, "any row: {jumped:?}");
    assert_eq!(jumped, notes(&jumping, 32), "a render repeats");
    // Halfway, some steps walk on and some jump, and the walk carries on where it was.
    let half = notes(&song(&SCALE, Order::Forward, 0x80), 32);
    let walks = half
        .iter()
        .zip(WALKED.repeat(4))
        .filter(|(heard, walked)| **heard == *walked)
        .count();
    assert!((10..32).contains(&walks), "{half:?}");
}

#[test]
fn it_scrambles_any_order_and_stays_within_the_phrase() {
    // `Up-down` at none walks as it does; at full it plays rows of the phrase only.
    let up_down = notes(&song(&SCALE, Order::UpDown, 0), 14);
    assert_eq!(
        up_down,
        [60, 62, 64, 65, 67, 69, 71, 72, 71, 69, 67, 65, 64, 62]
    );
    // A chain of two phrases: a jump in the first never reaches the second's rows.
    let mut chained = song(&SCALE, Order::Forward, 0xFF);
    let high: Vec<_> = ["C-5 --", "D-5 --", "E-5 --", "F-5 --"]
        .iter()
        .map(|text| text.parse().unwrap())
        .collect();
    let track = &mut chained.tracks[0];
    track.phrases.insert(1, Arc::new(Phrase::from_steps(high)));
    track.chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![
                ChainEntry {
                    phrase: 0,
                    transpose: 0,
                },
                ChainEntry {
                    phrase: 1,
                    transpose: 0,
                },
            ],
            label: String::new(),
        }),
    );
    let heard = notes(&chained, 24);
    for (step, note) in heard.iter().enumerate() {
        if step % 12 < 8 {
            assert!(WALKED.contains(note), "{step}: {heard:?}");
        } else {
            assert!([84, 86, 88, 89].contains(note), "{step}: {heard:?}");
        }
    }
}

#[test]
fn a_jmp_on_a_row_sets_the_chance_for_the_step_that_walks_to_it() {
    // `JMP FF` on the third row: only the steps that walk to it jump.
    let mut rows = SCALE;
    rows[2] = "E-3 -- jump:FF --- ---";
    let heard = notes(&song(&rows, Order::Forward, 0), 64);
    let mut jumped = 0;
    for (step, note) in heard.iter().enumerate() {
        if step % 8 == 2 {
            jumped += usize::from(*note != 64);
        } else {
            assert_eq!(*note, WALKED[step % 8], "{step}: {heard:?}");
        }
    }
    assert!(jumped >= 4, "{heard:?}");
}

#[test]
fn an_lfo_on_jump_scrambles_its_high_half_alone() {
    // A square at full depth: every step jumps in its high half, none in its low.
    let mut lfo = song(&SCALE, Order::Forward, 0);
    lfo.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::JUMP),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0xFF);
    let heard = notes(&lfo, 32);
    let lap = WALKED.repeat(2);
    assert_ne!(heard[1..7], lap[1..7], "{heard:?}");
    assert_eq!(heard[9..15], lap[9..15], "{heard:?}");
    assert_eq!(heard, notes(&lfo, 32), "a render repeats");
}

#[test]
fn a_macro_turns_jump() {
    use vt_model::macros::{MacroMap, MacroTarget, Zone};
    let mut turned = song(&SCALE, Order::Forward, 0);
    turned.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::JUMP),
        min: 0,
        max: 0xFF00,
        curve: 0,
        s: 0,
        zone: Zone::default(),
    }];
    assert_eq!(notes(&turned, 16), WALKED.repeat(2));
    turned.tracks[0].macros[0].value = 0xFF00;
    assert_ne!(notes(&turned, 16), WALKED.repeat(2));
}
