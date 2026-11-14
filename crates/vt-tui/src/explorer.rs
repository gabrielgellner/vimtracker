//! What the file explorer lists for each of its purposes, after snacks.nvim's explorer: the picker
//! in tree mode (`input/picker.rs`) over the folders it starts from, read a folder at a time as
//! they're opened. A kit opens like a folder, to its sounds, and a sound with variants or layers
//! opens to those; in the kit explorer, the kit dialog, its sounds are marked to load as tracks.
//! The preset browser lists the presets folder's sounds or effects (decision 310), or its patches
//! (decision 311). Each sample, kit sound and preset is listed with its tags (decision 315).

use std::path::{Path, PathBuf};

use vt_model::kit::{KitEntry, load_kit};
use vt_model::preset::{PresetKind, load_effect, load_patch, load_sound, preset_name};
use vt_model::project::is_project;
use vt_model::sample::is_sample_file;
use vt_model::tag::{self, Source, Tag, TagLibrary};

use crate::input::picker::{Item, Pick, Preview};
use crate::picking::kit_item;

/// What the explorer is choosing, which decides what it lists and what choosing does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// A project to open: project folders, and the folders that hold them.
    Project,
    /// A folder to save a project in, before its name is typed.
    Save,
    /// A sample file for a track.
    Sample,
    /// A kit file, whose sounds can be marked and loaded as new tracks.
    Kit,
    /// A folder to render into, before the file's name is typed.
    Render,
    /// A preset to load on the cursor's track: its sound, or the effect in the slot under the
    /// cursor.
    Preset(PresetKind),
}

impl Purpose {
    /// Whether it chooses a folder, which `Enter` does, before a name line.
    pub fn chooses_folder(self) -> bool {
        matches!(self, Self::Save | Self::Render)
    }

    /// Whether the preview starts open: only where there's something to show. A preset says
    /// what it is beside its name instead.
    pub fn previews(self) -> bool {
        !self.chooses_folder() && !matches!(self, Self::Preset(_))
    }

    /// Whether what it lists has tags, which a search finds with `#` and `T` changes: samples,
    /// kits' sounds and presets (decision 315).
    pub fn tags(self) -> bool {
        matches!(self, Self::Sample | Self::Kit | Self::Preset(_))
    }

    /// Whether moving onto a file plays it: a sample, or a kit's sound in either explorer.
    pub fn auditions(self) -> bool {
        matches!(self, Self::Sample | Self::Kit)
    }

    /// The picker's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Project => "Open a project",
            Self::Save => "Save in",
            Self::Sample => "Samples",
            Self::Kit => "Kits",
            Self::Render => "Render into",
            Self::Preset(PresetKind::Sound) => "Sound presets",
            Self::Preset(PresetKind::Effect) => "Effect presets",
            Self::Preset(PresetKind::Patch) => "Patches",
        }
    }
}

/// What an entry in a folder is, for a purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A folder to open.
    Folder,
    /// A project's folder, which is chosen rather than opened.
    Project,
    Sample,
    /// A kit in the kit explorer, which `Enter` chooses and `l` opens to its sounds.
    Kit,
    /// A kit in the sample explorer, which opens like a folder to list its sounds.
    KitFolder,
    /// A preset of this kind, which `Enter` loads.
    Preset(PresetKind),
}

impl Kind {
    /// Whether it opens, as a folder does. A kit in the kit explorer opens too, though `Enter`
    /// chooses it.
    pub fn opens(self) -> bool {
        matches!(self, Self::Folder | Self::KitFolder | Self::Kit)
    }

    /// The icon a file or folder is drawn with, as it's been found in its folder, with its tags:
    /// a sample tagged `loop` is drawn as one.
    pub fn icon(self, tags: &[Tag]) -> Icon {
        match self {
            Self::Folder => Icon::Folder,
            Self::Project => Icon::Project,
            Self::Sample if tag::has(tags, tag::LOOP) => Icon::Loop,
            Self::Sample => Icon::Sample,
            Self::Kit | Self::KitFolder => Icon::Kit,
            // A preset's comes from reading it, which `item` does.
            Self::Preset(_) => Icon::Missing,
        }
    }
}

