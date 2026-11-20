//! The Space Echo's own spring tank, fitted to Delay TAPE-201's by its output alone (decision
//! 450). This program's `Spring`, ChowDSP's model, rings in one loop through its all-passes, so a
//! sound comes round about every quarter of a second, a flutter like a snare's rattle, and its
//! all-passes leave comb lines across the whole band. TAPE-201's has neither: a drip that rises,
//! its lows first and its highs about 10 ms after, then a smooth, dark tail. So this one makes
//! the drip apart from the tail. A chain of stretched all-passes, each a frame's all-pass with its
//! delay taken out to `CHIRP_FRAMES` frames, as Parker and Välimäki model a spring's dispersion,
//! holds highs back more than lows up to where the stretch turns them round, about 3.4 kHz; and a
//! feedback delay network of eight lines, each losing more of its highs than its lows, makes the
//! tail, with no one loop to come round. TAPE-201's drip is muted and dull, so the chirp is heard
//! quietly and through a low-pass, and the network hears it only after six short all-passes have
//! smeared it, or each line would bring the chirp back whole, a "whip" every 35 to 80 ms. A
//! band-pass, a steep high-pass at 180 Hz and a lift at 750 Hz then shape both as TAPE-201's is
//! shaped. It's linear but for a soft hold on what goes in.
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
/// The tail's lines, in frames at the tuning rate, none a multiple of another.
const LINES: [f32; 8] = [
    1693.0, 1931.0, 2203.0, 2477.0, 2797.0, 3089.0, 3433.0, 3851.0,
];
/// How long the tail rings at its bottom and at `HIGH_HZ`, in seconds, through a one-pole on each
/// line as Jot's network has.
const LOW_T60: f32 = 4.3;
const HIGH_T60: f32 = 2.5;
const HIGH_HZ: f32 = 3000.0;
/// How much of the drip and of the tail is heard, and the drip's one-pole low-pass.
const DRIP: f32 = 0.2;
const TAIL: f32 = 0.42;
const DRIP_LOW_PASS_HZ: f32 = 2000.0;
/// The band-pass: a third-order Butterworth high-pass and a fourth-order low-pass; a fourth-order
/// Butterworth high-pass under it, as TAPE-201's falls 40 dB from 200 to 100 Hz; and the lift.
const HIGH_PASS_HZ: f32 = 400.0;
const LOW_PASS_HZ: f32 = 1700.0;
const BOTTOM_HZ: f32 = 180.0;
const LIFT: (f32, f32, f32) = (750.0, 1.4, 6.5);
/// What goes in is held softly under this, so the tank's out is bounded and nothing near a
/// sound's level is bent: `C tanh(x / C)`, with anything huge or a NaN held first.
const HOLD: f32 = 4.0;
const LOUDEST_IN: f32 = HOLD * 16.0;

/// The tail's lines' signs on the way out, so the sides aren't the same.
const SIGNS: [f32; 4] = [1.0, -1.0, 1.0, -1.0];

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
    lines: Lines<8>,
    lengths: [usize; 8],
    /// Each line's one-pole: its gain at DC, its pole, and its state.
    keep: [f32; 8],
    pole: [f32; 8],
    damped: [f32; 8],
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
        let lengths = LINES.map(|frames| ((frames * scale).round() as usize).max(1));
        let longest = lengths.iter().copied().max().unwrap_or(1);
        // Jot's one-pole: `g (1 - p) / (1 - p z⁻¹)`, `g` for the low T60 and `p` so the gain at
        // `HIGH_HZ` is the high T60's.
        let omega = TAU * HIGH_HZ.min(rate as f32 * 0.45) / rate as f32;
        let mut keep = [0.0; 8];
        let mut pole = [0.0; 8];
        for (line, frames) in lengths.iter().enumerate() {
            let dies = |t60: f32| 0.001_f32.powf(*frames as f32 / (t60 * rate as f32));
            let (low, high) = (dies(LOW_T60), dies(HIGH_T60));
            let r = (high / low).powi(2);
            let (a, b) = (1.0 - r, -2.0 * (1.0 - r * omega.cos()));
            keep[line] = low;
            pole[line] = (-b - (b * b - 4.0 * a * a).sqrt()) / (2.0 * a);
        }
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
            lines: Lines::new(longest + 1),
            lengths,
            keep,
            pole,
            damped: [0.0; 8],
            one_pole_high: tan / (1.0 + tan),
            one_pole_state: [0.0; 2],
            drip_low: 1.0 - (-TAU * DRIP_LOW_PASS_HZ / rate_f).exp(),
            drip_state: [0.0; 2],
            high_pass: Biquad::high_pass(HIGH_PASS_HZ, 1.0, rate_f),
            low_pass: [
                Biquad::low_pass(LOW_PASS_HZ, 0.541_196, rate_f),
                Biquad::low_pass(LOW_PASS_HZ, 1.306_563, rate_f),
            ],
            bottom: [
                Biquad::high_pass(BOTTOM_HZ, 0.541_196, rate_f),
                Biquad::high_pass(BOTTOM_HZ, 1.306_563, rate_f),
            ],
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
        self.lines.clear();
        self.damped = [0.0; 8];
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

        // The tail: what each line gives, through its one-pole, mixed by a Householder matrix.
        self.lines.advance();
        let mut read = [0.0; 8];
        for (line, sample) in read.iter_mut().enumerate() {
            let back = self.lines.at(line, self.lengths[line]);
            let pole = self.pole[line];
            self.damped[line] = self.keep[line] * (1.0 - pole) * back + pole * self.damped[line];
            *sample = self.damped[line];
        }
        let mixed = read.iter().sum::<f32>() * (2.0 / 8.0);
        let mut tail = [0.0; 2];
        for (line, sample) in read.iter().enumerate() {
            let side = line / 4;
            tail[side] += sample * SIGNS[line % 4] * 0.5;
            self.lines.write(line, sample - mixed + fed[side]);
        }

        let mut out = [0.0; 2];
        for side in 0..2 {
            let drip = &mut self.drip_state[side];
            *drip += self.drip_low * (x[side] - *drip);
            let wet = *drip * DRIP + tail[side] * TAIL;
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
