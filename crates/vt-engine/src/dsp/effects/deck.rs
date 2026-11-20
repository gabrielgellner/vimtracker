//! A cassette deck or a VCR's linear audio track, worn or not (#259, direction 3): lo-fi tape.
//! What goes in is pressed by a Dolby-style compander, saturated through [`Colour`]'s frame with
//! the highs lifted first, as tape's pre-emphasis does, loses its loud highs, gathers hiss,
//! wobbles, drops out and chews, loses its highs to the playback head as `Reel`'s does at the
//! format's speed, goes through the format's replay, and comes back through the compander's other
//! half, which mistracks by `Track`.
//!
//! The format, `Type`, sets what a machine is: its speed and its head's losses, its replay, how
//! soon its loud highs go, how fast its wow and flutter go and how wide its stereo is. The
//! settings are amounts, the same for every format. Its sound is scaled to Wavesfactory's
//! Cassette, compared by output only (#1079, decision 451): `Cassette` is Cassette's Type I tape
//! on its Pro deck, `Worn` its Micro deck's replay, and the amounts' defaults are Cassette's.
//!
//! - **Drive** is how hard it hits the tape, made up after so it's no louder: at 0 dB a 1 kHz
//!   sine's harmonics are Cassette's at its input of 0 dB.
//! - **Loud highs soften**, as a cassette's do at its slow speed: a high shelf cuts by more as
//!   the level comes up, by a detector on what goes onto the tape.
//! - **Hiss** is pink, as Cassette's is, and rides on the sound too, as tape's modulation noise
//!   does.
//! - **Wow** is a slow sine and an Ornstein–Uhlenbeck drift, as ChowTape's `WowProcess` and
//!   `OHProcess` have it, so it wanders rather than repeats; **flutter** is ChowTape's three
//!   harmonics of the capstan's turn, weighted in pitch rather than in time, so its own rate is
//!   the one heard.
//! - **Dropouts** are its own, not ChowTape's: each a dip of a raised cosine, 10 to 120 ms long,
//!   that takes the highs far more than the lows, as a patch of missing oxide does, at random
//!   times.
//! - **Chew** is after ChowTape's `ChewProcessor`, its times simplified: now and then, a stretch of
//!   crinkled tape where the sound is bent by a power of its level and darkened.
//! - **The compander** is a one-band Dolby B of a sort: what's above 1.5 kHz is lifted by up to
//!   10 dB as it gets quiet, on the way on, and cut by as much on the way off by a detector on
//!   what comes out, which reads it up to 12 dB low as `Track` goes up, so the highs dull and
//!   breathe as a deck whose levels are off does. At `00` there's no compander at all.
//!
//! Every random draw is from a generator the track hands it, so a render repeats.
//!
//! The wow and flutter and the chew are adapted from `Timing_Effects/{WowProcess,OHProcess,
//! FlutterProcess}` and `Chew/ChewProcessor.cpp` in ChowTape (AnalogTapeModel), GPL-3.0:
//!
//! Copyright (c) 2019 Jatin Chowdhury.
//!
//! The pink noise is Paul Kellet's three-pole "economy" filter, its poles kept at the same
//! frequencies at any rate.
//!
//! [`Colour`]: crate::dsp::blocks::saturator

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use vt_model::insert::{Deck as Settings, DeckType, deck_drive_db};

use crate::dsp::blocks::biquad::Biquad;
use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::coefficient::{of_corner, of_time};
use crate::dsp::blocks::line::StereoLine;
use crate::dsp::blocks::saturator::{Colouring, Curve, Saturator, Tuning};
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::{SILENT, SongPlace};
use crate::dsp::effects::reel::{Head, Loss};
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::rng::Pcg32;

/// A format's replay after the head: a high-pass, a peak, a high shelf and a fourth-order
/// low-pass, Butterworth's first pair and a second of its own Q, none at a corner of nothing, and
/// a level in dB that brings 1 kHz back to where it went in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Replay {
    level_db: f32,
    low_hz: f32,
    low_q: f32,
    peak_hz: f32,
    peak_q: f32,
    peak_db: f32,
    shelf_hz: f32,
    shelf_db: f32,
    top_hz: f32,
    top_q: f32,
}

/// How a format's loud highs soften: a high shelf at a corner and a Q, cutting by `per_level` dB
/// for each unit of level, a sine's peak against full scale, toward at most `most_db`, by a knee
/// that rounds the turn.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Soften {
    hz: f32,
    q: f32,
    per_level: f32,
    most_db: f32,
    knee: f32,
}

impl Soften {
    /// The cut in dB at a level.
    #[inline]
    fn cut_db(self, level: f32) -> f32 {
        let share = (self.per_level * level / self.most_db).max(0.0);
        self.most_db * share.powf(self.knee).tanh().powf(1.0 / self.knee)
    }
}

/// What a format is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Format {
    /// Its speed in inches a second, and its head's gap and spacing and its coating's depth read,
    /// in µm.
    ips: f32,
    gap_um: f32,
    spacing_um: f32,
    thickness_um: f32,
    replay: Replay,
    soften: Soften,
    /// How far its pre-emphasis lifts the highs into the curve, as a share of `Colour`'s most.
    emphasis: f32,
    /// How fast its wow's sine and its capstan go, in Hz, and how far its wow goes against a
    /// cassette's.
    wow_hz: f32,
    flutter_hz: f32,
    wow_depth: f32,
    /// How many dropouts a second it has at `Drops` `00`, and how deep they go then.
    worn_drops: f32,
    /// How wide its stereo is, from mono at 0.
    width: f32,
}

