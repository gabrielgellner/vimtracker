//! Songs: sections playing in order, their lengths, and what each does to the tracks.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Phrase, Section, SectionStart, Song};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

fn samples() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 40],
        },
    );
    Arc::new(pool)
}

/// One track, with a chain per entry whose every step has this velocity, as a way of hearing
/// which chain and step is playing.
fn song(chains: &[&[&str]]) -> Song {
    let mut song = Song::new(1);
    song.tracks[0].sample = Some("s".to_owned());
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    song.limiter = false;
    song.sections.clear();
    for (id, steps) in chains.iter().enumerate() {
        let id = id as u32 + 10;
        let steps = steps.iter().map(|text| text.parse().unwrap()).collect();
        song.tracks[0]
            .phrases
            .insert(id, Arc::new(Phrase::from_steps(steps)));
        song.tracks[0].chains.insert(
            id,
            Arc::new(Chain {
                entries: vec![ChainEntry {
                    phrase: id,
                    transpose: 0,
                }],
            }),
        );
        song.sections.push(Section {
            chains: vec![Some(id)],
            ..Section::default()
        });
    }
    song
}

fn version(song: Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song), samples()))
}

/// The left channel a little into each step, rendering from the top.
fn levels(song: Song, steps: usize) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.play_from(0);
    let mut out = vec![0.0; steps * STEP * 2];
    engine.process(&mut out);
    (0..steps)
        .map(|step| out[(step * STEP + 3000) * 2])
        .collect()
}

const A: &str = "C-4 40 ---00 ---00 ---00";
const B: &str = "C-4 20 ---00 ---00 ---00";
const C: &str = "C-4 10 ---00 ---00 ---00";
const AV: f32 = 0x40 as f32 / 0x7F as f32;
const BV: f32 = 0x20 as f32 / 0x7F as f32;
const CV: f32 = 0x10 as f32 / 0x7F as f32;

#[track_caller]
fn same(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (at, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (a - e).abs() < 0.01,
            "step {at}: {actual:?}, not {expected:?}"
        );
    }
}

#[test]
fn sections_play_in_order_and_the_song_loops() {
    let out = levels(song(&[&[A, A], &[B], &[C, C, C]]), 8);
    same(&out, &[AV, AV, BV, CV, CV, CV, AV, AV]);
}

#[test]
fn a_set_length_cuts_a_section_short_or_lets_its_chains_loop_on() {
    let mut cut = song(&[&[A, A, A, A], &[B]]);
    cut.sections[0].length = Some(2);
    same(&levels(cut, 4), &[AV, AV, BV, AV]);
    let mut long = song(&[&[A], &[B]]);
    long.sections[0].length = Some(3);
    same(&levels(long, 5), &[AV, AV, AV, BV, AV]);
}

/// Three different notes tell which step of a chain is playing.
const STEPS: [&str; 3] = [
    "C-4 40 ---00 ---00 ---00",
    "C-4 20 ---00 ---00 ---00",
    "C-4 10 ---00 ---00 ---00",
];

#[test]
fn a_section_carries_positions_by_default_and_can_reset_them() {
    // Two steps of the first section, so the next carries on from its third step.
    let mut carry = song(&[&[A, A], &STEPS]);
    carry.sections[1].length = Some(3);
    assert_eq!(carry.sections[1].start, SectionStart::Carry);
    same(&levels(carry.clone(), 5), &[AV, AV, CV, AV, BV]);
    carry.sections[1].start = SectionStart::Reset;
    same(&levels(carry, 5), &[AV, AV, AV, BV, CV]);
}

#[test]
fn one_section_that_wraps_keeps_its_tracks_free_running() {
    let mut two = Song::new(2);
    two.tracks[0].sample = Some("s".to_owned());
    two.tracks[1].sample = Some("s".to_owned());
    two.tracks[1].phrases.insert(
        0,
        Arc::new(Phrase::from_steps(
            STEPS.iter().map(|text| text.parse().unwrap()).collect(),
        )),
    );
    two.tracks[0].phrases.insert(
        0,
        Arc::new(Phrase::from_steps(vec![
            "--- -- ---00 ---00 ---00"
                .parse()
                .unwrap();
            2
        ])),
    );
    two.sections[0].length = Some(2);
    two.tracks[1].envelopes[0].attack_ms = 0;
    two.tracks[1].envelopes[0].release_ms = 0;
    two.limiter = false;
    same(&levels(two, 4), &[AV, BV, CV, AV]);
}

#[test]
fn a_section_can_be_looped_while_working_on_it() {
    let mut engine = Engine::new(RATE, version(song(&[&[A], &[B, B], &[C]])));
    engine.set_loop_section(Some(1));
    engine.play_section(1, 0);
    let mut out = vec![0.0; 5 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.section(), 1);
    assert!((out[3000 * 2] - BV).abs() < 0.01);
    assert!((out[(4 * STEP + 3000) * 2] - BV).abs() < 0.01);
    engine.set_loop_section(None);
    let mut seen = Vec::new();
    for _ in 0..4 {
        let mut one = vec![0.0; STEP * 2];
        engine.process(&mut one);
        seen.push(engine.section());
    }
    assert!(seen.contains(&2), "it moves on again: {seen:?}");
}

