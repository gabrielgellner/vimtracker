//! `Follow`: the Note column mirroring another phrase, its source, for a canon. It writes the
//! source's notes with their velocities and gates, and with `fx` their FX too, into the rows
//! selected, looping a source shorter than them and cutting off a longer one, with the canon's
//! moves on the way: `Reverse` (retrograde), `Rotate` (a delayed entry), `Invert` and `Transpose`.
//! Each track still owns its phrases: a follower holds a copy, which its live generator rewrites
//! whenever the source changes.

use serde::{Deserialize, Serialize};

use super::{Ladder, ShapeColumn};
use crate::selection::StepRange;
use crate::step::{FxSlot, Note, Step};

/// The furthest `Transpose` goes either way, in degrees with the scale on and semitones off.
pub const MAX_TRANSPOSE: i8 = 48;

/// A phrase, by its track and its ID within it, as a `Follow` names its source.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Source {
    pub track: usize,
    pub phrase: u32,
}

/// As the grid names a track and a phrase: `T02:05` for phrase `05` on the second track.
impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T{:02}:{:02X}", self.track + 1, self.phrase)
    }
}

/// What a `Follow` fill reads and how it changes it on the way. Saved with a live generator's
/// shape, and left out while it's as it starts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Follow {
    pub source: Source,
    /// Up or down, in the scale's degrees while it's on and in semitones while it's off, after any
    /// inversion.
    pub transpose: i8,
    /// Mirrors the notes about `axis`, in degrees while the scale is on.
    pub invert: bool,
    /// The note inverted about, or none for the source's first note.
    pub axis: Option<u8>,
    /// Plays the source backwards: its last row first.
    pub reverse: bool,
    /// Comes in this many rows late, going round the source, as a canon's later voice does.
    pub rotate: u16,
    /// Copies the source's FX columns too. Off, as a lock may mean something else on another
    /// machine.
    pub fx: bool,
}

