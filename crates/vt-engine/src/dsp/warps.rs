//! Cross-modulation and a vocoder, from Mutable Instruments' Warps. Ported to Rust from
//! `warps/dsp/modulator.cc` and `modulator.h` (its algorithms, its saturating amplifier and how
//! the knob blends them), `vocoder.cc` and `vocoder.h` (its envelope followers and formant shift),
//! `filter_bank.cc` and `filter_bank.h`, `limiter.h`, `oscillator.cc` and `parameters.h`, with
//! the fold table from its generated `resources.cc`, taken into `tables/warps_bipolar_fold.bin`.
//! The slot's own sound is the carrier, Warps' first input, and the modulator, its second, is
//! another track or Warps' oscillator. It's mono, as Warps is.
//!
//! Warps runs at 96 kHz, with its non-linear algorithms six times over that and its filter bank's
//! lower bands at a twelfth and a third of it. Here everything runs at the program's rate in
//! float: the algorithms at twice it, through the halfband oversampler `Distort` uses, and every
//! band at the full rate, so no rate converters are needed. The bank's bands are worked out for
//! the rate as Warps' table was: a fourth-order low-pass at the bottom, a fourth-order high-pass
//! at the top, and third-octave band-passes between, each two state variable filters in a row.
//! Its per-sample constants are for 96 kHz, so each is changed to take as long at this rate.
//!
//! Copyright 2014 Emilie Gillet.
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

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, TAU};

use vt_model::insert::{
    ALGORITHM_FADE_START, ALGORITHM_MODULATOR, Warps as Settings, algorithm_knob, shift_semitones,
};

use crate::dsp::distort::saturating_gains;
use crate::dsp::drums::soft_clip;
use crate::dsp::oversampler::PairOversampler;
use crate::dsp::plaits_parts::{DcBlocker, Svf};
use crate::dsp::smoother::Smoothed;

/// Where `Algo` is at `FF`, out of 256.
const TOP_OF_ALGO: f32 = 255.0 / 256.0;
/// The rate Warps' per-sample constants are set for.
const WARPS_RATE: f32 = 96_000.0;
/// How many bands the vocoder splits each sound into, and where the lowest is: a third of an
/// octave below 110 Hz, so the top one is at 7040 Hz.
const BANDS: usize = 20;
const LOWEST_BAND_HZ: f32 = 87.307_06;
/// How narrow each band-pass in a band is, so two in a row are 3 dB down a sixth of an octave
/// either side, where the next band takes over.
const BAND_Q: f32 = 2.779;
/// How much each band's envelope is turned up, as Warps', so they add up to about the sound.
const FOLLOWER_GAIN: f32 = 4.472_136;
/// How long Warps' blocks are, over which the vocoder's band gains move and its peaks are taken.
const BLOCK_SECONDS: f32 = 60.0 / WARPS_RATE;
/// How long it keeps going once nothing comes in, fading out, so the vocoder lets go and the
/// oscillator doesn't stop dead.
const TAIL_SECONDS: f32 = 1.0;
/// Below this power, the modulator's gate starts closing, as Warps' does.
const GATE_FLOOR: f32 = 0.0001;
/// The oscillator's highest note, as Warps' table stops at 12 kHz.
const HIGHEST_HZ: f32 = 12_000.0;
/// Where the high-pass on the way out sits, in Hz. Warps' outputs are AC-coupled, and XOR and the
/// comparator leave an offset.
const DC_HZ: f32 = 5.0;

/// Warps' wavefolder, `lut_bipolar_fold`: 4097 little-endian floats, with no fold in the middle.
static FOLD: &[u8; FOLD_SIZE * 4] = include_bytes!("tables/warps_bipolar_fold.bin");
const FOLD_SIZE: usize = 4097;

/// One entry of the fold table.
#[inline]
fn fold_entry(index: usize) -> f32 {
    let at = index * 4;
    f32::from_le_bytes([FOLD[at], FOLD[at + 1], FOLD[at + 2], FOLD[at + 3]])
}

/// The fold table read between entries, at a place from `-1` to `1` with the ends held.
#[inline]
fn folded(place: f32) -> f32 {
    let index = (place * 2048.0 + 2048.0).clamp(0.0, (FOLD_SIZE - 1) as f32 - 0.001);
    let whole = index as usize;
    let a = fold_entry(whole);
    let b = fold_entry(whole + 1);
    a + (b - a) * (index - whole as f32)
}

