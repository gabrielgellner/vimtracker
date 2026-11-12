//! A stereo delay. It reads between samples and eases to a new time, so changing the time glides,
//! and its feedback goes through filters and a soft clip, so repeats darken, degrade and stay
//! bounded. These follow Vital's delay, by Matt Tytel, GPL 3 or later. Its buffer is made ahead of
//! time, for its longest time at the slowest tempo.
//!
//! `Retime`'s `Jump` and `Wow` follow the Delay of Surge's sst-effects (`effects/Delay.h`, GPL 3 or
//! later, Copyright the Surge Synth Team): a new time crossfades from the old read to the new one
//! over half the new time, ignoring changes while it fades, and a slow wobble moves the two sides'
//! reads opposite ways.

use vt_model::insert::Delay as Settings;
use vt_model::lfo::DIVISION_TICKS;

use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::dsp::filters::filter::{Coefficients, Trim};
use crate::dsp::voices::drums::soft_clip;

/// How much of what's in the line comes out. Tuned by ear with the send levels.
const WET: f32 = 0.6;
/// How long a change of time takes to go half way, in seconds.
const GLIDE_HALF_LIFE: f64 = 0.05;
/// The closest it reads to where it's writing, leaving room to read between samples.
const SHORTEST_FRAMES: f64 = 4.0;
/// How far the soft clip's range is from what goes through it with no `Drive`, so only what's very
/// loud is held back. It's what stops the highest `Feedback` growing without end.
const CLEAN_HEADROOM: f32 = 4.0;
/// How much further `Drive` goes, pushing it in and bringing it back down by as much.
const MOST_DRIVE: f32 = 16.0;
/// The shortest and longest a jump's crossfade lasts, in seconds. Between them it's half the new
/// time, so a long delay fades gently and a short one isn't swamped by fades.
const JUMP_FADE_SECONDS: (f64, f64) = (0.01, 0.5);
/// How fast `Wow` wobbles, in Hz, and how far it bends the repeats' pitch at `FF`, as a share of
/// their speed: half a percent, about 9 cents, either way.
const WOW_HZ: f64 = 0.5;
const MOST_WOW: f64 = 0.005;
/// How much of a turn of the wobble's triangle its corners are rounded over, so the pitch moves
/// between its two bends rather than stepping.
const WOW_ROUNDING: f64 = 1.0 / 16.0;
/// Where the wobble's triangle starts in its turn: at its middle, going down.
const WOW_START: f64 = 0.25;

/// The delay line and what's in its feedback.
#[derive(Debug, Clone)]
pub struct Delay {
    buffer: Vec<[f32; 2]>,
    write: usize,
    /// Whether there's anything to clear.
    used: bool,
    /// How long it is now, and how long it's easing to, in frames.
    frames: f64,
    target_frames: f64,
    glide: f64,
    /// Whether a new time jumps there, crossfading, rather than gliding.
    jump: bool,
    /// Where a jump fades from, in frames, how far through the fade it is, `1` once it's done, and
    /// how far each frame takes it.
    outgoing: f64,
    faded: f64,
    fade_step: f64,
    /// The wobble: where its triangle is in its turn, and how far each frame takes it; where it is
    /// once its corners are rounded, from -1 to 1, and how fast that follows; and how far it moves
    /// the reads at the top of `Wow`, in frames.
    wow_phase: f64,
    wow_step: f64,
    wow_now: f64,
    wow_rounding: f64,
    wow_frames: f64,
    wow: Smoothed,
    feedback: Smoothed,
    width: Smoothed,
    drive: Smoothed,
    smoothing: f32,
    ping_pong: bool,
    trim: [Trim; 2],
    low_cut: Option<(u8, Coefficients)>,
    high_cut: Option<(u8, Coefficients)>,
    rate: u32,
}

