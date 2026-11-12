//! A stereo wash of reverb that goes on and on, after Airwindows' Galactic: what goes in is
//! wobbled by a vibrato in a short pre-delay and darkened, then goes round a tank of twelve plain
//! delay lines a side, three blocks of four, each block mixing its four as each one less the other
//! three. What comes out of the last block goes back into the first block of the other side, so
//! the sides feed each other, and the four lines of the last block summed are what's heard, through
//! the same darkening again. The tank has no damping of its own and no reads between samples, so
//! it's cheap and its tail stays bright and dense for as long as it rings.
//!
//! At twice Airwindows' 44.1 kHz or more, the tank runs once every two, three or four frames, as
//! Galactic's does, and what comes out is drawn between its frames; the vibrato is slowed and
//! deepened to match, so it sounds the same at any rate. Galactic picks the vibrato's next speed
//! from its dither's noise each time round; here that's `rng.rs`, seeded afresh whenever it's
//! cleared, so a render repeats. Its dry/wet is the slot's own `Mix`. At the top of its `Replace`
//! the tank keeps all it has for ever; here `Decay` stops short of that, so the longest tail still
//! dies away. The darkening's one-poles are scaled to the rate exactly, where Galactic divides by
//! the square root of how many times 44.1 kHz it runs at.
//!
//! Adapted from `src/GalacticProc.cpp` in Airwindows' plugins as Surge XT ships them, at
//! <https://github.com/surge-synthesizer/surge> (`libs/airwindows`), MIT:
//!
//! Copyright (c) 2018 Chris Johnson.
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

use std::f32::consts::TAU;

use vt_model::insert::Space as Settings;

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::rng::Pcg32;

/// The rate Galactic's lengths and filters are tuned at.
const TUNING_RATE: f32 = 44_100.0;
/// The tank's lines at Galactic's size of one, in the tank's frames: the first block's four, the
/// second's and the third's.
const LENGTHS: [[f32; 4]; 3] = [
    [3407.0, 1823.0, 859.0, 331.0],
    [4801.0, 2909.0, 1153.0, 461.0],
    [7607.0, 4217.0, 2269.0, 1597.0],
];
/// How big the tank is, as so many times those lengths, at the bottom and the top of `Size`.
const SMALLEST: f32 = 0.1;
const LARGEST: f32 = 1.87;
/// The most of what's in the tank each trip round keeps, as Galactic's `regen` at its `Replace`'s
/// ends: each trip goes through three blocks that each double what's in them, so at an eighth it
/// keeps it all.
const LEAST_REGEN: f32 = 0.0625;
const KEEPS_ALL: f32 = 0.125;
/// How close to `Replace`'s bottom the top of `Decay` goes, where the tank would keep all it has.
/// Here each trip keeps 97.5%, so at the biggest size the tail falls 60 dB in well over a minute.
const LEAST_REPLACE: f32 = 0.05;
/// The vibrato's pre-delay, its depth either way, and its speed at the top of `Mod`, all at the
/// tuning rate: radians a frame for each turn of its speed.
const VIBRATO_FRAMES: f32 = 256.0;
const VIBRATO_DEPTH: f32 = 127.0;
const MOST_DRIFT: f32 = 0.001;
/// How much the darkening's one-poles let through each frame at the tuning rate, at each end of
/// `Damp`, as the square of Galactic's `Brightness`.
const DAMP_BRIGHTNESS: (f32, f32) = (1.0, 0.1);
/// What the tank's feedback and what comes out of it are held to, far past anything a sound makes
/// in it, so a NaN that reached it is held too and dies away (#461).
const HEADROOM: f32 = 64.0;
/// How often what's slow to work out is worked out again, in frames.
const BLOCK: u32 = 32;
/// How loud what comes out is. Tuned so it's near the reverb's at both their defaults.
const WET: f32 = 1.6;
/// Where the vibrato's random speeds start, whenever it's cleared.
const SEED: u64 = 0x4741_4C41_4354;

