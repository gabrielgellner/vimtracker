//! The recorder: the master, a track or the input into a sample that grows as it's written, which
//! a pattern can play before it has stopped (#288).

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::recorder::{Ending, RecordQuantise, RecordSource, Recorded, RecorderEvent};
use vt_model::growing::Growing;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
const BLOCK: usize = 256;
/// A step at 120 BPM, in frames.
const STEP: usize = 6000;

/// Two synth tracks, each playing its own notes, with the limiter off.
fn song() -> Song {
    song_of(2)
}

/// So many tracks, the first two synths playing their own notes, with the limiter off.
fn song_of(tracks: usize) -> Song {
    let mut song = Song::new(tracks);
    song.limiter = false;
    let notes = [
        [
            "C-3 7F --- --- ---",
            "--- -- --- --- ---",
            "E-3 60 --- --- ---",
        ],
        [
            "G-2 7F --- --- ---",
            "C-4 50 --- --- ---",
            "--- -- --- --- ---",
        ],
    ];
    for (track, notes) in notes.iter().enumerate() {
        song.tracks[track].machine = Machine::Plaits(SynthVoice::default());
        let steps = bar(notes);
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    song
}

fn engine_with(song: Song, pool: SamplePool) -> Engine {
    Engine::new(
        RATE,
        Arc::new(Published::new(Arc::new(song), Arc::new(pool))),
    )
}

/// A sample with room for so many frames, all of its chunks in place.
fn take(frames: u64) -> Arc<Growing> {
    let growing = Growing::new(frames);
    growing.make_room(frames);
    Arc::new(growing)
}

/// A bar of steps, these first and rests after.
fn bar(steps: &[&str]) -> Vec<vt_model::step::Step> {
    let mut steps: Vec<vt_model::step::Step> =
        steps.iter().map(|text| text.parse().unwrap()).collect();
    steps.resize(16, vt_model::step::Step::default());
    steps
}

/// So many blocks, in frames.
fn blocks(count: usize) -> usize {
    count * BLOCK
}

/// Run so many frames a block at a time, keeping every frame played and the recorder's end.
fn run(engine: &mut Engine, frames: usize, out: &mut Vec<f32>, ended: &mut Option<Recorded>) {
    assert_eq!(frames % BLOCK, 0, "whole blocks");
    let mut block = vec![0.0; BLOCK * 2];
    for _ in 0..frames / BLOCK {
        engine.process(&mut block);
        out.extend_from_slice(&block);
        while let Some(event) = engine.take_recorder_event() {
            if let RecorderEvent::Ended(recorded) = event {
                *ended = Some(recorded);
            }
        }
    }
}

/// Record a source of the song from the top, on the bar, for so many frames, and stop there.
fn record(source: RecordSource, frames: usize) -> (Recorded, Vec<f32>) {
    let mut engine = engine_with(song(), SamplePool::default());
    engine.record(take(RATE as u64 * 4), source, RecordQuantise::Bar, 0);
    engine.play_from(0);
    let (mut out, mut ended) = (Vec::new(), None);
    run(&mut engine, frames, &mut out, &mut ended);
    engine.stop_recording(RecordQuantise::Now, 0);
    run(&mut engine, blocks(200), &mut out, &mut ended);
    (ended.expect("it ended"), out)
}

/// The frames of a recording from its start to its end, interleaved.
fn proper(recorded: &Recorded) -> Vec<f32> {
    let frames = recorded.take.to_frames();
    frames[recorded.before as usize..recorded.end as usize]
        .iter()
        .flatten()
        .copied()
        .collect()
}

#[test]
fn a_recording_of_a_track_matches_an_offline_render_of_it_frame_for_frame() {
    let length = blocks(70);
    let (recorded, _) = record(RecordSource::Track(1), length);
    assert_eq!(recorded.ending, Ending::Stopped);
    // Recorded on the bar from the top, with nothing to compensate for, it starts on the song's
    // first frame.
    let mut alone = engine_with(song(), SamplePool::default());
    alone.set_solo(1, true);
    alone.play_from(0);
    let mut offline = vec![0.0; length * 2];
    alone.process(&mut offline);
    let recording = proper(&recorded);
    assert_eq!(recording.len(), offline.len());
    assert!(offline.iter().any(|sample| sample.abs() > 0.05), "it plays");
    assert_eq!(recording, offline);
}

#[test]
fn a_recording_of_the_master_is_what_played() {
    let length = blocks(47);
    let (recorded, out) = record(RecordSource::Master, length);
    assert_eq!(proper(&recorded), out[..length * 2]);
    // The margin before the song's start is silence, and the one past the end what came next.
    let frames = recorded.take.to_frames();
    assert!(frames[..recorded.before as usize] == vec![[0.0; 2]; recorded.before as usize]);
    let after: Vec<f32> = frames[recorded.end as usize..]
        .iter()
        .flatten()
        .copied()
        .collect();
    let margin = recorded.before as usize;
    assert_eq!(after.len(), margin * 2);
    assert_eq!(after, out[length * 2..(length + margin) * 2]);
}

#[test]
fn the_input_recorded_with_its_latency_lines_up_with_what_played() {
    // The input hears the master again, as a cable from the outputs to the inputs would, this many
    // frames late: the time out through the speakers and back in.
    let latency = 300;
    let length = blocks(47);
    let (master, _) = record(RecordSource::Master, length);
    let mut engine = engine_with(song(), SamplePool::default());
    engine.record(
        take(RATE as u64 * 4),
        RecordSource::Input,
        RecordQuantise::Bar,
        latency as u64,
    );
    engine.play_from(0);
    let mut played: Vec<[f32; 2]> = Vec::new();
    let mut ended = None;
    let mut block = vec![0.0; BLOCK * 2];
    let mut stopped = false;
    while ended.is_none() {
        if !stopped && played.len() >= length {
            engine.stop_recording(RecordQuantise::Now, latency as u64);
            stopped = true;
        }
        let now = played.len();
        let room = engine.input_room(BLOCK);
        for (at, frame) in room.iter_mut().enumerate() {
            *frame = (now + at)
                .checked_sub(latency)
                .map_or([0.0; 2], |heard| played[heard]);
        }
        engine.process(&mut block);
        played.extend(block.as_chunks::<2>().0);
        while let Some(event) = engine.take_recorder_event() {
            if let RecorderEvent::Ended(recorded) = event {
                ended = Some(recorded);
            }
        }
    }
    let input = ended.unwrap();
    assert_eq!((input.before, input.end), (master.before, master.end));
    assert_eq!(input.take.to_frames(), master.take.to_frames());
}

#[test]
fn the_margin_before_a_start_holds_the_frames_before_it() {
    let mut engine = engine_with(song(), SamplePool::default());
    engine.play_from(0);
    let (mut out, mut ended) = (Vec::new(), None);
    run(&mut engine, blocks(70), &mut out, &mut ended);
    let start = out.len() / 2;
    engine.record(
        take(RATE as u64),
        RecordSource::Master,
        RecordQuantise::Now,
        0,
    );
    run(&mut engine, blocks(23), &mut out, &mut ended);
    engine.stop_recording(RecordQuantise::Now, 0);
    run(&mut engine, blocks(200), &mut out, &mut ended);
    let recorded = ended.unwrap();
    let margin = engine.recording_margin() as usize;
    assert_eq!(recorded.before as usize, margin);
    let frames: Vec<f32> = recorded
        .take
        .to_frames()
        .iter()
        .flatten()
        .copied()
        .collect();
    assert_eq!(frames[..margin * 2], out[(start - margin) * 2..start * 2]);
    assert!(
        frames[..margin * 2]
            .iter()
            .any(|sample| sample.abs() > 0.01)
    );
    assert_eq!(
        frames[margin * 2..margin * 2 + 20],
        out[start * 2..start * 2 + 20]
    );
}

#[test]
fn a_start_and_stop_on_the_beat_land_on_its_frames_and_count_its_steps() {
    let mut engine = engine_with(song(), SamplePool::default());
    engine.play_from(0);
    let (mut out, mut ended) = (Vec::new(), None);
    // Part way into the first beat, the next is step 4.
    run(&mut engine, blocks(23), &mut out, &mut ended);
    engine.record(
        take(RATE as u64 * 4),
        RecordSource::Master,
        RecordQuantise::Beat,
        0,
    );
    // Part way into step 5, the next beat is step 8.
    run(&mut engine, blocks(117), &mut out, &mut ended);
    engine.stop_recording(RecordQuantise::Beat, 0);
    run(&mut engine, blocks(140), &mut out, &mut ended);
    let recorded = ended.unwrap();
    assert_eq!(recorded.steps, Some(4), "from step 4 to step 8");
    assert_eq!(recorded.end - recorded.before, (STEP * 4) as u64);
    assert_eq!(proper(&recorded), out[STEP * 4 * 2..STEP * 8 * 2]);
}

#[test]
fn armed_while_stopped_it_starts_with_the_song() {
    let mut engine = engine_with(song(), SamplePool::default());
    engine.record(
        take(RATE as u64 * 4),
        RecordSource::Master,
        RecordQuantise::Bar,
        0,
    );
    let (mut out, mut ended) = (Vec::new(), None);
    run(&mut engine, blocks(23), &mut out, &mut ended);
    assert!(engine.is_recording(), "armed");
    out.clear();
    engine.play_from(0);
    run(&mut engine, blocks(46), &mut out, &mut ended);
    engine.stop_recording(RecordQuantise::Now, 0);
    run(&mut engine, blocks(200), &mut out, &mut ended);
    let recorded = ended.unwrap();
    assert_eq!(proper(&recorded), out[..blocks(46) * 2]);
}

#[test]
fn a_sample_played_while_it_records_reads_only_frames_already_written() {
    // Track 3 plays the recording an octave up from step 1, twice as fast as it's written, so it
    // catches up with the frames being written: it plays up to them, and stops there.
    let mut song = song_of(3);
    song.sample_on(2, "recording").unwrap();
    let steps = bar(&["--- -- --- --- ---", "C-4 7F --- --- ---"]);
    song.tracks[2]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let recording = take(RATE as u64 * 4);
    let mut pool = SamplePool::default();
    pool.insert_growing("recording", Arc::clone(&recording));
    let mut engine = engine_with(song, pool);
    engine.record(
        Arc::clone(&recording),
        RecordSource::Track(0),
        RecordQuantise::Bar,
        0,
    );
    engine.play_from(0);
    let mut block = vec![0.0; BLOCK * 2];
    let mut played = 0;
    let mut stopped_playing = false;
    for _ in 0..(STEP * 6) / BLOCK {
        engine.process(&mut block);
        let written = recording.written();
        let heads: Vec<u64> = engine
            .playheads(2)
            .iter()
            .flatten()
            .map(|h| h.frame)
            .collect();
        for head in &heads {
            assert!(*head < written, "{head} read with {written} written");
        }
        if !heads.is_empty() {
            played += 1;
        } else if played > 0 {
            stopped_playing = true;
        }
    }
    assert!(played > 10, "it played the recording while it was written");
    assert!(
        stopped_playing,
        "and stopped as it reached the frames not yet written"
    );
    assert!(engine.is_recording());
}
