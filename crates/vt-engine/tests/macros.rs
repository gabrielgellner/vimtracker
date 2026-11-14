//! A track's macros as they play (decision 284): a turn, a step's `MAC` lock and an LFO on a
//! macro each move every setting it holds, within its range, and a setting it lets go of goes
//! back to the song's.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::insert::{Delay, Insert};
use vt_model::lfo::{Lfo, Rate};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::{Effect, SettingId};
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

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

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), flat()))
}

fn map(target: SettingId, min: u16, max: u16) -> MacroMap {
    MacroMap {
        target: MacroTarget::Setting(target),
        min,
        max,
        curve: 0,
        s: 0,
    }
}

/// The delay's `Feedback`, in slot 1.
fn feedback() -> SettingId {
    SettingId::slot(0, Effect::Delay, 1).unwrap()
}

/// One track, with a delay in slot 1, and macro 1 holding `Cut` from `30` to `A0`, `Res` the
/// other way from `C0` to `20`, and the delay's `Feedback` from `10` to `90`. Its notes are
/// these steps.
fn song(texts: &[&str]) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    let track = &mut song.tracks[0];
    track.cutoff = 0x80;
    track.resonance = 0x40;
    track.inserts[0] = Insert::Delay(Delay::DEFAULT);
    track.macros[0].maps = vec![
        map(SettingId::CUT, 0x3000, 0xA000),
        map(SettingId::RES, 0xC000, 0x2000),
        map(feedback(), 0x1000, 0x9000),
    ];
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (index, text) in texts.iter().enumerate() {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song
}

/// Where the delay's `Feedback` is among what slot 1 reports.
fn feedback_place() -> usize {
    Insert::Delay(Delay::DEFAULT)
        .params()
        .iter()
        .position(|param| param.setting == Effect::Delay.params()[1].setting)
        .unwrap()
}

/// What's heard of the three settings macro 1 holds, after a block of this many frames.
fn heard(engine: &mut Engine, frames: usize) -> (u8, u8, u8) {
    let mut out = vec![0.0; frames * 2];
    engine.process(&mut out);
    let live = engine.live(0);
    (
        live.cutoff,
        live.resonance,
        live.inserts[0][feedback_place()],
    )
}

#[test]
fn a_turn_puts_every_setting_the_macro_holds_along_its_range() {
    let mut song = song(&[]);
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(
        heard(&mut engine, 64),
        (0x30, 0xC0, 0x10),
        "at 00, each at its min"
    );
    for (value, expected) in [
        (0xFF00, (0xA0, 0x20, 0x90)),
        (0x8000, (0x68, 0x70, 0x50)),
        (0x4000, (0x4C, 0x98, 0x30)),
    ] {
        song.tracks[0].macros[0].value = value;
        engine.swap(version(&song));
        assert_eq!(heard(&mut engine, 64), expected, "at {value:04X}");
        assert_eq!(engine.live(0).macros[0], value);
    }
    // Each mapping bent its own way (decision 314), and heard where `MacroMap::at` puts it.
    let maps = &mut song.tracks[0].macros[0].maps;
    maps[0] = maps[0].bent(60, 0);
    maps[1] = maps[1].bent(-60, 40);
    let bent = song.tracks[0].macros[0].maps.clone();
    let coarse = |at: usize| (bent[at].at(64.0) >> 8) as u8;
    song.tracks[0].macros[0].value = 0x4000;
    engine.swap(version(&song));
    let (cut, res, feedback) = heard(&mut engine, 64);
    assert_eq!((cut, res, feedback), (coarse(0), coarse(1), 0x30));
    assert!(cut > 0x4C + 0x10, "Cut moves early: {cut:02X}");
    assert!(res > 0x98 + 0x08, "Res, turned down, moves late: {res:02X}");
}

