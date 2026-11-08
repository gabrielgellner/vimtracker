//! An acid bass voice after Roland's TB-303: a saw, or the square a 303 shapes from it, through a
//! diode ladder, with the 303's two envelopes, its accent and its slide. It plays one note at a
//! time, and a new note carries on from the last one's filter and accent rather than starting
//! fresh, as the one voice of a 303 does.
//!
//! How it works comes from Robin Whittle's notes on the 303 and the Devil Fish, and Tim
//! Stinchcombe's analysis of its diode ladder. Several measured values come from Robin Schmidt's
//! Open303: the cutoff's range and how the envelope moves it, the share of the filter envelope that
//! reaches the level, the level envelope's decay, and the high-pass before the filter. The filter is
//! Vital's, in [`super::diode_filter`].
//!
//! Open303 is copyright (c) 2009 Robin Schmidt (www.rs-met.com), under the MIT license:
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

use super::diode_filter::{DiodeFilter, RESONANT_SHARE};
use super::drums::soft_clip;
use super::oversampler::Oversampler;
use super::smoother::{Smoothed, coefficient as smoothing_coefficient};
use super::virtual_analog::Settings;
use vt_model::settings::slide_ms;
use vt_model::settings::{ACID_HIGHEST_CUTOFF, ACID_LOWEST_CUTOFF, acid_decay_ms, acid_drive_db};

/// How loud it is against the other voices: a plain note is about as loud as the virtual analog
/// voice. An accented note at full `Accent` peaks nearly three times as high, which a soft clip
/// rounds off, as a 303 driven hard into what follows it would be.
const LEVEL: f32 = 0.4;
/// The square's share of each cycle spent high. A 303 makes its square by driving its saw hard
/// into a transistor with an offset, which Open303 models as `tanh(70 saw + 4.37)`: that switches
/// a little before the saw's middle, so the square isn't quite even.
const SQUARE_DUTY: f32 = 0.531;
/// The filter envelope's decay on an accented note, in ms, whatever `Decay` says.
const ACCENT_DECAY_MS: f32 = 200.0;
/// The level envelope's fixed decay, as a time constant in ms. It falls to silence over about
/// three seconds, as a 303's does.
const LEVEL_DECAY_MS: f32 = 1230.0;
/// The level envelope's attack, as a time constant: a 303's takes about 3 ms.
const ATTACK_MS: f32 = 1.0;
/// How fast the level falls once the note ends: a 303 ends a plain note in about 16 ms, and an
/// accented one more slowly.
const RELEASE_MS: f32 = 4.0;
const ACCENT_RELEASE_MS: f32 = 50.0;
/// The filter envelope also reaches the level, through 47k and 33 nF on a 303, which softens it.
const ENVELOPE_TO_LEVEL: f32 = 0.45;
const SOFTENING_MS: f32 = 1.55;
/// How much the accent sweep adds to the level, against the level envelope's 1. Open303 adds 4,
/// for a full accent 11 dB over a plain note; this is gentler, at about 9 dB.
const ACCENT_TO_LEVEL: f32 = 2.5;
/// How far the accent sweep moves the cutoff at its height, in octaves.
const ACCENT_OCTAVES: f32 = 2.0;
/// The accent sweep's capacitor charges through a diode and 147k, and drains through 100k, on
/// 1 µF. It drains more slowly than accents come, so they build up.
const SWEEP_CHARGE_MS: f32 = 147.0;
const SWEEP_DRAIN_MS: f32 = 100.0;
/// What brings the capacitor's level up to the pulse's, where the two balance.
const SWEEP_GAIN: f32 = (SWEEP_CHARGE_MS + SWEEP_DRAIN_MS) / SWEEP_DRAIN_MS;
/// A slide's time constant, as a share of its time, so it's nearly there when its time is up.
const SLIDE_SHARE: f32 = 1.0 / 3.0;
/// The high-pass before the filter, in Hz, as Open303 measured a 303's.
const HIGH_PASS_HZ: f32 = 44.486;
/// The level's smoothing, so nothing that jumps in it clicks: Open303's 200 Hz.
const DECLICK_HZ: f32 = 200.0;
/// The highest cutoff the envelope and accent can take it to, as on the Devil Fish.
const HIGHEST_HZ: f32 = 20_000.0;

