//! `Divisions`: a rhythm from a chance for each note division to strike, after Vermona's
//! randomRHYTHM. Each strike a division could make in a bar, the quarters', the eighths', the
//! sixteenths' and the three triplets', has a die of its own, drawn from the seed and its place in
//! the bar, and its division's chance is the threshold the die must clear: at `0%` the division
//! never strikes, at `100%` always, and in between, raising it only adds strikes, as the module's
//! sliders reshape the pattern its `Dice` mode holds. The dice are the same each bar, so the
//! pattern cycles; `Vary` rolls new ones each pass, the module's `Realtime`.
//!
//! The divisions share one track: the straight ones fall on the rows, and a triplet strike that
//! falls between two rows is the nearer row's, nudged a third of a row early or late. On a track
//! at `1/32` every division has rows of its own, the sixteenths on the even rows and the triplets
//! on the odd ones; at `1/16` the eighth and quarter triplets take the odd rows, which the
//! sixteenths would too, so a row with two strikes plays the one on the row, or else the earlier.
//!
//! The track's steps are its clock, as the module counts the clock it's given: on a track at
//! another speed, its bar is as many of the track's steps, so at `1/2` it plays the rhythm eight
//! times as slow, and a triplet's nudge is a third of the track's row, as far as its nudges go.
//!
//! The module's two channels are two tracks: each plays its own channel's dice, `A` or `B`, and a
//! third can play `A+B`, the strikes of both, as the module's combined output does.

use serde::{Deserialize, Serialize};

use super::{GenCell, ShapeColumn, mix};
use crate::constants::{DEFAULT_NOTE_VELOCITY, NUDGE_TICKS, TICKS_PER_QUARTER, TICKS_PER_STEP};
use crate::selection::StepRange;
use crate::step::{Note, Step, most_nudge};

/// How many divisions there are, each with its chance.
pub const DIVISIONS: usize = 6;
/// What the dice are for, so they don't follow another fill's.
const DICE: u64 = 0xD1CE_0602;

/// A note division a strike can fall on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Division {
    Quarter,
    Eighth,
    Sixteenth,
    QuarterTriplet,
    EighthTriplet,
    SixteenthTriplet,
}

impl Division {
    /// Every division, in the order their chances are kept: the straight ones, then the triplets.
    pub const ALL: [Self; DIVISIONS] = [
        Self::Quarter,
        Self::Eighth,
        Self::Sixteenth,
        Self::QuarterTriplet,
        Self::EighthTriplet,
        Self::SixteenthTriplet,
    ];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Quarter => "1/4",
            Self::Eighth => "1/8",
            Self::Sixteenth => "1/16",
            Self::QuarterTriplet => "1/4T",
            Self::EighthTriplet => "1/8T",
            Self::SixteenthTriplet => "1/16T",
        }
    }

    /// Its place among the divisions, which its chance is kept at.
    pub fn index(self) -> usize {
        self as usize
    }

    /// How far apart its strikes are, in ticks.
    pub const fn ticks(self) -> u64 {
        match self {
            Self::Quarter => TICKS_PER_QUARTER,
            Self::Eighth => TICKS_PER_QUARTER / 2,
            Self::Sixteenth => TICKS_PER_QUARTER / 4,
            Self::QuarterTriplet => TICKS_PER_QUARTER * 2 / 3,
            Self::EighthTriplet => TICKS_PER_QUARTER / 3,
            Self::SixteenthTriplet => TICKS_PER_QUARTER / 6,
        }
    }
}

/// Which of the module's channels a track plays: its own dice, `A` or `B`, or the strikes of both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Channel {
    #[default]
    A,
    B,
    Both,
}

impl Channel {
    /// Every channel, in the order tracks across take them.
    pub const ALL: [Self; 3] = [Self::A, Self::B, Self::Both];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::Both => "A+B",
        }
    }

    /// What it plays, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::A => "Channel A's dice",
            Self::B => "Channel B's dice",
            Self::Both => "Both channels' strikes, each at its own chances",
        }
    }

    /// Its place among the channels.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The channel the panel gives the `nth` of several tracks: `A`, `B`, then `A+B`, going round.
    pub fn of_track(nth: usize) -> Self {
        Self::ALL[nth % Self::ALL.len()]
    }
}

