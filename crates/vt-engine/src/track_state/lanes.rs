//! Each FX column's lane as it plays: counted along, a row at a time, so a turn of the track's
//! `Length` or `Rate` for it takes effect at its next row rather than moving it, as an LFO's
//! `Rate` carries on from where it is (the lanes study's §5). With the track's `Cue` on, a turn of
//! its `Length`, `Rate` or `Start` waits for the lane to come round to its first row, as Kria's
//! Division Cueing does (decision 402). Its `Jump` has a row play any of the lane's rows instead
//! of the next, which no cue holds back, as a walk's choice isn't a loop's bounds (decision 415),
//! and its `Order` and `Reverse` walk its rows in another order, under its jumps and holds, which
//! no cue holds back either (decision 431).

use vt_model::constants::FX_SLOTS;
use vt_model::lane::{Heard, LANE_RATES, Lane, LaneMove};
use vt_model::order::{Dealer, LaneOrder, LaneWalked, Order, lane_held, lane_jumped};
use vt_model::settings::{LANE_SETTINGS, SettingId};

use super::TrackState;

/// Which entry of its chain a track read a lane in: the phrase, and its place in the chain. A
/// lane is placed again from the count as a new one starts.
pub(crate) type Entry = (u32, usize);

/// Where an FX column's lane is as it plays. It's kept for the next step it reads at, so an edit
/// can put it on the row it was going to read.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LaneRun {
    /// The count of the track's steps it reads at next and the chain entry it's in, or none until
    /// it's placed, after its chain starts again, as on play, a seek or a reset. A pause keeps it,
    /// so playing on carries on.
    next: Option<(i64, Entry)>,
    /// Its row in the length it's heard at, from 0: the one it's on, or, with none of it `left`,
    /// the one it starts next, which goes round to the first if it's past the length heard then.
    row: usize,
    /// How many of the track's steps of its row are still to be read; none if the row starts at
    /// the next, when its division is read.
    left: usize,
    /// How many times it has come round, which its generator's passes and a `CND` count.
    laps: i64,
    /// What it read last, at a count in an entry, for a step that reads it again.
    last: Option<(i64, Entry, LaneRead)>,
    /// With the track's `Cue` on, how it's heard and the start it took as it was placed or last
    /// came round to its first row, which it keeps until it comes round again (decision 402).
    cued: Option<(Heard, usize)>,
    /// Which of the lane's own rows the row it's on plays instead, as its `Jump` decided as the
    /// row started, or as it was first read after being placed: none until decided, then none
    /// again for the row counted to (decision 415).
    jumped: Option<Option<usize>>,
    /// The row among those heard its `Order` and `Reverse` walk the row it's on to, decided with
    /// its `Jump`, which plays unless a jump or hold takes its place (decision 431).
    order_row: usize,
    /// The stored row the row it's on plays because its `Hold` kept the one before, decided with
    /// its `Jump` (decision 422).
    kept: Option<usize>,
    /// The stored row it last played, which a row its `Hold` keeps plays again; none after it's
    /// placed, so a lane's first row is its own.
    played: Option<usize>,
    /// Where its `Order` last walked to, which the next row steps on from rather than walking
    /// from the lap's first; none after it's placed (decision 431).
    walked: Option<LaneWalked>,
}

/// What a lane's `Jump`, `Hold` and `Order` roll with: the track's dealer, the lane's column,
/// their chances now, `00` to `FF`, and the order its rows are walked in now.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LaneJump {
    pub dealer: Dealer,
    pub column: usize,
    pub chance: u8,
    pub hold: u8,
    pub order: LaneOrder,
    /// After an `SED`, the count of the track's steps since its row, which a row's jump and hold
    /// are rolled at in place of the lane's lap and row, so they come out the same each time the
    /// row plays (decisions 421 and 422).
    pub since: Option<i64>,
}

