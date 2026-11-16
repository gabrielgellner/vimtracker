//! Controlled chance, after Winter Modular's Eloquencer: an `RND xy` gives what's to its left on
//! its row a chance of `x` in `F` of varying, by up to `y` either way, each time the row plays. In
//! the first FX column, or beside anything but a lock or a `RET`, that's the row's note, by scale
//! degrees with the scale on and semitones with it off; beside a lock it's the lock's value, so
//! `VDLT` varies the velocity and `LEN` the length; and beside a `RET` it's whether the row
//! ratchets at all, and its units.
//!
//! Each roll is a hash of the project's seed, the track, the track's count since its chain started
//! and the column, rather than a draw from the track's generator, so `CHA` rolls as it did, a
//! render repeats, and playing from a step hears what playing up to it would.

use crate::random::splitmix64;
use crate::settings::{Kind, SettingId, Units};
use crate::step::{FxCommand, FxSlot};

/// The highest chance, `F`: always.
pub const ALWAYS: u8 = 0x0F;

/// How far one step of an `RND`'s range moves a lock on the `00` to `FF` scale.
pub const LOCK_STEP: u16 = 0x10;

/// An `RND`'s value: the chance `x` in `F` that it varies, and how far `y` either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variation {
    pub chance: u8,
    pub range: u8,
}

impl Variation {
    /// An `RND`'s value, its two digits.
    pub fn of(value: u8) -> Self {
        Self {
            chance: value >> 4,
            range: value & 0x0F,
        }
    }

    /// Its chance as a percentage, to the nearest whole one.
    pub fn percent(self) -> u32 {
        (u32::from(self.chance) * 100 + u32::from(ALWAYS) / 2) / u32::from(ALWAYS)
    }
}

/// What an `RND` varies, by what's to its left on the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Varies {
    /// The row's note.
    Note,
    /// The value of the lock to its left.
    Lock(SettingId),
    /// Whether the `RET` to its left ratchets, and its units.
    Ratchet,
    /// Nothing: a `GRV` or `RATE`, which the engine reads ahead of its row, or an `SLI` by a
    /// letter.
    Nothing,
}

impl Varies {
    /// What an `RND` varies, from the slot in the column to its left, or none in the first column.
    pub fn of(left: Option<FxSlot>) -> Self {
        let Some(left) = left else {
            return Self::Note;
        };
        match left.command {
            Some(FxCommand::Ret) => Self::Ratchet,
            Some(FxCommand::Set(SettingId::GROOVE | SettingId::SPEED)) => Self::Nothing,
            Some(FxCommand::Set(_)) if left.slice_mark().is_some() => Self::Nothing,
            Some(FxCommand::Set(id)) => Self::Lock(id),
            _ => Self::Note,
        }
    }
}

/// What an `RND` in a column varies, among a row's FX.
pub fn varies_at(fx: &[FxSlot], column: usize) -> Varies {
    Varies::of(column.checked_sub(1).and_then(|left| fx.get(left).copied()))
}

/// One `RND`'s roll, each time its row plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Roll(u64);

/// Keeps an `RND`'s hashes apart from any other the seed is hashed for.
const ROLL_STREAM: u64 = 0x726e_6400_0000_0000;

impl Roll {
    /// The roll for a track's `RND` in a column, at the track's count since its chain started.
    pub fn new(seed: u64, track: usize, count: i64, column: usize) -> Self {
        let track =
            splitmix64(splitmix64(seed) ^ splitmix64(ROLL_STREAM.wrapping_add(track as u64)));
        Self(splitmix64(splitmix64(track ^ count as u64) ^ column as u64))
    }

    /// Whether it varies, a chance of `x` in `F`: never at `0` and always at `F`.
    pub fn hits(self, variation: Variation) -> bool {
        below(self.0, u64::from(ALWAYS)) < u64::from(variation.chance)
    }

    /// How far it moves something, from 1 to `most` either way, each as likely, so it always
    /// moves when it can; nothing with `most` at 0.
    pub fn moved_by(self, most: u16) -> i16 {
        if most == 0 {
            return 0;
        }
        let size = below(splitmix64(self.0), 2 * u64::from(most)) as i16;
        if size < most as i16 {
            -(size + 1)
        } else {
            size - most as i16 + 1
        }
    }

