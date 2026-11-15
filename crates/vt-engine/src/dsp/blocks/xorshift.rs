//! Marsaglia's xorshift32, the generator the tape's wobble, Grains and Warps' noise each kept a
//! copy of. It's cheaper than `Pcg32` and seeded by the effect itself, so it's the same every
//! time it starts. Moving them onto `Pcg32` would change what they play, so they keep it.

/// A 32-bit xorshift, by shifts of 13, 17 and 5.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Xorshift {
    state: u32,
}

impl Xorshift {
    /// One started at `seed`, which mustn't be 0, or it stays there.
    pub(crate) const fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// The next value.
    #[inline]
    pub(crate) fn next_u32(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    /// The next value as a number from 0 to 1.
    #[inline]
    pub(crate) fn unit(&mut self) -> f32 {
        self.next_u32() as f32 / u32::MAX as f32
    }
}
