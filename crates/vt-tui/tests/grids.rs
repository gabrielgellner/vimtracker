//! The shape panel's `Grids`, by keys: `S` across a kit's tracks writes one beat, each track its
//! own part, kept live as a generator a track, each reopened with `gl` on its own.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::shape::{DrumPart, FillKind, GenParam};
use vt_model::song::{Machine, Phrase, Song, SynthVoice};
use vt_model::step::{MAX_VELOCITY, Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::track_page::Page;

mod common;
use common::*;

const E: &str = "--- -- --- --- ---";

/// A song of empty phrases of 16 rows, its tracks named and given these machines.
fn app_with(tracks: &[(&str, Machine)]) -> (App, Realtime) {
    let mut song = Song::new(tracks.len());
    for (track, (name, machine)) in song.tracks.iter_mut().zip(tracks) {
        track.name = (*name).to_owned();
        track.machine = *machine;
        let steps: Vec<Step> = (0..16).map(|_| E.parse().unwrap()).collect();
        track.phrases.insert(0, Arc::new(Phrase::from_steps(steps)));
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(48_000, version));
    (App::new(song, None, samples, handle, 48_000), realtime)
}

/// Type keys, with `<Enter>` written out.
fn keys(app: &mut App, text: &str) {
    let now = Instant::now();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(tail) = rest.strip_prefix("<Enter>") {
            app.key(Key::new(KeyCode::Enter), now);
            rest = tail;
        } else {
            app.key(Key::char(c), now);
            rest = &rest[c.len_utf8()..];
        }
    }
}

/// A track's rows as `X` for an accent, `x` for a note and `.` for none.
fn rhythm(app: &App, track: usize) -> String {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .map(|step| match (step.note, step.velocity) {
            (Note::On(_), Some(MAX_VELOCITY)) => 'X',
            (Note::On(_), _) => 'x',
            _ => '.',
        })
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(52, 16);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..16)
        .map(|y| {
            let line: String = (0..52).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The middle of the map at the densities it starts at: the kick's, the snare's and the hats'.
const BEAT: [&str; 3] = ["X.....x.X.x.....", "....X.....x.XXx.", "..X.x.X...X.x.x."];

#[test]
fn grids_across_a_kits_tracks_writes_one_beat_each_its_own_part() {
    // A hat, a kick and a pad, out of order: the pad takes the part left, the snare's.
    let (mut app, _rt) = app_with(&[
        ("HH", Machine::Sampler),
        ("", Machine::Plaits(SynthVoice::Kick)),
        ("Pad", Machine::Sampler),
    ]);
    let depth = app.history.undo_depth();
    keys(&mut app, "vGWWSK/grids<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Grids);
    assert_eq!(panel.shape.grids.part, None, "each track takes its own");
    use GenParam::*;
    assert_eq!(
        panel.rows(),
        [
            Fill,
            Pitch,
            MapX,
            MapY,
            KickDensity,
            SnareDensity,
            HatDensity,
            Chaos,
            Rows
        ]
    );
    assert_eq!(rhythm(&app, 0), BEAT[2]);
    assert_eq!(rhythm(&app, 1), BEAT[0]);
    assert_eq!(rhythm(&app, 2), BEAT[1]);
    let screen = drawn(&app);
    for expected in [
        "Note on 3 tracks:",
        "X          80",
        "BD density 80",
        "HH density 80",
        "Rows       1/16",
        "Each track plays its own part",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    // At the top of the map, the kick is four on the floor once its density's down.
    keys(&mut app, "jjji00ji40");
    assert_eq!(rhythm(&app, 1), "X...X...X...X...");
    keys(&mut app, "<Enter>");
    assert_eq!(
        app.history.undo_depth(),
        depth + 1,
        "one undo for all three"
    );
    assert_eq!(rhythm(&app, 1), "X...X...X...X...");
}

#[test]
fn kept_live_each_track_has_a_generator_of_its_part_reopened_on_its_own() {
    let (mut app, _rt) = app_with(&[
        ("BD", Machine::Sampler),
        ("SD", Machine::Sampler),
        ("CH", Machine::Sampler),
    ]);
    keys(&mut app, "vGWWSK/grids<Enter>L");
    for (track, part) in DrumPart::ALL.into_iter().enumerate() {
        let generators = app.song().tracks[track].phrases[&0].generators().to_vec();
        assert_eq!(generators.len(), 1);
        assert_eq!(generators[0].shape.grids.part, Some(part));
        assert_eq!(rhythm(&app, track), BEAT[track]);
    }
    // `gl` on the snare: its part and its own density alone.
    keys(&mut app, "Wgl");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.live, Some(0));
    use GenParam::*;
    assert_eq!(
        panel.rows(),
        [Fill, Part, Pitch, MapX, MapY, SnareDensity, Chaos, Rows]
    );
    let screen = drawn(&app);
    assert!(screen.contains("Part       SD"), "{screen}");
    // Its density down to none: the snare goes quiet, and the others play on.
    keys(&mut app, "jjjjji00L");
    assert_eq!(rhythm(&app, 1), "................");
    assert_eq!(rhythm(&app, 0), BEAT[0]);
    assert_eq!(rhythm(&app, 2), BEAT[2]);
    let shape = app.song().tracks[1].phrases[&0].generators()[0].shape;
    assert_eq!(shape.grids.density[DrumPart::Snare.index()], 0);
    // Its part, chosen with `K`: the snare's track plays the hats' part now.
    keys(&mut app, "glj");
    assert_eq!(app.shape.as_ref().unwrap().row, Part);
    keys(&mut app, "K/hh<Enter>");
    assert_eq!(rhythm(&app, 1), BEAT[2]);
}

#[test]
fn tracks_whose_sounds_say_nothing_take_the_parts_in_order() {
    let (mut app, _rt) = app_with(&[
        ("", Machine::Sampler),
        ("", Machine::Sampler),
        ("", Machine::Sampler),
    ]);
    keys(&mut app, "vGWWSK/grids<Enter>");
    for (track, beat) in BEAT.iter().enumerate() {
        assert_eq!(rhythm(&app, track), *beat);
    }
}

#[test]
fn a_live_grids_on_the_gen_page_is_named_by_its_part() {
    let (mut app, _rt) = app_with(&[("SD", Machine::Plaits(SynthVoice::Snare))]);
    keys(&mut app, "vGSK/grids<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(
        panel.shape.grids.part,
        Some(DrumPart::Snare),
        "its sound says"
    );
    keys(&mut app, "L");
    assert_eq!(rhythm(&app, 0), BEAT[1]);
    keys(&mut app, " 8");
    assert_eq!(app.track_page.page, Page::Gen);
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Grids  SD · Note 00–0F"), "{text}");
    for setting in ["X", "Y", "Chaos", "SD density", "Note", "Rows"] {
        assert!(text.contains(setting), "{setting} in\n{text}");
    }
}