/// A one-pole coefficient Warps uses at 96 kHz, changed to take as long at `rate`.
fn at_rate(coefficient: f32, rate: u32) -> f32 {
    1.0 - (1.0 - coefficient).powf(WARPS_RATE / rate as f32)
}

/// Julian Parker's diode, from "A simple digital model of the diode-based ring-modulator"
/// (DAFx-11), as Warps has it.
#[inline]
fn diode(x: f32) -> f32 {
    let dead_zone = x.abs() - 0.667;
    let dead_zone = dead_zone + dead_zone.abs();
    0.043_247_66 * dead_zone * dead_zone * x.signum()
}

/// A crossfade's gains for the modulator and the carrier, for a timbre, as Warps' table: equal
/// power, with a little held at either end.
#[inline]
fn crossfade(parameter: f32) -> (f32, f32) {
    let t = (1.04 * parameter - 0.02).clamp(0.0, 1.0) * FRAC_PI_2;
    (t.sin() * FRAC_1_SQRT_2, t.cos() * FRAC_1_SQRT_2)
}

/// One sample as a 16-bit integer, as Warps' converter gives it, for XOR.
#[inline]
fn to_short(x: f32) -> i16 {
    (x * 32768.0).clamp(-32768.0, 32767.0) as i16
}

/// One of Warps' cross-modulations of a modulator and a carrier, by its place among them, with its
/// parameter. The sixth is the modulator alone, which the comparator blends into.
#[inline]
fn cross_modulated(algorithm: usize, modulator: f32, carrier: f32, parameter: f32) -> f32 {
    match algorithm {
        0 => {
            let (fade_in, fade_out) = crossfade(parameter);
            modulator * fade_in + carrier * fade_out
        }
        1 => {
            let sum = (modulator + carrier + modulator * carrier * 0.25) * (0.02 + parameter);
            folded(sum / ((1.0 + 1.0 + 0.25) * 1.02))
        }
        2 => {
            let carrier = carrier * 2.0;
            let ring = diode(modulator + carrier) + diode(modulator - carrier);
            soft_clip(ring * (4.0 + parameter * 24.0))
        }
        3 => {
            let ring = 4.0 * modulator * carrier * (1.0 + parameter * 8.0);
            ring / (1.0 + ring.abs())
        }
        4 => {
            let bits = f32::from(to_short(modulator) ^ to_short(carrier)) / 32768.0;
            let sum = (modulator + carrier) * 0.7;
            sum + (bits - sum) * parameter
        }
        5 => {
            let place = parameter * 2.995;
            let whole = place as usize;
            let louder = modulator.abs() > carrier.abs();
            let direct = modulator.min(carrier);
            let threshold = if carrier > 0.05 { carrier } else { modulator };
            let window = if louder { modulator } else { carrier };
            let window_2 = if louder {
                modulator.abs()
            } else {
                -carrier.abs()
            };
            let sequence = [direct, threshold, window, window_2];
            let a = sequence[whole];
            let b = sequence[whole + 1];
            a + (b - a) * (place - whole as f32)
        }
        _ => modulator,
    }
}

/// Warps' timbre, bent towards its square as the algorithm knob turns, as the hardware does it:
/// Warps' `skewed_modulation_parameter` means to bend only some algorithms, but its knob reads
/// from `0` to `1`, so its tests all land on the first, and the bend follows the knob.
#[inline]
fn skewed(timbre: f32, algorithm: f32) -> f32 {
    timbre * (1.0 + algorithm * (timbre - 1.0))
}

/// Warps' saturating amplifier on the modulator: a gate for what's very quiet, then a drive that
/// soft-clips as hard as `limit` asks, which is none for the vocoder.
#[derive(Debug, Clone, Copy)]
struct Amplifier {
    level: f32,
    attack: f32,
    release: f32,
}

impl Amplifier {
    fn new(rate: u32) -> Self {
        Self {
            level: 0.0,
            attack: at_rate(0.1, rate),
            release: at_rate(0.0001, rate),
        }
    }

