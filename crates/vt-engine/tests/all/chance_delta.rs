//! `CDLT`, `Chance up or down` (decision 405): every row's chance to play moved up or down from
//! where it's written, as the Metropolix's Probability target and Elektron's track `PROB` move
//! them. Above `80` it raises each `CHA`, below it lowers them, and rows with no `CHA` count as
//! 100% and thin too, so it's a density fader. At `80` it changes nothing.

use std::sync::Arc;

use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// How many times round a 16-row phrase the sweeps listen for.
const PASSES: usize = 64;

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

/// One track playing these rows.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// Which steps a note starts on, over this many steps.
fn hits(song: &Song, steps: usize) -> Vec<usize> {
    let out = render_frames(version(song, hits_pool()), RATE, steps * STEP);
    (0..steps)
        .filter(|step| out[step * STEP * 2].abs() > 0.001)
        .collect()
}

/// A busy hat line: every other row always plays, as written, and the rows between have a
/// `CHA 80`, a little over half.
fn hats() -> Vec<String> {
    (0..16)
        .map(|row| match row % 2 {
            0 => "C-3 --".to_owned(),
            _ => "C-3 -- cha:80".to_owned(),
        })
        .collect()
}

/// The hat line with the track's `CDLT` at `value`.
fn hats_at(value: u8) -> Song {
    let rows = hats();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let mut song = song(&rows);
    song.tracks[0].chance_delta = value;
    song
}

/// How many notes are expected over the sweep's passes: the rows with no `CHA` at `own`, and
/// those with one at `cha`.
fn expected(own: f64, cha: f64) -> f64 {
    (8.0 * own + 8.0 * cha) * PASSES as f64
}

fn near(heard: usize, expected: f64) -> bool {
    (heard as f64 - expected).abs() <= expected * 0.12 + 1.0
}

#[test]
fn eighty_changes_nothing() {
    let plain = [
        "C-3 -- cha:60",
        "C-3 --",
        "C-3 -- rnd:93",
        "C-3 -- cnd:03",
        "C-3 -- cha:20",
        "C-3 -- cnd:04",
        "C-3 -- ret:83 rnd:84",
        "--- --",
    ];
    let locked: Vec<String> = plain
        .iter()
        .map(|row| format!("{row} chancedelta:80"))
        .collect();
    let locked: Vec<&str> = locked.iter().map(String::as_str).collect();
    let mut held = song(&plain);
    held.tracks[0].chance_delta = 0x80;
    let steps = 8 * 16;
    let out = render_frames(version(&song(&plain), hits_pool()), RATE, steps * STEP);
    assert_eq!(
        out,
        render_frames(version(&held, hits_pool()), RATE, steps * STEP)
    );
    assert_eq!(
        out,
        render_frames(version(&song(&locked), hits_pool()), RATE, steps * STEP)
    );
}

#[test]
fn a_sweep_thins_and_fills_a_hat_line_in_proportion() {
    let steps = PASSES * 16;
    let cha = 128.0 / 255.0;
    assert!(hits(&hats_at(0x00), steps).is_empty(), "00 plays nothing");
    let half = hits(&hats_at(0x40), steps).len();
    assert!(
        near(half, expected(0.5, cha * 0.5)),
        "40 halves every chance: {half}"
    );
    let as_written = hits(&hats_at(0x80), steps).len();
    assert!(near(as_written, expected(1.0, cha)), "80: {as_written}");
    let side = 64.0 / 127.0;
    let filled = hits(&hats_at(0xC0), steps).len();
    assert!(
        near(filled, expected(1.0, cha + (1.0 - cha) * side)),
        "C0 brings in about half of what's missing: {filled}"
    );
    assert_eq!(
        hits(&hats_at(0xFF), steps).len(),
        steps,
        "FF plays every row"
    );
}

#[test]
fn turning_it_up_only_adds_notes() {
    let steps = 16 * 16;
    let low = hits(&hats_at(0x40), steps);
    let middle = hits(&hats_at(0x80), steps);
    let high = hits(&hats_at(0xC0), steps);
    assert!(low.iter().all(|step| middle.contains(step)));
    assert!(middle.iter().all(|step| high.contains(step)));
    assert!(low.len() < middle.len() && middle.len() < high.len());
}

