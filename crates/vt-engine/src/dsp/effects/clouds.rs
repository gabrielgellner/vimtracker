//! Mutable Instruments' Clouds as the module plays: what comes in, with what's fed back, is
//! recorded all the time into a buffer of the module's length for its quality, and one of three
//! players reads it back: grains (`Granular`), two windows lined up where they match best
//! (`Stretch`, WSOLA) or a delay that loops what it holds when frozen (`Looping`). After the
//! player come Clouds' diffuser, its pitch shifter in `Looping`, its filters in `Stretch` and
//! `Looping`, and its reverb, then the blend of what came in with all that.
//!
//! It runs at the rate it's given rather than the module's 32 kHz, with every time the module
//! gives in samples scaled to it, so its buffers hold the module's seconds. The 8-bit qualities
//! record and play at half the rate, through the module's resampling filter, as the module's do
//! at 16 kHz, so their mu-law grit is band-limited as the module's is. The reverb is this
//! program's `Reverb`, which is Clouds' own, tuned as the module tunes it, and the pitch shifter
//! after the looping delay is `Pitch`'s, Clouds' own too. Its spectral mode is left out.
//!
//! Adapted from Clouds' `clouds/dsp/granular_processor.cc`, `granular_sample_player.h`,
//! `grain.h`, `wsola_sample_player.h`, `window.h`, `correlator.cc`, `looping_sample_player.h`,
//! `audio_buffer.h`, `mu_law.h`, `sample_rate_converter.h`, `fx/diffuser.h` and the tables of
//! `resources/lookup_tables.py` and `resources.cc`, MIT:
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

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI, SQRT_2};

use vt_model::insert::{Clouds as Settings, CloudsMode, shift_semitones};

use crate::dsp::blocks::plaits_parts::{Svf, soft_limit};
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::pitch_shifter::PitchShifter;
use crate::dsp::effects::reverb::Reverb;
use crate::rng::Pcg32;

/// The rate the module runs at, which its times in samples are for.
const CLOUDS_RATE: f32 = 32_000.0;
/// How many samples of each side the module records in each quality, at its rate for that
/// quality, 32 kHz or 16 kHz for 8 bits: about 1, 1.9, 4.1 and 7.4 seconds.
const RECORDING_SAMPLES: [usize; 4] = [32_696, 59_384, 65_400, 118_776];
/// The most of those any quality records, both sides together.
const MOST_RECORDED: usize = 130_800;
/// How many grains can play at once in each quality, as the module has them.
const GRAINS_OF_QUALITY: [usize; 4] = [32, 40, 46, 57];
const MOST_GRAINS: usize = 64;
/// How much of what comes in is kept while frozen, to fade the recording back in when it starts
/// again, and how long a frozen loop's ends fade, in the module's samples.
const FREEZE_TAIL: f32 = 256.0;
const LOOP_CROSSFADE: f32 = 64.0;
/// How far behind the newest sample the looping delay reads at its shortest, in samples, which
/// leaves room for reading between them.
const LOOP_LEAD: i64 = 4;
/// The longest window `Stretch` has, in the module's samples, and the most words the correlator
/// compares: of the window it's matching, and of twice the window it's looking in.
const MOST_WINDOW: f32 = 4096.0;
const SOURCE_WORDS: usize = 130;
const TARGET_WORDS: usize = 2 * SOURCE_WORDS + 2;
/// How much louder the wet sound comes out than it's played, as the module's.
const POST_GAIN: f32 = 1.2;
/// The module's diffuser: four allpasses a side, in its samples, and how far they spread.
const DIFFUSER_LENGTHS: [[f32; 4]; 2] =
    [[126.0, 180.0, 269.0, 444.0], [151.0, 205.0, 245.0, 405.0]];
const DIFFUSER_SPREAD: f32 = 0.625;
/// How the module tunes its reverb's allpasses and what goes into it.
const REVERB_DIFFUSION: f32 = 0.7;
const REVERB_INPUT_GAIN: f32 = 0.2;
/// The module's resampling filter, `src_filter_1x_2_45`, for its 8-bit qualities' half rate.
#[allow(clippy::excessive_precision, clippy::unreadable_literal)]
const RESAMPLING: [f32; 45] = [
    -6.928606892e-04,
    -5.894682972e-03,
    4.393903915e-04,
    5.352009980e-03,
    1.833575577e-03,
    -7.103853054e-03,
    -5.275577768e-03,
    7.999060050e-03,
    1.029879712e-02,
    -7.191125897e-03,
    -1.675763381e-02,
    3.628265970e-03,
    2.423749384e-02,
    4.020326715e-03,
    -3.208822586e-02,
    -1.775516900e-02,
    3.947412082e-02,
    4.200610725e-02,
    -4.553678524e-02,
    -9.270618476e-02,
    4.952442102e-02,
    3.157869177e-01,
    4.528032253e-01,
    3.157869177e-01,
    4.952442102e-02,
    -9.270618476e-02,
    -4.553678524e-02,
    4.200610725e-02,
    3.947412082e-02,
    -1.775516900e-02,
    -3.208822586e-02,
    4.020326715e-03,
    2.423749384e-02,
    3.628265970e-03,
    -1.675763381e-02,
    -7.191125897e-03,
    1.029879712e-02,
    7.999060050e-03,
    -5.275577768e-03,
    -7.103853054e-03,
    1.833575577e-03,
    5.352009980e-03,
    4.393903915e-04,
    -5.894682972e-03,
    -6.928606892e-04,
];
/// How many points the grains' smooth window and the blend's crossfades are read between.
const WINDOW_POINTS: usize = 4096;
const CROSSFADE_POINTS: usize = 16;

/// stmlib's `SoftClip`: `soft_limit` up to 3, and 1 past it.
#[inline]
fn soft_clip(x: f32) -> f32 {
    if x < -3.0 {
        -1.0
    } else if x > 3.0 {
        1.0
    } else {
        soft_limit(x)
    }
}

/// A sample's 8-bit mu-law code, as the module writes one.
fn mu_law_encode(pcm: i16) -> u8 {
    let mut value = i32::from(pcm) >> 2;
    let mask = if value < 0 {
        value = -value;
        0x7F
    } else {
        0xFF
    };
    value = value.min(8159) + (0x84 >> 2);
    // The segment is how many bits past six it takes.
    let segment = (32 - value.leading_zeros() as i32 - 6).clamp(0, 8);
    if segment >= 8 {
        0x7F ^ mask
    } else {
        ((segment << 4) as u8 | ((value >> (segment + 1)) & 0x0F) as u8) ^ mask
    }
}

/// What a mu-law code plays as, the module's `lut_ulaw`.
fn mu_law_decode(code: u8) -> i16 {
    let code = !code;
    let magnitude = ((i32::from(code & 0x0F) << 3) + 0x84) << ((code & 0x70) >> 4);
    if code & 0x80 != 0 {
        (0x84 - magnitude) as i16
    } else {
        (magnitude - 0x84) as i16
    }
}

/// What reaching a value at `per_block` a block of `frames` takes a frame.
fn per_frame(per_block: f32, frames: f32) -> f32 {
    1.0 - (1.0 - per_block).powf(1.0 / frames.max(1.0))
}

