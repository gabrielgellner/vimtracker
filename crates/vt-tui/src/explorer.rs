//! What the file explorer lists for each of its purposes, after snacks.nvim's explorer: the picker
//! in tree mode (`input/picker.rs`) over the folders it starts from, read a folder at a time as
//! they're opened. A kit opens like a folder, to its sounds, and a sound with variants or layers
//! opens to those; in the kit explorer, the kit dialog, its sounds are marked to load as tracks.
//! The preset browser lists the presets folder's sounds or effects (decision 310), or its patches
//! (decision 311).

use std::path::{Path, PathBuf};

use vt_model::kit::{KitEntry, load_kit};
use vt_model::preset::{PresetKind, load_effect, load_patch, load_sound, preset_name};
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

    /// The icon it's drawn with, as it's been found in its folder.
    pub fn icon(self, path: &Path) -> Icon {
        match self {
            Self::Folder => Icon::Folder,
            Self::Project => Icon::Project,
            Self::Sample if is_loop(path) => Icon::Loop,
            Self::Sample => Icon::Sample,
            Self::Kit | Self::KitFolder => Icon::Kit,
            Self::Preset(kind) => preset_icon(path, kind),
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

/// A preset's icon: a sound's by its machine, an effect's, or a warning if it won't read.
fn preset_icon(path: &Path, kind: PresetKind) -> Icon {
    match kind {
        PresetKind::Sound => {
            load_sound(path).map_or(Icon::Missing, |preset| Icon::Sound(preset.sound.machine))
        }
        PresetKind::Effect => load_effect(path).map_or(Icon::Missing, |_| Icon::Effect),
        PresetKind::Patch => load_patch(path).map_or(Icon::Missing, |_| Icon::Patch),
    }
}

/// What a preset is, beside its name in the browser, or why it won't read.
fn preset_meaning(path: &Path, kind: PresetKind) -> String {
    let read = match kind {
        PresetKind::Sound => load_sound(path).map(|preset| preset.summary()),
        PresetKind::Effect => load_effect(path).map(|preset| preset.summary()),
        PresetKind::Patch => load_patch(path).map(|preset| preset.summary()),
    };
    read.unwrap_or_else(|error| match error {
        vt_model::preset::PresetError::Parse { detail, .. }
        | vt_model::preset::PresetError::Holds { detail, .. } => detail,
        error => error.to_string(),
    })
}

/// The icon for a sample file a kit names: a loop's, or a warning if it isn't there.
pub fn sample_icon(path: &Path) -> Icon {
    if !path.exists() {
        Icon::Missing
    } else if is_loop(path) {
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

/// The words in a name, in lower case, without the spaces and marks between them.
fn words(name: &std::ffi::OsStr) -> Vec<String> {
    name.to_string_lossy()
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether a sample is a loop, as far as its folders and name say. The nearest folder named for
/// loops or one-shots decides, as Ableton's library has `Loops` and `One Shots`, whose one-shots
/// can have a tempo in their names. Otherwise a tempo in its name, such as `Break 120 bpm.wav`,
/// makes it a loop, as sample packs name them.
pub fn is_loop(path: &Path) -> bool {
    for folder in path.ancestors().skip(1) {
        let Some(name) = folder.file_name() else {
            continue;
        };
        let words = words(name);
        let has = |wanted: &str| words.iter().any(|word| word == wanted);
        if has("loop") || has("loops") {
            return true;
        }
        let one_shot = words
            .windows(2)
            .any(|pair| pair[0] == "one" && matches!(pair[1].as_str(), "shot" | "shots"));
        if one_shot || has("oneshot") || has("oneshots") {
            return false;
        }
    }
    let Some(stem) = path.file_stem() else {
        return false;
    };
    let words = words(stem);
    let tempo = |word: &str| {
        (2..=3).contains(&word.len())
            && word
                .parse::<u32>()
                .is_ok_and(|bpm| (40..=300).contains(&bpm))
    };
    words.iter().enumerate().any(|(at, word)| {
        let joined = word.strip_suffix("bpm").is_some_and(tempo);
        let apart = word == "bpm" && at > 0 && tempo(&words[at - 1]);
        joined || apart
    })
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
        // By the name `:preset load` takes, with what it is beside it.
        Kind::Preset(preset) => Item::new(
            preset_name(&path),
            Pick::Preset {
                kind: preset,
                path: path.clone(),
            },
        )
        .meaning(preset_meaning(&path, preset)),
    }
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
pub fn contents(path: &Path, purpose: Purpose, hidden: bool) -> Option<Vec<Entry>> {
    if path.is_dir() {
        let listed = list(path, purpose, hidden).ok()?;
        let entries = listed.into_iter().map(|(path, kind)| Entry {
            item: item(&path, kind),
            icon: kind.icon(&path),
            opens: kind.opens(),
            path,
        });
        return Some(entries.collect());
    }
    if path.is_file() {
        return kit_sounds(path);
    }
    let place = path
        .file_name()
        .and_then(|name| name.to_str()?.parse::<usize>().ok());
    match (path.parent(), place) {
        (Some(kit), Some(place)) if kit.is_file() => sound_files(kit, place),
        _ => None,
    }
}

/// A kit's sounds, label first and named beside it, as the kit in use lists them. A sound with
/// variants or layers opens to them.
fn kit_sounds(kit: &Path) -> Option<Vec<Entry>> {
    let loaded = load_kit(kit).ok()?;
    let sounds = loaded.sounds.iter().enumerate().map(|(place, sound)| {
        let opens = !sound.variants.is_empty() || !sound.layers.is_empty();
        Entry {
            path: kit.join(place.to_string()),
            item: kit_item(sound),
            icon: kit_sound_icon(&sound.path, opens),
            opens,
        }
    });
    Some(sounds.collect())
}

/// One of a kit's sounds as its files: its own, then its variants, then its velocity layers from
/// the softest, each by its file's name with what it is beside it. Each is chosen as the sound
/// is, with its label, choke group, gain and tune.
fn sound_files(kit: &Path, place: usize) -> Option<Vec<Entry>> {
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
        let icon = sample_icon(&file);
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
    fn a_loop_is_known_by_its_folder_or_the_tempo_in_its_name() {
        let library = "/Applications/Live.app/Core Library/Samples";
        let is = |path: &str| is_loop(Path::new(&path.replace("LIB", library)));
        assert!(is("LIB/Loops/FX/Scratch Baby.aif"), "under Loops");
        assert!(is("/samples/Drum Loops/amen.wav"));
        assert!(is("/samples/Break 90 bpm.wav"), "a tempo");
        assert!(is("/samples/break_174bpm.wav"));
        assert!(
            !is("LIB/One Shots/FX/Impact Deep Pad 120 bpm.wav"),
            "a one-shot's folder says so, whatever its name"
        );
        assert!(!is("/samples/oneshots/hit 120 bpm.wav"));
        assert!(!is("/samples/kick.wav"));
        assert!(!is("/samples/909 kick.wav"), "a number alone isn't a tempo");
        assert!(!is("/samples/bpm.wav"));
        assert!(!is("/samples/Looper/kick.wav"), "a word, not part of one");
    }

    #[test]
    fn each_entry_has_an_icon_and_a_folder_that_wont_read_has_none() {
        let root = everything("icons");
        let icons = |purpose: Purpose| -> Vec<(String, Icon)> {
            contents(&root, purpose, false)
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
        assert!(contents(&root.join("gone"), Purpose::Sample, false).is_none());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
