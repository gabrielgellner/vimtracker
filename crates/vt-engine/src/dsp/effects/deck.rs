//! A cassette deck or a VCR's linear audio track, worn or not (#259, direction 3): lo-fi tape.
//! What goes in is pressed by a Dolby-style compander, saturated through [`Colour`]'s frame with
//! the highs lifted first, as tape's pre-emphasis does, gathers hiss, wobbles, drops out and
//! chews, loses its highs to the playback head as `Reel`'s does at the format's speed, loses its
//! lows, and comes back through the compander's other half, which mistracks by `Track`.
//!
//! The format, `Type`, sets what a machine is: its speed and its head's losses, its lows, the
//! colour of its hiss, how fast its wow and flutter go and how wide its stereo is. The settings
//! are amounts, the same for every format.
//!
//! - **Wow** is a slow sine and an Ornstein–Uhlenbeck drift, as ChowTape's `WowProcess` and
//!   `OHProcess` have it, so it wanders rather than repeats; **flutter** is ChowTape's three
//!   harmonics of the capstan's turn.
//! - **Dropouts** are its own, not ChowTape's: each a dip of a raised cosine, 10 to 120 ms long,
//!   both quieter and darker, as a patch of missing oxide is, at random times.
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
//! [`Colour`]: crate::dsp::blocks::saturator

use std::f32::consts::TAU;

use vt_model::insert::{Deck as Settings, DeckType, tape_drive_db};

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::coefficient::{of_corner, of_time};
use crate::dsp::blocks::line::StereoLine;
use crate::dsp::blocks::saturator::{Colouring, Curve, Saturator, Tuning};
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::{SILENT, SongPlace};
use crate::dsp::effects::reel::{Head, Loss};
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::rng::Pcg32;

/// What a format is.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Format {
    /// Its speed in inches a second, and its head's gap and spacing and its coating's depth read,
    /// in µm.
    ips: f32,
    gap_um: f32,
    spacing_um: f32,
    thickness_um: f32,
    /// Where its lows fall away, by two poles.
    low_hz: f32,
    /// How far its pre-emphasis lifts the highs into the curve, as a share of `Colour`'s most.
    emphasis: f32,
    /// How much its hiss leans to the highs, from none, white, to all.
    hiss_highs: f32,
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
/// head, its transport loose, and dropping out by itself.
const FORMATS: [Format; 3] = [
    Format {
        ips: 1.875,
        gap_um: 1.0,
        spacing_um: 0.12,
        thickness_um: 0.5,
        low_hz: 30.0,
        emphasis: 0.5,
        hiss_highs: 0.5,
        wow_hz: 0.6,
        flutter_hz: 8.0,
        wow_depth: 1.0,
        worn_drops: 0.0,
        width: 1.0,
    },
    Format {
        ips: 1.31,
        gap_um: 1.0,
        spacing_um: 0.25,
        thickness_um: 0.5,
        low_hz: 90.0,
        emphasis: 0.3,
        hiss_highs: 0.2,
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
        low_hz: 60.0,
        emphasis: 0.6,
        hiss_highs: 0.4,
        wow_hz: 0.45,
        flutter_hz: 7.0,
        wow_depth: 1.6,
        worn_drops: 0.5,
        width: 0.8,
    },
];

/// The most hiss, as its RMS before the head: -30 dBFS.
const MOST_HISS: f32 = 0.031_6;
/// How long hiss carries on after the sound stops, and how long it takes to fade, in seconds,
/// so a quiet track falls silent and sleeps rather than hissing for ever.
const HISS_HOLD_SECONDS: f32 = 4.0;
const HISS_FADE_SECONDS: f32 = 1.0;
/// How far the wow moves the line either way at `FF`, in seconds, and the flutter.
const MOST_WOW_SECONDS: f32 = 0.003;
const MOST_FLUTTER_SECONDS: f32 = 0.000_05;
/// How long the wow's drift takes to forget where it was, in seconds.
const DRIFT_SECONDS: f32 = 0.8;
/// Where the drift is low-passed, in Hz.
const WANDER_HZ: f32 = 1.0;
/// How much of the wow is its sine, and how much its drift.
const WOW_SINE: f32 = 0.6;
const WOW_DRIFT: f32 = 0.4;
/// How many frames the line's length moves in a straight line between workings out, as the
/// dropouts' and chews' chances are drawn.
const CONTROL_FRAMES: u32 = 32;
/// The most dropouts a second at `Drops` `FF`, the shortest and longest, and the most one dips.
const MOST_DROPS: f32 = 4.0;
const DROP_SECONDS: (f32, f32) = (0.01, 0.12);
const MOST_DROP_DB: f32 = 30.0;
/// The corner a dropout darkens toward, and a chew.
const DROP_HZ: f32 = 2_000.0;
const CHEW_HZ: f32 = 5_000.0;
/// How long the tape runs clean between chews on average, at the least `Chew` and at `FF`, and
/// how long a chew lasts at the shortest and longest, in seconds.
const CHEW_APART_SECONDS: (f32, f32) = (6.0, 0.5);
const CHEW_SECONDS: (f32, f32) = (0.05, 0.4);
/// How fast a chew comes in and goes, in seconds.
const CHEW_FADE_SECONDS: f32 = 0.01;
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
    hiss: f32,
    dropout: f32,
    chew: f32,
    head: Head,
    low_cut: [f32; 2],
}

