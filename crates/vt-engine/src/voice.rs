//! Sample playback and its envelope.

use crate::dsp::voices::acid::Acid;
use crate::dsp::voices::additive::Additive;
use crate::dsp::voices::braids_analog::{CSaw, HardSync, Morph, SawSquare, Triple};
use crate::dsp::voices::braids_digital::{Ring, SawComb, Toy};
use crate::dsp::voices::braids_filter_fm::{ChaoticFm, DigitalFilter, FeedbackFm};
use crate::dsp::voices::braids_noise::{ClockedNoise, DigitalModulation, GranularCloud, TwinPeaks};
use crate::dsp::voices::braids_physical::{Blown, Bowed, Fluted, StruckBell, StruckDrum};
use crate::dsp::voices::braids_vocal::{Fof, Vosim};
use crate::dsp::voices::braids_waves::{Paraphonic, WaveLine, WaveMap};
use crate::dsp::voices::chiptune::Chiptune;
use crate::dsp::voices::chord::Chord;
use crate::dsp::voices::drums::{Hat, Snare};
use crate::dsp::voices::fm::Fm;
use crate::dsp::voices::four_op::FourOp;
use crate::dsp::voices::grain::Grain;
use crate::dsp::voices::kick::Kick;
use crate::dsp::voices::modal::Modal;
use crate::dsp::voices::noise::Noise;
use crate::dsp::voices::particle::ParticleNoise;
use crate::dsp::voices::phase_distortion::PhaseDistortion;
use crate::dsp::voices::six_op::SixOp;
use crate::dsp::voices::speech::Speech;
use crate::dsp::voices::string_model::StringModel;
use crate::dsp::voices::supersaw::Supersaw;
use crate::dsp::voices::swarm::Swarm;
use crate::dsp::voices::virtual_analog::{Settings, VirtualAnalog};
use crate::dsp::voices::virtual_analog_vcf::VirtualAnalogVcf;
use crate::dsp::voices::wave_terrain::WaveTerrain;
use crate::dsp::voices::waveshaping::Waveshaping;
use crate::dsp::voices::wavetable::Wavetable;
use vt_model::constants::DECLICK_FRAMES;
use vt_model::envelope::Envelope;
use vt_model::playable::Span;
use vt_model::samples::{Frames, SampleId};
use vt_model::song::SynthVoice;

use crate::dsp::blocks::smoother::fast_coefficient;
pub(crate) use crate::reader::Cut;
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

/// What a synth does to play a note on a voice. Each is plain data, held inline in a [`Synth`], so
/// the audio thread never allocates for one and reaches it through a match, never `dyn`. What
/// most synths don't do has a default.
pub(crate) trait SynthEngine: Copy {
    /// Whether it drifts as an analog synth does, which a seed for each note gives it.
    const DRIFTS: bool = false;
    /// Whether it draws noise, which a seed for each note chooses.
    const NOISE: bool = false;

    /// A new note, at a sample rate.
    fn new(rate: u32) -> Self;

    /// The next frame, at a frequency as a fraction of the sample rate.
    fn next(&mut self, frequency: f32, settings: Settings) -> f32;

    /// The next frame, on both sides: the same on each unless it spreads them, as `Supersaw` does.
    #[inline]
    fn next_sides(&mut self, frequency: f32, settings: Settings) -> [f32; 2] {
        let sample = self.next(frequency, settings);
        [sample, sample]
    }

    /// Start a note, for a synth with envelopes of its own that an accent changes.
    fn trigger(&mut self, _accent: bool) {}

    /// Let go of the note, for a synth with envelopes of its own.
    fn release(&mut self) {}

    /// Whether nothing it carries from note to note is left, so a voice let go can stop. `Acid`'s
    /// accent sweep drains between notes, as a 303's does, and the next note carries on from it.
    fn settled(&self) -> bool {
        true
    }

    /// Roll a new note's drift, and its noise floor, from a seed, for a synth that drifts.
    /// `Drift` as the note starts says whether its oscillators start anywhere in their cycles.
    fn seed_drift(&mut self, _seed: u32, _settings: Settings) {}

    /// What `Drift` multiplies the note's envelope times by. `Acid`'s own envelopes take it
    /// themselves.
    fn time(&self, _settings: Settings) -> f32 {
        1.0
    }

    /// Choose the noise of a synth that draws any, so each hit differs but a render repeats.
    fn seed_noise(&mut self, _seed: u32) {}
}

