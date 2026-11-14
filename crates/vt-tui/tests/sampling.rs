//! Sampling, `Space R`: the master, a track or the input recorded into a new bank entry, written to
//! the project's `samples/` once it stops (#288).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_engine::recorder::margin_frames;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::input::mode::View;

mod common;
use common::*;

const RATE: u32 = 48_000;
/// A step at 120 BPM, and a bar.
const STEP: u64 = 6000;
const BAR: u64 = STEP * 16;

/// A project folder of the test's own, empty.
fn folder(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!("vt-sampling-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).unwrap();
    folder
}

/// A project in a folder whose first track plays a synth note on each beat, the same on both
/// sides, so the master is mono.
fn project(folder: &Path) -> (App, Realtime) {
    let mut song = Song::new(2);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::default());
    let steps: Vec<Step> = (0..16)
        .map(|at| {
            let text = if at % 4 == 0 {
                "C-3 7F --- --- ---"
            } else {
                "--- -- --- --- ---"
            };
            text.parse().unwrap()
        })
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(RATE, version));
    let app = App::new(song, Some(folder.to_path_buf()), samples, handle, RATE);
    (app, realtime)
}

/// Run the audio side on so many frames, a block at a time, with the app's frame after each.
fn run(app: &mut App, realtime: &mut Realtime, frames: u64) {
    let mut block = vec![0.0; 512 * 2];
    for _ in 0..frames.div_ceil(512) {
        realtime.process(&mut block);
        app.tick();
    }
}

/// Type a command line and run it.
fn command(app: &mut App, line: &str) {
    tap(app, ":");
    for c in line.chars() {
        app.key(Key::new(KeyCode::Char(c)), Instant::now());
    }
    app.key(Key::new(KeyCode::Enter), Instant::now());
}

fn play(app: &mut App) {
    let tab = Key::new(KeyCode::Tab);
    app.key(tab, Instant::now());
    app.key_released(tab, Instant::now());
}

/// The sample files in a project's `samples/` folder.
fn files(folder: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(folder.join("samples"))
        .map(|files| files.flatten().map(|file| file.path()).collect())
        .unwrap_or_default();
    found.sort();
    found
}

