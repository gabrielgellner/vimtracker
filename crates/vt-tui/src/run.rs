//! Setting up audio and the terminal, and the main loop.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use thiserror::Error;
use vt_engine::engine::Engine;
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_model::constants::{DEFAULT_TRACKS, FRAME_MS};
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::app::App;
use crate::audio::{AudioError, AudioStats, Output};
use crate::input::keys::Key;
use crate::input::mode::Mode;
use crate::views;

/// Why the program stopped with an error.
#[derive(Debug, Error)]
pub enum RunError {
    #[error(transparent)]
    Audio(#[from] AudioError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the audio device was lost")]
    DeviceLost,
}

/// Open a project, or start an unsaved one, and run until the user quits.
pub fn run(path: Option<PathBuf>) -> Result<(), RunError> {
    let output = Output::open()?;
    let rate = output.rate();
    let mut status = String::new();
    let song = match &path {
        Some(folder) if folder.join("song.ron").exists() => project::load_song(folder)?,
        Some(folder) => {
            status = format!("New project: {} is created on :w", folder.display());
            Song::new(DEFAULT_TRACKS)
        }
        None => Song::new(DEFAULT_TRACKS),
    };
    let mut pool = SamplePool::default();
    let errors = project::load_samples(path.as_deref(), &song, rate, &mut pool);
    if let Some(first) = errors.first() {
        status = first.to_string();
    }
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(rate, version));
    let stats = Arc::new(AudioStats::default());
    let _stream = output.start(realtime, Arc::clone(&stats))?;
    let mut app = App::new(song, path, samples, handle, rate);
    app.status = status;

    let mut terminal = ratatui::init();
    let result = main_loop(&mut terminal, &mut app, &stats);
    ratatui::restore();
    result
}

fn main_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    stats: &AudioStats,
) -> Result<(), RunError> {
    let frame_time = Duration::from_millis(FRAME_MS);
    while !app.should_quit {
        if stats.lost.load(Ordering::Acquire) {
            return Err(RunError::DeviceLost);
        }
        app.tick();
        terminal.draw(|frame| {
            let [body, status] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
            app.follow_cursor(usize::from(body.height.saturating_sub(1)));
            views::phrase::draw(frame, body, app);
            views::status::draw(frame, status, app, Some(stats));
            views::help::draw_popup(frame, body, app, Instant::now());
            if app.mode == Mode::Help {
                views::help::draw_help(frame, body, app);
            }
        })?;
        if event::poll(frame_time)? {
            while event::poll(Duration::ZERO)? {
                if let Event::Key(key) = event::read()?
                    && key.kind != KeyEventKind::Release
                {
                    app.key(Key::from_event(key), Instant::now());
                }
            }
        }
    }
    Ok(())
}
