//! One complete state of the song, as the audio thread sees it.

use std::sync::Arc;

use vt_model::constants::{NUDGE_TICKS, TICKS_PER_STEP};
use vt_model::groove::GrooveId;
pub use vt_model::playable::{LoopSpan, Playable, Span};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_model::speed::Speed;
use vt_model::step::FxCommand;

/// Each bank number's entry, `00` to `FF`, worked out on the edit thread so the callback only has
/// to look it up, if it has one and its file loaded.
pub type Bank = Arc<[Option<Arc<Playable>>]>;

/// A song version, its samples, and how each of the bank's entries plays.
#[derive(Debug)]
pub struct Published {
    pub song: Arc<Song>,
    pub samples: Arc<SamplePool>,
    /// Each bank number's entry, `00` to `FF`: always 256 places.
    pub bank: Bank,
    /// How far ahead of its place on the grid a step is worked out, in ticks, so a groove or a
    /// nudge can play it early: as early as any groove the song can play lands a step, and further
    /// by the earliest nudge in the song, as shares of the longest step any track can play. With
    /// neither, it's 0, and steps are worked out on the grid as they always were. Every track
    /// works its steps out this far ahead, so they're all worked out in the order they fall.
    pub lead: u64,
    /// How early any groove the song can play lands a step, in ticks of a sixteenth, which a step's
    /// push is kept within.
    pub groove_lead: u64,
    /// Whether any row has a nudge, without which a step's row needn't be looked up for one.
    pub nudges: bool,
    /// Whether any step has a `GRV`, without which a step's groove needn't be looked for on it.
    pub groove_steps: bool,
    /// Whether any step has a `RATE`, without which a step's speed needn't be looked for on it.
    pub speed_steps: bool,
    /// Whether any track can play at another speed than a sixteenth, so a fast track's steps can
    /// be worked out before the one before has landed.
    pub speeds: bool,
}

impl Published {
    /// Build a version, working out each of the bank's entries on this thread, since the callback
    /// can't look up by string.
    pub fn new(song: Arc<Song>, samples: Arc<SamplePool>) -> Self {
        Self::after(song, samples, None)
    }

    /// Build a version after another, working out again only the bank's entries that changed, so
    /// the rest are the very ones the last version had.
    pub fn after(song: Arc<Song>, samples: Arc<SamplePool>, last: Option<&Published>) -> Self {
        let bank = match last {
            Some(last) if last.song.bank == song.bank && Arc::ptr_eq(&last.samples, &samples) => {
                Arc::clone(&last.bank)
            }
            _ => bank(&song, &samples, last),
        };
        let (groove_lead, groove_steps) = lead(&song);
        let earliest = earliest_nudge(&song);
        let (longest, speed_steps) = longest_step(&song);
        let early = groove_lead + earliest.map_or(0, |early| early.unsigned_abs());
        Self {
            bank,
            lead: early * longest / TICKS_PER_STEP,
            groove_lead,
            nudges: earliest.is_some(),
            groove_steps,
            speed_steps,
            speeds: speed_steps
                || song
                    .tracks
                    .iter()
                    .any(|track| track.speed != Speed::Sixteenth),
            song,
            samples,
        }
    }
}

/// Work out the bank's entries, keeping the last version's for each one that's the same and plays
/// the same sound.
fn bank(song: &Song, samples: &SamplePool, last: Option<&Published>) -> Bank {
    let mut bank = vec![None; 256];
    for (number, entry) in song.bank.iter() {
        let at = usize::from(number);
        let kept = last
            .filter(|last| last.song.bank.get(number) == Some(entry))
            .and_then(|last| last.bank[at].as_ref())
            .filter(|playable| {
                samples
                    .shared(playable.sample)
                    .is_some_and(|frames| Arc::ptr_eq(frames, &playable.frames))
            });
        bank[at] = match kept {
            Some(playable) => Some(Arc::clone(playable)),
            None => Playable::of(entry, samples).map(Arc::new),
        };
    }
    bank.into()
}

/// The longest step any track can play, in ticks: its own speed, or one a step's `RATE` chooses,
/// and whether any step has one.
fn longest_step(song: &Song) -> (u64, bool) {
    let mut longest = TICKS_PER_STEP;
    let mut speed_steps = false;
    for track in &song.tracks {
        longest = longest.max(track.speed.ticks());
        let slots = track
            .phrases
            .values()
            .flat_map(|phrase| phrase.steps())
            .flat_map(|step| step.fx.iter());
        for slot in slots {
            if slot.command == Some(FxCommand::Set(SettingId::SPEED)) {
                speed_steps = true;
                let speed = Speed::from_index(usize::from(slot.coarse()));
                longest = longest.max(speed.ticks());
            }
        }
    }
    (longest, speed_steps)
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
