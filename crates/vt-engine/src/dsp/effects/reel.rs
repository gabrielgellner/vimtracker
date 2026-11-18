//! A reel-to-reel tape machine after Jatin Chowdhury's ChowTape (#259): the head's field drives
//! the tape's magnetisation round a Jiles–Atherton hysteresis loop at twice the rate
//! ([`crate::dsp::blocks::hysteresis`]), a DC blocker takes away what the loop leaves magnetised,
//! the transport wobbles it with `Tape`'s wow and flutter, and the playback head loses its highs
//! to its gap, the tape's spacing from it and the depth it's read from, with a head bump in the
//! lows, each by how fast the tape runs.
//!
//! ChowTape's loss filter is a 64-tap FIR worked out from the three losses' formulas (Wallace's
//! spacing loss `exp(-k d)`, the thickness loss `(1 - exp(-k δ)) / (k δ)` and the gap loss
//! `sin(k g / 2) / (k g / 2)`, where `k` is `2π f / v`), linear in phase and half its length
//! late. Here each is a filter of its own, with nothing late but the gap's own transit:
//!
//! - **The gap** reads the tape over its width, so it's the average of the sound over the time
//!   the tape takes to cross it, `g / v`, whose response is the gap loss exactly, nulls and all.
//! - **The spacing loss** falls as an exponential in frequency, which no filter does for ever:
//!   two poles at `1.49 v / 2π d` with a Q of 0.71 and one at `0.53 v / 2π d` keep within
//!   1.5 dB of it down to -40 dB, found by a minimax fit of the curve's one shape.
//! - **The thickness loss** is a pole at `0.91 v / 2π δ`, within 0.9 dB down to -30 dB.
//!
//! The poles are mapped to the rate by their impulse response, `exp(s T)`, with the gain at
//! nothing kept at one, rather than by the bilinear transform: a corner far above the rate then
//! falls to no filter at all, so no spacing is no loss, where the bilinear transform would have
//! to stop at Nyquist.
//!
//! The head bump is ChowTape's peak, Q 2 and up to 3.5 dB near 100 Hz, at the speed over a head
//! 6.5 mm long, as a studio machine's bump is near 60 Hz at 15 ips; ChowTape ties it to the gap
//! instead. ChowTape's azimuth, its input filters, its degrade and chew, and its own wow and
//! flutter are left out: the wow and flutter are Beads', as `Tape`'s.
//!
//! Adapted from `Plugin/Source/Processors/Loss_Effects/LossFilter.cpp` and
//! `Hysteresis/HysteresisProcessor.cpp` in ChowTape (AnalogTapeModel), GPL-3.0:
//!
//! Copyright (c) 2019 Jatin Chowdhury.

use std::f32::consts::TAU;

use vt_model::insert::{
    Reel as Settings, reel_gap_um, reel_spacing_um, reel_speed_ips, reel_thickness_um,
    tape_drive_db,
};

use crate::dsp::blocks::biquad::Biquad;
use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::coefficient::of_corner;
use crate::dsp::blocks::hysteresis::{Hysteresis, Magnetics};
use crate::dsp::blocks::line::StereoLine;
use crate::dsp::blocks::oversampler::QuadrupleOversampler as Oversampler;
use crate::dsp::blocks::sides::SameBits;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::effects::memory::SongPlace;
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::tape::Wobble;

/// How many times the rate the hysteresis runs at.
const OVERSAMPLING: f64 = 4.0;
/// Where the DC blocker after the hysteresis turns, in Hz.
const DC_HZ: f32 = 5.0;
/// Metres an inch.
const METRES_AN_INCH: f32 = 0.0254;
/// The spacing loss's fit, as shares of `v / 2π d`: the two poles' corner and Q, and the one
/// pole's.
const SPACING_PAIR: f32 = 1.490_852;
const SPACING_Q: f32 = 0.708_941;
const SPACING_POLE: f32 = 0.531_423;
/// The thickness loss's pole, as a share of `v / 2π δ`.
const THICKNESS_POLE: f32 = 0.91;
/// The longest the gap's average can be, in frames, which 20 µm at 3.75 ips is short of at
/// 192 kHz.
const GAP_FRAMES: usize = 64;
/// How long the head is in contact with the tape, in metres, which sets the bump.
const HEAD_CONTACT_M: f32 = 0.0065;
/// The bump's width, and ChowTape's most lift near 100 Hz.
const BUMP_Q: f32 = 2.0;
/// How far what comes out can go, 12 dB over full scale, far past anything it makes.
const HEADROOM: f32 = 4.0;

