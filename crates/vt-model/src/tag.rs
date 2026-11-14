//! Tags: plain words such as `kick`, `loop` and `bass` that find samples, kits' sounds and presets
//! across folders, after Ableton's browser tags, Elektron's sound tags and Splice's (decision 315).
//!
//! A file's tags are guessed from its folders and its name, from a kit's label for its sound, and
//! from how its sound ends once it's been read, and given by hand. What's given by hand, and the
//! guesses taken off, are kept in `tags.ron` in the config folder by the file's content hash, so
//! they follow it when it's moved or renamed, and nothing is ever written beside the user's
//! samples. A preset keeps its own tags in its file, beside the ones its kind and machine give it.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::analysis::SILENCE_DB;
use crate::bank::ContentHash;
use crate::song::Machine;

/// The tag library's file, in the config folder.
pub const TAGS_FILE: &str = "tags.ron";

/// The most characters a tag has.
pub const MAX_TAG_CHARS: usize = 24;

/// A sample that's a loop, and one that's a single hit.
pub const LOOP: &str = "loop";
pub const ONE_SHOT: &str = "one-shot";

/// A file whose sound runs to its last 10 ms is cut while still sounding; one that falls silent
/// before then has decayed.
const END_MS: u64 = 10;
/// A sound that decays is a one-shot if it's at most this long, and one cut while still sounding
/// is a loop if it's at least the first and at most the second.
const ONE_SHOT_SECONDS: u64 = 10;
const LOOP_SECONDS: (u64, u64) = (1, 120);

/// Where a tag on something listed came from, which decides how it's drawn and whether `T` can
/// take it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Given by hand, or kept in a preset's file.
    Own,
    /// Guessed from a file's folders, its name, a kit's label or its sound.
    Guessed,
    /// What a preset is: its kind and a sound's machine, which are always so.
    Fixed,
}

/// A tag on something listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub source: Source,
}

impl Tag {
    pub fn new(name: impl Into<String>, source: Source) -> Self {
        Self {
            name: name.into(),
            source,
        }
    }
}

/// Whether a tag can be called this: lower-case letters and digits, with single dashes between
/// them, up to [`MAX_TAG_CHARS`]. Tags are flat, so there's no `/`.
pub fn is_tag(name: &str) -> bool {
    let allowed = |c: char| (c.is_alphanumeric() && !c.is_uppercase()) || c == '-';
    !name.is_empty()
        && name.chars().count() <= MAX_TAG_CHARS
        && name.chars().all(allowed)
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// A tag as it's typed, in lower case, without a `#` before it, and with spaces and underscores
/// as dashes, or none if it can't be one.
pub fn tag_name(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix('#').unwrap_or(text);
    let name: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '_' {
                '-'
            } else {
                c
            }
        })
        .collect();
    is_tag(&name).then_some(name)
}

