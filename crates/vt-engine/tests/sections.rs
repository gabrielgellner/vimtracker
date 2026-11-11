//! Songs: sections playing in order, their lengths, and what each does to the tracks.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems};
use vt_model::groove::GrooveId;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Launch, Phrase, Repeat, Section, SectionStart, Song};

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
    song.sample_on(0, "s");
    song.tracks[0].envelopes[0].attack_ms = 0;
    song.tracks[0].envelopes[0].release_ms = 0;
    song.limiter = false;
    song.launch = Launch::ChainEnd;
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
                label: String::new(),
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

const A: &str = "C-3 40 --- --- ---";
const B: &str = "C-3 20 --- --- ---";
const C: &str = "C-3 10 --- --- ---";
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
    "C-3 40 --- --- ---",
    "C-3 20 --- --- ---",
    "C-3 10 --- --- ---",
];

#[test]
fn a_different_chain_starts_from_its_top() {
    let mut new = song(&[&[A, A], &STEPS]);
    new.sections[1].length = Some(3);
    assert_eq!(new.sections[1].start, SectionStart::New);
    same(&levels(new, 5), &[AV, AV, AV, BV, CV]);
}

/// A chain of six steps, on two sections of three.
fn spanning() -> Song {
    let mut span = song(&[&["C-3 40 --- --- ---"], &STEPS]);
    let steps: Vec<_> = STEPS
        .iter()
        .chain(&[A, B, C])
        .map(|t| t.parse().unwrap())
        .collect();
    let track = &mut span.tracks[0];
    track
        .phrases
        .insert(11, Arc::new(Phrase::from_steps(steps)));
    span.sections[0].chains = vec![Some(11)];
    span.sections[0].length = Some(3);
    span.sections[1].chains = vec![Some(11)];
    span.sections[1].length = Some(3);
    span
}

#[test]
fn the_same_chain_runs_across_sections_and_a_reset_restarts_it() {
    same(&levels(spanning(), 6), &[AV, BV, CV, AV, BV, CV]);
    let mut restart = spanning();
    let steps: Vec<_> = STEPS
        .iter()
        .chain(&[C, C, C])
        .map(|t| t.parse().unwrap())
        .collect();
    restart.tracks[0]
        .phrases
        .insert(11, Arc::new(Phrase::from_steps(steps)));
    same(&levels(restart.clone(), 6), &[AV, BV, CV, CV, CV, CV]);
    restart.sections[1].start = SectionStart::Reset;
    same(&levels(restart, 6), &[AV, BV, CV, AV, BV, CV]);
}

#[test]
fn the_song_wrapping_starts_every_chain_from_its_top() {
    let mut wrap = song(&[&STEPS, &[C]]);
    wrap.sections[0].length = Some(2);
    // The second section keeps the first's chain, which a wrap restarts all the same.
    wrap.sections[1].chains = wrap.sections[0].chains.clone();
    wrap.sections[1].length = Some(2);
    same(&levels(wrap, 8), &[AV, BV, CV, AV, AV, BV, CV, AV]);
}

#[test]
fn a_wrap_keeps_the_running_count_so_a_groove_doesnt_change() {
    let mut wrap = song(&[&[A, A, A], &[B, B]]);
    wrap.groove = GrooveId::swing(66).unwrap();
    let frames = |wrap: Song| {
        let mut engine = Engine::new(RATE, version(wrap));
        engine.play_from(0);
        let mut out = vec![0.0; 7 * STEP * 2];
        engine.process(&mut out);
        engine.position()
    };
    assert!(frames(wrap) >= 6, "the count kept going past the wrap");
}

#[test]
fn an_empty_row_doesnt_change_a_song_of_one() {
    let looped = song(&[&[A], &STEPS]);
    let mut alone = looped.clone();
    alone.sections[0].length = Some(4);
    alone.sections[0].chains = vec![Some(11)];
    alone.sections.truncate(1);
    let mut extra = alone.clone();
    extra.sections.push(Section {
        chains: vec![None],
        ..Section::default()
    });
    same(&levels(extra.clone(), 9), &levels(alone.clone(), 9));
    assert_eq!(
        render_loop(version(extra), RATE, RenderOptions::default()),
        render_loop(version(alone), RATE, RenderOptions::default()),
    );
}

