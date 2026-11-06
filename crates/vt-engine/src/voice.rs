//! Sample playback and its envelope.

use vt_model::constants::DECLICK_FRAMES;
use vt_model::envelope::Envelope;
use vt_model::sample::SampleData;
use vt_model::samples::SampleId;

use crate::dsp::smoother::fast_coefficient;

/// How a voice's level is changing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    /// Following its envelope's attack, decay and sustain.
    Held {
        from: f32,
    },
    /// Following its envelope's release, from the level it had.
    Released {
        from: f32,
    },
    /// A short fade, to avoid a click.
    Fading {
        per_frame: f32,
    },
}

/// One playing sample. It keeps its sample ID until it ends, so replacing an instrument's sample
/// only affects new notes.
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    stage: Stage,
    sample: SampleId,
    position: f64,
    increment: f64,
    /// What modulation multiplies the speed by, and where that's heading.
    pitch: f64,
    pitch_target: f64,
    pitch_slew: f64,
    gain: f32,
    level: f32,
    shape: Envelope,
    rate: u32,
    /// Frames since the stage began.
    elapsed: u64,
    /// Frames until a timed release starts, for previews.
    release_after: Option<u32>,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            stage: Stage::Idle,
            sample: 0,
            position: 0.0,
            increment: 1.0,
            pitch: 1.0,
            pitch_target: 1.0,
            pitch_slew: 1.0,
            gain: 0.0,
            level: 0.0,
            shape: Envelope::note_shape(),
            rate: 48_000,
            elapsed: 0,
            release_after: None,
        }
    }
}

/// What a voice needs to start.
#[derive(Debug, Clone, Copy)]
pub struct NoteOn {
    pub sample: SampleId,
    /// MIDI note. `C-4` plays at original pitch.
    pub note: u8,
    pub gain: f32,
    /// The shape of its level.
    pub shape: Envelope,
    pub release_after: Option<u32>,
}

impl Voice {
    /// Start a note, from a level if the last note's should carry on.
    pub fn start(note: NoteOn, rate: u32, from: f32, pitch: f64) -> Self {
        Self {
            stage: Stage::Held { from },
            sample: note.sample,
            position: 0.0,
            increment: 2_f64.powf((f64::from(note.note) - 60.0) / 12.0),
            pitch,
            pitch_target: pitch,
            pitch_slew: f64::from(fast_coefficient(rate)),
            gain: note.gain,
            level: from,
            shape: note.shape,
            rate,
            elapsed: 0,
            release_after: note.release_after,
        }
    }

    /// Whether it is sounding.
    pub fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// The level its envelope is at now.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Where modulation wants its speed.
    pub fn set_pitch(&mut self, pitch: f64) {
        self.pitch_target = pitch;
    }

    /// Start the envelope's release.
    pub fn release(&mut self) {
        if matches!(self.stage, Stage::Held { .. }) {
            self.stage = Stage::Released { from: self.level };
            self.elapsed = 0;
        }
    }

    /// Fade out quickly, to avoid a click.
    pub fn fade(&mut self) {
        if self.active() {
            self.stage = Stage::Fading {
                per_frame: 1.0 / DECLICK_FRAMES as f32,
            };
        }
    }

    /// The sample this voice is playing.
    pub fn sample(&self) -> SampleId {
        self.sample
    }

    /// The next stereo frame, with reads bounds-checked. A voice past its sample's end stops.
    #[inline]
    pub fn next(&mut self, data: Option<&SampleData>) -> [f32; 2] {
        if self.stage == Stage::Idle {
            return [0.0, 0.0];
        }
        let Some(data) = data else {
            self.stage = Stage::Idle;
            return [0.0, 0.0];
        };
        let index = self.position as usize;
        let Some(first) = data.frames.get(index) else {
            self.stage = Stage::Idle;
            return [0.0, 0.0];
        };
        let second = data.frames.get(index + 1).unwrap_or(first);
        let fraction = (self.position - index as f64) as f32;
        self.level = match self.stage {
            Stage::Held { from } => self.shape.held(self.elapsed, from, self.rate),
            Stage::Released { from } => self.shape.released(self.elapsed, from, self.rate),
            Stage::Fading { per_frame } => self.level - per_frame,
            Stage::Idle => 0.0,
        }
        .max(0.0);
        let level = self.level * self.gain;
        let frame = [
            (first[0] + (second[0] - first[0]) * fraction) * level,
            (first[1] + (second[1] - first[1]) * fraction) * level,
        ];
        self.pitch += (self.pitch_target - self.pitch) * self.pitch_slew;
        self.position += self.increment * self.pitch;
        if let Some(frames) = &mut self.release_after {
            if *frames == 0 {
                self.release_after = None;
                self.release();
            } else {
                *frames -= 1;
            }
        }
        let finished = match self.stage {
            Stage::Held { .. } => self.shape.finished_holding(self.elapsed, self.rate),
            Stage::Released { .. } => self.shape.finished_releasing(self.elapsed, self.rate),
            Stage::Fading { .. } => self.level <= 0.0,
            Stage::Idle => false,
        };
        if finished {
            self.stage = Stage::Idle;
        }
        self.elapsed += 1;
        frame
    }
}
