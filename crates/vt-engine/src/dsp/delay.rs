//! A stereo delay at a tempo-synced time. Its buffer is made ahead of time, for the slowest tempo.

const FEEDBACK: f32 = 0.4;
const WET: f32 = 0.6;

/// The delay line.
#[derive(Debug, Clone)]
pub struct Delay {
    buffer: Vec<[f32; 2]>,
    write: usize,
}

impl Delay {
    /// Make a delay that can hold this many frames.
    pub fn new(frames: usize) -> Self {
        Self {
            buffer: vec![[0.0; 2]; frames.max(1)],
            write: 0,
        }
    }

    /// Forget what's in it.
    pub fn clear(&mut self) {
        self.buffer.fill([0.0; 2]);
    }

    /// Feed one frame in and get one out, delayed by `frames`.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], frames: usize) -> [f32; 2] {
        let length = self.buffer.len();
        let delay = frames.clamp(1, length);
        let read = (self.write + length - delay) % length;
        let out = self.buffer[read];
        self.buffer[self.write] = [input[0] + out[1] * FEEDBACK, input[1] + out[0] * FEEDBACK];
        self.write = (self.write + 1) % length;
        [out[0] * WET, out[1] * WET]
    }
}
