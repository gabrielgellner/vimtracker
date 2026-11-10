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
    CONSOLE_AUTO_RELEASE, Compressor as Settings, CompressorType, GLUE_AUTO_RELEASE, Insert,
    MOST_MAKEUP_DB, attack_ms, makeup_db, threshold_db,
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

fn console(threshold: u8, ratio: u8, attack: u8, release: u8) -> Settings {
    Settings {
        kind: CompressorType::Console,
        threshold,
        console_ratio: ratio,
        console_attack: attack,
        console_release: release,
        ..Settings::DEFAULT
    }
}

fn glue(threshold: u8, ratio: u8, attack: u8, release: u8) -> Settings {
    Settings {
        kind: CompressorType::Glue,
        threshold,
        glue_ratio: ratio,
        glue_attack: attack,
        glue_release: release,
        ..Settings::DEFAULT
    }
}

#[test]
fn each_types_static_curve_is_the_one_it_draws() {
    let settings = [
        clean(0xAA, 0x3B, 0x55, 0x80),
        clean(0xAA, 0x9B, 0x55, 0x80),
        console(0xAA, 0, 4, 1),
        console(0xAA, 1, 4, 1),
        console(0xAA, 2, 4, 1),
        glue(0xAA, 0, 5, 0),
        glue(0xAA, 1, 5, 0),
        glue(0xAA, 2, 5, 0),
        glue(0xAA, 1, 0, 3),
        glue(0xAA, 1, 6, 5),
        Settings {
            range: 0x16,
            ..glue(0xAA, 2, 5, 0)
        },
    ];
    for settings in settings {
        for level in [-40.0, -25.0, -18.0, -12.0, -6.0, 0.0] {
            let got = settled_out(settings, level);
            let drawn = curve_db(level, settings);
            // A sine's peaks are what's heard, so a steady sine settles on the curve, within the
            // ripple a held peak lets through. Glue's curve is where a sine's ripple settles.
            assert!(
                (got - drawn).abs() < 0.25,
                "{settings:?} {level}: {got} {drawn}",
            );
        }
    }
}

