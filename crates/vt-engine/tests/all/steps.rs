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

/// The steps a slot plays, a step at a time from the top: what the Mod page shows playing.
fn played(song: &Song, times: usize) -> Vec<u8> {
    heard(song, STEP, times)
        .into_iter()
        .map(|(_, step)| step.unwrap())
        .collect()
}

/// `Jump` at 0% walks as `Loop` says, as before there was one; at 100% each step is any of its
/// `Length`, in every mode, and a render jumps the same (decision 390).
#[test]
fn jump_at_none_walks_the_loop_and_at_full_plays_any_step() {
    let bouncing = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]),
        length: 6,
        looping: Loop::Bounce,
        mode: StepsMode::Free,
        ..Steps::default()
    };
    let walked = played(&song(&[], bouncing), 12);
    assert_eq!(walked, [0, 1, 2, 3, 4, 5, 4, 3, 2, 1, 0, 1]);
    for mode in StepsMode::ALL {
        let notes = ["C-3 -- --- --- ---"; 16];
        let jumping = Steps {
            jump: 100,
            mode,
            ..bouncing
        };
        let song = song(&notes, jumping);
        let jumped = played(&song, 16);
        assert!(jumped.iter().all(|step| *step < 6), "{mode:?} {jumped:?}");
        let mut seen = jumped.clone();
        seen.sort_unstable();
        seen.dedup();
        assert!(
            seen.len() >= 4,
            "{mode:?} visits steps at random: {jumped:?}"
        );
        assert!(
            jumped.windows(2).any(|pair| pair[1].abs_diff(pair[0]) > 1),
            "{mode:?} jumps rather than walks: {jumped:?}"
        );
        assert_eq!(jumped, played(&song, 16), "{mode:?}: a render repeats");
        // What's heard is the value of the step played.
        let cut = cutoffs(&song, STEP, 16);
        for (cut, step) in cut.iter().zip(&jumped) {
            assert_eq!(*cut, 0x90 + 0x10 * step, "{mode:?}");
        }
    }
    // Between, some steps walk on and some jump.
    let walked = played(&song(&[], bouncing), 24);
    let half = played(
        &song(
            &[],
            Steps {
                jump: 50,
                ..bouncing
            },
        ),
        24,
    );
    assert_ne!(half, walked);
    let walks = half.iter().zip(&walked).filter(|(a, b)| a == b).count();
    assert!(walks > 6, "{half:?}");
}

/// With `Mutate` locked at 0% after drifting, a jump replays the values it drew, as Marbles past
/// noon plays its locked loop's values out of order (decision 390).
#[test]
fn a_jump_replays_the_values_mutate_locked() {
    let written = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mode: StepsMode::Free,
        mutate: 100,
        ..Steps::default()
    };
    let drifting = song(&[], written);
    let version = |song: &Song| Arc::new(Published::new(Arc::new(song.clone()), flat()));
    let mut engine = Engine::new(RATE, version(&drifting));
    let mut out = vec![0.0; STEP * 2];
    let mut steps = |engine: &mut Engine, count: usize| -> Vec<(u8, u8)> {
        (0..count)
            .map(|_| {
                engine.process(&mut out);
                let live = engine.live(0);
                (live.cutoff, live.steps[2].unwrap())
            })
            .collect()
    };
    engine.play_from(0);
    let lap = steps(&mut engine, 4);
    let mut drawn = [0; 4];
    for (cut, step) in &lap {
        drawn[usize::from(*step)] = *cut;
    }
    let mut locked = drifting;
    locked.tracks[0].mods[2].steps.mutate = 0;
    locked.tracks[0].mods[2].steps.jump = 100;
    engine.swap(version(&locked));
    let jumped = steps(&mut engine, 16);
    for (cut, step) in &jumped {
        assert_eq!(
            *cut,
            drawn[usize::from(*step)],
            "{jumped:02X?} {drawn:02X?}"
        );
    }
}

