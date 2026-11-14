//! The explorer, `:sample` and `:kit`: the sample and kit folders, the rows' icons, previews,
//! and choosing from it.

use std::path::PathBuf;

use ratatui::crossterm::event::KeyCode;
use ratatui::style::Color;
use vt_engine::link::Realtime;
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn a_folder_opens_the_sample_explorer_and_choosing_loads_the_sample() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-browse-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(folder.join("kit/.hidden")).unwrap();
    wav(&folder.join("kit/kick.wav"));
    wav(&folder.join("kit/snare.WAV"));
    wav(&folder.join("kit/.hidden/secret.wav"));
    std::fs::write(folder.join("kit/notes.txt"), "not audio").unwrap();

    command(
        &mut app,
        &format!("sample {}", folder.join("kit").display()),
    );
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        explorer_rows(&app)[1..],
        ["    kick.wav", "    snare.WAV"],
        "only samples, and no hidden ones"
    );
    press(&mut app, "/sn");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    let chosen = sample_of(&app, 0).unwrap();
    assert!(chosen.ends_with("snare.WAV"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn the_sample_explorer_lists_and_loads_aiff_and_flac() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-browse-formats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let made = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../vt-model/tests/samples");
    std::fs::copy(made.join("stereo-16.aif"), folder.join("hat.aif")).unwrap();
    std::fs::copy(made.join("stereo-16.aif"), folder.join("ride.aiff")).unwrap();
    std::fs::copy(made.join("stereo-16.flac"), folder.join("tom.flac")).unwrap();
    wav(&folder.join("kick.wav"));
    std::fs::write(folder.join("loop.mp3"), "not read").unwrap();

    command(&mut app, &format!("sample {}", folder.display()));
    assert_eq!(
        explorer_rows(&app)[1..],
        [
            "    hat.aif",
            "    kick.wav",
            "    ride.aiff",
            "    tom.flac"
        ]
    );
    press(&mut app, "/tom");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.status, "Track 01 plays 01 tom, loaded into the bank");
    let chosen = sample_of(&app, 0).unwrap();
    assert!(chosen.ends_with("tom.flac"), "{chosen}");
    std::fs::remove_dir_all(&folder).unwrap();
}

/// A kit folder with a kit of three labelled sounds at its top, and one of Ableton's under its
/// category, returning the folder.
fn kit_folders() -> PathBuf {
    let kit = labelled_kit();
    let root = kit.parent().unwrap().to_path_buf();
    let deeper = root.join("Ableton/Drum Machines");
    std::fs::create_dir_all(&deeper).unwrap();
    std::fs::write(
        deeper.join("808 Core Kit.ron"),
        r#"(source: "Ableton Core Library", category: "Drum Machines", sounds: [
            (label: "BD", name: "Kick 808", path: "../../drums/kick.wav"),
            (label: "SD", name: "Snare 808", path: "../../drums/snare.wav"),
        ])"#,
    )
    .unwrap();
    std::fs::write(
        deeper.join("909 Core Kit.ron"),
        r#"(sounds: [(path: "../../drums/hat.wav")])"#,
    )
    .unwrap();
    root
}

