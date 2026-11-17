//! A Steps mod slot (#235): its values in turn, from each note, from the song's position or one a
//! note, at a division, a tick or a pass of the chain, each way its `Loop` says, as #235's uses
//! ask: a flourish on each note, a stepped burst, a stepped LFO, a sequence a note at a time, and
//! a shape that holds after its attack.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::settings::SettingId;
use vt_model::song::{Phrase, Song};
use vt_model::steps::{Loop, StepRate, Steps, StepsMode};

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;

fn flat() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    pool.insert(
        "s",
        SampleData {
            frames: vec![[1.0, 1.0]; RATE as usize * 40],
        },
    );
    Arc::new(pool)
}

/// A track held at `CUT 80` playing these steps, with a Steps slot in `Mod 3` on `Cut`.
fn song(texts: &[&str], steps: Steps) -> Song {
    let mut song = Song::new(1);
    for track in 0..song.sound_tracks() {
        song.sample_on(track, "s");
    }
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    // An empty text is a step left empty.
    for (index, text) in texts
        .iter()
        .enumerate()
        .filter(|(_, text)| !text.is_empty())
    {
        *phrase.step_mut(index).unwrap() = text.parse().unwrap();
    }
    song.tracks[0].cutoff = 0x80;
    song.tracks[0].mods[2] = ModSlot::steps(Some(SettingId::CUT), steps);
    song
}

fn values(written: &[i16]) -> [i16; 16] {
    let mut values = [0; 16];
    values[..written.len()].copy_from_slice(written);
    values
}

/// The cutoff and the step playing at the end of each of some stretches of frames from the top.
fn heard(song: &Song, every: usize, times: usize) -> Vec<(u8, Option<u8>)> {
    let version = Arc::new(Published::new(Arc::new(song.clone()), flat()));
    let mut engine = Engine::new(RATE, version);
    engine.play_from(0);
    let mut out = vec![0.0; every * 2];
    (0..times)
        .map(|_| {
            engine.process(&mut out);
            let live = engine.live(0);
            (live.cutoff, live.steps[2])
        })
        .collect()
}

fn cutoffs(song: &Song, every: usize, times: usize) -> Vec<u8> {
    heard(song, every, times)
        .into_iter()
        .map(|(cut, _)| cut)
        .collect()
}

#[test]
fn each_note_plays_a_flourish_from_its_start_and_holds_the_last_once() {
    // +10, +07 then 00 at a 32nd, half a step each, played once from each note.
    let flourish = Steps {
        values: values(&[0x10, 0x07, 0]),
        length: 3,
        rate: StepRate::Note(120),
        looping: Loop::Once,
        ..Steps::default()
    };
    let song = song(
        &["C-3 -- --- --- ---", "", "", "", "C-3 -- --- --- ---"],
        flourish,
    );
    let heard = heard(&song, STEP / 2, 12);
    let cut: Vec<u8> = heard.iter().map(|(cut, _)| *cut).collect();
    assert_eq!(cut[..4], [0x90, 0x87, 0x80, 0x80]);
    // The second note, at step 4, starts it again.
    assert_eq!(cut[8..11], [0x90, 0x87, 0x80]);
    assert_eq!(heard[0].1, Some(0));
    assert_eq!(heard[5].1, Some(2), "held on the last");
}

#[test]
fn before_a_note_trig_waits_and_note_mode_moves_on_one_step_a_note() {
    // Each note on the track gets the next of four cutoffs (#235's test).
    let each_note = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mode: StepsMode::Note,
        ..Steps::default()
    };
    let notes = ["C-3 -- --- --- ---"; 6];
    let cut = cutoffs(&song(&notes, each_note), STEP, 6);
    assert_eq!(cut, [0x90, 0xA0, 0xB0, 0xC0, 0x90, 0xA0]);
    // With no note yet, a `Trig` slot moves nothing.
    let waits = Steps {
        values: [0x40; 16],
        ..Steps::default()
    };
    let cut = cutoffs(&song(&["", "", "C-3 -- --- --- ---"], waits), STEP, 3);
    assert_eq!(cut, [0x80, 0x80, 0xC0]);
}

#[test]
fn free_steps_follow_the_song_and_a_tick_is_a_sixth_of_the_tracks_step() {
    // A stepped LFO at a 16th, from the song's position whatever the notes do.
    let stepped = Steps {
        values: values(&[0x10, -0x10]),
        length: 2,
        mode: StepsMode::Free,
        ..Steps::default()
    };
    let cut = cutoffs(&song(&["", "", "C-3 -- --- --- ---"], stepped), STEP, 5);
    assert_eq!(cut, [0x90, 0x70, 0x90, 0x70, 0x90]);
    // At `Tick`, six steps of the values go by in each of the track's steps.
    let burst = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]),
        length: 6,
        rate: StepRate::Tick,
        looping: Loop::Once,
        ..Steps::default()
    };
    let heard = heard(&song(&["C-3 -- --- --- ---"], burst), STEP / 6, 7);
    let steps: Vec<Option<u8>> = heard.iter().map(|(_, step)| *step).collect();
    assert_eq!(
        steps,
        [
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(4),
            Some(5),
            Some(5)
        ]
    );
}

