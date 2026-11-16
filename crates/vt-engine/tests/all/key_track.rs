//! The track filter's `Key track`: on, the cutoff follows the note an octave for an octave, with
//! `C-3` leaving it as set, so a resonant filter's peak keeps its place against the note's
//! harmonics up a ramp of notes rather than the low notes sitting in it (decision 381). Off, the
//! default, is the filter as it was.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::samples::SamplePool;
use vt_model::settings::glide_value;
use vt_model::song::{FULL_GATE, FilterMode, Machine, Phrase, Song, SynthVoice};

use crate::support::rms;

const RATE: u32 = 48_000;
/// Frames in a step at 120 BPM.
const STEP: usize = 6000;

/// One `CSaw` track through a resonant 12 dB low-pass, as the song that asked for it has, playing
/// these rows, with `Key track` on or off.
fn song(rows: &[&str], key_track: bool) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    let track = &mut song.tracks[0];
    track.machine = Machine::Plaits(SynthVoice::CSaw);
    track.cutoff = 94;
    track.resonance = 214;
    track.filter_mode = FilterMode::Slope12;
    track.gate_length = Some(FULL_GATE);
    track.key_track = key_track;
    let steps = rows.iter().map(|text| text.parse().unwrap()).collect();
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    song
}

/// The left channel, for this many steps.
fn render(song: &Song, steps: usize) -> Vec<f32> {
    let version = Arc::new(Published::new(
        Arc::new(song.clone()),
        Arc::new(SamplePool::default()),
    ));
    render_frames(version, RATE, steps * STEP)
        .chunks(2)
        .map(|frame| frame[0])
        .collect()
}

/// How loud a step is in dB, over its second half, once a new note's change has settled.
fn step_db(out: &[f32], step: usize) -> f32 {
    20.0 * rms(&out[step * STEP + STEP / 2..(step + 1) * STEP]).log10()
}

/// A ramp of notes from `C-3` up an octave, as the song has.
const RAMP: [&str; 6] = ["C-3 64", "D#3 64", "F-3 64", "G-3 64", "A#3 64", "C-4 64"];

#[test]
fn off_is_the_filter_as_it_was() {
    // The default, so no song moves: its goldens don't either.
    let mut unset = song(&RAMP, true);
    unset.tracks[0].key_track = vt_model::song::Track::default().key_track;
    assert_eq!(render(&unset, 6), render(&song(&RAMP, false), 6));
}

#[test]
fn c3_leaves_the_cutoff_as_set() {
    // The note it's set at is `C-3`, where on is off, sample for sample.
    let rows = ["C-3 64", "--- --", "C-3 40", "OFF --"];
    assert_eq!(
        render(&song(&rows, true), 4),
        render(&song(&rows, false), 4)
    );
}

#[test]
fn a_ramp_keeps_its_level_through_a_resonant_filter() {
    let off = render(&song(&RAMP, false), 6);
    let on = render(&song(&RAMP, true), 6);
    let levels = |out: &[f32]| (0..6).map(|step| step_db(out, step)).collect::<Vec<_>>();
    let (off, on) = (levels(&off), levels(&on));
    println!("off {off:.1?}\non  {on:.1?}");
    // Off, `C-3` sits in the peak and the octave above it far under it.
    assert!(off[0] - off[5] > 15.0, "{off:?}");
    // On, the peak moves with the note, and every note is within a dB of `C-3`'s.
    for (step, db) in on.iter().enumerate() {
        assert!((db - on[0]).abs() < 1.0, "step {step}: {on:?}");
    }
}

#[test]
fn a_step_can_lock_it() {
    // A lock on `C-4`'s row turns it on for that row, as on for the whole track does.
    let locked = render(&song(&["C-3 64", "C-4 64 keytrack:01"], false), 2);
    let on = render(&song(&["C-3 64", "C-4 64"], true), 2);
    let off = render(&song(&["C-3 64", "C-4 64"], false), 2);
    assert!((step_db(&locked, 1) - step_db(&on, 1)).abs() < 0.1);
    assert!(step_db(&locked, 1) - step_db(&off, 1) > 10.0);
    // And a lock to off on a track with it on leaves the cutoff where it's set.
    let unlocked = render(&song(&["C-3 64", "C-4 64 keytrack:00"], true), 2);
    assert!((step_db(&unlocked, 1) - step_db(&off, 1)).abs() < 0.1);
}

#[test]
fn a_glide_moves_the_cutoff_with_it() {
    // A tie gliding up an octave over half a second keeps its level within 2 dB all the way,
    // where the cutoff left behind loses it as the note leaves the peak.
    let rows = [
        "C-3 64 HLD1",
        "C-4 64",
        "--- --",
        "--- --",
        "--- --",
        "--- --",
    ];
    let level = |key_track| {
        let mut gliding = song(&rows, key_track);
        gliding.tracks[0].glide = glide_value(500.0);
        let out = render(&gliding, 6);
        // In tenths of a step, from the tie until well after the glide has landed.
        (10..60)
            .map(|tenth| {
                let at = tenth * STEP / 10;
                20.0 * rms(&out[at..at + STEP / 10]).log10()
            })
            .collect::<Vec<f32>>()
    };
    let (off, on) = (level(false), level(true));
    let spread = |levels: &[f32]| {
        let (low, high) = levels.iter().fold((f32::MAX, f32::MIN), |(low, high), db| {
            (low.min(*db), high.max(*db))
        });
        high - low
    };
    println!(
        "off spread {:.1} dB, on spread {:.1} dB",
        spread(&off),
        spread(&on)
    );
    assert!(spread(&off) > 15.0, "{off:?}");
    assert!(spread(&on) < 2.0, "{on:?}");
}

/// The largest second difference over these frames: what a click adds most of, as a step in
/// level or slope is a spike in it where a resonant tone's own is steady.
fn sharpest(out: &[f32], from: usize, to: usize) -> f32 {
    (from.max(2)..to)
        .map(|at| (out[at] - 2.0 * out[at - 1] + out[at - 2]).abs())
        .fold(0.0, f32::max)
}

#[test]
fn a_note_change_on_high_resonance_moves_the_cutoff_as_a_lock_of_cut_would() {
    // At the top of `Res`, a note an octave up or down moves the cutoff an octave, smoothed as a
    // change of `CUT` is: the 10 ms after the change are no sharper than with `CUT` locked by hand
    // to the same octave on the row, which is how the engine moves a cutoff without a click.
    // `78` and `44` are `5E`, 94, an octave up and down, near enough.
    let cases = [
        (["C-3 64", "C-4 64"], ["C-3 64", "C-4 64 cut:78"]),
        (["C-4 64", "C-3 64"], ["C-4 64 cut:78", "C-3 64"]),
        (["C-3 64", "C-2 64"], ["C-3 64", "C-2 64 cut:44"]),
        (["C-2 64", "C-3 64"], ["C-2 64 cut:44", "C-3 64"]),
    ];
    for (following, by_hand) in cases {
        let resonant = |rows: &[&str], key_track| {
            let mut song = song(rows, key_track);
            song.tracks[0].resonance = 0xFF;
            let out = render(&song, 2);
            let change = sharpest(&out, STEP, STEP + RATE as usize / 100);
            let settled = sharpest(&out, STEP + STEP / 2, 2 * STEP);
            (change, settled)
        };
        let (following, settled) = resonant(&following, true);
        let (by_hand, _) = resonant(&by_hand, false);
        println!("{following:.4} following, {by_hand:.4} by hand, {settled:.4} settled");
        assert!(
            following <= by_hand * 1.25,
            "{following} following against {by_hand} by hand"
        );
    }
}