/// The module's buffer of what it's recorded, a side or both, in 16 bits or 8-bit mu-law, kept
/// as what each plays back as. It's sized for the longest any quality records, and a new quality
/// starts a new recording in it; what hasn't been recorded since reads as silence.
#[derive(Debug, Clone)]
struct Recording {
    samples: Vec<f32>,
    /// How many each side holds, and how many sides.
    size: usize,
    sides: usize,
    mu_law: bool,
    /// Where the next is written.
    head: usize,
    /// How much has been written since it started, at most all of it.
    filled: usize,
    /// What came in while frozen, kept to fade the recording back in with, how much of it, and
    /// how much is left to fade.
    tail: Vec<[f32; 2]>,
    kept: usize,
    fading: usize,
    fade_of: usize,
    was_frozen: bool,
}

impl Recording {
    fn new(most: usize, tail: usize) -> Self {
        Self {
            samples: vec![0.0; most],
            size: most / 2,
            sides: 2,
            mu_law: false,
            head: 0,
            filled: 0,
            tail: vec![[0.0; 2]; tail],
            kept: 0,
            fading: 0,
            fade_of: 0,
            was_frozen: false,
        }
    }

    /// Start a new recording, of so many a side.
    fn reset(&mut self, size: usize, sides: usize, mu_law: bool) {
        self.size = size.min(self.samples.len() / sides);
        self.sides = sides;
        self.mu_law = mu_law;
        self.head = 0;
        self.filled = 0;
        self.kept = 0;
        self.fading = 0;
        self.was_frozen = false;
    }

    /// What a sample plays back as once it's written: clipped to 16 bits, then companded to 8.
    #[inline]
    fn stored(&self, sample: f32) -> f32 {
        let pcm = ((sample * 32_768.0) as i32).clamp(-32_768, 32_767) as i16;
        let pcm = if self.mu_law {
            mu_law_decode(mu_law_encode(pcm))
        } else {
            pcm
        };
        f32::from(pcm) / 32_768.0
    }

    /// Record a frame, or keep a little of it while frozen. The first frames once it records
    /// again fade from what was kept into what's coming in, so the join doesn't click.
    #[inline]
    fn write(&mut self, mut frame: [f32; 2], recording: bool) {
        if !recording {
            if !self.was_frozen {
                self.kept = 0;
                self.fading = 0;
            }
            self.was_frozen = true;
            if self.kept < self.tail.len() {
                self.tail[self.kept] = frame;
                self.kept += 1;
            }
            return;
        }
        if self.was_frozen {
            self.was_frozen = false;
            self.fade_of = self.kept;
            self.fading = self.kept;
        }
        if self.fading > 0 {
            let kept = self.tail[self.fade_of - self.fading];
            self.fading -= 1;
            let gain = self.fading as f32 / self.fade_of as f32;
            for (sample, kept) in frame.iter_mut().zip(kept) {
                *sample += (kept - *sample) * gain;
            }
        }
        for side in 0..self.sides {
            self.samples[side * self.size + self.head] = self.stored(frame[side]);
        }
        self.head += 1;
        self.filled = self.filled.max(self.head);
        if self.head >= self.size {
            self.head = 0;
        }
    }

    /// A sample by its place, counted round from the start.
    #[inline]
    fn at(&self, side: usize, mut index: usize) -> f32 {
        while index >= self.size {
            index -= self.size;
        }
        if index >= self.filled {
            0.0
        } else {
            self.samples[side * self.size + index]
        }
    }

    /// A place counted round from anywhere, ahead of the start or past the end.
    #[inline]
    fn wrapped(&self, index: i64) -> usize {
        index.rem_euclid(self.size as i64) as usize
    }

    #[inline]
    fn linear(&self, side: usize, index: usize, t: f32) -> f32 {
        let (a, b) = (self.at(side, index), self.at(side, index + 1));
        a + (b - a) * t
    }

    /// Between the second and third of four, after Laurent de Soras's Hermite interpolator, as
    /// the module reads.
    #[inline]
    fn hermite(&self, side: usize, index: usize, t: f32) -> f32 {
        let xm1 = self.at(side, index);
        let x0 = self.at(side, index + 1);
        let x1 = self.at(side, index + 2);
        let x2 = self.at(side, index + 3);
        let c = (x1 - xm1) * 0.5;
        let v = x0 - x1;
        let w = c + v;
        let a = w + v + (x2 - x0) * 0.5;
        let b_neg = w + a;
        (((a * t) - b_neg) * t + c) * t + x0
    }
}

/// One grain: where it starts reading, how far through it is, its envelope and its gains.
#[derive(Debug, Clone, Copy)]
struct Grain {
    active: bool,
    first: usize,
    phase: f64,
    increment: f64,
    /// Through its envelope, from 0 to 2, and how far each frame.
    envelope: f32,
    envelope_step: f32,
    /// How far the envelope's triangle bends towards a bell, or how steep its sides are.
    smoothness: f32,
    slope: f32,
    gains: [f32; 2],
    /// Whether it reads between samples with a cubic and has the bell, or the module's cheaper
    /// line and plain triangle, which it gives grains when most are playing.
    high: bool,
}

impl Grain {
    const IDLE: Self = Self {
        active: false,
        first: 0,
        phase: 0.0,
        increment: 1.0,
        envelope: 2.0,
        envelope_step: 0.0,
        smoothness: 0.0,
        slope: 0.0,
        gains: [1.0; 2],
        high: true,
    };
}

/// One of `Stretch`'s two windows.
#[derive(Debug, Clone, Copy)]
struct Window {
    first: usize,
    phase: f64,
    increment: f64,
    envelope_step: f32,
    done: bool,
    half: bool,
    regenerated: bool,
}

impl Window {
    const IDLE: Self = Self {
        first: 0,
        phase: 0.0,
        increment: 1.0,
        envelope_step: 0.0,
        done: true,
        half: false,
        regenerated: false,
    };

    /// Start reading a window's worth from a place, at a speed. The module's own leaves its
    /// window done, so it would never play; it's started here.
    fn start(&mut self, recording: &Recording, start: i64, width: i64, ratio: f32) {
        self.first = recording.wrapped(start);
        self.increment = f64::from(ratio);
        self.phase = 0.0;
        self.regenerated = false;
        self.done = false;
        self.half = false;
        self.envelope_step = 2.0 / width.max(1) as f32;
    }

    #[inline]
    fn needs_regeneration(&self) -> bool {
        self.half && !self.regenerated
    }

    /// Add this frame of it, under its triangle.
    #[inline]
    fn overlap_add(&mut self, recording: &Recording, out: &mut [f32; 2]) {
        if self.done {
            return;
        }
        let whole = self.phase.floor();
        let t = (self.phase - whole) as f32;
        let envelope = whole as f32 * self.envelope_step;
        self.done = envelope >= 2.0;
        self.half = envelope >= 1.0;
        let gain = if envelope >= 1.0 {
            2.0 - envelope
        } else {
            envelope
        };
        let index = self.first + whole as usize;
        let left = recording.hermite(0, index, t) * gain;
        if recording.sides == 1 {
            out[0] += left;
            out[1] += left;
        } else {
            out[0] += left;
            out[1] += recording.hermite(1, index, t) * gain;
        }
        self.phase += self.increment;
    }
}

