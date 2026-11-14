//! `VDLT`: how far each note's velocity is moved up or down from its own as it starts, from a
//! step, a lane, a glide, a mod slot or the track's `Velocity`.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::modulation::ModSlot;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::{Lane, Machine, Phrase, Song, SynthVoice};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;

/// A sample that holds one level, so what's heard is the note's gain alone.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    let frames = vec![[0.5, 0.5]; RATE as usize * 8];
    pool.insert("flat", SampleData { frames });
    Arc::new(pool)
}

/// One sample track playing these rows, with notes that start and stop at once.
fn song(rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.sample_on(0, "flat");
    song.tracks[0].mods[0].adsr.attack_ms = 0;
    song.tracks[0].mods[0].adsr.release_ms = 0;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(Arc::new(song.clone()), pool()));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// How loud the note at this step is, halfway through it.
fn level(out: &[f32], step: usize) -> f32 {
    out[step * STEP + STEP / 2].abs()
}

/// Whether two levels are within one step of velocity of each other.
fn near(heard: f32, expected: f32) -> bool {
    (heard - expected).abs() <= expected.max(heard) / 100.0 + 1e-6
}

#[test]
fn eighty_changes_nothing() {
    let plain = ["C-3 40", "C-3 --", "C-3 7F"];
    let set = [
        "C-3 40 velocitydelta:80",
        "C-3 -- velocitydelta:80",
        "C-3 7F velocitydelta:80",
    ];
    assert_eq!(render(&song(&plain), 3), render(&song(&set), 3));
}

#[test]
fn a_step_moves_its_note_by_as_many_steps_of_velocity_and_keeps_to_the_range() {
    let moved = [
        "C-3 40 velocitydelta:90",
        "C-3 40 velocitydelta:70",
        "C-3 70 velocitydelta:FF",
        "C-3 10 velocitydelta:00",
        "C-3 40",
    ];
    let written = ["C-3 50", "C-3 30", "C-3 7F", "C-3 00", "C-3 40"];
    assert_eq!(
        render(&song(&moved), 5),
        render(&song(&written), 5),
        "and a lock ends with its note"
    );
}

#[test]
fn it_is_read_once_as_the_note_starts() {
    let plain = ["C-3 40 SGL3", "--- --", "--- --"];
    let locked = ["C-3 40 SGL3", "--- -- velocitydelta:FF", "--- --"];
    assert_eq!(render(&song(&plain), 3), render(&song(&locked), 3));
}

#[test]
fn a_glide_across_notes_gives_each_its_own_share() {
    // Text gives a row only the FX slots it writes, and a glide looks along its own slot, so the
    // rows between write an empty one.
    let glided = [
        "C-3 40 velocitydelta:40g",
        "C-3 40 ---",
        "C-3 40 ---",
        "C-3 40 ---",
        "C-3 40 velocitydelta:C0",
    ];
    let out = render(&song(&glided), 5);
    let written = render(
        &song(&["C-3 00", "C-3 20", "C-3 40", "C-3 60", "C-3 7F"]),
        5,
    );
    for step in 1..5 {
        assert!(
            near(level(&out, step), level(&written, step)),
            "step {step}: {} against {}",
            level(&out, step),
            level(&written, step)
        );
    }
    assert!(level(&out, 1) < level(&out, 2) && level(&out, 2) < level(&out, 3));
}

#[test]
fn a_lane_of_it_is_an_accent_pattern_that_drifts_against_the_notes() {
    let rows = [
        "C-3 40 velocitydelta:A0",
        "C-3 40 velocitydelta:80",
        "C-3 40 velocitydelta:80",
        "C-3 40",
    ];
    let mut song = song(&rows);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    phrase.set_lane(
        0,
        Some(Lane {
            start: 0,
            length: 3,
        }),
    );
    let out = render(&song, 12);
    // The 4-row phrase's notes meet the 3-row lane's accent on every third step.
    let loud = level(&out, 0);
    let soft = level(&out, 1);
    assert!(loud > soft * 1.4, "{loud} against {soft}");
    for step in 0..12 {
        let expected = if step % 3 == 0 { loud } else { soft };
        assert!(near(level(&out, step), expected), "step {step}");
    }
}

#[test]
fn a_mod_slot_on_it_moves_velocity() {
    let rows = ["C-3 40", "C-3 40", "C-3 40", "C-3 40"];
    let mut moved = song(&rows);
    moved.tracks[0].mods[3] = ModSlot::lfo(
        Some(SettingId::VELOCITY_DELTA),
        Lfo {
            shape: Shape::Square,
            rate: Rate::Steps(4),
            ..Lfo::default()
        },
    )
    .with_depth(0x20);
    let out = render(&moved, 4);
    let plain = render(&song(&rows), 4);
    assert!(
        level(&out, 1) > level(&plain, 1) * 1.2,
        "{} against {}",
        level(&out, 1),
        level(&plain, 1)
    );
    assert!(
        level(&out, 3) < level(&plain, 3) * 0.8,
        "{} against {}",
        level(&out, 3),
        level(&plain, 3)
    );
}

#[test]
fn the_tracks_velocity_is_where_it_starts_and_a_lock_replaces_it() {
    let mut based = song(&["C-3 40", "C-3 40 velocitydelta:80", "C-3 40"]);
    based.tracks[0].velocity_delta = 0x90;
    let written = song(&["C-3 50", "C-3 40", "C-3 50"]);
    assert_eq!(render(&based, 3), render(&written, 3));
}

#[test]
fn on_acid_it_only_moves_a_note_across_the_accent() {
    let acid = |rows: &[&str]| {
        let mut song = song(rows);
        song.tracks[0].machine = Machine::Plaits(SynthVoice::Acid);
        song
    };
    let crossed = render(&acid(&["C-1 64 velocitydelta:8C", "--- --"]), 2);
    assert_eq!(crossed, render(&acid(&["C-1 70", "--- --"]), 2));
    assert_ne!(crossed, render(&acid(&["C-1 64", "--- --"]), 2));
    assert_eq!(
        render(&acid(&["C-1 64 velocitydelta:20", "--- --"]), 2),
        render(&acid(&["C-1 10", "--- --"]), 2),
        "below the accent every note sounds the same"
    );
}
