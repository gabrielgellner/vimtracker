//! The Space Echo's own spring tank, fitted to Delay TAPE-201's by its output alone (decision
//! 450). This program's `Spring`, ChowDSP's model, rings in one loop through its all-passes, so a
//! sound comes round about every quarter of a second, a flutter like a snare's rattle, and its
//! all-passes leave comb lines across the whole band. TAPE-201's has neither: a drip that rises,
//! its lows first and its highs about 10 ms after, then a smooth, dark tail that falls quickly at
//! first and then rings on, quieter and slower, for seconds. So this one makes the drip apart from
//! the tail. A chain of stretched all-passes, each a frame's all-pass with its delay taken out to
//! `CHIRP_FRAMES` frames, as Parker and Välimäki model a spring's dispersion, holds highs back
//! more than lows up to where the stretch turns them round, about 3.4 kHz. Two feedback delay
//! networks of eight lines make the tail, with no one loop to come round: a short one that falls
//! in about 3.7 s, and a long, quiet one that rings on for about 8 s at the bottom and dies
//! sooner above 1 kHz, the distant end of the tail. TAPE-201's drip is muted and dull, so the
//! chirp is heard quietly and through a low-pass, and the networks hear it only after six short
//! all-passes have smeared it, or each line would bring the chirp back whole, a "whip" every
//! 30 to 180 ms. A band-pass, a steep high-pass at 156 Hz with a little lift over it and a lift
//! at 750 Hz then shape both as TAPE-201's is shaped. Each side is a tank of its own, as
//! TAPE-201's are, so a sound on the left rings on the left only and a sound in the middle rings
//! the same on both. It's linear but for a soft hold on what goes in.
//!
//! `Spring` itself stays as it is, the full spring reverb on its own.

use std::f32::consts::TAU;

use crate::dsp::blocks::biquad::Biquad;
use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::line::Lines;

/// Where every length here is in frames.
const TUNING_RATE: f32 = 48_000.0;
/// How long before the drip starts, in seconds: TAPE-201's lows peak 58 ms after a click.
const PRE_SECONDS: f32 = 0.047;
/// The chirp: how many stretched all-passes, each how many frames long at the tuning rate, and
/// their coefficient. Their delay at 1, 2 and 3 kHz is 6, 11 and 25 ms, and 29 at 3.4 kHz,
/// where it turns.
const CHIRP_STAGES: usize = 85;
const CHIRP_FRAMES: f32 = 7.0;
const CHIRP_COEFFICIENT: f32 = 0.4;
/// The smearing in front of the tail: Schroeder all-passes `(g + z⁻ᴷ) / (1 + g z⁻ᴷ)`, `K` in
/// frames at the tuning rate, 2.4 to 11.6 ms.
const SMEAR_FRAMES: [f32; 6] = [113.0, 167.0, 241.0, 337.0, 431.0, 557.0];
const SMEAR_COEFFICIENT: f32 = 0.7;
/// The short network: its lines, in frames at the tuning rate, all prime, 29 to 65 ms; how long
/// it rings at its bottom and at `HIGH_HZ`, in seconds, through a one-pole on each line as Jot's
/// network has; and how much of it is heard.
const LINES: [f32; 8] = [
    1381.0, 1571.0, 1789.0, 2017.0, 2281.0, 2521.0, 2797.0, 3137.0,
];
const LOW_T60: f32 = 3.7;
const HIGH_T60: f32 = 2.48;
const HIGH_HZ: f32 = 3260.0;
const TAIL: f32 = 0.1355;
/// The long network, the tail's distant end: its lines, all prime, 92 to 176 ms; its T60s, at
/// the bottom and at `LATE_HIGH_HZ`; and how much of it is heard.
const LATE_LINES: [f32; 8] = [
    4409.0, 4951.0, 5503.0, 6067.0, 6619.0, 7213.0, 7817.0, 8461.0,
];
const LATE_LOW_T60: f32 = 8.1;
const LATE_HIGH_T60: f32 = 2.77;
const LATE_HIGH_HZ: f32 = 1360.0;
const LATE: f32 = 0.0612;
/// How much of the drip is heard, and its one-pole low-pass.
const DRIP: f32 = 0.2;
const DRIP_LOW_PASS_HZ: f32 = 2000.0;
/// The band-pass: a third-order Butterworth high-pass and a fourth-order low-pass; a fourth-order
/// high-pass under it, its second section's Q raised from Butterworth's 1.31 so it lifts a
/// little over 156 Hz, as TAPE-201's falls 40 dB from 200 to 100 Hz yet keeps its 125 Hz octave;
/// and the lift.
const HIGH_PASS_HZ: f32 = 400.0;
const LOW_PASS_HZ: f32 = 1700.0;
const BOTTOM_HZ: f32 = 156.0;
const BOTTOM_QS: [f32; 2] = [0.541_196, 1.94];
const LIFT: (f32, f32, f32) = (750.0, 1.4, 6.5);
/// What goes in is held softly under this, so the tank's out is bounded and nothing near a
/// sound's level is bent: `C tanh(x / C)`, with anything huge or a NaN held first.
const HOLD: f32 = 4.0;
const LOUDEST_IN: f32 = HOLD * 16.0;

