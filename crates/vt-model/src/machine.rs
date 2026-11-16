//! What makes a track's sound: the sampler, or one of the synth voices, each described once in
//! [`VOICES`]; and what a track playing it is for.

use serde::{Deserialize, Serialize};

use crate::grid::{GRID_ROWS, Grid};
use crate::settings::voices::{
    ACID, ADDITIVE, BLOWN, BOWED, CHIPTUNE, CHORD, CLOCKED, CSAW, DUST, FEEDBACK_FM, FLUTED, FM,
    FOF, FOLD, FORMANT, FOUR_OP, GRAIN_CLOUD, HAT, KICK, MODEM, MORPH, NOISE, PARAPHONIC, PD, RING,
    SAW_COMB, SAW_SQUARE, SIX_OP, SNARE, SPEECH, STRUCK, STRUCK_BELL, STRUCK_DRUM, SUPERSAW, SWARM,
    SYNC, TERRAIN, TOY, TRIPLE, TWIN_PEAKS, VA, VCF, VOSIM, WAVE_LINE, WAVE_MAP, WAVES, Z_FILTER,
};
use crate::settings::{
    Entry, FOUR_OP_EACH, FOUR_OP_SHARED, SAMPLER_SETTINGS, SettingId, voice_entries,
};

/// A synth's accent before any is set: Plaits' 0.8, as it strikes with nothing patched to its
/// level.
pub const DEFAULT_ACCENT: u8 = 0xCC;

/// A `Gate` that holds each note to the next, as every machine but `Acid` starts with.
pub const FULL_GATE: u8 = 0xFF;
/// `Acid`'s `Gate`, 50%, as a TB-303's gate closes about halfway through each step.
pub const ACID_GATE: u8 = 0x80;

/// What makes a track's sound.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Machine {
    /// Plays one of the bank's samples, once, at the note's speed.
    #[default]
    Sampler,
    /// One of the synth voices, after Mutable Instruments' Plaits, each with its own settings.
    Plaits(SynthVoice),
}

impl Machine {
    /// How much of its last pulse a note lasts until a track sets its own `Gate`.
    pub(crate) fn gate_length(self) -> u8 {
        match self {
            Self::Sampler => FULL_GATE,
            Self::Plaits(voice) => voice.spec().gate,
        }
    }

