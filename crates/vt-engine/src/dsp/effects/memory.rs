//! The long buffers some effects need, too big to keep for every slot of every track. The edit
//! thread makes them and sends them ahead of the version that needs them. The callback keeps them
//! in a pool, and slots take what they need from it, so an effect costs nothing until it's used.

use vt_model::constants::{MAX_TRACKS, MIN_MILLIBPM};
use vt_model::insert::{INSERTS, Insert};
use vt_model::lfo::DIVISION_TICKS;
use vt_model::song::Song;

use crate::dsp::dynamics::limiter::Limiter;
use crate::dsp::effects::chorus::Chorus;
use crate::dsp::effects::delay::Delay;
use crate::dsp::effects::flanger::Flanger;
use crate::dsp::effects::freq_shifter::FreqShifter;
use crate::dsp::effects::granular::Granular;
use crate::dsp::effects::pitch_shifter::PitchShifter;
use crate::dsp::effects::plate::Plate;
use crate::dsp::effects::resonator::Resonator;
use crate::dsp::effects::reverb::Reverb;
use crate::dsp::effects::space::Space;
use crate::dsp::effects::spring::Spring;
use crate::dsp::effects::tape::Tape;
use crate::dsp::effects::warps::Warps;
use crate::dsp::filters::comb::Comb;
use crate::engine::sample_of_tick;