/// Where a window lines up best with the one before, by the signs of their samples, 32 compared
/// at once, after the module's correlator. It searches a few candidates a frame.
#[derive(Debug, Clone)]
struct Correlator {
    source: [u32; SOURCE_WORDS],
    target: [u32; TARGET_WORDS],
    offset: i64,
    /// Samples between bits.
    increment: f64,
    /// How many bits it matches, how many candidates there are, and how many it tries a frame.
    size: usize,
    candidate: usize,
    per_frame: usize,
    best_score: u32,
    best_match: usize,
    done: bool,
}

impl Correlator {
    fn new() -> Self {
        Self {
            source: [0; SOURCE_WORDS],
            target: [0; TARGET_WORDS],
            offset: 0,
            increment: 1.0,
            size: 0,
            candidate: 0,
            per_frame: 1,
            best_score: 0,
            best_match: 0,
            done: true,
        }
    }

    fn best_match(&self) -> i64 {
        self.offset + (self.best_match as f64 * self.increment).floor() as i64
    }

    fn start_search(&mut self, size: usize, offset: i64, increment: f64, frames: usize) {
        self.offset = offset;
        self.increment = increment;
        self.best_score = 0;
        self.best_match = 0;
        self.candidate = 0;
        self.size = size;
        // Done within a quarter of a window, well before the next is due.
        self.per_frame = (size * 4).div_ceil(frames.max(1)) + 1;
        self.done = false;
    }

    #[inline]
    fn evaluate_some(&mut self) {
        for _ in 0..self.per_frame {
            if self.done {
                return;
            }
            let words = (self.size >> 5).min(SOURCE_WORDS);
            let offset_words = self.candidate >> 5;
            let offset_bits = self.candidate & 31;
            let mut score = 0;
            for word in 0..words {
                let at = (offset_words + word).min(TARGET_WORDS - 2);
                let mut target = self.target[at] << offset_bits;
                if offset_bits != 0 {
                    target |= self.target[at + 1] >> (32 - offset_bits);
                }
                score += (!(self.source[word] ^ target)).count_ones();
            }
            if score > self.best_score {
                self.best_match = self.candidate;
                self.best_score = score;
            }
            self.candidate += 1;
            self.done = self.candidate >= self.size;
        }
    }
}

/// The signs of the recording from a place on, a bit a sample so many apart, for so many samples:
/// how many bits it wrote, in whole words.
fn sign_bits(
    recording: &Recording,
    source: i64,
    length: i64,
    increment: f64,
    words: &mut [u32],
) -> usize {
    let start = recording.wrapped(source);
    let mut phase = 0.0_f64;
    let mut bits = 0_u32;
    let mut counter = 0_usize;
    let mut written = 0;
    let mut store = |counter: usize, bits: u32, written: &mut usize| {
        if counter & 31 == 31
            && let Some(word) = words.get_mut(counter >> 5)
        {
            *word = bits;
            *written += 32;
        }
    };
    while (phase.floor() as i64) < length {
        let whole = phase.floor();
        let t = (phase - whole) as f32;
        let index = start + whole as usize;
        let mut sample = recording.linear(0, index, t);
        if recording.sides == 2 {
            sample += recording.linear(1, index, t);
        }
        bits |= u32::from(sample > 0.0);
        store(counter, bits, &mut written);
        counter += 1;
        bits <<= 1;
        phase += increment;
    }
    while counter & 31 != 0 {
        store(counter, bits, &mut written);
        counter += 1;
        bits <<= 1;
    }
    written
}

/// An allpass of the diffuser, a line of its own.
#[derive(Debug, Clone)]
struct Allpass {
    line: Vec<f32>,
    at: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            line: vec![0.0; length.max(1)],
            at: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, spread: f32) -> f32 {
        let read = self.line[self.at];
        let sum = input + read * spread;
        self.line[self.at] = sum;
        self.at += 1;
        if self.at == self.line.len() {
            self.at = 0;
        }
        sum * -spread + read
    }
}

/// The module's resampling filter, halving or doubling the rate, with what it's heard.
#[derive(Debug, Clone, Copy)]
struct Resampler {
    history: [[f32; 2]; RESAMPLING.len()],
    newest: usize,
}

impl Resampler {
    const QUIET: Self = Self {
        history: [[0.0; 2]; RESAMPLING.len()],
        newest: 0,
    };

    #[inline]
    fn push(&mut self, frame: [f32; 2]) {
        self.newest = if self.newest == 0 {
            RESAMPLING.len() - 1
        } else {
            self.newest - 1
        };
        self.history[self.newest] = frame;
    }

    /// The taps from `first` on, `every` apart, against what it's heard from the newest back.
    #[inline]
    fn filtered(&self, first: usize, every: usize) -> [f32; 2] {
        let mut out = [0.0; 2];
        let mut at = self.newest;
        for tap in (first..RESAMPLING.len()).step_by(every) {
            let frame = self.history[at];
            out[0] += frame[0] * RESAMPLING[tap];
            out[1] += frame[1] * RESAMPLING[tap];
            at += 1;
            if at == RESAMPLING.len() {
                at = 0;
            }
        }
        out
    }
}

/// What the module works out once a block of a millisecond, from where its settings are.
#[derive(Debug, Clone, Copy, Default)]
struct Control {
    position: f32,
    size: f32,
    semitones: f32,
    density: f32,
    texture: f32,
    spread: f32,
    feedback: f32,
    /// `Granular`'s overlap and envelope, and whether its grains come regularly.
    overlap: f32,
    window_shape: f32,
    regular: bool,
    diffusion: f32,
    /// How much of the fed back sound goes in, and how much reverb comes out.
    feedback_gain: f32,
    reverb_mix: f32,
}

/// Clouds: its recording, its players, what comes after them, and what moves them.
#[derive(Debug, Clone)]
pub struct Clouds {
    recording: Recording,
    mode: CloudsMode,
    quality: u8,
    frozen: bool,
    grains: [Grain; MOST_GRAINS],
    most_grains: usize,
    /// `Granular`'s count of grains playing, smoothed, its gain for them, the size of grain
    /// it's been starting, and how long since it started one.
    playing: f32,
    grain_gain: f32,
    size_hint: f32,
    since_grain: f32,
    windows: [Window; 2],
    window_size: i64,
    smoothed_semitones: f32,
    next_ratio: f32,
    loaded: bool,
    search_source: i64,
    search_target: i64,
    correlator: Correlator,
    /// The looping delay: where it reads, and its loop and the fade from the last one.
    delay: f32,
    loop_phase: f32,
    loop_point: f32,
    loop_duration: f32,
    tail_start: f32,
    tail_duration: f32,
    loop_reset: f32,
    diffuser: [[Allpass; 4]; 2],
    shifter: PitchShifter,
    low_pass: [Svf; 2],
    high_pass: [Svf; 2],
    low_pass_open: bool,
    feedback_filter: [Svf; 2],
    reverb: Reverb,
    /// What came out a block ago, to feed back, as the module feeds back its last block.
    fed: Vec<[f32; 2]>,
    fed_at: usize,
    /// The 8-bit qualities' halving and doubling of the rate, and the frame doubled ahead.
    down: Resampler,
    up: Resampler,
    odd: bool,
    ahead: [f32; 2],
    /// How far frozen it is, for fading out the feedback and turning up the reverb.
    freeze_fade: f32,
    position: Smoothed,
    size: Smoothed,
    semitones: Smoothed,
    density: Smoothed,
    texture: Smoothed,
    blend: Smoothed,
    spread: Smoothed,
    feedback: Smoothed,
    reverb_amount: Smoothed,
    control: Control,
    /// Frames to the next block, and how many a block has.
    until_control: usize,
    block: usize,
    /// How many of its frames make one of the module's samples.
    scale: f32,
    /// What smooths per block, what `Granular`'s count and gain move by, what the delay glides by.
    block_smoothing: f32,
    playing_up: f32,
    playing_down: f32,
    grain_gain_smoothing: f32,
    delay_smoothing: f32,
    freeze_smoothing: f32,
    smoothing: f32,
    /// The grains' smooth window and the blend's crossfades, as the module's tables.
    window: Vec<f32>,
    fade_in: [f32; CROSSFADE_POINTS + 1],
    fade_out: [f32; CROSSFADE_POINTS + 1],
    rng: Pcg32,
    rate: u32,
}

