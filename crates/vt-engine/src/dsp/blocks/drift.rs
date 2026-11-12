//! What makes each note of an analog synth a little different from the last, for the voices with a
//! `Drift` setting (decision 190), and the noise floor under their filters (decision 191). Every
//! value comes from a generator seeded for each note, so a render repeats, where OB-Xf and Surge
//! both call `rand()`.
//!
//! The offsets each note rolls are OB-Xf's "slop": one value for each oscillator's tuning, the
//! cutoff, the level and the envelopes' times (`OBXF/OscillatorBlock.h`, `OBXF/Voice.h`,
//! `OBXF/SynthEngine.h`). The slow wander in pitch is sst's `DriftLFO`, one-pole-filtered noise
//! stepped once a block, as Surge's Modern oscillator uses it (`OscillatorDriftUnisonCharacter.h`,
//! `ModernOscillator.cpp`). Both are GPL-3.0-or-later; this is written from them.

use crate::rng::Pcg32;

/// The most oscillators a voice detunes one by one: `Chiptune`'s five squares, a chord of four
/// with one note taken up or down as it's inverted.
pub(crate) const OSCILLATORS: usize = 5;
/// How far each oscillator is tuned off at `Drift` full, either way, in semitones: OB-Xf's
/// `tuningSlop`, a white value per oscillator.
const TUNE_SEMITONES: f32 = 0.1;
/// How fast the wander moves: sst's filter coefficient, once a block.
const WANDER_COEFFICIENT: f32 = 1e-5;
/// What brings the wander up to about a semitone's swing at its widest, as sst's `1/sqrt(filter)`
/// does. From a note's start it moves about a tenth of a semitone in its first second at `Drift`
/// full, at 48 kHz, so a long note drifts and a short one hardly does.
const WANDER_GAIN: f32 = 316.227_77;
/// How far the cutoff moves at `Drift` full, either way, in semitones. OB-Xf's filter slop goes to
/// 9; a third of that is enough to hear notes differ without one going dull.
const CUTOFF_SEMITONES: f32 = 3.0;
/// How far the level moves at `Drift` full, either way, in dB. OB-Xf's goes to about 3.
const LEVEL_DB: f32 = 1.5;
/// How much longer or shorter the envelopes are at `Drift` full, as a share. OB-Xf's go to a half.
const TIME_SHARE: f32 = 0.2;
/// The noise under the filter, as a share of full scale: -90 dB, so a filter at full resonance
/// rings from silence as an analog one does (decision 191).
const NOISE_FLOOR: f32 = 3.162_278e-5;
/// Its generator's stream, apart from the other voices' noise.
const STREAM: u64 = 9;

/// A note's offsets, its wander and its noise floor. It's plain data, so a voice can hold one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Drift {
    rng: Pcg32,
    /// Each oscillator's tuning, the cutoff, the level and the envelopes' times, each on `-1..1`,
    /// which `Drift` scales.
    tune: [f32; OSCILLATORS],
    cutoff: f32,
    level: f32,
    time: f32,
    /// sst's `d2`: the noise, filtered.
    wander: f32,
}

impl Default for Drift {
    fn default() -> Self {
        Self::new()
    }
}

impl Drift {
    /// One that hasn't been seeded: no offsets, so a voice nobody seeds plays as it always has.
    pub(crate) fn new() -> Self {
        Self {
            rng: Pcg32::new(0, STREAM),
            tune: [0.0; OSCILLATORS],
            cutoff: 0.0,
            level: 0.0,
            time: 0.0,
            wander: 0.0,
        }
    }

