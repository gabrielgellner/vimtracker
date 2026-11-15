//! The shape panel's `Where` of `Cells`, by keys (decision 326): on one column, each row a cell of
//! an elementary cellular automaton; across tracks, each track a cell and each row a generation,
//! one automaton for all of them, kept live as a generator a track; and on the Gen page, its
//! `Rule` and `Start` under the `Where`.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::shape::{CellStart, GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::controls::Page;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// A song of `tracks` empty phrases of 16 rows.
fn app_with(tracks: usize) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in song.tracks.iter_mut().take(tracks) {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    app_for(song)
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    shape_panel(app, 60, 30)
}

/// Open the panel on the selection, go to its `Where` and choose `Cells`, with this rule from
/// one cell.
fn cells_from_one(app: &mut App, select: &str, rule: u8) {
    keys(app, &format!("{select}S"));
    while app.overlays.shape.as_ref().unwrap().row != GenParam::Where {
        keys(app, "j");
    }
    keys(app, "K/cells<Enter>j");
    assert_eq!(app.overlays.shape.as_ref().unwrap().row, GenParam::Rule);
    keys(app, &format!("i{rule}<Enter>jK/one<Enter>"));
}

#[test]
fn on_one_column_each_row_is_a_cell() {
    let (mut app, _rt) = app_with(1);
    cells_from_one(&mut app, "vG", 90);
    let panel = app.overlays.shape.clone().unwrap();
    let Where::Cells(cells) = panel.shape.place else {
        panic!("{:?}", panel.shape.place);
    };
    assert_eq!(
        (cells.rule, cells.start, cells.across),
        (90, CellStart::One, None)
    );
    {
        use GenParam::*;
        assert_eq!(
            panel.rows(),
            [Fill, From, To, Curve, S, Run, Where, Rule, CellStart, Vary]
        );
    }
    let screen = drawn(&app);
    for expected in [
        "Where   Cells",
        "Rule    90",
        "Start   One",
        "Each row a cell, stepped on by Vary",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // The seed row: one cell, in the middle.
    assert_eq!(rhythm(&app, 0), "........x.......");
    // `Hits` starts from the notes there, which a column generated on hasn't.
    keys(&mut app, "K/hits<Enter>");
    assert_eq!(rhythm(&app, 0), "................");
    keys(&mut app, "K/seed<Enter>");
    let seeded = rhythm(&app, 0);
    assert!(seeded.contains('x'), "{seeded}");
    keys(&mut app, "r");
    assert_ne!(rhythm(&app, 0), seeded, "r draws another seed row");
    keys(&mut app, "L");
    let generator = &app.song().tracks[0].phrases[&0].generators()[0];
    assert!(matches!(generator.shape.place, Where::Cells(_)));
}

#[test]
fn across_tracks_each_is_a_cell_of_one_automaton() {
    let (mut app, _rt) = app_with(5);
    let depth = app.services.history.undo_depth();
    cells_from_one(&mut app, "vGWWWW", 90);
    let screen = drawn(&app);
    assert!(
        screen.contains("Each track a cell, a row a generation"),
        "{screen}"
    );
    // Read down the five tracks, row by row, it's rule 90's picture on a ring of five.
    let tracks: Vec<String> = (0..5).map(|track| rhythm(&app, track)).collect();
    let picture: Vec<String> = (0..8)
        .map(|row| tracks.iter().map(|track| &track[row..=row]).collect())
        .collect();
    assert_eq!(
        picture,
        [
            "..x..", ".x.x.", "x...x", "xx.xx", ".x.x.", "x...x", "xx.xx", ".x.x."
        ]
    );
    keys(&mut app, "L");
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo for all five"
    );
    let across: Vec<_> = (0..5)
        .map(|track| {
            match app.song().tracks[track].phrases[&0].generators()[0]
                .shape
                .place
            {
                Where::Cells(cells) => cells.across.map(|across| (across.cell, across.cells)),
                _ => None,
            }
        })
        .collect();
    assert_eq!(
        across,
        [
            Some((0, 5)),
            Some((1, 5)),
            Some((2, 5)),
            Some((3, 5)),
            Some((4, 5))
        ]
    );
    // Reopened on one, it has its rule and no start, as theirs is worked out for all five.
    keys(&mut app, "gg");
    keys(&mut app, "gl");
    let panel = app.overlays.shape.clone().expect("gl opens the panel");
    assert!(panel.rows().contains(&GenParam::Rule));
    assert!(!panel.rows().contains(&GenParam::CellStart));
}

#[test]
fn across_tracks_starts_from_the_notes_on_the_first_row() {
    let (mut app, _rt) = app_with(4);
    for track in [0, 2] {
        let mut song = Song::clone(app.song());
        let mut steps = song.tracks[track].phrases[&0].steps().to_vec();
        steps[0] = "C-3 64 --- --- ---".parse().unwrap();
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
        app.services.history.commit(Arc::new(song));
    }
    cells_from_one(&mut app, "vGWWW", 30);
    keys(&mut app, "K/hits<Enter>L");
    let rows: Vec<_> = (0..4)
        .map(|track| {
            match app.song().tracks[track].phrases[&0].generators()[0]
                .shape
                .place
            {
                Where::Cells(cells) => cells.across.map(|across| across.row),
                _ => None,
            }
        })
        .collect();
    assert_eq!(
        rows,
        [Some(0b0101); 4],
        "one start for all four, from their first rows"
    );
}

#[test]
fn a_live_cells_on_the_gen_page_has_its_rule_and_start_under_its_where() {
    let (mut app, _rt) = app_with(1);
    cells_from_one(&mut app, "vG", 30);
    keys(&mut app, "jK/each<Enter>L 8");
    assert_eq!(app.track_page.page, Page::Gen);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Ramp  Cells  Each pass"), "{text}");
    for setting in ["Rule", "Start", "One"] {
        assert!(text.contains(setting), "{setting} in\n{text}");
    }
    let shape = app.song().tracks[0].phrases[&0].generators()[0].shape;
    assert!(shape.evolves() && shape.varies());
}
