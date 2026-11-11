use std::sync::Arc;

use vt_engine::engine::{Engine, Transport};
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::insert::{Distort, DistortType, INSERTS, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Song, level_gain};

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
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
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
            "C-3 -- --- --- ---",
            "--- -- vol:40 --- ---",
            "--- -- --- --- ---",
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
        song(&["C-3 -- --- --- ---", "--- -- vol:00 --- ---"]),
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
    let out = render(song(&["C-3 -- pan:00 --- ---"]), flat(0.5), STEP);
    let last = out[STEP - 1];
    assert!(last[0] > 0.3 && last[1].abs() < 1e-3, "{last:?}");
    let out = render(song(&["C-3 -- pan:FF --- ---"]), flat(0.5), STEP);
    let last = out[STEP - 1];
    assert!(last[1] > 0.3 && last[0].abs() < 1e-3, "{last:?}");
    let out = render(song(&["C-3 -- pan:80 --- ---"]), flat(0.5), STEP);
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
    let open = render(song(&["C-3 -- --- --- ---"]), pool(tone.clone()), STEP);
    let closed = render(song(&["C-3 -- cut:40 --- ---"]), pool(tone), STEP);
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
    // cut:80 is about 632 Hz, so test a tone there.
    let tone: Vec<[f32; 2]> = (0..RATE as usize)
        .map(|i| {
            let value = (i as f32 * 632.0 / RATE as f32 * std::f32::consts::TAU).sin() * 0.1;
            [value, value]
        })
        .collect();
    let flat_response = render(
        song(&["C-3 -- cut:80 res:00 ---"]),
        pool(tone.clone()),
        STEP,
    );
    let resonant = render(song(&["C-3 -- cut:80 res:FF ---"]), pool(tone), STEP);
    let peak = |out: &[[f32; 2]]| out[3000..].iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert!(peak(&resonant) > peak(&flat_response) * 2.0);
    assert!(peak(&resonant).is_finite());
}

#[test]
fn del_delays_the_note_by_groove_units() {
    let out = render(song(&["C-3 -- del:02 --- ---"]), flat(0.5), STEP);
    let first = out.iter().position(|f| f[0] != 0.0);
    assert_eq!(first, Some(UNIT * 2));
}

#[test]
fn kil_cuts_after_groove_units_counted_from_a_delayed_start() {
    let out = render(song(&["C-3 -- kil:03 --- ---"]), flat(0.5), STEP);
    assert_ne!(out[UNIT * 3 - 1][0], 0.0);
    assert_eq!(out[UNIT * 3 + 100][0], 0.0);
    let out = render(song(&["C-3 -- del:02 kil:01 ---"]), flat(0.5), STEP);
    assert_ne!(out[UNIT * 2 + 100][0], 0.0);
    assert_eq!(out[UNIT * 3 + 100][0], 0.0);
}

#[test]
fn kil_on_a_step_with_no_note_cuts_the_playing_voice() {
    let out = render(
        song(&["C-3 -- --- --- ---", "--- -- kil:01 --- ---"]),
        flat(0.5),
        STEP * 2,
    );
    assert_ne!(out[STEP + 500][0], 0.0);
    assert_eq!(out[STEP + UNIT + 100][0], 0.0);
}

#[test]
fn off_starts_the_notes_release() {
    let mut with_release = song(&["C-3 -- off:01 --- ---"]);
    with_release.tracks[0].envelopes[0].release_ms = 20;
    let out = render(with_release, flat(0.5), STEP);
    let at_off = level_at(&out, UNIT - 1);
    let during = level_at(&out, UNIT + 480);
    assert!(during < at_off && during > 0.0, "{during} vs {at_off}");
    assert_eq!(level_at(&out, UNIT + 1200), 0.0);
}

#[test]
fn a_new_note_cancels_a_pending_cut() {
    let out = render(
        song(&["C-3 -- kil:FF --- ---", "C-3 -- --- --- ---"]),
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
        song(&["C-3 -- ret:02 --- ---"]),
        pool(vec![[1.0, 1.0]; 10]),
        STEP * 2,
    );
    let fired: Vec<usize> = (0..12).filter(|unit| out[unit * UNIT][0] != 0.0).collect();
    assert_eq!(fired, [0, 2, 4]);
}

#[test]
fn ret_with_y_zero_does_nothing() {
    let out = render(
        song(&["C-3 -- ret:10 --- ---"]),
        pool(vec![[1.0, 1.0]; 10]),
        STEP,
    );
    let fired = (0..6).filter(|unit| out[unit * UNIT][0] != 0.0).count();
    assert_eq!(fired, 1);
}

