//! Distort's guitar pedals, `Overdrive` and `Distortion` (#361): their level, their harmonics,
//! their tone, mix and bypass, and that nothing goes wrong at the extremes.

use std::f32::consts::TAU;

use vt_engine::dsp::effects::distort::InsertState;
use vt_engine::track_state::Keys;
use vt_model::insert::{Distort, DistortType, Insert};

const RATE: u32 = 48_000;
/// Settings take effect at once, as the smoother has nothing to do.
const NO_SMOOTHING: f32 = 1.0;
const PEDALS: [DistortType; 2] = [DistortType::Overdrive, DistortType::Distortion];

fn pedal(kind: DistortType, amount: u8, tone: u8, mix: u8) -> InsertState {
    let mut state = InsertState::default();
    state.set(Insert::Distort(Distort {
        kind,
        amount,
        tone,
        mix,
        ..Distort::default()
    }));
    state.settle();
    state
}

fn play(state: &mut InsertState, input: &[f32]) -> Vec<f32> {
    input
        .iter()
        .map(|&sample| state.process([sample; 2], NO_SMOOTHING, RATE, &Keys::default())[0])
        .collect()
}

fn sine(hertz: f32, level: f32, seconds: f32) -> Vec<f32> {
    (0..(seconds * RATE as f32) as usize)
        .map(|frame| (frame as f32 / RATE as f32 * hertz * TAU).sin() * level)
        .collect()
}

/// Something like an acid line: a bright sawtooth at 55 Hz, falling and coming back each eighth
/// of a second as a 303's notes do, at about the Blade riff's level, -16 dBFS RMS.
fn acid_line(seconds: f32) -> Vec<f32> {
    let mut low = 0.0;
    (0..(seconds * RATE as f32) as usize)
        .map(|frame| {
            let time = frame as f32 / RATE as f32;
            let saw = 2.0 * (time * 55.0).fract() - 1.0;
            let note = (time * 8.0).fract();
            low += (saw - low) * (0.05 + 0.3 * (1.0 - note));
            low * 0.38 * (1.0 - note * 0.7)
        })
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    // Past the oversampling's and the filters' settling.
    let tail = &samples[samples.len() / 4..];
    (tail.iter().map(|s| s * s).sum::<f32>() / tail.len() as f32).sqrt()
}

fn db(value: f32) -> f32 {
    20.0 * value.max(1e-12).log10()
}

/// A sine's harmonics, the 1st to the 5th, from the last half of what came out, by correlating
/// with each over a whole number of cycles.
fn harmonics(samples: &[f32], hertz: f32) -> [f32; 5] {
    let cycle = RATE as f32 / hertz;
    let cycles = ((samples.len() / 2) as f32 / cycle).floor();
    let count = (cycles * cycle).round() as usize;
    let part = &samples[samples.len() - count..];
    std::array::from_fn(|index| {
        let number = (index + 1) as f32;
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        for (frame, &sample) in part.iter().enumerate() {
            let phase = f64::from(frame as f32 * number * hertz * TAU / RATE as f32);
            re += f64::from(sample) * phase.cos();
            im += f64::from(sample) * phase.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / count as f64) as f32
    })
}

#[test]
fn an_acid_line_comes_out_about_as_loud_as_it_went_in_at_the_defaults() {
    let line = acid_line(1.0);
    for kind in PEDALS {
        let defaults = Distort {
            kind,
            ..Distort::default()
        };
        let out = play(
            &mut pedal(kind, defaults.amount, defaults.tone, 0xFF),
            &line,
        );
        let change = db(rms(&out)) - db(rms(&line));
        assert!(
            change.abs() < 4.0,
            "{kind:?} changes the level by {change} dB"
        );
    }
}

#[test]
fn the_overdrive_clips_evenly_and_its_mids_far_sooner_than_its_bass() {
    let thd = |hertz: f32, level: f32| {
        let out = play(
            &mut pedal(DistortType::Overdrive, 0x40, 0x80, 0xFF),
            &sine(hertz, level, 0.5),
        );
        let found = harmonics(&out, hertz);
        let rest = found[1..].iter().map(|h| h * h).sum::<f32>().sqrt();
        (db(rest / found[0]), db(found[1] / found[0]))
    };
    let (mids, even) = thd(1000.0, 0.1);
    let (bass, _) = thd(110.0, 0.1);
    assert!(even < -60.0, "no 2nd harmonic, as its diodes match: {even}");
    assert!(mids > -25.0, "a 1 kHz sine at -20 dBFS clips: {mids}");
    assert!(
        bass < mids - 6.0,
        "110 Hz, under its gain branch, clips less: {bass} against {mids}"
    );
}

