//! Delay lines, and the cubic reads between their samples.
//!
//! - [`Lines`]: lines a power of two long, one a side, written a frame at a time and read back
//!   from the newest, as the flanger, tape, pitch shifter, chorus, frequency shifter, comb and
//!   rotary have them. Each keeps its own read.
//! - [`Tape`]: Delay's and Echo's frame line, any length, wrapped round by a subtraction.
//! - [`SharedLines`]: one buffer that many short lines share, each at its own place, all moving
//!   together, as Clouds' reverb has it; the reverb, the plate and Beads' reverb use it.
//!
//! There are two forms of the four-point Hermite cubic. They're the same curve, but their
//! operations come in a different order, so their roundings differ: [`hermite`], as the
//! effects written here have it, and [`hermite_soras`], Laurent de Soras's, as Mutable
//! Instruments' code and Vital have it. Each user keeps the form it was written with, so its
//! sound stays the same to the bit.

/// Between `current` and `older`, `between` of the way, by a cubic through the four frames around
/// it, after Hermite, so a whole number of frames reads exactly.
#[inline]
pub(crate) fn hermite(newer: f32, current: f32, older: f32, oldest: f32, between: f32) -> f32 {
    let slope_in = (older - newer) * 0.5;
    let slope_out = (oldest - current) * 0.5;
    let difference = current - older;
    let bend = difference + slope_in;
    let lift = bend + difference + slope_out;
    ((lift * between - bend - lift) * between + slope_in) * between + current
}

/// The same cubic as [`hermite`], between `x0` and `x1`, worked out in the order of Laurent de
/// Soras's interpolator.
#[inline]
pub(crate) fn hermite_soras(xm1: f32, x0: f32, x1: f32, x2: f32, fraction: f32) -> f32 {
    let c = (x1 - xm1) * 0.5;
    let v = x0 - x1;
    let w = c + v;
    let a = w + v + (x2 - x0) * 0.5;
    let b_neg = w + a;
    (((a * fraction) - b_neg) * fraction + c) * fraction + x0
}

/// `SIDES` delay lines a power of two long, written together a frame at a time.
#[derive(Debug, Clone)]
pub(crate) struct Lines<const SIDES: usize> {
    lines: [Vec<f32>; SIDES],
    mask: usize,
    /// Where the newest frame is.
    write: usize,
}

/// A line for each side.
pub(crate) type StereoLine = Lines<2>;

impl<const SIDES: usize> Lines<SIDES> {
    /// Lines of at least this many frames, made a power of two long.
    pub(crate) fn new(frames: usize) -> Self {
        let size = frames.next_power_of_two();
        Self {
            lines: std::array::from_fn(|_| vec![0.0; size]),
            mask: size - 1,
            write: 0,
        }
    }

    /// How many frames each holds.
    #[inline]
    pub(crate) fn frames(&self) -> usize {
        self.mask + 1
    }

    /// The furthest back there is, the length less one.
    #[inline]
    pub(crate) fn mask(&self) -> usize {
        self.mask
    }

    pub(crate) fn clear(&mut self) {
        for line in &mut self.lines {
            line.fill(0.0);
        }
    }

    /// Move on a frame, to where the next one is written.
    #[inline]
    pub(crate) fn advance(&mut self) {
        self.write = (self.write + 1) & self.mask;
    }

    /// Write a side's newest frame.
    #[inline]
    pub(crate) fn write(&mut self, side: usize, sample: f32) {
        self.lines[side][self.write] = sample;
    }

    /// What a side holds `back` frames before its newest.
    #[inline]
    pub(crate) fn at(&self, side: usize, back: usize) -> f32 {
        self.lines[side][self.write.wrapping_sub(back) & self.mask]
    }

    /// A side `whole` frames back and `between` of a frame further, by [`hermite`].
    #[inline]
    pub(crate) fn hermite(&self, side: usize, whole: usize, between: f32) -> f32 {
        let line = &self.lines[side];
        let at = |back: usize| line[self.write.wrapping_sub(back) & self.mask];
        hermite(
            at(whole - 1),
            at(whole),
            at(whole + 1),
            at(whole + 2),
            between,
        )
    }
}

/// A line of frames, any length, written a frame at a time, as Delay's buffer and Echo's tape are.
/// It isn't a power of two long, as that would nearly double the memory of the longest time and
/// the clearing of it.
#[derive(Debug, Clone)]
pub(crate) struct Tape {
    frames: Vec<[f32; 2]>,
    /// Where the next frame goes.
    write: usize,
}

