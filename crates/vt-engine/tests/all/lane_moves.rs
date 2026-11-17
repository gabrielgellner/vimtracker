//! A lane's division of its own and the track's `Length` and `Rate` for its column, which move
//! every phrase's lane on the column from its own (decisions 346 to 348, the lanes study).

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::lane::{Lane, LaneMove};
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::settings::SettingId;
use vt_model::song::{Chain, ChainEntry, Phrase, Song};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

/// A phrase of `rows` steps whose first FX column holds these, from its first step.
fn phrase(rows: usize, fx: &[&str]) -> Phrase {
    let mut phrase = Phrase::new(rows);
    for (at, text) in fx.iter().enumerate() {
        let step: Step = format!("--- -- {text} --- ---").parse().unwrap();
        *phrase.step_mut(at).unwrap() = step;
    }
    phrase
}

/// One track playing these phrases in one chain, numbered from 10.
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

/// Four held sends, `a b c d`, in a lane of four on FX 1, `division` steps a row.
fn four(division: usize) -> Phrase {
    let mut phrase = phrase(16, &["sna:10h", "sna:20h", "sna:30h", "sna:40h"]);
    phrase.set_lane(
        0,
        Some(Lane {
            division,
            ..Lane::new(0, 4)
        }),
    );
    phrase
}

fn engine(song: &Song) -> Engine {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    engine
}

/// Play a step, and say what the track's send is after it and where its lanes read.
fn step(engine: &mut Engine) -> (u8, [Option<usize>; 3]) {
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    let lanes = engine
        .track_heard(0)
        .lanes
        .map(|lane| lane.map(|lane| lane.row));
    (engine.live(0).send_a, lanes)
}

/// The sends heard over this many steps.
fn sends(engine: &mut Engine, steps: usize) -> Vec<u8> {
    (0..steps).map(|_| step(engine).0).collect()
}

/// A song whose track's `Length` and `Rate` for FX 1 are these.
fn moved(mut song: Song, length: u8, rate: u8) -> Song {
    song.tracks[0].lanes[0] = LaneMove { length, rate };
    song
}

#[test]
fn a_track_at_rest_reads_its_lanes_as_the_count_did_over_a_long_chain_with_an_edit() {
    // Lanes of three lengths on three columns, a phrase with none, and one with two.
    let mut first = phrase(16, &[]);
    first.set_lane(0, Some(Lane::new(0, 3)));
    first.set_lane(2, Some(Lane::new(5, 7)));
    let mut second = phrase(8, &[]);
    second.set_lane(0, Some(Lane::new(2, 5)));
    second.set_lane(1, Some(Lane::new(0, 4)));
    let third = phrase(4, &[]);
    let original = song(vec![first, second, third]);
    let mut engine = engine(&original);
    // The rows as the count gave them before lanes counted along: a lane's start and the count
    // with its column's offset, round its length; and an edit moves the offset so the column
    // reads the row it was going to.
    let mut offsets = [0_i64; 3];
    let mut song = original;
    let mut edited = false;
    for _ in 0..4096 {
        let (_, lanes) = step(&mut engine);
        let place = engine.track_heard(0);
        let count = place.count;
        let chain = song.tracks[0].chain_steps(0) as i64;
        let position = song.tracks[0]
            .chain_position(
                0,
                count as usize,
                vt_model::order::Dealer::new(song.seed, 0),
                vt_model::order::WalkMove::default(),
            )
            .unwrap();
        let phrase = &song.tracks[0].phrases[&position.phrase];
        let expected = std::array::from_fn(|slot| {
            phrase.lane(slot).map(|lane| {
                lane.start + (count + offsets[slot]).rem_euclid(lane.length as i64) as usize
            })
        });
        assert_eq!(
            lanes,
            expected,
            "at {count}, {} into the chain",
            count % chain
        );
        // Partway, while the first phrase plays, its FX 1 lane grows from 3 to 4, and the
        // column keeps the row it was going to read next.
        if !edited && count > 1000 && position.phrase == 10 && position.step < 10 {
            edited = true;
            let next = count + 1;
            let old = phrase.lane(0).unwrap();
            let was = old.start + (next + offsets[0]).rem_euclid(old.length as i64) as usize;
            let mut longer = song.clone();
            let changed = Arc::make_mut(longer.tracks[0].phrases.get_mut(&10).unwrap());
            changed.set_lane(0, Some(Lane::new(0, 4)));
            let lane = changed.lane(0).unwrap();
            let target = if lane.contains(was) { was } else { lane.start };
            offsets[0] = ((target - lane.start) as i64 - next).rem_euclid(lane.length as i64);
            engine.swap(version(&longer));
            song = longer;
        }
    }
    assert!(edited);
}

