//! A track's speed: how long each of its steps is, from a 32nd to a quarter with the triplets
//! between, set on the track or on a step with `RATE`. Its rows, pulses, groove, nudge, timing
//! commands and lengths all follow its steps, while sections, resets and renders count the
//! song's sixteenths.

use std::sync::Arc;

use vt_engine::engine::{Engine, Transport};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_frames, render_loop};
use vt_model::groove::{Groove, GrooveId};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Phrase, Section, SectionStart, Song};
use vt_model::speed::Speed;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 25 per tick.
const STEP: usize = 6000;
const TICK: usize = 25;
/// Below this, nothing's sounding.
const SILENT: f32 = 1e-4;
/// How near a note's end is to where it's due, with its fade out.
const NEAR: usize = 100;

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

/// A long flat sample, so a note's length shows.
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

/// Tracks playing these rows at these speeds, panned apart when there are two so each channel is
/// one track's.
fn song(tracks: &[(Speed, &[&str])]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    for (index, (speed, rows)) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.speed = *speed;
        track.mods[0].adsr.attack_ms = 0;
        track.mods[0].adsr.release_ms = 0;
        let pan = match (tracks.len(), index) {
            (1, _) => "",
            (_, 0) => " pan:00",
            _ => " pan:FF",
        };
        let steps = rows
            .iter()
            .map(|text| format!("{text}{pan}").parse().unwrap())
            .collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

/// A project playing one of its own grooves.
fn with_own(mut song: Song, groove: Groove) -> Song {
    song.grooves = vec![groove];
    song.groove = GrooveId::custom(0).unwrap();
    song
}

fn version(song: &Song, pool: Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool))
}

/// One channel of a render, for this many frames.
fn render(song: &Song, pool: Arc<SamplePool>, frames: usize, channel: usize) -> Vec<f32> {
    render_frames(version(song, pool), RATE, frames)
        .chunks(2)
        .map(|frame| frame[channel])
        .collect()
}

/// Where notes begin in one channel, for songs of short hits.
fn hits(song: &Song, frames: usize, channel: usize) -> Vec<usize> {
    let out = render(song, hits_pool(), frames, channel);
    let on = |at: usize| out[at].abs() > 0.001;
    (0..out.len())
        .filter(|&at| on(at) && (at == 0 || !on(at - 1)))
        .collect()
}

/// Where a flat note sounding from the start falls silent.
fn ends(song: &Song, frames: usize) -> usize {
    let out = render(song, flat_pool(), frames, 0);
    out.iter().position(|sample| sample.abs() < SILENT).unwrap()
}

#[test]
fn each_speed_plays_its_steps_that_long() {
    for speed in Speed::ALL {
        let length = speed.ticks() as usize * TICK;
        let played = song(&[(speed, &["C-3 --"; 4])]);
        assert_eq!(
            hits(&played, 4 * length, 0),
            [0, length, 2 * length, 3 * length],
            "{speed}"
        );
    }
}

#[test]
fn a_sixteenth_beside_a_slower_track_is_unchanged() {
    let played = song(&[
        (Speed::Sixteenth, &["C-3 --"; 8]),
        (Speed::EighthTriplet, &["C-3 --"; 6]),
    ]);
    let sixteenths: Vec<usize> = (0..8).map(|step| step * STEP).collect();
    assert_eq!(hits(&played, 8 * STEP, 0), sixteenths);
    let triplets: Vec<usize> = (0..6).map(|step| step * 320 * TICK).collect();
    assert_eq!(hits(&played, 8 * STEP, 1), triplets);
}

#[test]
fn a_track_at_an_eighth_swings_its_eighths() {
    // Swing 66 puts every other step 77 ticks of a sixteenth late, so every other eighth is 154
    // ticks of a sixteenth late, as the groove counts the track's own steps.
    let swung = with_own(
        song(&[
            (Speed::Eighth, &["C-3 --"; 4]),
            (Speed::Sixteenth, &["C-3 --"; 4]),
        ]),
        Groove::pattern(&[0, 77]),
    );
    let eighth = 2 * STEP;
    assert_eq!(
        hits(&swung, 8 * STEP, 0),
        [0, eighth + 154 * TICK, 2 * eighth, 3 * eighth + 154 * TICK]
    );
    assert_eq!(
        hits(&swung, 4 * STEP, 1),
        [0, STEP + 77 * TICK, 2 * STEP, 3 * STEP + 77 * TICK]
    );
}

