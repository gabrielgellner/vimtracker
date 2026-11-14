//! Projects: saving, opening and starting them, their names and the window title, autosave,
//! rendering, grooves, and the project page.

use std::path::PathBuf;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Realtime;
use vt_model::groove::GrooveId;
use vt_model::song::Song;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::keys::parse_sequence;
use vt_tui::input::mode::{Mode, View};

mod common;
use common::*;

#[test]
fn quit_refuses_unsaved_changes() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " qq");
    assert!(!app.should_quit);
    assert!(app.status.contains("Unsaved"));
    command(&mut app, "q!");
    assert!(app.should_quit);
}

#[test]
fn saving_needs_a_folder_then_round_trips() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert!(app.dirty);
    assert_eq!(app.mode, Mode::Picker, "the explorer chooses where");
    assert_eq!(app.picker.as_ref().unwrap().title, "Save in");
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.naming, None, "nothing waits for a name");
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(app.folder.as_deref(), Some(path.as_path()));

    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(notes(&other)[0], Note::On(60));
    assert!(!other.dirty);
}

#[test]
fn the_window_title_names_the_project() {
    let (mut app, _rt) = app();
    assert_eq!(
        app.window_title(),
        format!("vimtracker: {}", app.project_name()),
        "its random name before it's saved"
    );
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        app.window_title(),
        format!("vimtracker: {name}"),
        "after save as"
    );

    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(
        other.window_title(),
        format!("vimtracker: {name}"),
        "after open"
    );
    let fresh = folder();
    command(&mut other, &format!("new {}", fresh.display()));
    let name = fresh.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        other.window_title(),
        format!("vimtracker: {name}"),
        "after new"
    );
}

#[test]
fn opening_refuses_unsaved_changes() {
    let (mut app, _rt) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, &format!("new {}", path.display()));
    assert!(app.status.contains("Unsaved"));
    assert!(!path.exists());
}

#[test]
fn groove_commands_set_the_project_and_the_cursors_track_and_undo() {
    let (mut app, _realtime) = app();
    let groove = |name: &str| GrooveId::named(name).unwrap();
    command(&mut app, "swing 66");
    assert_eq!(app.song().groove, groove("Swing 66"));
    command(&mut app, "swing 67");
    assert_eq!(
        app.song().groove,
        groove("Swing 66"),
        "only a swing groove's percent"
    );
    assert!(app.status.contains("52, 54"), "{}", app.status);
    command(&mut app, "groove mpc-58");
    assert_eq!(app.song().groove, groove("MPC 58"));
    command(&mut app, "tgroove pull");
    assert_eq!(app.song().tracks[0].groove, Some(groove("Pull")));
    assert_eq!(app.song().tracks[1].groove, None);
    command(&mut app, "tgroove off");
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "uu");
    assert_eq!(app.song().groove, groove("MPC 58"));
    assert_eq!(app.song().tracks[0].groove, None);
    press(&mut app, "u");
    assert_eq!(app.song().groove, groove("Swing 66"));
    press(&mut app, "u");
    assert_eq!(app.song().groove, GrooveId::STRAIGHT);
}

