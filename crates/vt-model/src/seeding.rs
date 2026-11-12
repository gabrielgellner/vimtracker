//! Seeds from the file (#279): the slices, trim, loop, gain, root note and tempo that other tools
//! leave in a WAV's marker chunks or in an Octatrack `.ot` beside it, turned into a bank entry's
//! points and hints when the file joins the bank. A seed is read once, and the song's copy wins
//! after that. Nothing is written to or beside the file.
//!
//! An `.ot` beside the file seeds its points, as it's what the Octatrack saved of it on purpose;
//! otherwise its `cue ` points are its slices and its `smpl` loop its loop. Its root note comes
//! from its `smpl` or `inst` chunk either way.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::bank::{Hints, Loop, Points, SampleEdit, is_mark};
use crate::octatrack::{OtFile, OtLoop};
use crate::riff::{self, Markers};
use crate::step::Note;

/// What a file says of how it plays, to seed its bank entry with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileSeed {
    pub points: Points,
    pub hints: Hints,
    /// In decibels, an `.ot`'s, if it isn't 0.
    pub gain_db: Option<f32>,
    /// Where the points came from, as `"its markers"` or `"kick.ot"`, in words for the status line.
    pub from: Vec<String>,
    /// Each marker chunk or `.ot` that couldn't be read, and was left out, in words.
    pub problems: Vec<String>,
}

impl FileSeed {
    /// Whether the file says nothing of how it plays.
    pub fn is_empty(&self) -> bool {
        self.points.is_whole() && self.hints.is_empty() && self.gain_db.is_none()
    }

    /// Give an entry's edit what the file says.
    pub fn apply(&self, edit: &mut SampleEdit) {
        edit.points = self.points.clone();
        edit.hints = self.hints;
        if let Some(gain) = self.gain_db {
            edit.play.gain_db = gain;
        }
    }

    /// What it seeds, in words, as `16 slices and a loop, from its markers`.
    pub fn describe(&self) -> String {
        let points = &self.points;
        let mut what = Vec::new();
        match points.slices.len() {
            0 => {}
            1 => what.push("1 slice".to_owned()),
            count => what.push(format!("{count} slices")),
        }
        match (points.start > 0, points.end.is_some()) {
            (true, true) => what.push("a start and end".to_owned()),
            (true, false) => what.push("a start".to_owned()),
            (false, true) => what.push("an end".to_owned()),
            (false, false) => {}
        }
        if points.loop_.is_some() {
            what.push("a loop".to_owned());
        }
        if let Some(gain) = self.gain_db {
            what.push(format!("a gain of {gain:+.1} dB"));
        }
        if let Some(root) = self.hints.root {
            what.push(format!("its root {}", Note::On(root)));
        }
        if let Some(tempo) = self.hints.millibpm {
            what.push(format!("{} BPM", f64::from(tempo) / 1000.0));
        }
        format!("{}, from {}", and_list(&what), and_list(&self.from))
    }
}