/// One of the tank's lines: where it starts in the shared buffer, how long its place is less one,
/// a power of two, and how many of the tank's frames it holds back now.
#[derive(Debug, Clone, Copy)]
struct Line {
    base: usize,
    mask: usize,
    length: f32,
    delay: usize,
}

/// One side's twelve lines, and what its last block gives back to the other side.
#[derive(Debug, Clone, Copy)]
struct Side {
    blocks: [[Line; 4]; 3],
    feedback: [f32; 4],
    /// What came out of the tank last, from the frame before, for drawing between its frames.
    references: [f32; 5],
    /// The darkening before the tank and after it.
    before: f32,
    after: f32,
}

/// Each line's own value as each one less the other three.
#[inline]
fn mixed(outs: [f32; 4]) -> [f32; 4] {
    let sum: f32 = outs.iter().sum();
    outs.map(|out| out + out - sum)
}

/// The stereo space.
#[derive(Debug, Clone)]
pub struct Space {
    /// Every line of both sides, each in a place of its own.
    buffer: Vec<f32>,
    /// Where each line is written on this frame of the tank. It goes down by one each.
    write: usize,
    sides: [Side; 2],
    /// The vibrato's lines, one a side, and where they're written this frame.
    vibrato: Vec<f32>,
    vibrato_mask: usize,
    vibrato_write: usize,
    /// How far round the vibrato is, and its speed, which changes each time it goes round.
    phase: f32,
    speed: f32,
    /// How many frames the tank runs once every, and where in them it is.
    every: usize,
    cycle: usize,
    decay: Smoothed,
    size: Smoothed,
    damp: Smoothed,
    drift: Smoothed,
    smoothing: f32,
    /// How many times the tuning rate this is.
    scale: f32,
    rng: Pcg32,
    /// What each trip round the tank keeps, how much of what comes in goes into it, and how much
    /// the darkening lets through each frame, worked out every block.
    regen: f32,
    attenuate: f32,
    lowpass: f32,
    /// Frames until they're worked out again.
    countdown: u32,
}

impl Space {
    /// Make one for a sample rate, with lines long enough for the biggest.
    pub fn new(rate: u32) -> Self {
        let scale = rate as f32 / TUNING_RATE;
        let every = (scale.floor() as usize).clamp(1, 4);
        let mut base = 0;
        let mut place = |length: f32| {
            let room = ((length * LARGEST) as usize + 2).next_power_of_two();
            let line = Line {
                base,
                mask: room - 1,
                length,
                delay: 1,
            };
            base += room;
            line
        };
        let side = |place: &mut dyn FnMut(f32) -> Line| Side {
            blocks: LENGTHS.map(|block| block.map(&mut *place)),
            feedback: [0.0; 4],
            references: [0.0; 5],
            before: 0.0,
            after: 0.0,
        };
        let sides = [side(&mut place), side(&mut place)];
        let vibrato_room = ((VIBRATO_FRAMES + 2.0) as usize * every).next_power_of_two();
        let mut space = Self {
            buffer: vec![0.0; base],
            write: 0,
            sides,
            vibrato: vec![0.0; vibrato_room * 2],
            vibrato_mask: vibrato_room - 1,
            vibrato_write: 0,
            phase: 0.0,
            speed: 0.0,
            every,
            cycle: 0,
            decay: Smoothed::at(0.5),
            size: Smoothed::at(1.0),
            damp: Smoothed::at(0.5),
            drift: Smoothed::at(0.5),
            smoothing: coefficient(rate),
            scale,
            rng: Pcg32::new(SEED, 0),
            regen: LEAST_REGEN,
            attenuate: 0.0,
            lowpass: 1.0,
            countdown: 0,
        };
        space.clear();
        space.follow(Settings::DEFAULT);
        space.settle();
        space
    }