/// The formats, in [`DeckType`]'s order: a cassette at 1⅞ ips; a VCR's linear audio track at
/// SP's 1.31 ips, with its crosstalk between the tracks; and a worn cassette, its tape off the
/// head, its transport loose, and dropping out by itself. `Cassette`'s replay and soft highs are
/// fitted to Cassette's Type I on its Pro deck, `Worn`'s replay to its Micro deck and its highs
/// to its Home deck's; `VHS` keeps its lows and highs of decision 444, and its highs soften as a
/// cassette's do, from a lower corner, as it runs slower.
const FORMATS: [Format; 3] = [
    Format {
        ips: 1.875,
        gap_um: 1.0,
        spacing_um: 0.12,
        thickness_um: 0.5,
        replay: Replay {
            level_db: 0.0,
            low_hz: 23.2,
            low_q: 0.856,
            peak_hz: 156.0,
            peak_q: 0.472,
            peak_db: 3.7,
            shelf_hz: 7_200.0,
            shelf_db: 7.33,
            top_hz: 17_400.0,
            top_q: 0.515,
        },
        soften: Soften {
            hz: 6_870.0,
            q: 0.78,
            per_level: 38.3,
            most_db: 16.8,
            knee: 1.3,
        },
        emphasis: 0.5,
        wow_hz: 1.0,
        flutter_hz: 10.0,
        wow_depth: 1.0,
        worn_drops: 0.0,
        width: 1.0,
    },
    Format {
        ips: 1.31,
        gap_um: 1.0,
        spacing_um: 0.25,
        thickness_um: 0.5,
        replay: Replay {
            level_db: 0.0,
            low_hz: 90.0,
            low_q: 0.5,
            peak_hz: 1_000.0,
            peak_q: 0.7,
            peak_db: 0.0,
            shelf_hz: 9_000.0,
            shelf_db: 0.0,
            top_hz: 0.0,
            top_q: 0.7,
        },
        soften: Soften {
            hz: 5_000.0,
            q: 0.78,
            per_level: 38.3,
            most_db: 16.8,
            knee: 1.3,
        },
        emphasis: 0.3,
        wow_hz: 0.35,
        flutter_hz: 14.0,
        wow_depth: 0.8,
        worn_drops: 0.0,
        width: 0.5,
    },
    Format {
        ips: 1.875,
        gap_um: 2.0,
        spacing_um: 0.6,
        thickness_um: 1.0,
        replay: Replay {
            level_db: 2.8,
            low_hz: 253.0,
            low_q: 0.659,
            peak_hz: 493.0,
            peak_q: 0.789,
            peak_db: -6.0,
            shelf_hz: 4_770.0,
            shelf_db: -9.45,
            top_hz: 13_600.0,
            top_q: 1.6,
        },
        soften: Soften {
            hz: 6_045.0,
            q: 0.55,
            per_level: 19.6,
            most_db: 18.5,
            knee: 1.02,
        },
        emphasis: 0.6,
        wow_hz: 0.45,
        flutter_hz: 7.0,
        wow_depth: 3.0,
        worn_drops: 0.8,
        width: 0.8,
    },
];

/// Where the curve sits against the drive shown, as a gain: at 0 dB a 1 kHz sine's third
/// harmonic is Cassette's at its input of 0 dB, -33 dB at full scale.
const CURVE_LEVEL: f32 = 0.46;
/// How far the lows are lifted into the curve, and let down after, so they saturate first, as
/// Cassette's do: a shelf's corner, its Q and its lift in dB, which at 60 and 200 Hz is 13 and
/// 12 dB, at 1 kHz half a dB, and above it nothing.
const LOWS_LIFT: (f32, f32, f32) = (400.0, 0.7, 13.0);
/// The most hiss, as its RMS before the head: at `FF` about -24 dBFS heard from `Cassette`; it
/// goes as the cube of `Hiss`, so `40` is about -60 dBFS, Cassette's own.
const MOST_HISS: f32 = 0.061_6;
/// The most noise riding on the sound, as a share of its level, at `Hiss` `FF`; it goes straight
/// up with `Hiss`, so at `40` it's about 42 dB under a sine's peak, as Cassette's is.
const MOST_MODULATION: f32 = 0.033;
/// Paul Kellet's economy pink: its poles' frequencies at 44.1 kHz, each one's weight over its
/// pole's distance from one, and the white's, with its RMS at 48 kHz, so it's unit noise there.
const PINK_HZ: [f32; 3] = [16.5, 264.0, 3_940.0];
const PINK_WEIGHTS: [f32; 3] = [42.147, 8.0140, 2.4481];
const PINK_WHITE: f32 = 0.1848;
const PINK_RMS: f32 = 1.646;
/// How long hiss carries on after the sound stops, and how long it takes to fade, in seconds,
/// so a quiet track falls silent and sleeps rather than hissing for ever.
const HISS_HOLD_SECONDS: f32 = 4.0;
const HISS_FADE_SECONDS: f32 = 1.0;
/// How far the wow moves the line either way at `FF`, in seconds, and the flutter, and how each
/// goes up with its setting, as a power of its share: `FF` is Cassette's most of each, 15 and
/// 38 cents RMS, and `40` its default of each.
const MOST_WOW_SECONDS: f32 = 0.003_47;
const MOST_FLUTTER_SECONDS: f32 = 0.000_575;
const WOW_POWER: f32 = 1.3;
const FLUTTER_POWER: f32 = 1.6;
/// The capstan's second and third harmonics, as ChowTape weights them in pitch, and their sum
/// with the first's.
const FLUTTER_SECOND: f32 = 0.35;
const FLUTTER_THIRD: f32 = 0.43;
const FLUTTER_SUM: f32 = 1.0 + FLUTTER_SECOND / 2.0 + FLUTTER_THIRD / 3.0;
/// How long the wow's drift takes to forget where it was, in seconds.
const DRIFT_SECONDS: f32 = 0.8;
/// Where the drift is low-passed, in Hz.
const WANDER_HZ: f32 = 1.0;
/// How much of the wow is its sine, and how much its drift.
const WOW_SINE: f32 = 0.6;
const WOW_DRIFT: f32 = 0.4;
/// How many frames the line's length moves in a straight line between workings out, as the
/// dropouts' and chews' chances are drawn and the soft highs' shelf is worked out again.
const CONTROL_FRAMES: u32 = 32;
/// The most dropouts a second at `Drops` `FF`, the shortest and longest, and the most one takes
/// off the whole sound, in dB; its highs go far further.
const MOST_DROPS: f32 = 8.0;
const DROP_SECONDS: (f32, f32) = (0.01, 0.12);
const MOST_DROP_DB: f32 = 6.0;
/// The corner a dropout darkens toward, by two poles, and a chew's, by one.
const DROP_HZ: f32 = 1_500.0;
const CHEW_HZ: f32 = 5_000.0;
/// How long the tape runs clean between chews on average, at the least `Chew` and at `FF`, and
/// how long a chew lasts at the shortest and longest, in seconds.
const CHEW_APART_SECONDS: (f32, f32) = (6.0, 0.5);
const CHEW_SECONDS: (f32, f32) = (0.05, 0.4);
/// How fast a chew comes in and goes, in seconds.
const CHEW_FADE_SECONDS: f32 = 0.01;
/// How fast the level the loud highs soften by rises and falls, in seconds, at each of its two
/// poles.
const SOFTEN_SECONDS: (f32, f32) = (0.03, 0.09);
/// The compander: where its band starts, the most it lifts, the level of the highs from which it
/// starts to lift and how far below that it reaches its most, in dB, its detector's attack and
/// release, in seconds, and how far `Track` at `FF` reads the level low, in dB.
const COMPANDER_HZ: f32 = 1_500.0;
const MOST_LIFT_DB: f32 = 10.0;
const LIFT_FROM_DB: f32 = -12.0;
const LIFT_OVER_DB: f32 = 30.0;
const DETECT_SECONDS: (f32, f32) = (0.002, 0.06);
const MOST_MISTRACK_DB: f32 = 12.0;
/// How far what comes out can go, 12 dB over full scale.
const HEADROOM: f32 = 4.0;

