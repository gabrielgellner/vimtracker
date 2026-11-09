//! Accumulators, after the Metropolix's ACCUM: a row's note that moves on each time it plays, by
//! scale degrees, or semitones with the scale off. An `ACU` in an FX slot holds how far, and the
//! track's settings say how far it can go, what happens there, whether it's the row or the whole
//! track that moves, and when it starts again from the notes as written.

use serde::{Deserialize, Serialize};

use crate::scale::Scale;
use crate::step::{MAX_NOTE, MIN_NOTE};

/// The furthest an accumulator can take a note either way, and the most an `ACU` moves it at once:
/// 28 scale degrees, four octaves of a seven-note scale, as on the Metropolix.
pub const MOST_DEGREES: u8 = 28;

/// How far an accumulator goes either way unless changed: an octave of a seven-note scale.
pub const DEFAULT_LIMIT: u8 = 7;

/// The most times round its chain a track can count before its accumulators start again.
pub const MOST_LOOPS: u8 = 16;

/// Whose note an `ACU` moves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Moves {
    /// Only its own row's, as the Metropolix's Stage mode.
    #[default]
    Row,
    /// Every row's on the track, with or without an `ACU` of its own, by what the track's `ACU`s
    /// have added up to.
    Track,
}

/// What an accumulator does at one of its limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtLimit {
    /// Past it, it carries on from the note as written.
    #[default]
    Wrap,
    /// At it, it turns back, and turns again at the note as written.
    Bounce,
    /// It stays at it until it starts again.
    Hold,
}

/// When an `ACU` adds, as the Metropolix's transpose trigger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Adds {
    /// Once each time its row plays.
    #[default]
    Row,
    /// On each pulse a `MLT` plays, and once for any other gate.
    Pulse,
    /// On each hit of a `RET` too, as well as each pulse a `MLT` plays.
    Retrig,
}

/// When a track's accumulators go back to the notes as written. Playing always starts them there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reset {
    /// Wherever the track's chain starts, as a condition's passes start again: the top, a section
    /// that brings the chain in or resets, a launch and `:reset`.
    #[default]
    Chain,
    /// Only when playing starts, so it carries on across sections and chains.
    Play,
    /// Every so many times round its chain, from 1 to [`MOST_LOOPS`], and where the chain starts.
    Loops(u8),
}

/// A track's accumulator settings, which its `ACU`s share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Accumulation {
    /// How far above the note as written it can go, from 1 to [`MOST_DEGREES`].
    pub up: u8,
    /// How far below.
    pub down: u8,
    pub moves: Moves,
    pub at_limit: AtLimit,
    pub adds: Adds,
    pub reset: Reset,
}

impl Default for Accumulation {
    fn default() -> Self {
        Self {
            up: DEFAULT_LIMIT,
            down: DEFAULT_LIMIT,
            moves: Moves::default(),
            at_limit: AtLimit::default(),
            adds: Adds::default(),
            reset: Reset::default(),
        }
    }
}

impl Accumulation {
    /// Whether its settings can be played: both limits from 1 to 28, and from 1 to 16 loops.
    pub fn in_range(self) -> bool {
        let limit = 1..=MOST_DEGREES;
        let loops = match self.reset {
            Reset::Loops(loops) => (1..=MOST_LOOPS).contains(&loops),
            _ => true,
        };
        limit.contains(&self.up) && limit.contains(&self.down) && loops
    }
}

/// How far an `ACU` value moves its note each time, as a signed byte: `01` to `1C` up, `FF` to
/// `E4` down, and `00` not at all. One past 28 either way isn't an amount.
pub fn amount(value: u8) -> Option<i8> {
    let amount = value as i8;
    (amount.unsigned_abs() <= MOST_DEGREES).then_some(amount)
}

/// An amount's `ACU` value.
pub fn amount_value(amount: i8) -> u8 {
    amount as u8
}

/// What an amount reads as on the status line, in degrees or semitones as the scale is on or off.
pub fn amount_text(amount: i8, scale_on: bool) -> String {
    let size = amount.unsigned_abs();
    let unit = match (scale_on, size) {
        (true, 1) => "scale degree",
        (true, _) => "scale degrees",
        (false, 1) => "semitone",
        (false, _) => "semitones",
    };
    match amount {
        0 => "doesn't move".to_owned(),
        up if up > 0 => format!("up {size} {unit}"),
        _ => format!("down {size} {unit}"),
    }
}

