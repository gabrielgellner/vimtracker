//! A sampler's `Reverse`, `SREV`, turns a note round from the way its sample or slice plays
//! (decision 412): the part `STA` and `SLEN` choose plays from its other end, as the mirror of
//! itself, round a loop backwards and a ping-pong starting down. A lock turns one row's note, the
//! next note is the track's own again, a macro and an LFO turn it too, and a render repeats.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::bank::{BankEntry, ContentHash};
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::macros::{MacroMap, MacroTarget};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// The samples' length: a second.
const LENGTH: usize = RATE as usize;

/// A tone whose pitch rises, `tone`, the same backwards, `enot`, and a ramp from silence up, so a
/// frame's level says where it was read.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let mut phase = 0.0_f32;
    let tone: Vec<[f32; 2]> = (0..LENGTH)
        .map(|at| {
            phase += (110.0 + 330.0 * at as f32 / LENGTH as f32) / RATE as f32;
            let value = (phase * std::f32::consts::TAU).sin() * 0.5;
            [value, value * 0.8]
        })
        .collect();
    let mut enot = tone.clone();
    enot.reverse();
    pool.insert("tone", SampleData { frames: tone });
    pool.insert("enot", SampleData { frames: enot });
    let ramp = (0..LENGTH)
        .map(|at| at as f32 / LENGTH as f32)
        .map(|value| [value, value])
        .collect();
    pool.insert("ramp", SampleData { frames: ramp });
    pool.insert(
        "steady",
        SampleData {
            frames: vec![[0.5, 0.5]; LENGTH],
        },
    );
    Arc::new(pool)
}

/// One track playing a file's entry, `01`, as an edit says, with notes that start and stop at
/// once, through these rows.
fn song(file: &str, edit: impl FnOnce(&mut BankEntry), rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let mut entry = BankEntry::of_file(file, ContentHash::UNKNOWN);
    edit(&mut entry);
    song.bank.set(1, Some(entry));
    let track = &mut song.tracks[0];
    track.set_setting(SettingId::SAMPLE, 1);
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (at, text) in rows.iter().enumerate() {
        *phrase.step_mut(at).unwrap() = text.parse().unwrap();
    }
    song
}

fn render(song: Song, frames: usize) -> Vec<f32> {
    render_frames(
        Arc::new(Published::new(Arc::new(song), pool())),
        RATE,
        frames,
    )
}

/// The left channel.
fn left(out: &[f32]) -> Vec<f32> {
    out.iter().step_by(2).copied().collect()
}

/// What the track does to a level: a steady sample's level through it, over the steady 0.5.
fn through() -> f32 {
    let out = left(&render(
        song("steady", |_| {}, &["C-3 -- --- --- ---"]),
        1000,
    ));
    out[500] / 0.5
}

/// Where in the ramp a frame was read, from its level.
fn read_at(level: f32, through: f32) -> f32 {
    level / through * LENGTH as f32
}

/// Turn the track's notes round.
fn turned(song: &mut Song) {
    song.tracks[0].set_setting(SettingId::SAMPLE_REVERSE, 1);
}

const NOTE: &str = "C-3 -- --- --- ---";

#[test]
fn turned_round_a_one_shot_plays_the_sample_backwards() {
    // At its own pitch, it's the file backwards played forwards, frame for frame.
    let mut back = song("tone", |_| {}, &[NOTE]);
    turned(&mut back);
    let backwards = song("enot", |_| {}, &[NOTE]);
    let frames = LENGTH + STEP;
    let out = render(back, frames);
    assert!(out.iter().any(|value| value.abs() > 0.1), "it plays");
    assert_eq!(out, render(backwards, frames));
    // Played up a fifth, it's the same within what reading between frames from the other side
    // rounds differently, up to its last read: going forward, a read past the last frame holds
    // it, as it always has, and going back, a read before the first is past the sample.
    let up = "G-3 -- --- --- ---";
    let mut back = song("tone", |_| {}, &[up]);
    turned(&mut back);
    let out = render(back, frames);
    let backwards = render(song("enot", |_| {}, &[up]), frames);
    let lasts = (LENGTH as f64 / 2_f64.powf(7.0 / 12.0)) as usize - 2;
    assert!(out[lasts * 2 - 2..].iter().any(|value| *value != 0.0));
    let most = out[..lasts * 2]
        .iter()
        .zip(&backwards)
        .map(|(one, other)| (one - other).abs())
        .fold(0.0, f32::max);
    assert!(most < 1e-5, "{most}");
}

