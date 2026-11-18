//! An FX column's lane's `Order` and `Reverse` (decision 431): the order its rows are walked in,
//! from the whole list, `00` `Forward` as written, and a switch that turns the walk the other way
//! up. The lap stays the lane's length, so a longer round drifts across its laps; a dealt order is
//! dealt again each lap from the seed; `Jump` and `Hold` act on top; at `Forward` and `Off` every
//! lane plays as before.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::macros::{MacroMap, MacroTarget, Zone};
use vt_model::order::Order;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Phrase, Song};
use vt_model::step::Step;

use crate::support::no_samples::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

/// The place of an order in the list `LORD` chooses from.
fn listed(order: Order) -> u8 {
    Order::ALL.iter().position(|at| *at == order).unwrap() as u8
}

/// One track playing a phrase of 64 rows whose FX 1 lane is its first `rows`, a step a row, each a
/// send of its own, walked in `order`, turned the other way up if `reverse`.
fn song(rows: usize, order: Order, reverse: bool) -> Song {
    let mut phrase = Phrase::new(64);
    for row in 0..rows {
        let text = format!("--- -- sna:{:02X}h --- ---", (row + 1) * 0x10);
        *phrase.step_mut(row).unwrap() = text.parse::<Step>().unwrap();
    }
    phrase.set_lane(0, Some(Lane::new(0, rows)));
    let mut song = Song::new(1);
    song.seed = 0x5EED;
    song.tracks[0].phrases.insert(0, Arc::new(phrase));
    song.tracks[0].lanes[0].order = listed(order);
    song.tracks[0].lanes[0].reverse = reverse;
    song
}

/// The stored row FX 1's lane reads at each of this many steps, playing from `from`.
fn rows_from(song: &Song, from: usize, steps: usize) -> Vec<usize> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(from);
    let mut out = vec![0.0; STEP * 2];
    (0..steps)
        .map(|_| {
            engine.process(&mut out);
            engine.track_heard(0).lanes[0].unwrap().row
        })
        .collect()
}

fn rows(song: &Song, steps: usize) -> Vec<usize> {
    rows_from(song, 0, steps)
}

/// The rows a lane of `rows` counts to, one a step.
fn counted(rows: usize, steps: usize) -> Vec<usize> {
    (0..steps).map(|step| step % rows).collect()
}

/// Whether `pattern` repeats every `period` of its places.
fn repeats(pattern: &[usize], period: usize) -> bool {
    pattern
        .iter()
        .zip(pattern.iter().skip(period))
        .all(|(a, b)| a == b)
}

#[test]
fn forward_and_off_count_as_before() {
    let forward = song(8, Order::Forward, false);
    assert_eq!(rows(&forward, 64), counted(8, 64));
    // Played sample for sample as a song that never had them.
    let render = |song: &Song| {
        let mut engine = Engine::new(RATE, version(song));
        engine.play_from(0);
        let mut out = vec![0.0; STEP * 2 * 32];
        engine.process(&mut out);
        out
    };
    let mut moved = song(8, Order::Reverse, true);
    moved.tracks[0].lanes[0].order = 0;
    moved.tracks[0].lanes[0].reverse = false;
    assert_eq!(render(&moved), render(&forward));
}

#[test]
fn reverse_in_the_list_walks_from_the_last_row() {
    let heard = rows(&song(4, Order::Reverse, false), 16);
    assert_eq!(heard, [3, 2, 1, 0].repeat(4));
}

#[test]
fn up_down_drifts_across_the_laps_of_a_four_row_lane() {
    // `1 2 3 4 | 3 2 1 2 | 3 4 3 2`: a round of six over laps of four, as a phrase's walk drifts
    // across its passes.
    let heard = rows(&song(4, Order::UpDown, false), 24);
    assert_eq!(heard, [0, 1, 2, 3, 2, 1].repeat(4));
}

#[test]
fn reverse_turns_any_order_the_other_way_up() {
    let up_down = rows(&song(4, Order::UpDown, false), 24);
    let turned = rows(&song(4, Order::UpDown, true), 24);
    let mirrored: Vec<usize> = up_down.iter().map(|row| 3 - row).collect();
    assert_eq!(turned, mirrored);
    // And Forward the other way up is Reverse.
    assert_eq!(
        rows(&song(4, Order::Forward, true), 16),
        rows(&song(4, Order::Reverse, false), 16)
    );
}

#[test]
fn a_dealt_order_repeats_for_a_seed_and_is_dealt_again_each_lap() {
    for order in [
        Order::Random,
        Order::Brownian,
        Order::QuasiRandom,
        Order::Shuffle,
    ] {
        let dealt = song(8, order, false);
        let heard = rows(&dealt, 64);
        assert_eq!(heard, rows(&dealt, 64), "{order}: a render repeats");
        assert!(
            !repeats(&heard, 8),
            "{order}: dealt again each lap: {heard:?}"
        );
        let mut other = dealt;
        other.seed = 0xD1CE;
        assert_ne!(
            rows(&other, 64),
            heard,
            "{order}: another seed deals another walk"
        );
    }
    // Brownian walks from the first row each lap, a row on, back or staying each time.
    let walked = rows(&song(8, Order::Brownian, false), 64);
    for lap in walked.chunks(8) {
        assert_eq!(lap[0], 0, "{walked:?}");
        for two in lap.windows(2) {
            let step = (two[1] as isize - two[0] as isize).rem_euclid(8);
            assert!([0, 1, 7].contains(&step), "{walked:?}");
        }
    }
    // Shuffle plays every row once a lap.
    for lap in rows(&song(8, Order::Shuffle, false), 64).chunks(8) {
        let mut sorted = lap.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..8).collect::<Vec<_>>());
    }
}

