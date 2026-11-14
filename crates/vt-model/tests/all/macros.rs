use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use vt_model::action::{Action, EditError, edit};
use vt_model::macros::{MACROS, MacroMap, MacroTarget};
use vt_model::modulation::{MOD_SLOTS, ModKind};
use vt_model::project::{load_song, save_song};
use vt_model::settings::{Owner, SettingId};
use vt_model::song::Song;
use vt_model::step::FxCommand;

fn folder() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let path = std::env::temp_dir().join(format!(
        "vt-model-macros-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

fn cut() -> MacroTarget {
    MacroTarget::Setting(SettingId::CUT)
}

fn map(at: usize, target: MacroTarget, min: u16, max: u16) -> Action {
    Action::MapMacro {
        track: 0,
        at,
        map: MacroMap {
            target,
            min,
            max,
            curve: 0,
            s: 0,
        },
    }
}

#[test]
fn a_macro_is_a_setting_called_mac_with_its_number() {
    let track = Song::new(1).tracks[0].clone();
    let labels: Vec<_> = SettingId::macros().filter_map(SettingId::label).collect();
    assert_eq!(
        labels,
        [
            "MAC1", "MAC2", "MAC3", "MAC4", "MAC5", "MAC6", "MAC7", "MAC8"
        ]
    );
    let third = SettingId::macro_at(2).unwrap();
    assert_eq!(third.key(), "macro3");
    assert_eq!(SettingId::from_key("macro3"), Some(third));
    assert_eq!(third.macro_place(), Some(2));
    assert_eq!(third.group(), "Macros");
    assert_eq!(
        FxCommand::typed("mac3", &track),
        Some(FxCommand::Set(third))
    );
    assert!(FxCommand::on_track(&track).contains(&FxCommand::Set(third)));
    for slot in 0..MOD_SLOTS {
        let offered = SettingId::for_mod(&track, slot);
        assert!(
            SettingId::macros().all(|id| offered.contains(&id)),
            "slot {slot} can move every macro"
        );
    }
    assert!(
        !SettingId::for_macro(&track).contains(&third),
        "a macro never reaches a macro"
    );
}

#[test]
fn a_slots_own_settings_are_moved_but_never_locked() {
    let mut track = Song::new(1).tracks[0].clone();
    track.mods[4].kind = ModKind::Steps;
    let mutate = SettingId::mod_own(4, 7).unwrap();
    assert_eq!(mutate.key(), "mod5.mutate");
    assert_eq!(mutate.owner(), (Owner::ModOwn(4), 7));
    assert!(!mutate.lockable());
    assert!(mutate.exists_on(&track));
    assert_eq!(mutate.name_on(&track), "M5 Mutate");
    assert!(SettingId::for_mod(&track, 2).contains(&mutate));
    assert!(
        !SettingId::for_mod(&track, 4).contains(&mutate),
        "no slot moves its own, nor one before it"
    );
    assert!(SettingId::for_macro(&track).contains(&mutate));
    assert!(!FxCommand::on_track(&track).contains(&FxCommand::Set(mutate)));
    let attack = SettingId::mod_own(4, 0).unwrap();
    assert!(!attack.exists_on(&track), "Steps have no attack");
    assert_eq!(attack.name_on(&track), "M5 Attack ---");
    let up = SettingId::accumulation(0).unwrap();
    assert_eq!(up.key(), "acu.up");
    assert!(SettingId::for_mod(&track, 0).contains(&up));
}

#[test]
fn a_slots_own_values_go_and_come_back_through_the_registrys_scale() {
    let track = Song::new(1).tracks[0].clone();
    for kind in ModKind::ALL {
        let mut slot = track.mods[0];
        slot.kind = kind;
        slot.adsr.sustain = 40;
        slot.steps.length = 7;
        slot.steps.mutate = 33;
        slot.steps.smooth = -90;
        for at in 0.. {
            let Some(id) = SettingId::mod_own(0, at) else {
                break;
            };
            let Some(value) = slot.own_value(at) else {
                continue;
            };
            let mut back = slot;
            back.set_own(at, f32::from(value) / 256.0);
            assert_eq!(back, slot, "{kind:?} {}", id.key());
        }
    }
    let mut accumulation = track.accumulation;
    accumulation.up = 9;
    let mut back = accumulation;
    back.set_own(0, f32::from(accumulation.own_value(0)) / 256.0);
    assert_eq!(back, accumulation);
    let mut arpeggio = track.arpeggio;
    arpeggio.octaves = 3;
    let mut back = arpeggio;
    back.set_own(1, f32::from(arpeggio.own_value(1)) / 256.0);
    back.set_own(0, f32::from(arpeggio.own_value(0)) / 256.0);
    assert_eq!(back, arpeggio);
}

#[test]
fn a_setting_is_on_one_macro_at_most_and_moves_when_mapped_to_another() {
    let song = Arc::new(Song::new(1));
    let song = edit(&song, &map(0, cut(), 0x3000, 0xA000)).unwrap();
    assert_eq!(song.tracks[0].macro_holding(cut()).unwrap().0, 0);
    let song = edit(&song, &map(0, cut(), 0x4000, 0xA000)).unwrap();
    assert_eq!(song.tracks[0].macros[0].maps.len(), 1, "changed in place");
    assert_eq!(song.tracks[0].macros[0].maps[0].min, 0x4000);
    let res = MacroTarget::Setting(SettingId::RES);
    let song = edit(&song, &map(0, res, 0, 0xFF00)).unwrap();
    let song = edit(&song, &map(4, cut(), 0, 0xFF00)).unwrap();
    let track = &song.tracks[0];
    assert_eq!(track.macro_holding(cut()).unwrap().0, 4);
    assert_eq!(track.macros[0].maps.len(), 1, "Res stays");
    let song = edit(
        &song,
        &Action::UnmapMacro {
            track: 0,
            target: cut(),
        },
    )
    .unwrap();
    assert!(song.tracks[0].macro_holding(cut()).is_none());
    assert!(song.tracks[0].has_macro_maps());
}

#[test]
fn a_macro_refuses_another_macro_a_value_out_of_range_and_a_long_name() {
    let song = Arc::new(Song::new(1));
    let other = MacroTarget::Setting(SettingId::macro_at(1).unwrap());
    assert_eq!(
        edit(&song, &map(0, other, 0, 0xFF00)),
        Err(EditError::Macro)
    );
    let mode = MacroTarget::Setting(SettingId::FILTER_MODE);
    assert_eq!(edit(&song, &map(0, mode, 0, 0xFF00)), Err(EditError::Macro));
    assert_eq!(
        edit(&song, &map(MACROS, cut(), 0, 0)),
        Err(EditError::Macro)
    );
    let name = |name: &str| Action::NameMacro {
        track: 0,
        at: 2,
        name: Some(name.to_owned()),
    };
    assert_eq!(
        edit(&song, &name("Far too long a name")),
        Err(EditError::Macro)
    );
    let song = edit(&song, &name("Change")).unwrap();
    assert_eq!(song.tracks[0].macros[2].name.as_deref(), Some("Change"));
}

#[test]
fn macros_are_saved_only_once_something_is_set_and_read_back() {
    let dir = folder();
    let song = Song::new(1);
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(!text.contains("macros"), "{text}");
    let song = Arc::new(song);
    let song = edit(&song, &map(2, cut(), 0x3000, 0xA000)).unwrap();
    let song = edit(
        &song,
        &Action::SetMacro {
            track: 0,
            at: 2,
            value: 0x8040,
        },
    )
    .unwrap();
    save_song(&dir, &song).unwrap();
    let text = fs::read_to_string(dir.join("song.ron")).unwrap();
    assert!(text.contains("macros"), "{text}");
    assert!(text.contains("Setting(\"cut\")"), "{text}");
    assert_eq!(&load_song(&dir).unwrap(), song.as_ref());
}
