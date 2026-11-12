//! A rotary speaker, a Leslie: an organ's cabinet with a horn spinning over a crossover at 800 Hz
//! and a drum spinning under it. The horn's sound reaches each of two microphones by a path that
//! grows and shrinks as it turns, so its pitch wavers by Doppler shift, and it's loudest as it
//! points at one; the drum's low middle swells and fades as it turns, and the lowest bass doesn't
//! move. `Speed` switches both between slow and fast, and they take time to get there, as heavy
//! things do: the horn about a second, the drum several.
//!
//! Adapted from sst-effects' `effects/RotarySpeaker.h`: the crossover and the bass under 200 Hz,
//! the horn's place on a circle with microphones at `(-1, -2)` and `(1, -2)`, the delay that
//! follows the distance to each, and the loudness that follows where it points. sst's speeds jump
//! when they're changed; here each rotor's speed eases to the new one at its own pace, faster to
//! speed up than to slow down for the horn, and the other way round for the heavy drum. sst moves
//! the drum's swell the same on both sides; here it's heard from the same two microphones as the
//! horn, so it moves across too. Its sinc reads are a cubic here, and its eight waveshapers one
//! soft clip.
//!
//! sst-effects - an open source library of audio effects built by Surge Synth Team.
//! Copyright 2018-2023, various authors, as described in the GitHub transaction log.
//!
//! sst-effects is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later).
//! The license is found in the "LICENSE" file in the root of this repository, or at
//! <https://www.gnu.org/licenses/gpl-3.0.en.html>.

use std::f32::consts::{FRAC_1_SQRT_2, TAU};

use vt_model::insert::{FAST_SPEED, Rotary as Settings};

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::sine_table::SINE;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};

/// The horn's and the drum's speeds, slow and fast, in turns a second, about a Leslie 122's.
const HORN_HZ: [f32; 2] = [0.8, 6.7];
const DRUM_HZ: [f32; 2] = [0.7, 5.8];
/// How long each takes to get most of the way to a new speed, speeding up and slowing down, in
/// seconds, at `Inertia` `80`: a time constant, so the horn has all but arrived in about a second
/// and the drum in about five.
const HORN_SECONDS: [f32; 2] = [0.25, 0.4];
const DRUM_SECONDS: [f32; 2] = [1.2, 1.0];
/// Where the horn takes over from the drum, and the bass under the drum's swell, in Hz, as sst's.
const CROSSOVER_HZ: f32 = 800.0;
const BASS_HZ: f32 = 200.0;
/// How far the horn's delay goes at full `Doppler`, in seconds a unit of distance, as sst's.
const DOPPLER_SECONDS: f32 = 0.0018;
/// How deep the swell goes at full `Tremolo`, as sst's.
const MOST_TREMOLO: f32 = 0.6;
/// The furthest a microphone is from the horn, in units of its circle: `√10 + 1` at most.
const FURTHEST: f32 = 4.2;
/// How often where the rotors are is worked out, in frames. In between it's ramped.
const CONTROL_FRAMES: u32 = 16;
/// How far the filters' states can go, so a NaN dies away in them rather than staying (#461).
const HEADROOM: f32 = 16.0;
/// How much of `Drive` it takes to drive fully into the soft clip, from clean at `00`, and where
/// sst starts turning it down to make up.
const DRIVE_EASED: f32 = 0.18;

/// A low-pass biquad's coefficients, the RBJ cookbook's at a Q of 0.707, as sst's `coeff_LP2B`.
#[derive(Debug, Clone, Copy)]
struct LowPass {
    b0: f32,
    b1: f32,
    a1: f32,
    a2: f32,
}