/// Writes [`Synth`] from one list of the `SynthVoice`s and the engines that play them: the enum,
/// `Synth::new` by voice, and the forwarding of each [`SynthEngine`] method.
macro_rules! synths {
    ($($voice:ident($engine:ty)),+ $(,)?) => {
        /// A synth playing a note. The audio thread can't allocate, so each voice has room for any
        /// of them.
        #[derive(Debug, Clone, Copy)]
        #[allow(clippy::large_enum_variant)]
        enum Synth {
            $($voice($engine),)+
        }

        impl Synth {
            /// A new note on a voice.
            fn new(voice: SynthVoice, rate: u32) -> Self {
                match voice {
                    $(SynthVoice::$voice => Self::$voice(SynthEngine::new(rate)),)+
                }
            }

            #[inline]
            fn next_sides(&mut self, frequency: f32, settings: Settings) -> [f32; 2] {
                match self {
                    $(Self::$voice(synth) => SynthEngine::next_sides(synth, frequency, settings),)+
                }
            }

            fn trigger(&mut self, accent: bool) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::trigger(synth, accent),)+
                }
            }

            fn release(&mut self) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::release(synth),)+
                }
            }

            fn settled(&self) -> bool {
                match self {
                    $(Self::$voice(synth) => SynthEngine::settled(synth),)+
                }
            }

            fn drifts(&self) -> bool {
                match self {
                    $(Self::$voice(_) => <$engine as SynthEngine>::DRIFTS,)+
                }
            }

            fn seed_drift(&mut self, seed: u32, settings: Settings) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::seed_drift(synth, seed, settings),)+
                }
            }

            fn time(&self, settings: Settings) -> f32 {
                match self {
                    $(Self::$voice(synth) => SynthEngine::time(synth, settings),)+
                }
            }

            fn uses_noise(&self) -> bool {
                match self {
                    $(Self::$voice(_) => <$engine as SynthEngine>::NOISE,)+
                }
            }

            fn seed_noise(&mut self, seed: u32) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::seed_noise(synth, seed),)+
                }
            }
        }
    };
}

// Each voice and its engine, in `SynthVoice`'s order.
synths! {
    VirtualAnalog(VirtualAnalog),
    VirtualAnalogVcf(VirtualAnalogVcf),
    Acid(Acid),
    Supersaw(Supersaw),
    Fm(Fm),
    PhaseDistortion(PhaseDistortion),
    Waveshaping(Waveshaping),
    Formant(Grain),
    Additive(Additive),
    Wavetable(Wavetable),
    WaveTerrain(WaveTerrain),
    SixOp(SixOp),
    FourOp(FourOp),
    Chord(Chord),
    Chiptune(Chiptune),
    Swarm(Swarm),
    String(StringModel),
    Modal(Modal),
    Dust(ParticleNoise),
    Noise(Noise),
    Kick(Kick),
    Snare(Snare),
    Hat(Hat),
    Speech(Speech),
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
    Bowed(Bowed),
    Blown(Blown),
    Fluted(Fluted),
    Bell(StruckBell),
    Drum(StruckDrum),
    WaveMap(WaveMap),
    WaveLine(WaveLine),
    Paraphonic(Paraphonic),
    TwinPeaks(TwinPeaks),
    Clocked(ClockedNoise),
    GrainCloud(GranularCloud),
    Modem(DigitalModulation),
}

