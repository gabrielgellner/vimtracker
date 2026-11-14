//! Patches (decision 311): the tutorial's hand-built Turing Machine saved with `:patch save` and
//! added to another track, into its free mod slots and macros, keeping its sound, as one undo
//! step; refused when too few are free, and put over the last ones with `!`; the browser and
//! `K`'s ways in; and a live generator a macro holds carried onto the phrase under the cursor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ratatui::crossterm::event::KeyCode;
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::modulation::ModKind;
use vt_model::settings::SettingId;
use vt_model::shape::{GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::config::Settings;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

/// An app on this song, whose presets folder is a scratch folder of its own.
fn app_on(song: Song) -> (App, vt_engine::link::Realtime, PathBuf) {
    let samples = Arc::new(vt_model::samples::SamplePool::default());
    let version = Arc::new(vt_engine::published::Published::new(
        Arc::new(song.clone()),
        Arc::clone(&samples),
    ));
    let (handle, realtime) =
        vt_engine::link::connect(vt_engine::engine::Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let presets = folder().join("presets");
    app.settings = Settings {
        presets: presets.display().to_string(),
        ..Settings::none()
    };
    (app, realtime, presets)
}

/// The generators tutorial's `turing-machine` song: track 1 an `Acid` with a Steps slot fourth
/// on `Pitch` in scale and `Change`, `Length` and `Range` on its `Mutate`, `Length` and `Depth`.
fn turing_machine() -> (App, vt_engine::link::Realtime, PathBuf) {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/manual/src/tutorials/songs/turing-machine");
    app_on(vt_model::project::load_song(&folder).unwrap())
}

fn key_of(map: &MacroMap) -> String {
    map.target.setting().unwrap().key()
}

#[test]
fn the_turing_machine_saved_as_a_patch_goes_onto_another_track_keeping_its_sound() {
    let (mut app, _realtime, presets) = turing_machine();
    command(&mut app, "patch save Turing");
    let file = presets.join("patches/Turing.ron");
    assert!(file.is_file(), "{}", app.status);
    assert!(
        app.status
            .starts_with("Saved track 01's patch as Turing, 1 slot, 3 macros"),
        "{}",
        app.status
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("kind: Steps"), "{text}");
    assert!(text.contains("\"mod1.mutate\""), "{text}");
    assert!(!text.contains("acid"), "the sound isn't in it: {text}");
    assert!(
        !text.contains("\"vol\""),
        "nor the slots its macros don't reach"
    );

    // A name that's taken is refused, and `!` writes over it.
    command(&mut app, "patch save Turing");
    assert!(app.status.contains(":patch! save Turing"), "{}", app.status);
    command(&mut app, "patch! save Turing");
    assert!(app.status.starts_with("Saved"), "{}", app.status);

    // On the kick, whose first two slots shape its note and cutoff.
    app.cursor.track = 1;
    let before = app.song().tracks[1].clone();
    let depth = app.history.undo_depth();
    command(&mut app, "patch load Turing");
    let after = &app.song().tracks[1];
    assert_eq!(
        app.status,
        "Track 02 has Turing in mod slot 3 and macros 1, 2 and 3, keeping its sound: u undoes it"
    );
    assert_eq!(after.mods[2].kind, ModKind::Steps);
    assert_eq!(after.mods[2].destination, Some(SettingId::PITCH));
    assert!(after.mods[2].in_scale);
    assert_eq!(after.mods[2].steps, app.song().tracks[0].mods[3].steps);
    let keys: Vec<String> = after.macros[..3]
        .iter()
        .map(|m| key_of(&m.maps[0]))
        .collect();
    assert_eq!(keys, ["mod3.mutate", "mod3.length", "mod3.depth"]);
    let names: Vec<_> = after.macros[..3].iter().map(|m| m.name.clone()).collect();
    assert_eq!(
        names,
        [
            Some("Change".into()),
            Some("Length".into()),
            Some("Range".into())
        ]
    );
    assert_eq!(after.machine, before.machine, "the sound is kept");
    assert_eq!(after.settings, before.settings);
    assert_eq!(after.mods[..2], before.mods[..2]);
    assert_eq!(after.phrases, before.phrases);
    assert_eq!(app.history.undo_depth(), depth + 1, "one undo step");
    press(&mut app, "u");
    assert_eq!(app.song().tracks[1], before);
}

#[test]
fn too_few_free_is_refused_saying_how_many_and_bang_puts_it_over_the_last() {
    let (mut app, _realtime, _presets) = turing_machine();
    command(&mut app, "patch save Turing");
    // The kick's macros 1 to 6 hold settings of its own, so only two are free.
    let mut song = Song::clone(app.song());
    let own = [
        SettingId::CUT,
        SettingId::PAN,
        SettingId::SEND_A,
        SettingId::SEND_B,
        SettingId::LEVEL,
        SettingId::GROOVE,
    ];
    for (at, id) in own.into_iter().enumerate() {
        song.tracks[1].macros[at].maps = vec![MacroMap::whole(MacroTarget::Setting(id))];
    }
    let track = song.tracks[1].clone();
    app.apply(&vt_model::action::Action::LoadSound {
        track: 1,
        preset: Box::new(vt_model::preset::SoundPreset::of(&track, None)),
        sample: 0,
    });
    app.cursor.track = 1;
    let before = app.song().tracks[1].clone();
    command(&mut app, "patch load Turing");
    assert_eq!(
        app.status,
        "Turing needs 1 free mod slot and 3 free macros; track 02 has 4 free slots and 2 free macros: :patch! puts it over the last ones"
    );
    assert_eq!(app.song().tracks[1], before, "nothing is added");

    command(&mut app, "patch! load Turing");
    assert_eq!(
        app.status,
        "Track 02 has Turing in mod slot 3 and macros 6, 7 and 8, over macro 6, keeping its sound: u undoes it"
    );
    let after = &app.song().tracks[1];
    assert_eq!(after.macros[5].name.as_deref(), Some("Change"));
    assert_eq!(key_of(&after.macros[7].maps[0]), "mod3.depth");
    assert_eq!(key_of(&after.macros[4].maps[0]), "level", "the others stay");
}

#[test]
fn the_browser_and_k_add_a_patch() {
    let (mut app, _realtime, presets) = turing_machine();
    command(&mut app, "patch");
    assert!(
        app.status
            .starts_with("No patches yet: :patch save name saves track 01's macros"),
        "{}",
        app.status
    );
    command(&mut app, "patch save Turing");
    assert!(presets.join("patches").is_dir());

    // `K` on the Mod page's Type lists adding one, which opens the browser on the patches.
    app.cursor.track = 2;
    press(&mut app, " 6");
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    let labels: Vec<String> = picker.items().iter().map(listed).collect();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Add a patch  :patch")),
        "{labels:?}"
    );
    press(&mut app, "/add a p");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Picker, "{}", app.status);
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Patches");
    assert_eq!(explorer_rows(&app), ["v recent", "    Turing"]);
    assert_eq!(picker.current().unwrap().meaning, "1 slot, 3 macros");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.song().tracks[2].mods[2].kind, ModKind::Steps);
    assert!(
        app.status.starts_with("Track 03 has Turing"),
        "{}",
        app.status
    );

    // Save is there too, typing the command for the name.
    press(&mut app, "K");
    press(&mut app, "/save as a p");
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.mode, Mode::Command);
    assert_eq!(app.command_line, "patch save ");

    // And on a macro, the patch's knob, on the Gen page.
    special(&mut app, KeyCode::Esc);
    press(&mut app, " 8gg0");
    press(&mut app, "K");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.title, "Patches");
    let labels: Vec<String> = picker.items().iter().map(listed).collect();
    assert_eq!(labels.len(), 2, "{labels:?}");
}

