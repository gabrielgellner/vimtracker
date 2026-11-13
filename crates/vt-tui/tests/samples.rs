//! Samples: loading one onto a track, the slices and loops that come with it, referring to files
//! rather than copying them, and the bank.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

#[test]
fn a_sample_can_be_loaded_and_played() {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("tone.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..48_000 {
        writer.write_sample(16_000_i16).unwrap();
    }
    writer.finalize().unwrap();

    let (mut app, mut rt) = app();
    command(&mut app, &format!("sample {}", wav.display()));
    assert!(app.status.contains("loaded"), "{}", app.status);
    // Writing a note plays it while its key is held.
    press(&mut app, "ia");
    let mut out = vec![0.0; 512];
    rt.process(&mut out);
    rt.process(&mut out);
    assert!(out.iter().any(|s| *s != 0.0), "the note should sound");
    assert_eq!(notes(&app)[0], Note::On(60));
}

/// A second of a quiet tone, at 48 kHz.
fn second_of_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for at in 0..48_000 {
        writer.write_sample((at % 96) as i16 * 100).unwrap();
    }
    writer.finalize().unwrap();
}

/// The current track's bank entry.
fn entry_of(app: &App) -> vt_model::bank::BankEntry {
    let number = app.song().tracks[app.cursor.track].sample();
    app.song().bank.get(number).unwrap().clone()
}

