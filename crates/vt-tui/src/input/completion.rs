//! What Tab completes on the `:` command line, after Vim's command-line completion: command names,
//! `:set` keys and their values, each command's own words, kits and paths. The words come from the
//! parser's own tables in `command.rs`, so the two can't drift apart.

use std::path::{Path, PathBuf};

use vt_model::groove::GrooveId;
use vt_model::order::{ORDER_WORDS, Snake};
use vt_model::sample::is_sample_file;
use vt_model::scale::{ROOTS, ScaleKind};

use crate::config::expand_from;
use crate::input::command::{self, COMMANDS, KIT_ALL, PRESET_WORDS, RENDER_FLAGS, SETTINGS, Takes};
use crate::input::picker::{Item, Pick, Picker};

/// Where completion looks for files: the kit folders, and the home folder `~` stands for.
#[derive(Debug, Clone, Copy)]
pub struct Places<'a> {
    pub kit_folders: &'a [PathBuf],
    pub home: Option<&'a Path>,
    /// The presets folder's `sounds` or `effects`, for what `:preset` acts on here.
    pub presets: Option<&'a Path>,
}

/// One way to finish the word: the text that replaces it, and the name and meaning the list shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub text: String,
    pub label: String,
    pub meaning: String,
}

impl Candidate {
    /// A word that's shown as it's put in.
    fn word(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            label: text.to_owned(),
            meaning: String::new(),
        }
    }

    fn meaning(self, meaning: &str) -> Self {
        Self {
            meaning: meaning.to_owned(),
            ..self
        }
    }
}

/// What Tab can do with a command line: where the word being completed starts, in bytes, what the
/// list of ways to finish it is called, and the ways, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completions {
    pub start: usize,
    pub title: &'static str,
    pub candidates: Vec<Candidate>,
}

impl Completions {
    fn none(start: usize) -> Self {
        Self {
            start,
            title: "",
            candidates: Vec::new(),
        }
    }

    /// The text every candidate starts with, ignoring case, in the first one's case.
    pub fn common(&self) -> &str {
        let Some((first, rest)) = self.candidates.split_first() else {
            return "";
        };
        let mut length = first.text.len();
        for other in rest {
            let shared = first
                .text
                .char_indices()
                .zip(other.text.chars())
                .take_while(|((_, a), b)| a.to_lowercase().eq(b.to_lowercase()))
                .last()
                .map_or(0, |((at, a), _)| at + a.len_utf8());
            length = length.min(shared);
        }
        &first.text[..length]
    }

    /// A picker over the candidates, which the list above the command line shows.
    pub fn picker(&self) -> Picker {
        let items = self
            .candidates
            .iter()
            .map(|candidate| {
                Item::new(&candidate.label, Pick::Completion(candidate.text.clone()))
                    .meaning(&candidate.meaning)
            })
            .collect();
        Picker::new(self.title, items)
    }
}

/// The list of matches open above the command line: a picker over them, and where the word they
/// finish starts.
#[derive(Debug, Clone)]
pub struct CommandMenu {
    pub picker: Picker,
    pub start: usize,
}

impl CommandMenu {
    /// What the highlighted match puts in place of the word.
    pub fn highlighted(&self) -> Option<&str> {
        match &self.picker.current()?.pick {
            Pick::Completion(text) => Some(text),
            _ => None,
        }
    }

    /// How many characters into the line the names it lists start: where the word starts, after
    /// any folder typed before a path's names.
    pub fn column(&self, line: &str) -> usize {
        let before = line[..self.start.min(line.len())].chars().count();
        let folder = self
            .picker
            .items()
            .first()
            .map_or(0, |item| match &item.pick {
                Pick::Completion(text) => text.chars().count() - item.label.chars().count(),
                _ => 0,
            });
        before + folder
    }