#[test]
fn rows_without_a_chance_thin_below_the_middle_and_stay_above_it() {
    let plain = ["C-3 --"; 16];
    let steps = 16 * 16;
    let mut thinned = song(&plain);
    thinned.tracks[0].chance_delta = 0x20;
    let heard = hits(&thinned, steps).len();
    assert!(
        near(heard, steps as f64 * 0.25),
        "20 keeps a quarter: {heard}"
    );
    let mut raised = song(&plain);
    raised.tracks[0].chance_delta = 0xC0;
    assert_eq!(hits(&raised, steps).len(), steps);
}

#[test]
fn a_condition_decides_its_row_and_pre_follows_a_chance_row() {
    // The first row's `CHA 80` is moved down and the second follows it with `PRE`; the third
    // has a `1:2`, which `CDLT` leaves to its condition, as Elektron's conditions override
    // `PROB`.
    let mut song = song(&["C-3 -- cha:80", "C-3 -- cnd:03", "C-3 -- cnd:12", "--- --"]);
    song.tracks[0].chance_delta = 0x40;
    let heard = hits(&song, 64 * 4);
    let mut led = 0;
    for pass in 0..64 {
        let at = pass * 4;
        assert_eq!(
            heard.contains(&at),
            heard.contains(&(at + 1)),
            "pass {pass}: PRE follows whether the row before it played"
        );
        led += usize::from(heard.contains(&at));
        assert_eq!(heard.contains(&(at + 2)), pass % 2 == 0, "pass {pass}");
    }
    assert!((4..=28).contains(&led), "a quarter of 64: {led}");
}

#[test]
fn a_lock_sets_it_for_its_own_row() {
    let steps = 4 * 8;
    let locked = song(&[
        "C-3 -- chancedelta:00",
        "C-3 --",
        "C-3 -- cha:00 chancedelta:FF",
        "C-3 --",
    ]);
    let heard = hits(&locked, steps);
    let expected: Vec<usize> = (0..steps).filter(|step| step % 4 != 0).collect();
    assert_eq!(heard, expected);
}

#[test]
fn a_macro_turns_it() {
    let rows = ["C-3 --"; 4];
    let mut turned = song(&rows);
    turned.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::CHANCE_DELTA),
        min: 0x0000,
        max: 0x8000,
        curve: 0,
        s: 0,
        zone: Default::default(),
        member: None,
    }];
    turned.tracks[0].macros[0].value = 0x0000;
    assert!(
        hits(&turned, 16).is_empty(),
        "the macro at 00 holds it at 00"
    );
    turned.tracks[0].macros[0].value = 0xFF00;
    assert_eq!(hits(&turned, 16).len(), 16);
}

#[test]
fn an_lfo_on_it_thins_where_it_dips() {
    let rows = ["C-3 --"; 8];
    let mut moved = song(&rows);
    // A square over 8 steps at full depth: half of each cycle up at `FF` or past it, where every
    // row plays, and half down at `00`, where none does. The rows where it turns are left out.
    moved.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::CHANCE_DELTA),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(8),
            ..Lfo::default()
        },
    )
    .with_depth(0x80);
    let heard = hits(&moved, 8 * 8);
    let count = |rows: [usize; 3]| {
        heard
            .iter()
            .filter(|step| rows.contains(&(*step % 8)))
            .count()
    };
    let (up, down) = (count([1, 2, 3]), count([5, 6, 7]));
    assert!(
        (up, down) == (24, 0) || (up, down) == (0, 24),
        "a whole half of each cycle drops: {heard:?}"
    );
}

#[test]
fn renders_repeat_and_follow_the_seed() {
    let thinned = hats_at(0x50);
    let steps = 16 * 8;
    let once = hits(&thinned, steps);
    assert_eq!(once, hits(&thinned, steps));
    let mut reseeded = thinned;
    reseeded.seed ^= 0x1234_5678;
    assert_ne!(once, hits(&reseeded, steps));
}

#[test]
fn it_moves_an_rnds_chance_too() {
    let steps = 32;
    let plain = render_frames(
        version(&song(&["C-3 --"; 4]), hits_pool()),
        RATE,
        steps * STEP,
    );
    let mut rolled = song(&["C-3 -- rnd:07"; 4]);
    let at = |song: &Song| render_frames(version(song, hits_pool()), RATE, steps * STEP);
    assert_eq!(at(&rolled), plain, "a chance of 0 never varies");
    rolled.tracks[0].chance_delta = 0xFF;
    assert_ne!(at(&rolled), plain, "at FF every RND varies");
}
