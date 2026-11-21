//! The program's settings for this machine, kept in `settings.ron` beside `audio.ron`: where its
//! kits, samples and presets are, and whether the line describing the setting under the cursor
//! shows. They belong to the machine rather than to a song. The file is the user's to edit, and
//! the program writes back only that last one, as `Space d` or the Audio page switches it
//! (decision 476), changing its one line so the rest stays as it was written.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The settings file's name, in the config folder.
pub(crate) const SETTINGS_FILE: &str = "settings.ron";

/// What a new settings file says: the defaults, with a note on each.
const TEMPLATE: &str = r#"// vimtracker's settings for this machine. A path can start with ~/ for your home folder.
(
    // Folders to find kits in, in order. `:kit 909` opens the first 909.ron found.
    kits: ["~/Music/vimtracker/kits"],
    // Folders the sample browser lists, after the project's own samples.
    samples: ["~/Music/vimtracker/samples"],
    // The folder presets are saved in and loaded from, with sounds and effects inside it.
    presets: "~/Music/vimtracker/presets",
    // Whether the line describing the setting under the cursor shows. Space d switches it.
    describe: false,
)
"#;

/// The config folder: `$XDG_CONFIG_HOME/vimtracker`, or `~/.config/vimtracker`.
pub(crate) fn config_folder() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("vimtracker"))
}

/// A path as it's written, with a leading `~/` meaning the home folder.
pub(crate) fn expand_home(text: &str) -> PathBuf {
    expand_from(text, home_folder().as_deref())
}

/// A path as it's written, with a leading `~/` meaning this home folder, if there is one.
pub(crate) fn expand_from(text: &str, home: Option<&Path>) -> PathBuf {
    match (text.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(text),
    }
}

/// The home folder, which `~` stands for.
pub(crate) fn home_folder() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Where kits, samples and presets are found, and whether the description line shows. Each list
/// is searched in order, and a folder that isn't there is passed over. Paths are kept as written,
/// so `~/` stays readable in the file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub kits: Vec<String>,
    pub samples: Vec<String>,
    /// The one folder presets are saved in and loaded from, made on the first save, or empty for
    /// none.
    pub presets: String,
    /// Whether the line describing the setting under the cursor shows, `Space d`: off until
    /// turned on, then as it was left, in every project.
    pub describe: bool,
}

impl Default for Settings {
    /// Kits, samples and presets in their own folders under `~/Music/vimtracker`, as the M8 and
    /// the Polyend Tracker keep theirs in folders on the card.
    fn default() -> Self {
        Self {
            kits: vec!["~/Music/vimtracker/kits".to_owned()],
            samples: vec!["~/Music/vimtracker/samples".to_owned()],
            presets: "~/Music/vimtracker/presets".to_owned(),
            describe: false,
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
            presets: String::new(),
            describe: false,
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

    /// Keep whether the description line shows in the file at `path`, changing its `describe`
    /// line or adding one, and leaving the rest as it's written there, even if it was edited since
    /// the program started. A file that's gone is made again from these settings; one that won't
    /// read is left alone, and the error says why.
    pub(crate) fn save_describe(&self, path: &Path) -> Result<(), String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut kept: Self = ron::from_str(&text).map_err(|error| {
                    format!("{}: {error}. Describe isn't kept", path.display())
                })?;
                kept.describe = self.describe;
                // Where the line can't be changed or added in place, the file is written again
                // from what it says, with the template's notes.
                with_describe(&text, self.describe)
                    .filter(|changed| ron::from_str::<Self>(changed).ok().as_ref() == Some(&kept))
                    .unwrap_or_else(|| kept.written())
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => self.written(),
            Err(error) => {
                return Err(format!("{}: {error}. Describe isn't kept", path.display()));
            }
        };
        vt_model::ron_file::write_atomic(path, text)
            .map_err(|error| format!("{}: couldn't be saved: {error}", path.display()))
    }

