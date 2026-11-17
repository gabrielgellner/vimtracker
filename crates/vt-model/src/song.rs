//! The song and everything it refers to by ID.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::accumulator::Accumulation;
use crate::action::TrackControl;
use crate::arpeggio::Arpeggio;
use crate::bank::{Bank, BankEntry, ContentHash};
use crate::constants::{
    BAR_STEPS, BEAT_STEPS, DB_PER_LEVEL, DB_PER_TRIM, DEFAULT_MILLIBPM, DEFAULT_PHRASE_STEPS,
    FIXED_TRACKS, FORMAT_VERSION, FX_SLOTS, MAX_GROUPS, MAX_LEVEL, MAX_TRACK_RESET, UNITY_LEVEL,
    UNITY_TRIM,
};
use crate::groove::{Groove, GrooveId};
use crate::insert::{INSERTS, Insert};
use crate::lane::LaneMove;
use crate::macros::{MACROS, Macro, MacroMap, MacroTarget};
use crate::modulation::{MOD_SLOTS, ModSlot, default_mods};
use crate::order::{Deal, Dealer};
use crate::scale::Scale;
use crate::settings::{Owner, SettingId};
use crate::shape::Source;
use crate::speed::Speed;
use crate::step::{FxCommand, FxSlot, Step};
use crate::view::ViewState;

pub use crate::filter::{FILTER_MODE_NAMES, FilterMode, FilterType};
pub use crate::machine::{
    ACID_GATE, DEFAULT_ACCENT, FULL_GATE, GLIDE_MODE_NAMES, GlideMode, Machine, PHASE_NAMES, Phase,
    Role, SIX_OP_PATCHES, SynthVoice, VOICE_MODE_NAMES, VoiceMode,
};
pub use crate::phrase::{Lane, Phrase, Visit};

mod tracks;

pub use tracks::Tracks;

/// The gain of a level value, on an even scale in dB like a fader's. `00` is silent, nothing goes
/// past `MAX_LEVEL`, and the value can be between steps, as an LFO moves it.
pub fn level_gain(level: f32) -> f32 {
    if level <= 0.0 {
        return 0.0;
    }
    let level = level.min(f32::from(MAX_LEVEL));
    10.0_f32.powf((level - f32::from(UNITY_LEVEL)) * DB_PER_LEVEL / 20.0)
}

/// What a trim value does in dB, a quarter of a dB a step either side of `80`, or `None` for `00`,
/// which is off.
pub fn trim_db(trim: u8) -> Option<f32> {
    (trim > 0).then(|| (f32::from(trim) - f32::from(UNITY_TRIM)) * DB_PER_TRIM)
}

/// The gain of a trim value: exactly 1 at `80`, and 0 at `00`.
pub fn trim_gain(trim: u8) -> f32 {
    trim_db(trim).map_or(0.0, |db| 10.0_f32.powf(db / 20.0))
}

/// A trim value as the status line shows it: `+12.0 dB`, `-0.25 dB`, or `-inf dB` for off. Its
/// quarters are shown, as each step is one.
pub fn trim_text(trim: u8) -> String {
    match trim_db(trim) {
        None => "-inf dB".to_owned(),
        Some(db) if (db * 2.0).fract() == 0.0 => format!("{db:+.1} dB"),
        Some(db) => format!("{db:+.2} dB"),
    }
}

/// What a track is. Only `Sound` tracks have a machine, and the rest are fixed, after them, in this
/// order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    #[default]
    Sound,
    /// A group, from 1 to `MAX_GROUPS`, which sums the sound tracks put in it and plays the sum
    /// through its own slots, level and pan to the master, as Ableton's group tracks do. It sends
    /// to the returns as a sound track does.
    Group(u8),
    /// A return, which other tracks send to.
    ReturnA,
    ReturnB,
    /// What everything ends up in.
    Master,
}

impl TrackKind {
    /// The fixed tracks, in order: the groups, the returns and the master.
    pub(crate) const FIXED: [Self; FIXED_TRACKS] = {
        let mut kinds = [Self::Master; FIXED_TRACKS];
        let mut group = 0;
        while group < MAX_GROUPS {
            kinds[group] = Self::Group(group as u8 + 1);
            group += 1;
        }
        kinds[MAX_GROUPS] = Self::ReturnA;
        kinds[MAX_GROUPS + 1] = Self::ReturnB;
        kinds
    };

    /// The short name of a fixed track.
    pub fn letter(self) -> String {
        match self {
            Self::Sound => String::new(),
            Self::Group(group) => format!("G{group}"),
            Self::ReturnA => "A".to_owned(),
            Self::ReturnB => "B".to_owned(),
            Self::Master => "M".to_owned(),
        }
    }

    /// What a fixed track is called in full.
    pub fn title(self) -> String {
        match self {
            Self::Sound => String::new(),
            Self::Group(group) => format!("Group {group}"),
            Self::ReturnA => "Return A".to_owned(),
            Self::ReturnB => "Return B".to_owned(),
            Self::Master => "Master".to_owned(),
        }
    }

    /// Where it is among the fixed tracks, counted from the first after the sound tracks.
    pub(crate) fn fixed_place(self) -> Option<usize> {
        Self::FIXED.iter().position(|kind| *kind == self)
    }
}

