//! Track, one of a mod slot's types (#428): the note as a source, its pitch, how hard it's played
//! or a value rolled for it, mapped from `Low` to `High` onto the slot's depth, as the M8's
//! tracking modulator maps its source from `LVAL` to `HVAL`. It takes its value as each note
//! starts and holds it to the next, so it's key tracking with the note as the source, and
//! velocity or a roll a note with the others.

use serde::{Deserialize, Serialize};

use crate::step::{MAX_NOTE, MAX_VELOCITY, MIN_NOTE, Note};

/// What a Track slot follows: its mode, in its title.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Source {
    /// The note's pitch, key tracking.
    Note,
    /// How hard the note is played, as it's heard, `VDLT` and the groove included.
    #[default]
    Velocity,
    /// A value rolled as each note starts, from the project's seed, so a render repeats.
    Random,
}

impl Source {
    /// Every source, in the order `+` goes through them.
    pub const ALL: [Self; 3] = [Self::Note, Self::Velocity, Self::Random];

    /// The name in the title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Velocity => "Velocity",
            Self::Random => "Random",
        }
    }

    /// What it follows, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Note => "the note's pitch, key tracking",
            Self::Velocity => "how hard each note is played",
            Self::Random => "a value rolled at each note",
        }
    }

    /// The lowest and highest it can be: a step's notes, a velocity, or a roll's byte.
    pub fn span(self) -> (u8, u8) {
        match self {
            Self::Note => (MIN_NOTE, MAX_NOTE),
            Self::Velocity => (0, MAX_VELOCITY),
            Self::Random => (0, 0xFF),
        }
    }

    /// One of its values as text: a note's name, or hex as a velocity and a roll are written.
    pub fn text(self, value: u8) -> String {
        match self {
            Self::Note => Note::On(value).to_string(),
            Self::Velocity | Self::Random => format!("{value:02X}"),
        }
    }

    /// Where a value is across its span, from 0 at the lowest to 1 at the highest, for the
    /// picture of the map and its dot.
    pub fn along(self, value: f32) -> f32 {
        let (lowest, highest) = self.span();
        ((value - f32::from(lowest)) / f32::from(highest - lowest)).clamp(0.0, 1.0)
    }
}

/// Where a Track slot's map starts and ends, in its source's values: none of its depth at `low`
/// and all of it at `high`, flat either side. `low` above `high` turns it round, as the M8's does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub low: u8,
    pub high: u8,
}

impl Bounds {
    /// How much of the depth a value gives, from 0 to 1: a straight line from `low` to `high`,
    /// clamped. With the two the same, it's a switch, none below them and all from them up.
    #[inline]
    pub fn level(self, value: f32) -> f32 {
        let (low, high) = (f32::from(self.low), f32::from(self.high));
        if self.low == self.high {
            return if value >= low { 1.0 } else { 0.0 };
        }
        ((value - low) / (high - low)).clamp(0.0, 1.0)
    }

    fn within(self, (lowest, highest): (u8, u8)) -> bool {
        (lowest..=highest).contains(&self.low) && (lowest..=highest).contains(&self.high)
    }
}

/// A Track slot's own settings: its source, and a map for each, so trying another source and
/// coming back finds the first's as it was, as a slot keeps each type's. Where it goes and how
/// far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tracking {
    #[serde(skip_serializing_if = "is_default")]
    pub source: Source,
    #[serde(skip_serializing_if = "is_default_note")]
    pub note: Bounds,
    #[serde(skip_serializing_if = "is_default_velocity")]
    pub velocity: Bounds,
    #[serde(skip_serializing_if = "is_default_random")]
    pub random: Bounds,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

fn is_default_note(bounds: &Bounds) -> bool {
    *bounds == Tracking::default().note
}

fn is_default_velocity(bounds: &Bounds) -> bool {
    *bounds == Tracking::default().velocity
}

fn is_default_random(bounds: &Bounds) -> bool {
    *bounds == Tracking::default().random
}

/// A new Track slot's map of the note: four octaves, `C-1` to `C-5`, around middle C, so both
/// ways it's centred on `C-3`.
const NOTE_BOUNDS: Bounds = Bounds { low: 36, high: 84 };

impl Default for Tracking {
    fn default() -> Self {
        Self {
            source: Source::Velocity,
            note: NOTE_BOUNDS,
            velocity: Bounds {
                low: 0,
                high: MAX_VELOCITY,
            },
            random: Bounds { low: 0, high: 0xFF },
        }
    }
}

