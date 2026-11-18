//! Opening the file explorer, what it previews, the file it plays as its cursor moves, and the
//! name line that saving and rendering go on to.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use vt_engine::link::Command;
use vt_model::bank::BankEntry;
use vt_model::kit::{KitEntry, load_kit};
use vt_model::names::numbered;
use vt_model::project;
use vt_model::sample::load_sample;

use crate::app::App;
use crate::graphics;
use crate::input::mode::Mode;
use crate::overlays::Overlays;
use crate::overlays::command::parse;
use crate::overlays::picker::draw::{
    PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH, PREVIEW_WAVE_HEIGHT, PREVIEW_WAVE_WIDTH,
};
use crate::overlays::picker::explorer::{Purpose, kit_item, place_name};
use crate::overlays::picker::places::RecentPlaces;
use crate::overlays::picker::{Item, KitChoice, Pick, Picker, Preview, PreviewLine, Root, Shown};
use crate::screen::status::format_bpm;
use crate::services::Services;
use crate::services::config::home_folder;
use crate::services::loading::{Loader, Read};

/// What a folder chosen in the explorer goes on to: a command line, waiting for the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Naming {
    pub purpose: Purpose,
    /// The command, such as `w`.
    pub command: &'static str,
    /// What follows the name, such as `:render`'s flags.
    pub after: String,
}

/// The file explorer's state between its openings: the kit in use, the reader of the files it
/// previews and plays, the places and tags it remembers, and what a folder chosen in it goes on to.
pub struct Explorer {
    /// The sample library the sample explorer lists, such as Ableton Live's, if this machine has
    /// one: found as the program starts, and none in tests.
    pub library: Option<PathBuf>,
    /// The sounds of the kit in use, to choose from for a track.
    pub kit: Vec<KitEntry>,
    /// The file of the kit in use, which the kit picker opens on.
    pub kit_file: Option<PathBuf>,
    /// Reads the files the explorer previews and plays, off this thread.
    pub loader: Loader,
    /// Whether the explorer plays the sample its cursor moves onto, which `A` switches and the
    /// next one opened keeps.
    pub audition_files: bool,
    /// The sample the explorer's cursor is on, to play once it's read, and the one playing.
    pub file_wanted: Option<PathBuf>,
    pub file_playing: Option<PathBuf>,
    /// The folders the explorer was last used in.
    pub recent_places: RecentPlaces,
    /// The tags given to files by hand, by their content.
    pub tag_library: Arc<vt_model::tag::TagLibrary>,
    /// What each sample read for the explorer is by how its sound ends, `loop` or `one-shot`,
    /// kept for the session.
    pub heard_tags: HashMap<PathBuf, &'static str>,
    /// The command line a folder chosen in the explorer goes on to.
    pub naming: Option<Naming>,
    /// Whether the project the explorer opens throws away unsaved changes, as after `:e!`.
    pub(crate) opening_discards: bool,
    /// Whether the patch the explorer adds goes over the last mod slots and macros where there
    /// are too few free, as after `:patch!`.
    pub(crate) patch_over: bool,
}

impl Explorer {
    /// Nothing remembered yet, and a reader for files at this rate.
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            library: None,
            kit: Vec::new(),
            kit_file: None,
            loader: Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT)),
            audition_files: true,
            file_wanted: None,
            file_playing: None,
            recent_places: RecentPlaces::default(),
            tag_library: Arc::default(),
            heard_tags: HashMap::new(),
            naming: None,
            opening_discards: false,
            patch_over: false,
        }
    }
}

