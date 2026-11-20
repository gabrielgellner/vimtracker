//! A cassette deck or a VCR's linear audio track, worn or not (#259, direction 3): lo-fi tape.
//! What goes in is pressed by a Dolby-style compander, saturated through [`Colour`]'s frame with
//! the lows and low mids lifted first and the presence dipped, loses its loud highs, takes its tape's
//! response and the noise that rides on it, wobbles, drops out and chews, loses its highs to the
//! playback head as `Reel`'s does at the format's speed, goes through the format's replay, gathers
//! hiss, and comes back through the compander's other half, which mistracks by `Track`; `Worn`
//! ends in its deck's limiter.
//!
//! The format, `Type`, sets what a machine is: its speed and its head's losses, its replay, how
//! soon its loud highs go, how fast its wow and flutter go, its hiss's colour and how wide its
//! stereo is; its `Tape` how bright it is, how soon its loud highs go and how hard it saturates.
//! The settings are amounts, the same for every format. Its sound is scaled to Wavesfactory's
//! Cassette, compared by output only (#1079, decision 451): `Cassette` is Cassette's Pro deck,
//! `Worn` its Micro deck, each with its Type I, II and IV tape, and the amounts' defaults are
//! Cassette's.
//!
//! - **Drive** is how hard it hits the tape, made up after so it's no louder: at 0 dB a 1 kHz
//!   sine's harmonics are Cassette's at its input of 0 dB.
//! - **Loud highs soften**, as a cassette's do at its slow speed: a high shelf cuts by more as
//!   the level comes up, by a detector on what goes onto the tape.
//! - **Hiss** is pink, as Cassette's is, added after the replay; and the sound's level is wobbled
//!   by noise under 800 Hz, tape's modulation noise, which sits round what's playing.
//! - **Wow** is a slow sine and **flutter** the capstan's turn, a sine whose rate and depth wander
//!   a little; each has noise round its rate, so it wanders rather than repeats, as Cassette's
//!   randomness does.
//! - **Degradation**, with the dropouts: the highs dull by a slow, random wander, as Cassette's
//!   Degradation does.
//! - **Dropouts** are its own, not ChowTape's: the tape flickers on and off the head about eight
//!   times a second, and off it the sound goes through one pole whose corner `Drops` sets, so the
//!   highs flicker and the lows and mids hold, as Cassette's do.
//! - **Chew** is after ChowTape's `ChewProcessor`, its times simplified: now and then, a stretch of
//!   crinkled tape where the sound is bent by a power of its level and darkened.
//! - **The compander** is a one-band Dolby B of a sort: what's above 1.5 kHz is lifted by up to
//!   10 dB as it gets quiet, on the way on, and cut by as much on the way off by a detector on
//!   what comes out, which reads it up to 12 dB low as `Track` goes up, so the highs dull and
//!   breathe as a deck whose levels are off does. At `00` there's no compander at all.
//!
//! Every random draw is from a generator the track hands it, so a render repeats.
//!
//! The flutter's wanders follow ChowTape's `Timing_Effects/OHProcess`, and the chew is adapted
//! from `Chew/ChewProcessor.cpp` in ChowTape (AnalogTapeModel), GPL-3.0:
//!
//! Copyright (c) 2019 Jatin Chowdhury.
//!
//! The pink noise is Paul Kellet's three-pole "economy" filter, its poles kept at the same
//! frequencies at any rate.
//!
//! [`Colour`]: crate::dsp::blocks::saturator

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, TAU};

use vt_model::insert::{Deck as Settings, DeckTape, DeckType, deck_drive_db};

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
    /// How deep it drops out at `Drops` `00`, as a share of [`DROP_OCTAVES`], and the least it
    /// drops out by at any.
    own_drops: f32,
    /// How wide its stereo is, from mono at 0.
    width: f32,
    /// Where its hiss is darkened, by two poles at a Q of a half, or none at 0.
    hiss_hz: f32,
    /// Whether it ends in a limiter, as Cassette's Micro deck does.
    limits: bool,
}

/// The formats, in [`DeckType`]'s order: a cassette at 1⅞ ips; a VCR's linear audio track at
/// SP's 1.31 ips, with its crosstalk between the tracks; and a worn cassette, its tape off the
/// head, its transport loose, and dropping out by itself. `Cassette`'s replay and soft highs are
/// fitted to Cassette's Type I on its Pro deck, `Worn`'s to its Micro deck; `VHS` keeps its lows and highs of decision 444, and its highs soften as a
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
            hz: 7_655.0,
            q: 0.758,
            per_level: 78.6,
            most_db: 18.44,
            knee: 3.42,
        },
        emphasis: 0.053,
        wow_hz: 1.0,
        flutter_hz: 10.0,
        wow_depth: 1.0,
        own_drops: 0.0,
        width: 1.0,
        hiss_hz: 0.0,
        limits: false,
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
            hz: 5_570.0,
            q: 0.758,
            per_level: 78.6,
            most_db: 18.44,
            knee: 3.42,
        },
        emphasis: 0.3,
        wow_hz: 0.35,
        flutter_hz: 14.0,
        wow_depth: 0.8,
        own_drops: 0.0,
        width: 0.5,
        hiss_hz: 8_000.0,
        limits: false,
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
            hz: 8_163.0,
            q: 0.743,
            per_level: 88.2,
            most_db: 17.84,
            knee: 8.0,
        },
        emphasis: 0.6,
        wow_hz: 0.45,
        flutter_hz: 7.0,
        wow_depth: 3.0,
        own_drops: 0.125,
        width: 0.8,
        hiss_hz: 5_000.0,
        limits: true,
    },
];

