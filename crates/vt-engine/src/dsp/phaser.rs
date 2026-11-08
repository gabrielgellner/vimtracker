//! A stereo phaser, as Vital has it: up to twelve first-order all-passes in a row, whose frequency
//! a triangle sweeps in pitch, with what comes out band-passed, softly clipped and fed back into
//! the front. Mixed half and half with the dry sound, each four all-passes make two notches, and
//! feedback sharpens them into peaks. The two sides sweep a third of a turn apart, for width.
//!
//! Adapted from Vital's `effects/phaser.cpp` and `filters/phaser_filter.h`, GPL 3 or later, with
//! the algorithm taken out of its processor graph and SIMD types:
//!
//! Copyright 2013-2019 Matt Tytel.
//!
//! Vital is free software: you can redistribute it and/or modify it under the terms of the GNU
//! General Public License as published by the Free Software Foundation, either version 3 of the
//! License, or (at your option) any later version.

use std::f32::consts::PI;

use vt_model::insert::{MOST_PHASER_SEMITONES, Phaser as Settings};
use vt_model::lfo::SWEEP_TICKS;

use crate::dsp::filter::soft_clip;
use crate::dsp::smoother::Smoothed;

/// How many all-passes there are at most, and how many each choice of `Stages` adds.
const STAGES: usize = 12;
const STAGES_PER_CHOICE: usize = 4;
/// The lowest frequency it sweeps to, in Hz, and the highest, as a share of the rate.
const LOWEST_HZ: f32 = 10.0;
const HIGHEST_SHARE: f32 = 0.45;
/// How far either side of the all-passes the band on what's fed back reaches, as a ratio.
const CLEAR_RATIO: f32 = 20.0;
/// The most a band filter on what's fed back moves each frame, as Vital has it.
const MOST_CLEAR: f32 = 0.9;
/// How far round the right side's sweep is from the left's, as Vital starts.
const SIDE_APART: f32 = 1.0 / 3.0;
/// How many frames the all-passes' coefficient takes to move in a straight line to the next one
/// worked out. Working one out costs more than the twelve all-passes, and the sweep is slow.
const CONTROL_FRAMES: u32 = 16;

/// A one-pole low-pass, in the trapezoidal form, which an all-pass and the feedback's band are
/// built from.
#[inline]
fn low_pass(state: &mut f32, input: f32, coefficient: f32) -> f32 {
    let delta = coefficient * (input - *state);
    let out = *state + delta;
    *state = out + delta;
    out
}

/// A triangle from -1 to 1, at a place in its turn.
#[inline]
fn triangle(turns: f32) -> f32 {
    let turns = turns - turns.floor();
    4.0 * (turns - 0.5).abs() - 1.0
}

/// One side's all-passes and feedback.
#[derive(Debug, Clone, Copy, Default)]
struct Side {
    stages: [f32; STAGES],
    /// The band on what's fed back: what's above it taken away, then what's below.
    lows: f32,
    highs: f32,
    /// The last that came out of the all-passes, which goes round again.
    out: f32,
}

impl Side {
    /// One sample through, at a coefficient for the all-passes, with the given feedback and how
    /// much of the fourth, eighth and twelfth all-pass's output comes out.
    #[inline]
    fn process(&mut self, input: f32, coefficient: f32, feedback: f32, taps: [f32; 3]) -> f32 {
        let lows = low_pass(
            &mut self.lows,
            self.out,
            (coefficient * CLEAR_RATIO).min(MOST_CLEAR),
        );
        let highs = low_pass(&mut self.highs, lows, coefficient / CLEAR_RATIO);
        let fed_back = soft_clip(feedback * (lows - highs));
        // Vital turns up what goes in as feedback rises, so the level holds.
        let mut through = input * (1.0 + feedback * 0.5) + fed_back;
        let mut out = 0.0;
        for (group, tap) in self
            .stages
            .as_chunks_mut::<STAGES_PER_CHOICE>()
            .0
            .iter_mut()
            .zip(taps)
        {
            for stage in group {
                through -= 2.0 * low_pass(stage, through, coefficient);
            }
            out += through * tap;
        }
        self.out = out;
        out
    }
}

/// A phaser's all-passes and sweep. It's small enough to keep in every slot.
#[derive(Debug, Clone, Copy)]
pub struct Phaser {
    sides: [Side; 2],
    /// Where the sweep is, in turns, and how far it goes each frame.
    phase: f32,
    step: f32,
    /// How far it sweeps either way, and where around, in octaves above `LOWEST_HZ`.
    depth: Smoothed,
    centre: Smoothed,
    feedback: Smoothed,
    /// How much of the fourth, eighth and twelfth all-pass's output comes out, so changing `Stages`
    /// fades from one to another.
    taps: [Smoothed; 3],
    /// Each side's coefficient for the all-passes, how far it moves each frame, and how many
    /// frames until the next is worked out. Without one, the next is jumped to.
    coefficients: Option<[f32; 2]>,
    slopes: [f32; 2],
    countdown: u32,
}

