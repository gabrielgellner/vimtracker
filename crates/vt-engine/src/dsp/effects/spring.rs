//! A stereo spring reverb: ChowDSP's model of a spring tank, as Surge XT ships it. Each side goes
//! into a loop through a soft clip, a DC blocker and sixteen nested all-passes, whose chirp is the
//! drip of a spring; what comes round the loop is delayed by the springs' length. What comes out
//! of the all-passes takes away four early reflections, mixed by a Householder matrix, and is
//! darkened by a low-pass. A knock adds a short bump to the tank, as a kick to a spring tank does.
//!
//! Surge runs the two sides and each side's copy for the loop as four lanes of one SIMD register
//! (`simdState`); here they're four plain lanes, `[left, left's loop, right, right's loop]`. Its
//! random numbers come from `rng.rs`, seeded afresh whenever it's cleared, so a render repeats.
//! Its four reflection lines are written with the same sample, so here each side has one line
//! read at four places. The 1000 frames its loop always has are at 48 kHz, scaled to the rate it
//! runs at, as every other length is, so it sounds the same at any rate. Its one shelf for both
//! sides' reflections is one for each here, and the reflections are never read closer than two
//! frames back, where Surge reads stale samples at the smallest size.
//!
//! Adapted from `src/common/dsp/effects/chowdsp/spring_reverb/` (`SpringReverbProc.cpp`,
//! `SchroederAllpass.h`, `ReflectionNetwork.h`) and `chowdsp/shared/` (the Thiran and third-order
//! Lagrange reads in `chowdsp_DelayInterpolation.h`, the state-variable filter, the shelf) at
//! <https://github.com/surge-synthesizer/surge>, and the clip from sst-basic-blocks' `fasttanh`.
//! The model is Jatin Chowdhury's, of ChowDSP.
//!
//! Surge XT - a free and open source hybrid synthesizer, built by Surge Synth Team.
//! Copyright 2018-2024, various authors, as described in the GitHub transaction log.
//!
//! Surge XT is released under the GNU General Public Licence v3 or later (GPL-3.0-or-later). The
//! license is found in the "LICENSE" file in the root of this repository, or at
//! <https://www.gnu.org/licenses/gpl-3.0.en.html>.

use std::f32::consts::PI;

use vt_model::insert::Spring as Settings;

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::smoother::{Smoothed, coefficient};
use crate::rng::Pcg32;

/// How often what's slow to work out is worked out again, in frames, as Surge's block is.
const BLOCK: u32 = 32;
/// The all-passes in each lane, each with one nested inside it.
const STAGES: usize = 16;
/// `[left, left's loop, right, right's loop]`, as Surge's `simdState`.
const LANES: usize = 4;
/// The rate the loop's fixed 1000 frames are at.
const TUNING_RATE: f32 = 48_000.0;
/// The loop's delay: 1000 frames at the tuning rate, and this many seconds more at full `Size`.
const LOOP_FRAMES: f32 = 1000.0;
const LOOP_SECONDS_PER_SIZE: f32 = 0.099;
/// How far `Chaos` moves the loop's delay at most, as a share of it.
const MOST_CHAOS: f32 = 0.07;
/// How long `Chaos` takes to glide to each new place, in seconds, as Surge's smoother does.
const CHAOS_GLIDE_SECONDS: f32 = 0.05;
/// Each all-pass's delay: this many ms, and this many more at full `Size`.
const ALLPASS_MS: f32 = 0.35;
const ALLPASS_MS_PER_SIZE: f32 = 3.0;
/// How long it rings, in seconds, at the bottom and top of `Decay` before `Size` corrects it.
const LOW_T60: f32 = 0.5;
const HIGH_T60: f32 = 4.5;
/// Where the reflections are, in seconds at full `Size`.
const REFLECTION_SECONDS: [f32; 4] = [0.07, 0.17, 0.23, 0.29];
/// The low-pass on what comes out, from open at the bottom of `Damp` to dark at the top, in Hz.
const DAMP_HZ: (f32, f32) = (18_000.0, 4000.0);
/// The DC blocker's cutoff, in Hz.
const DC_HZ: f32 = 40.0;
/// Where the reflections' shelf turns, in Hz, and how far it cuts above, in dB, at each end of
/// `Damp`.
const SHELF_HZ: f32 = 800.0;
const SHELF_DB: (f32, f32) = (-1.0, -10.0);
/// How long a knock is, in seconds, from the shortest to the longest, before `Size` stretches it.
const KNOCK_SECONDS: (f32, f32) = (0.0005, 0.001);
/// How far `Size` moves in a second at most: its whole range, so the springs' lengths glide and
/// what's in them bends in pitch rather than clicking.
const SIZE_PER_SECOND: f32 = 1.0;
/// What goes into the clip is held to where `fasttanh` is good, and what goes into the
/// reflections to well over what any sound makes, so a NaN in either is held too (#461).
const CLIP_EDGE: f32 = 5.0;
const HEADROOM: f32 = 8.0;
/// Where the knock's and the chaos's random numbers start, whenever it's cleared.
const SEED: u64 = 0x5350_5249_4E47;

