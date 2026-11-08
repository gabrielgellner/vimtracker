//! A look-ahead peak limiter, after Geraint Luff's "Designing a straightforward limiter" for
//! Signalsmith Audio (https://signalsmith-audio.co.uk/writing/2022/limiter/). x42's `peaklim.cc` in
//! dpl.lv2 (GPL 3 or later) was read for comparison, and nothing here is taken from it.
//!
//! The sound waits in a short line while the gain it needs is worked out ahead of it. Each frame
//! asks for the gain that brings its louder side down to the ceiling. The least asked for over
//! the look-ahead is held, so a peak is known about before it arrives. A one-pole release lets go
//! of it slowly, and a moving average as long as the look-ahead smooths the turn down, so it's
//! already as far down as a peak needs when the peak comes out. Both sides share one gain, so the
//! stereo picture doesn't move. It doesn't oversample, so a peak between samples can go a little
//! over.

use vt_model::insert::{Limiter as Settings, ceiling_db, input_db, limit_release_ms};

use crate::dsp::smoother::{Smoothed, coefficient};

/// How far ahead it looks, in ms. Everything through it comes out this late.
pub const LOOK_AHEAD_MS: f32 = 2.0;

/// A gain from dB.
fn gain_of(db: f32) -> f32 {
    10_f32.powf(db / 20.0)
}

/// A limiter's lines and the gain it's worked out.
#[derive(Debug, Clone)]
pub struct Limiter {
    /// The sound, driven and over the ceiling as it was going in, waiting to come out: it's limited
    /// to no more than 1, and the ceiling brought back as it comes out.
    lines: [Vec<f32>; 2],
    /// The same sound as it came in, so what's mixed in dry is as late.
    dry: [Vec<f32>; 2],
    mask: usize,
    write: usize,
    /// How many frames it looks ahead, which is how late the sound comes out.
    ahead: usize,
    /// The least gain asked for over the window, as a queue that rises from front to back. Each has
    /// the frame it was asked for in.
    held: Vec<(u32, f32)>,
    held_front: usize,
    held_count: usize,
    /// Frames counted, for when what's held is too old.
    now: u32,
    /// What's let go after the hold.
    released: f32,
    /// The gain it last turned the sound down by, before the ceiling, for a meter.
    turned: f32,
    /// The released gains over the window, and their total, for the moving average.
    averaged: Vec<f32>,
    averaged_at: usize,
    total: f64,
    /// How much it turns up what goes in, and the most that comes out, as gains.
    drive: Smoothed,
    ceiling: Smoothed,
    /// How much of the last gain the release keeps each frame.
    keep: f32,
    rate: u32,
    /// The settings' glide when it's run without one given.
    smoothing: f32,
}

impl Limiter {
    /// Lines long enough to look ahead at this rate.
    pub fn new(rate: u32) -> Self {
        let ahead = ((LOOK_AHEAD_MS / 1000.0 * rate as f32).round() as usize).max(1);
        // The window takes in the frame coming out and every frame behind it in the line.
        let window = ahead + 1;
        let size = window.next_power_of_two();
        let mut limiter = Self {
            lines: [vec![0.0; size], vec![0.0; size]],
            dry: [vec![0.0; size], vec![0.0; size]],
            mask: size - 1,
            write: 0,
            ahead,
            held: vec![(0, 1.0); size],
            held_front: 0,
            held_count: 0,
            now: 0,
            released: 1.0,
            turned: 1.0,
            averaged: vec![1.0; window],
            averaged_at: 0,
            total: window as f64,
            drive: Smoothed::at(1.0),
            ceiling: Smoothed::at(1.0),
            keep: 0.0,
            rate,
            smoothing: coefficient(rate),
        };
        limiter.follow(Settings::DEFAULT);
        limiter.settle();
        limiter
    }

    /// How long anything stays in it, in frames, which is more than it looks ahead.
    pub fn frames(&self) -> usize {
        self.mask + 1
    }

    /// How late it puts the sound, in frames.
    pub fn latency(&self) -> usize {
        self.ahead
    }

    /// How far it's turning down now, in dB, for a meter: what it's taking off peaks over the
    /// ceiling, not the ceiling itself.
    pub fn reduction(&self) -> f32 {
        -20.0 * self.turned.log10()
    }

    /// Forget what's in it, for a fresh start.
    pub fn clear(&mut self) {
        for line in self.lines.iter_mut().chain(&mut self.dry) {
            line.fill(0.0);
        }
        self.rest();
    }

    /// Let go entirely, as it would have after a long silence.
    pub fn rest(&mut self) {
        self.held_count = 0;
        self.released = 1.0;
        self.turned = 1.0;
        self.averaged.fill(1.0);
        self.total = self.averaged.len() as f64;
    }

    /// Take what's set now.
    pub fn follow(&mut self, settings: Settings) {
        self.drive.target = gain_of(input_db(f32::from(settings.input)));
        self.ceiling.target = gain_of(ceiling_db(f32::from(settings.ceiling)));
        let frames = limit_release_ms(f32::from(settings.release)) / 1000.0 * self.rate as f32;
        self.keep = (-1.0 / frames.max(1.0)).exp();
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        self.drive.settle();
        self.ceiling.settle();
    }

