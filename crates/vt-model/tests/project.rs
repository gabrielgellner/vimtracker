use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::constants::FORMAT_VERSION;
use vt_model::project::{
    ProjectError, create_project, import_sample, load_samples, load_song, save_as, save_song,
};
use vt_model::sample::{SampleError, load_wav};
use vt_model::samples::SamplePool;
use vt_model::song::{Launch, Machine, Repeat, Song, SynthVoice};

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-model-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn wav(path: &Path, channels: u16, bits: u16, rate: u32, frames: usize) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: bits,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for frame in 0..frames {
        for _ in 0..channels {
            let phase = frame as f32 / 50.0;
            let value = (phase * std::f32::consts::TAU).sin() * 0.5;
            let scale = (1_i64 << (bits - 1)) as f32;
            if bits == 16 {
                writer.write_sample((value * scale) as i16).unwrap();
            } else {
                writer.write_sample((value * scale) as i32).unwrap();
            }
        }
    }
    writer.finalize().unwrap();
}

#[test]
fn a_song_round_trips() {
    let dir = folder();
    let mut song = Song::new(3);
    song.millibpm = 137_630;
    song.launch = Launch::every(12).unwrap();
    song.tracks[1].machine = Machine::Plaits(SynthVoice::Fm);
    let index = vt_model::settings::SettingId::voice(SynthVoice::Fm, 1).unwrap();
    song.tracks[1].set_setting(index, 0x40);
    assert!(
        !song.tracks[1].settings.is_empty(),
        "a changed setting is kept"
    );
    Arc::get_mut(song.tracks[0].chains.get_mut(&0).unwrap())
        .unwrap()
        .label = "bass".to_owned();
    let phrase = Arc::get_mut(song.tracks[0].phrases.get_mut(&0).unwrap()).unwrap();
    *phrase.step_mut(2).unwrap() = "C-4 64 CUT40 ---00 RET14".parse().unwrap();
    save_song(&dir, &song).unwrap();
    assert_eq!(load_song(&dir).unwrap(), song);
}

#[test]
fn a_repeat_of_nothing_is_refused_on_load() {
    let dir = folder();
    let mut song = Song::new(1);
    song.sections[0].repeat = Repeat::new(3);
    save_song(&dir, &song).unwrap();
    assert_eq!(load_song(&dir).unwrap(), song);
    let path = dir.join("song.ron");
    let text = fs::read_to_string(&path).unwrap();
    let saved = written_repeat(&text);
    assert!(text.contains(&saved), "{text}");
    fs::write(&path, text.replace(&saved, &saved.replace('3', "0"))).unwrap();
    assert!(load_song(&dir).is_err());
}

/// How a repeat of 3 is written in a saved song.
fn written_repeat(text: &str) -> String {
    if text.contains("repeat: 3") {
        "repeat: 3"
    } else {
        "repeat:3"
    }
    .to_owned()
}

#[test]
fn a_launch_grid_of_nothing_is_refused_on_load() {
    let dir = folder();
    let mut song = Song::new(1);
    song.launch = Launch::every(12).unwrap();
    save_song(&dir, &song).unwrap();
    let path = dir.join("song.ron");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("Every(12)"), "{text}");
    fs::write(&path, text.replace("Every(12)", "Every(0)")).unwrap();
    assert!(load_song(&dir).is_err());
}

#[test]
fn a_different_format_names_both_versions() {
    let dir = folder();
    fs::write(
        dir.join("song.ron"),
        "(format_version: 99, a_new_field: [1, (b: \"c\")], other: Some(3))",
    )
    .unwrap();
    match load_song(&dir) {
        Err(ProjectError::Version {
            found, expected, ..
        }) => assert_eq!((found, expected), (99, FORMAT_VERSION)),
        other => panic!("expected a version error, got {other:?}"),
    }
}

#[test]
fn a_failed_save_leaves_the_previous_file() {
    let dir = folder();
    let song = Song::new(1);
    save_song(&dir, &song).unwrap();
    // A directory in the temporary file's place makes the write fail.
    fs::create_dir(dir.join(".song.ron.tmp")).unwrap();
    let mut changed = song.clone();
    changed.millibpm = 90_000;
    assert!(save_song(&dir, &changed).is_err());
    assert_eq!(load_song(&dir).unwrap(), song);
}

