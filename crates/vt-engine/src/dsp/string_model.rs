//! A plucked string, modelled as a delay line with damping, dispersion and a curved bridge, from
//! Mutable Instruments' Plaits. Ported to Rust from `string_engine.cc`,
//! `physical_modelling/string_voice.cc` and `physical_modelling/string.cc`, with its main output,
//! limited as Plaits limits it.
//!
//! It plays one string. Plaits keeps three, so one rings on under the next note, which a track's
//! own voice does here instead.
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

use super::plaits_parts::{
    DcBlocker, DelayLine, Limiter, SVF_SHIFT, Svf, interpolate, random, tan_dirty,
};
use super::virtual_analog::Settings;
use crate::rng::Pcg32;

/// How many frames are worked out at once, as in Plaits.
const BLOCK: usize = 24;
const LINE: usize = 1024;
/// How hard every note hits, as Plaits does with nothing patched to its level.
const ACCENT: f32 = 0.8;
/// How loud it is against the other machines.
const LEVEL: f32 = 0.83;

#[inline]
fn semitones(interval: f32) -> f32 {
    (interval / 12.0).exp2()
}

/// One string. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct StringModel {
    rate: f32,
    rng: Pcg32,
    excitation: Svf,
    /// Samples of noise left in the pluck.
    remaining_noise: usize,
    line: DelayLine<LINE>,
    stretch: DelayLine<{ LINE / 4 }>,
    damping_filter: Svf,
    dc_blocker: DcBlocker,
    delay: f32,
    dispersion_noise: f32,
    curved_bridge: f32,
    /// A linear upsampler for notes too low for the line.
    src_phase: f32,
    out_sample: [f32; 2],
    limiter: Limiter,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl StringModel {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32;
        Self {
            rate,
            rng: Pcg32::new(0, 0),
            excitation: Svf::default(),
            remaining_noise: 0,
            line: DelayLine::new(),
            stretch: DelayLine::new(),
            damping_filter: Svf::default(),
            dc_blocker: DcBlocker::new(1.0 - 20.0 / rate),
            delay: 100.0,
            dispersion_noise: 0.0,
            curved_bridge: 0.0,
            src_phase: 0.0,
            out_sample: [0.0; 2],
            limiter: Limiter::default(),
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// Choose the noise of the pluck, so each differs but a render repeats.
    pub fn seed(&mut self, seed: u32) {
        self.rng = Pcg32::new(u64::from(seed), 2);
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.clamp(1e-5, 0.25), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let trigger = self.fresh;
        let structure = settings.harmonics.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        let mut brightness = timbre * timbre;
        let mut damping = settings.morph.clamp(0.0, 1.0);
        brightness += 0.25 * ACCENT * (1.0 - brightness);
        damping += 0.25 * ACCENT * (1.0 - damping);

        // The pluck: a burst of noise as long as one cycle, filtered by how bright it is.
        if trigger {
            let cutoff =
                (4.0 * f0 * semitones((brightness * (2.0 - brightness) - 0.5) * 72.0)).min(0.499);
            self.remaining_noise = (1.0 / f0) as usize;
            self.excitation.set_g_q(tan_dirty(cutoff), 0.5);
        }
        let mut excitation = [0.0; BLOCK];
        let noisy = self.remaining_noise.min(BLOCK);
        self.remaining_noise -= noisy;
        for sample in &mut excitation[..noisy] {
            *sample = 2.0 * random(&mut self.rng) - 1.0;
        }
        for sample in &mut excitation {
            *sample = self.excitation.low_pass(*sample);
        }

        // Below a quarter it's a curved bridge's buzz, above it dispersion, with none between.
        let non_linearity = if structure < 0.24 {
            (structure - 0.24) * 4.166
        } else if structure > 0.26 {
            (structure - 0.26) * 1.351_35
        } else {
            0.0
        };
        let mut output = [0.0; BLOCK];
        self.process(
            f0,
            non_linearity,
            brightness,
            damping,
            &excitation,
            &mut output,
        );
        for (out, sample) in self.output.iter_mut().zip(output) {
            *out = self.limiter.process(1.0, sample);
        }
        self.fresh = false;
    }

    fn process(
        &mut self,
        f0: f32,
        non_linearity: f32,
        mut brightness: f32,
        damping: f32,
        input: &[f32; BLOCK],
        output: &mut [f32; BLOCK],
    ) {
        let dispersion = non_linearity > 0.0;
        let amount = non_linearity.abs();
        let delay = (1.0 / f0).clamp(4.0, LINE as f32 - 4.0);
        // A note too low for the line plays at the lowest it can, upsampled on the fly.
        let mut src_ratio = delay * f0;
        if src_ratio >= 0.9999 {
            self.src_phase = 1.0;
            src_ratio = 1.0;
        }

        let mut damping_cutoff = (12.0 + damping * damping * 60.0 + brightness * 24.0).min(84.0);
        let mut damping_f = (f0 * semitones(damping_cutoff)).min(0.499);
        // Fade towards a string that never stops ringing.
        if damping >= 0.95 {
            let to_infinite = 20.0 * (damping - 0.95);
            brightness += to_infinite * (1.0 - brightness);
            damping_f += to_infinite * (0.4999 - damping_f);
            damping_cutoff += to_infinite * (128.0 - damping_cutoff);
        }
        self.damping_filter.set_fast(damping_f, 0.5);
        let compensation = interpolate(&SVF_SHIFT, damping_cutoff, 1.0);
        let target = delay * compensation;
        if self.fresh {
            self.delay = target;
        }
        let delay_step = (target - self.delay) / BLOCK as f32;

        let stretch_point = amount * (2.0 - amount) * 0.225;
        let stretch_correction = (160.0 / self.rate * delay).clamp(1.0, 2.1);
        let noise_amount_sqrt = if amount > 0.75 {
            4.0 * (amount - 0.75)
        } else {
            0.0
        };
        let noise_amount = noise_amount_sqrt * noise_amount_sqrt * 0.1;
        let noise_filter = 0.06 + 0.94 * brightness * brightness;
        let bridge_curving = amount * amount * 0.01;
        let ap_gain = -0.618 * amount / (0.15 + amount);

        for (out, sample) in output.iter_mut().zip(input) {
            self.src_phase += src_ratio;
            if self.src_phase > 1.0 {
                self.src_phase -= 1.0;
                self.delay += delay_step;
                let mut delay = self.delay;
                let mut s;
                if dispersion {
                    let noise = random(&mut self.rng) - 0.5;
                    self.dispersion_noise += noise_filter * (noise - self.dispersion_noise);
                    delay *= 1.0 + self.dispersion_noise * noise_amount;
                    let ap_delay = delay * stretch_point;
                    let main_delay =
                        delay - ap_delay * (0.408 - stretch_point * 0.308) * stretch_correction;
                    if ap_delay >= 4.0 && main_delay >= 4.0 {
                        s = self.line.read(main_delay);
                        s = self.stretch.allpass(s, ap_delay as usize, ap_gain);
                    } else {
                        s = self.line.read_hermite(delay);
                    }
                } else {
                    delay *= 1.0 - self.curved_bridge * bridge_curving;
                    s = self.line.read_hermite(delay);
                    let value = s.abs() - 0.025;
                    let sign = if s > 0.0 { 1.0 } else { -1.5 };
                    self.curved_bridge = (value.abs() + value) * sign;
                }
                s = (s + sample).clamp(-20.0, 20.0);
                s = self.dc_blocker.process(s);
                s = self.damping_filter.low_pass(s);
                self.line.write(s);
                self.out_sample = [s, self.out_sample[0]];
            }
            let [newer, older] = self.out_sample;
            *out = older + (newer - older) * self.src_phase;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = StringModel::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.25,
            timbre: 0.5,
            morph: 0.5,
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn a_pluck_rings_at_its_note_and_dies_away() {
        let out = run(220.0, plain(), 48_000);
        let loud = peak(&out[..4_800]);
        assert!(loud > 0.05, "{loud}");
        let upward = out[2_400..7_200]
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count();
        assert!((upward as i32 - 22).abs() <= 2, "{upward}");
        assert!(peak(&out[43_200..]) < loud * 0.1, "it rings on");
    }

    #[test]
    fn more_decay_rings_longer() {
        let short = run(220.0, plain(), 24_000);
        let long = run(
            220.0,
            Settings {
                morph: 0.9,
                ..plain()
            },
            24_000,
        );
        assert!(peak(&long[19_200..]) > peak(&short[19_200..]) * 2.0);
    }

    #[test]
    fn each_setting_changes_it() {
        let out = run(220.0, plain(), 4_800);
        for other in [
            Settings {
                harmonics: 0.0,
                ..plain()
            },
            Settings {
                harmonics: 0.9,
                ..plain()
            },
            Settings {
                timbre: 0.9,
                ..plain()
            },
        ] {
            assert_ne!(run(220.0, other, 4_800), out, "{other:?}");
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                        };
                        for sample in run(hertz, settings, 4_800) {
                            assert!(
                                sample.is_finite() && sample.abs() <= 1.0,
                                "{sample} at {hertz} {settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