#[test]
fn more_amount_clips_the_overdrive_harder() {
    let thd = |amount: u8| {
        let out = play(
            &mut pedal(DistortType::Overdrive, amount, 0x80, 0xFF),
            &sine(500.0, 0.05, 0.5),
        );
        let found = harmonics(&out, 500.0);
        db(found[2] / found[0])
    };
    assert!(thd(0xFF) > thd(0x00) + 3.0, "{} {}", thd(0xFF), thd(0x00));
}

#[test]
fn the_distortion_squares_a_loud_sine_and_holds_its_ceiling() {
    let mut state = pedal(DistortType::Distortion, 0x80, 0xFF, 0xFF);
    let loud = harmonics(&play(&mut state, &sine(220.0, 1.0, 0.5)), 220.0);
    // A square's 3rd and 5th harmonics are a third and a fifth of its fundamental.
    assert!((db(loud[2] / loud[0]) + 9.54).abs() < 1.0, "{loud:?}");
    assert!((db(loud[4] / loud[0]) + 13.98).abs() < 1.5, "{loud:?}");
    let mut state = pedal(DistortType::Distortion, 0x80, 0xFF, 0xFF);
    let quieter = harmonics(&play(&mut state, &sine(220.0, 0.25, 0.5)), 220.0);
    assert!(
        (db(loud[0]) - db(quieter[0])).abs() < 0.5,
        "12 dB more in is the same out: {} and {}",
        db(loud[0]),
        db(quieter[0])
    );
    // Its clip sits a little off centre, as Dist OPAMP-21's does: a 2nd harmonic, small and
    // falling as it's driven harder, as one side's head start counts for less.
    let (second, quieter_second) = (db(loud[1] / loud[0]), db(quieter[1] / quieter[0]));
    assert!(second < -45.0, "{second}");
    assert!(
        quieter_second > second + 6.0,
        "{quieter_second} and {second}"
    );
}

#[test]
fn the_distortions_2nd_harmonic_as_it_starts_to_clip_is_dist_opamp_21s() {
    // At `Amount` 80, as Dist OPAMP-21 at 50%, a 110 Hz sine at -24 dBFS: its 2nd harmonic is
    // 28.7 dB under the fundamental, and at 0 dBFS 52.4.
    for (level, expected) in [(-24.0, -28.7), (0.0, -52.4)] {
        let mut state = pedal(DistortType::Distortion, 0x80, 0xFF, 0xFF);
        let found = harmonics(
            &play(&mut state, &sine(110.0, 10_f32.powf(level / 20.0), 0.6)),
            110.0,
        );
        let second = db(found[1] / found[0]);
        assert!(
            (second - expected).abs() < 3.0,
            "{level} dBFS: {second} against {expected}"
        );
    }
}

#[test]
fn the_distortions_small_signal_gain_follows_amount_as_fitted() {
    // Under its clip it's its gain going in times its ceiling: 17.5 dB at 00 to 43.1 dB at FF,
    // with the ceiling's -13.2 dB.
    for (amount, expected) in [(0x00, 17.5 - 13.15), (0xFF, 43.1 - 13.15)] {
        let mut state = pedal(DistortType::Distortion, amount, 0xFF, 0xFF);
        let found = harmonics(&play(&mut state, &sine(1000.0, 0.0005, 0.5)), 1000.0);
        let gain = db(found[0] / 0.0005);
        assert!(
            (gain - expected).abs() < 0.5,
            "{amount:02X}: {gain} against {expected}"
        );
    }
}

#[test]
fn tone_darkens_both_pedals() {
    for kind in PEDALS {
        let bright = |tone: u8| {
            let out = play(&mut pedal(kind, 0x80, tone, 0xFF), &sine(440.0, 0.5, 0.5));
            let found = harmonics(&out, 440.0);
            db(found[4] / found[0])
        };
        assert!(
            bright(0xFF) > bright(0x00) + 8.0,
            "{kind:?}: the 5th harmonic at FF {} and 00 {}",
            bright(0xFF),
            bright(0x00)
        );
    }
}