/// One voice's lane through the song.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    /// Whether it plays sounds, or is one of the fixed tracks at the end.
    #[serde(default)]
    pub kind: TrackKind,
    /// Two characters, a group's up to five, or empty for none.
    #[serde(default)]
    pub name: String,
    /// What makes its sound.
    #[serde(default)]
    pub machine: Machine,
    /// What it's for, as drums or a bass, which its colour shows: `Auto` goes by its machine.
    #[serde(default, skip_serializing_if = "Role::is_auto")]
    pub role: Role,
    /// The machines' settings it has changed from their defaults, by setting. A track keeps every
    /// voice's, so switching voice and back finds the first as it was.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<SettingId, u8>,
    /// The fine parts of its own settings and its machines', where they aren't `00`, so `Cut` at
    /// `40` with `80` here is half a step above `40`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fine: BTreeMap<SettingId, u8>,
    /// Whether the track is muted, as saved with the song. While the song is open the app's live
    /// copy, `Services.live.levels`, is the truth; it's read from here when the song opens and
    /// written back when it's saved. A render of the file hears this one.
    #[serde(rename = "mute")]
    pub saved_mute: bool,
    /// Whether the track is soloed, as saved with the song. While the song is open the app's live
    /// copy, `Services.live.levels`, is the truth; it's read from here when the song opens and
    /// written back when it's saved. A render of the file hears this one.
    #[serde(rename = "solo")]
    pub saved_solo: bool,
    /// This track's own groove, instead of the project's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groove: Option<GrooveId>,
    /// The track's level, set outside its patterns. It also scales what the track sends to the
    /// returns.
    #[serde(default = "unity_level")]
    pub level: u8,
    /// What comes into it is turned up or down by, before anything else it does: a sound track's
    /// voice before its filter, and a fixed track's input before its slots. It's for gain staging,
    /// set outside its patterns and never modulated, `80` for 0 dB as [`trim_db`] has it, and
    /// `00` silences it.
    #[serde(default = "unity_trim", skip_serializing_if = "is_unity_trim")]
    pub trim: u8,
    /// What the track's held `CUT` starts from, and what its LFOs move around until a step sets it.
    #[serde(default = "open_cutoff")]
    pub cutoff: u8,
    /// Where held `RES` starts.
    #[serde(default)]
    pub resonance: u8,
    #[serde(default)]
    pub filter: FilterType,
    /// Where held `MODE` starts: how its filter type responds.
    #[serde(default)]
    pub filter_mode: FilterMode,
    /// How hard the filter is driven.
    #[serde(default)]
    pub drive: u8,
    /// How far a clean filter's cutoff and damping rise as its low-pass swings up, so a ringing
    /// peak starts higher and falls in pitch as it dies away, after Braids' SVF. `00` is off.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub punch: u8,
    /// Whether `Res` stops just short of where the filter rings by itself, after Cytomic The
    /// Drop's `SAFE`: on, its whole turn reaches only as far as its loudest peak that dies away.
    #[serde(default, skip_serializing_if = "is_off")]
    pub safe: bool,
    /// Whether its cutoff follows the note's pitch, an octave for an octave, where `C-3` leaves
    /// it as set (decision 381).
    #[serde(default, skip_serializing_if = "is_off")]
    pub key_track: bool,
    /// The utility filter after it: a high-pass, which `00` turns off.
    #[serde(default)]
    pub low_cut: u8,
    /// And a low-pass, which `FF` turns off.
    #[serde(default = "open_cutoff")]
    pub high_cut: u8,
    /// The effects after the filters, in order.
    #[serde(default)]
    pub inserts: [Insert; INSERTS],
    /// The slots switched out, to hear the track without them. They keep running underneath.
    #[serde(default, skip_serializing_if = "none_bypassed")]
    pub bypassed: [bool; INSERTS],
    /// Where held `PAN` starts.
    #[serde(default = "centre")]
    pub pan: u8,
    /// How much it sends to return `A`, and where held `SNA` starts.
    #[serde(default)]
    pub send_a: u8,
    /// How much it sends to return `B`, and where held `SNB` starts.
    #[serde(default)]
    pub send_b: u8,
    /// Where held `PTCH` starts, in semitones from `80`, as a kit sound's tune sets it.
    #[serde(default = "centre")]
    pub pitch: u8,
    /// Where held `VDLT` starts: how far its notes' velocities are moved up or down from their
    /// own, by [`velocity_delta`](crate::settings::velocity_delta), with `80` as none.
    #[serde(default = "centre", skip_serializing_if = "is_centre")]
    pub velocity_delta: u8,
    /// Where held `QNT` starts: how far its rows' nudges are brought toward the grid as they play,
    /// by [`quantised_nudge`](crate::settings::quantised_nudge), from none at `00` to all at `FF`.
    /// The nudges written stay as they are.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub quantise: u8,
    /// Where held `JMP` starts: the chance each step plays any row of its phrase instead of the one
    /// its order walks to, from none at `00` to every step at `FF` (decision 391).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub jump: u8,
    /// The choke group it's in, if any, from 1 to `CHOKE_GROUPS`: a note on it cuts off what's
    /// sounding on the other tracks in the group, as a closed hat cuts an open one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choke: Option<u8>,
    /// The group it plays through, from 1 to `MAX_GROUPS`, or `None` to go straight to the master.
    /// Only a sound track can be in one, and its sends still go from the track itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u8>,
    /// Where held `LEN` starts, its `Gate`: how much of its last pulse a note lasts, or `None`
    /// for its machine's own, which [`Track::gate_length`] gives.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_length: Option<u8>,
    /// How long each of its steps is, and where held `RATE` starts: a sixteenth unless changed.
    #[serde(default, skip_serializing_if = "is_sixteenth")]
    pub speed: Speed,
    /// How many of the song's steps its chain plays before starting again from its top, counted
    /// from where its chain last started, or `None` to play on, so a short part can be kept to a
    /// bar while the others drift. From 1 to [`MAX_TRACK_RESET`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset: Option<u16>,
    /// How long a tie glides into the next note, and where held `GLD` starts, on the `00` to `FF`
    /// scale of [`glide_ms`](crate::settings::glide_ms): `00`, unless changed, jumps, or on `Acid`
    /// slides by its own `Slide`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub glide: u8,
    /// Which notes `glide` slides into: only ties, or every note from the one sounding.
    #[serde(default, skip_serializing_if = "GlideMode::is_ties")]
    pub glide_mode: GlideMode,
    /// Whether a synth's oscillators run on from note to note or start again on each.
    #[serde(default, skip_serializing_if = "Phase::is_free")]
    pub phase: Phase,
    /// How its notes join: each struck, or tied into one whose gate is still open.
    #[serde(default, skip_serializing_if = "VoiceMode::is_mono")]
    pub voice_mode: VoiceMode,
    /// How its `ACU`s move its notes: how far, what happens at the limits, and when they start
    /// again.
    #[serde(default, skip_serializing_if = "is_default_accumulation")]
    pub accumulation: Accumulation,
    /// How its `ARP`s play: how fast, in what order, over how many octaves, and whether each note
    /// starts again.
    #[serde(default, skip_serializing_if = "is_default_arpeggio")]
    pub arpeggio: Arpeggio,
    /// What its `Length` and `Rate` for each FX column move every phrase's lane on that column by,
    /// from the lane's own. Saved only while one isn't at rest.
    #[serde(default, skip_serializing_if = "lanes_at_rest")]
    pub lanes: [LaneMove; FX_SLOTS],
    /// Its mod slots, worked out in order, each able to move only those after it.
    #[serde(default = "default_mods")]
    pub mods: [ModSlot; MOD_SLOTS],
    /// Its eight macros, each turning any of its settings at once. Saved only while any isn't as a
    /// new track's.
    #[serde(default, skip_serializing_if = "no_macros")]
    pub macros: [Macro; MACROS],
    /// The chains this track can play, numbered within it.
    #[serde(default)]
    pub chains: BTreeMap<u32, Arc<Chain>>,
    /// The phrases its chains can play, numbered within it.
    #[serde(default)]
    pub phrases: BTreeMap<u32, Arc<Phrase>>,
}

fn is_centre(value: &u8) -> bool {
    *value == centre()
}

fn is_zero(value: &u8) -> bool {
    *value == 0
}

fn is_off(value: &bool) -> bool {
    !*value
}

fn is_sixteenth(speed: &Speed) -> bool {
    *speed == Speed::Sixteenth
}

fn is_default_accumulation(accumulation: &Accumulation) -> bool {
    *accumulation == Accumulation::default()
}

fn is_default_arpeggio(arpeggio: &Arpeggio) -> bool {
    *arpeggio == Arpeggio::default()
}

fn lanes_at_rest(lanes: &[LaneMove; FX_SLOTS]) -> bool {
    lanes.iter().all(|moved| moved.at_rest())
}

fn no_macros(macros: &[Macro; MACROS]) -> bool {
    macros.iter().all(Macro::is_empty)
}

fn none_bypassed(bypassed: &[bool; INSERTS]) -> bool {
    !bypassed.contains(&true)
}

impl Track {
    /// A fixed track: named for what it is, and with no chain until one is given to it, so it
    /// doesn't change how long a section is.
    fn fixed(kind: TrackKind) -> Self {
        let mut inserts = [Insert::Empty; INSERTS];
        inserts[0] = match kind {
            TrackKind::ReturnA => Insert::RETURN_DELAY,
            TrackKind::ReturnB => Insert::RETURN_REVERB,
            TrackKind::Sound | TrackKind::Group(_) | TrackKind::Master => Insert::Empty,
        };
        Self {
            kind,
            name: kind.letter(),
            inserts,
            ..Self::default()
        }
    }

    /// Its name, or its number, as `03`, if it has none: the grids' headers and the views'
    /// titles. `index` is where it is in the song.
    pub fn label(&self, index: usize) -> String {
        if self.name.is_empty() {
            format!("{:02}", index + 1)
        } else {
            self.name.clone()
        }
    }

    /// What the status line calls it: `Track 03`, or a fixed track's title, as `Return A`.
    /// `index` is where it is in the song.
    pub fn title(&self, index: usize) -> String {
        if self.is_sound() {
            format!("Track {:02}", index + 1)
        } else {
            self.kind.title()
        }
    }

    /// Whether this track plays sounds.
    pub fn is_sound(&self) -> bool {
        self.kind == TrackKind::Sound
    }

    /// Whether this is one of the group tracks.
    pub fn is_group(&self) -> bool {
        matches!(self.kind, TrackKind::Group(_))
    }

    /// The synth voice it plays, if it plays one.
    pub fn voice(&self) -> Option<SynthVoice> {
        match self.machine {
            Machine::Plaits(voice) if self.is_sound() => Some(voice),
            _ => None,
        }
    }

    /// What a sound track is for: its own role, or its machine's while it's `Auto`. A fixed track
    /// has none, as a group, a return and the master are told by what they are.
    pub fn role(&self) -> Role {
        match self.role {
            _ if !self.is_sound() => Role::None,
            Role::Auto => self.machine.role(),
            role => role,
        }
    }

    /// How much of its last pulse a note lasts, on the `00` to `FF` scale: its own `Gate`, or its
    /// machine's if it hasn't one.
    pub fn gate_length(&self) -> u8 {
        self.gate_length
            .unwrap_or_else(|| self.machine.gate_length())
    }

    /// The bank entry its sampler plays, before any step chooses another: `00` for none.
    pub fn sample(&self) -> u8 {
        self.setting(SettingId::SAMPLE)
    }

    /// One of its machine's settings, by its place among them: a synth's, or the sampler's, whose
    /// first is `Sample`.
    pub fn machine_setting(&self, setting: usize) -> Option<SettingId> {
        match self.machine {
            Machine::Sampler if self.is_sound() => SettingId::sampler(setting),
            _ => SettingId::voice(self.voice()?, setting),
        }
    }

    /// The value of one of its machine's settings, by its place among them, for `HAR`, `TIM`,
    /// `MOR` and `ACC` to start from. A machine without that setting gives 0, which nothing reads.
    pub fn machine_value(&self, setting: usize) -> u8 {
        self.machine_setting(setting)
            .map_or(0, |id| self.setting(id))
    }

    /// Set one of its machine's settings, by its place among them, if the machine has it.
    pub fn set_machine_value(&mut self, setting: usize, value: u8) {
        if let Some(id) = self.machine_setting(setting) {
            self.set_setting(id, value);
        }
    }