/// The most wow and flutter move the line, in frames at 48 kHz, as `Tape`'s.
const MOST_WOW_FRAMES: f32 = 4.0 * 105.0;
const MOST_FLUTTER_FRAMES: f32 = 4.0 * 17.0;
const BEADS_RATE: f32 = 48_000.0;

/// A low-pass of one pole or a pair, its poles mapped by `exp(s T)` and one zero placed so its
/// gain at nothing and at Nyquist are the analog filter's, as Vicanek's matched filters ("Matched
/// Second Order Digital Filters", 2016) do: `(b0 + b1 z⁻¹) / (1 + a1 z⁻¹ + a2 z⁻²)`. A corner far
/// past the rate leaves the poles at nothing and the zero with them, so it's no filter at all.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Matched {
    b0: f32,
    b1: f32,
    a1: f32,
    a2: f32,
}

impl Matched {
    /// From the poles and the analog gain at Nyquist.
    fn from_poles(a1: f32, a2: f32, at_nyquist: f32) -> Self {
        let at_nothing = 1.0 + a1 + a2;
        let at_top = (1.0 - a1 + a2) * at_nyquist;
        Self {
            b0: 0.5 * (at_nothing + at_top),
            b1: 0.5 * (at_nothing - at_top),
            a1,
            a2,
        }
    }

    /// One pole at a corner.
    fn pole(hertz: f32, rate: f32) -> Self {
        let ratio = 0.5 * rate / hertz;
        Self::from_poles(
            -(-TAU * hertz / rate).exp(),
            0.0,
            1.0 / (1.0 + ratio * ratio).sqrt(),
        )
    }

    /// A pair at a corner and a Q above a half.
    fn pair(hertz: f32, q: f32, rate: f32) -> Self {
        let omega = TAU * hertz / rate;
        let radius = (-omega / (2.0 * q)).exp();
        let angle = omega * (1.0 - 1.0 / (4.0 * q * q)).sqrt();
        let ratio = 0.5 * rate / hertz;
        let squared = 1.0 - ratio * ratio;
        Self::from_poles(
            if radius > 0.0 {
                -2.0 * radius * angle.cos()
            } else {
                0.0
            },
            radius * radius,
            1.0 / (squared * squared + ratio * ratio / (q * q)).sqrt(),
        )
    }

    /// `state` is the last sample in and the last two out.
    #[inline]
    fn process(self, state: &mut [f32; 3], x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * state[0] - self.a1 * state[1] - self.a2 * state[2];
        *state = [x, y, state[1]];
        y
    }
}

/// The playback head's losses, worked out for a speed, a gap, a spacing and a thickness.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Loss {
    /// How many frames the gap's average is over; one or less is none.
    gap: f32,
    spacing_pair: Matched,
    spacing_pole: Matched,
    thickness: Matched,
    bump: Biquad,
}

impl Loss {
    /// For a speed in inches a second and lengths in µm.
    fn new(ips: f32, gap_um: f32, spacing_um: f32, thickness_um: f32, rate: f32) -> Self {
        let speed = ips * METRES_AN_INCH;
        // A corner for a length: a share of `v / 2π l`, or none past any rate for no length.
        let corner = |share: f32, um: f32| {
            if um > 0.0 {
                share * speed / (TAU * um * 1e-6)
            } else {
                f32::INFINITY
            }
        };
        let bump_hz = speed / HEAD_CONTACT_M;
        let bump = (1.5 * (1000.0 - (bump_hz - 100.0).abs()) / 1000.0).max(1.0);
        Self {
            gap: (gap_um * 1e-6 / speed * rate).min((GAP_FRAMES - 1) as f32),
            spacing_pair: Matched::pair(corner(SPACING_PAIR, spacing_um), SPACING_Q, rate),
            spacing_pole: Matched::pole(corner(SPACING_POLE, spacing_um), rate),
            thickness: Matched::pole(corner(THICKNESS_POLE, thickness_um), rate),
            bump: Biquad::peak(bump_hz, BUMP_Q, 20.0 * bump.log10(), rate),
        }
    }
}

