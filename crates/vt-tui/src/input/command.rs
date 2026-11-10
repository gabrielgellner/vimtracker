//! The `:` command line.

use std::path::PathBuf;
use vt_model::scale::{ScaleKind, parse_root};

use thiserror::Error;
use vt_model::bank::{MAX_CENTS, MAX_SLICES, MIN_NORMALIZE, MOST_PAN, is_mark};
use vt_model::constants::{MAX_GROUPS, MAX_LABEL_CHARS};
use vt_model::groove::{GrooveId, SWING_PERCENTS};
use vt_model::kit::MAX_TUNE;
use vt_model::label::is_name;
use vt_model::order::{ORDER_WORDS, Order};
use vt_model::song::{LAUNCH_WORDS, Launch};

/// A parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// Save, or save to a new folder. With no folder yet, the explorer chooses one.
    Write(Option<PathBuf>),
    /// Open a project, or choose one in the explorer.
    Edit(Option<PathBuf>),
    Quit {
        force: bool,
    },
    /// Start a project, or choose where in the explorer.
    New(Option<PathBuf>),
    /// Put a sample on this track, or browse a folder of them, or choose one in the explorer.
    Sample(Option<PathBuf>),
    /// Change where the sample this track plays starts, ends or loops, or how it plays.
    SampleChange(SampleChange),
    /// Slice the sample this track plays, or change how one of its slices plays.
    Slice(SliceChange),
    /// Name a slice of the sample this track plays with a letter: one by its number, or the one
    /// here.
    Mark {
        letter: char,
        slice: Option<u8>,
    },
    /// List the letters naming slices of the sample this track plays.
    Marks,
    /// Take these letters off the slices of the sample this track plays, or every letter.
    DelMarks(Option<Vec<char>>),
    /// Make a kit's sounds the ones to choose from, by name or path, or choose a kit.
    Kit(Option<PathBuf>),
    /// List the bank, to choose what this track plays.
    Bank,
    /// Remove the bank's samples nothing plays.
    BankClean,
    /// Bring this track's sample to a peak level, in hundredths of a dBFS, or stop normalising it.
    Normalize(Option<i16>),
    /// Start and end this track's sample where its sound does.
    Trim,
    /// Put a kit's sounds on tracks, by name or path.
    KitTracks(PathBuf),
    /// Name this track, or clear its name.
    Name(Option<String>),
    /// Keep playing the section being edited, or go back to playing the song.
    Loop,
    Bpm(u32),
    /// A loop's length.
    Len(usize),
    /// Give the cursor's FX column a lane this long, or none to follow the notes again.
    Lane(Option<usize>),
    /// Play this phrase's rows in an order, or choose one from a list.
    Order(Option<Order>),
    TrackAdd(usize),
    TrackDel,
    /// Put tracks in a group, from 1 to `MAX_GROUPS`, or take them out of any: those numbered
    /// from the first to the last, counted from 1, or the cursor's.
    Group {
        group: Option<u8>,
        tracks: Option<(usize, usize)>,
    },
    Mute,
    Solo,
    Double,
    SetSkip(u32),
    SetLimiter(bool),
    SetScale(ScaleKind),
    SetRoot(u8),
    ScaleOn(bool),
    /// Choose a kind of scale from a list.
    PickScale,
    /// Move this phrase's notes to the nearest in the scale.
    FitToScale,
    Seed(u64),
    /// The groove every track plays with unless it has its own.
    Groove(GrooveId),
    /// Choose the project's groove from a list, or the track's.
    PickGroove {
        track: bool,
    },
    Chain(u32),
    /// Render the loop, into a file the explorer chooses the folder of if there's no path.
    Render {
        path: Option<PathBuf>,
        rows: Option<usize>,
        tail: bool,
        /// Write one file per group, and per track in none, into the path, as a folder.
        stems: bool,
        /// Write the stems of each track instead, those in groups too.
        each_track: bool,
    },
    /// This track's own groove, or none to follow the project's.
    TrackGroove(Option<GrooveId>),
    TrackLevel(u8),
    Reset(Option<usize>),
    /// Set when launched chains start, or show it.
    Launch(Option<Launch>),
    /// Label the chain under the cursor, or clear its label.
    Label(Option<String>),
}

/// A change to the sample the current track plays, its bank entry, from `:sample` and a word.
/// Times are in microseconds, so a line compares exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleChange {
    /// Where it starts.
    Start(u64),
    /// Where it ends, or none for the file's end.
    End(Option<u64>),
    /// Its loop's start and end and its crossfade, or none.
    Loop(Option<(u64, u64, u64)>),
    /// Whether it plays reversed, or the other way to how it is.
    Reverse(Option<bool>),
    /// How long it fades in and out.
    Fade(u64, u64),
    /// Its gain, in hundredths of a dB.
    Gain(i32),
    /// Its tune, in semitones and cents.
    Tune(i8, i8),
}

/// The words `:sample` takes to change the track's sample, rather than a file to load.
pub const SAMPLE_WORDS: [&str; 7] = ["start", "end", "loop", "reverse", "fade", "gain", "tune"];

/// A change to the slices of the sample the current track plays, from `:slice`. A slice's own
/// setting that's none plays as the entry does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceChange {
    /// Cut it into this many slices of the same length, as `:slice 16` does.
    Even(usize),
    /// Take its slices away, so it plays as one.
    Clear,
    /// The slice's gain, in hundredths of a dB.
    Gain(Option<i32>),
    /// Its pan, from 50 to the left, below 0, to 50 to the right.
    Pan(Option<i8>),
    /// Its tune, in semitones.
    Tune(Option<i8>),
    /// Whether it plays reversed.
    Reverse(Option<bool>),
    /// How long it fades in and out, in microseconds.
    Fade(Option<(u64, u64)>),
}

/// The words `:slice` takes, beside a number of slices.
pub const SLICE_WORDS: [&str; 6] = ["clear", "gain", "pan", "tune", "reverse", "fade"];

/// The most an entry's gain can be moved by `:sample gain`, in dB either way.
pub const MOST_SAMPLE_DB: f32 = 24.0;

