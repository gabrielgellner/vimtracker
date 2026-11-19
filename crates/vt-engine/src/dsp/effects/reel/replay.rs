//! What a reel machine aligned for its tape does around the tape itself (decision 449): the replay
//! EQ that makes up for the head's losses at the speed, the head's response for its gap, the lows
//! the heads leave, the highs that loud short wavelengths erase, and what the bias does to the
//! level and the top.

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
/// spacing or coating away from those changes its top.
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
/// sees and what comes off it, and the shelf's gain, each in dB, and the share of the tape's
/// saturation that switches late when it's biased too little ([`Switching`]), as the bias against
/// level, output and highs of a machine is measured, and as Satin's from -5 to +5 is: under-biased
/// is a little dirtier, quieter and brighter, and loud, its harmonics come and go with the level;
/// over-biased cleaner, a little quieter and duller. The switching's centre and width are the
/// anhysteretic curve's argument, fitted with the gains to Satin's gain, third and fifth harmonic
/// of a 1 kHz sine from -12 to +12 dBFS at its bias -5 and -2.5.
///
/// [`Switching`]: crate::dsp::blocks::hysteresis::Switching
const BIAS_IN_DB: [f32; 5] = [2.88, 1.46, 0.0, -1.5, -5.0];
const BIAS_OUT_DB: [f32; 5] = [-5.72, -2.63, 0.0, 1.7, 3.9];
const BIAS_TILT_DB: [f32; 5] = [1.2, 0.8, 0.0, -1.0, -3.0];
const BIAS_SWITCHING: [f32; 5] = [0.954, 0.494, 0.0, 0.0, 0.0];
pub(super) const SWITCHING_CENTRE: f64 = 4.341;
pub(super) const SWITCHING_WIDTH: f64 = 1.823;

/// A table at five even places across 0 to 1, read straight between.
fn read(table: &[f32; 5], share: f32) -> f32 {
    let place = share.clamp(0.0, 1.0) * 4.0;
    let at = (place.floor() as usize).min(3);
    let part = place - at as f32;
    table[at] + (table[at + 1] - table[at]) * part
}

/// The bias's gains, as shares: into the tape, out of it, and the tilt's; and the share of the
/// saturation that switches late.
pub(super) fn bias(share: f32) -> [f32; 4] {
    let [into, out, tilt] = [&BIAS_IN_DB, &BIAS_OUT_DB, &BIAS_TILT_DB]
        .map(|table| 10_f32.powf(read(table, share) / 20.0));
    [into, out, tilt, read(&BIAS_SWITCHING, share)]
}

/// The playback head's response against the gap, at 3.75, 7.5, 15 and 30 ips: a high shelf's
/// corner and Q, then at a gap of 1, 2, 3, 4 and 5 µm the level in dB against 3 µm's and the
/// shelf's gain in dB. Satin's `Gap Width` turns its top and its level so, at every speed (a
/// narrow gap brighter and a little quieter), where a gap's own loss would barely touch 16 kHz at
/// 15 ips and null it at 3.75; fitted to Satin's, within 0.75 dB from 300 Hz to 19 kHz at -30 dBFS,
/// and its 3 µm is Satin's default, so this is also each speed's own top: +2 dB at 16 kHz at 30 ips.
const HEAD: [[f32; 12]; 4] = [
    [
        2172.8, 0.46, -1.93, 8.53, -0.67, 2.61, 0.0, -0.98, 0.41, -3.33, 0.81, -5.02,
    ],
    [
        4000.2, 0.45, -1.81, 7.35, -0.75, 2.34, 0.0, -1.22, 0.61, -3.82, 1.21, -5.84,
    ],
    [
        7069.1, 0.44, -1.78, 7.66, -0.78, 3.93, 0.0, 0.70, 0.72, -1.97, 1.45, -4.16,
    ],
    [
        8534.7, 0.45, -1.90, 6.55, -0.88, 4.18, 0.0, 2.11, 0.84, 0.17, 1.69, -1.70,
    ],
];

/// What's worked out for a speed, a bias and a drive.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Shape {
    replay: Replay,
    /// The head's shelf and level for the gap.
    head: Biquad,
    level: f32,
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
    /// For a speed, a gap in µm, the losses aligned for and the bias's tilt.
    pub(super) fn new(ips: f32, gap_um: f32, aligned: &Loss, tilt: f32, rate: f32) -> Self {
        let place = (ips / 3.75).max(1.0).log2().min(3.0);
        let at = (place.floor() as usize).min(2);
        let part = place - at as f32;
        let lows: [f32; 8] =
            std::array::from_fn(|k| LOWS[at][k] + (LOWS[at + 1][k] - LOWS[at][k]) * part);
        let head: [f32; 12] =
            std::array::from_fn(|k| HEAD[at][k] + (HEAD[at + 1][k] - HEAD[at][k]) * part);
        let gap = (gap_um - 1.0).clamp(0.0, 4.0);
        let gap_at = (gap.floor() as usize).min(3);
        let gap_part = gap - gap_at as f32;
        let by_gap = |k: usize| {
            let (low, high) = (head[2 + 2 * gap_at + k], head[4 + 2 * gap_at + k]);
            low + (high - low) * gap_part
        };
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
            head: Biquad::high_shelf(head[0].min(0.45 * rate), head[1], by_gap(1), rate),
            level: 10_f32.powf(by_gap(0) / 20.0),
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
    head: [f32; 2],
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

    /// Off the head: the replay EQ, the head's response for its gap, the lows and the bias's
    /// tilt.
    #[inline]
    pub(super) fn replay(&mut self, x: f32, shape: &Shape) -> f32 {
        let mut out = x;
        for (filter, state) in shape.replay.filters.iter().zip(&mut self.replay) {
            out = filter.process(state, out);
        }
        out = shape.head.process(&mut self.head, out) * shape.level;
        out = shape.high_pass.process(&mut self.high_pass, out);
        out = shape.bump.process(&mut self.bump, out);
        out = shape.dip.process(&mut self.dip, out);
        self.tilt_low = out + (self.tilt_low - out) * shape.tilt_pole;
        out + shape.tilt * (out - self.tilt_low)
    }
}
