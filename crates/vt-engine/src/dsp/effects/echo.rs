//! A Space Echo: a Roland RE-201's tape loop, written from what the machine does and fitted to
//! Arturia's Delay TAPE-201 by its output alone (decision 337). What comes in is driven onto the
//! tape through a saturation; three playback heads, the second and third two and three times as
//! far from the record head as the first, read it back; the mode chooses which heads are heard;
//! and `Intensity` sends what they play back to the record head, so each repeat goes through the
//! tape again, darker and more saturated than the last, until from about `DC` it runs away.
//!
//! The tape's speed is the delay: `Rate`, or a note division at the song's tempo, sets the first
//! head's, and the motor comes to a new speed through two one-poles, the slower of them
//! `Inertia`'s, so a repeat already on the tape is read faster or slower and bends in pitch as
//! the machine's does. The capstan's wobble moves every head's read together, at a rate that goes
//! with the tape's speed, by as much in pitch at any speed, with a slow drift seeded from the track.
//!
//! The spring reverb is this program's `Spring`, ChowDSP's model (decision 206), at one setting,
//! fed from what comes in. `Bass` and `Treble` are shelves on what comes out, as the RE-201's tone
//! controls are on its echo and reverb, not in the loop.

use std::f64::consts::TAU;

use vt_model::insert::{
    ECHO_BASS, ECHO_HEAD_SECONDS, ECHO_TREBLE, Echo as Settings, MOST_INTENSITY,
    Spring as SpringSettings, echo_head_seconds, echo_input_db, echo_mode, from_middle,
};
use vt_model::lfo::DIVISION_TICKS;
use vt_model::song::level_gain;

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::line::Tape;
use crate::dsp::blocks::response::{Gain, one_pole, svf, warped};
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::delay::{MOST_TAPS, Repeats, Tap};
use crate::dsp::effects::memory::SongPlace;
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::spring::Spring;
use crate::rng::Pcg32;

/// The longest the first head can be behind the record head, in seconds: `Rate`'s slowest is a
/// second, and `Sync` can ask for up to twice that at a slow tempo.
const LONGEST_HEAD_SECONDS: f64 = 2.0;
/// The most the wobble moves a read either way, in seconds, at the slowest speed, with room.
const MOST_WOBBLE_SECONDS: f64 = 0.04;

/// The playback head's low-pass, the high-pass on what comes out, and the loop's high-pass and
/// low-pass, each fitted to TAPE-201's first repeat and to what each trip round its loop takes,
/// with the tape at 400 ms. The head's opens as the tape runs faster, as a tape's highs do: a
/// little at 150 ms, to about 8.5 kHz at 100 and out of hearing by 50, `(1 + (s / d)^p)` times
/// its own for a first head `d` seconds behind.
const HEAD_LOW_PASS: (f32, f32) = (4345.0, 0.469);
const HEAD_OPENS: (f64, f64) = (0.1016, 5.9);
const OUT_HIGH_PASS: (f32, f32) = (102.0, 0.531);
const LOOP_HIGH_PASS: (f32, f32) = (92.8, 0.558);
const LOOP_LOW_PASS_HZ: f32 = 3776.0;
/// How loud a quiet 1 kHz sine comes back from a head at full `Echo`, in dB, as TAPE-201's does.
const ECHO_DB_AT_1K: f64 = -5.38;
/// How much of what the heads play goes round at an `Intensity` of 1, on TAPE-201's scale, through
/// the loop's filters, set so it runs away where TAPE-201's does: a click at 1.0 dies away by
/// 2.3 dB a second at 150 ms, and grows from about 1.03. Each trip at 0.8 then takes 2.2 dB where
/// it passes most, where TAPE-201's band measurements say 2.6.
const LOOP_GAIN: f32 = 1.0156;
/// Where the saturation on the record head holds what's written: `C tanh(x / C)`, which with the
/// head's gain lets a 1 kHz sine out at no more than -4.2 dBFS, as TAPE-201's does.
const SATURATION: f32 = 0.9;
/// The most that goes onto the saturation, so a NaN or anything huge is held there first.
const LOUDEST_IN: f32 = SATURATION * 64.0;

/// How fast the capstan wobbles at a first head of 150 ms, how that goes with the tape's speed,
/// and its shape: its harmonics, by their number, and how far each bends the pitch at full `Wow`,
/// in cents, at their peaks. TAPE-201 measured from a held sine: 2.26 Hz at 150 ms, 7.6 at 50 and
/// 0.5 at 500, with harmonics 2, 4 and 6 over it, 21 cents RMS at its full `Flutter`, falling to
/// 15 at 500 ms and 13 at 50, which a bell over the log of the delay, `WOBBLE_WIDTH` wide, follows.
const WOBBLE_HZ: f64 = 2.26;
const WOBBLE_AT_SECONDS: f64 = 0.15;
const WOBBLE_SPEED_POWER: f64 = 1.2;
const WOBBLE_WIDTH: f64 = 1.8;
const WOBBLE_HARMONICS: [(f64, f64); 4] = [(1.0, 28.14), (2.0, 2.85), (4.0, 5.01), (6.0, 5.45)];
/// How far the wobble's rate drifts at random, as a share, and how often it heads somewhere new.
const DRIFT: f64 = 0.04;
const DRIFT_SECONDS: f64 = 1.5;
/// The fast and slow one-poles the motor comes to a new speed through, in seconds: TAPE-201's
/// fast one, and its slow one at `Motor Inertia` 0 and 1, which goes evenly between them.
const MOTOR_FAST_SECONDS: f64 = 0.013;
const MOTOR_SLOW_SECONDS: (f64, f64) = (0.19, 2.11);

/// The tone controls, the model's, so the page shows what they do.
const BASS: (f32, f32, f32) = ECHO_BASS;
const TREBLE: (f32, f32, f32) = ECHO_TREBLE;

/// The spring tank the reverb is, set once, how long before it's heard, and how loud.
const SPRING: SpringSettings = SpringSettings {
    decay: 0xF8,
    size: 0xA0,
    damp: 0x80,
    spin: 0x80,
    reflect: 0xFF,
    chaos: 0x00,
    knock: 0,
    mix: 0xFF,
    gain: 0xFF,
};
const REVERB_PRE_SECONDS: f64 = 0.0;
const REVERB_GAIN: f32 = 0.484;
/// How long the reverb rings, for knowing when the slot is quiet, in seconds.
const REVERB_QUIET_SECONDS: f64 = 4.0;
/// How often the tone controls' shelves are worked out again while they move, in frames.
const BLOCK: u32 = 32;

