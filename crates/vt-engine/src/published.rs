//! One complete state of the song, as the audio thread sees it.

use std::collections::BTreeMap;
use std::sync::Arc;

use vt_model::samples::{SampleId, SamplePool};
use vt_model::song::Song;

/// A song version, its samples, and which sample each instrument plays.
#[derive(Debug)]
pub struct Published {
    pub song: Arc<Song>,
    pub samples: Arc<SamplePool>,
    pub instrument_samples: BTreeMap<u32, SampleId>,
}

impl Published {
    /// Build a version, resolving each instrument's sample path on this thread, since the
    /// callback can't look up by string.
    pub fn new(song: Arc<Song>, samples: Arc<SamplePool>) -> Self {
        let instrument_samples = song
            .instruments
            .iter()
            .filter_map(|(id, instrument)| {
                let sample = instrument.sample.as_deref()?;
                Some((*id, samples.id_of(sample)?))
            })
            .collect();
        Self {
            song,
            samples,
            instrument_samples,
        }
    }
}
