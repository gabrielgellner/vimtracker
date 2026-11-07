//! Every setting a step can lock or a source can move, registered once: the track's, each
//! machine's, each effect's in each slot, and each LFO's. An entry has a key that songs store, a
//! long name for finding it, a short label for the grid, its kind and its default. A setting's
//! place in the table is its [`SettingId`], fixed when the program is built.

use crate::constants::{MAX_LEVEL, UNITY_LEVEL};
use crate::insert::{DELAY_PARAMS, DISTORT_PARAMS, INSERTS, Insert, Param, REVERB_PARAMS};
use crate::lfo::LFOS_PER_TRACK;
use crate::patch_names::PATCH_NAMES;
use crate::song::{DEFAULT_ACCENT, Machine, SynthVoice, Track};
use crate::step::FxCommand;

/// The most settings a machine has.
pub const MAX_MACHINE_SETTINGS: usize = 4;

/// What values a setting takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// From `00` up to a top: `FF`, or the level scale's top for volume, sends and gain.
    Level { top: u8 },
    /// Either side of `80`, like pan.
    Centred,
    /// A place in a list of choices, which goes round past the end or stops.
    List {
        choices: &'static [&'static str],
        wraps: bool,
    },
}

/// One setting, as its owner declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The name songs store it by, which never changes.
    pub key: &'static str,
    /// What it's called in lists and on the status line.
    pub name: &'static str,
    /// The grid's label, 3 or 4 letters. Without one, modulation can move it but steps can't.
    pub label: Option<&'static str>,
    pub kind: Kind,
    pub default: u8,
}

const fn level(key: &'static str, name: &'static str, label: &'static str, default: u8) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: Kind::Level { top: 0xFF },
        default,
    }
}

const fn centred(key: &'static str, name: &'static str, label: &'static str) -> Entry {
    Entry {
        key,
        name,
        label: Some(label),
        kind: Kind::Centred,
        default: 0x80,
    }
}

const fn on_level_scale(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    default: u8,
) -> Entry {
    Entry {
        kind: Kind::Level { top: MAX_LEVEL },
        ..level(key, name, label, default)
    }
}

/// The track's own, in the order the table keeps them.
const TRACK: [Entry; 7] = [
    on_level_scale("vol", "Volume", "VOL", UNITY_LEVEL),
    centred("pan", "Pan", "PAN"),
    level("cut", "Cutoff", "CUT", 0xFF),
    level("res", "Resonance", "RES", 0x00),
    on_level_scale("sna", "Send A", "SNA", 0x00),
    on_level_scale("snb", "Send B", "SNB", 0x00),
    Entry {
        key: "pitch",
        name: "Pitch",
        label: None,
        kind: Kind::Centred,
        default: 0x80,
    },
];
/// What each of the track's is listed with.
const TRACK_GROUPS: [&str; 7] = [
    "Output", "Output", "Filter", "Filter", "Output", "Output", "Machine",
];

const ACCENT: Entry = level("accent", "Accent", "ACC", DEFAULT_ACCENT);

