//! A track's eight macros, after Ableton Live's Rack macros (`docs/designs/generators-page.md` §4):
//! each is one knob that turns any number of the track's settings at once, each between a min and a
//! max of its own. A macro is itself a setting, `MAC1` to `MAC8`, which a step locks and modulation
//! moves, and it's absolute: at `00` each setting it holds is at its min and at `FF` at its max.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::constants::{FX_SLOTS, UNITY_LEVEL};
use crate::generator::Generator;
use crate::insert::INSERTS;
use crate::lfo::phase_value;
use crate::modulation::{MOD_SLOTS, ModKind, Polarity, depth_of, depth_value};
use crate::order::MOST_ORDER_NUMBER;
use crate::settings::{Kind, Owner, SettingId, SettingSet};
use crate::shape::{GenParam, MAX_BEND, Shape, ShapeColumn, curved};
use crate::song::{FilterMode, FilterType, GlideMode, Phase, Song, Track, VoiceMode};
use crate::speed::Speed;
use crate::step::Step;

/// How many macros a track has.
pub const MACROS: usize = 8;

/// The longest a macro's name can be: the Roto's width.
pub const MACRO_NAME_LENGTH: usize = 12;

/// One macro: its name, where it's turned, and what it turns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Macro {
    /// Its name, as typed, or none to go by its first mapping's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Where it's turned, from `0000` to `FF00`, a coarse byte and a fine one, as a cutoff is.
    #[serde(skip_serializing_if = "is_zero")]
    pub value: u16,
    /// What it turns, a setting a map, in the order they were mapped.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub maps: Vec<MacroMap>,
}

fn is_zero(value: &u16) -> bool {
    *value == 0
}

fn is_straight(bend: &i8) -> bool {
    *bend == 0
}

/// One setting a macro turns, how far, and along what curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroMap {
    pub target: MacroTarget,
    /// What the setting is with the macro at `00` and at `FF`, in the setting's own values, with
    /// a coarse byte and a fine one. `min` above `max` turns it the other way.
    pub min: u16,
    pub max: u16,
    /// Its response from `min` to `max`, bent as the shape panel's ramp is: `curve` bends it, above
    /// 0 moving early and below 0 late, and `s` then eases both ends, above 0 an S and below 0 the
    /// reverse, each from -100 to 100 and 0 straight, as a new mapping is.
    #[serde(default, skip_serializing_if = "is_straight")]
    pub curve: i8,
    #[serde(default, skip_serializing_if = "is_straight")]
    pub s: i8,
    /// The part of the macro's turn it acts over, `00` to `FF` by default: below the zone it holds
    /// its setting at `min`, above it at `max`, and across it runs from one to the other.
    #[serde(default, skip_serializing_if = "Zone::is_whole")]
    pub zone: Zone,
    /// On a group track, the track of the group it reaches, by its place among the group's tracks
    /// from 0, so a group's patch reaches the same places wherever it's loaded (decision 408). None
    /// is the macro's own track, as every sound track's mappings are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<u8>,
}

/// The part of a macro's turn a mapping acts over, from `start` to `end` on the macro's `00` to
/// `FF` scale, so one macro can be a bipolar knob, one mapping on its left half and another on its
/// right (decision 392). `start` is always below `end`: a zone that turns a setting the other way
/// is a range with `min` above `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ZoneFile")]
pub struct Zone {
    pub start: u8,
    pub end: u8,
}

/// A zone as a file has it, refused unless it starts below its end.
#[derive(Deserialize)]
struct ZoneFile {
    start: u8,
    end: u8,
}

impl TryFrom<ZoneFile> for Zone {
    type Error = String;

    fn try_from(file: ZoneFile) -> Result<Self, Self::Error> {
        Self::new(file.start, file.end).ok_or_else(|| {
            format!(
                "a macro's zone starts below its end, and this one is {:02X} to {:02X}",
                file.start, file.end
            )
        })
    }
}

impl Default for Zone {
    fn default() -> Self {
        Self::WHOLE
    }
}

impl Zone {
    /// The whole turn, as a new mapping acts over.
    pub const WHOLE: Self = Self {
        start: 0x00,
        end: 0xFF,
    };

    /// A zone from `start` to `end`, or none if `start` isn't below `end`.
    pub fn new(start: u8, end: u8) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }

    /// Whether it's the whole turn.
    pub fn is_whole(&self) -> bool {
        *self == Self::WHOLE
    }

    /// How far across it the macro is at `value`, on the `00` to `FF` scale with any fine part as
    /// the fraction, from 0 at its start to 1 at its end, and held at 0 below it and 1 above. The
    /// whole turn's is `value / 255` as it always was, to the bit.
    pub fn along(self, value: f32) -> f32 {
        if self.is_whole() {
            return (value / 255.0).clamp(0.0, 1.0);
        }
        let (start, end) = (f32::from(self.start), f32::from(self.end));
        ((value - start) / (end - start)).clamp(0.0, 1.0)
    }

    /// Where the macro is, on the `0000` to `FF00` scale, this far across it, from 0 to 1.
    pub fn turn_at(self, along: f32) -> u16 {
        if self.is_whole() {
            return (along * f32::from(0xFF00_u16)).round() as u16;
        }
        let (start, end) = (f32::from(self.start), f32::from(self.end));
        ((start + (end - start) * along) * 256.0).round() as u16
    }

    /// It as the view writes it, `00–7F`.
    pub fn text(self) -> String {
        format!("{:02X}–{:02X}", self.start, self.end)
    }
}

/// What a mapping reaches. Keyed by what it is rather than where, so a mapping saved with a sound
/// takes hold of the same setting wherever it's loaded, and a generator's by its column, so it
/// takes hold of whichever phrase's generator is on that column (the design's §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MacroTarget {
    /// A setting of the registry: the track's own, its machine's, an effect's, a mod slot's, the
    /// accumulator's or the arpeggio's. Never a macro.
    Setting(SettingId),
    /// A setting of the live generator on a column, in every phrase of the track with one there.
    /// Its ends are numbers in the setting's own, as [`GenParam::number`] gives them, set off by
    /// [`GEN_ZERO`]. A turn writes the column again, and the engine holds the setting where a lock
    /// or modulation puts the macro as it plays.
    Gen {
        column: GenColumn,
        #[serde(with = "param_key")]
        param: GenParam,
    },
}

/// What a generator setting's number is set off by in a mapping's ends, so a negative one, as a
/// `Curve` below none or a `Transpose` down, fits.
pub const GEN_ZERO: i32 = 0x8000;