/// A level in dB, held off the floor.
#[inline]
fn decibels(level: f32) -> f32 {
    20.0 * level.max(1e-6).log10()
}

/// How much the compander lifts the highs at a level of them in dB, as a gain.
#[inline]
fn lift(db: f32) -> f32 {
    let share = ((LIFT_FROM_DB - db) / LIFT_OVER_DB).clamp(0.0, 1.0);
    10_f32.powf(share * MOST_LIFT_DB / 20.0)
}

/// Noise near a normal distribution's, of unit variance, from four uniform draws.
#[inline]
fn gaussian(rng: &mut Pcg32) -> f32 {
    let sum = rng.unit() + rng.unit() + rng.unit() + rng.unit();
    (sum - 2.0) * 3.0_f32.sqrt()
}

/// One half of the compander: what's below its band, the level of what's above, and the gain
/// on it.
#[derive(Debug, Clone, Copy)]
struct Half {
    lows: f32,
    level: f32,
    gain: f32,
    target: f32,
}

impl Default for Half {
    fn default() -> Self {
        Self {
            lows: 0.0,
            level: 0.0,
            gain: 1.0,
            target: 1.0,
        }
    }
}

impl Half {
    /// Follow a level of the highs, toward a gain.
    #[inline]
    fn detect(&mut self, level: f32, frame: &Frame, misread_db: f32) {
        let speed = if level > self.level {
            frame.attack
        } else {
            frame.release
        };
        self.level += (level - self.level) * speed;
        // The gain it glides to is worked out again once a control block, which its glide and
        // the detector's attack are both longer than.
        if frame.retarget {
            self.target = lift(decibels(self.level) - misread_db);
        }
        self.gain += (self.target - self.gain) * frame.glide;
    }
}

/// One side's deck.
#[derive(Debug, Clone, Copy, Default)]
struct Side {
    encoder: Half,
    decoder: Half,
    saturator: Saturator,
    lift: [f32; 2],
    unlift: [f32; 2],
    soften: [f32; 2],
    pink: [f32; 3],
    dropout: [f32; 2],
    chew: f32,
    head: Head,
    replay: [[f32; 2]; 5],
}

/// Kellet's pink at a rate: each pole and its weight, and what the whole is scaled by.
#[derive(Debug, Clone, Copy)]
struct Pink {
    poles: [f32; 3],
    weights: [f32; 3],
    scale: f32,
}

impl Pink {
    fn new(rate: f32) -> Self {
        let poles = PINK_HZ.map(|hertz| (-TAU * hertz / rate).exp());
        Self {
            poles,
            weights: [0, 1, 2].map(|at| PINK_WEIGHTS[at] * (1.0 - poles[at])),
            // White of the same level a hertz at any rate, so it's heard as loud at each.
            scale: (rate / 48_000.0).sqrt() / PINK_RMS,
        }
    }
}

/// What both sides share for a frame.
#[derive(Debug, Clone, Copy)]
struct Frame {
    colouring: Colouring,
    /// What the curve's level is made up by after, so the drive is no louder, and the lows'
    /// lift into it and its undoing after.
    makeup: f32,
    lift: Biquad,
    unlift: Biquad,
    /// The loud highs' shelf now.
    soften: Biquad,
    /// Whether the compander is in, its band's share, its detector's, its gain's glide and how
    /// low `Track` reads, in dB.
    companding: bool,
    band: f32,
    attack: f32,
    release: f32,
    glide: f32,
    misread_db: f32,
    /// Whether the compander works its gains out again this frame.
    retarget: bool,
    /// The hiss's level, with what rides on the sound, and its colour.
    hiss: f32,
    pink: Pink,
    /// How far into its dip a dropout is, times how deep it goes, from 0 to 1, what's left of the
    /// level then, and its darkening's share.
    drop: f32,
    drop_level: f32,
    drop_band: f32,
    /// How far a chew is in, its power and its darkening's share.
    chew: f32,
    chew_power: f32,
    chew_band: f32,
}

impl Side {
    /// Onto the tape: lifted, saturated, its loud highs softened, and hiss on it.
    #[inline]
    fn record(&mut self, x: f32, frame: &Frame, tuning: &Tuning, rng: &mut Pcg32) -> f32 {
        let mut x = x;
        if frame.companding {
            let highs = x - self.encoder.lows;
            self.encoder.lows += highs * frame.band;
            self.encoder.detect(highs.abs(), frame, 0.0);
            x += highs * (self.encoder.gain - 1.0);
        }
        let x = frame.lift.process(&mut self.lift, x);
        let out = self.saturator.process(x, frame.colouring, tuning) * frame.makeup;
        let out = frame.unlift.process(&mut self.unlift, out);
        let mut out = frame.soften.process(&mut self.soften, out);
        if frame.hiss > 0.0 {
            let white = rng.bipolar();
            let pink = &frame.pink;
            let mut noise = PINK_WHITE * white;
            for (state, (pole, weight)) in self
                .pink
                .iter_mut()
                .zip(pink.poles.iter().zip(&pink.weights))
            {
                *state = pole * *state + weight * white;
                noise += *state;
            }
            out += noise * pink.scale * frame.hiss;
        }
        out
    }