#[test]
fn plus_two_on_a_four_row_lane_plays_its_own_rows_again() {
    let heard = |length: u8, steps: usize| {
        let mut engine = engine(&moved(song(vec![four(1)]), length, 15));
        sends(&mut engine, steps)
    };
    let a_b_c_d_a_b = [0x10, 0x20, 0x30, 0x40, 0x10, 0x20];
    assert_eq!(heard(0x82, 12), [a_b_c_d_a_b, a_b_c_d_a_b].concat());
    // Four more is an exact double, and one less plays its first three.
    assert_eq!(
        heard(0x84, 8),
        [0x10, 0x20, 0x30, 0x40, 0x10, 0x20, 0x30, 0x40]
    );
    assert_eq!(heard(0x7F, 6), [0x10, 0x20, 0x30, 0x10, 0x20, 0x30]);
}

#[test]
fn a_pause_keeps_a_lanes_place() {
    let original = song(vec![four(1)]);
    let mut engine = engine(&original);
    assert_eq!(sends(&mut engine, 38)[37], 0x20, "row 1 at count 37");
    engine.swap(version(&moved(original, 0x81, 15)));
    assert_eq!(sends(&mut engine, 1), [0x30]);
    engine.pause_or_resume();
    engine.pause_or_resume();
    // On to row 3, where placing it again from the count would have row 39 % 5 = 4.
    assert_eq!(sends(&mut engine, 2), [0x40, 0x10]);
}

#[test]
fn the_playhead_is_on_the_row_being_copied() {
    let song = moved(song(vec![four(1)]), 0x82, 15);
    let mut engine = engine(&song);
    let rows: Vec<usize> = (0..6).map(|_| step(&mut engine).1[0].unwrap()).collect();
    assert_eq!(
        rows,
        [0, 1, 2, 3, 0, 1],
        "the stored rows, a copy on the first two"
    );
    let heard = engine.track_heard(0).lanes[0].unwrap().heard;
    assert_eq!((heard.length, heard.division), (6, 1));
}

#[test]
fn turning_the_length_while_it_plays_carries_on_from_its_row() {
    let original = song(vec![four(1)]);
    let mut engine = engine(&original);
    // Count 37 reads row 37 % 4 = 1.
    let played = sends(&mut engine, 38);
    assert_eq!(played[37], 0x20);
    // At 5 rows, the count would have row 38 % 5 = 3 next; counting along, it's row 2, then on
    // into the copy of row 0, and round.
    engine.swap(version(&moved(original.clone(), 0x81, 15)));
    assert_eq!(sends(&mut engine, 6), [0x30, 0x40, 0x10, 0x10, 0x20, 0x30]);
    // Shorter, with row 3 next, past the new end, it goes to the first.
    engine.swap(version(&moved(original, 0x7F, 15)));
    assert_eq!(sends(&mut engine, 4), [0x10, 0x20, 0x30, 0x10]);
    // A row of two steps past a new end is finished before the lane goes to its first.
    let slow = song(vec![four(2)]);
    let mut slower = self::engine(&slow);
    assert_eq!(sends(&mut slower, 7)[6], 0x40, "row 3's first step");
    slower.swap(version(&moved(slow, 0x7E, 15)));
    assert_eq!(sends(&mut slower, 3), [0x40, 0x10, 0x10]);
}

#[test]
fn a_divided_rate_slows_a_lane_and_a_turn_applies_from_the_next_row() {
    let original = song(vec![four(2)]);
    let mut engine = engine(&original);
    assert_eq!(
        sends(&mut engine, 8),
        [0x10, 0x10, 0x20, 0x20, 0x30, 0x30, 0x40, 0x40],
        "two steps a row"
    );
    // `÷2` plays it half as fast from the next row on, rows of four steps, and `×2` twice as fast.
    engine.swap(version(&moved(original.clone(), 0x80, 14)));
    assert_eq!(
        sends(&mut engine, 8),
        [0x10, 0x10, 0x10, 0x10, 0x20, 0x20, 0x20, 0x20]
    );
    engine.swap(version(&moved(original, 0x80, 16)));
    assert_eq!(sends(&mut engine, 3), [0x30, 0x40, 0x10]);
}

