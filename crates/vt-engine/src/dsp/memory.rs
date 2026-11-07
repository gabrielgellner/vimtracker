//! The long buffers some effects need, too big to keep for every slot of every track. The edit
//! thread makes them and sends them ahead of the version that needs them. The callback keeps them
//! in a pool, and slots take what they need from it, so an effect costs nothing until it's used.

use vt_model::constants::{MAX_TRACKS, MIN_MILLIBPM};
use vt_model::insert::{INSERTS, Insert};
use vt_model::song::Song;

use crate::dsp::delay::Delay;
use crate::dsp::reverb::Reverb;
use crate::engine::{DELAY_TICKS, sample_of_tick};

/// How long a reverb has to stay below `SILENT` to be done, in seconds. It's longer than any of its
/// lines, so nothing louder is still going round.
const REVERB_QUIET_SECONDS: f32 = 0.1;
/// How quiet an effect has to be to be done, about -100 dB.
pub const SILENT: f32 = 0.00001;

/// What one slot's effect keeps.
#[derive(Debug, Clone)]
pub enum Memory {
    Delay(Box<Delay>),
    Reverb(Box<Reverb>),
}

impl Memory {
    /// A delay's, which can hold its time at the slowest tempo.
    pub fn delay(rate: u32) -> Self {
        let frames = sample_of_tick(DELAY_TICKS, rate, MIN_MILLIBPM) as usize + 1;
        Self::Delay(Box::new(Delay::new(frames)))
    }

    /// A reverb's.
    pub fn reverb(rate: u32) -> Self {
        Self::Reverb(Box::new(Reverb::new(rate)))
    }

    /// Feed one frame in and get what's wet out.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], delay_frames: usize) -> [f32; 2] {
        match self {
            Self::Delay(delay) => delay.process(input, delay_frames),
            Self::Reverb(reverb) => reverb.process(input),
        }
    }

    /// How many quiet frames in a row mean nothing's left in it.
    pub fn quiet_frames(&self, delay_frames: usize, rate: u32) -> u32 {
        match self {
            Self::Delay(_) => delay_frames as u32,
            Self::Reverb(_) => (REVERB_QUIET_SECONDS * rate as f32) as u32,
        }
    }

    fn clear(&mut self) {
        match self {
            Self::Delay(delay) => delay.clear(),
            Self::Reverb(reverb) => reverb.clear(),
        }
    }
}

/// How many of each a song needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryCount {
    pub delay: u16,
    pub reverb: u16,
}

impl MemoryCount {
    /// What a song's slots need.
    pub fn of(song: &Song) -> Self {
        let mut count = Self::default();
        for insert in song.tracks.iter().flat_map(|track| track.inserts) {
            match insert {
                Insert::Delay(_) => count.delay += 1,
                Insert::Reverb(_) => count.reverb += 1,
                Insert::Empty | Insert::Distort(_) => {}
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
            owed: MemoryCount::default(),
        };
        let needed = MemoryCount::of(song);
        for _ in 0..needed.delay {
            pool.put(Memory::delay(rate));
        }
        for _ in 0..needed.reverb {
            pool.put(Memory::reverb(rate));
        }
        pool
    }

    /// Keep what's no longer in use.
    pub fn put(&mut self, memory: Memory) {
        let held = match memory {
            Memory::Delay(_) => &mut self.delays,
            Memory::Reverb(_) => &mut self.reverbs,
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
            Insert::Empty | Insert::Distort(_) => return None,
        };
        let mut memory = held.pop()?;
        memory.clear();
        Some(memory)
    }

    /// Note that the edit thread wants this many back.
    pub fn owe(&mut self, count: MemoryCount) {
        self.owed.delay += count.delay;
        self.owed.reverb += count.reverb;
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
        None
    }

    /// How many are free.
    pub fn spare(&self) -> usize {
        self.delays.len() + self.reverbs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::insert::Delay as DelaySettings;

    const DELAY: Insert = Insert::Delay(DelaySettings {
        mix: 0xFF,
        gain: 0xE0,
    });

    #[test]
    fn a_delay_holds_its_time_at_the_slowest_tempo_at_any_rate() {
        for rate in [44_100, 48_000, 96_000] {
            let frames = sample_of_tick(DELAY_TICKS, rate, MIN_MILLIBPM) as usize;
            let mut memory = Memory::delay(rate);
            let hit = memory.process([1.0, 1.0], frames);
            assert_eq!(hit, [0.0, 0.0]);
            for _ in 1..frames {
                assert_eq!(memory.process([0.0, 0.0], frames), [0.0, 0.0]);
            }
            assert!(memory.process([0.0, 0.0], frames)[0] > 0.1, "{rate}");
        }
    }

    #[test]
    fn what_comes_out_of_the_pool_is_empty() {
        let mut pool = MemoryPool::for_song(&Song::new(1), 48_000);
        let mut memory = pool.take(DELAY).unwrap();
        memory.process([1.0, 1.0], 10);
        pool.put(memory);
        let mut memory = pool.take(DELAY).unwrap();
        for _ in 0..20 {
            assert_eq!(memory.process([0.0, 0.0], 10), [0.0, 0.0]);
        }
    }

    #[test]
    fn what_is_owed_goes_back_only_once_it_is_free() {
        let mut pool = MemoryPool::for_song(&Song::new(1), 48_000);
        let held = pool.take(DELAY).unwrap();
        pool.owe(MemoryCount {
            delay: 1,
            reverb: 0,
        });
        assert!(pool.next_owed().is_none(), "a slot still has it");
        pool.put(held);
        assert!(pool.next_owed().is_some());
        assert!(pool.next_owed().is_none());
    }
}