#[test]
fn ret_x_changes_level_on_each_retrigger() {
    let out = render(
        song(&["C-3 -- ret:52 --- ---"]),
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
    let mut song = song(&vec!["C-3 -- cha:80 --- ---"; 16]);
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
    let out = render(song(&["C-3 -- cha:FF --- ---"]), flat(0.5), 100);
    assert_ne!(out[50][0], 0.0);
    let out = render(
        song(&["C-3 -- --- --- ---", "C-3 -- cha:00 vol:00 ---"]),
        flat(0.5),
        STEP * 2,
    );
    // The note is skipped, so the first one keeps sounding, but VOL still applies.
    assert!(level_at(&out, STEP * 2 - 1) < 0.001);
}

#[test]
fn chance_varies_across_loop_passes() {
    let mut song = song(&["C-3 -- cha:80 --- ---"]);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_len(1);
    let out = render(song, pool(vec![[1.0, 1.0]; 10]), STEP * 32);
    let pattern: Vec<bool> = (0..32).map(|s| out[s * STEP][0] != 0.0).collect();
    assert!(pattern.contains(&true) && pattern.contains(&false));
}

#[test]
fn delay_and_reverb_sends_add_a_tail() {
    // Without sends a short hit ends, and with them it keeps sounding.
    let dry = render(
        song(&["C-3 -- --- --- ---"]),
        pool(vec![[0.8, 0.8]; 400]),
        STEP * 8,
    );
    let late = |out: &[[f32; 2]]| out[2000..].iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert_eq!(late(&dry), 0.0);
    let delayed = render(
        song(&["C-3 -- sna:F8 --- ---"]),
        pool(vec![[0.8, 0.8]; 400]),
        STEP * 8,
    );
    assert!(late(&delayed) > 0.05, "{}", late(&delayed));
    // 3/16 of a note at 120 BPM is 18000 frames.
    assert_eq!(delayed[17_000][0], 0.0);
    assert_ne!(delayed[18_100][0], 0.0);
    let reverbed = render(
        song(&["C-3 -- snb:F8 --- ---"]),
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
        Arc::new(song(&["C-3 -- vol:00 --- ---"])),
        samples,
    ));
    let mut engine = Engine::new(RATE, Arc::clone(&version));
    engine.play_from(0);
    let mut out = vec![0.0; 2 * 4000];
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "vol:00 applies");
    // Resuming keeps the held value.
    engine.pause_or_resume();
    engine.pause_or_resume();
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001);
    // Going back to the top resets it, so a step with no VOL plays at full level.
    engine.return_to_start();
    let plain = Arc::new(Published::new(
        Arc::new(song(&["C-3 -- --- --- ---"])),
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
        Arc::new(song(&["C-3 -- vol:00h --- ---", "C-3 -- --- --- ---"])),
        flat(0.5),
    ));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(1);
    let mut out = vec![0.0; 2 * 4000];
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() > 0.1, "from step 1 no VOL has fired");
    engine.play_from(0);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "vol:00 applies at the top");
    engine.play_from(1);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() < 0.001, "not from the top, so it holds");
    engine.play_from(0);
    let plain = Arc::new(Published::new(
        Arc::new(song(&["C-3 -- --- --- ---"])),
        flat(0.5),
    ));
    engine.swap(plain);
    engine.play_from(0);
    engine.process(&mut out);
    assert!(out[2 * 3999].abs() > 0.1, "the top resets it");
}

#[test]
fn track_level_scales_the_track_and_multiplies_with_velocity() {
    // Quiet enough that the master limiter is doing nothing.
    let text = ["C-3 -- --- --- ---"];
    let full = render(song(&text), flat(0.02), STEP);
    let mut lower = song(&text);
    lower.tracks[0].level = 0x80;
    let halved = render(lower, flat(0.02), STEP);
    let (full, halved) = (level_at(&full, STEP - 1), level_at(&halved, STEP - 1));
    let down = level_gain(128.0);
    assert!((halved / full - down).abs() < 1e-3, "{halved} vs {full}");

    let soft = ["C-3 40 --- --- ---"];
    let mut both = song(&soft);
    both.tracks[0].level = 0x80;
    let both = level_at(&render(both, flat(0.02), STEP), STEP - 1);
    let expected = full * (64.0 / 127.0) * down;
    assert!((both / expected - 1.0).abs() < 1e-3, "{both} vs {expected}");
}

#[test]
fn track_level_also_turns_down_the_sends() {
    let text = ["C-3 -- sna:F8 snb:F8 ---"];
    let mut silent = song(&text);
    silent.tracks[0].level = 0;
    let out = render(silent, flat(0.5), STEP * 8);
    let loudest = out
        .iter()
        .map(|f| f[0].abs().max(f[1].abs()))
        .fold(0.0, f32::max);
    assert!(loudest < 1e-6, "{loudest}");
}

#[test]
fn track_level_stays_in_force_after_the_loop_returns_to_the_top() {
    let text = ["C-3 -- --- --- ---"];
    let mut quiet = song(&text);
    quiet.tracks[0].level = 0x40;
    Arc::make_mut(quiet.tracks[0].phrases.get_mut(&0).unwrap()).set_len(1);
    let out = render(quiet, flat(0.5), STEP * 3);
    let first = level_at(&out, STEP - 1);
    let later = level_at(&out, STEP * 3 - 1);
    assert!((first - later).abs() < 1e-3, "{first} vs {later}");
}

/// Two tracks with a note each on step 0, one on track 1 on step 1, and nothing on step 2.
fn two_tracks() -> Song {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.add_track();
    song.sample_on(1, "s");
    let chain = song.sections[0].chains[1].unwrap();
    let phrase = song.tracks[1].chains[&chain].entries[0].phrase;
    let steps = Arc::make_mut(song.tracks[1].phrases.get_mut(&phrase).unwrap());
    *steps.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    *steps.step_mut(1).unwrap() = "C-3 40 --- --- ---".parse().unwrap();
    song
}

fn auditioned(song: Song, level: f32, act: impl Fn(&mut Engine)) -> f32 {
    let version = Arc::new(Published::new(Arc::new(song), flat(level)));
    let mut engine = Engine::new(RATE, version);
    act(&mut engine);
    assert_ne!(engine.transport(), Transport::Playing);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    let frames = out.as_chunks::<2>().0;
    level_at(frames, STEP - 1)
}

#[test]
fn audition_sounds_every_track_on_the_row_and_stays_stopped() {
    let one = auditioned(two_tracks(), 0.02, |engine| engine.audition(1));
    let both = auditioned(two_tracks(), 0.02, |engine| engine.audition(0));
    assert!(one > 0.0);
    assert!(both > one * 2.5, "{both} vs {one}");
    assert_eq!(
        auditioned(two_tracks(), 0.02, |engine| engine.audition(2)),
        0.0,
        "an empty row is silent"
    );
}