/// What both sides share for a frame.
#[derive(Debug, Clone, Copy)]
struct Frame {
    colouring: Colouring,
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
    /// The hiss's level and how far it leans to the highs.
    hiss: f32,
    hiss_highs: f32,
    /// How far into its dip a dropout is, from 0 to 1, what's left of the level then, and its
    /// darkening's share.
    drop: f32,
    drop_level: f32,
    drop_band: f32,
    /// How far a chew is in, its power and its darkening's share.
    chew: f32,
    chew_power: f32,
    chew_band: f32,
    low_cut: f32,
}

impl Side {
    /// Onto the tape: lifted, saturated, and hiss on it.
    #[inline]
    fn record(&mut self, x: f32, frame: &Frame, tuning: &Tuning, rng: &mut Pcg32) -> f32 {
        let mut x = x;
        if frame.companding {
            let highs = x - self.encoder.lows;
            self.encoder.lows += highs * frame.band;
            self.encoder.detect(highs.abs(), frame, 0.0);
            x += highs * (self.encoder.gain - 1.0);
        }
        let mut out = self.saturator.process(x, frame.colouring, tuning);
        if frame.hiss > 0.0 {
            let white = rng.bipolar();
            // What a one-pole leaves above it, toward which the format's hiss leans.
            let highs = white - self.hiss;
            self.hiss += highs * 0.5;
            out += (white + (highs - white) * frame.hiss_highs) * frame.hiss;
        }
        out
    }

    /// Off the tape: dropped out, chewed, through the head, the lows lost, and expanded back.
    #[inline]
    fn play(&mut self, x: f32, frame: &Frame, loss: &Loss) -> f32 {
        let mut x = x;
        self.dropout += (x - self.dropout) * frame.drop_band;
        if frame.drop > 0.0 {
            x = (x + (self.dropout - x) * frame.drop) * frame.drop_level;
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
        for state in &mut self.low_cut {
            *state += (x - *state) * frame.low_cut;
            x -= *state;
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
    /// A dropout's place and length in frames, and how deep it goes, in dB; none at no length.
    drop_at: u32,
    drop_frames: u32,
    drop_db: f32,
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
            drop_db: 0.0,
            chew_left: 0,
            chewing: false,
            chew_power: 1.0,
        }
    }
}

/// How far each of its filters and followers moves a frame, worked out for the rate and format.
#[derive(Debug, Clone, Copy)]
struct Shares {
    band: f32,
    attack: f32,
    release: f32,
    glide: f32,
    drop_band: f32,
    chew_band: f32,
    chew_fade: f32,
    low_cut: f32,
}

impl Shares {
    fn new(rate: f32, low_hz: f32) -> Self {
        let frames = rate as u32;
        Self {
            band: of_corner(COMPANDER_HZ, rate),
            attack: of_time(DETECT_SECONDS.0, frames),
            release: of_time(DETECT_SECONDS.1, frames),
            glide: of_time(0.001, frames),
            drop_band: of_corner(DROP_HZ, rate),
            chew_band: of_corner(CHEW_HZ, rate),
            chew_fade: of_time(CHEW_FADE_SECONDS, frames),
            low_cut: of_corner(low_hz, rate),
        }
    }
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
    tuning: Tuning,
    shares: Shares,
    drive: Smoothed,
    hiss: Smoothed,
    /// How far wow and flutter move the line at most, in frames.
    wow: Smoothed,
    flutter: Smoothed,
    drops: Smoothed,
    chew: Smoothed,
    chew_mix: Smoothed,
    /// `Track`, from 0 to 1; none is no compander.
    track: Smoothed,
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
            tuning: Tuning::default(),
            shares: Shares::new(rate as f32, FORMATS[0].low_hz),
            drive: Smoothed::default(),
            hiss: Smoothed::default(),
            wow: Smoothed::default(),
            flutter: Smoothed::default(),
            drops: Smoothed::default(),
            chew: Smoothed::default(),
            chew_mix: Smoothed::default(),
            track: Smoothed::default(),
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
        self.tuning = Tuning::new(format.emphasis, 0.0, self.rate);
        self.shares = Shares::new(rate, format.low_hz);
    }

    pub(crate) fn frames(&self) -> usize {
        self.line.frames()
    }