#[test]
fn a_setting_let_go_goes_back_to_the_songs_and_one_moved_to_another_macro_follows_it() {
    let mut song = song(&[]);
    song.tracks[0].macros[0].value = 0xFF00;
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(heard(&mut engine, 64).0, 0xA0);
    song.tracks[0].macros[0].maps.remove(0);
    engine.swap(version(&song));
    assert_eq!(
        heard(&mut engine, 64),
        (0x80, 0x20, 0x90),
        "Cut is the song's again"
    );
    song.tracks[0].macros[3].maps = vec![map(SettingId::CUT, 0x0000, 0x4000)];
    engine.swap(version(&song));
    assert_eq!(heard(&mut engine, 64).0, 0x00, "macro 4 is at 00");
}

#[test]
fn a_mac_lock_turns_the_macro_and_all_it_holds_for_its_row() {
    // A lock on the first row, a note with none on the third.
    let song = song(&[
        "C-3 -- macro1:FF --- ---",
        "--- -- --- --- ---",
        "C-3 -- --- --- ---",
    ]);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    assert_eq!(heard(&mut engine, STEP), (0xA0, 0x20, 0x90));
    assert_eq!(engine.live(0).macros[0], 0xFF00);
    assert_eq!(
        heard(&mut engine, STEP),
        (0xA0, 0x20, 0x90),
        "the lock holds"
    );
    assert_eq!(
        heard(&mut engine, STEP),
        (0x30, 0xC0, 0x10),
        "the next note ends it"
    );
}

#[test]
fn an_lfo_on_a_macro_sweeps_every_setting_it_holds_within_its_range_together() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].macros[0].value = 0x8000;
    song.tracks[0].mods[3] = ModSlot::lfo(
        SettingId::macro_at(0),
        Lfo {
            rate: Rate::Steps(4),
            ..Lfo::default()
        },
    )
    .with_depth(0x7F);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let seen: Vec<_> = (0..64).map(|_| heard(&mut engine, STEP / 8)).collect();
    let cutoffs: Vec<_> = seen.iter().map(|heard| heard.0).collect();
    let (lowest, highest) = (
        *cutoffs.iter().min().unwrap(),
        *cutoffs.iter().max().unwrap(),
    );
    assert!(lowest >= 0x30 && highest <= 0xA0, "{cutoffs:?}");
    assert!(highest - lowest > 0x50, "it sweeps: {cutoffs:?}");
    for (cutoff, resonance, feedback) in &seen {
        assert!((0x20..=0xC0).contains(resonance), "{seen:?}");
        assert!((0x10..=0x90).contains(feedback), "{seen:?}");
        // Each is the same way along its own range, `Res` turned the other way.
        let along = f32::from(cutoff - 0x30) / f32::from(0xA0 - 0x30_u8);
        let res_along = f32::from(0xC0 - resonance) / f32::from(0xC0 - 0x20_u8);
        let feedback_along = f32::from(feedback - 0x10) / f32::from(0x90 - 0x10_u8);
        assert!((along - res_along).abs() < 0.03, "{seen:?}");
        assert!((along - feedback_along).abs() < 0.03, "{seen:?}");
    }
    let macros: Vec<_> = (0..16)
        .map(|_| {
            heard(&mut engine, STEP / 4);
            engine.live(0).macros[0]
        })
        .collect();
    assert!(macros.iter().any(|value| *value > 0x9000), "{macros:X?}");
    assert!(macros.iter().any(|value| *value < 0x7000), "{macros:X?}");
}

#[test]
fn a_song_with_macros_holding_nothing_is_as_one_without() {
    let mut song = song(&["C-3 -- --- --- ---"]);
    song.tracks[0].macros[0].maps.clear();
    let mut with = song.clone();
    with.tracks[0].macros[2].value = 0x8000;
    with.tracks[0].macros[5].name = Some("Nothing".to_owned());
    let render = |song: &Song| vt_engine::render::render_frames(version(song), RATE, STEP * 2);
    assert_eq!(render(&song), render(&with));
}
