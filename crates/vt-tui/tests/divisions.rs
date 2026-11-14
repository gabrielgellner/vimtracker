//! The shape panel's `Divisions`, by keys: a chance for each note division, `S` across tracks
//! giving each a channel of the module's, `A`, `B` and `A+B`, kept live as a generator a track; a
//! track at `1/32` reading its rows as 32nds, its triplets nudged on the odd rows.

use std::sync::Arc;
use std::time::Instant;

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::Engine;
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::shape::{Bar, Channel, FillKind, GenParam};
use vt_model::song::{Phrase, Song};
use vt_model::speed::Speed;
use vt_model::step::{Note, Step};
use vt_tui::app::App;
use vt_tui::input::keys::Key;
use vt_tui::track_page::Page;

mod common;
use common::*;

const E: &str = "--- -- --- --- ---";

/// A song of empty phrases of `rows`, its tracks at these speeds.
fn app_with(speeds: &[Speed], rows: usize) -> (App, Realtime) {
    let mut song = Song::new(speeds.len());
    for (track, speed) in song.tracks.iter_mut().zip(speeds) {
        track.speed = *speed;
        let steps: Vec<Step> = (0..rows).map(|_| E.parse().unwrap()).collect();
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

/// A track's strikes, by row and nudge.
fn strikes(app: &App, track: usize) -> Vec<(usize, i8)> {
    app.song().tracks[track].phrases[&0]
        .steps()
        .iter()
        .enumerate()
        .filter(|(_, step)| matches!(step.note, Note::On(_)))
        .map(|(row, step)| (row, step.nudge))
        .collect()
}

/// The panel as drawn, a line a row.
fn drawn(app: &App) -> String {
    let panel = app.shape.clone().unwrap();
    let backend = ratatui::backend::TestBackend::new(60, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::shape::draw(frame, frame.area(), app, &panel))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..30)
        .map(|y| {
            let line: String = (0..60).map(|x| buffer[(x, y)].symbol()).collect();
            line.trim_end().to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn across_three_tracks_each_plays_a_channel_and_the_third_both() {
    let (mut app, _rt) = app_with(&[Speed::Sixteenth; 3], 16);
    let depth = app.services.history.undo_depth();
    keys(&mut app, "vGWWSK/divisions<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.kind, FillKind::Divisions);
    assert_eq!(
        panel.shape.divisions.channel, None,
        "each track takes its own"
    );
    {
        use GenParam::*;
        assert_eq!(
            panel.rows(),
            [
                Fill,
                Pitch,
                Bar,
                Quarters,
                Eighths,
                Sixteenths,
                QuarterTriplets,
                EighthTriplets,
                SixteenthTriplets,
                QuartersB,
                EighthsB,
                SixteenthsB,
                QuarterTripletsB,
                EighthTripletsB,
                SixteenthTripletsB,
                Vary
            ]
        );
    }
    let screen = drawn(&app);
    for expected in [
        "Note on 3 tracks:",
        "Bar     4/4 16ths",
        "1/4     75%",
        "B 1/8T  25%",
        "Each track its own channel: A, B, A+B",
    ] {
        assert!(screen.contains(expected), "{expected} in\n{screen}");
    }
    let (a, b, both) = (strikes(&app, 0), strikes(&app, 1), strikes(&app, 2));
    assert!(!a.is_empty() && !b.is_empty());
    assert_ne!(a, b, "dice of their own");
    // The third plays every row either of the others does, at its nudge.
    for strike in a.iter().chain(&b) {
        assert!(both.iter().any(|played| played.0 == strike.0), "{strike:?}");
    }
    assert!(
        both.iter()
            .all(|played| { a.iter().chain(&b).any(|strike| strike.0 == played.0) })
    );
    // B's eighth triplets on the odd rows, nudged a third of a sixteenth.
    assert!(
        b.iter()
            .all(|(row, nudge)| *nudge == 0 || (row % 2 == 1 && nudge.abs() == 8))
    );
    keys(&mut app, "L");
    assert_eq!(
        app.services.history.undo_depth(),
        depth + 1,
        "one undo for all three"
    );
    let channels: Vec<Option<Channel>> = (0..3)
        .map(|track| {
            app.song().tracks[track].phrases[&0].generators()[0]
                .shape
                .divisions
                .channel
        })
        .collect();
    assert_eq!(
        channels,
        [Some(Channel::A), Some(Channel::B), Some(Channel::Both)]
    );
}

#[test]
fn a_track_at_1_32_reads_its_rows_as_32nds_with_the_triplets_on_the_odd_ones() {
    let (mut app, _rt) = app_with(&[Speed::ThirtySecond], 32);
    keys(&mut app, "vGSK/divisions<Enter>");
    let panel = app.shape.clone().unwrap();
    assert_eq!(panel.shape.divisions.bar, Bar::FourFine);
    assert_eq!(panel.shape.divisions.channel, Some(Channel::A));
    // Only eighth triplets, every one.
    keys(&mut app, "jjjj");
    assert_eq!(app.shape.as_ref().unwrap().row, GenParam::Quarters);
    keys(&mut app, "i0<Enter>ji0<Enter>ji0<Enter>jji100<Enter>");
    assert_eq!(
        strikes(&app, 0),
        [
            (0, 0),
            (3, -4),
            (5, 4),
            (8, 0),
            (11, -4),
            (13, 4),
            (16, 0),
            (19, -4),
            (21, 4),
            (24, 0),
            (27, -4),
            (29, 4)
        ]
    );
    assert!(!drawn(&app).contains("share rows"));
}

#[test]
fn at_1_16_sixteenths_and_triplets_sharing_rows_are_pointed_out() {
    let (mut app, _rt) = app_with(&[Speed::Sixteenth], 16);
    keys(&mut app, "vGSK/divisions<Enter>");
    assert_eq!(app.shape.as_ref().unwrap().shape.divisions.bar, Bar::Four);
    // The sixteenths at 10% and the eighth triplets up.
    keys(&mut app, "jjjjjjjji50<Enter>");
    assert_eq!(app.shape.as_ref().unwrap().row, GenParam::EighthTriplets);
    let screen = drawn(&app);
    assert!(
        screen.contains("16ths and triplets need 32nds to fit"),
        "{screen}"
    );
}

#[test]
fn a_live_divisions_on_the_gen_page_shows_its_channel_bar_and_chances() {
    let (mut app, _rt) = app_with(&[Speed::Sixteenth], 16);
    keys(&mut app, "vGSK/divisions<Enter>L");
    keys(&mut app, " 8");
    assert_eq!(app.track_page.page, Page::Gen);
    let text = fx_page_text(&app).join("\n");
    assert!(
        text.contains("Divisions  A  4/4 16ths  Once · Note 00–0F"),
        "{text}"
    );
    for setting in ["1/4", "1/8", "1/16", "1/4T", "1/8T", "1/16T", "Note"] {
        assert!(text.contains(setting), "{setting} in\n{text}");
    }
    assert!(!text.contains("B 1/4"), "{text}");
    // As `A+B`, B's six take the page's last two columns.
    let (mut app, _rt) = app_with(&[Speed::Sixteenth], 16);
    keys(&mut app, "vGS");
    assert_eq!(app.shape.as_ref().unwrap().row, GenParam::Fill);
    keys(&mut app, "K/divisions<Enter>jj");
    assert_eq!(app.shape.as_ref().unwrap().row, GenParam::Channel);
    keys(&mut app, "K/A+B<Enter>L 8");
    let shape = app.song().tracks[0].phrases[&0].generators()[0].shape;
    assert_eq!(shape.divisions.channel, Some(Channel::Both));
    let text = fx_page_text(&app).join("\n");
    assert!(text.contains("Divisions  A+B  4/4 16ths"), "{text}");
    for setting in ["B 1/4", "B 1/8", "B 1/16T"] {
        assert!(text.contains(setting), "{setting} in\n{text}");
    }
}