const VA: [Entry; 3] = [
    centred("detune", "Detune", "DTUN"),
    level("square", "Square", "SQR", 0x00),
    level("saw", "Saw", "SAW", 0x80),
];
const FM: [Entry; 3] = [
    level("ratio", "Ratio", "RAT", 0x80),
    level("index", "Index", "INDX", 0x00),
    centred("feedback", "Feedback", "FDBK"),
];
const FOLD: [Entry; 3] = [
    centred("shape", "Shape", "SHPE"),
    level("fold", "Fold", "FOLD", 0x00),
    level("slope", "Slope", "SLPE", 0x80),
];
const WAVES: [Entry; 3] = [
    level("bank", "Bank", "BANK", 0x80),
    level("wave", "Wave", "WAVE", 0x00),
    level("row", "Row", "ROW", 0x80),
];
const SIX_OP: [Entry; 4] = [
    Entry {
        key: "patch",
        name: "Patch",
        label: Some("PAT"),
        kind: Kind::List {
            choices: &PATCH_NAMES,
            wraps: false,
        },
        // E.PIANO 1.
        default: 0x20,
    },
    centred("bright", "Bright", "BRT"),
    centred("time", "Time", "TIME"),
    ACCENT,
];
/// A string's or a bar's: Material starts where it's in tune.
const STRUCK: [Entry; 4] = [
    level("material", "Material", "MATL", 0x40),
    level("bright", "Bright", "BRT", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const DUST: [Entry; 3] = [
    level("spread", "Spread", "SPRD", 0x80),
    level("density", "Density", "DENS", 0x00),
    centred("filter", "Filter", "FILT"),
];
const NOISE: [Entry; 3] = [
    centred("type", "Type", "TYPE"),
    level("clock", "Clock", "CLCK", 0x00),
    level("ring", "Ring", "RING", 0x80),
];
const KICK: [Entry; 4] = [
    level("punch", "Punch", "PNCH", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const SNARE: [Entry; 4] = [
    level("snappy", "Snappy", "SNAP", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];
const HAT: [Entry; 4] = [
    level("noise", "Noise", "NOIS", 0x80),
    level("tone", "Tone", "TONE", 0x00),
    level("decay", "Decay", "DCAY", 0x80),
    ACCENT,
];

/// A voice's settings, in the order it takes them.
pub const fn voice_entries(voice: SynthVoice) -> &'static [Entry] {
    match voice {
        SynthVoice::VirtualAnalog => &VA,
        SynthVoice::Fm => &FM,
        SynthVoice::Waveshaping => &FOLD,
        SynthVoice::Wavetable => &WAVES,
        SynthVoice::SixOp => &SIX_OP,
        SynthVoice::String | SynthVoice::Modal => &STRUCK,
        SynthVoice::Dust => &DUST,
        SynthVoice::Noise => &NOISE,
        SynthVoice::Kick => &KICK,
        SynthVoice::Snare => &SNARE,
        SynthVoice::Hat => &HAT,
    }
}

/// The name a voice's settings are stored under.
pub const fn voice_key(voice: SynthVoice) -> &'static str {
    match voice {
        SynthVoice::VirtualAnalog => "va",
        SynthVoice::Fm => "fm",
        SynthVoice::Waveshaping => "fold",
        SynthVoice::Wavetable => "waves",
        SynthVoice::SixOp => "6op",
        SynthVoice::String => "string",
        SynthVoice::Modal => "modal",
        SynthVoice::Dust => "dust",
        SynthVoice::Noise => "noise",
        SynthVoice::Kick => "kick",
        SynthVoice::Snare => "snare",
        SynthVoice::Hat => "hat",
    }
}

/// An effect a slot can hold, apart from its settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Distort,
    Delay,
    Reverb,
}

impl Effect {
    pub const ALL: [Self; 3] = [Self::Distort, Self::Delay, Self::Reverb];

    /// Its settings, in the order its slot keeps them. Distort's Amount is shown as Bits on
    /// Crush, and is the same setting.
    pub const fn params(self) -> &'static [Param] {
        match self {
            Self::Distort => &DISTORT_PARAMS,
            Self::Delay => &DELAY_PARAMS,
            Self::Reverb => &REVERB_PARAMS,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Distort => "distort",
            Self::Delay => "delay",
            Self::Reverb => "reverb",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Distort => "Distort",
            Self::Delay => "Delay",
            Self::Reverb => "Reverb",
        }
    }

    /// The effect a slot holds, if any.
    pub fn of(insert: Insert) -> Option<Self> {
        match insert {
            Insert::Empty => None,
            Insert::Distort(_) => Some(Self::Distort),
            Insert::Delay(_) => Some(Self::Delay),
            Insert::Reverb(_) => Some(Self::Reverb),
        }
    }
}

/// An effect's setting as an entry.
fn param_entry(param: &Param) -> Entry {
    Entry {
        key: param.key,
        name: param.name,
        label: Some(param.label),
        kind: if param.choices.is_empty() {
            Kind::Level {
                top: param.setting.highest(),
            }
        } else {
            Kind::List {
                choices: param.choices,
                wraps: param.wraps,
            }
        },
        default: param.default,
    }
}

const LFO: [Entry; 2] = [
    Entry {
        key: "depth",
        name: "Depth",
        label: None,
        kind: Kind::Centred,
        default: 0x80,
    },
    Entry {
        key: "phase",
        name: "Phase",
        label: None,
        kind: Kind::Level { top: 0xFF },
        default: 0x00,
    },
];

const fn voices_size() -> usize {
    let mut total = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        total += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    total
}

/// Where a voice's settings start among the voices'.
const fn voice_start(voice: SynthVoice) -> usize {
    let mut start = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        if SynthVoice::ALL[at] as usize == voice as usize {
            return start;
        }
        start += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    start
}

const fn effect_start(effect: Effect) -> usize {
    match effect {
        Effect::Distort => 0,
        Effect::Delay => DISTORT_PARAMS.len(),
        Effect::Reverb => DISTORT_PARAMS.len() + DELAY_PARAMS.len(),
    }
}

const VOICES_START: usize = TRACK.len();
const SLOTS_START: usize = VOICES_START + voices_size();
/// The places a slot's settings take: one for each setting of each effect it could hold.
const SLOT_SIZE: usize = DISTORT_PARAMS.len() + DELAY_PARAMS.len() + REVERB_PARAMS.len();
const LFOS_START: usize = SLOTS_START + INSERTS * SLOT_SIZE;

/// What a setting belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Track,
    Voice(SynthVoice),
    Slot { slot: usize, effect: Effect },
    Lfo(usize),
}