    /// Every machine, in the order the machine list, its `-` and `=` and the Roto's knob go
    /// through them: `Sampler` first, as the most used, then the voices kind by kind.
    pub const ALL: [Self; 1 + SynthVoice::ALL.len()] = {
        let mut all = [Self::Sampler; 1 + SynthVoice::ALL.len()];
        let mut at = 1;
        let mut kind = 0;
        while kind < SynthVoice::KINDS.len() {
            let voices = SynthVoice::KINDS[kind].1;
            let mut voice = 0;
            while voice < voices.len() {
                all[at] = Self::Plaits(voices[voice]);
                at += 1;
                voice += 1;
            }
            kind += 1;
        }
        assert!(at == all.len(), "every voice is in one kind");
        all
    };

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sampler => "Sampler",
            Self::Plaits(voice) => voice.name(),
        }
    }

    /// What it does, for a list to show.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Sampler => "A sample from the bank, at the speed of its note",
            Self::Plaits(voice) => voice.meaning(),
        }
    }

    /// The category a list shows it under, or none for the top of the list: `Sampler` is at the
    /// top, and each voice is under its kind.
    pub fn category(self) -> &'static str {
        match self {
            Self::Sampler => "",
            Self::Plaits(voice) => voice.kind(),
        }
    }

    /// Its settings, by their places among them: the sampler's own, or a synth's.
    pub fn settings(self) -> impl Iterator<Item = SettingId> {
        let count = match self {
            Self::Sampler => SAMPLER_SETTINGS,
            Self::Plaits(voice) => voice_entries(voice).len(),
        };
        (0..count).filter_map(move |at| match self {
            Self::Sampler => SettingId::sampler(at),
            Self::Plaits(voice) => SettingId::voice(voice, at),
        })
    }

    /// The place of a setting it has, by the key its file saves it under.
    fn setting_at(self, key: &str) -> Option<usize> {
        self.settings().position(|id| id.entry().key == key)
    }

    /// The setting that gives it its character, which its device's title shows after it, as an
    /// effect's type is: the sample the sampler plays, or a voice's [`VoiceSpec::type_key`], such
    /// as `Acid`'s wave or 6-op's patch. Other synths have none.
    pub fn type_param(self) -> Option<usize> {
        let key = match self {
            Self::Sampler => "sample",
            Self::Plaits(voice) => voice.spec().type_key?,
        };
        self.setting_at(key)
    }

    /// Where its settings sit on the Sound page, by their places among them, its type being in the
    /// title instead. Most have theirs down each column in the order they're listed. The sampler's
    /// say which part of the sample plays and how down its first column, and where that part starts
    /// and how long it is down its second, as the sample page has them; a voice that places its
    /// own has them in its [`VoiceSpec::grid`].
    pub fn grid(self) -> Grid {
        let placed: &[&[&str]] = match self {
            Self::Sampler => &[&["slice", "play"], &["start", "length"]],
            Self::Plaits(voice) => match voice.spec().grid {
                Some(placed) => placed,
                None => {
                    let own: Vec<usize> = (0..self.settings().count())
                        .filter(|at| Some(*at) != self.type_param())
                        .collect();
                    let columns: Vec<Vec<Option<usize>>> = own
                        .chunks(GRID_ROWS)
                        .map(|column| column.iter().copied().map(Some).collect())
                        .collect();
                    return Grid::from_columns(&columns);
                }
            },
        };
        let columns: Vec<Vec<Option<usize>>> = placed
            .iter()
            .map(|column| column.iter().map(|key| self.setting_at(key)).collect())
            .collect();
        Grid::from_columns(&columns)
    }

    /// The devices its settings go on past its own, for a machine with more than a device holds:
    /// 4-op's four operators, `Op 1` to `Op 4`, each its pitch, its sound and its envelope down
    /// four columns, by their places among the machine's settings. Other machines have none.
    pub fn operators(self) -> Vec<(String, Grid)> {
        if self != Self::Plaits(SynthVoice::FourOp) {
            return Vec::new();
        }
        (0..4)
            .map(|operator| {
                let at = |setting: usize| Some(FOUR_OP_SHARED + operator * FOUR_OP_EACH + setting);
                let grid = Grid::from_columns(&[
                    vec![at(0), at(1), at(2)],
                    vec![at(3), at(4), at(5)],
                    vec![at(6), at(7)],
                    vec![at(8), at(9)],
                ]);
                (format!("Op {}", operator + 1), grid)
            })
            .collect()
    }

    /// What a track playing it is for until it's told: the drum voices are drums, `Acid` a bass,
    /// the chord voices a pad, the noises a texture and every other voice a synth; a sample could
    /// be anything, so the sampler says nothing.
    pub fn role(self) -> Role {
        match self {
            Self::Sampler => Role::None,
            Self::Plaits(voice) => voice.spec().role,
        }
    }
}

crate::listed::listed!(Machine: meaning, category);

/// What a sound track is for, which its colour shows wherever it's named. `Auto`, where every track
/// starts, goes by its machine; `None` has no colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    #[default]
    Auto,
    None,
    Drums,
    Bass,
    Synth,
    Pad,
    Texture,
}

impl Role {
    /// Every role, in the order its list and `-` and `=` go through them.
    pub const ALL: [Self; 7] = [
        Self::Auto,
        Self::None,
        Self::Drums,
        Self::Bass,
        Self::Synth,
        Self::Pad,
        Self::Texture,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::None => "None",
            Self::Drums => "Drums",
            Self::Bass => "Bass",
            Self::Synth => "Synth",
            Self::Pad => "Pad",
            Self::Texture => "Texture",
        }
    }

    /// What it's for, for a list to show.
    pub(crate) fn meaning(self) -> &'static str {
        match self {
            Self::Auto => "Whatever its machine is for",
            Self::None => "No colour",
            Self::Drums => "Drums and percussion",
            Self::Bass => "A bass line",
            Self::Synth => "A lead, an arpeggio or a melody",
            Self::Pad => "Chords and pads",
            Self::Texture => "Noise, risers and atmosphere",
        }
    }

    pub fn is_auto(&self) -> bool {
        *self == Self::Auto
    }
}

crate::listed::listed!(Role: meaning);

/// How many patches 6-op has, which its `Patch` counts from `00`.
pub const SIX_OP_PATCHES: u8 = 96;