/// A time as `:sample` takes it, in microseconds: seconds, as `1.5` or `1.5s`, or milliseconds,
/// as `250ms`.
fn time(text: &str) -> Option<u64> {
    let (number, scale) = match text.strip_suffix("ms") {
        Some(number) => (number, 1000.0),
        None => (text.strip_suffix('s').unwrap_or(text), 1_000_000.0),
    };
    let value: f64 = number.parse().ok()?;
    (value.is_finite() && value >= 0.0).then(|| (value * scale).round() as u64)
}

/// Read `:sample`'s arguments: a word and what it takes, or else a file to load.
fn sample(argument: &str) -> Result<Line, LineError> {
    let mut words = argument.split_whitespace();
    let word = words.next().unwrap_or("");
    if !SAMPLE_WORDS.contains(&word) {
        return Ok(Line::Sample(optional_path(argument)));
    }
    let rest: Vec<&str> = words.collect();
    let times = |texts: &[&str]| {
        texts
            .iter()
            .map(|text| time(text))
            .collect::<Option<Vec<_>>>()
    };
    let change = match (word, &rest[..]) {
        ("start", []) => Some(SampleChange::Start(0)),
        ("start", [at]) => time(at).map(SampleChange::Start),
        ("end", []) => Some(SampleChange::End(None)),
        ("end", [at]) => time(at).map(|at| SampleChange::End(Some(at))),
        ("loop", ["off"]) => Some(SampleChange::Loop(None)),
        ("loop", [_, _] | [_, _, _]) => times(&rest).map(|found| {
            SampleChange::Loop(Some((
                found[0],
                found[1],
                found.get(2).copied().unwrap_or(0),
            )))
        }),
        ("reverse", []) => Some(SampleChange::Reverse(None)),
        ("reverse", ["on"]) => Some(SampleChange::Reverse(Some(true))),
        ("reverse", ["off"]) => Some(SampleChange::Reverse(Some(false))),
        ("fade", [] | ["off"]) => Some(SampleChange::Fade(0, 0)),
        ("fade", [_, _]) => times(&rest).map(|found| SampleChange::Fade(found[0], found[1])),
        ("gain", [db]) => db
            .trim_end_matches("dB")
            .parse::<f32>()
            .ok()
            .filter(|db| db.abs() <= MOST_SAMPLE_DB)
            .map(|db| SampleChange::Gain((db * 100.0).round() as i32)),
        ("tune", [semitones] | [semitones, _]) => {
            let semitones = semitones.parse::<i8>().ok().filter(|s| s.abs() <= MAX_TUNE);
            let cents = match rest.get(1) {
                Some(cents) => cents.parse::<i8>().ok().filter(|c| c.abs() <= MAX_CENTS),
                None => Some(0),
            };
            semitones.zip(cents).map(|(s, c)| SampleChange::Tune(s, c))
        }
        _ => None,
    };
    change
        .map(Line::SampleChange)
        .ok_or(LineError::SampleChange)
}

/// Read `:mark`'s arguments: a letter, and a slice's number in hex, as `SLI` shows it, or nothing
/// for the slice here.
fn mark(argument: &str) -> Result<Line, LineError> {
    let (letter, slice) = match argument.split_whitespace().collect::<Vec<_>>()[..] {
        [letter] => (letter, None),
        [letter, slice] => (
            letter,
            Some(u8::from_str_radix(slice, 16).map_err(|_| LineError::Mark)?),
        ),
        _ => return Err(LineError::Mark),
    };
    let mut letters = letter.chars();
    match (letters.next(), letters.next()) {
        (Some(letter), None) if is_mark(letter) => Ok(Line::Mark { letter, slice }),
        _ => Err(LineError::Mark),
    }
}

/// Read `:delmarks`'s letters, as Vim's are: each alone, run together, or a range, `a-c`, with
/// spaces between or not.
fn delmarks(argument: &str) -> Result<Line, LineError> {
    let text: Vec<char> = argument.chars().filter(|c| !c.is_whitespace()).collect();
    let mut letters = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let from = text[at];
        if !is_mark(from) {
            return Err(LineError::DelMarks);
        }
        match text.get(at + 1..at + 3) {
            Some(['-', to]) if is_mark(*to) && from <= *to => {
                letters.extend(from..=*to);
                at += 3;
            }
            Some(['-', _]) => return Err(LineError::DelMarks),
            _ => {
                letters.push(from);
                at += 1;
            }
        }
    }
    if letters.is_empty() {
        return Err(LineError::DelMarks);
    }
    letters.sort_unstable();
    letters.dedup();
    Ok(Line::DelMarks(Some(letters)))
}

/// Read `:slice`'s arguments: a number of slices, `clear`, or one of a slice's own settings and
/// its value, where no value plays it as the entry does.
fn slice(argument: &str) -> Result<Line, LineError> {
    let words: Vec<&str> = argument.split_whitespace().collect();
    let change = match &words[..] {
        [count] if steps(count).is_some() => steps(count)
            .filter(|count| *count <= MAX_SLICES)
            .map(SliceChange::Even),
        ["clear"] => Some(SliceChange::Clear),
        ["gain"] => Some(SliceChange::Gain(None)),
        ["gain", db] => db
            .trim_end_matches("dB")
            .parse::<f32>()
            .ok()
            .filter(|db| db.abs() <= MOST_SAMPLE_DB)
            .map(|db| SliceChange::Gain(Some((db * 100.0).round() as i32))),
        ["pan"] => Some(SliceChange::Pan(None)),
        ["pan", pan] => pan
            .parse::<i8>()
            .ok()
            .filter(|pan| pan.abs() <= MOST_PAN)
            .map(|pan| SliceChange::Pan(Some(pan))),
        ["tune"] => Some(SliceChange::Tune(None)),
        ["tune", semitones] => semitones
            .parse::<i8>()
            .ok()
            .filter(|semitones| semitones.abs() <= MAX_TUNE)
            .map(|semitones| SliceChange::Tune(Some(semitones))),
        ["reverse"] => Some(SliceChange::Reverse(None)),
        ["reverse", "on"] => Some(SliceChange::Reverse(Some(true))),
        ["reverse", "off"] => Some(SliceChange::Reverse(Some(false))),
        ["fade"] => Some(SliceChange::Fade(None)),
        ["fade", "off"] => Some(SliceChange::Fade(Some((0, 0)))),
        ["fade", fade_in, fade_out] => time(fade_in)
            .zip(time(fade_out))
            .map(|fades| SliceChange::Fade(Some(fades))),
        _ => None,
    };
    change.map(Line::Slice).ok_or(LineError::Slice)
}

