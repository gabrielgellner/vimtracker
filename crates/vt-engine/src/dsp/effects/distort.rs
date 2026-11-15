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

use vt_model::insert::{Distort, DistortType};

use crate::dsp::blocks::clip::soft_clip;
use crate::dsp::blocks::clip::within;
use crate::dsp::blocks::flush::{flush, tiny_by_fields};
use crate::dsp::blocks::oversampler::Oversampler;
use crate::dsp::blocks::sides::{SameBits, Sides};
use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::effects::memory::SongPlace;
use crate::dsp::effects::pedal::Pedal;
use crate::dsp::effects::slot::{Mono, SlotEffect};

/// How much `Amount` turns up what's clipped or folded, at full, in dB.
const MOST_DRIVE_DB: f32 = 30.0;
/// The longest `Down` holds a sample, in seconds, so it's the same at any device rate.
const LONGEST_HOLD: f32 = 0.01;
/// How many octaves over the note `Down` holds at with `Track` on, at `00`. At `FF` it holds once
/// a cycle of the note.
const TRACKED_OCTAVES: f32 = 8.0;
/// Mu-law's `μ`, as 8-bit telephone and sampler audio has it.
const MU: f32 = 255.0;
/// How close two samples at the higher rate can be before the antiderivatives' difference is
/// taken as no better than the shape itself, halfway between them.
const ADAA_CLOSEST: f64 = 1e-4;

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

tiny_by_fields!(Shaper {
    oversampler,
    last,
    last_integral
});

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
        // With nothing coming in, all it keeps goes once it's tiny, so it falls to an exact zero
        // rather than trading denormals round its oversampling for good (#260). Each shape's
        // antiderivative is nothing at nothing, so the last sample and its antiderivative go
        // together.
        if input == 0.0 {
            flush(self);
        }
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
        // `sin(pi x / 2)`, its turn taken about nothing so that nothing gives exactly nothing,
        // where a turn about a half gave `sin(pi)`, about -141 dB, for good (#260).
        DistortType::Sine => {
            let turn = driven * 0.25;
            ((turn - turn.round()) * std::f32::consts::TAU).sin()
        }
        // These don't shape at the higher rate here: the pedals have their own.
        DistortType::Crush
        | DistortType::Down
        | DistortType::Overdrive
        | DistortType::Distortion => driven,
    }
}

/// What a steady input level comes out as through a distortion that shapes each sample alone, wet
/// and dry mixed as `Mix` has them, before `Gain`: its transfer curve, for a drawing of it. `Soft`,
/// `Hard`, `Fold` and `Sine` are driven by `Amount`, and `Crush` rounds to its `Bits`, through
/// mu-law if it's on. `Down` holds samples over time, and the pedals filter round their clip, so
/// none of them has one.
pub fn transfer(settings: Distort, input: f32) -> Option<f32> {
    let amount = f32::from(settings.amount) / 255.0;
    let wet = match settings.kind {
        DistortType::Soft => {
            let (pre, post) = overdrive_gains(amount);
            soft_clip(input * pre) * post
        }
        DistortType::Hard | DistortType::Fold | DistortType::Sine => {
            shaped(settings.kind, input * drive_of(amount))
        }
        DistortType::Crush => crushed(
            input,
            crush_step(f32::from(settings.bits)),
            settings.mu_law != 0,
        ),
        DistortType::Down | DistortType::Overdrive | DistortType::Distortion => return None,
    };
    let mix = f32::from(settings.mix) / 255.0;
    Some(wet * mix + input * (1.0 - mix))
}

/// What a slot keeps for a distortion beside its `Amount`, `Mix` and `Gain`, which the slot eases
/// and blends as it does every effect's.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Distortion {
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
    /// A guitar pedal's filters and oversampling.
    pedal: Pedal,
}

impl Default for Distortion {
    fn default() -> Self {
        Self {
            oversampler: Sides::default(),
            bits: Smoothed::at(0.0),
            held: [0.0; 2],
            before: [0.0; 2],
            since: 0.0,
            pedal: Pedal::default(),
        }
    }
}

impl SameBits for Distortion {
    #[inline]
    fn same_bits(&self, other: &Self) -> bool {
        self.bits.same_bits(&other.bits)
            && self.held.same_bits(&other.held)
            && self.before.same_bits(&other.before)
            && self.since.same_bits(&other.since)
            && self.oversampler.same_bits(&other.oversampler)
            && self.pedal.same_bits(&other.pedal)
    }
}

impl Distortion {
    /// Take what's set: `Crush`'s bits, and a pedal's tone.
    pub(crate) fn follow(&mut self, settings: Distort) {
        self.bits.target = f32::from(settings.bits);
        if settings.kind.is_pedal() {
            self.pedal.follow(f32::from(settings.tone) / 255.0);
        }
    }

    /// Jump to what's set, for playing from the start or a distortion just chosen.
    pub(crate) fn settle(&mut self) {
        self.bits.settle();
        self.pedal.settle();
    }

