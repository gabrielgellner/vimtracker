//! A track's mod slots: six modulators, each of a type chosen for it, which work out in order so
//! each can move only those after it (`docs/designs/mod-slots.md`). Every slot has a type, where
//! it goes, how far and a switch, and keeps each type's own settings, so trying another type and
//! coming back finds the first as it was.

use serde::{Deserialize, Serialize};

use crate::envelope::Adsr;
use crate::lfo::Lfo;
use crate::settings::SettingId;

/// How many mod slots a track has.
pub const MOD_SLOTS: usize = 6;
/// The furthest either way a slot's depth goes, in the units of what it moves.
pub const MAX_DEPTH: i16 = 0xFF;

/// What a mod slot is. ADSR and LFO are the first; AHD, Drum, LPG, Steps and Track follow in M20.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum ModKind {
    #[default]
    Adsr,
    Lfo,
}

impl ModKind {
    /// Every type, in the order the type picker lists them.
    pub const ALL: [Self; 2] = [Self::Adsr, Self::Lfo];

    /// The name in the slot's title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Adsr => "ADSR",
            Self::Lfo => "LFO",
        }
    }

    /// What it does, for the type picker.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Adsr => "held while the note is",
            Self::Lfo => "a cycle, free or from each note",
        }
    }

    /// Whether it's an envelope, which each note starts and which can shape the notes themselves.
    pub fn is_envelope(self) -> bool {
        matches!(self, Self::Adsr)
    }

    /// Whether it has a mode, which its title shows after it.
    pub fn has_mode(self) -> bool {
        matches!(self, Self::Lfo)
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// One mod slot: its type, where it goes, how far and its switch, which every type shares, and each
/// type's own settings, kept while another type plays. A song saves a type's settings only where
/// they differ from that type's defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModSlot {
    pub kind: ModKind,
    pub destination: Option<SettingId>,
    /// How far it moves its destination, in that destination's units, with semitones for `Pitch`.
    /// Negative flips it. The slot that shapes each note has none.
    pub depth: i16,
    /// Whether it's switched out, to hear the track without it. It keeps running underneath, so
    /// it comes back where it would have been. The note-shaping slot leaves notes the plain shape
    /// a new track's have.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bypassed: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub adsr: Adsr,
    #[serde(default, skip_serializing_if = "is_default")]
    pub lfo: Lfo,
}

impl ModSlot {
    /// An ADSR going somewhere, with no depth yet.
    pub fn adsr(destination: Option<SettingId>, adsr: Adsr) -> Self {
        Self {
            kind: ModKind::Adsr,
            destination,
            depth: 0,
            bypassed: false,
            adsr,
            lfo: Lfo::default(),
        }
    }

    /// An LFO going somewhere, with no depth yet.
    pub fn lfo(destination: Option<SettingId>, lfo: Lfo) -> Self {
        Self {
            kind: ModKind::Lfo,
            destination,
            depth: 0,
            bypassed: false,
            adsr: Adsr::default(),
            lfo,
        }
    }

    /// The same, moving its destination this far.
    #[must_use]
    pub fn with_depth(self, depth: i16) -> Self {
        Self { depth, ..self }
    }

    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.depth.abs() <= MAX_DEPTH && self.adsr.in_range() && self.lfo.in_range()
    }

    /// The name of its mode, for a type that has one.
    pub fn mode_name(&self) -> Option<&'static str> {
        match self.kind {
            ModKind::Adsr => None,
            ModKind::Lfo => Some(self.lfo.mode.name()),
        }
    }
}

/// What a new track's slots start as, today's three envelopes and three LFOs: the first envelope
/// shapes each note and the second waits on the filter, and the rest start with nowhere to go.
pub fn default_mods() -> [ModSlot; MOD_SLOTS] {
    std::array::from_fn(default_mod)
}

/// What one slot starts as, and goes back to when its type is cleared.
pub fn default_mod(slot: usize) -> ModSlot {
    match slot {
        0 => ModSlot::adsr(Some(SettingId::VOL), Adsr::note_shape()),
        1 => ModSlot::adsr(Some(SettingId::CUT), Adsr::decay()),
        2 => ModSlot::adsr(None, Adsr::decay()),
        _ => ModSlot::lfo(None, Lfo::default()),
    }
}

/// The slot that shapes each note's level, if one does: the first envelope on `Vol`, switched in or
/// not, as one switched out leaves notes the plain shape rather than handing the job on.
pub fn note_shape_slot(mods: &[ModSlot; MOD_SLOTS]) -> Option<usize> {
    mods.iter()
        .position(|slot| slot.kind.is_envelope() && slot.destination == Some(SettingId::VOL))
}

