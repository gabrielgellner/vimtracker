//! A stereo pitch shifter, as in Mutable Instruments' Clouds: each side goes down a short line, and
//! two grains read it back faster or slower than it's written. Each grain's delay sweeps across a
//! window and jumps back, and the two are half a window apart, so one fades out under a triangle
//! as it nears its jump while the other fades in. What comes out keeps its time and moves in pitch.
//!
//! Clouds' window is 2047 frames at 32 kHz; here it's set in ms and worked out at the rate it runs
//! at, and its line comes from the effect pool, long enough for the longest window at the pool's
//! rate. The grains are read between samples with a cubic rather than Clouds' straight line. The
//! shift glides, and the window moves no faster than its whole range in a second, so neither
//! clicks. Some of what comes out can go back into the line, so each trip round is shifted again.
//!
//! Adapted from `clouds/dsp/fx/pitch_shifter.h`, MIT:
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

use vt_model::insert::{
    LONGEST_GRAIN_MS, Pitch as Settings, SHORTEST_GRAIN_MS, fine_cents, grain_ms, shift_semitones,
};

use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::filters::filter::soft_clip;

/// The most of what comes out that goes back in, well short of all, so stacked shifts die away.
pub const MOST_FEEDBACK: f32 = 0.85;
/// How far what goes into the line can get before a soft clip holds it back, so only what's very
/// loud is touched.
const HEADROOM: f32 = 4.0;

/// A pitch shifter's lines and grains.
#[derive(Debug, Clone)]
pub struct PitchShifter {
    lines: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    /// Where the first grain is across its window, from 0 to 1. The second is half a turn on.
    phase: f32,
    /// How far it shifts, in semitones.
    semitones: Smoothed,
    /// How long the window is wanted, in frames, and how long it is now, which follows no faster
    /// than it may move.
    window: Smoothed,
    window_now: f32,
    feedback: Smoothed,
    /// What came out last, to feed back.
    last: [f32; 2],
    /// How far the window may move in a frame, in frames.
    most_move: f32,
    rate: u32,
    smoothing: f32,
}

