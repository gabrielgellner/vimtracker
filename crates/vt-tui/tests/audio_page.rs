//! The audio page, `Space 7`: devices, buffers and rates, what plays, the CPU's leaf, dropouts,
//! a new engine, and the scope.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_model::samples::SamplePool;
use vt_model::song::{Machine, Song, SynthVoice};
use vt_model::step::Note;
use vt_tui::app::App;
use vt_tui::input::mode::Mode;

mod common;
use common::*;

#[test]
fn the_audio_page_chooses_a_buffer_a_rate_and_a_device() {
    use vt_tui::audio::AudioChoice;
    use vt_tui::views::status::column_name;
    let (mut app, _realtime) = on_the_audio_page();
    assert_eq!(column_name(&app), "Device");
    // A smaller buffer, one step down from what's playing.
    press(&mut app, "jj-");
    assert_eq!(column_name(&app), "Buffer");
    assert_eq!(
        app.audio_request,
        Some(AudioChoice {
            buffer: Some(256),
            ..AudioChoice::default()
        })
    );
    // As if the main loop had opened it.
    app.audio.choice = app.audio_request.take().unwrap();
    app.audio.buffer = Some(256);
    press(&mut app, "k-");
    assert_eq!(app.audio_request.as_ref().unwrap().rate, Some(44_100));
    app.audio_request = None;
    // Another device keeps the buffer only if it takes it.
    press(&mut app, "k=");
    let request = app.audio_request.take().unwrap();
    assert_eq!(request.device.as_deref(), Some("coreaudio:headset"));
    assert_eq!(request.buffer, None, "the headset doesn't take 256");
    // x goes back to the system's own choice for the row, and K lists the choices.
    press(&mut app, "jjx");
    assert_eq!(app.audio_request.take().unwrap().buffer, None);
    press(&mut app, "K");
    assert_eq!(app.mode, Mode::Picker);
    let labels: Vec<String> = app
        .picker
        .as_ref()
        .unwrap()
        .items()
        .iter()
        .map(listed)
        .collect();
    assert_eq!(labels[0], "64 frames, 1.3 ms", "{labels:?}");
    assert_eq!(
        app.picker
            .as_ref()
            .unwrap()
            .current()
            .map(|item| item.label.as_str()),
        Some("256 frames, 5.3 ms"),
        "it opens on the one playing"
    );
    special(&mut app, KeyCode::Enter);
    assert_eq!(app.audio_request, None, "which is already playing");
}

#[test]
fn the_audio_page_shows_what_plays_and_when_it_stops() {
    let (mut app, _realtime) = on_the_audio_page();
    app.audio.latency_us = 21_300;
    let text = |app: &App| -> Vec<String> {
        let backend = ratatui::backend::TestBackend::new(90, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| vt_tui::views::audio::draw(frame, frame.area(), app))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..12)
            .map(|y| (0..90).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect()
    };
    let lines = text(&app);
    assert!(lines[2].contains("Speakers"), "{lines:?}");
    assert!(lines[3].contains("48000 Hz"), "{lines:?}");
    assert!(
        lines[4].contains("512 frames, 10.7 ms, the device's own"),
        "{lines:?}"
    );
    assert!(
        lines[5].contains("Wheel") && lines[5].contains("Off: the terminal keeps its mouse"),
        "{lines:?}"
    );
    assert!(
        lines[6].contains("Direction") && lines[6].contains("With the fingers"),
        "{lines:?}"
    );
    assert!(
        lines[7].contains("Roto") && lines[7].contains("Off, so Live can have it"),
        "{lines:?}"
    );
    assert!(lines[9].contains("21.3 ms"), "{lines:?}");
    app.wheel.setting.on = true;
    assert!(
        text(&app)[5].contains("Scrolling turns the setting under the cursor"),
        "{lines:?}"
    );
    app.audio.running = false;
    assert!(text(&app).iter().any(|line| line.contains("Stopped")));
}

/// The audio thread reported as slowed so many thousandths, for long enough to settle.
fn slowed_for_a_while(app: &mut App, thousandths: u64) {
    let start = Instant::now();
    for frame in 0..600 {
        app.slowdown
            .sample(thousandths, start + Duration::from_millis(33 * frame));
    }
}

#[test]
fn a_leaf_before_the_cpu_figure_shows_while_the_audio_thread_is_slowed() {
    use vt_tui::glyphs::LEAF;
    let (mut app, _realtime) = app();
    let top = |app: &mut App| screen_rows(app)[0].clone();
    let plain = top(&mut app);
    assert!(
        plain.trim_end().ends_with("CPU  0%  late 0  xruns 0"),
        "{plain}"
    );
    assert!(!plain.contains(LEAF));
    slowed_for_a_while(&mut app, 3600);
    let slowed = top(&mut app);
    assert!(
        slowed
            .trim_end()
            .ends_with(&format!("{LEAF} CPU  0%  late 0  xruns 0")),
        "{slowed}"
    );
    // It comes before the figure, so the figure stays where it was.
    let trailing = |row: &str| row.len() - row.trim_end().len();
    assert_eq!(trailing(&slowed), trailing(&plain));
    slowed_for_a_while(&mut app, 1000);
    assert_eq!(top(&mut app), plain, "and goes at full speed");
}