/// The first head's delay a Space Echo's settings ask for, in frames, at a tempo.
fn head_frames(settings: Settings, frames_per_tick: f64, rate: u32) -> f64 {
    let rate = f64::from(rate);
    let seconds = if settings.sync == 0 {
        f64::from(echo_head_seconds(f32::from(settings.rate)))
    } else {
        let at = usize::from(settings.sync - 1).min(DIVISION_TICKS.len() - 1);
        f64::from(DIVISION_TICKS[at]) * frames_per_tick / rate
    };
    seconds.clamp(f64::from(ECHO_HEAD_SECONDS.0), LONGEST_HEAD_SECONDS) * rate
}

/// The share each head is heard at in a mode: the same for each one playing, so a mode of two or
/// three heads is as loud as one, as TAPE-201's are.
fn head_weights(mode: u8) -> ([f32; 3], bool) {
    let (heads, reverb) = echo_mode(mode);
    let playing = heads.iter().filter(|on| **on).count().max(1) as f32;
    (heads.map(|on| if on { 1.0 / playing } else { 0.0 }), reverb)
}

/// A shelf's one-pole cutoff and what it takes of the low-pass and the high-pass, for a tone
/// control at `k`, from -1 to 1.
fn shelf(k: f32, (corner, most_db, power): (f32, f32, f32), treble: bool) -> (f32, f32, f32) {
    let g = 10_f32.powf(most_db * k.abs().powf(power) / 20.0);
    match (treble, k >= 0.0) {
        (true, true) => (corner * g, 1.0, g),
        (true, false) => (corner, 1.0, 1.0 / g),
        (false, true) => (corner, g, 1.0),
        (false, false) => (corner * g, 1.0 / g, 1.0),
    }
}

/// A trapezoidal state-variable filter's tuning.
#[derive(Debug, Clone, Copy)]
struct Tuning {
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl Tuning {
    fn new(hertz: f32, q: f32, rate: u32) -> Self {
        let g = (std::f32::consts::PI * (hertz / rate as f32).min(0.49)).tan();
        let k = 1.0 / q;
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        Self {
            k,
            a1,
            a2,
            a3: g * a2,
        }
    }
}

/// A state-variable filter's state.
#[derive(Debug, Clone, Copy, Default)]
struct Svf {
    s1: f32,
    s2: f32,
}

impl Svf {
    /// Its low-pass and high-pass.
    #[inline]
    fn process(&mut self, x: f32, t: Tuning) -> (f32, f32) {
        let v3 = x - self.s2;
        let v1 = t.a1 * self.s1 + t.a2 * v3;
        let v2 = self.s2 + t.a2 * self.s1 + t.a3 * v3;
        self.s1 = 2.0 * v1 - self.s1;
        self.s2 = 2.0 * v2 - self.s2;
        (v2, x - t.k * v1 - v2)
    }
}

/// A trapezoidal one-pole's `g / (1 + g)`.
fn one_pole_step(hertz: f32, rate: u32) -> f32 {
    let g = (std::f32::consts::PI * (hertz / rate as f32).min(0.49)).tan();
    g / (1.0 + g)
}

/// A one-pole low-pass's state.
#[derive(Debug, Clone, Copy, Default)]
struct OnePole {
    s: f32,
}

impl OnePole {
    #[inline]
    fn process(&mut self, x: f32, step: f32) -> f32 {
        let v = (x - self.s) * step;
        let low = v + self.s;
        self.s = low + v;
        low
    }
}

/// A shelf's tuning: its one-pole and how much of each half it takes.
#[derive(Debug, Clone, Copy)]
struct ShelfTuning {
    step: f32,
    low: f32,
    high: f32,
}

impl ShelfTuning {
    fn new(k: f32, control: (f32, f32, f32), treble: bool, rate: u32) -> Self {
        let (hertz, low, high) = shelf(k, control, treble);
        Self {
            step: one_pole_step(hertz, rate),
            low,
            high,
        }
    }
}

/// One side's filters.
#[derive(Debug, Clone, Copy, Default)]
struct Side {
    head: Svf,
    out: Svf,
    loop_high: Svf,
    loop_low: OnePole,
    bass: OnePole,
    treble: OnePole,
}

/// How many frames the wobble's offset moves in a straight line to the next one worked out.
const WOBBLE_FRAMES: u32 = 16;

/// The capstan's wobble: where it is in its turn, its harmonics' phases, and its drift; and the
/// offset it gives the reads, worked out every `WOBBLE_FRAMES` frames and followed in a straight
/// line between, as it moves slowly and its harmonics' cosines cost.
#[derive(Debug, Clone)]
struct Wobble {
    phase: f64,
    phases: [f64; 4],
    drift: f64,
    drift_target: f64,
    drift_countdown: u32,
    offset: f64,
    slope: f64,
    countdown: u32,
    rng: Pcg32,
    /// The generator as it was given, to start again from when the tape's cleared.
    seed: Pcg32,
}

impl Wobble {
    fn new(rng: Pcg32) -> Self {
        let mut wobble = Self {
            phase: 0.0,
            phases: [0.0; 4],
            drift: 0.0,
            drift_target: 0.0,
            drift_countdown: 0,
            offset: 0.0,
            slope: 0.0,
            countdown: 0,
            rng,
            seed: rng,
        };
        for at in 0..4 {
            wobble.phases[at] = wobble.random();
        }
        wobble.drift = (wobble.random() * 2.0 - 1.0) * DRIFT;
        wobble.drift_target = wobble.drift;
        wobble
    }

    fn random(&mut self) -> f64 {
        f64::from(self.rng.next_u32()) / 4_294_967_296.0
    }

    /// How fast it turns at a first head this many seconds behind.
    fn hertz(head_seconds: f64) -> f64 {
        WOBBLE_HZ * (WOBBLE_AT_SECONDS / head_seconds.max(0.01)).powf(WOBBLE_SPEED_POWER)
    }

