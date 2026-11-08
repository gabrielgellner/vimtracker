//! A feed-forward compressor, after Giannoulis, Massberg and Reiss, "Digital Dynamic Range
//! Compressor Design — A Tutorial and Analysis" (JAES 2012): a gain computer with a soft knee,
//! worked out in dB, then the smooth branching peak detector on how much it turns down, so the
//! attack and release each shape their own side. Both sides are linked: the louder one decides,
//! so the stereo picture doesn't move. What it hears can be another track, for ducking.

use vt_model::insert::{Compressor as Settings, attack_ms, ratio, release_ms, threshold_db};

/// The knee's width in dB: the curve bends over this much either side of the threshold.
const KNEE_DB: f32 = 6.0;
/// Below this, a level counts as silence, about -120 dB, so the log stays finite.
const FLOOR: f32 = 1e-6;

/// What it's worked out from its settings, kept until they change.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Set {
    threshold: f32,
    /// One over the ratio, so a limiter is `0`.
    slope: f32,
    attack: f32,
    release: f32,
}

/// The detector's state and the settings it was worked out from.
#[derive(Debug, Clone, Copy, Default)]
pub struct Compressor {
    /// How far it's turning down now, in dB.
    reduction: f32,
    set: Option<(Settings, Set)>,
}

/// How much of the last value a one-pole smoother keeps, for a time in ms at a rate.
fn keeps(ms: f32, rate: u32) -> f32 {
    (-1.0 / (ms / 1000.0 * rate as f32).max(1.0)).exp()
}

/// The level out, in dB, for a level in, with the soft knee.
fn computed(level: f32, set: &Set) -> f32 {
    let over = level - set.threshold;
    if 2.0 * over < -KNEE_DB {
        level
    } else if 2.0 * over <= KNEE_DB {
        let into = over + KNEE_DB / 2.0;
        level + (set.slope - 1.0) * into * into / (2.0 * KNEE_DB)
    } else {
        set.threshold + over * set.slope
    }
}

impl Compressor {
    /// How far it's turning down now, in dB, for a meter.
    pub fn reduction(&self) -> f32 {
        self.reduction
    }

    /// Forget how far it was turning down, for a fresh start.
    pub fn clear(&mut self) {
        self.reduction = 0.0;
    }

    /// What it's worked out from these settings, from the last time if they haven't changed.
    fn set(&mut self, settings: Settings, rate: u32) -> Set {
        if let Some((was, set)) = self.set
            && was == settings
        {
            return set;
        }
        let set = Set {
            threshold: threshold_db(f32::from(settings.threshold)),
            slope: 1.0 / ratio(f32::from(settings.ratio)),
            attack: keeps(attack_ms(f32::from(settings.attack)), rate),
            release: keeps(release_ms(f32::from(settings.release)), rate),
        };
        self.set = Some((settings, set));
        set
    }

    /// One frame turned down by what `key` asks for, all wet. `key` is the frame itself unless it
    /// hears another track.
    #[inline]
    pub fn process(
        &mut self,
        input: [f32; 2],
        key: [f32; 2],
        settings: Settings,
        rate: u32,
    ) -> [f32; 2] {
        let set = self.set(settings, rate);
        let peak = key[0].abs().max(key[1].abs()).max(FLOOR);
        let level = 20.0 * peak.log10();
        let wanted = level - computed(level, &set);
        // Rising takes the attack, and falling the release.
        let keep = if wanted > self.reduction {
            set.attack
        } else {
            set.release
        };
        self.reduction = keep * self.reduction + (1.0 - keep) * wanted;
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
        let mut compressor = Compressor::default();
        let level = 10_f32.powf(level_db / 20.0);
        for _ in 0..RATE {
            compressor.process([level; 2], [level; 2], settings, RATE);
        }
        compressor.reduction()
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
        let set = Set {
            threshold: -20.0,
            slope: 0.25,
            attack: 0.0,
            release: 0.0,
        };
        let at = |level: f32| level - computed(level, &set);
        assert_eq!(at(-23.0), 0.0, "where the knee starts");
        assert!(
            (at(-17.0) - 3.0 * 0.75).abs() < 1e-4,
            "where it ends, on the line"
        );
        // Halfway through, it's already turning down a little, not jumping.
        assert!(at(-20.0) > 0.0 && at(-20.0) < at(-17.0));
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
        let mut compressor = Compressor::default();
        // A millisecond in, a 100 ms attack has barely started.
        for _ in 0..RATE / 1000 {
            compressor.process([1.0; 2], [1.0; 2], slow, RATE);
        }
        let early = compressor.reduction();
        assert!(early > 0.0 && early < 6.0, "{early}");
        for _ in 0..RATE {
            compressor.process([1.0; 2], [1.0; 2], slow, RATE);
        }
        let full = compressor.reduction();
        for _ in 0..RATE / 100 {
            compressor.process([0.0; 2], [0.0; 2], slow, RATE);
        }
        assert!(
            compressor.reduction() > full * 0.9,
            "a second's release hasn't let go yet"
        );
    }

    #[test]
    fn another_tracks_key_ducks_it_and_a_quiet_one_leaves_it() {
        let duck = settings(0x80, 0xFF);
        let mut compressor = Compressor::default();
        let mut out = [0.0; 2];
        for _ in 0..RATE / 10 {
            out = compressor.process([0.5; 2], [1.0; 2], duck, RATE);
        }
        assert!(out[0] < 0.1, "{out:?}");
        let mut quiet = Compressor::default();
        for _ in 0..RATE / 10 {
            out = quiet.process([0.5; 2], [0.0; 2], duck, RATE);
        }
        assert_eq!(out, [0.5; 2]);
    }
}
