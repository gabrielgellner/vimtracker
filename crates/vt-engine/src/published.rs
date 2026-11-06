//! One complete state of the song, as the audio thread sees it.

use std::sync::Arc;

use vt_model::samples::{SampleId, SamplePool};
use vt_model::song::Song;

/// A song version, its samples, and which sample each track plays.
#[derive(Debug)]
pub struct Published {
    pub song: Arc<Song>,
    pub samples: Arc<SamplePool>,
    pub track_samples: Vec<Option<SampleId>>,
}

impl Published {
    /// Build a version, resolving each track's sample path on this thread, since the callback
    /// can't look up by string.
    pub fn new(song: Arc<Song>, samples: Arc<SamplePool>) -> Self {
        let track_samples = song
            .tracks
            .iter()
            .map(|track| samples.id_of(track.sample.as_deref()?))
            .collect();
        Self {
            song,
            samples,
            track_samples,
        }
    }
}