/// A macro holding a Steps slot's `Jump` turns it, as it does `Mutate`.
#[test]
fn a_macro_turns_jump() {
    use vt_model::macros::{MacroMap, MacroTarget, Zone};
    use vt_model::modulation::ModField;
    let walking = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x08]),
        mode: StepsMode::Free,
        ..Steps::default()
    };
    let mut song = song(&[], walking);
    let jump = SettingId::mod_own(
        2,
        ModField::Jump
            .own(vt_model::modulation::ModKind::Steps)
            .unwrap(),
    )
    .unwrap();
    song.tracks[0].macros[0].maps = vec![MacroMap {
        target: MacroTarget::Setting(jump),
        min: 0,
        max: 0x6400,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    }];
    let walk = [0, 1, 2, 3, 4, 5, 6, 7].repeat(2);
    assert_eq!(played(&song, 16), walk);
    song.tracks[0].macros[0].value = 0xFF00;
    let jumped = played(&song, 16);
    assert_ne!(jumped, walk, "{jumped:?}");
    assert_eq!(jumped, played(&song, 16), "a render repeats");
}

/// In `Trig`, each note starts the slot again, so a note that ends before its first step does gets
/// `Mutate`'s chance at that step each time, as a note that runs on gets it at each step it
/// reaches: at 100% every note draws. `Free` counts on from the song's position whatever the notes
/// do, drawing each step once as it comes round, and `Note` a step a note (decision 397).
#[test]
fn mutate_in_trig_draws_again_for_each_note_that_restarts_the_slot() {
    let every_row = ["C-3 -- --- --- ---"; 8];
    // A step of the slot's lasts two rows, so each note is shorter than a step.
    let run = |mode| {
        let slow = Steps {
            values: values(&[0x10, 0x20]),
            length: 2,
            rate: StepRate::Note(480),
            mutate: 100,
            mode,
            ..Steps::default()
        };
        // A small depth, so what's drawn doesn't pin the cutoff at either end.
        let mut song = song(&every_row, slow);
        song.tracks[0].mods[2].depth = 0x20;
        cutoffs(&song, STEP, 8)
    };
    let trig = run(StepsMode::Trig);
    let mut distinct = trig.clone();
    distinct.dedup();
    assert!(distinct.len() > 4, "each note draws: {trig:02X?}");
    assert_eq!(trig, run(StepsMode::Trig), "a render repeats");
    // `Free` draws once a step, two rows each.
    let free = run(StepsMode::Free);
    for pair in free.chunks(2) {
        assert_eq!(pair[0], pair[1], "{free:02X?}");
    }
    assert_eq!(free, run(StepsMode::Free), "a render repeats");
    // `Note` moves a step and draws at each note.
    let note = run(StepsMode::Note);
    let mut moved = note.clone();
    moved.dedup();
    assert!(moved.len() > 4, "{note:02X?}");
    assert_eq!(note, run(StepsMode::Note), "a render repeats");
}

/// `Mutate`'s draws over four steps written `+10` to `+40`, sixteen steps from the top at 100%,
/// spread and leaned by `spread` and `bias`.
fn mutated_laps(spread: u8, bias: i8) -> Vec<u8> {
    let steps = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mutate: 100,
        spread,
        bias,
        ..Steps::default()
    };
    cutoffs(&song(&["C-3 -- --- --- ---"], steps), STEP, 16)
}

/// At an even `Spread` and no `Bias`, `Mutate` draws what it always drew, evenly over `-FF` to
/// `+FF`, the same numbers as before there were either (decision 403).
#[test]
fn mutate_at_an_even_spread_and_no_bias_draws_as_it_always_did() {
    assert_eq!(
        mutated_laps(50, 0),
        [
            0xFF, 0x13, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x04, 0x12, 0x02, 0x6A, 0xFF, 0xC5, 0xF5,
            0xB1, 0xFF
        ],
    );
}