/// A tape on a format, against Type I on it: its response, as a level, a low shelf, a peak and a
/// high shelf, after the curve; how many dB sooner its loud highs soften; how many dB hotter it
/// hits the curve, and how many more its lows are lifted into it; and how many more of hiss it
/// has.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tape {
    level_db: f32,
    low_hz: f32,
    low_db: f32,
    peak_hz: f32,
    peak_q: f32,
    peak_db: f32,
    high_hz: f32,
    high_q: f32,
    high_db: f32,
    soften_db: f32,
    curve_db: f32,
    lows_db: f32,
    hiss_db: f32,
}

/// Type I, against itself; on `Worn` it hits the curve softer and hisses more, as the Micro
/// deck's does.
const TYPE_I: Tape = Tape {
    level_db: 0.0,
    low_hz: 100.0,
    low_db: 0.0,
    peak_hz: 1_000.0,
    peak_q: 0.7,
    peak_db: 0.0,
    high_hz: 8_000.0,
    high_q: 0.7,
    high_db: 0.0,
    soften_db: 0.0,
    curve_db: 0.0,
    lows_db: 0.0,
    hiss_db: 0.0,
};

/// A VCR's track, whatever the tape: Type I, its hiss kept where decision 445 had it.
const VHS_TAPE: Tape = Tape {
    hiss_db: -5.0,
    ..TYPE_I
};

/// The tapes, by format in [`DeckType`]'s order and then in [`DeckTape`]'s, each fitted to
/// Cassette's against its Type I on the deck the format is fitted to: `Cassette`'s on the Pro
/// deck, `Worn`'s on the Micro, whose fixed replay suits Type I and so lifts II's and IV's highs
/// by 20 dB and more, and drives IV far harder. A VCR takes no cassette, so `VHS`'s are all
/// Type I.
const TAPES: [[Tape; 3]; 3] = [
    [
        TYPE_I,
        Tape {
            level_db: -1.21,
            low_hz: 138.0,
            low_db: -4.34,
            peak_hz: 8_020.0,
            peak_q: 0.3,
            peak_db: 3.71,
            high_hz: 14_000.0,
            high_q: 0.685,
            high_db: 15.3,
            soften_db: 6.0,
            curve_db: -11.34,
            lows_db: 13.49,
            hiss_db: -0.3,
        },
        Tape {
            level_db: -1.38,
            low_hz: 64.9,
            low_db: -5.77,
            peak_hz: 11_000.0,
            peak_q: 0.3,
            peak_db: 8.88,
            high_hz: 14_000.0,
            high_q: 0.801,
            high_db: 18.0,
            soften_db: 12.0,
            curve_db: 13.33,
            lows_db: -10.91,
            hiss_db: -2.0,
        },
    ],
    [VHS_TAPE; 3],
    [
        Tape {
            curve_db: -12.39,
            lows_db: 10.27,
            hiss_db: 3.0,
            ..TYPE_I
        },
        Tape {
            level_db: 1.11,
            low_hz: 287.0,
            low_db: -2.81,
            peak_hz: 6_420.0,
            peak_q: 1.03,
            peak_db: 18.0,
            high_hz: 6_470.0,
            high_q: 0.922,
            high_db: 16.1,
            soften_db: 6.0,
            curve_db: -23.64,
            lows_db: 22.59,
            hiss_db: 3.2,
        },
        Tape {
            level_db: 0.64,
            low_hz: 349.0,
            low_db: -6.23,
            peak_hz: 6_420.0,
            peak_q: 1.63,
            peak_db: 17.4,
            high_hz: 4_460.0,
            high_q: 0.631,
            high_db: 11.0,
            soften_db: 12.0,
            curve_db: 24.27,
            lows_db: -19.86,
            hiss_db: 0.6,
        },
    ],
];

impl Tape {
    /// Whether it changes the response at all.
    fn shapes(self) -> bool {
        [self.level_db, self.low_db, self.peak_db, self.high_db]
            .iter()
            .any(|db| db.abs() > 0.0)
    }
}

/// A tape's response as filters at a rate, its shelves' corners kept under Nyquist.
fn tape_filters(tape: Tape, rate: f32) -> [Biquad; 3] {
    let top = 0.45 * rate;
    [
        Biquad::high_shelf(tape.low_hz, FRAC_1_SQRT_2, -tape.low_db, rate)
            .scaled(10_f32.powf((tape.low_db + tape.level_db) / 20.0)),
        Biquad::peak(tape.peak_hz.min(top), tape.peak_q, tape.peak_db, rate),
        Biquad::high_shelf(tape.high_hz.min(top), tape.high_q, tape.high_db, rate),
    ]
}