    /// Forget what's in it, and start its random speeds again.
    pub fn clear(&mut self) {
        self.buffer.fill(0.0);
        self.vibrato.fill(0.0);
        for side in &mut self.sides {
            side.feedback = [0.0; 4];
            side.references = [0.0; 5];
            side.before = 0.0;
            side.after = 0.0;
        }
        self.cycle = 0;
        self.rng = Pcg32::new(SEED, 0);
        // Galactic starts its vibrato three radians in.
        self.phase = 3.0;
        self.speed = self.next_speed();
    }

    /// How many frames its lines hold, for the tests.
    #[cfg(test)]
    fn frames(&self) -> usize {
        self.buffer.len()
    }

    /// The vibrato's next speed, as Galactic's `oldfpd` from its noise.
    fn next_speed(&mut self) -> f32 {
        (0.429_496_73 + f64::from(self.rng.next_u32()) * 0.000_000_000_061_8) as f32
    }

    /// Take the settings.
    pub fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.decay.target = share(settings.decay);
        self.size.target = share(settings.size);
        self.damp.target = share(settings.damp);
        self.drift.target = share(settings.excursion);
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.decay.settle();
        self.size.settle();
        self.damp.settle();
        self.drift.settle();
        self.countdown = 0;
    }

    /// Work out what's slow to, from the settings as they are now.
    fn update(&mut self) {
        let replace = 1.0 - self.decay.value * (1.0 - LEAST_REPLACE);
        self.regen = LEAST_REGEN + (1.0 - replace) * (KEEPS_ALL - LEAST_REGEN);
        self.attenuate = (1.0 - self.regen / KEEPS_ALL) * 1.333;
        let damp = self.damp.value;
        let brightness = DAMP_BRIGHTNESS.0 + (DAMP_BRIGHTNESS.1 - DAMP_BRIGHTNESS.0) * damp;
        self.lowpass = 1.0 - (1.0 - brightness * brightness).powf(1.0 / self.scale);
        let bigness = SMALLEST + (LARGEST - SMALLEST) * self.size.value;
        for side in &mut self.sides {
            for line in side.blocks.iter_mut().flatten() {
                line.delay = ((line.length * bigness) as usize).max(1);
            }
        }
    }

    /// One frame of the tank, for both sides, from what goes into each, giving what comes out.
    #[inline]
    fn tank(&mut self, input: [f32; 2], regen: f32) -> [f32; 2] {
        self.write = self.write.wrapping_sub(1);
        let write = self.write;
        let buffer = &mut self.buffer;
        let mut pass = |line: Line, value: f32| {
            buffer[line.base + (write & line.mask)] = value;
            buffer[line.base + (write.wrapping_add(line.delay) & line.mask)]
        };
        // Each side's first block takes the other's feedback.
        let feedback = [self.sides[1].feedback, self.sides[0].feedback];
        let mut out = [0.0; 2];
        for ((side, fed), (input, out)) in self
            .sides
            .iter_mut()
            .zip(feedback)
            .zip(input.into_iter().zip(&mut out))
        {
            let [first, second, third] = side.blocks;
            let outs: [f32; 4] = std::array::from_fn(|at| pass(first[at], input + fed[at] * regen));
            let into = mixed(outs);
            let outs: [f32; 4] = std::array::from_fn(|at| pass(second[at], into[at]));
            let into = mixed(outs);
            let outs: [f32; 4] = std::array::from_fn(|at| pass(third[at], into[at]));
            side.feedback = mixed(outs).map(|value| within(value, -HEADROOM, HEADROOM));
            *out = within(outs.iter().sum::<f32>() / 8.0, -HEADROOM, HEADROOM);
        }
        out
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.decay.next(self.smoothing);
        self.size.next(self.smoothing);
        self.damp.next(self.smoothing);
        let drift = self.drift.next(self.smoothing);
        if self.countdown == 0 {
            self.update();
            self.countdown = BLOCK;
        }
        self.countdown -= 1;
        let (regen, attenuate, lowpass) = (self.regen, self.attenuate, self.lowpass);

        // The vibrato, slowed as the tank is when it runs once every few frames.
        let every = self.every as f32;
        self.phase += self.speed * drift * drift * drift * MOST_DRIFT / every;
        if self.phase > TAU {
            self.phase = 0.0;
            self.speed = self.next_speed();
        }
        self.vibrato_write = self.vibrato_write.wrapping_sub(1);
        let room = self.vibrato_mask + 1;
        // The right side's vibrato is a quarter turn on from the left's.
        let (sin, cos) = self.phase.sin_cos();
        let mut wobbled = [0.0; 2];
        for (side, ((input, wobbled), turn)) in input
            .into_iter()
            .zip(&mut wobbled)
            .zip([sin, cos])
            .enumerate()
        {
            let base = side * room;
            let at =
                |back: usize| base + (self.vibrato_write.wrapping_add(back) & self.vibrato_mask);
            self.vibrato[at(0)] = input * attenuate;
            let back = (VIBRATO_FRAMES - (turn + 1.0) * VIBRATO_DEPTH) * every;
            let whole = back as usize;
            let fraction = back - whole as f32;
            let (a, b) = (self.vibrato[at(whole)], self.vibrato[at(whole + 1)]);
            *wobbled = a + (b - a) * fraction;
        }

        let mut into = [0.0; 2];
        for ((side, wobbled), into) in self.sides.iter_mut().zip(wobbled).zip(&mut into) {
            side.before += (wobbled - side.before) * lowpass;
            *into = side.before;
        }
        self.cycle += 1;
        if self.cycle == self.every {
            let tank = self.tank(into, regen);
            for (side, sample) in self.sides.iter_mut().zip(tank) {
                let refs = &mut side.references;
                // Drawn between the last frame of the tank and this one, as Galactic does.
                match self.every {
                    4 => {
                        refs[0] = refs[4];
                        refs[2] = (refs[0] + sample) / 2.0;
                        refs[1] = (refs[0] + refs[2]) / 2.0;
                        refs[3] = (refs[2] + sample) / 2.0;
                        refs[4] = sample;
                    }
                    3 => {
                        refs[0] = refs[3];
                        refs[2] = (refs[0] + refs[0] + sample) / 3.0;
                        refs[1] = (refs[0] + sample + sample) / 3.0;
                        refs[3] = sample;
                    }
                    2 => {
                        refs[0] = refs[2];
                        refs[1] = (refs[0] + sample) / 2.0;
                        refs[2] = sample;
                    }
                    _ => refs[0] = sample,
                }
            }
            self.cycle = 0;
        }
        let cycle = self.cycle;
        let mut out = [0.0; 2];
        for (side, out) in self.sides.iter_mut().zip(&mut out) {
            side.after += (side.references[cycle] - side.after) * lowpass;
            *out = side.after * WET;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn space(settings: Settings, rate: u32) -> Space {
        let mut space = Space::new(rate);
        space.follow(settings);
        space.settle();
        space
    }

    fn settings(decay: u8) -> Settings {
        Settings {
            decay,
            ..Settings::DEFAULT
        }
    }

    /// A single frame at full scale, and then what comes out until this many frames.
    fn impulse(space: &mut Space, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| {
                let level = if at == 0 { 1.0 } else { 0.0 };
                space.process([level, level])
            })
            .collect()
    }

    /// How much comes out in each second.
    fn seconds(out: &[[f32; 2]], rate: u32) -> Vec<f32> {
        out.chunks(rate as usize)
            .map(|chunk| chunk.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum())
            .collect()
    }

    /// How many dB it falls each second, from its second second to its last.
    fn falls_per_second(out: &[[f32; 2]], rate: u32) -> f32 {
        let seconds = seconds(out, rate);
        let (first, last) = (seconds[1], seconds[seconds.len() - 1]);
        10.0 * (first / last).log10() / (seconds.len() - 2) as f32
    }

    #[test]
    fn an_impulse_gives_a_dense_tail_that_dies_away() {
        let out = impulse(&mut space(settings(0x40), RATE), RATE as usize * 20);
        assert!(out.iter().flatten().all(|s| s.is_finite()));
        let tail = &out[RATE as usize..RATE as usize * 2];
        let loud = tail.iter().fold(0.0_f32, |most, f| most.max(f[0].abs()));
        let busy = tail.iter().filter(|f| f[0].abs() > loud * 0.01).count();
        assert!(busy > tail.len() / 2, "{busy} of {}", tail.len());
        let seconds = seconds(&out, RATE);
        assert!(seconds[19] < seconds[1] * 1e-4, "{seconds:?}");
    }

    #[test]
    fn decay_makes_its_tail_longer_and_at_the_top_it_still_dies_away() {
        let fall = |decay| {
            falls_per_second(
                &impulse(&mut space(settings(decay), RATE), RATE as usize * 12),
                RATE,
            )
        };
        let (short, middle, long) = (fall(0x00), fall(0x80), fall(0xFF));
        assert!(short > middle * 1.5, "{short} {middle}");
        assert!(middle > long * 3.0, "{middle} {long}");
        assert!(long > 0.2 && long < 3.0, "{long} dB a second");
        // At the top it rings for minutes, but each second is quieter than the last.
        let top = impulse(&mut space(settings(0xFF), RATE), RATE as usize * 20);
        let seconds = seconds(&top, RATE);
        for pair in seconds[2..].windows(2) {
            assert!(pair[1] < pair[0], "{seconds:?}");
        }
    }

    #[test]
    fn its_decay_is_about_the_same_at_any_rate() {
        let fall = |rate| {
            falls_per_second(
                &impulse(&mut space(settings(0x60), rate), rate as usize * 8),
                rate,
            )
        };
        let (low, mid, high, top) = (fall(44_100), fall(48_000), fall(96_000), fall(192_000));
        // Its tank runs at 44.1 or 48 kHz, so it falls a little faster at 48 than at 44.1.
        assert!((low / mid - 1.0).abs() < 0.2, "{low} {mid}");
        assert!((high / mid - 1.0).abs() < 0.2, "{high} {mid}");
        assert!((top / mid - 1.0).abs() < 0.2, "{top} {mid}");
    }

    /// How much of a tone comes out while it plays and rings.
    fn energy(settings: Settings, hertz: f32) -> f32 {
        let mut space = space(settings, RATE);
        let mut total = 0.0;
        for at in 0..RATE as usize * 2 {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            let input = if at < RATE as usize / 4 { tone } else { 0.0 };
            let out = space.process([input, input]);
            total += out[0] * out[0] + out[1] * out[1];
        }
        total
    }

    #[test]
    fn damp_darkens_it() {
        let open = Settings {
            damp: 0,
            ..Settings::DEFAULT
        };
        let dark = Settings { damp: 0xFF, ..open };
        assert!(energy(dark, 6000.0) < energy(open, 6000.0) * 0.01);
        assert!(energy(dark, 60.0) > energy(open, 60.0) * 0.2);
    }

    #[test]
    fn size_makes_its_tank_bigger() {
        let first = |size| {
            let out = impulse(
                &mut space(
                    Settings {
                        size,
                        ..Settings::DEFAULT
                    },
                    RATE,
                ),
                RATE as usize,
            );
            out.iter().position(|f| f[0].abs() > 1e-4).unwrap()
        };
        let (small, big) = (first(0x00), first(0xFF));
        assert!(big > small * 5, "{small} {big}");
    }

    #[test]
    fn mod_wobbles_what_goes_in() {
        let out = |excursion| {
            let mut space = space(
                Settings {
                    excursion,
                    ..Settings::DEFAULT
                },
                RATE,
            );
            (0..RATE as usize)
                .map(|at| {
                    let tone = (at as f32 / RATE as f32 * 440.0 * std::f32::consts::TAU).sin();
                    space.process([tone * 0.1; 2])
                })
                .collect::<Vec<_>>()
        };
        assert_ne!(out(0x00), out(0xFF));
    }

    #[test]
    fn its_sides_differ_so_it_spreads() {
        let mut space = space(Settings::DEFAULT, RATE);
        let out: Vec<[f32; 2]> = (0..RATE as usize * 2)
            .map(|at| space.process([if at == 0 { 1.0 } else { 0.0 }, 0.0]))
            .collect();
        let tail = &out[RATE as usize / 2..];
        let (mut both, mut left, mut right) = (0.0_f64, 0.0_f64, 0.0_f64);
        for [l, r] in tail {
            both += f64::from(l * r);
            left += f64::from(l * l);
            right += f64::from(r * r);
        }
        let correlation = both / (left * right).sqrt();
        assert!(correlation.abs() < 0.3, "{correlation}");
        assert!((left / right).log10().abs() < 0.5, "{left} {right}");
    }

    #[test]
    fn it_repeats_after_clearing() {
        let mut space = space(Settings::DEFAULT, RATE);
        let first = impulse(&mut space, RATE as usize * 2);
        space.clear();
        space.follow(Settings::DEFAULT);
        space.settle();
        assert_eq!(impulse(&mut space, RATE as usize * 2), first);
    }

    /// The biggest jump between frames from a tone through it, after a second, with the settings
    /// switching between two every 64 frames, or held at the first.
    fn biggest_jump(from: Settings, to: Settings, moving: bool) -> f32 {
        let tone =
            |frame: usize| (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
        let mut space = space(from, RATE);
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
                space.follow(now);
            }
            let out = space.process([tone(frame); 2]);
            if late {
                most = most.max((out[0] - last[0]).abs().max((out[1] - last[1]).abs()));
            }
            last = out;
        }
        most
    }

    #[test]
    fn moving_a_setting_glides_without_a_step() {
        let plain = Settings::DEFAULT;
        let pairs = [
            ("Decay", settings(0x20), settings(0xFF)),
            (
                "Damp",
                Settings { damp: 0, ..plain },
                Settings {
                    damp: 0xFF,
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
        ];
        for (name, from, to) in pairs {
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
            for size in [0x00, 0xFF] {
                for damp in [0x00, 0xFF] {
                    for (rate, excursion) in [(44_100, 0x00), (96_000, 0x80), (192_000, 0xFF)] {
                        let mut space = space(
                            Settings {
                                decay,
                                size,
                                damp,
                                excursion,
                                ..Settings::DEFAULT
                            },
                            rate,
                        );
                        for frame in 0..rate as usize * 3 {
                            // Noise 12 dB over full scale, a square, DC and silence in turn.
                            let level = 4.0
                                * match frame / (rate as usize / 4) % 4 {
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
                            let out = space.process([level, -level * 0.5]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() < 24.0),
                                "{decay:x} {size:x} {damp:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_nan_in_its_tank_dies_away() {
        let mut space = space(settings(0xFF), RATE);
        impulse(&mut space, 1000);
        let line = space.sides[0].blocks[0][0];
        let at = line.base + (space.write.wrapping_add(5) & line.mask);
        space.buffer[at] = f32::NAN;
        let out = impulse(&mut space, RATE as usize * 4);
        assert!(out.iter().flatten().all(|s| s.is_finite()));
        // It's heard as a click, and dies away as anything in it does.
        let seconds = seconds(&out, RATE);
        assert!(seconds[3] < seconds[1], "{seconds:?}");
    }

    #[test]
    fn its_lines_hold_the_biggest_tank_at_any_rate() {
        let needed: f32 = LENGTHS.iter().flatten().sum::<f32>() * LARGEST * 2.0;
        for rate in [44_100, 48_000, 96_000, 192_000] {
            assert!(Space::new(rate).frames() as f32 >= needed, "{rate}");
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut space = space(Settings::DEFAULT, RATE);
        impulse(&mut space, 1000);
        space.clear();
        for _ in 0..RATE {
            assert_eq!(space.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
