//! A track's dice loop, after Marbles' Déjà vu and Length (decisions 410 and 411): with `Dice loop`
//! on, each step's `CHA`s and `RND`s roll the dice of a place in a loop of that many steps;
//! `Dice mutate` is the chance a place rolls afresh as it comes round, `00` locking the loop and
//! `FF` fresh dice every step, and `Dice jump` the chance a step reads another place. Off, the
//! default, plays as before, and the loop is rolled from the seed, so a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Phrase, Song};

use crate::support::ramp::{RAMP_FRAMES, render, version};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;

/// `Dice loop`'s values for loops of four and eight steps.
const FOUR: u8 = 4;
const EIGHT: u8 = 8;

/// One track playing these rows, its notes starting and stopping at once, with its dice loop at
/// `length`, `mutate` and `jump`.
fn song(rows: &[&str], length: u8, mutate: u8, jump: u8) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.seed = 0x5EED;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.dice_loop = length;
    track.dice_mutate = mutate;
    track.dice_jump = jump;
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

/// Whether each step plays, as `x` for a note and `.` for none.
fn hits(song: &Song, steps: usize) -> String {
    heard(song, steps)
        .iter()
        .map(|note| if note.is_some() { 'x' } else { '.' })
        .collect()
}

/// A row whose `CHA` plays it half the time.
const HALF: &str = "C-3 -- cha:80";
/// A row whose `RND` always moves its note, by up to seven semitones either way.
const MOVED: &str = "C-3 -- rnd:F7";

/// Whether `pattern` repeats every `period` of its places.
fn repeats(pattern: &[impl PartialEq], period: usize) -> bool {
    pattern
        .iter()
        .zip(pattern.iter().skip(period))
        .all(|(a, b)| a == b)
}

#[test]
fn off_plays_as_before_whatever_mutate_and_jump_say() {
    let rows = [HALF, MOVED, "D-3 -- cha:40 rnd:F3", "--- --"];
    let before = render(&song(&rows, 0, 0, 0), 32);
    assert_eq!(render(&song(&rows, 0, 0x80, 0xFF), 32), before);
    assert_eq!(render(&song(&rows, 0, 0xFF, 0x40), 32), before);
}

#[test]
fn a_locked_loop_repeats_its_chances_every_length() {
    for (length, steps) in [(FOUR, 4), (EIGHT, 8)] {
        let locked = hits(&song(&[HALF], length, 0, 0), 64);
        let pattern: Vec<char> = locked.chars().collect();
        assert!(repeats(&pattern, steps), "{locked}");
        assert!(locked.contains('x') && locked.contains('.'), "{locked}");
    }
    // Off, the same chances don't repeat.
    let fresh: Vec<char> = hits(&song(&[HALF], 0, 0, 0), 64).chars().collect();
    assert!(!repeats(&fresh, 4) && !repeats(&fresh, 8));
}

#[test]
fn a_locked_loop_repeats_its_variations_every_length() {
    let locked: Vec<Option<i32>> = heard(&song(&[MOVED], FOUR, 0, 0), 32);
    assert!(locked.iter().all(Option::is_some));
    assert!(repeats(&locked, 4), "{locked:?}");
    // The loop holds the first four steps' variations, which are what they are with no loop.
    let fresh = heard(&song(&[MOVED], 0, 0, 0), 32);
    assert_eq!(locked[..4], fresh[..4]);
    assert!(!repeats(&fresh, 4));
}

#[test]
fn full_mutate_rolls_fresh_dice_every_step() {
    // An `RND` rolls just as it does with no loop.
    let rows = [MOVED, "D-3 -- rnd:F3"];
    assert_eq!(
        render(&song(&rows, EIGHT, 0xFF, 0), 32),
        render(&song(&rows, 0, 0, 0), 32)
    );
    // A `CHA` rolls each step afresh, and plays about half the time.
    let fresh = hits(&song(&[HALF], EIGHT, 0xFF, 0), 128);
    let pattern: Vec<char> = fresh.chars().collect();
    assert!(!repeats(&pattern, 8), "{fresh}");
    let played = fresh.chars().filter(|hit| *hit == 'x').count();
    assert!((40..88).contains(&played), "{fresh}");
}

#[test]
fn halfway_mutate_drifts_the_loop() {
    let locked = heard(&song(&[MOVED], EIGHT, 0, 0), 64);
    let drifting = heard(&song(&[MOVED], EIGHT, 0x80, 0), 64);
    let fresh = heard(&song(&[MOVED], 0, 0, 0), 64);
    assert!(!repeats(&drifting, 8));
    let kept = drifting
        .iter()
        .zip(drifting.iter().skip(8))
        .filter(|(a, b)| a == b)
        .count();
    assert!(kept > 8, "some come round again: {drifting:?}");
    assert_ne!(drifting, locked);
    assert_ne!(drifting, fresh);
}

#[test]
fn jumping_plays_the_loops_dice_out_of_order() {
    let rows = [MOVED];
    let locked = heard(&song(&rows, FOUR, 0, 0), 64);
    let jumping = heard(&song(&rows, FOUR, 0, 0xFF), 64);
    assert!(!repeats(&jumping, 4), "it reorders: {jumping:?}");
    // Locked, a step only ever takes the dice of one of the loop's four places.
    let mut held: Vec<_> = jumping[16..].to_vec();
    held.sort_unstable();
    held.dedup();
    assert!(held.len() <= 4, "{jumping:?}");
    assert_ne!(jumping, locked);
    // Halfway, some steps walk the loop and some jump.
    let half = heard(&song(&rows, FOUR, 0, 0x80), 64);
    assert_ne!(half, locked);
    assert_ne!(half, jumping);
}