impl Delay {
    /// Make a delay that can hold this many frames. Its memory is touched here, so the callback
    /// doesn't have to wait for it to be paged in.
    pub fn new(frames: usize, rate: u32) -> Self {
        let mut buffer = vec![[0.0; 2]; frames.max(8)];
        buffer.fill([0.0; 2]);
        std::hint::black_box(&mut buffer);
        let start = SHORTEST_FRAMES;
        Self {
            buffer,
            write: 0,
            used: false,
            frames: start,
            target_frames: start,
            glide: 1.0 - 0.5_f64.powf(1.0 / (GLIDE_HALF_LIFE * f64::from(rate))),
            jump: false,
            outgoing: start,
            faded: 1.0,
            fade_step: 0.0,
            wow_phase: WOW_START,
            wow_step: WOW_HZ / f64::from(rate),
            wow_now: 0.0,
            wow_rounding: 1.0 - (-WOW_HZ / (WOW_ROUNDING * f64::from(rate))).exp(),
            // A triangle this far either way moves the read this fast at its steepest.
            wow_frames: MOST_WOW * f64::from(rate) / (4.0 * WOW_HZ),
            wow: Smoothed::at(0.0),
            feedback: Smoothed::at(0.0),
            width: Smoothed::at(1.0),
            drive: Smoothed::at(0.0),
            smoothing: coefficient(rate),
            ping_pong: false,
            trim: [Trim::default(); 2],
            low_cut: None,
            high_cut: None,
            rate,
        }
    }

    /// Forget what's in it.
    pub fn clear(&mut self) {
        if self.used {
            self.buffer.fill([0.0; 2]);
            self.used = false;
        }
        self.trim = [Trim::default(); 2];
        self.faded = 1.0;
        self.wow_phase = WOW_START;
        self.wow_now = 0.0;
    }

    /// Take the settings, at a tempo. The time eases to a new value.
    pub fn follow(&mut self, settings: Settings, frames_per_tick: f64) {
        let ticks = DIVISION_TICKS[usize::from(settings.time).min(DIVISION_TICKS.len() - 1)];
        let longest = (self.buffer.len() - 1) as f64 - SHORTEST_FRAMES;
        self.target_frames = (f64::from(ticks) * frames_per_tick).clamp(SHORTEST_FRAMES, longest);
        self.feedback.target = f32::from(settings.feedback) / 255.0;
        self.jump = settings.retime != 0;
        if !self.jump {
            // A glide goes on from where the jump was heading.
            self.faded = 1.0;
        }
        self.wow.target = f32::from(settings.wow) / 255.0;
        self.width.target = f32::from(settings.width) / 255.0;
        self.drive.target = f32::from(settings.drive) / 255.0;
        self.ping_pong = settings.ping_pong != 0;
        let rate = self.rate;
        let update = |slot: &mut Option<(u8, Coefficients)>, value: u8, on: bool| {
            if !on {
                *slot = None;
            } else if slot.as_ref().is_none_or(|(at, _)| *at != value) {
                *slot = Some((
                    value,
                    Coefficients::new(f32::from(value) / 255.0, 0.0, rate),
                ));
            }
        };
        update(&mut self.low_cut, settings.low_cut, settings.low_cut > 0);
        update(
            &mut self.high_cut,
            settings.high_cut,
            settings.high_cut < 0xFF,
        );
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.frames = self.target_frames;
        self.faded = 1.0;
        self.wow.settle();
        self.feedback.settle();
        self.width.settle();
        self.drive.settle();
    }

    /// How long it is now, in frames.
    pub fn frames(&self) -> f64 {
        self.frames
    }

    /// The furthest back it reads now, in frames: past its time while a jump fades from a longer
    /// one, and by as far as `Wow` moves it.
    pub fn reach(&self) -> f64 {
        let outgoing = if self.faded < 1.0 { self.outgoing } else { 0.0 };
        self.frames.max(outgoing) + self.wow_frames * f64::from(self.wow.value)
    }

    /// Where in the buffer is this many frames back, less than its length. Wrapping round by
    /// taking the length away once is cheaper than `%`, and the buffer isn't a power of two
    /// long, as that would nearly double the memory of the longest time and the clearing of it.
    #[inline]
    fn back(&self, frames: usize) -> usize {
        let length = self.buffer.len();
        let at = self.write + length - frames;
        if at >= length { at - length } else { at }
    }

    /// What's in one side of the line this far back, reading between samples through four of
    /// them, so a whole number of frames gives exactly what went in.
    #[inline]
    fn read_side(&self, side: usize, frames: f64) -> f32 {
        let frames = frames.clamp(SHORTEST_FRAMES, (self.buffer.len() - 3) as f64);
        let whole = frames as usize;
        let between = (frames - whole as f64) as f32;
        let at = |back: usize| self.buffer[self.back(back)][side];
        let current = at(whole);
        if between == 0.0 {
            return current;
        }
        let (a, b, c, d) = (at(whole - 1), current, at(whole + 1), at(whole + 2));
        let half = (c - a) * 0.5;
        let w = b - c;
        let v = half + w;
        let slope = v + w + (d - b) * 0.5;
        let bend = v + slope;
        (((slope * between - bend) * between + half) * between) + b
    }

