//! How a level meter moves: what it shows from one frame to the next, with no drawing, so a mixer
//! can have one per track later.

use vt_engine::mixer::Reading;
use vt_model::insert::INSERTS;

/// The bottom of the scale, in dB, where the meter reads silence.
pub const FLOOR_DB: f32 = -65.0;
/// Tuning: how fast a bar falls, in dB per second.
const FALL_DB_PER_SECOND: f32 = 24.0;
/// Tuning: how long the highest level stays marked, in seconds.
const HOLD_SECONDS: f32 = 1.5;
/// Tuning: how long the average takes to follow the sound, in seconds. A VU meter uses 0.3.
const AVERAGE_SECONDS: f32 = 0.3;

/// The top of the scale, in dB. Levels can go higher, and the readout still shows them.
pub(crate) const TOP_DB: f32 = 6.0;

/// The scale, as dB and height pairs, modelled on Ableton Live's: an even spread above `-20` for
/// detail near full level, tightening below it. Live's manual gives no values, so these are a
/// close match rather than a copy.
const SCALE: [(f32, f32); 8] = [
    (FLOOR_DB, 0.0),
    (-60.0, 0.02),
    (-50.0, 0.07),
    (-40.0, 0.14),
    (-30.0, 0.28),
    (-20.0, 0.45),
    (0.0, 0.873),
    (TOP_DB, 1.0),
];

/// A linear level in dB, stopping at the floor.
pub(crate) fn to_db(level: f32) -> f32 {
    if level > 0.0 {
        (20.0 * level.log10()).max(FLOOR_DB)
    } else {
        FLOOR_DB
    }
}

/// How high up the scale a level sits, from 0 to 1.
pub(crate) fn height_of(db: f32) -> f32 {
    let db = db.clamp(FLOOR_DB, TOP_DB);
    for pair in SCALE.windows(2) {
        let ((low_db, low), (high_db, high)) = (pair[0], pair[1]);
        if db <= high_db {
            return low + (db - low_db) / (high_db - low_db) * (high - low);
        }
    }
    1.0
}

/// The level at a height on the scale, the reverse of [`height_of`].
pub(crate) fn db_at(height: f32) -> f32 {
    let height = height.clamp(0.0, 1.0);
    for pair in SCALE.windows(2) {
        let ((low_db, low), (high_db, high)) = (pair[0], pair[1]);
        if height <= high {
            return low_db + (height - low) / (high - low) * (high_db - low_db);
        }
    }
    TOP_DB
}

/// A value that jumps up at once, falls slowly, and remembers its recent highest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    floor: f32,
    /// Where the bar is now.
    pub now: f32,
    /// The recent highest.
    pub top: f32,
    left: f32,
}

impl Held {
    /// Resting at `floor`.
    pub fn new(floor: f32) -> Self {
        Self {
            floor,
            now: floor,
            top: floor,
            left: 0.0,
        }
    }

    /// Move on by some seconds, given what it is now.
    pub fn update(&mut self, value: f32, seconds: f32) {
        let fall = FALL_DB_PER_SECOND * seconds;
        self.now = (self.now - fall).max(value).max(self.floor);
        if self.now >= self.top {
            self.top = self.now;
            self.left = HOLD_SECONDS;
        } else if self.left > 0.0 {
            self.left -= seconds;
        } else {
            self.top = (self.top - fall).max(self.now);
        }
    }
}

/// One channel's bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Channel {
    pub peak: Held,
    /// The average power, which follows how loud the sound seems.
    power: f32,
}

impl Channel {
    /// The average level in dB.
    pub fn average(&self) -> f32 {
        if self.power > 0.0 {
            (10.0 * self.power.log10()).max(FLOOR_DB)
        } else {
            FLOOR_DB
        }
    }
}

/// A stereo meter and how much the limiter is taking off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Meter {
    pub channels: [Channel; 2],
    /// The limiter's gain reduction, in dB above zero.
    pub reduction: Held,
}

impl Default for Meter {
    fn default() -> Self {
        let channel = Channel {
            peak: Held::new(FLOOR_DB),
            power: 0.0,
        };
        Self {
            channels: [channel; 2],
            reduction: Held::new(0.0),
        }
    }
}

impl Meter {
    /// Move on by some seconds, given what the callback reported in that time, if anything.
    pub fn update(&mut self, reading: Option<Reading>, seconds: f32) {
        let reading = reading.unwrap_or_default();
        let mut reduction = 0.0_f32;
        for (index, channel) in self.channels.iter_mut().enumerate() {
            channel.peak.update(to_db(reading.before[index]), seconds);
            if reading.frames > 0 {
                let power = reading.power[index] / reading.frames as f32;
                let follow = 1.0 - (-seconds / AVERAGE_SECONDS).exp();
                channel.power += (power - channel.power) * follow;
            }
            if reading.after[index] > 0.0 {
                let taken = 20.0 * (reading.before[index] / reading.after[index]).log10();
                reduction = reduction.max(taken);
            }
        }
        self.reduction.update(reduction, seconds);
    }

    /// The highest recent peak of either channel.
    pub fn top(&self) -> f32 {
        self.channels[0].peak.top.max(self.channels[1].peak.top)
    }
}