/// Why a command line was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LineError {
    #[error("unknown command: {0}")]
    Unknown(String),
    #[error(":name takes two characters, the first a letter, or nothing to clear it")]
    Name,
    #[error(":bpm takes a number from 20 to 400")]
    Bpm,
    #[error(":len takes a number of steps, 1 or more, with 0x for hex: 0x10")]
    Len,
    #[error(":lane takes a number of steps, 1 or more, or nothing to follow the notes again")]
    Lane,
    #[error(
        ":order takes forward, reverse, up-down, up-down-inc, down-up, down-up-inc, odd-even, converge, diverge, pedal, shuffle or random, or nothing to choose"
    )]
    Order,
    #[error(":bank takes clean, or nothing to list the bank")]
    Bank,
    #[error(
        ":sample takes a file, or start, end, loop from to [crossfade] or off, reverse, fade in out, gain dB or tune st [ct], with times as 1.5 or 250ms"
    )]
    SampleChange,
    #[error(
        ":slice takes a number of slices up to 256, clear, or gain dB, pan -50 to 50, tune st, reverse on or off, or fade in out, each alone to play as the sample"
    )]
    Slice,
    #[error(
        ":mark takes a letter, a to z, and a slice's number in hex, or none for the slice here"
    )]
    Mark,
    #[error(":delmarks takes letters, as a b, abc or a-c, and :delmarks! takes every one away")]
    DelMarks,
    #[error(":normalize takes a peak level in dBFS from -60 to 0, such as -0.3, or off")]
    Normalize,
    #[error(":track takes add [n] or del")]
    Track,
    #[error(
        ":group takes a group from 1 to {MAX_GROUPS} or off, then tracks, as 1-4, or none for this one"
    )]
    Group,
    #[error(":set takes skip=n, limiter=on or off, scale=dorian, on or off, or root=d")]
    Set,
    #[error(":scale takes fit, or nothing to choose from a list")]
    Scale,
    #[error(":chain takes a chain ID in hex")]
    Chain,
    #[error(":render takes a path, --rows n, --tail, --stems and --split")]
    Render,
    #[error(":seed takes a number")]
    Seed,
    #[error(":groove takes a groove's name, such as swing-66, or nothing to choose from a list")]
    Groove,
    #[error(":tgroove takes a groove's name, such as mpc-58, off, or nothing to choose")]
    TrackGroove,
    #[error(":swing takes 50, 52, 54, 56, 58, 60, 62, 64, 66 or 70, a swing groove's percent")]
    Swing,
    #[error(":tvol takes a level in hex, from 00 to FF")]
    TrackLevel,
    #[error(":reset takes a number of steps, 1 or more, with 0x for hex, or off")]
    Reset,
    #[error(":launch takes step, 1/4, 1/2, 1bar, 2bar, 4bar, 8bar, a number of steps, or chain")]
    Launch,
    #[error(":label takes up to 32 characters, or nothing to clear it")]
    Label,
}

/// Expand a leading `~/` using the home directory.
fn path(text: &str) -> PathBuf {
    crate::config::expand_home(text)
}

/// Read a tempo as thousandths of a BPM, so `137.63` is exact.
fn millibpm(text: &str) -> Option<u32> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || (whole.is_empty() && fraction.is_empty()) {
        return None;
    }
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if !digits(whole) || !digits(fraction) {
        return None;
    }
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole.parse().ok()?
    };
    let scaled: u32 = format!("{fraction:0<3}").parse().ok()?;
    whole.checked_mul(1000)?.checked_add(scaled)
}

/// Read a number of steps, 1 or more: decimal, or hex after `0x` as in Rust, since rows are numbered
/// in hex on screen.
fn steps(text: &str) -> Option<usize> {
    let parsed = match text.strip_prefix("0x") {
        Some(digits) => usize::from_str_radix(digits, 16).ok(),
        None => text.parse().ok(),
    };
    parsed.filter(|steps| *steps > 0)
}

/// The swing groove at a percent, which `:swing` is a shorthand for: 50 is straight.
pub(crate) fn swing(text: &str) -> Option<GrooveId> {
    let percent: u16 = text.trim_end_matches('%').parse().ok()?;
    debug_assert!(SWING_PERCENTS.contains(&66));
    GrooveId::swing(percent)
}

/// A groove's name as the command line writes it: lower case, with a hyphen for each space, so
/// `Swing 66` is `swing-66`.
pub fn groove_word(id: GrooveId) -> String {
    id.name().to_lowercase().replace(' ', "-")
}

/// Read `:group`'s arguments: a group or `off`, then the tracks, by their numbers as one or a
/// range, as `3` or `1-4`, or nothing for the cursor's.
fn group(argument: &str) -> Result<Line, LineError> {
    let mut words = argument.split_whitespace();
    let group = match words.next().ok_or(LineError::Group)? {
        "off" => None,
        number => Some(
            number
                .parse::<u8>()
                .ok()
                .filter(|group| (1..=MAX_GROUPS).contains(&usize::from(*group)))
                .ok_or(LineError::Group)?,
        ),
    };
    let number = |text: &str| text.parse::<usize>().ok().filter(|track| *track > 0);
    let tracks = match words.next() {
        None => None,
        Some(range) => {
            let (first, last) = range.split_once('-').unwrap_or((range, range));
            let (first, last) = number(first).zip(number(last)).ok_or(LineError::Group)?;
            Some((first.min(last), first.max(last)))
        }
    };
    if words.next().is_some() {
        return Err(LineError::Group);
    }
    Ok(Line::Group { group, tracks })
}

/// `:render`'s flags, which its parser reads and Tab completes. `--split` renders stems of each
/// track rather than each group, so it means `--stems` too.
pub const RENDER_FLAGS: [&str; 4] = ["--rows", "--tail", "--stems", "--split"];