/// One side's tape.
#[derive(Debug, Clone, Copy)]
struct Side {
    oversampler: Oversampler,
    hysteresis: Hysteresis,
    dc: f64,
    /// The last frames the gap reads, going round, and where the newest is.
    gap: [f32; GAP_FRAMES],
    at: usize,
    spacing_pair: [f32; 3],
    spacing_pole: [f32; 3],
    thickness: [f32; 3],
    bump: [f32; 2],
}

impl Default for Side {
    fn default() -> Self {
        Self {
            oversampler: Oversampler::default(),
            hysteresis: Hysteresis::default(),
            dc: 0.0,
            gap: [0.0; GAP_FRAMES],
            at: 0,
            spacing_pair: [0.0; 3],
            spacing_pole: [0.0; 3],
            thickness: [0.0; 3],
            bump: [0.0; 2],
        }
    }
}

impl Side {
    /// Onto the tape: the hysteresis at four times the rate, and what it leaves magnetised taken away.
    #[inline]
    fn record(&mut self, x: f32, tape: &Magnetics, period: f64, dc: f64) -> f32 {
        let Self {
            oversampler,
            hysteresis,
            ..
        } = self;
        let magnetised = oversampler.process(x, |h| hysteresis.process(h, period, tape) as f32);
        // In double precision, as a one-pole this slow in single stops short of what it follows,
        // and before the makeup, so what the tape holds stays blocked as the settings move.
        let magnetised = f64::from(magnetised);
        self.dc += (magnetised - self.dc) * dc;
        ((magnetised - self.dc) * tape.makeup) as f32
    }

    /// Off the tape, through the head's losses.
    #[inline]
    fn play(&mut self, x: f32, loss: &Loss) -> f32 {
        self.at = (self.at + 1) % GAP_FRAMES;
        self.gap[self.at] = x;
        let mut out = x;
        if loss.gap > 1.0 {
            let whole = loss.gap as usize;
            let mut sum = 0.0;
            for back in 0..whole {
                sum += self.gap[(self.at + GAP_FRAMES - back) % GAP_FRAMES];
            }
            let part = loss.gap - whole as f32;
            sum += part * self.gap[(self.at + GAP_FRAMES - whole) % GAP_FRAMES];
            out = sum / loss.gap;
        }
        out = loss.spacing_pair.process(&mut self.spacing_pair, out);
        out = loss.spacing_pole.process(&mut self.spacing_pole, out);
        out = loss.thickness.process(&mut self.thickness, out);
        out = loss.bump.process(&mut self.bump, out);
        within(out, -HEADROOM, HEADROOM)
    }
}

/// A reel's state and line.
#[derive(Debug, Clone)]
pub(crate) struct Reel {
    sides: [Side; 2],
    line: StereoLine,
    wobble: Wobble,
    drive: Smoothed,
    sat: Smoothed,
    bias: Smoothed,
    /// In inches a second, gliding from one speed to the next.
    speed: Smoothed,
    /// In µm.
    gap: Smoothed,
    spacing: Smoothed,
    thickness: Smoothed,
    /// How far wow and flutter move the line at most, in frames.
    wow: Smoothed,
    flutter: Smoothed,
    /// The tape and the losses, worked out for what they were last at.
    tape: Magnetics,
    taped: [f32; 3],
    loss: Loss,
    lost: [f32; 4],
    dc: f64,
    period: f64,
    rate: u32,
    smoothing: f32,
}

