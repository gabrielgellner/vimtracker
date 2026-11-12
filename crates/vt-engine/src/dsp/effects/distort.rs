//! The `Distort` effect. Most of its shapes come from Vital's `distortion.cpp`, by Matt Tytel, GPL 3
//! or later: Copyright 2013-2019 Matt Tytel. `Soft` comes from Plaits' `dsp/fx/overdrive.h`, by Emilie
//! Gillet, MIT: Copyright 2014 Emilie Gillet. The other types' scaling, the mix and state are this
//! program's own. The guitar pedals, `Overdrive` and `Distortion`, are in [`super::pedal`].
//!
//! At the higher rate, `Hard`, `Fold` and `Sine` are anti-aliased with their antiderivatives, as
//! sst-waveshapers' `ADAA.h` and `Wavefolders.h` do (GPL 3 or later, Copyright the Surge Synth
//! Team). Surge's one-pole before its shapers (`QuadFilterChain.cpp:63`) is left out: here it took
//! 3 dB from 10 kHz and 6 dB from 20 kHz at any drive, for less aliasing only on low notes, where
//! there's little left (decision 187). `Crush` can compand as mu-law does, and `Down` can hold at a whole multiple
//! of the note, after sst-effects' `BitCrusher.h`.

use vt_model::constants::UNITY_LEVEL;
use vt_model::insert::{
    Chorus, Comb, Compressor, CompressorType, Delay, Distort, DistortType, Equaliser, Flanger,
    FreqShift, Grains, Insert, Limiter, LowPassGate, Phaser, Pitch, Plate, Resonator, Reverb,
    Rotary, Space, Spring, Stutter, Tape, Vowel, Warps,
};
use vt_model::song::level_gain;

use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::sides::{SameBits, Sides, same_sides};
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::dynamics::compressor::Compressor as Dynamics;
use crate::dsp::dynamics::eq::ThreeBands;
use crate::dsp::dynamics::glue::PeakClip;
use crate::dsp::effects::memory::{Memory, MemoryPool, SILENT};
use crate::dsp::effects::pedal::Pedal;
use crate::dsp::effects::phaser::Phaser as AllPasses;
use crate::dsp::filters::lpg::LowPassGate as Gate;
use crate::dsp::filters::vowel::Formants;
use crate::dsp::voices::drums::soft_clip;
use crate::track_state::Keys;

/// How much `Amount` turns up what's clipped or folded, at full, in dB.
const MOST_DRIVE_DB: f32 = 30.0;
/// The longest `Down` holds a sample, in seconds, so it's the same at any device rate.
const LONGEST_HOLD: f32 = 0.01;
/// The note a slot's oscillator plays before the track has played one: middle C.
const DEFAULT_NOTE: f32 = 60.0;
/// How many octaves over the note `Down` holds at with `Track` on, at `00`. At `FF` it holds once
/// a cycle of the note.
const TRACKED_OCTAVES: f32 = 8.0;
/// Mu-law's `μ`, as 8-bit telephone and sampler audio has it.
const MU: f32 = 255.0;
/// How close two samples at the higher rate can be before the antiderivatives' difference is
/// taken as no better than the shape itself, halfway between them.
const ADAA_CLOSEST: f64 = 1e-4;
/// How long switching a slot out or back in takes, in ms: long enough not to click, and short
/// enough that a step's lock of its bypass lands with the step.
pub const BYPASS_FADE_MS: f32 = 5.0;

