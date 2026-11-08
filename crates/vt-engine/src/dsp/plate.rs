//! A stereo plate: Jon Dattorro's, from "Effect Design, Part 1: Reverberator and Other Filters"
//! (J. Audio Eng. Soc., 1997). What goes in is band-limited, held back and spread by four
//! all-passes, then goes round a tank of two halves that feed each other, each a wobbling
//! all-pass, a delay, a damping low-pass, a second all-pass and a second delay. Each side of what
//! comes out is seven taps from all over the tank, as the paper gives them.
//!
//! The paper's lengths are for 29761 Hz, and are scaled here to the rate it runs at. `Size` scales
//! the tank's lengths and taps again, read between samples, and moves no faster than its whole
//! range in a second, so a change bends the tail's pitch a little rather than clicking. Its lines
//! share one buffer from the effect pool, long enough for the biggest plate at the pool's rate.
//!
//! Ported from el-visio's `verb.c` at <https://github.com/el-visio/dattorro-verb>, MIT, and checked
//! against the paper. That port steps its wobble one sample at a time and adds the last tap of
//! each side, where the paper takes it away; here the wobble is a sine read between samples, and
//! the taps' signs are the paper's.
//!
//! Copyright (c) 2022 Pauli Pölkki.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction,
//! including without limitation the rights to use, copy, modify, merge, publish, distribute,
//! sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is
//! furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT
//! NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT
//! OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

use vt_model::insert::{Plate as Settings, plate_size};

use crate::dsp::smoother::{Smoothed, coefficient};

/// The rate the paper's lengths are written for.
const TUNING_RATE: f32 = 29_761.0;
/// The input all-passes' lengths, and how much each spreads what goes through it.
const DIFFUSER_LENGTHS: [f32; 4] = [142.0, 107.0, 379.0, 277.0];
const DIFFUSIONS: [f32; 4] = [0.75, 0.75, 0.625, 0.625];
/// Each half of the tank: its wobbling all-pass, its delay before the damping, its second all-pass
/// and its delay after.
const TANK_LENGTHS: [[f32; 4]; 2] = [
    [672.0, 4453.0, 1800.0, 3720.0],
    [908.0, 4217.0, 2656.0, 3163.0],
];
/// How much the tank's first all-passes spread, taken away rather than added as the paper has it.
const DECAY_DIFFUSION: f32 = 0.7;
/// The most and least the second all-passes spread, which goes with `Decay`, as the paper has it.
const SECOND_DIFFUSION: (f32, f32) = (0.25, 0.5);
/// What each trip through a half of the tank keeps, twice, at the top of `Decay`. Just short of
/// one, so even the longest tail dies away, in about a minute.
const MOST_DECAY: f32 = 0.98;
/// The damping filter's coefficient at the top of `Damp`, at the tuning rate, as the reverb's. At
/// `00` it's open.
const MOST_DAMPING: f32 = 0.1;
/// The longest pre-delay, in seconds, as the reverb's.
const LONGEST_PRE: f32 = 0.25;
/// The lowest `High cut`, in hertz, and how many times higher it goes, as the filters' cutoff.
const LOWEST_CUT_HZ: f32 = 20.0;
const CUT_SPAN: f32 = 1000.0;
/// How far the first all-passes wobble at the top of `Mod`, at the tuning rate. The paper's 16 is
/// at `80`.
const MOST_EXCURSION: f32 = 32.0;
/// How fast they wobble. The two halves are a quarter of a turn apart.
const WOBBLE_HZ: f64 = 1.0;
/// The biggest plate, as so many times the paper's.
const LARGEST: f32 = 2.0;
/// How far `Size` moves in a second at most, as so many times the paper's: its whole range. So
/// the lines' lengths change by at most about a quarter of a frame each frame, and the pitch of
/// what's in them bends by no more than a quarter as it moves.
const SIZE_PER_SECOND: f32 = 1.5;
/// How far the pre-delay moves in a frame at most, in frames, for the same reason: across its
/// whole range in a second.
const PRE_PER_FRAME: f32 = LONGEST_PRE;
/// What each tap is scaled by, as the paper has it.
const TAP_GAIN: f32 = 0.6;
/// How loud what comes out is. Tuned so it's near the reverb's at both their defaults.
const WET: f32 = 0.7;

