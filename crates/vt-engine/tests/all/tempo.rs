//! Loop to tempo (#286): under `Play`'s `Tempo`, an entry that says how many steps it lasts plays
//! round its loop at the speed that has it last that long, whatever the tempo, and never drifts
//! from the sequencer's steps. With the `Beats` warp, each of its sounds starts at its own time and
//! plays at its own speed.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::bank::{BankEntry, ContentHash, Warp};
use vt_model::constants::TICKS_PER_STEP;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{PlayMode, SettingId};
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// The loop's length: a second.
const LENGTH: usize = RATE as usize;
/// Where the hits start in `hits`, a quarter of a second apart, and how long each sounds.
const HITS: [usize; 4] = [0, 12_000, 24_000, 36_000];
const HIT: usize = 3000;

/// A second's ramp, so a frame's level says where it was read, and four hits in a second, each a
/// steady level for a while and then silence.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let ramp = (0..LENGTH)
        .map(|at| at as f32 / LENGTH as f32)
        .map(|value| [value, value])
        .collect();
    pool.insert("ramp", SampleData { frames: ramp });
    let hits = (0..LENGTH)
        .map(|at| {
            let hit = HITS.iter().any(|start| (*start..start + HIT).contains(&at));
            if hit { [0.5, 0.5] } else { [0.0, 0.0] }
        })
        .collect();
    pool.insert("hits", SampleData { frames: hits });
    Arc::new(pool)
}

/// One track playing a file's entry on `Tempo`, the entry lasting so many steps, at a tempo, with
/// a note on its first step of a phrase long enough that it isn't played again.
fn song(file: &str, steps: u32, millibpm: u32, edit: impl FnOnce(&mut BankEntry)) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.millibpm = millibpm;
    let mut entry = BankEntry::of_file(file, ContentHash::UNKNOWN);
    entry.edit.hints.set_length_steps(Some(steps));
    edit(&mut entry);
    song.bank.set(1, Some(entry));
    let track = &mut song.tracks[0];
    track.set_setting(SettingId::SAMPLE, 1);
    track.set_setting(SettingId::PLAY, PlayMode::Tempo as u8);
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    phrase.set_len(4096);
    *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    song
}

fn engine(song: Song) -> Engine {
    let mut engine = Engine::new(RATE, Arc::new(Published::new(Arc::new(song), pool())));
    engine.play_from(0);
    engine
}

/// How many frames a step lasts at a tempo.
fn step_frames(millibpm: u32) -> f64 {
    let quarter = 60.0 * f64::from(RATE) * 1000.0 / f64::from(millibpm);
    quarter * TICKS_PER_STEP as f64 / vt_model::constants::TICKS_PER_QUARTER as f64
}

/// Where the track's note is in its sample.
fn playhead(engine: &Engine) -> u64 {
    engine.playheads(0)[0].expect("the note is playing").frame
}

/// Play on so many frames, and give the left channel.
fn run(engine: &mut Engine, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out.iter().step_by(2).copied().collect()
}

/// Play on to so many frames from the note's start, and check its playhead is where a loop of
/// `loop_frames` lasting `lasts` frames has it, within a frame, counting round the loop.
fn check(engine: &mut Engine, played: &mut usize, to: usize, loop_frames: f64, lasts: f64) {
    run(engine, to - *played);
    *played = to;
    let expected = (to as f64 / lasts).fract() * loop_frames;
    let found = playhead(engine) as f64;
    let off = (found - expected).abs();
    let off = off.min(loop_frames - off);
    assert!(off <= 1.0, "at {to}: {found}, not {expected}");
}

#[test]
fn a_bar_long_loop_lasts_sixteen_steps_at_any_tempo() {
    let length = LENGTH as f64;
    for millibpm in [90_000, 120_000, 137_630, 174_000] {
        let mut engine = engine(song("ramp", 16, millibpm, |_| {}));
        let bar = 16.0 * step_frames(millibpm);
        let mut played = 0;
        // Through the first time, just before and after it comes round, and again.
        for at in [0.25, 0.999, 1.001, 1.5, 1.999, 2.001] {
            let to = (bar * at) as usize;
            check(&mut engine, &mut played, to, length, bar);
        }
    }
}

#[test]
fn one_that_says_its_tempo_and_not_its_length_plays_its_tempo_at_the_songs() {
    // Seeded at 60 BPM, as an Octatrack file says: at 60 it plays as recorded, at 120 twice as
    // fast, at 90 half as fast again.
    for millibpm in [60_000, 120_000, 90_000] {
        let mut engine = engine(song("ramp", 16, millibpm, |entry| {
            entry.edit.hints.set_length_steps(None);
            entry.edit.hints.millibpm = Some(60_000);
        }));
        let lasts = LENGTH as f64 * 60_000.0 / f64::from(millibpm);
        let mut played = 0;
        for at in [0.25, 0.999, 1.001, 1.5] {
            let to = (lasts * at) as usize;
            check(&mut engine, &mut played, to, LENGTH as f64, lasts);
        }
    }
}

#[test]
fn half_a_bar_said_in_steps_lasts_eight() {
    let mut engine = engine(song("ramp", 8, 120_000, |_| {}));
    let mut played = 0;
    // 8 steps of 6000 frames: played as it was recorded.
    for to in [12_000, 47_990, 48_010, 95_990] {
        check(&mut engine, &mut played, to, LENGTH as f64, 48_000.0);
    }
}

#[test]
fn it_never_drifts_over_64_bars() {
    // At a tempo whose steps aren't a whole number of frames.
    let millibpm = 333_330;
    let mut engine = engine(song("ramp", 16, millibpm, |_| {}));
    let bar = 16.0 * step_frames(millibpm);
    let mut played = 0;
    for at in [16.5, 32.0, 63.999, 64.001] {
        let to = (bar * at) as usize;
        check(&mut engine, &mut played, to, LENGTH as f64, bar);
    }
}