/// A generator's column, as a mapping keys it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GenColumn {
    Note,
    Velocity,
    Nudge,
    Length,
    Fx(usize),
}

impl GenColumn {
    /// The column a generator fills.
    pub fn of(column: ShapeColumn) -> Self {
        match column {
            ShapeColumn::Note { .. } => Self::Note,
            ShapeColumn::Velocity => Self::Velocity,
            ShapeColumn::Nudge { .. } => Self::Nudge,
            ShapeColumn::Length => Self::Length,
            ShapeColumn::Fx { slot, .. } => Self::Fx(slot),
        }
    }

    /// Its name, as the phrase view heads it: `Note`, `Vel`, `FX1`.
    pub fn name(self) -> String {
        match self {
            Self::Note => "Note".to_owned(),
            Self::Velocity => "Vel".to_owned(),
            Self::Nudge => "Nudge".to_owned(),
            Self::Length => "Len".to_owned(),
            Self::Fx(slot) => format!("FX{}", slot + 1),
        }
    }
}

/// A generator setting stored by its key, which never changes.
mod param_key {
    use serde::{Deserialize, Deserializer, Serializer};

    use crate::shape::GenParam;

    pub(crate) fn serialize<S: Serializer>(
        param: &GenParam,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(param.key())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<GenParam, D::Error> {
        let key = String::deserialize(deserializer)?;
        GenParam::by_key(&key).ok_or_else(|| {
            serde::de::Error::custom(format!("there's no generator setting called {key}"))
        })
    }
}

impl MacroTarget {
    /// Whether a macro can reach it: anything in the registry but another macro, and any
    /// generator setting that's a number, which a `List`'s values aren't.
    pub fn mappable(self) -> bool {
        match self {
            Self::Setting(id) => !matches!(id.owner().0, Owner::Macro(_)),
            // The fill and the `Where` are what the generator is, which the app builds, and a
            // `Follow`'s source is a phrase; the rest are numbers to turn.
            Self::Gen { param, .. } => !matches!(
                param,
                GenParam::Values | GenParam::Fill | GenParam::Where | GenParam::Source
            ),
        }
    }

    /// Whether it's reached by hand alone: a generator's setting the engine can't hold as it
    /// plays, `Wave` or `Rate`, which a turn writes into its column but a lock or modulation
    /// doesn't reach.
    pub fn by_hand(self) -> bool {
        matches!(self, Self::Gen { param, .. } if !param.held_in_engine())
    }

    /// The setting of the registry it is, if it's one.
    pub fn setting(self) -> Option<SettingId> {
        match self {
            Self::Setting(id) => Some(id),
            Self::Gen { .. } => None,
        }
    }

    /// Its name on a track: a setting's, or a generator's by its column, as `Note Hits`.
    pub fn name_on(self, track: &Track) -> String {
        match self {
            Self::Setting(id) => id.name_on(track),
            Self::Gen { column, param } => format!("{} {}", column.name(), param.name()),
        }
    }

    /// Its plain name, for a macro named after it.
    pub fn name(self) -> String {
        match self {
            Self::Setting(id) => id.name().to_owned(),
            Self::Gen { param, .. } => param.name().to_owned(),
        }
    }
}

/// A mapping's response with this `curve` and `s`, as [`MacroMap::response`] gives it.
pub fn response(along: f32, curve: i8, s: i8) -> f32 {
    if curve == 0 && s == 0 {
        along
    } else {
        curved(f64::from(along), curve, s) as f32
    }
}

/// The highest value a setting takes, with its coarse byte and fine one: a fine part goes no
/// further than the top's coarse value.
pub fn top_value(kind: Kind) -> u16 {
    u16::from(kind.top()) << 8
}

impl MacroMap {
    /// A mapping over a setting's whole range, from its bottom to its top, as a new mapping
    /// starts, as Live's do. A generator's has its range from its bounds, with
    /// [`MacroMap::numbers`].
    pub fn whole(target: MacroTarget) -> Self {
        let top = target.setting().map_or(0, |id| top_value(id.entry().kind));
        Self {
            target,
            min: 0,
            max: top,
            curve: 0,
            s: 0,
            zone: Zone::WHOLE,
            member: None,
        }
    }

    /// A mapping onto a generator's setting, from one number to another in its own.
    pub fn numbers(target: MacroTarget, (low, high): (i32, i32)) -> Self {
        let end = |number: i32| (number + GEN_ZERO).clamp(0, i32::from(u16::MAX)) as u16;
        Self {
            target,
            min: end(low),
            max: end(high),
            curve: 0,
            s: 0,
            zone: Zone::WHOLE,
            member: None,
        }
    }

    /// The same acting over this zone of its macro's turn.
    #[must_use]
    pub fn zoned(self, zone: Zone) -> Self {
        Self { zone, ..self }
    }

    /// Whether its response is a straight line, as a new mapping's is.
    pub fn is_straight(&self) -> bool {
        self.curve == 0 && self.s == 0
    }

    /// The same with its `curve` and `s` at these, each kept within the shape panel's -100 to
    /// 100.
    #[must_use]
    pub fn bent(self, curve: i32, s: i32) -> Self {
        let most = i32::from(MAX_BEND);
        Self {
            curve: curve.clamp(-most, most) as i8,
            s: s.clamp(-most, most) as i8,
            ..self
        }
    }

    /// How far from `min` toward `max` it puts its setting with its macro `along` its turn, both
    /// from 0 to 1: `along` itself while it's straight, and otherwise along the shape panel's
    /// ramp ([`curved`]), `curve` first and `s` over it, so `S` eases the ends of whatever
    /// `Curve` makes. It starts at 0 and ends at 1 however it's bent, so its ends are always
    /// `min` and `max`.
    pub(crate) fn response(&self, along: f32) -> f32 {
        response(along, self.curve, self.s)
    }

    /// Whether its ends are values its setting takes, and its `curve` and `s` within -100 to 100.
    /// A generator's ends are kept to its bounds as they're used, as those hang on the phrase
    /// it's in.
    pub(crate) fn in_range(&self) -> bool {
        let fits = match self.target {
            MacroTarget::Setting(id) => {
                let top = top_value(id.entry().kind);
                self.min <= top && self.max <= top
            }
            MacroTarget::Gen { .. } => true,
        };
        let bends = [self.curve, self.s]
            .iter()
            .all(|bend| bend.unsigned_abs() <= MAX_BEND.unsigned_abs());
        let zoned = self.zone.start < self.zone.end;
        self.target.mappable() && fits && bends && zoned
    }