#[test]
fn a_launch_ending_on_the_same_chain_keeps_going() {
    let mut moving = song(&[&[A, A, A, A], &STEPS, &[C]]);
    moving.sections[0].length = Some(4);
    moving.sections[1].length = Some(2);
    moving.sections[2].chains = vec![Some(11)];
    moving.launch = Launch::every(2).unwrap();
    let mut engine = Engine::new(RATE, version(moving));
    engine.play_from(0);
    let mut out = vec![0.0; 3000 * 2];
    engine.process(&mut out);
    engine.launch(0, Some(11));
    let mut rest = vec![0.0; 7 * STEP * 2];
    engine.process(&mut rest);
    out.extend_from_slice(&rest);
    let got: Vec<f32> = (0..7).map(|step| out[(step * STEP + 3000) * 2]).collect();
    // Launched at step 2, and the second section's chain is the same chain, so it runs on through.
    same(&got, &[AV, AV, AV, BV, CV, AV, BV]);
}

/// The levels from a section on, playing from it or playing through to it.
fn from_and_through(
    song: Song,
    section: usize,
    before: usize,
    steps: usize,
) -> (Vec<f32>, Vec<f32>) {
    let through = levels(song.clone(), before + steps)[before..].to_vec();
    let mut engine = Engine::new(RATE, version(song));
    engine.play_section(section, 0);
    let mut out = vec![0.0; steps * STEP * 2];
    engine.process(&mut out);
    let from = (0..steps)
        .map(|step| out[(step * STEP + 3000) * 2])
        .collect();
    (from, through)
}

#[test]
fn playing_from_a_row_stands_where_playing_through_would() {
    let (from, through) = from_and_through(spanning(), 1, 3, 3);
    same(&from, &through);
    same(&from, &[AV, BV, CV]);
}

#[test]
fn playing_from_a_row_accounts_for_resets_before_it() {
    let longest = [A, B, C, A, B, C, A, B, C, A, B, C];
    let cases = [
        (Some(4), SectionStart::New),
        (Some(16), SectionStart::New),
        (None, SectionStart::Reset),
    ];
    for (reset, start) in cases {
        let mut song = song(&[&longest, &[C], &[C]]);
        song.sections[0].length = Some(5);
        song.sections[0].reset = reset;
        song.sections[1].chains = vec![Some(10)];
        song.sections[1].length = Some(3);
        song.sections[1].start = start;
        song.sections[2].chains = vec![Some(10)];
        song.sections[2].length = Some(3);
        let (from, through) = from_and_through(song, 2, 8, 3);
        same(&from, &through);
    }
}

#[test]
fn a_track_can_be_placed_past_where_the_row_has_got_to() {
    let mut engine = Engine::new(RATE, version(spanning()));
    engine.play_section(0, 1);
    engine.place_track(0, 4);
    let mut out = vec![0.0; 2 * STEP * 2];
    engine.process(&mut out);
    same(&[out[3000 * 2], out[(STEP + 3000) * 2]], &[BV, CV]);
}