#[test]
fn kit_explores_the_kit_folders_with_each_kits_sounds_beside_them() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "kit");
    assert_eq!(app.mode, Mode::Picker);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Kits");
    assert!(picker.hint.unwrap().contains("L on tracks"));
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "  > 909"]
    );
    // The kit under the cursor is shown beside the list, each sound with its waveform.
    press(&mut app, "jj");
    let picker = app.picker.as_ref().unwrap();
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "909");
    assert_eq!(shown.note, "3 sounds");
    let labels: Vec<&str> = shown.lines.iter().map(|line| line.label.as_str()).collect();
    assert_eq!(labels, ["BD", "CH", ""]);
    assert!(shown.lines.iter().all(|line| line.wave.is_some()));

    // Into the folders, and the preview follows.
    press(&mut app, "kkll");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "808 Core Kit");
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "808 Core Kit");
    assert_eq!(shown.note, "2 sounds from Ableton Core Library");
    assert_eq!(shown.lines[1].name, "Snare 808");

    // Enter makes it the kit :sample lists first, and adds nothing.
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.kit.len(), 2);
    assert_eq!(app.history.undo_depth(), depth);
    assert!(
        app.status.starts_with("Kit 808 Core Kit: 2 sounds"),
        "{}",
        app.status
    );
    assert_eq!(
        app.recent_places.of(vt_tui::explorer::Purpose::Kit),
        [root.join("Ableton/Drum Machines")],
        "its folder is a recent place"
    );

    // It opens on the kit in use, with its folder at the top as a recent place, and L puts its
    // sounds on tracks, as one undo step.
    command(&mut app, "kit");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().unwrap().label,
        "808 Core Kit",
        "opens on it"
    );
    assert_eq!(picker.marked(), picker.current_index());
    assert_eq!(explorer_roots(&app)[0].1, "recent");
    press(&mut app, "L");
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("kick.wav"));
    assert!(sample_of(&app, 1).unwrap().ends_with("snare.wav"));
    assert_eq!(app.song().tracks[1].name, "SD");
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(sample_of(&app, 0), None);
}

#[test]
fn a_kits_preview_draws_its_sounds_with_their_waveforms() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "kit");
    press(&mut app, "jj");
    // The terminal is sent a waveform for each sound.
    let mut sent = Vec::new();
    let mut graphics = vt_tui::kitty::Graphics::new();
    graphics.sync(&mut app, &mut sent).unwrap();
    let mut picker = app.picker.clone().unwrap();
    assert_eq!(picker.preview_images.len(), 3);
    assert!(picker.preview_images.iter().all(Option::is_some));
    let backend = ratatui::backend::TestBackend::new(120, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let text: String = (0..24)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                + "\n"
        })
        .collect();
    assert!(text.contains("╭ Kits ─"), "{text}");
    assert!(
        text.contains("Enter use  L on tracks  T tag  H \u{f204}   hidden  P \u{f205}   preview"),
        "{text}"
    );
    assert!(text.contains("│ 909"), "{text}");
    let bd = text.lines().find(|line| line.contains(" BD  ")).unwrap();
    assert!(bd.contains('\u{10EEEE}'), "its waveform beside it: {bd}");
    // Moving to another kit sends that kit's, and lets the old ones go.
    press(&mut app, "kkll");
    sent.clear();
    graphics.sync(&mut app, &mut sent).unwrap();
    let sent = String::from_utf8_lossy(&sent);
    assert_eq!(sent.matches("a=T").count(), 2, "one for each of its sounds");
}

#[test]
fn sample_explores_the_sample_folders_and_opens_on_the_tracks_own() {
    let (mut app, _realtime) = app();
    let library = folder();
    std::fs::create_dir_all(library.join("hats")).unwrap();
    std::fs::create_dir_all(library.join("kicks")).unwrap();
    wav(&library.join("hats/closed.wav"));
    wav(&library.join("hats/open.wav"));
    wav(&library.join("kicks/deep.wav"));
    wav(&library.join("kicks/short.wav"));
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string()],
        presets: String::new(),
    };
    command(&mut app, "sample");
    assert_eq!(
        explorer_rows(&app)[..3],
        ["v samples", "  > hats", "  > kicks"],
        "the first place open, on the list"
    );
    press(&mut app, "jl/short");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("kicks/short.wav"));
    command(&mut app, "sample");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().unwrap().label,
        "short.wav",
        "on the track's own"
    );
    assert_eq!(picker.marked(), picker.current_index());
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v recent", "    deep.wav", "    short.wav", "> samples"],
        "in its folder, at the top as the last place a sample came from"
    );
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn the_sample_explorer_starts_from_the_project_the_settings_here_and_home() {
    let (mut app, _realtime) = app();
    let library = folder();
    std::fs::create_dir_all(library.join("hats")).unwrap();
    wav(&library.join("hats/closed.wav"));
    let project = folder();
    command(&mut app, &format!("w {}", project.display()));
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string(), "/nowhere/samples".to_owned()],
        presets: String::new(),
    };
    command(&mut app, "sample");
    assert_eq!(app.mode, Mode::Picker);
    let meanings: Vec<String> = explorer_roots(&app)
        .into_iter()
        .map(|(_, meaning)| meaning)
        .collect();
    assert_eq!(
        meanings,
        ["project", "samples", "here", "home"],
        "a folder that isn't there is passed over"
    );
    std::fs::remove_dir_all(&library).unwrap();
    std::fs::remove_dir_all(&project).unwrap();
}