/// sst-basic-blocks' `fasttanh`, a Padé approximation good from -5 to 5.
#[inline]
fn fast_tanh(x: f32) -> f32 {
    let x2 = x * x;
    let numerator = x * (135_135.0 + x2 * (17_325.0 + x2 * (378.0 + x2)));
    let denominator = 135_135.0 + x2 * (62_370.0 + x2 * (3150.0 + 28.0 * x2));
    numerator / denominator
}

/// A ring buffer of lines the same length, all written once a frame at the same place.
#[derive(Debug, Clone)]
struct Lines {
    buffer: Vec<f32>,
    /// Each line's length, a power of two, less one.
    mask: usize,
    /// Each line's length.
    length: usize,
}

impl Lines {
    fn new(count: usize, longest: f32) -> Self {
        // Room for the reads past the delay.
        let length = (longest.ceil() as usize + 4).next_power_of_two();
        Self {
            buffer: vec![0.0; count * length],
            mask: length - 1,
            length,
        }
    }

    /// What went into a line this many frames ago, as a whole number, one or more.
    #[inline]
    fn read(&self, line: usize, write: usize, frames: usize) -> f32 {
        self.buffer[line * self.length + (write.wrapping_add(frames) & self.mask)]
    }

    #[inline]
    fn write(&mut self, line: usize, write: usize, value: f32) {
        self.buffer[line * self.length + (write & self.mask)] = value;
    }

    /// What went in this many frames ago, read between samples by a third-order Lagrange curve,
    /// as Chow's `Lagrange3rd` does. It's two frames back or more.
    #[inline]
    fn lagrange(&self, line: usize, write: usize, frames: f32) -> f32 {
        let whole = frames as usize - 1;
        let d = frames - whole as f32;
        let [v1, v2, v3, v4] = std::array::from_fn(|at| self.read(line, write, whole + at));
        let (d1, d2, d3) = (d - 1.0, d - 2.0, d - 3.0);
        let c1 = -d1 * d2 * d3 / 6.0;
        let c2 = d2 * d3 * 0.5;
        let c3 = -d1 * d3 * 0.5;
        let c4 = d1 * d2 / 6.0;
        v1 * c1 + d * (v2 * c2 + v3 * c3 + v4 * c4)
    }
}

/// The sixteen nested all-passes of all four lanes, each two lines, the outer and the one nested
/// in it, with the four lanes side by side in each line so they're worked out together.
#[derive(Debug, Clone)]
struct Allpasses {
    buffer: Vec<[f32; LANES]>,
    mask: usize,
    length: usize,
    /// What each line's Thiran read gave last.
    held: [[[f32; LANES]; 2]; STAGES],
}

impl Allpasses {
    fn new(longest: f32) -> Self {
        let length = (longest.ceil() as usize + 4).next_power_of_two();
        Self {
            buffer: vec![[0.0; LANES]; STAGES * 2 * length],
            mask: length - 1,
            length,
            held: [[[0.0; LANES]; 2]; STAGES],
        }
    }

    fn clear(&mut self) {
        self.buffer.fill([0.0; LANES]);
        self.held = [[[0.0; LANES]; 2]; STAGES];
    }

