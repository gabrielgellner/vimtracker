//! What was heard of a live generator a macro moved as it played (decision 300): the phrase's
//! title marks its column `~`, the playhead's row draws what was heard there, the status line on
//! a moved cell says both, and `Space g a` and `Space g A` keep the rows as heard.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::link::{Command, Realtime, connect};
use vt_engine::published::Published;
use vt_model::action::{Action, edit};
use vt_model::generator::Span;
use vt_model::lfo::Shape as Wave;
use vt_model::macros::{GenColumn, MacroMap, MacroTarget};
use vt_model::samples::SamplePool;
use vt_model::selection::StepRange;
use vt_model::shape::{
    Acid, Bits, DEFAULT_RATE, FillKind, Follow, GenParam, Pink, Shape, ShapeColumn, Values, Walk,
    Where,
};
use vt_model::song::{Phrase, Song};
use vt_model::step::Step;
use vt_tui::app::App;

mod common;
use common::*;

/// 6000 frames a step at 120 BPM.
const STEP: usize = 6000;

/// Three hits of `7F` spread over sixteen rows, `40` between.
fn tresillo() -> Shape {
    Shape {
        from: 0x7F,
        to: 0x7F,
        curve: 0,
        s: 0,
        kind: FillKind::Alternate,
        wave: Wave::Sine,
        rate: DEFAULT_RATE,
        phase: 0,
        run: 1,
        values: Values::default(),
        place: Where::Euclid { hits: 3, rotate: 0 },
        seed: 7,
        acid: Acid::default(),
        follow: Follow::default(),
        walk: Walk::default(),
        pink: Pink::default(),
        bits: Bits::default(),
        grids: vt_model::shape::Grids::default(),
        vary: 0,
    }
}

fn apply(song: Song, action: &Action) -> Song {
    Arc::unwrap_or_clone(edit(&Arc::new(song), action).expect("the edit applies"))
}

/// Sixteen notes at `40` with the tresillo live on their velocities, macro 1 holding its `Hits`
/// from 1 to 16 at 3, and a `MAC1` lock of `FF` on row 8, which isn't a hit as written.
fn locked() -> Song {
    let mut song = Song::new(1);
    let mut steps: Vec<Step> = vec!["C-3 40 --- --- ---".parse().unwrap(); 16];
    steps[8] = "C-3 40 macro1:FF --- ---".parse().unwrap();
    song.tracks[0]
        .phrases
        .insert(0, Arc::new(Phrase::from_steps(steps)));
    let song = apply(
        song,
        &Action::Generate {
            track: 0,
            phrase: 0,
            column: ShapeColumn::Velocity,
            span: Span::rows(StepRange::between(0, 15)),
            shape: tresillo(),
        },
    );
    let hits = MacroTarget::Gen {
        column: GenColumn::Velocity,
        param: GenParam::Hits,
    };
    let song = apply(
        song,
        &Action::MapMacro {
            track: 0,
            at: 0,
            map: MacroMap::numbers(hits, (1, 16)),
        },
    );
    apply(
        song,
        &Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x2200,
        },
    )
}

/// The app on the locked song, played from the top to halfway through row 8.
fn heard_to_row_8() -> (App, Realtime) {
    let song = locked();
    assert_eq!(velocities(&song)[8], Some(0x40));
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    block(&mut realtime, &mut app);
    app.handle.send(Command::PlayFrom { step: 0 });
    for _ in 0..(STEP * 17 / 2) / 256 {
        block(&mut realtime, &mut app);
    }
    (app, realtime)
}

fn velocities(song: &Song) -> Vec<Option<u8>> {
    song.tracks[0].phrases[&0]
        .steps()
        .iter()
        .map(|step| step.velocity)
        .collect()
}

fn lines(app: &App) -> Vec<String> {
    let buffer = screen(app);
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn the_playhead_row_title_and_status_line_show_what_a_mac_lock_made_heard() {
    let (mut app, _realtime) = heard_to_row_8();
    let screen = lines(&app);
    assert!(screen[0].contains("Vel~"), "{}", screen[0]);
    // The playhead's row draws the hit heard; the grid elsewhere, what's written.
    let row = |number: &str| {
        screen
            .iter()
            .find(|line| line.get(4..6) == Some(number))
            .cloned()
            .unwrap_or_default()
    };
    assert!(row("08").contains("C-3 7F"), "{screen:#?}");
    assert!(row("07").contains("C-3 40"), "{screen:#?}");
    assert_eq!(velocities(app.song())[8], Some(0x40), "written as it was");
    // On row 8's velocity, the status line says both.
    press(&mut app, "8jl");
    let said = vt_tui::views::status::value_shown(&app).unwrap_or_default();
    assert_eq!(said, "40 written; 7F heard on pass 1");
}

#[test]
fn space_g_a_keeps_what_was_heard_as_one_undo_step() {
    let (mut app, _realtime) = heard_to_row_8();
    let written = velocities(app.song());
    press(&mut app, "l");
    let depth = app.history.undo_depth();
    press(&mut app, " ga");
    assert!(app.song().tracks[0].phrases[&0].generators().is_empty());
    let mut wanted = written.clone();
    wanted[8] = Some(0x7F);
    assert_eq!(
        velocities(app.song()),
        wanted,
        "row 8 as heard, the rest as written"
    );
    assert!(app.status.contains("applied as heard"), "{}", app.status);
    assert!(app.status.contains("not heard yet"), "{}", app.status);
    assert_eq!(app.history.undo_depth(), depth + 1);
    press(&mut app, "u");
    assert_eq!(velocities(app.song()), written);
    assert_eq!(app.song().tracks[0].phrases[&0].generators().len(), 1);
}

#[test]
fn space_g_capital_a_writes_in_the_moving_generators_as_heard() {
    let (mut app, _realtime) = heard_to_row_8();
    press(&mut app, " gA");
    assert!(app.song().tracks[0].phrases[&0].generators().is_empty());
    assert_eq!(velocities(app.song())[8], Some(0x7F));
    assert!(
        app.status.contains("Applied Vel as heard"),
        "{}",
        app.status
    );
}