    /// How far it moves the reads `WOBBLE_FRAMES` frames on, in frames, at a depth from 0 to 1,
    /// with the first head this many frames behind. Each harmonic moves them by what bends the
    /// pitch by its cents, worked out from its rate, so a faster tape wobbles faster.
    fn ahead(&mut self, head_frames: f64, depth: f64, rate: f64) -> f64 {
        let frames = f64::from(WOBBLE_FRAMES);
        if self.drift_countdown < WOBBLE_FRAMES {
            self.drift_countdown = (DRIFT_SECONDS * rate) as u32;
            self.drift_target = (self.random() * 2.0 - 1.0) * DRIFT;
        }
        self.drift_countdown -= WOBBLE_FRAMES;
        self.drift += (self.drift_target - self.drift) * frames / (DRIFT_SECONDS * rate);
        let seconds = head_frames / rate;
        let hertz = Self::hertz(seconds) * (1.0 + self.drift);
        self.phase += hertz * frames / rate;
        self.phase -= self.phase.floor();
        if depth <= 0.0 {
            return 0.0;
        }
        let cents = std::f64::consts::LN_2 / 1200.0;
        let bell = (-((seconds / WOBBLE_AT_SECONDS).ln() / WOBBLE_WIDTH).powi(2)).exp();
        WOBBLE_HARMONICS
            .iter()
            .zip(self.phases)
            .map(|((harmonic, size), phase)| {
                let turn = TAU * (harmonic * self.phase + phase);
                size * cents / (TAU * harmonic * hertz) * rate * turn.cos()
            })
            .sum::<f64>()
            * depth
            * bell
    }

    /// How far it moves the reads this frame, in frames.
    #[inline]
    fn next(&mut self, head_frames: f64, depth: f64, rate: f64) -> f64 {
        if self.countdown == 0 {
            self.countdown = WOBBLE_FRAMES;
            let ahead = self.ahead(head_frames, depth, rate);
            self.slope = (ahead - self.offset) / f64::from(WOBBLE_FRAMES);
        }
        self.countdown -= 1;
        self.offset += self.slope;
        self.offset
    }
}

/// A Space Echo's tape, heads, loop and spring.
#[derive(Debug, Clone)]
pub struct Echo {
    tape: Tape,
    used: bool,
    sides: [Side; 2],
    /// The first head's delay the settings ask for, and where the motor's fast and slow one-poles
    /// have it now, in frames, with how far the slow one goes each frame.
    target: f64,
    fast: f64,
    slow: f64,
    fast_step: f64,
    slow_step: f64,
    wobble: Wobble,
    /// The share each head is heard at, and the reverb's, gliding when the mode changes.
    heads: [Smoothed; 3],
    reverb_on: Smoothed,
    drive: Smoothed,
    intensity: Smoothed,
    wow: Smoothed,
    bass: Smoothed,
    treble: Smoothed,
    echo: Smoothed,
    reverb: Smoothed,
    head_tuning: Tuning,
    /// The first head's delay the head's low-pass was last worked out for, in frames.
    head_tuned: f64,
    out_tuning: Tuning,
    loop_high_tuning: Tuning,
    loop_low_step: f32,
    bass_tuning: ShelfTuning,
    treble_tuning: ShelfTuning,
    /// The tone controls' places the shelves were last worked out for.
    tuned: [f32; 2],
    /// The head's gain, so a quiet 1 kHz sine comes back as loud as TAPE-201's.
    echo_gain: f32,
    spring: Box<Spring>,
    pre: Vec<[f32; 2]>,
    pre_at: usize,
    countdown: u32,
    smoothing: f32,
    rate: u32,
}

impl Echo {
    /// A Space Echo for a rate, its tape long enough for the slowest speed. Its memory is touched
    /// here, so the callback doesn't wait for it to be paged in.
    pub fn new(rate: u32) -> Self {
        let frames = |seconds: f64| (seconds * f64::from(rate)).ceil() as usize;
        let length = frames(LONGEST_HEAD_SECONDS * 3.0 + MOST_WOBBLE_SECONDS) + 8;
        let tape = Tape::new(length);
        let mut spring = Box::new(Spring::new(rate));
        spring.follow(SPRING);
        spring.settle();
        let start = head_frames(Settings::DEFAULT, 1.0, rate);
        let mut echo = Self {
            tape,
            used: false,
            sides: [Side::default(); 2],
            target: start,
            fast: start,
            slow: start,
            fast_step: 1.0 - (-1.0 / (MOTOR_FAST_SECONDS * f64::from(rate))).exp(),
            slow_step: 0.0,
            wobble: Wobble::new(Pcg32::new(0, 0)),
            heads: [Smoothed::at(0.0); 3],
            reverb_on: Smoothed::at(0.0),
            drive: Smoothed::at(1.0),
            intensity: Smoothed::at(0.0),
            wow: Smoothed::at(0.0),
            bass: Smoothed::at(0.0),
            treble: Smoothed::at(0.0),
            echo: Smoothed::at(1.0),
            reverb: Smoothed::at(0.0),
            head_tuning: Tuning::new(head_hertz(start, rate), HEAD_LOW_PASS.1, rate),
            head_tuned: start,
            out_tuning: Tuning::new(OUT_HIGH_PASS.0, OUT_HIGH_PASS.1, rate),
            loop_high_tuning: Tuning::new(LOOP_HIGH_PASS.0, LOOP_HIGH_PASS.1, rate),
            loop_low_step: one_pole_step(LOOP_LOW_PASS_HZ, rate),
            bass_tuning: ShelfTuning::new(0.0, BASS, false, rate),
            treble_tuning: ShelfTuning::new(0.0, TREBLE, true, rate),
            tuned: [0.0; 2],
            echo_gain: echo_gain(rate) as f32,
            spring,
            pre: vec![[0.0; 2]; frames(REVERB_PRE_SECONDS).max(1)],
            pre_at: 0,
            countdown: 0,
            smoothing: coefficient(rate),
            rate,
        };
        echo.follow(Settings::DEFAULT, 1.0);
        echo.settle();
        echo
    }

    /// Forget what's on the tape and in the spring.
    pub(crate) fn clear(&mut self) {
        if self.used {
            self.tape.clear();
            self.pre.fill([0.0; 2]);
            self.used = false;
        }
        self.sides = [Side::default(); 2];
        self.spring.clear();
        self.spring.follow(SPRING);
        self.spring.settle();
        self.wobble = Wobble::new(self.wobble.seed);
    }

    /// Draw the wobble's phases and drift from a generator of the track's, so a render repeats.
    pub fn seed(&mut self, rng: Pcg32) {
        self.wobble = Wobble::new(rng);
    }

