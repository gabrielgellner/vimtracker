//! The final stage: the limiter.
//!
//! The limiter is a safety and not part of the sound. It does nothing up to full level and clips
//! hard above it, so the fix for hearing it is to turn the mix down.

/// A sample, or silence if it is a broken value, so that can't reach a device.
#[inline]
fn finite(sample: f32) -> f32 {
    if sample.is_finite() { sample } else { 0.0 }
}

/// Finish a summed stereo frame, keeping it in range if the limiter is on.
#[inline]
pub fn master(frame: [f32; 2], limiter: bool) -> [f32; 2] {
    frame.map(|sample| {
        let sample = finite(sample);
        if limiter {
            sample.clamp(-1.0, 1.0)
        } else {
            sample
        }
    })
}

/// What a level meter needs to know about some frames of the master.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    /// The highest level going into the limiter.
    pub before: [f32; 2],
    /// The highest level coming out of it.
    pub after: [f32; 2],
    /// The samples going in, squared and added up, for an average over [`Self::frames`].
    pub power: [f32; 2],
    pub frames: u32,
}

impl Reading {
    /// Count one frame, given the mix going into [`master`] and what came out.
    #[inline]
    pub fn add(&mut self, mixed: [f32; 2], out: [f32; 2]) {
        for channel in 0..2 {
            let before = finite(mixed[channel]);
            self.before[channel] = self.before[channel].max(before.abs());
            self.after[channel] = self.after[channel].max(out[channel].abs());
            self.power[channel] += before * before;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn through(level: f32) -> f32 {
        master([level; 2], true)[0]
    }

    #[test]
    fn up_to_full_level_nothing_changes_at_all() {
        for level in [0.0, 0.001, 0.25, 0.5, 0.75, 0.999, 1.0, -0.25, -0.999, -1.0] {
            assert_eq!(through(level), level);
        }
    }

    #[test]
    fn above_full_level_it_clips_hard() {
        for level in [1.001, 1.5, 4.0, 1000.0] {
            assert_eq!(through(level), 1.0);
            assert_eq!(through(-level), -1.0);
        }
    }

    #[test]
    fn with_the_limiter_off_nothing_changes() {
        let loud = 8.0;
        assert_eq!(master([loud, -loud], false), [loud, -loud]);
    }

    #[test]
    fn a_broken_value_becomes_silence_either_way() {
        for broken in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(master([broken, 0.5], true), [0.0, 0.5]);
            assert_eq!(master([broken, 0.5], false), [0.0, 0.5]);
        }
    }

    #[test]
    fn a_broken_value_cannot_stick_on_the_meter() {
        let mut reading = Reading::default();
        let mixed = [f32::NAN, f32::INFINITY];
        reading.add(mixed, master(mixed, true));
        assert_eq!(reading.before, [0.0, 0.0]);
        assert_eq!(reading.power, [0.0, 0.0]);
    }

    #[test]
    fn a_reading_measures_before_and_after_the_limiter() {
        let mut reading = Reading::default();
        let mixed = [2.0, 0.25];
        reading.add(mixed, master(mixed, true));
        assert_eq!(reading.before, [2.0, 0.25]);
        assert_eq!(reading.after, [1.0, 0.25]);
        assert_eq!(reading.power, [4.0, 0.0625]);
        assert_eq!(reading.frames, 1);
    }
}