    #[inline]
    fn process(&mut self, input: f32, drive: f32, limit: f32) -> f32 {
        let error = input * input - self.level;
        self.level += error
            * if error > 0.0 {
                self.attack
            } else {
                self.release
            };
        let gated = if self.level <= GATE_FLOOR {
            input * self.level / GATE_FLOOR
        } else {
            input
        };
        let (pre, post) = saturating_gains(drive);
        let driven = gated * pre;
        driven + (soft_clip(driven) * post - driven) * limit
    }
}

/// Warps' oscillator, as the modulator when no track is: a sine, a triangle and a saw as for its
/// cross-modulation, a square, and filtered noise. The edges are smoothed with polyBLEPs.
#[derive(Debug, Clone, Copy)]
struct Oscillator {
    phase: f32,
    next_sample: f32,
    high: bool,
    smoothed: f32,
    /// How fast it goes, and the note that was worked out from.
    increment: f32,
    note: f32,
    noise: Svf,
    seed: u32,
    /// Warps' one-poles taking the edge off the saw, at this rate, and the rate itself.
    saw_smoothing: f32,
    rate: u32,
}

/// What a polyBLEP adds to the sample a step lands in, and to the one after.
#[inline]
fn this_blep(t: f32) -> f32 {
    0.5 * t * t
}

#[inline]
fn next_blep(t: f32) -> f32 {
    let t = 1.0 - t;
    -0.5 * t * t
}

impl Oscillator {
    fn new(rate: u32) -> Self {
        Self {
            phase: 0.0,
            next_sample: 0.0,
            high: false,
            smoothed: 0.0,
            increment: 0.0,
            note: f32::NAN,
            noise: Svf::default(),
            seed: 0x2545_f491,
            saw_smoothing: at_rate(0.3, rate),
            rate,
        }
    }

    /// Follow a MIDI note, working out the increment and the noise's filter only when it moves.
    fn tune(&mut self, note: f32) {
        if note == self.note {
            return;
        }
        self.note = note;
        let rate = self.rate as f32;
        let hertz = 440.0 * ((note - 69.0) / 12.0).exp2();
        let hertz = hertz.clamp(1.0, HIGHEST_HZ.min(rate * 0.45));
        self.increment = hertz / rate;
        self.noise.set_exact((self.increment * 4.0).min(0.45), 1.0);
    }

    /// The next sample of a waveform, by its place among `WAVE_NAMES`.
    #[inline]
    fn next(&mut self, wave: u8) -> f32 {
        let increment = self.increment;
        match wave {
            0 => {
                self.phase += increment;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                }
                (self.phase * TAU).sin()
            }
            1 | 3 => {
                // A square, smoothed at its edges, which the triangle integrates.
                let mut this_sample = self.next_sample;
                self.next_sample = 0.0;
                self.phase += increment;
                if !self.high && self.phase >= 0.5 {
                    let t = (self.phase - 0.5) / increment;
                    this_sample += this_blep(t);
                    self.next_sample += next_blep(t);
                    self.high = true;
                }
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                    let t = self.phase / increment;
                    this_sample -= this_blep(t);
                    self.next_sample -= next_blep(t);
                    self.high = false;
                }
                self.next_sample += if self.phase < 0.5 { 0.0 } else { 1.0 };
                if wave == 3 {
                    return 2.0 * this_sample - 1.0;
                }
                let square = 128.0 * (this_sample - 0.5);
                self.smoothed += increment * 0.0625 * (square - self.smoothed);
                self.smoothed
            }
            2 => {
                let mut this_sample = self.next_sample;
                self.next_sample = 0.0;
                self.phase += increment;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                    let t = self.phase / increment;
                    this_sample -= this_blep(t);
                    self.next_sample -= next_blep(t);
                }
                self.next_sample += self.phase;
                let saw = this_sample * 2.0 - 1.0;
                self.smoothed += self.saw_smoothing * (saw - self.smoothed);
                self.smoothed
            }
            _ => {
                // A xorshift, so it's the same every time.
                self.seed ^= self.seed << 13;
                self.seed ^= self.seed >> 17;
                self.seed ^= self.seed << 5;
                let white = self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0;
                self.noise.low_pass(white)
            }
        }
    }
}

