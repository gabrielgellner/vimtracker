//! Notes from outside the steps: the keyboard's previews, one-off files, the sample page's
//! playheads and auditions, and the note a step or a key starts.

use std::sync::Arc;

use vt_model::constants::{DEFAULT_NOTE_VELOCITY, MAX_TRACKS, PREVIEW_MS, TICKS_PER_STEP};
use vt_model::envelope::{Adsr, Envelope};
use vt_model::sample::SampleData;
use vt_model::settings::{PlayMode, SettingId};
use vt_model::song::{Machine, SynthVoice, Track};
use vt_model::step::{ACCENT_VELOCITY, MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note};

use crate::preview::{FILE_SAMPLES, LetGo};
use crate::published::Span;
use crate::voice::{Cut, NoteOn};

use super::steps::Read;
use super::{Engine, PLAYHEADS, Playhead, Transport};

impl Engine {
    /// Play a note on a preview voice for a short while. It never cuts a sequenced note, or is
    /// cut by one.
    pub fn preview(&mut self, track: usize, note: u8, velocity: u8) {
        let frames = PREVIEW_MS * self.rate / 1000;
        self.start_preview(track, note, velocity, Some(frames));
    }

    /// Play a note on a preview voice until [`Engine::release_preview`] lets it go, for a key
    /// held down. Each key down has a voice of its own, so they play a chord.
    pub fn hold_preview(&mut self, track: usize, note: u8, velocity: u8) {
        self.start_preview(track, note, velocity, None);
    }

    /// Let go of a note held through a track, leaving any others sounding.
    pub fn release_preview(&mut self, track: usize, note: u8) {
        self.previews.let_go(track, note, LetGo::Now);
    }

    /// Let go of a note held through a track once it has sounded as long as a preview does: an
    /// audition's key tapped rather than held.
    pub fn finish_preview(&mut self, track: usize, note: u8) {
        let frames = PREVIEW_MS * self.rate / 1000;
        self.previews
            .let_go(track, note, LetGo::OnceSounded(frames));
    }

    /// Play a one-off file through a track on a preview voice, to its end or until another is
    /// played or it's stopped, as a sample browser plays the file its cursor moves onto. With a
    /// span, only that much of it plays, the way the span goes, with its gain, pan, tune and fades,
    /// as the sample page plays a slice or from its cursor. It's handed over whole, so nothing is
    /// loaded or freed here. What's given back is for the edit thread to free: the file it
    /// replaces, or this one if the track can't play it.
    pub fn play_file(
        &mut self,
        track: usize,
        file: Arc<SampleData>,
        span: Option<Span>,
    ) -> Option<Arc<SampleData>> {
        self.play_one_off(
            track,
            file,
            span,
            velocity_gain(DEFAULT_NOTE_VELOCITY),
            Envelope::note_shape(),
        )
    }

    /// Play audio through a track as it is, at full level from its first frame to its last with no
    /// fade in or out, so what comes out is what the track's filter and effects make of it, for
    /// `vimtracker probe`. Otherwise it goes as [`Engine::play_file`] does.
    pub fn play_input(&mut self, track: usize, input: Arc<SampleData>) -> Option<Arc<SampleData>> {
        let unshaped = Adsr {
            attack_ms: 0,
            release_ms: 0,
            ..Adsr::note_shape()
        };
        self.play_one_off(track, input, None, 1.0, Envelope::Adsr(unshaped))
    }

    fn play_one_off(
        &mut self,
        track: usize,
        file: Arc<SampleData>,
        span: Option<Span>,
        gain: f32,
        shape: Envelope,
    ) -> Option<Arc<SampleData>> {
        let plays = self
            .current
            .song
            .tracks
            .get(track)
            .is_some_and(Track::is_sound);
        if !plays {
            return Some(file);
        }
        for id in FILE_SAMPLES {
            self.previews.fade_sample(id);
        }
        // A span is of this file, whatever length it was worked out for, so it turns round at
        // the file's own end when it's reversed.
        let frames = file.frames.len() as u64;
        let span = span.map(|span| Span { frames, ..span });
        // The other place, as the last file may still be fading out of this one.
        let place = 1 - self.file_place;
        self.file_place = place;
        let old = self.files[place].replace(file);
        let on = NoteOn {
            sample: FILE_SAMPLES[place],
            synth: None,
            note: 60,
            velocity: MAX_VELOCITY,
            gain,
            entry: None,
            slice: 0,
            span,
            cut: Cut::default(),
            accent: false,
            shape,
            release_after: None,
        };
        let sounding = self.previews.sounding();
        let previewing = self.previews.on_track(&sounding, track);
        let state = &mut self.tracks[track];
        if !state.sounding() && !previewing {
            state.settle();
        }
        self.previews
            .start(track, on, state.synth_settings(), self.rate);
        old
    }