impl Reel {
    pub(crate) fn new(rate: u32) -> Self {
        let longest = 1.0 + (MOST_WOW_FRAMES + MOST_FLUTTER_FRAMES) * rate as f32 / BEADS_RATE;
        let size = (longest.ceil() as usize + 4).next_power_of_two();
        let mut reel = Self {
            sides: [Side::default(); 2],
            line: StereoLine::new(size),
            wobble: Wobble::new(rate),
            drive: Smoothed::default(),
            sat: Smoothed::default(),
            bias: Smoothed::default(),
            speed: Smoothed::default(),
            gap: Smoothed::default(),
            spacing: Smoothed::default(),
            thickness: Smoothed::default(),
            wow: Smoothed::default(),
            flutter: Smoothed::default(),
            tape: Magnetics::new(1.0, 0.5, 0.5),
            taped: [f32::NAN; 3],
            loss: Loss::default(),
            lost: [f32::NAN; 4],
            dc: f64::from(of_corner(DC_HZ, rate as f32)),
            period: 1.0 / (f64::from(rate) * OVERSAMPLING),
            rate,
            smoothing: coefficient(rate),
        };
        reel.follow(Settings::DEFAULT);
        reel.settle();
        reel
    }

    /// The longest anything stays in its line, in frames.
    pub(crate) fn frames(&self) -> usize {
        self.line.frames()
    }

    pub(crate) fn clear(&mut self) {
        self.line.clear();
        self.sides = [Side::default(); 2];
        self.wobble = Wobble::new(self.rate);
    }

    /// Take what's set now.
    pub(crate) fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.drive.target = 10_f32.powf(tape_drive_db(f32::from(settings.drive)) / 20.0);
        self.sat.target = share(settings.sat);
        self.bias.target = share(settings.bias);
        self.speed.target = reel_speed_ips(settings.speed);
        self.gap.target = reel_gap_um(f32::from(settings.gap));
        self.spacing.target = reel_spacing_um(f32::from(settings.space));
        self.thickness.target = reel_thickness_um(f32::from(settings.thick));
        let frames = self.rate as f32 / BEADS_RATE;
        let amount = |value: u8| (f32::from(value) / 255.0).powi(2);
        self.wow.target = amount(settings.wow) * MOST_WOW_FRAMES * frames;
        self.flutter.target = amount(settings.flutter) * MOST_FLUTTER_FRAMES * frames;
    }

    /// Jump to what's set.
    pub(crate) fn settle(&mut self) {
        for setting in [
            &mut self.drive,
            &mut self.sat,
            &mut self.bias,
            &mut self.speed,
            &mut self.gap,
            &mut self.spacing,
            &mut self.thickness,
            &mut self.wow,
            &mut self.flutter,
        ] {
            setting.settle();
        }
        self.wobble.restart();
    }

    /// The tape and the losses for the settings as they glide, worked out again only when they
    /// moved.
    #[inline]
    fn tune(&mut self) {
        let taped = [
            self.drive.next(self.smoothing),
            self.sat.next(self.smoothing),
            self.bias.next(self.smoothing),
        ];
        if !taped.same_bits(&self.taped) {
            self.taped = taped;
            let [drive, sat, bias] = taped.map(f64::from);
            self.tape = Magnetics::new(drive, sat, bias);
        }
        let lost = [
            self.speed.next(self.smoothing),
            self.gap.next(self.smoothing),
            self.spacing.next(self.smoothing),
            self.thickness.next(self.smoothing),
        ];
        if !lost.same_bits(&self.lost) {
            self.lost = lost;
            let [speed, gap, spacing, thickness] = lost;
            self.loss = Loss::new(speed, gap, spacing, thickness, self.rate as f32);
        }
    }

    /// One frame through the reel, all wet.
    #[inline]
    pub(crate) fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.tune();
        let wow = self.wow.next(self.smoothing);
        let flutter = self.flutter.next(self.smoothing);
        let delay = self.wobble.next(wow, flutter, self.rate);
        let delay = delay.clamp(1.0, (self.line.mask() - 3) as f32);
        let (whole, between) = (delay as usize, delay - delay.floor());
        self.line.advance();
        let mut out = [0.0; 2];
        for (side, sample) in input.into_iter().enumerate() {
            let recorded = self.sides[side].record(sample, &self.tape, self.period, self.dc);
            self.line.write(side, recorded);
            let wobbled = self.line.hermite(side, whole, between);
            out[side] = self.sides[side].play(wobbled, &self.loss);
        }
        out
    }
}

impl SlotEffect for Reel {
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

