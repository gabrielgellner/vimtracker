//! The envelope types of mod slots, ADSR and AHD, and the curves every envelope's stages follow.

use serde::{Deserialize, Serialize};

use crate::constants::MAX_ENVELOPE_MS;

/// The longest an AHD's hold can be timed, in milliseconds.
pub const MAX_HOLD_MS: u16 = 2000;

/// How far `Exp`'s decays and releases bend: the same as Mutable Instruments' Peaks.
const FALL_RATE: f32 = 4.0;
/// How far `Snap`'s bend, twice as far, so they fall about twice as fast and then have a longer
/// tail.
const SNAP_RATE: f32 = 8.0;
/// How steeply a bent attack starts slowly, also from Peaks.
const RISE_POWER: f32 = 3.32;
/// How far `Analog`'s attack bends: a capacitor charging toward 1.5 and switched to its decay at
/// 1, as Surge's analog envelope is, which is `1.5 (1 - 3^-t)`, exact at 1.
const ANALOG_RISE_RATE: f32 = 1.098_612_3; // ln 3
/// How far `Analog`'s decays and releases bend: a capacitor discharging, which would take its time
/// to fall 60 dB, as a reverb's time is given; scaled to land, as every fall is, it's 60 dB down
/// nine tenths of the way.
const ANALOG_FALL_RATE: f32 = 6.907_755; // ln 1000

/// How an envelope's stages bend, its mode (#46): one curve for the whole envelope rather than one
/// for each stage, as #46 suggested trying first. A decay or release is Vital's `powerScale`,
/// `(e^(p t) - 1) / (e^p - 1)`, of which Peaks' fall is the case `p = -4`, so `Exp` is the sound
/// envelopes always had.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum Curve {
    /// An attack that starts slowly, and falls that are fast and then slow, as an analog
    /// envelope's, from Peaks.
    #[default]
    Exp,
    /// Straight lines.
    Lin,
    /// `Exp`'s attack, and falls about twice as fast at first, with a longer tail: a snappier
    /// drum or pluck.
    Snap,
    /// A capacitor charging and discharging, as an analog envelope does: an attack that starts
    /// fast and slows into full, and falls that are true RC curves, 30 dB down halfway.
    Analog,
}

impl Curve {
    /// Every curve, in the order `=` goes through them.
    pub const ALL: [Self; 4] = [Self::Exp, Self::Lin, Self::Snap, Self::Analog];

    /// The name in the slot's title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Exp => "Exp",
            Self::Lin => "Lin",
            Self::Snap => "Snap",
            Self::Analog => "Analog",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Exp => "rises slowly, falls fast then slow",
            Self::Lin => "straight lines",
            Self::Snap => "falls twice as fast, then a long tail",
            Self::Analog => "rises fast and slows, as a capacitor charges",
        }
    }

    /// Where an attack is at `t` from 0 to 1. Bent, it starts slowly, so a long one sounds even;
    /// `Analog`'s starts fast and slows into full.
    #[inline]
    pub fn rise(self, t: f32) -> f32 {
        match self {
            Self::Lin => t,
            Self::Exp | Self::Snap => t.powf(RISE_POWER),
            Self::Analog => bent(ANALOG_RISE_RATE, t),
        }
    }

    /// How far through an attack it reaches `level`, from 0 to 1: [`Curve::rise`] turned round,
    /// so an attack started from a level can start where its curve has it (#467).
    #[inline]
    pub fn rise_time(self, level: f32) -> f32 {
        let level = level.clamp(0.0, 1.0);
        match self {
            Self::Lin => level,
            Self::Exp | Self::Snap => level.powf(RISE_POWER.recip()),
            Self::Analog => unbent(ANALOG_RISE_RATE, level),
        }
    }

    /// How far a decay or release has fallen at `t` from 0 to 1. Bent, it's like a capacitor
    /// discharging, fast and then slow, but scaled so it lands exactly at 1 and its time is exact.
    #[inline]
    pub fn fall(self, t: f32) -> f32 {
        match self {
            Self::Exp => bent(FALL_RATE, t),
            Self::Lin => t,
            Self::Snap => bent(SNAP_RATE, t),
            Self::Analog => bent(ANALOG_FALL_RATE, t),
        }
    }
}

/// Vital's `powerScale` at `p = -rate`, written as Peaks writes its fall.
#[inline]
fn bent(rate: f32, t: f32) -> f32 {
    (1.0 - (-rate * t).exp()) / (1.0 - (-rate).exp())
}