/// Where in the tank a tap reads: which half, which of its four lines, how far back at the tuning
/// rate, and whether it's added or taken away.
type Tap = (usize, usize, f32, f32);

/// The paper's taps: the left side's, then the right's.
const TAPS: [[Tap; 7]; 2] = [
    [
        (1, 1, 266.0, 1.0),
        (1, 1, 2974.0, 1.0),
        (1, 2, 1913.0, -1.0),
        (1, 3, 1996.0, 1.0),
        (0, 1, 1990.0, -1.0),
        (0, 2, 187.0, -1.0),
        (0, 3, 1066.0, -1.0),
    ],
    [
        (0, 1, 353.0, 1.0),
        (0, 1, 3627.0, 1.0),
        (0, 2, 1228.0, -1.0),
        (0, 3, 2673.0, 1.0),
        (1, 1, 2111.0, -1.0),
        (1, 2, 335.0, -1.0),
        (1, 3, 121.0, -1.0),
    ],
];

/// One delay line's place in the shared buffer: where it starts, and how long it is at the tuning
/// rate and the paper's size.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    length: f32,
}

/// The buffer all the lines share, each with its own place, moving together.
#[derive(Debug, Clone)]
struct Lines {
    buffer: Vec<f32>,
    mask: usize,
    write: usize,
}

impl Lines {
    #[inline]
    fn read(&self, line: Line, offset: usize) -> f32 {
        self.buffer[(self.write + line.base + offset) & self.mask]
    }

    #[inline]
    fn write(&mut self, line: Line, value: f32) {
        let at = (self.write + line.base) & self.mask;
        self.buffer[at] = value;
    }

    /// What went in this many frames ago, read between samples.
    #[inline]
    fn between(&self, line: Line, frames: f32) -> f32 {
        let whole = frames as usize;
        let fraction = frames - whole as f32;
        let a = self.read(line, whole);
        let b = self.read(line, whole + 1);
        a + (b - a) * fraction
    }

    /// A step through an all-pass this many frames long.
    #[inline]
    fn allpass(&mut self, line: Line, frames: f32, input: f32, diffusion: f32) -> f32 {
        let delayed = self.between(line, frames);
        let sum = input - delayed * diffusion;
        self.write(line, sum);
        delayed + sum * diffusion
    }
}

/// The stereo plate.
#[derive(Debug, Clone)]
pub struct Plate {
    lines: Lines,
    diffusers: [Line; 4],
    /// How long each input all-pass is at this rate, in frames.
    diffuser_frames: [f32; 4],
    tank: [[Line; 4]; 2],
    /// The band-limiting low-pass on what goes in, and each half's damping.
    band: f32,
    damped: [f32; 2],
    /// The wobble, as a point going round a circle, and how far it turns each frame.
    wobble: (f32, f32),
    turn: (f32, f32),
    pre: Vec<f32>,
    pre_write: usize,
    pre_frames: Smoothed,
    /// How far back the pre-delay is read now, which follows `pre_frames` no faster than it may
    /// move.
    pre_now: f32,
    decay: Smoothed,
    damping: Smoothed,
    /// How much of what goes in the band-limiting low-pass lets through each frame.
    bandwidth: Smoothed,
    size: Smoothed,
    /// The size the lines are read at now, which follows `size` no faster than it may move.
    size_now: f32,
    excursion: Smoothed,
    smoothing: f32,
    /// This rate over the tuning rate, which every length is scaled by.
    scale: f32,
    rate: u32,
}

