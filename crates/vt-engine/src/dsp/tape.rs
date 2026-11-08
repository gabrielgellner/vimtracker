//! A tape machine, after Airwindows' ToTape9: the sound is driven onto the tape through a
//! noise-reduction style encoder that lifts its highs, wobbles with wow and flutter, is biased,
//! saturates, gets the heads' bump in the lows, and comes back off through the matching decoder
//! and a soft clip that holds it under about -0.5 dB. `Tone` tilts the encoder against the
//! decoder, so the highs come out brighter or darker.
//!
//! ToTape's own flutter is left out for the wow and flutter of Mutable Instruments' Beads: a short
//! line read between samples, whose length a slow and a fast sine wobble, with a slower one
//! drifting the slow one's speed and depth. Beads' delays are tuned at 48 kHz and scaled here to
//! the rate it runs at. Its low-pass after the line is left out, as `Tone` does that job.
//!
//! ToTape's input, output and dry/wet controls are the slot's own `Drive`, `Gain` and `Mix`. Its
//! `Shape` and head bump frequency are kept where they start, half way. Airwindows' noise for
//! dither and against denormals is left out, and so is the noise that moves the clip's level, so
//! the same sound always comes out the same.
//!
//! Adapted from `plugins/LinuxVST/src/ToTape9/ToTape9Proc.cpp` at
//! <https://github.com/airwindows/airwindows>, MIT:
//!
//! Copyright (c) 2018 Chris Johnson.
//!
//! And from `beads/dsp/fx/wow_and_flutter.h` in Mutable Instruments' eurorack, MIT:
//!
//! Copyright 2018 Emilie Gillet.
//!
//! Both under the same licence: permission is hereby granted, free of charge, to any person
//! obtaining a copy of this software and associated documentation files (the "Software"), to deal
//! in the Software without restriction, including without limitation the rights to use, copy,
//! modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
//! permit persons to whom the Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
//! NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
//! OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use std::f32::consts::{PI, TAU};

use vt_model::insert::{Tape as Settings, from_middle, tape_drive_db};

use crate::dsp::smoother::{Smoothed, coefficient};

/// The rate Airwindows' filters and slews are tuned at, and Beads' delays.
const AIRWINDOWS_RATE: f32 = 44_100.0;
const BEADS_RATE: f32 = 48_000.0;

/// How far the encoder's and decoder's one-poles move each frame at Airwindows' rate, with ToTape's
/// `Shape` half way.
const DUBLY_FREQUENCY: f32 = 0.5;
/// How much of the highs the encoder and decoder take now, and how much a frame later.
const ENCODE_HIGHS: (f32, f32) = (2.848, 1.152);
const DECODE_HIGHS: (f32, f32) = (2.628, 1.372);
/// `log10(256)`, which ToTape divides a natural log by to ease the encoder's level.
const DUBLY_EASE: f32 = 2.408_24;

/// How many slew limits the bias has in a row, each looser than the next by the golden ratio.
const BIAS_STAGES: usize = 9;
/// How much each slew limit lets the last sample fall back toward nothing.
const BIAS_LEAK: f32 = 0.975;
/// The most `Bias` overbiases at `FF`. At ToTape's full overbias the slews close and it falls
/// silent, so it stops short.
const MOST_OVERBIAS: f32 = 0.75;

/// The most the tiny hysteresis holds, and how much of it is added.
const HYSTERESIS_MOST: f32 = 0.011_449;
const HYSTERESIS_SHARE: f32 = 0.012;
const HYSTERESIS_LEAK: f32 = 0.999;

/// Where the saturation's series for a sine stops: past it, the series turns back.
const SATURATION_EDGE: f32 = 2.305_929;

/// The heads' bump, with ToTape's `HeadFrq` half way: two band-passes a sixteenth apart.
const BUMP_HZ: f32 = 68.75;
const BUMP_SPREAD: f32 = 0.9375;
const BUMP_Q: f32 = 0.618_034;
/// How hard the bump's integrator is pushed, and how much of it comes out, at full `Bump`.
const BUMP_DRIVE: f32 = 0.1;
const BUMP_MIX: f32 = 0.5;
/// How much the bump's integrator leaks, by its cube, at Airwindows' rate.
const BUMP_LEAK: f32 = 0.0618;

