//! A reel machine's wow and flutter, its own rather than `Tape`'s (decision 449): the speed of the
//! tape wanders, and the line is read back by how far it has wandered.
//!
//! - **Flutter** is what turns with the tape: lines at 13.4 Hz and 7.75 Hz, the capstan and its
//!   pinch roller, moving with the speed, and at 34.4 Hz and 55.5 Hz, the idlers and the motor,
//!   moving less with it, as a studio machine at 15 ips measures (and as Satin's does), with a
//!   little random flutter between them, part of it each side's own, and each line's speed
//!   wandering by about a percent.
//! - **Wow** is a slow, smooth random drift from about 0.1 Hz to 1.5 Hz, as the reels' tension
//!   changes, not a sine: a lean in pitch rather than a pulse.
//!
//! Both are worked out as the tape's speed, in cents, every 16 frames, and summed into how far the
//! line is read back, going straight between. Each knob's depth is even in its value, and `00` none
//! at all: the line is then read seven frames back, still, and exactly. `Flutter`'s `FF` is about
//! 9.6 cents rms, Satin's `Wow & Flutter` at 100, and `Wow`'s the same, so its top moves the pitch
//! as far as Satin's whole knob does, only slowly.

use std::f32::consts::TAU;

use vt_model::random::Xorshift32;

use crate::dsp::blocks::line::StereoLine;

/// How often the speed is worked out, in frames.
const CONTROL_FRAMES: u32 = 16;
/// How far back the line is always read, in frames: as near as the reader can.
pub(super) const READ_BACK: f32 = (TAPS / 2 - 1) as f32;
/// Cents rms at the top of each knob.
pub(super) const MOST_FLUTTER_CENTS: f32 = 9.6;
pub(super) const MOST_WOW_CENTS: f32 = 9.6;
/// Where wow's band starts and ends, in Hz.
const WOW_LOWEST_HZ: f32 = 0.1;
const WOW_HIGHEST_HZ: f32 = 1.5;
/// One cent as a share of the speed, near enough: `ln 2 / 1200`.
const CENT: f32 = 5.776_227e-4;

/// Flutter's lines at 15 ips: where, the share of flutter's power each has, and how far each
/// moves with the speed (1 in proportion, less below it). The rest of the power is random.
const LINES: [(f32, f32, f32); 4] = [
    (13.4, 0.57, 1.0),
    (34.4, 0.32, 0.36),
    (7.75, 0.03, 1.0),
    (55.5, 0.03, 0.36),
];
const RANDOM_SHARE: f32 = 0.05;
/// How much of the random flutter is each side's own, as a share of flutter's power.
const OWN_SHARE: f32 = 0.03;
/// How far each line's speed wanders, as a share.
const WANDER: f32 = 0.01;

/// How much flutter there is for the speed, against 15 ips, at 3.75, 7.5, 15 and 30 ips.
const FLUTTER_BY_SPEED: [f32; 4] = [1.12, 1.06, 1.0, 0.55];

/// Gaussian-ish noise of unit variance, from the sum of three uniforms.
#[inline]
fn noise(random: &mut Xorshift32) -> f32 {
    (random.unit() + random.unit() + random.unit() - 1.5) * 2.0
}

/// A state-variable filter run at the control rate, for shaping the noise.
#[derive(Debug, Clone, Copy, Default)]
struct Shaper {
    low: f32,
    band: f32,
    f: f32,
    damping: f32,
}

impl Shaper {
    fn new(hertz: f32, q: f32, rate: f32) -> Self {
        Self {
            low: 0.0,
            band: 0.0,
            f: 2.0 * (std::f32::consts::PI * hertz / rate).sin(),
            damping: 1.0 / q,
        }
    }

    /// Its low, band and high outputs.
    #[inline]
    fn process(&mut self, x: f32) -> (f32, f32, f32) {
        self.low += self.f * self.band;
        let high = x - self.low - self.damping * self.band;
        self.band += self.f * high;
        (self.low, self.band, high)
    }
}

/// How loud each shaper's output is for unit noise, found once by running it, so the knobs read in
/// cents whatever the rate.
fn loudness(mut shape: impl FnMut(f32) -> f32, seed: u32) -> f32 {
    let mut random = Xorshift32::new(seed);
    let (mut sum, count) = (0.0_f64, 1 << 16);
    for step in 0..(count + 4096) {
        let y = shape(noise(&mut random));
        if step >= 4096 {
            sum += f64::from(y) * f64::from(y);
        }
    }
    ((sum / f64::from(count)).sqrt() as f32).max(1e-9)
}