/// How long the dice's bar is and how long a row: a bar of four or three beats, in sixteenths, or
/// in 32nds for a track at `1/32`, where every division has rows of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Bar {
    #[default]
    Four,
    Three,
    FourFine,
    ThreeFine,
}

impl Bar {
    /// Every bar, in the order a list offers them.
    pub const ALL: [Self; 4] = [Self::Four, Self::Three, Self::FourFine, Self::ThreeFine];

    /// The name on screen.
    pub fn name(self) -> &'static str {
        match self {
            Self::Four => "4/4 16ths",
            Self::Three => "3/4 16ths",
            Self::FourFine => "4/4 32nds",
            Self::ThreeFine => "3/4 32nds",
        }
    }

    /// What it is, for a list to choose from.
    pub fn description(self) -> &'static str {
        match self {
            Self::Four => "16 rows a bar, for a track at 1/16",
            Self::Three => "12 rows a bar, for a track at 1/16",
            Self::FourFine => "32 rows a bar, for a track at 1/32",
            Self::ThreeFine => "24 rows a bar, for a track at 1/32",
        }
    }

    /// Its place among the bars.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether a row is a 32nd.
    pub fn fine(self) -> bool {
        matches!(self, Self::FourFine | Self::ThreeFine)
    }

    /// How long a row is, in ticks.
    pub fn row_ticks(self) -> u64 {
        if self.fine() {
            TICKS_PER_STEP / 2
        } else {
            TICKS_PER_STEP
        }
    }

    /// How long the bar is, in ticks.
    fn ticks(self) -> u64 {
        let beats = match self {
            Self::Four | Self::FourFine => 4,
            Self::Three | Self::ThreeFine => 3,
        };
        beats * TICKS_PER_QUARTER
    }

    /// How many rows the bar takes.
    pub fn rows(self) -> usize {
        (self.ticks() / self.row_ticks()) as usize
    }

    /// The bar for a track at this many ticks a step: 32nds at `1/32`, and sixteenths otherwise.
    pub fn for_step(ticks: u64) -> Self {
        if ticks < TICKS_PER_STEP {
            Self::FourFine
        } else {
            Self::Four
        }
    }
}

/// A `Divisions` fill's chances, the channel it plays and its bar. Saved with a live generator's
/// shape, and left out while it's as it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Divisions {
    /// Each division's chance, in percent, in [`Division::ALL`]'s order: first the track's own
    /// channel's, or channel `A`'s for `A+B`, then channel `B`'s, which only `A+B` plays.
    pub chances: [[u8; DIVISIONS]; 2],
    /// The channel it plays. None only while the panel shapes several tracks at once, each taking
    /// its own.
    pub channel: Option<Channel>,
    pub bar: Bar,
    /// How long the track's step was when the panel opened, in ticks, where that isn't the bar's
    /// row, as at `1/8`: its rows play that long, so a triplet's nudge is a third of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<u16>,
}

impl Default for Divisions {
    /// Channel `A` a part on the beats with a few eighths and sixteenths, and `B` eighths and
    /// eighth triplets, which share no rows at `1/16`.
    fn default() -> Self {
        Self {
            chances: [[75, 25, 10, 0, 0, 0], [0, 50, 0, 0, 25, 0]],
            channel: Some(Channel::A),
            bar: Bar::Four,
            ticks: None,
        }
    }
}

/// Where a strike lands: its row, by its offset in the span, and its nudge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Strike {
    pub offset: usize,
    pub nudge: i8,
}