#[test]
fn audition_uses_velocity_and_is_cut_by_the_next_one() {
    let full = auditioned(song(&["C-3 -- --- --- ---"]), 0.02, |e| e.audition(0));
    let soft = auditioned(song(&["C-3 40 --- --- ---"]), 0.02, |e| e.audition(0));
    assert!(
        (soft / full - 64.0 / 127.0).abs() < 1e-3,
        "{soft} vs {full}"
    );

    let cut = auditioned(two_tracks(), 0.02, |engine| {
        engine.audition(0);
        engine.audition(2);
    });
    assert_eq!(cut, 0.0, "an empty row silences the last audition");
}

#[test]
fn audition_respects_mute_and_solo() {
    let silent = auditioned(two_tracks(), 0.02, |engine| {
        engine.set_mute(1, true);
        engine.audition(1);
    });
    assert_eq!(silent, 0.0);
    let soloed = auditioned(two_tracks(), 0.02, |engine| {
        engine.set_solo(0, true);
        engine.audition(1);
    });
    assert_eq!(soloed, 0.0);
}

#[test]
fn audition_does_not_run_fx_or_change_later_playback() {
    let text = ["C-3 -- vol:00 --- ---", "--- -- --- --- ---"];
    let heard = auditioned(song(&text), 0.02, |engine| engine.audition(0));
    assert!(heard > 0.0, "vol:00 did not run");

    let play = |audition: bool| {
        let version = Arc::new(Published::new(
            Arc::new(song(&["C-3 -- --- --- ---"])),
            flat(0.02),
        ));
        let mut engine = Engine::new(RATE, version);
        if audition {
            engine.audition(0);
            let mut silence = vec![0.0; RATE as usize * 2 * 8];
            engine.process(&mut silence);
        }
        engine.play_from(0);
        let mut out = vec![0.0; STEP * 2];
        engine.process(&mut out);
        out
    };
    let before = play(false);
    let after = play(true);
    let peak = |out: &[f32]| out.iter().fold(0.0_f32, |a, b| a.max(b.abs()));
    assert!((peak(&before) - peak(&after)).abs() < 1e-6);
}

#[test]
fn audition_does_nothing_while_playing() {
    let version = Arc::new(Published::new(Arc::new(two_tracks()), flat(0.02)));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(2);
    engine.audition(0);
    let mut out = vec![0.0; 2000];
    engine.process(&mut out);
    assert!(out.iter().all(|sample| *sample == 0.0));
}

#[test]
fn a_preview_goes_through_its_tracks_sound() {
    let hear = |change: fn(&mut Song)| {
        let mut start = song(&[]);
        change(&mut start);
        let version = Arc::new(Published::new(Arc::new(start), flat(0.5)));
        let mut engine = Engine::new(RATE, version);
        engine.preview(0, 60, 0x7F);
        let mut out = vec![0.0; 2000 * 2];
        engine.process(&mut out);
        level_at(out.as_chunks::<2>().0, 1999)
    };
    let open = hear(|_| {});
    assert!(open > 0.1, "{open}");
    let quiet = hear(|song| song.tracks[0].level = 0x80);
    assert!((quiet - open * level_gain(128.0)).abs() < 0.001, "{quiet}");
    let panned = hear(|song| song.tracks[0].pan = 0x00);
    assert!(
        (panned - open * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
        "{panned}"
    );
    let no_sample =
        hear(|song| song.tracks[0].set_setting(vt_model::settings::SettingId::SAMPLE, 0));
    assert_eq!(no_sample, 0.0);
}

/// A sine through these in the first slots, and the rest empty.
fn distorted(inserts: &[Insert]) -> Vec<[f32; 2]> {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].inserts[..inserts.len()].copy_from_slice(inserts);
    let sine = (0..9600).map(|frame| {
        let t = frame as f32 / RATE as f32;
        [(t * 220.0 * std::f32::consts::TAU).sin() * 0.5; 2]
    });
    render(song, pool(sine.collect()), 4800)
}

/// A distortion going as hard as `amount` says in its mode: its drive, or fewer of `Crush`'s
/// bits, from 12 at `00` to 2 at `FF`.
fn distort(kind: DistortType, amount: u8) -> Insert {
    Insert::Distort(Distort {
        kind,
        amount,
        bits: 12 - (f32::from(amount) / 25.5).round() as u8,
        mix: 0xFF,
        ..Distort::default()
    })
}

#[test]
fn empty_inserts_change_nothing_and_an_effect_changes_the_sound() {
    let plain = distorted(&[]);
    assert!(plain.iter().any(|frame| frame[0] != 0.0));
    let crushed = distorted(&[
        distort(DistortType::Crush, 0xFF),
        Insert::Empty,
        Insert::Empty,
    ]);
    assert_ne!(plain, crushed);
    // Whichever slot it's in, alone it does the same.
    let mut last = [Insert::Empty; INSERTS];
    last[INSERTS - 1] = distort(DistortType::Crush, 0xFF);
    let later = distorted(&last);
    assert_eq!(crushed, later);
}

#[test]
fn inserts_run_in_order() {
    let hard = distort(DistortType::Hard, 0xFF);
    let crush = distort(DistortType::Crush, 0xFF);
    let distinct = |out: &[[f32; 2]]| {
        let mut seen: Vec<u32> = out.iter().map(|frame| frame[0].to_bits()).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };
    let crushed_last = distorted(&[hard, crush, Insert::Empty]);
    let crushed_first = distorted(&[crush, hard, Insert::Empty]);
    assert_ne!(crushed_last, crushed_first);
    // Crushing last leaves only its steps, scaled by the level after it.
    assert!(
        distinct(&crushed_last) < distinct(&crushed_first),
        "{} and {}",
        distinct(&crushed_last),
        distinct(&crushed_first)
    );
}

