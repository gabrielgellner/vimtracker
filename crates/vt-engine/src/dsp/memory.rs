//! The long buffers some effects need, too big to keep for every slot of every track. The edit
//! thread makes them and sends them ahead of the version that needs them. The callback keeps them
//! in a pool, and slots take what they need from it, so an effect costs nothing until it's used.

use vt_model::constants::{MAX_TRACKS, MIN_MILLIBPM};
use vt_model::insert::{INSERTS, Insert};
use vt_model::lfo::DIVISION_TICKS;
use vt_model::song::Song;

use crate::dsp::chorus::Chorus;
use crate::dsp::delay::Delay;
use crate::dsp::flanger::Flanger;
use crate::dsp::limiter::Limiter;
use crate::dsp::reverb::Reverb;
use crate::dsp::tape::Tape;
use crate::engine::sample_of_tick;

/// How long a reverb has to stay below `SILENT` to be done, in seconds. It's longer than the way
/// round its loops and its longest pre-delay, so nothing louder is still on its way.
const REVERB_QUIET_SECONDS: f32 = 1.0;
/// How quiet an effect has to be to be done, about -100 dB.
pub const SILENT: f32 = 0.00001;

/// What one slot's effect keeps.
#[derive(Debug, Clone)]
pub enum Memory {
    Delay(Box<Delay>),
    Reverb(Box<Reverb>),
    Chorus(Box<Chorus>),
    Limit(Box<Limiter>),
    Flanger(Box<Flanger>),
    Tape(Box<Tape>),
}

impl Memory {
    /// A delay's, which can hold its longest time at the slowest tempo, and read around it.
    pub fn delay(rate: u32) -> Self {
        let longest = u64::from(DIVISION_TICKS[DIVISION_TICKS.len() - 1]);
        let frames = sample_of_tick(longest, rate, MIN_MILLIBPM) as usize + 8;
        Self::Delay(Box::new(Delay::new(frames, rate)))
    }

    /// A reverb's.
    pub fn reverb(rate: u32) -> Self {
        Self::Reverb(Box::new(Reverb::new(rate)))
    }

    /// A chorus's.
    pub fn chorus(rate: u32) -> Self {
        Self::Chorus(Box::new(Chorus::new(rate)))
    }

    /// A limiter's, which looks as far ahead at any rate.
    pub fn limit(rate: u32) -> Self {
        Self::Limit(Box::new(Limiter::new(rate)))
    }

    /// A flanger's.
    pub fn flanger(rate: u32) -> Self {
        Self::Flanger(Box::new(Flanger::new(rate)))
    }

    /// A tape's, with a line for its wow and flutter.
    pub fn tape(rate: u32) -> Self {
        Self::Tape(Box::new(Tape::new(rate)))
    }

    /// Feed one frame in and get what's wet out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        match self {
            Self::Delay(delay) => delay.process(input),
            Self::Reverb(reverb) => reverb.process(input),
            Self::Chorus(chorus) => chorus.process(input),
            Self::Limit(limiter) => limiter.process_alone(input),
            Self::Flanger(flanger) => flanger.process(input),
            Self::Tape(tape) => tape.process(input),
        }
    }

    /// How many quiet frames in a row mean nothing's left in it.
    pub fn quiet_frames(&self, rate: u32) -> u32 {
        match self {
            Self::Delay(delay) => delay.frames() as u32,
            Self::Reverb(_) => (REVERB_QUIET_SECONDS * rate as f32) as u32,
            Self::Chorus(chorus) => chorus.frames() as u32,
            Self::Limit(limiter) => limiter.frames() as u32,
            Self::Flanger(flanger) => flanger.frames() as u32,
            Self::Tape(tape) => tape.quiet_frames(),
        }
    }

    /// Take what's set now, at a tempo.
    #[inline]
    pub fn follow(&mut self, insert: Insert, frames_per_tick: f64, frames: u64) {
        match (self, insert) {
            (Self::Delay(delay), Insert::Delay(settings)) => {
                delay.follow(settings, frames_per_tick);
            }
            (Self::Reverb(reverb), Insert::Reverb(settings)) => reverb.follow(settings, frames),
            (Self::Chorus(chorus), Insert::Chorus(settings)) => chorus.follow(settings),
            (Self::Limit(limiter), Insert::Limit(settings)) => limiter.follow(settings),
            (Self::Flanger(flanger), Insert::Flanger(settings)) => {
                flanger.follow(settings, frames_per_tick);
            }
            (Self::Tape(tape), Insert::Tape(settings)) => tape.follow(settings),
            _ => {}
        }
    }

    /// Jump to what's set, as it's only just started.
    pub fn settle(&mut self) {
        match self {
            Self::Delay(delay) => delay.settle(),
            Self::Reverb(reverb) => reverb.settle(),
            Self::Chorus(chorus) => chorus.settle(),
            Self::Limit(limiter) => limiter.settle(),
            Self::Flanger(flanger) => flanger.settle(),
            Self::Tape(tape) => tape.settle(),
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Delay(delay) => delay.clear(),
            Self::Reverb(reverb) => reverb.clear(),
            Self::Chorus(chorus) => chorus.clear(),
            Self::Limit(limiter) => limiter.clear(),
            Self::Flanger(flanger) => flanger.clear(),
            Self::Tape(tape) => tape.clear(),
        }
    }
}