impl Tape {
    /// The closest it reads to where it's writing, leaving room to read between samples.
    pub(crate) const SHORTEST_FRAMES: f64 = 4.0;

    /// A tape this many frames long. Its memory is touched here, so the callback doesn't have to
    /// wait for it to be paged in.
    pub(crate) fn new(length: usize) -> Self {
        let mut frames = vec![[0.0; 2]; length];
        frames.fill([0.0; 2]);
        std::hint::black_box(&mut frames);
        Self { frames, write: 0 }
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn clear(&mut self) {
        self.frames.fill([0.0; 2]);
    }

    /// Write the next frame.
    #[inline]
    pub(crate) fn push(&mut self, frame: [f32; 2]) {
        self.frames[self.write] = frame;
        self.write = if self.write + 1 == self.frames.len() {
            0
        } else {
            self.write + 1
        };
    }

    /// Where is this many frames back, less than its length. Wrapping round by taking the length
    /// away once is cheaper than `%`.
    #[inline]
    fn back(&self, frames: usize) -> usize {
        let length = self.frames.len();
        let at = self.write + length - frames;
        if at >= length { at - length } else { at }
    }

    /// One side this far back, read between samples through four of them by [`hermite_soras`],
    /// so a whole number of frames gives exactly what went in.
    #[inline]
    pub(crate) fn read(&self, side: usize, frames: f64) -> f32 {
        let frames = frames.clamp(Self::SHORTEST_FRAMES, (self.frames.len() - 3) as f64);
        let whole = frames as usize;
        let between = (frames - whole as f64) as f32;
        let at = |back: usize| self.frames[self.back(back)][side];
        let current = at(whole);
        if between == 0.0 {
            return current;
        }
        hermite_soras(
            at(whole - 1),
            current,
            at(whole + 1),
            at(whole + 2),
            between,
        )
    }

    /// The frame written this many frames ago, for a test to spoil.
    #[cfg(test)]
    pub(crate) fn back_mut(&mut self, frames: usize) -> &mut [f32; 2] {
        let at = self.back(frames);
        &mut self.frames[at]
    }
}

/// One buffer a power of two long that many short lines share, each starting at its own `base`,
/// all moving on together a frame at a time: Clouds' `FxEngine`.
#[derive(Debug, Clone)]
pub(crate) struct SharedLines {
    buffer: Vec<f32>,
    mask: usize,
    write: usize,
}

impl SharedLines {
    /// A buffer of at least this many frames, made a power of two long.
    pub(crate) fn new(frames: usize) -> Self {
        let size = frames.next_power_of_two();
        Self {
            buffer: vec![0.0; size],
            mask: size - 1,
            write: 0,
        }
    }

    /// How many frames it holds, for the tests.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.buffer.len()
    }

    pub(crate) fn clear(&mut self) {
        self.buffer.fill(0.0);
    }

    /// Move every line on a frame.
    #[inline]
    pub(crate) fn advance(&mut self) {
        self.write = self.write.wrapping_sub(1) & self.mask;
    }

    /// Where the buffer has turned to, which a block's work can be counted by.
    #[inline]
    pub(crate) fn turned(&self) -> usize {
        self.write
    }

    /// What the line at `base` holds `offset` frames in.
    #[inline]
    pub(crate) fn read(&self, base: usize, offset: usize) -> f32 {
        self.buffer[(self.write + base + offset) & self.mask]
    }

    #[inline]
    pub(crate) fn write(&mut self, base: usize, offset: usize, value: f32) {
        let at = (self.write + base + offset) & self.mask;
        self.buffer[at] = value;
    }

    /// The line at `base` read between samples, in a straight line.
    #[inline]
    pub(crate) fn between(&self, base: usize, offset: f32) -> f32 {
        let whole = offset as usize;
        let fraction = offset - whole as f32;
        let a = self.read(base, whole);
        let b = self.read(base, whole + 1);
        a + (b - a) * fraction
    }

    /// A step through an all-pass `length` long at `base`, going by what it reads from the end
    /// of its line, with what it writes passed through `keep`.
    #[inline]
    pub(crate) fn allpass_keeping(
        &mut self,
        base: usize,
        length: usize,
        input: f32,
        into: f32,
        keep: impl Fn(f32) -> f32,
    ) -> f32 {
        let read = self.read(base, length - 1);
        let sum = input + read * into;
        self.write(base, 0, keep(sum));
        sum * -into + read
    }

    /// The same, writing what it works out as it is.
    #[inline]
    pub(crate) fn allpass(&mut self, base: usize, length: usize, input: f32, into: f32) -> f32 {
        self.allpass_keeping(base, length, input, into, |sum| sum)
    }
}
