//! The project's random number generators, each written once. They're our own rather than the
//! `rand` crate's because a saved seed must play the same notes in every later version, and a
//! crate's generators may change their values between releases (decision 361). Each is a small
//! `Copy` type or a `const fn`, with no allocation, so the audio thread can use any of them, and
//! each is tested against values its authors published, or where they published none, against a
//! second implementation (see the tests).
//!
//! Some are kept in more than one variant, because a copy that differed in one detail plays by
//! that detail: the engine's PCG32 has a multiplier of its own, and the model's analysis tests use
//! musl's 64-bit LCG, MMIX's multiplier with an increment of 1. A new use takes whichever fits;
//! for a stateless value from a key and an index, `splitmix64_nth`.

/// SplitMix64's increment, 2^64 over the golden ratio, odd.
pub(crate) const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// The value SplitMix64 gives next from `state`: the state stepped by `GOLDEN_GAMMA`, then mixed
/// by its finaliser, which spreads every bit of what goes in over what comes out. As a stateless
/// mix, the same value always gives the same result. From Sebastiano Vigna's `splitmix64.c`
/// (<https://prng.di.unimi.it/splitmix64.c>), after Steele, Lea and Flood's `SplittableRandom`.
#[inline]
pub(crate) const fn splitmix64(state: u64) -> u64 {
    let mut z = state.wrapping_add(GOLDEN_GAMMA);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The `n`th value, counted from 0, that a `SplitMix64` started at `seed` gives, without
/// stepping through the ones before: a stateless number for a row or a cycle, so one row's value
/// doesn't depend on any other's.
#[inline]
pub(crate) const fn splitmix64_nth(seed: u64, n: u64) -> u64 {
    splitmix64(seed.wrapping_add(n.wrapping_mul(GOLDEN_GAMMA)))
}

/// SplitMix64 as a generator, stepping its state by `GOLDEN_GAMMA` each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// One started at `seed`, any value.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next value.
    #[inline]
    pub const fn next_u64(&mut self) -> u64 {
        let value = splitmix64(self.state);
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        value
    }
}

/// A 32-bit linear congruential generator, `state * MULTIPLIER + INCREMENT` modulo 2^32, giving
/// its whole new state. Its low bits repeat soon, the lowest every other step, so a use takes the
/// high ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lcg32<const MULTIPLIER: u32, const INCREMENT: u32> {
    state: u32,
}

impl<const MULTIPLIER: u32, const INCREMENT: u32> Lcg32<MULTIPLIER, INCREMENT> {
    /// One started at `seed`, any value.
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The state after `state`, for a copy that keeps its state as a plain number.
    #[inline]
    pub const fn step(state: u32) -> u32 {
        state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT)
    }

    /// The next value, the new state.
    #[inline]
    pub const fn next_u32(&mut self) -> u32 {
        self.state = Self::step(self.state);
        self.state
    }
}

/// Numerical Recipes' "even quicker" generator, `ranqd1`: Knuth's multiplier 1664525 with H. W.
/// Lewis's increment 1013904223 (Press et al., *Numerical Recipes in C*, 2nd ed., 1992, §7.1).
/// stmlib's `Random`, which Braids' noise draws from, is the same generator.
pub type NumericalRecipesLcg = Lcg32<1_664_525, 1_013_904_223>;

/// A 64-bit linear congruential generator, `state * MULTIPLIER + INCREMENT` modulo 2^64, giving
/// its whole new state. As with `Lcg32`, a use takes the high bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lcg64<const MULTIPLIER: u64, const INCREMENT: u64> {
    state: u64,
}

impl<const MULTIPLIER: u64, const INCREMENT: u64> Lcg64<MULTIPLIER, INCREMENT> {
    /// One started at `seed`, any value.
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next value, the new state.
    #[inline]
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(INCREMENT);
        self.state
    }
}

/// Knuth's multiplier for MMIX, which PCG's state steps by too (*The Art of Computer
/// Programming*, vol. 2, 3rd ed., §3.3.4, table 1).
pub(crate) const MMIX_MULTIPLIER: u64 = 6_364_136_223_846_793_005;

/// Knuth's MMIX generator, his multiplier with his increment 1442695040888963407 (*The Art of
/// Computer Programming*, vol. 2, 3rd ed., §3.3.4, table 1).
pub type MmixLcg = Lcg64<MMIX_MULTIPLIER, 1_442_695_040_888_963_407>;

