//! Sampling (#288): `Space R`, `R` on the sample page and `:record` record the master, a track or
//! the audio input into a new bank entry, the resample of an Octatrack, an M8 or a Digitakt. The
//! entry is in the bank from its first frame, so a step's `SMP` can play it while it records, as
//! the Deluge's recordings play.
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

use vt_engine::link::Command;
use vt_engine::recorder::{
    Ending, RecordQuantise, RecordSource, Recorded, RecorderEvent, is_mono, margin_frames,
    write_recording,
};
use vt_model::action::Action;
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::growing::Growing;
use vt_model::project;
use vt_model::sample::SampleData;
use vt_model::samples::{SampleId, SamplePool};
use vt_model::song::Song;

use crate::app::App;
use crate::input::mode::View;

/// The longest a recording can be, in seconds: ten minutes.
pub const MOST_SECONDS: u64 = 600;

/// How far ahead of what's written chunks are made, in seconds, so a slow frame here never leaves
/// the recorder without room.
const AHEAD_SECONDS: u64 = 4;

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
    fn engine(self) -> RecordSource {
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
    file: PathBuf,
    folder: Option<PathBuf>,
    /// Its place in the pool.
    id: SampleId,
    pub source: Source,
    pub quantise: RecordQuantise,
    /// How many frames late the input hears what's played, for a recording of it.
    latency: u64,
    /// Whether the recorder has started writing it, and whether it's been asked to stop.
    pub started: bool,
    pub stopping: bool,
    /// Whether it was started from the sample page, which shows it once it's done.
    from_page: bool,
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
    made: Vec<(PathBuf, Option<PathBuf>)>,
}

impl Sampling {
    /// The files recorded this session.
    pub fn made(&self) -> impl Iterator<Item = &Path> {
        self.made.iter().map(|(file, _)| file.as_path())
    }
}

/// A source as the status line names it.
fn source_name(song: &Song, source: Source) -> String {
    match source {
        Source::Master => "the master".to_owned(),
        Source::Input => "the input".to_owned(),
        Source::Track(index) => match song.tracks.get(index) {
            Some(track) if !track.name.is_empty() => {
                format!("track {:02} {}", index + 1, track.name)
            }
            _ => format!("track {:02}", index + 1),
        },
    }
}

/// When a recording waits for, in words.
fn on(quantise: RecordQuantise) -> &'static str {
    match quantise {
        RecordQuantise::Now => "now",
        RecordQuantise::Beat => "on the next beat",
        RecordQuantise::Bar => "on the next bar",
    }
}

impl App {
    /// `Space R` and the sample page's `R`: record what's set up, or stop the recording going on.
    pub(crate) fn toggle_sampling(&mut self) {
        if self.sampling.live.is_some() {
            self.stop_sampling();
        } else {
            let from_page = self.view == View::Sample;
            self.start_sampling(from_page);
        }
    }

    /// `:record`: set up what's recorded and when, from what's given, and start.
    pub(crate) fn record_command(
        &mut self,
        what: Option<RecordWhat>,
        quantise: Option<RecordQuantise>,
    ) {
        if let Some(live) = &self.sampling.live {
            self.status = format!(
                "Already recording into {:02X}: Space R or :record stop stops it",
                live.number
            );
            return;
        }
        let source = match what {
            None => None,
            Some(RecordWhat::Master) => Some(Source::Master),
            Some(RecordWhat::Input) => Some(Source::Input),
            Some(RecordWhat::ThisTrack) => Some(Source::Track(self.cursor.track)),
            Some(RecordWhat::Track(number)) => {
                let sounds = self.song().sound_tracks();
                if number == 0 || number > sounds {
                    self.status = format!(":record takes a track from 1 to {sounds}");
                    return;
                }
                Some(Source::Track(number - 1))
            }
        };
        if let Some(source) = source {
            self.sampling.source = source;
        }
        if let Some(quantise) = quantise {
            self.sampling.quantise = quantise;
        }
        let from_page = self.view == View::Sample;
        self.start_sampling(from_page);
    }

