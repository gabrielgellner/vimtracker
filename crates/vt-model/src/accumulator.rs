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

/// What an accumulator does at one of its limits, after the Metropolix's LIM Order and Polarity:
/// unipolar between the note as written and the limit an amount points to, or bipolar, `Bi`,
/// between both limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtLimit {
    /// Past it, it carries on from the note as written.
    #[default]
    Wrap,
    /// Past one limit, it carries on from the other.
    WrapBi,
    /// At it, it turns back, and turns again at the note as written.
    Bounce,
    /// At one limit, it turns back, and turns again at the other.
    BounceBi,
    /// It stays at it until it starts again.
    Hold,
    /// A drunken walk between the note as written and the limit an amount points to: each add
    /// moves the amount's way half the time, the other way 30% and not at all 20%, by 1 to the
    /// amount, carrying on from the other end past either.
    Random,
    /// The same walk between both limits.
    RandomBi,
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
    /// that brings the chain in or resets, a launch, `:reset` and the track's own `Reset`.
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
    /// `Up` at 0 or `Down` at 1, as the registry has them ([`SettingId::accumulation`]): a place in
    /// a list from 1, with a coarse byte and a fine one.
    ///
    /// [`SettingId::accumulation`]: crate::settings::SettingId::accumulation
    pub fn own_value(self, at: usize) -> u16 {
        let limit = if at == 0 { self.up } else { self.down };
        u16::from(limit.saturating_sub(1)) << 8
    }

    /// Set `Up` or `Down` from a value on the `00` to `FF` scale, kept from 1 to [`MOST_DEGREES`].
    pub fn set_own(&mut self, at: usize, value: f32) {
        let limit = value.round().clamp(0.0, f32::from(MOST_DEGREES - 1)) as u8 + 1;
        if at == 0 {
            self.up = limit;
        } else {
            self.down = limit;
        }
    }

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
    /// written, the Metropolix's unipolar orders, so a rising line never goes below it, and their
    /// `Bi` forms go on to the other limit. `Random` and `RandomBi` take their rolls from `roll`,
    /// which the others leave alone.
    pub fn add(&mut self, amount: i8, settings: Accumulation, roll: &mut impl FnMut() -> u32) {
        let (up, down) = (i16::from(settings.up), i16::from(settings.down));
        // Where a unipolar walk goes: a positive amount between the note and the upper limit, and
        // a negative one between the lower limit and the note.
        let one_way = if amount >= 0 { (0, up) } else { (-down, 0) };
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
            AtLimit::WrapBi => {
                self.offset = around(self.offset + i16::from(amount), -down, up);
            }
            AtLimit::Bounce => self.bounce(amount, one_way, (down, up)),
            AtLimit::BounceBi => self.bounce(amount, (-down, up), (down, up)),
            AtLimit::Random | AtLimit::RandomBi => {
                let (low, high) = if settings.at_limit == AtLimit::Random {
                    one_way
                } else {
                    (-down, up)
                };
                // The Metropolix's odds: the amount's way 5 in 10, the other way 3 and none 2.
                let way = match roll() % 10 {
                    0..5 => 1,
                    5..8 => -1,
                    _ => return,
                };
                let size = 1 + (roll() % u32::from(amount.unsigned_abs().max(1))) as i16;
                let step = way * size * if amount < 0 { -1 } else { 1 };
                self.offset = around(self.offset + step, low, high);
            }
        }
    }

    /// Move by an amount between `low` and `high`, turning back each time it passes one, and kept
    /// within the limits, `down` below the note and `up` above.
    fn bounce(&mut self, amount: i8, (low, high): (i16, i16), (down, up): (i16, i16)) {
        let step = if self.turned {
            -i16::from(amount)
        } else {
            i16::from(amount)
        };
        let mut moved = self.offset + step;
        let mut rising = step > 0;
        // Each turn takes some of the way back, so a large amount turns more than once.
        for _ in 0..=2 * MOST_DEGREES {
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

/// An offset kept between `low` and `high` by going round: one past either carries on from the
/// other.
fn around(offset: i16, low: i16, high: i16) -> i16 {
    low + (offset - low).rem_euclid(high - low + 1)
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
    /// Every choice, each unipolar one beside its `Bi` form.
    pub const ALL: [Self; 7] = [
        Self::Wrap,
        Self::WrapBi,
        Self::Bounce,
        Self::BounceBi,
        Self::Hold,
        Self::Random,
        Self::RandomBi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Wrap => "Wrap",
            Self::WrapBi => "Wrap bi",
            Self::Bounce => "Bounce",
            Self::BounceBi => "Bounce bi",
            Self::Hold => "Hold",
            Self::Random => "Random",
            Self::RandomBi => "Random bi",
        }
    }

    pub fn meaning(self) -> &'static str {
        match self {
            Self::Wrap => "Past the limit, carry on from the note as written",
            Self::WrapBi => "Past one limit, carry on from the other",
            Self::Bounce => "Turn back at the limit, and again at the note as written",
            Self::BounceBi => "Turn back at one limit, and again at the other",
            Self::Hold => "Stay at the limit until it starts again",
            Self::Random => "Step at random, mostly the ACU's way, between the note and the limit",
            Self::RandomBi => "Step at random, mostly the ACU's way, between both limits",
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
        rolled(amount, settings, adds, &mut || {
            unreachable!("only Random rolls")
        })
    }

    /// The same, with rolls for `Random`.
    fn rolled(
        amount: i8,
        settings: Accumulation,
        adds: usize,
        roll: &mut impl FnMut() -> u32,
    ) -> Vec<i16> {
        let mut walk = Walk::default();
        let mut offsets = vec![walk.offset];
        for _ in 0..adds {
            walk.add(amount, settings, roll);
            offsets.push(walk.offset);
        }
        offsets
    }

    /// Rolls from a small generator, xorshift, for a long random walk.
    fn dice(seed: u32) -> impl FnMut() -> u32 {
        let mut state = seed.max(1);
        move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        }
    }

    #[test]
    fn the_bipolar_forms_go_between_both_limits_as_the_metropolix_manual_draws_them() {
        // Its figures have the limits at +9 and -5.
        let wrap = limits(9, 5, AtLimit::WrapBi);
        let mut climb: Vec<i16> = (0..=9).collect();
        climb.extend(-5..=9);
        climb.extend(-5..=0);
        assert_eq!(walk(1, wrap, 30), climb);
        assert_eq!(walk(4, wrap, 8), [0, 4, 8, -3, 1, 5, 9, -2, 2]);
        assert_eq!(walk(-1, wrap, 7), [0, -1, -2, -3, -4, -5, 9, 8]);
        let bounce = limits(9, 5, AtLimit::BounceBi);
        let mut swing: Vec<i16> = (0..=9).collect();
        swing.extend((-5..=8).rev());
        swing.extend(-4..=0);
        assert_eq!(walk(1, bounce, 28), swing);
        assert_eq!(walk(4, bounce, 9), [0, 4, 8, 6, 2, -2, -4, 0, 4, 8]);
    }

    #[test]
    fn random_walks_drunkenly_within_its_range() {
        // Unipolar, between the note and the limit the amount points to.
        let random = limits(9, 5, AtLimit::Random);
        for amount in [1, 3, -2] {
            let offsets = rolled(amount, random, 2000, &mut dice(7));
            let (low, high) = if amount > 0 { (0, 9) } else { (-5, 0) };
            assert!(offsets.iter().all(|offset| (low..=high).contains(offset)));
            let distinct: std::collections::BTreeSet<_> = offsets.iter().collect();
            assert_eq!(
                distinct.len(),
                10.min(1 + (high - low) as usize),
                "{amount}"
            );
        }
        // Bipolar, between both.
        let both = rolled(3, limits(9, 5, AtLimit::RandomBi), 2000, &mut dice(11));
        assert!(both.iter().all(|offset| (-5..=9).contains(offset)));
        assert!(both.contains(&-5) && both.contains(&9));
        // Each add moves by at most the amount, and over many, half go its way, 30% the other way
        // and 20% nowhere, as the Metropolix's walk.
        let steps = rolled(1, limits(28, 28, AtLimit::RandomBi), 4000, &mut dice(3));
        let (mut with, mut against, mut still) = (0.0, 0.0, 0.0);
        for pair in steps.windows(2) {
            match pair[1] - pair[0] {
                1 | -56 => with += 1.0,
                -1 | 56 => against += 1.0,
                0 => still += 1.0,
                other => panic!("a step of {other}"),
            }
        }
        assert!((with / 4000.0 - 0.5_f64).abs() < 0.03, "{with}");
        assert!((against / 4000.0 - 0.3_f64).abs() < 0.03, "{against}");
        assert!((still / 4000.0 - 0.2_f64).abs() < 0.03, "{still}");
        // Same rolls, same walk.
        assert_eq!(
            rolled(3, random, 50, &mut dice(5)),
            rolled(3, random, 50, &mut dice(5))
        );
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
        // C-3 is 60: up two degrees of C major is E, and down one is B below.
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