#[test]
fn a_chorus_and_an_ensemble_each_change_the_sound_and_spread_it() {
    use vt_model::insert::{Chorus, ChorusType};
    let plain = distorted(&[]);
    let chorus = |kind| {
        Insert::Chorus(Chorus {
            kind,
            depth: 0xFF,
            ..Chorus::DEFAULT
        })
    };
    let one = distorted(&[chorus(ChorusType::Chorus), Insert::Empty, Insert::Empty]);
    let other = distorted(&[chorus(ChorusType::Ensemble), Insert::Empty, Insert::Empty]);
    assert_ne!(one, plain);
    assert_ne!(one, other);
    for out in [&one, &other] {
        assert!(out.iter().any(|[l, r]| (l - r).abs() > 0.01), "it spreads");
        assert!(
            out.iter()
                .flatten()
                .all(|s| s.is_finite() && s.abs() <= 1.0)
        );
    }
}

#[test]
fn a_flat_eq_changes_nothing_and_a_boost_does() {
    use vt_model::insert::Equaliser;
    let plain = distorted(&[]);
    let flat = distorted(&[Insert::Eq(Equaliser::DEFAULT), Insert::Empty, Insert::Empty]);
    assert_eq!(flat, plain, "a flat band mixes back exactly what went in");
    let boosted = Insert::Eq(Equaliser {
        mid_frequency: 0x6E,
        mid: 0xFF,
        ..Equaliser::DEFAULT
    });
    let loud = distorted(&[boosted, Insert::Empty, Insert::Empty]);
    let peak = |out: &[[f32; 2]]| out.iter().fold(0.0_f32, |most, f| most.max(f[0].abs()));
    assert!(
        peak(&loud) > peak(&plain) * 2.0,
        "{} {}",
        peak(&loud),
        peak(&plain)
    );
}

/// Two tracks holding a note, the first muted, and the second through a compressor in its first
/// slot.
fn ducked(compressor: Insert) -> Vec<[f32; 2]> {
    let mut song = Song::new(2);
    for track in 0..2 {
        song.sample_on(track, "s");
        song.tracks[track].envelopes[0].attack_ms = 0;
        song.tracks[track].envelopes[0].release_ms = 0;
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    }
    song.tracks[1].inserts[0] = compressor;
    let mut engine = Engine::new(RATE, Arc::new(Published::new(Arc::new(song), flat(0.5))));
    engine.set_mute(0, true);
    engine.play_from(0);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    out.as_chunks::<2>().0.to_vec()
}

#[test]
fn a_compressor_hearing_a_muted_track_ducks_under_it() {
    use vt_model::insert::Compressor;
    let hard = |source| {
        Insert::Compressor(Compressor {
            threshold: 0x00,
            ratio: 0xFF,
            attack: 0x00,
            source,
            ..Compressor::DEFAULT
        })
    };
    let plain = ducked(Insert::Empty);
    let under = ducked(hard(1));
    let at = STEP / 2;
    assert!(level_at(&plain, at) > 0.1, "{}", level_at(&plain, at));
    // Track 01 is muted, but it's heard after its slots, so it still ducks the second track.
    assert!(
        level_at(&under, at) < level_at(&plain, at) * 0.05,
        "{} {}",
        level_at(&under, at),
        level_at(&plain, at)
    );
    // A source that's silent leaves it alone: track 64 has nothing on it.
    let silent = ducked(hard(64));
    assert_eq!(silent, plain);
}

#[test]
fn a_bypassed_slot_lets_the_sound_through_as_it_was() {
    let quieter = |bypassed: bool| {
        let mut song = song(&["C-3 -- --- --- ---"]);
        song.tracks[0].inserts[0] = Insert::Distort(Distort {
            gain: 0x80,
            ..Distort::DEFAULT
        });
        song.tracks[0].bypassed[0] = bypassed;
        level_at(&render(song, flat(0.25), STEP), STEP - 1)
    };
    let plain = level_at(
        &render(song(&["C-3 -- --- --- ---"]), flat(0.25), STEP),
        STEP - 1,
    );
    assert!(quieter(false) < plain * 0.2, "{} {plain}", quieter(false));
    assert!(
        (quieter(true) - plain).abs() < 1e-4,
        "{} {plain}",
        quieter(true)
    );
}

#[test]
fn a_limiter_delays_a_quiet_sound_and_holds_a_loud_one_under_its_ceiling() {
    use vt_model::insert::{Limiter, ceiling_db};
    let peak = |out: &[[f32; 2]]| {
        out.iter()
            .flatten()
            .fold(0.0_f32, |most, s| most.max(s.abs()))
    };
    let plain = distorted(&[]);
    // At its defaults, a sound at half level comes through as it was, 2 ms late.
    let quiet = distorted(&[
        Insert::Limit(Limiter::DEFAULT),
        Insert::Empty,
        Insert::Empty,
    ]);
    let late = RATE as usize / 500;
    assert!(quiet[..late].iter().flatten().all(|s| *s == 0.0));
    for (frame, (got, was)) in quiet[late..].iter().zip(&plain).enumerate() {
        for side in 0..2 {
            assert!((got[side] - was[side]).abs() < 1e-5, "{frame}");
        }
    }
    // Driven as hard as it goes into the lowest ceiling, it comes out no louder than that.
    let hard = Limiter {
        input: 0xFF,
        ceiling: 0x00,
        ..Limiter::DEFAULT
    };
    let loud = distorted(&[Insert::Limit(hard), Insert::Empty, Insert::Empty]);
    // What the slot let out, as the track's level and pan scale both the same.
    let ceiling = 10_f32.powf(ceiling_db(0.0) / 20.0);
    let most = peak(&plain) / 0.5 * ceiling;
    assert!(peak(&loud) <= most * 1.0001, "{} {most}", peak(&loud));
    assert!(peak(&loud) > most * 0.9, "{} {most}", peak(&loud));
}

