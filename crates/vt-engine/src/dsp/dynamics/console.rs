//! The `Console` compressor, after the SSL G-series bus compressor as Jakob Erland's G-SSL clone
//! documents its circuit (https://gyraf.dk/gy_pd/ssl/ssl.htm), tuned against the compressor in
//! Arturia's Bus FORCE with `tools/compare` (#338, decision 118). Only Bus FORCE's outputs were
//! measured.
//!
//! **Feedback.** The G-SSL's detector doesn't hear the input itself: it hears a second, "dummy"
//! VCA, fed the input and driven by the same control voltage as the main ones, so it hears what the
//! compressor has already turned down. That makes it a feedback compressor, and so does this. The
//! ratio switch sets how hard the rectifier drives the control voltage, `ratio - 1` times what's
//! over, which comes to the ratio once it's settled. While it's still turning down, the loop
//! speeds the attack up by as much, so `10:1` grabs faster than `2:1`. The loop is solved within
//! each frame rather than a frame late, so a fast attack at `10:1` can't ring.
//!
//! **In dB.** The control voltage is the reduction in dB, as the G-SSL's VCAs take theirs, and it's
//! charged and let go as its timing capacitor is: the attack's resistor charges it while the
//! rectifier asks for more, and the release's resistor lets it go toward nothing otherwise. The
//! rectifier's peak holds for a moment, as the capacitor after a real one does, so a wave's
//! troughs don't let go of what its peaks asked for.
//!
//! **Auto** is the G-SSL's fifth release: a 0.47 uF capacitor let go by 750k, with a 6.8 uF one
//! hung on it through 91k. A short peak charges only the small one, which lets go in about 40 ms,
//! but held down, the big one charges too, and the pair let go over seconds.
//!
//! Its knee and where its line sits are fitted to Bus FORCE's static curve at each ratio, as the
//! G-SSL's ratio switch moves its threshold to make up for itself. It adds no colour: Bus FORCE's
//! compressor adds no even harmonics, nor any at all until it turns down.

use vt_model::insert::{
    CONSOLE_AUTO_RELEASE, CONSOLE_RATIOS, CONSOLE_RELEASES_MS, Compressor as Settings, threshold_db,
};

use crate::dsp::blocks::sides::same_bits_by_fields;
use crate::dsp::blocks::smoother::Smoothed;

/// Below this, a level counts as silence, about -120 dB, so the log stays finite.
const FLOOR: f32 = 1e-6;
/// For each ratio: how wide the knee is, in dB of what the detector hears, and how far over the
/// threshold the line it bends into sits, fitted to Bus FORCE's static curve (`comp-bus-static`).
const KNEES_DB: [f32; 3] = [9.5, 6.5, 5.5];
const OFFSETS_DB: [f32; 3] = [0.25, 1.75, 2.5];
/// The attack's time constant for each of the switch's, in ms. The loop makes a 4:1 attack this
/// over 4. From 1 ms up they're fitted so that 4:1 reaches 63% and 90% of its turn down within
/// 15% of when Bus FORCE does at the same time (`comp-bus-attack`), which, like the switch, says
/// how long it takes rather than what the capacitor is. Bus FORCE goes no faster than 1 ms, so 0.1
/// and 0.3 ms are what the switch says, at 4:1.
const ATTACK_MS: [f32; 6] = [0.4, 1.2, 13.7, 20.4, 44.0, 121.0];
/// How long the rectifier's held peak takes to fall to about a third, in ms.
const HOLD_MS: f32 = 10.0;
/// Auto's time constants, in seconds: the small capacitor let go by its own resistor, the small one
/// emptying into the big one, and the big one filling from the small one.
const AUTO_DISCHARGE: f32 = 750e3 * 0.47e-6;
const AUTO_INTO_SLOW: f32 = 91e3 * 0.47e-6;
const AUTO_SLOW_FILLS: f32 = 91e3 * 6.8e-6;

/// How much of the last value a one-pole keeps each frame, for a time constant in seconds.
fn keeps(seconds: f32, rate: u32) -> f32 {
    (-1.0 / (seconds * rate as f32).max(1.0)).exp()
}

/// The control voltage the rectifier asks for, in dB, for how far what it hears is over the
/// threshold: none below it, then bending over the knee into `slope` times what's over.
#[inline]
fn asked(over: f32, slope: f32, knee: f32) -> f32 {
    if over <= 0.0 {
        0.0
    } else if over < knee {
        slope * over * over / (2.0 * knee)
    } else {
        slope * (over - knee / 2.0)
    }
}

