//! An acid bass voice after Roland's TB-303: a saw, or the square a 303 shapes from it, through a
//! diode ladder, with the 303's two envelopes, its accent and its slide. It plays one note at a
//! time, and a new note carries on from the last one's filter and accent rather than starting
//! fresh, as the one voice of a 303 does.
//!
//! How it works comes from Robin Whittle's notes on the 303 and the Devil Fish, and Tim
//! Stinchcombe's analysis of its diode ladder. Several measured values come from Robin Schmidt's
//! Open303: how the envelope moves the cutoff, the share of the filter envelope that reaches the
//! level, the level envelope's decay, and the high-pass before the filter. The filter is Vital's,
//! in [`crate::dsp::filters::diode_filter`]. The knobs' ranges and tapers are fitted to Arturia's
//! Acid V by the comparison harness, `tools/compare` (decision 117), and the slide, the accent
//! and how a note ends to where Acid V and AudioRealism's ABL3 agree, with Whittle's measurements
//! of a 303 settling where they don't (decision 127), as are how its resonance stands through the
//! chirp (decision 129) and where the chirp starts (decision 131).
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

use super::drums::soft_clip;
use super::virtual_analog::Settings;
use crate::dsp::blocks::drift::Drift;
use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::smoother::{Smoothed, coefficient as smoothing_coefficient};
use crate::dsp::filters::diode_filter::{
    DiodeFilter, HIGH_PASS_RATIO, RESONANT_SHARE, feedback_coefficient,
};
use vt_model::settings::slide_ms;
use vt_model::settings::{
    ACID_HIGHEST_CUTOFF, ACID_LOWEST_CUTOFF, acid_cutoff_position, acid_decay_ms, acid_drive_db,
};