    /// Move the highlight, going round from one end to the other, as Vim's list does.
    pub fn step(&mut self, by: isize) {
        let count = self.picker.rows().len();
        if count > 0 {
            let at = self.picker.selected as isize + by;
            self.picker.selected = at.rem_euclid(count as isize) as usize;
        }
    }
}

/// The line with the text from `start` on replaced.
pub fn replaced(line: &str, start: usize, text: &str) -> String {
    format!("{}{text}", &line[..start.min(line.len())])
}

/// Whether `text` starts with `word`, ignoring case.
fn starts_with(text: &str, word: &str) -> bool {
    text.to_lowercase().starts_with(&word.to_lowercase())
}

/// The words that start with what's typed, in order.
fn words<'a>(
    start: usize,
    title: &'static str,
    typed: &str,
    words: impl IntoIterator<Item = &'a str>,
) -> Completions {
    Completions {
        start,
        title,
        candidates: words
            .into_iter()
            .filter(|word| starts_with(word, typed))
            .map(Candidate::word)
            .collect(),
    }
}

/// What can finish a command line, without its leading `:`, as the cursor stands at its end.
pub fn complete(line: &str, places: Places) -> Completions {
    // A range before the command, as `'<,'>`, is passed over.
    let after_range = match command::split_range(line) {
        Ok((_, rest)) => rest,
        Err(_) => line.trim_start(),
    };
    let leading = line.len() - after_range.len();
    let rest = &line[leading..];
    let Some(space) = rest.find(char::is_whitespace) else {
        return Completions {
            start: leading,
            title: "Commands",
            candidates: COMMANDS
                .iter()
                .filter(|command| starts_with(command.name, rest))
                .map(|command| Candidate::word(command.name).meaning(command.meaning))
                .collect(),
        };
    };
    let after = &rest[space..];
    let start = leading + space + after.len() - after.trim_start().len();
    let argument = &line[start..];
    let Some(command) = command::find(&rest[..space]) else {
        return Completions::none(start);
    };
    match command.takes {
        Takes::Setting => match argument.split_once('=') {
            Some((key, value)) => match SETTINGS.iter().find(|setting| setting.key == key) {
                Some(setting) => value_of(
                    setting.takes,
                    start + key.len() + 1,
                    value,
                    setting.key,
                    places,
                ),
                None => Completions::none(start),
            },
            None => Completions {
                start,
                title: "Settings",
                candidates: SETTINGS
                    .iter()
                    .filter(|setting| starts_with(setting.key, argument))
                    .map(|setting| {
                        Candidate::word(&format!("{}=", setting.key)).meaning(setting.meaning)
                    })
                    .collect(),
            },
        },
        Takes::Render => render(start, argument, places),
        Takes::Kit => kit(start, argument, places),
        Takes::Preset => preset(start, argument, command.name, places),
        takes => value_of(takes, start, argument, command.name, places),
    }
}

/// What can finish a word that's one value, starting at `start`, under a list called `title`.
fn value_of(
    takes: Takes,
    start: usize,
    typed: &str,
    title: &'static str,
    places: Places,
) -> Completions {
    match takes {
        Takes::Words(list) if !typed.contains(char::is_whitespace) => {
            words(start, title, typed, list.iter().copied())
        }
        Takes::Scale => {
            let names: Vec<String> = ScaleKind::ALL
                .iter()
                .map(|kind| kind.name().to_lowercase().replace(' ', "-"))
                .collect();
            let all = ["on", "off"]
                .into_iter()
                .chain(names.iter().map(String::as_str));
            words(start, title, typed, all)
        }
        Takes::Root => words(start, title, typed, ROOTS),
        Takes::Order => match typed.split_once(char::is_whitespace) {
            None => words(start, title, typed, ORDER_WORDS),
            Some((first, snake)) if first.eq_ignore_ascii_case("snake") => {
                let snake = snake.trim_start();
                let at = start + typed.len() - snake.len();
                words(at, "snake", snake, Snake::ALL.map(Snake::word))
            }
            Some(_) => Completions::none(start + typed.len()),
        },
        Takes::Groove { off } => {
            let names: Vec<String> = GrooveId::all().map(command::groove_word).collect();
            let off = off.then_some("off");
            let all = off.into_iter().chain(names.iter().map(String::as_str));
            words(start, title, typed, all)
        }
        Takes::Folder => paths(start, typed, places.home, Files::Folders),
        Takes::Sample => paths(start, typed, places.home, Files::Samples),
        Takes::Render => paths(start, typed, places.home, Files::Wav),
        Takes::Kit => paths(start, typed, places.home, Files::Kits),
        Takes::Words(_) | Takes::Nothing | Takes::Setting | Takes::Preset => {
            Completions::none(start)
        }
    }
}