    /// A setting's value on this track: what it's set to, or its default.
    pub fn setting(&self, id: SettingId) -> u8 {
        self.settings
            .get(&id)
            .copied()
            .unwrap_or_else(|| id.entry().default)
    }

    /// Set a setting, within what it takes. A setting back at its default isn't kept.
    pub fn set_setting(&mut self, id: SettingId, value: u8) {
        let entry = id.entry();
        let value = value.min(entry.kind.top());
        if value == entry.default {
            self.settings.remove(&id);
        } else {
            self.settings.insert(id, value);
        }
    }

    /// One of the values it holds that its page sets, its coarse byte where it has a fine part.
    pub fn control(&self, control: TrackControl) -> u8 {
        match control {
            TrackControl::Cutoff => self.cutoff,
            TrackControl::Resonance => self.resonance,
            TrackControl::Pan => self.pan,
            TrackControl::SendA => self.send_a,
            TrackControl::SendB => self.send_b,
            TrackControl::Drive => self.drive,
            TrackControl::Punch => self.punch,
            TrackControl::LowCut => self.low_cut,
            TrackControl::HighCut => self.high_cut,
            TrackControl::Pitch => self.pitch,
            TrackControl::VelocityDelta => self.velocity_delta,
            TrackControl::Quantise => self.quantise,
            TrackControl::Jump => self.jump,
        }
    }

    /// Set one of the values it holds that its page sets, leaving any fine part as it is. A send
    /// stops at the level scale's top.
    pub fn set_control(&mut self, control: TrackControl, value: u8) {
        let held = match control {
            TrackControl::Cutoff => &mut self.cutoff,
            TrackControl::Resonance => &mut self.resonance,
            TrackControl::Pan => &mut self.pan,
            TrackControl::SendA => &mut self.send_a,
            TrackControl::SendB => &mut self.send_b,
            TrackControl::Drive => &mut self.drive,
            TrackControl::Punch => &mut self.punch,
            TrackControl::LowCut => &mut self.low_cut,
            TrackControl::HighCut => &mut self.high_cut,
            TrackControl::Pitch => &mut self.pitch,
            TrackControl::VelocityDelta => &mut self.velocity_delta,
            TrackControl::Quantise => &mut self.quantise,
            TrackControl::Jump => &mut self.jump,
        };
        *held = match control {
            TrackControl::SendA | TrackControl::SendB => value.min(MAX_LEVEL),
            _ => value,
        };
    }

    /// A setting's fine part on this track, `00` unless it's been set.
    pub fn fine(&self, id: SettingId) -> u8 {
        self.fine.get(&id).copied().unwrap_or(0)
    }

    /// Set a setting's fine part. `00` isn't kept, and there's none at the setting's top, nor on
    /// one with no fine part, as the level scale's sends have none.
    pub fn set_fine(&mut self, id: SettingId, fine: u8) {
        if fine == 0
            || !id.entry().kind.has_fine()
            || self.coarse(id) == Some(id.entry().kind.top())
        {
            self.fine.remove(&id);
        } else {
            self.fine.insert(id, fine);
        }
    }

    /// Which macro holds a setting, and its mapping there, if one does: a setting is on one
    /// macro at most.
    pub fn macro_holding(&self, target: MacroTarget) -> Option<(usize, &MacroMap)> {
        self.macros_holding(target).next()
    }

    /// Every macro holding a setting, with its mapping there, in the macros' order. `MapMacro`
    /// keeps a setting on one macro at most, so there's one at most from the program's own edits.
    pub fn macros_holding(
        &self,
        target: MacroTarget,
    ) -> impl Iterator<Item = (usize, &MacroMap)> + '_ {
        self.macros
            .iter()
            .enumerate()
            .filter_map(move |(at, held)| {
                held.maps
                    .iter()
                    .find(|map| map.target == target)
                    .map(|map| (at, map))
            })
    }

    /// Whether any of its macros holds anything.
    #[inline]
    pub fn has_macro_maps(&self) -> bool {
        self.macros.iter().any(|held| !held.maps.is_empty())
    }

    /// The coarse value of one of its own settings or its machines', which a fine part goes with.
    pub fn coarse(&self, id: SettingId) -> Option<u8> {
        match id.owner().0 {
            Owner::Voice(_) | Owner::Sampler => Some(self.setting(id)),
            Owner::Track => match id {
                SettingId::CUT => Some(self.cutoff),
                SettingId::RES => Some(self.resonance),
                SettingId::PAN => Some(self.pan),
                SettingId::SEND_A => Some(self.send_a),
                SettingId::SEND_B => Some(self.send_b),
                _ => None,
            },
            Owner::Slot { .. }
            | Owner::Mod(_)
            | Owner::Bypass(_)
            | Owner::Macro(_)
            | Owner::ModOwn(_)
            | Owner::Accumulation
            | Owner::Arpeggio
            | Owner::Lane(_) => None,
        }
    }

    /// The total steps a chain lasts: the pulses of every row in its phrases' loops. A missing or
    /// empty chain has none, and so does one whose rows are all skipped.
    pub fn chain_steps(&self, chain: u32) -> usize {
        self.chains.get(&chain).map_or(0, |chain| {
            chain
                .entries
                .iter()
                .filter_map(|entry| self.phrases.get(&entry.phrase))
                .map(|phrase| phrase.time())
                .sum()
        })
    }

    /// Find a step within a chain, counted from where it started, looping the chain if the
    /// position is past its end. Which time round it is says how far each phrase's order has
    /// walked, as each pass of a phrase carries its walk on from the last, and deals any phrase in
    /// it whose order is dealt, from the track's dealer.
    pub fn chain_position(&self, chain: u32, step: usize, dealer: Dealer) -> Option<ChainPosition> {
        let total = self.chain_steps(chain);
        if total == 0 {
            return None;
        }
        let lap = step / total;
        let mut remaining = step % total;
        let chain = self.chains.get(&chain)?;
        for (slot, entry) in chain.entries.iter().enumerate() {
            let Some(phrase) = self.phrases.get(&entry.phrase) else {
                continue;
            };
            let time = phrase.time();
            if remaining < time {
                let deal = dealer.deal(lap as i64, slot);
                let visit = phrase.walk_at(lap, remaining, deal)?;
                return Some(ChainPosition {
                    slot,
                    phrase: entry.phrase,
                    step: visit.row,
                    pulse: visit.pulse,
                    transpose: entry.transpose,
                    time: remaining,
                    visit: visit.visit,
                    deal,
                });
            }
            remaining -= time;
        }
        None
    }

    /// Where a slot and a row of its phrase's loop fall in a chain, if the chain still has them,
    /// on a time round it: where that pass of the phrase's walk plays the row at this pulse,
    /// nearest to `near` steps into the pass, or `near` itself if the pass doesn't play the row.
    pub fn chain_place(
        &self,
        chain: u32,
        at: RowPlace,
        lap: usize,
        dealer: Dealer,
    ) -> Option<usize> {
        let entries = &self.chains.get(&chain)?.entries;
        let phrase = self.phrases.get(&entries.get(at.slot)?.phrase)?;
        if at.row >= phrase.len() {
            return None;
        }
        let before: usize = entries[..at.slot]
            .iter()
            .filter_map(|entry| self.phrases.get(&entry.phrase))
            .map(|phrase| phrase.time())
            .sum();
        let deal = dealer.deal(lap as i64, at.slot);
        let place = phrase
            .place_in_pass(at.row, at.pulse, lap, at.near, deal)
            .unwrap_or_else(|| at.near.min(phrase.time().saturating_sub(1)));
        Some(before + place)
    }

    /// The step at a position, once a chain position has been found.
    pub fn step_at(&self, position: ChainPosition) -> Option<&Step> {
        self.phrases
            .get(&position.phrase)?
            .loop_steps()
            .get(position.step)
    }

    /// Make a chain with this ID and one new phrase.
    pub fn add_chain(&mut self, chain: u32) {
        let phrase = lowest_unused(&self.phrases);
        self.phrases
            .insert(phrase, Arc::new(Phrase::new(DEFAULT_PHRASE_STEPS)));
        self.chains.insert(
            chain,
            Arc::new(Chain {
                entries: vec![ChainEntry {
                    phrase,
                    transpose: 0,
                }],
                label: String::new(),
            }),
        );
    }
}

