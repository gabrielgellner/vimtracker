//! The bank's analyses: worked out on a thread of their own and kept in a cache folder, so a
//! project opened again finds them there, and `:normalize` and `:trim`, which change only the bank
//! entry's settings, so `u` takes them back. A trimmed entry plays only the sound it kept, cut
//! further by its track's `STA` and `SLEN`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::bank::{BankEntry, Points};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_tui::app::App;
use vt_tui::input::keys::Key;

fn new_app() -> (App, Realtime) {
    let song = Song::new(2);
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

fn press(app: &mut App, keys: &str) {
    let now = Instant::now();
    for c in keys.chars() {
        app.key(Key::char(c), now);
    }
}

fn command(app: &mut App, line: &str) {
    press(app, ":");
    press(app, line);
    app.key(Key::new(KeyCode::Enter), Instant::now());
}

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-tui-analysis-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// A stereo hit at 44.1 kHz with silence either side: frames 4410 to 39690 sound, the left
/// channel peaking at half of full scale and the right at a quarter.
fn hit(path: &Path) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for at in 0..44_100 {
        let sounds = (4410..39_690).contains(&at);
        let value = if sounds {
            (16_384.0 * (at as f32 * 0.05).sin()) as i16
        } else {
            0
        };
        writer.write_sample(value).unwrap();
        writer.write_sample(value / 2).unwrap();
    }
    writer.finalize().unwrap();
}

fn entry(app: &App) -> BankEntry {
    app.song()
        .bank
        .get(1)
        .cloned()
        .expect("a sample in the bank")
}

#[test]
fn normalize_and_trim_change_only_the_entrys_settings_and_u_takes_them_back() {
    let folder = folder();
    let file = folder.join("hit.wav");
    hit(&file);
    let bytes = std::fs::read(&file).unwrap();
    let (mut app, _realtime) = new_app();
    command(&mut app, &format!("sample {}", file.display()));
    let before = entry(&app);
    let song_before = Song::clone(app.song());

    command(&mut app, "normalize -0.3");
    assert_eq!(entry(&app).edit.play.normalize, Some(-0.3));
    assert!(
        app.status
            .contains("normalised to -0.3 dBFS, +5.7 dB from its peak of -6.0 dBFS"),
        "{}",
        app.status
    );
    let analysis = app.samples.analysis(before.source().unwrap().hash);
    assert!(analysis.is_some(), "it waited for the analysis");

    command(&mut app, "trim");
    assert_eq!(
        entry(&app).edit.points,
        Points {
            start: 4410,
            end: Some(39_690),
            ..Points::default()
        }
    );
    assert!(app.status.contains("trimmed to 0.100 s"), "{}", app.status);
    // Only the entry changed: its timeline, name and the rest of the song are as they were.
    let mut song = Song::clone(app.song());
    song.bank.set(1, Some(before.clone()));
    assert_eq!(song, song_before);
    assert_eq!(entry(&app).edit.timeline, before.edit.timeline);
    // Nothing was written, to the file or beside it.
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);

    command(&mut app, "trim");
    assert!(app.status.contains("trimmed already"), "{}", app.status);
    press(&mut app, "u");
    assert_eq!(entry(&app).edit.points, Points::default());
    assert_eq!(entry(&app).edit.play.normalize, Some(-0.3));
    press(&mut app, "u");
    assert_eq!(entry(&app), before);

    command(&mut app, "normalize");
    assert_eq!(entry(&app).edit.play.normalize, Some(0.0));
    command(&mut app, "normalize off");
    assert_eq!(entry(&app).edit.play.normalize, None);
    assert!(
        app.status.contains("no longer normalised"),
        "{}",
        app.status
    );
}

#[test]
fn a_track_with_no_sample_has_nothing_to_normalize_or_trim() {
    let (mut app, _realtime) = new_app();
    command(&mut app, "trim");
    assert_eq!(app.status, "Track 01 plays no sample");
    command(&mut app, "normalize -1");
    assert_eq!(app.status, "Track 01 plays no sample");
    command(&mut app, "normalize 3");
    assert!(app.status.contains("from -60 to 0"), "{}", app.status);
}

