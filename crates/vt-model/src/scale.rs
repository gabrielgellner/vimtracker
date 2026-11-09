//! The song's scale, an aid to entering and moving notes, and what an `ACU` moves notes through.

use serde::{Deserialize, Serialize};

use crate::step::{MAX_NOTE, MIN_NOTE};

/// The notes in an octave.
const OCTAVE: u16 = 12;
/// The roots' names, from C, which `:set root=` completes from.
pub const ROOTS: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// A kind of scale, apart from the note it starts on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleKind {
    #[default]
    Major,
    Minor,
    Dorian,
    Phrygian,
    Lydian,
    Mixolydian,
    Locrian,
    MinorPentatonic,
    MajorPentatonic,
    HarmonicMinor,
    MelodicMinor,
    Blues,
}

impl ScaleKind {
    /// Every kind, in the order a list shows them.
    pub const ALL: [Self; 12] = [
        Self::Major,
        Self::Minor,
        Self::Dorian,
        Self::Phrygian,
        Self::Lydian,
        Self::Mixolydian,
        Self::Locrian,
        Self::MinorPentatonic,
        Self::MajorPentatonic,
        Self::HarmonicMinor,
        Self::MelodicMinor,
        Self::Blues,
    ];

    /// Its notes, in semitones above the root, from the root itself.
    pub fn intervals(self) -> &'static [u8] {
        match self {
            Self::Major => &[0, 2, 4, 5, 7, 9, 11],
            Self::Minor => &[0, 2, 3, 5, 7, 8, 10],
            Self::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Self::Phrygian => &[0, 1, 3, 5, 7, 8, 10],
            Self::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Self::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Self::Locrian => &[0, 1, 3, 5, 6, 8, 10],
            Self::MinorPentatonic => &[0, 3, 5, 7, 10],
            Self::MajorPentatonic => &[0, 2, 4, 7, 9],
            Self::HarmonicMinor => &[0, 2, 3, 5, 7, 8, 11],
            Self::MelodicMinor => &[0, 2, 3, 5, 7, 9, 11],
            Self::Blues => &[0, 3, 5, 6, 7, 10],
        }
    }

    /// The name in a list.
    pub fn name(self) -> &'static str {
        match self {
            Self::Major => "Major",
            Self::Minor => "Minor",
            Self::Dorian => "Dorian",
            Self::Phrygian => "Phrygian",
            Self::Lydian => "Lydian",
            Self::Mixolydian => "Mixolydian",
            Self::Locrian => "Locrian",
            Self::MinorPentatonic => "Minor pentatonic",
            Self::MajorPentatonic => "Major pentatonic",
            Self::HarmonicMinor => "Harmonic minor",
            Self::MelodicMinor => "Melodic minor",
            Self::Blues => "Blues",
        }
    }

    /// The name on the status line.
    pub fn short_name(self) -> &'static str {
        match self {
            Self::Major => "Maj",
            Self::Minor => "Min",
            Self::Dorian => "Dor",
            Self::Phrygian => "Phr",
            Self::Lydian => "Lyd",
            Self::Mixolydian => "Mix",
            Self::Locrian => "Loc",
            Self::MinorPentatonic => "MinP",
            Self::MajorPentatonic => "MajP",
            Self::HarmonicMinor => "HMin",
            Self::MelodicMinor => "MMin",
            Self::Blues => "Blues",
        }
    }

    /// A kind from its name, in any case, with a space or a dash between words.
    pub fn parse(text: &str) -> Option<Self> {
        let wanted = text.trim().to_lowercase().replace('-', " ");
        Self::ALL
            .into_iter()
            .find(|kind| kind.name().to_lowercase() == wanted)
    }
}

/// A root from its name, in any case, with sharps or flats: `d`, `F#`, `Bb`.
pub fn parse_root(text: &str) -> Option<u8> {
    let mut letters = text.trim().chars();
    let natural = match letters.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let moved = match (letters.next(), letters.next()) {
        (None, _) => natural,
        (Some('#'), None) => natural + 1,
        (Some('b'), None) => natural + 11,
        _ => return None,
    };
    Some(moved % 12)
}

/// The song's scale. Off, entry and moves are chromatic, and the root and kind wait to be used again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scale {
    pub on: bool,
    /// The note it starts on, from 0 for C up to 11 for B.
    pub root: u8,
    pub kind: ScaleKind,
}