/// Which way round each line hears what comes in and is heard, so the lines don't start alike.
const IN_SIGNS: [f32; 8] = [1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const OUT_SIGNS: [f32; 8] = [1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0];

/// One side's feedback delay network: eight lines mixed by a Householder matrix, each through
/// Jot's one-pole `g (1 - p) / (1 - p z⁻¹)`, `g` for the low T60 and `p` so the gain at the high
/// frequency is the high T60's. The lines are one after another in `buffer`, each exactly its
/// length.
#[derive(Debug, Clone)]
struct Network {
    buffer: Vec<f32>,
    starts: [usize; 8],
    lengths: [usize; 8],
    at: [usize; 8],
    keep: [f32; 8],
    pole: [f32; 8],
    damped: [f32; 8],
}

impl Network {
    fn new(frames: [f32; 8], rate: u32, (low_t60, high_t60, high_hz): (f32, f32, f32)) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let lengths = frames.map(|frames| ((frames * scale).round() as usize).max(1));
        let mut starts = [0; 8];
        let mut total = 0;
        for (start, length) in starts.iter_mut().zip(lengths) {
            *start = total;
            total += length;
        }
        let omega = TAU * high_hz.min(rate as f32 * 0.45) / rate as f32;
        let mut keep = [0.0; 8];
        let mut pole = [0.0; 8];
        for (line, frames) in lengths.iter().enumerate() {
            let dies = |t60: f32| 0.001_f32.powf(*frames as f32 / (t60 * rate as f32));
            let (low, high) = (dies(low_t60), dies(high_t60));
            let r = (high / low).powi(2);
            let (a, b) = (1.0 - r, -2.0 * (1.0 - r * omega.cos()));
            keep[line] = low;
            pole[line] = (-b - (b * b - 4.0 * a * a).sqrt()) / (2.0 * a);
        }
        Self {
            buffer: vec![0.0; total],
            starts,
            lengths,
            at: [0; 8],
            keep,
            pole,
            damped: [0.0; 8],
        }
    }

    fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.damped = [0.0; 8];
    }

    /// Feed one sample in and get the tail out: what each line gives, through its one-pole,
    /// mixed and written back with what comes in.
    #[inline]
    fn process(&mut self, fed: f32) -> f32 {
        let mut read = [0.0; 8];
        for (line, sample) in read.iter_mut().enumerate() {
            let back = self.buffer[self.starts[line] + self.at[line]];
            let pole = self.pole[line];
            self.damped[line] = self.keep[line] * (1.0 - pole) * back + pole * self.damped[line];
            *sample = self.damped[line];
        }
        let mixed = read.iter().sum::<f32>() * (2.0 / 8.0);
        let mut tail = 0.0;
        for (line, sample) in read.iter().enumerate() {
            tail += sample * OUT_SIGNS[line];
            self.buffer[self.starts[line] + self.at[line]] = sample - mixed + fed * IN_SIGNS[line];
            self.at[line] = if self.at[line] + 1 == self.lengths[line] {
                0
            } else {
                self.at[line] + 1
            };
        }
        tail * 0.5
    }
}