/// Which synth the Plaits machine plays, in the order they came, which is [`SynthVoice::ALL`]'s
/// and [`VOICES`]'.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SynthVoice {
    /// A square and a saw.
    #[default]
    VirtualAnalog,
    /// A saw or a square, with a sub, through a resonant filter.
    VirtualAnalogVcf,
    /// A saw or a square through a diode ladder, with accent and slide, after Roland's TB-303.
    Acid,
    /// Up to eight detuned saws, after Roland's JP-8000.
    Supersaw,
    /// Two-operator FM.
    Fm,
    /// A sine bent by a second wave's phase.
    PhaseDistortion,
    /// A slope through a waveshaper and a wavefolder.
    Waveshaping,
    /// Two formants, made of grains.
    Formant,
    /// 24 harmonics, in bumps along the spectrum.
    Additive,
    /// A grid of waves in four banks.
    Wavetable,
    /// A path traced over a surface of waves.
    WaveTerrain,
    /// Six-operator FM, playing DX7 patches.
    SixOp,
    /// Four-operator FM you build yourself, after TetraOP's matrix.
    FourOp,
    /// A chord from one note, on organ, strings or waves.
    Chord,
    /// A chord of square waves, as on old game consoles.
    Chiptune,
    /// A cloud of eight saws in grains.
    Swarm,
    /// A plucked string.
    String,
    /// A struck resonator.
    Modal,
    /// Random impulses through resonant filters.
    Dust,
    /// Clocked noise through a resonant filter.
    Noise,
    /// An 808-style bass drum.
    Kick,
    /// An 808-style snare drum.
    Snare,
    /// An 808-style hi-hat.
    Hat,
    /// Formants, vowels and the words of old speech chips.
    Speech,
    /// A saw that starts each cycle on a step, after the CS-80's.
    CSaw,
    /// A triangle morphing through a saw and a square to a pulse, through a fuzzy filter.
    Morph,
    /// A saw blended into a square.
    SawSquare,
    /// Two squares or saws in hard sync.
    HardSync,
    /// Three oscillators of one wave, as a chord.
    Triple,
    /// A sine ring modulated by two more.
    Ring,
    /// A saw through a tuned comb filter.
    SawComb,
    /// A low-resolution, circuit-bent tone.
    Toy,
    /// A resonant filter faked as Casio's CZ did, by a sine restarted each cycle.
    ZFilter,
    /// A sine bent by a second, which its own output bends.
    FeedbackFm,
    /// A sine bent by a second, whose speed its own output moves.
    ChaoticFm,
    /// Two sines at pitches of their own, windowed each cycle of the note.
    Vosim,
    /// A saw through five formant filters, from vowel to vowel.
    Fof,
    /// A bowed string.
    Bowed,
    /// A reed blown into a bore.
    Blown,
    /// A jet of air across a flute's bore.
    Fluted,
    /// A bell of eleven partials, struck.
    Bell,
    /// A drum of six partials and noise, struck.
    Drum,
    /// A map of waves to wander over.
    WaveMap,
    /// A line of waves to scan.
    WaveLine,
    /// A chord of four voices on a line of waves.
    Paraphonic,
    /// Noise through two resonant peaks.
    TwinPeaks,
    /// Clocked noise in a few steps, that loops.
    Clocked,
    /// A cloud of sine grains.
    GrainCloud,
    /// A carrier keyed through four phases, as a modem sends data.
    Modem,
}

impl SynthVoice {
    /// Every voice, in the order they came. The machine list shows them by kind, as
    /// `Machine::ALL` has them.
    pub const ALL: [Self; 49] = [
        Self::VirtualAnalog,
        Self::VirtualAnalogVcf,
        Self::Acid,
        Self::Supersaw,
        Self::Fm,
        Self::PhaseDistortion,
        Self::Waveshaping,
        Self::Formant,
        Self::Additive,
        Self::Wavetable,
        Self::WaveTerrain,
        Self::SixOp,
        Self::FourOp,
        Self::Chord,
        Self::Chiptune,
        Self::Swarm,
        Self::String,
        Self::Modal,
        Self::Dust,
        Self::Noise,
        Self::Kick,
        Self::Snare,
        Self::Hat,
        Self::Speech,
        Self::CSaw,
        Self::Morph,
        Self::SawSquare,
        Self::HardSync,
        Self::Triple,
        Self::Ring,
        Self::SawComb,
        Self::Toy,
        Self::ZFilter,
        Self::FeedbackFm,
        Self::ChaoticFm,
        Self::Vosim,
        Self::Fof,
        Self::Bowed,
        Self::Blown,
        Self::Fluted,
        Self::Bell,
        Self::Drum,
        Self::WaveMap,
        Self::WaveLine,
        Self::Paraphonic,
        Self::TwinPeaks,
        Self::Clocked,
        Self::GrainCloud,
        Self::Modem,
    ];

