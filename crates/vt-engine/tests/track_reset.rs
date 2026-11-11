//! A track's own reset: its chain starts again every so many of the song's steps, counted from
//! where it last started, while the other tracks drift on. `Space a` does the same once, by hand,
//! for one track.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::groove::GrooveId;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Launch, Phrase, Section, Song};
use vt_model::speed::Speed;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Halfway into a step.
const HALF: usize = STEP / 2;

/// A long ramp, so a note's level says how far into it the note is, and its velocity how high it
/// starts.
fn pool() -> Arc<SamplePool> {
    let frames = RATE as usize * 8;
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: (0..frames)
                .map(|at| {
                    let level = 0.25 + 0.5 * at as f32 / frames as f32;
                    [level, level]
                })
                .collect(),
        },
    );
    Arc::new(pool)
}

/// Tracks playing these rows, with notes that start and stop at once, in a long section.
fn song(tracks: &[&[&str]]) -> Song {
    let mut song = Song::new(tracks.len());
    song.limiter = false;
    song.sections[0].length = Some(64);
    for (index, rows) in tracks.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        track.envelopes[0].attack_ms = 0;
        track.envelopes[0].release_ms = 0;
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// Rows of notes, one a step.
fn rows(count: usize) -> Vec<&'static str> {
    vec!["C-3 40"; count]
}

/// Where in its chain a track is heard, as its playhead is drawn: its count since its chain
/// started, within the chain's length.
fn place(engine: &Engine, track: usize) -> i64 {
    let jam = engine.jam(track);
    let length = jam.chain.map_or(1, |chain| {
        engine.current().song.tracks[track].chain_steps(chain) as i64
    });
    (engine.track_heard(track).count - jam.started).rem_euclid(length.max(1))
}

/// Each track's place in its chain, halfway into each of the song's steps, playing from the top.
fn places(song: &Song, steps: usize) -> Vec<Vec<i64>> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut heard = vec![Vec::new(); song.sound_tracks()];
    engine.process(&mut vec![0.0; HALF * 2]);
    for _ in 0..steps {
        for (track, places) in heard.iter_mut().enumerate() {
            places.push(place(&engine, track));
        }
        engine.process(&mut vec![0.0; STEP * 2]);
    }
    heard
}

/// The places a chain of this length plays in, starting again at each of these steps.
fn expected(length: i64, starts: &[usize], steps: usize) -> Vec<i64> {
    (0..steps)
        .map(|step| {
            let start = starts.iter().rfind(|start| **start <= step).copied();
            (step - start.unwrap_or(0)) as i64 % length
        })
        .collect()
}

#[test]
fn a_five_step_part_reset_every_sixteen_keeps_to_a_bar_while_a_seven_step_part_drifts() {
    let mut drifting = song(&[&rows(5), &rows(7)]);
    drifting.tracks[0].reset = Some(16);
    let heard = places(&drifting, 40);
    // Five, five, five and one, every bar.
    assert_eq!(heard[0], expected(5, &[0, 16, 32], 40));
    assert_eq!(heard[1], (0..40).map(|step| step % 7).collect::<Vec<_>>());
}

#[test]
fn without_a_reset_a_track_plays_on() {
    let free = song(&[&rows(5), &rows(7)]);
    let heard = places(&free, 40);
    assert_eq!(heard[0], (0..40).map(|step| step % 5).collect::<Vec<_>>());
}

#[test]
fn a_section_reset_starts_a_tracks_count_again() {
    let mut both = song(&[&rows(5), &rows(7)]);
    both.tracks[0].reset = Some(8);
    both.sections[0].reset = Some(12);
    let heard = places(&both, 40);
    // Its own at 8, the section's at 12, its own 8 after that at 20, the section's at 24, and so
    // on: its own counts from the section's.
    assert_eq!(heard[0], expected(5, &[0, 8, 12, 20, 24, 32, 36], 40));
    assert_eq!(heard[1], expected(7, &[0, 12, 24, 36], 40));
}

#[test]
fn where_the_two_coincide_they_start_it_once() {
    let mut both = song(&[&rows(5)]);
    both.tracks[0].reset = Some(8);
    both.sections[0].reset = Some(16);
    let mut own = both.clone();
    own.sections[0].reset = None;
    assert_eq!(places(&both, 40), places(&own, 40));
}

/// Two sections, the second bringing a new chain in on the first track, which resets every 8.
fn two_sections() -> Song {
    let mut two = song(&[&rows(5), &rows(16)]);
    two.sections[0].length = Some(12);
    let track = &mut two.tracks[0];
    track.reset = Some(8);
    track.phrases.insert(
        1,
        Arc::new(Phrase::from_steps(vec!["D-3 40".parse().unwrap(); 5])),
    );
    track.chains.insert(
        1,
        Arc::new(Chain {
            entries: vec![ChainEntry {
                phrase: 1,
                transpose: 0,
            }],
            label: String::new(),
        }),
    );
    two.sections.push(Section {
        chains: vec![Some(1), Some(0)],
        length: Some(16),
        ..Section::default()
    });
    two
}

