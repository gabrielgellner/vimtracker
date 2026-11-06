//! A variable square and a variable saw, from the virtual analog model of Mutable Instruments'
//! Plaits: its `VA_VARIANT 2`, main output. Ported to Rust from `virtual_analog_engine.cc`,
//! `variable_shape_oscillator.h`, `variable_saw_oscillator.h` and `stmlib/dsp/polyblep.h`.
//!
//! Copyright 2016, 2017 Emilie Gillet.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

/// How many frames are worked out at once, with settings moving in a line across them.
pub(super) const BLOCK: usize = 16;
/// The highest frequency an oscillator runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;
const NOTCH_DEPTH: f32 = 0.2;
/// The intervals `Harm` moves between, in semitones.
const INTERVALS: [f32; 5] = [0.0, 7.01, 12.01, 19.01, 24.01];

/// The three settings, each on `0..=1`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Detunes the saw from the square, with the middle in tune.
    pub harmonics: f32,
    /// The square, from nothing to a narrow pulse, to a full square, to hard sync.
    pub timbre: f32,
    /// The saw, from a triangle to a saw, to a saw with a widening notch.
    pub morph: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            harmonics: 0.5,
            timbre: 0.0,
            morph: 0.5,
        }
    }
}

#[inline]
fn this_blep(t: f32) -> f32 {
    0.5 * t * t
}

#[inline]
fn next_blep(t: f32) -> f32 {
    let t = 1.0 - t;
    -0.5 * t * t
}

#[inline]
pub(super) fn next_integrated_blep(t: f32) -> f32 {
    let t1 = 0.5 * t;
    let t2 = t1 * t1;
    let t4 = t2 * t2;
    0.1875 - t1 + 1.5 * t2 - t4
}

#[inline]
pub(super) fn this_integrated_blep(t: f32) -> f32 {
    next_integrated_blep(1.0 - t)
}

#[inline]
fn squash(x: f32) -> f32 {
    x * x * (3.0 - 2.0 * x)
}

#[inline]
fn semitones(interval: f32) -> f32 {
    2_f32.powf(interval / 12.0)
}

/// A setting moving in a line across a block, and where it was left.
#[derive(Debug, Clone, Copy)]
pub(super) struct Moving {
    value: f32,
    step: f32,
}

impl Moving {
    #[inline]
    pub(super) fn towards(state: f32, target: f32) -> Self {
        Self {
            value: state,
            step: (target - state) / BLOCK as f32,
        }
    }

    #[inline]
    pub(super) fn next(&mut self) -> f32 {
        self.value += self.step;
        self.value
    }
}

/// How `Harm` detunes the saw, in semitones.
fn detuning(harmonics: f32) -> f32 {
    let detune = (2.05 * harmonics - 1.025).clamp(-1.0, 1.0);
    let sign = if detune < 0.0 { -1.0 } else { 1.0 };
    let detune = detune * sign * 3.9999;
    let whole = detune as usize;
    let fraction = detune - whole as f32;
    let (a, b) = (INTERVALS[whole], INTERVALS[whole + 1]);
    (a + (b - a) * squash(squash(fraction))) * sign
}

/// A square whose phase a master frequency resets, for hard sync.
#[derive(Debug, Clone, Copy)]
struct SyncedSquare {
    master_phase: f32,
    slave_phase: f32,
    next_sample: f32,
    previous_pw: f32,
    high: bool,
    master_frequency: f32,
    slave_frequency: f32,
    pw: f32,
    fresh: bool,
}

impl SyncedSquare {
    fn new() -> Self {
        Self {
            master_phase: 0.0,
            slave_phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            master_frequency: 0.0,
            slave_frequency: 0.01,
            pw: 0.5,
            fresh: true,
        }
    }

    #[inline]
    fn naive(phase: f32, pw: f32) -> f32 {
        if phase < pw { 0.0 } else { 1.0 }
    }

