//! What songs and presets store things by, which a refactor mustn't move: each setting's place
//! and key, each effect's, machine's, fill's and mod type's name, and each generator setting's key
//! (`docs/designs/device-interface.md` §2.6, #718).
//!
//! Each list is kept in `golden/`, one line to a thing, in order. `VT_BLESS=1` writes them again
//! after an intended change, which then shows in the diff.

use std::collections::HashSet;
use std::path::PathBuf;

use vt_model::insert::Insert;
use vt_model::listed::choices_of;
use vt_model::modulation::ModKind;
use vt_model::settings::{Effect, SettingId};
use vt_model::shape::{FillKind, GenParam};
use vt_model::song::{FilterMode, Machine};
use vt_model::speed::Speed;

/// Checks `lines` against `golden/{name}.txt`, or writes it there with `VT_BLESS=1`.
fn golden(name: &str, lines: impl IntoIterator<Item = String>) {
    let text: String = lines.into_iter().map(|line| line + "\n").collect();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.txt"));
    if std::env::var_os("VT_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        return;
    }
    let saved = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{} is missing: run with VT_BLESS=1", path.display()));
    let (was, is): (HashSet<&str>, HashSet<&str>) =
        (saved.lines().collect(), text.lines().collect());
    let gone: Vec<&str> = saved.lines().filter(|line| !is.contains(line)).collect();
    let new: Vec<&str> = text.lines().filter(|line| !was.contains(line)).collect();
    assert!(
        saved == text,
        "{name} moved: gone {gone:?}, new {new:?} (or only the order changed); \
         run with VT_BLESS=1 if that was intended"
    );
}

fn ron(value: &impl serde::Serialize) -> String {
    ron::to_string(value).unwrap()
}

/// Each track's saved `settings` and `fine` maps are ordered by place, so a setting moved changes
/// the bytes a song saves, and a key changed loses what it held.
#[test]
fn every_setting_keeps_its_place_and_key() {
    golden(
        "settings",
        SettingId::all().map(|id| format!("{} {}", id.index(), id.key())),
    );
}

/// An effect's key starts its settings' keys; its name is what lists show.
#[test]
fn every_effect_keeps_its_key_and_name() {
    golden(
        "effects",
        Effect::ALL.map(|effect| format!("{} | {}", effect.key(), effect.name())),
    );
}

/// A slot saves by serde: its variant's name and its effect's fields, here as each starts.
#[test]
fn every_insert_saves_as_it_did() {
    golden(
        "inserts",
        Insert::ALL.map(|insert| format!("{} | {}", insert.name(), ron(&insert))),
    );
}

/// A sound saves its machine by serde.
#[test]
fn every_machine_saves_as_it_did() {
    golden(
        "machines",
        Machine::ALL.map(|machine| format!("{} | {}", ron(&machine), machine.name())),
    );
}

/// A macro mapping on a generator saves its setting's key.
#[test]
fn every_generator_setting_keeps_its_key() {
    golden(
        "gen_params",
        GenParam::ALL.map(|param| format!("{} | {}", param.key(), param.name())),
    );
}

/// A shape saves its fill by serde.
#[test]
fn every_fill_keeps_its_name() {
    golden(
        "fills",
        FillKind::ALL.map(|fill| format!("{} | {}", ron(&fill), fill.name())),
    );
}

/// A mod slot saves its type by serde.
#[test]
fn every_mod_type_keeps_its_name() {
    golden(
        "mods",
        ModKind::ALL.map(|kind| format!("{} | {}", ron(&kind), kind.name())),
    );
}

/// The filter's mode and the step's speed are saved as places in their lists.
#[test]
fn every_listed_value_keeps_its_place() {
    let modes = choices_of::<FilterMode>()
        .enumerate()
        .map(|(at, name)| format!("filter mode {at} {name}"));
    let speeds = choices_of::<Speed>()
        .enumerate()
        .map(|(at, name)| format!("speed {at} {name}"));
    golden("listed_values", modes.chain(speeds));
}