    /// Take what's set now, at a tempo.
    pub fn follow(&mut self, settings: Settings, frames_per_tick: f64) {
        let rate = f64::from(self.rate);
        let longest = (self.tape.len() - 8) as f64 / 3.0 - MOST_WOBBLE_SECONDS * rate;
        self.target = head_frames(settings, frames_per_tick, self.rate).min(longest);
        let share = |value: u8| f64::from(value) / 255.0;
        let (slowest, slower) = MOTOR_SLOW_SECONDS;
        let seconds = slowest + (slower - slowest) * share(settings.inertia);
        self.slow_step = 1.0 - (-1.0 / (seconds * rate)).exp();
        let (weights, reverb) = head_weights(settings.mode);
        for (head, weight) in self.heads.iter_mut().zip(weights) {
            head.target = weight;
        }
        self.reverb_on.target = if reverb { 1.0 } else { 0.0 };
        self.drive.target = 10_f32.powf(echo_input_db(f32::from(settings.input)) / 20.0);
        self.intensity.target = f32::from(settings.intensity) / 255.0 * MOST_INTENSITY;
        self.wow.target = f32::from(settings.wow) / 255.0;
        self.bass.target = from_middle(f32::from(settings.bass));
        self.treble.target = from_middle(f32::from(settings.treble));
        self.echo.target = f32::from(settings.echo) / 255.0;
        self.reverb.target = f32::from(settings.reverb) / 255.0;
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.fast = self.target;
        self.slow = self.target;
        for head in &mut self.heads {
            head.settle();
        }
        self.reverb_on.settle();
        self.drive.settle();
        self.intensity.settle();
        self.wow.settle();
        self.bass.settle();
        self.treble.settle();
        self.echo.settle();
        self.reverb.settle();
        self.tune();
        self.tune_head();
    }

    /// The head's low-pass, for the tape's speed now.
    fn tune_head(&mut self) {
        self.head_tuned = self.slow;
        self.head_tuning =
            Tuning::new(head_hertz(self.slow, self.rate), HEAD_LOW_PASS.1, self.rate);
    }

    /// The tone controls' shelves, from where they are now.
    fn tune(&mut self) {
        self.tuned = [self.bass.value, self.treble.value];
        self.bass_tuning = ShelfTuning::new(self.bass.value, BASS, false, self.rate);
        self.treble_tuning = ShelfTuning::new(self.treble.value, TREBLE, true, self.rate);
    }

    /// How far behind the record head the first head is now, in frames.
    #[cfg(test)]
    pub(crate) fn frames(&self) -> f64 {
        self.slow
    }

    /// How many quiet frames in a row mean nothing's left in it: the third head's delay and the
    /// most the wobble moves it, and the spring's ring.
    pub(crate) fn quiet_frames(&self) -> u32 {
        let rate = f64::from(self.rate);
        (self.slow.max(self.target) * 3.0 + (MOST_WOBBLE_SECONDS + REVERB_QUIET_SECONDS) * rate)
            as u32
    }

    /// Feed one frame in and get what's wet out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let smoothing = self.smoothing;
        let drive = self.drive.next(smoothing);
        let intensity = self.intensity.next(smoothing) * LOOP_GAIN;
        let wow = self.wow.next(smoothing);
        let echo = self.echo.next(smoothing) * self.echo_gain;
        let reverb = self.reverb.next(smoothing) * self.reverb_on.next(smoothing) * REVERB_GAIN;
        let heads = [
            self.heads[0].next(smoothing),
            self.heads[1].next(smoothing),
            self.heads[2].next(smoothing),
        ];
        self.bass.next(smoothing);
        self.treble.next(smoothing);
        if self.countdown == 0 {
            self.countdown = BLOCK;
            if self.tuned != [self.bass.value, self.treble.value] {
                self.tune();
            }
            if (self.slow / self.head_tuned - 1.0).abs() > 1e-3 {
                self.tune_head();
            }
        }
        self.countdown -= 1;

        // The motor comes to its speed, and the capstan wobbles every head together.
        self.fast += (self.target - self.fast) * self.fast_step;
        self.slow += (self.fast - self.slow) * self.slow_step;
        let rate = f64::from(self.rate);
        let wobble = self.wobble.next(self.slow, f64::from(wow), rate);

        let reads = [
            self.slow + wobble,
            self.slow * 2.0 + wobble,
            self.slow * 3.0 + wobble,
        ];
        let mut played = [0.0; 2];
        for (side, sample) in played.iter_mut().enumerate() {
            let mut heard = 0.0;
            for (weight, frames) in heads.iter().zip(reads) {
                if *weight > 0.0 {
                    heard += weight * self.tape.read(side, frames);
                }
            }
            // A NaN read off the tape is held here, so it can't stay in the head's filters.
            *sample = within(heard, -LOUDEST_IN, LOUDEST_IN);
        }

        let mut out = [0.0; 2];
        let mut written = [0.0; 2];
        let pre = self.pre[self.pre_at];
        for side in 0..2 {
            let filters = &mut self.sides[side];
            let (head, _) = filters.head.process(played[side], self.head_tuning);
            let (_, repeat) = filters.out.process(head, self.out_tuning);
            let (_, looped) = filters.loop_high.process(head, self.loop_high_tuning);
            let looped = filters.loop_low.process(looped, self.loop_low_step);
            let recorded = within(
                input[side] * drive + looped * intensity,
                -LOUDEST_IN,
                LOUDEST_IN,
            );
            written[side] = SATURATION * (recorded / SATURATION).tanh();
            out[side] = repeat * echo;
        }
        self.tape.push(written);
        self.pre[self.pre_at] = [input[0] * drive, input[1] * drive];
        self.pre_at = (self.pre_at + 1) % self.pre.len();
        self.used = true;

        let springs = if reverb > 0.0 {
            self.spring.process(pre)
        } else {
            [0.0; 2]
        };
        let (bass, treble) = (self.bass_tuning, self.treble_tuning);
        for side in 0..2 {
            let filters = &mut self.sides[side];
            let wet = out[side] + springs[side] * reverb;
            let low = filters.bass.process(wet, bass.step);
            let wet = low * bass.low + (wet - low) * bass.high;
            let low = filters.treble.process(wet, treble.step);
            out[side] = low * treble.low + (wet - low) * treble.high;
        }
        out
    }
}

/// A trapezoidal state-variable filter's low-pass and high-pass at a frequency, in cycles a frame.
fn svf_at(turn: f64, (hertz, q): (f32, f32), rate: u32) -> [Gain; 2] {
    let w = warped(turn) / warped(f64::from(hertz) / f64::from(rate));
    let [low, _, high] = svf(w, 1.0 / f64::from(q));
    [low, high]
}

