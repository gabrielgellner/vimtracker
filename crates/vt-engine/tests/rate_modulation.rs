//! An LFO's and a Steps slot's `Rate` moved by a macro or modulation (decision 301): each keeps a
//! running phase, counted by the song's ticks from where it was at each change of rate, so moving
//! the rate speeds it up from where it is rather than jumping, a synced rate stays on the bar, a
//! render repeats whatever the blocks, and a song whose rates never move plays as it always did,
//! from the top or from anywhere.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::lfo::{Fade, Lfo, LfoMode, Rate, Shape};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;
use vt_model::steps::{Loop, StepRate, Steps, StepsMode};

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM, and 16 to the bar.
const STEP: usize = 6000;
const BAR: usize = 16 * STEP;
/// How often the modulation is worked out, in frames.
const UPDATE: usize = 32;
/// A whole turn of an LFO's reported phase.
const TURN: i64 = 1 << 16;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 20],
        },
    );
    Arc::new(pool)
}

/// A slow tone, so the filter's moves are heard.
fn tone() -> Arc<SamplePool> {
    let frames = (0..RATE as usize * 2)
        .map(|frame| {
            let at = frame as f32 / RATE as f32;
            let value = (at * 220.0 * std::f32::consts::TAU).sin() * 0.5;
            [value, value]
        })
        .collect();
    let mut pool = SamplePool::default();
    pool.insert("s", SampleData { frames });
    Arc::new(pool)
}

fn version(song: &Song, pool: &Arc<SamplePool>) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(pool)))
}

/// One track of a sample playing these steps, held at `CUT 80`.
fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.sample_on(0, "s");
    let track = &mut song.tracks[0];
    track.cutoff = 0x80;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (index, text) in texts
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// A rising LFO at a rate on `Cut`.
fn ramp(rate: Rate) -> ModSlot {
    ModSlot::lfo(
        Some(SettingId::CUT),
        Lfo {
            shape: Shape::Ramp,
            rate,
            ..Lfo::default()
        },
    )
    .with_depth(0x40)
}

/// A slot's `Rate`, as a macro or modulation reaches it.
fn rate_of(slot: usize) -> SettingId {
    SettingId::mod_own(slot, 9).unwrap()
}

/// Macro 1 holding a slot's `Rate`, from one rate at `00` to another at `FF`.
fn rate_on_macro(song: &mut Song, slot: usize, from: u16, to: u16) {
    song.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(rate_of(slot)),
        min: from,
        max: to,
        curve: 0,
        s: 0,
    }];
}

/// An engine playing a song from the top, and how many frames it has played.
struct Playing {
    engine: Engine,
    played: usize,
}

impl Playing {
    fn new(song: &Song, pool: &Arc<SamplePool>) -> Self {
        let mut engine = Engine::new(RATE, version(song, pool));
        engine.play_from(0);
        Self { engine, played: 0 }
    }

    /// Play on to a frame.
    fn to(&mut self, frame: usize) {
        let mut out = vec![0.0; (frame - self.played) * 2];
        self.engine.process(&mut out);
        self.played = frame;
    }

    /// Where Mod 4 is in its wave as worked out at a frame on the grid it's worked out on.
    fn phase_at(&mut self, frame: usize) -> i64 {
        self.to(frame + 1);
        i64::from(self.engine.live(0).mods[3].expect("the LFO runs").phase)
    }
}

/// How far apart two places in a turn are, the short way round.
fn apart(a: i64, b: i64) -> i64 {
    let gap = (a - b).rem_euclid(TURN);
    gap.min(TURN - gap)
}

