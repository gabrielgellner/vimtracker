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
    // Before any is saved, the browser lists the shipped ones, under `Factory`.
    command(&mut app, "patch");
    assert_eq!(app.mode, Mode::Picker, "{}", app.status);
    let rows = explorer_rows(&app);
    assert_eq!(rows[..2], ["v built in", "    Turing Machine"], "{rows:?}");
    special(&mut app, KeyCode::Esc);
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
    let rows = explorer_rows(&app);
    assert_eq!(
        rows[..4],
        ["v recent", "    Turing", "v built in", "    Turing Machine"],
        "the folder's, then the shipped ones"
    );
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

/// A ramp from `A0` to `A0` over every row, as an FX generator's shape: a depth of `+40` when it
/// locks a slot's.
fn level() -> vt_model::shape::Shape {
    use vt_model::shape::{
        Acid, Bits, DEFAULT_RATE, FillKind, Follow, Grids, Pink, Random, Shape, Values, Variation,
        Walk,
    };
    Shape {
        from: 0xA0,
        to: 0xA0,
        curve: 0,
        s: 0,
        kind: FillKind::Ramp,
        wave: vt_model::lfo::Shape::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::AllRows,
        seed: 1,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: Grids::default(),
        random: Random::default(),
        vary: 0,
        variation: Variation::default(),
    }
}

/// An envelope held full while a note plays.
fn held() -> vt_model::envelope::Adsr {
    vt_model::envelope::Adsr {
        attack_ms: 0,
        decay_ms: 0,
        sustain: 100,
        ..vt_model::envelope::Adsr::decay()
    }
}

/// Two tracks playing a flat sample at `CUT 80`, eight notes each. The first's slot 4 is held
/// full on `Cut` with no depth of its own, and its FX1 is kept live writing `DEP4` at `+40` on
/// every row, a generator macro 1 holds, so its patch is that slot, the macro and the generator.
fn lock_on_slot_4() -> Song {
    use vt_model::generator::{Generator, Span};
    use vt_model::modulation::ModSlot;
    use vt_model::shape::ShapeColumn;
    use vt_model::step::FxCommand;
    let mut song = Song::new(2);
    for track in 0..2 {
        song.sample_on(track, "s");
        let owner = &mut song.tracks[track];
        owner.cutoff = 0x80;
        let steps: Vec<Step> = (0..8)
            .map(|_| "C-3 -- --- --- ---".parse().unwrap())
            .collect();
        owner.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    let first = &mut song.tracks[0];
    first.mods[3] = ModSlot::adsr(Some(SettingId::CUT), held());
    first.macros[0].maps = vec![MacroMap::numbers(
        MacroTarget::Gen {
            column: GenColumn::Fx(0),
            param: GenParam::To,
        },
        (0xA0, 0xA0),
    )];
    let column = ShapeColumn::Fx {
        slot: 0,
        command: FxCommand::Set(SettingId::modulator(3, 0).unwrap()),
    };
    let generator = Generator::new(column, Span::Rows { first: 0, last: 7 }, level());
    let phrase = Arc::make_mut(first.phrases.get_mut(&0).unwrap());
    assert!(phrase.add_generator(generator, None));
    let written = phrase.steps()[0].to_string();
    assert!(written.contains("mod4.depth:A0"), "{written}");
    song
}

/// The cutoff a track plays with half way through its second note, rendered from the top on a
/// flat sample.
fn cutoff_heard(song: &Song, track: usize) -> u8 {
    let mut pool = vt_model::samples::SamplePool::default();
    pool.insert(
        "s",
        vt_model::sample::SampleData {
            frames: vec![[1.0, 1.0]; 48_000 * 4],
        },
    );
    let version = Arc::new(vt_engine::published::Published::new(
        Arc::new(song.clone()),
        Arc::new(pool),
    ));
    let mut engine = vt_engine::engine::Engine::new(48_000, version);
    engine.play_from(0);
    // A 16th at 120 BPM is 6000 frames.
    let mut out = vec![0.0; 9000 * 2];
    engine.process(&mut out);
    engine.live(track).cutoff
}

/// The FX1 cells of a track's phrase `00`.
fn fx1(app: &App, track: usize) -> Vec<String> {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.fx[0].to_string())
        .collect()
}