    /// One frame through the distortion, all wet, at `amount` as the slot has eased it, and
    /// tracking `note` for `Down`. `same` says whether the frame's sides are the same, bit for bit.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub(crate) fn process(
        &mut self,
        input: [f32; 2],
        same: bool,
        settings: Distort,
        amount: f32,
        note: f32,
        smoothing: f32,
        rate: u32,
    ) -> [f32; 2] {
        let kind = settings.kind;
        match kind {
            DistortType::Crush => {
                let step = crush_step(self.bits.next(smoothing));
                input.map(|sample| crushed(sample, step, settings.mu_law != 0))
            }
            DistortType::Down => {
                // Following the note, it holds at a whole number of times the note's frequency, so
                // what it folds back lands on the note's harmonics.
                let period = if settings.track != 0 {
                    let hertz = 440.0 * ((note - 69.0) / 12.0).exp2();
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
        }
    }
}

impl SlotEffect for Distortion {
    type Settings = Distort;

    /// Each of its types works each side alone, the same way, with nothing between them.
    const MONO: Mono = Mono::OneSide;

    fn new(_rate: u32) -> Self {
        Self::default()
    }

    fn follow(&mut self, settings: Distort, _song: SongPlace, _frames: u64) {
        self.follow(settings);
    }

    fn settle(&mut self) {
        self.settle();
    }

    fn clear(&mut self) {
        *self = Self::default();
    }

    /// None: it's worked every frame, so the slot never waits on its tail.
    fn quiet_frames(&self, _rate: u32) -> u32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use vt_model::insert::Insert;

    use super::*;
    use crate::dsp::effects::slot::{InsertState, Keys};
    use crate::dsp::signals::{peak, sines};

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

    fn played(kind: DistortType, amount: u8, mix: u8, level: f32) -> Vec<f32> {
        let mut state = state(kind, amount, mix);
        sines(RATE as usize / 10, 220.0, level, RATE)
            .map(|sample| state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
            .collect()
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
    fn a_steady_level_comes_out_where_the_transfer_curve_says() {
        for kind in DistortType::ALL {
            for (amount, mix, mu_law) in [(0x40, 0xFF, 0), (0xC0, 0xFF, 1), (0xA0, 0x80, 0)] {
                let settings = Distort {
                    kind,
                    amount,
                    bits: 4,
                    mix,
                    mu_law,
                    ..Distort::DEFAULT
                };
                let curve = transfer(settings, 0.3);
                if matches!(
                    kind,
                    DistortType::Down | DistortType::Overdrive | DistortType::Distortion
                ) {
                    assert_eq!(curve, None, "{kind:?}");
                    continue;
                }
                for level in [-0.9, -0.3, 0.05, 0.3, 0.7] {
                    let mut state = InsertState::default();
                    state.set(Insert::Distort(settings));
                    let mut out = 0.0;
                    for _ in 0..512 {
                        out = state.process([level; 2], NO_SMOOTHING, RATE, &Keys::default())[0];
                    }
                    let curve = transfer(settings, level).unwrap();
                    assert!(
                        (out - curve).abs() < 1e-3,
                        "{kind:?} {amount:02X} at {level}: {out} against {curve}"
                    );
                }
            }
        }
    }

    /// The average level, past the oversampler's short delay.
    fn rms(samples: &[f32]) -> f32 {
        crate::dsp::signals::rms(&samples[64..])
    }

    #[test]
    fn soft_is_about_clean_at_none_and_a_loud_sound_stays_about_as_loud() {
        let plain: Vec<f32> = sines(RATE as usize / 10, 220.0, 0.25, RATE).collect();
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
            let gentle = peak(played(kind, 0x00, 0xFF, 0.1));
            let hard = peak(played(kind, 0xFF, 0xFF, 0.1));
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
            sines(RATE as usize / 10, 220.0, 0.02, RATE)
                .map(|x| state.process([x; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
                .collect::<Vec<f32>>()
        };
        // Linear steps of an eighth gate a sound this quiet; mu-law's are fine enough near nothing.
        assert_eq!(peak(quiet(0)), 0.0);
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
            for (frame, sample) in sines(frames, 440.0, 0.5, RATE).enumerate() {
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
            let plain: Vec<f32> = sines(RATE as usize / 10, 220.0, 0.5, RATE).collect();
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
            sines(RATE as usize / 10, 220.0, 0.5, RATE)
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
        assert_eq!(peak(run(0x00, 0xFF)), 0.0);
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
        assert_eq!(state.amount().value, 0.0, "it hasn't jumped");
        assert_eq!(state.amount().target, 1.0);
        state.settle();
        assert_eq!(state.amount().value, 1.0);
        state.set(settings(0x00));
        assert_eq!(state.amount().value, 1.0);
    }

    fn level_at(kind: DistortType, hertz: f32, probe: f32) -> f32 {
        // A loud sine, in integer cycles, and how much of a frequency comes out.
        let mut state = state(kind, 0xFF, 0xFF);
        let frames = RATE as usize;
        let (mut real, mut imaginary) = (0.0_f64, 0.0_f64);
        for (frame, sample) in sines(frames, hertz, 0.5, RATE).enumerate() {
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