impl Clouds {
    /// A recording long enough for any quality at this rate, and everything after it.
    pub fn new(rate: u32) -> Self {
        let rate_f = rate as f32;
        let scale = rate_f / CLOUDS_RATE;
        let most = (MOST_RECORDED as f32 * scale).ceil() as usize + 2;
        let tail = (FREEZE_TAIL * scale).ceil() as usize;
        let block = ((rate_f / 1000.0).round() as usize).max(1);
        let window = (0..=WINDOW_POINTS)
            .map(|at| 1.0 - ((at as f32 / WINDOW_POINTS as f32 * PI).cos() + 1.0) / 2.0)
            .collect();
        let crossfade = |at: usize| {
            let t = at as f32 / CROSSFADE_POINTS as f32;
            (1.04 * t - 0.02).clamp(0.0, 1.0) * FRAC_PI_2
        };
        let smoothing = coefficient(rate);
        let mut clouds = Self {
            recording: Recording::new(most, tail),
            mode: CloudsMode::Granular,
            quality: 0,
            frozen: false,
            grains: [Grain::IDLE; MOST_GRAINS],
            most_grains: GRAINS_OF_QUALITY[0],
            playing: 0.0,
            grain_gain: 1.0,
            size_hint: 1024.0 * scale,
            since_grain: 0.0,
            windows: [Window::IDLE; 2],
            window_size: (MOST_WINDOW * 0.5 * scale) as i64,
            smoothed_semitones: 0.0,
            next_ratio: 1.0,
            loaded: true,
            search_source: 0,
            search_target: 0,
            correlator: Correlator::new(),
            delay: 0.0,
            loop_phase: 0.0,
            loop_point: 0.0,
            loop_duration: 0.0,
            tail_start: 0.0,
            tail_duration: 1.0,
            loop_reset: 0.0,
            diffuser: DIFFUSER_LENGTHS
                .map(|side| side.map(|length| Allpass::new((length * scale) as usize - 1))),
            shifter: PitchShifter::new(rate),
            low_pass: [Svf::default(); 2],
            high_pass: [Svf::default(); 2],
            low_pass_open: true,
            feedback_filter: [Svf::default(); 2],
            reverb: Reverb::new(rate),
            fed: vec![[0.0; 2]; block],
            fed_at: 0,
            down: Resampler::QUIET,
            up: Resampler::QUIET,
            odd: false,
            ahead: [0.0; 2],
            freeze_fade: 0.0,
            position: Smoothed::default(),
            size: Smoothed::default(),
            semitones: Smoothed::default(),
            density: Smoothed::default(),
            texture: Smoothed::default(),
            blend: Smoothed::default(),
            spread: Smoothed::default(),
            feedback: Smoothed::default(),
            reverb_amount: Smoothed::default(),
            control: Control::default(),
            until_control: 0,
            block,
            scale,
            block_smoothing: 1.0 - (1.0 - smoothing).powf(block as f32),
            playing_up: 0.9,
            playing_down: 0.2,
            grain_gain_smoothing: per_frame(0.01, scale),
            delay_smoothing: per_frame(0.000_05, scale),
            freeze_smoothing: 0.0005,
            smoothing,
            window,
            fade_in: std::array::from_fn(|at| crossfade(at).sin() * FRAC_1_SQRT_2),
            fade_out: std::array::from_fn(|at| crossfade(at).cos() * FRAC_1_SQRT_2),
            rng: Pcg32::new(0, 0),
            rate,
        };
        clouds.start_recording(0);
        clouds.follow(Settings::DEFAULT, 0);
        clouds.settle();
        clouds
    }

    /// How many seconds its recording holds now.
    pub fn seconds(&self) -> f32 {
        self.recording.size as f32 / self.core_rate()
    }

    /// The rate its recording and players run at: its own, or half for the 8-bit qualities.
    fn core_rate(&self) -> f32 {
        if self.low_fidelity() {
            self.rate as f32 * 0.5
        } else {
            self.rate as f32
        }
    }

    fn low_fidelity(&self) -> bool {
        self.quality >> 1 != 0
    }

    fn mono(&self) -> bool {
        self.quality & 1 != 0
    }

    /// How many quiet frames in a row mean nothing's left in it: its recording, its reverb's tail
    /// and its pitch shifter's window. Frozen, what it holds can always come back, so it never is.
    pub fn quiet_frames(&self) -> u32 {
        if self.frozen {
            u32::MAX
        } else {
            ((self.seconds() + 1.5) * self.rate as f32) as u32
        }
    }

    /// Draw its grains' places and pans from a generator of its own, so a song renders the same
    /// each time.
    pub fn seed(&mut self, rng: Pcg32) {
        self.rng = rng;
    }

    pub fn clear(&mut self) {
        self.start_recording(self.quality);
        self.reverb.clear();
        self.shifter.clear();
        for side in &mut self.diffuser {
            for allpass in side {
                allpass.line.fill(0.0);
            }
        }
        self.reset_filters();
        self.fed.fill([0.0; 2]);
        self.freeze_fade = 0.0;
    }

    /// Start a new recording in a quality, with every player starting afresh, as the module does
    /// when its quality changes.
    fn start_recording(&mut self, quality: u8) {
        self.quality = quality.min(3);
        let at = usize::from(self.quality);
        let size = (RECORDING_SAMPLES[at] as f32 * self.scale) as usize;
        self.recording
            .reset(size, if self.mono() { 1 } else { 2 }, self.low_fidelity());
        self.grains = [Grain::IDLE; MOST_GRAINS];
        self.most_grains = GRAINS_OF_QUALITY[at];
        self.playing = 0.0;
        self.grain_gain = 1.0;
        self.size_hint = 1024.0 * self.scale;
        self.since_grain = 0.0;
        self.windows = [Window::IDLE; 2];
        self.window_size = (MOST_WINDOW * 0.5 * self.scale) as i64;
        self.smoothed_semitones = 0.0;
        self.next_ratio = 1.0;
        self.loaded = true;
        self.correlator = Correlator::new();
        self.loop_phase = 0.0;
        self.loop_point = 0.0;
        self.loop_duration = 0.0;
        self.tail_duration = 1.0;
        self.loop_reset = 0.0;
        self.down = Resampler::QUIET;
        self.up = Resampler::QUIET;
        self.odd = false;
        self.ahead = [0.0; 2];
        let block = self.block as f32 * if self.low_fidelity() { 0.5 } else { 1.0 };
        self.playing_up = per_frame(0.9, block);
        self.playing_down = per_frame(0.2, block);
    }

