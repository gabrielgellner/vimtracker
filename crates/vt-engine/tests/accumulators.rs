//! Accumulators, after the Metropolix's ACCUM: an `ACU` moves its row's note on each time the row
//! plays, by scale degrees, or semitones with the scale off, within the track's limits, and the
//! track's settings say what happens there, whether the row or the whole track moves, when it adds
//! and when it starts again.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::accumulator::{Accumulation, Adds, AtLimit, Moves, Reset};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::song::{Chain, ChainEntry, Phrase, Song};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-4`, which plays the ramp at its own speed.
const C4: i32 = 60;

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("ramp", SampleData { frames });
    Arc::new(pool)
}

const C_MAJOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Major,
};

/// One track playing these rows in C major, with notes that start and stop at once, and these
/// accumulator settings.
fn song(rows: &[&str], accumulation: Accumulation) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.scale = C_MAJOR;
    let track = &mut song.tracks[0];
    track.sample = Some("ramp".to_owned());
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    track.accumulation = accumulation;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    render_frames(version(song), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// Below this a little into a step, a note started there. One carried on from the step before has
/// climbed at least a step's worth, `6000 / RAMP_FRAMES`, about 0.016.
const STARTED: f32 = 0.005;

/// The note that starts at a frame, by how fast the ramp climbs over a thousand frames past the
/// fade of the note before, or none if no note starts there.
fn note_at(out: &[f32], frame: usize) -> Option<i32> {
    let climb = out[frame + 1600] - out[frame + 600];
    let own = 1000.0 / RAMP_FRAMES as f32;
    let started = out[frame + 500] < STARTED && climb > own * 0.1;
    started.then(|| C4 + (12.0 * (climb / own).log2()).round() as i32)
}

/// The note each step starts with, for this many steps.
fn notes(song: &Song, steps: usize) -> Vec<Option<i32>> {
    let out = render(song, steps);
    (0..steps).map(|step| note_at(&out, step * STEP)).collect()
}

/// The notes of C major from `C-4` up, by name, for the steps to be read against.
fn named(names: &str) -> Vec<Option<i32>> {
    names
        .split_whitespace()
        .map(|name| match name {
            "." => None,
            "B3" => Some(59),
            "A3" => Some(57),
            "C" => Some(60),
            "C#" => Some(61),
            "D" => Some(62),
            "D#" => Some(63),
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

/// The manual's four-stage sequence, `C D E F`, with this on its first row.
fn manuals(fx: &str, accumulation: Accumulation) -> Song {
    let first = format!("C-4 -- {fx}");
    song(&[&first, "D-4 --", "E-4 --", "F-4 --"], accumulation)
}

fn with(moves: Moves, at_limit: AtLimit, up: u8, down: u8) -> Accumulation {
    Accumulation {
        up,
        down,
        moves,
        at_limit,
        ..Accumulation::default()
    }
}

#[test]
fn row_mode_moves_only_its_own_row_as_the_manuals_example() {
    let song = manuals("acu:01", Accumulation::default());
    assert_eq!(
        notes(&song, 16),
        named("C D E F  D D E F  E D E F  F D E F")
    );
}

#[test]
fn track_mode_moves_every_row_as_the_manuals_example() {
    let track = with(Moves::Track, AtLimit::Wrap, 7, 7);
    let song = manuals("acu:01", track);
    assert_eq!(
        notes(&song, 16),
        named("C D E F  D E F G  E F G A  F G A B")
    );
}

#[test]
fn wrap_carries_on_from_the_note_as_written() {
    let song = song(&["C-4 -- acu:01"], with(Moves::Row, AtLimit::Wrap, 3, 7));
    assert_eq!(notes(&song, 9), named("C D E F C D E F C"));
}

#[test]
fn bounce_turns_back_at_the_limit_and_again_at_the_note() {
    let song = song(&["C-4 -- acu:01"], with(Moves::Row, AtLimit::Bounce, 3, 7));
    assert_eq!(notes(&song, 9), named("C D E F E D C D E"));
}

#[test]
fn hold_stays_at_the_limit() {
    let song = song(&["C-4 -- acu:01"], with(Moves::Row, AtLimit::Hold, 3, 7));
    assert_eq!(notes(&song, 6), named("C D E F F F"));
}

#[test]
fn a_negative_amount_goes_down_to_the_lower_limit() {
    let song = song(&["C-4 -- acu:FF"], with(Moves::Row, AtLimit::Wrap, 7, 2));
    assert_eq!(notes(&song, 5), named("C B3 A3 C B3"));
}

#[test]
fn degrees_with_the_scale_on_and_semitones_with_it_off() {
    let mut chromatic = song(&["C-4 -- acu:01"], Accumulation::default());
    chromatic.scale.on = false;
    assert_eq!(notes(&chromatic, 4), named("C C# D D#"));
    // Two degrees a time in C major goes through the thirds.
    let thirds = song(&["C-4 -- acu:02"], Accumulation::default());
    assert_eq!(notes(&thirds, 4), named("C E G B"));
}

#[test]
fn a_note_outside_the_scale_lands_on_the_next_one_in() {
    let song = song(&["C#4 -- acu:01"], Accumulation::default());
    assert_eq!(notes(&song, 3), named("C# D E"));
}

#[test]
fn the_scale_changed_while_playing_is_heard_from_the_next_note() {
    let song = song(&["C-4 -- acu:01"], Accumulation::default());
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut out = vec![0.0; 3 * STEP * 2];
    engine.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    assert_eq!(note_at(&left, 2 * STEP), Some(64), "two degrees up, E");
    engine.set_scale(Scale {
        on: false,
        ..C_MAJOR
    });
    engine.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    assert_eq!(note_at(&left, 0), Some(63), "three semitones up, D#");
}

#[test]
fn a_row_skipped_by_a_condition_or_a_chance_doesnt_add() {
    let condition = song(&["C-4 -- cnd:12 acu:01"], Accumulation::default());
    assert_eq!(notes(&condition, 7), named("C . D . E . F"));
    // With a chance, each note that plays is one more than the one before.
    let mut chance = song(&["C-4 -- cha:80 acu:01"], Accumulation::default());
    chance.scale.on = false;
    let heard: Vec<i32> = notes(&chance, 32).into_iter().flatten().collect();
    assert!(heard.len() > 4 && heard.len() < 28, "{heard:?}");
    let expected: Vec<i32> = (0..heard.len() as i32)
        .map(|played| C4 + played % 8)
        .collect();
    assert_eq!(heard, expected);
}

#[test]
fn a_resting_row_doesnt_add() {
    let song = song(
        &["C-4 -- acu:01", "C-4 -- RST1 acu:01"],
        Accumulation::default(),
    );
    assert_eq!(notes(&song, 6), named("C . D . E ."));
}

#[test]
fn multi_adds_on_each_pulse_only_when_set_to() {
    let rows = ["C-4 -- MLT4 acu:01"];
    let per_row = song(&rows, Accumulation::default());
    assert_eq!(notes(&per_row, 8), named("C C C C D D D D"));
    let per_pulse = song(
        &rows,
        Accumulation {
            adds: Adds::Pulse,
            ..Accumulation::default()
        },
    );
    assert_eq!(notes(&per_pulse, 8), named("C D E F G A B C5"));
    // A `SGL` lasting the same adds once a row, either way.
    let single = song(
        &["C-4 -- SGL4 acu:01"],
        Accumulation {
            adds: Adds::Pulse,
            ..Accumulation::default()
        },
    );
    let out = render(&single, 8);
    assert_eq!(note_at(&out, 0), Some(60));
    assert_eq!(note_at(&out, 4 * STEP), Some(62));
}

#[test]
fn a_retrigger_adds_on_each_hit_only_when_set_to() {
    // `RET 03` hits every half step, so twice a row.
    let rows = ["C-4 -- ret:03 acu:01"];
    let hits = |accumulation| {
        let out = render(&song(&rows, accumulation), 3);
        (0..6)
            .map(|hit| note_at(&out, hit * STEP / 2))
            .collect::<Vec<_>>()
    };
    assert_eq!(hits(Accumulation::default()), named("C C D D E E"));
    let each_hit = Accumulation {
        adds: Adds::Retrig,
        ..Accumulation::default()
    };
    assert_eq!(hits(each_hit), named("C D E F G A"));
}

#[test]
fn every_row_with_an_acu_adds_to_the_tracks_in_track_mode() {
    let rows = ["C-4 -- acu:01", "C-4 --", "C-4 -- acu:02", "C-4 --"];
    let song = song(&rows, with(Moves::Track, AtLimit::Hold, 28, 7));
    // Each row's first play adds nothing, and after that, one and then two more each pass.
    assert_eq!(notes(&song, 12), named("C C C C  D D F F  G G B B"));
}

#[test]
fn the_chain_starting_again_starts_them_again() {
    let mut reset = song(&["C-4 -- acu:01"], Accumulation::default());
    reset.sections[0].reset = Some(3);
    assert_eq!(notes(&reset, 7), named("C D E C D E C"));
}

#[test]
fn reset_on_play_carries_on_past_the_chain_starting_again() {
    let mut song = song(
        &["C-4 -- acu:01"],
        Accumulation {
            reset: Reset::Play,
            ..Accumulation::default()
        },
    );
    song.sections[0].reset = Some(3);
    assert_eq!(notes(&song, 7), named("C D E F G A B"));
}

#[test]
fn reset_every_so_many_loops_counts_the_chains_loops() {
    let loops = |loops| Accumulation {
        reset: Reset::Loops(loops),
        ..Accumulation::default()
    };
    let two = song(&["C-4 -- acu:01", "--- --"], loops(2));
    assert_eq!(notes(&two, 10), named("C . D . C . D . C ."));
    // A phrase in its chain twice is round the chain once for each two times the row plays.
    let mut twice = song(&["C-4 -- acu:01"], loops(1));
    let entry = ChainEntry {
        phrase: 0,
        transpose: 0,
    };
    twice.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![entry, entry],
            label: String::new(),
        }),
    );
    assert_eq!(notes(&twice, 6), named("C D C D C D"));
}

#[test]
fn a_phrase_played_twice_in_its_chain_moves_on_each_time() {
    let mut twice = song(&["C-4 -- acu:01"], Accumulation::default());
    let entry = ChainEntry {
        phrase: 0,
        transpose: 0,
    };
    twice.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![entry, entry],
            label: String::new(),
        }),
    );
    assert_eq!(notes(&twice, 4), named("C D E F"));
}