/// Where the clip after the tape starts, and the most it lets out.
const CLIP: f32 = 0.908_509_7;
const CLIP_MOST: f32 = 0.94;
/// How much a fast slew pulls the clip's ceiling down.
const CLIP_SLEW: f32 = 1.398_601_3;

/// Beads' wow and flutter at the tuning rate: how far each moves the line at its full amount,
/// here four times Beads', and how fast each goes.
const MOST_WOW_FRAMES: f32 = 4.0 * 105.0;
const MOST_FLUTTER_FRAMES: f32 = 4.0 * 17.0;
const WOW_HZ: f32 = 0.23;
const FLUTTER_HZ: f32 = 1.99;
/// How fast the drift that changes the wow goes as it starts, and how slowly and fast it can go.
const FIRST_DRIFT_HZ: f32 = 0.125;
const SLOWEST_DRIFT_HZ: f32 = 0.05;
const DRIFT_SPREAD_HZ: f32 = 0.2;
/// How many frames the line's length moves in a straight line to the next one worked out, as Beads
/// works it out once a block.
const CONTROL_FRAMES: u32 = 32;

/// A one-pole high-shelf of a sort, as ToTape's "Dubly" encoder and decoder have it: what's above
/// a one-pole is taken, its level eased, and added back in.
#[derive(Debug, Clone, Copy)]
struct Dubly {
    lows: f32,
    level: f32,
    last: f32,
}

impl Default for Dubly {
    fn default() -> Self {
        Self {
            lows: 0.0,
            level: 1.0,
            last: 0.0,
        }
    }
}

impl Dubly {
    #[inline]
    fn process(&mut self, sample: f32, frequency: f32, (now, later): (f32, f32), by: f32) -> f32 {
        self.lows += (sample - self.lows) * frequency;
        let highs = sample - self.lows;
        let part = (highs * now + self.last).clamp(-1.0, 1.0);
        self.last = highs * later;
        let level = part.abs();
        if level <= 0.0 {
            return sample;
        }
        let ease = (255.0 * level).ln_1p() / DUBLY_EASE;
        let eased = if ease > 0.0 { level / ease } else { level };
        self.level += (eased - self.level) * frequency;
        sample + part * self.level * by
    }
}

/// ToTape's averages of the last 32, 16, 8, 4 and 2 samples, one after another. How many of them
/// are used goes with the rate, so they darken about the same at any rate.
#[derive(Debug, Clone, Copy, Default)]
struct Averages {
    of32: [f32; 32],
    of16: [f32; 16],
    of8: [f32; 8],
    of4: [f32; 4],
    of2: [f32; 2],
}

/// Put a sample in, at a place that goes round, and get the average of all of them.
#[inline]
fn averaged<const N: usize>(samples: &mut [f32; N], at: usize, sample: f32) -> f32 {
    samples[at % N] = sample;
    samples.iter().sum::<f32>() / N as f32
}

impl Averages {
    #[inline]
    fn process(&mut self, sample: f32, at: usize, longest: usize) -> f32 {
        let mut dark = sample;
        if longest >= 32 {
            dark = averaged(&mut self.of32, at, dark);
        }
        if longest >= 16 {
            dark = averaged(&mut self.of16, at, dark);
        }
        if longest >= 8 {
            dark = averaged(&mut self.of8, at, dark);
        }
        if longest >= 4 {
            dark = averaged(&mut self.of4, at, dark);
        }
        averaged(&mut self.of2, at, dark)
    }
}

/// A band-pass, in the transposed direct form, as the heads' bump has two of.
#[derive(Debug, Clone, Copy, Default)]
struct BandPass {
    a0: f32,
    b1: f32,
    b2: f32,
}

impl BandPass {
    fn new(share_of_rate: f32) -> Self {
        let warped = (PI * share_of_rate).tan();
        let norm = 1.0 / (1.0 + warped / BUMP_Q + warped * warped);
        Self {
            a0: warped / BUMP_Q * norm,
            b1: 2.0 * (warped * warped - 1.0) * norm,
            b2: (1.0 - warped / BUMP_Q + warped * warped) * norm,
        }
    }