/// The noise for wow: a band from about 0.1 Hz to 1.5 Hz, of unit rms, its edges damped so nothing
/// in it rings at a rate of its own.
#[derive(Debug, Clone, Copy, Default)]
struct Drift {
    high: Shaper,
    low: Shaper,
    scale: f32,
}

impl Drift {
    fn new(rate: f32) -> Self {
        let mut drift = Self {
            high: Shaper::new(WOW_LOWEST_HZ, 0.5, rate),
            low: Shaper::new(WOW_HIGHEST_HZ, 0.5, rate),
            scale: 1.0,
        };
        let mut probe = drift;
        drift.scale = 1.0 / loudness(|x| probe.process(x), 0x2545_F491);
        drift
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let high = self.high.process(x).2;
        self.low.process(high).0 * self.scale
    }
}

/// The noise for random flutter: a band about 20 Hz, of unit rms.
#[derive(Debug, Clone, Copy, Default)]
struct Rattle {
    band: Shaper,
    low: Shaper,
    scale: f32,
}

impl Rattle {
    fn new(rate: f32) -> Self {
        let mut rattle = Self {
            band: Shaper::new(20.0, 0.8, rate),
            low: Shaper::new(45.0, 0.71, rate),
            scale: 1.0,
        };
        let mut probe = rattle;
        rattle.scale = 1.0 / loudness(|x| probe.process(x), 0x68E3_1DA4);
        rattle
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let band = self.band.process(x).1;
        self.low.process(band).0 * self.scale
    }
}

/// A reel's transport.
#[derive(Debug, Clone)]
pub(super) struct Transport {
    random: Xorshift32,
    /// Where round each line is, and its speed's wander.
    turns: [f32; 4],
    wander: [Drift; 4],
    drift: Drift,
    rattle: Rattle,
    own: [Rattle; 2],
    /// How far each side's line is read back beyond the frame it's always read back, in frames,
    /// with no wow or flutter at all, before the knobs scale it.
    wow_frames: f32,
    flutter_frames: [f32; 2],
    /// How far back each side is read now, how far that moves each frame, and frames until the
    /// next is worked out. Without one, the next is jumped to.
    delay: Option<[f32; 2]>,
    slope: [f32; 2],
    countdown: u32,
    /// The control rate, and how much a cent's speed moves the line a step, in frames.
    control: f32,
    leak: f32,
}

impl Transport {
    pub(super) fn new(rate: u32) -> Self {
        let control = rate as f32 / CONTROL_FRAMES as f32;
        let drift = Drift::new(control);
        let rattle = Rattle::new(control);
        Self {
            random: Xorshift32::new(0x1F12_3BB5),
            turns: [0.0, 0.31, 0.62, 0.17],
            wander: [drift; 4],
            drift,
            rattle,
            own: [rattle; 2],
            wow_frames: 0.0,
            flutter_frames: [0.0; 2],
            delay: None,
            slope: [0.0; 2],
            countdown: 0,
            control,
            // What wow and the random flutter have moved the line leaks away below about 0.05 Hz,
            // so it never wanders off.
            leak: (-TAU * 0.05 / control).exp(),
        }
    }

    /// Jump to the next length worked out, as a reel only just started does.
    pub(super) fn restart(&mut self) {
        self.delay = None;
        self.countdown = 0;
    }

    /// The most either side can be read back by its moving part, in frames at a rate, with room
    /// for the rare loud stretch of the noise.
    pub(super) fn most_frames(rate: u32) -> f32 {
        2.0 * (Self::wow_room() + Self::flutter_room()) * rate as f32
    }

    /// How far wow can read the line back, in seconds: six times how far it reads it back rms, which
    /// is its speed's rms over `2π 0.44 Hz` (measured over 20 minutes, in which its farthest was
    /// 4.1 times its rms).
    fn wow_room() -> f32 {
        6.0 * MOST_WOW_CENTS * CENT / (TAU * 0.44)
    }

    /// How far flutter can: its slowest line, at 3.75 ips (3.35 Hz), at its fullest.
    fn flutter_room() -> f32 {
        2.0 * MOST_FLUTTER_CENTS * CENT * FLUTTER_BY_SPEED[0] / (TAU * 3.35)
    }

