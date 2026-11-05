//! The final stage: master gain and a soft clip.

use vt_model::constants::MASTER_GAIN;

/// Scale a summed stereo frame and keep it in range without a hard edge.
#[inline]
pub fn master(frame: [f32; 2]) -> [f32; 2] {
    [
        (frame[0] * MASTER_GAIN).tanh(),
        (frame[1] * MASTER_GAIN).tanh(),
    ]
}

/// What a level meter needs to know about some frames of the master.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    /// The highest level going into the soft clip, after the master gain.
    pub before: [f32; 2],
    /// The highest level coming out of it.
    pub after: [f32; 2],
    /// The output's samples squared and added up, for an average over [`Self::frames`].
    pub power: [f32; 2],
    pub frames: u32,
}

impl Reading {
    /// Count one frame, given the mix going into [`master`] and what came out.
    #[inline]
    pub fn add(&mut self, mixed: [f32; 2], out: [f32; 2]) {
        for channel in 0..2 {
            self.before[channel] = self.before[channel].max((mixed[channel] * MASTER_GAIN).abs());
            self.after[channel] = self.after[channel].max(out[channel].abs());
            self.power[channel] += out[channel] * out[channel];
        }
        self.frames += 1;
    }

    /// Add another reading, keeping the highest peaks of the two.
    pub fn merge(&mut self, other: Self) {
        for channel in 0..2 {
            self.before[channel] = self.before[channel].max(other.before[channel]);
            self.after[channel] = self.after[channel].max(other.after[channel]);
            self.power[channel] += other.power[channel];
        }
        self.frames += other.frames;
    }
}