/// The words in a name, in lower case, without the spaces and marks between them.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What a name's words say a sound is: a drum, as Ableton's kit import guesses a pad, or an
/// instrument. A number on the end of a word counts for nothing, so `BD01` is a kick.
fn sound_words(words: &[String]) -> Vec<&'static str> {
    let bare: Vec<&str> = words
        .iter()
        .map(|word| word.trim_end_matches(|c: char| c.is_ascii_digit()))
        .collect();
    let has = |options: &[&str]| bare.iter().any(|word| options.contains(word));
    let pair = |first: &str, second: &str| bare.windows(2).any(|w| w[0] == first && w[1] == second);
    let bass_drum = pair("bass", "drum") || has(&["bassdrum"]);
    let mut tags = Vec::new();
    if has(&["kick", "kicks", "bd", "kd"]) || bass_drum {
        tags.push("kick");
    }
    if has(&["snare", "snares", "sd"]) {
        tags.push("snare");
    }
    if has(&["clap", "claps", "handclap", "cp"]) {
        tags.push("clap");
    }
    if has(&["rim", "rims", "rimshot", "rs"]) {
        tags.push("rim");
    }
    let open = has(&["oh"]) || (has(&["open"]) && has(&["hat", "hats", "hihat", "hihats"]));
    if open || has(&["hat", "hats", "hihat", "hihats", "hh", "ch"]) {
        tags.push("hat");
    }
    if open {
        tags.push("open-hat");
    }
    if has(&["ride", "rides"]) {
        tags.push("ride");
    }
    if has(&["crash", "crashes"]) {
        tags.push("crash");
    }
    if has(&[
        "ride", "rides", "crash", "crashes", "cymbal", "cymbals", "cym", "china", "splash",
    ]) {
        tags.push("cymbal");
    }
    if has(&["tom", "toms"]) {
        tags.push("tom");
    }
    if has(&["cowbell", "cowbells"]) {
        tags.push("cowbell");
    }
    if has(&["shaker", "shakers"]) {
        tags.push("shaker");
    }
    if has(&[
        "perc",
        "percs",
        "percussion",
        "conga",
        "congas",
        "bongo",
        "bongos",
        "tambourine",
        "tamb",
    ]) {
        tags.push("perc");
    }
    if has(&["bass", "basses"]) && !bass_drum {
        tags.push("bass");
    }
    for (tag, options) in [
        ("pad", &["pad", "pads"][..]),
        ("lead", &["lead", "leads"]),
        ("chord", &["chord", "chords"]),
        ("vocal", &["vocal", "vocals", "vox"]),
        ("fx", &["fx", "sfx"]),
    ] {
        if has(options) {
            tags.push(tag);
        }
    }
    tags
}

/// Whether a tag says a drum, which is a one-shot unless something says it's a loop.
fn is_drum(tag: &str) -> bool {
    matches!(
        tag,
        "kick"
            | "snare"
            | "clap"
            | "rim"
            | "hat"
            | "open-hat"
            | "ride"
            | "crash"
            | "cymbal"
            | "tom"
            | "cowbell"
            | "shaker"
            | "perc"
    )
}

/// The tags a kit's label says, as the Rytm names its drums: `BD` a kick, `OH` an open hat.
pub fn label_tags(label: &str) -> &'static [&'static str] {
    let label = label.trim().to_ascii_uppercase();
    match label.trim_end_matches(|c: char| c.is_ascii_digit()) {
        "BD" | "KD" => &["kick"],
        "SD" | "SN" => &["snare"],
        "CP" => &["clap"],
        "RS" => &["rim"],
        "CH" | "HH" => &["hat"],
        "OH" => &["hat", "open-hat"],
        "RD" => &["ride", "cymbal"],
        "CY" | "CR" => &["cymbal"],
        "CB" => &["cowbell"],
        "LT" | "MT" | "HT" | "BT" => &["tom"],
        _ => &[],
    }
}

/// Whether a sample is a loop, a one-shot, or neither as far as its folders and name say. The
/// nearest folder named for loops or one-shots decides, as Ableton's library has `Loops` and
/// `One Shots`, whose one-shots can have a tempo in their names. Otherwise `loop` in its name,
/// or a tempo, such as `Break 120 bpm.wav`, makes it a loop, as sample packs name them.
pub fn loop_by_name(path: &Path) -> Option<bool> {
    for folder in path.ancestors().skip(1) {
        let Some(name) = folder.file_name() else {
            continue;
        };
        let words = words(&name.to_string_lossy());
        let has = |wanted: &str| words.iter().any(|word| word == wanted);
        if has("loop") || has("loops") {
            return Some(true);
        }
        let one_shot = words
            .windows(2)
            .any(|pair| pair[0] == "one" && matches!(pair[1].as_str(), "shot" | "shots"));
        if one_shot || has("oneshot") || has("oneshots") {
            return Some(false);
        }
    }
    let stem = path.file_stem()?;
    let words = words(&stem.to_string_lossy());
    if words.iter().any(|word| word == "loop" || word == "loops") {
        return Some(true);
    }
    let tempo = |word: &str| {
        (2..=3).contains(&word.len())
            && word
                .parse::<u32>()
                .is_ok_and(|bpm| (40..=300).contains(&bpm))
    };
    let named = words.iter().enumerate().any(|(at, word)| {
        let joined = word.strip_suffix("bpm").is_some_and(tempo);
        let apart = word == "bpm" && at > 0 && tempo(&words[at - 1]);
        joined || apart
    });
    named.then_some(true)
}

