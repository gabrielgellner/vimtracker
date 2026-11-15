//! The engine's side of the recorder: starting and stopping it, on the song's grid when asked.

use std::sync::Arc;

use vt_model::constants::TICKS_PER_STEP;
use vt_model::growing::Growing;

use crate::recorder::{RecordQuantise, RecordSource, RecorderEvent};

use super::{Engine, Transport, sample_of_tick};

impl Engine {
    /// Listen to a source, so the margin before a recording's start is of it.
    pub(crate) fn listen_to(&mut self, source: RecordSource) {
        self.recorder.listen_to(source);
    }

    /// Record a source into a sample, at once or from the grid's next line, starting so many
    /// frames later still, for the input's latency. Armed on the grid while the song is stopped,
    /// it starts as the song does. Anything recording is cancelled first.
    pub fn record(
        &mut self,
        take: Arc<Growing>,
        source: RecordSource,
        quantise: RecordQuantise,
        latency: u64,
    ) {
        self.recorder.record(take, source, quantise, latency);
    }

    /// Stop recording, at once or at the grid's next line while the song plays, so many frames
    /// later still, for the input's latency. A recording not started yet is cancelled.
    pub fn stop_recording(&mut self, quantise: RecordQuantise, latency: u64) {
        let playing = self.transport == Transport::Playing;
        self.recorder.stop(quantise, latency, playing);
    }

    /// Take a recording back, to be dropped.
    pub(crate) fn cancel_recording(&mut self) {
        self.recorder.cancel();
    }

    /// Whether the recorder is recording, or waiting to.
    pub fn is_recording(&self) -> bool {
        self.recorder.is_busy()
    }

    /// How many frames the margin before a recording's start is.
    pub fn recording_margin(&self) -> u64 {
        self.recorder.margin()
    }

    /// The recorder's oldest event not taken yet: a start, or an end with what it recorded, to be
    /// freed off the audio thread.
    pub fn take_recorder_event(&mut self) -> Option<RecorderEvent> {
        self.recorder.take_event()
    }

    /// Room for the input's frames for the next block, to fill before it's rendered.
    pub fn input_room(&mut self, frames: usize) -> &mut [[f32; 2]] {
        self.recorder.input_room(frames)
    }

    /// The song's step about to fire: start or stop the recorder on its frame, if it's a line of
    /// the grid it waits for. The step may be worked out ahead of its place on the grid, as a
    /// groove can land it early, so the frame is its place's, not now.
    pub(super) fn recorder_line(&mut self) {
        let Some(steps) = self.recorder.waits_for() else {
            return;
        };
        if !self.on_grid_line(steps) {
            return;
        }
        let grid = self.next_step * TICKS_PER_STEP;
        let at = sample_of_tick(
            grid.saturating_sub(self.origin_tick),
            self.rate,
            self.millibpm,
        );
        let frames = at.saturating_sub(self.frames_since_origin);
        self.recorder.line(frames, self.next_step);
    }
}