    /// One frame through every all-pass, each delayed by `whole` frames and read between samples
    /// by Thiran's all-pass at `alpha`, spreading by `g`, a lane's own.
    #[inline]
    fn process(
        &mut self,
        mut x: [f32; LANES],
        write: usize,
        (whole, alpha): (usize, f32),
        g: [f32; LANES],
    ) -> [f32; LANES] {
        let (length, mask) = (self.length, self.mask);
        let at = |line: usize, back: usize| line * length + (write.wrapping_add(back) & mask);
        for (stage, held) in self.held.iter_mut().enumerate() {
            let (outer, inner) = (stage * 2, stage * 2 + 1);
            let [near, far] = [at(outer, whole), at(outer, whole + 1)];
            let (near, far) = (self.buffer[near], self.buffer[far]);
            let popped: [f32; LANES] =
                std::array::from_fn(|lane| far[lane] + alpha * (near[lane] - held[0][lane]));
            held[0] = popped;
            let [near, far] = [at(inner, whole), at(inner, whole + 1)];
            let (near, far) = (self.buffer[near], self.buffer[far]);
            let nested: [f32; LANES] =
                std::array::from_fn(|lane| far[lane] + alpha * (near[lane] - held[1][lane]));
            held[1] = nested;
            let nested_in: [f32; LANES] =
                std::array::from_fn(|lane| popped[lane] + g[lane] * nested[lane]);
            self.buffer[at(inner, 0)] = nested_in;
            let delayed: [f32; LANES] =
                std::array::from_fn(|lane| nested[lane] - g[lane] * nested_in[lane]);
            let into: [f32; LANES] = std::array::from_fn(|lane| x[lane] + g[lane] * delayed[lane]);
            self.buffer[at(outer, 0)] = into;
            x = std::array::from_fn(|lane| delayed[lane] - g[lane] * into[lane]);
        }
        x
    }
}

/// A Thiran read's whole part and its all-pass coefficient, for a delay, as Chow's `Thiran` works
/// them out: the fraction is kept from 0.618 to 1.618, where the all-pass is well behaved.
#[inline]
fn thiran(frames: f32) -> (usize, f32) {
    let mut whole = frames as usize;
    let mut fraction = frames - whole as f32;
    if fraction < 0.618 && whole >= 1 {
        fraction += 1.0;
        whole -= 1;
    }
    (whole, (1.0 - fraction) / (1.0 + fraction))
}

/// Chow's state-variable filter's coefficients, Zavalishin's TPT form at a Q of 1/√2.
#[derive(Debug, Clone, Copy)]
struct Tpt {
    g: f32,
    h: f32,
    g_plus_r2: f32,
}

impl Tpt {
    fn new(hertz: f32, rate: u32) -> Self {
        let g = (PI * hertz / rate as f32).tan();
        let r2 = std::f32::consts::SQRT_2;
        let h = 1.0 / (1.0 + r2 * g + g * g);
        Self {
            g,
            h,
            g_plus_r2: g + r2,
        }
    }
}

/// One side's state-variable filter.
#[derive(Debug, Clone, Copy, Default)]
struct Svf {
    s1: f32,
    s2: f32,
}

impl Svf {
    /// One step, giving the high-pass and the low-pass.
    #[inline]
    fn process(&mut self, x: f32, tpt: Tpt) -> (f32, f32) {
        let high = (x - self.s1 * tpt.g_plus_r2 - self.s2) * tpt.h;
        let y = high * tpt.g;
        self.s1 += y + y;
        let ga = (self.s1 - y) * tpt.g;
        self.s2 += ga + ga;
        (high, self.s2 - ga)
    }
}

/// A first-order shelf's coefficients, after Abel and Berners, as Chow's `ShelfFilter`.
#[derive(Debug, Clone, Copy)]
struct Shelf {
    b0: f32,
    b1: f32,
    a1: f32,
}

impl Shelf {
    fn new(low_gain: f32, high_gain: f32, hertz: f32, rate: u32) -> Self {
        let rho = (high_gain / low_gain).sqrt();
        let k = 1.0 / (PI * hertz / rate as f32).tan();
        let (bs0, bs1) = (high_gain / rho, low_gain);
        let (as0, as1) = (1.0 / rho, 1.0);
        let a0 = as0 * k + as1;
        Self {
            b0: (bs0 * k + bs1) / a0,
            b1: (-bs0 * k + bs1) / a0,
            a1: (-as0 * k + as1) / a0,
        }
    }
}

/// Where a knock is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Knock {
    /// `Knock` is off, or on and not yet heard.
    Ready,
    /// This many frames into one this long.
    Playing { at: u32, length: u32 },
    /// Heard, and waiting for `Knock` to go off before it can knock again.
    Done,
}