/// The tags guessed for a sample file, and for one of a kit's sounds what its label says too:
/// what its name says it is, or failing that one of its three nearest folders, as `Kicks/`; then
/// a loop or a one-shot by [`loop_by_name`], and a drum a one-shot if nothing says it's a loop.
pub fn guess(path: &Path, label: &str) -> Vec<&'static str> {
    let mut tags: Vec<&'static str> = label_tags(label).to_vec();
    let add = |found: Vec<&'static str>, tags: &mut Vec<&'static str>| {
        for tag in found {
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    };
    let named = path
        .file_stem()
        .map(|stem| sound_words(&words(&stem.to_string_lossy())))
        .unwrap_or_default();
    if named.is_empty() && tags.is_empty() {
        let folders = path.ancestors().skip(1).take(3);
        let found = folders
            .filter_map(|folder| folder.file_name())
            .map(|name| sound_words(&words(&name.to_string_lossy())))
            .find(|found| !found.is_empty());
        add(found.unwrap_or_default(), &mut tags);
    } else {
        add(named, &mut tags);
    }
    match loop_by_name(path) {
        Some(true) => tags.push(LOOP),
        Some(false) => tags.push(ONE_SHOT),
        None if tags.iter().any(|tag| is_drum(tag)) => tags.push(ONE_SHOT),
        None => {}
    }
    tags
}

/// Just past the last frame above [`SILENCE_DB`] in either channel, or none if it's all silence.
pub fn sound_end(frames: &[[f32; 2]]) -> Option<u64> {
    let threshold = 10_f32.powf(SILENCE_DB / 20.0);
    frames
        .iter()
        .rposition(|frame| frame.iter().any(|sample| sample.abs() > threshold))
        .map(|last| last as u64 + 1)
}

/// A one-shot or a loop by how a file's sound ends, once it's been read: one that falls silent
/// before its last 10 ms and lasts up to 10 s has decayed, so it's a one-shot, and one still
/// sounding there that lasts from 1 s to 2 minutes was cut from something longer, so it's a
/// loop. Anything else says nothing.
pub fn guess_heard(frames: u64, rate: u32, end: Option<u64>) -> Option<&'static str> {
    let end = end?;
    let rate = u64::from(rate.max(1));
    let tail = rate * END_MS / 1000;
    if end + tail < frames {
        (frames <= ONE_SHOT_SECONDS * rate).then_some(ONE_SHOT)
    } else {
        (LOOP_SECONDS.0 * rate..=LOOP_SECONDS.1 * rate)
            .contains(&frames)
            .then_some(LOOP)
    }
}

/// The tags a machine gives a sound preset: `sampler`, or a synth voice's kind and name, as
/// `analog` and `acid`, with `synth` or, for the drum voices, `drum`.
pub fn machine_tags(machine: Machine) -> Vec<String> {
    let voice = match machine {
        Machine::Sampler => return vec!["sampler".to_owned()],
        Machine::Plaits(voice) => voice,
    };
    let drum = voice.kind() == "Drums";
    let mut tags = vec![if drum { "drum" } else { "synth" }.to_owned()];
    if !drum {
        tags.extend(tag_name(voice.kind()));
    }
    tags.extend(tag_name(voice.name()));
    tags.dedup();
    tags
}

