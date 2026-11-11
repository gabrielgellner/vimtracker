use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::envelope::Adsr;
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{FilterMode, FilterType, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Frames in a millisecond.
const MS: usize = 48;

fn pool(frames: Vec<[f32; 2]>) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn flat() -> Arc<SamplePool> {
    pool(vec![[1.0, 1.0]; RATE as usize * 10])
}

/// Every frame alternating, which a low-pass filter clearly turns down.
fn buzz() -> Arc<SamplePool> {
    let wave = |frame: usize| {
        let value = if frame.is_multiple_of(2) { 0.5 } else { -0.5 };
        [value, value]
    };
    pool((0..RATE as usize * 4).map(wave).collect())
}

/// A rising line, so where playback is reading from shows in the output.
fn ramp() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 4)
        .map(|frame| {
            let value = frame as f32 / (RATE as f32 * 4.0);
            [value, value]
        })
        .collect();
    pool(frames)
}

fn song(texts: &[&str], shape: ModSlot) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song.tracks[0].mods[0] = shape;
    song
}

/// An ADSR on `Vol`, which in the first slot shapes each note.
fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> ModSlot {
    let shape = Adsr {
        attack_ms,
        decay_ms,
        sustain,
        release_ms,
        ..Adsr::note_shape()
    };
    ModSlot::adsr(Some(SettingId::VOL), shape)
}

/// The envelope slots a new track has, before its LFOs.
const ENVELOPE_SLOTS: usize = 3;

fn render(song: Song, samples: Arc<SamplePool>, frames: usize) -> Vec<f32> {
    render_frames(
        Arc::new(Published::new(Arc::new(song), samples)),
        RATE,
        frames,
    )
}

fn left(out: &[f32], frame: usize) -> f32 {
    out[frame * 2]
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.01, "{actual} and {expected}");
}

const NOTE: &str = "C-3 -- --- --- ---";
const OFF: &str = "OFF -- --- --- ---";
const EMPTY: &str = "--- -- --- --- ---";

#[test]
fn a_note_follows_attack_decay_and_sustain() {
    let out = render(song(&[NOTE], adsr(10, 20, 50, 0)), flat(), STEP);
    assert_eq!(left(&out, 0), 0.0);
    assert!(left(&out, 5 * MS) < 0.2, "the attack starts slowly");
    close(left(&out, 10 * MS), 1.0);
    let during = left(&out, 20 * MS);
    assert!(during > 0.5 && during < 0.75, "{during}");
    close(left(&out, 30 * MS), 0.5);
    close(left(&out, 100 * MS), 0.5);
}

#[test]
fn a_decay_to_nothing_ends_the_voice() {
    let out = render(song(&[NOTE], adsr(0, 50, 0, 0)), flat(), STEP);
    close(left(&out, 0), 1.0);
    assert_eq!(left(&out, 50 * MS + 2), 0.0);
    assert_eq!(left(&out, STEP - 1), 0.0);
}

#[test]
fn a_note_off_runs_the_release_from_where_it_is() {
    let texts = [NOTE, OFF];
    let out = render(song(&texts, adsr(0, 0, 50, 40)), flat(), STEP * 2);
    close(left(&out, STEP - 1), 0.5);
    let during = left(&out, STEP + 20 * MS);
    assert!(during > 0.0 && during < 0.2, "{during}");
    assert_eq!(left(&out, STEP + 41 * MS), 0.0);

    // Mid-attack, it falls from wherever it got to.
    let out = render(song(&texts, adsr(1000, 0, 100, 40)), flat(), STEP * 2);
    let at_off = left(&out, STEP - 1);
    assert!(at_off < 0.5);
    assert!(left(&out, STEP + 2) <= at_off);
}

#[test]
fn without_an_off_a_note_holds_until_the_next_and_release_never_runs() {
    let texts = [NOTE, EMPTY, EMPTY, NOTE];
    let out = render(song(&texts, adsr(0, 0, 100, 2000)), flat(), STEP * 4);
    close(left(&out, STEP * 3 - 1), 1.0);
    close(left(&out, STEP * 3 + 100), 1.0);
}

