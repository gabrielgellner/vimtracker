//! The kit dialog, `:kit`: marking a kit's sounds and loading them as new tracks after the last,
//! in a new group named after the kit and folded, with a beat from Grids offered after.

use std::path::{Path, PathBuf};

use ratatui::crossterm::event::KeyCode;
use vt_model::constants::MAX_GROUPS;
use vt_model::song::TrackKind;
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::banking::group_name;
use vt_tui::config::Settings;
use vt_tui::input::mode::Mode;
use vt_tui::shaping::KIT_BEAT_PICKER;

mod common;
use common::*;

/// A kit folder holding `909.ron`, of a kick, a snare, two hats sharing a choke group and a pad,
/// with the app's settings pointing at it. Returns the kit.
fn kit_in_folder(app: &mut App) -> PathBuf {
    let kit = kit_file();
    std::fs::write(
        &kit,
        r#"(sounds: [
            (label: "BD", name: "Kick", path: "drums/kick.wav"),
            (label: "SD", name: "Snare", path: "drums/snare.wav", gain: -3.0),
            (label: "CH", name: "Closed", path: "drums/hat.wav", choke: Some(1)),
            (label: "OH", name: "Open", path: "drums/snare.wav", choke: Some(1), tune: 5),
            (label: "PD", name: "Pad", path: "drums/kick.wav", tune: -12),
        ])"#,
    )
    .unwrap();
    app.settings = Settings {
        kits: vec![kit.parent().unwrap().display().to_string()],
        samples: Vec::new(),
    };
    kit
}

/// Open the dialog and go into the kit, on its first sound.
fn open_kit(app: &mut App) {
    command(app, "kit");
    assert_eq!(app.mode, Mode::Picker);
    assert_eq!(
        explorer_rows(app)[..3],
        ["v kits", "  > drums", "  > 909"],
        "a kit opens, as a folder does"
    );
    press(app, "jl");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "BD");
}

/// The group the new tracks went in, by its number, and its name.
fn group_of(app: &App, track: usize) -> Option<(u8, String)> {
    let song = app.song();
    let number = song.tracks[track].group?;
    let at = song.fixed_track(TrackKind::Group(number))?;
    Some((number, song.tracks[at].name.clone()))
}

fn names(app: &App) -> Vec<String> {
    let song = app.song();
    (0..song.sound_tracks())
        .map(|track| song.tracks[track].name.clone())
        .collect()
}

fn has_notes(app: &App, track: usize) -> bool {
    app.song().tracks[track].phrases.values().any(|phrase| {
        phrase
            .steps()
            .iter()
            .any(|step| matches!(step.note, Note::On(_)))
    })
}

fn ends_with(app: &App, track: usize, file: &str) -> bool {
    sample_of(app, track).is_some_and(|path| Path::new(&path).ends_with(file))
}

#[test]
fn marked_sounds_load_as_new_tracks_after_the_last_in_a_folded_group_as_one_undo() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    open_kit(&mut app);
    let hint = app.picker.as_ref().unwrap().shown_hint();
    assert_eq!(hint, Some("Space mark  a all  Enter load"));

    // x marks the kick and goes on to the snare, j passes it, Space marks the closed hat.
    press(&mut app, "xj ");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.marked_count(), 2);
    assert_eq!(picker.current().unwrap().label, "OH");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);

    // Two tracks were added after the two there, each with one sound named by its label.
    assert_eq!(app.song().sound_tracks(), 4);
    assert_eq!(names(&app), ["", "", "BD", "CH"]);
    assert_eq!(sample_of(&app, 0), None, "nothing written over");
    assert!(ends_with(&app, 2, "kick.wav"));
    assert!(ends_with(&app, 3, "hat.wav"));
    assert_eq!(app.song().tracks[3].choke, Some(1));
    assert_eq!(app.song().tracks[2].choke, None);
    assert_eq!(app.song().bank.get(1).unwrap().title(), "BD Kick");

    // In the first group, named after the kit, folded.
    assert_eq!(group_of(&app, 2), Some((1, "909".to_owned())));
    assert_eq!(group_of(&app, 3), Some((1, "909".to_owned())));
    assert!(app.folds.is_closed(app.song(), 1));
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    assert_eq!(app.status, "Kit 909: tracks 03-04 in G1, folded");

    // The beat is offered, and Esc leaves the tracks empty.
    assert_eq!(app.picker.as_ref().unwrap().title, KIT_BEAT_PICKER);
    special(&mut app, KeyCode::Esc);
    assert_eq!(app.mode, Mode::Normal);
    assert!(!has_notes(&app, 2) && !has_notes(&app, 3));
    assert_eq!(app.history.undo_depth(), depth + 1);

    // u takes the tracks, the group's name and the bank entries off together.
    press(&mut app, "u");
    assert_eq!(app.song().sound_tracks(), 2);
    assert!(app.song().bank.is_empty());
    let group = app.song().fixed_track(TrackKind::Group(1)).unwrap();
    assert_eq!(app.song().tracks[group].name, "G1");
}

