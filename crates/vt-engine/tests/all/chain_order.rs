//! A chain's `Order` and `Jump` (decisions 423 and 424): which phrase each turn of a track's chain
//! plays, read as the turn starts and kept for it, so a lock still holding from the phrase before,
//! a macro or a turn while playing is heard from the next turn; and `LST` foreseeing the next
//! turn's phrase through them, with the die that turn rolls.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::render::render_frames;
use vt_model::macros::{MacroMap, MacroTarget, Zone};
use vt_model::order::{ChainMove, Dealer, Order, WalkMove};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Chain, ChainEntry, Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
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

const QUIET: &str = "--- --";

/// One track whose chain 0 plays these phrases of four rows each, numbered from 0.
fn song_of(phrases: &[[&str; 4]]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.phrases.clear();
    track.chains.clear();
    let mut entries = Vec::new();
    for (id, rows) in phrases.iter().enumerate() {
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        track
            .phrases
            .insert(id as u32, Arc::new(Phrase::from_steps(steps)));
        entries.push(ChainEntry {
            phrase: id as u32,
            transpose: 0,
        });
    }
    track.chains.insert(
        0,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    song
}

/// Four phrases, the `n`th with its note on its `n`th row, so where a turn's note lands says
/// which plays.
fn four() -> Song {
    let row = |phrase: usize| {
        let mut rows = [QUIET; 4];
        rows[phrase] = "C-3 --";
        rows
    };
    song_of(&[row(0), row(1), row(2), row(3)])
}

fn ordered(mut song: Song, order: Order, jump: u8) -> Song {
    song.tracks[0].chain_order = Order::ALL.iter().position(|o| *o == order).unwrap() as u8;
    song.tracks[0].chain_jump = jump;
    song
}

/// Which steps a note starts on.
fn heard(out: &[f32], steps: usize) -> Vec<usize> {
    (0..steps)
        .filter(|step| out[step * STEP * 2].abs() > 0.001)
        .collect()
}

fn hits(song: &Song, steps: usize) -> Vec<usize> {
    heard(
        &render_frames(version(song, hits_pool()), RATE, steps * STEP),
        steps,
    )
}

/// The phrase each turn of [`four`] plays, from its note.
fn phrases(song: &Song, turns: usize) -> Vec<usize> {
    let hits = hits(song, turns * 4);
    assert_eq!(hits.len(), turns, "{hits:?}");
    hits.iter()
        .enumerate()
        .map(|(turn, step)| step - turn * 4)
        .collect()
}

#[test]
fn the_chain_plays_its_phrases_in_its_order() {
    assert_eq!(phrases(&four(), 8), [0, 1, 2, 3, 0, 1, 2, 3]);
    assert_eq!(
        phrases(&ordered(four(), Order::Reverse, 0), 8),
        [3, 2, 1, 0, 3, 2, 1, 0]
    );
    assert_eq!(
        phrases(&ordered(four(), Order::UpDown, 0), 8),
        [0, 1, 2, 3, 2, 1, 0, 1]
    );
    // A dealt order and a jump, rendered twice, play the same.
    let dealt = ordered(four(), Order::Shuffle, 0x80);
    assert_eq!(phrases(&dealt, 16), phrases(&dealt, 16));
}

#[test]
fn what_the_engine_plays_is_what_the_model_walks() {
    for (order, jump) in [
        (Order::Hopscotch, 0),
        (Order::Random, 0),
        (Order::Brownian, 0x40),
        (Order::Forward, 0xC0),
    ] {
        let song = ordered(four(), order, jump);
        let walk = song.tracks[0].walk();
        let walked: Vec<usize> = (0..12)
            .map(|turn| {
                song.tracks[0]
                    .chain_position(0, turn * 4, Dealer::new(song.seed, 0), walk)
                    .unwrap()
                    .slot
            })
            .collect();
        assert_eq!(phrases(&song, 12), walked, "{order} {jump:02X}");
    }
}

#[test]
fn a_lock_still_holding_as_a_turn_starts_sets_how_it_plays() {
    // `CORD 01`, reverse, on the first phrase's last row holds until the next note, so the
    // second turn plays reverse's phrase for that turn, the third, and the rest play forward.
    let mut song = four();
    let mut rows = [QUIET; 4];
    rows[0] = "C-3 --";
    rows[3] = "--- -- chainorder:01 --- ---";
    let track = &mut song.tracks[0];
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    assert_eq!(phrases(&song, 8), [0, 2, 2, 3, 0, 2, 2, 3]);
}

#[test]
fn a_macro_turns_the_chains_order() {
    let mut song = four();
    song.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::CHAIN_ORDER),
        min: 0x0000,
        max: 0x0100,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    }];
    assert_eq!(phrases(&song, 4), [0, 1, 2, 3]);
    song.tracks[0].macros[0].value = 0xFF00;
    assert_eq!(phrases(&song, 4), [3, 2, 1, 0]);
}