impl LowPass {
    fn new(hertz: f32, rate: u32) -> Self {
        let omega = TAU * hertz / rate as f32;
        let alpha = omega.sin() / (2.0 * FRAC_1_SQRT_2);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// One sample through it, with its state in transposed direct form II.
    #[inline]
    fn step(&self, held: &mut [f32; 2], x: f32) -> f32 {
        let y = within(self.b0 * x + held[0], -HEADROOM, HEADROOM);
        held[0] = self.b1 * x - self.a1 * y + held[1];
        held[1] = self.b0 * x - self.a2 * y;
        y
    }
}

/// A place on the circle's sine and cosine, in turns, read from Plaits' table between its steps.
/// A sine and cosine of the same angle can be worked out together in a release build and come
/// out a bit apart from a debug build's, which a table can't.
#[inline]
fn sine_cosine(turns: f32) -> (f32, f32) {
    let at = turns.rem_euclid(1.0) * 512.0;
    let whole = (at as usize).min(511);
    let between = at - whole as f32;
    let read = |index: usize| SINE[index] + (SINE[index + 1] - SINE[index]) * between;
    (read(whole), read(whole + 128))
}

/// How each microphone hears a rotor at a place on its circle, in turns: how far its sound goes
/// to each, and how much it points at each, from -1 away to 1 straight at it.
#[inline]
fn heard(turns: f32) -> [[f32; 2]; 2] {
    let (i, r) = sine_cosine(turns);
    [-1.0_f32, 1.0].map(|x| {
        let (to_r, to_i) = (x - r, -2.0 - i);
        let distance = (to_r * to_r + to_i * to_i).sqrt();
        [distance, (to_r * r + to_i * i) / distance]
    })
}

/// A value ramped in a straight line between the control frames.
#[derive(Debug, Clone, Copy, Default)]
struct Ramp {
    now: f32,
    step: f32,
}

impl Ramp {
    #[inline]
    fn toward(&mut self, target: f32, settle: bool) {
        if settle {
            self.now = target;
            self.step = 0.0;
        } else {
            self.step = (target - self.now) / CONTROL_FRAMES as f32;
        }
    }

    #[inline]
    fn next(&mut self) -> f32 {
        self.now += self.step;
        self.now
    }
}

/// The rotary speaker's line and rotors.
#[derive(Debug, Clone)]
pub struct Rotary {
    /// What goes to the horn, read back at each microphone's distance.
    line: Vec<f32>,
    mask: usize,
    write: usize,
    crossover: LowPass,
    bass: LowPass,
    crossed: [f32; 2],
    bassed: [f32; 2],
    /// Where each rotor is, in turns, and how fast it's turning, in turns a second.
    horn: f32,
    drum: f32,
    horn_hz: f32,
    drum_hz: f32,
    fast: bool,
    inertia: f32,
    doppler: Smoothed,
    tremolo: Smoothed,
    drive: Smoothed,
    width: Smoothed,
    /// The horn's delay and loudness at each microphone, and the drum's swell at each.
    delays: [Ramp; 2],
    horn_gains: [Ramp; 2],
    drum_gains: [Ramp; 2],
    countdown: u32,
    rate: u32,
    smoothing: f32,
}

impl Rotary {
    /// A line long enough for the horn's furthest delay at this rate.
    pub fn new(rate: u32) -> Self {
        let longest = DOPPLER_SECONDS * FURTHEST * rate as f32;
        let size = (longest.ceil() as usize + 4).next_power_of_two();
        let mut rotary = Self {
            line: vec![0.0; size],
            mask: size - 1,
            write: 0,
            crossover: LowPass::new(CROSSOVER_HZ, rate),
            bass: LowPass::new(BASS_HZ, rate),
            crossed: [0.0; 2],
            bassed: [0.0; 2],
            horn: 0.0,
            drum: 0.0,
            horn_hz: HORN_HZ[0],
            drum_hz: DRUM_HZ[0],
            fast: false,
            inertia: 0.5,
            doppler: Smoothed::default(),
            tremolo: Smoothed::default(),
            drive: Smoothed::default(),
            width: Smoothed::default(),
            delays: [Ramp::default(); 2],
            horn_gains: [Ramp::default(); 2],
            drum_gains: [Ramp::default(); 2],
            countdown: 0,
            rate,
            smoothing: coefficient(rate),
        };
        rotary.follow(Settings::DEFAULT);
        rotary.settle();
        rotary
    }

    /// The longest anything stays in it, in frames.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    /// Forget what's in it, and start the rotors from the front.
    pub fn clear(&mut self) {
        self.line.fill(0.0);
        self.crossed = [0.0; 2];
        self.bassed = [0.0; 2];
        self.horn = 0.0;
        self.drum = 0.0;
        self.settle();
    }

    /// Take what's set now. A new speed is eased to.
    pub fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.fast = settings.fast == FAST_SPEED;
        self.inertia = share(settings.inertia);
        self.doppler.target = share(settings.doppler);
        self.tremolo.target = share(settings.tremolo) * MOST_TREMOLO;
        self.drive.target = share(settings.drive);
        self.width.target = share(settings.width);
    }

