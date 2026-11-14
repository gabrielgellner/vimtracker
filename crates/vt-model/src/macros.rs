//! A track's eight macros, after Ableton Live's Rack macros (`docs/designs/generators-page.md`
//! §4, decision 282): each is one knob that turns any number of the track's settings at once,
//! each between a min and a max of its own. A macro is itself a setting, `MAC1` to `MAC8`, which a
//! step locks and modulation moves, and it's absolute: at `00` each setting it holds is at its
//! min and at `FF` at its max.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::constants::UNITY_LEVEL;
use crate::generator::Generator;
use crate::lfo::phase_value;
use crate::modulation::{ModKind, Polarity, depth_of, depth_value};
use crate::settings::{Effect, Kind, Owner, SettingId};
use crate::shape::{GenParam, MAX_BEND, Shape, ShapeColumn, curved};
use crate::song::{FilterMode, Track};
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
    /// Its response from `min` to `max`, bent as the shape panel's ramp is (decision 314):
    /// `curve` bends it, above 0 moving early and below 0 late, and `s` then eases both ends,
    /// above 0 an S and below 0 the reverse, each from -100 to 100 and 0 straight, as a new
    /// mapping is.
    #[serde(default, skip_serializing_if = "is_straight")]
    pub curve: i8,
    #[serde(default, skip_serializing_if = "is_straight")]
    pub s: i8,
}