    pub(crate) fn clear(&mut self) {
        self.line.clear();
        self.sides = [Side::default(); 2];
        self.transport = Transport::new(self.seed);
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
        let squared = |value: u8| share(value) * share(value);
        let rate = self.rate as f32;
        self.drive.target = 10_f32.powf(tape_drive_db(f32::from(settings.drive)) / 20.0);
        self.hiss.target = squared(settings.hiss) * MOST_HISS;
        self.wow.target = squared(settings.wow) * MOST_WOW_SECONDS * self.format.wow_depth * rate;
        self.flutter.target = squared(settings.flutter) * MOST_FLUTTER_SECONDS * rate;
        self.drops.target = share(settings.drops);
        self.chew.target = share(settings.chew);
        self.track.target = share(settings.track);
    }

    /// Jump to what's set.
    pub(crate) fn settle(&mut self) {
        for setting in [
            &mut self.drive,
            &mut self.hiss,
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
            // The flutter: ChowTape's three harmonics of the capstan.
            let turn = &mut t.flutter;
            let first = Self::turned(turn, format.flutter_hz * seconds);
            let phase = *turn * TAU;
            let flutter_now = (first
                + 0.35 * (2.0 * phase + 13.0 * std::f32::consts::PI / 4.0).sin()
                + 0.43 * (3.0 * phase - std::f32::consts::PI / 10.0).sin())
                / 1.78;
            let next = 1.0 + wow * (1.0 + wobble) + flutter * (1.0 + flutter_now);
            let now = *t.delay.get_or_insert(next);
            t.slope = (next - now) / frames;
            // A dropout, by chance, with a block's worth of its rate.
            let drop_rate = drops * drops * MOST_DROPS + format.worn_drops;
            if t.drop_frames == 0 && t.rng.unit() < drop_rate * seconds {
                let (shortest, longest) = DROP_SECONDS;
                let length = shortest + (longest - shortest) * t.rng.unit();
                t.drop_frames = (length * rate) as u32;
                t.drop_at = 0;
                let depth = drops.max(format.worn_drops);
                t.drop_db = MOST_DROP_DB * depth * (0.3 + 0.7 * t.rng.unit());
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
                t.chew_power = 1.0 + (1.0 + 2.0 * t.rng.unit()) * chew;
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

        // The hiss stops a while after the sound does.
        if input.iter().any(|s| s.abs() >= SILENT) {
            self.unheard = 0;
        } else {
            self.unheard = self.unheard.saturating_add(1);
        }
        let past = self.unheard as f32 / rate - HISS_HOLD_SECONDS;
        let running = (1.0 - past / HISS_FADE_SECONDS).clamp(0.0, 1.0);

        let t = &mut self.transport;
        let drop = if t.drop_frames > 0 {
            let along = t.drop_at as f32 / t.drop_frames as f32;
            t.drop_at += 1;
            if t.drop_at >= t.drop_frames {
                t.drop_frames = 0;
            }
            let dip = (std::f32::consts::PI * along).sin();
            dip * dip
        } else {
            0.0
        };
        let shares = self.shares;
        self.chew_mix.target = if t.chewing { 1.0 } else { 0.0 };
        let chew_mix = self.chew_mix.next(shares.chew_fade) * chew;
        let track = self.track.next(smoothing);
        let frame = Frame {
            colouring: Colouring {
                curve: Curve::Warm,
                drive: self.drive.next(smoothing),
                even: 0.0,
                hard: 0.0,
            },
            companding: track > 0.0,
            band: shares.band,
            attack: shares.attack,
            release: shares.release,
            glide: shares.glide,
            misread_db: track * MOST_MISTRACK_DB,
            retarget: t.countdown == CONTROL_FRAMES - 1,
            hiss: self.hiss.next(smoothing) * running,
            hiss_highs: self.format.hiss_highs,
            drop,
            drop_level: if drop > 0.0 {
                10_f32.powf(-t.drop_db * drop / 20.0)
            } else {
                1.0
            },
            drop_band: shares.drop_band,
            chew: chew_mix,
            chew_power: t.chew_power,
            chew_band: shares.chew_band,
            low_cut: shares.low_cut,
        };
        self.line.advance();
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            let recorded =
                self.sides[side].record(sample, &frame, &self.tuning, &mut self.transport.rng);
            self.line.write(side, recorded);
            let wobbled = self.line.hermite(side, whole, between);
            out[side] = self.sides[side].play(wobbled, &frame, &self.loss);
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
            most > some + 10.0 && most < -25.0 && some > -60.0,
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
    fn dropouts_dip_the_level_now_and_then() {
        let dips = |drops: u8, kind: DeckType| {
            let mut deck = deck(Settings {
                drops,
                kind,
                ..CLEAN
            });
            let out = left(&through(&mut deck, 1_000.0, 0.5, RATE as usize * 8));
            // Windows of 10 ms whose level is 6 dB or more under the tone's.
            out.chunks(RATE as usize / 100)
                .skip(10)
                .filter(|window| rms(*window) < 0.5 / 2.0_f32.sqrt() * 0.5)
                .count()
        };
        assert_eq!(dips(0, DeckType::Cassette), 0);
        assert!(dips(0xFF, DeckType::Cassette) > 3);
        // A worn tape drops out by itself.
        assert!(dips(0, DeckType::Worn) > 0);
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
