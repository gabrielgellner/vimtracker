//! A sampler plays a slice of its entry, chosen by its `SLI`: from the slice's start to the next
//! one's, clamped to the last, or on through to the entry's end with `PLY` on `Thru`, and with
//! the slice's own gain, pan, tune and reverse where it has them, and the entry's where it hasn't.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::action::{Action, edit};
use vt_model::bank::{BankEntry, ContentHash, MOST_PAN, SliceParams};
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::settings::SettingId;
use vt_model::song::Song;

const RATE: u32 = 48_000;
/// 6000 frames per step at 120 BPM.
const STEP: usize = 6000;
/// The ramp's length: a second, in eight slices of a step each.
const LENGTH: usize = RATE as usize;

/// A ramp from silence up, so a frame's level says where it was read, and a steady level.
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
    Arc::new(pool)
}

/// One track playing a file's entry, `01`, in eight slices, with an edit to them, through these
/// rows, with notes that start and stop at once.
fn song(file: &str, edit: impl FnOnce(&mut BankEntry), rows: &[&str]) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let mut entry = BankEntry::of_file(file, ContentHash::UNKNOWN);
    let starts = entry.edit.points.even_starts(8, LENGTH as u64);
    entry.edit.points.reslice(&starts);
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

fn render(song: Song, frames: usize) -> Vec<f32> {
    render_frames(
        Arc::new(Published::new(Arc::new(song), pool())),
        RATE,
        frames,
    )
}

/// The left and the right channels.
fn channels(out: &[f32]) -> (Vec<f32>, Vec<f32>) {
    (
        out.iter().step_by(2).copied().collect(),
        out.iter().skip(1).step_by(2).copied().collect(),
    )
}

/// What the track does to a level: a steady sample's level through it, over the steady 0.5.
fn through() -> f32 {
    let (left, _) = channels(&render(
        song("steady", |_| {}, &["C-3 -- --- --- ---"]),
        1000,
    ));
    left[500] / 0.5
}

/// Where in the ramp a frame was read, from its level.
fn read_at(level: f32, through: f32) -> f32 {
    level / through * LENGTH as f32
}

fn slice_row(slice: usize) -> String {
    format!("C-3 -- sampler.slice:{slice:02X} --- ---")
}

#[test]
fn sli_00_to_07_down_a_phrase_plays_the_loop_back_in_order() {
    let through = through();
    let rows: Vec<String> = (0..8).map(slice_row).collect();
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let (left, _) = channels(&render(song("ramp", |_| {}, &rows), 8 * STEP));
    for step in 0..8 {
        let at = read_at(left[step * STEP + 1000], through);
        let wanted = (step * STEP + 1000) as f32;
        assert!((at - wanted).abs() < 2.0, "step {step}: {at}");
    }
}

#[test]
fn a_slice_ends_at_the_next_ones_start_and_past_the_last_plays_the_last() {
    let through = through();
    let rest = "--- -- --- --- ---";
    let rows = [slice_row(2), rest.to_owned(), slice_row(0x40)];
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let (left, _) = channels(&render(song("ramp", |_| {}, &rows), 4 * STEP));
    assert!((read_at(left[1000], through) - 13_000.0).abs() < 2.0);
    // Slice 02 is a step long, so it has ended by the middle of the next step.
    assert_eq!(left[STEP + 3000], 0.0, "it ended at slice 03's start");
    let last = read_at(left[2 * STEP + 1000], through);
    assert!((last - 43_000.0).abs() < 2.0, "{last}");
}

#[test]
fn thru_plays_from_the_slice_on_to_the_entrys_end() {
    let through = through();
    let mut song = song("ramp", |_| {}, &[&slice_row(5)]);
    song.tracks[0].set_setting(SettingId::PLAY, 3);
    let (left, _) = channels(&render(song, 3 * STEP + 1000));
    assert!((read_at(left[1000], through) - 31_000.0).abs() < 2.0);
    // Past slice 05's end, on into 06 and 07.
    let later = read_at(left[2 * STEP + 1000], through);
    assert!((later - 43_000.0).abs() < 2.0, "{later}");
    assert_eq!(left[3 * STEP + 100], 0.0, "and ends with the entry");
}

