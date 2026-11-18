//! A chain's `Order` and `Jump` (decisions 423 and 424): its turns walked as an order walks a
//! phrase's rows, and the chance a turn plays any of its phrases, each turn keeping its own
//! phrase's length so the chain keeps its time.

use std::sync::Arc;

use vt_model::order::{ChainMove, Deal, Dealer, Order, Round, WalkMove};
use vt_model::song::{Chain, ChainEntry, Phrase, Song, Track};

/// One track whose chain 0 plays phrases 10, 11, ... of these lengths, in turn.
fn chained(lengths: &[usize]) -> Track {
    let mut song = Song::new(1);
    let mut track = std::mem::take(&mut song.tracks[0]);
    track.chains.clear();
    let entries = lengths
        .iter()
        .enumerate()
        .map(|(at, length)| {
            let phrase = 10 + at as u32;
            track
                .phrases
                .insert(phrase, Arc::new(Phrase::new(*length)));
            ChainEntry {
                phrase,
                transpose: at as i8,
            }
        })
        .collect();
    track.chains.insert(
        0,
        Arc::new(Chain {
            entries,
            label: String::new(),
        }),
    );
    track
}

fn moved(order: u8, jump: u8) -> WalkMove {
    WalkMove {
        chain: ChainMove { order, jump },
        ..WalkMove::default()
    }
}

/// The slot each turn plays, for so many turns of a chain of four-row phrases.
fn turns(track: &Track, turns: usize, walk: WalkMove) -> Vec<usize> {
    (0..turns)
        .map(|turn| {
            track
                .chain_position(0, turn * 4, Dealer::new(7, 0), walk)
                .unwrap()
                .slot
        })
        .collect()
}

#[test]
fn every_fixed_order_walks_a_chain_of_four_as_it_walks_four_rows() {
    let track = chained(&[4, 4, 4, 4]);
    let rows = vec![vt_model::step::Step::default(); 4];
    for (at, order) in Order::ALL.into_iter().enumerate() {
        if order.is_dealt() {
            continue;
        }
        // Its round of four rows, carried on round after round, as a phrase's walk drifts across
        // its passes.
        let round: Vec<usize> = Round::new(&rows, order, Deal::default()).collect();
        let expected: Vec<usize> = round.iter().copied().cycle().take(24).collect();
        assert_eq!(turns(&track, 24, moved(at as u8, 0)), expected, "{order}");
    }
    assert_eq!(turns(&track, 8, moved(0, 0)), [0, 1, 2, 3, 0, 1, 2, 3]);
    assert_eq!(turns(&track, 8, moved(1, 0)), [3, 2, 1, 0, 3, 2, 1, 0]);
    assert_eq!(turns(&track, 8, moved(2, 0)), [0, 1, 2, 3, 2, 1, 0, 1]);
}

#[test]
fn the_dealt_orders_are_dealt_again_each_time_round() {
    let track = chained(&[4, 4, 4, 4]);
    let at = |order: Order| Order::ALL.iter().position(|o| *o == order).unwrap() as u8;
    let laps = 32;
    let shuffled = turns(&track, 4 * laps, moved(at(Order::Shuffle), 0));
    let mut differ = false;
    for lap in shuffled.chunks(4) {
        let mut sorted = lap.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, [0, 1, 2, 3], "each phrase once a time round: {lap:?}");
        differ |= lap != &shuffled[..4];
    }
    assert!(differ, "dealt again each time round");
    // `Quasi-random` never plays the turn's phrase just played within a time round, and
    // `Brownian` starts from the first each time round and steps a phrase at most.
    let quasi = turns(&track, 4 * laps, moved(at(Order::QuasiRandom), 0));
    for lap in quasi.chunks(4) {
        assert!(lap.windows(2).all(|pair| pair[0] != pair[1]), "{lap:?}");
    }
    let brownian = turns(&track, 4 * laps, moved(at(Order::Brownian), 0));
    for lap in brownian.chunks(4) {
        assert_eq!(lap[0], 0);
        assert!(
            lap.windows(2)
                .all(|pair| matches!((pair[1] + 4 - pair[0]) % 4, 0 | 1 | 3)),
            "{lap:?}"
        );
    }
    let random = turns(&track, 4 * laps, moved(at(Order::Random), 0));
    assert!((0..4).all(|slot| random.contains(&slot)));
    assert_eq!(random, turns(&track, 4 * laps, moved(at(Order::Random), 0)));
}