/// What a row of the explorer is, which its icon shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// A folder, or a place the explorer starts from, drawn open or closed.
    Folder,
    /// A project's folder.
    Project,
    /// A kit, or the kit in use at the top of the sample explorer.
    Kit,
    /// One of a kit's sounds.
    KitSound,
    /// One of a kit's sounds that opens to its variants or velocity layers.
    KitSoundLayered,
    Sample,
    /// A sample that's a loop, as far as its folders and name say.
    Loop,
    /// A sound preset, by what its machine is: a sampler, a drum voice or another synth voice.
    Sound(vt_model::song::Machine),
    /// An effect preset.
    Effect,
    /// A patch, behaviour without a sound.
    Patch,
    /// A file that isn't there, or a file or folder that couldn't be read.
    Missing,
}

/// How a preset is listed, from reading it once: its icon, a sound's by its machine; what it is,
/// beside its name; and its tags, those it always has and then its own. One that won't read has
/// a warning, and why beside it.
fn preset_listing(path: &Path, kind: PresetKind) -> (Icon, String, Vec<Tag>) {
    let read = match kind {
        PresetKind::Sound => load_sound(path).map(|preset| {
            let icon = Icon::Sound(preset.sound.machine);
            (icon, preset.summary(), preset.fixed_tags(), preset.tags)
        }),
        PresetKind::Effect => load_effect(path).map(|preset| {
            (
                Icon::Effect,
                preset.summary(),
                preset.fixed_tags(),
                preset.tags,
            )
        }),
        PresetKind::Patch => load_patch(path).map(|preset| {
            (
                Icon::Patch,
                preset.summary(),
                preset.fixed_tags(),
                preset.tags,
            )
        }),
    };
    match read {
        Ok((icon, meaning, fixed, own)) => {
            let mut tags: Vec<Tag> = fixed
                .into_iter()
                .map(|name| Tag::new(name, Source::Fixed))
                .collect();
            for name in own {
                if !tag::has(&tags, &name) {
                    tags.push(Tag::new(name, Source::Own));
                }
            }
            (icon, meaning, tags)
        }
        Err(error) => {
            let why = match error {
                vt_model::preset::PresetError::Parse { detail, .. }
                | vt_model::preset::PresetError::Holds { detail, .. } => detail,
                error => error.to_string(),
            };
            (Icon::Missing, why, Vec::new())
        }
    }
}

/// A sample file's tags: what's guessed from where it is, its name and, for one of a kit's
/// sounds, its label, and what's kept for it by hand (decision 315).
pub fn file_tags(path: &Path, label: &str, library: &TagLibrary) -> Vec<Tag> {
    tag::file_tags(&tag::guess(path, label), library.of_file(path))
}

/// The icon for a sample file a kit names: a loop's if its tags say so, or a warning if it isn't
/// there.
pub fn sample_icon(path: &Path, tags: &[Tag]) -> Icon {
    if !path.exists() {
        Icon::Missing
    } else if tag::has(tags, tag::LOOP) {
        Icon::Loop
    } else {
        Icon::Sample
    }
}

/// The icon for one of a kit's sounds, played from this file: a warning if it isn't there.
pub fn kit_sound_icon(path: &Path, layered: bool) -> Icon {
    if !path.exists() {
        Icon::Missing
    } else if layered {
        Icon::KitSoundLayered
    } else {
        Icon::KitSound
    }
}

/// What a path is, for a purpose, or none if it isn't listed.
pub fn kind_of(path: &Path, purpose: Purpose) -> Option<Kind> {
    if path.is_dir() {
        return Some(if purpose == Purpose::Project && is_project(path) {
            Kind::Project
        } else {
            Kind::Folder
        });
    }
    let kit = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ron"));
    match purpose {
        Purpose::Sample if is_sample_file(path) => Some(Kind::Sample),
        Purpose::Kit if kit => Some(Kind::Kit),
        // Only a file that reads as a kit, so a project's `song.ron` isn't one.
        Purpose::Sample if kit && load_kit(path).is_ok() => Some(Kind::KitFolder),
        Purpose::Preset(preset) if kit => Some(Kind::Preset(preset)),
        _ => None,
    }
}

/// Whether a name is hidden, as a dot file is.
pub fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
}

/// What a folder holds that's listed for a purpose, folders first and then files, each in order
/// of name ignoring case, as snacks sorts them. Hidden ones are left out unless asked for.
pub fn list(
    folder: &Path,
    purpose: Purpose,
    hidden: bool,
) -> std::io::Result<Vec<(PathBuf, Kind)>> {
    let read = std::fs::read_dir(folder)?;
    let mut found: Vec<(PathBuf, Kind)> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| hidden || !is_hidden(path))
        .filter_map(|path| Some((path.clone(), kind_of(&path, purpose)?)))
        .collect();
    found.sort_by_key(|(path, kind)| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        (*kind != Kind::Folder, name)
    });
    Ok(found)
}

