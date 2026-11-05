use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Instrument, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 1000 per groove unit.
const STEP: usize = 6000;
const UNIT: usize = 1000;

fn pool(frames: Vec<[f32; 2]>) -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn flat(level: f32) -> Arc<SamplePool> {
    pool(vec![[level, level]; RATE as usize * 4])
}

/// A song with one track playing these step texts, and instant envelopes.
fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.instruments.insert(
        0,
        Arc::new(Instrument {
            sample: Some("s".to_owned()),
            attack_ms: 0,
            release_ms: 0,
            ..Instrument::default()
        }),
    );
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

fn render(song: Song, samples: Arc<SamplePool>, frames: usize) -> Vec<[f32; 2]> {
    let version = Arc::new(Published::new(Arc::new(song), samples));
    let out = render_frames(version, RATE, frames);
    out.as_chunks::<2>().0.to_vec()
}

fn level_at(out: &[[f32; 2]], frame: usize) -> f32 {
    (out[frame][0].abs() + out[frame][1].abs()) / 2.0
}

#[test]
fn vol_changes_track_gain_and_holds() {
    let out = render(
        song(&[
            "C-4 -- 00 ---00 ---00 ---00",
            "--- -- -- VOL40 ---00 ---00",
            "--- -- -- ---00 ---00 ---00",
        ]),
        flat(0.5),
        STEP * 3,
    );
    let full = level_at(&out, STEP - 1);
    let quiet = level_at(&out, STEP * 2 + 2000);
    let still = level_at(&out, STEP * 3 - 1);
    assert!(quiet < full * 0.4, "{quiet} vs {full}");
    assert!((still - quiet).abs() < 1e-3, "VOL should hold");
}

#[test]
fn vol_changes_are_smoothed() {
    let out = render(
        song(&["C-4 -- 00 ---00 ---00 ---00", "--- -- -- VOL00 ---00 ---00"]),
        flat(0.5),
        STEP * 2,
    );
    let before = level_at(&out, STEP - 1);
    let next = level_at(&out, STEP + 1);
    assert!(
        next > before * 0.9,
        "should ramp, not jump: {next} vs {before}"
    );
    assert!(level_at(&out, STEP + 4000) < 0.001);
}

#[test]
fn pan_moves_between_channels() {
    let out = render(song(&["C-4 -- 00 PAN00 ---00 ---00"]), flat(0.5), STEP);
    let last = out[STEP - 1];
    assert!(last[0] > 0.3 && last[1].abs() < 1e-3, "{last:?}");
    let out = render(song(&["C-4 -- 00 PANFF ---00 ---00"]), flat(0.5), STEP);
    let last = out[STEP - 1];
    assert!(last[1] > 0.3 && last[0].abs() < 1e-3, "{last:?}");
    let out = render(song(&["C-4 -- 00 PAN80 ---00 ---00"]), flat(0.5), STEP);
    let last = out[STEP - 1];
    assert!((last[0] - last[1]).abs() < 1e-5, "{last:?}");
}

#[test]
fn cut_filters_high_frequencies() {
    let tone: Vec<[f32; 2]> = (0..RATE as usize)
        .map(|i| {
            let value = (i as f32 * 8000.0 / RATE as f32 * std::f32::consts::TAU).sin() * 0.5;
            [value, value]
        })
        .collect();
    let open = render(
        song(&["C-4 -- 00 ---00 ---00 ---00"]),
        pool(tone.clone()),
        STEP,
    );
    let closed = render(song(&["C-4 -- 00 CUT40 ---00 ---00"]), pool(tone), STEP);
    let peak = |out: &[[f32; 2]]| out[2000..].iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert!(
        peak(&closed) < peak(&open) * 0.1,
        "{} vs {}",
        peak(&closed),
        peak(&open)
    );
}

#[test]
fn resonance_raises_a_tone_at_the_cutoff() {
    // CUT80 is about 632 Hz, so test a tone there.
    let tone: Vec<[f32; 2]> = (0..RATE as usize)
        .map(|i| {
            let value = (i as f32 * 632.0 / RATE as f32 * std::f32::consts::TAU).sin() * 0.1;
            [value, value]
        })
        .collect();
    let flat_response = render(
        song(&["C-4 -- 00 CUT80 RES00 ---00"]),
        pool(tone.clone()),
        STEP,
    );
    let resonant = render(song(&["C-4 -- 00 CUT80 RESFF ---00"]), pool(tone), STEP);
    let peak = |out: &[[f32; 2]]| out[3000..].iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert!(peak(&resonant) > peak(&flat_response) * 2.0);
    assert!(peak(&resonant).is_finite());
}