    /// Let go of any one-off file playing.
    pub fn stop_file(&mut self) {
        for id in FILE_SAMPLES {
            self.previews.release_sample(id);
        }
    }

    /// How many preview voices are sounding.
    pub fn previews_sounding(&self) -> usize {
        self.previews.count()
    }

    /// Where each voice playing a sample through a track is in it, for the sample page's
    /// playheads: the track's own note and its tail, then the preview voices, as many as there's
    /// room for. A synth's note has no place in a sample, and a one-off file has no entry.
    pub fn playheads(&self, track: usize) -> [Option<Playhead>; PLAYHEADS] {
        let mut found = [None; PLAYHEADS];
        let Some(state) = self.tracks.get(track) else {
            return found;
        };
        let voices = [&state.voice, &state.tail]
            .into_iter()
            .chain(self.previews.on(track));
        let heads = voices.filter_map(|voice| {
            let frame = voice.frame()?;
            let file = FILE_SAMPLES.contains(&voice.sample());
            let entry = if file { None } else { Some(voice.entry()?) };
            Some(Playhead { entry, frame })
        });
        for (slot, head) in found.iter_mut().zip(heads) {
            *slot = Some(head);
        }
        found
    }

    fn start_preview(&mut self, track: usize, note: u8, velocity: u8, release_after: Option<u32>) {
        let shape = self.note_shape(track);
        let Some(on) = self.note_on(track, note, velocity, shape) else {
            return;
        };
        let sounding = self.previews.sounding();
        let previewing = self.previews.on_track(&sounding, track);
        let Some(state) = self.tracks.get_mut(track) else {
            return;
        };
        if !state.sounding() && !previewing {
            state.settle();
        }
        let on = NoteOn {
            release_after,
            ..on
        };
        self.previews
            .start(track, on, state.synth_settings(), self.rate);
    }

    /// A note at a velocity, which sets its level and whether it's accented. `Acid` has no velocity,
    /// as a 303 has none: its velocity only says whether it's accented, and every plain note it
    /// plays sounds the same, as does every accent (decision 121). A sampler plays the bank entry
    /// its `SMP` is on as the note starts, the slice of it its `SLI` is on, as its `STA`, `SLEN`
    /// and `PLY` cut it then, and has nothing to play with no entry.
    pub(super) fn note_on(
        &self,
        track: usize,
        note: u8,
        velocity: u8,
        shape: Envelope,
    ) -> Option<NoteOn> {
        let owner = self.current.song.tracks.get(track)?;
        if !owner.is_sound() {
            return None;
        }
        let (sample, entry, slice, span, cut, synth) = match owner.machine {
            Machine::Sampler => {
                let state = self.tracks.get(track)?;
                let number = state.sample_number();
                let playable = self.current.bank[usize::from(number)].as_deref()?;
                let cut = state.sample_cut();
                let slice = state.slice_number(playable, owner.setting(SettingId::SLICE));
                (
                    playable.sample,
                    Some(number),
                    slice,
                    Some(playable.span(slice, cut.mode == PlayMode::Thru)),
                    cut,
                    None,
                )
            }
            Machine::Plaits(voice) => (0, None, 0, None, Cut::default(), Some(voice)),
        };
        let accent = velocity >= ACCENT_VELOCITY;
        let heard = match synth {
            Some(SynthVoice::Acid) => ACID_VELOCITY,
            _ => velocity,
        };
        Some(NoteOn {
            sample,
            synth,
            note,
            velocity,
            gain: velocity_gain(heard),
            entry,
            slice,
            span,
            cut,
            accent,
            shape,
            release_after: None,
        })
    }