#[test]
fn a_lock_a_patchs_generator_writes_plays_on_the_slot_its_slot_went_to() {
    let (mut app, _realtime, _presets) = app_on(lock_on_slot_4());
    assert_eq!(cutoff_heard(app.song(), 0), 0xC0, "slot 4 opens it by +40");
    command(&mut app, "patch save Lock");
    assert!(
        app.status.contains("FX1 generator, 1 slot, 1 macro"),
        "{}",
        app.status
    );

    // The second track's first two slots shape its note and cutoff, so slot 4 lands in `Mod 3`
    // and its lock with it: left as `DEP4`, it would lock a slot going nowhere.
    app.cursor.track = 1;
    assert_eq!(cutoff_heard(app.song(), 1), 0x80);
    command(&mut app, "patch load Lock");
    assert!(
        app.status
            .starts_with("Track 02 has Lock in mod slot 3, macro 1 and live FX1 on phrase 00"),
        "{}",
        app.status
    );
    let track = &app.song().tracks[1];
    assert_eq!(track.mods[2].destination, Some(SettingId::CUT));
    assert_eq!(track.mods[3].destination, None);
    let cells = fx1(&app, 1);
    assert!(
        cells.iter().all(|cell| cell.starts_with("mod3.depth:A0")),
        "{cells:?}"
    );
    assert_eq!(cutoff_heard(app.song(), 1), 0xC0, "the lock plays on Mod 3");
}