/// Where an accumulator has taken its notes: how far from the note as written, in degrees or
/// semitones, and whether a bounce has turned it back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Walk {
    pub offset: i16,
    pub turned: bool,
}

impl Walk {
    /// Add an amount, as these settings say at their limits. Wrap and bounce go back to the note as
    /// written, the Metropolix's unipolar orders, so a rising line never goes below it.
    pub fn add(&mut self, amount: i8, settings: Accumulation) {
        let (up, down) = (i16::from(settings.up), i16::from(settings.down));
        match settings.at_limit {
            AtLimit::Hold => {
                self.offset = (self.offset + i16::from(amount)).clamp(-down, up);
            }
            AtLimit::Wrap => {
                let moved = self.offset + i16::from(amount);
                self.offset = if moved > up {
                    (moved - up - 1).rem_euclid(up + 1)
                } else if moved < -down {
                    -(-moved - down - 1).rem_euclid(down + 1)
                } else {
                    moved
                };
            }
            AtLimit::Bounce => {
                // A positive amount goes between the note and the upper limit, and a negative one
                // between the lower limit and the note, turning back each time it passes one.
                let (low, high) = if amount >= 0 { (0, up) } else { (-down, 0) };
                let step = if self.turned {
                    -i16::from(amount)
                } else {
                    i16::from(amount)
                };
                let mut moved = self.offset + step;
                let mut rising = step > 0;
                // Each turn takes some of the way back, so a large amount turns more than once.
                for _ in 0..=MOST_DEGREES {
                    if rising && moved > high {
                        moved = 2 * high - moved;
                    } else if !rising && moved < low {
                        moved = 2 * low - moved;
                    } else {
                        break;
                    }
                    rising = !rising;
                    self.turned = !self.turned;
                }
                self.offset = moved.clamp(-down, up);
            }
        }
    }
}

/// A note moved by an accumulator's offset: by scale degrees with the scale on, a note outside it
/// landing on the next one in, or by semitones with it off, and kept to the notes there are.
pub fn moved_note(note: u8, offset: i16, scale: Scale) -> u8 {
    if offset == 0 {
        return note;
    }
    if scale.on {
        return scale.moved(note, i32::from(offset));
    }
    (i16::from(note) + offset).clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8
}

impl Moves {
    pub const ALL: [Self; 2] = [Self::Row, Self::Track];

    pub fn name(self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Track => "Track",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::Row => "Only the row with the ACU moves",
            Self::Track => "Every row on the track moves with the ACUs",
        }
    }
}

impl AtLimit {
    pub const ALL: [Self; 3] = [Self::Wrap, Self::Bounce, Self::Hold];

    pub fn name(self) -> &'static str {
        match self {
            Self::Wrap => "Wrap",
            Self::Bounce => "Bounce",
            Self::Hold => "Hold",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::Wrap => "Past the limit, carry on from the note as written",
            Self::Bounce => "Turn back at the limit, and again at the note as written",
            Self::Hold => "Stay at the limit until it starts again",
        }
    }
}

impl Adds {
    pub const ALL: [Self; 3] = [Self::Row, Self::Pulse, Self::Retrig];

    pub fn name(self) -> &'static str {
        match self {
            Self::Row => "Row",
            Self::Pulse => "Pulse",
            Self::Retrig => "Retrig",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::Row => "Once each time the row plays",
            Self::Pulse => "On every pulse of a MLT",
            Self::Retrig => "On every hit of a RET, and every pulse of a MLT",
        }
    }

    /// What one add is called, for the status line.
    pub fn each(self) -> &'static str {
        match self {
            Self::Row => "each time it plays",
            Self::Pulse => "each pulse",
            Self::Retrig => "each hit",
        }
    }
}

impl Reset {
    /// Every choice, in the order a list shows them: the chain, playing, then 1 to 16 loops.
    pub fn all() -> impl Iterator<Item = Self> {
        [Self::Chain, Self::Play]
            .into_iter()
            .chain((1..=MOST_LOOPS).map(Self::Loops))
    }

    pub fn name(self) -> String {
        match self {
            Self::Chain => "Chain".to_owned(),
            Self::Play => "Play".to_owned(),
            Self::Loops(1) => "1 loop".to_owned(),
            Self::Loops(loops) => format!("{loops} loops"),
        }
    }

