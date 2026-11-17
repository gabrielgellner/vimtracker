//! A track's dice loop, after Marbles' Déjà vu and Length (decisions 410 and 411): with `Dice loop`
//! on, the dice a track rolls at each step, its `CHA`s and `RND`s, are the dice of a place in a
//! loop of that many steps rather than fresh ones. Each place keeps the step whose dice it holds,
//! and as the loop comes round, `Dice mutate` is the chance the place takes the step now instead,
//! from `00`, a locked loop, to `FF`, fresh dice every step, as with no loop; `Dice jump` is the
//! chance a step reads another place of the loop than the one it's at, so a locked loop plays its
//! dice out of order. The rolls that decide both are hashes of the seed, the track and the step, so
//! a render repeats.
//!
//! The loop is a ring of fixed size, which the audio thread keeps for each track and fills as it
//! plays, so it allocates nothing.

use crate::random::splitmix64;

/// The most steps a loop holds, its longest length.
pub const DICE_LOOP_MOST: usize = 64;

/// The lengths `Dice loop` chooses from, by its value: off, then 1 to 16 steps, as Marbles'
/// `Length` goes, then two and four bars of sixteenths.
pub const DICE_LOOP_LENGTHS: [u8; 19] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 32, 64,
];

/// `Dice loop`'s choices as they're shown, by its value.
pub const DICE_LOOP_NAMES: [&str; DICE_LOOP_LENGTHS.len()] = [
    "Off", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "32", "64",
];

/// The highest `Dice loop` value, the longest loop.
pub const MOST_DICE_LOOP: u8 = DICE_LOOP_LENGTHS.len() as u8 - 1;

/// How many steps a `Dice loop` value loops over, `0` for off.
pub fn dice_loop_length(value: u8) -> usize {
    usize::from(DICE_LOOP_LENGTHS[usize::from(value.min(MOST_DICE_LOOP))])
}

/// Keeps the dice loop's hashes apart from any other the seed is hashed for.
const LOOP_STREAM: u64 = 0x646c_7000_0000_0000;
/// Keeps a looped `CHA`'s hashes apart from the loop's own.
const CHANCE_STREAM: u64 = 0x6368_6100_0000_0000;

/// Which step's dice a step rolls: where the track was in its own count of steps, `running`, and
/// in its count since its chain started, `count`, which an `RND` hashes, so with fresh dice an
/// `RND` rolls as it does with no loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiceStep {
    pub running: i64,
    pub count: i64,
}

/// Whether a looped `CHA` in this column comes up, a chance of `value` out of 255, rolled from the
/// step whose dice it is: `00` never does and `FF` always does, as with no loop.
pub fn looped_chance(seed: u64, track: usize, step: DiceStep, column: usize, value: u8) -> bool {
    let hash = track_hash(seed, CHANCE_STREAM, track) ^ splitmix64(step.running as u64);
    let roll = splitmix64(splitmix64(hash) ^ column as u64);
    below(roll, 255) < u64::from(value)
}

/// The settings a loop is played with at a step, after locks and modulation, each on its own
/// scale: `Dice loop`'s value, and `Dice mutate`'s and `Dice jump`'s chances out of 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiceSettings {
    pub length: u8,
    pub mutate: u8,
    pub jump: u8,
}

/// A track's dice loop as it plays: the step each place holds the dice of, if it's been filled
/// since playing started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiceLoop {
    kept: [DiceStep; DICE_LOOP_MOST],
    /// Which places hold a step, a bit each.
    filled: u64,
}

impl Default for DiceLoop {
    fn default() -> Self {
        Self {
            kept: [DiceStep::default(); DICE_LOOP_MOST],
            filled: 0,
        }
    }
}

impl DiceLoop {
    /// Empty it, as playing starts, so the loop fills again from the steps that play.
    pub fn clear(&mut self) {
        self.filled = 0;
    }

    /// The step whose dice the step `now` rolls, with the loop at `settings`, on track `track` of a
    /// song seeded `seed`; `None` with the loop off, when the track rolls as it always has. The
    /// place is the step's in the track's own count, so a loop of 16 keeps to the bar. With `Dice
    /// jump`'s chance it reads any place of the loop instead; then, with `Dice mutate`'s chance,
    /// or if it's empty, the place takes the step now. A place past the loop's length keeps what
    /// it holds, so a loop made shorter and then longer again finds it.
    pub fn roll(
        &mut self,
        settings: DiceSettings,
        seed: u64,
        track: usize,
        now: DiceStep,
    ) -> Option<DiceStep> {
        let length = dice_loop_length(settings.length);
        if length == 0 {
            return None;
        }
        let hash = track_hash(seed, LOOP_STREAM, track) ^ splitmix64(now.running as u64);
        let jumps = splitmix64(hash);
        let mutates = splitmix64(jumps);
        let walked = now.running.rem_euclid(length as i64) as usize;
        let place = if settings.jump > 0 && below(jumps, 255) < u64::from(settings.jump) {
            below(splitmix64(mutates), length as u64) as usize
        } else {
            walked
        };
        let empty = self.filled & (1u64 << place) == 0;
        if empty || below(mutates, 255) < u64::from(settings.mutate) {
            self.kept[place] = now;
            self.filled |= 1u64 << place;
        }
        Some(self.kept[place])
    }
}