/// What an effect does with a sound that's the same on both sides, bit for bit, as a mono voice's
/// is until something spreads it (decision 147). A slot is told whether what comes in is, by the
/// slot before or by the track looking once, and an effect that keeps it so tells the slot after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mono {
    /// Each side goes through the same steps with its own state and nothing between them, so it
    /// works the left out alone and gives it to both, keeping the right's state to copy from the
    /// left's the moment the sides differ ([`Sides`]). What comes out is exactly what working out
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
    /// Each side's oversampling for the shapes that need it, and what its shaping keeps, once for
    /// both while a sound is the same on both sides.
    oversampler: Sides<Shaper>,
    /// `Crush`'s bit depth, eased so moving it doesn't step.
    bits: Smoothed,
    held: [f32; 2],
    /// What came in the frame before, for `Down` to hold from between frames.
    before: [f32; 2],
    /// How many frames since `Down` last took a sample.
    since: f32,
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
    /// A guitar pedal's filters and oversampling, small enough as well.
    pedal: Pedal,
    /// How far the slot is switched in: `1` in, `0` bypassed. It crossfades in a straight line
    /// over [`BYPASS_FADE_MS`], so switching doesn't click.
    on: Smoothed,
    /// The track's note, as a MIDI note with its `Pitch` added, for an oscillator or resonator to
    /// follow.
    note: f32,
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
            oversampler: Sides::default(),
            bits: Smoothed::at(0.0),
            held: [0.0; 2],
            before: [0.0; 2],
            since: 0.0,
            eq: ThreeBands::default(),
            dynamics: Dynamics::default(),
            peak_clip: PeakClip::default(),
            phaser: AllPasses::default(),
            gate: Gate::default(),
            vowel: Formants::default(),
            pedal: Pedal::default(),
            on: Smoothed::at(1.0),
            note: DEFAULT_NOTE,
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
        | (Insert::Stutter(_), Insert::Stutter(_)) => true,
        _ => false,
    }
}

/// How much `Amount` turns up the signal: 0 dB at `00`.
#[inline]
fn drive_of(amount: f32) -> f32 {
    10.0_f32.powf(amount * MOST_DRIVE_DB / 20.0)
}

/// What `Soft` does before and after it clips. The overdrive this comes from is about unity with a
/// quiet sound at half way, and quiet below that, so `Amount` covers its top half: from clean at
/// `00`, to pushing hard at `FF`. Its gain after the clip holds back a loud sound's level, but it
/// can't do that for a quiet one, which comes up as the clip takes hold.
#[inline]
fn overdrive_gains(amount: f32) -> (f32, f32) {
    saturating_gains(0.5 + amount * 0.5)
}

/// The gains before and after a soft clip for a drive from `0` to `1`, as Plaits' overdrive and
/// Warps' saturating amplifier both have them: about unity a little past half way, and the gain
/// after the clip holding back a loud sound's level.
#[inline]
pub(crate) fn saturating_gains(drive: f32) -> (f32, f32) {
    let drive_2 = drive * drive;
    let low = drive * 0.5;
    let high = drive_2 * drive_2 * drive * 24.0;
    let pre = low + (high - low) * drive_2;
    let squashed = drive * (2.0 - drive);
    (pre, 1.0 / soft_clip(0.33 + squashed * (pre - 0.33)))
}

/// The distance between the steps `Crush` rounds to, for some bits, which needn't be whole.
#[inline]
fn crush_step(bits: f32) -> f32 {
    (1.0 - bits).exp2()
}

/// A sample crushed to steps this far apart, through mu-law if it's on: squeezed by its log, so
/// quiet sounds get finer steps than loud ones, rounded, and opened out again.
#[inline]
fn crushed(sample: f32, step: f32, mu_law: bool) -> f32 {
    if !mu_law {
        return (sample / step).round() * step;
    }
    let squeezed = (MU * sample.abs()).ln_1p() / MU.ln_1p();
    let rounded = (squeezed / step).round() * step;
    ((rounded * MU.ln_1p()).exp_m1() / MU).copysign(sample)
}

/// What a shape that's anti-aliased integrates to, at a driven sample: the antiderivative of
/// `Hard`'s clip, of `Fold`'s triangle, which repeats every 4, so its antiderivative, whose mean is
/// nothing, does too, and of `Sine`, which is `sin(πx/2)`. Each is nothing at nothing, as a
/// [`Shaper`] starts there.
#[inline]
fn integral(kind: DistortType, x: f64) -> f64 {
    match kind {
        DistortType::Sine => {
            std::f64::consts::FRAC_2_PI * (1.0 - (std::f64::consts::FRAC_PI_2 * x).cos())
        }
        DistortType::Hard => {
            if x.abs() <= 1.0 {
                0.5 * x * x
            } else {
                x.abs() - 0.5
            }
        }
        _ => {
            // `Fold` is `|2 - 4t| - 1` at `t`, the place in its turn: `1 - 4t` to half way,
            // then `4t - 3`.
            let turn = x * 0.25 + 0.75;
            let t = turn - turn.floor();
            let within_turn = if t < 0.5 {
                t - 2.0 * t * t
            } else {
                2.0 * t * t - 3.0 * t + 1.0
            };
            4.0 * within_turn + 0.5
        }
    }
}

