//! Kit lines (decision 464, #1093): `S` across a group's drum tracks with `Mini` makes one line
//! on every track, its hits their names; kept live, an edit on any track's Gen page is made on
//! all of them as one undo step, and the line's field reads in the names.

use std::sync::Arc;

use vt_engine::link::Realtime;
use vt_model::notation::Around;
use vt_model::shape::{FillKind, GenParam};
use vt_model::song::{Phrase, Song};
use vt_model::step::{Note, Step};
use vt_tui::app::App;

use crate::common::*;

const E: &str = "--- -- --- --- ---";
const NAMES: [&str; 3] = ["BD", "SN", "HH"];

/// Three drum tracks named for their sounds, in group 1, each with an empty phrase of 16 rows,
/// and a fourth track outside the group.
fn kit_app() -> (App, Realtime) {
    let mut song = Song::new(4);
    for (index, track) in song.tracks.iter_mut().enumerate().take(4) {
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
        if let Some(name) = NAMES.get(index) {
            track.name = (*name).to_owned();
            track.group = Some(1);
        }
    }
    app_for(song)
}

fn panel(app: &App) -> &vt_tui::overlays::shape::ShapePanel {
    app.overlays.shape.as_ref().expect("the panel")
}

/// The rows a track plays, `x` for a note.
fn rows(app: &App, track: usize) -> String {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match step.note {
            Note::On(_) => 'x',
            _ => '.',
        })
        .collect()
}

fn text(app: &App, track: usize) -> String {
    app.song().tracks[track].phrases[&0].generators()[0]
        .rhythm()
        .to_owned()
}

/// `S` across the group's tracks, `Mini`: a kit line, its field reading the names, `K`
/// offering them; kept live, the same text on every track and each playing its own rows.
#[test]
fn s_across_a_group_makes_a_kit_line() {
    let (mut app, _rt) = kit_app();
    keys(&mut app, "vGWWS");
    keys(&mut app, "K/mini<Enter>");
    assert_eq!(panel(&app).shape.kind, FillKind::Mini);
    assert!(panel(&app).shape.kit.is_some(), "a kit line");
    assert_eq!(panel(&app).names, NAMES.map(str::to_owned));
    assert_eq!(panel(&app).mini, "BD . SN . BD BD SN ., HH!8");
    down_to(&mut app, GenParam::MiniText);
    keys(&mut app, "Sbd . sn . bd bd sn ., hh*2<Enter>");
    assert_eq!(panel(&app).mini, "bd . sn . bd bd sn ., hh*2");
    let drawn = shape_panel(&app, 80, 30);
    assert!(
        drawn.contains("Text    bd . sn . bd bd sn ., hh*2"),
        "{drawn}"
    );
    // A name no track of the line has is refused, the line kept.
    keys(&mut app, "a cp<Enter>");
    assert_eq!(panel(&app).mini, "bd . sn . bd bd sn ., hh*2");
    assert!(
        app.services.status.contains("cp isn't a track of the line"),
        "{}",
        app.services.status
    );
    keys(&mut app, "<Esc>");
    assert_eq!(panel(&app).mini, "bd . sn . bd bd sn ., hh*2");
    let field = panel(&app).field(GenParam::MiniText).unwrap();
    assert_eq!(field.names(), NAMES.map(str::to_owned));
    let around = Around {
        column: panel(&app).column,
        from: 0,
        to: 0,
        tracks: &[],
        reached: &[],
    };
    let offered: Vec<String> = field
        .offers(0, &around)
        .into_iter()
        .map(|o| o.text)
        .collect();
    assert_eq!(offered[..3], NAMES.map(str::to_owned));
    keys(&mut app, "L");
    for track in 0..3 {
        assert_eq!(text(&app, track), "bd . sn . bd bd sn ., hh*2");
    }
    assert_eq!(rows(&app, 0), "x...xx..x...xx..");
    assert_eq!(rows(&app, 1), "..x...x...x...x.");
    assert_eq!(
        rows(&app, 2),
        "xxxxxxxxxxxxxxxx",
        "a ratchet each row, its RET"
    );
    assert!(app.song().tracks[3].phrases[&0].generators().is_empty());
}

/// On the Gen page of any of its tracks, a line typed is made on all of them, one undo step;
/// `u` takes it back on all.
#[test]
fn an_edit_on_one_tracks_gen_page_is_made_on_all() {
    let (mut app, _rt) = kit_app();
    keys(&mut app, "vGWWS");
    keys(&mut app, "K/mini<Enter>");
    down_to(&mut app, GenParam::MiniText);
    keys(&mut app, "Sbd sn hh .<Enter>L");
    // To the third track's Gen page.
    app.services.cursor.track = 2;
    keys(&mut app, " 5LLL");
    to_cell(&mut app, GenParam::MiniText);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "Shh hh sn bd<Enter>");
    assert_eq!(app.services.history.undo_depth(), depth + 1);
    for track in 0..3 {
        assert_eq!(text(&app, track), "hh hh sn bd");
    }
    assert_eq!(rows(&app, 0), "...x...x...x...x");
    // Its device shows the whole line, and its title the group's kit.
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            vt_tui::run::draw_view(frame, frame.area(), &app, vt_tui::input::mode::View::Track)
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let page: Vec<String> = (0..42)
        .map(|y| (0..163).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    assert!(
        page.iter()
            .any(|line| line.contains("Mini · Note kit G1 00–0F")),
        "{page:#?}"
    );
    assert!(
        page.iter()
            .any(|line| line.contains("Text      hh hh sn bd")),
        "{page:#?}"
    );
    keys(&mut app, "a qq<Enter><Esc><Esc>");
    assert_eq!(text(&app, 0), "hh hh sn bd", "a name of none of its tracks");
    // Turned in its tokens, a name goes to the one before among the line's tracks.
    keys(&mut app, "<Enter>-<Enter>");
    for track in 0..3 {
        assert_eq!(text(&app, track), "SN hh sn bd");
    }
    keys(&mut app, "uu");
    for track in 0..3 {
        assert_eq!(text(&app, track), "bd sn hh .");
    }
}

/// A kit already playing starts its line from its rows, each track keeping the note it plays.
#[test]
fn a_kit_starts_from_its_rows_at_its_own_notes() {
    let (app, rt) = kit_app();
    let mut song = (**app.song()).clone();
    drop((app, rt));
    let kick = "C-1 7F --- --- ---";
    let snare = "D-2 60 --- --- ---";
    for (track, note, at) in [(0, kick, [0, 8]), (1, snare, [4, 12])] {
        let mut steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        for row in at {
            steps[row] = note.parse().unwrap();
        }
        song.tracks[track]
            .phrases
            .insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    let (mut app, _rt) = app_for(song);
    keys(&mut app, "vGWWS");
    keys(&mut app, "K/mini<Enter>");
    assert_eq!(panel(&app).mini, "BD . . . SN . . . BD . . . SN . . .");
    keys(&mut app, "L");
    let note = |track: usize, row: usize| app.song().tracks[track].phrases[&0].steps()[row].note;
    assert_eq!(rows(&app, 0), "x.......x.......");
    assert_eq!(rows(&app, 1), "....x.......x...");
    assert_eq!(note(0, 8), kick.parse::<Step>().unwrap().note);
    assert_eq!(note(1, 4), snare.parse::<Step>().unwrap().note);
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