#[test]
fn the_chains_transpose_goes_on_after() {
    let mut song = song(&["C-4 -- acu:01"], Accumulation::default());
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 0,
                transpose: 1,
            }],
            label: String::new(),
        }),
    );
    // C, then D, each a semitone up.
    assert_eq!(notes(&song, 2), named("C# D#"));
}

#[test]
fn playing_from_a_step_starts_them_from_the_notes_as_written() {
    // The phrase twice in its chain, so its row plays at steps 0 and 4.
    let rows = ["C-4 -- acu:01", "--- --", "--- --", "--- --"];
    let mut song = song(&rows, Accumulation::default());
    let entry = ChainEntry {
        phrase: 0,
        transpose: 0,
    };
    song.tracks[0].chains.insert(
        0,
        Arc::new(Chain {
            entries: vec![entry, entry],
            label: String::new(),
        }),
    );
    let mut engine = Engine::new(RATE, version(&song));
    let mut play = |from: usize| {
        engine.play_from(from);
        let mut out = vec![0.0; 2 * STEP * 2];
        engine.process(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
        note_at(&left, 0)
    };
    assert_eq!(play(0), Some(60));
    let mut from_top = Engine::new(RATE, version(&song));
    from_top.play_from(0);
    let mut out = vec![0.0; 6 * STEP * 2];
    from_top.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    assert_eq!(
        note_at(&left, 4 * STEP),
        Some(62),
        "playing through, it moves on"
    );
    assert_eq!(play(4), Some(60), "played from step 4, it's as written");
}

#[test]
fn where_its_heard_says_the_note_it_moved_to() {
    let song = song(&["C-4 -- acu:02", "D-4 --"], Accumulation::default());
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP / 2 * 2];
    engine.process(&mut out);
    let mut heard = vec![engine.track_heard(0).accumulated];
    let mut out = vec![0.0; STEP * 2];
    for _ in 0..3 {
        engine.process(&mut out);
        heard.push(engine.track_heard(0).accumulated);
    }
    // Halfway through each row: C as written, D, then E for the first row moved on.
    assert_eq!(heard, [None, None, Some(64), None]);
}

#[test]
fn a_value_past_28_does_nothing() {
    let song = song(&["C-4 -- acu:40"], Accumulation::default());
    assert_eq!(notes(&song, 3), named("C C C"));
}

#[test]
fn renders_with_accumulators_repeat() {
    let rows = [
        "C-4 -- cha:80 acu:01",
        "E-4 -- MLT2 acu:FE",
        "G-4 -- ret:02 acu:03",
        "C-4 -- cnd:13",
    ];
    for moves in [Moves::Row, Moves::Track] {
        for at_limit in AtLimit::ALL {
            for adds in Adds::ALL {
                let accumulation = Accumulation {
                    moves,
                    at_limit,
                    adds,
                    ..Accumulation::default()
                };
                let song = song(&rows, accumulation);
                let render = || render_frames(version(&song), RATE, 24 * STEP);
                assert_eq!(render(), render(), "{moves:?} {at_limit:?} {adds:?}");
            }
        }
    }
}