#[test]
fn jump_plays_any_phrase_at_its_chance() {
    let track = chained(&[4, 4, 4, 4]);
    let count = 4000;
    let forward: Vec<usize> = (0..count).map(|turn| turn % 4).collect();
    assert_eq!(turns(&track, count, moved(0, 0)), forward);
    for (jump, share) in [(0x40, 0.25), (0x80, 0.5), (0xFF, 1.0)] {
        let heard = turns(&track, count, moved(0, jump));
        // A jump lands on the walked phrase a quarter of the time.
        let moved_away = heard.iter().zip(&forward).filter(|(a, b)| a != b).count();
        let expected = share * 0.75 * count as f64;
        assert!(
            (moved_away as f64 - expected).abs() < 0.05 * count as f64,
            "{jump:02X}: {moved_away} of {count}, about {expected}"
        );
        for slot in 0..4 {
            let times = heard.iter().filter(|heard| **heard == slot).count();
            assert!((times as f64 - count as f64 / 4.0).abs() < 0.05 * count as f64);
        }
        assert_eq!(heard, turns(&track, count, moved(0, jump)), "it repeats");
    }
}

#[test]
fn a_chain_of_one_phrase_plays_it_whatever_the_order_or_jump() {
    let track = chained(&[4]);
    for order in 0..Order::ALL.len() as u8 {
        assert_eq!(turns(&track, 6, moved(order, 0xFF)), [0; 6]);
    }
}

#[test]
fn a_turn_keeps_its_own_length_whatever_plays_in_it() {
    // Phrases of 2, 4 and 8 rows, reversed: the 2-row turn plays the 8 cut short, the 8-row turn
    // plays the 2 four times over, and the chain keeps its 14 steps.
    let track = chained(&[2, 4, 8]);
    assert_eq!(track.chain_steps(0), 14);
    let walk = moved(1, 0);
    let heard: Vec<(usize, usize, usize, usize)> = (0..14)
        .map(|step| {
            let position = track.chain_position(0, step, Dealer::new(7, 0), walk).unwrap();
            (position.slot, position.step, position.turn, position.turn_left)
        })
        .collect();
    assert_eq!(&heard[..2], &[(2, 0, 0, 2), (2, 1, 0, 1)]);
    assert_eq!(heard[2..6].iter().map(|h| h.0).collect::<Vec<_>>(), [1; 4]);
    assert_eq!(
        heard[6..].iter().map(|h| (h.0, h.1)).collect::<Vec<_>>(),
        [(0, 0), (0, 1), (0, 0), (0, 1), (0, 0), (0, 1), (0, 0), (0, 1)]
    );
    assert_eq!(heard[13].3, 1);
    // The heard slot's transpose comes with it.
    let position = track.chain_position(0, 0, Dealer::new(7, 0), walk).unwrap();
    assert_eq!(position.transpose, 2);
}

#[test]
fn at_rest_a_chain_plays_as_written() {
    let track = chained(&[2, 4, 8]);
    let dealer = Dealer::new(7, 0);
    for step in 0..40 {
        let position = track.chain_position(0, step, dealer, WalkMove::default()).unwrap();
        assert_eq!(position.slot, position.turn);
        let own = track.walk();
        assert_eq!(own.chain, ChainMove::default());
    }
}

#[test]
fn the_chain_orders_list_names_every_order_as_it_shows() {
    for (name, order) in vt_model::order::CHAIN_ORDER_NAMES.iter().zip(Order::ALL) {
        assert_eq!(*name, order.to_string());
    }
}