/// `:render`'s path, then its flags once the path is typed: after a space, or on a word that
/// starts with `-`. A flag already given isn't offered again, and nothing follows `--rows`, which
/// takes a number.
fn render(start: usize, argument: &str, places: Places) -> Completions {
    let last_start = argument.rfind(char::is_whitespace).map_or(0, |at| {
        at + argument[at..].chars().next().map_or(1, char::len_utf8)
    });
    let last = &argument[last_start..];
    let earlier: Vec<&str> = argument[..last_start].split_whitespace().collect();
    let [rows_flag, ..] = RENDER_FLAGS;
    if earlier.last() == Some(&rows_flag) {
        return Completions::none(start + last_start);
    }
    if last.starts_with('-') || (last.is_empty() && !earlier.is_empty()) {
        let unused = RENDER_FLAGS
            .into_iter()
            .filter(|flag| !earlier.contains(flag));
        return words(start + last_start, "render", last, unused);
    }
    if earlier.iter().any(|word| word.starts_with('-')) {
        return Completions::none(start + last_start);
    }
    value_of(Takes::Render, start, argument, "render", places)
}

/// `:kit`'s kit, by name from the kit folders, or by path when it starts as one does, then `all`.
fn kit(start: usize, argument: &str, places: Places) -> Completions {
    if let Some(at) = argument.rfind(' ') {
        let (kit, last) = (argument[..at].trim_end(), &argument[at + 1..]);
        if starts_with(KIT_ALL, last) && is_kit(kit, places) {
            return Completions {
                start: start + at + 1,
                title: "kit",
                candidates: vec![Candidate::word(KIT_ALL).meaning("Put its sounds on tracks")],
            };
        }
    }
    if argument.starts_with(['/', '~', '.']) {
        return value_of(Takes::Kit, start, argument, "kit", places);
    }
    let (folder_typed, prefix) = split_path(argument);
    let mut candidates: Vec<Candidate> = places
        .kit_folders
        .iter()
        .flat_map(|root| entries(&root.join(folder_typed), prefix, Files::KitNames))
        .map(|entry| entry.candidate(folder_typed))
        .collect();
    candidates.sort_by_key(|candidate| candidate.text.to_lowercase());
    candidates.dedup_by(|a, b| a.text == b.text);
    Completions {
        start,
        title: "Kits",
        candidates,
    }
}

/// `:preset`'s word, then a preset's name from the presets folder, by its folders and name without
/// `.ron`, as `:kit` completes a kit's. `:preset!` only saves.
fn preset(start: usize, argument: &str, command: &'static str, places: Places) -> Completions {
    let Some((word, name)) = argument.split_once(char::is_whitespace) else {
        let list: &[&str] = if command.ends_with('!') {
            &PRESET_WORDS[..1]
        } else {
            &PRESET_WORDS
        };
        return words(start, command, argument, list.iter().copied());
    };
    if !PRESET_WORDS.contains(&word) {
        return Completions::none(start);
    }
    let start = start + word.len() + 1;
    let (folder_typed, prefix) = split_path(name);
    let candidates = places.presets.map_or_else(Vec::new, |root| {
        entries(&root.join(folder_typed), prefix, Files::KitNames)
            .into_iter()
            .map(|entry| entry.candidate(folder_typed))
            .collect()
    });
    Completions {
        start,
        title: "Presets",
        candidates,
    }
}

