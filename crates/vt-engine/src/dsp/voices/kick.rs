//! An 808-style bass drum, from Mutable Instruments' Plaits. Ported to Rust from
//! `drums/analog_bass_drum.h`, `engine/bass_drum_engine.cc`, `fx/overdrive.h`, and the state
//! variable filter in `stmlib/dsp/filter.h`.
//!
//! Copyright 2014, 2016 Emilie Gillet.
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

use std::f32::consts::PI;

use super::virtual_analog::{BLOCK, Moving, Settings};

#[inline]
fn diode(x: f32) -> f32 {
    if x >= 0.0 {
        x
    } else {
        let x = x * 2.0;
        0.7 * x / (1.0 + x.abs())
    }
}

#[inline]
fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Kick {
    rate: f32,
    trigger: bool,
    pulse_remaining: i32,
    fm_pulse_remaining: i32,
    pulse: f32,
    pulse_height: f32,
    pulse_lp: f32,
    fm_pulse_lp: f32,
    retrig_pulse: f32,
    lp_out: f32,
    tone_lp: f32,
    /// The resonator: a state variable filter's two states.
    state_1: f32,
    state_2: f32,
    pre_gain: f32,
    post_gain: f32,
    output: [f32; BLOCK],
    at: usize,
}

impl Kick {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate as f32,
            trigger: true,
            pulse_remaining: 0,
            fm_pulse_remaining: 0,
            pulse: 0.0,
            pulse_height: 0.0,
            pulse_lp: 0.0,
            fm_pulse_lp: 0.0,
            retrig_pulse: 0.0,
            lp_out: 0.0,
            tone_lp: 0.0,
            state_1: 0.0,
            state_2: 0.0,
            pre_gain: 0.0,
            post_gain: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.max(1e-5), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample
    }

    fn render(&mut self, f0: f32, settings: Settings) {
        let Settings {
            harmonics,
            timbre: tone,
            morph: decay,
            accent,
            ..
        } = settings;
        let attack_fm_amount = (harmonics * 4.0).min(1.0);
        let self_fm_amount = (harmonics * 4.0 - 1.0).clamp(0.0, 1.0);
        let drive = (harmonics * 2.0 - 1.0).max(0.0) * (1.0 - 16.0 * f0).max(0.0);

        let pulse_decay = 1.0 - 1.0 / (0.2e-3 * self.rate);
        let pulse_filter = 1.0 / (0.1e-3 * self.rate);
        let retrig_decay = 1.0 - 1.0 / (0.05 * self.rate);
        let scale = 0.001 / f0;
        let q = 1500.0 * 2_f32.powf(decay * 80.0 / 12.0);
        let tone_f = (4.0 * f0 * 2_f32.powf(tone * 108.0 / 12.0)).min(1.0);
        let exciter_leak = 0.08 * (tone + 0.25);

        let fresh = self.trigger;
        if self.trigger {
            self.pulse_remaining = (1.0e-3 * self.rate) as i32;
            self.fm_pulse_remaining = (6.0e-3 * self.rate) as i32;
            self.pulse_height = 3.0 + 7.0 * accent;
            self.lp_out = 0.0;
            self.trigger = false;
        }

        for out in &mut self.output {
            let mut pulse = if self.pulse_remaining > 0 {
                self.pulse_remaining -= 1;
                self.pulse = if self.pulse_remaining > 0 {
                    self.pulse_height
                } else {
                    self.pulse_height - 1.0
                };
                self.pulse
            } else {
                self.pulse *= pulse_decay;
                self.pulse
            };
            self.pulse_lp += (pulse - self.pulse_lp) * pulse_filter;
            pulse = diode((pulse - self.pulse_lp) + pulse * 0.044);

            let mut fm_pulse = 0.0;
            if self.fm_pulse_remaining > 0 {
                self.fm_pulse_remaining -= 1;
                fm_pulse = 1.0;
                self.retrig_pulse = if self.fm_pulse_remaining > 0 {
                    0.0
                } else {
                    -0.8
                };
            } else {
                self.retrig_pulse *= retrig_decay;
            }
            self.fm_pulse_lp += (fm_pulse - self.fm_pulse_lp) * pulse_filter;

            let punch = 0.7 + diode(10.0 * self.lp_out - 1.0);
            let attack_fm = self.fm_pulse_lp * 1.7 * attack_fm_amount;
            let self_fm = punch * 0.08 * self_fm_amount;
            let f = (f0 * (1.0 + attack_fm + self_fm)).clamp(0.0, 0.4);

            // A band-pass and low-pass from one state variable filter.
            let a = 0.3736 * PI * PI * PI;
            let g = f * (PI + a * f * f);
            let r = 1.0 / (1.0 + q * f);
            let h = 1.0 / (1.0 + r * g + g * g);
            let input = (pulse - self.retrig_pulse * 0.2) * scale;
            let hp = (input - r * self.state_1 - g * self.state_1 - self.state_2) * h;
            let bp = g * hp + self.state_1;
            self.state_1 = g * hp + bp;
            let lp = g * bp + self.state_2;
            self.state_2 = g * bp + lp;
            self.lp_out = lp;

            self.tone_lp += (pulse * exciter_leak + bp - self.tone_lp) * tone_f;
            *out = self.tone_lp;
        }

        // Overdrive.
        let drive = 0.5 + 0.5 * drive;
        let drive_2 = drive * drive;
        let pre_a = drive * 0.5;
        let pre_b = drive_2 * drive_2 * drive * 24.0;
        let pre_gain = pre_a + (pre_b - pre_a) * drive_2;
        let squashed = drive * (2.0 - drive);
        let post_gain = 1.0 / soft_clip(0.33 + squashed * (pre_gain - 0.33));
        if fresh {
            self.pre_gain = pre_gain;
            self.post_gain = post_gain;
        }
        let mut pre = Moving::towards(self.pre_gain, pre_gain);
        let mut post = Moving::towards(self.post_gain, post_gain);
        for out in &mut self.output {
            *out = soft_clip(pre.next() * *out) * post.next();
        }
        self.pre_gain = pre_gain;
        self.post_gain = post_gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Kick::new(RATE);
        (0..frames)
            .map(|_| voice.next(hertz / RATE as f32, settings))
            .collect()
    }

    fn plain() -> Settings {
        Settings {
            harmonics: 0.5,
            timbre: 0.3,
            morph: 0.5,
            ..Settings::default()
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |peak: f32, s| peak.max(s.abs()))
    }

    #[test]
    fn it_hits_and_dies_away_on_its_own() {
        let out = run(55.0, plain(), 96_000);
        assert!(peak(&out[..4_800]) > 0.2, "{}", peak(&out[..4_800]));
        assert!(peak(&out[86_400..]) < peak(&out[..4_800]) * 0.05);
    }

    #[test]
    fn morph_is_how_long_it_rings() {
        let tail = |morph: f32| {
            let settings = Settings { morph, ..plain() };
            peak(&run(55.0, settings, 48_000)[19_200..])
        };
        assert!(tail(0.8) > tail(0.2) * 2.0, "{} {}", tail(0.8), tail(0.2));
    }

    #[test]
    fn timbre_brightens_it_and_harm_adds_punch() {
        let roughness =
            |samples: &[f32]| -> f32 { samples.windows(2).map(|p| (p[1] - p[0]).powi(2)).sum() };
        let dark = Settings {
            timbre: 0.0,
            ..plain()
        };
        let bright = Settings {
            timbre: 1.0,
            ..plain()
        };
        assert!(roughness(&run(55.0, bright, 4_800)) > roughness(&run(55.0, dark, 4_800)));
        let punchy = Settings {
            harmonics: 1.0,
            ..plain()
        };
        assert_ne!(run(55.0, punchy, 4_800), run(55.0, plain(), 4_800));
    }

    #[test]
    fn its_note_sets_its_pitch() {
        let upward = |hertz: f32| {
            run(hertz, plain(), 24_000)[2_400..]
                .windows(2)
                .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
                .count()
        };
        assert!(upward(110.0) > upward(55.0) + upward(55.0) / 2);
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 220.0, 1_000.0, 8_000.0, 30_000.0] {
            for harmonics in [0.0, 0.25, 0.5, 1.0] {
                for timbre in [0.0, 0.5, 1.0] {
                    for morph in [0.0, 0.5, 1.0] {
                        let settings = Settings {
                            harmonics,
                            timbre,
                            morph,
                            ..Settings::default()
                        };
                        for sample in run(hertz, settings, 4_800) {
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
}