#[test]
fn a_plain_write_keeps_live_mutes_and_solos() {
    let (mut app, _realtime) = app();
    let folder = std::env::temp_dir().join(format!("vt-mute-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    command(&mut app, &format!("w {}", folder.display()));
    tap(&mut app, " m");
    command(&mut app, "w");
    let saved = vt_model::project::load_song(&folder).unwrap();
    assert!(saved.tracks[0].mute);
    std::fs::remove_dir_all(&folder).unwrap();
}

#[test]
fn render_writes_what_is_in_memory_with_live_mutes() {
    let (mut app, _realtime) = app();
    let path = std::env::temp_dir().join(format!("vt-app-render-{}.wav", std::process::id()));
    let _ = std::fs::remove_file(&path);
    command(&mut app, &format!("render {} --rows 4", path.display()));
    assert!(app.status.contains("Rendered"), "{}", app.status);
    let reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.len(), 24_000 * 2);
    std::fs::remove_file(&path).unwrap();
    assert!(!app.dirty, "rendering isn't an edit");
    command(&mut app, "render /no/such/folder/x.wav");
    assert!(app.status.contains("Couldn't write"), "{}", app.status);
}

mod autosave {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use ratatui::crossterm::event::KeyCode;
    use vt_model::project;
    use vt_model::song::Song;
    use vt_model::step::Note;
    use vt_tui::input::mode::Mode;

    use super::{app, command, press, special};

    const AUTOSAVE: &str = "song.autosave.ron";

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-auto-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    fn later(start: Instant, seconds: u64) -> Instant {
        start + Duration::from_secs(seconds)
    }

    #[test]
    fn only_a_changed_project_is_written_and_only_when_due() {
        let path = folder("changed");
        let (mut app, _realtime) = app();
        command(&mut app, &format!("w {}", path.display()));
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(!path.join(AUTOSAVE).exists(), "nothing has changed");

        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        app.autosave_if_due(later(start, 62));
        assert!(!path.join(AUTOSAVE).exists(), "not due yet");
        app.autosave_if_due(later(start, 125));
        assert!(path.join(AUTOSAVE).exists());
        let saved = project::load_song(&path).unwrap();
        assert_eq!(
            saved.tracks[0].phrases[&0].steps()[0].note,
            Note::Empty,
            "song.ron is untouched"
        );

        // No change since then means no new write.
        std::fs::remove_file(path.join(AUTOSAVE)).unwrap();
        app.autosave_if_due(later(start, 190));
        assert!(!path.join(AUTOSAVE).exists());

        // A real save clears it.
        press(&mut app, "is");
        special(&mut app, KeyCode::Esc);
        app.autosave_if_due(later(start, 260));
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "w");
        assert!(!path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn an_unsaved_project_with_no_folder_is_left_alone() {
        let (mut app, _realtime) = app();
        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(app.dirty);
        assert!(app.status.is_empty(), "{}", app.status);
    }

    /// A folder as a crash would leave it: a saved song, and newer work beside it.
    fn crashed(path: &Path) {
        project::save_song(path, &Song::new(2)).unwrap();
        let mut newer = Song::new(2);
        std::sync::Arc::make_mut(newer.tracks[0].phrases.get_mut(&0).unwrap())
            .step_mut(0)
            .unwrap()
            .note = Note::On(60);
        project::write_autosave(path, &newer).unwrap();
    }

    #[test]
    fn a_newer_autosave_is_offered_and_restores_as_unsaved() {
        let path = folder("restore");
        crashed(&path);
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        assert_eq!(app.mode, Mode::Restore);
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::Empty
        );
        // Other keys don't answer.
        press(&mut app, "q");
        special(&mut app, KeyCode::Esc);
        assert_eq!(app.mode, Mode::Restore);
        press(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::On(60)
        );
        assert!(app.dirty);
        command(&mut app, "w");
        let saved = project::load_song(&path).unwrap();
        assert_eq!(saved.tracks[0].phrases[&0].steps()[0].note, Note::On(60));
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn declining_deletes_the_autosave_and_keeps_the_song() {
        let path = folder("decline");
        crashed(&path);
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        press(&mut app, "n");
        assert_eq!(app.mode, Mode::Normal);
        assert!(!path.join(AUTOSAVE).exists());
        assert_eq!(
            app.song().tracks[0].phrases[&0].steps()[0].note,
            Note::Empty
        );
        assert!(!app.dirty);
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn an_unreadable_autosave_is_kept_and_reported() {
        let path = folder("broken");
        project::save_song(&path, &Song::new(2)).unwrap();
        std::fs::write(path.join(AUTOSAVE), "(format_version: 99)").unwrap();
        let (mut app, _realtime) = app();
        command(&mut app, &format!("e {}", path.display()));
        press(&mut app, "y");
        assert_eq!(app.mode, Mode::Normal);
        assert!(app.status.contains("kept"), "{}", app.status);
        assert!(path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }

    #[test]
    fn quitting_on_purpose_removes_the_autosave() {
        let path = folder("quit");
        let (mut app, _realtime) = app();
        command(&mut app, &format!("w {}", path.display()));
        press(&mut app, "ia");
        special(&mut app, KeyCode::Esc);
        let start = Instant::now();
        app.autosave_if_due(start);
        app.autosave_if_due(later(start, 61));
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "q");
        assert!(!app.should_quit, "unsaved work needs :q!");
        assert!(path.join(AUTOSAVE).exists());
        command(&mut app, "q!");
        assert!(app.should_quit);
        assert!(!path.join(AUTOSAVE).exists());
        std::fs::remove_dir_all(&path).unwrap();
    }
}

#[test]
fn render_stems_writes_a_named_file_per_track() {
    let (mut app, _realtime) = app();
    with_a_sound(&mut app);
    command(&mut app, "name bd");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let out = folder().join("stems");
    command(&mut app, &format!("render {} --stems", out.display()));
    assert!(app.status.contains("2 stems"), "{}", app.status);
    assert!(out.join("01-BD.wav").exists());
    assert!(out.join("02.wav").exists());
    command(&mut app, "mute");
    let quieter = folder().join("stems");
    command(&mut app, &format!("render {} --stems", quieter.display()));
    assert!(app.status.contains("1 stem to"), "{}", app.status);
    assert!(!quieter.join("01-BD.wav").exists());
}

/// A folder of two projects and a plain folder.
fn projects() -> PathBuf {
    let root = folder();
    for (name, bpm) in [("one", 120_000), ("two", 137_630)] {
        let mut song = Song::new(4);
        song.millibpm = bpm;
        vt_model::project::save_song(&root.join(name), &song).unwrap();
    }
    std::fs::create_dir_all(root.join("ideas")).unwrap();
    std::fs::write(root.join("notes.txt"), "").unwrap();
    root
}

#[test]
fn e_with_no_folder_opens_a_project_from_beside_this_one_with_its_preview() {
    let root = projects();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("e {}", root.join("one").display()));
    assert_eq!(app.folder.as_deref(), Some(root.join("one").as_path()));
    command(&mut app, "e");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().title, "Open a project");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v projects", "  > ideas", "    one", "    two"],
        "projects and folders only, on this one"
    );
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.current().unwrap().label, "one");
    assert_eq!(picker.marked(), picker.current_index());
    press(&mut app, "j");
    let shown = app.picker.as_ref().unwrap().preview.clone().unwrap();
    assert_eq!(shown.heading, "two");
    assert_eq!(shown.note, "137.63 BPM  4 tracks");
    assert_eq!(shown.details, ["Saved just now"]);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.folder.as_deref(), Some(root.join("two").as_path()));
    assert_eq!(app.song().millibpm, 137_630);
    assert_eq!(
        app.recent_places.of(vt_tui::explorer::Purpose::Project),
        std::slice::from_ref(&root)
    );

    // Unsaved changes stop it before anything opens.
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "e");
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.status.contains("Unsaved"), "{}", app.status);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn e_bang_opens_a_project_and_discards_unsaved_changes() {
    let root = projects();
    let (mut app, _realtime) = app();
    let one = root.join("one");
    command(&mut app, &format!("e {}", one.display()));
    command(&mut app, "bpm 90");
    assert!(app.dirty);
    vt_model::project::write_autosave(&one, app.song()).unwrap();

    // :e refuses, and says how to go on.
    command(&mut app, &format!("e {}", root.join("two").display()));
    assert_eq!(
        app.status,
        "Unsaved changes: :w to save, or :e! to discard them"
    );
    assert_eq!(app.folder.as_deref(), Some(one.as_path()));

    // :e! opens it, and the changes' autosave goes with them.
    command(&mut app, &format!("e! {}", root.join("two").display()));
    assert_eq!(app.folder.as_deref(), Some(root.join("two").as_path()));
    assert_eq!(app.song().millibpm, 137_630);
    assert!(!app.dirty);
    assert!(!one.join("song.autosave.ron").exists());

    // On this project's own folder, it goes back to how it was saved.
    command(&mut app, "bpm 90");
    command(&mut app, &format!("e! {}", root.join("two").display()));
    assert_eq!(app.song().millibpm, 137_630);
    assert!(!app.dirty);
    assert_eq!(app.mode, Mode::Normal, "with nothing to restore");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn e_bang_with_no_folder_opens_what_the_explorer_chooses_without_refusing() {
    let root = projects();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("e {}", root.join("one").display()));
    command(&mut app, "bpm 90");
    command(&mut app, "e!");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(app.picker.as_ref().unwrap().title, "Open a project");
    press(&mut app, "j");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.folder.as_deref(), Some(root.join("two").as_path()));
    assert_eq!(app.song().millibpm, 137_630);
    assert!(!app.dirty);

    // An :e! explorer closed unused leaves later opening to refuse as before.
    command(&mut app, "e!");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "bpm 90");
    command(&mut app, &format!("e {}", root.join("one").display()));
    assert!(app.status.contains(":e!"), "{}", app.status);
    assert_eq!(app.folder.as_deref(), Some(root.join("two").as_path()));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn new_bang_starts_a_project_and_discards_unsaved_changes() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let path = folder();
    command(&mut app, "new");
    assert_eq!(
        app.status,
        "Unsaved changes: :w to save, or :new! to discard them"
    );
    assert_ne!(app.mode, Mode::Picker);
    command(&mut app, &format!("new {}", path.display()));
    assert!(!path.exists());

    // Without a folder, :new! chooses one, and its name line is :new!'s.
    command(&mut app, "new!");
    assert_eq!(app.mode, Mode::Picker);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    assert!(
        app.command_line.starts_with("new! "),
        "{}",
        app.command_line
    );
    special(&mut app, KeyCode::Esc);

    command(&mut app, &format!("new! {}", path.display()));
    assert_eq!(app.folder.as_deref(), Some(path.as_path()));
    assert!(!app.dirty);
    assert!(
        !notes(&app).iter().any(|note| matches!(note, Note::On(_))),
        "a fresh song"
    );
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn saving_a_new_project_chooses_a_folder_then_names_it_on_the_command_line() {
    let root = projects();
    let (mut app, _realtime) = app();
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v recent", "  > ideas", "  > one", "  > two"],
        "folders only, the recent place first"
    );
    let picker = app.picker.as_ref().unwrap();
    assert!(!picker.has_previews(), "nothing to show");
    // l opens a folder, and Enter chooses one.
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    let shown = vt_tui::explorer::place_name(&root, Some(&home));
    // The project's random name is filled in after the folder, so Enter saves it.
    let name = app.project_name();
    assert_eq!(app.command_line, format!("w {shown}/{name}"));
    assert!(app.status.contains("name"), "{}", app.status);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.folder.as_deref(), Some(root.join(&name).as_path()));
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(app.project_name(), name, "the folder's name now");

    // :new does the same, with a new name, which can be typed over.
    command(&mut app, "new");
    assert_eq!(app.picker.as_ref().unwrap().title, "Save in");
    special(&mut app, KeyCode::Enter);
    let line = app.command_line.clone();
    assert!(line.starts_with(&format!("new {shown}/")), "{line}");
    assert!(!line.ends_with(&format!("/{name}")), "{line} is a new name");
    for _ in line.rsplit('/').next().unwrap().chars() {
        special(&mut app, KeyCode::Backspace);
    }
    press(&mut app, "fresh");
    special(&mut app, KeyCode::Enter);
    assert!(root.join("fresh/song.ron").is_file(), "{}", app.status);
    std::fs::remove_dir_all(&root).unwrap();
}