#[test]
fn kits_in_folders_are_found_in_the_explorer_and_named_by_their_path() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap();
    let deeper = root.join("Ableton/Drum Machines");
    std::fs::create_dir_all(&deeper).unwrap();
    let text = std::fs::read_to_string(&kit)
        .unwrap()
        .replace("drums/", "../../drums/");
    std::fs::write(deeper.join("808 Core Kit.ron"), text).unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "kit");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "  > 909"],
        "the first kit folder open, its folders first"
    );
    press(&mut app, "ll");
    assert_eq!(
        explorer_rows(&app)[..5],
        [
            "v kits",
            "  v Ableton",
            "    v Drum Machines",
            "      > 808 Core Kit",
            "  > drums"
        ]
    );
    special(&mut app, KeyCode::Esc);
    command(&mut app, "kit Ableton/Drum Machines/808 Core Kit all");
    assert_eq!(app.status, "Kit 808 Core Kit: 3 tracks");
}

#[test]
fn the_sample_explorer_opens_a_kit_like_a_folder_and_plays_and_chooses_its_sounds() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "sample");
    assert_eq!(explorer_roots(&app)[0].1, "kits");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > Ableton", "  > drums", "  > 909"],
        "a kit is a folder, after the real ones"
    );
    // Down to a kit, by its name, and into it.
    press(&mut app, "ll");
    assert_eq!(
        app.picker.as_ref().unwrap().current().unwrap().label,
        "808 Core Kit"
    );
    press(&mut app, "l");
    assert_eq!(
        explorer_rows(&app)[3..6],
        ["      v 808 Core Kit", "          BD", "          SD"]
    );
    let picker = app.picker.as_ref().unwrap();
    let bd = picker.current().unwrap();
    assert_eq!((bd.label.as_str(), bd.meaning.as_str()), ("BD", "Kick 808"));

    // It plays as the cursor moves onto it, with its waveform beside the list.
    app.finish_loading();
    assert!(
        app.file_playing
            .as_ref()
            .is_some_and(|path| path.ends_with("drums/kick.wav")),
        "{:?}",
        app.file_playing
    );
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "kick.wav");
    assert!(shown.wave.is_some());

    // Hidden files shown keep the kit open and the cursor on its sound.
    press(&mut app, "jH");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "SD");
    press(&mut app, "H");

    // Choosing it is choosing from the kit in use: its label names the track.
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/snare.wav"));
    assert_eq!(app.song().tracks[0].name, "SD");
    assert_eq!(app.song().bank.get(1).unwrap().title(), "SD Snare 808");
}

