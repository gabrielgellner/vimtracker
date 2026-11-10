//! The compressor's types, each measured as `tools/compare` measures it against its reference
//! (#338): the static curve from a sine held at steady levels, and attack and release from a tone
//! burst, as how long the gain takes to move 63% and 90% of the way.

mod support;

use std::f32::consts::TAU;
use std::sync::Arc;

use support::golden;
use vt_engine::dsp::compressor::{Compressor, curve_db};
use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::insert::{
    BUS_AUTO_RELEASE, Compressor as Settings, CompressorType, Insert, attack_ms, threshold_db,
};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// The tone the measurements play, as the harness's specs do.
const TONE_HZ: f32 = 1000.0;

/// A 1 kHz sine at a level in dB of full scale at its peak, for so many frames, from a phase.
fn sine(level_db: f32, frames: usize, from: usize) -> impl Iterator<Item = f32> {
    let amplitude = 10_f32.powf(level_db / 20.0);
    (from..from + frames).map(move |frame| {
        let phase = (frame as f64 * f64::from(TONE_HZ) / f64::from(RATE)).fract() as f32;
        amplitude * (phase * TAU).sin()
    })
}

/// What comes out of a compressor for a mono signal.
fn through(settings: Settings, signal: &[f32]) -> Vec<f32> {
    let mut compressor = Compressor::default();
    signal
        .iter()
        .map(|&sample| compressor.process([sample; 2], [sample; 2], settings, 1.0, RATE)[0])
        .collect()
}

/// A sine's level in dB of full scale at its peak, from its RMS over whole cycles.
fn level_db(samples: &[f32]) -> f32 {
    let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    10.0 * (2.0 * power).log10()
}

/// How loud a steady level comes out once it's settled, in dB.
fn settled_out(settings: Settings, level: f32) -> f32 {
    let frames = RATE as usize;
    let signal: Vec<f32> = sine(level, frames, 0).collect();
    let out = through(settings, &signal);
    // The last 0.1 s, a whole number of cycles.
    level_db(&out[frames - RATE as usize / 10..])
}

fn clean(threshold: u8, ratio: u8, attack: u8, release: u8) -> Settings {
    Settings {
        threshold,
        ratio,
        attack,
        release,
        ..Settings::DEFAULT
    }
}

fn bus(threshold: u8, ratio: u8, attack: u8, release: u8) -> Settings {
    Settings {
        kind: CompressorType::Bus,
        threshold,
        bus_ratio: ratio,
        bus_attack: attack,
        bus_release: release,
        ..Settings::DEFAULT
    }
}

#[test]
fn each_types_static_curve_is_the_one_it_draws() {
    let settings = [
        clean(0xAA, 0x3B, 0x55, 0x80),
        clean(0xAA, 0x9B, 0x55, 0x80),
        bus(0xAA, 0, 4, 1),
        bus(0xAA, 1, 4, 1),
        bus(0xAA, 2, 4, 1),
    ];
    for settings in settings {
        for level in [-40.0, -25.0, -18.0, -12.0, -6.0, 0.0] {
            let got = settled_out(settings, level);
            let drawn = curve_db(level, settings);
            // A sine's peaks are what's heard, so a steady sine settles on the curve, within the
            // ripple a held peak lets through.
            assert!(
                (got - drawn).abs() < 0.25,
                "{:?} {level}: {got} {drawn}",
                settings.kind
            );
        }
    }
}