impl Default for Phaser {
    fn default() -> Self {
        let mut phaser = Self {
            sides: [Side::default(); 2],
            phase: 0.0,
            step: 0.0,
            depth: Smoothed::default(),
            centre: Smoothed::default(),
            feedback: Smoothed::default(),
            taps: [Smoothed::default(); 3],
            coefficients: None,
            slopes: [0.0; 2],
            countdown: 0,
        };
        phaser.follow(Settings::DEFAULT, 1.0);
        phaser.settle();
        phaser
    }
}

impl Phaser {
    /// Take what's set now, at a tempo.
    pub fn follow(&mut self, settings: Settings, frames_per_tick: f64) {
        let ticks = SWEEP_TICKS[usize::from(settings.rate).min(SWEEP_TICKS.len() - 1)];
        let frames = f64::from(ticks) * frames_per_tick;
        self.step = if frames >= 1.0 {
            (1.0 / frames) as f32
        } else {
            0.0
        };
        self.depth.target = f32::from(settings.depth) / 255.0 * MOST_PHASER_SEMITONES / 12.0;
        // The centre is the filter's cutoff, from 20 Hz to 20 kHz, here in octaves over the lowest.
        let centre = f32::from(settings.centre) / 255.0 * 1000_f32.log2();
        self.centre.target = centre + (20.0 / LOWEST_HZ).log2();
        self.feedback.target = f32::from(settings.feedback) / 255.0;
        let chosen = usize::from(settings.stages).min(self.taps.len() - 1);
        for (at, tap) in self.taps.iter_mut().enumerate() {
            tap.target = if at == chosen { 1.0 } else { 0.0 };
        }
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.depth.settle();
        self.centre.settle();
        self.feedback.settle();
        for tap in &mut self.taps {
            tap.settle();
        }
        self.coefficients = None;
        self.countdown = 0;
    }

    /// The all-passes' coefficient for a frequency in octaves above the lowest.
    #[inline]
    fn coefficient(octaves: f32, rate: u32) -> f32 {
        let hertz = (LOWEST_HZ * octaves.exp2()).min(rate as f32 * HIGHEST_SHARE);
        let warped = (PI * hertz / rate as f32).tan();
        warped / (1.0 + warped)
    }

