//! A track's `Phrase` settings (decision 400): `ORD` goes so many places on through the list of
//! orders from each phrase's own, `REV` turns its rows the other way up, and `ORDX` and `ORDY` put
//! numbers of their own in place of the order's. They're read before the row plays, as `JMP` is,
//! so a lock on a row, a macro or an LFO moves them, the walk is worked out from the place alone,
//! and a render repeats.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::order::{Arp, Cartesian, Dealer, Order, Snake, WalkMove};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};

use crate::support::version;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;

fn ramp_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

/// Sixteen rows a semitone apart from `C-3`, each a note that starts and stops at once.
const NOTES: [&str; 16] = [
    "C-3 --", "C#3 --", "D-3 --", "D#3 --", "E-3 --", "F-3 --", "F#3 --", "G-3 --", "G#3 --",
    "A-3 --", "A#3 --", "B-3 --", "C-4 --", "C#4 --", "D-4 --", "D#4 --",
];

/// One track playing the first `rows` of [`NOTES`], or these rows, in this order.
fn song_of(rows: &[&str], order: Order) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    let mut phrase = Phrase::from_steps(steps);
    phrase.set_order(order);
    track.phrases.insert(0, Arc::new(phrase));
    song
}

fn song(rows: usize, order: Order) -> Song {
    song_of(&NOTES[..rows], order)
}

