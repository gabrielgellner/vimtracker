//! A sampler plays a span of its bank entry: from its start to its end, round a loop forward or
//! back and forth, reversed, with fades, and cut by the track's `STA`, `SLEN` and `PLY`, with a
//! declick wherever it cuts into the sound. Moving a point while it plays is heard at once.

mod support;

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::bank::{BankEntry, ContentHash, Loop, PlayParams};
use vt_model::constants::DECLICK_FRAMES;
use vt_model::sample::SampleData;
use vt_model::samples::{Rates, SamplePool};
use vt_model::settings::SettingId;
use vt_model::song::Song;

use support::golden;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp's length: a second.
const LENGTH: usize = RATE as usize;

/// A ramp from silence up, so a frame's level says where it was read, a steady level, and a tone
/// whose pitch rises, for the golden renders to hear loops in.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
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
    let mut phase = 0.0_f32;
    let tone = (0..LENGTH)
        .map(|at| {
            phase += (110.0 + 330.0 * at as f32 / LENGTH as f32) / RATE as f32;
            let value = (phase * std::f32::consts::TAU).sin() * 0.5;
            [value, value * 0.8]
        })
        .collect();
    pool.insert("tone", SampleData { frames: tone });
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
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    for (at, text) in rows.iter().enumerate() {
        *phrase.step_mut(at).unwrap() = text.parse().unwrap();
    }
    song
}

/// Play on `Loop` or `Ping-pong`.
fn play(song: &mut Song, mode: u8) {
    song.tracks[0].set_setting(SettingId::PLAY, mode);
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
        song("steady", |_| {}, &["C-4 -- --- --- ---"]),
        1000,
    ));
    out[500] / 0.5
}

/// Where in the ramp a frame was read, from its level.
fn read_at(level: f32, through: f32) -> f32 {
    level / through * LENGTH as f32
}

