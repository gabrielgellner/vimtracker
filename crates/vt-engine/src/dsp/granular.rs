//! A granular effect, as the granular mode of Mutable Instruments' Beads: what comes in is
//! recorded all the time into a few seconds of line, and up to 32 grains play it back from behind
//! the recording, each from where `Time` says, for as long as `Size` says, shifted by `Pitch`
//! and faded in and out under `Shape`'s envelope. `Density` starts them, evenly or at random, and
//! each new grain steals the one nearest its end when they're all playing, fading it out first.
//! `Freeze` stops the recording, so the grains keep playing what's held.
//!
//! Each grain is as loud as one over the square root of how many are playing when it starts, as
//! Beads' are, and Beads' limiter comes after them, so however many overlap it stays bounded. The
//! line is Beads' four seconds at whatever rate it runs at, of floats rather than Beads' 16, 12 or
//! 8 bits, and its times Beads gives in frames at 48 kHz are scaled to the rate. Its delay mode,
//! wavetable mode, tape qualities, feedback and reverb are left out.
//!
//! Adapted from Beads' `beads/dsp/granular_player.h`, `grain.h`, `delay_grain.h`,
//! `grain_envelope.h`, `voice_stealing_envelope.h`, `trigger_generator.cc`, `delay_line.h`,
//! `randomized_parameter.h` and `limiter.h`, MIT:
//!
//! Copyright 2021 Emilie Gillet.
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

use vt_model::insert::{
    GRAIN_BUFFER_SECONDS, Grains as Settings, Triggering, grain_density, grain_length,
    shift_semitones,
};

use crate::dsp::filter::soft_clip;
use crate::dsp::smoother::{Smoothed, coefficient};

/// How many grains can play at once, as Beads'.
pub const GRAINS: usize = 32;
/// The rate Beads runs at, which its times in frames are for.
const BEADS_RATE: f32 = 48_000.0;
/// How near either end of the line a grain fades out, in frames at Beads' rate.
const SAFETY_FRAMES: f32 = 256.0;
/// How much of what comes in just after freezing is kept, to fade the recording back in when it
/// starts again, in frames at Beads' rate.
const FREEZE_TAIL_FRAMES: f32 = 256.0;
/// How fast grains that are going to be stolen fade out, in seconds, and how close together new
/// grains have to come before the oldest is simply taken.
const INSTANT_CROSSFADE_SECONDS: f32 = 0.005;
const PREEMPTIVE_CROSSFADE_SECONDS: f32 = 0.1;
const SIMPLE_SCHEDULER_SECONDS: f32 = 0.025;
/// How many of the grains nearest their end are faded out as each new one starts, so the next can
/// take one without a click.
const SAFETY_GRAINS: usize = 4;
/// The shortest attack and release a grain's envelope has, in seconds.
const SHORTEST_ATTACK_SECONDS: f32 = 0.0005;
const SHORTEST_RELEASE_SECONDS: f32 = 0.010;
/// The loudest what comes out gets before the limiter turns it down, and how far past it a soft
/// clip holds it while the limiter catches up.
const CEILING: f32 = 1.0;
const HEADROOM: f32 = 4.0;
/// How long the limiter takes to let go, in seconds, as Beads' output limiter.
const LIMITER_RELEASE_SECONDS: f32 = 6.0;

/// A smooth step from 0 to 1.
#[inline]
fn smooth_step(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

/// Where a grain's envelope is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Attack,
    Sustain,
    Release,
    Done,
}

impl Stage {
    fn next(self) -> Self {
        match self {
            Self::Attack => Self::Sustain,
            Self::Sustain => Self::Release,
            Self::Release | Self::Done => Self::Done,
        }
    }
}

/// A grain's envelope, as Beads': an attack, a sustain and a release whose lengths and curves the
/// shape sets, never shorter than a little at either end so its edges never click.
#[derive(Debug, Clone, Copy)]
struct Envelope {
    stage: Stage,
    phase: f32,
    /// How far each stage goes each frame, from 0 to 1.
    steps: [f32; 3],
    /// How far each edge bends from a straight line or a fourth power towards a smooth step.
    smoothness: f32,
}

impl Envelope {
    const DONE: Self = Self {
        stage: Stage::Done,
        phase: 0.0,
        steps: [0.5; 3],
        smoothness: 0.0,
    };