impl Default for Track {
    fn default() -> Self {
        Self {
            kind: TrackKind::default(),
            name: String::new(),
            machine: Machine::default(),
            role: Role::Auto,
            settings: BTreeMap::new(),
            fine: BTreeMap::new(),
            saved_mute: false,
            saved_solo: false,
            groove: None,
            level: unity_level(),
            trim: unity_trim(),
            cutoff: open_cutoff(),
            resonance: 0,
            filter: FilterType::default(),
            filter_mode: FilterMode::default(),
            drive: 0,
            punch: 0,
            safe: false,
            key_track: false,
            low_cut: 0,
            high_cut: open_cutoff(),
            inserts: Default::default(),
            bypassed: [false; INSERTS],
            pan: centre(),
            send_a: 0,
            send_b: 0,
            pitch: centre(),
            velocity_delta: centre(),
            quantise: 0,
            jump: 0,
            choke: None,
            group: None,
            gate_length: None,
            speed: Speed::Sixteenth,
            reset: None,
            glide: 0,
            glide_mode: GlideMode::Ties,
            phase: Phase::Free,
            voice_mode: VoiceMode::Mono,
            accumulation: Accumulation::default(),
            arpeggio: Arpeggio::default(),
            lanes: [LaneMove::default(); FX_SLOTS],
            mods: default_mods(),
            macros: Default::default(),
            chains: BTreeMap::new(),
            phrases: BTreeMap::new(),
        }
    }
}

fn unity_level() -> u8 {
    UNITY_LEVEL
}

fn unity_trim() -> u8 {
    UNITY_TRIM
}

fn is_unity_trim(trim: &u8) -> bool {
    *trim == UNITY_TRIM
}

fn open_cutoff() -> u8 {
    0xFF
}

fn centre() -> u8 {
    0x80
}

/// What a section does to the tracks when it starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionStart {
    /// A track given a different chain starts it from its top, and one given the same keeps going.
    #[default]
    New,
    /// Every track goes back to its start, each time the section's length comes round.
    Reset,
}

/// When a launched chain starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Launch {
    /// When the track's own chain ends.
    ChainEnd,
    /// On the next step this many into the row, so every row's start is one.
    Every(NonZeroU32),
}

/// The lengths `:launch` has names for.
const LAUNCH_NAMES: [(&str, u32); 7] = [
    ("step", 1),
    ("1/4", BEAT_STEPS as u32),
    ("1/2", BEAT_STEPS as u32 * 2),
    ("1bar", BAR_STEPS as u32),
    ("2bar", BAR_STEPS as u32 * 2),
    ("4bar", BAR_STEPS as u32 * 4),
    ("8bar", BAR_STEPS as u32 * 8),
];

/// What `:launch` takes for launching when a chain ends.
const LAUNCH_CHAIN: &str = "chain";

/// Every name `:launch` takes, in order, which the command line completes from.
pub const LAUNCH_WORDS: [&str; LAUNCH_NAMES.len() + 1] = {
    let mut words = [LAUNCH_CHAIN; LAUNCH_NAMES.len() + 1];
    let mut at = 0;
    while at < LAUNCH_NAMES.len() {
        words[at] = LAUNCH_NAMES[at].0;
        at += 1;
    }
    words
};

impl Launch {
    /// Launch on a grid of this many steps, if it's 1 or more.
    pub fn every(steps: usize) -> Option<Self> {
        let steps = u32::try_from(steps).ok()?;
        NonZeroU32::new(steps).map(Self::Every)
    }

    /// The setting with this name in `:launch`.
    pub fn named(name: &str) -> Option<Self> {
        if name == LAUNCH_CHAIN {
            return Some(Self::ChainEnd);
        }
        let (_, steps) = LAUNCH_NAMES.iter().find(|(known, _)| *known == name)?;
        NonZeroU32::new(*steps).map(Self::Every)
    }

    /// When a launch starts, to finish a sentence on the status line.
    pub fn when(self) -> String {
        match self {
            Self::ChainEnd => "when its chain ends".to_owned(),
            Self::Every(_) => format!("on the {self} grid"),
        }
    }
}

impl Default for Launch {
    fn default() -> Self {
        Self::every(BAR_STEPS).unwrap_or(Self::ChainEnd)
    }
}

impl fmt::Display for Launch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self::Every(steps) = self else {
            return f.write_str("chain");
        };
        match LAUNCH_NAMES.iter().find(|(_, known)| *known == steps.get()) {
            Some((name, _)) => f.write_str(name),
            None => write!(f, "{steps}-step"),
        }
    }
}

/// A track's own `Reset` as the track page shows it: `Off`, whole bars as `:launch` names them,
/// such as `1bar`, and anything else in steps, such as `5-step`.
pub fn reset_name(reset: Option<u16>) -> String {
    match reset {
        None => "Off".to_owned(),
        Some(steps) if usize::from(steps).is_multiple_of(BAR_STEPS) => {
            format!("{}bar", usize::from(steps) / BAR_STEPS)
        }
        Some(steps) => format!("{steps}-step"),
    }
}

/// A track's own `Reset` from what's typed: `off`, a number of steps, in hex after `0x`, or of
/// bars, such as `2bar`, from 1 step to [`MAX_TRACK_RESET`]. The outer `None` is for text that
/// isn't one.
pub(crate) fn reset_named(text: &str) -> Option<Option<u16>> {
    let text = text.trim().to_ascii_lowercase();
    if text == "off" {
        return Some(None);
    }
    let steps = if let Some(bars) = text.strip_suffix("bars").or(text.strip_suffix("bar")) {
        bars.trim()
            .parse::<u32>()
            .ok()?
            .checked_mul(BAR_STEPS as u32)?
    } else if let Some(digits) = text.strip_prefix("0x") {
        u32::from_str_radix(digits, 16).ok()?
    } else {
        text.trim_end_matches("-step")
            .trim_end_matches("steps")
            .trim_end_matches("step")
            .trim()
            .parse::<u32>()
            .ok()?
    };
    let steps = u16::try_from(steps).ok()?;
    (1..=MAX_TRACK_RESET)
        .contains(&steps)
        .then_some(Some(steps))
}

/// How many times a section plays before moving on. Saved songs can't hold 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Repeat(NonZeroU16);

impl Repeat {
    /// As many times as this, from 1 to `u16::MAX`.
    pub fn new(times: i64) -> Self {
        let times = times.clamp(1, i64::from(u16::MAX)) as u16;
        Self(NonZeroU16::new(times).unwrap_or(NonZeroU16::MIN))
    }

    /// How many times it is.
    pub fn get(self) -> u16 {
        self.0.get()
    }
}

impl Default for Repeat {
    fn default() -> Self {
        Self(NonZeroU16::MIN)
    }
}

/// One row of the song, holding one chain per track.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Section {
    pub chains: Vec<Option<u32>>,
    /// How many steps it plays. If not set, the length of its longest chain.
    #[serde(default)]
    pub length: Option<usize>,
    /// How many times it plays before moving on.
    #[serde(default)]
    pub repeat: Repeat,
    /// What it does to the tracks when it starts.
    #[serde(default)]
    pub start: SectionStart,
    /// Send every track back to its start after this many steps. Without it, each loops on its own.
    #[serde(default)]
    pub reset: Option<usize>,
}

/// One phrase in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainEntry {
    pub phrase: u32,
    pub transpose: i8,
}

/// A list of phrases, each with a transpose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chain {
    pub entries: Vec<ChainEntry>,
    /// What to call it on screen, or empty for just its number.
    #[serde(default)]
    pub label: String,
}

/// A row of a chain's slot to find again after a change, with the pulse it was on and how many
/// steps into its pass it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowPlace {
    pub slot: usize,
    pub row: usize,
    pub pulse: usize,
    pub near: usize,
}

/// Where a position in a chain lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainPosition {
    pub slot: usize,
    pub phrase: u32,
    /// The row of the phrase's loop.
    pub step: usize,
    /// Which of the row's pulses, from 0 for its first.
    pub pulse: usize,
    pub transpose: i8,
    /// How many steps into the phrase's pass it is.
    pub time: usize,
    /// Which visit of the round of the phrase's order it is, and the pass's deal, for looking on
    /// in its order.
    pub visit: usize,
    pub deal: Deal,
}

impl ChainPosition {
    /// Its row, to find again after a change.
    pub fn row_place(&self) -> RowPlace {
        RowPlace {
            slot: self.slot,
            row: self.step,
            pulse: self.pulse,
            near: self.time,
        }
    }
}

fn default_limiter() -> bool {
    true
}

fn default_skip() -> u32 {
    BEAT_STEPS as u32
}

