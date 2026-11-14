//! The recorder (#288): the master, a track's output or the audio input, written into a sample
//! that grows as it's recorded, so a pattern can play it before it has stopped, as the Deluge's
//! recordings are samples from their first frame. It's the resample of an Octatrack's track
//! recorders, an M8's and a Digitakt's sampling.
//!
//! - **It always listens.** A ring of the last quarter second of its source is kept while nothing
//!   records, and a recording starts with it, so it holds the sound from just before it started,
//!   as the Deluge's margin does. It carries on a quarter second past where it stops, too. The
//!   bank entry starts and ends inside these margins, so moving its start or end earlier or later
//!   finds sound there, and a loop's crossfade has some to run in from.
//! - **On the grid.** A start or a stop can wait for the next beat or bar of the song, and lands
//!   on the frame that step falls on, so a loop resampled from a bar to a bar is exactly its bars
//!   long. Armed while the song is stopped, it starts as the song does.
//! - **Latency.** The input hears what was played as the speakers sounded it, the output's and
//!   the input's latency late, so a recording of it starts and stops that much later than the
//!   song's frame, to line up with it. A track or the master is heard as it's made, with none.
//! - **Nothing on the audio thread allocates.** Its frames go into chunks put in place ahead of
//!   it on the edit thread ([`Growing::make_room`]), and a recording is handed back to that thread
//!   when it ends, so it's freed there. A recording that finds no chunk ready ends there.

use std::io;
use std::path::Path;
use std::sync::Arc;

use vt_model::constants::{BAR_STEPS, BEAT_STEPS};
use vt_model::growing::Growing;

/// How long the margins before a recording's start and after its end are, in seconds.
pub const MARGIN_SECONDS: f64 = 0.25;

/// How many frames the margins before a recording's start and after its end are at a rate.
pub fn margin_frames(rate: u32) -> u64 {
    (MARGIN_SECONDS * f64::from(rate)).round().max(1.0) as u64
}

/// The most input frames the recorder takes in a block, as the callback renders at most this many
/// at once.
pub const MAX_INPUT_FRAMES: usize = 4096;

/// How many of the recorder's events can wait to be taken: a recording's start and its end, and a
/// cancelled one's end before them. They're taken after every command that makes one and after
/// every block.
const EVENTS: usize = 4;

/// What the recorder hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordSource {
    /// The master, after its limiter: what plays.
    #[default]
    Master,
    /// A track's output as it goes to the mix or its group, by its place among the tracks: a sound
    /// track, a group, or a return.
    Track(usize),
    /// The audio input.
    Input,
}

/// Where a recording starts and stops: at once, or on the song's next beat or bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordQuantise {
    Now,
    Beat,
    #[default]
    Bar,
}

impl RecordQuantise {
    pub const ALL: [Self; 3] = [Self::Now, Self::Beat, Self::Bar];

    /// How many steps apart its lines are, or none for at once.
    pub fn steps(self) -> Option<u64> {
        match self {
            Self::Now => None,
            Self::Beat => Some(BEAT_STEPS as u64),
            Self::Bar => Some(BAR_STEPS as u64),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::Beat => "beat",
            Self::Bar => "bar",
        }
    }
}

/// How a recording ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    /// It was stopped, and has its margin after the end.
    Stopped,
    /// It ran out of room: it's as long as it can be, or the edit thread didn't keep ahead of it.
    Full,
    /// It was taken back, before or while it recorded, and is to be dropped.
    Cancelled,
}

/// A recording that has ended, handed back to the edit thread with its frames.
#[derive(Debug)]
pub struct Recorded {
    pub take: Arc<Growing>,
    /// How many frames of the margin come before its start.
    pub before: u64,
    /// One past its last frame, counted from the margin's first. The frames after it, to the end
    /// of what's written, are the margin past its end.
    pub end: u64,
    /// How many of the song's steps it lasts, if it started and stopped on the grid while the song
    /// played straight through.
    pub steps: Option<u64>,
    pub ending: Ending,
}

/// What the recorder has to tell the edit thread.
#[derive(Debug)]
pub enum RecorderEvent {
    /// A recording has started: its first frame, after the margin, is being written.
    Started,
    /// A recording has ended.
    Ended(Recorded),
}