#[test]
fn a_turn_while_playing_waits_for_the_next_turn_cued_or_not() {
    for cue in [0, 1] {
        let mut own = four();
        own.tracks[0].cue = cue;
        let reversed = ordered(own.clone(), Order::Reverse, 0);
        let mut engine = Engine::new(RATE, version(&own, hits_pool()));
        engine.play_from(0);
        // Half a turn in, the second phrase playing, the order turns to reverse.
        let mut out = vec![0.0; 6 * STEP * 2];
        engine.process(&mut out);
        engine.swap(version(&reversed, hits_pool()));
        let mut after = vec![0.0; 10 * STEP * 2];
        engine.process(&mut after);
        out.extend_from_slice(&after);
        // The second turn plays its own phrase out, then the third and fourth reverse's.
        assert_eq!(heard(&out, 16), [0, 5, 9, 12], "cue {cue}");
    }
}

/// A phrase of four rows whose first plays on its last pass, `LST`.
const LAST: [&str; 4] = ["C-3 -- cnd:09", QUIET, QUIET, QUIET];

#[test]
fn lst_foresees_the_next_phrase_through_the_order() {
    // The `LST` phrase twice, then a quiet one.
    let mut song = song_of(&[LAST, [QUIET; 4]]);
    let entry = |phrase| ChainEntry {
        phrase,
        transpose: 0,
    };
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![entry(0), entry(0), entry(1)],
            label: String::new(),
        }),
    );
    // Forward, the second pass is the last before the quiet phrase.
    assert_eq!(hits(&song, 24), [4, 16]);
    // Reversed, the quiet phrase comes first and the first phrase's pass, the third turn, is
    // the last.
    let reversed = ordered(song, Order::Reverse, 0);
    assert_eq!(hits(&reversed, 24), [8, 20]);
}

#[test]
fn lst_rolls_ahead_with_the_die_the_next_turn_rolls() {
    let song = ordered(song_of(&[LAST, LAST, LAST, [QUIET; 4]]), Order::Forward, 0x80);
    let walk = WalkMove {
        chain: ChainMove {
            order: 0,
            jump: 0x80,
        },
        ..WalkMove::default()
    };
    let turns = 40;
    let played: Vec<u32> = (0..=turns)
        .map(|turn| {
            song.tracks[0]
                .chain_position(0, turn * 4, Dealer::new(song.seed, 0), walk)
                .unwrap()
                .phrase
        })
        .collect();
    let expected: Vec<usize> = (0..turns)
        // A pass of one of the three `LST` phrases is the last when another phrase follows it.
        .filter(|turn| played[*turn] != 3 && played[*turn] != played[turn + 1])
        .map(|turn| turn * 4)
        .collect();
    assert!(played.windows(2).any(|pair| pair[0] == pair[1] && pair[0] != 3));
    assert!(!expected.is_empty());
    assert_eq!(hits(&song, turns * 4), expected);
}
