//! Arpeggios, after Renoise's `-Axy` and the M8's `ARP`: an `ARP xy` goes round its row's note, the
//! note and `x` above it, and the note and `y` above it, in scale degrees with the scale on and
//! semitones with it off, for as long as the note lasts. The track's settings say how fast, in what
//! order, over how many octaves, and whether each note starts again or only changes the pitch.

use serde::{Deserialize, Serialize};

use crate::accumulator::moved_note;
use crate::constants::TICKS_PER_STEP;
use crate::scale::Scale;
use crate::step::{MAX_NOTE, MIN_NOTE};

/// The most octaves an arpeggio climbs before it starts again.
pub const MOST_OCTAVES: u8 = 4;

/// The most notes an arpeggio goes round: three to an octave.
pub const MOST_ARP_NOTES: usize = 3 * MOST_OCTAVES as usize;

/// How long each note of an arpeggio is, as a share of the track's step: a tick, a sixth of a step
/// as `DEL` counts, or a note division as it would be on a track at `1/16`. On a track at another
/// speed each is that much longer or shorter, as everything that's a share of a step is.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum ArpRate {
    HundredTwentyEighth,
    /// A sixth of a step, the tracker feel.
    #[default]
    Tick,
    SixtyFourth,
    ThirtySecondTriplet,
    ThirtySecond,
    SixteenthTriplet,
    Sixteenth,
    EighthTriplet,
    Eighth,
}

impl ArpRate {
    /// Every rate, from the shortest note to the longest.
    pub const ALL: [Self; 9] = [
        Self::HundredTwentyEighth,
        Self::Tick,
        Self::SixtyFourth,
        Self::ThirtySecondTriplet,
        Self::ThirtySecond,
        Self::SixteenthTriplet,
        Self::Sixteenth,
        Self::EighthTriplet,
        Self::Eighth,
    ];

    /// How long each note is, in ticks of a step 240 ticks long.
    pub const fn ticks(self) -> u64 {
        match self {
            Self::HundredTwentyEighth => TICKS_PER_STEP / 8,
            Self::Tick => TICKS_PER_STEP / 6,
            Self::SixtyFourth => TICKS_PER_STEP / 4,
            Self::ThirtySecondTriplet => TICKS_PER_STEP / 3,
            Self::ThirtySecond => TICKS_PER_STEP / 2,
            Self::SixteenthTriplet => TICKS_PER_STEP * 2 / 3,
            Self::Sixteenth => TICKS_PER_STEP,
            Self::EighthTriplet => TICKS_PER_STEP * 4 / 3,
            Self::Eighth => TICKS_PER_STEP * 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::HundredTwentyEighth => "1/128",
            Self::Tick => "Tick",
            Self::SixtyFourth => "1/64",
            Self::ThirtySecondTriplet => "1/32T",
            Self::ThirtySecond => "1/32",
            Self::SixteenthTriplet => "1/16T",
            Self::Sixteenth => "1/16",
            Self::EighthTriplet => "1/8T",
            Self::Eighth => "1/8",
        }
    }

    /// How many notes it plays to a step of the track's.
    pub const fn meaning(self) -> &'static str {
        match self {
            Self::HundredTwentyEighth => "Eight notes to a step",
            Self::Tick => "Six notes to a step, a tick, as DEL counts",
            Self::SixtyFourth => "Four notes to a step",
            Self::ThirtySecondTriplet => "Three notes to a step",
            Self::ThirtySecond => "Two notes to a step",
            Self::SixteenthTriplet => "Three notes in two steps",
            Self::Sixteenth => "A note a step",
            Self::EighthTriplet => "Three notes in four steps",
            Self::Eighth => "A note every two steps",
        }
    }
}

/// The order an arpeggio goes round its notes in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArpMode {
    /// The note, `x`, `y`, then each octave up.
    #[default]
    Up,
    /// The same backwards, from the top.
    Down,
    /// Up and then back down, not playing the top and bottom twice.
    UpDown,
    /// Up and then back down, playing the top and bottom twice, as a phrase's `Up-down-inc` order
    /// plays its end rows.
    UpDownInc,
    /// Any of them each time, from a random stream of the track's own.
    Random,
}

impl ArpMode {
    pub const ALL: [Self; 5] = [
        Self::Up,
        Self::Down,
        Self::UpDown,
        Self::UpDownInc,
        Self::Random,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
            Self::UpDown => "Up-down",
            Self::UpDownInc => "Up-down-inc",
            Self::Random => "Random",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::Up => "The note, x, then y, and on up each octave",
            Self::Down => "The same backwards, from the top",
            Self::UpDown => "Up and back down, the ends played once",
            Self::UpDownInc => "Up and back down, the ends played twice",
            Self::Random => "Any of the notes each time",
        }
    }
}