impl Tracking {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        Source::ALL
            .iter()
            .all(|source| self.bounds_of(*source).within(source.span()))
    }

    /// The map of a source.
    pub fn bounds_of(&self, source: Source) -> Bounds {
        match source {
            Source::Note => self.note,
            Source::Velocity => self.velocity,
            Source::Random => self.random,
        }
    }

    /// The map of the source it follows.
    pub fn bounds(&self) -> Bounds {
        self.bounds_of(self.source)
    }

    /// The map of the source it follows, to change.
    pub fn bounds_mut(&mut self) -> &mut Bounds {
        match self.source {
            Source::Note => &mut self.note,
            Source::Velocity => &mut self.velocity,
            Source::Random => &mut self.random,
        }
    }

    /// The value of its source for a note, from the note's pitch, its velocity and the slot's
    /// roll for it.
    #[inline]
    pub fn value(&self, note: u8, velocity: u8, roll: u8) -> u8 {
        match self.source {
            Source::Note => note,
            Source::Velocity => velocity,
            Source::Random => roll,
        }
    }

    /// How much of the depth a note gives, from 0 to 1.
    #[inline]
    pub fn level(&self, note: u8, velocity: u8, roll: u8) -> f32 {
        self.bounds()
            .level(f32::from(self.value(note, velocity, roll)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_rises_from_low_to_high_and_is_flat_either_side() {
        let bounds = Bounds {
            low: 0x20,
            high: 0x60,
        };
        assert_eq!(bounds.level(0.0), 0.0);
        assert_eq!(bounds.level(32.0), 0.0);
        assert_eq!(bounds.level(64.0), 0.5);
        assert_eq!(bounds.level(96.0), 1.0);
        assert_eq!(bounds.level(127.0), 1.0);
        // Turned round, it falls.
        let turned = Bounds {
            low: 0x60,
            high: 0x20,
        };
        assert_eq!(turned.level(32.0), 1.0);
        assert_eq!(turned.level(64.0), 0.5);
        assert_eq!(turned.level(120.0), 0.0);
        // The same twice is a switch.
        let switch = Bounds {
            low: 0x40,
            high: 0x40,
        };
        assert_eq!(switch.level(63.0), 0.0);
        assert_eq!(switch.level(64.0), 1.0);
    }

    #[test]
    fn a_track_slot_follows_its_source_with_that_sources_own_map() {
        let mut tracking = Tracking::default();
        assert_eq!(tracking.source, Source::Velocity);
        assert_eq!(tracking.level(60, 0x7F, 0), 1.0);
        assert_eq!(tracking.level(60, 0, 0xFF), 0.0);
        tracking.source = Source::Note;
        assert_eq!(tracking.level(36, 0x7F, 0), 0.0);
        assert_eq!(tracking.level(60, 0x7F, 0), 0.5);
        assert_eq!(tracking.level(84, 0, 0), 1.0);
        tracking.bounds_mut().high = 48;
        tracking.source = Source::Random;
        assert_eq!(tracking.level(0, 0, 0x80), 128.0 / 255.0);
        tracking.source = Source::Note;
        assert_eq!(tracking.bounds().high, 48, "kept while another plays");
    }

    #[test]
    fn tracking_saves_only_what_differs_from_a_new_ones() {
        assert_eq!(ron::to_string(&Tracking::default()).unwrap(), "()");
        let mut tracking = Tracking {
            source: Source::Note,
            ..Tracking::default()
        };
        tracking.note.low = 48;
        let text = ron::to_string(&tracking).unwrap();
        assert_eq!(text, "(source:Note,note:(low:48,high:84))");
        assert_eq!(ron::from_str::<Tracking>(&text).unwrap(), tracking);
        assert!(tracking.in_range());
        tracking.note.low = 0;
        assert!(!tracking.in_range(), "below the notes a step holds");
        tracking.note.low = 12;
        tracking.velocity.high = 0x80;
        assert!(!tracking.in_range(), "past the highest velocity");
    }

    #[test]
    fn a_sources_values_show_as_notes_or_hex() {
        assert_eq!(Source::Note.text(60), "C-3");
        assert_eq!(Source::Note.text(12), "c_1");
        assert_eq!(Source::Velocity.text(0x7F), "7F");
        assert_eq!(Source::Random.text(0x0A), "0A");
        assert_eq!(Source::Note.along(12.0), 0.0);
        assert_eq!(Source::Note.along(127.0), 1.0);
        assert_eq!(Source::Velocity.along(200.0), 1.0);
    }
}
