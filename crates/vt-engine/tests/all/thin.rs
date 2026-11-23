//! A track's `Thin` (`THIN`): where each row's draw against its chance comes from, as `CDLT` and
//! `CHA` thin it (decision 479). `Random` rolls it, as ever; `Chance` gives every row the same, so
//! rows go by their chances, the Cirklon's mask; `Beats` by where the row falls in the bar; and
//! `Spread` by its place among the notes, so those left stay evenly spaced.

use std::sync::Arc;

use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::support::panned::{render, song};

/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

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

/// The steps a note starts on, over so many steps.
fn played(song: &Song, steps: usize) -> Vec<usize> {
    let out = render(song, hits_pool(), steps, 0);
    (0..steps)
        .filter(|step| {
            out[step * STEP..step * STEP + 20]
                .iter()
                .any(|v| v.abs() > 0.001)
        })
        .collect()
}

/// A track of these rows, thinned this way at this `CDLT`.
fn thinned(rows: &[&str], thin: u8, delta: u8) -> Song {
    let mut song = song(&[rows]);
    song.tracks[0].thin = thin;
    song.tracks[0].chance_delta = delta;
    song
}

const EVERY_ROW: [&str; 16] = ["C-3 --"; 16];

#[test]
fn random_thins_as_it_always_has() {
    // `Random` is where a track starts, and the goldens hold its renders.
    assert_eq!(Song::new(1).tracks[0].thin, 0);
    let random = played(&thinned(&EVERY_ROW, 0, 0x40), 32);
    assert!(random.len() > 4 && random.len() < 28, "{random:?}");
}

#[test]
fn chance_keeps_the_rows_past_half_and_cdlt_moves_where_that_falls() {
    let rows = [
        "C-3 -- cha:B3", // 70%
        "C-3 -- cha:4D", // 30%
        "C-3 --",        // 100%
        "C-3 -- cha:E6", // 90%
    ];
    // At `80`: the 70%, the 100% and the 90% rows, every pass the same.
    assert_eq!(played(&thinned(&rows, 1, 0x80), 8), [0, 2, 3, 4, 6, 7]);
    // Down at `60`, three quarters of each: the 70% row is at 52%, still past half.
    assert_eq!(played(&thinned(&rows, 1, 0x60), 8), [0, 2, 3, 4, 6, 7]);
    // At `48`, the 70% row goes; the 90% and the 100% stay.
    assert_eq!(played(&thinned(&rows, 1, 0x48), 8), [2, 3, 6, 7]);
    // Up at `C0`, the 30% row comes in, at 65%.
    assert_eq!(
        played(&thinned(&rows, 1, 0xC0), 8),
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
    // Rows with no `CHA` all go together, at `40` and below.
    assert_eq!(played(&thinned(&EVERY_ROW, 1, 0x42), 16).len(), 16);
    assert!(played(&thinned(&EVERY_ROW, 1, 0x40), 16).is_empty());
}

#[test]
fn beats_takes_the_sixteenths_first_and_the_bars_first_row_last() {
    let at = |delta| played(&thinned(&EVERY_ROW, 2, delta), 16);
    assert_eq!(at(0x80).len(), 16);
    assert_eq!(at(0x60), [0, 2, 4, 6, 8, 10, 12, 14], "the eighths");
    assert_eq!(at(0x40), [0, 4, 8, 12], "the beats");
    assert_eq!(at(0x20), [0], "the bar's first row");
    assert!(at(0x00).is_empty());
    // The same every pass.
    assert_eq!(
        played(&thinned(&EVERY_ROW, 2, 0x40), 32),
        [0, 4, 8, 12, 16, 20, 24, 28]
    );
}

#[test]
fn spread_keeps_the_notes_left_evenly_spaced() {
    // Five notes, on rows 0, 3, 6, 10 and 12 of 16.
    let rows: Vec<&str> = (0..16)
        .map(|row| match row {
            0 | 3 | 6 | 10 | 12 => "C-3 --",
            _ => "--- --",
        })
        .collect();
    let at = |delta| played(&thinned(&rows, 3, delta), 16);
    assert_eq!(at(0x80), [0, 3, 6, 10, 12]);
    // At half, the first, third and fifth notes: every other one.
    assert_eq!(at(0x40), [0, 6, 12]);
    // At a quarter, the first and the fifth.
    assert_eq!(at(0x20), [0, 12]);
}

#[test]
fn a_lock_on_a_row_thins_it_its_own_way() {
    // The track thins by `Beats`, which takes every sixteenth at `60`; the lock gives row 1
    // `Chance`, where a row with no `CHA` stays until `40`.
    let mut rows = EVERY_ROW;
    rows[1] = "C-3 -- thin:01";
    let heard = played(&thinned(&rows, 2, 0x60), 16);
    assert_eq!(heard, [0, 1, 2, 4, 6, 8, 10, 12, 14]);
}