#[test]
fn a_long_synced_lfo_stays_on_the_bar_while_a_macro_lock_moves_its_rate() {
    // `1/8` for the first half of every bar, from the lock to the next note, and `1/4` for the
    // second: six whole cycles a bar.
    let mut song = song(&[
        "C-3 -- macro1:FF --- ---",
        "",
        "",
        "",
        "",
        "",
        "",
        "",
        "C-3 -- --- --- ---",
    ]);
    song.tracks[0].mods[3] = ramp(Rate::Note(960));
    rate_on_macro(
        &mut song,
        3,
        Rate::Note(960).place(),
        Rate::Note(480).place(),
    );
    let mut playing = Playing::new(&song, &flat());
    // A step into the bar it's half a turn on at `1/8`, and a step into its second half a
    // quarter on from the bar's four cycles, at `1/4`: it sped up and slowed down.
    assert!(apart(playing.phase_at(STEP), TURN / 2) < 600);
    assert!(apart(playing.phase_at(9 * STEP), TURN / 4) < 600);
    for bar in 1..=24 {
        let phase = playing.phase_at(bar * BAR);
        assert!(apart(phase, 0) <= 4, "bar {bar} starts at {phase}");
    }
}

#[test]
fn a_turn_of_the_macro_speeds_the_lfo_up_from_where_it_is() {
    let pool = flat();
    let mut song = song(&[]);
    song.tracks[0].mods[3] = ramp(Rate::Note(960));
    rate_on_macro(
        &mut song,
        3,
        Rate::Note(960).place(),
        Rate::Note(240).place(),
    );
    let mut playing = Playing::new(&song, &pool);
    // Each 32 frames is 1.28 ticks: 87 of a turn's 65536 at `1/4`, and 350 at `1/16`.
    let mut last = playing.phase_at(0);
    let mut moves = Vec::new();
    for at in 1..1000 {
        if at == 500 {
            song.tracks[0].macros[0].value = 0xFF00;
            playing.engine.swap(version(&song, &pool));
        }
        let phase = playing.phase_at(at * UPDATE);
        moves.push((phase - last).rem_euclid(TURN));
        last = phase;
    }
    assert!(moves.iter().all(|by| (80..=360).contains(by)), "{moves:?}");
    assert!(moves[..490].iter().all(|by| (86..=89).contains(by)));
    assert!(moves[510..].iter().all(|by| (348..=351).contains(by)));
}

#[test]
fn an_lfo_on_an_lfos_rate_moves_it_smoothly_and_a_render_repeats_whatever_the_blocks() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    let track = &mut song.tracks[0];
    track.mods[2] = ModSlot::lfo(
        Some(rate_of(3)),
        Lfo {
            shape: Shape::Sine,
            rate: Rate::Note(3840),
            ..Lfo::default()
        },
    )
    .with_depth(0x30);
    track.mods[3] = ramp(Rate::Note(480));
    let pool = flat();
    let mut playing = Playing::new(&song, &pool);
    let mut last = playing.phase_at(0);
    let mut moves = Vec::new();
    for at in 1..4 * BAR / UPDATE {
        let phase = playing.phase_at(at * UPDATE);
        moves.push((phase - last).rem_euclid(TURN));
        last = phase;
    }
    // From `1/4d` to `1/32`, about 58 to 699 a step of 32 frames, and never a jump.
    let (slowest, fastest) = (*moves.iter().min().unwrap(), *moves.iter().max().unwrap());
    assert!(slowest >= 50 && fastest <= 710, "{slowest} to {fastest}");
    assert!(fastest > 4 * slowest, "it speeds up and slows down");

    // The same, heard, in blocks of 512 and of 37 frames, twice.
    let render = |block: usize| {
        let mut engine = Engine::new(RATE, version(&song, &tone()));
        engine.play_from(0);
        let mut out = Vec::new();
        let mut buffer = vec![0.0; block * 2];
        let frames = 2 * BAR;
        while out.len() < frames * 2 {
            let run = block.min(frames - out.len() / 2);
            engine.process(&mut buffer[..run * 2]);
            out.extend_from_slice(&buffer[..run * 2]);
        }
        out
    };
    let whole = render(512);
    assert!(whole.iter().any(|sample| *sample != 0.0));
    assert!(whole == render(512), "a render repeats");
    assert!(whole == render(37), "whatever the blocks");
}

