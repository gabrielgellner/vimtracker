//! A track's slot: what it keeps for whichever effect it holds, and what plays it. An effect with
//! a long buffer keeps it in [`super::memory`]'s pool, and its DSP is in a file of its own; the
//! small effects are kept in every slot, and so is what [`Distort`](super::distort) keeps.
//!
//! What the slot needs to know of each effect comes from its DSP's [`SlotEffect`] and from its
//! description in the model, so an effect with a long buffer that the slot rings needs nothing
//! here (decision 340).

use vt_model::constants::{MAX_TRACKS, UNITY_LEVEL};
use vt_model::insert::{Distort, DistortType, EffectKind, Insert, Param, Place, Shown, Warps};
use vt_model::song::level_gain;

use crate::dsp::blocks::sides::{SameBits, same_sides};
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::dynamics::compressor::Compressor as Dynamics;
use crate::dsp::dynamics::eq::ThreeBands;
use crate::dsp::dynamics::glue::PeakClip;
use crate::dsp::effects::distort::Distortion;
use crate::dsp::effects::exciter::Exciter as Excitement;
use crate::dsp::effects::memory::{BUFFERED_FACTS, Memory, MemoryPool, SILENT, SongPlace};
use crate::dsp::effects::phaser::Phaser as AllPasses;
use crate::dsp::filters::lpg::LowPassGate as Gate;
use crate::dsp::filters::vowel::Formants;
use crate::rng::Pcg32;

/// The note a slot's oscillator plays before the track has played one: middle C.
pub const DEFAULT_NOTE: f32 = 60.0;
/// How long switching a slot out or back in takes, in ms: long enough not to click, and short
/// enough that a step's lock of its bypass lands with the step.
pub(crate) const BYPASS_FADE_MS: f32 = 5.0;

/// What a slot effect does, besides how it's played: it's made on the edit thread, follows what's
/// set, is emptied for the pool and says how long its tail lasts, what it does with a sound that's
/// the same on both sides and how the slot blends it. One with a long buffer is reached through
/// [`Memory`]'s `match`es, which `memories!` writes, never through `dyn` (decision 340). One small
/// enough to keep in every slot, in `KEPT_IN_SLOT`, has a field of [`InsertState`]'s; the pool
/// never holds it, so it has no tail the slot waits on. Nothing here may allocate but `new`, which
/// runs on the edit thread.
///
/// Each effect's own methods mostly have these names already, and an `impl` hands each call on to
/// them: a method of the type's own is picked over the trait's, so `self.follow(settings)` inside
/// one isn't a call back into it.
pub(crate) trait SlotEffect: Sized {
    /// Its settings, as its `Insert` holds them.
    type Settings: Copy;

    /// What it does with a sound that's the same on both sides (decision 147). `Mono::Stereo` is
    /// always safe, and `tests/all/mono.rs` checks every other claim.
    const MONO: Mono = Mono::Stereo;
    /// How the slot blends what it gives with the sound going in, and turns it up or down.
    const MIXING: Mixing = Mixing::Wet;

    /// Made on the edit thread, with its long buffers.
    fn new(rate: u32) -> Self;
    /// Take what's set now, where the song is and this many frames in.
    fn follow(&mut self, settings: Self::Settings, song: SongPlace, frames: u64);
    /// Jump to what's set, as it's only just started.
    fn settle(&mut self) {}
    /// Empty what it keeps, before the pool hands it out again.
    fn clear(&mut self);
    /// How many quiet frames in a row mean nothing's left in it.
    fn quiet_frames(&self, rate: u32) -> u32;
    /// Move what goes by the frame rather than by what it's done to where it is this many frames
    /// in, as taking its settings does.
    fn keep_time(&mut self, _frames: u64, _song: SongPlace) {}
    /// Give what draws random numbers its own generator, from the track's.
    fn seed(&mut self, _rng: Pcg32) {}
    /// Whether it's about to play something of its own with nothing going in, so it has to be
    /// woken.
    fn waking(&self) -> bool {
        false
    }
    /// A step has switched the slot's trigger on.
    fn strike(&mut self) {}
}

/// A slot effect the slot plays through its `ring`: a frame in, and what's wet out, with nothing
/// more from the slot. The limiter, Warps, the resonator, the comb and the frequency shifter aren't
/// rung: the slot plays each itself, handing it the note, its glide or another track.
pub(crate) trait Rung: SlotEffect {
    fn process(&mut self, input: [f32; 2]) -> [f32; 2];
}

/// Take what's set now, where the song is and this many frames in, and jump there if it's only
/// just started, `fresh`, rather than gliding from where it was.
#[inline]
pub(crate) fn follow_fresh<E: SlotEffect>(
    effect: &mut E,
    settings: E::Settings,
    song: SongPlace,
    frames: u64,
    fresh: bool,
) {
    effect.follow(settings, song, frames);
    if fresh {
        effect.settle();
    }
}

/// Each track's sound after its slots and before its level and mute, from the latest frame, for a
/// compressor that hears another track. A track renders in order, so one later than the listener
/// is heard a frame late, which is far too little to hear.
#[derive(Debug, Clone)]
pub struct Keys {
    pub frames: [[f32; 2]; MAX_TRACKS],
    /// How many sound tracks there are, so the returns' places are known.
    pub sounds: usize,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            frames: [[0.0; 2]; MAX_TRACKS],
            sounds: 0,
        }
    }
}

impl Keys {
    /// A track's latest frame, or silence for one that isn't there. A setting's range keeps a
    /// key on a track, so one past them is a mistake the tests should hear of.
    pub(crate) fn of(&self, track: usize) -> [f32; 2] {
        debug_assert!(track < self.frames.len(), "no track {track} to key from");
        self.frames.get(track).copied().unwrap_or_default()
    }
}

/// What an effect does with a sound that's the same on both sides, bit for bit, as a mono voice's
/// is until something spreads it (decision 147). A slot is told whether what comes in is, by the
/// slot before or by the track looking once, and an effect that keeps it so tells the slot after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mono {
    /// Each side goes through the same steps with its own state and nothing between them, so it
    /// works the left out alone and gives it to both, keeping the right's state to copy from the
    /// left's the moment the sides differ (`Sides`). What comes out is exactly what working out
    /// both gives. After the sides have differed, their states can stay apart for a while, so it
    /// works out both until they're the same again, and the slot looks at what comes out.
    OneSide,
    /// One state for both sides, such as a compressor's linked detector, and the same done to
    /// each: what comes out is the same on both sides, with nothing more to save, so the slot
    /// passes that on without looking.
    Linked,
    /// It spreads the sides apart or mixes them, or hasn't been looked at for this: it works out
    /// both, and the slot looks at what comes out.
    Stereo,
}