/// Everything saved in `song.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Song {
    pub format_version: u32,
    /// The project's name, which saving it to a new folder names the folder after. Empty in a song
    /// that hasn't been given one, which takes its folder's.
    #[serde(default)]
    pub name: String,
    /// Tempo, in thousandths of a BPM.
    pub millibpm: u32,
    pub seed: u64,
    /// Whether entering something in Insert moves on by the grid, `skip`, as saved with the song.
    /// Not used to play it. While the song is open the app's live copy, `Services.live.auto_jump`,
    /// is the truth; it's read from here when the song opens and written back when it's saved.
    #[serde(default, rename = "auto_jump")]
    pub saved_auto_jump: bool,
    /// The grid: how far the left and right arrows move in Insert, and auto-jump with them.
    #[serde(default = "default_skip")]
    pub skip: u32,
    /// The scale for entering and moving notes, as saved with the song. Playing reads it only for
    /// what an `ACU` moves notes through. While the song is open the app's live copy,
    /// `Services.live.scale`, is the truth, which it sends the engine as it changes; it's read from
    /// here when the song opens and written back when it's saved.
    #[serde(default, rename = "scale")]
    pub saved_scale: Scale,
    /// Whether the main output clips at full level. Off, a render keeps levels above it.
    #[serde(default = "default_limiter")]
    pub limiter: bool,
    /// The groove every track plays with unless it has its own.
    #[serde(default)]
    pub groove: GrooveId,
    /// The project's own grooves, `Custom 1` first. One not made yet plays on the grid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grooves: Vec<Groove>,
    /// When launched chains start.
    #[serde(default)]
    pub launch: Launch,
    /// The project's samples, which a sampler track's `Sample` picks from by number.
    #[serde(default, skip_serializing_if = "Bank::is_empty")]
    pub bank: Bank,
    /// Its tracks, each shared with the versions before and after that an edit didn't change it
    /// in.
    pub tracks: Tracks,
    pub sections: Vec<Section>,
    /// How the views show the song, its folded groups, as saved with the song. Not undone. While
    /// the song is open the app's live copy, `Services.folds`, is the truth; it's read from here
    /// when the song opens and written back when it's saved.
    #[serde(default, rename = "view", skip_serializing_if = "ViewState::is_empty")]
    pub saved_view: ViewState,
    /// Whether the song is parked, as Vim's `readonly`, as saved with the song: `:w` to its own
    /// folder refuses, and `:w!` or a copy saved elsewhere are the ways to keep a change. Left out
    /// when off. Not undone. While the song is open the app's live copy,
    /// `Services.live.read_only`, is the truth; it's read from here when the song opens and
    /// written back when it's saved.
    #[serde(
        default,
        rename = "read_only",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub saved_read_only: bool,
}

impl Song {
    /// A new project: one section, one chain and one phrase per track.
    pub fn new(tracks: usize) -> Self {
        let mut song = Self {
            format_version: FORMAT_VERSION,
            name: String::new(),
            millibpm: DEFAULT_MILLIBPM,
            seed: 0,
            saved_auto_jump: false,
            skip: default_skip(),
            saved_scale: Scale::default(),
            limiter: default_limiter(),
            groove: GrooveId::STRAIGHT,
            grooves: Vec::new(),
            launch: Launch::default(),
            bank: Bank::default(),
            tracks: Tracks::new(),
            sections: vec![Section::default()],
            saved_view: ViewState::default(),
            saved_read_only: false,
        };
        song.ensure_fixed_tracks();
        for _ in 0..tracks.max(1) {
            song.add_track();
        }
        song
    }

    /// Give a song any fixed tracks it lacks, each in its place after the sound tracks: all of them
    /// for a song built without, or the groups past those it was saved with once `MAX_GROUPS` is
    /// raised.
    pub(crate) fn ensure_fixed_tracks(&mut self) {
        let first = self
            .tracks
            .iter()
            .position(|track| !track.is_sound())
            .unwrap_or(self.tracks.len());
        for (place, kind) in TrackKind::FIXED.into_iter().enumerate() {
            let at = first + place;
            if self.tracks.get(at).is_some_and(|track| track.kind == kind) {
                continue;
            }
            self.tracks.insert(at, Track::fixed(kind));
            for section in &mut self.sections {
                if at <= section.chains.len() {
                    section.chains.insert(at, None);
                }
            }
        }
        for section in &mut self.sections {
            section.chains.resize(self.tracks.len(), None);
        }
    }

    /// Put a file in the bank, unless it's in it already, and have a track's sampler play it,
    /// giving the entry's number. Its hash is left unknown, for songs built in code, such as tests',
    /// whose samples are put in the pool by name. A full bank, or no such track, changes nothing.
    pub fn sample_on(&mut self, track: usize, path: &str) -> Option<u8> {
        let number = self
            .bank
            .add(BankEntry::of_file(path, ContentHash::UNKNOWN))?;
        let track = self.tracks.get_mut(track)?;
        track.machine = Machine::Sampler;
        track.set_setting(SettingId::SAMPLE, number);
        Some(number)
    }

    /// The bank entries something names: a track's own `Sample`, or a step's `SMP`. An LFO or
    /// an envelope that moves `Sample` can reach others, which aren't counted.
    pub fn samples_in_use(&self) -> BTreeSet<u8> {
        let mut used = BTreeSet::new();
        for track in &self.tracks {
            used.insert(track.sample());
            let locks = track
                .phrases
                .values()
                .flat_map(|phrase| phrase.steps())
                .flat_map(|step| step.fx.iter())
                .filter(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)));
            used.extend(locks.map(|slot| slot.coarse()));
        }
        used.remove(&0);
        used
    }

    /// Have every step that locks a slice of an entry by number lock the one it's now numbered,
    /// from what each old number became, so patterns keep their chops as the entry is sliced
    /// again. A step plays the entry its own `SMP` locks, or else its track's `Sample`. A number
    /// past the last slice played the last, so it follows the last. An LFO's depth on `SLI` is a
    /// distance rather than a slice, so it's left as it is.
    pub(crate) fn renumber_slices(&mut self, number: u8, renumbered: &[u8]) {
        let Some(last) = renumbered.len().checked_sub(1) else {
            return;
        };
        let renumber = |slot: &mut FxSlot| {
            let old = usize::from(slot.coarse()).min(last);
            slot.value = (u16::from(renumbered[old]) << 8) | u16::from(slot.fine());
        };
        // A lock by a letter follows its slice already, through the entry's marks.
        let is_slice = |slot: &FxSlot| {
            slot.command == Some(FxCommand::Set(SettingId::SLICE)) && slot.slice_mark().is_none()
        };
        for index in 0..self.tracks.len() {
            let track = &self.tracks[index];
            if track.machine != Machine::Sampler || !track.is_sound() {
                continue;
            }
            let own = track.sample();
            let plays = |step: &Step| {
                let locked = step
                    .fx
                    .iter()
                    .rev()
                    .find(|slot| slot.command == Some(FxCommand::Set(SettingId::SAMPLE)));
                locked.map_or(own, |slot| slot.coarse()) == number
            };
            let touched = |phrase: &Phrase| {
                phrase
                    .steps()
                    .iter()
                    .any(|step| plays(step) && step.fx.iter().any(is_slice))
            };
            // Only a track with such a step is written to, as writing to one copies it.
            if !track.phrases.values().any(|phrase| touched(phrase)) {
                continue;
            }
            for phrase in self.tracks[index].phrases.values_mut() {
                if !touched(phrase) {
                    continue;
                }
                let phrase = Arc::make_mut(phrase);
                for at in 0..phrase.steps().len() {
                    let Some(step) = phrase.step_mut(at) else {
                        continue;
                    };
                    if plays(step) {
                        step.fx
                            .iter_mut()
                            .filter(|slot| is_slice(slot))
                            .for_each(&renumber);
                    }
                }
            }
        }
    }

    /// How many tracks play sounds. They come first, and then the fixed tracks.
    pub fn sound_tracks(&self) -> usize {
        self.tracks.len().saturating_sub(FIXED_TRACKS)
    }

    /// A sound track's phrase, as a `Follow` names it, if there is one. A fixed track has no notes
    /// to follow, and moves along when sound tracks come or go.
    pub fn source_phrase(&self, source: Source) -> Option<&Arc<Phrase>> {
        (source.track < self.sound_tracks())
            .then(|| self.tracks[source.track].phrases.get(&source.phrase))
            .flatten()
    }

    /// The phrases following one, each with the place of its generator that follows it.
    pub fn followers(&self, source: Source) -> Vec<(Source, usize)> {
        let mut found = Vec::new();
        for (track, owner) in self.tracks.iter().enumerate().take(self.sound_tracks()) {
            for (id, phrase) in &owner.phrases {
                for (index, generator) in phrase.generators().iter().enumerate() {
                    if generator.source() == Some(source) {
                        let here = Source { track, phrase: *id };
                        found.push((here, index));
                    }
                }
            }
        }
        found
    }

    /// Where a fixed track is among all the tracks.
    pub fn fixed_track(&self, kind: TrackKind) -> Option<usize> {
        Some(self.sound_tracks() + kind.fixed_place()?)
    }

    /// The group track a sound track plays through, if it's in a group.
    pub fn group_track_of(&self, track: usize) -> Option<usize> {
        let group = self.tracks.get(track).filter(|t| t.is_sound())?.group?;
        self.fixed_track(TrackKind::Group(group))
    }

    /// The sound tracks a group track sums, in order, or none for any other track.
    pub fn group_members(&self, group: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.sound_tracks()).filter(move |&track| self.group_track_of(track) == Some(group))
    }

    /// Whether a track is heard, by the mutes and solos saved with the song: a sound track by its
    /// own and its group's, and a group while any of its tracks is. Soloing a track keeps its
    /// group heard, and soloing a group plays all its tracks. The returns and the master are
    /// always counted heard. While the song is open the app's mutes are the truth, and
    /// [`Song::heard_by`] takes them.
    pub fn heard_as_saved(&self, track: usize) -> bool {
        self.heard_by(track, |at| {
            (self.tracks[at].saved_mute, self.tracks[at].saved_solo)
        })
    }

    /// Whether a track is heard as [`Song::heard_as_saved`] has it, by mutes and solos given for
    /// each track as `(mute, solo)`, such as the ones the keys set while playing.
    pub fn heard_by(&self, track: usize, silenced: impl Fn(usize) -> (bool, bool) + Copy) -> bool {
        let Some(this) = self.tracks.get(track) else {
            return false;
        };
        if this.is_group() {
            return self
                .group_members(track)
                .any(|member| self.heard_by(member, silenced));
        }
        if !this.is_sound() {
            return true;
        }
        let (mute, solo) = silenced(track);
        let group = self.group_track_of(track).map(silenced);
        let any_solo =
            (0..self.tracks.len().min(self.sound_tracks() + MAX_GROUPS)).any(|at| silenced(at).1);
        !mute
            && !group.is_some_and(|(mute, _)| mute)
            && (!any_solo || solo || group.is_some_and(|(_, solo)| solo))
    }

    /// A groove's pushes: one that comes with the program, or the project's own, which is a bar
    /// on the grid until it's made.
    pub fn groove(&self, id: GrooveId) -> Groove {
        match id.custom_number() {
            Some(number) => self.grooves.get(number).copied().unwrap_or_default(),
            None => id.preset().unwrap_or_default(),
        }
    }

    /// The groove a track plays with when no step changes it: its own, or the project's.
    pub fn track_groove(&self, track: usize) -> GrooveId {
        self.tracks
            .get(track)
            .and_then(|track| track.groove)
            .unwrap_or(self.groove)
    }

    /// Add a track after the last that plays sounds, with a new chain and phrase in the first
    /// section and none in the rest.
    pub fn add_track(&mut self) {
        let mut track = Track::default();
        track.add_chain(0);
        let at = self.sound_tracks();
        self.tracks.insert(at, track);
        for (index, section) in self.sections.iter_mut().enumerate() {
            section
                .chains
                .insert(at.min(section.chains.len()), (index == 0).then_some(0));
        }
    }

    /// Remove the last track that plays sounds.
    pub fn remove_last_sound_track(&mut self) {
        let Some(at) = self.sound_tracks().checked_sub(1) else {
            return;
        };
        self.tracks.remove(at);
        for section in &mut self.sections {
            if at < section.chains.len() {
                section.chains.remove(at);
            }
        }
    }

    /// How many steps a section plays before the next one.
    pub fn section_length(&self, section: &Section) -> usize {
        section
            .length
            .unwrap_or_else(|| self.section_steps(section))
    }

    /// How many steps a section plays before moving on, with its repeats.
    pub fn section_plays(&self, section: &Section) -> usize {
        self.section_length(section) * usize::from(section.repeat.get())
    }

    /// How many steps the whole song plays, once through.
    pub fn song_steps(&self) -> usize {
        self.sections.iter().map(|s| self.section_plays(s)).sum()
    }

    /// Add a section after this position, with no chains.
    pub(crate) fn insert_section(&mut self, at: usize) {
        let section = Section {
            chains: vec![None; self.tracks.len()],
            ..Section::default()
        };
        self.sections.insert(at.min(self.sections.len()), section);
    }

    /// A section's length: its longest chain, in the song's steps, so a chain at half speed lasts
    /// twice its rows. A triplet chain that doesn't end on one of the song's steps is rounded up.
    pub fn section_steps(&self, section: &Section) -> usize {
        self.tracks
            .iter()
            .zip(&section.chains)
            .filter_map(|(track, chain)| {
                chain.map(|chain| track.speed.song_steps(track.chain_steps(chain)))
            })
            .max()
            .unwrap_or(0)
    }
}