/// The sound through one slot, which must change it, spread it, and keep it finite and bounded.
fn changes_and_spreads(insert: Insert) -> Vec<[f32; 2]> {
    let plain = distorted(&[]);
    let out = distorted(&[insert, Insert::Empty, Insert::Empty]);
    assert_ne!(out, plain);
    // Each side sweeps its own way, so even a sound in the middle comes out wide.
    assert!(out.iter().any(|[l, r]| (l - r).abs() > 0.01), "it spreads");
    assert!(
        out.iter()
            .flatten()
            .all(|s| s.is_finite() && s.abs() <= 1.0)
    );
    out
}

#[test]
fn a_phaser_changes_the_sound_and_spreads_it_even_at_full_feedback() {
    use vt_model::insert::Phaser;
    let gentle = changes_and_spreads(Insert::Phaser(Phaser::DEFAULT));
    let fed_back = changes_and_spreads(Insert::Phaser(Phaser {
        feedback: 0xFF,
        stages: 2,
        ..Phaser::DEFAULT
    }));
    assert_ne!(gentle, fed_back);
}

#[test]
fn a_flanger_changes_the_sound_and_spreads_it_with_feedback_either_way() {
    use vt_model::insert::Flanger;
    let up = changes_and_spreads(Insert::Flanger(Flanger {
        feedback: 0xFF,
        ..Flanger::DEFAULT
    }));
    let down = changes_and_spreads(Insert::Flanger(Flanger {
        feedback: 0x00,
        ..Flanger::DEFAULT
    }));
    assert_ne!(up, down);
}

#[test]
fn a_tape_colours_the_sound_and_drive_holds_it_under_its_clip() {
    use vt_model::insert::Tape;
    let peak = |out: &[[f32; 2]]| out.iter().flatten().fold(0.0_f32, |a, b| a.max(b.abs()));
    let plain = distorted(&[]);
    let gentle = distorted(&[Insert::Tape(Tape::DEFAULT), Insert::Empty, Insert::Empty]);
    assert_ne!(gentle, plain);
    // As it starts, it's about as loud as it was.
    assert!(
        (peak(&gentle) / peak(&plain) - 1.0).abs() < 0.1,
        "{} {}",
        peak(&gentle),
        peak(&plain)
    );
    // Driven hard, it clips under -0.5 dB of what went in at full scale, which is a half here.
    let driven = distorted(&[
        Insert::Tape(Tape {
            drive: 0xFF,
            ..Tape::DEFAULT
        }),
        Insert::Empty,
        Insert::Empty,
    ]);
    assert!(driven.iter().flatten().all(|s| s.is_finite()));
    let most = peak(&plain) / 0.5 * 0.94;
    assert!(peak(&driven) <= most * 1.001, "{} {most}", peak(&driven));
    assert!(peak(&driven) > peak(&plain) * 1.5, "{}", peak(&driven));
}

#[test]
fn a_plate_rings_on_after_a_short_hit_wide_and_dying_away() {
    use vt_model::insert::Plate;
    let hit = |inserts: &[Insert]| {
        let mut song = song(&["C-3 -- --- --- ---"]);
        song.tracks[0].inserts[..inserts.len()].copy_from_slice(inserts);
        render(song, pool(vec![[0.8, 0.8]; 400]), STEP * 8)
    };
    let loudest = |out: &[[f32; 2]]| out.iter().flatten().fold(0.0_f32, |a, b| a.max(b.abs()));
    let dry = hit(&[]);
    assert_eq!(loudest(&dry[2000..]), 0.0);
    let plate = Insert::Plate(Plate {
        mix: 0xFF,
        ..Plate::DEFAULT
    });
    let wet = hit(&[plate, Insert::Empty, Insert::Empty]);
    assert!(wet.iter().flatten().all(|s| s.is_finite()));
    let (early, late) = (loudest(&wet[2000..STEP]), loudest(&wet[STEP * 7..]));
    assert!(early > 0.01, "{early}");
    assert!(late < early * 0.1, "{early} {late}");
    assert!(
        wet[2000..].iter().any(|[l, r]| (l - r).abs() > 0.01),
        "it spreads"
    );
}

#[test]
fn a_pitch_shifter_up_twelve_plays_an_octave_up_and_down_twelve_an_octave_down() {
    use vt_model::insert::{MOST_SHIFT, Pitch, grain_ms};
    // A tone that fits its window a whole number of times, so the grains jump back by whole cycles
    // and its frequency can be counted.
    let window = grain_ms(f32::from(Pitch::DEFAULT.size)) / 1000.0;
    let tone = 22.0 / window;
    let played = |shift: Option<i8>| {
        let mut song = song(&["C-3 -- --- --- ---"]);
        if let Some(semitones) = shift {
            song.tracks[0].inserts[0] = Insert::Pitch(Pitch {
                shift: (MOST_SHIFT as i8 + semitones) as u8,
                ..Pitch::DEFAULT
            });
        }
        let sine = (0..RATE as usize * 2).map(|frame| {
            let t = frame as f32 / RATE as f32;
            [(t * tone * std::f32::consts::TAU).sin() * 0.5; 2]
        });
        render(song, pool(sine.collect()), STEP * 8)
    };
    // How often it goes from a little below zero to a little above, in hertz, once it's going.
    let hertz = |out: &[[f32; 2]]| {
        let (mut below, mut ups) = (false, 0);
        for frame in &out[STEP..] {
            if frame[0] < -0.05 {
                below = true;
            } else if frame[0] > 0.05 && below {
                below = false;
                ups += 1;
            }
        }
        ups as f32 * RATE as f32 / (out.len() - STEP) as f32
    };
    let dry = hertz(&played(None));
    let (unshifted, up, down) = (
        hertz(&played(Some(0))),
        hertz(&played(Some(12))),
        hertz(&played(Some(-12))),
    );
    assert!((unshifted / dry - 1.0).abs() < 0.01, "{dry} {unshifted}");
    assert!((up / dry - 2.0).abs() < 0.02, "{dry} {up}");
    assert!((down / dry - 0.5).abs() < 0.005, "{dry} {down}");
    assert!(played(Some(12)).iter().flatten().all(|s| s.is_finite()));
}