/// Whether a name is two lower case words joined by a hyphen, as random names are.
fn random_name_like(name: &str) -> bool {
    name.split_once('-').is_some_and(|(first, second)| {
        [first, second]
            .iter()
            .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase()))
    })
}

#[test]
fn a_new_project_has_a_random_name_numbered_in_a_folder_that_has_it() {
    let root = projects();
    let (mut app, _realtime) = app();
    let name = app.project_name();
    assert!(random_name_like(&name), "{name}");
    let (other, _other_realtime) = self::app();
    assert_ne!(other.project_name(), name, "each starts with its own");
    // A folder with the name already in it gets it numbered, as the project keeps its name.
    std::fs::create_dir_all(root.join(&name)).unwrap();
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut app, "w");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let given = app.command_line.rsplit('/').next().unwrap().to_owned();
    assert_eq!(
        given,
        format!("{name}-2"),
        "{name} is in the folder already"
    );
    // :new is a new project, so it starts with a new random name.
    special(&mut app, KeyCode::Esc);
    command(&mut app, "new");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let fresh = app.command_line.rsplit('/').next().unwrap().to_owned();
    assert!(random_name_like(&fresh), "{fresh}");
    assert_ne!(fresh, name);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn ctrl_r_rolls_a_new_name_on_a_name_line() {
    let root = projects();
    let (mut app, _realtime) = app();
    let roll = parse_sequence("C-r").unwrap()[0];
    let typed = format!("w {}/typed", root.display());
    press(&mut app, ":");
    press(&mut app, &typed);
    app.key(roll, Instant::now());
    let first = app.command_line.clone();
    let folder = format!("w {}/", root.display());
    assert!(first.starts_with(&folder), "{first}");
    assert!(random_name_like(&first[folder.len()..]), "{first}");
    app.key(roll, Instant::now());
    assert_ne!(app.command_line, first, "each press rolls again");
    assert_eq!(app.mode, Mode::Command);
    special(&mut app, KeyCode::Enter);
    let saved = app.folder.clone().unwrap();
    assert_eq!(saved.parent(), Some(root.as_path()), "{}", app.status);

    // A render's extension and flags stay.
    command(&mut app, "render --tail");
    special(&mut app, KeyCode::Enter);
    let line = app.command_line.clone();
    let project = app.project_name();
    assert!(line.ends_with(&format!("/{project}.wav --tail")), "{line}");
    app.key(roll, Instant::now());
    assert!(
        app.command_line.ends_with(".wav --tail"),
        "{}",
        app.command_line
    );
    assert!(!app.command_line.contains(&format!("/{project}.wav")));

    // A line that names nothing is left alone.
    special(&mut app, KeyCode::Esc);
    press(&mut app, ":bpm 12");
    app.key(roll, Instant::now());
    assert_eq!(app.command_line, "bpm 12");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn render_with_no_path_chooses_a_folder_and_keeps_its_flags_after_the_name() {
    let root = projects();
    let (mut app, _realtime) = app();
    command(&mut app, &format!("e {}", root.join("one").display()));
    command(&mut app, "render --tail");
    assert_eq!(app.picker.as_ref().unwrap().title, "Render into");
    // Choosing a folder shows nothing beside it, so only `H` has a switch.
    let text = picker_text(&app, 163, 42);
    assert!(
        text.contains("╰ l open  Enter choose  H \u{f204}   hidden ─"),
        "{text}"
    );
    assert_eq!(
        explorer_rows(&app)[0],
        "v project",
        "the project's folder first"
    );
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
    let shown = vt_tui::explorer::place_name(&root.join("one"), Some(&home));
    assert_eq!(app.command_line, format!("render {shown}/one.wav --tail"));
    special(&mut app, KeyCode::Enter);
    assert!(root.join("one/one.wav").is_file(), "{}", app.status);
    // Another render is numbered, rather than written over the first.
    command(&mut app, "render");
    press(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.command_line, format!("render {shown}/one-2.wav"));
    special(&mut app, KeyCode::Esc);

    // A project with no folder yet renders under its random name.
    let (mut unsaved, _unsaved_realtime) = self::app();
    unsaved
        .recent_places
        .add(vt_tui::explorer::Purpose::Render, &root)
        .unwrap();
    command(&mut unsaved, "render");
    special(&mut unsaved, KeyCode::Enter);
    let name = unsaved.project_name();
    assert!(
        unsaved.command_line.ends_with(&format!("/{name}.wav")),
        "{}",
        unsaved.command_line
    );
    std::fs::remove_dir_all(&root).unwrap();
}

/// Open the project page, with the cursor on its first row.
fn on_the_project_page() -> (App, Realtime) {
    let (mut app, realtime) = app();
    tap(&mut app, " 9");
    assert_eq!(app.view, View::Project);
    assert_eq!(app.key_scope(), Mode::Project);
    (app, realtime)
}

#[test]
fn the_project_page_sets_the_tempo_and_its_fine_part_as_bpm_does_with_undo() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Bpm);
    tap(&mut app, "=");
    assert_eq!(app.song().millibpm, 121_000);
    tap(&mut app, "+");
    assert_eq!(app.song().millibpm, 131_000, "a large step is 10 BPM");
    tap(&mut app, "u");
    assert_eq!(app.song().millibpm, 121_000, "each change is an undo step");
    // l goes onto the fine part, which moves by hundredths and carries into the whole.
    tap(&mut app, "l");
    assert!(app.project_fine);
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    tap(&mut app, "-");
    assert_eq!(app.song().millibpm, 120_990);
    tap(&mut app, "x");
    assert_eq!(app.song().millibpm, 120_000, "x on the fine part clears it");
    // Typing on the fine part is the digits after the point.
    tap(&mut app, "i63");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 120_630);
    // On the whole, it's what :bpm takes, and says what :bpm says.
    tap(&mut app, "h");
    tap(&mut app, "i137.635");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 137_635);
    tap(&mut app, "i9");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().millibpm, 137_635);
    assert!(app.status.contains("20 to 400"), "{}", app.status);
    tap(&mut app, "ifast");
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains(":bpm takes"), "{}", app.status);
    tap(&mut app, "x");
    assert_eq!(
        app.song().millibpm,
        120_000,
        "x goes back to a new project's"
    );
    // Cmd with the movement keys changes it too, as everywhere.
    app.key(parse_sequence("D-l").unwrap()[0], Instant::now());
    assert_eq!(app.song().millibpm, 121_000);
}

