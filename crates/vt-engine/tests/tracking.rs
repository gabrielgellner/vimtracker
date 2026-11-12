//! A Track mod slot (#428): the note as a source, its pitch, its velocity or a roll, mapped from
//! `Low` to `High` onto the slot's depth and held from each note to the next, as #428 and the mod
//! slots study's §2 ask: key tracking around a note, velocity opening the filter, and a value
//! rolled a note that a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::modulation::{ModSlot, Polarity};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::tracking::{Bounds, Source, Tracking};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 40],
        },
    );
    Arc::new(pool)
}

/// A track held at `CUT 80` playing these steps, with a Track slot in `Mod 3` on `Cut`.
fn song(texts: &[&str], slot: ModSlot) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    let phrase: &mut Phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    // An empty text is a step left empty.
    for (index, text) in texts
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song.tracks[0].cutoff = 0x80;
    song.tracks[0].mods[2] = slot;
    song
}

/// A Track slot on `Cut` following a source, mapped from `low` to `high`, at a depth.
fn tracking(source: Source, low: u8, high: u8, depth: i16) -> ModSlot {
    let mut tracking = Tracking {
        source,
        ..Tracking::default()
    };
    *tracking.bounds_mut() = Bounds { low, high };
    ModSlot::tracking(Some(SettingId::CUT), tracking).with_depth(depth)
}

/// The cutoff after each of some stretches of frames from the top.
fn cutoffs(song: &Song, every: usize, times: usize) -> Vec<u8> {
    let version = Arc::new(Published::new(Arc::new(song.clone()), flat()));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(0);
    let mut out = vec![0.0; every * 2];
    (0..times)
        .map(|_| {
            engine.process(&mut out);
            engine.live(0).cutoff
        })
        .collect()
}

#[test]
fn velocity_opens_the_filter_from_low_to_high_and_holds_to_the_next_note() {
    // Half way from `00` to `40` is half the depth, and past `40` all of it.
    let slot = tracking(Source::Velocity, 0, 0x40, 0x40);
    let notes = ["C-3 00", "C-3 20", "C-3 7F", "", "C-3 10"];
    let cut = cutoffs(&song(&notes, slot), STEP, 5);
    assert_eq!(cut, [0x80, 0xA0, 0xC0, 0xC0, 0x90]);
}

#[test]
fn before_any_note_it_moves_nothing() {
    let slot = tracking(Source::Velocity, 0, 0x7F, 0x40);
    let cut = cutoffs(&song(&["", "", "C-3 7F"], slot), STEP, 3);
    assert_eq!(cut, [0x80, 0x80, 0xC0]);
}

#[test]
fn the_note_tracks_both_ways_around_middle_c_and_low_above_high_turns_it_round() {
    // `C-1` to `C-5` both ways is centred on `C-3`, a sixth of the depth an octave.
    let mut slot = tracking(Source::Note, 36, 84, 0x30);
    slot.polarity = Some(Polarity::Bi);
    let notes = ["C-1 7F", "C-3 7F", "C-5 7F", "C-4 7F", "C-0 7F", "G-8 7F"];
    let cut = cutoffs(&song(&notes, slot), STEP, 6);
    assert_eq!(cut, [0x50, 0x80, 0xB0, 0x98, 0x50, 0xB0]);
    // Turned round, the low notes open it.
    let turned = tracking(Source::Note, 84, 36, 0x30);
    let cut = cutoffs(&song(&notes, turned), STEP, 6);
    assert_eq!(cut, [0xB0, 0x98, 0x80, 0x8C, 0xB0, 0x80]);
    // A negative depth closes it instead.
    let closing = tracking(Source::Note, 36, 84, -0x30);
    let cut = cutoffs(&song(&notes, closing), STEP, 3);
    assert_eq!(cut, [0x80, 0x68, 0x50]);
}

#[test]
fn a_new_note_is_followed_from_its_first_frame() {
    // The step's modulation is worked out before its note is, so it's worked out again as the
    // note starts, rather than a block of frames later.
    let slot = tracking(Source::Velocity, 0, 0x7F, 0x7F);
    let song = song(&["C-3 00", "C-3 7F"], slot);
    let version = Arc::new(Published::new(Arc::new(song), flat()));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(0);
    let mut out = vec![0.0; (STEP + 1) * 2];
    engine.process(&mut out);
    assert_eq!(engine.live(0).cutoff, 0xFF);
}

#[test]
fn a_roll_a_note_repeats_render_to_render() {
    let slot = tracking(Source::Random, 0, 0xFF, 0x7F);
    let notes = ["C-3 7F"; 8];
    let first = cutoffs(&song(&notes, slot), STEP, 8);
    assert_eq!(first, cutoffs(&song(&notes, slot), STEP, 8));
    let mut seen = first.clone();
    seen.sort_unstable();
    seen.dedup();
    assert!(seen.len() >= 6, "a new roll each note: {first:02X?}");
    assert!(first.iter().all(|cut| (0x80..=0xFF).contains(cut)));
    // Another project's seed rolls others.
    let mut other = song(&notes, slot);
    other.seed ^= 1;
    assert_ne!(cutoffs(&other, STEP, 8), first);
}

#[test]
fn a_track_slot_leaves_the_tracks_other_rolls_as_they_were() {
    // Each step's `CHA` plays it or not by the track's own rolls, which a roll a note doesn't take
    // from: the velocity followed in `Mod 3` shows which played.
    let notes: Vec<String> = (1..=8)
        .map(|at| format!("C-3 {:02X} cha:80", at * 0x0F))
        .collect();
    let notes: Vec<&str> = notes.iter().map(String::as_str).collect();
    let plain = song(&notes, tracking(Source::Velocity, 0, 0x7F, 0x7F));
    let played = cutoffs(&plain, STEP, 8);
    let mut held = played.clone();
    held.dedup();
    assert!(held.len() < 8 && held.len() > 2, "{played:02X?}");
    let mut rolling = plain.clone();
    rolling.tracks[0].mods[3] = ModSlot::tracking(
        Some(SettingId::RES),
        Tracking {
            source: Source::Random,
            ..Tracking::default()
        },
    )
    .with_depth(0x40);
    assert_eq!(cutoffs(&rolling, STEP, 8), played);
}