/// Read `:render`'s arguments. Anything that isn't a flag is the path, which can hold spaces.
fn render(argument: &str) -> Result<Line, LineError> {
    let [rows_flag, tail_flag, stems_flag, split_flag] = RENDER_FLAGS;
    let mut words = Vec::new();
    let mut rows = None;
    let mut tail = false;
    let mut stems = false;
    let mut each_track = false;
    let mut parts = argument.split_whitespace();
    while let Some(part) = parts.next() {
        if part == tail_flag {
            tail = true;
        } else if part == stems_flag {
            stems = true;
        } else if part == split_flag {
            stems = true;
            each_track = true;
        } else if part == rows_flag {
            let count = parts.next().and_then(|n| n.parse().ok());
            rows = Some(count.filter(|n| *n > 0).ok_or(LineError::Render)?);
        } else {
            words.push(part);
        }
    }
    Ok(Line::Render {
        path: (!words.is_empty()).then(|| path(&words.join(" "))),
        rows,
        tail,
        stems,
        each_track,
    })
}

/// What comes after a command's name, or after a `:set` key's `=`, which says what Tab completes
/// there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Takes {
    /// Nothing, or a number or text that nothing completes.
    Nothing,
    /// One of these words, or for some commands a number in their place.
    Words(&'static [&'static str]),
    /// A project's folder.
    Folder,
    /// A sample file.
    Sample,
    /// A WAV file, or a folder for stems, then `:render`'s flags.
    Render,
    /// A kit by name, from the kit folders, or by path, then `all`.
    Kit,
    /// One of [`SETTINGS`], as `key=value`.
    Setting,
    /// `on`, `off`, or a kind of scale.
    Scale,
    /// A root note.
    Root,
    /// A groove's name, and for some commands `off`.
    Groove { off: bool },
}

/// A command: its name, what it does, what it takes, and how its argument is read.
#[derive(Debug, Clone, Copy)]
pub struct Command {
    pub name: &'static str,
    pub meaning: &'static str,
    pub takes: Takes,
    /// Read the argument, already trimmed.
    parse: fn(&str) -> Result<Line, LineError>,
}

/// A `:set` key: its name, what it sets, what its value is, and how the value is read.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    pub key: &'static str,
    pub meaning: &'static str,
    pub takes: Takes,
    parse: fn(&str) -> Option<Line>,
}

/// `on` and `off`, for settings that turn something on.
const ON_OFF: &[&str] = &["on", "off"];

/// Every `:set` key, which the parser reads and Tab completes.
pub const SETTINGS: [Setting; 4] = [
    Setting {
        key: "skip",
        meaning: "Steps the arrows jump in Insert",
        takes: Takes::Nothing,
        parse: |value| value.parse().ok().map(Line::SetSkip),
    },
    Setting {
        key: "limiter",
        meaning: "The limiter on the output",
        takes: Takes::Words(ON_OFF),
        parse: |value| match value {
            "on" => Some(Line::SetLimiter(true)),
            "off" => Some(Line::SetLimiter(false)),
            _ => None,
        },
    },
    Setting {
        key: "scale",
        meaning: "The kind of scale, or on or off",
        takes: Takes::Scale,
        parse: |value| match value {
            "on" => Some(Line::ScaleOn(true)),
            "off" => Some(Line::ScaleOn(false)),
            kind => ScaleKind::parse(kind).map(Line::SetScale),
        },
    },
    Setting {
        key: "root",
        meaning: "The scale's root note",
        takes: Takes::Root,
        parse: |value| parse_root(value).map(Line::SetRoot),
    },
];

/// A path that can be left out.
fn optional_path(argument: &str) -> Option<PathBuf> {
    (!argument.is_empty()).then(|| path(argument))
}