    /// Its line, and half a second for the DC blocker and the head bump to settle.
    fn quiet_frames(&self, rate: u32) -> u32 {
        (self.frames() + rate as usize / 2) as u32
    }
}

impl Rung for Reel {
    #[inline]
    fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::signals::sine;

    const RATE: u32 = 48_000;

    fn reel(settings: Settings) -> Reel {
        let mut reel = Reel::new(RATE);
        reel.follow(settings);
        reel.settle();
        reel
    }

    /// Still: no wow or flutter.
    const STILL: Settings = Settings {
        wow: 0,
        flutter: 0,
        ..Settings::DEFAULT
    };

    /// No losses either: only the tape.
    const BARE: Settings = Settings {
        gap: 0,
        space: 0,
        thick: 0,
        speed: 3,
        ..STILL
    };

    fn through(reel: &mut Reel, hertz: f32, level: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| reel.process([sine(frame, hertz, level, RATE); 2])[0])
            .collect()
    }

    /// How loud a sine at a frequency is in the last half, as a share of full scale.
    fn harmonic(out: &[f32], hertz: f32) -> f32 {
        let start = out.len() / 2;
        let (mut sine_sum, mut cosine_sum) = (0.0_f64, 0.0_f64);
        for (frame, sample) in out.iter().enumerate().skip(start) {
            let turns = frame as f64 / f64::from(RATE) * f64::from(hertz) * std::f64::consts::TAU;
            sine_sum += f64::from(*sample) * turns.sin();
            cosine_sum += f64::from(*sample) * turns.cos();
        }
        (2.0 * sine_sum.hypot(cosine_sum) / (out.len() - start) as f64) as f32
    }

    fn db(share: f32) -> f32 {
        20.0 * share.log10()
    }

    /// The loudest of what isn't a harmonic below 20 kHz, against the fundamental, in dB: a
    /// Hann-windowed DFT of the last 4800 frames, in 10 Hz bins.
    fn aliasing(out: &[f32], hertz: f32) -> f32 {
        let n = 4800;
        let tail = &out[out.len() - n..];
        let window = |k: usize| 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / n as f64).cos();
        let bin = |b: usize| {
            let (mut re, mut im) = (0.0, 0.0);
            for (k, s) in tail.iter().enumerate() {
                let turn = std::f64::consts::TAU * (b * k) as f64 / n as f64;
                let w = window(k) * f64::from(*s);
                re += w * turn.cos();
                im -= w * turn.sin();
            }
            re.hypot(im)
        };
        let fundamental = bin((hertz / 10.0) as usize);
        let mut worst = 0.0_f64;
        for b in 2..2000 {
            let f = b as f32 * 10.0;
            let near = (1..=4).any(|h| (f - hertz * h as f32).abs() < 30.0);
            if !near {
                worst = worst.max(bin(b));
            }
        }
        (20.0 * (worst / fundamental).log10()) as f32
    }

    /// How loud a sine comes out, in dB against how it went in.
    fn response(settings: Settings, hertz: f32, level: f32) -> f32 {
        let mut reel = reel(settings);
        db(harmonic(&through(&mut reel, hertz, level, RATE as usize / 2), hertz) / level)
    }

    #[test]
    fn a_quiet_sound_comes_through_at_about_its_level_at_any_drive_or_saturation() {
        for drive in [0x00, 0x80, 0xFF] {
            for sat in [0x00, 0x80, 0xFF] {
                let gain = response(Settings { drive, sat, ..BARE }, 1_000.0, 0.01);
                assert!(gain.abs() < 1.0, "{drive:02X} {sat:02X}: {gain}");
            }
        }
        // Underbiased, a quiet sound barely moves the tape at all.
        assert!(response(Settings { bias: 0, ..BARE }, 1_000.0, 0.01) < -20.0);
    }

    #[test]
    fn drive_and_saturation_saturate_it_sooner_and_its_peaks_stay_held() {
        let distortion = |settings: Settings| {
            let mut reel = reel(settings);
            let out = through(&mut reel, 200.0, 0.5, RATE as usize / 2);
            assert!(out.iter().all(|s| s.abs() < HEADROOM));
            harmonic(&out, 600.0).hypot(harmonic(&out, 1_000.0)) / harmonic(&out, 200.0)
        };
        let (gentle, middle, driven) = (
            distortion(Settings {
                drive: 0x00,
                ..BARE
            }),
            distortion(BARE),
            distortion(Settings {
                drive: 0xFF,
                ..BARE
            }),
        );
        assert!(
            gentle < middle && middle * 4.0 < driven,
            "{gentle} {middle} {driven}"
        );
        assert!(distortion(Settings { sat: 0xFF, ..BARE }) > middle * 2.0);
        // Odd harmonics only: the loop is the same both ways.
        let mut reel = reel(Settings {
            drive: 0xFF,
            ..BARE
        });
        let out = through(&mut reel, 200.0, 0.5, RATE as usize / 2);
        assert!(harmonic(&out, 400.0) < harmonic(&out, 200.0) * 1e-4);
    }

    #[test]
    fn it_aliases_little() {
        for drive in [0x80, 0xFF] {
            let mut reel = reel(Settings { drive, ..BARE });
            let out = through(&mut reel, 5_010.0, 1.0, RATE as usize / 2);
            let found = aliasing(&out, 5_010.0);
            assert!(found < -45.0, "{drive:02X}: {found}");
        }
    }

    #[test]
    fn slower_tape_is_darker_and_its_bump_lower() {
        let at = |speed: u8, hertz: f32| response(Settings { speed, ..STILL }, hertz, 0.01);
        let highs: Vec<f32> = (0..4).map(|speed| at(speed, 10_000.0)).collect();
        assert!(
            highs.windows(2).all(|pair| pair[0] < pair[1] - 0.05),
            "{highs:?}"
        );
        assert!(highs[0] < -3.0 && highs[3] > -1.0, "{highs:?}");
        // The bump: at 15 ips near 60 Hz, at 30 ips near 120 Hz, a few dB up either way.
        assert!(at(2, 58.0) > at(2, 1_000.0) + 1.5);
        assert!(at(3, 117.0) > at(3, 1_000.0) + 1.5);
        assert!(at(3, 58.0) < at(3, 117.0) - 1.0);
    }

    #[test]
    fn each_loss_follows_its_formula() {
        let speed = 0.381; // 15 ips, in m/s
        let k = |hertz: f32| TAU * hertz / speed;
        let base = Settings {
            gap: 0,
            space: 0,
            thick: 0,
            speed: 2,
            ..STILL
        };
        // What a setting takes away, against none at the same speed.
        let lost = |settings: Settings, hertz: f32| {
            let none = Settings {
                speed: settings.speed,
                ..base
            };
            response(settings, hertz, 0.01) - response(none, hertz, 0.01)
        };
        // Spacing: Wallace's exp(-k d), within 1.5 dB down to -30 dB.
        let spacing = Settings {
            space: 0x80,
            ..base
        };
        let d = reel_spacing_um(128.0) * 1e-6;
        for hertz in [1_000.0, 2_000.0, 5_000.0, 10_000.0] {
            let wanted = db((-k(hertz) * d).exp());
            let found = lost(spacing, hertz);
            assert!((found - wanted).abs() < 1.5, "{hertz}: {found} {wanted}");
        }
        // Thickness: (1 - exp(-k δ)) / k δ, within 1.5 dB.
        let thickness = Settings {
            thick: 0xC0,
            ..base
        };
        let delta = reel_thickness_um(192.0) * 1e-6;
        let loss = |hertz: f32| (1.0 - (-k(hertz) * delta).exp()) / (k(hertz) * delta);
        for hertz in [1_000.0, 2_000.0, 5_000.0, 10_000.0] {
            let wanted = db(loss(hertz));
            let found = lost(thickness, hertz);
            assert!((found - wanted).abs() < 1.5, "{hertz}: {found} {wanted}");
        }
        // The gap: a null where the tape crosses it in a cycle, v / g, here at 3.75 ips.
        let gap = Settings {
            speed: 0,
            gap: 0xFF,
            ..base
        };
        let null = 0.095_25 / (vt_model::insert::MOST_REEL_GAP_UM * 1e-6);
        assert!(lost(gap, null) < -25.0, "{}", lost(gap, null));
        assert!(lost(gap, null * 0.5) > -6.0, "{}", lost(gap, null * 0.5));
    }

    /// How far a 1 kHz sine's pitch moves, from where it crosses zero going up.
    fn pitch_spread(settings: Settings, seconds: usize) -> f32 {
        let mut reel = reel(settings);
        let out = through(&mut reel, 1_000.0, 0.25, RATE as usize * seconds);
        let mut crossings = Vec::new();
        for at in RATE as usize / 2..out.len() {
            if out[at - 1] < 0.0 && out[at] >= 0.0 {
                crossings.push(at as f32 - out[at] / (out[at] - out[at - 1]));
            }
        }
        let periods: Vec<f32> = crossings
            .chunks(50)
            .filter(|found| found.len() == 50)
            .map(|found| found[49] - found[0])
            .collect();
        let (low, high) = periods.iter().fold((f32::MAX, 0.0_f32), |(low, high), p| {
            (low.min(*p), high.max(*p))
        });
        high / low - 1.0
    }

    #[test]
    fn wow_and_flutter_move_the_pitch() {
        assert!(pitch_spread(STILL, 2) < 1e-4);
        assert!(pitch_spread(Settings { wow: 0xFF, ..STILL }, 8) > 0.002);
        assert!(
            pitch_spread(
                Settings {
                    flutter: 0xFF,
                    ..STILL
                },
                2
            ) > 0.002
        );
    }

    #[test]
    fn the_same_sound_comes_out_the_same() {
        let render = || {
            let mut reel = reel(Settings {
                wow: 0xFF,
                flutter: 0xFF,
                drive: 0xC0,
                ..Settings::DEFAULT
            });
            through(&mut reel, 330.0, 0.7, RATE as usize)
        };
        assert!(
            render()
                .iter()
                .zip(render())
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let mut reel = reel(Settings::DEFAULT);
        let tone = |frame: usize| sine(frame, 100.0, 0.5, RATE);
        let mut last = [0.0; 2];
        for frame in 0..RATE as usize / 10 {
            last = reel.process([tone(frame); 2]);
        }
        for frame in 0..RATE as usize {
            let step = (frame / 64 % 2) as u8;
            reel.follow(Settings {
                drive: 0x20 + step * 0xC0,
                sat: 0x20 + step * 0xC0,
                bias: 0x40 + step * 0x80,
                speed: step * 3,
                gap: step * 0xFF,
                space: step * 0xFF,
                thick: step * 0xFF,
                wow: step * 0xFF,
                flutter: step * 0xFF,
                ..Settings::DEFAULT
            });
            let at = RATE as usize / 10 + frame;
            let out = reel.process([tone(at); 2]);
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
        for drive in [0x00, 0xFF] {
            for sat in [0x00, 0xFF] {
                for bias in [0x00, 0xFF] {
                    for (speed, loss) in [(0, 0xFF), (3, 0x00)] {
                        for (rate, wobble) in [(44_100, 0x00), (192_000, 0xFF)] {
                            let mut reel = Reel::new(rate);
                            reel.follow(Settings {
                                drive,
                                sat,
                                bias,
                                speed,
                                gap: loss,
                                space: loss,
                                thick: loss,
                                wow: wobble,
                                flutter: wobble,
                                ..Settings::DEFAULT
                            });
                            reel.settle();
                            for frame in 0..rate as usize / 4 {
                                // Full-scale noise, silence, a full-scale square and DC in turn,
                                // and far over full scale.
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
                                let out = reel.process([level, -level]);
                                assert!(
                                    out.iter().all(|s| s.is_finite() && s.abs() <= HEADROOM),
                                    "{drive:x} {sat:x} {bias:x} {speed} {rate}: {out:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn it_falls_to_nothing_in_silence() {
        let mut reel = reel(Settings::DEFAULT);
        through(&mut reel, 100.0, 1.0, RATE as usize / 4);
        let quiet = SlotEffect::quiet_frames(&reel, RATE) as usize;
        let mut last = [1.0; 2];
        for _ in 0..quiet {
            last = reel.process([0.0; 2]);
        }
        assert!(last.iter().all(|s| s.abs() < 1e-6), "{last:?}");
    }
}