    /// What it puts its setting at with its macro at `value`, on the `00` to `FF` scale with any
    /// fine part as the fraction: from `min` at its zone's start to `max` at its end, held at
    /// `min` below the zone and `max` above it, along its
    /// [response](MacroMap::response), and to a whole step for a setting with no fine part, as a
    /// list's or the level scale's. A generator's is its number set off by [`GEN_ZERO`], to the
    /// nearest whole one.
    pub fn at(&self, value: f32) -> u16 {
        let along = self.response(self.zone.along(value));
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        let to = min + (max - min) * along;
        let Some(id) = self.target.setting() else {
            return to.round() as u16;
        };
        let kind = id.entry().kind;
        let to = to.round().clamp(0.0, f32::from(top_value(kind)));
        if kind.has_fine() {
            to as u16
        } else {
            ((to / 256.0).round() as u16) << 8
        }
    }

    /// The number it puts a generator's setting at with its macro at `value`.
    pub fn number_at(&self, value: f32) -> i32 {
        i32::from(self.at(value)) - GEN_ZERO
    }

    /// Where a macro would put its setting at this value, inverted: the macro's value, on the
    /// `0000` to `FF00` scale, that puts the setting nearest `value`, for a first mapping that
    /// turns its macro to where the setting is so nothing jumps. It's within the mapping's zone.
    pub fn macro_for(&self, value: u16) -> u16 {
        if self.min == self.max {
            return self.zone.turn_at(0.0);
        }
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        let wanted = ((f32::from(value) - min) / (max - min)).clamp(0.0, 1.0);
        if self.is_straight() {
            return self.zone.turn_at(wanted);
        }
        // However it's bent, the response only rises, so halving finds where it reaches what's
        // wanted.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if self.response(middle) < wanted {
                low = middle;
            } else {
                high = middle;
            }
        }
        self.zone.turn_at((low + high) / 2.0)
    }
}

impl Macro {
    /// Whether it's as a new track's are: no name, at `00`, holding nothing.
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Its name as shown: its own, or its first mapping's setting's, or none if it holds nothing.
    pub fn shown_name(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.maps.first().map(|map| map.target.name()))
    }

    /// The place among its maps of the one reaching this on its own track.
    pub fn map_of(&self, target: MacroTarget) -> Option<usize> {
        self.map_on(None, target)
    }

    /// The place among its maps of the one reaching this on a track of its group, by the track's
    /// place there, or on its own track with none.
    pub fn map_on(&self, member: Option<u8>, target: MacroTarget) -> Option<usize> {
        self.maps
            .iter()
            .position(|map| map.member == member && map.target == target)
    }

    /// Its maps on its own track, leaving out a group's on its tracks.
    pub fn own_maps(&self) -> impl Iterator<Item = &MacroMap> {
        self.maps.iter().filter(|map| map.member.is_none())
    }
}

/// Whether a name is one a macro can have: from 1 to [`MACRO_NAME_LENGTH`] printable ASCII
/// characters, as the Roto draws only those.
pub fn valid_name(name: &str) -> bool {
    (1..=MACRO_NAME_LENGTH).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
}

impl Track {
    /// Whether any of its macros holds a setting on a track of its group.
    pub fn has_member_maps(&self) -> bool {
        self.macros
            .iter()
            .flat_map(|held| &held.maps)
            .any(|map| map.member.is_some())
    }

    /// Each of its mappings on its own track, with where its macro is turned by hand, on the `00`
    /// to `FF` scale.
    fn own_holds(&self) -> impl Iterator<Item = (f32, &MacroMap)> {
        self.macros.iter().flat_map(|held| {
            let value = f32::from(held.value) / 256.0;
            held.own_maps().map(move |map| (value, map))
        })
    }
}

impl Song {
    /// A sound track's group track and its place among the group's tracks, from 0, if it's in a
    /// group: the place a group's mapping names it by.
    pub fn group_place(&self, track: usize) -> Option<(usize, u8)> {
        let group = self.group_track_of(track)?;
        let place = self.group_members(group).position(|at| at == track)?;
        Some((group, u8::try_from(place).ok()?))
    }

    /// The sound track at a place in a group, as a group's mapping names it.
    pub fn group_member(&self, group: usize, place: u8) -> Option<usize> {
        self.group_members(group).nth(usize::from(place))
    }

    /// Each of its group's macros' mappings on a sound track, with the macro's place, in the
    /// macros' order: none for a track in no group, or whose group's macros hold nothing of it.
    pub fn group_maps_on(&self, track: usize) -> impl Iterator<Item = (usize, &MacroMap)> + '_ {
        self.group_place(track)
            .into_iter()
            .flat_map(move |(group, place)| {
                self.tracks[group]
                    .macros
                    .iter()
                    .enumerate()
                    .flat_map(move |(at, held)| {
                        held.maps
                            .iter()
                            .filter(move |map| map.member == Some(place))
                            .map(move |map| (at, map))
                    })
            })
    }

    /// Which of its group's macros holds a setting on a sound track, and its mapping there, if
    /// one does.
    pub fn group_holding(&self, track: usize, target: MacroTarget) -> Option<(usize, &MacroMap)> {
        self.group_maps_on(track)
            .find(|(_, map)| map.target == target)
    }

    /// Each mapping on a track, its own macros' and its group's, with where its macro is turned
    /// by hand, on the `00` to `FF` scale: its own first, so its group's has the last word, as
    /// the engine puts them.
    fn holds_on(&self, track: usize) -> impl Iterator<Item = (f32, &MacroMap)> + '_ {
        let group = self.group_place(track).map(|(group, _)| &self.tracks[group]);
        let own = self.tracks.get(track).into_iter().flat_map(Track::own_holds);
        let from_group = self.group_maps_on(track).filter_map(move |(at, map)| {
            Some((f32::from(group?.macros[at].value) / 256.0, map))
        });
        own.chain(from_group)
    }

    /// Whether any macro holds a generator's setting on a track: one of its own, or its group's.
    pub(crate) fn has_gen_maps(&self, track: usize) -> bool {
        self.holds_on(track)
            .any(|(_, map)| matches!(map.target, MacroTarget::Gen { .. }))
    }

    /// The shape a live generator of one of a track's phrases is heard with by hand: its own, with
    /// each setting a macro holds on its column, the track's own or its group's, put where the
    /// macro's turn puts it, kept to the setting's bounds in that phrase. None if no macro holds
    /// any of its settings, or only ones its shape hasn't. `steps` are the phrase's, and `source`
    /// the loop a `Follow` reads.
    pub(crate) fn held_shape(
        &self,
        track: usize,
        generator: &Generator,
        steps: &[Step],
        source: Option<&[Step]>,
    ) -> Option<Shape> {
        let column = GenColumn::of(generator.column);
        let held: Vec<(f32, &MacroMap, GenParam)> = self
            .holds_on(track)
            .filter_map(|(value, map)| match map.target {
                MacroTarget::Gen { column: on, param } if on == column => Some((value, map, param)),
                _ => None,
            })
            .collect();
        if held.is_empty() {
            return None;
        }
        let under = generator.under(steps);
        let context = generator.context(&under, &[], source)?;
        let mut shape = generator.shape;
        let mut any = false;
        for (value, map, param) in held {
            if param.exists_for(&shape, context.column, context.generates) {
                param.set_number(&mut shape, &context, map.number_at(value));
                any = true;
            }
        }
        any.then_some(shape)
    }

    /// A track as its macros have it, by hand, its own and its group's: each setting a macro holds
    /// put where the macro's turn puts it, for the pages to show. One no macro holds anything of
    /// is the track itself.
    pub fn track_with_macros(&self, track: usize) -> Cow<'_, Track> {
        let own = self.tracks[track].with_macros();
        if self.group_maps_on(track).next().is_none() {
            return own;
        }
        let mut heard = own.into_owned();
        for (at, map) in self.group_maps_on(track) {
            if let (Some(id), Some((group, _))) = (map.target.setting(), self.group_place(track)) {
                let value = f32::from(self.tracks[group].macros[at].value) / 256.0;
                heard.set_value_of(id, map.at(value));
            }
        }
        Cow::Owned(heard)
    }
}

