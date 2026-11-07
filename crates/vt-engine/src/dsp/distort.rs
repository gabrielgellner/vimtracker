//! The `Distort` effect. Its shapes come from Vital's `distortion.cpp`, by Matt Tytel, GPL 3 or later:
//! Copyright 2013-2019 Matt Tytel. Its scaling, mix and state are this program's own.

use vt_model::insert::{Distort, DistortType, Insert};

use crate::dsp::filter::soft_clip;
use crate::dsp::oversampler::Oversampler;
use crate::dsp::smoother::Smoothed;

/// How much `Amount` turns up what's clipped or folded, at full, in dB.
const MOST_DRIVE_DB: f32 = 30.0;
/// The bit depths `Crush` runs through, from `Amount` at `00` to `FF`.
const CRUSH_BITS: (f32, f32) = (16.0, 2.0);
/// The longest `Down` holds a sample, in seconds, so it's the same at any device rate.
const LONGEST_HOLD: f32 = 0.01;

/// What a track keeps for one of its slots. Nothing here allocates.
#[derive(Debug, Clone, Copy)]
pub struct InsertState {
    insert: Insert,
    amount: Smoothed,
    mix: Smoothed,
    oversampler: [Oversampler; 2],
    held: [f32; 2],
    /// How many frames since `Down` last took a sample.
    since: f32,
}

impl Default for InsertState {
    fn default() -> Self {
        Self {
            insert: Insert::Empty,
            amount: Smoothed::at(0.0),
            mix: Smoothed::at(0.0),
            oversampler: [Oversampler::default(); 2],
            held: [0.0; 2],
            since: 0.0,
        }
    }
}

/// How much `Amount` turns up the signal: 0 dB at `00`.
#[inline]
fn drive_of(amount: f32) -> f32 {
    10.0_f32.powf(amount * MOST_DRIVE_DB / 20.0)
}

/// The distance between the steps `Crush` rounds to.
#[inline]
fn crush_step(amount: f32) -> f32 {
    2.0_f32.powf(1.0 - (CRUSH_BITS.0 + (CRUSH_BITS.1 - CRUSH_BITS.0) * amount))
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
    /// Take what's now in the slot. A different effect starts afresh.
    pub fn set(&mut self, insert: Insert) {
        let same_kind = matches!(
            (self.insert, insert),
            (Insert::Distort(before), Insert::Distort(after)) if before.kind == after.kind
        );
        if !same_kind {
            *self = Self::default();
            if let Insert::Distort(Distort { amount, mix, .. }) = insert {
                self.amount = Smoothed::at(f32::from(amount) / 255.0);
                self.mix = Smoothed::at(f32::from(mix) / 255.0);
            }
        }
        self.insert = insert;
        if let Insert::Distort(Distort { amount, mix, .. }) = insert {
            self.amount.target = f32::from(amount) / 255.0;
            self.mix.target = f32::from(mix) / 255.0;
        }
    }

    /// Jump to the settings, for playing from the start.
    pub fn settle(&mut self) {
        self.amount.settle();
        self.mix.settle();
    }

    /// One frame through the slot. An empty one does nothing else.
    #[inline]
    pub fn process(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let Insert::Distort(Distort { kind, .. }) = self.insert else {
            return input;
        };
        let amount = self.amount.next(smoothing);
        let mix = self.mix.next(smoothing);
        let wet = match kind {
            DistortType::Crush => {
                let step = crush_step(amount);
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
            wet[0] * mix + input[0] * (1.0 - mix),
            wet[1] * mix + input[1] * (1.0 - mix),
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
        state.set(Insert::Distort(Distort { kind, amount, mix }));
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
    fn a_changed_amount_moves_smoothly_and_keeps_its_state() {
        let mut state = state(DistortType::Soft, 0x00, 0xFF);
        state.process([0.5; 2], NO_SMOOTHING, RATE);
        let settings = |amount| {
            Insert::Distort(Distort {
                kind: DistortType::Soft,
                amount,
                mix: 0xFF,
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