#[test]
fn an_early_groove_on_a_quarter_works_its_steps_out_that_far_ahead() {
    // Half a step early at a quarter is an eighth early, so a 32nd track beside it has several of
    // its steps worked out before the one heard has landed.
    let pushed = with_own(
        song(&[
            (Speed::Quarter, &["C-3 --"; 2]),
            (Speed::ThirtySecond, &["C-3 --"; 16]),
        ]),
        Groove::pattern(&[0, -120]),
    );
    let published = Published::new(Arc::new(pushed.clone()), hits_pool());
    assert_eq!(published.lead, 480);
    assert_eq!(hits(&pushed, 8 * STEP, 0), [0, 2 * STEP]);
    let thirty_seconds = hits(&pushed, 8 * STEP, 1);
    let expected: Vec<usize> = (0..16)
        .map(|step| step * STEP / 2 - if step % 2 == 1 { STEP / 4 } else { 0 })
        .collect();
    assert_eq!(thirty_seconds, expected);
}

#[test]
fn a_rate_lock_plays_its_note_at_that_speed_until_the_next_note() {
    // `04` is an eighth. The next note ends the lock, so it's a sixteenth again.
    let locked = song(&[(
        Speed::Sixteenth,
        &["C-3 -- speed:04", "C-3 --", "C-3 --", "C-3 --"],
    )]);
    assert_eq!(
        hits(&locked, 5 * STEP, 0),
        [0, 2 * STEP, 3 * STEP, 4 * STEP]
    );
    // With no note after it, the rows after go at its speed too.
    let held_on = song(&[(
        Speed::Sixteenth,
        &["C-3 -- speed:04", "--- --", "C-3 --", "C-3 --"],
    )]);
    assert_eq!(hits(&held_on, 6 * STEP, 0), [0, 4 * STEP, 5 * STEP]);
    // A hold keeps it.
    let held = song(&[(
        Speed::Sixteenth,
        &["C-3 -- speed:04h", "C-3 --", "C-3 --", "C-3 --"],
    )]);
    assert_eq!(hits(&held, 8 * STEP, 0), [0, 2 * STEP, 4 * STEP, 6 * STEP]);
}

#[test]
fn a_step_at_a_new_speed_waits_for_that_speeds_grid() {
    // On the second sixteenth, an eighth lasts to the next eighth, so the track stays on the
    // eighths' grid.
    let locked = song(&[(
        Speed::Sixteenth,
        &["C-3 --", "C-3 -- speed:04h", "C-3 --", "C-3 --"],
    )]);
    assert_eq!(hits(&locked, 6 * STEP, 0), [0, STEP, 2 * STEP, 4 * STEP]);
    // From a triplet back to a sixteenth, the triplet's second step lasts to the next sixteenth.
    let back = song(&[(
        Speed::SixteenthTriplet,
        &["C-3 --", "C-3 -- speed:02h", "C-3 --", "C-3 --"],
    )]);
    assert_eq!(hits(&back, 3 * STEP, 0), [0, 160 * TICK, STEP, 2 * STEP]);
}

#[test]
fn a_track_page_change_of_speed_is_heard_from_the_next_step() {
    let played = song(&[(Speed::Sixteenth, &["C-3 --"; 8])]);
    let mut engine = Engine::new(RATE, version(&played, hits_pool()));
    engine.play_from(0);
    let mut out = vec![0.0; (2 * STEP + 1) * 2];
    engine.process(&mut out);
    let mut slower = played.clone();
    slower.tracks[0].speed = Speed::Eighth;
    engine.swap(version(&slower, hits_pool()));
    let mut after = vec![0.0; 6 * STEP * 2];
    engine.process(&mut after);
    out.extend(after);
    let on = |at: usize| out[at * 2].abs() > 0.001;
    let starts: Vec<usize> = (0..out.len() / 2)
        .filter(|&at| on(at) && (at == 0 || !on(at - 1)))
        .collect();
    // The fourth step lasts to the next eighth, and the track goes on in eighths.
    assert_eq!(
        starts,
        [0, STEP, 2 * STEP, 3 * STEP, 4 * STEP, 6 * STEP, 8 * STEP]
    );
}

#[test]
fn a_reset_lines_a_slower_track_up_again() {
    // Reset after 3 sixteenths: the eighth from the second has only one sixteenth before the
    // track starts again.
    let mut reset = song(&[(Speed::Eighth, &["C-3 --", "D-3 --", "E-3 --", "F-3 --"])]);
    reset.sections[0].reset = Some(3);
    assert_eq!(
        hits(&reset, 9 * STEP, 0),
        [0, 2 * STEP, 3 * STEP, 5 * STEP, 6 * STEP, 8 * STEP]
    );
}