impl Track {

    /// Where a setting is on this track before any step sets it, with a coarse byte and a fine
    /// one, whoever owns it: a voice keeps its own, an effect's setting is what's in its slot, or
    /// its default while another effect is there, and a mod slot's depth and phase are what its
    /// page has. The engine's held values start here, and a mapping is shown from it.
    pub fn value_of(&self, id: SettingId) -> u16 {
        let modulator = |slot: usize, at: usize| match at {
            0 => depth_value(self.mods[slot].depth),
            1 if self.mods[slot].kind == ModKind::Steps => self.mods[slot].steps.start_value(),
            1 => phase_value(self.mods[slot].lfo.phase),
            2 => u16::from(self.mods[slot].polarity().index()) << 8,
            _ => depth_value(self.mods[slot].lfo.deform),
        };
        let whole = match id.owner() {
            (Owner::Mod(n), at) => return modulator(n, at),
            (Owner::Macro(at), _) => return self.macros[at].value,
            // One its type hasn't is at its default, and does nothing until its type has it.
            (Owner::ModOwn(n), at) => {
                return self.mods[n]
                    .own_value(at)
                    .unwrap_or(u16::from(id.entry().default) << 8);
            }
            (Owner::Accumulation, at) => return self.accumulation.own_value(at),
            (Owner::Arpeggio, at) => return self.arpeggio.own_value(at),
            (Owner::Lane(slot), at) => self.lanes[slot].value(at),
            (Owner::Track, _) => self.own_value(id),
            (Owner::Bypass(slot), _) => u8::from(self.bypassed[slot]),
            (Owner::Voice(_) | Owner::Sampler, _) => self.setting(id),
            (Owner::Slot { slot, effect }, at) => {
                let insert = self.inserts[slot];
                // A distortion keeps every mode's settings, so one its mode doesn't show still
                // reads.
                if insert.kind() == Some(effect) {
                    insert.read(at).unwrap_or(0)
                } else {
                    id.entry().default
                }
            }
        };
        u16::from(whole) << 8 | u16::from(self.fine(id))
    }

    /// One of the track's own settings' coarse byte, from the field that holds it.
    fn own_value(&self, id: SettingId) -> u8 {
        match id {
            SettingId::VOL => UNITY_LEVEL,
            SettingId::LEVEL => self.level,
            SettingId::PAN => self.pan,
            SettingId::CUT => self.cutoff,
            SettingId::RES => self.resonance,
            SettingId::SEND_A => self.send_a,
            SettingId::SEND_B => self.send_b,
            SettingId::PITCH => self.pitch,
            SettingId::VELOCITY_DELTA => self.velocity_delta,
            SettingId::QUANTISE => self.quantise,
            SettingId::JUMP => self.jump,
            SettingId::ORDER => self.order,
            SettingId::REVERSE => self.reverse,
            SettingId::ORDER_X => self.order_x,
            SettingId::ORDER_Y => self.order_y,
            SettingId::FILTER_MODE => self.filter_mode.index(),
            SettingId::GATE_LENGTH => self.gate_length(),
            SettingId::GROOVE => self
                .groove
                .map_or(0, |groove| (groove.index() + 1).min(0xFF) as u8),
            SettingId::SPEED => self.speed.index(),
            SettingId::GLIDE => self.glide,
            SettingId::PHASE => self.phase.index(),
            SettingId::VOICE_MODE => self.voice_mode.index(),
            SettingId::KEY_TRACK => u8::from(self.key_track),
            SettingId::GLIDE_MODE => self.glide_mode.index(),
            SettingId::DRIVE => self.drive,
            SettingId::PUNCH => self.punch,
            SettingId::LOW_CUT => self.low_cut,
            SettingId::HIGH_CUT => self.high_cut,
            SettingId::FILTER_TYPE => self.filter.index(),
            SettingId::SAFE => u8::from(self.safe),
            _ => id.entry().default,
        }
    }