    /// The kinds the machine list groups the voices under, in its order, each with its voices
    /// in theirs.
    pub const KINDS: [(&'static str, &'static [Self]); 8] = [
        (
            "Analog",
            &[
                Self::VirtualAnalog,
                Self::VirtualAnalogVcf,
                Self::Acid,
                Self::Supersaw,
                Self::Chiptune,
                Self::CSaw,
                Self::Morph,
                Self::SawSquare,
                Self::HardSync,
                Self::Triple,
            ],
        ),
        (
            "FM",
            &[
                Self::Fm,
                Self::SixOp,
                Self::FourOp,
                Self::FeedbackFm,
                Self::ChaoticFm,
            ],
        ),
        (
            "Digital",
            &[
                Self::PhaseDistortion,
                Self::Waveshaping,
                Self::Additive,
                Self::Ring,
                Self::SawComb,
                Self::Toy,
                Self::ZFilter,
            ],
        ),
        (
            "Wavetable",
            &[
                Self::Wavetable,
                Self::WaveTerrain,
                Self::WaveMap,
                Self::WaveLine,
                Self::Paraphonic,
            ],
        ),
        (
            "Physical",
            &[
                Self::String,
                Self::Modal,
                Self::Bowed,
                Self::Blown,
                Self::Fluted,
                Self::Bell,
                Self::Drum,
            ],
        ),
        (
            "Voice",
            &[
                Self::Formant,
                Self::Speech,
                Self::Chord,
                Self::Vosim,
                Self::Fof,
            ],
        ),
        (
            "Noise",
            &[
                Self::Dust,
                Self::Noise,
                Self::Swarm,
                Self::TwinPeaks,
                Self::Clocked,
                Self::GrainCloud,
                Self::Modem,
            ],
        ),
        ("Drums", &[Self::Kick, Self::Snare, Self::Hat]),
    ];

    /// The kind the machine list shows it under.
    pub(crate) fn kind(self) -> &'static str {
        Self::KINDS
            .iter()
            .find(|(_, voices)| voices.contains(&self))
            .map_or("", |(kind, _)| kind)
    }

    /// Its description.
    #[inline]
    pub(crate) const fn spec(self) -> &'static VoiceSpec {
        &VOICES[self as usize]
    }

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        self.spec().name
    }

    /// What it does, for a list to show.
    pub(crate) const fn meaning(self) -> &'static str {
        self.spec().meaning
    }

    /// Whether a note keeps sounding until something ends it.
    pub const fn holds(self) -> bool {
        self.spec().holds
    }

    /// Whether it's one of Braids' models, rather than Plaits' or our own.
    pub const fn is_macro(self) -> bool {
        self.spec().braids
    }
}

crate::listed::listed!(SynthVoice: meaning);

impl SynthVoice {
    /// Whether its sound has a phase that a track's `Phase` can start again on each note: not a
    /// noise's, nor a struck or blown model's, whose strike starts it afresh anyway, nor `Acid`'s,
    /// which carries on as a 303 does (decision 370).
    pub const fn has_phase(self) -> bool {
        !matches!(
            self,
            Self::Acid
                | Self::String
                | Self::Modal
                | Self::Dust
                | Self::Noise
                | Self::Kick
                | Self::Snare
                | Self::Hat
                | Self::Bowed
                | Self::Blown
                | Self::Fluted
                | Self::Bell
                | Self::Drum
                | Self::TwinPeaks
                | Self::Clocked
                | Self::GrainCloud
        )
    }
}

/// Whether a synth track's oscillators run on from note to note or start again on each, its
/// `Phase` (decision 370).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Phase {
    /// They run on, as an analog synth's do.
    #[default]
    Free,
    /// They start again from where a new note starts them, at the bottom of a short dip to
    /// nothing, so every note starts the same.
    Reset,
}

