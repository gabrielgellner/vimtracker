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