/// How many decibels double a level.
const DECIBELS_PER_OCTAVE: f32 = 6.020_6;

/// What's smoothed, by place among the voice's settings.
const CUTOFF: usize = 0;
const RESONANCE: usize = 1;
const ENVELOPE_MOD: usize = 2;
const DECAY: usize = 3;
const ACCENT: usize = 4;
const SLIDE: usize = 5;
const DRIVE: usize = 6;
const SMOOTHED: usize = 7;
/// How many frames go by between working out what follows from the settings, which are smoothed,
/// so they move little in that time.
const CONTROL_FRAMES: u32 = 16;

/// How much of the rest of the way a one-pole covers each frame, for a time constant in ms.
fn one_pole(ms: f32, rate: f32) -> f32 {
    1.0 - (-1000.0 / (ms * rate)).exp()
}

/// What to multiply by each frame to fall with a time constant in ms.
fn falling(ms: f32, rate: f32) -> f32 {
    (-1000.0 / (ms * rate)).exp()
}

/// The correction for a jump at `phase`, so a wave that jumps there doesn't alias: a two-frame
/// polynomial step.
#[inline]
fn blep(phase: f32, increment: f32) -> f32 {
    if phase < increment {
        let t = phase / increment;
        t + t - t * t - 1.0
    } else if phase > 1.0 - increment {
        let t = (phase - 1.0) / increment;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// One voice. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Acid {
    rate: f32,
    phase: f32,
    /// The frequency it plays now, as a fraction of the rate, which a slide moves toward the note.
    heard: f32,
    sliding: bool,
    /// Whether the next frame takes its note's pitch at once, as a new note does.
    jump: bool,
    accented: bool,
    gate: bool,
    /// The filter envelope, and the same softened on its way to the level.
    envelope: f32,
    softened: f32,
    /// The accent sweep's capacitor.
    sweep: f32,
    /// The level envelope, whether it's still rising, and how much of the filter envelope and
    /// accent reach the level, which falls with it once the note ends.
    level: f32,
    attacking: bool,
    open: f32,
    /// The level heard, smoothed.
    amplitude: f32,
    high_pass: f32,
    filter: DiodeFilter,
    oversampler: Oversampler,
    smoothed: [Smoothed; SMOOTHED],
    smoothing: f32,
    fresh: bool,
    /// The cutoff it played the latest frame at, in Hz.
    cutoff: f32,
    /// What follows from the settings, worked out every so many frames: the filter envelope's
    /// fall each frame, and how far a slide goes each frame.
    countdown: u32,
    decay: f32,
    slide: f32,
    // Coefficients for the fixed times.
    attack: f32,
    level_decay: f32,
    release: f32,
    accent_release: f32,
    softening: f32,
    charge: f32,
    drain: f32,
    declick: f32,
    high_pass_coefficient: f32,
}

impl Acid {
    pub fn new(rate: u32) -> Self {
        let rate = rate as f32;
        Self {
            rate,
            phase: 0.0,
            heard: 0.0,
            sliding: false,
            jump: true,
            accented: false,
            gate: false,
            envelope: 0.0,
            softened: 0.0,
            sweep: 0.0,
            level: 0.0,
            attacking: false,
            open: 0.0,
            amplitude: 0.0,
            high_pass: 0.0,
            filter: DiodeFilter::new(rate * 2.0),
            oversampler: Oversampler::default(),
            smoothed: [Smoothed::default(); SMOOTHED],
            smoothing: smoothing_coefficient(rate as u32),
            fresh: true,
            cutoff: ACID_LOWEST_CUTOFF,
            countdown: 0,
            decay: 0.0,
            slide: 0.0,
            attack: one_pole(ATTACK_MS, rate),
            level_decay: falling(LEVEL_DECAY_MS, rate),
            release: falling(RELEASE_MS, rate),
            accent_release: falling(ACCENT_RELEASE_MS, rate),
            softening: one_pole(SOFTENING_MS, rate),
            charge: one_pole(SWEEP_CHARGE_MS, rate),
            drain: one_pole(SWEEP_DRAIN_MS, rate),
            declick: 1.0 - (-std::f32::consts::TAU * DECLICK_HZ / rate).exp(),
            high_pass_coefficient: 1.0 - (-std::f32::consts::TAU * HIGH_PASS_HZ / rate).exp(),
        }
    }

    /// Start a note, from wherever the last one left the filter and the accent sweep.
    pub fn trigger(&mut self, accent: bool) {
        self.gate = true;
        self.accented = accent;
        self.envelope = 1.0;
        self.attacking = true;
        self.sliding = false;
        self.jump = true;
        self.countdown = 0;
    }

    /// Glide to the next note with the gate held, so neither envelope starts again. The note
    /// slid to brings its own accent, which sets the filter envelope's decay from here.
    pub fn slide(&mut self, accent: bool) {
        self.accented = accent;
        self.sliding = true;
        self.countdown = 0;
    }

    /// Let go of the note.
    pub fn release(&mut self) {
        self.gate = false;
    }

    /// The cutoff it played the latest frame at, in Hz.
    pub fn cutoff(&self) -> f32 {
        self.cutoff
    }

    /// The frequency it played the latest frame at, as a fraction of the rate.
    pub fn heard(&self) -> f32 {
        self.heard
    }

    /// The filter envelope's level.
    pub fn envelope(&self) -> f32 {
        self.envelope
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        for (place, smoothed) in self.smoothed.iter_mut().enumerate() {
            smoothed.target = settings.at(place + 1).clamp(0.0, 1.0);
        }
        if self.fresh {
            self.smoothed.iter_mut().for_each(Smoothed::settle);
            self.fresh = false;
        }
        let mut values = [0.0; SMOOTHED];
        for (value, smoothed) in values.iter_mut().zip(&mut self.smoothed) {
            *value = smoothed.next(self.smoothing);
        }
        let square = (settings.at(0) * 255.0).round() >= 1.0;
        let resonance = values[RESONANCE];
        if self.countdown == 0 {
            let decay = if self.accented {
                ACCENT_DECAY_MS
            } else {
                acid_decay_ms(values[DECAY] * 255.0)
            };
            self.decay = falling(decay, self.rate);
            self.slide = one_pole(slide_ms(values[SLIDE] * 255.0) * SLIDE_SHARE, self.rate);
            self.countdown = CONTROL_FRAMES;
        }
        self.countdown -= 1;

        // A new note takes its pitch at once, and a slide glides there.
        let target = frequency.clamp(1e-6, 0.45);
        if self.jump || !self.sliding {
            self.heard = target;
            self.jump = false;
        } else {
            self.heard *= (target / self.heard).powf(self.slide);
        }

        // The filter envelope, and the accent's pulse from it into the sweep's capacitor, which
        // charges through a diode.
        self.envelope *= self.decay;
        self.softened += (self.envelope - self.softened) * self.softening;
        let pulse = if self.accented && self.gate {
            values[ACCENT] * self.envelope
        } else {
            0.0
        };
        self.sweep += (pulse - self.sweep).max(0.0) * self.charge - self.sweep * self.drain;
        // Resonance turns the same knob as the sweep's: low, the cutoff follows the pulse, and
        // high, the capacitor's slower swell.
        let accent = pulse + (self.sweep * SWEEP_GAIN - pulse) * resonance;

        // The level envelope, with the filter envelope and accent added while the note's held.
        if self.gate {
            if self.attacking {
                self.level += (1.0 - self.level) * self.attack;
                self.attacking = self.level < 0.999;
            } else {
                self.level *= self.level_decay;
            }
            self.open += (1.0 - self.open) * self.attack;
        } else {
            let release = if self.accented {
                self.accent_release
            } else {
                self.release
            };
            self.level *= release;
            self.open *= release;
        }
        let amplitude = (self.level
            + self.open * (ENVELOPE_TO_LEVEL * self.softened + ACCENT_TO_LEVEL * accent))
            / (1.0 + ENVELOPE_TO_LEVEL);
        self.amplitude += (amplitude - self.amplitude) * self.declick;

        // The cutoff: the envelope moves it by an amount and around a centre that Open303
        // measured on a 303, which depend on both knobs, and the accent adds to it.
        let knob = values[CUTOFF];
        let amount = values[ENVELOPE_MOD];
        let low = 3.774 * amount + 0.737;
        let high = 4.195 * amount + 0.864;
        let scale = (1.0 - knob) * low + knob * high;
        let centre = 0.0483 * knob + 0.2944;
        let octaves = scale * (self.envelope - centre) + ACCENT_OCTAVES * accent;
        // `acid_cutoff_hz`, in octaves, so one power of two does both.
        let knob_octaves = knob * (ACID_HIGHEST_CUTOFF / ACID_LOWEST_CUTOFF).log2();
        self.cutoff =
            (ACID_LOWEST_CUTOFF * (knob_octaves + octaves).exp2()).clamp(10.0, HIGHEST_HZ);

        // The oscillator, high-passed as a 303's is on its way to the filter.
        let increment = self.heard;
        self.phase += increment;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        let wave = if square {
            let high = if self.phase < SQUARE_DUTY { 1.0 } else { -1.0 };
            let fall = (self.phase - SQUARE_DUTY + 1.0).fract();
            high + blep(self.phase, increment) - blep(fall, increment)
        } else {
            2.0 * self.phase - 1.0 - blep(self.phase, increment)
        };
        self.high_pass += (wave - self.high_pass) * self.high_pass_coefficient;
        let input = wave - self.high_pass;

        // `acid_drive_db` as a gain.
        let drive = (acid_drive_db(values[DRIVE] * 255.0) / DECIBELS_PER_OCTAVE).exp2();
        // The filter resonates under the cutoff it's given, and Open303's are where a 303 resonates.
        let cutoff = self.cutoff / RESONANT_SHARE / (self.rate * 2.0);
        let filter = &mut self.filter;
        let filtered = self.oversampler.process(input, |sample| {
            filter.next(sample, cutoff, resonance, drive)
        });
        soft_clip(filtered * self.amplitude * LEVEL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Settings in their places: wave, cutoff, resonance, env mod, decay, accent, slide, drive.
    fn places(values: [f32; 8]) -> Settings {
        Settings::from_places(values)
    }

    fn plain() -> Settings {
        places([0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.0, 0.0])
    }

    fn hertz(hertz: f32) -> f32 {
        hertz / RATE as f32
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    /// How much each frame jumps against how loud it is, a rough measure of brightness.
    fn brightness(samples: &[f32]) -> f32 {
        let jumps: f32 = samples
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).powi(2))
            .sum();
        jumps / samples.iter().map(|s| s * s).sum::<f32>()
    }

    fn note(accent: bool, settings: Settings, frames: usize) -> Vec<f32> {
        let mut acid = Acid::new(RATE);
        acid.trigger(accent);
        (0..frames)
            .map(|_| acid.next(hertz(110.0), settings))
            .collect()
    }

    #[test]
    fn it_plays_the_notes_pitch() {
        for (wave, note) in [(0.0, 55.0), (1.0 / 255.0, 110.0), (0.0, 220.0)] {
            let mut acid = Acid::new(RATE);
            acid.trigger(false);
            let settings = places([wave, 1.0, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0]);
            let out: Vec<f32> = (0..RATE as usize)
                .map(|_| acid.next(hertz(note), settings))
                .collect();
            // Count rising zero crossings over the second half second.
            let crossings = out[RATE as usize / 2..]
                .windows(2)
                .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
                .count() as f32;
            let measured = crossings * 2.0;
            assert!(
                (measured - note).abs() <= note * 0.03,
                "{measured} for {note} Hz"
            );
        }
    }

    #[test]
    fn it_sounds_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = crate::dsp::virtual_analog::VirtualAnalog::new();
        let va: Vec<f32> = (0..24_000)
            .map(|_| va.next(hertz(110.0), Settings::default()))
            .collect();
        let acid = note(false, plain(), 24_000);
        let decibels = 20.0 * (rms(&acid[..4_800]) / rms(&va)).log10();
        assert!(decibels.abs() < 6.0, "{decibels} dB");
    }

    #[test]
    fn an_accent_is_louder_and_brighter() {
        let plain_note = note(false, plain(), 4_800);
        let accented = note(true, plain(), 4_800);
        let louder = 20.0 * (rms(&accented) / rms(&plain_note)).log10();
        assert!(louder > 3.0, "{louder} dB");
        // The cutoff over the note's first 100 ms is higher.
        let mean_cutoff = |accent| {
            let mut acid = Acid::new(RATE);
            acid.trigger(accent);
            let total: f32 = (0..4_800)
                .map(|_| {
                    acid.next(hertz(110.0), plain());
                    acid.cutoff().log2()
                })
                .sum();
            (total / 4_800.0).exp2()
        };
        let (plain_cutoff, accented_cutoff) = (mean_cutoff(false), mean_cutoff(true));
        assert!(
            accented_cutoff > plain_cutoff * 1.1,
            "{plain_cutoff} {accented_cutoff}"
        );
        // Which is heard, on a darker sound.
        let dark = places([0.0, 0.2, 0.5, 0.2, 0.5, 0.8, 0.0, 0.0]);
        let (plain_note, accented) = (note(false, dark, 4_800), note(true, dark, 4_800));
        assert!(
            brightness(&accented) > brightness(&plain_note) * 1.25,
            "{} {}",
            brightness(&accented),
            brightness(&plain_note)
        );
    }

    #[test]
    fn an_accent_shortens_the_filter_envelope() {
        let long = places([0.0, 0.5, 0.5, 0.5, 1.0, 0.5, 0.0, 0.0]);
        let mut plain_note = Acid::new(RATE);
        let mut accented = Acid::new(RATE);
        plain_note.trigger(false);
        accented.trigger(true);
        for _ in 0..RATE {
            plain_note.next(hertz(110.0), long);
            accented.next(hertz(110.0), long);
        }
        assert!(accented.envelope() < plain_note.envelope() * 0.1);
    }

    /// The highest cutoff over each of `notes` accented notes, a sixteenth apart at 120 BPM.
    fn accent_peaks(notes: usize, settings: Settings) -> Vec<f32> {
        let mut acid = Acid::new(RATE);
        let sixteenth = RATE as usize / 8;
        (0..notes)
            .map(|_| {
                acid.trigger(true);
                (0..sixteenth)
                    .map(|_| {
                        acid.next(hertz(55.0), settings);
                        acid.cutoff()
                    })
                    .fold(0.0, f32::max)
            })
            .collect()
    }

    #[test]
    fn accents_in_a_row_build_up() {
        let resonant = places([0.0, 0.3, 1.0, 0.0, 0.5, 1.0, 0.0, 0.0]);
        let peaks = accent_peaks(4, resonant);
        assert!(peaks[1] > peaks[0] * 1.1, "{peaks:?}");
        assert!(peaks[3] > peaks[1], "{peaks:?}");
        // Low resonance follows the envelope closely, so there's little to build.
        let open = places([0.0, 0.3, 0.0, 0.0, 0.5, 1.0, 0.0, 0.0]);
        let peaks = accent_peaks(4, open);
        assert!(peaks[3] < peaks[0] * 1.1, "{peaks:?}");
    }

    #[test]
    fn a_slide_glides_the_pitch_and_keeps_the_envelopes_going() {
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        for _ in 0..RATE / 10 {
            acid.next(hertz(110.0), plain());
        }
        let before = acid.envelope();
        acid.slide(false);
        // Ten ms into a 60 ms slide, it's on its way.
        for _ in 0..RATE / 100 {
            acid.next(hertz(220.0), plain());
        }
        let partway = acid.heard() * RATE as f32;
        assert!(partway > 120.0 && partway < 200.0, "{partway} Hz");
        assert!(acid.envelope() < before, "the envelope didn't start again");
        for _ in 0..RATE / 10 {
            acid.next(hertz(220.0), plain());
        }
        let there = acid.heard() * RATE as f32;
        assert!((there - 220.0).abs() < 1.0, "{there} Hz");
        // A new note after it jumps.
        acid.trigger(false);
        acid.next(hertz(110.0), plain());
        assert!((acid.heard() * RATE as f32 - 110.0).abs() < 0.01);
        let fresh = falling(acid_decay_ms(127.5), RATE as f32);
        assert!(
            (acid.envelope() - fresh).abs() < 1e-4,
            "{}",
            acid.envelope()
        );
    }

    #[test]
    fn a_longer_slide_takes_longer() {
        let mut quick = Acid::new(RATE);
        let mut slow = Acid::new(RATE);
        let slow_settings = places([0.0, 0.5, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0]);
        for (acid, settings) in [(&mut quick, plain()), (&mut slow, slow_settings)] {
            acid.trigger(false);
            acid.next(hertz(110.0), settings);
            acid.slide(false);
            for _ in 0..RATE / 20 {
                acid.next(hertz(220.0), settings);
            }
        }
        assert!(slow.heard() < quick.heard() - 20.0 / RATE as f32);
    }

    #[test]
    fn moving_settings_in_steps_glides_without_a_step() {
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        let mut last_cutoff = 0.0;
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as f32;
            let jumpy = places([
                0.0,
                0.2 + step * 0.6,
                0.2 + step * 0.6,
                0.2 + step * 0.6,
                0.2 + step * 0.6,
                0.2 + step * 0.6,
                step,
                step,
            ]);
            let out = acid.next(hertz(110.0), jumpy);
            assert!(out.is_finite());
            let cutoff = acid.cutoff();
            if frame > 0 {
                // A few cents a frame at most, where an unsmoothed knob would jump octaves.
                let moved = (cutoff / last_cutoff).log2().abs();
                assert!(moved < 0.01, "{frame}: {last_cutoff} to {cutoff}");
            }
            last_cutoff = cutoff;
        }
        // Every value moves only a little each frame.
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        acid.next(hertz(110.0), places([0.0; 8]));
        let before = acid.smoothed.map(|smoothed| smoothed.value);
        acid.next(hertz(110.0), places([1.0; 8]));
        for (smoothed, before) in acid.smoothed.iter().zip(before) {
            assert!(smoothed.value - before < 0.01, "{}", smoothed.value);
        }
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for rate in [44_100, 192_000] {
            for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
                for corner in 0..64 {
                    let value = |bit: u32| if corner >> bit & 1 == 1 { 1.0 } else { 0.0 };
                    let settings = places([
                        value(0) / 255.0,
                        value(1),
                        value(2),
                        value(3),
                        value(4),
                        value(5),
                        0.0,
                        1.0,
                    ]);
                    let mut acid = Acid::new(rate);
                    for frame in 0..rate as usize / 10 {
                        // Accented notes close together, with a slide between two.
                        if frame % (rate as usize / 40) == 0 {
                            if frame % (rate as usize / 20) == 0 {
                                acid.trigger(true);
                            } else {
                                acid.slide(true);
                            }
                        }
                        let sample = acid.next(hertz / rate as f32, settings);
                        assert!(
                            sample.is_finite() && sample.abs() <= 1.0,
                            "{sample} at {hertz} {settings:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn it_falls_silent_once_released() {
        let mut acid = Acid::new(RATE);
        acid.trigger(true);
        for _ in 0..RATE / 10 {
            acid.next(hertz(110.0), plain());
        }
        acid.release();
        for _ in 0..RATE / 2 {
            acid.next(hertz(110.0), plain());
        }
        let tail = (0..100)
            .map(|_| acid.next(hertz(110.0), plain()).abs())
            .fold(0.0, f32::max);
        assert!(tail < 1e-4, "{tail}");
    }
}
