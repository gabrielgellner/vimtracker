//! Looping a range of sections while jamming: `:loop` over sections keeps them playing in turn,
//! from the next line of the launch grid, and `:loop` alone goes back to the song.

use std::sync::Arc;

use vt_engine::engine::{Engine, SectionLoop};
use vt_engine::published::Published;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::{Chain, ChainEntry, Launch, Phrase, Section, Song};

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

/// One track, and four sections of four steps, each with a chain of its own whose every step has
/// the section's velocity, so a step's level says which section played it, and where in its chain:
/// a chain's first step is louder than the rest.
fn song(launch: Launch) -> Song {
    let mut song = Song::new(1);
    song.sample_on(0, "s");
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    song.limiter = false;
    song.launch = launch;
    song.sections.clear();
    for section in 0..4u32 {
        let id = section + 10;
        let velocity = 0x10 * (section + 1);
        let rows = [
            format!("C-3 {:02X}", velocity + 8),
            format!("C-3 {velocity:02X}"),
            format!("C-3 {velocity:02X}"),
            format!("C-3 {velocity:02X}"),
        ];
        let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
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

fn engine(launch: Launch) -> Engine {
    Engine::new(
        RATE,
        Arc::new(Published::new(Arc::new(song(launch)), samples())),
    )
}

/// The section each of so many steps played in, and whether it was its chain's first step, from
/// its level.
fn steps(engine: &mut Engine, count: usize) -> Vec<(usize, bool)> {
    (0..count)
        .map(|_| {
            let mut out = vec![0.0; STEP * 2];
            engine.process(&mut out);
            let velocity = out[3000 * 2] * 0x7F as f32;
            let section = (velocity / 16.0 + 0.01).floor() as usize - 1;
            let first = velocity - 16.0 * (section as f32 + 1.0) > 4.0;
            (section, first)
        })
        .collect()
}

/// The sections only.
fn sections(engine: &mut Engine, count: usize) -> Vec<usize> {
    steps(engine, count).into_iter().map(|(at, _)| at).collect()
}

#[test]
fn a_loop_of_sections_2_to_3_plays_2_3_2_3() {
    let mut engine = engine(Launch::default());
    engine.loop_sections(Some(SectionLoop::new(2, 1)));
    engine.play_section(1, 0);
    let heard = steps(&mut engine, 16);
    let expected: Vec<(usize, bool)> = [1, 2, 1, 2]
        .iter()
        .flat_map(|&section| (0..4).map(move |step| (section, step == 0)))
        .collect();
    assert_eq!(heard, expected, "each section from its chain's top");
}

#[test]
fn a_loop_set_while_playing_starts_on_the_next_launch_line() {
    // A line every 2 steps: asked for in step 1 of section 1, it starts section 3 at step 2.
    let mut engine = engine(Launch::every(2).unwrap());
    engine.play_from(0);
    assert_eq!(sections(&mut engine, 1), [0]);
    let mut out = vec![0.0; STEP];
    engine.process(&mut out);
    engine.loop_sections(Some(SectionLoop::new(2, 3)));
    assert!(engine.loop_waiting());
    engine.process(&mut out);
    assert_eq!(engine.section(), 0, "not before the line");
    let heard = steps(&mut engine, 9);
    assert!(!engine.loop_waiting());
    assert_eq!(heard[0], (2, true), "from its first section's top");
    assert_eq!(
        heard.iter().map(|(at, _)| *at).collect::<Vec<_>>(),
        [2, 2, 2, 2, 3, 3, 3, 3, 2]
    );
}

#[test]
fn changing_the_range_while_it_plays_takes_effect_at_the_next_launch_line() {
    let mut engine = engine(Launch::every(2).unwrap());
    engine.loop_sections(Some(SectionLoop::new(1, 2)));
    engine.play_section(1, 0);
    assert_eq!(sections(&mut engine, 1), [1]);
    // Still in the loop, a range that holds the section playing changes what comes next only,
    // and at once.
    engine.loop_sections(Some(SectionLoop::new(0, 1)));
    assert!(!engine.loop_waiting());
    assert_eq!(sections(&mut engine, 4), [1, 1, 1, 0]);
    // One that doesn't starts its first section on the next line, here partway into section 0.
    engine.loop_sections(Some(SectionLoop::one(3)));
    assert_eq!(sections(&mut engine, 6), [0, 3, 3, 3, 3, 3]);
    assert_eq!(engine.looping(), Some(SectionLoop::one(3)));
}

#[test]
fn loop_alone_goes_back_to_the_song() {
    let mut engine = engine(Launch::default());
    engine.loop_sections(Some(SectionLoop::new(0, 1)));
    engine.play_from(0);
    assert_eq!(sections(&mut engine, 10), [0, 0, 0, 0, 1, 1, 1, 1, 0, 0]);
    // At once: the section playing plays out and the song moves on.
    engine.loop_sections(None);
    assert_eq!(engine.looping(), None);
    assert_eq!(sections(&mut engine, 10), [0, 0, 1, 1, 1, 1, 2, 2, 2, 2]);
}

#[test]
fn stopping_makes_a_waiting_loop_the_loop() {
    let mut engine = engine(Launch::default());
    engine.play_from(0);
    engine.process(&mut vec![0.0; STEP * 2]);
    engine.loop_sections(Some(SectionLoop::new(2, 3)));
    assert!(engine.loop_waiting());
    engine.pause_or_resume();
    assert!(!engine.loop_waiting());
    assert_eq!(engine.looping(), Some(SectionLoop::new(2, 3)));
}