/// A narrow `Spread` draws round the middle, a wide one out at the ends, and `Bias` leans them,
/// each render the same (decision 403).
#[test]
fn spread_and_bias_shape_what_mutate_draws_and_a_render_repeats() {
    let narrow = mutated_laps(10, 0);
    assert!(
        narrow.iter().all(|cut| cut.abs_diff(0x80) < 0x40),
        "{narrow:02X?}"
    );
    assert_eq!(mutated_laps(0, 0), [0x80; 16], "the bias point alone");
    let ends = mutated_laps(100, 0);
    assert!(
        ends.iter().all(|cut| *cut == 0x00 || *cut == 0xFF),
        "{ends:02X?}"
    );
    assert_eq!(mutated_laps(100, 100), [0xFF; 16], "the high end alone");
    // Leaned high, they sit above the middle, and a narrow bell sits on the bias point.
    let high = mutated_laps(50, 80);
    let mean = high.iter().map(|cut| f64::from(*cut)).sum::<f64>() / 16.0;
    assert!(mean > 0xB0 as f64, "{high:02X?}");
    let leaned = mutated_laps(5, -25);
    assert!(
        leaned.iter().all(|cut| cut.abs_diff(0x40) < 0x20),
        "a quarter of the way down: {leaned:02X?}"
    );
    for (spread, bias) in [(10, 0), (35, -20), (80, 40)] {
        assert_eq!(
            mutated_laps(spread, bias),
            mutated_laps(spread, bias),
            "a render repeats"
        );
    }
}

/// A loop drifted at a narrow `Spread`, then locked with `Mutate` at none, stays as narrow as
/// it was drawn, lap after lap.
#[test]
fn a_loop_locked_after_drifting_narrow_stays_narrow() {
    let narrow = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mutate: 100,
        spread: 10,
        ..Steps::default()
    };
    let drifting = song(&["C-3 -- --- --- ---"], narrow);
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
    let mut locked = drifting;
    locked.tracks[0].mods[2].steps.mutate = 0;
    engine.swap(version(&locked));
    let held = steps(&mut engine, 16);
    assert!(
        held.iter().all(|cut| cut.abs_diff(0x80) < 0x40),
        "{held:02X?}"
    );
    assert!(
        held.iter()
            .all(|cut| ![0x90, 0xA0, 0xB0, 0xC0].contains(cut)),
        "drawn, not as written: {held:02X?}"
    );
    for lap in held.chunks(4) {
        assert_eq!(lap, &first[4..], "the lap drawn last plays on");
    }
}

/// Macros hold a Steps slot's `Spread` and `Bias`, as they do `Mutate`.
#[test]
fn macros_turn_spread_and_bias() {
    use vt_model::macros::{MacroMap, MacroTarget, Zone};
    use vt_model::modulation::{ModField, ModKind};
    let steps = Steps {
        values: values(&[0x10, 0x20, 0x30, 0x40]),
        length: 4,
        mutate: 100,
        ..Steps::default()
    };
    let mut song = song(&["C-3 -- --- --- ---"], steps);
    let map = |field: ModField, min: u16, max: u16| MacroMap {
        target: MacroTarget::Setting(
            SettingId::mod_own(2, field.own(ModKind::Steps).unwrap()).unwrap(),
        ),
        min,
        max,
        curve: 0,
        s: 0,
        zone: Zone::default(),
        member: None,
    };
    // The first macro takes `Spread` from 0% to 100%, the second `Bias` from none to +100.
    song.tracks[0].macros[0].maps = vec![map(ModField::Spread, 0, 0x6400)];
    song.tracks[0].macros[1].maps = vec![map(ModField::Bias, 0x8000, 0xFF00)];
    let laps = |song: &Song| cutoffs(song, STEP, 16);
    assert_eq!(laps(&song), [0x80; 16], "Spread 0%: the middle alone");
    song.tracks[0].macros[0].value = 0xFF00;
    let ends = laps(&song);
    assert!(
        ends.iter().all(|cut| *cut == 0x00 || *cut == 0xFF),
        "{ends:02X?}"
    );
    assert_eq!(ends, laps(&song), "a render repeats");
    song.tracks[0].macros[1].value = 0xFF00;
    assert_eq!(laps(&song), [0xFF; 16], "Bias +100: the high end alone");
}
