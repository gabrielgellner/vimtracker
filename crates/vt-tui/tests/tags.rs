//! Tags in the explorer (decision 315): guessed from folders, names, a kit's labels and how a
//! sound ends, found with `#` below the open folders, given and taken off with `T`, kept by a
//! file's content or in a preset's file.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use vt_model::tag::{Source, Tag};
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::explorer::Icon;
use vt_tui::input::picker::Row;

mod common;
use common::*;

/// A sample folder with kicks and a snare in folders below it, and two loops, as the first
/// sample folder. None of its folders is open.
fn drum_library(app: &mut App) -> PathBuf {
    let library = folder();
    for folder in ["drums/kicks", "breaks"] {
        std::fs::create_dir_all(library.join(folder)).unwrap();
    }
    for file in [
        "drums/kicks/BD01.wav",
        "drums/kicks/thump.wav",
        "drums/snare 808.wav",
        "breaks/amen 136 bpm.wav",
        "breaks/kick loop.wav",
    ] {
        wav(&library.join(file));
    }
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string()],
        presets: String::new(),
    };
    library
}

/// The names of the files the search kept, in order.
fn found(app: &App) -> Vec<String> {
    let picker = app.picker.as_ref().unwrap();
    picker
        .rows()
        .iter()
        .filter_map(|row| match row {
            Row::Item(item) if picker.folder_open(*item).is_none() => {
                Some(picker.item(*item).label.clone())
            }
            _ => None,
        })
        .collect()
}

/// The picker drawn on the author's screen, as text.
fn drawn(app: &App) -> String {
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut picker = app.picker.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..42)
        .map(|y| {
            (0..163)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect()
}

fn files_in(folder: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_in(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[test]
fn a_search_by_tag_finds_every_kick_below_the_open_folders() {
    let (mut app, _realtime) = app();
    let library = drum_library(&mut app);
    command(&mut app, "sample");
    assert_eq!(found(&app), Vec::<String>::new(), "nothing below is open");

    press(&mut app, "#kick");
    assert_eq!(found(&app), ["kick loop.wav", "BD01.wav", "thump.wav"]);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.tag_found(), Some(3));
    assert_eq!(
        picker.current().unwrap().label,
        "kick loop.wav",
        "on the first found, not a folder it's in"
    );
    let text = drawn(&app);
    assert!(text.contains("Samples 3 found"), "{text}");
    assert!(text.contains("> #kick_"), "{text}");
    assert!(text.contains("  #kick 3"), "the tag row: {text}");

    // Each tag narrows it, and the rest of the search is the name.
    press(&mut app, " #one");
    assert_eq!(found(&app), ["BD01.wav", "thump.wav"]);
    press(&mut app, " thu");
    assert_eq!(found(&app), ["thump.wav"]);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "#loop");
    assert_eq!(found(&app), ["amen 136 bpm.wav", "kick loop.wav"]);

    // `#` alone lists everything below, with each tag in use and how many have it.
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.picker.as_ref().unwrap().tag_row(),
        None,
        "only while the search is typed"
    );
    press(&mut app, "#");
    assert_eq!(found(&app).len(), 5);
    let row = app.picker.as_ref().unwrap().tag_row().unwrap();
    assert_eq!(
        row,
        [
            ("kick".to_owned(), 3),
            ("one-shot".to_owned(), 3),
            ("loop".to_owned(), 2),
            ("snare".to_owned(), 1)
        ]
    );
    // `Tab` finishes the tag being typed as the row's first.
    press(&mut app, "sn");
    special(&mut app, KeyCode::Tab);
    assert_eq!(app.picker.as_ref().unwrap().query, "#snare ");
    assert_eq!(found(&app), ["snare 808.wav"]);
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn t_gives_a_sample_tags_by_its_content_and_takes_a_wrong_guess_off() {
    let (mut app, _realtime) = app();
    let library = drum_library(&mut app);
    let before = files_in(&library);
    command(&mut app, "sample");
    press(&mut app, "#kick thu");
    special(&mut app, KeyCode::Esc);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "thump.wav");
    assert_eq!(
        picker.current().unwrap().tags,
        [
            Tag::new("kick", Source::Guessed),
            Tag::new("one-shot", Source::Guessed)
        ],
        "from its folder, a drum's a one-shot"
    );

    press(&mut app, "T");
    let text = drawn(&app);
    assert!(text.contains("# kick one-shot _"), "{text}");
    assert!(text.contains("Enter tag  Tab complete  Esc back"), "{text}");
    press(&mut app, "punchy");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "thump.wav: #kick #one-shot #punchy");
    let thump = library.join("drums/kicks/thump.wav");
    assert_eq!(app.tag_library.of_file(&thump).unwrap().tags, ["punchy"]);
    let text = drawn(&app);
    assert!(
        text.lines()
            .any(|line| line.contains("thump.wav") && line.contains("#punchy│")),
        "given by hand, at the right of its row: {text}"
    );

    // Deleting a guess takes it off, and `Tab` finishes a tag from those in use.
    press(&mut app, "T");
    for _ in 0.."kick one-shot punchy ".len() {
        special(&mut app, KeyCode::Backspace);
    }
    press(&mut app, "pun");
    special(&mut app, KeyCode::Tab);
    assert_eq!(app.picker.as_ref().unwrap().tag_line, "punchy ");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "thump.wav: #punchy");
    let kept = app.tag_library.of_file(&thump).unwrap();
    assert_eq!(kept.not, ["kick", "one-shot"]);
    press(&mut app, "#kick");
    assert_eq!(found(&app), ["kick loop.wav", "BD01.wav"]);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "#punchy");
    assert_eq!(found(&app), ["thump.wav"]);

    // `Esc` leaves the tags as they were.
    special(&mut app, KeyCode::Esc);
    press(&mut app, "T");
    press(&mut app, "x");
    special(&mut app, KeyCode::Esc);
    assert_eq!(
        app.picker.as_ref().unwrap().current().unwrap().tags.len(),
        1
    );
    assert_eq!(
        files_in(&library),
        before,
        "nothing written beside the samples"
    );
    std::fs::remove_dir_all(&library).unwrap();
}

