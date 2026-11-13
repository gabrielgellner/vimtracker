//! The `Glue` compressor, after Ableton Live's Glue Compressor, itself Cytomic's model of the SSL
//! console's bus compressor, fitted to it with `tools/compare` through the Live bridge (#338,
//! #364, decision 134). Only what Glue puts out was measured: its static curves, its attacks and
//! releases at every place on its switches, its `Range`, and its detector against a tone of a
//! tenth the pitch and two tones together.
//!
//! **Feedback.** Its 10 ms attack reaches 63% in 16 ms at 2:1, 12 ms at 4:1 and 6 ms at 10:1, as
//! a detector hearing what it's already turned down does, the loop speeding it up, where a
//! feed-forward one takes as long at any ratio. So, as the SSL's circuit has it and as `Console`
//! does, the detector hears the key turned down by the control voltage, solved within each frame.
//!
//! **The timing capacitor** is the control voltage, the reduction in dB. Its attack charges it
//! toward what the rectifier asks while that's more, and its release lets it go toward nothing all
//! the while, as a resistor across a capacitor does. So it settles where the two balance, lower
//! the slower the attack is against the release: Glue holding a tone 10 dB over at 4:1 turns down
//! 6.8 dB with its fastest attack and 3.7 dB with its slowest, and this does the same. The
//! rectifier is the key's level each frame, with nothing to hold its peaks, so the capacitor
//! smooths a wave's ripple itself, which Glue's harmonics show: its 3rd at 100 Hz is 20 dB louder
//! than at 1 kHz.
//!
//! **The knee** is no setting of Glue's, which "gets sharper as the ratio rises", as Live's manual
//! has it: at 2:1 it's already turning down 0.2 dB 20 dB under the threshold, and at 10:1 it
//! doesn't start until 3 dB over.
//! Each ratio's is a curve of what the rectifier asks, in dB over the threshold: none, then a
//! power of how far into the knee it is, then a line. Where each starts, how wide it is, its power
//! and its line's slope are fitted to Glue's static curves at a -20 dB threshold with its 10 ms
//! attack and `.1` release, as the attacks are to its bursts, together.
//!
//! **Releases** aren't the times their names say, as the SSL's aren't: each lets go as one
//! exponential in dB, and their time constants are what Glue's measured. **Auto** is the SSL's two
//! capacitors, as `Console` has it, with their time constants fitted to Glue's: a short burst lets
//! go in about 50 ms, two seconds' in about 2.4 s.
//!
//! **Range** is the most the rectifier asks for, so the capacitor's ripple brings it in softly.
//! Glue's `6 dB` turns a tone 30 dB over down by 3.4 dB, its `20 dB` by 13.3 dB; the ceiling that
//! matches each is a line in the range, fitted at 3, 6, 12 and 20 dB.
//!
//! **No colour.** Glue's odd harmonics are its gain following the wave, which this matches to a
//! decibel; its 2nd is under -84 dB at 1 kHz and -64 dB at 100 Hz, and this adds none.
//!
//! **Peak Clip** is Live's `Peak Clip In`, a fixed soft clip after Glue's output and its dry/wet,
//! so it's the slot's [`PeakClip`], after the mix. Glue puts out exactly a curve of what comes in,
//! sample by sample: it leaves anything under -2 dBFS as it is, bit for bit, then bends over with
//! no kink, nor any in its slope or how that changes, to flatten at -0.5 dBFS, where it stays, a
//! curve of degree four (`live-glue-clip`). It's odd, so it adds only odd harmonics.
//!
//! **Left out:** its side-chain EQ, as `Source` is the side-chain here.

use vt_model::insert::{
    Compressor as Settings, GLUE_AUTO_RELEASE, GLUE_RATIOS, range_db, threshold_db,
};

use crate::dsp::blocks::sides::same_bits_by_fields;
use crate::dsp::blocks::smoother::Smoothed;

