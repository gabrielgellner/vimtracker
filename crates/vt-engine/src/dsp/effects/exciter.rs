//! An exciter, after the Aphex Aural Exciter as Jatin Chowdhury models it: the highs of the sound,
//! driven, through a `tanh` whose output is turned up and down by a level detector with a diode's
//! curve, so the louder the highs, the more harmonics are made of them. What it makes is added to
//! the sound, for air on hats, vocals and a whole mix. It all runs at twice the rate, as the
//! harmonics would otherwise alias.
//!
//! Adapted from Surge XT's `src/common/dsp/effects/chowdsp/ExciterEffect.cpp` and
//! `exciter/LevelDetector.h` at <https://github.com/surge-synthesizer/surge>, with two of its
//! slips put right: Surge's right side reads the left's detector (`ExciterEffect.h:75`), so one
//! detector is stepped twice a sample, where here each side has its own; and Surge works out its
//! detector's times for the normal rate and runs it at twice that, so they're half what its knobs
//! say, where here they're worked out for the rate it runs at.
//!
//! Surge XT - a free and open source hybrid synthesizer, built by Surge Synth Team.
//! Copyright 2018-2024, various authors, as described in the GitHub transaction log.
//!
//! Surge XT is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later). The
//! license is found in the "LICENSE" file in the root of this repository, or at
//! <https://www.gnu.org/licenses/gpl-3.0.en.html>.

use std::f32::consts::{FRAC_1_SQRT_2, PI};

use vt_model::insert::{
    Exciter as Settings, exciter_attack_ms, exciter_release_ms, exciter_tone_hz,
};

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::sides::{SameBits, Sides};
use crate::dsp::blocks::smoother::Smoothed;

/// The diode's curve, `25 β (exp(α x) - 1)`, with Chowdhury's `α = 0.05 / 0.0259` and `β = 0.2`.
const DIODE_ALPHA: f32 = 0.05 / 0.0259;
const DIODE_SCALE: f32 = 25.0 * 0.2;
/// How far the highs going into the diode can go, where its curve is already 30 times what's in:
/// past it the exponential would make a level of millions, so it's held, and a NaN with it.
const DIODE_EDGE: f32 = 1.0;
/// How far the high-pass's output can go, far past what full drive makes of a sound at full scale.
const HEADROOM: f32 = 64.0;
/// How far what it makes can go, back at the normal rate, 12 dB over full scale: the diode's edge
/// still lets it make 30 times the sound at full drive, and the way down can ring past that.
/// Nothing it makes at sensible settings gets near it.
const MOST_WET: f32 = 4.0;
/// How hard `Drive` drives at most, as Surge's.
const MOST_DRIVE: f32 = 8.0;
/// What `Drive` keeps of itself with `Tone` at its lowest, as more of the sound gets through.
const DRIVE_MAKEUP: f32 = 0.2;

/// A high-pass biquad's coefficients, the RBJ cookbook's at a Q of 0.707, as Surge's `coeff_HP`.
#[derive(Debug, Clone, Copy, Default)]
struct HighPass {
    b0: f32,
    b1: f32,
    a1: f32,
    a2: f32,
}

impl HighPass {
    fn new(hertz: f32, rate: f32) -> Self {
        let omega = 2.0 * PI * (hertz / rate).min(0.48);
        let alpha = omega.sin() / (2.0 * FRAC_1_SQRT_2);
        let cos = omega.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// What one side keeps.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Side {
    oversampler: Oversampler,
    /// The high-pass's state, in transposed direct form II.
    held: [f32; 2],
    /// The detector's level.
    level: f32,
}

impl SameBits for Side {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self.oversampler.same_bits(&other.oversampler)
            && self.held.same_bits(&other.held)
            && self.level.same_bits(&other.level)
    }
}

/// The stereo exciter, all wet: what it makes, to be added to the sound.
#[derive(Debug, Clone)]
pub struct Exciter {
    sides: Sides<Side>,
    drive: Smoothed,
    tone: Smoothed,
    /// The detector's times, in ms.
    attack_ms: f32,
    release_ms: f32,
    /// The rate and tone the high-pass and the detector are worked out for.
    rate: u32,
    tuned: f32,
    high_pass: HighPass,
    /// How much of the detector's level stays each sample at twice the rate, rising and falling.
    attack: f32,
    release: f32,
}