#[test]
fn no_mix_is_the_sound_as_it_was_and_bypass_lands_on_it() {
    let line = acid_line(0.25);
    for kind in PEDALS {
        let dry = play(&mut pedal(kind, 0xFF, 0x80, 0x00), &line);
        assert_eq!(dry, line, "{kind:?} at no mix");
        let mut state = pedal(kind, 0xFF, 0x80, 0xFF);
        state.bypass(true);
        state.settle();
        assert_eq!(play(&mut state, &line), line, "{kind:?} bypassed");
        let half = play(&mut pedal(kind, 0xFF, 0x80, 0x80), &line);
        let wet = play(&mut pedal(kind, 0xFF, 0x80, 0xFF), &line);
        for ((half, wet), dry) in half.iter().zip(&wet).zip(&line) {
            let blend = dry + (wet - dry) * f32::from(0x80_u8) / 255.0;
            assert!(
                (half - blend).abs() < 1e-5,
                "{kind:?}: {half} against {blend}"
            );
        }
    }
}

#[test]
fn nothing_goes_wrong_at_the_extremes() {
    for kind in PEDALS {
        let mut state = pedal(kind, 0xFF, 0xFF, 0xFF);
        let mut input: Vec<f32> = sine(3000.0, 100.0, 0.1);
        input.extend([1e6, -1e6, f32::MIN_POSITIVE, -1e-30, 0.0]);
        input.extend(acid_line(0.1));
        input.extend(std::iter::repeat_n(0.0, RATE as usize));
        let out = play(&mut state, &input);
        assert!(out.iter().all(|s| s.is_finite()), "{kind:?}");
        assert!(
            out.iter().all(|s| *s == 0.0 || s.is_normal()),
            "{kind:?}: no denormals"
        );
    }
}

#[test]
fn silence_rings_out_to_an_exact_zero_and_starts_again_as_new() {
    for kind in PEDALS {
        let mut state = pedal(kind, 0xFF, 0xFF, 0xFF);
        let mut input = acid_line(0.25);
        // Then three seconds of nothing, for the filters to ring out.
        input.extend(std::iter::repeat_n(0.0, 3 * RATE as usize));
        let out = play(&mut state, &input);
        assert!(
            out.iter().all(|s| *s == 0.0 || s.is_normal()),
            "{kind:?}: no denormals"
        );
        assert!(
            out[out.len() - RATE as usize / 2..]
                .iter()
                .all(|s| *s == 0.0),
            "{kind:?} rests at an exact zero"
        );
        // And it starts again from rest as it would have.
        let again = play(&mut state, &sine(220.0, 0.5, 0.05));
        let fresh = play(&mut pedal(kind, 0xFF, 0xFF, 0xFF), &sine(220.0, 0.5, 0.05));
        assert_eq!(again, fresh, "{kind:?}");
    }
}

#[test]
fn the_oversampling_keeps_aliasing_well_under_the_harmonics() {
    // A 5 kHz sine clipped: its 3rd harmonic, at 15 kHz, is real, and its 5th, at 25 kHz, folds
    // back to 23 kHz at 48 kHz, and its 7th, at 35 kHz, to 13 kHz. Those fold-backs are what the
    // oversampling holds down.
    for kind in PEDALS {
        let out = play(&mut pedal(kind, 0x80, 0xFF, 0xFF), &sine(5000.0, 0.25, 0.5));
        let tail = &out[out.len() / 2..];
        let level_at = |hertz: f32| {
            let (mut re, mut im) = (0.0_f64, 0.0_f64);
            for (frame, &sample) in tail.iter().enumerate() {
                let phase = f64::from(frame as f32 * hertz * TAU / RATE as f32);
                re += f64::from(sample) * phase.cos();
                im += f64::from(sample) * phase.sin();
            }
            db((2.0 * (re * re + im * im).sqrt() / tail.len() as f64) as f32)
        };
        let fundamental = level_at(5000.0);
        let folded = level_at(13_000.0);
        assert!(
            folded < fundamental - 40.0,
            "{kind:?}: the 7th harmonic folded to 13 kHz is {} dB under the fundamental",
            fundamental - folded
        );
    }
}