#[test]
fn the_console_curve_follows_bus_forces_within_half_a_db() {
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
        let settings = console(0xAA, ratio as u8, 4, 1);
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
fn the_console_times_follow_bus_forces_at_four_to_one() {
    // Bus FORCE's 63% and 90% times in ms at the same attack, with its Force at 4.3, from
    // `comp-bus-attack`; it goes no faster than 1 ms.
    let bus_force = [
        (2, 4.5, 8.8),
        (3, 6.8, 12.1),
        (4, 13.9, 27.2),
        (5, 33.8, 73.2),
    ];
    for (attack, rise_theirs, risen_theirs) in bus_force {
        let [rise, risen, ..] = burst_times(console(0xD5, 1, attack, 0), 0.5, 1.0);
        assert!(within(rise, rise_theirs, 0.15), "{attack}: {rise}");
        assert!(within(risen, risen_theirs, 0.15), "{attack}: {risen}");
    }
    // Its releases are their times, as Bus FORCE's are.
    for (release, time) in [(0, 100.0), (1, 300.0)] {
        let [.., fall, _] = burst_times(console(0xD5, 1, 2, release), 0.5, 2.0);
        assert!(within(fall, time, 0.15), "{release}: {fall}");
    }
}

#[test]
fn the_console_attack_grabs_faster_at_a_higher_ratio() {
    let rise = |ratio| burst_times(console(0xD5, ratio, 4, 0), 0.5, 1.0)[0];
    assert!(
        rise(0) > rise(1) && rise(1) > rise(2),
        "{} {} {}",
        rise(0),
        rise(1),
        rise(2)
    );
}

#[test]
fn the_console_auto_release_lets_go_of_a_short_burst_faster_than_a_long_one() {
    let auto = console(0xD5, 1, 2, CONSOLE_AUTO_RELEASE);
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
    let console = console(0x99, 1, 3, CONSOLE_AUTO_RELEASE);
    let clean = Settings {
        kind: CompressorType::Clean,
        ..console
    };
    let glue = Settings {
        kind: CompressorType::Glue,
        ..console
    };
    let outs = [console, clean, glue].map(hits_through);
    assert_ne!(outs[0], outs[1]);
    assert_ne!(outs[0], outs[2]);
    assert_ne!(outs[1], outs[2]);
    let peak = |out: &[f32]| out.iter().fold(0.0_f32, |most, s| most.max(s.abs()));
    assert!(outs.iter().all(|out| peak(out) > 0.1));
}

#[test]
fn golden_compressor_types_stay_the_same() {
    golden("comp-clean", &hits_through(clean(0x99, 0x80, 0x60, 0x70)));
    // The console type is the bus type it was renamed from, bit for bit: its hash is the one
    // `comp-bus` had.
    golden(
        "comp-console",
        &hits_through(console(0x99, 1, 3, CONSOLE_AUTO_RELEASE)),
    );
    golden(
        "comp-glue",
        &hits_through(glue(0x99, 1, 3, GLUE_AUTO_RELEASE)),
    );
}

#[test]
fn makeup_is_unity_at_00_and_turns_up_by_24_db_at_ff_in_every_type() {
    // The threshold at 0 dB, so -20 dB is well under it and only the makeup moves it.
    let signal: Vec<f32> = sine(-20.0, RATE as usize, 0).collect();
    for settings in [
        clean(0xFF, 0x60, 0x55, 0x80),
        console(0xFF, 1, 4, 1),
        glue(0xFF, 1, 3, 3),
    ] {
        assert_eq!(settings.makeup, 0x00, "it starts at none");
        assert_eq!(through(settings, &signal), signal, "{:?}", settings.kind);
        let most = Settings {
            makeup: 0xFF,
            ..settings
        };
        let out = through(most, &signal);
        let raised = level_db(&out) - level_db(&signal);
        assert!((raised - MOST_MAKEUP_DB).abs() < 1e-3, "{raised}");
        // From the first frame, as it starts where it's set.
        let gain = 10_f32.powf(MOST_MAKEUP_DB / 20.0);
        assert!(
            out.iter()
                .zip(&signal)
                .all(|(out, went_in)| (out - went_in * gain).abs() < 1e-5)
        );
    }
}

#[test]
fn the_console_under_its_threshold_lets_the_sound_through_bit_for_bit_at_every_ratio() {
    // At -20 dB, Bus FORCE's comp path gives back exactly what went in up to its threshold
    // (`comp-bus-static`), and the G-SSL's ratio switch moves the threshold up, never down.
    for ratio in 0..3 {
        let settings = console(0xAA, ratio, 4, 1);
        for level in [-60.0, -40.0, -30.0, -24.0, -20.0] {
            let signal: Vec<f32> = sine(level, RATE as usize, 0).collect();
            assert_eq!(through(settings, &signal), signal, "{ratio} {level}");
        }
    }
}

#[test]
fn the_console_with_auto_comes_out_as_loud_as_it_went_in_and_makeup_adds_to_that() {
    // A held tone 24 dB over a -30 dB threshold, settled over 8 seconds.
    let signal: Vec<f32> = sine(-6.0, RATE as usize * 8, 0).collect();
    let last_second = |out: &[f32]| level_db(&out[out.len() - RATE as usize..]);
    let went_in = last_second(&signal);
    for ratio in 0..3 {
        let settings = console(0x80, ratio, 4, CONSOLE_AUTO_RELEASE);
        let turned_down = last_second(&through(settings, &signal)) - went_in;
        assert!(turned_down < -5.0, "{ratio}: {turned_down}");
        let auto = Settings {
            auto: 1,
            ..settings
        };
        let given_back = last_second(&through(auto, &signal)) - went_in;
        assert!(given_back.abs() < 0.25, "{ratio}: {given_back}");
        let more = Settings {
            makeup: 0x80,
            ..auto
        };
        let over = last_second(&through(more, &signal)) - went_in;
        let makeup = makeup_db(f32::from(more.makeup));
        assert!((over - makeup).abs() < 0.25, "{ratio}: {over} {makeup}");
    }
}

#[test]
fn makeup_turns_up_only_what_it_turned_down_so_an_all_dry_mix_is_left_alone() {
    let dry = Settings {
        mix: 0x00,
        ..console(0x99, 1, 3, CONSOLE_AUTO_RELEASE)
    };
    let turned_up = Settings {
        makeup: 0xFF,
        ..dry
    };
    assert_eq!(hits_through(dry), hits_through(turned_up));
    let wet = Settings { mix: 0xFF, ..dry };
    assert_ne!(
        hits_through(wet),
        hits_through(Settings {
            mix: 0xFF,
            ..turned_up
        })
    );
}

/// The levels the static curves below are read at, in dBFS, as `live-glue-static` steps them.
const CURVE_LEVELS: [f32; 8] = [-40.0, -30.0, -20.0, -16.0, -12.0, -8.0, -4.0, 0.0];

#[test]
fn the_glue_curve_follows_glues_at_every_ratio() {
    // Glue Compressor at -20 dB with its 10 ms attack and `.1` release, measured through Live by
    // `live-glue-static`: its output in dBFS at each of `CURVE_LEVELS`, at 2, 4 and 10 to 1.
    let glue_compressor: [[f32; 8]; 3] = [
        [
            -40.20, -30.85, -22.82, -20.10, -17.66, -15.49, -13.46, -11.22,
        ],
        [-40.0, -30.0, -20.18, -17.82, -15.95, -14.34, -12.92, -11.64],
        [-40.0, -30.0, -20.0, -16.25, -15.16, -14.34, -13.63, -12.97],
    ];
    for (ratio, theirs) in glue_compressor.iter().enumerate() {
        let settings = glue(0xAA, ratio as u8, 5, 0);
        let errors: Vec<f32> = CURVE_LEVELS
            .iter()
            .zip(theirs)
            .map(|(&level, &out)| settled_out(settings, level) - out)
            .collect();
        let rms = (errors.iter().map(|e| e * e).sum::<f32>() / errors.len() as f32).sqrt();
        assert!(rms < 0.15, "{ratio}: {errors:?}");
        assert!(
            errors.iter().all(|error| error.abs() < 0.3),
            "{ratio}: {errors:?}"
        );
    }
}

#[test]
fn the_glue_times_follow_glues() {
    // Glue's 63% and 90% times in ms at 4:1 with a `.1` release, from `live-glue-attack`.
    for (attack, rise_theirs, risen_theirs) in [
        (3, 2.0, 6.0),
        (4, 5.3, 14.7),
        (5, 12.2, 31.7),
        (6, 26.6, 64.7),
    ] {
        let [rise, risen, ..] = burst_times(glue(0xD5, 1, attack, 0), 0.5, 1.0);
        assert!(within(rise, rise_theirs, 0.15), "{attack}: {rise}");
        assert!(within(risen, risen_theirs, 0.15), "{attack}: {risen}");
    }
    // Its releases at a 1 ms attack, from `live-glue-release`: quicker than their names.
    for (release, fall_theirs) in [(0, 79.4), (1, 116.4), (3, 223.4), (5, 410.5)] {
        let [.., fall, _] = burst_times(glue(0xD5, 1, 3, release), 0.5, 2.0);
        assert!(within(fall, fall_theirs, 0.15), "{release}: {fall}");
    }
    // At 10 ms, the loop grabs faster the higher the ratio, as Glue's 16.2, 12.2 and 6.1 ms.
    for (ratio, rise_theirs) in [(0, 16.2), (1, 12.2), (2, 6.1)] {
        let [rise, ..] = burst_times(glue(0xD5, ratio, 5, 0), 0.5, 1.0);
        assert!(within(rise, rise_theirs, 0.15), "{ratio}: {rise}");
    }
    // Auto lets a 20 ms burst go in about 48 ms and two seconds' in about 2.4 s.
    let auto = glue(0xD5, 1, 3, GLUE_AUTO_RELEASE);
    let short = burst_times(auto, 0.02, 6.0)[2];
    let long = burst_times(auto, 2.0, 6.0)[2];
    assert!(within(short, 48.3, 0.2), "{short}");
    assert!(within(long, 2420.0, 0.15), "{long}");
}

#[test]
fn the_glue_range_caps_how_far_it_turns_down() {
    // A tone 30 dB over a -30 dB threshold at 10:1, which Glue turns down by 21 dB with its
    // range at 40 dB, 13.3 dB at 20 and 3.4 dB at 6 (`live-glue-range`).
    let reduction = |range: u8| {
        let settings = Settings {
            range,
            ..glue(0x80, 2, 5, 0)
        };
        0.0 - settled_out(settings, 0.0)
    };
    for (range, theirs) in [(0x92, 21.02), (0x49, 13.28), (0x16, 3.42)] {
        let ours = reduction(range);
        assert!((ours - theirs).abs() < 0.3, "{range:x}: {ours} {theirs}");
    }
    // None of it turns nothing down, bit for bit.
    let none = Settings {
        range: 0x00,
        ..glue(0x80, 2, 5, 0)
    };
    let signal: Vec<f32> = sine(0.0, RATE as usize / 2, 0).collect();
    assert_eq!(through(none, &signal), signal);
}

#[test]
fn glue_under_its_knee_lets_the_sound_through_bit_for_bit() {
    // Its knee starts 1.4 dB under the threshold at 4:1 and 3.4 dB over it at 10:1, but 47 dB
    // under it at 2:1, which Glue turns down by 0.2 dB 20 dB under.
    for (ratio, levels) in [
        (0, &[-90.0, -70.0][..]),
        (1, &[-60.0, -40.0, -30.0, -22.0][..]),
        (2, &[-60.0, -40.0, -30.0, -20.0, -18.0][..]),
    ] {
        let settings = glue(0xAA, ratio, 5, 0);
        for &level in levels {
            let signal: Vec<f32> = sine(level, RATE as usize / 2, 0).collect();
            assert_eq!(through(settings, &signal), signal, "{ratio} {level}");
        }
    }
}

#[test]
fn glue_never_makes_a_nan_at_any_setting() {
    // Silence, a tone, a step far over full scale and back, and the smallest numbers there are.
    let signal: Vec<f32> = (0..2400)
        .map(|frame| match frame {
            0..400 => 0.0,
            400..800 => ((frame as f32) * 0.13).sin(),
            800..1200 => 1.0e4 * ((frame as f32) * 0.07).sin(),
            1200..1600 => f32::MIN_POSITIVE * 0.5,
            _ => 1.0e-3 * ((frame as f32) * 0.3).sin(),
        })
        .collect();
    for ratio in 0..3 {
        for attack in 0..7 {
            for release in 0..=GLUE_AUTO_RELEASE {
                for range in [0x00, 0x01, 0x40, 0xFF] {
                    for threshold in [0x00, 0xFF] {
                        let settings = Settings {
                            range,
                            auto: 1,
                            ..glue(threshold, ratio, attack, release)
                        };
                        let mut compressor = Compressor::default();
                        for &sample in &signal {
                            let out =
                                compressor.process([sample; 2], [sample; 2], settings, 1.0, RATE);
                            assert!(
                                out.iter().all(|s| s.is_finite())
                                    && compressor.reduction().is_finite(),
                                "{settings:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
