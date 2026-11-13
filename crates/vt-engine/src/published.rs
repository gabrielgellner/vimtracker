//! One complete state of the song, as the audio thread sees it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use vt_model::constants::{NUDGE_TICKS, TICKS_PER_STEP};
use vt_model::groove::GrooveId;
pub use vt_model::playable::{LoopSpan, Playable, Span};
use vt_model::playable::{normalize_peak, onset_analysis};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::shape::ShapeColumn;
use vt_model::song::{Song, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, most_nudge};

use crate::gens::{GenScratch, ScratchSlot, Unders};

/// Each bank number's entry, `00` to `FF`, worked out on the edit thread so the callback only has
/// to look it up, if it has one and its file loaded.
pub type Bank = Arc<[Option<Arc<Playable>>]>;

/// A song version, its samples, and how each of the bank's entries plays.
#[derive(Debug)]
pub struct Published {
    /// Which version it is, different for each one built, for a report to say which version its
    /// values were worked out from.
    pub id: u64,
    pub song: Arc<Song>,
    pub samples: Arc<SamplePool>,
    /// Each bank number's entry, `00` to `FF`: always 256 places.
    pub bank: Bank,
    /// How far ahead of its place on the grid a step is worked out, in ticks, so a groove or a
    /// nudge can play it early: as early as any groove the song can play lands a step, as shares
    /// of the longest step any track can play, and further by the earliest nudge in the song,
    /// which is the same time on every track. A live Nudge generator counts as early as its
    /// track's range allows, as the engine can move it. With neither, it's 0, and steps are worked
    /// out on the grid as they always were. Every track works its steps out this far ahead, so
    /// they're all worked out in the order they fall.
    pub lead: u64,
    /// How early any groove the song can play lands a step, in ticks of a sixteenth, which a step's
    /// push is kept within.
    pub groove_lead: u64,
    /// Whether any row has a nudge, or a live generator could give one, without which a step's
    /// row needn't be looked up for one.
    pub nudges: bool,
    /// Whether any step has a `GRV`, without which a step's groove needn't be looked for on it.
    pub groove_steps: bool,
    /// Whether any step has a `RATE`, without which a step's speed needn't be looked for on it.
    pub speed_steps: bool,
    /// Whether any track can play at another speed than a sixteenth, so a fast track's steps can
    /// be worked out before the one before has landed.
    pub speeds: bool,
    /// The steps under each live generator, which the engine renders a moving one from.
    pub(crate) unders: Unders,
    /// What the engine renders moving generators into, until it takes it.
    pub(crate) scratch: ScratchSlot,
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
        let early = earliest.map_or(0, |early| early.unsigned_abs());
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            bank,
            lead: groove_lead * longest / TICKS_PER_STEP + early,
            groove_lead,
            nudges: earliest.is_some(),
            groove_steps,
            speed_steps,
            speeds: speed_steps
                || song
                    .tracks
                    .iter()
                    .any(|track| track.speed != Speed::Sixteenth),
            unders: Unders::of(&song, last.map(|last| (&*last.song, &last.unders))),
            scratch: ScratchSlot::new(GenScratch::for_song(&song)),
            song,
            samples,
        }
    }
}

/// Work out the bank's entries, keeping the last version's for each one that's the same, plays the
/// same sound, normalises from the same peak and starts its beats from the same sounds, so one
/// whose analysis has come in since is worked out again at its new gain and with its sounds.
fn bank(song: &Song, samples: &SamplePool, last: Option<&Published>) -> Bank {
    let mut bank = vec![None; 256];
    for (number, entry) in song.bank.iter() {
        let at = usize::from(number);
        let kept = last
            .filter(|last| last.song.bank.get(number) == Some(entry))
            .filter(|last| normalize_peak(entry, &last.samples) == normalize_peak(entry, samples))
            .filter(|last| {
                let was = onset_analysis(entry, &last.samples).map(Arc::as_ptr);
                was == onset_analysis(entry, samples).map(Arc::as_ptr)
            })
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
        let (own, rates) = track_longest(track);
        longest = longest.max(own);
        speed_steps |= rates;
    }
    (longest, speed_steps)
}

/// The longest step a track can play, in ticks, its own speed's or one a `RATE` on its rows
/// chooses, and whether any of its rows has one.
fn track_longest(track: &Track) -> (u64, bool) {
    let mut longest = track.speed.ticks();
    let mut speed_steps = false;
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
    (longest, speed_steps)
}

/// How far the earliest nudge in the song moves its row, in ticks, if any row has one: 0 when every
/// nudge is late. A nudge plays no further than its step's range, so a track's own are as early
/// as the longest step it can play lets them be, and a live Nudge generator, which the engine can
/// move to any nudge its column takes, counts as the earliest of those.
fn earliest_nudge(song: &Song) -> Option<i64> {
    song.tracks
        .iter()
        .flat_map(|track| {
            let most = most_nudge(track_longest(track).0);
            let phrases = track.phrases.values();
            let generated = phrases
                .clone()
                .flat_map(|phrase| phrase.generators())
                .filter_map(move |generator| match generator.column {
                    ShapeColumn::Nudge { most: own } => Some(-own.clamp(0, most)),
                    _ => None,
                });
            phrases
                .flat_map(|phrase| phrase.steps())
                .filter(|step| step.nudge != 0)
                .map(move |step| step.nudge.clamp(-most, 0))
                .chain(generated)
        })
        .min()
        .map(|nudge| i64::from(nudge) * NUDGE_TICKS as i64)
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