impl Divisions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// For a track whose step is this many ticks: 32nds at `1/32`, and sixteenths otherwise, its
    /// rows playing as long as the track's.
    pub fn for_step(self, ticks: u64) -> Self {
        Self {
            bar: Bar::for_step(ticks),
            ..self
        }
        .played_at(ticks)
    }

    /// The same, with another bar, its rows still playing as long as the track's.
    pub fn with_bar(self, bar: Bar) -> Self {
        let played = self.played();
        Self { bar, ..self }.played_at(played)
    }

    fn played_at(self, ticks: u64) -> Self {
        let ticks = (ticks != self.bar.row_ticks()).then(|| ticks.min(u64::from(u16::MAX)) as u16);
        Self { ticks, ..self }
    }

    /// How long one of its rows plays, in ticks: the track's step.
    pub fn played(&self) -> u64 {
        self.ticks.map_or(self.bar.row_ticks(), u64::from)
    }

    /// A strike so many ticks off its row in the bar, as a nudge: as far again on the track's own
    /// row, in 24ths of a sixteenth, to the nearest, and no further than the track's nudges go.
    fn nudge(&self, ticks: i64) -> i8 {
        let played = self.played();
        let scaled = ticks.unsigned_abs() * played / self.bar.row_ticks();
        let size = ((scaled + NUDGE_TICKS / 2) / NUDGE_TICKS).min(most_nudge(played) as u64) as i8;
        if ticks < 0 { -size } else { size }
    }

    /// The earliest a strike is nudged, in 24ths of a sixteenth: a triplet's third of a row, `-8`
    /// on a track at `1/16`, `-4` at `1/32` and -64, `-40` as the grid shows it, at `1/2`.
    pub fn earliest(&self) -> i8 {
        self.nudge(-((self.bar.row_ticks() / 3) as i64))
    }

    /// The channel it plays, `A` if it's yet to be given one.
    pub fn channel(&self) -> Channel {
        self.channel.unwrap_or_default()
    }

    /// What the `nth` of several tracks shaped at once plays: a channel of its own, `A`, `B`, then
    /// `A+B`, going round, a `B` taking channel `B`'s chances as its own.
    pub fn for_track(self, nth: usize) -> Self {
        let channel = Channel::of_track(nth);
        let mut chances = self.chances;
        if channel == Channel::B {
            chances[0] = chances[1];
        }
        Self {
            chances,
            channel: Some(channel),
            ..self
        }
    }

    /// Whether its sixteenths and its triplets would share rows: at `1/16`, where the triplets
    /// take the odd rows, with any chance of both.
    pub fn crowded(&self) -> bool {
        if self.bar.fine() {
            return false;
        }
        let sets = match self.channel {
            Some(Channel::A | Channel::B) => &self.chances[..1],
            _ => &self.chances[..],
        };
        let any = |set: &[u8; DIVISIONS], divisions: &[Division]| {
            divisions.iter().any(|division| set[division.index()] > 0)
        };
        let triplets = [Division::QuarterTriplet, Division::EighthTriplet];
        let fine = [Division::Sixteenth, Division::SixteenthTriplet];
        sets.iter().any(|set| any(set, &fine)) && sets.iter().any(|set| any(set, &triplets))
            || sets
                .iter()
                .any(|set| set[Division::SixteenthTriplet.index()] > 0)
    }

    /// Whether a row strikes, by its offset in the span, and its nudge if it does. Its place in
    /// the bar is its offset's, so the bar starts on the span's first row.
    pub fn strike(&self, seed: u64, offset: usize) -> Option<i8> {
        let row = self.bar.row_ticks();
        let at = (offset % self.bar.rows()) as u64 * row;
        let channels: &[(usize, Channel)] = match self.channel() {
            Channel::A => &[(0, Channel::A)],
            Channel::B => &[(0, Channel::B)],
            Channel::Both => &[(0, Channel::A), (1, Channel::B)],
        };
        let mut found: Option<i64> = None;
        for &(set, channel) in channels {
            for division in Division::ALL {
                let chance = self.chances[set][division.index()];
                if chance == 0 {
                    continue;
                }
                for (nth, tick) in points(division, at, row, self.bar.ticks()) {
                    if !struck(seed, channel, division, nth, chance) {
                        continue;
                    }
                    let early = tick as i64 - at as i64;
                    // The strike on the row, or else the earliest.
                    found = Some(match found {
                        Some(0) => 0,
                        Some(other) if early != 0 => other.min(early),
                        _ => early,
                    });
                }
            }
        }
        found.map(|ticks| self.nudge(ticks))
    }

    /// Every strike over a span of `rows`, in order.
    pub fn strikes(&self, seed: u64, rows: usize) -> impl Iterator<Item = Strike> + '_ {
        (0..rows).filter_map(move |offset| {
            self.strike(seed, offset)
                .map(|nudge| Strike { offset, nudge })
        })
    }
}

/// The seed the `nth` of several tracks shaped at once takes: the same for each three, `A`, `B`
/// and `A+B`, so the third plays exactly the other two's strikes, and a new one for the next three.
pub fn track_seed(seed: u64, nth: usize) -> u64 {
    super::track_seed(seed, nth / Channel::ALL.len())
}