/// The stereo spring reverb.
#[derive(Debug, Clone)]
pub struct Spring {
    allpasses: Allpasses,
    /// Each side's loop, and the line its reflections are read from.
    loops: Lines,
    reflections: Lines,
    /// Where every line is written this frame. It goes down by one each frame.
    write: usize,
    /// What came out of each side last, which goes round the all-passes for the loop.
    last: [f32; 2],
    dc: [Svf; 2],
    damping: [Svf; 2],
    /// Each side's reflection shelf, the state of its one pole.
    shelved: [f32; 2],
    dc_tpt: Tpt,
    damping_tpt: Tpt,
    shelf: Shelf,
    /// How much of the loop comes round, and how much of each reflection.
    feedback: f32,
    reflection_gains: [f32; 4],
    /// How much the all-passes spread, Surge's `apfG`.
    spread: f32,
    decay: Smoothed,
    size: Smoothed,
    /// The size the springs are now, which follows `size` no faster than it may move.
    size_now: f32,
    damp: Smoothed,
    spin: Smoothed,
    reflect: Smoothed,
    chaos: Smoothed,
    /// How far the loop's delay is moved now, in frames, and how far it moves each frame.
    wander: f32,
    wander_step: f32,
    wander_left: u32,
    knocked: bool,
    knock: Knock,
    rng: Pcg32,
    /// Frames until what's slow to work out is worked out again.
    countdown: u32,
    smoothing: f32,
    rate: u32,
}

impl Spring {
    /// Make a spring for a sample rate, with lines long enough for the biggest.
    pub fn new(rate: u32) -> Self {
        let frames = |seconds: f32| seconds * rate as f32;
        let scale = rate as f32 / TUNING_RATE;
        let longest_loop =
            (LOOP_FRAMES * scale + frames(LOOP_SECONDS_PER_SIZE)) * (1.0 + MOST_CHAOS);
        let longest_allpass = frames((ALLPASS_MS + ALLPASS_MS_PER_SIZE) / 1000.0);
        let mut spring = Self {
            allpasses: Allpasses::new(longest_allpass),
            loops: Lines::new(2, longest_loop),
            reflections: Lines::new(2, frames(REFLECTION_SECONDS[3])),
            write: 0,
            last: [0.0; 2],
            dc: [Svf::default(); 2],
            damping: [Svf::default(); 2],
            shelved: [0.0; 2],
            dc_tpt: Tpt::new(DC_HZ, rate),
            damping_tpt: Tpt::new(DAMP_HZ.0, rate),
            shelf: Shelf::new(1.0, 1.0, SHELF_HZ, rate),
            feedback: 0.0,
            reflection_gains: [0.0; 4],
            spread: 0.0,
            decay: Smoothed::at(0.5),
            size: Smoothed::at(0.5),
            size_now: 0.5,
            damp: Smoothed::at(0.5),
            spin: Smoothed::at(0.5),
            reflect: Smoothed::at(1.0),
            chaos: Smoothed::at(0.0),
            wander: 0.0,
            wander_step: 0.0,
            wander_left: 0,
            knocked: false,
            knock: Knock::Ready,
            rng: Pcg32::new(SEED, 0),
            countdown: 0,
            smoothing: coefficient(rate),
            rate,
        };
        spring.follow(Settings::DEFAULT);
        spring.settle();
        spring
    }

    /// Forget what's in it, and start its random numbers again.
    pub fn clear(&mut self) {
        self.allpasses.clear();
        self.loops.buffer.fill(0.0);
        self.reflections.buffer.fill(0.0);
        self.last = [0.0; 2];
        self.dc = [Svf::default(); 2];
        self.damping = [Svf::default(); 2];
        self.shelved = [0.0; 2];
        self.wander = 0.0;
        self.wander_step = 0.0;
        self.wander_left = 0;
        self.knock = Knock::Ready;
        self.rng = Pcg32::new(SEED, 0);
        self.countdown = 0;
    }

    /// How many frames its lines hold, for the tests.
    #[cfg(test)]
    fn frames(&self) -> [usize; 3] {
        [
            self.allpasses.length,
            self.loops.length,
            self.reflections.length,
        ]
    }

    /// Take the settings.
    pub fn follow(&mut self, settings: Settings) {
        let share = |value: u8| f32::from(value) / 255.0;
        self.decay.target = share(settings.decay);
        self.size.target = share(settings.size);
        self.damp.target = share(settings.damp);
        self.spin.target = share(settings.spin);
        self.reflect.target = share(settings.reflect);
        self.chaos.target = share(settings.chaos);
        self.knocked = settings.knock != 0;
    }

    /// Jump to the settings, for starting afresh.
    pub fn settle(&mut self) {
        self.decay.settle();
        self.size.settle();
        self.size_now = self.size.value;
        self.damp.settle();
        self.spin.settle();
        self.reflect.settle();
        self.chaos.settle();
        self.countdown = 0;
    }

    /// Whether it's about to be knocked, so it has something to play with nothing going in.
    pub fn knocking(&self) -> bool {
        self.knocked && self.knock == Knock::Ready
    }

