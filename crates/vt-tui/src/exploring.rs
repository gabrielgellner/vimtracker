//! Opening the file explorer, what it previews, the file it plays as its cursor moves, and the
//! name line that saving and rendering go on to.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use vt_engine::link::Command;
use vt_model::bank::BankEntry;
use vt_model::names::numbered;
use vt_model::project;

use crate::app::{App, UNSAVED_OPEN};
use crate::config::home_folder;
use crate::explorer::{Purpose, place_name};
use crate::input::command;
use crate::input::mode::Mode;
use crate::input::picker::{Item, Pick, Picker, Preview, Root, Shown};
use crate::loading::Read;
use crate::views::status::format_bpm;

/// What a folder chosen in the explorer goes on to: a command line, waiting for the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub purpose: Purpose,
    /// The command, such as `w`.
    pub command: &'static str,
    /// What follows the name, such as `:render`'s flags.
    pub after: String,
}

/// How long ago something was, roughly, as a file browser says it.
pub fn ago(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    let (count, unit) = match minutes {
        0 => return "just now".to_owned(),
        1..60 => (minutes, "minute"),
        60..1440 => (minutes / 60, "hour"),
        1440..10_080 => (minutes / 1440, "day"),
        10_080..43_200 => (minutes / 10_080, "week"),
        43_200..525_600 => (minutes / 43_200, "month"),
        _ => (minutes / 525_600, "year"),
    };
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {unit}{plural} ago")
}

/// A sample's length, rate and channels, as the preview gives them: `1.25 s  44.1 kHz  stereo`.
pub fn sample_note(info: &vt_model::sample::SampleInfo) -> String {
    let seconds = info.seconds();
    let length = if seconds < 10.0 {
        format!("{seconds:.2} s")
    } else if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let whole = seconds.round() as u64;
        format!("{}:{:02}", whole / 60, whole % 60)
    };
    let rate = format!("{:.1}", f64::from(info.rate) / 1000.0);
    let rate = rate.strip_suffix(".0").unwrap_or(&rate);
    let channels = match info.channels {
        1 => "mono".to_owned(),
        2 => "stereo".to_owned(),
        count => format!("{count} channels"),
    };
    format!("{length}  {rate} kHz  {channels}")
}

/// A file's name, or the whole path if it has none.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The names in a folder, without their extensions and in lower case, which a new name mustn't
/// be. A folder that can't be read has none.
pub fn taken_in(folder: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            let stem = path
                .file_stem()
                .unwrap_or(entry.file_name().as_os_str())
                .to_owned();
            stem.to_string_lossy().to_lowercase()
        })
        .collect()
}

/// Whether `:render`'s flags ask for stems, into a folder rather than a file.
fn is_stems(flags: &str) -> bool {
    flags.contains("--stems") || flags.contains("--split")
}

/// The commands whose line ends in a name, which `C-r` can roll.
const NAMED_BY: [&str; 4] = ["w", "new", "new!", "render"];

/// A name line with a new name from `name_in`, given the folder as typed, in place of the last
/// part of its path. The folder, an extension and `:render`'s flags are kept, and a render with no
/// name yet gets `.wav`, or nothing for `--stems`. Any other line gives nothing.
pub fn rolled_line(line: &str, name_in: impl FnOnce(&str) -> String) -> Option<String> {
    let (command, argument) = command::split(line);
    if !NAMED_BY.contains(&command) {
        return None;
    }
    // `:render`'s flags come after its path.
    let flags_at = if command == "render" {
        argument
            .match_indices("--")
            .map(|(at, _)| at)
            .find(|at| *at == 0 || argument[..*at].ends_with(char::is_whitespace))
            .unwrap_or(argument.len())
    } else {
        argument.len()
    };
    let (path, flags) = argument.split_at(flags_at);
    let (path, flags) = (path.trim(), flags.trim());
    let (folder, last) = match path.rfind('/') {
        Some(at) => path.split_at(at + 1),
        None => ("", path),
    };
    let extension = match last.rfind('.') {
        Some(at) if at > 0 => &last[at..],
        _ if last.is_empty() && command == "render" && !is_stems(flags) => ".wav",
        _ => "",
    };
    let name = name_in(folder);
    let mut rolled = format!("{command} {folder}{name}{extension}");
    if !flags.is_empty() {
        rolled.push(' ');
        rolled.push_str(flags);
    }
    Some(rolled)
}

