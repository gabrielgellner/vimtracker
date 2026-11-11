//! How many times slower than its full speed the system is running the audio thread. macOS slows
//! a light, long-period callback by putting it on an efficiency core or a lower clock, and Low
//! Power Mode slows every core, so the same work takes longer and the CPU figure reads higher.
//!
//! A small fixed piece of arithmetic, [`kernel`], is timed now and then in the callback, and its
//! time is set against its time at full speed, which [`full_speed`] finds on a busy thread before
//! audio starts. Neither the kernel nor its timing allocates, locks or calls the system: the clock
//! is read from memory the kernel shares with every process.

use std::time::{Duration, Instant};

/// Which kernel this is, so a full speed kept from another is never compared with it.
pub const KERNEL_VERSION: u32 = 1;
/// The kernel's rounds: about 1.2 µs on an Apple M5's performance core at full speed.
const ROUNDS: usize = 1024;
/// The filters the kernel runs side by side, as a track's voices and effects do.
const LANES: usize = 32;
/// How many times the kernel runs for one look, the fastest counting. A look the system
/// interrupts is then thrown out unless every run is interrupted.
const RUNS_A_LOOK: usize = 3;
/// How long [`full_speed`] keeps a thread busy timing the kernel, the fastest run counting. The
/// system soon moves a busy thread to a fast core and raises its clock, and on a machine busy
/// with other work, the longer it runs, the likelier it gets a fast core to itself for a while.
const FULL_SPEED_BUSY: Duration = Duration::from_millis(100);
/// The most a look says, so one the system interrupts throughout can't say much more.
const MOST: f64 = 8.0;

/// A fixed run of arithmetic like an audio engine's, which always gives the same answer for a
/// seed: a bank of one-pole filters, fed by a random number generator, in single floats. It
/// takes as long as the core and its clock make it take, and nothing else, so its time against
/// its time at full speed is how much the core is slowed.
#[inline(never)]
pub fn kernel(seed: u64) -> u64 {
    let mut state = seed;
    let mut lanes = [0.0_f32; LANES];
    let coefficients: [f32; LANES] = std::array::from_fn(|lane| 0.5 + lane as f32 / 128.0);
    for _ in 0..ROUNDS {
        // Knuth's MMIX generator, with its top 24 bits as a sample from 0 to 1.
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let input = (state >> 40) as f32 * (1.0 / 16_777_216.0);
        for (level, coefficient) in lanes.iter_mut().zip(coefficients) {
            *level = *level * coefficient + input * (1.0 - coefficient);
        }
    }
    lanes.iter().fold(state, |sum, level| {
        sum.rotate_left(7) ^ u64::from(level.to_bits())
    })
}

/// The kernel's time, the fastest of a few runs.
pub fn time_kernel() -> Duration {
    let mut fastest = Duration::MAX;
    for run in 0..RUNS_A_LOOK {
        let started = Instant::now();
        std::hint::black_box(kernel(std::hint::black_box(run as u64)));
        fastest = fastest.min(started.elapsed());
    }
    fastest
}

/// The kernel's time at full speed: the fastest of many runs, on this thread kept busy for
/// 100 ms. It's for before audio starts, never the audio thread.
pub fn full_speed() -> Duration {
    let started = Instant::now();
    let mut fastest = Duration::MAX;
    while started.elapsed() < FULL_SPEED_BUSY {
        fastest = fastest.min(time_kernel());
    }
    fastest
}

/// The callback's look at how slowed it is, every quarter of a second of audio.
#[derive(Debug, Clone)]
pub struct SlowdownCheck {
    /// The kernel's time at full speed, in seconds.
    full_speed: f64,
    /// Frames between looks.
    every: usize,
    /// Frames since the last look.
    since: usize,
    /// The last look's own factor.
    last: Option<f64>,
}

/// What one look found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// How long the kernel took.
    pub time: Duration,
    /// How many times its time at full speed that is, up to 8, or the last look's, whichever is
    /// less. A look the system interrupts only ever reads slower, so one alone doesn't count.
    pub factor: f64,
}

impl SlowdownCheck {
    /// Looks four times a second of audio at a rate, against the kernel's time at full speed.
    pub fn new(full_speed: Duration, rate: u32) -> Self {
        let every = (rate as usize / 4).max(1);
        Self {
            full_speed: full_speed.as_secs_f64(),
            every,
            // The first callback looks, so the figure is there from the start.
            since: every,
            last: None,
        }
    }

    /// Count a callback's frames, and look if it's time. Call it at the callback's end, so the
    /// kernel runs as the engine just did.
    #[inline]
    pub fn after(&mut self, frames: usize) -> Option<Look> {
        self.since += frames;
        if self.since < self.every {
            return None;
        }
        // What's over carries on, so the looks keep to four a second whatever the buffer.
        self.since %= self.every;
        Some(self.found(time_kernel()))
    }

    /// What a look finds when the kernel takes so long.
    pub fn found(&mut self, time: Duration) -> Look {
        let factor = (time.as_secs_f64() / self.full_speed.max(1e-12)).min(MOST);
        let last = self.last.replace(factor).unwrap_or(factor);
        Look {
            time,
            factor: factor.min(last),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt_guard::forbidden;

    #[test]
    fn looking_never_allocates() {
        let mut check = SlowdownCheck::new(Duration::from_micros(2), 48_000);
        let (looks, violations) =
            forbidden(|| (0..400).filter(|_| check.after(512).is_some()).count());
        assert_eq!(violations, 0);
        // 400 callbacks of 512 frames are 4.27 s, a look each 12,000 frames, and one at the start.
        assert_eq!(looks, 18);
    }
}
