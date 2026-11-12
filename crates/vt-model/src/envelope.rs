//! ADSR envelopes, one of a mod slot's types, and the curves every envelope's stages follow.

use serde::{Deserialize, Serialize};

use crate::constants::MAX_ENVELOPE_MS;

/// How far `Exp`'s decays and releases bend: the same as Mutable Instruments' Peaks.
const FALL_RATE: f32 = 4.0;
/// How far `Snap`'s bend, twice as far, so they fall about twice as fast and then have a longer
/// tail.
const SNAP_RATE: f32 = 8.0;
/// How steeply a bent attack starts slowly, also from Peaks.
const RISE_POWER: f32 = 3.32;

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
}

impl Curve {
    /// Every curve, in the order `=` goes through them.
    pub const ALL: [Self; 3] = [Self::Exp, Self::Lin, Self::Snap];

    /// The name in the slot's title.
    pub fn name(self) -> &'static str {
        match self {
            Self::Exp => "Exp",
            Self::Lin => "Lin",
            Self::Snap => "Snap",
        }
    }

    /// What it does, for a list to choose from.
    pub fn meaning(self) -> &'static str {
        match self {
            Self::Exp => "falls fast then slow, as an analog envelope",
            Self::Lin => "straight lines",
            Self::Snap => "falls twice as fast, then a long tail",
        }
    }

    /// Where an attack is at `t` from 0 to 1. Bent, it starts slowly, so a long one sounds even.
    #[inline]
    pub fn rise(self, t: f32) -> f32 {
        match self {
            Self::Lin => t,
            Self::Exp | Self::Snap => t.powf(RISE_POWER),
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
        }
    }
}

/// Vital's `powerScale` at `p = -rate`, written as Peaks writes its fall.
#[inline]
fn bent(rate: f32, t: f32) -> f32 {
    (1.0 - (-rate * t).exp()) / (1.0 - (-rate).exp())
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

    /// The level some frames after a note, starting from where it was.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let attack = frames_of(self.attack_ms, rate);
        if frames < attack {
            return from + (1.0 - from) * self.curve.rise(frames as f32 / attack as f32);
        }
        let decay = frames_of(self.decay_ms, rate);
        let into_decay = frames - attack;
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
}