#[test]
fn the_project_page_edits_the_values_its_commands_set() {
    use vt_model::song::Launch;
    use vt_tui::project_page::{PROJECT_ROWS, ProjectControl};
    let (mut app, _realtime) = on_the_project_page();

    project_cursor_on(&mut app, ProjectControl::Groove);
    tap(&mut app, "=");
    assert_eq!(app.song().groove, GrooveId::swing(52).unwrap());
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/mpc 66");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().groove, GrooveId::named("MPC 66").unwrap());
    tap(&mut app, "x");
    assert_eq!(app.song().groove, GrooveId::STRAIGHT);
    tap(&mut app, "u");
    assert_eq!(
        app.song().groove,
        GrooveId::named("MPC 66").unwrap(),
        "undo, as :groove has"
    );

    project_cursor_on(&mut app, ProjectControl::Launch);
    assert_eq!(app.song().launch, Launch::named("1bar").unwrap());
    tap(&mut app, "=");
    assert_eq!(app.song().launch, Launch::named("2bar").unwrap());
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/chain");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().launch, Launch::ChainEnd);
    tap(&mut app, "i12");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().launch, Launch::every(12).unwrap());
    tap(&mut app, "=");
    assert_eq!(
        app.song().launch,
        Launch::named("1bar").unwrap(),
        "a grid with no name goes to the named one above it"
    );
    tap(&mut app, "u");
    assert_eq!(app.song().launch, Launch::every(12).unwrap());

    project_cursor_on(&mut app, ProjectControl::Reset);
    tap(&mut app, "+");
    assert_eq!(app.song().sections[0].reset, Some(16));
    tap(&mut app, "i0x40");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().sections[0].reset, Some(64), "as :reset reads it");
    tap(&mut app, "x");
    assert_eq!(app.song().sections[0].reset, None);
    tap(&mut app, "u");
    assert_eq!(app.song().sections[0].reset, Some(64));

    project_cursor_on(&mut app, ProjectControl::Seed);
    tap(&mut app, "i42");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().seed, 42);
    tap(&mut app, "+");
    assert_eq!(app.song().seed, 52);
    tap(&mut app, "u");
    assert_eq!(app.song().seed, 42);

    // The limiter is on the master's Sound page now, not here.
    assert!(
        !project_page_lines(&app)
            .iter()
            .any(|line| line.contains("Limiter")),
        "{:#?}",
        project_page_lines(&app)
    );
    assert!(
        PROJECT_ROWS
            .iter()
            .all(|row| !row.iter().any(|control| control.label() == "Limiter"))
    );

    project_cursor_on(&mut app, ProjectControl::Grid);
    assert_eq!(app.song().skip, 4);
    tap(&mut app, "=");
    assert_eq!(app.song().skip, 8);
    tap(&mut app, "i3");
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.song().skip,
        3,
        "any number of steps, as :set skip takes"
    );
    tap(&mut app, "K");
    tap(&mut app, "/1/16");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().skip, 1);
    tap(&mut app, "u");
    assert_eq!(app.song().skip, 3);
}