#[test]
fn moving_the_end_changes_the_speed_as_the_length_in_steps_stays() {
    let mut engine = engine(song("ramp", 16, 120_000, |entry| {
        entry.edit.points.end = Some(24_000);
    }));
    let mut played = 0;
    // Half a second of it lasts the bar's two seconds.
    for to in [4000, 95_990, 96_010, 120_000] {
        check(&mut engine, &mut played, to, 24_000.0, 96_000.0);
    }
}

#[test]
fn its_note_doesnt_change_its_speed() {
    let mut song = song("ramp", 16, 120_000, |_| {});
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-4 -- --- --- ---".parse().unwrap();
    let mut engine = engine(song);
    run(&mut engine, 48_000);
    assert!((playhead(&engine) as i64 - 24_000).abs() <= 1);
}

#[test]
fn with_no_length_it_loops_at_its_own_speed() {
    let mut engine = engine(song("ramp", 16, 120_000, |entry| {
        entry.edit.hints.set_length_steps(None);
    }));
    let mut played = 0;
    for to in [12_000, 47_990, 48_010] {
        check(&mut engine, &mut played, to, LENGTH as f64, 48_000.0);
    }
}

/// Where the hits sound in what's played: the frames where it goes from silence to sound.
fn starts(out: &[f32]) -> Vec<usize> {
    let mut found: Vec<usize> = out
        .first()
        .filter(|level| **level > 0.01)
        .map(|_| 0)
        .into_iter()
        .collect();
    for at in 1..out.len() {
        if out[at] > 0.01 && out[at - 1] <= 0.01 {
            found.push(at);
        }
    }
    found
}

/// How many frames sound after each start, before it falls silent again.
fn lengths(out: &[f32]) -> Vec<usize> {
    starts(out)
        .into_iter()
        .map(|start| {
            out[start..]
                .iter()
                .position(|level| *level <= 0.01)
                .unwrap_or(out.len() - start)
        })
        .collect()
}

#[test]
fn beats_start_each_slice_at_its_time_and_play_it_at_its_own_speed() {
    // Two seconds a bar: each hit plays as recorded, half a second apart, with a gap after it.
    let slow = song("hits", 16, 120_000, |entry| {
        entry.edit.play.warp = Warp::Beats;
        let starts: Vec<u64> = HITS.iter().map(|at| *at as u64).collect();
        entry.edit.points.reslice(&starts);
    });
    let out = run(&mut engine(slow), 96_000);
    let found = starts(&out);
    assert_eq!(found.len(), 4, "{found:?}");
    for (at, expected) in found.iter().zip([0, 24_000, 48_000, 72_000]) {
        assert!(at.abs_diff(expected) <= 4, "{found:?}");
    }
    for length in lengths(&out) {
        assert!(length.abs_diff(HIT) <= 70, "{length}");
    }
}

#[test]
fn beats_cut_a_slice_short_when_the_next_is_due_first() {
    // The whole second in a step: each hit is due 1500 frames after the last, before it has
    // finished, so each is cut into the next, and nothing plays faster.
    let fast = song("hits", 1, 120_000, |entry| {
        entry.edit.play.warp = Warp::Beats;
        let starts: Vec<u64> = HITS.iter().map(|at| *at as u64).collect();
        entry.edit.points.reslice(&starts);
    });
    let out = run(&mut engine(fast), 12_000);
    // Never louder than a hit, never a click, and sounding all the while once started.
    let through = out[100];
    assert!(through > 0.1);
    assert!(out.iter().all(|level| *level <= through + 1e-3));
    assert!(out[100..11_900].iter().all(|level| *level > through * 0.4));
}

#[test]
fn beats_with_no_slices_play_as_one_sound() {
    let one = song("hits", 16, 120_000, |entry| {
        entry.edit.play.warp = Warp::Beats;
    });
    // The whole second plays at its own speed and then there's a gap: its hits are a quarter of
    // a second apart, and the next round starts at two seconds.
    let out = run(&mut engine(one), 100_000);
    let found = starts(&out);
    assert_eq!(found.len(), 5, "{found:?}");
    assert!(found[1].abs_diff(12_000) <= 4, "{found:?}");
    assert!(found[4].abs_diff(96_000) <= 4, "{found:?}");
}

#[test]
fn a_tempo_change_while_it_plays_is_kept_to_from_there() {
    let first = song("ramp", 16, 120_000, |_| {});
    let mut faster = first.clone();
    faster.millibpm = 150_000;
    let mut engine = engine(first);
    // Half way round at 120, a second in.
    run(&mut engine, 48_000);
    assert!((playhead(&engine) as i64 - 24_000).abs() <= 1);
    engine.swap(Arc::new(Published::new(Arc::new(faster), pool())));
    // The other half lasts half a bar at 150: 38_400 frames.
    run(&mut engine, 38_390);
    let near = playhead(&engine);
    assert!(
        near.abs_diff(47_993) <= 1,
        "10 frames short, at 0.625 a frame: {near}"
    );
    run(&mut engine, 20);
    let round = playhead(&engine);
    assert!(round <= 20, "it's come round, as the bar has: {round}");
}

#[test]
fn a_reversed_loop_keeps_to_the_tempo_too() {
    let mut engine = engine(song("ramp", 16, 120_000, |entry| {
        entry.edit.play.reverse = true;
    }));
    run(&mut engine, 48_000);
    let found = playhead(&engine);
    assert!(found.abs_diff(23_999) <= 1, "half way back: {found}");
}