#[test]
fn a_kit_sound_opens_to_its_variants_and_layers_and_a_missing_one_says_so() {
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap().to_path_buf();
    std::fs::write(
        root.join("layered.ron"),
        r#"(name: "Layered", sounds: [
            (label: "BD", name: "Kick", path: "drums/kick.wav", variants: ["drums/snare.wav"],
             layers: ["drums/hat.wav", "drums/kick.wav"], gain: -3.0, choke: Some(2)),
            (label: "GH", name: "Ghost", path: "drums/gone.wav"),
        ])"#,
    )
    .unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "sample");
    press(&mut app, "jjl");
    assert_eq!(
        explorer_rows(&app)[3..6],
        ["  v Layered", "    > BD", "      GH"],
        "a sound with variants or layers opens"
    );
    // l opens it: its own file, then its variants, then its layers from the softest.
    press(&mut app, "l");
    let picker = app.picker.as_ref().unwrap();
    let files: Vec<String> = picker
        .rows()
        .iter()
        .filter_map(|row| match row {
            vt_tui::input::picker::Row::Item(item) if picker.depth(*item) == 3 => {
                let item = picker.item(*item);
                Some(format!("{}  {}", item.label, item.meaning))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        files,
        [
            "kick.wav  Main",
            "snare.wav  Variant 1",
            "hat.wav  Layer 1 soft",
            "kick.wav  Layer 2 hard"
        ]
    );
    // A variant is chosen as the sound is, with its gain, label and choke group.
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/snare.wav"));
    assert_eq!(app.song().tracks[0].name, "BD");
    assert_eq!(app.song().tracks[0].choke, Some(2));
    assert_eq!(app.song().bank.get(1).unwrap().edit.play.gain_db, -3.0);
    press(&mut app, "u");

    // Enter on the sound itself chooses its own file, where l opens it.
    command(&mut app, "sample");
    press(&mut app, "jjl");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "BD");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert!(sample_of(&app, 0).unwrap().ends_with("drums/kick.wav"));
    press(&mut app, "u");

    // A sound whose file has gone is listed, and says why as a missing file does.
    command(&mut app, "sample");
    press(&mut app, "jjlj");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "GH");
    app.finish_loading();
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "gone.wav");
    assert!(shown.note.contains("gone.wav"), "{}", shown.note);
    special(&mut app, KeyCode::Enter);
    assert_eq!(sample_of(&app, 0), None);
    assert!(app.status.contains("gone.wav"), "{}", app.status);
}