#[test]
fn the_project_page_sets_the_scale_its_root_and_auto_jump() {
    use vt_model::scale::ScaleKind;
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Scale);
    assert!(!app.scale.on);
    tap(&mut app, "=");
    assert!(app.scale.on);
    assert_eq!(app.scale.kind, ScaleKind::ALL[0]);
    tap(&mut app, "-");
    assert!(!app.scale.on, "off is the first choice");
    // i lists them, as K does.
    tap(&mut app, "i");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/dorian");
    special(&mut app, KeyCode::Enter);
    assert!(app.scale.on);
    assert_eq!(app.scale.kind, ScaleKind::Dorian);

    // The root moves by a semitone, and keeps the scale as it is.
    tap(&mut app, "j");
    assert_eq!(app.project_control(), ProjectControl::Root);
    tap(&mut app, "=");
    assert_eq!(app.scale.root, 1);
    tap(&mut app, "if#");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.scale.root, 6);
    tap(&mut app, "x");
    assert_eq!(app.scale.root, 0);
    project_cursor_on(&mut app, ProjectControl::Scale);
    tap(&mut app, "x");
    assert!(!app.scale.on);
    project_cursor_on(&mut app, ProjectControl::Root);
    tap(&mut app, "=");
    assert!(!app.scale.on, "the root alone doesn't turn it on");
    assert_eq!(app.scale.root, 1);

    project_cursor_on(&mut app, ProjectControl::AutoJump);
    assert!(!app.auto_jump);
    tap(&mut app, "=");
    assert!(app.auto_jump);
    assert!(app.dirty, "it's kept with the song");
    tap(&mut app, "K");
    assert!(!app.auto_jump);
}

#[test]
fn the_project_page_names_the_project_and_the_window_title_says_it() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    let random = app.project_name();
    assert_eq!(app.song().name, random, "the song holds its name");
    assert_eq!(app.window_title(), format!("vimtracker: {random}"));
    assert_eq!(app.project_control(), ProjectControl::Name);
    // i starts with the name as it is.
    tap(&mut app, "i");
    assert_eq!(app.mode, Mode::Insert);
    assert_eq!(app.entry.text, random);
    app.key(parse_sequence("C-u").unwrap()[0], Instant::now());
    tap(&mut app, "Verse Ideas");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), "Verse Ideas");
    assert_eq!(app.window_title(), "vimtracker: Verse Ideas");
    assert!(app.dirty);
    tap(&mut app, "u");
    assert_eq!(app.project_name(), random, "renaming is an undo step");
    // C-r while typing rolls a new name.
    tap(&mut app, "i");
    app.key(parse_sequence("C-r").unwrap()[0], Instant::now());
    let rolled = app.entry.text.clone();
    assert!(random_name_like(&rolled), "{rolled}");
    assert_ne!(rolled, random);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), rolled);
    // A name that couldn't be a folder's is refused.
    tap(&mut app, "i");
    app.key(parse_sequence("C-u").unwrap()[0], Instant::now());
    tap(&mut app, "a/b");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.project_name(), rolled);
    assert!(app.status.contains("no /"), "{}", app.status);
}

