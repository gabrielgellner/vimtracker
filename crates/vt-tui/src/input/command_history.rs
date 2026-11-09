//! The command lines run before, which `Up` and `Down` go back through on the `:` line, as Vim's
//! history does. They're kept for the session and in `history.ron` beside `settings.ron`, for this
//! machine rather than for a song.

use std::path::{Path, PathBuf};

/// The history file's name, in the config folder.
pub const HISTORY_FILE: &str = "history.ron";
/// How many lines are kept, the oldest going first.
pub const MAX_LINES: usize = 100;

/// Where `Up` has got to: the line it's on, and what was typed before it started, which it
/// searches for and `Down` comes back to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Browsing {
    at: usize,
    typed: String,
}

/// The lines run, oldest first, and where they're kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandHistory {
    lines: Vec<String>,
    browsing: Option<Browsing>,
    /// The file they're saved to, or none to keep them for the session only, as tests do.
    path: Option<PathBuf>,
}

impl CommandHistory {
    /// A history of these lines, oldest first, kept for the session only.
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }

    /// Where it's kept, if there's a config folder.
    pub fn default_path() -> Option<PathBuf> {
        crate::config::config_folder().map(|folder| folder.join(HISTORY_FILE))
    }

    /// The history kept at `path`, saved there again as lines are added. No file starts an empty
    /// one, and a file that won't read starts an empty one and says why.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let (lines, note) = match std::fs::read_to_string(path) {
            Ok(text) => match ron::from_str::<Vec<String>>(&text) {
                Ok(lines) => (lines, None),
                Err(error) => (Vec::new(), Some(format!("{}: {error}", path.display()))),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
            Err(error) => (Vec::new(), Some(format!("{}: {error}", path.display()))),
        };
        let mut history = Self::new(lines);
        history.trim();
        history.path = Some(path.to_path_buf());
        (history, note)
    }

    /// Write it to its file, if it has one.
    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = ron::ser::to_string_pretty(&self.lines, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// The lines, oldest first.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Add a line that was run. One run before moves to the end rather than being kept twice, and
    /// the oldest go past [`MAX_LINES`]. It's saved, if it has a file.
    pub fn add(&mut self, line: &str) -> std::io::Result<()> {
        self.browsing = None;
        let line = line.trim();
        if line.is_empty() {
            return Ok(());
        }
        self.lines.retain(|earlier| earlier != line);
        self.lines.push(line.to_owned());
        self.trim();
        self.save()
    }

    fn trim(&mut self) {
        let over = self.lines.len().saturating_sub(MAX_LINES);
        self.lines.drain(..over);
    }

    /// `Up`: the line before the one shown that starts with what was typed before going back, or
    /// none if there's no earlier one, which leaves the line as it is.
    pub fn older(&mut self, typed: &str) -> Option<String> {
        let browsing = self.browsing.get_or_insert_with(|| Browsing {
            at: self.lines.len(),
            typed: typed.to_owned(),
        });
        let at = self.lines[..browsing.at]
            .iter()
            .rposition(|line| line.starts_with(&browsing.typed))?;
        browsing.at = at;
        Some(self.lines[at].clone())
    }

    /// `Down`: the line after the one shown that starts with what was typed, or what was typed
    /// once past the newest. None while not going back.
    pub fn newer(&mut self) -> Option<String> {
        let browsing = self.browsing.as_mut()?;
        let after = browsing.at + 1;
        let found = self.lines[after..]
            .iter()
            .position(|line| line.starts_with(&browsing.typed));
        match found {
            Some(found) => {
                browsing.at = after + found;
                Some(self.lines[browsing.at].clone())
            }
            None => self.browsing.take().map(|browsing| browsing.typed),
        }
    }

    /// Stop going back, as typing does, so the next `Up` searches for what's typed then.
    pub fn stop(&mut self) {
        self.browsing = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(lines: &[&str]) -> CommandHistory {
        CommandHistory::new(lines.iter().map(|line| (*line).to_owned()).collect())
    }

    #[test]
    fn up_goes_back_and_down_comes_forward_to_what_was_typed() {
        let mut history = history(&["w", "set skip=4", "bpm 120"]);
        assert_eq!(history.older("").as_deref(), Some("bpm 120"));
        assert_eq!(history.older("").as_deref(), Some("set skip=4"));
        assert_eq!(history.older("").as_deref(), Some("w"));
        assert_eq!(history.older(""), None, "nothing older");
        assert_eq!(history.newer().as_deref(), Some("set skip=4"));
        assert_eq!(history.newer().as_deref(), Some("bpm 120"));
        assert_eq!(
            history.newer().as_deref(),
            Some(""),
            "back to what was typed"
        );
        assert_eq!(history.newer(), None);
    }

    #[test]
    fn up_with_something_typed_finds_lines_starting_with_it() {
        let mut history = history(&["set skip=4", "bpm 120", "set root=d", "w"]);
        assert_eq!(history.older("se").as_deref(), Some("set root=d"));
        // What's shown now isn't searched for, what was typed is.
        assert_eq!(history.older("set root=d").as_deref(), Some("set skip=4"));
        assert_eq!(history.older("set skip=4"), None);
        assert_eq!(history.newer().as_deref(), Some("set root=d"));
        assert_eq!(history.newer().as_deref(), Some("se"));
        history.stop();
        assert_eq!(history.older("b").as_deref(), Some("bpm 120"));
        assert_eq!(history.older("zz"), None, "still searching for b");
        history.stop();
        assert_eq!(history.older("zz"), None);
    }

    #[test]
    fn a_line_run_again_moves_to_the_end_and_the_oldest_go() {
        let mut history = history(&["w", "bpm 120"]);
        history.add(" w ").unwrap();
        history.add("").unwrap();
        assert_eq!(history.lines(), ["bpm 120", "w"]);
        for at in 0..MAX_LINES + 5 {
            history.add(&format!("seed {at}")).unwrap();
        }
        assert_eq!(history.lines().len(), MAX_LINES);
        assert_eq!(history.lines()[0], "seed 5");
    }

    #[test]
    fn it_saves_and_loads_its_file() {
        let folder = std::env::temp_dir().join(format!("vt-history-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join("vimtracker").join(HISTORY_FILE);
        let (mut history, note) = CommandHistory::load(&path);
        assert_eq!(
            (history.lines().len(), note),
            (0, None),
            "no file is an empty history"
        );
        history.add("set skip=4").unwrap();
        history.add("w").unwrap();
        let (loaded, note) = CommandHistory::load(&path);
        assert_eq!(note, None);
        assert_eq!(loaded.lines(), ["set skip=4", "w"]);
        std::fs::write(&path, "not ron").unwrap();
        let (broken, note) = CommandHistory::load(&path);
        assert!(broken.lines().is_empty());
        assert!(note.unwrap().contains(HISTORY_FILE));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