/// How many of each a song needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryCount {
    pub delay: u16,
    pub reverb: u16,
    pub chorus: u16,
    pub limit: u16,
    pub flanger: u16,
    pub tape: u16,
}

impl MemoryCount {
    /// What a song's slots need.
    pub fn of(song: &Song) -> Self {
        let mut count = Self::default();
        for insert in song.tracks.iter().flat_map(|track| track.inserts) {
            match insert {
                Insert::Delay(_) => count.delay += 1,
                Insert::Reverb(_) => count.reverb += 1,
                Insert::Chorus(_) => count.chorus += 1,
                Insert::Limit(_) => count.limit += 1,
                Insert::Flanger(_) => count.flanger += 1,
                Insert::Tape(_) => count.tape += 1,
                Insert::Empty
                | Insert::Distort(_)
                | Insert::Eq(_)
                | Insert::Compressor(_)
                | Insert::Phaser(_) => {}
            }
        }
        count
    }
}

/// What the callback holds that no slot is using. Nothing here allocates or frees.
#[derive(Debug)]
pub struct MemoryPool {
    delays: Vec<Memory>,
    reverbs: Vec<Memory>,
    choruses: Vec<Memory>,
    limiters: Vec<Memory>,
    flangers: Vec<Memory>,
    tapes: Vec<Memory>,
    /// How many to send back to be freed, once they're free.
    owed: MemoryCount,
}

/// The most the pool holds of each: every slot's, and as many more on their way to being freed.
const POOL_CAPACITY: usize = MAX_TRACKS * INSERTS * 2;

impl MemoryPool {
    /// A pool with what a song needs, for before audio starts.
    pub fn for_song(song: &Song, rate: u32) -> Self {
        let mut pool = Self {
            delays: Vec::with_capacity(POOL_CAPACITY),
            reverbs: Vec::with_capacity(POOL_CAPACITY),
            choruses: Vec::with_capacity(POOL_CAPACITY),
            limiters: Vec::with_capacity(POOL_CAPACITY),
            flangers: Vec::with_capacity(POOL_CAPACITY),
            tapes: Vec::with_capacity(POOL_CAPACITY),
            owed: MemoryCount::default(),
        };
        let needed = MemoryCount::of(song);
        for _ in 0..needed.delay {
            pool.put(Memory::delay(rate));
        }
        for _ in 0..needed.reverb {
            pool.put(Memory::reverb(rate));
        }
        for _ in 0..needed.chorus {
            pool.put(Memory::chorus(rate));
        }
        for _ in 0..needed.limit {
            pool.put(Memory::limit(rate));
        }
        for _ in 0..needed.flanger {
            pool.put(Memory::flanger(rate));
        }
        for _ in 0..needed.tape {
            pool.put(Memory::tape(rate));
        }
        pool
    }

    /// Keep what's no longer in use.
    pub fn put(&mut self, memory: Memory) {
        let held = match memory {
            Memory::Delay(_) => &mut self.delays,
            Memory::Reverb(_) => &mut self.reverbs,
            Memory::Chorus(_) => &mut self.choruses,
            Memory::Limit(_) => &mut self.limiters,
            Memory::Flanger(_) => &mut self.flangers,
            Memory::Tape(_) => &mut self.tapes,
        };
        // It can't be this full, as the edit thread sends what's needed. Letting it leak is better
        // than growing or freeing in the callback.
        if held.len() < held.capacity() {
            held.push(memory);
        } else {
            std::mem::forget(memory);
        }
    }

