//! Mixer automation (decision 174): `LEV` sets the fader from a step, the mixer, the Output device
//! and the Roto show the faders where the steps have them, a hand moving one takes it over, and
//! `Space e` and `Space E` give them back.

use std::sync::Arc;

use vt_engine::engine::Engine;
use vt_engine::link::{Command, Realtime, connect};
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::Song;
use vt_tui::app::App;
use vt_tui::glyphs;
use vt_tui::track_page::Control;

mod common;
use common::*;

/// Two tracks, the first with a step that holds its fader at `80` and its `Pan` at `20`, playing
/// from the top until the steps have set them.
fn automated() -> (App, Realtime) {
    let mut song = Song::new(2);
    for track in 0..2 {
        *Arc::make_mut(song.tracks[track].phrases.get_mut(&0).unwrap())
            .step_mut(0)
            .unwrap() = "C-3 -- level:80h pan:20h ---".parse().unwrap();
    }
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    app.handle.send(Command::PlayFrom { step: 0 });
    for _ in 0..8 {
        block(&mut realtime, &mut app);
    }
    (app, realtime)
}

/// The mixer's text at the width the program is laid out for.
fn mixer_text(app: &App) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(163, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::mixer::draw(frame, frame.area(), app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    (0..30)
        .map(|y| (0..163).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// The mixer's row of one setting, by its name.
fn mixer_line(app: &App, name: &str) -> String {
    mixer_text(app)
        .into_iter()
        .find(|line| line.starts_with(name))
        .unwrap()
}

#[test]
fn a_held_lev_and_pan_move_the_faders_shown_but_not_the_song() {
    let (app, _realtime) = automated();
    let faders = app.faders(0);
    assert!(
        faders.follows_steps(0) && faders.follows_steps(1),
        "{faders:?}"
    );
    assert_eq!(faders.at[0], 0x8000);
    assert_eq!(app.song().tracks[0].level, 0xE0, "the song keeps its own");
    let level = mixer_line(&app, "Level");
    assert!(level.starts_with("Level  80     80     "), "{level}");
    let pan = mixer_line(&app, "Pan");
    assert!(pan.starts_with("Pan    20     20     "), "{pan}");
}

#[test]
fn the_status_line_says_a_fader_is_from_the_steps() {
    let (mut app, _realtime) = automated();
    press(&mut app, " 6");
    let status = status_line(&app);
    assert!(status.contains("-24.0 dB, from the steps"), "{status}");
}

#[test]
fn a_hand_moving_a_fader_the_steps_drive_takes_it_over_from_where_it_is() {
    let (mut app, mut realtime) = automated();
    press(&mut app, " 6");
    press(&mut app, "=");
    assert_eq!(
        app.song().tracks[0].level,
        0x81,
        "a step up from where it's heard"
    );
    assert!(app.taken_over(0, Control::Level));
    block(&mut realtime, &mut app);
    let faders = app.faders(0);
    assert!(faders.taken[0], "the engine says so too");
    assert_eq!(faders.at[0], 0x8100, "what the hand set is heard");
    assert!(faders.follows_steps(1), "the pan still follows the steps");
    assert!(
        app.faders(1).follows_steps(0),
        "and the other track's level"
    );
    let level = mixer_line(&app, "Level");
    assert!(level.contains(&format!("81 {}", glyphs::TAKEN)), "{level}");
    let status = status_line(&app);
    assert!(status.contains("taken over from the steps"), "{status}");
    // Undoing the move leaves it taken over, as a takeover isn't an edit.
    press(&mut app, "u");
    block(&mut realtime, &mut app);
    assert!(app.faders(0).taken[0]);
    assert_eq!(app.faders(0).at[0], 0xE000);
}

#[test]
fn space_e_gives_the_tracks_faders_back_and_space_capital_e_every_tracks() {
    let (mut app, mut realtime) = automated();
    press(&mut app, " 6=l=h");
    block(&mut realtime, &mut app);
    assert!(app.faders(0).taken[0] && app.faders(1).taken[0]);
    press(&mut app, " e");
    assert!(!app.faders(0).taken[0], "{}", app.status);
    assert!(app.faders(1).taken[0], "only the cursor's track");
    block(&mut realtime, &mut app);
    assert!(app.faders(0).follows_steps(0));
    assert_eq!(app.faders(0).at[0], 0x8000, "back to the steps' hold");
    press(&mut app, " E");
    block(&mut realtime, &mut app);
    assert!(app.faders(1).follows_steps(0));
    assert_eq!(app.status, "Every fader follows the steps again");
    press(&mut app, " e");
    assert!(app.status.contains("no fader taken over"), "{}", app.status);
}

#[test]
fn a_fader_no_step_has_set_is_only_edited() {
    let (mut app, mut realtime) = automated();
    press(&mut app, " 6jj=");
    assert_eq!(app.song().tracks[0].send_a, 0x01);
    block(&mut realtime, &mut app);
    assert!(!app.faders(0).taken[2] && !app.faders(0).automated[2]);
}

#[test]
fn the_output_device_shows_the_fader_where_the_steps_have_it() {
    let (mut app, _realtime) = automated();
    press(&mut app, " 4");
    let backend = ratatui::backend::TestBackend::new(163, 42);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::views::track::draw(frame, frame.area(), &app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines: Vec<String> = (0..42)
        .map(|y| (0..163).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let line = |name: &str| lines.iter().find(|line| line.contains(name)).unwrap();
    assert!(
        line("Level ").contains("Level     80 "),
        "{}",
        line("Level ")
    );
    assert!(line("Pan ").contains("Pan       2000"), "{}", line("Pan "));
}

#[test]
fn the_rotos_mix_mode_knobs_follow_the_faders_the_steps_drive() {
    use vt_tui::roto::mix::{level_position, strips};
    let (mut app, mut realtime) = automated();
    let faders: Vec<_> = (0..app.song().tracks.len())
        .map(|at| app.faders(at))
        .collect();
    let [tracks, _] = strips(app.song(), &app.levels, &faders);
    assert_eq!(tracks[0].level, Some(level_position(f64::from(0x80_u8))));
    let unmoved = strips(app.song(), &app.levels, &[]);
    assert_eq!(
        unmoved[0][0].level,
        Some(level_position(f64::from(0xE0_u8)))
    );
    // Taken over, the knob is where the hand put it.
    press(&mut app, " 6=");
    block(&mut realtime, &mut app);
    let faders: Vec<_> = (0..app.song().tracks.len())
        .map(|at| app.faders(at))
        .collect();
    let [tracks, _] = strips(app.song(), &app.levels, &faders);
    assert_eq!(tracks[0].level, Some(level_position(f64::from(0x81_u8))));
}