#[test]
fn reset_off_carries_on_from_the_level_it_had() {
    let mut carry = adsr(50, 0, 100, 0);
    let texts = [NOTE, NOTE];
    let reset = render(song(&texts, carry), flat(), STEP + 20 * MS);
    carry.adsr.reset = false;
    let kept = render(song(&texts, carry), flat(), STEP + 20 * MS);
    // A step is 125 ms, so the first note has finished its attack.
    close(left(&kept, STEP + 10), 1.0);
    assert!(left(&reset, STEP + 10 * MS) < 0.3);
}

#[test]
fn turning_the_first_envelope_off_vol_leaves_notes_their_plain_shape() {
    let off = ModSlot {
        destination: None,
        ..adsr(0, 50, 0, 0)
    };
    let out = render(song(&[NOTE], off), flat(), STEP);
    close(left(&out, STEP - 1), 1.0);
    assert!(left(&out, 0) < 0.1, "the plain shape's short attack");
}

#[test]
fn an_envelope_moves_the_filter_with_its_depth() {
    let loudness = |out: &[f32], at: usize| -> f32 {
        (at..at + 200)
            .map(|frame| left(out, frame).abs())
            .sum::<f32>()
            / 200.0
    };
    // Every envelope after the first, which shapes each note, does the same.
    for slot in 1..ENVELOPE_SLOTS {
        let plain = |depth: i16| {
            let mut with = song(&[NOTE], adsr(0, 0, 100, 0));
            with.tracks[0].cutoff = 0x40;
            with.tracks[0].mods[slot] = ModSlot {
                destination: Some(SettingId::CUT),
                depth,
                ..adsr(0, 40, 0, 0)
            };
            render(with, buzz(), STEP)
        };
        let still = plain(0);
        let moved = plain(0xBF);
        assert!(
            loudness(&moved, 2 * MS) > loudness(&still, 2 * MS) * 3.0,
            "Env {}'s filter opens",
            slot + 1
        );
        let settled = 80 * MS;
        let ratio = loudness(&moved, settled) / loudness(&still, settled);
        assert!((ratio - 1.0).abs() < 0.05, "and closes again: {ratio}");
    }
}

#[test]
fn the_envelopes_after_the_second_start_with_nowhere_to_go_and_change_nothing() {
    let plain = song(&[NOTE], adsr(5, 20, 50, 10));
    let envelopes = &plain.tracks[0].mods[..ENVELOPE_SLOTS];
    assert!(envelopes[2..].iter().all(|e| e.destination.is_none()));
    let mut with = plain.clone();
    with.tracks[0].mods[ENVELOPE_SLOTS - 1].depth = 0x40;
    assert_eq!(
        render(plain, flat(), STEP),
        render(with, flat(), STEP),
        "a depth with no Dest does nothing"
    );
}

#[test]
fn what_an_envelope_moves_stays_fast() {
    // Pan shares the filter's path and can be read exactly. The first note starts where it should,
    // so it's the second that shows smoothing, arriving with the envelope back at nothing and a voice
    // still sounding.
    let mut with = song(&[NOTE, NOTE], adsr(0, 0, 100, 0));
    with.tracks[0].mods[1] = ModSlot {
        destination: Some(SettingId::PAN),
        depth: 0x7F,
        ..adsr(0, 40, 0, 0)
    };
    let out = render(with, flat(), STEP * 2);
    let right_share = |frame: usize| {
        let (left, right) = (out[frame * 2], out[frame * 2 + 1]);
        right / (left + right)
    };
    // 10 ms of smoothing would have moved only a quarter of the way by now.
    let (first, second) = (right_share(3 * MS), right_share(STEP + 3 * MS));
    assert!(first > 0.8, "{first}");
    assert!((first - second).abs() < 0.05, "{first} and {second}");
}