#[test]
fn space_r_armed_records_the_master_from_the_song_start_to_a_bar_into_a_new_entry() {
    let folder = folder("bar");
    let (mut app, mut rt) = project(&folder);
    let undo = app.history.undo_depth();
    tap(&mut app, " R");
    assert!(app.status.contains("as the song starts"), "{}", app.status);
    let number = app.sampling.live.as_ref().unwrap().number;
    assert_eq!(number, 1);
    let entry = app.song().bank.get(number).unwrap();
    assert_eq!(entry.edit.points.start, margin_frames(RATE));
    run(&mut app, &mut rt, 4096);
    assert!(
        status_line(&app).contains(" REC 01"),
        "armed: {}",
        status_line(&app)
    );
    play(&mut app);
    run(&mut app, &mut rt, BAR + STEP);
    assert_eq!(app.transport, Transport::Playing);
    let line = status_line(&app);
    assert!(
        line.contains(vt_tui::glyphs::RECORD) && line.contains(" 01 "),
        "{line}"
    );
    tap(&mut app, " R");
    assert!(app.status.contains("on the next bar"), "{}", app.status);
    run(&mut app, &mut rt, BAR + STEP * 3);
    assert!(app.sampling.live.is_none(), "it ended: {}", app.status);
    assert!(app.status.starts_with("Recorded 01"), "{}", app.status);
    assert!(app.status.contains("4.00 s in mono"), "{}", app.status);
    // On the project's samples/ in mono, with the margins either side.
    let written = files(&folder);
    assert_eq!(written.len(), 1, "{written:?}");
    let wav = hound::WavReader::open(&written[0]).unwrap();
    assert_eq!(wav.spec().channels, 1);
    let margin = margin_frames(RATE);
    assert_eq!(u64::from(wav.duration()), margin + 2 * BAR + margin);
    // Its entry plays from the bar it started on to the bar it stopped on, knows it lasts those
    // steps and its file's content.
    let entry = app.song().bank.get(number).unwrap();
    assert_eq!(entry.edit.points.start, margin);
    assert_eq!(entry.edit.points.end, Some(margin + 2 * BAR));
    assert_eq!(entry.edit.hints.length_steps(), Some(32));
    assert!(entry.source().unwrap().hash.is_known());
    assert!(entry.source().unwrap().path.starts_with("samples/"));
    // The whole recording is one undo step.
    assert_eq!(app.history.undo_depth(), undo + 1);
    tap(&mut app, "u");
    assert!(app.song().bank.get(number).is_none());
    let redo = vt_tui::input::keys::parse_sequence("C-r").unwrap()[0];
    app.key(redo, Instant::now());
    let entry = app.song().bank.get(number).unwrap();
    assert_eq!(
        entry.edit.points.end,
        Some(margin + 2 * BAR),
        "redone whole"
    );
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_recording_taken_back_leaves_no_entry_and_no_file() {
    let folder = folder("cancel");
    let (mut app, mut rt) = project(&folder);
    command(&mut app, "record master now");
    run(&mut app, &mut rt, STEP);
    assert!(app.sampling.live.as_ref().unwrap().started);
    command(&mut app, "record cancel");
    run(&mut app, &mut rt, STEP);
    assert!(app.song().bank.is_empty());
    assert!(files(&folder).is_empty());
    assert!(app.status.contains("taken back"), "{}", app.status);
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn recordings_the_saved_song_doesnt_play_are_removed_on_quitting() {
    let folder = folder("unkept");
    let (mut app, mut rt) = project(&folder);
    for _ in 0..2 {
        command(&mut app, "record now");
        run(&mut app, &mut rt, STEP);
        tap(&mut app, " R");
        run(&mut app, &mut rt, STEP * 4);
        assert!(app.sampling.live.is_none(), "{}", app.status);
        if app.song().bank.len() == 1 {
            command(&mut app, "w");
        }
    }
    let both = files(&folder);
    assert_eq!(both.len(), 2);
    command(&mut app, "q!");
    assert!(app.should_quit);
    // The first is played by the song as saved, and the second only by the song left unsaved.
    let kept = files(&folder);
    assert_eq!(kept.len(), 1);
    let saved = vt_model::project::load_song(&folder).unwrap();
    let first = saved.bank.get(1).unwrap().source().unwrap();
    assert_eq!(folder.join(&first.path), kept[0]);
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn the_input_records_once_one_is_open_and_a_silent_right_side_makes_it_mono() {
    let folder = folder("input");
    let (mut app, mut rt) = project(&folder);
    command(&mut app, "record input now");
    assert!(app.status.contains("No input is open"), "{}", app.status);
    assert!(app.sampling.live.is_none());
    // A fake input in place of a device, with sound on its left side only.
    let (mut input, heard) = rtrb::RingBuffer::new(vt_model::constants::INPUT_QUEUE);
    rt.listen(heard);
    app.audio.input = Some("Fake input".to_owned());
    command(&mut app, "record input now");
    let mut block = vec![0.0; 512 * 2];
    let mut at = 0_u64;
    for round in 0..60 {
        if round == 20 {
            tap(&mut app, " R");
        }
        for _ in 0..512 {
            let _ = input.push((at as f32 * 0.05).sin() * 0.5);
            let _ = input.push(0.0);
            at += 1;
        }
        rt.process(&mut block);
        app.tick();
    }
    assert!(app.sampling.live.is_none(), "{}", app.status);
    assert!(app.status.contains("in mono"), "{}", app.status);
    let written = files(&folder);
    let wav = hound::WavReader::open(&written[0]).unwrap();
    assert_eq!(wav.spec().channels, 1);
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn r_on_the_sample_page_records_and_shows_the_recording_once_it_stops() {
    let folder = folder("page");
    let (mut app, mut rt) = project(&folder);
    command(&mut app, "record 1 now");
    // Back out, and in again from the sample page.
    command(&mut app, "record cancel");
    tap(&mut app, " 0");
    assert_eq!(app.view, View::Sample);
    tap(&mut app, "R");
    let number = app.sampling.live.as_ref().unwrap().number;
    play(&mut app);
    run(&mut app, &mut rt, STEP);
    // Track 01 doesn't play the recording, so the page shows what it says of it once pinned to it.
    let shown = app.still_recording(number).unwrap();
    assert!(shown.starts_with("Recording into 01"), "{shown}");
    tap(&mut app, "R");
    run(&mut app, &mut rt, STEP * 4);
    assert!(app.sampling.live.is_none(), "{}", app.status);
    app.finish_analysis();
    let shown = app.sample_shown().unwrap();
    assert_eq!(shown.number, number);
    assert_eq!(app.sampling.source, vt_tui::sampling::Source::Track(0));
    fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn a_pattern_plays_the_recording_before_it_has_stopped() {
    let folder = folder("pattern");
    let (mut app, mut rt) = project(&folder);
    // Track 02 plays sample 01 on the second beat of each bar, which is the recording once it
    // starts.
    let mut song = Song::clone(app.song());
    song.tracks[1].machine = Machine::Sampler;
    song.tracks[1].set_setting(vt_model::settings::SettingId::SAMPLE, 1);
    let mut steps = vec![Step::default(); 16];
    steps[4] = "C-3 7F --- --- ---".parse().unwrap();
    song.tracks[1]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    app.history.commit(Arc::new(song));
    app.publish();
    command(&mut app, "record master now");
    play(&mut app);
    let mut heard = false;
    let mut block = vec![0.0; 512 * 2];
    for _ in 0..(BAR / 512) {
        rt.process(&mut block);
        app.tick();
        let heads = rt.engine().playheads(1);
        heard |= heads.iter().flatten().any(|head| head.entry == Some(1));
    }
    assert!(app.sampling.live.as_ref().unwrap().started);
    assert!(heard, "track 02 played the recording while it recorded");
    command(&mut app, "record cancel");
    fs::remove_dir_all(&folder).unwrap();
}
