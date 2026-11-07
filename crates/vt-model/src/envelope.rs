//! ADSR envelopes, which each track has two of.

use serde::{Deserialize, Serialize};

use crate::constants::{MAX_ENVELOPE_MS, TRACK_ENVELOPES};
use crate::lfo::MAX_LFO_DEPTH;
use crate::settings::SettingId;

/// How far `fall` curves: the same as Mutable Instruments' Peaks.
const FALL_RATE: f32 = 4.0;
/// How steeply `rise` starts slowly, also from Peaks.
const RISE_POWER: f32 = 3.32;

/// Where an attack is at `t` from 0 to 1. It starts slowly, so a long one sounds even.
#[inline]
pub fn rise(t: f32) -> f32 {
    t.powf(RISE_POWER)
}

/// How far a decay or release has fallen at `t` from 0 to 1. Like a capacitor discharging, it's fast
/// and then slow, but scaled so it lands exactly at 1 and its time is exact.
#[inline]
pub fn fall(t: f32) -> f32 {
    (1.0 - (-FALL_RATE * t).exp()) / (1.0 - (-FALL_RATE).exp())
}

/// One envelope's settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub destination: Option<SettingId>,
    pub attack_ms: u16,
    pub decay_ms: u16,
    /// Percent of full, while a note holds.
    pub sustain: u8,
    pub release_ms: u16,
    /// How far full moves its destination, in that destination's units, with semitones for `Pitch`.
    /// Negative flips it. The first envelope on `Vol` shapes each note instead.
    pub depth: i16,
    /// Whether each note starts it from zero. Otherwise it goes on from where it is.
    pub reset: bool,
}

impl Default for Envelope {
    fn default() -> Self {
        Self::note_shape()
    }
}

impl Envelope {
    /// The shape a note has with nothing else set: a click-free start, and a short release.
    pub fn note_shape() -> Self {
        Self {
            destination: Some(SettingId::VOL),
            attack_ms: 1,
            decay_ms: 0,
            sustain: 100,
            release_ms: 50,
            depth: 0,
            reset: true,
        }
    }

    /// The second envelope's starting point: a decay on the filter, doing nothing until it has depth.
    pub fn filter_shape() -> Self {
        Self {
            destination: Some(SettingId::CUT),
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
            && self.depth.abs() <= MAX_LFO_DEPTH
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
            return from + (1.0 - from) * rise(frames as f32 / attack as f32);
        }
        let decay = frames_of(self.decay_ms, rate);
        let into_decay = frames - attack;
        let sustain = self.sustain_level();
        if into_decay < decay {
            return 1.0 + (sustain - 1.0) * fall(into_decay as f32 / decay as f32);
        }
        sustain
    }

    /// The level some frames after a release, starting from where it was.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let release = frames_of(self.release_ms, rate);
        if frames < release {
            from * (1.0 - fall(frames as f32 / release as f32))
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

/// What a new track's envelopes start as.
pub fn default_envelopes() -> [Envelope; TRACK_ENVELOPES] {
    [Envelope::note_shape(), Envelope::filter_shape()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Envelope {
        Envelope {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            ..Envelope::note_shape()
        }
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} and {b}");
    }

    #[test]
    fn the_curves_land_exactly_and_never_turn_back() {
        close(rise(0.0), 0.0);
        close(rise(1.0), 1.0);
        close(fall(0.0), 0.0);
        close(fall(1.0), 1.0);
        for i in 0..100 {
            let (a, b) = (i as f32 / 100.0, (i + 1) as f32 / 100.0);
            assert!(rise(a) <= rise(b));
            assert!(fall(a) <= fall(b));
        }
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
        let deep = Envelope {
            depth: MAX_LFO_DEPTH + 1,
            ..Envelope::default()
        };
        assert!(!deep.in_range());
    }
}