    /// Start recording what's set up into a new bank entry.
    fn start_sampling(&mut self, from_page: bool) {
        let source = self.sampling.source;
        let quantise = self.sampling.quantise;
        if self.read_only {
            self.status =
                "This song is read-only, so nothing is recorded into its folder".to_owned();
            return;
        }
        if source == Source::Input && self.audio.input.is_none() {
            self.status = "No input is open: Space 9 L, then Input, chooses one".to_owned();
            return;
        }
        if let Source::Track(index) = source
            && index >= self.song().tracks.len()
        {
            self.status = format!("There's no track {:02} to record", index + 1);
            return;
        }
        let Some(number) = self.song().bank.free() else {
            self.status =
                "The bank is full, at 255 samples: :bank clean removes those nothing plays"
                    .to_owned();
            return;
        };
        let (folder, files) = match &self.folder {
            Some(folder) => (Some(folder.clone()), folder.join("samples")),
            None => (None, std::env::temp_dir().join("vimtracker-recordings")),
        };
        let name = self
            .names
            .fresh(|name| files.join(format!("{name}.wav")).exists());
        let file = files.join(format!("{name}.wav"));
        let path = match &folder {
            Some(_) => format!("samples/{name}.wav"),
            None => file.display().to_string(),
        };
        let take = Growing::new(u64::from(self.rate) * MOST_SECONDS);
        take.make_room(u64::from(self.rate) * AHEAD_SECONDS);
        let take = Arc::new(take);
        let mut pool = SamplePool::clone(&self.samples);
        let id = pool.insert_growing(&path, Arc::clone(&take));
        self.samples = Arc::new(pool);
        let mut entry = BankEntry::of_file(path.clone(), ContentHash::UNKNOWN);
        entry.name = name;
        entry.edit.points.start = margin_frames(self.rate);
        self.apply(&Action::SetBankEntry {
            number,
            entry: Some(entry),
        });
        let latency = match source {
            Source::Input => self.input_latency(),
            Source::Master | Source::Track(_) => 0,
        };
        let sent = self
            .handle
            .record(Arc::clone(&take), source.engine(), quantise, latency);
        let live = Live {
            take,
            number,
            path,
            file,
            folder,
            id,
            source,
            quantise,
            latency,
            started: false,
            stopping: false,
            from_page,
        };
        if !sent {
            self.drop_recording(&live);
            self.status =
                "Audio has stopped, so nothing records. Space 9 L chooses a device".to_owned();
            return;
        }
        let waits = match (quantise, self.transport) {
            (RecordQuantise::Now, _) => "now",
            (_, vt_engine::engine::Transport::Playing) => on(quantise),
            _ => "as the song starts",
        };
        self.status = format!(
            "Recording {} into {number:02X} {waits}: Space R stops it",
            source_name(self.song(), source)
        );
        self.sampling.live = Some(live);
    }

    /// How many frames late the input hears what plays: out through the output's buffer and
    /// device, and back in through the input's.
    fn input_latency(&self) -> u64 {
        let micros = self.audio.latency_us + self.audio.input_latency_us;
        micros * u64::from(self.rate) / 1_000_000
    }

    /// Stop the recording going on, on the beat or bar if it started on one while playing. One
    /// still waiting to start is taken back.
    pub(crate) fn stop_sampling(&mut self) {
        let Some(live) = &mut self.sampling.live else {
            self.status = "Nothing is recording".to_owned();
            return;
        };
        if !live.started {
            self.cancel_sampling();
            return;
        }
        live.stopping = true;
        let (quantise, latency, number) = (live.quantise, live.latency, live.number);
        self.handle
            .send(Command::StopRecording { quantise, latency });
        let playing = self.transport == vt_engine::engine::Transport::Playing;
        let when = if playing { on(quantise) } else { "now" };
        self.status = format!("Stopping {number:02X} {when}");
    }

    /// `:record cancel`: take the recording back. It's never written, and its entry goes.
    pub(crate) fn cancel_sampling(&mut self) {
        let Some(live) = self.sampling.live.take() else {
            self.status = "Nothing is recording".to_owned();
            return;
        };
        self.handle.send(Command::CancelRecording);
        self.drop_recording(&live);
        self.status = format!("Recording into {:02X} taken back", live.number);
    }

    /// Take a recording's entry out of every version, and its sample out of the pool's paths.
    fn drop_recording(&mut self, live: &Live) {
        let (number, path) = (live.number, live.path.clone());
        self.history
            .amend(|song| plays(song, number, &path).then(|| without(song, number)));
        let mut pool = SamplePool::clone(&self.samples);
        pool.forget(&live.path);
        self.samples = Arc::new(pool);
        self.dirty = true;
        self.after_history();
    }

    /// The audio has changed, so the engine recording is gone: its recording goes too.
    pub(crate) fn lose_sampling(&mut self) {
        if let Some(live) = self.sampling.live.take() {
            self.drop_recording(&live);
            self.status = format!(
                "Recording into {:02X} dropped, as the audio changed",
                live.number
            );
        }
    }

    /// Each frame: keep chunks made ahead of the recording, and take in what the recorder says.
    pub(crate) fn follow_sampling(&mut self) {
        if let Some(live) = &self.sampling.live {
            live.take.make_room(u64::from(self.rate) * AHEAD_SECONDS);
        }
        while let Some(event) = self.handle.recorder_event() {
            match event {
                RecorderEvent::Started => {
                    if let Some(live) = &mut self.sampling.live {
                        live.started = true;
                    }
                }
                RecorderEvent::Ended(recorded) => self.recording_ended(recorded),
            }
        }
    }

    /// A recording has ended: keep it, unless it was taken back.
    fn recording_ended(&mut self, recorded: Recorded) {
        let ours = self
            .sampling
            .live
            .as_ref()
            .is_some_and(|live| Arc::ptr_eq(&live.take, &recorded.take));
        if !ours || recorded.ending == Ending::Cancelled {
            // Taken back here already, or one before it that was.
            return;
        }
        let Some(live) = self.sampling.live.take() else {
            return;
        };
        match self.keep_recording(&live, &recorded) {
            Ok(said) => self.status = said,
            Err(error) => {
                self.drop_recording(&live);
                self.status = format!("The recording couldn't be written: {error}");
            }
        }
    }