/// Below this, a level counts as silence, about -120 dB, so the log stays finite.
const FLOOR: f32 = 1e-6;
/// For each ratio, where its knee starts, in dB over the threshold, how wide it is, its power and
/// the slope of the line it bends into, fitted to Glue's static curves (`live-glue-static`). Each
/// slope is the ratio's less one, made steeper by about half, as the release always letting go
/// leaves the capacitor short of what's asked.
const KNEE_STARTS_DB: [f32; 3] = [-46.84, -1.365, 3.444];
const KNEE_WIDTHS_DB: [f32; 3] = [50.42, 9.584, 6.853];
const KNEE_POWERS: [f32; 3] = [6.0, 1.617, 1.234];
const SLOPES: [f32; 3] = [1.666, 4.353, 14.30];
/// The attack's time constant for each place on its switch, in ms, fitted to Glue's bursts at 4:1
/// (`live-glue-attack`).
const ATTACK_MS: [f32; 7] = [0.757, 1.338, 2.212, 4.764, 13.44, 38.56, 123.1];
/// The release's time constant for each set place on its switch, in ms, as Glue lets go after a
/// burst (`live-glue-release`).
const RELEASE_MS: [f32; 6] = [80.5, 117.5, 160.3, 225.3, 303.0, 413.5];
/// Auto's time constants, in seconds: the small capacitor let go by its own resistor, the small one
/// emptying into the big one, and the big one filling from the small one, fitted to Glue's.
const AUTO_DISCHARGE: f32 = 0.4015;
const AUTO_INTO_SLOW: f32 = 0.05162;
const AUTO_SLOW_FILLS: f32 = 0.5418;
/// The most the rectifier asks for at a `Range`: no less than this share of it, nor than the line
/// fitted to Glue's (`live-glue-range`).
const RANGE_LEAST_SHARE: f32 = 0.63;
const RANGE_SLOPE: f32 = 1.154;
const RANGE_OFFSET_DB: f32 = 1.886;
/// How many of Newton's steps the loop takes at most to solve a frame, each from below, and how
/// close in dB it stops.
const MOST_STEPS: usize = 8;
const CLOSE_DB: f32 = 1e-4;
/// Where `Peak Clip` starts to bend, -2 dBFS, and where it flattens, -0.5 dBFS, as gains, as Glue's
/// measured to a part in a million (`live-glue-clip`).
const CLIP_KNEE: f32 = 0.794_328_2;
const CLIP_CEILING: f32 = 0.944_060_9;
/// How long switching `Peak Clip` in or out takes, in ms, as long as switching a slot out does, so
/// it doesn't click on a loud sound.
const CLIP_SWITCH_MS: f32 = 5.0;

/// How much of the way a one-pole goes each frame, for a time constant in seconds.
fn takes(seconds: f32, rate: u32) -> f32 {
    1.0 - (-1.0 / (seconds * rate as f32).max(1.0)).exp()
}

/// The most the rectifier asks for, in dB, at a value of `Range`.
fn ceiling(value: u8) -> f32 {
    let range = range_db(f32::from(value));
    (range * RANGE_LEAST_SHARE).max(range * RANGE_SLOPE - RANGE_OFFSET_DB)
}

/// A ratio's knee and line, and the ceiling `Range` puts on them.
#[derive(Debug, Clone, Copy)]
struct Curve {
    start: f32,
    width: f32,
    power: f32,
    slope: f32,
    ceiling: f32,
}

impl Curve {
    fn of(settings: Settings) -> Self {
        let at = usize::from(settings.glue_ratio).min(GLUE_RATIOS.len() - 1);
        Self {
            start: KNEE_STARTS_DB[at],
            width: KNEE_WIDTHS_DB[at],
            power: KNEE_POWERS[at],
            slope: SLOPES[at],
            ceiling: ceiling(settings.range),
        }
    }

    /// What the rectifier asks for, in dB, for how far what it hears is over the threshold,
    /// before the ceiling, and how fast that grows.
    #[inline]
    fn knee(&self, over: f32) -> (f32, f32) {
        let into = over - self.start;
        if into <= 0.0 {
            (0.0, 0.0)
        } else if into < self.width {
            let share = (into / self.width).powf(self.power - 1.0);
            (self.slope * into * share / self.power, self.slope * share)
        } else {
            let line = self.slope * (into - self.width + self.width / self.power);
            (line, self.slope)
        }
    }