/// What the recorder is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Waiting for a line of the song's grid so many steps apart, then so many more frames.
    Armed {
        steps: u64,
        latency: u64,
    },
    /// Starting after so many more frames.
    Waiting {
        left: u64,
    },
    /// Writing, and stopping after so many more frames if it's been asked to.
    Recording {
        stop: Stop,
    },
    /// Writing the margin past its end, so many more frames of it.
    After {
        left: u64,
    },
}

/// When a recording stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    NotYet,
    /// At a line of the song's grid so many steps apart, then so many more frames.
    Armed {
        steps: u64,
        latency: u64,
    },
    /// After so many more frames.
    In(u64),
}

/// The recorder. It's made with everything it needs, and nothing it does allocates.
#[derive(Debug)]
pub struct Recorder {
    source: RecordSource,
    /// The last frames heard, for the margin before a start, and where the next goes.
    ring: Box<[[f32; 2]]>,
    ring_at: usize,
    /// The input's frames for this block, how many there are, and how many have been heard.
    input: Box<[[f32; 2]]>,
    input_len: usize,
    input_read: usize,
    /// The recording it's writing or waiting to, if any.
    take: Option<Arc<Growing>>,
    state: State,
    /// How many frames the margin after the end is.
    after: u64,
    /// Where the recording proper ends, once it's been stopped, in frames from the margin's first.
    end: u64,
    /// The song's step it started on, if it started on the grid while the song played, and how
    /// many steps it lasted once it stopped on the grid.
    started_step: Option<u64>,
    steps: Option<u64>,
    events: [Option<RecorderEvent>; EVENTS],
}

impl Recorder {
    /// A recorder listening to the master, with margins of [`MARGIN_SECONDS`] at a rate.
    pub fn new(rate: u32) -> Self {
        let margin = margin_frames(rate) as usize;
        Self {
            source: RecordSource::Master,
            ring: vec![[0.0; 2]; margin].into(),
            ring_at: 0,
            input: vec![[0.0; 2]; MAX_INPUT_FRAMES].into(),
            input_len: 0,
            input_read: 0,
            take: None,
            state: State::Idle,
            after: margin as u64,
            end: 0,
            started_step: None,
            steps: None,
            events: Default::default(),
        }
    }

    /// What it hears.
    pub fn source(&self) -> RecordSource {
        self.source
    }

    /// Listen to another source. What it heard of the last is forgotten, so a margin is only ever
    /// of the source it's recording. Changing it while recording doesn't change what's recorded.
    pub fn listen_to(&mut self, source: RecordSource) {
        if source != self.source && self.take.is_none() {
            self.source = source;
            self.ring.fill([0.0; 2]);
        }
    }

    /// How many frames the margin before a start is.
    pub fn margin(&self) -> u64 {
        self.ring.len() as u64
    }

    /// Whether it's recording, or waiting to.
    pub fn is_busy(&self) -> bool {
        self.take.is_some()
    }

    /// Record into a sample from a source, at once or from the grid's next line, starting so many
    /// frames later still, for the input's latency. Anything it was recording is cancelled first.
    pub fn record(
        &mut self,
        take: Arc<Growing>,
        source: RecordSource,
        quantise: RecordQuantise,
        latency: u64,
    ) {
        self.cancel();
        self.listen_to(source);
        self.take = Some(take);
        self.started_step = None;
        self.steps = None;
        self.end = 0;
        self.state = match quantise.steps() {
            Some(steps) => State::Armed { steps, latency },
            None => State::Waiting { left: latency },
        };
    }

    /// Stop recording, at once or at the grid's next line while the song plays, so many frames
    /// later still for the input's latency. One not started yet is cancelled.
    pub fn stop(&mut self, quantise: RecordQuantise, latency: u64, playing: bool) {
        match self.state {
            State::Armed { .. } | State::Waiting { .. } => self.cancel(),
            State::Recording { .. } => {
                let stop = match quantise.steps() {
                    Some(steps) if playing => Stop::Armed { steps, latency },
                    _ => Stop::In(latency),
                };
                self.state = State::Recording { stop };
            }
            State::Idle | State::After { .. } => {}
        }
    }

    /// Take a recording back, whatever it's doing, to be dropped.
    pub fn cancel(&mut self) {
        if self.take.is_some() {
            self.finish(Ending::Cancelled);
        }
    }