    /// Both sides at a time, with `Wow` moving the left's read one way and the right's the other.
    #[inline]
    fn read(&self, frames: f64, wow: f64) -> [f32; 2] {
        [
            self.read_side(0, frames + wow),
            self.read_side(1, frames - wow),
        ]
    }

    /// How far the wobble moves the left's read now, in frames.
    #[inline]
    fn wobble(&mut self) -> f64 {
        let depth = self.wow.next(self.smoothing);
        self.wow_phase += self.wow_step;
        if self.wow_phase >= 1.0 {
            self.wow_phase -= 1.0;
        }
        let triangle = 4.0 * (self.wow_phase - 0.5).abs() - 1.0;
        self.wow_now += (triangle - self.wow_now) * self.wow_rounding;
        self.wow_now * self.wow_frames * f64::from(depth)
    }

    /// What it reads now, as it glides or jumps to its time.
    #[inline]
    fn retimed(&mut self, wow: f64) -> [f32; 2] {
        if !self.jump {
            self.frames += (self.target_frames - self.frames) * self.glide;
            return self.read(self.frames, wow);
        }
        // A change that comes while a fade runs waits for it, and one of less than half a frame
        // is the tempo's rounding rather than a change.
        if self.faded >= 1.0 && (self.target_frames - self.frames).abs() >= 0.5 {
            self.outgoing = self.frames;
            self.frames = self.target_frames;
            let rate = f64::from(self.rate);
            let fade =
                (self.frames * 0.5).clamp(JUMP_FADE_SECONDS.0 * rate, JUMP_FADE_SECONDS.1 * rate);
            self.faded = 0.0;
            self.fade_step = 1.0 / fade;
        }
        let now = self.read(self.frames, wow);
        if self.faded >= 1.0 {
            return now;
        }
        let before = self.read(self.outgoing, wow);
        let faded = self.faded as f32;
        self.faded = (self.faded + self.fade_step).min(1.0);
        [
            before[0] + (now[0] - before[0]) * faded,
            before[1] + (now[1] - before[1]) * faded,
        ]
    }

    /// Feed one frame in and get what's wet out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let wow = self.wobble();
        let out = self.retimed(wow);
        let feedback = self.feedback.next(self.smoothing);
        let width = self.width.next(self.smoothing);
        let drive = self.drive.next(self.smoothing);
        let push = MOST_DRIVE.powf(drive) / CLEAN_HEADROOM;
        let back = |side: usize, sample: f32, delay: &mut Self| {
            let mut sample = sample;
            if let Some((_, c)) = &delay.low_cut {
                sample = delay.trim[side].low_cut(sample, c);
            }
            if let Some((_, c)) = &delay.high_cut {
                sample = delay.trim[side].high_cut(sample, c);
            }
            soft_clip(sample * push) / push * feedback
        };
        let (left, right) = (back(0, out[0], self), back(1, out[1], self));
        self.buffer[self.write] = if self.ping_pong {
            // All of it goes in on one side, and each side feeds the other.
            [(input[0] + input[1]) * 0.5 + right, left]
        } else {
            [input[0] + left, input[1] + right]
        };
        self.write = if self.write + 1 == self.buffer.len() {
            0
        } else {
            self.write + 1
        };
        self.used = true;
        let middle = (out[0] + out[1]) * 0.5;
        let side = (out[0] - out[1]) * 0.5 * width;
        [(middle + side) * WET, (middle - side) * WET]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    /// 120 BPM at this rate, so 1/4 is 24000 frames.
    const FRAMES_PER_TICK: f64 = 25.0;
    /// `1/4`, `1/8d` and `1/8` in the list of times.
    const QUARTER: u8 = 12;
    const EIGHTH: u8 = 9;

    fn settings() -> Settings {
        Settings {
            feedback: 0,
            ping_pong: 0,
            mix: 0xFF,
            time: QUARTER,
            ..Settings::DEFAULT
        }
    }

    fn delay(settings: Settings) -> Delay {
        let mut delay = Delay::new(RATE as usize * 12, RATE);
        delay.follow(settings, FRAMES_PER_TICK);
        delay.settle();
        delay
    }

    /// A hit, and then what comes out until this many frames.
    fn hit(delay: &mut Delay, level: [f32; 2], frames: usize) -> Vec<[f32; 2]> {
        let mut out = vec![delay.process(level)];
        out.extend((1..frames).map(|_| delay.process([0.0, 0.0])));
        out
    }