#[test]
fn a_sliced_wav_comes_in_with_its_slices_and_u_leaves_them_out() {
    use vt_model::riff::{Cue, Markers, with_markers};
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let plain = dir.join("plain.wav");
    second_of_wav(&plain);
    let markers = Markers {
        cues: [0, 12_000, 24_000, 36_000]
            .into_iter()
            .zip(1..)
            .map(|(frame, id)| Cue {
                id,
                frame,
                label: None,
            })
            .collect(),
        root: Some(60),
        ..Markers::default()
    };
    let chain = dir.join("chain.wav");
    let bytes = with_markers(&std::fs::read(&plain).unwrap(), &markers).unwrap();
    std::fs::write(&chain, &bytes).unwrap();

    let (mut app, _rt) = app();
    command(&mut app, &format!("sample {}", chain.display()));
    assert_eq!(
        app.status,
        "Track 01 plays 01 chain, loaded into the bank with 4 slices and its root C-3, from its \
         markers: u leaves them out"
    );
    let seeded = entry_of(&app);
    assert_eq!(seeded.edit.points.slices.len(), 4);
    assert_eq!(seeded.edit.hints.root, Some(60));
    assert_eq!(
        std::fs::read(&chain).unwrap(),
        bytes,
        "the file is as it was"
    );

    press(&mut app, "u");
    let plain_entry = entry_of(&app);
    assert!(plain_entry.edit.points.is_whole(), "the file stays, plain");
    assert!(plain_entry.edit.hints.is_default());
    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0), None);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_sample_with_an_ot_beside_it_comes_in_with_its_trim_loop_and_slices() {
    use vt_model::octatrack::{OtFile, OtLoop, OtSlice};
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("break.wav");
    second_of_wav(&file);
    let ot = OtFile {
        loop_mode: OtLoop::On,
        trim_start: 480,
        trim_end: 40_000,
        loop_start: 20_000,
        slices: [480, 10_000, 30_000]
            .into_iter()
            .map(|start| OtSlice {
                start,
                end: start + 5000,
                loop_start: None,
            })
            .collect(),
        ..OtFile::new(48_000, 120)
    };
    std::fs::write(dir.join("break.ot"), ot.to_bytes()).unwrap();
    std::fs::write(dir.join("broken.ot"), "not an ot").unwrap();
    let broken = dir.join("broken.wav");
    wav(&broken);

    let (mut app, _rt) = app();
    command(&mut app, &format!("sample {}", file.display()));
    assert!(app.status.contains("from break.ot"), "{}", app.status);
    let points = entry_of(&app).edit.points;
    assert_eq!(points.start, 480);
    assert_eq!(points.end, Some(40_000));
    assert_eq!(
        points.loop_.map(|l| (l.start, l.end)),
        Some((20_000, 40_000))
    );
    let starts: Vec<u64> = points.slices.iter().map(|slice| slice.start).collect();
    assert_eq!(starts, [480, 10_000, 30_000]);

    press(&mut app, "W");
    command(&mut app, &format!("sample {}", broken.display()));
    assert_eq!(
        app.status,
        "Track 02 plays 02 broken, loaded into the bank. The broken.ot beside it is skipped: \
         it's 9 bytes, not the 832 of an .ot file"
    );
    assert!(entry_of(&app).edit.points.is_whole());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_kits_sound_comes_with_its_files_loop_but_keeps_the_kits_gain() {
    use vt_model::octatrack::{OtFile, OtLoop};
    let kit = kit_file();
    let drums = kit.parent().unwrap().join("drums");
    second_of_wav(&drums.join("snare.wav"));
    let ot = OtFile {
        loop_mode: OtLoop::On,
        gain: 60,
        loop_start: 1000,
        ..OtFile::new(48_000, 120)
    };
    std::fs::write(drums.join("snare.ot"), ot.to_bytes()).unwrap();
    std::fs::write(
        &kit,
        r#"(sounds: [(path: "drums/snare.wav", gain: -2.0), (path: "drums/kick.wav")])"#,
    )
    .unwrap();
    let (mut app, _rt) = app();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(
        app.status,
        "Kit 909: 2 tracks, 1 sound with points from its file: u leaves them out"
    );
    let snare = entry_of(&app);
    assert_eq!(snare.edit.points.loop_.map(|l| l.start), Some(1000));
    assert_eq!(snare.edit.play.gain_db, -2.0);
    press(&mut app, "u");
    assert!(entry_of(&app).edit.points.is_whole());
    assert_eq!(
        sample_of(&app, 1)
            .as_deref()
            .map(|p| p.ends_with("kick.wav")),
        Some(true)
    );
}

#[test]
fn a_sample_is_referred_to_rather_than_copied() {
    let project = folder();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    let source = folder();
    std::fs::create_dir_all(&source).unwrap();
    let tone = source.join("tone.wav");
    wav(&tone);
    command(&mut app, &format!("sample {}", tone.display()));
    assert_eq!(
        sample_of(&app, 0).unwrap(),
        tone.canonicalize().unwrap().display().to_string()
    );
    assert!(!project.join("samples").exists());
    assert!(app.status.contains("loaded"), "{}", app.status);
}

#[test]
fn a_sample_inside_the_project_stays_relative_so_a_bundle_can_move() {
    let project = folder();
    let inside = project.join("samples");
    std::fs::create_dir_all(&inside).unwrap();
    wav(&inside.join("tone.wav"));
    let (mut app, _realtime) = app();
    command(&mut app, &format!("w {}", project.display()));
    command(&mut app, &format!("sample {}/tone.wav", inside.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/tone.wav");

    std::fs::write(
        project.join("kit.ron"),
        r#"(sounds: [(path: "samples/tone.wav")])"#,
    )
    .unwrap();
    command(&mut app, &format!("kit {}/kit.ron", project.display()));
    assert_eq!(sample_of(&app, 0).unwrap(), "samples/tone.wav");
}

#[test]
fn a_bundle_keeps_working_after_its_sources_are_gone() {
    let source = folder();
    std::fs::create_dir_all(&source).unwrap();
    let tone = source.join("tone.wav");
    wav(&tone);
    let (mut app, _realtime) = app();
    command(&mut app, &format!("sample {}", tone.display()));
    let bundle = folder();
    command(&mut app, &format!("w {}", bundle.display()));
    std::fs::remove_dir_all(&source).unwrap();

    let song = vt_model::project::load_song(&bundle).unwrap();
    let mut pool = SamplePool::default();
    let errors = vt_model::project::load_samples(Some(&bundle), &song, 48_000, &mut pool);
    assert!(errors.is_empty());
    assert_eq!(file_of(&song, 0).as_deref(), Some("samples/tone.wav"));
}

#[test]
fn a_step_locks_a_sample_from_the_bank() {
    use vt_model::step::FxCommand;
    use vt_tui::views::status::value_shown;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    press(&mut app, " 3");
    let slot = |app: &App| app.phrase_here().unwrap().steps()[0].fx[0];
    press(&mut app, "lllllK");
    press(&mut app, "smp");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.mode,
        Mode::Picker,
        "an SMP with no value lists the bank"
    );
    press(&mut app, "/kick");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(slot(&app).command, Some(FxCommand::Set(SettingId::SAMPLE)));
    assert_eq!(slot(&app).coarse(), 2);
    assert_eq!(value_shown(&app).as_deref(), Some("02 kick: kick.wav"));
    let step = app.phrase_here().unwrap().steps()[0].to_string();
    assert!(step.contains("sampler.sample:02"), "{step}");

    // K on its value lists the bank again, on the one it locks.
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "02  kick");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(slot(&app).coarse(), 1);
    // And it's on the list to lock, and for an LFO or an envelope to move.
    let track = &app.song().tracks[0];
    let sample = FxCommand::Set(SettingId::SAMPLE);
    assert!(FxCommand::on_track(track).contains(&sample));
    assert!(SettingId::for_mod(track, 3).contains(&SettingId::SAMPLE));
    assert!(SettingId::for_mod(track, 0).contains(&SettingId::SAMPLE));
    assert_eq!(FxCommand::typed("smp", track), Some(sample));
}

#[test]
fn cleaning_the_bank_removes_what_nothing_plays_and_keeps_the_numbers() {
    use vt_model::action::Action;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    command(&mut app, &format!("kit {} all", kit.display()));
    assert_eq!(app.song().bank.len(), 3);
    // Track 2 plays nothing now, but a step on track 1 locks the hat.
    let phrase = app.song().tracks[0].chains[&0].entries[0].phrase;
    app.apply(&Action::SetTrackSample {
        track: 1,
        sample: 0,
    });
    app.apply(&Action::SetTrackSample {
        track: 2,
        sample: 0,
    });
    let step: Step = "C-3 -- sampler.sample:03 --- ---".parse().unwrap();
    app.apply(&Action::SetStep {
        track: 0,
        phrase,
        step: 4,
        value: step,
    });
    command(&mut app, "bank clean");
    let left: Vec<u8> = app.song().bank.iter().map(|(number, _)| number).collect();
    assert_eq!(left, [1, 3]);
    assert!(app.status.contains("Removed 1 sample"), "{}", app.status);
    command(&mut app, "bank clean");
    assert_eq!(app.status, "Every sample in the bank is played");
    press(&mut app, "u");
    assert_eq!(app.song().bank.len(), 3);
}

#[test]
fn space_5_chooses_a_sample_for_the_current_track() {
    let (mut app, _realtime) = app();
    // With nothing to list it asks for a path, and either way the view stays.
    press(&mut app, " 5");
    assert_ne!(app.mode, Mode::Normal);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.view, View::Phrase);
}