#[test]
fn a_new_section_lines_a_slower_track_up_with_its_start() {
    // A quarter track in a section 6 sixteenths long: its second step is cut short when the next
    // section starts it on another chain.
    let mut played = song(&[(Speed::Quarter, &["C-3 --", "D-3 --"])]);
    let track = &mut played.tracks[0];
    track.chains.insert(1, Arc::clone(&track.chains[&0]));
    played.sections = vec![
        Section {
            chains: vec![Some(0), None, None, None],
            length: Some(6),
            ..Section::default()
        },
        Section {
            chains: vec![Some(1), None, None, None],
            length: Some(8),
            start: SectionStart::New,
            ..Section::default()
        },
    ];
    assert_eq!(
        hits(&played, 14 * STEP, 0),
        [0, 4 * STEP, 6 * STEP, 10 * STEP]
    );
}

#[test]
fn a_section_lasts_its_longest_chain_in_the_songs_steps() {
    let played = song(&[
        (Speed::Eighth, &["C-3 --"; 4]),
        (Speed::Sixteenth, &["C-3 --"; 4]),
    ]);
    assert_eq!(played.section_length(&played.sections[0]), 8);
    // Three 16th triplets are two sixteenths, and four don't end on one, so they round up.
    let triplets = song(&[(Speed::SixteenthTriplet, &["C-3 --"; 4])]);
    assert_eq!(triplets.section_length(&triplets.sections[0]), 3);
}

#[test]
fn triplets_run_on_against_the_section_on_their_own_grid() {
    // Four 16th triplets in a section of 3 sixteenths: the track loops on its own, staying on the
    // triplets' grid from the start.
    let triplets = song(&[(Speed::SixteenthTriplet, &["C-3 --"; 4])]);
    let expected: Vec<usize> = (0..9).map(|step| step * 160 * TICK).collect();
    assert_eq!(hits(&triplets, 9 * 160 * TICK, 0), expected);
}

#[test]
fn timing_commands_are_shares_of_the_tracks_step() {
    // `DEL 3` is half a step, so half an eighth.
    let delayed = song(&[(Speed::Eighth, &["C-3 --", "C-3 -- del:03"])]);
    assert_eq!(hits(&delayed, 4 * STEP, 0), [0, 3 * STEP]);
    // `RET 3` repeats every half step, so every eighth at a quarter.
    let repeated = song(&[(Speed::Quarter, &["C-3 -- ret:03"])]);
    assert_eq!(hits(&repeated, 4 * STEP, 0), [0, 2 * STEP]);
    // A nudge isn't: it's in 24ths of a sixteenth on every track, so 6 is a quarter of one.
    let nudged = song(&[(Speed::Eighth, &["C-3 --", "C-3 -- +6"])]);
    assert_eq!(hits(&nudged, 4 * STEP, 0), [0, 2 * STEP + STEP / 4]);
}

/// A nudge's unit, a 24th of a sixteenth, in frames.
const NUDGE: usize = 10 * TICK;

/// How long a step is at a speed, in frames.
fn step_frames(speed: Speed) -> usize {
    speed.ticks() as usize * TICK
}

#[test]
fn the_same_nudge_plays_at_the_same_time_on_every_speed() {
    for speed in Speed::ALL {
        let step = step_frames(speed);
        for nudge in ["+5", "-5", "+1", "-3"] {
            let rows = ["C-3 --".to_owned(), format!("C-3 -- {nudge}")];
            let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
            let played = song(&[(speed, &rows)]);
            let by: i64 = nudge.parse::<i64>().unwrap();
            let at = (step as i64 + by * NUDGE as i64) as usize;
            assert_eq!(hits(&played, 2 * step, 0), [0, at], "{nudge} at {speed}");
        }
    }
}

#[test]
fn a_nudge_goes_just_under_half_its_tracks_step() {
    for speed in Speed::ALL {
        let step = step_frames(speed);
        let most = speed.most_nudge();
        assert!(2 * usize::from(most.unsigned_abs()) * NUDGE < step, "{speed}");
        assert!(2 * (usize::from(most.unsigned_abs()) + 1) * NUDGE >= step, "{speed}");
        // The furthest any track goes, written on this one, plays as far as this one's goes.
        let played = song(&[(speed, &["C-3 --", "C-3 -- +2F", "C-3 -- -2F", "--- --"])]);
        let most = usize::from(most.unsigned_abs()) * NUDGE;
        assert_eq!(
            hits(&played, 4 * step, 0),
            [0, step + most, 2 * step - most],
            "{speed}"
        );
    }
}