#[test]
fn one_section_that_wraps_keeps_its_tracks_free_running() {
    let mut two = Song::new(2);
    two.sample_on(0, "s");
    two.sample_on(1, "s");
    two.tracks[1].phrases.insert(
        0,
        Arc::new(Phrase::from_steps(
            STEPS.iter().map(|text| text.parse().unwrap()).collect(),
        )),
    );
    two.tracks[0].phrases.insert(
        0,
        Arc::new(Phrase::from_steps(vec![
            "--- -- --- --- ---"
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
    let whole = song(&[&[A, "--- -- cha:80 --- ---"], &[B, C]]);
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
            engine.continue_in(1, 3);
        }
        engine.process(late);
        out
    };
    assert_eq!(play(true), play(false));
}

/// Play from the top with the song held on its row, launching after the first step, and give the
/// level in each step.
fn on_grid(mut song: Song, every: usize, steps: usize, chain: Option<u32>) -> Vec<f32> {
    song.launch = Launch::every(every).unwrap();
    jam(song, steps, 0, |engine| engine.launch(0, chain))
}

#[test]
fn a_launch_starts_on_the_next_grid_line_and_cuts_its_chain_there() {
    let out = on_grid(song(&[&[A, A, A, A, A, A], &[B, C]]), 4, 8, Some(11));
    same(&out, &[AV, AV, AV, AV, BV, CV, BV, CV]);
}

#[test]
fn every_step_is_a_line_on_a_grid_of_one() {
    let out = on_grid(song(&[&[A, A, A, A], &[B, C]]), 1, 4, Some(11));
    same(&out, &[AV, BV, CV, BV]);
}

#[test]
fn a_stop_waits_for_the_grid_too() {
    let out = on_grid(song(&[&[A, A, A, A]]), 2, 4, None);
    same(&out, &[AV, AV, 0.0, 0.0]);
}

#[test]
fn a_short_chain_loops_until_the_grid_line() {
    let mut short = song(&[&[A], &[B, C]]);
    short.sections[0].length = Some(8);
    let out = on_grid(short, 4, 7, Some(11));
    same(&out, &[AV, AV, AV, AV, BV, CV, BV]);
}

#[test]
fn a_row_shorter_than_the_grid_only_has_a_line_at_its_start() {
    let mut twelve = song(&[&[A, A, A], &[B, C]]);
    twelve.sections[0].length = Some(3);
    let out = on_grid(twelve, 16, 6, Some(11));
    same(&out, &[AV, AV, AV, BV, CV, BV]);
}

#[test]
fn launching_the_chain_playing_restarts_it_on_the_grid() {
    let out = on_grid(song(&[&[A, B, C, A, B, C]]), 4, 6, Some(10));
    same(&out, &[AV, BV, CV, AV, AV, BV]);
}

#[test]
fn the_grid_counts_from_the_row_so_a_reset_doesnt_move_it() {
    let mut reset = song(&[&[A, A, A, A, A, A, A, A], &[B, C]]);
    reset.sections[0].reset = Some(3);
    reset.launch = Launch::every(4).unwrap();
    let mut engine = Engine::new(RATE, version(reset));
    engine.set_loop_section(Some(0));
    engine.play_from(0);
    let mut out = vec![0.0; 3000 * 2];
    engine.process(&mut out);
    engine.launch(0, Some(11));
    let mut out = vec![0.0; 3 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.jam(0).cued, Some(Some(11)), "not at the reset");
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.jam(0).playing, Some(Some(11)), "on the row's step 4");
}

#[test]
fn a_cue_survives_the_song_moving_on_and_starts_with_the_new_row() {
    for launch in [Launch::ChainEnd, Launch::every(128).unwrap()] {
        let mut moving = song(&[&[A, A, A], &[C, C, C, C], &[B, C]]);
        moving.sections[0].length = Some(2);
        moving.launch = launch;
        let mut engine = Engine::new(RATE, version(moving));
        engine.play_from(0);
        let mut out = vec![0.0; 3000 * 2];
        engine.process(&mut out);
        engine.launch(0, Some(12));
        let mut rest = vec![0.0; 4 * STEP * 2];
        engine.process(&mut rest);
        out.extend_from_slice(&rest);
        let got: Vec<f32> = (0..4).map(|step| out[(step * STEP + 3000) * 2]).collect();
        same(&got, &[AV, AV, BV, CV]);
        assert_eq!(engine.jam(0).playing, Some(Some(12)));
    }
}

#[test]
fn a_cue_into_a_reset_row_plays_from_its_top() {
    let mut moving = song(&[&[A, A, A], &[C, C, C, C], &[B, C]]);
    moving.sections[0].length = Some(3);
    moving.sections[1].start = SectionStart::Reset;
    moving.launch = Launch::every(128).unwrap();
    let mut engine = Engine::new(RATE, version(moving));
    engine.play_from(0);
    let mut out = vec![0.0; 3000 * 2];
    engine.process(&mut out);
    engine.launch(0, Some(12));
    let mut rest = vec![0.0; 4 * STEP * 2];
    engine.process(&mut rest);
    out.extend_from_slice(&rest);
    let got: Vec<f32> = (0..4).map(|step| out[(step * STEP + 3000) * 2]).collect();
    same(&got, &[AV, AV, AV, BV]);
}

#[test]
fn playing_again_forgets_what_was_cued() {
    let mut engine = Engine::new(RATE, version(song(&[&[A, A], &[B, C]])));
    engine.play_from(0);
    engine.launch(0, Some(11));
    engine.play_from(0);
    assert_eq!(engine.jam(0).cued, None);
}

#[test]
fn a_row_of_nothing_with_a_set_length_is_a_rest() {
    let mut rest = song(&[&[A, A], &[B], &[C, C]]);
    rest.sections[1].chains = vec![None];
    rest.sections[1].length = Some(2);
    // Nothing plays over the last note, so it rings on until the next row's.
    same(&levels(rest, 6), &[AV, AV, AV, AV, CV, CV]);
}

#[test]
fn a_song_can_start_on_a_row_of_nothing() {
    let mut rest = song(&[&[A], &[B]]);
    rest.sections[0].chains = vec![None];
    rest.sections[0].length = Some(2);
    same(&levels(rest, 3), &[0.0, 0.0, BV]);
}

#[test]
fn a_song_starting_on_an_empty_row_plays_the_next() {
    let mut empty = song(&[&[A], &[B]]);
    empty.sections[0].chains = vec![None];
    same(&levels(empty, 2), &[BV, BV]);
}

#[test]
fn a_section_plays_its_repeats_before_moving_on() {
    let mut twice = song(&[&[A], &[B]]);
    twice.sections[0].length = Some(2);
    twice.sections[0].repeat = Repeat::new(2);
    same(&levels(twice, 5), &[AV, AV, AV, AV, BV]);
}

#[test]
fn chains_run_on_across_repeats_unless_the_section_resets() {
    let mut twice = song(&[&STEPS]);
    twice.sections[0].length = Some(2);
    twice.sections[0].repeat = Repeat::new(2);
    same(&levels(twice.clone(), 4), &[AV, BV, CV, AV]);
    twice.sections[0].start = SectionStart::Reset;
    same(&levels(twice, 4), &[AV, BV, AV, BV]);
}

#[test]
fn a_loop_keeps_a_section_past_its_repeats() {
    let mut twice = song(&[&[A], &[B]]);
    twice.sections[0].repeat = Repeat::new(2);
    let mut engine = Engine::new(RATE, version(twice));
    engine.set_loop_section(Some(0));
    engine.play_from(0);
    let mut out = vec![0.0; 5 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.section(), 0);
    engine.set_loop_section(None);
    let mut out = vec![0.0; 2 * STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.section(), 1, "it moves on when let go");
}

#[test]
fn lowering_a_repeat_while_past_it_ends_the_section_at_the_next_step() {
    let mut four = song(&[&[A], &[B]]);
    four.sections[0].repeat = Repeat::new(4);
    let mut engine = Engine::new(RATE, version(four.clone()));
    engine.play_from(0);
    let mut out = vec![0.0; (2 * STEP + 3000) * 2];
    engine.process(&mut out);
    four.sections[0].repeat = Repeat::new(1);
    let _ = engine.swap(version(four));
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.section(), 1);
}