/// The row each step starts, counted from 0, by how fast the ramp climbs, for this many steps.
fn rows_heard(song: &Song, steps: usize) -> Vec<usize> {
    let out: Vec<f32> = render_frames(version(song, ramp_pool()), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect();
    let own = 1000.0 / RAMP_FRAMES as f32;
    (0..steps)
        .map(|step| {
            let frame = step * STEP;
            let climb = out[frame + 1600] - out[frame + 600];
            let note = C3 + (12.0 * (climb / own).log2()).round() as i32;
            (note - C3) as usize
        })
        .collect()
}

/// The rows the model's walk plays over this many steps, as the track's settings move it.
fn walked(song: &Song, steps: usize, walk: WalkMove) -> Vec<usize> {
    let track = &song.tracks[0];
    (0..steps)
        .map(|step| {
            track
                .chain_position(0, step, Dealer::new(song.seed, 0), walk)
                .unwrap()
                .step
        })
        .collect()
}

fn ordered(order: u8) -> WalkMove {
    WalkMove {
        order,
        ..WalkMove::default()
    }
}

fn numbered(x: u8, y: u8) -> WalkMove {
    WalkMove {
        x,
        y,
        ..WalkMove::default()
    }
}

/// The song with the track holding this walk.
fn holding(mut song: Song, walk: WalkMove) -> Song {
    let track = &mut song.tracks[0];
    track.order = walk.order;
    track.reverse = u8::from(walk.reverse);
    track.order_x = walk.x;
    track.order_y = walk.y;
    song
}

/// Counted from 1, as the Metropolix's manual and the orders' meanings count rows.
fn manual(text: &str) -> Vec<usize> {
    text.split_whitespace()
        .map(|n| n.parse::<usize>().unwrap() - 1)
        .collect()
}

#[test]
fn at_rest_every_phrase_plays_its_own_order_sample_for_sample() {
    for order in [
        Order::Forward,
        Order::UpDown,
        Order::Shuffle,
        Order::Jump(5),
    ] {
        let own = song(8, order);
        let at_rest = holding(own.clone(), WalkMove::default());
        let frames = |song: &Song| render_frames(version(song, ramp_pool()), RATE, 24 * STEP);
        assert_eq!(frames(&own), frames(&at_rest), "{order}");
        assert_eq!(rows_heard(&own, 24), walked(&own, 24, WalkMove::default()));
    }
}

#[test]
fn every_order_offset_moves_through_the_list_and_stops_at_its_ends() {
    // From a phrase of its own `Jump 5`, every place either way and past both ends.
    let own = song(8, Order::Jump(5));
    let jump = Order::ALL
        .iter()
        .position(|order| order.is_like(Order::Jump(5)))
        .unwrap() as i32;
    let last = Order::ALL.len() as i32 - 1;
    for by in -(jump + 2)..=(last - jump + 2) {
        let walk = ordered((0x80 + by) as u8);
        let expected = match by {
            0 => Order::Jump(5),
            by => Order::ALL[(jump + by).clamp(0, last) as usize],
        };
        let phrase = &own.tracks[0].phrases[&0];
        assert_eq!(phrase.order_walked(walk), expected, "{by:+}");
        let moved = holding(own.clone(), walk);
        let heard = rows_heard(&moved, 24);
        assert_eq!(heard, walked(&own, 24, walk), "{by:+} plays {expected}");
        assert_eq!(heard, rows_heard(&moved, 24), "{by:+}: a render repeats");
    }
    // A few by hand: one on from forward is reverse, two on is up-down.
    let forward = song(8, Order::Forward);
    assert_eq!(
        rows_heard(&holding(forward.clone(), ordered(0x81)), 8),
        manual("8 7 6 5 4 3 2 1")
    );
    assert_eq!(
        rows_heard(&holding(forward.clone(), ordered(0x82)), 14),
        manual("1 2 3 4 5 6 7 8 7 6 5 4 3 2")
    );
    assert_eq!(
        rows_heard(&holding(forward, ordered(0x7F)), 8),
        manual("1 2 3 4 5 6 7 8"),
        "nothing before forward"
    );
}

#[test]
fn reverse_turns_the_walk_the_other_way_up() {
    let reversed = WalkMove {
        reverse: true,
        ..WalkMove::default()
    };
    let heard = |order: Order, steps: usize| rows_heard(&holding(song(8, order), reversed), steps);
    assert_eq!(heard(Order::Forward, 8), manual("8 7 6 5 4 3 2 1"));
    assert_eq!(heard(Order::Reverse, 8), manual("1 2 3 4 5 6 7 8"));
    // Up-down the other way up is down-up, the Metropolix's Pendulum in reverse.
    assert_eq!(
        heard(Order::UpDown, 14),
        manual("8 7 6 5 4 3 2 1 2 3 4 5 6 7")
    );
    assert_eq!(heard(Order::Jump(3), 8), manual("8 5 2 7 4 1 6 3"));
    // A dealt order deals the same rows each pass, the other way up.
    let own = song(8, Order::Shuffle);
    let shuffled = rows_heard(&own, 16);
    let flipped: Vec<usize> = shuffled.iter().map(|row| 7 - row).collect();
    assert_eq!(rows_heard(&holding(own, reversed), 16), flipped);
}

#[test]
fn order_x_and_y_number_jump_arp_snake_and_cartesian() {
    // Jump's rows.
    let jump = holding(song(8, Order::Jump(3)), numbered(5, 0));
    assert_eq!(rows_heard(&jump, 8), manual("1 6 3 8 5 2 7 4"));
    // An arp's notes and step: runs of four, each two on.
    let arp = holding(song(8, Order::Arp(Arp::START)), numbered(4, 2));
    assert_eq!(
        rows_heard(&arp, 16),
        manual("1 2 3 4 3 4 5 6 5 6 7 8 7 8 1 2")
    );
    // Only Y: runs of three, each two on.
    let step = holding(song(8, Order::Arp(Arp::START)), numbered(0, 2));
    assert_eq!(rows_heard(&step, 9), manual("1 2 3 3 4 5 5 6 7"));
    // The snake by its number, as René's are chosen: 6 is the spiral.
    let snake = holding(song(16, Order::Snake(Snake::Zigzag)), numbered(6, 0));
    assert_eq!(
        rows_heard(&snake, 16),
        manual("1 2 3 4 8 12 16 15 14 13 9 5 6 7 11 10")
    );
    // Cartesian's X and Y rates.
    let own = song(16, Order::Cartesian(Cartesian::new(1, 3)));
    let cartesian = holding(own.clone(), numbered(2, 5));
    let phrase = &own.tracks[0].phrases[&0];
    assert_eq!(
        phrase.order_walked(numbered(2, 5)),
        Order::Cartesian(Cartesian::new(2, 5))
    );
    assert_eq!(rows_heard(&cartesian, 32), walked(&own, 32, numbered(2, 5)));
    assert_ne!(rows_heard(&cartesian, 32), rows_heard(&own, 32));
    // An order without numbers takes none, and `00` is the phrase's own.
    let forward = holding(song(8, Order::Forward), numbered(5, 2));
    assert_eq!(rows_heard(&forward, 8), manual("1 2 3 4 5 6 7 8"));
    let own_jump = holding(song(8, Order::Jump(5)), numbered(0, 7));
    assert_eq!(rows_heard(&own_jump, 8), manual("1 6 3 8 5 2 7 4"));
}

#[test]
fn an_ord_on_a_row_moves_the_step_that_walks_to_it() {
    // `ORD 81` on the third row: the step that walks there plays reverse's row at that place,
    // the sixth, and the walk goes on as written, as the lock was read, not played.
    let mut rows: Vec<&str> = NOTES[..8].to_vec();
    rows[2] = "D-3 -- order:81 --- ---";
    let locked = song_of(&rows, Order::Forward);
    assert_eq!(
        rows_heard(&locked, 16),
        manual("1 2 6 4 5 6 7 8 1 2 6 4 5 6 7 8")
    );
    // A `REV` on the last row turns the step that walks there the other way up, so it plays
    // the first, each pass.
    let mut rows: Vec<&str> = NOTES[..8].to_vec();
    rows[7] = "G-3 -- --- reverse:01 ---";
    let reversed = song_of(&rows, Order::Forward);
    assert_eq!(
        rows_heard(&reversed, 16),
        manual("1 2 3 4 5 6 7 1 1 2 3 4 5 6 7 1")
    );
}

#[test]
fn a_macro_turns_the_order() {
    use vt_model::macros::{MacroMap, MacroTarget, Zone};
    let mut turned = song(8, Order::Forward);
    turned.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::ORDER),
        min: 0x8000,
        max: 0x8100,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    }];
    assert_eq!(rows_heard(&turned, 8), manual("1 2 3 4 5 6 7 8"));
    turned.tracks[0].macros[0].value = 0xFF00;
    assert_eq!(rows_heard(&turned, 8), manual("8 7 6 5 4 3 2 1"));
}