#[test]
fn a_pitch_envelope_changes_speed_and_settles() {
    // An octave up to begin with reads the ramp at twice the speed.
    let up = |depth: i16| {
        let mut with = song(&[NOTE], adsr(0, 0, 100, 0));
        with.tracks[0].mods[1] = ModSlot {
            destination: Some(SettingId::PITCH),
            depth,
            ..adsr(0, 0, 100, 0)
        };
        render(with, ramp(), STEP)
    };
    let (still, octave) = (up(0), up(12));
    let speed =
        (left(&octave, 2000) - left(&octave, 1000)) / (left(&still, 2000) - left(&still, 1000));
    assert!((speed - 2.0).abs() < 0.05, "{speed}");
    let down = up(-12);
    let speed = (left(&down, 2000) - left(&down, 1000)) / (left(&still, 2000) - left(&still, 1000));
    assert!((speed - 0.5).abs() < 0.05, "{speed}");

    // A decay brings it back to where it started.
    let mut falling = song(&[NOTE], adsr(0, 0, 100, 0));
    falling.tracks[0].mods[1] = ModSlot {
        destination: Some(SettingId::PITCH),
        depth: 24,
        ..adsr(0, 20, 0, 0)
    };
    let out = render(falling, ramp(), STEP);
    let late = 60 * MS;
    let speed = (left(&out, late + 1000) - left(&out, late))
        / (left(&still, late + 1000) - left(&still, late));
    assert!((speed - 1.0).abs() < 0.05, "{speed}");
}

#[test]
fn a_second_envelope_on_vol_moves_the_level_by_its_depth() {
    let mut with = song(&["C-3 -- vol:80 --- ---"], adsr(0, 0, 100, 0));
    with.tracks[0].mods[1] = ModSlot {
        destination: Some(SettingId::VOL),
        depth: 0x40,
        ..adsr(0, 0, 100, 0)
    };
    let out = render(with, flat(), STEP);
    close(left(&out, STEP - 1), vt_model::song::level_gain(192.0));
}

#[test]
fn depth_zero_and_off_do_nothing() {
    let plain = render(song(&[NOTE], adsr(5, 20, 50, 10)), flat(), STEP);
    let mut off = song(&[NOTE], adsr(5, 20, 50, 10));
    off.tracks[0].mods[1].destination = None;
    assert_eq!(plain, render(off, flat(), STEP));
}

#[test]
fn a_tracks_filter_type_and_drive_change_what_it_sounds_like() {
    let with = |filter, drive| {
        let mut note = song(&[NOTE], adsr(0, 0, 100, 0));
        note.tracks[0].filter = filter;
        note.tracks[0].drive = drive;
        note.tracks[0].cutoff = 0x80;
        render(note, buzz(), STEP)
    };
    let level = |out: &[f32]| -> f32 {
        (1000..5000)
            .map(|frame| left(out, frame).abs())
            .sum::<f32>()
            / 4000.0
    };
    let low = with(FilterType::LowPass, 0);
    let high = with(FilterType::HighPass, 0);
    assert!(level(&high) > level(&low) * 20.0, "a buzz is all highs");
    assert_ne!(low, with(FilterType::LowPass, 0x80));
    assert_ne!(low, with(FilterType::Ladder, 0));
    // The same again gives the same, and every frame is sound.
    let ladder = with(FilterType::Ladder, 0xFF);
    assert_eq!(ladder, with(FilterType::Ladder, 0xFF));
    assert!(ladder.iter().all(|sample| sample.is_finite()));
}

#[test]
fn a_tracks_filter_mode_and_a_lock_on_it_are_heard() {
    let with = |rows: &[&str], filter, mode| {
        let mut note = song(rows, adsr(0, 0, 100, 0));
        note.tracks[0].filter = filter;
        note.tracks[0].filter_mode = mode;
        note.tracks[0].cutoff = 0x80;
        render(note, buzz(), STEP)
    };
    let level = |out: &[f32]| -> f32 {
        (1000..5000)
            .map(|frame| left(out, frame).abs())
            .sum::<f32>()
            / 4000.0
    };
    let low = with(&[NOTE], FilterType::Ladder, FilterMode::LowPass24);
    let high = with(&[NOTE], FilterType::Ladder, FilterMode::HighPass24);
    assert!(level(&high) > level(&low) * 20.0, "a buzz is all highs");
    // A step's lock plays its mode for the step, gliding there from the track's.
    let locked = with(
        &["C-3 -- mode:06 --- ---"],
        FilterType::Ladder,
        FilterMode::LowPass24,
    );
    assert!(
        (level(&locked) / level(&high) - 1.0).abs() < 0.05,
        "{} and {}",
        level(&locked),
        level(&high)
    );
    // A type that doesn't offer the track's mode plays its first, and one with none plays as ever.
    assert_eq!(
        with(&[NOTE], FilterType::LowPass, FilterMode::HighPass24),
        with(&[NOTE], FilterType::LowPass, FilterMode::Slope12)
    );
    assert_eq!(
        with(&[NOTE], FilterType::Ms20LowPass, FilterMode::Slope24),
        with(&[NOTE], FilterType::Ms20LowPass, FilterMode::Slope12)
    );
    assert_ne!(
        with(&[NOTE], FilterType::LowPass, FilterMode::Slope24),
        with(&[NOTE], FilterType::LowPass, FilterMode::Slope12)
    );
}