/// What each phase is called, in order.
pub const PHASE_NAMES: [&str; 2] = ["Free", "Reset"];

impl Phase {
    /// Both, in order.
    pub const ALL: [Self; 2] = [Self::Free, Self::Reset];

    /// Its place in [`Phase::ALL`].
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The one at a place in [`Phase::ALL`], the last for one past it.
    pub fn from_index(index: u8) -> Self {
        Self::ALL[usize::from(index).min(Self::ALL.len() - 1)]
    }

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        PHASE_NAMES[self as usize]
    }

    /// What it does, for a list to choose from.
    pub(crate) const fn meaning(self) -> &'static str {
        match self {
            Self::Free => "oscillators run on from note to note",
            Self::Reset => "oscillators start again on each note",
        }
    }

    /// Whether it's `Free`, as a track starts, which a song doesn't save.
    pub(crate) fn is_free(&self) -> bool {
        *self == Self::Free
    }
}

crate::listed::listed!(Phase: meaning);

/// How a track's notes join, its `Mode` (decision 380). Each mode keeps to its own behaviour:
/// `Legato` adds one rule to `Mono` and takes nothing else from anywhere.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum VoiceMode {
    /// One voice, each note struck: a holding synth carries its voice on, by `Reset`'s dip or
    /// from where its level is, and anything else starts under the old note's fade.
    #[default]
    Mono,
    /// As `Mono`, but a note arriving while the one sounding still has its gate open, on the same
    /// synth or sample, ties into it as `HLD` ties: its pitch moves, by `Glide` if set, and its
    /// velocity is taken, with nothing struck again.
    Legato,
}

/// What each voice mode is called, in order.
pub const VOICE_MODE_NAMES: [&str; 2] = ["Mono", "Legato"];

impl VoiceMode {
    /// Both, in order.
    pub const ALL: [Self; 2] = [Self::Mono, Self::Legato];

    /// Its place in [`VoiceMode::ALL`].
    pub const fn index(self) -> u8 {
        self as u8
    }

    /// The one at a place in [`VoiceMode::ALL`], the last for one past it.
    pub fn from_index(index: u8) -> Self {
        Self::ALL[usize::from(index).min(Self::ALL.len() - 1)]
    }

    /// The name on screen.
    pub const fn name(self) -> &'static str {
        VOICE_MODE_NAMES[self as usize]
    }

    /// What it does, for a list to choose from.
    pub(crate) const fn meaning(self) -> &'static str {
        match self {
            Self::Mono => "each note strikes",
            Self::Legato => "a note ties into one whose gate is open",
        }
    }

    /// Whether it's `Mono`, as a track starts, which a song doesn't save.
    pub(crate) fn is_mono(&self) -> bool {
        *self == Self::Mono
    }
}

crate::listed::listed!(VoiceMode: meaning);

/// One synth voice, described once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VoiceSpec {
    /// What its settings are stored under, which never changes.
    pub key: &'static str,
    /// On screen and in lists.
    pub name: &'static str,
    /// What it does, for a list to choose from.
    pub meaning: &'static str,
    /// Its settings, in the order it takes them.
    pub params: &'static [Entry],
    /// Whether a note keeps sounding until something ends it.
    pub holds: bool,
    /// Whether it's one of Braids' models, rather than Plaits' or our own.
    pub braids: bool,
    /// The key of the setting that gives it its character, which its device's title shows, or
    /// none.
    pub type_key: Option<&'static str>,
    /// Where it places its settings on the Sound page, by key, column by column, or none to have
    /// them down each column in the order they're listed.
    pub grid: Option<&'static [&'static [&'static str]]>,
    /// What a track playing it is for until it's told: the drum voices are drums, `Acid` a bass,
    /// the chord voices a pad, the noises a texture and every other voice a synth.
    pub role: Role,
    /// How much of its last pulse a note lasts until a track sets its own `Gate`.
    pub gate: u8,
}