#[test]
fn a_turn_moves_steps_on_faster_from_the_step_they_are_on() {
    let pool = flat();
    let mut song = song(&[]);
    let mut values = [0; 16];
    for (at, value) in values.iter_mut().enumerate() {
        *value = at as i16 * 8;
    }
    song.tracks[0].mods[3] = ModSlot::steps(
        Some(SettingId::CUT),
        Steps {
            values,
            mode: StepsMode::Free,
            length: 16,
            rate: StepRate::Note(240),
            ..Steps::default()
        },
    );
    rate_on_macro(
        &mut song,
        3,
        StepRate::Note(240).place().unwrap(),
        StepRate::Note(60).place().unwrap(),
    );
    let mut playing = Playing::new(&song, &pool);
    let mut heard = Vec::new();
    // Halfway through the third step, at `1/16`.
    let turned = (2 * STEP + STEP / 2).div_ceil(UPDATE);
    for at in 1..800 {
        if at == turned {
            song.tracks[0].macros[0].value = 0xFF00;
            playing.engine.swap(version(&song, &pool));
        }
        playing.to(at * UPDATE + 1);
        heard.push(playing.engine.live(0).steps[3].unwrap());
    }
    assert_eq!(heard[turned - 2], 2);
    assert!(
        heard[turned..turned + 10].iter().all(|step| *step == 2),
        "it goes on from the step it's on: {heard:?}"
    );
    assert!(
        heard.windows(2).all(|pair| {
            let on = (i16::from(pair[1]) - i16::from(pair[0])).rem_euclid(16);
            on <= 1
        }),
        "never skipping one: {heard:?}"
    );
    // Then a step each 1500 frames rather than 6000.
    let after = &heard[turned + 60..];
    let changes = after.windows(2).filter(|pair| pair[0] != pair[1]).count();
    let expected = after.len() * UPDATE / (STEP / 4);
    assert!(changes.abs_diff(expected) <= 1, "{changes} for {expected}");
}

#[test]
fn a_resting_track_keeps_its_lfo_going_at_the_rate_it_was_moved_to() {
    let pool = flat();
    let mut song = song(&[]);
    song.tracks[0].mods[3] = ramp(Rate::Note(960));
    rate_on_macro(
        &mut song,
        3,
        Rate::Note(960).place(),
        Rate::Note(240).place(),
    );
    let phases = |resting: bool| {
        let mut song = song.clone();
        let mut playing = Playing::new(&song, &pool);
        playing.engine.let_rest(resting);
        playing.to(STEP);
        // Faster, then nowhere to go, so the silent track rests, then back on `Cut`.
        song.tracks[0].macros[0].value = 0xFF00;
        playing.engine.swap(version(&song, &pool));
        playing.to(2 * STEP);
        song.tracks[0].mods[3] = song.tracks[0].mods[3].with_depth(0);
        playing.engine.swap(version(&song, &pool));
        playing.to(8 * STEP + 7);
        assert!(playing.engine.rests(0) || !resting, "it rests");
        song.tracks[0].mods[3] = song.tracks[0].mods[3].with_depth(0x40);
        playing.engine.swap(version(&song, &pool));
        (0..64)
            .map(|at| playing.phase_at(8 * STEP + 32 + at * UPDATE))
            .collect::<Vec<_>>()
    };
    let rested = phases(true);
    assert_eq!(rested, phases(false));
    // Seven steps at `1/16` from where it was a step in, and a quarter turn at `1/4`.
    assert!(apart(rested[0], TURN / 4) < 600, "{}", rested[0]);
}

