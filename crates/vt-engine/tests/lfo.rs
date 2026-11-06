use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Destination, Lfo, Rate, Shape};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Song, level_gain};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// A quarter note, as ticks.
const QUARTER: u32 = 960;
/// Held `VOL 80`, which leaves room for an LFO either way.
const HELD: f32 = 128.0;
/// What `Depth 40` swings by, in level values.
const SWING: f32 = 64.0;

fn pool(frames: Vec<[f32; 2]>) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn flat() -> Arc<SamplePool> {
    pool(vec![[1.0, 1.0]; RATE as usize * 20])
}

/// A slow wave, so a jump in the output can only come from the filter.
fn slow_sine() -> Arc<SamplePool> {
    let wave = |frame: usize| {
        let at = frame as f32 / RATE as f32;
        let value = (at * 200.0 * std::f32::consts::TAU).sin() * 0.5;
        [value, value]
    };
    pool((0..RATE as usize * 8).map(wave).collect())
}

/// Every frame alternating, which a low-pass filter clearly turns down.
fn buzz() -> Arc<SamplePool> {
    let wave = |frame: usize| {
        let value = if frame.is_multiple_of(2) { 0.5 } else { -0.5 };
        [value, value]
    };
    pool((0..RATE as usize * 8).map(wave).collect())
}

fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in &mut song.tracks {
        track.sample = Some("s".to_owned());
    }
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// A song holding `VOL 80` on one long note, with this LFO moving the volume.
fn moving(lfo: Lfo) -> Song {
    let mut song = song(&["C-4 -- VOL80 ---00 ---00"]);
    song.tracks[0].lfos[0] = Lfo {
        destination: Some(Destination::Vol),
        depth: 0x40,
        ..lfo
    };
    song
}

fn version(song: Song, samples: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), samples))
}

fn render(song: Song, samples: Arc<SamplePool>, frames: usize) -> Vec<f32> {
    render_frames(version(song, samples), RATE, frames)
}

fn left(out: &[f32], frame: usize) -> f32 {
    out[frame * 2]
}

/// Within a dB and a half, since smoothing means a fast swing lags slightly at its peaks.
fn close(actual: f32, expected: f32) {
    let apart = 20.0 * (actual / expected).log10();
    assert!(apart.abs() < 1.5, "{actual} and {expected}");
}

#[test]
fn a_synced_lfo_moves_around_the_held_value_in_time() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP * 8);
    close(left(&out, STEP), level_gain(HELD + SWING));
    close(left(&out, STEP * 2), level_gain(HELD));
    close(left(&out, STEP * 3), level_gain(HELD - SWING));
    close(left(&out, STEP * 5), level_gain(HELD + SWING));
}

#[test]
fn a_held_vol_of_e0_is_exactly_unity_and_the_top_is_6_db() {
    let out = render(song(&["C-4 -- ---00 ---00 ---00"]), flat(), STEP);
    assert!((left(&out, STEP - 1) - 1.0).abs() < 1e-6);
    let out = render(song(&["C-4 -- VOLE0 ---00 ---00"]), flat(), STEP);
    assert!((left(&out, STEP - 1) - 1.0).abs() < 1e-6);
    let mut limitless = song(&["C-4 -- VOLFF ---00 ---00"]);
    limitless.limiter = false;
    let out = render(limitless, flat(), STEP);
    let top = left(&out, STEP - 1);
    assert!((20.0 * top.log10() - 6.0).abs() < 0.01, "{top}");
}

#[test]
fn a_synced_rate_follows_the_tempo_and_hertz_does_not() {
    let synced = Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let hertz = Lfo {
        rate: Rate::Hertz(100),
        ..Lfo::default()
    };
    // At 60 BPM a quarter note takes a second, so both peak at the same frame.
    let at_60 = |lfo| {
        let mut song = moving(lfo);
        song.millibpm = 60_000;
        render(song, flat(), RATE as usize)
    };
    let peak = RATE as usize / 4;
    close(left(&at_60(synced), peak), level_gain(HELD + SWING));
    close(left(&at_60(hertz), peak), level_gain(HELD + SWING));

    // At 120 BPM, a quarter note takes half the time and hertz doesn't change.
    let out = render(moving(synced), flat(), RATE as usize);
    close(left(&out, peak), level_gain(HELD));
    let out = render(moving(hertz), flat(), RATE as usize);
    close(left(&out, peak), level_gain(HELD + SWING));
}

#[test]
fn a_reset_leaves_lfos_running() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER * 3),
        ..Lfo::default()
    };
    let free = render(moving(lfo), flat(), STEP * 12);
    let mut reset = moving(lfo);
    reset.sections[0].reset = Some(1000);
    // The step holding VOL isn't reached again, so only the reset's own effect could differ.
    let reset_late = render(reset.clone(), flat(), STEP * 12);
    assert_eq!(free, reset_late);

    reset.sections[0].reset = Some(5);
    // Each reset replays the first step, which holds the same value, so the LFO's path shows.
    let out = render(reset, flat(), STEP * 12);
    for frame in [STEP * 3 + 1000, STEP * 7 + 1000, STEP * 11 + 1000] {
        close(left(&out, frame), left(&free, frame));
    }
}