/// What one side's shaping keeps at the higher rate: the oversampler, and the last sample shaped
/// and its antiderivative.
#[derive(Debug, Clone, Copy, Default)]
struct Shaper {
    oversampler: Oversampler,
    last: f64,
    last_integral: f64,
}

impl SameBits for Shaper {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self.oversampler.same_bits(&other.oversampler)
            && self.last.to_bits() == other.last.to_bits()
            && self.last_integral.to_bits() == other.last_integral.to_bits()
    }
}

impl Shaper {
    /// One sample through `shape` at twice the rate, driven by `drive`. `Hard`, `Fold` and `Sine`
    /// are anti-aliased.
    #[inline]
    fn process(
        &mut self,
        input: f32,
        kind: DistortType,
        drive: f32,
        shape: impl Fn(f32) -> f32,
    ) -> f32 {
        let Self {
            oversampler,
            last,
            last_integral,
        } = self;
        oversampler.process(input, |x| {
            let driven = x * drive;
            if !matches!(
                kind,
                DistortType::Hard | DistortType::Fold | DistortType::Sine
            ) {
                return shape(driven);
            }
            // The mean of the shape between this sample and the last: the difference of its
            // antiderivative over the distance between them, or the shape halfway once they're
            // too close for that to be any better.
            let x = f64::from(driven);
            let now = integral(kind, x);
            let apart = x - *last;
            let out = if apart.abs() < ADAA_CLOSEST {
                shape((0.5 * (x + *last)) as f32)
            } else {
                ((now - *last_integral) / apart) as f32
            };
            (*last, *last_integral) = (x, now);
            out
        })
    }
}

/// A shape that needs a higher rate, given what's already been driven.
#[inline]
fn shaped(kind: DistortType, driven: f32) -> f32 {
    match kind {
        DistortType::Soft => soft_clip(driven),
        DistortType::Hard => within(driven, -1.0, 1.0),
        DistortType::Fold => {
            let adjusted = driven * 0.25 + 0.75;
            (adjusted - adjusted.floor()).mul_add(-4.0, 2.0).abs() - 1.0
        }
        DistortType::Sine => {
            let adjusted = driven * -0.25 + 0.5;
            ((adjusted - adjusted.floor()) * std::f32::consts::TAU).sin()
        }
        // These don't shape at the higher rate here: the pedals have their own.
        DistortType::Crush
        | DistortType::Down
        | DistortType::Overdrive
        | DistortType::Distortion => driven,
    }
}

