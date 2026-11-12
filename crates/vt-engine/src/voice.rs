//! Sample playback and its envelope.

use crate::dsp::acid::Acid;
use crate::dsp::additive::Additive;
use crate::dsp::braids_analog::{CSaw, HardSync, Morph, SawSquare, Triple};
use crate::dsp::braids_digital::{Ring, SawComb, Toy};
use crate::dsp::braids_filter_fm::{ChaoticFm, DigitalFilter, FeedbackFm};
use crate::dsp::braids_vocal::{Fof, Vosim};
use crate::dsp::chiptune::Chiptune;
use crate::dsp::chord::Chord;
use crate::dsp::drums::{Hat, Snare};
use crate::dsp::fm::Fm;
use crate::dsp::grain::Grain;
use crate::dsp::kick::Kick;
use crate::dsp::modal::Modal;
use crate::dsp::noise::Noise;
use crate::dsp::particle::ParticleNoise;
use crate::dsp::phase_distortion::PhaseDistortion;
use crate::dsp::six_op::SixOp;
use crate::dsp::string_model::StringModel;
use crate::dsp::swarm::Swarm;
use crate::dsp::virtual_analog::{Settings, VirtualAnalog};
use crate::dsp::virtual_analog_vcf::VirtualAnalogVcf;
use crate::dsp::wave_terrain::WaveTerrain;
use crate::dsp::waveshaping::Waveshaping;
use crate::dsp::wavetable::Wavetable;
use vt_model::constants::DECLICK_FRAMES;
use vt_model::envelope::Envelope;
use vt_model::playable::Span;
use vt_model::sample::SampleData;
use vt_model::samples::SampleId;
use vt_model::song::SynthVoice;

use crate::dsp::smoother::fast_coefficient;
pub use crate::reader::Cut;
use crate::reader::Reader;

/// How a voice's level is changing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    /// Following its envelope's attack, decay and sustain.
    Held {
        from: f32,
    },
    /// Following its envelope's release, from the level it had.
    Released {
        from: f32,
    },
    /// A short fade, to avoid a click.
    Fading {
        per_frame: f32,
    },
}

/// The frequency of `C-3`, in hertz, for a synth.
const MIDDLE_C: f64 = 261.625_565_3;

/// A glide's time constant, as a share of its time, so it's nearly there when its time is up, as
/// `Acid`'s slide is.
const GLIDE_SHARE: f64 = 1.0 / 3.0;
/// How near its note a glide ends, as a ratio of speeds: well under a hundredth of a cent.
const GLIDE_DONE: f64 = 1e-6;

/// A tie gliding a note's speed to the next note's.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Gliding {
    to: f64,
    /// How much of the rest of the way it goes each frame, in pitch.
    share: f64,
}

/// A synth playing a note. The audio thread can't allocate, so each voice has room for any of them.
#[derive(Debug, Clone, Copy)]
#[allow(clippy::large_enum_variant)]
enum Synth {
    VirtualAnalog(VirtualAnalog),
    VirtualAnalogVcf(VirtualAnalogVcf),
    Acid(Acid),
    Fm(Fm),
    PhaseDistortion(PhaseDistortion),
    Formant(Grain),
    Additive(Additive),
    Swarm(Swarm),
    WaveTerrain(WaveTerrain),
    Chord(Chord),
    Chiptune(Chiptune),
    Kick(Kick),
    Snare(Snare),
    Hat(Hat),
    Waveshaping(Waveshaping),
    Wavetable(Wavetable),
    SixOp(SixOp),
    String(StringModel),
    Modal(Modal),
    Dust(ParticleNoise),
    Noise(Noise),
    CSaw(CSaw),
    Morph(Morph),
    SawSquare(SawSquare),
    HardSync(HardSync),
    Triple(Triple),
    Ring(Ring),
    SawComb(SawComb),
    Toy(Toy),
    ZFilter(DigitalFilter),
    FeedbackFm(FeedbackFm),
    ChaoticFm(ChaoticFm),
    Vosim(Vosim),
    Fof(Fof),
}

