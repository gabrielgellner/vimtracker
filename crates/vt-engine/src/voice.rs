//! Sample playback and its envelope.

use vt_model::constants::DECLICK_FRAMES;
use vt_model::sample::SampleData;
use vt_model::samples::SampleId;

/// How a voice's level is changing.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    Idle,
    Attack,
    Hold,
    Release { per_frame: f32 },
}

/// One playing sample. It keeps its sample ID until it ends, so replacing an instrument's sample
/// only affects new notes.
#[derive(Debug, Clone, Copy)]
pub struct Voice {
    stage: Stage,
    sample: SampleId,
    position: f64,
    increment: f64,
    gain: f32,
    level: f32,
    attack_per_frame: f32,
    release_per_frame: f32,
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
            gain: 0.0,
            level: 0.0,
            attack_per_frame: 1.0,
            release_per_frame: 1.0,
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
    pub attack_ms: u16,
    pub release_ms: u16,
    pub release_after: Option<u32>,
}

fn per_frame(ms: u16, rate: u32) -> f32 {
    let frames = f32::from(ms) * rate as f32 / 1000.0;
    if frames <= 1.0 { 1.0 } else { 1.0 / frames }
}

impl Voice {
    /// Start a note.
    pub fn start(note: NoteOn, rate: u32) -> Self {
        let attack_per_frame = per_frame(note.attack_ms, rate);
        Self {
            stage: if attack_per_frame >= 1.0 {
                Stage::Hold
            } else {
                Stage::Attack
            },
            sample: note.sample,
            position: 0.0,
            increment: 2_f64.powf((f64::from(note.note) - 60.0) / 12.0),
            gain: note.gain,
            level: attack_per_frame.min(1.0),
            attack_per_frame,
            release_per_frame: per_frame(note.release_ms, rate),
            release_after: note.release_after,
        }
    }

    /// Whether it is sounding.
    pub fn active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Start the instrument's release.
    pub fn release(&mut self) {
        if self.active() {
            self.stage = Stage::Release {
                per_frame: self.release_per_frame,
            };
        }
    }

    /// Fade out quickly, to avoid a click.
    pub fn fade(&mut self) {
        if self.active() {
            self.stage = Stage::Release {
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
        let level = self.level * self.gain;
        let frame = [
            (first[0] + (second[0] - first[0]) * fraction) * level,
            (first[1] + (second[1] - first[1]) * fraction) * level,
        ];
        self.position += self.increment;
        if let Some(frames) = &mut self.release_after {
            if *frames == 0 {
                self.release_after = None;
                self.release();
            } else {
                *frames -= 1;
            }
        }
        match self.stage {
            Stage::Attack => {
                self.level += self.attack_per_frame;
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Hold;
                }
            }
            Stage::Release { per_frame } => {
                self.level -= per_frame;
                if self.level <= 0.0 {
                    self.stage = Stage::Idle;
                }
            }
            Stage::Hold | Stage::Idle => {}
        }
        frame
    }
}
