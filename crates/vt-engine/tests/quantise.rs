//! `QNT`: how far each row's nudge is brought toward the grid as it plays, from the track's
//! `Quantise`, a step, a lane or a mod slot, while the nudges written stay as they are.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Phrase, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 25 per tick.
const STEP: usize = 6000;
const TICK: usize = 25;
/// A nudge's unit, a 24th of a sixteenth, in frames.
const NUDGE: usize = 10 * TICK;

/// A short hit, so each note's start shows.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; 10],
        },
    );
    Arc::new(pool)
}

/// One track playing these rows, with its `Quantise` at this.
fn song(rows: &[&str], quantise: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.quantise = quantise;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// Where notes begin, for this many steps.
fn hits(song: &Song, steps: usize) -> Vec<usize> {
    let version = Arc::new(Published::new(Arc::new(song.clone()), pool()));
    let out: Vec<f32> = render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect();
    let on = |at: usize| out[at].abs() > 0.001;
    (0..out.len())
        .filter(|&at| on(at) && (at == 0 || !on(at - 1)))
        .collect()
}

/// Rows nudged late and early, as far as a nudge goes and by an odd amount.
const NUDGED: [&str; 4] = ["C-3 --", "C-3 -- +B", "C-3 -- -3", "C-3 -- -B"];

#[test]
fn none_plays_the_nudges_as_written_half_halves_them_and_all_plays_them_on_the_grid() {
    assert_eq!(
        hits(&song(&NUDGED, 0x00), 4),
        [
            0,
            STEP + 11 * NUDGE,
            2 * STEP - 3 * NUDGE,
            3 * STEP - 11 * NUDGE
        ]
    );
    // `80`, shown as 50 %, halves each, to the tick: a 24th is 10 ticks.
    assert_eq!(
        hits(&song(&NUDGED, 0x80), 4),
        [
            0,
            STEP + 11 * NUDGE / 2,
            2 * STEP - 3 * NUDGE / 2,
            3 * STEP - 11 * NUDGE / 2
        ]
    );
    assert_eq!(hits(&song(&NUDGED, 0xFF), 4), [0, STEP, 2 * STEP, 3 * STEP]);
}

#[test]
fn the_nudges_written_are_left_as_they_are() {
    let quantised = song(&NUDGED, 0xFF);
    let _ = hits(&quantised, 4);
    assert_eq!(quantised, song(&NUDGED, 0xFF));
    let nudges: Vec<i8> = quantised.tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.nudge)
        .collect();
    assert_eq!(nudges, [0, 11, -3, -11]);
}

#[test]
fn a_rows_own_lock_is_read_before_it_plays_and_ends_with_its_note() {
    // The lock brings its own row to the grid, though the row's FX run only once it lands, and
    // the next row's note plays at the track's own again.
    let rows = ["C-3 --", "C-3 -- -B quantise:FF", "C-3 -- -B"];
    assert_eq!(
        hits(&song(&rows, 0x00), 3),
        [0, STEP, 2 * STEP - 11 * NUDGE]
    );
    // A row of the track's own at `FF` locked back to none.
    let rows = ["C-3 --", "C-3 -- -B quantise:00", "C-3 -- -B"];
    assert_eq!(
        hits(&song(&rows, 0xFF), 3),
        [0, STEP - 11 * NUDGE, 2 * STEP]
    );
}

#[test]
fn a_lanes_value_is_read_at_its_step() {
    // The first column is a lane two steps long, quantising every other step.
    let rows = [
        "C-3 -- quantise:FF",
        "C-3 -- -6 quantise:00",
        "C-3 -- -6",
        "C-3 -- -6",
    ];
    let mut laned = song(&rows, 0x00);
    let phrase = Arc::make_mut(laned.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 2,
        }),
    );
    assert_eq!(
        hits(&laned, 4)[1..],
        [STEP - 6 * NUDGE, 2 * STEP, 3 * STEP - 6 * NUDGE]
    );
}

#[test]
fn a_mod_slot_on_it_moves_how_far_the_nudges_are_brought_in() {
    let rows = ["C-3 --", "C-3 -- -B", "C-3 -- -B", "C-3 -- -B"];
    let mut moved = song(&rows, 0x80);
    moved.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::QUANTISE),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(4),
            ..Lfo::default()
        },
    )
    .with_depth(0x40);
    let heard = hits(&moved, 4);
    let half = 11 * NUDGE / 2;
    // Up for the first two steps, nearer the grid than half, and down for the last two.
    assert!(STEP - heard[1] < half, "{heard:?}");
    assert!(3 * STEP - heard[3] > half, "{heard:?}");
}