    /// Start a note from its seed: new offsets, and the wander from the middle.
    pub(crate) fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), STREAM);
        self.wander = 0.0;
        self.roll();
    }

    /// New offsets for a note that carries on from the last, as `Acid`'s does, leaving the wander
    /// where it is.
    pub(crate) fn roll(&mut self) {
        for oscillator in 0..OSCILLATORS {
            self.tune[oscillator] = self.bipolar();
        }
        self.cutoff = self.bipolar();
        self.level = self.bipolar();
        self.time = self.bipolar();
    }

    /// Noise on `-1..1`.
    #[inline]
    fn bipolar(&mut self) -> f32 {
        self.rng.next_u32() as f32 / 2_147_483_648.0 - 1.0
    }

    /// A start phase, on `0..1`.
    pub(crate) fn phase(&mut self) -> f32 {
        self.rng.next_u32() as f32 / 4_294_967_296.0
    }

    /// Move the wander on, once a block.
    #[inline]
    pub(crate) fn step(&mut self) {
        let white = self.bipolar();
        self.wander += (white - self.wander) * WANDER_COEFFICIENT;
    }

    /// What an oscillator's frequency is multiplied by, at `Drift` `amount` on `0..1`: its own
    /// offset and the wander they all share. Exactly 1 at `00`.
    #[inline]
    pub(crate) fn ratio(&self, oscillator: usize, amount: f32) -> f32 {
        let semitones = TUNE_SEMITONES * self.tune[oscillator] + self.wander * WANDER_GAIN;
        (amount * semitones / 12.0).exp2()
    }

    /// How far the cutoff moves, in octaves.
    #[inline]
    pub(crate) fn cutoff_octaves(&self, amount: f32) -> f32 {
        amount * CUTOFF_SEMITONES / 12.0 * self.cutoff
    }

    /// What the level is multiplied by.
    #[inline]
    pub(crate) fn level(&self, amount: f32) -> f32 {
        (amount * LEVEL_DB / 20.0 * self.level * std::f32::consts::LN_10).exp()
    }

    /// What the envelopes' times are multiplied by.
    #[inline]
    pub(crate) fn time(&self, amount: f32) -> f32 {
        1.0 + amount * TIME_SHARE * self.time
    }

    /// The next sample of the noise floor, to add to what goes into a filter.
    #[inline]
    pub(crate) fn noise(&mut self) -> f32 {
        NOISE_FLOOR * self.bipolar()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_none_it_changes_nothing() {
        let mut drift = Drift::new();
        drift.seed(42);
        for _ in 0..1000 {
            drift.step();
        }
        for oscillator in 0..OSCILLATORS {
            assert_eq!(drift.ratio(oscillator, 0.0), 1.0);
        }
        assert_eq!(drift.cutoff_octaves(0.0), 0.0);
        assert_eq!(drift.level(0.0), 1.0);
        assert_eq!(drift.time(0.0), 1.0);
    }

    #[test]
    fn a_seed_repeats_and_another_differs() {
        let rolled = |seed| {
            let mut drift = Drift::new();
            drift.seed(seed);
            (drift.ratio(0, 1.0), drift.ratio(1, 1.0), drift.level(1.0))
        };
        assert_eq!(rolled(1), rolled(1));
        assert_ne!(rolled(1), rolled(2));
    }

    #[test]
    fn its_offsets_stay_in_their_ranges() {
        let cents = |ratio: f32| 1200.0 * ratio.log2();
        for seed in 0..500 {
            let mut drift = Drift::new();
            drift.seed(seed);
            for oscillator in 0..OSCILLATORS {
                let off = cents(drift.ratio(oscillator, 1.0));
                assert!(off.abs() <= 10.5, "{off} cents");
            }
            assert!(drift.cutoff_octaves(1.0).abs() <= 0.25);
            let db = 20.0 * drift.level(1.0).log10();
            assert!(db.abs() <= 1.51, "{db} dB");
            assert!((0.8..=1.2).contains(&drift.time(1.0)));
        }
    }

    #[test]
    fn the_wander_moves_slowly() {
        // A second of blocks of 16 at 48 kHz.
        let mut drift = Drift::new();
        let mut widest = 0.0_f32;
        for seed in 0..20 {
            drift.seed(seed);
            drift.tune = [0.0; OSCILLATORS];
            for _ in 0..3000 {
                drift.step();
            }
            widest = widest.max((1200.0 * drift.ratio(0, 1.0).log2()).abs());
        }
        assert!((1.0..60.0).contains(&widest), "{widest} cents");
    }

    #[test]
    fn the_noise_floor_is_ninety_db_down() {
        let mut drift = Drift::new();
        drift.seed(3);
        let loudest = (0..48_000).map(|_| drift.noise().abs()).fold(0.0, f32::max);
        let db = 20.0 * loudest.log10();
        assert!((-91.0..=-90.0).contains(&db), "{db}");
    }
}