    /// Jump to what's set, the rotors at their speed, as it's only just started.
    pub fn settle(&mut self) {
        self.doppler.settle();
        self.tremolo.settle();
        self.drive.settle();
        self.width.settle();
        let at = usize::from(self.fast);
        self.horn_hz = HORN_HZ[at];
        self.drum_hz = DRUM_HZ[at];
        self.control(true);
        self.countdown = CONTROL_FRAMES;
    }

    /// How fast each rotor turns now, in turns a second, for the tests.
    #[cfg(test)]
    fn speeds(&self) -> [f32; 2] {
        [self.horn_hz, self.drum_hz]
    }

    /// The rotors' speeds and places a control's frames on, and where the ramps go next.
    fn control(&mut self, settle: bool) {
        let seconds = CONTROL_FRAMES as f32 / self.rate as f32;
        let at = usize::from(self.fast);
        // `Inertia` at `00` jumps, at `80` is a Leslie's, at `FF` twice as slow.
        let inertia = 2.0 * self.inertia;
        let ease = |hertz: f32, target: f32, times: [f32; 2]| {
            let time = times[usize::from(target < hertz)] * inertia;
            if time <= 0.0 {
                target
            } else {
                target + (hertz - target) * (-seconds / time).exp()
            }
        };
        self.horn_hz = ease(self.horn_hz, HORN_HZ[at], HORN_SECONDS);
        self.drum_hz = ease(self.drum_hz, DRUM_HZ[at], DRUM_SECONDS);
        if !settle {
            self.horn = (self.horn + self.horn_hz * seconds).fract();
            self.drum = (self.drum + self.drum_hz * seconds).fract();
        }
        let horn = heard(self.horn);
        let drum = heard(self.drum);
        let doppler = DOPPLER_SECONDS * self.rate as f32 * self.doppler.value;
        let tremolo = self.tremolo.value;
        for side in 0..2 {
            let [distance, facing] = horn[side];
            self.delays[side].toward(distance * doppler, settle);
            self.horn_gains[side].toward(1.0 - tremolo + tremolo * facing, settle);
            self.drum_gains[side].toward(1.0 - tremolo + tremolo * drum[side][1], settle);
        }
    }

    /// The horn's line a number of frames back, read between samples through four of them.
    #[inline]
    fn read(&self, frames: f32) -> f32 {
        let frames = frames.clamp(1.0, (self.mask - 3) as f32);
        let whole = frames as usize;
        let between = frames - whole as f32;
        let at = |back: usize| self.line[self.write.wrapping_sub(back) & self.mask];
        let (newer, current, older, oldest) =
            (at(whole - 1), at(whole), at(whole + 1), at(whole + 2));
        // A cubic through the four, after Hermite, so a whole number of frames reads exactly.
        let slope_in = (older - newer) * 0.5;
        let slope_out = (oldest - current) * 0.5;
        let difference = current - older;
        let a = slope_in + difference + slope_out + difference;
        let b = -(difference + a + slope_in);
        ((a * between + b) * between + slope_in) * between + current
    }

