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

/// The most times a drum envelope's peak strikes before its body.
pub const MAX_RATCHET: u8 = 8;

/// A drum envelope's own settings, the M8's Drum env: a fall from full to its `Body` over `Peak`,
/// struck `Ratchet` times, then a decay from the body to nothing over `Decay`. It's a one-shot,
/// for a kick's or a tom's pitch, the click's sweep then the note's drop, or with a few strikes, a
/// clap's or a flam's. Where it goes and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drum {
    pub peak_ms: u16,
    /// Percent of full the peak falls to, where the decay starts.
    pub body: u8,
    pub decay_ms: u16,
    /// How many times the peak strikes, each `Peak` long, before the body decays: 1 for a drum,
    /// 3 or 4 for an 808's clap (#45, after the review's MSEG ratchet).
    pub ratchet: u8,
    /// How its body's decay bends, its mode. The peak's fall is always steep.
    pub curve: Curve,
}

/// A kick's pitch: a 10 ms click falling to 40%, then a 300 ms drop.
impl Default for Drum {
    fn default() -> Self {
        Self {
            peak_ms: 10,
            body: 40,
            decay_ms: 300,
            ratchet: 1,
            curve: Curve::Exp,
        }
    }
}

impl Drum {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.peak_ms <= MAX_ENVELOPE_MS
            && self.decay_ms <= MAX_ENVELOPE_MS
            && self.body <= 100
            && (1..=MAX_RATCHET).contains(&self.ratchet)
    }

    /// Its body's level, from 0 to 1.
    #[inline]
    pub fn body_level(&self) -> f32 {
        f32::from(self.body) / 100.0
    }

    /// Its level `share` of the way through one strike of its peak, from 0 to 1: a fall from full
    /// to its body, always as steep as `Snap`'s, whatever its curve.
    #[inline]
    pub fn strike(&self, share: f32) -> f32 {
        1.0 + (self.body_level() - 1.0) * bent(SNAP_RATE, share)
    }

    /// One strike's frames, and all of them.
    #[inline]
    fn strikes(&self, rate: u32) -> (u64, u64) {
        let peak = frames_of(self.peak_ms, rate);
        (peak, peak * u64::from(self.ratchet.max(1)))
    }

    /// The level some frames after a note. It always strikes from full, so where it was doesn't
    /// matter, and it has no reset.
    #[inline]
    pub fn held(&self, frames: u64, rate: u32) -> f32 {
        let (peak, struck) = self.strikes(rate);
        let body = self.body_level();
        if frames < struck {
            return self.strike((frames % peak) as f32 / peak as f32);
        }
        let into_decay = frames - struck;
        let decay = frames_of(self.decay_ms, rate);
        if into_decay < decay {
            body * (1.0 - self.curve.fall(into_decay as f32 / decay as f32))
        } else {
            0.0
        }
    }

    /// The level some frames after a release, which a one-shot never has: a decay from where it
    /// was, as an AHD's.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        let decay = frames_of(self.decay_ms, rate);
        if frames < decay {
            from * (1.0 - self.curve.fall(frames as f32 / decay as f32))
        } else {
            0.0
        }
    }

    /// Whether a note has gone silent for good: once its body has decayed.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        frames >= self.strikes(rate).1 + frames_of(self.decay_ms, rate)
    }

    /// Whether a release has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        frames >= frames_of(self.decay_ms, rate)
    }
}

/// How much faster a low-pass gate's vactrol falls near full than near silence: Plaits' ratio
/// with its decay knob at the middle, `10 · 2^(-2 · 0.5)`.
const VACTROL_RATIO: f32 = 5.0;
/// How far down a low-pass gate's `Decay` is timed to, 60 dB, below which it's scaled to land.
const VACTROL_FLOOR: f32 = 0.001;
/// Plaits' brightness for a vactrol's level `v`, `0.003 + 0.3 v⁴` of the rate, at silence and at
/// full.
const DARKEST: f32 = 0.003;
const BRIGHTEST: f32 = 0.303;

/// A low-pass gate's own settings (#45): a vactrol pinged by each note, after Mutable
/// Instruments' Plaits (`LPGEnvelope`, MIT): a quick rise, then a fall that slows as it nears
/// silence, which also closes the track's filter by `Colour`, so one slot dulls a note as it dies.
/// Where it goes and how far are its mod slot's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lpg {
    pub attack_ms: u16,
    /// How long it takes to fall 60 dB, where it's scaled to land.
    pub decay_ms: u16,
    /// How far its fall also closes the track's filter, in `Cut`'s units at silence: `00` moves
    /// only what it reaches, `FF` closes the filter all the way.
    pub colour: u8,
    /// Whether each note starts it from zero. Otherwise it goes on from where it is.
    pub reset: bool,
}