/// Every command, in the order Tab lists them, which the parser reads and Tab completes.
pub const COMMANDS: [Command; 38] = [
    Command {
        name: "w",
        meaning: "Save, or save to a new folder",
        takes: Takes::Folder,
        parse: |argument| Ok(Line::Write(optional_path(argument))),
    },
    Command {
        name: "e",
        meaning: "Open a project",
        takes: Takes::Folder,
        parse: |argument| Ok(Line::Edit(optional_path(argument))),
    },
    Command {
        name: "new",
        meaning: "Start a new project",
        takes: Takes::Folder,
        parse: |argument| Ok(Line::New(optional_path(argument))),
    },
    Command {
        name: "q",
        meaning: "Quit",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Quit { force: false }),
    },
    Command {
        name: "q!",
        meaning: "Quit and discard changes",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Quit { force: true }),
    },
    Command {
        name: "render",
        meaning: "Render the loop to a WAV",
        takes: Takes::Render,
        parse: render,
    },
    Command {
        name: "sample",
        meaning: "Load a sample on this track, or set where it starts, ends and loops",
        takes: Takes::Sample,
        parse: sample,
    },
    Command {
        name: "slice",
        meaning: "Slice this track's sample into so many, or clear, or set how a slice plays",
        takes: Takes::Words(&SLICE_WORDS),
        parse: slice,
    },
    Command {
        name: "mark",
        meaning: "Name a slice of this track's sample with a letter, a to z",
        takes: Takes::Nothing,
        parse: mark,
    },
    Command {
        name: "marks",
        meaning: "List the letters naming slices of this track's sample",
        takes: Takes::Nothing,
        parse: |argument| match argument {
            "" => Ok(Line::Marks),
            _ => Err(LineError::Unknown(format!("marks {argument}"))),
        },
    },
    Command {
        name: "delmarks",
        meaning: "Take letters off the slices of this track's sample, as a-c",
        takes: Takes::Nothing,
        parse: delmarks,
    },
    Command {
        name: "delmarks!",
        meaning: "Take every letter off the slices of this track's sample",
        takes: Takes::Nothing,
        parse: |argument| match argument {
            "" => Ok(Line::DelMarks(None)),
            _ => Err(LineError::DelMarks),
        },
    },
    Command {
        name: "kit",
        meaning: "Choose a kit, or put one on tracks with all",
        takes: Takes::Kit,
        parse: |argument| {
            Ok(match argument.strip_suffix(KIT_ALL) {
                Some(kit) if kit.ends_with(' ') => Line::KitTracks(path(kit.trim_end())),
                _ => Line::Kit(optional_path(argument)),
            })
        },
    },
    Command {
        name: "bank",
        meaning: "List the project's samples, or clean to remove those nothing plays",
        takes: Takes::Words(&["clean"]),
        parse: |argument| match argument {
            "" => Ok(Line::Bank),
            "clean" => Ok(Line::BankClean),
            _ => Err(LineError::Bank),
        },
    },
    Command {
        name: "normalize",
        meaning: "Bring this track's sample to a peak level in dBFS, 0 if none, or off",
        takes: Takes::Words(&["off"]),
        parse: |argument| match argument {
            "" => Ok(Line::Normalize(Some(0))),
            "off" => Ok(Line::Normalize(None)),
            _ => argument
                .parse::<f32>()
                .ok()
                .filter(|level| (MIN_NORMALIZE..=0.0).contains(level))
                .map(|level| Line::Normalize(Some((level * 100.0).round() as i16)))
                .ok_or(LineError::Normalize),
        },
    },
    Command {
        name: "trim",
        meaning: "Start and end this track's sample where its sound does",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Trim),
    },
    Command {
        name: "set",
        meaning: "Set skip, limiter, scale or root",
        takes: Takes::Setting,
        parse: |argument| {
            let (key, value) = argument.split_once('=').ok_or(LineError::Set)?;
            let setting = SETTINGS.iter().find(|setting| setting.key == key);
            setting
                .and_then(|setting| (setting.parse)(value))
                .ok_or(LineError::Set)
        },
    },
    Command {
        name: "scale",
        meaning: "Choose a scale, or fit notes to it",
        takes: Takes::Words(&["fit"]),
        parse: |argument| match argument {
            "" => Ok(Line::PickScale),
            "fit" => Ok(Line::FitToScale),
            _ => Err(LineError::Scale),
        },
    },
    Command {
        name: "bpm",
        meaning: "The tempo",
        takes: Takes::Nothing,
        parse: |argument| millibpm(argument).map(Line::Bpm).ok_or(LineError::Bpm),
    },
    Command {
        name: "groove",
        meaning: "The project's groove, or choose one",
        takes: Takes::Groove { off: false },
        parse: |argument| match argument {
            "" => Ok(Line::PickGroove { track: false }),
            _ => GrooveId::named(argument)
                .map(Line::Groove)
                .ok_or(LineError::Groove),
        },
    },
    Command {
        name: "tgroove",
        meaning: "This track's groove, or off to follow the project's",
        takes: Takes::Groove { off: true },
        parse: |argument| match argument {
            "" => Ok(Line::PickGroove { track: true }),
            "off" => Ok(Line::TrackGroove(None)),
            _ => GrooveId::named(argument)
                .map(|groove| Line::TrackGroove(Some(groove)))
                .ok_or(LineError::TrackGroove),
        },
    },
    Command {
        name: "swing",
        meaning: "The project's groove, as a swing percent",
        takes: Takes::Nothing,
        parse: |argument| swing(argument).map(Line::Groove).ok_or(LineError::Swing),
    },
    Command {
        name: "tvol",
        meaning: "This track's level, in hex",
        takes: Takes::Nothing,
        parse: |argument| {
            u8::from_str_radix(argument, 16)
                .map(Line::TrackLevel)
                .map_err(|_| LineError::TrackLevel)
        },
    },
    Command {
        name: "len",
        meaning: "The phrase's or lane's length",
        takes: Takes::Nothing,
        parse: |argument| steps(argument).map(Line::Len).ok_or(LineError::Len),
    },
    Command {
        name: "lane",
        meaning: "Give this FX column a lane",
        takes: Takes::Nothing,
        parse: |argument| match argument {
            "" => Ok(Line::Lane(None)),
            _ => steps(argument)
                .map(|length| Line::Lane(Some(length)))
                .ok_or(LineError::Lane),
        },
    },
    Command {
        name: "order",
        meaning: "The order this phrase's rows play in",
        takes: Takes::Words(&ORDER_WORDS),
        parse: |argument| match argument {
            "" => Ok(Line::Order(None)),
            _ => Order::named(argument)
                .map(|order| Line::Order(Some(order)))
                .ok_or(LineError::Order),
        },
    },
    Command {
        name: "reset",
        meaning: "Start every track again after this many steps, or off",
        takes: Takes::Words(&["off"]),
        parse: |argument| match argument {
            "off" => Ok(Line::Reset(None)),
            _ => steps(argument)
                .map(|steps| Line::Reset(Some(steps)))
                .ok_or(LineError::Reset),
        },
    },
    Command {
        name: "launch",
        meaning: "When launched chains start",
        takes: Takes::Words(&LAUNCH_WORDS),
        parse: |argument| match argument {
            "" => Ok(Line::Launch(None)),
            _ => Launch::named(argument)
                .or_else(|| steps(argument).and_then(Launch::every))
                .map(|launch| Line::Launch(Some(launch)))
                .ok_or(LineError::Launch),
        },
    },
    Command {
        name: "track",
        meaning: "Add tracks, or delete the last",
        takes: Takes::Words(&["add", "del"]),
        parse: |argument| match argument.split_whitespace().collect::<Vec<_>>()[..] {
            ["add"] => Ok(Line::TrackAdd(1)),
            ["add", count] => count
                .parse()
                .ok()
                .filter(|count| *count > 0)
                .map(Line::TrackAdd)
                .ok_or(LineError::Track),
            ["del"] => Ok(Line::TrackDel),
            _ => Err(LineError::Track),
        },
    },
    Command {
        name: "group",
        meaning: "Put tracks in a group, or take them out",
        takes: Takes::Words(&["off"]),
        parse: group,
    },
    Command {
        name: "name",
        meaning: "Name this track",
        takes: Takes::Nothing,
        parse: |argument| match argument {
            "" => Ok(Line::Name(None)),
            _ if is_name(argument) => Ok(Line::Name(Some(argument.to_ascii_uppercase()))),
            _ => Err(LineError::Name),
        },
    },
    Command {
        name: "label",
        meaning: "Label this chain",
        takes: Takes::Nothing,
        parse: |argument| match argument {
            "" => Ok(Line::Label(None)),
            _ if argument.chars().count() <= MAX_LABEL_CHARS => {
                Ok(Line::Label(Some(argument.to_owned())))
            }
            _ => Err(LineError::Label),
        },
    },
    Command {
        name: "chain",
        meaning: "Put a chain on this track",
        takes: Takes::Nothing,
        parse: |argument| {
            u32::from_str_radix(argument, 16)
                .map(Line::Chain)
                .map_err(|_| LineError::Chain)
        },
    },
    Command {
        name: "double",
        meaning: "Double the phrase, notes and all",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Double),
    },
    Command {
        name: "loop",
        meaning: "Keep playing this section, or the song again",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Loop),
    },
    Command {
        name: "mute",
        meaning: "Mute this track",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Mute),
    },
    Command {
        name: "solo",
        meaning: "Solo this track",
        takes: Takes::Nothing,
        parse: |_| Ok(Line::Solo),
    },
    Command {
        name: "seed",
        meaning: "The seed chance FX play from",
        takes: Takes::Nothing,
        parse: |argument| {
            argument
                .parse()
                .map(Line::Seed)
                .map_err(|_| LineError::Seed)
        },
    },
];

