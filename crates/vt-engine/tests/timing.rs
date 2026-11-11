//! Timing where the engine's time goes, for `vimtracker bench --tracks`: it changes nothing that's
//! heard, and it gives the time to the parts that took it.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::insert::{Distort, DistortType, Insert};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

const RATE: u32 = 48_000;
const BLOCK: usize = 512;

/// A song with one track playing a note on every step through an overdrive in its second slot.
fn version() -> Arc<Published> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: (0..RATE as usize)
                .map(|frame| [(frame as f32 * 0.01).sin() * 0.5; 2])
                .collect(),
        },
    );
    let mut song = Song::new(1);
    song.sample_on(0, "s");
    song.tracks[0].inserts[1] = Insert::Distort(Distort {
        kind: DistortType::Overdrive,
        ..Distort::default()
    });
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    for index in 0..16 {
        *phrase.step_mut(index).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    }
    Arc::new(Published::new(Arc::new(song), Arc::new(pool)))
}

/// A second of the song from the start, in blocks, timed or not.
fn play(timed: bool) -> (Vec<f32>, Engine) {
    let mut engine = Engine::new(RATE, version());
    engine.play_from(0);
    if timed {
        engine.time_parts();
    }
    let mut out = Vec::new();
    let mut block = [0.0_f32; BLOCK * 2];
    for _ in 0..RATE as usize / BLOCK {
        engine.process(&mut block);
        out.extend_from_slice(&block);
    }
    (out, engine)
}

#[test]
fn timing_the_parts_changes_nothing_heard() {
    let (plain, mut engine) = play(false);
    let (timed, _) = play(true);
    assert!(plain.iter().any(|sample| *sample != 0.0));
    assert_eq!(plain, timed);
    assert!(
        engine.part_times().is_none(),
        "nothing is timed unless asked"
    );
}

#[test]
fn the_time_goes_to_the_parts_that_took_it() {
    let (_, mut engine) = play(true);
    let times = engine.part_times().unwrap();
    let frames = (RATE as usize / BLOCK * BLOCK) as u64;
    // The track's voice and the rest of its work are timed each frame it plays.
    assert!(times.voices[0] > std::time::Duration::ZERO);
    assert!(times.voice_laps[0] > frames / 2);
    assert!(times.shaping[0] > std::time::Duration::ZERO);
    // Only the slot with an effect in it is timed.
    assert_eq!(times.slot_laps[0][0], 0);
    assert_eq!(times.slot_laps[0][1], times.shaping_laps[0]);
    assert!(times.slots[0][1] > std::time::Duration::ZERO);
    assert!(times.slots[0][1] <= times.shaping[0]);
    // The master and the limiter run every frame, and the sequencing once a block at least.
    let master = engine.current().song.tracks.len() - 1;
    assert_eq!(times.shaping_laps[master], frames);
    assert_eq!(times.limiting_laps, frames);
    assert!(times.sequencing_laps >= (RATE as usize / BLOCK) as u64);
}
