//! One-off files: a sample on no track, played through a track's preview voices, as the sample
//! browser plays the file its cursor moves onto.

use std::sync::Arc;

use vt_engine::engine::{Engine, Playhead};
use vt_engine::link::{Command, connect};
use vt_engine::published::{Published, Span};
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
        engine.play_file(0, flat(SECOND, 0.5), None),
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
    assert!(engine.play_file(0, Arc::clone(&first), None).is_none());
    level(&mut engine, 2000);
    assert!(
        engine.play_file(0, Arc::clone(&second), None).is_none(),
        "a place free"
    );
    level(&mut engine, 2000);
    assert_eq!(engine.previews_sounding(), 1, "the first has faded out");
    let given = engine
        .play_file(0, third, None)
        .expect("the first is given back");
    assert!(Arc::ptr_eq(&given, &first));
    assert!(level(&mut engine, 2000) > 0.01, "the third plays");
    assert_eq!(engine.previews_sounding(), 1);
}

#[test]
fn stopping_lets_the_file_go() {
    let mut engine = engine();
    engine.play_file(1, flat(SECOND * 4, 0.5), None);
    assert!(level(&mut engine, 2000) > 0.05);
    engine.stop_file();
    assert!(level(&mut engine, SECOND / 4) < 1e-4, "it's released");
}

#[test]
fn a_track_that_makes_no_sound_gives_the_file_back() {
    let mut engine = engine();
    let file = flat(SECOND, 0.5);
    // The fixed tracks after the sound tracks, such as the returns, play no notes.
    let given = engine.play_file(2, Arc::clone(&file), None).unwrap();
    assert!(Arc::ptr_eq(&given, &file));
    assert_eq!(engine.previews_sounding(), 0);
}

#[test]
fn a_file_crosses_to_the_callback_and_comes_back_to_be_freed() {
    let (mut handle, mut realtime) = connect(engine());
    let first = flat(SECOND * 4, 0.5);
    assert!(handle.play_file(0, Arc::clone(&first), None));
    let mut out = vec![0.0; 2000];
    realtime.process(&mut out);
    assert!(out[out.len() - 2].abs() > 0.05, "it plays");
    assert_eq!(Arc::strong_count(&first), 2, "the callback holds it");
    handle.play_file(0, flat(SECOND, 0.25), None);
    realtime.process(&mut out);
    handle.play_file(0, flat(SECOND, 0.25), None);
    realtime.process(&mut out);
    handle.pump();
    assert_eq!(Arc::strong_count(&first), 1, "given back, and freed here");
    handle.send(Command::StopFile);
    let mut later = vec![0.0; SECOND / 2];
    realtime.process(&mut later);
    assert!(later[later.len() - 2].abs() < 1e-4, "stopped");
}

#[test]
fn an_input_plays_through_a_track_as_it_is_with_no_fades() {
    let mut engine = engine();
    let ramp: Vec<[f32; 2]> = (0..2000)
        .map(|frame| {
            let value = 0.25 + frame as f32 / 8000.0;
            [value, -value]
        })
        .collect();
    let input = Arc::new(SampleData {
        frames: ramp.clone(),
    });
    assert!(engine.play_input(0, input).is_none());
    let mut out = vec![0.0; 2100 * 2];
    engine.process(&mut out);
    for (frame, [left, right]) in ramp.iter().enumerate() {
        // Within a float's rounding of the track's unity level.
        assert!(
            (out[frame * 2] - left).abs() < 1e-6,
            "frame {frame}, from the first, at full level"
        );
        assert!((out[frame * 2 + 1] - right).abs() < 1e-6);
    }
    assert_eq!(out[2000 * 2], 0.0, "and it stops at its end, with no tail");
}

/// A file of 4000 frames that sounds only from frame 1000 to frame 2000, so what's heard says
/// which of its frames are played.
fn sounding_in_the_middle() -> Arc<SampleData> {
    let frames = (0..4000)
        .map(|frame| {
            let level = if (1000..2000).contains(&frame) {
                0.5
            } else {
                0.0
            };
            [level, level]
        })
        .collect();
    Arc::new(SampleData { frames })
}

/// A span of a file, the way it goes.
fn range(frames: u64, start: u64, end: u64, reverse: bool) -> Span {
    Span {
        start,
        end,
        reverse,
        ..Span::whole(frames)
    }
}

/// Each frame's level on the left, for this many frames.
fn levels(engine: &mut Engine, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    out.iter().step_by(2).map(|level| level.abs()).collect()
}

/// Where the one voice playing through the first track is in its file.
fn playhead(engine: &Engine) -> Option<u64> {
    let heads = engine.playheads(0);
    assert!(heads[1..].iter().all(Option::is_none), "{heads:?}");
    heads[0].map(|head| {
        assert_eq!(head.entry, None, "a one-off file has no entry");
        head.frame
    })
}

#[test]
fn a_range_of_a_file_plays_exactly_that_range_forward() {
    let mut engine = engine();
    let file = sounding_in_the_middle();
    engine.play_file(0, Arc::clone(&file), Some(range(4000, 1000, 2000, false)));
    assert_eq!(
        playhead(&engine),
        Some(1000),
        "it starts at the range's start"
    );
    let heard = levels(&mut engine, 100);
    assert!(heard[90] > 0.05, "it plays the sound from its first frame");
    assert_eq!(playhead(&engine), Some(1100));
    let heard = levels(&mut engine, 1000);
    assert!(heard[800] > 0.05, "all of it");
    // It ends at the range's end, faded over a declick, and the voice is free.
    assert!(heard[999] < 1e-4, "{}", heard[999]);
    assert_eq!(playhead(&engine), None);
    assert_eq!(engine.previews_sounding(), 0);
    // A range where the file is silent plays silence.
    engine.play_file(0, file, Some(range(4000, 0, 1000, false)));
    assert!(levels(&mut engine, 1100).iter().all(|level| *level < 1e-4));
}

#[test]
fn a_reversed_range_plays_from_its_end_back_to_its_start() {
    let mut engine = engine();
    engine.play_file(
        0,
        sounding_in_the_middle(),
        Some(range(4000, 500, 1500, true)),
    );
    assert_eq!(playhead(&engine), Some(1499), "from its last frame");
    let heard = levels(&mut engine, 400);
    assert!(heard[300] > 0.05, "back through the sound");
    assert_eq!(playhead(&engine), Some(1099), "going back");
    let heard = levels(&mut engine, 700);
    assert!(
        heard[200..].iter().all(|level| *level < 1e-4),
        "then the silence before it"
    );
    assert_eq!(playhead(&engine), None, "and it ends at the range's start");
}

#[test]
fn the_report_says_where_a_file_is_on_the_track_watched() {
    let (mut handle, mut realtime) = connect(engine());
    let span = range(4000, 1000, 2000, false);
    assert!(handle.play_file(0, sounding_in_the_middle(), Some(span)));
    let mut out = vec![0.0; 200];
    realtime.process(&mut out);
    let report = handle.latest().expect("a report");
    assert_eq!(report.watched, 0);
    assert_eq!(
        report.playheads[0],
        Some(Playhead {
            entry: None,
            frame: 1100
        })
    );
    // Another track's voices aren't the watched track's.
    handle.send(Command::Watch { track: 1 });
    realtime.process(&mut out);
    let report = handle.latest().expect("a report");
    assert!(report.playheads.iter().all(Option::is_none));
}
