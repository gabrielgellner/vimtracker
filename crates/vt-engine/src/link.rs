//! The queues between the edit thread and the audio callback.
//!
//! `rtrb` can't take back a queued item, so the newest unsent version waits in a slot on the edit
//! thread. Every edit and every frame tries to send it, so the last edit always reaches audio.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};
use vt_model::constants::MAX_TRACKS;
use vt_model::growing::Growing;
use vt_model::insert::EffectKind;

use vt_model::sample::SampleData;
use vt_model::scale::Scale;

use crate::dsp::effects::memory::{Memory, MemoryCount};
use crate::engine::{Engine, PLAYHEADS, Playhead, SectionLoop, TrackPlace, Transport};
use crate::gens::GenReport;
use crate::mixer::Reading;
use crate::published::{Handover, Published, Span};
pub use crate::queues::INPUT_QUEUE;
use crate::queues::{
    COMMAND_QUEUE, FILE_QUEUE, GEN_HEARD_QUEUE, INPUT_MOST_WAITING, MEMORY_QUEUE, NOTE_PITCH_QUEUE,
    PUBLISH_QUEUE, RECORDER_QUEUE, RETURN_QUEUE, SCOPE_QUEUE, TAKE_QUEUE, TELEMETRY_QUEUE,
};
use crate::recorder::{RecordQuantise, RecordSource, RecorderEvent};
#[cfg(test)]
use crate::track_state::Drawn;
use crate::track_state::{Faders, LiveValues, NotePitch};

/// A transport or preview message. The engine resolves it against its own state, so two quick
/// presses can't act on an out-of-date view of whether it is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Which track's live values to report.
    Watch { track: usize },
    /// `Space c` and `Shift-Tab`: play from a section step, whatever is happening now.
    PlayFrom { step: usize },
    /// `Space c`: play from a step of a section, or from a step of a track's own with its chain at
    /// that step if it's given.
    PlaySection {
        section: usize,
        step: usize,
        track: Option<usize>,
    },
    /// `:loop`: keep playing sections in turn, from the next line of the launch grid while
    /// playing, or go back to playing the song.
    Loop(Option<SectionLoop>),
    /// `Tab`: pause in place, or play from there.
    PauseOrResume,
    /// Opening a project: stop, and go back to the start.
    Return,
    /// Play an entered note on a preview voice for a short while.
    Preview {
        track: usize,
        note: u8,
        velocity: u8,
    },
    /// Play a note on a preview voice for as long as its key is held. Each held key has a voice
    /// of its own, so held keys play a chord.
    HoldPreview {
        track: usize,
        note: u8,
        velocity: u8,
    },
    /// A held key was let go: its note ends, and any others held carry on.
    ReleasePreview { track: usize, note: u8 },
    /// Hear a row of the section while not playing: a step of the song's, or of a track's own,
    /// with every other track's row playing then.
    Audition {
        section: usize,
        section_step: usize,
        track: Option<usize>,
    },
    /// The audition's key was let go, after a hold or a tap.
    EndAudition { held: bool },
    /// An audition's key on a preview voice was tapped: let its note go once it has sounded
    /// as long as a preview does.
    FinishPreview { track: usize, note: u8 },
    /// A hand moved one of a track's faders, by its place in `FADERS`, that the steps drive: the
    /// track page's value is heard from now, and the steps' aren't, until it's given back.
    TakeOver { track: usize, fader: usize },
    /// `Space e` and `Space E`: give a track's faders, or every track's, back to the steps.
    GiveBack { track: Option<usize> },
    /// Mute or unmute a track.
    SetMute { track: usize, mute: bool },
    /// Solo or unsolo a track.
    SetSolo { track: usize, solo: bool },
    /// Turn fill on or off, for `CND`'s `FILL` and `!FILL`.
    SetFill { on: bool },
    /// `Space A`: send every track back to its start at the next line of the launch grid, or
    /// take back a realign waiting for it.
    Realign,
    /// `Space a`: start one track's chain again at the next line of the launch grid, or take back
    /// one waiting for it.
    RealignTrack { track: usize },
    /// `Space g m`: switch a track's mute at the next line of the launch grid, or take back a
    /// switch waiting for it.
    MuteOnGrid { track: usize },
    /// `Space g s`: switch a track's solo at the next line of the launch grid, or take back a
    /// switch waiting for it.
    SoloOnGrid { track: usize },
    /// Set the scale an `ACU` moves notes through.
    SetScale { scale: Scale },
    /// Play a chain on one track when its launch timing allows, or stop it then.
    Launch { track: usize, chain: Option<u32> },
    /// Carry on in a captured section, with launched chains still running.
    ContinueIn { section: usize, length: usize },
    /// Send this many long buffers back to be freed, as they come free.
    ReleaseMemory(MemoryCount),
    /// Play the next one-off file sent with [`Handle::play_file`] through a track.
    PlayFile { track: usize },
    /// Let go of any one-off file playing.
    StopFile,
    /// Listen to a source, so a recording's margin before its start is of it.
    ListenTo { source: RecordSource },
    /// Record into the next sample sent with [`Handle::record`].
    Record {
        source: RecordSource,
        quantise: RecordQuantise,
        latency: u64,
    },
    /// Stop recording, at once or on the grid, so many frames later still.
    StopRecording {
        quantise: RecordQuantise,
        latency: u64,
    },
    /// Take the recording back, to be dropped.
    CancelRecording,
}

/// What one track is doing while jamming. The outer `None`s mean it's following its section and
/// nothing is waiting, and the inner ones that it's stopped, or stopping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JamState {
    pub playing: Option<Option<u32>>,
    pub cued: Option<Option<u32>>,
    /// The chain the track is on, launched or not.
    pub chain: Option<u32>,
    /// The step of the track's own running count that chain began at.
    pub started: i64,
    /// Its mute and solo, and whether each switches at the next line of the launch grid.
    pub mute: bool,
    pub solo: bool,
    pub mute_waiting: bool,
    pub solo_waiting: bool,
    /// How many of those switches have landed, which changes when one does.
    pub switches_landed: u8,
}

/// What the callback reports back each block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    pub section: usize,
    pub position: usize,
    /// How far through that place the transport is, from 0 to 1.
    pub progress: f32,
    /// Where each track is heard, as its own steps count.
    pub places: [TrackPlace; MAX_TRACKS],
    pub transport: Transport,
    /// Whether a realign is waiting for its line of the grid.
    pub realigning: bool,
    /// Whether each track's own realign is waiting for its line of the grid.
    pub realigning_tracks: [bool; MAX_TRACKS],
    /// Whether a loop is waiting for its line of the grid.
    pub loop_waiting: bool,
    pub reading: Reading,
    /// Each track's levels.
    pub tracks: [Reading; MAX_TRACKS],
    /// Each track's faders as they're heard: where the steps have them, and whether a hand has
    /// taken them over.
    pub faders: [Faders; MAX_TRACKS],
    /// The version playing, by its [`Published::id`](crate::published::Published::id), which the
    /// live values were worked out from.
    pub version: u64,
    /// The track being watched, and what it's playing with.
    pub watched: usize,
    pub live: LiveValues,
    /// Where each voice playing a sample through the track watched is in it.
    pub playheads: [Option<Playhead>; PLAYHEADS],
}

/// The edit thread's end.
pub struct Handle {
    versions: Producer<Handover>,
    returned: Consumer<Handover>,
    commands: Producer<Command>,
    telemetry: Consumer<Telemetry>,
    /// The main output, mixed to one channel, for the scope.
    scope: Consumer<f32>,
    jam: Consumer<(usize, JamState)>,
    /// What the mod slots moved each note's pitch by, by track, as each note is heard.
    note_pitches: Consumer<(usize, NotePitch)>,
    /// What was heard of each track's live generators: passes, rows and rests.
    gen_heard: Consumer<(usize, GenReport)>,
    pending: Option<Handover>,
    memories: Producer<Memory>,
    freed: Consumer<Memory>,
    /// One-off files to hear, each with how much of it plays, and those the callback is done
    /// with, to be freed here.
    files: Producer<(Arc<SampleData>, Option<Span>)>,
    heard: Consumer<Arc<SampleData>>,
    /// Recordings to write into, and the recorder's starts and ends coming back.
    takes: Producer<Arc<Growing>>,
    recorded: Consumer<RecorderEvent>,
    /// Long buffers that can't go until there's room in the queue. A version waits for them.
    unsent: Vec<Memory>,
    /// How many long buffers the callback has or is getting, beyond any it's been told to release.
    held: MemoryCount,
    rate: u32,
}

/// The callback's end. Nothing here allocates, frees or locks.
pub struct Realtime {
    engine: Engine,
    versions: Consumer<Handover>,
    returned: Producer<Handover>,
    commands: Consumer<Command>,
    telemetry: Producer<Telemetry>,
    scope: Producer<f32>,
    jam: Producer<(usize, JamState)>,
    note_pitches: Producer<(usize, NotePitch)>,
    gen_heard: Producer<(usize, GenReport)>,
    memories: Consumer<Memory>,
    freed: Producer<Memory>,
    files: Consumer<(Arc<SampleData>, Option<Span>)>,
    heard: Producer<Arc<SampleData>>,
    takes: Consumer<Arc<Growing>>,
    recorded: Producer<RecorderEvent>,
    /// The audio input's samples, interleaved stereo, if an input is open.
    input: Option<Consumer<f32>>,
    /// What the edit thread has been told about each track.
    reported: [JamState; MAX_TRACKS],
    /// The track whose live values are reported.
    watched: usize,
    /// How long the engine took over the last block, apart from the queues and the report, for
    /// `vimtracker bench --device`.
    engine_time: Duration,
}

