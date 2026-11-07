//! A Freeverb-style reverb with fixed settings, tuned by ear later.

const COMB_TUNINGS: [usize; 4] = [1116, 1188, 1277, 1356];
const ALLPASS_TUNINGS: [usize; 2] = [556, 441];
const STEREO_SPREAD: usize = 23;
const TUNING_RATE: f32 = 44_100.0;
const FEEDBACK: f32 = 0.84;
const DAMP: f32 = 0.25;
const ALLPASS_FEEDBACK: f32 = 0.5;
const INPUT_GAIN: f32 = 0.015;
const WET: f32 = 3.0;

#[derive(Debug, Clone)]
struct Comb {
    buffer: Vec<f32>,
    index: usize,
    store: f32,
}

impl Comb {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - DAMP) + self.store * DAMP;
        self.buffer[self.index] = input + self.store * FEEDBACK;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

#[derive(Debug, Clone)]
struct Allpass {
    buffer: Vec<f32>,
    index: usize,
}

impl Allpass {
    fn new(length: usize) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let stored = self.buffer[self.index];
        self.buffer[self.index] = input + stored * ALLPASS_FEEDBACK;
        self.index = (self.index + 1) % self.buffer.len();
        stored - input
    }
}

#[derive(Debug, Clone)]
struct Channel {
    combs: [Comb; 4],
    allpasses: [Allpass; 2],
}

impl Channel {
    fn new(rate: u32, spread: usize) -> Self {
        let scale = |tuning: usize| ((tuning + spread) as f32 * rate as f32 / TUNING_RATE) as usize;
        Self {
            combs: COMB_TUNINGS.map(|t| Comb::new(scale(t))),
            allpasses: ALLPASS_TUNINGS.map(|t| Allpass::new(scale(t))),
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for comb in &mut self.combs {
            out += comb.process(input);
        }
        for allpass in &mut self.allpasses {
            out = allpass.process(out);
        }
        out
    }
}

/// The stereo reverb.
#[derive(Debug, Clone)]
pub struct Reverb {
    channels: [Channel; 2],
}

impl Reverb {
    /// Make a reverb for a sample rate.
    pub fn new(rate: u32) -> Self {
        Self {
            channels: [Channel::new(rate, 0), Channel::new(rate, STEREO_SPREAD)],
        }
    }

    /// Forget what's in it.
    pub fn clear(&mut self) {
        for channel in &mut self.channels {
            for comb in &mut channel.combs {
                comb.buffer.fill(0.0);
                comb.store = 0.0;
            }
            for allpass in &mut channel.allpasses {
                allpass.buffer.fill(0.0);
            }
        }
    }

    /// Feed one frame in and get the wet signal out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mono = (input[0] + input[1]) * INPUT_GAIN;
        [
            self.channels[0].process(mono) * WET,
            self.channels[1].process(mono) * WET,
        ]
    }
}