/// musl's `rand`: MMIX's multiplier with an increment of 1, whose `rand()` is the state's top
/// 31 bits, `state >> 33` (musl's `src/prng/rand.c`).
pub type MuslLcg = Lcg64<MMIX_MULTIPLIER, 1>;

/// The multiplier the engine's PCG32 has always used. It matches PCG's, `MMIX_MULTIPLIER`, only
/// in its first seven digits, but it's still 1 modulo 4, so its state still runs through all 2^64
/// values, and every saved seed's effects and chances play by it, so it stays.
pub(crate) const ENGINE_PCG_MULTIPLIER: u64 = 6_364_136_299_858_216_321;

/// PCG's XSH RR generator, 64 bits of state giving 32 at a time, with the multiplier its state
/// steps by. Its seeding and output are Melissa O'Neill's `pcg32_srandom_r` and `pcg32_random_r`
/// (<https://www.pcg-random.org>, `pcg_basic.c`); `Pcg32` is hers exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pcg<const MULTIPLIER: u64> {
    state: u64,
    increment: u64,
}

/// PCG32 as published.
pub type Pcg32 = Pcg<MMIX_MULTIPLIER>;

/// The engine's PCG32, with its own multiplier, `ENGINE_PCG_MULTIPLIER`. Each track, effect and
/// modulation slot has one, on a stream of its own, so adding a track doesn't change what other
/// tracks do.
pub type EnginePcg32 = Pcg<ENGINE_PCG_MULTIPLIER>;

impl<const MULTIPLIER: u64> Pcg<MULTIPLIER> {
    /// A generator for a seed and a stream, any values: generators on different streams give
    /// different sequences from the same seed.
    pub const fn new(seed: u64, stream: u64) -> Self {
        let mut rng = Self {
            state: 0,
            increment: (stream << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// The next value.
    #[inline]
    pub const fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULTIPLIER).wrapping_add(self.increment);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    /// The next value as noise on `0..1`.
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / 4_294_967_296.0
    }

    /// The next value as noise on `-1..1`.
    #[inline]
    pub fn bipolar(&mut self) -> f32 {
        self.next_u32() as f32 / 2_147_483_648.0 - 1.0
    }

    /// Whether a chance of `value` out of 255 comes up. `00` never does and `FF` always does.
    #[inline]
    pub fn chance(&mut self, value: u8) -> bool {
        (self.next_u32() % 255) < u32::from(value)
    }
}

/// Marsaglia's xorshift32, by shifts of 13, 17 and 5 ("Xorshift RNGs", *Journal of Statistical
/// Software* 8(14), 2003). It's cheaper than PCG32; the tape's wobble, Grains, Warps' noise and the
/// 6-op's LFO each kept a copy, seeded by the effect itself so it's the same every time it
/// starts, and moving them onto PCG32 would change what they play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xorshift32 {
    state: u32,
}

impl Xorshift32 {
    /// One started at `seed`, which mustn't be 0, or it stays there.
    pub const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The next value.
    #[inline]
    pub(crate) const fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    /// The next value as a number from 0 to 1. `u32::MAX as f32` rounds to 2^32, so this is the
    /// same as dividing by `4_294_967_296.0`, as the 6-op's LFO wrote it.
    #[inline]
    pub fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_gives_vignas_reference_values() {
        // From Vigna's `splitmix64.c`, seeded with 1477776061723855037, as rand_xoshiro's
        // `SplitMix64` test lists them (rust-random/rngs, `rand_xoshiro/src/splitmix64.rs`).
        let expected = [
            1_985_237_415_132_408_290,
            2_979_275_885_539_914_483,
            13_511_426_838_097_143_398,
            8_488_337_342_461_049_707,
            15_141_737_807_933_549_159,
            17_093_170_987_380_407_015,
            16_389_528_042_912_955_399,
            13_177_319_091_862_933_652,
        ];
        let seed = 1_477_776_061_723_855_037;
        let mut rng = SplitMix64::new(seed);
        for (n, value) in expected.into_iter().enumerate() {
            assert_eq!(rng.next_u64(), value);
            assert_eq!(splitmix64_nth(seed, n as u64), value);
        }
        assert_eq!(splitmix64(seed), expected[0]);
    }