/// The head's low-pass cutoff with the first head this many frames behind, in Hz.
fn head_hertz(frames: f64, rate: u32) -> f32 {
    let seconds = frames / f64::from(rate);
    let (opens, power) = HEAD_OPENS;
    let hertz = f64::from(HEAD_LOW_PASS.0) * (1.0 + (opens / seconds).powf(power));
    hertz.min(f64::from(rate) * 0.45) as f32
}

/// What the playback head and the high-pass on what comes out do to a sine, and what a trip round
/// the loop from the head to the record head does, at a frequency in cycles a frame, with the
/// first head this many frames behind.
fn path_at(turn: f64, rate: u32, frames: f64) -> (Gain, Gain) {
    let [head, _] = svf_at(turn, (head_hertz(frames, rate), HEAD_LOW_PASS.1), rate);
    let [_, out] = svf_at(turn, OUT_HIGH_PASS, rate);
    let [_, loop_high] = svf_at(turn, LOOP_HIGH_PASS, rate);
    let w = warped(turn) / warped(f64::from(LOOP_LOW_PASS_HZ) / f64::from(rate));
    let [loop_low, _] = one_pole(w);
    (head.times(out), head.times(loop_high).times(loop_low))
}

/// The head's gain, so a quiet 1 kHz sine comes back `ECHO_DB_AT_1K` down with the tape at
/// 100 ms, where TAPE-201's was measured.
fn echo_gain(rate: u32) -> f64 {
    let (out, _) = path_at(1000.0 / f64::from(rate), rate, 0.1 * f64::from(rate));
    10_f64.powf(ECHO_DB_AT_1K / 20.0) / out.size()
}

/// What the tone controls do at a frequency.
fn tone_at(turn: f64, bass: f32, treble: f32, rate: u32) -> Gain {
    let one = |k: f32, control: (f32, f32, f32), is_treble: bool| {
        let (hertz, low, high) = shelf(k, control, is_treble);
        let w = warped(turn) / warped(f64::from(hertz) / f64::from(rate));
        let [lp, hp] = one_pole(w);
        lp.scale(f64::from(low)).plus(hp.scale(f64::from(high)))
    };
    one(bass, BASS, false).times(one(treble, TREBLE, true))
}

/// Where the repeats are measured: the frequency, among 192 from 20 Hz to 20 kHz, where a trip
/// round the loop keeps the most, as a sine there dies away last.
fn loudest_hertz(rate: u32, frames: f64) -> f32 {
    (0..=192)
        .map(|step| 20.0 * 1000_f64.powf(f64::from(step) / 192.0))
        .filter(|hertz| *hertz < f64::from(rate) * 0.45)
        .fold((1000.0, 0.0), |best, hertz| {
            let (_, round) = path_at(hertz / f64::from(rate), rate, frames);
            if round.size() > best.1 {
                (hertz, round.size())
            } else {
                best
            }
        })
        .0 as f32
}

/// A Space Echo's repeats at its settings and a tempo, for the FX page's zoom, as
/// [`Echo::process`] makes them for a quiet sound the same on both sides once its settings have
/// settled, measured where the loop keeps the most: each comes when the tape brings it to a head,
/// a whole number of times the first head's delay, and is every way round the heads the mode
/// plays that adds up to that time, each trip through the loop's filters and `Intensity`, the
/// last through the head's and the tone's. Its wow bends them by cents and moves them by a few
/// milliseconds, so they're drawn without it, and so is the reverb.
pub fn repeats(settings: Settings, frames_per_tick: f64, rate: u32) -> Repeats {
    let frames = head_frames(settings, frames_per_tick, rate);
    let hertz = loudest_hertz(rate, frames);
    let turn = f64::from(hertz) / f64::from(rate);
    let (out, round) = path_at(turn, rate, frames);
    let (weights, _) = head_weights(settings.mode);
    let intensity = f64::from(settings.intensity) / 255.0 * f64::from(MOST_INTENSITY * LOOP_GAIN);
    let tone = tone_at(
        turn,
        from_middle(f32::from(settings.bass)),
        from_middle(f32::from(settings.treble)),
        rate,
    );
    let mix = f64::from(settings.mix) / 255.0;
    let gain = f64::from(level_gain(f32::from(settings.gain)));
    let level = echo_gain(rate) * f64::from(settings.echo) / 255.0 * mix * gain;
    // What reaches the record head at each whole number of the first head's delays: the sound at
    // 0, and what's been played and sent round since.
    let mut recorded = vec![Gain::ZERO; MOST_TAPS * 3 + 1];
    recorded[0] = Gain::ONE;
    let most = out.times(tone).size() * level;
    let mut taps = Vec::new();
    for at in 1..recorded.len() {
        let mut played = Gain::ZERO;
        for (head, weight) in weights.iter().enumerate() {
            if *weight > 0.0 && at > head {
                played = played.plus(recorded[at - head - 1].scale(f64::from(*weight)));
            }
        }
        recorded[at] = played.times(round).scale(intensity);
        let heard = played.times(out).times(tone).size() * level;
        if heard >= f64::from(QUIETEST_TAP) {
            taps.push(Tap {
                seconds: (at as f64 * frames / f64::from(rate)) as f32,
                levels: [heard as f32; 2],
            });
            if taps.len() == MOST_TAPS {
                break;
            }
        }
        // Nothing louder can come once all the heads would read is too quiet to be drawn.
        let latest = recorded[at.saturating_sub(2)..=at].iter();
        if latest
            .map(|gain| gain.size() * most)
            .all(|heard| heard < f64::from(QUIETEST_TAP))
        {
            break;
        }
    }
    Repeats {
        dry: ((1.0 - mix) * gain) as f32,
        hertz,
        taps,
    }
}

/// How quiet a repeat is when it's no longer drawn, as a gain: -60 dB.
const QUIETEST_TAP: f32 = 0.001;

impl SlotEffect for Echo {
    type Settings = Settings;

    /// A Space Echo's, whose tape holds its slowest speed, and its spring.
    fn new(rate: u32) -> Self {
        Self::new(rate)
    }

    fn follow(&mut self, settings: Settings, song: SongPlace, _frames: u64) {
        self.follow(settings, song.frames_per_tick);
    }

