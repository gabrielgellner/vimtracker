//! FX columns with lanes of their own, which drift against the notes.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Lane, Phrase, Song};
use vt_model::step::Step;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ))
}

/// A phrase of 16 steps whose first FX column holds these, from its first step.
fn phrase(fx: &[&str]) -> Phrase {
    let mut phrase = Phrase::new(16);
    for (at, text) in fx.iter().enumerate() {
        let step: Step = format!("--- -- {text} ---00 ---00").parse().unwrap();
        *phrase.step_mut(at).unwrap() = step;
    }
    phrase
}

/// One track playing these phrases in one chain.
fn song(phrases: Vec<Phrase>) -> Song {
    let mut song = Song::new(1);
    let mut entries = Vec::new();
    for (at, phrase) in phrases.into_iter().enumerate() {
        let id = at as u32 + 10;
        song.tracks[0].phrases.insert(id, Arc::new(phrase));
        entries.push(ChainEntry {
            phrase: id,
            transpose: 0,
        });
    }
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    song
}

fn lane(start: usize, length: usize) -> Option<Lane> {
    Some(Lane { start, length })
}

/// Three held sends in a lane of three.
fn three() -> Phrase {
    let mut phrase = phrase(&["SNA10h", "SNA20h", "SNA30h"]);
    phrase.set_lane(0, lane(0, 3));
    phrase
}

fn engine(song: &Song) -> Engine {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    engine
}

/// Play on to the end of the step at this place, and say what send it left.
fn send_at(engine: &mut Engine, place: usize) -> u8 {
    let mut out = vec![0.0; STEP * 2];
    loop {
        engine.process(&mut out);
        if engine.position() >= place {
            return engine.live(0).send_a;
        }
    }
}

#[test]
fn a_lane_repeats_on_its_own_and_carries_on_across_its_phrases_repeats() {
    let song = song(vec![three()]);
    let mut engine = engine(&song);
    let sends: Vec<u8> = (0..20).map(|place| send_at(&mut engine, place)).collect();
    let expected: Vec<u8> = (0..20).map(|place| [0x10, 0x20, 0x30][place % 3]).collect();
    assert_eq!(sends, expected);
    // The phrase's second pass starts at place 16, which reads the lane's row 16 % 3 = 1.
    assert_eq!(sends[16], 0x20);
}

#[test]
fn a_lane_carries_on_into_the_next_phrase() {
    let mut next = phrase(&["SNA40h", "SNA50h", "SNA60h"]);
    next.set_len(8);
    next.set_lane(0, lane(0, 3));
    let song = song(vec![three(), next]);
    let mut engine = engine(&song);
    // The second phrase starts at place 16, and its lane reads its row 16 % 3 = 1.
    assert_eq!(send_at(&mut engine, 16), 0x50);
    assert_eq!(send_at(&mut engine, 17), 0x60);
}

#[test]
fn a_column_without_a_lane_follows_the_notes() {
    let song = song(vec![phrase(&["SNA10h", "SNA20h", "SNA30h"])]);
    let mut engine = engine(&song);
    assert_eq!(send_at(&mut engine, 4), 0x30, "held from row 2");
    assert_eq!(send_at(&mut engine, 16), 0x10);
}

#[test]
fn playing_from_a_step_reads_the_lane_as_playing_to_it_does() {
    let song = song(vec![three()]);
    let mut to = engine(&song);
    let played = send_at(&mut to, 7);
    let mut from = Engine::new(RATE, version(&song));
    from.play_from(7);
    let mut out = vec![0.0; 2];
    from.process(&mut out);
    assert_eq!(from.position(), 7);
    assert_eq!(from.live(0).send_a, played);
}

#[test]
fn a_lane_changed_while_playing_stays_on_its_step() {
    let song = song(vec![three()]);
    let mut engine = engine(&song);
    // Place 4 reads the lane's row 1, so the next is its row 2.
    assert_eq!(send_at(&mut engine, 4), 0x20);
    let mut longer = song.clone();
    let phrase = Arc::make_mut(longer.tracks[0].phrases.get_mut(&10).unwrap());
    *phrase.step_mut(3).unwrap() = "--- -- SNA40h ---00 ---00".parse().unwrap();
    phrase.set_lane(0, lane(0, 4));
    engine.swap(version(&longer));
    assert_eq!(send_at(&mut engine, 5), 0x30, "row 2, not 5 % 4 = 1");
    assert_eq!(send_at(&mut engine, 6), 0x40);
    assert_eq!(send_at(&mut engine, 7), 0x10);
}

#[test]
fn a_glide_in_a_lane_goes_to_the_next_value_in_that_lane() {
    let mut phrase = phrase(&["---00", "SNA20h", "SNA40h", "SNA60g"]);
    phrase.set_lane(0, lane(1, 3));
    let song = song(vec![phrase]);
    let mut engine = engine(&song);
    // Place 2 reads the lane's last row, which glides back round to its first, one step away.
    assert_eq!(send_at(&mut engine, 1), 0x40);
    // Half way through place 2.
    let mut out = vec![0.0; STEP];
    engine.process(&mut out);
    assert_eq!(engine.position(), 2);
    let halfway = engine.live(0).send_a;
    assert!((0x30..0x50).contains(&halfway), "{halfway:#x}");
}