    /// Off the tape: dropped out, chewed, through the head and the replay, and expanded back.
    #[inline]
    fn play(&mut self, x: f32, frame: &Frame, loss: &Loss, replay: &[Biquad; 5]) -> f32 {
        let mut x = x;
        self.dropout[0] += (x - self.dropout[0]) * frame.drop_band;
        self.dropout[1] += (self.dropout[0] - self.dropout[1]) * frame.drop_band;
        if frame.drop > 0.0 {
            x = (x + (self.dropout[1] - x) * frame.drop) * frame.drop_level;
        }
        if frame.chew > 0.0 {
            let held = within(x, -1.0, 1.0);
            let bent = held.abs().powf(frame.chew_power).copysign(held);
            self.chew += (bent - self.chew) * frame.chew_band;
            x += (self.chew - x) * frame.chew;
        } else {
            self.chew = x;
        }
        x = self.head.play(x, loss);
        for (filter, state) in replay.iter().zip(&mut self.replay) {
            x = filter.process(state, x);
        }
        if frame.companding {
            let highs = x - self.decoder.lows;
            self.decoder.lows += highs * frame.band;
            let cut = highs / self.decoder.gain;
            x += cut - highs;
            // Its detector reads what it gives, as Dolby's decoder does, but `Track` low.
            self.decoder.detect(cut.abs(), frame, frame.misread_db);
        }
        x
    }
}

/// The transport and the tape's faults, which both sides share: the line's length, the dropouts
/// and the chews, worked out every [`CONTROL_FRAMES`].
#[derive(Debug, Clone, Copy)]
struct Transport {
    rng: Pcg32,
    wow: f32,
    flutter: f32,
    drift: f32,
    wander: f32,
    /// How far back the line is read, how far that moves each frame, and the frames until the
    /// next is worked out. Without one, the next is jumped to.
    delay: Option<f32>,
    slope: f32,
    countdown: u32,
    /// A dropout's place and length in frames, and how deep it goes, from 0 to 1; none at no
    /// length.
    drop_at: u32,
    drop_frames: u32,
    drop_depth: f32,
    /// Frames left in a chew or until the next, whether it's chewing, and how hard.
    chew_left: u32,
    chewing: bool,
    chew_power: f32,
}

impl Transport {
    fn new(rng: Pcg32) -> Self {
        Self {
            rng,
            wow: 0.0,
            flutter: 0.0,
            drift: 0.0,
            wander: 0.0,
            delay: None,
            slope: 0.0,
            countdown: 0,
            drop_at: 0,
            drop_frames: 0,
            drop_depth: 0.0,
            chew_left: 0,
            chewing: false,
            chew_power: 1.0,
        }
    }
}

/// How far each of its filters and followers moves a frame, worked out for the rate.
#[derive(Debug, Clone, Copy)]
struct Shares {
    band: f32,
    attack: f32,
    release: f32,
    glide: f32,
    drop_band: f32,
    chew_band: f32,
    chew_fade: f32,
    soften_attack: f32,
    soften_release: f32,
    lift: Biquad,
    unlift: Biquad,
    pink: Pink,
}

impl Shares {
    fn new(rate: f32) -> Self {
        let frames = rate as u32;
        let (hertz, q, db) = LOWS_LIFT;
        let lift = Biquad::high_shelf(hertz, q, -db, rate).scaled(10_f32.powf(db / 20.0));
        Self {
            band: of_corner(COMPANDER_HZ, rate),
            attack: of_time(DETECT_SECONDS.0, frames),
            release: of_time(DETECT_SECONDS.1, frames),
            glide: of_time(0.001, frames),
            drop_band: of_corner(DROP_HZ, rate),
            chew_band: of_corner(CHEW_HZ, rate),
            chew_fade: of_time(CHEW_FADE_SECONDS, frames),
            soften_attack: of_time(SOFTEN_SECONDS.0, frames),
            soften_release: of_time(SOFTEN_SECONDS.1, frames),
            lift,
            unlift: lift.inverse(),
            pink: Pink::new(rate),
        }
    }
}

/// A format's replay as filters at a rate, its low-pass's corner kept under Nyquist.
fn replay_filters(replay: Replay, rate: f32) -> [Biquad; 5] {
    let top = |q: f32| {
        if replay.top_hz > 0.0 {
            Biquad::low_pass(replay.top_hz.min(0.45 * rate), q, rate)
        } else {
            Biquad::FLAT
        }
    };
    [
        Biquad::high_pass(replay.low_hz, replay.low_q, rate),
        Biquad::peak(replay.peak_hz, replay.peak_q, replay.peak_db, rate),
        Biquad::high_shelf(
            replay.shelf_hz,
            std::f32::consts::FRAC_1_SQRT_2,
            replay.shelf_db,
            rate,
        )
        .scaled(10_f32.powf(replay.level_db / 20.0)),
        top(0.541),
        top(replay.top_q),
    ]
}

/// A deck's state and line.
#[derive(Debug, Clone)]
pub(crate) struct Deck {
    sides: [Side; 2],
    line: StereoLine,
    transport: Transport,
    seed: Pcg32,
    kind: DeckType,
    format: Format,
    loss: Loss,
    replay: [Biquad; 5],
    tuning: Tuning,
    shares: Shares,
    drive: Smoothed,
    hiss: Smoothed,
    /// How much noise rides on the sound, as a share of its level.
    modulation: Smoothed,
    /// How far wow and flutter move the line at most, in frames.
    wow: Smoothed,
    flutter: Smoothed,
    drops: Smoothed,
    chew: Smoothed,
    chew_mix: Smoothed,
    /// `Track`, from 0 to 1; none is no compander.
    track: Smoothed,
    /// The level the loud highs soften by, of what goes onto the tape, as a rectified average
    /// through two poles, and their shelf now.
    level: [f32; 2],
    soften: Biquad,
    /// Frames since anything came in, for the hiss to stop.
    unheard: u32,
    rate: u32,
    smoothing: f32,
}

