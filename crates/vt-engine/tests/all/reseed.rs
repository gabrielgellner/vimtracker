//! `SED`, the M8's reseed (decision 421): from its row on, a track's dice roll from a seed its
//! value gives, counted from the row, so a random passage after it plays the same each time the
//! row plays. Its `CHA`s, `RND`s, `%`s, its `Jump`, its lanes' jumps and its dice loop all follow
//! it, and different values roll differently.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::lane::Lane;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;

use crate::support::ramp::{RAMP_FRAMES, render};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;

/// One track playing these rows, its notes starting and stopping at once.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.seed = 0x5EED;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// Below this a little into a step, a note started there.
const STARTED: f32 = 0.005;

/// The note that starts at a frame, by how fast the ramp climbs, or none if none starts there.
fn note_at(out: &[f32], frame: usize) -> Option<i32> {
    let climb = out[frame + 1600] - out[frame + 600];
    let own = 1000.0 / RAMP_FRAMES as f32;
    let started = out[frame + 500] < STARTED && climb > own * 0.1;
    started.then(|| C3 + (12.0 * (climb / own).log2()).round() as i32)
}

/// What each step starts, for this many steps.
fn heard(song: &Song, steps: usize) -> Vec<Option<i32>> {
    let out = render(song, steps);
    (0..steps).map(|step| note_at(&out, step * STEP)).collect()
}

/// Whether `pattern` repeats every `period` of its places.
fn repeats(pattern: &[impl PartialEq], period: usize) -> bool {
    pattern
        .iter()
        .zip(pattern.iter().skip(period))
        .all(|(a, b)| a == b)
}

/// Eight rows of chances and variations, the first with `first` in its last FX column.
fn passage(first: &str) -> Vec<String> {
    let mut rows = vec![format!("C-3 -- cha:80 rnd:F7 {first}")];
    rows.extend((1..8).map(|row| match row % 3 {
        0 => "D-3 -- cha:80 ---".to_owned(),
        1 => "C-3 -- rnd:F7 ---".to_owned(),
        _ => "E-3 -- cnd:E8 rnd:F3".to_owned(),
    }));
    rows
}

fn rows(rows: &[String]) -> Vec<&str> {
    rows.iter().map(String::as_str).collect()
}

#[test]
fn after_sed_a_passage_plays_the_same_every_pass() {
    let reseeded = heard(&song(&rows(&passage("sed:2A"))), 64);
    assert!(repeats(&reseeded, 8), "{reseeded:?}");
    assert!(reseeded.iter().any(Option::is_none) && reseeded.iter().any(Option::is_some));
    // Without it, the same rows vary from pass to pass.
    let fresh = heard(&song(&rows(&passage("---"))), 64);
    assert!(!repeats(&fresh, 8), "{fresh:?}");
}

#[test]
fn different_values_roll_differently_and_the_songs_seed_doesnt_matter() {
    let first = song(&rows(&passage("sed:2A")));
    let other = song(&rows(&passage("sed:2B")));
    assert_ne!(heard(&first, 16), heard(&other, 16));
    // The seed is the value's alone, so the passage is the same in a song of any seed.
    let mut moved = first.clone();
    moved.seed = 0xBEEF;
    assert_eq!(heard(&moved, 16), heard(&first, 16));
}

#[test]
fn renders_repeat() {
    let reseeded = song(&rows(&passage("sed:2A")));
    assert_eq!(render(&reseeded, 48), render(&reseeded, 48));
}

#[test]
fn a_sed_halfway_repeats_only_what_follows_it() {
    // Sixteen rows with an `SED` on the ninth: the second half plays the same every pass, while
    // the first half varies as it did.
    let mut both = passage("---");
    both.extend(passage("sed:2A"));
    let notes = heard(&song(&rows(&both)), 64);
    let halves = |half: usize| -> Vec<_> {
        (0..4)
            .map(|pass| notes[pass * 16 + half * 8..pass * 16 + half * 8 + 8].to_vec())
            .collect()
    };
    let second = halves(1);
    assert!(second.iter().all(|pass| *pass == second[0]), "{notes:?}");
    let first = halves(0);
    assert!(first.iter().any(|pass| *pass != first[0]), "{notes:?}");
}

#[test]
fn a_jump_follows_it() {
    let notes = ["C-3 --", "D-3 --", "E-3 --", "F-3 --"];
    let jumping = |first: &str| {
        let mut rows: Vec<String> = notes.iter().map(|note| format!("{note} ---")).collect();
        rows[0] = format!("C-3 -- {first}");
        let mut song = song(&rows.iter().map(String::as_str).collect::<Vec<_>>());
        song.tracks[0].jump = 0x80;
        heard(&song, 64)
    };
    let reseeded = jumping("sed:07");
    assert!(repeats(&reseeded, 4), "{reseeded:?}");
    assert_ne!(reseeded[..4], [Some(60), Some(62), Some(64), Some(65)]);
    let fresh = jumping("---");
    assert!(!repeats(&fresh, 4), "{fresh:?}");
}

#[test]
fn a_locked_dice_loop_drifting_comes_round_the_same_after_it() {
    // A loop of four drifting at half: with an `SED` at the top of the phrase of eight, the loop
    // empties and fills again from the same dice each pass.
    let looped = |first: &str| {
        let mut song = song(&rows(&passage(first)));
        song.tracks[0].dice_loop = 4;
        song.tracks[0].dice_mutate = 0x80;
        song.tracks[0].dice_jump = 0x40;
        heard(&song, 64)
    };
    let reseeded = looped("sed:2A");
    assert!(repeats(&reseeded, 8), "{reseeded:?}");
    assert!(!repeats(&looped("---"), 8));
}

/// The stored row FX 1's lane reads at each of this many steps, on a phrase of 16 rows whose lane
/// is its first four, with its `Jump` at half, and whose first row has `first` in FX 2.
fn lane_rows(first: &str, steps: usize) -> Vec<usize> {
    let mut phrase = Phrase::new(16);
    for row in 0..4 {
        let text = format!("--- -- sna:{:02X}h --- ---", (row + 1) * 0x10);
        *phrase.step_mut(row).unwrap() = text.parse::<Step>().unwrap();
    }
    let text = format!("--- -- sna:10h {first} ---");
    *phrase.step_mut(0).unwrap() = text.parse::<Step>().unwrap();
    phrase.set_lane(0, Some(Lane::new(0, 4)));
    let mut song = Song::new(1);
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song.tracks[0].lanes[0].jump = 0x80;
    let mut engine = Engine::new(RATE, crate::support::no_samples::version(&song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    (0..steps)
        .map(|_| {
            engine.process(&mut out);
            engine.track_heard(0).lanes[0].unwrap().row
        })
        .collect()
}

#[test]
fn a_lanes_jumps_follow_it_from_its_own_step() {
    let reseeded = lane_rows("sed:2A", 64);
    assert!(repeats(&reseeded, 16), "{reseeded:?}");
    assert!(
        reseeded[..16]
            .iter()
            .zip((0..4).cycle())
            .any(|(row, counted)| *row != counted),
        "it jumps: {reseeded:?}"
    );
    let fresh = lane_rows("---", 64);
    assert!(!repeats(&fresh, 16), "{fresh:?}");
}