#[test]
fn a_new_project_refuses_an_existing_song() {
    let dir = folder();
    create_project(&dir, &Song::new(1)).unwrap();
    assert!(matches!(
        create_project(&dir, &Song::new(1)),
        Err(ProjectError::Exists(_))
    ));
}

#[test]
fn mono_16_bit_loads_as_stereo() {
    let dir = folder();
    let path = dir.join("mono.wav");
    wav(&path, 1, 16, 48_000, 1000);
    let sample = load_wav(&path, 48_000).unwrap();
    assert_eq!(sample.frames.len(), 1000);
    assert!(sample.frames.iter().all(|f| f[0] == f[1]));
}

#[test]
fn stereo_24_bit_converts_rate_and_keeps_length() {
    let dir = folder();
    let path = dir.join("stereo.wav");
    wav(&path, 2, 24, 44_100, 44_100);
    let sample = load_wav(&path, 48_000).unwrap();
    let length = sample.frames.len() as i64;
    assert!((length - 48_000).abs() <= 2, "got {length} frames");
    let peak = sample.frames.iter().fold(0.0_f32, |p, f| p.max(f[0].abs()));
    assert!((peak - 0.5).abs() < 0.02, "got peak {peak}");
}

#[test]
fn more_than_two_channels_are_refused() {
    let dir = folder();
    let path = dir.join("surround.wav");
    wav(&path, 4, 16, 48_000, 10);
    assert!(matches!(
        load_wav(&path, 48_000),
        Err(SampleError::Channels { channels: 4, .. })
    ));
}

#[test]
fn a_missing_sample_names_its_path_and_the_rest_load() {
    let dir = folder();
    wav(&dir.join("here.wav"), 1, 16, 48_000, 10);
    let mut song = Song::new(2);
    song.tracks[0].sample = Some("gone.wav".into());
    song.tracks[1].sample = Some("here.wav".into());
    let mut pool = SamplePool::default();
    let errors = load_samples(Some(&dir), &song, 48_000, &mut pool);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("gone.wav"));
    assert!(pool.id_of("here.wav").is_some());
}

#[test]
fn imports_keep_same_files_and_rename_different_ones() {
    let source = folder();
    let project = folder();
    let first = source.join("kick.wav");
    wav(&first, 1, 16, 48_000, 10);
    assert_eq!(import_sample(&project, &first).unwrap(), "samples/kick.wav");
    assert_eq!(import_sample(&project, &first).unwrap(), "samples/kick.wav");
    let other = folder().join("kick.wav");
    wav(&other, 1, 16, 48_000, 20);
    assert_eq!(
        import_sample(&project, &other).unwrap(),
        "samples/kick-2.wav"
    );
}

#[test]
fn save_as_copies_samples_and_rewrites_paths() {
    let source = folder();
    let absolute = source.join("snare.wav");
    wav(&absolute, 1, 16, 48_000, 10);
    let mut song = Song::new(1);
    song.tracks[0].sample = Some(absolute.display().to_string());
    let target = folder();
    let saved = save_as(None, &target, &song).unwrap();
    assert_eq!(saved.tracks[0].sample.as_deref(), Some("samples/snare.wav"));
    assert!(target.join("samples/snare.wav").exists());
    assert_eq!(load_song(&target).unwrap(), saved);
    assert!(matches!(
        save_as(None, &target, &song),
        Err(ProjectError::Exists(_))
    ));
}

#[test]
fn a_song_from_before_the_fixed_tracks_loads_with_them() {
    let dir = folder();
    let mut song = Song::new(2);
    song.tracks.truncate(2);
    song.sections[0].chains.truncate(2);
    save_song(&dir, &song).unwrap();
    let loaded = load_song(&dir).unwrap();
    assert_eq!(loaded.sound_tracks(), 2);
    assert_eq!(loaded.tracks.len(), 5);
    assert_eq!(loaded.sections[0].chains.len(), 5);
    assert_eq!(loaded.tracks[2].name, "A");
}
