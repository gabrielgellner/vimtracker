//! The program's settings for this machine, kept in `settings.ron` beside `audio.ron`: where its
//! kits and samples are. They belong to the machine rather than to a song.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The settings file's name, in the config folder.
pub const SETTINGS_FILE: &str = "settings.ron";

/// What a new settings file says: the defaults, with a note on each.
const TEMPLATE: &str = r#"// vimtracker's settings for this machine. A path can start with ~/ for your home folder.
(
    // Folders to find kits in, in order. `:kit 909` opens the first 909.ron found.
    kits: ["~/Music/vimtracker/kits"],
    // Folders the sample browser lists, after the project's own samples.
    samples: ["~/Music/vimtracker/samples"],
)
"#;

/// The config folder: `$XDG_CONFIG_HOME/vimtracker`, or `~/.config/vimtracker`.
pub fn config_folder() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("vimtracker"))
}

/// A path as it's written, with a leading `~/` meaning the home folder.
pub fn expand_home(text: &str) -> PathBuf {
    if let Some(rest) = text.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(text)
}

/// Where kits and samples are found. Each list is searched in order, and a folder that isn't
/// there is passed over. Paths are kept as written, so `~/` stays readable in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub kits: Vec<String>,
    pub samples: Vec<String>,
}

impl Default for Settings {
    /// Kits and samples in their own folders under `~/Music/vimtracker`, as the M8 and the
    /// Polyend Tracker keep theirs in folders on the card.
    fn default() -> Self {
        Self {
            kits: vec!["~/Music/vimtracker/kits".to_owned()],
            samples: vec!["~/Music/vimtracker/samples".to_owned()],
        }
    }
}

impl Settings {
    /// No folders at all, as the program starts before its settings are read, and as tests
    /// use so nothing on this machine is found.
    pub fn none() -> Self {
        Self {
            kits: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// Where they're kept, if there's a home folder.
    pub fn path() -> Option<PathBuf> {
        config_folder().map(|folder| folder.join(SETTINGS_FILE))
    }

    /// Read the settings at `path`. If there's no file, the defaults are written there, so
    /// there's one to edit, and used. A file that won't read gives the defaults and says why.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => match ron::from_str(&text) {
                Ok(settings) => (settings, None),
                Err(error) => (
                    Self::default(),
                    Some(format!(
                        "{}: {error}. Using the default settings",
                        path.display()
                    )),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let written = path
                    .parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(path, TEMPLATE));
                let note = written
                    .err()
                    .map(|error| format!("{}: couldn't be made: {error}", path.display()));
                (Self::default(), note)
            }
            Err(error) => (
                Self::default(),
                Some(format!(
                    "{}: {error}. Using the default settings",
                    path.display()
                )),
            ),
        }
    }

    /// The folders to find kits in, with `~/` expanded.
    pub fn kit_folders(&self) -> Vec<PathBuf> {
        self.kits.iter().map(|folder| expand_home(folder)).collect()
    }

    /// The folders the sample browser lists, with `~/` expanded.
    pub fn sample_folders(&self) -> Vec<PathBuf> {
        self.samples
            .iter()
            .map(|folder| expand_home(folder))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-settings-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn the_template_reads_as_the_defaults() {
        assert_eq!(
            ron::from_str::<Settings>(TEMPLATE).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn a_missing_file_is_made_from_the_template() {
        let root = folder("missing");
        let path = root.join("vimtracker").join(SETTINGS_FILE);
        assert_eq!(Settings::load(&path), (Settings::default(), None));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TEMPLATE);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_list_left_out_keeps_its_default() {
        let settings: Settings = ron::from_str(r#"(kits: ["/kits", "~/more"])"#).unwrap();
        assert_eq!(settings.kits, ["/kits", "~/more"]);
        assert_eq!(settings.samples, Settings::default().samples);
    }

    #[test]
    fn a_file_that_wont_read_says_why_and_uses_the_defaults() {
        let folder = folder("bad");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(SETTINGS_FILE);
        std::fs::write(&path, r#"(kit: ["/kits"])"#).unwrap();
        let (settings, note) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        let note = note.unwrap();
        assert!(
            note.contains("settings.ron") && note.contains("kit"),
            "{note}"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_leading_tilde_is_the_home_folder() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(expand_home("~/Music/kits"), home.join("Music/kits"));
        assert_eq!(expand_home("/kits"), Path::new("/kits"));
        assert_eq!(expand_home("kits/~/x"), Path::new("kits/~/x"));
    }
}