    /// Somewhere from `most` below to `most` above, each as likely, nothing among them.
    pub fn within(self, most: u16) -> i16 {
        below(splitmix64(self.0), 2 * u64::from(most) + 1) as i16 - most as i16
    }
}

/// How far one step of an `RND`'s range moves a lock of this setting: a choice on a list, a
/// semitone on a setting in semitones, and `10` on the rest.
pub fn lock_step(id: SettingId) -> u16 {
    match id.entry().kind {
        Kind::Stepped { .. }
        | Kind::Continuous {
            units: Units::Semitones,
            ..
        } => 1,
        Kind::Continuous { .. } => LOCK_STEP,
    }
}

/// A lock's value as a roll varies it, up to `range` of its steps either way, kept within its
/// range, with its fine part.
pub fn varied_lock(id: SettingId, value: u16, range: u8, roll: Roll) -> u16 {
    let by = roll.moved_by(u16::from(range) * lock_step(id));
    let top = i32::from(id.entry().kind.top());
    let coarse = (i32::from(value >> 8) + i32::from(by)).clamp(0, top);
    let fine = if coarse == top { 0 } else { value & 0xFF };
    (coarse as u16) << 8 | fine
}

/// A `RET`'s value as a roll varies it: its units up to `range` either way, from 1 to `F`, and its
/// fade as written. A `RET` with no units does nothing, and stays so.
pub fn varied_ratchet(value: u8, range: u8, roll: Roll) -> u8 {
    let units = i16::from(value & 0x0F);
    if units == 0 {
        return value;
    }
    let units = (units + roll.within(u16::from(range))).clamp(1, 0x0F) as u8;
    value & 0xF0 | units
}

/// What an `RND` does, for the status line: what it varies, how likely and how far.
pub fn text(value: u8, varies: Varies, scale_on: bool) -> String {
    let variation = Variation::of(value);
    let percent = variation.percent();
    let range = variation.range;
    match varies {
        Varies::Nothing => "nothing: it can't vary the value to its left".to_owned(),
        Varies::Ratchet => {
            let units = match range {
                0 => String::new(),
                1 => ", its units 1 either way".to_owned(),
                _ => format!(", its units up to {range} either way"),
            };
            format!("the RET ratchets {percent}% of the times it plays{units}")
        }
        _ if variation.chance == 0 => "never varies, as its chance is 0".to_owned(),
        _ if range == 0 => "doesn't vary, as its range is 0".to_owned(),
        Varies::Note => {
            let unit = match (scale_on, range) {
                (true, 1) => "scale degree",
                (true, _) => "scale degrees",
                (false, 1) => "semitone",
                (false, _) => "semitones",
            };
            format!(
                "{percent}% of the times it plays, moves the note up to {range} {unit} either way"
            )
        }
        Varies::Lock(id) => {
            let step = lock_step(id);
            let most = u16::from(range) * step;
            let how = if step == 1 {
                format!("{most}")
            } else {
                format!("{most:02X}")
            };
            let name = FxCommand::Set(id).name();
            format!("{percent}% of the times it plays, moves {name} up to {how} either way")
        }
    }
}