    /// The loop's delay without its wander, in frames, at a size.
    #[inline]
    fn loop_frames(&self, size: f32) -> f32 {
        let rate = self.rate as f32;
        LOOP_FRAMES * rate / TUNING_RATE + size * LOOP_SECONDS_PER_SIZE * rate
    }

    /// A random number from 0 to 1.
    fn random(&mut self) -> f32 {
        (f64::from(self.rng.next_u32()) / 4_294_967_296.0) as f32
    }

    /// Work out what's slow to, from the settings as they are now, as Surge's `setParams` does
    /// once a block.
    fn update(&mut self) {
        let rate = self.rate as f32;
        let size = self.size_now;
        if self.knocked && self.knock == Knock::Ready {
            let amount = self.random();
            let seconds = (KNOCK_SECONDS.0 + (KNOCK_SECONDS.1 - KNOCK_SECONDS.0) * amount)
                * (1.0 + 0.5 * size);
            let length = ((rate * seconds) as u32).max(1);
            self.knock = Knock::Playing { at: 0, length };
        } else if !self.knocked && self.knock == Knock::Done {
            self.knock = Knock::Ready;
        }

        let correction = 0.7 * (1.0 - size * size);
        let t60 = LOW_T60 * (HIGH_T60 / LOW_T60).powf(0.95 * self.decay.value - correction);
        // The wander glides to a new place each block, as Surge's linear smoother does.
        let frames = self.loop_frames(size);
        let target = self.random() * frames * MOST_CHAOS;
        let glide = (CHAOS_GLIDE_SECONDS * rate) as u32;
        self.wander_step = (target - self.wander) / glide as f32;
        self.wander_left = glide;
        let chaos = self.chaos.value;
        let loop_frames = frames + chaos * chaos * chaos * self.wander;
        let dies = |frames: f32| 0.001_f32.powf(frames / (t60 * rate));
        self.feedback = dies(loop_frames);

        let damp = self.damp.value;
        let hertz = DAMP_HZ.1 * (DAMP_HZ.0 / DAMP_HZ.1).powf(1.0 - damp);
        self.damping_tpt = Tpt::new(hertz.min(rate * 0.45), self.rate);
        let reflect = self.reflect.value;
        let skew = 0.95 * reflect * reflect * 0.23 * (0.735 + 0.235 * size);
        self.reflection_gains =
            REFLECTION_SECONDS.map(|seconds| dies(self.reflection_frames(seconds, size)) * skew);
        let db = SHELF_DB.0 + (SHELF_DB.1 - SHELF_DB.0) * damp;
        self.shelf = Shelf::new(1.0, 10_f32.powf(db / 20.0), SHELF_HZ, self.rate);
    }

    /// How far back a reflection is read, in frames, at a size: never closer than two frames, so
    /// the Lagrange read stays on what's been written.
    #[inline]
    fn reflection_frames(&self, seconds: f32, size: f32) -> f32 {
        (seconds * size * self.rate as f32).max(2.0)
    }

    /// The knock's next frame: a half sine, two high.
    #[inline]
    fn knock_next(&mut self) -> f32 {
        let Knock::Playing { at, length } = self.knock else {
            return 0.0;
        };
        let value = 2.0 * (PI * at as f32 / length as f32).sin();
        self.knock = if at + 1 >= length {
            Knock::Done
        } else {
            Knock::Playing { at: at + 1, length }
        };
        value
    }