/// A track's arpeggio settings, which its `ARP`s share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Arpeggio {
    pub rate: ArpRate,
    pub mode: ArpMode,
    /// How many octaves it climbs before it starts again, from 1 to [`MOST_OCTAVES`].
    pub octaves: u8,
    /// Whether each note starts the envelopes again, rather than only changing the pitch.
    pub retrig: bool,
}

impl Default for Arpeggio {
    fn default() -> Self {
        Self {
            rate: ArpRate::default(),
            mode: ArpMode::default(),
            octaves: 1,
            retrig: false,
        }
    }
}

impl Arpeggio {
    /// Whether its settings can be played: from 1 to 4 octaves.
    pub fn in_range(self) -> bool {
        (1..=MOST_OCTAVES).contains(&self.octaves)
    }
}

/// What `Retrig` on or off does, for a list and the status line.
pub fn retrig_meaning(retrig: bool) -> &'static str {
    if retrig {
        "Each note starts again, with its envelopes"
    } else {
        "Legato: each note only changes the pitch, and Acid slides"
    }
}

/// How many octaves it climbs, for the status line.
pub fn octaves_text(octaves: u8) -> String {
    match octaves {
        1 => "1 octave".to_owned(),
        octaves => format!("{octaves} octaves"),
    }
}

/// The notes an arpeggio goes round, in `Up`'s order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cycle {
    notes: [u8; MOST_ARP_NOTES],
    len: usize,
}

impl Cycle {
    /// The notes an `ARP` with this value goes round from a note, before its chain's transpose:
    /// the note, then `x` and `y` above it, leaving out either that's `0`, and the same again an
    /// octave up for each octave past the first. `x` and `y` are scale degrees with the scale on,
    /// and semitones with it off. The transpose goes on after, as an `ACU`'s does.
    pub fn new(note: u8, value: u8, octaves: u8, scale: Scale, transpose: i8) -> Self {
        let intervals = [0, value >> 4, value & 0x0F];
        let mut cycle = Self {
            notes: [0; MOST_ARP_NOTES],
            len: 0,
        };
        for octave in 0..i16::from(octaves.clamp(1, MOST_OCTAVES)) {
            for (at, interval) in intervals.into_iter().enumerate() {
                if at > 0 && interval == 0 {
                    continue;
                }
                let moved = moved_note(note, i16::from(interval), scale);
                let played = (i16::from(moved) + 12 * octave + i16::from(transpose))
                    .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE));
                cycle.notes[cycle.len] = played as u8;
                cycle.len += 1;
            }
        }
        cycle
    }

    /// The notes, in `Up`'s order.
    pub fn notes(&self) -> &[u8] {
        &self.notes[..self.len]
    }

    /// Whether it goes round more than one note, so there's an arpeggio to play.
    pub fn moves(&self) -> bool {
        self.len > 1
    }

    /// Which of its notes it plays at this many notes in, from 0, in this order. `Random` takes
    /// what's rolled.
    pub fn place(&self, mode: ArpMode, played: u64, roll: u32) -> usize {
        let len = self.len.max(1) as u64;
        let at = match mode {
            ArpMode::Up => played % len,
            ArpMode::Down => len - 1 - played % len,
            ArpMode::UpDown if len < 2 => 0,
            ArpMode::UpDown => {
                let round = 2 * len - 2;
                let along = played % round;
                if along < len { along } else { round - along }
            }
            ArpMode::UpDownInc => {
                let round = 2 * len;
                let along = played % round;
                if along < len {
                    along
                } else {
                    round - 1 - along
                }
            }
            ArpMode::Random => u64::from(roll) % len,
        };
        at as usize
    }

    /// The note at a place, or the last one past its end, as a tie onto fewer notes keeps going.
    pub fn note(&self, place: usize) -> u8 {
        self.notes[place.min(self.len.saturating_sub(1))]
    }
}