    /// What the rectifier asks for, in dB, with the ceiling.
    #[inline]
    fn asked(&self, over: f32) -> f32 {
        self.knee(over).0.min(self.ceiling)
    }

    /// Where the control voltage gets to this frame, charging from `from` by `attack`, the share
    /// of the way it goes, while the detector hears the key `over` the threshold less the voltage
    /// itself. It solves `r = from + attack * (asked(over - r) - from)`, whose right side falls as
    /// `r` rises, so it has the one answer.
    #[inline]
    fn charged(&self, from: f32, attack: f32, over: f32) -> f32 {
        // At the ceiling, it charges toward the ceiling alone.
        let flat = from + attack * (self.ceiling - from);
        if self.knee(over - flat).0 >= self.ceiling {
            return flat;
        }
        // Under it, the knee bends up, so Newton's steps from below never pass the answer.
        let mut reduction = from;
        for _ in 0..MOST_STEPS {
            let (asked, growth) = self.knee(over - reduction);
            let short = from + attack * (asked - from) - reduction;
            let step = short / (1.0 + attack * growth);
            reduction += step;
            if step < CLOSE_DB {
                break;
            }
        }
        reduction.min(flat)
    }
}

/// How far a steady sine is turned down once it's settled, in dB, for its level at its peak: the
/// static curve, for drawing. Over a cycle, what the attack charges while the rectifier asks for
/// more than the capacitor holds balances what the release lets go all the while.
pub fn settled(level: f32, settings: Settings) -> f32 {
    /// Points over a quarter of the wave, enough for the balance to come within a hundredth of a
    /// dB of a long sine's.
    const PHASES: usize = 32;
    let curve = Curve::of(settings);
    let over = level - threshold_db(f32::from(settings.threshold));
    let attack = ATTACK_MS[usize::from(settings.glue_attack).min(ATTACK_MS.len() - 1)] / 1000.0;
    let release = if settings.glue_release >= GLUE_AUTO_RELEASE {
        AUTO_DISCHARGE
    } else {
        RELEASE_MS[usize::from(settings.glue_release)] / 1000.0
    };
    let wave: [f32; PHASES] = std::array::from_fn(|at| {
        let phase = (at as f32 + 0.5) / PHASES as f32 * std::f32::consts::FRAC_PI_2;
        20.0 * phase.sin().log10()
    });
    // How much faster it charges than it lets go, at a reduction.
    let gaining = |reduction: f32| {
        let charge = wave
            .iter()
            .map(|place| (curve.asked(over + place - reduction) - reduction).max(0.0))
            .sum::<f32>()
            / PHASES as f32;
        charge / attack - reduction / release
    };
    let (mut low, mut high) = (0.0, curve.asked(over).max(0.0));
    for _ in 0..40 {
        let middle = 0.5 * (low + high);
        if gaining(middle) > 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    0.5 * (low + high)
}

/// What it keeps each frame for the switches' times, worked out when they change.
#[derive(Debug, Clone, Copy)]
struct Times {
    attack: f32,
    /// The release's, or nothing for `Auto`.
    release: Option<f32>,
    /// Auto's, as how much of the way each goes each frame.
    discharge: f32,
    into_slow: f32,
    slow_fills: f32,
}

/// The control voltage and its capacitors, and where its curve sits, gliding to where it's set.
#[derive(Debug, Clone, Copy, Default)]
pub struct Glue {
    /// How far it's turning down now, in dB: the timing capacitor's voltage.
    reduction: f32,
    /// Auto's big capacitor's voltage, in dB.
    slow: f32,
    threshold: Smoothed,
    start: Smoothed,
    width: Smoothed,
    power: Smoothed,
    slope: Smoothed,
    ceiling: Smoothed,
    /// The settings the times were worked out from, and the times.
    times: Option<(Settings, Times)>,
}

same_bits_by_fields!(Times {
    attack,
    release,
    discharge,
    into_slow,
    slow_fills,
});
same_bits_by_fields!(Glue {
    reduction,
    slow,
    threshold,
    start,
    width,
    power,
    slope,
    ceiling,
    times,
});
same_bits_by_fields!(PeakClip { on, target });

impl Glue {
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
        let curve = Curve::of(settings);
        self.threshold.target = threshold_db(f32::from(settings.threshold));
        self.start.target = curve.start;
        self.width.target = curve.width;
        self.power.target = curve.power;
        self.slope.target = curve.slope;
        self.ceiling.target = curve.ceiling;
        if self.times.is_none() {
            for smoothed in [
                &mut self.threshold,
                &mut self.start,
                &mut self.width,
                &mut self.power,
                &mut self.slope,
                &mut self.ceiling,
            ] {
                smoothed.settle();
            }
        }
        let attack = ATTACK_MS[usize::from(settings.glue_attack).min(ATTACK_MS.len() - 1)];
        let release = (settings.glue_release < GLUE_AUTO_RELEASE).then(|| {
            takes(
                RELEASE_MS[usize::from(settings.glue_release)] / 1000.0,
                rate,
            )
        });
        let times = Times {
            attack: takes(attack / 1000.0, rate),
            release,
            discharge: takes(AUTO_DISCHARGE, rate),
            into_slow: takes(AUTO_INTO_SLOW, rate),
            slow_fills: takes(AUTO_SLOW_FILLS, rate),
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
        let curve = Curve {
            start: self.start.next(smoothing),
            width: self.width.next(smoothing),
            power: self.power.next(smoothing),
            slope: self.slope.next(smoothing),
            ceiling: self.ceiling.next(smoothing),
        };
        // Both sides share one detector, so the stereo picture doesn't move.
        let peak = key[0].abs().max(key[1].abs());
        let over = 20.0 * peak.max(FLOOR).log10() - threshold;
        let was = self.reduction;
        // The release lets go every frame, as the resistor across the capacitor does.
        let let_go = match times.release {
            Some(release) => was - was * release,
            None => was - was * times.discharge - (was - self.slow) * times.into_slow,
        };
        // Then the attack charges, if what it hears, turned down by that, still asks for more.
        self.reduction = if curve.asked(over - let_go) > let_go {
            curve.charged(let_go, times.attack, over)
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

/// A sample through `Peak Clip`: itself under the knee, and from there a curve that leaves the
/// knee as the line does, with no change in how it bends, and comes to the ceiling flat, with no
/// change in how it bends there either, at twice the ceiling's height over the knee. Over that it's
/// the ceiling. Glue's own puts out -1.08 dBFS for -1 dBFS in.
#[inline]
pub fn clipped(sample: f32) -> f32 {
    let size = sample.abs();
    if size <= CLIP_KNEE {
        return sample;
    }
    // How far past the knee, from 0 to 2 where it reaches the ceiling, in the knee's height.
    let past = ((size - CLIP_KNEE) / (CLIP_CEILING - CLIP_KNEE)).min(2.0);
    let curve = past * (1.0 - past * past * (0.25 - past / 16.0));
    (CLIP_KNEE + (CLIP_CEILING - CLIP_KNEE) * curve).copysign(sample)
}

/// Glue's `Peak Clip`, after its output and its mix, switched in and out over a few ms. Switched
/// out, it leaves what comes through bit for bit.
#[derive(Debug, Clone, Copy, Default)]
pub struct PeakClip {
    /// How far it's switched in, from `0` to `1`, and where it's going.
    on: f32,
    target: f32,
}

impl PeakClip {
    /// Switch it in or out.
    pub fn switch(&mut self, on: bool) {
        self.target = if on { 1.0 } else { 0.0 };
    }

    /// Jump to where it's switched.
    pub fn settle(&mut self) {
        self.on = self.target;
    }

    /// One frame through it.
    #[inline]
    pub fn process(&mut self, frame: [f32; 2], rate: u32) -> [f32; 2] {
        if self.on == self.target {
            return if self.on == 0.0 {
                frame
            } else {
                frame.map(clipped)
            };
        }
        // A straight line, as a slot's bypass fades, so it's done within a few ms.
        let by = 1000.0 / (CLIP_SWITCH_MS * rate as f32);
        self.on = if self.target > self.on {
            (self.on + by).min(self.target)
        } else {
            (self.on - by).max(self.target)
        };
        frame.map(|sample| sample + (clipped(sample) - sample) * self.on)
    }
}

#[cfg(test)]
mod tests {
    use vt_model::insert::CompressorType;

    use super::*;

    fn glue(ratio: u8, attack: u8, release: u8) -> Settings {
        Settings {
            kind: CompressorType::Glue,
            threshold: 0xAA,
            glue_ratio: ratio,
            glue_attack: attack,
            glue_release: release,
            ..Settings::DEFAULT
        }
    }

    #[test]
    fn each_frame_solves_the_loop() {
        for ratio in 0..3 {
            let curve = Curve::of(glue(ratio, 0, 0));
            // It charges only while what it hears asks for more than it holds.
            for (from, attack, over) in [
                (0.0, 1.0, 20.0),
                (2.0, 0.01, 30.0),
                (1.0, 0.3, 12.0),
                (0.0, 0.5, 300.0),
            ] {
                assert!(curve.asked(over - from) > from);
                let got = curve.charged(from, attack, over);
                let wanted = from + attack * (curve.asked(over - got) - from);
                assert!((got - wanted).abs() < 1e-3, "{ratio}: {got} {wanted}");
            }
        }
    }

    #[test]
    fn the_range_caps_what_it_asks_for() {
        let mut settings = glue(2, 0, 0);
        settings.range = 0x00;
        assert_eq!(Curve::of(settings).asked(60.0), 0.0, "none at all");
        // The most it asks for follows the range: under it at the bottom, where Glue settles
        // well under its range, and a little over it at the top.
        for range in [0x0B, 0x40, 0xFF] {
            settings.range = range;
            let curve = Curve::of(settings);
            assert_eq!(curve.asked(300.0), curve.ceiling);
            let range = range_db(f32::from(range));
            assert!(
                (curve.ceiling - range).abs() < 2.0 + range * 0.13,
                "{range}"
            );
        }
    }

    #[test]
    fn the_clip_is_smooth_where_it_starts_and_where_it_flattens() {
        let db = |gain: f32| 20.0 * gain.log10();
        assert_eq!(clipped(0.7), 0.7);
        assert_eq!(clipped(-CLIP_KNEE), -CLIP_KNEE);
        // Glue's: -1 dBFS comes out at -1.08, and it's flat at -0.5 dBFS from there on.
        assert!((db(clipped(0.891_250_9)) + 1.08).abs() < 0.005);
        let top = CLIP_KNEE + 2.0 * (CLIP_CEILING - CLIP_KNEE);
        for size in [top, 1.5, 1e30, f32::INFINITY] {
            assert_eq!(clipped(size), CLIP_CEILING, "{size}");
            assert_eq!(clipped(-size), -CLIP_CEILING, "{size}");
        }
        // Its slope is the line's going in and flat coming out.
        let slope = |at: f32| (clipped(at + 1e-3) - clipped(at - 1e-3)) / 2e-3;
        assert!(
            (slope(CLIP_KNEE) - 1.0).abs() < 2e-3,
            "{}",
            slope(CLIP_KNEE)
        );
        assert!(slope(top).abs() < 2e-3, "{}", slope(top));
        // It only ever turns down, and never turns round.
        let mut last = 0.0;
        for step in 0..2000 {
            let size = step as f32 / 1000.0;
            let out = clipped(size);
            assert!(out <= size && out >= last, "{size}");
            last = out;
        }
    }

    #[test]
    fn the_clip_switches_in_and_out_without_a_jump() {
        let mut clip = PeakClip::default();
        assert_eq!(clip.process([1.2, -0.3], 48_000), [1.2, -0.3], "out");
        clip.switch(true);
        let mut last = 1.2;
        for _ in 0..48_000 / 100 {
            let out = clip.process([1.2; 2], 48_000)[0];
            assert!((out - last).abs() < 0.01, "{last} {out}");
            last = out;
        }
        assert_eq!(last, CLIP_CEILING, "switched in");
        clip.switch(false);
        clip.settle();
        assert_eq!(clip.process([1.2; 2], 48_000), [1.2; 2], "jumps when asked");
    }
}
