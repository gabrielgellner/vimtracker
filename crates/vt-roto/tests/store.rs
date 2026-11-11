//! vimtracker's own setups stored on the Roto: written under its own hashes only, only when they
//! change, never over another program's, and removed without touching anything else.

use vt_roto::bytes::{Label, PluginHash};
use vt_roto::fake::{FakeRoto, StoredSetup};
use vt_roto::live::{Feel, KNOB_TOP, LiveControl, LiveDevice};
use vt_roto::serial::{Request, StoredKind, StoredPlugin, SwitchHaptic};
use vt_roto::store::{self, Guarded, LIGHT_OFF, NAME_PREFIX, OwnSetup, StoreError, Versions};

fn control(name: &str, feel: Feel, position: u16) -> Option<LiveControl> {
    Some(LiveControl {
        name: Label::new(name),
        feel,
        position,
        colour: 19,
    })
}

/// A compressor stored with two buttons.
fn glue() -> LiveDevice {
    let mut controls = vec![None; 8];
    controls[0] = control("Threshold", Feel::Smooth, 0);
    controls[1] = control(
        "Ratio",
        Feel::Steps(vec![
            Label::new("2:1"),
            Label::new("4:1"),
            Label::new("10:1"),
        ]),
        0,
    );
    let mut switches = vec![None; 8];
    switches[4] = control("Auto", Feel::Switch, 0);
    switches[6] = control("Peak Clip", Feel::Switch, KNOB_TOP);
    LiveDevice {
        key: format!("{}fx/comp-glue", store::NAMESPACE),
        name: Label::new("1 Comp"),
        enabled: true,
        stored: true,
        controls,
        switches,
    }
}

/// A setup another program stored, as Ableton Live's are.
fn ableton(name: &str) -> StoredSetup {
    StoredSetup {
        plugin: StoredPlugin {
            hash: PluginHash::of(&format!("ableton {name}")),
            name: Label::new(name),
            kind: StoredKind::Normal,
        },
        knobs: Default::default(),
        switches: Default::default(),
    }
}

fn changes(fake: &FakeRoto) -> Vec<Request> {
    fake.serial_changes.clone()
}

#[test]
fn a_setup_is_written_once_and_not_again_until_it_changes() {
    let mut fake = FakeRoto::new();
    fake.setups.push(ableton("Drift"));
    let setup = OwnSetup::of(&glue(), "Glue");
    let mut versions = Versions::new();
    let synced = store::sync(&mut fake, std::slice::from_ref(&setup), &mut versions).unwrap();
    assert_eq!(synced.written, std::slice::from_ref(&setup.key));
    assert_eq!(fake.setups.len(), 2, "Drift kept, and the Glue added");
    let stored = &fake.setups[1];
    assert_eq!(stored.plugin.name.as_str(), "vt Glue");
    assert_eq!(stored.knobs.len(), 2);
    assert_eq!(stored.switches.len(), 2);
    let auto = &stored.switches[&4];
    assert_eq!(auto.name.as_str(), "Auto");
    assert_eq!(auto.param_index, 5, "numbered from 1 across the pages");
    assert_eq!(auto.haptic, SwitchHaptic::Push);
    assert_eq!((auto.light_on, auto.light_off), (19, LIGHT_OFF));
    // Every change was to vimtracker's own setup, inside one config update.
    let made = changes(&fake);
    assert_eq!(made.first(), Some(&Request::StartConfigUpdate));
    assert_eq!(made.last(), Some(&Request::EndConfigUpdate));
    for request in &made[1..made.len() - 1] {
        assert_eq!(request.plugin_changed(), Some(setup.hash()), "{request:?}");
    }
    // The same again: nothing written.
    let before = fake.serial_changes.len();
    let again = store::sync(&mut fake, std::slice::from_ref(&setup), &mut versions).unwrap();
    assert_eq!((again.written.len(), again.unchanged), (0, 1));
    assert_eq!(
        fake.serial_changes.len(),
        before,
        "nothing sent that changes it"
    );
    // Changed: written again in its place.
    let mut changed = glue();
    changed.switches[6] = control("Clip", Feel::Switch, 0);
    let changed = OwnSetup::of(&changed, "Glue");
    assert_ne!(changed.version(), setup.version());
    let third = store::sync(&mut fake, std::slice::from_ref(&changed), &mut versions).unwrap();
    assert_eq!(third.written.len(), 1);
    assert!(fake.serial_changes[before..].contains(&Request::ClearPlugin(setup.hash())));
    assert_eq!(fake.setups.len(), 2);
    assert_eq!(fake.setups[1].switches[&6].name.as_str(), "Clip");
    assert_eq!(fake.setups[0].plugin.name.as_str(), "Drift");
}