    /// One frame in and out of the all-passes, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let depth = self.depth.next(smoothing);
        let centre = self.centre.next(smoothing);
        let feedback = self.feedback.next(smoothing);
        let taps = self.taps.each_mut().map(|tap| tap.next(smoothing));
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        if self.countdown == 0 {
            self.countdown = CONTROL_FRAMES;
            // Where each side will be when the next is worked out.
            let ahead = self.phase + self.step * CONTROL_FRAMES as f32;
            let next: [f32; 2] = std::array::from_fn(|side| {
                let turns = ahead + (side as f32 - 0.5) * SIDE_APART;
                let octaves = (centre + triangle(turns) * depth).max(0.0);
                Self::coefficient(octaves, rate)
            });
            let now = *self.coefficients.get_or_insert(next);
            self.slopes =
                std::array::from_fn(|side| (next[side] - now[side]) / CONTROL_FRAMES as f32);
        }
        self.countdown -= 1;
        let coefficients = self.coefficients.get_or_insert([0.0; 2]);
        let mut out = [0.0; 2];
        for (side, (state, sample)) in self.sides.iter_mut().zip(input).enumerate() {
            coefficients[side] += self.slopes[side];
            out[side] = state.process(sample, coefficients[side], feedback, taps);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::smoother::coefficient;
    use std::f32::consts::TAU;

    const RATE: u32 = 48_000;
    /// 120 BPM, as a tick is a 960th of a beat.
    const FRAMES_PER_TICK: f64 = 48_000.0 / 2.0 / 960.0;

    fn phaser(settings: Settings) -> Phaser {
        let mut phaser = Phaser::default();
        phaser.follow(settings, FRAMES_PER_TICK);
        phaser.settle();
        phaser
    }

    /// Still, with no feedback, so its notches stay where they are.
    fn still(centre: u8, stages: u8) -> Phaser {
        phaser(Settings {
            depth: 0,
            centre,
            feedback: 0,
            stages,
            ..Settings::DEFAULT
        })
    }

    /// How loud a sine comes out with the dry sound mixed in half and half, as the slot starts.
    fn response(phaser: &mut Phaser, hertz: f32) -> f32 {
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize / 5 {
            let sample = (frame as f32 / RATE as f32 * hertz * TAU).sin();
            let wet = phaser.process([sample; 2], 1.0, RATE)[0];
            if frame > RATE as usize / 10 {
                most = most.max(((sample + wet) * 0.5).abs());
            }
        }
        most
    }

    /// The frequency, of some spread evenly in pitch, that comes out quietest.
    fn deepest(centre: u8) -> (f32, f32) {
        (0..60)
            .map(|at| 100.0 * 2_f32.powf(at as f32 / 10.0))
            .map(|hertz| (hertz, response(&mut still(centre, 0), hertz)))
            .fold(
                (0.0, f32::MAX),
                |best, found| {
                    if found.1 < best.1 { found } else { best }
                },
            )
    }

    #[test]
    fn it_makes_notches_that_the_centre_moves() {
        let (low, low_level) = deepest(0x60);
        let (high, high_level) = deepest(0xA0);
        assert!(
            low_level < 0.1 && high_level < 0.1,
            "{low_level} {high_level}"
        );
        assert!(high > low * 2.0, "{low} Hz to {high} Hz");
        // Far from a notch, it comes through as loud as it went in.
        let far = response(&mut still(0x60, 0), 30.0);
        assert!(far > 0.9, "{far}");
    }

    #[test]
    fn more_stages_make_more_notches() {
        let notches = |stages: u8| {
            let levels: Vec<f32> = (0..120)
                .map(|at| 30.0 * 2_f32.powf(at as f32 / 15.0))
                .map(|hertz| response(&mut still(0x80, stages), hertz))
                .collect();
            levels
                .windows(3)
                .filter(|three| three[1] < three[0] && three[1] < three[2] && three[1] < 0.3)
                .count()
        };
        assert_eq!(notches(0), 2);
        assert_eq!(notches(1), 4);
        assert_eq!(notches(2), 6);
    }

    #[test]
    fn its_notches_sweep_and_each_side_its_own_way() {
        let mut phaser = phaser(Settings {
            // A half note, so a sweep takes a second.
            rate: 15,
            depth: 0xFF,
            feedback: 0,
            ..Settings::DEFAULT
        });
        // A steady tone's level rises and falls as the notches pass, at different times each side.
        let mut levels = [[f32::MAX, 0.0_f32]; 2];
        let mut apart = 0.0_f32;
        let window = RATE as usize / 100;
        let mut peaks = [0.0_f32; 2];
        for frame in 0..RATE as usize * 2 {
            let sample = (frame as f32 / RATE as f32 * 1_000.0 * TAU).sin();
            let wet = phaser.process([sample; 2], 1.0, RATE);
            for side in 0..2 {
                peaks[side] = peaks[side].max(((sample + wet[side]) * 0.5).abs());
            }
            if frame % window == window - 1 {
                for side in 0..2 {
                    levels[side][0] = levels[side][0].min(peaks[side]);
                    levels[side][1] = levels[side][1].max(peaks[side]);
                }
                apart = apart.max((peaks[0] - peaks[1]).abs());
                peaks = [0.0; 2];
            }
        }
        for [quietest, loudest] in levels {
            assert!(quietest < 0.2 && loudest > 0.8, "{quietest} {loudest}");
        }
        assert!(apart > 0.3, "{apart}");
    }

    #[test]
    fn feedback_turns_notches_into_peaks() {
        let level = |feedback: u8| {
            let mut phaser = phaser(Settings {
                depth: 0,
                feedback,
                ..Settings::DEFAULT
            });
            (0..80)
                .map(|at| 50.0 * 2_f32.powf(at as f32 / 10.0))
                .map(|hertz| response(&mut phaser, hertz))
                .fold(0.0_f32, f32::max)
        };
        assert!(
            level(0xFF) > level(0x00) * 1.2,
            "{} {}",
            level(0xFF),
            level(0)
        );
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let smoothing = coefficient(RATE);
        let mut phaser = phaser(Settings::DEFAULT);
        let tone = |frame: usize| (frame as f32 / RATE as f32 * 100.0 * TAU).sin() * 0.5;
        let mut last = [0.0; 2];
        for frame in 0..RATE as usize / 10 {
            last = phaser.process([tone(frame); 2], smoothing, RATE);
        }
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as u8;
            phaser.follow(
                Settings {
                    rate: step * 21,
                    depth: step * 0xFF,
                    centre: 0x20 + step * 0xC0,
                    feedback: step * 0xFF,
                    stages: step * 2,
                    ..Settings::DEFAULT
                },
                FRAMES_PER_TICK,
            );
            let at = RATE as usize / 10 + frame;
            let out = phaser.process([tone(at); 2], smoothing, RATE);
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
        for depth in [0x00, 0xFF] {
            for centre in [0x00, 0xFF] {
                for feedback in [0x00, 0xFF] {
                    for stages in [0, 2] {
                        for (rate, sweep) in [(44_100, 0), (192_000, 21)] {
                            let mut phaser = Phaser::default();
                            phaser.follow(
                                Settings {
                                    rate: sweep,
                                    depth,
                                    centre,
                                    feedback,
                                    stages,
                                    ..Settings::DEFAULT
                                },
                                f64::from(rate) / 2.0 / 960.0,
                            );
                            phaser.settle();
                            for frame in 0..rate as usize / 2 {
                                // Full-scale noise, silence and a full-scale square in turn.
                                let level = match frame / 2000 % 3 {
                                    0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                    1 => 0.0,
                                    _ => {
                                        if frame / 50 % 2 == 0 {
                                            1.0
                                        } else {
                                            -1.0
                                        }
                                    }
                                };
                                let out = phaser.process([level, -level], 1.0, rate);
                                // A square's edges ring through twelve all-passes, so some
                                // room over what goes in is fair.
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                                    "{depth:x} {centre:x} {feedback:x} {stages} {rate}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