impl Plate {
    /// Make a plate for a sample rate, with lines long enough for the biggest.
    pub fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let mut base = 0;
        let mut place = |longest: f32| {
            let line = Line { base, length: 0.0 };
            // Room to read between samples past the end.
            base += (longest * scale).ceil() as usize + 3;
            line
        };
        let diffusers = DIFFUSER_LENGTHS.map(|length| Line {
            length,
            ..place(length)
        });
        let tank = TANK_LENGTHS.map(|half| {
            let mut first = true;
            half.map(|length| {
                let wobble = if first { MOST_EXCURSION } else { 0.0 };
                first = false;
                Line {
                    length,
                    ..place(length * LARGEST + wobble)
                }
            })
        });
        let size = base.next_power_of_two();
        let pre_frames = (LONGEST_PRE * rate as f32) as usize + 2;
        let turning = std::f64::consts::TAU * WOBBLE_HZ / f64::from(rate);
        let mut plate = Self {
            lines: Lines {
                buffer: vec![0.0; size],
                mask: size - 1,
                write: 0,
            },
            diffusers,
            diffuser_frames: DIFFUSER_LENGTHS.map(|length| (length * scale).round()),
            tank,
            band: 0.0,
            damped: [0.0; 2],
            wobble: (1.0, 0.0),
            turn: (turning.cos() as f32, turning.sin() as f32),
            pre: vec![0.0; pre_frames],
            pre_write: 0,
            pre_frames: Smoothed::at(0.0),
            pre_now: 0.0,
            decay: Smoothed::at(0.0),
            damping: Smoothed::at(1.0),
            bandwidth: Smoothed::at(1.0),
            size: Smoothed::at(1.0),
            size_now: 1.0,
            excursion: Smoothed::at(0.0),
            smoothing: coefficient(rate),
            scale,
            rate,
        };
        plate.follow(Settings::DEFAULT, 0);
        plate.settle();
        plate
    }

    /// Forget what's in it.
    pub fn clear(&mut self) {
        self.lines.buffer.fill(0.0);
        self.pre.fill(0.0);
        self.band = 0.0;
        self.damped = [0.0; 2];
    }

    /// How many frames its buffer holds, for the tests.
    #[cfg(test)]
    fn frames(&self) -> usize {
        self.lines.buffer.len()
    }

    /// Take the settings, and move the wobble to where it is this many frames in. It goes by the
    /// frame rather than by what the plate has done, so one that's slept is where it would have
    /// been.
    pub fn follow(&mut self, settings: Settings, frames: u64) {
        let turns = frames as f64 / f64::from(self.rate) * WOBBLE_HZ;
        let angle = std::f64::consts::TAU * (turns - turns.floor());
        self.wobble = (angle.cos() as f32, angle.sin() as f32);
        self.pre_frames.target = f32::from(settings.pre) / 255.0 * LONGEST_PRE * self.rate as f32;
        self.decay.target = f32::from(settings.decay) / 255.0 * MOST_DECAY;
        let at_tuning_rate = 1.0 - f32::from(settings.damp) / 255.0 * (1.0 - MOST_DAMPING);
        self.damping.target = 1.0 - (1.0 - at_tuning_rate).powf(1.0 / self.scale);
        self.bandwidth.target = if settings.high_cut == 0xFF {
            1.0
        } else {
            let hertz = LOWEST_CUT_HZ * CUT_SPAN.powf(f32::from(settings.high_cut) / 255.0);
            1.0 - (-std::f32::consts::TAU * hertz / self.rate as f32).exp()
        };
        self.size.target = plate_size(f32::from(settings.size));
        self.excursion.target = f32::from(settings.excursion) / 255.0 * MOST_EXCURSION * self.scale;
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.pre_frames.settle();
        self.pre_now = self.pre_frames.value;
        self.decay.settle();
        self.damping.settle();
        self.bandwidth.settle();
        self.size.settle();
        self.size_now = self.size.value;
        self.excursion.settle();
    }

    /// What goes in: both sides together, through the pre-delay and the band-limiting low-pass.
    #[inline]
    fn before_lines(&mut self, input: [f32; 2]) -> f32 {
        let length = self.pre.len();
        self.pre[self.pre_write] = (input[0] + input[1]) * 0.5;
        let wanted = self.pre_frames.next(self.smoothing);
        self.pre_now += (wanted - self.pre_now).clamp(-PRE_PER_FRAME, PRE_PER_FRAME);
        let back = self.pre_now;
        let whole = back as usize;
        let fraction = back - whole as f32;
        let a = self.pre[(self.pre_write + length - whole) % length];
        let b = self.pre[(self.pre_write + length - whole - 1) % length];
        self.pre_write = (self.pre_write + 1) % length;
        let held = if fraction == 0.0 {
            a
        } else {
            a + (b - a) * fraction
        };
        let bandwidth = self.bandwidth.next(self.smoothing);
        self.band += (held - self.band) * bandwidth;
        self.band
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let incoming = self.before_lines(input);
        let decay = self.decay.next(self.smoothing);
        let damping = self.damping.next(self.smoothing);
        let excursion = self.excursion.next(self.smoothing);
        let wanted = self.size.next(self.smoothing);
        let most = SIZE_PER_SECOND / self.rate as f32;
        self.size_now += (wanted - self.size_now).clamp(-most, most);
        let stretch = self.size_now * self.scale;
        let second_diffusion = (decay + 0.15).clamp(SECOND_DIFFUSION.0, SECOND_DIFFUSION.1);
        let (cos, sin) = self.wobble;
        self.wobble = (
            cos * self.turn.0 - sin * self.turn.1,
            sin * self.turn.0 + cos * self.turn.1,
        );

        let lines = &mut self.lines;
        lines.write = lines.write.wrapping_sub(1) & lines.mask;
        let mut diffused = incoming;
        for ((line, frames), diffusion) in self
            .diffusers
            .iter()
            .zip(self.diffuser_frames)
            .zip(DIFFUSIONS)
        {
            diffused = lines.allpass(*line, frames, diffused, diffusion);
        }

        // Each half takes what the other's last delay gives, so it all goes round.
        let wobbles = [cos, sin];
        let mut ends = [0.0; 2];
        for (half, end) in ends.iter_mut().enumerate() {
            let [_, _, _, other_end] = self.tank[1 - half];
            *end = lines.between(other_end, other_end.length * stretch);
        }
        for half in 0..2 {
            let [first, before, second, after] = self.tank[half];
            let mut sample = diffused + ends[1 - half] * decay;
            let wobbled = first.length * stretch + excursion * wobbles[half];
            sample = lines.allpass(first, wobbled, sample, -DECAY_DIFFUSION);
            lines.write(before, sample);
            sample = lines.between(before, before.length * stretch);
            self.damped[half] += (sample - self.damped[half]) * damping;
            sample = self.damped[half] * decay;
            sample = lines.allpass(second, second.length * stretch, sample, second_diffusion);
            lines.write(after, sample);
        }

        let mut out = [0.0; 2];
        for (side, taps) in out.iter_mut().zip(&TAPS) {
            *side = taps
                .iter()
                .map(|&(half, line, frames, sign)| {
                    lines.between(self.tank[half][line], frames * stretch) * sign
                })
                .sum::<f32>()
                * TAP_GAIN
                * WET;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn plate(settings: Settings, rate: u32) -> Plate {
        let mut plate = Plate::new(rate);
        plate.follow(settings, 0);
        plate.settle();
        plate
    }

    fn settings(decay: u8) -> Settings {
        Settings {
            decay,
            ..Settings::DEFAULT
        }
    }

    /// A single frame at full scale, and then what comes out until this many frames.
    fn impulse(plate: &mut Plate, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| {
                let level = if at == 0 { 1.0 } else { 0.0 };
                plate.process([level, level])
            })
            .collect()
    }

    /// How much comes out in each slice of 50 ms.
    fn energies(out: &[[f32; 2]], rate: u32) -> Vec<f32> {
        out.chunks(rate as usize / 20)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect()
    }

    /// How long until it's stayed 60 dB below its loudest, in seconds.
    fn decay_seconds(out: &[[f32; 2]], rate: u32) -> f32 {
        let energies = energies(out, rate);
        let loudest = energies.iter().fold(0.0_f32, |most, e| most.max(*e));
        let last = energies
            .iter()
            .rposition(|e| *e > loudest * 1e-6)
            .unwrap_or(0);
        (last + 1) as f32 * 0.05
    }

    #[test]
    fn an_impulse_gives_a_dense_tail_that_dies_away() {
        let out = impulse(&mut plate(Settings::DEFAULT, RATE), RATE as usize * 6);
        // Dense: once it's built up, most frames have something in them, not a few echoes.
        let tail = &out[RATE as usize / 10..RATE as usize / 2];
        let loud = tail.iter().fold(0.0_f32, |most, f| most.max(f[0].abs()));
        let busy = tail.iter().filter(|f| f[0].abs() > loud * 0.01).count();
        assert!(busy > tail.len() * 3 / 4, "{busy} of {}", tail.len());
        // And it dies away.
        let energies = energies(&out, RATE);
        let (first, last) = (energies[2], energies[energies.len() - 1]);
        assert!(last < first * 1e-4, "{first} {last}");
    }

    #[test]
    fn decay_makes_its_tail_longer_and_still_dies_away_at_the_top() {
        let time = |decay| {
            decay_seconds(
                &impulse(&mut plate(settings(decay), RATE), RATE as usize * 20),
                RATE,
            )
        };
        let (short, middle, long) = (time(0x40), time(0x80), time(0xC0));
        assert!(short > 0.2 && short < 3.0, "{short}");
        assert!(middle > short * 1.5, "{short} {middle}");
        assert!(long > middle * 1.5, "{middle} {long}");
        // At the top it rings for a long time, but each second it's quieter than the last.
        let top = impulse(&mut plate(settings(0xFF), RATE), RATE as usize * 20);
        assert!(top.iter().flatten().all(|s| s.is_finite()));
        let seconds: Vec<f32> = top
            .chunks(RATE as usize)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect();
        for pair in seconds[1..].windows(2) {
            assert!(pair[1] < pair[0], "{seconds:?}");
        }
        assert!(seconds[19] > seconds[1] * 1e-4, "it's long: {seconds:?}");
        assert!(seconds[19] < seconds[1] * 0.5, "it dies away: {seconds:?}");
    }

    #[test]
    fn its_decay_is_about_the_same_at_any_rate() {
        let time = |rate| {
            decay_seconds(
                &impulse(&mut plate(settings(0x80), rate), rate as usize * 8),
                rate,
            )
        };
        let (low, mid, high) = (time(44_100), time(48_000), time(96_000));
        assert!((low / mid - 1.0).abs() < 0.2, "{low} {mid}");
        assert!((high / mid - 1.0).abs() < 0.2, "{high} {mid}");
    }

    /// How much of a tone comes out while it plays and rings.
    fn energy(settings: Settings, hertz: f32) -> f32 {
        let mut plate = plate(settings, RATE);
        let mut total = 0.0;
        for at in 0..RATE as usize * 2 {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            let input = if at < RATE as usize / 4 { tone } else { 0.0 };
            let out = plate.process([input, input]);
            // Only the tail, once what went in has stopped.
            if at > RATE as usize / 2 {
                total += out[0] * out[0] + out[1] * out[1];
            }
        }
        total
    }

    #[test]
    fn damp_darkens_the_tail() {
        let open = Settings {
            damp: 0,
            ..settings(0xC0)
        };
        let dark = Settings { damp: 0xFF, ..open };
        assert!(energy(dark, 6000.0) < energy(open, 6000.0) * 0.01);
        // The lows ring on.
        assert!(energy(dark, 150.0) > energy(open, 150.0) * 0.2);
    }

    #[test]
    fn high_cut_takes_highs_from_what_goes_in() {
        let open = Settings::DEFAULT;
        let cut = Settings {
            high_cut: 0x80,
            ..open
        };
        assert!(energy(cut, 8000.0) < energy(open, 8000.0) * 0.05);
        assert!(energy(cut, 200.0) > energy(open, 200.0) * 0.5);
    }

    #[test]
    fn its_sides_differ_so_it_spreads() {
        let out = impulse(&mut plate(Settings::DEFAULT, RATE), RATE as usize * 2);
        let tail = &out[RATE as usize / 10..];
        let (mut both, mut left, mut right) = (0.0_f64, 0.0_f64, 0.0_f64);
        for [l, r] in tail {
            both += f64::from(l * r);
            left += f64::from(l * l);
            right += f64::from(r * r);
        }
        let correlation = both / (left * right).sqrt();
        assert!(correlation.abs() < 0.2, "{correlation}");
        // About as loud on each side.
        assert!((left / right - 1.0).abs() < 0.5, "{left} {right}");
    }

    #[test]
    fn pre_delay_holds_back_what_comes_out() {
        let first = |pre| {
            let settings = Settings {
                pre,
                ..Settings::DEFAULT
            };
            let out = impulse(&mut plate(settings, RATE), RATE as usize);
            out.iter().position(|f| f[0] != 0.0 || f[1] != 0.0).unwrap()
        };
        let (none, most) = (first(0), first(0xFF));
        let held = (most - none) as f32 / RATE as f32;
        assert!((held - LONGEST_PRE).abs() < 0.002, "{held}");
    }

    #[test]
    fn size_makes_the_plate_bigger_and_its_tail_longer() {
        let out = |size| {
            let settings = Settings {
                size,
                ..Settings::DEFAULT
            };
            impulse(&mut plate(settings, RATE), RATE as usize * 12)
        };
        let (small, big) = (out(0x00), out(0xFF));
        let first = |out: &[[f32; 2]]| out.iter().position(|f| f[0] != 0.0).unwrap();
        assert!(
            first(&big) > first(&small) * 3,
            "{} {}",
            first(&small),
            first(&big)
        );
        assert!(decay_seconds(&big, RATE) > decay_seconds(&small, RATE) * 2.0);
    }

    /// The biggest jump between frames from a tone through it, after a second, with the settings
    /// switching between two every 64 frames, as a knob or an LFO steps them block by block, or
    /// held at the first.
    fn biggest_jump(from: Settings, to: Settings, moving: bool) -> f32 {
        let tone =
            |frame: usize| (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
        let mut plate = plate(from, RATE);
        let mut last = [0.0_f32; 2];
        let mut most = 0.0_f32;
        for frame in 0..RATE as usize * 3 {
            let late = frame > RATE as usize;
            if late {
                let now = if moving && frame / 64 % 2 == 1 {
                    to
                } else {
                    from
                };
                plate.follow(now, frame as u64);
            }
            let out = plate.process([tone(frame); 2]);
            if late {
                most = most.max((out[0] - last[0]).abs().max((out[1] - last[1]).abs()));
            }
            last = out;
        }
        most
    }

    #[test]
    fn moving_any_setting_glides_without_a_step() {
        let plain = Settings::DEFAULT;
        let pairs = [
            ("Pre", plain, Settings { pre: 0xFF, ..plain }),
            ("Decay", settings(0x40), settings(0xFF)),
            (
                "Damp",
                plain,
                Settings {
                    damp: 0xFF,
                    ..plain
                },
            ),
            (
                "High cut",
                Settings {
                    high_cut: 0x60,
                    ..plain
                },
                plain,
            ),
            (
                "Size",
                Settings { size: 0, ..plain },
                Settings {
                    size: 0xFF,
                    ..plain
                },
            ),
            (
                "Mod",
                Settings {
                    excursion: 0,
                    ..plain
                },
                Settings {
                    excursion: 0xFF,
                    ..plain
                },
            ),
            (
                "all",
                Settings {
                    decay: 0x40,
                    high_cut: 0x60,
                    size: 0,
                    excursion: 0,
                    ..plain
                },
                Settings {
                    pre: 0xFF,
                    decay: 0xFF,
                    damp: 0xFF,
                    size: 0xFF,
                    excursion: 0xFF,
                    ..plain
                },
            ),
        ];
        for (name, from, to) in pairs {
            // It jumps no further moving than it does held at either end.
            let still = biggest_jump(from, from, false).max(biggest_jump(to, to, false));
            let moving = biggest_jump(from, to, true);
            assert!(
                moving < still * 1.25,
                "{name}: {moving} moving, {still} still"
            );
        }
    }

    #[test]
    fn it_stays_finite_and_bounded_at_every_extreme() {
        for decay in [0x00, 0xFF] {
            for damp in [0x00, 0xFF] {
                for size in [0x00, 0xFF] {
                    for (rate, excursion, high_cut) in [(44_100, 0x00, 0xFF), (192_000, 0xFF, 0x00)]
                    {
                        let mut plate = plate(
                            Settings {
                                pre: excursion,
                                decay,
                                damp,
                                high_cut,
                                size,
                                excursion,
                                ..Settings::DEFAULT
                            },
                            rate,
                        );
                        for frame in 0..rate as usize * 3 {
                            // Full-scale noise, a full-scale square, DC and silence in turn.
                            let level = match frame / (rate as usize / 4) % 4 {
                                0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                1 => {
                                    if frame / 50 % 2 == 0 {
                                        1.0
                                    } else {
                                        -1.0
                                    }
                                }
                                2 => 1.0,
                                _ => 0.0,
                            };
                            let out = plate.process([level, -level * 0.5]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                                "{decay:x} {damp:x} {size:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn its_lines_hold_the_biggest_plate_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let plate = Plate::new(rate);
            let scale = rate as f32 / TUNING_RATE;
            let needed: f32 = DIFFUSER_LENGTHS.iter().sum::<f32>() * scale
                + TANK_LENGTHS.iter().flatten().sum::<f32>() * LARGEST * scale;
            assert!(plate.frames() as f32 >= needed, "{rate}");
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut plate = plate(Settings::DEFAULT, RATE);
        impulse(&mut plate, 1000);
        plate.clear();
        for _ in 0..RATE {
            assert_eq!(plate.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
