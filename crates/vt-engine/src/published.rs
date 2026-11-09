//! One complete state of the song, as the audio thread sees it.

use std::sync::Arc;

use vt_model::constants::NUDGE_TICKS;
use vt_model::groove::GrooveId;
use vt_model::samples::{SampleId, SamplePool};
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_model::step::FxCommand;

/// A song version, its samples, and which sample each track plays.
#[derive(Debug)]
pub struct Published {
    pub song: Arc<Song>,
    pub samples: Arc<SamplePool>,
    pub track_samples: Vec<Option<SampleId>>,
    /// How far ahead of its place on the grid a step is worked out, in ticks, so a groove or a
    /// nudge can play it early: as early as any groove the song can play lands a step, and further
    /// by the earliest nudge in the song. With neither, it's 0, and steps are worked out on the
    /// grid as they always were.
    pub lead: u64,
    /// How early any groove the song can play lands a step, which a step's push is kept within.
    pub groove_lead: u64,
    /// Whether any row has a nudge, without which a step's row needn't be looked up for one.
    pub nudges: bool,
    /// Whether any step has a `GRV`, without which a step's groove needn't be looked for on it.
    pub groove_steps: bool,
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
        let (groove_lead, groove_steps) = lead(&song);
        let earliest = earliest_nudge(&song);
        Self {
            track_samples,
            lead: groove_lead + earliest.map_or(0, |early| early.unsigned_abs()),
            groove_lead,
            nudges: earliest.is_some(),
            groove_steps,
            song,
            samples,
        }
    }
}

/// How far the earliest nudge in the song moves its row, in ticks, if any row has one: 0 when every
/// nudge is late.
fn earliest_nudge(song: &Song) -> Option<i64> {
    song.tracks
        .iter()
        .flat_map(|track| track.phrases.values())
        .flat_map(|phrase| phrase.steps())
        .filter(|step| step.nudge != 0)
        .map(|step| i64::from(step.nudge.min(0)) * NUDGE_TICKS as i64)
        .min()
}

/// How early any groove the song can play lands a step: the project's, the tracks' own, and any a
/// step's `GRV` chooses, and whether any step has one.
fn lead(song: &Song) -> (u64, bool) {
    let mut grooves: Vec<GrooveId> = vec![song.groove];
    let mut groove_steps = false;
    grooves.extend(song.tracks.iter().filter_map(|track| track.groove));
    let steps = song
        .tracks
        .iter()
        .flat_map(|track| track.phrases.values())
        .flat_map(|phrase| phrase.steps());
    for slot in steps.flat_map(|step| step.fx.iter()) {
        if slot.command == Some(FxCommand::Set(SettingId::GROOVE)) {
            groove_steps = true;
            // `00` is the project's, which is counted already.
            let chosen = usize::from(slot.coarse()).checked_sub(1);
            grooves.extend(chosen.and_then(GrooveId::from_index));
        }
    }
    let lead = grooves
        .into_iter()
        .map(|id| song.groove(id).lead())
        .max()
        .unwrap_or(0);
    (lead, groove_steps)
}
