//! One complete state of the song, as the audio thread sees it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use vt_model::constants::{NUDGE_TICKS, TICKS_PER_STEP};
use vt_model::groove::GrooveId;
pub use vt_model::playable::Span;
use vt_model::playable::{Playable, normalize_peak, onset_analysis};
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::shape::{FillKind, ShapeColumn};
use vt_model::song::{Song, Track};
use vt_model::speed::Speed;
use vt_model::step::{FxCommand, most_nudge};

use crate::gens::{GenScratch, Unders};

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
    /// How far ahead of its place on the grid a step is worked out, in ticks, to play it early.
    ///
    /// It's as early as any groove the song can play lands a step, as shares of the longest step
    /// any track can play, and further by the earliest nudge in the song, which is the same time on
    /// every track. A live Nudge generator counts as early as its track's range allows, as the
    /// engine can move it. With neither, it's 0, and steps are worked out on the grid as they
    /// always were. Every track works its steps out this far ahead, so they're all worked out in
    /// the order they fall.
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
    /// Whether any section has a length, so there's a next step to fire, which the engine asks
    /// several times a run. One with a length and no chains is a rest, and one with neither is
    /// skipped.
    pub has_steps: bool,
    /// The steps under each live generator, which the engine renders a moving one from.
    pub(crate) unders: Unders,
    /// What each track's rows ask of the version, in the song's order, which the next version
    /// keeps for each track it shares with this one.
    rows: Vec<Arc<TrackRows>>,
}

/// What a track's rows ask of a version, found by reading every one of them: so it's kept from
/// the version before while the track is the very same, and read again only for a track an edit
/// copied.
#[derive(Debug, PartialEq, Eq)]
struct TrackRows {
    /// The grooves its steps' `GRV`s choose, each once. The project's, `00`, isn't among them.
    grooves: Vec<GrooveId>,
    /// Whether any of its steps has a `GRV`.
    groove_steps: bool,
    /// How far its earliest nudge moves its row, in nudges, if any row has one or a live generator
    /// could give one: 0 when every one is late.
    earliest: Option<i8>,
    /// The longest step it can play, in ticks: its own speed, or one a step's `RATE` chooses.
    longest: u64,
    /// Whether any of its steps has a `RATE`.
    speed_steps: bool,
}

/// A version on its way to the engine, or back from it to be dropped, with its generators' scratch.
///
/// Going, it's made for its song on the edit thread, with what its moving generators render into;
/// coming back, it's the version the engine was playing, with what they rendered into then, both to
/// be dropped on the edit thread. It moves through the queues whole, so the callback takes the
/// scratch without a lock.
#[derive(Debug)]
pub struct Handover {
    pub version: Arc<Published>,
    pub(crate) scratch: GenScratch,
}

impl Handover {
    /// Make room for a version's generators, here rather than in the callback.
    pub fn new(version: Arc<Published>) -> Self {
        let scratch = GenScratch::for_song(&version.song);
        Self { version, scratch }
    }
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
        let rows: Vec<Arc<TrackRows>> = (0..song.tracks.len())
            .map(|index| {
                last.filter(|last| song.tracks.same(index, &last.song.tracks, index))
                    .and_then(|last| last.rows.get(index))
                    .map_or_else(|| Arc::new(TrackRows::of(&song.tracks[index])), Arc::clone)
            })
            .collect();
        let (groove_lead, groove_steps) = lead(&song, &rows);
        let earliest = earliest_nudge(&rows);
        let (longest, speed_steps) = longest_step(&rows);
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
            has_steps: song.sections.iter().any(|s| song.section_length(s) > 0),
            unders: Unders::of(&song, last.map(|last| (&*last.song, &last.unders))),
            rows,
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
                    .sound(playable.sample)
                    .is_some_and(|sound| sound.same(&playable.sound))
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
fn longest_step(rows: &[Arc<TrackRows>]) -> (u64, bool) {
    let longest = rows.iter().map(|track| track.longest).max();
    let speed_steps = rows.iter().any(|track| track.speed_steps);
    (longest.unwrap_or(0).max(TICKS_PER_STEP), speed_steps)
}

impl TrackRows {
    /// Read a track's rows.
    fn of(track: &Track) -> Self {
        let (longest, speed_steps) = track_longest(track);
        let (grooves, groove_steps) = track_grooves(track);
        Self {
            grooves,
            groove_steps,
            earliest: track_earliest(track, longest),
            longest,
            speed_steps,
        }
    }
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
/// nudge is late.
fn earliest_nudge(rows: &[Arc<TrackRows>]) -> Option<i64> {
    rows.iter()
        .filter_map(|track| track.earliest)
        .min()
        .map(|nudge| i64::from(nudge) * NUDGE_TICKS as i64)
}

/// How far a track's earliest nudge moves its row, in nudges, if any row has one: 0 when every
/// nudge is late. A nudge plays no further than its step's range, so a track's own are as early
/// as the longest step it can play lets them be, and a live Nudge generator, which the engine can
/// move to any nudge its column takes, counts as the earliest of those, as a live `Divisions`
/// does as its triplets' earliest, which its dice or a macro may bring in on any row.
fn track_earliest(track: &Track, longest: u64) -> Option<i8> {
    let most = most_nudge(longest);
    let phrases = track.phrases.values();
    let generated = phrases
        .clone()
        .flat_map(|phrase| phrase.generators())
        .filter_map(move |generator| match generator.column {
            ShapeColumn::Nudge { most: own } => Some(-own.clamp(0, most)),
            ShapeColumn::Note { .. } if generator.shape.kind == FillKind::Divisions => {
                Some(generator.shape.divisions.earliest().clamp(-most, 0))
            }
            _ => None,
        });
    phrases
        .flat_map(|phrase| phrase.steps())
        .filter(|step| step.nudge != 0)
        .map(move |step| step.nudge.clamp(-most, 0))
        .chain(generated)
        .min()
}

/// How early any groove the song can play lands a step: the project's, the tracks' own, and any a
/// step's `GRV` chooses, and whether any step has one.
fn lead(song: &Song, rows: &[Arc<TrackRows>]) -> (u64, bool) {
    let own = song.tracks.iter().filter_map(|track| track.groove);
    let chosen = rows.iter().flat_map(|track| track.grooves.iter().copied());
    let lead = std::iter::once(song.groove)
        .chain(own)
        .chain(chosen)
        .map(|id| song.groove(id).lead())
        .max()
        .unwrap_or(0);
    (lead, rows.iter().any(|track| track.groove_steps))
}

/// The grooves a track's steps' `GRV`s choose, each once, and whether any step has one.
fn track_grooves(track: &Track) -> (Vec<GrooveId>, bool) {
    let mut grooves = Vec::new();
    let mut groove_steps = false;
    let slots = track
        .phrases
        .values()
        .flat_map(|phrase| phrase.steps())
        .flat_map(|step| step.fx.iter());
    for slot in slots {
        if slot.command == Some(FxCommand::Set(SettingId::GROOVE)) {
            groove_steps = true;
            // `00` is the project's, which is counted already.
            let chosen = usize::from(slot.coarse()).checked_sub(1);
            if let Some(id) = chosen.and_then(GrooveId::from_index)
                && !grooves.contains(&id)
            {
                grooves.push(id);
            }
        }
    }
    (grooves, groove_steps)
}

#[cfg(test)]
mod tests {
    use vt_model::action::{Action, edit};
    use vt_model::step::{FxSlot, Step};