/// How loud it is: a plain note with every knob at noon is as loud as Acid V's at 0 dB, and within
/// 6 dB of the virtual analog voice on average, as its resonance makes its peaks high for its
/// level (decision 117). It's a little under decision 117's 0.75, as the soft clip after it now
/// leaves a plain note's peaks alone (decision 127), and under that again as the filter's
/// saturation takes less of a plain note (decision 129), so the note is as loud as it was.
const LEVEL: f32 = 0.6768;
/// Where the soft clip after the voice levels off, against full scale: 12 dB over, so a full
/// accent's peaks reach 13 to 14 dB over a plain note's, as both references' do, rather than
/// being rounded off at 5.6 dB (decision 127). A 303 driven hard into what follows it is still
/// rounded off, and the track's level, its slots and the master's limiter come after it.
const HEADROOM: f32 = 4.0;
/// The square's share of each cycle spent high. A 303 makes its square by driving its saw hard
/// into a transistor with an offset, which Open303 models as `tanh(70 saw + 4.37)`: that switches
/// a little before the saw's middle, so the square isn't quite even.
const SQUARE_DUTY: f32 = 0.531;
/// The filter envelope's decay on an accented note, as a time constant in ms, or the plain
/// note's if `Decay` makes that shorter. A 303 shorts its Decay pot out on an accent, so it's
/// the pot's shortest, which Whittle gives as 200 ms; the knob's shortest, 40 ms, fitted to
/// Acid V's range (decision 117), left an accent at `Accent` 00 0.2 octaves darker than both
/// references' (decision 127).
const ACCENT_DECAY_MS: f32 = 70.0;
/// The level envelope's fixed decay, as a time constant in ms. It falls to silence over about
/// three seconds, as a 303's does.
const LEVEL_DECAY_MS: f32 = 1230.0;
/// The level envelope's attack, as a time constant: a 303's takes about 3 ms.
const ATTACK_MS: f32 = 1.0;
/// How a plain note ends: a 303 holds its level for about 8 ms after the gate closes, and then
/// falls in a straight line to nothing over about 8 more (Whittle's notes on the Devil Fish).
const RELEASE_HOLD_MS: f32 = 8.0;
const RELEASE_FALL_MS: f32 = 8.0;
/// How fast an accented note's level falls once it ends, as a time constant in ms.
const ACCENT_RELEASE_MS: f32 = 50.0;
/// The filter envelope also reaches the level, through 47k and 33 nF on a 303, which softens it.
const ENVELOPE_TO_LEVEL: f32 = 0.45;
const SOFTENING_MS: f32 = 1.55;
/// How much the accent sweep adds to the level with `Accent` full, against the level envelope,
/// which rises to one. Open303 adds 4; this is fitted to where Acid V and ABL3 agree (decision
/// 127). It's all an accent adds to the level, as an accented note plays at a plain one's
/// velocity.
const ACCENT_TO_LEVEL: f32 = 7.0;
/// The Accent knob's taper for the level: how far it's turned, squared, is how much louder an
/// accent is, so it's about even in dB from a quarter of the way up, as Acid V's is.
const ACCENT_LEVEL_TAPER: f32 = 2.0;
/// How far the accent sweep moves the cutoff at its height with `Accent` full, in octaves. The
/// pulse falls with the accent's short envelope, so it's at its height only for the first few
/// milliseconds.
const ACCENT_OCTAVES: f32 = 3.33;
/// The Accent knob's taper for the cutoff, steeper than the level's: both references' accents
/// grow louder from a quarter of the way up but brighter only over the top half, and at `00`
/// neither is any brighter than its envelope makes it (decision 127).
const ACCENT_SWEEP_TAPER: f32 = 2.7;
/// The accent sweep's capacitor charges through a diode and 147k, and drains through 100k, on
/// 1 µF. It drains more slowly than accents come, so they build up.
const SWEEP_CHARGE_MS: f32 = 147.0;
const SWEEP_DRAIN_MS: f32 = 100.0;
/// What brings the capacitor's level up to the pulse's, where the two balance, and a little over
/// that, so accents a sixteenth apart at full Res build up by about 0.4 octaves, as both
/// references' do, while accents an eighth apart hardly do. Decision 124's 3.5 times, fitted to
/// Acid V alone, built up three times as far at full Res (decision 127).
const SWEEP_GAIN: f32 = 1.05 * (SWEEP_CHARGE_MS + SWEEP_DRAIN_MS) / SWEEP_DRAIN_MS;
/// How the Resonance knob turns the accent from following the pulse to following the
/// capacitor's slower swell: as the knob to this power, so the swell takes over mostly in its
/// top half.
const SWEEP_RESONANCE_CURVE: f32 = 1.4;
/// A slide's time constant, as a share of its time, so it's nearly there when its time is up.
const SLIDE_SHARE: f32 = 1.0 / 3.0;
/// How much longer a slide down takes than one up. Whittle gives a 303's slide as 100k into
/// 0.22 µF, a time constant of 22 ms, which `Slide` at 00 is near, and both references glide up
/// to 90% in 45 to 51 ms; but both take 51 to 59 ms down a fifth or an octave, which this
/// matches (decision 127).
const FALLING_SLIDE: f32 = 1.3;
/// The high-pass before the filter, in Hz, as Open303 measured a 303's.
const HIGH_PASS_HZ: f32 = 44.486;
/// Below this, its level and accent sweep have nothing left to carry to the next note.
const SETTLED: f32 = 1e-4;
/// The level's smoothing, so nothing that jumps in it clicks: Open303's 200 Hz.
const DECLICK_HZ: f32 = 200.0;
/// The highest cutoff the envelope and accent can take it to, as on the Devil Fish.
const HIGHEST_HZ: f32 = 20_000.0;
/// How loud the saw goes into the filter, which is made up again after it. The ladder passes the
/// bass at up to 1.9 times at no resonance, and a low note's saw swings to about ±1.6 once it's
/// high-passed, so at full scale its last stage was clipped flat for up to a third of each cycle,
/// which cut deep notches into the harmonics that moved with the note (#332). At 0.3 it stayed
/// under the clip, but the ladder drives its first stage harder as the resonance rises, and its
/// saturation there still flattened a high resonance's peak and cut a cliff just past it: the
/// harmonic an octave over the peak was 6 to 8 dB further under it than ABL3's and Acid V's,
/// whose filters fall evenly past their peaks, and at Res 85% with Env Mod full the peak sank
/// about 5 dB over the note's first 40 ms, where theirs hold. At this level the ladder is nearly
/// linear at a 303's level, so the chirp keeps its shape, and `Drive` still pushes it into the
/// saturation and the clip (decision 129).
const FILTER_LEVEL: f32 = 0.1;

/// How the Resonance knob reaches the ladder, whose own law is a cube of what it's given: the
/// feedback rises as the knob to this power, up to this share of the ladder's most. A cube left
/// the knob's lower half with no peak and its top few steps with nearly all of it; this way the
/// peak climbs about evenly in dB from a quarter of the way up, and the bass thins from the first
/// step, as Acid V's does (decision 117). With the saturation no longer holding the peak down, it
/// takes less feedback to stand as high over the fundamental as both references' do, and its
/// curve is a little steeper (decision 129).
const RESONANCE_CURVE: f32 = 1.2;
const RESONANCE_TOP: f32 = 0.68;
/// How much quieter the whole voice is with Resonance full, in dB, and the knob's curve to it.
const RESONANCE_LOSS_DB: f32 = 4.0;
const RESONANCE_LOSS_CURVE: f32 = 0.7;