    fn settle(&mut self) {
        self.settle();
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn quiet_frames(&self, _rate: u32) -> u32 {
        self.quiet_frames()
    }

    fn seed(&mut self, rng: Pcg32) {
        self.seed(rng);
    }
}

impl Rung for Echo {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::effects::spring;
    use crate::dsp::signals::{rms, sines};

    const RATE: u32 = 48_000;
    /// 120 BPM at this rate.
    const FRAMES_PER_TICK: f64 = 25.0;

    /// The first head alone, nothing going round, no wobble and all wet.
    const PLAIN: Settings = Settings {
        mode: 0,
        intensity: 0,
        wow: 0,
        reverb: 0,
        mix: 0xFF,
        ..Settings::DEFAULT
    };

    fn echo(settings: Settings) -> Echo {
        let mut echo = Echo::new(RATE);
        echo.seed(Pcg32::new(7, 1));
        echo.follow(settings, FRAMES_PER_TICK);
        echo.settle();
        echo
    }

    /// What comes out for `input`, then silence, until this many frames, on the left.
    fn through(echo: &mut Echo, input: &[f32], frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|at| echo.process([input.get(at).copied().unwrap_or(0.0); 2])[0])
            .collect()
    }

    /// How loud a sine at a frequency is in a stretch, by its Fourier sum.
    fn level_at(out: &[f32], hertz: f32) -> f32 {
        let (mut sine, mut cosine) = (0.0_f64, 0.0_f64);
        for (at, sample) in out.iter().enumerate() {
            let turn = at as f64 / f64::from(RATE) * f64::from(hertz) * TAU;
            sine += f64::from(*sample) * turn.sin();
            cosine += f64::from(*sample) * turn.cos();
        }
        (2.0 * sine.hypot(cosine) / out.len() as f64) as f32
    }

    fn loudest(out: &[f32]) -> usize {
        out.iter()
            .enumerate()
            .fold((0, 0.0_f32), |best, (at, s)| {
                if s.abs() > best.1 {
                    (at, s.abs())
                } else {
                    best
                }
            })
            .0
    }

    #[test]
    fn the_heads_are_one_two_and_three_times_the_first_ones_delay() {
        for rate in [0x00_u8, 0x69, 0xFF] {
            let first = f64::from(echo_head_seconds(f32::from(rate))) * f64::from(RATE);
            for (mode, heads) in [(0_u8, 1.0), (1, 2.0), (2, 3.0)] {
                let mut echo = echo(Settings {
                    rate,
                    mode,
                    ..PLAIN
                });
                let out = through(&mut echo, &[1.0], (first * 3.0) as usize + 4800);
                // The head's filters bring it a few samples late.
                let at = loudest(&out) as f64;
                assert!(
                    (at - first * heads).abs() < 8.0,
                    "{rate:x} {mode}: {at} against {}",
                    first * heads
                );
            }
        }
        // `69` is TAPE-201's 150 ms.
        assert!((echo_head_seconds(f32::from(0x69_u8)) - 0.150).abs() < 0.001);
    }

    #[test]
    fn a_quiet_sine_comes_back_as_loud_as_tape_201s() {
        // The tape at 100 ms, where TAPE-201's was measured.
        let mut echo = echo(Settings {
            rate: 0x80,
            ..PLAIN
        });
        let out = through(
            &mut echo,
            &sines(RATE as usize, 1000.0, 0.01, RATE).collect::<Vec<_>>(),
            RATE as usize,
        );
        let db = 20.0 * (level_at(&out[RATE as usize / 2..], 1000.0) / 0.01).log10();
        assert!((db - ECHO_DB_AT_1K as f32).abs() < 0.1, "{db}");
    }

    #[test]
    fn synced_the_first_head_is_a_note_division_at_the_tempo() {
        // `1/8` is the tenth division, after `Free`: 480 ticks, 12000 frames at 120 BPM.
        let mut echo = echo(Settings { sync: 10, ..PLAIN });
        assert_eq!(echo.frames(), 12_000.0);
        let out = through(&mut echo, &[1.0], 14_000);
        assert!(loudest(&out).abs_diff(12_000) < 8);
    }

    #[test]
    fn a_loud_sine_is_held_by_the_saturation_and_gains_odd_harmonics() {
        let measured = |level: f32| {
            let mut echo = echo(PLAIN);
            let out = through(
                &mut echo,
                &sines(RATE as usize, 1000.0, level, RATE).collect::<Vec<_>>(),
                RATE as usize,
            );
            let tail = &out[RATE as usize / 2..];
            let peak = tail.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
            (level_at(tail, 1000.0), level_at(tail, 3000.0), peak)
        };
        let (quiet, quiet_third, _) = measured(0.01);
        let (loud, loud_third, peak) = measured(1.0);
        assert!(quiet_third / quiet < 1e-3, "{}", quiet_third / quiet);
        assert!(loud_third / loud > 0.03, "{}", loud_third / loud);
        // TAPE-201 compresses a full-scale 1 kHz sine by 2.2 dB.
        let compressed = 20.0 * (loud / quiet / 100.0).log10();
        assert!((-3.0..-1.2).contains(&compressed), "{compressed}");
        assert!(peak < 1.0, "{peak}");
    }

    /// A 30 ms burst at the frequency the loop keeps most comes back at each tap as loud as the
    /// taps say.
    fn bursts_against_taps(settings: Settings) {
        // The slot turns what comes out by `Gain`, which the taps have in them.
        let gain = level_gain(f32::from(settings.gain));
        let said = repeats(settings, FRAMES_PER_TICK, RATE);
        let mut echo = echo(settings);
        let length = RATE as usize * 3 / 100;
        let burst: Vec<f32> = (0..length)
            .map(|at| {
                let fade = (at as f32 / length as f32 * std::f32::consts::PI)
                    .sin()
                    .powi(2);
                let turn = at as f32 / RATE as f32 * said.hertz * std::f32::consts::TAU;
                turn.sin() * fade * 0.001
            })
            .collect();
        let energy = |samples: &[f32]| samples.iter().map(|s| s * s).sum::<f32>();
        let went_in = energy(&burst);
        let last = said.taps.last().unwrap().seconds;
        let out = through(&mut echo, &burst, ((last + 0.2) * RATE as f32) as usize);
        assert!(said.taps.len() >= 3, "{settings:?}");
        for tap in &said.taps {
            let start = (tap.seconds * RATE as f32) as usize - 1_000;
            let heard = (energy(&out[start..start + length + 2_000]) / went_in).sqrt() * gain;
            let db = 20.0 * (heard / tap.levels[0]).log10();
            assert!(db.abs() < 0.3, "{settings:?} at {}: {db} dB", tap.seconds);
        }
    }