impl Synth {
    /// A new note on a voice.
    fn new(voice: SynthVoice, rate: u32) -> Self {
        match voice {
            SynthVoice::VirtualAnalog => Self::VirtualAnalog(VirtualAnalog::new()),
            SynthVoice::VirtualAnalogVcf => Self::VirtualAnalogVcf(VirtualAnalogVcf::new()),
            SynthVoice::Acid => Self::Acid(Acid::new(rate)),
            SynthVoice::PhaseDistortion => Self::PhaseDistortion(PhaseDistortion::new()),
            SynthVoice::Formant => Self::Formant(Grain::new()),
            SynthVoice::Additive => Self::Additive(Additive::new()),
            SynthVoice::Swarm => Self::Swarm(Swarm::new()),
            SynthVoice::WaveTerrain => Self::WaveTerrain(WaveTerrain::new()),
            SynthVoice::Chord => Self::Chord(Chord::new()),
            SynthVoice::Chiptune => Self::Chiptune(Chiptune::new()),
            SynthVoice::Fm => Self::Fm(Fm::new(rate)),
            SynthVoice::Waveshaping => Self::Waveshaping(Waveshaping::new()),
            SynthVoice::Wavetable => Self::Wavetable(Wavetable::new()),
            SynthVoice::SixOp => Self::SixOp(SixOp::new(rate)),
            SynthVoice::String => Self::String(StringModel::new(rate)),
            SynthVoice::Modal => Self::Modal(Modal::new()),
            SynthVoice::Dust => Self::Dust(ParticleNoise::new()),
            SynthVoice::Noise => Self::Noise(Noise::new()),
            SynthVoice::Kick => Self::Kick(Kick::new(rate)),
            SynthVoice::Snare => Self::Snare(Snare::new(rate)),
            SynthVoice::Hat => Self::Hat(Hat::new(rate)),
            SynthVoice::CSaw => Self::CSaw(CSaw::new(rate)),
            SynthVoice::Morph => Self::Morph(Morph::new(rate)),
            SynthVoice::SawSquare => Self::SawSquare(SawSquare::new(rate)),
            SynthVoice::HardSync => Self::HardSync(HardSync::new(rate)),
            SynthVoice::Triple => Self::Triple(Triple::new(rate)),
            SynthVoice::Ring => Self::Ring(Ring::new(rate)),
            SynthVoice::SawComb => Self::SawComb(SawComb::new(rate)),
            SynthVoice::Toy => Self::Toy(Toy::new(rate)),
            SynthVoice::ZFilter => Self::ZFilter(DigitalFilter::new(rate)),
            SynthVoice::FeedbackFm => Self::FeedbackFm(FeedbackFm::new(rate)),
            SynthVoice::ChaoticFm => Self::ChaoticFm(ChaoticFm::new(rate)),
            SynthVoice::Vosim => Self::Vosim(Vosim::new(rate)),
            SynthVoice::Fof => Self::Fof(Fof::new(rate)),
        }
    }