#[test]
fn saving_a_project_somewhere_new_names_it_after_the_folder_and_keeps_the_name_after() {
    let root = projects();
    let (mut app, _realtime) = app();
    app.apply(&vt_model::action::Action::SetSongName("verse".to_owned()));
    // Saving starts with its name.
    app.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut app, "w");
    tap(&mut app, "k");
    special(&mut app, KeyCode::Enter);
    assert!(app.command_line.ends_with("/verse"), "{}", app.command_line);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.folder.as_deref(), Some(root.join("verse").as_path()));
    // Renaming keeps the folder, and Save as starts with the new name.
    app.apply(&vt_model::action::Action::SetSongName("chorus".to_owned()));
    command(&mut app, "w");
    assert_eq!(app.folder.as_deref(), Some(root.join("verse").as_path()));
    let (mut other, _other_realtime) = self::app();
    command(&mut other, &format!("e {}", root.join("verse").display()));
    assert_eq!(
        other.project_name(),
        "chorus",
        "the name is kept in the song"
    );
    // A copy of the same name in the same place is numbered.
    command(&mut other, "w");
    let (mut copy, _copy_realtime) = self::app();
    copy.apply(&vt_model::action::Action::SetSongName("one".to_owned()));
    copy.recent_places
        .add(vt_tui::explorer::Purpose::Save, &root)
        .unwrap();
    command(&mut copy, "w");
    tap(&mut copy, "k");
    special(&mut copy, KeyCode::Enter);
    assert!(
        copy.command_line.ends_with("/one-2"),
        "{}",
        copy.command_line
    );
    // A name typed in its place names the project after it.
    for _ in 0.."one-2".len() {
        special(&mut copy, KeyCode::Backspace);
    }
    tap(&mut copy, "bridge");
    special(&mut copy, KeyCode::Enter);
    assert_eq!(copy.project_name(), "bridge");
    // An old song with no name takes its folder's.
    let mut nameless = Song::new(1);
    nameless.name.clear();
    vt_model::project::save_song(&root.join("old"), &nameless).unwrap();
    command(&mut copy, &format!("e {}", root.join("old").display()));
    assert_eq!(copy.project_name(), "old");
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn the_project_pages_actions_open_the_explorer_and_its_audio_row_the_audio_page() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    for (action, title) in [
        (ProjectControl::Save, "Save in"),
        (ProjectControl::SaveAs, "Save in"),
        (ProjectControl::Render, "Render into"),
        (ProjectControl::Open, "Open a project"),
        (ProjectControl::New, "Save in"),
    ] {
        project_cursor_on(&mut app, action);
        special(&mut app, KeyCode::Enter);
        assert_eq!(app.mode, Mode::Picker, "{action:?}");
        assert_eq!(app.picker.as_ref().unwrap().title, title, "{action:?}");
        special(&mut app, KeyCode::Esc);
        assert_eq!(app.view, View::Project, "{action:?} comes back to the page");
    }
    // A folder chosen goes on to the name line, as :w's does.
    project_cursor_on(&mut app, ProjectControl::SaveAs);
    tap(&mut app, "K");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    let name = app.project_name();
    assert!(
        app.command_line.starts_with("w ") && app.command_line.ends_with(&format!("/{name}")),
        "{}",
        app.command_line
    );
    special(&mut app, KeyCode::Esc);
    // With changes, open and new refuse, as :e and :new do.
    project_cursor_on(&mut app, ProjectControl::Seed);
    tap(&mut app, "=");
    project_cursor_on(&mut app, ProjectControl::Open);
    special(&mut app, KeyCode::Enter);
    assert_ne!(app.mode, Mode::Picker);
    assert!(app.status.contains("Unsaved"), "{}", app.status);

    project_cursor_on(&mut app, ProjectControl::Audio);
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.view, View::Audio);
    assert!(app.audio.refresh, "it lists the devices afresh");
}

#[test]
fn the_project_page_keeps_the_column_through_j_and_k() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::Root);
    tap(&mut app, "jj");
    assert_eq!(app.project_control(), ProjectControl::AutoJump);
    tap(&mut app, "j");
    assert_eq!(app.project_control(), ProjectControl::Seed, "a row of one");
    tap(&mut app, "j");
    assert_eq!(app.project_control(), ProjectControl::GrooveShown);
    tap(&mut app, "j");
    assert_eq!(
        app.project_control(),
        ProjectControl::Humanise,
        "back in the right column"
    );
    tap(&mut app, "kkk");
    assert_eq!(app.project_control(), ProjectControl::AutoJump);
    // A count goes further, and gg and G go to the ends, keeping it too.
    tap(&mut app, "2k");
    assert_eq!(app.project_control(), ProjectControl::Root);
    tap(&mut app, "G");
    assert_eq!(app.project_control(), ProjectControl::New);
    tap(&mut app, "g");
    tap(&mut app, "g");
    assert_eq!(app.project_control(), ProjectControl::Name);
    project_cursor_on(&mut app, ProjectControl::Scale);
    tap(&mut app, "0");
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    assert!(!app.project_fine);
    tap(&mut app, "$");
    assert_eq!(app.project_control(), ProjectControl::Scale);
    // h from the right column lands on the tempo's fine part, as the track page's does.
    tap(&mut app, "h");
    assert_eq!(app.project_control(), ProjectControl::Bpm);
    assert!(app.project_fine);
    // The status line names the row.
    assert_eq!(vt_tui::views::status::column_name(&app), "BPM");
}

