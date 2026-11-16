//! Sampling: `Space R`, `R` on the sample page and `:record` record the master, a track or the
//! audio input into a new bank entry, the resample of an Octatrack, an M8 or a Digitakt. The entry
//! is in the bank from its first frame, so a step's `SMP` can play it while it records, as the
//! Deluge's recordings play.
//!
//! - **Starting.** A file name is rolled, a sample that grows as it's written goes in the pool
//!   under it, and the entry goes in the bank's first free place as one undo step, its start after
//!   the margin the recorder keeps from before it started. The recorder then waits for the next
//!   beat or bar, or starts at once, and with the song stopped it starts as the song does.
//! - **While it records,** chunks for it are made here ahead of what's written, so the audio thread
//!   never allocates.
//! - **On stopping,** it's written to the project's `samples/` as a 32-bit float WAV, in mono if
//!   its right side is silent or the same as its left, and the entry gets the file's hash, its end
//!   before the margin past it, and how many steps it lasts if it was on the grid, in every version
//!   undo can reach, so `u` takes the whole recording back as one step. Its level is left as it was
//!   recorded: `:normalize` brings it up as a gain, never by writing the audio again.
//! - **Unkept recordings are dropped:** one taken back with `:record cancel` is never written,
//!   and a file recorded this session that the song as saved doesn't play is removed on quitting.
//!   A song never saved records to the system's temporary folder, and `:w` copies what it plays
//!   into its new `samples/`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use vt_engine::recorder::{RecordQuantise, RecordSource};
use vt_model::growing::Growing;
use vt_model::samples::SampleId;

/// The longest a recording can be, in seconds: ten minutes.
pub(crate) const MOST_SECONDS: u64 = 600;

/// What `Space R` records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    /// The master, as it plays.
    #[default]
    Master,
    /// A track's output, by its place among the tracks.
    Track(usize),
    /// The audio input the audio page chooses.
    Input,
}

impl Source {
    pub(crate) fn engine(self) -> RecordSource {
        match self {
            Self::Master => RecordSource::Master,
            Self::Track(index) => RecordSource::Track(index),
            Self::Input => RecordSource::Input,
        }
    }
}

/// What `:record` asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordWhat {
    Master,
    /// The cursor's track.
    ThisTrack,
    /// A track by its number, from 1.
    Track(usize),
    Input,
}

/// A recording going on, or waiting to start.
#[derive(Debug)]
pub struct Live {
    pub take: Arc<Growing>,
    /// Its bank entry's number, and the path the entry plays.
    pub number: u8,
    pub path: String,
    /// Where its file goes, and the project it's recorded in, if it's saved.
    pub(crate) file: PathBuf,
    pub(crate) folder: Option<PathBuf>,
    /// Its place in the pool.
    pub(crate) id: SampleId,
    pub source: Source,
    pub quantise: RecordQuantise,
    /// How many frames late the input hears what's played, for a recording of it.
    pub(crate) latency: u64,
    /// Whether the recorder has started writing it, and whether it's been asked to stop.
    pub started: bool,
    pub(crate) stopping: bool,
    /// Whether it was started from the sample page, which shows it once it's done.
    pub(crate) from_page: bool,
}

/// The recorder as the app sees it.
#[derive(Debug, Default)]
pub struct Sampling {
    /// What `Space R` records, and when it starts and stops.
    pub source: Source,
    pub quantise: RecordQuantise,
    /// The recording going on.
    pub live: Option<Live>,
    /// The files recorded this session, each with the project it was recorded in, if any.
    pub(crate) made: Vec<(PathBuf, Option<PathBuf>)>,
}

impl Sampling {
    /// The files recorded this session.
    pub fn made(&self) -> impl Iterator<Item = &Path> {
        self.made.iter().map(|(file, _)| file.as_path())
    }
}

/// Why the sample page can't show an entry that's still recording, if it is one, from the
/// recording as [`App::sampling_shown`] gives it, which a page is given in its [`Ctx`].
///
/// [`Ctx`]: crate::pages::Ctx
pub(crate) fn still_recording(recording: Option<(u8, Option<f64>)>, number: u8) -> Option<String> {
    let (recorded, seconds) = recording?;
    if recorded != number {
        return None;
    }
    Some(match seconds {
        Some(seconds) => format!(
            "Recording into {number:02X}: {seconds:.1} s so far. Space R stops it, and then it's shown"
        ),
        None => format!("{number:02X} waits to record: Space R takes it back"),
    })
}
