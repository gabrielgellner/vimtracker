//! The voices the keyboard plays through, apart from the tracks' own, so a chord sounds and a
//! preview never cuts a sequenced note or is cut by one.
//!
//! Each voice is tied to the note whose key started it, so letting go of one key ends only its
//! note. They're all made up front, and starting a note only overwrites one.

use std::sync::Arc;

use vt_model::constants::PREVIEW_VOICES;
use vt_model::sample::SampleData;
use vt_model::samples::{SampleId, SamplePool};
use vt_model::song::SynthVoice;

use crate::dsp::virtual_analog::Settings;
use crate::voice::{NoteOn, Voice};

/// The ids a one-off file plays under, which no sample in the pool has. There are two places, so a
/// file fading out keeps its own sound while the next one starts.
pub const FILE_SAMPLES: [SampleId; 2] = [SampleId::MAX, SampleId::MAX - 1];

/// The one-off files the preview voices can play, in the places [`FILE_SAMPLES`] names.
pub type Files = [Option<Arc<SampleData>>; 2];

/// A sample's sound, from the pool or from the one-off files.
#[inline]
fn data<'a>(samples: &'a SamplePool, files: &'a Files, id: SampleId) -> Option<&'a SampleData> {
    match FILE_SAMPLES.iter().position(|file| *file == id) {
        Some(place) => files[place].as_deref(),
        None => samples.get(id),
    }
}

/// The voices, and one more for fading out a voice that's taken over, so it doesn't click.
const PLACES: usize = PREVIEW_VOICES + 1;
const FADING: usize = PREVIEW_VOICES;

/// One voice of the bank.
#[derive(Debug, Clone, Copy)]
struct Preview {
    voice: Voice,
    /// The track it goes through.
    track: usize,
    /// The note whose key is holding it, until that key is let go.
    playing: Option<NoteOn>,
    /// When it started, counted in notes started, for taking over the oldest.
    started: u64,
    /// For a voice that plays one note at a time, the keys still down under the one it's
    /// playing, oldest first, to go back to when the newest is let go.
    under: [Option<NoteOn>; PREVIEW_VOICES],
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            voice: Voice::default(),
            track: 0,
            playing: None,
            started: 0,
            under: [None; PREVIEW_VOICES],
        }
    }
}

impl Preview {
    /// Whether a key is still holding it.
    fn held(&self) -> bool {
        self.playing.is_some() && self.voice.is_held()
    }

    /// The note whose key is holding it.
    fn note(&self) -> Option<u8> {
        self.playing.map(|on| on.note)
    }

    /// Remember a key still down under the one it's going to play, dropping the oldest if
    /// there's no room.
    fn push_under(&mut self, on: NoteOn) {
        if self.under[PREVIEW_VOICES - 1].is_some() {
            self.under.rotate_left(1);
            self.under[PREVIEW_VOICES - 1] = None;
        }
        if let Some(free) = self.under.iter_mut().find(|place| place.is_none()) {
            *free = Some(on);
        }
    }

    /// The newest key still down under the one it's playing, taken off the list.
    fn pop_under(&mut self) -> Option<NoteOn> {
        let last = self.under.iter().rposition(Option::is_some)?;
        self.under[last].take()
    }

    /// Forget a key under the one it's playing, which was let go.
    fn remove_under(&mut self, note: u8) {
        if let Some(at) = self
            .under
            .iter()
            .position(|place| place.is_some_and(|under| under.note == note))
        {
            self.under[at] = None;
            self.under[at..].rotate_left(1);
        }
    }
}

/// How a key's note is let go.
#[derive(Debug, Clone, Copy)]
pub enum LetGo {
    /// Now.
    Now,
    /// Once it has sounded this many frames, or now if it has.
    OnceSounded(u32),
}

/// The bank of preview voices.
pub struct Previews {
    voices: Box<[Preview; PLACES]>,
    /// Notes started so far, for telling the oldest voice.
    started: u64,
}

impl Default for Previews {
    fn default() -> Self {
        Self {
            voices: Box::new([Preview::default(); PLACES]),
            started: 0,
        }
    }
}

/// The places of the voices sounding at the start of a run of frames. A note only starts
/// between runs, so no others can start in one.
pub struct Sounding {
    places: [usize; PLACES],
    count: usize,
}

impl Sounding {
    fn places(&self) -> &[usize] {
        &self.places[..self.count]
    }
}

impl Previews {
    /// Have each voice and each note held for one follow a change, as a voice playing a bank entry
    /// follows the entry's.
    pub fn follow(&mut self, voice: impl Fn(&mut Voice), note: impl Fn(&mut NoteOn)) {
        for preview in self.voices.iter_mut() {
            voice(&mut preview.voice);
            preview.playing.iter_mut().for_each(&note);
            preview.under.iter_mut().flatten().for_each(&note);
        }
    }