    /// Write a recording to its file and make its entry whole.
    fn keep_recording(&mut self, live: &Live, recorded: &Recorded) -> std::io::Result<String> {
        let mut frames = recorded.take.to_frames();
        let (before, end) = (recorded.before as usize, recorded.end as usize);
        let end = end.min(frames.len());
        let mono = is_mono(&frames[before.min(end)..end]);
        if let Some(parent) = live.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        write_recording(&live.file, self.rate, &frames, mono)?;
        let hash = ContentHash::of_file(&live.file)?;
        self.sampling
            .made
            .push((live.file.clone(), live.folder.clone()));
        // As the file reads back: one side on both.
        if mono {
            for frame in &mut frames {
                frame[1] = frame[0];
            }
        }
        let total = frames.len() as u64;
        let mut pool = SamplePool::clone(&self.samples);
        pool.finish_growing(live.id, SampleData { frames });
        self.samples = Arc::new(pool);
        let (number, path) = (live.number, live.path.clone());
        let end = recorded.end;
        let steps = recorded.steps;
        self.history.amend(|song| {
            if !plays(song, number, &path) {
                return None;
            }
            let mut song = song.clone();
            let mut entry = song.bank.get(number)?.clone();
            for source in entry.sources_mut() {
                source.hash = hash;
            }
            if entry.edit.points.end.is_none() && end < total {
                entry.edit.points.end = Some(end);
            }
            if let Some(steps) = steps
                && entry.edit.hints.length_steps().is_none()
            {
                entry.edit.hints.set_length_steps(Some(steps as u32));
            }
            song.bank.set(number, Some(entry));
            Some(song)
        });
        self.dirty = true;
        self.after_history();
        if live.from_page || self.view == View::Sample {
            self.sample_page.pinned = Some((self.cursor.track, number));
        }
        let seconds = (end - recorded.before) as f64 / f64::from(self.rate);
        let channels = if mono { "mono" } else { "stereo" };
        let full = match recorded.ending {
            Ending::Full => ", where it ran out of room",
            Ending::Stopped | Ending::Cancelled => "",
        };
        let title = self
            .song()
            .bank
            .get(number)
            .map_or_else(String::new, BankEntry::title);
        Ok(format!(
            "Recorded {number:02X} {title}, {seconds:.2} s in {channels}{full}. SMP {number:02X} plays it"
        ))
    }

    /// On quitting: remove each file recorded this session that the song as saved in its project
    /// doesn't play, or its autosave, and take back any recording going on.
    pub(crate) fn drop_unkept_recordings(&mut self) {
        if self.sampling.live.is_some() {
            self.cancel_sampling();
        }
        for (file, folder) in std::mem::take(&mut self.sampling.made) {
            let kept = folder.as_deref().is_some_and(|folder| {
                let saved = [project::load_song(folder), project::load_autosave(folder)];
                saved
                    .iter()
                    .flatten()
                    .any(|song| plays_file(song, folder, &file))
            });
            if !kept {
                let _ = std::fs::remove_file(&file);
            }
        }
    }

    /// The recording's seconds so far, and its entry's number, for the status line: none while
    /// nothing records, and no seconds while it waits to start.
    pub fn sampling_shown(&self) -> Option<(u8, Option<f64>)> {
        let live = self.sampling.live.as_ref()?;
        let seconds = live.started.then(|| {
            let written = live.take.written().saturating_sub(margin_frames(self.rate));
            written as f64 / f64::from(self.rate)
        });
        Some((live.number, seconds))
    }

    /// Why the sample page can't show an entry that's still recording, if it is one.
    pub fn still_recording(&self, number: u8) -> Option<String> {
        let live = self.sampling.live.as_ref()?;
        if live.number != number {
            return None;
        }
        Some(match self.sampling_shown() {
            Some((_, Some(seconds))) => format!(
                "Recording into {number:02X}: {seconds:.1} s so far. Space R stops it, and then it's shown"
            ),
            _ => format!("{number:02X} waits to record: Space R takes it back"),
        })
    }
}

/// Whether a song's entry at a number plays a path.
fn plays(song: &Song, number: u8, path: &str) -> bool {
    song.bank
        .get(number)
        .and_then(BankEntry::source)
        .is_some_and(|source| source.path == path)
}

/// A song without its entry at a number.
fn without(song: &Song, number: u8) -> Song {
    let mut song = song.clone();
    song.bank.set(number, None);
    song
}

/// Whether a song saved in a folder plays a file.
fn plays_file(song: &Song, folder: &Path, file: &Path) -> bool {
    song.bank.iter().any(|(_, entry)| {
        entry
            .sources()
            .any(|source| project::resolve_sample(Some(folder), &source.path) == file)
    })
}
