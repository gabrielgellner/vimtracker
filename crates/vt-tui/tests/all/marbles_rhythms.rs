//! Marbles' rhythms as `Where`s, by keys (decision 321): `Complementary` across two tracks gives
//! each row to one or the other, kept live as a generator a track; and on the Gen page, each
//! rhythm's amount is a cell to turn and `Complementary`'s side one to switch.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::generator::Generator;
use vt_model::shape::{GenParam, Where};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;
use vt_tui::pages::track::Page;

use crate::common::*;

const E: &str = "--- -- --- --- ---";

/// A song of so many tracks, each with an empty phrase of 16 rows.
fn app_with(tracks: usize) -> (App, Realtime) {
    let mut song = Song::new(tracks);
    for track in &mut song.tracks {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    app_for(song)
}

fn generators(app: &App, track: usize) -> Vec<Generator> {
    app.song().tracks[track].phrases[&0].generators().to_vec()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    shape_panel(app, 52, 16)
}

/// Press a key until the Gen page's cursor is on a setting.
fn until(app: &mut App, key: &str, param: GenParam) {
    for _ in 0..8 {
        if here(app) == Some(param) {
            return;
        }
        keys(app, key);
    }
    assert_eq!(here(app), Some(param));
}

#[test]
fn complementary_across_two_tracks_gives_each_row_to_one_or_the_other() {
    let (mut app, _rt) = app_with(2);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "vGWS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/compl<Enter>");
    let panel = app.overlays.shape.clone().unwrap();
    assert_eq!(
        panel.shape.place,
        Where::Complementary {
            share: 50,
            second: false
        }
    );
    assert!(panel.rows().contains(&GenParam::Share));
    assert!(panel.rows().contains(&GenParam::Side));
    let screen = drawn(&app);
    for expected in [
        "Note on 2 tracks: 16 of 32 rows",
        "Where   Complementary",
        "Amount  50%",
        "Side    First",
        "Each row goes to one track or the other",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    let (one, two) = (rhythm(&app, 0), rhythm(&app, 1));
    assert!(
        one.chars().zip(two.chars()).all(|(a, b)| a != b),
        "{one}\n{two}"
    );
    // A larger share gives the first track more of the rows, and still each row to one.
    down_to(&mut app, GenParam::Share);
    keys(&mut app, "i80<Enter>");
    let (one, two) = (rhythm(&app, 0), rhythm(&app, 1));
    assert!(
        one.matches('x').count() > two.matches('x').count(),
        "{one}\n{two}"
    );
    assert!(one.chars().zip(two.chars()).all(|(a, b)| a != b));
    keys(&mut app, "L");
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo for both"
    );
    let (first, second) = (generators(&app, 0)[0].shape, generators(&app, 1)[0].shape);
    assert_eq!(first.seed, second.seed, "the two tracks toss one coin");
    assert_eq!(
        (first.place, second.place),
        (
            Where::Complementary {
                share: 80,
                second: false
            },
            Where::Complementary {
                share: 80,
                second: true
            }
        )
    );
    assert!(first.draws(), "so it has Vary");
}

#[test]
fn the_gen_page_turns_a_rhythms_amount_and_switches_complementarys_side() {
    let (mut app, _rt) = app_with(1);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/clusters<Enter>L");
    assert_eq!(generators(&app, 0)[0].shape.place, Where::Clusters(50));
    // Bunched: every run of notes two rows long at least, but one the end cuts short.
    let runs = rhythm(&app, 0);
    assert!(runs.contains("xx") && runs.contains('.'), "{runs}");
    let whole = runs.trim_end_matches('x');
    assert!(!whole.split('.').any(|run| run.len() == 1), "{runs}");
    keys(&mut app, " 5LLL");
    assert_eq!(app.track_page.page, Page::Gen);
    keys(&mut app, "}k");
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Ramp  Clusters  Once"), "{text}");
    assert!(text.contains("Amount"), "{text}");
    // The amount is the third column's last cell, as a `Chance`'s is.
    keys(&mut app, "jjj$");
    assert_eq!(here(&app), Some(GenParam::Clusters));
    let depth = app.services.history.undo_depth();
    keys(&mut app, "i100<Enter>");
    assert_eq!(generators(&app, 0)[0].shape.place, Where::Clusters(100));
    assert_eq!(rhythm(&app, 0), "x".repeat(16));
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    assert!(status_line(&app).contains("100%"), "{}", status_line(&app));
    // `x` puts it back to half the rows.
    keys(&mut app, "x");
    assert_eq!(generators(&app, 0)[0].shape.place, Where::Clusters(50));
    // From the title, `Complementary`, then its side switched with `K`.
    until(&mut app, "k", GenParam::Vary);
    keys(&mut app, "h");
    assert_eq!(here(&app), Some(GenParam::Where));
    keys(&mut app, "Kcompl<Enter>");
    assert!(matches!(
        generators(&app, 0)[0].shape.place,
        Where::Complementary { .. }
    ));
    let first = rhythm(&app, 0);
    // Under the title's `Where`, the third column's last two cells.
    keys(&mut app, "jj$");
    assert_eq!(here(&app), Some(GenParam::Share));
    keys(&mut app, "j");
    assert_eq!(here(&app), Some(GenParam::Side));
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("First"), "{text}");
    keys(&mut app, "K");
    assert_eq!(
        generators(&app, 0)[0].shape.place,
        Where::Complementary {
            share: 50,
            second: true
        }
    );
    // The other side of the same coin.
    let second = rhythm(&app, 0);
    assert!(
        first.chars().zip(second.chars()).all(|(a, b)| a != b),
        "{first}\n{second}"
    );
}