#[test]
fn turned_round_start_and_length_play_the_same_part_back() {
    let through = through();
    // A quarter in, for half the sample: 12 000 to 36 000, so from 36 000 back.
    let rows = ["C-3 -- sampler.start:40 sampler.length:80 ---"];
    let mut back = song("ramp", |_| {}, &rows);
    turned(&mut back);
    let out = left(&render(back, 6 * STEP));
    let at = |frame: usize| read_at(out[frame], through);
    let into = (LENGTH as f32 * 64.0 / 255.0).round();
    let until = into + (LENGTH as f32 * 128.0 / 255.0).round();
    assert!((at(1000) - (until - 1001.0)).abs() < 2.0, "{}", at(1000));
    assert!(
        (at(20_000) - (until - 20_001.0)).abs() < 2.0,
        "{}",
        at(20_000)
    );
    let length = (until - into) as usize;
    assert!(out[length - 200] > 0.0);
    assert_eq!(out[length + 2], 0.0, "it ended at the part's start");
    // The entry's own reverse turned round plays forward, from the same part's start.
    let mut again = song("ramp", |entry| entry.edit.play.reverse = true, &rows);
    turned(&mut again);
    let out = left(&render(again, 2 * STEP));
    let at = read_at(out[1000], through);
    let from = LENGTH as f32 - (LENGTH as f32 * 64.0 / 255.0) - (LENGTH as f32 * 128.0 / 255.0);
    assert!((at - (from + 1000.0)).abs() < 2.0, "{at} from {from}");
}

#[test]
fn turned_round_slices_play_from_their_ends_and_thru_runs_back_to_the_entrys_start() {
    let through = through();
    let sliced = |entry: &mut BankEntry| {
        let starts = entry.edit.points.even_starts(8, LENGTH as u64);
        entry.edit.points.reslice(&starts);
    };
    let rest = "--- -- --- --- ---";
    let rows = ["C-3 -- sampler.slice:02 --- ---", rest, rest, rest];
    let mut back = song("ramp", sliced, &rows);
    turned(&mut back);
    let out = left(&render(back, 2 * STEP));
    // The third slice is 12 000 to 18 000, read from its end back.
    let at = read_at(out[1000], through);
    assert!((at - (18_000.0 - 1001.0)).abs() < 2.0, "{at}");
    assert_eq!(out[STEP + 2], 0.0, "it ends at the slice's start");
    // On through, it carries on back past the slice's start, to the entry's.
    let mut thru = song("ramp", sliced, &rows);
    turned(&mut thru);
    thru.tracks[0].set_setting(SettingId::PLAY, 3);
    let out = left(&render(thru, 4 * STEP));
    let at = read_at(out[STEP + 1000], through);
    assert!((at - (18_000.0 - STEP as f32 - 1001.0)).abs() < 2.0, "{at}");
    assert_eq!(out[3 * STEP + 2], 0.0, "it ends at the entry's start");
}