    #[inline]
    fn process(self, state: &mut [f32; 2], sample: f32) -> f32 {
        let out = sample * self.a0 + state[0];
        state[0] = -out * self.b1 + state[1];
        state[1] = -sample * self.a0 - out * self.b2;
        out
    }
}

/// One side's tape.
#[derive(Debug, Clone, Copy)]
struct Side {
    encode: Dubly,
    decode: Dubly,
    /// The last sample each slew limit let through, leaked a little.
    slews: [f32; BIAS_STAGES],
    hysteresis: f32,
    before: Averages,
    after: Averages,
    last_dark: f32,
    bump: f32,
    bump_states: [[f32; 2]; 2],
    /// The clip's last sample, which comes out a frame late, and whether the newest went over
    /// either way.
    last: f32,
    over: bool,
    under: bool,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            encode: Dubly::default(),
            decode: Dubly::default(),
            slews: [0.0; BIAS_STAGES],
            hysteresis: 0.0,
            before: Averages::default(),
            after: Averages::default(),
            last_dark: 0.0,
            bump: 0.0,
            bump_states: [[0.0; 2]; 2],
            last: 0.0,
            over: false,
            under: false,
        }
    }
}

/// What every side shares for a frame.
#[derive(Debug, Clone, Copy)]
struct Frame {
    /// How much the encoder adds and the decoder takes away.
    encode: f32,
    decode: f32,
    /// Whether it's biased, how far underbias makes a sample stick, and each slew limit.
    biased: bool,
    under: f32,
    limits: [f32; BIAS_STAGES],
    bump_drive: f32,
    bump_mix: f32,
    /// Where the averages are, and how many go into the longest.
    at: usize,
}

/// What a tape keeps for its rate.
#[derive(Debug, Clone, Copy)]
struct Fixed {
    /// How many times Airwindows' rate this is.
    scale: f32,
    longest: usize,
    frequency: f32,
    bump_leak: f32,
    bumps: [BandPass; 2],
    /// How many sides of the clip's newest sample it holds after going over, as ToTape's
    /// averaging of its intermediate samples leaves it.
    clip_lift: f32,
}

impl Side {
    #[inline]
    fn before_line(&mut self, sample: f32, frame: &Frame, fixed: &Fixed) -> f32 {
        self.encode
            .process(sample, fixed.frequency, ENCODE_HIGHS, frame.encode)
    }

    #[inline]
    fn after_line(&mut self, sample: f32, frame: &Frame, fixed: &Fixed) -> f32 {
        let mut sample = sample;
        // The bias: overbias limits how fast it can move, more at each stage, and underbias makes
        // it stick near where it was. Unbiased, the slews still follow, so biasing again is smooth.
        if frame.biased {
            for (slew, limit) in self.slews.iter_mut().zip(frame.limits) {
                let previous = *slew / BIAS_LEAK;
                if frame.under > 0.0 {
                    let stuck = (sample - previous).abs() / frame.under;
                    if stuck < 1.0 {
                        sample = sample * stuck + previous * (1.0 - stuck);
                    }
                }
                sample = sample.clamp(*slew - limit, *slew + limit);
                *slew = sample * BIAS_LEAK;
            }
        } else {
            self.slews.fill(sample * BIAS_LEAK);
        }

        // A tiny hysteresis, most on what's quiet.
        let apply = (1.0 - sample.abs()).powi(2) * HYSTERESIS_SHARE;
        self.hysteresis = (self.hysteresis + sample * sample.abs())
            .clamp(-HYSTERESIS_MOST, HYSTERESIS_MOST)
            * HYSTERESIS_LEAK;
        sample += self.hysteresis * apply;

        // What moves fast is darkened, before and after it saturates, by as much each time.
        let dark = self.before.process(sample, frame.at, fixed.longest);
        let slew = ((self.last_dark - sample).abs() * 0.12 * fixed.scale).min(1.0);
        // ToTape writes this `1 - (1 - a - a)`.
        let blend = 2.0 * slew;
        sample += (dark - sample) * blend;
        self.last_dark = dark;

        // The saturation: a sine, as the start of its series, up to where it turns back.
        let x = sample.clamp(-SATURATION_EDGE, SATURATION_EDGE);
        let squared = x * x;
        let mut power = x * squared;
        let mut saturated = x - power / 6.0;
        power *= squared;
        saturated += power / 69.0;
        power *= squared;
        saturated -= power / 2530.08;
        power *= squared;
        saturated += power / 224_985.6;
        power *= squared;
        saturated -= power / 9_979_200.0;
        let dark = self.after.process(saturated, frame.at, fixed.longest);
        sample = saturated + (dark - saturated) * blend;

        // The heads' bump: an integrator held back by its cube, through two band-passes.
        if frame.bump_mix > 0.0 {
            self.bump += sample * frame.bump_drive;
            self.bump -= self.bump * self.bump * self.bump * fixed.bump_leak;
            let [first, second] = &mut self.bump_states;
            let bumped = fixed.bumps[0].process(first, self.bump);
            sample += fixed.bumps[1].process(second, bumped) * frame.bump_mix;
        }

        sample = self
            .decode
            .process(sample, fixed.frequency, DECODE_HIGHS, frame.decode);
        self.clip(sample, fixed)
    }