/// Where the control voltage gets to this frame, charging from `from` by the attack, which keeps
/// `attack` of it, while the detector hears the input `over` the threshold less the voltage itself.
/// It solves `r = attack * from + (1 - attack) * asked(over - r)`, which has the one answer, as
/// the right side falls as `r` rises.
#[inline]
fn charged(from: f32, attack: f32, over: f32, slope: f32, knee: f32) -> f32 {
    let (kept, taken) = (attack * from, 1.0 - attack);
    let short = |r: f32| r - kept - taken * asked(over - r, slope, knee);
    if short(over - knee) >= 0.0 {
        // Past the knee, on the line.
        (kept + taken * slope * (over - knee / 2.0)) / (1.0 + taken * slope)
    } else if short(over) >= 0.0 {
        // In the knee: a quadratic in how far over it hears, solved in the form that stays exact
        // when it's barely over.
        let bend = taken * slope / (2.0 * knee);
        let rest = over - kept;
        over - 2.0 * rest / (1.0 + (1.0 + 4.0 * bend * rest).sqrt())
    } else {
        kept
    }
}

/// How far a steady level is turned down once it's settled, in dB, by the ratio and knee at a
/// place on the switch.
pub fn settled(level: f32, settings: Settings) -> f32 {
    let at = usize::from(settings.console_ratio).min(CONSOLE_RATIOS.len() - 1);
    let over = level - threshold_db(f32::from(settings.threshold)) - OFFSETS_DB[at];
    charged(0.0, 0.0, over, CONSOLE_RATIOS[at] - 1.0, KNEES_DB[at])
}

/// What it keeps each frame for the switches' times, worked out when they change.
#[derive(Debug, Clone, Copy)]
struct Times {
    attack: f32,
    /// The release's, or nothing for `Auto`.
    release: Option<f32>,
    hold: f32,
    /// Auto's, as how much of the way each goes each frame.
    discharge: f32,
    into_slow: f32,
    slow_fills: f32,
}

/// The control voltage and its capacitors, and where its curve sits, gliding to where it's set.
#[derive(Debug, Clone, Copy, Default)]
pub struct Console {
    /// How far it's turning down now, in dB: the timing capacitor's voltage.
    reduction: f32,
    /// Auto's big capacitor's voltage, in dB.
    slow: f32,
    /// The rectifier's held peak, as a level.
    held: f32,
    threshold: Smoothed,
    /// `ratio - 1`, the knee and where its line sits, for the ratio switch's place.
    slope: Smoothed,
    knee: Smoothed,
    offset: Smoothed,
    /// The settings the times were worked out from, and the times.
    times: Option<(Settings, Times)>,
}

same_bits_by_fields!(Times {
    attack,
    release,
    hold,
    discharge,
    into_slow,
    slow_fills,
});
same_bits_by_fields!(Console {
    reduction,
    slow,
    held,
    threshold,
    slope,
    knee,
    offset,
    times,
});

impl Console {
    /// How far it's turning down now, in dB, for a meter.
    pub fn reduction(&self) -> f32 {
        self.reduction
    }

    /// Take what's set now, jumping to it the first time.
    fn follow(&mut self, settings: Settings, rate: u32) -> Times {
        if let Some((was, times)) = self.times
            && was == settings
        {
            return times;
        }
        let ratio = usize::from(settings.console_ratio).min(CONSOLE_RATIOS.len() - 1);
        self.threshold.target = threshold_db(f32::from(settings.threshold));
        self.slope.target = CONSOLE_RATIOS[ratio] - 1.0;
        self.knee.target = KNEES_DB[ratio];
        self.offset.target = OFFSETS_DB[ratio];
        if self.times.is_none() {
            self.threshold.settle();
            self.slope.settle();
            self.knee.settle();
            self.offset.settle();
        }
        let attack = ATTACK_MS[usize::from(settings.console_attack).min(ATTACK_MS.len() - 1)];
        let release = (settings.console_release < CONSOLE_AUTO_RELEASE).then(|| {
            keeps(
                CONSOLE_RELEASES_MS[usize::from(settings.console_release)] / 1000.0,
                rate,
            )
        });
        let times = Times {
            attack: keeps(attack / 1000.0, rate),
            release,
            hold: keeps(HOLD_MS / 1000.0, rate),
            discharge: 1.0 - keeps(AUTO_DISCHARGE, rate),
            into_slow: 1.0 - keeps(AUTO_INTO_SLOW, rate),
            slow_fills: 1.0 - keeps(AUTO_SLOW_FILLS, rate),
        };
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
        let times = self.follow(settings, rate);
        let threshold = self.threshold.next(smoothing);
        let slope = self.slope.next(smoothing);
        let knee = self.knee.next(smoothing);
        let offset = self.offset.next(smoothing);
        // Both sides share one detector, so the stereo picture doesn't move.
        let peak = key[0].abs().max(key[1].abs());
        self.held = peak.max(self.held * times.hold);
        let over = 20.0 * self.held.max(FLOOR).log10() - threshold - offset;
        let was = self.reduction;
        // Let go first, as the capacitor does whenever the rectifier isn't charging it.
        let let_go = match times.release {
            Some(release) => was * release,
            None => was - was * times.discharge - (was - self.slow) * times.into_slow,
        };
        // Then charge, if what it hears, turned down by that, still asks for more.
        self.reduction = if asked(over - let_go, slope, knee) > let_go {
            charged(was, times.attack, over, slope, knee).max(let_go)
        } else {
            let_go
        };
        self.slow = match times.release {
            Some(_) => 0.0,
            None => self.slow + (self.reduction - self.slow) * times.slow_fills,
        };
        let gain = 10_f32.powf(-self.reduction / 20.0);
        input.map(|sample| sample * gain)
    }
}