/// Connect an engine to the edit thread.
pub fn connect(engine: Engine) -> (Handle, Realtime) {
    let (version_tx, version_rx) = RingBuffer::new(PUBLISH_QUEUE);
    let (return_tx, return_rx) = RingBuffer::new(RETURN_QUEUE);
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_QUEUE);
    let (telemetry_tx, telemetry_rx) = RingBuffer::new(TELEMETRY_QUEUE);
    let (scope_tx, scope_rx) = RingBuffer::new(SCOPE_QUEUE);
    let (jam_tx, jam_rx) = RingBuffer::new(MAX_TRACKS * 2);
    let (pitch_tx, pitch_rx) = RingBuffer::new(NOTE_PITCH_QUEUE);
    let (pass_tx, pass_rx) = RingBuffer::new(GEN_HEARD_QUEUE);
    let (memory_tx, memory_rx) = RingBuffer::new(MEMORY_QUEUE);
    let (freed_tx, freed_rx) = RingBuffer::new(MEMORY_QUEUE);
    let (file_tx, file_rx) = RingBuffer::new(FILE_QUEUE);
    // Room for every file that can be queued, and one playing, so one always fits.
    let (heard_tx, heard_rx) = RingBuffer::new(FILE_QUEUE + 1);
    let (take_tx, take_rx) = RingBuffer::new(TAKE_QUEUE);
    let (recorded_tx, recorded_rx) = RingBuffer::new(RECORDER_QUEUE);
    let held = MemoryCount::of(&engine.current().song);
    let rate = engine.rate();
    (
        Handle {
            versions: version_tx,
            returned: return_rx,
            commands: command_tx,
            telemetry: telemetry_rx,
            scope: scope_rx,
            jam: jam_rx,
            note_pitches: pitch_rx,
            gen_heard: pass_rx,
            pending: None,
            memories: memory_tx,
            freed: freed_rx,
            files: file_tx,
            heard: heard_rx,
            takes: take_tx,
            recorded: recorded_rx,
            unsent: Vec::new(),
            held,
            rate,
        },
        Realtime {
            engine,
            versions: version_rx,
            returned: return_tx,
            commands: command_rx,
            telemetry: telemetry_tx,
            scope: scope_tx,
            jam: jam_tx,
            note_pitches: pitch_tx,
            gen_heard: pass_tx,
            memories: memory_rx,
            freed: freed_tx,
            files: file_rx,
            heard: heard_tx,
            takes: take_rx,
            recorded: recorded_tx,
            input: None,
            reported: [JamState::default(); MAX_TRACKS],
            watched: 0,
            engine_time: Duration::ZERO,
        },
    )
}

impl Handle {
    /// Send a new version. If an unsent one is waiting, this replaces it, and it is freed here.
    pub fn publish(&mut self, version: Arc<Published>) {
        self.meet_memory_needs(MemoryCount::of(&version.song));
        self.pending = Some(Handover::new(version));
        self.pump();
    }

    /// Make the long buffers a version needs, or ask for back those it doesn't. They go ahead of
    /// it, so what it needs is there when it arrives.
    fn meet_memory_needs(&mut self, needed: MemoryCount) {
        for kind in EffectKind::ALL {
            for _ in self.held[kind]..needed[kind] {
                self.unsent.extend(Memory::new(kind, self.rate));
            }
        }
        let spare = MemoryCount(std::array::from_fn(|at| {
            self.held.0[at].saturating_sub(needed.0[at])
        }));
        if spare != MemoryCount::default() {
            self.send(Command::ReleaseMemory(spare));
        }
        self.held = needed;
    }

    /// Free returned versions and long buffers, and try to send what's waiting. Call this every
    /// frame.
    pub fn pump(&mut self) {
        while self.returned.pop().is_ok() {}
        while self.freed.pop().is_ok() {}
        while self.heard.pop().is_ok() {}
        while let Some(memory) = self.unsent.pop() {
            if let Err(rtrb::PushError::Full(memory)) = self.memories.push(memory) {
                self.unsent.push(memory);
                return;
            }
        }
        if let Some(version) = self.pending.take()
            && let Err(rtrb::PushError::Full(version)) = self.versions.push(version)
        {
            self.pending = Some(version);
        }
    }

    /// Send a command. If the queue is full, which means audio has stopped, it is dropped.
    pub fn send(&mut self, command: Command) -> bool {
        self.commands.push(command).is_ok()
    }

    /// Play a one-off file through a track, loaded on this thread, as the sample browser plays
    /// the file its cursor is on: the whole of it, or only a span, the way the span goes, as the
    /// sample page plays a slice. The callback only plays it, and gives it back to be freed here.
    /// If the queue is full, which means audio has stopped, it's dropped here.
    pub fn play_file(&mut self, track: usize, file: Arc<SampleData>, span: Option<Span>) -> bool {
        self.pump();
        self.files.push((file, span)).is_ok() && self.send(Command::PlayFile { track })
    }

    /// Record a source into a sample that grows as it's written, at once or on the grid, so many
    /// frames later still for the input's latency. The sample's chunks must be kept ahead of what's
    /// written here, with [`Growing::make_room`]. If the queue is full, which means audio has
    /// stopped, it's dropped here.
    pub fn record(
        &mut self,
        take: Arc<Growing>,
        source: RecordSource,
        quantise: RecordQuantise,
        latency: u64,
    ) -> bool {
        self.pump();
        self.takes.push(take).is_ok()
            && self.send(Command::Record {
                source,
                quantise,
                latency,
            })
    }

    /// The recorder's next start or end, an end with what it recorded.
    pub fn recorder_event(&mut self) -> Option<RecorderEvent> {
        self.recorded.pop().ok()
    }

    /// The newest report from the callback, if there is one. Its reading covers every report
    /// since the last call, so a meter doesn't miss a hit that fell between two frames.
    pub fn latest(&mut self) -> Option<Telemetry> {
        let mut latest: Option<Telemetry> = None;
        while let Ok(mut report) = self.telemetry.pop() {
            if let Some(earlier) = latest {
                report.reading.merge(earlier.reading);
                for (reading, earlier) in report.tracks.iter_mut().zip(earlier.tracks) {
                    reading.merge(earlier);
                }
                // The level after the trim keeps its loudest too, while it's the same track's.
                if report.watched == earlier.watched {
                    for (highest, earlier) in report
                        .live
                        .after_trim
                        .iter_mut()
                        .zip(earlier.live.after_trim)
                    {
                        *highest = highest.max(earlier);
                    }
                }
            }
            latest = Some(report);
        }
        latest
    }

    /// The main output's samples since the last call, mixed to one channel and oldest first.
    pub fn read_scope(&mut self, into: &mut impl Extend<f32>) {
        if let Ok(samples) = self.scope.read_chunk(self.scope.slots()) {
            into.extend(samples);
        }
    }

    /// The next change in what a track is doing while jamming.
    pub fn jam_change(&mut self) -> Option<(usize, JamState)> {
        self.jam.pop().ok()
    }

    /// The next note heard on any track whose pitch a mod slot could move, and what each slot moved
    /// it by.
    pub fn note_pitch(&mut self) -> Option<(usize, NotePitch)> {
        self.note_pitches.pop().ok()
    }

    /// The next thing heard of a live generator, on any track: a pass of one that varies, a row
    /// of a moving one, or one at rest again.
    pub fn gen_heard(&mut self) -> Option<(usize, GenReport)> {
        self.gen_heard.pop().ok()
    }

    /// Whether a version is still waiting to be sent.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Realtime {
    /// Hear an audio input, its samples interleaved stereo, which the recorder can record. It's
    /// set before the callback starts.
    pub fn listen(&mut self, input: Consumer<f32>) {
        self.input = Some(input);
    }

    /// Pass the recorder's events on, as far as there's room. What doesn't fit waits in the engine.
    fn pass_recorder_events(&mut self) {
        while !self.recorded.is_full()
            && let Some(event) = self.engine.take_recorder_event()
        {
            let _ = self.recorded.push(event);
        }
    }

    /// Give the recorder the input's frames for a block: as many as have come, up to the block's,
    /// and silence for any that haven't. Too many waiting, as two devices' clocks drift apart,
    /// and the oldest are dropped, so the input never falls further and further behind.
    fn take_input(&mut self, frames: usize) {
        let Some(input) = &mut self.input else {
            return;
        };
        let waiting = input.slots() / 2;
        let late = waiting.saturating_sub(frames.max(INPUT_MOST_WAITING));
        if late > 0
            && let Ok(chunk) = input.read_chunk(late * 2)
        {
            chunk.commit_all();
        }
        let room = self.engine.input_room(frames);
        let count = room.len().min(input.slots() / 2);
        if let Ok(chunk) = input.read_chunk(count * 2) {
            let (first, second) = chunk.as_slices();
            for (at, sample) in first.iter().chain(second).enumerate() {
                room[at / 2][at % 2] = *sample;
            }
            chunk.commit_all();
        }
    }