#[test]
fn the_bus_curve_follows_bus_forces_within_half_a_db() {
    // Bus FORCE's comp path at -20 dB, its Force set where its top 10 dB has our ratio (2.7, 4.3
    // and 5.25), measured by `comp-bus-static`: its output in dBFS for inputs of -40, -30, -20,
    // -16, -12, -8, -4 and 0 dBFS.
    let bus_force: [[f32; 8]; 3] = [
        [-40.0, -30.0, -20.0, -16.51, -13.85, -11.54, -9.49, -7.66],
        [-40.0, -30.0, -20.0, -16.77, -14.78, -13.32, -12.25, -11.51],
        [-40.0, -30.0, -20.0, -16.92, -15.33, -14.37, -13.89, -13.80],
    ];
    let levels = [-40.0, -30.0, -20.0, -16.0, -12.0, -8.0, -4.0, 0.0];
    for (ratio, theirs) in bus_force.iter().enumerate() {
        let settings = bus(0xAA, ratio as u8, 4, 1);
        let errors: Vec<f32> = levels
            .iter()
            .zip(theirs)
            .map(|(&level, &out)| settled_out(settings, level) - out)
            .collect();
        let rms = (errors.iter().map(|e| e * e).sum::<f32>() / errors.len() as f32).sqrt();
        assert!(rms < 0.5, "{ratio}: {errors:?}");
    }
}

/// Attack and release from a burst: a sine at -20 dB, then 0 dB for `on` seconds, then -20 dB,
/// as how many ms the gain takes to move 63% and 90% of the way to where it settles, after the
/// burst starts and after it ends.
fn burst_times(settings: Settings, on: f32, after: f32) -> [f32; 4] {
    let (before, on, after) = (
        RATE as usize / 2,
        (on * RATE as f32) as usize,
        (after * RATE as f32) as usize,
    );
    let signal: Vec<f32> = sine(-20.0, before, 0)
        .chain(sine(0.0, on, before))
        .chain(sine(-20.0, after, before + on))
        .collect();
    let out = through(settings, &signal);
    // The gain over each cycle, from the ratio of the RMS out to in.
    let cycle = (RATE as f32 / TONE_HZ) as usize;
    let gain = |at: usize| level_db(&out[at..at + cycle]) - level_db(&signal[at..at + cycle]);
    let reach = |from: usize, to: usize, start: f32, end: f32, share: f32| {
        let target = start + share * (end - start);
        (from..to - cycle)
            .find(|&at| (gain(at) - target) * (end - start).signum() >= 0.0)
            .map_or(f32::NAN, |at| (at - from) as f32 / RATE as f32 * 1000.0)
    };
    let resting = gain(before - cycle);
    let held = gain(before + on - cycle);
    let end = signal.len() - cycle;
    let let_go = gain(end);
    // The window is a cycle wide, so a time is read from its middle.
    let half = cycle as f32 / 2.0 / RATE as f32 * 1000.0;
    [
        reach(before, before + on, resting, held, 0.63) + half,
        reach(before, before + on, resting, held, 0.9) + half,
        reach(before + on, end, held, let_go, 0.63) + half,
        reach(before + on, end, held, let_go, 0.9) + half,
    ]
}

fn within(got: f32, expected: f32, share: f32) -> bool {
    (got / expected - 1.0).abs() <= share
}

#[test]
fn the_clean_attack_is_a_one_pole_with_no_long_tail() {
    // 10 ms and 30 ms attacks, at a 100 ms and 500 ms release.
    for (attack, release) in [(0xAA, 0x80), (0xD3, 0xD9)] {
        let settings = clean(0xD5, 0x65, attack, release);
        let [rise, risen, ..] = burst_times(settings, 0.5, 1.5);
        let time = attack_ms(f32::from(attack));
        // A one-pole reaches 63% in its time and 90% in 2.3 times it (#334: it took 4.4 times).
        assert!(within(rise, time, 0.15), "{time}: {rise}");
        assert!(within(risen, 2.3 * time, 0.15), "{time}: {risen}");
    }
}

