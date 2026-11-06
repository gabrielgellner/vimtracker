//! Setting up audio and the terminal, and the main loop.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::widgets::Block;
use thiserror::Error;
use vt_engine::engine::Engine;
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_model::constants::{DEFAULT_TRACKS, FRAME_MS};
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::app::App;
use crate::audio::{AudioError, AudioStats, Output};
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::theme;
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
    #[error("couldn't write the render: {0}")]
    Render(#[from] hound::Error),
    #[error("{0} couldn't be loaded, so the render would be missing it")]
    MissingSample(String),
}

/// What `vimtracker render` was asked for.
pub struct RenderRequest {
    pub project: PathBuf,
    pub out: PathBuf,
    pub rate: u32,
    pub rows: Option<usize>,
    pub seed: Option<u64>,
    pub tail: bool,
}

/// Render a saved project to a file, with no terminal or sound device. A sample that won't load
/// stops it, since a file that quietly lacks a part is worse than no file.
pub fn render(request: &RenderRequest) -> Result<(), RunError> {
    let mut song = project::load_song(&request.project)?;
    if let Some(seed) = request.seed {
        song.seed = seed;
    }
    let mut pool = SamplePool::default();
    let errors = project::load_samples(Some(&request.project), &song, request.rate, &mut pool);
    if let Some(first) = errors.first() {
        return Err(RunError::MissingSample(first.to_string()));
    }
    let version = Published::new(Arc::new(song), Arc::new(pool));
    let options = RenderOptions {
        rows: request.rows,
        tail: request.tail,
    };
    let samples = render_loop(Arc::new(version), request.rate, options);
    write_wav(&request.out, request.rate, &samples)?;
    println!(
        "Rendered {} ({:.2} s)",
        request.out.display(),
        samples.len() as f64 / 2.0 / f64::from(request.rate)
    );
    Ok(())
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
    app.offer_restore();

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
        app.cpu
            .sample(stats.load.load(Ordering::Relaxed), Instant::now());
        app.autosave_if_due(Instant::now());
        terminal.draw(|frame| {
            frame.render_widget(Block::new().style(theme::screen()), frame.area());
            let [body, status] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(frame.area());
            app.follow_cursor(usize::from(
                body.height.saturating_sub(views::phrase::ROWS_AROUND_STEPS),
            ));
            let (view, meter) = views::meter::split(body);
            match app.view {
                View::Chain => views::chain::draw(frame, view, app),
                View::Phrase => views::phrase::draw(frame, view, app),
                View::Track => views::track::draw(frame, view, app),
            }
            views::system::draw(frame, view, app, Some(stats));
            if let Some(meter) = meter {
                views::meter::draw(frame, meter, &app.meter, app.song().limiter);
            }
            views::status::draw(frame, status, app);
            views::help::draw_popup(frame, body, app, Instant::now());
            if app.mode == Mode::Help {
                views::help::draw_help(frame, body, app);
            }
            app.guide_page = views::guide::page(body.height);
            app.guide_width = views::guide::text_width(body.width);
            if let Some(guide) = app.guide {
                views::guide::draw(frame, body, guide, app.guide_scroll);
            }
            if let Some(panel) = &app.shape {
                views::shape::draw(frame, view, app, panel);
            }
            if let Some(picker) = &app.picker {
                views::picker::draw(frame, body, picker);
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