/// A setting's place in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingId(u16);

impl SettingId {
    /// How many settings there are.
    pub const COUNT: usize = LFOS_START + LFOS_PER_TRACK * LFO.len();

    /// Every setting, in the table's order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u16).map(Self)
    }

    /// Its place, for an array of every setting.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// One of the track's own, by its place among them.
    pub const fn track(setting: usize) -> Option<Self> {
        if setting < TRACK.len() {
            Some(Self(setting as u16))
        } else {
            None
        }
    }

    /// One of a voice's settings, by its place among the voice's.
    pub const fn voice(voice: SynthVoice, setting: usize) -> Option<Self> {
        if setting < voice_entries(voice).len() {
            Some(Self((VOICES_START + voice_start(voice) + setting) as u16))
        } else {
            None
        }
    }

    /// One of an effect's settings, in a slot.
    pub const fn slot(slot: usize, effect: Effect, setting: usize) -> Option<Self> {
        if slot < INSERTS && setting < effect.params().len() {
            Some(Self(
                (SLOTS_START + slot * SLOT_SIZE + effect_start(effect) + setting) as u16,
            ))
        } else {
            None
        }
    }

    /// One of an LFO's settings: 0 is depth, 1 is phase.
    pub const fn lfo(lfo: usize, setting: usize) -> Option<Self> {
        if lfo < LFOS_PER_TRACK && setting < LFO.len() {
            Some(Self((LFOS_START + lfo * LFO.len() + setting) as u16))
        } else {
            None
        }
    }

    /// What it belongs to, and its place among that owner's settings.
    pub fn owner(self) -> (Owner, usize) {
        let at = self.index();
        if at < VOICES_START {
            return (Owner::Track, at);
        }
        if at < SLOTS_START {
            let within = at - VOICES_START;
            for voice in SynthVoice::ALL {
                let start = voice_start(voice);
                if within < start + voice_entries(voice).len() {
                    return (Owner::Voice(voice), within - start);
                }
            }
        }
        if at < LFOS_START {
            let within = at - SLOTS_START;
            let (slot, within) = (within / SLOT_SIZE, within % SLOT_SIZE);
            for effect in Effect::ALL {
                let start = effect_start(effect);
                if within < start + effect.params().len() {
                    return (Owner::Slot { slot, effect }, within - start);
                }
            }
        }
        let within = at - LFOS_START;
        (Owner::Lfo(within / LFO.len()), within % LFO.len())
    }

    /// What its owner declares.
    pub fn entry(self) -> Entry {
        match self.owner() {
            (Owner::Track, at) => TRACK[at],
            (Owner::Voice(voice), at) => voice_entries(voice)[at],
            (Owner::Slot { effect, .. }, at) => param_entry(&effect.params()[at]),
            (Owner::Lfo(_), at) => LFO[at],
        }
    }

    /// The name songs store it by, which never changes, like `cut`, `string.decay`,
    /// `2.delay.feedback` or `lfo1.depth`.
    pub fn key(self) -> String {
        let key = self.entry().key;
        match self.owner().0 {
            Owner::Track => key.to_owned(),
            Owner::Voice(voice) => format!("{}.{key}", voice_key(voice)),
            Owner::Slot { slot, effect } => format!("{}.{}.{key}", slot + 1, effect.key()),
            Owner::Lfo(lfo) => format!("lfo{}.{key}", lfo + 1),
        }
    }

    /// The setting a stored key names.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().find(|id| id.key() == key)
    }

    /// Its label in the grid, with its slot's digit before an effect's, if steps can lock it.
    pub fn label(self) -> Option<String> {
        let label = self.entry().label?;
        Some(match self.owner().0 {
            Owner::Slot { slot, .. } => format!("{}{label}", slot + 1),
            _ => label.to_owned(),
        })
    }

    /// What it's called in lists.
    pub fn name(self) -> &'static str {
        self.entry().name
    }

    /// What it's listed with.
    pub fn group(self) -> String {
        match self.owner() {
            (Owner::Track, at) => TRACK_GROUPS[at].to_owned(),
            (Owner::Voice(voice), _) => voice.name().to_owned(),
            (Owner::Slot { slot, effect }, _) => format!("FX {} {}", slot + 1, effect.name()),
            (Owner::Lfo(lfo), _) => format!("LFO {}", lfo + 1),
        }
    }
}