#[test]
fn a_marks_them_all_and_the_beat_plays_on_the_kick_snare_and_first_hat() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    command(&mut app, "kit");
    // a on the kit itself opens it and marks every sound; again takes the marks off.
    press(&mut app, "ja");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 5);
    press(&mut app, "a");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 0);
    press(&mut app, "a");
    let depth = app.history.undo_depth();
    special(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["", "", "BD", "SD", "CH", "OH", "PD"]);
    assert_eq!(
        app.song().tracks[5].choke,
        Some(1),
        "the hats share a group"
    );
    assert!(ends_with(&app, 5, "snare.wav"));
    assert_eq!(
        app.song()
            .bank
            .get(app.song().tracks[5].sample())
            .unwrap()
            .edit
            .play
            .tune,
        5
    );
    for track in 2..7 {
        assert_eq!(group_of(&app, track).map(|(number, _)| number), Some(1));
    }

    // Enter takes the beat: the kick, the snare and the closed hat, each its own part.
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, KIT_BEAT_PICKER);
    assert_eq!(
        picker.current().unwrap().meaning,
        "A beat on BD SD CH from Grids' map"
    );
    special(&mut app, KeyCode::Enter);
    assert!(has_notes(&app, 2) && has_notes(&app, 3) && has_notes(&app, 4));
    assert!(!has_notes(&app, 5) && !has_notes(&app, 6));
    let kick: Vec<bool> = app.song().tracks[2].phrases[&0]
        .steps()
        .iter()
        .map(|step| matches!(step.note, Note::On(_)))
        .collect();
    assert!(kick[0], "the kick starts the bar");
    assert_ne!(
        app.song().tracks[2].phrases[&0].steps(),
        app.song().tracks[4].phrases[&0].steps(),
        "each plays its own part"
    );
    assert_eq!(
        app.history.undo_depth(),
        depth + 2,
        "the beat is a step of its own"
    );
    press(&mut app, "u");
    assert!(!has_notes(&app, 2));
    assert_eq!(app.song().sound_tracks(), 7, "u takes the beat off first");
    press(&mut app, "u");
    assert_eq!(app.song().sound_tracks(), 2);
}

#[test]
fn enter_with_nothing_marked_loads_the_sound_under_the_cursor() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    open_kit(&mut app);
    press(&mut app, "jjjj");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "PD");
    special(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["", "", "PD"]);
    assert_eq!(group_of(&app, 2), Some((1, "909".to_owned())));
    assert_eq!(app.status, "Kit 909: track 03 in G1, folded");
    // A pad isn't a drum, so no beat is offered.
    assert_eq!(app.mode, Mode::Normal);

    // Loading again goes after it, in the next group, and never over a track. The dialog opens
    // on the kit last loaded from, open.
    command(&mut app, "kit");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "909");
    press(&mut app, "l");
    special(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["", "", "PD", "BD"]);
    assert_eq!(group_of(&app, 3), Some((2, "909".to_owned())));
    special(&mut app, KeyCode::Esc);
}

