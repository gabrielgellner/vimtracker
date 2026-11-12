//! A track's mod slots: six modulators, each of a type chosen for it, which work out in order so
//! each can move only those after it (`docs/designs/mod-slots.md`). Every slot has a type, where
//! it goes, how far and a switch, and keeps each type's own settings, so trying another type and
//! coming back finds the first as it was.

use serde::{Deserialize, Serialize};

use crate::envelope::{Adsr, Ahd, Drum, Envelope, Lpg};
use crate::lfo::Lfo;
use crate::settings::SettingId;
use crate::steps::Steps;

/// How many mod slots a track has.
pub const MOD_SLOTS: usize = 6;
/// The furthest either way a slot's depth goes, in the units of what it moves.
pub const MAX_DEPTH: i16 = 0xFF;

/// What a mod slot is. ADSR, AHD, Drum, LPG, LFO and Steps so far; Track follows in M20.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum ModKind {
    #[default]
    Adsr,
    Ahd,
    Drum,
    Lpg,
    Lfo,
    Steps,
}

impl ModKind {
    /// Every type, in the order the type picker lists them.
    pub const ALL: [Self; 6] = [
        Self::Adsr,
        Self::Ahd,
        Self::Drum,
        Self::Lpg,
        Self::Lfo,
        Self::Steps,
    ];

    /// The name in the slot's title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Adsr => "ADSR",
            Self::Ahd => "AHD",
            Self::Drum => "Drum",
            Self::Lpg => "LPG",
            Self::Lfo => "LFO",
            Self::Steps => "Steps",
        }
    }

    /// What it does, for the type picker.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Adsr => "held while the note is",
            Self::Ahd => "a hold at full, for a time or the note",
            Self::Drum => "a fast peak onto a body that decays",
            Self::Lpg => "a vactrol's fall, closing the filter",
            Self::Lfo => "a cycle, free or from each note",
            Self::Steps => "up to 16 values, in time or a note each",
        }
    }

    /// Whether it's an envelope, which each note starts and which can shape the notes themselves.
    pub fn is_envelope(self) -> bool {
        matches!(self, Self::Adsr | Self::Ahd | Self::Drum | Self::Lpg)
    }

    /// Whether it has a mode, which its title shows after it: an envelope's curve, and an LFO's
    /// and Steps' mode. A low-pass gate has none, as its curve is the point of it.
    pub fn has_mode(self) -> bool {
        !matches!(self, Self::Lpg)
    }

    /// Whether it has a polarity to choose. Steps' values are signed, so they say which way
    /// themselves.
    pub fn has_polarity(self) -> bool {
        self != Self::Steps
    }

    /// What its mode is called, as the status line and the Roto name it.
    pub fn mode_label(self) -> &'static str {
        match self {
            Self::Adsr | Self::Ahd | Self::Drum | Self::Lpg => "Curve",
            Self::Lfo | Self::Steps => "Mode",
        }
    }

    /// Which way it moves what it reaches unless its slot says otherwise: an envelope rises from
    /// the setting, and an LFO swings around it, as before slots had a polarity.
    pub fn polarity(self) -> Polarity {
        match self {
            Self::Adsr | Self::Ahd | Self::Drum | Self::Lpg => Polarity::Uni,
            Self::Lfo | Self::Steps => Polarity::Bi,
        }
    }
}

/// Which way a slot moves what it reaches, as Bitwig's and Vital's modulators have it (#434):
/// one way from the setting, or both ways around it. A negative depth still flips the direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Polarity {
    /// One way: an LFO from 0 to 1, for a tremolo that only dips or a vibrato that only bends up,
    /// and an envelope rising from the setting.
    Uni,
    /// Both ways: an LFO from -1 to 1, and an envelope centred on the setting, from below it at
    /// rest to above it at full.
    Bi,
}

