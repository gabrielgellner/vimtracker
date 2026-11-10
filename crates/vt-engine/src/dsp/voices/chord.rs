//! Four-note chords from a divide-down organ and string machine that crossfades into a wavetable,
//! from the chord model of Mutable Instruments' Plaits, with its main output. Ported to Rust from
//! `chord_engine.cc`, `string_synth_oscillator.h` and `wavetable_oscillator.h`. Its waves are
//! fifteen of the `wav_integrated_waves` the wavetable voice plays.
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

use super::chord_bank::{ChordBank, VOICES};
use super::virtual_analog::{BLOCK, Moving, Settings, next_blep, this_blep};
use super::wavetable::{PER_BANK, WAVE, stored};

/// The highest frequency a wavetable voice runs at, as a fraction of the sample rate.
const MAX_FREQUENCY: f32 = 0.25;
/// How loud it is against the other machines. Plaits gives this model and the virtual analog one
/// the same output gain, and it comes out about as loud: over a second at 220 Hz, a major chord
/// at its root on the organ's saws (Morph a quarter) measures 1 dB louder than the virtual analog
/// voice's defaults. The organ's plain square, just below the middle, is about 6 dB louder and
/// the wavetable end up to 2.5 dB quieter. So it's left as it comes.
const LEVEL: f32 = 1.0;

/// Where Morph fades each voice from the organ into the wavetable, staggered so they don't all
/// change at once.
const FADE_POINT: [f32; VOICES] = [0.55, 0.47, 0.49, 0.51, 0.53];

/// The organ's registrations, as levels of a saw and a square at each of three octaves.
const REGISTRATIONS: [[f32; 6]; 8] = [
    [0.0, 1.0, 0.0, 0.0, 0.0, 0.0],    // Square
    [1.0, 0.0, 0.0, 0.0, 0.0, 0.0],    // Saw
    [0.5, 0.0, 0.5, 0.0, 0.0, 0.0],    // Saw and saw
    [0.33, 0.0, 0.33, 0.0, 0.33, 0.0], // Full saw
    [0.33, 0.0, 0.0, 0.33, 0.0, 0.33], // Full saw and square
    [0.5, 0.0, 0.0, 0.0, 0.0, 0.5],    // Saw and a high square
    [0.0, 0.5, 0.0, 0.0, 0.0, 0.5],    // Square and a high square
    [0.0, 0.1, 0.1, 0.0, 0.2, 0.6],    // Saw, square and high harmonics
];

const fn wave(bank: usize, row: usize, column: usize) -> usize {
    bank * PER_BANK + row * 8 + column
}

/// The waves Morph scans through past the organ.
const WAVES: [usize; 15] = [
    wave(2, 6, 1),
    wave(2, 6, 6),
    wave(2, 6, 4),
    wave(0, 6, 0),
    wave(0, 6, 1),
    wave(0, 6, 2),
    wave(0, 6, 7),
    wave(2, 4, 7),
    wave(2, 4, 6),
    wave(2, 4, 5),
    wave(2, 4, 4),
    wave(2, 4, 3),
    wave(2, 4, 2),
    wave(2, 4, 1),
    wave(2, 4, 0),
];

/// Levels across the registrations for a setting on `0..=1`, with a seventh, empty one for the
/// octave above the organ's highest.
fn registration(registration: f32) -> [f32; 7] {
    let registration = registration * (REGISTRATIONS.len() as f32 - 1.001);
    let integral = registration as usize;
    let fractional = registration - integral as f32;
    let mut amplitudes = [0.0; 7];
    for (i, amplitude) in amplitudes.iter_mut().take(6).enumerate() {
        let a = REGISTRATIONS[integral][i];
        let b = REGISTRATIONS[integral + 1][i];
        *amplitude = a + (b - a) * fractional;
    }
    amplitudes
}

/// A string machine's divide-down oscillator: one phase counting through eight segments makes
/// saws at four octaves, which add up to saws and squares at three.
#[derive(Debug, Clone, Copy)]
struct StringSynth {
    phase: f32,
    next_sample: f32,
    segment: i32,
    frequency: f32,
    gains: [f32; 4],
    fresh: bool,
}