#[test]
fn each_repeat_starts_the_launch_grid_again() {
    let mut twice = song(&[&[A, A, A], &[B, C]]);
    twice.sections[0].length = Some(3);
    twice.sections[0].repeat = Repeat::new(4);
    // A grid of 2 falls on steps 0 and 2 of each pass of 3. Launched after step 2's line, it waits
    // for the next pass rather than the fourth step of the row.
    twice.launch = Launch::every(2).unwrap();
    let mut engine = Engine::new(RATE, version(twice));
    engine.set_loop_section(Some(0));
    engine.play_from(0);
    let mut out = vec![0.0; (2 * STEP + 3000) * 2];
    engine.process(&mut out);
    engine.launch(0, Some(11));
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(
        engine.jam(0).playing,
        Some(Some(11)),
        "on step 3, a pass's start"
    );
}

#[test]
fn continuing_in_a_captured_section_lands_within_it() {
    let mut twice = song(&[&[A, A], &[B, B]]);
    twice.sections[0].length = Some(2);
    twice.sections[0].repeat = Repeat::new(4);
    twice.sections[1].length = Some(2);
    twice.sections[1].start = SectionStart::Reset;
    let mut engine = Engine::new(RATE, version(twice));
    engine.set_loop_section(Some(0));
    engine.play_from(0);
    let mut out = vec![0.0; (5 * STEP + 3000) * 2];
    engine.process(&mut out);
    let place = engine.position();
    engine.continue_in(1, 2);
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.section(), 1);
    assert_eq!(engine.position(), place + 1, "nothing reset on the way in");
}

#[test]
fn playing_from_a_row_counts_the_repeats_before_it() {
    let mut twice = song(&[&[A, B, C, A, B, C, A, B], &[C]]);
    twice.sections[0].length = Some(3);
    twice.sections[0].repeat = Repeat::new(2);
    twice.sections[1].chains = vec![Some(10)];
    twice.sections[1].length = Some(2);
    let (from, through) = from_and_through(twice.clone(), 1, 6, 2);
    same(&from, &through);
    twice.sections[0].start = SectionStart::Reset;
    let (from, through) = from_and_through(twice, 1, 6, 2);
    same(&from, &through);
}

#[test]
fn a_render_covers_repeats_and_stems_line_up() {
    let mut twice = song(&[&[A, A], &[B]]);
    twice.sections[0].repeat = Repeat::new(3);
    let whole = render_loop(version(twice.clone()), RATE, RenderOptions::default());
    assert_eq!(whole.len() / 2, 7 * STEP);
    let stems = render_stems(&version(twice), RATE, RenderOptions::default());
    assert!(stems.iter().all(|stem| stem.samples.len() == whole.len()));
}