    fn sounding(out: &[[f32; 2]]) -> Vec<(usize, [f32; 2])> {
        out.iter()
            .enumerate()
            .filter(|(_, frame)| frame[0] != 0.0 || frame[1] != 0.0)
            .map(|(at, frame)| (at, *frame))
            .collect()
    }

    #[test]
    fn its_time_is_a_note_division_at_the_tempo() {
        let out = hit(&mut delay(settings()), [1.0, 1.0], 30_000);
        assert_eq!(sounding(&out), [(24_000, [WET, WET])]);
        let out = hit(
            &mut delay(Settings {
                time: EIGHTH,
                ..settings()
            }),
            [1.0, 1.0],
            30_000,
        );
        assert_eq!(sounding(&out)[0].0, 12_000);
    }

    #[test]
    fn a_time_between_samples_is_read_between_them() {
        let mut delay = Delay::new(RATE as usize * 12, RATE);
        delay.follow(settings(), 25.51);
        delay.settle();
        // A slow sine comes back close to where the time says it should be.
        let frames = 960.0 * 25.51;
        let sine = |at: f64| (at * 0.01).sin() as f32;
        let mut last = 0.0;
        for at in 0..30_000 {
            last = delay.process([sine(f64::from(at)); 2])[0];
        }
        let expected = sine(29_999.0 - frames) * WET;
        assert!((last - expected).abs() < 1e-3, "{last} {expected}");
    }

    #[test]
    fn a_change_of_time_glides_without_a_jump() {
        let mut delay = delay(settings());
        let sine = |at: usize| (at as f32 * 0.03).sin() * 0.5;
        let mut last = 0.0_f32;
        for at in 0..48_000 {
            last = delay.process([sine(at); 2])[0];
        }
        delay.follow(
            Settings {
                time: EIGHTH,
                ..settings()
            },
            FRAMES_PER_TICK,
        );
        let mut biggest = 0.0_f32;
        for at in 48_000..96_000 {
            let now = delay.process([sine(at); 2])[0];
            biggest = biggest.max((now - last).abs());
            last = now;
        }
        // The pitch bends as it moves, so the slope grows a few times over, and doesn't step.
        assert!(biggest < 0.03 * 0.5 * WET * 8.0, "{biggest}");
        assert!(
            (delay.frames() - 12_000.0).abs() < 1.0,
            "{}",
            delay.frames()
        );
    }

    #[test]
    fn the_top_of_feedback_stays_bounded_and_keeps_repeating() {
        let mut delay = delay(Settings {
            feedback: 0xFF,
            time: 0,
            ..settings()
        });
        let mut peak = 0.0_f32;
        for at in 0..RATE as usize * 4 {
            let level = if at % 100 < 50 { 1.0 } else { -1.0 };
            let out = delay.process([level, level]);
            assert!(out[0].is_finite());
            peak = peak.max(out[0].abs());
        }
        assert!(peak <= CLEAN_HEADROOM * WET * 1.01 + WET, "{peak}");
        let mut quiet = self::delay(Settings {
            feedback: 0xFF,
            ..settings()
        });
        let out = hit(&mut quiet, [0.1, 0.1], 24_000 * 10 + 1);
        assert!(out[24_000 * 10][0].abs() > 0.1 * WET * 0.99, "still there");
    }

    #[test]
    fn feedback_sets_how_fast_repeats_fall() {
        let mut delay = delay(Settings {
            feedback: 0x80,
            ..settings()
        });
        let out = hit(&mut delay, [0.1, 0.1], 24_000 * 2 + 1);
        let ratio = out[48_000][0] / out[24_000][0];
        assert!((ratio - 0x80 as f32 / 255.0).abs() < 0.01, "{ratio}");
    }

    /// How loud a tone at this frequency is after going round the feedback a few times.
    fn after_repeats(settings: Settings, hertz: f32) -> f32 {
        let mut delay = delay(Settings {
            feedback: 0xFF,
            time: 0,
            ..settings
        });
        let frames = 60 * 25;
        let mut peak = 0.0_f32;
        for at in 0..frames * 12 {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            // It fades in and out, as the edges of a sudden burst have highs of their own.
            let fade = (at as f32 / frames as f32 * std::f32::consts::PI)
                .sin()
                .powi(2);
            let input = if at < frames { tone * fade } else { 0.0 };
            let out = delay.process([input, input])[0];
            if at >= frames * 11 {
                peak = peak.max(out.abs());
            }
        }
        peak
    }