impl Follow {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// The first note a phrase plays, which `Invert` turns about unless told otherwise.
pub fn first_note(source: &[Step]) -> Option<u8> {
    source.iter().find_map(|step| match step.note {
        Note::On(note) => Some(note),
        _ => None,
    })
}

/// Copy a step's FX slots onto another's, as many as it has, emptying any the source hasn't.
pub(crate) fn copy_fx(from: &Step, to: &mut Step) {
    for (slot, cell) in to.fx.iter_mut().enumerate() {
        *cell = from.fx.get(slot).copied().unwrap_or(FxSlot::default());
    }
}

/// The rows a `Follow` fill writes, every row of the range, each with its step as it leaves it:
/// the source's note, velocity and gate on that row once the source is turned round and rotated,
/// with its pitch inverted and transposed. A source with no rows writes nothing.
pub(super) fn written(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    follow: &Follow,
    source: &[Step],
) -> Vec<(usize, Step)> {
    let ShapeColumn::Note { scale, .. } = column else {
        return Vec::new();
    };
    if source.is_empty() {
        return Vec::new();
    }
    let ladder = Ladder::new(scale);
    let axis = follow
        .axis
        .or_else(|| first_note(source))
        .map(|note| ladder.position(note));
    let pitch = |note: u8| {
        let mut at = ladder.position(note);
        if follow.invert
            && let Some(axis) = axis
        {
            at = 2.0 * axis - at;
        }
        ladder.note(at + f64::from(follow.transpose))
    };
    let length = source.len();
    let late = usize::from(follow.rotate) % length;
    (range.first..=range.last)
        .filter_map(|row| {
            let mut step = steps.get(row)?.clone();
            // Late by `rotate`, so the row reads the source's from that many before it.
            let at = (row - range.first + length - late) % length;
            let from = &source[if follow.reverse { length - 1 - at } else { at }];
            step.note = match from.note {
                Note::On(note) => Note::On(pitch(note)),
                other => other,
            };
            step.velocity = from.velocity;
            step.gate = from.gate;
            if follow.fx {
                copy_fx(from, &mut step);
            }
            Some((row, step))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::{Scale, ScaleKind};

    const C_MAJOR: Scale = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    const OFF: Scale = Scale {
        on: false,
        root: 0,
        kind: ScaleKind::Major,
    };

    fn column(scale: Scale) -> ShapeColumn {
        ShapeColumn::Note {
            scale,
            writes: None,
        }
    }

    fn rows(steps: &[&str]) -> Vec<Step> {
        steps.iter().map(|text| text.parse().unwrap()).collect()
    }

    /// The theme: C-3 E-3 G-3, a rest, then D-3, at velocities of their own, the E accented and
    /// sliding.
    fn theme() -> Vec<Step> {
        rows(&[
            "C-3 50 --- --- ---",
            "E-3 7F HLD1 --- --- ---",
            "G-3 40 --- --- ---",
            "--- -- --- --- ---",
            "D-3 60 --- --- ---",
        ])
    }

    fn empty(count: usize) -> Vec<Step> {
        rows(&vec!["--- -- --- --- ---"; count])
    }

    fn follow(steps: &[Step], scale: Scale, follow: Follow, source: &[Step]) -> Vec<String> {
        let range = StepRange::between(0, steps.len() - 1);
        written(steps, range, column(scale), &follow, source)
            .into_iter()
            .map(|(_, step)| step.to_string())
            .collect()
    }

    fn notes(written: &[String]) -> Vec<&str> {
        written.iter().map(|text| &text[..3]).collect()
    }

    #[test]
    fn it_copies_the_notes_with_their_velocities_and_gates() {
        let out = follow(&empty(5), C_MAJOR, Follow::default(), &theme());
        let theme: Vec<String> = theme().iter().map(ToString::to_string).collect();
        assert_eq!(out, theme);
    }

    #[test]
    fn a_shorter_source_loops_and_a_longer_one_is_cut_off() {
        let out = follow(&empty(7), OFF, Follow::default(), &theme());
        assert_eq!(
            notes(&out),
            ["C-3", "E-3", "G-3", "---", "D-3", "C-3", "E-3"]
        );
        let out = follow(&empty(3), OFF, Follow::default(), &theme());
        assert_eq!(notes(&out), ["C-3", "E-3", "G-3"]);
    }

    #[test]
    fn transpose_moves_in_degrees_with_the_scale_on_and_semitones_off() {
        let up = Follow {
            transpose: 4,
            ..Follow::default()
        };
        // A fifth up in C major is four degrees: C E G D to G B D A.
        let out = follow(&empty(5), C_MAJOR, up, &theme());
        assert_eq!(notes(&out), ["G-3", "B-3", "D-4", "---", "A-3"]);
        // Off, four semitones: C E G D to E G# B F#.
        let out = follow(&empty(5), OFF, up, &theme());
        assert_eq!(notes(&out), ["E-3", "G#3", "B-3", "---", "F#3"]);
    }

    #[test]
    fn invert_mirrors_about_the_first_note_or_one_chosen() {
        let inverted = Follow {
            invert: true,
            ..Follow::default()
        };
        // About C-3 in C major: E (2 degrees up) goes 2 down to A-2, G to F-2, D to B-2.
        let out = follow(&empty(5), C_MAJOR, inverted, &theme());
        assert_eq!(notes(&out), ["C-3", "A-2", "F-2", "---", "B-2"]);
        // In semitones: E (+4) to G#2, G (+7) to F-2, D (+2) to A#2.
        let out = follow(&empty(5), OFF, inverted, &theme());
        assert_eq!(notes(&out), ["C-3", "G#2", "F-2", "---", "A#2"]);
        // About E-3: C to G#3 and E stays, chromatically.
        let about_e = Follow {
            invert: true,
            axis: Some(64),
            ..Follow::default()
        };
        let out = follow(&empty(5), OFF, about_e, &theme());
        assert_eq!(notes(&out), ["G#3", "E-3", "C#3", "---", "F#3"]);
        // Inverted and then a fifth up, the fugue's inverted answer.
        let answer = Follow {
            transpose: 4,
            ..inverted
        };
        let out = follow(&empty(5), C_MAJOR, answer, &theme());
        assert_eq!(notes(&out), ["G-3", "E-3", "C-3", "---", "F-3"]);
    }

    #[test]
    fn reverse_plays_it_backwards_with_each_rows_velocity_and_gate() {
        let back = Follow {
            reverse: true,
            ..Follow::default()
        };
        let out = follow(&empty(5), OFF, back, &theme());
        assert_eq!(notes(&out), ["D-3", "---", "G-3", "E-3", "C-3"]);
        assert_eq!(out[3], "E-3 7F HLD1 --- --- ---");
    }

    #[test]
    fn rotate_brings_it_in_late_going_round_the_source() {
        let late = Follow {
            rotate: 2,
            ..Follow::default()
        };
        let out = follow(&empty(5), OFF, late, &theme());
        assert_eq!(notes(&out), ["---", "D-3", "C-3", "E-3", "G-3"]);
        // Late by more than the source goes round it.
        let later = Follow {
            rotate: 7,
            ..Follow::default()
        };
        assert_eq!(follow(&empty(5), OFF, later, &theme()), out);
        // Reversed then late: the retrograde comes in two rows on.
        let both = Follow {
            reverse: true,
            ..late
        };
        let out = follow(&empty(5), OFF, both, &theme());
        assert_eq!(notes(&out), ["E-3", "C-3", "D-3", "---", "G-3"]);
    }

    #[test]
    fn rests_and_offs_come_through_and_fx_only_on_request() {
        let source = rows(&[
            "C-3 -- --- cut:40 ---",
            "OFF -- --- --- ---",
            "--- -- --- res:20 ---",
        ]);
        let steps = rows(&vec!["A-4 30 --- pan:10 ---"; 3]);
        let out = follow(&steps, OFF, Follow::default(), &source);
        assert_eq!(
            out,
            [
                "C-3 -- --- pan:10 ---",
                "OFF -- --- pan:10 ---",
                "--- -- --- pan:10 ---"
            ]
        );
        let with_fx = Follow {
            fx: true,
            ..Follow::default()
        };
        let out = follow(&steps, OFF, with_fx, &source);
        assert_eq!(
            out,
            [
                "C-3 -- --- cut:40 ---",
                "OFF -- --- --- ---",
                "--- -- --- res:20 ---"
            ]
        );
    }

    #[test]
    fn a_source_is_named_as_the_grid_names_it() {
        let source = Source {
            track: 1,
            phrase: 5,
        };
        assert_eq!(source.to_string(), "T02:05");
    }
}