/// The largest step from one frame to the next.
fn largest_step(out: &[f32]) -> f32 {
    out.windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

/// Points at 0.25 s, 0.75 s, and a loop from 0.5 s to 0.625 s.
fn points(entry: &mut BankEntry) {
    entry.edit.points.start = 12_000;
    entry.edit.points.end = Some(36_000);
    entry.edit.points.loop_ = Some(Loop {
        start: 24_000,
        end: 30_000,
        crossfade: 0,
    });
}

#[test]
fn a_span_plays_from_its_start_to_its_end() {
    let through = through();
    let out = left(&render(
        song("ramp", points, &["C-4 -- --- --- ---"]),
        5 * STEP,
    ));
    let at = |frame: usize| read_at(out[frame], through);
    assert!((at(1000) - 13_000.0).abs() < 2.0, "{}", at(1000));
    assert!((at(20_000) - 32_000.0).abs() < 2.0, "{}", at(20_000));
    assert_eq!(out[24_001], 0.0, "it ends at 0.75 s");
    assert!(out[23_990] > 0.0);
}

#[test]
fn sta_and_slen_cut_the_span_on_a_step() {
    let through = through();
    let rows = [
        "C-4 -- sampler.start:80 --- ---",
        "--- -- --- --- ---",
        "C-4 -- sampler.length:40 --- ---",
    ];
    let out = left(&render(song("ramp", points, &rows), 6 * STEP));
    // Half-way into the span, which is 24000 frames long.
    let at = |frame: usize| read_at(out[frame], through);
    let half = 12_000.0 + 24_000.0 * 128.0 / 255.0;
    assert!((at(1000) - (half + 1000.0)).abs() < 2.0, "{}", at(1000));
    // A quarter of it, from its start: 6000 frames, so it ends before the next step.
    let third = 2 * STEP;
    assert!((at(third + 1000) - 13_000.0).abs() < 2.0);
    let quarter = (24_000.0 * 64.0 / 255.0) as usize;
    assert!(out[third + quarter - 200] > 0.0);
    assert_eq!(out[third + quarter + 2], 0.0, "it ended");
}

#[test]
fn a_loop_goes_round_the_entrys_loop_and_ping_pong_back_and_forth() {
    let through = through();
    let mut looped = song("ramp", points, &["C-4 -- --- --- ---"]);
    play(&mut looped, 1);
    let out = left(&render(looped, 12 * STEP));
    // Long after the first pass, it's in the loop, round every 6000 frames.
    for frame in [40_000, 51_000, 63_000] {
        let at = read_at(out[frame], through);
        assert!((24_000.0 - 64.0..30_000.0).contains(&at), "{at}");
        assert!((out[frame] - out[frame + 6000]).abs() < 1e-4);
    }
    let mut ping_pong = song("ramp", points, &["C-4 -- --- --- ---"]);
    play(&mut ping_pong, 2);
    let out = left(&render(ping_pong, 14 * STEP));
    // Up to the loop's end, then back down to its start, every 12000 frames.
    for frame in [40_000, 51_000, 63_000] {
        let at = read_at(out[frame], through);
        assert!((24_000.0 - 1.0..30_000.0).contains(&at), "{at}");
        assert!((out[frame] - out[frame + 12_000]).abs() < 1e-4);
    }
    let rising = out[50_000] < out[50_001];
    assert_ne!(rising, out[56_000] < out[56_001], "it turns");
}

#[test]
fn reversed_it_plays_from_its_end_back_to_its_start() {
    let through = through();
    let reversed = |entry: &mut BankEntry| {
        points(entry);
        entry.edit.play.reverse = true;
    };
    let rest = "--- -- --- --- ---";
    let rows = [
        "C-4 -- --- --- ---",
        rest,
        rest,
        rest,
        "C-4 -- sampler.start:40 --- ---",
    ];
    let out = left(&render(song("ramp", reversed, &rows), 8 * STEP));
    let at = |frame: usize| read_at(out[frame], through);
    assert!((at(1000) - 35_000.0).abs() < 2.0, "{}", at(1000));
    // A quarter in from where it starts, its end.
    let quarter = 36_000.0 - 24_000.0 * 64.0 / 255.0;
    assert!((at(4 * STEP + 1000) - (quarter - 1000.0)).abs() < 2.0);
}

#[test]
fn fades_shape_the_span_and_its_gain_and_tune_are_its_own() {
    let through = through();
    let faded = |entry: &mut BankEntry| {
        points(entry);
        entry.edit.play = PlayParams {
            fade_in: 4800,
            fade_out: 4800,
            ..PlayParams::default()
        };
    };
    let out = left(&render(
        song("steady", faded, &["C-4 -- --- --- ---"]),
        4 * STEP,
    ));
    let level = |frame: usize| out[frame] / through / 0.5;
    assert!((level(2400) - 0.5).abs() < 0.01, "half way in");
    assert!((level(12_000) - 1.0).abs() < 0.01);
    assert!(
        (level(24_000 - 1200) - 0.25).abs() < 0.01,
        "a quarter from the end"
    );
    let tuned = |entry: &mut BankEntry| {
        points(entry);
        entry.edit.play.tune = 12;
        entry.edit.play.cents = 50;
        entry.edit.play.gain_db = -6.0;
    };
    let out = left(&render(
        song("ramp", tuned, &["C-4 -- --- --- ---"]),
        2 * STEP,
    ));
    let at = read_at(out[1000] / 10_f32.powf(-6.0 / 20.0), through);
    let faster = 2_f32.powf(12.5 / 12.0);
    assert!(
        (at - (12_000.0 + 1000.0 * faster)).abs() < 2.0,
        "an octave and a quarter tone up: {at}"
    );
}

#[test]
fn a_spans_edges_never_step_further_than_a_declick() {
    let through = through();
    // A steady level steps furthest where it's cut, and the most a declick allows is its level
    // over the declick's length.
    let most = 0.5 * through / DECLICK_FRAMES as f32 * 1.01;
    for mode in 0..3 {
        let mut cut = song("steady", points, &["C-4 -- sampler.start:20 --- ---"]);
        play(&mut cut, mode);
        let out = left(&render(cut, 14 * STEP));
        assert!(largest_step(&out) <= most, "{mode}: {}", largest_step(&out));
    }
    let reversed = |entry: &mut BankEntry| {
        points(entry);
        entry.edit.play.reverse = true;
    };
    let out = left(&render(
        song("steady", reversed, &["C-4 -- --- --- ---"]),
        14 * STEP,
    ));
    assert!(largest_step(&out) <= most, "{}", largest_step(&out));
}

#[test]
fn a_forward_loops_crossfade_runs_into_its_start() {
    let through = through();
    let crossfaded = |entry: &mut BankEntry| {
        points(entry);
        entry.edit.points.loop_ = Some(Loop {
            start: 24_000,
            end: 30_000,
            crossfade: 2400,
        });
    };
    let mut looped = song("ramp", crossfaded, &["C-4 -- --- --- ---"]);
    play(&mut looped, 1);
    let out = left(&render(looped, 12 * STEP));
    // The ramp jumps from the loop's end to its start, which a crossfade turns into a slope.
    let step = largest_step(&out[20_000..]);
    let jump = 6000.0 / LENGTH as f32 * through;
    assert!(step < jump / 2000.0, "{step} against {jump}");
    for frame in [40_000, 51_000] {
        assert!((out[frame] - out[frame + 6000]).abs() < 1e-4);
    }
}

/// An engine playing a version, and frames from it.
struct Playing {
    engine: Engine,
}

impl Playing {
    fn new(song: Song) -> Self {
        let mut engine = Engine::new(RATE, Arc::new(Published::new(Arc::new(song), pool())));
        engine.play_from(0);
        Self { engine }
    }

    fn frames(&mut self, frames: usize) -> Vec<f32> {
        let mut out = vec![0.0; frames * 2];
        self.engine.process(&mut out);
        left(&out)
    }

    /// Change the song, as an edit does, keeping what the last version worked out.
    fn edit(&mut self, change: impl FnOnce(&mut Song)) {
        let last = Arc::clone(self.engine.current());
        let mut song = Song::clone(&last.song);
        change(&mut song);
        let next = Published::after(Arc::new(song), Arc::clone(&last.samples), Some(&last));
        self.engine.swap(Arc::new(next));
    }
}

#[test]
fn moving_a_point_while_it_plays_is_heard_at_once_without_a_click() {
    let through = through();
    let mut looped = song("ramp", points, &["C-4 -- --- --- ---"]);
    play(&mut looped, 1);
    let mut playing = Playing::new(looped);
    let mut out = playing.frames(40_000);
    // The loop moves earlier, under the note.
    playing.edit(|song| {
        let entry = song.bank.get(1).unwrap().clone();
        let mut moved = entry;
        moved.edit.points.loop_ = Some(Loop {
            start: 15_000,
            end: 18_000,
            crossfade: 0,
        });
        song.bank.set(1, Some(moved));
    });
    out.extend(playing.frames(20_000));
    let after = read_at(out[40_000 + 3000], through);
    assert!(
        (15_000.0 - 64.0..18_000.0).contains(&after),
        "at once: {after}"
    );
    assert!(
        (out[50_000] - out[53_000]).abs() < 1e-4,
        "round the new loop"
    );
    let jump = 12_000.0 / LENGTH as f32 * through;
    assert!(largest_step(&out) < jump / 32.0, "{}", largest_step(&out));

    // Its end moves before where a one-shot is, which fades out.
    let mut once = Playing::new(song("ramp", points, &["C-4 -- --- --- ---"]));
    let mut out = once.frames(10_000);
    once.edit(|song| {
        let mut moved = song.bank.get(1).unwrap().clone();
        moved.edit.points.end = Some(15_000);
        song.bank.set(1, Some(moved));
    });
    out.extend(once.frames(1000));
    assert_eq!(out[10_000 + DECLICK_FRAMES as usize + 1], 0.0, "faded out");
    let most = 22_000.0 / LENGTH as f32 * through / DECLICK_FRAMES as f32 * 1.01;
    assert!(largest_step(&out) <= most, "{}", largest_step(&out));
}

#[test]
fn a_note_from_the_keyboard_plays_the_span_as_its_track_cuts_it() {
    let through = through();
    let mut cut = song("ramp", points, &[]);
    cut.tracks[0].set_setting(SettingId::SAMPLE_START, 0x80);
    let mut engine = Engine::new(RATE, Arc::new(Published::new(Arc::new(cut), pool())));
    engine.hold_preview(0, 60, 0x7F);
    let mut out = vec![0.0; 2000 * 2];
    engine.process(&mut out);
    let at = read_at(out[1000 * 2], through);
    let half = 12_000.0 + 24_000.0 * 128.0 / 255.0;
    assert!((at - (half + 1000.0)).abs() < 2.0, "{at}");
}

#[test]
fn only_a_changed_entry_is_worked_out_again() {
    let mut song = song("ramp", points, &[]);
    song.bank
        .set(2, Some(BankEntry::of_file("tone", ContentHash::UNKNOWN)));
    let samples = pool();
    let first = Published::new(Arc::new(song.clone()), Arc::clone(&samples));
    let same = Published::after(Arc::new(song.clone()), Arc::clone(&samples), Some(&first));
    assert!(Arc::ptr_eq(&first.bank, &same.bank), "nothing changed");
    song.bank.get(1).unwrap();
    let mut moved = song.bank.get(1).unwrap().clone();
    moved.edit.points.start = 100;
    song.bank.set(1, Some(moved));
    let next = Published::after(Arc::new(song), Arc::clone(&samples), Some(&first));
    let (old, new) = (&first.bank, &next.bank);
    assert!(Arc::ptr_eq(
        old[2].as_ref().unwrap(),
        new[2].as_ref().unwrap()
    ));
    assert!(!Arc::ptr_eq(
        old[1].as_ref().unwrap(),
        new[1].as_ref().unwrap()
    ));
    assert_eq!(new[1].as_ref().unwrap().span(0, false).start, 100);
}

#[test]
fn points_at_a_files_own_rate_play_the_same_frames_at_the_programs() {
    let through = through();
    // The ramp as a 44.1 kHz file would be once converted: a frame of the file every 48/44.1 of
    // the program's.
    let mut pool = SamplePool::default();
    let ramp = (0..LENGTH)
        .map(|at| at as f32 / LENGTH as f32)
        .map(|value| [value, value])
        .collect();
    let rates = Rates {
        file: 44_100,
        loaded: RATE,
    };
    pool.insert_converted("ramp", SampleData { frames: ramp }, rates);
    pool.insert(
        "steady",
        SampleData {
            frames: vec![[0.5, 0.5]; LENGTH],
        },
    );
    let at_file_rate = |entry: &mut BankEntry| {
        entry.edit.points.start = 11_025;
        entry.edit.points.end = Some(22_050);
    };
    let version = Published::new(
        Arc::new(song("ramp", at_file_rate, &["C-4 -- --- --- ---"])),
        Arc::new(pool),
    );
    let out = left(&render_frames(Arc::new(version), RATE, 4 * STEP));
    // A quarter and a half of a second, at either rate.
    let at = read_at(out[1000], through);
    assert!((at - 13_000.0).abs() < 2.0, "{at}");
    assert!(out[11_990] > 0.0);
    assert_eq!(out[12_001], 0.0);
}

#[test]
fn golden_spans_stay_the_same() {
    let rows = [
        "C-4 -- --- --- ---",
        "--- -- --- --- ---",
        "C-4 -- sampler.start:40 --- ---",
        "--- -- --- --- ---",
        "G-4 -- sampler.length:60 --- ---",
        "--- -- --- --- ---",
        "C-3 -- sampler.start:20 sampler.length:C0 ---",
    ];
    let frames = 16 * STEP;
    for (name, mode, reverse) in [
        ("span-one-shot", 0, false),
        ("span-loop", 1, false),
        ("span-ping-pong", 2, false),
        ("span-reverse", 1, true),
    ] {
        let edit = |entry: &mut BankEntry| {
            points(entry);
            entry.edit.points.loop_ = Some(Loop {
                start: 24_000,
                end: 30_000,
                crossfade: 480,
            });
            entry.edit.play.reverse = reverse;
        };
        let mut song = song("tone", edit, &rows);
        play(&mut song, mode);
        golden(name, &render(song, frames));
    }
}