/// The project page as text, a line a row.
fn project_page_lines(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(120, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::project::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..40)
        .map(|y| {
            (0..120)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn the_project_page_draws_the_projects_groove_as_ticks_by_its_pushes() {
    let (mut app, _realtime) = on_the_project_page();
    command(&mut app, "swing 66");
    let lines = project_page_lines(&app);
    let at = |start: &str| {
        lines
            .iter()
            .position(|line| line.starts_with(start))
            .unwrap_or_else(|| panic!("{start}: {lines:#?}"))
    };
    let edit = at("Edit");
    assert!(lines[edit].contains("Swing 66"), "{}", lines[edit]);
    let steps = at("Steps");
    assert!(lines[steps].contains("Steps     2 "), "{}", lines[steps]);
    assert!(lines[steps].contains("Humanise  0 ticks"));
    let push = at("Push");
    assert!(lines[push].contains("    0  +77  ticks"), "{}", lines[push]);
    assert!(lines[at("Velocity")].contains("  100  100  %"));
    // A tick above the line over the late step, and none under it.
    assert!(
        lines[push - 3].trim_end().ends_with('▆'),
        "{}",
        lines[push - 3]
    );
    assert!(lines[push - 2].contains("──────────"));
    assert!(lines[push - 1].trim().is_empty());
}

#[test]
fn changing_a_groove_that_comes_with_it_makes_one_of_the_projects_own() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _realtime) = on_the_project_page();
    command(&mut app, "swing 66");
    let custom = GrooveId::custom(0).unwrap();
    project_cursor_on(&mut app, ProjectControl::Push(1));
    tap(&mut app, "=");
    assert_eq!(app.song().groove, custom, "the project plays the copy");
    assert_eq!(app.song().grooves[0].pushes(), &[0, 78]);
    assert_eq!(app.groove_shown, Some(custom));
    assert!(app.status.contains("copied to Custom 1"), "{}", app.status);
    tap(&mut app, "u");
    assert_eq!(
        app.song().groove,
        GrooveId::swing(66).unwrap(),
        "one undo step"
    );
    assert!(app.song().grooves.is_empty());
    // Its own changes in place.
    command(&mut app, "groove custom-1");
    assert_eq!(app.project_control(), ProjectControl::Push(1));
    tap(&mut app, "i-12");
    special(&mut app, KeyCode::Enter);
    let own = |app: &App| app.song().groove(custom);
    assert_eq!(own(&app).push(1), -12);
    tap(&mut app, "+");
    assert_eq!(own(&app).push(1), -2);
    tap(&mut app, "x");
    assert_eq!(own(&app).push(1), 0, "back on the grid");
    tap(&mut app, "i200");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).push(1), 0, "past half a step is refused");
    assert!(app.status.contains("-120 to 120"), "{}", app.status);
    // The first step stays on the grid.
    tap(&mut app, "0=");
    assert_eq!(own(&app).push(0), 0);
    assert!(app.status.contains("first step"), "{}", app.status);

    project_cursor_on(&mut app, ProjectControl::GrooveSteps);
    tap(&mut app, "i16");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).steps(), 16);
    tap(&mut app, "-");
    assert_eq!(own(&app).steps(), 15);
    project_cursor_on(&mut app, ProjectControl::Humanise);
    tap(&mut app, "+");
    assert_eq!(own(&app).humanise(), 4);
    project_cursor_on(&mut app, ProjectControl::Velocity(3));
    tap(&mut app, "_");
    assert_eq!(own(&app).velocity(3), 90);
    tap(&mut app, "i150");
    special(&mut app, KeyCode::Enter);
    assert_eq!(own(&app).velocity(3), 150);

    // Edit chooses which is drawn, and x goes back to the project's.
    project_cursor_on(&mut app, ProjectControl::GrooveShown);
    tap(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    tap(&mut app, "/pull");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.shown_groove().0, GrooveId::named("Pull").unwrap());
    assert_eq!(app.song().groove, custom, "only what's drawn changes");
    tap(&mut app, "x");
    assert_eq!(app.shown_groove().0, custom);
}

#[test]
fn the_project_page_has_the_songs_settings_then_this_machines_audio_on_h_and_l() {
    let (mut app, _rt) = app();
    let title = |app: &mut App| {
        screen_rows(app)
            .into_iter()
            .find(|row| row.starts_with("Project"))
            // The CPU's figure is at the far right of the same row.
            .and_then(|row| row.split("    ").next().map(str::to_owned))
    };
    press(&mut app, " 9");
    assert_eq!(app.view, View::Project, "Song first");
    assert_eq!(
        title(&mut app).as_deref(),
        Some("Project  Song  Audio · this machine")
    );
    press(&mut app, "jj");
    press(&mut app, "L");
    assert_eq!(app.view, View::Audio);
    assert_eq!(app.mode, Mode::Normal);
    press(&mut app, "jL");
    assert_eq!(app.view, View::Audio, "L stops at the last");
    assert_eq!(app.audio_row, 1);
    press(&mut app, "H");
    assert_eq!(app.view, View::Project);
    assert_eq!(app.project_row, 2, "each keeps its cursor");
    press(&mut app, "HL");
    assert_eq!((app.view, app.audio_row), (View::Audio, 1));

    // Space 9 opens whichever showed last, as a jump C-o comes back from.
    press(&mut app, " 3 9");
    assert_eq!(app.view, View::Audio);
    press(&mut app, "H 3 9");
    assert_eq!(app.view, View::Project);
    app.key(parse_sequence("C-o").unwrap()[0], Instant::now());
    assert_eq!(app.view, View::Phrase);
}

/// A saved project made read-only, as the tutorial's songs are, opened in a fresh app.
fn a_read_only_project() -> (App, Realtime, PathBuf) {
    let path = folder();
    let song = Song {
        read_only: true,
        ..Song::new(2)
    };
    vt_model::project::save_song(&path, &song).unwrap();
    let (mut app, realtime) = app();
    command(&mut app, &format!("e {}", path.display()));
    assert!(app.read_only, "{}", app.status);
    (app, realtime, path)
}