#[test]
fn retrig_restarts_it_on_each_note() {
    let texts = [
        "C-4 -- VOL80h ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "--- -- ---00 ---00 ---00",
        "C-4 -- ---00 ---00 ---00",
    ];
    let ramp = |retrig| {
        let mut song = song(&texts);
        song.tracks[0].lfos[0] = Lfo {
            destination: Some(Destination::Vol),
            shape: Shape::Ramp,
            rate: Rate::Note(QUARTER * 4),
            depth: 0x40,
            retrig,
            ..Lfo::default()
        };
        render(song, flat(), STEP * 6)
    };
    let frame = STEP * 4 + 2000;
    close(left(&ramp(false), frame), level_gain(HELD - SWING * 0.5));
    close(left(&ramp(true), frame), level_gain(HELD - SWING));
}

#[test]
fn phase_and_negative_depth_move_the_wave() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER),
        phase: 90,
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP);
    close(left(&out, 100), level_gain(HELD + SWING));

    let mut flipped = moving(Lfo {
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    });
    flipped.tracks[0].lfos[0].depth = -0x40;
    let out = render(flipped, flat(), STEP * 2);
    close(left(&out, STEP), level_gain(HELD - SWING));
}

#[test]
fn two_lfos_add_up_and_no_destination_does_nothing() {
    let lfo = Lfo {
        rate: Rate::Note(QUARTER * 4),
        ..Lfo::default()
    };
    let mut both = moving(lfo);
    both.tracks[0].lfos[1] = both.tracks[0].lfos[0];
    both.tracks[0].lfos[1].depth = 0x20;
    let out = render(both, flat(), STEP * 5);
    close(left(&out, STEP * 4), level_gain(HELD + SWING * 1.5));

    let mut none = moving(lfo);
    none.tracks[0].lfos[0].destination = None;
    let out = render(none, flat(), STEP * 5);
    close(left(&out, STEP * 4), level_gain(HELD));
}

#[test]
fn a_random_wave_holds_for_a_cycle_and_follows_the_seed() {
    let lfo = Lfo {
        shape: Shape::Random,
        rate: Rate::Note(QUARTER),
        ..Lfo::default()
    };
    let out = render(moving(lfo), flat(), STEP * 12);
    close(left(&out, STEP), left(&out, STEP * 3));
    assert!((left(&out, STEP * 3) - left(&out, STEP * 7)).abs() > 0.001);
    assert_eq!(out, render(moving(lfo), flat(), STEP * 12));

    let mut reseeded = moving(lfo);
    reseeded.seed += 1;
    assert_ne!(out, render(reseeded, flat(), STEP * 12));
}

#[test]
fn block_size_does_not_change_the_result() {
    let lfo = Lfo {
        shape: Shape::Sine,
        rate: Rate::Hertz(370),
        ..Lfo::default()
    };
    let whole = render(moving(lfo), flat(), STEP * 4);
    let mut engine = Engine::new(RATE, version(moving(lfo), flat()));
    engine.play_from(0);
    let mut pieces = vec![0.0; whole.len()];
    for chunk in pieces.chunks_mut(2 * 37) {
        engine.process(chunk);
    }
    assert_eq!(whole, pieces);
}

#[test]
fn the_filter_stays_running_when_an_lfo_takes_it_to_the_top() {
    // Half of each cycle sits at the top of the range. If the filter stopped there, it would come
    // back with old state and jump.
    let mut moved = song(&["C-4 -- ---00 ---00 ---00"]);
    moved.tracks[0].cutoff = 0xFF;
    moved.tracks[0].lfos[0] = Lfo {
        destination: Some(Destination::Cut),
        rate: Rate::Note(QUARTER),
        depth: 0xC0,
        ..Lfo::default()
    };
    let out = render(moved, slow_sine(), STEP * 8);
    let biggest = out
        .as_chunks::<2>()
        .0
        .iter()
        .map(|frame| frame[0])
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max);
    assert!(biggest < 0.03, "jumped {biggest}");
}

#[test]
fn a_tracks_base_is_where_its_held_values_start() {
    let from_step = render(song(&["C-4 -- CUT40 PAN20 ---00"]), buzz(), STEP);
    let mut from_page = song(&["C-4 -- ---00 ---00 ---00"]);
    from_page.tracks[0].cutoff = 0x40;
    from_page.tracks[0].pan = 0x20;
    assert_eq!(from_step, render(from_page, buzz(), STEP));
}

#[test]
fn a_page_change_is_heard_now_and_leaves_what_a_step_set() {
    let start = song(&["C-4 -- PAN00h ---00 ---00"]);
    let samples = flat();
    let mut engine = Engine::new(RATE, version(start.clone(), Arc::clone(&samples)));
    engine.play_from(0);
    let mut out = vec![0.0; 4000 * 2];
    engine.process(&mut out);
    let left_only = out[out.len() - 2];
    assert!(out[out.len() - 1].abs() < 1e-3);

    // Changing something else leaves the step's pan alone.
    let mut other = start.clone();
    other.tracks[0].cutoff = 0xF0;
    engine.swap(version(other.clone(), Arc::clone(&samples)));
    engine.process(&mut out);
    close(out[out.len() - 2], left_only);

    // Changing the pan itself moves it, until a step sets it again.
    other.tracks[0].pan = 0xFF;
    engine.swap(version(other, Arc::clone(&samples)));
    engine.process(&mut out);
    assert!(out[out.len() - 2].abs() < 1e-3, "{}", out[out.len() - 2]);
    assert!(out[out.len() - 1] > 0.5);
}
