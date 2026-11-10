//! A slot's compressor, of whichever type it's set to (#338): an enum matched each frame rather
//! than boxed, so changing type allocates nothing.
//!
//! `Clean` is a feed-forward compressor after Giannoulis, Massberg and Reiss, "Digital Dynamic
//! Range Compressor Design — A Tutorial and Analysis" (JAES 2012): a gain computer with a soft
//! knee, worked out in dB, then their smooth decoupled peak detector on how much it turns down. Its
//! release comes first, holding what a wave's peaks asked for through its troughs, then its attack
//! smooths that, so the attack is a plain one-pole with no long tail (#334). Both sides are linked:
//! the louder one decides, so the stereo picture doesn't move. What it hears can be another track,
//! for ducking.
//!
//! `Bus` is in [`super::bus`].
//!
//! Every type's `Auto` and `Makeup` come after it, on what it turned down and before the mix, so
//! a new type has them with nothing of its own: `Auto` matches how loud it comes out to how loud
//! it went in, and `Makeup` turns it up from there.

use vt_model::insert::{
    Compressor as Settings, CompressorType, attack_ms, makeup_db, ratio, release_ms, threshold_db,
};

use super::bus::{self, Bus};
use crate::dsp::blocks::smoother::Smoothed;

/// The knee's width in dB: the curve bends over this much either side of the threshold.
const KNEE_DB: f32 = 6.0;
/// Below this, a level counts as silence, about -120 dB, so the log stays finite.
const FLOOR: f32 = 1e-6;
/// How long Auto listens to how loud it is going in and coming out, so it follows the sound's
/// level and not each hit.
const AUTO_MS: f32 = 1_000.0;
/// Below this power going in, about -80 dB, Auto holds what it's giving rather than chase the
/// silence.
const AUTO_QUIET: f32 = 1e-8;
/// The most Auto gives, in dB, however hard it's turned down.
const AUTO_MOST: f32 = 60.0;

/// A compressor of one of the types, with its own state, and the `Auto` and `Makeup` every type
/// shares.
#[derive(Debug, Clone, Copy, Default)]
pub struct Compressor {
    kind: Kind,
    auto: AutoGain,
    /// `Makeup` as a gain, gliding to where it's set, and the value it was worked out from.
    makeup: Smoothed,
    makeup_from: Option<u8>,
}

/// The type's own state.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Clean(Clean),
    Bus(Bus),
}

impl Default for Kind {
    fn default() -> Self {
        Self::Clean(Clean::default())
    }
}

impl Compressor {
    /// How far it's turning down now, in dB, for a meter.
    pub fn reduction(&self) -> f32 {
        match &self.kind {
            Kind::Clean(clean) => clean.reduction,
            Kind::Bus(bus) => bus.reduction(),
        }
    }

    /// One frame turned down by what `key` asks for, as its type does, then turned up by `Auto`
    /// and `Makeup`, all wet. `key` is the frame itself unless it hears another track. Changing
    /// type starts the new one afresh.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        key: [f32; 2],
        settings: Settings,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        let turned = match (&mut self.kind, settings.kind) {
            (Kind::Clean(clean), CompressorType::Clean) => {
                clean.process(input, key, settings, smoothing, rate)
            }
            (Kind::Bus(bus), CompressorType::Bus) => {
                bus.process(input, key, settings, smoothing, rate)
            }
            (_, CompressorType::Clean) => {
                self.kind = Kind::Clean(Clean::default());
                return self.process(input, key, settings, smoothing, rate);
            }
            (_, CompressorType::Bus) => {
                self.kind = Kind::Bus(Bus::default());
                return self.process(input, key, settings, smoothing, rate);
            }
        };
        if self.makeup_from != Some(settings.makeup) {
            self.makeup.target = 10_f32.powf(makeup_db(f32::from(settings.makeup)) / 20.0);
            if self.makeup_from.is_none() {
                self.makeup.settle();
            }
            self.makeup_from = Some(settings.makeup);
        }
        let gain = self
            .auto
            .gain(input, turned, settings.auto > 0, smoothing, rate)
            * self.makeup.next(smoothing);
        turned.map(|sample| sample * gain)
    }
}

