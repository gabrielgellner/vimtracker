//! A stereo flanger, as Vital has it: each side read from a short delay line whose length a
//! triangle sweeps, with some of what's read fed back in, either way up. Mixed half and half with
//! the dry sound, the delay makes a comb, its first notch at half a cycle of the delay, and
//! feedback sharpens its teeth: positive feedback puts peaks where the delay is a whole number of
//! cycles, and negative halfway between. The two sides sweep a third of a turn apart, for width.
//!
//! Adapted from Vital's `modules/flanger_module.cpp`, GPL 3 or later, on a line from the pool like
//! the chorus's, read between samples:
//!
//! Copyright 2013-2019 Matt Tytel.
//!
//! Vital is free software: you can redistribute it and/or modify it under the terms of the GNU
//! General Public License as published by the Free Software Foundation, either version 3 of the
//! License, or (at your option) any later version.

use vt_model::insert::{
    Flanger as Settings, LONGEST_FLANGE_MS, SHORTEST_FLANGE_MS, flanger_delay_ms, flanger_feedback,
};
use vt_model::lfo::SWEEP_TICKS;

use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::filters::filter::soft_clip;

/// The most feedback goes to either way, so the comb rings but never runs away.
pub const MOST_FEEDBACK: f32 = 0.95;
/// How far what goes into the line can get before a soft clip holds it back, so only what's very
/// loud is touched.
const HEADROOM: f32 = 4.0;
/// How far round the right side's sweep is from the left's, as Vital starts.
const SIDE_APART: f32 = 1.0 / 3.0;

/// A triangle from -1 to 1, at a place in its turn.
#[inline]
fn triangle(turns: f32) -> f32 {
    let turns = turns - turns.floor();
    4.0 * (turns - 0.5).abs() - 1.0
}

/// A flanger's delay lines and sweep.
#[derive(Debug, Clone)]
pub struct Flanger {
    lines: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    /// Where the sweep is, in turns, and how far it goes each frame.
    phase: f32,
    step: f32,
    /// How far the delay sweeps, from 0 to 1.
    depth: Smoothed,
    /// The delay at the middle of the sweep, in frames.
    delay: Smoothed,
    feedback: Smoothed,
    /// The shortest the delay gets, in frames.
    shortest: f32,
    rate: u32,
    smoothing: f32,
}