/// What a sample's preview shows: its name, then its length, rate and channels and its waveform
/// once it's been read, or why it couldn't be.
pub fn sample_preview(item: usize, path: &Path, read: Option<&Read>) -> Shown {
    let name = name_of(path);
    match read {
        None => Shown::new(item, name, "Loading"),
        Some(Err(error)) => Shown::new(item, name, error.clone()),
        Some(Ok(loaded)) => Shown {
            wave: Some(loaded.wave.clone()),
            ..Shown::new(item, name, sample_note(&loaded.info))
        },
    }
}

/// What a project's preview shows: its name, its tempo and tracks, and when it was saved.
pub fn project_preview(item: usize, folder: &Path) -> Shown {
    let mut name = name_of(folder);
    match project::load_song(folder) {
        Ok(song) => {
            if !song.name.is_empty() {
                name.clone_from(&song.name);
            }
            let tracks = match song.sound_tracks() {
                1 => "1 track".to_owned(),
                count => format!("{count} tracks"),
            };
            let note = format!("{} BPM  {tracks}", format_bpm(song.millibpm));
            let saved = std::fs::metadata(folder.join("song.ron"))
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                .map(|elapsed| format!("Saved {}", ago(elapsed)));
            Shown {
                details: saved.into_iter().collect(),
                ..Shown::new(item, name, note)
            }
        }
        Err(error) => Shown::new(item, name, error.to_string()),
    }
}

impl App {
    /// The folders the explorer starts from for a purpose, at the top of its list: the recent
    /// ones, then the project's, the settings' own, for samples the kit folders and the sample
    /// library too, where the program was run from and the home folder. Each is listed once, and
    /// one that isn't there is passed over.
    pub(crate) fn places(&self, purpose: Purpose) -> Vec<Root> {
        let mut found: Vec<(PathBuf, &str)> = self
            .recent_places
            .of(purpose)
            .iter()
            .map(|folder| (folder.clone(), "recent"))
            .collect();
        let project = self.folder.clone();
        match purpose {
            // The kits open like folders to their sounds, and the library is the one they play
            // from, which is large, so it comes last.
            Purpose::Sample => {
                found.extend(project.map(|folder| (folder, "project")));
                found.extend(
                    self.settings
                        .kit_folders()
                        .into_iter()
                        .map(|folder| (folder, "kits")),
                );
                found.extend(
                    self.settings
                        .sample_folders()
                        .into_iter()
                        .map(|folder| (folder, "samples")),
                );
                found.extend(self.library.clone().map(|folder| (folder, "library")));
            }
            Purpose::Kit => {
                found.extend(
                    self.settings
                        .kit_folders()
                        .into_iter()
                        .map(|folder| (folder, "kits")),
                );
                found.extend(project.map(|folder| (folder, "project")));
            }
            Purpose::Project | Purpose::Save => {
                let around = project.as_deref().and_then(Path::parent);
                found.extend(around.map(|folder| (folder.to_path_buf(), "projects")));
            }
            Purpose::Render => found.extend(project.map(|folder| (folder, "project"))),
        }
        found.extend(std::env::current_dir().ok().map(|folder| (folder, "here")));
        found.extend(home_folder().map(|folder| (folder, "home")));
        let mut seen: Vec<PathBuf> = Vec::new();
        found
            .into_iter()
            .filter(|(folder, _)| folder.is_dir())
            .filter(|(folder, _)| {
                let real = folder.canonicalize().unwrap_or_else(|_| folder.clone());
                let new = !seen.contains(&real);
                seen.push(real);
                new
            })
            .map(|(path, meaning)| Root::Place {
                path,
                meaning: meaning.to_owned(),
            })
            .collect()
    }

    /// Open the explorer, with these at the top, on a file or folder if it's listed.
    pub(crate) fn open_explorer(&mut self, purpose: Purpose, roots: Vec<Root>, on: Option<&Path>) {
        self.count = None;
        // `H`, `P` and `A` are drawn after these, with their switches.
        let hint = match purpose {
            Purpose::Sample => "Enter choose",
            Purpose::Kit => "Enter use  L on tracks",
            Purpose::Project => "Enter open",
            Purpose::Save | Purpose::Render => "l open  Enter choose",
        };
        let mut picker = Picker::explorer(purpose, roots, home_folder())
            .following(on)
            .opening_first()
            .hinted(hint);
        if let Some(tree) = picker.tree_mut() {
            tree.audition &= self.audition_files;
        }
        self.show_picker(picker);
    }