/// Whether a kit's name or path names a kit file.
fn is_kit(kit: &str, places: Places) -> bool {
    !kit.is_empty()
        && (expand_from(kit, places.home).is_file()
            || places
                .kit_folders
                .iter()
                .any(|folder| folder.join(kit).with_extension("ron").is_file()))
}

/// Which files a path can name, beside folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Files {
    /// Only folders, as a project is one.
    Folders,
    Samples,
    Wav,
    /// Kit files, by path.
    Kits,
    /// Kit files, by name, without `.ron`.
    KitNames,
}

impl Files {
    fn wants(self, file: &Path) -> bool {
        let extension = |wanted: &str| {
            file.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case(wanted))
        };
        match self {
            Self::Folders => false,
            Self::Samples => is_sample_file(file),
            Self::Wav => extension("wav"),
            Self::Kits | Self::KitNames => extension("ron"),
        }
    }
}

/// A file or folder in a folder, as completion offers it.
struct Entry {
    name: String,
    folder: bool,
}

impl Entry {
    /// What replaces the word: the folder as typed, then the name, with a `/` on a folder so the
    /// next Tab goes into it.
    fn candidate(&self, folder_typed: &str) -> Candidate {
        let label = if self.folder {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        };
        Candidate {
            text: format!("{folder_typed}{label}"),
            label,
            meaning: String::new(),
        }
    }
}

