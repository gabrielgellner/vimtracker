//! A frequency shifter: every frequency in the sound moved up or down by the same number of Hz,
//! which a pitch shifter doesn't do, so a harmonic sound comes out inharmonic, like a bell, and a
//! shift of a few Hz beats and swirls. A Hilbert transform makes from each side a pair a quarter
//! of a turn apart at every frequency, and turning that pair round at the shift, as a complex
//! number is multiplied, moves the whole spectrum one way. Feedback through a delay sends what's
//! shifted round to be shifted again, so each repeat climbs or falls further, the barber-pole
//! spiral.
//!
//! The Hilbert transform is adapted from sst-basic-blocks' `dsp/HilbertTransform.h`, which is
//! offered under the MIT licence or the GPL 3, as its README describes: two chains of three
//! second-order all-passes, with coefficients from Bernie Hutchins' Electronotes, shared by Sean
//! Costello. Its notice:
//!
//! sst-basic-blocks - an open source library of core audio utilities built by Surge Synth Team.
//! Copyright 2023, various authors, as described in the GitHub transaction log. A very small
//! number of explicitly chosen header files can also be used in an MIT/BSD context. This file is
//! available for use under the MIT license or the GPL3 license, as described in README.
//!
//! The feedback is after sst-effects' `voice-effects/modulation/FreqShiftMod.h`, GPL 3 or later
//! (Copyright 2018-2023, various authors, as described in the GitHub transaction log): what goes
//! round loses a little over 5 kHz each time, is held by a soft limit, `√2 x / (1 + x²)`, and
//! what comes out is high-passed at 15 Hz, so a stack of shifts stays bounded and drops what it
//! pushes under the bass. The delay in the loop is after Surge's `FrequencyShifterEffect.cpp`,
//! GPL 3 or later, written afresh: its line is written with what comes in and what went round, and
//! read back before it's shifted. Tracking the note is after `FreqShiftMod.h`'s keytrack.

use std::f32::consts::{FRAC_1_SQRT_2, PI, SQRT_2, TAU};

use vt_model::insert::{
    FreqShift as Settings, LONGEST_SHIFT_DELAY_MS, shift_delay_ms, shift_hertz, spread_hertz,
};

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::resonator::semitones;

/// The poles of the two chains of first-order all-passes, as multiples of the lowest frequency the
/// pair keeps a quarter turn apart: the first six for the real chain, the rest for the imaginary.
const POLES: [f32; 12] = [
    0.3609, 2.7412, 11.1573, 44.7581, 179.6242, 798.4578, 1.2524, 5.5671, 22.3423, 89.6271,
    364.7914, 2770.1114,
];
/// The lowest frequency the pair keeps a quarter turn apart, in Hz, as sst sets it.
const LOWEST_HZ: f32 = 25.0;
/// The note the shift is set at when it follows the note: middle C.
const MIDDLE_C: f32 = 60.0;
/// The most of what comes out that goes round again, so a click dies away even at `FF`.
const MOST_FEEDBACK: f32 = 0.95;
/// Where what goes round starts to lose its top, and how much it keeps above there each time, as
/// `FreqShiftMod.h`'s shelf.
const SHELF_HZ: f32 = 5000.0;
const SHELF_GAIN: f32 = 0.85;
/// Where what comes out is high-passed, so nothing shifted under the bass builds up.
const DC_HZ: f32 = 15.0;
/// How far what's read back can get before it's held, 12 dB over full scale. Nothing finite gets
/// near it, so it's there for a NaN, which comes out at an end and dies away rather than staying
/// in the all-passes for good (#461).
const HEADROOM: f32 = 4.0;

/// A second-order all-pass in transposed direct form, as sst writes it: its numerator is its
/// denominator backwards.
#[derive(Debug, Clone, Copy, Default)]
struct AllPass {
    a1: f32,
    a2: f32,
    held: [f32; 2],
}

