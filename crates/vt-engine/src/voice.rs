//! Sample playback and its envelope, and the synths a voice can play in place of a sample.

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
use crate::dsp::voices::virtual_analog::VirtualAnalog;
use crate::dsp::voices::virtual_analog_vcf::VirtualAnalogVcf;
use crate::dsp::voices::wave_terrain::WaveTerrain;
use crate::dsp::voices::waveshaping::Waveshaping;
use crate::dsp::voices::wavetable::Wavetable;
use crate::dsp::voices::{Settings, SynthEngine};
use vt_model::constants::{CUT_FADE_FRAMES, RESTART_DIP_FRAMES};
use vt_model::envelope::Envelope;
use vt_model::playable::Span;
use vt_model::samples::{Frames, SampleId};
use vt_model::song::SynthVoice;

use crate::dsp::blocks::coefficient::of_time;
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
    /// A carried note's dip to nothing with `Reset`, from the level it had over
    /// [`RESTART_DIP_FRAMES`], so many frames into it.
    Dip {
        from: f32,
        at: u32,
    },
    /// Cut off, by a new note on a voice of its own, a choke, `KIL` or the transport stopping:
    /// a fade from the level it had over [`CUT_FADE_FRAMES`], so many frames into it.
    Cut {
        from: f32,
        at: u32,
    },
}

/// How much of a cut voice's level is left, `at` frames into its fade: a quarter of a cosine,
/// which starts flat (decision 368).
#[inline]
fn cut_fall(at: u32) -> f32 {
    let into = (at as f32 / CUT_FADE_FRAMES as f32).min(1.0);
    (std::f32::consts::FRAC_PI_2 * into).cos()
}

/// How much of a carried note's level is left `at` frames into its dip, counting the frame being
/// made: half a cosine, flat where it starts and where it reaches nothing, so neither end is a
/// corner the top of the spectrum hears (decision 377).
#[inline]
fn dip_fall(at: u32) -> f32 {
    if at >= RESTART_DIP_FRAMES {
        return 0.0;
    }
    let into = at as f32 / RESTART_DIP_FRAMES as f32;
    0.5 * (1.0 + (std::f32::consts::PI * into).cos())
}

/// How far a new note rises over the fade of the one it cut, `at` frames into it: the fall's
/// power complement, `√(1 − fall²)`, which is a quarter of a sine, so the two together keep their
/// power where the notes differ (decision 368).
#[inline]
fn cut_rise(at: u32) -> f32 {
    let into = (at as f32 / CUT_FADE_FRAMES as f32).min(1.0);
    (std::f32::consts::FRAC_PI_2 * into).sin()
}

/// The time constant of each of the two one-poles a voice's amp follows its envelope through,
/// about half a millisecond for the pair: the shortest that takes the click out of an envelope's
/// step to nothing on a sine, measured against a cut's 64-frame fade (decision 366).
const AMP_POLE_SECONDS: f32 = 0.000_25;