    /// One side's reflections: its four, mixed by a Householder matrix, scaled to die away with
    /// the tank, and shelved.
    #[inline]
    fn reflected(&mut self, side: usize, size: f32) -> f32 {
        let taps: [f32; 4] = std::array::from_fn(|at| {
            let frames = self.reflection_frames(REFLECTION_SECONDS[at], size);
            self.reflections.lagrange(side, self.write, frames)
        });
        let sum = taps.iter().sum::<f32>() * (-2.0 / 4.0);
        let mixed = taps
            .iter()
            .zip(self.reflection_gains)
            .map(|(tap, gain)| (tap + sum) * gain)
            .sum::<f32>()
            / 4.0;
        // The shelf's pole is a loop of its own, so a NaN mustn't reach it.
        let mixed = within(mixed, -HEADROOM, HEADROOM);
        let shelf = self.shelf;
        let out = self.shelved[side] + mixed * shelf.b0;
        self.shelved[side] = mixed * shelf.b1 - out * shelf.a1;
        out
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let wanted = self.size.next(self.smoothing);
        let most = SIZE_PER_SECOND / self.rate as f32;
        self.size_now += (wanted - self.size_now).clamp(-most, most);
        self.decay.next(self.smoothing);
        self.damp.next(self.smoothing);
        self.reflect.next(self.smoothing);
        self.chaos.next(self.smoothing);
        // The all-passes' spread moves every frame, as a step in it clicks.
        self.spread = 0.5 - 0.4 * self.spin.next(self.smoothing);
        if self.countdown == 0 {
            self.update();
            self.countdown = BLOCK;
        }
        self.countdown -= 1;
        if self.wander_left > 0 {
            self.wander += self.wander_step;
            self.wander_left -= 1;
        }
        let size = self.size_now;
        let chaos = self.chaos.value;
        let loop_frames = self.loop_frames(size) + chaos * chaos * chaos * self.wander;
        let allpass_frames = (ALLPASS_MS + ALLPASS_MS_PER_SIZE * size) / 1000.0 * self.rate as f32;
        self.write = self.write.wrapping_sub(1);
        let knock = self.knock_next();

        let mut lanes = [0.0; LANES];
        for side in 0..2 {
            let back = self.loops.lagrange(side, self.write, loop_frames);
            let clipped = fast_tanh(within(
                input[side] - self.feedback * back,
                -CLIP_EDGE,
                CLIP_EDGE,
            ));
            let (high, _) = self.dc[side].process(clipped, self.dc_tpt);
            lanes[side * 2] = high + knock;
            lanes[side * 2 + 1] = self.last[side];
        }
        // The loop's lanes spread the other way, as Surge's `{g, -g, g, -g}`.
        let g = self.spread;
        let lanes =
            self.allpasses
                .process(lanes, self.write, thiran(allpass_frames), [g, -g, g, -g]);
        let mut out = [0.0; 2];
        for side in 0..2 {
            self.loops.write(side, self.write, lanes[side * 2 + 1]);
            let reflected = self.reflected(side, size);
            let sample = within(lanes[side * 2] - reflected, -HEADROOM, HEADROOM);
            self.reflections.write(side, self.write, sample);
            let (_, low) = self.damping[side].process(sample, self.damping_tpt);
            self.last[side] = low;
            out[side] = low;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn spring(settings: Settings, rate: u32) -> Spring {
        let mut spring = Spring::new(rate);
        spring.follow(settings);
        spring.settle();
        spring
    }

    fn settings(decay: u8) -> Settings {
        Settings {
            decay,
            ..Settings::DEFAULT
        }
    }

    /// A single frame at full scale, and then what comes out until this many frames.
    fn impulse(spring: &mut Spring, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|at| {
                let level = if at == 0 { 1.0 } else { 0.0 };
                spring.process([level, level])
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
    fn the_clip_is_tanh_where_it_is_used() {
        for at in -50..=50 {
            let x = at as f32 / 10.0;
            assert!((fast_tanh(x) - x.tanh()).abs() < 2e-4, "{x}");
        }
        // And it's held to its edge, a NaN to the bottom.
        assert!(fast_tanh(within(f32::NAN, -CLIP_EDGE, CLIP_EDGE)) < -0.99);
    }

    #[test]
    fn an_impulse_rings_and_dies_away() {
        let out = impulse(&mut spring(Settings::DEFAULT, RATE), RATE as usize * 8);
        assert!(out.iter().flatten().all(|s| s.is_finite()));
        let energies = energies(&out, RATE);
        let loudest = energies.iter().fold(0.0_f32, |most, e| most.max(*e));
        assert!(loudest > 1e-3, "{loudest}");
        // It's still ringing half a second in, and gone by the end.
        assert!(energies[10] > loudest * 1e-4, "{energies:?}");
        assert!(
            energies[energies.len() - 1] < loudest * 1e-6,
            "{energies:?}"
        );
    }

    #[test]
    fn decay_makes_its_tail_longer_and_it_always_dies_away() {
        let time = |decay| {
            decay_seconds(
                &impulse(&mut spring(settings(decay), RATE), RATE as usize * 12),
                RATE,
            )
        };
        let (short, middle, long) = (time(0x00), time(0x80), time(0xFF));
        assert!(short < 1.0, "{short}");
        assert!(middle > short * 1.5, "{short} {middle}");
        assert!(long > middle * 1.5, "{middle} {long}");
        assert!(long < 11.0, "{long}");
    }

    #[test]
    fn its_decay_is_about_the_same_at_any_rate() {
        let time = |rate| {
            decay_seconds(
                &impulse(&mut spring(settings(0x80), rate), rate as usize * 8),
                rate,
            )
        };
        let (low, mid, high) = (time(44_100), time(48_000), time(96_000));
        assert!((low / mid - 1.0).abs() < 0.2, "{low} {mid}");
        assert!((high / mid - 1.0).abs() < 0.2, "{high} {mid}");
    }

    /// When the first thing that's nearly as loud as the loudest comes back round the loop after
    /// the impulse, in frames.
    fn first_echo(settings: Settings) -> usize {
        let out = impulse(&mut spring(settings, RATE), RATE as usize / 2);
        let start = RATE as usize / 100;
        let loudest = out[start..]
            .iter()
            .fold(0.0_f32, |most, f| most.max(f[0].abs()));
        start
            + out[start..]
                .iter()
                .position(|f| f[0].abs() > loudest * 0.5)
                .unwrap()
    }

    #[test]
    fn size_lengthens_the_loop() {
        let small = first_echo(Settings {
            size: 0x00,
            ..Settings::DEFAULT
        });
        let big = first_echo(Settings {
            size: 0xFF,
            ..Settings::DEFAULT
        });
        assert!(big > small * 2, "{small} {big}");
    }

    #[test]
    fn spin_makes_its_chirp_shorter() {
        // How long the impulse takes to come through the all-passes, before the loop comes round:
        // from when a tenth of its energy is in to when nine tenths are.
        let spread = |spin| {
            let mut spring = spring(
                Settings {
                    spin,
                    reflect: 0,
                    ..Settings::DEFAULT
                },
                RATE,
            );
            let out = impulse(&mut spring, 900);
            let total: f32 = out.iter().map(|f| f[0] * f[0]).sum();
            let by = |share: f32| {
                let mut so_far = 0.0;
                out.iter()
                    .position(|f| {
                        so_far += f[0] * f[0];
                        so_far > total * share
                    })
                    .unwrap()
            };
            by(0.9) - by(0.1)
        };
        let (smeared, tight) = (spread(0x00), spread(0xFF));
        assert!(tight < smeared, "{smeared} {tight}");
    }

    /// How much of a tone comes out once it's stopped and the tank rings.
    fn energy(settings: Settings, hertz: f32) -> f32 {
        let mut spring = spring(settings, RATE);
        let mut total = 0.0;
        for at in 0..RATE as usize * 2 {
            let tone = (at as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * 0.1;
            let input = if at < RATE as usize / 4 { tone } else { 0.0 };
            let out = spring.process([input, input]);
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
        assert!(energy(dark, 9000.0) < energy(open, 9000.0) * 0.1);
        assert!(energy(dark, 300.0) > energy(open, 300.0) * 0.3);
    }

    #[test]
    fn reflections_add_to_the_tail() {
        let none = Settings {
            reflect: 0,
            ..Settings::DEFAULT
        };
        let out = |settings| impulse(&mut spring(settings, RATE), RATE as usize);
        assert_ne!(out(none), out(Settings::DEFAULT));
    }

    #[test]
    fn a_knock_rings_the_tank_once_until_it_goes_off_again() {
        let mut spring = spring(Settings::DEFAULT, RATE);
        let quiet = |spring: &mut Spring, frames: usize| {
            (0..frames)
                .map(|_| spring.process([0.0, 0.0]))
                .fold(0.0_f32, |most, f| most.max(f[0].abs()))
        };
        assert_eq!(quiet(&mut spring, 1000), 0.0);
        let knocked = Settings {
            knock: 1,
            ..Settings::DEFAULT
        };
        spring.follow(knocked);
        assert!(spring.knocking());
        let loud = quiet(&mut spring, RATE as usize / 10);
        assert!(loud > 0.05, "{loud}");
        assert!(!spring.knocking());
        // Held on, it doesn't knock again: it dies away.
        quiet(&mut spring, RATE as usize * 6);
        assert!(quiet(&mut spring, RATE as usize / 10) < 1e-4);
        // Off and on again, it does.
        spring.follow(Settings::DEFAULT);
        quiet(&mut spring, 100);
        spring.follow(knocked);
        assert!(quiet(&mut spring, RATE as usize / 10) > 0.05);
    }

    #[test]
    fn it_repeats_after_clearing_even_with_chaos_and_knocks() {
        let settings = Settings {
            chaos: 0xFF,
            knock: 1,
            ..Settings::DEFAULT
        };
        let mut spring = spring(settings, RATE);
        let first = impulse(&mut spring, RATE as usize);
        spring.clear();
        spring.follow(settings);
        spring.settle();
        assert_eq!(impulse(&mut spring, RATE as usize), first);
    }

    #[test]
    fn chaos_makes_the_loop_wander() {
        let out = |chaos| {
            impulse(
                &mut spring(
                    Settings {
                        chaos,
                        ..Settings::DEFAULT
                    },
                    RATE,
                ),
                RATE as usize,
            )
        };
        assert_ne!(out(0x00), out(0xFF));
    }

    #[test]
    fn its_sides_ring_apart_from_a_sound_on_one() {
        let mut spring = spring(Settings::DEFAULT, RATE);
        let out: Vec<[f32; 2]> = (0..RATE as usize)
            .map(|at| spring.process([if at == 0 { 1.0 } else { 0.0 }, 0.0]))
            .collect();
        assert!(out.iter().any(|f| f[0].abs() > 1e-3));
        assert!(out.iter().all(|f| f[1] == 0.0));
    }

    /// The biggest jump between frames from a tone through it, after a second, with the settings
    /// switching between two every 64 frames, as a knob or an LFO steps them block by block, or
    /// held at the first.
    fn biggest_jump(from: Settings, to: Settings, moving: bool) -> f32 {
        let tone =
            |frame: usize| (frame as f32 / RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
        let mut spring = spring(from, RATE);
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
                spring.follow(now);
            }
            let out = spring.process([tone(frame); 2]);
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
            ("Decay", settings(0x40), settings(0xFF)),
            (
                "Size",
                Settings { size: 0, ..plain },
                Settings {
                    size: 0xFF,
                    ..plain
                },
            ),
            (
                "Damp",
                Settings { damp: 0, ..plain },
                Settings {
                    damp: 0xFF,
                    ..plain
                },
            ),
            (
                "Spin",
                Settings { spin: 0, ..plain },
                Settings {
                    spin: 0xFF,
                    ..plain
                },
            ),
            (
                "Reflect",
                Settings {
                    reflect: 0,
                    ..plain
                },
                plain,
            ),
            (
                "Chaos",
                plain,
                Settings {
                    chaos: 0xFF,
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
        // `cargo test --release` plays all 8 corners of `Decay`, `Size` and `Spin` at each rate.
        // A test build, with its overflow checks, takes the 4 of them that still hold every pair
        // of ends of any two of the three, in half the time.
        let all = !cfg!(debug_assertions);
        for decay in [0x00, 0xFF] {
            for size in [0x00, 0xFF] {
                for spin in [0x00, 0xFF] {
                    if !all && spin != decay ^ size {
                        continue;
                    }
                    for (rate, chaos, damp, reflect) in
                        [(44_100, 0x00, 0x00, 0xFF), (192_000, 0xFF, 0xFF, 0x00)]
                    {
                        let mut spring = spring(
                            Settings {
                                decay,
                                size,
                                spin,
                                chaos,
                                damp,
                                reflect,
                                knock: 1,
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
                            let out = spring.process([level, -level * 0.5]);
                            assert!(
                                out.iter().all(|s| s.is_finite() && s.abs() < 8.0),
                                "{decay:x} {size:x} {spin:x} {rate}: {out:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_nan_in_its_lines_dies_away() {
        let mut spring = spring(settings(0xFF), RATE);
        impulse(&mut spring, 1000);
        let at = spring.write & spring.loops.mask;
        spring.loops.buffer[at] = f32::NAN;
        let at = spring.write.wrapping_add(10) & spring.reflections.mask;
        spring.reflections.buffer[at] = f32::NAN;
        let out = impulse(&mut spring, RATE as usize * 12);
        // A NaN read between samples can come out for a few frames, then it's gone.
        assert!(out[RATE as usize..].iter().flatten().all(|s| s.is_finite()));
        let last = &out[out.len() - 1000..];
        assert!(
            last.iter().flatten().all(|s| s.abs() < 1e-3),
            "{:?}",
            last[0]
        );
    }

    #[test]
    fn its_lines_hold_the_biggest_spring_at_any_rate() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let [allpass, loops, reflections] = Spring::new(rate).frames();
            let rate = rate as f32;
            assert!(allpass as f32 > (ALLPASS_MS + ALLPASS_MS_PER_SIZE) / 1000.0 * rate + 2.0);
            let longest_loop = (LOOP_FRAMES * rate / TUNING_RATE + LOOP_SECONDS_PER_SIZE * rate)
                * (1.0 + MOST_CHAOS);
            assert!(loops as f32 > longest_loop + 3.0);
            assert!(reflections as f32 > REFLECTION_SECONDS[3] * rate + 3.0);
        }
    }

    #[test]
    fn clearing_forgets_what_is_in_it() {
        let mut spring = spring(Settings::DEFAULT, RATE);
        impulse(&mut spring, 1000);
        spring.clear();
        for _ in 0..RATE {
            assert_eq!(spring.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }
}