/// How many places a gain reduction history keeps across its sweep, finer than any box it's
/// drawn in, so each column of one is the deepest of the places it covers.
pub(crate) const HISTORY_PLACES: usize = 256;
/// How long a gain reduction history's sweep is, in beats: two bars.
pub(crate) const HISTORY_BEATS: f64 = 8.0;

/// What each FX slot of the cursor's track turned down over the last two bars, for the FX page's
/// zoom (decision 329). It's a sweep: a place a little over a sixtieth of a second wide at 120 BPM
/// is written as the reports come in, left to right, and it starts again at the left after two
/// bars at the song's tempo, so a steady beat's pumping stands still and the newest is where the
/// sweep is. It's sampled from the engine's reports, a frame at a time, on the app's side, and
/// holds all it will ever hold from the start.
#[derive(Debug, Clone, PartialEq)]
pub struct ReductionHistory {
    /// Each slot's reduction in each place, in dB.
    places: [[f32; HISTORY_PLACES]; INSERTS],
    /// Where the sweep is, in places, from `0` up to [`HISTORY_PLACES`].
    at: f64,
    /// The track it's of, so another track's starts empty.
    track: usize,
}

impl Default for ReductionHistory {
    fn default() -> Self {
        Self {
            places: [[0.0; HISTORY_PLACES]; INSERTS],
            at: 0.0,
            track: 0,
        }
    }
}

impl ReductionHistory {
    /// Move the sweep on by some seconds at a tempo in BPM, writing what each slot of a track is
    /// turning down now into each place it passes into, or, if it's still in the same one, the
    /// deepest there. A different track's starts again, empty.
    pub fn update(&mut self, track: usize, reduction: [f32; INSERTS], seconds: f32, bpm: f64) {
        if track != self.track {
            *self = Self {
                track,
                ..Self::default()
            };
        }
        let sweep = HISTORY_BEATS * 60.0 / bpm.max(1.0);
        let moved = f64::from(seconds.max(0.0)) / sweep * HISTORY_PLACES as f64;
        let first = self.at.floor() as usize;
        // However long it was, the sweep goes round once at most.
        let passed = ((self.at + moved).floor() as usize - first).min(HISTORY_PLACES);
        for (slot, places) in self.places.iter_mut().enumerate() {
            let now = reduction[slot].max(0.0);
            if passed == 0 {
                let here = &mut places[first % HISTORY_PLACES];
                *here = here.max(now);
            }
            for step in 1..=passed {
                places[(first + step) % HISTORY_PLACES] = now;
            }
        }
        self.at = (self.at + moved) % HISTORY_PLACES as f64;
    }

    /// The column of a box this many wide the sweep is in.
    pub fn head(&self, columns: usize) -> usize {
        (self.at as usize * columns / HISTORY_PLACES).min(columns.saturating_sub(1))
    }