    fn render(&mut self, master: f32, slave: f32, pw: f32, out: &mut [f32; BLOCK]) {
        let master = master.min(MAX_FREQUENCY);
        let slave = slave.min(MAX_FREQUENCY);
        let pw = if slave >= 0.25 {
            0.5
        } else {
            pw.clamp(slave * 2.0, 1.0 - 2.0 * slave)
        };
        if self.fresh {
            (self.master_frequency, self.slave_frequency, self.pw) = (master, slave, pw);
            self.fresh = false;
        }
        let mut master_fm = Moving::towards(self.master_frequency, master);
        let mut slave_fm = Moving::towards(self.slave_frequency, slave);
        let mut pwm = Moving::towards(self.pw, pw);
        let mut next_sample = self.next_sample;
        for sample in out.iter_mut() {
            let mut reset = false;
            let mut transition_during_reset = false;
            let mut reset_time = 0.0;
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let master_frequency = master_fm.next();
            let slave_frequency = slave_fm.next();
            let pw = pwm.next();

            self.master_phase += master_frequency;
            if self.master_phase >= 1.0 {
                self.master_phase -= 1.0;
                reset_time = self.master_phase / master_frequency;
                let mut phase_at_reset = self.slave_phase + (1.0 - reset_time) * slave_frequency;
                reset = true;
                if phase_at_reset >= 1.0 {
                    phase_at_reset -= 1.0;
                    transition_during_reset = true;
                }
                if !self.high && phase_at_reset >= pw {
                    transition_during_reset = true;
                }
                let value = Self::naive(phase_at_reset, pw);
                this_sample -= value * this_blep(reset_time);
                next_sample -= value * next_blep(reset_time);
            }

            self.slave_phase += slave_frequency;
            // At most two edges can fall in a frame. A fixed count means a bad value can't hang the
            // audio thread, where the original loops until it breaks.
            if transition_during_reset || !reset {
                for _ in 0..4 {
                    if !self.high {
                        if self.slave_phase < pw {
                            break;
                        }
                        let t = (self.slave_phase - pw) / (self.previous_pw - pw + slave_frequency);
                        this_sample += this_blep(t);
                        next_sample += next_blep(t);
                        self.high = true;
                    }
                    if self.high {
                        if self.slave_phase < 1.0 {
                            break;
                        }
                        self.slave_phase -= 1.0;
                        let t = self.slave_phase / slave_frequency;
                        this_sample -= this_blep(t);
                        next_sample -= next_blep(t);
                        self.high = false;
                    }
                }
            }
            if reset {
                self.slave_phase = reset_time * slave_frequency;
                self.high = false;
            }
            next_sample += Self::naive(self.slave_phase, pw);
            self.previous_pw = pw;
            *sample = 2.0 * this_sample - 1.0;
        }
        self.next_sample = next_sample;
        (self.master_frequency, self.slave_frequency, self.pw) = (master, slave, pw);
    }
}

/// A saw with a variable slope or notch.
#[derive(Debug, Clone, Copy)]
struct VariableSaw {
    phase: f32,
    next_sample: f32,
    previous_pw: f32,
    high: bool,
    frequency: f32,
    pw: f32,
    waveshape: f32,
    fresh: bool,
}

impl VariableSaw {
    fn new() -> Self {
        Self {
            phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            frequency: 0.01,
            pw: 0.5,
            waveshape: 0.0,
            fresh: true,
        }
    }

    #[inline]
    fn naive(phase: f32, pw: f32, up: f32, down: f32, triangle: f32, notch: f32) -> f32 {
        let notch_saw = if phase < pw { phase } else { 1.0 + NOTCH_DEPTH };
        let slope = if phase < pw {
            phase * up
        } else {
            1.0 - (phase - pw) * down
        };
        notch_saw * notch + slope * triangle
    }