    /// The settings as a new file says them, with the template's notes.
    fn written(&self) -> String {
        TEMPLATE
            .replace(r#"["~/Music/vimtracker/kits"]"#, &text(&self.kits))
            .replace(r#"["~/Music/vimtracker/samples"]"#, &text(&self.samples))
            .replace(r#""~/Music/vimtracker/presets""#, &text(&self.presets))
            .replace("describe: false", &format!("describe: {}", self.describe))
    }

    /// The folders to find kits in, with `~/` expanded.
    pub fn kit_folders(&self) -> Vec<PathBuf> {
        self.kits.iter().map(|folder| expand_home(folder)).collect()
    }

    /// The presets folder, with `~/` expanded, if there's one.
    pub(crate) fn preset_folder(&self) -> Option<PathBuf> {
        (!self.presets.is_empty()).then(|| expand_home(&self.presets))
    }

    /// The folders the sample browser lists, with `~/` expanded.
    pub(crate) fn sample_folders(&self) -> Vec<PathBuf> {
        self.samples
            .iter()
            .map(|folder| expand_home(folder))
            .collect()
    }
}

/// A value as the settings file writes it.
fn text<T: Serialize + ?Sized>(value: &T) -> String {
    ron::to_string(value).unwrap_or_default()
}

/// A settings file's text with its `describe` line saying `on`, or with one added before its
/// closing bracket where it has none, if the bracket has a line of its own.
fn with_describe(text: &str, on: bool) -> Option<String> {
    let line = format!("describe: {on},");
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|each| {
            let trimmed = each.trim_start();
            if !found && trimmed.starts_with("describe:") {
                found = true;
                format!("{}{line}", &each[..each.len() - trimmed.len()])
            } else {
                each.to_owned()
            }
        })
        .collect();
    if !found {
        let close = lines.iter().rposition(|each| each.trim() == ")")?;
        lines.insert(close, format!("    {line}"));
    }
    Some(lines.join("\n") + "\n")
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
        assert_eq!(Settings::default().written(), TEMPLATE);
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
        assert_eq!(settings.presets, Settings::default().presets);
        assert!(!settings.describe);
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

    fn describing() -> Settings {
        Settings {
            describe: true,
            ..Settings::none()
        }
    }

    #[test]
    fn describe_changes_its_one_line_and_keeps_the_rest_as_written() {
        let folder = folder("describe");
        let path = folder.join(SETTINGS_FILE);
        std::fs::create_dir_all(&folder).unwrap();
        let mine = TEMPLATE.replace("    kits: [", "    // Mine first.\n    kits: [\"/mine\", ");
        std::fs::write(&path, &mine).unwrap();
        describing().save_describe(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, mine.replace("describe: false", "describe: true"));
        let (read, note) = Settings::load(&path);
        assert_eq!(note, None);
        assert!(read.describe);
        assert_eq!(read.kits[0], "/mine");
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_file_without_describe_has_it_added() {
        let folder = folder("added");
        let path = folder.join(SETTINGS_FILE);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&path, "(\n    kits: [\"/kits\"],\n)\n").unwrap();
        describing().save_describe(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "(\n    kits: [\"/kits\"],\n    describe: true,\n)\n"
        );
        // On one line, it's written again from what it says.
        std::fs::write(&path, r#"(kits: ["/kits"])"#).unwrap();
        describing().save_describe(&path).unwrap();
        let (read, note) = Settings::load(&path);
        assert_eq!(note, None);
        assert!(read.describe);
        assert_eq!(read.kits, ["/kits"]);
        assert_eq!(read.samples, Settings::default().samples);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_file_that_wont_read_isnt_written_over() {
        let folder = folder("unread");
        let path = folder.join(SETTINGS_FILE);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(&path, r#"(kit: ["/kits"])"#).unwrap();
        let error = describing().save_describe(&path).unwrap_err();
        assert!(error.contains("settings.ron"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"(kit: ["/kits"])"#
        );
        // And one that's gone is made again.
        std::fs::remove_file(&path).unwrap();
        Settings::default().save_describe(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), TEMPLATE);
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