#[test]
fn a_rate_lock_keeps_a_nudges_time_and_keeps_it_to_the_new_steps_range() {
    // A sixteenth's `+8` under a lock to an eighth is still 8 24ths of a sixteenth late.
    let slower = song(&[(Speed::Sixteenth, &["C-3 --", "C-3 -- +8 speed:04", "C-3 --"])]);
    // The eighth starts on the second sixteenth, so it lasts to the next eighth.
    assert_eq!(hits(&slower, 3 * STEP, 0), [0, STEP + 8 * NUDGE, 2 * STEP]);
    // Under a lock to a 32nd, whose range is 5, it's kept to 5.
    let faster = song(&[(Speed::Sixteenth, &["C-3 --", "C-3 -- +8 speed:00", "C-3 --"])]);
    assert_eq!(
        hits(&faster, 2 * STEP, 0),
        [0, STEP + 5 * NUDGE, STEP + STEP / 2]
    );
    // And early, under a lock to a 32nd held on, the earliest it can go there.
    let early = song(&[(
        Speed::Sixteenth,
        &["C-3 -- speed:00h", "C-3 -- -9", "C-3 --"],
    )]);
    let half = STEP / 2;
    assert_eq!(hits(&early, 3 * half, 0), [0, half - 5 * NUDGE, 2 * half]);
}

#[test]
fn the_engine_works_ahead_as_far_as_each_speeds_earliest_nudge() {
    for speed in Speed::ALL {
        let step = step_frames(speed);
        let most = speed.most_nudge();
        let early = format!("C-3 -- -{:X}", most);
        let played = song(&[(speed, &["C-3 --", early.as_str(), "C-3 --"])]);
        let published = version(&played, hits_pool());
        assert_eq!(published.lead, u64::from(most.unsigned_abs()) * 10, "{speed}");
        let at = step - usize::from(most.unsigned_abs()) * NUDGE;
        assert_eq!(hits(&played, 3 * step, 0), [0, at, 2 * step], "{speed}");
    }
    // Two tracks: the slow one's earliest nudge is the lead, and both play where they're due.
    let both = song(&[
        (Speed::Quarter, &["C-3 --", "C-3 -- -2F"]),
        (Speed::ThirtySecond, &["C-3 --", "C-3 -- -5"]),
    ]);
    assert_eq!(version(&both, hits_pool()).lead, 470);
    let quarter = step_frames(Speed::Quarter);
    let thirty_second = step_frames(Speed::ThirtySecond);
    assert_eq!(hits(&both, 2 * quarter, 0), [0, quarter - 47 * NUDGE]);
    assert_eq!(
        hits(&both, 2 * thirty_second, 1),
        [0, thirty_second - 5 * NUDGE]
    );
}

#[test]
fn pulses_and_lengths_follow_the_tracks_step() {
    let pulses = song(&[(Speed::Eighth, &["C-3 -- MLT2", "--- --"])]);
    assert_eq!(hits(&pulses, 4 * STEP, 0), [0, 2 * STEP]);
    // A gate of 50% on an eighth lasts a sixteenth.
    let mut gated = song(&[(Speed::Eighth, &["C-3 --", "--- --"])]);
    gated.tracks[0].gate_length = Some(0x80);
    let end = ends(&gated, 4 * STEP);
    assert!(end.abs_diff(STEP) < NEAR, "{end}");
}

#[test]
fn a_loop_renders_as_long_as_its_longest_chain_at_its_speed() {
    let played = song(&[(Speed::Eighth, &["C-3 --"; 4])]);
    let out = render_loop(
        version(&played, hits_pool()),
        RATE,
        RenderOptions::default(),
    );
    assert_eq!(out.len(), 8 * STEP * 2);
}

#[test]
fn each_track_is_heard_on_its_own_step() {
    let played = song(&[
        (Speed::Eighth, &["C-3 --"; 4]),
        (Speed::Sixteenth, &["C-3 --"; 8]),
    ]);
    let mut engine = Engine::new(RATE, version(&played, hits_pool()));
    engine.play_from(0);
    let mut out = vec![0.0; (5 * STEP + STEP / 2) * 2];
    engine.process(&mut out);
    assert_eq!(engine.transport(), Transport::Playing);
    let eighth = engine.track_heard(0);
    assert_eq!(eighth.count, 2);
    assert!((eighth.progress - 0.75).abs() < 0.01, "{eighth:?}");
    assert_eq!(eighth.ticks, 480);
    let sixteenth = engine.track_heard(1);
    assert_eq!(sixteenth.count, 5);
    assert!((sixteenth.progress - 0.5).abs() < 0.01, "{sixteenth:?}");
}

#[test]
fn playing_from_a_slower_tracks_row_starts_the_song_where_it_falls() {
    let played = song(&[
        (Speed::Eighth, &["C-3 --", "D-3 --", "E-3 --", "F-3 --"]),
        (Speed::Sixteenth, &["C-3 --"; 8]),
    ]);
    let mut engine = Engine::new(RATE, version(&played, hits_pool()));
    engine.play_section_at(0, 3, Some(0));
    assert_eq!(engine.position(), 6);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.track_heard(0).count, 3);
    assert_eq!(engine.track_heard(1).count, 6);
}