/// Five tracks, each with LFOs on `Cut`, `Send A` and `Vol` in one of the five modes at a note, a
/// dotted note, steps, hertz and bars, and a Steps slot on `Res` in each of its modes at a tick, a
/// division or a pass, some with macros holding other settings, so each slot is read through a
/// copy. None of their rates moves.
fn unmoved() -> Song {
    let mut song = Song::new(5);
    song.seed = 11;
    let note = "C-3 -- --- --- ---";
    let rows: [&[&str]; 5] = [
        &[note, "", "", "E-3 -- --- --- ---", "", "G-3 -- --- --- ---"],
        &[note, "", "", "", "", "", "", "", "D-3 -- --- --- ---"],
        &[note, "", note, "", "", note],
        &["", "", note, "", "", "", "", note],
        &[note, "", "", "", "", "", "", note, "", "", note],
    ];
    let lfo_rates = [
        Rate::Note(960),
        Rate::Note(1440),
        Rate::Steps(7),
        Rate::Hertz(130),
        Rate::Note(7680),
    ];
    let shapes = [
        Shape::Sine,
        Shape::Tri,
        Shape::Saw,
        Shape::Random,
        Shape::Smooth,
    ];
    let step_rates = [
        StepRate::Tick,
        StepRate::Note(240),
        StepRate::Pass,
        StepRate::Note(720),
        StepRate::Note(120),
    ];
    for (index, rows) in rows.iter().enumerate() {
        song.sample_on(index, "s");
        let track = &mut song.tracks[index];
        let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
        for (at, text) in rows.iter().enumerate().filter(|(_, text)| !text.is_empty()) {
            *phrase.step_mut(at).unwrap() = text.parse().unwrap();
        }
        track.cutoff = 0x80;
        for (at, to) in [
            (3, SettingId::CUT),
            (4, SettingId::SEND_A),
            (5, SettingId::VOL),
        ] {
            let lfo = Lfo {
                shape: shapes[(index + at) % 5],
                rate: lfo_rates[(index + at * 2) % 5],
                phase: (index * 40) as u16,
                mode: LfoMode::ALL[index],
                deform: if at == 4 { 0x40 } else { 0 },
                fade: if index == 2 && at == 3 {
                    Fade::In(960)
                } else {
                    Fade::Off
                },
            };
            track.mods[at] = ModSlot::lfo(Some(to), lfo).with_depth(0x30);
        }
        let mut values = [0; 16];
        for (at, value) in values.iter_mut().enumerate() {
            *value = (at as i16 * 37) % 64 - 32;
        }
        let steps = Steps {
            values,
            mode: StepsMode::ALL[index % 3],
            length: 5 + index as u8,
            rate: step_rates[index],
            looping: if index == 3 {
                Loop::Bounce
            } else {
                Loop::From(1)
            },
            mutate: if index == 4 { 30 } else { 0 },
            smooth: [0, 0x60, 0, -0x90, 0][index],
            ..Steps::default()
        };
        track.mods[2] = ModSlot::steps(Some(SettingId::RES), steps);
        if index % 2 == 0 {
            track.macros[0].value = 0x8000;
            track.macros[0].maps.push(MacroMap {
                target: MacroTarget::Setting(SettingId::CUT),
                min: 0x2000,
                max: 0xE000,
                curve: 0,
                s: 0,
            });
            track.macros[1].value = 0x4000;
            track.macros[1].maps.push(MacroMap {
                target: MacroTarget::Setting(SettingId::mod_own(1, 1).unwrap()),
                min: 0x2000,
                max: 0x9000,
                curve: 0,
                s: 0,
            });
        }
    }
    song
}

#[test]
fn golden_lfos_and_steps_whose_rates_never_move_play_as_before_from_anywhere() {
    for start in [0, 37] {
        let mut engine = Engine::new(RATE, version(&unmoved(), &tone()));
        engine.play_from(start);
        let mut out = vec![0.0; 4 * BAR * 2];
        engine.process(&mut out);
        golden(&format!("unmoved-rates-from-{start}"), &out);
    }
}