/// The icon of the first row shown with this name.
fn icon_of(app: &App, label: &str) -> vt_tui::explorer::Icon {
    use vt_tui::input::picker::Row;
    let picker = app.picker.as_ref().expect("the explorer is open");
    picker
        .rows()
        .iter()
        .find_map(|row| match row {
            Row::Item(item) if picker.item(*item).label == label => picker.icon(*item),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no row {label}"))
}

/// Where the explorer draws a glyph, and its colour.
fn glyph_colour(app: &App, glyph: char) -> Option<Color> {
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let mut picker = app.picker.clone().unwrap();
    terminal
        .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let at = buffer
        .content
        .iter()
        .position(|cell| cell.symbol() == glyph.to_string())?;
    let after = &buffer.content[at + 1];
    assert_eq!(
        after.symbol(),
        "\u{a0}",
        "a no-break space keeps it one cell"
    );
    Some(buffer.content[at].fg)
}

#[test]
fn the_explorer_shows_what_each_row_is_with_an_icon() {
    use vt_tui::explorer::Icon;
    use vt_tui::glyphs;
    let (mut app, _realtime) = app();
    let kit = kit_file();
    let root = kit.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(root.join("Loops")).unwrap();
    wav(&root.join("Loops/break.wav"));
    wav(&root.join("Fill 120 bpm.wav"));
    std::fs::write(root.join("broken.wav"), "not a sound").unwrap();
    std::fs::write(
        root.join("layered.ron"),
        r#"(name: "Layered", sounds: [
            (label: "BD", name: "Kick", path: "drums/kick.wav", layers: ["drums/hat.wav"]),
            (label: "SD", name: "Snare", path: "drums/snare.wav"),
            (label: "GH", name: "Ghost", path: "drums/gone.wav"),
        ])"#,
    )
    .unwrap();
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    command(&mut app, "sample");
    assert_eq!(
        explorer_rows(&app)[..7],
        [
            "v kits",
            "  > drums",
            "  > Loops",
            "  > 909",
            "    broken.wav",
            "    Fill 120 bpm.wav",
            "  > Layered",
        ]
    );
    for (label, icon) in [
        ("drums", Icon::Folder),
        ("909", Icon::Kit),
        ("Layered", Icon::Kit),
        ("broken.wav", Icon::Sample),
        ("Fill 120 bpm.wav", Icon::Loop),
    ] {
        assert_eq!(icon_of(&app, label), icon, "{label}");
    }
    // A folder's icon is open or closed, in place of an arrow, and dim.
    assert_eq!(
        glyph_colour(&app, glyphs::FOLDER_OPEN),
        Some(vt_tui::theme::OVERLAY2)
    );
    assert_eq!(
        glyph_colour(&app, glyphs::FOLDER),
        Some(vt_tui::theme::OVERLAY2)
    );
    assert!(!picker_text(&app, 163, 42).contains('▸'), "no arrows");

    // A kit's sounds are pads, stacked where they open to more, and a warning where the file has
    // gone, in the error colour.
    press(&mut app, "5jl");
    assert_eq!(icon_of(&app, "BD"), Icon::KitSoundLayered);
    assert_eq!(icon_of(&app, "SD"), Icon::KitSound);
    assert_eq!(icon_of(&app, "GH"), Icon::Missing);
    assert_eq!(
        glyph_colour(&app, glyphs::MISSING),
        Some(vt_tui::theme::RED)
    );
    press(&mut app, "l");
    assert_eq!(
        icon_of(&app, "hat.wav"),
        Icon::Sample,
        "a layer is a sample"
    );
    press(&mut app, "hh");

    // A file that won't read is warned of once the explorer has tried it.
    press(&mut app, "kk");
    assert_eq!(
        app.picker.as_ref().unwrap().current().unwrap().label,
        "broken.wav"
    );
    app.finish_loading();
    assert_eq!(icon_of(&app, "broken.wav"), Icon::Missing);

    // The sample in use is marked by its icon in the colour of what's current.
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert!(sample_of(&app, 0).unwrap().ends_with("Fill 120 bpm.wav"));
    command(&mut app, "sample");
    assert_eq!(glyph_colour(&app, glyphs::LOOP), Some(vt_tui::theme::GREEN));
}

#[test]
fn the_sample_explorer_lists_the_kits_and_the_library_and_no_other_ron_files() {
    let (mut app, _realtime) = app();
    let root = kit_folders();
    let library = folder();
    std::fs::create_dir_all(library.join("Loops")).unwrap();
    wav(&library.join("Loops/break.wav"));
    let project = folder();
    command(&mut app, &format!("w {}", project.display()));
    app.settings = Settings {
        kits: vec![root.display().to_string()],
        samples: Vec::new(),
        presets: String::new(),
    };
    app.library = Some(library.clone());
    command(&mut app, "sample");
    let meanings: Vec<String> = explorer_roots(&app)
        .into_iter()
        .map(|(_, meaning)| meaning)
        .collect();
    assert_eq!(meanings, ["project", "kits", "library", "here", "home"]);
    // The project's own song isn't a kit, so it isn't listed.
    let rows = explorer_rows(&app);
    assert!(!rows.iter().any(|row| row.contains("song")), "{rows:?}");
    std::fs::remove_dir_all(&library).unwrap();
    std::fs::remove_dir_all(&project).unwrap();
}

/// A WAV of a steady level, this long, at 44.1 kHz in stereo.
fn long_wav(path: &std::path::Path, seconds: f32) {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).unwrap();
    for _ in 0..(seconds * 44_100.0) as usize * 2 {
        writer.write_sample(8000_i16).unwrap();
    }
    writer.finalize().unwrap();
}

