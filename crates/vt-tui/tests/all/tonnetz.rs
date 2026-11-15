//! The shape panel's `Tonnetz`, by keys (decision 332): on a `Chord` track, a chord a row, its
//! root and its chord locked, with the moves typed as letters; on another track, the roots; and
//! across three tracks, a voice of the triad each, one of them moving from chord to chord.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::{CHORD_SETTING, FillKind, GenParam, Moves, TonnetzMove, TonnetzVoice};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::{FxCommand, Note, Step};
use vt_tui::app::App;

use crate::common::{app_for, keys, shape_panel};

const E: &str = "--- -- --- --- ---";

/// A song of tracks of 16 empty rows, each playing this machine.
fn app_with(tracks: usize, machine: Machine) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in &mut song.tracks {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.machine = machine;
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    app_for(song)
}

fn steps(app: &App, track: usize) -> Vec<Step> {
    app.song().tracks[track].phrases[&0].steps().to_vec()
}

fn pitches(app: &App, track: usize) -> Vec<u8> {
    steps(app, track)
        .iter()
        .map(|step| match step.note {
            Note::On(note) => note,
            _ => panic!("a note on every row"),
        })
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    shape_panel(app, 60, 30)
}

/// Go down the panel to a setting.
fn to_row(app: &mut App, param: GenParam) {
    while app.shape.as_ref().unwrap().row != param {
        keys(app, "j");
    }
}

#[test]
fn on_the_chord_voice_each_row_is_a_root_with_its_chord_locked() {
    let (mut app, _rt) = app_with(1, Machine::Plaits(SynthVoice::Chord));
    keys(&mut app, "vGSK/tonnetz<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Tonnetz);
    assert_eq!(panel.shape.tonnetz.voice, Some(TonnetzVoice::Chord));
    {
        use GenParam::*;
        assert_eq!(
            panel.rows(),
            [Fill, From, Triad, Voice, Moves, MoveList, InScale, Where]
        );
    }
    // The hint names the walk's first chords.
    let names: Vec<String> = panel
        .shape
        .tonnetz
        .chords(panel.shape.from, panel.shape.seed, app.scale, 8)
        .into_iter()
        .map(|chord| chord.name())
        .collect();
    let screen = drawn(&app);
    for expected in ["Tonnetz", "Triad", "Major", "Chord", "P L R L", "Free"] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    assert!(screen.contains(&names.join(" ")), "{names:?} in\n{screen}");
    // The moves typed as letters, spaces or none.
    to_row(&mut app, GenParam::MoveList);
    keys(&mut app, "ilr<Enter>");
    let list = app.shape.as_ref().unwrap().shape.tonnetz.list;
    assert_eq!(list.as_slice(), [TonnetzMove::L, TonnetzMove::R]);
    keys(&mut app, "<Enter>");
    let written = steps(&app, 0);
    let lock = FxCommand::Set(CHORD_SETTING);
    for (row, step) in written.iter().enumerate() {
        assert!(matches!(step.note, Note::On(_)), "row {row}");
        let chord = step.fx.iter().find(|fx| fx.command == Some(lock));
        assert!(chord.is_some(), "a chord locked on row {row}");
    }
    // `L R` from a major chord: minor, major, minor, and on.
    let chords: Vec<u8> = written
        .iter()
        .map(|step| {
            step.fx
                .iter()
                .find(|fx| fx.command == Some(lock))
                .unwrap()
                .coarse()
        })
        .collect();
    assert_eq!(chords[..4], [0x0A, 0x03, 0x0A, 0x03]);
}

#[test]
fn on_another_voice_it_writes_the_roots_and_draws_its_moves_at_random() {
    let (mut app, _rt) = app_with(1, Machine::Plaits(SynthVoice::VirtualAnalog));
    keys(&mut app, "vGSK/tonnetz<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.tonnetz.voice, Some(TonnetzVoice::Root));
    to_row(&mut app, GenParam::Moves);
    keys(&mut app, "=");
    let shape = app.shape.as_ref().unwrap().shape;
    assert_eq!(shape.tonnetz.moves, Moves::Random);
    {
        use GenParam::*;
        let rows = app.shape.as_ref().unwrap().rows();
        assert!(rows.contains(&PWeight) && rows.contains(&Vary), "{rows:?}");
        assert!(!rows.contains(&MoveList), "{rows:?}");
    }
    keys(&mut app, "L");
    let written = steps(&app, 0);
    assert!(
        written
            .iter()
            .all(|step| step.fx.iter().all(|fx| fx.command.is_none()))
    );
    // Each root within a tritone of the first.
    let roots = pitches(&app, 0);
    assert!(
        roots.iter().all(|root| root.abs_diff(roots[0]) <= 6),
        "{roots:?}"
    );
    assert_eq!(app.song().tracks[0].phrases[&0].generators().len(), 1);
}

#[test]
fn across_three_tracks_each_plays_a_voice_and_one_moves_at_a_time() {
    let (mut app, _rt) = app_with(3, Machine::Plaits(SynthVoice::VirtualAnalog));
    keys(&mut app, "vGWWSK/tonnetz<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.parts.len(), 3);
    assert_eq!(panel.shape.tonnetz.voice, None);
    assert!(!panel.rows().contains(&GenParam::Voice), "each track's own");
    let screen = drawn(&app);
    assert!(screen.contains("A voice a track"), "{screen}");
    keys(&mut app, "L");
    let voices: Vec<Option<TonnetzVoice>> = (0..3)
        .map(|track| {
            app.song().tracks[track].phrases[&0].generators()[0]
                .shape
                .tonnetz
                .voice
        })
        .collect();
    assert_eq!(
        voices,
        [
            Some(TonnetzVoice::Low),
            Some(TonnetzVoice::Mid),
            Some(TonnetzVoice::High)
        ]
    );
    let tracks: Vec<Vec<u8>> = (0..3).map(|track| pitches(&app, track)).collect();
    for row in 1..16 {
        let moved: Vec<i16> = tracks
            .iter()
            .map(|notes| i16::from(notes[row]) - i16::from(notes[row - 1]))
            .filter(|by| *by != 0)
            .collect();
        assert_eq!(moved.len(), 1, "row {row}: {moved:?}");
        assert!(matches!(moved[0].abs(), 1 | 2 | 10 | 11), "row {row}");
    }
}