/// How long ago something was, roughly, as a file browser says it.
pub(crate) fn ago(elapsed: Duration) -> String {
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
pub(crate) fn sample_note(info: &vt_model::sample::SampleInfo) -> String {
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
pub(crate) fn taken_in(folder: &Path) -> HashSet<String> {
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
pub(crate) fn rolled_line(line: &str, name_in: impl FnOnce(&str) -> String) -> Option<String> {
    let (command, argument) = parse::split(line);
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
pub(crate) fn sample_preview(item: usize, path: &Path, read: Option<&Read>) -> Shown {
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
pub(crate) fn project_preview(item: usize, folder: &Path) -> Shown {
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

/// What a kit's preview shows: its name, how many sounds it has and where it came from, and each
/// sound with its label, its name and its waveform. Waveforms already worked out are used again.
pub(crate) fn kit_preview(
    item: usize,
    path: &Path,
    rate: u32,
    waves: &mut HashMap<PathBuf, Option<Vec<(usize, usize)>>>,
) -> Shown {
    let kit = match load_kit(path) {
        Ok(kit) => kit,
        Err(error) => {
            let heading = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Shown::new(item, heading, error.to_string());
        }
    };
    let count = match kit.sounds.len() {
        1 => "1 sound".to_owned(),
        count => format!("{count} sounds"),
    };
    let note = if kit.source.is_empty() {
        count
    } else {
        format!("{count} from {}", kit.source)
    };
    let lines = kit
        .sounds
        .iter()
        .map(|sound| PreviewLine {
            label: sound.label.clone(),
            name: sound.name.clone(),
            wave: waves
                .entry(sound.path.clone())
                .or_insert_with(|| {
                    load_sample(&sound.path, rate).ok().map(|sample| {
                        graphics::waveform_spans(
                            &sample.frames,
                            PREVIEW_WAVE_WIDTH,
                            PREVIEW_WAVE_HEIGHT,
                        )
                    })
                })
                .clone(),
        })
        .collect();
    Shown {
        lines,
        ..Shown::new(item, kit.name, note)
    }
}

impl App {
    /// The folders the explorer starts from for a purpose, at the top of its list: the recent
    /// ones, then the project's, the settings' own, for samples the kit folders and the sample
    /// library too, and but for presets where the program was run from and the home folder. Each
    /// is listed once, and one that isn't there is passed over.
    pub(crate) fn places(&self, purpose: Purpose) -> Vec<Root> {
        let mut found: Vec<(PathBuf, &str)> = self
            .overlays
            .explorer
            .recent_places
            .of(purpose)
            .iter()
            .map(|folder| (folder.clone(), "recent"))
            .collect();
        let project = self.services.folder.clone();
        match purpose {
            // The kits open like folders to their sounds, and the library is the one they play
            // from, which is large, so it comes last.
            Purpose::Sample => {
                found.extend(project.map(|folder| (folder, "project")));
                found.extend(
                    self.services
                        .settings
                        .kit_folders()
                        .into_iter()
                        .map(|folder| (folder, "kits")),
                );
                found.extend(
                    self.services
                        .settings
                        .sample_folders()
                        .into_iter()
                        .map(|folder| (folder, "samples")),
                );
                found.extend(
                    self.overlays
                        .explorer
                        .library
                        .clone()
                        .map(|folder| (folder, "library")),
                );
            }
            Purpose::Kit => {
                found.extend(
                    self.services
                        .settings
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
            Purpose::Preset(kind) => found.extend(
                self.services
                    .settings
                    .preset_folder()
                    .map(|folder| (folder.join(kind.folder()), kind.folder())),
            ),
        }
        // Presets are kept in their own folder, so the browser lists only it and those used lately.
        if !matches!(purpose, Purpose::Preset(_)) {
            found.extend(std::env::current_dir().ok().map(|folder| (folder, "here")));
            found.extend(home_folder().map(|folder| (folder, "home")));
        }
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
        self.modes.count = None;
        // `H`, `P` and `A` are drawn after these, with their switches.
        let hint = match purpose {
            Purpose::Sample => "Enter choose  T tag",
            Purpose::Kit => "Enter use  L on tracks  T tag",
            Purpose::Project => "Enter open",
            Purpose::Save | Purpose::Render => "l open  Enter choose",
            Purpose::Preset(vt_model::preset::PresetKind::Patch)
                if self.overlays.explorer.patch_over =>
            {
                "Enter add over the last  T tag"
            }
            Purpose::Preset(vt_model::preset::PresetKind::Patch) => "Enter add  T tag",
            Purpose::Preset(_) => "Enter load  T tag",
        };
        let mut picker = Picker::explorer(purpose, roots, home_folder())
            .with_tags(std::sync::Arc::clone(&self.overlays.explorer.tag_library))
            .following(on)
            .opening_first()
            .hinted(hint);
        if let Some(tree) = picker.tree_mut() {
            tree.audition &= self.overlays.explorer.audition_files;
        }
        self.show_picker(picker);
    }

    /// `:e` with no folder: choose a project to open, beside this one. Forced, as `:e!`, the
    /// project chosen throws away unsaved changes.
    pub(crate) fn open_project_explorer(&mut self, force: bool) {
        if self.services.dirty && !force {
            self.services.status = self.unsaved(":e! to discard them");
            return;
        }
        self.overlays.explorer.opening_discards = force;
        let roots = self.places(Purpose::Project);
        let on = self.services.folder.clone();
        self.open_explorer(Purpose::Project, roots, on.as_deref());
    }

    /// Choose a folder, then type a name on the command line, which starts with this command.
    pub(crate) fn open_naming_explorer(
        &mut self,
        purpose: Purpose,
        command: &'static str,
        after: String,
    ) {
        self.overlays.explorer.naming = Some(Naming {
            purpose,
            command,
            after,
        });
        let roots = self.places(purpose);
        // A project is saved beside this one, and a render goes in it.
        let on = match purpose {
            Purpose::Save => self.services.folder.as_deref().and_then(Path::parent),
            _ => self.services.folder.as_deref(),
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
                let stem = numbered(&self.services.project_name(), is_taken);
                if is_stems(&naming.after) {
                    stem
                } else {
                    format!("{stem}.wav")
                }
            }
            // Saving keeps the project's name, numbered if the folder has it, as a copy is.
            _ if naming.command == "w" => numbered(&self.services.project_name(), is_taken),
            _ => self.services.names.fresh(is_taken),
        }
    }

    /// Choose a kit, from the kit folders, on the kit in use.
    pub(crate) fn open_kit_explorer(&mut self) {
        let roots = self.places(Purpose::Kit);
        let on = self.overlays.explorer.kit_file.clone();
        self.open_explorer(Purpose::Kit, roots, on.as_deref());
    }

    /// Make a kit's sounds the ones to choose from for a track. Nothing is loaded until one is.
    pub(crate) fn use_kit(&mut self, path: &Path) {
        match load_kit(path) {
            Ok(kit) if kit.sounds.is_empty() => {
                self.services.status = format!("{} has no sounds", path.display());
            }
            Ok(kit) => {
                self.services.status = format!(
                    "Kit {}: {} sounds. :sample chooses one for this track",
                    kit.name,
                    kit.sounds.len()
                );
                self.overlays.explorer.kit = kit.sounds;
                self.overlays.explorer.kit_file = Some(path.to_path_buf());
            }
            Err(error) => self.services.status = error.to_string(),
        }
    }

    /// Put the command line on a chosen folder, waiting for the name, with Tab to complete it.
    pub(crate) fn name_in(&mut self, folder: &Path) {
        let Some(naming) = self.overlays.explorer.naming.take() else {
            return;
        };
        self.note_place(naming.purpose, folder);
        let name = self.suggested_name(&naming, folder);
        let home = home_folder();
        let folder = place_name(folder, home.as_deref());
        let folder = folder.trim_end_matches('/');
        self.overlays.command.line = format!("{} {folder}/{name}{}", naming.command, naming.after);
        self.overlays.command.menu = None;
        self.modes.mode = Mode::Command;
        self.services.status = "Type a name, or Ctrl-r for another, then Enter".to_owned();
    }

    /// Remember a folder something was chosen in, for the top of the explorer next time.
    pub(crate) fn note_place(&mut self, purpose: Purpose, folder: &Path) {
        if let Err(error) = self.overlays.explorer.recent_places.add(purpose, folder) {
            self.services.status = format!("Couldn't keep the recent places: {error}");
        }
    }

    /// Take in files read in the background: show the one under the explorer's cursor, and play
    /// it.
    pub(crate) fn take_loaded(&mut self) {
        let done = self.overlays.explorer.loader.poll();
        self.show_loaded(&done);
    }

    /// Wait for the file under the explorer's cursor to be read, then take it in, for tests and
    /// the screenshots.
    pub fn finish_loading(&mut self) {
        let done = self
            .overlays
            .explorer
            .loader
            .wait(crate::services::loading::WAIT);
        self.show_loaded(&done);
    }

    fn show_loaded(&mut self, done: &[PathBuf]) {
        let shown = self.overlays.picker.as_ref().and_then(|picker| {
            let item = picker.preview.as_ref()?.item;
            match &picker.item(item).preview {
                Some(Preview::Sample(path)) => Some(path.clone()),
                _ => None,
            }
        });
        if let Some(path) = shown
            && done.contains(&path)
            && let Some(picker) = self.overlays.picker.as_mut()
        {
            picker.preview = None;
            self.overlays.refresh_preview(self.services.rate);
        }
        if !done.is_empty() {
            self.follow_file();
        }
    }

    /// Play the sample under the explorer's cursor on the current track, once it's been read,
    /// and stop the last one when the cursor moves. It takes the place of anything the sample
    /// page was playing.
    pub(crate) fn follow_file(&mut self) {
        let overlays = &mut self.overlays;
        if overlays
            .explorer
            .follow_file(overlays.picker.as_mut(), &mut self.services)
        {
            self.sample_page.heard = None;
        }
    }

    /// Choose this track's sample in the explorer, with the kit's sounds first.
    pub(crate) fn open_default_sample_picker(&mut self) {
        let kit_name = self
            .overlays
            .explorer
            .kit_file
            .as_deref()
            .and_then(Path::file_stem)
            .map(|stem| format!("Kit {}", stem.to_string_lossy()))
            .unwrap_or_else(|| "Kit".to_owned());
        let kit = self.overlays.explorer.kit.iter().map(kit_item).collect();
        self.open_sample_explorer(kit, kit_name);
    }

    /// Choose this track's sample, from the kit's sounds and the usual places, on the track's own
    /// sample. With no sample, it's on the kit's first sound.
    pub(crate) fn open_sample_explorer(&mut self, kit: Vec<Item>, kit_name: String) {
        let song = self.song();
        let sample = song
            .tracks
            .get(self.services.cursor.track)
            .and_then(|track| song.bank.get(track.sample()))
            .and_then(BankEntry::source)
            .map(|source| project::resolve_sample(self.services.folder.as_deref(), &source.path));
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
            .position(|item| matches!(&item.pick, Pick::KitSound(sound) if same(&sound.path)))
            .map(|at| at + 1);
        let first_sound = (!kit.is_empty()).then_some(1);
        let mut roots = Vec::new();
        if !kit.is_empty() {
            roots.push(Root::Group {
                label: kit_name,
                meaning: String::new(),
                icon: crate::overlays::picker::explorer::Icon::Kit,
                items: kit,
            });
        }
        roots.extend(self.places(Purpose::Sample));
        self.open_explorer(Purpose::Sample, roots, None);
        if let Some(picker) = self.overlays.picker.take() {
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
        if let Some(picker) = self.overlays.picker.take() {
            self.show_picker(picker.opening_roots());
        }
    }
}

impl Explorer {
    /// What the explorer's preview shows for an item, from what's been read.
    fn preview(&mut self, picker: &mut Picker, item: usize, preview: &Preview) -> Option<Shown> {
        match preview {
            Preview::Sample(path) => {
                let read = self.loader.want(path);
                match &read {
                    Some(Err(_)) => picker.mark_unreadable(path),
                    Some(Ok(loaded)) => self.note_heard(Some(picker), path, loaded.heard),
                    None => {}
                }
                Some(sample_preview(item, path, read.as_ref()))
            }
            Preview::Project(folder) => Some(project_preview(item, folder)),
            Preview::Kit(_) => None,
        }
    }

    /// Load the preview of the item under the picker's cursor, if it has one and it isn't already
    /// shown. A sample's is read in the background, and shows it's loading until it's in.
    pub(crate) fn refresh_preview(&mut self, picker: &mut Picker, rate: u32) {
        let wanted = picker
            .current_index()
            .filter(|item| picker.has_previews() && picker.item(*item).preview.is_some());
        if picker.preview.as_ref().map(|shown| shown.item) == wanted {
            return;
        }
        picker.preview_images.clear();
        picker.preview_wave = None;
        let Some(item) = wanted else {
            picker.preview = None;
            return;
        };
        let Some(preview) = picker.item(item).preview.clone() else {
            return;
        };
        let shown = match &preview {
            Preview::Kit(path) => Some(kit_preview(item, path, rate, &mut picker.waves)),
            _ => self.preview(picker, item, &preview),
        };
        picker.preview = shown.map(|shown| {
            let tags = picker.item(item).tags.clone();
            Arc::new(Shown { tags, ..shown })
        });
    }

    /// The sample under the explorer's cursor, while it plays what it moves onto.
    fn file_to_play(&mut self, picker: &Picker) -> Option<PathBuf> {
        let tree = picker.tree()?;
        if !tree.purpose.auditions() {
            return None;
        }
        self.audition_files = tree.audition;
        if !tree.audition {
            return None;
        }
        match &picker.current()?.pick {
            Pick::Sample(path) | Pick::KitSound(KitChoice { path, .. }) => Some(path.clone()),
            _ => None,
        }
    }

    /// Play the sample under the explorer's cursor on the current track, as the M8's and
    /// Elektron's browsers do, once it's been read, and stop the last one when the cursor moves.
    /// Says whether it started one playing.
    pub(crate) fn follow_file(
        &mut self,
        mut picker: Option<&mut Picker>,
        services: &mut Services,
    ) -> bool {
        let wanted = picker
            .as_deref()
            .and_then(|picker| self.file_to_play(picker));
        if wanted != self.file_wanted {
            self.stop_file(services);
            self.file_wanted = wanted;
        }
        let Some(path) = self.file_wanted.clone() else {
            return false;
        };
        if self.file_playing.is_some() {
            return false;
        }
        match self.loader.want(&path) {
            Some(Ok(loaded)) => {
                self.note_heard(picker.as_deref_mut(), &path, loaded.heard);
                let track = services.cursor.track;
                if services
                    .handle
                    .play_file(track, Arc::clone(&loaded.sample), None)
                {
                    self.file_playing = Some(path);
                    return true;
                }
            }
            // It won't play, and the preview says why.
            Some(Err(_)) => {
                if let Some(picker) = picker {
                    picker.mark_unreadable(&path);
                }
                self.file_playing = Some(path);
            }
            None => {}
        }
        false
    }

    /// Stop the file the explorer is playing, if it is.
    fn stop_file(&mut self, services: &mut Services) {
        if self.file_playing.take().is_some() {
            services.handle.send(Command::StopFile);
        }
    }

    /// The explorer has closed: stop what it plays, and forget any name line it was for.
    pub(crate) fn leave(&mut self, services: &mut Services) {
        self.stop_file(services);
        self.file_wanted = None;
        self.naming = None;
    }
}

impl Overlays {
    /// Load the preview of the item under the open picker's cursor, as
    /// [`Explorer::refresh_preview`] does.
    pub(crate) fn refresh_preview(&mut self, rate: u32) {
        if let Some(picker) = self.picker.as_mut() {
            self.explorer.refresh_preview(picker, rate);
        }
    }
}

impl Services {
    /// A kit at a path, or by name in the first of the settings' kit folders that has it. A
    /// name found nowhere is looked for in the first folder, so the error names it.
    pub(crate) fn kit_path(&self, name: &Path) -> PathBuf {
        if name.is_file() {
            return name.to_path_buf();
        }
        let candidates: Vec<PathBuf> = self
            .settings
            .kit_folders()
            .iter()
            .map(|folder| folder.join(name).with_extension("ron"))
            .collect();
        candidates
            .iter()
            .find(|candidate| candidate.is_file())
            .or(candidates.first())
            .cloned()
            .unwrap_or_else(|| name.with_extension("ron"))
    }

    /// The project's name, which is the song's own: the random one it was given when it was
    /// made, its folder's when it's saved somewhere new, or what the project page names it.
    pub fn project_name(&self) -> String {
        self.song().name.clone()
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