impl StringSynth {
    fn new() -> Self {
        Self {
            phase: 0.0,
            next_sample: 0.0,
            segment: 0,
            frequency: 0.001,
            gains: [0.0; 4],
            fresh: true,
        }
    }

    fn render(&mut self, frequency: f32, unshifted: &[f32; 7], gain: f32, out: &mut [f32; BLOCK]) {
        let mut frequency = frequency * 8.0;
        // A very high note is played as the second or fourth harmonic of one an octave or two
        // down, by moving the registration up.
        let mut shift = 0;
        while frequency > 0.5 {
            shift += 2;
            frequency *= 0.5;
        }
        if shift >= 8 {
            return;
        }
        let mut r = [0.0; 7];
        r[shift..].copy_from_slice(&unshifted[..7 - shift]);
        if self.fresh {
            self.frequency = frequency;
            self.fresh = false;
        }
        let targets = [
            (r[0] + 2.0 * r[1]) * gain,
            (r[2] - r[1] + 2.0 * r[3]) * gain,
            (r[4] - r[3] + 2.0 * r[5]) * gain,
            (r[6] - r[5]) * gain,
        ];
        let mut fm = Moving::towards(self.frequency, frequency);
        let mut saw_8 = Moving::towards(self.gains[0], targets[0]);
        let mut saw_4 = Moving::towards(self.gains[1], targets[1]);
        let mut saw_2 = Moving::towards(self.gains[2], targets[2]);
        let mut saw_1 = Moving::towards(self.gains[3], targets[3]);

        let mut phase = self.phase;
        let mut next_sample = self.next_sample;
        let mut segment = self.segment;
        for out in out.iter_mut() {
            let mut this_sample = next_sample;
            next_sample = 0.0;
            let frequency = fm.next();
            let saw_8_gain = saw_8.next();
            let saw_4_gain = saw_4.next();
            let saw_2_gain = saw_2.next();
            let saw_1_gain = saw_1.next();

            phase += frequency;
            let mut next_segment = phase as i32;
            if next_segment != segment {
                let mut discontinuity = 0.0;
                if next_segment == 8 {
                    phase -= 8.0;
                    next_segment -= 8;
                    discontinuity -= saw_8_gain;
                }
                if next_segment & 3 == 0 {
                    discontinuity -= saw_4_gain;
                }
                if next_segment & 1 == 0 {
                    discontinuity -= saw_2_gain;
                }
                discontinuity -= saw_1_gain;
                if discontinuity != 0.0 {
                    let fraction = phase - next_segment as f32;
                    let t = fraction / frequency;
                    this_sample += this_blep(t) * discontinuity;
                    next_sample += next_blep(t) * discontinuity;
                }
            }
            segment = next_segment;

            next_sample += (phase - 4.0) * saw_8_gain * 0.125;
            next_sample += (phase - (segment & 4) as f32 - 2.0) * saw_4_gain * 0.25;
            next_sample += (phase - (segment & 6) as f32 - 1.0) * saw_2_gain * 0.5;
            next_sample += (phase - (segment & 7) as f32 - 0.5) * saw_1_gain;
            *out += 2.0 * this_sample;
        }
        self.phase = phase;
        self.next_sample = next_sample;
        self.segment = segment;
        self.frequency = frequency;
        self.gains = targets;
    }
}

/// A voice scanning the fifteen waves. They're stored integrated, and played back differentiated
/// and smoothed, which keeps high notes from aliasing.
#[derive(Debug, Clone, Copy)]
struct WaveVoice {
    phase: f32,
    frequency: f32,
    amplitude: f32,
    waveform: f32,
    lp: f32,
    slope: f32,
    last: f32,
    fresh: bool,
}

impl WaveVoice {
    fn new() -> Self {
        Self {
            phase: 0.0,
            frequency: 0.0,
            amplitude: 0.0,
            waveform: 0.0,
            lp: 0.0,
            slope: 0.0,
            last: 0.0,
            fresh: true,
        }
    }

    #[inline]
    fn read(wave: usize, index: usize, fraction: f32) -> f32 {
        let a = stored(WAVES[wave], index);
        let b = stored(WAVES[wave], index + 1);
        a + (b - a) * fraction
    }