#[test]
fn w_refuses_a_read_only_song_and_w_bang_writes_over_it_keeping_it_read_only() {
    use vt_model::project::load_song;
    use vt_tui::project_page::ProjectControl;
    let (mut app, _rt, path) = a_read_only_project();
    let before = std::fs::read_to_string(path.join("song.ron")).unwrap();
    assert!(before.contains("read_only: true"), "{before}");
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert_eq!(
        app.status,
        "This song is read-only: :w path saves a copy, :w! writes over it"
    );
    assert!(app.dirty);
    assert_eq!(
        std::fs::read_to_string(path.join("song.ron")).unwrap(),
        before,
        "the file is untouched"
    );
    // Naming its own folder is the same.
    command(&mut app, &format!("w {}", path.display()));
    assert!(app.status.contains("read-only"), "{}", app.status);
    assert!(app.dirty);
    // The project page's Save is :w, and refuses too.
    tap(&mut app, " 9");
    project_cursor_on(&mut app, ProjectControl::Save);
    special(&mut app, KeyCode::Enter);
    assert!(app.status.contains("read-only"), "{}", app.status);
    assert!(app.dirty);

    command(&mut app, "w!");
    assert!(!app.dirty, "{}", app.status);
    assert!(app.status.contains("stays read-only"), "{}", app.status);
    let saved = load_song(&path).unwrap();
    assert_eq!(saved.tracks[0].phrases[&0].steps()[0].note, Note::On(60));
    assert!(saved.read_only, ":w! keeps it parked");
    assert!(app.read_only);
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn w_to_a_new_folder_saves_a_copy_that_isnt_read_only_to_carry_on_in() {
    use vt_model::project::load_song;
    let (mut app, _rt, path) = a_read_only_project();
    let before = std::fs::read_to_string(path.join("song.ron")).unwrap();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let copy = folder();
    command(&mut app, &format!("w {}", copy.display()));
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(app.folder.as_deref(), Some(copy.as_path()));
    assert!(!app.read_only, "the copy is the user's own");
    assert!(!app.window_title().contains("[RO]"));
    assert!(!load_song(&copy).unwrap().read_only);
    assert!(
        !std::fs::read_to_string(copy.join("song.ron"))
            .unwrap()
            .contains("read_only"),
        "left out when off"
    );
    // And :w carries on there.
    press(&mut app, "jis");
    special(&mut app, KeyCode::Esc);
    command(&mut app, "w");
    assert!(!app.dirty, "{}", app.status);
    assert_eq!(
        load_song(&copy).unwrap().tracks[0].phrases[&0].steps()[1].note,
        Note::On(62)
    );
    assert_eq!(
        std::fs::read_to_string(path.join("song.ron")).unwrap(),
        before,
        "the read-only one is untouched"
    );
    std::fs::remove_dir_all(&path).unwrap();
    std::fs::remove_dir_all(&copy).unwrap();
}

#[test]
fn set_readonly_and_noreadonly_switch_it_and_the_status_line_and_title_say_ro() {
    use vt_model::project::load_song;
    let (mut app, _rt) = app();
    let path = folder();
    command(&mut app, &format!("w {}", path.display()));
    assert!(!status_line(&app).contains("[RO]"));
    for (line, on) in [
        ("set readonly", true),
        ("set noreadonly", false),
        ("set ro", true),
    ] {
        command(&mut app, line);
        assert_eq!(app.read_only, on, "{line}");
        assert!(app.dirty, "{line} marks the song changed");
    }
    assert!(
        status_line(&app).contains(" * [RO]"),
        "{}",
        status_line(&app)
    );
    assert!(
        app.window_title().ends_with(" [RO]"),
        "{}",
        app.window_title()
    );
    // It isn't an edit to undo.
    tap(&mut app, "u");
    assert!(app.read_only);
    command(&mut app, "w");
    assert!(app.dirty, "now it refuses");
    command(&mut app, "set noro");
    command(&mut app, "w");
    assert!(!app.dirty, "{}", app.status);
    assert!(!load_song(&path).unwrap().read_only);
    command(&mut app, "set ro");
    command(&mut app, "w!");
    assert!(load_song(&path).unwrap().read_only, "saved with the song");
    // Switching it to what it is already changes nothing.
    command(&mut app, "set ro");
    assert!(!app.dirty);
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn a_read_only_song_autosaves_and_restoring_never_writes_over_it() {
    use std::time::Duration;
    use vt_model::project::load_song;
    let (mut app, _rt, path) = a_read_only_project();
    let before = std::fs::read_to_string(path.join("song.ron")).unwrap();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    let start = Instant::now();
    app.autosave_if_due(start);
    app.autosave_if_due(start + Duration::from_secs(61));
    assert!(
        path.join("song.autosave.ron").exists(),
        "work isn't lost: {}",
        app.status
    );
    assert_eq!(
        std::fs::read_to_string(path.join("song.ron")).unwrap(),
        before
    );

    // As after a crash: opened again, the work is offered back, unsaved.
    let (mut other, _rt2) = self::app();
    command(&mut other, &format!("e {}", path.display()));
    assert_eq!(other.mode, Mode::Restore);
    press(&mut other, "y");
    assert!(other.dirty);
    assert!(other.read_only);
    assert!(other.status.contains("read-only"), "{}", other.status);
    assert_eq!(
        other.song().tracks[0].phrases[&0].steps()[0].note,
        Note::On(60)
    );
    assert_eq!(
        std::fs::read_to_string(path.join("song.ron")).unwrap(),
        before,
        "restoring leaves the file alone"
    );
    command(&mut other, "w");
    assert_eq!(
        std::fs::read_to_string(path.join("song.ron")).unwrap(),
        before,
        "and so does :w"
    );
    assert!(load_song(&path).unwrap().read_only);
    std::fs::remove_dir_all(&path).unwrap();
}

#[test]
fn the_project_page_shows_and_switches_read_only() {
    use vt_tui::project_page::ProjectControl;
    let (mut app, _rt) = on_the_project_page();
    project_cursor_on(&mut app, ProjectControl::ReadOnly);
    let row = |app: &App| {
        project_page_lines(app)
            .into_iter()
            .find(|line| line.starts_with("Read-only"))
            .map(|line| line.trim_end().to_owned())
    };
    assert_eq!(row(&app).as_deref(), Some("Read-only off"));
    tap(&mut app, "=");
    assert!(app.read_only);
    assert!(app.dirty);
    assert_eq!(
        row(&app).as_deref(),
        Some("Read-only on  :w refuses, so Save as makes a copy to change")
    );
    tap(&mut app, "-");
    assert!(!app.read_only);
    tap(&mut app, "K");
    assert!(app.read_only, "K switches it");
    tap(&mut app, "i");
    assert!(!app.read_only, "and i, as it isn't typed");
    special(&mut app, KeyCode::Enter);
    assert!(app.read_only, "and Enter");
    tap(&mut app, "x");
    assert!(
        !app.read_only,
        "x puts it back off, as a new project has it"
    );
}