/// What `Auto` gives back: how much quieter a type comes out than it went in, each measured over
/// about a second, so it follows the sound's level and not each hit.
#[derive(Debug, Clone, Copy, Default)]
struct AutoGain {
    /// How loud it's been going in lately, and coming out of the type, as power.
    heard: f32,
    left: f32,
    /// What it gives back, in dB: the difference between those two.
    makeup: f32,
    /// How much of `makeup` it gives back: all of it with Auto on, none with it off.
    on: Smoothed,
    /// The rate its time was worked out for, and how much of the last power it keeps each frame.
    keeps: Option<(u32, f32)>,
}

impl AutoGain {
    /// The gain it gives back this frame, for what went in and what came out of the type.
    #[inline]
    fn gain(
        &mut self,
        input: [f32; 2],
        turned: [f32; 2],
        on: bool,
        smoothing: f32,
        rate: u32,
    ) -> f32 {
        self.on.target = if on { 1.0 } else { 0.0 };
        let kept = match self.keeps {
            Some((was, kept)) if was == rate => kept,
            _ => {
                if self.keeps.is_none() {
                    self.on.settle();
                }
                let kept = keeps(AUTO_MS, rate);
                self.keeps = Some((rate, kept));
                kept
            }
        };
        // It matches how loud it comes out to how loud it went in, so switching it in or out
        // compares like with like, however hard it's turned down.
        let power = |frame: [f32; 2]| (frame[0] * frame[0] + frame[1] * frame[1]) / 2.0;
        self.heard = kept * self.heard + (1.0 - kept) * power(input);
        self.left = kept * self.left + (1.0 - kept) * power(turned);
        if self.heard > AUTO_QUIET {
            let difference = 10.0 * (self.heard / self.left.max(AUTO_QUIET * 1e-6)).log10();
            self.makeup = difference.clamp(0.0, AUTO_MOST);
        }
        10_f32.powf(self.makeup * self.on.next(smoothing) / 20.0)
    }
}

/// The clean type's detector, and where its curve sits, moving smoothly to where it's set.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clean {
    /// How far it's turning down now, in dB.
    reduction: f32,
    /// What the release lets go of, before the attack smooths it, in dB.
    released: f32,
    threshold: Smoothed,
    /// One over the ratio, so a limiter is `0`.
    slope: Smoothed,
    /// The settings the times were worked out from, and how much of the last value each keeps:
    /// the attack and the release.
    times: Option<(Settings, [f32; 2])>,
}

/// How much of the last value a one-pole smoother keeps, for a time in ms at a rate.
fn keeps(ms: f32, rate: u32) -> f32 {
    (-1.0 / (ms / 1000.0 * rate as f32).max(1.0)).exp()
}

/// The level out, in dB, for a level in, with the soft knee.
fn computed(level: f32, threshold: f32, slope: f32) -> f32 {
    let over = level - threshold;
    if 2.0 * over < -KNEE_DB {
        level
    } else if 2.0 * over <= KNEE_DB {
        let into = over + KNEE_DB / 2.0;
        level + (slope - 1.0) * into * into / (2.0 * KNEE_DB)
    } else {
        threshold + over * slope
    }
}

/// How loud a steady level comes out, in dB, once it's settled, before `Auto`, `Makeup` and `Mix`:
/// the static curve of the type it's set to, for drawing, worked out as the compressor does.
pub fn curve_db(level: f32, settings: Settings) -> f32 {
    match settings.kind {
        CompressorType::Clean => {
            let threshold = threshold_db(f32::from(settings.threshold));
            let slope = 1.0 / ratio(f32::from(settings.ratio));
            computed(level, threshold, slope)
        }
        CompressorType::Bus => level - bus::settled(level, settings),
    }
}

impl Clean {
    /// Take what's set now, jumping to it the first time.
    fn follow(&mut self, settings: Settings, rate: u32) -> [f32; 2] {
        if let Some((was, times)) = self.times
            && was == settings
        {
            return times;
        }
        self.threshold.target = threshold_db(f32::from(settings.threshold));
        self.slope.target = 1.0 / ratio(f32::from(settings.ratio));
        if self.times.is_none() {
            self.threshold.settle();
            self.slope.settle();
        }
        let times = [
            keeps(attack_ms(f32::from(settings.attack)), rate),
            keeps(release_ms(f32::from(settings.release)), rate),
        ];
        self.times = Some((settings, times));
        times
    }

