//! `LST` and `!LST`, after Elektron's (decision 419): a row that plays only on the last pass of
//! its phrase before the track moves on to another phrase or chain, or on every pass but that
//! one. The chain's next phrase, the section's end, a chain launched and a loop of sections
//! waiting each move it on; a chain that loops on its own has no last pass. A reset or a realign
//! moves it on only if the chain's first phrase is another (decision 436).

use std::sync::Arc;

use vt_engine::engine::{Engine, SectionLoop};
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Launch, Phrase, Repeat, Section, SectionStart, Song};

use crate::support::{self, version};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

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

/// A phrase of four rows whose first plays on its last pass.
const LAST: u32 = 1;
/// The same, playing on every pass but its last.
const NOT_LAST: u32 = 2;
/// Four rows whose last plays on its phrase's last pass, so a change queued partway shows.
const LAST_AT_END: u32 = 3;
/// Four empty rows.
const QUIET: u32 = 4;

/// One track with the four phrases, a chain for each list of phrases, numbered from 0, and a
/// section for each chain and length, its longest chain's if `None`.
fn song(chains: &[&[u32]], sections: &[(u32, Option<usize>)]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let quiet = "--- --";
    for (id, rows) in [
        (LAST, ["C-3 -- cnd:09", quiet, quiet, quiet]),
        (NOT_LAST, ["C-3 -- cnd:0A", quiet, quiet, quiet]),
        (LAST_AT_END, [quiet, quiet, quiet, "C-3 -- cnd:09"]),
        (QUIET, [quiet; 4]),
    ] {
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
        track
            .phrases
            .insert(id, Arc::new(Phrase::from_steps(steps)));
    }
    track.chains.clear();
    for (id, phrases) in chains.iter().enumerate() {
        let entries = phrases
            .iter()
            .map(|&phrase| ChainEntry {
                phrase,
                transpose: 0,
            })
            .collect();
        track.chains.insert(
            id as u32,
            Arc::new(Chain {
                entries,
                label: String::new(),
            }),
        );
    }
    song.sections = sections
        .iter()
        .map(|&(chain, length)| Section {
            chains: vec![Some(chain)],
            length,
            ..Section::default()
        })
        .collect();
    song
}

/// Which of so many steps a note starts on.
fn heard(out: &[f32], steps: usize) -> Vec<usize> {
    (0..steps)
        .filter(|step| out[step * STEP * 2].abs() > 0.001)
        .collect()
}

/// Which steps a note starts on, playing the song from the top.
fn hits(song: &Song, steps: usize) -> Vec<usize> {
    heard(
        &render_frames(version(song, hits_pool()), RATE, steps * STEP),
        steps,
    )
}

/// Which steps a note starts on, playing from the top with the first section looping if `looped`,
/// and doing something halfway through step `at`.
fn playing(
    song: &Song,
    steps: usize,
    looped: bool,
    at: usize,
    act: impl Fn(&mut Engine),
) -> Vec<usize> {
    let mut engine = Engine::new(RATE, version(song, hits_pool()));
    if looped {
        engine.set_loop_section(Some(0));
    }
    engine.play_from(0);
    let mut out = vec![0.0; (at * STEP + STEP / 2) * 2];
    engine.process(&mut out);
    act(&mut engine);
    let mut after = vec![0.0; (steps * STEP - out.len() / 2) * 2];
    engine.process(&mut after);
    out.extend_from_slice(&after);
    heard(&out, steps)
}

#[test]
fn the_last_pass_before_the_chain_moves_to_another_phrase() {
    // Twice round the phrase, then a quiet one: the second time is the last.
    let twice = song(&[&[LAST, LAST, QUIET]], &[(0, None)]);
    assert_eq!(hits(&twice, 24), [4, 16]);
    let not = song(&[&[NOT_LAST, NOT_LAST, QUIET]], &[(0, None)]);
    assert_eq!(hits(&not, 24), [0, 12]);
    // With another phrase after it every time, every pass is its last.
    let between = song(&[&[LAST, QUIET]], &[(0, None)]);
    assert_eq!(hits(&between, 16), [0, 8]);
}

#[test]
fn a_chain_looping_on_its_own_has_no_last_pass() {
    let looping = song(&[&[LAST]], &[(0, None)]);
    assert_eq!(hits(&looping, 16), Vec::<usize>::new());
    let not = song(&[&[NOT_LAST]], &[(0, None)]);
    assert_eq!(hits(&not, 16), [0, 4, 8, 12]);
}

#[test]
fn the_last_pass_before_the_section_moves_to_another_chain() {
    // Four passes of the chain, then a section of a quiet one, and round again.
    let sections = song(&[&[LAST], &[QUIET]], &[(0, Some(16)), (1, Some(4))]);
    assert_eq!(hits(&sections, 40), [12, 32]);
    // A section with the same chain carries it on, so it isn't moving on.
    let same = song(&[&[LAST]], &[(0, Some(16)), (0, Some(4))]);
    assert_eq!(hits(&same, 40), Vec::<usize>::new());
    // A section's repeats play before it moves on.
    let mut repeated = sections;
    repeated.sections[0].repeat = Repeat::new(2);
    assert_eq!(hits(&repeated, 40), [28]);
}

#[test]
fn looping_the_section_leaves_it_no_last_pass() {
    let sections = song(&[&[LAST], &[QUIET]], &[(0, Some(16)), (1, Some(4))]);
    assert_eq!(playing(&sections, 40, true, 0, |_| {}), Vec::<usize>::new());
}