/// The picker's item for an entry, with its tags, and its icon: a folder opens, and anything else
/// is chosen.
pub fn item(path: &Path, kind: Kind, library: &TagLibrary) -> (Item, Icon) {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let path = path.to_path_buf();
    let item = match kind {
        Kind::Folder => Item::new(name, Pick::Folder(path)),
        Kind::Project => {
            Item::new(name, Pick::Project(path.clone())).preview(Preview::Project(path))
        }
        Kind::Sample => {
            let tags = file_tags(&path, "", library);
            Item::new(name, Pick::Sample(path.clone()))
                .preview(Preview::Sample(path))
                .tagged(tags)
        }
        // A kit by the name `:kit` takes, without `.ron`.
        Kind::Kit => Item::new(
            path.file_stem()
                .map_or(name, |stem| stem.to_string_lossy().into_owned()),
            Pick::Kit(path.clone()),
        )
        .wider(Pick::KitTracks(path.clone()))
        .preview(Preview::Kit(path)),
        // By the kit's own name, such as `909 Core Kit`.
        Kind::KitFolder => Item::new(
            load_kit(&path).map_or(name, |kit| kit.name),
            Pick::Folder(path.clone()),
        )
        .preview(Preview::Kit(path)),
        // By the name `:preset load` takes, with what it is beside it.
        Kind::Preset(preset) => {
            let (icon, meaning, tags) = preset_listing(&path, preset);
            let item = Item::new(
                preset_name(&path),
                Pick::Preset {
                    kind: preset,
                    path: path.clone(),
                },
            )
            .meaning(meaning)
            .tagged(tags);
            return (item, icon);
        }
    };
    let icon = kind.icon(&item.tags);
    (item, icon)
}

/// One thing in a folder or kit that's been opened: the path the tree knows it by, its item, its
/// icon, and whether it opens.
pub struct Entry {
    pub path: PathBuf,
    pub item: Item,
    pub icon: Icon,
    pub opens: bool,
}

/// What's listed in a folder or kit as it's opened for a purpose, or none if it couldn't be read.
/// A kit's sounds are known by their places in it, under the kit's own path, as
/// `909 Core Kit.ron/0`, and a sound's files by their places under the sound's, so the tree finds
/// them again as it finds files.
pub fn contents(
    path: &Path,
    purpose: Purpose,
    hidden: bool,
    library: &TagLibrary,
) -> Option<Vec<Entry>> {
    if path.is_dir() {
        let listed = list(path, purpose, hidden).ok()?;
        let entries = listed.into_iter().map(|(path, kind)| {
            let (item, icon) = item(&path, kind, library);
            Entry {
                item,
                icon,
                opens: kind.opens(),
                path,
            }
        });
        return Some(entries.collect());
    }
    if path.is_file() {
        return kit_sounds(path, library);
    }
    let place = path
        .file_name()
        .and_then(|name| name.to_str()?.parse::<usize>().ok());
    match (path.parent(), place) {
        (Some(kit), Some(place)) if kit.is_file() => sound_files(kit, place, library),
        _ => None,
    }
}

/// A kit's sounds, label first and named beside it, as the kit in use lists them. A sound with
/// variants or layers opens to them.
fn kit_sounds(kit: &Path, library: &TagLibrary) -> Option<Vec<Entry>> {
    let loaded = load_kit(kit).ok()?;
    let sounds = loaded.sounds.iter().enumerate().map(|(place, sound)| {
        let opens = !sound.variants.is_empty() || !sound.layers.is_empty();
        Entry {
            path: kit.join(place.to_string()),
            item: kit_item(sound).tagged(file_tags(&sound.path, &sound.label, library)),
            icon: kit_sound_icon(&sound.path, opens),
            opens,
        }
    });
    Some(sounds.collect())
}