impl Deck {
    pub(crate) fn new(rate: u32) -> Self {
        let most = (MOST_WOW_SECONDS * FORMATS.iter().map(|f| f.wow_depth).fold(0.0, f32::max)
            + MOST_FLUTTER_SECONDS)
            * 2.0
            * rate as f32;
        let size = (most.ceil() as usize + 8).next_power_of_two();
        let seed = Pcg32::new(0x5EED_DEC4, 0);
        let mut deck = Self {
            sides: [Side::default(); 2],
            line: StereoLine::new(size),
            transport: Transport::new(seed),
            seed,
            kind: DeckType::Cassette,
            format: FORMATS[0],
            loss: Loss::default(),
            replay: [Biquad::FLAT; 5],
            tuning: Tuning::default(),
            shares: Shares::new(rate as f32),
            drive: Smoothed::default(),
            hiss: Smoothed::default(),
            modulation: Smoothed::default(),
            wow: Smoothed::default(),
            flutter: Smoothed::default(),
            drops: Smoothed::default(),
            chew: Smoothed::default(),
            chew_mix: Smoothed::default(),
            track: Smoothed::default(),
            level: [0.0; 2],
            soften: Biquad::FLAT,
            unheard: 0,
            rate,
            smoothing: coefficient(rate),
        };
        deck.shape(DeckType::Cassette);
        deck.follow(Settings::DEFAULT);
        deck.settle();
        deck
    }

    /// Work out what a format fixes.
    fn shape(&mut self, kind: DeckType) {
        self.kind = kind;
        self.format = FORMATS[kind as usize];
        let format = self.format;
        let rate = self.rate as f32;
        self.loss = Loss::new(
            format.ips,
            format.gap_um,
            format.spacing_um,
            format.thickness_um,
            rate,
        );
        self.replay = replay_filters(format.replay, rate);
        self.tuning = Tuning::new(format.emphasis, 0.0, self.rate);
        self.soften = self.shelf();
    }

    /// The loud highs' shelf at the level now.
    fn shelf(&self) -> Biquad {
        let soften = self.format.soften;
        // A sine's rectified average is 2/π of its peak.
        let cut = soften.cut_db(self.level[1] * FRAC_PI_2);
        Biquad::high_shelf(soften.hz, soften.q, -cut, self.rate as f32)
    }

    pub(crate) fn frames(&self) -> usize {
        self.line.frames()
    }

    pub(crate) fn clear(&mut self) {
        self.line.clear();
        self.sides = [Side::default(); 2];
        self.transport = Transport::new(self.seed);
        self.level = [0.0; 2];
        self.soften = self.shelf();
        self.unheard = 0;
    }

    /// Draw from a generator of the track's, so a render repeats.
    pub(crate) fn seed(&mut self, rng: Pcg32) {
        self.seed = rng;
        self.transport.rng = rng;
    }

    /// Take what's set now.
    pub(crate) fn follow(&mut self, settings: Settings) {
        if settings.kind != self.kind {
            self.shape(settings.kind);
        }
        let share = |value: u8| f32::from(value) / 255.0;
        let rate = self.rate as f32;
        self.drive.target = 10_f32.powf(deck_drive_db(f32::from(settings.drive)) / 20.0);
        self.hiss.target = share(settings.hiss).powi(3) * MOST_HISS;
        self.modulation.target = share(settings.hiss) * MOST_MODULATION;
        self.wow.target =
            share(settings.wow).powf(WOW_POWER) * MOST_WOW_SECONDS * self.format.wow_depth * rate;
        self.flutter.target =
            share(settings.flutter).powf(FLUTTER_POWER) * MOST_FLUTTER_SECONDS * rate;
        self.drops.target = share(settings.drops);
        self.chew.target = share(settings.chew);
        self.track.target = share(settings.track);
    }

    /// Jump to what's set.
    pub(crate) fn settle(&mut self) {
        for setting in [
            &mut self.drive,
            &mut self.hiss,
            &mut self.modulation,
            &mut self.wow,
            &mut self.flutter,
            &mut self.drops,
            &mut self.chew,
            &mut self.track,
        ] {
            setting.settle();
        }
        self.transport.delay = None;
        self.transport.countdown = 0;
    }

    /// Where a sine is after going a share of a turn.
    #[inline]
    fn turned(phase: &mut f32, by: f32) -> f32 {
        *phase += by;
        *phase -= phase.floor();
        (*phase * TAU).sin()
    }

    /// The transport's next frame: how far back to read the line.
    #[inline]
    fn transport(&mut self, wow: f32, flutter: f32, drops: f32, chew: f32) -> f32 {
        let rate = self.rate as f32;
        let format = self.format;
        let t = &mut self.transport;
        if t.countdown == 0 {
            t.countdown = CONTROL_FRAMES;
            let frames = CONTROL_FRAMES as f32;
            let seconds = frames / rate;
            // The wow: a sine, and an Ornstein–Uhlenbeck drift held near it.
            let sine = Self::turned(&mut t.wow, format.wow_hz * seconds);
            let pull = seconds / DRIFT_SECONDS;
            t.drift += -t.drift * pull + (2.0 * pull).sqrt() * gaussian(&mut t.rng);
            // Smoothed, as ChowTape low-passes its drift, so it bends the pitch rather than jitters it.
            t.wander += (t.drift - t.wander) * of_corner(WANDER_HZ, 1.0 / seconds);
            let wobble = within(WOW_SINE * sine + WOW_DRIFT * t.wander, -1.0, 1.0);
            // The flutter: ChowTape's three harmonics of the capstan, each moving the line by its
            // weight over its number, so each bends the pitch by its weight.
            let turn = &mut t.flutter;
            let first = Self::turned(turn, format.flutter_hz * seconds);
            let phase = *turn * TAU;
            let flutter_now = (first
                + FLUTTER_SECOND / 2.0 * (2.0 * phase + 13.0 * PI / 4.0).sin()
                + FLUTTER_THIRD / 3.0 * (3.0 * phase - PI / 10.0).sin())
                / FLUTTER_SUM;
            let next = 1.0 + wow * (1.0 + wobble) + flutter * (1.0 + flutter_now);
            let now = *t.delay.get_or_insert(next);
            t.slope = (next - now) / frames;
            // A dropout, by chance, with a block's worth of its rate, deeper as `Drops` goes up.
            let drop_rate = drops * MOST_DROPS + format.worn_drops;
            if t.drop_frames == 0 && t.rng.unit() < drop_rate * seconds {
                let (shortest, longest) = DROP_SECONDS;
                let length = shortest + (longest - shortest) * t.rng.unit();
                t.drop_frames = (length * rate) as u32;
                t.drop_at = 0;
                let depth = drops.sqrt().max(format.worn_drops);
                t.drop_depth = depth * (0.3 + 0.7 * t.rng.unit());
            }
            // A chew starts or ends, with a fresh power while it lasts.
            if chew > 0.0 {
                if t.chew_left == 0 {
                    t.chewing = !t.chewing;
                    let span = if t.chewing {
                        let (shortest, longest) = CHEW_SECONDS;
                        shortest + (longest - shortest) * t.rng.unit()
                    } else {
                        let (rarest, most) = CHEW_APART_SECONDS;
                        let mean = rarest * (most / rarest).powf(chew);
                        -mean * (1.0 - t.rng.unit()).ln()
                    };
                    t.chew_left = (span * rate) as u32;
                }
                t.chew_left = t.chew_left.saturating_sub(CONTROL_FRAMES);
                t.chew_power = 1.0 + (1.0 + 2.0 * t.rng.unit()) * chew.sqrt();
            } else {
                t.chewing = false;
                t.chew_left = 0;
            }
        }
        t.countdown -= 1;
        let delay = t.delay.get_or_insert(1.0);
        *delay += t.slope;
        *delay
    }

