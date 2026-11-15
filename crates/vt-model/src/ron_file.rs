//! RON files, read and written the one way: the song and its autosave, presets, kits and the tag
//! library here, and vt-tui's recent places, command history, imported kits and settings.
//!
//! A file is written whole or not at all: to a temporary file beside it, which is then renamed
//! over it ([`write_atomic`]), so a crash or a failed write leaves the file before intact. The
//! analysis cache writes its binary files the same way, and a project its imported samples. What
//! won't read says which file it is.
//!
//! A format's number is read on its own first ([`probe`]), so a file of another format says so
//! rather than failing on a field it doesn't know. What each format reads is its own: a song reads
//! only its own format, a preset its own and older ones. A kit is the one file with no format
//! number.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde::de::{self, DeserializeOwned, DeserializeSeed, IgnoredAny, MapAccess, Visitor};
use thiserror::Error;

/// Why a file could not be read or written, naming it.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: String, source: io::Error },
    #[error("{path}: {detail}")]
    Parse { path: String, detail: String },
    #[error("{path}: {detail}")]
    Serialize { path: String, detail: String },
}

/// What's said of a file that won't write, for the files whose writers give a plain I/O error.
impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Io { source, .. } => source,
            Error::Parse { detail, .. } | Error::Serialize { detail, .. } => {
                io::Error::other(detail)
            }
        }
    }
}

fn io_at(path: &Path) -> impl Fn(io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.display().to_string(),
        source,
    }
}

fn parse_at(path: &Path) -> impl Fn(ron::error::SpannedError) -> Error + '_ {
    move |error| Error::Parse {
        path: path.display().to_string(),
        detail: error.to_string(),
    }
}

/// A file's text.
pub fn read_text(path: &Path) -> Result<String, Error> {
    fs::read_to_string(path).map_err(io_at(path))
}

/// A value from a file's text, `path` naming it if it won't read.
pub fn parse<T: DeserializeOwned>(text: &str, path: &Path) -> Result<T, Error> {
    ron::from_str(text).map_err(parse_at(path))
}

/// A value from a file.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T, Error> {
    parse(&read_text(path)?, path)
}

/// The format number in a file's text, from its `field`, such as `format_version`, reading
/// nothing else of it, so it's read before the rest.
pub fn probe(text: &str, path: &Path, field: &'static str) -> Result<u32, Error> {
    ron::Options::default()
        .from_str_seed(text, FormatField(field))
        .map_err(parse_at(path))
}

/// Reads one field of a struct, as a struct of that field alone would, and passes over the rest.
struct FormatField(&'static str);

impl<'de> DeserializeSeed<'de> for FormatField {
    type Value = u32;

    fn deserialize<D: de::Deserializer<'de>>(self, reader: D) -> Result<u32, D::Error> {
        reader.deserialize_struct("Format", &[], self)
    }
}

impl<'de> Visitor<'de> for FormatField {
    type Value = u32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a struct with `{}`", self.0)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<u32, A::Error> {
        let mut found = None;
        while let Some(key) = map.next_key::<String>()? {
            if key != self.0 {
                map.next_value::<IgnoredAny>()?;
            } else if found.is_some() {
                return Err(de::Error::duplicate_field(self.0));
            } else {
                found = Some(map.next_value()?);
            }
        }
        found.ok_or_else(|| de::Error::missing_field(self.0))
    }
}

/// Write a value as pretty RON, whole or not at all, making its folders.
pub fn write<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), Error> {
    let text =
        ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default()).map_err(|error| {
            Error::Serialize {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
        })?;
    write_atomic(path, text)
}

/// Where [`write_atomic`] writes a file before renaming it: hidden beside it, and this process's
/// own, so two running at once don't write into the same one.
pub fn temporary(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let folder = path.parent().unwrap_or(Path::new(""));
    folder.join(format!(".{name}.{}.tmp", std::process::id()))
}

/// Write a file's bytes through a temporary file and a rename, making its folders, so a crash or
/// a failed write leaves the file before intact. A failed one takes its temporary file away; one
/// that's written takes away what crashed writes of the same file left ([`sweep`]).
pub fn write_atomic(path: &Path, bytes: impl AsRef<[u8]>) -> Result<(), Error> {
    let folder = path.parent().unwrap_or(Path::new(""));
    fs::create_dir_all(folder).map_err(io_at(folder))?;
    let temporary = temporary(path);
    fs::write(&temporary, bytes)
        .map_err(io_at(&temporary))
        .and_then(|()| fs::rename(&temporary, path).map_err(io_at(path)))
        .inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })?;
    sweep(path);
    Ok(())
}

/// How long another process's temporary file goes unchanged before it's taken as left by a crash
/// rather than still being written.
const STALE: Duration = Duration::from_secs(60);