    use super::*;

    /// What a version worked out from its song's rows.
    fn worked_out(version: &Published) -> (u64, u64, [bool; 4], Vec<&TrackRows>) {
        let flags = [
            version.nudges,
            version.groove_steps,
            version.speed_steps,
            version.speeds,
        ];
        let rows = version.rows.iter().map(|rows| &**rows).collect();
        (version.lead, version.groove_lead, flags, rows)
    }

    /// A row with a nudge and an FX command.
    fn row(nudge: i8, command: Option<SettingId>, coarse: u16) -> Step {
        let mut step: Step = "C-3 --".parse().unwrap();
        step.nudge = nudge;
        step.fx = vec![FxSlot {
            command: command.map(FxCommand::Set),
            value: coarse << 8,
            ..FxSlot::default()
        }];
        step
    }

    fn set(track: usize, step: usize, value: Step) -> Action {
        Action::SetStep {
            track,
            phrase: 0,
            step,
            value,
        }
    }

    #[test]
    fn a_version_after_another_works_out_what_one_from_nothing_does() {
        let pool = Arc::new(SamplePool::default());
        let mut song = Arc::new(Song::new(4));
        let mut last = Arc::new(Published::new(Arc::clone(&song), Arc::clone(&pool)));
        let edits = [
            // A groove chosen on a step, an early nudge, a slow step, a slow track.
            set(1, 0, row(0, Some(SettingId::GROOVE), 3)),
            set(2, 4, row(-9, None, 0)),
            set(0, 2, row(0, Some(SettingId::SPEED), Speed::Eighth as u16)),
            Action::SetTrackSpeed {
                track: 3,
                speed: Speed::Eighth,
            },
            Action::SetMilliBpm(98_000),
            // The fixed tracks move along, and back.
            Action::AddTracks(1),
            Action::RemoveLastTrack,
            // And each of them gone again.
            set(1, 0, row(0, None, 0)),
            set(2, 4, row(3, None, 0)),
            set(0, 2, row(0, None, 0)),
        ];
        let mut seen = Vec::new();
        for action in edits {
            let next = edit(&song, &action).unwrap();
            let after = Published::after(Arc::clone(&next), Arc::clone(&pool), Some(&last));
            let alone = Published::new(Arc::clone(&next), Arc::clone(&pool));
            assert_eq!(worked_out(&after), worked_out(&alone), "{action:?}");
            // A track the edit shared keeps what was read from its rows, and no other does.
            assert_eq!(after.rows.len(), next.tracks.len());
            for (index, rows) in after.rows.iter().enumerate() {
                let shared = next.tracks.same(index, &song.tracks, index);
                let was = last.rows.get(index);
                let kept = was.is_some_and(|was| Arc::ptr_eq(was, rows));
                assert_eq!(kept, shared, "track {index} after {action:?}");
            }
            seen.push((
                after.lead,
                after.nudges,
                after.groove_steps,
                after.speed_steps,
            ));
            (song, last) = (next, Arc::new(after));
        }
        // The edits did move what's worked out, so the comparison above had something to catch.
        assert!(seen.iter().any(|&(lead, ..)| lead > 0));
        assert_eq!(seen.last(), Some(&(0, true, false, false)));
        for (at, flags) in [(0, (false, true, false)), (2, (true, true, true))] {
            let (_, nudges, groove_steps, speed_steps) = seen[at];
            assert_eq!(
                (nudges, groove_steps, speed_steps),
                flags,
                "after edit {at}"
            );
        }
    }
}