/// One playing note. It keeps its sample ID until it ends, so replacing an instrument's sample
/// only affects new notes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Voice {
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
pub(crate) struct NoteOn {
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
    pub(crate) fn start(note: NoteOn, rate: u32, from: f32, pitch: f64) -> Self {
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

    /// Choose the noise of a synth that draws any, so each hit differs but a render repeats.
    pub(crate) fn seed_noise(&mut self, seed: u32) {
        if let Some(synth) = &mut self.synth {
            synth.seed_noise(seed);
        }
    }

    /// Roll this note's drift from a seed, for a synth that drifts as an analog one does, once
    /// its settings are set, as `Drift` as it starts says how far its envelope's times move and
    /// whether its oscillators start anywhere in their cycles (decision 190).
    pub(crate) fn seed_drift(&mut self, seed: u32) {
        if let Some(synth) = &mut self.synth {
            synth.seed_drift(seed, self.settings);
            let time = synth.time(self.settings);
            self.envelope_rate = (self.rate as f32 * time).round() as u32;
        }
    }

    /// Whether this note's synth drifts, and so wants [`Voice::seed_drift`].
    pub(crate) fn drifts(&self) -> bool {
        self.synth.as_ref().is_some_and(Synth::drifts)
    }

    /// Whether this note's synth draws noise, and so wants [`Voice::seed_noise`].
    pub(crate) fn uses_noise(&self) -> bool {
        self.synth.as_ref().is_some_and(Synth::uses_noise)
    }

    /// Start the release of a note that would otherwise hold until something ended it.
    pub(crate) fn release_if_held(&mut self) {
        if self.holds {
            self.release();
        }
    }

    /// Whether its note sounds until something ends it, as a synth's like `VA` does.
    pub(crate) fn holds(&self) -> bool {
        self.holds
    }

    /// Start the release once the note has been held this many frames, or now if it has.
    pub(crate) fn release_once_sounded(&mut self, frames: u32) {
        if matches!(self.stage, Stage::Held { .. }) {
            match u64::from(frames).checked_sub(self.elapsed) {
                Some(left) if left > 0 => self.release_after = Some(left as u32),
                _ => self.release(),
            }
        }
    }

    pub(crate) fn set_synth(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub(crate) fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Whether it's sounding and hasn't been let go.
    pub(crate) fn is_held(&self) -> bool {
        matches!(self.stage, Stage::Held { .. })
    }

    /// The level its envelope is at now.
    pub(crate) fn level(&self) -> f32 {
        self.level
    }

    /// Carry on as another note, with no new attack: a sample keeps playing at the new note's
    /// speed, and a synth changes pitch without starting its envelope again. With a glide time,
    /// it glides there in about that long, on the same lag `Acid` slides on, and without one it
    /// jumps. `Acid` always slides, for as long as its own `Slide` says unless it's given a time.
    pub(crate) fn tie(&mut self, note: NoteOn, glide_ms: Option<f32>) {
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
    pub(crate) fn plays_acid(&self) -> bool {
        self.active() && matches!(self.synth, Some(Synth::Acid(_)))
    }

    /// Start a new note on the synth already playing, from where it is, for a voice that plays
    /// one note at a time and carries its filter and accent from note to note, as `Acid` does.
    /// Its level starts from where it is too, as the synth's own envelope gives its attack.
    pub(crate) fn retrigger(&mut self, note: NoteOn) {
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
    pub(crate) fn set_pitch(&mut self, pitch: f64) {
        self.pitch_target = pitch;
    }

    /// Go to the pitch it's been given at once, for a voice that hasn't played yet.
    pub(crate) fn snap_pitch(&mut self) {
        self.pitch = self.pitch_target;
    }

    /// Start the envelope's release. A one-shot shape plays on through, though a synth's own
    /// gate still closes.
    pub(crate) fn release(&mut self) {
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
    pub(crate) fn fade(&mut self) {
        if self.active() {
            self.stage = Stage::Fading {
                per_frame: 1.0 / DECLICK_FRAMES as f32,
            };
        }
    }

    /// The sample this voice is playing.
    pub(crate) fn sample(&self) -> SampleId {
        self.sample
    }

    /// The bank entry it's playing, if it's a sampler's.
    pub(crate) fn entry(&self) -> Option<u8> {
        self.entry
    }

    /// Carry on with its entry's span changed, as moving a point or changing its gain or tune does,
    /// heard at once and without a click: it reads on from where it is, a new gain fades in, and a
    /// new tune moves its pitch.
    #[allow(
        clippy::float_cmp,
        reason = "only a gain that moves at all rescales the target"
    )]
    pub(crate) fn respan(&mut self, span: Span) {
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
    pub(crate) fn keeps_time(&self) -> bool {
        self.active() && self.synth.is_none() && self.reader.keeps_time()
    }

    /// Keep to the song's tempo for the next run, at this many frames a step. With `Beats`,
    /// `around` says where the sound its clock is in starts, and where the next does.
    pub(crate) fn keep_time(&mut self, step: f64, around: impl FnOnce(&Span, f64) -> (f64, f64)) {
        self.reader.keep_time(step, self.rate, around);
    }

    /// The frame of its sample it's at, for a playhead, while it's sounding a sample: not a
    /// synth's note, and not one fading out after something cut it or took its place.
    pub(crate) fn frame(&self) -> Option<u64> {
        let sounding = matches!(self.stage, Stage::Held { .. } | Stage::Released { .. });
        (sounding && self.synth.is_none()).then(|| self.reader.frame())
    }

    /// The slice of its entry it's playing, and whether on past its end.
    pub(crate) fn slice(&self) -> (u8, bool) {
        (self.slice, self.reader.thru())
    }

    /// The next stereo frame, with reads bounds-checked. A voice past its sample's end stops.
    #[inline]
    #[allow(clippy::float_cmp, reason = "the glides land on their targets exactly")]
    pub(crate) fn next(&mut self, data: Option<Frames<'_>>) -> [f32; 2] {
        if self.stage == Stage::Idle {
            return [0.0, 0.0];
        }
        let source = if let Some(synth) = &mut self.synth {
            let frequency = MIDDLE_C * self.increment * self.pitch / f64::from(self.rate);
            synth.next_sides(frequency as f32, self.settings)
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