    /// Start a note through a track, held until its key is let go if `on` has no timed release.
    ///
    /// `Acid` plays one note at a time, as the 303 does: a key pressed while another is down
    /// slides there, and letting go of the newest slides back to the one under it. A sample and
    /// every other synth take a voice for each key. A note already held on the track starts again
    /// on its own voice, and with every voice in use, the one let go longest ago is taken over,
    /// or failing that the oldest.
    pub fn start(&mut self, track: usize, on: NoteOn, settings: Settings, rate: u32) {
        self.started += 1;
        let started = self.started;
        if on.synth == Some(SynthVoice::Acid)
            && let Some(preview) = self.voices[..PREVIEW_VOICES]
                .iter_mut()
                .find(|preview| preview.track == track && preview.voice.plays_acid())
        {
            // Only a held key slides, as a timed preview has no key to keep down.
            if preview.held() && on.release_after.is_none() {
                if let Some(playing) = preview.playing
                    && playing.release_after.is_none()
                {
                    preview.push_under(playing);
                }
                preview.voice.tie(on, None);
            } else {
                preview.under = [None; PREVIEW_VOICES];
                preview.voice.retrigger(on);
            }
            preview.playing = Some(on);
            preview.started = started;
            preview.voice.set_synth(settings);
            return;
        }
        let place = self.place_for(track, on.note);
        if self.voices[place].voice.active() {
            self.voices[FADING] = self.voices[place];
            self.voices[FADING].playing = None;
            self.voices[FADING].voice.fade();
        }
        let mut voice = Voice::start(on, rate, 0.0, 1.0);
        voice.set_synth(settings);
        self.voices[place] = Preview {
            voice,
            track,
            playing: Some(on),
            started,
            under: [None; PREVIEW_VOICES],
        };
    }

    /// Where a new note goes: the voice already holding it on this track, a free one, the one let
    /// go longest ago, or the oldest.
    fn place_for(&self, track: usize, note: u8) -> usize {
        let voices = &self.voices[..PREVIEW_VOICES];
        let oldest = |held: bool| {
            voices
                .iter()
                .enumerate()
                .filter(|(_, preview)| preview.held() == held)
                .min_by_key(|(_, preview)| preview.started)
                .map(|(place, _)| place)
        };
        voices
            .iter()
            .position(|preview| preview.track == track && preview.note() == Some(note))
            .or_else(|| voices.iter().position(|preview| !preview.voice.active()))
            .or_else(|| oldest(false))
            .or_else(|| oldest(true))
            .unwrap_or(0)
    }

    /// The key holding a note through a track was let go. A voice playing one note at a time
    /// that has another key down under it slides back there instead.
    pub fn let_go(&mut self, track: usize, note: u8, how: LetGo) {
        for preview in self.voices[..PREVIEW_VOICES]
            .iter_mut()
            .filter(|preview| preview.track == track)
        {
            if preview.note() != Some(note) {
                preview.remove_under(note);
                continue;
            }
            if let Some(under) = preview.pop_under() {
                preview.voice.tie(under, None);
                preview.playing = Some(under);
                continue;
            }
            match how {
                LetGo::Now => preview.voice.release(),
                LetGo::OnceSounded(frames) => preview.voice.release_once_sounded(frames),
            }
            preview.playing = None;
        }
    }

    /// Fade out every voice playing this sample, so it can be replaced.
    pub fn fade_sample(&mut self, id: SampleId) {
        for preview in self.voices.iter_mut() {
            if preview.voice.active() && preview.voice.sample() == id {
                preview.voice.fade();
                preview.playing = None;
            }
        }
    }

    /// Let go of every voice playing this sample.
    pub fn release_sample(&mut self, id: SampleId) {
        for preview in self.voices.iter_mut() {
            if preview.voice.active() && preview.voice.sample() == id {
                preview.voice.release();
                preview.playing = None;
            }
        }
    }

    /// How many voices are sounding, a fading one included.
    pub fn count(&self) -> usize {
        self.voices
            .iter()
            .filter(|preview| preview.voice.active())
            .count()
    }

    /// The voices sounding now, for a run of frames.
    pub fn sounding(&self) -> Sounding {
        let mut sounding = Sounding {
            places: [0; PLACES],
            count: 0,
        };
        for (place, preview) in self.voices.iter().enumerate() {
            if preview.voice.active() {
                sounding.places[sounding.count] = place;
                sounding.count += 1;
            }
        }
        sounding
    }

    /// The voices sounding through a track, for where each is in its sample.
    pub fn on(&self, track: usize) -> impl Iterator<Item = &Voice> {
        self.voices
            .iter()
            .filter(move |preview| preview.track == track && preview.voice.active())
            .map(|preview| &preview.voice)
    }

    /// Whether any of these voices is sounding through a track.
    pub fn on_track(&self, sounding: &Sounding, track: usize) -> bool {
        sounding.places().iter().any(|&place| {
            let preview = &self.voices[place];
            preview.track == track && preview.voice.active()
        })
    }

    /// The next frame of these voices going through a track, or nothing if none of them is
    /// sounding through it.
    #[inline]
    pub fn next(
        &mut self,
        sounding: &Sounding,
        track: usize,
        samples: &SamplePool,
        files: &Files,
    ) -> Option<[f32; 2]> {
        let mut frame: Option<[f32; 2]> = None;
        for &place in sounding.places() {
            let preview = &mut self.voices[place];
            if preview.track != track || !preview.voice.active() {
                continue;
            }
            let next = preview
                .voice
                .next(data(samples, files, preview.voice.sample()));
            let sum = frame.get_or_insert([0.0; 2]);
            sum[0] += next[0];
            sum[1] += next[1];
        }
        frame
    }
}