#[test]
fn a_slice_plays_with_its_own_gain_pan_tune_and_reverse_and_the_others_as_the_entry() {
    let through = through();
    let own = |entry: &mut BankEntry| {
        entry.edit.play.gain_db = -6.0;
        entry.edit.points.slices[1].over = SliceParams {
            gain_db: Some(0.0),
            pan: Some(-MOST_PAN),
            tune: Some(12),
            ..SliceParams::default()
        };
        entry.edit.points.slices[3].over.reverse = Some(true);
    };
    let rest = "--- -- --- --- ---";
    let rows = [
        slice_row(1),
        rest.to_owned(),
        slice_row(2),
        rest.to_owned(),
        slice_row(3),
    ];
    let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
    let (left, right) = channels(&render(song("ramp", own, &rows), 5 * STEP));
    // Slice 01 at 0 dB, all to the left, an octave up: two frames a frame, from 6000.
    let at = read_at(left[1000], through) / std::f32::consts::SQRT_2;
    assert!((at - 8000.0).abs() < 4.0, "{at}");
    assert!(right[1000].abs() < 1e-6);
    // An octave up, it's over in half a step.
    assert_eq!(left[3500], 0.0);
    // Slice 02 as the entry: -6 dB, in the middle, at its pitch.
    let gain = 10_f32.powf(-6.0 / 20.0);
    let at = read_at(left[2 * STEP + 1000], through) / gain;
    assert!((at - 13_000.0).abs() < 4.0, "{at}");
    assert_eq!(left[2 * STEP + 1000], right[2 * STEP + 1000]);
    // Slice 03 reversed, from its end down.
    let at = read_at(left[4 * STEP + 1000], through) / gain;
    assert!((at - (24_000.0 - 1001.0)).abs() < 4.0, "{at}");
}

#[test]
fn an_entry_with_no_slices_ignores_sli() {
    let through = through();
    let whole = |entry: &mut BankEntry| {
        entry.edit.points.reslice(&[]);
    };
    let (left, _) = channels(&render(song("ramp", whole, &[&slice_row(3)]), STEP));
    assert!((read_at(left[1000], through) - 1000.0).abs() < 2.0);
}

#[test]
fn sli_by_a_letter_keeps_playing_its_chop_as_slices_are_added_before_it() {
    let through = through();
    // `a` names slice 05, from 30 000.
    let named = |entry: &mut BankEntry| {
        assert!(entry.edit.points.set_mark('a', 5));
    };
    let first = song("ramp", named, &["C-3 -- sampler.slice:'a --- ---"]);
    let (left, _) = channels(&render(first.clone(), STEP));
    assert!((read_at(left[1000], through) - 31_000.0).abs() < 2.0);
    // Two splits before it make it slice 07, and the step plays the same sound unchanged.
    let mut next = Arc::new(first);
    for at in [1000, 9000] {
        next = edit(&next, &Action::SplitSlice { number: 1, at }).unwrap();
    }
    assert_eq!(next.bank.get(1).unwrap().edit.points.marked('a'), Some(7));
    let step = &next.tracks[0].phrases[&0].steps()[0];
    assert_eq!(
        step.fx[0].slice_mark(),
        Some('a'),
        "the step isn't rewritten"
    );
    let (left, _) = channels(&render((*next).clone(), STEP));
    assert!((read_at(left[1000], through) - 31_000.0).abs() < 2.0);
}

#[test]
fn sli_by_a_letter_naming_no_slice_plays_the_tracks_own() {
    let through = through();
    let rest = "--- -- --- --- ---";
    let none = "C-3 -- sampler.slice:'z --- ---";
    let rows = [none, rest, "C-3 -- sampler.slice:03h --- ---", rest, none];
    let mut song = song("ramp", |_| {}, &rows);
    song.tracks[0].set_setting(SettingId::SLICE, 2);
    let (left, _) = channels(&render(song, 5 * STEP));
    // The track's `Slice`, 02, from 12 000.
    assert!((read_at(left[1000], through) - 13_000.0).abs() < 2.0);
    // Once 03 is held, it's the track's own under the lock.
    let held = read_at(left[4 * STEP + 1000], through);
    assert!((held - 19_000.0).abs() < 2.0, "{held}");
}