/// Where [`bent`] reaches `level`.
#[inline]
fn unbent(rate: f32, level: f32) -> f32 {
    (-(1.0 - level * (1.0 - (-rate).exp())).ln() / rate).min(1.0)
}

/// Where an attack is some frames after a note: rising, this share of the way through it, or done
/// so many frames ago.
enum Attack {
    Rising(f32),
    Done(u64),
}

/// Where an attack started from `from` rather than from nothing is: from where its curve reaches
/// that level (#467), so it keeps its shape and is only as long as what's left of it, as an analog
/// envelope's capacitor charges on from where it is, and the stages after it come as much sooner.
/// From nothing, it's worked out exactly as it always was.
#[inline]
fn attack_at(curve: Curve, attack: u64, frames: u64, from: f32) -> Attack {
    if from <= 0.0 || attack == 0 {
        return match frames.checked_sub(attack) {
            None => Attack::Rising(frames as f32 / attack as f32),
            Some(after) => Attack::Done(after),
        };
    }
    let start = curve.rise_time(from);
    let left = ((1.0 - start) * attack as f32).round() as u64;
    match frames.checked_sub(left) {
        None => Attack::Rising(start + frames as f32 / attack as f32),
        Some(after) => Attack::Done(after),
    }
}

/// An ADSR envelope's own settings: its times, its sustain and whether each note starts it from
/// nothing. Where it goes and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adsr {
    pub attack_ms: u16,
    pub decay_ms: u16,
    /// Percent of full, while a note holds.
    pub sustain: u8,
    pub release_ms: u16,
    /// Whether each note starts it from zero. Otherwise it goes on from where it is.
    pub reset: bool,
    /// How its stages bend, its mode.
    #[serde(default)]
    pub curve: Curve,
}

/// A modulating envelope's starting point: a decay to nothing, moving nothing until it has depth.
impl Default for Adsr {
    fn default() -> Self {
        Self::decay()
    }
}

impl Adsr {
    /// The shape a note has with nothing else set: a click-free start, and a short release.
    pub const fn note_shape() -> Self {
        Self {
            attack_ms: 1,
            decay_ms: 0,
            sustain: 100,
            release_ms: 50,
            reset: true,
            curve: Curve::Exp,
        }
    }

    /// A modulating envelope's starting point: the note shape's attack and release, with a decay
    /// to nothing, as a filter's envelope has.
    pub const fn decay() -> Self {
        Self {
            decay_ms: 200,
            sustain: 0,
            ..Self::note_shape()
        }
    }

    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        [self.attack_ms, self.decay_ms, self.release_ms]
            .iter()
            .all(|ms| *ms <= MAX_ENVELOPE_MS)
            && self.sustain <= 100
    }

    /// The sustain level, from 0 to 1.
    #[inline]
    pub fn sustain_level(&self) -> f32 {
        f32::from(self.sustain) / 100.0
    }

    /// The level some frames after a note, starting from where it was: an attack from a level
    /// starts where its curve reaches it.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let into_decay = match attack_at(self.curve, frames_of(self.attack_ms, rate), frames, from)
        {
            Attack::Rising(at) => return self.curve.rise(at),
            Attack::Done(after) => after,
        };
        let decay = frames_of(self.decay_ms, rate);
        let sustain = self.sustain_level();
        if into_decay < decay {
            return 1.0 + (sustain - 1.0) * self.curve.fall(into_decay as f32 / decay as f32);
        }
        sustain
    }

    /// The level some frames after a release, starting from where it was.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let release = frames_of(self.release_ms, rate);
        if frames < release {
            from * (1.0 - self.curve.fall(frames as f32 / release as f32))
        } else {
            0.0
        }
    }

    /// Whether a held note has gone silent for good, with nothing to release.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        self.sustain == 0
            && frames >= frames_of(self.attack_ms, rate) + frames_of(self.decay_ms, rate)
    }

    /// Whether a release has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        frames >= frames_of(self.release_ms, rate)
    }
}

/// How long an AHD holds at full before it decays (#85).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hold {
    /// While the note lasts, then the decay from its end, as the Digitakt's `HOLD NOTE`.
    Note,
    /// So many milliseconds, whatever the note's length: a one-shot, as the M8's AHD and a drum's
    /// punch.
    Ms(u16),
}