impl AllPass {
    /// The two first-order all-passes with these coefficients, one after the other.
    fn of(first: f32, second: f32) -> Self {
        Self {
            a1: -(first + second),
            a2: first * second,
            held: [0.0; 2],
        }
    }

    #[inline]
    fn step(&mut self, input: f32) -> f32 {
        let out = input * self.a2 + self.held[0];
        self.held[0] = input * self.a1 - self.a1 * out + self.held[1];
        self.held[1] = input - self.a2 * out;
        out
    }
}

/// One side's Hilbert transform: two chains whose outputs are a quarter of a turn apart from 25 Hz
/// up, the real and imaginary parts of the side as a sound with only positive frequencies.
#[derive(Debug, Clone, Copy, Default)]
struct Hilbert {
    real: [AllPass; 3],
    imaginary: [AllPass; 3],
}

impl Hilbert {
    fn new(rate: u32) -> Self {
        // Each first-order all-pass through the bilinear transform, as sst has it, unwarped.
        let coefficients = POLES.map(|pole| {
            let k = LOWEST_HZ * PI * pole / rate as f32;
            (1.0 - k) / (1.0 + k)
        });
        // Paired highest with lowest in each chain, as sst pairs them.
        let chain = |first: usize| {
            [0, 1, 2].map(|at| AllPass::of(coefficients[first + at], coefficients[first + 5 - at]))
        };
        Self {
            real: chain(0),
            imaginary: chain(6),
        }
    }

    #[inline]
    fn step(&mut self, input: f32) -> (f32, f32) {
        let (mut real, mut imaginary) = (input, input);
        for (re, im) in self.real.iter_mut().zip(&mut self.imaginary) {
            real = re.step(real);
            imaginary = im.step(imaginary);
        }
        (real, imaginary)
    }

    fn clear(&mut self) {
        for pass in self.real.iter_mut().chain(&mut self.imaginary) {
            pass.held = [0.0; 2];
        }
    }
}

/// The soft limit on what goes round, `FreqShiftMod.h`'s `√2 x / (1 + x²)` with its input scaled
/// so it passes a quiet sound as it is: it rises to its most, 0.71, at 1.41, and is held there
/// above, so it never turns back down. A NaN is held to the bottom.
#[inline]
fn limited(x: f32) -> f32 {
    let x = within(x * FRAC_1_SQRT_2, -1.0, 1.0);
    SQRT_2 * x / (1.0 + x * x)
}

/// A frequency shifter's lines, transforms and oscillators.
#[derive(Debug, Clone)]
pub struct FreqShifter {
    lines: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    hilbert: [Hilbert; 2],
    /// Each side's oscillator, as a point going round the unit circle: its cosine and sine.
    turning: [[f32; 2]; 2],
    /// How far each side's oscillator turns a frame, as a point on the circle, and the shift in Hz
    /// it was worked out for.
    step: [[f32; 2]; 2],
    stepped_hertz: [f32; 2],
    /// What went round last frame, to go into the line with what comes in.
    fed_back: [f32; 2],
    /// The shelf's low-pass, and the high-pass's last in and out, on each side.
    shelf: [f32; 2],
    dc: [[f32; 2]; 2],
    shift: Smoothed,
    spread: Smoothed,
    track: Smoothed,
    /// The delay in the loop, in frames.
    delay: Smoothed,
    feedback: Smoothed,
    shelf_coefficient: f32,
    dc_coefficient: f32,
    rate: u32,
    smoothing: f32,
}