    /// `:e` with no folder: choose a project to open, beside this one. Forced, as `:e!`, the
    /// project chosen throws away unsaved changes.
    pub(crate) fn open_project_explorer(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = UNSAVED_OPEN.to_owned();
            return;
        }
        self.opening_discards = force;
        let roots = self.places(Purpose::Project);
        let on = self.folder.clone();
        self.open_explorer(Purpose::Project, roots, on.as_deref());
    }

    /// Choose a folder, then type a name on the command line, which starts with this command.
    pub(crate) fn open_naming_explorer(
        &mut self,
        purpose: Purpose,
        command: &'static str,
        after: String,
    ) {
        self.naming = Some(Naming {
            purpose,
            command,
            after,
        });
        let roots = self.places(purpose);
        // A project is saved beside this one, and a render goes in it.
        let on = match purpose {
            Purpose::Save => self.folder.as_deref().and_then(Path::parent),
            _ => self.folder.as_deref(),
        }
        .map(Path::to_path_buf);
        self.open_explorer(purpose, roots, on.as_deref());
    }

    /// The name a name line starts with in a folder. Saving a project somewhere new starts with
    /// its name, and a render with the project's name, either with a number after it if that's
    /// taken there, so nothing is written over; `:new` starts with a new random name, rolled again
    /// if it's taken.
    pub(crate) fn suggested_name(&mut self, naming: &Naming, folder: &Path) -> String {
        let taken = taken_in(folder);
        let is_taken = |name: &str| taken.contains(&name.to_lowercase());
        match naming.purpose {
            Purpose::Render => {
                let stem = numbered(&self.project_name(), is_taken);
                if is_stems(&naming.after) {
                    stem
                } else {
                    format!("{stem}.wav")
                }
            }
            // Saving keeps the project's name, numbered if the folder has it, as a copy is.
            _ if naming.command == "w" => numbered(&self.project_name(), is_taken),
            _ => self.names.fresh(is_taken),
        }
    }

    /// The project's name, which is the song's own: the random one it was given when it was
    /// made, its folder's when it's saved somewhere new, or what the project page names it.
    pub fn project_name(&self) -> String {
        self.song().name.clone()
    }

    /// `C-r` on a name line: put a new random name in place of the one there, keeping the folder,
    /// the extension and any flags, and not one already in that folder.
    pub(crate) fn roll_name(&mut self) {
        let names = &mut self.names;
        let rolled = rolled_line(&self.command_line, |folder| {
            let folder = if folder.is_empty() {
                PathBuf::from(".")
            } else {
                crate::config::expand_home(folder)
            };
            let taken = taken_in(&folder);
            names.fresh(|name| taken.contains(name))
        });
        if let Some(line) = rolled {
            self.command_line = line;
            self.command_menu = None;
            self.command_history.stop();
        }
    }

    /// Choose a kit, from the kit folders, on the kit in use.
    pub(crate) fn open_kit_explorer(&mut self) {
        let roots = self.places(Purpose::Kit);
        let on = self.kit_file.clone();
        self.open_explorer(Purpose::Kit, roots, on.as_deref());
    }

    /// Put the command line on a chosen folder, waiting for the name, with Tab to complete it.
    pub(crate) fn name_in(&mut self, folder: &Path) {
        let Some(naming) = self.naming.take() else {
            return;
        };
        self.note_place(naming.purpose, folder);
        let name = self.suggested_name(&naming, folder);
        let home = home_folder();
        let folder = place_name(folder, home.as_deref());
        let folder = folder.trim_end_matches('/');
        self.command_line = format!("{} {folder}/{name}{}", naming.command, naming.after);
        self.command_menu = None;
        self.mode = Mode::Command;
        self.status = "Type a name, or C-r for another, then Enter".to_owned();
    }

    /// Remember a folder something was chosen in, for the top of the explorer next time.
    pub(crate) fn note_place(&mut self, purpose: Purpose, folder: &Path) {
        if let Err(error) = self.recent_places.add(purpose, folder) {
            self.status = format!("Couldn't keep the recent places: {error}");
        }
    }

    /// What the explorer's preview shows for an item, from what's been read.
    pub(crate) fn explorer_preview(&mut self, item: usize, preview: &Preview) -> Option<Shown> {
        match preview {
            Preview::Sample(path) => {
                let read = self.loader.want(path);
                if let Some(Err(_)) = read {
                    self.mark_unreadable(path);
                }
                Some(sample_preview(item, path, read.as_ref()))
            }
            Preview::Project(folder) => Some(project_preview(item, folder)),
            Preview::Kit(_) => None,
        }
    }

    /// Take in files read in the background: show the one under the explorer's cursor, and play
    /// it.
    pub(crate) fn take_loaded(&mut self) {
        let done = self.loader.poll();
        self.show_loaded(&done);
    }

    /// Wait for the file under the explorer's cursor to be read, then take it in, for tests and
    /// the screenshots.
    pub fn finish_loading(&mut self) {
        let done = self.loader.wait(crate::loading::WAIT);
        self.show_loaded(&done);
    }

    fn show_loaded(&mut self, done: &[PathBuf]) {
        let shown = self.picker.as_ref().and_then(|picker| {
            let item = picker.preview.as_ref()?.item;
            match &picker.item(item).preview {
                Some(Preview::Sample(path)) => Some(path.clone()),
                _ => None,
            }
        });
        if let Some(path) = shown
            && done.contains(&path)
            && let Some(picker) = self.picker.as_mut()
        {
            picker.preview = None;
            self.refresh_preview();
        }
        if !done.is_empty() {
            self.follow_file();
        }
    }

    /// The sample under the explorer's cursor, while it plays what it moves onto.
    fn file_to_play(&mut self) -> Option<PathBuf> {
        let picker = self.picker.as_ref()?;
        let tree = picker.tree()?;
        if !tree.purpose.auditions() {
            return None;
        }
        self.audition_files = tree.audition;
        if !tree.audition {
            return None;
        }
        match &picker.current()?.pick {
            Pick::Sample(path) | Pick::KitSound { path, .. } => Some(path.clone()),
            _ => None,
        }
    }

    /// Play the sample under the explorer's cursor on the current track, as the M8's and
    /// Elektron's browsers do, once it's been read, and stop the last one when the cursor moves.
    pub(crate) fn follow_file(&mut self) {
        let wanted = self.file_to_play();
        if wanted != self.file_wanted {
            self.stop_file();
            self.file_wanted = wanted;
        }
        let Some(path) = self.file_wanted.clone() else {
            return;
        };
        if self.file_playing.is_some() {
            return;
        }
        match self.loader.want(&path) {
            Some(Ok(loaded)) => {
                let track = self.cursor.track;
                if self
                    .handle
                    .play_file(track, Arc::clone(&loaded.sample), None)
                {
                    self.file_playing = Some(path);
                    // It takes the place of anything the sample page was playing.
                    self.sample_page.heard = None;
                }
            }
            // It won't play, and the preview says why.
            Some(Err(_)) => {
                self.mark_unreadable(&path);
                self.file_playing = Some(path);
            }
            None => {}
        }
    }

    /// Show a sample that couldn't be read with a warning in the explorer.
    fn mark_unreadable(&mut self, path: &Path) {
        if let Some(picker) = self.picker.as_mut() {
            picker.mark_unreadable(path);
        }
    }

    /// Stop the file the explorer is playing, if it is.
    pub(crate) fn stop_file(&mut self) {
        if self.file_playing.take().is_some() {
            self.handle.send(Command::StopFile);
        }
    }

    /// The explorer has closed: stop what it plays, and forget any name line it was for.
    pub(crate) fn leave_explorer(&mut self) {
        self.stop_file();
        self.file_wanted = None;
        self.naming = None;
    }

    /// Choose this track's sample, from the kit's sounds and the usual places, on the track's own
    /// sample. With no sample, it's on the kit's first sound.
    pub(crate) fn open_sample_explorer(&mut self, kit: Vec<Item>, kit_name: String) {
        let song = self.song();
        let sample = song
            .tracks
            .get(self.cursor.track)
            .and_then(|track| song.bank.get(track.sample()))
            .and_then(BankEntry::source)
            .map(|source| project::resolve_sample(self.folder.as_deref(), &source.path));
        let same = |path: &Path| {
            sample.as_deref().is_some_and(|sample| {
                path.file_name() == sample.file_name()
                    && (path == sample
                        || path
                            .canonicalize()
                            .is_ok_and(|path| sample.canonicalize().is_ok_and(|s| s == path)))
            })
        };
        // The kit's sounds come after their group's heading.
        let in_kit = kit
            .iter()
            .position(|item| matches!(&item.pick, Pick::KitSound { path, .. } if same(path)))
            .map(|at| at + 1);
        let first_sound = (!kit.is_empty()).then_some(1);
        let mut roots = Vec::new();
        if !kit.is_empty() {
            roots.push(Root::Group {
                label: kit_name,
                items: kit,
            });
        }
        roots.extend(self.places(Purpose::Sample));
        self.open_explorer(Purpose::Sample, roots, None);
        if let Some(picker) = self.picker.take() {
            let picker = match in_kit {
                Some(at) => picker.opened_on(Some(at)),
                None if sample.is_some() => picker.following(sample.as_deref()),
                None => picker.revealing(first_sound),
            };
            self.show_picker(picker);
        }
    }

    /// Choose a sample from one folder, open.
    pub(crate) fn open_sample_folder(&mut self, folder: &Path) {
        let roots = vec![Root::Place {
            path: folder.to_path_buf(),
            meaning: String::new(),
        }];
        self.open_explorer(Purpose::Sample, roots, None);
        if let Some(picker) = self.picker.take() {
            self.show_picker(picker.opening_roots());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::sample::SampleInfo;

    #[test]
    fn rolling_a_name_keeps_the_folder_extension_and_flags() {
        let roll = |line: &str| {
            let mut folder_seen = String::new();
            let rolled = rolled_line(line, |folder| {
                folder_seen = folder.to_owned();
                "dusty-breakdown".to_owned()
            });
            rolled.map(|rolled| (rolled, folder_seen))
        };
        let rolled = |line: &str, folder: &str| Some((line.to_owned(), folder.to_owned()));
        assert_eq!(
            roll("w ~/songs/typed"),
            rolled("w ~/songs/dusty-breakdown", "~/songs/")
        );
        assert_eq!(
            roll("w ~/songs/"),
            rolled("w ~/songs/dusty-breakdown", "~/songs/")
        );
        assert_eq!(roll("w"), rolled("w dusty-breakdown", ""));
        assert_eq!(roll("new a/b"), rolled("new a/dusty-breakdown", "a/"));
        assert_eq!(
            roll("render ~/one/one.wav --rows 16 --tail"),
            rolled(
                "render ~/one/dusty-breakdown.wav --rows 16 --tail",
                "~/one/"
            )
        );
        assert_eq!(
            roll("render --tail"),
            rolled("render dusty-breakdown.wav --tail", "")
        );
        assert_eq!(
            roll("render out/ --stems"),
            rolled("render out/dusty-breakdown --stems", "out/")
        );
        assert_eq!(roll("bpm 120"), None, "only a name line has a name");
        assert_eq!(roll("e ~/songs/one"), None, "opening names nothing new");
    }

    #[test]
    fn a_folders_names_are_taken_without_their_extensions() {
        let folder = std::env::temp_dir().join(format!("vt-taken-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("Quiet-Harbor")).unwrap();
        std::fs::write(folder.join("dusty-breakdown.wav"), "").unwrap();
        let taken = taken_in(&folder);
        assert!(taken.contains("quiet-harbor"), "{taken:?}");
        assert!(taken.contains("dusty-breakdown"), "{taken:?}");
        assert_eq!(taken.len(), 2);
        assert!(taken_in(&folder.join("missing")).is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn times_are_rough() {
        let minutes = |count: u64| Duration::from_secs(count * 60);
        assert_eq!(ago(Duration::from_secs(20)), "just now");
        assert_eq!(ago(minutes(1)), "1 minute ago");
        assert_eq!(ago(minutes(150)), "2 hours ago");
        assert_eq!(ago(minutes(60 * 24 * 3)), "3 days ago");
        assert_eq!(ago(minutes(60 * 24 * 400)), "1 year ago");
    }

    #[test]
    fn a_sample_is_described_by_its_length_rate_and_channels() {
        let info = |rate: u32, channels: u16, seconds: f64| SampleInfo {
            rate,
            channels,
            frames: (seconds * f64::from(rate)) as usize,
        };
        assert_eq!(
            sample_note(&info(44_100, 2, 1.25)),
            "1.25 s  44.1 kHz  stereo"
        );
        assert_eq!(sample_note(&info(48_000, 1, 12.5)), "12.5 s  48 kHz  mono");
        assert_eq!(sample_note(&info(48_000, 2, 125.0)), "2:05  48 kHz  stereo");
    }
}
