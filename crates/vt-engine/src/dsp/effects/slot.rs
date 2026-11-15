//! A track's slot: what it keeps for whichever effect it holds, and what plays it. An effect with
//! a long buffer keeps it in [`super::memory`]'s pool, and its DSP is in a file of its own; the
//! small effects are kept in every slot, and so is what [`Distort`](super::distort) keeps.

use vt_model::constants::{MAX_TRACKS, UNITY_LEVEL};
use vt_model::insert::{
    Beads, Chorus, Clouds, Comb, Compressor, CompressorType, Delay, Distort, DistortType, Echo,
    Equaliser, Exciter, Flanger, FreqShift, Grains, Insert, Limiter, LowPassGate, Phaser, Pitch,
    Plate, Resonator, Reverb, Rotary, Space, Spring, Stutter, Tape, Vowel, Warps,
};
use vt_model::song::level_gain;

use crate::dsp::blocks::sides::{SameBits, same_sides};
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::dynamics::compressor::Compressor as Dynamics;
use crate::dsp::dynamics::eq::ThreeBands;
use crate::dsp::dynamics::glue::PeakClip;
use crate::dsp::effects::distort::Distortion;
use crate::dsp::effects::exciter::Exciter as Excitement;
use crate::dsp::effects::memory::{Memory, MemoryPool, SILENT, SongPlace};
use crate::dsp::effects::phaser::Phaser as AllPasses;
use crate::dsp::filters::lpg::LowPassGate as Gate;
use crate::dsp::filters::vowel::Formants;
use crate::rng::Pcg32;