    fn reset_filters(&mut self) {
        for filter in self
            .low_pass
            .iter_mut()
            .chain(&mut self.high_pass)
            .chain(&mut self.feedback_filter)
        {
            filter.reset();
        }
    }

    /// Take what's set now, and move the reverb's wobbles to where they are this many frames in.
    pub fn follow(&mut self, settings: Settings, frames: u64) {
        self.reverb.keep_time(frames);
        if settings.quality.min(3) != self.quality {
            self.start_recording(settings.quality);
            self.reverb.clear();
            for side in &mut self.diffuser {
                for allpass in side {
                    allpass.line.fill(0.0);
                }
            }
        }
        if settings.mode != self.mode {
            // Between these modes the module keeps its recording, and starts its filters and pitch
            // shifter afresh.
            self.mode = settings.mode;
            self.reset_filters();
            self.shifter.clear();
        }
        self.frozen = settings.freeze != 0;
        self.position.target = f32::from(settings.position) / 255.0;
        self.size.target = f32::from(settings.size) / 255.0;
        self.semitones.target = shift_semitones(f32::from(settings.pitch));
        self.density.target = f32::from(settings.density) / 255.0;
        self.texture.target = f32::from(settings.texture) / 255.0;
        self.blend.target = f32::from(settings.blend) / 255.0;
        self.spread.target = f32::from(settings.spread) / 255.0;
        self.feedback.target = f32::from(settings.feedback) / 255.0;
        self.reverb_amount.target = f32::from(settings.reverb) / 255.0;
    }

    /// Move the reverb's wobbles to where they are this many frames in.
    pub fn keep_time(&mut self, frames: u64) {
        self.reverb.keep_time(frames);
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        for setting in [
            &mut self.position,
            &mut self.size,
            &mut self.semitones,
            &mut self.density,
            &mut self.texture,
            &mut self.blend,
            &mut self.spread,
            &mut self.feedback,
            &mut self.reverb_amount,
        ] {
            setting.settle();
        }
        self.freeze_fade = if self.frozen { 1.0 } else { 0.0 };
        self.work_out_control();
        self.until_control = self.block;
        self.reverb.settle();
        self.shifter.settle();
        self.smoothed_semitones = self.control.semitones;
        let longest = self.loop_longest();
        self.delay = self.control.position * longest;
    }

    /// A number from 0 to 1, from its own generator.
    #[inline]
    fn uniform(&mut self) -> f32 {
        self.rng.next_u32() as f32 / 4_294_967_296.0
    }

    /// The block's settings, as the module works them out from its knobs.
    fn work_out_control(&mut self) {
        let mut control = Control {
            position: self.position.value,
            size: self.size.value,
            semitones: self.semitones.value,
            density: self.density.value,
            texture: self.texture.value,
            spread: self.spread.value,
            feedback: self.feedback.value,
            ..Control::default()
        };
        let density = control.density;
        control.regular = density < 0.5;
        control.overlap = if density >= 0.53 {
            (density - 0.53) * 2.12
        } else if density <= 0.47 {
            (0.47 - density) * 2.12
        } else {
            0.0
        };
        let texture = control.texture;
        control.window_shape = if texture < 0.75 { texture * 1.333 } else { 1.0 };
        control.diffusion = if self.mode == CloudsMode::Granular {
            if texture > 0.75 {
                (texture - 0.75) * 4.0
            } else {
                0.0
            }
        } else {
            density
        };

        let feedback = control.feedback;
        control.feedback_gain = feedback * (1.0 - self.freeze_fade);
        // The module works its feedback's filter out against its own rate, which halves for 8
        // bits, though the filter runs at the full rate; so it's an octave higher then.
        let hertz =
            (20.0 + 100.0 * feedback * feedback) * if self.low_fidelity() { 2.0 } else { 1.0 };
        let rate = self.rate as f32;
        for filter in &mut self.feedback_filter {
            filter.set_exact(hertz / rate, 1.0);
        }

        if matches!(self.mode, CloudsMode::Stretch | CloudsMode::Looping) {
            let ratio = |semitones: f32| (semitones / 12.0).exp2();
            let low = (0.5 * ratio(if texture < 0.5 { texture - 0.5 } else { 0.0 } * 216.0))
                .clamp(0.0, 0.499);
            let high = (0.25 * ratio(if texture < 0.5 { -0.5 } else { texture - 1.0 } * 216.0))
                .clamp(0.0, 0.499);
            let resonance = 1.0 + 3.0 * (1.0 - feedback) * (0.5 - low);
            // Where the module's low-pass reaches its top it's open, so it's open here too,
            // rather than at the module's 16 kHz.
            self.low_pass_open = low >= 0.499;
            let to_here = CLOUDS_RATE / rate;
            for filter in &mut self.low_pass {
                filter.set_exact((low * to_here).min(0.49), resonance);
            }
            for filter in &mut self.high_pass {
                filter.set_exact((high * to_here).min(0.49), 1.0);
            }
        }

        let amount = (self.reverb_amount.value * 0.95
            + feedback * (2.0 - feedback) * self.freeze_fade)
            .clamp(0.0, 1.0);
        control.reverb_mix = amount * 0.54;
        self.reverb.tune(
            0.35 + 0.63 * amount,
            0.6 + 0.37 * feedback,
            REVERB_DIFFUSION,
            REVERB_INPUT_GAIN,
        );
        self.control = control;
    }

    /// A block's worth of moving its settings, then working out what they give.
    fn next_block(&mut self) {
        let by = self.block_smoothing;
        for setting in [
            &mut self.position,
            &mut self.size,
            &mut self.semitones,
            &mut self.density,
            &mut self.texture,
            &mut self.spread,
            &mut self.feedback,
            &mut self.reverb_amount,
        ] {
            setting.next(by);
        }
        let frozen = if self.frozen { 1.0 } else { 0.0 };
        self.freeze_fade += self.freeze_smoothing * (frozen - self.freeze_fade);
        self.work_out_control();
    }

    /// Start a grain in the place of an idle one.
    fn start_grain(&mut self, index: usize, high: bool) {
        let control = self.control;
        let size = self.recording.size as f32;
        let mut grain_size = (1024.0 * (control.size * 4.0).exp2()).floor() * self.scale;
        let ratio = (control.semitones / 12.0).exp2();
        let pan = 0.5 + control.spread * (self.uniform() - 0.5);
        let gains = if self.recording.sides == 1 {
            [(pan * FRAC_PI_2).sin(), (pan * FRAC_PI_2).cos()]
        } else if pan < 0.5 {
            [1.0, 2.0 * pan]
        } else {
            [2.0 * (1.0 - pan), 1.0]
        };
        if ratio > 1.0 {
            // Reading faster than it records, it mustn't read past what's being recorded.
            grain_size = grain_size.min(size * 0.25 / ratio);
        }
        let eaten_by_playing = grain_size * ratio;
        let available = size - eaten_by_playing - grain_size;
        let width = (grain_size as i64 & !1).max(2);
        let newest = self.recording.head as i64 - 1;
        let start = newest - (control.position * available + eaten_by_playing) as i64;
        let shape = control.window_shape;
        let first = self.recording.wrapped(start);
        let grain = &mut self.grains[index];
        *grain = Grain {
            active: true,
            first,
            phase: 0.0,
            increment: f64::from(ratio),
            envelope: 0.0,
            envelope_step: 2.0 / width as f32,
            smoothness: if shape >= 0.5 {
                (shape - 0.5) * 2.0
            } else {
                0.0
            },
            slope: if shape >= 0.5 {
                0.0
            } else {
                0.5 / (shape + 0.01)
            },
            gains,
            high,
        };
        self.size_hint += 0.1 * (grain_size - self.size_hint);
    }

