//! Where each setting sits in the table, and whose it is.

use super::mods::{ACCUMULATION, ARPEGGIO, MACRO, MACRO_NAMES, MOD, MOD_OWN};
use super::sampler::{EARLY_SAMPLER, SAMPLER};
use super::{
    EARLY_LANE, Entry, LANE, LATER_TRACK, LATER_TRACK_GROUPS, SLOT_OWN, TRACK, TRACK_GROUPS,
    TRACK_SETTINGS, voice_entries, voice_key,
};
use crate::constants::FX_SLOTS;
use crate::insert::{EFFECTS, EffectKind, INSERTS};
use crate::machine::SynthVoice;
use crate::macros::MACROS;
use crate::modulation::MOD_SLOTS;

const fn voices_size() -> usize {
    let mut total = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        total += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    total
}

/// Where each voice's settings start among the voices', by the voice's number, worked out as the
/// program is built. Searched for at run time, it went through every voice before, and `owner`
/// did that for each voice in turn, which came to a fifth of a stopped song's time.
const VOICE_STARTS: [usize; SynthVoice::ALL.len()] = {
    let mut starts = [0; SynthVoice::ALL.len()];
    let mut start = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        starts[SynthVoice::ALL[at] as usize] = start;
        start += voice_entries(SynthVoice::ALL[at]).len();
        at += 1;
    }
    starts
};

/// Where a voice's settings start among the voices'.
const fn voice_start(voice: SynthVoice) -> usize {
    VOICE_STARTS[voice as usize]
}

/// Whose each of the voices' settings is, as a place in `SynthVoice::ALL`, so `owner` finds it at
/// once rather than going through the voices.
const VOICE_OF: [u8; voices_size()] = {
    let mut owners = [0; voices_size()];
    let mut start = 0;
    let mut at = 0;
    while at < SynthVoice::ALL.len() {
        let end = start + voice_entries(SynthVoice::ALL[at]).len();
        while start < end {
            owners[start] = at as u8;
            start += 1;
        }
        at += 1;
    }
    owners
};

/// Where each effect's settings start in a slot's, by the effect's number, worked out as the
/// program is built.
const EFFECT_STARTS: [usize; EffectKind::COUNT] = {
    let mut starts = [0; EffectKind::COUNT];
    let mut start = 0;
    let mut at = 0;
    while at < EffectKind::COUNT {
        starts[at] = start;
        start += EFFECTS[at].params.len();
        at += 1;
    }
    starts
};

/// Where an effect's settings start in a slot's.
const fn effect_start(effect: EffectKind) -> usize {
    EFFECT_STARTS[effect as usize]
}

const VOICES_START: usize = TRACK.len();
const SLOTS_START: usize = VOICES_START + voices_size();
/// The places a slot's settings take: one for each setting of each effect it could hold.
const SLOT_SIZE: usize = {
    let last = EffectKind::COUNT - 1;
    EFFECT_STARTS[last] + EFFECTS[last].params.len()
};
const MODS_START: usize = SLOTS_START + INSERTS * SLOT_SIZE;
const BYPASS_START: usize = MODS_START + MOD_SLOTS * MOD.len();
const SAMPLER_START: usize = BYPASS_START + INSERTS * SLOT_OWN.len();
const MACROS_START: usize = SAMPLER_START + EARLY_SAMPLER;
const MOD_OWN_START: usize = MACROS_START + MACROS;
const ACCUMULATION_START: usize = MOD_OWN_START + MOD_SLOTS * MOD_OWN.len();
const ARPEGGIO_START: usize = ACCUMULATION_START + ACCUMULATION.len();
const LANES_START: usize = ARPEGGIO_START + ARPEGGIO.len();
const LATER_TRACK_START: usize = LANES_START + FX_SLOTS * EARLY_LANE;
/// Each FX column's lane's settings added after the track's own, `Start` (decision 401).
const LATER_LANE_START: usize = LATER_TRACK_START + LATER_TRACK.len();
/// The sampler's settings added after the lanes', `Reverse` (decision 412), so none moves.
const LATER_SAMPLER_START: usize = LATER_LANE_START + FX_SLOTS * (LANE.len() - EARLY_LANE);