    /// One frame in, at its own glide, and the limited frame out.
    #[inline]
    pub fn process_alone(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.process(input, self.smoothing)[0]
    }

    /// The least gain asked for over the window, with this frame's added.
    #[inline]
    fn hold(&mut self, asked: f32) -> f32 {
        let size = self.held.len();
        let mask = size - 1;
        // What's at least as much as this frame asks for can never be the least again.
        while self.held_count > 0 {
            let back = (self.held_front + self.held_count - 1) & mask;
            if self.held[back].1 < asked {
                break;
            }
            self.held_count -= 1;
        }
        let back = (self.held_front + self.held_count) & mask;
        self.held[back] = (self.now, asked);
        self.held_count += 1;
        let window = self.ahead as u32 + 1;
        while self.now.wrapping_sub(self.held[self.held_front].0) >= window {
            self.held_front = (self.held_front + 1) & mask;
            self.held_count -= 1;
        }
        self.now = self.now.wrapping_add(1);
        self.held[self.held_front].1
    }

    /// The average of the window's released gains, with this one added.
    #[inline]
    fn average(&mut self, gain: f32) -> f32 {
        let oldest = std::mem::replace(&mut self.averaged[self.averaged_at], gain);
        self.averaged_at += 1;
        if self.averaged_at == self.averaged.len() {
            self.averaged_at = 0;
        }
        self.total += f64::from(gain) - f64::from(oldest);
        (self.total / self.averaged.len() as f64) as f32
    }

