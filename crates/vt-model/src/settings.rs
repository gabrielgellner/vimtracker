//! Every setting a step can lock or a source can move, registered once: the track's, each
//! machine's, each effect's in each slot, and each mod slot's. An entry has a key that songs store, a
//! long name for finding it, a short label for the grid, its kind and its default. A setting's
//! place in the table is its [`SettingId`], fixed when the program is built.

use crate::constants::{MAX_LEVEL, UNITY_LEVEL};
use crate::groove::GROOVE_CHOICES;
use crate::lane::{LANE_RATES, LENGTH_AS_WRITTEN, RATE_AS_WRITTEN};
use std::borrow::Cow;

use crate::param::{Choice, Descriptor, Reading, Typing};
use crate::song::{FILTER_MODE_NAMES, FULL_GATE, GLIDE_MODE_NAMES, PHASE_NAMES, VOICE_MODE_NAMES};
use crate::speed::{SPEED_NAMES, Speed};

mod id;
pub mod meanings;
mod mods;
mod sampler;
mod units;
pub(crate) mod voices;

pub use id::{FADER_COUNT, FADERS, Owner, SettingId, SettingSet};
pub use mods::{MOD_OWN_SETTINGS, PLAYED_SETTINGS};
pub use sampler::{BANK_CHOICES, PLAY_CHOICES, PlayMode, SAMPLER_SETTINGS, SLICE_CHOICES};
pub use units::{
    ACID_CUTOFF_TAPER, ACID_HIGHEST_CUTOFF, ACID_LONGEST_DECAY, ACID_LOWEST_CUTOFF,
    ACID_SHORTEST_DECAY, BAND_DB, Kind, LEAST_Q, LONGEST_GLIDE_MS, MOST_Q, SHORTEST_GATE,
    SHORTEST_GLIDE_MS, Units, acid_cutoff_hz, acid_cutoff_position, acid_decay_ms, acid_drive_db,
    band_db, band_q, gate_delta, gate_percent, gate_share, gate_value, glide_ms, glide_value,
    moved_velocity, quantised_nudge, rows_text, slide_ms, velocity_delta,
};
pub use voices::{
    FOUR_OP_ALGORITHMS, FOUR_OP_EACH, FOUR_OP_SHARED, MAX_MACHINE_SETTINGS, voice_entries,
    voice_key, voice_place,
};