#[test]
fn a_loop_of_sections_waiting_makes_the_pass_it_lands_in_the_last() {
    let mut sections = song(&[&[LAST], &[QUIET]], &[(0, Some(16)), (1, Some(4))]);
    sections.launch = Launch::Every(std::num::NonZeroU32::new(8).unwrap());
    assert_eq!(playing(&sections, 24, false, 0, |_| {}), [12]);
    // Looping the quiet section, asked for in the first pass, lands on the line at step 8.
    let cued = playing(&sections, 24, false, 1, |engine| {
        engine.loop_sections(Some(SectionLoop::one(1)));
    });
    assert_eq!(cued, [4]);
}

#[test]
fn a_launch_waiting_makes_the_pass_it_lands_in_the_last() {
    // A chain of one phrase, looped, with its row at the phrase's end.
    let mut looped = song(&[&[LAST_AT_END], &[QUIET]], &[(0, Some(16))]);
    looped.launch = Launch::ChainEnd;
    assert_eq!(playing(&looped, 16, true, 1, |_| {}), Vec::<usize>::new());
    // Launched at the chain's end, the pass playing is its last.
    let launched = playing(&looped, 16, true, 1, |engine| engine.launch(0, Some(1)));
    assert_eq!(launched, [3]);
    let stopped = playing(&looped, 16, true, 1, |engine| engine.launch(0, None));
    assert_eq!(stopped, [3]);
    // Launching the chain playing starts it again, which isn't moving on.
    let again = playing(&looped, 16, true, 1, |engine| engine.launch(0, Some(0)));
    assert_eq!(again, Vec::<usize>::new());
    // On a grid of eight steps, the launch lands at step 8, so the second pass is the last.
    looped.launch = Launch::Every(std::num::NonZeroU32::new(8).unwrap());
    let on_grid = playing(&looped, 16, true, 1, |engine| engine.launch(0, Some(1)));
    assert_eq!(on_grid, [7]);
}

#[test]
fn a_window_waiting_under_cue_isnt_moving_on() {
    let mut cued = song(&[&[LAST]], &[(0, None)]);
    cued.tracks[0].cue = 1;
    let mut turned = cued.clone();
    turned.tracks[0].phrase_length = 0x7E;
    let heard = playing(&cued, 16, false, 1, |engine| {
        engine.swap(version(&turned, hits_pool()));
    });
    assert_eq!(heard, Vec::<usize>::new());
}

#[test]
fn a_section_starting_again_sends_the_chain_back_to_its_first_phrase() {
    // Each repeat of a resetting section of four steps starts the chain again before its quiet
    // phrase plays, so the first phrase never moves on.
    let mut reset = song(&[&[LAST, QUIET]], &[(0, Some(4))]);
    reset.sections[0].repeat = Repeat::new(4);
    reset.sections[0].start = SectionStart::Reset;
    assert_eq!(hits(&reset, 16), Vec::<usize>::new());
    let mut not = song(&[&[NOT_LAST, QUIET]], &[(0, Some(4))]);
    not.sections[0].repeat = Repeat::new(4);
    not.sections[0].start = SectionStart::Reset;
    assert_eq!(hits(&not, 16), [0, 4, 8, 12]);
    // A song of one such section starts it again each time it comes in.
    reset.sections[0].repeat = Repeat::new(1);
    assert_eq!(hits(&reset, 16), Vec::<usize>::new());
    // Without the reset the chain carries on through the repeats, to its quiet phrase.
    reset.sections[0].start = SectionStart::default();
    assert_eq!(hits(&reset, 16), [0, 8]);
}

#[test]
fn a_reset_sends_the_chain_back_to_its_first_phrase() {
    let mut reset = song(&[&[LAST, QUIET]], &[(0, Some(16))]);
    reset.sections[0].reset = Some(4);
    assert_eq!(hits(&reset, 16), Vec::<usize>::new());
    // A reset after the quiet phrase has played leaves the pass before it the last.
    reset.sections[0].reset = Some(8);
    assert_eq!(hits(&reset, 16), [0, 8]);
}

#[test]
fn the_tracks_own_reset_sends_the_chain_back_to_its_first_phrase() {
    let mut reset = song(&[&[LAST, QUIET]], &[(0, Some(16))]);
    reset.tracks[0].reset = Some(4);
    assert_eq!(hits(&reset, 16), Vec::<usize>::new());
    // Back to another phrase is moving on, even partway through the pass: the second phrase is
    // cut short at step 6 by the chain starting again.
    let mut other = song(&[&[QUIET, LAST]], &[(0, Some(16))]);
    other.tracks[0].reset = Some(6);
    assert_eq!(hits(&other, 16), [4, 10]);
}

#[test]
fn a_realign_waiting_sends_the_chain_back_to_its_first_phrase() {
    // The row at the phrase's end, with the realign landing at its pass's end on a grid of four.
    let mut realigned = song(&[&[LAST_AT_END, QUIET]], &[(0, Some(16))]);
    realigned.launch = Launch::Every(std::num::NonZeroU32::new(4).unwrap());
    assert_eq!(playing(&realigned, 16, false, 1, |_| {}), [3, 11]);
    let every = playing(&realigned, 16, false, 1, Engine::toggle_realign);
    assert_eq!(every, [7, 15]);
    let own = playing(&realigned, 16, false, 1, |engine| {
        engine.toggle_realign_track(0);
    });
    assert_eq!(own, [7, 15]);
}

#[test]
fn renders_with_last_repeat() {
    let song = song(
        &[&[LAST, NOT_LAST, LAST, QUIET], &[LAST_AT_END]],
        &[(0, None), (1, Some(8))],
    );
    support::repeats("last", || {
        render_frames(version(&song, hits_pool()), RATE, 48 * STEP)
    });
}