#[test]
fn renders_repeat_and_a_new_seed_rolls_another_loop() {
    let rows = [HALF, MOVED, "E-3 -- cha:C0"];
    for (mutate, jump) in [(0, 0), (0x60, 0), (0, 0x80), (0x40, 0x40)] {
        let looped = song(&rows, EIGHT, mutate, jump);
        assert_eq!(render(&looped, 48), render(&looped, 48));
    }
    let mut reseeded = song(&[MOVED], EIGHT, 0, 0);
    let first = heard(&reseeded, 16);
    reseeded.seed = 0xBEEF;
    assert_ne!(heard(&reseeded, 16), first);
}

#[test]
fn playing_again_fills_the_loop_again_and_notes_dont_restart_it() {
    // From the top again, the loop starts empty and fills with the same dice, so it plays the
    // same: it's the track's, kept across its notes, its phrases and its chain, and cleared only
    // when playing starts.
    let looped = song(&[MOVED, HALF, "--- --", "G-3 -- rnd:F2"], EIGHT, 0, 0);
    let mut engine = Engine::new(RATE, version(&looped));
    let mut play = || {
        let mut out = vec![0.0; 24 * STEP * 2];
        engine.play_from(0);
        engine.process(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
        (0..24)
            .map(|step| note_at(&left, step * STEP))
            .collect::<Vec<_>>()
    };
    let first = play();
    // The first note of the second play starts under the last one's fade, so it's left out.
    let again = play();
    assert_eq!(first[1..], again[1..]);
    // A phrase of four played round keeps the loop of eight going over its passes.
    assert!(repeats(&first, 8), "{first:?}");
    assert!(!repeats(&first, 4), "{first:?}");
}

#[test]
fn a_lock_a_lane_an_lfo_and_a_macro_reach_it() {
    // A `DLP` lock on a phrase's one row holds the loop on.
    let locked = song(&["C-3 -- rnd:F7 diceloop:04"], 0, 0, 0);
    let notes = heard(&locked, 32);
    assert!(repeats(&notes, 4), "{notes:?}");
    // On the first of four rows alone, only the steps that play it loop: each takes the dice of
    // the place it's at, which the first time round it filled.
    let rows = ["C-3 -- rnd:F7 diceloop:04", MOVED, MOVED, MOVED];
    let first = heard(&song(&rows, 0, 0, 0), 32);
    let fresh = heard(&song(&[MOVED], 0, 0, 0), 32);
    assert!(
        (0..32).step_by(4).all(|step| first[step] == first[0]),
        "{first:?}"
    );
    assert_eq!(first[1..4], fresh[1..4]);
    // In a lane of that one row, every step reads it, so every step loops.
    let mut laned = song(&rows, 0, 0, 0);
    let phrase = Arc::make_mut(laned.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(1, Some(Lane::new(0, 1)));
    let lane_notes = heard(&laned, 32);
    assert!(repeats(&lane_notes, 4), "{lane_notes:?}");
    // A square LFO on `Dice mutate` over a loop of four: fresh dice in its high half, then the
    // last four it rolled, locked, in its low half.
    let mut lfo = song(&[MOVED], FOUR, 0, 0);
    lfo.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::DICE_MUTATE),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0xFF);
    let moved = heard(&lfo, 16);
    // The square turns at the start of the ninth step, which still rolls afresh.
    assert_eq!(moved[..9], fresh[..9], "{moved:?}");
    assert_eq!(moved[9..12], fresh[5..8], "{moved:?}");
    assert_eq!(moved[12..16], moved[8..12], "{moved:?}");
    // A macro holding `Dice loop` at four plays as `Dice loop` at four does.
    use vt_model::macros::{MacroMap, MacroTarget, Zone};
    let mut turned = song(&[MOVED], 0, 0, 0);
    turned.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::DICE_LOOP),
        min: 0,
        max: u16::from(FOUR) << 8,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    }];
    assert_eq!(heard(&turned, 32), fresh, "at none it's off");
    turned.tracks[0].macros[0].value = 0xFFFF;
    assert_eq!(heard(&turned, 32), heard(&song(&[MOVED], FOUR, 0, 0), 32));
}

#[test]
fn a_locked_loop_holds_what_chance_up_or_down_thins_and_what_it_moves() {
    // `CDLT` below `80` thins rows with no `CHA` by the loop's dice too.
    let mut thinned = song(&["C-3 --"], EIGHT, 0, 0);
    thinned.tracks[0].chance_delta = 0x40;
    let pattern: Vec<char> = hits(&thinned, 64).chars().collect();
    assert!(repeats(&pattern, 8), "{pattern:?}");
    assert!(pattern.contains(&'x') && pattern.contains(&'.'));
    // And a looped `CHA` is moved by it, as a fresh one is.
    let mut raised = song(&[HALF], EIGHT, 0, 0);
    raised.tracks[0].chance_delta = 0xFF;
    assert_eq!(hits(&raised, 16), "x".repeat(16));
}
