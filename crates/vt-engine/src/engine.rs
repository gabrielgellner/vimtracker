//! The block loop. It sequences inside the audio callback, so screen redraws don't affect timing.

use std::sync::Arc;

use vt_model::constants::{MAX_TRACKS, PREVIEW_MS, TICKS_PER_QUARTER, TICKS_PER_STEP};
use vt_model::step::{MAX_NOTE, MIN_NOTE, Note};

use crate::mixer::master;
use crate::published::Published;
use crate::track_state::TrackState;
use crate::voice::{NoteOn, Voice};

/// Whether the sequencer is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Stopped,
    Playing,
    Paused,
}

/// The sample an event falls on, counted from the tick where the current tempo began.
///
/// Always calculated from that origin and never accumulated, so rounding can't build up.
pub fn sample_of_tick(tick_from_origin: u64, rate: u32, millibpm: u32) -> u64 {
    let numerator = u128::from(tick_from_origin) * 60 * u128::from(rate) * 1000;
    let denominator = u128::from(TICKS_PER_QUARTER) * u128::from(millibpm);
    (numerator / denominator) as u64
}

/// Sequencer and voices. It owns all its state ahead of time, and `process` doesn't allocate.
pub struct Engine {
    rate: u32,
    current: Arc<Published>,
    tracks: Box<[TrackState; MAX_TRACKS]>,
    preview: Voice,
    transport: Transport,
    /// The next step to fire, counted from the section start without wrapping.
    next_step: u64,
    /// The tick the tempo began at.
    origin_tick: u64,
    /// Frames played since `origin_tick`.
    frames_since_origin: u64,
    millibpm: u32,
}

impl Engine {
    /// A stopped engine at the top of the section.
    pub fn new(rate: u32, current: Arc<Published>) -> Self {
        let millibpm = current.song.millibpm;
        Self {
            rate,
            current,
            tracks: Box::new([TrackState::default(); MAX_TRACKS]),
            preview: Voice::default(),
            transport: Transport::Stopped,
            next_step: 0,
            origin_tick: 0,
            frames_since_origin: 0,
            millibpm,
        }
    }

    /// The output sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The transport state.
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// The current version.
    pub fn current(&self) -> &Arc<Published> {
        &self.current
    }

    /// The last step fired, within the section, or the next to fire if none has yet.
    pub fn position(&self) -> usize {
        let length = self.section_steps();
        if length == 0 {
            return 0;
        }
        let fired = self.transport == Transport::Playing && self.frames_since_origin > 0;
        (self.next_step.saturating_sub(u64::from(fired)) % length as u64) as usize
    }

    fn section_steps(&self) -> usize {
        let song = &self.current.song;
        song.sections.first().map_or(0, |s| song.section_steps(s))
    }

    /// Switch to a new version, returning the old one so it can be freed elsewhere.
    ///
    /// A shorter phrase wraps by the modulo in sequencing. An ID that no longer exists is silent,
    /// and a removed track's voices fade.
    pub fn swap(&mut self, version: Arc<Published>) -> Arc<Published> {
        let old = std::mem::replace(&mut self.current, version);
        for track in self.tracks.iter_mut().skip(self.current.song.tracks.len()) {
            track.stop();
        }
        self.retune();
        old
    }

    /// Move the tempo origin here if the tempo changed, so the next step's time stays right.
    fn retune(&mut self) {
        let millibpm = self.current.song.millibpm;
        if millibpm == self.millibpm || millibpm == 0 {
            return;
        }
        let numerator = u128::from(self.frames_since_origin)
            * u128::from(TICKS_PER_QUARTER)
            * u128::from(self.millibpm);
        let denominator = 60 * u128::from(self.rate) * 1000;
        self.origin_tick += (numerator / denominator) as u64;
        self.frames_since_origin = 0;
        self.millibpm = millibpm;
    }

    /// Play from a step. Enter stops if already playing.
    pub fn play_or_stop(&mut self, from_step: usize) {
        if self.transport == Transport::Playing {
            self.halt(Transport::Stopped);
        } else {
            self.play_from(from_step);
        }
    }

    /// Start at a step within the section.
    pub fn play_from(&mut self, step: usize) {
        self.next_step = step as u64;
        self.origin_tick = step as u64 * TICKS_PER_STEP;
        self.frames_since_origin = 0;
        self.millibpm = self.current.song.millibpm;
        self.transport = Transport::Playing;
    }

    /// Pause in place, or resume from the stored position.
    pub fn pause_or_resume(&mut self) {
        if self.transport == Transport::Playing {
            self.halt(Transport::Paused);
        } else {
            self.transport = Transport::Playing;
        }
    }

    /// Go back to the start. If playing, stop there.
    pub fn return_to_start(&mut self) {
        self.halt(Transport::Stopped);
        self.next_step = 0;
        self.origin_tick = 0;
        self.frames_since_origin = 0;
    }

    /// Stop moving. Voices fade so stopping doesn't click.
    fn halt(&mut self, to: Transport) {
        for track in self.tracks.iter_mut() {
            track.stop();
        }
        self.transport = to;
    }

