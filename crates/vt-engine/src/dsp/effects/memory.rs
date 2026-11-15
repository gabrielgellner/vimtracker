//! The long buffers some effects need, too big to keep for every slot of every track. The edit
//! thread makes them and sends them ahead of the version that needs them. The callback keeps them
//! in a pool, and slots take what they need from it, so an effect costs nothing until it's used.

use std::ops::{Index, IndexMut};

use vt_model::constants::MAX_TRACKS;
use vt_model::insert::{EffectKind, INSERTS, Insert};
use vt_model::song::Song;

use crate::dsp::dynamics::limiter::Limiter;
use crate::dsp::effects::beads::Beads;
use crate::dsp::effects::chorus::Chorus;
use crate::dsp::effects::clouds::Clouds;
use crate::dsp::effects::delay::Delay;
use crate::dsp::effects::echo::Echo;
use crate::dsp::effects::flanger::Flanger;
use crate::dsp::effects::freq_shifter::FreqShifter;
use crate::dsp::effects::granular::Granular;
use crate::dsp::effects::pitch_shifter::PitchShifter;
use crate::dsp::effects::plate::Plate;
use crate::dsp::effects::resonator::Resonator;
use crate::dsp::effects::reverb::Reverb;
use crate::dsp::effects::rotary::Rotary;
use crate::dsp::effects::slot::{Rung, SlotEffect};
use crate::dsp::effects::space::Space;
use crate::dsp::effects::spring::Spring;
use crate::dsp::effects::stutter::Stutter;
use crate::dsp::effects::tape::Tape;
use crate::dsp::effects::warps::Warps;
use crate::dsp::filters::comb::Comb;
use crate::rng::Pcg32;

/// Where the song is, for the effects that keep time with it: its tick, whether it's playing,
/// and how many frames a tick takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongPlace {
    pub tick: f64,
    pub playing: bool,
    pub frames_per_tick: f64,
}

impl SongPlace {
    /// A song that isn't playing, for an effect that's only just started.
    pub const STILL: Self = Self {
        tick: 0.0,
        playing: false,
        frames_per_tick: 1.0,
    };
}

/// How long a reverb, plate, spring or space has to stay below `SILENT` to be done, in seconds.
/// It's longer than the way round its loops and its longest pre-delay, so nothing louder is still
/// on its way.
pub(crate) const REVERB_QUIET_SECONDS: f32 = 1.0;
/// How quiet an effect has to be to be done, about -100 dB.
pub const SILENT: f32 = 0.00001;