    /// A frame of `Granular`'s grains.
    #[inline]
    fn play_grains(&mut self) -> [f32; 2] {
        let control = self.control;
        let overlap = control.overlap * control.overlap * control.overlap;
        let wanted = self.most_grains as f32 * overlap;
        let space = self.size_hint / wanted;
        let chance = wanted / self.size_hint;
        if !control.regular {
            self.since_grain = -1000.0;
        }
        let mut idle = self.grains[..self.most_grains]
            .iter()
            .filter(|grain| !grain.active)
            .count();
        self.since_grain += 1.0;
        let by_chance = !control.regular && self.uniform() < chance && wanted > self.playing;
        let in_time = self.since_grain >= space;
        if idle > 0 && (by_chance || in_time) {
            let index = (0..self.most_grains)
                .rev()
                .find(|at| !self.grains[*at].active)
                .unwrap_or(0);
            idle -= 1;
            let high = idle >= 3 * self.most_grains / 4;
            self.start_grain(index, high);
            self.since_grain = 0.0;
        }

        let recording = &self.recording;
        let window = &self.window;
        let mut out = [0.0_f32; 2];
        for grain in &mut self.grains[..self.most_grains] {
            if !grain.active {
                continue;
            }
            let mut gain = grain.envelope;
            if gain >= 1.0 {
                gain = 2.0 - gain;
            }
            if grain.smoothness > 0.0 {
                if grain.high {
                    let at = gain * WINDOW_POINTS as f32;
                    let whole = (at as usize).min(WINDOW_POINTS - 1);
                    let shaped =
                        window[whole] + (window[whole + 1] - window[whole]) * (at - whole as f32);
                    gain += grain.smoothness * (shaped - gain);
                }
            } else {
                gain = (gain * grain.slope).min(1.0);
            }
            grain.envelope += grain.envelope_step;
            if grain.envelope >= 2.0 {
                grain.active = false;
                continue;
            }
            let whole = grain.phase.floor();
            let t = (grain.phase - whole) as f32;
            let index = grain.first + whole as usize;
            let read = |side| {
                if grain.high {
                    recording.hermite(side, index, t)
                } else {
                    recording.linear(side, index, t)
                }
            };
            let left = read(0) * gain;
            let [gain_left, gain_right] = grain.gains;
            if recording.sides == 1 {
                out[0] += left * gain_left;
                out[1] += left * gain_right;
            } else {
                let right = read(1) * gain;
                out[0] += left * gain_left + right * (1.0 - gain_right);
                out[1] += right * gain_right + left * (1.0 - gain_left);
            }
            grain.phase += grain.increment;
        }

        // As loud as one over the root of how many play, less so for smooth windows overlapping.
        let active = self.grains[..self.most_grains]
            .iter()
            .filter(|grain| grain.active)
            .count() as f32;
        let error = active - self.playing;
        self.playing += error
            * if error > 0.0 {
                self.playing_up
            } else {
                self.playing_down
            };
        let mut gain = if self.playing > 2.0 {
            1.0 / (self.playing - 1.0).sqrt()
        } else {
            1.0
        };
        let window_gain = (1.0 + 2.0 * control.window_shape).clamp(1.0, 2.0);
        gain *= 1.0 + (window_gain - 1.0) * control.overlap;
        self.grain_gain += self.grain_gain_smoothing * (gain - self.grain_gain);
        out.map(|side| side * self.grain_gain)
    }

    /// Start `Stretch`'s next window where the last search found the best match, and set the
    /// next search going.
    fn schedule_window(&mut self, which: usize) {
        let control = self.control;
        let next_position = self.correlator.best_match();
        self.loaded = false;
        let width = self.window_size;
        self.windows[which].start(
            &self.recording,
            next_position - width / 2,
            width,
            self.next_ratio,
        );
        // The pitch moves no more than an octave a window.
        let error = (control.semitones - self.smoothed_semitones).clamp(-12.0, 12.0);
        self.smoothed_semitones += error;
        let ratio = (self.smoothed_semitones / 12.0).exp2();
        self.next_ratio = ratio;

        let factor = ((control.size - 1.0) * 5.0).exp2();
        let wanted = (factor * MOST_WINDOW * self.scale) as i64;
        if (wanted - self.window_size).abs() as f32 > 64.0 * self.scale {
            let error = (wanted - self.window_size).div_euclid(32);
            let size = self.window_size + error;
            self.window_size = (size - size % 4).max(8);
        }
        let width = self.window_size;
        let limit =
            (self.recording.size as i64 - (2.0 * width as f32 / ratio) as i64 - 2 * width).max(0);
        let target = self.recording.head as i64 - (limit as f32 * control.position) as i64 - width;
        self.search_source = next_position;
        self.search_target = target;
        self.load_correlator();
    }

    /// Read the signs of the window just started and of where the next should go, and start
    /// looking for where they match.
    fn load_correlator(&mut self) {
        if self.loaded {
            return;
        }
        let width = self.window_size;
        let stride = (width as f32 / (2048.0 * self.scale)).clamp(1.0, 2.0) * self.scale;
        let increment = f64::from(stride * self.next_ratio.max(1.25));
        let size = sign_bits(
            &self.recording,
            self.search_source,
            width,
            increment,
            &mut self.correlator.source,
        );
        sign_bits(
            &self.recording,
            self.search_target - width,
            width * 2,
            increment,
            &mut self.correlator.target,
        );
        self.correlator.start_search(
            size,
            self.search_target - width + width / 2,
            increment,
            width as usize,
        );
        self.loaded = true;
    }

    /// A frame of `Stretch`'s two windows.
    #[inline]
    fn play_windows(&mut self) -> [f32; 2] {
        if self.windows[0].done && self.windows[1].done {
            self.windows[1].regenerated = true;
            self.schedule_window(0);
        }
        let mut out = [0.0; 2];
        for window in &mut self.windows {
            window.overlap_add(&self.recording, &mut out);
        }
        for which in 0..2 {
            if self.windows[which].needs_regeneration() {
                self.windows[which].regenerated = true;
                self.schedule_window(1 - which);
                self.windows[1 - which].overlap_add(&self.recording, &mut out);
            }
        }
        self.correlator.evaluate_some();
        out
    }

    /// The longest the looping delay goes back.
    fn loop_longest(&self) -> f32 {
        self.recording.size as f32 - LOOP_CROSSFADE * self.scale
    }