/// A library of one folder with a long sample and a short one in it, as the first sample folder.
fn sample_library(app: &mut App) -> PathBuf {
    let library = folder();
    std::fs::create_dir_all(library.join("loops")).unwrap();
    long_wav(&library.join("loops/break.wav"), 2.5);
    long_wav(&library.join("loops/fill.wav"), 0.5);
    app.settings = Settings {
        kits: Vec::new(),
        samples: vec![library.display().to_string()],
        presets: String::new(),
    };
    library
}

#[test]
fn a_samples_preview_is_read_in_the_background_with_its_waveform_length_and_rate() {
    let (mut app, _realtime) = app();
    let library = sample_library(&mut app);
    command(&mut app, "sample");
    press(&mut app, "l");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "break.wav");
    let shown = picker.preview.as_deref().unwrap();
    assert_eq!(shown.heading, "break.wav");
    if app.loader.is_waiting() {
        assert_eq!(shown.note, "Loading", "a placeholder until it's read");
        assert_eq!(shown.wave, None);
    }
    app.finish_loading();
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.note, "2.50 s  44.1 kHz  stereo");
    assert!(shown.wave.is_some());

    // Its waveform is sent as an image, and drawn under the note.
    let mut sent = Vec::new();
    let mut graphics = vt_tui::kitty::Graphics::new();
    graphics.sync(&mut app, &mut sent).unwrap();
    let sent = String::from_utf8_lossy(&sent);
    assert_eq!(sent.matches("c=28,r=4").count(), 1, "28 by 4");
    let picker = app.picker.as_ref().unwrap();
    assert!(picker.preview_wave.is_some());
    let text = picker_text(&app, 120, 26);
    assert!(text.contains("│ 2.50 s  44.1 kHz  stereo"), "{text}");
    let waves = text
        .lines()
        .filter(|line| line.contains('\u{10EEEE}'))
        .count();
    assert_eq!(waves, 4, "four rows of it: {text}");

    // Moving away and back finds it kept, with nothing to wait for.
    press(&mut app, "jk");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .preview
            .as_deref()
            .unwrap()
            .note,
        "2.50 s  44.1 kHz  stereo"
    );
    // P hides the preview, and shows it again.
    press(&mut app, "P");
    let picker = app.picker.as_ref().unwrap();
    assert!(!picker.has_previews());
    assert!(picker.preview.is_none());
    press(&mut app, "P");
    assert!(app.picker.as_ref().unwrap().preview.is_some());
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn the_sample_explorers_hint_shows_whether_h_p_and_a_are_on_with_switches() {
    let (mut app, _realtime) = app();
    let library = sample_library(&mut app);
    command(&mut app, "sample");
    press(&mut app, "l");
    let on = vt_tui::glyphs::SWITCH_ON;
    let off = vt_tui::glyphs::SWITCH_OFF;
    let hint = |app: &App| {
        let text = picker_text(app, 163, 42);
        text.lines()
            .find(|line| line.contains("Enter choose"))
            .unwrap()
            .to_owned()
    };
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "╰ Enter choose  T tag  H {off}   hidden  P {on}   preview  A {on}   audition ─"
        )),
        "both on, whole: {line}"
    );
    // Each switch is green while it's on, and faint while it's off, three cells in its colour,
    // the glyph and the plain spaces its picture covers, then a space before its name.
    let switch_colour = |app: &App, glyph: char, after: &str| {
        let backend = ratatui::backend::TestBackend::new(163, 42);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mut picker = app.picker.clone().unwrap();
        terminal
            .draw(|frame| vt_tui::views::picker::draw(frame, frame.area(), &mut picker))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let area = buffer.area;
        let (x, y) = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .find(|&(x, y)| {
                buffer[(x, y)].symbol() == glyph.to_string()
                    && (1..=after.len() as u16)
                        .map(|i| buffer[(x + 3 + i, y)].symbol())
                        .collect::<String>()
                        == after
            })
            .unwrap();
        for after in [1, 2] {
            assert_eq!(
                buffer[(x + after, y)].symbol(),
                " ",
                "covered by its picture"
            );
            assert_eq!(buffer[(x + after, y)].fg, buffer[(x, y)].fg);
        }
        buffer[(x, y)].fg
    };
    assert_eq!(switch_colour(&app, on, "preview"), vt_tui::theme::GREEN);
    // With the preview off the box is at its narrowest, and the hint still fits.
    press(&mut app, "P");
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "╰ Enter choose  T tag  H {off}   hidden  P {off}   preview  A {on}   audition ─╯"
        )),
        "{line}"
    );
    assert_eq!(switch_colour(&app, off, "preview"), vt_tui::theme::OVERLAY0);
    press(&mut app, "A");
    let line = hint(&app);
    assert!(
        line.contains(&format!("P {off}   preview  A {off}   audition")),
        "{line}"
    );
    assert_eq!(
        switch_colour(&app, off, "audition"),
        vt_tui::theme::OVERLAY0
    );
    press(&mut app, "PA");
    assert_eq!(switch_colour(&app, on, "audition"), vt_tui::theme::GREEN);
    assert_eq!(switch_colour(&app, off, "hidden"), vt_tui::theme::OVERLAY0);
    press(&mut app, "H");
    let line = hint(&app);
    assert!(
        line.contains(&format!(
            "H {on}   hidden  P {on}   preview  A {on}   audition"
        )),
        "{line}"
    );
    assert_eq!(switch_colour(&app, on, "hidden"), vt_tui::theme::GREEN);
    std::fs::remove_dir_all(&library).unwrap();
}