    #[test]
    fn a_burst_repeats_as_loud_as_the_taps_say_in_every_kind_of_mode() {
        // A slow tape, so each repeat's window holds it alone.
        for (mode, intensity) in [(0, 0xA0), (3, 0x90), (10, 0x70), (7, 0xC0)] {
            bursts_against_taps(Settings {
                mode,
                intensity,
                rate: 0x30,
                gain: 0xC0,
                bass: 0x60,
                treble: 0xB0,
                ..PLAIN
            });
        }
    }

    #[test]
    fn a_mode_of_two_heads_comes_at_each_and_the_taps_run_on_when_fed_back_at_the_top() {
        let said = repeats(Settings { mode: 3, ..PLAIN }, FRAMES_PER_TICK, RATE);
        let first = echo_head_seconds(f32::from(PLAIN.rate));
        let times: Vec<f32> = said.taps.iter().map(|tap| tap.seconds / first).collect();
        assert_eq!(times.len(), 2, "{times:?}");
        assert!((times[0] - 2.0).abs() < 1e-3 && (times[1] - 3.0).abs() < 1e-3);
        assert!((said.taps[0].levels[0] / said.taps[1].levels[0] - 1.0).abs() < 1e-6);
        let running = repeats(
            Settings {
                intensity: 0xFF,
                ..PLAIN
            },
            FRAMES_PER_TICK,
            RATE,
        );
        assert_eq!(running.taps.len(), MOST_TAPS);
    }

    #[test]
    fn from_about_dc_it_runs_away_and_stays_bounded() {
        let grows = |intensity: u8| {
            let mut echo = echo(Settings {
                intensity,
                rate: 0xA0,
                ..PLAIN
            });
            let out = through(&mut echo, &[0.01], RATE as usize * 8);
            let peak = out.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
            (
                rms(&out[RATE as usize * 7..]) / rms(&out[..RATE as usize]),
                peak,
            )
        };
        assert!(grows(0xD4).0 < 1.0, "{:?}", grows(0xD4));
        let (growth, peak) = grows(0xFF);
        assert!(growth > 10.0, "{growth}");
        assert!(peak < 1.0, "{peak}");
    }

    /// How fast a 1 kHz sine through it goes in each 10 cycles, from where it crosses zero going
    /// up, from a frame on.
    fn pitches(echo: &mut Echo, input: &[f32], from: usize) -> Vec<f32> {
        let out = through(echo, input, input.len());
        let mut crossings = Vec::new();
        for at in from.max(1)..out.len() {
            if out[at - 1] < 0.0 && out[at] >= 0.0 {
                crossings.push(at as f32 - out[at] / (out[at] - out[at - 1]));
            }
        }
        crossings
            .windows(11)
            .step_by(10)
            .map(|found| 10.0 / (found[10] - found[0]) * RATE as f32)
            .collect()
    }

    #[test]
    fn slowing_the_tape_bends_whats_on_it_down_and_inertia_bends_it_less() {
        let bent = |inertia: u8| {
            let settings = Settings { inertia, ..PLAIN };
            let mut echo = echo(settings);
            let tone: Vec<f32> = sines(RATE as usize * 6, 1000.0, 0.1, RATE).collect();
            through(&mut echo, &tone[..RATE as usize], RATE as usize);
            // From 150 ms to 300 ms, as TAPE-201 was measured.
            let slower = (255.0 * (1000.0_f32 / 300.0).ln() / 100_f32.ln()).round() as u8;
            echo.follow(
                Settings {
                    rate: slower,
                    ..settings
                },
                FRAMES_PER_TICK,
            );
            let after = pitches(&mut echo, &tone[RATE as usize..], 0);
            let low = after.iter().fold(f32::MAX, |most, p| most.min(*p));
            let end = after[after.len() - 5..].iter().sum::<f32>() / 5.0;
            (low, end)
        };
        let (quick, quick_end) = bent(0x00);
        let (slow, _) = bent(0xFF);
        // TAPE-201 falls to a third of the pitch at its lowest inertia, and 0.93 at its highest.
        assert!((300.0..420.0).contains(&quick), "{quick}");
        assert!((900.0..960.0).contains(&slow), "{slow}");
        assert!((quick_end - 1000.0).abs() < 2.0, "{quick_end}");
    }

    #[test]
    fn wow_bends_the_pitch_by_its_cents_and_repeats_from_its_seed() {
        let spread = |wow: u8, seed: u64| {
            let mut echo = echo(Settings { wow, ..PLAIN });
            echo.seed(Pcg32::new(seed, 1));
            let found = pitches(
                &mut echo,
                &sines(RATE as usize * 6, 1000.0, 0.1, RATE).collect::<Vec<_>>(),
                RATE as usize,
            );
            let cents: Vec<f32> = found.iter().map(|p| 1200.0 * (p / 1000.0).log2()).collect();
            let mean = cents.iter().sum::<f32>() / cents.len() as f32;
            let deviations: Vec<f32> = cents.iter().map(|c| c - mean).collect();
            (rms(&deviations), cents)
        };
        assert!(spread(0, 1).0 < 0.1, "{}", spread(0, 1).0);
        // TAPE-201's full `Flutter` is 21 cents RMS; ten cycles at a time smooth a little of it.
        let (full, once) = spread(0xFF, 1);
        assert!((15.0..24.0).contains(&full), "{full}");
        assert_eq!(once, spread(0xFF, 1).1);
        assert_ne!(once, spread(0xFF, 2).1);
    }

    #[test]
    fn the_tone_controls_lift_and_cut_as_tape_201s() {
        let gain = |bass: u8, treble: u8, hertz: f32| {
            tone_at(
                f64::from(hertz) / f64::from(RATE),
                from_middle(f32::from(bass)),
                from_middle(f32::from(treble)),
                RATE,
            )
            .db()
        };
        assert!(gain(0x80, 0x80, 40.0).abs() < 1e-3 && gain(0x80, 0x80, 8000.0).abs() < 1e-3);
        // TAPE-201's ends: 17.5 dB either way at 40 Hz, and 15 dB at 10 kHz.
        assert!((15.0..20.0).contains(&gain(0xFF, 0x80, 40.0)));
        assert!((-20.0..-15.0).contains(&gain(0x00, 0x80, 40.0)));
        assert!((13.0..16.0).contains(&gain(0x80, 0xFF, 10_000.0)));
        assert!(gain(0x80, 0xFF, 200.0) < 1.0);
    }