/// What a mapping reaches. Keyed by what it is rather than where, so a mapping saved with a sound
/// takes hold of the same setting wherever it's loaded, and a generator's by its column, so it
/// takes hold of whichever phrase's generator is on that column (the design's §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MacroTarget {
    /// A setting of the registry: the track's own, its machine's, an effect's, a mod slot's, the
    /// accumulator's or the arpeggio's. Never a macro.
    Setting(SettingId),
    /// A setting of the live generator on a column, in every phrase of the track with one there
    /// (decision 294). Its ends are numbers in the setting's own, as [`GenParam::number`] gives
    /// them, set off by [`GEN_ZERO`]. A turn writes the column again, and the engine holds the
    /// setting where a lock or modulation puts the macro as it plays (decision 300).
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

    pub fn serialize<S: Serializer>(param: &GenParam, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(param.key())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<GenParam, D::Error> {
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
    /// doesn't reach (decisions 294 and 300).
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
        }
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
    pub fn response(&self, along: f32) -> f32 {
        response(along, self.curve, self.s)
    }

    /// Whether its ends are values its setting takes, and its `curve` and `s` within -100 to 100.
    /// A generator's ends are kept to its bounds as they're used, as those hang on the phrase
    /// it's in.
    pub fn in_range(&self) -> bool {
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
        self.target.mappable() && fits && bends
    }

    /// What it puts its setting at with its macro at `value`, on the `00` to `FF` scale with any
    /// fine part as the fraction: from `min` at `00` to `max` at `FF`, along its
    /// [response](MacroMap::response), and to a whole step for a setting with no fine part, as a
    /// list's or the level scale's. A generator's is its number set off by [`GEN_ZERO`], to the
    /// nearest whole one.
    pub fn at(&self, value: f32) -> u16 {
        let along = self.response((value / 255.0).clamp(0.0, 1.0));
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
    /// turns its macro to where the setting is so nothing jumps.
    pub fn macro_for(&self, value: u16) -> u16 {
        if self.min == self.max {
            return 0;
        }
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        let wanted = ((f32::from(value) - min) / (max - min)).clamp(0.0, 1.0);
        if self.is_straight() {
            return (wanted * f32::from(0xFF00_u16)).round() as u16;
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
        ((low + high) / 2.0 * f32::from(0xFF00_u16)).round() as u16
    }
}

impl Macro {
    /// Whether it's as a new track's are: no name, at `00`, holding nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Its name as shown: its own, or its first mapping's setting's, or none if it holds nothing.
    pub fn shown_name(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.maps.first().map(|map| map.target.name()))
    }

    /// The place among its maps of the one reaching this.
    pub fn map_of(&self, target: MacroTarget) -> Option<usize> {
        self.maps.iter().position(|map| map.target == target)
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
    /// Whether any of its macros holds a generator's setting.
    pub fn has_gen_maps(&self) -> bool {
        self.macros
            .iter()
            .flat_map(|held| &held.maps)
            .any(|map| matches!(map.target, MacroTarget::Gen { .. }))
    }

    /// The shape a live generator of one of its phrases is heard with by hand (decision 294): its
    /// own, with each setting a macro holds on its column put where the macro's turn puts it, kept
    /// to the setting's bounds in that phrase. None if no macro holds any of its settings, or
    /// only ones its shape hasn't. `steps` are the phrase's, and `source` the loop a `Follow`
    /// reads.
    pub fn held_shape(
        &self,
        generator: &Generator,
        steps: &[Step],
        source: Option<&[Step]>,
    ) -> Option<Shape> {
        let column = GenColumn::of(generator.column);
        let held: Vec<(f32, &MacroMap, GenParam)> = self
            .macros
            .iter()
            .flat_map(|held| {
                let value = f32::from(held.value) / 256.0;
                held.maps.iter().filter_map(move |map| match map.target {
                    MacroTarget::Gen { column: on, param } if on == column => {
                        Some((value, map, param))
                    }
                    _ => None,
                })
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
            (Owner::Track, _) => match id {
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
                SettingId::FILTER_MODE => self.filter_mode.index(),
                SettingId::GATE_LENGTH => self.gate_length(),
                SettingId::GROOVE => self
                    .groove
                    .map_or(0, |groove| (groove.index() + 1).min(0xFF) as u8),
                SettingId::SPEED => self.speed.index(),
                SettingId::GLIDE => self.glide,
                _ => id.entry().default,
            },
            (Owner::Bypass(slot), _) => u8::from(self.bypassed[slot]),
            (Owner::Voice(_) | Owner::Sampler, _) => self.setting(id),
            (Owner::Slot { slot, effect }, at) => {
                let insert = self.inserts[slot];
                // A distortion keeps every mode's settings, so one its mode doesn't show still
                // reads.
                if Effect::of(insert) == Some(effect) {
                    insert.read(effect.params()[at].setting).unwrap_or(0)
                } else {
                    id.entry().default
                }
            }
        };
        u16::from(whole) << 8 | u16::from(self.fine(id))
    }

    /// Put a setting at a value, with a coarse byte and a fine one, wherever its owner keeps it,
    /// as a macro would: for the pages to show what a macro gives what it holds. A groove, which
    /// is chosen, and `Vol` and `LDLT`, which the track doesn't keep, are left alone.
    pub fn set_value_of(&mut self, id: SettingId, value: u16) {
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
            (Owner::Bypass(slot), _) => self.bypassed[slot] = coarse != 0,
            (Owner::Voice(_) | Owner::Sampler, _) => {
                self.set_setting(id, coarse);
                self.set_fine(id, fine);
            }
            (Owner::Slot { slot, effect }, at) => {
                let insert = self.inserts[slot];
                if Effect::of(insert) == Some(effect) {
                    self.inserts[slot] =
                        insert.write(effect.params()[at].setting, f32::from(coarse));
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
                    SettingId::FILTER_MODE => {
                        self.filter_mode =
                            FilterMode::ALL[usize::from(coarse).min(FilterMode::ALL.len() - 1)];
                    }
                    SettingId::GATE_LENGTH => self.gate_length = Some(coarse),
                    SettingId::SPEED => {
                        self.speed = Speed::ALL[usize::from(coarse).min(Speed::ALL.len() - 1)];
                    }
                    SettingId::GLIDE => self.glide = coarse,
                    _ => return,
                }
                self.set_fine(id, fine);
            }
        }
    }

    /// The track as its macros have it, by hand: each setting a macro holds put where the macro's
    /// turn puts it, for the pages to show. One whose macros hold nothing is the track itself.
    pub fn with_macros(&self) -> Cow<'_, Track> {
        if !self.has_macro_maps() {
            return Cow::Borrowed(self);
        }
        let mut heard = self.clone();
        for held in &self.macros {
            let value = f32::from(held.value) / 256.0;
            for map in &held.maps {
                if let Some(id) = map.target.setting() {
                    heard.set_value_of(id, map.at(value));
                }
            }
        }
        Cow::Owned(heard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_macro_turned_to_where_its_setting_is_puts_it_there() {
        let map = MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x2000,
            max: 0xE000,
            curve: 0,
            s: 0,
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