/// A depth as a setting's value, with a coarse byte and a fine one: `8000` is none, and `00` and
/// `FF` are the furthest either way. The sides have their own scales, as there are 128 coarse
/// values below the middle and 127 above.
pub fn depth_value(depth: i16) -> u16 {
    let depth = f64::from(depth.clamp(-MAX_DEPTH, MAX_DEPTH));
    let side = if depth < 0.0 { 128.0 } else { 127.0 };
    (32768.0 + depth / f64::from(MAX_DEPTH) * side * 256.0).round() as u16
}

/// A depth from a value on the `00` to `FF` scale, the inverse of [`depth_value`].
pub fn depth_of(value: f32) -> f32 {
    let from_middle = value - 128.0;
    let side = if from_middle < 0.0 { 128.0 } else { 127.0 };
    from_middle / side * f32::from(MAX_DEPTH)
}

/// A depth as text, with its sign, in hex like the values it moves.
pub fn depth_text(depth: i16) -> String {
    match depth {
        0 => "00".to_owned(),
        depth if depth < 0 => format!("-{:02X}", depth.unsigned_abs()),
        depth => format!("+{depth:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lfo::LfoMode;

    #[test]
    fn depth_goes_there_and_back_as_a_value() {
        assert_eq!(depth_value(0), 0x8000);
        assert_eq!(depth_value(MAX_DEPTH), 0xFF00);
        assert_eq!(depth_value(-MAX_DEPTH), 0x0000);
        for depth in [-255, -40, -1, 0, 1, 0x40, 255] {
            let back = depth_of(f32::from(depth_value(depth)) / 256.0);
            assert!(
                (back - f32::from(depth)).abs() < 0.01,
                "{depth} came back as {back}"
            );
        }
    }

    #[test]
    fn every_whole_depth_comes_back_whole() {
        for depth in -MAX_DEPTH..=MAX_DEPTH {
            let back = depth_of(f32::from(depth_value(depth)) / 256.0).round();
            assert_eq!(back, f32::from(depth));
        }
    }

    #[test]
    fn depth_shows_its_sign_in_hex() {
        assert_eq!(depth_text(0), "00");
        assert_eq!(depth_text(0x40), "+40");
        assert_eq!(depth_text(-0xFF), "-FF");
    }

    #[test]
    fn the_defaults_are_three_envelopes_then_three_lfos() {
        let mods = default_mods();
        let kinds = mods.map(|slot| slot.kind);
        assert_eq!(kinds[..3], [ModKind::Adsr; 3]);
        assert_eq!(kinds[3..], [ModKind::Lfo; 3]);
        assert_eq!(mods[0].adsr, Adsr::note_shape());
        assert_eq!(mods[1].destination, Some(SettingId::CUT));
        assert_eq!(mods[2].destination, None);
        assert!(mods.iter().all(|slot| slot.depth == 0 && !slot.bypassed));
        assert_eq!(note_shape_slot(&mods), Some(0));
    }

    #[test]
    fn the_first_envelope_on_vol_shapes_the_notes_wherever_it_is() {
        let mut mods = default_mods();
        mods[0] = ModSlot::lfo(Some(SettingId::VOL), Lfo::default());
        assert_eq!(note_shape_slot(&mods), None);
        mods[2].destination = Some(SettingId::VOL);
        assert_eq!(note_shape_slot(&mods), Some(2));
        mods[2].bypassed = true;
        assert_eq!(
            note_shape_slot(&mods),
            Some(2),
            "switched out still holds it"
        );
    }

    #[test]
    fn a_slot_saves_only_the_settings_that_differ_from_its_types() {
        let mods = default_mods();
        let text = ron::to_string(&mods[3]).unwrap();
        assert_eq!(text, "(kind:Lfo,destination:None,depth:0)");
        let trig = ModSlot {
            lfo: Lfo {
                mode: LfoMode::Trig,
                ..Lfo::default()
            },
            ..mods[3]
        };
        let text = ron::to_string(&trig).unwrap();
        assert!(text.contains("mode:Trig"), "{text}");
        assert_eq!(ron::from_str::<ModSlot>(&text).unwrap(), trig);
        let text = ron::to_string(&mods[0]).unwrap();
        assert_eq!(ron::from_str::<ModSlot>(&text).unwrap(), mods[0]);
    }

    #[test]
    fn a_type_keeps_its_settings_while_another_plays() {
        let mut slot = default_mod(3);
        slot.lfo.phase = 90;
        slot.kind = ModKind::Adsr;
        slot.adsr.attack_ms = 300;
        slot.kind = ModKind::Lfo;
        assert_eq!((slot.lfo.phase, slot.adsr.attack_ms), (90, 300));
    }
}
