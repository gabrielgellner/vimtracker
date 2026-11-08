//! A feed-forward compressor, after Giannoulis, Massberg and Reiss, "Digital Dynamic Range
//! Compressor Design — A Tutorial and Analysis" (JAES 2012): a gain computer with a soft knee,
//! worked out in dB, then the smooth branching peak detector on how much it turns down, so the
//! attack and release each shape their own side. Both sides are linked: the louder one decides,
//! so the stereo picture doesn't move. What it hears can be another track, for ducking.

use vt_model::insert::{Compressor as Settings, attack_ms, ratio, release_ms, threshold_db};

use crate::dsp::smoother::Smoothed;

/// The knee's width in dB: the curve bends over this much either side of the threshold.
const KNEE_DB: f32 = 6.0;
/// Below this, a level counts as silence, about -120 dB, so the log stays finite.
const FLOOR: f32 = 1e-6;
/// How long Auto takes to follow what it's turning down, so it gives back the average and not
/// each hit.
const AUTO_MS: f32 = 1_000.0;

/// The detector's state, and where its curve sits, moving smoothly to where it's set.
#[derive(Debug, Clone, Copy, Default)]
pub struct Compressor {
    /// How far it's turning down now, in dB.
    reduction: f32,
    /// How far it's turned down lately, in dB, for Auto to give back.
    average: f32,
    threshold: Smoothed,
    /// One over the ratio, so a limiter is `0`.
    slope: Smoothed,
    /// How much of `average` it gives back: all of it with Auto on, none with it off.
    auto: Smoothed,
    /// The settings the times were worked out from, and how much of the last value each keeps:
    /// the attack, the release and Auto's.
    times: Option<(Settings, [f32; 3])>,
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

impl Compressor {
    /// How far it's turning down now, in dB, for a meter.
    pub fn reduction(&self) -> f32 {
        self.reduction
    }

    /// Forget how far it was turning down, for a fresh start.
    pub fn clear(&mut self) {
        self.reduction = 0.0;
        self.average = 0.0;
    }

    /// Take what's set now, jumping to it the first time.
    fn follow(&mut self, settings: Settings, rate: u32) -> [f32; 3] {
        if let Some((was, times)) = self.times
            && was == settings
        {
            return times;
        }
        self.threshold.target = threshold_db(f32::from(settings.threshold));
        self.slope.target = 1.0 / ratio(f32::from(settings.ratio));
        self.auto.target = if settings.auto > 0 { 1.0 } else { 0.0 };
        if self.times.is_none() {
            self.threshold.settle();
            self.slope.settle();
            self.auto.settle();
        }
        let times = [
            keeps(attack_ms(f32::from(settings.attack)), rate),
            keeps(release_ms(f32::from(settings.release)), rate),
            keeps(AUTO_MS, rate),
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
        let [attack, release, auto] = self.follow(settings, rate);
        let threshold = self.threshold.next(smoothing);
        let slope = self.slope.next(smoothing);
        let peak = key[0].abs().max(key[1].abs()).max(FLOOR);
        let level = 20.0 * peak.log10();
        let wanted = level - computed(level, threshold, slope);
        // Rising takes the attack, and falling the release.
        let keep = if wanted > self.reduction {
            attack
        } else {
            release
        };
        self.reduction = keep * self.reduction + (1.0 - keep) * wanted;
        // Auto gives back what it's taken on average, so it's about as loud as it went in, and
        // switching it in or out compares like with like.
        self.average = auto * self.average + (1.0 - auto) * self.reduction;
        let given = self.average * self.auto.next(smoothing);
        let gain = 10_f32.powf((given - self.reduction) / 20.0);
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
            compressor.process([level; 2], [level; 2], settings, 1.0, RATE);
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
    fn moving_the_threshold_or_ratio_glides_without_a_step() {
        let mut compressor = Compressor::default();
        let smoothing = crate::dsp::smoother::coefficient(RATE);
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
        let mut compressor = Compressor::default();
        // A millisecond in, a 100 ms attack has barely started.
        for _ in 0..RATE / 1000 {
            compressor.process([1.0; 2], [1.0; 2], slow, 1.0, RATE);
        }
        let early = compressor.reduction();
        assert!(early > 0.0 && early < 6.0, "{early}");
        for _ in 0..RATE {
            compressor.process([1.0; 2], [1.0; 2], slow, 1.0, RATE);
        }
        let full = compressor.reduction();
        for _ in 0..RATE / 100 {
            compressor.process([0.0; 2], [0.0; 2], slow, 1.0, RATE);
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
            out = compressor.process([0.5; 2], [1.0; 2], duck, 1.0, RATE);
        }
        assert!(out[0] < 0.1, "{out:?}");
        let mut quiet = Compressor::default();
        for _ in 0..RATE / 10 {
            out = quiet.process([0.5; 2], [0.0; 2], duck, 1.0, RATE);
        }
        assert_eq!(out, [0.5; 2]);
    }
}