/// An AHD envelope's own settings (#85): an attack to full, a hold there for a time or while the
/// note lasts, and a decay to nothing, with the same reset and curve as an ADSR's. Where it goes
/// and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ahd {
    pub attack_ms: u16,
    pub hold: Hold,
    pub decay_ms: u16,
    /// Whether each note starts it from zero. Otherwise it goes on from where it is.
    pub reset: bool,
    /// How its stages bend, its mode.
    pub curve: Curve,
}

/// A punch: a click-free start, 30 ms at full, and a short decay.
impl Default for Ahd {
    fn default() -> Self {
        Self {
            attack_ms: 1,
            hold: Hold::Ms(30),
            decay_ms: 200,
            reset: true,
            curve: Curve::Exp,
        }
    }
}

impl Ahd {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.attack_ms <= MAX_ENVELOPE_MS
            && self.decay_ms <= MAX_ENVELOPE_MS
            && match self.hold {
                Hold::Note => true,
                Hold::Ms(ms) => ms <= MAX_HOLD_MS,
            }
    }

    /// Whether it plays through whatever the note's length, its hold being a time.
    #[inline]
    pub fn one_shot(&self) -> bool {
        matches!(self.hold, Hold::Ms(_))
    }

    /// The level some frames after a note, starting from where it was: its attack, from where its
    /// curve reaches that level, its hold, then, for a timed hold, its decay.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let into_hold = match attack_at(self.curve, frames_of(self.attack_ms, rate), frames, from) {
            Attack::Rising(at) => return self.curve.rise(at),
            Attack::Done(after) => after,
        };
        let Hold::Ms(hold) = self.hold else {
            return 1.0;
        };
        let hold = frames_of(hold, rate);
        let Some(into_decay) = into_hold.checked_sub(hold) else {
            return 1.0;
        };
        let decay = frames_of(self.decay_ms, rate);
        if into_decay < decay {
            1.0 - self.curve.fall(into_decay as f32 / decay as f32)
        } else {
            0.0
        }
    }

    /// The level some frames after the note ends, starting from where it was: a hold of `Note`
    /// decays from there. A one-shot is never released, as it plays through.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let decay = frames_of(self.decay_ms, rate);
        if frames < decay {
            from * (1.0 - self.curve.fall(frames as f32 / decay as f32))
        } else {
            0.0
        }
    }

    /// Whether a held note has gone silent for good: a one-shot once its decay is done.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        match self.hold {
            Hold::Note => false,
            Hold::Ms(hold) => {
                frames
                    >= frames_of(self.attack_ms, rate)
                        + frames_of(hold, rate)
                        + frames_of(self.decay_ms, rate)
            }
        }
    }

    /// Whether the decay from a note's end has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        frames >= frames_of(self.decay_ms, rate)
    }
}

/// An envelope of either type, as a note's shape and a slot's run follow it: small and `Copy`, so
/// a voice keeps the one it started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    Adsr(Adsr),
    Ahd(Ahd),
}

impl Envelope {
    /// The shape a note has with nothing else set.
    pub const fn note_shape() -> Self {
        Self::Adsr(Adsr::note_shape())
    }

    /// Whether each note starts it from zero.
    #[inline]
    pub fn reset(&self) -> bool {
        match self {
            Self::Adsr(adsr) => adsr.reset,
            Self::Ahd(ahd) => ahd.reset,
        }
    }

    /// Whether it plays through whatever the note's length, so a note's end doesn't release it.
    #[inline]
    pub fn one_shot(&self) -> bool {
        match self {
            Self::Adsr(_) => false,
            Self::Ahd(ahd) => ahd.one_shot(),
        }
    }

    /// The level some frames after a note, starting from where it was.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        match self {
            Self::Adsr(adsr) => adsr.held(frames, from, rate),
            Self::Ahd(ahd) => ahd.held(frames, from, rate),
        }
    }

    /// The level some frames after a release, starting from where it was.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        match self {
            Self::Adsr(adsr) => adsr.released(frames, from, rate),
            Self::Ahd(ahd) => ahd.released(frames, from, rate),
        }
    }

    /// Whether a held note has gone silent for good, with nothing to release.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        match self {
            Self::Adsr(adsr) => adsr.finished_holding(frames, rate),
            Self::Ahd(ahd) => ahd.finished_holding(frames, rate),
        }
    }

    /// Whether a release has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        match self {
            Self::Adsr(adsr) => adsr.finished_releasing(frames, rate),
            Self::Ahd(ahd) => ahd.finished_releasing(frames, rate),
        }
    }
}