#[test]
fn the_audio_page_says_what_the_leaf_means_on_the_status_line() {
    use vt_tui::glyphs::LEAF;
    let (mut app, _realtime) = on_the_audio_page();
    app.status.clear();
    let message = |app: &mut App| screen_rows(app)[41].trim_end().to_owned();
    assert_eq!(message(&mut app), "", "nothing to say at full speed");
    slowed_for_a_while(&mut app, 3600);
    assert_eq!(
        message(&mut app),
        format!("{LEAF} CPU slowed ×3.6 by power saving, under 1% at full speed")
    );
    let rows = screen_rows(&mut app);
    assert!(
        rows.iter()
            .any(|row| row.starts_with(&format!("Keeping up  {LEAF} CPU 0%"))),
        "the page's own figure has the leaf too: {rows:#?}"
    );
    // A message comes first, and other pages don't say it.
    app.status = "Playing through Speakers at 48000 Hz".to_owned();
    assert_eq!(message(&mut app), "Playing through Speakers at 48000 Hz");
    app.status.clear();
    press(&mut app, " 3");
    assert_eq!(message(&mut app), "");
}

#[test]
fn a_new_engine_keeps_the_song_its_undo_and_the_mutes() {
    let (mut app, _realtime) = app();
    press(&mut app, "ia");
    special(&mut app, KeyCode::Esc);
    press(&mut app, " m");
    let song = Arc::clone(app.song());
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::clone(&song), Arc::clone(&samples)));
    let (handle, _fresh) = connect(Engine::new(44_100, version));
    app.reconnect(handle, samples, 44_100);
    assert_eq!(app.rate, 44_100);
    assert!(Arc::ptr_eq(&song, app.song()));
    assert!(app.levels[0].mute, "still muted");
    assert_eq!(app.transport, Transport::Stopped);
    press(&mut app, "u");
    assert_eq!(
        notes(&app)[0],
        Note::Empty,
        "and undo still reaches before it"
    );
}

#[test]
fn dropouts_put_a_warning_at_the_right_of_the_status_line_only_once_there_are_any() {
    use vt_tui::glyphs::{SPINNER, WARNING};
    let (mut app, _rt) = app();
    let line = status_line(&app);
    assert!(!line.contains(WARNING));
    assert!(
        !line.contains(SPINNER),
        "nothing is going on in the background"
    );
    assert_eq!(app.background_work(), None);
    app.audio.late = 2;
    app.audio.xruns = 1;
    let line = status_line(&app);
    assert!(line.ends_with(&format!("{WARNING} 3  ")), "{line}");
}

#[test]
fn the_scope_sends_a_picture_only_when_it_changes_and_draws_above_the_meter() {
    use vt_tui::kitty::Graphics;
    let mut song = Song::new(2);
    song.tracks[0].machine = Machine::Plaits(SynthVoice::VirtualAnalog);
    let phrase = Arc::make_mut(song.tracks[0].phrases.get_mut(&0).unwrap());
    *phrase.step_mut(0).unwrap() = "C-2 64 --- --- ---".parse().unwrap();
    let samples = Arc::new(SamplePool::default());
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(48_000, version));
    let mut app = App::new(song, None, samples, handle, 48_000);
    let mut graphics = Graphics::new();
    let sync = |app: &mut App, graphics: &mut Graphics| {
        let mut out = Vec::new();
        graphics.sync(app, &mut out).unwrap();
        String::from_utf8(out)
            .unwrap()
            .matches(",c=12,r=3,")
            .count()
    };
    // Silence is a flat line, sent once.
    assert_eq!(sync(&mut app, &mut graphics), 1);
    let flat = app.scope_image.unwrap();
    block(&mut realtime, &mut app);
    assert_eq!(sync(&mut app, &mut graphics), 0, "still silent");
    assert_eq!(app.scope_image, Some(flat));

    special(&mut app, KeyCode::Tab);
    for _ in 0..8 {
        block(&mut realtime, &mut app);
    }
    // It's brought up to date every other frame, so one of the next two sends it.
    let sent = sync(&mut app, &mut graphics) + sync(&mut app, &mut graphics);
    assert_eq!(sent, 1, "the note is heard");
    assert_ne!(app.scope_image, Some(flat));

    // A picture after every frame, for longer than there are ids, never takes the one on screen.
    let mut ids = std::collections::HashSet::new();
    for _ in 0..600 {
        block(&mut realtime, &mut app);
        let before = app.scope_image;
        let mut out = Vec::new();
        graphics.sync(&mut app, &mut out).unwrap();
        let out = String::from_utf8(out).unwrap();
        let shown = app.scope_image.unwrap();
        assert!(
            !out.contains(&format!("a=d,d=I,i={shown},")),
            "the shown picture is kept"
        );
        if app.scope_image != before {
            ids.insert(shown);
        }
    }
    assert!(ids.len() > 100, "{}", ids.len());

    // Drawn at the top of the meter's column, after its gap.
    let backend = ratatui::backend::TestBackend::new(120, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let corner = &buffer[(108, 0)];
    assert_eq!(corner.symbol(), "\u{10EEEE}\u{305}\u{305}");
    assert_eq!(corner.fg, vt_tui::kitty::id_color(app.scope_image.unwrap()));
    assert!(app.scope_shown);

    // On a screen too short for it, it isn't drawn, and nothing more is sent.
    let backend = ratatui::backend::TestBackend::new(120, 10);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| vt_tui::run::draw_screen(frame, &mut app, None, Instant::now()))
        .unwrap();
    assert!(!app.scope_shown);
    block(&mut realtime, &mut app);
    let sent = sync(&mut app, &mut graphics) + sync(&mut app, &mut graphics);
    assert_eq!(sent, 0);
}