/// Whether two keys are the same, for the tables that find a setting by its key as they're made.
pub(crate) const fn same_key(one: &str, other: &str) -> bool {
    let (one, other) = (one.as_bytes(), other.as_bytes());
    if one.len() != other.len() {
        return false;
    }
    let mut at = 0;
    while at < one.len() {
        if one[at] != other[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// One setting, as its owner declares it: the track, a machine, an effect, a mod slot. The pages,
/// stepping, `x`, `K`, typing and the status line read these, through [`Entry::described`], so a
/// new effect or voice needs no new page code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The name songs store it by, which never changes.
    pub key: &'static str,
    /// What it's called in lists and on the status line.
    pub name: &'static str,
    /// The grid's label, 3 or 4 letters. Without one, modulation can move it but steps can't.
    pub label: Option<&'static str>,
    /// Its range and the units it reads in, or its choices. A choice that's a scale stops at its
    /// ends, and one that's a list goes round.
    pub kind: Kind,
    pub default: u8,
    /// What it is to its owner: an effect's own, its type, beside its type or what comes out.
    /// Everything that isn't an effect's is its owner's own.
    pub place: Place,
    /// What it does, in a few plain words starting in lower case, for the line that describes the
    /// setting under the cursor ([`Descriptor::meaning`]). Every entry has one.
    pub meaning: &'static str,
}

/// Where a setting sits in its owner. Only an effect's are anything but [`Place::Own`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// One of its owner's own, in its grid.
    Own,
    /// What the effect is: its type or mode, in the slot's title rather than its grid.
    Type,
    /// In the title too, after its type, but a played setting: Beads' quality.
    Title,
    /// What comes out, `Mix` and `Gain`: most effects' last column.
    Output,
}

/// The choices of a setting that's off or on, such as a delay's `Ping-pong`.
pub const OFF_ON: [&str; 2] = ["Off", "On"];

impl Entry {
    /// What it chooses between, in order, or nothing for a value in hex.
    pub const fn choices(&self) -> &'static [&'static str] {
        match self.kind {
            Kind::Stepped { choices, .. } => choices,
            Kind::Continuous { .. } => &[],
        }
    }

    /// Whether stepping past its last choice goes round to the first, or stops.
    #[cfg(test)]
    pub(crate) const fn wraps(&self) -> bool {
        matches!(self.kind, Kind::Stepped { wraps: true, .. })
    }

    /// The same, saying what it does ([`Entry::meaning`]).
    pub(crate) const fn means(self, meaning: &'static str) -> Self {
        Self { meaning, ..self }
    }

    /// Whether it's off or on and nothing else, which the pages show as a toggle.
    pub fn is_toggle(&self) -> bool {
        self.choices() == OFF_ON
    }

    /// What full modulation moves it by, on the scale a step gives: the whole of a list, and `FF`
    /// for a value, as a depth is in hex steps of what it moves. `Gain` too, though it stops at
    /// the level scale's top, as the faders do.
    pub fn span(&self) -> f32 {
        match self.kind {
            Kind::Stepped { .. } => f32::from(self.kind.top()),
            Kind::Continuous { .. } => 255.0,
        }
    }

    /// The same, reading in `units`.
    pub(crate) const fn units(self, units: Units) -> Self {
        let Kind::Continuous { top, middle, .. } = self.kind else {
            return self;
        };
        Self {
            kind: Kind::Continuous { top, middle, units },
            ..self
        }
    }

    /// The same, with its middle at `80`, either way of which it reads.
    pub(crate) const fn centred(self) -> Self {
        let Kind::Continuous { top, units, .. } = self.kind else {
            return self;
        };
        Self {
            kind: Kind::Continuous {
                top,
                middle: true,
                units,
            },
            ..self
        }
    }

    /// It described for whatever shows or edits it: a value in hex from `00` to its top, centred
    /// where it has a middle, reading in its units; or one of its choices, going round where they
    /// do, turned over by `i` where it's off or on.
    pub fn described(&self) -> Descriptor {
        let described = match self.kind {
            Kind::Continuous { top, middle, units } => Descriptor {
                reading: Reading::Byte { units, top, middle },
                centred: middle,
                ..Descriptor::hex(self.key, self.name, top, self.default)
            },
            Kind::Stepped { choices, wraps } => Descriptor {
                typing: match self.is_toggle() {
                    true => Typing::Toggled,
                    false => Typing::Listed,
                },
                ..Descriptor::listed(
                    self.key,
                    self.name,
                    choices.iter().copied().map(Choice::named),
                    usize::from(self.default),
                    wraps,
                )
            },
        };
        Descriptor {
            label: self.label,
            meaning: Cow::Borrowed(self.meaning),
            ..described
        }
    }
}

const fn level(key: &'static str, name: &'static str, label: &'static str, default: u8) -> Entry {
    Entry {
        meaning: "",
        key,
        name,
        label: Some(label),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Percent,
        },
        default,
        place: Place::Own,
    }
}

const fn centred(key: &'static str, name: &'static str, label: &'static str) -> Entry {
    Entry {
        meaning: "",
        key,
        name,
        label: Some(label),
        kind: CENTRED,
        default: 0x80,
        place: Place::Own,
    }
}

const fn on_level_scale(
    key: &'static str,
    name: &'static str,
    label: &'static str,
    default: u8,
) -> Entry {
    Entry {
        kind: ON_LEVEL_SCALE,
        ..level(key, name, label, default)
    }
}