/// A band of the vocoder's filter bank: what it lets through, and its two filters for one sound.
#[derive(Debug, Clone, Copy)]
enum Pass {
    Low,
    Band,
    High,
}

/// A band's filters for one sound, two in a row.
#[derive(Debug, Clone, Copy)]
struct BandFilters {
    filters: [Svf; 2],
}

impl BandFilters {
    fn new(pass: Pass, hertz: f32, rate: u32) -> Self {
        let f = hertz / rate as f32;
        let q = match pass {
            Pass::Band => BAND_Q,
            Pass::Low | Pass::High => FRAC_1_SQRT_2,
        };
        let mut filter = Svf::default();
        filter.set_exact(f, q);
        Self {
            filters: [filter; 2],
        }
    }

    #[inline]
    fn process(&mut self, pass: Pass, input: f32) -> f32 {
        let mut x = input;
        for filter in &mut self.filters {
            let (high, band, low) = filter.process_all(x);
            x = match pass {
                Pass::Low => low,
                // Turned up so its peak is unity.
                Pass::Band => band / BAND_Q,
                Pass::High => high,
            };
        }
        x
    }
}

/// Warps' vocoder: the modulator and the carrier each split into bands, each carrier band turned
/// up and down by an envelope follower on the matching modulator band, and the bands summed back
/// under a limiter. Away from the middle of `Timbre`, the carrier's bands take their gains from
/// the modulator's bands above or below instead, held a block at a time, shifting the formants.
#[derive(Debug, Clone, Copy)]
struct Vocoder {
    passes: [Pass; BANDS],
    carrier: [BandFilters; BANDS],
    modulator: [BandFilters; BANDS],
    /// Each band's follower: where it is, its highest in the block so far, and its peak, which
    /// follows the blocks' highest.
    envelopes: [f32; BANDS],
    block_highest: [f32; BANDS],
    peaks: [f32; BANDS],
    /// How fast each band's follower rises and falls, a sample at a time, for the release.
    attacks: [f32; BANDS],
    decays: [f32; BANDS],
    /// How much of each carrier band passes on its own, and how much as its follower moves it,
    /// and how far each moves a frame to get where this block wants it.
    carrier_gains: [f32; BANDS],
    vocoder_gains: [f32; BANDS],
    carrier_steps: [f32; BANDS],
    vocoder_steps: [f32; BANDS],
    /// Frames until the next block, and how long one is.
    countdown: u32,
    block: u32,
    /// The limiter's peak, and how fast it rises and falls.
    limiter_peak: f32,
    limiter_attack: f32,
    limiter_release: f32,
    rate: u32,
}

impl Vocoder {
    fn new(rate: u32) -> Self {
        let mut passes = [Pass::Band; BANDS];
        passes[0] = Pass::Low;
        passes[BANDS - 1] = Pass::High;
        let carrier: [BandFilters; BANDS] = std::array::from_fn(|band| {
            let hertz = LOWEST_BAND_HZ * (band as f32 / 3.0).exp2();
            BandFilters::new(passes[band], hertz, rate)
        });
        Self {
            passes,
            carrier,
            modulator: carrier,
            envelopes: [0.0; BANDS],
            block_highest: [0.0; BANDS],
            peaks: [0.0; BANDS],
            attacks: [0.0; BANDS],
            decays: [0.0; BANDS],
            carrier_gains: [0.0; BANDS],
            vocoder_gains: [0.0; BANDS],
            carrier_steps: [0.0; BANDS],
            vocoder_steps: [0.0; BANDS],
            countdown: 0,
            block: (BLOCK_SECONDS * rate as f32).round().max(1.0) as u32,
            limiter_peak: 0.5,
            limiter_attack: at_rate(0.05, rate),
            limiter_release: at_rate(0.000_02, rate),
            rate,
        }
    }