/// Words joined as a list is said: `a`, `a and b`, `a, b and c`.
fn and_list(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The Octatrack's `.ot` beside a file, with the same name, if there is one.
pub fn ot_beside(file: &Path) -> Option<PathBuf> {
    ["ot", "OT"]
        .into_iter()
        .map(|extension| file.with_extension(extension))
        .find(|path| path.is_file())
}

/// What a file says of how it plays, from its bytes and an `.ot` beside it, for a file whose audio
/// is this many frames long at its own rate. A WAV's own length is taken from its chunks when it
/// gives it.
pub fn seed(file: &Path, bytes: &[u8], frames: u64) -> FileSeed {
    let wav = riff::read(bytes);
    let frames = wav.frames.unwrap_or(frames);
    let mut seed = FileSeed {
        problems: wav.problems,
        ..FileSeed::default()
    };
    seed.hints.root = wav.markers.root;
    let ot = ot_beside(file).map(|path| {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let read = fs::read(&path).map_err(|error: io::Error| error.to_string());
        (
            name,
            read.and_then(|bytes| OtFile::read(&bytes).map_err(|e| e.to_string())),
        )
    });
    match ot {
        Some((name, Ok(ot))) => {
            seed.points = ot_points(&ot, frames);
            seed.gain_db = Some(ot.gain_db()).filter(|gain| *gain != 0.0);
            seed.hints.millibpm = ot.millibpm();
            seed.from.push(name);
        }
        Some((name, Err(problem))) => {
            seed.problems
                .push(format!("the {name} beside it is skipped: {problem}"));
            seed.points = wav_points(&wav.markers, frames, &mut seed.problems);
        }
        None => seed.points = wav_points(&wav.markers, frames, &mut seed.problems),
    }
    let from_ot = !seed.from.is_empty();
    if seed.hints.root.is_some() || (!from_ot && !seed.points.is_whole()) {
        seed.from.push("its markers".to_owned());
    }
    seed
}

/// The points a WAV's markers give, in a file this many frames long: each cue point starts a
/// slice, and the first `smpl` loop is the loop. A loop at least as long as what follows it is
/// taken as a trim as well, as the Deluge takes it, so the entry ends where it does.
pub fn wav_points(markers: &Markers, frames: u64, problems: &mut Vec<String>) -> Points {
    let mut points = Points::default();
    if let Some(first) = markers.loops.first() {
        let end = first.end.min(frames);
        if first.start < end {
            points.loop_ = Some(Loop {
                start: first.start,
                end,
                crossfade: 0,
            });
            if end < frames && end - first.start >= frames - end {
                points.end = Some(end);
            }
        } else {
            problems.push("its `smpl` loop isn't inside its audio, so it's skipped".to_owned());
        }
    }
    let starts: Vec<u64> = markers.cues.iter().map(|cue| cue.frame).collect();
    slice(&mut points, &starts, frames);
    for cue in &markers.cues {
        let Some(letter) = cue.label.as_deref().and_then(mark_letter) else {
            continue;
        };
        let at = points.slice_at(cue.frame);
        let starts_here = points.slices.get(at).is_some_and(|s| s.start == cue.frame);
        if starts_here && !points.marks.contains_key(&letter) {
            points.set_mark(letter, at);
        }
    }
    points
}

/// The letter a cue's label names a slice by, if it's one letter, as `a` or `A`.
fn mark_letter(label: &str) -> Option<char> {
    let mut letters = label.chars();
    let letter = letters.next()?.to_ascii_lowercase();
    (letters.next().is_none() && is_mark(letter)).then_some(letter)
}

/// The points an `.ot` gives, in a file this many frames long: its trim is the entry's start and
/// end, its loop runs from its loop start to the trim's end, and its slices start where its
/// slices do. Its slices' own ends and loops are left, as an entry's slice ends where the next
/// starts.
pub fn ot_points(ot: &OtFile, frames: u64) -> Points {
    let mut points = Points::default();
    let (start, end) = (u64::from(ot.trim_start), u64::from(ot.trim_end).min(frames));
    if start < end {
        points.start = start;
        points.end = (end < frames).then_some(end);
    }
    let end = points.end_in(frames);
    let loop_start = u64::from(ot.loop_start);
    if ot.loop_mode != OtLoop::Off && (points.start..end).contains(&loop_start) {
        points.loop_ = Some(Loop {
            start: loop_start,
            end,
            crossfade: 0,
        });
    }
    let starts: Vec<u64> = ot
        .slices
        .iter()
        .map(|slice| u64::from(slice.start))
        .collect();
    slice(&mut points, &starts, frames);
    points
}

/// Slice the entry at these frames, with a slice from its start if none starts there, so its
/// slices cover it as `:slice` and `Enter` cut it. Frames outside it are left out, and one slice
/// alone is no slicing.
fn slice(points: &mut Points, starts: &[u64], frames: u64) {
    let end = points.end_in(frames);
    let mut starts: Vec<u64> = starts
        .iter()
        .copied()
        .filter(|start| (points.start..end).contains(start))
        .collect();
    if starts.is_empty() {
        return;
    }
    starts.push(points.start);
    starts.sort_unstable();
    starts.dedup();
    if starts.len() > 1 {
        points.reslice(&starts);
    }
}