/// The strikes a division could make within a row's half either side of `at`, in a bar of
/// `bar` ticks, each with its place among the division's strikes in the bar and its tick: at most
/// two, as no division's strikes are closer than a row of sixteenths.
fn points(division: Division, at: u64, row: u64, bar: u64) -> impl Iterator<Item = (u64, u64)> {
    let every = division.ticks();
    let start = at.saturating_sub(row / 2);
    let end = (at + row - row / 2).min(bar);
    let first = start.div_ceil(every);
    (first..first + 2)
        .map(move |nth| (nth, nth * every))
        .filter(move |(_, tick)| *tick < end && *tick >= start)
}

/// Whether a strike's die clears its division's chance: a percent drawn from the seed, the
/// channel, the division and the strike's place in the bar.
fn struck(seed: u64, channel: Channel, division: Division, nth: u64, chance: u8) -> bool {
    let key = (division.index() << 16) | nth as usize;
    let die = mix(seed ^ DICE ^ ((channel.index() as u64 + 1) << 40), key) >> 32;
    die % 100 < u64::from(chance)
}

/// A `Divisions` fill's cells for `range`, one for each row, the first in `cells[0]`. A row that
/// strikes is a note at its nudge: the note there if there is one, or else the first in the rows,
/// or `from` where there's none, with the velocity there, or the first note's, or the one notes
/// are typed with on a column that had none. A row that doesn't strike is cleared, keeping its
/// nudge, so a render laid over an earlier one leaves nothing of it. An `OFF` stays. It writes
/// notes; on any other column it leaves the rows alone.
pub(super) fn render(
    steps: &[Step],
    range: StepRange,
    column: ShapeColumn,
    from: u8,
    seed: u64,
    divisions: Divisions,
    cells: &mut [GenCell],
) {
    let ShapeColumn::Note { writes, .. } = column else {
        return;
    };
    let rows = steps.iter().skip(range.first).take(cells.len());
    let first = rows.clone().find_map(|step| match step.note {
        Note::On(note) => Some((note, step)),
        _ => None,
    });
    let (pitch, velocity, gate) = match first {
        Some((note, step)) => (note, step.velocity, step.gate),
        None => (from, Some(writes.unwrap_or(DEFAULT_NOTE_VELOCITY)), None),
    };
    for (offset, (cell, step)) in cells.iter_mut().zip(rows).enumerate() {
        *cell = match (step.note, divisions.strike(seed, offset)) {
            (Note::Off, _) => GenCell::Under,
            (Note::On(note), Some(nudge)) => GenCell::Timed {
                note: Note::On(note),
                velocity: step.velocity,
                gate: step.gate,
                nudge,
            },
            (Note::Empty, Some(nudge)) => GenCell::Timed {
                note: Note::On(pitch),
                velocity,
                gate: first.map_or(step.gate, |_| gate),
                nudge,
            },
            (Note::On(_), None) => GenCell::Timed {
                note: Note::Empty,
                velocity: None,
                gate: None,
                nudge: step.nudge,
            },
            (Note::Empty, None) => GenCell::Timed {
                note: Note::Empty,
                velocity: step.velocity,
                gate: step.gate,
                nudge: step.nudge,
            },
        };
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::scale::Scale;
    use crate::shape::{
        Acid, Bits, DEFAULT_RATE, FillKind, Follow, Grids, Pink, Shape, Values, Walk, Where,
        written,
    };

    fn shape(divisions: Divisions, seed: u64) -> Shape {
        Shape {
            from: 36,
            to: 48,
            curve: 0,
            s: 0,
            kind: FillKind::Divisions,
            wave: crate::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::default(),
            place: Where::AllRows,
            seed,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: Grids::default(),
            random: crate::shape::Random::default(),
            vary: 0,
            variation: crate::shape::Variation::default(),
            divisions,
            ring: crate::shape::Ring::default(),
        }
    }

    fn writing() -> ShapeColumn {
        ShapeColumn::Note {
            scale: Scale::default(),
            writes: Some(0x50),
        }
    }

    /// Only these divisions, each at its chance, on channel `A`.
    fn only(bar: Bar, chances: &[(Division, u8)]) -> Divisions {
        let mut set = [0; DIVISIONS];
        for (division, chance) in chances {
            set[division.index()] = *chance;
        }
        Divisions {
            chances: [set, [0; DIVISIONS]],
            channel: Some(Channel::A),
            bar,
            ticks: None,
        }
    }

    /// The ticks each strike over `rows` lands on, its row's and its nudge's.
    fn landed(divisions: Divisions, seed: u64, rows: usize) -> BTreeSet<i64> {
        let row = divisions.bar.row_ticks() as i64;
        divisions
            .strikes(seed, rows)
            .map(|strike| strike.offset as i64 * row + i64::from(strike.nudge) * NUDGE_TICKS as i64)
            .collect()
    }

    /// The rows a set of ticks falls on, each the nearest.
    fn rows_of(ticks: &BTreeSet<i64>, bar: Bar) -> BTreeSet<i64> {
        let row = bar.row_ticks() as i64;
        ticks
            .iter()
            .map(|tick| (tick + row / 2).div_euclid(row))
            .collect()
    }

    /// Every tick a division strikes on in a bar.
    fn ticks_of(division: Division, bar: Bar) -> BTreeSet<i64> {
        (0..bar.ticks())
            .step_by(division.ticks() as usize)
            .map(|tick| tick as i64)
            .collect()
    }

    /// Where each strike over `rows` is, by row and nudge.
    fn placed(divisions: Divisions, seed: u64, rows: usize) -> Vec<(usize, i8)> {
        divisions
            .strikes(seed, rows)
            .map(|strike| (strike.offset, strike.nudge))
            .collect()
    }

    #[test]
    fn at_0_a_division_never_strikes_and_at_100_always_does() {
        for bar in Bar::ALL {
            for division in Division::ALL {
                for seed in 0..20 {
                    let off = only(bar, &[(division, 0)]);
                    assert_eq!(off.strikes(seed, 64).count(), 0);
                    let on = only(bar, &[(division, 100)]);
                    let wanted = ticks_of(division, bar);
                    let found = landed(on, seed, bar.rows());
                    if bar.fine() || division != Division::SixteenthTriplet {
                        assert_eq!(found, wanted, "{} {}", division.name(), bar.name());
                    } else {
                        // Two to an odd row at 1/16: the one on the row, or else the earlier, so
                        // every row strikes.
                        assert!(found.is_subset(&wanted));
                        assert_eq!(found.len(), bar.rows());
                    }
                }
            }
        }
    }

    #[test]
    fn each_division_lands_on_its_own_ticks_the_triplets_nudged_a_third_of_a_row() {
        // At 1/32: the sixteenths on the even rows, and the eighth triplets on the odd ones, nudged
        // 4 either way, 40 ticks, a third of a 32nd.
        let eighth_triplets = only(Bar::FourFine, &[(Division::EighthTriplet, 100)]);
        assert_eq!(placed(eighth_triplets, 1, 8), [(0, 0), (3, -4), (5, 4)]);
        let sixteenths = only(Bar::FourFine, &[(Division::Sixteenth, 100)]);
        assert!(
            sixteenths
                .strikes(1, 32)
                .all(|strike| strike.offset % 2 == 0 && strike.nudge == 0)
        );
        let fine_triplets = only(Bar::FourFine, &[(Division::SixteenthTriplet, 100)]);
        assert_eq!(
            placed(fine_triplets, 1, 8),
            [(0, 0), (1, 4), (3, -4), (4, 0), (5, 4), (7, -4)]
        );
        // At 1/16 the eighth and quarter triplets take the odd rows, nudged 8, a third of a
        // sixteenth.
        let coarse = only(Bar::Four, &[(Division::QuarterTriplet, 100)]);
        assert_eq!(placed(coarse, 1, 8), [(0, 0), (3, -8), (5, 8)]);
        assert_eq!(only(Bar::Four, &[]).earliest(), -8);
        assert_eq!(only(Bar::FourFine, &[]).earliest(), -4);
        // Every division at once has a row for each of its ticks at 1/32, so none is lost.
        let all = only(
            Bar::FourFine,
            &Division::ALL.map(|division| (division, 100)),
        );
        let wanted: BTreeSet<i64> = Division::ALL
            .into_iter()
            .flat_map(|division| ticks_of(division, Bar::FourFine))
            .collect();
        assert_eq!(landed(all, 7, 32), wanted);
        assert!(!all.crowded());
        // At 1/16, the sixteenths keep the odd rows they share with the triplets.
        let shared = only(
            Bar::Four,
            &[(Division::Sixteenth, 100), (Division::EighthTriplet, 100)],
        );
        assert!(shared.crowded());
        assert!(shared.strikes(1, 16).all(|strike| strike.nudge == 0));
        assert!(!only(Bar::Four, &[(Division::EighthTriplet, 50)]).crowded());
    }

    #[test]
    fn raising_a_chance_only_adds_strikes() {
        for bar in Bar::ALL {
            for division in Division::ALL {
                for seed in 0..8 {
                    let mut divisions = Divisions {
                        bar,
                        ..Divisions::default()
                    };
                    divisions.chances[0][division.index()] = 0;
                    let mut before = landed(divisions, seed, 64);
                    for chance in 0..=100 {
                        divisions.chances[0][division.index()] = chance;
                        let now = landed(divisions, seed, 64);
                        let name = division.name();
                        assert!(
                            rows_of(&before, bar).is_subset(&rows_of(&now, bar)),
                            "{name} at {chance}"
                        );
                        // On rows of their own, every strike stays where it was.
                        if bar.fine() {
                            assert!(before.is_subset(&now), "{name} at {chance}");
                        }
                        before = now;
                    }
                }
            }
        }
    }

    #[test]
    fn a_seed_repeats_each_bar_and_another_rolls_other_dice() {
        let divisions = Divisions {
            chances: [[50; DIVISIONS], [50; DIVISIONS]],
            bar: Bar::FourFine,
            ..Divisions::default()
        };
        let steps = vec![Step::default(); 64];
        let range = StepRange::between(0, 63);
        let once = written(&steps, range, writing(), &shape(divisions, 3));
        assert_eq!(
            once,
            written(&steps, range, writing(), &shape(divisions, 3))
        );
        assert_ne!(
            once,
            written(&steps, range, writing(), &shape(divisions, 4))
        );
        // Dice: each bar the same.
        let bars: Vec<Vec<(usize, i8)>> = (0..2)
            .map(|bar| {
                placed(divisions, 3, 64)
                    .into_iter()
                    .filter(|(row, _)| row / 32 == bar)
                    .map(|(row, nudge)| (row % 32, nudge))
                    .collect()
            })
            .collect();
        assert!(!bars[0].is_empty());
        assert_eq!(bars[0], bars[1]);
        // A bar of 3/4 goes round in 24 rows.
        let three = Divisions {
            bar: Bar::ThreeFine,
            ..divisions
        };
        let second: Vec<(usize, i8)> = placed(three, 3, 48)
            .into_iter()
            .filter(|(row, _)| *row >= 24)
            .map(|(row, nudge)| (row - 24, nudge))
            .collect();
        assert_eq!(placed(three, 3, 24), second);
    }

    #[test]
    fn a_and_b_roll_dice_of_their_own_and_a_plus_b_plays_both() {
        let chances = [[30, 40, 20, 10, 30, 10], [10, 30, 40, 20, 20, 30]];
        let panel = Divisions {
            chances,
            channel: None,
            bar: Bar::FourFine,
            ticks: None,
        };
        let (a, b, both) = (panel.for_track(0), panel.for_track(1), panel.for_track(2));
        assert_eq!(
            [a.channel, b.channel, both.channel],
            [Some(Channel::A), Some(Channel::B), Some(Channel::Both)]
        );
        // B on a track of its own plays B's chances as its own.
        assert_eq!(b.chances[0], chances[1]);
        assert_eq!(both.chances, chances);
        for seed in 0..20 {
            let (a, b, both) = (
                landed(a, seed, 64),
                landed(b, seed, 64),
                landed(both, seed, 64),
            );
            assert_ne!(a, b, "dice of their own");
            let union: BTreeSet<i64> = a.union(&b).copied().collect();
            assert_eq!(both, union);
        }
        // The same chances on B roll other dice than on A.
        let same = Divisions {
            chances: [chances[0], chances[0]],
            ..panel
        };
        assert_ne!(
            landed(same.for_track(0), 1, 64),
            landed(same.for_track(1), 1, 64)
        );
        assert_eq!(panel.for_track(3).channel, Some(Channel::A));
        assert_eq!(track_seed(9, 2), track_seed(9, 0));
        assert_ne!(track_seed(9, 3), track_seed(9, 0));
    }

    /// Its dice are drawn from its seed, so `Vary` rolls new ones each pass, the module's
    /// `Realtime` (decision 293), the first pass being the one written.
    #[test]
    fn vary_rolls_new_dice_each_pass() {
        let steps = vec![Step::default(); 32];
        let varying = Shape {
            vary: 1,
            ..shape(Divisions::default(), 1)
        };
        assert!(varying.draws() && varying.varies());
        let range = StepRange::between(0, 31);
        let pass = |pass| written(&steps, range, writing(), &varying.on_pass(pass));
        assert_eq!(
            pass(0),
            written(&steps, range, writing(), &shape(Divisions::default(), 1))
        );
        assert_ne!(pass(1), pass(0));
        assert_eq!(pass(1), pass(1));
    }

    #[test]
    fn on_a_slower_track_a_triplets_nudge_is_a_third_of_its_row() {
        let triplets = |ticks: u64| {
            let divisions = only(Bar::Four, &[(Division::QuarterTriplet, 100)]).for_step(ticks);
            (placed(divisions, 1, 8), divisions.earliest())
        };
        assert_eq!(triplets(240), (vec![(0, 0), (3, -8), (5, 8)], -8));
        // At 1/8 a row is 480 ticks, and a third of it 16 nudges; at 1/2, 64.
        assert_eq!(triplets(480), (vec![(0, 0), (3, -16), (5, 16)], -16));
        assert_eq!(triplets(1920), (vec![(0, 0), (3, -64), (5, 64)], -64));
        // At 1/1 a third of the row, 128, is past the most a nudge goes, so it stops at 7F.
        assert_eq!(triplets(3840), (vec![(0, 0), (3, -127), (5, 127)], -127));
        // Only a track at 1/16 or 1/32, whose rows are the bar's, keeps nothing more.
        assert_eq!(Divisions::default().for_step(240).ticks, None);
        assert_eq!(Divisions::default().for_step(120).bar, Bar::FourFine);
        assert_eq!(Divisions::default().for_step(120).ticks, None);
        assert_eq!(Divisions::default().for_step(1920).ticks, Some(1920));
        // Another bar keeps its rows as long as the track's.
        let fine = Divisions::default().for_step(240).with_bar(Bar::FourFine);
        assert_eq!((fine.played(), fine.ticks), (240, Some(240)));
        assert_eq!(fine.with_bar(Bar::Four).ticks, None);
    }

    #[test]
    fn a_strike_is_a_note_at_its_nudge_keeping_the_note_there() {
        let mut steps = vec![Step::default(); 8];
        steps[0].note = Note::On(40);
        steps[0].velocity = Some(0x30);
        steps[2].note = Note::On(41);
        steps[2].nudge = 3;
        steps[4].note = Note::Off;
        steps[6].nudge = -2;
        let column = ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        };
        let divisions = only(Bar::FourFine, &[(Division::EighthTriplet, 100)]);
        let range = StepRange::between(0, 7);
        let mut out = steps.clone();
        for (row, step) in written(&steps, range, column, &shape(divisions, 1)) {
            out[row] = step;
        }
        // Row 0's note stays, and rows 3 and 5 take its pitch and velocity, nudged.
        assert_eq!(out[0].note, Note::On(40));
        assert_eq!(
            (out[3].note, out[3].velocity, out[3].nudge),
            (Note::On(40), Some(0x30), -4)
        );
        assert_eq!((out[5].note, out[5].nudge), (Note::On(40), 4));
        // Row 2's note goes and its nudge stays, as on a row it doesn't strike; the OFF stays.
        assert_eq!((out[2].note, out[2].nudge), (Note::Empty, 3));
        assert_eq!(out[4].note, Note::Off);
        assert_eq!(out[6].nudge, -2);
        // On an empty column, `From` at the velocity notes are typed with.
        let empty = vec![Step::default(); 8];
        let out = written(&empty, range, writing(), &shape(divisions, 1));
        let (_, first) = out.iter().find(|(row, _)| *row == 0).unwrap();
        assert_eq!((first.note, first.velocity), (Note::On(36), Some(0x50)));
        // Other columns are left alone.
        assert!(written(&empty, range, ShapeColumn::Velocity, &shape(divisions, 1)).is_empty());
    }
}
