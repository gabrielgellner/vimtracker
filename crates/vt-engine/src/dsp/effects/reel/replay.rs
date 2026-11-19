//! What a reel machine aligned for its tape does around the tape itself (decision 449): the replay
//! EQ that makes up for the head's losses at the speed, the lows the heads leave, the highs that
//! loud short wavelengths erase, and what the bias does to the level and the top.

use std::f32::consts::TAU;

use crate::dsp::blocks::biquad::Biquad;

use super::{Loss, Matched};

/// The inverse of one of the losses' [`Matched`] filters: its poles become zeros and its zero a
/// pole, which sits inside the circle as long as the filter's gain at Nyquist is above nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Unmatched {
    c0: f32,
    c1: f32,
    c2: f32,
    d1: f32,
}

impl Unmatched {
    pub(super) fn of(filter: Matched) -> Self {
        let per = 1.0 / filter.b0;
        Self {
            c0: per,
            c1: filter.a1 * per,
            c2: filter.a2 * per,
            d1: filter.b1 * per,
        }
    }

    /// `state` is the last two in and the last out.
    #[inline]
    fn process(self, state: &mut [f32; 3], x: f32) -> f32 {
        let y = self.c0 * x + self.c1 * state[0] + self.c2 * state[1] - self.d1 * state[2];
        *state = [x, state[0], y];
        y
    }
}

/// The replay EQ: the head's spacing and thickness losses at the speed for the lengths the
/// machine was aligned for, undone, so a quiet sound comes back flat at every speed, and only a
/// gap, spacing or coating away from those changes its top. The gap's own loss is left: at the
/// lengths aligned for it's under a dB at 16 kHz at 3.75 ips.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Replay {
    filters: [Unmatched; 3],
}

impl Replay {
    pub(super) fn of(aligned: &Loss) -> Self {
        Self {
            filters: [
                Unmatched::of(aligned.spacing_pair),
                Unmatched::of(aligned.spacing_pole),
                Unmatched::of(aligned.thickness),
            ],
        }
    }
}

/// The lows at each speed, at 3.75, 7.5, 15 and 30 ips: the replay amplifier's high-pass (corner,
/// Q), the head bump (where, by how much in dB, Q) and the dip above it (as a share of the bump's
/// frequency, how deep, Q). Fitted to Satin's at its default bump, as a studio machine's measure,
/// within 0.2 dB from 20 Hz to 400 Hz at 7.5 ips and up, and 0.5 dB at 3.75, where Satin's
/// ripples on above 60 Hz.
const LOWS: [[f32; 8]; 4] = [
    [19.05, 0.72, 22.36, 4.00, 4.00, 1.96, 4.00, 3.11],
    [25.55, 0.74, 36.23, 2.60, 1.88, 2.00, 2.14, 2.07],
    [19.03, 0.41, 51.01, 3.37, 0.94, 2.37, 1.15, 1.31],
    [19.13, 0.40, 70.54, 2.31, 0.89, 3.01, 0.53, 1.06],
];

/// Where the highs a loud sound erases start at 3.75 ips, in Hz, moving with the speed, and the
/// level of them that loses half, at 3.75 ips and the drive aligned, rising with the sixth power of
/// the speed, as short wavelengths erase themselves and long ones barely do. What's kept is
/// `1 / (1 + (level / that)^1.5)`, so it starts late and grows fast, as Satin's does at 3.75 ips:
/// 16 kHz about 1 dB down at -10 dBFS and 3 dB at 0 dBFS, against a quiet sound.
const ERASURE_HZ: f32 = 2_000.0;
const ERASURE_LEVEL: f32 = 1.2;
const ERASURE_ATTACK_S: f32 = 0.000_3;
const ERASURE_RELEASE_S: f32 = 0.03;

/// The bias's tilt: a one-pole shelf from about 6 kHz, its gain at the top, so about half of it
/// at 10 kHz.
const TILT_HZ: f32 = 6_000.0;

/// What the bias does, at `00`, `40`, `80`, `C0` and `FF`: how far it moves the level the tape
/// sees and what comes off it, and the shelf's gain, each in dB, as the bias against level, output
/// and highs of a machine is measured, and as Satin's from -5 to +5 is: under-biased is a little
/// dirtier, quieter and brighter; over-biased cleaner, a little quieter and duller.
const BIAS_IN_DB: [f32; 5] = [1.75, 0.95, 0.0, -1.5, -5.0];
const BIAS_OUT_DB: [f32; 5] = [-4.25, -1.95, 0.0, 1.7, 3.9];
const BIAS_TILT_DB: [f32; 5] = [1.2, 0.8, 0.0, -1.0, -3.0];