#[test]
fn del_delays_the_note_by_groove_units() {
    let out = render(song(&["C-4 -- 00 DEL02 ---00 ---00"]), flat(0.5), STEP);
    let first = out.iter().position(|f| f[0] != 0.0);
    assert_eq!(first, Some(UNIT * 2));
}

#[test]
fn kil_cuts_after_groove_units_counted_from_a_delayed_start() {
    let out = render(song(&["C-4 -- 00 KIL03 ---00 ---00"]), flat(0.5), STEP);
    assert_ne!(out[UNIT * 3 - 1][0], 0.0);
    assert_eq!(out[UNIT * 3 + 100][0], 0.0);
    let out = render(song(&["C-4 -- 00 DEL02 KIL01 ---00"]), flat(0.5), STEP);
    assert_ne!(out[UNIT * 2 + 100][0], 0.0);
    assert_eq!(out[UNIT * 3 + 100][0], 0.0);
}

#[test]
fn kil_on_a_step_with_no_note_cuts_the_playing_voice() {
    let out = render(
        song(&["C-4 -- 00 ---00 ---00 ---00", "--- -- -- KIL01 ---00 ---00"]),
        flat(0.5),
        STEP * 2,
    );
    assert_ne!(out[STEP + 500][0], 0.0);
    assert_eq!(out[STEP + UNIT + 100][0], 0.0);
}

#[test]
fn off_starts_the_instruments_release() {
    let mut with_release = song(&["C-4 -- 00 OFF01 ---00 ---00"]);
    with_release.instruments.insert(
        0,
        Arc::new(Instrument {
            sample: Some("s".to_owned()),
            attack_ms: 0,
            release_ms: 20,
            ..Instrument::default()
        }),
    );
    let out = render(with_release, flat(0.5), STEP);
    let at_off = level_at(&out, UNIT - 1);
    let during = level_at(&out, UNIT + 480);
    assert!(during < at_off && during > 0.0, "{during} vs {at_off}");
    assert_eq!(level_at(&out, UNIT + 1200), 0.0);
}

#[test]
fn a_new_note_cancels_a_pending_cut() {
    let out = render(
        song(&["C-4 -- 00 KILFF ---00 ---00", "C-4 -- 00 ---00 ---00 ---00"]),
        flat(0.5),
        STEP * 3,
    );
    assert_ne!(
        out[STEP * 3 - 1][0],
        0.0,
        "the second note should keep playing"
    );
}

#[test]
fn ret_retriggers_every_y_units_and_stops_at_the_next_step() {
    // A short sample, so each retrigger shows up separately.
    let out = render(
        song(&["C-4 -- 00 RET02 ---00 ---00"]),
        pool(vec![[1.0, 1.0]; 10]),
        STEP * 2,
    );
    let fired: Vec<usize> = (0..12).filter(|unit| out[unit * UNIT][0] != 0.0).collect();
    assert_eq!(fired, [0, 2, 4]);
}

#[test]
fn ret_with_y_zero_does_nothing() {
    let out = render(
        song(&["C-4 -- 00 RET10 ---00 ---00"]),
        pool(vec![[1.0, 1.0]; 10]),
        STEP,
    );
    let fired = (0..6).filter(|unit| out[unit * UNIT][0] != 0.0).count();
    assert_eq!(fired, 1);
}

#[test]
fn ret_x_changes_level_on_each_retrigger() {
    let out = render(
        song(&["C-4 -- 00 RET52 ---00 ---00"]),
        pool(vec![[0.5, 0.5]; 10]),
        STEP,
    );
    let first = out[0][0];
    let second = out[UNIT * 2][0];
    let third = out[UNIT * 4][0];
    assert!(
        second < first * 0.7 && second > first * 0.4,
        "{second} vs {first}"
    );
    assert!(third < second * 0.7, "it compounds");
}

fn fire_pattern(seed: u64, track_count: usize) -> Vec<bool> {
    let mut song = song(&vec!["C-4 -- 00 CHA80 ---00 ---00"; 16]);
    song.seed = seed;
    for _ in 1..track_count {
        song.add_track();
    }
    let out = render(song, pool(vec![[1.0, 1.0]; 10]), STEP * 16);
    (0..16).map(|step| out[step * STEP][0] != 0.0).collect()
}