    fn render(&mut self, frequency: f32, amplitude: f32, waveform: f32, out: &mut [f32; BLOCK]) {
        let frequency = frequency.clamp(0.000_000_1, MAX_FREQUENCY);
        // Quieter towards the top, and scaled so a differentiated wave comes out at about the
        // level it was stored at.
        let amplitude = amplitude * (1.0 - 2.0 * frequency) / (frequency * 131_072.0);
        let waveform = waveform * (WAVES.len() as f32 - 1.0001);
        if self.fresh {
            (self.frequency, self.waveform) = (frequency, waveform);
        }
        let mut fm = Moving::towards(self.frequency, frequency);
        let mut level = Moving::towards(self.amplitude, amplitude);
        let mut scan = Moving::towards(self.waveform, waveform);
        for out in out.iter_mut() {
            let f0 = fm.next();
            let cutoff = (WAVE as f32 * f0).min(1.0);
            self.phase += f0;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let waveform = scan.next();
            let wave = waveform as usize;
            let wave_fraction = waveform - wave as f32;
            let p = self.phase * WAVE as f32;
            let index = (p as usize).min(WAVE - 1);
            let fraction = p - index as f32;
            let x0 = Self::read(wave, index, fraction);
            let x1 = Self::read(wave + 1, index, fraction);
            let s = x0 + (x1 - x0) * wave_fraction;
            // Plaits' differentiator starts from nothing, a jump of a whole stored sample. A new
            // voice starts from its first sample instead, so it doesn't click.
            if self.fresh {
                self.last = s;
                self.fresh = false;
            }
            self.slope += cutoff * (s - self.last - self.slope);
            self.last = s;
            self.lp += cutoff * (self.slope - self.lp);
            *out += level.next() * self.lp;
        }
        (self.frequency, self.amplitude, self.waveform) = (frequency, amplitude, waveform);
    }
}

/// One voice of the model. It's plain data, so a track's state can hold one ahead of time.
#[derive(Debug, Clone, Copy)]
pub struct Chord {
    chords: ChordBank,
    organ: [StringSynth; VOICES],
    waves: [WaveVoice; VOICES],
    morph: f32,
    timbre: f32,
    output: [f32; BLOCK],
    at: usize,
    fresh: bool,
}

impl Default for Chord {
    fn default() -> Self {
        Self::new()
    }
}

impl Chord {
    pub fn new() -> Self {
        Self {
            chords: ChordBank::new(),
            organ: [StringSynth::new(); VOICES],
            waves: [WaveVoice::new(); VOICES],
            morph: 0.0,
            timbre: 0.0,
            output: [0.0; BLOCK],
            at: BLOCK,
            fresh: true,
        }
    }

    /// The next frame, at a frequency as a fraction of the sample rate.
    #[inline]
    pub fn next(&mut self, frequency: f32, settings: Settings) -> f32 {
        if self.at == BLOCK {
            self.render(frequency.clamp(0.0, 0.5), settings);
            self.at = 0;
        }
        let sample = self.output[self.at];
        self.at += 1;
        sample * LEVEL
    }

