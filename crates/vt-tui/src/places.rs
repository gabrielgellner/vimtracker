//! The folders the explorer was last used in, which it lists first the next time, for each kind of
//! thing it chooses. They're kept in `places.ron` beside `history.ron`, for this machine rather
//! than for a song.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::explorer::Purpose;

/// The file's name, in the config folder.
pub const PLACES_FILE: &str = "places.ron";
/// How many are kept of each kind, the oldest going first.
pub const MAX_PLACES: usize = 3;

/// The folders, newest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Lists {
    projects: Vec<PathBuf>,
    samples: Vec<PathBuf>,
    kits: Vec<PathBuf>,
    renders: Vec<PathBuf>,
}

/// The folders used recently, and where they're kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecentPlaces {
    lists: Lists,
    /// The file they're saved to, or none to keep them for the session only, as tests do.
    path: Option<PathBuf>,
}

impl RecentPlaces {
    /// Where they're kept, if there's a config folder.
    pub fn default_path() -> Option<PathBuf> {
        crate::config::config_folder().map(|folder| folder.join(PLACES_FILE))
    }

    /// The places kept at `path`, saved there again as they change. No file starts with none,
    /// and a file that won't read starts with none and says why.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let (lists, note) = match std::fs::read_to_string(path) {
            Ok(text) => match ron::from_str::<Lists>(&text) {
                Ok(lists) => (lists, None),
                Err(error) => (
                    Lists::default(),
                    Some(format!("{}: {error}", path.display())),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Lists::default(), None),
            Err(error) => (
                Lists::default(),
                Some(format!("{}: {error}", path.display())),
            ),
        };
        let places = Self {
            lists,
            path: Some(path.to_path_buf()),
        };
        (places, note)
    }

    fn list(&self, purpose: Purpose) -> &Vec<PathBuf> {
        match purpose {
            Purpose::Project | Purpose::Save => &self.lists.projects,
            Purpose::Sample => &self.lists.samples,
            Purpose::Kit => &self.lists.kits,
            Purpose::Render => &self.lists.renders,
        }
    }

    fn list_mut(&mut self, purpose: Purpose) -> &mut Vec<PathBuf> {
        match purpose {
            Purpose::Project | Purpose::Save => &mut self.lists.projects,
            Purpose::Sample => &mut self.lists.samples,
            Purpose::Kit => &mut self.lists.kits,
            Purpose::Render => &mut self.lists.renders,
        }
    }

    /// The folders used for a purpose, newest first.
    pub fn of(&self, purpose: Purpose) -> &[PathBuf] {
        self.list(purpose)
    }

    /// A folder something was chosen in. One used before moves to the front, and it's saved, if
    /// there's a file.
    pub fn add(&mut self, purpose: Purpose, folder: &Path) -> std::io::Result<()> {
        let list = self.list_mut(purpose);
        list.retain(|kept| kept != folder);
        list.insert(0, folder.to_path_buf());
        list.truncate(MAX_PLACES);
        self.save()
    }

    fn save(&self) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = ron::ser::to_string_pretty(&self.lists, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_newest_comes_first_and_only_a_few_are_kept() {
        let mut places = RecentPlaces::default();
        for folder in ["/a", "/b", "/c", "/a", "/d"] {
            places.add(Purpose::Sample, Path::new(folder)).unwrap();
        }
        let kept: Vec<&Path> = places
            .of(Purpose::Sample)
            .iter()
            .map(PathBuf::as_path)
            .collect();
        assert_eq!(kept, [Path::new("/d"), Path::new("/a"), Path::new("/c")]);
        assert!(places.of(Purpose::Kit).is_empty(), "each kind its own");
        places.add(Purpose::Save, Path::new("/songs")).unwrap();
        assert_eq!(
            places.of(Purpose::Project),
            [PathBuf::from("/songs")],
            "projects are opened and saved in the same places"
        );
    }

    #[test]
    fn they_are_kept_in_a_file() {
        let folder = std::env::temp_dir().join(format!("vt-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join(PLACES_FILE);
        let (mut places, note) = RecentPlaces::load(&path);
        assert_eq!(note, None, "no file is none");
        places.add(Purpose::Render, Path::new("/renders")).unwrap();
        let (again, _) = RecentPlaces::load(&path);
        assert_eq!(again.of(Purpose::Render), [PathBuf::from("/renders")]);
        std::fs::write(&path, "(nonsense: [])").unwrap();
        let (bad, note) = RecentPlaces::load(&path);
        assert!(bad.of(Purpose::Render).is_empty());
        assert!(note.unwrap().contains("places.ron"));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
