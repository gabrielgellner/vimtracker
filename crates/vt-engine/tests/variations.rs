//! Controlled chance, after the Eloquencer (decision 323): an `RND xy` gives what's to its left a
//! chance `x` in `F` of varying by up to `y` each time its row plays: the note, by scale degrees
//! or semitones, a lock's value, or whether a `RET` ratchets. Each roll is a hash of the seed, the
//! track, its count and the column, so a render repeats and playing from a step hears what playing
//! up to it would.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::accumulator::Accumulation;
use vt_model::arpeggio::{ArpRate, Arpeggio};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::song::{Phrase, Song};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp the sample plays, so a note's pitch is how fast it climbs.
const RAMP_FRAMES: usize = RATE as usize * 8;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;

fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = (0..RAMP_FRAMES)
        .map(|at| {
            let level = at as f32 / RAMP_FRAMES as f32;
            [level, level]
        })
        .collect();
    pool.insert("ramp", SampleData { frames });
    Arc::new(pool)
}

/// One track playing these rows, in C major with the scale on, with notes that start and stop at
/// once.
fn song(rows: &[&str], scale_on: bool) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.seed = 0x600;
    song.scale = Scale {
        on: scale_on,
        root: 0,
        kind: ScaleKind::Major,
    };
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

fn version(song: &Song) -> Arc<Published> {
    Arc::new(Published::new(Arc::new(song.clone()), pool()))
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    render_frames(version(song), RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// Below this a little into a step, a note started there.
const STARTED: f32 = 0.005;

/// The note that starts at a frame, by how fast the ramp climbs, or none if none starts there.
fn note_at(out: &[f32], frame: usize) -> Option<i32> {
    let climb = out[frame + 1600] - out[frame + 600];
    let own = 1000.0 / RAMP_FRAMES as f32;
    let started = out[frame + 500] < STARTED && climb > own * 0.1;
    started.then(|| C3 + (12.0 * (climb / own).log2()).round() as i32)
}

/// The note each step starts with, for this many steps.
fn notes(song: &Song, steps: usize) -> Vec<i32> {
    let out = render(song, steps);
    (0..steps)
        .map(|step| note_at(&out, step * STEP).expect("a note on every step"))
        .collect()
}

/// C major's notes, as MIDI numbers, from `C-3` four degrees either way.
const AROUND_C: [i32; 9] = [53, 55, 57, 59, 60, 62, 64, 65, 67];

#[test]
fn chance_0_never_varies_and_f_always_does() {
    let never = song(&["C-3 -- rnd:04"], true);
    assert!(notes(&never, 24).iter().all(|note| *note == C3));
    let always = song(&["C-3 -- rnd:F4"], true);
    assert!(notes(&always, 24).iter().all(|note| *note != C3));
}

#[test]
fn a_varied_note_stays_in_range_and_in_scale_and_reaches_each() {
    let heard = notes(&song(&["C-3 -- rnd:F4"], true), 64);
    assert!(
        heard.iter().all(|note| AROUND_C.contains(note)),
        "{heard:?}"
    );
    for note in AROUND_C.iter().filter(|note| **note != C3) {
        assert!(heard.contains(note), "{note} in {heard:?}");
    }
    // A note outside the scale lands on the next one in, as an `ACU`'s does.
    let sharp = notes(&song(&["C#3 -- rnd:F1"], true), 32);
    assert!(
        sharp.iter().all(|note| [60, 62].contains(note)),
        "{sharp:?}"
    );
}

#[test]
fn with_the_scale_off_it_moves_by_semitones() {
    let heard = notes(&song(&["C-3 -- rnd:F2"], false), 48);
    assert!(heard.iter().all(|note| [58, 59, 61, 62].contains(note)));
    assert!([58, 59, 61, 62].iter().all(|note| heard.contains(note)));
}

#[test]
fn about_half_vary_at_8_and_a_seed_repeats_while_another_differs() {
    let rows = ["C-3 -- rnd:82"];
    let first = notes(&song(&rows, true), 96);
    let moved = first.iter().filter(|note| **note != C3).count();
    assert!((36..=66).contains(&moved), "{moved} of 96");
    assert_eq!(notes(&song(&rows, true), 96), first, "a render repeats");
    let mut other = song(&rows, true);
    other.seed = 0x601;
    assert_ne!(notes(&other, 96), first, "another seed rolls otherwise");
}

#[test]
fn playing_from_a_step_hears_what_playing_up_to_it_would() {
    // A phrase of four rows four times in its chain, so each row varies on each of its passes.
    let rows = ["C-3 -- rnd:F3", "E-3 -- --- rnd:A2"].repeat(2);
    let mut song = song(&rows, true);
    let chain = Arc::make_mut(song.tracks[0].chains.get_mut(&0).unwrap());
    let entry = chain.entries[0];
    chain.entries = vec![entry; 4];
    let through = render(&song, 16);
    let passes: Vec<Option<i32>> = (0..4)
        .map(|pass| note_at(&through, 4 * pass * STEP))
        .collect();
    assert!(passes.windows(2).any(|two| two[0] != two[1]), "{passes:?}");
    for from in [3, 6, 9, 12] {
        let mut engine = Engine::new(RATE, version(&song));
        engine.play_from(from);
        let mut out = vec![0.0; 2 * STEP * 2];
        engine.process(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
        for step in 0..2 {
            assert_eq!(
                note_at(&left, step * STEP),
                note_at(&through, (from + step) * STEP),
                "from step {from}, its step {step}"
            );
        }
    }
}

#[test]
fn it_leaves_chas_rolls_as_they_were() {
    let plain = song(&["C-3 -- cha:80", "D-3 -- ---"], true);
    let varied = song(&["C-3 -- cha:80", "D-3 -- rnd:F2"], true);
    let played = |song: &Song| -> Vec<bool> {
        let out = render(song, 32);
        (0..16)
            .map(|at| note_at(&out, 2 * at * STEP).is_some())
            .collect()
    };
    let rolled = played(&plain);
    assert!(rolled.contains(&true) && rolled.contains(&false));
    assert_eq!(played(&varied), rolled);
}

#[test]
fn beside_a_lock_it_varies_the_lock_by_its_steps() {
    // `PTCH` is in semitones, so `RND F2` moves it 1 or 2 either way, and the note with it.
    let heard = notes(&song(&["C-3 -- pitch:80 rnd:F2"], true), 48);
    assert!(heard.iter().all(|note| [58, 59, 61, 62].contains(note)));
    assert!([58, 59, 61, 62].iter().all(|note| heard.contains(note)));
    // The note itself isn't moved: a lock's `RND` is the lock's alone.
    let held = notes(&song(&["C-3 -- pitch:80 rnd:02"], true), 8);
    assert!(held.iter().all(|note| *note == C3));
}

#[test]
fn beside_a_ret_it_is_the_chance_the_row_ratchets() {
    // `RET 03` hits halfway through each row.
    let ratchets = |fx: &str| -> Vec<bool> {
        let row = format!("C-3 -- ret:03 {fx}");
        let out = render(&song(&[&row], true), 33);
        (0..32)
            .map(|step| note_at(&out, step * STEP + STEP / 2).is_some())
            .collect()
    };
    assert!(ratchets("rnd:00").iter().all(|hit| !hit), "never");
    assert!(ratchets("rnd:F0").iter().all(|hit| *hit), "always");
    let some = ratchets("rnd:80");
    let count = some.iter().filter(|hit| **hit).count();
    assert!((10..=24).contains(&count), "{count} of 32");
    // With a range, its units vary: some hits come early.
    let row = "C-3 -- ret:04 rnd:F2";
    let out = render(&song(&[row], true), 33);
    let early = (0..32).any(|step| note_at(&out, step * STEP + 2 * STEP / 6).is_some());
    assert!(early, "a hit 2 units in, from 4 moved down 2");
}

#[test]
fn it_moves_the_note_from_where_an_accumulator_has_it_without_adding_to_the_walk() {
    // Up a semitone each time round, then 1 either way on top.
    let mut song = song(&["C-3 -- acu:01 rnd:F1"], false);
    song.tracks[0].accumulation = Accumulation {
        up: 28,
        ..Accumulation::default()
    };
    let heard = notes(&song, 16);
    for (pass, note) in heard.iter().enumerate() {
        let walked = C3 + pass as i32;
        assert!(
            (note - walked).abs() == 1,
            "pass {pass}: {note} round {walked}"
        );
    }
}

#[test]
fn an_arpeggio_goes_round_the_note_it_moved_to() {
    let mut song = song(&["C-3 -- rnd:F3 arp:40"], false);
    song.tracks[0].arpeggio = Arpeggio {
        rate: ArpRate::ThirtySecond,
        retrig: true,
        ..Arpeggio::default()
    };
    let out = render(&song, 17);
    for step in 0..16 {
        let root = note_at(&out, step * STEP).unwrap();
        assert!(root != C3 && (root - C3).abs() <= 3, "{root}");
        assert_eq!(note_at(&out, step * STEP + STEP / 2), Some(root + 4));
    }
}

#[test]
fn where_its_heard_and_the_report_say_the_note_it_moved_to() {
    let song = song(&["C-3 -- rnd:F4", "D-3 --"], true);
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_from(0);
    let mut out = vec![0.0; STEP / 2 * 2];
    engine.process(&mut out);
    let heard = engine.track_heard(0).accumulated.expect("it always moves");
    assert!(AROUND_C.contains(&i32::from(heard)) && heard != 60);
    let pitch = engine
        .take_note_pitch(0)
        .expect("the row's note is reported");
    assert_eq!((pitch.row.phrase, pitch.row.row), (0, 0));
    assert_eq!(i32::from(pitch.varied), i32::from(heard) - C3);
    // The next row has no `RND`, so it's as written and has no report.
    let mut out = vec![0.0; STEP * 2];
    engine.process(&mut out);
    assert_eq!(engine.track_heard(0).accumulated, None);
    assert!(engine.take_note_pitch(0).is_none());
}

#[test]
fn renders_with_variations_repeat() {
    let rows = [
        "C-3 64 rnd:84 cut:40 rnd:F3",
        "E-3 50 MLT2 rnd:A2",
        "G-3 -- ret:03 rnd:82 acu:01",
        "C-3 -- velocitydelta:80 rnd:F4 cnd:12",
    ];
    for scale_on in [false, true] {
        let song = song(&rows, scale_on);
        let render = || render_frames(version(&song), RATE, 24 * STEP);
        assert_eq!(render(), render(), "scale on: {scale_on}");
    }
}
