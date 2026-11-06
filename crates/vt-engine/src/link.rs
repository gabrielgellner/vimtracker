//! The queues between the edit thread and the audio callback.
//!
//! `rtrb` can't take back a queued item, so the newest unsent version waits in a slot on the edit
//! thread. Every edit and every frame tries to send it, so the last edit always reaches audio.

use std::sync::Arc;

use rtrb::{Consumer, Producer, RingBuffer};
use vt_model::constants::{COMMAND_QUEUE, PUBLISH_QUEUE, RETURN_QUEUE, TELEMETRY_QUEUE};

use crate::engine::{Engine, Transport};
use crate::mixer::Reading;
use crate::published::Published;

/// A transport or preview message. The engine resolves it against its own state, so two quick
/// presses can't act on an out-of-date view of whether it is playing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// `Space c` and `Shift-Tab`: play from a section step, whatever is happening now.
    PlayFrom { step: usize },
    /// `Tab`: pause in place, or play from there.
    PauseOrResume,
    /// Opening a project: stop, and go back to the start.
    Return,
    /// Play an entered note on the preview voice.
    Preview {
        track: usize,
        note: u8,
        velocity: u8,
    },
    /// Hear a row of the section while not playing.
    Audition { section_step: usize },
    /// Mute or unmute a track.
    SetMute { track: usize, mute: bool },
    /// Solo or unsolo a track.
    SetSolo { track: usize, solo: bool },
}

/// What the callback reports back each block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Telemetry {
    pub position: usize,
    pub transport: Transport,
    pub reading: Reading,
}

/// The edit thread's end.
pub struct Handle {
    versions: Producer<Arc<Published>>,
    returned: Consumer<Arc<Published>>,
    commands: Producer<Command>,
    telemetry: Consumer<Telemetry>,
    pending: Option<Arc<Published>>,
}

/// The callback's end. Nothing here allocates, frees or locks.
pub struct Realtime {
    engine: Engine,
    versions: Consumer<Arc<Published>>,
    returned: Producer<Arc<Published>>,
    commands: Consumer<Command>,
    telemetry: Producer<Telemetry>,
}

/// Connect an engine to the edit thread.
pub fn connect(engine: Engine) -> (Handle, Realtime) {
    let (version_tx, version_rx) = RingBuffer::new(PUBLISH_QUEUE);
    let (return_tx, return_rx) = RingBuffer::new(RETURN_QUEUE);
    let (command_tx, command_rx) = RingBuffer::new(COMMAND_QUEUE);
    let (telemetry_tx, telemetry_rx) = RingBuffer::new(TELEMETRY_QUEUE);
    (
        Handle {
            versions: version_tx,
            returned: return_rx,
            commands: command_tx,
            telemetry: telemetry_rx,
            pending: None,
        },
        Realtime {
            engine,
            versions: version_rx,
            returned: return_tx,
            commands: command_rx,
            telemetry: telemetry_tx,
        },
    )
}

impl Handle {
    /// Send a new version. If an unsent one is waiting, this replaces it, and it is freed here.
    pub fn publish(&mut self, version: Arc<Published>) {
        self.pending = Some(version);
        self.pump();
    }

    /// Free returned versions and try to send the waiting one. Call this every frame.
    pub fn pump(&mut self) {
        while self.returned.pop().is_ok() {}
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

    /// The newest report from the callback, if there is one. Its reading covers every report
    /// since the last call, so a meter doesn't miss a hit that fell between two frames.
    pub fn latest(&mut self) -> Option<Telemetry> {
        let mut latest: Option<Telemetry> = None;
        while let Ok(mut report) = self.telemetry.pop() {
            if let Some(earlier) = latest {
                report.reading.merge(earlier.reading);
            }
            latest = Some(report);
        }
        latest
    }

    /// Whether a version is still waiting to be sent.
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }
}