/// A table at five even places across 0 to 1, read straight between.
fn read(table: &[f32; 5], share: f32) -> f32 {
    let place = share.clamp(0.0, 1.0) * 4.0;
    let at = (place.floor() as usize).min(3);
    let part = place - at as f32;
    table[at] + (table[at + 1] - table[at]) * part
}

/// The bias's gains, as shares: into the tape, out of it, and the tilt's.
pub(super) fn bias(share: f32) -> [f32; 3] {
    [&BIAS_IN_DB, &BIAS_OUT_DB, &BIAS_TILT_DB].map(|table| 10_f32.powf(read(table, share) / 20.0))
}

/// What's worked out for a speed, a bias and a drive.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Shape {
    replay: Replay,
    high_pass: Biquad,
    bump: Biquad,
    dip: Biquad,
    /// The one-pole high-passes' coefficient, for the erasure and the tilt.
    erasure_pole: f32,
    tilt_pole: f32,
    /// The tilt's gain over its band, less one.
    tilt: f32,
    /// One over the level that erases half, and the follower's coefficients.
    per_erasing: f32,
    attack: f32,
    release: f32,
}

impl Shape {
    pub(super) fn new(ips: f32, aligned: &Loss, tilt: f32, rate: f32) -> Self {
        let place = (ips / 3.75).max(1.0).log2().min(3.0);
        let at = (place.floor() as usize).min(2);
        let part = place - at as f32;
        let lows: [f32; 8] =
            std::array::from_fn(|k| LOWS[at][k] + (LOWS[at + 1][k] - LOWS[at][k]) * part);
        let [
            high_hz,
            high_q,
            bump_hz,
            bump_db,
            bump_q,
            dip_at,
            dip_db,
            dip_q,
        ] = lows;
        let pole = |hertz: f32| (-TAU * hertz.min(0.45 * rate) / rate).exp();
        let follower = |seconds: f32| 1.0 - (-1.0 / (seconds * rate)).exp();
        Self {
            replay: Replay::of(aligned),
            high_pass: Biquad::high_pass(high_hz, high_q, rate),
            bump: Biquad::peak(bump_hz, bump_q, bump_db, rate),
            dip: Biquad::peak(bump_hz * dip_at, dip_q, -dip_db, rate),
            erasure_pole: pole(ERASURE_HZ * ips / 3.75),
            tilt_pole: pole(TILT_HZ),
            tilt: tilt - 1.0,
            per_erasing: 1.0 / (ERASURE_LEVEL * (ips / 3.75).powi(6)),
            attack: follower(ERASURE_ATTACK_S),
            release: follower(ERASURE_RELEASE_S),
        }
    }
}

/// One side's state for it.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Played {
    replay: [[f32; 3]; 3],
    high_pass: [f32; 2],
    bump: [f32; 2],
    dip: [f32; 2],
    /// The erasure's and the tilt's one-pole low-passes, whose rest is their high-pass, and the
    /// erasure's follower.
    erasure_low: f32,
    tilt_low: f32,
    level: f32,
}

impl Played {
    /// What the tape holds after the highs a loud sound erases are taken off: `field` is how hard
    /// the tape is hit against the drive that's aligned, which the level that erases is read
    /// against.
    #[inline]
    pub(super) fn erase(&mut self, x: f32, field: f32, shape: &Shape) -> f32 {
        self.erasure_low = x + (self.erasure_low - x) * shape.erasure_pole;
        let high = x - self.erasure_low;
        let hit = (high * field).abs();
        let rate = if hit > self.level {
            shape.attack
        } else {
            shape.release
        };
        self.level += (hit - self.level) * rate;
        let against = self.level * shape.per_erasing;
        let kept = 1.0 / (1.0 + against * against.sqrt());
        x - (1.0 - kept) * high
    }

    /// Off the head: the replay EQ, the lows and the bias's tilt.
    #[inline]
    pub(super) fn replay(&mut self, x: f32, shape: &Shape) -> f32 {
        let mut out = x;
        for (filter, state) in shape.replay.filters.iter().zip(&mut self.replay) {
            out = filter.process(state, out);
        }
        out = shape.high_pass.process(&mut self.high_pass, out);
        out = shape.bump.process(&mut self.bump, out);
        out = shape.dip.process(&mut self.dip, out);
        self.tilt_low = out + (self.tilt_low - out) * shape.tilt_pole;
        out + shape.tilt * (out - self.tilt_low)
    }
}
