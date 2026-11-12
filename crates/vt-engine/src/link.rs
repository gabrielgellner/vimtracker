//! The queues between the edit thread and the audio callback.
//!
//! `rtrb` can't take back a queued item, so the newest unsent version waits in a slot on the edit
//! thread. Every edit and every frame tries to send it, so the last edit always reaches audio.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rtrb::{Consumer, Producer, RingBuffer};
use vt_model::constants::{
    COMMAND_QUEUE, FILE_QUEUE, FX_SLOTS, MAX_TRACKS, MEMORY_QUEUE, PUBLISH_QUEUE, RETURN_QUEUE,
    SCOPE_QUEUE, TELEMETRY_QUEUE,
};

use vt_model::sample::SampleData;
use vt_model::scale::Scale;

use crate::dsp::memory::{Memory, MemoryCount};
use crate::engine::{Engine, PLAYHEADS, Playhead, SectionLoop, TrackPlace, Transport};
use crate::mixer::Reading;
use crate::published::{Published, Span};
use crate::track_state::{Faders, LiveValues};

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
    /// What's added to that count for each FX column's lane.
    pub lane_offsets: [i64; FX_SLOTS],
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
    versions: Producer<Arc<Published>>,
    returned: Consumer<Arc<Published>>,
    commands: Producer<Command>,
    telemetry: Consumer<Telemetry>,
    /// The main output, mixed to one channel, for the scope.
    scope: Consumer<f32>,
    jam: Consumer<(usize, JamState)>,
    pending: Option<Arc<Published>>,
    memories: Producer<Memory>,
    freed: Consumer<Memory>,
    /// One-off files to hear, each with how much of it plays, and those the callback is done
    /// with, to be freed here.
    files: Producer<(Arc<SampleData>, Option<Span>)>,
    heard: Consumer<Arc<SampleData>>,
    /// Long buffers that can't go until there's room in the queue. A version waits for them.
    unsent: Vec<Memory>,
    /// How many long buffers the callback has or is getting, beyond any it's been told to release.
    held: MemoryCount,
    rate: u32,
}