#[cfg(test)]
mod tests {
    use vt_model::insert::CompressorType;

    use super::*;

    const RATE: u32 = 48_000;

    fn console(ratio: u8, attack: u8, release: u8) -> Settings {
        Settings {
            kind: CompressorType::Console,
            threshold: 0xAA,
            console_ratio: ratio,
            console_attack: attack,
            console_release: release,
            ..Settings::DEFAULT
        }
    }

    #[test]
    fn the_loop_settles_where_the_curve_says() {
        for ratio in 0..3 {
            let settings = console(ratio, 0, 0);
            for level_db in [-30.0, -15.0, -5.0, 0.0] {
                let mut compressor = Console::default();
                let level = 10_f32.powf(level_db / 20.0);
                for _ in 0..RATE / 2 {
                    compressor.process([level; 2], [level; 2], settings, 1.0, RATE);
                }
                let expected = settled(level_db, settings);
                let got = compressor.reduction();
                assert!(
                    (got - expected).abs() < 0.01,
                    "{ratio} {level_db}: {got} {expected}"
                );
            }
        }
    }

    #[test]
    fn the_curve_comes_to_its_ratio() {
        for (at, ratio) in CONSOLE_RATIOS.into_iter().enumerate() {
            let settings = console(at as u8, 0, 0);
            assert_eq!(settled(-40.0, settings), 0.0, "nothing under the threshold");
            // 10 dB more in, well past the knee, comes out 10 / ratio more. At 10:1, the knee of
            // what the detector hears, turned down, is 30 dB of input wide.
            let low = Settings {
                threshold: 0x00,
                ..settings
            };
            let out = |level: f32| level - settled(level, low);
            let measured = 10.0 / (out(0.0) - out(-10.0));
            assert!((measured - ratio).abs() < 0.01, "{ratio}: {measured}");
        }
    }

    #[test]
    fn a_fast_attack_at_ten_to_one_doesnt_ring() {
        let settings = console(2, 0, 0);
        let mut compressor = Console::default();
        let mut last = 0.0_f32;
        for frame in 0..RATE / 10 {
            compressor.process([1.0; 2], [1.0; 2], settings, 1.0, RATE);
            let now = compressor.reduction();
            // It rises and stays risen, never overshooting and coming back.
            assert!(now >= last - 1e-4, "{frame}: {last} to {now}");
            last = now;
        }
        assert!((last - settled(0.0, settings)).abs() < 0.01);
    }

    /// How far it's still turned down a while after a burst of so many seconds at full level.
    fn left_after(burst: f32, wait: f32, release: u8) -> f32 {
        let settings = console(1, 2, release);
        let mut compressor = Console::default();
        for _ in 0..(burst * RATE as f32) as usize {
            compressor.process([1.0; 2], [1.0; 2], settings, 1.0, RATE);
        }
        let full = compressor.reduction();
        for _ in 0..(wait * RATE as f32) as usize {
            compressor.process([0.0; 2], [0.0; 2], settings, 1.0, RATE);
        }
        compressor.reduction() / full
    }

    #[test]
    fn auto_lets_a_short_peak_go_quickly_and_a_long_one_slowly() {
        let short = left_after(0.02, 0.2, CONSOLE_AUTO_RELEASE);
        let long = left_after(3.0, 0.2, CONSOLE_AUTO_RELEASE);
        assert!(short < 0.1, "a short peak's gone in 200 ms: {short}");
        assert!(long > 0.8, "a long one's barely let go: {long}");
        // A set release lets go the same whatever came before.
        let set = |burst| left_after(burst, 0.3, 1);
        assert!((set(0.02) - set(3.0)).abs() < 0.01);
        assert!((set(3.0) - (-1.0_f32).exp()).abs() < 0.02, "{}", set(3.0));
    }
}