/// The hash of a seed for one of a track's streams.
fn track_hash(seed: u64, stream: u64, track: usize) -> u64 {
    splitmix64(splitmix64(seed) ^ splitmix64(stream.wrapping_add(track as u64)))
}

/// A hash brought to `0..below`, by its top bits.
fn below(hash: u64, below: u64) -> u64 {
    ((hash >> 32) * below) >> 32
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCKED: DiceSettings = DiceSettings {
        length: 4,
        mutate: 0,
        jump: 0,
    };

    fn step(running: i64) -> DiceStep {
        DiceStep {
            running,
            count: running,
        }
    }

    /// The steps whose dice each of this many steps rolls, from the first.
    fn rolled(settings: DiceSettings, steps: i64) -> Vec<i64> {
        let mut dice = DiceLoop::default();
        (0..steps)
            .map(|at| dice.roll(settings, 7, 0, step(at)).unwrap().running)
            .collect()
    }

    #[test]
    fn off_rolls_nothing_and_every_length_fits_the_ring() {
        let mut dice = DiceLoop::default();
        let off = DiceSettings::default();
        assert_eq!(dice.roll(off, 7, 0, step(3)), None);
        assert_eq!(dice, DiceLoop::default());
        assert_eq!(dice_loop_length(MOST_DICE_LOOP), DICE_LOOP_MOST);
        assert_eq!(
            dice_loop_length(0xFF),
            DICE_LOOP_MOST,
            "kept to the longest"
        );
        assert!(DICE_LOOP_LENGTHS.iter().all(|at| usize::from(*at) <= 64));
    }

    #[test]
    fn locked_it_repeats_the_first_steps_dice() {
        assert_eq!(rolled(LOCKED, 12), [0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3]);
    }

    #[test]
    fn at_full_mutate_every_step_rolls_its_own() {
        let fresh = DiceSettings {
            mutate: 0xFF,
            ..LOCKED
        };
        assert_eq!(rolled(fresh, 12), (0..12).collect::<Vec<_>>());
    }

    #[test]
    fn halfway_it_drifts() {
        let half = DiceSettings {
            mutate: 0x80,
            ..LOCKED
        };
        let heard = rolled(half, 64);
        let kept = heard
            .iter()
            .enumerate()
            .skip(4)
            .filter(|(at, from)| **from != *at as i64)
            .count();
        assert!((10..50).contains(&kept), "{heard:?}");
    }

    #[test]
    fn jumping_plays_the_locked_dice_out_of_order() {
        let jumping = DiceSettings {
            jump: 0xFF,
            ..LOCKED
        };
        let mut dice = DiceLoop::default();
        // Fill the loop first, without jumping, then jump around it.
        for at in 0..4 {
            dice.roll(LOCKED, 7, 0, step(at));
        }
        let heard: Vec<i64> = (4..40)
            .map(|at| dice.roll(jumping, 7, 0, step(at)).unwrap().running)
            .collect();
        assert!(heard.iter().all(|from| (0..4).contains(from)), "{heard:?}");
        assert_ne!(heard, [0, 1, 2, 3].repeat(9), "out of order");
        let mut seen = heard.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, [0, 1, 2, 3], "the same dice, every one of them");
    }

    #[test]
    fn a_shorter_loop_keeps_the_places_past_it() {
        let mut dice = DiceLoop::default();
        let eight = DiceSettings {
            length: 8,
            ..LOCKED
        };
        for at in 0..8 {
            dice.roll(eight, 7, 0, step(at));
        }
        for at in 8..16 {
            dice.roll(LOCKED, 7, 0, step(at));
        }
        let back: Vec<i64> = (16..24)
            .map(|at| dice.roll(eight, 7, 0, step(at)).unwrap().running)
            .collect();
        assert_eq!(back, [0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn a_looped_chance_never_comes_up_at_none_and_always_at_full() {
        for running in 0..200 {
            assert!(!looped_chance(1, 0, step(running), 0, 0));
            assert!(looped_chance(1, 0, step(running), 0, 0xFF));
        }
        let hits = (0..2000)
            .filter(|at| looped_chance(1, 0, step(*at), 0, 0x80))
            .count();
        assert!((900..1100).contains(&hits), "{hits}");
    }
}