#[test]
fn notes_played_while_editing_preview_on_this_tracks_sound() {
    let (mut app, mut realtime) = app();
    press(&mut app, "W");
    with_a_sound(&mut app);
    block(&mut realtime, &mut app);
    press(&mut app, "ia");
    assert!(loudest(&mut realtime, 2000) > 0.01, "it can be heard");
}

/// A second of silence at 44.1 kHz, as a file in a folder of its own.
fn file_at_44k() -> PathBuf {
    let dir = folder();
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("break.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&wav, spec).unwrap();
    for _ in 0..44_100 {
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
    wav
}

#[test]
fn sample_sets_where_the_tracks_sample_plays_in_its_files_own_frames() {
    use vt_model::bank::Loop;
    let (mut app, _realtime) = app();
    command(&mut app, "sample start 0.25");
    assert!(app.status.contains("plays no sample"), "{}", app.status);
    command(&mut app, &format!("sample {}", file_at_44k().display()));
    let entry = |app: &App| app.song().bank.get(1).unwrap().edit.clone();

    command(&mut app, "sample start 0.25");
    assert_eq!(entry(&app).points.start, 11_025, "at the file's 44.1 kHz");
    assert!(app.status.contains("starts at 0.250 s"), "{}", app.status);
    command(&mut app, "sample end 500ms");
    assert_eq!(entry(&app).points.end, Some(22_050));
    command(&mut app, "sample end 0.1");
    assert!(app.status.contains("before the start"), "{}", app.status);
    assert_eq!(entry(&app).points.end, Some(22_050), "unchanged");
    command(&mut app, "sample loop 0.3 0.4 10ms");
    assert_eq!(
        entry(&app).points.loop_,
        Some(Loop {
            start: 13_230,
            end: 17_640,
            crossfade: 441,
        })
    );
    command(&mut app, "sample reverse");
    assert!(entry(&app).play.reverse);
    command(&mut app, "sample fade 10ms 20ms");
    assert_eq!(
        (entry(&app).play.fade_in, entry(&app).play.fade_out),
        (441, 882)
    );
    command(&mut app, "sample gain -3");
    command(&mut app, "sample tune 2 -30");
    let play = entry(&app).play;
    assert_eq!((play.gain_db, play.tune, play.cents), (-3.0, 2, -30));
    command(&mut app, "sample end");
    assert_eq!(entry(&app).points.end, None, "the file's end");

    // Each is a step to undo.
    press(&mut app, "uuu");
    assert_eq!(entry(&app).play.fade_in, 441);
    assert_eq!(entry(&app).play.gain_db, 0.0);
    press(&mut app, "uu");
    assert!(!entry(&app).play.reverse);
    assert!(entry(&app).points.loop_.is_some());
}