    #[test]
    fn the_cuts_take_away_more_with_each_repeat() {
        let open = settings();
        let high_cut = Settings {
            high_cut: 0x80,
            ..open
        };
        assert!(after_repeats(high_cut, 8000.0) < after_repeats(open, 8000.0) * 0.1);
        assert!(after_repeats(high_cut, 200.0) > after_repeats(open, 200.0) * 0.5);
        let low_cut = Settings {
            low_cut: 0x80,
            ..open
        };
        assert!(after_repeats(low_cut, 60.0) < after_repeats(open, 60.0) * 0.1);
        assert!(after_repeats(low_cut, 8000.0) > after_repeats(open, 8000.0) * 0.5);
    }

    #[test]
    fn drive_holds_back_loud_repeats_and_leaves_quiet_ones_about_as_they_are() {
        let driven = Settings {
            drive: 0xFF,
            feedback: 0xFF,
            ..settings()
        };
        let clean = Settings {
            feedback: 0xFF,
            ..settings()
        };
        let second = |settings: Settings, level: f32| {
            hit(&mut delay(settings), [level, level], 48_001)[48_000][0]
        };
        assert!(second(driven, 1.0) < second(clean, 1.0) * 0.5);
        let (quiet_driven, quiet_clean) = (second(driven, 0.001), second(clean, 0.001));
        assert!((quiet_driven / quiet_clean - 1.0).abs() < 0.01);
    }

    #[test]
    fn ping_pong_goes_from_side_to_side_and_off_it_keeps_its_side() {
        let bouncing = Settings {
            ping_pong: 1,
            feedback: 0x80,
            ..settings()
        };
        let out = hit(&mut delay(bouncing), [0.1, 0.1], 72_001);
        assert!(out[24_000][0] > 0.0 && out[24_000][1] == 0.0, "left first");
        assert!(out[48_000][0] == 0.0 && out[48_000][1] > 0.0, "then right");
        assert!(out[72_000][0] > 0.0 && out[72_000][1] == 0.0, "then left");

        let staying = Settings {
            feedback: 0x80,
            ..settings()
        };
        let out = hit(&mut delay(staying), [0.1, 0.0], 48_001);
        assert!(out[24_000][0] > 0.0 && out[24_000][1] == 0.0);
        assert!(out[48_000][0] > 0.0 && out[48_000][1] == 0.0);
    }

    #[test]
    fn width_brings_the_sides_together() {
        let bouncing = Settings {
            ping_pong: 1,
            width: 0,
            ..settings()
        };
        let out = hit(&mut delay(bouncing), [0.1, 0.1], 24_001);
        assert_eq!(out[24_000][0], out[24_000][1]);
        assert!(out[24_000][0] > 0.0);
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut delay = delay(settings());
        delay.process([1.0, 1.0]);
        delay.clear();
        let out = hit(&mut delay, [0.0, 0.0], 30_000);
        assert!(sounding(&out).is_empty());
    }

    #[test]
    fn a_nan_in_the_line_comes_out_once_and_what_goes_round_dies_away() {
        let mut delay = delay(Settings {
            feedback: 0xC0,
            time: 0,
            ..settings()
        });
        delay.process([0.5; 2]);
        let last = (delay.write + delay.buffer.len() - 1) % delay.buffer.len();
        delay.buffer[last] = [f32::NAN; 2];
        // Read once, it's held to the soft clip's end on the way round, so it repeats as a click.
        let out: Vec<f32> = (0..RATE * 4)
            .flat_map(|_| delay.process([0.0; 2]))
            .collect();
        let after = &out[delay.frames() as usize * 2 + 8..];
        assert!(after.iter().all(|s| s.is_finite()));
        assert!(after[after.len() - 100..].iter().all(|s| s.abs() < 1e-4));
    }

    /// A 1 kHz tone through it, all wet, its time changed from a quarter to an eighth after a
    /// second, and how many times each 50 ms of what comes out crosses zero from then on.
    fn crossings_after_a_change(retime: u8) -> Vec<usize> {
        let mut delay = delay(Settings {
            retime,
            ..settings()
        });
        let tone = |at: usize| (at as f32 / RATE as f32 * 1000.0 * std::f32::consts::TAU).sin();
        for at in 0..RATE as usize {
            delay.process([tone(at); 2]);
        }
        delay.follow(
            Settings {
                retime,
                time: EIGHTH,
                ..settings()
            },
            FRAMES_PER_TICK,
        );
        let out: Vec<f32> = (RATE as usize..RATE as usize * 2)
            .map(|at| delay.process([tone(at); 2])[0])
            .collect();
        out.chunks(RATE as usize / 20)
            .map(|chunk| {
                chunk
                    .windows(2)
                    .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
                    .count()
            })
            .collect()
    }