    /// Fill one block of interleaved stereo output.
    pub fn process(&mut self, out: &mut [f32]) {
        // What a version needs comes ahead of it.
        while let Ok(memory) = self.memories.pop() {
            self.engine.receive_memory(memory);
        }
        // Take a new version only when the old one can be sent back to be freed.
        if !self.returned.is_full()
            && let Ok(version) = self.versions.pop()
        {
            let old = self.engine.hand_over(version);
            let _ = self.returned.push(old);
        }
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Watch { track } => self.watched = track,
                Command::ReleaseMemory(count) => self.engine.release_memory(count),
                Command::PlayFrom { step } => self.engine.play_from(step),
                Command::PauseOrResume => self.engine.pause_or_resume(),
                Command::Return => self.engine.return_to_start(),
                Command::Preview {
                    track,
                    note,
                    velocity,
                } => self.engine.preview(track, note, velocity),
                Command::HoldPreview {
                    track,
                    note,
                    velocity,
                } => self.engine.hold_preview(track, note, velocity),
                Command::ReleasePreview { track, note } => {
                    self.engine.release_preview(track, note);
                }
                Command::PlaySection {
                    section,
                    step,
                    track,
                } => self.engine.play_section_at(section, step, track),
                Command::Loop(sections) => self.engine.loop_sections(sections),
                Command::Audition {
                    section,
                    section_step,
                    track,
                } => self.engine.audition_from(section, section_step, track),
                Command::EndAudition { held } => self.engine.end_audition(held),
                Command::FinishPreview { track, note } => {
                    self.engine.finish_preview(track, note);
                }
                Command::TakeOver { track, fader } => self.engine.take_over(track, fader),
                Command::GiveBack { track } => self.engine.give_back(track),
                Command::SetMute { track, mute } => self.engine.set_mute(track, mute),
                Command::SetFill { on } => self.engine.set_fill(on),
                Command::Realign => self.engine.toggle_realign(),
                Command::RealignTrack { track } => self.engine.toggle_realign_track(track),
                Command::MuteOnGrid { track } => self.engine.toggle_mute_on_grid(track),
                Command::SoloOnGrid { track } => self.engine.toggle_solo_on_grid(track),
                Command::SetScale { scale } => self.engine.set_scale(scale),
                Command::SetSolo { track, solo } => self.engine.set_solo(track, solo),
                Command::Launch { track, chain } => self.engine.launch(track, chain),
                Command::ContinueIn { section, length } => {
                    self.engine.continue_in(section, length);
                }
                // Taken only when what it replaces can be sent back, so nothing is freed here.
                Command::PlayFile { track } => {
                    if !self.heard.is_full()
                        && let Ok((file, span)) = self.files.pop()
                        && let Some(done) = self.engine.play_file(track, file, span)
                    {
                        let _ = self.heard.push(done);
                    }
                }
                Command::StopFile => self.engine.stop_file(),
                Command::ListenTo { source } => self.engine.listen_to(source),
                // Taken only when what it ends can be sent back, so nothing is freed here.
                Command::Record {
                    source,
                    quantise,
                    latency,
                } => {
                    if self.recorded.slots() >= 2
                        && let Ok(take) = self.takes.pop()
                    {
                        self.engine.record(take, source, quantise, latency);
                    }
                    self.pass_recorder_events();
                }
                Command::StopRecording { quantise, latency } => {
                    self.engine.stop_recording(quantise, latency);
                    self.pass_recorder_events();
                }
                Command::CancelRecording => {
                    self.engine.cancel_recording();
                    self.pass_recorder_events();
                }
            }
        }
        while !self.freed.is_full()
            && let Some(memory) = self.engine.next_freed_memory()
        {
            let _ = self.freed.push(memory);
        }
        self.take_input(out.len() / 2);
        let started = Instant::now();
        self.engine.process(out);
        self.engine_time = started.elapsed();
        self.pass_recorder_events();
        // The scope's tap. The callback can't wait for room, so what doesn't fit is dropped.
        let frames = out.len() / 2;
        if let Ok(room) = self
            .scope
            .write_chunk_uninit(self.scope.slots().min(frames))
        {
            let mono = out.as_chunks::<2>().0.iter();
            room.fill_from_iter(mono.map(|[left, right]| (left + right) * 0.5));
        }
        let reading = self.engine.take_reading();
        let mut live = self.engine.live(self.watched);
        live.after_trim = self.engine.take_after_trim(self.watched);
        let _ = self.telemetry.push(Telemetry {
            section: self.engine.section(),
            position: self.engine.position(),
            progress: self.engine.step_progress(),
            places: std::array::from_fn(|track| self.engine.track_heard(track)),
            transport: self.engine.transport(),
            realigning: self.engine.realign_waiting(),
            realigning_tracks: std::array::from_fn(|track| {
                self.engine.track_realign_waiting(track)
            }),
            loop_waiting: self.engine.loop_waiting(),
            reading,
            tracks: self.engine.take_track_readings(reading.frames),
            faders: std::array::from_fn(|track| self.engine.faders(track)),
            version: self.engine.version(),
            watched: self.watched,
            live,
            playheads: self.engine.playheads(self.watched),
        });
        // What doesn't fit is dropped, as the app reads them every frame. A track reports the last
        // of its notes in a block, which is only ever one at any speed a row can play.
        for track in 0..MAX_TRACKS {
            if let Some(pitch) = self.engine.take_note_pitch(track) {
                let _ = self.note_pitches.push((track, pitch));
            }
        }
        let heard = &mut self.gen_heard;
        self.engine.take_gen_reports(|track, report| {
            let _ = heard.push((track, report));
        });
        // A change that doesn't fit stays different, so it goes in a later block.
        for (track, reported) in self.reported.iter_mut().enumerate() {
            let state = self.engine.jam(track);
            if state != *reported && self.jam.push((track, state)).is_ok() {
                *reported = state;
            }
        }
    }

    /// The engine, for tests.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// How long the engine itself took over the last block.
    pub fn engine_time(&self) -> Duration {
        self.engine_time
    }

    /// The engine back, once the callback is done with it, to read where its time went.
    pub fn into_engine(self) -> Engine {
        self.engine
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vt_model::constants::MAX_SOUND_TRACKS;
    use vt_model::insert::{Distort, DistortType, INSERTS, Insert};
    use vt_model::sample::SampleData;
    use vt_model::samples::SamplePool;
    use vt_model::settings::SettingId;
    use vt_model::song::Song;
    use vt_model::step::{Note, Step};

    use super::*;
    use crate::dsp::effects::memory::BUFFERED;
    use crate::recorder::RecordQuantise;
    use crate::rt_guard::forbidden;

    /// A limiter, which takes a short line from the pool.
    const LIMIT: Insert = Insert::Limit(vt_model::insert::Limiter::DEFAULT);
    /// A flanger, which takes a short line from the pool, and a phaser, which needs none.
    const FLANGER: Insert = Insert::Flanger(vt_model::insert::Flanger::DEFAULT);
    const PHASER: Insert = Insert::Phaser(vt_model::insert::Phaser::DEFAULT);
    /// A tape, which takes a short line from the pool for its wow and flutter.
    const TAPE: Insert = Insert::Tape(vt_model::insert::Tape::DEFAULT);
    /// A plate, which takes its tank's lines from the pool.
    const PLATE: Insert = Insert::Plate(vt_model::insert::Plate::DEFAULT);
    /// A pitch shifter, which takes its short lines from the pool.
    const PITCH: Insert = Insert::Pitch(vt_model::insert::Pitch::DEFAULT);
    /// A granular effect, which takes its recording from the pool.
    const GRAINS: Insert = Insert::Grains(vt_model::insert::Grains::DEFAULT);
    /// Warps, which takes its vocoder's filter bank from the pool.
    const WARPS: Insert = Insert::Warps(vt_model::insert::Warps::DEFAULT);
    /// A resonator, which takes its modes from the pool.
    const RESO: Insert = Insert::Reso(vt_model::insert::Resonator::DEFAULT);
    /// A comb, which takes its lines from the pool.
    const COMB: Insert = Insert::Comb(vt_model::insert::Comb::DEFAULT);
    /// A frequency shifter, which takes the lines of its loop from the pool.
    const SHIFT: Insert = Insert::FreqShift(vt_model::insert::FreqShift::DEFAULT);
    /// A rotary speaker, fast, which takes its horn's line from the pool.
    const ROTARY: Insert = Insert::Rotary(vt_model::insert::Rotary {
        fast: 1,
        ..vt_model::insert::Rotary::DEFAULT
    });
    /// A stutter, switched on so it's stuttering as it comes, which takes its pieces from the pool.
    const STUTTER: Insert = Insert::Stutter(vt_model::insert::Stutter {
        trig: 1,
        ..vt_model::insert::Stutter::DEFAULT
    });
    /// A spring reverb, knocked, which takes its springs' lines from the pool.
    const SPRING: Insert = Insert::Spring(vt_model::insert::Spring {
        knock: 1,
        chaos: 0xFF,
        ..vt_model::insert::Spring::DEFAULT
    });
    /// A space, which takes its tank's lines from the pool.
    const SPACE: Insert = Insert::Space(vt_model::insert::Space::DEFAULT);
    /// Clouds, which takes its recording from the pool.
    const CLOUDS: Insert = Insert::Clouds(vt_model::insert::Clouds::DEFAULT);
    /// Beads, which takes its recording from the pool.
    const BEADS: Insert = Insert::Beads(vt_model::insert::Beads::DEFAULT);
    /// A Space Echo, which takes its tape and spring from the pool.
    const ECHO: Insert = Insert::Echo(vt_model::insert::Echo::DEFAULT);

    const BLOCK: usize = 256;

    fn samples() -> Arc<SamplePool> {
        let mut pool = SamplePool::default();
        pool.insert(
            "flat",
            SampleData {
                frames: vec![[0.5, 0.5]; 48_000],
            },
        );
        Arc::new(pool)
    }

    fn song(tracks: usize, steps: usize) -> Song {
        let mut song = Song::new(tracks);
        for track in 0..song.sound_tracks() {
            song.sample_on(track, "flat");
        }
        for id in 0..tracks as u32 {
            let phrase = Arc::make_mut(song.tracks[id as usize].phrases.get_mut(&0).unwrap());
            for step in 0..steps {
                *phrase.step_mut(step).unwrap() = Step {
                    note: Note::On(60),
                    ..Step::default()
                };
            }
        }
        song
    }

    fn published(song: Song, pool: &Arc<SamplePool>) -> Arc<Published> {
        Arc::new(Published::new(Arc::new(song), Arc::clone(pool)))
    }

    /// Run blocks through the guard and return how many times they allocated.
    fn blocks(rt: &mut Realtime, count: usize) -> usize {
        let mut out = vec![0.0; BLOCK * 2];
        let mut total = 0;
        for _ in 0..count {
            let ((), violations) = forbidden(|| rt.process(&mut out));
            total += violations;
        }
        total
    }

    /// Lanes changing, their divisions too, and each track's `Length` and `Rate` for every FX
    /// column turned every block, with `Cue` on for half the tracks (decision 402).
    #[test]
    fn lanes_changing_while_playing_never_allocate_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = song(8, 16);
            for (at, track) in next.tracks.iter_mut().take(8).enumerate() {
                track.cue = (at % 2) as u8;
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for (slot, moved) in track.lanes.iter_mut().enumerate() {
                    let lane = vt_model::song::Lane {
                        division: 1 + (round + slot) % 5,
                        ..vt_model::song::Lane::new((round + slot) % 4, 1 + (round * 3 + slot) % 7)
                    };
                    phrase.set_lane(slot, Some(lane));
                    *moved = vt_model::lane::LaneMove {
                        length: (0x7C + (round * 5 + slot) % 9) as u8,
                        rate: ((round * 7 + slot) % vt_model::lane::LANE_RATES.len()) as u8,
                        start: ((round * 11 + slot) % 9) as u8,
                        jump: ((round * 13 + slot) % 3 * 0x40) as u8,
                    };
                }
            }
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    /// Each track's `Phrase` settings turned every block, through every order and the dealt ones,
    /// reversed and numbered, so every step walks another way (decision 400), its window cued on
    /// and off (decision 402).
    #[test]
    fn the_walk_moving_every_block_never_allocates_in_the_callback() {
        use vt_model::order::Order;
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = song(8, 16);
            for (at, track) in next.tracks.iter_mut().take(8).enumerate() {
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                phrase.set_order(Order::ALL[(round + at) % Order::ALL.len()]);
                track.order = (0x70 + (round * 3 + at) % 0x20) as u8;
                track.reverse = ((round + at) % 2) as u8;
                track.order_x = ((round * 5 + at) % 17) as u8;
                track.order_y = ((round * 7 + at) % 17) as u8;
                track.phrase_length = (0x78 + (round * 3 + at) % 0x12) as u8;
                track.phrase_start = ((round * 5 + at) % 20) as u8;
                track.cue = ((round / 3 + at) % 2) as u8;
            }
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    /// Each track's dice loop turned every block, its length through every choice and past them,
    /// its mutate and jump with it, over rows that roll `CHA` and `RND` and lock `DLP` and `DJMP`,
    /// so its ring fills, drifts and jumps as it plays (decisions 410 and 411).
    #[test]
    fn the_dice_loop_moving_every_block_never_allocates_in_the_callback() {
        let rows = [
            "C-3 -- cha:80 rnd:F3 ---",
            "D-3 -- rnd:F7 diceloop:04 ---",
            "E-3 -- cha:40 dicejump:C0 ---",
            "--- -- --- --- ---",
        ];
        let dice = || {
            let mut song = song(8, 16);
            for track in song.tracks.iter_mut().take(8) {
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for step in 0..16 {
                    *phrase.step_mut(step).unwrap() = rows[step % rows.len()].parse().unwrap();
                }
            }
            song
        };
        let pool = samples();
        let engine = Engine::new(48_000, published(dice(), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = dice();
            for (at, track) in next.tracks.iter_mut().take(8).enumerate() {
                track.dice_loop = ((round + at) % 24) as u8;
                track.dice_mutate = ((round * 37 + at * 11) % 256) as u8;
                track.dice_jump = ((round * 53 + at * 7) % 256) as u8;
            }
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    /// Tracks reseeding their dice with `SED`s on rows and in a lane, over rows that roll `CHA`,
    /// `RND` and `%`, with `Jump`, a lane's `Jump` and the dice loop on, each turned every block
    /// (decision 421).
    #[test]
    fn reseeding_every_block_never_allocates_in_the_callback() {
        use vt_model::lane::Lane;
        let rows = [
            "C-3 -- cha:80 rnd:F3 sed:2A",
            "D-3 -- rnd:F7 cnd:E8 ---",
            "E-3 -- sed:07 cha:40 ---",
            "--- -- --- --- ---",
        ];
        let reseeding = |round: usize| {
            let mut song = song(8, 16);
            for (at, track) in song.tracks.iter_mut().take(8).enumerate() {
                track.jump = ((round * 29 + at * 13) % 256) as u8;
                track.lanes[2].jump = ((round * 31 + at * 5) % 256) as u8;
                track.dice_loop = ((round + at) % 24) as u8;
                track.dice_mutate = ((round * 37 + at * 11) % 256) as u8;
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for step in 0..16 {
                    *phrase.step_mut(step).unwrap() = rows[step % rows.len()].parse().unwrap();
                }
                phrase.set_lane(2, (at % 2 == 0).then(|| Lane::new(0, 3)));
            }
            song
        };
        let pool = samples();
        let engine = Engine::new(48_000, published(reseeding(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            handle.publish(published(reseeding(round), &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    /// Tracks in `Legato`, tying and striking, on synths and samples alike (decision 380).
    #[test]
    fn legato_never_allocates_in_the_callback() {
        let pool = samples();
        let mut legato = song(8, 16);
        for (index, track) in legato.tracks.iter_mut().take(8).enumerate() {
            track.voice_mode = vt_model::song::VoiceMode::Legato;
            // Half let each note go before the next, which then strikes.
            track.gate_length = (index % 4 < 2).then_some(0x80);
            if index % 2 == 0 {
                track.machine =
                    vt_model::song::Machine::Plaits(vt_model::song::SynthVoice::VirtualAnalog);
            }
        }
        let engine = Engine::new(48_000, published(legato, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    /// Tracks whose filter follows the key, through gliding ties and struck notes, on synths and
    /// samples alike (decision 381).
    #[test]
    fn key_tracking_never_allocates_in_the_callback() {
        let pool = samples();
        let mut following = song(8, 16);
        for (index, track) in following.tracks.iter_mut().take(8).enumerate() {
            track.key_track = true;
            track.cutoff = 0x60;
            track.resonance = 0xC0;
            track.glide = if index % 4 < 2 { 0x40 } else { 0 };
            if index % 2 == 0 {
                track.machine =
                    vt_model::song::Machine::Plaits(vt_model::song::SynthVoice::VirtualAnalog);
            }
            // Notes up and down two octaves, every other one tied into the next, so it glides.
            let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
            for step in 0..16 {
                let row = phrase.step_mut(step).unwrap();
                row.note = Note::On(36 + (step as u8 * 7) % 48);
                if step % 2 == 0 {
                    row.gate = Some(vt_model::step::Gate {
                        kind: vt_model::step::GateKind::Hold,
                        pulses: 1,
                        length: None,
                    });
                }
            }
        }
        let engine = Engine::new(48_000, published(following, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    /// Tracks whose steps lock the filter's type, drive, punch, cuts and `Safe`, a type on every
    /// row so each fades from the one before, with an LFO on `Drive` and a macro on `Low cut`
    /// (decision 398).
    #[test]
    fn locks_of_the_filters_settings_never_allocate_in_the_callback() {
        use vt_model::macros::{MacroMap, MacroTarget};
        use vt_model::settings::SettingId;
        let pool = samples();
        let mut locking = song(8, 16);
        for (index, track) in locking.tracks.iter_mut().take(8).enumerate() {
            track.resonance = 0xC0;
            if index % 2 == 0 {
                track.machine =
                    vt_model::song::Machine::Plaits(vt_model::song::SynthVoice::VirtualAnalog);
            }
            track.mods[3] = vt_model::modulation::ModSlot::lfo(
                Some(SettingId::DRIVE),
                vt_model::lfo::Lfo::default(),
            )
            .with_depth(0x40);
            track.macros[0].maps = vec![MacroMap {
                target: MacroTarget::Setting(SettingId::LOW_CUT),
                min: 0,
                max: 0x8000,
                curve: 0,
                s: 0,
                zone: Default::default(),
                member: None,
            }];
            track.macros[0].value = 0x4000;
            let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
            for step in 0..16 {
                let kind = (step + index) % 10;
                let text = format!(
                    "C-3 64 type:{kind:02X} drive:{:02X} punch:80 safe:{:02X} highcut:A0",
                    step * 16,
                    step % 2
                );
                *phrase.step_mut(step).unwrap() = text.parse().unwrap();
            }
        }
        let engine = Engine::new(48_000, published(locking, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    /// Sampler tracks whose steps turn their notes round every other row, through each play mode
    /// and cut, on through included, with an LFO on `SREV` and a macro holding it
    /// (decision 412).
    #[test]
    fn turning_a_samplers_notes_round_never_allocates_in_the_callback() {
        use vt_model::macros::{MacroMap, MacroTarget};
        use vt_model::settings::SettingId;
        let pool = samples();
        let mut turning = song(8, 16);
        for (index, track) in turning.tracks.iter_mut().take(8).enumerate() {
            track.mods[3] = vt_model::modulation::ModSlot::lfo(
                Some(SettingId::SAMPLE_REVERSE),
                vt_model::lfo::Lfo::default(),
            )
            .with_depth(0xFF);
            if index % 2 == 0 {
                track.macros[0].maps = vec![MacroMap {
                    target: MacroTarget::Setting(SettingId::SAMPLE_REVERSE),
                    min: 0,
                    max: 0x0100,
                    curve: 0,
                    s: 0,
                    zone: Default::default(),
                    member: None,
                }];
                track.macros[0].value = 0xFF00;
            }
            let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
            for step in 0..16 {
                let text = format!(
                    "C-3 64 sampler.reverse:{:02X} sampler.play:{:02X} sampler.start:{:02X}",
                    (step + index) % 2,
                    step % 5,
                    step * 8,
                );
                *phrase.step_mut(step).unwrap() = text.parse().unwrap();
            }
        }
        let engine = Engine::new(48_000, published(turning, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    /// Tracks whose rows slide their locks, and the next note's, to where each next changes, back
    /// to the track's own too, a macro's among them, under a window turned every block
    /// (decision 417).
    #[test]
    fn slides_never_allocate_in_the_callback() {
        use vt_model::macros::{MacroMap, MacroTarget};
        use vt_model::settings::SettingId;
        let pool = samples();
        let sliding = |round: usize| {
            let mut sliding = song(8, 16);
            for (index, track) in sliding.tracks.iter_mut().take(8).enumerate() {
                track.phrase_length = (0x7A + (round + index) % 6) as u8;
                track.cue = (index % 2) as u8;
                track.macros[0].maps = vec![MacroMap {
                    target: MacroTarget::Setting(SettingId::RES),
                    min: 0,
                    max: 0xC000,
                    curve: 0,
                    s: 0,
                    zone: Default::default(),
                    member: None,
                }];
                if index % 2 == 0 {
                    track.machine =
                        vt_model::song::Machine::Plaits(vt_model::song::SynthVoice::VirtualAnalog);
                }
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for step in 0..16 {
                    let text = match (step + index) % 4 {
                        0 => format!("C-3 64 cut:{:02X} sld:01 macro1:40", step * 16),
                        1 => "--- -- vol:A0 sld:01 pan:20".to_owned(),
                        2 => "C-3 64 type:04 sld:01 ---".to_owned(),
                        _ => "C-3 64 cut:E0 macro1:F0 vol:60".to_owned(),
                    };
                    *phrase.step_mut(step).unwrap() = text.parse().unwrap();
                }
            }
            sliding
        };
        let engine = Engine::new(48_000, published(sliding(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            handle.publish(published(sliding(round), &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    /// Tracks gliding every note at `Always`, struck, carried and tied (decision 382).
    #[test]
    fn glide_mode_always_never_allocates_in_the_callback() {
        let pool = samples();
        let mut always = song(8, 16);
        for (index, track) in always.tracks.iter_mut().take(8).enumerate() {
            track.glide_mode = vt_model::song::GlideMode::Always;
            track.glide = 0x80;
            if index % 4 < 2 {
                track.voice_mode = vt_model::song::VoiceMode::Legato;
                track.gate_length = Some(0x80);
            }
            if index % 2 == 0 {
                track.machine =
                    vt_model::song::Machine::Plaits(vt_model::song::SynthVoice::VirtualAnalog);
            }
        }
        let engine = Engine::new(48_000, published(always, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    #[test]
    fn a_length_changing_while_playing_never_allocates_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = song(8, 16);
            for track in next.tracks.iter_mut().take(8) {
                Arc::make_mut(track.phrases.get_mut(&0).unwrap()).set_len(1 + round % 16);
            }
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn groups_never_allocate_in_the_callback() {
        let pool = samples();
        let bus = Insert::Compressor(vt_model::insert::Compressor {
            kind: vt_model::insert::CompressorType::Console,
            ..vt_model::insert::Compressor::DEFAULT
        });
        let grouped = |round: usize| {
            let mut next = song(8, 16);
            for (track, sound) in next.tracks.iter_mut().take(8).enumerate() {
                sound.group = (track + round % 3 < 6).then_some((1 + track % 4) as u8);
                sound.send_a = 0x80;
            }
            for group in next
                .tracks
                .iter_mut()
                .skip(8)
                .take(vt_model::constants::MAX_GROUPS)
            {
                group.inserts = [bus, COMB, LIMIT, COMB];
                group.send_b = 0x80;
            }
            next
        };
        let engine = Engine::new(48_000, published(grouped(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        handle.pump();
        let mut violations = blocks(&mut rt, 8);
        for round in 0..200 {
            handle.publish(published(grouped(round), &pool));
            let group = 8 + round % 4;
            handle.send(Command::SetMute {
                track: group,
                mute: round % 5 == 1,
            });
            handle.send(Command::SetSolo {
                track: if round % 2 == 0 { group } else { round % 8 },
                solo: round % 3 == 0,
            });
            handle.pump();
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn publishing_never_allocates_or_frees_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..1000u32 {
            let mut next = song(8, 16);
            next.millibpm = 100_000 + round;
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn choosing_a_long_effect_never_allocates_or_frees_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = song(8, 16);
            if round % 4 == 0 {
                next.tracks[round % 8].inserts = [
                    Insert::ALL[2],
                    Insert::ALL[3],
                    Insert::ALL[4],
                    Insert::ALL[3],
                ];
            } else if round % 4 == 2 {
                next.tracks[round % 8].inserts = [LIMIT, Insert::ALL[2], LIMIT, Insert::ALL[2]];
            } else if round % 8 == 1 {
                next.tracks[round % 8].inserts = [FLANGER, PHASER, FLANGER, PHASER];
            } else if round % 32 == 5 {
                next.tracks[round % 8].inserts = [SHIFT, COMB, STUTTER, COMB];
            } else if round % 32 == 21 {
                next.tracks[round % 8].inserts = [SPACE, SPRING, SPACE, SPRING];
            } else if round % 16 == 13 {
                next.tracks[round % 8].inserts = [SPRING, SHIFT, ROTARY, SHIFT];
            } else if round % 8 == 3 {
                next.tracks[round % 8].inserts = [TAPE, LIMIT, TAPE, LIMIT];
            } else if round % 16 == 7 {
                next.tracks[round % 8].inserts = [PLATE, TAPE, PLATE, TAPE];
            } else if round % 32 == 15 {
                next.tracks[round % 8].inserts = [PITCH, PLATE, PITCH, PLATE];
            } else if round % 64 == 31 {
                next.tracks[round % 8].inserts = [GRAINS, PITCH, GRAINS, PITCH];
            } else if round % 64 == 47 {
                next.tracks[round % 8].inserts = [CLOUDS, PITCH, CLOUDS, GRAINS];
            } else if round % 64 == 55 {
                next.tracks[round % 8].inserts = [BEADS, GRAINS, BEADS, CLOUDS];
            } else if round % 128 == 63 {
                next.tracks[round % 8].inserts = [WARPS, GRAINS, WARPS, GRAINS];
            } else if round % 256 == 127 {
                next.tracks[round % 8].inserts = [RESO, WARPS, RESO, WARPS];
            } else {
                next.tracks[round % 8].inserts = [COMB, RESO, COMB, RESO];
            }
            handle.publish(published(next, &pool));
            handle.pump();
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn long_buffers_go_ahead_of_their_version_and_come_back_to_be_freed() {
        let pool = samples();
        let plain = song(6, 16);
        let needed = MemoryCount::of(&plain);
        let engine = Engine::new(48_000, published(plain.clone(), &pool));
        let (mut handle, mut rt) = connect(engine);
        blocks(&mut rt, 1);
        assert_eq!(rt.engine().spare_memory(), 0, "the returns have theirs");

        let mut more = plain.clone();
        // One of each kind that needs memory, with a slot of its own, the last slot's among them.
        more.tracks[0].inserts = [
            Insert::ALL[2],
            Insert::ALL[3],
            Insert::ALL[4],
            Insert::Empty,
        ];
        more.tracks[1].inserts = [LIMIT, FLANGER, TAPE, Insert::Empty];
        more.tracks[2].inserts = [PLATE, ROTARY, CLOUDS, Insert::Empty];
        more.tracks[3].inserts = [Insert::Empty, PITCH, SPRING, SPACE];
        more.tracks[4].inserts = [Insert::Empty, BEADS, STUTTER, GRAINS];
        more.tracks[5].inserts = [WARPS, COMB, RESO, SHIFT];
        more.tracks[0].inserts[3] = ECHO;
        handle.publish(published(more, &pool));
        let mut one_more = needed;
        for kind in EffectKind::ALL {
            if BUFFERED[kind as usize] {
                one_more[kind] += 1;
            }
        }
        assert_eq!(handle.held, one_more);
        blocks(&mut rt, 1);
        assert_eq!(rt.engine().spare_memory(), 0, "the slots have theirs");

        handle.publish(published(plain, &pool));
        assert_eq!(handle.held, needed);
        // One block to take the version, one to free the slots and one to send them.
        blocks(&mut rt, 3);
        assert_eq!(rt.engine().spare_memory(), 0, "nothing's kept");
        let mut freed = 0;
        while handle.freed.pop().is_ok() {
            freed += 1;
        }
        assert_eq!(freed, 20);
    }

    #[test]
    fn every_distort_type_never_allocates_in_the_callback() {
        let pool = samples();
        let mut song = song(8, 16);
        for (track, kind) in song.tracks.iter_mut().zip(DistortType::ALL) {
            let insert = Insert::Distort(Distort {
                kind,
                ..Distort::default()
            });
            track.inserts = [insert; INSERTS];
        }
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    #[test]
    fn every_effect_never_allocates_in_the_callback() {
        let pool = samples();
        let mut song = song(Insert::ALL.len(), 16);
        assert!(
            song.tracks.len() >= Insert::ALL.len(),
            "a track for every effect"
        );
        for (track, insert) in song.tracks.iter_mut().zip(Insert::ALL) {
            track.inserts = [insert; INSERTS];
        }
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 200), 0);
    }

    #[test]
    fn clouds_changing_mode_quality_and_freeze_never_allocates_in_the_callback() {
        use vt_model::insert::{Clouds, CloudsMode};
        let pool = samples();
        let changing = |round: usize| {
            let mut next = song(2, 16);
            let clouds = Insert::Clouds(Clouds {
                mode: CloudsMode::ALL[round % 4],
                quality: (round / 4 % 4) as u8,
                freeze: (round / 2 % 2) as u8,
                position: (round * 37 % 256) as u8,
                size: (round * 91 % 256) as u8,
                density: (round * 53 % 256) as u8,
                feedback: 0xC0,
                ..Clouds::DEFAULT
            });
            next.tracks[0].inserts = [clouds, Insert::Empty, clouds, Insert::Empty];
            next.tracks[1].inserts = [CLOUDS; INSERTS];
            next
        };
        let engine = Engine::new(48_000, published(changing(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 1..200 {
            handle.publish(published(changing(round), &pool));
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn beads_changing_mode_quality_clock_and_freeze_never_allocates_in_the_callback() {
        use vt_model::insert::{Beads, BeadsMode};
        let pool = samples();
        let changing = |round: usize| {
            let mut next = song(2, 16);
            let beads = Insert::Beads(Beads {
                mode: BeadsMode::ALL[round % 2],
                quality: (round / 2 % 4) as u8,
                clock: (round / 3 % 2) as u8,
                freeze: (round / 5 % 2) as u8,
                time: (round * 37 % 256) as u8,
                size: (round * 91 % 256) as u8,
                density: (round * 53 % 256) as u8,
                random: (round * 29 % 256) as u8,
                feedback: 0xC0,
                ..Beads::DEFAULT
            });
            next.tracks[0].inserts = [beads, Insert::Empty, beads, Insert::Empty];
            next.tracks[1].inserts = [BEADS; INSERTS];
            next
        };
        let engine = Engine::new(48_000, published(changing(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 1..200 {
            handle.publish(published(changing(round), &pool));
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn a_space_echo_changing_mode_speed_and_sync_never_allocates_in_the_callback() {
        use vt_model::insert::Echo;
        let pool = samples();
        let changing = |round: usize| {
            let mut next = song(2, 16);
            let echo = Insert::Echo(Echo {
                mode: (round % 12) as u8,
                rate: (round * 37 % 256) as u8,
                sync: (round / 3 % 4) as u8 * 5,
                intensity: (round * 53 % 256) as u8,
                wow: (round * 29 % 256) as u8,
                bass: (round * 91 % 256) as u8,
                reverb: 0xC0,
                ..Echo::DEFAULT
            });
            next.tracks[0].inserts = [echo, Insert::Empty, echo, Insert::Empty];
            next.tracks[1].inserts = [ECHO; INSERTS];
            next
        };
        let engine = Engine::new(48_000, published(changing(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 1..200 {
            handle.publish(published(changing(round), &pool));
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn modulating_a_slot_never_allocates_in_the_callback() {
        let pool = samples();
        let mut song = song(1, 16);
        let target =
            vt_model::settings::SettingId::slot(0, vt_model::insert::EffectKind::Distort, 1)
                .unwrap();
        song.tracks[0].inserts = [Insert::ALL[1]; INSERTS];
        song.tracks[0].mods[3].destination = Some(target);
        song.tracks[0].mods[3].depth = 0x80;
        song.tracks[0].mods[1].destination = Some(target);
        song.tracks[0].mods[1].depth = 0x40;
        song.tracks[0].mods[4].destination = Some(SettingId::CUT);
        song.tracks[0].mods[3].destination = Some(SettingId::modulator(4, 0).unwrap());
        let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
        for step in 0..8 {
            *phrase.step_mut(step).unwrap() =
                "C-3 -- 1.distort.amount:80g 2.distort.gain:40 3.distort.mix:20h"
                    .parse()
                    .unwrap();
        }
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 400), 0);
    }

    #[test]
    fn attractors_never_allocate_in_the_callback() {
        use vt_model::lfo::{Lfo, LfoMode, Rate, Shape};
        use vt_model::modulation::ModSlot;
        // Lorenz and Rössler in every mode, at a fast rate, seeded at each note in `Trig`, with
        // their shape flipping between them and a sine now and then, and their deform moving.
        let pool = samples();
        let changing = |round: usize| {
            let mut next = song(4, 16);
            for (index, track) in next.tracks.iter_mut().take(4).enumerate() {
                for slot in [1, 3, 5] {
                    let shape = [Shape::Lorenz, Shape::Rossler, Shape::Sine][(round + slot) % 3];
                    let lfo = Lfo {
                        shape,
                        rate: Rate::Hertz(4000),
                        mode: LfoMode::ALL[(index + slot) % 5],
                        deform: (round as i16 * 37) % 0x1FF - 0xFF,
                        ..Lfo::default()
                    };
                    track.mods[slot] = ModSlot::lfo(Some(SettingId::CUT), lfo).with_depth(0x40);
                }
            }
            next
        };
        let engine = Engine::new(48_000, published(changing(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 40);
        for round in 1..200 {
            if round % 4 == 0 {
                handle.publish(published(changing(round), &pool));
            }
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn changing_a_slots_type_every_block_never_allocates_in_the_callback() {
        use vt_model::modulation::ModKind;
        let pool = samples();
        let changing = |round: usize| {
            let mut next = song(4, 16);
            for track in next.tracks.iter_mut().take(4) {
                // The notes' own shape an AHD, a drum or a gate now and then, each a one-shot.
                if round.is_multiple_of(3) {
                    track.mods[0].kind = [ModKind::Ahd, ModKind::Drum, ModKind::Lpg][round / 3 % 3];
                }
                for slot in [1, 3] {
                    let modulator = &mut track.mods[slot];
                    modulator.kind = if round % 2 == slot % 2 {
                        [ModKind::Adsr, ModKind::Ahd, ModKind::Drum, ModKind::Lpg][round / 2 % 4]
                    } else {
                        [ModKind::Lfo, ModKind::Steps, ModKind::Track][round / 2 % 3]
                    };
                    modulator.steps.values =
                        [0x20, -0x20, 0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
                    modulator.steps.rate = vt_model::steps::StepRate::Tick;
                    modulator.destination = Some(SettingId::CUT);
                    modulator.depth = 0x40;
                }
            }
            next
        };
        let engine = Engine::new(48_000, published(changing(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 1..400 {
            handle.publish(published(changing(round), &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn locks_holds_and_glides_never_allocate_in_the_callback() {
        let pool = samples();
        let mut marked = song(8, 16);
        for id in 0..8u32 {
            let phrase = Arc::make_mut(marked.tracks[id as usize].phrases.get_mut(&0).unwrap());
            for step in 0..16 {
                let text = match step % 4 {
                    0 => "C-3 -- cut:40g vol:80 pan:C0h",
                    1 => "--- -- res:20g --- ---",
                    2 => "C-3 -- cut:F0h vol:A0g ---",
                    _ => "--- -- --- res:40 ---",
                };
                *phrase.step_mut(step).unwrap() = text.parse().unwrap();
            }
        }
        let engine = Engine::new(48_000, published(marked, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 2000), 0);
    }

    /// Each row's note with locks of its slots' own times, levels and rates, which its start of
    /// the slots hears (decision 407).
    #[test]
    fn locks_of_a_slots_own_times_never_allocate_in_the_callback() {
        use vt_model::modulation::ModKind;
        let pool = samples();
        let mut marked = song(8, 16);
        for track in marked.tracks.iter_mut().take(8) {
            track.mods[0].kind = ModKind::Adsr;
            track.mods[0].destination = Some(SettingId::VOL);
            track.mods[1].destination = Some(SettingId::CUT);
            track.mods[1].depth = 0x40;
            track.mods[2].kind = ModKind::Steps;
            track.mods[2].destination = Some(SettingId::RES);
            track.mods[2].depth = 0x40;
            let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
            for step in 0..16 {
                let text = match step % 4 {
                    0 => "C-3 -- mod1.decay:40 mod2.rate:C0 mod3.length:03",
                    1 => "C-3 -- mod1.attack:20 mod3.mutate:40 mod3.bias:C0",
                    2 => "C-3 -- mod1.sustain:30g mod2.rate:20h mod3.jump:20",
                    _ => "C-3 -- mod1.release:A0 mod3.smooth:F0 mod3.spread:10",
                };
                *phrase.step_mut(step).unwrap() = text.parse().unwrap();
            }
        }
        let engine = Engine::new(48_000, published(marked, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert_eq!(blocks(&mut rt, 2000), 0);
    }

    #[test]
    fn a_preview_through_a_track_never_allocates_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        let mut violations = 0;
        for round in 0..200 {
            assert!(handle.send(Command::Preview {
                track: round % 8,
                note: 60,
                velocity: 0x7F,
            }));
            violations += blocks(&mut rt, 2);
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn held_chords_and_taking_voices_over_never_allocate_in_the_callback() {
        use vt_model::song::{Machine, SynthVoice};
        let pool = samples();
        let mut song = song(4, 16);
        song.tracks[1].machine = Machine::Plaits(SynthVoice::Acid);
        song.tracks[2].machine = Machine::Plaits(SynthVoice::SixOp);
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = 0;
        for round in 0..40 {
            let track = round % 4;
            // Twelve keys down, so the bank of eight is taken over, and on `Acid` they slide.
            for note in 0..12 {
                assert!(handle.send(Command::HoldPreview {
                    track,
                    note: 48 + note,
                    velocity: 0x7F,
                }));
                violations += blocks(&mut rt, 1);
            }
            for note in (0..12).rev() {
                let command = if note % 2 == 0 {
                    Command::ReleasePreview {
                        track,
                        note: 48 + note,
                    }
                } else {
                    Command::FinishPreview {
                        track,
                        note: 48 + note,
                    }
                };
                assert!(handle.send(command));
                violations += blocks(&mut rt, 1);
            }
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn changing_track_count_and_removing_a_phrase_dont_allocate() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        let mut violations = blocks(&mut rt, 4);
        for tracks in [MAX_SOUND_TRACKS, 1, 8] {
            handle.publish(published(song(tracks, 16), &pool));
            violations += blocks(&mut rt, 4);
            handle.pump();
        }
        // Launching, reporting it and capturing don't allocate either.
        handle.send(Command::PlayFrom { step: 0 });
        for track in 0..8 {
            handle.send(Command::Launch {
                track,
                chain: (track % 2 == 0).then_some(0),
            });
        }
        handle.send(Command::ContinueIn {
            section: 0,
            length: 4,
        });
        violations += blocks(&mut rt, 64);
        while handle.jam_change().is_some() {}
        let mut removed = song(8, 16);
        removed.tracks[0].phrases.remove(&0);
        handle.publish(published(removed, &pool));
        violations += blocks(&mut rt, 4);
        assert_eq!(violations, 0);
    }

    #[test]
    fn every_fx_command_stays_off_the_allocator() {
        let pool = samples();
        let mut fx = Song::new(2);
        for track in 0..fx.sound_tracks() {
            fx.sample_on(track, "flat");
        }
        let texts = [
            "C-3 01 vol:80 pan:20 cut:40",
            "C-3 -- res:C0 sna:80 snb:80",
            "C-3 -- del:02 kil:03 off:01",
            "C-3 -- ret:52 cha:80 ---",
            "OFF -- --- --- ---",
        ];
        for id in 0..2 {
            let phrase = Arc::make_mut(fx.tracks[id as usize].phrases.get_mut(&0).unwrap());
            for (index, text) in texts.iter().enumerate() {
                *phrase.step_mut(index).unwrap() = text.parse().unwrap();
            }
        }
        let engine = Engine::new(48_000, published(fx, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        assert_eq!(blocks(&mut rt, 2000), 0);
    }

    #[test]
    fn launching_on_the_grid_and_across_rows_stays_off_the_allocator() {
        let pool = samples();
        let mut played = Song::new(2);
        for track in 0..played.sound_tracks() {
            played.sample_on(track, "flat");
        }
        for track in &mut played.tracks {
            track.add_chain(1);
        }
        played.launch = vt_model::song::Launch::every(2).unwrap();
        played.sections[0].length = Some(4);
        let mut second = played.sections[0].clone();
        second.chains[1] = Some(1);
        played.sections.push(second);
        let engine = Engine::new(48_000, published(played, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        for round in 0..20 {
            handle.send(Command::Launch {
                track: round % 2,
                chain: (round % 3 > 0).then_some(1),
            });
            assert_eq!(blocks(&mut rt, 4), 0);
        }
    }

    #[test]
    fn playing_from_a_row_and_wrapping_stay_off_the_allocator() {
        let pool = samples();
        let mut played = Song::new(2);
        for track in 0..played.sound_tracks() {
            played.sample_on(track, "flat");
        }
        for track in &mut played.tracks {
            track.add_chain(1);
        }
        played.sections[0].length = Some(2);
        played.sections[0].repeat = vt_model::song::Repeat::new(3);
        let mut second = played.sections[0].clone();
        second.chains[1] = Some(1);
        second.reset = Some(1);
        second.start = vt_model::song::SectionStart::Reset;
        played.sections.push(second);
        let engine = Engine::new(48_000, published(played, &pool));
        let (mut handle, mut rt) = connect(engine);
        for round in 0..20 {
            handle.send(Command::PlaySection {
                section: round % 2,
                step: round,
                track: (round % 3 == 0).then_some(0),
            });
            assert_eq!(blocks(&mut rt, 4), 0);
        }
    }

    #[test]
    fn synths_stay_off_the_allocator() {
        let pool = samples();
        let voices = vt_model::song::SynthVoice::ALL;
        let mut played = Song::new(voices.len());
        for (track, voice) in played.tracks.iter_mut().zip(voices) {
            track.machine = vt_model::song::Machine::Plaits(voice);
            let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
            // A voice with only two settings locks its second where others lock their third.
            let count = vt_model::settings::voice_entries(voice).len();
            let key = |setting: usize| {
                vt_model::settings::SettingId::voice(voice, setting.min(count - 1))
                    .unwrap()
                    .key()
            };
            *phrase.step_mut(0).unwrap() = format!("C-1 7F {}:40 --- ---", key(1)).parse().unwrap();
            // Half way up the first setting, which may be a list as short as the chords.
            let first = vt_model::settings::SettingId::voice(voice, 0)
                .unwrap()
                .entry()
                .kind
                .top()
                / 2;
            *phrase.step_mut(4).unwrap() =
                format!("G-1 7F {}:20 {}:{first:02X} ---", key(2), key(0))
                    .parse()
                    .unwrap();
            // A plain note held into the next, which `Acid` slides to.
            *phrase.step_mut(2).unwrap() = "D-1 64 HLD1".parse().unwrap();
            *phrase.step_mut(3).unwrap() = "E-1 64".parse().unwrap();
        }
        let engine = Engine::new(48_000, published(played, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        assert_eq!(blocks(&mut rt, 64), 0);
    }

    #[test]
    fn audition_stays_off_the_allocator() {
        let pool = samples();
        let mut played = Song::new(2);
        for track in 0..played.sound_tracks() {
            played.sample_on(track, "flat");
        }
        for id in 0..2 {
            let phrase = Arc::make_mut(played.tracks[id as usize].phrases.get_mut(&0).unwrap());
            *phrase.step_mut(0).unwrap() = "C-3 01 vol:80 --- ---".parse().unwrap();
        }
        let engine = Engine::new(48_000, published(played, &pool));
        let (mut handle, mut rt) = connect(engine);
        for step in 0..50 {
            handle.send(Command::Audition {
                section: 0,
                section_step: step,
                track: None,
            });
            assert_eq!(blocks(&mut rt, 4), 0);
        }
    }

    #[test]
    fn the_newest_version_always_arrives() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(1, 0), &pool));
        let (mut handle, mut rt) = connect(engine);
        for bpm in 100..200u32 {
            let mut next = song(1, 0);
            next.millibpm = bpm * 1000;
            handle.publish(published(next, &pool));
        }
        assert!(handle.has_pending(), "most should wait while audio is idle");
        // With no further edits, pumping each frame still gets the last one through.
        for _ in 0..4 {
            blocks(&mut rt, 1);
            handle.pump();
        }
        assert!(!handle.has_pending());
        assert_eq!(rt.engine().current().song.millibpm, 199_000);
    }

    #[test]
    fn a_full_return_queue_keeps_the_old_version_playing() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(1, 0), &pool));
        let (mut handle, mut rt) = connect(engine);
        // Never pump, so nothing is freed and the return queue fills up.
        for bpm in 0..(RETURN_QUEUE as u32 + 4) {
            let mut next = song(1, 0);
            next.millibpm = 100_000 + bpm;
            while handle.versions.is_full() {
                blocks(&mut rt, 1);
                if rt.returned.is_full() {
                    break;
                }
            }
            if rt.returned.is_full() {
                break;
            }
            let _ = handle.versions.push(Handover::new(published(next, &pool)));
        }
        let current = rt.engine().current().song.millibpm;
        assert_eq!(blocks(&mut rt, 2), 0);
        assert_eq!(rt.engine().current().song.millibpm, current);
        handle.pump();
        blocks(&mut rt, 1);
    }

    #[test]
    fn playing_again_restarts_and_tab_pauses() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(1, 1), &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        handle.send(Command::PlayFrom { step: 0 });
        blocks(&mut rt, 1);
        assert_eq!(rt.engine().transport(), Transport::Playing);
        handle.send(Command::PauseOrResume);
        blocks(&mut rt, 1);
        assert_eq!(rt.engine().transport(), Transport::Paused);
    }

    #[test]
    fn the_scope_tap_never_allocates_and_drops_what_doesnt_fit() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(1, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        // Nothing reads it, so it fills, and the callback carries on regardless.
        let blocks_to_fill = SCOPE_QUEUE / BLOCK + 4;
        assert_eq!(blocks(&mut rt, blocks_to_fill), 0);
        let mut taken = Vec::new();
        handle.read_scope(&mut taken);
        assert_eq!(taken.len(), SCOPE_QUEUE, "the newest are dropped");
        assert!(taken.iter().any(|sample| *sample != 0.0));
        handle.read_scope(&mut taken);
        assert_eq!(taken.len(), SCOPE_QUEUE, "nothing is read twice");

        // With room again, a block arrives whole, as the two channels mixed.
        let mut out = vec![0.0; BLOCK * 2];
        let ((), violations) = forbidden(|| rt.process(&mut out));
        assert_eq!(violations, 0);
        let mut next = Vec::new();
        handle.read_scope(&mut next);
        let mixed: Vec<f32> = out
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[l, r]| (l + r) * 0.5)
            .collect();
        assert_eq!(next, mixed);
    }

    #[test]
    fn telemetry_reports_position_and_transport() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(1, 1), &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 3 });
        blocks(&mut rt, 1);
        let report = handle.latest().unwrap();
        assert_eq!(report.transport, Transport::Playing);
        assert_eq!(report.position, 3);
    }

    #[test]
    fn the_reading_keeps_the_loudest_hit_across_reports() {
        let pool = samples();
        let mut one = song(1, 0);
        *Arc::make_mut(one.tracks[0].phrases.get_mut(&0).unwrap())
            .step_mut(0)
            .unwrap() = "C-3 -- --- --- ---".parse().unwrap();
        let engine = Engine::new(48_000, published(one, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert_eq!(blocks(&mut rt, 1), 0);
        handle.send(Command::Audition {
            section: 0,
            section_step: 0,
            track: None,
        });
        assert_eq!(blocks(&mut rt, 2), 0);
        handle.send(Command::Audition {
            section: 0,
            section_step: 1,
            track: None,
        });
        assert_eq!(blocks(&mut rt, 2), 0, "the audition has been cut");
        let reading = handle.latest().unwrap().reading;
        // A 0.5 sample, which the limiter leaves alone.
        assert!((reading.before[0] - 0.5).abs() < 1e-3, "{reading:?}");
        assert_eq!(reading.after[0], reading.before[0]);
        assert_eq!(reading.frames, BLOCK as u32 * 5);
        assert!(reading.power[0] > 0.0);
        assert_eq!(handle.latest(), None, "nothing is counted twice");
    }

    #[test]
    fn the_level_after_the_trim_is_reported_for_the_track_watched_without_allocating() {
        let pool = samples();
        let mut two = song(2, 16);
        // The first track's trim at +12 dB, its filter shut and its fader down, so only a level
        // taken after the trim and before the filter reads anything.
        two.tracks[0].trim = vt_model::constants::UNITY_TRIM + 48;
        two.tracks[0].filter = vt_model::song::FilterType::HighPass;
        two.tracks[0].cutoff = 0xFF;
        two.tracks[0].level = 0;
        let engine = Engine::new(48_000, published(two, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        assert_eq!(blocks(&mut rt, 4), 0, "the tap never allocates");
        let report = handle.latest().unwrap();
        assert_eq!(report.watched, 0);
        let boosted = 0.5 * vt_model::song::trim_gain(vt_model::constants::UNITY_TRIM + 48);
        assert!(
            (report.live.after_trim[0] - boosted).abs() < 1e-3,
            "{:?}",
            report.live.after_trim
        );
        assert!(report.tracks[0].before[0] < 1e-3, "the fader is down");
        // The loudest of the reports since the last look is kept, and only the track watched is.
        handle.send(Command::Watch { track: 1 });
        assert_eq!(blocks(&mut rt, 3), 0);
        let report = handle.latest().unwrap();
        assert_eq!(report.watched, 1);
        assert!((report.live.after_trim[0] - 0.5).abs() < 1e-3, "{report:?}");
    }

    #[test]
    fn each_lfos_place_in_its_wave_is_reported_for_the_track_watched_without_allocating() {
        use vt_model::constants::TICKS_PER_QUARTER;
        use vt_model::lfo::{Lfo, Rate, Shape, wave};
        use vt_model::modulation::ModSlot;
        let pool = samples();
        let mut one = song(1, 16);
        let quarter = Rate::Note(TICKS_PER_QUARTER as u32);
        let lfo = |shape, phase, bypassed| {
            let wave = Lfo {
                shape,
                rate: quarter,
                phase,
                ..Lfo::default()
            };
            ModSlot {
                bypassed,
                ..ModSlot::lfo(Some(SettingId::CUT), wave).with_depth(0x40)
            }
        };
        one.tracks[0].mods[3..].copy_from_slice(&[
            lfo(Shape::Sine, 90, false),
            // Going nowhere, and switched out from the start, neither runs.
            ModSlot {
                destination: None,
                ..lfo(Shape::Tri, 0, false)
            },
            lfo(Shape::Ramp, 0, true),
        ]);
        let engine = Engine::new(48_000, published(one, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert_eq!(blocks(&mut rt, 2), 0);
        assert_eq!(
            handle.latest().unwrap().live.mods,
            [None; 6],
            "stopped, none moves"
        );
        handle.send(Command::PlayFrom { step: 0 });
        let mut phases = Vec::new();
        for _ in 0..8 {
            assert_eq!(blocks(&mut rt, 1), 0, "the report never allocates");
            let mods = handle.latest().unwrap().live.mods;
            assert_eq!(mods[..3], [None; 3], "the envelopes are no LFOs: {mods:?}");
            assert_eq!(mods[4..], [None, None], "{mods:?}");
            let now = mods[3].unwrap();
            // Its value is the wave's where it says it is, its `Phase` of a quarter turn included.
            let turns = f64::from(now.phase) / 65536.0;
            let expected = wave(Shape::Sine, turns, 0) * f32::from(i16::MAX);
            assert!(
                (f32::from(now.value) - expected).abs() < 40.0,
                "{now:?} {expected}"
            );
            phases.push(now.phase);
        }
        // It starts a quarter of the way round, and goes on round as the song plays: a quarter
        // note is 24,000 frames at 120 BPM, and each block 256 of them.
        assert!(phases[0].abs_diff(16384) < 2000, "{phases:?}");
        assert!(
            phases.windows(2).all(|pair| pair[1] > pair[0]),
            "{phases:?}"
        );
        let turn = f64::from(phases[7] - phases[0]) / 65536.0;
        let expected = 7.0 * BLOCK as f64 / 24_000.0;
        assert!((turn - expected).abs() < 0.01, "{turn} {expected}");
        // Stopped again, it's gone.
        handle.send(Command::Return);
        assert_eq!(blocks(&mut rt, 2), 0);
        assert_eq!(handle.latest().unwrap().live.mods, [None; 6]);
    }

    #[test]
    fn what_mutate_drew_is_reported_for_the_track_watched_without_allocating() {
        use vt_model::modulation::ModSlot;
        use vt_model::steps::{Steps, StepsMode};
        let pool = samples();
        let mut one = song(1, 16);
        let drifting = Steps {
            values: [0x10; 16],
            length: 4,
            mutate: 100,
            mode: StepsMode::Free,
            ..Steps::default()
        };
        one.tracks[0].mods[3] = ModSlot::steps(Some(SettingId::CUT), drifting);
        // The same, at no `Mutate`, has nothing drawn to report.
        one.tracks[0].mods[4] = ModSlot::steps(
            Some(SettingId::PAN),
            Steps {
                mutate: 0,
                ..drifting
            },
        );
        // And one drawn through a bent `Spread` and `Bias`, which allocates nothing either.
        one.tracks[0].mods[5] = ModSlot::steps(
            Some(SettingId::RES),
            Steps {
                spread: 20,
                bias: 30,
                ..drifting
            },
        );
        let engine = Engine::new(48_000, published(one, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        // Into its second 16th, each step drawn again as it comes round.
        assert_eq!(blocks(&mut rt, 40), 0, "the report never allocates");
        let drawn = handle.latest().unwrap().live.drawn;
        assert_eq!(drawn[3], rt.engine().live(0).drawn[3]);
        assert_eq!(drawn[3].which, 0b11, "{drawn:?}");
        assert!(drawn[3].value(0).is_some() && drawn[3].value(2).is_none());
        assert_eq!(drawn[4], Drawn::default());
        assert_eq!(drawn[5].which, 0b11, "{drawn:?}");
    }

    #[test]
    fn what_the_slots_move_each_notes_pitch_by_is_reported_without_allocating() {
        use vt_model::modulation::ModSlot;
        use vt_model::steps::{Steps, StepsMode};
        let pool = samples();
        // Every track a note a step, the first with Steps on `Pitch`, up 2 then 5, one a note.
        let mut song = song(2, 16);
        let up = Steps {
            values: std::array::from_fn(|step| [2, 5][step % 2]),
            length: 2,
            mode: StepsMode::Note,
            ..Steps::default()
        };
        song.tracks[0].mods[3] = ModSlot::steps(Some(SettingId::PITCH), up);
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        // Three 16ths, at 6000 frames each.
        assert_eq!(blocks(&mut rt, 70), 0, "reporting never allocates");
        let mut heard = Vec::new();
        while let Some((track, pitch)) = handle.note_pitch() {
            heard.push((track, pitch.row.row, pitch.moves[3]));
            assert_eq!(
                pitch.moves[..3],
                [0.0; 3],
                "only the slot on Pitch moves it"
            );
        }
        // The other track has nothing on `Pitch`, so it reports nothing.
        assert_eq!(heard, [(0, 0, 2.0), (0, 1, 5.0), (0, 2, 2.0)]);
    }

    #[test]
    fn macros_moved_by_turns_locks_and_lfos_never_allocate_in_the_callback() {
        use vt_model::lfo::{Lfo, Rate};
        use vt_model::macros::{MacroMap, MacroTarget};
        use vt_model::modulation::{ModKind, ModSlot};
        use vt_model::song::{Machine, SynthVoice};

        let pool = samples();
        let turned = |round: usize| {
            let mut next = song(8, 16);
            for (index, track) in next.tracks.iter_mut().take(8).enumerate() {
                if index % 2 == 1 {
                    track.machine = Machine::Plaits(SynthVoice::VirtualAnalog);
                }
                track.inserts[0] = FLANGER;
                track.mods[4].kind = ModKind::Steps;
                track.mods[4].destination = Some(SettingId::RES);
                track.mods[4].depth = 0x40;
                track.mods[5].destination = Some(SettingId::PAN);
                track.mods[5].depth = 0x40;
                track.mods[3] = ModSlot::lfo(
                    SettingId::macro_at(0),
                    Lfo {
                        rate: Rate::Steps(1),
                        ..Lfo::default()
                    },
                )
                .with_depth(0x60);
                let targets = [
                    SettingId::CUT,
                    SettingId::voice(SynthVoice::VirtualAnalog, 0).unwrap(),
                    SettingId::slot(0, vt_model::insert::EffectKind::Flanger, 0).unwrap(),
                    SettingId::mod_own(1, 1).unwrap(),
                    SettingId::mod_own(4, 7).unwrap(),
                    // Steps' and an LFO's `Rate`, which keep a running phase.
                    SettingId::mod_own(4, 9).unwrap(),
                    SettingId::mod_own(5, 9).unwrap(),
                    SettingId::accumulation(0).unwrap(),
                    SettingId::arpeggio(1).unwrap(),
                    // The accumulator's and arpeggio's lists (decision 413).
                    SettingId::accumulation(2).unwrap(),
                    SettingId::accumulation(3).unwrap(),
                    SettingId::accumulation(4).unwrap(),
                    SettingId::arpeggio(2).unwrap(),
                ];
                // Bent, as a mapping's response is worked out each block.
                let bend = [0, 60, -80][index % 3];
                for target in targets {
                    track.macros[index % 2]
                        .maps
                        .push(MacroMap::whole(MacroTarget::Setting(target)).bent(bend, -bend / 2));
                }
                track.macros[1].value = (round as u16 * 0x0140) % 0xFF00;
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for step in (0..16).step_by(3) {
                    *phrase.step_mut(step).unwrap() =
                        "C-3 -- macro2:C0 acu.adds:01 arp.mode:04".parse().unwrap();
                }
            }
            next
        };
        let engine = Engine::new(48_000, published(turned(0), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        handle.pump();
        let mut violations = blocks(&mut rt, 8);
        for round in 0..200 {
            handle.publish(published(turned(round), &pool));
            violations += blocks(&mut rt, 2);
            handle.pump();
        }
        assert_eq!(violations, 0);
    }

    #[test]
    fn a_silent_track_count_of_one_eight_and_sixty_four_all_run() {
        let pool = samples();
        for tracks in [1, 8, 64] {
            let engine = Engine::new(48_000, published(song(tracks, 16), &pool));
            let (mut handle, mut rt) = connect(engine);
            handle.send(Command::PlayFrom { step: 0 });
            let mut out = vec![0.0; BLOCK * 2];
            rt.process(&mut out);
            assert!(out.iter().any(|s| *s != 0.0), "{tracks} tracks");
        }
    }

    /// A sample with room for so many frames, its first chunks in place, as the edit thread makes
    /// them ahead of the recorder.
    fn take(frames: u64) -> Arc<Growing> {
        let growing = Growing::new(frames);
        growing.make_room(48_000);
        Arc::new(growing)
    }

    #[test]
    fn recording_and_playing_what_records_never_allocate_in_the_callback() {
        // Track 2 plays the recording as it's written, while each source is recorded in turn.
        let recording = take(48_000 * 20);
        let mut pool = SamplePool::clone(&samples());
        pool.insert_growing("recording", Arc::clone(&recording));
        let pool = Arc::new(pool);
        let mut song = song(4, 16);
        song.sample_on(2, "recording");
        let engine = Engine::new(48_000, published(song, &pool));
        let (mut handle, mut rt) = connect(engine);
        let (mut input, heard) = RingBuffer::new(INPUT_QUEUE);
        rt.listen(heard);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        assert!(handle.record(
            Arc::clone(&recording),
            RecordSource::Track(0),
            RecordQuantise::Bar,
            0
        ));
        let mut violations = 0;
        let mut ended = 0;
        let sources = [
            (RecordSource::Master, RecordQuantise::Now),
            (RecordSource::Input, RecordQuantise::Beat),
            (RecordSource::Track(1), RecordQuantise::Bar),
            (RecordSource::Track(1), RecordQuantise::Now),
        ];
        for round in 0..400 {
            // The edit thread keeps chunks ahead, and the input comes in.
            recording.make_room(48_000);
            for at in 0..BLOCK * 2 {
                let _ = input.push((at as f32 * 0.01).sin());
            }
            if round % 50 == 25 {
                let quantise = sources[(round / 50) % sources.len()].1;
                assert!(handle.send(Command::StopRecording {
                    quantise,
                    latency: 64
                }));
            }
            if round % 50 == 49 {
                let (source, quantise) = sources[(round / 50) % sources.len()];
                assert!(handle.send(Command::ListenTo { source }));
                let fresh = take(48_000 * 4);
                assert!(handle.record(fresh, source, quantise, 64));
            }
            if round == 390 {
                assert!(handle.send(Command::CancelRecording));
            }
            violations += blocks(&mut rt, 2);
            handle.pump();
            while let Some(event) = handle.recorder_event() {
                if let RecorderEvent::Ended(_) = event {
                    ended += 1;
                }
            }
        }
        assert_eq!(violations, 0);
        assert!(ended >= 8, "{ended} recordings ended");
    }

    /// A report is copied into its ring every block, so its size is watched. It's about 14 KB,
    /// nearly all of it the arrays of a place for every track there can be, `places` most of all.
    /// Filling those took about 3 µs a block in a release build, a few percent of a light song's
    /// block at 256 frames, so they're still sent every block; past this, it's time to send them
    /// less often or only for the tracks in use.
    #[test]
    fn a_report_stays_under_sixteen_kilobytes() {
        let size = size_of::<Telemetry>();
        assert!(size <= 16 * 1024, "{size} bytes");
    }
}