    /// The settings whose [`value_of`](Self::value_of) may differ from what it is on `before`:
    /// each one of what differs between them, a mod slot, an effect or the track's own fields, and
    /// each one either keeps a value or a fine part of that the other doesn't. What's the same
    /// costs comparisons and no look-ups, so a track an edit didn't touch names none, and the
    /// engine, which follows each new version's on the audio thread, needn't go through all of
    /// the settings for it.
    pub fn settings_moved_from(&self, before: &Track) -> SettingSet {
        let mut moved = SettingSet::default();
        differing_keys(&self.settings, &before.settings, &mut moved);
        differing_keys(&self.fine, &before.fine, &mut moved);
        let own = || (0..).map_while(SettingId::track);
        if own().any(|id| self.own_value(id) != before.own_value(id)) {
            moved.extend(own());
        }
        for slot in 0..MOD_SLOTS {
            if self.mods[slot] != before.mods[slot] {
                moved.extend((0..).map_while(|at| SettingId::modulator(slot, at)));
                moved.extend((0..).map_while(|at| SettingId::mod_own(slot, at)));
            }
        }
        for (at, (now, was)) in self.macros.iter().zip(&before.macros).enumerate() {
            if now.value != was.value {
                moved.extend(SettingId::macro_at(at));
            }
        }
        if self.accumulation != before.accumulation {
            moved.extend((0..).map_while(SettingId::accumulation));
        }
        if self.arpeggio != before.arpeggio {
            moved.extend((0..).map_while(SettingId::arpeggio));
        }
        for slot in 0..FX_SLOTS {
            if self.lanes[slot] != before.lanes[slot] {
                moved.extend((0..).map_while(|at| SettingId::lane(slot, at)));
            }
        }
        for slot in 0..INSERTS {
            if self.bypassed[slot] != before.bypassed[slot] {
                moved.extend(SettingId::bypass(slot));
            }
            // Another effect's settings in the slot are at their defaults on both.
            let (now, was) = (self.inserts[slot], before.inserts[slot]);
            if now != was {
                for effect in [now.kind(), was.kind()].into_iter().flatten() {
                    let ids = (0..).map_while(|at| SettingId::slot(slot, effect, at));
                    moved.extend(ids);
                }
            }
        }
        moved
    }

    /// Put a setting at a value, with a coarse byte and a fine one, wherever its owner keeps it,
    /// as a macro would: for the pages to show what a macro gives what it holds. A groove, which
    /// is chosen, and `Vol` and `LDLT`, which the track doesn't keep, are left alone.
    pub(crate) fn set_value_of(&mut self, id: SettingId, value: u16) {
        let [coarse, fine] = value.to_be_bytes();
        let scaled = f32::from(value) / 256.0;
        match id.owner() {
            (Owner::Mod(n), 0) => self.mods[n].depth = depth_of(scaled).round() as i16,
            (Owner::Mod(n), 1) if self.mods[n].kind == ModKind::Steps => {
                let steps = &mut self.mods[n].steps;
                steps.start = steps.start_of(scaled) as u8;
            }
            (Owner::Mod(n), 1) => {
                self.mods[n].lfo.phase = (scaled / 255.0 * 360.0).round() as u16 % 360;
            }
            (Owner::Mod(n), 2) => {
                self.mods[n].polarity = Some(Polarity::ALL[usize::from(coarse.min(1))]);
            }
            (Owner::Mod(n), _) => self.mods[n].lfo.deform = depth_of(scaled).round() as i16,
            (Owner::Macro(at), _) => self.macros[at].value = value,
            (Owner::ModOwn(n), at) => self.mods[n].set_own(at, scaled),
            (Owner::Accumulation, at) => self.accumulation.set_own(at, scaled),
            (Owner::Arpeggio, at) => self.arpeggio.set_own(at, scaled),
            (Owner::Lane(slot), at) => self.lanes[slot].set(at, coarse),
            (Owner::Bypass(slot), _) => self.bypassed[slot] = coarse != 0,
            (Owner::Voice(_) | Owner::Sampler, _) => {
                self.set_setting(id, coarse);
                self.set_fine(id, fine);
            }
            (Owner::Slot { slot, effect }, at) => {
                let insert = self.inserts[slot];
                if insert.kind() == Some(effect) {
                    self.inserts[slot] = insert.write(at, f32::from(coarse));
                }
            }
            (Owner::Track, _) => {
                match id {
                    SettingId::LEVEL => self.level = coarse,
                    SettingId::PAN => self.pan = coarse,
                    SettingId::CUT => self.cutoff = coarse,
                    SettingId::RES => self.resonance = coarse,
                    SettingId::SEND_A => self.send_a = coarse,
                    SettingId::SEND_B => self.send_b = coarse,
                    SettingId::PITCH => self.pitch = coarse,
                    SettingId::VELOCITY_DELTA => self.velocity_delta = coarse,
                    SettingId::QUANTISE => self.quantise = coarse,
                    SettingId::JUMP => self.jump = coarse,
                    SettingId::ORDER => self.order = coarse,
                    SettingId::REVERSE => self.reverse = coarse.min(1),
                    SettingId::ORDER_X => self.order_x = coarse.min(MOST_ORDER_NUMBER),
                    SettingId::ORDER_Y => self.order_y = coarse.min(MOST_ORDER_NUMBER),
                    SettingId::FILTER_MODE => {
                        self.filter_mode =
                            FilterMode::ALL[usize::from(coarse).min(FilterMode::ALL.len() - 1)];
                    }
                    SettingId::GATE_LENGTH => self.gate_length = Some(coarse),
                    SettingId::SPEED => {
                        self.speed = Speed::ALL[usize::from(coarse).min(Speed::ALL.len() - 1)];
                    }
                    SettingId::GLIDE => self.glide = coarse,
                    SettingId::PHASE => self.phase = Phase::from_index(coarse),
                    SettingId::VOICE_MODE => self.voice_mode = VoiceMode::from_index(coarse),
                    SettingId::KEY_TRACK => self.key_track = coarse != 0,
                    SettingId::GLIDE_MODE => self.glide_mode = GlideMode::from_index(coarse),
                    SettingId::DRIVE => self.drive = coarse,
                    SettingId::PUNCH => self.punch = coarse,
                    SettingId::LOW_CUT => self.low_cut = coarse,
                    SettingId::HIGH_CUT => self.high_cut = coarse,
                    SettingId::FILTER_TYPE => self.filter = FilterType::at(coarse),
                    SettingId::SAFE => self.safe = coarse != 0,
                    _ => return,
                }
                self.set_fine(id, fine);
            }
        }
    }

    /// The track as its macros have it, by hand: each setting a macro holds put where the macro's
    /// turn puts it, for the pages to show. One whose macros hold nothing is the track itself.
    pub fn with_macros(&self) -> Cow<'_, Track> {
        if self.own_holds().next().is_none() {
            return Cow::Borrowed(self);
        }
        let mut heard = self.clone();
        for (value, map) in self.own_holds() {
            if let Some(id) = map.target.setting() {
                heard.set_value_of(id, map.at(value));
            }
        }
        Cow::Owned(heard)
    }
}

