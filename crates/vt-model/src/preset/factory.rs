//! The patches that come with the program (decision 318): behaviour from the generators
//! tutorial's songs, each a track's macros and what they move, built into the program rather
//! than written into the presets folder, so `:patch` lists them from the start and they can't be
//! changed or written over. The files are made from the tutorial's songs by `just
//! tutorial-songs`, in `crates/vt-model/factory/patches`, and a test checks each is what its
//! builder makes, so a change to the format can't leave them stale.

use std::path::{Path, PathBuf};

use super::{PatchPreset, PresetError, patch::read_patch};

/// What the shipped patches are listed under, and what names one in a message.
pub const FACTORY: &str = "Factory";

/// Each shipped patch by name, with its file's text, in the tutorial's order.
pub const FACTORY_PATCHES: [(&str, &str); 9] = [
    (
        "Turing Machine",
        include_str!("../../factory/patches/Turing Machine.ron"),
    ),
    ("Deja vu", include_str!("../../factory/patches/Deja vu.ron")),
    ("Euclid", include_str!("../../factory/patches/Euclid.ron")),
    ("Kria", include_str!("../../factory/patches/Kria.ron")),
    ("Accents", include_str!("../../factory/patches/Accents.ron")),
    (
        "Acid line",
        include_str!("../../factory/patches/Acid line.ron"),
    ),
    (
        "Drunk walk",
        include_str!("../../factory/patches/Drunk walk.ron"),
    ),
    (
        "Marbles X",
        include_str!("../../factory/patches/Marbles X.ron"),
    ),
    (
        "Variations",
        include_str!("../../factory/patches/Variations.ron"),
    ),
];

/// Where a shipped patch is said to be in a message, as `Factory/Kria`.
fn origin(name: &str) -> PathBuf {
    Path::new(FACTORY).join(name)
}

/// The shipped patch of this name, read, or none if there isn't one.
pub fn factory_patch(name: &str) -> Option<Result<PatchPreset, PresetError>> {
    FACTORY_PATCHES
        .iter()
        .find(|(shipped, _)| *shipped == name)
        .map(|(name, text)| read_patch(text, &origin(name)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::Song;

    #[test]
    fn every_shipped_patch_reads_and_fits_a_new_track() {
        let track = &Song::new(1).tracks[0];
        for (name, _) in FACTORY_PATCHES {
            let preset = factory_patch(name).unwrap().unwrap();
            assert!(super::super::is_preset_name(name), "{name}");
            assert!(
                preset.patch.place(track, false).is_ok(),
                "{name} fits a new track"
            );
            for held in &preset.patch.macros {
                let named = held.name.as_deref().unwrap_or_default();
                assert!(crate::macros::valid_name(named), "{name}'s {named:?}");
            }
        }
        assert!(factory_patch("Nothing").is_none());
    }
}