#[inline]
fn frames_of(ms: u16, rate: u32) -> u64 {
    u64::from(ms) * u64::from(rate) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Adsr {
        Adsr {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            ..Adsr::note_shape()
        }
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} and {b}");
    }

    #[test]
    fn the_curves_land_exactly_and_never_turn_back() {
        for curve in Curve::ALL {
            close(curve.rise(0.0), 0.0);
            close(curve.rise(1.0), 1.0);
            close(curve.fall(0.0), 0.0);
            close(curve.fall(1.0), 1.0);
            for i in 0..100 {
                let (a, b) = (i as f32 / 100.0, (i + 1) as f32 / 100.0);
                assert!(curve.rise(a) <= curve.rise(b), "{curve:?}");
                assert!(curve.fall(a) <= curve.fall(b), "{curve:?}");
            }
        }
    }

    #[test]
    fn exp_is_peaks_curves_to_the_bit() {
        for i in 0..=100 {
            let t = i as f32 / 100.0;
            let peaks = (1.0 - (-4.0 * t).exp()) / (1.0 - (-4.0_f32).exp());
            assert_eq!(Curve::Exp.fall(t).to_bits(), peaks.to_bits());
            assert_eq!(Curve::Exp.rise(t).to_bits(), t.powf(3.32).to_bits());
        }
    }

    #[test]
    fn lin_is_straight_and_snap_falls_twice_as_fast_then_tails() {
        for i in 0..=10 {
            let t = i as f32 / 10.0;
            close(Curve::Lin.rise(t), t);
            close(Curve::Lin.fall(t), t);
        }
        // Snap starts falling about twice as fast as Exp, and is further down all the way.
        let slope = |curve: Curve| curve.fall(1e-3) / 1e-3;
        let ratio = slope(Curve::Snap) / slope(Curve::Exp);
        assert!((1.9..2.1).contains(&ratio), "{ratio}");
        for i in 1..10 {
            let t = i as f32 / 10.0;
            assert!(Curve::Snap.fall(t) > Curve::Exp.fall(t));
            assert!(Curve::Exp.fall(t) > Curve::Lin.fall(t));
        }
        // Their attacks are the same.
        assert_eq!(Curve::Snap.rise(0.3), Curve::Exp.rise(0.3));
    }

    #[test]
    fn halfway_through_a_decay_each_curve_is_its_own_depth_down() {
        let db = |curve| {
            let env = Adsr {
                curve,
                ..adsr(0, 1000, 0, 50)
            };
            20.0 * env.held(24_000, 0.0, RATE).log10()
        };
        assert!((db(Curve::Lin) + 6.0).abs() < 0.1, "{}", db(Curve::Lin));
        assert!((db(Curve::Snap) + 34.9).abs() < 0.2, "{}", db(Curve::Snap));
        assert!(db(Curve::Snap) < db(Curve::Exp));
    }

    #[test]
    fn an_envelope_saved_before_curves_loads_as_exp() {
        let text = "(attack_ms:1,decay_ms:200,sustain:0,release_ms:50,reset:true)";
        let env: Adsr = ron::from_str(text).unwrap();
        assert_eq!(env, Adsr::decay());
        assert_eq!(env.curve, Curve::Exp);
        let snap = Adsr {
            curve: Curve::Snap,
            ..Adsr::decay()
        };
        let text = ron::to_string(&snap).unwrap();
        assert!(text.contains("curve:Snap"), "{text}");
        assert_eq!(ron::from_str::<Adsr>(&text).unwrap(), snap);
    }

    #[test]
    fn a_decay_falls_at_the_rate_peaks_uses() {
        // Halfway through a decay to nothing is about 18.5 dB down, where a straight line is 6.
        let env = adsr(0, 1000, 0, 50);
        let level = env.held(24_000, 0.0, RATE);
        let db = 20.0 * level.log10();
        assert!((db + 18.5).abs() < 0.5, "{db}");
    }

    #[test]
    fn stages_run_in_order_and_reach_their_levels() {
        let env = adsr(10, 10, 50, 10);
        close(env.held(0, 0.0, RATE), 0.0);
        close(env.held(480, 0.0, RATE), 1.0);
        close(env.held(960, 0.0, RATE), 0.5);
        close(env.held(100_000, 0.0, RATE), 0.5);
        close(env.released(0, 0.5, RATE), 0.5);
        close(env.released(480, 0.5, RATE), 0.0);
    }

    #[test]
    fn zero_length_stages_are_skipped_safely() {
        let env = adsr(0, 0, 100, 0);
        close(env.held(0, 0.0, RATE), 1.0);
        close(env.released(0, 1.0, RATE), 0.0);
        assert!(env.finished_releasing(0, RATE));
        let dropped = adsr(0, 0, 40, 0);
        close(dropped.held(0, 0.0, RATE), 0.4);
        assert!(adsr(0, 0, 0, 0).finished_holding(0, RATE));
    }

    #[test]
    fn an_attack_can_start_from_where_it_was() {
        let env = adsr(10, 0, 100, 0);
        close(env.held(0, 0.6, RATE), 0.6);
        close(env.held(480, 0.6, RATE), 1.0);
        assert!(env.held(240, 0.6, RATE) > 0.6);
    }

    #[test]
    fn analog_charges_toward_one_and_a_half_and_discharges_60_db_in_its_time() {
        for i in 0..=20 {
            let t = i as f32 / 20.0;
            close(Curve::Analog.rise(t), 1.5 * (1.0 - 3.0_f32.powf(-t)));
        }
        // It starts fast, where Exp starts slowly: a quarter of the way, it's most of the way up.
        assert!(Curve::Analog.rise(0.25) > 0.35 && Curve::Exp.rise(0.25) < 0.02);
        // A true RC fall: halfway, 30 dB down, and 60 dB nine tenths of the way, as it lands.
        let db = |t: f32| 20.0 * (1.0 - Curve::Analog.fall(t)).log10();
        assert!((db(0.5) + 30.0).abs() < 0.5, "{}", db(0.5));
        assert!((db(0.9) + 60.0).abs() < 0.5, "{}", db(0.9));
    }

    #[test]
    fn every_curves_rise_time_turns_its_rise_round() {
        for curve in Curve::ALL {
            for i in 0..=20 {
                let t = i as f32 / 20.0;
                close(curve.rise_time(curve.rise(t)), t);
            }
            close(curve.rise_time(0.0), 0.0);
            close(curve.rise_time(1.0), 1.0);
        }
    }

    #[test]
    fn an_attack_from_a_level_keeps_its_shape_and_only_what_is_left_of_its_time() {
        for curve in Curve::ALL {
            let env = Adsr {
                curve,
                ..adsr(100, 100, 0, 50)
            };
            let from = 0.6;
            // It starts where it was, and is as far on as a fresh attack is where it reaches that.
            close(env.held(0, from, RATE), from);
            let skipped = (curve.rise_time(from) * 4800.0).round() as u64;
            for frames in [100, 700, 1500, 4000, 6000, 9000] {
                close(
                    env.held(frames, from, RATE),
                    env.held(frames + skipped, 0.0, RATE),
                );
            }
            // So it reaches full that much sooner, and its decay starts then.
            close(env.held(4800 - skipped, from, RATE), 1.0);
            assert!(env.held(4800, from, RATE) < 1.0 - 1e-3, "{curve:?}");
        }
        // At full already, it goes straight into its decay.
        let env = adsr(100, 100, 0, 50);
        assert!(env.held(10, 1.0, RATE) < 1.0);
        // An AHD's does the same.
        let ahd = ahd(100, Hold::Ms(10), 100);
        let skipped = (Curve::Exp.rise_time(0.5) * 4800.0).round() as u64;
        close(ahd.held(0, 0.5, RATE), 0.5);
        close(
            ahd.held(5000, 0.5, RATE),
            ahd.held(5000 + skipped, 0.0, RATE),
        );
    }

    #[test]
    fn an_attack_from_nothing_is_worked_out_as_it_always_was() {
        let env = adsr(10, 0, 100, 0);
        for frames in 0..480 {
            let before = Curve::Exp.rise(frames as f32 / 480.0);
            assert_eq!(env.held(frames, 0.0, RATE).to_bits(), before.to_bits());
        }
    }

    #[test]
    fn a_held_note_only_finishes_if_it_decays_to_nothing() {
        assert!(!adsr(1, 10, 50, 10).finished_holding(10_000_000, RATE));
        let drum = adsr(1, 10, 0, 10);
        assert!(!drum.finished_holding(100, RATE));
        assert!(drum.finished_holding(1000, RATE));
    }

    #[test]
    fn settings_out_of_range_are_found() {
        assert!(adsr(MAX_ENVELOPE_MS, MAX_ENVELOPE_MS, 100, MAX_ENVELOPE_MS).in_range());
        assert!(!adsr(MAX_ENVELOPE_MS + 1, 0, 100, 0).in_range());
        assert!(!adsr(0, 0, 101, 0).in_range());
    }

    fn ahd(attack_ms: u16, hold: Hold, decay_ms: u16) -> Ahd {
        Ahd {
            attack_ms,
            hold,
            decay_ms,
            ..Ahd::default()
        }
    }

    #[test]
    fn an_ahd_with_a_timed_hold_rises_holds_and_decays_whatever_the_note() {
        let env = ahd(10, Hold::Ms(20), 10);
        assert!(env.one_shot());
        close(env.held(0, 0.0, RATE), 0.0);
        close(env.held(480, 0.0, RATE), 1.0);
        close(env.held(480 + 959, 0.0, RATE), 1.0);
        let halfway = env.held(480 + 960 + 240, 0.0, RATE);
        assert!(halfway > 0.0 && halfway < 0.5, "{halfway}");
        close(env.held(480 + 960 + 480, 0.0, RATE), 0.0);
        assert!(!env.finished_holding(480 + 960 + 479, RATE));
        assert!(env.finished_holding(480 + 960 + 480, RATE));
        // Its curve bends its attack and decay as an ADSR's.
        let lin = Ahd {
            curve: Curve::Lin,
            ..env
        };
        close(lin.held(240, 0.0, RATE), 0.5);
        close(lin.held(480 + 960 + 240, 0.0, RATE), 0.5);
    }

    #[test]
    fn an_ahd_holding_for_the_note_decays_from_its_end() {
        let env = ahd(10, Hold::Note, 10);
        assert!(!env.one_shot());
        close(env.held(480, 0.0, RATE), 1.0);
        close(env.held(10_000_000, 0.0, RATE), 1.0);
        assert!(!env.finished_holding(10_000_000, RATE));
        // The note's end starts its decay from wherever it is.
        close(env.released(0, 0.7, RATE), 0.7);
        assert!(env.released(240, 0.7, RATE) < 0.35);
        close(env.released(480, 0.7, RATE), 0.0);
        assert!(env.finished_releasing(480, RATE));
        // Its attack can start from where it was.
        close(env.held(0, 0.6, RATE), 0.6);
    }

    #[test]
    fn an_ahd_with_no_times_is_safe() {
        let env = ahd(0, Hold::Ms(0), 0);
        close(env.held(0, 0.0, RATE), 0.0);
        assert!(env.finished_holding(0, RATE));
        let note = ahd(0, Hold::Note, 0);
        close(note.held(0, 0.0, RATE), 1.0);
        close(note.released(0, 1.0, RATE), 0.0);
    }

    #[test]
    fn an_ahds_hold_is_saved_as_a_time_or_note_and_its_range_checked() {
        for env in [Ahd::default(), ahd(5, Hold::Note, 300)] {
            let text = ron::to_string(&env).unwrap();
            assert_eq!(ron::from_str::<Ahd>(&text).unwrap(), env);
        }
        assert!(
            ron::to_string(&Ahd::default())
                .unwrap()
                .contains("hold:Ms(30)")
        );
        assert!(ahd(0, Hold::Ms(MAX_HOLD_MS), MAX_ENVELOPE_MS).in_range());
        assert!(!ahd(0, Hold::Ms(MAX_HOLD_MS + 1), 0).in_range());
        assert!(!ahd(MAX_ENVELOPE_MS + 1, Hold::Note, 0).in_range());
    }

    #[test]
    fn either_envelope_plays_as_its_own_type() {
        let (four, three) = (adsr(10, 10, 50, 10), ahd(10, Hold::Ms(5), 10));
        let (adsr, ahd) = (Envelope::Adsr(four), Envelope::Ahd(three));
        for (frames, from) in [(0, 0.0), (300, 0.2), (700, 0.0), (5000, 0.0)] {
            assert_eq!(adsr.held(frames, from, RATE), four.held(frames, from, RATE));
            assert_eq!(ahd.held(frames, from, RATE), three.held(frames, from, RATE));
            assert_eq!(
                ahd.released(frames, 0.5, RATE),
                three.released(frames, 0.5, RATE)
            );
        }
        assert!(ahd.one_shot() && !adsr.one_shot());
        assert!(ahd.reset() && adsr.reset());
        assert_eq!(Envelope::note_shape(), Envelope::Adsr(Adsr::note_shape()));
    }
}