/// Where the curve sits against the drive shown, as a gain: at 0 dB a 1 kHz sine's third
/// harmonic is Cassette's at its input of 0 dB, -33 dB at full scale.
const CURVE_LEVEL: f32 = 0.401;
/// How far the lows are lifted into the curve, and let down after, so they saturate first, as
/// Cassette's do: two shelves, each a corner, a Q and its lift in dB under it. Against 1 kHz, 60
/// Hz goes in 11 dB harder, 200 Hz 10, 500 Hz 5, 2 kHz 5 dB softer and 4 kHz 5 harder, which
/// puts each third harmonic within a dB or so of Cassette's from 50 Hz to 4 kHz at -18 to -6
/// dBFS. The tape's lows go on the first.
const LOWS_LIFT: [(f32, f32, f32); 2] = [(1_571.0, 0.347, 26.0), (2_744.0, 1.466, -11.29)];
/// The most hiss, as its RMS: at `FF` about -24 dBFS heard from `Cassette`; it goes as the cube of
/// `Hiss`, so `40` is about -60 dBFS, Cassette's own. It's added after the replay, as Cassette's
/// is, so it stays pink whatever the format and tape do to the sound.
const MOST_HISS: f32 = 0.069;
/// The noise riding on the sound, tape's modulation noise: the sound's level wobbled by noise
/// under [`MODULATION_HZ`], by three poles, so it sits round what's playing rather than over the
/// whole band, as Cassette's Dynamic Noise does. Its depth at `Hiss` `FF`, and the power of its
/// share it goes up by: at `40` its sidebands are about 66 dB a hertz under a tone, Cassette's
/// own, and 8 dB more at `FF`, Cassette's most.
const MOST_MODULATION: f32 = 0.47;
const MODULATION_POWER: f32 = 0.664;
const MODULATION_HZ: f32 = 800.0;
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
/// goes up with its setting, as a power of its share: `FF` is Cassette's most of each with its
/// own randomness in as it starts, 12.4 and 31.7 cents RMS, and `40` its default of each, 2.5 and
/// 3.9.
const MOST_WOW_SECONDS: f32 = 0.001_62;
const MOST_FLUTTER_SECONDS: f32 = 0.000_392;
const WOW_POWER: f32 = 1.154;
const FLUTTER_POWER: f32 = 1.51;
/// How far the flutter's rate and depth wander from where they'd sit, each a share of it, and how
/// long each wander takes to forget where it was, in seconds, smoothed at [`WANDER_HZ`].
const FLUTTER_RATE_WANDER: f32 = 0.015;
const FLUTTER_DEPTH_WANDER: f32 = 0.12;
const FLUTTER_WANDER_SECONDS: f32 = 0.4;
/// Where the wanders are low-passed, in Hz.
const WANDER_HZ: f32 = 1.0;
/// Each wobble's randomness, as Cassette's is: noise in the line's length through two poles at a
/// multiple of its rate and a Q, so the pitch it bends by is a band round the rate, and how loud
/// that is against its sine's, in pitch, for the wow and the flutter.
const NOISE_AT: f32 = 1.1;
const NOISE_Q: f32 = 0.8;
const WOW_NOISE: f32 = 0.36;
const FLUTTER_NOISE: f32 = 0.46;
/// How far either way each wobble can go, as a share of its sine's depth.
const WOW_REACH: f32 = 2.5;
const FLUTTER_REACH: f32 = 3.0;
/// Degradation, as Cassette's: the highs dulled by a slow, random wander, which deepens the loud
/// highs' shelf by a typical cut and a spread about it in dB, never below none, at `Drops` `40`
/// and up; the spread goes up by a power of `Drops`, the typical cut is reached at `40`. The
/// wander forgets where it was over its seconds and is smoothed at its Hz.
const DEGRADE_TYPICAL_DB: f32 = 4.0;
const DEGRADE_MOST_DB: f32 = 6.5;
const DEGRADE_POWER: f32 = 0.51;
const DEGRADE_SECONDS: f32 = 0.3;
const DEGRADE_HZ: f32 = 3.0;
/// How many frames the line's length moves in a straight line between workings out, as the
/// dropouts' and chews' spells are counted and the soft highs' and the dropouts' filters are
/// worked out again.
const CONTROL_FRAMES: u32 = 32;
/// Dropouts, as Cassette's are: the tape flickers between in touch with the head and a little off
/// it, each spell at least the first time and then, on average, the second longer in touch or
/// the third off it, in seconds, so about eight a second. Off it, the sound goes through one pole whose corner is
/// [`DROP_OPEN_HZ`] halved [`DROP_OCTAVES`] times over by `Drops`' depth, gliding in and out over
/// [`DROP_GLIDE_SECONDS`]: at `40` toward 10 kHz, at `FF` toward 900 Hz. The level of the lows and
/// mids holds.
const DROP_SPELL_SECONDS: (f32, f32, f32) = (0.015, 0.06, 0.03);
const DROP_OPEN_HZ: f32 = 20_000.0;
const DROP_OCTAVES: f32 = 8.0;
const DROP_GLIDE_SECONDS: f32 = 0.004;
/// How deep `Drops` goes, as the share of [`DROP_OCTAVES`] at `FF`, and the power of its share it
/// goes up by, so `40` is an eighth, Cassette's own.
const MOST_DROP_DEPTH: f32 = 0.56;
const DROP_POWER: f32 = 1.08;
/// A chew's corner, by one pole.
const CHEW_HZ: f32 = 5_000.0;
/// How long the tape runs clean between chews on average, at the least `Chew` and at `FF`, and
/// how long a chew lasts at the shortest and longest, in seconds.
const CHEW_APART_SECONDS: (f32, f32) = (6.0, 0.5);
const CHEW_SECONDS: (f32, f32) = (0.05, 0.4);
/// How fast a chew comes in and goes, in seconds.
const CHEW_FADE_SECONDS: f32 = 0.01;
/// How fast the level the loud highs soften by rises and falls, in seconds, at each of its two
/// poles.
const SOFTEN_SECONDS: (f32, f32) = (0.045, 0.055);
/// The compander: where its band starts, the most it lifts, the level of the highs from which it
/// starts to lift and how far below that it reaches its most, in dB, its detector's attack and
/// release, in seconds, and how far `Track` at `FF` reads the level low, in dB.
const COMPANDER_HZ: f32 = 1_500.0;
const MOST_LIFT_DB: f32 = 10.0;
const LIFT_FROM_DB: f32 = -12.0;
const LIFT_OVER_DB: f32 = 30.0;
const DETECT_SECONDS: (f32, f32) = (0.002, 0.06);
const MOST_MISTRACK_DB: f32 = 12.0;
/// The Micro deck's limiter on `Worn`: the most a sine's peak can play at, 8 dB under full scale
/// before the drive is made up, as Cassette's limits a 1 kHz sine from -8 dBFS in, and how fast its
/// mean square follows up and down, in seconds.
const LIMIT_CEILING: f32 = 0.49;
const LIMIT_SECONDS: (f32, f32) = (0.005, 0.015);
/// How far, in spreads, a wander can go before it's held.
const WANDER_REACH: f32 = 2.5;
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
    lift: [[f32; 2]; 2],
    unlift: [[f32; 2]; 2],
    soften: [f32; 2],
    tape: [[f32; 2]; 3],
    /// The noise the level is wobbled by, through its three poles.
    wobble: [f32; 2],
    wobble_last: f32,
    pink: [f32; 3],
    hiss_tone: [f32; 2],
    dropout: f32,
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
    lift: [Biquad; 2],
    unlift: [Biquad; 2],
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
    /// The tape's own response, when it isn't Type I's.
    tape: Option<[Biquad; 3]>,
    /// The hiss's level and colour, its darkening after, and how deep the noise riding on the
    /// sound wobbles it, with its poles.
    hiss: f32,
    pink: Pink,
    hiss_tone: Option<Biquad>,
    modulation: f32,
    wobble: Biquad,
    wobble_band: f32,
    /// How far off the head the tape is in a dropout, from 0 to 1, and its darkening's share
    /// then.
    drop: f32,
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
        for (filter, state) in frame.lift.iter().zip(&mut self.lift) {
            x = filter.process(state, x);
        }
        let mut out = self.saturator.process(x, frame.colouring, tuning) * frame.makeup;
        for (filter, state) in frame.unlift.iter().zip(&mut self.unlift) {
            out = filter.process(state, out);
        }
        let mut out = frame.soften.process(&mut self.soften, out);
        if let Some(tape) = &frame.tape {
            for (filter, state) in tape.iter().zip(&mut self.tape) {
                out = filter.process(state, out);
            }
        }
        if frame.modulation > 0.0 {
            let noise = frame.wobble.process(&mut self.wobble, rng.bipolar());
            self.wobble_last += (noise - self.wobble_last) * frame.wobble_band;
            out += out * self.wobble_last * frame.modulation;
        }
        out
    }

    /// The tape's hiss, pink and darkened by the format.
    #[inline]
    fn hiss(&mut self, frame: &Frame, rng: &mut Pcg32) -> f32 {
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
        let noise = noise * pink.scale * frame.hiss;
        match &frame.hiss_tone {
            Some(tone) => tone.process(&mut self.hiss_tone, noise),
            None => noise,
        }
    }

    /// Off the tape: dropped out, chewed, through the head and the replay, hiss on it, and
    /// expanded back.
    #[inline]
    fn play(
        &mut self,
        x: f32,
        frame: &Frame,
        loss: &Loss,
        replay: &[Biquad; 5],
        rng: &mut Pcg32,
    ) -> f32 {
        let mut x = x;
        self.dropout += (x - self.dropout) * frame.drop_band;
        if frame.drop > 0.0 {
            x += (self.dropout - x) * frame.drop;
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
        if frame.hiss > 0.0 {
            x += self.hiss(frame, rng);
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
    /// Each wobble's noise, through its two poles.
    wow_noise: [f32; 2],
    flutter_noise: [f32; 2],
    /// The degradation's wander, of unit spread, and the same smoothed.
    degrade: f32,
    degrade_smooth: f32,
    /// How far back the line is read, how far that moves each frame, and the frames until the
    /// next is worked out. Without one, the next is jumped to.
    delay: Option<f32>,
    slope: f32,
    countdown: u32,
    /// How far the flutter's rate and depth have wandered, each of unit spread, and the same
    /// smoothed.
    flutter_wander: [f32; 2],
    flutter_smooth: [f32; 2],
    /// Whether the tape is off the head, and the frames until it flickers back or off again.
    dropped: bool,
    drop_left: u32,
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
            wow_noise: [0.0; 2],
            flutter_noise: [0.0; 2],
            degrade: 0.0,
            degrade_smooth: 0.0,
            delay: None,
            slope: 0.0,
            countdown: 0,
            flutter_wander: [0.0; 2],
            flutter_smooth: [0.0; 2],
            dropped: false,
            drop_left: 0,
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
    drop_glide: f32,
    chew_band: f32,
    chew_fade: f32,
    soften_attack: f32,
    soften_release: f32,
    limit_attack: f32,
    limit_release: f32,
    pink: Pink,
    wobble: Biquad,
    wobble_band: f32,
}

impl Shares {
    fn new(rate: f32) -> Self {
        let frames = rate as u32;
        Self {
            band: of_corner(COMPANDER_HZ, rate),
            attack: of_time(DETECT_SECONDS.0, frames),
            release: of_time(DETECT_SECONDS.1, frames),
            glide: of_time(0.001, frames),
            drop_glide: of_time(DROP_GLIDE_SECONDS, frames),
            chew_band: of_corner(CHEW_HZ, rate),
            chew_fade: of_time(CHEW_FADE_SECONDS, frames),
            soften_attack: of_time(SOFTEN_SECONDS.0, frames),
            soften_release: of_time(SOFTEN_SECONDS.1, frames),
            limit_attack: of_time(LIMIT_SECONDS.0, frames),
            limit_release: of_time(LIMIT_SECONDS.1, frames),
            pink: Pink::new(rate),
            wobble: Biquad::low_pass(MODULATION_HZ, FRAC_1_SQRT_2, rate),
            wobble_band: of_corner(MODULATION_HZ, rate),
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
    /// The tape, what it is on this format, and its response's filters when it isn't Type I's.
    tape_kind: DeckTape,
    tape: Tape,
    tape_filters: Option<[Biquad; 3]>,
    /// The lows' lift into the curve and its undoing after.
    lift: [Biquad; 2],
    unlift: [Biquad; 2],
    hiss_tone: Option<Biquad>,
    /// Each wobble's noise filter, at the rate the transport is worked out, and what its noise is
    /// scaled by so its pitch is its share of its sine's.
    wow_noise: Biquad,
    wow_noise_scale: f32,
    flutter_noise: Biquad,
    flutter_noise_scale: f32,
    /// How far the degradation deepens the loud highs' shelf now, in dB.
    degrade_db: f32,
    tuning: Tuning,
    shares: Shares,
    drive: Smoothed,
    hiss: Smoothed,
    /// How deep the noise riding on the sound wobbles its level.
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
    /// How far off the head the tape is now, from 0 to 1, and its darkening's share.
    drop: f32,
    drop_band: f32,
    /// The limiter's mean square of what comes out, both sides as one.
    limit: f32,
    /// Frames since anything came in, for the hiss to stop.
    unheard: u32,
    rate: u32,
    smoothing: f32,
}

impl Deck {
    pub(crate) fn new(rate: u32) -> Self {
        let most = (MOST_WOW_SECONDS
            * FORMATS.iter().map(|f| f.wow_depth).fold(0.0, f32::max)
            * WOW_REACH
            + MOST_FLUTTER_SECONDS * FLUTTER_REACH)
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
            tape_kind: DeckTape::I,
            tape: TYPE_I,
            tape_filters: None,
            lift: [Biquad::FLAT; 2],
            unlift: [Biquad::FLAT; 2],
            hiss_tone: None,
            wow_noise: Biquad::FLAT,
            wow_noise_scale: 0.0,
            flutter_noise: Biquad::FLAT,
            flutter_noise_scale: 0.0,
            degrade_db: 0.0,
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
            drop: 0.0,
            drop_band: 1.0,
            limit: 0.0,
            unheard: 0,
            rate,
            smoothing: coefficient(rate),
        };
        deck.shape(DeckType::Cassette, DeckTape::I);
        deck.follow(Settings::DEFAULT);
        deck.settle();
        deck
    }

    /// Work out what a format and its tape fix.
    fn shape(&mut self, kind: DeckType, tape: DeckTape) {
        self.kind = kind;
        self.tape_kind = tape;
        self.format = FORMATS[kind as usize];
        self.tape = TAPES[kind as usize][tape as usize];
        let format = self.format;
        let rate = self.rate as f32;
        self.tape_filters = self.tape.shapes().then(|| tape_filters(self.tape, rate));
        for (at, (hertz, q, db)) in LOWS_LIFT.into_iter().enumerate() {
            // The tape's lows go on the first, the lift itself.
            let db = if at == 0 { db + self.tape.lows_db } else { db };
            self.lift[at] = Biquad::high_shelf(hertz, q, -db, rate).scaled(10_f32.powf(db / 20.0));
            self.unlift[at] = self.lift[at].inverse();
        }
        self.hiss_tone =
            (format.hiss_hz > 0.0).then(|| Biquad::low_pass(format.hiss_hz, 0.5, rate));
        // Two poles at a corner bend the pitch, unit noise in, by the root of Q ω³ over twice the
        // rate they run at; a unit sine at its rate bends it by its ω over root two.
        let control = rate / CONTROL_FRAMES as f32;
        let noise = |hertz: f32, share: f32| {
            let corner = NOISE_AT * hertz;
            let q = NOISE_Q;
            let omega = TAU * corner;
            let bends = (q * omega.powi(3) / (2.0 * control)).sqrt();
            (
                Biquad::low_pass(corner, q, control),
                share * TAU * hertz * FRAC_1_SQRT_2 / bends,
            )
        };
        (self.wow_noise, self.wow_noise_scale) = noise(format.wow_hz, WOW_NOISE);
        (self.flutter_noise, self.flutter_noise_scale) = noise(format.flutter_hz, FLUTTER_NOISE);
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
        // A sine's rectified average is 2/π of its peak; a tape that softens sooner reads it
        // louder.
        let sooner = 10_f32.powf(self.tape.soften_db / 20.0);
        let cut = soften.cut_db(self.level[1] * FRAC_PI_2 * sooner) + self.degrade_db;
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
        self.degrade_db = 0.0;
        self.soften = self.shelf();
        self.drop = 0.0;
        self.limit = 0.0;
        self.unheard = 0;
    }

    /// Draw from a generator of the track's, so a render repeats.
    pub(crate) fn seed(&mut self, rng: Pcg32) {
        self.seed = rng;
        self.transport.rng = rng;
    }

    /// Take what's set now.
    pub(crate) fn follow(&mut self, settings: Settings) {
        if settings.kind != self.kind || settings.tape != self.tape_kind {
            self.shape(settings.kind, settings.tape);
        }
        let share = |value: u8| f32::from(value) / 255.0;
        let rate = self.rate as f32;
        self.drive.target = 10_f32.powf(deck_drive_db(f32::from(settings.drive)) / 20.0);
        self.hiss.target =
            share(settings.hiss).powi(3) * MOST_HISS * 10_f32.powf(self.tape.hiss_db / 20.0);
        self.modulation.target = share(settings.hiss).powf(MODULATION_POWER) * MOST_MODULATION;
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
            // The wow: a sine, and noise round its rate.
            let sine = Self::turned(&mut t.wow, format.wow_hz * seconds);
            let noise = self
                .wow_noise
                .process(&mut t.wow_noise, gaussian(&mut t.rng));
            let wobble = within(sine + noise * self.wow_noise_scale, -WOW_REACH, WOW_REACH);
            // The flutter: the capstan's turn, one sine whose rate and depth wander, each moving
            // the line less as it goes faster, so the pitch it bends by is its depth's.
            let pull = seconds / FLUTTER_WANDER_SECONDS;
            let smooth = of_corner(WANDER_HZ, 1.0 / seconds);
            for (wander, smoothed) in t.flutter_wander.iter_mut().zip(&mut t.flutter_smooth) {
                *wander += -*wander * pull + (2.0 * pull).sqrt() * gaussian(&mut t.rng);
                *wander = wander.clamp(-WANDER_REACH, WANDER_REACH);
                *smoothed += (*wander - *smoothed) * smooth;
            }
            let faster = 1.0 + FLUTTER_RATE_WANDER * t.flutter_smooth[0];
            let deeper = 1.0 + FLUTTER_DEPTH_WANDER * t.flutter_smooth[1];
            let turn = Self::turned(&mut t.flutter, format.flutter_hz * faster * seconds);
            let noise = self
                .flutter_noise
                .process(&mut t.flutter_noise, gaussian(&mut t.rng));
            let flutter_now = within(
                turn * deeper / faster + noise * self.flutter_noise_scale,
                -FLUTTER_REACH,
                FLUTTER_REACH,
            );
            let next = 1.0 + wow * (WOW_REACH + wobble) + flutter * (FLUTTER_REACH + flutter_now);
            // The degradation's wander.
            let pull = seconds / DEGRADE_SECONDS;
            t.degrade += -t.degrade * pull + (2.0 * pull).sqrt() * gaussian(&mut t.rng);
            t.degrade_smooth +=
                (t.degrade - t.degrade_smooth) * of_corner(DEGRADE_HZ, 1.0 / seconds);
            let now = *t.delay.get_or_insert(next);
            t.slope = (next - now) / frames;
            // The tape flickers on and off the head while it drops out at all, each spell as long
            // as the shortest and a random wait.
            if drops > 0.0 {
                t.drop_left = t.drop_left.saturating_sub(CONTROL_FRAMES);
                if t.drop_left == 0 {
                    t.dropped = !t.dropped;
                    let (shortest, touching, off) = DROP_SPELL_SECONDS;
                    let mean = if t.dropped { off } else { touching };
                    let spell = shortest - mean * (1.0 - t.rng.unit()).ln();
                    t.drop_left = (spell * rate) as u32;
                }
            } else {
                t.dropped = false;
                t.drop_left = 0;
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
        // How deep it drops out, as a share of its octaves.
        let depth = if drops > 0.0 || self.format.own_drops > 0.0 {
            (MOST_DROP_DEPTH * drops.powf(DROP_POWER)).max(self.format.own_drops)
        } else {
            0.0
        };
        let delay = self.transport(wow, flutter, depth, chew);
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
            self.degrade_db = if drops > 0.0 {
                let spread = DEGRADE_MOST_DB * drops.powf(DEGRADE_POWER);
                let typical = DEGRADE_TYPICAL_DB * (drops * 4.0).min(1.0);
                (typical + spread * self.transport.degrade_smooth).max(0.0)
            } else {
                0.0
            };
            self.soften = self.shelf();
            let corner = DROP_OPEN_HZ * (-DROP_OCTAVES * depth).exp2();
            self.drop_band = of_corner(corner.min(0.45 * rate), rate);
        }

        // The hiss stops a while after the sound does.
        if input.iter().any(|s| s.abs() >= SILENT) {
            self.unheard = 0;
        } else {
            self.unheard = self.unheard.saturating_add(1);
        }
        let past = self.unheard as f32 / rate - HISS_HOLD_SECONDS;
        let running = (1.0 - past / HISS_FADE_SECONDS).clamp(0.0, 1.0);
        let modulation = self.modulation.next(smoothing);

        let t = &mut self.transport;
        // Off the head and back, gliding, and none at all once it's back and stays.
        let off = if t.dropped { 1.0 } else { 0.0 };
        self.drop += (off - self.drop) * shares.drop_glide;
        if !t.dropped && self.drop < 1e-4 {
            self.drop = 0.0;
        }
        self.chew_mix.target = if t.chewing { 1.0 } else { 0.0 };
        let chew_mix = self.chew_mix.next(shares.chew_fade) * chew.sqrt();
        let track = self.track.next(smoothing);
        let curve = drive * CURVE_LEVEL * 10_f32.powf(self.tape.curve_db / 20.0);
        let frame = Frame {
            colouring: Colouring {
                curve: Curve::Warm,
                drive: curve,
                even: 0.0,
                hard: 0.0,
            },
            makeup: 1.0 / curve,
            lift: self.lift,
            unlift: self.unlift,
            soften: self.soften,
            tape: self.tape_filters,
            companding: track > 0.0,
            band: shares.band,
            attack: shares.attack,
            release: shares.release,
            glide: shares.glide,
            misread_db: track * MOST_MISTRACK_DB,
            retarget,
            hiss: self.hiss.next(smoothing) * running,
            pink: shares.pink,
            hiss_tone: self.hiss_tone,
            modulation,
            wobble: shares.wobble,
            wobble_band: shares.wobble_band,
            drop: self.drop,
            drop_band: self.drop_band,
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
            out[side] = self.sides[side].play(
                wobbled,
                &frame,
                &self.loss,
                &self.replay,
                &mut self.transport.rng,
            );
        }
        // The Micro deck's limiter, both sides as one, on the level it plays at before the
        // drive is made up.
        if self.format.limits {
            let square = (out[0] * out[0]).max(out[1] * out[1]);
            let speed = if square > self.limit {
                shares.limit_attack
            } else {
                shares.limit_release
            };
            self.limit += (square - self.limit) * speed;
            let level = (2.0 * self.limit).sqrt() * drive;
            if level > LIMIT_CEILING {
                let gain = LIMIT_CEILING / level;
                out = out.map(|s| s * gain);
            }
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
        // And the low mids with them, as Cassette's do, up to about 500 Hz: at -12 dBFS 400 Hz's
        // third is within 3 dB of 100 Hz's, and 1.6 kHz's is 15 dB under 1 kHz's.
        let at = |hertz: f32| {
            let mut deck = deck(CLEAN);
            let out = left(&through(&mut deck, hertz, 0.25, RATE as usize));
            db(harmonic(&out, 3.0 * hertz) / harmonic(&out, hertz))
        };
        let (low, low_mid) = (at(100.0), at(400.0));
        assert!((low - low_mid).abs() < 3.0, "{low} {low_mid}");
        assert!(at(1_600.0) < middle - 15.0, "{}", at(1_600.0));
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
        // The flutter's noise moves the line off its turn: a whole turn later, a tenth of a
        // second at 10 Hz, a sine alone would be back within a few hundredths of its depth.
        let mut deck = deck(Settings {
            flutter: 0xFF,
            ..CLEAN
        });
        let mut lengths = Vec::new();
        for frame in 0..RATE as usize * 4 {
            let flutter = deck.flutter.next(deck.smoothing);
            deck.transport(0.0, flutter, 0.0, 0.0);
            if frame % CONTROL_FRAMES as usize == 0 {
                lengths.push(deck.transport.delay.unwrap_or(1.0));
            }
        }
        let turn = RATE as usize / 10 / CONTROL_FRAMES as usize;
        let moved = lengths
            .windows(turn + 1)
            .map(|span| (span[turn] - span[0]).abs())
            .fold(0.0_f32, f32::max);
        assert!(
            moved > 0.5 * deck.flutter.target,
            "{moved} {}",
            deck.flutter.target
        );
    }

    #[test]
    fn dropouts_flicker_the_highs_away_and_leave_the_lows() {
        // Windows of 10 ms whose level is under a share of the tone's usual.
        let dips = |drops: u8, kind: DeckType, hertz: f32, under: f32| {
            let mut deck = deck(Settings {
                drops,
                kind,
                ..CLEAN
            });
            let out = left(&through(&mut deck, hertz, 0.05, RATE as usize * 8));
            let levels: Vec<f32> = out.chunks(RATE as usize / 100).skip(10).map(rms).collect();
            let mut sorted = levels.clone();
            sorted.sort_by(f32::total_cmp);
            let usual = sorted[sorted.len() / 2];
            levels
                .iter()
                .filter(|level| **level < usual * under)
                .count()
        };
        assert_eq!(dips(0, DeckType::Cassette, 6_000.0, 0.95), 0);
        // At the most, 6 kHz goes 6 dB down a good share of the time and 300 Hz never does.
        let (highs, lows) = (
            dips(0xFF, DeckType::Cassette, 6_000.0, 0.5),
            dips(0xFF, DeckType::Cassette, 300.0, 0.5),
        );
        assert!(highs > 100 && lows == 0, "{highs} {lows}");
        // As it starts, 8 kHz flickers by a dB or two, and 1 kHz doesn't.
        let (highs, middle) = (
            dips(0x40, DeckType::Cassette, 8_000.0, 0.89),
            dips(0x40, DeckType::Cassette, 1_000.0, 0.97),
        );
        assert!(highs > 100 && middle == 0, "{highs} {middle}");
        // A worn tape drops out by itself.
        assert!(dips(0, DeckType::Worn, 8_000.0, 0.89) > 0);
    }

    #[test]
    fn the_highs_degrade_slowly_with_the_drops() {
        // A quiet 12 kHz tone's level in tenths of a second: with `Drops` it sits a few dB lower
        // on average and wanders by a dB or more; 1 kHz doesn't.
        let levels = |drops: u8, hertz: f32| {
            let mut deck = deck(Settings { drops, ..CLEAN });
            let out = left(&through(&mut deck, hertz, 0.01, RATE as usize * 10));
            let levels: Vec<f32> = out
                .chunks(RATE as usize / 10)
                .skip(5)
                .map(|chunk| db(rms(chunk)))
                .collect();
            let mean = levels.iter().sum::<f32>() / levels.len() as f32;
            let spread = (levels.iter().map(|l| (l - mean).powi(2)).sum::<f32>()
                / levels.len() as f32)
                .sqrt();
            (mean, spread)
        };
        let (clean, still) = levels(0, 12_000.0);
        let (worn, wandering) = levels(0x40, 12_000.0);
        assert!(
            still < 0.1 && clean - worn > 2.0 && wandering > 1.0,
            "{clean} {still} {worn} {wandering}"
        );
        let (clean, _) = levels(0, 1_000.0);
        let (worn, _) = levels(0x40, 1_000.0);
        assert!((clean - worn).abs() < 0.3, "{clean} {worn}");
    }

    #[test]
    fn a_worn_deck_limits_what_is_loud() {
        let gain = |kind: DeckType, level: f32| {
            let mut deck = deck(of(kind));
            db(harmonic(
                &left(&through(&mut deck, 1_000.0, level, RATE as usize / 2)),
                1_000.0,
            ) / level)
        };
        // Quiet, it's as loud as it came in; at full scale about 8 dB down, as Cassette's Micro
        // deck is, where a cassette is nearly as loud.
        assert!(gain(DeckType::Worn, 0.03).abs() < 1.0);
        let (worn, cassette) = (gain(DeckType::Worn, 1.0), gain(DeckType::Cassette, 1.0));
        assert!(
            (worn + 7.9).abs() < 1.5 && cassette > -1.5,
            "{worn} {cassette}"
        );
    }

    #[test]
    fn type_ii_and_iv_are_brighter_and_a_vcr_has_no_tape() {
        // Quiet, under where the loud highs soften.
        let highs = |kind: DeckType, tape: DeckTape| {
            let at = |hertz: f32| {
                let mut deck = deck(Settings { tape, ..of(kind) });
                harmonic(
                    &left(&through(&mut deck, hertz, 0.002, RATE as usize / 2)),
                    hertz,
                )
            };
            db(at(10_000.0) / at(1_000.0))
        };
        let (one, two, four) = (
            highs(DeckType::Cassette, DeckTape::I),
            highs(DeckType::Cassette, DeckTape::II),
            highs(DeckType::Cassette, DeckTape::IV),
        );
        assert!(two > one + 3.0 && four > two + 3.0, "{one} {two} {four}");
        assert!(highs(DeckType::Worn, DeckTape::II) > highs(DeckType::Worn, DeckTape::I) + 10.0);
        assert!(
            (highs(DeckType::Vhs, DeckTape::IV) - highs(DeckType::Vhs, DeckTape::I)).abs() < 0.01
        );
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