#[test]
fn an_lfo_on_the_order_moves_its_high_half_alone() {
    // A square at full depth: its high half moves forward to the list's end, its low half to
    // its start, which is forward.
    let mut lfo = song(8, Order::Forward);
    lfo.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::ORDER),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(16),
            ..Lfo::default()
        },
    )
    .with_depth(0xFF);
    let heard = rows_heard(&lfo, 32);
    let forward = manual("1 2 3 4 5 6 7 8 1 2 3 4 5 6 7 8");
    assert_ne!(heard[1..7], forward[1..7], "{heard:?}");
    assert_eq!(heard[9..15], forward[9..15], "{heard:?}");
    assert_eq!(heard, rows_heard(&lfo, 32), "a render repeats");
}

#[test]
fn the_engine_reports_the_walk_it_read_so_the_playhead_follows_it() {
    let moved = holding(song(8, Order::Forward), ordered(0x81));
    let mut engine = Engine::new(RATE, crate::support::no_samples::version(&moved));
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    for _ in 0..5 {
        engine.process(&mut out);
        let place = engine.track_heard(0);
        assert_eq!(place.walk, ordered(0x81));
        let position = moved.tracks[0]
            .chain_position(
                0,
                place.count as usize,
                Dealer::new(moved.seed, 0),
                place.walk,
            )
            .unwrap();
        assert_eq!(position.step, 7 - place.count as usize, "reverse's row");
    }
}