/// A file's tags: the guesses not taken off, then those given by hand.
pub fn file_tags(guessed: &[&str], kept: Option<&FileTags>) -> Vec<Tag> {
    let given = kept.map_or(&[][..], |kept| kept.tags.as_slice());
    let taken_off = kept.map_or(&[][..], |kept| kept.not.as_slice());
    let mut tags: Vec<Tag> = guessed
        .iter()
        .filter(|tag| !taken_off.iter().any(|off| off == *tag))
        .map(|tag| {
            let source = if given.iter().any(|own| own == tag) {
                Source::Own
            } else {
                Source::Guessed
            };
            Tag::new(*tag, source)
        })
        .collect();
    for own in given {
        if !tags.iter().any(|tag| tag.name == *own) {
            tags.push(Tag::new(own.clone(), Source::Own));
        }
    }
    tags
}

/// Whether a tag is among these, by name.
pub fn has(tags: &[Tag], name: &str) -> bool {
    tags.iter().any(|tag| tag.name == name)
}

/// The tags given to one file by hand, and the guesses taken off it, with where it was last
/// seen and its length, so a listing finds it by its path without reading it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileTags {
    /// Where the file was seen when it was tagged, as listed and as it is on disk.
    pub paths: Vec<PathBuf>,
    /// Its length in bytes: a file that's been moved is found again by reading only the files
    /// of the same length.
    pub size: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Guesses that are wrong for it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub not: Vec<String>,
}

/// The library file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct LibraryFile {
    files: BTreeMap<ContentHash, FileTags>,
}

/// Every file's tags given by hand, by content hash, and where they're kept.
#[derive(Debug, Clone, Default)]
pub struct TagLibrary {
    files: BTreeMap<ContentHash, FileTags>,
    by_path: HashMap<PathBuf, ContentHash>,
    /// The lengths of the files none of whose paths are there now, each with their hashes, so a
    /// file of one of those lengths is read to see whether it's one of them, moved.
    lost: HashMap<u64, Vec<ContentHash>>,
    /// The file they're saved to, or none to keep them for the session only, as tests do.
    path: Option<PathBuf>,
}

impl TagLibrary {
    /// The tags kept at `path`, saved there again as they change. No file starts with none, and
    /// a file that won't read starts with none and says why, and isn't written over.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let note = |error: &dyn std::fmt::Display| Some(format!("{}: {error}", path.display()));
        let (files, note, writable) = match fs::read_to_string(path) {
            Ok(text) => match ron::from_str::<LibraryFile>(&text) {
                Ok(file) => (file.files, None, true),
                Err(error) => (BTreeMap::new(), note(&error), false),
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => (BTreeMap::new(), None, true),
            Err(error) => (BTreeMap::new(), note(&error), false),
        };
        let mut library = Self {
            files,
            path: writable.then(|| path.to_path_buf()),
            ..Self::default()
        };
        library.index();
        (library, note)
    }

    fn index(&mut self) {
        self.by_path.clear();
        self.lost.clear();
        for (hash, file) in &self.files {
            for path in &file.paths {
                self.by_path.insert(path.clone(), *hash);
            }
            if !file.paths.iter().any(|path| path.exists()) {
                self.lost.entry(file.size).or_default().push(*hash);
            }
        }
    }

    /// Whether any file has tags given by hand or guesses taken off.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// What's kept for the file at a path: by the path, if it was seen there and is the same
    /// length, or else, if it's as long as a file that's gone from where it was seen, by its
    /// content, read to find out.
    pub fn of_file(&self, path: &Path) -> Option<&FileTags> {
        if let Some(hash) = self.by_path.get(path) {
            let file = self.files.get(hash)?;
            let size = fs::metadata(path).ok()?.len();
            return (size == file.size).then_some(file);
        }
        if self.lost.is_empty() {
            return None;
        }
        let size = fs::metadata(path).ok()?.len();
        let hashes = self.lost.get(&size)?;
        let hash = ContentHash::of_file(path).ok()?;
        hashes
            .contains(&hash)
            .then(|| self.files.get(&hash))
            .flatten()
    }