#[test]
fn the_bus_times_follow_bus_forces_at_four_to_one() {
    // Bus FORCE's 63% and 90% times in ms at the same attack, with its Force at 4.3, from
    // `comp-bus-attack`; it goes no faster than 1 ms.
    let bus_force = [
        (2, 4.5, 8.8),
        (3, 6.8, 12.1),
        (4, 13.9, 27.2),
        (5, 33.8, 73.2),
    ];
    for (attack, rise_theirs, risen_theirs) in bus_force {
        let [rise, risen, ..] = burst_times(bus(0xD5, 1, attack, 0), 0.5, 1.0);
        assert!(within(rise, rise_theirs, 0.15), "{attack}: {rise}");
        assert!(within(risen, risen_theirs, 0.15), "{attack}: {risen}");
    }
    // Its releases are their times, as Bus FORCE's are.
    for (release, time) in [(0, 100.0), (1, 300.0)] {
        let [.., fall, _] = burst_times(bus(0xD5, 1, 2, release), 0.5, 2.0);
        assert!(within(fall, time, 0.15), "{release}: {fall}");
    }
}

#[test]
fn the_bus_attack_grabs_faster_at_a_higher_ratio() {
    let rise = |ratio| burst_times(bus(0xD5, ratio, 4, 0), 0.5, 1.0)[0];
    assert!(
        rise(0) > rise(1) && rise(1) > rise(2),
        "{} {} {}",
        rise(0),
        rise(1),
        rise(2)
    );
}

#[test]
fn the_bus_auto_release_lets_go_of_a_short_burst_faster_than_a_long_one() {
    let auto = bus(0xD5, 1, 2, BUS_AUTO_RELEASE);
    let short = burst_times(auto, 0.05, 6.0)[2];
    let long = burst_times(auto, 2.0, 6.0)[2];
    assert!(short < 100.0, "{short}");
    assert!(long > 1000.0, "{long}");
}

#[test]
fn a_clean_compressor_under_its_knee_lets_the_sound_through_bit_for_bit() {
    let settings = clean(0xC0, 0x60, 0x55, 0x80);
    // The threshold is -15 dB and the knee 6 dB wide, so -20 dB is under it.
    assert!(threshold_db(f32::from(settings.threshold)) - 3.0 > -20.0);
    let signal: Vec<f32> = sine(-20.0, RATE as usize, 0).collect();
    assert_eq!(through(settings, &signal), signal);
}

/// A second of a drum-like hit every quarter second, through a track with a compressor of each
/// type in its first slot, as the engine plays it.
fn hits_through(settings: Settings) -> Vec<f32> {
    let mut song = Song::new(1);
    song.tracks[0].inserts[0] = Insert::Compressor(settings);
    let frames: Vec<[f32; 2]> = (0..RATE as usize)
        .map(|frame| {
            let since = (frame % (RATE as usize / 4)) as f32 / RATE as f32;
            let hit = (since * 110.0 * TAU).sin() * (-since * 12.0).exp() * 0.9;
            [hit, hit * 0.8]
        })
        .collect();
    let version = Published::new(Arc::new(song), Arc::new(SamplePool::default()));
    let mut engine = Engine::new(RATE, Arc::new(version));
    engine.play_input(0, Arc::new(SampleData { frames }));
    let mut out = vec![0.0; RATE as usize * 2];
    engine.process(&mut out);
    out
}

#[test]
fn a_track_plays_the_type_its_slot_is_set_to() {
    let bus = bus(0x99, 1, 3, BUS_AUTO_RELEASE);
    let clean = Settings {
        kind: CompressorType::Clean,
        ..bus
    };
    let (bus_out, clean_out) = (hits_through(bus), hits_through(clean));
    assert_ne!(bus_out, clean_out);
    let peak = |out: &[f32]| out.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
    assert!(peak(&bus_out) > 0.1 && peak(&clean_out) > 0.1);
}

#[test]
fn golden_compressor_types_stay_the_same() {
    golden("comp-clean", &hits_through(clean(0x99, 0x80, 0x60, 0x70)));
    golden("comp-bus", &hits_through(bus(0x99, 1, 3, BUS_AUTO_RELEASE)));
}