/// A number below `below` from a hash, without the bias a remainder has.
fn below(hash: u64, below: u64) -> u64 {
    ((hash >> 32) * below) >> 32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::step::Mark;

    fn rolls(seed: u64) -> impl Iterator<Item = Roll> {
        (0..4000).map(move |count| Roll::new(seed, 2, count, 1))
    }

    fn lock(id: SettingId, coarse: u8) -> FxSlot {
        FxSlot {
            command: Some(FxCommand::Set(id)),
            value: u16::from(coarse) << 8,
            mark: Mark::Lock,
        }
    }

    #[test]
    fn a_seed_repeats_and_each_track_count_and_column_rolls_apart() {
        let first: Vec<Roll> = rolls(7).collect();
        assert_eq!(first, rolls(7).collect::<Vec<_>>());
        assert_ne!(first, rolls(8).collect::<Vec<_>>());
        let at = Roll::new(7, 2, 5, 1);
        assert_ne!(at, Roll::new(7, 3, 5, 1));
        assert_ne!(at, Roll::new(7, 2, 6, 1));
        assert_ne!(at, Roll::new(7, 2, 5, 2));
    }

    #[test]
    fn chance_0_never_varies_f_always_does_and_8_about_half() {
        let hits = |chance| {
            rolls(3)
                .filter(|roll| roll.hits(Variation { chance, range: 1 }))
                .count()
        };
        assert_eq!(hits(0), 0);
        assert_eq!(hits(ALWAYS), 4000);
        let half = hits(8) as f32 / 4000.0;
        assert!((half - 8.0 / 15.0).abs() < 0.03, "{half}");
    }

    #[test]
    fn a_move_is_1_to_its_range_either_way_and_reaches_each() {
        let mut seen = [0; 9];
        for roll in rolls(11) {
            let by = roll.moved_by(4);
            assert!(by != 0 && by.abs() <= 4, "{by}");
            seen[(by + 4) as usize] += 1;
        }
        assert_eq!(seen[4], 0);
        assert!(seen.iter().enumerate().all(|(at, n)| at == 4 || *n > 300));
        assert!(rolls(1).all(|roll| roll.moved_by(0) == 0));
        assert!(rolls(1).all(|roll| roll.within(2).abs() <= 2));
    }

    #[test]
    fn what_it_varies_is_whats_to_its_left() {
        let rnd = FxSlot {
            command: Some(FxCommand::Rnd),
            value: 0x8400,
            mark: Mark::Lock,
        };
        let ret = FxSlot {
            command: Some(FxCommand::Ret),
            value: 0x0300,
            mark: Mark::Lock,
        };
        assert_eq!(varies_at(&[rnd], 0), Varies::Note);
        assert_eq!(varies_at(&[FxSlot::default(), rnd], 1), Varies::Note);
        assert_eq!(varies_at(&[rnd, rnd], 1), Varies::Note);
        assert_eq!(
            varies_at(&[lock(SettingId::CUT, 0x40), rnd], 1),
            Varies::Lock(SettingId::CUT)
        );
        assert_eq!(varies_at(&[ret, rnd], 1), Varies::Ratchet);
        assert_eq!(
            varies_at(&[lock(SettingId::SPEED, 2), rnd], 1),
            Varies::Nothing
        );
        let by_letter = FxSlot::slice_by_mark('a', Mark::Lock);
        assert_eq!(varies_at(&[by_letter, rnd], 1), Varies::Nothing);
    }

    #[test]
    fn a_lock_moves_by_tens_a_list_by_choices_and_stays_in_range() {
        for roll in rolls(5) {
            let cut = varied_lock(SettingId::CUT, 0x4080, 3, roll);
            let by = i32::from(cut >> 8) - 0x40;
            assert!(by != 0 && by.abs() <= 0x30, "{by}");
            assert_eq!(cut & 0xFF, 0x80, "the fine part stays");
            let high = varied_lock(SettingId::CUT, 0xF800, 0xF, roll);
            assert!(high <= 0xFF00);
            let play = varied_lock(SettingId::PLAY, 0, 2, roll) >> 8;
            assert!(play <= u16::from(SettingId::PLAY.entry().kind.top()));
        }
    }

    #[test]
    fn a_ratchet_varies_its_units_and_keeps_its_fade() {
        for roll in rolls(9) {
            let varied = varied_ratchet(0x83, 2, roll);
            assert_eq!(varied & 0xF0, 0x80);
            assert!((1..=5).contains(&(varied & 0x0F)), "{varied:02X}");
            assert_eq!(varied_ratchet(0x80, 2, roll), 0x80, "no units stays none");
            assert_eq!(varied_ratchet(0x83, 0, roll), 0x83);
        }
    }

    #[test]
    fn the_status_line_says_what_it_varies() {
        assert_eq!(
            text(0x84, Varies::Note, true),
            "53% of the times it plays, moves the note up to 4 scale degrees either way"
        );
        assert_eq!(
            text(0xF1, Varies::Note, false),
            "100% of the times it plays, moves the note up to 1 semitone either way"
        );
        assert_eq!(
            text(0x83, Varies::Lock(SettingId::CUT), true),
            "53% of the times it plays, moves CUT up to 30 either way"
        );
        assert_eq!(
            text(0x80, Varies::Ratchet, true),
            "the RET ratchets 53% of the times it plays"
        );
        assert_eq!(
            text(0x04, Varies::Note, true),
            "never varies, as its chance is 0"
        );
    }
}