    #[inline]
    fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        match self {
            Self::VirtualAnalog(synth) => synth.next(frequency, settings),
            Self::VirtualAnalogVcf(synth) => synth.next(frequency, settings),
            Self::Acid(synth) => synth.next(frequency, settings),
            Self::PhaseDistortion(synth) => synth.next(frequency, settings),
            Self::Formant(synth) => synth.next(frequency, settings),
            Self::Additive(synth) => synth.next(frequency, settings),
            Self::Swarm(synth) => synth.next(frequency, settings),
            Self::WaveTerrain(synth) => synth.next(frequency, settings),
            Self::Chord(synth) => synth.next(frequency, settings),
            Self::Chiptune(synth) => synth.next(frequency, settings),
            Self::Fm(synth) => synth.next(frequency, settings),
            Self::Kick(synth) => synth.next(frequency, settings),
            Self::Snare(synth) => synth.next(frequency, settings),
            Self::Hat(synth) => synth.next(frequency, settings),
            Self::Waveshaping(synth) => synth.next(frequency, settings),
            Self::Wavetable(synth) => synth.next(frequency, settings),
            Self::SixOp(synth) => synth.next(frequency, settings),
            Self::String(synth) => synth.next(frequency, settings),
            Self::Modal(synth) => synth.next(frequency, settings),
            Self::Dust(synth) => synth.next(frequency, settings),
            Self::Noise(synth) => synth.next(frequency, settings),
            Self::CSaw(synth) => synth.next(frequency, settings),
            Self::Morph(synth) => synth.next(frequency, settings),
            Self::SawSquare(synth) => synth.next(frequency, settings),
            Self::HardSync(synth) => synth.next(frequency, settings),
            Self::Triple(synth) => synth.next(frequency, settings),
            Self::Ring(synth) => synth.next(frequency, settings),
            Self::SawComb(synth) => synth.next(frequency, settings),
            Self::Toy(synth) => synth.next(frequency, settings),
            Self::ZFilter(synth) => synth.next(frequency, settings),
            Self::FeedbackFm(synth) => synth.next(frequency, settings),
            Self::ChaoticFm(synth) => synth.next(frequency, settings),
            Self::Vosim(synth) => synth.next(frequency, settings),
            Self::Fof(synth) => synth.next(frequency, settings),
        }
    }

    /// Let go of the note, for a synth with envelopes of its own.
    fn release(&mut self) {
        match self {
            Self::SixOp(synth) => synth.release(),
            Self::Acid(synth) => synth.release(),
            _ => {}
        }
    }

    /// Whether nothing it carries from note to note is left, so a voice let go can stop. `Acid`'s
    /// accent sweep drains between notes, as a 303's does, and the next note carries on from it.
    fn settled(&self) -> bool {
        match self {
            Self::Acid(synth) => synth.settled(),
            _ => true,
        }
    }

    /// Start a note, for a synth with envelopes of its own that an accent changes.
    fn trigger(&mut self, accent: bool) {
        if let Self::Acid(synth) = self {
            synth.trigger(accent);
        }
    }

    /// Whether it drifts as an analog synth does, which a seed for each note gives it.
    fn drifts(&self) -> bool {
        matches!(
            self,
            Self::VirtualAnalog(_) | Self::VirtualAnalogVcf(_) | Self::Acid(_) | Self::Chiptune(_)
        )
    }

    /// Roll a new note's drift, and its noise floor, from a seed. `Drift` as the note starts
    /// says whether its oscillators start anywhere in their cycles.
    fn seed_drift(&mut self, seed: u32, settings: Settings) {
        match self {
            Self::VirtualAnalog(synth) => synth.seed(seed, settings),
            Self::VirtualAnalogVcf(synth) => synth.seed(seed, settings),
            Self::Chiptune(synth) => synth.seed(seed, settings),
            Self::Acid(synth) => synth.seed(seed),
            _ => {}
        }
    }

    /// What `Drift` multiplies the note's envelope times by. `Acid`'s own envelopes take it
    /// themselves.
    fn time(&self, settings: Settings) -> f32 {
        match self {
            Self::VirtualAnalog(synth) => synth.time(settings),
            Self::VirtualAnalogVcf(synth) => synth.time(settings),
            Self::Chiptune(synth) => synth.time(settings),
            _ => 1.0,
        }
    }
}

/// One playing note. It keeps its sample ID until it ends, so replacing an instrument's sample
/// only affects new notes.
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    stage: Stage,
    sample: SampleId,
    /// The bank entry it plays, if it's a sampler's, which it follows as the entry changes, and
    /// which of its slices.
    entry: Option<u8>,
    slice: u8,
    /// Where it is in its sample, and how it reads on.
    reader: Reader,
    increment: f64,
    /// A tie gliding the speed to the next note's, if one is.
    gliding: Option<Gliding>,
    /// What modulation multiplies the speed by, and where that's heading.
    pitch: f64,
    pitch_target: f64,
    pitch_slew: f64,
    /// What it multiplies the sound by, the note's gain and its entry's, and where that's heading
    /// after the entry's gain changes.
    gain: f32,
    gain_target: f32,
    /// What its slice's pan multiplies the left and the right by, and where that's heading after
    /// the slice's pan changes.
    pan: [f32; 2],
    pan_target: [f32; 2],
    level: f32,
    shape: Envelope,
    rate: u32,
    /// The rate its envelope is worked out at, which `Drift` moves from `rate` to make its times
    /// longer or shorter.
    envelope_rate: u32,
    /// Frames since the stage began.
    elapsed: u64,
    /// Frames until a timed release starts, for previews.
    release_after: Option<u32>,
    /// A synth to play instead of the sample, and its settings.
    synth: Option<Synth>,
    settings: Settings,
    /// Whether it sounds until something ends it.
    holds: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            stage: Stage::Idle,
            sample: 0,
            entry: None,
            slice: 0,
            reader: Reader::default(),
            increment: 1.0,
            gliding: None,
            pitch: 1.0,
            pitch_target: 1.0,
            pitch_slew: 1.0,
            gain: 0.0,
            gain_target: 0.0,
            pan: [1.0, 1.0],
            pan_target: [1.0, 1.0],
            level: 0.0,
            shape: Envelope::note_shape(),
            rate: 48_000,
            envelope_rate: 48_000,
            elapsed: 0,
            release_after: None,
            synth: None,
            settings: Settings::default(),
            holds: false,
        }
    }
}