#[test]
fn it_counts_from_where_a_section_brought_the_chain_in() {
    let heard = places(&two_sections(), 28);
    // Its own at 8, the new chain at 12, and its own 8 after that, at 20, not at 16.
    assert_eq!(heard[0], expected(5, &[0, 8, 12, 20], 28));
}

#[test]
fn a_slower_track_starts_again_on_the_songs_step_cutting_its_own_short() {
    let mut slow = song(&[&rows(4), &rows(16)]);
    slow.tracks[0].speed = Speed::Eighth;
    slow.tracks[0].reset = Some(5);
    let heard = places(&slow, 15);
    // Eighths at 0, 2 and 4, cut short at 5, then 5, 7, 9, cut short at 10.
    assert_eq!(heard[0], [0, 0, 1, 1, 2, 0, 0, 1, 1, 2, 0, 0, 1, 1, 2]);
}

/// Each track's place for some steps after playing from each of these steps of a section, given
/// with where the section starts in the song, against playing from the top through them. A
/// slower track partway through a step where playing starts comes in on its next, so the step
/// playing starts on isn't compared.
fn check_play_from(song: &Song, section: usize, starts_at: usize, starts: &[usize]) {
    const AFTER: usize = 6;
    let steps = starts_at + starts.iter().max().copied().unwrap_or(0) + AFTER + 1;
    let through = places(song, steps);
    for start in starts {
        let mut engine = Engine::new(RATE, version(song));
        engine.play_section(section, *start);
        engine.process(&mut vec![0.0; (STEP + HALF) * 2]);
        for step in start + 1..=start + AFTER {
            for (track, heard) in through.iter().enumerate() {
                assert_eq!(
                    place(&engine, track),
                    heard[starts_at + step],
                    "track {track} at {step} playing from {start} of section {section}"
                );
            }
            engine.process(&mut vec![0.0; STEP * 2]);
        }
    }
}

#[test]
fn playing_from_a_step_hears_what_playing_up_to_it_would() {
    let mut drifting = song(&[&rows(5), &rows(7), &rows(3)]);
    drifting.tracks[0].reset = Some(16);
    drifting.tracks[1].reset = Some(6);
    drifting.tracks[2].speed = Speed::EighthTriplet;
    drifting.tracks[2].reset = Some(5);
    drifting.sections[0].reset = Some(36);
    check_play_from(
        &drifting,
        0,
        0,
        &[0, 3, 5, 6, 7, 15, 16, 17, 30, 31, 35, 36, 37, 41, 50],
    );
    check_play_from(&two_sections(), 0, 0, &[2, 5, 9]);
    check_play_from(&two_sections(), 1, 12, &[0, 1, 7, 8, 9]);
}

#[test]
fn playing_from_a_row_of_a_tracks_chain_counts_its_reset_from_the_chains_top() {
    let mut part = song(&[&rows(5)]);
    part.tracks[0].reset = Some(8);
    let mut engine = Engine::new(RATE, version(&part));
    engine.play_section_at(0, 3, Some(0));
    let mut heard = Vec::new();
    engine.process(&mut vec![0.0; HALF * 2]);
    for _ in 0..8 {
        heard.push(place(&engine, 0));
        engine.process(&mut vec![0.0; STEP * 2]);
    }
    // From its fourth row, so the reset comes 5 steps in, on its eighth.
    assert_eq!(heard, [3, 4, 0, 1, 2, 0, 1, 2]);
}

/// The whole of the first so many steps.
fn played(song: &Song, steps: usize) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; steps * STEP * 2];
    engine.process(&mut out);
    out
}

#[test]
fn a_reset_plays_the_same_as_the_bar_written_out_swing_and_all() {
    let part = ["C-3 40", "D-3 20", "--- --", "E-3 30", "G-3 10"];
    let mut reset = song(&[&part]);
    reset.groove = GrooveId::swing(60).unwrap();
    reset.tracks[0].reset = Some(16);
    let bar: Vec<&str> = part.iter().cycle().take(16).copied().collect();
    let mut written = song(&[&bar]);
    written.groove = reset.groove;
    assert_eq!(played(&reset, 48), played(&written, 48));
}

#[test]
fn a_reset_is_a_chain_start_so_a_first_pass_condition_plays_again() {
    // `CND 1ST` on the first row, with quieter rows after, so a loud step is the first row heard.
    let mut first = song(&[&["C-3 40 cnd:07", "C-3 10", "C-3 10"]]);
    first.tracks[0].reset = Some(8);
    let out = played(&first, 24);
    let level = |step: usize| out[(step * STEP + 100) * 2];
    let loudest = (0..24).map(level).fold(0.0, f32::max);
    let loud: Vec<usize> = (0..24)
        .filter(|step| level(*step) > loudest * 0.75)
        .collect();
    assert_eq!(loud, [0, 8, 16]);
}