#[test]
fn moving_onto_a_sample_plays_it_on_the_track_until_the_cursor_moves_off() {
    let (mut app, mut realtime) = app();
    let library = sample_library(&mut app);
    let heard = |realtime: &mut Realtime, app: &mut App| {
        let mut out = vec![0.0; 4096];
        realtime.process(&mut out);
        app.tick();
        out[out.len() - 2].abs()
    };
    command(&mut app, "sample");
    assert_eq!(app.file_wanted, None, "a folder plays nothing");
    press(&mut app, "l");
    app.finish_loading();
    assert_eq!(app.file_playing, Some(library.join("loops/break.wav")));
    assert!(heard(&mut realtime, &mut app) > 0.01, "it plays");
    assert_eq!(realtime.engine().previews_sounding(), 1);
    assert!(sample_of(&app, 0).is_none(), "and isn't put on the track");

    // The next one takes over.
    press(&mut app, "j");
    app.finish_loading();
    assert_eq!(app.file_playing, Some(library.join("loops/fill.wav")));
    heard(&mut realtime, &mut app);

    // A turns it off, and stops what's playing.
    press(&mut app, "A");
    assert_eq!(app.file_playing, None);
    for _ in 0..8 {
        heard(&mut realtime, &mut app);
    }
    assert_eq!(realtime.engine().previews_sounding(), 0, "stopped");
    press(&mut app, "k");
    assert_eq!(app.file_wanted, None, "nothing plays while it's off");
    press(&mut app, "A");
    app.finish_loading();
    assert!(app.file_playing.is_some(), "A turns it on again");

    // Leaving the explorer stops it.
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.file_playing, None);
    for _ in 0..8 {
        heard(&mut realtime, &mut app);
    }
    assert_eq!(realtime.engine().previews_sounding(), 0);
    // Turned off, it stays off the next time.
    command(&mut app, "sample");
    press(&mut app, "lA");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "sample");
    press(&mut app, "l");
    assert!(!app.picker.as_ref().unwrap().tree().unwrap().audition);
    assert_eq!(app.file_wanted, None);
    std::fs::remove_dir_all(&library).unwrap();
}