/// The events, which act on a note and aren't settings. Their labels share the grid with
/// settings', so none may match one.
pub const EVENTS: [FxCommand; 5] = [
    FxCommand::Del,
    FxCommand::Kil,
    FxCommand::Off,
    FxCommand::Ret,
    FxCommand::Cha,
];

impl FxCommand {
    /// The setting this command locks on a track, if it's one in the table.
    pub fn setting_on(self, track: &Track) -> Option<SettingId> {
        let voice = |setting| match track.machine {
            Machine::Plaits if track.is_sound() => SettingId::voice(track.synth_voice, setting),
            _ => None,
        };
        match self {
            Self::Vol => SettingId::track(0),
            Self::Pan => SettingId::track(1),
            Self::Cut => SettingId::track(2),
            Self::Res => SettingId::track(3),
            Self::SendA => SettingId::track(4),
            Self::SendB => SettingId::track(5),
            Self::Har => voice(0),
            Self::Tim => voice(1),
            Self::Mor => voice(2),
            Self::Acc => voice(3),
            Self::Slot { slot, param } => {
                let slot = usize::from(slot);
                let effect = Effect::of(*track.inserts.get(slot)?)?;
                SettingId::slot(slot, effect, usize::from(param))
            }
            Self::Del | Self::Kil | Self::Off | Self::Ret | Self::Cha => None,
        }
    }