/// Makes [`Memory`], what one slot's effect keeps, from one list of the effects that keep a long
/// buffer, with everything that hands a call on to one of them by `match` (decision 340). Each
/// entry is the effect's name, which its [`EffectKind`], its `Insert` and its `Memory` share, and
/// its DSP, which implements [`SlotEffect`]. The slot plays the first list's through its `ring`
/// ([`Rung`]), and the second's itself, with its note, its glide or another track.
macro_rules! memories {
    (
        rung: $($rung:ident($rung_dsp:ty)),+ $(,)?;
        played_by_slot: $($own:ident($own_dsp:ty)),+ $(,)?;
    ) => {
        memories!(@made [$($rung($rung_dsp),)+ $($own($own_dsp),)+] [$($rung),+] [$($own),+]);
    };
    (@made [$($kind:ident($dsp:ty),)+] [$($rung:ident),+] [$($own:ident),+]) => {
        /// What one slot's effect keeps.
        #[derive(Debug, Clone)]
        pub enum Memory {
            $($kind(Box<$dsp>),)+
        }

        /// Whether each effect keeps a long buffer, which it takes from the pool, by
        /// [`EffectKind`].
        pub const BUFFERED: [bool; EffectKind::COUNT] = {
            let mut buffered = [false; EffectKind::COUNT];
            $(buffered[EffectKind::$kind as usize] = true;)+
            buffered
        };

        impl Memory {
            /// What an effect keeps, made on the edit thread, if it keeps a long buffer.
            pub fn new(kind: EffectKind, rate: u32) -> Option<Self> {
                Some(match kind {
                    $(EffectKind::$kind => Self::$kind(Box::new(<$dsp as SlotEffect>::new(rate))),)+
                    _ => return None,
                })
            }

            /// Which effect's it is.
            pub fn kind(&self) -> EffectKind {
                match self {
                    $(Self::$kind(_) => EffectKind::$kind,)+
                }
            }

            /// Feed one frame in and get what's wet out.
            #[inline]
            pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
                match self {
                    $(Self::$rung(dsp) => Rung::process(&mut **dsp, input),)+
                    // The slot plays these itself, so they never come here. Were one to, it's
                    // heard as silence rather than stopping the callback.
                    $(Self::$own(_))|+ => {
                        debug_assert!(false, "{:?} is played by its slot", self.kind());
                        [0.0; 2]
                    }
                }
            }

            /// How many quiet frames in a row mean nothing's left in it.
            pub fn quiet_frames(&self, rate: u32) -> u32 {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::quiet_frames(&**dsp, rate),)+
                }
            }

            /// Take what's set now, where the song is and at its tempo.
            #[inline]
            pub fn follow(&mut self, insert: Insert, song: SongPlace, frames: u64) {
                match (self, insert) {
                    $((Self::$kind(dsp), Insert::$kind(settings)) => {
                        SlotEffect::follow(&mut **dsp, settings, song, frames);
                    })+
                    _ => {}
                }
            }

            /// Move what goes by the frame rather than by what it's done to where it is this many
            /// frames in, as taking its settings does: a reverb's and a plate's wobbles, Clouds'
            /// and Beads' reverbs', and the song's steps Beads follows.
            pub fn keep_time(&mut self, frames: u64, song: SongPlace) {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::keep_time(&mut **dsp, frames, song),)+
                }
            }

            /// Give what draws random numbers its own generator, from the track's: Clouds' and
            /// Beads' grains, and the Space Echo's wobble.
            pub fn seed(&mut self, rng: Pcg32) {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::seed(&mut **dsp, rng),)+
                }
            }

            /// Whether it's about to play something of its own with nothing going in, as a spring
            /// that's just been knocked does, or a stutter that's just been triggered, so it has to
            /// be woken.
            pub fn waking(&self) -> bool {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::waking(&**dsp),)+
                }
            }

            /// A step has switched its trigger on, so a stutter starts again, whether or not it
            /// was on.
            pub fn strike(&mut self) {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::strike(&mut **dsp),)+
                }
            }

            /// Jump to what's set, as it's only just started.
            pub fn settle(&mut self) {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::settle(&mut **dsp),)+
                }
            }

            fn clear(&mut self) {
                match self {
                    $(Self::$kind(dsp) => SlotEffect::clear(&mut **dsp),)+
                }
            }
        }
    };
}

memories! {
    rung: Delay(Delay), Reverb(Reverb), Chorus(Chorus), Flanger(Flanger), Tape(Tape),
        Plate(Plate), Pitch(PitchShifter), Grains(Granular), Spring(Spring), Space(Space),
        Rotary(Rotary), Stutter(Stutter), Clouds(Clouds), Beads(Beads), Echo(Echo);
    played_by_slot: Limit(Limiter), Warps(Warps), Reso(Resonator), Comb(Comb),
        FreqShift(FreqShifter);
}

impl Memory {
    /// Whether an effect keeps a long buffer, which it takes from the pool.
    pub fn kept_by(insert: Insert) -> bool {
        insert.kind().is_some_and(|kind| BUFFERED[kind as usize])
    }
}

/// How many of each a song needs, by [`EffectKind`]: none of what keeps no long buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryCount(pub [u16; EffectKind::COUNT]);

impl MemoryCount {
    /// What a song's slots need.
    pub fn of(song: &Song) -> Self {
        let mut count = Self::default();
        for insert in song.tracks.iter().flat_map(|track| track.inserts) {
            if Memory::kept_by(insert)
                && let Some(kind) = insert.kind()
            {
                count[kind] += 1;
            }
        }
        count
    }
}