/// Take away the temporary files other processes left beside `path`, of that file alone, which a
/// crash between writing and renaming leaves. One written to in the last [`STALE`] is left, as
/// another vimtracker may be saving the same file. What can't be taken away is left for next time.
fn sweep(path: &Path) {
    let Some(name) = path.file_name() else {
        return;
    };
    let prefix = format!(".{}.", name.to_string_lossy());
    let folder = match path.parent() {
        Some(folder) if !folder.as_os_str().is_empty() => folder,
        _ => Path::new("."),
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let file = entry.file_name();
        let Some(pid) = file
            .to_str()
            .and_then(|file| file.strip_prefix(&prefix))
            .and_then(|rest| rest.strip_suffix(".tmp"))
        else {
            continue;
        };
        let theirs = !pid.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && pid.parse() != Ok(std::process::id());
        let stale = entry.metadata().is_ok_and(|metadata| {
            metadata.is_file()
                && metadata
                    .modified()
                    .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age >= STALE))
        });
        if theirs && stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn folder(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vt-ron-file-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Thing {
        name: String,
        count: u32,
    }

    fn thing(count: u32) -> Thing {
        Thing {
            name: "kick".into(),
            count,
        }
    }

    /// A value that fails partway through being written out.
    struct Refuses;

    impl Serialize for Refuses {
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("refused"))
        }
    }

    #[test]
    fn a_file_reads_back_as_written_and_leaves_nothing_beside_it() {
        let root = folder("back");
        let path = root.join("deep/thing.ron");
        write(&path, &thing(3)).unwrap();
        assert_eq!(read::<Thing>(&path).unwrap(), thing(3));
        assert_eq!(
            read_text(&path).unwrap(),
            ron::ser::to_string_pretty(&thing(3), ron::ser::PrettyConfig::default()).unwrap()
        );
        let beside: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(beside.len(), 1, "no temporary file left");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_interrupted_write_leaves_the_file_before() {
        let root = folder("interrupted");
        let path = root.join("thing.ron");
        write(&path, &thing(1)).unwrap();

        // Stopped while writing out the value: nothing is touched.
        assert!(matches!(
            write(&path, &Refuses),
            Err(Error::Serialize { .. })
        ));
        assert_eq!(read::<Thing>(&path).unwrap(), thing(1));

        // Stopped while writing the file, by a folder in the temporary file's place.
        fs::create_dir(temporary(&path)).unwrap();
        assert!(matches!(write(&path, &thing(2)), Err(Error::Io { .. })));
        assert_eq!(read::<Thing>(&path).unwrap(), thing(1));
        fs::remove_dir(temporary(&path)).unwrap();

        // Stopped before the rename, as a crash would, leaving half of the new one.
        fs::write(temporary(&path), "(name: \"ki").unwrap();
        assert_eq!(read::<Thing>(&path).unwrap(), thing(1));
        write(&path, &thing(2)).unwrap();
        assert_eq!(read::<Thing>(&path).unwrap(), thing(2));
        assert!(!temporary(&path).exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_write_takes_away_what_crashed_writes_of_that_file_left() {
        let root = folder("sweep");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("thing.ron");
        // Half a file, last written `age` seconds ago.
        let left = |name: &str, age: u64| {
            let left = root.join(name);
            fs::write(&left, "(name: \"ki").unwrap();
            let then = std::time::SystemTime::now() - Duration::from_secs(age);
            let file = fs::File::options().write(true).open(&left).unwrap();
            file.set_modified(then).unwrap();
            left
        };
        let crashed = left(".thing.ron.4000000001.tmp", 3600);
        let saving = left(".thing.ron.4000000002.tmp", 0);
        let another_file = left(".other.ron.4000000001.tmp", 3600);
        let not_a_process = left(".thing.ron.x1.tmp", 3600);
        let longer_name = left(".thing.ron.bak.1.tmp", 3600);
        write(&path, &thing(1)).unwrap();
        assert!(!crashed.exists(), "another process's old one is taken away");
        assert!(saving.exists(), "one still being written is left");
        assert!(another_file.exists(), "another file's is left");
        assert!(not_a_process.exists(), "only a process's number");
        assert!(longer_name.exists(), "only this file's");
        assert_eq!(read::<Thing>(&path).unwrap(), thing(1));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn the_format_is_read_alone_so_a_wrong_one_is_refused_before_its_fields() {
        let path = Path::new("song.ron");
        let text = "(tracks: [(format: 3, wobble: true)], format_version: 10, later: Some([1]))";
        assert_eq!(probe(text, path, "format_version").unwrap(), 10);
        // The format a reader of 11 refuses, from a file the whole reading would fail on.
        assert!(parse::<Thing>(text, path).is_err());
        assert_eq!(
            probe("(format: 2, sound: ())", path, "format").unwrap(),
            2,
            "a preset's field"
        );
        for broken in [
            "(tracks: [])",
            "(format_version: 1, format_version: 2)",
            "(format",
        ] {
            match probe(broken, path, "format_version") {
                Err(Error::Parse { path, .. }) => assert_eq!(path, "song.ron"),
                other => panic!("{broken} read as {other:?}"),
            }
        }
    }

    #[test]
    fn what_wont_read_names_its_file() {
        let root = folder("names");
        let path = root.join("thing.ron");
        let missing = read::<Thing>(&path).unwrap_err();
        assert!(
            matches!(&missing, Error::Io { source, .. } if source.kind() == io::ErrorKind::NotFound)
        );
        write_atomic(&path, "(name: 3)").unwrap();
        let error = read::<Thing>(&path).unwrap_err().to_string();
        assert!(error.starts_with(&path.display().to_string()), "{error}");
        fs::remove_dir_all(&root).unwrap();
    }
}
