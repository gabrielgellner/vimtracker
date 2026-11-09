//! A sampler plays the bank entry its `Sample` is on as each note starts: the track's own, or a
//! step's `SMP` lock, as Elektron's sample locks do, with each entry's own gain and tune.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::bank::{BankEntry, ContentHash, PlayParams};
use vt_model::lfo::{Lfo, Rate, Shape};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// Long enough to outlast any note here.
const LENGTH: usize = RATE as usize * 4;

/// Files at a steady level each, and a ramp from silence up, by name.
fn pool() -> Arc<SamplePool> {
    let mut pool = SamplePool::default();
    for (name, level) in [("quiet", 0.25), ("loud", 0.5), ("louder", 0.75)] {
        let frames = vec![[level, level]; LENGTH];
        pool.insert(name, SampleData { frames });
    }
    let ramp = (0..LENGTH)
        .map(|at| at as f32 / LENGTH as f32)
        .map(|value| [value, value])
        .collect();
    pool.insert("ramp", SampleData { frames: ramp });
    Arc::new(pool)
}

/// The bank: `01` quiet, `02` loud, `03` louder and `04` the ramp, with nothing at `05`.
fn bank(song: &mut Song) {
    for (number, name) in [(1, "quiet"), (2, "loud"), (3, "louder"), (4, "ramp")] {
        song.bank
            .set(number, Some(BankEntry::of_file(name, ContentHash::UNKNOWN)));
    }
}

/// One track on bank entry `sample`, playing these rows, with notes that start and stop at once.
fn song(sample: u8, rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    bank(&mut song);
    let track = &mut song.tracks[0];
    track.set_setting(SettingId::SAMPLE, sample);
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
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

/// The left channel's level partway into a step, once a new note's fade-in has passed.
fn on_step(out: &[f32], step: usize) -> f32 {
    out[(step * STEP + STEP / 2) * 2].abs()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < expected.abs() * 0.01 + 1e-6,
        "{actual} and {expected}"
    );
}

#[test]
fn a_step_locks_another_sample_for_its_note_and_the_next_goes_back() {
    let out = render(
        song(
            1,
            &[
                "C-4 -- --- --- ---",
                "C-4 -- sampler.sample:02 --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
        STEP * 3,
    );
    let quiet = on_step(&out, 0);
    assert!(quiet > 0.0);
    close(on_step(&out, 1), quiet * 2.0);
    close(on_step(&out, 2), quiet);
}

#[test]
fn a_held_sample_lock_stays_for_the_notes_after_it() {
    let out = render(
        song(
            1,
            &[
                "C-4 -- --- --- ---",
                "C-4 -- sampler.sample:03h --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
        STEP * 3,
    );
    let quiet = on_step(&out, 0);
    close(on_step(&out, 1), quiet * 3.0);
    close(on_step(&out, 2), quiet * 3.0);
}

#[test]
fn a_lock_without_a_note_leaves_the_sounding_note_and_ends_with_the_next() {
    // As any lock does: it lasts until the next note, which brings back the track's own, and a
    // note keeps the sample it started with.
    let out = render(
        song(
            1,
            &[
                "C-4 -- --- --- ---",
                "--- -- sampler.sample:02 --- ---",
                "C-4 -- --- --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
        STEP * 4,
    );
    let quiet = on_step(&out, 0);
    close(on_step(&out, 1), quiet);
    close(on_step(&out, 2), quiet);
    let held = render(
        song(
            1,
            &[
                "C-4 -- --- --- ---",
                "--- -- sampler.sample:02h --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
        STEP * 3,
    );
    close(on_step(&held, 2), quiet * 2.0);
}

#[test]
fn off_and_a_number_with_nothing_in_it_are_silent() {
    let out = render(
        song(
            2,
            &[
                "C-4 -- sampler.sample:00 --- ---",
                "C-4 -- sampler.sample:05 --- ---",
                "C-4 -- --- --- ---",
            ],
        ),
        STEP * 3,
    );
    assert_eq!(on_step(&out, 0), 0.0);
    assert_eq!(on_step(&out, 1), 0.0);
    assert!(on_step(&out, 2) > 0.0);
    let new_track = render(song(0, &["C-4 -- --- --- ---"]), STEP);
    assert!(
        new_track.iter().all(|value| *value == 0.0),
        "a new track, at 00, plays nothing"
    );
}

#[test]
fn two_tracks_play_one_entry_and_each_locks_its_own() {
    let mut song = song(1, &["C-4 -- sampler.sample:02 --- ---"]);
    song.add_track();
    song.tracks[1].set_setting(SettingId::SAMPLE, 1);
    let phrase = Arc::make_mut(song.tracks[1].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-4 -- --- --- ---".parse().unwrap();
    // Panned apart, so each channel is one track's.
    song.tracks[0].pan = 0x00;
    song.tracks[1].pan = 0xFF;
    song.tracks[1].envelopes[0].attack_ms = 0;
    let out = render(song, STEP);
    let (left, right) = (out[STEP], out[STEP + 1]);
    close(left, right * 2.0);
}

#[test]
fn an_entrys_gain_and_tune_are_its_own_on_any_track() {
    let plain = render(song(4, &["C-4 -- --- --- ---"]), STEP);
    let mut tuned = song(4, &["C-4 -- --- --- ---"]);
    let ramp = BankEntry {
        edit: vt_model::bank::SampleEdit {
            play: PlayParams {
                gain_db: -6.0,
                tune: 12,
            },
            ..tuned.bank.get(4).unwrap().edit.clone()
        },
        ..tuned.bank.get(4).unwrap().clone()
    };
    tuned.bank.set(4, Some(ramp));
    let tuned = render(tuned, STEP);
    // An octave up reads the ramp twice as fast, and -6 dB is about half as loud.
    let at = STEP / 2 * 2;
    let ratio = tuned[at] / plain[at];
    assert!((ratio - 2.0 * 0.501).abs() < 0.01, "{ratio}");
}

#[test]
fn an_lfo_on_sample_moves_which_entry_a_note_starts_with() {
    let mut moved = song(2, &["--- -- --- --- ---", "C-4 -- --- --- ---"]);
    moved.tracks[0].lfos[0] = Lfo {
        destination: Some(SettingId::SAMPLE),
        shape: Shape::Square,
        rate: Rate::Note(960 * 8),
        depth: 0x01,
        ..Lfo::default()
    };
    let out = render(moved, STEP * 2);
    let heard = on_step(&out, 1);
    let quiet = render(song(1, &["C-4 -- --- --- ---"]), STEP);
    let quiet = on_step(&quiet, 0);
    assert!(
        (heard - quiet).abs() < 1e-3 || (heard - quiet * 3.0).abs() < 1e-3,
        "a whole entry either way of 02: {heard}, against {quiet} for 01"
    );
}