    /// ToTape's ClipOnly3: a clip that eases into and out of going over, a frame late, under a
    /// ceiling that a fast slew pulls down.
    #[inline]
    fn clip(&mut self, sample: f32, fixed: &Fixed) -> f32 {
        let mut sample = sample;
        if self.over {
            self.last = if sample < self.last { CLIP } else { CLIP_MOST };
        }
        self.over = sample > CLIP;
        if self.over {
            sample = CLIP;
        }
        if self.under {
            self.last = if sample > self.last {
                -CLIP
            } else {
                -CLIP_MOST
            };
        }
        self.under = sample < -CLIP;
        if self.under {
            sample = -CLIP;
        }
        let slew = (self.last - sample).abs();
        let out = self.last;
        self.last = if self.over || self.under {
            sample * fixed.clip_lift
        } else {
            sample
        };
        let most = CLIP_MOST / (1.0 + slew * CLIP_SLEW);
        out.clamp(-most, most)
    }
}

/// Beads' wobbles, which the line's length follows.
#[derive(Debug, Clone, Copy)]
struct Wobble {
    wow: f32,
    flutter: f32,
    drift: f32,
    /// How far round the drift goes each frame, now.
    drift_step: f32,
    random: u32,
    /// How far back the line is read, how far that moves each frame, and how many frames until
    /// the next is worked out. Without one, the next is jumped to.
    delay: Option<f32>,
    slope: f32,
    countdown: u32,
}

impl Wobble {
    fn new(rate: u32) -> Self {
        Self {
            wow: 0.0,
            flutter: 0.0,
            drift: 0.0,
            drift_step: FIRST_DRIFT_HZ / rate as f32,
            random: 0x9E37_79B9,
            delay: None,
            slope: 0.0,
            countdown: 0,
        }
    }

    /// A number from 0 to 1 that's hard to guess, the same each time it starts.
    #[inline]
    fn random(&mut self) -> f32 {
        self.random ^= self.random << 13;
        self.random ^= self.random >> 17;
        self.random ^= self.random << 5;
        self.random as f32 / u32::MAX as f32
    }

    /// Where a sine from 0 to 1 is, after going a share of a turn.
    #[inline]
    fn turned(phase: &mut f32, by: f32) -> f32 {
        *phase += by;
        *phase -= phase.floor();
        0.5 + 0.5 * (*phase * TAU).sin()
    }