/// The callback's end. Nothing here allocates, frees or locks.
pub struct Realtime {
    engine: Engine,
    versions: Consumer<Arc<Published>>,
    returned: Producer<Arc<Published>>,
    commands: Consumer<Command>,
    telemetry: Producer<Telemetry>,
    scope: Producer<f32>,
    jam: Producer<(usize, JamState)>,
    memories: Consumer<Memory>,
    freed: Producer<Memory>,
    files: Consumer<(Arc<SampleData>, Option<Span>)>,
    heard: Producer<Arc<SampleData>>,
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
    let (memory_tx, memory_rx) = RingBuffer::new(MEMORY_QUEUE);
    let (freed_tx, freed_rx) = RingBuffer::new(MEMORY_QUEUE);
    let (file_tx, file_rx) = RingBuffer::new(FILE_QUEUE);
    // Room for every file that can be queued, and one playing, so one always fits.
    let (heard_tx, heard_rx) = RingBuffer::new(FILE_QUEUE + 1);
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
            pending: None,
            memories: memory_tx,
            freed: freed_rx,
            files: file_tx,
            heard: heard_rx,
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
            memories: memory_rx,
            freed: freed_tx,
            files: file_rx,
            heard: heard_tx,
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
        self.pending = Some(version);
        self.pump();
    }

    /// Make the long buffers a version needs, or ask for back those it doesn't. They go ahead of
    /// it, so what it needs is there when it arrives.
    fn meet_memory_needs(&mut self, needed: MemoryCount) {
        for _ in self.held.delay..needed.delay {
            self.unsent.push(Memory::delay(self.rate));
        }
        for _ in self.held.reverb..needed.reverb {
            self.unsent.push(Memory::reverb(self.rate));
        }
        for _ in self.held.chorus..needed.chorus {
            self.unsent.push(Memory::chorus(self.rate));
        }
        for _ in self.held.limit..needed.limit {
            self.unsent.push(Memory::limit(self.rate));
        }
        for _ in self.held.flanger..needed.flanger {
            self.unsent.push(Memory::flanger(self.rate));
        }
        for _ in self.held.tape..needed.tape {
            self.unsent.push(Memory::tape(self.rate));
        }
        for _ in self.held.plate..needed.plate {
            self.unsent.push(Memory::plate(self.rate));
        }
        for _ in self.held.pitch..needed.pitch {
            self.unsent.push(Memory::pitch(self.rate));
        }
        for _ in self.held.grains..needed.grains {
            self.unsent.push(Memory::grains(self.rate));
        }
        for _ in self.held.warps..needed.warps {
            self.unsent.push(Memory::warps(self.rate));
        }
        for _ in self.held.reso..needed.reso {
            self.unsent.push(Memory::reso(self.rate));
        }
        for _ in self.held.comb..needed.comb {
            self.unsent.push(Memory::comb(self.rate));
        }
        for _ in self.held.freq_shift..needed.freq_shift {
            self.unsent.push(Memory::freq_shift(self.rate));
        }
        let spare = MemoryCount {
            delay: self.held.delay.saturating_sub(needed.delay),
            reverb: self.held.reverb.saturating_sub(needed.reverb),
            chorus: self.held.chorus.saturating_sub(needed.chorus),
            limit: self.held.limit.saturating_sub(needed.limit),
            flanger: self.held.flanger.saturating_sub(needed.flanger),
            tape: self.held.tape.saturating_sub(needed.tape),
            plate: self.held.plate.saturating_sub(needed.plate),
            pitch: self.held.pitch.saturating_sub(needed.pitch),
            grains: self.held.grains.saturating_sub(needed.grains),
            warps: self.held.warps.saturating_sub(needed.warps),
            reso: self.held.reso.saturating_sub(needed.reso),
            comb: self.held.comb.saturating_sub(needed.comb),
            freq_shift: self.held.freq_shift.saturating_sub(needed.freq_shift),
        };
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

    /// Whether a version is still waiting to be sent.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Realtime {
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
            let old = self.engine.swap(version);
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
            }
        }
        while !self.freed.is_full()
            && let Some(memory) = self.engine.next_freed_memory()
        {
            let _ = self.freed.push(memory);
        }
        let started = Instant::now();
        self.engine.process(out);
        self.engine_time = started.elapsed();
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

    #[test]
    fn lanes_changing_while_playing_never_allocate_in_the_callback() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        assert!(handle.send(Command::PlayFrom { step: 0 }));
        let mut violations = blocks(&mut rt, 4);
        for round in 0..200 {
            let mut next = song(8, 16);
            for track in next.tracks.iter_mut().take(8) {
                let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
                for slot in 0..FX_SLOTS {
                    let lane = vt_model::song::Lane {
                        start: (round + slot) % 4,
                        length: 1 + (round * 3 + slot) % 7,
                    };
                    phrase.set_lane(slot, Some(lane));
                }
            }
            handle.publish(published(next, &pool));
            violations += blocks(&mut rt, 1);
            handle.pump();
        }
        assert_eq!(violations, 0);
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
            for group in 8..8 + vt_model::constants::MAX_GROUPS {
                next.tracks[group].inserts = [bus, COMB, LIMIT, COMB];
                next.tracks[group].send_b = 0x80;
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
            } else if round % 8 == 5 {
                next.tracks[round % 8].inserts = [SHIFT, COMB, SHIFT, COMB];
            } else if round % 8 == 3 {
                next.tracks[round % 8].inserts = [TAPE, LIMIT, TAPE, LIMIT];
            } else if round % 16 == 7 {
                next.tracks[round % 8].inserts = [PLATE, TAPE, PLATE, TAPE];
            } else if round % 32 == 15 {
                next.tracks[round % 8].inserts = [PITCH, PLATE, PITCH, PLATE];
            } else if round % 64 == 31 {
                next.tracks[round % 8].inserts = [GRAINS, PITCH, GRAINS, PITCH];
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
        more.tracks[2].inserts = [PLATE, Insert::Empty, Insert::Empty, Insert::Empty];
        more.tracks[3].inserts = [Insert::Empty, PITCH, Insert::Empty, Insert::Empty];
        more.tracks[4].inserts = [Insert::Empty, Insert::Empty, Insert::Empty, GRAINS];
        more.tracks[5].inserts = [WARPS, COMB, RESO, SHIFT];
        handle.publish(published(more, &pool));
        assert_eq!(
            handle.held,
            MemoryCount {
                delay: needed.delay + 1,
                reverb: needed.reverb + 1,
                chorus: needed.chorus + 1,
                limit: needed.limit + 1,
                flanger: needed.flanger + 1,
                tape: needed.tape + 1,
                plate: needed.plate + 1,
                pitch: needed.pitch + 1,
                grains: needed.grains + 1,
                warps: needed.warps + 1,
                reso: needed.reso + 1,
                comb: needed.comb + 1,
                freq_shift: needed.freq_shift + 1,
            }
        );
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
        assert_eq!(freed, 13);
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
    fn modulating_a_slot_never_allocates_in_the_callback() {
        let pool = samples();
        let mut song = song(1, 16);
        let target =
            vt_model::settings::SettingId::slot(0, vt_model::settings::Effect::Distort, 1).unwrap();
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
                        ModKind::Lfo
                    };
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
            let key = |setting| {
                vt_model::settings::SettingId::voice(voice, setting)
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
            let _ = handle.versions.push(published(next, &pool));
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
}