/// A path as typed, split after its last `/`: the folder, and the start of a name in it.
fn split_path(typed: &str) -> (&str, &str) {
    match typed.rfind('/') {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

/// Paths, one folder at a time: the folders and wanted files in the folder typed whose names start
/// with what follows it. `~` is the home folder, and hidden files are left out unless a `.` is
/// typed.
fn paths(start: usize, typed: &str, home: Option<&Path>, files: Files) -> Completions {
    let title = "Paths";
    if typed == "~" && home.is_some() {
        return Completions {
            start,
            title,
            candidates: vec![Candidate::word("~/")],
        };
    }
    let (folder_typed, prefix) = split_path(typed);
    let folder = if folder_typed.is_empty() {
        PathBuf::from(".")
    } else {
        expand_from(folder_typed, home)
    };
    Completions {
        start,
        title,
        candidates: entries(&folder, prefix, files)
            .iter()
            .map(|entry| entry.candidate(folder_typed))
            .collect(),
    }
}

/// The folders and wanted files in a folder whose names start with `prefix`, ignoring case, in
/// order of name.
fn entries(folder: &Path, prefix: &str, files: Files) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<Entry> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if (name.starts_with('.') && !prefix.starts_with('.')) || !starts_with(&name, prefix) {
                return None;
            }
            let path = entry.path();
            let folder = path.is_dir();
            if !folder && !files.wants(&path) {
                return None;
            }
            let name = if files == Files::KitNames && !folder {
                path.file_stem()?.to_string_lossy().into_owned()
            } else {
                name
            };
            Some(Entry { name, folder })
        })
        .collect();
    found.sort_by_key(|entry| entry.name.to_lowercase());
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::command::parse;

    fn texts(found: &Completions) -> Vec<&str> {
        found
            .candidates
            .iter()
            .map(|candidate| candidate.text.as_str())
            .collect()
    }

    fn nowhere() -> Places<'static> {
        Places {
            kit_folders: &[],
            home: None,
            presets: None,
        }
    }

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-complete-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// The line after one Tab, the way the command line puts it: the only match, or the part every
    /// match shares.
    fn tab(line: &str, places: Places) -> String {
        let found = complete(line, places);
        match found.candidates.as_slice() {
            [] => line.to_owned(),
            [only] => replaced(line, found.start, &only.text),
            _ => replaced(line, found.start, found.common()),
        }
    }

    #[test]
    fn command_names_complete_with_what_they_mean() {
        assert_eq!(tab("sa", nowhere()), "sample");
        assert_eq!(tab("tv", nowhere()), "tvol");
        let found = complete("s", nowhere());
        assert_eq!(found.title, "Commands");
        assert_eq!(
            texts(&found),
            ["sample", "slice", "set", "scale", "swing", "solo", "seed"]
        );
        assert_eq!(
            found.candidates[0].meaning,
            "Load a sample on this track, or set where it starts, ends and loops"
        );
        assert_eq!(tab("se", nowhere()), "se", "seed and set share only se");
        assert_eq!(texts(&complete("q", nowhere())), ["q", "q!"]);
        assert_eq!(texts(&complete("ne", nowhere())), ["new", "new!"]);
        assert!(texts(&complete("e", nowhere())).starts_with(&["e", "e!"]));
        assert!(complete("zz", nowhere()).candidates.is_empty());
    }

    #[test]
    fn a_range_before_a_command_is_passed_over() {
        assert_eq!(tab("'<,'>sli", nowhere()), "'<,'>slice");
        assert_eq!(tab("'a,'b slice tr", nowhere()), "'a,'b slice transients");
        assert_eq!(tab("%slice si", nowhere()), "%slice silence");
    }

    #[test]
    fn set_completes_its_keys_then_their_values() {
        assert_eq!(tab("set sk", nowhere()), "set skip=");
        assert_eq!(tab("set li", nowhere()), "set limiter=");
        assert_eq!(tab("set limiter=of", nowhere()), "set limiter=off");
        assert_eq!(tab("set root=f", nowhere()), "set root=F");
        assert_eq!(texts(&complete("set root=f", nowhere())), ["F", "F#"]);
        assert_eq!(tab("set scale=dor", nowhere()), "set scale=dorian");
        assert_eq!(tab("set scale=harm", nowhere()), "set scale=harmonic-minor");
        let found = complete("set scale=m", nowhere());
        assert_eq!(found.title, "scale");
        assert_eq!(
            texts(&found),
            [
                "major",
                "minor",
                "mixolydian",
                "minor-pentatonic",
                "major-pentatonic",
                "melodic-minor"
            ]
        );
        assert!(complete("set skip=", nowhere()).candidates.is_empty());
        assert!(complete("set nope=", nowhere()).candidates.is_empty());
    }

    #[test]
    fn each_command_completes_its_own_words() {
        assert_eq!(tab("track a", nowhere()), "track add");
        assert_eq!(texts(&complete("track ", nowhere())), ["add", "del"]);
        assert!(complete("track add ", nowhere()).candidates.is_empty());
        assert_eq!(tab("scale f", nowhere()), "scale fit");
        assert_eq!(tab("reset o", nowhere()), "reset off");
        assert_eq!(tab("launch 4", nowhere()), "launch 4bar");
        assert_eq!(tab("launch c", nowhere()), "launch chain");
        assert_eq!(
            texts(&complete("launch 1", nowhere())),
            ["1/4", "1/2", "1bar"]
        );
        assert_eq!(tab("launch 1", nowhere()), "launch 1");
        assert_eq!(texts(&complete("bpm ", nowhere())), ["sample"]);
        assert!(complete("nope ", nowhere()).candidates.is_empty());
    }

    #[test]
    fn render_completes_a_wav_then_its_flags() {
        let root = folder("render");
        std::fs::write(root.join("loop.wav"), "").unwrap();
        std::fs::write(root.join("notes.txt"), "").unwrap();
        let places = nowhere();
        let at = root.display();
        assert_eq!(
            tab(&format!("render {at}/"), places),
            format!("render {at}/loop.wav")
        );
        assert_eq!(
            tab(&format!("render {at}/loop.wav --t"), places),
            format!("render {at}/loop.wav --tail")
        );
        assert_eq!(
            texts(&complete("render out.wav --tail ", places)),
            ["--rows", "--stems", "--split"],
            "a flag given isn't offered again"
        );
        assert!(
            complete("render out.wav --rows ", places)
                .candidates
                .is_empty()
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn paths_go_one_folder_at_a_time_and_tilde_is_home() {
        let home = folder("home");
        std::fs::create_dir_all(home.join("Music/kits")).unwrap();
        std::fs::create_dir_all(home.join("Music/songs")).unwrap();
        std::fs::create_dir_all(home.join(".hidden")).unwrap();
        std::fs::write(home.join("Music/kick.wav"), "").unwrap();
        std::fs::write(home.join("Music/readme.txt"), "").unwrap();
        let places = Places {
            kit_folders: &[],
            home: Some(&home),
            presets: None,
        };
        assert_eq!(tab("e ~", places), "e ~/");
        assert_eq!(
            tab("e ~/", places),
            "e ~/Music/",
            "hidden folders are left out"
        );
        assert_eq!(tab("e ~/.h", places), "e ~/.hidden/");
        assert_eq!(tab("e ~/mu", places), "e ~/Music/", "case doesn't matter");
        let found = complete("e ~/Music/", places);
        assert_eq!(found.title, "Paths");
        assert_eq!(
            texts(&found),
            ["~/Music/kits/", "~/Music/songs/"],
            "only folders"
        );
        assert_eq!(
            found
                .candidates
                .iter()
                .map(|candidate| candidate.label.as_str())
                .collect::<Vec<_>>(),
            ["kits/", "songs/"],
            "the list shows names"
        );
        assert_eq!(
            texts(&complete("sample ~/Music/", places)),
            ["~/Music/kick.wav", "~/Music/kits/", "~/Music/songs/"],
            "samples and folders"
        );
        assert_eq!(tab("sample ~/Music/ki", places), "sample ~/Music/ki");
        assert_eq!(tab("sample ~/Music/kic", places), "sample ~/Music/kick.wav");
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn kits_complete_by_name_from_every_kit_folder_then_all() {
        let first = folder("kits-a");
        let second = folder("kits-b");
        std::fs::create_dir_all(first.join("Ableton/Drum Machines")).unwrap();
        std::fs::write(first.join("Ableton/Drum Machines/909 Core.ron"), "").unwrap();
        std::fs::write(first.join("808.ron"), "").unwrap();
        std::fs::write(second.join("909.ron"), "").unwrap();
        std::fs::write(second.join("notes.txt"), "").unwrap();
        let folders = [first.clone(), second.clone()];
        let places = Places {
            kit_folders: &folders,
            home: None,
            presets: None,
        };
        let found = complete("kit ", places);
        assert_eq!(found.title, "Kits");
        assert_eq!(texts(&found), ["808", "909", "Ableton/"]);
        assert_eq!(tab("kit a", places), "kit Ableton/");
        assert_eq!(tab("kit Ableton/", places), "kit Ableton/Drum Machines/");
        assert_eq!(
            tab("kit Ableton/Drum Machines/", places),
            "kit Ableton/Drum Machines/909 Core"
        );
        assert_eq!(tab("kit 909 a", places), "kit 909 all");
        assert_eq!(
            tab("kit Ableton/Drum Machines/909 Core ", places),
            "kit Ableton/Drum Machines/909 Core all"
        );
        assert_eq!(tab("kit nope a", places), "kit nope a", "not a kit");
        let path = first.join("808.ron");
        let typed = format!("kit {}", first.join("80").display());
        assert_eq!(tab(&typed, places), format!("kit {}", path.display()));
        std::fs::remove_dir_all(&first).unwrap();
        std::fs::remove_dir_all(&second).unwrap();
    }

    #[test]
    fn every_command_and_setting_can_be_completed_and_what_completes_parses() {
        for command in &COMMANDS {
            let found = complete(&command.name[..1], nowhere());
            assert!(
                texts(&found).contains(&command.name),
                "{} from its first letter",
                command.name
            );
        }
        for setting in &SETTINGS {
            let found = complete("set ", nowhere());
            let key = format!("{}=", setting.key);
            assert!(texts(&found).contains(&key.as_str()), "{}", setting.key);
        }
        // Every word a command or a setting completes makes a line that parses.
        let mut checked = 0;
        for command in &COMMANDS {
            let lines: Vec<String> = match command.takes {
                Takes::Setting => SETTINGS
                    .iter()
                    .flat_map(|setting| {
                        let line = format!("set {}=", setting.key);
                        let found = complete(&line, nowhere());
                        found
                            .candidates
                            .into_iter()
                            .map(move |candidate| replaced(&line, found.start, &candidate.text))
                    })
                    .collect(),
                _ => {
                    let line = format!("{} ", command.name);
                    let found = complete(&line, nowhere());
                    // `:preset`'s words take a name after them.
                    let name = if command.takes == Takes::Preset {
                        " Acid"
                    } else {
                        ""
                    };
                    found
                        .candidates
                        .iter()
                        .map(|candidate| replaced(&line, found.start, &candidate.text) + name)
                        .collect()
                }
            };
            if matches!(
                command.takes,
                Takes::Words(_)
                    | Takes::Setting
                    | Takes::Groove { .. }
                    | Takes::Order
                    | Takes::Preset
            ) {
                assert!(!lines.is_empty(), "{} completes something", command.name);
            }
            for line in lines {
                assert!(parse(&line).is_ok(), "{line} parses");
                checked += 1;
            }
        }
        assert!(checked > 30, "only {checked} lines");
    }

    #[test]
    fn preset_completes_its_words_then_the_presets_names() {
        let root = folder("presets");
        std::fs::create_dir_all(root.join("bass")).unwrap();
        std::fs::write(root.join("Pad.ron"), "").unwrap();
        std::fs::write(root.join("bass/Acid Bite.ron"), "").unwrap();
        let places = Places {
            kit_folders: &[],
            home: None,
            presets: Some(&root),
        };
        assert_eq!(texts(&complete("preset ", places)), ["save", "load"]);
        assert_eq!(texts(&complete("preset! ", places)), ["save"]);
        let found = complete("preset load ", places);
        assert_eq!(found.title, "Presets");
        assert_eq!(texts(&found), ["bass/", "Pad"]);
        assert_eq!(
            texts(&complete("preset save bass/", places)),
            ["bass/Acid Bite"]
        );
        assert!(complete("preset wobble ", places).candidates.is_empty());
    }

    #[test]
    fn order_completes_its_words_and_a_snake_after_snake() {
        assert_eq!(tab("order sn", nowhere()), "order snake");
        assert_eq!(tab("order cart", nowhere()), "order cartesian");
        assert_eq!(tab("order snake sp", nowhere()), "order snake spiral");
        assert_eq!(
            tab("order snake spiral-", nowhere()),
            "order snake spiral-out"
        );
        assert_eq!(tab("order cartesian 1", nowhere()), "order cartesian 1");
        let found = complete("order snake ", nowhere());
        assert_eq!(found.candidates.len(), 8);
        for candidate in &found.candidates {
            let line = replaced("order snake ", found.start, &candidate.text);
            assert!(parse(&line).is_ok(), "{line} parses");
        }
    }

    #[test]
    fn the_common_part_ignores_case() {
        let found = Completions {
            start: 0,
            title: "",
            candidates: vec![Candidate::word("Samples/"), Candidate::word("sampler.wav")],
        };
        assert_eq!(found.common(), "Sample");
    }
}