    /// How far back to read the line this frame, in frames, at an amount of wow and flutter in
    /// frames at their fullest.
    #[inline]
    fn next(&mut self, wow: f32, flutter: f32, rate: u32) -> f32 {
        if self.countdown == 0 {
            self.countdown = CONTROL_FRAMES;
            let frames = CONTROL_FRAMES as f32;
            let before = self.drift;
            let drift = Self::turned(&mut self.drift, self.drift_step * frames);
            if before < 0.25 && self.drift >= 0.25 {
                let hertz = SLOWEST_DRIFT_HZ + self.random() * DRIFT_SPREAD_HZ;
                self.drift_step = hertz / rate as f32;
            }
            let speed = 0.5 + 1.5 * drift * drift;
            let depth = 1.0 - 0.4 * drift;
            let slow = Self::turned(&mut self.wow, WOW_HZ / rate as f32 * frames * speed);
            let fast = Self::turned(&mut self.flutter, FLUTTER_HZ / rate as f32 * frames);
            let next = 1.0 + (slow * wow + fast * flutter) * depth;
            let now = *self.delay.get_or_insert(next);
            self.slope = (next - now) / frames;
        }
        self.countdown -= 1;
        let delay = self.delay.get_or_insert(1.0);
        *delay += self.slope;
        *delay
    }
}

/// A tape's state and line.
#[derive(Debug, Clone)]
pub struct Tape {
    sides: [Side; 2],
    lines: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    wobble: Wobble,
    at: usize,
    drive: Smoothed,
    /// ToTape's `Tilt`, from 0 to 1.
    tilt: Smoothed,
    /// From -1, all underbias, to the most overbias.
    bias: Smoothed,
    bump: Smoothed,
    /// How far wow and flutter move the line at most, in frames.
    wow: Smoothed,
    flutter: Smoothed,
    fixed: Fixed,
    rate: u32,
    smoothing: f32,
}

impl Tape {
    /// A line long enough for the most wow and flutter at this rate.
    pub fn new(rate: u32) -> Self {
        let scale = rate as f32 / AIRWINDOWS_RATE;
        let longest = 1.0 + (MOST_WOW_FRAMES + MOST_FLUTTER_FRAMES) * rate as f32 / BEADS_RATE;
        let size = (longest.ceil() as usize + 4).next_power_of_two();
        let spacing = scale.floor().clamp(1.0, 16.0);
        let bump = BUMP_HZ / rate as f32;
        let mut tape = Self {
            sides: [Side::default(); 2],
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            wobble: Wobble::new(rate),
            at: 0,
            drive: Smoothed::default(),
            tilt: Smoothed::default(),
            bias: Smoothed::default(),
            bump: Smoothed::default(),
            wow: Smoothed::default(),
            flutter: Smoothed::default(),
            fixed: Fixed {
                scale,
                longest: ((scale * 2.0).floor() as usize).clamp(2, 32),
                frequency: DUBLY_FREQUENCY / scale,
                bump_leak: BUMP_LEAK / scale.sqrt(),
                bumps: [BandPass::new(bump), BandPass::new(bump * BUMP_SPREAD)],
                clip_lift: (spacing + 1.0) / spacing,
            },
            rate,
            smoothing: coefficient(rate),
        };
        tape.follow(Settings::DEFAULT);
        tape.settle();
        tape
    }