/// Every voice's description, by [`SynthVoice`].
pub(crate) const VOICES: [VoiceSpec; SynthVoice::ALL.len()] = [
    VoiceSpec {
        key: "va",
        name: "VA",
        meaning: "A square and a saw, for bass and leads",
        params: &VA,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "vcf",
        name: "VA VCF",
        meaning: "A saw or square with a sub, through a filter",
        params: &VCF,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "acid",
        name: "Acid",
        meaning: "A 303-style bass, with accent and slide",
        params: &ACID,
        holds: true,
        braids: false,
        type_key: Some("wave"),
        // In pairs as a TB-303's knobs are, the filter's, its envelope's, then accent and slide,
        // with the Devil Fish's drive and `Drift` after them.
        grid: Some(&[
            &["cutoff", "resonance"],
            &["envmod", "decay"],
            &["accent", "slide"],
            &["drive", "drift"],
        ]),
        role: Role::Bass,
        gate: ACID_GATE,
    },
    VoiceSpec {
        key: "supersaw",
        name: "Supersaw",
        meaning: "Up to eight detuned saws, for trance leads and pads",
        params: &SUPERSAW,
        holds: true,
        braids: false,
        type_key: Some("stack"),
        // By what they do: the stack's spread, then how it sounds, then how each note starts and
        // strays.
        grid: Some(&[
            &["saws", "detune", "warp"],
            &["mix", "shape", "width"],
            &["phase", "drift"],
        ]),
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "fm",
        name: "FM",
        meaning: "Two-operator FM, for bass, bells and keys",
        params: &FM,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "pd",
        name: "PD",
        meaning: "A sine bent out of shape, as on Casio's CZ",
        params: &PD,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "fold",
        name: "Fold",
        meaning: "A wave folded into harmonics, for bass and leads",
        params: &FOLD,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "formant",
        name: "Formant",
        meaning: "Two formants of grains, for vowels and leads",
        params: &FORMANT,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "additive",
        name: "Additive",
        meaning: "24 harmonics in bumps, for organs and glass",
        params: &ADDITIVE,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "waves",
        name: "Waves",
        meaning: "Waves in four banks to sweep, for pads and leads",
        params: &WAVES,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "terrain",
        name: "Terrain",
        meaning: "A path over a surface of waves, for evolving tones",
        params: &TERRAIN,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "6op",
        name: "6-op",
        meaning: "Six-op FM with 96 DX7 patches, for keys and bells",
        params: &SIX_OP,
        holds: true,
        braids: false,
        type_key: Some("patch"),
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "4op",
        name: "4-op",
        meaning: "Four-op FM you build, for bass, bells and keys",
        params: &FOUR_OP,
        holds: true,
        braids: false,
        type_key: Some("algorithm"),
        // Only what its operators share, each operator's being on a device of its own (see
        // `Machine::operators`).
        grid: Some(&[&["feedback", "ring", "beat"]]),
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "chord",
        name: "Chord",
        meaning: "A chord from one note, on organ, strings or waves",
        params: &CHORD,
        holds: true,
        braids: false,
        type_key: Some("chord"),
        grid: None,
        role: Role::Pad,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "chiptune",
        name: "Chip",
        meaning: "A chord of squares, as on old game consoles",
        params: &CHIPTUNE,
        holds: true,
        braids: false,
        type_key: Some("chord"),
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "swarm",
        name: "Swarm",
        meaning: "Eight saws in grains, for pads and drones",
        params: &SWARM,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "string",
        name: "String",
        meaning: "A plucked string, from a sitar's buzz to metal",
        params: &STRUCK,
        holds: false,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "modal",
        name: "Modal",
        meaning: "A struck resonator, for bells, bars and wood",
        params: &STRUCK,
        holds: false,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "dust",
        name: "Dust",
        meaning: "Random clicks, filtered, for crackle and rain",
        params: &DUST,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "noise",
        name: "Noise",
        meaning: "Clocked noise through a filter, for hiss and wind",
        params: &NOISE,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "kick",
        name: "Kick",
        meaning: "An 808-style bass drum, tuned by its note",
        params: &KICK,
        holds: false,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Drums,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "snare",
        name: "Snare",
        meaning: "An 808-style snare drum, tuned by its note",
        params: &SNARE,
        holds: false,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Drums,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "hat",
        name: "Hat",
        meaning: "An 808-style hi-hat, tuned by its note",
        params: &HAT,
        holds: false,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Drums,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "speech",
        name: "Speech",
        meaning: "Formants, vowels and words, for robots and chants",
        params: &SPEECH,
        holds: true,
        braids: false,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "csaw",
        name: "CSaw",
        meaning: "A saw with a notch, after the CS-80's, for brass",
        params: &CSAW,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "morph",
        name: "Morph",
        meaning: "Triangle to saw to pulse, with fuzz, for leads",
        params: &MORPH,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "sawsquare",
        name: "Saw Sq",
        meaning: "A saw blended into a square, for bass and leads",
        params: &SAW_SQUARE,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "sync",
        name: "Sync",
        meaning: "Two waves in hard sync, for tearing leads",
        params: &SYNC,
        holds: true,
        braids: true,
        type_key: Some("wave"),
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "triple",
        name: "Triple",
        meaning: "Three oscillators as a chord, for stabs and pads",
        params: &TRIPLE,
        holds: true,
        braids: true,
        type_key: Some("wave"),
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "ring",
        name: "Ring",
        meaning: "Three sines ring modulated, for bells and clangs",
        params: &RING,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "sawcomb",
        name: "Saw Comb",
        meaning: "A saw through a tuned comb, for flanged leads",
        params: &SAW_COMB,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "toy",
        name: "Toy",
        meaning: "A low-resolution, circuit-bent tone, for lo-fi",
        params: &TOY,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "zfilter",
        name: "Z Filter",
        meaning: "A resonant filter faked by phase, for CZ sweeps",
        params: &Z_FILTER,
        holds: true,
        braids: true,
        type_key: Some("type"),
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "fbfm",
        name: "FB FM",
        meaning: "FM with feedback, for growling bass and leads",
        params: &FEEDBACK_FM,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "chaosfm",
        name: "Chaos FM",
        meaning: "FM that never settles, for chaotic tones",
        params: &FEEDBACK_FM,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "vosim",
        name: "VOSIM",
        meaning: "Two windowed sines, for nasal and vocal leads",
        params: &VOSIM,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "fof",
        name: "FOF",
        meaning: "A saw through vowel formants, for choirs and pads",
        params: &FOF,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "bowed",
        name: "Bowed",
        meaning: "A bowed string, for cellos, fiddles and drones",
        params: &BOWED,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "blown",
        name: "Blown",
        meaning: "A reed blown into a bore, for woodwinds",
        params: &BLOWN,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "fluted",
        name: "Fluted",
        meaning: "A jet of air across a bore, for flutes and pipes",
        params: &FLUTED,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "bell",
        name: "Bell",
        meaning: "A struck bell of eleven partials, which rings out",
        params: &STRUCK_BELL,
        holds: false,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "drum",
        name: "Drum",
        meaning: "A struck drum of partials and noise, for toms",
        params: &STRUCK_DRUM,
        holds: false,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Drums,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "wavemap",
        name: "Wave Map",
        meaning: "A map of waves to wander over, for digital tones",
        params: &WAVE_MAP,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "waveline",
        name: "Wave Line",
        meaning: "A line of waves to scan, rough or smooth",
        params: &WAVE_LINE,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Synth,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "paraphonic",
        name: "Paraphonic",
        meaning: "A chord of four voices on waves, for pads",
        params: &PARAPHONIC,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Pad,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "twinpeaks",
        name: "Twin Peaks",
        meaning: "Noise through two resonant peaks, for whistles",
        params: &TWIN_PEAKS,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "clocked",
        name: "Clocked",
        meaning: "Looping 8-bit noise, for bleeps and glitches",
        params: &CLOCKED,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "graincloud",
        name: "Grain Cloud",
        meaning: "A cloud of sine grains, for shimmer and texture",
        params: &GRAIN_CLOUD,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
    VoiceSpec {
        key: "modem",
        name: "Modem",
        meaning: "A modem's data tone, for digital chatter",
        params: &MODEM,
        holds: true,
        braids: true,
        type_key: None,
        grid: None,
        role: Role::Texture,
        gate: FULL_GATE,
    },
];

// `SynthVoice::spec` finds a voice's by its number, so `ALL` has each at its own.
const _: () = {
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        assert!(
            SynthVoice::ALL[at] as usize == at,
            "ALL is in SynthVoice's order"
        );
        at += 1;
    }
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_machine_list_starts_with_the_sampler_then_each_voice_once_under_its_kind() {
        // The sampler is the most used, so it comes first, at the top with no heading.
        assert_eq!(Machine::ALL[0], Machine::Sampler);
        assert_eq!(Machine::Sampler.category(), "");
        for voice in SynthVoice::ALL {
            let kinds: Vec<&str> = SynthVoice::KINDS
                .iter()
                .flat_map(|(kind, voices)| voices.iter().filter(|v| **v == voice).map(|_| *kind))
                .collect();
            assert_eq!(kinds.len(), 1, "{voice:?} is in {kinds:?}");
            assert_eq!(Machine::Plaits(voice).category(), kinds[0]);
            let listed = Machine::ALL
                .iter()
                .filter(|m| **m == Machine::Plaits(voice));
            assert_eq!(listed.count(), 1, "{voice:?}");
        }
        // Each kind's voices are next to each other, in the kinds' order, as the picker wants.
        let categories: Vec<&str> = Machine::ALL[1..].iter().map(|m| m.category()).collect();
        let headings: Vec<&str> = categories
            .chunk_by(|a, b| a == b)
            .map(|run| run[0])
            .collect();
        let kinds: Vec<&str> = SynthVoice::KINDS.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(headings, kinds);
    }

    #[test]
    fn every_machines_grid_places_each_of_its_settings_once_with_its_type_in_the_title() {
        use crate::grid::GRID_COLUMNS;
        use crate::settings::Kind;
        for machine in Machine::ALL {
            let grid = machine.grid();
            let count = machine.settings().count();
            let mut placed: Vec<usize> = grid.settings().collect();
            for (_, operator) in machine.operators() {
                placed.extend(operator.settings());
            }
            placed.sort_unstable();
            let wanted: Vec<usize> = (0..count)
                .filter(|at| Some(*at) != machine.type_param())
                .collect();
            assert_eq!(placed, wanted, "{machine:?}: {grid:?}");
            // A type is a choice from a list, as an effect's is.
            if let Some(at) = machine.type_param() {
                let id = machine.settings().nth(at).unwrap();
                assert!(
                    matches!(id.entry().kind, Kind::Stepped { .. }),
                    "{machine:?}"
                );
            }
            // No column is left empty before one that's used.
            let used: Vec<usize> = (0..GRID_COLUMNS)
                .filter(|column| (0..GRID_ROWS).any(|row| grid.at(row, *column).is_some()))
                .collect();
            assert_eq!(used, (0..used.len()).collect::<Vec<_>>(), "{machine:?}");
        }
    }

    #[test]
    fn the_sampler_and_acid_place_their_settings_by_what_they_mean() {
        let names = |machine: Machine, row: usize| {
            let ids: Vec<SettingId> = machine.settings().collect();
            machine
                .grid()
                .row(row)
                .map(|at| at.map_or("", |at| ids[at].name()))
        };
        assert_eq!(Machine::Sampler.type_param(), Some(0), "the sample");
        assert_eq!(names(Machine::Sampler, 0), ["Slice", "Start", "", ""]);
        assert_eq!(names(Machine::Sampler, 1), ["Play", "Length", "", ""]);
        let acid = Machine::Plaits(SynthVoice::Acid);
        assert_eq!(acid.type_param(), Some(0), "the wave");
        assert_eq!(names(acid, 0), ["Cutoff", "Env mod", "Accent", "Drive"]);
        assert_eq!(names(acid, 1), ["Resonance", "Decay", "Slide", "Drift"]);
        assert_eq!(names(acid, 2), ["", "", "", ""]);
        let kick = Machine::Plaits(SynthVoice::Kick);
        assert_eq!(kick.type_param(), None);
        assert_eq!(names(kick, 0), ["Punch", "Accent", "", ""]);
        assert_eq!(names(kick, 2), ["Decay", "", "", ""]);
        let six_op = Machine::Plaits(SynthVoice::SixOp);
        assert_eq!(names(six_op, 0), ["Bright", "", "", ""]);
        assert_eq!(names(six_op, 2), ["Accent", "", "", ""]);
        let chord = Machine::Plaits(SynthVoice::Chord);
        assert_eq!(chord.type_param(), Some(0), "the chord");
        assert_eq!(names(chord, 1), ["Wave", "", "", ""]);
    }

    #[test]
    fn the_noises_are_textures_and_the_drums_drums() {
        for voice in SynthVoice::ALL {
            let role = voice.spec().role;
            match voice.kind() {
                "Noise" => assert_eq!(role, Role::Texture, "{voice:?}"),
                "Drums" => assert_eq!(role, Role::Drums, "{voice:?}"),
                _ => assert_ne!(role, Role::Texture, "{voice:?}"),
            }
        }
    }
}