impl Polarity {
    /// Both, in the order lists and the registry's values have them.
    pub const ALL: [Self; 2] = [Self::Uni, Self::Bi];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Uni => "Uni",
            Self::Bi => "Bi",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Uni => "one way from the setting",
            Self::Bi => "both ways around the setting",
        }
    }

    /// Its place among [`Polarity::ALL`], as a setting's value.
    pub fn index(self) -> u8 {
        match self {
            Self::Uni => 0,
            Self::Bi => 1,
        }
    }

    /// The other one.
    #[must_use]
    pub fn other(self) -> Self {
        match self {
            Self::Uni => Self::Bi,
            Self::Bi => Self::Uni,
        }
    }

    /// A modulator's output, from 0 to 1 for an envelope's level or -1 to 1 for a wave, in this
    /// polarity, from the one its type gives.
    #[inline]
    pub fn of(self, natural: Self, value: f32) -> f32 {
        match (natural, self) {
            (Self::Bi, Self::Uni) => (value + 1.0) * 0.5,
            (Self::Uni, Self::Bi) => value * 2.0 - 1.0,
            _ => value,
        }
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
    /// Which way it moves what it reaches, if it's been chosen; otherwise its type's own, so a
    /// type change keeps one that was chosen and follows the type's if none was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polarity: Option<Polarity>,
    /// Whether, going to `Pitch`, it moves the note along the song's scale, to the nearest of its
    /// notes, so a random LFO or Steps play in key (#235's and #86's comments). It's one switch
    /// for every type, and does nothing going anywhere else.
    #[serde(default, skip_serializing_if = "is_false")]
    pub in_scale: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub adsr: Adsr,
    #[serde(default, skip_serializing_if = "is_default")]
    pub ahd: Ahd,
    #[serde(default, skip_serializing_if = "is_default")]
    pub drum: Drum,
    #[serde(default, skip_serializing_if = "is_default")]
    pub lpg: Lpg,
    #[serde(default, skip_serializing_if = "is_default")]
    pub lfo: Lfo,
    #[serde(default, skip_serializing_if = "is_default")]
    pub steps: Steps,
}

impl ModSlot {
    /// An ADSR going somewhere, with no depth yet.
    pub fn adsr(destination: Option<SettingId>, adsr: Adsr) -> Self {
        Self {
            kind: ModKind::Adsr,
            destination,
            depth: 0,
            bypassed: false,
            polarity: None,
            in_scale: false,
            adsr,
            ahd: Ahd::default(),
            drum: Drum::default(),
            lpg: Lpg::default(),
            lfo: Lfo::default(),
            steps: Steps::default(),
        }
    }

    /// An LFO going somewhere, with no depth yet.
    pub fn lfo(destination: Option<SettingId>, lfo: Lfo) -> Self {
        Self {
            kind: ModKind::Lfo,
            destination,
            depth: 0,
            bypassed: false,
            polarity: None,
            in_scale: false,
            adsr: Adsr::default(),
            ahd: Ahd::default(),
            drum: Drum::default(),
            lpg: Lpg::default(),
            lfo,
            steps: Steps::default(),
        }
    }

    /// An AHD going somewhere, with no depth yet.
    pub fn ahd(destination: Option<SettingId>, ahd: Ahd) -> Self {
        Self {
            kind: ModKind::Ahd,
            ahd,
            ..Self::adsr(destination, Adsr::default())
        }
    }

    /// A drum envelope going somewhere, with no depth yet.
    pub fn drum(destination: Option<SettingId>, drum: Drum) -> Self {
        Self {
            kind: ModKind::Drum,
            drum,
            ..Self::adsr(destination, Adsr::default())
        }
    }

    /// A low-pass gate going somewhere, with no depth yet.
    pub fn lpg(destination: Option<SettingId>, lpg: Lpg) -> Self {
        Self {
            kind: ModKind::Lpg,
            lpg,
            ..Self::adsr(destination, Adsr::default())
        }
    }

    /// A Steps slot going somewhere, its values heard as written.
    pub fn steps(destination: Option<SettingId>, steps: Steps) -> Self {
        Self {
            kind: ModKind::Steps,
            steps,
            depth: MAX_DEPTH,
            ..Self::adsr(destination, Adsr::default())
        }
    }

