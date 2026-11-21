//! A section's own scale (decision 426): while a section with one plays, everything that reads the
//! scale reads its own, from its first step, a live generator's notes rendered through it in the
//! engine; a section without one plays in the song's, as before.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::scale::{Scale, ScaleKind};
use vt_model::selection::StepRange;
use vt_model::shape::{Shape, ShapeColumn};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;

use crate::support::ramp::{RAMP_FRAMES, render, version};

const RATE: u32 = 48_000;
/// Frames per step at 120 BPM.
const STEP: usize = 6000;
/// `C-3`, which plays the ramp at its own speed.
const C3: i32 = 60;
/// Rows in the phrase, and steps in each section.
const ROWS: usize = 16;

const C_MAJOR: Scale = Scale {
    on: true,
    root: 0,
    kind: ScaleKind::Major,
};
/// C#, D#, F, G# and A#: only F is also in C major.
const C_SHARP_PENTATONIC: Scale = Scale {
    on: true,
    root: 1,
    kind: ScaleKind::MajorPentatonic,
};

/// One track in C major playing these sixteen rows in two sections, the second in a scale of its
/// own if `second` is given.
fn song(steps: Vec<Step>, second: Option<Scale>) -> Song {
    let mut song = Song::new(1);
    song.limiter = false;
    song.saved_scale = C_MAJOR;
    song.sample_on(0, "ramp");
    let track = &mut song.tracks[0];
    track.mods[0].adsr.attack_ms = 0;
    track.mods[0].adsr.release_ms = 0;
    track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    let mut next = song.sections[0].clone();
    next.scale = second;
    song.sections.push(next);
    song
}

/// Every row a note whose `RND` moves it by up to seven degrees either way.
fn varied(second: Option<Scale>) -> Song {
    song(vec!["C-3 -- rnd:F7".parse().unwrap(); ROWS], second)
}