    /// The deepest a slot turned down in a column of a box this many wide, across the places it
    /// covers, in dB.
    pub fn column(&self, slot: usize, column: usize, columns: usize) -> f32 {
        let columns = columns.max(1);
        let from = column * HISTORY_PLACES / columns;
        let to = ((column + 1) * HISTORY_PLACES / columns).max(from + 1);
        self.places[slot][from..to.min(HISTORY_PLACES)]
            .iter()
            .fold(0.0, |deepest, db| deepest.max(*db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 0.033;

    fn reading(before: f32, after: f32) -> Reading {
        Reading {
            before: [before; 2],
            after: [after; 2],
            power: [before * before * 100.0; 2],
            frames: 100,
        }
    }

    #[test]
    fn the_scale_runs_bottom_to_top_and_spreads_out_the_top() {
        assert_eq!(height_of(FLOOR_DB), 0.0);
        assert_eq!(height_of(TOP_DB), 1.0);
        assert_eq!(height_of(20.0), 1.0, "nothing draws above the top");
        assert_eq!(height_of(-20.0), 0.45);
        assert!(height_of(0.0) < 1.0, "there is room above full level");
        assert!(height_of(0.0) - height_of(-6.0) > height_of(-40.0) - height_of(-46.0));
        for db in [-65.0, -55.0, -33.0, -12.0, -3.0, 0.0, 4.0, 6.0] {
            assert!((db_at(height_of(db)) - db).abs() < 1e-3, "{db}");
        }
    }

    #[test]
    fn a_level_rises_at_once_and_falls_slowly() {
        let mut meter = Meter::default();
        meter.update(Some(reading(0.5, 0.5)), FRAME);
        let hit = meter.channels[0].peak.now;
        assert!((hit - to_db(0.5)).abs() < 1e-3);
        meter.update(Some(reading(0.0, 0.0)), 0.5);
        let fallen = hit - meter.channels[0].peak.now;
        assert!((fallen - 12.0).abs() < 1e-3, "fell {fallen}");
    }

    #[test]
    fn the_highest_level_holds_then_falls() {
        let mut meter = Meter::default();
        meter.update(Some(reading(0.5, 0.5)), FRAME);
        let top = meter.top();
        meter.update(None, 1.0);
        assert_eq!(meter.top(), top, "still held");
        meter.update(None, 1.0);
        meter.update(None, 0.5);
        assert!(meter.top() < top, "released");
    }

    #[test]
    fn the_average_follows_slowly_and_stays_below_a_short_hit() {
        let mut meter = Meter::default();
        meter.update(Some(reading(0.5, 0.5)), FRAME);
        let first = meter.channels[0].average();
        assert!(first < to_db(0.5) - 6.0, "{first}");
        for _ in 0..200 {
            meter.update(Some(reading(0.5, 0.5)), FRAME);
        }
        assert!((meter.channels[0].average() - to_db(0.5)).abs() < 0.01);
    }

    #[test]
    fn reduction_is_the_gap_across_the_limiter() {
        let mut meter = Meter::default();
        meter.update(Some(reading(0.5, 0.5)), FRAME);
        assert_eq!(meter.reduction.now, 0.0);
        let mut meter = Meter::default();
        meter.update(Some(reading(1.0, 0.5)), FRAME);
        assert!((meter.reduction.now - 6.02).abs() < 0.01);
        assert!((meter.reduction.top - 6.02).abs() < 0.01);
    }

    #[test]
    fn a_level_over_full_shows_on_the_bar_and_past_the_top() {
        let mut meter = Meter::default();
        meter.update(Some(reading(4.0, 0.99)), FRAME);
        assert!((meter.top() - 12.04).abs() < 0.01, "{}", meter.top());
        assert!((meter.reduction.top - 12.13).abs() < 0.01);
    }

    #[test]
    fn with_the_limiter_off_nothing_reads_as_reduced() {
        let mut meter = Meter::default();
        meter.update(Some(reading(2.0, 2.0)), FRAME);
        assert_eq!(meter.reduction.now, 0.0);
        assert!((meter.top() - 6.02).abs() < 0.01);
    }

    #[test]
    fn silence_reads_the_floor() {
        let mut meter = Meter::default();
        meter.update(None, FRAME);
        meter.update(Some(Reading::default()), FRAME);
        assert_eq!(meter.top(), FLOOR_DB);
        assert_eq!(meter.channels[0].average(), FLOOR_DB);
        assert_eq!(meter.reduction.now, 0.0);
    }

    /// At 120 BPM two bars are 4 seconds, so a second is 64 places.
    const SECOND: f32 = 1.0;

    #[test]
    fn a_reduction_history_sweeps_two_bars_and_writes_over_the_last_pass() {
        let mut history = ReductionHistory::default();
        assert_eq!(history.head(64), 0);
        // A second of 6 dB on slot 1, then a second of nothing.
        history.update(0, [0.0, 6.0, 0.0, 0.0], SECOND, 120.0);
        assert_eq!(
            history.head(64),
            16,
            "a quarter of the way, a second of four"
        );
        history.update(0, [0.0; INSERTS], SECOND, 120.0);
        assert_eq!(history.head(64), 32);
        // The first place held what it started with, and the next sixty-four the 6 dB.
        assert_eq!(history.column(1, 0, HISTORY_PLACES), 0.0);
        for place in 1..=64 {
            assert_eq!(history.column(1, place, HISTORY_PLACES), 6.0, "{place}");
        }
        assert_eq!(history.column(1, 65, HISTORY_PLACES), 0.0);
        assert_eq!(
            history.column(0, 10, HISTORY_PLACES),
            0.0,
            "only its own slot"
        );
        // A box narrower than the places shows the deepest of those each column covers.
        assert_eq!(history.column(1, 8, 64), 6.0);
        assert_eq!(
            history.column(1, 16, 64),
            6.0,
            "its first place is the last 6 dB"
        );
        assert_eq!(history.column(1, 17, 64), 0.0);
        // Within one place the deepest is kept.
        history.update(0, [0.0, 3.0, 0.0, 0.0], 0.001, 120.0);
        history.update(0, [0.0, 1.0, 0.0, 0.0], 0.001, 120.0);
        assert_eq!(history.column(1, 128, HISTORY_PLACES), 3.0);
        // Round past the start, the next pass writes over the last.
        history.update(0, [0.0, 2.0, 0.0, 0.0], 2.125, 120.0);
        assert_eq!(history.head(64), 2);
        assert_eq!(history.column(1, 4, HISTORY_PLACES), 2.0);
        assert_eq!(
            history.column(1, 40, HISTORY_PLACES),
            6.0,
            "not reached yet"
        );
        // At 60 BPM it goes half as fast.
        let mut slow = ReductionHistory::default();
        slow.update(0, [0.0; INSERTS], SECOND, 60.0);
        assert_eq!(slow.head(64), 8);
        // However long a frame took, it goes round once at most.
        slow.update(0, [5.0, 0.0, 0.0, 0.0], 100.0, 60.0);
        assert!((0..64).all(|column| slow.column(0, column, 64) == 5.0));
        // Another track's starts empty.
        slow.update(1, [0.0; INSERTS], 0.0, 60.0);
        assert!((0..64).all(|column| slow.column(0, column, 64) == 0.0));
        assert_eq!(slow.head(64), 0);
    }
}