    /// The same, moving its destination this far.
    #[must_use]
    pub fn with_depth(self, depth: i16) -> Self {
        Self { depth, ..self }
    }

    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.depth.abs() <= MAX_DEPTH
            && self.adsr.in_range()
            && self.ahd.in_range()
            && self.drum.in_range()
            && self.lpg.in_range()
            && self.lfo.in_range()
            && self.steps.in_range()
    }

    /// Whether it moves the note along the song's scale: it's in scale and goes to `Pitch`.
    pub fn goes_in_scale(&self) -> bool {
        self.in_scale && self.destination == Some(SettingId::PITCH)
    }

    /// Which way it moves what it reaches: its own if it's been chosen, else its type's. Steps
    /// always go both ways, as their values are signed.
    pub fn polarity(&self) -> Polarity {
        match self.kind {
            ModKind::Steps => Polarity::Bi,
            kind => self.polarity.unwrap_or_else(|| kind.polarity()),
        }
    }

    /// The name of its mode, for a type that has one.
    pub fn mode_name(&self) -> Option<&'static str> {
        match self.kind {
            ModKind::Adsr => Some(self.adsr.curve.name()),
            ModKind::Ahd => Some(self.ahd.curve.name()),
            ModKind::Drum => Some(self.drum.curve.name()),
            ModKind::Lpg => None,
            ModKind::Lfo => Some(self.lfo.mode.name()),
            ModKind::Steps => Some(self.steps.mode.name()),
        }
    }

    /// The envelope it runs: its AHD's as an AHD, its drum's as a Drum, and otherwise its ADSR's, which every slot keeps
    /// running underneath, so one that becomes an envelope picks it up where the note has it.
    #[inline]
    pub fn envelope(&self) -> Envelope {
        match self.kind {
            ModKind::Ahd => Envelope::Ahd(self.ahd),
            ModKind::Drum => Envelope::Drum(self.drum),
            ModKind::Lpg => Envelope::Lpg(self.lpg),
            ModKind::Adsr | ModKind::Lfo | ModKind::Steps => Envelope::Adsr(self.adsr),
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

    #[test]
    fn a_slots_polarity_follows_its_type_until_one_is_chosen() {
        let mut slot = default_mod(3);
        assert_eq!(slot.polarity(), Polarity::Bi);
        slot.kind = ModKind::Adsr;
        assert_eq!(slot.polarity(), Polarity::Uni, "an envelope's own");
        slot.polarity = Some(Polarity::Bi);
        slot.kind = ModKind::Lfo;
        slot.kind = ModKind::Adsr;
        assert_eq!(slot.polarity(), Polarity::Bi, "chosen, it stays");
        let text = ron::to_string(&slot).unwrap();
        assert!(text.contains("polarity:Some(Bi)"), "{text}");
        assert_eq!(ron::from_str::<ModSlot>(&text).unwrap(), slot);
        assert!(
            !ron::to_string(&default_mod(3))
                .unwrap()
                .contains("polarity")
        );
    }

    #[test]
    fn an_ahd_slot_is_an_envelope_with_its_own_settings_saved_only_once_changed() {
        use crate::envelope::{Curve, Hold};
        let mut slot = default_mod(2);
        assert!(!ron::to_string(&slot).unwrap().contains("ahd"));
        slot.kind = ModKind::Ahd;
        assert!(slot.kind.is_envelope());
        assert_eq!(slot.polarity(), Polarity::Uni);
        assert_eq!(slot.mode_name(), Some("Exp"));
        assert_eq!(slot.envelope(), Envelope::Ahd(Ahd::default()));
        slot.ahd.hold = Hold::Note;
        slot.ahd.curve = Curve::Snap;
        let text = ron::to_string(&slot).unwrap();
        assert!(
            text.contains("kind:Ahd") && text.contains("hold:Note"),
            "{text}"
        );
        assert_eq!(ron::from_str::<ModSlot>(&text).unwrap(), slot);
        // As an ADSR again, it runs its ADSR, and keeps its AHD for later.
        slot.kind = ModKind::Adsr;
        assert_eq!(slot.envelope(), Envelope::Adsr(Adsr::decay()));
        assert_eq!(slot.ahd.hold, Hold::Note);
        // It can shape the notes.
        let mut mods = default_mods();
        mods[0] = ModSlot::ahd(Some(SettingId::VOL), Ahd::default());
        assert_eq!(note_shape_slot(&mods), Some(0));
    }

    #[test]
    fn a_polarity_maps_its_types_output_only_when_it_differs() {
        // An LFO one way goes from 0 to 1, and an envelope both ways from -1 to 1.
        assert_eq!(Polarity::Uni.of(Polarity::Bi, -1.0), 0.0);
        assert_eq!(Polarity::Uni.of(Polarity::Bi, 1.0), 1.0);
        assert_eq!(Polarity::Bi.of(Polarity::Uni, 0.0), -1.0);
        assert_eq!(Polarity::Bi.of(Polarity::Uni, 0.75), 0.5);
        // Its own leaves every value exactly as it was.
        for value in [-0.3_f32, 0.1, 0.7] {
            assert_eq!(
                Polarity::Bi.of(Polarity::Bi, value).to_bits(),
                value.to_bits()
            );
            assert_eq!(
                Polarity::Uni.of(Polarity::Uni, value).to_bits(),
                value.to_bits()
            );
        }
    }
}