/// Put in each key one map has a value for that the other hasn't, or has another value for,
/// going through both in order.
fn differing_keys(
    one: &BTreeMap<SettingId, u8>,
    other: &BTreeMap<SettingId, u8>,
    into: &mut SettingSet,
) {
    let (mut one, mut other) = (one.iter().peekable(), other.iter().peekable());
    loop {
        let order = match (one.peek(), other.peek()) {
            (None, None) => return,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some((a, _)), Some((b, _))) => a.cmp(b),
        };
        let (a, b) = match order {
            Ordering::Less => (one.next(), None),
            Ordering::Greater => (None, other.next()),
            Ordering::Equal => (one.next(), other.next()),
        };
        if let Some((id, _)) = a.or(b)
            && a.map(|(_, value)| value) != b.map(|(_, value)| value)
        {
            into.insert(*id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::insert::Insert;
    use crate::machine::{Machine, SynthVoice};

    /// Each setting whose value differs between two tracks, going through every one.
    fn every_moved(now: &Track, was: &Track) -> Vec<SettingId> {
        SettingId::all()
            .filter(|id| now.value_of(*id) != was.value_of(*id))
            .collect()
    }

    /// A track with something of everything set.
    fn busy() -> Track {
        let mut track = Track {
            machine: Machine::Plaits(SynthVoice::FourOp),
            cutoff: 0x60,
            ..Track::default()
        };
        track.inserts[0] = Insert::RETURN_DELAY;
        track.inserts[2] = Insert::RETURN_REVERB;
        track.mods[1].depth = 20;
        track.macros[3].value = 0x4000;
        track.bypassed[2] = true;
        for at in 0..3 {
            let id = SettingId::voice(SynthVoice::FourOp, at).unwrap();
            track.set_setting(id, 0x30 + at as u8);
        }
        track.set_fine(SettingId::CUT, 0x80);
        track
    }

    #[test]
    fn a_track_as_it_was_names_no_settings() {
        let track = busy();
        assert!(track.settings_moved_from(&track.clone()).is_empty());
        let fresh = Track::default();
        assert!(fresh.settings_moved_from(&Track::default()).is_empty());
    }

    #[test]
    fn a_turn_of_the_cutoff_names_only_the_tracks_own() {
        let was = busy();
        let mut now = was.clone();
        now.cutoff += 1;
        let moved = now.settings_moved_from(&was);
        assert!(moved.contains(SettingId::CUT));
        assert!(moved.iter().all(|id| id.owner().0 == Owner::Track));
    }

    #[test]
    fn every_setting_that_moves_is_named() {
        let busy = busy();
        // Each setting turned on its own, wherever its owner keeps it.
        let mut pairs: Vec<(Track, Track)> = SettingId::all()
            .map(|id| {
                let mut now = busy.clone();
                now.set_value_of(id, busy.value_of(id) ^ 0x1180);
                (now, busy.clone())
            })
            .collect();
        // Or only its fine part.
        pairs.extend(SettingId::all().map(|id| {
            let mut now = busy.clone();
            now.set_fine(id, 0x40);
            (now, busy.clone())
        }));
        // And what changes many at once: another machine, effect or mod slot, or a track as new.
        let mut other_machine = busy.clone();
        other_machine.machine = Machine::Plaits(SynthVoice::Chord);
        let mut other_effect = busy.clone();
        other_effect.inserts[0] = Insert::RETURN_REVERB;
        other_effect.inserts[2] = Insert::Empty;
        let mut other_mod = busy.clone();
        other_mod.mods[4].depth = -40;
        other_mod.mods[4].lfo.phase = 90;
        for now in [other_machine, other_effect, other_mod, Track::default()] {
            pairs.push((now.clone(), busy.clone()));
            pairs.push((busy.clone(), now));
        }
        for (now, was) in &pairs {
            let moved = now.settings_moved_from(was);
            for id in every_moved(now, was) {
                assert!(moved.contains(id), "{} moved and isn't named", id.key());
            }
        }
    }

    #[test]
    fn only_a_generators_list_settings_are_reached_by_hand_alone() {
        let on = |param| MacroTarget::Gen {
            column: GenColumn::Note,
            param,
        };
        assert!(on(GenParam::Wave).by_hand());
        assert!(on(GenParam::Rate).by_hand());
        for param in [
            GenParam::Hits,
            GenParam::Density,
            GenParam::To,
            GenParam::Vary,
        ] {
            assert!(!on(param).by_hand(), "{param:?}");
        }
        assert!(!MacroTarget::Setting(SettingId::CUT).by_hand());
    }

    #[test]
    fn a_map_goes_from_its_min_at_00_to_its_max_at_ff() {
        let map = MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x3000,
            max: 0xA000,
            curve: 0,
            s: 0,
            zone: Default::default(),
            member: None,
        };
        assert_eq!(map.at(0.0), 0x3000);
        assert_eq!(map.at(255.0), 0xA000);
        assert_eq!(map.at(127.5), 0x6800);
        assert_eq!(map.at(300.0), 0xA000, "kept to its range");
        let back = MacroMap {
            min: 0xA000,
            max: 0x3000,
            ..map
        };
        assert_eq!(back.at(0.0), 0xA000, "min above max turns it the other way");
        assert_eq!(back.at(255.0), 0x3000);
    }

    #[test]
    fn a_setting_with_no_fine_part_moves_in_whole_steps() {
        let map = MacroMap::whole(MacroTarget::Setting(SettingId::FILTER_MODE));
        let top = top_value(SettingId::FILTER_MODE.entry().kind);
        assert_eq!(map.max, top);
        for value in [0.0, 30.0, 100.0, 200.0, 255.0] {
            assert_eq!(map.at(value) & 0xFF, 0, "{value}");
        }
    }

    /// Mutate on the left half, coming down from its top to 0, and Jump on the right, going up
    /// from 0 to its top, on one macro, as Marbles' Déjà vu.
    fn deja_vu() -> (MacroMap, MacroMap) {
        use crate::modulation::ModField;
        let own = |field: ModField| {
            MacroTarget::Setting(
                SettingId::mod_own(0, field.own(crate::modulation::ModKind::Steps).unwrap())
                    .unwrap(),
            )
        };
        let mutate = MacroMap::whole(own(ModField::Mutate));
        let mutate = MacroMap {
            min: mutate.max,
            max: mutate.min,
            ..mutate
        }
        .zoned(Zone::new(0x00, 0x7F).unwrap());
        let jump = MacroMap::whole(own(ModField::Jump)).zoned(Zone::new(0x80, 0xFF).unwrap());
        (mutate, jump)
    }

    #[test]
    fn the_whole_turn_is_the_zone_a_map_had_before_zones_to_the_bit() {
        let map = MacroMap::whole(MacroTarget::Setting(SettingId::CUT));
        assert_eq!(map.zone, Zone::WHOLE);
        for (curve, s) in [(0, 0), (60, 0), (-40, 70)] {
            let map = map.bent(curve, s);
            for step in 0..=1020 {
                let value = step as f32 / 4.0;
                let along = map.response((value / 255.0).clamp(0.0, 1.0));
                let before = (f32::from(map.max) * along).round() as u16;
                assert_eq!(map.at(value), before, "{curve} {s} {value}");
            }
            for value in [0, 0x1234, 0x8000, 0xFF00] {
                let turned = map.macro_for(value);
                let back = map.at(f32::from(turned) / 256.0);
                assert!(back.abs_diff(value) <= 0x100, "{value:04X}: {back:04X}");
            }
        }
        // Whole, it isn't written; a part is.
        let text = ron::to_string(&map).unwrap();
        assert!(!text.contains("zone"), "{text}");
        let zoned = map.zoned(Zone::new(0x80, 0xFF).unwrap());
        let text = ron::to_string(&zoned).unwrap();
        assert!(text.contains("zone:(start:128,end:255)"), "{text}");
        assert_eq!(ron::from_str::<MacroMap>(&text).unwrap(), zoned);
    }

    #[test]
    fn a_left_half_and_a_right_half_make_one_macro_the_deja_vu_knob() {
        let (mutate, jump) = deja_vu();
        let (top, jump_top) = (mutate.min, jump.max);
        assert!(top > 0 && jump_top > 0);
        // At 00 Mutate is at its top and Jump at 0; both are 0 at the middle; Jump is at its top
        // at FF.
        assert_eq!((mutate.at(0.0), jump.at(0.0)), (top, 0));
        assert_eq!((mutate.at(127.0), jump.at(127.0)), (0, 0));
        assert_eq!((mutate.at(128.0), jump.at(128.0)), (0, 0));
        assert_eq!((mutate.at(255.0), jump.at(255.0)), (0, jump_top));
        // Halfway across each half, each is halfway along its range.
        let half = |top: u16| f32::from(top) / 2.0;
        assert!((f32::from(mutate.at(63.5)) - half(top)).abs() <= 256.0);
        assert!((f32::from(jump.at(191.5)) - half(jump_top)).abs() <= 256.0);
    }

    #[test]
    fn outside_its_zone_a_map_holds_at_the_nearer_end() {
        let map = MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x3000,
            max: 0xA000,
            curve: 40,
            s: 0,
            zone: Zone::new(0x40, 0xC0).unwrap(),
            member: None,
        };
        for value in [0.0, 20.0, 63.9, 64.0] {
            assert_eq!(map.at(value), 0x3000, "below, at Min: {value}");
        }
        for value in [192.0, 200.0, 255.0] {
            assert_eq!(map.at(value), 0xA000, "above, at Max: {value}");
        }
        let middle = map.at(128.0);
        assert!(middle > 0x6800, "bent early across the zone: {middle:04X}");
        // Turned to where a setting is, a macro lands inside the zone.
        let turned = map.macro_for(0x6800);
        assert!((0x4000..=0xC000).contains(&turned), "{turned:04X}");
        assert_eq!(map.at(f32::from(turned) / 256.0) >> 8, 0x68);
        assert_eq!(
            map.macro_for(0x1000),
            0x4000,
            "below its range, the zone's start"
        );
    }

    #[test]
    fn a_zone_starts_below_its_end_or_is_refused() {
        assert_eq!(Zone::new(0x80, 0x80), None);
        assert_eq!(Zone::new(0x90, 0x80), None);
        assert_eq!(
            Zone::new(0x7F, 0x80).map(Zone::text).as_deref(),
            Some("7F–80")
        );
        let map = MacroMap::whole(MacroTarget::Setting(SettingId::CUT));
        let empty = MacroMap {
            zone: Zone {
                start: 0x80,
                end: 0x80,
            },
            ..map
        };
        assert!(!empty.in_range(), "an action refuses it");
        let text = format!(
            "(target:Setting(\"{}\"),min:0,max:65280,zone:(start:200,end:100))",
            SettingId::CUT.key()
        );
        let error = ron::from_str::<MacroMap>(&text).unwrap_err().to_string();
        assert!(error.contains("C8 to 64"), "{error}");
    }

    #[test]
    fn a_macro_turned_to_where_its_setting_is_puts_it_there() {
        let map = MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x2000,
            max: 0xE000,
            curve: 0,
            s: 0,
            zone: Default::default(),
            member: None,
        };
        let turned = map.macro_for(0x8000);
        assert_eq!(map.at(f32::from(turned) / 256.0), 0x8000);
    }

    #[test]
    fn a_macro_is_named_by_its_first_mapping_until_it_has_a_name() {
        let mut held = Macro::default();
        assert!(held.is_empty());
        assert_eq!(held.shown_name(), None);
        held.maps
            .push(MacroMap::whole(MacroTarget::Setting(SettingId::CUT)));
        assert_eq!(held.shown_name().as_deref(), Some("Cut"));
        held.name = Some("Bite".to_owned());
        assert_eq!(held.shown_name().as_deref(), Some("Bite"));
        assert!(valid_name("Bite"));
        assert!(!valid_name(""));
        assert!(!valid_name("Thirteen long"));
        assert!(!valid_name("Café"));
    }

    #[test]
    fn every_setting_a_macro_can_hold_is_put_where_it_reads_back() {
        let mut track = Track::default();
        track.inserts[1] = crate::insert::Insert::ALL[2];
        track.mods[4].kind = ModKind::Steps;
        // `Vol` and `LDLT` the track doesn't keep, as they're stages over its own, and a groove is
        // chosen.
        let unkept = [SettingId::VOL, SettingId::GATE_DELTA, SettingId::GROOVE];
        let kept = |id: &SettingId| !unkept.contains(id);
        for id in SettingId::for_macro(&track).into_iter().filter(kept) {
            let kind = id.entry().kind;
            let top = top_value(kind);
            // A phase's top is a whole turn, so back at its start.
            let ends = if kind.wraps() { top - 0x0100 } else { top };
            for value in [0, top / 2, ends] {
                let value = if kind.has_fine() {
                    value
                } else {
                    value & 0xFF00
                };
                let mut set = track.clone();
                set.set_value_of(id, value);
                let back = set.value_of(id);
                // Lists and whole steps land on a step, Steps' start in the middle of its share,
                // and depths, phases and times near; and where it lands, it stays.
                assert!(
                    back.abs_diff(value) <= 0x1000,
                    "{} at {value:04X} read back as {back:04X}",
                    id.key()
                );
                set.set_value_of(id, back);
                assert_eq!(set.value_of(id), back, "{} stays", id.key());
            }
        }
    }

    #[test]
    fn a_track_with_its_macros_has_each_setting_where_its_macro_puts_it() {
        let mut track = Track::default();
        assert!(matches!(track.with_macros(), Cow::Borrowed(_)));
        track.macros[1].value = 0xFF00;
        track.macros[1].maps.push(MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x2000,
            max: 0x6000,
            curve: 0,
            s: 0,
            zone: Default::default(),
            member: None,
        });
        assert_eq!(track.with_macros().cutoff, 0x60);
        assert_eq!(track.cutoff, 0xFF, "the track keeps its own");
    }

    #[test]
    fn a_macro_cant_reach_a_macro() {
        let other = MacroTarget::Setting(SettingId::macro_at(2).unwrap());
        assert!(!other.mappable());
        assert!(!MacroMap::whole(other).in_range());
        assert!(MacroMap::whole(MacroTarget::Setting(SettingId::RES)).in_range());
    }

    /// A mapping of `Cut` from `00` to `7F`, bent so.
    fn to_7f(curve: i32, s: i32) -> MacroMap {
        MacroMap {
            max: 0x7F00,
            ..MacroMap::whole(MacroTarget::Setting(SettingId::CUT))
        }
        .bent(curve, s)
    }

    /// Where it puts its setting, to the nearest coarse step, with its macro at each of so many
    /// places evenly from `00` to `FF`.
    fn coarse(map: MacroMap, places: usize) -> Vec<u8> {
        (0..places)
            .map(|at| {
                let value = 255.0 * at as f32 / (places - 1) as f32;
                ((u32::from(map.at(value)) + 0x80) >> 8) as u8
            })
            .collect()
    }

    #[test]
    fn a_straight_mapping_is_the_line_it_always_was_and_saves_as_before() {
        for (min, max) in [
            (0, 0xFF00),
            (0x3000, 0xA000),
            (0xA000, 0x3000),
            (0x1234, 0x1234),
        ] {
            let map = MacroMap {
                min,
                max,
                ..MacroMap::whole(MacroTarget::Setting(SettingId::CUT))
            };
            assert!(map.is_straight());
            for quarter in 0..=1020 {
                let value = quarter as f32 / 4.0;
                let (low, high) = (f32::from(min), f32::from(max));
                let line = (low + (high - low) * (value / 255.0)).round() as u16;
                assert_eq!(map.at(value), line, "{min:04X} to {max:04X} at {value}");
            }
        }
        let text = ron::to_string(&MacroMap::whole(MacroTarget::Setting(SettingId::CUT))).unwrap();
        assert!(!text.contains("curve") && !text.contains("s:"), "{text}");
        let bent = to_7f(40, -20);
        let text = ron::to_string(&bent).unwrap();
        assert!(text.contains("curve:40,s:-20"), "{text}");
        assert_eq!(ron::from_str::<MacroMap>(&text).unwrap(), bent);
    }

    #[test]
    fn curve_and_s_bend_a_mapping_as_they_bend_the_shape_panels_ramp() {
        // The shape panel's own numbers for a ramp over 9 rows from 00 to 7F, with `Curve +60`
        // and with `S +60` (the shape's `a_curve_reaches_the_values_the_design_gives`).
        assert_eq!(
            coarse(to_7f(60, 0), 9),
            [0x00, 0x2E, 0x49, 0x5A, 0x66, 0x6E, 0x75, 0x7B, 0x7F]
        );
        assert_eq!(
            coarse(to_7f(0, 60), 9),
            [0x00, 0x05, 0x0D, 0x1B, 0x40, 0x64, 0x72, 0x7A, 0x7F]
        );
        for curve in [-100, -60, -1, 0, 1, 60, 100] {
            for s in [-100, -60, 0, 60, 100] {
                let map = to_7f(curve, s);
                for at in 0..=64 {
                    let along = at as f32 / 64.0;
                    let ramp = curved(f64::from(along), curve as i8, s as i8) as f32;
                    assert!((map.response(along) - ramp).abs() < 1e-6, "{curve} {s}");
                }
                let out = coarse(map, 33);
                assert_eq!((out[0], out[32]), (0x00, 0x7F), "{curve} {s}: its ends");
                assert!(out.windows(2).all(|pair| pair[0] <= pair[1]), "{out:?}");
            }
        }
        assert_eq!((to_7f(300, -300).curve, to_7f(300, -300).s), (100, -100));
    }

    #[test]
    fn an_inverted_mapping_bends_the_same_way_along_its_turn() {
        for (curve, s) in [(60, 0), (-60, 0), (0, 60), (40, -30), (-100, 100)] {
            let up = to_7f(curve, s);
            let down = MacroMap {
                min: up.max,
                max: up.min,
                ..up
            };
            for at in 0..=32 {
                let value = 255.0 * at as f32 / 32.0;
                // As far from its own min as the upright one is from its, the other way.
                let sum = i32::from(up.at(value)) + i32::from(down.at(value));
                assert!((sum - 0x7F00).abs() <= 1, "{curve} {s} at {value}");
            }
        }
        // `Curve +60` from 7F down to 00 gets most of the way early, as it does going up.
        let down = MacroMap {
            min: 0x7F00,
            max: 0,
            ..to_7f(60, 0)
        };
        assert_eq!(
            coarse(down, 9),
            [0x7F, 0x51, 0x36, 0x25, 0x19, 0x11, 0x0A, 0x04, 0x00]
        );
    }

    #[test]
    fn a_bent_macro_turned_to_where_its_setting_is_still_puts_it_there() {
        let map = MacroMap {
            min: 0x2000,
            max: 0xE000,
            ..to_7f(60, -40)
        };
        for value in [0x2000, 0x3000, 0x5000, 0x8000, 0xC000, 0xE000] {
            let turned = map.macro_for(value);
            let back = map.at(f32::from(turned) / 256.0);
            assert!(back.abs_diff(value) <= 0x40, "{value:04X}: {back:04X}");
        }
    }
}