    #[test]
    fn numerical_recipes_lcg_gives_its_books_check_sequence() {
        // "See if you can generate the following sequence of 32-bit values (given here in hex):
        // 00000000, 3C6EF35F, 47502932, ..." (Numerical Recipes in C, 2nd ed., §7.1, `ranqd1`).
        let expected = [
            0x3C6E_F35F,
            0x4750_2932,
            0xD1CC_F6E9,
            0xAAF9_5334,
            0x6252_E503,
            0x9F2E_C686,
            0x57FE_6C2D,
            0xA3D9_5FA8,
            0x81FD_BEE7,
            0x94F0_AF1A,
            0xCBF6_33B1,
        ];
        let mut rng = NumericalRecipesLcg::new(0);
        let mut state = 0;
        for value in expected {
            assert_eq!(rng.next_u32(), value);
            state = NumericalRecipesLcg::step(state);
            assert_eq!(state, value);
        }
    }

    #[test]
    fn mmix_and_musl_lcgs_give_a_second_implementations_values() {
        // Neither Knuth nor musl publishes a sequence. These are from a separate implementation
        // in Python's exact integers, `(state * a + c) % 2**64` from 0, written for this test; the
        // first of each is the increment itself.
        let mut mmix = MmixLcg::new(0);
        for value in [
            1_442_695_040_888_963_407,
            1_876_011_003_808_476_466,
            11_166_244_414_315_200_793,
            7_401_132_627_792_533_940,
            7_076_646_890_315_895_283,
        ] {
            assert_eq!(mmix.next_u64(), value);
        }
        // musl's `rand()` after `srand(1)`, which starts its state at 0.
        let mut musl = MuslLcg::new(0);
        for value in [0, 740_882_966, 1_616_430_695, 1_708_849_955, 1_669_437_588] {
            assert_eq!(musl.next_u64() >> 33, value);
        }
    }

    #[test]
    fn pcg32_gives_the_pcg_demos_values() {
        // pcg32 seeded with 42 on stream 54, round 1 of the PCG library's own check
        // (imneme/pcg-c, `test-high/expected/check-pcg32.out`, as `pcg32-demo` prints it).
        let mut rng = Pcg32::new(42, 54);
        for value in [
            0xA15C_02B7,
            0x7B47_F409,
            0xBA1D_3330,
            0x83D2_F293,
            0xBFA4_784B,
            0xCBED_606E,
        ] {
            assert_eq!(rng.next_u32(), value);
        }
    }

    #[test]
    fn the_engines_pcg32_is_pcg32_with_its_own_multiplier() {
        // Its multiplier is 1 modulo 4, so its state runs through every value.
        assert_eq!(ENGINE_PCG_MULTIPLIER % 4, 1);
        let first = |rng: &mut EnginePcg32| (0..8).map(|_| rng.next_u32()).collect::<Vec<_>>();
        let (mut a, mut b, mut c) = (
            EnginePcg32::new(7, 0),
            EnginePcg32::new(7, 0),
            EnginePcg32::new(7, 1),
        );
        assert_eq!(first(&mut a), first(&mut b));
        assert_ne!(first(&mut a), first(&mut c));
        let mut published = Pcg32::new(42, 54);
        let published: Vec<u32> = (0..8).map(|_| published.next_u32()).collect();
        assert_ne!(first(&mut EnginePcg32::new(42, 54)), published);
    }

    #[test]
    fn chance_limits_always_and_never_hold() {
        let mut rng = EnginePcg32::new(1, 0);
        assert!((0..1000).all(|_| rng.chance(0xFF)));
        assert!((0..1000).all(|_| !rng.chance(0)));
        let hits = (0..10_000).filter(|_| rng.chance(0x80)).count();
        assert!((4_500..5_500).contains(&hits), "{hits}");
    }

    #[test]
    fn xorshift32_gives_a_second_implementations_values() {
        // Marsaglia's paper gives the generator and the seed 2463534242 but no outputs. These are
        // from a separate implementation in Python, masking each left shift to 32 bits.
        let mut rng = Xorshift32::new(2_463_534_242);
        for value in [
            723_471_715,
            2_497_366_906,
            2_064_144_800,
            2_008_045_182,
            3_532_304_609,
            374_114_282,
        ] {
            assert_eq!(rng.next_u32(), value);
        }
        assert_eq!((u32::MAX as f32).to_bits(), 4_294_967_296.0_f32.to_bits());
    }
}