    /// How many steps apart the lines of the grid it's waiting for are, to start or to stop on,
    /// if it's waiting for one.
    pub fn waits_for(&self) -> Option<u64> {
        match self.state {
            State::Armed { steps, .. }
            | State::Recording {
                stop: Stop::Armed { steps, .. },
            } => Some(steps),
            _ => None,
        }
    }

    /// The line of the grid it's waiting for falls so many frames from now, on this step of the
    /// song's, counted from where it started playing: start or stop there.
    pub fn line(&mut self, frames: u64, step: u64) {
        match self.state {
            State::Armed { latency, .. } => {
                self.started_step = Some(step);
                self.state = State::Waiting {
                    left: frames + latency,
                };
            }
            State::Recording {
                stop: Stop::Armed { latency, .. },
            } => {
                self.steps = self.started_step.map(|started| step - started);
                self.state = State::Recording {
                    stop: Stop::In(frames + latency),
                };
            }
            _ => {}
        }
    }

    /// The song has stopped: a stop waiting for the grid happens now, as no line will come.
    pub fn transport_stopped(&mut self) {
        if let State::Recording {
            stop: Stop::Armed { latency, .. },
        } = self.state
        {
            self.state = State::Recording {
                stop: Stop::In(latency),
            };
        }
    }

    /// Room for the input's frames for the next block, up to [`MAX_INPUT_FRAMES`], to fill
    /// before it's rendered. Frames not filled are silence.
    pub fn input_room(&mut self, frames: usize) -> &mut [[f32; 2]] {
        let frames = frames.min(MAX_INPUT_FRAMES);
        self.input_len = frames;
        self.input_read = 0;
        let room = &mut self.input[..frames];
        room.fill([0.0; 2]);
        room
    }

    /// The block is over: input not filled for the next is silence.
    pub fn end_block(&mut self) {
        self.input_len = 0;
        self.input_read = 0;
    }

    /// The input's next frame, once each frame, or silence past what's come in.
    #[inline]
    pub fn next_input(&mut self) -> [f32; 2] {
        if self.input_read < self.input_len {
            self.input_read += 1;
            self.input[self.input_read - 1]
        } else {
            [0.0; 2]
        }
    }

    /// Hear the source's next frame: keep it for the margin, and write it if recording.
    #[inline]
    pub fn hear(&mut self, frame: [f32; 2]) {
        if self.take.is_some() {
            self.write(frame);
        }
        self.ring[self.ring_at] = frame;
        self.ring_at = (self.ring_at + 1) % self.ring.len();
    }

    /// Hear the same frame so many times, as a resting master gives.
    pub fn hear_steady(&mut self, frame: [f32; 2], frames: u32) {
        for _ in 0..frames {
            self.hear(frame);
        }
    }

    /// Start, stop and write as a frame comes in.
    #[inline]
    fn write(&mut self, frame: [f32; 2]) {
        if let State::Waiting { left } = &mut self.state {
            if *left > 0 {
                *left -= 1;
                return;
            }
            self.begin();
        }
        if let State::Recording {
            stop: Stop::In(left),
        } = &mut self.state
        {
            if *left > 0 {
                *left -= 1;
            } else {
                self.end = self.written();
                self.state = State::After { left: self.after };
            }
        }
        match &mut self.state {
            State::Recording { .. } => {
                if !self.push(frame) {
                    self.end = self.written();
                    self.finish(Ending::Full);
                }
            }
            State::After { left } => {
                if *left == 0 {
                    self.finish(Ending::Stopped);
                    return;
                }
                *left -= 1;
                let done = *left == 0;
                if !self.push(frame) || done {
                    self.finish(Ending::Stopped);
                }
            }
            State::Idle | State::Armed { .. } | State::Waiting { .. } => {}
        }
    }

    /// Start writing, with the margin first: the ring's frames, oldest first.
    fn begin(&mut self) {
        let (newer, older) = self.ring.split_at(self.ring_at);
        let mut room = true;
        if let Some(take) = &self.take {
            for frame in older.iter().chain(newer) {
                room &= take.push(*frame);
            }
        }
        self.state = State::Recording { stop: Stop::NotYet };
        self.event(RecorderEvent::Started);
        if !room {
            self.end = self.written();
            self.finish(Ending::Full);
        }
    }

    fn push(&self, frame: [f32; 2]) -> bool {
        self.take.as_ref().is_some_and(|take| take.push(frame))
    }