#[test]
fn a_slow_row_runs_its_fx_on_its_first_step_and_a_lock_holds_through_it() {
    // A `CHA` of none in the lane's first row, which skips the note each time it runs, then
    // locks of `Cut`, all a row of three steps, over a note on every step, which the locks
    // outlive as no note is new until the next.
    let mut slow = phrase(16, &[]);
    let rows = [
        "C-3 40 cha:00 --- ---",
        "C-3 40 cut:40 --- ---",
        "C-3 40 cut:80 --- ---",
    ];
    for row in 0..16 {
        let text = rows.get(row).copied().unwrap_or("C-3 40 --- --- ---");
        *slow.step_mut(row).unwrap() = text.parse().unwrap();
    }
    slow.set_lane(
        0,
        Some(Lane {
            division: 3,
            ..Lane::new(0, 3)
        }),
    );
    let mut engine = engine(&song(vec![slow]));
    let mut skipped = Vec::new();
    let mut cuts = Vec::new();
    for _ in 0..12 {
        step(&mut engine);
        skipped.push(engine.track_heard(0).skipped);
        cuts.push(engine.live(0).cutoff);
    }
    // The `CHA` acts once a row, on its first step, as a row's own FX on its first pulse.
    let once = [true, false, false];
    let rest = [false; 3];
    assert_eq!(skipped, [once, rest, rest, once].concat());
    // A lock from a lane holds for as long as its row, though each step's note ends the last
    // note's locks, as it's put again on each of its row's steps.
    assert_eq!(&cuts[3..9], [0x40, 0x40, 0x40, 0x80, 0x80, 0x80]);
    assert_eq!(
        cuts[10], 0xFF,
        "the next row has none, and its first note is skipped"
    );
}

#[test]
fn playing_from_a_step_places_a_lane_at_the_length_and_rate_heard_there() {
    // A phrase of 64, so every place is in the section's first time through.
    let mut long = phrase(64, &["sna:10h", "sna:20h", "sna:30h", "sna:40h"]);
    long.set_lane(0, Some(Lane::new(0, 4)));
    let song = moved(song(vec![long]), 0x82, 14);
    let mut top = engine(&song);
    let mut from_top = Vec::new();
    for _ in 0..40 {
        from_top.push(step(&mut top).1[0]);
    }
    for place in [7, 13, 25, 31] {
        let mut from = Engine::new(RATE, version(&song));
        from.play_from(place);
        let (_, lanes) = step(&mut from);
        assert_eq!(lanes[0], from_top[place], "from step {place}");
    }
}

#[test]
fn modulation_and_a_macro_reach_a_lanes_length() {
    // A square LFO two rows either way, a step's worth at a time.
    let mut lfo_song = song(vec![four(1)]);
    lfo_song.tracks[0].mods[3] = ModSlot::lfo(
        SettingId::lane(0, 0),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(8),
            ..Lfo::default()
        },
    )
    .with_depth(0x02);
    let mut lfo = engine(&lfo_song);
    let heard: Vec<usize> = (0..16)
        .map(|_| {
            step(&mut lfo);
            lfo.track_heard(0).lanes[0].unwrap().heard.length
        })
        .collect();
    assert!(
        heard.iter().all(|length| (2..=6).contains(length)),
        "{heard:?}"
    );
    assert!(heard.iter().any(|length| *length != 4), "{heard:?}");
    // A macro turned to its top holds `Length` at its map's, +12.
    let mut song = song(vec![four(1)]);
    let id = SettingId::lane(0, 0).unwrap();
    song.tracks[0].macros[0]
        .maps
        .push(vt_model::macros::MacroMap {
            target: vt_model::macros::MacroTarget::Setting(id),
            min: 0x7D00,
            max: 0x8C00,
            curve: 0,
            s: 0,
            zone: Default::default(),
        });
    song.tracks[0].macros[0].value = 0xFF00;
    let mut engine = engine(&song);
    step(&mut engine);
    step(&mut engine);
    assert_eq!(engine.track_heard(0).lanes[0].unwrap().heard.length, 16);
}
