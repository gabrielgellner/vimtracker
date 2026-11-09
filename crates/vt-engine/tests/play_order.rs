//! Play orders, after the Metropolix's ORDER: a phrase's rows play forward, reverse, down and back,
//! out of turn or dealt at random from the project's seed, with each row's pulses together, ties
//! going into whatever row plays next, and lanes keeping their own forward loops.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::order::{Deal, Order, Pass};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::song::{Lane, Phrase, Song};
use vt_model::speed::Speed;
use vt_model::step::Step;

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-4`, which plays the ramp at its own speed.
const C4: i32 = 60;

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

/// A long flat sample, so a note's level shows for as long as it lasts.
fn flat_pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[0.5, 0.5]; RATE as usize * 8],
        },
    );
    Arc::new(pool)
}

/// One track playing these rows in this order, in C major, with notes that start and stop at once.
fn song(rows: &[&str], order: Order) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    let track = &mut song.tracks[0];
    track.sample = Some("s".to_owned());
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    let mut phrase = Phrase::from_steps(steps);
    phrase.set_order(order);
    track.phrases.insert(0, Arc::new(phrase));
    song
}

/// The eight notes of C major from `C-4` up, a row each.
fn scale_song(order: Order) -> Song {
    let rows = [
        "C-4 --", "D-4 --", "E-4 --", "F-4 --", "G-4 --", "A-4 --", "B-4 --", "C-5 --",
    ];
    song(&rows, order)
}

fn version(song: &Song, pool: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool))
}

/// The left channel, for this many steps.
fn render(song: &Song, pool: Arc<SamplePool>, steps: usize) -> Vec<f32> {
    render_frames(version(song, pool), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// Below this a little into a step, a note started there. One carried on from the step before has
/// climbed at least a step's worth, `6000 / RAMP_FRAMES`, about 0.016.
const STARTED: f32 = 0.005;

/// The note that starts at a frame, by how fast the ramp climbs, or none if none starts there.
fn note_at(out: &[f32], frame: usize) -> Option<i32> {
    let climb = out[frame + 1600] - out[frame + 600];
    let own = 1000.0 / RAMP_FRAMES as f32;
    let started = out[frame + 500] < STARTED && climb > own * 0.1;
    started.then(|| C4 + (12.0 * (climb / own).log2()).round() as i32)
}

/// The note heard at a frame, started there or carried on, by how fast the ramp climbs.
fn pitch_at(out: &[f32], frame: usize) -> i32 {
    let climb = out[frame + 1600] - out[frame + 600];
    let own = 1000.0 / RAMP_FRAMES as f32;
    C4 + (12.0 * (climb / own).log2()).round() as i32
}

/// The note each step starts with, for this many steps.
fn notes(song: &Song, steps: usize) -> Vec<Option<i32>> {
    let out = render(song, ramp_pool(), steps);
    (0..steps).map(|step| note_at(&out, step * STEP)).collect()
}

/// The notes of C major from `C-4` up, by name, or `.` for none starting.
fn named(names: &str) -> Vec<Option<i32>> {
    names
        .split_whitespace()
        .map(|name| match name {
            "." => None,
            "C" => Some(60),
            "D" => Some(62),
            "E" => Some(64),
            "F" => Some(65),
            "G" => Some(67),
            "A" => Some(69),
            "B" => Some(71),
            "C5" => Some(72),
            other => panic!("no note {other}"),
        })
        .collect()
}

/// Which steps a note starts on, over this many steps.
fn hits(song: &Song, steps: usize) -> Vec<usize> {
    let out = render(song, hits_pool(), steps);
    (0..steps)
        .filter(|step| out[step * STEP].abs() > 0.001)
        .collect()
}

#[test]
fn reverse_and_pendulum_play_as_the_manual_shows() {
    assert_eq!(
        notes(&scale_song(Order::Reverse), 16),
        named("C5 B A G F E D C  C5 B A G F E D C")
    );
    // The end rows once each, so a pass of eight rows lasts fourteen steps.
    assert_eq!(
        notes(&scale_song(Order::Pendulum), 16),
        named("C D E F G A B C5 B A G F E D  C D")
    );
    assert_eq!(
        notes(&scale_song(Order::PingPong), 17),
        named("C D E F G A B C5 C5 B A G F E D C  C")
    );
}

#[test]
fn every_fixed_order_plays_its_rows_in_turn() {
    let scale = named("C D E F G A B C5");
    let rows = vec![Step::default(); 8];
    for order in Order::ALL.into_iter().filter(|order| !order.is_dealt()) {
        let pass: Vec<Option<i32>> = Pass::new(&rows, order, Deal::default())
            .map(|row| scale[row])
            .collect();
        let twice = [pass.clone(), pass.clone()].concat();
        assert_eq!(notes(&scale_song(order), twice.len()), twice, "{order}");
    }
}

#[test]
fn a_rows_pulses_stay_together_in_any_order() {
    // Reverse plays the single of two pulses, the middle row, then the multi's two notes.
    let rows = ["C-4 -- MLT2", "D-4 --", "E-4 -- SGL2"];
    assert_eq!(
        notes(&song(&rows, Order::Reverse), 10),
        named("E . D C C  E . D C C")
    );
}

#[test]
fn a_hold_ties_into_the_row_the_order_plays_next() {
    // In reverse, the held C is followed by the E that starts the next pass.
    let rows = ["C-4 -- HLD1", "D-4 --", "E-4 --"];
    let out = render(&song(&rows, Order::Reverse), ramp_pool(), 6);
    let started: Vec<Option<i32>> = (0..6).map(|step| note_at(&out, step * STEP)).collect();
    assert_eq!(started, named("E D C . D C"), "no new attack after C");
    assert_eq!(pitch_at(&out, 3 * STEP), 64, "but it's E that's heard");
}

#[test]
fn a_lane_keeps_its_own_forward_loop() {
    let level = |song: &Song, step: usize| {
        let out = render(song, flat_pool(), step + 1);
        out[step * STEP + STEP / 2].abs()
    };
    let rows = [
        "C-4 -- vol:00",
        "C-4 -- vol:C0",
        "C-4 -- vol:C0",
        "C-4 -- vol:C0",
    ];
    // Without a lane, the silent row is the last to play in reverse.
    let rows_only = song(&rows, Order::Reverse);
    assert!(level(&rows_only, 0) > 0.1);
    assert!(level(&rows_only, 3) < 0.001);
    // With one, its column reads its steps forward, so the silent one is first.
    let mut lane = rows_only.clone();
    let phrase = Arc::make_mut(lane.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 4,
        }),
    );
    assert!(level(&lane, 0) < 0.001);
    assert!(level(&lane, 3) > 0.1);
}

#[test]
fn a_conditions_pass_is_a_time_round_the_order() {
    // Reverse plays the first row last, so its 1:2 is heard at the end of every other pass.
    let first = ["C-4 -- cnd:12", "--- --", "--- --", "--- --"];
    assert_eq!(hits(&song(&first, Order::Reverse), 16), [3, 11]);
    // Pendulum plays the second row twice a pass, both in the same pass: at 1 and 5 of the first
    // pass's six steps, neither in the second, and both in the third.
    let second = ["--- --", "C-4 -- cnd:12", "--- --", "--- --"];
    assert_eq!(hits(&song(&second, Order::Pendulum), 18), [1, 5, 13, 17]);
}

#[test]
fn an_accumulator_adds_each_time_the_order_plays_its_row() {
    // Pendulum plays the second row at 1 and 5 of each six steps, and each play moves it on.
    let rows = ["--- --", "C-4 -- acu:01", "--- --", "--- --"];
    assert_eq!(
        notes(&song(&rows, Order::Pendulum), 12),
        named(". C . . . D  . E . . . F")
    );
}

#[test]
fn a_track_at_another_speed_plays_its_order_on_its_own_steps() {
    let mut slow = song(&["C-4 --", "D-4 --", "E-4 --", "F-4 --"], Order::Reverse);
    slow.tracks[0].speed = Speed::Eighth;
    assert_eq!(notes(&slow, 8), named("F . E . D . C ."));
}

/// The notes of each pass of eight rows, for this many passes.
fn passes(song: &Song, count: usize) -> Vec<Vec<Option<i32>>> {
    notes(song, count * 8)
        .chunks(8)
        .map(<[Option<i32>]>::to_vec)
        .collect()
}

#[test]
fn a_shuffle_plays_every_row_once_a_pass_dealt_from_the_seed() {
    let mut shuffled = scale_song(Order::Shuffle);
    shuffled.seed = 3;
    let heard = passes(&shuffled, 3);
    for pass in &heard {
        let mut sorted = pass.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, named("C D E F G A B C5"));
    }
    assert!(
        heard[0] != heard[1] || heard[1] != heard[2],
        "dealt each pass"
    );
    assert_ne!(heard[0], named("C D E F G A B C5"));
    assert_eq!(passes(&shuffled, 3), heard, "a render repeats");
    let mut other = shuffled.clone();
    other.seed = 4;
    assert_ne!(passes(&other, 3), heard, "another seed deals another");
}

#[test]
fn random_picks_rows_from_the_seed_and_can_repeat_them() {
    let mut random = scale_song(Order::Random);
    random.seed = 11;
    let heard: Vec<Option<i32>> = passes(&random, 4).concat();
    assert!(heard.iter().all(Option::is_some), "a pick every step");
    let mut distinct = heard.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() > 4, "many rows are picked: {heard:?}");
    let repeats = heard.windows(8).any(|pass| {
        let mut sorted = pass.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len() < 8
    });
    assert!(repeats, "some pass repeats a row");
    assert_eq!(passes(&random, 4).concat(), heard, "a render repeats");
}

#[test]
fn a_chance_rolls_the_same_whatever_the_order() {
    // Every row's the same, so whichever the order plays, each step rolls one `CHA`.
    let rows = ["C-4 -- cha:80"; 8];
    let mut forward = song(&rows, Order::Forward);
    forward.seed = 9;
    let mut random = forward.clone();
    let phrase = Arc::make_mut(random.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_order(Order::Random);
    let rolled = hits(&forward, 32);
    assert!(!rolled.is_empty() && rolled.len() < 32);
    assert_eq!(hits(&random, 32), rolled);
}

#[test]
fn playing_from_a_step_hears_what_playing_up_to_it_would() {
    // A longer track makes the section long enough to start in its third pass.
    let mut song = scale_song(Order::Shuffle);
    song.seed = 21;
    song.add_track();
    song.tracks[1].phrases.insert(0, Arc::new(Phrase::new(64)));
    let whole = render(&song, ramp_pool(), 24);
    let mut engine = Engine::new(RATE, version(&song, ramp_pool()));
    engine.play_from(16);
    let mut out = vec![0.0; 8 * STEP * 2];
    engine.process(&mut out);
    let from: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    let started = |out: &[f32], step: usize| note_at(out, step * STEP);
    let expected: Vec<Option<i32>> = (16..24).map(|step| started(&whole, step)).collect();
    let heard: Vec<Option<i32>> = (0..8).map(|step| started(&from, step)).collect();
    assert_eq!(heard, expected);
}

#[test]
fn changing_the_order_while_playing_keeps_the_row_about_to_play() {
    let mut engine = Engine::new(RATE, version(&scale_song(Order::Forward), ramp_pool()));
    engine.play_from(0);
    // Rows 00 to 03 have started, so row 04, G, is next.
    let mut out = vec![0.0; (3 * STEP + 1) * 2];
    engine.process(&mut out);
    engine.swap(version(&scale_song(Order::Reverse), ramp_pool()));
    let mut after = vec![0.0; 5 * STEP * 2];
    engine.process(&mut after);
    let left: Vec<f32> = after.chunks(2).map(|frame| frame[0]).collect();
    let heard: Vec<Option<i32>> = (1..5).map(|step| note_at(&left, step * STEP - 1)).collect();
    assert_eq!(heard, named("G F E D"), "on from G, in reverse");
}