    /// One frame through the deck, all wet.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let smoothing = self.smoothing;
        let rate = self.rate as f32;
        let wow = self.wow.next(smoothing);
        let flutter = self.flutter.next(smoothing);
        let drops = self.drops.next(smoothing);
        let chew = self.chew.next(smoothing);
        let delay = self.transport(wow, flutter, drops, chew);
        let delay = delay.clamp(1.0, (self.line.mask() - 3) as f32);
        let (whole, between) = (delay as usize, delay - delay.floor());
        let retarget = self.transport.countdown == CONTROL_FRAMES - 1;

        // The level going onto the tape, both sides as one so the image holds, which the loud
        // highs soften by and the noise on the sound rides.
        let drive = self.drive.next(smoothing);
        let shares = self.shares;
        let mut level = within(input[0].abs().max(input[1].abs()), 0.0, HEADROOM) * drive;
        // Two poles, so a hit's first few milliseconds go through bright before its highs
        // soften, as Cassette's do.
        for stage in &mut self.level {
            let speed = if level > *stage {
                shares.soften_attack
            } else {
                shares.soften_release
            };
            *stage += (level - *stage) * speed;
            level = *stage;
        }
        if retarget {
            self.soften = self.shelf();
        }

        // The hiss stops a while after the sound does.
        if input.iter().any(|s| s.abs() >= SILENT) {
            self.unheard = 0;
        } else {
            self.unheard = self.unheard.saturating_add(1);
        }
        let past = self.unheard as f32 / rate - HISS_HOLD_SECONDS;
        let running = (1.0 - past / HISS_FADE_SECONDS).clamp(0.0, 1.0);
        let riding = self.modulation.next(smoothing) * self.level[1] * FRAC_PI_2 / drive;

        let t = &mut self.transport;
        let drop = if t.drop_frames > 0 {
            let along = t.drop_at as f32 / t.drop_frames as f32;
            t.drop_at += 1;
            if t.drop_at >= t.drop_frames {
                t.drop_frames = 0;
            }
            let dip = (PI * along).sin();
            dip * dip * t.drop_depth
        } else {
            0.0
        };
        self.chew_mix.target = if t.chewing { 1.0 } else { 0.0 };
        let chew_mix = self.chew_mix.next(shares.chew_fade) * chew.sqrt();
        let track = self.track.next(smoothing);
        let curve = drive * CURVE_LEVEL;
        let frame = Frame {
            colouring: Colouring {
                curve: Curve::Warm,
                drive: curve,
                even: 0.0,
                hard: 0.0,
            },
            makeup: 1.0 / curve,
            lift: shares.lift,
            unlift: shares.unlift,
            soften: self.soften,
            companding: track > 0.0,
            band: shares.band,
            attack: shares.attack,
            release: shares.release,
            glide: shares.glide,
            misread_db: track * MOST_MISTRACK_DB,
            retarget,
            hiss: (self.hiss.next(smoothing) + riding) * running,
            pink: shares.pink,
            drop,
            drop_level: if drop > 0.0 {
                10_f32.powf(-MOST_DROP_DB * drop / 20.0)
            } else {
                1.0
            },
            drop_band: shares.drop_band,
            chew: chew_mix,
            chew_power: t.chew_power,
            chew_band: shares.chew_band,
        };
        self.line.advance();
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            let recorded =
                self.sides[side].record(sample, &frame, &self.tuning, &mut self.transport.rng);
            self.line.write(side, recorded);
            let wobbled = self.line.hermite(side, whole, between);
            out[side] = self.sides[side].play(wobbled, &frame, &self.loss, &self.replay);
        }
        // The format's stereo: its tracks' crosstalk narrows it.
        let middle = 0.5 * (out[0] + out[1]);
        let apart = 0.5 * (out[0] - out[1]) * self.format.width;
        [middle + apart, middle - apart].map(|s| within(s, -HEADROOM, HEADROOM))
    }
}

impl SlotEffect for Deck {
    type Settings = Settings;

    fn new(rate: u32) -> Self {
        Self::new(rate)
    }

    fn follow(&mut self, settings: Settings, _song: SongPlace, _frames: u64) {
        self.follow(settings);
    }

    fn settle(&mut self) {
        self.settle();
    }

    fn clear(&mut self) {
        self.clear();
    }

    fn seed(&mut self, rng: Pcg32) {
        self.seed(rng);
    }

    /// Its line, and the hiss's hold and fade.
    fn quiet_frames(&self, rate: u32) -> u32 {
        self.frames() as u32 + ((HISS_HOLD_SECONDS + HISS_FADE_SECONDS + 0.5) * rate as f32) as u32
    }
}

impl Rung for Deck {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::signals::{rms, sine};

    const RATE: u32 = 48_000;

    fn deck(settings: Settings) -> Deck {
        let mut deck = Deck::new(RATE);
        deck.seed(Pcg32::new(7, 0));
        deck.follow(settings);
        deck.settle();
        deck
    }

    /// Clean: no hiss, wobble, faults or noise reduction, so only the format and its curve.
    const CLEAN: Settings = Settings {
        hiss: 0,
        wow: 0,
        flutter: 0,
        drops: 0,
        chew: 0,
        track: 0,
        ..Settings::DEFAULT
    };

    fn of(kind: DeckType) -> Settings {
        Settings { kind, ..CLEAN }
    }