    /// One frame turned down by what `key` asks for, all wet. `key` is the frame itself unless it
    /// hears another track.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        key: [f32; 2],
        settings: Settings,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        let [attack, release] = self.follow(settings, rate);
        let threshold = self.threshold.next(smoothing);
        let slope = self.slope.next(smoothing);
        let peak = key[0].abs().max(key[1].abs()).max(FLOOR);
        let level = 20.0 * peak.log10();
        let wanted = level - computed(level, threshold, slope);
        // The release lets go with no attack of its own, so it holds what a wave's peaks ask for
        // through its troughs, and the attack smooths what it holds.
        self.released = wanted.max(release * self.released + (1.0 - release) * wanted);
        self.reduction = attack * self.reduction + (1.0 - attack) * self.released;
        let gain = 10_f32.powf(-self.reduction / 20.0);
        input.map(|sample| sample * gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn settings(threshold: u8, ratio: u8) -> Settings {
        Settings {
            threshold,
            ratio,
            attack: 0x00,
            release: 0x00,
            ..Settings::DEFAULT
        }
    }

    /// How far a steady level is turned down once it's settled, in dB.
    fn settled(level_db: f32, settings: Settings) -> f32 {
        let mut compressor = Clean::default();
        let level = 10_f32.powf(level_db / 20.0);
        for _ in 0..RATE {
            compressor.process([level; 2], [level; 2], settings, 1.0, RATE);
        }
        compressor.reduction
    }

    #[test]
    fn below_the_knee_it_does_nothing_and_above_it_follows_the_ratio() {
        // -15 dB, at 4:1, has 4 dB of knee either side.
        let four = settings(0xC0, 0x6E);
        let threshold = threshold_db(f32::from(four.threshold));
        let ratio = ratio(f32::from(four.ratio));
        assert!(settled(threshold - 10.0, four) < 0.001);
        // 12 dB over a ratio of r comes out at 12 / r over, so it's turned down by the rest.
        let expected = 12.0 * (1.0 - 1.0 / ratio);
        let got = settled(threshold + 12.0, four);
        assert!((got - expected).abs() < 0.05, "{got} {expected}");
    }

    #[test]
    fn the_knee_bends_smoothly_through_the_threshold() {
        let at = |level: f32| level - computed(level, -20.0, 0.25);
        assert_eq!(at(-23.0), 0.0, "where the knee starts");
        assert!(
            (at(-17.0) - 3.0 * 0.75).abs() < 1e-4,
            "where it ends, on the line"
        );
        // Halfway through, it's already turning down a little, not jumping.
        assert!(at(-20.0) > 0.0 && at(-20.0) < at(-17.0));
    }

    /// How loud a steady level comes out once it's settled, in dB, with what's set.
    fn comes_out(level_db: f32, settings: Settings) -> f32 {
        let mut compressor = Compressor::default();
        let level = 10_f32.powf(level_db / 20.0);
        let mut out = [0.0; 2];
        for _ in 0..RATE * 8 {
            out = compressor.process([level; 2], [level; 2], settings, 1.0, RATE);
        }
        20.0 * out[0].log10()
    }

    #[test]
    fn auto_gain_gives_back_what_it_takes_even_with_the_threshold_right_down() {
        let auto = |threshold, ratio| Settings {
            auto: 1,
            ..settings(threshold, ratio)
        };
        for (threshold, ratio) in [(0xC0, 0x6E), (0x00, 0xFF), (0x00, 0x60)] {
            for level in [-6.0, -20.0] {
                let got = comes_out(level, auto(threshold, ratio));
                assert!(
                    (got - level).abs() < 0.5,
                    "{threshold:x} {ratio:x} {level}: {got}"
                );
            }
        }
        // And it's off unless asked for.
        assert!(comes_out(-6.0, settings(0x00, 0xFF)) < -50.0);
    }

    #[test]
    fn auto_gain_keeps_hits_and_gaps_as_loud_as_they_went_in() {
        // A hit every quarter second, a 220 Hz tone dying away, as a drum loop is.
        let hit = |frame: usize| {
            let since = (frame % (RATE as usize / 4)) as f32 / RATE as f32;
            (since * 220.0 * std::f32::consts::TAU).sin() * (-since * 20.0).exp() * 0.8
        };
        let power =
            |samples: &[f32]| samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
        for (threshold, ratio) in [(0x00, 0xFF), (0x20, 0xC0), (0x80, 0x60), (0xC0, 0x30)] {
            let auto = Settings {
                auto: 1,
                release: 0x60,
                ..settings(threshold, ratio)
            };
            let mut compressor = Compressor::default();
            let (mut went_in, mut came_out) = (Vec::new(), Vec::new());
            for frame in 0..RATE as usize * 6 {
                let sample = hit(frame);
                let out = compressor.process([sample; 2], [sample; 2], auto, 1.0, RATE)[0];
                if frame >= RATE as usize * 4 {
                    went_in.push(sample);
                    came_out.push(out);
                }
            }
            let db = 10.0 * (power(&came_out) / power(&went_in)).log10();
            assert!(db.abs() < 1.0, "{threshold:x} {ratio:x}: {db} dB");
        }
    }

    #[test]
    fn moving_the_threshold_or_ratio_glides_without_a_step() {
        let mut compressor = Clean::default();
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let mut last = 0.0;
        for _ in 0..RATE / 10 {
            last = compressor.process([0.5; 2], [0.5; 2], settings(0x40, 0x20), 1.0, RATE)[0];
        }
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as u8;
            let jumpy = Settings {
                threshold: 0x40 + step * 0x80,
                ratio: 0x20 + step * 0xD0,
                ..settings(0, 0)
            };
            let out = compressor.process([0.5; 2], [0.5; 2], jumpy, smoothing, RATE)[0];
            assert!((out - last).abs() < 0.02, "{frame}: {last} to {out}");
            last = out;
        }
    }