/// What a voice needs to start.
#[derive(Debug, Clone, Copy)]
pub struct NoteOn {
    pub sample: SampleId,
    /// A synth to play it, which ignores the sample.
    pub synth: Option<SynthVoice>,
    /// MIDI note. `C-3` plays at original pitch.
    pub note: u8,
    /// How hard it's played, as its gain comes from, for a Track mod slot to follow.
    pub velocity: u8,
    pub gain: f32,
    /// The bank entry it plays, if it's a sampler's, and which of its slices.
    pub entry: Option<u8>,
    pub slice: u8,
    /// What it plays of its sample, with its own gain and tune, or none for the whole sample as it
    /// is, as a synth's and a file's are.
    pub span: Option<Span>,
    /// What the track's `Start`, `Length` and `Play` make of the span.
    pub cut: Cut,
    /// Whether its velocity accents it, for a voice with an accent of its own.
    pub accent: bool,
    /// The shape of its level.
    pub shape: Envelope,
    pub release_after: Option<u32>,
}

impl NoteOn {
    /// How fast it plays: its note, where `C-3` is as recorded, at its entry's tune.
    fn increment(&self) -> f64 {
        2_f64.powf((f64::from(self.note) - 60.0) / 12.0) * self.span.map_or(1.0, |span| span.speed)
    }

    /// What it multiplies the sound by: its own gain and its entry's.
    fn sample_gain(&self) -> f32 {
        self.gain * self.span.map_or(1.0, |span| span.gain)
    }

    /// What its slice's pan multiplies the left and the right by.
    fn pan(&self) -> [f32; 2] {
        self.span.map_or([1.0, 1.0], |span| span.pan)
    }
}

impl Voice {
    /// Start a note, from a level if the last note's should carry on.
    pub fn start(note: NoteOn, rate: u32, from: f32, pitch: f64) -> Self {
        let mut synth = note.synth.map(|voice| Synth::new(voice, rate));
        if let Some(synth) = &mut synth {
            synth.trigger(note.accent);
        }
        Self {
            stage: Stage::Held { from },
            sample: note.sample,
            entry: note.entry,
            slice: note.slice,
            reader: Reader::new(note.span, note.cut),
            increment: note.increment(),
            gliding: None,
            pitch,
            pitch_target: pitch,
            pitch_slew: f64::from(fast_coefficient(rate)),
            gain: note.sample_gain(),
            gain_target: note.sample_gain(),
            pan: note.pan(),
            pan_target: note.pan(),
            level: from,
            shape: note.shape,
            rate,
            envelope_rate: rate,
            elapsed: 0,
            release_after: note.release_after,
            synth,
            holds: note.synth.is_some_and(SynthVoice::holds),
            settings: Settings::default(),
        }
    }

    /// Whether it is sounding.
    /// Choose the noise of a synth that uses any, so each hit differs but a render repeats.
    pub fn seed_noise(&mut self, seed: u32) {
        match &mut self.synth {
            Some(Synth::Snare(synth)) => synth.seed(seed),
            Some(Synth::Hat(synth)) => synth.seed(seed),
            Some(Synth::String(synth)) => synth.seed(seed),
            Some(Synth::Dust(synth)) => synth.seed(seed),
            Some(Synth::Noise(synth)) => synth.seed(seed),
            Some(Synth::Swarm(synth)) => synth.seed(seed),
            _ => {}
        }
    }

    /// Roll this note's drift from a seed, for a synth that drifts as an analog one does, once
    /// its settings are set, as `Drift` as it starts says how far its envelope's times move and
    /// whether its oscillators start anywhere in their cycles (decision 190).
    pub fn seed_drift(&mut self, seed: u32) {
        if let Some(synth) = &mut self.synth {
            synth.seed_drift(seed, self.settings);
            let time = synth.time(self.settings);
            self.envelope_rate = (self.rate as f32 * time).round() as u32;
        }
    }

    /// Whether this note's synth drifts, and so wants [`Voice::seed_drift`].
    pub fn drifts(&self) -> bool {
        self.synth.as_ref().is_some_and(Synth::drifts)
    }