/// How a slot blends what its effect gives with the sound going in, by the effect's `Mix`, and
/// turns what comes out up or down by its `Gain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mixing {
    /// What it gives is all wet, blended with the sound by `Mix`, then turned by `Gain`. Written
    /// so all wet and all dry are exact.
    Wet,
    /// What it makes is added to the sound, as an Aural Exciter's is, so `Mix` at `00` is the
    /// sound alone and at `FF` the sound with all it makes.
    Added,
    /// It's blended with the sound by `Mix`, with nothing after: a compressor's makeup is its own,
    /// on what it turned down, so compressing in parallel turns up only that.
    Parallel,
    /// What it gives is already its own blend of the sound and what it made, as Clouds' and
    /// Beads' are, so the slot only turns it by `Gain`. Until its recording comes, the sound
    /// passes.
    Inside,
    /// It's all wet, and what it leaves alone comes through as it is, as an EQ's does, so the slot
    /// only turns it by `Gain`.
    AllWet,
}

/// What a slot needs to know of an effect: from its DSP's [`SlotEffect`], and where its `Mix` and
/// `Gain` are, from its description.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Facts {
    mono: Mono,
    mixing: Mixing,
    /// Where its `Mix` and its `Gain` are among the settings it keeps, found when `FACTS` is
    /// made.
    mix: Option<usize>,
    gain: Option<usize>,
}

impl Facts {
    /// What a DSP says of itself.
    pub(crate) const fn of<E: SlotEffect>() -> Self {
        Self {
            mono: E::MONO,
            mixing: E::MIXING,
            mix: None,
            gain: None,
        }
    }
}

/// An empty slot's: it passes the sound as it came, which is the same on both sides if it was.
const EMPTY: Facts = Facts {
    mono: Mono::Linked,
    mixing: Mixing::Wet,
    mix: None,
    gain: None,
};

/// The effects kept in every slot rather than in the pool, each with its DSP, which has a field of
/// [`InsertState`]'s. An effect with a long buffer is listed in `memories!` instead.
const KEPT_IN_SLOT: [(EffectKind, Facts); 7] = [
    (EffectKind::Distort, Facts::of::<Distortion>()),
    (EffectKind::Eq, Facts::of::<ThreeBands>()),
    (EffectKind::Compressor, Facts::of::<Dynamics>()),
    (EffectKind::Phaser, Facts::of::<AllPasses>()),
    (EffectKind::Lpg, Facts::of::<Gate>()),
    (EffectKind::Vowel, Facts::of::<Formants>()),
    (EffectKind::Exciter, Facts::of::<Excitement>()),
];

/// What every effect is to its slot, by [`EffectKind`]. Each has to be kept in the slot or in the
/// pool, not both, and has to have the `Mix` and `Gain` its mixing takes, or it doesn't build.
static FACTS: [Facts; EffectKind::COUNT] = {
    let mut found = BUFFERED_FACTS;
    let mut at = 0;
    while at < KEPT_IN_SLOT.len() {
        let (kind, facts) = KEPT_IN_SLOT[at];
        assert!(
            found[kind as usize].is_none(),
            "an effect is kept in its slot and in the pool"
        );
        found[kind as usize] = Some(facts);
        at += 1;
    }
    let mut facts = [EMPTY; EffectKind::COUNT];
    let mut at = 0;
    while at < EffectKind::COUNT {
        let Some(mut these) = found[at] else {
            panic!("an effect has no DSP");
        };
        let params = EffectKind::ALL[at].params();
        these.mix = output(params, "mix");
        these.gain = output(params, "gain");
        let (mixes, turns) = match these.mixing {
            Mixing::Wet | Mixing::Added => (true, true),
            Mixing::Parallel => (true, false),
            Mixing::Inside | Mixing::AllWet => (false, true),
        };
        assert!(
            !mixes || these.mix.is_some(),
            "an effect that mixes has no Mix"
        );
        assert!(
            !turns || these.gain.is_some(),
            "an effect that turns has no Gain"
        );
        facts[at] = these;
        at += 1;
    }
    facts
};

/// The place among `params` of the one stored as `key` among what comes out, if there's one.
const fn output(params: &[Param], key: &str) -> Option<usize> {
    let mut at = 0;
    while at < params.len() {
        let param = &params[at];
        if matches!(param.place, Place::Output) && same_bytes(param.key.as_bytes(), key.as_bytes())
        {
            return Some(at);
        }
        at += 1;
    }
    None
}