impl LaneJump {
    /// None: every row is the one counted to.
    fn none() -> Self {
        Self {
            dealer: Dealer::default(),
            column: 0,
            chance: 0,
            hold: 0,
            order: LaneOrder::default(),
            since: None,
        }
    }
}

/// What a lane read at one of the track's steps: the stored row, how many times it had come
/// round, whether its row starts there, which its FX run on, and how it was heard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneRead {
    pub row: usize,
    pub laps: i64,
    pub first: bool,
    /// How many of the lane's own rows on it starts, as the track's `Start` for its column has it
    /// (decision 401), kept small as a report carries one for every lane of every track.
    pub start: u16,
    pub heard: Heard,
}

impl LaneRead {
    /// What a lane heard so reads at a count of its track's steps since its chain started, worked
    /// out from the count, as playing from the top with nothing turned on the way places it: for
    /// drawing a playhead without playing, so with no `Jump`, but walked in its `order`, dealt from
    /// the track's `dealer` (decision 431).
    pub fn placed(lane: Lane, heard: Heard, count: i64, order: LaneOrder, dealer: Dealer) -> Self {
        let jump = LaneJump {
            dealer,
            order,
            ..LaneJump::none()
        };
        LaneRun::default().read(lane, count, (0, 0), 0, (heard, 0), false, jump)
    }
}

impl LaneRun {
    /// Where it is at a place in its rows, `place` being the track's count since its chain started
    /// with the lane's offset: worked out from the place at the length and rate heard now, as
    /// playing from the top would have it with nothing turned on the way.
    fn placed(place: i64, heard: Heard, next: (i64, Entry)) -> Self {
        let division = heard.division.max(1) as i64;
        let length = heard.length.max(1) as i64;
        let (rows, within) = (place.div_euclid(division), place.rem_euclid(division));
        Self {
            next: Some(next),
            row: rows.rem_euclid(length) as usize,
            left: if within == 0 {
                0
            } else {
                (division - within) as usize
            },
            laps: rows.div_euclid(length),
            last: None,
            cued: None,
            jumped: None,
            order_row: 0,
            kept: None,
            played: None,
            walked: None,
        }
    }

    /// Where it is for the step at `count` in `entry`: on from where it was if that's the step it
    /// was waiting for, or placed again from the count, with `offset`, if not.
    fn at(self, count: i64, entry: Entry, offset: i64, heard: Heard) -> Self {
        match self.next {
            Some(next) if next == (count, entry) => self,
            _ => Self::placed(count + offset, heard, (count, entry)),
        }
    }