    /// Whether this note's synth draws noise.
    pub fn uses_noise(&self) -> bool {
        matches!(
            self.synth,
            Some(
                Synth::Snare(_)
                    | Synth::Hat(_)
                    | Synth::String(_)
                    | Synth::Dust(_)
                    | Synth::Noise(_)
                    | Synth::Swarm(_)
            )
        )
    }

    /// Start the release of a note that would otherwise hold until something ended it.
    pub fn release_if_held(&mut self) {
        if self.holds {
            self.release();
        }
    }

    /// Whether its note sounds until something ends it, as a synth's like `VA` does.
    pub fn holds(&self) -> bool {
        self.holds
    }

    /// Start the release once the note has been held this many frames, or now if it has.
    pub fn release_once_sounded(&mut self, frames: u32) {
        if matches!(self.stage, Stage::Held { .. }) {
            match u64::from(frames).checked_sub(self.elapsed) {
                Some(left) if left > 0 => self.release_after = Some(left as u32),
                _ => self.release(),
            }
        }
    }

    pub fn set_synth(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Whether it's sounding and hasn't been let go.
    pub fn is_held(&self) -> bool {
        matches!(self.stage, Stage::Held { .. })
    }

    /// The level its envelope is at now.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Carry on as another note, with no new attack: a sample keeps playing at the new note's
    /// speed, and a synth changes pitch without starting its envelope again. With a glide time,
    /// it glides there in about that long, on the same lag `Acid` slides on, and without one it
    /// jumps. `Acid` always slides, for as long as its own `Slide` says unless it's given a time.
    pub fn tie(&mut self, note: NoteOn, glide_ms: Option<f32>) {
        let to = note.increment();
        self.gain = note.sample_gain();
        self.gain_target = self.gain;
        if let Some(Synth::Acid(synth)) = &mut self.synth {
            // It lags its pitch itself, so it's given the note at once.
            self.increment = to;
            self.gliding = None;
            synth.slide(note.accent, glide_ms);
            return;
        }
        self.gliding = glide_ms.map(|ms| {
            let frames = f64::from(ms) / 1000.0 * f64::from(self.rate) * GLIDE_SHARE;
            Gliding {
                to,
                share: 1.0 - (-1.0 / frames.max(1.0)).exp(),
            }
        });
        if self.gliding.is_none() {
            self.increment = to;
        }
    }

    /// Whether it's sounding an `Acid` note, which a new one carries on from.
    pub fn plays_acid(&self) -> bool {
        self.active() && matches!(self.synth, Some(Synth::Acid(_)))
    }

    /// Start a new note on the synth already playing, from where it is, for a voice that plays
    /// one note at a time and carries its filter and accent from note to note, as `Acid` does.
    /// Its level starts from where it is too, as the synth's own envelope gives its attack.
    pub fn retrigger(&mut self, note: NoteOn) {
        self.stage = Stage::Held { from: self.level };
        self.elapsed = 0;
        self.increment = note.increment();
        self.gliding = None;
        self.gain = note.sample_gain();
        self.gain_target = self.gain;
        self.shape = note.shape;
        self.release_after = note.release_after;
        if let Some(synth) = &mut self.synth {
            synth.trigger(note.accent);
        }
    }

    /// Where modulation wants its speed.
    pub fn set_pitch(&mut self, pitch: f64) {
        self.pitch_target = pitch;
    }

    /// Start the envelope's release. A one-shot shape plays on through, though a synth's own
    /// gate still closes.
    pub fn release(&mut self) {
        if matches!(self.stage, Stage::Held { .. }) {
            if !self.shape.one_shot() {
                self.stage = Stage::Released { from: self.level };
                self.elapsed = 0;
            }
            if let Some(synth) = &mut self.synth {
                synth.release();
            }
        }
    }

    /// Fade out quickly, to avoid a click.
    pub fn fade(&mut self) {
        if self.active() {
            self.stage = Stage::Fading {
                per_frame: 1.0 / DECLICK_FRAMES as f32,
            };
        }
    }

    /// The sample this voice is playing.
    pub fn sample(&self) -> SampleId {
        self.sample
    }

    /// The bank entry it's playing, if it's a sampler's.
    pub fn entry(&self) -> Option<u8> {
        self.entry
    }

    /// Carry on with its entry's span changed, as moving a point or changing its gain or tune does,
    /// heard at once and without a click: it reads on from where it is, a new gain fades in, and a
    /// new tune moves its pitch.
    pub fn respan(&mut self, span: Span) {
        let Some(old) = self.reader.span() else {
            return;
        };
        if old == span || !self.active() {
            return;
        }
        let ratio = span.speed / old.speed;
        self.increment *= ratio;
        if let Some(gliding) = &mut self.gliding {
            gliding.to *= ratio;
        }
        if span.gain != old.gain && old.gain != 0.0 {
            self.gain_target = self.gain_target / old.gain * span.gain;
        }
        self.pan_target = span.pan;
        self.reader.respan(span);
    }

    /// Whether it's playing a sample in time with the song's tempo, under `Play`'s `Tempo`, and so
    /// wants [`Voice::keep_time`] before each run.
    pub fn keeps_time(&self) -> bool {
        self.active() && self.synth.is_none() && self.reader.keeps_time()
    }

    /// Keep to the song's tempo for the next run, at this many frames a step. With `Beats`,
    /// `around` says where the sound its clock is in starts, and where the next does.
    pub fn keep_time(&mut self, step: f64, around: impl FnOnce(&Span, f64) -> (f64, f64)) {
        self.reader.keep_time(step, around);
    }

    /// The frame of its sample it's at, for a playhead, while it's sounding a sample: not a
    /// synth's note, and not one fading out after something cut it or took its place.
    pub fn frame(&self) -> Option<u64> {
        let sounding = matches!(self.stage, Stage::Held { .. } | Stage::Released { .. });
        (sounding && self.synth.is_none()).then(|| self.reader.frame())
    }

    /// The slice of its entry it's playing, and whether on past its end.
    pub fn slice(&self) -> (u8, bool) {
        (self.slice, self.reader.thru())
    }

    /// The next stereo frame, with reads bounds-checked. A voice past its sample's end stops.
    #[inline]
    pub fn next(&mut self, data: Option<&SampleData>) -> [f32; 2] {
        if self.stage == Stage::Idle {
            return [0.0, 0.0];
        }
        let source = if let Some(synth) = &mut self.synth {
            let frequency = MIDDLE_C * self.increment * self.pitch / f64::from(self.rate);
            let sample = synth.next(frequency as f32, self.settings);
            [sample, sample]
        } else {
            let Some(frame) = data.and_then(|data| self.reader.next(data)) else {
                self.stage = Stage::Idle;
                return [0.0, 0.0];
            };
            frame
        };
        self.level = match self.stage {
            Stage::Held { from } => self.shape.held(self.elapsed, from, self.envelope_rate),
            Stage::Released { from } => self.shape.released(self.elapsed, from, self.envelope_rate),
            Stage::Fading { per_frame } => self.level - per_frame,
            Stage::Idle => 0.0,
        }
        .max(0.0);
        if self.gain != self.gain_target {
            self.gain += (self.gain_target - self.gain) * self.pitch_slew as f32;
            if (self.gain - self.gain_target).abs() < 1e-6 {
                self.gain = self.gain_target;
            }
        }
        if self.pan != self.pan_target {
            for (pan, target) in self.pan.iter_mut().zip(self.pan_target) {
                *pan += (target - *pan) * self.pitch_slew as f32;
                if (*pan - target).abs() < 1e-6 {
                    *pan = target;
                }
            }
        }
        let level = self.level * self.gain;
        let frame = [
            source[0] * level * self.pan[0],
            source[1] * level * self.pan[1],
        ];
        self.pitch += (self.pitch_target - self.pitch) * self.pitch_slew;
        let more = self.synth.is_some() || self.reader.advance(self.increment * self.pitch);
        if let Some(gliding) = self.gliding {
            self.increment *= (gliding.to / self.increment).powf(gliding.share);
            if (self.increment / gliding.to - 1.0).abs() < GLIDE_DONE {
                self.increment = gliding.to;
                self.gliding = None;
            }
        }
        if let Some(frames) = &mut self.release_after {
            if *frames == 0 {
                self.release_after = None;
                self.release();
            } else {
                *frames -= 1;
            }
        }
        let finished = match self.stage {
            Stage::Held { .. } => self
                .shape
                .finished_holding(self.elapsed, self.envelope_rate),
            Stage::Released { .. } => {
                self.shape
                    .finished_releasing(self.elapsed, self.envelope_rate)
                    && self.synth.as_ref().is_none_or(Synth::settled)
            }
            Stage::Fading { .. } => self.level <= 0.0,
            Stage::Idle => false,
        } || !more;
        if finished {
            self.stage = Stage::Idle;
        }
        self.elapsed += 1;
        frame
    }
}