/// The Env Mod knob's taper: how far it's turned, to this power, is how far along Open303's
/// measured range the envelope's sweep is. Acid V's knob does little until three quarters of the
/// way and then sweeps most of its range; a square, like an audio-taper pot, keeps noon near
/// Acid V's without leaving a dead stretch (decision 117).
const ENVELOPE_MOD_TAPER: f32 = 2.0;
/// How much wider the envelope's sweep is than Open303 measured, with Env Mod at 0 and full, so
/// it spans what Acid V's does. A 303's can't be turned down to nothing.
const SWEEP_LEAST: f32 = 1.4;
const SWEEP_MOST: f32 = 1.25;
/// How much of Open303's centre the envelope's sweep is around, so the cutoff it settles to once
/// the envelope has fallen sits nearer Acid V's.
const CENTRE_SHARE: f32 = 0.6;
/// How the envelope's push on the cutoff gives as it grows, in octaves: over where the knob puts
/// the cutoff, each octave more the envelope asks for moves it less, along a tanh this wide, as a
/// control path running short of headroom does on a large swing. Env Mod at noon, whose sweep
/// reaches about two octaves over the knob, loses 0.04 octaves at its top, and Env Mod full, about
/// five, loses 0.57. With full Env Mod, both references' resonant peaks start that much lower than
/// a sweep that stays exponential all the way, and fall more slowly at first, where ours had
/// started high and fallen fastest at its top (decision 131).
const SWEEP_SOFTENING: f32 = 7.3;

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
const DRIFT: usize = 7;
const SMOOTHED: usize = 8;
/// How many frames go by between working out what follows from the settings, which are smoothed,
/// so they move little in that time.
const CONTROL_FRAMES: u32 = 16;

/// How much of the rest of the way a one-pole covers each frame, for a time constant in ms.
fn one_pole(ms: f32, rate: f32) -> f32 {
    1.0 - (-1000.0 / (ms * rate)).exp()
}