    /// One frame in, and out of the far end of the line: the limited frame, all wet, and the dry
    /// one, each as late as the other.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32) -> [[f32; 2]; 2] {
        let drive = self.drive.next(smoothing);
        let ceiling = self.ceiling.next(smoothing);
        let scale = drive / ceiling;
        let scaled = input.map(|sample| sample * scale);
        self.write = (self.write + 1) & self.mask;
        for side in 0..2 {
            self.lines[side][self.write] = scaled[side];
            self.dry[side][self.write] = input[side];
        }
        let peak = scaled[0].abs().max(scaled[1].abs());
        let asked = if peak > 1.0 { 1.0 / peak } else { 1.0 };
        let held = self.hold(asked);
        // It turns down as soon as it's asked, and the average spreads that over the look-ahead.
        self.released = if held < self.released {
            held
        } else {
            held + (self.released - held) * self.keep
        };
        self.turned = self.average(self.released);
        let gain = self.turned * ceiling;
        let read = self.write.wrapping_sub(self.ahead) & self.mask;
        [
            [self.lines[0][read] * gain, self.lines[1][read] * gain],
            [self.dry[0][read], self.dry[1][read]],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: u32 = 48_000;

    fn limiter(input: u8, ceiling: u8, release: u8) -> Limiter {
        let mut limiter = Limiter::new(RATE);
        limiter.follow(Settings {
            input,
            ceiling,
            release,
            ..Settings::DEFAULT
        });
        limiter.settle();
        limiter
    }

    fn sine(frame: usize, hertz: f32, level: f32) -> f32 {
        (frame as f32 / RATE as f32 * hertz * TAU).sin() * level
    }

    /// The most it lets out over some frames, from `level` going in.
    fn loudest(limiter: &mut Limiter, frames: usize, level: impl Fn(usize) -> f32) -> f32 {
        (0..frames)
            .flat_map(|frame| limiter.process([level(frame), -level(frame) * 0.5], 1.0)[0])
            .fold(0.0_f32, |most, sample| most.max(sample.abs()))
    }

    #[test]
    fn it_looks_two_milliseconds_ahead_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let limiter = Limiter::new(rate);
            let expected = (rate as f32 * 0.002).round() as usize;
            assert_eq!(limiter.latency(), expected, "{rate}");
            assert!(limiter.frames() > limiter.latency());
        }
    }

    #[test]
    fn a_loud_transient_never_goes_over_the_ceiling() {
        for ceiling in [0x00, 0x80, 0xFC, 0xFF] {
            let most = gain_of(ceiling_db(f32::from(ceiling)));
            let mut limiter = limiter(0x00, ceiling, 0x00);
            // Quiet, then a click 24 dB over full level out of nowhere, then a loud burst.
            let level = |frame: usize| match frame {
                1000 => 16.0,
                2000..3000 => sine(frame, 3_000.0, 8.0),
                _ => sine(frame, 110.0, 0.05),
            };
            let got = loudest(&mut limiter, RATE as usize / 10, level);
            assert!(got <= most * 1.0001, "{ceiling:x}: {got} over {most}");
            assert!(got > most * 0.99, "{ceiling:x}: it gets right up to it");
        }
    }

    #[test]
    fn at_full_input_it_holds_the_ceiling() {
        for release in [0x00, 0xAA, 0xFF] {
            let mut limiter = limiter(0xFF, Settings::DEFAULT.ceiling, release);
            let most = gain_of(ceiling_db(f32::from(Settings::DEFAULT.ceiling)));
            let got = loudest(&mut limiter, RATE as usize, |frame| {
                sine(frame, 60.0, 0.9) + sine(frame, 5_000.0, 0.3)
            });
            assert!(got <= most * 1.0001, "{release:x}: {got} over {most}");
        }
    }

    #[test]
    fn below_the_ceiling_it_only_delays_the_sound() {
        let mut limiter = limiter(0x00, 0xFF, 0xAA);
        let ahead = limiter.latency();
        let level = |frame: usize| sine(frame, 440.0, 0.7);
        for frame in 0..RATE as usize / 10 {
            let [wet, dry] = limiter.process([level(frame), -level(frame)], 1.0);
            let expected = frame.checked_sub(ahead).map_or(0.0, level);
            assert!((wet[0] - expected).abs() < 1e-6, "{frame}");
            assert!((wet[1] + expected).abs() < 1e-6, "{frame}");
            assert_eq!(dry, [expected, -expected], "{frame}");
        }
    }

    /// How far it's turned down, as a gain, at some frames after a loud second ends.
    fn let_go(release: u8, after: usize) -> f32 {
        let mut limiter = limiter(0x00, 0xFF, release);
        for frame in 0..RATE as usize {
            limiter.process([sine(frame, 200.0, 4.0); 2], 1.0);
        }
        let mut gain = 0.0;
        for frame in 0..after {
            // A steady quiet tone, so what comes out over what went in is the gain.
            let quiet = 0.1;
            let out = limiter.process([quiet; 2], 1.0)[0][0];
            if frame + 1 == after {
                gain = out / quiet;
            }
        }
        gain
    }

    #[test]
    fn it_lets_go_over_its_release_time() {
        let early = let_go(0xAA, RATE as usize / 100);
        let later = let_go(0xAA, RATE as usize / 5);
        let long = let_go(0xAA, RATE as usize);
        // 100 ms: still well down after 10 ms, most of the way back after 200 ms, back after a second.
        assert!(early < 0.5, "{early}");
        assert!(later > 0.8 && later > early, "{later}");
        assert!(long > 0.999, "{long}");
        // A shorter release is back sooner, and a longer one later.
        assert!(let_go(0x00, RATE as usize / 50) > 0.999);
        assert!(let_go(0xFF, RATE as usize / 5) < 0.5);
    }

    #[test]
    fn moving_the_input_or_ceiling_glides_without_a_step() {
        let mut limiter = limiter(0x00, 0xFF, 0x60);
        let smoothing = coefficient(RATE);
        let mut last = [0.0; 2];
        for _ in 0..RATE / 10 {
            last = limiter.process([0.5, 0.4], smoothing)[0];
        }
        for frame in 0..RATE as usize {
            // A jump every 64 frames, as a knob or an LFO steps it block by block.
            let step = (frame / 64 % 2) as u8;
            limiter.follow(Settings {
                input: step * 0xFF,
                ceiling: 0xFF - step * 0xFF,
                release: 0x60 + step * 0x9F,
                ..Settings::DEFAULT
            });
            let out = limiter.process([0.5, 0.4], smoothing)[0];
            for side in 0..2 {
                assert!(
                    (out[side] - last[side]).abs() < 0.02,
                    "{frame}: {last:?} to {out:?}"
                );
            }
            last = out;
        }
    }

    #[test]
    fn it_stays_finite_and_under_the_ceiling_at_every_extreme() {
        for input in [0x00, 0xFF] {
            for ceiling in [0x00, 0xFF] {
                for release in [0x00, 0xFF] {
                    for rate in [44_100, 192_000] {
                        let mut limiter = Limiter::new(rate);
                        limiter.follow(Settings {
                            input,
                            ceiling,
                            release,
                            ..Settings::DEFAULT
                        });
                        limiter.settle();
                        let most = gain_of(ceiling_db(f32::from(ceiling)));
                        for frame in 0..rate as usize / 4 {
                            // Full-scale noise, silence and a huge step in turn.
                            let level = match frame / 2000 % 3 {
                                0 => (frame * 7919 % 2003) as f32 / 1001.5 - 1.0,
                                1 => 0.0,
                                _ => 1000.0,
                            };
                            let [wet, dry] = limiter.process([level, -level], 1.0);
                            assert!(wet.iter().chain(&dry).all(|s| s.is_finite()));
                            assert!(wet.iter().all(|s| s.abs() <= most * 1.0001));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn both_sides_share_one_gain() {
        let mut limiter = limiter(0x00, 0xFF, 0xAA);
        for frame in 0..RATE as usize / 10 {
            // Only the left side is loud, and the right is turned down with it.
            let [wet, dry] = limiter.process([sine(frame, 100.0, 4.0), 0.25], 1.0);
            if frame > RATE as usize / 20 {
                assert!(wet[1] < dry[1] * 0.5, "{frame}: {wet:?}");
            }
        }
    }
}