    fn written(&self) -> u64 {
        self.take.as_ref().map_or(0, |take| take.written())
    }

    /// Hand the recording back.
    fn finish(&mut self, ending: Ending) {
        let before = match self.state {
            State::Idle | State::Armed { .. } | State::Waiting { .. } => 0,
            State::Recording { .. } | State::After { .. } => self.margin(),
        };
        if let Some(take) = self.take.take() {
            let end = match (ending, self.state) {
                (Ending::Stopped, _) | (Ending::Full, _) => self.end,
                (Ending::Cancelled, State::After { .. }) => self.end,
                (Ending::Cancelled, _) => take.written(),
            };
            self.event(RecorderEvent::Ended(Recorded {
                take,
                before,
                end: end.max(before),
                steps: self.steps,
                ending,
            }));
        }
        self.state = State::Idle;
    }

    fn event(&mut self, event: RecorderEvent) {
        if let Some(slot) = self.events.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(event);
        }
    }

    /// The oldest event not taken yet.
    pub fn take_event(&mut self) -> Option<RecorderEvent> {
        let event = self.events[0].take();
        self.events.rotate_left(1);
        event
    }
}

/// Whether a stereo recording is really mono, as the Deluge tells: a right side much quieter than
/// the left, under a sixty-fourth of its level summed over the whole, is silent, and one whose
/// difference from the left is as small is the same.
pub fn is_mono(frames: &[[f32; 2]]) -> bool {
    let (mut left, mut right, mut apart) = (0.0_f64, 0.0_f64, 0.0_f64);
    for [l, r] in frames {
        left += f64::from(l.abs());
        right += f64::from(r.abs());
        apart += f64::from((l - r).abs());
    }
    let tiny = left / 64.0;
    right <= tiny || apart <= tiny
}

