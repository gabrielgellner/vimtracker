//! `Chance target` (decision 406): whether a row's chance, as `CHA` writes it and `CDLT` moves
//! it, decides the whole row, `Row`, or each of its pulses and ratchet hits, `Pulse`, as the
//! Metropolix's probability targets a stage or a pulse.

use std::sync::Arc;

use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A sixth of a step, `RET`'s unit.
const UNIT: usize = STEP / 6;

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

/// One track playing these rows, its chances moved by `delta` and decided by `target`.
fn song(rows: &[&str], delta: u8, target: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.chance_delta = delta;
    track.chance_target = target;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn render(song: &Song, steps: usize) -> Vec<f32> {
    render_frames(version(song, hits_pool()), RATE, steps * STEP)
}

/// Whether a note starts at each of these frames, in the left channel.
fn struck(out: &[f32], frames: impl Iterator<Item = usize>) -> Vec<bool> {
    frames.map(|frame| out[frame * 2].abs() > 0.001).collect()
}

/// Of each run of `size` places, how many have every one struck, how many some and how many
/// none.
fn runs(struck: &[bool], size: usize) -> (usize, usize, usize) {
    let mut counts = (0, 0, 0);
    for run in struck.chunks(size) {
        match run.iter().filter(|hit| **hit).count() {
            all if all == size => counts.0 += 1,
            0 => counts.2 += 1,
            _ => counts.1 += 1,
        }
    }
    counts
}

#[test]
fn row_is_the_default_and_changes_nothing() {
    let rows = [
        "C-3 -- MLT4 cha:90",
        "C-3 -- ret:02",
        "C-3 -- MLT2 cha:60 ret:83",
        "C-3 --",
    ];
    let locked: Vec<String> = rows
        .iter()
        .map(|row| format!("{row} chancetarget:00"))
        .collect();
    let locked: Vec<&str> = locked.iter().map(String::as_str).collect();
    for delta in [0x40, 0x80, 0xC0] {
        let plain = render(&song(&rows, delta, 0), 8 * 9);
        assert_eq!(
            plain,
            render(&song(&locked, delta, 0), 8 * 9),
            "{delta:02X}"
        );
    }
}

#[test]
fn on_rows_a_multis_pulses_go_together_and_on_pulses_each_rolls() {
    let rows = ["C-3 -- MLT4"];
    let steps = 4 * 64;
    let at = |target| {
        let out = render(&song(&rows, 0x40, target), steps);
        struck(&out, (0..steps).map(|step| step * STEP))
    };
    let (all, some, none) = runs(&at(0), 4);
    assert_eq!(some, 0, "a row plays whole or not at all");
    assert!(
        (20..=44).contains(&all) && all + none == 64,
        "about half: {all}"
    );
    let pulses = at(1);
    let (_, some, _) = runs(&pulses, 4);
    assert!(some > 16, "rows partly heard: {some}");
    let heard = pulses.iter().filter(|hit| **hit).count();
    assert!(
        (100..=156).contains(&heard),
        "about half the pulses: {heard}"
    );
}

#[test]
fn on_pulses_a_cha_decides_each_pulse_even_at_eighty() {
    let rows = ["C-3 -- MLT4 cha:80"];
    let steps = 4 * 64;
    let out = render(&song(&rows, 0x80, 1), steps);
    let pulses = struck(&out, (0..steps).map(|step| step * STEP));
    let (_, some, _) = runs(&pulses, 4);
    assert!(some > 16, "rows partly heard: {some}");
    let out = render(&song(&rows, 0x80, 0), steps);
    let (_, some, _) = runs(&struck(&out, (0..steps).map(|step| step * STEP)), 4);
    assert_eq!(some, 0);
}

#[test]
fn on_pulses_a_ratchets_hits_thin_one_by_one() {
    // `RET 02` strikes three times a step: as the note starts and two units apart after.
    let rows = ["C-3 -- ret:02"];
    let steps = 96;
    let hits = |target| {
        let out = render(&song(&rows, 0x40, target), steps);
        struck(
            &out,
            (0..steps).flat_map(|step| (0..3).map(move |hit| step * STEP + hit * 2 * UNIT)),
        )
    };
    // Where the row's note plays, whether each later hit does too.
    let after_a_note = |struck: &[bool]| -> Vec<bool> {
        struck
            .chunks(3)
            .filter(|run| run[0])
            .flat_map(|run| run[1..].to_vec())
            .collect()
    };
    let rows_heard = after_a_note(&hits(0));
    assert!(
        rows_heard.iter().all(|hit| *hit),
        "on rows a ratchet goes with its note"
    );
    assert!(
        (24..=72).contains(&(rows_heard.len() / 2)),
        "about half the rows"
    );
    let pulses_heard = after_a_note(&hits(1));
    let dropped = pulses_heard.iter().filter(|hit| !**hit).count();
    assert!(
        dropped * 4 > pulses_heard.len() && dropped * 4 < pulses_heard.len() * 3,
        "on pulses about half the hits after a note drop: {dropped} of {}",
        pulses_heard.len()
    );
}

#[test]
fn a_lock_sets_it_for_its_row_and_renders_repeat() {
    let rows = ["C-3 -- MLT4 chancetarget:01"];
    let steps = 4 * 32;
    let song = song(&rows, 0x40, 0);
    let out = render(&song, steps);
    assert_eq!(out, render(&song, steps));
    let (_, some, _) = runs(&struck(&out, (0..steps).map(|step| step * STEP)), 4);
    assert!(some > 6, "rows partly heard: {some}");
}