    /// One frame in, for the pool's own use.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input, self.smoothing)
    }

    /// Feed one frame in and get the cabinet's sound out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32) -> [f32; 2] {
        self.doppler.next(smoothing);
        self.tremolo.next(smoothing);
        let drive = self.drive.next(smoothing);
        let width = self.width.next(smoothing);
        if self.countdown == 0 {
            self.control(false);
            self.countdown = CONTROL_FRAMES;
        }
        self.countdown -= 1;
        let mono = 0.5 * (input[0] + input[1]);
        // The amplifier's drive, after sst's: up to 16 times into a soft clip, turned down again
        // as it gets harder, and eased in over the knob's first part so it starts from clean.
        let driven = if drive > 0.0 {
            let pushed = 1.0 + drive * drive * 15.0;
            let made_up = 1.0 + (drive - DRIVE_EASED).max(0.0) * 4.0;
            let shaped = within(mono * pushed, -HEADROOM, HEADROOM).tanh() / made_up;
            mono + (shaped - mono) * (drive / DRIVE_EASED).min(1.0)
        } else {
            mono
        };
        let lower = self.crossover.step(&mut self.crossed, driven);
        self.write = (self.write + 1) & self.mask;
        self.line[self.write] = driven - lower;
        let bass = self.bass.step(&mut self.bassed, lower);
        let middle = lower - bass;
        let mut out = [0.0; 2];
        for (side, out) in out.iter_mut().enumerate() {
            let delay = self.delays[side].next();
            let horn = self.read(delay) * self.horn_gains[side].next();
            *out = horn + bass + middle * self.drum_gains[side].next();
        }
        // Width, as the delay's: what differs between the sides, turned down toward mono.
        let (mid, apart) = (0.5 * (out[0] + out[1]), 0.5 * (out[0] - out[1]) * width);
        [mid + apart, mid - apart]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn rotary(settings: Settings) -> Rotary {
        let mut rotary = Rotary::new(RATE);
        rotary.follow(settings);
        rotary.settle();
        rotary
    }

    fn sine(hertz: f32, frame: usize) -> f32 {
        (frame as f32 / RATE as f32 * hertz * TAU).sin() * 0.5
    }

    /// A sine through it for a second and a half, with the last second of both sides.
    fn played(rotary: &mut Rotary, hertz: f32) -> Vec<[f32; 2]> {
        (0..RATE as usize * 3 / 2)
            .map(|frame| rotary.process([sine(hertz, frame); 2], 1.0))
            .skip(RATE as usize / 2)
            .collect()
    }

    /// How strong a frequency is in a side, by Goertzel's algorithm.
    fn strength(samples: &[f32], hertz: f32) -> f32 {
        let coefficient = 2.0 * (TAU * hertz / RATE as f32).cos();
        let (mut before, mut last) = (0.0_f32, 0.0_f32);
        for sample in samples {
            let next = sample + coefficient * last - before;
            before = last;
            last = next;
        }
        (before * before + last * last - coefficient * before * last).sqrt() / samples.len() as f32
            * 2.0
    }

    /// The loudest and quietest a side is, over windows of so many frames.
    fn swing(samples: &[f32], window: usize) -> (f32, f32) {
        samples
            .chunks(window)
            .fold((0.0_f32, f32::MAX), |(most, least), chunk| {
                let loudest = chunk.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                (most.max(loudest), least.min(loudest))
            })
    }

    #[test]
    fn its_rotors_ease_to_a_new_speed_the_drum_slower_than_the_horn() {
        let mut slow = rotary(Settings::DEFAULT);
        assert_eq!(slow.speeds(), [HORN_HZ[0], DRUM_HZ[0]]);
        slow.follow(Settings {
            fast: FAST_SPEED,
            ..Settings::DEFAULT
        });
        let mut at = Vec::new();
        for second in 0..8 {
            for _ in 0..RATE {
                slow.process([0.1; 2], 1.0);
            }
            at.push((second, slow.speeds()));
        }
        // Neither jumps: after a second the horn is nearly there and the drum well short; after
        // eight both are.
        let share = |hertz: f32, of: [f32; 2]| (hertz - of[0]) / (of[1] - of[0]);
        let [horn, drum] = at[0].1;
        assert!(
            share(horn, HORN_HZ) > 0.9 && share(horn, HORN_HZ) < 1.0,
            "{horn}"
        );
        assert!(share(drum, DRUM_HZ) < 0.6, "{drum}");
        let [horn, drum] = at[7].1;
        assert!(
            share(horn, HORN_HZ) > 0.999 && share(drum, DRUM_HZ) > 0.99,
            "{horn} {drum}"
        );

        // With no inertia it jumps, as sst's does.
        let mut jumping = rotary(Settings {
            inertia: 0x00,
            ..Settings::DEFAULT
        });
        jumping.follow(Settings {
            fast: FAST_SPEED,
            inertia: 0x00,
            ..Settings::DEFAULT
        });
        for _ in 0..CONTROL_FRAMES + 1 {
            jumping.process([0.1; 2], 1.0);
        }
        assert_eq!(jumping.speeds(), [HORN_HZ[1], DRUM_HZ[1]]);
    }

    #[test]
    fn doppler_wavers_the_horns_pitch() {
        let still = Settings {
            fast: FAST_SPEED,
            tremolo: 0x00,
            doppler: 0x00,
            ..Settings::DEFAULT
        };
        let moving = Settings {
            doppler: 0xFF,
            ..still
        };
        let left = |settings: Settings| -> Vec<f32> {
            played(&mut rotary(settings), 3_000.0)
                .iter()
                .map(|frame| frame[0])
                .collect()
        };
        let (still, moving) = (left(still), left(moving));
        assert!(strength(&still, 3_000.0) > 0.45);
        assert!(
            strength(&moving, 3_000.0) < strength(&still, 3_000.0) * 0.5,
            "{} {}",
            strength(&moving, 3_000.0),
            strength(&still, 3_000.0)
        );
    }

    #[test]
    fn tremolo_swells_the_highs_and_middle_and_hardly_the_bass() {
        let settings = Settings {
            fast: FAST_SPEED,
            doppler: 0x00,
            tremolo: 0xFF,
            ..Settings::DEFAULT
        };
        // How far down from its loudest a sine swells, in windows of two cycles or 5 ms.
        let dips = |hertz: f32| {
            let out = played(&mut rotary(settings), hertz);
            let left: Vec<f32> = out.iter().map(|frame| frame[0]).collect();
            let (most, least) = swing(&left, ((2.0 * RATE as f32 / hertz) as usize).max(240));
            least / most
        };
        assert!(dips(3_000.0) < 0.5, "{}", dips(3_000.0));
        assert!(dips(400.0) < 0.5, "{}", dips(400.0));
        // The deep bass is under the drum's swell, but for what the crossovers' phases leave of
        // it, as sst's.
        assert!(dips(40.0) > 0.55, "{}", dips(40.0));
        // The sides swell apart, as the horn points at one and then the other.
        let out = played(&mut rotary(settings), 3_000.0);
        let apart = out
            .chunks(240)
            .map(|chunk| {
                let side = |at: usize| chunk.iter().fold(0.0_f32, |m, f| m.max(f[at].abs()));
                (side(0) - side(1)).abs()
            })
            .fold(0.0_f32, f32::max);
        assert!(apart > 0.2, "{apart}");
        // At no width they're the same.
        let narrow = played(
            &mut rotary(Settings {
                width: 0x00,
                ..settings
            }),
            3_000.0,
        );
        assert!(narrow.iter().all(|frame| frame[0] == frame[1]));
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for fast in [0, FAST_SPEED] {
            for value in [0x00, 0xFF] {
                let settings = Settings {
                    fast,
                    inertia: value,
                    doppler: value,
                    tremolo: value,
                    drive: value,
                    width: value,
                    ..Settings::DEFAULT
                };
                for rate in [44_100, 192_000] {
                    let mut rotary = Rotary::new(rate);
                    rotary.follow(settings);
                    rotary.settle();
                    for frame in 0..rate as usize {
                        // A square 12 dB over full scale, its sides apart.
                        let input = if (frame / 50) % 2 == 0 { 4.0 } else { -4.0 };
                        if frame == rate as usize / 2 {
                            rotary.follow(Settings {
                                fast: FAST_SPEED - fast,
                                ..settings
                            });
                        }
                        let out = rotary.process([input, -input * 0.5], 1.0);
                        assert!(
                            out.iter().all(|s| s.is_finite() && s.abs() < 16.0),
                            "{settings:?} {rate}: {out:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_nan_in_its_line_or_filters_dies_away() {
        let mut rotary = rotary(Settings::DEFAULT);
        rotary.process([0.5; 2], 1.0);
        rotary.line[rotary.write] = f32::NAN;
        rotary.crossed = [f32::NAN; 2];
        rotary.bassed = [f32::NAN; 2];
        let mut last = [1.0; 2];
        for _ in 0..RATE {
            last = rotary.process([0.0; 2], 1.0);
        }
        assert!(last.iter().all(|s| s.abs() < 1e-6), "{last:?}");
    }

    #[test]
    fn it_holds_the_horns_furthest_delay_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let rotary = Rotary::new(rate);
            let furthest = (0..1000)
                .map(|at| heard(at as f32 / 1000.0)[0][0])
                .fold(0.0_f32, f32::max);
            assert!(furthest < FURTHEST);
            assert!(rotary.frames() as f32 > DOPPLER_SECONDS * FURTHEST * rate as f32 + 3.0);
        }
    }
}
