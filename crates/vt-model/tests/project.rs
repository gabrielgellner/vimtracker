use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::bank::{BankEntry, ContentHash, Loop, PlayParams, Points, SliceParams};
use vt_model::constants::{FIXED_TRACKS, FORMAT_VERSION, MAX_GROUPS};
use vt_model::insert::{Compressor, CompressorType, Insert};
use vt_model::project::{
    ProjectError, create_project, find_sources, import_sample, load_samples, load_song, save_as,
    save_song,
};
use vt_model::sample::{SampleError, load_sample};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Launch, Machine, Repeat, Song, SynthVoice, TrackKind};

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
    *phrase.step_mut(2).unwrap() = "C-3 64 cut:40 --- ret:14".parse().unwrap();
    save_song(&dir, &song).unwrap();
    assert_eq!(load_song(&dir).unwrap(), song);
}

#[test]
fn a_switched_out_modulator_is_saved_and_only_then() {
    let dir = folder();
    let mut song = Song::new(1);
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(
        !text.contains("bypassed"),
        "nothing switched out, nothing written"
    );
    song.tracks[0].lfos[1].bypassed = true;
    song.tracks[0].envelopes[2].bypassed = true;
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert_eq!(text.matches("bypassed: true").count(), 2, "{text}");
    let loaded = load_song(&dir).unwrap();
    assert!(loaded.tracks[0].lfos[1].bypassed);
    assert!(loaded.tracks[0].envelopes[2].bypassed);
    assert!(!loaded.tracks[0].lfos[0].bypassed);
    assert_eq!(loaded, song);
}

#[test]
fn a_bank_round_trips_with_its_numbers_hashes_and_play_settings() {
    let dir = folder();
    let mut song = Song::new(2);
    let mut kick = BankEntry::of_file("samples/kick.wav", ContentHash::of(b"kick"));
    kick.label = "BD".to_owned();
    kick.name = "Bass Drum 909".to_owned();
    kick.edit.play = PlayParams {
        gain_db: -3.5,
        tune: -2,
        cents: 30,
        reverse: true,
        fade_in: 480,
        fade_out: 4800,
        normalize: Some(-0.5),
        snap_zero: true,
    };
    kick.edit.points = Points {
        start: 100,
        end: Some(40_000),
        loop_: Some(Loop {
            start: 2000,
            end: 30_000,
            crossfade: 960,
        }),
        ..Points::default()
    };
    kick.edit.points.reslice(&[100, 20_000]);
    kick.edit.points.slices[1].over = SliceParams {
        gain_db: Some(-1.5),
        pan: Some(-20),
        reverse: Some(false),
        ..SliceParams::default()
    };
    song.bank.set(0x01, Some(kick));
    song.bank.set(
        0xFF,
        Some(BankEntry::of_file(
            "/elsewhere/hat.aif",
            ContentHash::UNKNOWN,
        )),
    );
    song.tracks[0].set_setting(SettingId::SAMPLE, 0x01);
    song.tracks[1].set_setting(SettingId::SAMPLE, 0xFF);
    song.tracks[1].set_setting(SettingId::SAMPLE_START, 0x40);
    song.tracks[1].set_setting(SettingId::PLAY, 2);
    let phrase = Arc::get_mut(song.tracks[0].phrases.get_mut(&0).unwrap()).unwrap();
    *phrase.step_mut(1).unwrap() = "C-3 -- sampler.sample:FF --- ---".parse().unwrap();
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(text.contains("sampler.sample"), "the setting by its key");
    assert!(text.contains("sampler.start") && text.contains("sampler.play"));
    assert!(text.contains("loop: Some("), "the loop under its own name");
    assert!(text.contains("slices: ["), "the slices");
    assert!(
        text.contains(&ContentHash::of(b"kick").to_string()),
        "the hash in hex"
    );
    let loaded = load_song(&dir).unwrap();
    assert_eq!(loaded, song);
    assert_eq!(loaded.bank.get(0xFF).unwrap().name, "hat");
    assert_eq!(
        loaded.samples_in_use().into_iter().collect::<Vec<_>>(),
        [0x01, 0xFF]
    );
}

#[test]
fn points_in_a_44k_files_frames_land_on_the_same_sound_at_48k() {
    use vt_model::playable::Playable;
    let dir = folder();
    let path = dir.join("loop.wav");
    wav(&path, 1, 16, 44_100, 44_100);
    let mut pool = SamplePool::default();
    pool.load("loop.wav", &path, 48_000).unwrap();
    let mut entry = BankEntry::of_file("loop.wav", ContentHash::UNKNOWN);
    entry.edit.points.start = 4410;
    entry.edit.points.end = Some(22_050);
    let span = Playable::of(&entry, &pool).unwrap().span(0, false);
    assert_eq!((span.start, span.end), (4800, 24_000));
    assert!(span.frames.abs_diff(48_000) <= 2, "{}", span.frames);
}