/// What a setting belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Track,
    Voice(SynthVoice),
    Slot {
        slot: usize,
        effect: EffectKind,
    },
    /// One of the track's mod slots, by its place, whatever its type.
    Mod(usize),
    /// A slot's bypass, which is the slot's whatever effect is in it.
    Bypass(usize),
    /// The sampler machine's.
    Sampler,
    /// One of the track's eight macros, by its place.
    Macro(usize),
    /// One of a mod slot's type's own settings, by the slot's place, which macros and modulation
    /// move, and steps lock where it's labelled.
    ModOwn(usize),
    /// The accumulator's and the arpeggio's, which only macros and modulation move.
    Accumulation,
    Arpeggio,
    /// One of the track's FX columns' lanes, by the column's place, which move every phrase's lane
    /// on that column from its own.
    Lane(usize),
}

/// A setting's place in the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingId(u16);

/// How many faders a track has.
pub const FADER_COUNT: usize = 4;

/// A track's faders, in the mixer's order: the settings a step's `LEV`, `PAN`, `SNA` and `SNB` set
/// absolutely, which the mixer, the Output device and the Roto show as the steps move them, and
/// which a hand takes over from the steps.
pub const FADERS: [SettingId; FADER_COUNT] = [
    SettingId::LEVEL,
    SettingId::PAN,
    SettingId::SEND_A,
    SettingId::SEND_B,
];