/// Notes at full velocity with a live generator moving them from `C-3` up to `C-4` in the song's
/// C major, as the edit thread leaves them.
fn generated(second: Option<Scale>) -> Song {
    let song = song(vec!["C-3 --".parse().unwrap(); ROWS], second);
    let column = ShapeColumn::Note {
        scale: C_MAJOR,
        writes: None,
    };
    let generate = Action::Generate {
        track: 0,
        phrase: 0,
        column,
        span: Span::rows(StepRange::between(0, ROWS - 1)),
        shape: Shape {
            from: 60,
            to: 72,
            seed: 7,
            ..Shape::default()
        },
        list: Box::default(),
        rhythm: Box::default(),
    };
    Arc::unwrap_or_clone(edit(&Arc::new(song), &generate).expect("the edit applies"))
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

/// The note each step starts, for both sections.
fn notes(song: &Song) -> Vec<i32> {
    notes_for(song, 2 * ROWS)
}

fn notes_for(song: &Song, steps: usize) -> Vec<i32> {
    let out = render(song, steps);
    (0..steps)
        .map(|step| note_at(&out, step * STEP).expect("every step plays a note"))
        .collect()
}

fn all_in(notes: &[i32], scale: Scale) -> bool {
    notes.iter().all(|note| scale.contains(*note as u8))
}

#[test]
fn a_generators_notes_change_key_at_the_sections_start() {
    let heard = notes(&generated(Some(C_SHARP_PENTATONIC)));
    let (first, second) = heard.split_at(ROWS);
    assert!(all_in(first, C_MAJOR), "{first:?}");
    assert!(all_in(second, C_SHARP_PENTATONIC), "{second:?}");
    assert!(!all_in(second, C_MAJOR), "{second:?}");
    // The first section is as the generator wrote it.
    let written = notes(&generated(None));
    assert_eq!(first, &written[..ROWS]);
    assert_eq!(&written[ROWS..], &written[..ROWS], "both as written");
}

#[test]
fn rnd_quantises_to_the_scale_of_the_section_playing() {
    let heard = notes(&varied(Some(C_SHARP_PENTATONIC)));
    let (first, second) = heard.split_at(ROWS);
    assert!(all_in(first, C_MAJOR), "{first:?}");
    assert!(all_in(second, C_SHARP_PENTATONIC), "{second:?}");
    assert!(!all_in(second, C_MAJOR), "{second:?}");
}

#[test]
fn an_acu_moves_through_the_scale_of_the_section_playing() {
    let mut rows: Vec<Step> = vec![Step::default(); ROWS];
    rows[0] = "C-3 -- acu:01".parse().unwrap();
    let mut song = song(rows, Some(C_SHARP_PENTATONIC));
    // Every row of the phrase is the accumulating one.
    for row in 1..ROWS {
        let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
        *phrase.step_mut(row).unwrap() = "C-3 -- acu:01".parse().unwrap();
    }
    let heard = notes(&song);
    let (first, second) = heard.split_at(ROWS);
    assert!(all_in(&first[1..], C_MAJOR), "{first:?}");
    assert!(all_in(second, C_SHARP_PENTATONIC), "{second:?}");
}

/// Unset, a section plays in the song's scale exactly as before, and so does one set to the
/// song's own.
#[test]
fn a_section_without_a_scale_plays_in_the_songs() {
    for song_of in [varied, generated] {
        let unset = song_of(None);
        let plain = render(&unset, 2 * ROWS);
        assert_eq!(render(&song_of(Some(C_MAJOR)), 2 * ROWS), plain);
        let mut one = unset;
        one.sections.truncate(1);
        let once = render(&one, ROWS);
        assert_eq!(&plain[..ROWS * STEP], &once[..]);
    }
}

#[test]
fn a_render_in_a_sections_scale_repeats() {
    for song in [
        varied(Some(C_SHARP_PENTATONIC)),
        generated(Some(C_SHARP_PENTATONIC)),
    ] {
        assert_eq!(render(&song, 2 * ROWS), render(&song, 2 * ROWS));
    }
    // Round the song again, the generator back in C major and then in the second's scale.
    let twice = notes_for(&generated(Some(C_SHARP_PENTATONIC)), 4 * ROWS);
    assert_eq!(twice[..2 * ROWS], twice[2 * ROWS..]);
}

#[test]
fn playing_from_a_section_starts_in_its_scale() {
    let song = generated(Some(C_SHARP_PENTATONIC));
    let mut engine = Engine::new(RATE, version(&song));
    assert_eq!(
        engine.scale_now(),
        C_MAJOR,
        "the first section's, the song's"
    );
    engine.play_section(1, 0);
    assert_eq!(engine.scale_now(), C_SHARP_PENTATONIC);
    let mut out = vec![0.0; ROWS * STEP * 2];
    engine.process(&mut out);
    let left: Vec<f32> = out.chunks(2).map(|frame| frame[0]).collect();
    let heard: Vec<i32> = (0..ROWS - 1)
        .filter_map(|step| note_at(&left, step * STEP))
        .collect();
    assert_eq!(heard.len(), ROWS - 1);
    assert!(all_in(&heard, C_SHARP_PENTATONIC), "{heard:?}");
}

/// The song's scale changed while a section with its own plays leaves the section's in force,
/// and is heard again once the song moves back to a section without one.
#[test]
fn the_songs_scale_set_while_a_section_has_its_own_waits_for_the_next() {
    let song = varied(Some(C_SHARP_PENTATONIC));
    let mut engine = Engine::new(RATE, version(&song));
    engine.play_section(1, 0);
    let minor = Scale {
        kind: ScaleKind::Minor,
        ..C_MAJOR
    };
    engine.set_scale(minor);
    assert_eq!(engine.scale_now(), C_SHARP_PENTATONIC);
    // Into the first step of the first section again.
    let mut out = vec![0.0; (ROWS * STEP + STEP / 2) * 2];
    engine.process(&mut out);
    assert_eq!(engine.scale_now(), minor, "back in the first section");
}