impl Scale {
    /// The root's name.
    pub fn root_name(self) -> &'static str {
        ROOTS[usize::from(self.root) % ROOTS.len()]
    }

    /// Whether a note is in the scale.
    pub fn contains(self, note: u8) -> bool {
        let above = (u16::from(note) + OCTAVE - u16::from(self.root)) % OCTAVE;
        self.kind.intervals().contains(&(above as u8))
    }

    /// The note for a degree counted from 0 at the root, going up through the scale from an octave
    /// on the status line.
    pub fn degree(self, degree: usize, octave: u8) -> u8 {
        let intervals = self.kind.intervals();
        let octaves = u16::from(octave) + 1 + (degree / intervals.len()) as u16;
        let note = octaves * OCTAVE
            + u16::from(self.root)
            + u16::from(intervals[degree % intervals.len()]);
        note.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8
    }

    /// A note moved some way through the scale. One step from a note outside it lands on the next
    /// one in, and it stops at the last one in range.
    pub fn moved(self, note: u8, by: i32) -> u8 {
        let direction = by.signum();
        let mut current = i32::from(note);
        for _ in 0..by.unsigned_abs() {
            let mut next = current + direction;
            while (i32::from(MIN_NOTE)..=i32::from(MAX_NOTE)).contains(&next)
                && !self.contains(next as u8)
            {
                next += direction;
            }
            if !(i32::from(MIN_NOTE)..=i32::from(MAX_NOTE)).contains(&next) {
                break;
            }
            current = next;
        }
        current as u8
    }

    /// The nearest note in the scale, the lower one if two are as near.
    pub fn fitted(self, note: u8) -> u8 {
        let note = i32::from(note);
        (0..i32::from(OCTAVE))
            .flat_map(|distance| [note - distance, note + distance])
            .filter(|near| (i32::from(MIN_NOTE)..=i32::from(MAX_NOTE)).contains(near))
            .find(|near| self.contains(*near as u8))
            .unwrap_or(note) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scale(root: &str, kind: ScaleKind) -> Scale {
        Scale {
            on: true,
            root: parse_root(root).unwrap(),
            kind,
        }
    }

    /// C-4 in this program's numbering, which is MIDI 60.
    const C4: u8 = 60;

    #[test]
    fn every_kind_starts_on_its_root_and_rises_inside_an_octave() {
        for kind in ScaleKind::ALL {
            let intervals = kind.intervals();
            assert_eq!(intervals[0], 0, "{kind:?}");
            assert!(
                intervals.windows(2).all(|pair| pair[0] < pair[1]),
                "{kind:?}"
            );
            assert!(*intervals.last().unwrap() < 12, "{kind:?}");
            assert_eq!(ScaleKind::parse(kind.name()), Some(kind));
            assert_eq!(
                ScaleKind::parse(&kind.name().to_uppercase().replace(' ', "-")),
                Some(kind)
            );
        }
        assert_eq!(ScaleKind::parse("nope"), None);
    }

    #[test]
    fn roots_read_with_sharps_and_flats() {
        assert_eq!(parse_root("c"), Some(0));
        assert_eq!(parse_root("F#"), Some(6));
        assert_eq!(parse_root("Bb"), Some(10));
        assert_eq!(parse_root("cb"), Some(11));
        assert_eq!(parse_root("h"), None);
        assert_eq!(parse_root("c##"), None);
        assert_eq!(parse_root(""), None);
    }

    #[test]
    fn degrees_go_up_from_the_root_in_the_octave_shown() {
        // C major is the white keys.
        let c_major = scale("c", ScaleKind::Major);
        let notes: Vec<u8> = (0..10).map(|degree| c_major.degree(degree, 4)).collect();
        assert_eq!(notes, [60, 62, 64, 65, 67, 69, 71, 72, 74, 76]);
        let d_dorian = scale("d", ScaleKind::Dorian);
        assert_eq!(d_dorian.degree(0, 4), 62);
        assert_eq!(d_dorian.degree(7, 4), 74, "an octave up");
        // A root late in the octave stays in the octave shown.
        assert_eq!(scale("b", ScaleKind::Major).degree(0, 4), 71);
        // A short scale reaches further.
        let pentatonic = scale("a", ScaleKind::MinorPentatonic);
        assert_eq!(pentatonic.degree(0, 4), 69);
        assert_eq!(pentatonic.degree(5, 4), 81);
        assert_eq!(pentatonic.degree(9, 4), 91);
    }

    #[test]
    fn a_degree_stays_a_valid_note_at_the_top() {
        assert_eq!(scale("b", ScaleKind::Major).degree(9, 9), MAX_NOTE);
    }

    #[test]
    fn moving_goes_through_the_scale_and_steps_on_from_outside_it() {
        let c_major = scale("c", ScaleKind::Major);
        assert_eq!(c_major.moved(C4, 1), 62);
        assert_eq!(c_major.moved(C4, 2), 64);
        assert_eq!(c_major.moved(C4, 3), 65, "E to F is a semitone");
        assert_eq!(c_major.moved(C4, -1), 59);
        assert_eq!(c_major.moved(61, 1), 62, "C# up lands on D");
        assert_eq!(c_major.moved(61, -1), 60, "C# down lands on C");
        assert_eq!(c_major.moved(C4, 0), C4);
    }

    #[test]
    fn moving_stops_at_the_last_note_in_range() {
        let c_major = scale("c", ScaleKind::Major);
        let top = c_major.moved(C4, 0xFFFF);
        assert!(c_major.contains(top));
        assert_eq!(c_major.moved(top, 1), top);
        let bottom = c_major.moved(C4, -0xFFFF);
        assert!(c_major.contains(bottom));
        assert_eq!(c_major.moved(bottom, -1), bottom);
    }

    #[test]
    fn fitting_goes_to_the_nearest_and_down_on_a_tie() {
        let c_major = scale("c", ScaleKind::Major);
        assert_eq!(c_major.fitted(C4), C4, "already in");
        assert_eq!(c_major.fitted(61), 60, "C# is as near C as D");
        assert_eq!(c_major.fitted(66), 65, "F# is as near F as G");
        let pentatonic = scale("c", ScaleKind::MinorPentatonic);
        assert_eq!(pentatonic.fitted(62), 63, "D is nearer Eb than C");
        for note in MIN_NOTE..=MAX_NOTE {
            assert!(pentatonic.contains(pentatonic.fitted(note)), "{note}");
        }
    }

    #[test]
    fn a_scale_is_off_and_in_c_major_unless_told_otherwise() {
        let scale = Scale::default();
        assert!(!scale.on);
        assert_eq!(scale.root_name(), "C");
        assert_eq!(scale.kind, ScaleKind::Major);
    }
}