    /// How far back to read each side's line this frame, in frames: `wow` and `flutter` are
    /// shares from 0 to 1 of the most, at a speed in inches a second.
    #[inline]
    pub(super) fn next(&mut self, wow: f32, flutter: f32, ips: f32) -> [f32; 2] {
        if self.countdown == 0 {
            self.countdown = CONTROL_FRAMES;
            let next = self.step(wow, flutter, ips);
            let now = *self.delay.get_or_insert(next);
            for side in 0..2 {
                self.slope[side] = (next[side] - now[side]) / CONTROL_FRAMES as f32;
            }
        }
        self.countdown -= 1;
        let delay = self.delay.get_or_insert([READ_BACK; 2]);
        for side in 0..2 {
            delay[side] += self.slope[side];
        }
        *delay
    }

    /// The line's length at the next control step.
    fn step(&mut self, wow: f32, flutter: f32, ips: f32) -> [f32; 2] {
        let frames = CONTROL_FRAMES as f32;
        let ratio = ips / 15.0;
        // Each line, as how far it moves the line, in frames a cent: a speed of `a cos ωt` reads
        // the line `a / ω` further at most.
        let by_speed = {
            let place = (ips / 3.75).max(1.0).log2().min(3.0);
            let at = place.floor() as usize;
            let next = (at + 1).min(3);
            let part = place - at as f32;
            FLUTTER_BY_SPEED[at] + (FLUTTER_BY_SPEED[next] - FLUTTER_BY_SPEED[at]) * part
        };
        let mut lines = 0.0;
        for (index, (hertz, share, follows)) in LINES.into_iter().enumerate() {
            let wander = self.wander[index].process(noise(&mut self.random));
            let hertz = hertz * ratio.powf(follows) * (1.0 + WANDER * wander);
            self.turns[index] += hertz / self.control;
            self.turns[index] -= self.turns[index].floor();
            // A line of `c` cents rms is `√2 c` at its peak.
            let peak = std::f32::consts::SQRT_2 * share.sqrt() * CENT * self.control * frames;
            lines += peak / (TAU * hertz) * (self.turns[index] * TAU).sin();
        }
        // The noises are speeds, so they're summed into the line as they go, leaking away.
        let random = self.rattle.process(noise(&mut self.random));
        let mut own = [0.0; 2];
        for (side, rattle) in self.own.iter_mut().enumerate() {
            own[side] = rattle.process(noise(&mut self.random));
        }
        let drift = self.drift.process(noise(&mut self.random));
        self.wow_frames = self.wow_frames * self.leak + drift * CENT * frames;
        let shared = (RANDOM_SHARE - OWN_SHARE).sqrt();
        for side in 0..2 {
            let speed = (shared * random + OWN_SHARE.sqrt() * own[side]) * CENT * frames;
            self.flutter_frames[side] = self.flutter_frames[side] * self.leak + speed;
        }
        let depth_wow = wow * MOST_WOW_CENTS;
        let depth_flutter = flutter * MOST_FLUTTER_CENTS * by_speed;
        // The line is read back half the reader's width, and further by as much as either can
        // take it, so it's never read ahead: a little later as either is turned up, but still at
        // `00`.
        let rate = self.control * frames;
        let ahead = (wow * Self::wow_room()
            + flutter * by_speed / FLUTTER_BY_SPEED[0] * Self::flutter_room())
            * rate;
        let mut out = [0.0; 2];
        for side in 0..2 {
            let flutter_part = (lines + self.flutter_frames[side]) * depth_flutter;
            let wow_part = self.wow_frames * depth_wow;
            out[side] = READ_BACK + ahead + (wow_part + flutter_part).clamp(-ahead, ahead);
        }
        out
    }
}

/// How many frames the line is read over between its samples, and how many places between two
/// frames the reader's weights are worked out for.
pub(super) const TAPS: usize = 16;
const PLACES: usize = 256;
/// The Kaiser window's shape.
const KAISER_BETA: f64 = 5.0;

/// Reading the line between its samples by a windowed sinc 16 frames wide, rather than a cubic:
/// a four-point cubic half way between frames is 3 dB down at 16 kHz at 48 kHz, which would dull
/// the top whenever the transport moves at all. This is within 0.05 dB to 18 kHz at any place
/// between, and exact on a frame.
#[derive(Debug, Clone)]
pub(super) struct Reader {
    weights: Vec<[f32; TAPS]>,
}