    fn render(&mut self, frequency: f32, settings: Settings) {
        let morph = settings.morph.clamp(0.0, 1.0);
        let timbre = settings.timbre.clamp(0.0, 1.0);
        if self.fresh {
            // A new note starts where the settings are, rather than sweeping up from nothing.
            (self.morph, self.timbre) = (morph, timbre);
            self.fresh = false;
        }
        self.morph += 0.1 * (morph - self.morph);
        self.timbre += 0.1 * (timbre - self.timbre);
        // Harmonics is a place in the list of chords.
        self.chords.set_listed(settings.harmonics.clamp(0.0, 1.0));

        // The lower half of Morph runs back through the registrations, the upper scans the waves.
        let harmonics = registration((1.0 - self.morph * 2.15).max(0.0));
        let (ratios, amplitudes) = self.chords.inversion(self.timbre);
        let f0 = frequency * 0.998;
        let waveform = ((self.morph - 0.535) * 2.15).max(0.0);

        self.output = [0.0; BLOCK];
        for note in 0..VOICES {
            let wavetable_amount = (50.0 * (self.morph - FADE_POINT[note])).clamp(0.0, 1.0);
            let note_f0 = f0 * ratios[note];
            // The organ fades out as a note nears an eighth of the rate, where its octaves alias.
            let organ_gain = (4.0 - note_f0 * 32.0).clamp(0.0, 1.0);
            let organ_amount = (1.0 - wavetable_amount) * organ_gain;
            if wavetable_amount != 0.0 {
                self.waves[note].render(
                    note_f0 * 1.004,
                    amplitudes[note] * wavetable_amount,
                    waveform,
                    &mut self.output,
                );
            }
            if organ_amount != 0.0 {
                self.organ[note].render(
                    note_f0,
                    &harmonics,
                    amplitudes[note] * organ_amount,
                    &mut self.output,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn run(hertz: f32, settings: Settings, frames: usize) -> Vec<f32> {
        let mut voice = Chord::new();
        (0..frames)
            .map(|_| voice.next(hertz / RATE, settings))
            .collect()
    }

    fn at(harmonics: f32, timbre: f32, morph: f32) -> Settings {
        Settings {
            harmonics,
            timbre,
            morph,
            ..Settings::default()
        }
    }

    /// A major chord at its root, played on the organ's saws.
    fn plain() -> Settings {
        at(1.0, 0.4, 0.25)
    }

    fn energy(samples: &[f32]) -> f32 {
        samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32
    }

    /// How strongly a frequency sounds, by correlating with a sine and cosine at it.
    fn strength(samples: &[f32], hertz: f32) -> f32 {
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        for (n, &s) in samples.iter().enumerate() {
            let phase = std::f64::consts::TAU * f64::from(hertz) * n as f64 / f64::from(RATE);
            re += f64::from(s) * phase.cos();
            im += f64::from(s) * phase.sin();
        }
        ((re * re + im * im).sqrt() / samples.len() as f64) as f32
    }

    #[test]
    fn it_sounds() {
        let out = run(220.0, plain(), 48_000);
        assert!(energy(&out[24_000..]) > 1e-3, "{}", energy(&out[24_000..]));
    }

    #[test]
    fn a_major_chord_sounds_its_third_and_fifth_as_well_as_its_root() {
        let out = run(110.0, plain(), 48_000);
        let out = &out[4_800..];
        let semitone = |n: f32| 110.0 * (n / 12.0).exp2();
        let root = strength(out, semitone(0.0));
        let third = strength(out, semitone(4.0));
        let fifth = strength(out, semitone(7.0));
        // Neither a minor third nor a fourth, which aren't in the chord.
        let between = strength(out, semitone(3.0)).max(strength(out, semitone(5.0)));
        for note in [root, third, fifth] {
            assert!(note > between * 10.0, "{root} {third} {fifth} {between}");
        }
    }

    #[test]
    fn each_setting_changes_the_sound() {
        let plain = run(220.0, plain(), 9_600);
        for other in [
            at(3.0 / 255.0, 0.4, 0.25),
            at(1.0, 0.7, 0.25),
            at(1.0, 0.4, 0.8),
        ] {
            let changed = run(220.0, other, 9_600);
            let difference: Vec<f32> = plain.iter().zip(&changed).map(|(a, b)| a - b).collect();
            assert!(
                energy(&difference) > energy(&plain) * 0.1,
                "{other:?} {}",
                energy(&difference)
            );
        }
    }

    #[test]
    fn a_new_note_starts_without_a_click() {
        let out = run(220.0, at(0.5, 0.5, 0.9), 480);
        let jump = out
            .windows(2)
            .fold(0.0_f32, |most, pair| most.max((pair[1] - pair[0]).abs()));
        assert!(jump < 0.5, "{jump}");
    }

    #[test]
    fn every_setting_at_any_note_stays_finite_and_in_range() {
        for hertz in [20.0, 55.0, 440.0, 3_000.0, 12_000.0, 40_000.0] {
            for harmonics in [0.0, 0.5, 1.0] {
                for timbre in [0.0, 0.3, 0.5, 0.8, 1.0] {
                    for morph in [0.0, 0.25, 0.5, 0.75, 1.0] {
                        let settings = at(harmonics, timbre, morph);
                        for sample in run(hertz, settings, 2_000) {
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