    /// Start a grain of so many frames, with a shape from 0, square, through 0.5, a decay, to 1, a
    /// reversed decay, and a bell at two thirds. It starts a share of a frame late.
    fn start(&mut self, frames: f32, shape: f32, late: f32, shortest: [f32; 2]) {
        let sustain = (1.0 - 3.0 * shape).max(0.0);
        let curve = (shape - 1.0 / 3.0) * 1.5;
        let attack = if curve < 0.0 {
            0.0
        } else if curve < 0.5 {
            curve * curve * curve * 4.0
        } else {
            curve
        };
        let release = (1.0 - attack) * (1.0 - sustain);
        let attack_frames = (frames * attack).max(shortest[0] * (1.0 + 3.0 * sustain));
        let sustain_frames = (frames * sustain).max(2.0);
        let release_frames = (frames * release).max(shortest[1]);
        self.steps = [
            1.0 / attack_frames,
            1.0 / sustain_frames,
            1.0 / release_frames,
        ];
        self.smoothness = if attack < 0.5 {
            2.0 * attack
        } else {
            2.0 - 2.0 * attack
        };
        self.stage = Stage::Attack;
        self.phase = self.steps[0] * late;
        self.wrap();
    }

    fn step(&self) -> f32 {
        match self.stage {
            Stage::Attack => self.steps[0],
            Stage::Sustain => self.steps[1],
            Stage::Release => self.steps[2],
            Stage::Done => 0.0,
        }
    }

    /// Go on to the next stage once this one's done, carrying over what's past its end.
    #[inline]
    fn wrap(&mut self) {
        if self.phase >= 1.0 {
            let over = (self.phase - 1.0) / self.step();
            self.stage = self.stage.next();
            self.phase = over * self.step();
        }
    }

    fn done(&self) -> bool {
        self.stage == Stage::Done
    }

    #[inline]
    fn next(&mut self) -> f32 {
        let value = match self.stage {
            Stage::Done => return 0.0,
            Stage::Sustain => 1.0,
            stage => {
                let attack = stage == Stage::Attack;
                let t = if attack { self.phase } else { 1.0 - self.phase };
                let base = if attack { t } else { (t * t) * (t * t) };
                base + (smooth_step(t) - base) * self.smoothness
            }
        };
        self.phase += self.step();
        self.wrap();
        value
    }
}

/// One grain: where it reads, how fast, its envelope, the fade that takes it when it's stolen,
/// and how much of each side goes to each.
#[derive(Debug, Clone, Copy)]
struct Grain {
    /// How far behind the recording it reads, in frames.
    delay: f64,
    /// How many frames of the recording it goes through each frame, backwards if negative.
    speed: f64,
    envelope: Envelope,
    /// The fade it's given when it's to be stolen, from 1 down, and how far it falls each frame.
    stealing: f32,
    stealing_step: f32,
    /// About how many frames it has left.
    remaining: i32,
    /// How much of each side, left then right, goes to the left and then the right.
    gains: [[f32; 2]; 2],
}

impl Grain {
    const DONE: Self = Self {
        delay: 0.0,
        speed: 1.0,
        envelope: Envelope::DONE,
        stealing: 1.0,
        stealing_step: 0.0,
        remaining: 0,
        gains: [[0.5; 2]; 2],
    };

    fn done(&self) -> bool {
        self.envelope.done() || self.stealing <= 0.0
    }

    /// Fade it out over so many frames, if it has longer than that left.
    fn fade_out(&mut self, frames: i32) {
        if self.remaining > frames {
            let frames = frames.max(1) as f32;
            self.stealing_step = 1.0 / frames;
            self.remaining = (self.stealing * frames) as i32;
        }
    }
}

/// When grains start, as Beads' trigger generator does with its seed held: a pulse that runs on
/// by itself, evenly or at random intervals.
#[derive(Debug, Clone, Copy)]
struct Trigger {
    phase: f32,
    /// How much longer or shorter than the average the interval is now, when it's at random.
    ratio: f32,
}

/// The start of `-ln(1 - x)`'s series, as Beads uses for its random intervals.
#[inline]
fn log_one_minus(x: f32) -> f32 {
    -x * (1.0 + x * (0.5 + x * (0.3333 + x * 0.25)))
}

