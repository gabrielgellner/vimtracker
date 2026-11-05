//! PCG32, a small seeded generator, so renders are repeatable.

/// Each track has its own, so adding a track doesn't change what other tracks do.
#[derive(Debug, Clone, Copy)]
pub struct Pcg32 {
    state: u64,
    increment: u64,
}

impl Pcg32 {
    /// A generator for a project seed and a track's index.
    pub fn new(seed: u64, stream: u64) -> Self {
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
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6_364_136_299_858_216_321)
            .wrapping_add(self.increment);
        let shifted = (((old >> 18) ^ old) >> 27) as u32;
        shifted.rotate_right((old >> 59) as u32)
    }

    /// Whether a chance of `value` out of 255 comes up. `00` never does and `FF` always does.
    pub fn chance(&mut self, value: u8) -> bool {
        (self.next_u32() % 255) < u32::from(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_repeats_and_streams_differ() {
        let mut a = Pcg32::new(7, 0);
        let mut b = Pcg32::new(7, 0);
        let mut c = Pcg32::new(7, 1);
        let first: Vec<u32> = (0..8).map(|_| a.next_u32()).collect();
        let second: Vec<u32> = (0..8).map(|_| b.next_u32()).collect();
        let third: Vec<u32> = (0..8).map(|_| c.next_u32()).collect();
        assert_eq!(first, second);
        assert_ne!(first, third);
    }

    #[test]
    fn chance_limits_always_and_never_hold() {
        let mut rng = Pcg32::new(1, 0);
        assert!((0..1000).all(|_| rng.chance(0xFF)));
        assert!((0..1000).all(|_| !rng.chance(0)));
        let hits = (0..10_000).filter(|_| rng.chance(0x80)).count();
        assert!((4_500..5_500).contains(&hits), "{hits}");
    }
}