/// What an `ARP` with this value goes round, for the status line.
pub fn arp_text(value: u8, scale_on: bool) -> String {
    let (x, y) = (value >> 4, value & 0x0F);
    let unit = |size: u8| match (scale_on, size) {
        (true, 1) => "scale degree",
        (true, _) => "scale degrees",
        (false, 1) => "semitone",
        (false, _) => "semitones",
    };
    match (x, y) {
        (0, 0) => "only the note's octaves".to_owned(),
        (0, one) | (one, 0) => format!("the note and {one} {} up", unit(one)),
        (x, y) => format!("the note, {x} and {y} {} up", unit(y.max(x))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::ScaleKind;

    const OFF: Scale = Scale {
        on: false,
        root: 0,
        kind: ScaleKind::Major,
    };

    /// The notes it plays, from the first, in this order.
    fn played(cycle: &Cycle, mode: ArpMode, notes: u64) -> Vec<u8> {
        (0..notes)
            .map(|at| cycle.note(cycle.place(mode, at, 0)))
            .collect()
    }

    #[test]
    fn a_cycle_is_the_note_then_x_and_y_and_each_octave_up() {
        assert_eq!(Cycle::new(60, 0x47, 1, OFF, 0).notes(), [60, 64, 67]);
        assert_eq!(
            Cycle::new(60, 0x47, 2, OFF, 0).notes(),
            [60, 64, 67, 72, 76, 79]
        );
        // As written, not sorted.
        assert_eq!(Cycle::new(60, 0x73, 1, OFF, 0).notes(), [60, 67, 63]);
        // A 0 leaves that one out, for two notes.
        assert_eq!(Cycle::new(60, 0x70, 1, OFF, 0).notes(), [60, 67]);
        assert_eq!(Cycle::new(60, 0x07, 1, OFF, 0).notes(), [60, 67]);
        assert!(!Cycle::new(60, 0x00, 1, OFF, 0).moves());
        assert_eq!(Cycle::new(60, 0x00, 3, OFF, 0).notes(), [60, 72, 84]);
        // The transpose goes on after, and the notes stay in range.
        assert_eq!(Cycle::new(60, 0x47, 1, OFF, 2).notes(), [62, 66, 69]);
        assert_eq!(Cycle::new(MAX_NOTE, 0x47, 1, OFF, 0).notes(), [MAX_NOTE; 3]);
    }

    #[test]
    fn with_the_scale_on_x_and_y_are_degrees() {
        let minor = Scale {
            on: true,
            root: 9,
            kind: ScaleKind::Minor,
        };
        // A minor from A-4: up two degrees is C, up four E.
        assert_eq!(Cycle::new(69, 0x24, 1, minor, 0).notes(), [69, 72, 76]);
        let major = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        assert_eq!(
            Cycle::new(60, 0x24, 2, major, 0).notes(),
            [60, 64, 67, 72, 76, 79]
        );
    }

    #[test]
    fn the_modes_go_round_in_their_orders() {
        let cycle = Cycle::new(60, 0x47, 1, OFF, 0);
        assert_eq!(played(&cycle, ArpMode::Up, 7), [60, 64, 67, 60, 64, 67, 60]);
        assert_eq!(
            played(&cycle, ArpMode::Down, 7),
            [67, 64, 60, 67, 64, 60, 67]
        );
        assert_eq!(
            played(&cycle, ArpMode::UpDown, 9),
            [60, 64, 67, 64, 60, 64, 67, 64, 60]
        );
        assert_eq!(
            played(&cycle, ArpMode::UpDownInc, 9),
            [60, 64, 67, 67, 64, 60, 60, 64, 67]
        );
        let two = Cycle::new(60, 0x07, 1, OFF, 0);
        assert_eq!(played(&two, ArpMode::UpDown, 4), [60, 67, 60, 67]);
        assert_eq!(
            played(&two, ArpMode::UpDownInc, 6),
            [60, 67, 67, 60, 60, 67]
        );
        let one = Cycle::new(60, 0x00, 1, OFF, 0);
        assert_eq!(played(&one, ArpMode::UpDownInc, 3), [60, 60, 60]);
        assert_eq!(cycle.place(ArpMode::Random, 5, 7), 1);
    }

    #[test]
    fn rates_are_shares_of_a_step() {
        let ticks: Vec<u64> = ArpRate::ALL.iter().map(|rate| rate.ticks()).collect();
        assert_eq!(ticks, [30, 40, 60, 80, 120, 160, 240, 320, 480]);
        assert_eq!(ArpRate::default(), ArpRate::Tick);
    }

    #[test]
    fn the_text_says_what_it_goes_round() {
        assert_eq!(arp_text(0x47, false), "the note, 4 and 7 semitones up");
        assert_eq!(arp_text(0x20, true), "the note and 2 scale degrees up");
        assert_eq!(arp_text(0x01, false), "the note and 1 semitone up");
        assert_eq!(arp_text(0x00, true), "only the note's octaves");
    }

    #[test]
    fn settings_are_in_range_from_one_to_four_octaves() {
        assert!(Arpeggio::default().in_range());
        let octaves = |octaves| Arpeggio {
            octaves,
            ..Arpeggio::default()
        };
        assert!(octaves(4).in_range());
        assert!(!octaves(0).in_range() && !octaves(5).in_range());
    }
}