#[test]
fn a_fixed_seed_gives_the_same_chance_results() {
    assert_eq!(fire_pattern(3, 1), fire_pattern(3, 1));
    assert_ne!(fire_pattern(3, 1), fire_pattern(4, 1));
    let pattern = fire_pattern(3, 1);
    assert!(pattern.contains(&true) && pattern.contains(&false));
}

#[test]
fn adding_a_track_doesnt_change_another_tracks_results() {
    assert_eq!(fire_pattern(3, 1), fire_pattern(3, 4));
}

#[test]
fn cha_limits_always_and_never_hold_and_other_fx_still_apply() {
    let out = render(song(&["C-4 -- 00 CHAFF ---00 ---00"]), flat(0.5), 100);
    assert_ne!(out[50][0], 0.0);
    let out = render(
        song(&["C-4 -- 00 ---00 ---00 ---00", "C-4 -- 00 CHA00 VOL00 ---00"]),
        flat(0.5),
        STEP * 2,
    );
    // The note is skipped, so the first one keeps sounding, but VOL still applies.
    assert!(level_at(&out, STEP * 2 - 1) < 0.001);
}

#[test]
fn chance_varies_across_loop_passes() {
    let mut song = song(&["C-4 -- 00 CHA80 ---00 ---00"]);
    let phrase = Arc::make_mut(song.phrases.get_mut(&0).unwrap());
    phrase.set_len(1);
    let out = render(song, pool(vec![[1.0, 1.0]; 10]), STEP * 32);
    let pattern: Vec<bool> = (0..32).map(|s| out[s * STEP][0] != 0.0).collect();
    assert!(pattern.contains(&true) && pattern.contains(&false));
}

#[test]
fn delay_and_reverb_sends_add_a_tail() {
    // Without sends a short hit ends, and with them it keeps sounding.
    let dry = render(
        song(&["C-4 -- 00 ---00 ---00 ---00"]),
        pool(vec![[0.8, 0.8]; 400]),
        STEP * 8,
    );
    let late = |out: &[[f32; 2]]| out[2000..].iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert_eq!(late(&dry), 0.0);
    let delayed = render(
        song(&["C-4 -- 00 DLYFF ---00 ---00"]),
        pool(vec![[0.8, 0.8]; 400]),
        STEP * 8,
    );
    assert!(late(&delayed) > 0.05, "{}", late(&delayed));
    // 3/16 of a note at 120 BPM is 18000 frames.
    assert_eq!(delayed[17_000][0], 0.0);
    assert_ne!(delayed[18_100][0], 0.0);
    let reverbed = render(
        song(&["C-4 -- 00 REVFF ---00 ---00"]),
        pool(vec![[0.8, 0.8]; 400]),
        STEP * 8,
    );
    assert!(late(&reverbed) > 0.01, "{}", late(&reverbed));
    assert!(reverbed.iter().all(|f| f[0].is_finite()));
}

#[test]
fn held_values_reset_on_return_and_keep_on_resume() {
    let samples = flat(0.5);
    let version = Arc::new(Published::new(
        Arc::new(song(&["C-4 -- 00 VOL00 ---00 ---00"])),
        samples,
    ));
    let mut engine = Engine::new(RATE, Arc::clone(&version));
    engine.play_from(0);
    let mut out = vec![0.0; 2 * 4000];
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "VOL00 applies");
    // Resuming keeps the held value.
    engine.pause_or_resume();
    engine.pause_or_resume();
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001);
    // Going back to the top resets it, so a step with no VOL plays at full level.
    engine.return_to_start();
    let plain = Arc::new(Published::new(
        Arc::new(song(&["C-4 -- 00 ---00 ---00 ---00"])),
        flat(0.5),
    ));
    engine.swap(plain);
    engine.play_from(0);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() > 0.1);
}

#[test]
fn starting_from_the_top_resets_held_values_and_repeats_chance() {
    let version = Arc::new(Published::new(
        Arc::new(song(&[
            "C-4 -- 00 VOL00 ---00 ---00",
            "C-4 -- 00 ---00 ---00 ---00",
        ])),
        flat(0.5),
    ));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(1);
    let mut out = vec![0.0; 2 * 4000];
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() > 0.1, "from step 1 no VOL has fired");
    engine.play_or_stop(0);
    engine.play_from(0);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "VOL00 applies at the top");
    engine.play_from(1);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "not from the top, so it holds");
    engine.play_from(0);
    let plain = Arc::new(Published::new(
        Arc::new(song(&["C-4 -- 00 ---00 ---00 ---00"])),
        flat(0.5),
    ));
    engine.swap(plain);
    engine.play_from(0);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() > 0.1, "the top resets it");
}