#[test]
fn a_project_opened_again_finds_its_analyses_in_the_cache() {
    let folder = folder();
    let cache = folder.join("cache");
    let file = folder.join("hit.wav");
    hit(&file);
    let project = folder.join("song");
    let (mut app, _realtime) = new_app();
    app.use_analysis_cache(Some(cache.clone()));
    command(&mut app, &format!("sample {}", file.display()));
    command(&mut app, &format!("w {}", project.display()));
    app.finish_analysis();
    assert_eq!((app.analyst.built, app.analyst.cached), (1, 0));
    let hash = entry(&app).source().unwrap().hash;
    assert!(app.samples.analysis(hash).is_some());
    assert!(
        !project.join("samples").read_dir().unwrap().any(|file| {
            let name = file.unwrap().file_name();
            !name.to_string_lossy().ends_with(".wav")
        }),
        "nothing but samples in the project's samples folder"
    );

    let (mut again, _realtime) = new_app();
    again.use_analysis_cache(Some(cache));
    command(&mut again, &format!("e {}", project.display()));
    again.finish_analysis();
    assert_eq!((again.analyst.built, again.analyst.cached), (0, 1));
    assert!(again.samples.analysis(hash).is_some());
}

/// The app's song with a note on its first step, played through its samples with no envelope to
/// speak of, the track's sample start and length set, for this many frames, at 48 kHz.
fn play(app: &App, start: u8, length: u8, frames: usize) -> Vec<f32> {
    let mut song = Song::clone(app.song());
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.set_setting(SettingId::SAMPLE_START, start);
    track.set_setting(SettingId::SAMPLE_LENGTH, length);
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    let version = Published::new(Arc::new(song), Arc::clone(&app.samples));
    let out = render_frames(Arc::new(version), 48_000, frames);
    out.iter().step_by(2).copied().collect()
}

/// The loudest frame over a stretch.
fn loudest(out: &[f32], frames: std::ops::Range<usize>) -> f32 {
    out[frames]
        .iter()
        .fold(0.0, |most, frame| most.max(frame.abs()))
}

#[test]
fn a_trimmed_entry_plays_only_its_sound_and_sta_and_slen_cut_that() {
    let folder = folder();
    let file = folder.join("hit.wav");
    hit(&file);
    let (mut app, _realtime) = new_app();
    command(&mut app, &format!("sample {}", file.display()));
    let whole = play(&app, 0, 0xFF, 48_000);
    // At 48 kHz the sound is from frame 4800 to 43200 of the file.
    assert!(loudest(&whole, 0..4700) < 1e-3, "silence first, untrimmed");
    assert!(loudest(&whole, 39_000..43_000) > 0.1);

    command(&mut app, "trim");
    let trimmed = play(&app, 0, 0xFF, 48_000);
    assert!(loudest(&trimmed, 0..1000) > 0.1, "the sound starts at once");
    assert!(loudest(&trimmed, 37_000..38_300) > 0.1);
    assert!(loudest(&trimmed, 38_500..48_000) < 1e-3, "and ends with it");

    // `STA` starts half-way into the 38400 frames kept, and `SLEN` plays a quarter of them.
    let half = play(&app, 0x80, 0xFF, 48_000);
    let left = 38_400 - 38_400 * 128 / 255;
    assert!(loudest(&half, left - 1000..left - 100) > 0.1);
    assert!(loudest(&half, left + 100..48_000) < 1e-3);
    let quarter = play(&app, 0, 0x40, 48_000);
    let kept = 38_400 * 64 / 255;
    assert!(loudest(&quarter, 0..1000) > 0.1);
    assert!(loudest(&quarter, kept - 1000..kept - 100) > 0.1);
    assert!(loudest(&quarter, kept + 100..48_000) < 1e-3);
}