impl SettingId {
    pub const VOL: Self = Self(0);
    pub const PAN: Self = Self(1);
    pub const CUT: Self = Self(2);
    pub const RES: Self = Self(3);
    pub const SEND_A: Self = Self(4);
    pub const SEND_B: Self = Self(5);
    /// In semitones, from the track's own pitch.
    pub const PITCH: Self = Self(6);
    /// The filter's mode, a place in [`FilterMode::ALL`](crate::song::FilterMode::ALL).
    pub const FILTER_MODE: Self = Self(7);
    /// How much of its last pulse a note lasts, as [`gate_share`](super::gate_share) gives it.
    pub const GATE_LENGTH: Self = Self(8);
    /// How far a note's length is moved up or down from its own, as [`gate_delta`](super::gate_delta) gives it.
    pub const GATE_DELTA: Self = Self(9);
    /// The groove a step plays with: `0` for the project's, then a place in
    /// [`GROOVE_NAMES`](crate::groove::GROOVE_NAMES) after it.
    pub const GROOVE: Self = Self(10);
    /// How long a step is: a place in [`Speed::ALL`](crate::speed::Speed::ALL).
    pub const SPEED: Self = Self(11);
    /// How long a tie glides into the next note, as [`glide_ms`](super::glide_ms) gives it.
    pub const GLIDE: Self = Self(12);
    /// How far a note's velocity is moved up or down from its own, as [`velocity_delta`](super::velocity_delta) gives it.
    pub const VELOCITY_DELTA: Self = Self(13);
    /// The track's fader, its `Level`, on the level scale.
    pub const LEVEL: Self = Self(14);
    /// How far a row's nudge is brought toward the grid as it plays, as [`quantised_nudge`](super::quantised_nudge) gives
    /// it.
    pub const QUANTISE: Self = Self(15);
    /// Whether a synth's oscillators start again on each note: a place in
    /// [`Phase::ALL`](crate::song::Phase::ALL).
    pub const PHASE: Self = Self(LATER_TRACK_START as u16);
    /// How a track's notes join: a place in
    /// [`VoiceMode::ALL`](crate::song::VoiceMode::ALL).
    pub const VOICE_MODE: Self = Self(LATER_TRACK_START as u16 + 1);
    /// Whether the filter's cutoff follows the note's pitch: `0` off, `1` on.
    pub const KEY_TRACK: Self = Self(LATER_TRACK_START as u16 + 2);
    /// Which notes `Glide` slides into: a place in
    /// [`GlideMode::ALL`](crate::song::GlideMode::ALL).
    pub const GLIDE_MODE: Self = Self(LATER_TRACK_START as u16 + 3);
    /// The chance each step plays any row of its phrase instead of the one its order walks to, from
    /// none at `00` to every step at `FF`.
    pub const JUMP: Self = Self(LATER_TRACK_START as u16 + 4);
    /// How hard the sound is pushed into the filter.
    pub const DRIVE: Self = Self(LATER_TRACK_START as u16 + 5);
    /// How far a clean filter's ringing peak starts higher and falls in pitch.
    pub const PUNCH: Self = Self(LATER_TRACK_START as u16 + 6);
    /// The utility high-pass after the filter, off at `00`.
    pub const LOW_CUT: Self = Self(LATER_TRACK_START as u16 + 7);
    /// The utility low-pass after the filter, off at `FF`.
    pub const HIGH_CUT: Self = Self(LATER_TRACK_START as u16 + 8);
    /// The filter's type: a place in [`FilterType::ALL`](crate::song::FilterType::ALL).
    pub const FILTER_TYPE: Self = Self(LATER_TRACK_START as u16 + 9);
    /// Whether the filter's `Res` stops short of ringing by itself: `0` off, `1` on.
    pub const SAFE: Self = Self(LATER_TRACK_START as u16 + 10);
    /// How far through the list of orders from its phrase's own a track's phrases play in, `80`
    /// as written (decision 400).
    pub const ORDER: Self = Self(LATER_TRACK_START as u16 + 11);
    /// Whether a track's phrases' walks go the other way up.
    pub const REVERSE: Self = Self(LATER_TRACK_START as u16 + 12);
    /// The first of the order's numbers, `00` for the phrase's own.
    pub const ORDER_X: Self = Self(LATER_TRACK_START as u16 + 13);
    /// The second of the order's numbers, `00` for the phrase's own.
    pub const ORDER_Y: Self = Self(LATER_TRACK_START as u16 + 14);
    /// How many rows more or fewer than its loop's a phrase's order walks over, `80` for none
    /// (decision 401).
    pub const PHRASE_LENGTH: Self = Self(LATER_TRACK_START as u16 + 15);
    /// The row the window a phrase's order walks over starts on, rows on from its loop's first.
    pub const PHRASE_START: Self = Self(LATER_TRACK_START as u16 + 16);
    /// How far every row's chance to play is moved up or down, `80` none, `00` never and `FF`
    /// always, as [`moved_chance`](super::moved_chance) gives it (decision 405).
    pub const CHANCE_DELTA: Self = Self(LATER_TRACK_START as u16 + 17);
    /// What a row's chance decides: `0` the whole row, `1` each of its pulses and ratchets
    /// (decision 406).
    pub const CHANCE_TARGET: Self = Self(LATER_TRACK_START as u16 + 18);
    /// Whether a turn of the phrase's window or a lane's `Length`, `Rate` or `Start` waits for
    /// its loop to come round (decision 402).
    pub const CUE: Self = Self(LATER_TRACK_START as u16 + 19);
    /// How many steps a track's dice loop over, `00` off, as [`crate::dice::DICE_LOOP_LENGTHS`]
    /// has them (decision 410).
    pub const DICE_LOOP: Self = Self(LATER_TRACK_START as u16 + 20);
    /// The chance a step of a track's dice loop rolls afresh as it comes round, `00` locked.
    pub const DICE_MUTATE: Self = Self(LATER_TRACK_START as u16 + 21);
    /// The chance a step takes the dice of any step of its track's dice loop (decision 411).
    pub const DICE_JUMP: Self = Self(LATER_TRACK_START as u16 + 22);
    /// How late the second step of each pair of a track's lands, on top of its groove, from
    /// 50.0% at `00` to 75.0% at `FA`, as [`crate::groove::swing_tenths`] reads it (decision 420).
    pub const SWING: Self = Self(LATER_TRACK_START as u16 + 23);
    /// Which of the bank's entries a sampler plays: `00` none, or an entry's number.
    pub const SAMPLE: Self = Self(SAMPLER_START as u16);
    /// Which of its entry's slices a sampler plays, by number, clamped to the last.
    pub const SLICE: Self = Self(SAMPLER_START as u16 + 1);
    /// Where in its slice a sampler's note starts, as a share of the whole.
    pub const SAMPLE_START: Self = Self(SAMPLER_START as u16 + 2);
    /// How much of its slice a sampler's note plays from its start, as a share of the whole.
    pub const SAMPLE_LENGTH: Self = Self(SAMPLER_START as u16 + 3);
    /// How a sampler's note plays: a place in [`PLAY_CHOICES`](super::PLAY_CHOICES).
    pub const PLAY: Self = Self(SAMPLER_START as u16 + 4);
    /// Whether a sampler's note plays its part of the sample the other way from the sample's own:
    /// `0` as the sample plays, `1` turned round (decision 412).
    pub const SAMPLE_REVERSE: Self = Self(LATER_SAMPLER_START as u16);