#[test]
fn granular_frozen_keeps_playing_a_tone_after_it_stops_and_free_goes_quiet() {
    use vt_model::insert::Grains;
    use vt_model::settings::{Effect, SettingId};
    use vt_model::step::{FxCommand, FxSlot, Mark};
    // Half a second of a tone, then nothing.
    let tone = (0..RATE as usize * 4).map(|frame| {
        let t = frame as f32 / RATE as f32;
        let level = if t < 0.5 {
            (t * 330.0 * std::f32::consts::TAU).sin() * 0.5
        } else {
            0.0
        };
        [level; 2]
    });
    let samples = pool(tone.collect());
    let played = |freeze: bool| {
        let mut song = song(&["C-3 -- --- --- ---"]);
        // Reading three quarters of a second back, so a second in it's in the middle of the tone.
        song.tracks[0].inserts[0] = Insert::Grains(Grains {
            time: 0x30,
            ..Grains::DEFAULT
        });
        // Long enough not to come round and play the tone again.
        Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap()).set_len(64);
        if freeze {
            // Frozen a second in, a while after the tone's stopped coming in.
            let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
            phrase.step_mut(8).unwrap().fx = vec![FxSlot {
                command: Some(FxCommand::Set(
                    SettingId::slot(0, Effect::Grains, 4).unwrap(),
                )),
                value: 0x0100,
                mark: Mark::Hold,
            }];
        }
        render(song, Arc::clone(&samples), RATE as usize * 4)
    };
    let loudness = |out: &[[f32; 2]]| {
        let energy: f32 = out.iter().flatten().map(|s| s * s).sum();
        (energy / out.len() as f32 / 2.0).sqrt()
    };
    let (frozen, free) = (played(true), played(false));
    assert!(frozen.iter().flatten().all(|s| s.is_finite()));
    // In its last second, long after the tone, the frozen one still plays it and the other's
    // silent.
    let last = RATE as usize * 3..;
    assert!(
        loudness(&frozen[last.clone()]) > 0.05,
        "{}",
        loudness(&frozen[last.clone()])
    );
    assert!(
        loudness(&free[last.clone()]) < 1e-4,
        "{}",
        loudness(&free[last])
    );
    // While the tone plays, both are the same.
    assert_eq!(frozen[..RATE as usize / 2], free[..RATE as usize / 2]);
}

/// How strong a frequency is in the left side of some frames.
fn strength(out: &[[f32; 2]], hertz: f32) -> f32 {
    let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
    for (frame, sample) in out.iter().enumerate() {
        let phase =
            f64::from(frame as u32) / f64::from(RATE) * f64::from(hertz) * std::f64::consts::TAU;
        real += f64::from(sample[0]) * phase.cos();
        imaginary += f64::from(sample[0]) * phase.sin();
    }
    ((real * real + imaginary * imaginary).sqrt() * 2.0 / out.len() as f64) as f32
}

#[test]
fn warps_ring_modulates_one_track_by_another_thats_muted() {
    use vt_model::insert::Warps;
    // A 300 Hz tone, which the first track plays at `C-3` and the second a tritone up, so none of
    // what comes out lands on either tone.
    let tone: Vec<[f32; 2]> = (0..RATE as usize * 4)
        .map(|frame| [(frame as f32 / RATE as f32 * 300.0 * std::f32::consts::TAU).sin() * 0.5; 2])
        .collect();
    let samples = pool(tone);
    let tritone = 300.0 * std::f32::consts::SQRT_2;
    let played = |source| {
        let mut song = Song::new(2);
        for (track, note) in [(0, "C-3"), (1, "F#3")] {
            song.sample_on(track, "s");
            song.tracks[track].envelopes[0].attack_ms = 0;
            song.tracks[track].envelopes[0].release_ms = 0;
            let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
            *phrase.step_mut(0).unwrap() = format!("{note} -- --- --- ---").parse().unwrap();
        }
        // The digital ring modulation, with the first track as the modulator.
        song.tracks[1].inserts[0] = Insert::Warps(Warps {
            timbre: 0x00,
            source,
            ..Warps::DEFAULT
        });
        let version = Arc::new(Published::new(Arc::new(song), Arc::clone(&samples)));
        let mut engine = Engine::new(RATE, version);
        engine.set_mute(0, true);
        engine.play_from(0);
        let mut out = vec![0.0; STEP * 8];
        engine.process(&mut out);
        // A tenth of a second in, once it's going.
        out.as_chunks::<2>().0[RATE as usize / 10..].to_vec()
    };
    let ringing = played(1);
    assert!(ringing.iter().flatten().all(|s| s.is_finite()));
    // Track 01 is muted, but it's heard after its slots: out comes the sum and difference, and
    // neither tone on its own.
    let sum = strength(&ringing, tritone + 300.0);
    let difference = strength(&ringing, tritone - 300.0);
    assert!(sum > 0.02 && difference > 0.02, "{sum} {difference}");
    for alone in [300.0, tritone] {
        let alone = strength(&ringing, alone);
        assert!(
            alone < sum.min(difference) * 0.05,
            "{alone} {sum} {difference}"
        );
    }
    // A source that's silent ring modulates by nothing: track 64 has nothing on it.
    let silent = played(64);
    let loudest = silent
        .iter()
        .flatten()
        .fold(0.0_f32, |most, s| most.max(s.abs()));
    assert!(loudest < 1e-3, "{loudest}");
}