/// A WAV of a level for a while and then silence for a while, at 48 kHz.
fn sounding(path: &Path, sound: f32, silence: f32) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..(sound * 48_000.0) as usize {
        writer.write_sample(8000_i16).unwrap();
    }
    for _ in 0..(silence * 48_000.0) as usize {
        writer.write_sample(0_i16).unwrap();
    }
    writer.finalize().unwrap();
}

#[test]
fn how_a_sample_ends_says_one_shot_or_loop_once_its_read() {
    let (mut app, _realtime) = app();
    let library = folder();
    std::fs::create_dir_all(&library).unwrap();
    sounding(&library.join("a hit.wav"), 0.1, 0.2);
    sounding(&library.join("b drone.wav"), 2.0, 0.0);
    command(&mut app, &format!("sample {}", library.display()));
    app.finish_loading();
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "a hit.wav");
    assert_eq!(
        picker.current().unwrap().tags,
        [Tag::new("one-shot", Source::Guessed)]
    );
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.tags, [Tag::new("one-shot", Source::Guessed)]);
    let text = drawn(&app);
    assert!(text.contains("│ #one-shot"), "in the preview: {text}");
    press(&mut app, "j");
    app.finish_loading();
    let picker = app.picker.as_ref().unwrap();
    let drone = picker.current_index().unwrap();
    assert_eq!(picker.item(drone).tags, [Tag::new("loop", Source::Guessed)]);
    assert_eq!(picker.icon(drone), Some(Icon::Loop), "drawn as a loop");
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn a_kits_sounds_have_their_labels_tags() {
    let (mut app, _realtime) = app();
    let kit = labelled_kit();
    let root = kit.parent().unwrap().to_path_buf();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "kit");
    // The kit isn't open, so the search reads it. Its file names say nothing, so its labels do.
    press(&mut app, "#hat");
    assert_eq!(found(&app), ["CH"]);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "#kick");
    assert_eq!(found(&app), ["BD"]);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_presets_tags_are_kept_in_its_file_and_patch_lists_the_patches() {
    let (mut app, _realtime) = app();
    let presets = folder().join("presets");
    app.settings = Settings {
        presets: presets.display().to_string(),
        ..Settings::none()
    };
    command(&mut app, "preset save Plain");
    command(&mut app, "preset");
    assert_eq!(app.picker.as_ref().unwrap().title, "Sound presets");
    press(&mut app, "#sampler");
    assert_eq!(found(&app), ["Plain"], "its machine's tag");
    special(&mut app, KeyCode::Esc);
    press(&mut app, "T");
    assert_eq!(
        app.picker.as_ref().unwrap().tag_line,
        "",
        "what it always is isn't for changing"
    );
    press(&mut app, "bass dark");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "Plain: #sound #sampler #bass #dark");
    let path = presets.join("sounds/Plain.ron");
    let read = vt_model::preset::load_sound(&path).unwrap();
    assert_eq!(read.tags, ["bass", "dark"]);
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);

    std::fs::create_dir_all(presets.join("patches")).unwrap();
    std::fs::write(
        presets.join("patches/Empty.ron"),
        "(format: 1, patch: (), tags: [\"pitch\"])",
    )
    .unwrap();
    command(&mut app, "patch");
    assert_eq!(app.picker.as_ref().unwrap().title, "Patches");
    press(&mut app, "#patch");
    assert_eq!(found(&app), ["Empty"]);
    special(&mut app, KeyCode::Esc);
    press(&mut app, "#pitch");
    assert_eq!(found(&app), ["Empty"]);
    std::fs::remove_dir_all(presets.parent().unwrap()).unwrap();
}