    /// How many settings there are.
    pub const COUNT: usize = LATER_SAMPLER_START + SAMPLER.len() - EARLY_SAMPLER;

    /// Every setting, in the table's order.
    pub fn all() -> impl Iterator<Item = Self> {
        (0..Self::COUNT as u16).map(Self)
    }

    /// Its place, for an array of every setting.
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Whether it's one of a mod slot's own settings, as [`SettingId::owner`] has it, which the
    /// engine asks of each mod slot's destination at every update.
    #[inline]
    pub const fn is_mod_own(self) -> bool {
        let at = self.index();
        at >= MOD_OWN_START && at < ACCUMULATION_START
    }

    /// Its place among the track's faders, [`FADERS`], if it's one.
    pub fn fader(self) -> Option<usize> {
        FADERS.iter().position(|fader| *fader == self)
    }

    /// One of the track's own, by its place among them.
    pub const fn track(setting: usize) -> Option<Self> {
        if setting < TRACK.len() {
            Some(Self(setting as u16))
        } else if setting < TRACK_SETTINGS {
            Some(Self((LATER_TRACK_START + setting - TRACK.len()) as u16))
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
    pub const fn slot(slot: usize, effect: EffectKind, setting: usize) -> Option<Self> {
        if slot < INSERTS && setting < effect.params().len() {
            Some(Self(
                (SLOTS_START + slot * SLOT_SIZE + effect_start(effect) + setting) as u16,
            ))
        } else {
            None
        }
    }

    /// One of a mod slot's settings: 0 is depth, 1 is phase, 2 is polarity.
    pub const fn modulator(slot: usize, setting: usize) -> Option<Self> {
        if slot < MOD_SLOTS && setting < MOD.len() {
            Some(Self((MODS_START + slot * MOD.len() + setting) as u16))
        } else {
            None
        }
    }

    /// One of the sampler's settings, by its place among them: 0 is `Sample`.
    pub const fn sampler(setting: usize) -> Option<Self> {
        if setting < EARLY_SAMPLER {
            Some(Self((SAMPLER_START + setting) as u16))
        } else if setting < SAMPLER.len() {
            Some(Self((LATER_SAMPLER_START + setting - EARLY_SAMPLER) as u16))
        } else {
            None
        }
    }

    /// One of the track's macros, by its place, from 0 to 7.
    pub const fn macro_at(at: usize) -> Option<Self> {
        if at < MACROS {
            Some(Self((MACROS_START + at) as u16))
        } else {
            None
        }
    }

    /// One of a mod slot's type's own settings, by its place among them: 0 is `Attack`.
    pub const fn mod_own(slot: usize, setting: usize) -> Option<Self> {
        if slot < MOD_SLOTS && setting < MOD_OWN.len() {
            Some(Self(
                (MOD_OWN_START + slot * MOD_OWN.len() + setting) as u16,
            ))
        } else {
            None
        }
    }

    /// One of the accumulator's: 0 is `Up`, 1 is `Down`.
    pub const fn accumulation(setting: usize) -> Option<Self> {
        if setting < ACCUMULATION.len() {
            Some(Self((ACCUMULATION_START + setting) as u16))
        } else {
            None
        }
    }

    /// One of the arpeggio's: 0 is `Rate`, 1 is `Octaves`.
    pub const fn arpeggio(setting: usize) -> Option<Self> {
        if setting < ARPEGGIO.len() {
            Some(Self((ARPEGGIO_START + setting) as u16))
        } else {
            None
        }
    }

    /// One of an FX column's lane's: 0 is `Length`, 1 is `Rate`, 2 is `Start`, whose places come
    /// after the track's own added later.
    pub const fn lane(slot: usize, setting: usize) -> Option<Self> {
        if slot >= FX_SLOTS || setting >= LANE.len() {
            None
        } else if setting < EARLY_LANE {
            Some(Self((LANES_START + slot * EARLY_LANE + setting) as u16))
        } else {
            let later = LANE.len() - EARLY_LANE;
            Some(Self(
                (LATER_LANE_START + slot * later + setting - EARLY_LANE) as u16,
            ))
        }
    }

    /// Every FX column's lane's settings, in order.
    pub(crate) fn lanes() -> impl Iterator<Item = Self> {
        (0..FX_SLOTS).flat_map(|slot| (0..LANE.len()).filter_map(move |at| Self::lane(slot, at)))
    }

    /// Which macro it is, if it's one.
    pub const fn macro_place(self) -> Option<usize> {
        let at = self.index();
        if at >= MACROS_START && at < MACROS_START + MACROS {
            Some(at - MACROS_START)
        } else {
            None
        }
    }

    /// A slot's bypass: `00` heard, `01` switched out.
    pub const fn bypass(slot: usize) -> Option<Self> {
        if slot < INSERTS {
            Some(Self((BYPASS_START + slot * SLOT_OWN.len()) as u16))
        } else {
            None
        }
    }

    /// What it belongs to, and its place among that owner's settings.
    pub fn owner(self) -> (Owner, usize) {
        let at = self.index();
        if at >= LATER_SAMPLER_START {
            return (Owner::Sampler, EARLY_SAMPLER + at - LATER_SAMPLER_START);
        }
        if at >= LATER_LANE_START {
            let (within, later) = (at - LATER_LANE_START, LANE.len() - EARLY_LANE);
            return (Owner::Lane(within / later), EARLY_LANE + within % later);
        }
        if at >= LATER_TRACK_START {
            return (Owner::Track, TRACK.len() + at - LATER_TRACK_START);
        }
        if at >= LANES_START {
            let within = at - LANES_START;
            return (Owner::Lane(within / EARLY_LANE), within % EARLY_LANE);
        }
        if at >= ARPEGGIO_START {
            return (Owner::Arpeggio, at - ARPEGGIO_START);
        }
        if at >= ACCUMULATION_START {
            return (Owner::Accumulation, at - ACCUMULATION_START);
        }
        if at >= MOD_OWN_START {
            let within = at - MOD_OWN_START;
            return (
                Owner::ModOwn(within / MOD_OWN.len()),
                within % MOD_OWN.len(),
            );
        }
        if at >= MACROS_START {
            return (Owner::Macro(at - MACROS_START), 0);
        }
        if at >= SAMPLER_START {
            return (Owner::Sampler, at - SAMPLER_START);
        }
        if at >= BYPASS_START {
            let within = at - BYPASS_START;
            return (
                Owner::Bypass(within / SLOT_OWN.len()),
                within % SLOT_OWN.len(),
            );
        }
        if at < VOICES_START {
            return (Owner::Track, at);
        }
        if at < SLOTS_START {
            let within = at - VOICES_START;
            let voice = SynthVoice::ALL[VOICE_OF[within] as usize];
            return (Owner::Voice(voice), within - voice_start(voice));
        }
        if at < MODS_START {
            let within = at - SLOTS_START;
            let (slot, within) = (within / SLOT_SIZE, within % SLOT_SIZE);
            for effect in EffectKind::ALL {
                let start = effect_start(effect);
                if within < start + effect.params().len() {
                    return (Owner::Slot { slot, effect }, within - start);
                }
            }
        }
        let within = at - MODS_START;
        (Owner::Mod(within / MOD.len()), within % MOD.len())
    }

    /// What its owner declares.
    pub fn entry(self) -> Entry {
        match self.owner() {
            (Owner::Track, at) if at < TRACK.len() => TRACK[at],
            (Owner::Track, at) => LATER_TRACK[at - TRACK.len()],
            (Owner::Voice(voice), at) => voice_entries(voice)[at],
            (Owner::Slot { effect, .. }, at) => effect.params()[at],
            (Owner::Mod(_), at) => MOD[at],
            (Owner::Bypass(_), at) => SLOT_OWN[at],
            (Owner::Sampler, at) => SAMPLER[at],
            (Owner::Macro(at), _) => Entry {
                name: MACRO_NAMES[at],
                ..MACRO
            },
            (Owner::ModOwn(_), at) => MOD_OWN[at],
            (Owner::Accumulation, at) => ACCUMULATION[at],
            (Owner::Arpeggio, at) => ARPEGGIO[at],
            (Owner::Lane(_), at) => LANE[at],
        }
    }

    /// The name songs store it by, which never changes, like `cut`, `string.decay`,
    /// `2.delay.feedback` or `mod4.depth`.
    pub fn key(self) -> String {
        let key = self.entry().key;
        match self.owner().0 {
            Owner::Track => key.to_owned(),
            Owner::Voice(voice) => format!("{}.{key}", voice_key(voice)),
            Owner::Slot { slot, effect } => format!("{}.{}.{key}", slot + 1, effect.key()),
            Owner::Mod(slot) | Owner::ModOwn(slot) => format!("mod{}.{key}", slot + 1),
            Owner::Bypass(slot) => format!("{}.{key}", slot + 1),
            Owner::Sampler => format!("sampler.{key}"),
            Owner::Macro(at) => format!("{key}{}", at + 1),
            Owner::Accumulation => format!("acu.{key}"),
            Owner::Arpeggio => format!("arp.{key}"),
            Owner::Lane(slot) => format!("lane{}.{key}", slot + 1),
        }
    }

    /// The setting a stored key names.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::all().find(|id| id.key() == key)
    }

    /// Its label in the grid, with its slot's digit before an effect's and its mod slot's number
    /// after a modulator's, if steps can lock it.
    pub fn label(self) -> Option<String> {
        let label = self.entry().label?;
        Some(match self.owner().0 {
            Owner::Slot { slot, .. } | Owner::Bypass(slot) => format!("{}{label}", slot + 1),
            Owner::Mod(slot) | Owner::ModOwn(slot) | Owner::Macro(slot) | Owner::Lane(slot) => {
                format!("{label}{}", slot + 1)
            }
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
            (Owner::Track, at) if at < TRACK.len() => TRACK_GROUPS[at].to_owned(),
            (Owner::Track, at) => LATER_TRACK_GROUPS[at - TRACK.len()].to_owned(),
            (Owner::Voice(voice), _) => voice.name().to_owned(),
            (Owner::Slot { slot, effect }, _) => format!("FX {} {}", slot + 1, effect.name()),
            (Owner::Mod(slot) | Owner::ModOwn(slot), _) => format!("Mod {}", slot + 1),
            (Owner::Bypass(slot), _) => format!("FX {}", slot + 1),
            (Owner::Sampler, _) => "Sampler".to_owned(),
            (Owner::Macro(_), _) => "Macros".to_owned(),
            (Owner::Accumulation, _) => "Accumulator".to_owned(),
            (Owner::Arpeggio, _) => "Arpeggio".to_owned(),
            (Owner::Lane(slot), _) => format!("FX{} lane", slot + 1),
        }
    }
}

/// How many words a [`SettingSet`] takes, a bit for each setting.
const SET_WORDS: usize = SettingId::COUNT.div_ceil(64);

/// Some settings, a bit for each, which needs no room beyond its own, so the audio thread can make
/// and read one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingSet([u64; SET_WORDS]);

impl Default for SettingSet {
    fn default() -> Self {
        Self([0; SET_WORDS])
    }
}

impl SettingSet {
    pub(crate) fn insert(&mut self, id: SettingId) {
        self.0[id.index() / 64] |= 1 << (id.index() % 64);
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, id: SettingId) -> bool {
        self.0[id.index() / 64] & 1 << (id.index() % 64) != 0
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    /// Put in each of some settings.
    pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = SettingId>) {
        for id in ids {
            self.insert(id);
        }
    }

    /// Each setting in it, in the table's order.
    pub fn iter(&self) -> impl Iterator<Item = SettingId> + '_ {
        self.0.iter().enumerate().flat_map(|(at, word)| {
            let mut bits = *word;
            std::iter::from_fn(move || {
                let bit = bits.trailing_zeros() as usize;
                bits &= bits.checked_sub(1)?;
                Some(SettingId((at * 64 + bit) as u16))
            })
        })
    }
}

/// Songs store a setting by its key, which never changes.
impl serde::Serialize for SettingId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> serde::Deserialize<'de> for SettingId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_key(&key)
            .ok_or_else(|| serde::de::Error::custom(format!("there's no setting called {key}")))
    }
}