#[test]
fn a_low_pass_gate_opens_for_another_track_thats_muted() {
    use vt_model::insert::LowPassGate;
    // A pad holding a note on the second track, gated by the first, which hits once and is muted.
    let played = |source| {
        let mut song = Song::new(2);
        for (track, text) in [(0, "C-3 -- kil:02 --- ---"), (1, "C-3 -- --- --- ---")] {
            song.sample_on(track, "s");
            song.tracks[track].envelopes[0].attack_ms = 0;
            song.tracks[track].envelopes[0].release_ms = 0;
            let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
            *phrase.step_mut(0).unwrap() = text.parse().unwrap();
        }
        song.tracks[1].inserts[0] = Insert::Lpg(LowPassGate {
            source,
            ..LowPassGate::DEFAULT
        });
        let mut engine = Engine::new(RATE, Arc::new(Published::new(Arc::new(song), flat(0.5))));
        engine.set_mute(0, true);
        engine.play_from(0);
        let mut out = vec![0.0; STEP * 8 * 2];
        engine.process(&mut out);
        out.as_chunks::<2>().0.to_vec()
    };
    let gated = played(1);
    assert!(gated.iter().flatten().all(|s| s.is_finite()));
    // Track 01 is muted, but it's heard after its slots: the pad opens as it hits, then closes
    // slowly once it's cut, as a vactrol does.
    let hit = level_at(&gated, UNIT);
    assert!(hit > 0.1, "{hit}");
    let after = level_at(&gated, STEP * 8 - 1);
    assert!(after < hit * 0.1, "{hit} {after}");
    // A source that's silent keeps it closed: track 64 has nothing on it.
    let silent = played(64);
    assert!(silent.iter().flatten().all(|s| *s == 0.0));
}

/// How far each slot of the first track is turning down, in dB, once a loud note has played a
/// while, and once it's stopped.
fn reduction_playing_and_after(insert: Insert, slot: usize) -> ([f32; INSERTS], [f32; INSERTS]) {
    let mut song = song(&["C-3 -- --- --- ---", "OFF -- --- --- ---"]);
    song.tracks[0].inserts[slot] = insert;
    let version = Arc::new(Published::new(Arc::new(song), flat(0.9)));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(0);
    let mut out = vec![0.0; 512 * 2];
    for _ in 0..(STEP / 512) {
        engine.process(&mut out);
    }
    let playing = engine.live(0).reduction;
    for _ in 0..(STEP * 8 / 512) {
        engine.process(&mut out);
    }
    (playing, engine.live(0).reduction)
}

#[test]
fn a_compressor_reports_how_far_its_turning_down() {
    use vt_engine::dsp::compressor::curve_db;
    use vt_model::insert::Compressor;
    let settings = Compressor {
        threshold: 0x80,
        ratio: 0xC0,
        attack: 0x00,
        ..Compressor::DEFAULT
    };
    let (playing, after) = reduction_playing_and_after(Insert::Compressor(settings), 1);
    // A steady level going in is turned down by the static curve's difference, once settled. The
    // sample comes in at 0.9, under the track's level and filter, so only roughly that.
    let level = 20.0 * 0.9_f32.log10();
    let most = level - curve_db(level, settings);
    assert!(
        playing[1] > 3.0 && playing[1] <= most + 0.5,
        "{playing:?}, {most}"
    );
    assert_eq!([playing[0], playing[2], playing[3]], [0.0; 3]);
    assert_eq!(after, [0.0; INSERTS], "once nothing plays through it");
}

#[test]
fn a_limiter_reports_how_far_its_turning_down() {
    use vt_model::insert::Limiter;
    let settings = Limiter {
        input: 0xFF,
        ..Limiter::DEFAULT
    };
    let (playing, _) = reduction_playing_and_after(Insert::Limit(settings), 0);
    // 24 dB in, and a level near full: most of that is taken off again.
    assert!(playing[0] > 15.0 && playing[0] < 30.0, "{playing:?}");
}

#[test]
fn a_resonator_rings_at_the_note_the_track_plays() {
    use vt_model::insert::Resonator;
    // Clicks, every tenth of a second, played up a fifth at `G-3`.
    let clicks: Vec<[f32; 2]> = (0..RATE as usize * 4)
        .map(|frame| [if frame % 4800 == 0 { 0.8 } else { 0.0 }; 2])
        .collect();
    let played = |reso: bool| {
        let mut song = song(&["G-3 -- --- --- ---"]);
        if reso {
            song.tracks[0].inserts[0] = Insert::Reso(Resonator {
                bright: 0x00,
                ..Resonator::DEFAULT
            });
        }
        render(song, pool(clicks.clone()), STEP * 4)
    };
    let rung = played(true);
    assert!(
        rung.iter()
            .flatten()
            .all(|s| s.is_finite() && s.abs() <= 1.0)
    );
    let g = 440.0 * (-2.0_f32 / 12.0).exp2();
    let (at_note, below) = (strength(&rung, g), strength(&rung, g * 0.75));
    assert!(at_note > below * 10.0, "{at_note} {below}");
    // The clicks alone have nothing in particular at the note.
    let plain = played(false);
    assert!(
        at_note > strength(&plain, g) * 5.0,
        "{at_note} {}",
        strength(&plain, g)
    );
}

#[test]
fn a_comb_rings_at_the_note_the_track_plays() {
    use vt_model::insert::Comb;
    // Clicks, every tenth of a second, played up a fifth at `G-3`.
    let clicks: Vec<[f32; 2]> = (0..RATE as usize * 4)
        .map(|frame| [if frame % 4800 == 0 { 0.8 } else { 0.0 }; 2])
        .collect();
    let played = |comb: bool| {
        let mut song = song(&["G-3 -- --- --- ---"]);
        if comb {
            song.tracks[0].inserts[0] = Insert::Comb(Comb {
                feedback: 0xFC,
                tone: 0x20,
                ..Comb::DEFAULT
            });
        }
        render(song, pool(clicks.clone()), STEP * 4)
    };
    let rung = played(true);
    assert!(
        rung.iter()
            .flatten()
            .all(|s| s.is_finite() && s.abs() <= 1.0)
    );
    let g = 440.0 * (-2.0_f32 / 12.0).exp2();
    let (at_note, below) = (strength(&rung, g), strength(&rung, g * 0.75));
    assert!(at_note > below * 10.0, "{at_note} {below}");
    // The clicks alone have nothing in particular at the note.
    let plain = played(false);
    assert!(
        at_note > strength(&plain, g) * 5.0,
        "{at_note} {}",
        strength(&plain, g)
    );
}