/// A pluck: a click-free start, a 400 ms fall, and most of the filter closing with it.
impl Default for Lpg {
    fn default() -> Self {
        Self {
            attack_ms: 1,
            decay_ms: 400,
            colour: 0xC0,
            reset: true,
        }
    }
}

impl Lpg {
    /// Whether these are settings that can be set.
    pub fn in_range(&self) -> bool {
        self.attack_ms <= MAX_ENVELOPE_MS && self.decay_ms <= MAX_ENVELOPE_MS
    }

    /// Its level some seconds into its fall from full. Plaits' vactrol falls by
    /// `dv/dt = -(c - (c - a) v⁴) v`, at the rate `a` near full and `c` near silence, stepped once
    /// a block at its one sample rate; here it's that equation solved, `v⁴ = E / (R + (1 - R) E)`
    /// with `E = e^(-4 c t)` and `R = a / c`, so it's the same at any rate and costs an `exp` and
    /// two square roots. `c` is chosen so it's 60 dB down at its `Decay`, and it's scaled to land
    /// there, as every fall is.
    #[inline]
    pub fn fall(&self, seconds: f32) -> f32 {
        let decay = f32::from(self.decay_ms) / 1000.0;
        if seconds >= decay {
            return 0.0;
        }
        let near_silence = (-VACTROL_FLOOR.ln() - 0.25 * VACTROL_RATIO.ln()) / decay;
        let e = (-4.0 * near_silence * seconds).exp();
        let level = (e / (VACTROL_RATIO + (1.0 - VACTROL_RATIO) * e))
            .sqrt()
            .sqrt();
        ((level - VACTROL_FLOOR) / (1.0 - VACTROL_FLOOR)).max(0.0)
    }

    /// How far its level `v` has closed the filter, from 0 at full to 1 at silence: Plaits'
    /// brightness, `0.003 + 0.3 v⁴`, in pitch, as the track's `Cut` is.
    #[inline]
    pub fn closing(level: f32) -> f32 {
        let v2 = level * level;
        let bright = DARKEST + (BRIGHTEST - DARKEST) * v2 * v2;
        ((BRIGHTEST / bright).ln() / (BRIGHTEST / DARKEST).ln()).clamp(0.0, 1.0)
    }

    /// The level some frames after a note, starting from where it was: a straight rise, from
    /// where it reaches that level, as Plaits' ramp is, then its fall.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        match attack_at(Curve::Lin, frames_of(self.attack_ms, rate), frames, from) {
            Attack::Rising(at) => at,
            Attack::Done(after) => self.fall(after as f32 / rate as f32),
        }
    }

    /// The level some frames after a release, which a one-shot never has: its fall from where it
    /// was.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        from * self.fall(frames as f32 / rate as f32)
    }

    /// Whether a note has gone silent for good: once its fall has landed.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        frames >= frames_of(self.attack_ms, rate) + frames_of(self.decay_ms, rate)
    }

    /// Whether a release has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        frames >= frames_of(self.decay_ms, rate)
    }
}

