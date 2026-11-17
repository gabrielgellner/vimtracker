//! Each FX column's lane as it plays: counted along, a row at a time, so a turn of the track's
//! `Length` or `Rate` for it takes effect at its next row rather than moving it, as an LFO's
//! `Rate` carries on from where it is (the lanes study's §5).

use vt_model::lane::{Heard, LANE_RATES, Lane, LaneMove};
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
    /// drawing a playhead without playing.
    pub fn placed(lane: Lane, heard: Heard, count: i64) -> Self {
        LaneRun::default().read(lane, count, (0, 0), 0, heard, 0)
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
    /// It then waits for the step after.
    fn read(
        &mut self,
        lane: Lane,
        count: i64,
        entry: Entry,
        offset: i64,
        heard: Heard,
        start: usize,
    ) -> LaneRead {
        if let Some((at, was, read)) = self.last
            && (at, was) == (count, entry)
        {
            return read;
        }
        *self = self.at(count, entry, offset, heard);
        let first = self.left == 0;
        if first {
            if self.row >= heard.length {
                self.row = 0;
                self.laps += 1;
            }
            self.left = heard.division.max(1);
        }
        let read = LaneRead {
            row: lane.stored_from(self.row, start),
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
    fn ahead(self, count: i64, entry: Entry, offset: i64, heard: Heard) -> LaneAhead {
        let run = self.at(count, entry, offset, heard);
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
    /// chain, places it again from the count.
    pub(crate) fn lane_row(
        &mut self,
        slot: usize,
        lane: Lane,
        entry: Entry,
        count: i64,
    ) -> LaneRead {
        let heard = self.heard_lane(slot, lane);
        let start = self.lane_start(slot, lane);
        let offset = self.lane_offsets[slot];
        self.lane_runs[slot].read(lane, count, entry, offset, heard, start)
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
        self.lane_runs[slot].ahead(count, entry, self.lane_offsets[slot], heard)
    }

    /// Have an FX column's lane go on from `at` at the step `count` in `entry`, as an edit keeps it
    /// on the row it was going to read.
    pub(crate) fn put_lane(&mut self, slot: usize, entry: Entry, count: i64, at: LaneAhead) {
        self.lane_runs[slot] = LaneRun {
            next: Some((count, entry)),
            row: at.row,
            left: at.left,
            laps: at.laps,
            last: None,
        };
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

    /// An FX column's `Length`, `Rate` and `Start` as held, with what modulation adds.
    fn lane_moved(&self, slot: usize) -> LaneMove {
        let value = |at: usize| {
            let by = SettingId::lane(slot, at).map_or(0.0, |id| self.offsets[id.index()]);
            (self.lane_held[slot][at] + by * 255.0)
                .clamp(0.0, 255.0)
                .round() as u8
        };
        LaneMove {
            length: value(0),
            rate: value(1).min(LANE_RATES.len() as u8 - 1),
            start: value(2),
        }
    }
}

/// What each FX column's `Length` and `Rate` hold at rest, on the `00` to `FF` scale.
pub(super) fn lanes_at_rest() -> [f32; LANE_SETTINGS] {
    let rest = LaneMove::default();
    [
        f32::from(rest.length),
        f32::from(rest.rate),
        f32::from(rest.start),
    ]
}