#[test]
fn jump_and_hold_act_on_top_of_the_order() {
    // Where the forward lane jumps, the reversed one jumps to the same row, as the jumps roll from
    // the lap and row whatever the order; everywhere else it plays the reversed walk.
    let jumping = |reverse_order: bool| {
        let order = if reverse_order {
            Order::Reverse
        } else {
            Order::Forward
        };
        let mut song = song(8, order, false);
        song.tracks[0].lanes[0].jump = 0x80;
        rows(&song, 128)
    };
    let (forward, reversed) = (jumping(false), jumping(true));
    let mut walked = 0;
    for step in 0..128 {
        if forward[step] != step % 8 {
            assert_eq!(reversed[step], forward[step], "step {step}");
        } else if reversed[step] == 7 - step % 8 {
            walked += 1;
        }
    }
    assert!(walked > 32, "{walked}: {reversed:?}");
    // A hold keeps what the order played before, so at full the order's first row holds.
    let mut held = song(4, Order::Reverse, false);
    held.tracks[0].lanes[0].hold = 0xFF;
    assert_eq!(rows(&held, 16), vec![3; 16]);
}

#[test]
fn a_lock_turns_the_order_and_reverse_from_the_next_row_on() {
    // `lane1.order:01`, Reverse, on the phrase's tenth row, outside the lane.
    let mut locked = song(8, Order::Forward, false);
    let phrase = Arc::make_mut(locked.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(9).unwrap() = "--- -- --- lane1.order:01 ---".parse().unwrap();
    let heard = rows(&locked, 32);
    assert_eq!(heard[..10], counted(8, 10)[..], "{heard:?}");
    let reversed: Vec<usize> = (11..32).map(|step| 7 - step % 8).collect();
    assert_eq!(heard[11..], reversed[..], "{heard:?}");
    assert_eq!(SettingId::lane(0, 5).unwrap().key(), "lane1.order");
    assert_eq!(SettingId::lane(0, 6).unwrap().key(), "lane1.reverse");
    assert_eq!(
        SettingId::lane(2, 6).unwrap().label().as_deref(),
        Some("LREV3")
    );
    // `lane1.reverse:01` does the same with the order forward.
    let mut switched = song(8, Order::Forward, false);
    let phrase = Arc::make_mut(switched.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(9).unwrap() = "--- -- --- lane1.reverse:01 ---".parse().unwrap();
    assert_eq!(rows(&switched, 32), heard);
}

#[test]
fn a_macro_turns_the_order_and_reverse() {
    let mapped = |setting: usize| {
        let mut turned = song(8, Order::Forward, false);
        turned.tracks[0].macros[0].maps = vec![MacroMap {
            target: MacroTarget::Setting(SettingId::lane(0, setting).unwrap()),
            min: 0,
            max: 0xFF00,
            curve: 0,
            s: 0,
            zone: Zone::default(),
            member: None,
        }];
        turned
    };
    let mut order = mapped(5);
    assert_eq!(rows(&order, 32), counted(8, 32));
    order.tracks[0].macros[0].value = 0xFF00;
    let last = *Order::ALL.last().unwrap();
    assert_eq!(rows(&order, 32), rows(&song(8, last, false), 32));
    let mut reverse = mapped(6);
    reverse.tracks[0].macros[0].value = 0xFF00;
    assert_eq!(
        rows(&reverse, 32),
        rows(&song(8, Order::Reverse, false), 32)
    );
}

#[test]
fn playing_from_a_step_walks_as_playing_from_the_top() {
    for order in [
        Order::UpDown,
        Order::Brownian,
        Order::QuasiRandom,
        Order::Pedal,
        Order::Snake(vt_model::order::Snake::Spiral),
        Order::ALL[Order::ALL.len() - 1],
    ] {
        let walked = song(6, order, true);
        let top = rows(&walked, 64);
        for place in [5, 13, 27, 40] {
            assert_eq!(
                rows_from(&walked, place, 64 - place),
                top[place..],
                "{order} from step {place}"
            );
        }
    }
}

#[test]
fn cue_never_holds_the_order_back() {
    let mut cued = song(8, Order::UpDown, false);
    cued.tracks[0].cue = 1;
    assert_eq!(rows(&cued, 64), rows(&song(8, Order::UpDown, false), 64));
}

#[test]
fn after_sed_a_dealt_order_walks_the_same_every_pass() {
    // A phrase of 16 rows whose FX 1 lane is its first four, walked at random, with an `SED` in
    // FX 2 on its first row or not (decision 421).
    let walked = |first: &str| {
        let mut phrase = Phrase::new(16);
        for row in 0..4 {
            let text = format!("--- -- sna:{:02X}h --- ---", (row + 1) * 0x10);
            *phrase.step_mut(row).unwrap() = text.parse::<Step>().unwrap();
        }
        let text = format!("--- -- sna:10h {first} ---");
        *phrase.step_mut(0).unwrap() = text.parse::<Step>().unwrap();
        phrase.set_lane(0, Some(Lane::new(0, 4)));
        let mut song = Song::new(1);
        song.seed = 0x5EED;
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        song.tracks[0].lanes[0].order = listed(Order::Random);
        rows(&song, 64)
    };
    let reseeded = walked("sed:2A");
    assert!(repeats(&reseeded, 16), "{reseeded:?}");
    assert!(!repeats(&reseeded, 4), "{reseeded:?}");
    let fresh = walked("---");
    assert!(!repeats(&fresh, 16), "{fresh:?}");
}
