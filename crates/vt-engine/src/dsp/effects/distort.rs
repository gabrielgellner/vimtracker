//! The `Distort` effect. Most of its shapes come from Vital's `distortion.cpp`, by Matt Tytel, GPL 3
//! or later: Copyright 2013-2019 Matt Tytel. `Soft` comes from Plaits' `dsp/fx/overdrive.h`, by Emilie
//! Gillet, MIT: Copyright 2014 Emilie Gillet. The other types' scaling, the mix and state are this
//! program's own. The guitar pedals, `Overdrive` and `Distortion`, are in [`super::pedal`].

use vt_model::constants::UNITY_LEVEL;
use vt_model::insert::{
    Chorus, Comb, Compressor, Delay, Distort, DistortType, Equaliser, Flanger, Grains, Insert,
    Limiter, LowPassGate, Phaser, Pitch, Plate, Resonator, Reverb, Tape, Vowel, Warps,
};
use vt_model::song::level_gain;

use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::dynamics::compressor::Compressor as Dynamics;
use crate::dsp::dynamics::eq::ThreeBands;
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
/// How long switching a slot out or back in takes, in ms: long enough not to click, and short
/// enough that a step's lock of its bypass lands with the step.
pub const BYPASS_FADE_MS: f32 = 5.0;

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
    oversampler: [Oversampler; 2],
    held: [f32; 2],
    /// How many frames since `Down` last took a sample.
    since: f32,
    /// An EQ's bands, which are small enough to keep in every slot.
    eq: ThreeBands,
    /// A compressor's detector, small enough as well.
    dynamics: Dynamics,
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
            oversampler: [Oversampler::default(); 2],
            held: [0.0; 2],
            since: 0.0,
            eq: ThreeBands::default(),
            dynamics: Dynamics::default(),
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
        | (Insert::Vowel(_), Insert::Vowel(_)) => true,
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

/// The distance between the steps `Crush` rounds to, for some whole bits.
#[inline]
fn crush_step(bits: u8) -> f32 {
    2.0_f32.powi(1 - i32::from(bits))
}

/// A shape that needs a higher rate, given what's already been driven.
#[inline]
fn shaped(kind: DistortType, driven: f32) -> f32 {
    match kind {
        DistortType::Soft => soft_clip(driven),
        DistortType::Hard => driven.clamp(-1.0, 1.0),
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
            | Insert::Pitch(Pitch { mix, gain, .. })
            | Insert::Grains(Grains { mix, gain, .. })
            | Insert::Warps(Warps { mix, gain, .. })
            | Insert::Lpg(LowPassGate { mix, gain, .. })
            | Insert::Reso(Resonator { mix, gain, .. })
            | Insert::Comb(Comb { mix, gain, .. })
            | Insert::Vowel(Vowel { mix, gain, .. }) => (0, mix, gain),
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
        }
        if let Insert::Eq(settings) = insert {
            let fresh = !same_effect(self.insert, insert);
            self.eq.follow(settings);
            if fresh {
                self.eq.settle();
            }
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

    /// One frame through a resonator or comb, all wet, tuned to the track's note. Like `ring`, once
    /// it's quiet with nothing going in it does no work, and when it starts again it's at the note
    /// and what's set, rather than gliding there from where it stopped.
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

    /// Follow the track's note, for an oscillator, resonator, comb or vowel filter of the slot's own.
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
        self.mix.settle();
        self.gain.settle();
        self.phaser.settle();
        self.pedal.settle();
    }

    /// One frame through the slot, faded toward what came in as it's bypassed.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32, keys: &Keys) -> [f32; 2] {
        let wet = self.effect(input, smoothing, rate, keys);
        if self.on.value == self.on.target {
            return if self.on.target > 0.0 { wet } else { input };
        }
        // A straight line rather than a smoother's curve, so a step's lock is done within a few
        // ms of its step, however long it lasts.
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
    }

    /// One frame through the effect. An empty slot does nothing else.
    #[inline]
    fn effect(&mut self, input: [f32; 2], smoothing: f32, rate: u32, keys: &Keys) -> [f32; 2] {
        let (kind, set) = match self.insert {
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
                return [
                    wet[0] * mix + input[0] * (1.0 - mix),
                    wet[1] * mix + input[1] * (1.0 - mix),
                ];
            }
            Insert::Lpg(settings) => {
                // It's opened by its own input, or another track's as it was after its slots.
                let key = settings
                    .source_track(keys.sounds)
                    .map_or(input, |track| keys.of(track));
                let wet = self.gate.process(input, key, settings, smoothing, rate);
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
            Insert::Reso(_) | Insert::Comb(_) => {
                let wet = self.resonate(input, smoothing, rate);
                let mix = self.mix.next(smoothing);
                let gain = self.gain.next(smoothing);
                return [
                    (wet[0] * mix + input[0] * (1.0 - mix)) * gain,
                    (wet[1] * mix + input[1] * (1.0 - mix)) * gain,
                ];
            }
            Insert::Vowel(_) => {
                let wet = self.vowel.process(input, self.note, smoothing, rate);
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
                    .process(input, smoothing, rate)
                    .map(|sample| sample * gain);
            }
            Insert::Distort(Distort { kind, bits, .. }) => (Some(kind), bits),
            Insert::Delay(_)
            | Insert::Reverb(_)
            | Insert::Chorus(_)
            | Insert::Flanger(_)
            | Insert::Tape(_)
            | Insert::Plate(_)
            | Insert::Pitch(_)
            | Insert::Grains(_) => (None, 0),
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
                // Whole bits change at once, so this isn't smoothed.
                let step = crush_step(set);
                input.map(|sample| (sample / step).round() * step)
            }
            DistortType::Down => {
                // A hold of one frame or less is every sample, which leaves the sound as it is.
                let period = (amount * amount * LONGEST_HOLD * rate as f32).max(1.0);
                self.since += 1.0;
                if self.since >= period {
                    self.since -= period;
                    self.held = input;
                }
                self.held
            }
            DistortType::Overdrive | DistortType::Distortion => {
                self.pedal.process(kind, input, amount, smoothing, rate)
            }
            DistortType::Soft => {
                let (pre, post) = overdrive_gains(amount);
                let [left, right] = &mut self.oversampler;
                [
                    left.process(input[0], |x| soft_clip(x * pre)) * post,
                    right.process(input[1], |x| soft_clip(x * pre)) * post,
                ]
            }
            _ => {
                let drive = drive_of(amount);
                let [left, right] = &mut self.oversampler;
                [
                    left.process(input[0], |x| shaped(kind, x * drive)),
                    right.process(input[1], |x| shaped(kind, x * drive)),
                ]
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
            assert!(peak(&folded) <= 1.02, "{kind:?}: {}", peak(&folded));
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
    fn crush_changes_bits_at_once() {
        let mut state = state(DistortType::Crush, 0x00, 0xFF);
        let settings = Insert::Distort(Distort {
            kind: DistortType::Crush,
            bits: 2,
            mix: 0xFF,
            ..Distort::default()
        });
        state.set(settings);
        // The steps are already a half, with nothing to smooth.
        let out = state.process([0.3; 2], 0.0001, RATE, &Keys::default())[0];
        assert_eq!(out, 0.5);
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
}