/// A granular effect's line, grains and what moves them.
#[derive(Debug, Clone)]
pub struct Granular {
    /// The recording, a stereo frame at a time, the newest at `write`.
    line: Vec<[f32; 2]>,
    write: usize,
    /// How far back a grain can read, in frames: Beads' four seconds.
    longest: f64,
    /// What came in just after it froze, and how much of it is left to fade the recording back
    /// in with, of how much.
    tail: Vec<[f32; 2]>,
    tail_written: usize,
    tail_fade: usize,
    /// Whether it's frozen now, and whether it was last frame.
    frozen: bool,
    was_frozen: bool,
    grains: [Grain; GRAINS],
    /// The grains, those best to steal first.
    order: [u8; GRAINS],
    /// Frames since the last grain started.
    since: i32,
    trigger: Trigger,
    /// The settings, each on its own `00` to `FF` scale but `Pitch`, in semitones.
    time: Smoothed,
    size: Smoothed,
    semitones: Smoothed,
    shape: Smoothed,
    density: Smoothed,
    spread: Smoothed,
    random: Smoothed,
    /// The limiter's peak, and the peak smoothed again, which sets how far it turns down.
    peak: f32,
    peak_smoothed: f32,
    release: f32,
    /// Frames near either end of the line where grains fade, and the shortest attack and release.
    safety: f64,
    shortest: [f32; 2],
    instant_crossfade: i32,
    preemptive_crossfade: i32,
    simple_scheduler: i32,
    random_state: u32,
    rate: u32,
    smoothing: f32,
}

impl Granular {
    /// A line of Beads' four seconds at this rate.
    pub fn new(rate: u32) -> Self {
        let rate_f = rate as f32;
        let longest = (GRAIN_BUFFER_SECONDS * rate_f).round() as usize;
        let at_beads = rate_f / BEADS_RATE;
        let tail = (FREEZE_TAIL_FRAMES * at_beads).ceil() as usize;
        let mut granular = Self {
            // Room past the longest for a read between samples.
            line: vec![[0.0; 2]; longest + 4],
            write: 0,
            longest: longest as f64,
            tail: vec![[0.0; 2]; tail],
            tail_written: 0,
            tail_fade: 0,
            frozen: false,
            was_frozen: false,
            grains: [Grain::DONE; GRAINS],
            order: std::array::from_fn(|index| index as u8),
            since: 1,
            trigger: Trigger {
                phase: 0.0,
                ratio: 1.0,
            },
            time: Smoothed::default(),
            size: Smoothed::default(),
            semitones: Smoothed::default(),
            shape: Smoothed::default(),
            density: Smoothed::default(),
            spread: Smoothed::default(),
            random: Smoothed::default(),
            peak: 0.5,
            peak_smoothed: 0.5,
            release: 1.0 / (rate_f * LIMITER_RELEASE_SECONDS),
            safety: f64::from(SAFETY_FRAMES * at_beads),
            shortest: [
                SHORTEST_ATTACK_SECONDS * rate_f,
                SHORTEST_RELEASE_SECONDS * rate_f,
            ],
            instant_crossfade: (INSTANT_CROSSFADE_SECONDS * rate_f) as i32,
            preemptive_crossfade: (PREEMPTIVE_CROSSFADE_SECONDS * rate_f) as i32,
            simple_scheduler: (SIMPLE_SCHEDULER_SECONDS * rate_f) as i32,
            random_state: 0x2545_F491,
            rate,
            smoothing: coefficient(rate),
        };
        granular.follow(Settings::DEFAULT);
        granular.settle();
        granular
    }

    /// How many frames its line holds.
    pub fn frames(&self) -> usize {
        self.line.len()
    }

    /// How many quiet frames in a row mean nothing's left in it. Frozen, what it holds can always
    /// come back, so it never is.
    pub fn quiet_frames(&self) -> u32 {
        if self.frozen {
            u32::MAX
        } else {
            self.line.len() as u32
        }
    }