#[test]
fn a_vowel_filter_says_its_vowel_whatever_the_note() {
    use vt_model::insert::Vowel;
    // White noise, played at `C-3` and an octave up, through `A` and through `I`.
    let mut seed = 1_u32;
    let noise: Vec<[f32; 2]> = (0..RATE as usize * 4)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            [(seed >> 8) as f32 / 8_388_608.0 - 1.0; 2]
        })
        .collect();
    let played = |vowel: u8, note: &str| {
        let mut song = song(&[&format!("{note} -- --- --- ---")]);
        song.tracks[0].inserts[0] = Insert::Vowel(Vowel {
            vowel,
            resonance: 0xC0,
            ..Vowel::DEFAULT
        });
        render(song, pool(noise.clone()), STEP * 4)
    };
    // How strong it is around a frequency, across a quarter of a semitone either side.
    let around = |out: &[[f32; 2]], hertz: f32| {
        (-5..=5)
            .map(|step| strength(out, hertz * (step as f32 / 240.0).exp2()))
            .sum::<f32>()
    };
    // Their first formants, in the middle voice: `A`'s at `E-4` and `I`'s at `C-3`.
    let (a_first, i_first) = (659.26, 261.63);
    for note in ["C-3", "C-4"] {
        let (a, i) = (played(0x00, note), played(0x80, note));
        assert!(a.iter().flatten().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(around(&a, a_first) > around(&i, a_first) * 4.0, "{note}");
        assert!(around(&i, i_first) > around(&a, i_first) * 4.0, "{note}");
    }
}

/// A track whose first slot turns its sound well down, holding one note, with these rows.
fn bypassing(texts: &[&str]) -> Vec<[f32; 2]> {
    let mut song = song(texts);
    song.tracks[0].inserts[0] = Insert::Distort(Distort {
        gain: 0x80,
        ..Distort::DEFAULT
    });
    render(song, flat(0.25), STEP * texts.len())
}

#[test]
fn a_step_locks_a_slots_bypass_and_it_switches_without_a_click() {
    let out = bypassing(&[
        "C-3 -- --- --- ---",
        "--- -- 1.bypass:01 --- ---",
        "--- -- 1.bypass:00 --- ---",
    ]);
    let wet = level_at(&out, STEP - 1);
    let dry = 0.25;
    assert!(wet < dry * 0.2, "{wet}");
    // Switched out on the second step, and back in on the third.
    assert!((level_at(&out, 2 * STEP - 1) - dry).abs() < 1e-4);
    assert!((level_at(&out, 3 * STEP - 1) - wet).abs() < 1e-4);
    for switch in [STEP, 2 * STEP] {
        // A straight crossfade, nowhere outside the two levels, over a few ms each way.
        let around = &out[switch - 100..switch + 1000];
        for pair in around.windows(2) {
            let jump = (pair[1][0] - pair[0][0]).abs();
            assert!(jump < (dry - wet) / 100.0, "a jump of {jump} at {switch}");
        }
        for frame in around {
            assert!(
                (wet - 1e-4..=dry + 1e-4).contains(&frame[0]),
                "{}",
                frame[0]
            );
        }
        assert!(
            (level_at(&out, switch + 200) - level_at(&out, switch - 1)).abs() > (dry - wet) / 2.0,
            "it's on its way within a few ms"
        );
        let done = level_at(&out, switch + 10 * RATE as usize / 1000);
        let settled = level_at(&out, switch + STEP - 1);
        assert!(
            (done - settled).abs() < 1e-4,
            "done within 10 ms at {switch}"
        );
    }
}

#[test]
fn a_bypass_lock_lasts_its_note_and_a_held_one_stays() {
    let locked = bypassing(&[
        "C-3 -- 1.bypass:01 --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    let dry = 0.25;
    assert!(
        (level_at(&locked, STEP + 3000) - dry).abs() < 1e-4,
        "it lasts the note"
    );
    assert!(
        level_at(&locked, 2 * STEP + 3000) < dry * 0.2,
        "the next note brings it back"
    );
    let held = bypassing(&[
        "C-3 -- 1.bypass:01h --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    assert!(
        (level_at(&held, 2 * STEP + 3000) - dry).abs() < 1e-4,
        "held, it stays"
    );
}

#[test]
fn the_last_slots_bypass_is_locked_by_its_digit() {
    use vt_model::settings::SettingId;
    let last = SettingId::bypass(INSERTS - 1).unwrap();
    assert_eq!(last.key(), format!("{INSERTS}.bypass"));
    assert_eq!(last.label(), Some(format!("{INSERTS}BYP")));
    let mut song = song(&[
        "C-3 -- --- --- ---",
        &format!("--- -- {INSERTS}.bypass:01 --- ---"),
    ]);
    song.tracks[0].inserts[INSERTS - 1] = Insert::Distort(Distort {
        gain: 0x80,
        ..Distort::DEFAULT
    });
    let out = render(song, flat(0.25), STEP * 2);
    let dry = 0.25;
    assert!(level_at(&out, STEP - 1) < dry * 0.2, "the last slot plays");
    assert!(
        (level_at(&out, 2 * STEP - 1) - dry).abs() < 1e-4,
        "and the lock switches it out"
    );
}

#[test]
fn a_bypass_lock_on_an_empty_slot_does_nothing() {
    let locked = song(&["C-3 -- 1.bypass:01 --- ---"]);
    let plain = song(&["C-3 -- --- --- ---"]);
    assert_eq!(
        render(locked, flat(0.25), STEP),
        render(plain, flat(0.25), STEP)
    );
}