    /// Its label in the grid, on this track: what it locks there, or its name if it locks
    /// nothing there. A slot's setting with nothing in its place shows its slot's digit.
    pub fn label_on(self, track: &Track) -> String {
        match (self.setting_on(track).and_then(SettingId::label), self) {
            (Some(label), _) => label,
            (None, Self::Slot { slot, .. }) => format!("{} ---", slot + 1),
            (None, _) => self.name().to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::insert::{DistortType, Insert};

    fn labels(ids: impl Iterator<Item = SettingId>) -> Vec<String> {
        ids.filter_map(SettingId::label).collect()
    }

    #[test]
    fn every_place_names_the_setting_it_holds() {
        for id in SettingId::all() {
            let back = match id.owner() {
                (Owner::Track, at) => SettingId::track(at),
                (Owner::Voice(voice), at) => SettingId::voice(voice, at),
                (Owner::Slot { slot, effect }, at) => SettingId::slot(slot, effect, at),
                (Owner::Lfo(lfo), at) => SettingId::lfo(lfo, at),
            };
            assert_eq!(back, Some(id), "{}", id.key());
        }
        assert_eq!(SettingId::all().count(), SettingId::COUNT);
    }

    #[test]
    fn every_key_is_unique_and_finds_its_setting() {
        let mut seen = HashSet::new();
        for id in SettingId::all() {
            assert!(seen.insert(id.key()), "{} twice", id.key());
            assert_eq!(SettingId::from_key(&id.key()), Some(id));
        }
        assert_eq!(
            SettingId::from_key("2.delay.feedback"),
            SettingId::slot(1, Effect::Delay, 1)
        );
        assert_eq!(
            SettingId::from_key("string.decay"),
            SettingId::voice(SynthVoice::String, 2)
        );
    }

    #[test]
    fn every_label_is_three_or_four_letters_after_a_slots_digit() {
        for id in SettingId::all() {
            let Some(label) = id.label() else { continue };
            let letters = match id.owner().0 {
                Owner::Slot { slot, .. } => {
                    assert!(label.starts_with(&(slot + 1).to_string()), "{label}");
                    &label[1..]
                }
                _ => &label[..],
            };
            assert!(
                (3..=4).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_uppercase()),
                "{label}"
            );
        }
        for event in EVENTS {
            assert_eq!(event.name().len(), 3);
        }
    }

    #[test]
    fn every_setting_has_a_name_and_a_group() {
        for id in SettingId::all() {
            assert!(!id.name().is_empty(), "{}", id.key());
            assert!(!id.group().is_empty(), "{}", id.key());
        }
    }

    #[test]
    fn no_two_labels_on_a_track_match() {
        let track_and_events = || {
            labels((0..TRACK.len()).filter_map(SettingId::track))
                .into_iter()
                .chain(EVENTS.map(|event| event.name().to_owned()))
        };
        for voice in SynthVoice::ALL {
            let mut seen = HashSet::new();
            let voice_labels = labels(
                (0..voice_entries(voice).len()).filter_map(|at| SettingId::voice(voice, at)),
            );
            for label in track_and_events().chain(voice_labels) {
                assert!(seen.insert(label.clone()), "{label} twice on {voice:?}");
            }
        }
        for effect in Effect::ALL {
            let mut seen = HashSet::new();
            for at in 0..effect.params().len() {
                let label = SettingId::slot(0, effect, at).unwrap().label().unwrap();
                assert!(seen.insert(label.clone()), "{label} twice on {effect:?}");
            }
        }
    }

    #[test]
    fn a_voices_defaults_are_its_own() {
        let six_op = |at| SettingId::voice(SynthVoice::SixOp, at).unwrap().entry();
        assert_eq!(PATCH_NAMES[usize::from(six_op(0).default)], "E.PIANO 1");
        assert!(matches!(six_op(0).kind, Kind::List { choices, .. } if choices.len() == 96));
        assert_eq!(six_op(1).default, 0x80, "Bright as the patch was made");
        let string = SettingId::voice(SynthVoice::String, 0).unwrap().entry();
        assert_eq!(string.default, 0x40);
        assert_eq!(
            SettingId::voice(SynthVoice::Kick, 3)
                .unwrap()
                .entry()
                .default,
            DEFAULT_ACCENT
        );
        assert_eq!(SettingId::voice(SynthVoice::VirtualAnalog, 3), None);
    }

    #[test]
    fn commands_show_the_label_of_what_they_lock_on_the_track() {
        let mut track = Track {
            machine: Machine::Plaits,
            synth_voice: SynthVoice::Kick,
            ..Track::default()
        };
        assert_eq!(FxCommand::Cut.label_on(&track), "CUT");
        assert_eq!(FxCommand::Har.label_on(&track), "PNCH");
        assert_eq!(FxCommand::Acc.label_on(&track), "ACC");
        assert_eq!(FxCommand::Del.label_on(&track), "DEL");
        track.synth_voice = SynthVoice::VirtualAnalog;
        assert_eq!(
            FxCommand::Acc.label_on(&track),
            "ACC",
            "nothing to lock on VA"
        );
        track.inserts[1] = Insert::ALL[2];
        assert_eq!(
            FxCommand::Slot { slot: 1, param: 1 }.label_on(&track),
            "2FDBK"
        );
        assert_eq!(
            FxCommand::Slot { slot: 0, param: 1 }.label_on(&track),
            "1 ---"
        );
        track.inserts[0] = Insert::ALL[1];
        let amount = FxCommand::Slot { slot: 0, param: 1 };
        assert_eq!(amount.label_on(&track), "1AMNT");
        if let Insert::Distort(distort) = &mut track.inserts[0] {
            distort.kind = DistortType::Crush;
        }
        assert_eq!(amount.label_on(&track), "1AMNT", "Bits is the same setting");
        let samples = Track::default();
        assert_eq!(FxCommand::Har.label_on(&samples), "HAR");
    }
}