    /// Take what an effect needs, empty, if it needs something and there's one free.
    pub fn take(&mut self, insert: Insert) -> Option<Memory> {
        let held = match insert {
            Insert::Delay(_) => &mut self.delays,
            Insert::Reverb(_) => &mut self.reverbs,
            Insert::Chorus(_) => &mut self.choruses,
            Insert::Limit(_) => &mut self.limiters,
            Insert::Flanger(_) => &mut self.flangers,
            Insert::Tape(_) => &mut self.tapes,
            Insert::Empty
            | Insert::Distort(_)
            | Insert::Eq(_)
            | Insert::Compressor(_)
            | Insert::Phaser(_) => {
                return None;
            }
        };
        let mut memory = held.pop()?;
        memory.clear();
        Some(memory)
    }

    /// Note that the edit thread wants this many back.
    pub fn owe(&mut self, count: MemoryCount) {
        self.owed.delay += count.delay;
        self.owed.reverb += count.reverb;
        self.owed.chorus += count.chorus;
        self.owed.limit += count.limit;
        self.owed.flanger += count.flanger;
        self.owed.tape += count.tape;
    }

    /// The next to send back to be freed, if any is owed and free.
    pub fn next_owed(&mut self) -> Option<Memory> {
        if self.owed.delay > 0
            && let Some(memory) = self.delays.pop()
        {
            self.owed.delay -= 1;
            return Some(memory);
        }
        if self.owed.reverb > 0
            && let Some(memory) = self.reverbs.pop()
        {
            self.owed.reverb -= 1;
            return Some(memory);
        }
        if self.owed.chorus > 0
            && let Some(memory) = self.choruses.pop()
        {
            self.owed.chorus -= 1;
            return Some(memory);
        }
        if self.owed.limit > 0
            && let Some(memory) = self.limiters.pop()
        {
            self.owed.limit -= 1;
            return Some(memory);
        }
        if self.owed.flanger > 0
            && let Some(memory) = self.flangers.pop()
        {
            self.owed.flanger -= 1;
            return Some(memory);
        }
        if self.owed.tape > 0
            && let Some(memory) = self.tapes.pop()
        {
            self.owed.tape -= 1;
            return Some(memory);
        }
        None
    }

    /// How many are free.
    pub fn spare(&self) -> usize {
        self.delays.len()
            + self.reverbs.len()
            + self.choruses.len()
            + self.limiters.len()
            + self.flangers.len()
            + self.tapes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::insert::Delay as DelaySettings;

    const DELAY: Insert = Insert::Delay(DelaySettings::DEFAULT);

    #[test]
    fn a_delay_holds_its_time_at_the_slowest_tempo_at_any_rate() {
        for rate in [44_100, 48_000, 96_000] {
            // A whole bar, at 20 BPM.
            let frames = sample_of_tick(3840, rate, MIN_MILLIBPM) as usize;
            let mut memory = Memory::delay(rate);
            let settings = DelaySettings {
                time: 17,
                ..DelaySettings::DEFAULT
            };
            memory.follow(Insert::Delay(settings), frames as f64 / 3840.0, 0);
            memory.settle();
            assert_eq!(memory.process([1.0, 1.0]), [0.0, 0.0]);
            for _ in 1..frames {
                assert_eq!(memory.process([0.0, 0.0]), [0.0, 0.0]);
            }
            assert!(memory.process([0.0, 0.0])[0] > 0.1, "{rate}");
        }
    }

    #[test]
    fn what_comes_out_of_the_pool_is_empty() {
        let mut pool = MemoryPool::for_song(&Song::new(1), 48_000);
        let mut memory = pool.take(DELAY).unwrap();
        memory.process([1.0, 1.0]);
        pool.put(memory);
        let mut memory = pool.take(DELAY).unwrap();
        for _ in 0..20 {
            assert_eq!(memory.process([0.0, 0.0]), [0.0, 0.0]);
        }
    }

    #[test]
    fn what_is_owed_goes_back_only_once_it_is_free() {
        let mut pool = MemoryPool::for_song(&Song::new(1), 48_000);
        let held = pool.take(DELAY).unwrap();
        pool.owe(MemoryCount {
            delay: 1,
            ..MemoryCount::default()
        });
        assert!(pool.next_owed().is_none(), "a slot still has it");
        pool.put(held);
        assert!(pool.next_owed().is_some());
        assert!(pool.next_owed().is_none());
    }
}