    /// A frame of the looping delay, or of its loop while frozen.
    #[inline]
    fn play_loop(&mut self) -> [f32; 2] {
        let control = self.control;
        let longest = self.loop_longest();
        let newest = (self.recording.head as i64 - LOOP_LEAD + self.recording.size as i64) as f32;
        let read = |recording: &Recording, back: f32| {
            let at = (newest - back).max(0.0);
            let whole = at.floor();
            let index = whole as usize;
            let t = at - whole;
            let left = recording.hermite(0, index, t);
            if recording.sides == 1 {
                [left; 2]
            } else {
                [left, recording.hermite(1, index, t)]
            }
        };
        if !self.frozen {
            // A tape's delay: moving `Position` glides it there slowly, bending the pitch.
            let target = control.position * longest;
            self.delay += self.delay_smoothing * (target - self.delay);
            self.loop_phase = 0.0;
            return read(&self.recording, self.delay);
        }
        let mut loop_point = control.position * longest * 15.0 / 16.0 + LOOP_CROSSFADE * self.scale;
        let size = control.size;
        let loop_duration = (0.01 + 0.99 * size * size * size) * longest;
        if loop_point + loop_duration >= longest {
            loop_point = longest - loop_duration;
        }
        let increment = (control.semitones / 12.0).exp2();
        if self.loop_phase >= self.loop_duration || self.loop_phase == 0.0 {
            if self.loop_phase >= self.loop_duration {
                self.loop_reset = self.loop_duration;
            }
            if self.loop_reset >= self.loop_duration {
                self.loop_reset = self.loop_duration;
            }
            self.tail_start = self.loop_duration - self.loop_reset + self.loop_point;
            self.loop_phase = 0.0;
            self.tail_duration = LOOP_CROSSFADE.min(LOOP_CROSSFADE * increment) * self.scale;
            self.loop_point = loop_point;
            self.loop_duration = loop_duration;
        }
        self.loop_phase += increment;
        let gain = if self.tail_duration != 0.0 {
            (self.loop_phase / self.tail_duration).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let back = self.loop_duration - self.loop_phase + self.loop_point;
        let mut out = read(&self.recording, back).map(|side| side * gain);
        if gain != 1.0 {
            let tail = read(&self.recording, self.tail_start - self.loop_phase);
            for (side, tail) in out.iter_mut().zip(tail) {
                *side += tail * (1.0 - gain);
            }
        }
        out
    }

    /// One frame at the recording's rate: record it, then play.
    #[inline]
    fn core(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.recording.write(input, !self.frozen);
        match self.mode {
            CloudsMode::Granular => self.play_grains(),
            CloudsMode::Stretch => self.play_windows(),
            CloudsMode::Looping => self.play_loop(),
        }
    }

    /// Feed one frame in and get what comes out, the sound blended with what Clouds makes of it.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.until_control == 0 {
            self.next_block();
            self.until_control = self.block;
        }
        self.until_control -= 1;
        let control = self.control;

        let mut sound = input;
        if self.mono() {
            let both = (input[0] + input[1]) * 0.5;
            sound = [both; 2];
        }
        // What came out a block ago goes back in, past a high-pass so it can't build up a
        // swing, and through a soft limit.
        let fed = self.fed[self.fed_at];
        let gain = control.feedback_gain;
        for side in 0..2 {
            let back = self.feedback_filter[side].process_all(fed[side]).0;
            sound[side] += gain * (soft_limit(gain * 1.4 * back + sound[side]) - sound[side]);
        }

        let mut out = if self.low_fidelity() {
            self.down.push(sound);
            self.odd = !self.odd;
            if self.odd {
                self.ahead
            } else {
                let halved = self.down.filtered(0, 1);
                let played = self.core(halved);
                self.up.push(played);
                let [first, second] =
                    [0, 1].map(|phase| self.up.filtered(phase, 2).map(|side| side * 2.0));
                self.ahead = second;
                first
            }
        } else {
            self.core(sound)
        };

        // The diffuser, which `Texture` or `Density` turns up.
        for (side, allpasses) in self.diffuser.iter_mut().enumerate() {
            let mut wet = out[side];
            for allpass in allpasses {
                wet = allpass.process(wet, DIFFUSER_SPREAD);
            }
            out[side] += control.diffusion * (wet - out[side]);
        }
        if self.mode == CloudsMode::Looping && !self.frozen {
            let window = (128.0 + 1919.0 * control.size * control.size * control.size) * self.scale;
            self.shifter.aim(control.semitones, window);
            out = self.shifter.process(out);
        }
        if matches!(self.mode, CloudsMode::Stretch | CloudsMode::Looping) {
            for side in 0..2 {
                if !self.low_pass_open {
                    out[side] = self.low_pass[side].low_pass(out[side]);
                }
                out[side] = self.high_pass[side].process_all(out[side]).0;
            }
        }
        self.fed[self.fed_at] = out;
        self.fed_at += 1;
        if self.fed_at == self.fed.len() {
            self.fed_at = 0;
        }

        // The reverb isn't fed back.
        let wet = self.reverb.process_tank(out);
        for (side, wet) in out.iter_mut().zip(wet) {
            *side += (wet - *side) * control.reverb_mix;
        }

        // The module's blend, an equal-power crossfade at 3 dB down into a soft clip; here 3 dB
        // up after it, so the sound alone comes out as it went in.
        let blend = self.blend.next(self.smoothing) * CROSSFADE_POINTS as f32;
        let whole = (blend as usize).min(CROSSFADE_POINTS - 1);
        let t = blend - whole as f32;
        let fade_in = self.fade_in[whole] + (self.fade_in[whole + 1] - self.fade_in[whole]) * t;
        let fade_out = self.fade_out[whole] + (self.fade_out[whole + 1] - self.fade_out[whole]) * t;
        [0, 1].map(|side| {
            let mixed = input[side] * fade_out + out[side] * POST_GAIN * fade_in;
            soft_clip(mixed) * SQRT_2
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    use vt_model::insert::MOST_SHIFT;

    const RATE: u32 = 48_000;

    fn clouds(settings: Settings, rate: u32) -> Clouds {
        let mut clouds = Clouds::new(rate);
        clouds.follow(settings, 0);
        clouds.settle();
        clouds
    }

    fn sine(frame: usize, hertz: f32, rate: u32) -> f32 {
        (frame as f32 / rate as f32 * hertz * TAU).sin() * 0.5
    }

    fn wet(mode: CloudsMode) -> Settings {
        Settings {
            mode,
            blend: 0xFF,
            reverb: 0,
            ..Settings::DEFAULT
        }
    }

    /// How strong a frequency is in a sound, as a sine's amplitude, by a single bin of a DFT over
    /// each 40 ms through a Hann window, on average.
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

    /// The last second of three of a tone through it, on the left.
    fn through(settings: Settings, hertz: f32) -> Vec<f32> {
        let mut clouds = clouds(settings, RATE);
        (0..RATE as usize * 3)
            .map(|frame| clouds.process([sine(frame, hertz, RATE); 2])[0])
            .skip(RATE as usize * 2)
            .collect()
    }

    fn rms(sound: &[f32]) -> f32 {
        (sound.iter().map(|s| s * s).sum::<f32>() / sound.len() as f32).sqrt()
    }

    #[test]
    fn mu_law_reads_back_as_the_modules_table() {
        // A few of `lut_ulaw`'s entries.
        assert_eq!(mu_law_decode(0), -32_124);
        assert_eq!(mu_law_decode(0x7F), 0);
        assert_eq!(mu_law_decode(0x80), 32_124);
        assert_eq!(mu_law_decode(0xB0), 3_900);
        assert_eq!(mu_law_decode(0xEF), 132);
        for pcm in [-32_768, -20_000, -1_000, -3, 0, 5, 700, 12_345, 32_767] {
            let back = mu_law_decode(mu_law_encode(pcm));
            let error = (i32::from(back) - i32::from(pcm)).abs();
            // Within its step, which grows with the level.
            assert!(error <= i32::from(pcm).abs() / 16 + 140, "{pcm} {back}");
        }
    }

    #[test]
    fn each_quality_records_the_modules_seconds_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            for (quality, seconds) in [1.02, 1.86, 4.09, 7.42].into_iter().enumerate() {
                let clouds = clouds(
                    Settings {
                        quality: quality as u8,
                        ..Settings::DEFAULT
                    },
                    rate,
                );
                assert!(
                    (clouds.seconds() - seconds).abs() < 0.01,
                    "{rate} {quality}: {}",
                    clouds.seconds()
                );
            }
        }
    }