    /// Start a block: take the last one's peaks, and work out where the followers and the bands'
    /// gains go, for a release from `0` to `1` and a formant shift with none at a half.
    fn start_block(&mut self, release: f32, formant_shift: f32) {
        for (peak, highest) in self.peaks.iter_mut().zip(&mut self.block_highest) {
            let error = *highest - *peak;
            *peak += if error > 0.0 { 0.5 } else { 0.1 } * error;
            *highest = 0.0;
        }
        let frozen = release > 0.995;
        let mut f = 80.0 * (-6.0 * release).exp2();
        for band in 0..BANDS {
            let decay = (f / self.rate as f32).min(0.5);
            (self.attacks[band], self.decays[band]) = if frozen {
                (0.0, 0.0)
            } else {
                (decay * 2.0, decay * 0.5)
            };
            // A third of an octave up, for the next band.
            f *= 1.2599;
        }
        let mut amount = 2.0 * (formant_shift - 0.5).abs();
        amount *= 2.0 - amount;
        amount *= 2.0 - amount;
        let increment = 4.0 * (-4.0 * formant_shift).exp2();
        let last = BANDS as f32 - 1.0001;
        let mut source = 0.0_f32;
        let step = 1.0 / self.block as f32;
        for band in 0..BANDS {
            let place = source.clamp(0.0, last);
            let whole = place as usize;
            let a = self.peaks[whole];
            let b = self.peaks[whole + 1];
            let mut gain = a + (b - a) * (place - whole as f32);
            let past = source - last;
            if past >= 0.0 {
                gain /= 1.0 + past;
            }
            source += increment;
            self.carrier_steps[band] = (gain * amount - self.carrier_gains[band]) * step;
            self.vocoder_steps[band] = (1.0 - amount - self.vocoder_gains[band]) * step;
        }
        self.countdown = self.block;
    }

    #[inline]
    fn process(&mut self, modulator: f32, carrier: f32, release: f32, formant_shift: f32) -> f32 {
        if self.countdown == 0 {
            self.start_block(release, formant_shift);
        }
        self.countdown -= 1;
        let mut out = 0.0;
        for band in 0..BANDS {
            let pass = self.passes[band];
            let heard = self.modulator[band].process(pass, modulator);
            let error = (heard * FOLLOWER_GAIN).abs() - self.envelopes[band];
            self.envelopes[band] += error
                * if error > 0.0 {
                    self.attacks[band]
                } else {
                    self.decays[band]
                };
            self.block_highest[band] = self.block_highest[band].max(self.envelopes[band]);
            let shaped = self.carrier[band].process(pass, carrier);
            self.carrier_gains[band] += self.carrier_steps[band];
            self.vocoder_gains[band] += self.vocoder_steps[band];
            out += shaped
                * (self.carrier_gains[band] + self.vocoder_gains[band] * self.envelopes[band]);
        }
        // Warps' limiter, with its soft limit held at full scale.
        let driven = out * 1.4;
        let error = driven.abs() - self.limiter_peak;
        self.limiter_peak += error
            * if error > 0.0 {
                self.limiter_attack
            } else {
                self.limiter_release
            };
        let gain = if self.limiter_peak <= 1.0 {
            1.0
        } else {
            1.0 / self.limiter_peak
        };
        soft_clip(driven * gain * 0.8)
    }
}

/// What Warps keeps, from the effect pool, as its filter bank is a couple of kilobytes.
#[derive(Debug, Clone)]
pub struct Warps {
    algorithm: Smoothed,
    timbre: Smoothed,
    level: Smoothed,
    /// The oscillator's waveform, and how far it is from the note in semitones.
    wave: u8,
    shift: f32,
    /// The settings last followed, so the first jumps to them.
    settings: Option<Settings>,
    oversampler: PairOversampler,
    amplifier: Amplifier,
    oscillator: Oscillator,
    vocoder: Vocoder,
    dc_blocker: DcBlocker,
    rate: u32,
}

impl Warps {
    pub fn new(rate: u32) -> Self {
        Self {
            algorithm: Smoothed::default(),
            timbre: Smoothed::default(),
            level: Smoothed::default(),
            wave: 0,
            shift: 0.0,
            settings: None,
            oversampler: PairOversampler::default(),
            amplifier: Amplifier::new(rate),
            oscillator: Oscillator::new(rate),
            vocoder: Vocoder::new(rate),
            dc_blocker: DcBlocker::new(1.0 - TAU * DC_HZ / rate as f32),
            rate,
        }
    }

    /// Forget everything it was playing, for a fresh start.
    pub fn clear(&mut self) {
        let settings = self.settings;
        *self = Self::new(self.rate);
        if let Some(settings) = settings {
            self.follow(settings);
            self.settle();
        }
    }