/// One of a kit's sounds as its files: its own, then its variants, then its velocity layers from
/// the softest, each by its file's name with what it is beside it. Each is chosen as the sound
/// is, with its label, choke group, gain and tune.
fn sound_files(kit: &Path, place: usize, library: &TagLibrary) -> Option<Vec<Entry>> {
    let sound = load_kit(kit).ok()?.sounds.into_iter().nth(place)?;
    let layers = sound.layers.len();
    let mut files = vec![(sound.path.clone(), "Main".to_owned())];
    files.extend(
        sound
            .variants
            .iter()
            .enumerate()
            .map(|(at, path)| (path.clone(), format!("Variant {}", at + 1))),
    );
    files.extend(sound.layers.iter().enumerate().map(|(at, path)| {
        let which = match at {
            0 if layers > 1 => " soft",
            _ if at + 1 == layers && layers > 1 => " hard",
            _ => "",
        };
        (path.clone(), format!("Layer {}{which}", at + 1))
    }));
    let folder = kit.join(place.to_string());
    let entries = files.into_iter().enumerate().map(|(at, (file, meaning))| {
        let label = file.file_name().map_or_else(
            || file.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let tags = file_tags(&file, &sound.label, library);
        let icon = sample_icon(&file, &tags);
        let entry = KitEntry {
            path: file,
            ..sound.clone()
        };
        Entry {
            path: folder.join(at.to_string()),
            item: Item {
                label,
                meaning,
                tags,
                ..kit_item(&entry)
            },
            icon,
            opens: false,
        }
    });
    Some(entries.collect())
}

/// A folder as the top of the tree names it: from the home folder as `~`, or in full.
pub fn place_name(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-explorer-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn names(found: &[(PathBuf, Kind)]) -> Vec<String> {
        found
            .iter()
            .map(|(path, kind)| {
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                format!("{name} {kind:?}")
            })
            .collect()
    }

    /// A folder with a bit of everything in it.
    fn everything(name: &str) -> PathBuf {
        let root = folder(name);
        for folder in ["song", "drums", ".git", "Zed"] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        std::fs::write(root.join("song/song.ron"), "").unwrap();
        for file in [
            "kick.wav",
            "Snare.AIF",
            "909.ron",
            "notes.txt",
            ".hidden.wav",
        ] {
            std::fs::write(root.join(file), "").unwrap();
        }
        root
    }

    #[test]
    fn each_purpose_lists_folders_first_and_only_its_own_files() {
        let root = everything("purposes");
        assert_eq!(
            names(&list(&root, Purpose::Sample, false).unwrap()),
            [
                "drums Folder",
                "song Folder",
                "Zed Folder",
                "kick.wav Sample",
                "Snare.AIF Sample"
            ]
        );
        assert_eq!(
            names(&list(&root, Purpose::Kit, false).unwrap()),
            ["drums Folder", "song Folder", "Zed Folder", "909.ron Kit"]
        );
        assert_eq!(
            names(&list(&root, Purpose::Project, false).unwrap()),
            ["drums Folder", "Zed Folder", "song Project"],
            "a project is chosen, so it comes with the files"
        );
        for purpose in [Purpose::Save, Purpose::Render] {
            assert_eq!(
                names(&list(&root, purpose, false).unwrap()),
                ["drums Folder", "song Folder", "Zed Folder"],
                "folders only"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hidden_files_are_listed_only_when_asked_for() {
        let root = everything("hidden");
        let shown = names(&list(&root, Purpose::Sample, true).unwrap());
        assert!(shown.contains(&".git Folder".to_owned()), "{shown:?}");
        assert!(
            shown.contains(&".hidden.wav Sample".to_owned()),
            "{shown:?}"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_place_is_named_from_the_home_folder() {
        let home = Path::new("/Users/me");
        assert_eq!(place_name(home, Some(home)), "~");
        assert_eq!(
            place_name(Path::new("/Users/me/Music/samples"), Some(home)),
            "~/Music/samples"
        );
        assert_eq!(
            place_name(Path::new("/Volumes/Card"), Some(home)),
            "/Volumes/Card"
        );
    }

    #[test]
    fn each_entry_has_an_icon_and_a_folder_that_wont_read_has_none() {
        let root = everything("icons");
        let icons = |purpose: Purpose| -> Vec<(String, Icon)> {
            contents(&root, purpose, false, &TagLibrary::default())
                .unwrap()
                .into_iter()
                .map(|entry| (entry.item.label, entry.icon))
                .collect()
        };
        assert_eq!(
            icons(Purpose::Project)[2],
            ("song".to_owned(), Icon::Project)
        );
        assert_eq!(icons(Purpose::Kit)[3], ("909".to_owned(), Icon::Kit));
        assert_eq!(
            icons(Purpose::Sample)[3],
            ("kick.wav".to_owned(), Icon::Sample)
        );
        assert!(
            contents(
                &root.join("gone"),
                Purpose::Sample,
                false,
                &TagLibrary::default()
            )
            .is_none()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