impl Reader {
    pub(super) fn new() -> Self {
        // The modified Bessel function of the first kind, order 0, by its series.
        let bessel = |x: f64| {
            let (mut sum, mut term) = (1.0, 1.0);
            for k in 1..40 {
                term *= (x / (2.0 * f64::from(k))).powi(2);
                sum += term;
            }
            sum
        };
        let half = (TAPS / 2) as f64;
        let weights = (0..=PLACES)
            .map(|place| {
                let between = place as f64 / PLACES as f64;
                let mut row = [0.0_f64; TAPS];
                for (tap, weight) in row.iter_mut().enumerate() {
                    // How far this frame is from the point read, in frames.
                    let t = tap as f64 - (half - 1.0) - between;
                    // On a frame the sinc is one there and nothing elsewhere, exactly, so a still
                    // transport reads its line to the bit.
                    let sinc = if t.abs() < 1e-12 {
                        1.0
                    } else if between == 0.0 {
                        0.0
                    } else {
                        (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
                    };
                    let edge = (1.0 - (t / half).powi(2)).max(0.0);
                    *weight = sinc * bessel(KAISER_BETA * edge.sqrt()) / bessel(KAISER_BETA);
                }
                let sum: f64 = row.iter().sum();
                row.map(|weight| (weight / sum) as f32)
            })
            .collect();
        Self { weights }
    }

    /// A side's line `delay` frames back, which must be at least `TAPS / 2 - 1`.
    #[inline]
    pub(super) fn read(&self, line: &StereoLine, side: usize, delay: f32) -> f32 {
        let whole = delay as usize;
        let place = (delay - whole as f32) * PLACES as f32;
        let at = (place as usize).min(PLACES - 1);
        let part = place - at as f32;
        let (low, high) = (&self.weights[at], &self.weights[at + 1]);
        let first = whole + 1 - TAPS / 2;
        let mut sum = 0.0;
        for tap in 0..TAPS {
            let weight = low[tap] + (high[tap] - low[tap]) * part;
            sum += weight * line.at(side, first + tap);
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// The speed's deviation in cents, frame by frame, from how the line's length moves.
    fn cents(wow: f32, flutter: f32, ips: f32, seconds: f32) -> Vec<f32> {
        let mut transport = Transport::new(RATE);
        let frames = (seconds * RATE as f32) as usize;
        let delays: Vec<f32> = (0..frames)
            .map(|_| transport.next(wow, flutter, ips)[0])
            .collect();
        delays
            .windows(2)
            .skip(RATE as usize)
            .map(|pair| (pair[1] - pair[0]) / CENT)
            .collect()
    }

    fn rms(values: &[f32]) -> f32 {
        let mean = values.iter().sum::<f32>() / values.len() as f32;
        (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
    }

    #[test]
    fn at_00_it_never_moves() {
        let mut transport = Transport::new(RATE);
        for _ in 0..RATE {
            assert_eq!(transport.next(0.0, 0.0, 15.0), [READ_BACK; 2]);
        }
    }

    #[test]
    fn each_knob_reads_evenly_in_cents_up_to_satins_most() {
        for (wow, flutter) in [(0.0, 1.0), (0.0, 0.5), (1.0, 0.0), (0.5, 0.0)] {
            let found = rms(&cents(wow, flutter, 15.0, 60.0));
            let wanted = MOST_WOW_CENTS * wow + MOST_FLUTTER_CENTS * flutter;
            assert!(
                (found / wanted - 1.0).abs() < 0.15,
                "{wow} {flutter}: {found}"
            );
        }
    }

    #[test]
    fn a_frame_is_read_exactly_and_between_frames_the_top_stays() {
        let reader = Reader::new();
        let mut line = StereoLine::new(64);
        for frame in 0..40 {
            line.advance();
            line.write(0, frame as f32);
        }
        assert_eq!(reader.read(&line, 0, 9.0).to_bits(), 30.0_f32.to_bits());
        // A 16 kHz sine read half way between frames keeps its level.
        let mut line = StereoLine::new(64);
        let (mut read, mut written) = (0.0_f64, 0.0_f64);
        for frame in 0..4800 {
            line.advance();
            let x = (std::f32::consts::TAU * 16_000.0 * frame as f32 / RATE as f32).sin();
            line.write(0, x);
            if frame > 100 {
                read += f64::from(reader.read(&line, 0, 10.5)).powi(2);
                written += f64::from(x).powi(2);
            }
        }
        let db = 10.0 * (read / written).log10();
        assert!(db.abs() < 0.05, "{db}");
    }
}