impl FreqShifter {
    /// Lines long enough for its longest delay at this rate.
    pub fn new(rate: u32) -> Self {
        let longest = LONGEST_SHIFT_DELAY_MS / 1000.0 * rate as f32;
        let size = (longest.ceil() as usize + 2).next_power_of_two();
        let mut shifter = Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            hilbert: [Hilbert::new(rate); 2],
            turning: [[1.0, 0.0]; 2],
            step: [[1.0, 0.0]; 2],
            stepped_hertz: [0.0; 2],
            fed_back: [0.0; 2],
            shelf: [0.0; 2],
            dc: [[0.0; 2]; 2],
            shift: Smoothed::default(),
            spread: Smoothed::default(),
            track: Smoothed::default(),
            delay: Smoothed::default(),
            feedback: Smoothed::default(),
            shelf_coefficient: 1.0 - (-TAU * SHELF_HZ / rate as f32).exp(),
            dc_coefficient: 1.0 - TAU * DC_HZ / rate as f32,
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
        for hilbert in &mut self.hilbert {
            hilbert.clear();
        }
        self.turning = [[1.0, 0.0]; 2];
        self.fed_back = [0.0; 2];
        self.shelf = [0.0; 2];
        self.dc = [[0.0; 2]; 2];
    }

    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.shift.target = shift_hertz(f32::from(settings.shift));
        self.spread.target = spread_hertz(f32::from(settings.spread));
        self.track.target = f32::from(settings.track) / 255.0;
        // A whole number of frames, so once it's still each read is exact.
        let frames = (shift_delay_ms(f32::from(settings.time)) / 1000.0 * self.rate as f32).round();
        self.delay.target = frames.min((self.mask - 1) as f32);
        self.feedback.target = f32::from(settings.feedback) / 255.0 * MOST_FEEDBACK;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.shift.settle();
        self.spread.settle();
        self.track.settle();
        self.delay.settle();
        self.feedback.settle();
    }

    /// One frame in, at middle C, for the pool's own use.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input, MIDDLE_C, self.smoothing)
    }

    /// A line's sound a number of frames back, `0` being what was just written, read between
    /// samples.
    #[inline]
    fn read(&self, side: usize, frames: f32) -> f32 {
        let whole = frames as usize;
        let between = frames - whole as f32;
        let line = &self.lines[side];
        let newer = line[self.write.wrapping_sub(whole) & self.mask];
        let older = line[self.write.wrapping_sub(whole + 1) & self.mask];
        newer + (older - newer) * between
    }

    /// Turn a side's oscillator on a frame, at a shift in Hz. Its step is worked out again only
    /// when the shift moves, and it's kept on the circle by pulling it back each frame.
    #[inline]
    fn turn(&mut self, side: usize, hertz: f32) -> [f32; 2] {
        if hertz != self.stepped_hertz[side] {
            let angle = TAU * hertz / self.rate as f32;
            self.step[side] = [angle.cos(), angle.sin()];
            self.stepped_hertz[side] = hertz;
        }
        let [cos, sin] = self.turning[side];
        let [by_cos, by_sin] = self.step[side];
        let (cos, sin) = (cos * by_cos - sin * by_sin, cos * by_sin + sin * by_cos);
        let pull = 1.5 - 0.5 * (cos * cos + sin * sin);
        self.turning[side] = [cos * pull, sin * pull];
        self.turning[side]
    }

    /// Feed one frame in and get it shifted out, all wet, with the shift following `note`, a MIDI
    /// note, as far as `Track` says.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], note: f32, smoothing: f32) -> [f32; 2] {
        let shift = self.shift.next(smoothing);
        let spread = self.spread.next(smoothing);
        let track = self.track.next(smoothing);
        let delay = self.delay.next(smoothing);
        let feedback = self.feedback.next(smoothing);
        let followed = if track > 0.0 {
            semitones((note - MIDDLE_C) * track)
        } else {
            1.0
        };
        // The left goes further up and the right further down, by half the spread each.
        let hertz = [
            (shift + spread * 0.5) * followed,
            (shift - spread * 0.5) * followed,
        ];
        self.write = (self.write + 1) & self.mask;
        let mut out = [0.0; 2];
        for side in 0..2 {
            self.lines[side][self.write] = input[side] + self.fed_back[side];
            let read = within(self.read(side, delay), -HEADROOM, HEADROOM);
            let (real, imaginary) = self.hilbert[side].step(read);
            let [cos, sin] = self.turn(side, hertz[side]);
            // The imaginary chain comes out a quarter of a turn ahead, so the side as a sound of
            // positive frequencies alone is `real - i imaginary`, and this is the real part of it
            // turned round by the oscillator.
            let shifted = real * cos + imaginary * sin;
            let [last_in, last_out] = self.dc[side];
            let heard = shifted - last_in + self.dc_coefficient * last_out;
            self.dc[side] = [shifted, heard];
            // What goes round keeps its lows and loses a little of its top.
            self.shelf[side] += (heard - self.shelf[side]) * self.shelf_coefficient;
            let shelved = self.shelf[side] + (heard - self.shelf[side]) * SHELF_GAIN;
            self.fed_back[side] = limited(shelved * feedback);
            out[side] = heard;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn shifter(settings: Settings) -> FreqShifter {
        let mut shifter = FreqShifter::new(RATE);
        shifter.follow(settings);
        shifter.settle();
        shifter
    }

    /// The setting that shifts by about so many Hz.
    fn shift_for(hertz: f32) -> u8 {
        (0..=255_u8)
            .min_by(|a, b| {
                let off = |value: &u8| (shift_hertz(f32::from(*value)) - hertz).abs();
                off(a).total_cmp(&off(b))
            })
            .unwrap()
    }

    /// How strong a frequency is in some of a side, by Goertzel's algorithm.
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

    /// A sine through it, with the left side of what comes out once it's settled.
    fn shifted(shifter: &mut FreqShifter, hertz: f32, note: f32) -> Vec<f32> {
        (0..RATE as usize)
            .map(|frame| {
                let sample = (frame as f32 / RATE as f32 * hertz * TAU).sin() * 0.5;
                shifter.process([sample; 2], note, 1.0)[0]
            })
            .skip(RATE as usize / 2)
            .collect()
    }

    #[test]
    fn a_sine_comes_out_shifted_by_the_hertz_set_either_way() {
        for (input, by) in [
            (1_000.0, 100.0),
            (1_000.0, -100.0),
            (220.0, 30.0),
            (5_000.0, -700.0),
        ] {
            let value = shift_for(by);
            let by = shift_hertz(f32::from(value));
            let out = shifted(
                &mut shifter(Settings {
                    shift: value,
                    ..Settings::DEFAULT
                }),
                input,
                MIDDLE_C,
            );
            let (wanted, image, left) = (
                strength(&out, input + by),
                strength(&out, input - by),
                strength(&out, input),
            );
            assert!(
                (wanted - 0.5).abs() < 0.03,
                "{input} {by}: {wanted} {image} {left}"
            );
            // The other side band and the sound itself are at least 30 dB under.
            assert!(image < wanted * 0.03, "{input} {by}: {image} {wanted}");
            assert!(left < wanted * 0.03, "{input} {by}: {left} {wanted}");
        }
    }

    #[test]
    fn spread_takes_the_sides_apart_and_track_follows_the_note() {
        let spread = (0..=255_u8)
            .find(|value| spread_hertz(f32::from(*value)) >= 20.0)
            .unwrap();
        let apart = spread_hertz(f32::from(spread));
        let mut both = shifter(Settings {
            shift: 0x80,
            spread,
            ..Settings::DEFAULT
        });
        let sides: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|frame| {
                let sample = (frame as f32 / RATE as f32 * 1_000.0 * TAU).sin() * 0.5;
                both.process([sample; 2], MIDDLE_C, 1.0)
            })
            .skip(RATE as usize / 2)
            .collect();
        let left: Vec<f32> = sides.iter().map(|frame| frame[0]).collect();
        let right: Vec<f32> = sides.iter().map(|frame| frame[1]).collect();
        assert!(strength(&left, 1_000.0 + apart / 2.0) > 0.45);
        assert!(strength(&right, 1_000.0 - apart / 2.0) > 0.45);

        // At full `Track`, an octave up shifts twice as far.
        let value = shift_for(50.0);
        let by = shift_hertz(f32::from(value));
        let followed = Settings {
            shift: value,
            track: 0xFF,
            ..Settings::DEFAULT
        };
        let out = shifted(&mut shifter(followed), 1_000.0, MIDDLE_C + 12.0);
        assert!(strength(&out, 1_000.0 + by * 2.0) > 0.45);
        let out = shifted(&mut shifter(followed), 1_000.0, MIDDLE_C);
        assert!(strength(&out, 1_000.0 + by) > 0.45);
    }

    #[test]
    fn its_delay_holds_back_what_comes_out() {
        let time = 0x80;
        let frames = (shift_delay_ms(f32::from(time)) / 1000.0 * RATE as f32).round() as usize;
        let mut shifter = shifter(Settings {
            time,
            ..Settings::DEFAULT
        });
        let out: Vec<f32> = (0..frames * 2)
            .map(|frame| shifter.process([f32::from(u8::from(frame == 0)); 2], 60.0, 1.0)[0])
            .collect();
        assert!(out[..frames].iter().all(|sample| *sample == 0.0));
        assert!(out[frames..].iter().any(|sample| sample.abs() > 0.1));
    }

    #[test]
    fn feedback_stays_bounded_at_every_extreme() {
        for shift in [0x00, 0x80, 0x90, 0xFF] {
            for time in [0x00, 0x20, 0xFF] {
                let mut shifter = shifter(Settings {
                    shift,
                    spread: 0xFF,
                    time,
                    feedback: 0xFF,
                    ..Settings::DEFAULT
                });
                // A square 12 dB over full scale, then nothing.
                let mut most = 0.0_f32;
                for frame in 0..RATE as usize * 4 {
                    let input = if frame < RATE as usize * 2 {
                        if (frame / 50) % 2 == 0 { 4.0 } else { -4.0 }
                    } else {
                        0.0
                    };
                    let out = shifter.process([input; 2], 60.0, 1.0);
                    assert!(out.iter().all(|s| s.is_finite()), "{shift:x} {time:x}");
                    most = most.max(out[0].abs()).max(out[1].abs());
                }
                assert!(most < 12.0, "{shift:x} {time:x}: {most}");
            }
        }
    }

    #[test]
    fn the_limit_on_what_goes_round_passes_quiet_sounds_and_holds_a_nan() {
        assert!((limited(0.01) - 0.01).abs() < 1e-5);
        assert!((limited(-0.01) + 0.01).abs() < 1e-5);
        assert!((limited(1e9) - FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((limited(f32::INFINITY) - FRAC_1_SQRT_2).abs() < 1e-6);
        assert!((limited(f32::NAN) + FRAC_1_SQRT_2).abs() < 1e-6);
        // It never turns back down as it gets louder.
        let mut last = limited(-10.0);
        for at in -1000..=1000 {
            let next = limited(at as f32 / 100.0);
            assert!(next >= last);
            last = next;
        }
    }

    #[test]
    fn a_nan_in_the_line_comes_out_once_and_what_goes_round_dies_away() {
        let mut shifter = shifter(Settings {
            time: 0x40,
            feedback: 0xFF,
            ..Settings::DEFAULT
        });
        shifter.process([0.5; 2], 60.0, 1.0);
        shifter.lines[0][shifter.write] = f32::NAN;
        let out: Vec<f32> = (0..RATE * 20)
            .flat_map(|_| shifter.process([0.0; 2], 60.0, 1.0))
            .collect();
        assert!(out.iter().all(|s| s.is_finite()));
        assert!(out[out.len() - 100..].iter().all(|s| s.abs() < 1e-4));
    }

    #[test]
    fn it_holds_its_longest_delay_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let shifter = FreqShifter::new(rate);
            assert!(shifter.frames() as f32 > LONGEST_SHIFT_DELAY_MS / 1000.0 * rate as f32 + 1.0);
        }
    }
}