const fn same_bytes(one: &[u8], other: &[u8]) -> bool {
    if one.len() != other.len() {
        return false;
    }
    let mut at = 0;
    while at < one.len() {
        if one[at] != other[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// What the slot needs to know of the effect in it.
#[inline]
fn facts(insert: Insert) -> &'static Facts {
    match insert.kind() {
        Some(kind) => &FACTS[kind as usize],
        None => &EMPTY,
    }
}

/// What the slot eases and blends for an effect, as set: its amount, its mix and its gain.
fn levels(insert: Insert) -> (u8, u8, u8) {
    if insert.kind().is_none() {
        return (0, 0, UNITY_LEVEL);
    }
    let facts = facts(insert);
    let mix = || facts.mix.and_then(|at| insert.read(at)).unwrap_or(0xFF);
    let gain = || {
        facts
            .gain
            .and_then(|at| insert.read(at))
            .unwrap_or(UNITY_LEVEL)
    };
    let (mix, gain) = match facts.mixing {
        Mixing::Wet | Mixing::Added => (mix(), gain()),
        Mixing::Parallel => (mix(), UNITY_LEVEL),
        Mixing::Inside | Mixing::AllWet => (0xFF, gain()),
    };
    let amount = match insert {
        // `Down` sweeps how long it holds as the shaping modes sweep how hard they drive.
        Insert::Distort(Distort {
            kind: DistortType::Down,
            rate,
            ..
        }) => rate,
        Insert::Distort(Distort { amount, .. }) => amount,
        _ => 0,
    };
    (amount, mix, gain)
}

/// `wet` blended with `dry` by `mix`, then turned by `gain`, so all wet and all dry are exact.
#[inline]
fn mixed(wet: [f32; 2], dry: [f32; 2], mix: f32, gain: f32) -> [f32; 2] {
    [
        (wet[0] * mix + dry[0] * (1.0 - mix)) * gain,
        (wet[1] * mix + dry[1] * (1.0 - mix)) * gain,
    ]
}

/// What a track keeps for one of its slots. Nothing here allocates.
#[derive(Debug, Clone)]
pub struct InsertState {
    /// What its effect keeps, if it needs a long buffer.
    memory: Option<Memory>,
    /// How many frames in a row it's been quiet. It starts with nothing to play.
    quiet: u32,
    insert: Insert,
    amount: Smoothed,
    mix: Smoothed,
    gain: Smoothed,
    /// What a distortion keeps beside its `Amount`, `Mix` and `Gain`, small enough to keep in
    /// every slot.
    distort: Distortion,
    /// An EQ's bands, which are small enough to keep in every slot.
    eq: ThreeBands,
    /// A compressor's detector, small enough as well, and Glue's clip after its mix.
    dynamics: Dynamics,
    peak_clip: PeakClip,
    /// A phaser's all-passes, small enough as well.
    phaser: AllPasses,
    /// A low-pass gate's follower, vactrol and filters, small enough as well.
    gate: Gate,
    /// A vowel filter's formants, small enough as well.
    vowel: Formants,
    /// An exciter's filters, detectors and oversampling, small enough as well.
    exciter: Excitement,
    /// How far the slot is switched in: `1` in, `0` bypassed. It crossfades in a straight line
    /// over [`BYPASS_FADE_MS`], so switching doesn't click.
    on: Smoothed,
    /// The track's note, as a MIDI note with its `Pitch` added, for an oscillator or resonator to
    /// follow.
    note: f32,
    /// The generator an effect that draws random numbers starts from, from the track's seed, so
    /// a song renders the same each time: Clouds' and Beads' grains.
    rng: Pcg32,
}

impl Default for InsertState {
    fn default() -> Self {
        Self {
            memory: None,
            quiet: u32::MAX,
            insert: Insert::Empty,
            amount: Smoothed::at(0.0),
            mix: Smoothed::at(0.0),
            gain: Smoothed::at(1.0),
            distort: Distortion::default(),
            eq: ThreeBands::default(),
            dynamics: Dynamics::default(),
            peak_clip: PeakClip::default(),
            phaser: AllPasses::default(),
            gate: Gate::default(),
            vowel: Formants::default(),
            exciter: Excitement::default(),
            on: Smoothed::at(1.0),
            note: DEFAULT_NOTE,
            rng: Pcg32::new(0, 0),
        }
    }
}

/// Whether it's the same effect, so it carries on rather than starting afresh. One that shows
/// each type's own settings, as Distort and Comp do, is a different effect in each type.
fn same_effect(before: Insert, after: Insert) -> bool {
    let kind = after.kind();
    before.kind() == kind
        && kind.is_none_or(|kind| match kind.spec().shown {
            Shown::All => true,
            Shown::ByType { type_at, .. } => {
                before.read(usize::from(type_at)) == after.read(usize::from(type_at))
            }
        })
}

impl InsertState {
    /// Take what's now in the slot, with what it keeps coming from and going back to the pool. If
    /// there's none free yet, it's silent and asks again next time.
    pub(crate) fn hold(
        &mut self,
        insert: Insert,
        pool: &mut MemoryPool,
        song: SongPlace,
        frames: u64,
    ) {
        // An empty slot taking an empty slot again has nothing to set: an empty slot's own
        // settings are never heard, and an effect put in it starts afresh (#187).
        if matches!((self.insert, insert), (Insert::Empty, Insert::Empty)) && self.memory.is_none()
        {
            return;
        }
        let fresh = !same_effect(self.insert, insert);
        if fresh {
            self.give_back(pool);
        }
        self.take(insert, fresh, song, frames);
        let started = self.memory.is_none();
        if started {
            self.memory = pool.take(insert);
            if let Some(memory) = &mut self.memory {
                memory.seed(self.rng);
            }
        }
        if let Some(memory) = &mut self.memory {
            memory.follow(insert, song, frames, started);
            // A knock rings the tank with nothing going in, and a stutter just triggered has a
            // piece to take, so neither may stay asleep.
            if memory.waking() {
                self.quiet = 0;
            }
        }
    }

    /// Whether taking the same settings again would change nothing: it has what its effect keeps,
    /// if it keeps anything, and nothing in it is about to wake.
    pub(crate) fn holding(&self) -> bool {
        match &self.memory {
            Some(memory) => !memory.waking(),
            None => !Memory::kept_by(self.insert),
        }
    }

    /// What taking its settings again would still change, while they stay the same: what goes by
    /// the frame.
    pub(crate) fn keep_time(&mut self, frames: u64, song: SongPlace) {
        if let Some(memory) = &mut self.memory {
            memory.keep_time(frames, song);
        }
    }

    /// A step has switched the slot's trigger on: a stutter in it starts again from what comes in
    /// next, even if it's on already. Nothing else has one.
    pub(crate) fn strike(&mut self) {
        if let Some(memory) = &mut self.memory {
            memory.strike();
            if memory.waking() {
                self.quiet = 0;
            }
        }
    }

    /// Start drawing random numbers again from a generator of the track's, as playing from the top
    /// does, so what it plays repeats.
    pub(crate) fn reseed(&mut self, rng: Pcg32) {
        self.rng = rng;
        if let Some(memory) = &mut self.memory {
            memory.seed(rng);
        }
    }

    /// Give what it keeps back to the pool.
    pub(crate) fn give_back(&mut self, pool: &mut MemoryPool) {
        if let Some(memory) = self.memory.take() {
            pool.put(memory);
        }
        self.quiet = u32::MAX;
    }

    /// Take what's now in the slot, as `hold` does with the song standing still. A different
    /// effect starts afresh. What it keeps must be given back first, as this drops it.
    pub fn set(&mut self, insert: Insert) {
        self.take(
            insert,
            !same_effect(self.insert, insert),
            SongPlace::STILL,
            0,
        );
    }

    /// Take what's now in the slot, where the song is and this many frames in, starting afresh if
    /// `fresh`. Its long buffer follows in `hold`.
    fn take(&mut self, insert: Insert, fresh: bool, song: SongPlace, frames: u64) {
        let (amount, mix, gain) = levels(insert);
        let (amount, mix, gain) = (
            f32::from(amount) / 255.0,
            f32::from(mix) / 255.0,
            level_gain(f32::from(gain)),
        );
        if fresh {
            let (on, note, rng) = (self.on, self.note, self.rng);
            *self = Self::default();
            self.on = on;
            self.note = note;
            self.rng = rng;
            self.amount = Smoothed::at(amount);
            self.mix = Smoothed::at(mix);
            self.gain = Smoothed::at(gain);
        }
        match insert {
            Insert::Distort(settings) => {
                follow_fresh(&mut self.distort, settings, song, frames, fresh);
            }
            Insert::Eq(settings) => follow_fresh(&mut self.eq, settings, song, frames, fresh),
            Insert::Compressor(settings) => {
                follow_fresh(&mut self.dynamics, settings, song, frames, fresh);
                follow_fresh(&mut self.peak_clip, settings, song, frames, fresh);
            }
            Insert::Phaser(settings) => {
                follow_fresh(&mut self.phaser, settings, song, frames, fresh);
            }
            Insert::Lpg(settings) => follow_fresh(&mut self.gate, settings, song, frames, fresh),
            Insert::Vowel(settings) => follow_fresh(&mut self.vowel, settings, song, frames, fresh),
            Insert::Exciter(settings) => {
                follow_fresh(&mut self.exciter, settings, song, frames, fresh);
            }
            _ => {}
        }
        self.insert = insert;
        self.amount.target = amount;
        self.mix.target = mix;
        self.gain.target = gain;
    }

    /// How far a compressor or limiter in the slot is turning down now, in dB, for a meter. Any
    /// other effect, or a bypassed one, turns down nothing.
    pub(crate) fn reduction(&self) -> f32 {
        if self.on.target == 0.0 {
            return 0.0;
        }
        match (self.insert, &self.memory) {
            (Insert::Compressor(_), _) => self.dynamics.reduction(),
            (Insert::Limit(_), Some(Memory::Limit(limiter))) => limiter.reduction(),
            _ => 0.0,
        }
    }

    /// How far a low-pass gate in the slot is open now, its level and cutoff from `0` to `1`, for
    /// the FX page's zoom to draw. Any other effect has none.
    pub(crate) fn opening(&self) -> Option<(f32, f32)> {
        matches!(self.insert, Insert::Lpg(_)).then(|| self.gate.opening())
    }

    /// The track's note as it's heard, with its `Pitch`, which a resonator, comb, vowel filter or
    /// oscillator in the slot follows.
    pub(crate) fn note(&self) -> f32 {
        self.note
    }

    /// Whether it's got something left to play with nothing going in.
    #[inline]
    pub(crate) fn ringing(&self, rate: u32) -> bool {
        self.memory
            .as_ref()
            .is_some_and(|memory| self.quiet < memory.quiet_frames(rate))
    }

    /// What the slot would give for `input`, and whether its two sides are the same, if taking it
    /// would leave everything it keeps as it was, so that it would give the same for the same
    /// input every frame after. A mix bus whose slots all have settled like this with silence
    /// coming in can skip them until something comes in or changes (#558). What it hears from
    /// another track has to stay as `keys` has it now. `smoothing` is what its settings move by.
    ///
    /// A copy of the slot, without its long buffer, takes the frame, and what it keeps is compared
    /// with the slot's bit for bit. One with a long buffer does no work only with nothing coming in
    /// and nothing ringing, and is otherwise never settled. A phaser's sweep, a vowel filter's
    /// block and `Down`'s hold go on by themselves, so they never settle either.
    pub(crate) fn settled(
        &self,
        input: [f32; 2],
        same: bool,
        smoothing: f32,
        rate: u32,
        keys: &Keys,
    ) -> Option<([f32; 2], bool)> {
        let moves_by_itself = matches!(
            self.insert,
            Insert::Phaser(_)
                | Insert::Vowel(_)
                | Insert::Distort(Distort {
                    kind: DistortType::Down,
                    ..
                })
        );
        let buffer_working =
            Memory::kept_by(self.insert) && (input != [0.0, 0.0] || self.ringing(rate));
        if !self.on.value.same_bits(&self.on.target) || moves_by_itself || buffer_working {
            return None;
        }
        // An empty slot keeps nothing, so there's nothing to try.
        if let Some(passed) = self.passed_through(input, same) {
            return Some(passed);
        }
        let mut tried = self.without_memory();
        let out = tried.process_sides(input, same, smoothing, rate, keys);
        tried.kept_as(self).then_some(out)
    }

    /// The slot as it is but for its long buffer, which it doesn't have, to try a frame on.
    /// Each field is copied by hand rather than by `..self.clone()`, as cloning the slot would
    /// copy its `Memory` too, allocating on the audio thread.
    fn without_memory(&self) -> Self {
        let Self {
            memory: _,
            quiet,
            insert,
            amount,
            mix,
            gain,
            distort,
            eq,
            dynamics,
            peak_clip,
            phaser,
            gate,
            vowel,
            exciter,
            on,
            note,
            rng,
        } = self;
        Self {
            memory: None,
            quiet: *quiet,
            insert: *insert,
            amount: *amount,
            mix: *mix,
            gain: *gain,
            distort: *distort,
            eq: *eq,
            dynamics: *dynamics,
            peak_clip: *peak_clip,
            phaser: *phaser,
            gate: *gate,
            vowel: *vowel,
            exciter: exciter.clone(),
            on: *on,
            note: *note,
            rng: *rng,
        }
    }

    /// Whether everything a frame can change, but for a long buffer, a phaser's and a vowel
    /// filter's, is as it is in `other`, bit for bit.
    fn kept_as(&self, other: &Self) -> bool {
        self.quiet == other.quiet
            && self.amount.same_bits(&other.amount)
            && self.mix.same_bits(&other.mix)
            && self.gain.same_bits(&other.gain)
            && self.on.same_bits(&other.on)
            && self.distort.same_bits(&other.distort)
            && self.dynamics.same_bits(&other.dynamics)
            && self.peak_clip.same_bits(&other.peak_clip)
            && self.gate.same_bits(&other.gate)
            && self.exciter.same_bits(&other.exciter)
            && self.eq.same_bits(&other.eq)
    }

    /// Keep up what skipping these frames of silence, once the slot has settled, would leave
    /// behind: how long Warps has been quiet.
    pub(crate) fn rested(&mut self, frames: u32) {
        if let (Insert::Warps(_), Some(Memory::Warps(_))) = (self.insert, &self.memory) {
            self.quiet = self.quiet.saturating_add(frames);
        }
    }

    /// One frame through what the slot keeps. Once it's quiet with nothing going in, it does no work.
    #[inline]
    fn ring(&mut self, input: [f32; 2], rate: u32) -> [f32; 2] {
        let nothing_in = input == [0.0, 0.0];
        if nothing_in && !self.ringing(rate) {
            return [0.0, 0.0];
        }
        let Some(memory) = &mut self.memory else {
            return [0.0, 0.0];
        };
        let wet = memory.process(input);
        let quiet = nothing_in && wet[0].abs() < SILENT && wet[1].abs() < SILENT;
        self.quiet = if quiet {
            self.quiet.saturating_add(1)
        } else {
            0
        };
        wet
    }

    /// One frame through a limiter, all wet, and the dry frame as late. Like `ring`, once it's quiet
    /// with nothing going in it does no work, and it starts again as if it had let go.
    #[inline]
    fn limit(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [[f32; 2]; 2] {
        let nothing_in = input == [0.0, 0.0];
        let resting = !self.ringing(rate);
        if nothing_in && resting {
            return [[0.0; 2]; 2];
        }
        let Some(Memory::Limit(limiter)) = &mut self.memory else {
            return [[0.0; 2]; 2];
        };
        if resting {
            limiter.rest();
        }
        let [wet, dry] = limiter.process(input, smoothing);
        let quiet = nothing_in && wet.iter().chain(&dry).all(|sample| sample.abs() < SILENT);
        self.quiet = if quiet {
            self.quiet.saturating_add(1)
        } else {
            0
        };
        [wet, dry]
    }

    /// One frame through Warps, all wet, hearing another track or playing its oscillator. It keeps
    /// going while either the slot's own sound or the track it hears has something in it, then
    /// fades out over its tail, so its oscillator and vocoder never stop dead.
    #[inline]
    fn warps(&mut self, input: [f32; 2], settings: Warps, smoothing: f32, keys: &Keys) -> [f32; 2] {
        let heard = |frame: [f32; 2]| frame[0].abs() >= SILENT || frame[1].abs() >= SILENT;
        let modulator = settings
            .source_track(keys.sounds)
            .map(|track| keys.of(track));
        let busy = heard(input) || modulator.is_some_and(heard);
        let Some(Memory::Warps(warps)) = &mut self.memory else {
            return [0.0; 2];
        };
        self.quiet = if busy {
            0
        } else {
            self.quiet.saturating_add(1)
        };
        let tail = warps.quiet_frames();
        if self.quiet >= tail {
            return [0.0; 2];
        }
        let fade = 1.0 - self.quiet as f32 / tail as f32;
        [warps.process(input, modulator, self.note, smoothing) * fade; 2]
    }

    /// One frame through a resonator or comb, all wet, tuned to the track's note, or a frequency
    /// shifter following it. Like `ring`, once it's quiet with nothing going in it does no work,
    /// and when it starts again it's at the note and what's set, rather than gliding there from
    /// where it stopped.
    #[inline]
    fn resonate(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let nothing_in = input == [0.0, 0.0];
        let resting = !self.ringing(rate);
        if nothing_in && resting {
            return [0.0; 2];
        }
        let wet = match &mut self.memory {
            Some(Memory::Reso(resonator)) => {
                if resting {
                    resonator.settle();
                }
                resonator.process(input, self.note, smoothing)
            }
            Some(Memory::Comb(comb)) => {
                if resting {
                    comb.settle();
                }
                comb.process(input, self.note, smoothing)
            }
            Some(Memory::FreqShift(shifter)) => {
                if resting {
                    shifter.settle();
                }
                shifter.process(input, self.note, smoothing)
            }
            _ => return [0.0; 2],
        };
        let quiet = nothing_in && wet[0].abs() < SILENT && wet[1].abs() < SILENT;
        self.quiet = if quiet {
            self.quiet.saturating_add(1)
        } else {
            0
        };
        wet
    }

    /// Follow the track's note, for an oscillator, resonator, comb, vowel filter or frequency
    /// shifter of the slot's own.
    pub(crate) fn follow_note(&mut self, note: f32) {
        self.note = note;
    }

    /// Switch the slot out, or back in. It keeps running underneath, so a tail or a compressor's
    /// state carries on as if it had never gone.
    pub fn bypass(&mut self, bypassed: bool) {
        self.on.target = if bypassed { 0.0 } else { 1.0 };
    }

    /// Jump to the settings, for playing from the start.
    pub fn settle(&mut self) {
        self.on.settle();
        self.amount.settle();
        self.distort.settle();
        self.mix.settle();
        self.gain.settle();
        self.phaser.settle();
        self.peak_clip.settle();
    }

    /// How hard its distortion goes, eased, for Distort's tests.
    #[cfg(test)]
    pub(crate) fn amount(&self) -> Smoothed {
        self.amount
    }

    /// Whether the slot has no effect in it.
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self.insert, Insert::Empty)
    }

    /// What an empty slot gives for a frame while it isn't fading, which is all
    /// [`InsertState::process_sides`] does with one: the frame as it came, with a NaN or an
    /// infinity heard as silence, and whether its sides are the same. It keeps nothing, so it's
    /// passed by without working through the rest (#187). None for any other slot.
    #[inline]
    #[allow(
        clippy::float_cmp,
        reason = "the switch's ramp lands on its target exactly"
    )]
    pub(crate) fn passed_through(&self, input: [f32; 2], same: bool) -> Option<([f32; 2], bool)> {
        if !self.is_empty() || self.on.value != self.on.target {
            return None;
        }
        let out = input.map(|sample| if sample.is_finite() { sample } else { 0.0 });
        Some((out, same || same_sides(out)))
    }

    /// What its effect does with a sound that's the same on both sides (decision 147), as its
    /// DSP's `SlotEffect::MONO` says. An empty slot is linked: it passes the sound as it came.
    pub fn mono(&self) -> Mono {
        facts(self.insert).mono
    }

    /// One frame through the slot, faded toward what came in as it's bypassed.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32, keys: &Keys) -> [f32; 2] {
        self.process_sides(input, same_sides(input), smoothing, rate, keys)
            .0
    }

    /// The same, with `same` saying whether the frame's two sides are the same, bit for bit, as
    /// the slot before found or made them, and saying whether those of what comes out are, for
    /// the slot after. False is always safe. A linked effect passes true on without looking; any other
    /// looks at what comes out, as one that works one side out for both can have sides that
    /// still differ for a while after the sound's did.
    #[inline]
    pub fn process_sides(
        &mut self,
        input: [f32; 2],
        same: bool,
        smoothing: f32,
        rate: u32,
        keys: &Keys,
    ) -> ([f32; 2], bool) {
        // A NaN or an infinity would stay in any filter or loop it reached, silencing the slot for
        // good whatever clips it on the way round, so it's heard as a frame of silence instead
        // (#461). Sides that were the same, bit for bit, still are.
        let input = input.map(|sample| if sample.is_finite() { sample } else { 0.0 });
        let kept = same && self.mono() == Mono::Linked;
        let wet = self.effect(input, same, smoothing, rate, keys);
        #[allow(
            clippy::float_cmp,
            reason = "the switch's ramp lands on its target exactly"
        )]
        let out = if self.on.value == self.on.target {
            if self.on.target > 0.0 { wet } else { input }
        } else {
            // A straight line rather than a smoother's curve, so a step's lock is done within a
            // few ms of its step, however long it lasts.
            let by = 1000.0 / (BYPASS_FADE_MS * rate as f32);
            let on = if self.on.target > self.on.value {
                (self.on.value + by).min(self.on.target)
            } else {
                (self.on.value - by).max(self.on.target)
            };
            self.on.value = on;
            [
                input[0] + (wet[0] - input[0]) * on,
                input[1] + (wet[1] - input[1]) * on,
            ]
        };
        (out, kept || same_sides(out))
    }

    /// One frame through the effect. An empty slot does nothing else.
    #[inline]
    fn effect(
        &mut self,
        input: [f32; 2],
        same: bool,
        smoothing: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        let (wet, dry) = match self.insert {
            Insert::Empty => return input,
            Insert::Compressor(settings) => {
                // It hears its own input, or another track's as it was after its slots.
                let key = settings
                    .source_track(keys.sounds)
                    .map_or(input, |track| keys.of(track));
                let wet = self.dynamics.process(input, key, settings, smoothing, rate);
                // Glue's clip is the last thing it does, after its makeup and the mix.
                let out = self.output(wet, input, smoothing);
                return self.peak_clip.process(out, rate);
            }
            Insert::Lpg(settings) => {
                // It's opened by its own input, or another track's as it was after its slots.
                let key = settings
                    .source_track(keys.sounds)
                    .map_or(input, |track| keys.of(track));
                let wet = self
                    .gate
                    .process(input, same, key, settings, smoothing, rate);
                (wet, input)
            }
            // The dry sound comes out as late as the limited one, so blending them doesn't comb.
            Insert::Limit(_) => {
                let [wet, dry] = self.limit(input, smoothing, rate);
                (wet, dry)
            }
            Insert::Warps(settings) => (self.warps(input, settings, smoothing, keys), input),
            Insert::Reso(_) | Insert::Comb(_) | Insert::FreqShift(_) => {
                (self.resonate(input, smoothing, rate), input)
            }
            Insert::Vowel(_) => {
                let wet = self.vowel.process(input, same, self.note, smoothing, rate);
                (wet, input)
            }
            Insert::Exciter(_) => (self.exciter.process(input, same, smoothing, rate), input),
            Insert::Phaser(_) => (self.phaser.process(input, smoothing, rate), input),
            Insert::Eq(_) => (self.eq.process(input, same, smoothing, rate), input),
            Insert::Distort(settings) => {
                let amount = self.amount.next(smoothing);
                let wet = self
                    .distort
                    .process(input, same, settings, amount, self.note, smoothing, rate);
                (wet, input)
            }
            // Every other effect the slot rings. One that blends inside itself passes the sound
            // until its recording comes.
            insert => {
                let waiting = self.memory.is_none() && facts(insert).mixing == Mixing::Inside;
                let wet = if waiting {
                    input
                } else {
                    self.ring(input, rate)
                };
                (wet, input)
            }
        };
        self.output(wet, dry, smoothing)
    }

    /// What comes out of the slot's effect: what it gave, `wet`, with the `dry` sound as its
    /// effect's [`Mixing`] says, at the slot's mix and gain.
    #[inline]
    fn output(&mut self, wet: [f32; 2], dry: [f32; 2], smoothing: f32) -> [f32; 2] {
        match facts(self.insert).mixing {
            Mixing::Wet => {
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                mixed(wet, dry, mix, gain)
            }
            Mixing::Added => {
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                [
                    (dry[0] + wet[0] * mix) * gain,
                    (dry[1] + wet[1] * mix) * gain,
                ]
            }
            Mixing::Parallel => {
                let mix = self.mix.next(smoothing);
                mixed(wet, dry, mix, 1.0)
            }
            Mixing::Inside | Mixing::AllWet => {
                let gain = self.gain.next(smoothing);
                wet.map(|sample| sample * gain)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::signals::{peak, sines};
    use vt_model::insert::{Compressor, CompressorType, Delay, Equaliser};

    const RATE: u32 = 48_000;
    /// Settings take effect at once, as the smoother has nothing to do.
    const NO_SMOOTHING: f32 = 1.0;

    #[test]
    fn an_empty_slot_passed_by_gives_what_working_it_through_does() {
        let keys = Keys::default();
        let frames = [
            [0.25, -0.5],
            [0.3, 0.3],
            [-0.0, 0.0],
            [f32::NAN, 0.1],
            [f32::INFINITY, f32::INFINITY],
        ];
        for bypassed in [false, true] {
            let mut state = InsertState::default();
            state.bypass(bypassed);
            state.settle();
            for frame in frames {
                for same in [false, true] {
                    let passed = state.passed_through(frame, same).unwrap();
                    let worked = state.process_sides(frame, same, NO_SMOOTHING, RATE, &keys);
                    assert_eq!(passed.0.map(f32::to_bits), worked.0.map(f32::to_bits));
                    assert_eq!(passed.1, worked.1);
                    assert_eq!(
                        state.settled(frame, same, NO_SMOOTHING, RATE, &keys),
                        Some(passed)
                    );
                }
            }
            // Fading in or out, it's worked through, as the fade moves.
            state.bypass(!bypassed);
            assert!(state.passed_through([0.1, 0.2], false).is_none());
        }
        let mut soft = InsertState::default();
        soft.set(Insert::Distort(Distort {
            kind: DistortType::Soft,
            amount: 0x80,
            ..Distort::DEFAULT
        }));
        assert!(soft.passed_through([0.1, 0.2], false).is_none());
    }

    #[test]
    fn an_empty_slot_leaves_the_sound_alone() {
        let mut state = InsertState::default();
        for sample in sines(1000, 220.0, 0.5, RATE) {
            assert_eq!(
                state.process([sample, -sample], 0.01, RATE, &Keys::default()),
                [sample, -sample]
            );
        }
    }

    #[test]
    fn bypassing_fades_rather_than_jumps_and_lands_on_the_input() {
        let mut state = InsertState::default();
        state.set(Insert::Distort(Distort {
            gain: 0x80,
            ..Distort::DEFAULT
        }));
        state.settle();
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let mut last = state.process([0.5; 2], smoothing, RATE, &Keys::default())[0];
        state.bypass(true);
        for _ in 0..RATE / 10 {
            let out = state.process([0.5; 2], smoothing, RATE, &Keys::default())[0];
            assert!((out - last).abs() < 0.03, "{last} to {out}");
            last = out;
        }
        assert_eq!(last, 0.5);
    }
    /// Every effect, with what comes round set high, fed a NaN once among ordinary sound: what
    /// comes out after stays finite and dies away, rather than the slot going silent for good.
    #[test]
    fn a_nan_going_in_is_gone_at_once_and_what_was_ringing_dies_away() {
        use vt_model::insert::*;
        let looped = [
            Insert::Delay(Delay {
                feedback: 0xE0,
                time: 6,
                ..Delay::DEFAULT
            }),
            Insert::Delay(Delay {
                feedback: 0xE0,
                time: 6,
                low_cut: 0x40,
                high_cut: 0xC0,
                drive: 0xFF,
                ..Delay::DEFAULT
            }),
            Insert::Reverb(Reverb {
                decay: 0xE0,
                ..Reverb::DEFAULT
            }),
            Insert::Plate(Plate {
                decay: 0xE0,
                ..Plate::DEFAULT
            }),
            Insert::Spring(Spring {
                decay: 0xFF,
                ..Spring::DEFAULT
            }),
            // Its longest tail takes over a minute to die away, so this one's its middle.
            Insert::Space(Space::DEFAULT),
            Insert::Phaser(Phaser {
                feedback: 0xFF,
                ..Phaser::DEFAULT
            }),
            Insert::Flanger(Flanger {
                feedback: 0xFF,
                ..Flanger::DEFAULT
            }),
            Insert::Pitch(Pitch {
                feedback: 0xFF,
                ..Pitch::DEFAULT
            }),
            Insert::Reso(Resonator::DEFAULT),
            Insert::Comb(Comb::DEFAULT),
            Insert::Comb(Comb {
                feedback: 0x10,
                ..Comb::DEFAULT
            }),
        ];
        let filtered = [
            Insert::Chorus(Chorus::DEFAULT),
            Insert::Chorus(Chorus {
                kind: ChorusType::Ensemble,
                ..Chorus::DEFAULT
            }),
            Insert::Eq(Equaliser::DEFAULT),
            Insert::Compressor(Compressor::DEFAULT),
            Insert::Limit(Limiter::DEFAULT),
            Insert::Tape(Tape::DEFAULT),
            Insert::Grains(Grains::DEFAULT),
            Insert::Warps(Warps::DEFAULT),
            Insert::Lpg(LowPassGate::DEFAULT),
            Insert::Vowel(Vowel::DEFAULT),
            Insert::Rotary(Rotary {
                fast: 1,
                drive: 0xFF,
                ..Rotary::DEFAULT
            }),
            Insert::Exciter(Exciter {
                drive: 0xFF,
                tone: 0x00,
                ..Exciter::DEFAULT
            }),
            Insert::Stutter(Stutter {
                trig: 1,
                ..Stutter::DEFAULT
            }),
            Insert::Distort(Distort {
                kind: DistortType::Hard,
                ..Distort::DEFAULT
            }),
            Insert::Distort(Distort {
                kind: DistortType::Overdrive,
                ..Distort::DEFAULT
            }),
        ];
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let keys = Keys::default();
        for insert in looped.into_iter().chain(filtered) {
            let mut song = vt_model::song::Song::new(1);
            song.tracks[0].inserts[0] = insert;
            let mut pool = MemoryPool::for_song(&song, RATE);
            let mut state = InsertState::default();
            let song = SongPlace {
                frames_per_tick: 25.0,
                ..SongPlace::STILL
            };
            state.hold(insert, &mut pool, song, 0);
            state.settle();
            for sample in sines(RATE as usize / 4, 220.0, 0.5, RATE) {
                state.process([sample, -sample * 0.5], smoothing, RATE, &keys);
            }
            state.process([f32::NAN, f32::INFINITY], smoothing, RATE, &keys);
            let after: Vec<f32> = (0..RATE * 12)
                .flat_map(|_| state.process([0.0; 2], smoothing, RATE, &keys))
                .collect();
            assert!(after.iter().all(|s| s.is_finite()), "{insert:?}");
            let last = peak(&after[after.len() - RATE as usize / 5..]);
            assert!(last < 1e-3, "{insert:?}: {last}");
        }
    }

    /// Every effect, every compressor type with `Auto` and Glue's clip on, and every distortion.
    fn every_effect() -> Vec<Insert> {
        let mut inserts = Insert::ALL.to_vec();
        inserts.extend(CompressorType::ALL.map(|kind| {
            Insert::Compressor(Compressor {
                kind,
                auto: 1,
                peak_clip: 1,
                ..Compressor::DEFAULT
            })
        }));
        inserts.extend(DistortType::ALL.map(|kind| {
            Insert::Distort(Distort {
                kind,
                amount: 0xA0,
                ..Distort::DEFAULT
            })
        }));
        inserts
    }

    /// Every effect in each of its types and modes, with what the slot reads of it set apart from
    /// its defaults.
    fn every_type_of_every_effect() -> Vec<Insert> {
        let mut inserts = Vec::new();
        for insert in every_effect() {
            let Some(kind) = insert.kind() else {
                inserts.push(insert);
                continue;
            };
            let mut set = insert;
            for (at, param) in kind.params().iter().enumerate() {
                if let "mix" | "gain" | "amount" | "rate" = param.key {
                    set = set.write(at, (0x13 + 0x1D * at) as f32);
                }
            }
            inserts.push(set);
            for (at, param) in kind.params().iter().enumerate() {
                if param.place == Place::Type {
                    for value in 0..=param.kind.top() {
                        inserts.push(set.write(at, f32::from(value)));
                    }
                }
            }
        }
        inserts
    }

    /// The setting stored as `key` of an effect, found by its key alone.
    fn by_key(insert: Insert, key: &str) -> Option<u8> {
        let kind = insert.kind()?;
        let at = kind.params().iter().position(|param| param.key == key)?;
        insert.read(at)
    }

    /// What the slot eases and blends for each effect is read from the effect's own settings, by
    /// the rule its mixing gives: `Mix` unless it gives its own blend or is all wet, and `Gain`
    /// unless it's blended in parallel. Distort's drive is the one amount, `Rate` for `Down`.
    #[test]
    fn the_slot_reads_each_effects_levels_from_its_own_settings() {
        for insert in every_type_of_every_effect() {
            let Some(kind) = insert.kind() else {
                assert_eq!(levels(insert), (0, 0, UNITY_LEVEL));
                continue;
            };
            let mix = || by_key(insert, "mix").expect("a Mix");
            let gain = || by_key(insert, "gain").expect("a Gain");
            let (mix, gain) = match FACTS[kind as usize].mixing {
                Mixing::Wet | Mixing::Added => (mix(), gain()),
                Mixing::Parallel => (mix(), UNITY_LEVEL),
                Mixing::Inside | Mixing::AllWet => (0xFF, gain()),
            };
            let amount = match insert {
                Insert::Distort(Distort {
                    kind: DistortType::Down,
                    ..
                }) => by_key(insert, "rate").expect("a Rate"),
                Insert::Distort(_) => by_key(insert, "amount").expect("an Amount"),
                _ => 0,
            };
            assert_eq!(levels(insert), (amount, mix, gain), "{insert:?}");
        }
    }

    /// An effect carries on through a change of its settings, but one whose types don't all show
    /// the same settings is another effect in each type, and starts afresh.
    #[test]
    fn an_effect_whose_types_show_their_own_settings_starts_afresh_in_another() {
        let inserts = every_type_of_every_effect();
        let type_of = |insert: Insert| {
            let kind = insert.kind()?;
            let at = kind
                .params()
                .iter()
                .position(|param| param.place == Place::Type)?;
            insert.read(at)
        };
        let by_type = |insert: Insert| {
            inserts
                .iter()
                .filter(|other| other.kind() == insert.kind())
                .any(|other| other.params() != insert.params())
        };
        for &insert in &inserts {
            for &other in &inserts {
                let same = insert.kind() == other.kind()
                    && (!by_type(insert) || type_of(insert) == type_of(other));
                assert_eq!(same_effect(insert, other), same, "{insert:?} {other:?}");
            }
        }
    }

    #[test]
    fn each_effect_is_kept_in_its_slot_or_in_the_pool() {
        for kind in EffectKind::ALL {
            let kept = KEPT_IN_SLOT.iter().any(|(listed, _)| *listed == kind);
            assert_ne!(kept, BUFFERED_FACTS[kind as usize].is_some(), "{kind:?}");
        }
    }

    /// A slot holding `insert`, with a long buffer of its own if it takes one.
    fn holding(insert: Insert) -> InsertState {
        let mut song = vt_model::song::Song::new(1);
        song.tracks[0].inserts[0] = insert;
        let mut pool = MemoryPool::for_song(&song, RATE);
        let mut state = InsertState::default();
        let song = SongPlace {
            frames_per_tick: 100.0,
            ..SongPlace::STILL
        };
        state.hold(insert, &mut pool, song, 0);
        state.settle();
        state
    }

    #[test]
    fn a_settled_slot_gives_what_it_said_it_would_for_silence() {
        // After a sound, it's asked every 16 frames of silence, and each frame until the next is
        // worked out as usual and has to be what it said, as a resting mix trusts it (#558).
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let keys = Keys::default();
        for insert in every_effect() {
            let mut state = holding(insert);
            for sample in sines(RATE as usize / 20, 220.0, 0.5, RATE) {
                state.process_sides([sample, sample * 0.8], false, smoothing, RATE, &keys);
            }
            let mut settled = 0;
            for _ in 0..RATE as usize / 16 {
                let said = state.settled([0.0; 2], true, smoothing, RATE, &keys);
                for _ in 0..16 {
                    let out = state.process_sides([0.0; 2], true, smoothing, RATE, &keys);
                    if let Some((frame, same)) = said {
                        assert!(frame.same_bits(&out.0), "{insert:?}: {frame:?} {out:?}");
                        assert_eq!(same, out.1, "{insert:?}");
                    }
                }
                settled += usize::from(said.is_some());
            }
            let never = matches!(
                insert,
                Insert::Phaser(_)
                    | Insert::Vowel(_)
                    | Insert::Distort(Distort {
                        kind: DistortType::Down,
                        ..
                    })
            );
            if never {
                assert_eq!(settled, 0, "{insert:?}");
            }
        }
    }

    /// A burst of a loud sine with noise on it, different on each side, then silence for at most
    /// `seconds`, until the slot has given exact silence for a second and, if `settles`, has
    /// settled. Returns when, in seconds, or none if it never did.
    #[allow(clippy::float_cmp, reason = "silence is exact zeros")]
    fn falls_silent(insert: Insert, seconds: usize, settles: bool) -> Option<f32> {
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let keys = Keys::default();
        let mut state = holding(insert);
        let mut noise = 1_u32;
        for sample in sines(RATE as usize / 5, 220.0, 0.9, RATE) {
            noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (noise >> 9) as f32 / (1 << 23) as f32 - 0.5;
            let frame = [sample + noise, sample * 0.7 - noise];
            state.process_sides(frame, false, smoothing, RATE, &keys);
        }
        let mut silent_since = None;
        for frame in 0..seconds * RATE as usize {
            let (out, _) = state.process_sides([0.0; 2], true, smoothing, RATE, &keys);
            if out != [0.0; 2] {
                silent_since = None;
                continue;
            }
            let since = *silent_since.get_or_insert(frame);
            let settled = !settles
                || state
                    .settled([0.0; 2], true, smoothing, RATE, &keys)
                    .is_some();
            if frame - since >= RATE as usize && settled {
                return Some(frame as f32 / RATE as f32);
            }
        }
        None
    }

    #[test]
    fn every_effect_falls_to_an_exact_silence_after_a_sound() {
        // Left to itself, a filter's state fell into the denormals and traded the smallest floats
        // between its states for good, so a delay after it rang for ever and a resting mix never
        // rested (#260). Each slot now gives exact silence soon after a sound, and has settled
        // too, but for what moves by itself and what takes longer by design: a compressor's
        // `Auto` follows the level for a minute and a half before it settles (decision 260), and
        // a low-pass gate's vactrol takes minutes to forget it was lit.
        for insert in every_effect() {
            let settles = !matches!(
                insert,
                Insert::Phaser(_)
                    | Insert::Vowel(_)
                    | Insert::Compressor(_)
                    | Insert::Lpg(_)
                    | Insert::Distort(Distort {
                        kind: DistortType::Down,
                        ..
                    })
            );
            assert!(
                falls_silent(insert, 30, settles).is_some(),
                "{insert:?} still sounds or moves after 30 s"
            );
        }
    }

    #[test]
    fn an_eq_at_its_narrowest_settles_too() {
        // The EQ at its narrowest and loudest.
        let narrow = Insert::Eq(Equaliser {
            low: 0xFF,
            mid: 0xFF,
            mid_q: 0xFF,
            high: 0xFF,
            ..Equaliser::DEFAULT
        });
        assert!(falls_silent(narrow, 30, true).is_some());
    }

    #[test]
    fn a_slot_has_settled_only_while_nothing_moves_it() {
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let keys = Keys::default();
        let silence = |state: &InsertState, input: [f32; 2]| {
            state
                .settled(input, same_sides(input), smoothing, RATE, &keys)
                .is_some()
        };
        let mut eq = holding(Insert::Eq(Equaliser::DEFAULT));
        assert!(
            !silence(&eq, [0.0; 2]),
            "its first frame works out its bands"
        );
        eq.process_sides([0.0; 2], true, smoothing, RATE, &keys);
        assert!(silence(&eq, [0.0; 2]), "and then it has settled");
        assert!(!silence(&eq, [0.5; 2]), "a sound moves it");
        eq.set(Insert::Eq(Equaliser {
            low: 0x20,
            ..Equaliser::DEFAULT
        }));
        assert!(!silence(&eq, [0.0; 2]), "a setting on its way moves it");
        let delay = holding(Insert::Delay(Delay::DEFAULT));
        assert!(silence(&delay, [0.0; 2]), "an empty delay has settled");
        assert!(
            !silence(&delay, [1e-30; 2]),
            "a long buffer settles only with nothing coming in"
        );
        let mut bypassed = holding(Insert::Eq(Equaliser::DEFAULT));
        bypassed.bypass(true);
        assert!(!silence(&bypassed, [0.0; 2]), "a switch fading moves it");
    }
}