/// The note a slot's oscillator plays before the track has played one: middle C.
pub const DEFAULT_NOTE: f32 = 60.0;
/// How long switching a slot out or back in takes, in ms: long enough not to click, and short
/// enough that a step's lock of its bypass lands with the step.
pub const BYPASS_FADE_MS: f32 = 5.0;

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
    /// A track's latest frame, or silence for one that isn't there.
    pub fn of(&self, track: usize) -> [f32; 2] {
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
    /// left's the moment the sides differ ([`Sides`](crate::dsp::blocks::sides::Sides)). What
    /// comes out is exactly what working out both gives. After the sides have differed, their
    /// states can stay apart for a while, so it works out both until they're the same again, and
    /// the slot looks at what comes out.
    OneSide,
    /// One state for both sides, such as a compressor's linked detector, and the same done to
    /// each: what comes out is the same on both sides, with nothing more to save, so the slot
    /// passes that on without looking.
    Linked,
    /// It spreads the sides apart or mixes them, or hasn't been looked at for this: it works out
    /// both, and the slot looks at what comes out.
    Stereo,
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

/// Whether it's the same effect, so it carries on rather than starting afresh.
fn same_effect(before: Insert, after: Insert) -> bool {
    match (before, after) {
        (Insert::Distort(before), Insert::Distort(after)) => before.kind == after.kind,
        (Insert::Compressor(before), Insert::Compressor(after)) => before.kind == after.kind,
        (Insert::Empty, Insert::Empty)
        | (Insert::Delay(_), Insert::Delay(_))
        | (Insert::Reverb(_), Insert::Reverb(_))
        | (Insert::Chorus(_), Insert::Chorus(_))
        | (Insert::Eq(_), Insert::Eq(_))
        | (Insert::Limit(_), Insert::Limit(_))
        | (Insert::Phaser(_), Insert::Phaser(_))
        | (Insert::Flanger(_), Insert::Flanger(_))
        | (Insert::Tape(_), Insert::Tape(_))
        | (Insert::Plate(_), Insert::Plate(_))
        | (Insert::Pitch(_), Insert::Pitch(_))
        | (Insert::Grains(_), Insert::Grains(_))
        | (Insert::Warps(_), Insert::Warps(_))
        | (Insert::Lpg(_), Insert::Lpg(_))
        | (Insert::Reso(_), Insert::Reso(_))
        | (Insert::Comb(_), Insert::Comb(_))
        | (Insert::Vowel(_), Insert::Vowel(_))
        | (Insert::FreqShift(_), Insert::FreqShift(_))
        | (Insert::Spring(_), Insert::Spring(_))
        | (Insert::Space(_), Insert::Space(_))
        | (Insert::Rotary(_), Insert::Rotary(_))
        | (Insert::Exciter(_), Insert::Exciter(_))
        | (Insert::Stutter(_), Insert::Stutter(_))
        | (Insert::Clouds(_), Insert::Clouds(_))
        | (Insert::Beads(_), Insert::Beads(_))
        | (Insert::Echo(_), Insert::Echo(_)) => true,
        _ => false,
    }
}

impl InsertState {
    /// Take what's now in the slot, with what it keeps coming from and going back to the pool. If
    /// there's none free yet, it's silent and asks again next time.
    pub fn hold(&mut self, insert: Insert, pool: &mut MemoryPool, song: SongPlace, frames: u64) {
        // An empty slot taking an empty slot again has nothing to set: an empty slot's own
        // settings are never heard, and an effect put in it starts afresh (#187).
        if matches!((self.insert, insert), (Insert::Empty, Insert::Empty)) && self.memory.is_none()
        {
            return;
        }
        let frames_per_tick = song.frames_per_tick;
        if !same_effect(self.insert, insert) {
            self.give_back(pool);
        }
        let fresh = !same_effect(self.insert, insert);
        self.set(insert);
        // A phaser's sweep keeps time with the song, so it follows here, where the tempo is known.
        if let Insert::Phaser(settings) = insert {
            self.phaser.follow(settings, frames_per_tick);
            if fresh {
                self.phaser.settle();
            }
        }
        let started = self.memory.is_none();
        if started {
            self.memory = pool.take(insert);
            if let Some(memory) = &mut self.memory {
                memory.seed(self.rng);
            }
        }
        if let Some(memory) = &mut self.memory {
            memory.follow(insert, song, frames);
            if started {
                memory.settle();
            }
            // A knock rings the tank with nothing going in, and a stutter just triggered has a
            // piece to take, so neither may stay asleep.
            if memory.waking() {
                self.quiet = 0;
            }
        }
    }

    /// Whether taking the same settings again would change nothing: it has what its effect keeps,
    /// if it keeps anything, and nothing in it is about to wake.
    pub fn holding(&self) -> bool {
        match &self.memory {
            Some(memory) => !memory.waking(),
            None => !Memory::kept_by(self.insert),
        }
    }

    /// What taking its settings again would still change, while they stay the same: what goes by
    /// the frame.
    pub fn keep_time(&mut self, frames: u64, song: SongPlace) {
        if let Some(memory) = &mut self.memory {
            memory.keep_time(frames, song);
        }
    }

    /// A step has switched the slot's trigger on: a stutter in it starts again from what comes in
    /// next, even if it's on already. Nothing else has one.
    pub fn strike(&mut self) {
        if let Some(memory) = &mut self.memory {
            memory.strike();
            if memory.waking() {
                self.quiet = 0;
            }
        }
    }

    /// Start drawing random numbers again from a generator of the track's, as playing from the top
    /// does, so what it plays repeats.
    pub fn reseed(&mut self, rng: Pcg32) {
        self.rng = rng;
        if let Some(memory) = &mut self.memory {
            memory.seed(rng);
        }
    }

    /// Give what it keeps back to the pool.
    pub fn give_back(&mut self, pool: &mut MemoryPool) {
        if let Some(memory) = self.memory.take() {
            pool.put(memory);
        }
        self.quiet = u32::MAX;
    }

    /// Take what's now in the slot. A different effect starts afresh. What it keeps must be given
    /// back first, as this drops it.
    pub fn set(&mut self, insert: Insert) {
        let (amount, mix, gain) = match insert {
            Insert::Empty => (0, 0, UNITY_LEVEL),
            // `Down` sweeps how long it holds as the shaping modes sweep how hard they drive.
            Insert::Distort(Distort {
                kind: DistortType::Down,
                rate,
                mix,
                gain,
                ..
            }) => (rate, mix, gain),
            Insert::Distort(Distort {
                amount, mix, gain, ..
            }) => (amount, mix, gain),
            Insert::Delay(Delay { mix, gain, .. })
            | Insert::Reverb(Reverb { mix, gain, .. })
            | Insert::Chorus(Chorus { mix, gain, .. })
            | Insert::Limit(Limiter { mix, gain, .. })
            | Insert::Phaser(Phaser { mix, gain, .. })
            | Insert::Flanger(Flanger { mix, gain, .. })
            | Insert::Tape(Tape { mix, gain, .. })
            | Insert::Plate(Plate { mix, gain, .. })
            | Insert::Spring(Spring { mix, gain, .. })
            | Insert::Echo(Echo { mix, gain, .. })
            | Insert::Stutter(Stutter { mix, gain, .. })
            | Insert::Exciter(Exciter { mix, gain, .. })
            | Insert::Rotary(Rotary { mix, gain, .. })
            | Insert::Space(Space { mix, gain, .. })
            | Insert::Pitch(Pitch { mix, gain, .. })
            | Insert::Grains(Grains { mix, gain, .. })
            | Insert::Warps(Warps { mix, gain, .. })
            | Insert::Lpg(LowPassGate { mix, gain, .. })
            | Insert::Reso(Resonator { mix, gain, .. })
            | Insert::Comb(Comb { mix, gain, .. })
            | Insert::Vowel(Vowel { mix, gain, .. })
            | Insert::FreqShift(FreqShift { mix, gain, .. }) => (0, mix, gain),
            // A compressor's makeup is its own, on what it turned down, with nothing after the mix.
            Insert::Compressor(Compressor { mix, .. }) => (0, mix, UNITY_LEVEL),
            // Clouds blends as the module does, inside it.
            Insert::Clouds(Clouds { gain, .. }) | Insert::Beads(Beads { gain, .. }) => {
                (0, 0xFF, gain)
            }
            // An EQ is all wet: what it leaves alone comes through as it is.
            Insert::Eq(Equaliser { gain, .. }) => (0, 0xFF, gain),
        };
        let (amount, mix, gain) = (
            f32::from(amount) / 255.0,
            f32::from(mix) / 255.0,
            level_gain(f32::from(gain)),
        );
        if !same_effect(self.insert, insert) {
            let (on, note, rng) = (self.on, self.note, self.rng);
            *self = Self::default();
            self.on = on;
            self.note = note;
            self.rng = rng;
            self.amount = Smoothed::at(amount);
            self.mix = Smoothed::at(mix);
            self.gain = Smoothed::at(gain);
            if let Insert::Distort(settings) = insert {
                self.distort = Distortion::starting(settings);
            }
        }
        if let Insert::Distort(settings) = insert {
            let fresh = !same_effect(self.insert, insert);
            self.distort.follow(settings, fresh);
        }
        if let Insert::Eq(settings) = insert {
            let fresh = !same_effect(self.insert, insert);
            self.eq.follow(settings);
            if fresh {
                self.eq.settle();
            }
        }
        // Only Glue has its clip, which the other types keep while they're shown.
        let clips = matches!(
            insert,
            Insert::Compressor(Compressor {
                kind: CompressorType::Glue,
                peak_clip: 1..,
                ..
            })
        );
        self.peak_clip.switch(clips);
        if !same_effect(self.insert, insert) {
            self.peak_clip.settle();
        }
        if let Insert::Vowel(settings) = insert {
            let fresh = !same_effect(self.insert, insert);
            self.vowel.follow(settings);
            if fresh {
                self.vowel.settle();
            }
        }
        if let Insert::Exciter(settings) = insert {
            let fresh = !same_effect(self.insert, insert);
            self.exciter.follow(settings);
            if fresh {
                self.exciter.settle();
            }
        }
        self.insert = insert;
        self.amount.target = amount;
        self.mix.target = mix;
        self.gain.target = gain;
    }

    /// How far a compressor or limiter in the slot is turning down now, in dB, for a meter. Any
    /// other effect, or a bypassed one, turns down nothing.
    pub fn reduction(&self) -> f32 {
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
    pub fn opening(&self) -> Option<(f32, f32)> {
        matches!(self.insert, Insert::Lpg(_)).then(|| self.gate.opening())
    }

    /// The track's note as it's heard, with its `Pitch`, which a resonator, comb, vowel filter or
    /// oscillator in the slot follows.
    pub fn note(&self) -> f32 {
        self.note
    }

    /// Whether it's got something left to play with nothing going in.
    #[inline]
    pub fn ringing(&self, rate: u32) -> bool {
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
    pub fn settled(
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
    pub fn rested(&mut self, frames: u32) {
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
    pub fn follow_note(&mut self, note: f32) {
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
    pub fn is_empty(&self) -> bool {
        matches!(self.insert, Insert::Empty)
    }

    /// What an empty slot gives for a frame while it isn't fading, which is all
    /// [`InsertState::process_sides`] does with one: the frame as it came, with a NaN or an
    /// infinity heard as silence, and whether its sides are the same. It keeps nothing, so it's
    /// passed by without working through the rest (#187). None for any other slot.
    #[inline]
    pub fn passed_through(&self, input: [f32; 2], same: bool) -> Option<([f32; 2], bool)> {
        if !self.is_empty() || self.on.value != self.on.target {
            return None;
        }
        let out = input.map(|sample| if sample.is_finite() { sample } else { 0.0 });
        Some((out, same || same_sides(out)))
    }

    /// What its effect does with a sound that's the same on both sides (decision 147). Every
    /// effect says, so a new one has to choose: `Mono::Stereo` is always safe, and
    /// `tests/all/mono.rs` checks every other claim.
    pub fn mono(&self) -> Mono {
        match self.insert {
            // Each of its types works each side alone, the same way, with nothing between them.
            Insert::Distort(_)
            | Insert::Eq(_)
            | Insert::Lpg(_)
            | Insert::Vowel(_)
            | Insert::Exciter(_) => Mono::OneSide,
            // One detector for both sides, and one gain on each.
            Insert::Empty | Insert::Compressor(_) => Mono::Linked,
            // These spread the sides apart or mix them, or keep their sides in the effect pool's
            // buffers, which haven't been looked at for this.
            Insert::Delay(_)
            | Insert::Reverb(_)
            | Insert::Chorus(_)
            | Insert::Limit(_)
            | Insert::Phaser(_)
            | Insert::Flanger(_)
            | Insert::Tape(_)
            | Insert::Plate(_)
            | Insert::Pitch(_)
            | Insert::Grains(_)
            | Insert::Warps(_)
            | Insert::Reso(_)
            | Insert::Comb(_)
            | Insert::FreqShift(_)
            | Insert::Spring(_)
            | Insert::Space(_)
            | Insert::Rotary(_)
            | Insert::Stutter(_)
            | Insert::Clouds(_)
            | Insert::Beads(_)
            | Insert::Echo(_) => Mono::Stereo,
        }
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
        let distort = match self.insert {
            Insert::Empty => return input,
            Insert::Compressor(settings) => {
                // It hears its own input, or another track's as it was after its slots.
                let key = settings
                    .source_track(keys.sounds)
                    .map_or(input, |track| keys.of(track));
                // Its makeup is in what comes out wet, so compressing in parallel turns up only
                // what it turned down.
                let wet = self.dynamics.process(input, key, settings, smoothing, rate);
                let mix = self.mix.next(smoothing);
                // Glue's clip is the last thing it does, after its makeup and the mix.
                let mixed = [
                    wet[0] * mix + input[0] * (1.0 - mix),
                    wet[1] * mix + input[1] * (1.0 - mix),
                ];
                return self.peak_clip.process(mixed, rate);
            }
            Insert::Lpg(settings) => {
                // It's opened by its own input, or another track's as it was after its slots.
                let key = settings
                    .source_track(keys.sounds)
                    .map_or(input, |track| keys.of(track));
                let wet = self
                    .gate
                    .process(input, same, key, settings, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Limit(_) => {
                // The dry sound comes out as late as the limited one, so blending them doesn't comb.
                let [wet, dry] = self.limit(input, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + dry[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + dry[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Warps(settings) => {
                let wet = self.warps(input, settings, smoothing, keys);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Reso(_) | Insert::Comb(_) | Insert::FreqShift(_) => {
                let wet = self.resonate(input, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Vowel(_) => {
                let wet = self.vowel.process(input, same, self.note, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            // What it makes is added to the sound, as an Aural Exciter's is, so `Mix` at `00` is
            // the sound alone and at `FF` the sound with all it makes.
            Insert::Exciter(_) => {
                let wet = self.exciter.process(input, same, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (input[0] + wet[0] * mix) * gain,
                    (input[1] + wet[1] * mix) * gain,
                ];
            }
            Insert::Phaser(_) => {
                let wet = self.phaser.process(input, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Eq(_) => {
                let gain = self.gain.next(smoothing);
                return self
                    .eq
                    .process(input, same, smoothing, rate)
                    .map(|sample| sample * gain);
            }
            // What comes out is already the module's blend of the sound and what it made, so the
            // slot only turns it up or down. Until its recording comes, the sound passes.
            Insert::Clouds(_) | Insert::Beads(_) => {
                let gain = self.gain.next(smoothing);
                let out = if self.memory.is_some() {
                    self.ring(input, rate)
                } else {
                    input
                };
                return out.map(|sample| sample * gain);
            }
            Insert::Distort(settings) => Some(settings),
            Insert::Delay(_)
            | Insert::Reverb(_)
            | Insert::Chorus(_)
            | Insert::Flanger(_)
            | Insert::Tape(_)
            | Insert::Plate(_)
            | Insert::Spring(_)
            | Insert::Space(_)
            | Insert::Pitch(_)
            | Insert::Grains(_)
            | Insert::Rotary(_)
            | Insert::Stutter(_)
            | Insert::Echo(_) => None,
        };
        let amount = self.amount.next(smoothing);
        let mix = self.mix.next(smoothing);
        let gain = self.gain.next(smoothing);
        let Some(settings) = distort else {
            let wet = self.ring(input, rate);
            return [
                (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
            ];
        };
        let wet = self
            .distort
            .process(input, same, settings, amount, self.note, smoothing, rate);
        // Written so all wet and all dry are exact.
        [
            (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
            (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn sine(hertz: f32, level: f32, frames: usize) -> impl Iterator<Item = f32> {
        (0..frames).map(move |frame| {
            (frame as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * level
        })
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |most: f32, s| most.max(s.abs()))
    }

    #[test]
    fn an_empty_slot_leaves_the_sound_alone() {
        let mut state = InsertState::default();
        for sample in sine(220.0, 0.5, 1000) {
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
            for sample in sine(220.0, 0.5, RATE as usize / 4) {
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
            for sample in sine(220.0, 0.5, RATE as usize / 20) {
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
    fn falls_silent(insert: Insert, seconds: usize, settles: bool) -> Option<f32> {
        let smoothing = crate::dsp::blocks::smoother::coefficient(RATE);
        let keys = Keys::default();
        let mut state = holding(insert);
        let mut noise = 1_u32;
        for sample in sine(220.0, 0.9, RATE as usize / 5) {
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
