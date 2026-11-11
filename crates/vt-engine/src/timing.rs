//! Where the engine's time goes, a track and a slot at a time, for `vimtracker bench --tracks`.
//!
//! It's off unless asked for, when every lap is a check of an empty option. On, each lap reads the
//! clock once and gives the time since the last one to a part, so the parts add up to the whole.
//! Each read costs a little itself, which [`clock_cost`] measures for the bench to take off.

use std::time::{Duration, Instant};

use vt_model::constants::MAX_TRACKS;
use vt_model::insert::INSERTS;

/// What a stretch of the engine's time went on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// Working out steps, events and LFOs between runs of frames.
    Sequencing,
    /// Finding which tracks play, and adding up the mix, around the tracks themselves.
    Mixing,
    /// A track's voices: its own, its tail and any preview through it.
    Voice(usize),
    /// A track's filter, slots, level, pan and sends, or all of a group's, return's or the
    /// master's work.
    Shaping(usize),
    /// The master's limiter, after the master's slots.
    Limiting,
}

/// The time each part took, since timing began.
#[derive(Debug, Clone)]
pub struct PartTimes {
    pub sequencing: Duration,
    pub mixing: Duration,
    pub limiting: Duration,
    pub voices: [Duration; MAX_TRACKS],
    /// Includes the track's slots, which are also counted on their own.
    pub shaping: [Duration; MAX_TRACKS],
    pub slots: [[Duration; INSERTS]; MAX_TRACKS],
    /// The clock reads behind each figure, for taking their own cost off.
    pub sequencing_laps: u64,
    pub mixing_laps: u64,
    pub limiting_laps: u64,
    pub voice_laps: [u64; MAX_TRACKS],
    pub shaping_laps: [u64; MAX_TRACKS],
    pub slot_laps: [[u64; INSERTS]; MAX_TRACKS],
}

impl Default for PartTimes {
    fn default() -> Self {
        Self {
            sequencing: Duration::ZERO,
            mixing: Duration::ZERO,
            limiting: Duration::ZERO,
            voices: [Duration::ZERO; MAX_TRACKS],
            shaping: [Duration::ZERO; MAX_TRACKS],
            slots: [[Duration::ZERO; INSERTS]; MAX_TRACKS],
            sequencing_laps: 0,
            mixing_laps: 0,
            limiting_laps: 0,
            voice_laps: [0; MAX_TRACKS],
            shaping_laps: [0; MAX_TRACKS],
            slot_laps: [[0; INSERTS]; MAX_TRACKS],
        }
    }
}

/// The engine's clock for its parts. It reads nothing while off.
#[derive(Debug, Default)]
pub struct Stopwatch {
    running: Option<Box<Running>>,
}

#[derive(Debug)]
struct Running {
    last: Instant,
    times: PartTimes,
}

impl Stopwatch {
    /// Start timing from nothing. This allocates, so it's done before audio starts.
    pub fn start(&mut self) {
        self.running = Some(Box::new(Running {
            last: Instant::now(),
            times: PartTimes::default(),
        }));
    }

    /// The times so far, if it's running.
    pub fn times(&self) -> Option<&PartTimes> {
        self.running.as_ref().map(|running| &running.times)
    }

    /// Start the next lap from now, giving the time since the last to nothing.
    #[inline]
    pub fn restart_lap(&mut self) {
        if let Some(running) = &mut self.running {
            running.last = Instant::now();
        }
    }

    /// Give the time since the last lap to a part.
    #[inline]
    pub fn lap(&mut self, part: Part) {
        let Some(running) = &mut self.running else {
            return;
        };
        let now = Instant::now();
        let spent = now - running.last;
        running.last = now;
        let times = &mut running.times;
        let (total, laps) = match part {
            Part::Sequencing => (&mut times.sequencing, &mut times.sequencing_laps),
            Part::Mixing => (&mut times.mixing, &mut times.mixing_laps),
            Part::Limiting => (&mut times.limiting, &mut times.limiting_laps),
            Part::Voice(track) => (&mut times.voices[track], &mut times.voice_laps[track]),
            Part::Shaping(track) => (&mut times.shaping[track], &mut times.shaping_laps[track]),
        };
        *total += spent;
        *laps += 1;
    }

    /// Add a slot's time, which a track times on its own as it runs its slots.
    pub fn add_slots(&mut self, track: usize, slots: &mut SlotTimes) {
        if let Some(running) = &mut self.running {
            for slot in 0..INSERTS {
                running.times.slots[track][slot] += std::mem::take(&mut slots.times[slot]);
                running.times.slot_laps[track][slot] += std::mem::take(&mut slots.laps[slot]);
            }
        }
    }
}

/// A track's own count of the time each of its slots takes, kept while the engine's stopwatch runs.
#[derive(Debug, Clone, Default)]
pub struct SlotTimes {
    on: bool,
    times: [Duration; INSERTS],
    laps: [u64; INSERTS],
}

impl SlotTimes {
    /// Timing from nothing.
    pub fn running() -> Self {
        Self {
            on: true,
            ..Self::default()
        }
    }

    /// Run a slot, timing it if timing is on and the slot has an effect in it.
    #[inline]
    pub fn time<T>(&mut self, slot: usize, effect: bool, run: impl FnOnce() -> T) -> T {
        if !self.on || !effect {
            return run();
        }
        let started = Instant::now();
        let result = run();
        self.times[slot] += started.elapsed();
        self.laps[slot] += 1;
        result
    }
}

/// What one read of the clock costs, on average, for taking off what timing adds.
pub fn clock_cost() -> Duration {
    const READS: u32 = 1_000_000;
    let started = Instant::now();
    let mut last = started;
    for _ in 0..READS {
        last = std::hint::black_box(Instant::now());
    }
    (last - started) / READS
}