    pub fn clear(&mut self) {
        self.line.fill([0.0; 2]);
        self.tail.fill([0.0; 2]);
        self.tail_written = 0;
        self.tail_fade = 0;
        self.grains = [Grain::DONE; GRAINS];
        self.order = std::array::from_fn(|index| index as u8);
        self.since = 1;
        self.trigger.phase = 0.0;
        self.peak = 0.5;
        self.peak_smoothed = 0.5;
    }

    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.time.target = f32::from(settings.time);
        self.size.target = f32::from(settings.size);
        self.semitones.target = shift_semitones(f32::from(settings.pitch));
        self.shape.target = f32::from(settings.shape);
        self.density.target = f32::from(settings.density);
        self.spread.target = f32::from(settings.spread);
        self.random.target = f32::from(settings.random);
        self.frozen = settings.freeze != 0;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.time.settle();
        self.size.settle();
        self.semitones.settle();
        self.shape.settle();
        self.density.settle();
        self.spread.settle();
        self.random.settle();
    }

    /// A number from 0 to 1 that's hard to guess, the same each time it starts.
    #[inline]
    fn uniform(&mut self) -> f32 {
        self.random_state ^= self.random_state << 13;
        self.random_state ^= self.random_state >> 17;
        self.random_state ^= self.random_state << 5;
        self.random_state as f32 / u32::MAX as f32
    }

    /// Record a frame, unless it's frozen. Just after freezing it keeps a little of what comes in,
    /// and when it records again it fades from that into what's coming in, so where the
    /// recording stopped and starts again joins smoothly.
    #[inline]
    fn record(&mut self, mut input: [f32; 2]) {
        if self.frozen {
            if !self.was_frozen {
                self.tail_written = 0;
            }
            if self.tail_written < self.tail.len() {
                self.tail[self.tail_written] = input;
                self.tail_written += 1;
            }
        } else {
            if self.was_frozen && self.tail_written > 0 {
                self.tail_fade = self.tail_written;
            }
            if self.tail_written > 0 {
                let kept = self.tail[self.tail_fade - self.tail_written];
                let gain = smooth_step(self.tail_written as f32 / self.tail_fade as f32);
                for (sample, kept) in input.iter_mut().zip(kept) {
                    *sample += (kept - *sample) * gain;
                }
                self.tail_written -= 1;
            }
            self.write += 1;
            if self.write == self.line.len() {
                self.write = 0;
            }
            self.line[self.write] = input;
        }
        self.was_frozen = self.frozen;
    }

    /// Whether a grain starts this frame, and how far through the frame it starts.
    #[inline]
    fn triggered(&mut self) -> Option<f32> {
        let (hertz, random) = match grain_density(self.density.next(self.smoothing)) {
            Triggering::Regular(hertz) => (hertz, false),
            Triggering::Random(hertz) => (hertz, true),
            Triggering::Off => return None,
        };
        if !random {
            self.trigger.ratio = 1.0;
        }
        let step = (hertz / self.rate as f32 * self.trigger.ratio).clamp(0.000_001, 0.25);
        self.trigger.phase += step;
        if self.trigger.phase < 1.0 {
            return None;
        }
        self.trigger.phase -= 1.0;
        let late = self.trigger.phase / step;
        if random {
            self.trigger.ratio = -1.0 / log_one_minus(0.1 + 0.9 * self.uniform());
        }
        Some(late)
    }

    /// A setting on its `00` to `FF` scale from 0 to 1, strayed at random by `Random`'s share
    /// towards anywhere in its range, as Beads' attenuverters do with nothing patched.
    #[inline]
    fn strayed(&mut self, value: f32, random: f32) -> f32 {
        let base = value / 255.0;
        if random <= 0.0 {
            return base;
        }
        (base + (self.uniform() - base) * random).clamp(0.0, 1.0)
    }

    /// Start a grain, a share of a frame late, in the place of the best one to steal.
    fn start_grain(&mut self, late: f32) {
        let random = self.random.value / 255.0;
        let time = self.strayed(self.time.value, random);
        let size = self.strayed(self.size.value, random);
        let shape = self.strayed(self.shape.value, random);
        let (seconds, reversed) = grain_length(size * 255.0);
        let mut frames = f64::from(seconds) * f64::from(self.rate);
        let ratio = f64::from((self.semitones.value / 12.0).exp2());
        let speed = if reversed { -ratio } else { ratio };

        // As loud as one over the root of how many there'll be, and spread wider the more there
        // are.
        let playing = self.grains.iter().filter(|grain| !grain.done()).count() + 1;
        let amplitude = 1.0 / (playing as f32).sqrt();
        let spread = 0.5 * playing as f32 * self.spread.value / 255.0;
        let (u, v) = (self.uniform(), self.uniform());
        let pan = 0.5 + (u - 0.5) * (spread * v).min(1.0);

        // How fast it moves through the line, which also moves on as it records. It's kept from
        // reaching the recording or the line's far end before it's done.
        let consumed = speed - if self.frozen { 0.0 } else { 1.0 };
        let mut delay = f64::from(time) * (self.longest - 2.0 * self.safety) + self.safety;
        delay -= f64::from(late) * consumed;
        if consumed != 0.0 {
            frames = frames.min(self.longest / consumed.abs());
        }
        if consumed > 0.0 {
            delay = delay.max(frames * consumed);
        } else if consumed < 0.0 {
            delay = delay.min(self.longest + frames * consumed);
        }

        // Roughly equal power, from the first-order square root, each side crossing to the other
        // as it pans away from it.
        let gains = if pan < 0.5 {
            let pan = pan * 2.0;
            [
                [amplitude, (1.0 - pan) * (1.0 + pan) * amplitude],
                [0.0, pan * (2.0 - pan) * amplitude],
            ]
        } else {
            let pan = (1.0 - pan) * 2.0;
            [
                [pan * (2.0 - pan) * amplitude, 0.0],
                [(1.0 - pan) * (1.0 + pan) * amplitude, amplitude],
            ]
        };

        let grain = &mut self.grains[usize::from(self.order[0])];
        grain.delay = delay;
        grain.speed = speed;
        grain.gains = gains;
        grain
            .envelope
            .start(frames as f32, shape, late, self.shortest);
        grain.remaining = frames as i32;
        grain.stealing = 1.0;
        grain.stealing_step = 0.0;
        self.sort_grains();
    }

    /// Put the grains best to steal first, as a new one has started, and fade those out, so they
    /// end quickly in case another starts soon. When grains come fast the one that's just
    /// started goes to the back and the next in line is taken; otherwise the four with least left
    /// come first.
    fn sort_grains(&mut self) {
        let mut fades;
        if self.since < self.simple_scheduler {
            self.order.rotate_left(1);
            fades = [
                self.since + 1,
                self.instant_crossfade,
                self.instant_crossfade,
                self.preemptive_crossfade,
            ];
            if self.since > self.instant_crossfade {
                fades.swap(0, 2);
            }
        } else {
            for first in 0..SAFETY_GRAINS {
                let least = (first..GRAINS)
                    .min_by_key(|&at| self.grains[usize::from(self.order[at])].remaining)
                    .unwrap_or(first);
                self.order.swap(first, least);
            }
            fades = [
                self.instant_crossfade,
                self.instant_crossfade,
                self.since,
                self.preemptive_crossfade,
            ];
            if self.since > self.preemptive_crossfade {
                fades.swap(2, 3);
            }
        }
        for (at, fade) in fades.into_iter().enumerate() {
            self.grains[usize::from(self.order[at])].fade_out(fade);
        }
        self.since = 0;
    }

    /// The line a number of frames behind the newest, read between samples through four of them.
    #[inline]
    fn read(&self, back: f64) -> [f32; 2] {
        let back = back.clamp(1.0, self.longest + 1.0);
        let whole = back as usize;
        let between = (back - whole as f64) as f32;
        let length = self.line.len();
        let at = |frames: usize| {
            let index = if frames > self.write {
                self.write + length - frames
            } else {
                self.write - frames
            };
            self.line[index]
        };
        let (newer, current, older, oldest) =
            (at(whole - 1), at(whole), at(whole + 1), at(whole + 2));
        let mut out = [0.0; 2];
        for side in 0..2 {
            // A cubic through the four, after Hermite, so a whole number of frames reads exactly.
            let slope_in = (older[side] - newer[side]) * 0.5;
            let slope_out = (oldest[side] - current[side]) * 0.5;
            let difference = current[side] - older[side];
            let bend = difference + slope_in;
            let lift = bend + difference + slope_out;
            out[side] =
                ((lift * between - bend - lift) * between + slope_in) * between + current[side];
        }
        out
    }

    /// Feed one frame in and get the grains out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        for setting in [
            &mut self.time,
            &mut self.size,
            &mut self.semitones,
            &mut self.shape,
            &mut self.spread,
            &mut self.random,
        ] {
            setting.next(self.smoothing);
        }
        self.record(input);
        if let Some(late) = self.triggered() {
            self.start_grain(late);
        }

        let mut out = [0.0_f32; 2];
        let still = if self.frozen { 0.0 } else { 1.0 };
        for index in 0..GRAINS {
            if self.grains[index].done() {
                continue;
            }
            let grain = &mut self.grains[index];
            grain.delay += still - grain.speed;
            if grain.delay < 0.0 || grain.delay >= self.longest {
                grain.delay = 0.0;
                grain.envelope.stage = Stage::Done;
            }
            // It fades near the recording and the line's far end, where what's newest and oldest
            // meet.
            let delay = grain.delay;
            let near = if delay >= self.longest * 0.5 {
                self.longest - delay
            } else {
                delay
            };
            let fade = smooth_step((near / self.safety).min(1.0) as f32);
            let sample = self.read(delay);
            let grain = &mut self.grains[index];
            grain.stealing -= grain.stealing_step;
            let gain = fade * grain.envelope.next() * grain.stealing.max(0.0);
            let [left, right] = [sample[0] * gain, sample[1] * gain];
            out[0] += left * grain.gains[0][0] + right * grain.gains[0][1];
            out[1] += left * grain.gains[1][0] + right * grain.gains[1][1];
            grain.remaining -= 1;
            if grain.done() {
                grain.remaining = 0;
            }
        }
        self.since = self.since.saturating_add(1);

        // Beads' limiter: a fast peak follower that lets go slowly turns it down past the
        // ceiling, and a soft clip catches what gets through before it does.
        let peak = out[0].abs().max(out[1].abs()).min(40.0);
        let slope = |value: &mut f32, to: f32, up: f32, down: f32| {
            let error = to - *value;
            *value += if error > 0.0 { up } else { down } * error;
        };
        slope(&mut self.peak, peak, 0.1, self.release);
        let smoothed_peak = self.peak;
        slope(&mut self.peak_smoothed, smoothed_peak, 0.5, self.release);
        let gain = if self.peak_smoothed <= CEILING {
            1.0
        } else {
            CEILING / self.peak_smoothed
        };
        out.map(|side| soft_clip(side * gain / HEADROOM) * HEADROOM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use vt_model::insert::MOST_SHIFT;

    const RATE: u32 = 48_000;

    fn granular(settings: Settings, rate: u32) -> Granular {
        let mut granular = Granular::new(rate);
        granular.follow(settings);
        granular.settle();
        granular
    }

    fn sine(frame: usize, hertz: f32, rate: u32) -> f32 {
        (frame as f32 / rate as f32 * hertz * TAU).sin() * 0.5
    }

    /// How many grains started over a stretch.
    fn started(settings: Settings, seconds: f32) -> usize {
        let mut granular = granular(settings, RATE);
        (0..(seconds * RATE as f32) as usize)
            .filter(|_| granular.triggered().is_some())
            .count()
    }

    #[test]
    fn density_starts_none_in_the_middle_and_many_at_either_end() {
        let at = |density| Settings {
            density,
            ..Settings::DEFAULT
        };
        assert_eq!(started(at(0x80), 2.0), 0);
        // Fastest evenly is Beads' 130.8 Hz.
        let even = started(at(0x00), 2.0);
        assert!((even as f32 - 261.6).abs() < 2.0, "{even}");
        // Fastest at random is half that on average, give or take.
        let random = started(at(0xFF), 2.0);
        assert!(random > 80 && random < 250, "{random}");
        // Near the middle on either side, hardly any.
        assert!(started(at(0x72), 2.0) < 4);
        assert!(started(at(0x8E), 2.0) < 4);
        // Turning towards either end, more and more.
        let evens: Vec<usize> = [0x60, 0x40, 0x20, 0x00]
            .map(|density| started(at(density), 2.0))
            .to_vec();
        assert!(evens.windows(2).all(|pair| pair[0] < pair[1]), "{evens:?}");
        let randoms: Vec<usize> = [0xA0, 0xC0, 0xE0, 0xFF]
            .map(|density| started(at(density), 4.0))
            .to_vec();
        assert!(
            randoms.windows(2).all(|pair| pair[0] < pair[1]),
            "{randoms:?}"
        );
    }

    #[test]
    fn its_grains_play_what_it_recorded_and_stop_with_density_in_the_middle() {
        let mut granular = granular(Settings::DEFAULT, RATE);
        let mut energy = 0.0;
        for frame in 0..RATE as usize * 2 {
            let out = granular.process([sine(frame, 330.0, RATE); 2]);
            if frame > RATE as usize {
                energy += out[0] * out[0] + out[1] * out[1];
            }
        }
        // About a sine at half scale, grains overlapping.
        let rms = (energy / (2.0 * RATE as f32)).sqrt();
        assert!(rms > 0.1 && rms < 0.6, "{rms}");
        let mut quiet = granular.clone();
        quiet.follow(Settings {
            density: 0x80,
            ..Settings::DEFAULT
        });
        // Once the grains playing are done, nothing more comes out.
        let out: Vec<[f32; 2]> = (0..RATE as usize * 2)
            .map(|frame| quiet.process([sine(frame, 330.0, RATE); 2]))
            .collect();
        assert!(out[RATE as usize..].iter().flatten().all(|s| *s == 0.0));
    }

    /// How strong a frequency is in a sound, as a sine's amplitude, by a single bin of a DFT over
    /// each 40 ms through a Hann window, on average.
    fn strength(sound: &[f32], hertz: f32) -> f32 {
        let chunks = sound.as_chunks::<{ RATE as usize / 25 }>().0;
        let count = chunks.len() as f32;
        chunks
            .iter()
            .map(|chunk| {
                let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
                let length = chunk.len() as f64;
                for (frame, sample) in chunk.iter().enumerate() {
                    let at = frame as f64;
                    let hann = 0.5 - 0.5 * (std::f64::consts::TAU * at / length).cos();
                    let angle = f64::from(TAU * hertz / RATE as f32) * at;
                    real += f64::from(*sample) * hann * angle.cos();
                    imaginary += f64::from(*sample) * hann * angle.sin();
                }
                ((real * real + imaginary * imaginary).sqrt() * 4.0 / length) as f32
            })
            .sum::<f32>()
            / count
    }

    /// The last second of three of a tone through it, on the left.
    fn through(settings: Settings, hertz: f32) -> Vec<f32> {
        let mut granular = granular(settings, RATE);
        (0..RATE as usize * 3)
            .map(|frame| granular.process([sine(frame, hertz, RATE); 2])[0])
            .skip(RATE as usize * 2)
            .collect()
    }

    #[test]
    fn pitch_up_twelve_plays_an_octave_up_and_down_twelve_an_octave_down() {
        let shifted = |semitones: i8| Settings {
            pitch: (MOST_SHIFT as i8 + semitones) as u8,
            spread: 0,
            ..Settings::DEFAULT
        };
        let plain = through(shifted(0), 440.0);
        assert!(strength(&plain, 440.0) > strength(&plain, 880.0) * 10.0);
        let up = through(shifted(12), 440.0);
        let (octave, left) = (strength(&up, 880.0), strength(&up, 440.0));
        assert!(octave > 0.1, "{octave}");
        assert!(left < octave * 0.1, "{octave} {left}");
        let down = through(shifted(-12), 440.0);
        let (octave, left) = (strength(&down, 220.0), strength(&down, 440.0));
        assert!(octave > 0.1, "{octave}");
        assert!(left < octave * 0.1, "{octave} {left}");
    }

    #[test]
    fn frozen_it_keeps_playing_a_tone_after_it_stops() {
        let mut granular = granular(Settings::DEFAULT, RATE);
        for frame in 0..RATE as usize * 2 {
            granular.process([sine(frame, 330.0, RATE); 2]);
        }
        let mut frozen = granular.clone();
        frozen.follow(Settings {
            freeze: 1,
            ..Settings::DEFAULT
        });
        let mut free = granular;
        let after = |granular: &mut Granular| -> Vec<f32> {
            (0..RATE as usize * 4)
                .map(|_| granular.process([0.0; 2])[0])
                .collect()
        };
        let (frozen_out, free_out) = (after(&mut frozen), after(&mut free));
        // Long after the tone stopped, the frozen one still plays it, and the other's silent.
        let late = &frozen_out[RATE as usize * 3..];
        assert!(strength(late, 330.0) > 0.1, "{}", strength(late, 330.0));
        assert!(free_out[RATE as usize..].iter().all(|s| s.abs() < 1e-6));
        assert_eq!(frozen.quiet_frames(), u32::MAX);
        // Thawed, the silence coming in is recorded over it, and it goes quiet.
        frozen.follow(Settings::DEFAULT);
        let thawed = after(&mut frozen);
        assert!(thawed[RATE as usize * 3..].iter().all(|s| s.abs() < 1e-6));
    }

    /// The biggest jump between frames from a tone through it, after two seconds, with the settings
    /// switching between two every so many frames, as a knob or an LFO steps them block by block,
    /// or held at either.
    fn biggest_jump(from: Settings, to: Settings, moving: bool, every: usize) -> f32 {
        let mut granular = granular(from, RATE);
        let mut last = [0.0_f32; 2];
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize * 4 {
            let late = frame > RATE as usize * 2;
            if late {
                let now = if moving && frame / every % 2 == 1 {
                    to
                } else {
                    from
                };
                granular.follow(now);
            }
            let out = granular.process([sine(frame, 220.0, RATE); 2]);
            if late {
                most = most.max((out[0] - last[0]).abs().max((out[1] - last[1]).abs()));
            }
            last = out;
        }
        most
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let plain = Settings::DEFAULT;
        let pairs = [
            (
                "Time",
                Settings { time: 0, ..plain },
                Settings {
                    time: 0xFF,
                    ..plain
                },
            ),
            (
                "Size",
                Settings {
                    size: 0x60,
                    ..plain
                },
                Settings {
                    size: 0xFF,
                    ..plain
                },
            ),
            (
                "Size reversed",
                Settings { size: 0, ..plain },
                Settings {
                    size: 0x50,
                    ..plain
                },
            ),
            (
                "Pitch",
                Settings { pitch: 0, ..plain },
                Settings {
                    pitch: MOST_SHIFT * 2,
                    ..plain
                },
            ),
            (
                "Shape",
                Settings { shape: 0, ..plain },
                Settings {
                    shape: 0xFF,
                    ..plain
                },
            ),
            (
                "Density",
                Settings {
                    density: 0x00,
                    ..plain
                },
                Settings {
                    density: 0xFF,
                    ..plain
                },
            ),
            (
                "Spread",
                Settings { spread: 0, ..plain },
                Settings {
                    spread: 0xFF,
                    ..plain
                },
            ),
            (
                "Freeze",
                Settings { freeze: 0, ..plain },
                Settings { freeze: 1, ..plain },
            ),
            (
                "Random",
                Settings { random: 0, ..plain },
                Settings {
                    random: 0xFF,
                    ..plain
                },
            ),
        ];
        for (name, from, to) in pairs {
            // `Freeze` is a switch, which a step flips rather than an LFO block by block. Flipped
            // that fast, what it keeps to join the recording back up is too short to fade over, so
            // it's flipped every sixteenth at 120 BPM.
            let every = if name == "Freeze" {
                RATE as usize / 8
            } else {
                64
            };
            let still =
                biggest_jump(from, from, false, every).max(biggest_jump(to, to, false, every));
            let moving = biggest_jump(from, to, true, every);
            assert!(
                moving < still * 1.25,
                "{name}: {moving} moving, {still} still"
            );
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for rate in [44_100, 192_000] {
            for time in [0x00, 0xFF] {
                for size in [0x00, 0x55, 0xFF] {
                    for pitch in [0, MOST_SHIFT * 2] {
                        for density in [0x00, 0x80, 0xFF] {
                            for (shape, spread, freeze, random) in [
                                (0x00, 0x00, 0, 0x00),
                                (0xFF, 0xFF, 1, 0xFF),
                                (0x80, 0xFF, 0, 0xFF),
                            ] {
                                let settings = Settings {
                                    time,
                                    size,
                                    pitch,
                                    shape,
                                    density,
                                    spread,
                                    freeze: 0,
                                    random,
                                    ..Settings::DEFAULT
                                };
                                let mut granular = granular(settings, rate);
                                let quarter = rate as usize / 4;
                                for frame in 0..rate as usize * 2 {
                                    if frame == rate as usize {
                                        granular.follow(Settings { freeze, ..settings });
                                    }
                                    // Full-scale noise, a full-scale square, DC and silence.
                                    let level = match frame / quarter % 4 {
                                        0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                        1 => {
                                            if frame / 50 % 2 == 0 {
                                                1.0
                                            } else {
                                                -1.0
                                            }
                                        }
                                        2 => 1.0,
                                        _ => 0.0,
                                    };
                                    let out = granular.process([level, -level * 0.5]);
                                    assert!(
                                        out.iter().all(|s| s.is_finite() && s.abs() <= HEADROOM),
                                        "{settings:?} {rate}: {out:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn however_many_overlap_it_stays_near_full_scale() {
        // The densest, longest grains of a full-scale tone.
        let mut granular = granular(
            Settings {
                density: 0x00,
                size: 0xFF,
                spread: 0,
                ..Settings::DEFAULT
            },
            RATE,
        );
        let mut loudest = 0.0_f32;
        for frame in 0..RATE as usize * 6 {
            let out = granular.process([sine(frame, 110.0, RATE) * 2.0; 2]);
            if frame > RATE as usize {
                loudest = loudest.max(out[0].abs()).max(out[1].abs());
            }
        }
        assert!(loudest < CEILING * 1.5, "{loudest}");
    }

    #[test]
    fn its_line_holds_four_seconds_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let granular = Granular::new(rate);
            assert!(
                granular.frames() as f32 >= GRAIN_BUFFER_SECONDS * rate as f32,
                "{rate}"
            );
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut granular = granular(Settings::DEFAULT, RATE);
        for frame in 0..RATE as usize {
            granular.process([sine(frame, 440.0, RATE); 2]);
        }
        granular.clear();
        for _ in 0..RATE {
            assert_eq!(granular.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