    #[test]
    fn changing_mode_glides_from_head_to_head_without_a_step() {
        let mut echo = echo(Settings { mode: 0, ..PLAIN });
        let tone: Vec<f32> = sines(RATE as usize * 2, 200.0, 0.5, RATE).collect();
        let mut last = 0.0;
        for (at, sample) in tone.iter().enumerate() {
            if at == RATE as usize {
                echo.follow(Settings { mode: 2, ..PLAIN }, FRAMES_PER_TICK);
            }
            let out = echo.process([*sample; 2])[0];
            if at > RATE as usize / 2 {
                assert!((out - last).abs() < 0.02, "{at}: {last} to {out}");
            }
            last = out;
        }
    }

    #[test]
    fn reverb_alone_plays_no_repeats_and_its_reverb_is_heard() {
        let reverb_only = |reverb: u8| {
            let mut echo = echo(Settings {
                mode: 11,
                reverb,
                ..PLAIN
            });
            through(&mut echo, &[0.5], RATE as usize)
        };
        assert!(reverb_only(0xFF).iter().any(|s| s.abs() > 1e-3));
        assert!(reverb_only(0x00).iter().all(|s| s.abs() < 1e-9));
    }

    #[test]
    fn clearing_forgets_whats_on_the_tape() {
        let mut echo = echo(Settings {
            intensity: 0xC0,
            reverb: 0xFF,
            mode: 4,
            ..PLAIN
        });
        through(&mut echo, &[1.0], 2000);
        echo.clear();
        let out = through(&mut echo, &[], RATE as usize);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let mut echo = echo(Settings::DEFAULT);
        let tone: Vec<f32> = sines(RATE as usize * 2, 100.0, 0.3, RATE).collect();
        let mut last = [0.0; 2];
        for (at, sample) in tone.iter().enumerate() {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (at / 64 % 2) as u8;
            echo.follow(
                Settings {
                    intensity: 0x20 + step * 0x80,
                    input: 0x60 + step * 0x40,
                    wow: step * 0xFF,
                    bass: 0x40 + step * 0x80,
                    treble: 0x40 + step * 0x80,
                    echo: 0x80 + step * 0x7F,
                    reverb: step * 0xFF,
                    mode: 4 + step * 6,
                    ..Settings::DEFAULT
                },
                FRAMES_PER_TICK,
            );
            let out = echo.process([*sample; 2]);
            if at > 64 {
                for side in 0..2 {
                    assert!(
                        (out[side] - last[side]).abs() < 0.05,
                        "{at}: {last:?} to {out:?}"
                    );
                }
            }
            last = out;
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        // `cargo test --release` plays all 8 corners of `Rate`, `Intensity` and `Input` at each
        // rate. A test build takes the 4 of them that still hold every pair of ends of any two.
        let all = !cfg!(debug_assertions);
        for speed in [0x00_u8, 0xFF] {
            for intensity in [0x00_u8, 0xFF] {
                for input in [0x00_u8, 0xFF] {
                    if !all && input != speed ^ intensity {
                        continue;
                    }
                    for (rate, wow, ends) in [(44_100, 0x00, 0x00), (192_000, 0xFF, 0xFF)] {
                        let mut echo = Echo::new(rate);
                        echo.follow(
                            Settings {
                                rate: speed,
                                intensity,
                                input,
                                wow,
                                bass: ends,
                                treble: ends,
                                inertia: ends,
                                reverb: 0xFF,
                                mode: 10,
                                ..Settings::DEFAULT
                            },
                            1.0,
                        );
                        echo.settle();
                        for frame in 0..rate as usize * 2 {
                            // Noise 12 dB over full scale, a square, DC and silence in turn.
                            let level = 4.0
                                * match frame / (rate as usize / 4) % 4 {
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
                            let out = echo.process([level, -level * 0.5]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() < 16.0),
                                "{speed:x} {intensity:x} {input:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_nan_on_the_tape_is_held_and_dies_away() {
        let mut echo = echo(Settings {
            intensity: 0xC0,
            ..PLAIN
        });
        through(&mut echo, &[0.5], 100);
        *echo.tape.back_mut(10) = [f32::NAN; 2];
        let out = through(&mut echo, &[], RATE as usize * 6);
        assert!(out[RATE as usize..].iter().all(|s| s.is_finite()));
        assert!(out[out.len() - 100..].iter().all(|s| s.abs() < 1e-4));
    }

    #[test]
    fn the_page_shows_what_the_input_and_the_shelves_do() {
        use vt_model::settings::SettingId;
        let shown_db = |key: &str, value: u8| -> f32 {
            let id = SettingId::from_key(&format!("1.echo.{key}")).unwrap();
            let text = id.entry().kind.shown(u16::from(value) << 8);
            text.trim_end_matches(" dB").parse().unwrap()
        };
        for value in [0x00_u8, 0x30, 0x60, 0x80, 0xA0, 0xC0, 0xE0, 0xFF] {
            // The drive as the effect takes it.
            let mut echo = echo(Settings {
                input: value,
                ..PLAIN
            });
            echo.follow(
                Settings {
                    input: value,
                    ..PLAIN
                },
                FRAMES_PER_TICK,
            );
            let drive = 20.0 * echo.drive.target.log10();
            assert!((shown_db("input", value) - drive).abs() < 0.06, "{value:x}");
            // Each shelf where it's flat again: the bass towards DC, the treble at the top.
            let k = from_middle(f32::from(value));
            let bass = tone_at(1e-6, k, 0.0, RATE).db();
            let treble = tone_at(0.4999, 0.0, k, RATE).db();
            assert!(
                (shown_db("bass", value) - bass).abs() < 0.06,
                "{value:x}: {bass}"
            );
            assert!(
                (shown_db("treble", value) - treble).abs() < 0.06,
                "{value:x}: {treble}"
            );
        }
    }

    #[test]
    fn the_spring_rings_as_long_as_tape_201s() {
        // TAPE-201's falls 60 dB in 4.7 s from half a second on, from a click.
        let tail = spring::tail(SPRING, RATE).unwrap();
        assert!((4.2..5.2).contains(&tail.seconds[0]), "{tail:?}");
    }
}
