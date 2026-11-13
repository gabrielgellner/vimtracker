//! A track's eight macros, after Ableton Live's Rack macros (`docs/designs/generators-page.md`
//! §4, decision 282): each is one knob that turns any number of the track's settings at once,
//! each between a min and a max of its own. A macro is itself a setting, `MAC1` to `MAC8`, which a
//! step locks and modulation moves, and it's absolute: at `00` each setting it holds is at its
//! min and at `FF` at its max.

use serde::{Deserialize, Serialize};

use crate::settings::{Kind, Owner, SettingId};

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

/// One setting a macro turns, and how far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroMap {
    pub target: MacroTarget,
    /// What the setting is with the macro at `00` and at `FF`, in the setting's own values, with
    /// a coarse byte and a fine one. `min` above `max` turns it the other way.
    pub min: u16,
    pub max: u16,
}

/// What a mapping reaches. Keyed by what it is rather than where, so a mapping saved with a sound
/// takes hold of the same setting wherever it's loaded. Generator settings will join it, keyed by
/// their column, when macros reach them (the design's §4.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MacroTarget {
    /// A setting of the registry: the track's own, its machine's, an effect's, a mod slot's, the
    /// accumulator's or the arpeggio's. Never a macro.
    Setting(SettingId),
}

impl MacroTarget {
    /// Whether a macro can reach it: anything in the registry but another macro.
    pub fn mappable(self) -> bool {
        match self {
            Self::Setting(id) => !matches!(id.owner().0, Owner::Macro(_)),
        }
    }

    /// The setting it is.
    pub fn setting(self) -> SettingId {
        match self {
            Self::Setting(id) => id,
        }
    }
}

/// The highest value a setting takes, with its coarse byte and fine one: a fine part goes no
/// further than the top's coarse value.
pub fn top_value(kind: Kind) -> u16 {
    u16::from(kind.top()) << 8
}

impl MacroMap {
    /// A mapping over a setting's whole range, from its bottom to its top, as a new mapping
    /// starts, as Live's do.
    pub fn whole(target: MacroTarget) -> Self {
        let MacroTarget::Setting(id) = target;
        Self {
            target,
            min: 0,
            max: top_value(id.entry().kind),
        }
    }

    /// Whether its ends are values its setting takes.
    pub fn in_range(&self) -> bool {
        let top = top_value(self.target.setting().entry().kind);
        self.target.mappable() && self.min <= top && self.max <= top
    }

    /// What it puts its setting at with its macro at `value`, on the `00` to `FF` scale with any
    /// fine part as the fraction: from `min` at `00` to `max` at `FF`, in a straight line, and to
    /// a whole step for a setting with no fine part, as a list's or the level scale's.
    pub fn at(&self, value: f32) -> u16 {
        let kind = self.target.setting().entry().kind;
        let along = (value / 255.0).clamp(0.0, 1.0);
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        let to = (min + (max - min) * along)
            .round()
            .clamp(0.0, f32::from(top_value(kind)));
        if kind.has_fine() {
            to as u16
        } else {
            ((to / 256.0).round() as u16) << 8
        }
    }

    /// Where a macro would put its setting at this value, inverted: the macro's value, on the
    /// `0000` to `FF00` scale, that puts the setting nearest `value`, for a first mapping that
    /// turns its macro to where the setting is so nothing jumps.
    pub fn macro_for(&self, value: u16) -> u16 {
        if self.min == self.max {
            return 0;
        }
        let (min, max) = (f32::from(self.min), f32::from(self.max));
        let along = ((f32::from(value) - min) / (max - min)).clamp(0.0, 1.0);
        (along * f32::from(0xFF00_u16)).round() as u16
    }
}

impl Macro {
    /// Whether it's as a new track's are: no name, at `00`, holding nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Its name as shown: its own, or its first mapping's setting's, or none if it holds nothing.
    pub fn shown_name(&self) -> Option<String> {
        self.name.clone().or_else(|| {
            self.maps
                .first()
                .map(|map| map.target.setting().name().to_owned())
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_goes_from_its_min_at_00_to_its_max_at_ff() {
        let map = MacroMap {
            target: MacroTarget::Setting(SettingId::CUT),
            min: 0x3000,
            max: 0xA000,
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
    fn a_macro_cant_reach_a_macro() {
        let other = MacroTarget::Setting(SettingId::macro_at(2).unwrap());
        assert!(!other.mappable());
        assert!(!MacroMap::whole(other).in_range());
        assert!(MacroMap::whole(MacroTarget::Setting(SettingId::RES)).in_range());
    }
}