    /// The longest anything stays in its line, in frames.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    /// How many quiet frames in a row mean nothing's left in it: its line, and a tenth of a second
    /// for the heads' bump to ring down.
    pub fn quiet_frames(&self) -> u32 {
        (self.frames() + self.rate as usize / 10) as u32
    }

    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.sides = [Side::default(); 2];
        self.wobble = Wobble::new(self.rate);
        self.at = 0;
    }

    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.drive.target = 10_f32.powf(tape_drive_db(f32::from(settings.drive)) / 20.0);
        self.tilt.target = 0.5 + 0.5 * from_middle(f32::from(settings.tone));
        let bias = from_middle(f32::from(settings.bias));
        self.bias.target = if bias > 0.0 {
            bias * MOST_OVERBIAS
        } else {
            bias
        };
        self.bump.target = f32::from(settings.bump) / 255.0;
        // Squared, so the first half is gentle and `80` is Beads' own amount.
        let frames = self.rate as f32 / BEADS_RATE;
        let amount = |value: u8| (f32::from(value) / 255.0).powi(2);
        self.wow.target = amount(settings.wow) * MOST_WOW_FRAMES * frames;
        self.flutter.target = amount(settings.flutter) * MOST_FLUTTER_FRAMES * frames;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.drive.settle();
        self.tilt.settle();
        self.bias.settle();
        self.bump.settle();
        self.wow.settle();
        self.flutter.settle();
        self.wobble.delay = None;
        self.wobble.countdown = 0;
    }

    /// What every side shares this frame, from the settings as they glide.
    #[inline]
    fn frame(&mut self) -> Frame {
        let tilt = self.tilt.next(self.smoothing);
        let bias = self.bias.next(self.smoothing);
        let bump = self.bump.next(self.smoothing);
        let scale = self.fixed.scale;
        let mut limit = if bias > 0.0 {
            (1.0 - bias).powi(3) / scale
        } else {
            1.0 / scale
        };
        let golden_ratio = (1.0 + 5_f32.sqrt()) * 0.5;
        let mut limits = [0.0; BIAS_STAGES];
        // The last stage is the tightest.
        for stage in limits.iter_mut().rev() {
            *stage = limit;
            limit *= golden_ratio;
        }
        Frame {
            encode: tilt * 2.0,
            decode: ((1.0 - tilt) * -2.0).max(-1.0),
            biased: bias.abs() > 0.001,
            under: if bias < 0.0 {
                bias.powi(4) * 0.25 / scale
            } else {
                0.0
            },
            limits,
            bump_drive: bump * BUMP_DRIVE / scale,
            bump_mix: bump * BUMP_MIX,
            at: self.at,
        }
    }

    /// A line's sound a number of frames back, read between samples through four of them.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let frames = frames.clamp(1.0, (self.mask - 3) as f32);
        let whole = frames as usize;
        let between = frames - whole as f32;
        let line = &self.lines[side];
        let at = |back: usize| line[self.write.wrapping_sub(back) & self.mask];
        let (newer, current, older, oldest) =
            (at(whole - 1), at(whole), at(whole + 1), at(whole + 2));
        // A cubic through the four, after Hermite, so a whole number of frames reads exactly.
        let slope_in = (older - newer) * 0.5;
        let slope_out = (oldest - current) * 0.5;
        let difference = current - older;
        let bend = difference + slope_in;
        let lift = bend + difference + slope_out;
        ((lift * between - bend - lift) * between + slope_in) * between + current
    }

    /// One frame through the tape, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let drive = self.drive.next(self.smoothing);
        let wow = self.wow.next(self.smoothing);
        let flutter = self.flutter.next(self.smoothing);
        let frame = self.frame();
        let delay = self.wobble.next(wow, flutter, self.rate);
        self.write = (self.write + 1) & self.mask;
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            let encoded = self.sides[side].before_line(sample * drive, &frame, &self.fixed);
            self.lines[side][self.write] = encoded;
            let wobbled = self.read(side, delay);
            out[side] = self.sides[side].after_line(wobbled, &frame, &self.fixed);
        }
        self.at = (self.at + 1) % 32;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn tape(settings: Settings) -> Tape {
        let mut tape = Tape::new(RATE);
        tape.follow(settings);
        tape.settle();
        tape
    }

    /// Gentle: no drive, tilt or bias, no bump and no wobble.
    const GENTLE: Settings = Settings {
        bump: 0,
        wow: 0,
        flutter: 0,
        ..Settings::DEFAULT
    };

    fn sine(frame: usize, hertz: f32, level: f32) -> f32 {
        (frame as f32 / RATE as f32 * hertz * TAU).sin() * level
    }

    /// How loud a sine at a frequency comes out, as a share of how loud it went in, measured by
    /// correlating with it over the last half of a quarter second. The tape is two frames late.
    fn harmonic(out: &[f32], hertz: f32) -> f32 {
        let start = out.len() / 2;
        let (mut sine_sum, mut cosine_sum) = (0.0_f32, 0.0_f32);
        for (frame, sample) in out.iter().enumerate().skip(start) {
            let turns = frame as f32 / RATE as f32 * hertz * TAU;
            sine_sum += sample * turns.sin();
            cosine_sum += sample * turns.cos();
        }
        2.0 * sine_sum.hypot(cosine_sum) / (out.len() - start) as f32
    }

    fn through(tape: &mut Tape, hertz: f32, level: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| tape.process([sine(frame, hertz, level); 2])[0])
            .collect()
    }

    #[test]
    fn gently_it_leaves_the_sound_nearly_as_it_is() {
        for hertz in [100.0, 1_000.0, 5_000.0] {
            // A quiet sound comes through within half a dB, lined up with how it went in.
            let mut quiet = tape(GENTLE);
            let out = through(&mut quiet, hertz, 0.05, RATE as usize / 4);
            let level = harmonic(&out, hertz) / 0.05;
            assert!((level - 1.0).abs() < 0.06, "{hertz} Hz: {level}");
            let off = out[RATE as usize / 8..]
                .iter()
                .enumerate()
                .map(|(at, sample)| (sample - sine(at + RATE as usize / 8 - 2, hertz, 0.05)).abs())
                .fold(0.0_f32, f32::max);
            assert!(off < 0.05 * 0.1, "{hertz} Hz: {off}");
        }
        // A louder one, below where its highs start to soften, has barely anything added.
        for hertz in [100.0, 1_000.0] {
            let mut louder = tape(GENTLE);
            let out = through(&mut louder, hertz, 0.25, RATE as usize / 4);
            let third = harmonic(&out, hertz * 3.0) / 0.25;
            assert!(third < 0.01, "{hertz} Hz: {third}");
        }
    }

    #[test]
    fn loud_highs_soften_as_tape_does_and_the_lows_stay() {
        let level = |hertz: f32, level: f32| {
            let mut tape = tape(GENTLE);
            harmonic(&through(&mut tape, hertz, level, RATE as usize / 4), hertz) / level
        };
        assert!(
            (level(100.0, 0.25) - 1.0).abs() < 0.06,
            "{}",
            level(100.0, 0.25)
        );
        assert!(level(10_000.0, 0.5) < level(10_000.0, 0.05) * 0.7);
    }

    #[test]
    fn drive_saturates_with_odd_harmonics_and_holds_its_peaks() {
        let third = |drive: u8| {
            let mut tape = tape(Settings { drive, ..GENTLE });
            let out = through(&mut tape, 200.0, 0.5, RATE as usize / 4);
            let peak = out.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
            assert!(peak <= CLIP_MOST, "{drive:x}: {peak}");
            harmonic(&out, 600.0) / harmonic(&out, 200.0)
        };
        let clean = third(0x80);
        let driven = third(0xFF);
        assert!(driven > 0.05, "{driven}");
        assert!(driven > clean * 10.0, "{clean} {driven}");
    }

    #[test]
    fn tone_tilts_the_highs() {
        let highs = |tone: u8| {
            let mut tape = tape(Settings { tone, ..GENTLE });
            harmonic(
                &through(&mut tape, 8_000.0, 0.1, RATE as usize / 4),
                8_000.0,
            )
        };
        assert!(
            highs(0x00) < highs(0x80) * 0.9,
            "{} {}",
            highs(0),
            highs(0x80)
        );
        assert!(
            highs(0xFF) > highs(0x80) * 1.1,
            "{} {}",
            highs(0xFF),
            highs(0x80)
        );
    }

    #[test]
    fn the_bump_lifts_the_lows() {
        let lows = |bump: u8| {
            let mut tape = tape(Settings { bump, ..GENTLE });
            harmonic(&through(&mut tape, 70.0, 0.1, RATE as usize), 70.0)
        };
        assert!(lows(0xFF) > lows(0x00) * 1.2, "{} {}", lows(0xFF), lows(0));
    }

    #[test]
    fn overbias_softens_what_moves_fast() {
        let highs = |bias: u8| {
            let mut tape = tape(Settings { bias, ..GENTLE });
            harmonic(
                &through(&mut tape, 5_000.0, 0.5, RATE as usize / 4),
                5_000.0,
            )
        };
        assert!(
            highs(0xFF) < highs(0x80) * 0.7,
            "{} {}",
            highs(0xFF),
            highs(0x80)
        );
    }

    /// How fast a sine through it goes in each window, from where it crosses zero going up, read
    /// between samples.
    fn pitches(tape: &mut Tape, frames: usize, window: usize) -> Vec<f32> {
        let out = through(tape, 1_000.0, 0.25, frames);
        let mut crossings = Vec::new();
        for at in 1..out.len() {
            if out[at - 1] < 0.0 && out[at] >= 0.0 {
                crossings.push(at as f32 - out[at] / (out[at] - out[at - 1]));
            }
        }
        crossings
            .chunks(window)
            .filter(|found| found.len() == window)
            .map(|found| (window - 1) as f32 / (found[window - 1] - found[0]) * RATE as f32)
            .collect()
    }

    fn spread(pitches: &[f32]) -> f32 {
        let (low, high) = pitches
            .iter()
            .skip(2)
            .fold((f32::MAX, 0.0_f32), |(low, high), p| {
                (low.min(*p), high.max(*p))
            });
        high / low - 1.0
    }

    #[test]
    fn wow_and_flutter_move_the_pitch() {
        let still = spread(&pitches(&mut tape(GENTLE), RATE as usize * 4, 50));
        assert!(still < 1e-4, "{still}");
        let wow = spread(&pitches(
            &mut tape(Settings {
                wow: 0xFF,
                ..GENTLE
            }),
            RATE as usize * 8,
            50,
        ));
        assert!(wow > 0.002, "{wow}");
        let flutter = spread(&pitches(
            &mut tape(Settings {
                flutter: 0xFF,
                ..GENTLE
            }),
            RATE as usize * 2,
            20,
        ));
        assert!(flutter > 0.002, "{flutter}");
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let mut tape = tape(Settings::DEFAULT);
        let tone = |frame: usize| sine(frame, 100.0, 0.5);
        let mut last = [0.0; 2];
        for frame in 0..RATE as usize / 10 {
            last = tape.process([tone(frame); 2]);
        }
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as u8;
            tape.follow(Settings {
                drive: 0x20 + step * 0xC0,
                tone: 0x20 + step * 0xC0,
                bias: 0x20 + step * 0xC0,
                bump: step * 0xFF,
                wow: step * 0xFF,
                flutter: step * 0xFF,
                ..Settings::DEFAULT
            });
            let at = RATE as usize / 10 + frame;
            let out = tape.process([tone(at); 2]);
            for side in 0..2 {
                assert!(
                    (out[side] - last[side]).abs() < 0.05,
                    "{frame}: {last:?} to {out:?}"
                );
            }
            last = out;
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for drive in [0x00, 0xFF] {
            for tone in [0x00, 0xFF] {
                for bias in [0x00, 0x80, 0xFF] {
                    for bump in [0x00, 0xFF] {
                        for (rate, wobble) in [(44_100, 0x00), (192_000, 0xFF)] {
                            let mut tape = Tape::new(rate);
                            tape.follow(Settings {
                                drive,
                                tone,
                                bias,
                                bump,
                                wow: wobble,
                                flutter: wobble,
                                ..Settings::DEFAULT
                            });
                            tape.settle();
                            for frame in 0..rate as usize / 2 {
                                // Full-scale noise, silence, a full-scale square and DC in turn.
                                let level = match frame / 2000 % 4 {
                                    0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                    1 => 0.0,
                                    2 => {
                                        if frame / 50 % 2 == 0 {
                                            1.0
                                        } else {
                                            -1.0
                                        }
                                    }
                                    _ => 1.0,
                                };
                                let out = tape.process([level, -level]);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() <= CLIP_MOST),
                                    "{drive:x} {tone:x} {bias:x} {bump:x} {rate}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_reads_as_far_back_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let tape = Tape::new(rate);
            let longest = 1.0 + (MOST_WOW_FRAMES + MOST_FLUTTER_FRAMES) * rate as f32 / BEADS_RATE;
            assert!(tape.frames() as f32 >= longest + 4.0, "{rate}");
        }
    }
}