#[test]
fn space_a_starts_one_tracks_chain_again_on_the_launch_grid() {
    let mut two = song(&[&rows(5), &rows(7)]);
    two.launch = Launch::every(4).unwrap();
    let mut engine = Engine::new(RATE, version(&two));
    engine.play_from(0);
    engine.process(&mut vec![0.0; (STEP + HALF) * 2]);
    engine.toggle_realign_track(0);
    assert!(engine.track_realign_waiting(0));
    assert!(!engine.track_realign_waiting(1));
    assert!(!engine.realign_waiting(), "it isn't every track's realign");
    let mut heard = [Vec::new(), Vec::new()];
    for _ in 1..10 {
        for (track, places) in heard.iter_mut().enumerate() {
            places.push(place(&engine, track));
        }
        engine.process(&mut vec![0.0; STEP * 2]);
    }
    // Asked for in step 1, it lands on step 4; the other track carries on.
    assert_eq!(heard[0], [1, 2, 3, 0, 1, 2, 3, 4, 0]);
    assert_eq!(heard[1], [1, 2, 3, 4, 5, 6, 0, 1, 2]);
    assert!(!engine.track_realign_waiting(0), "it's done once landed");
}

#[test]
fn a_tracks_own_reset_leaves_its_realign_waiting_for_the_grid() {
    let mut part = song(&[&rows(5)]);
    part.tracks[0].reset = Some(3);
    let mut engine = Engine::new(RATE, version(&part));
    engine.play_from(0);
    engine.process(&mut vec![0.0; (STEP + HALF) * 2]);
    engine.toggle_realign_track(0);
    // Its own resets at 3, 6, 9, 12 and 15 come and go, and the realign still waits for the bar.
    engine.process(&mut vec![0.0; 14 * STEP * 2]);
    assert!(engine.track_realign_waiting(0), "still waiting in step 15");
    engine.process(&mut vec![0.0; STEP * 2]);
    assert!(!engine.track_realign_waiting(0), "landed on the bar");
}

#[test]
fn space_a_again_takes_it_back_and_stopping_forgets_it() {
    let two = song(&[&rows(5), &rows(7)]);
    let mut engine = Engine::new(RATE, version(&two));
    engine.toggle_realign_track(0);
    assert!(
        !engine.track_realign_waiting(0),
        "nothing to realign while stopped"
    );
    engine.play_from(0);
    engine.process(&mut vec![0.0; HALF * 2]);
    engine.toggle_realign_track(0);
    engine.toggle_realign_track(0);
    assert!(
        !engine.track_realign_waiting(0),
        "a second press takes it back"
    );
    engine.toggle_realign_track(1);
    engine.pause_or_resume();
    assert!(!engine.track_realign_waiting(1), "pausing forgets it");
}

#[test]
fn every_tracks_realign_takes_one_tracks_with_it() {
    let two = song(&[&rows(5), &rows(7)]);
    let mut engine = Engine::new(RATE, version(&two));
    engine.play_from(0);
    engine.process(&mut vec![0.0; (STEP + HALF) * 2]);
    engine.toggle_realign_track(1);
    engine.toggle_realign();
    engine.process(&mut vec![0.0; 15 * STEP * 2]);
    assert!(!engine.realign_waiting());
    assert!(!engine.track_realign_waiting(1));
    assert_eq!(place(&engine, 0), 0);
    assert_eq!(place(&engine, 1), 0);
}

#[test]
fn one_tracks_realign_keeps_its_swing_on_the_songs_beat() {
    // Realigned on an odd step, its next step still swings as the song's odd steps do: the same
    // as the bar written out from that step.
    let mut swung = song(&[&["C-3 40", "D-3 20", "E-3 30"]]);
    swung.groove = GrooveId::swing(66).unwrap();
    swung.launch = Launch::named("step").unwrap();
    let mut engine = Engine::new(RATE, version(&swung));
    engine.play_from(0);
    let mut out = vec![0.0; (STEP + HALF) * 2];
    engine.process(&mut out);
    engine.toggle_realign_track(0);
    let mut rest = vec![0.0; (7 * STEP - HALF) * 2];
    engine.process(&mut rest);
    out.extend(rest);
    let mut written = song(&[&[
        "C-3 40", "D-3 20", "C-3 40", "D-3 20", "E-3 30", "C-3 40", "D-3 20", "E-3 30",
    ]]);
    written.groove = swung.groove;
    assert_eq!(out, played(&written, 8));
}