    /// Give the file at a path these tags by hand and take these guesses off it, by its content,
    /// and save. With neither, it's forgotten.
    pub fn set(&mut self, path: &Path, tags: Vec<String>, not: Vec<String>) -> io::Result<()> {
        let hash = ContentHash::of_file(path)?;
        let size = fs::metadata(path)?.len();
        let mut paths = vec![path.to_path_buf()];
        if let Ok(real) = path.canonicalize()
            && real != path
        {
            paths.push(real);
        }
        for (other, file) in &mut self.files {
            if *other != hash {
                file.paths.retain(|seen| !paths.contains(seen));
            }
        }
        if tags.is_empty() && not.is_empty() {
            self.files.remove(&hash);
        } else {
            let file = self.files.entry(hash).or_default();
            for seen in paths {
                if !file.paths.contains(&seen) {
                    file.paths.push(seen);
                }
            }
            file.paths.retain(|seen| seen.exists());
            file.size = size;
            file.tags = tags;
            file.not = not;
        }
        self.index();
        self.save()
    }

    /// Every tag given by hand, for completing one as it's typed.
    pub fn given(&self) -> impl Iterator<Item = &str> {
        self.files
            .values()
            .flat_map(|file| file.tags.iter().map(String::as_str))
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let folder = path.parent().unwrap_or(Path::new("."));
        fs::create_dir_all(folder)?;
        let file = LibraryFile {
            files: self.files.clone(),
        };
        let text = ron::ser::to_string_pretty(&file, ron::ser::PrettyConfig::default())
            .map_err(io::Error::other)?;
        let partial = folder.join(format!("{TAGS_FILE}.{}.part", std::process::id()));
        fs::write(&partial, text)?;
        fs::rename(&partial, path).inspect_err(|_| {
            let _ = fs::remove_file(&partial);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-tags-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn a_tag_is_lower_case_words_and_dashes() {
        for good in ["kick", "one-shot", "808", "räum", "a"] {
            assert!(is_tag(good), "{good}");
        }
        for bad in [
            "",
            "Kick",
            "drum/kick",
            "-kick",
            "kick-",
            "one--shot",
            "a b",
            "#kick",
        ] {
            assert!(!is_tag(bad), "{bad}");
        }
        assert!(!is_tag(&"x".repeat(MAX_TAG_CHARS + 1)));
        assert_eq!(tag_name("#Kick").as_deref(), Some("kick"));
        assert_eq!(tag_name("One Shot").as_deref(), Some("one-shot"));
        assert_eq!(tag_name("open_hat").as_deref(), Some("open-hat"));
        assert_eq!(tag_name("#"), None);
        assert_eq!(tag_name("drum/kick"), None);
    }

    #[test]
    fn a_machine_says_what_a_sound_is() {
        use crate::song::SynthVoice;
        assert_eq!(machine_tags(Machine::Sampler), ["sampler"]);
        assert_eq!(
            machine_tags(Machine::Plaits(SynthVoice::Acid)),
            ["synth", "analog", "acid"]
        );
        assert_eq!(
            machine_tags(Machine::Plaits(SynthVoice::SixOp)),
            ["synth", "fm", "6-op"]
        );
        assert_eq!(
            machine_tags(Machine::Plaits(SynthVoice::Kick)),
            ["drum", "kick"]
        );
    }

    #[test]
    fn a_kits_label_says_its_drum() {
        assert_eq!(label_tags("BD"), ["kick"]);
        assert_eq!(label_tags("sd2"), ["snare"]);
        assert_eq!(label_tags("OH"), ["hat", "open-hat"]);
        assert_eq!(label_tags("RD"), ["ride", "cymbal"]);
        assert_eq!(label_tags("LT"), ["tom"]);
        assert!(label_tags("PD").is_empty());
    }

    /// The guesses on a fixture set of kicks, snares, hats and loops, from the names Ableton's
    /// library, sample packs and the author's folders use.
    #[test]
    fn the_guesses_on_kicks_snares_hats_and_loops() {
        let library = "/Applications/Live.app/Core Library/Samples";
        let guessed =
            |path: &str, label: &str| guess(Path::new(&path.replace("LIB", library)), label);
        for (path, label, tags) in [
            (
                "LIB/One Shots/Drums/Kick/Kick 909 Tune11.wav",
                "",
                &["kick", ONE_SHOT][..],
            ),
            ("/samples/Kicks/BD01.wav", "", &["kick", ONE_SHOT]),
            ("/samples/Kicks/thump.wav", "", &["kick", ONE_SHOT]),
            ("/samples/drums/Bass Drum 3.aif", "", &["kick", ONE_SHOT]),
            ("/samples/Snare 808.wav", "", &["snare", ONE_SHOT]),
            (
                "LIB/One Shots/Drums/Hihat/Hihat Closed 808.aif",
                "",
                &["hat", ONE_SHOT],
            ),
            ("/samples/909 OH.wav", "", &["hat", "open-hat", ONE_SHOT]),
            ("/samples/Open Hat.wav", "", &["hat", "open-hat", ONE_SHOT]),
            ("/samples/Ride 909.wav", "", &["ride", "cymbal", ONE_SHOT]),
            ("/samples/Low Tom.wav", "", &["tom", ONE_SHOT]),
            ("LIB/Loops/Drums/Amen 136 bpm.wav", "", &[LOOP]),
            ("/samples/Hat Loop.wav", "", &["hat", LOOP]),
            ("/samples/break_174bpm.wav", "", &[LOOP]),
            (
                "LIB/One Shots/FX/Impact Deep Pad 120 bpm.wav",
                "",
                &["pad", ONE_SHOT],
            ),
            ("/samples/Sub Bass 01.wav", "", &["bass"]),
            ("/samples/x/sample01.wav", "BD", &["kick", ONE_SHOT]),
            ("/kits/drums/kick.wav", "SD", &["snare", "kick", ONE_SHOT]),
            ("/samples/Looper/thing.wav", "", &[]),
            ("/samples/909 thing.wav", "", &[]),
        ] {
            assert_eq!(guessed(path, label), tags, "{path} {label}");
        }
    }

    #[test]
    fn a_loop_is_known_by_its_folder_its_name_or_a_tempo() {
        let library = "/Applications/Live.app/Core Library/Samples";
        let is = |path: &str| loop_by_name(Path::new(&path.replace("LIB", library)));
        assert_eq!(
            is("LIB/Loops/FX/Scratch Baby.aif"),
            Some(true),
            "under Loops"
        );
        assert_eq!(is("/samples/Drum Loops/amen.wav"), Some(true));
        assert_eq!(is("/samples/Break 90 bpm.wav"), Some(true), "a tempo");
        assert_eq!(is("/samples/break_174bpm.wav"), Some(true));
        assert_eq!(
            is("LIB/One Shots/FX/Impact Deep Pad 120 bpm.wav"),
            Some(false),
            "a one-shot's folder says so, whatever its name"
        );
        assert_eq!(is("/samples/oneshots/hit 120 bpm.wav"), Some(false));
        assert_eq!(is("/samples/kick.wav"), None);
        assert_eq!(
            is("/samples/909 kick.wav"),
            None,
            "a number alone isn't a tempo"
        );
        assert_eq!(is("/samples/bpm.wav"), None);
        assert_eq!(
            is("/samples/Looper/kick.wav"),
            None,
            "a word, not part of one"
        );
    }

    #[test]
    fn how_a_sound_ends_says_one_shot_or_loop() {
        let rate = 48_000;
        let second = u64::from(rate);
        assert_eq!(
            guess_heard(second / 2, rate, Some(second / 4)),
            Some(ONE_SHOT)
        );
        assert_eq!(guess_heard(second * 4, rate, Some(second * 4)), Some(LOOP));
        assert_eq!(
            guess_heard(second / 2, rate, Some(second / 2)),
            None,
            "short and cut while sounding: a trimmed hit, or a short loop"
        );
        assert_eq!(
            guess_heard(second * 30, rate, Some(second)),
            None,
            "too long for a hit"
        );
        assert_eq!(guess_heard(second, rate, None), None, "silence");
        let mut frames = vec![[0.5, -0.5]; 100];
        frames.extend([[0.0, 0.0]; 50]);
        assert_eq!(sound_end(&frames), Some(100));
        assert_eq!(sound_end(&[[0.0, 0.0]; 10]), None);
    }

    #[test]
    fn given_tags_come_after_the_guesses_left_on() {
        let kept = FileTags {
            tags: vec!["punchy".into(), "kick".into()],
            not: vec![ONE_SHOT.into()],
            ..FileTags::default()
        };
        assert_eq!(
            file_tags(&["kick", ONE_SHOT], Some(&kept)),
            [
                Tag::new("kick", Source::Own),
                Tag::new("punchy", Source::Own)
            ]
        );
        assert_eq!(
            file_tags(&["kick"], None),
            [Tag::new("kick", Source::Guessed)]
        );
    }

    /// A tag on a sample survives moving and renaming the file, and nothing is written in the
    /// sample folders.
    #[test]
    fn a_tag_follows_its_file_when_its_moved_and_renamed() {
        let root = folder("moved");
        let samples = root.join("samples");
        fs::create_dir_all(samples.join("drums")).unwrap();
        let file = samples.join("drums/thing.wav");
        fs::write(&file, b"RIFF one sound").unwrap();
        fs::write(samples.join("other.wav"), b"RIFF another one").unwrap();
        let path = root.join("config").join(TAGS_FILE);
        let (mut library, note) = TagLibrary::load(&path);
        assert_eq!(note, None);
        library
            .set(&file, vec!["punchy".into()], vec![ONE_SHOT.into()])
            .unwrap();
        assert_eq!(library.of_file(&file).unwrap().tags, ["punchy"]);
        assert!(library.of_file(&samples.join("other.wav")).is_none());

        let moved = samples.join("kicks/big one.wav");
        fs::create_dir_all(moved.parent().unwrap()).unwrap();
        fs::rename(&file, &moved).unwrap();
        let (again, _) = TagLibrary::load(&path);
        let kept = again.of_file(&moved).expect("found by its content");
        assert_eq!(kept.tags, ["punchy"]);
        assert_eq!(kept.not, [ONE_SHOT]);
        assert!(
            again.of_file(&samples.join("other.wav")).is_none(),
            "another file isn't it"
        );
        let mut written: Vec<PathBuf> = walk(&samples);
        written.sort();
        assert_eq!(
            written,
            [samples.join("kicks/big one.wav"), samples.join("other.wav")],
            "nothing beside the samples"
        );

        // Changed, it's another sound, so its tags don't hold.
        fs::write(&moved, b"RIFF changed").unwrap();
        assert!(again.of_file(&moved).is_none());
        fs::remove_dir_all(&root).unwrap();
    }

    fn walk(folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found
    }

    #[test]
    fn the_library_reads_back_and_forgets_a_file_with_nothing_kept() {
        let root = folder("forget");
        let file = root.join("kick.wav");
        fs::write(&file, b"RIFF kick").unwrap();
        let path = root.join(TAGS_FILE);
        let (mut library, _) = TagLibrary::load(&path);
        library.set(&file, vec!["909".into()], Vec::new()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"909\""), "{text}");
        assert_eq!(
            TagLibrary::load(&path).0.given().collect::<Vec<_>>(),
            ["909"]
        );
        library.set(&file, Vec::new(), Vec::new()).unwrap();
        assert!(TagLibrary::load(&path).0.is_empty());
        fs::write(&path, "(nonsense: 1)").unwrap();
        let (bad, note) = TagLibrary::load(&path);
        assert!(note.unwrap().contains(TAGS_FILE));
        assert!(
            bad.path.is_none(),
            "a file that won't read isn't written over"
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