    /// Read the lane at the step `count` in `entry`: its row there, starting it if it starts
    /// there, past the length heard now going round to the first, with the division heard now.
    /// With `cue`, the length, division and start are those it took as it was placed or last came
    /// round, and what's heard `now` is taken as it comes round again. Its `jump` is read as each
    /// row starts, whatever the cue. It then waits for the step after.
    #[allow(clippy::too_many_arguments)]
    fn read(
        &mut self,
        lane: Lane,
        count: i64,
        entry: Entry,
        offset: i64,
        now: (Heard, usize),
        cue: bool,
        jump: LaneJump,
    ) -> LaneRead {
        if let Some((at, was, read)) = self.last
            && (at, was) == (count, entry)
        {
            return read;
        }
        *self = self.at(count, entry, offset, now.0);
        let (mut heard, mut start) = match self.cued {
            Some(cued) if cue => cued,
            _ => now,
        };
        let first = self.left == 0;
        if first {
            if self.row >= heard.length {
                self.row = 0;
                self.laps += 1;
            }
            if self.row == 0 {
                (heard, start) = now;
            }
            self.left = heard.division.max(1);
            self.jumped = None;
        }
        self.cued = cue.then_some((heard, start));
        // Whether the row jumps, and whether it keeps the row before, is decided once, for all its
        // steps, as a phrase's visit is. A row kept plays the one before again, whatever its jump.
        // After an `SED`, both are rolled at the count since its row in place of the lane's lap and
        // row, so they come out the same each time the row plays (decision 421).
        let (lap, row) = jump.since.map_or((self.laps, self.row), |since| (since, 0));
        let deciding = self.jumped.is_none();
        // Which of the rows heard its order walks to is decided with them, under them. After an
        // `SED`, a dealt order is dealt from the lane's rows counted since its row, a lap of the
        // lane's length at a time, so it comes out the same each time the row plays (decision 431).
        if deciding {
            let rows = heard.length;
            let (lap, row) = match jump.since {
                Some(since) if jump.order.order().is_dealt() => {
                    let division = heard.division.max(1) as i64;
                    let into = division - self.left as i64;
                    let counted = (since - into).div_euclid(division);
                    let length = rows.max(1) as i64;
                    (
                        counted.div_euclid(length),
                        counted.rem_euclid(length) as usize,
                    )
                }
                _ => (self.laps, self.row),
            };
            self.order_row =
                jump.order
                    .walked(jump.dealer, jump.column, lap, row, rows, &mut self.walked);
        }
        let jumped = *self.jumped.get_or_insert_with(|| {
            let chance = f32::from(jump.chance) / 255.0;
            // Within the rows it's heard for, from its `Start`, as its order walks them
            // (decision 437).
            let rows = heard.length.max(1);
            lane_jumped(jump.dealer, jump.column, lap, row, rows, chance)
        });
        if deciding {
            let chance = f32::from(jump.hold) / 255.0;
            self.kept = self
                .played
                .filter(|_| lane_held(jump.dealer, jump.column, lap, row, chance));
        }
        let stored = self.kept.unwrap_or_else(|| {
            jumped.map_or_else(
                || lane.stored_from(self.order_row, start),
                |own| lane.stored_from(own, start),
            )
        });
        self.played = Some(stored);
        let read = LaneRead {
            row: stored,
            laps: self.laps,
            first,
            start: start as u16,
            heard,
        };
        self.left -= 1;
        if self.left == 0 {
            self.row += 1;
        }
        self.next = Some((count + 1, entry));
        self.last = Some((count, entry, read));
        read
    }

    /// Where it is at the step `count` in `entry`, without reading it: its row in the length heard
    /// and how many steps of it are left, none if it starts there.
    fn ahead(self, count: i64, entry: Entry, offset: i64, heard: Heard, cue: bool) -> LaneAhead {
        let run = self.at(count, entry, offset, heard);
        let heard = match run.cued {
            Some((cued, _)) if cue => cued,
            _ => heard,
        };
        let round = run.left == 0 && run.row >= heard.length;
        LaneAhead {
            row: if round { 0 } else { run.row },
            left: run.left,
            laps: run.laps + i64::from(round),
        }
    }
}

/// Where a lane is at a step it hasn't read yet: its row in the length heard, the steps of it
/// left, none if it starts there, and how many times it has come round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LaneAhead {
    pub row: usize,
    pub left: usize,
    pub laps: i64,
}

impl TrackState {
    /// Where an FX column's lane reads at a count of the track's steps since its chain started,
    /// in an entry of its chain, and how it's heard there: the one place every lane is read from
    /// as the track plays. Read once a step, as the step is worked out, in order, it counts along;
    /// a step it wasn't waiting for, as after playing starts, a seek, a reset or a new entry of the
    /// chain, places it again from the count. Its `Jump` rolls from the track's `dealer`.
    pub(crate) fn lane_row(
        &mut self,
        slot: usize,
        lane: Lane,
        entry: Entry,
        count: i64,
        dealer: Dealer,
    ) -> LaneRead {
        let now = (self.heard_lane(slot, lane), self.lane_start(slot, lane));
        let offset = self.lane_offsets[slot];
        let cue = self.cues();
        let jump = LaneJump {
            dealer,
            column: slot,
            chance: self.lane_moved(slot).jump,
            hold: self.lane_moved(slot).hold,
            order: self.lane_moved(slot).walk(),
            since: self.reseed.map(|reseed| count - reseed.from.count),
        };
        self.lane_runs[slot].read(lane, count, entry, offset, now, cue, jump)
    }