impl Default for Exciter {
    fn default() -> Self {
        let mut exciter = Self {
            sides: Sides::default(),
            drive: Smoothed::default(),
            tone: Smoothed::default(),
            attack_ms: 0.0,
            release_ms: 0.0,
            rate: 0,
            tuned: f32::NAN,
            high_pass: HighPass::default(),
            attack: 0.0,
            release: 0.0,
        };
        exciter.follow(Settings::DEFAULT);
        exciter.settle();
        exciter
    }
}

impl Exciter {
    /// Take the settings.
    pub fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        let tone = share(settings.tone);
        // Surge's: Chowdhury's curve on the knob, and less of it as `Tone` lets more through.
        self.drive.target =
            MOST_DRIVE * share(settings.drive).powf(1.5) * DRIVE_MAKEUP.powf(1.0 - tone);
        self.tone.target = tone;
        let times = (
            exciter_attack_ms(f32::from(settings.attack)),
            exciter_release_ms(f32::from(settings.release)),
        );
        if times != (self.attack_ms, self.release_ms) {
            (self.attack_ms, self.release_ms) = times;
            // Worked out again at the next frame.
            self.rate = 0;
        }
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.drive.settle();
        self.tone.settle();
    }

    /// One frame in, and what it makes of it out, to be added to the sound. `same` says the frame's
    /// sides are the same, bit for bit (decision 147).
    #[inline]
    pub fn process(&mut self, input: [f32; 2], same: bool, smoothing: f32, rate: u32) -> [f32; 2] {
        let drive = self.drive.next(smoothing);
        let tone = self.tone.next(smoothing);
        if rate != self.rate {
            self.rate = rate;
            self.tuned = f32::NAN;
            // The ballistics' `exp(-2π 1000 / (rate ms))`, at the rate the detector runs at.
            let twice = 2.0 * rate as f32;
            let constant = |ms: f32| (-2.0 * PI * 1000.0 / (twice * ms)).exp();
            self.attack = constant(self.attack_ms);
            self.release = constant(self.release_ms);
        }
        if tone != self.tuned {
            self.tuned = tone;
            let hertz = exciter_tone_hz(tone * 255.0);
            self.high_pass = HighPass::new(hertz, 2.0 * rate as f32);
        }
        let HighPass { b0, b1, a1, a2 } = self.high_pass;
        let (attack, release) = (self.attack, self.release);
        self.sides.process(input, same, |side, input| {
            let Side {
                oversampler,
                held,
                level,
            } = side;
            let made = oversampler.process(input * drive, |x| {
                // Held, as it goes round, so a NaN dies away rather than staying.
                let high = within(b0 * x + held[0], -HEADROOM, HEADROOM);
                held[0] = b1 * x - a1 * high + held[1];
                held[1] = b0 * x - a2 * high;
                let into = within(high, -DIODE_EDGE, DIODE_EDGE);
                let rectified = DIODE_SCALE * ((DIODE_ALPHA * into).exp() - 1.0);
                let keep = if rectified > *level { attack } else { release };
                *level = rectified + keep * (*level - rectified);
                high.tanh() * *level
            });
            within(made, -MOST_WET, MOST_WET)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    const RATE: u32 = 48_000;

    fn exciter(settings: Settings) -> Exciter {
        let mut exciter = Exciter::default();
        exciter.follow(settings);
        exciter.settle();
        exciter
    }

    fn sine(hertz: f32, level: f32, frame: usize) -> f32 {
        (frame as f32 / RATE as f32 * hertz * TAU).sin() * level
    }

    /// The loudest of what it makes of a sine on both sides, once it's settled.
    fn made(settings: Settings, hertz: f32, level: f32) -> f32 {
        let mut exciter = exciter(settings);
        (0..RATE as usize / 2)
            .map(|frame| exciter.process([sine(hertz, level, frame); 2], true, 1.0, RATE)[0])
            .skip(RATE as usize / 4)
            .fold(0.0, |most, sample| most.max(sample.abs()))
    }

    /// How strong a frequency is in a sound, by Goertzel's algorithm.
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

    #[test]
    fn it_excites_the_highs_and_leaves_the_lows() {
        let low = made(Settings::DEFAULT, 100.0, 0.5);
        let high = made(Settings::DEFAULT, 5_000.0, 0.5);
        assert!(high > 0.1, "{high}");
        assert!(low < high * 0.01, "{low} {high}");
        // `Tone` up, a sine under it is left alone.
        let toned = Settings {
            tone: 0xFF,
            ..Settings::DEFAULT
        };
        assert!(made(toned, 3_000.0, 0.5) < made(Settings::DEFAULT, 3_000.0, 0.5) * 0.2);
    }

    #[test]
    fn what_it_makes_has_the_harmonics_and_grows_faster_than_the_sound() {
        let mut exciter = exciter(Settings::DEFAULT);
        let out: Vec<f32> = (0..RATE as usize)
            .map(|frame| exciter.process([sine(3_000.0, 0.5, frame); 2], true, 1.0, RATE)[0])
            .skip(RATE as usize / 2)
            .collect();
        // `tanh` makes odd harmonics, and the diode's lopsided level even ones.
        assert!(strength(&out, 9_000.0) > strength(&out, 3_000.0) * 0.01);
        assert!(strength(&out, 6_000.0) > strength(&out, 3_000.0) * 0.001);
        // The level detector turns it up as the highs get louder, so twice as loud makes much
        // more than twice as much.
        let quiet = made(Settings::DEFAULT, 5_000.0, 0.1);
        let loud = made(Settings::DEFAULT, 5_000.0, 0.2);
        assert!(loud > quiet * 3.0, "{quiet} {loud}");
    }

    #[test]
    fn each_side_has_its_own_detector() {
        // Surge's right side reads the left's detector, so loud highs on the left alone would
        // turn the right's up too.
        let mut exciter = exciter(Settings::DEFAULT);
        let mut right = 0.0_f32;
        for frame in 0..RATE as usize / 2 {
            let quiet = sine(5_000.0, 0.01, frame);
            let out = exciter.process([sine(5_000.0, 0.5, frame), quiet], false, 1.0, RATE);
            right = right.max(out[1].abs());
        }
        assert!(
            right < made(Settings::DEFAULT, 5_000.0, 0.01) * 1.01,
            "{right}"
        );
    }

    #[test]
    fn its_detector_takes_the_time_set_at_twice_the_rate() {
        // Twice the rate, a step's rise reaches 1 - 1/e of the way in the attack's ms, give or
        // take what the high-pass takes.
        for rate in [44_100, 48_000, 96_000] {
            let mut exciter = exciter(Settings::DEFAULT);
            exciter.process([0.0; 2], true, 1.0, rate);
            let twice = 2.0 * rate as f32;
            let attack = (-2.0 * PI * 1000.0 / (twice * exciter_attack_ms(128.0))).exp();
            assert!((exciter.attack - attack).abs() < 1e-6);
            // Chowdhury's constant falls by `exp(-2π)` over the time set, at the rate it runs at.
            let samples = twice * exciter_attack_ms(128.0) / 1000.0;
            assert!((exciter.attack.powf(samples) - (-2.0 * PI).exp()).abs() < 1e-4);
        }
    }

    #[test]
    fn a_nan_in_its_high_pass_dies_away() {
        let mut exciter = exciter(Settings::DEFAULT);
        exciter.process([0.5; 2], false, 1.0, RATE);
        exciter.sides.process([0.0; 2], false, |side, _| {
            side.held = [f32::NAN; 2];
            0.0
        });
        let mut last = [1.0; 2];
        for _ in 0..RATE {
            last = exciter.process([0.0; 2], false, 1.0, RATE);
            assert!(last.iter().all(|s| s.is_finite()));
        }
        assert!(last.iter().all(|s| s.abs() < 1e-6), "{last:?}");
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for drive in [0x00, 0x80, 0xFF] {
            for tone in [0x00, 0xFF] {
                for (attack, release) in [(0x00, 0x00), (0xFF, 0xFF)] {
                    let settings = Settings {
                        drive,
                        tone,
                        attack,
                        release,
                        ..Settings::DEFAULT
                    };
                    for rate in [44_100, 192_000] {
                        let mut exciter = exciter(settings);
                        for frame in 0..rate as usize / 2 {
                            // A square 12 dB over full scale, with a frame of silence where the
                            // slot takes out a NaN or an infinity.
                            let input = match frame {
                                1000 | 2000 => 0.0,
                                _ if (frame / 7) % 2 == 0 => 4.0,
                                _ => -4.0,
                            };
                            let out = exciter.process([input, -input * 0.5], false, 1.0, rate);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() <= MOST_WET),
                                "{settings:?} {rate}: {out:?}"
                            );
                        }
                        // And it comes back to nothing.
                        let mut last = [1.0; 2];
                        for _ in 0..rate {
                            last = exciter.process([0.0; 2], false, 1.0, rate);
                        }
                        assert!(
                            last.iter().all(|s| s.abs() < 1e-6),
                            "{settings:?}: {last:?}"
                        );
                    }
                }
            }
        }
    }
}