impl Index<EffectKind> for MemoryCount {
    type Output = u16;

    fn index(&self, kind: EffectKind) -> &u16 {
        &self.0[kind as usize]
    }
}

impl IndexMut<EffectKind> for MemoryCount {
    fn index_mut(&mut self, kind: EffectKind) -> &mut u16 {
        &mut self.0[kind as usize]
    }
}

/// What the callback holds that no slot is using, by [`EffectKind`]. Nothing here allocates or
/// frees.
#[derive(Debug)]
pub struct MemoryPool {
    free: [Vec<Memory>; EffectKind::COUNT],
    /// How many to send back to be freed, once they're free.
    owed: MemoryCount,
}

/// The most the pool holds of each: every slot's, and as many more on their way to being freed.
const POOL_CAPACITY: usize = MAX_TRACKS * INSERTS * 2;

impl MemoryPool {
    /// A pool with what a song needs, for before audio starts.
    pub fn for_song(song: &Song, rate: u32) -> Self {
        let mut pool = Self {
            // What keeps no long buffer never has one to hold, so it takes no room.
            free: std::array::from_fn(|at| {
                if BUFFERED[at] {
                    Vec::with_capacity(POOL_CAPACITY)
                } else {
                    Vec::new()
                }
            }),
            owed: MemoryCount::default(),
        };
        let needed = MemoryCount::of(song);
        for kind in EffectKind::ALL {
            for _ in 0..needed[kind] {
                if let Some(memory) = Memory::new(kind, rate) {
                    pool.put(memory);
                }
            }
        }
        pool
    }

    /// Keep what's no longer in use.
    pub fn put(&mut self, memory: Memory) {
        let held = &mut self.free[memory.kind() as usize];
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
        let mut memory = self.free[insert.kind()? as usize].pop()?;
        memory.clear();
        Some(memory)
    }

    /// Note that the edit thread wants this many back.
    pub fn owe(&mut self, count: MemoryCount) {
        for (owed, more) in self.owed.0.iter_mut().zip(count.0) {
            *owed += more;
        }
    }

    /// The next to send back to be freed, if any is owed and free, the first effect's first.
    pub fn next_owed(&mut self) -> Option<Memory> {
        for kind in EffectKind::ALL {
            if self.owed[kind] > 0
                && let Some(memory) = self.free[kind as usize].pop()
            {
                self.owed[kind] -= 1;
                return Some(memory);
            }
        }
        None
    }

    /// How many are free.
    pub fn spare(&self) -> usize {
        self.free.iter().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::sample_of_tick;
    use vt_model::constants::MIN_MILLIBPM;
    use vt_model::insert::Delay as DelaySettings;

    const DELAY: Insert = Insert::Delay(DelaySettings::DEFAULT);

    #[test]
    fn a_delay_holds_its_time_at_the_slowest_tempo_at_any_rate() {
        for rate in [44_100, 48_000, 96_000] {
            // A whole bar, at 20 BPM.
            let frames = sample_of_tick(3840, rate, MIN_MILLIBPM) as usize;
            let mut memory = Memory::new(EffectKind::Delay, rate).unwrap();
            let settings = DelaySettings {
                time: 17,
                ..DelaySettings::DEFAULT
            };
            let song = SongPlace {
                frames_per_tick: frames as f64 / 3840.0,
                ..SongPlace::STILL
            };
            memory.follow(Insert::Delay(settings), song, 0);
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
        let mut owed = MemoryCount::default();
        owed[EffectKind::Delay] = 1;
        pool.owe(owed);
        assert!(pool.next_owed().is_none(), "a slot still has it");
        pool.put(held);
        assert!(pool.next_owed().is_some());
        assert!(pool.next_owed().is_none());
    }

    #[test]
    fn what_keeps_a_buffer_is_made_of_its_own_kind_and_nothing_else_is() {
        for kind in EffectKind::ALL {
            let made = Memory::new(kind, 48_000).map(|memory| memory.kind());
            assert_eq!(made, BUFFERED[kind as usize].then_some(kind), "{kind:?}");
        }
    }
}
