//! An oscillator that morphs from a triangle to a saw to a square, with hard sync and a phase
//! output for phase distortion, from Mutable Instruments' Plaits. Ported to Rust from
//! `variable_shape_oscillator.h`, for the phase distortion and filtered virtual analog voices.
//!
//! Copyright 2016 Emilie Gillet.
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

use super::virtual_analog::{
    Moving, next_blep, next_integrated_blep, this_blep, this_integrated_blep,
};

/// The highest frequency it runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;

/// A triangle at `waveshape` 0, a saw at a half and a square at 1, each leaning by `pw`.
///
/// With `SYNC`, a master frequency resets its phase. With `PHASE`, it gives the master's phase
/// moved by the waveform, for a sine to read, rather than the waveform itself.
#[derive(Debug, Clone, Copy)]
pub(super) struct VariableShape {
    master_phase: f32,
    slave_phase: f32,
    next_sample: f32,
    previous_pw: f32,
    high: bool,
    master_frequency: f32,
    slave_frequency: f32,
    pw: f32,
    waveshape: f32,
    phase_modulation: f32,
    fresh: bool,
}

impl VariableShape {
    pub(super) fn new() -> Self {
        Self {
            master_phase: 0.0,
            slave_phase: 0.0,
            next_sample: 0.0,
            previous_pw: 0.5,
            high: false,
            master_frequency: 0.0,
            slave_frequency: 0.01,
            pw: 0.5,
            waveshape: 0.0,
            phase_modulation: 0.0,
            fresh: true,
        }
    }

    #[inline]
    fn naive(phase: f32, pw: f32, up: f32, down: f32, triangle: f32, square: f32) -> f32 {
        let mut saw = phase;
        let square_wave = if phase < pw { 0.0 } else { 1.0 };
        let triangle_wave = if phase < pw {
            phase * up
        } else {
            1.0 - (phase - pw) * down
        };
        saw += (square_wave - saw) * square;
        saw += (triangle_wave - saw) * triangle;
        saw
    }

    /// Start somewhere other than the top of its cycle, before it first plays. It's for a free
    /// wave, without `SYNC` or `PHASE`.
    pub(super) fn start_at(&mut self, phase: f32) {
        self.slave_phase = phase;
    }