impl Realtime {
    /// Fill one block of interleaved stereo output.
    pub fn process(&mut self, out: &mut [f32]) {
        // Take a new version only when the old one can be sent back to be freed.
        if !self.returned.is_full()
            && let Ok(version) = self.versions.pop()
        {
            let old = self.engine.swap(version);
            let _ = self.returned.push(old);
        }
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::PlayFrom { step } => self.engine.play_from(step),
                Command::PauseOrResume => self.engine.pause_or_resume(),
                Command::Return => self.engine.return_to_start(),
                Command::Preview {
                    track,
                    note,
                    velocity,
                } => self.engine.preview(track, note, velocity),
                Command::Audition { section_step } => self.engine.audition(section_step),
                Command::SetMute { track, mute } => self.engine.set_mute(track, mute),
                Command::SetSolo { track, solo } => self.engine.set_solo(track, solo),
            }
        }
        self.engine.process(out);
        let _ = self.telemetry.push(Telemetry {
            position: self.engine.position(),
            transport: self.engine.transport(),
            reading: self.engine.take_reading(),
        });
    }

    /// The engine, for tests.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use vt_model::constants::MAX_TRACKS;
    use vt_model::sample::SampleData;
    use vt_model::samples::SamplePool;
    use vt_model::song::Song;
    use vt_model::step::{Note, Step};

    use super::*;
    use crate::rt_guard::forbidden;

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
        for track in &mut song.tracks {
            track.sample = Some("flat".to_owned());
        }
        for id in 0..tracks as u32 {
            let phrase = Arc::make_mut(song.phrases.get_mut(&id).unwrap());
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
    fn locks_holds_and_glides_never_allocate_in_the_callback() {
        let pool = samples();
        let mut marked = song(8, 16);
        for id in 0..8u32 {
            let phrase = Arc::make_mut(marked.phrases.get_mut(&id).unwrap());
            for step in 0..16 {
                let text = match step % 4 {
                    0 => "C-4 -- CUT40g VOL80 PANC0h",
                    1 => "--- -- RES20g ---00 ---00",
                    2 => "C-4 -- CUTF0h VOLA0g ---00",
                    _ => "--- -- ---00 RES40 ---00",
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
    fn changing_track_count_and_removing_a_phrase_dont_allocate() {
        let pool = samples();
        let engine = Engine::new(48_000, published(song(8, 16), &pool));
        let (mut handle, mut rt) = connect(engine);
        handle.send(Command::PlayFrom { step: 0 });
        let mut violations = blocks(&mut rt, 4);
        for tracks in [MAX_TRACKS, 1, 8] {
            handle.publish(published(song(tracks, 16), &pool));
            violations += blocks(&mut rt, 4);
            handle.pump();
        }
        let mut removed = song(8, 16);
        removed.phrases.remove(&0);
        handle.publish(published(removed, &pool));
        violations += blocks(&mut rt, 4);
        assert_eq!(violations, 0);
    }

    #[test]
    fn every_fx_command_stays_off_the_allocator() {
        let pool = samples();
        let mut fx = Song::new(2);
        for track in &mut fx.tracks {
            track.sample = Some("flat".to_owned());
        }
        let texts = [
            "C-4 01 VOL80 PAN20 CUT40",
            "C-4 -- RESC0 DLY80 REV80",
            "C-4 -- DEL02 KIL03 OFF01",
            "C-4 -- RET52 CHA80 ---00",
            "OFF -- ---00 ---00 ---00",
        ];
        for id in 0..2 {
            let phrase = Arc::make_mut(fx.phrases.get_mut(&id).unwrap());
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
    fn audition_stays_off_the_allocator() {
        let pool = samples();
        let mut played = Song::new(2);
        for track in &mut played.tracks {
            track.sample = Some("flat".to_owned());
        }
        for id in 0..2 {
            let phrase = Arc::make_mut(played.phrases.get_mut(&id).unwrap());
            *phrase.step_mut(0).unwrap() = "C-4 01 VOL80 ---00 ---00".parse().unwrap();
        }
        let engine = Engine::new(48_000, published(played, &pool));
        let (mut handle, mut rt) = connect(engine);
        for step in 0..50 {
            handle.send(Command::Audition { section_step: step });
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
        *Arc::make_mut(one.phrases.get_mut(&0).unwrap())
            .step_mut(0)
            .unwrap() = "C-4 -- ---00 ---00 ---00".parse().unwrap();
        let engine = Engine::new(48_000, published(one, &pool));
        let (mut handle, mut rt) = connect(engine);
        assert_eq!(blocks(&mut rt, 1), 0);
        handle.send(Command::Audition { section_step: 0 });
        assert_eq!(blocks(&mut rt, 2), 0);
        handle.send(Command::Audition { section_step: 1 });
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