#[test]
fn a_loop_from_a_step_holds_a_shape_after_its_attack_and_bounce_turns_round() {
    // An attack of two steps, then the last two round and round: the M8's `HOP`.
    let attack = Steps {
        values: values(&[0x40, 0x30, 0x10, 0x20]),
        length: 4,
        looping: Loop::From(2),
        ..Steps::default()
    };
    let cut = cutoffs(&song(&["C-3 -- --- --- ---"], attack), STEP, 8);
    assert_eq!(cut, [0xC0, 0xB0, 0x90, 0xA0, 0x90, 0xA0, 0x90, 0xA0]);
    let bounce = Steps {
        looping: Loop::Bounce,
        ..attack
    };
    let cut = cutoffs(&song(&["C-3 -- --- --- ---"], bounce), STEP, 7);
    assert_eq!(cut, [0xC0, 0xB0, 0x90, 0xA0, 0x90, 0xB0, 0xC0]);
}

#[test]
fn a_shuffle_plays_each_step_once_a_time_round_and_a_render_repeats() {
    let shuffled = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        looping: Loop::Shuffle,
        ..Steps::default()
    };
    let song = song(&["C-3 -- --- --- ---"], shuffled);
    let cut = cutoffs(&song, STEP, 12);
    for lap in cut.chunks(4) {
        let mut sorted = lap.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, [0x90, 0xA0, 0xB0, 0xC0], "{cut:02X?}");
    }
    assert_ne!(cut[..4], cut[4..8]);
    assert_eq!(cut, cutoffs(&song, STEP, 12));
}

#[test]
fn a_pass_rate_moves_on_once_each_time_round_the_chain() {
    // Four steps a phrase, so each pass of the chain is four steps: `00 10 20 30` builds.
    let building = Steps {
        values: values(&[0, 0x10, 0x20, 0x30]),
        length: 4,
        rate: StepRate::Pass,
        mode: StepsMode::Free,
        ..Steps::default()
    };
    let mut song = song(&[], building);
    let mut four = Phrase::new(4);
    *four.step_mut(0).unwrap() = "C-3 -- --- --- ---".parse().unwrap();
    song.tracks[0].phrases.insert(0, Arc::new(four));
    let cut = cutoffs(&song, STEP * 4, 5);
    assert_eq!(cut, [0x80, 0x90, 0xA0, 0xB0, 0x80]);
}

#[test]
fn a_steps_start_is_its_phase_which_a_step_locks() {
    let run = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        start: 2,
        ..Steps::default()
    };
    let cut = cutoffs(&song(&["C-3 -- --- --- ---"], run), STEP, 3);
    assert_eq!(cut, [0xB0, 0xC0, 0x90]);
    // `PHS3 40` is a quarter of the way along four steps, the second.
    let locked = song(&["C-3 -- mod3.phase:40 --- ---"], Steps { start: 0, ..run });
    let cut = cutoffs(&locked, STEP, 2);
    assert_eq!(cut, [0xA0, 0xB0]);
}

#[test]
fn a_depth_scales_the_values_and_a_negative_one_turns_them_over() {
    let run = Steps {
        values: [0x40; 16],
        ..Steps::default()
    };
    let mut half = song(&["C-3 -- --- --- ---"], run);
    half.tracks[0].mods[2].depth = -0x80;
    assert_eq!(cutoffs(&half, STEP, 1), [0x60]);
}

#[test]
fn mutate_draws_values_again_from_the_seed_and_none_from_the_top_plays_them_as_written() {
    let written = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        ..Steps::default()
    };
    let laps = |mutate| {
        let song = song(&["C-3 -- --- --- ---"], Steps { mutate, ..written });
        cutoffs(&song, STEP, 16)
    };
    let as_written = [0x90, 0xA0, 0xB0, 0xC0];
    assert_eq!(laps(0), as_written.repeat(4));
    // At 100% every value is drawn again each time round, and a render draws the same.
    let all = laps(100);
    assert_ne!(all[..4], all[4..8]);
    assert_eq!(all, laps(100), "a render repeats");
    // Between, some are as written and some drawn.
    let half = laps(50);
    assert!(
        half.iter().any(|cut| as_written.contains(cut)),
        "{half:02X?}"
    );
    assert!(
        half.iter().any(|cut| !as_written.contains(cut)),
        "{half:02X?}"
    );
}