/// How many octaves the envelope moves the cutoff, for how many it would if the cutoff followed
/// it exponentially all the way: the same under where the knob puts it, and less and less over.
#[inline]
fn softened_sweep(octaves: f32) -> f32 {
    if octaves <= 0.0 {
        return octaves;
    }
    SWEEP_SOFTENING * (octaves / SWEEP_SOFTENING).tanh()
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
    /// How many frames ago the gate closed, which a plain note's end is timed from.
    released_for: u32,
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
    /// The filter's [`feedback_coefficient`] at twice the rate.
    feedback: f32,
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
    /// The same for a slide down.
    falling_slide: f32,
    /// How long the slide under way takes, if it was given a time rather than taking `Slide`'s.
    slide_ms: Option<f32>,
    /// The note's drift and the noise floor under the filter, and what the drift makes of the
    /// pitch and the level, worked out with the rest.
    drift: Drift,
    detune: f32,
    drift_level: f32,
    // Coefficients for the fixed times.
    attack: f32,
    level_decay: f32,
    hold_frames: u32,
    fall_frames: f32,
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
            released_for: 0,
            envelope: 0.0,
            softened: 0.0,
            sweep: 0.0,
            level: 0.0,
            attacking: false,
            open: 0.0,
            amplitude: 0.0,
            high_pass: 0.0,
            filter: DiodeFilter::default(),
            feedback: feedback_coefficient(rate * 2.0),
            oversampler: Oversampler::default(),
            smoothed: [Smoothed::default(); SMOOTHED],
            smoothing: smoothing_coefficient(rate as u32),
            fresh: true,
            cutoff: ACID_LOWEST_CUTOFF,
            countdown: 0,
            decay: 0.0,
            slide: 0.0,
            falling_slide: 0.0,
            slide_ms: None,
            drift: Drift::new(),
            detune: 1.0,
            drift_level: 1.0,
            attack: one_pole(ATTACK_MS, rate),
            level_decay: falling(LEVEL_DECAY_MS, rate),
            hold_frames: (RELEASE_HOLD_MS * rate / 1000.0) as u32,
            fall_frames: RELEASE_FALL_MS * rate / 1000.0,
            accent_release: falling(ACCENT_RELEASE_MS, rate),
            softening: one_pole(SOFTENING_MS, rate),
            charge: one_pole(SWEEP_CHARGE_MS, rate),
            drain: one_pole(SWEEP_DRAIN_MS, rate),
            declick: 1.0 - (-std::f32::consts::TAU * DECLICK_HZ / rate).exp(),
            high_pass_coefficient: 1.0 - (-std::f32::consts::TAU * HIGH_PASS_HZ / rate).exp(),
        }
    }

    /// Seed a voice starting from silence, so each differs but a render repeats. A 303's
    /// oscillator runs free, and its gate only opens the envelopes, so a note from silence starts
    /// anywhere in its cycle, whatever `Drift` is, and a note after it carries on from where the
    /// last left it (decision 191).
    pub fn seed(&mut self, seed: u32) {
        self.drift.seed(seed);
        self.phase = self.drift.phase();
    }

    /// Start a note, from wherever the last one left the filter and the accent sweep.
    pub fn trigger(&mut self, accent: bool) {
        self.drift.roll();
        // A note let go and played again before it has fallen away starts from where it fell to.
        if !self.gate && !self.accented {
            let closing = self.closing();
            self.level *= closing;
            self.open *= closing;
        }
        self.released_for = 0;
        self.slide_ms = None;
        self.gate = true;
        self.accented = accent;
        self.envelope = 1.0;
        self.attacking = true;
        self.sliding = false;
        self.jump = true;
        self.countdown = 0;
    }

    /// Glide to the next note with the gate held, so neither envelope starts again. The note
    /// slid to brings its own accent, which sets the filter envelope's decay from here. It takes
    /// as long as its `Slide` says, or a time it's given, from the track's `Glide`.
    pub fn slide(&mut self, accent: bool, ms: Option<f32>) {
        self.accented = accent;
        self.sliding = true;
        self.slide_ms = ms;
        self.countdown = 0;
    }

    /// Let go of the note.
    pub fn release(&mut self) {
        if self.gate {
            self.released_for = 0;
        }
        self.gate = false;
    }

    /// How much of a plain note's level is left as it ends: all of it while it's held and for
    /// the hold after, then falling in a straight line to nothing.
    fn closing(&self) -> f32 {
        if self.gate || self.accented {
            return 1.0;
        }
        let falling_for = self.released_for.saturating_sub(self.hold_frames) as f32;
        (1.0 - falling_for / self.fall_frames).max(0.0)
    }

    /// Whether its level and accent sweep have fallen away, so a note after it would start as
    /// if from nothing.
    pub fn settled(&self) -> bool {
        self.level < SETTLED && self.sweep < SETTLED
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
            let plain_decay = acid_decay_ms(values[DECAY] * 255.0);
            let decay = if self.accented {
                ACCENT_DECAY_MS.min(plain_decay)
            } else {
                plain_decay
            };
            let drift = values[DRIFT];
            self.decay = falling(decay * self.drift.time(drift), self.rate);
            self.drift.step();
            self.detune = self.drift.ratio(0, drift);
            self.drift_level = self.drift.level(drift);
            let slide = self
                .slide_ms
                .unwrap_or_else(|| slide_ms(values[SLIDE] * 255.0));
            self.slide = one_pole(slide * SLIDE_SHARE, self.rate);
            self.falling_slide = one_pole(slide * SLIDE_SHARE * FALLING_SLIDE, self.rate);
            self.countdown = CONTROL_FRAMES;
        }
        self.countdown -= 1;

        // A new note takes its pitch at once, and a slide glides there.
        let target = frequency.clamp(1e-6, 0.45);
        if self.jump || !self.sliding {
            self.heard = target;
            self.jump = false;
        } else {
            // A slide down takes a little longer than one up.
            let share = if target < self.heard {
                self.falling_slide
            } else {
                self.slide
            };
            self.heard *= (target / self.heard).powf(share);
        }

        // The filter envelope, and the accent's pulse from it into the sweep's capacitor, which
        // charges through a diode. Both are as if `Accent` were full, and the knob scales what
        // they add to the level and the cutoff, each on its own taper.
        self.envelope *= self.decay;
        self.softened += (self.envelope - self.softened) * self.softening;
        let pulse = if self.accented && self.gate {
            self.envelope
        } else {
            0.0
        };
        self.sweep += (pulse - self.sweep).max(0.0) * self.charge - self.sweep * self.drain;
        // Resonance turns the same knob as the sweep's: low, the cutoff follows the pulse, and
        // high, the capacitor's slower swell.
        let swell = resonance.powf(SWEEP_RESONANCE_CURVE);
        let accent = pulse + (self.sweep * SWEEP_GAIN - pulse) * swell;
        let knob = values[ACCENT];
        let accent_level = ACCENT_TO_LEVEL * knob.powf(ACCENT_LEVEL_TAPER) * accent;
        let accent_octaves = ACCENT_OCTAVES * knob.powf(ACCENT_SWEEP_TAPER) * accent;

        // The level envelope, with the filter envelope and accent added while the note's held.
        if self.gate {
            if self.attacking {
                self.level += (1.0 - self.level) * self.attack;
                self.attacking = self.level < 0.999;
            } else {
                self.level *= self.level_decay;
            }
            self.open += (1.0 - self.open) * self.attack;
        } else if self.accented {
            self.level *= self.accent_release;
            self.open *= self.accent_release;
        } else {
            // A plain note holds and then falls to nothing, where it's left.
            self.released_for = self.released_for.saturating_add(1);
            if self.closing() == 0.0 {
                self.level = 0.0;
                self.open = 0.0;
            }
        }
        let amplitude = (self.level
            + self.open * (ENVELOPE_TO_LEVEL * self.softened + accent_level))
            / (1.0 + ENVELOPE_TO_LEVEL)
            * self.closing();
        self.amplitude += (amplitude - self.amplitude) * self.declick;

        // The cutoff: the envelope moves it by an amount and around a centre that Open303
        // measured on a 303, which depend on both knobs, widened and raised to Acid V's span,
        // and the accent adds to it. Both knobs are taken through their tapers first.
        let knob = acid_cutoff_position(values[CUTOFF] * 255.0);
        let amount = values[ENVELOPE_MOD].powf(ENVELOPE_MOD_TAPER);
        let low = 3.774 * amount + 0.737;
        let high = 4.195 * amount + 0.864;
        let scale = ((1.0 - knob) * low + knob * high)
            * (SWEEP_LEAST + (SWEEP_MOST - SWEEP_LEAST) * amount);
        let centre = (0.0483 * knob + 0.2944) * CENTRE_SHARE;
        // The envelope's push gives as it grows, so the chirp starts lower and falls more evenly.
        let octaves = softened_sweep(scale * (self.envelope - centre)) + accent_octaves;
        // `acid_cutoff_hz`, in octaves, so one power of two does both.
        let knob_octaves = knob * (ACID_HIGHEST_CUTOFF / ACID_LOWEST_CUTOFF).log2();
        let drifted = self.drift.cutoff_octaves(values[DRIFT]);
        self.cutoff = (ACID_LOWEST_CUTOFF * (knob_octaves + octaves + drifted).exp2())
            .clamp(10.0, HIGHEST_HZ);

        // The oscillator, high-passed as a 303's is on its way to the filter, which is as good as
        // a DC blocker, so it needs no other (decision 191).
        let increment = self.heard * self.detune;
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
        // Over a floor of noise the filter can ring from.
        let input = wave - self.high_pass + self.drift.noise();

        // `acid_drive_db` as a gain.
        let drive = (acid_drive_db(values[DRIVE] * 255.0) / DECIBELS_PER_OCTAVE).exp2();
        // The filter resonates under the cutoff it's given, and the cutoff is where it resonates.
        let cutoff = self.cutoff / RESONANT_SHARE / (self.rate * 2.0);
        // Resonance thins the bass in the ladder itself, and quietens the whole voice a little
        // more, so it loses as much under its peak as Acid V does.
        let loss = (-RESONANCE_LOSS_DB / DECIBELS_PER_OCTAVE
            * resonance.powf(RESONANCE_LOSS_CURVE))
        .exp2();
        let resonance = (RESONANCE_TOP * resonance.powf(RESONANCE_CURVE)).cbrt();
        let (filter, feedback) = (&mut self.filter, self.feedback);
        let filtered = self.oversampler.process(input * FILTER_LEVEL, |sample| {
            filter.next(sample, cutoff, resonance, drive, feedback, HIGH_PASS_RATIO)
        }) * loss
            / FILTER_LEVEL;
        HEADROOM * soft_clip(filtered * self.amplitude * self.drift_level * LEVEL / HEADROOM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Settings in their places: wave, cutoff, resonance, env mod, decay, accent, slide, drive,
    /// with no drift.
    fn places(values: [f32; 8]) -> Settings {
        let [wave, cutoff, resonance, envmod, decay, accent, slide, drive] = values;
        Settings::from_places([
            wave, cutoff, resonance, envmod, decay, accent, slide, drive, 0.0,
        ])
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

    /// The level of each of a sound's first `count` harmonics in dB, times its number, so a saw's
    /// come out level and what's left is the filter's shape. Each is read at its own frequency
    /// through a Hann window.
    fn harmonics(samples: &[f32], fundamental: f32, count: usize) -> Vec<f32> {
        let length = samples.len() as f32;
        (1..=count)
            .map(|number| {
                let turn = std::f32::consts::TAU * hertz(fundamental) * number as f32;
                let (mut real, mut imaginary) = (0.0_f32, 0.0_f32);
                for (frame, sample) in samples.iter().enumerate() {
                    let at = frame as f32;
                    let window = 0.5 - 0.5 * (std::f32::consts::TAU * at / length).cos();
                    real += sample * window * (turn * at).cos();
                    imaginary += sample * window * (turn * at).sin();
                }
                20.0 * (real.hypot(imaginary) * number as f32).log10()
            })
            .collect()
    }

    #[test]
    fn a_held_saws_harmonics_follow_the_filter_without_notches() {
        // A held saw, once the filter envelope has died away, through the filter at its bottom,
        // middle and top, with no resonance and with half. Its harmonics should follow the
        // filter's smooth slope, as Acid V's do: none falls far under its two neighbours. The
        // ladder's last stage clipping a full-scale saw cut 20 to 34 dB notches here, at a
        // harmonic that moved with the note (#332).
        for note in [32.7, 65.4, 98.0, 130.8] {
            for cutoff in [0.0, 0.5, 1.0] {
                for resonance in [0.0, 0.5] {
                    let settings = places([0.0, cutoff, resonance, 0.0, 0.0, 0.5, 0.0, 0.0]);
                    let mut acid = Acid::new(RATE);
                    acid.trigger(false);
                    let out: Vec<f32> = (0..RATE as usize * 3 / 2)
                        .map(|_| acid.next(hertz(note), settings))
                        .collect();
                    let levels = harmonics(&out[RATE as usize..], note, 16);
                    for number in 2..16 {
                        let (before, here, after) =
                            (levels[number - 2], levels[number - 1], levels[number]);
                        // A notch is under both its neighbours. A harmonic on the steep side of
                        // a peak at the fundamental, as the lowest cutoff with some resonance
                        // makes, is under their mean but not under both.
                        let dip = (before + after) / 2.0 - here;
                        assert!(
                            here > before.min(after) || dip < 6.0,
                            "harmonic {number} is {dip} dB under its neighbours at {note} Hz, \
                             cutoff {cutoff}, resonance {resonance}: {levels:?}"
                        );
                    }
                }
            }
        }
    }

    /// C-1, as the chirp's specs play it.
    const C2: f32 = 65.406;

    /// The resonant peak in two cycles of a C-1 centred `ms` into a note: where the harmonic
    /// standing highest over the fundamental is, in Hz, how far it stands over the fundamental,
    /// and how far the harmonic an octave over it is under it, in dB, each harmonic times its
    /// number, as `tools/compare`'s `chirp` reports read them.
    fn resonant_peak(out: &[f32], ms: f32) -> (f32, f32, f32) {
        let size = (2.0 * RATE as f32 / C2).round() as usize;
        let start = (ms * RATE as f32 / 1000.0) as usize - size / 2;
        let levels = harmonics(&out[start..start + size], C2, 240);
        let top = (1..120)
            .max_by(|a, b| levels[*a].total_cmp(&levels[*b]))
            .expect("there are harmonics");
        let octave = levels[2 * top + 1] - levels[top];
        ((top + 1) as f32 * C2, levels[top] - levels[0], octave)
    }

    /// A plain C-1 held for half a second, at a share of each knob's turn: cutoff, resonance,
    /// env mod and decay, with `Accent` at noon.
    fn chirp(knobs: [f32; 4]) -> Vec<f32> {
        let [cutoff, resonance, envmod, decay] = knobs;
        let settings = places([0.0, cutoff, resonance, envmod, decay, 0.5, 0.0, 0.0]);
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        (0..RATE as usize / 2)
            .map(|_| acid.next(hertz(C2), settings))
            .collect()
    }

    /// The resonant peak's height over the fundamental, and the harmonic an octave over it, under
    /// it, each the mean over a note's frames every 10 ms from `from` to `to` ms.
    fn mean_peak(out: &[f32], from: usize, to: usize) -> (f32, f32) {
        let frames: Vec<_> = (from..=to)
            .step_by(10)
            .map(|ms| resonant_peak(out, ms as f32))
            .collect();
        let count = frames.len() as f32;
        (
            frames.iter().map(|frame| frame.1).sum::<f32>() / count,
            frames.iter().map(|frame| frame.2).sum::<f32>() / count,
        )
    }

    #[test]
    fn at_high_resonance_the_peak_stands_high_over_the_fundamental_through_the_chirp() {
        // Res 85% with Env Mod at noon, and 95% with it full, at Cutoff noon and Decay long: over
        // the note's first 200 ms the peak stands about as high over the fundamental as Acid V's
        // and ABL3's, where the saturation had held the first under 14.5 dB (decision 129).
        let (height, _) = mean_peak(&chirp([0.5, 0.85, 0.5, 0.8]), 40, 200);
        assert!((14.8..=17.5).contains(&height), "{height} dB at Res 85%");
        let (height, _) = mean_peak(&chirp([0.5, 0.95, 1.0, 0.8]), 40, 200);
        assert!((18.0..=22.5).contains(&height), "{height} dB at Res 95%");
    }

    #[test]
    fn past_its_peak_the_filter_falls_evenly_rather_than_off_a_cliff() {
        // The harmonic an octave over the peak, under it: both references fall about 23 to 26 dB
        // there, as a four-pole filter does, where the saturation had cut a cliff 30 to 34 dB deep
        // just past the peak (decision 129).
        for knobs in [[0.5, 0.85, 0.5, 0.8], [0.5, 0.85, 1.0, 0.2]] {
            let (_, octave) = mean_peak(&chirp(knobs), 40, 200);
            assert!(
                octave > -29.0,
                "{octave} dB an octave over the peak at {knobs:?}"
            );
        }
    }

    #[test]
    fn the_chirp_sweeps_down_from_where_the_envelope_throws_it() {
        // Res 95%, Env Mod full, Cutoff noon and Decay long: the peak starts near 4.2 kHz, where
        // Acid V's and ABL3's start, and is near 1.6 kHz by 150 ms, a little under theirs, 1.8 and
        // 2.4 kHz (decisions 129 and 131).
        let out = chirp([0.5, 0.95, 1.0, 0.8]);
        let (start, _, _) = resonant_peak(&out, 20.0);
        let (later, _, _) = resonant_peak(&out, 150.0);
        assert!((3_700.0..=4_800.0).contains(&start), "{start} Hz at 20 ms");
        assert!((1_400.0..=2_200.0).contains(&later), "{later} Hz at 150 ms");
    }

    #[test]
    fn the_envelopes_push_on_the_cutoff_gives_as_it_grows() {
        // Under the knob's cutoff the sweep is as it was; over it, each octave more moves the
        // cutoff less: hardly at Env Mod noon's two octaves, and by over half an octave at Env
        // Mod full's five (decision 131).
        assert_eq!(softened_sweep(-1.0), -1.0);
        assert!((softened_sweep(1.94) - 1.94).abs() < 0.05);
        let full = softened_sweep(4.75);
        assert!((4.1..=4.25).contains(&full), "{full} octaves");
        let steps: Vec<f32> = (0..=80)
            .map(|step| softened_sweep(step as f32 * 0.1))
            .collect();
        assert!(steps.windows(2).all(|pair| pair[1] > pair[0]));
    }

    #[test]
    fn with_env_mod_full_the_chirp_starts_lower_and_falls_less_steeply() {
        // Res 95%, Env Mod full and Decay long. Where both references agree, the peak starts
        // about 0.4 octaves under where an envelope exponential all the way threw ours, 5.8 kHz
        // at Cutoff noon and 3.5 at Cutoff 20%, and falls more slowly at first: from 20 to 120 ms
        // Acid V's falls 0.65 octaves and ABL3's 0.93, where ours fell 1.37 (decision 131).
        let out = chirp([0.5, 0.95, 1.0, 0.8]);
        let (start, _, _) = resonant_peak(&out, 20.0);
        let (later, _, _) = resonant_peak(&out, 120.0);
        let fall = (start / later).log2();
        assert!(
            (0.8..=1.2).contains(&fall),
            "{fall} octaves from 20 to 120 ms"
        );
        let (low, _, _) = resonant_peak(&chirp([0.2, 0.95, 1.0, 0.8]), 20.0);
        assert!(
            (2_200.0..=3_000.0).contains(&low),
            "{low} Hz at 20 ms, Cutoff 20%"
        );
    }

    #[test]
    fn it_sounds_about_as_loud_as_the_virtual_analog_voice() {
        let mut va = crate::dsp::voices::virtual_analog::VirtualAnalog::new();
        let va: Vec<f32> = (0..24_000)
            .map(|_| va.next(hertz(110.0), Settings::default()))
            .collect();
        let acid = note(false, plain(), 24_000);
        let decibels = 20.0 * (rms(&acid[..4_800]) / rms(&va)).log10();
        assert!(decibels.abs() < 6.0, "{decibels} dB");
    }

    #[test]
    fn an_accent_is_louder_and_brighter() {
        // As on Acid V and ABL3, an accent is hardly louder with `Accent` at 00, about 4 dB at
        // noon and about 10 dB full (decisions 124 and 127).
        for (accent, least, most) in [(0.0, -1.0, 1.5), (0.5, 3.0, 6.0), (1.0, 8.0, 11.5)] {
            let settings = places([0.0, 0.5, 0.5, 0.5, 0.5, accent, 0.0, 0.0]);
            let plain_note = note(false, settings, 4_800);
            let accented = note(true, settings, 4_800);
            let louder = 20.0 * (rms(&accented) / rms(&plain_note)).log10();
            assert!(louder > least && louder < most, "{louder} dB at {accent}");
        }
        // The cutoff over the note's first 20 ms is higher with `Accent` full. After that, an
        // accent's short filter envelope has it darker than a plain note, as on a 303.
        let full = places([0.0, 0.5, 0.5, 0.5, 0.5, 1.0, 0.0, 0.0]);
        let mean_cutoff = |accent| {
            let mut acid = Acid::new(RATE);
            acid.trigger(accent);
            let total: f32 = (0..960)
                .map(|_| {
                    acid.next(hertz(110.0), full);
                    acid.cutoff().log2()
                })
                .sum();
            (total / 960.0).exp2()
        };
        let (plain_cutoff, accented_cutoff) = (mean_cutoff(false), mean_cutoff(true));
        assert!(
            accented_cutoff > plain_cutoff * 1.1,
            "{plain_cutoff} {accented_cutoff}"
        );
        // Which is heard, on a darker sound.
        let dark = places([0.0, 0.2, 0.5, 0.2, 0.5, 1.0, 0.0, 0.0]);
        let (plain_note, accented) = (note(false, dark, 960), note(true, dark, 960));
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

    /// The highest cutoff over each of `notes` accented notes, `apart` frames apart, each held
    /// for half a sixteenth at 120 BPM.
    fn accent_peaks_apart(notes: usize, settings: Settings, apart: usize) -> Vec<f32> {
        let mut acid = Acid::new(RATE);
        let held = RATE as usize / 16;
        (0..notes)
            .map(|_| {
                acid.trigger(true);
                (0..apart)
                    .map(|frame| {
                        if frame == held {
                            acid.release();
                        }
                        acid.next(hertz(55.0), settings);
                        acid.cutoff()
                    })
                    .fold(0.0, f32::max)
            })
            .collect()
    }

    /// The same, a sixteenth apart.
    fn accent_peaks(notes: usize, settings: Settings) -> Vec<f32> {
        accent_peaks_apart(notes, settings, RATE as usize / 8)
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
    fn accents_a_sixteenth_apart_build_up_and_an_eighth_apart_hardly_do() {
        // With every knob at noon but `Accent` full: four accents in a row peak higher by the
        // last, and four with a sixteenth's rest between them build far less, as the capacitor
        // drains in the rest. Decision 124 had them peak over 0.8 octaves higher, fitted to
        // Acid V alone, whose accents build up even with `Accent` at 00; where it and ABL3 agree
        // they build far less, so a third to a half of an octave here (decision 127).
        let settings = places([0.0, 0.25, 0.5, 0.5, 0.5, 1.0, 0.0, 0.0]);
        let built = |peaks: Vec<f32>| (peaks[3] / peaks[0]).log2();
        let in_a_row = built(accent_peaks_apart(4, settings, RATE as usize / 8));
        let apart = built(accent_peaks_apart(4, settings, RATE as usize / 4));
        assert!(
            (0.2..=0.6).contains(&in_a_row),
            "{in_a_row} octaves in a row"
        );
        assert!(
            apart < 0.4 && apart < in_a_row / 3.0,
            "{apart} octaves apart"
        );
    }

    #[test]
    fn a_slide_glides_the_pitch_and_keeps_the_envelopes_going() {
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        for _ in 0..RATE / 10 {
            acid.next(hertz(110.0), plain());
        }
        let before = acid.envelope();
        acid.slide(false, None);
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
            acid.slide(false, None);
            for _ in 0..RATE / 20 {
                acid.next(hertz(220.0), settings);
            }
        }
        assert!(slow.heard() < quick.heard() - 20.0 / RATE as f32);
    }

    /// How many ms a slide at `Slide` 00 from one pitch to another takes to go 90% of the way.
    fn ninety_percent_ms(from: f32, to: f32) -> f32 {
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        acid.next(hertz(from), plain());
        acid.slide(false, None);
        let ninety = (to / from).log2() * 0.9;
        let frames = (1..RATE)
            .find(|_| {
                acid.next(hertz(to), plain());
                (acid.heard() / hertz(from)).log2().abs() >= ninety.abs()
            })
            .expect("it gets there");
        frames as f32 * 1000.0 / RATE as f32
    }

    #[test]
    fn a_slide_up_takes_about_46_ms_to_go_90_percent_and_one_down_about_60() {
        // A 303's slide is its 100k and 0.22 µF, a 22 ms time constant, so about 51 ms to 90%
        // (Whittle). Both references reach 90% up in 45 to 51 ms by any interval, and down a
        // fifth or an octave in 51 to 59 (decision 127).
        for (from, to) in [(65.4, 130.8), (65.4, 98.0), (65.4, 69.3)] {
            let up = ninety_percent_ms(from, to);
            assert!((42.0..=50.0).contains(&up), "{up} ms up to {to} Hz");
            let down = ninety_percent_ms(to, from);
            assert!((55.0..=65.0).contains(&down), "{down} ms down from {to} Hz");
        }
    }

    /// A plain note's level over each ms after it's let go, against its level just before.
    fn released_levels(acid: &mut Acid, ms: usize) -> Vec<f32> {
        let before = acid.amplitude;
        acid.release();
        (0..ms)
            .map(|_| {
                for _ in 0..RATE / 1000 {
                    acid.next(hertz(65.4), plain());
                }
                acid.amplitude / before
            })
            .collect()
    }

    #[test]
    fn a_plain_note_holds_for_8_ms_once_let_go_and_falls_to_nothing_by_17() {
        // As Whittle measured a 303: 8 ms at its level, then 8 ms of a straight fall. Through
        // the voice and its track's note shape, the harness reads 20 dB down in about 17 ms and
        // 40 dB in about 23, between the references' (decision 127).
        let mut acid = Acid::new(RATE);
        acid.trigger(false);
        for _ in 0..RATE / 4 {
            acid.next(hertz(65.4), plain());
        }
        let levels = released_levels(&mut acid, 30);
        assert!(levels[6] > 0.95, "{levels:?}");
        // Half way down the fall, 12 ms after, with a little lag from the declick.
        assert!((0.4..=0.65).contains(&levels[11]), "{levels:?}");
        assert!(levels[17] < 0.01, "{levels:?}");
        assert!(
            acid.level == 0.0 && acid.open == 0.0,
            "it's left at nothing"
        );
    }

    #[test]
    fn a_note_played_again_5_ms_after_it_ends_hardly_dips() {
        // A 5 ms gap falls inside the hold, so the level hardly moves, as on ABL3, where it dips
        // 0.1 dB; Acid V's dips 2.7. 20 ms apart, the note has ended and starts again.
        let again_after = |gap| {
            let mut acid = Acid::new(RATE);
            acid.trigger(false);
            for _ in 0..RATE / 4 {
                acid.next(hertz(65.4), plain());
            }
            let levels = released_levels(&mut acid, gap);
            acid.trigger(false);
            acid.next(hertz(65.4), plain());
            (levels, acid.level)
        };
        let (levels, level) = again_after(5);
        assert!(levels.iter().all(|level| *level > 0.97), "{levels:?}");
        // Played again, it carries on from its level rather than starting from nothing.
        assert!(level > 0.7, "{level}");
        let (_, level) = again_after(20);
        assert!(level < 0.1, "{level}");
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
                                acid.slide(true, None);
                            }
                        }
                        // Up to the soft clip's ceiling, 12 dB over full scale, which a full
                        // accent's peaks may reach (decision 127).
                        let sample = acid.next(hertz / rate as f32, settings);
                        assert!(
                            sample.is_finite() && sample.abs() <= HEADROOM,
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