    #[test]
    fn jumping_to_a_new_time_keeps_the_repeats_pitch_where_gliding_bends_it() {
        // 50 ms of 1 kHz crosses zero 100 times.
        let jumped = crossings_after_a_change(1);
        assert!(jumped.iter().all(|n| n.abs_diff(100) <= 1), "{jumped:?}");
        let glided = crossings_after_a_change(0);
        assert!(glided.iter().any(|n| n.abs_diff(100) > 10), "{glided:?}");
    }

    #[test]
    fn a_jump_crossfades_without_a_step_and_lands_on_the_new_time() {
        let jumping = Settings {
            retime: 1,
            ..settings()
        };
        let mut delay = delay(jumping);
        let sine = |at: usize| (at as f32 * 0.03).sin() * 0.5;
        let mut last = 0.0_f32;
        for at in 0..48_000 {
            last = delay.process([sine(at); 2])[0];
        }
        delay.follow(
            Settings {
                time: EIGHTH,
                ..jumping
            },
            FRAMES_PER_TICK,
        );
        let mut biggest = 0.0_f32;
        for at in 48_000..96_000 {
            let now = delay.process([sine(at); 2])[0];
            biggest = biggest.max((now - last).abs());
            last = now;
        }
        // No steeper than the sine itself: the two reads only ever add up to it.
        assert!(biggest <= 0.03 * 0.5 * WET * 1.01, "{biggest}");
        assert_eq!(delay.frames(), 12_000.0);
    }

    #[test]
    fn a_change_while_a_jump_fades_waits_for_it_to_finish() {
        let jumping = Settings {
            retime: 1,
            ..settings()
        };
        let mut delay = delay(jumping);
        delay.follow(
            Settings {
                time: EIGHTH,
                ..jumping
            },
            FRAMES_PER_TICK,
        );
        delay.process([0.0; 2]);
        assert_eq!(delay.frames(), 12_000.0);
        // Half the new time: 6000 frames.
        delay.follow(jumping, FRAMES_PER_TICK);
        for _ in 0..5_990 {
            delay.process([0.0; 2]);
        }
        assert_eq!(delay.frames(), 12_000.0);
        assert_eq!(delay.reach(), 24_000.0);
        for _ in 0..20 {
            delay.process([0.0; 2]);
        }
        assert_eq!(delay.frames(), 24_000.0);
    }

    #[test]
    fn wow_bends_the_two_sides_pitch_opposite_ways() {
        let crossings = |wow: u8| {
            let mut delay = delay(Settings {
                time: 0,
                wow,
                ..settings()
            });
            let tone = |at: usize| (at as f32 / RATE as f32 * 1000.0 * std::f32::consts::TAU).sin();
            let out: Vec<[f32; 2]> = (0..RATE as usize * 3 / 2)
                .map(|at| delay.process([tone(at); 2]))
                .collect();
            // From a little past half a second, a turn and a half in, to a little before a second
            // and a half, its triangle rises: the left's read moves back and the right's forward.
            let rising = &out[RATE as usize * 6 / 10..RATE as usize * 14 / 10];
            let count = |side: usize| {
                rising
                    .windows(2)
                    .filter(|w| (w[0][side] < 0.0) != (w[1][side] < 0.0))
                    .count()
            };
            [count(0), count(1)]
        };
        // 0.8 s of 1 kHz crosses zero 1600 times, and half a percent is 8 either way.
        assert_eq!(crossings(0).map(|n| n.abs_diff(1600) <= 1), [true; 2]);
        let [left, right] = crossings(0xFF);
        assert!((1588..1596).contains(&left), "{left}");
        assert!((1604..1612).contains(&right), "{right}");
    }

    #[test]
    fn without_wow_both_sides_read_from_the_same_place() {
        let mut delay = delay(Settings {
            time: 1,
            ..settings()
        });
        delay.follow(settings(), 25.3);
        for at in 0..30_000 {
            let x = (at as f32 * 0.01).sin();
            let out = delay.process([x, -x]);
            assert_eq!(out[0], -out[1]);
        }
    }
}