    fn render(&mut self, frequency: f32, pw: f32, waveshape: f32, out: &mut [f32; BLOCK]) {
        let frequency = frequency.min(MAX_FREQUENCY);
        let pw = if frequency >= 0.25 {
            0.5
        } else {
            pw.clamp(frequency * 2.0, 1.0 - 2.0 * frequency)
        };
        if self.fresh {
            (self.frequency, self.pw, self.waveshape) = (frequency, pw, waveshape);
            self.fresh = false;
        }
        let mut fm = Moving::towards(self.frequency, frequency);
        let mut pwm = Moving::towards(self.pw, pw);
        let mut shape = Moving::towards(self.waveshape, waveshape);
        let mut next_sample = self.next_sample;
        for sample in out.iter_mut() {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let frequency = fm.next();
            let pw = pwm.next();
            let triangle = shape.next();
            let notch = 1.0 - triangle;
            let up = 1.0 / pw;
            let down = 1.0 / (1.0 - pw);

            self.phase += frequency;
            if !self.high && self.phase >= pw {
                let step = (up + down) * frequency * triangle;
                let height = (NOTCH_DEPTH + 1.0 - pw) * notch;
                let t = (self.phase - pw) / (self.previous_pw - pw + frequency);
                this_sample += height * this_blep(t);
                next_sample += height * next_blep(t);
                this_sample -= step * this_integrated_blep(t);
                next_sample -= step * next_integrated_blep(t);
                self.high = true;
            } else if self.phase >= 1.0 {
                self.phase -= 1.0;
                let step = (up + down) * frequency * triangle;
                let height = (NOTCH_DEPTH + 1.0) * notch;
                let t = self.phase / frequency;
                this_sample -= height * this_blep(t);
                next_sample -= height * next_blep(t);
                this_sample += step * this_integrated_blep(t);
                next_sample += step * next_integrated_blep(t);
                self.high = false;
            }
            next_sample += Self::naive(self.phase, pw, up, down, triangle, notch);
            self.previous_pw = pw;
            *sample = (2.0 * this_sample - 1.0) / (1.0 + NOTCH_DEPTH);
        }
        self.next_sample = next_sample;
        (self.frequency, self.pw, self.waveshape) = (frequency, pw, waveshape);
    }
}