impl InsertState {
    /// Take what's now in the slot, with what it keeps coming from and going back to the pool. If
    /// there's none free yet, it's silent and asks again next time.
    pub fn hold(
        &mut self,
        insert: Insert,
        pool: &mut MemoryPool,
        frames_per_tick: f64,
        frames: u64,
    ) {
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
        }
        if let Some(memory) = &mut self.memory {
            memory.follow(insert, frames_per_tick, frames);
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
            | Insert::Stutter(Stutter { mix, gain, .. })
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
            // An EQ is all wet: what it leaves alone comes through as it is.
            Insert::Eq(Equaliser { gain, .. }) => (0, 0xFF, gain),
        };
        let (amount, mix, gain) = (
            f32::from(amount) / 255.0,
            f32::from(mix) / 255.0,
            level_gain(f32::from(gain)),
        );
        if !same_effect(self.insert, insert) {
            let (on, note) = (self.on, self.note);
            *self = Self::default();
            self.on = on;
            self.note = note;
            self.amount = Smoothed::at(amount);
            self.mix = Smoothed::at(mix);
            self.gain = Smoothed::at(gain);
            if let Insert::Distort(settings) = insert {
                self.bits = Smoothed::at(f32::from(settings.bits));
            }
        }
        if let Insert::Distort(settings) = insert {
            self.bits.target = f32::from(settings.bits);
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
        if let Insert::Distort(settings) = insert
            && settings.kind.is_pedal()
        {
            let fresh = !same_effect(self.insert, insert);
            self.pedal.follow(f32::from(settings.tone) / 255.0);
            if fresh {
                self.pedal.settle();
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

    /// Whether it's got something left to play with nothing going in.
    #[inline]
    pub fn ringing(&self, rate: u32) -> bool {
        self.memory
            .as_ref()
            .is_some_and(|memory| self.quiet < memory.quiet_frames(rate))
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
        self.bits.settle();
        self.mix.settle();
        self.gain.settle();
        self.phaser.settle();
        self.pedal.settle();
        self.peak_clip.settle();
    }

    /// Whether the slot has no effect in it.
    pub fn is_empty(&self) -> bool {
        matches!(self.insert, Insert::Empty)
    }

    /// What its effect does with a sound that's the same on both sides (decision 147). Every
    /// effect says, so a new one has to choose: `Mono::Stereo` is always safe, and
    /// `tests/mono.rs` checks every other claim.
    pub fn mono(&self) -> Mono {
        match self.insert {
            // Each of its types works each side alone, the same way, with nothing between them.
            Insert::Distort(_) | Insert::Eq(_) | Insert::Lpg(_) | Insert::Vowel(_) => Mono::OneSide,
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
            | Insert::Stutter(_) => Mono::Stereo,
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
        let (kind, settings) = match self.insert {
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
            Insert::Distort(settings) => (Some(settings.kind), settings),
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
            | Insert::Stutter(_) => (None, Distort::DEFAULT),
        };
        let amount = self.amount.next(smoothing);
        let mix = self.mix.next(smoothing);
        let gain = self.gain.next(smoothing);
        let Some(kind) = kind else {
            let wet = self.ring(input, rate);
            return [
                (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
            ];
        };
        let wet = match kind {
            DistortType::Crush => {
                let step = crush_step(self.bits.next(smoothing));
                input.map(|sample| crushed(sample, step, settings.mu_law != 0))
            }
            DistortType::Down => {
                // Following the note, it holds at a whole number of times the note's frequency, so
                // what it folds back lands on the note's harmonics.
                let period = if settings.track != 0 {
                    let hertz = 440.0 * ((self.note - 69.0) / 12.0).exp2();
                    let times = (TRACKED_OCTAVES * (1.0 - amount)).exp2().round();
                    rate as f32 / (hertz * times)
                } else {
                    amount * amount * LONGEST_HOLD * rate as f32
                };
                // A hold of one frame or less is every sample, which leaves the sound as it is.
                let period = period.max(1.0);
                self.since += 1.0;
                if self.since >= period {
                    self.since -= period;
                    // Its time came `since` frames ago, so it holds what the sound was then,
                    // between this frame and the last, as Airwindows' DeRez2 does: a hold that
                    // isn't a whole number of frames then keeps to its time rather than jittering.
                    let ago = self.since.min(1.0);
                    let before = self.before;
                    self.held = [0, 1].map(|side| input[side] + (before[side] - input[side]) * ago);
                }
                self.before = input;
                self.held
            }
            DistortType::Overdrive | DistortType::Distortion => self
                .pedal
                .process(kind, input, same, amount, smoothing, rate),
            DistortType::Soft => {
                let (pre, post) = overdrive_gains(amount);
                self.oversampler.process(input, same, |side, input| {
                    side.process(input, kind, pre, soft_clip) * post
                })
            }
            _ => {
                let drive = drive_of(amount);
                self.oversampler.process(input, same, |side, input| {
                    side.process(input, kind, drive, |x| shaped(kind, x))
                })
            }
        };
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

    /// A distortion with `amount` as how hard each mode goes: its drive, `Down`'s hold, or fewer
    /// of `Crush`'s bits, from 12 at `00` to 2 at `FF`.
    fn state(kind: DistortType, amount: u8, mix: u8) -> InsertState {
        let mut state = InsertState::default();
        state.set(Insert::Distort(Distort {
            kind,
            amount,
            bits: 12 - (f32::from(amount) / 25.5).round() as u8,
            rate: amount,
            mix,
            ..Distort::default()
        }));
        state
    }

    fn sine(hertz: f32, level: f32, frames: usize) -> impl Iterator<Item = f32> {
        (0..frames).map(move |frame| {
            (frame as f32 / RATE as f32 * hertz * std::f32::consts::TAU).sin() * level
        })
    }

    fn played(kind: DistortType, amount: u8, mix: u8, level: f32) -> Vec<f32> {
        let mut state = state(kind, amount, mix);
        sine(220.0, level, RATE as usize / 10)
            .map(|sample| state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |most: f32, s| most.max(s.abs()))
    }

    /// How far apart two runs are, past the oversampler's short delay.
    fn differs(a: &[f32], b: &[f32]) -> f32 {
        a.iter()
            .zip(b)
            .skip(64)
            .fold(0.0, |most: f32, (a, b)| most.max((a - b).abs()))
    }

    /// How many different values there are.
    fn distinct(samples: &[f32]) -> usize {
        // Adding zero turns -0 into 0, as they're the same value.
        let mut seen: Vec<u32> = samples.iter().map(|s| (s + 0.0).to_bits()).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
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

    /// The average level, past the oversampler's short delay.
    fn rms(samples: &[f32]) -> f32 {
        let tail = &samples[64..];
        (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
    }

    #[test]
    fn soft_is_about_clean_at_none_and_a_loud_sound_stays_about_as_loud() {
        let plain: Vec<f32> = sine(220.0, 0.25, RATE as usize / 10).collect();
        let clean = played(DistortType::Soft, 0x00, 0xFF, 0.25);
        let db = |a: f32, b: f32| 20.0 * (a / b).log10();
        let at_none = db(rms(&clean), rms(&plain));
        assert!(at_none.abs() < 1.0, "{at_none} dB at 00");
        // A loud sound stays within a few dB of itself. A clip can only hold back what reaches it,
        // so a quieter sound does get louder as it's turned up.
        let change = |level: f32, amount: u8| {
            let dry = level / std::f32::consts::SQRT_2;
            db(rms(&played(DistortType::Soft, amount, 0xFF, level)), dry)
        };
        for amount in [0x00, 0x40, 0x80, 0xC0, 0xFF] {
            assert!(
                change(1.0, amount).abs() < 4.0,
                "{} dB at {amount:02X}",
                change(1.0, amount)
            );
            assert!(
                change(0.5, amount).abs() < 10.0,
                "{} dB at {amount:02X}",
                change(0.5, amount)
            );
        }
        // And it does more as it goes, turning towards a square at the top.
        let bent = |amount| {
            let out = played(DistortType::Soft, amount, 0xFF, 0.5);
            peak(&out) / rms(&out)
        };
        assert!(bent(0xFF) < bent(0x80), "{} and {}", bent(0xFF), bent(0x80));
        assert!(bent(0x80) < bent(0x00), "{} and {}", bent(0x80), bent(0x00));
    }

    #[test]
    fn the_overdrive_matches_where_it_comes_from() {
        // At full drive, 24 times in, and whatever makes a full clip come out at one.
        let (pre, post) = overdrive_gains(1.0);
        assert!((pre - 24.0).abs() < 1e-3, "{pre}");
        assert!((post - 1.0).abs() < 1e-3, "{post}");
        // At half drive, about unity for a quiet sound.
        let (pre, post) = overdrive_gains(0.0);
        assert!((pre * post - 1.0).abs() < 0.1, "{}", pre * post);
    }

    #[test]
    fn clips_stay_in_range_and_amount_makes_them_louder_up_to_it() {
        for kind in [DistortType::Soft, DistortType::Hard] {
            let gentle = peak(&played(kind, 0x00, 0xFF, 0.1));
            let hard = peak(&played(kind, 0xFF, 0xFF, 0.1));
            assert!(hard > gentle * 3.0, "{kind:?}: {gentle} to {hard}");
            // The oversampler's filters overshoot a corner very slightly.
            assert!(hard <= 1.02, "{kind:?}: {hard}");
        }
        // At unity with a quiet signal, hard clipping doesn't touch it.
        let clean = played(DistortType::Hard, 0x00, 0xFF, 0.5);
        assert!((peak(&clean) - 0.5).abs() < 0.02, "{}", peak(&clean));
    }

    #[test]
    fn folds_turn_back_inside_the_range() {
        for kind in [DistortType::Fold, DistortType::Sine] {
            let folded = played(kind, 0xFF, 0xFF, 1.0);
            // The halfband filters overshoot what's folded hardest a little, the anti-aliased sine's
            // most.
            assert!(peak(&folded) <= 1.07, "{kind:?}: {}", peak(&folded));
            // Hard driven, it crosses zero far more than the 220 Hz going in.
            let crossings = folded.windows(2).filter(|p| p[0] * p[1] < 0.0).count();
            assert!(crossings > 44 * 5, "{kind:?}: {crossings}");
        }
    }

    #[test]
    fn a_small_fold_follows_its_input() {
        // Before a fold, the triangle is a straight line through zero.
        let fold = shaped(DistortType::Fold, 0.25);
        assert!((fold - 0.25).abs() < 1e-5, "{fold}");
        assert!(shaped(DistortType::Sine, 0.0).abs() < 1e-5);
        assert!((shaped(DistortType::Sine, 1.0) - 1.0).abs() < 1e-5);
        assert!(
            (shaped(DistortType::Fold, 2.0)).abs() < 1e-5,
            "folded back to zero"
        );
    }

    #[test]
    fn crush_has_fewer_steps_with_more_amount() {
        let light = distinct(&played(DistortType::Crush, 0x00, 0xFF, 1.0));
        let heavy = distinct(&played(DistortType::Crush, 0xFF, 0xFF, 1.0));
        assert!(heavy <= 5, "{heavy} values, for steps of a half");
        assert!(light > heavy * 10, "{light} and {heavy}");
    }

    #[test]
    fn crush_eases_between_bit_depths_so_moving_it_doesnt_step() {
        let mut state = state(DistortType::Crush, 0x00, 0xFF);
        let settings = Insert::Distort(Distort {
            kind: DistortType::Crush,
            bits: 2,
            mix: 0xFF,
            ..Distort::default()
        });
        state.set(settings);
        // From 12 bits, a frame on it's still fine; settled, its steps are a half.
        let out = state.process([0.3; 2], 0.0001, RATE, &Keys::default())[0];
        assert!((out - 0.3).abs() < 0.01, "{out}");
        state.settle();
        let out = state.process([0.3; 2], 0.0001, RATE, &Keys::default())[0];
        assert_eq!(out, 0.5);
        // Between whole bits, the steps are between theirs.
        assert!(crush_step(8.5) < crush_step(8.0) && crush_step(8.5) > crush_step(9.0));
    }

    #[test]
    fn mu_law_keeps_finer_steps_for_quiet_sounds() {
        let quiet = |mu_law: u8| {
            let mut state = InsertState::default();
            state.set(Insert::Distort(Distort {
                kind: DistortType::Crush,
                bits: 4,
                mix: 0xFF,
                mu_law,
                ..Distort::default()
            }));
            sine(220.0, 0.02, RATE as usize / 10)
                .map(|x| state.process([x; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
                .collect::<Vec<f32>>()
        };
        // Linear steps of an eighth gate a sound this quiet; mu-law's are fine enough near nothing.
        assert_eq!(peak(&quiet(0)), 0.0);
        assert!(distinct(&quiet(1)) >= 3, "{}", distinct(&quiet(1)));
        // A loud one comes back about as loud, its steps coarser at the top.
        assert_eq!(crushed(1.0, crush_step(4.0), true), 1.0);
        assert!((crushed(-0.5, crush_step(4.0), true) + 0.5).abs() < 0.1);
    }

    #[test]
    fn down_following_the_note_folds_back_onto_its_harmonics() {
        // A 440 Hz sine held at a whole number of times 440 Hz: what it folds back lands on 440's
        // multiples and nowhere between.
        let level = |track: u8, probe: f32| {
            let mut state = InsertState::default();
            state.set(Insert::Distort(Distort {
                kind: DistortType::Down,
                rate: 0x90,
                mix: 0xFF,
                track,
                ..Distort::default()
            }));
            state.follow_note(69.0);
            state.settle();
            let frames = RATE as usize;
            let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
            for (frame, sample) in sine(440.0, 0.5, frames).enumerate() {
                let out = state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0];
                let phase =
                    frame as f64 / f64::from(RATE) * f64::from(probe) * std::f64::consts::TAU;
                real += f64::from(out) * phase.cos();
                imaginary += f64::from(out) * phase.sin();
            }
            (real * real + imaginary * imaginary).sqrt() / frames as f64
        };
        let between = [2_970.0, 5_830.0, 9_130.0];
        let worst_between = |track| between.iter().map(|&f| level(track, f)).fold(0.0, f64::max);
        let fundamental = level(1, 440.0);
        assert!(
            worst_between(1) < fundamental * 1e-3,
            "{}",
            worst_between(1) / fundamental
        );
        // Left free, it holds at about 313 Hz, which folds 440 back to about 127 Hz, between them.
        let amount = f32::from(0x90_u8) / 255.0;
        let hold = 1.0 / (amount * amount * LONGEST_HOLD);
        let folded = level(0, 440.0 - hold);
        assert!(folded > fundamental * 1e-2, "{}", folded / fundamental);
    }

    #[test]
    fn down_holds_for_a_time_whatever_the_rate() {
        // The same hold in time is twice the frames at twice the rate.
        let run = |rate: u32| {
            let mut state = state(DistortType::Down, 0xFF, 0xFF);
            (0..rate as usize / 10)
                .map(|frame| {
                    let t = frame as f32 / rate as f32;
                    let sample = (t * 220.0 * std::f32::consts::TAU).sin();
                    state.process([sample; 2], NO_SMOOTHING, rate, &Keys::default())[0]
                })
                .collect::<Vec<f32>>()
        };
        let changes = |out: &[f32]| out.windows(2).filter(|p| p[0] != p[1]).count();
        let (slow, fast) = (changes(&run(48_000)), changes(&run(96_000)));
        assert!((slow as i32 - fast as i32).abs() <= 1, "{slow} and {fast}");
        // A tenth of a second, held for a hundredth at a time.
        assert!((9..=11).contains(&slow), "{slow}");
        // At none, every sample goes through.
        let clean = played(DistortType::Down, 0x00, 0xFF, 0.5);
        assert_eq!(
            distinct(&clean),
            distinct(&played(DistortType::Crush, 0, 0, 0.5))
        );
    }

    #[test]
    fn mix_blends_from_dry_to_wet() {
        for kind in DistortType::ALL {
            let dry = played(kind, 0xFF, 0x00, 0.5);
            let plain: Vec<f32> = sine(220.0, 0.5, RATE as usize / 10).collect();
            assert_eq!(differs(&dry, &plain), 0.0, "{kind:?}");
            let wet = played(kind, 0xFF, 0xFF, 0.5);
            assert!(differs(&wet, &plain) > 0.01, "{kind:?}");
            let half = played(kind, 0xFF, 0x80, 0.5);
            let between = differs(&half, &plain);
            assert!(between < differs(&wet, &plain), "{kind:?}");
            assert!(between > 0.0, "{kind:?}");
        }
    }

    #[test]
    fn gain_turns_it_all_down_on_the_level_scale() {
        let run = |gain: u8, mix: u8| {
            let mut state = InsertState::default();
            state.set(Insert::Distort(Distort {
                kind: DistortType::Hard,
                amount: 0xFF,
                mix,
                gain,
                ..Distort::DEFAULT
            }));
            sine(220.0, 0.5, RATE as usize / 10)
                .map(|sample| state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
                .collect::<Vec<f32>>()
        };
        let db = |a: f32, b: f32| 20.0 * (a / b).log10();
        // Every `10` is 4 dB.
        let down = db(rms(&run(0xD0, 0xFF)), rms(&run(0xE0, 0xFF)));
        assert!((down + 4.0).abs() < 0.01, "{down}");
        // It also takes the dry sound with it.
        let dry = db(rms(&run(0xD0, 0x00)), rms(&run(0xE0, 0x00)));
        assert!((dry + 4.0).abs() < 0.01, "{dry}");
        assert_eq!(peak(&run(0x00, 0xFF)), 0.0);
    }

    #[test]
    fn a_changed_amount_moves_smoothly_and_keeps_its_state() {
        let mut state = state(DistortType::Soft, 0x00, 0xFF);
        state.process([0.5; 2], NO_SMOOTHING, RATE, &Keys::default());
        let settings = |amount| {
            Insert::Distort(Distort {
                kind: DistortType::Soft,
                amount,
                mix: 0xFF,
                ..Distort::default()
            })
        };
        state.set(settings(0xFF));
        assert_eq!(state.amount.value, 0.0, "it hasn't jumped");
        assert_eq!(state.amount.target, 1.0);
        state.settle();
        assert_eq!(state.amount.value, 1.0);
        state.set(settings(0x00));
        assert_eq!(state.amount.value, 1.0);
    }

    fn level_at(kind: DistortType, hertz: f32, probe: f32) -> f32 {
        // A loud sine, in integer cycles, and how much of a frequency comes out.
        let mut state = state(kind, 0xFF, 0xFF);
        let frames = RATE as usize;
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for (frame, sample) in sine(hertz, 0.5, frames).enumerate() {
            let out = state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0];
            if frame >= RATE as usize / 2 {
                let phase = f64::from(frame as u32) / f64::from(RATE)
                    * f64::from(probe)
                    * std::f64::consts::TAU;
                real += f64::from(out) * phase.cos();
                imaginary += f64::from(out) * phase.sin();
            }
        }
        ((real * real + imaginary * imaginary).sqrt() / (frames as f64 / 2.0)) as f32
    }

    /// The worst of what a loud 2.5 kHz sine's odd harmonics fold back to, under its fundamental, in
    /// dB, driven as hard as it goes.
    fn worst_folded(kind: DistortType) -> f32 {
        let fundamental = level_at(kind, 2500.0, 2500.0);
        // The 11th to the 19th harmonics, which fold back past 24 kHz to between the harmonics.
        [20_500.0, 15_500.0, 10_500.0, 5_500.0, 500.0]
            .into_iter()
            .map(|alias| 20.0 * (level_at(kind, 2500.0, alias) / fundamental).log10())
            .fold(f32::MIN, f32::max)
    }

    #[test]
    fn hard_fold_and_sine_are_anti_aliased_too() {
        // Before their antiderivatives, at 2x alone, these were -49, -23 and -52 dB.
        for (kind, most) in [
            (DistortType::Hard, -70.0),
            (DistortType::Fold, -45.0),
            (DistortType::Sine, -58.0),
        ] {
            let db = worst_folded(kind);
            assert!(db < most, "{kind:?}: {db} dB");
        }
    }

    #[test]
    fn oversampling_keeps_aliasing_down_for_the_clips() {
        // A 5 kHz sine, clipped hard. Its 7th harmonic folds back to 13 kHz and its 9th to 3 kHz.
        for kind in [DistortType::Soft, DistortType::Hard] {
            let fundamental = level_at(kind, 5000.0, 5000.0);
            for alias in [3000.0, 13_000.0] {
                let db = 20.0 * (level_at(kind, 5000.0, alias) / fundamental).log10();
                assert!(db < -30.0, "{kind:?} at {alias} is at {db} dB");
            }
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
            state.hold(insert, &mut pool, 25.0, 0);
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

    #[test]
    fn the_clips_hold_a_nan_to_an_end() {
        for clip in [soft_clip, crate::dsp::filters::filter::soft_clip, |x| {
            shaped(DistortType::Hard, x)
        }] {
            assert!(clip(f32::NAN).is_finite());
            assert!(clip(f32::INFINITY).is_finite());
            assert!(clip(f32::NEG_INFINITY).is_finite());
        }
    }

    #[test]
    fn each_antiderivative_is_its_shapes_and_nothing_at_nothing() {
        for kind in [DistortType::Hard, DistortType::Fold, DistortType::Sine] {
            assert_eq!(integral(kind, 0.0), 0.0, "{kind:?}");
            for at in -400..400 {
                let x = f64::from(at) * 0.037;
                let slope = (integral(kind, x + 1e-6) - integral(kind, x - 1e-6)) / 2e-6;
                let shape = f64::from(shaped(kind, x as f32));
                assert!(
                    (slope - shape).abs() < 1e-4,
                    "{kind:?} at {x}: {slope} {shape}"
                );
            }
        }
    }

    #[test]
    fn a_fresh_shaper_starts_without_a_jump() {
        // A step from nothing, driven hard, rings the halfband filters a little, and no more.
        for kind in [DistortType::Hard, DistortType::Fold, DistortType::Sine] {
            let mut state = state(kind, 0xFF, 0xFF);
            let out: Vec<f32> = (0..100)
                .map(|_| state.process([0.5; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
                .collect();
            assert!(peak(&out) <= 1.2, "{kind:?}: {}", peak(&out));
        }
    }
}
