//! The `Distort` effect. Most of its shapes come from Vital's `distortion.cpp`, by Matt Tytel, GPL 3
//! or later: Copyright 2013-2019 Matt Tytel. `Soft` comes from Plaits' `dsp/fx/overdrive.h`, by Emilie
//! Gillet, MIT: Copyright 2014 Emilie Gillet. The other types' scaling, the mix and state are this
//! program's own.

use vt_model::constants::UNITY_LEVEL;
use vt_model::insert::{Delay, Distort, DistortType, Insert, Reverb, crush_bits};
use vt_model::song::level_gain;

use crate::dsp::drums::soft_clip;
use crate::dsp::memory::{Memory, MemoryPool, SILENT};
use crate::dsp::oversampler::Oversampler;
use crate::dsp::smoother::Smoothed;

/// How much `Amount` turns up what's clipped or folded, at full, in dB.
const MOST_DRIVE_DB: f32 = 30.0;
/// The longest `Down` holds a sample, in seconds, so it's the same at any device rate.
const LONGEST_HOLD: f32 = 0.01;

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
        }
    }
}

/// Whether it's the same effect, so it carries on rather than starting afresh.
fn same_effect(before: Insert, after: Insert) -> bool {
    match (before, after) {
        (Insert::Distort(before), Insert::Distort(after)) => before.kind == after.kind,
        (Insert::Delay(_), Insert::Delay(_)) | (Insert::Reverb(_), Insert::Reverb(_)) => true,
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
    let drive = 0.5 + amount * 0.5;
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
        DistortType::Crush | DistortType::Down => driven,
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
        self.set(insert);
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
            Insert::Distort(Distort {
                amount, mix, gain, ..
            }) => (amount, mix, gain),
            Insert::Delay(Delay { mix, gain, .. }) | Insert::Reverb(Reverb { mix, gain, .. }) => {
                (0, mix, gain)
            }
        };
        let (amount, mix, gain) = (
            f32::from(amount) / 255.0,
            f32::from(mix) / 255.0,
            level_gain(f32::from(gain)),
        );
        if !same_effect(self.insert, insert) {
            *self = Self::default();
            self.amount = Smoothed::at(amount);
            self.mix = Smoothed::at(mix);
            self.gain = Smoothed::at(gain);
        }
        self.insert = insert;
        self.amount.target = amount;
        self.mix.target = mix;
        self.gain.target = gain;
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

    /// Jump to the settings, for playing from the start.
    pub fn settle(&mut self) {
        self.amount.settle();
        self.mix.settle();
        self.gain.settle();
    }

    /// One frame through the slot. An empty one does nothing else.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let (kind, set) = match self.insert {
            Insert::Empty => return input,
            Insert::Distort(Distort { kind, amount, .. }) => (Some(kind), amount),
            Insert::Delay(_) | Insert::Reverb(_) => (None, 0),
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
                let step = crush_step(crush_bits(set));
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

    fn state(kind: DistortType, amount: u8, mix: u8) -> InsertState {
        let mut state = InsertState::default();
        state.set(Insert::Distort(Distort {
            kind,
            amount,
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
            .map(|sample| state.process([sample; 2], NO_SMOOTHING, RATE)[0])
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
                state.process([sample, -sample], 0.01, RATE),
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
            amount: 0xFF,
            mix: 0xFF,
            ..Distort::default()
        });
        state.set(settings);
        // The smoother is still at the old amount, but the steps are already a half.
        let out = state.process([0.3; 2], 0.0001, RATE)[0];
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
                    state.process([sample; 2], NO_SMOOTHING, rate)[0]
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
            }));
            sine(220.0, 0.5, RATE as usize / 10)
                .map(|sample| state.process([sample; 2], NO_SMOOTHING, RATE)[0])
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
        state.process([0.5; 2], NO_SMOOTHING, RATE);
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
            let out = state.process([sample; 2], NO_SMOOTHING, RATE)[0];
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
}