/// Eight empty rows on each of two tracks, a tresillo of notes kept live on the first's, its
/// `Hits` on macro 1.
fn euclid_on_first() -> (App, vt_engine::link::Realtime, PathBuf) {
    let mut song = Song::new(2);
    for track in &mut song.tracks[..2] {
        let steps: Vec<Step> = (0..8)
            .map(|_| "--- -- --- --- ---".parse().unwrap())
            .collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    let (mut app, realtime, presets) = app_on(song);
    press(&mut app, "vGSjjjjjj===");
    special(&mut app, KeyCode::Char('L'));
    press(&mut app, " 8}kjj$");
    press(&mut app, "M1");
    (app, realtime, presets)
}

fn notes(app: &App, track: usize) -> usize {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .filter(|step| matches!(step.note, Note::On(_)))
        .count()
}

#[test]
fn a_live_generator_a_macro_holds_goes_onto_the_phrase_under_the_cursor() {
    let (mut app, _realtime, _presets) = euclid_on_first();
    let hits = MacroTarget::Gen {
        column: GenColumn::Note,
        param: GenParam::Hits,
    };
    assert!(app.song().tracks[0].macro_holding(hits).is_some());
    let notes_first = notes(&app, 0);
    assert_eq!(notes_first, 3);
    command(&mut app, "patch save Euclid");
    assert!(
        app.status.contains("Note generator, 1 macro"),
        "{}",
        app.status
    );

    app.cursor.track = 1;
    command(&mut app, "patch load Euclid");
    assert!(
        app.status
            .starts_with("Track 02 has Euclid in macro 1 and live Note on phrase 00"),
        "{}",
        app.status
    );
    let track = &app.song().tracks[1];
    let generator = &track.phrases[&0].generators()[0];
    assert!(generator.column.writes_notes());
    assert!(matches!(
        generator.shape.place,
        Where::Euclid { hits: 3, .. }
    ));
    assert_eq!(track.macro_holding(hits).map(|(at, _)| at), Some(0));
    assert_eq!(notes(&app, 1), notes_first, "the same tresillo");
}