impl Flanger {
    /// Lines long enough for its longest sweep at this rate.
    pub fn new(rate: u32) -> Self {
        let frames_per_ms = rate as f32 / 1000.0;
        // At full depth it sweeps to twice the longest middle, less the shortest.
        let longest = (2.0 * LONGEST_FLANGE_MS - SHORTEST_FLANGE_MS) * frames_per_ms;
        let size = (longest.ceil() as usize + 4).next_power_of_two();
        let mut flanger = Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            phase: 0.0,
            step: 0.0,
            depth: Smoothed::default(),
            delay: Smoothed::default(),
            feedback: Smoothed::default(),
            shortest: SHORTEST_FLANGE_MS * frames_per_ms,
            rate,
            smoothing: coefficient(rate),
        };
        flanger.follow(Settings::DEFAULT, 1.0);
        flanger.settle();
        flanger
    }

    /// The longest anything stays in it, in frames.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.phase = 0.0;
    }

    /// Take what's set now, at a tempo.
    pub fn follow(&mut self, settings: Settings, frames_per_tick: f64) {
        let ticks = SWEEP_TICKS[usize::from(settings.rate).min(SWEEP_TICKS.len() - 1)];
        let frames = f64::from(ticks) * frames_per_tick;
        self.step = if frames >= 1.0 {
            (1.0 / frames) as f32
        } else {
            0.0
        };
        self.depth.target = f32::from(settings.depth) / 255.0;
        self.delay.target = flanger_delay_ms(f32::from(settings.delay)) * self.rate as f32 / 1000.0;
        self.feedback.target = flanger_feedback(f32::from(settings.feedback)) * MOST_FEEDBACK;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.depth.settle();
        self.delay.settle();
        self.feedback.settle();
    }

    /// A line's sound a number of frames back, read between samples through four of them.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let frames = frames.clamp(2.0, (self.mask - 2) as f32);
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

    /// Feed one frame in and get the delayed sound out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let depth = self.depth.next(self.smoothing);
        let delay = self.delay.next(self.smoothing);
        let feedback = self.feedback.next(self.smoothing);
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let mut out = [0.0; 2];
        for (side, read) in out.iter_mut().enumerate() {
            let turns = self.phase + (side as f32 - 0.5) * SIDE_APART;
            // Vital's sweep: from the shortest at the bottom to as far past the middle at the top.
            let swept = (delay - self.shortest) * (1.0 + depth * triangle(turns)) + self.shortest;
            // What's just come in isn't in the line yet, so the newest there is a frame back.
            *read = self.read(side, swept - 1.0);
        }
        self.write = (self.write + 1) & self.mask;
        for (side, sample) in input.into_iter().enumerate() {
            let fed = sample + out[side] * feedback;
            self.lines[side][self.write] = soft_clip(fed / HEADROOM) * HEADROOM;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: u32 = 48_000;
    /// 120 BPM, as a tick is a 960th of a beat.
    const FRAMES_PER_TICK: f64 = 48_000.0 / 2.0 / 960.0;

    fn flanger(settings: Settings) -> Flanger {
        let mut flanger = Flanger::new(RATE);
        flanger.follow(settings, FRAMES_PER_TICK);
        flanger.settle();
        flanger
    }

    /// Still, at a delay, with some feedback.
    fn still(delay: u8, feedback: u8) -> Flanger {
        flanger(Settings {
            depth: 0,
            delay,
            feedback,
            ..Settings::DEFAULT
        })
    }

    /// How loud a sine comes out with the dry sound mixed in half and half, as the slot starts.
    fn response(flanger: &mut Flanger, hertz: f32) -> f32 {
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize / 5 {
            // Quiet, so even the loudest peak stays clear of the soft clip.
            let sample = (frame as f32 / RATE as f32 * hertz * TAU).sin() * 0.05;
            let wet = flanger.process([sample; 2])[0];
            if frame > RATE as usize / 10 {
                most = most.max(((sample + wet) * 0.5).abs());
            }
        }
        most / 0.05
    }

    #[test]
    fn its_comb_follows_its_delay() {
        for delay in [0x20, 0x80, 0xE0] {
            let seconds = flanger_delay_ms(f32::from(delay)) / 1000.0;
            let notch = response(&mut still(delay, 0x80), 0.5 / seconds);
            let tooth = response(&mut still(delay, 0x80), 1.0 / seconds);
            assert!(notch < 0.02, "{delay:x}: {notch}");
            assert!(tooth > 0.98, "{delay:x}: {tooth}");
        }
    }

    #[test]
    fn feedback_either_way_puts_peaks_in_different_places() {
        let seconds = flanger_delay_ms(f32::from(0x80_u8)) / 1000.0;
        let whole = 1.0 / seconds;
        let half = 0.5 / seconds;
        let up = |hertz| response(&mut still(0x80, 0xFF), hertz);
        let down = |hertz| response(&mut still(0x80, 0x00), hertz);
        assert!(up(whole) > 5.0, "{}", up(whole));
        assert!(down(half) > 5.0, "{}", down(half));
        // Where one peaks the other dips.
        assert!(
            up(whole) > up(half) * 5.0 && down(half) > down(whole) * 5.0,
            "{} {} {} {}",
            up(whole),
            up(half),
            down(half),
            down(whole)
        );
    }

    #[test]
    fn depth_sweeps_the_comb_and_each_side_its_own_way() {
        let mut flanger = flanger(Settings {
            // A half note, so a sweep takes a second.
            rate: 15,
            depth: 0xFF,
            feedback: 0x80,
            ..Settings::DEFAULT
        });
        let mut apart = 0.0_f32;
        let mut quietest = f32::MAX;
        let mut loudest = 0.0_f32;
        let mut peaks = [0.0_f32; 2];
        let window = RATE as usize / 200;
        for frame in 0..RATE as usize * 2 {
            let sample = (frame as f32 / RATE as f32 * 1_000.0 * TAU).sin();
            let wet = flanger.process([sample; 2]);
            for side in 0..2 {
                peaks[side] = peaks[side].max(((sample + wet[side]) * 0.5).abs());
            }
            if frame % window == window - 1 {
                quietest = quietest.min(peaks[0]);
                loudest = loudest.max(peaks[0]);
                apart = apart.max((peaks[0] - peaks[1]).abs());
                peaks = [0.0; 2];
            }
        }
        assert!(quietest < 0.2 && loudest > 0.9, "{quietest} {loudest}");
        assert!(apart > 0.3, "{apart}");
    }

    #[test]
    fn at_full_feedback_either_way_it_rings_down_and_never_runs_away() {
        for feedback in [0x00, 0xFF] {
            for delay in [0x00, 0xFF] {
                let mut flanger = still(delay, feedback);
                flanger.process([1.0, 1.0]);
                let mut loudest = 0.0_f32;
                let mut last = 0.0_f32;
                for frame in 0..RATE as usize * 10 {
                    let out = flanger.process([0.0; 2]);
                    loudest = loudest.max(out[0].abs());
                    if frame >= RATE as usize * 9 {
                        last = last.max(out[0].abs());
                    }
                }
                assert!(loudest <= 1.0, "{feedback:x} {delay:x}: {loudest}");
                assert!(last < 1e-4, "{feedback:x} {delay:x}: {last}");
            }
        }
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let mut flanger = flanger(Settings::DEFAULT);
        let tone = |frame: usize| (frame as f32 / RATE as f32 * 100.0 * TAU).sin() * 0.5;
        let mut last = [0.0; 2];
        for frame in 0..RATE as usize / 10 {
            last = flanger.process([tone(frame); 2]);
        }
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as u8;
            flanger.follow(
                Settings {
                    rate: step * 21,
                    depth: step * 0xFF,
                    delay: 0x20 + step * 0xC0,
                    feedback: 0x40 + step * 0x80,
                    ..Settings::DEFAULT
                },
                FRAMES_PER_TICK,
            );
            let at = RATE as usize / 10 + frame;
            let out = flanger.process([tone(at); 2]);
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
            for delay in [0x00, 0xFF] {
                for feedback in [0x00, 0xFF] {
                    for (rate, sweep) in [(44_100, 0), (192_000, 21)] {
                        let mut flanger = Flanger::new(rate);
                        flanger.follow(
                            Settings {
                                rate: sweep,
                                depth,
                                delay,
                                feedback,
                                ..Settings::DEFAULT
                            },
                            f64::from(rate) / 2.0 / 960.0,
                        );
                        flanger.settle();
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
                            let out = flanger.process([level, -level]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() <= HEADROOM),
                                "{depth:x} {delay:x} {feedback:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_reads_as_far_back_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let flanger = Flanger::new(rate);
            let longest = (2.0 * LONGEST_FLANGE_MS - SHORTEST_FLANGE_MS) * rate as f32 / 1000.0;
            assert!(flanger.frames() as f32 >= longest + 4.0, "{rate}");
        }
    }
}