    /// Hear a row while not playing: every audible track's note at this step of the section, with
    /// its velocity and transpose. Whatever an audition was sounding is cut first. Its
    /// FX don't run, and nothing that playback relies on changes.
    pub fn audition(&mut self, section_step: usize) {
        self.audition_section(self.section(), section_step);
    }

    /// Hear a row of a given section while not playing.
    pub fn audition_section(&mut self, section: usize, section_step: usize) {
        self.audition_from(section, section_step, None);
    }

    /// Hear a row of a given section while not playing: a step of the song's, or one of a
    /// track's own, and on every other track the row that's playing then at its own speed.
    pub fn audition_from(&mut self, section: usize, step: usize, track: Option<usize>) {
        if self.transport == Transport::Playing {
            return;
        }
        let tick = step as u64 * track.map_or(TICKS_PER_STEP, |track| self.speed_ticks(track));
        let current = Arc::clone(&self.current);
        let song = &current.song;
        let Some(section) = song.sections.get(section) else {
            return;
        };
        self.auditioned = [false; MAX_TRACKS];
        for index in 0..song.tracks.len().min(self.tracks.len()) {
            self.tracks[index].cut_to_tail();
            if !self.plays(index) {
                continue;
            }
            let Some(Some(chain)) = section.chains.get(index) else {
                continue;
            };
            let owner = &song.tracks[index];
            // A track with a reset of its own has started its chain again since the section's.
            let tick = owner.reset.map_or(tick, |steps| {
                tick % (u64::from(steps.max(1)) * TICKS_PER_STEP)
            });
            let place = (tick / owner.speed.ticks()) as usize;
            let Some(position) = owner.chain_position(*chain, place, self.dealer(index)) else {
                continue;
            };
            let Some(step) = owner.step_at(position) else {
                continue;
            };
            let row = owner
                .phrases
                .get(&position.phrase)
                .map_or(0, |phrase| phrase.start())
                + position.step;
            let read = Read {
                chain: *chain,
                count: place as i64,
                position,
            };
            let step = self.view_row(index, read, row, step);
            let Note::On(midi) = step.note else {
                continue;
            };
            let note = (i16::from(midi) + i16::from(position.transpose))
                .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
            let velocity = self.tracks[index].velocity(step.velocity.unwrap_or(MAX_VELOCITY));
            let shape = self.note_shape(index);
            let Some(on) = self.note_on(index, note, velocity, shape) else {
                continue;
            };
            let track = &mut self.tracks[index];
            if !track.sounding() {
                track.settle();
            }
            let last_note = track.last_note;
            track.start(on, self.rate);
            track.last_note = last_note;
            track.wake();
            self.auditioned[index] = true;
            self.choke(index);
        }
    }

    /// The audition's key was let go. After a hold, every note it started is let go. After a tap,
    /// a sample plays out, and a synth's note that would otherwise sound until something ends it
    /// is let go once it has sounded as long as a preview does. Once playing has started, the
    /// tracks are playback's and are left alone.
    pub fn end_audition(&mut self, held: bool) {
        let auditioned = std::mem::replace(&mut self.auditioned, [false; MAX_TRACKS]);
        if self.transport == Transport::Playing {
            return;
        }
        let preview = PREVIEW_MS * self.rate / 1000;
        for (track, _) in self
            .tracks
            .iter_mut()
            .zip(auditioned)
            .filter(|(_, auditioned)| *auditioned)
        {
            if held {
                track.voice.release();
            } else if track.voice.holds() {
                track.voice.release_once_sounded(preview);
            }
        }
    }
}

/// The velocity every `Acid` note plays at, whatever its own: 99, where decision 117 fitted its
/// level, so a note there sounds as it did before velocity was taken away from it (decision 121).
/// An accented note plays at it too, as all an accent adds to the level is the voice's own accent
/// circuit's (decision 127).
const ACID_VELOCITY: u8 = 99;

/// The gain of a step's velocity.
fn velocity_gain(velocity: u8) -> f32 {
    f32::from(velocity) / f32::from(MAX_VELOCITY)
}