/// Write a recording as a 32-bit float WAV at a rate: one channel, the left, if it's mono, or two.
pub fn write_recording(path: &Path, rate: u32, frames: &[[f32; 2]], mono: bool) -> io::Result<()> {
    let spec = hound::WavSpec {
        channels: if mono { 1 } else { 2 },
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let wav = |error: hound::Error| io::Error::other(error.to_string());
    let mut writer = hound::WavWriter::create(path, spec).map_err(wav)?;
    for [left, right] in frames {
        writer.write_sample(*left).map_err(wav)?;
        if !mono {
            writer.write_sample(*right).map_err(wav)?;
        }
    }
    writer.finalize().map_err(wav)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame that says when it was heard.
    fn frame(at: u64) -> [f32; 2] {
        [at as f32, at as f32 + 0.5]
    }

    fn take(frames: u64) -> Arc<Growing> {
        let growing = Growing::with_chunks(64, frames);
        growing.make_room(frames);
        Arc::new(growing)
    }

    fn ended(recorder: &mut Recorder) -> Recorded {
        loop {
            match recorder.take_event() {
                Some(RecorderEvent::Ended(recorded)) => return recorded,
                Some(RecorderEvent::Started) => {}
                None => panic!("it didn't end"),
            }
        }
    }

    #[test]
    fn the_margin_before_holds_what_was_heard_before_the_start() {
        let mut recorder = Recorder::new(400);
        assert_eq!(recorder.margin(), 100);
        for at in 0..1000 {
            recorder.hear(frame(at));
        }
        recorder.record(take(10_000), RecordSource::Master, RecordQuantise::Now, 0);
        for at in 1000..1500 {
            recorder.hear(frame(at));
        }
        recorder.stop(RecordQuantise::Now, 0, false);
        for at in 1500..2000 {
            recorder.hear(frame(at));
        }
        let recorded = ended(&mut recorder);
        assert_eq!(recorded.ending, Ending::Stopped);
        assert_eq!((recorded.before, recorded.end), (100, 600));
        let frames = recorded.take.to_frames();
        // The margin before, the recording, and the margin after, with nothing missed or doubled.
        let heard: Vec<[f32; 2]> = (900..1600).map(frame).collect();
        assert_eq!(frames, heard);
    }

    #[test]
    fn latency_starts_and_stops_it_that_much_later() {
        let mut recorder = Recorder::new(400);
        recorder.record(take(10_000), RecordSource::Input, RecordQuantise::Now, 30);
        for at in 0..200 {
            recorder.hear(frame(at));
        }
        recorder.stop(RecordQuantise::Now, 30, false);
        for at in 200..400 {
            recorder.hear(frame(at));
        }
        let recorded = ended(&mut recorder);
        let frames = recorded.take.to_frames();
        assert_eq!(frames[recorded.before as usize], frame(30));
        assert_eq!(recorded.end - recorded.before, 200);
    }

    #[test]
    fn armed_it_waits_for_its_line_of_the_grid() {
        let mut recorder = Recorder::new(400);
        recorder.record(take(10_000), RecordSource::Master, RecordQuantise::Bar, 0);
        assert_eq!(recorder.waits_for(), Some(16));
        for at in 0..50 {
            recorder.hear(frame(at));
        }
        assert!(recorder.take_event().is_none(), "it waits");
        recorder.line(10, 16);
        for at in 50..300 {
            recorder.hear(frame(at));
        }
        recorder.stop(RecordQuantise::Bar, 0, true);
        assert_eq!(recorder.waits_for(), Some(16));
        recorder.line(5, 48);
        for at in 300..500 {
            recorder.hear(frame(at));
        }
        let recorded = ended(&mut recorder);
        let frames = recorded.take.to_frames();
        assert_eq!(frames[recorded.before as usize], frame(60));
        assert_eq!(recorded.end - recorded.before, 305 - 60);
        assert_eq!(recorded.steps, Some(32));
    }

    #[test]
    fn running_out_of_room_ends_it_where_it_is() {
        let mut recorder = Recorder::new(400);
        let growing = Arc::new(Growing::with_chunks(64, 1000));
        growing.make_room(128);
        recorder.record(growing, RecordSource::Master, RecordQuantise::Now, 0);
        for at in 0..1000 {
            recorder.hear(frame(at));
        }
        let recorded = ended(&mut recorder);
        assert_eq!(recorded.ending, Ending::Full);
        assert_eq!(recorded.end, 128);
        assert!(!recorder.is_busy());
    }

    #[test]
    fn a_recording_not_started_is_cancelled_by_a_stop() {
        let mut recorder = Recorder::new(400);
        recorder.record(take(1000), RecordSource::Master, RecordQuantise::Bar, 0);
        recorder.stop(RecordQuantise::Bar, 0, true);
        let recorded = ended(&mut recorder);
        assert_eq!(recorded.ending, Ending::Cancelled);
        assert_eq!(recorded.take.written(), 0);
    }

    #[test]
    fn a_silent_or_doubled_right_side_is_mono() {
        let left: Vec<f32> = (0..1000).map(|at| (at as f32 * 0.1).sin()).collect();
        let silent: Vec<[f32; 2]> = left.iter().map(|l| [*l, 0.0]).collect();
        let hiss: Vec<[f32; 2]> = left.iter().map(|l| [*l, l * 0.001]).collect();
        let doubled: Vec<[f32; 2]> = left.iter().map(|l| [*l, *l]).collect();
        let nearly: Vec<[f32; 2]> = left.iter().map(|l| [*l, l * 0.999]).collect();
        let stereo: Vec<[f32; 2]> = left
            .iter()
            .enumerate()
            .map(|(at, l)| [*l, (at as f32 * 0.13).cos()])
            .collect();
        let quieter: Vec<[f32; 2]> = left.iter().map(|l| [*l, l * 0.5]).collect();
        assert!(is_mono(&silent));
        assert!(is_mono(&hiss));
        assert!(is_mono(&doubled));
        assert!(is_mono(&nearly));
        assert!(!is_mono(&stereo));
        assert!(
            !is_mono(&quieter),
            "a side half as loud is panned, not mono"
        );
        assert!(is_mono(&[[0.0; 2]; 10]), "silence is mono");
    }

    #[test]
    fn a_mono_recording_is_written_as_one_channel() {
        let folder = std::env::temp_dir().join(format!("vt-recorder-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("mono.wav");
        let frames: Vec<[f32; 2]> = (0..100).map(|at| [at as f32 / 100.0; 2]).collect();
        write_recording(&path, 48_000, &frames, true).unwrap();
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len(), 100);
        write_recording(&path, 48_000, &frames, false).unwrap();
        let reader = hound::WavReader::open(&path).unwrap();
        assert_eq!(reader.spec().channels, 2);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
