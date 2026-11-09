//! Choke groups, and a track's own pitch.

use std::sync::Arc;

use vt_engine::published::Published;
use vt_engine::render::render_frames;
use vt_model::sample::SampleData;
use vt_model::samples::SamplePool;
use vt_model::song::Song;
use vt_model::step::{Note, Step};

const STEP: usize = 6000;

fn flat(frames: usize, level: f32) -> SampleData {
    SampleData {
        frames: vec![[level, level]; frames],
    }
}

/// Two tracks in these choke groups: the first plays a long open sound from step 0, the second
/// a quieter one from step 2.
fn hats(open: Option<u8>, closed: Option<u8>) -> Vec<f32> {
    let mut song = Song::new(2);
    song.sample_on(0, "open");
    song.sample_on(1, "closed");
    for (track, choke) in song.tracks.iter_mut().zip([open, closed]) {
        track.choke = choke;
        track.envelopes[0].attack_ms = 0;
        track.envelopes[0].release_ms = 0;
    }
    for (track, step) in [(0, 0), (1, 2)] {
        let phrase = Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap());
        *phrase.step_mut(step).unwrap() = Step {
            note: Note::On(60),
            ..Step::default()
        };
    }
    let mut pool = SamplePool::default();
    pool.insert("open", flat(4 * STEP, 0.25));
    pool.insert("closed", flat(4 * STEP, 0.125));
    let version = Arc::new(Published::new(Arc::new(song), Arc::new(pool)));
    render_frames(version, 48_000, 4 * STEP)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|frame| frame[0])
        .collect()
}

#[test]
fn a_note_cuts_off_the_others_in_its_choke_group_without_a_click() {
    let left = hats(Some(1), Some(1));
    assert!(
        (left[STEP] - 0.25).abs() < 1e-3,
        "the open hat, {}",
        left[STEP]
    );
    let after = left[3 * STEP];
    assert!((after - 0.125).abs() < 1e-3, "only the closed hat, {after}");
    // Less the closed hat, which starts at once, the open hat fades over several frames.
    let open: Vec<f32> = (2 * STEP - 10..2 * STEP + 200)
        .map(|frame| left[frame] - if frame >= 2 * STEP { 0.125 } else { 0.0 })
        .collect();
    assert!(open[0] > 0.24 && open[open.len() - 1].abs() < 1e-3);
    let biggest = open
        .windows(2)
        .map(|w| (w[1] - w[0]).abs())
        .fold(0.0, f32::max);
    assert!(biggest < 0.05, "it fades, but jumped {biggest}");
}

#[test]
fn tracks_in_no_group_or_another_play_on() {
    for (open, closed) in [
        (None, None),
        (Some(1), None),
        (None, Some(1)),
        (Some(1), Some(2)),
    ] {
        let left = hats(open, closed);
        let after = left[3 * STEP];
        assert!(
            (after - 0.375).abs() < 1e-3,
            "{open:?} {closed:?}: both, {after}"
        );
    }
}

#[test]
fn a_tracks_own_pitch_moves_its_notes() {
    let mut song = Song::new(1);
    song.sample_on(0, "flat");
    let track = &mut song.tracks[0];
    track.envelopes[0].attack_ms = 0;
    track.envelopes[0].release_ms = 0;
    track.pitch = 0x8C;
    let phrase = Arc::make_mut(track.phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = Step {
        note: Note::On(60),
        ..Step::default()
    };
    let mut pool = SamplePool::default();
    pool.insert("flat", flat(1000, 1.0));
    let version = Arc::new(Published::new(Arc::new(song), Arc::new(pool)));
    let out = render_frames(version, 48_000, 2000);
    // An octave up plays 1000 frames in about 500.
    let last = out
        .as_chunks::<2>()
        .0
        .iter()
        .rposition(|frame| frame[0] != 0.0)
        .unwrap();
    assert!((495..=510).contains(&last), "ended at {last}");
}