#[test]
fn a_setup_already_there_with_no_version_known_is_written_again() {
    let mut fake = FakeRoto::new();
    let setup = OwnSetup::of(&glue(), "Glue");
    store::sync(
        &mut fake,
        std::slice::from_ref(&setup),
        &mut Versions::new(),
    )
    .unwrap();
    // Another machine, which hasn't a record of it.
    let synced = store::sync(
        &mut fake,
        std::slice::from_ref(&setup),
        &mut Versions::new(),
    )
    .unwrap();
    assert_eq!(synced.written.len(), 1);
    assert_eq!(fake.setups.len(), 1);
}

#[test]
fn another_programs_setup_under_vimtrackers_hash_stops_it_with_nothing_written() {
    let mut fake = FakeRoto::new();
    let setup = OwnSetup::of(&glue(), "Glue");
    fake.setups.push(StoredSetup {
        plugin: StoredPlugin {
            hash: setup.hash(),
            name: Label::new("Not ours"),
            kind: StoredKind::Normal,
        },
        knobs: Default::default(),
        switches: Default::default(),
    });
    let result = store::sync(
        &mut fake,
        std::slice::from_ref(&setup),
        &mut Versions::new(),
    );
    assert!(matches!(result, Err(StoreError::NotOurs(_))), "{result:?}");
    assert!(fake.serial_changes.is_empty());
    assert_eq!(fake.setups[0].plugin.name.as_str(), "Not ours");
}

#[test]
fn a_full_store_is_left_as_it_is() {
    let mut fake = FakeRoto::new();
    for at in 0..64 {
        fake.setups.push(ableton(&format!("Live {at}")));
    }
    let setup = OwnSetup::of(&glue(), "Glue");
    let result = store::sync(
        &mut fake,
        std::slice::from_ref(&setup),
        &mut Versions::new(),
    );
    assert!(
        matches!(
            result,
            Err(StoreError::Full {
                stored: 64,
                needed: 1
            })
        ),
        "{result:?}"
    );
    assert!(fake.serial_changes.is_empty());
}

#[test]
fn the_guard_refuses_anything_but_vimtrackers_own_setups_and_never_resets() {
    let mut fake = FakeRoto::new();
    fake.setups.push(ableton("Drift"));
    let ours = OwnSetup::of(&glue(), "Glue").hash();
    let drift = fake.setups[0].plugin.hash;
    let mut guarded = Guarded::new(&mut fake, [ours]);
    for request in [
        Request::FactoryReset,
        Request::ClearPlugin(drift),
        Request::SetPluginName {
            hash: drift,
            name: Label::new("x"),
        },
        Request::ClearMidiSetup(0),
        Request::SetSetupName {
            setup: 0,
            name: Label::new("x"),
        },
    ] {
        assert!(
            matches!(guarded.send(&request), Err(StoreError::NotOurs(_))),
            "{request:?}"
        );
    }
    assert!(
        guarded.send(&Request::GetFirstPlugin).is_ok(),
        "asking is fine"
    );
    assert!(fake.serial_changes.is_empty(), "nothing refused was sent");
    assert_eq!(fake.setups.len(), 1);
}

#[test]
fn clearing_removes_vimtrackers_own_setups_and_nothing_else() {
    let mut fake = FakeRoto::new();
    fake.setups.push(ableton("Drift"));
    let setup = OwnSetup::of(&glue(), "Glue");
    store::sync(
        &mut fake,
        std::slice::from_ref(&setup),
        &mut Versions::new(),
    )
    .unwrap();
    fake.setups.push(ableton("Acid V"));
    let found = store::own_stored(&mut fake, [setup.hash()]).unwrap();
    assert_eq!(found.len(), 1);
    assert!(found[0].name.as_str().starts_with(NAME_PREFIX));
    let removed = store::clear_own(&mut fake, [setup.hash()]).unwrap();
    assert_eq!(removed.len(), 1);
    let left: Vec<&str> = fake.setups.iter().map(|s| s.plugin.name.as_str()).collect();
    assert_eq!(left, ["Drift", "Acid V"]);
    // Nothing of vimtracker's left: nothing more to do.
    assert!(
        store::clear_own(&mut fake, [setup.hash()])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_stored_knob_keeps_up_to_sixteen_named_steps_and_a_longer_list_turns_smoothly() {
    let mut device = glue();
    let names: Vec<Label> = (0..18).map(|at| Label::new(&at.to_string())).collect();
    device.controls[2] = control("Long", Feel::Steps(names), 0);
    device.controls[3] = control("Pan", Feel::CentreNotch, 0);
    let setup = OwnSetup::of(&device, "Glue");
    use vt_roto::serial::KnobHaptic;
    assert_eq!(setup.knobs[1].haptic, KnobHaptic::Steps(3));
    assert_eq!(setup.knobs[1].step_names.len(), 3);
    assert_eq!(
        setup.knobs[2].haptic,
        KnobHaptic::Smooth { indents: [None; 2] }
    );
    assert_eq!(setup.knobs[3].haptic, KnobHaptic::CentreIndent);
    assert!(setup.knobs.iter().all(|knob| !knob.macro_param));
}
