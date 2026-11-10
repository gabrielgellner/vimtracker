//! What the file explorer lists for each of its purposes, after snacks.nvim's explorer: the picker
//! in tree mode (`input/picker.rs`) over the folders it starts from, read a folder at a time as
//! they're opened. In the sample explorer a kit opens like a folder, to its sounds, and a sound
//! with variants or layers opens to those.

use std::path::{Path, PathBuf};

use vt_model::kit::{KitEntry, load_kit};
use vt_model::project::is_project;
use vt_model::sample::is_sample_file;

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
    /// A kit file.
    Kit,
    /// A folder to render into, before the file's name is typed.
    Render,
}

impl Purpose {
    /// Whether it chooses a folder, which `Enter` does, before a name line.
    pub fn chooses_folder(self) -> bool {
        matches!(self, Self::Save | Self::Render)
    }

    /// Whether the preview starts open: only where there's something to show.
    pub fn previews(self) -> bool {
        !self.chooses_folder()
    }

    /// Whether moving onto a file plays it.
    pub fn auditions(self) -> bool {
        self == Self::Sample
    }

    /// The picker's title.
    pub fn title(self) -> &'static str {
        match self {
            Self::Project => "Open a project",
            Self::Save => "Save in",
            Self::Sample => "Samples",
            Self::Kit => "Kits",
            Self::Render => "Render into",
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
    Kit,
    /// A kit in the sample explorer, which opens like a folder to list its sounds.
    KitFolder,
}

impl Kind {
    /// Whether it opens, as a folder does, rather than being chosen.
    pub fn opens(self) -> bool {
        matches!(self, Self::Folder | Self::KitFolder)
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
pub fn list(folder: &Path, purpose: Purpose, hidden: bool) -> Vec<(PathBuf, Kind)> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
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
    found
}

/// The picker's item for an entry: a folder opens, and anything else is chosen.
pub fn item(path: &Path, kind: Kind) -> Item {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let path = path.to_path_buf();
    match kind {
        Kind::Folder => Item::new(name, Pick::Folder(path)),
        Kind::Project => {
            Item::new(name, Pick::Project(path.clone())).preview(Preview::Project(path))
        }
        Kind::Sample => Item::new(name, Pick::Sample(path.clone())).preview(Preview::Sample(path)),
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
    }
}

/// One thing in a folder or kit that's been opened: the path the tree knows it by, its item, and
/// whether it opens.
pub struct Entry {
    pub path: PathBuf,
    pub item: Item,
    pub opens: bool,
}

/// What's listed in a folder or kit as it's opened for a purpose. A kit's sounds are known by
/// their places in it, under the kit's own path, as `909 Core Kit.ron/0`, and a sound's files by
/// their places under the sound's, so the tree finds them again as it finds files.
pub fn contents(path: &Path, purpose: Purpose, hidden: bool) -> Vec<Entry> {
    if path.is_dir() {
        return list(path, purpose, hidden)
            .into_iter()
            .map(|(path, kind)| Entry {
                item: item(&path, kind),
                opens: kind.opens(),
                path,
            })
            .collect();
    }
    if path.is_file() {
        return kit_sounds(path);
    }
    let place = path
        .file_name()
        .and_then(|name| name.to_str()?.parse::<usize>().ok());
    match (path.parent(), place) {
        (Some(kit), Some(place)) if kit.is_file() => sound_files(kit, place),
        _ => Vec::new(),
    }
}

/// A kit's sounds, label first and named beside it, as the kit in use lists them. A sound with
/// variants or layers opens to them.
fn kit_sounds(kit: &Path) -> Vec<Entry> {
    let Ok(loaded) = load_kit(kit) else {
        return Vec::new();
    };
    loaded
        .sounds
        .iter()
        .enumerate()
        .map(|(place, sound)| Entry {
            path: kit.join(place.to_string()),
            item: kit_item(sound),
            opens: !sound.variants.is_empty() || !sound.layers.is_empty(),
        })
        .collect()
}

/// One of a kit's sounds as its files: its own, then its variants, then its velocity layers from
/// the softest, each by its file's name with what it is beside it. Each is chosen as the sound
/// is, with its label, choke group, gain and tune.
fn sound_files(kit: &Path, place: usize) -> Vec<Entry> {
    let Some(sound) = load_kit(kit)
        .ok()
        .and_then(|loaded| loaded.sounds.into_iter().nth(place))
    else {
        return Vec::new();
    };
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
    files
        .into_iter()
        .enumerate()
        .map(|(at, (file, meaning))| {
            let label = file.file_name().map_or_else(
                || file.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            let entry = KitEntry {
                path: file,
                ..sound.clone()
            };
            Entry {
                path: folder.join(at.to_string()),
                item: Item {
                    label,
                    meaning,
                    ..kit_item(&entry)
                },
                opens: false,
            }
        })
        .collect()
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
            names(&list(&root, Purpose::Sample, false)),
            [
                "drums Folder",
                "song Folder",
                "Zed Folder",
                "kick.wav Sample",
                "Snare.AIF Sample"
            ]
        );
        assert_eq!(
            names(&list(&root, Purpose::Kit, false)),
            ["drums Folder", "song Folder", "Zed Folder", "909.ron Kit"]
        );
        assert_eq!(
            names(&list(&root, Purpose::Project, false)),
            ["drums Folder", "Zed Folder", "song Project"],
            "a project is chosen, so it comes with the files"
        );
        for purpose in [Purpose::Save, Purpose::Render] {
            assert_eq!(
                names(&list(&root, purpose, false)),
                ["drums Folder", "song Folder", "Zed Folder"],
                "folders only"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn hidden_files_are_listed_only_when_asked_for() {
        let root = everything("hidden");
        let shown = names(&list(&root, Purpose::Sample, true));
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
}
