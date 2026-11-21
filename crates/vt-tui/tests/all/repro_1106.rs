use crate::common::*;
use std::sync::Arc;
use vt_model::shape::GenParam;
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;

fn on_rhythm() -> App {
    let mut song = Song::new(1);
    let steps: Vec<Step> = (0..16)
        .map(|_| "--- -- --- --- ---".parse().unwrap())
        .collect();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let (mut app, rt) = app_for(song);
    std::mem::forget(rt);
    keys(&mut app, "vGS");
    down_to(&mut app, GenParam::Where);
    keys(&mut app, "K/rhythm<Enter>");
    app
}

fn find(app: &mut App, param: GenParam) -> (usize, usize) {
    keys(app, "gg");
    for row in 0..8 {
        keys(app, "0");
        for col in 0..4 {
            if here(app) == Some(param) {
                return (row, col);
            }
            keys(app, "l");
            if app.modes.mode != vt_tui::input::mode::Mode::Normal {
                keys(app, "<Esc>");
            }
        }
        keys(app, "j");
    }
    panic!()
}

#[test]
fn repro() {
    let mut app = on_rhythm();
    down_to(&mut app, GenParam::OwnText);
    keys(&mut app, "ix");
    eprintln!(
        "panel i then x: {:?}",
        app.overlays
            .shape
            .as_ref()
            .unwrap()
            .typing
            .as_ref()
            .map(|t| t.as_str().to_owned())
    );
    keys(&mut app, "<Esc>");
    keys(&mut app, "L 5LLL");
    let named = find(&mut app, GenParam::OwnText);
    eprintln!("named Text cell at {named:?}");
    keys(&mut app, "l");
    eprintln!(
        "l on named Text: mode {:?} here {:?}",
        app.modes.mode,
        here(&app)
    );
    keys(&mut app, "x<Esc>");
    let own = find(&mut app, GenParam::OwnText);
    eprintln!("Own Text cell at {own:?}");
    keys(&mut app, "l");
    eprintln!(
        "l on own Text: mode {:?} here {:?}",
        app.modes.mode,
        here(&app)
    );
}