/// One voice of the model. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct VirtualAnalog {
    square: SyncedSquare,
    saw: VariableSaw,
    square_gain: f32,
    saw_gain: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl VirtualAnalog {
    pub fn new() -> Self {
        Self {
            square: SyncedSquare::new(),
            saw: VariableSaw::new(),
            square_gain: 0.0,
            saw_gain: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency, settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let Settings {
            harmonics,
            timbre,
            morph,
        } = settings;
        let saw_frequency = frequency * semitones(detuning(harmonics));

        let square_pw = (1.3 * timbre - 0.15).clamp(0.005, 0.5);
        let sync_ratio = if timbre < 0.5 {
            0.0
        } else {
            (timbre - 0.5) * (timbre - 0.5) * 4.0 * 48.0
        };
        let square_gain = (timbre * 8.0).min(1.0);

        let saw_pw = if morph < 0.5 {
            morph + 0.5
        } else {
            1.0 - (morph - 0.5) * 2.0
        };
        let saw_pw = (saw_pw * 1.1).clamp(0.005, 1.0);
        let saw_shape = (10.0 - 21.0 * morph).clamp(0.0, 1.0);
        let saw_gain = (8.0 * (1.0 - morph)).clamp(0.02, 1.0);

        let mut square = [0.0; BLOCK];
        let synced = frequency * semitones(sync_ratio);
        self.square
            .render(frequency, synced, square_pw, &mut square);
        self.saw
            .render(saw_frequency, saw_pw, saw_shape, &mut self.output);

        let norm = 1.0 / square_gain.max(saw_gain);
        let (square_target, saw_target) = (square_gain * 0.3 * norm, saw_gain * 0.5 * norm);
        if self.fresh {
            (self.square_gain, self.saw_gain) = (square_target, saw_target);
            self.fresh = false;
        }
        let mut square_level = Moving::towards(self.square_gain, square_target);
        let mut saw_level = Moving::towards(self.saw_gain, saw_target);
        for (out, square) in self.output.iter_mut().zip(square) {
            *out = *out * saw_level.next() + square * square_level.next();
        }
        (self.square_gain, self.saw_gain) = (square_target, saw_target);
    }
}

impl Default for VirtualAnalog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = VirtualAnalog::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    /// The frequency from upward crossings of the middle, past the start.
    fn measured_pitch(samples: &[f32]) -> f32 {
        let crossings: Vec<usize> = samples
            .windows(2)
            .enumerate()
            .filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0)
            .map(|(at, _)| at)
            .collect();
        let (first, last) = (crossings[1], crossings[crossings.len() - 1]);
        (crossings.len() - 2) as f32 * RATE / (last - first) as f32
    }

    #[test]
    fn the_middle_of_harm_is_in_tune_and_its_ends_are_two_octaves_away() {
        assert_eq!(detuning(0.5), 0.0);
        assert!((detuning(1.0) - 24.01).abs() < 0.01);
        assert!((detuning(0.0) + 24.01).abs() < 0.01);
    }

    #[test]
    fn a_saw_plays_the_note_it_is_asked_for() {
        let out = run(110.0, Settings::default(), 48_000);
        let pitch = measured_pitch(&out);
        assert!((pitch - 110.0).abs() < 1.0, "{pitch}");
    }

    #[test]
    fn a_square_plays_the_note_it_is_asked_for() {
        // All square, as the saw is as quiet as it goes.
        let settings = Settings {
            harmonics: 0.5,
            timbre: 0.45,
            morph: 1.0,
        };
        let pitch = measured_pitch(&run(220.0, settings, 48_000));
        assert!((pitch - 220.0).abs() < 2.0, "{pitch}");
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 0.75, 1.0] {
                for timbre in [0.0, 0.1, 0.4, 0.5, 0.75, 1.0] {
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                        };
                        for sample in run(hertz, settings, 2_000) {
                            assert!(
                                sample.is_finite() && sample.abs() < 2.0,
                                "{sample} at {hertz} {settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn moving_settings_while_it_plays_stays_in_range() {
        let mut voice = VirtualAnalog::new();
        for frame in 0..48_000 {
            let sweep = (frame as f32 / 48_000.0 * 20.0).sin() * 0.5 + 0.5;
            let settings = Settings {
                harmonics: sweep,
                timbre: sweep,
                morph: 1.0 - sweep,
            };
            let sample = voice.next((55.0 + sweep * 2_000.0) / RATE, settings);
            assert!(sample.is_finite() && sample.abs() < 2.0, "{sample}");
        }
    }

    /// A rough measure of energy near the top of the spectrum: how much each frame jumps.
    fn roughness(samples: &[f32]) -> f32 {
        let total: f32 = samples
            .windows(3)
            .map(|w| (w[0] - 2.0 * w[1] + w[2]).powi(2))
            .sum();
        total / samples.len() as f32
    }

    #[test]
    fn its_edges_are_smoothed_compared_to_a_plain_saw() {
        let hertz = 3_000.0;
        let band_limited = run(hertz, Settings::default(), 48_000);
        let mut phase = 0.0_f32;
        let plain: Vec<f32> = (0..48_000)
            .map(|_| {
                phase = (phase + hertz / RATE).fract();
                (2.0 * phase - 1.0) * 0.5 / (1.0 + NOTCH_DEPTH)
            })
            .collect();
        assert!(
            roughness(&band_limited) < roughness(&plain) * 0.8,
            "{} against {}",
            roughness(&band_limited),
            roughness(&plain)
        );
    }

    #[test]
    fn timbre_and_morph_change_the_sound() {
        let base = run(110.0, Settings::default(), 4_800);
        let square = Settings {
            timbre: 0.4,
            ..Settings::default()
        };
        let triangle = Settings {
            morph: 0.0,
            ..Settings::default()
        };
        assert_ne!(base, run(110.0, square, 4_800));
        assert!(roughness(&run(110.0, triangle, 4_800)) < roughness(&base));
    }
}