#[test]
fn a_moved_file_is_found_by_its_content_and_a_changed_one_is_noticed() {
    let dir = folder();
    fs::create_dir_all(dir.join("samples")).unwrap();
    wav(&dir.join("samples/renamed.wav"), 1, 16, 48_000, 10);
    wav(&dir.join("samples/changed.wav"), 1, 16, 48_000, 20);
    let moved = ContentHash::of_file(&dir.join("samples/renamed.wav")).unwrap();
    let mut song = Song::new(1);
    song.bank
        .set(1, Some(BankEntry::of_file("samples/kick.wav", moved)));
    song.bank.set(
        2,
        Some(BankEntry::of_file(
            "samples/changed.wav",
            ContentHash::of(b"what it was"),
        )),
    );
    song.bank.set(
        3,
        Some(BankEntry::of_file("gone.wav", ContentHash::of(b"nowhere"))),
    );
    let notes = find_sources(Some(&dir), &mut song);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(
        notes[0].contains("found at samples/renamed.wav"),
        "{notes:?}"
    );
    assert!(notes[1].contains("has changed"), "{notes:?}");
    assert_eq!(
        song.bank.get(1).unwrap().source().unwrap().path,
        "samples/renamed.wav"
    );
    assert_eq!(
        song.bank.get(3).unwrap().source().unwrap().path,
        "gone.wav",
        "what's nowhere is left for loading to report"
    );
}

#[test]
fn the_pool_loads_a_file_once_however_many_tracks_play_it() {
    let dir = folder();
    wav(&dir.join("kick.wav"), 1, 16, 48_000, 10);
    let mut song = Song::new(4);
    for track in 0..4 {
        song.sample_on(track, "kick.wav");
    }
    assert_eq!(song.bank.len(), 1);
    let mut pool = SamplePool::default();
    assert!(load_samples(Some(&dir), &song, 48_000, &mut pool).is_empty());
    assert_eq!(pool.len(), 1);
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
    let sample = load_sample(&path, 48_000).unwrap();
    assert_eq!(sample.frames.len(), 1000);
    assert!(sample.frames.iter().all(|f| f[0] == f[1]));
}

#[test]
fn stereo_24_bit_converts_rate_and_keeps_length() {
    let dir = folder();
    let path = dir.join("stereo.wav");
    wav(&path, 2, 24, 44_100, 44_100);
    let sample = load_sample(&path, 48_000).unwrap();
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
        load_sample(&path, 48_000),
        Err(SampleError::Channels { channels: 4, .. })
    ));
}

#[test]
fn a_missing_sample_names_its_path_and_the_rest_load() {
    let dir = folder();
    wav(&dir.join("here.wav"), 1, 16, 48_000, 10);
    let mut song = Song::new(2);
    song.sample_on(0, "gone.wav");
    song.sample_on(1, "here.wav");
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
    let copy = folder().join("snare copy.wav");
    fs::copy(&absolute, &copy).unwrap();
    let hash = ContentHash::of_file(&absolute).unwrap();
    let mut song = Song::new(1);
    song.bank.set(
        1,
        Some(BankEntry::of_file(absolute.display().to_string(), hash)),
    );
    song.bank.set(
        2,
        Some(BankEntry::of_file(copy.display().to_string(), hash)),
    );
    song.tracks[0].set_setting(SettingId::SAMPLE, 1);
    let target = folder();
    let saved = save_as(None, &target, &song).unwrap();
    let path = |number| {
        saved
            .bank
            .get(number)
            .unwrap()
            .source()
            .unwrap()
            .path
            .clone()
    };
    assert_eq!(path(1), "samples/snare.wav");
    assert_eq!(
        path(2),
        "samples/snare.wav",
        "the same content is copied once"
    );
    assert!(target.join("samples/snare.wav").exists());
    assert!(!target.join("samples/snare copy.wav").exists());
    assert_eq!(saved.tracks[0].sample(), 1);
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
    assert_eq!(loaded.tracks.len(), 2 + FIXED_TRACKS);
    assert_eq!(loaded.sections[0].chains.len(), 2 + FIXED_TRACKS);
    assert_eq!(loaded.tracks[2].name, "G1");
    assert_eq!(loaded.tracks[2 + MAX_GROUPS].name, "A");
}

#[test]
fn groups_and_what_is_in_them_round_trip() {
    let dir = folder();
    let mut song = Song::new(4);
    song.tracks[0].group = Some(1);
    song.tracks[1].group = Some(1);
    song.tracks[3].group = Some(MAX_GROUPS as u8);
    let group = song.fixed_track(TrackKind::Group(1)).unwrap();
    song.tracks[group].level = 0xC0;
    song.tracks[group].pan = 0x60;
    song.tracks[group].send_b = 0x50;
    song.tracks[group].mute = true;
    song.tracks[group].inserts[0] = Insert::Compressor(Compressor {
        kind: CompressorType::Console,
        ..Compressor::DEFAULT
    });
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(
        text.contains("kind: Group(1)"),
        "in the list of tracks: {text}"
    );
    let loaded = load_song(&dir).unwrap();
    assert_eq!(loaded, song);
    assert_eq!(loaded.group_track_of(0), Some(group));
    assert_eq!(loaded.group_track_of(2), None);
}