#[test]
fn playing_from_a_section_starts_there_and_auditions_use_it() {
    let mut engine = Engine::new(RATE, version(song(&[&[A], &[B]])));
    engine.play_section(1, 0);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert!((out[3000 * 2] - BV).abs() < 0.01);
    engine.return_to_start();
    assert_eq!(engine.section(), 0);
    engine.audition_section(1, 0);
    let mut out = vec![0.0; 3000 * 2];
    engine.process(&mut out);
    assert!((out[2000 * 2] - BV).abs() < 0.01);
}

#[test]
fn a_section_with_no_length_is_skipped_and_nothing_is_fine() {
    let mut gap = song(&[&[A], &[B]]);
    gap.sections.insert(1, Section::default());
    gap.sections[1].chains = vec![None];
    same(&levels(gap, 3), &[AV, BV, AV]);
    let mut empty = song(&[&[A]]);
    empty.sections[0].chains = vec![None];
    same(&levels(empty, 2), &[0.0, 0.0]);
}

#[test]
fn removing_the_playing_section_moves_on_without_harm() {
    let start = song(&[&[A], &[B, B], &[C]]);
    let mut engine = Engine::new(RATE, version(start.clone()));
    engine.play_section(2, 0);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    let mut less = start;
    less.sections.truncate(1);
    engine.swap(version(less));
    engine.process(&mut out);
    assert_eq!(engine.section(), 0);
}

#[test]
fn a_render_covers_the_whole_song_once() {
    let whole = song(&[&[A, A], &[B], &[C, C, C]]);
    let frames = |song: Song| render_loop(version(song), RATE, RenderOptions::default()).len() / 2;
    assert_eq!(frames(whole.clone()), 6 * STEP);
    let out = render_loop(version(whole.clone()), RATE, RenderOptions::default());
    for (step, level) in [(0, AV), (1, AV), (2, BV), (3, CV), (5, CV)] {
        let at = (step * STEP + 3000) * 2;
        assert!((out[at] - level).abs() < 0.01, "step {step}: {}", out[at]);
    }
    let stems = render_stems(version(whole).as_ref(), RATE, RenderOptions::default());
    assert_eq!(stems[0].samples.len(), 6 * STEP * 2, "stems cover it too");
}

#[test]
fn songs_with_sections_render_the_same_each_time() {
    let whole = song(&[&[A, "--- -- CHA80 ---00 ---00"], &[B, C]]);
    let render = || render_loop(version(whole.clone()), RATE, RenderOptions::default());
    assert_eq!(render(), render());
}

/// Play from the top, launching partway through, and give the level in each step.
fn jam(song: Song, steps: usize, launch_at: usize, act: impl Fn(&mut Engine)) -> Vec<f32> {
    let mut engine = Engine::new(RATE, version(song));
    engine.set_loop_section(Some(0));
    engine.play_from(0);
    let mut before = vec![0.0; (launch_at * STEP + 3000) * 2];
    engine.process(&mut before);
    act(&mut engine);
    let mut after = vec![0.0; (steps * STEP - before.len() / 2) * 2];
    engine.process(&mut after);
    before.extend_from_slice(&after);
    (0..steps)
        .map(|step| before[(step * STEP + 3000) * 2])
        .collect()
}

#[test]
fn a_launched_chain_waits_for_the_end_then_plays_from_its_own_start() {
    let out = jam(song(&[&[A, A, A], &[B, C]]), 9, 1, |engine| {
        engine.launch(0, Some(11));
    });
    same(&out, &[AV, AV, AV, BV, CV, BV, CV, BV, CV]);
}

#[test]
fn launching_on_a_none_stops_the_track_at_its_end() {
    let out = jam(song(&[&[A, A]]), 5, 0, |engine| engine.launch(0, None));
    same(&out, &[AV, AV, 0.0, 0.0, 0.0]);
}

#[test]
fn launches_end_on_a_new_section_or_playing_again() {
    let mut engine = Engine::new(RATE, version(song(&[&[A, A], &[B]])));
    engine.play_from(0);
    engine.launch(0, Some(11));
    let mut out = vec![0.0; 3 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.jam(0).playing, None, "the song moved on");

    engine.set_loop_section(Some(0));
    engine.play_from(0);
    engine.launch(0, Some(11));
    let mut out = vec![0.0; 3 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.jam(0).playing, Some(Some(11)));
    engine.play_from(0);
    assert_eq!(engine.jam(0).playing, None);
}

#[test]
fn a_reset_keeps_a_launched_chain_and_starts_it_over() {
    let mut reset = song(&[&[A, A], &[B, C, C]]);
    reset.sections[0].start = SectionStart::Reset;
    let out = jam(reset, 7, 0, |engine| engine.launch(0, Some(11)));
    // Launched at step 2. The section's 2 steps send it back to the chain's start each time.
    same(&out, &[AV, AV, BV, CV, BV, CV, BV]);
}

#[test]
fn continuing_in_a_captured_section_doesnt_change_what_plays() {
    let mut captured = song(&[&[A, A, A], &[B, C]]);
    captured.sections.insert(1, captured.sections[1].clone());
    let play = |capture: bool| {
        let mut engine = Engine::new(RATE, version(captured.clone()));
        engine.set_loop_section(Some(0));
        engine.play_from(0);
        engine.launch(0, Some(11));
        let mut out = vec![0.0; 10 * STEP * 2];
        let (early, late) = out.split_at_mut(4 * STEP * 2);
        engine.process(early);
        if capture {
            engine.continue_in(1);
        }
        engine.process(late);
        out
    };
    assert_eq!(play(true), play(false));
}