    #[test]
    fn every_mode_plays_what_it_recorded() {
        for mode in CloudsMode::ALL {
            let out = through(wet(mode), 330.0);
            let level = rms(&out);
            assert!(level > 0.05 && level < 1.0, "{mode:?}: {level}");
            assert!(strength(&out, 330.0) > 0.05, "{mode:?}");
        }
    }

    #[test]
    fn granular_has_no_grains_with_density_in_the_middle() {
        let out = through(
            Settings {
                density: 0x80,
                ..wet(CloudsMode::Granular)
            },
            330.0,
        );
        assert!(out.iter().all(|s| s.abs() < 1e-6), "{}", rms(&out));
    }

    #[test]
    fn blend_at_none_passes_the_sound_through() {
        let settings = Settings {
            blend: 0,
            ..Settings::DEFAULT
        };
        let mut clouds = clouds(settings, RATE);
        for frame in 0..RATE as usize {
            let sample = sine(frame, 440.0, RATE) * 0.2;
            let out = clouds.process([sample; 2]);
            // The module's soft clip bends a quiet sound by well under a percent.
            assert!((out[0] - sample).abs() < 0.002, "{frame}: {out:?} {sample}");
        }
    }

    #[test]
    fn pitch_shifts_each_mode_an_octave() {
        // The looping delay's shifter jumps back half its window, 32 ms at its longest, so an
        // octave that fits that a whole number of times comes out clean, and anything else
        // wobbles in phase as it does in `Pitch`.
        let window = 2047.0 / CLOUDS_RATE;
        let octave_hertz = (880.0 * window * 0.5_f32).round() * 2.0 / window;
        for mode in CloudsMode::ALL {
            let up = through(
                Settings {
                    pitch: MOST_SHIFT + 12,
                    size: 0xFF,
                    spread: 0,
                    texture: 0x80,
                    density: if mode == CloudsMode::Granular {
                        0xF0
                    } else {
                        0
                    },
                    ..wet(mode)
                },
                octave_hertz / 2.0,
            );
            let (octave, left) = (
                strength(&up, octave_hertz),
                strength(&up, octave_hertz / 2.0),
            );
            assert!(octave > 0.05, "{mode:?}: {octave}");
            assert!(left < octave * 0.3, "{mode:?}: {octave} {left}");
        }
    }

    #[test]
    fn frozen_every_mode_keeps_playing_after_the_sound_stops() {
        for mode in CloudsMode::ALL {
            let mut free = clouds(wet(mode), RATE);
            for frame in 0..RATE as usize * 2 {
                free.process([sine(frame, 330.0, RATE); 2]);
            }
            let mut frozen = free.clone();
            frozen.follow(
                Settings {
                    freeze: 1,
                    ..wet(mode)
                },
                0,
            );
            let after = |clouds: &mut Clouds| -> Vec<f32> {
                (0..RATE as usize * 3)
                    .map(|_| clouds.process([0.0; 2])[0])
                    .collect()
            };
            let (held, gone) = (after(&mut frozen), after(&mut free));
            let late = &held[RATE as usize * 2..];
            assert!(
                strength(late, 330.0) > 0.03,
                "{mode:?}: {}",
                strength(late, 330.0)
            );
            assert!(
                gone[RATE as usize * 2..].iter().all(|s| s.abs() < 1e-3),
                "{mode:?}"
            );
            assert_eq!(frozen.quiet_frames(), u32::MAX);
        }
    }

    #[test]
    fn the_same_seed_plays_the_same_and_another_differs() {
        let play = |seed: u64| -> Vec<f32> {
            let mut clouds = clouds(Settings::DEFAULT, RATE);
            clouds.seed(Pcg32::new(seed, 3));
            (0..RATE as usize)
                .map(|frame| clouds.process([sine(frame, 220.0, RATE); 2])[0])
                .collect()
        };
        assert_eq!(play(1), play(1));
        assert_ne!(play(1), play(2));
    }

    #[test]
    fn the_8_bit_qualities_are_band_limited_as_the_modules() {
        // A tone over a quarter of the rate is above what the half rate holds.
        let level = |quality: u8| {
            let out = through(
                Settings {
                    quality,
                    mode: CloudsMode::Looping,
                    ..wet(CloudsMode::Looping)
                },
                14_000.0,
            );
            rms(&out)
        };
        assert!(level(0) > 0.1, "{}", level(0));
        assert!(level(2) < level(0) * 0.05, "{} {}", level(2), level(0));
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        // `cargo test --release` plays every mode, quality, rate and corner. A test build, with its
        // overflow checks, takes each mode and quality once at a rate of their own, with every
        // corner.
        let all = !cfg!(debug_assertions);
        let rates = [44_100, 192_000];
        for (m, mode) in CloudsMode::ALL.into_iter().enumerate() {
            for quality in 0..4_u8 {
                for (r, rate) in rates.into_iter().enumerate() {
                    if !all && (m + usize::from(quality) + r) % 2 == 1 {
                        continue;
                    }
                    for (low, high) in [(0x00, 0x00), (0xFF, 0xFF), (0x00, 0xFF), (0xFF, 0x00)] {
                        let settings = Settings {
                            mode,
                            quality,
                            position: low,
                            size: high,
                            pitch: if low == 0 { 0 } else { MOST_SHIFT * 2 },
                            density: high,
                            texture: low,
                            blend: 0xFF,
                            spread: high,
                            feedback: 0xFF,
                            reverb: high,
                            ..Settings::DEFAULT
                        };
                        let mut clouds = clouds(settings, rate);
                        let quarter = rate as usize / 4;
                        for frame in 0..rate as usize * 2 {
                            if frame == rate as usize {
                                clouds.follow(
                                    Settings {
                                        freeze: 1,
                                        ..settings
                                    },
                                    0,
                                );
                            }
                            // Full-scale noise, a full-scale square, DC and silence.
                            let level = match frame / quarter % 4 {
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
                            let out = clouds.process([level, -level * 0.5]);
                            // What it made, before the reverb and the blend's soft clip, which
                            // would hide its running away.
                            let fed = clouds.fed
                                [(clouds.fed_at + clouds.fed.len() - 1) % clouds.fed.len()];
                            assert!(
                                fed.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                                "{settings:?} {rate}: {fed:?}"
                            );
                            assert!(
                                out.iter()
                                    .all(|s| s.is_finite() && s.abs() <= SQRT_2 * 1.001),
                                "{settings:?} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        for mode in CloudsMode::ALL {
            let mut clouds = clouds(wet(mode), RATE);
            for frame in 0..RATE as usize {
                clouds.process([sine(frame, 440.0, RATE); 2]);
            }
            clouds.clear();
            for _ in 0..RATE {
                assert_eq!(clouds.process([0.0, 0.0]), [0.0, 0.0], "{mode:?}");
            }
        }
    }
}