    pub fn meaning(self) -> String {
        match self {
            Self::Chain => "Wherever the track's chain starts".to_owned(),
            Self::Play => "Only when playing starts".to_owned(),
            Self::Loops(1) => "Each time round the chain".to_owned(),
            Self::Loops(loops) => format!("Every {loops} times round the chain"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    /// The offsets an accumulator plays over some adds, from the note as written.
    fn walk(amount: i8, settings: Accumulation, adds: usize) -> Vec<i16> {
        let mut walk = Walk::default();
        let mut offsets = vec![walk.offset];
        for _ in 0..adds {
            walk.add(amount, settings);
            offsets.push(walk.offset);
        }
        offsets
    }

    fn limits(up: u8, down: u8, at_limit: AtLimit) -> Accumulation {
        Accumulation {
            up,
            down,
            at_limit,
            ..Accumulation::default()
        }
    }

    #[test]
    fn wrap_carries_on_from_the_note_as_written() {
        let wrap = limits(3, 2, AtLimit::Wrap);
        assert_eq!(walk(1, wrap, 8), [0, 1, 2, 3, 0, 1, 2, 3, 0]);
        assert_eq!(walk(2, wrap, 5), [0, 2, 0, 2, 0, 2]);
        assert_eq!(walk(3, limits(7, 7, AtLimit::Wrap), 4), [0, 3, 6, 1, 4]);
        assert_eq!(walk(-1, wrap, 6), [0, -1, -2, 0, -1, -2, 0]);
    }

    #[test]
    fn bounce_turns_at_the_limit_and_at_the_note() {
        let bounce = limits(3, 2, AtLimit::Bounce);
        assert_eq!(walk(1, bounce, 8), [0, 1, 2, 3, 2, 1, 0, 1, 2]);
        assert_eq!(walk(-1, bounce, 6), [0, -1, -2, -1, 0, -1, -2]);
        // A step past the limit comes back as far as it went over.
        assert_eq!(
            walk(3, limits(7, 7, AtLimit::Bounce), 6),
            [0, 3, 6, 5, 2, 1, 4]
        );
    }

    #[test]
    fn hold_stays_at_the_limit() {
        let hold = limits(3, 2, AtLimit::Hold);
        assert_eq!(walk(1, hold, 5), [0, 1, 2, 3, 3, 3]);
        assert_eq!(walk(-2, hold, 3), [0, -2, -2, -2]);
    }

    #[test]
    fn amounts_are_signed_bytes_up_to_28() {
        assert_eq!(amount(0x01), Some(1));
        assert_eq!(amount(0x1C), Some(28));
        assert_eq!(amount(0xFF), Some(-1));
        assert_eq!(amount(0xE4), Some(-28));
        assert_eq!(amount(0x00), Some(0));
        assert_eq!(amount(0x1D), None);
        assert_eq!(amount(0xE3), None);
        assert_eq!(amount(0x80), None);
        assert_eq!(amount_value(-3), 0xFD);
        assert_eq!(amount_text(-3, true), "down 3 scale degrees");
        assert_eq!(amount_text(1, false), "up 1 semitone");
    }

    #[test]
    fn a_note_moves_by_degrees_with_the_scale_on_and_semitones_with_it_off() {
        let major = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        // C-4 is 60: up two degrees of C major is E, and down one is B below.
        assert_eq!(moved_note(60, 2, major), 64);
        assert_eq!(moved_note(60, -1, major), 59);
        assert_eq!(moved_note(60, 7, major), 72);
        let off = Scale { on: false, ..major };
        assert_eq!(moved_note(60, 2, off), 62);
        assert_eq!(moved_note(MAX_NOTE, 5, off), MAX_NOTE);
    }

    #[test]
    fn settings_are_in_range_only_within_their_limits() {
        assert!(Accumulation::default().in_range());
        assert!(!limits(0, 7, AtLimit::Wrap).in_range());
        assert!(!limits(7, 29, AtLimit::Wrap).in_range());
        let loops = |loops| Accumulation {
            reset: Reset::Loops(loops),
            ..Accumulation::default()
        };
        assert!(loops(16).in_range());
        assert!(!loops(0).in_range() && !loops(17).in_range());
        assert_eq!(Reset::all().count(), 18);
    }
}