    #[test]
    fn a_limiter_holds_the_threshold() {
        let limit = settings(0xC0, 0xFF);
        let threshold = threshold_db(f32::from(limit.threshold));
        let got = settled(threshold + 20.0, limit);
        assert!((got - 20.0).abs() < 0.05, "{got}");
    }

    #[test]
    fn the_attack_turns_down_slowly_and_the_release_lets_go_slowly() {
        let slow = Settings {
            threshold: 0x00,
            ratio: 0xFF,
            attack: 0xFF,
            release: 0xFF,
            ..Settings::DEFAULT
        };
        let mut compressor = Clean::default();
        // A millisecond in, a 100 ms attack has barely started.
        for _ in 0..RATE / 1000 {
            compressor.process([1.0; 2], [1.0; 2], slow, 1.0, RATE);
        }
        let early = compressor.reduction;
        assert!(early > 0.0 && early < 6.0, "{early}");
        for _ in 0..RATE {
            compressor.process([1.0; 2], [1.0; 2], slow, 1.0, RATE);
        }
        let full = compressor.reduction;
        for _ in 0..RATE / 100 {
            compressor.process([0.0; 2], [0.0; 2], slow, 1.0, RATE);
        }
        assert!(
            compressor.reduction > full * 0.9,
            "a second's release hasn't let go yet"
        );
    }

    #[test]
    fn another_tracks_key_ducks_it_and_a_quiet_one_leaves_it() {
        let duck = settings(0x80, 0xFF);
        let mut compressor = Clean::default();
        let mut out = [0.0; 2];
        for _ in 0..RATE / 10 {
            out = compressor.process([0.5; 2], [1.0; 2], duck, 1.0, RATE);
        }
        assert!(out[0] < 0.1, "{out:?}");
        let mut quiet = Clean::default();
        for _ in 0..RATE / 10 {
            out = quiet.process([0.5; 2], [0.0; 2], duck, 1.0, RATE);
        }
        assert_eq!(out, [0.5; 2]);
    }
}
