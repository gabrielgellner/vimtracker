//! Clouds' reverb as the module has it: the Griesinger topology, four diffusers and a figure of
//! eight of two allpasses and a delay a side, its output read where each side's allpasses end,
//! with its line lengths scaled from 32 kHz to the rate it runs at. Clouds tunes it block by block
//! as the module does. The `Reverb` slot effect began as this tank and has gone its own way
//! (decision 447); this one stays as it was, bit for bit, so Clouds sounds as it did.
//!
//! Adapted from `clouds/dsp/fx/reverb.h` and `fx_engine.h`:
//!
//! Copyright 2014 Emilie Gillet.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction,
//! including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
//! NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
//! OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use crate::dsp::blocks::line::SharedLines;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};

/// The rate the line lengths and damping are written for.
const TUNING_RATE: f32 = 32_000.0;
/// How fast the two slow wobbles go, in hertz.
const WOBBLE_HERTZ: [f64; 2] = [0.5, 0.3];
/// Every line's length at the tuning rate: the four diffusers, then each loop's two allpasses and
/// its delay.
const LENGTHS: [usize; 10] = [113, 162, 241, 399, 1653, 2038, 3411, 1913, 1663, 4782];
/// Where the second loop's delay is read, at the tuning rate, and how far its wobble moves it.
const SWING: [f32; 2] = [4680.0, 100.0];
/// What each trip round its loops keeps and its damping before Clouds first tunes it: what the
/// `Reverb` slot effect it once was kept at its own start, a 2 s tail with its loops counted 1.4
/// times their lines and `Damp` at `55`, so a tank that plays before its first tuning starts where
/// it always has.
fn first_kept(rate: u32) -> [f32; 2] {
    let seconds = vt_model::insert::reverb_decay_seconds(f32::from(0x60_u8));
    let scale = rate as f32 / TUNING_RATE;
    let line = |at: usize| (LENGTHS[at] as f32 * scale) as usize as f32;
    [
        line(4) + line(5) + line(6) - 1.0,
        line(7) + line(8) + SWING[0] * scale,
    ]
    .map(|frames| 0.001_f32.powf(frames * 1.4 / (seconds * rate as f32)))
}

fn first_damping(rate_ratio: f32) -> f32 {
    let at_tuning_rate = 1.0 - f32::from(0x55_u8) / 255.0 * (1.0 - 0.1);
    1.0 - (1.0 - at_tuning_rate).powf(rate_ratio)
}

/// One delay line's place in the shared buffer.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    length: usize,
}

/// Clouds' stereo reverb.
#[derive(Debug, Clone)]
pub(crate) struct CloudsReverb {
    lines: SharedLines,
    diffusers: [Line; 4],
    /// Each side's loop: two allpasses and a delay.
    loops: [[Line; 3]; 2],
    /// Where the smear in the first diffuser reads and writes, and how far it moves.
    smear: [f32; 3],
    /// Where the second loop's delay reads, and how far it moves.
    swing: [f32; 2],
    damped: [f32; 2],
    wobble: [f32; 2],
    /// What each half of the loop keeps each time round.
    decay: [Smoothed; 2],
    damping: Smoothed,
    /// How much the allpasses spread what goes through them, and how much of what goes in reaches
    /// the lines, as Clouds tunes them.
    diffusion: f32,
    input_gain: f32,
    smoothing: f32,
    /// What damping at the tuning rate takes to be the same at this one.
    rate_ratio: f32,
    rate: u32,
}

