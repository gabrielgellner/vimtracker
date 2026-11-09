//! One-off files: a sample on no track, played through a track's preview voices, as the sample
//! browser plays the file its cursor moves onto.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::link::{Command, connect};
use vt_engine::published::Published;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

const SECOND: usize = 48_000;

fn flat(frames: usize, level: f32) -> Arc<SampleData> {
    Arc::new(SampleData {
        frames: vec![[level, level]; frames],
    })
}

/// An engine on a song of two tracks with no samples.
fn engine() -> Engine {
    let song = Arc::new(Song::new(2));
    let version = Published::new(song, Arc::new(SamplePool::default()));
    Engine::new(48_000, Arc::new(version))
}

/// The left channel's last frame after this many frames.
fn level(engine: &mut Engine, frames: usize) -> f32 {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out[out.len() - 2].abs()
}

#[test]
fn a_file_on_no_track_plays_through_one_to_its_end() {
    let mut engine = engine();
    assert_eq!(
        engine.play_file(0, flat(SECOND, 0.5)),
        None,
        "nothing replaced"
    );
    assert!(level(&mut engine, 2000) > 0.05, "it sounds");
    assert_eq!(engine.previews_sounding(), 1);
    assert!(level(&mut engine, SECOND) < 1e-4, "and ends with the file");
    assert_eq!(engine.previews_sounding(), 0);
}

#[test]
fn the_next_file_fades_the_last_out_and_gives_back_the_one_it_replaces() {
    let mut engine = engine();
    let first = flat(SECOND * 4, 0.5);
    let second = flat(SECOND * 4, 0.25);
    let third = flat(SECOND * 4, 0.125);
    assert!(engine.play_file(0, Arc::clone(&first)).is_none());
    level(&mut engine, 2000);
    assert!(
        engine.play_file(0, Arc::clone(&second)).is_none(),
        "a place free"
    );
    level(&mut engine, 2000);
    assert_eq!(engine.previews_sounding(), 1, "the first has faded out");
    let given = engine.play_file(0, third).expect("the first is given back");
    assert!(Arc::ptr_eq(&given, &first));
    assert!(level(&mut engine, 2000) > 0.01, "the third plays");
    assert_eq!(engine.previews_sounding(), 1);
}

#[test]
fn stopping_lets_the_file_go() {
    let mut engine = engine();
    engine.play_file(1, flat(SECOND * 4, 0.5));
    assert!(level(&mut engine, 2000) > 0.05);
    engine.stop_file();
    assert!(level(&mut engine, SECOND / 4) < 1e-4, "it's released");
}

#[test]
fn a_track_that_makes_no_sound_gives_the_file_back() {
    let mut engine = engine();
    let file = flat(SECOND, 0.5);
    // The fixed tracks after the sound tracks, such as the returns, play no notes.
    let given = engine.play_file(2, Arc::clone(&file)).unwrap();
    assert!(Arc::ptr_eq(&given, &file));
    assert_eq!(engine.previews_sounding(), 0);
}

#[test]
fn a_file_crosses_to_the_callback_and_comes_back_to_be_freed() {
    let (mut handle, mut realtime) = connect(engine());
    let first = flat(SECOND * 4, 0.5);
    assert!(handle.play_file(0, Arc::clone(&first)));
    let mut out = vec![0.0; 2000];
    realtime.process(&mut out);
    assert!(out[out.len() - 2].abs() > 0.05, "it plays");
    assert_eq!(Arc::strong_count(&first), 2, "the callback holds it");
    handle.play_file(0, flat(SECOND, 0.25));
    realtime.process(&mut out);
    handle.play_file(0, flat(SECOND, 0.25));
    realtime.process(&mut out);
    handle.pump();
    assert_eq!(Arc::strong_count(&first), 1, "given back, and freed here");
    handle.send(Command::StopFile);
    let mut later = vec![0.0; SECOND / 2];
    realtime.process(&mut later);
    assert!(later[later.len() - 2].abs() < 1e-4, "stopped");
}