    /// Fill `out`, with the settings moving in a line across it from where the last call left
    /// them. The first call starts at them.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render<const SYNC: bool, const PHASE: bool>(
        &mut self,
        master_frequency: f32,
        frequency: f32,
        pw: f32,
        waveshape: f32,
        phase_modulation: f32,
        out: &mut [f32],
    ) {
        let master_frequency = master_frequency.min(MAX_FREQUENCY);
        let frequency = frequency.min(MAX_FREQUENCY);
        let pw = if frequency >= 0.25 {
            0.5
        } else {
            pw.clamp(frequency * 2.0, 1.0 - 2.0 * frequency)
        };
        if self.fresh {
            self.master_frequency = master_frequency;
            self.slave_frequency = frequency;
            self.pw = pw;
            self.waveshape = waveshape;
            self.phase_modulation = phase_modulation;
            if self.slave_phase > 0.0 {
                let square = (waveshape - 0.5).max(0.0) * 2.0;
                let triangle = (1.0 - waveshape * 2.0).max(0.0);
                let (up, down) = (1.0 / pw, 1.0 / (1.0 - pw));
                self.high = self.slave_phase >= pw;
                self.next_sample = Self::naive(self.slave_phase, pw, up, down, triangle, square);
            }
            self.fresh = false;
        }
        let frames = out.len();
        let mut master_fm = Moving::across(self.master_frequency, master_frequency, frames);
        let mut fm = Moving::across(self.slave_frequency, frequency, frames);
        let mut pwm = Moving::across(self.pw, pw, frames);
        let mut shape = Moving::across(self.waveshape, waveshape, frames);
        let mut pm = Moving::across(self.phase_modulation, phase_modulation, frames);

        let mut next_sample = self.next_sample;
        for sample in out.iter_mut() {
            let mut reset = false;
            let mut transition_during_reset = false;
            let mut reset_time = 0.0;
            let mut this_sample = next_sample;
            next_sample = 0.0;

            let master_frequency = master_fm.next();
            let slave_frequency = fm.next();
            let pw = pwm.next();
            let waveshape = shape.next();
            let square = (waveshape - 0.5).max(0.0) * 2.0;
            let triangle = (1.0 - waveshape * 2.0).max(0.0);
            let up = 1.0 / pw;
            let down = 1.0 / (1.0 - pw);

            if SYNC {
                self.master_phase += master_frequency;
                if self.master_phase >= 1.0 {
                    self.master_phase -= 1.0;
                    reset_time = self.master_phase / master_frequency;
                    let mut phase_at_reset =
                        self.slave_phase + (1.0 - reset_time) * slave_frequency;
                    reset = true;
                    if phase_at_reset >= 1.0 {
                        phase_at_reset -= 1.0;
                        transition_during_reset = true;
                    }
                    if !self.high && phase_at_reset >= pw {
                        transition_during_reset = true;
                    }
                    let value = Self::naive(phase_at_reset, pw, up, down, triangle, square);
                    this_sample -= value * this_blep(reset_time);
                    next_sample -= value * next_blep(reset_time);
                }
            } else if PHASE {
                self.master_phase += master_frequency;
                if self.master_phase >= 1.0 {
                    self.master_phase -= 1.0;
                }
            }

            self.slave_phase += slave_frequency;
            // At most two edges can fall in a frame. A fixed count means a bad value can't hang the
            // audio thread, where the original loops until it breaks.
            if transition_during_reset || !reset {
                for _ in 0..4 {
                    let step = (up + down) * slave_frequency * triangle;
                    if !self.high {
                        if self.slave_phase < pw {
                            break;
                        }
                        let t = (self.slave_phase - pw) / (self.previous_pw - pw + slave_frequency);
                        this_sample += square * this_blep(t);
                        next_sample += square * next_blep(t);
                        this_sample -= step * this_integrated_blep(t);
                        next_sample -= step * next_integrated_blep(t);
                        self.high = true;
                    }
                    if self.high {
                        if self.slave_phase < 1.0 {
                            break;
                        }
                        self.slave_phase -= 1.0;
                        let t = self.slave_phase / slave_frequency;
                        this_sample -= (1.0 - triangle) * this_blep(t);
                        next_sample -= (1.0 - triangle) * next_blep(t);
                        this_sample += step * this_integrated_blep(t);
                        next_sample += step * next_integrated_blep(t);
                        self.high = false;
                    }
                }
            }
            if SYNC && reset {
                self.slave_phase = reset_time * slave_frequency;
                self.high = false;
            }

            next_sample += Self::naive(self.slave_phase, pw, up, down, triangle, square);
            self.previous_pw = pw;

            *sample = if PHASE {
                let mut phasor = self.master_phase;
                if SYNC {
                    // Fade the waveform out where the phase wraps, so the wrap doesn't click.
                    let w = 4.0 * (1.0 - self.master_phase) * self.master_phase;
                    this_sample *= w * (2.0 - w);
                    // And lean the main phase as the waveform leans.
                    let p2 = phasor * phasor;
                    phasor += (p2 * p2 - phasor) * (pw - 0.5).abs() * 2.0;
                }
                phasor + pm.next() * this_sample
            } else {
                2.0 * this_sample - 1.0
            };
        }
        self.next_sample = next_sample;
        self.master_frequency = master_frequency;
        self.slave_frequency = frequency;
        self.pw = pw;
        self.waveshape = waveshape;
        self.phase_modulation = phase_modulation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saw_runs_from_bottom_to_top() {
        let mut oscillator = VariableShape::new();
        let mut out = [0.0; 480];
        for _ in 0..10 {
            oscillator.render::<false, false>(0.0, 0.01, 0.5, 0.5, 0.0, &mut out);
        }
        let (low, high) = out
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)));
        assert!(low < -0.9 && high > 0.9, "{low} {high}");
    }
}