impl PitchShifter {
    /// Lines long enough for its longest window at this rate.
    pub fn new(rate: u32) -> Self {
        let frames_per_ms = rate as f32 / 1000.0;
        let longest = LONGEST_GRAIN_MS * frames_per_ms;
        let size = (longest.ceil() as usize + 4).next_power_of_two();
        let mut shifter = Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            phase: 0.0,
            semitones: Smoothed::default(),
            window: Smoothed::default(),
            window_now: 0.0,
            feedback: Smoothed::default(),
            last: [0.0; 2],
            // Across the whole range in a second.
            most_move: (LONGEST_GRAIN_MS - SHORTEST_GRAIN_MS) / 1000.0,
            rate,
            smoothing: coefficient(rate),
        };
        shifter.follow(Settings::DEFAULT);
        shifter.settle();
        shifter
    }

    /// The longest anything stays in it, in frames.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    pub fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
        self.last = [0.0; 2];
        self.phase = 0.0;
    }

    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.semitones.target = shift_semitones(f32::from(settings.shift))
            + fine_cents(f32::from(settings.fine)) / 100.0;
        self.window.target = grain_ms(f32::from(settings.size)) * self.rate as f32 / 1000.0;
        self.feedback.target = f32::from(settings.feedback) / 255.0 * MOST_FEEDBACK;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.semitones.settle();
        self.window.settle();
        self.window_now = self.window.value;
        self.feedback.settle();
    }

    /// A line's sound a number of frames back, read between samples through four of them. The
    /// newest, just written, is none back.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let frames = frames.clamp(1.0, (self.mask - 2) as f32);
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

    /// Feed one frame in and get the shifted sound out, all wet.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let ratio = (self.semitones.next(self.smoothing) / 12.0).exp2();
        let wanted = self.window.next(self.smoothing);
        self.window_now += (wanted - self.window_now).clamp(-self.most_move, self.most_move);
        let window = self.window_now;
        let feedback = self.feedback.next(self.smoothing);

        // Higher reads faster than it's written, so the delay shrinks, and lower lets it grow.
        self.phase += (1.0 - ratio) / window;
        self.phase -= self.phase.floor();
        let fade = 2.0
            * if self.phase >= 0.5 {
                1.0 - self.phase
            } else {
                self.phase
            };
        let first = self.phase * window;
        let mut second = first + window * 0.5;
        if second >= window {
            second -= window;
        }

        self.write = (self.write + 1) & self.mask;
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            let fed = sample + self.last[side] * feedback;
            self.lines[side][self.write] = soft_clip(fed / HEADROOM) * HEADROOM;
            out[side] = self.read(side, first) * fade + self.read(side, second) * (1.0 - fade);
        }
        self.last = out;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use vt_model::insert::MOST_SHIFT;

    const RATE: u32 = 48_000;

    fn shifter(settings: Settings, rate: u32) -> PitchShifter {
        let mut shifter = PitchShifter::new(rate);
        shifter.follow(settings);
        shifter.settle();
        shifter
    }

    fn shifted(semitones: i8) -> Settings {
        Settings {
            shift: (MOST_SHIFT as i8 + semitones) as u8,
            ..Settings::DEFAULT
        }
    }

    fn sine(frame: usize, hertz: f32, rate: u32) -> f32 {
        (frame as f32 / rate as f32 * hertz * TAU).sin() * 0.5
    }

    /// A sine through it for two seconds, with the last second of what comes out.
    fn through(settings: Settings, hertz: f32) -> Vec<f32> {
        let mut shifter = shifter(settings, RATE);
        (0..RATE as usize * 2)
            .map(|frame| shifter.process([sine(frame, hertz, RATE); 2])[0])
            .skip(RATE as usize)
            .collect()
    }

    /// How strong a frequency is in a sound, as a sine's amplitude, by a single bin of a DFT over
    /// each 40 ms through a Hann window, on average. That's not much longer than a grain, so the
    /// grains' fades don't blur it much.
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

    /// Its frequency, from how often it goes from below a little under zero to above a little
    /// over, so a fade that wobbles round zero isn't counted.
    fn crossing_hertz(sound: &[f32]) -> f32 {
        let mut below = false;
        let mut ups = 0;
        for sample in sound {
            if *sample < -0.05 {
                below = true;
            } else if *sample > 0.05 && below {
                below = false;
                ups += 1;
            }
        }
        ups as f32 * RATE as f32 / sound.len() as f32
    }

    /// The frequency nearest this whose cycles fit a whole number of times in the window, and in
    /// half of it. A grain then jumps back by whole cycles, and the two are in step, so what comes
    /// out is a clean sine, and its frequency can be counted exactly. Any other frequency wobbles
    /// in phase as each grain jumps, by up to a cycle a window.
    fn in_tune(hertz: f32) -> f32 {
        let window = grain_ms(f32::from(Settings::DEFAULT.size)) / 1000.0;
        let cycles = (hertz * window * 0.5).round() * 2.0;
        cycles / window
    }

    #[test]
    fn up_twelve_is_an_octave_up() {
        let played = in_tune(440.0);
        let out = through(shifted(12), played);
        let hertz = crossing_hertz(&out);
        assert!((hertz / (played * 2.0) - 1.0).abs() < 0.002, "{hertz}");
        let (octave, left) = (strength(&out, played * 2.0), strength(&out, played));
        assert!(octave > 0.45, "{octave}");
        assert!(left < octave * 0.01, "{octave} {left}");
        // A tone that doesn't fit the window still comes out an octave up, its phase wobbling.
        let out = through(shifted(12), 440.0);
        let hertz = crossing_hertz(&out);
        assert!((hertz / 880.0 - 1.0).abs() < 0.025, "{hertz}");
        let (octave, left) = (strength(&out, 880.0), strength(&out, 440.0));
        assert!(octave > 0.3, "{octave}");
        assert!(left < octave * 0.05, "{octave} {left}");
    }

    #[test]
    fn down_twelve_is_an_octave_down_and_fine_moves_it_in_cents() {
        let played = in_tune(440.0);
        let out = through(shifted(-12), played);
        let hertz = crossing_hertz(&out);
        assert!((hertz / (played * 0.5) - 1.0).abs() < 0.002, "{hertz}");
        assert!(strength(&out, played * 0.5) > strength(&out, played) * 50.0);
        // Up seven and fifty cents more is a fifth and a quarter tone.
        let out = through(
            Settings {
                fine: 0xFF,
                ..shifted(7)
            },
            played,
        );
        let expected = played * (7.5_f32 / 12.0).exp2();
        let hertz = crossing_hertz(&out);
        assert!((hertz / expected - 1.0).abs() < 0.002, "{hertz} {expected}");
    }

    #[test]
    fn with_no_shift_it_is_the_input_half_a_window_late() {
        let mut shifter = shifter(Settings::DEFAULT, RATE);
        let late = grain_ms(f32::from(Settings::DEFAULT.size)) * RATE as f32 / 1000.0 * 0.5;
        for frame in 0..RATE as usize {
            let out = shifter.process([sine(frame, 220.0, RATE), -sine(frame, 330.0, RATE)]);
            if frame > late as usize + 4 {
                let at = frame as f32 - late;
                let left = (at / RATE as f32 * 220.0 * TAU).sin() * 0.5;
                let right = -(at / RATE as f32 * 330.0 * TAU).sin() * 0.5;
                assert!((out[0] - left).abs() < 5e-3, "{frame}: {out:?} {left}");
                assert!((out[1] - right).abs() < 5e-3, "{frame}: {out:?} {right}");
            }
        }
    }

    /// The biggest jump between frames from a tone through it, after a second, with the settings
    /// switching between two every 64 frames, as a knob or an LFO steps them block by block, or
    /// held at the first.
    fn biggest_jump(from: Settings, to: Settings, moving: bool) -> f32 {
        let mut shifter = shifter(from, RATE);
        let mut last = [0.0_f32; 2];
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize * 3 {
            let late = frame > RATE as usize;
            if late {
                let now = if moving && frame / 64 % 2 == 1 {
                    to
                } else {
                    from
                };
                shifter.follow(now);
            }
            let out = shifter.process([sine(frame, 220.0, RATE); 2]);
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
            ("Pitch", shifted(-24), shifted(24)),
            ("Pitch near", shifted(0), shifted(7)),
            (
                "Fine",
                Settings { fine: 0, ..plain },
                Settings {
                    fine: 0xFF,
                    ..plain
                },
            ),
            (
                "Size",
                Settings { size: 0, ..plain },
                Settings {
                    size: 0xFF,
                    ..plain
                },
            ),
            (
                "Feedback",
                shifted(12),
                Settings {
                    feedback: 0xFF,
                    ..shifted(12)
                },
            ),
            (
                "all",
                Settings {
                    size: 0,
                    fine: 0,
                    ..shifted(-24)
                },
                Settings {
                    size: 0xFF,
                    fine: 0xFF,
                    feedback: 0xFF,
                    ..shifted(24)
                },
            ),
        ];
        for (name, from, to) in pairs {
            // It jumps no further moving than it does held at either end.
            let still = biggest_jump(from, from, false).max(biggest_jump(to, to, false));
            let moving = biggest_jump(from, to, true);
            assert!(
                moving < still * 1.25,
                "{name}: {moving} moving, {still} still"
            );
        }
    }

    /// How much comes out in each quarter of a second, for four seconds, from a tenth of a second
    /// of a tone and then nothing.
    fn ringing(settings: Settings) -> Vec<f32> {
        let mut shifter = shifter(settings, RATE);
        let out: Vec<f32> = (0..RATE as usize * 4)
            .map(|frame| {
                let tone = if frame < RATE as usize / 10 {
                    sine(frame, 330.0, RATE)
                } else {
                    0.0
                };
                shifter.process([tone; 2])[0]
            })
            .collect();
        assert!(out.iter().all(|s| s.is_finite() && s.abs() < HEADROOM));
        out.chunks(RATE as usize / 4)
            .map(|chunk| chunk.iter().map(|s| s * s).sum())
            .collect()
    }

    #[test]
    fn feedback_rings_on_and_dies_away() {
        for semitones in [0, 12, -12, 7, -24] {
            let plain = ringing(shifted(semitones));
            let fed = ringing(Settings {
                feedback: 0xFF,
                ..shifted(semitones)
            });
            // Without feedback it's done a window after the tone, and with it, it rings on.
            assert_eq!(plain[1], 0.0, "{semitones}: {plain:?}");
            assert!(fed[1] > fed[0] * 1e-3, "{semitones}: {fed:?}");
            // And it keeps getting quieter, until it's gone.
            for pair in fed[1..].windows(2) {
                assert!(pair[1] <= pair[0], "{semitones}: {fed:?}");
            }
            assert!(fed[15] < fed[1] * 1e-6, "{semitones}: {fed:?}");
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for shift in [0, MOST_SHIFT, MOST_SHIFT * 2] {
            for fine in [0x00, 0xFF] {
                for size in [0x00, 0xFF] {
                    for feedback in [0x00, 0xFF] {
                        for rate in [44_100, 192_000] {
                            let mut shifter = shifter(
                                Settings {
                                    shift,
                                    fine,
                                    size,
                                    feedback,
                                    ..Settings::DEFAULT
                                },
                                rate,
                            );
                            for frame in 0..rate as usize * 2 {
                                // Full-scale noise, a full-scale square, DC and silence in turn.
                                let level = match frame / (rate as usize / 4) % 4 {
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
                                let out = shifter.process([level, -level * 0.5]);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() < 5.0),
                                    "{shift} {fine:x} {size:x} {feedback:x} {rate}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn its_lines_hold_the_longest_window_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let shifter = PitchShifter::new(rate);
            let needed = LONGEST_GRAIN_MS * rate as f32 / 1000.0 + 3.0;
            assert!(shifter.frames() as f32 >= needed, "{rate}");
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut shifter = shifter(
            Settings {
                feedback: 0xFF,
                ..shifted(12)
            },
            RATE,
        );
        for frame in 0..1000 {
            shifter.process([sine(frame, 440.0, RATE); 2]);
        }
        shifter.clear();
        for _ in 0..RATE {
            assert_eq!(shifter.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