#[test]
fn bang_puts_a_patch_over_the_last_slot_and_its_lock_follows_leaving_the_tracks_own() {
    // Every slot of the second track goes somewhere, so none is free, and a step of its own
    // locks its slot 5's depth on FX2.
    let mut song = lock_on_slot_4();
    let second = &mut song.tracks[1];
    for held in &mut second.mods[2..] {
        held.destination = Some(SettingId::PAN);
    }
    let phrase = Arc::make_mut(second.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-3 -- --- mod5.depth:90 ---".parse().unwrap();
    let (mut app, _realtime, _presets) = app_on(song);
    command(&mut app, "patch save Lock");
    app.cursor.track = 1;
    let own = app.song().tracks[1].phrases[&0].steps()[0].fx[1].to_string();
    assert!(own.starts_with("mod5.depth:90"), "{own}");

    command(&mut app, "patch load Lock");
    assert!(
        app.status.contains(":patch! puts it over"),
        "{}",
        app.status
    );
    command(&mut app, "patch! load Lock");
    assert!(
        app.status.starts_with(
            "Track 02 has Lock in mod slot 6, macro 1 and live FX1 on phrase 00, over mod slot 6"
        ),
        "{}",
        app.status
    );
    let track = &app.song().tracks[1];
    assert_eq!(track.mods[5].destination, Some(SettingId::CUT));
    let cells = fx1(&app, 1);
    assert!(
        cells.iter().all(|cell| cell.starts_with("mod6.depth:A0")),
        "{cells:?}"
    );
    let own = track.phrases[&0].steps()[0].fx[1].to_string();
    assert!(
        own.starts_with("mod5.depth:90"),
        "the track's own lock stays: {own}"
    );
    assert_eq!(cutoff_heard(app.song(), 1), 0xC0, "the lock plays on Mod 6");
}

/// A track that plays a `VA`, with a line of eight notes on its phrase `00` of 16 rows, every
/// other row, as a fresh track with a few notes typed.
fn fresh_track() -> Song {
    use vt_model::song::{Machine, SynthVoice};
    let mut song = Song::new(1);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let line = ["C-3", "D#3", "C-3", "G-3", "C-3", "D#3", "A#3", "G-3"];
    let steps: Vec<Step> = (0..16)
        .map(|row| {
            let text = if row % 2 == 0 {
                format!("{} 64 --- --- ---", line[row / 2])
            } else {
                "--- -- --- --- ---".to_owned()
            };
            text.parse().unwrap()
        })
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// Two bars of a song from the top, stereo.
fn two_bars(song: &Song) -> Vec<f32> {
    let version = Arc::new(vt_engine::published::Published::new(
        Arc::new(song.clone()),
        Arc::new(vt_model::samples::SamplePool::default()),
    ));
    let mut engine = vt_engine::engine::Engine::new(48_000, version);
    engine.play_from(0);
    // 32 sixteenths at 120 BPM.
    let mut out = vec![0.0; 32 * 6000 * 2];
    engine.process(&mut out);
    out
}

#[test]
fn every_shipped_patch_goes_onto_a_fresh_track_and_plays() {
    let plain = two_bars(&fresh_track());
    for (name, _) in vt_model::preset::FACTORY_PATCHES {
        let (mut app, _realtime, presets) = app_on(fresh_track());
        command(&mut app, &format!("patch load {name}"));
        assert!(
            app.status.starts_with(&format!("Track 01 has {name} in")),
            "{name}: {}",
            app.status
        );
        assert!(
            !presets.exists(),
            "nothing is written in the presets folder"
        );
        let named = app.song().tracks[0]
            .macros
            .iter()
            .filter(|held| held.name.is_some())
            .count();
        assert!(named > 0, "{name} has named macros");
        let out = two_bars(app.song());
        let peak = out.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        let rms = (out.iter().map(|s| s * s).sum::<f32>() / out.len() as f32).sqrt();
        assert!(rms > 0.01, "{name} plays: RMS {rms}");
        assert!(peak < 1.0, "{name} stays below full scale: peak {peak}");
        assert!(out != plain, "{name} changes what the track plays");
        press(&mut app, "u");
        assert_eq!(
            app.song().tracks[0],
            fresh_track().tracks[0],
            "{name} undoes"
        );
    }
}

#[test]
fn the_browser_adds_a_shipped_patch_and_kria_brings_its_lane() {
    let (mut app, _realtime, _presets) = app_on(fresh_track());
    command(&mut app, "patch");
    press(&mut app, "/kria");
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(
        picker.current().unwrap().meaning,
        "FX1 generator, 2 slots, 2 macros"
    );
    special(&mut app, KeyCode::Enter);
    assert_eq!(
        app.status,
        "Track 01 has Kria in mod slots 3 and 4, macros 1 and 2 and live FX1 on phrase 00, keeping its sound: u undoes it"
    );
    let track = &app.song().tracks[0];
    assert_eq!(
        track.phrases[&0].lane(0),
        Some(vt_model::song::Lane {
            start: 0,
            length: 4
        }),
        "the length loop has its lane of 4"
    );
    let written: Vec<String> = ["gate:20", "gate:C0", "gate:08", "gate:E6"]
        .iter()
        .map(|cell| {
            let step: Step = format!("--- -- {cell} --- ---").parse().unwrap();
            step.fx[0].to_string()
        })
        .collect();
    assert_eq!(fx1(&app, 0)[..4], written);
    let names: Vec<_> = track.macros[..2].iter().map(|m| m.name.clone()).collect();
    assert_eq!(
        names,
        [Some("Pitch loop".into()), Some("Octave loop".into())]
    );
    assert_eq!(key_of(&track.macros[0].maps[0]), "mod3.length");
    assert_eq!(track.mods[3].steps.length, 3, "the octave loop's 3 steps");
}

#[test]
fn the_shipped_patches_are_found_by_their_tags_and_cant_be_tagged() {
    let (mut app, _realtime, _presets) = app_on(fresh_track());
    command(&mut app, "patch");
    press(&mut app, "/#rhythm");
    let rows = explorer_rows(&app);
    assert_eq!(rows, ["v built in", "    Euclid"], "{rows:?}");
    special(&mut app, KeyCode::Esc);
    special(&mut app, KeyCode::Esc);
    command(&mut app, "patch");
    press(&mut app, "/#generative");
    let rows = explorer_rows(&app);
    assert_eq!(rows.len(), 10, "all nine: {rows:?}");
    special(&mut app, KeyCode::Esc);
    let tags: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .current()
        .unwrap()
        .tags
        .iter()
        .map(|tag| tag.name.clone())
        .collect();
    assert_eq!(tags, ["patch", "generative", "pitch"]);
    press(&mut app, "T");
    assert_eq!(app.mode, Mode::Picker);
    assert!(
        app.picker.as_ref().unwrap().taggable().is_none(),
        "a shipped patch is read-only"
    );
}

#[test]
fn a_patch_of_the_users_own_comes_before_a_shipped_one_of_the_same_name() {
    let (mut app, _realtime, presets) = turing_machine();
    command(&mut app, "patch save Euclid");
    assert!(presets.join("patches/Euclid.ron").is_file());
    app.cursor.track = 1;
    command(&mut app, "patch load Euclid");
    assert!(
        app.status
            .starts_with("Track 02 has Euclid in mod slot 3 and macros 1, 2 and 3"),
        "the user's own, a Turing Machine: {}",
        app.status
    );
    command(&mut app, "patch load Nothing");
    assert!(
        app.status.starts_with("There's no patch called Nothing in")
            && app.status.ends_with("nor one that comes with vimtracker"),
        "{}",
        app.status
    );
}