/// An envelope of any type, as a note's shape and a slot's run follow it: small and `Copy`, so a
/// voice keeps the one it started with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Envelope {
    Adsr(Adsr),
    Ahd(Ahd),
    Drum(Drum),
    Lpg(Lpg),
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
            Self::Drum(_) => true,
            Self::Lpg(lpg) => lpg.reset,
        }
    }

    /// Whether it plays through whatever the note's length, so a note's end doesn't release it.
    #[inline]
    pub fn one_shot(&self) -> bool {
        match self {
            Self::Adsr(_) => false,
            Self::Ahd(ahd) => ahd.one_shot(),
            Self::Drum(_) | Self::Lpg(_) => true,
        }
    }

    /// The level some frames after a note, starting from where it was.
    #[inline]
    pub fn held(&self, frames: u64, from: f32, rate: u32) -> f32 {
        match self {
            Self::Adsr(adsr) => adsr.held(frames, from, rate),
            Self::Ahd(ahd) => ahd.held(frames, from, rate),
            Self::Drum(drum) => drum.held(frames, rate),
            Self::Lpg(lpg) => lpg.held(frames, from, rate),
        }
    }

    /// The level some frames after a release, starting from where it was.
    #[inline]
    pub fn released(&self, frames: u64, from: f32, rate: u32) -> f32 {
        match self {
            Self::Adsr(adsr) => adsr.released(frames, from, rate),
            Self::Ahd(ahd) => ahd.released(frames, from, rate),
            Self::Drum(drum) => drum.released(frames, from, rate),
            Self::Lpg(lpg) => lpg.released(frames, from, rate),
        }
    }

    /// Whether a held note has gone silent for good, with nothing to release.
    #[inline]
    pub fn finished_holding(&self, frames: u64, rate: u32) -> bool {
        match self {
            Self::Adsr(adsr) => adsr.finished_holding(frames, rate),
            Self::Ahd(ahd) => ahd.finished_holding(frames, rate),
            Self::Drum(drum) => drum.finished_holding(frames, rate),
            Self::Lpg(lpg) => lpg.finished_holding(frames, rate),
        }
    }

    /// Whether a release has reached silence.
    #[inline]
    pub fn finished_releasing(&self, frames: u64, rate: u32) -> bool {
        match self {
            Self::Adsr(adsr) => adsr.finished_releasing(frames, rate),
            Self::Ahd(ahd) => ahd.finished_releasing(frames, rate),
            Self::Drum(drum) => drum.finished_releasing(frames, rate),
            Self::Lpg(lpg) => lpg.finished_releasing(frames, rate),
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

    fn drum(peak_ms: u16, body: u8, decay_ms: u16, ratchet: u8) -> Drum {
        Drum {
            peak_ms,
            body,
            decay_ms,
            ratchet,
            ..Drum::default()
        }
    }

    #[test]
    fn a_drum_falls_steeply_from_full_to_its_body_then_decays_whatever_the_note() {
        let env = drum(10, 40, 100, 1);
        close(env.held(0, RATE), 1.0);
        // The peak's fall is steep: most of the way down by half its time.
        let half = env.held(240, RATE);
        assert!(half < 0.5 && half > 0.4, "{half}");
        close(env.held(480, RATE), 0.4);
        // Then the body decays on its curve, and lands.
        let halfway = env.held(480 + 2400, RATE);
        assert!(halfway > 0.0 && halfway < 0.2, "{halfway}");
        close(env.held(480 + 4800, RATE), 0.0);
        assert!(!env.finished_holding(480 + 4799, RATE));
        assert!(env.finished_holding(480 + 4800, RATE));
        let lin = Drum {
            curve: Curve::Lin,
            ..env
        };
        close(lin.held(480 + 2400, RATE), 0.2);
        // It always strikes from full, as an envelope that resets.
        let shaped = Envelope::Drum(env);
        assert!(shaped.reset() && shaped.one_shot());
        assert_eq!(shaped.held(100, 0.7, RATE), env.held(100, RATE));
    }

    #[test]
    fn a_drums_ratchet_strikes_its_peak_again_before_the_body() {
        let clap = drum(10, 10, 200, 3);
        // Full at each strike, 10 ms apart, falling toward the body between them.
        for strike in 0..3 {
            close(clap.held(strike * 480, RATE), 1.0);
            assert!(clap.held(strike * 480 + 470, RATE) < 0.2);
        }
        // The body decays from the end of the third.
        close(clap.held(1440, RATE), 0.1);
        close(clap.held(1440 + 9600, RATE), 0.0);
        assert!(clap.finished_holding(1440 + 9600, RATE));
        assert!(!clap.finished_holding(1440 + 9599, RATE));
    }

    #[test]
    fn a_drum_with_no_times_is_safe_and_its_range_is_checked() {
        let none = drum(0, 50, 0, 4);
        close(none.held(0, RATE), 0.0);
        assert!(none.finished_holding(0, RATE));
        assert!(drum(MAX_ENVELOPE_MS, 100, MAX_ENVELOPE_MS, MAX_RATCHET).in_range());
        assert!(!drum(0, 101, 0, 1).in_range());
        assert!(!drum(0, 0, 0, 0).in_range());
        assert!(!drum(0, 0, 0, MAX_RATCHET + 1).in_range());
        let text = ron::to_string(&drum(8, 30, 120, 3)).unwrap();
        assert_eq!(ron::from_str::<Drum>(&text).unwrap(), drum(8, 30, 120, 3));
    }

    /// Plaits' own vactrol, stepped a block at a time at its rate with its own coefficients, for
    /// a ping: `short_decay` and `decay_tail` as `voice.cc` works them out, with no colour.
    fn plaits_ping(decay: f32, seconds: f32) -> Vec<(f32, f32)> {
        const BLOCK: f32 = 12.0;
        const PLAITS_RATE: f32 = 48_000.0;
        let short = 200.0 * BLOCK / PLAITS_RATE * 2_f32.powf(-96.0 * decay / 12.0);
        let tail = 20.0 * BLOCK / PLAITS_RATE * 2_f32.powf(-72.0 * decay / 12.0) - short;
        let mut state = 1.0_f32;
        let mut out = Vec::new();
        let blocks = (seconds * PLAITS_RATE / BLOCK) as usize;
        for block in 0..blocks {
            let v4 = state.powi(4);
            state += (short + (1.0 - v4) * tail) * -state;
            out.push(((block + 1) as f32 * BLOCK / PLAITS_RATE, state));
        }
        out
    }

    #[test]
    fn an_lpgs_fall_is_plaits_vactrol_solved_and_slows_as_it_nears_silence() {
        // With its knob at the middle, Plaits' ratio is ours, so its fall timed as ours matches
        // the vactrol stepped as Plaits steps it, to within a couple of dB down to -40 dB.
        let plaits = plaits_ping(0.5, 3.0);
        let silent = plaits.iter().find(|(_, v)| *v < VACTROL_FLOOR).unwrap().0;
        let lpg = Lpg {
            decay_ms: (silent * 1000.0).round() as u16,
            ..Lpg::default()
        };
        for (seconds, v) in plaits.iter().step_by(40) {
            if *v < 0.01 {
                break;
            }
            let db = |level: f32| 20.0 * level.log10();
            let ours = lpg.fall(*seconds);
            assert!((db(ours) - db(*v)).abs() < 2.0, "{seconds}: {ours} {v}");
        }
        // Its fall is fast at first and slows near silence: the time to fall the first 20 dB is
        // less than to fall the next 20.
        let lpg = Lpg::default();
        let reach = |level: f32| {
            (0..40_000)
                .map(|ms| ms as f32 / 100_000.0)
                .find(|s| lpg.fall(*s) < level)
                .unwrap()
        };
        assert!(
            reach(0.1) < reach(0.01) - reach(0.1),
            "{} {}",
            reach(0.1),
            reach(0.01)
        );
        // It lands at its decay.
        assert_eq!(lpg.fall(0.4), 0.0);
        assert!(lpg.fall(0.399) < 1e-4);
    }

    #[test]
    fn an_lpg_rises_then_falls_whatever_the_note_and_closes_the_filter_as_it_falls() {
        let lpg = Lpg {
            attack_ms: 10,
            decay_ms: 100,
            ..Lpg::default()
        };
        close(lpg.held(0, 0.0, RATE), 0.0);
        close(lpg.held(240, 0.0, RATE), 0.5);
        close(lpg.held(480, 0.0, RATE), 1.0);
        close(lpg.held(480 + 4800, 0.0, RATE), 0.0);
        assert!(lpg.finished_holding(480 + 4800, RATE));
        assert!(!lpg.finished_holding(480 + 4799, RATE));
        // With no reset, it rises from where it was.
        let carried = Lpg {
            reset: false,
            ..lpg
        };
        close(carried.held(0, 0.5, RATE), 0.5);
        close(carried.held(240, 0.5, RATE), 1.0);
        let shaped = Envelope::Lpg(lpg);
        assert!(shaped.one_shot() && shaped.reset() && !Envelope::Lpg(carried).reset());
        // The filter is open at full and shut at silence, and mostly shut by the time it's quiet.
        close(Lpg::closing(1.0), 0.0);
        close(Lpg::closing(0.0), 1.0);
        assert!(Lpg::closing(0.5) > 0.4, "{}", Lpg::closing(0.5));
        for i in 0..10 {
            assert!(Lpg::closing(i as f32 / 10.0) >= Lpg::closing((i + 1) as f32 / 10.0));
        }
        let text = ron::to_string(&Lpg::default()).unwrap();
        assert_eq!(ron::from_str::<Lpg>(&text).unwrap(), Lpg::default());
        assert!(
            !Lpg {
                decay_ms: MAX_ENVELOPE_MS + 1,
                ..lpg
            }
            .in_range()
        );
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