    /// Where an FX column's lane was going to read at the step `count` in `entry`, as it's heard
    /// now, for an edit to keep it there.
    pub(crate) fn lane_ahead(
        &self,
        slot: usize,
        lane: Lane,
        entry: Entry,
        count: i64,
    ) -> LaneAhead {
        let heard = self.heard_lane(slot, lane);
        let cue = self.cues();
        self.lane_runs[slot].ahead(count, entry, self.lane_offsets[slot], heard, cue)
    }

    /// Have an FX column's lane go on from `at` at the step `count` in `entry`, as an edit keeps it
    /// on the row it was going to read, with what it took for its cue.
    pub(crate) fn put_lane(&mut self, slot: usize, entry: Entry, count: i64, at: LaneAhead) {
        self.lane_runs[slot] = LaneRun {
            next: Some((count, entry)),
            row: at.row,
            left: at.left,
            laps: at.laps,
            last: None,
            cued: self.lane_runs[slot].cued,
            jumped: None,
            order_row: 0,
            kept: None,
            played: self.lane_runs[slot].played,
            walked: self.lane_runs[slot].walked,
        };
    }

    /// Where each lane is now, to put back with [`Self::put_lane_runs`] for reading them again.
    pub(crate) fn lane_runs_now(&self) -> [LaneRun; FX_SLOTS] {
        self.lane_runs
    }

    /// Put each lane back where [`Self::lane_runs_now`] found it.
    pub(crate) fn put_lane_runs(&mut self, runs: [LaneRun; FX_SLOTS]) {
        self.lane_runs = runs;
    }

    /// Forget where each lane is, so each is placed again from the count as it's next read.
    pub(crate) fn unplace_lanes(&mut self) {
        self.lane_runs = Default::default();
    }

    /// Forget where an FX column's lane is, so it's placed again from the count, with its offset,
    /// as it's next read.
    pub(crate) fn unplace_lane(&mut self, slot: usize) {
        self.lane_runs[slot] = LaneRun::default();
    }

    /// How an FX column's lane is heard now: its own length and division moved by the track's
    /// `Length` and `Rate` for the column as a lock, a macro and modulation have them.
    pub(crate) fn heard_lane(&self, slot: usize, lane: Lane) -> Heard {
        lane.heard(self.lane_moved(slot))
    }

    /// How many of a lane's own rows on it starts, as the track's `Start` for its column and what
    /// modulation adds have it now (decision 401).
    pub(crate) fn lane_start(&self, slot: usize, lane: Lane) -> usize {
        lane.heard_start(self.lane_moved(slot))
    }

    /// An FX column's lane's settings as held, with what modulation adds: across the whole
    /// `00` to `FF` for those on a scale, and across its list for `Rate`, `Order` and `Reverse`,
    /// as the walk's are (decisions 431 and 435).
    fn lane_moved(&self, slot: usize) -> LaneMove {
        let value = |at: usize| {
            let by = SettingId::lane(slot, at)
                .map_or(0.0, |id| self.offsets[id.index()] * id.entry().span());
            (self.lane_held[slot][at] + by).clamp(0.0, 255.0).round() as u8
        };
        LaneMove {
            length: value(0),
            rate: value(1).min(LANE_RATES.len() as u8 - 1),
            start: value(2),
            jump: value(3),
            hold: value(4),
            order: value(5).min(Order::ALL.len() as u8 - 1),
            reverse: value(6) >= 1,
        }
    }
}

/// What each FX column's lane's settings hold at rest, on the `00` to `FF` scale.
pub(super) fn lanes_at_rest() -> [f32; LANE_SETTINGS] {
    let rest = LaneMove::default();
    std::array::from_fn(|at| f32::from(rest.value(at)))
}