#[test]
fn marks_stay_in_one_kit_and_a_missing_sound_cant_be_marked() {
    let (mut app, _realtime) = app();
    let kit = kit_in_folder(&mut app);
    std::fs::write(
        kit.with_file_name("808.ron"),
        r#"(sounds: [
            (label: "BD", path: "drums/kick.wav"),
            (label: "GH", path: "drums/gone.wav"),
        ])"#,
    )
    .unwrap();
    command(&mut app, "kit");
    assert_eq!(
        explorer_rows(&app)[..4],
        ["v kits", "  > drums", "  > 808", "  > 909"]
    );
    // Two of the 909's marked, then one of the 808's takes their place.
    press(&mut app, "jjl");
    assert_eq!(app.picker.as_ref().unwrap().current().unwrap().label, "BD");
    press(&mut app, "  ");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 2);
    press(&mut app, "kkkkl");
    assert_eq!(explorer_rows(&app)[3..5], ["      BD", "      GH"]);
    press(&mut app, "x");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 1);
    // The missing one isn't marked, by Space or by a.
    press(&mut app, "x");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 1);
    press(&mut app, "ka");
    assert_eq!(
        app.picker.as_ref().unwrap().marked_count(),
        0,
        "all there are marked already, so a takes them off"
    );
    press(&mut app, "a");
    assert_eq!(app.picker.as_ref().unwrap().marked_count(), 1);
    special(&mut app, KeyCode::Enter);
    assert_eq!(names(&app), ["", "", "BD"]);
    assert_eq!(group_of(&app, 2), Some((1, "808".to_owned())));
    special(&mut app, KeyCode::Esc);
}

#[test]
fn with_every_group_taken_the_tracks_go_in_none() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    command(&mut app, &format!("track add {}", MAX_GROUPS - 2));
    for group in 1..=MAX_GROUPS {
        command(&mut app, &format!("group {group} {group}-{group}"));
    }
    open_kit(&mut app);
    special(&mut app, KeyCode::Enter);
    let added = MAX_GROUPS;
    assert_eq!(app.song().tracks[added].name, "BD");
    assert_eq!(app.song().tracks[added].group, None);
    assert_eq!(
        app.status,
        format!("Kit 909: track 05 in no group, as all {MAX_GROUPS} have tracks")
    );
    special(&mut app, KeyCode::Esc);
}

#[test]
fn the_kit_in_the_dialog_still_puts_its_sounds_on_tracks_from_the_first_with_l() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    command(&mut app, "kit");
    press(&mut app, "jL");
    assert_eq!(names(&app), ["BD", "SD", "CH", "OH", "PD"]);
    assert_eq!(app.song().tracks[0].group, None);
    assert_eq!(app.status, "Kit 909: 5 tracks");
}

#[test]
fn a_group_is_named_by_the_first_word_of_its_kit() {
    assert_eq!(group_name("909"), "909");
    assert_eq!(group_name("808 Core Kit"), "808");
    assert_eq!(group_name("Rytm drums"), "RYT");
    assert_eq!(group_name(""), "");
}

#[test]
fn the_dialog_shows_the_marks_and_counts_them() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    open_kit(&mut app);
    press(&mut app, "x");
    let lines = screen_cells(&mut app);
    let text = lines.join("\n");
    assert!(text.contains(" Kits 1 marked "), "{text}");
    assert!(text.contains("Space mark  a all  Enter load"), "{text}");
    let bd = lines.iter().find(|line| line.contains("BD  Kick")).unwrap();
    assert!(bd.contains('\u{f0132}'), "a ticked pad: {bd}");
    let sd = lines
        .iter()
        .find(|line| line.contains("SD  Snare"))
        .unwrap();
    assert!(!sd.contains('\u{f0132}'), "{sd}");
}

#[test]
fn a_group_named_after_its_kit_fits_its_column_folded_or_open() {
    let (mut app, _realtime) = app();
    kit_in_folder(&mut app);
    open_kit(&mut app);
    special(&mut app, KeyCode::Enter);
    special(&mut app, KeyCode::Esc);
    let text = |buffer: &ratatui::buffer::Buffer| -> String {
        let area = buffer.area;
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Folded, the song view cuts it to two for the mark, as the phrase view's labels are; the
    // mixer's strip has room for all three.
    let song = text(&drawn(&app, vt_tui::views::song::draw));
    assert!(song.contains("90\u{f084c}\u{a0}A"), "{song}");
    let mixer = text(&drawn(&app, vt_tui::views::mixer::draw));
    assert!(mixer.contains("909\u{f084c}"), "{mixer}");
    app.fold_group(1, false);
    let song = text(&drawn(&app, vt_tui::views::song::draw));
    assert!(song.contains("BD  909 A"), "{song}");
}
