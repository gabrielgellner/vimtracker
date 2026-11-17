//! The random streams a track draws from, each apart from the others, so `CHA` rolls as it did
//! and a render repeats.

use vt_model::constants::MAX_TRACKS;
use vt_model::insert::INSERTS;
use vt_model::modulation::MOD_SLOTS;

use crate::rng::Pcg32;

/// The generator for a track's humanise, on streams after every track's own for `CHA`.
pub(super) fn groove_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (MAX_TRACKS + index) as u64)
}

/// The generator for a track's random arpeggios, on streams after every track's humanise.
pub(super) fn arp_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (2 * MAX_TRACKS + index) as u64)
}

/// The generator for a mod slot's `Mutate`, on streams after every track's drift.
pub(super) fn mutate_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(seed, (4 * MAX_TRACKS + index * MOD_SLOTS + slot) as u64)
}

/// The generator for a Track mod slot's `Random`, on streams after every slot's `Mutate`.
pub(super) fn roll_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(
        seed,
        (4 * MAX_TRACKS + (MAX_TRACKS + index) * MOD_SLOTS + slot) as u64,
    )
}

/// The generator for a track's notes' drift, on streams after every track's arpeggios.
pub(super) fn drift_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (3 * MAX_TRACKS + index) as u64)
}

/// The generator for a track's `Random` accumulators, on streams after every Track mod slot's
/// `Random`.
pub(super) fn accumulator_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(
        seed,
        (4 * MAX_TRACKS + 2 * MAX_TRACKS * MOD_SLOTS + index) as u64,
    )
}

/// The generator for what a track's slot draws at random, Clouds' and Beads' grains, on streams after every
/// track's `Random` accumulators.
pub(super) fn effect_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(
        seed,
        (4 * MAX_TRACKS + 2 * MAX_TRACKS * MOD_SLOTS + MAX_TRACKS + index * INSERTS + slot) as u64,
    )
}

/// The generator for a Steps slot's `Jump`, on streams after every track's slots' effects.
pub(super) fn jump_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(
        seed,
        (4 * MAX_TRACKS
            + 2 * MAX_TRACKS * MOD_SLOTS
            + MAX_TRACKS
            + MAX_TRACKS * INSERTS
            + index * MOD_SLOTS
            + slot) as u64,
    )
}