    /// Set whether a track is muted. A muted track's sounding note fades.
    pub fn set_mute(&mut self, track: usize, mute: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.mute = mute;
        }
        self.apply_audibility();
    }

    /// Set whether a track is soloed. While any track is soloed, only soloed tracks sound.
    pub fn set_solo(&mut self, track: usize, solo: bool) {
        if let Some(state) = self.tracks.get_mut(track) {
            state.solo = solo;
        }
        self.apply_audibility();
    }

    fn any_solo(&self) -> bool {
        self.tracks.iter().any(|t| t.solo)
    }

    fn apply_audibility(&mut self) {
        let any_solo = self.any_solo();
        for track in self.tracks.iter_mut() {
            if track.mute || (any_solo && !track.solo) {
                track.stop();
            }
        }
    }

    /// Play a note on the preview voice. It never cuts a sequenced note, or is cut by one.
    pub fn preview(&mut self, instrument: u32, note: u8) {
        let Some(on) = self.note_on(instrument, note, 1.0) else {
            return;
        };
        let frames = PREVIEW_MS * self.rate / 1000;
        self.preview = Voice::start(
            NoteOn {
                release_after: Some(frames),
                ..on
            },
            self.rate,
        );
    }

    fn note_on(&self, instrument: u32, note: u8, step_gain: f32) -> Option<NoteOn> {
        let sample = *self.current.instrument_samples.get(&instrument)?;
        let settings = self.current.song.instruments.get(&instrument)?;
        Some(NoteOn {
            sample,
            note,
            gain: step_gain * f32::from(settings.volume) / 255.0,
            attack_ms: settings.attack_ms,
            release_ms: settings.release_ms,
            release_after: None,
        })
    }

    /// Fill interleaved stereo output. This doesn't allocate, lock or print.
    pub fn process(&mut self, out: &mut [f32]) {
        let mut written = 0;
        let total = out.len() / 2;
        while written < total {
            let until_event = self.frames_until_next_step();
            if until_event == 0 {
                self.fire_step();
                self.next_step += 1;
                continue;
            }
            let run = until_event.min((total - written) as u64) as usize;
            self.render(&mut out[written * 2..(written + run) * 2]);
            if self.transport == Transport::Playing {
                self.frames_since_origin += run as u64;
            }
            written += run;
        }
    }

    /// Frames until the next step fires. Not playing means no event in this block.
    fn frames_until_next_step(&self) -> u64 {
        if self.transport != Transport::Playing || self.section_steps() == 0 {
            return u64::MAX;
        }
        let tick = (self.next_step * TICKS_PER_STEP).saturating_sub(self.origin_tick);
        sample_of_tick(tick, self.rate, self.millibpm).saturating_sub(self.frames_since_origin)
    }

    /// Start or stop each track's note for the next step.
    fn fire_step(&mut self) {
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some(section) = song.sections.first() else {
            return;
        };
        let length = song.section_steps(section);
        if length == 0 {
            return;
        }
        let section_step = (self.next_step % length as u64) as usize;
        let any_solo = self.any_solo();
        for index in 0..song.tracks.len().min(MAX_TRACKS) {
            let Some(Some(chain)) = section.chains.get(index) else {
                continue;
            };
            let Some(position) = song.chain_position(*chain, section_step) else {
                continue;
            };
            let Some(step) = song.step_at(position) else {
                continue;
            };
            if let Some(instrument) = step.instrument {
                self.tracks[index].instrument = Some(instrument);
            }
            match step.note {
                Note::Empty => {}
                Note::Off => self.tracks[index].voice.release(),
                Note::On(midi) => {
                    let note = (i16::from(midi) + i16::from(position.transpose))
                        .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE))
                        as u8;
                    let gain = step.volume.map_or(1.0, |v| f32::from(v) / 255.0);
                    let on = self.tracks[index]
                        .instrument
                        .and_then(|i| self.note_on(i, note, gain));
                    let state = &self.tracks[index];
                    let audible = !state.mute && (!any_solo || state.solo);
                    match on {
                        Some(on) if audible => self.tracks[index].start(on, self.rate),
                        _ => self.tracks[index].cut_to_tail(),
                    }
                }
            }
        }
    }

    /// Render voices into a run of frames with no events in it.
    fn render(&mut self, out: &mut [f32]) {
        let samples = Arc::clone(&self.current.samples);
        out.fill(0.0);
        let tracks = self.tracks.iter_mut();
        for track in tracks {
            for voice in [&mut track.voice, &mut track.tail] {
                if voice.active() {
                    let data = samples.get(voice.sample());
                    for frame in out.as_chunks_mut::<2>().0 {
                        let value = voice.next(data);
                        frame[0] += value[0];
                        frame[1] += value[1];
                    }
                }
            }
        }
        if self.preview.active() {
            let data = samples.get(self.preview.sample());
            for frame in out.as_chunks_mut::<2>().0 {
                let value = self.preview.next(data);
                frame[0] += value[0];
                frame[1] += value[1];
            }
        }
        for frame in out.as_chunks_mut::<2>().0 {
            *frame = master(*frame);
        }
    }
}