/// The lowest ID not in use.
pub fn lowest_unused<T>(items: &BTreeMap<u32, T>) -> u32 {
    let mut candidate = 0;
    for id in items.keys() {
        if *id != candidate {
            break;
        }
        candidate += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::{Distort, DistortType};
    use crate::order::Order;
    use crate::phrase::tests::{gated, numbered};

    #[test]
    fn a_tracks_reset_is_named_in_bars_or_steps() {
        assert_eq!(reset_name(None), "Off");
        assert_eq!(reset_name(Some(5)), "5-step");
        assert_eq!(reset_name(Some(16)), "1bar");
        assert_eq!(reset_name(Some(48)), "3bar");
        assert_eq!(reset_named("off"), Some(None));
        assert_eq!(reset_named("Off"), Some(None));
        assert_eq!(reset_named("5"), Some(Some(5)));
        assert_eq!(reset_named("5-step"), Some(Some(5)));
        assert_eq!(reset_named("0x10"), Some(Some(16)));
        assert_eq!(reset_named("2bar"), Some(Some(32)));
        assert_eq!(reset_named("3 bars"), Some(Some(48)));
        assert_eq!(reset_named("1024"), Some(Some(MAX_TRACK_RESET)));
        for wrong in ["0", "0bar", "1025", "65bar", "", "x", "-1", "0x"] {
            assert_eq!(reset_named(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn a_track_is_labelled_by_its_name_and_titled_by_its_kind() {
        let mut track = Track::default();
        assert_eq!(
            (track.label(2), track.title(2)),
            ("03".into(), "Track 03".into())
        );
        track.name = "BD".to_owned();
        assert_eq!(
            (track.label(2), track.title(2)),
            ("BD".into(), "Track 03".into())
        );
        let master = Track::fixed(TrackKind::Master);
        assert_eq!(
            (master.label(9), master.title(9)),
            ("M".into(), "Master".into())
        );
    }

    #[test]
    fn a_tracks_reset_is_from_one_step_to_the_most() {
        let song = Arc::new(Song::new(1));
        let set = |reset| crate::action::Action::SetTrackReset { track: 0, reset };
        let edited = crate::action::edit(&song, &set(Some(16))).unwrap();
        assert_eq!(edited.tracks[0].reset, Some(16));
        assert!(crate::action::edit(&song, &set(Some(0))).is_err());
        assert!(crate::action::edit(&song, &set(Some(MAX_TRACK_RESET + 1))).is_err());
        let off = crate::action::edit(&edited, &set(None)).unwrap();
        assert_eq!(off.tracks[0].reset, None);
    }

    use crate::constants::DEFAULT_TRACKS;

    #[test]
    fn a_track_saved_without_a_level_loads_at_0_db() {
        let track: Track = ron::from_str("(mute: false, solo: true)").unwrap();
        assert_eq!(track.level, UNITY_LEVEL);
        assert!(track.saved_solo);
    }

    #[test]
    fn a_tracks_punch_saves_and_loads_and_none_is_left_out() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.punch, 0);
        assert!(!ron::to_string(&track).unwrap().contains("punch"));
        let track = Track {
            punch: 0x60,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("punch:96"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap().punch, 0x60);
    }

    #[test]
    fn a_tracks_quantise_saves_and_loads_and_none_is_left_out() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.quantise, 0);
        assert!(!ron::to_string(&track).unwrap().contains("quantise"));
        let track = Track {
            quantise: 0x80,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("quantise:128"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap().quantise, 0x80);
    }

    #[test]
    fn a_tracks_phase_saves_only_at_reset_and_a_sound_preset_keeps_it() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.phase, Phase::Free);
        assert!(!ron::to_string(&track).unwrap().contains("phase"));
        let track = Track {
            machine: Machine::Plaits(SynthVoice::VirtualAnalog),
            phase: Phase::Reset,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("phase:Reset"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap().phase, Phase::Reset);
        let preset = crate::preset::SoundPreset::of(&track, None);
        assert_eq!(preset.sound.phase, Phase::Reset);
        assert_eq!(
            track.value_of(SettingId::PHASE) >> 8,
            u16::from(Phase::Reset.index())
        );
    }

    #[test]
    fn a_tracks_voice_mode_saves_only_at_legato_and_a_sound_preset_keeps_it() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.voice_mode, VoiceMode::Mono);
        assert!(!ron::to_string(&track).unwrap().contains("voice_mode"));
        let track = Track {
            voice_mode: VoiceMode::Legato,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("voice_mode:Legato"), "{text}");
        assert_eq!(
            ron::from_str::<Track>(&text).unwrap().voice_mode,
            VoiceMode::Legato
        );
        let preset = crate::preset::SoundPreset::of(&track, None);
        assert_eq!(preset.sound.voice_mode, VoiceMode::Legato);
        assert_eq!(
            track.value_of(SettingId::VOICE_MODE) >> 8,
            u16::from(VoiceMode::Legato.index())
        );
    }

    #[test]
    fn a_tracks_key_track_saves_only_on_and_a_sound_preset_keeps_it() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert!(!track.key_track);
        assert!(!ron::to_string(&track).unwrap().contains("key_track"));
        let track = Track {
            key_track: true,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("key_track:true"), "{text}");
        assert!(ron::from_str::<Track>(&text).unwrap().key_track);
        let preset = crate::preset::SoundPreset::of(&track, None);
        assert!(preset.sound.key_track);
        let mut loaded = Track::default();
        preset.apply(&mut loaded, 0);
        assert!(loaded.key_track);
        assert_eq!(track.value_of(SettingId::KEY_TRACK) >> 8, 1);
        assert_eq!(Track::default().value_of(SettingId::KEY_TRACK), 0);
    }

    #[test]
    fn a_tracks_glide_mode_saves_only_at_always_and_a_sound_preset_keeps_it() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.glide_mode, GlideMode::Ties);
        assert!(!ron::to_string(&track).unwrap().contains("glide_mode"));
        let track = Track {
            glide_mode: GlideMode::Always,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("glide_mode:Always"), "{text}");
        assert_eq!(
            ron::from_str::<Track>(&text).unwrap().glide_mode,
            GlideMode::Always
        );
        let preset = crate::preset::SoundPreset::of(&track, None);
        assert_eq!(preset.sound.glide_mode, GlideMode::Always);
        assert_eq!(
            track.value_of(SettingId::GLIDE_MODE) >> 8,
            u16::from(GlideMode::Always.index())
        );
    }

    #[test]
    fn a_tracks_safe_saves_and_loads_and_off_is_left_out() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert!(!track.safe);
        assert!(!ron::to_string(&track).unwrap().contains("safe"));
        let track = Track {
            safe: true,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("safe:true"), "{text}");
        assert!(ron::from_str::<Track>(&text).unwrap().safe);
    }

    #[test]
    fn a_tracks_role_saves_when_told_and_goes_by_its_machine_until_then() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.role, Role::Auto);
        assert_eq!(track.role(), Role::None, "a sample could be anything");
        assert!(!ron::to_string(&track).unwrap().contains("role"));
        let kick = Track {
            machine: Machine::Plaits(SynthVoice::Kick),
            ..Track::default()
        };
        assert_eq!(kick.role(), Role::Drums);
        let noise = Machine::Plaits(SynthVoice::Swarm);
        assert_eq!(noise.role(), Role::Texture);
        let told = Track {
            role: Role::Bass,
            ..kick
        };
        assert_eq!(told.role(), Role::Bass);
        let text = ron::to_string(&told).unwrap();
        assert!(text.contains("role:Bass"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap().role, Role::Bass);
        let group = Track::fixed(TrackKind::Group(1));
        assert_eq!(
            Track {
                role: Role::Pad,
                ..group
            }
            .role(),
            Role::None,
            "a group is told by what it is"
        );
    }

    #[test]
    fn a_tracks_trim_saves_and_loads_and_0_db_is_left_out() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track.trim, UNITY_TRIM);
        assert!(!ron::to_string(&track).unwrap().contains("trim"));
        for trim in [0x00, 0x40, 0xFF] {
            let track = Track {
                trim,
                ..Track::default()
            };
            let text = ron::to_string(&track).unwrap();
            assert!(text.contains(&format!("trim:{trim}")), "{text}");
            let loaded: Track = ron::from_str(&text).unwrap();
            assert_eq!(loaded.trim, trim);
        }
    }

    #[test]
    fn a_trim_reads_in_db_to_its_quarter() {
        assert_eq!(trim_text(0x80), "+0.0 dB");
        assert_eq!(trim_text(0x81), "+0.25 dB");
        assert_eq!(trim_text(0x82), "+0.5 dB");
        assert_eq!(trim_text(0x7D), "-0.75 dB");
        assert_eq!(trim_text(0xC0), "+16.0 dB");
        assert_eq!(trim_text(0xFF), "+31.75 dB");
        assert_eq!(trim_text(0x01), "-31.75 dB");
        assert_eq!(trim_text(0x00), "-inf dB");
    }

    #[test]
    fn a_new_song_has_a_chain_and_phrase_per_track() {
        let song = Song::new(DEFAULT_TRACKS);
        assert_eq!(song.sound_tracks(), DEFAULT_TRACKS);
        assert_eq!(song.sections.len(), 1);
        assert_eq!(song.sections[0].chains.len(), song.tracks.len());
        for track in &song.tracks[..DEFAULT_TRACKS] {
            assert_eq!((track.chains.len(), track.phrases.len()), (1, 1));
        }
        assert_eq!(song.section_steps(&song.sections[0]), DEFAULT_PHRASE_STEPS);
    }

    #[test]
    fn the_fixed_tracks_stay_last_and_dont_set_a_sections_length() {
        let mut song = Song::new(2);
        let kinds = |song: &Song| song.tracks.iter().map(|t| t.kind).collect::<Vec<_>>();
        use TrackKind::{Group, Master, ReturnA, ReturnB, Sound};
        let fixed = [
            Group(1),
            Group(2),
            Group(3),
            Group(4),
            ReturnA,
            ReturnB,
            Master,
        ];
        assert_eq!(kinds(&song)[..2], [Sound, Sound]);
        assert_eq!(kinds(&song)[2..], fixed);
        let names: Vec<&str> = song.tracks[2..].iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["G1", "G2", "G3", "G4", "A", "B", "M"]);
        assert!(song.sections[0].chains[2..].iter().all(Option::is_none));

        song.add_track();
        assert_eq!(kinds(&song)[..3], [Sound, Sound, Sound]);
        assert_eq!(kinds(&song)[3..], fixed);
        assert_eq!(song.sections[0].chains[2], Some(0), "the new one's chain");
        assert_eq!(song.sections[0].chains[3], None, "G1's stays G1's");

        song.remove_last_sound_track();
        assert_eq!(kinds(&song)[2..], fixed);
        assert_eq!(song.sections[0].chains.len(), 2 + FIXED_TRACKS);
    }

    #[test]
    fn a_song_from_before_the_fixed_tracks_gets_them_once() {
        let mut song = Song::new(2);
        song.tracks.truncate(2);
        song.sections[0].chains.truncate(2);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
        assert_eq!(song.sound_tracks(), 2);
        assert_eq!(song.sections[0].chains.len(), 2 + FIXED_TRACKS);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
    }

    #[test]
    fn a_song_saved_with_fewer_groups_gets_the_rest_before_the_returns() {
        let mut song = Song::new(2);
        song.sections[0].chains[2 + MAX_GROUPS] = Some(0);
        // As a song saved when there were only two groups.
        song.tracks.drain(4..2 + MAX_GROUPS);
        song.sections[0].chains.drain(4..2 + MAX_GROUPS);
        song.ensure_fixed_tracks();
        assert_eq!(song.tracks.len(), 2 + FIXED_TRACKS);
        assert_eq!(song.sound_tracks(), 2);
        let kinds: Vec<_> = song.tracks[2..].iter().map(|t| t.kind).collect();
        assert_eq!(kinds, TrackKind::FIXED);
        assert_eq!(song.fixed_track(TrackKind::ReturnA), Some(2 + MAX_GROUPS));
        assert_eq!(
            song.sections[0].chains[2 + MAX_GROUPS],
            Some(0),
            "A's chain stays A's"
        );
    }

    #[test]
    fn a_track_in_a_group_plays_through_its_track_and_saves_it() {
        let mut song = Song::new(3);
        song.tracks[0].group = Some(1);
        song.tracks[1].group = Some(2);
        assert_eq!(song.group_track_of(0), Some(3));
        assert_eq!(song.group_track_of(1), Some(4));
        assert_eq!(song.group_track_of(2), None);
        let text = ron::to_string(&song.tracks[0]).unwrap();
        assert!(text.contains("group:Some(1)"), "{text}");
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded.group, Some(1));
        let alone = ron::to_string(&song.tracks[2]).unwrap();
        assert!(!alone.contains("group"), "none isn't written: {alone}");
    }

    #[test]
    fn muting_a_group_mutes_its_tracks_and_soloing_one_keeps_it() {
        let mut song = Song::new(3);
        song.tracks[0].group = Some(1);
        song.tracks[1].group = Some(1);
        let group = song.group_track_of(0).unwrap();
        let heard = |song: &Song| (0..3).map(|t| song.heard_as_saved(t)).collect::<Vec<_>>();
        assert_eq!(heard(&song), [true, true, true]);
        assert!(song.heard_as_saved(group));

        song.tracks[group].saved_mute = true;
        assert_eq!(
            heard(&song),
            [false, false, true],
            "a muted group mutes its tracks"
        );
        assert!(!song.heard_as_saved(group));
        song.tracks[group].saved_mute = false;

        song.tracks[0].saved_solo = true;
        assert_eq!(heard(&song), [true, false, false]);
        assert!(
            song.heard_as_saved(group),
            "a soloed track keeps its group heard"
        );
        song.tracks[0].saved_solo = false;

        song.tracks[group].saved_solo = true;
        assert_eq!(
            heard(&song),
            [true, true, false],
            "a soloed group plays its tracks"
        );
        song.tracks[1].saved_mute = true;
        assert_eq!(
            heard(&song),
            [true, false, false],
            "but not one muted itself"
        );
    }

    #[test]
    fn a_dealt_order_is_dealt_by_the_time_round_the_chain_and_the_slot() {
        let mut song = Song::new(1);
        let mut phrase = Phrase::new(8);
        phrase.set_order(Order::Shuffle);
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        let track = &song.tracks[0];
        let dealer = Dealer::new(5, 0);
        let lap = |lap: usize| -> Vec<usize> {
            (0..8)
                .map(|step| {
                    track
                        .chain_position(0, lap * 8 + step, dealer)
                        .unwrap()
                        .step
                })
                .collect()
        };
        assert_ne!(lap(0), lap(1));
        assert_eq!(lap(1), lap(1));
        let row = lap(1)[3];
        let place = RowPlace {
            slot: 0,
            row,
            pulse: 0,
            near: 0,
        };
        assert_eq!(
            track.chain_place(0, place, 1, dealer),
            Some(3),
            "where the deal plays it"
        );
    }

    #[test]
    fn a_chain_lasts_its_rows_pulses_and_finds_them() {
        let mut song = Song::new(1);
        song.tracks[0]
            .phrases
            .insert(0, Arc::new(gated(&["HLD3", "SGL1"])));
        song.tracks[0].phrases.insert(7, Arc::new(gated(&["MLT2"])));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        assert_eq!(track.chain_steps(0), 6);
        let at = |step| {
            let found = track.chain_position(0, step, Dealer::default()).unwrap();
            (found.slot, found.step, found.pulse)
        };
        assert_eq!(at(2), (0, 0, 2));
        assert_eq!(at(3), (0, 1, 0));
        assert_eq!(at(5), (1, 0, 1));
        assert_eq!(at(6), (0, 0, 0), "and round again");
        let place = |slot, row| RowPlace {
            slot,
            row,
            pulse: 0,
            near: 0,
        };
        assert_eq!(
            track.chain_place(0, place(1, 0), 0, Dealer::default()),
            Some(4)
        );
        assert_eq!(
            track.chain_place(0, place(0, 1), 0, Dealer::default()),
            Some(3)
        );
    }

    #[test]
    fn a_slot_and_row_find_their_place_in_a_chain_if_they_are_still_there() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        let entry = |phrase| ChainEntry {
            phrase,
            transpose: 0,
        };
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![entry(0), entry(7)],
                label: String::new(),
            }),
        );
        let track = &song.tracks[0];
        let found = |chain, slot, row| {
            let place = RowPlace {
                slot,
                row,
                pulse: 0,
                near: 0,
            };
            track.chain_place(chain, place, 0, Dealer::default())
        };
        assert_eq!(found(0, 1, 2), Some(16 + 2));
        assert_eq!(found(0, 1, 4), None, "past its loop");
        assert_eq!(found(0, 2, 0), None, "no third slot");
        assert_eq!(found(9, 0, 0), None, "no such chain");
    }

    #[test]
    fn chain_positions_cross_phrases_and_loop() {
        let mut song = Song::new(1);
        song.tracks[0].phrases.insert(7, Arc::new(Phrase::new(4)));
        song.tracks[0].chains.insert(
            0,
            Arc::new(Chain {
                entries: vec![
                    ChainEntry {
                        phrase: 0,
                        transpose: 0,
                    },
                    ChainEntry {
                        phrase: 7,
                        transpose: 3,
                    },
                ],
                label: String::new(),
            }),
        );
        assert_eq!(song.tracks[0].chain_steps(0), 20);
        let position = song.tracks[0]
            .chain_position(0, 17, Dealer::default())
            .unwrap();
        assert_eq!((position.slot, position.phrase, position.step), (1, 7, 1));
        assert_eq!(position.transpose, 3);
        assert_eq!(
            song.tracks[0]
                .chain_position(0, 20, Dealer::default())
                .unwrap()
                .step,
            0
        );
        assert_eq!(
            song.tracks[0].chain_position(99, 0, Dealer::default()),
            None
        );
    }

    #[test]
    fn a_tracks_filter_mode_saves_and_loads() {
        let track = Track {
            filter: FilterType::Ladder,
            filter_mode: FilterMode::HighPass24,
            ..Track::default()
        };
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("filter_mode:HighPass24"), "{text}");
        assert_eq!(ron::from_str::<Track>(&text).unwrap(), track);
    }

    #[test]
    fn a_track_saved_before_its_page_loads_with_the_old_sound() {
        let track: Track = ron::from_str("(mute: false, solo: false)").unwrap();
        assert_eq!(track, Track::default());
        assert_eq!((track.cutoff, track.resonance, track.pan), (0xFF, 0, 0x80));
        assert_eq!(track.mods, default_mods());
    }

    #[test]
    fn a_tracks_fine_parts_save_by_key_and_go_at_the_top() {
        let mut track = Track {
            cutoff: 0x40,
            ..Track::default()
        };
        track.set_fine(SettingId::CUT, 0x80);
        let text = ron::to_string(&track).unwrap();
        assert!(text.contains("fine:{\"cut\":128}"), "{text}");
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded.fine(SettingId::CUT), 0x80);
        track.cutoff = 0xFF;
        track.set_fine(SettingId::CUT, 0x80);
        assert_eq!(track.fine(SettingId::CUT), 0);
        assert!(!ron::to_string(&Track::default()).unwrap().contains("fine"));
    }

    #[test]
    fn a_tracks_inserts_save_and_load_and_start_empty() {
        let mut track = Track::default();
        assert_eq!(track.inserts, [Insert::Empty; INSERTS]);
        track.inserts[1] = Insert::Distort(Distort {
            kind: DistortType::Fold,
            amount: 0x80,
            mix: 0x40,
            gain: 0xD0,
            ..Distort::DEFAULT
        });
        let text = ron::to_string(&track).unwrap();
        let loaded: Track = ron::from_str(&text).unwrap();
        assert_eq!(loaded, track);
    }

    #[test]
    fn level_values_are_even_steps_in_db() {
        let db = |level: f32| 20.0 * level_gain(level).log10();
        let close = |a: f32, b: f32| assert!((a - b).abs() < 1e-3, "{a} and {b}");
        assert_eq!(level_gain(0xE0 as f32), 1.0);
        close(db(0xF0 as f32), 4.0);
        close(db(0xF8 as f32), 6.0);
        close(db(0xFF as f32), 6.0);
        close(db(0xD0 as f32), -4.0);
        close(db(0xC0 as f32), -8.0);
        close(db(0x80 as f32), -24.0);
        close(db(0x40 as f32), -40.0);
        close(db(0x01 as f32), -55.75);
        assert_eq!(level_gain(0.0), 0.0);
        assert_eq!(level_gain(-3.0), 0.0);
    }

    #[test]
    fn a_section_saved_without_a_reset_loads_with_none() {
        let section: Section = ron::from_str("(chains: [Some(0)])").unwrap();
        assert_eq!(section.reset, None);
        let saved = ron::to_string(&Section {
            reset: Some(64),
            ..section
        })
        .unwrap();
        assert_eq!(ron::from_str::<Section>(&saved).unwrap().reset, Some(64));
    }

    #[test]
    fn step_at_reads_within_the_loop() {
        let mut song = Song::new(1);
        let mut phrase = numbered(8);
        phrase.set_len(4);
        phrase.slide(2);
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        assert_eq!(song.tracks[0].chain_steps(0), 4);
        let position = song.tracks[0]
            .chain_position(0, 5, Dealer::default())
            .unwrap();
        assert_eq!(position.step, 1, "the chain loops over the loop");
        assert_eq!(song.tracks[0].step_at(position).unwrap().velocity, Some(3));
    }

    #[test]
    fn the_lowest_unused_id_fills_gaps() {
        let items = BTreeMap::from([(0, ()), (1, ()), (3, ())]);
        assert_eq!(lowest_unused(&items), 2);
        assert_eq!(lowest_unused(&BTreeMap::<u32, ()>::new()), 0);
    }
}