/// The word after a kit that puts its sounds on tracks.
pub const KIT_ALL: &str = "all";

/// The command with this name.
pub fn find(name: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.name == name)
}

/// Split a command line, without its leading `:`, into the command's name and its argument.
pub fn split(text: &str) -> (&str, &str) {
    let text = text.trim();
    match text.split_once(char::is_whitespace) {
        Some((name, rest)) => (name, rest.trim()),
        None => (text, ""),
    }
}

/// Parse a command line, without its leading `:`.
pub fn parse(text: &str) -> Result<Line, LineError> {
    let (name, argument) = split(text);
    let command = find(name).ok_or_else(|| LineError::Unknown(name.to_owned()))?;
    (command.parse)(argument)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_takes_a_name_or_a_number_of_steps() {
        let every = |steps| Ok(Line::Launch(Launch::every(steps)));
        assert_eq!(parse("launch"), Ok(Line::Launch(None)));
        assert_eq!(parse("launch step"), every(1));
        assert_eq!(parse("launch 1/4"), every(4));
        assert_eq!(parse("launch 1/2"), every(8));
        assert_eq!(parse("launch 1bar"), every(16));
        assert_eq!(parse("launch 2bar"), every(32));
        assert_eq!(parse("launch 4bar"), every(64));
        assert_eq!(parse("launch 8bar"), every(128));
        assert_eq!(parse("launch 12"), every(12));
        assert_eq!(parse("launch 0x10"), every(16));
        assert_eq!(
            parse("launch chain"),
            Ok(Line::Launch(Some(Launch::ChainEnd)))
        );
        assert_eq!(parse("launch 0"), Err(LineError::Launch));
        assert_eq!(parse("launch soon"), Err(LineError::Launch));
    }

    #[test]
    fn a_label_is_free_text_up_to_a_limit() {
        assert_eq!(parse("label"), Ok(Line::Label(None)));
        assert_eq!(
            parse("label verse bass"),
            Ok(Line::Label(Some("verse bass".to_owned())))
        );
        let longest = "x".repeat(MAX_LABEL_CHARS);
        assert_eq!(
            parse(&format!("label {longest}")),
            Ok(Line::Label(Some(longest.clone())))
        );
        assert_eq!(parse(&format!("label {longest}x")), Err(LineError::Label));
    }

    #[test]
    fn tempos_are_exact_thousandths() {
        assert_eq!(parse("bpm 137.63"), Ok(Line::Bpm(137_630)));
        assert_eq!(parse("bpm 120"), Ok(Line::Bpm(120_000)));
        assert_eq!(parse("bpm 99.5"), Ok(Line::Bpm(99_500)));
        assert_eq!(parse("bpm 99.1234"), Err(LineError::Bpm));
        assert_eq!(parse("bpm fast"), Err(LineError::Bpm));
    }

    #[test]
    fn group_takes_a_group_or_off_and_tracks() {
        let group = |group, tracks| Ok(Line::Group { group, tracks });
        assert_eq!(parse("group 1 1-4"), group(Some(1), Some((1, 4))));
        assert_eq!(parse("group 4 3"), group(Some(4), Some((3, 3))));
        assert_eq!(
            parse("group 2 6-5"),
            group(Some(2), Some((5, 6))),
            "either way round"
        );
        assert_eq!(parse("group 2"), group(Some(2), None), "the cursor's track");
        assert_eq!(parse("group off"), group(None, None));
        assert_eq!(parse("group off 2-3"), group(None, Some((2, 3))));
        for wrong in [
            "group",
            "group 0 1",
            "group 5 1",
            "group 1 0-2",
            "group 1 a-b",
            "group 1 1 2",
        ] {
            assert_eq!(parse(wrong), Err(LineError::Group), "{wrong}");
        }
    }

    #[test]
    fn commands_parse() {
        assert_eq!(parse("w"), Ok(Line::Write(None)));
        assert_eq!(parse("w a/b"), Ok(Line::Write(Some("a/b".into()))));
        assert_eq!(parse("q!"), Ok(Line::Quit { force: true }));
        assert_eq!(parse("len 32"), Ok(Line::Len(32)));
        assert_eq!(parse("seed 42"), Ok(Line::Seed(42)));
        assert_eq!(
            parse("render a b.wav --rows 8 --tail"),
            Ok(Line::Render {
                path: Some("a b.wav".into()),
                rows: Some(8),
                tail: true,
                stems: false,
                each_track: false,
            })
        );
        assert_eq!(
            parse("render out --stems"),
            Ok(Line::Render {
                path: Some("out".into()),
                rows: None,
                tail: false,
                stems: true,
                each_track: false,
            })
        );
        assert_eq!(
            parse("render out --split"),
            Ok(Line::Render {
                path: Some("out".into()),
                rows: None,
                tail: false,
                stems: true,
                each_track: true,
            }),
            "a stem of each track is stems"
        );
        assert_eq!(
            parse("render --tail"),
            Ok(Line::Render {
                path: None,
                rows: None,
                tail: true,
                stems: false,
                each_track: false,
            }),
            "the explorer chooses where"
        );
        assert!(parse("render a.wav --rows 0").is_err());
        assert!(parse("seed x").is_err());
        let groove = |name: &str| GrooveId::named(name).unwrap();
        assert_eq!(parse("swing 66"), Ok(Line::Groove(groove("Swing 66"))));
        assert_eq!(parse("swing 50"), Ok(Line::Groove(GrooveId::STRAIGHT)));
        assert_eq!(parse("swing 66.7"), Err(LineError::Swing));
        assert_eq!(parse("swing 68"), Err(LineError::Swing));
        assert_eq!(parse("groove mpc-58"), Ok(Line::Groove(groove("MPC 58"))));
        assert_eq!(
            parse("groove Swing 66"),
            Ok(Line::Groove(groove("swing66")))
        );
        assert_eq!(parse("groove"), Ok(Line::PickGroove { track: false }));
        assert_eq!(parse("groove fast"), Err(LineError::Groove));
        let custom = Line::TrackGroove(Some(groove("Custom 2")));
        assert_eq!(parse("tgroove custom-2"), Ok(custom));
        assert_eq!(parse("tgroove off"), Ok(Line::TrackGroove(None)));
        assert_eq!(parse("tgroove"), Ok(Line::PickGroove { track: true }));
        assert!(parse("tswing 60").is_err(), "a groove replaces it");
        assert_eq!(parse("reset 64"), Ok(Line::Reset(Some(64))));
        assert_eq!(parse("reset off"), Ok(Line::Reset(None)));
        assert!(parse("reset 0").is_err());
        assert!(parse("reset").is_err());
        assert_eq!(parse("reset 0x40"), Ok(Line::Reset(Some(64))));
        assert_eq!(parse("len 16"), Ok(Line::Len(16)));
        assert_eq!(parse("len 0x10"), Ok(Line::Len(16)));
        assert_eq!(parse("len 0xfF"), Ok(Line::Len(255)));
        assert_eq!(
            parse("len 10"),
            Ok(Line::Len(10)),
            "plain numbers stay decimal"
        );
        for wrong in [
            "len 0x",
            "len 0x0",
            "len 0",
            "len 0xg",
            "len x10",
            "len 0X10",
            "reset x40",
            "reset 0x0",
        ] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }
        assert_eq!(parse("tvol C0"), Ok(Line::TrackLevel(0xC0)));
        assert_eq!(parse("tvol"), Err(LineError::TrackLevel));
        assert_eq!(parse("tvol 100"), Err(LineError::TrackLevel));
        assert_eq!(parse("track add"), Ok(Line::TrackAdd(1)));
        assert_eq!(parse("track add 4"), Ok(Line::TrackAdd(4)));
        assert_eq!(parse("track del"), Ok(Line::TrackDel));
        assert_eq!(parse("set skip=8"), Ok(Line::SetSkip(8)));
        assert_eq!(parse("set skip"), Err(LineError::Set));
        assert_eq!(
            parse("set scale=dorian"),
            Ok(Line::SetScale(ScaleKind::Dorian))
        );
        assert_eq!(
            parse("set scale=harmonic minor"),
            Ok(Line::SetScale(ScaleKind::HarmonicMinor))
        );
        assert_eq!(
            parse("set scale=Minor-Pentatonic"),
            Ok(Line::SetScale(ScaleKind::MinorPentatonic))
        );
        assert_eq!(parse("set scale=on"), Ok(Line::ScaleOn(true)));
        assert_eq!(parse("set scale=off"), Ok(Line::ScaleOn(false)));
        assert_eq!(parse("set scale=nope"), Err(LineError::Set));
        assert_eq!(parse("set root=F#"), Ok(Line::SetRoot(6)));
        assert_eq!(parse("set root=h"), Err(LineError::Set));
        assert_eq!(parse("scale"), Ok(Line::PickScale));
        assert_eq!(parse("scale fit"), Ok(Line::FitToScale));
        assert_eq!(parse("scale nope"), Err(LineError::Scale));
        assert_eq!(parse("set limiter=off"), Ok(Line::SetLimiter(false)));
        assert_eq!(parse("set limiter=on"), Ok(Line::SetLimiter(true)));
        assert_eq!(parse("set limiter=2"), Err(LineError::Set));
        assert_eq!(parse("set other=1"), Err(LineError::Set));
        assert_eq!(parse("track add 0"), Err(LineError::Track));
        assert_eq!(parse("name bd"), Ok(Line::Name(Some("BD".into()))));
        assert_eq!(parse("name"), Ok(Line::Name(None)));
        assert_eq!(parse("name 1A"), Err(LineError::Name));
        assert_eq!(
            parse("sample a b.wav"),
            Ok(Line::Sample(Some("a b.wav".into())))
        );
        assert_eq!(parse("sample"), Ok(Line::Sample(None)));
        assert_eq!(parse("e"), Ok(Line::Edit(None)));
        assert_eq!(parse("new"), Ok(Line::New(None)));
        assert_eq!(parse("kit"), Ok(Line::Kit(None)));
        assert_eq!(parse("kit 909"), Ok(Line::Kit(Some("909".into()))));
        assert_eq!(parse("kit 909 all"), Ok(Line::KitTracks("909".into())));
        assert_eq!(parse("bank"), Ok(Line::Bank));
        assert_eq!(parse("bank clean"), Ok(Line::BankClean));
        assert_eq!(parse("bank all"), Err(LineError::Bank));
        assert_eq!(parse("normalize -0.3"), Ok(Line::Normalize(Some(-30))));
        assert_eq!(parse("normalize"), Ok(Line::Normalize(Some(0))));
        assert_eq!(parse("normalize off"), Ok(Line::Normalize(None)));
        assert_eq!(parse("normalize 1"), Err(LineError::Normalize));
        assert_eq!(parse("normalize -61"), Err(LineError::Normalize));
        assert_eq!(parse("normalize loud"), Err(LineError::Normalize));
        assert_eq!(parse("trim"), Ok(Line::Trim));
    }

    #[test]
    fn mark_takes_a_letter_and_a_slice_and_delmarks_letters_as_vims_do() {
        assert_eq!(
            parse("mark a"),
            Ok(Line::Mark {
                letter: 'a',
                slice: None
            })
        );
        assert_eq!(
            parse("mark z 0F"),
            Ok(Line::Mark {
                letter: 'z',
                slice: Some(15)
            })
        );
        assert_eq!(parse("mark A 3"), Err(LineError::Mark));
        assert_eq!(parse("mark ab 3"), Err(LineError::Mark));
        assert_eq!(parse("mark a 100"), Err(LineError::Mark));
        assert_eq!(parse("mark"), Err(LineError::Mark));
        assert_eq!(parse("marks"), Ok(Line::Marks));
        let letters = |letters: &str| Ok(Line::DelMarks(Some(letters.chars().collect())));
        assert_eq!(parse("delmarks a"), letters("a"));
        assert_eq!(parse("delmarks a-c"), letters("abc"));
        assert_eq!(parse("delmarks b a"), letters("ab"));
        assert_eq!(parse("delmarks a-cx"), letters("abcx"));
        assert_eq!(parse("delmarks c-a"), Err(LineError::DelMarks));
        assert_eq!(parse("delmarks"), Err(LineError::DelMarks));
        assert_eq!(parse("delmarks 3"), Err(LineError::DelMarks));
        assert_eq!(parse("delmarks!"), Ok(Line::DelMarks(None)));
    }

    #[test]
    fn slice_takes_a_count_clear_or_a_slices_own_setting() {
        let slice = |change| Ok(Line::Slice(change));
        assert_eq!(parse("slice 16"), slice(SliceChange::Even(16)));
        assert_eq!(parse("slice 0x40"), slice(SliceChange::Even(64)));
        assert_eq!(parse("slice 256"), slice(SliceChange::Even(256)));
        assert_eq!(parse("slice 257"), Err(LineError::Slice));
        assert_eq!(parse("slice 0"), Err(LineError::Slice));
        assert_eq!(parse("slice clear"), slice(SliceChange::Clear));
        assert_eq!(
            parse("slice gain -4.5"),
            slice(SliceChange::Gain(Some(-450)))
        );
        assert_eq!(parse("slice gain"), slice(SliceChange::Gain(None)));
        assert_eq!(parse("slice pan -50"), slice(SliceChange::Pan(Some(-50))));
        assert_eq!(parse("slice pan 51"), Err(LineError::Slice));
        assert_eq!(parse("slice tune 7"), slice(SliceChange::Tune(Some(7))));
        assert_eq!(
            parse("slice reverse off"),
            slice(SliceChange::Reverse(Some(false)))
        );
        assert_eq!(parse("slice reverse"), slice(SliceChange::Reverse(None)));
        assert_eq!(
            parse("slice fade 10ms 0.2"),
            slice(SliceChange::Fade(Some((10_000, 200_000))))
        );
        assert_eq!(parse("slice fade"), slice(SliceChange::Fade(None)));
        assert_eq!(parse("slice"), Err(LineError::Slice));
    }

    #[test]
    fn sample_takes_words_for_its_points_and_play_settings() {
        let change = |text: &str| {
            parse(text).map(|line| match line {
                Line::SampleChange(change) => Some(change),
                _ => None,
            })
        };
        assert_eq!(
            change("sample start 1.5"),
            Ok(Some(SampleChange::Start(1_500_000)))
        );
        assert_eq!(
            change("sample start 250ms"),
            Ok(Some(SampleChange::Start(250_000)))
        );
        assert_eq!(change("sample start"), Ok(Some(SampleChange::Start(0))));
        assert_eq!(
            change("sample end 2s"),
            Ok(Some(SampleChange::End(Some(2_000_000))))
        );
        assert_eq!(change("sample end"), Ok(Some(SampleChange::End(None))));
        assert_eq!(
            change("sample loop 0.5 1 20ms"),
            Ok(Some(SampleChange::Loop(Some((500_000, 1_000_000, 20_000)))))
        );
        assert_eq!(
            change("sample loop 0.5 1"),
            Ok(Some(SampleChange::Loop(Some((500_000, 1_000_000, 0)))))
        );
        assert_eq!(
            change("sample loop off"),
            Ok(Some(SampleChange::Loop(None)))
        );
        assert_eq!(
            change("sample reverse"),
            Ok(Some(SampleChange::Reverse(None)))
        );
        assert_eq!(
            change("sample reverse on"),
            Ok(Some(SampleChange::Reverse(Some(true))))
        );
        assert_eq!(
            change("sample fade 10ms 0.2"),
            Ok(Some(SampleChange::Fade(10_000, 200_000)))
        );
        assert_eq!(
            change("sample gain -3.5"),
            Ok(Some(SampleChange::Gain(-350)))
        );
        assert_eq!(
            change("sample tune -2 30"),
            Ok(Some(SampleChange::Tune(-2, 30)))
        );
        assert_eq!(change("sample tune 7"), Ok(Some(SampleChange::Tune(7, 0))));
        for bad in [
            "sample start -1",
            "sample start soon",
            "sample loop 1",
            "sample gain 40",
            "sample tune 1 100",
            "sample reverse maybe",
        ] {
            assert_eq!(parse(bad), Err(LineError::SampleChange), "{bad}");
        }
        assert_eq!(
            parse("sample starting.wav"),
            Ok(Line::Sample(Some("starting.wav".into()))),
            "a file whose name starts with a word"
        );
    }

    #[test]
    fn bad_commands_are_refused() {
        assert_eq!(parse("len 0"), Err(LineError::Len));
        assert_eq!(parse("len"), Err(LineError::Len));
        assert_eq!(parse("lane"), Ok(Line::Lane(None)));
        assert_eq!(parse("lane 3"), Ok(Line::Lane(Some(3))));
        assert_eq!(parse("lane 0"), Err(LineError::Lane));
        assert_eq!(parse("order"), Ok(Line::Order(None)));
        assert_eq!(
            parse("order Up-Down-Inc"),
            Ok(Line::Order(Some(Order::UpDownInc)))
        );
        assert_eq!(parse("order sideways"), Err(LineError::Order));
        assert_eq!(parse("nope"), Err(LineError::Unknown("nope".into())));
    }
}