const CENTRED: Kind = Kind::Continuous {
    top: 0xFF,
    middle: true,
    units: Units::Percent,
};
const ON_LEVEL_SCALE: Kind = Kind::Continuous {
    top: MAX_LEVEL,
    middle: false,
    units: Units::Decibels,
};

/// The track's own, in the order the table keeps them.
const TRACK: [Entry; 16] = [
    on_level_scale("vol", "Vol", "VOL", UNITY_LEVEL)
        .means("gain on top of the fader, which steps and modulation move"),
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Pan,
        },
        ..centred("pan", "Pan", "PAN").means("where the track sits between left and right")
    },
    Entry {
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Hertz,
        },
        ..level("cut", "Cut", "CUT", 0xFF).means("where the filter starts to take away the sound")
    },
    level("res", "Res", "RES", 0x00).means("how much the filter rings at its cutoff"),
    on_level_scale("sna", "Send A", "SNA", 0x00).means("how much of the track goes to return A"),
    on_level_scale("snb", "Send B", "SNB", 0x00).means("how much of the track goes to return B"),
    Entry {
        meaning: "moves every note on the track up or down, a semitone a step",
        key: "pitch",
        name: "Pitch",
        label: Some("PTCH"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Semitones,
        },
        default: 0x80,
        place: Place::Own,
    },
    Entry {
        meaning: "how the filter's type responds: its slope, or what it lets through",
        key: "mode",
        name: "Mode",
        label: Some("MODE"),
        kind: Kind::Stepped {
            choices: &FILTER_MODE_NAMES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    Entry {
        meaning: "how much of its last pulse each note lasts before it's let go",
        key: "gate",
        name: "Gate",
        label: Some("LEN"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::GateLength,
        },
        default: FULL_GATE,
        place: Place::Own,
    },
    // Its value is never heard alone: it's added to each note's length as it starts, so a lane
    // of it moves notes' lengths against their own.
    Entry {
        meaning: "moves each note's length up or down from its own as it starts",
        key: "gatedelta",
        name: "Gate up or down",
        label: Some("LDLT"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::GateDelta,
        },
        default: 0x80,
        place: Place::Own,
    },
    // A step's own groove, read before the step is played, as it says when the step lands. `00`
    // is the project's, and a track with its own holds that.
    Entry {
        meaning: "the groove the track's steps are played with, or the project's",
        key: "groove",
        name: "Groove",
        label: Some("GRV"),
        kind: Kind::Stepped {
            choices: &GROOVE_CHOICES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    // How long a step is, read before the step is played, as it says when the next one comes.
    Entry {
        meaning: "how long each of the track's steps is",
        key: "speed",
        name: "Speed",
        label: Some("RATE"),
        kind: Kind::Stepped {
            choices: &SPEED_NAMES,
            wraps: false,
        },
        default: Speed::Sixteenth.index(),
        place: Place::Own,
    },
    // How long a tie glides into the next note, read as the tie comes, so a lock on either row
    // sets it.
    Entry {
        meaning: "how long a tie takes to glide into the next note",
        key: "glide",
        name: "Glide",
        label: Some("GLD"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: false,
            units: Units::Glide,
        },
        default: 0,
        place: Place::Own,
    },
    // As `LDLT` is to a note's length: added to each note's velocity as it starts, so a lane of it
    // is an accent pattern and a glide of it a crescendo, and the note keeps its own.
    Entry {
        meaning: "moves each note's velocity up or down from its own as it starts",
        key: "velocitydelta",
        name: "Velocity up or down",
        label: Some("VDLT"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::VelocityDelta,
        },
        default: 0x80,
        place: Place::Own,
    },
    // The fader, which a step can set as it sets `PAN`, with `VOL` a stage on top of it (decision
    // 174). Its base is the track's `level`, and modulation can't reach it, as `VOL` is where
    // modulation moves the volume.
    on_level_scale("level", "Level", "LEV", UNITY_LEVEL)
        .means("the track's fader: how loud the whole track is"),
    // How far its rows' nudges are brought toward the grid as they play, read before a row plays,
    // as `GRV` is, so a take can be tightened live, or loosened again, and keeps what was played.
    level("quantise", "Quantise", "QNT", 0x00)
        .means("how far each row's nudge is brought toward the grid as it plays"),
];
/// What each of the track's is listed with.
const TRACK_GROUPS: [&str; 16] = [
    "Output", "Output", "Filter", "Filter", "Output", "Output", "Machine", "Filter", "Machine",
    "Machine", "Machine", "Machine", "Machine", "Machine", "Output", "Machine",
];

/// The track's own settings added since places were fixed, at the end of the table so none moves
/// (#718): the track's own after [`TRACK`]'s, in the order they came.
const LATER_TRACK: [Entry; 4] = [
    // Whether a synth's oscillators run on from note to note or start again on each, read as a
    // note starts, so a lock on its row sets it for that note (decision 370).
    Entry {
        meaning: "whether the oscillators run on from note to note or start again",
        key: "phase",
        name: "Phase",
        label: Some("OSC"),
        kind: Kind::Stepped {
            choices: &PHASE_NAMES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    // Whether a note ties into one whose gate is still open, read as a note starts, so a lock on
    // its row sets it for that note (decision 380).
    Entry {
        meaning: "how notes join: each struck, or tied while the gate is still open",
        key: "voicemode",
        name: "Mode",
        label: Some("VMOD"),
        kind: Kind::Stepped {
            choices: &VOICE_MODE_NAMES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    // Whether the filter's cutoff follows the note's pitch, an octave for an octave from `C-3`,
    // read every frame, so a lock on a row sets it for that row (decision 381).
    Entry {
        meaning: "whether the filter's cutoff follows the note, an octave for an octave",
        key: "keytrack",
        name: "Key track",
        label: Some("KTRK"),
        kind: Kind::Stepped {
            choices: &OFF_ON,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
    // Whether `Glide` slides only ties or every note from the one sounding, read as a note
    // starts, so a lock on its row sets it for that note (decision 382).
    Entry {
        meaning: "which notes Glide slides into: only ties, or every note",
        key: "glidemode",
        name: "Glide mode",
        label: Some("GMOD"),
        kind: Kind::Stepped {
            choices: &GLIDE_MODE_NAMES,
            wraps: false,
        },
        default: 0,
        place: Place::Own,
    },
];
/// What each of [`LATER_TRACK`] is listed with.
const LATER_TRACK_GROUPS: [&str; 4] = ["Machine", "Machine", "Filter", "Machine"];
/// How many of the track's own there are, [`TRACK`]'s and [`LATER_TRACK`]'s.
const TRACK_SETTINGS: usize = TRACK.len() + LATER_TRACK.len();

/// A slot's own, whatever effect is in it.
const SLOT_OWN: [Entry; 1] = [Entry {
    meaning: "whether the effect in the slot is heard or switched out",
    key: "bypass",
    name: "Bypass",
    // With the slot's digit before it, as `1BYP`.
    label: Some("BYP"),
    kind: Kind::Stepped {
        choices: &BYPASS_CHOICES,
        wraps: false,
    },
    default: 0,
    place: Place::Own,
}];

/// Each FX column's lane's, which move every phrase's lane on that column from its own: how many
/// rows it's heard for, one a step either side of `80`, and how much slower or faster its rows go.
/// With the column's number after each, as `LLEN1`.
const LANE: [Entry; 2] = [
    Entry {
        meaning: "how many rows this column's lanes are heard for, more or fewer",
        key: "length",
        name: "Length",
        label: Some("LLEN"),
        kind: Kind::Continuous {
            top: 0xFF,
            middle: true,
            units: Units::Rows,
        },
        default: LENGTH_AS_WRITTEN,
        place: Place::Own,
    },
    Entry {
        meaning: "how much slower or faster this column's lanes go than written",
        key: "rate",
        name: "Rate",
        label: Some("LRAT"),
        kind: Kind::Stepped {
            choices: &LANE_RATES,
            wraps: false,
        },
        default: RATE_AS_WRITTEN,
        place: Place::Own,
    },
];

/// How many settings each FX column's lane has.
pub const LANE_SETTINGS: usize = LANE.len();

/// What a slot's bypass can be: heard, at `00`, or switched out, at `01`.
pub(crate) const BYPASS_CHOICES: [&str; 2] = ["In", "Bypassed"];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::mods::MOD;
    use super::sampler::SAMPLER;
    use super::*;
    use crate::insert::EffectKind;
    use crate::modulation::MOD_SLOTS;
    use crate::patch_names::PATCH_NAMES;
    use crate::song::{DEFAULT_ACCENT, SynthVoice};
    use crate::step::FxCommand;

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
                (Owner::Mod(slot), at) => SettingId::modulator(slot, at),
                (Owner::Bypass(slot), _) => SettingId::bypass(slot),
                (Owner::Sampler, at) => SettingId::sampler(at),
                (Owner::Macro(at), _) => SettingId::macro_at(at),
                (Owner::ModOwn(slot), at) => SettingId::mod_own(slot, at),
                (Owner::Accumulation, at) => SettingId::accumulation(at),
                (Owner::Arpeggio, at) => SettingId::arpeggio(at),
                (Owner::Lane(slot), at) => SettingId::lane(slot, at),
            };
            assert_eq!(back, Some(id), "{}", id.key());
            let mod_own = matches!(id.owner().0, Owner::ModOwn(_));
            assert_eq!(id.is_mod_own(), mod_own, "{}", id.key());
        }
        assert_eq!(SettingId::all().count(), SettingId::COUNT);
    }

    /// Every setting says what it does, for the line that describes the one under the cursor
    /// (#969): each of the registry's, each generator setting and each mod slot's under every
    /// type that has it. A new setting without its words fails here.
    #[test]
    fn every_setting_says_what_it_does() {
        use crate::modulation::{ModField, ModKind};
        use crate::shape::GenParam;
        let said = |meaning: &str| !meaning.is_empty() && !meaning.ends_with('.');
        let mut missing: Vec<String> = SettingId::all()
            .filter(|id| !said(id.entry().meaning))
            .map(|id| id.key())
            .collect();
        missing.extend(
            GenParam::ALL
                .into_iter()
                .filter(|param| !said(param.meaning()))
                .map(|param| format!("gen {}", param.key())),
        );
        let fields = [
            ModField::Type,
            ModField::Mode,
            ModField::Polarity,
            ModField::Destination,
            ModField::Depth,
            ModField::Attack,
            ModField::Decay,
            ModField::Sustain,
            ModField::Release,
            ModField::Reset,
            ModField::Hold,
            ModField::Peak,
            ModField::Body,
            ModField::Ratchet,
            ModField::Colour,
            ModField::Shape,
            ModField::Rate,
            ModField::Phase,
            ModField::Deform,
            ModField::Fade,
            ModField::Length,
            ModField::Start,
            ModField::Loop,
            ModField::Mutate,
            ModField::Smooth,
            ModField::Step(0),
            ModField::Low,
            ModField::High,
        ];
        for kind in ModKind::ALL {
            missing.extend(
                fields
                    .into_iter()
                    .filter(|field| field.on(kind) && !said(field.meaning(kind)))
                    .map(|field| format!("{kind:?} {field:?}")),
            );
        }
        missing.dedup();
        assert!(
            missing.is_empty(),
            "{} settings say nothing of what they do, or end in a full stop: {}",
            missing.len(),
            missing.join(", ")
        );
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
            SettingId::slot(1, EffectKind::Delay, 1)
        );
        assert_eq!(
            SettingId::from_key("string.decay"),
            SettingId::voice(SynthVoice::String, 2)
        );
    }

    #[test]
    fn quantise_brings_every_nudge_toward_the_grid_by_its_share() {
        use crate::constants::NUDGE_TICKS;
        for nudge in -11_i64..=11 {
            let ticks = nudge * NUDGE_TICKS as i64;
            assert_eq!(quantised_nudge(ticks, 0.0), ticks, "as written");
            assert_eq!(
                quantised_nudge(ticks, 128.0),
                ticks / 2,
                "halved, to the tick"
            );
            assert_eq!(quantised_nudge(ticks, 255.0), 0, "on the grid");
        }
        assert_eq!(quantised_nudge(-110, 300.0), 0, "kept to the scale");
        assert_eq!(quantised_nudge(-110, -40.0), -110);
    }

    #[test]
    fn the_level_scale_is_two_digits_and_positions_are_four() {
        for id in [
            SettingId::VOL,
            SettingId::LEVEL,
            SettingId::SEND_A,
            SettingId::SEND_B,
        ] {
            assert!(!id.entry().kind.has_fine(), "{}", id.name());
            assert!(FxCommand::Set(id).glides(), "{} still glides", id.name());
        }
        for id in [
            SettingId::PAN,
            SettingId::CUT,
            SettingId::RES,
            SettingId::PITCH,
        ] {
            assert!(id.entry().kind.has_fine(), "{}", id.name());
        }
        assert!(!SettingId::SPEED.entry().kind.has_fine(), "a list");
        assert_eq!(SettingId::LEVEL.label().as_deref(), Some("LEV"));
        assert_eq!(SettingId::LEVEL.group(), "Output");
        assert_eq!(
            FADERS.map(|id| id.fader()),
            [Some(0), Some(1), Some(2), Some(3)]
        );
        assert_eq!(
            SettingId::VOL.fader(),
            None,
            "VOL is a stage on top, not a fader"
        );
    }

    #[test]
    fn every_label_is_three_or_four_letters_after_a_slots_digit() {
        for id in SettingId::all() {
            let Some(label) = id.label() else { continue };
            let letters = match id.owner().0 {
                Owner::Slot { slot, .. } | Owner::Bypass(slot) => {
                    assert!(label.starts_with(&(slot + 1).to_string()), "{label}");
                    &label[1..]
                }
                // A lane's end in its FX column's number, as a mod slot's end in the slot's.
                Owner::Mod(slot) | Owner::Macro(slot) | Owner::Lane(slot) => {
                    assert!(label.ends_with(&(slot + 1).to_string()), "{label}");
                    &label[..label.len() - 1]
                }
                // 4-op's operators' settings end in the operator's number, as a mod slot's do.
                Owner::Voice(SynthVoice::FourOp)
                    if label.ends_with(|c: char| c.is_ascii_digit()) =>
                {
                    &label[..label.len() - 1]
                }
                _ => &label[..],
            };
            assert!(
                (3..=4).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_uppercase()),
                "{label}"
            );
        }
        for event in FxCommand::EVENTS {
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
            labels((0..TRACK_SETTINGS).filter_map(SettingId::track))
                .into_iter()
                .chain(labels((0..MOD_SLOTS).flat_map(|slot| {
                    (0..MOD.len()).filter_map(move |at| SettingId::modulator(slot, at))
                })))
                .chain(labels(SettingId::macros()))
                .chain(labels(SettingId::lanes()))
                .chain(FxCommand::EVENTS.map(|event| event.name()))
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
        let mut seen = HashSet::new();
        let sampler = labels((0..SAMPLER.len()).filter_map(SettingId::sampler));
        assert_eq!(sampler, ["SMP", "SLI", "STA", "SLEN", "PLY"]);
        for label in track_and_events().chain(sampler) {
            assert!(seen.insert(label.clone()), "{label} twice on the sampler");
        }
        for effect in EffectKind::ALL {
            let mut seen = HashSet::from([SettingId::bypass(0).unwrap().label().unwrap()]);
            for at in 0..effect.params().len() {
                let Some(label) = SettingId::slot(0, effect, at).unwrap().label() else {
                    continue;
                };
                assert!(seen.insert(label.clone()), "{label} twice on {effect:?}");
            }
        }
    }

    #[test]
    fn a_voices_defaults_are_its_own() {
        let six_op = |at| SettingId::voice(SynthVoice::SixOp, at).unwrap().entry();
        assert_eq!(PATCH_NAMES[usize::from(six_op(0).default)], "E.PIANO 1");
        assert!(matches!(six_op(0).kind, Kind::Stepped { choices, .. } if choices.len() == 96));
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
        let drift = SettingId::voice(SynthVoice::VirtualAnalog, 3)
            .unwrap()
            .entry();
        assert_eq!(
            (drift.key, drift.default),
            ("drift", 0x00),
            "none unless asked"
        );
        assert_eq!(SettingId::voice(SynthVoice::VirtualAnalog, 4), None);
    }

    #[test]
    fn a_value_reads_in_its_settings_units() {
        let shown = |id: SettingId, value: u16| id.entry().kind.shown(value);
        assert_eq!(shown(SettingId::CUT, 0xFF00), "20.0 kHz");
        assert_eq!(shown(SettingId::CUT, 0x0000), "20 Hz");
        assert_eq!(shown(SettingId::VOL, 0xE000), "+0.0 dB");
        assert_eq!(shown(SettingId::VOL, 0xC000), "-8.0 dB");
        assert_eq!(shown(SettingId::VOL, 0x0000), "-inf dB");
        assert_eq!(shown(SettingId::PAN, 0x8000), "Centre");
        assert_eq!(shown(SettingId::PAN, 0x0000), "100 % L");
        assert_eq!(shown(SettingId::PAN, 0xFF00), "100 % R");
        assert_eq!(shown(SettingId::RES, 0x8000), "50 %");
        let patch = SettingId::voice(SynthVoice::SixOp, 0).unwrap();
        assert_eq!(shown(patch, 0x2000), "E.PIANO 1");
        let detune = SettingId::voice(SynthVoice::VirtualAnalog, 0).unwrap();
        assert_eq!(shown(detune, 0x8000), "+0 %");
        let gain = SettingId::slot(0, EffectKind::Reverb, 6).unwrap();
        assert_eq!(shown(gain, 0xE000), "+0.0 dB");
        let (depth, phase) = (
            SettingId::modulator(0, 0).unwrap(),
            SettingId::modulator(5, 1).unwrap(),
        );
        assert_eq!(shown(depth, crate::modulation::depth_value(0x40)), "+40");
        assert_eq!(shown(depth, 0x8000), "00");
        assert_eq!(shown(phase, crate::lfo::phase_value(90)), "90°");
        assert_eq!(shown(SettingId::PITCH, 0x8700), "+7 st");
        assert_eq!(shown(SettingId::PITCH, 0x8780), "+7 st +50 ct");
        assert_eq!(shown(SettingId::PITCH, 0x7F80), "+0 st -50 ct");
        assert_eq!(shown(SettingId::GATE_DELTA, 0x8000), "+0 %");
        assert_eq!(shown(SettingId::GATE_DELTA, 0x0000), "-50 %");
        assert_eq!(shown(SettingId::GATE_DELTA, 0xC000), "+25 %");
        assert_eq!(SettingId::GATE_DELTA.label(), Some("LDLT".into()));
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x8000), "+00 vel");
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x9000), "+10 vel");
        assert_eq!(shown(SettingId::VELOCITY_DELTA, 0x0000), "-80 vel");
        assert_eq!(SettingId::VELOCITY_DELTA.label(), Some("VDLT".into()));
        assert_eq!(shown(SettingId::QUANTISE, 0x0000), "0 %");
        assert_eq!(shown(SettingId::QUANTISE, 0x8000), "50 %");
        assert_eq!(shown(SettingId::QUANTISE, 0xFF00), "100 %");
        assert_eq!(SettingId::QUANTISE.label(), Some("QNT".into()));
        assert_eq!(SettingId::QUANTISE.key(), "quantise");
        assert_eq!(
            (depth.label(), phase.label(), SettingId::PITCH.label()),
            (
                Some("DEP1".into()),
                Some("PHS6".into()),
                Some("PTCH".into())
            )
        );
        assert!(phase.entry().kind.wraps() && !depth.entry().kind.wraps());
        let eq = |at| SettingId::slot(0, EffectKind::Eq, at).unwrap();
        assert_eq!(shown(eq(0), 0x3B00), "99 Hz");
        assert_eq!(shown(eq(1), 0x8000), "+0.0 dB");
        assert_eq!(shown(eq(1), 0xFF00), "+15.0 dB");
        assert_eq!(shown(eq(1), 0x0000), "-15.0 dB");
        assert_eq!(shown(eq(6), 0x4000), "Q 0.71");
        // A Space Echo's tape speed, drive and shelves, each on its own scale.
        let echo = |key: &str| SettingId::from_key(&format!("1.echo.{key}")).unwrap();
        assert_eq!(shown(echo("rate"), 0x6900), "150 ms");
        assert_eq!(shown(echo("rate"), 0x0000), "1.00 s");
        assert_eq!(shown(echo("input"), 0x8000), "+0.0 dB");
        assert_eq!(shown(echo("input"), 0xFF00), "+24.0 dB");
        assert_eq!(shown(echo("input"), 0x0000), "-24.0 dB");
        assert_eq!(shown(echo("bass"), 0x8000), "+0.0 dB");
        assert_eq!(shown(echo("bass"), 0xFF00), "+23.1 dB");
        assert_eq!(shown(echo("treble"), 0x0000), "-15.4 dB");
        let makeup = SettingId::from_key("1.comp.makeup").unwrap();
        assert_eq!(makeup.label(), Some("1MKUP".into()));
        assert_eq!(shown(makeup, 0x0000), "+0.0 dB");
        assert_eq!(shown(makeup, 0x8000), "+12.0 dB");
        assert_eq!(shown(makeup, 0xFF00), "+24.0 dB");
        let algo = SettingId::slot(0, EffectKind::Warps, 0).unwrap();
        assert_eq!(shown(algo, 0x0000), "Xfade");
        assert_eq!(shown(algo, 0x6000), "Digital");
        assert_eq!(shown(algo, 0x7000), "Digital>XOR");
        assert_eq!(shown(algo, 0xB300), "Mod");
        assert_eq!(shown(algo, 0xD000), "Vocoder");
        assert_eq!(shown(algo, 0xFF00), "Vocoder hold");
        let source = SettingId::slot(0, EffectKind::Warps, 2).unwrap();
        assert_eq!(shown(source, 0x0000), "Osc");
        assert_eq!(shown(source, 0x4100), "A");
    }

    #[test]
    fn a_lock_shows_its_settings_label_with_its_slots_digit() {
        assert_eq!(FxCommand::CUT.name(), "CUT");
        assert_eq!(FxCommand::Del.name(), "DEL");
        let punch = SettingId::voice(SynthVoice::Kick, 0).unwrap();
        assert_eq!(FxCommand::Set(punch).name(), "PNCH");
        let feedback = SettingId::slot(1, EffectKind::Delay, 1).unwrap();
        assert_eq!(FxCommand::Set(feedback).name(), "2FDBK");
        assert_eq!(FxCommand::Set(feedback).key(), "2.delay.feedback");
        assert_eq!(
            FxCommand::from_key("2.delay.feedback"),
            Some(FxCommand::Set(feedback))
        );
        assert_eq!(FxCommand::from_key("del"), Some(FxCommand::Del));
        assert_eq!(FxCommand::from_key("CUT"), None, "keys are lower case");
    }
}