impl CloudsReverb {
    /// Make one for a sample rate.
    pub(crate) fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let mut base = 0;
        let lines = LENGTHS.map(|length| {
            let line = Line {
                base,
                length: (length as f32 * scale) as usize,
            };
            base += line.length + 1;
            line
        });
        let size = base.next_power_of_two();
        let rate_ratio = TUNING_RATE / rate as f32;
        Self {
            lines: SharedLines::new(size),
            diffusers: [lines[0], lines[1], lines[2], lines[3]],
            loops: [
                [lines[4], lines[5], lines[6]],
                [lines[7], lines[8], lines[9]],
            ],
            smear: [50.0 * scale, 100.0 * scale, 45.0 * scale],
            swing: SWING.map(|frames| frames * scale),
            damped: [0.0; 2],
            wobble: [1.0; 2],
            decay: first_kept(rate).map(Smoothed::at),
            damping: Smoothed::at(first_damping(rate_ratio)),
            diffusion: 0.625,
            input_gain: 0.25,
            smoothing: coefficient(rate),
            rate_ratio,
            rate,
        }
    }

    /// Forget what's in it.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.damped = [0.0; 2];
    }

    /// Move the wobbles to where they are this many frames in. They go by the frame rather than
    /// by what the reverb has done, so one that's slept is where it would have been.
    pub(crate) fn keep_time(&mut self, frames: u64) {
        let seconds = frames as f64 / f64::from(self.rate);
        self.wobble =
            WOBBLE_HERTZ.map(|hertz| (std::f64::consts::TAU * hertz * seconds).cos() as f32);
    }

    /// Jump to the tuning, for starting afresh.
    pub(crate) fn settle(&mut self) {
        for decay in &mut self.decay {
            decay.settle();
        }
        self.damping.settle();
    }

    /// Tune the tank as Clouds tunes its own reverb, block by block: what each trip round its loops
    /// keeps, its damping as a one-pole's coefficient at 32 kHz, its allpasses' diffusion and how
    /// much of what goes in reaches the lines. The decay and damping glide there.
    pub(crate) fn tune(
        &mut self,
        decay: f32,
        damping_at_tuning_rate: f32,
        diffusion: f32,
        gain: f32,
    ) {
        for half in &mut self.decay {
            half.target = decay;
        }
        self.damping.target = 1.0 - (1.0 - damping_at_tuning_rate).powf(self.rate_ratio);
        self.diffusion = diffusion;
        self.input_gain = gain;
    }

    /// Feed one frame in, both sides together, and get the wet signal out.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let incoming = input[0] + input[1];
        let diffusion = self.diffusion;
        let [one, two] = &mut self.decay;
        let decay = [one.next(self.smoothing), two.next(self.smoothing)];
        let damping = self.damping.next(self.smoothing);
        let lines = &mut self.lines;
        lines.advance();

        // Smear the first allpass inside itself. In Clouds this reads below 0, out of the line and
        // into old memory, which would go round again whatever its decay is, so here it stays in.
        let [first, _, _, _] = self.diffusers;
        let [read_at, write_at, moves] = self.smear;
        let smeared = lines.between(first.base, read_at + moves * self.wobble[0]);
        lines.write(first.base, write_at as usize, smeared);

        let mut diffused = incoming * self.input_gain;
        for line in self.diffusers {
            diffused = lines.allpass(line.base, line.length, diffused, diffusion);
        }

        let [[one_a, one_b, one_delay], [two_a, two_b, two_delay]] = self.loops;
        let [swing_at, swing_moves] = self.swing;
        let from_two = lines.between(two_delay.base, swing_at + swing_moves * self.wobble[1]);
        let mut left = diffused + from_two * decay[1];
        self.damped[0] += damping * (left - self.damped[0]);
        left = self.damped[0];
        left = lines.allpass(one_a.base, one_a.length, left, -diffusion);
        left = lines.allpass(one_b.base, one_b.length, left, diffusion);
        lines.write(one_delay.base, 0, left);

        let from_one = lines.read(one_delay.base, one_delay.length - 1);
        let mut right = diffused + from_one * decay[0];
        self.damped[1] += damping * (right - self.damped[1]);
        right = self.damped[1];
        right = lines.allpass(two_a.base, two_a.length, right, diffusion);
        right = lines.allpass(two_b.base, two_b.length, right, -diffusion);
        lines.write(two_delay.base, 0, right);

        [left * 2.0, right * 2.0]
    }
}