/// How long a reverb or plate has to stay below `SILENT` to be done, in seconds. It's longer than the way
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
    Plate(Box<Plate>),
    Pitch(Box<PitchShifter>),
    Grains(Box<Granular>),
    Warps(Box<Warps>),
    Reso(Box<Resonator>),
    Comb(Box<Comb>),
    FreqShift(Box<FreqShifter>),
    Spring(Box<Spring>),
    Space(Box<Space>),
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

    /// A plate's, long enough for the biggest.
    pub fn plate(rate: u32) -> Self {
        Self::Plate(Box::new(Plate::new(rate)))
    }

    /// A pitch shifter's, long enough for its longest window.
    pub fn pitch(rate: u32) -> Self {
        Self::Pitch(Box::new(PitchShifter::new(rate)))
    }

    /// A granular effect's, which records Beads' four seconds.
    pub fn grains(rate: u32) -> Self {
        Self::Grains(Box::new(Granular::new(rate)))
    }

    /// Warps', for its vocoder's filter bank.
    pub fn warps(rate: u32) -> Self {
        Self::Warps(Box::new(Warps::new(rate)))
    }

    /// A resonator's, for its modes and their gains.
    pub fn reso(rate: u32) -> Self {
        Self::Reso(Box::new(Resonator::new(rate)))
    }

    /// A comb's, for its lines.
    pub fn comb(rate: u32) -> Self {
        Self::Comb(Box::new(Comb::new(rate)))
    }

    /// A frequency shifter's, with lines for its longest delay.
    pub fn freq_shift(rate: u32) -> Self {
        Self::FreqShift(Box::new(FreqShifter::new(rate)))
    }

    /// A spring reverb's, long enough for the biggest.
    pub fn spring(rate: u32) -> Self {
        Self::Spring(Box::new(Spring::new(rate)))
    }

    /// A space's, long enough for the biggest tank.
    pub fn space(rate: u32) -> Self {
        Self::Space(Box::new(Space::new(rate)))
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
            Self::Plate(plate) => plate.process(input),
            Self::Pitch(shifter) => shifter.process(input),
            Self::Grains(granular) => granular.process(input),
            Self::Warps(warps) => warps.process_alone(input),
            Self::Reso(resonator) => resonator.process_alone(input),
            Self::Comb(comb) => comb.process_alone(input),
            Self::FreqShift(shifter) => shifter.process_alone(input),
            Self::Spring(spring) => spring.process(input),
            Self::Space(space) => space.process(input),
        }
    }

    /// How many quiet frames in a row mean nothing's left in it.
    pub fn quiet_frames(&self, rate: u32) -> u32 {
        match self {
            Self::Delay(delay) => delay.reach() as u32,
            Self::Reverb(_) | Self::Plate(_) | Self::Spring(_) | Self::Space(_) => {
                (REVERB_QUIET_SECONDS * rate as f32) as u32
            }
            Self::Chorus(chorus) => chorus.frames() as u32,
            Self::Limit(limiter) => limiter.frames() as u32,
            Self::Flanger(flanger) => flanger.frames() as u32,
            Self::Tape(tape) => tape.quiet_frames(),
            Self::Pitch(shifter) => shifter.frames() as u32,
            Self::Grains(granular) => granular.quiet_frames(),
            Self::Warps(warps) => warps.quiet_frames(),
            Self::Reso(resonator) => resonator.quiet_frames(),
            Self::Comb(comb) => comb.quiet_frames(),
            Self::FreqShift(shifter) => shifter.frames() as u32,
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
            (Self::Plate(plate), Insert::Plate(settings)) => plate.follow(settings, frames),
            (Self::Pitch(shifter), Insert::Pitch(settings)) => shifter.follow(settings),
            (Self::Grains(granular), Insert::Grains(settings)) => granular.follow(settings),
            (Self::Warps(warps), Insert::Warps(settings)) => warps.follow(settings),
            (Self::Reso(resonator), Insert::Reso(settings)) => resonator.follow(settings),
            (Self::Comb(comb), Insert::Comb(settings)) => comb.follow(settings),
            (Self::FreqShift(shifter), Insert::FreqShift(settings)) => shifter.follow(settings),
            (Self::Spring(spring), Insert::Spring(settings)) => spring.follow(settings),
            (Self::Space(space), Insert::Space(settings)) => space.follow(settings),
            _ => {}
        }
    }

    /// Whether it's about to play something of its own with nothing going in, as a spring that's
    /// just been knocked does, so it has to be woken.
    pub fn waking(&self) -> bool {
        matches!(self, Self::Spring(spring) if spring.knocking())
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
            Self::Plate(plate) => plate.settle(),
            Self::Pitch(shifter) => shifter.settle(),
            Self::Grains(granular) => granular.settle(),
            Self::Warps(warps) => warps.settle(),
            Self::Reso(resonator) => resonator.settle(),
            Self::Comb(comb) => comb.settle(),
            Self::FreqShift(shifter) => shifter.settle(),
            Self::Spring(spring) => spring.settle(),
            Self::Space(space) => space.settle(),
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
            Self::Plate(plate) => plate.clear(),
            Self::Pitch(shifter) => shifter.clear(),
            Self::Grains(granular) => granular.clear(),
            Self::Warps(warps) => warps.clear(),
            Self::Reso(resonator) => resonator.clear(),
            Self::Comb(comb) => comb.clear(),
            Self::FreqShift(shifter) => shifter.clear(),
            Self::Spring(spring) => spring.clear(),
            Self::Space(space) => space.clear(),
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
    pub plate: u16,
    pub pitch: u16,
    pub grains: u16,
    pub warps: u16,
    pub reso: u16,
    pub comb: u16,
    pub freq_shift: u16,
    pub spring: u16,
    pub space: u16,
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
                Insert::Plate(_) => count.plate += 1,
                Insert::Pitch(_) => count.pitch += 1,
                Insert::Grains(_) => count.grains += 1,
                Insert::Warps(_) => count.warps += 1,
                Insert::Reso(_) => count.reso += 1,
                Insert::Comb(_) => count.comb += 1,
                Insert::FreqShift(_) => count.freq_shift += 1,
                Insert::Spring(_) => count.spring += 1,
                Insert::Space(_) => count.space += 1,
                Insert::Empty
                | Insert::Distort(_)
                | Insert::Exciter(_)
                | Insert::Eq(_)
                | Insert::Compressor(_)
                | Insert::Lpg(_)
                | Insert::Phaser(_)
                | Insert::Vowel(_) => {}
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
    plates: Vec<Memory>,
    pitches: Vec<Memory>,
    grains: Vec<Memory>,
    warps: Vec<Memory>,
    reso: Vec<Memory>,
    comb: Vec<Memory>,
    freq_shift: Vec<Memory>,
    springs: Vec<Memory>,
    spaces: Vec<Memory>,
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
            plates: Vec::with_capacity(POOL_CAPACITY),
            pitches: Vec::with_capacity(POOL_CAPACITY),
            grains: Vec::with_capacity(POOL_CAPACITY),
            warps: Vec::with_capacity(POOL_CAPACITY),
            reso: Vec::with_capacity(POOL_CAPACITY),
            comb: Vec::with_capacity(POOL_CAPACITY),
            freq_shift: Vec::with_capacity(POOL_CAPACITY),
            springs: Vec::with_capacity(POOL_CAPACITY),
            spaces: Vec::with_capacity(POOL_CAPACITY),
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
        for _ in 0..needed.plate {
            pool.put(Memory::plate(rate));
        }
        for _ in 0..needed.pitch {
            pool.put(Memory::pitch(rate));
        }
        for _ in 0..needed.grains {
            pool.put(Memory::grains(rate));
        }
        for _ in 0..needed.warps {
            pool.put(Memory::warps(rate));
        }
        for _ in 0..needed.reso {
            pool.put(Memory::reso(rate));
        }
        for _ in 0..needed.comb {
            pool.put(Memory::comb(rate));
        }
        for _ in 0..needed.freq_shift {
            pool.put(Memory::freq_shift(rate));
        }
        for _ in 0..needed.spring {
            pool.put(Memory::spring(rate));
        }
        for _ in 0..needed.space {
            pool.put(Memory::space(rate));
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
            Memory::Plate(_) => &mut self.plates,
            Memory::Pitch(_) => &mut self.pitches,
            Memory::Grains(_) => &mut self.grains,
            Memory::Warps(_) => &mut self.warps,
            Memory::Reso(_) => &mut self.reso,
            Memory::Comb(_) => &mut self.comb,
            Memory::FreqShift(_) => &mut self.freq_shift,
            Memory::Spring(_) => &mut self.springs,
            Memory::Space(_) => &mut self.spaces,
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
            Insert::Plate(_) => &mut self.plates,
            Insert::Pitch(_) => &mut self.pitches,
            Insert::Grains(_) => &mut self.grains,
            Insert::Warps(_) => &mut self.warps,
            Insert::Reso(_) => &mut self.reso,
            Insert::Comb(_) => &mut self.comb,
            Insert::FreqShift(_) => &mut self.freq_shift,
            Insert::Spring(_) => &mut self.springs,
            Insert::Space(_) => &mut self.spaces,
            Insert::Empty
            | Insert::Distort(_)
            | Insert::Exciter(_)
            | Insert::Eq(_)
            | Insert::Compressor(_)
            | Insert::Lpg(_)
            | Insert::Phaser(_)
            | Insert::Vowel(_) => {
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
        self.owed.plate += count.plate;
        self.owed.pitch += count.pitch;
        self.owed.grains += count.grains;
        self.owed.warps += count.warps;
        self.owed.reso += count.reso;
        self.owed.comb += count.comb;
        self.owed.freq_shift += count.freq_shift;
        self.owed.spring += count.spring;
        self.owed.space += count.space;
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
        if self.owed.plate > 0
            && let Some(memory) = self.plates.pop()
        {
            self.owed.plate -= 1;
            return Some(memory);
        }
        if self.owed.pitch > 0
            && let Some(memory) = self.pitches.pop()
        {
            self.owed.pitch -= 1;
            return Some(memory);
        }
        if self.owed.grains > 0
            && let Some(memory) = self.grains.pop()
        {
            self.owed.grains -= 1;
            return Some(memory);
        }
        if self.owed.warps > 0
            && let Some(memory) = self.warps.pop()
        {
            self.owed.warps -= 1;
            return Some(memory);
        }
        if self.owed.reso > 0
            && let Some(memory) = self.reso.pop()
        {
            self.owed.reso -= 1;
            return Some(memory);
        }
        if self.owed.comb > 0
            && let Some(memory) = self.comb.pop()
        {
            self.owed.comb -= 1;
            return Some(memory);
        }
        if self.owed.freq_shift > 0
            && let Some(memory) = self.freq_shift.pop()
        {
            self.owed.freq_shift -= 1;
            return Some(memory);
        }
        if self.owed.spring > 0
            && let Some(memory) = self.springs.pop()
        {
            self.owed.spring -= 1;
            return Some(memory);
        }
        if self.owed.space > 0
            && let Some(memory) = self.spaces.pop()
        {
            self.owed.space -= 1;
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
            + self.plates.len()
            + self.pitches.len()
            + self.grains.len()
            + self.warps.len()
            + self.reso.len()
            + self.comb.len()
            + self.freq_shift.len()
            + self.springs.len()
            + self.spaces.len()
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