    /// How many quiet frames in a row it keeps going for, fading out.
    pub fn quiet_frames(&self) -> u32 {
        (TAIL_SECONDS * self.rate as f32) as u32
    }

    /// Take what's set now, jumping to it the first time.
    pub fn follow(&mut self, settings: Settings) {
        if self.settings == Some(settings) {
            return;
        }
        self.algorithm.target = algorithm_knob(f32::from(settings.algorithm));
        self.timbre.target = f32::from(settings.timbre) / 255.0;
        self.level.target = f32::from(settings.level) / 255.0;
        self.wave = settings.shape;
        self.shift = shift_semitones(f32::from(settings.pitch));
        if self.settings.is_none() {
            self.settle();
        }
        self.settings = Some(settings);
    }

    /// Jump to what's set.
    pub fn settle(&mut self) {
        self.algorithm.settle();
        self.timbre.settle();
        self.level.settle();
    }

    /// One frame in with no other track, and the frame out, for the pool's own use.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        [self.process(input, None, 60.0, 1.0); 2]
    }

    /// One frame of the carrier, and of the modulator if it's a track rather than the
    /// oscillator, which plays `note` and its `Pitch` above it. What comes out is all wet, mono,
    /// and held to full scale, as Warps' converter holds it.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        modulator: Option<[f32; 2]>,
        note: f32,
        smoothing: f32,
    ) -> f32 {
        let algorithm = self.algorithm.next(smoothing);
        let timbre = self.timbre.next(smoothing);
        let drive = self.level.next(smoothing);
        let carrier = (input[0] + input[1]) * 0.5;
        let raw = match modulator {
            Some(frame) => (frame[0] + frame[1]) * 0.5,
            None => {
                self.oscillator.tune(note + self.shift);
                self.oscillator.next(self.wave)
            }
        };
        // `0` is all cross-modulation, `1` all vocoder, and a half the bare modulator between.
        let vocoder_amount = ((algorithm - ALGORITHM_MODULATOR) * 20.0 + 0.5).clamp(0.0, 1.0);
        let modulator = self.amplifier.process(raw, drive, 1.0 - vocoder_amount);
        let main = if vocoder_amount < 0.5 {
            let place = (algorithm * 8.0).min(5.999);
            let from = place as usize;
            let blend = place - from as f32;
            let parameter = skewed(timbre, algorithm);
            self.oversampler
                .process([modulator, carrier], |modulator, carrier| {
                    let a = cross_modulated(from, modulator, carrier, parameter);
                    let b = cross_modulated(from + 1, modulator, carrier, parameter);
                    a + (b - a) * blend
                })
        } else {
            0.0
        };
        // The vocoder starts as the fade towards it does, so its bands are full by the time it's
        // heard.
        let main = if algorithm > ALGORITHM_FADE_START {
            // Warps' release, from the fastest a quarter from the top to held at the top, which
            // here is `FF`.
            let release = ((algorithm - 0.75) / (TOP_OF_ALGO - 0.75)).clamp(0.0, 1.0);
            let vocoded =
                self.vocoder
                    .process(modulator, carrier, release * (2.0 - release), timbre);
            if vocoder_amount < 0.5 { main } else { vocoded }
        } else {
            main
        };
        let towards_modulator = 2.0 * vocoder_amount.min(1.0 - vocoder_amount);
        let main = main + towards_modulator * (modulator - main);
        self.dc_blocker.process(main).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::insert::MOST_SHIFT;

    const RATE: u32 = 48_000;
    const NO_SMOOTHING: f32 = 1.0;

    fn settings(algorithm: u8, timbre: u8) -> Settings {
        Settings {
            algorithm,
            timbre,
            ..Settings::DEFAULT
        }
    }

    fn sine(hertz: f32, level: f32, frame: usize, rate: u32) -> f32 {
        (frame as f32 / rate as f32 * hertz * TAU).sin() * level
    }

    /// A second of a carrier and a modulator through it, after a tenth to settle.
    fn played(
        settings: Settings,
        rate: u32,
        carrier: impl Fn(usize) -> f32,
        modulator: impl Fn(usize) -> Option<f32>,
    ) -> Vec<f32> {
        let mut warps = Warps::new(rate);
        warps.follow(settings);
        let frames = rate as usize;
        (0..frames + frames / 10)
            .map(|frame| {
                let key = modulator(frame).map(|sample| [sample; 2]);
                warps.process([carrier(frame); 2], key, 60.0, NO_SMOOTHING)
            })
            .skip(frames / 10)
            .collect()
    }

    /// How strong a frequency is in a whole number of cycles of it.
    fn strength(out: &[f32], hertz: f32, rate: u32) -> f32 {
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for (frame, sample) in out.iter().enumerate() {
            let phase = f64::from(frame as u32) / f64::from(rate)
                * f64::from(hertz)
                * std::f64::consts::TAU;
            real += f64::from(*sample) * phase.cos();
            imaginary += f64::from(*sample) * phase.sin();
        }
        ((real * real + imaginary * imaginary).sqrt() * 2.0 / out.len() as f64) as f32
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |most: f32, s| most.max(s.abs()))
    }

    #[test]
    fn ring_modulating_two_sines_makes_their_sum_and_difference() {
        let (carrier, modulator) = (300.0, 440.0);
        let ring = |algorithm| {
            played(
                settings(algorithm, 0x00),
                RATE,
                |frame| sine(carrier, 0.8, frame, RATE),
                |frame| Some(sine(modulator, 0.8, frame, RATE)),
            )
        };
        // The multiplier at `60` gives the sum and difference, and neither tone on its own.
        let out = ring(0x60);
        let sum = strength(&out, carrier + modulator, RATE);
        let difference = strength(&out, modulator - carrier, RATE);
        assert!(sum > 0.1 && difference > 0.1, "{sum} {difference}");
        for alone in [carrier, modulator] {
            let alone = strength(&out, alone, RATE);
            assert!(
                alone < sum.min(difference) * 0.01,
                "{alone} {sum} {difference}"
            );
        }
        // Warps' diodes at `40` are even in the carrier, so the modulator goes up and down by
        // twice it, and comes through on its own, but the carrier doesn't.
        let out = ring(0x40);
        let up = strength(&out, modulator + 2.0 * carrier, RATE);
        let down = strength(&out, 2.0 * carrier - modulator, RATE);
        let carried = strength(&out, carrier, RATE);
        assert!(up > 0.02 && down > 0.02, "{up} {down}");
        assert!(carried < up.min(down) * 0.01, "{carried} {up} {down}");
    }

    #[test]
    fn with_a_silent_modulator_the_ring_mods_and_vocoder_are_silent_and_the_rest_pass_the_carrier()
    {
        let carrier = |frame| sine(300.0, 0.5, frame, RATE);
        let silent = |_| Some(0.0);
        let level = |algorithm, timbre| {
            let out = played(settings(algorithm, timbre), RATE, carrier, silent);
            strength(&out, 300.0, RATE)
        };
        // Both ring modulations multiply by nothing, and so does the vocoder, whose bands follow
        // the modulator. Near `B3`, it's nearly all the bare modulator, which is nothing too.
        for algorithm in [0x40, 0x60, 0xD0, 0xFF] {
            let out = played(settings(algorithm, 0x80), RATE, carrier, silent);
            assert!(peak(&out) < 1e-3, "{algorithm:x}: {}", peak(&out));
        }
        // The crossfade gives the carrier at 3 dB down at `00`, and nothing at `FF`.
        assert!((level(0x00, 0x00) - 0.5 * FRAC_1_SQRT_2).abs() < 0.01);
        assert!(level(0x00, 0xFF) < 0.01);
        // The fold folds the carrier, gently at `00` and harder further up.
        assert!(level(0x20, 0x00) > 0.001, "{}", level(0x20, 0x00));
        // XOR with nothing is the carrier, from 0.7 of it at `00` to all of it at `FF`.
        assert!(
            (level(0x80, 0x00) - 0.35).abs() < 0.01,
            "{}",
            level(0x80, 0x00)
        );
        assert!(
            (level(0x80, 0xFF) - 0.5).abs() < 0.01,
            "{}",
            level(0x80, 0xFF)
        );
        // The comparator keeps what of the carrier is below nothing, at `00`.
        assert!(level(0xA0, 0x00) > 0.1, "{}", level(0xA0, 0x00));
    }

    /// A carrier with two tones, and how much of each comes through the vocoder for a modulator
    /// at one of them.
    fn vocoded(modulator: f32) -> (f32, f32) {
        let carrier = |frame| sine(300.0, 0.3, frame, RATE) + sine(2_000.0, 0.3, frame, RATE);
        let out = played(settings(0xE0, 0x80), RATE, carrier, |frame| {
            Some(sine(modulator, 0.5, frame, RATE))
        });
        (strength(&out, 300.0, RATE), strength(&out, 2_000.0, RATE))
    }

    #[test]
    fn the_vocoder_passes_the_carrier_shaped_by_the_modulators_spectrum() {
        let (low, high) = vocoded(300.0);
        assert!(low > 0.05 && low > high * 10.0, "{low} {high}");
        let (low, high) = vocoded(2_000.0);
        assert!(high > 0.05 && high > low * 10.0, "{low} {high}");
    }

    #[test]
    fn sweeping_algo_timbre_and_level_glides_without_a_step() {
        let mut warps = Warps::new(RATE);
        let smoothing = crate::dsp::smoother::coefficient(RATE);
        warps.follow(settings(0x00, 0x00));
        let mut last = 0.0;
        let frames = RATE as usize * 4;
        for frame in 0..frames {
            // A jump every 64 frames, as a knob or an LFO steps it block by block, over the whole
            // of `Algo`, with `Timbre` and `Level` jumping back and forth.
            let block = frame / 64;
            let algorithm = (block * 256 / (frames / 64)) as u8;
            let flip = (block % 2) as u8;
            warps.follow(Settings {
                timbre: flip * 0x20,
                level: 0x80 + flip * 0x30,
                ..settings(algorithm, 0)
            });
            let carrier = sine(50.0, 0.5, frame, RATE);
            let modulator = sine(70.0, 0.5, frame, RATE);
            let out = warps.process([carrier; 2], Some([modulator; 2]), 60.0, smoothing);
            assert!(
                (out - last).abs() < 0.05,
                "{frame} {algorithm:x}: {last} to {out}"
            );
            last = out;
        }
    }

    #[test]
    fn it_stays_finite_and_within_full_scale_at_every_extreme() {
        for rate in [44_100, 192_000] {
            for algorithm in [0x00, 0x20, 0x40, 0x60, 0x80, 0xA0, 0xB3, 0xD0, 0xFF] {
                for timbre in [0x00, 0x80, 0xFF] {
                    for level in [0x00, 0xFF] {
                        for (source, shape) in [(1, 0), (0, 0), (0, 1), (0, 2), (0, 4)] {
                            let mut warps = Warps::new(rate);
                            warps.follow(Settings {
                                algorithm,
                                timbre,
                                level,
                                source,
                                shape,
                                pitch: if shape == 0 { 0 } else { MOST_SHIFT * 2 },
                                ..Settings::DEFAULT
                            });
                            for frame in 0..rate as usize / 20 {
                                // A loud square, and a loud saw as the modulator.
                                let carrier = if frame / 50 % 2 == 0 { 4.0 } else { -4.0 };
                                let key = (source > 0).then_some([(frame % 70) as f32 / 10.0; 2]);
                                let note = if frame % 2 == 0 { 0.0 } else { 127.0 };
                                let out = warps.process([carrier; 2], key, note, NO_SMOOTHING);
                                assert!(
                                    out.is_finite() && out.abs() <= 1.0,
                                    "{rate} {algorithm:x} {timbre:x} {level:x} {shape}: {out}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_oscillator_plays_its_note_and_pitch_above_it() {
        // At `Level` `00` it's silent, so crossfade it in and listen to it alone.
        let out = played(
            Settings {
                pitch: MOST_SHIFT + 12,
                ..settings(0x00, 0xFF)
            },
            RATE,
            |_| 0.0,
            |_| None,
        );
        // Middle C an octave up.
        let at = 523.251_1;
        assert!(
            strength(&out, at, RATE) > 0.3,
            "{}",
            strength(&out, at, RATE)
        );
        assert!(strength(&out, at / 2.0, RATE) < 0.01);
    }
}