/// How far the amp is from its envelope's level when it's taken as there: under -120 dB.
const AMP_SETTLED: f32 = 1e-6;

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

            fn rung_out(&self) -> bool {
                match self {
                    $(Self::$voice(synth) => SynthEngine::rung_out(synth),)+
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

            fn reseed_drift(&mut self, seed: u32, settings: Settings) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::reseed_drift(synth, seed, settings),)+
                }
            }

            fn restart_phase(&mut self, rate: u32) {
                match self {
                    $(Self::$voice(synth) => SynthEngine::restart_phase(synth, rate),)+
                }
            }

            fn carries(&self) -> bool {
                match self {
                    $(Self::$voice(_) => <$engine as SynthEngine>::CARRIES,)+
                }
            }

            fn voice(&self) -> SynthVoice {
                match self {
                    $(Self::$voice(_) => SynthVoice::$voice,)+
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

/// A note carried on by a voice already sounding, with what it draws of its track's random
/// streams, drawn as it's played so each stream goes on as it would for a new voice. With `dip`,
/// it waits for the voice's level to fall to nothing first.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Carry {
    pub note: NoteOn,
    /// A seed for the synth's noise, if it draws any.
    pub noise: Option<u32>,
    /// A seed for its drift, if it drifts.
    pub drift: Option<u32>,
    /// Whether its level falls to nothing over the short fade before its attack, as the note's
    /// shape's `Reset` asks, rather than its attack starting from where the level is.
    pub dip: bool,
    /// Whether the synth's oscillators start again from where a new note starts them, at the
    /// point the dip reaches nothing. Only with `dip`.
    pub restart: bool,
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
    /// Where its envelope is.
    level: f32,
    /// The envelope's level through two short one-poles, as a VCA's gain is smoothed, so an
    /// envelope's step doesn't click: the first's, and the second's, which the sound is
    /// multiplied by. Then how much of the way each goes a frame.
    amp: [f32; 2],
    amp_coefficient: f32,
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
    /// A note carried on, waiting for the fade to reach nothing.
    carried: Option<Carry>,
    /// Whether it went quiet on its own, its envelope run to nothing, rather than cut, so a note
    /// on the same synth can take it up again (decision 374).
    rested: bool,
    /// Frames into its rise over the fade of the note it cut, while it's rising.
    rising: Option<u32>,
    /// Whether its note has been let go, by its gate closing, an `OFF` or its own length, which a
    /// one-shot shape's stage doesn't show.
    let_go: bool,
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
            amp: [0.0; 2],
            amp_coefficient: 1.0,
            shape: Envelope::note_shape(),
            rate: 48_000,
            envelope_rate: 48_000,
            elapsed: 0,
            release_after: None,
            synth: None,
            settings: Settings::default(),
            holds: false,
            carried: None,
            rested: false,
            rising: None,
            let_go: false,
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
    /// Start a note on a voice of its own, from nothing whatever its shape's `Reset` says: a new
    /// voice's sound starts from nothing too, so a level carried over to it would be a step
    /// (decision 373). Only a note carried on by the voice sounding starts from where it is.
    pub(crate) fn start(note: NoteOn, rate: u32, pitch: f64) -> Self {
        Self::start_from(note, rate, 0.0, pitch)
    }

    /// Start a note from a level.
    fn start_from(note: NoteOn, rate: u32, from: f32, pitch: f64) -> Self {
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
            // From where its envelope starts, so an attack of 0 starts at full on its first frame.
            amp: [note.shape.held(0, from, rate); 2],
            amp_coefficient: of_time(AMP_POLE_SECONDS, rate),
            shape: note.shape,
            rate,
            envelope_rate: rate,
            elapsed: 0,
            release_after: note.release_after,
            synth,
            holds: note.synth.is_some_and(SynthVoice::holds),
            settings: Settings::default(),
            carried: None,
            rested: false,
            rising: None,
            let_go: false,
        }
    }

    /// Start a note under the fade of the one it cut. An attack as long as the fade or longer
    /// plays as it's set, under the old note's fall. A shorter one sits inside the fade, so the
    /// note starts at full instead, its decay as much sooner as from full without `Reset`, and
    /// rises over the fade as the old note falls, at equal power (decision 368).
    pub(crate) fn start_under_cut(note: NoteOn, rate: u32, pitch: f64) -> Self {
        if note.shape.attack_frames(rate) >= u64::from(CUT_FADE_FRAMES) {
            return Self::start(note, rate, pitch);
        }
        Self {
            rising: Some(0),
            ..Self::start_from(note, rate, 1.0, pitch)
        }
    }

    /// Whether a note on this synth carries on from this voice rather than starting a voice of
    /// its own: the voice is sounding the same synth, and the synth holds its notes, so its
    /// oscillators run on into the next note, as an analog mono synth's do (decision 367). A
    /// synth that doesn't hold, a drum or a struck model, starts each note afresh, as its strike
    /// is the sound.
    pub(crate) fn carries(&self, synth: Option<SynthVoice>) -> bool {
        let sounding = match self.stage {
            Stage::Held { .. } | Stage::Released { .. } => true,
            Stage::Dip { .. } => self.carried.is_some(),
            Stage::Cut { .. } | Stage::Idle => false,
        };
        sounding && self.plays_on(synth)
    }

    /// Whether a note on this synth takes up this voice after it has gone quiet on its own, its
    /// envelope run to nothing, rather than starting a voice of its own: its oscillators go on
    /// from where they stopped, as an analog synth's run on between notes, so `Phase` at `Free`
    /// stays free after a pluck. A voice cut by a new note, a choke, `KIL` or stopping doesn't
    /// (decision 374).
    pub(crate) fn resumes(&self, synth: Option<SynthVoice>) -> bool {
        self.stage == Stage::Idle && self.rested && self.plays_on(synth)
    }

    /// Whether its synth holds its notes and carries them, and is the one a note is for.
    fn plays_on(&self, synth: Option<SynthVoice>) -> bool {
        self.holds
            && self.synth.as_ref().is_some_and(|playing| {
                playing.carries() && synth.is_some_and(|voice| playing.voice() == voice)
            })
    }

    /// Carry on as a new note, on the synth already playing, with no new voice: its oscillators
    /// keep running, its envelope starts again, and the synth is struck as a new note strikes
    /// it. With `dip`, the level falls to nothing over the short fade first, and the note starts
    /// there, at its own pitch, with an attack from nothing. Without, its attack starts from where
    /// the level is, at once, and a new velocity glides in rather than stepping.
    pub(crate) fn carry(&mut self, carry: Carry) {
        // What was timed for the note before is the old note's.
        self.release_after = None;
        if self.stage == Stage::Idle {
            // Gone quiet on its own, it has no level to dip from or velocity to glide from: the
            // note starts as on a voice of its own, at its own pitch, but on the synth as it was.
            self.carried = None;
            self.rested = false;
            self.level = 0.0;
            self.amp = [0.0; 2];
            self.pitch = self.pitch_target;
            self.strike(carry);
            return;
        }
        if carry.dip {
            self.carried = Some(carry);
            // The dip is a ramp of its own, from what's heard.
            self.level = self.amp[1];
            self.stage = Stage::Dip {
                from: self.level,
                at: 0,
            };
            return;
        }
        self.carried = None;
        let gain = self.gain;
        self.strike(carry);
        self.gain = gain;
    }

    /// Start a carried note, from where the level is: at nothing after a dip.
    fn strike(&mut self, carry: Carry) {
        let note = carry.note;
        self.stage = Stage::Held { from: self.level };
        self.let_go = false;
        self.elapsed = 0;
        self.increment = note.increment();
        self.gliding = None;
        self.gain = note.sample_gain();
        self.gain_target = self.gain;
        self.shape = note.shape;
        self.release_after = note.release_after;
        let Some(synth) = &mut self.synth else {
            return;
        };
        if carry.restart {
            synth.restart_phase(self.rate);
        }
        synth.trigger(note.accent);
        if let Some(seed) = carry.noise {
            synth.seed_noise(seed);
        }
        if let Some(seed) = carry.drift {
            if carry.restart {
                synth.seed_drift(seed, self.settings);
            } else {
                synth.reseed_drift(seed, self.settings);
            }
            self.envelope_rate = (self.rate as f32 * synth.time(self.settings)).round() as u32;
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
    /// whether its oscillators start anywhere in their cycles.
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

    /// The level its envelope is at now.
    #[cfg(test)]
    pub(crate) fn level(&self) -> f32 {
        self.level
    }

    /// Whether it's sounding and hasn't been let go.
    pub(crate) fn is_held(&self) -> bool {
        matches!(self.stage, Stage::Held { .. })
    }

    /// Whether its note's gate is still open, nothing having let it go, and it plays what a note
    /// would: the same synth, or the same sample, entry and slice. A one-shot shape plays on
    /// through its release, but its gate has closed all the same.
    pub(crate) fn gate_open_on(&self, note: &NoteOn) -> bool {
        self.is_held()
            && !self.let_go
            && match (&self.synth, note.synth) {
                (Some(playing), Some(voice)) => playing.voice() == voice,
                (None, None) => {
                    self.sample == note.sample
                        && self.entry == note.entry
                        && self.slice == note.slice
                }
                _ => false,
            }
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
        self.let_go = false;
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
        // A note let go before its dip has reached nothing never starts.
        self.carried = None;
        self.let_go = true;
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
        self.carried = None;
        // Cut, even after going quiet, the next note starts afresh.
        self.rested = false;
        if self.active() {
            // The fade is a ramp of its own, from what's heard, a rise over a cut before it too,
            // which the amp follows at once.
            let rise = self.rising.take().map_or(1.0, cut_rise);
            self.stage = Stage::Cut {
                from: self.amp[1] * rise,
                at: 0,
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
            Stage::Dip { from, at } => {
                self.stage = Stage::Dip { from, at: at + 1 };
                from * dip_fall(at + 1)
            }
            Stage::Cut { from, at } => {
                self.stage = Stage::Cut { from, at: at + 1 };
                from * cut_fall(at)
            }
            Stage::Idle => 0.0,
        }
        .max(0.0);
        // A fall goes through two one-poles in a row rather than Braids' one, so the amp's slope
        // starts from flat as well as its level: as smooth on a sine as a cut's fade, in half the
        // time. A rise is an attack, which it follows at once, as it's set, and so is a cut's
        // fade, a ramp of its own.
        let c = self.amp_coefficient;
        self.amp[0] += (self.level - self.amp[0]) * c;
        self.amp[1] += (self.amp[0] - self.amp[1]) * c;
        let fading = matches!(self.stage, Stage::Dip { .. } | Stage::Cut { .. });
        if fading
            || self.level >= self.amp[1]
            || self
                .amp
                .iter()
                .all(|amp| (amp - self.level).abs() < AMP_SETTLED)
        {
            self.amp = [self.level; 2];
        }
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
        let mut level = self.amp[1] * self.gain;
        if let Some(at) = self.rising {
            level *= cut_rise(at);
            self.rising = (at + 1 < CUT_FADE_FRAMES).then_some(at + 1);
        }
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
        // Its envelope done, it sounds on until its amp has followed to nothing.
        let finished = match self.stage {
            Stage::Held { .. } => {
                self.shape
                    .finished_holding(self.elapsed, self.envelope_rate)
                    && self.amp[1] == 0.0
            }
            // With no shape of its own, it ends as its machine settles, at full: at once for one
            // with no envelope of its own, as a raw oscillator with no amp would (decision 371).
            Stage::Released { .. } if self.shape == Envelope::Open => {
                self.synth.as_ref().is_none_or(Synth::rung_out)
            }
            Stage::Released { .. } => {
                self.shape
                    .finished_releasing(self.elapsed, self.envelope_rate)
                    && self.amp[1] == 0.0
                    && self.synth.as_ref().is_none_or(Synth::settled)
            }
            Stage::Dip { .. } | Stage::Cut { .. } => self.level <= 0.0 && self.amp[1] == 0.0,
            Stage::Idle => false,
        } || !more;
        self.elapsed += 1;
        if finished {
            // Its envelope run to nothing, not cut and not a sample ended, a synth's voice can
            // be taken up again.
            self.rested = more && matches!(self.stage, Stage::Held { .. } | Stage::Released { .. });
            self.stage = Stage::Idle;
            // A dip reaching nothing starts the note carried on, at its own pitch at once.
            if let Some(carry) = self.carried.take() {
                self.level = 0.0;
                self.pitch = self.pitch_target;
                self.strike(carry);
            }
        }
        frame
    }
}
