//! `Mini` (decision 462, #1092): a fill whose line holds its rhythm and its values, in the
//! column's units. It starts from what's in its rows; names and degrees play in one line; a stack
//! is refused on one track and gives each track a layer across them; the status line says how
//! many hits a `Step` drops; and kept live, the line is the generator's, saved as `mini`.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::scale::Scale;
use vt_model::shape::{FillKind, GenParam, ShapeColumn, degree_note};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// So many tracks, each with an empty phrase of 16 rows.
fn app_with(tracks: usize) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in &mut song.tracks {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    app_for(song)
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

/// The notes the panel writes on a track's phrase now, by row: none where it writes none.
fn shaped(app: &App, track: usize) -> Vec<Option<u8>> {
    let panel = panel(app);
    let nth = panel
        .parts
        .iter()
        .position(|part| part.track == track)
        .unwrap();
    let part = panel.parts[nth];
    let own = panel.own_of(nth);
    let steps = app.song().tracks[track].phrases[&0].steps().to_vec();
    let written = vt_model::shape::written_from(
        &steps,
        part.range,
        part.column,
        &panel.shape_of(nth),
        own.texts(&panel.list),
        None,
    );
    (0..16)
        .map(|row| {
            written
                .iter()
                .find(|(at, _)| *at == row)
                .and_then(|(_, step)| match step.note {
                    Note::On(note) => Some(note),
                    _ => None,
                })
        })
        .collect()
}

/// The panel open on the first track's rows, its fill `Mini`.
fn on_mini(app: &mut App, keys_first: &str) {
    keys(app, keys_first);
    keys(app, "K/mini<Enter>");
    assert_eq!(panel(app).shape.kind, FillKind::Mini);
}

/// On an empty column it starts from a line of its own; a line of names and degrees from 1 plays
/// each on its row; a stack is refused on one track; and hits sharing a row are counted.
#[test]
fn a_line_of_names_and_degrees_plays_a_token_a_row() {
    let (mut app, _rt) = app_with(1);
    on_mini(&mut app, "vGS");
    assert!(!panel(&app).mini.is_empty(), "it starts with a line");
    for row in [GenParam::MiniText, GenParam::MiniStep, GenParam::From] {
        assert!(panel(&app).rows().contains(&row), "{row:?}");
    }
    down_to(&mut app, GenParam::MiniText);
    keys(&mut app, "SC-3 . 1 c4<Enter>");
    assert_eq!(panel(&app).mini, "C-3 . 1 c4");
    let scale = match panel(&app).column {
        ShapeColumn::Note { scale, .. } => scale,
        _ => Scale::default(),
    };
    let root = degree_note(scale, panel(&app).shape.from, 0);
    let rows = shaped(&app, 0);
    assert_eq!(rows[..4], [Some(60), None, Some(root), Some(72)]);
    assert_eq!(rows[4], Some(60), "the line goes round");
    let drawn = shape_panel(&app, 80, 30);
    assert!(drawn.contains("Text    C-3 . 1 c4"), "{drawn}");
    // A stack on one track is refused, and the line kept.
    keys(&mut app, "a, G-3<Enter>");
    assert_eq!(panel(&app).mini, "C-3 . 1 c4");
    assert!(
        app.services.status.contains("layer a track"),
        "{}",
        app.services.status
    );
    // A triplet in a row drops two of its hits, and the status line says which `Step` keeps them.
    keys(&mut app, "S[C-3 D-3 E-3]<Enter>");
    assert!(
        app.services.status.contains("Step 3 rows keeps them"),
        "{}",
        app.services.status
    );
    down_to(&mut app, GenParam::MiniStep);
    keys(&mut app, "i4<Enter>");
    assert_eq!(shaped(&app, 0)[..4], [Some(60), Some(62), None, Some(64)]);
}

/// Across two tracks a stack gives each track its layer, as a bass and its answer.
#[test]
fn a_stack_across_tracks_gives_each_a_layer() {
    let (mut app, _rt) = app_with(2);
    on_mini(&mut app, "vGWS");
    assert_eq!(panel(&app).parts.len(), 2);
    down_to(&mut app, GenParam::MiniText);
    keys(&mut app, "SC-2*4, [. G-2]*2<Enter>");
    assert_eq!(panel(&app).mini, "C-2*4, [. G-2]*2");
    // A token a beat.
    down_to(&mut app, GenParam::MiniStep);
    keys(&mut app, "i4<Enter>");
    let bass = shaped(&app, 0);
    let answer = shaped(&app, 1);
    assert_eq!(bass.iter().flatten().count(), 16, "C-2 on every row");
    assert_eq!(answer[..4], [None, Some(55), None, Some(55)]);
    keys(&mut app, "L");
    let generator = |track: usize| app.song().tracks[track].phrases[&0].generators()[0].clone();
    assert_eq!(generator(0).rhythm(), "C-2*4");
    assert_eq!(generator(1).rhythm(), "[. G-2]*2");
}

/// Kept live, the line is the generator's, saved as `mini` and read back; on the Gen page its
/// `Text` takes a line typed, a stack refused.
#[test]
fn kept_live_the_line_is_saved_and_the_gen_page_types_it() {
    let (mut app, _rt) = app_with(1);
    on_mini(&mut app, "vGS");
    down_to(&mut app, GenParam::MiniText);
    keys(&mut app, "SC-3 <D#3 G-3> [. 5]<Enter>L");
    let generator = |app: &App| app.song().tracks[0].phrases[&0].generators()[0].clone();
    assert_eq!(generator(&app).rhythm(), "C-3 <D#3 G-3> [. 5]");
    assert!(generator(&app).heard().varies(), "its choice laps on");
    let folder = std::env::temp_dir().join(format!("vt-mini-line-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    vt_model::project::save_song(&folder, app.song()).expect("saves");
    let saved = std::fs::read_to_string(folder.join("song.ron")).unwrap();
    assert!(saved.contains("mini: \"C-3 <D#3 G-3> [. 5]\""), "{saved}");
    assert!(!saved.contains("rhythm: \""), "{saved}");
    let loaded = vt_model::project::load_song(&folder).expect("loads");
    let _ = std::fs::remove_dir_all(&folder);
    let again = &loaded.tracks[0].phrases[&0].generators()[0];
    assert_eq!(again.rhythm(), "C-3 <D#3 G-3> [. 5]");
    assert_eq!(again.shape, generator(&app).shape);
    assert_eq!(
        loaded.tracks[0].phrases[&0].steps(),
        app.song().tracks[0].phrases[&0].steps()
    );
    // The Gen page's `Text` cell.
    keys(&mut app, "L 5LLL");
    to_cell(&mut app, GenParam::MiniText);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "SE-3 F-3<Enter>");
    assert_eq!(generator(&app).rhythm(), "E-3 F-3");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    keys(&mut app, "a, F-3<Enter>");
    assert_eq!(generator(&app).rhythm(), "E-3 F-3");
    assert!(
        app.services.status.contains("layer a track"),
        "{}",
        app.services.status
    );
}

/// Down the Gen page's rows, and across each, to a generator's setting.
fn to_cell(app: &mut App, param: GenParam) {
    for _ in 0..8 {
        keys(app, "0");
        for _ in 0..4 {
            if here(app) == Some(param) {
                return;
            }
            keys(app, "l");
        }
        keys(app, "j");
    }
    panic!("no {param:?} on the Gen page");
}