#[test]
fn a_lock_turns_its_rows_note_and_the_next_note_is_the_tracks_own() {
    let through = through();
    let rest = "--- -- --- --- ---";
    let rows = [
        "C-3 -- sampler.reverse:01 --- ---",
        rest,
        "C-3 -- --- --- ---",
    ];
    let out = left(&render(song("ramp", |_| {}, &rows), 4 * STEP));
    let at = |frame: usize| read_at(out[frame], through);
    assert!(
        (at(1000) - (LENGTH as f32 - 1001.0)).abs() < 2.0,
        "{}",
        at(1000)
    );
    assert!(
        (at(2 * STEP + 1000) - 1000.0).abs() < 2.0,
        "{}",
        at(2 * STEP + 1000)
    );
    // Over a track that's turned, the lock turns it back for its row.
    let rows = [
        "C-3 -- sampler.reverse:00 --- ---",
        rest,
        "C-3 -- --- --- ---",
    ];
    let mut back = song("ramp", |_| {}, &rows);
    turned(&mut back);
    let out = left(&render(back, 4 * STEP));
    let at = |frame: usize| read_at(out[frame], through);
    assert!((at(1000) - 1000.0).abs() < 2.0, "{}", at(1000));
    assert!(
        (at(2 * STEP + 1000) - (LENGTH as f32 - 1001.0)).abs() < 2.0,
        "{}",
        at(2 * STEP + 1000)
    );
}

#[test]
fn turned_round_a_loop_goes_round_backwards_and_a_ping_pong_starts_down() {
    let through = through();
    // A quarter of the ramp, 12 000 to 24 000, round and round from 24 000 down.
    let rows = ["C-3 -- sampler.start:40 sampler.length:40 ---"];
    let mut looped = song("ramp", |_| {}, &rows);
    turned(&mut looped);
    looped.tracks[0].set_setting(SettingId::PLAY, 1);
    let out = left(&render(looped, 12 * STEP));
    let length = (LENGTH as f32 * 64.0 / 255.0).round() as usize;
    for frame in [30_000, 41_000, 53_000] {
        let at = read_at(out[frame], through);
        assert!((12_000.0..24_100.0).contains(&at), "{at}");
        assert!((out[frame] - out[frame + length]).abs() < 1e-4, "{frame}");
    }
    assert!(out[1001] < out[1000], "it goes down");
    let mut ping_pong = song("ramp", |_| {}, &rows);
    turned(&mut ping_pong);
    ping_pong.tracks[0].set_setting(SettingId::PLAY, 2);
    let out = left(&render(ping_pong, 14 * STEP));
    // Down to the part's start, then up to its end, and round every two lengths.
    assert!(out[1001] < out[1000], "it starts down");
    assert!(
        out[length + 1001] > out[length + 1000],
        "and turns at the start"
    );
    for frame in [30_000, 41_000, 53_000] {
        assert!(
            (out[frame] - out[frame + 2 * length]).abs() < 1e-4,
            "{frame}"
        );
    }
}

#[test]
fn a_macro_and_an_lfo_turn_it_and_a_render_repeats() {
    let through = through();
    let rest = "--- -- --- --- ---";
    let rows = [NOTE, rest, NOTE, rest, NOTE, rest, NOTE, rest];
    // A macro on `SREV`, turned up, turns every note.
    let mut turned_by_macro = song("ramp", |_| {}, &rows);
    turned_by_macro.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(SettingId::SAMPLE_REVERSE),
        min: 0x0000,
        max: 0x0100,
        curve: 0,
        s: 0,
        zone: Default::default(),
        member: None,
    }];
    turned_by_macro.tracks[0].macros[0].value = 0xFF00;
    let out = left(&render(turned_by_macro, 4 * STEP));
    let at = read_at(out[1000], through);
    assert!((at - (LENGTH as f32 - 1001.0)).abs() < 2.0, "{at}");
    // A square LFO a bar a cycle turns the notes in its first half and not in its second.
    let mut by_lfo = song("ramp", |_| {}, &rows);
    by_lfo.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::SAMPLE_REVERSE),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Note(3840 / 2),
            ..Lfo::default()
        },
    )
    .with_depth(0xFF);
    let out = left(&render(by_lfo.clone(), 8 * STEP));
    let directions: Vec<bool> = (0..4)
        .map(|note| out[note * 2 * STEP + 1001] < out[note * 2 * STEP + 1000])
        .collect();
    assert!(directions.contains(&true), "{directions:?}");
    assert!(directions.contains(&false), "{directions:?}");
    assert_eq!(render(by_lfo.clone(), 8 * STEP), render(by_lfo, 8 * STEP));
}