    fn through(deck: &mut Deck, hertz: f32, level: f32, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|frame| deck.process([sine(frame, hertz, level, RATE); 2]))
            .collect()
    }

    fn left(out: &[[f32; 2]]) -> Vec<f32> {
        out.iter().map(|frame| frame[0]).collect()
    }

    /// How loud a sine at a frequency is in the last half, as a share of full scale.
    fn harmonic(out: &[f32], hertz: f32) -> f32 {
        let start = out.len() / 2;
        let (mut s, mut c) = (0.0_f64, 0.0_f64);
        for (frame, sample) in out.iter().enumerate().skip(start) {
            let turns = frame as f64 / f64::from(RATE) * f64::from(hertz) * std::f64::consts::TAU;
            s += f64::from(*sample) * turns.sin();
            c += f64::from(*sample) * turns.cos();
        }
        (2.0 * s.hypot(c) / (out.len() - start) as f64) as f32
    }

    fn db(share: f32) -> f32 {
        20.0 * share.log10()
    }

    fn response(settings: Settings, hertz: f32) -> f32 {
        let mut deck = deck(settings);
        db(harmonic(
            &left(&through(&mut deck, hertz, 0.05, RATE as usize / 2)),
            hertz,
        ) / 0.05)
    }

    #[test]
    fn each_format_has_its_bandwidth() {
        for kind in DeckType::ALL {
            let middle = response(of(kind), 1_000.0);
            assert!(middle.abs() < 1.5, "{kind:?}: {middle}");
        }
        // A cassette keeps more highs than a VCR's track, and a worn one fewer than both.
        let highs = |kind| response(of(kind), 8_000.0) - response(of(kind), 1_000.0);
        let (cassette, vhs, worn) = (
            highs(DeckType::Cassette),
            highs(DeckType::Vhs),
            highs(DeckType::Worn),
        );
        assert!(
            cassette > -4.0 && vhs < cassette - 2.0 && worn < vhs,
            "{cassette} {vhs} {worn}"
        );
        // A VCR's lows go first.
        let lows = |kind| response(of(kind), 50.0) - response(of(kind), 1_000.0);
        assert!(lows(DeckType::Vhs) < lows(DeckType::Cassette) - 3.0);
    }

    #[test]
    fn loud_highs_soften_and_quiet_ones_dont() {
        // 10 kHz against itself 30 dB quieter: Cassette's is about 12 dB down at -12 dBFS.
        let at = |level: f32, drive: u8| {
            let mut deck = deck(Settings { drive, ..CLEAN });
            harmonic(
                &left(&through(&mut deck, 10_000.0, level, RATE as usize)),
                10_000.0,
            ) / level
        };
        let quiet = at(0.008, 0x80);
        let loud = db(at(0.25, 0x80) / quiet);
        assert!(loud < -8.0 && loud > -16.0, "{loud}");
        // Driven harder, it softens sooner.
        assert!(db(at(0.032, 0xD5) / quiet) < db(at(0.032, 0x80) / quiet) - 3.0);
    }

    #[test]
    fn drive_saturates_without_getting_louder() {
        let third = |drive: u8, level: f32| {
            let mut deck = deck(Settings { drive, ..CLEAN });
            let out = left(&through(&mut deck, 1_000.0, level, RATE as usize / 2));
            (
                harmonic(&out, 1_000.0) / level,
                harmonic(&out, 3_000.0) / harmonic(&out, 1_000.0),
            )
        };
        for drive in [0x00, 0x80, 0xFF] {
            let (gain, _) = third(drive, 0.03);
            assert!(db(gain).abs() < 0.5, "{drive:x}: {}", db(gain));
        }
        // At 0 dB a full-scale sine's third is Cassette's at its input of 0 dB, -33 dB, and it
        // comes up as the drive does.
        let (_, most) = third(0x80, 1.0);
        assert!((db(most) + 33.0).abs() < 1.5, "{}", db(most));
        assert!(db(third(0xFF, 0.25).1) > db(third(0x80, 0.25).1) + 20.0);
        // The lows saturate first: at -12 dBFS, 60 Hz's third is about 25 dB over 1 kHz's.
        let low = |level: f32| {
            let mut deck = deck(CLEAN);
            let out = left(&through(&mut deck, 60.0, level, RATE as usize));
            harmonic(&out, 180.0) / harmonic(&out, 60.0)
        };
        let (lows, middle) = (db(low(0.25)), db(third(0x80, 0.25).1));
        assert!(lows > middle + 18.0, "{lows} {middle}");
    }

    #[test]
    fn hiss_comes_up_with_its_setting_and_stops_after_the_sound() {
        let hiss = |amount: u8| {
            let mut deck = deck(Settings {
                hiss: amount,
                ..CLEAN
            });
            // A tick to start it, then nothing.
            deck.process([0.5; 2]);
            let out: Vec<f32> = (0..RATE as usize)
                .map(|_| deck.process([0.0; 2])[0])
                .collect();
            rms(&out[RATE as usize / 2..])
        };
        assert!(hiss(0) < 1e-6, "{}", hiss(0));
        let (some, most) = (db(hiss(0x80)), db(hiss(0xFF)));
        assert!(
            most > some + 10.0 && most < -20.0 && some > -60.0,
            "{some} {most}"
        );
        let mut deck = deck(Settings {
            hiss: 0xFF,
            ..CLEAN
        });
        deck.process([0.5; 2]);
        let quiet = SlotEffect::quiet_frames(&deck, RATE) as usize;
        let mut last = [1.0; 2];
        for _ in 0..quiet {
            last = deck.process([0.0; 2]);
        }
        assert!(last.iter().all(|s| s.abs() < SILENT), "{last:?}");
    }

    /// How far a 1 kHz sine's pitch moves, from where it crosses zero going up.
    fn pitch_spread(settings: Settings, seconds: usize) -> f32 {
        let mut deck = deck(settings);
        let out = left(&through(&mut deck, 1_000.0, 0.25, RATE as usize * seconds));
        let mut crossings = Vec::new();
        for at in RATE as usize / 2..out.len() {
            if out[at - 1] < 0.0 && out[at] >= 0.0 {
                crossings.push(at as f32 - out[at] / (out[at] - out[at - 1]));
            }
        }
        let periods: Vec<f32> = crossings
            .chunks(20)
            .filter(|found| found.len() == 20)
            .map(|found| found[19] - found[0])
            .collect();
        let (low, high) = periods.iter().fold((f32::MAX, 0.0_f32), |(low, high), p| {
            (low.min(*p), high.max(*p))
        });
        high / low - 1.0
    }

    #[test]
    fn wow_wanders_slow_and_wide_and_flutter_goes_fast() {
        assert!(pitch_spread(CLEAN, 2) < 1e-4);
        let wow = pitch_spread(Settings { wow: 0xFF, ..CLEAN }, 6);
        let worn = pitch_spread(
            Settings {
                wow: 0xFF,
                ..of(DeckType::Worn)
            },
            6,
        );
        assert!(wow > 0.01 && worn > wow, "{wow} {worn}");
        assert!(
            pitch_spread(
                Settings {
                    flutter: 0xFF,
                    ..CLEAN
                },
                2
            ) > 0.002
        );
    }

    #[test]
    fn dropouts_dip_the_highs_now_and_then_and_the_lows_less() {
        let dips = |drops: u8, kind: DeckType, hertz: f32| {
            let mut deck = deck(Settings {
                drops,
                kind,
                ..CLEAN
            });
            let out = left(&through(&mut deck, hertz, 0.05, RATE as usize * 8));
            // Windows of 10 ms whose level is 6 dB or more under the tone's usual.
            let levels: Vec<f32> = out.chunks(RATE as usize / 100).skip(10).map(rms).collect();
            let mut sorted = levels.clone();
            sorted.sort_by(f32::total_cmp);
            let usual = sorted[sorted.len() / 2];
            levels.iter().filter(|level| **level < usual * 0.5).count()
        };
        assert_eq!(dips(0, DeckType::Cassette, 6_000.0), 0);
        let (highs, lows) = (
            dips(0xFF, DeckType::Cassette, 6_000.0),
            dips(0xFF, DeckType::Cassette, 500.0),
        );
        assert!(highs > 40 && lows < highs / 2, "{highs} {lows}");
        // A worn tape drops out by itself.
        assert!(dips(0, DeckType::Worn, 6_000.0) > 0);
    }

    #[test]
    fn chew_crinkles_now_and_then() {
        // The most of the third harmonic against the tone in any 100 ms, as a chew comes and
        // goes.
        let distortion = |chew: u8| {
            let mut deck = deck(Settings { chew, ..CLEAN });
            let out = left(&through(&mut deck, 300.0, 0.5, RATE as usize * 8));
            out.chunks(RATE as usize / 10)
                .skip(1)
                .map(|window| harmonic(window, 900.0) / harmonic(window, 300.0))
                .fold(0.0, f32::max)
        };
        assert!(
            distortion(0xFF) > distortion(0) * 3.0,
            "{} {}",
            distortion(0),
            distortion(0xFF)
        );
    }

    #[test]
    fn mistracking_dulls_the_highs() {
        let highs = |track: u8| {
            let mut deck = deck(Settings { track, ..CLEAN });
            harmonic(
                &left(&through(&mut deck, 6_000.0, 0.1, RATE as usize / 2)),
                6_000.0,
            )
        };
        // Tracking nearly right, the highs come back near where they were; mistracking, duller.
        assert!(
            db(highs(0x08) / highs(0)).abs() < 2.0,
            "{}",
            db(highs(0x08) / highs(0))
        );
        assert!(
            db(highs(0xFF) / highs(0)) < -3.0,
            "{}",
            db(highs(0xFF) / highs(0))
        );
    }

    #[test]
    fn a_vcr_narrows_the_stereo() {
        let width = |kind| {
            let mut deck = deck(of(kind));
            let out: Vec<[f32; 2]> = (0..RATE as usize / 2)
                .map(|frame| {
                    let s = sine(frame, 500.0, 0.2, RATE);
                    deck.process([s, -s])
                })
                .collect();
            let side: Vec<f32> = out.iter().map(|f| 0.5 * (f[0] - f[1])).collect();
            rms(&side[RATE as usize / 4..])
        };
        assert!(width(DeckType::Vhs) < width(DeckType::Cassette) * 0.6);
    }

    #[test]
    fn the_same_song_renders_the_same_and_another_seed_differently() {
        let render = |seed: u64| {
            let mut deck = Deck::new(RATE);
            deck.seed(Pcg32::new(seed, 0));
            deck.follow(Settings {
                kind: DeckType::Worn,
                hiss: 0xC0,
                wow: 0xC0,
                drops: 0xC0,
                chew: 0xC0,
                track: 0x80,
                ..Settings::DEFAULT
            });
            deck.settle();
            left(&through(&mut deck, 330.0, 0.5, RATE as usize))
        };
        let (a, b) = (render(1), render(1));
        assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
        assert!(
            a.iter()
                .zip(render(2))
                .any(|(x, y)| x.to_bits() != y.to_bits())
        );
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let mut deck = deck(CLEAN);
        let tone = |frame: usize| sine(frame, 100.0, 0.5, RATE);
        let mut last = [0.0; 2];
        for frame in 0..RATE as usize / 10 {
            last = deck.process([tone(frame); 2]);
        }
        for frame in 0..RATE as usize {
            let step = (frame / 64 % 2) as u8;
            deck.follow(Settings {
                drive: 0x20 + step * 0xC0,
                wow: step * 0xFF,
                flutter: step * 0xFF,
                track: 0x10 + step * 0xE0,
                ..CLEAN
            });
            let out = deck.process([tone(RATE as usize / 10 + frame); 2]);
            for side in 0..2 {
                assert!(
                    (out[side] - last[side]).abs() < 0.05,
                    "{frame}: {last:?} to {out:?}"
                );
            }
            last = out;
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for kind in DeckType::ALL {
            for drive in [0x00, 0xFF] {
                for amount in [0x00, 0xFF] {
                    for rate in [44_100, 192_000] {
                        let mut deck = Deck::new(rate);
                        deck.follow(Settings {
                            kind,
                            drive,
                            hiss: amount,
                            wow: amount,
                            flutter: amount,
                            drops: amount,
                            chew: amount,
                            track: amount,
                            ..Settings::DEFAULT
                        });
                        deck.settle();
                        for frame in 0..rate as usize / 4 {
                            let level = match frame / 2000 % 5 {
                                0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                1 => 0.0,
                                2 => {
                                    if frame / 50 % 2 == 0 {
                                        1.0
                                    } else {
                                        -1.0
                                    }
                                }
                                3 => 1.0,
                                _ => 1e6,
                            };
                            let out = deck.process([level, -level]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() <= HEADROOM),
                                "{kind:?} {drive:x} {amount:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