/// The tank, both sides.
#[derive(Debug, Clone)]
pub(crate) struct Tank {
    pre: Lines<2>,
    pre_frames: usize,
    /// Each stretched all-pass's line, side by side, `chirp_frames` a stage.
    chirp: Vec<[f32; 2]>,
    chirp_frames: usize,
    chirp_at: usize,
    /// The smearing all-passes' lines, one after another in `smear`, each `smear_frames` long.
    smear: Vec<[f32; 2]>,
    smear_frames: [usize; 6],
    smear_at: [usize; 6],
    /// Each side's short and long network.
    tail: [Network; 2],
    late: [Network; 2],
    one_pole_high: f32,
    one_pole_state: [f32; 2],
    drip_low: f32,
    drip_state: [f32; 2],
    high_pass: Biquad,
    low_pass: [Biquad; 2],
    bottom: [Biquad; 2],
    lift: Biquad,
    states: [[[f32; 2]; 6]; 2],
}

impl Tank {
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let pre_frames = ((PRE_SECONDS * rate as f32) as usize).max(1);
        let chirp_frames = ((CHIRP_FRAMES * scale).round() as usize).max(1);
        let smear_frames = SMEAR_FRAMES.map(|frames| ((frames * scale).round() as usize).max(1));
        let rate_f = rate as f32;
        let tan = (std::f32::consts::PI * HIGH_PASS_HZ / rate_f).tan();
        Self {
            pre: Lines::new(pre_frames + 1),
            pre_frames,
            chirp: vec![[0.0; 2]; CHIRP_STAGES * chirp_frames],
            chirp_frames,
            chirp_at: 0,
            smear: vec![[0.0; 2]; smear_frames.iter().sum()],
            smear_frames,
            smear_at: [0; 6],
            tail: std::array::from_fn(|_| Network::new(LINES, rate, (LOW_T60, HIGH_T60, HIGH_HZ))),
            late: std::array::from_fn(|_| {
                Network::new(
                    LATE_LINES,
                    rate,
                    (LATE_LOW_T60, LATE_HIGH_T60, LATE_HIGH_HZ),
                )
            }),
            one_pole_high: tan / (1.0 + tan),
            one_pole_state: [0.0; 2],
            drip_low: 1.0 - (-TAU * DRIP_LOW_PASS_HZ / rate_f).exp(),
            drip_state: [0.0; 2],
            high_pass: Biquad::high_pass(HIGH_PASS_HZ, 1.0, rate_f),
            low_pass: [
                Biquad::low_pass(LOW_PASS_HZ, 0.541_196, rate_f),
                Biquad::low_pass(LOW_PASS_HZ, 1.306_563, rate_f),
            ],
            bottom: BOTTOM_QS.map(|q| Biquad::high_pass(BOTTOM_HZ, q, rate_f)),
            lift: Biquad::peak(LIFT.0, LIFT.1, LIFT.2, rate_f),
            states: [[[0.0; 2]; 6]; 2],
        }
    }

    /// Forget what's in it.
    pub(crate) fn clear(&mut self) {
        self.pre.clear();
        self.chirp.fill([0.0; 2]);
        self.smear.fill([0.0; 2]);
        self.drip_state = [0.0; 2];
        self.tail.iter_mut().for_each(Network::clear);
        self.late.iter_mut().for_each(Network::clear);
        self.one_pole_state = [0.0; 2];
        self.states = [[[0.0; 2]; 6]; 2];
    }

    /// Feed one frame in and get the reverb out.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.pre.advance();
        let mut x = [0.0; 2];
        for side in 0..2 {
            let held = within(input[side], -LOUDEST_IN, LOUDEST_IN);
            self.pre.write(side, HOLD * (held / HOLD).tanh());
            x[side] = self.pre.at(side, self.pre_frames);
        }

        // The drip: each stretched all-pass `(a + z⁻ᴷ) / (1 + a z⁻ᴷ)`, kept as `v = x - a v[-K]`
        // and `y = a v + v[-K]`.
        let a = CHIRP_COEFFICIENT;
        let at = self.chirp_at;
        for stage in self.chirp.chunks_exact_mut(self.chirp_frames) {
            let held = stage[at];
            for side in 0..2 {
                let v = x[side] - a * held[side];
                x[side] = a * v + held[side];
                stage[at][side] = v;
            }
        }
        self.chirp_at = (at + 1) % self.chirp_frames;

        // What the tail hears: the drip smeared, through each all-pass in turn.
        let mut fed = x;
        let g = SMEAR_COEFFICIENT;
        let mut start = 0;
        for (frames, at) in self.smear_frames.iter().zip(self.smear_at.iter_mut()) {
            let slot = &mut self.smear[start + *at];
            let held = *slot;
            for side in 0..2 {
                let v = fed[side] - g * held[side];
                fed[side] = g * v + held[side];
                slot[side] = v;
            }
            *at = (*at + 1) % frames;
            start += frames;
        }

        let mut out = [0.0; 2];
        for side in 0..2 {
            let tail = self.tail[side].process(fed[side]);
            let late = self.late[side].process(fed[side]);
            let drip = &mut self.drip_state[side];
            *drip += self.drip_low * (x[side] - *drip);
            let wet = *drip * DRIP + tail * TAIL + late * LATE;
            // The first-order high-pass, then the second-order one: third-order Butterworth.
            let state = &mut self.one_pole_state[side];
            let v = (wet - *state) * self.one_pole_high;
            let low = v + *state;
            *state = low + v;
            let states = &mut self.states[side];
            let y = self.high_pass.process(&mut states[0], wet - low);
            let y = self.low_pass[0].process(&mut states[1], y);
            let y = self.low_pass[1].process(&mut states[2], y);
            let y = self.bottom[0].process(&mut states[3], y);
            let y = self.bottom[1].process(&mut states[4], y);
            out[side] = self.lift.process(&mut states[5], y);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// When a short burst of a sine comes out loudest, in ms.
    fn loudest_ms(hertz: f32) -> f32 {
        let mut tank = Tank::new(RATE);
        let length = (RATE / 200) as usize;
        let (mut most, mut when) = (0.0, 0);
        let mut energy = 0.0;
        for frame in 0..(RATE / 5) as usize {
            let x = if frame < length {
                let window = (std::f32::consts::PI * frame as f32 / length as f32).sin();
                0.5 * window * window * (TAU * hertz * frame as f32 / RATE as f32).sin()
            } else {
                0.0
            };
            let [out, _] = tank.process([x, x]);
            energy = energy * 0.98 + out * out;
            if energy > most {
                (most, when) = (energy, frame);
            }
        }
        when as f32 * 1000.0 / RATE as f32
    }

    #[test]
    fn its_drip_rises_its_highs_after_its_lows() {
        let (low, high) = (loudest_ms(700.0), loudest_ms(3000.0));
        assert!((45.0..65.0).contains(&low), "{low}");
        assert!(high - low > 5.0, "{low} {high}");
    }

    #[test]
    fn its_drip_is_muted_under_its_tail_and_never_comes_back_whole() {
        // A click: the loudest 5 ms of the drip, 40 to 80 ms after it, against the median 5 ms of
        // the tail from 80 to 300 ms, and the tail's loudest 5 ms against that median, in dB.
        let mut tank = Tank::new(RATE);
        let window = (RATE / 200) as usize;
        let mut levels = Vec::new();
        let mut sum = 0.0;
        for frame in 0..(RATE * 3 / 10) as usize {
            let x = if frame == 0 { 0.5 } else { 0.0 };
            let [left, _] = tank.process([x, x]);
            sum += left * left;
            if frame % window == window - 1 {
                levels.push(10.0 * (sum + 1e-30).log10());
                sum = 0.0;
            }
        }
        let loudest = |part: &[f32]| part.iter().copied().fold(f32::MIN, f32::max);
        let drip = loudest(&levels[8..16]);
        let mut tail = levels[16..].to_vec();
        tail.sort_by(f32::total_cmp);
        let median = tail[tail.len() / 2];
        let tail_most = loudest(&tail);
        assert!(drip - median < 12.0, "{}", drip - median);
        assert!(tail_most - median < 6.0, "{}", tail_most - median);
    }

    #[test]
    fn its_tail_dies_away_smoothly_with_no_loop_coming_round() {
        let mut tank = Tank::new(RATE);
        let window = (RATE / 100) as usize;
        let mut levels = Vec::new();
        let mut sum = 0.0;
        for frame in 0..RATE as usize * 6 {
            let x = if frame == 0 { 0.5 } else { 0.0 };
            let [left, right] = tank.process([x, x]);
            sum += left * left + right * right;
            if frame % window == window - 1 {
                levels.push(10.0 * (sum + 1e-30).log10());
                sum = 0.0;
            }
        }
        // From 0.2 s to 1.2 s, each 10 ms against the straight line through them, in dB: a loop
        // coming round every quarter second would stand out by several dB.
        let part = &levels[20..120];
        let n = part.len() as f32;
        let mean_t = (n - 1.0) / 2.0;
        let mean = part.iter().sum::<f32>() / n;
        let slope = part
            .iter()
            .enumerate()
            .map(|(t, l)| (t as f32 - mean_t) * (l - mean))
            .sum::<f32>()
            / part
                .iter()
                .enumerate()
                .map(|(t, _)| (t as f32 - mean_t).powi(2))
                .sum::<f32>();
        let wander = part
            .iter()
            .enumerate()
            .map(|(t, l)| (l - mean - slope * (t as f32 - mean_t)).powi(2))
            .sum::<f32>()
            / n;
        assert!(wander.sqrt() < 1.5, "{}", wander.sqrt());
        // It falls about 60 dB in four seconds.
        let t60 = -60.0 / (slope * 100.0);
        assert!((3.0..5.5).contains(&t60), "{t60}");
        assert!(levels.iter().all(|l| l.is_finite()));
    }

    #[test]
    fn its_tail_falls_quickly_then_rings_on_slower() {
        // A click's 100 ms levels: how fast it falls from 0.3 to 1.5 s against from 3 to 6 s, in
        // dB a second, as TAPE-201's 250 Hz falls in 4.6 s early and 6.6 s late.
        let mut tank = Tank::new(RATE);
        let window = (RATE / 10) as usize;
        let mut levels = Vec::new();
        let mut sum = 0.0_f64;
        for frame in 0..RATE as usize * 6 {
            let x = if frame == 0 { 0.5 } else { 0.0 };
            let [left, _] = tank.process([x, x]);
            sum += f64::from(left * left);
            if frame % window == window - 1 {
                levels.push(10.0 * (sum + 1e-30).log10());
                sum = 0.0;
            }
        }
        let early = (levels[3] - levels[15]) / 1.2;
        let late = (levels[30] - levels[59]) / 2.9;
        assert!(early > late * 1.15, "{early} {late}");
        assert!(late > 60.0 / 10.0, "{late}");
    }

    #[test]
    fn each_side_is_a_tank_of_its_own() {
        let mut tank = Tank::new(RATE);
        let mut left = 0.0;
        for frame in 0..RATE as usize {
            let x = if frame == 0 { 0.5 } else { 0.0 };
            let [l, r] = tank.process([x, 0.0]);
            left += l * l;
            assert!(r == 0.0, "{r}");
        }
        assert!(left > 1e-6, "{left}");
        let mut tank = Tank::new(RATE);
        for frame in 0..RATE as usize {
            let x = if frame == 0 { 0.5 } else { 0.0 };
            let [l, r] = tank.process([x, x]);
            assert!(l == r, "{l} {r}");
        }
    }

    #[test]
    fn it_holds_anything_huge_and_is_silent_after_clearing() {
        let mut tank = Tank::new(44_100);
        for frame in 0..44_100 {
            let x = if frame % 2 == 0 { 1e6 } else { f32::NAN };
            let out = tank.process([x, -x]);
            assert!(
                out.iter().all(|s| s.is_finite() && s.abs() < 64.0),
                "{out:?}"
            );
        }
        tank.clear();
        assert!((0..1000).all(|_| tank.process([0.0; 2]).iter().all(|s| s.abs() < 1e-30)));
    }
}