/// Turning `Mutate` down to 0% locks the loop as it was last drawn, as the Turing Machine's knob
/// fully right does; a step written again is heard as written at once; and a play from the top
/// forgets the drift and draws it again the same from the seed (decision 384).
#[test]
fn mutate_at_none_locks_the_loop_an_edit_wins_and_the_top_forgets() {
    let written = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mutate: 100,
        ..Steps::default()
    };
    let drifting = song(&["C-3 -- --- --- ---"], written);
    let version = |song: &Song| Arc::new(Published::new(Arc::new(song.clone()), flat()));
    let mut engine = Engine::new(RATE, version(&drifting));
    let mut out = vec![0.0; STEP * 2];
    let mut steps = |engine: &mut Engine, count: usize| -> Vec<u8> {
        (0..count)
            .map(|_| {
                engine.process(&mut out);
                engine.live(0).cutoff
            })
            .collect()
    };
    engine.play_from(0);
    let first = steps(&mut engine, 8);
    // At 100% each lap is drawn afresh; the last one drawn is what's locked.
    assert_ne!(first[..4], first[4..]);
    let mut locked = drifting.clone();
    locked.tracks[0].mods[2].steps.mutate = 0;
    engine.swap(version(&locked));
    let held = steps(&mut engine, 8);
    assert_eq!(held[..4], first[4..], "the lap drawn last plays on");
    assert_eq!(held[4..], first[4..], "and again");
    let drawn = engine.live(0).drawn[2];
    assert_eq!(drawn.which, 0b1111, "kept and reported at 0%: {drawn:?}");
    // The second step written again is heard as written at once, and the rest stay as drawn.
    let mut edited = locked.clone();
    edited.tracks[0].mods[2].steps.values[1] = 0x70;
    engine.swap(version(&edited));
    let after = steps(&mut engine, 4);
    assert_eq!(after[1], 0xF0, "{after:02X?}");
    assert_eq!([after[0], after[2], after[3]], [held[4], held[6], held[7]]);
    assert_eq!(engine.live(0).drawn[2].which, 0b1101);
    // From the top, nothing is drawn: at 0% it plays as written, and back at 100% the same drift
    // is drawn again from the seed.
    engine.play_from(0);
    assert_eq!(steps(&mut engine, 4), [0x90, 0xF0, 0xB0, 0xC0]);
    engine.swap(version(&drifting));
    engine.play_from(0);
    assert_eq!(steps(&mut engine, 8), first, "a play from the top repeats");
}

#[test]
fn smooth_glides_into_each_step_or_plucks_it() {
    // `+40` then `-40` at a 16th from the song's position, heard a quarter and three quarters of
    // the way through the second step.
    let two = Steps {
        values: values(&[0x40, -0x40]),
        length: 2,
        mode: StepsMode::Free,
        ..Steps::default()
    };
    let through = |smooth| {
        let cut = cutoffs(&song(&[], Steps { smooth, ..two }), STEP / 4, 8);
        (i16::from(cut[4]), i16::from(cut[6]))
    };
    let close = |(a, b): (i16, i16), (x, y): (i16, i16)| {
        assert!((a - x).abs() <= 3 && (b - y).abs() <= 3, "{a:02X} {b:02X}");
    };
    // Stepped, it's the step's value throughout.
    close(through(0), (0x40, 0x40));
    // Halfway, a straight glide from the step before across the whole step.
    close(through(0x80), (0xA0, 0x60));
    // Down to `-80`, a pluck: the step's value falling to nothing across it.
    close(through(-0x80), (0x50, 0x70));
}

/// What the Steps slot in `Mod 4` moved the first row's note by on `Pitch`, in semitones, with
/// the scale set by command before playing if one is given, as `:scale` does.
fn pitch_moved(song: &Song, live: Option<Scale>) -> f32 {
    let version = Arc::new(Published::new(Arc::new(song.clone()), flat()));
    let mut engine = Engine::new(RATE, version);
    if let Some(scale) = live {
        engine.set_scale(scale);
    }
    engine.play_from(0);
    let mut out = vec![0.0; STEP];
    engine.process(&mut out);
    let pitch = engine
        .take_note_pitch(0)
        .expect("the row's note is reported");
    pitch.moves[3]
}

/// A slot's `Pitch in scale` follows the scale as it is now, which `:scale` sets by command
/// without making a new version of the song, not the one the song was opened with (#882).
#[test]
fn pitch_in_scale_follows_a_scale_changed_by_command() {
    // Up three semitones from `C-3`, to `D#3`: between C major's `D` and `E`, so the lower.
    let up_three = Steps {
        values: values(&[3]),
        length: 1,
        mode: StepsMode::Note,
        ..Steps::default()
    };
    let major = Scale {
        on: true,
        root: 0,
        kind: ScaleKind::Major,
    };
    let off = Scale { on: false, ..major };
    let song_in = |scale| {
        let mut song = song(&["C-3 64 --- --- ---"], Steps::default());
        song.saved_scale = scale;
        song.tracks[0].mods[3] = ModSlot {
            in_scale: true,
            ..ModSlot::steps(Some(SettingId::PITCH), up_three)
        };
        song
    };
    // As the song has it, with nothing sent.
    assert_eq!(pitch_moved(&song_in(off), None), 3.0);
    assert_eq!(pitch_moved(&song_in(major), None), 2.0);
    // Turned on by command, and turned off by command, over what the song has.
    assert_eq!(pitch_moved(&song_in(off), Some(major)), 2.0);
    assert_eq!(pitch_moved(&song_in(major), Some(off)), 3.0);
}