#[test]
fn a_changed_filter_type_is_heard_while_playing() {
    let start = song(&[NOTE], adsr(0, 0, 100, 0));
    let samples = buzz();
    let mut changed = start.clone();
    changed.tracks[0].cutoff = 0x20;
    changed.tracks[0].filter = FilterType::HighPass;
    let mut engine = Engine::new(
        RATE,
        Arc::new(Published::new(Arc::new(start), Arc::clone(&samples))),
    );
    engine.play_from(0);
    let mut out = vec![0.0; 4000 * 2];
    engine.process(&mut out);
    engine.swap(Arc::new(Published::new(Arc::new(changed), samples)));
    engine.process(&mut out);
    assert!(out.iter().all(|sample| sample.is_finite()));
    let level = out.iter().map(|sample| sample.abs()).sum::<f32>();
    assert!(level > 1000.0, "{level}");
}

#[test]
fn the_utility_filter_is_off_until_set_and_works_on_top_of_the_main_one() {
    let with = |low_cut, high_cut, filter| {
        let mut note = song(&[NOTE], adsr(0, 0, 100, 0));
        note.tracks[0].low_cut = low_cut;
        note.tracks[0].high_cut = high_cut;
        note.tracks[0].filter = filter;
        render(note, buzz(), STEP)
    };
    let level = |out: &[f32]| -> f32 {
        (1000..5000)
            .map(|frame| left(out, frame).abs())
            .sum::<f32>()
            / 4000.0
    };
    let plain = render(song(&[NOTE], adsr(0, 0, 100, 0)), buzz(), STEP);
    assert_eq!(
        plain,
        with(0, 0xFF, FilterType::LowPass),
        "off changes nothing"
    );
    assert!(level(&with(0, 0x60, FilterType::LowPass)) < level(&plain) * 0.1);
    // A buzz is all highs, so a low cut leaves it, and on flat DC it takes everything.
    assert!(level(&with(0x80, 0xFF, FilterType::LowPass)) > level(&plain) * 0.9);
    let mut dc = song(&[NOTE], adsr(0, 0, 100, 0));
    dc.tracks[0].low_cut = 0x80;
    let out = render(dc, flat(), STEP);
    assert!(
        left(&out, STEP - 1).abs() < 0.01,
        "{}",
        left(&out, STEP - 1)
    );
    // It sits on the ladder too.
    let ladder = with(0x20, 0xC0, FilterType::Ladder);
    assert!(ladder.iter().all(|sample| sample.is_finite()));
}

#[test]
fn turning_on_the_high_cut_while_playing_does_not_jump() {
    let start = song(&[NOTE], adsr(0, 0, 100, 0));
    let samples = flat();
    let mut cut = start.clone();
    cut.tracks[0].high_cut = 0xC0;
    let mut engine = Engine::new(
        RATE,
        Arc::new(Published::new(Arc::new(start), Arc::clone(&samples))),
    );
    engine.play_from(0);
    let mut out = vec![0.0; 2000 * 2];
    engine.process(&mut out);
    let before = out[out.len() - 2];
    engine.swap(Arc::new(Published::new(Arc::new(cut), samples)));
    let mut after = vec![0.0; 2000 * 2];
    engine.process(&mut after);
    close(after[0], before);
    let biggest = after
        .as_chunks::<2>()
        .0
        .windows(2)
        .map(|pair| (pair[1][0] - pair[0][0]).abs())
        .fold(0.0, f32::max);
    assert!(biggest < 0.01, "jumped {biggest}");
}
