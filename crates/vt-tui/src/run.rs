//! Setting up audio and the terminal, and the main loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, DisableFocusChange, EnableFocusChange, Event, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::SetTitle;
use ratatui::crossterm::{execute, terminal};
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::widgets::Block;
use thiserror::Error;
use vt_engine::engine::Engine;
use vt_engine::link::connect;
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems, write_wav};
use vt_model::analysis;
use vt_model::constants::{DEFAULT_TRACKS, FRAME_MS};
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::app::App;
use crate::audio::{self, AudioChoice, AudioError, AudioStats, Output};
use crate::config::Settings;
use crate::input::command_history::CommandHistory;
use crate::input::keys::{KEYBOARD_FLAGS, Key};
use crate::input::mode::{Mode, View};
use crate::kitty::{self, Graphics};
use crate::places::RecentPlaces;
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
    #[error("couldn't write the render: {0}")]
    Render(#[from] hound::Error),
    #[error("{0} couldn't be loaded, so the render would be missing it")]
    MissingSample(String),
    #[error(
        "this terminal doesn't offer the kitty keyboard protocol, which says when a key is held \
         and let go. vimtracker runs in kitty and Ghostty, and not inside tmux. `render`, `bench` \
         and `probe-keys` run anywhere"
    )]
    NoKeyboardProtocol,
    #[error(
        "this terminal doesn't draw images with the kitty graphics protocol, which shows the \
         envelopes and the filter's curve. vimtracker runs in kitty and Ghostty, and not inside \
         tmux. `render`, `bench` and `probe-keys` run anywhere"
    )]
    NoGraphicsProtocol,
}

/// What `vimtracker render` was asked for.
pub struct RenderRequest {
    pub project: PathBuf,
    pub out: PathBuf,
    pub rate: u32,
    pub rows: Option<usize>,
    pub seed: Option<u64>,
    pub tail: bool,
    /// Write one file per track, into `out` as a folder.
    pub stems: bool,
}

/// What a stem render reports.
pub fn stems_written(count: usize, folder: &Path) -> String {
    let stems = if count == 1 { "stem" } else { "stems" };
    format!("Rendered {count} {stems} to {}", folder.display())
}

/// Write one file per track that would be heard, as `01-BD.wav` or `02.wav`, and return how many.
pub fn write_stems(
    version: &Published,
    folder: &Path,
    rate: u32,
    options: RenderOptions,
) -> Result<usize, RunError> {
    std::fs::create_dir_all(folder)?;
    let stems = render_stems(version, rate, options);
    for stem in &stems {
        let name = &version.song.tracks[stem.track].name;
        let number = stem.track + 1;
        let file = if name.is_empty() {
            format!("{number:02}.wav")
        } else {
            format!("{number:02}-{name}.wav")
        };
        write_wav(&folder.join(file), rate, &stem.samples)?;
    }
    Ok(stems.len())
}

/// Read or work out the bank's analyses before playing, with no terminal, so a normalised sample
/// plays at its level from the first frame. A file that can't be analysed is said on stderr, and
/// plays as it is.
fn analyse_now(folder: Option<&Path>, song: &Song, pool: &mut SamplePool) {
    let cache = analysis::cache_folder();
    for error in project::analyse_samples(folder, song, cache.as_deref(), pool) {
        eprintln!("{error}");
    }
}

/// Render a saved project to a file, with no terminal or sound device. A sample that won't load
/// stops it, since a file that quietly lacks a part is worse than no file.
pub fn render(request: &RenderRequest) -> Result<(), RunError> {
    let mut song = project::load_song(&request.project)?;
    if let Some(seed) = request.seed {
        song.seed = seed;
    }
    for note in project::find_sources(Some(&request.project), &mut song) {
        eprintln!("{note}");
    }
    let mut pool = SamplePool::default();
    let errors = project::load_samples(Some(&request.project), &song, request.rate, &mut pool);
    if let Some(first) = errors.first() {
        return Err(RunError::MissingSample(first.to_string()));
    }
    analyse_now(Some(&request.project), &song, &mut pool);
    let version = Published::new(Arc::new(song), Arc::new(pool));
    let options = RenderOptions {
        rows: request.rows,
        tail: request.tail,
    };
    if request.stems {
        let count = write_stems(&version, &request.out, request.rate, options)?;
        println!("{}", stems_written(count, &request.out));
        return Ok(());
    }
    let samples = render_loop(Arc::new(version), request.rate, options);
    write_wav(&request.out, request.rate, &samples)?;
    println!(
        "Rendered {} ({:.2} s)",
        request.out.display(),
        samples.len() as f64 / 2.0 / f64::from(request.rate)
    );
    Ok(())
}

/// `vimtracker bench`: play a project through the engine in blocks as fast as it goes, stopped and
/// then playing, and print the share of each block's time it took, as the status line works it out
/// for the audio thread. A block has no deadline here, so this is the engine's own cost, without
/// the device or how the system schedules its thread.
pub fn bench(folder: &Path, rate: u32, block: usize, seconds: u32) -> Result<(), RunError> {
    let song = project::load_song(folder)?;
    let mut pool = SamplePool::default();
    let errors = project::load_samples(Some(folder), &song, rate, &mut pool);
    if let Some(first) = errors.first() {
        return Err(RunError::MissingSample(first.to_string()));
    }
    analyse_now(Some(folder), &song, &mut pool);
    let version = Arc::new(Published::new(Arc::new(song), Arc::new(pool)));
    let block = block.max(1);
    let blocks = (seconds as usize * rate as usize).div_ceil(block);
    let budget = block as f64 / f64::from(rate);
    let mut engine = Engine::new(rate, version);
    let mut out = vec![0.0_f32; block * 2];
    let mut measure = |engine: &mut Engine| {
        let (mut total, mut worst) = (0.0_f64, 0.0_f64);
        for _ in 0..blocks {
            let started = Instant::now();
            engine.process(&mut out);
            let spent = started.elapsed().as_secs_f64();
            total += spent;
            worst = worst.max(spent);
        }
        (
            total / blocks as f64 / budget * 100.0,
            worst / budget * 100.0,
        )
    };
    let (stopped, stopped_worst) = measure(&mut engine);
    engine.play_from(0);
    let (playing, playing_worst) = measure(&mut engine);
    println!(
        "{} at {rate} Hz in blocks of {block}, {seconds} s each",
        folder.display()
    );
    println!("stopped  {stopped:5.2}% of a core, worst block {stopped_worst:5.1}%");
    println!("playing  {playing:5.2}% of a core, worst block {playing_worst:5.1}%");
    Ok(())
}

/// Open a project, or start an unsaved one, and run until the user quits.
pub fn run(path: Option<PathBuf>) -> Result<(), RunError> {
    // Before anything opens, so a terminal that can't run it is told so straight away.
    if !terminal::supports_keyboard_enhancement().unwrap_or(false) {
        return Err(RunError::NoKeyboardProtocol);
    }
    if !kitty::supports_graphics()? {
        return Err(RunError::NoGraphicsProtocol);
    }
    // What this machine played through last, or the system's own choice if that won't open.
    let choice = AudioChoice::load();
    let output = Output::open(&choice).or_else(|_| Output::open(&AudioChoice::default()))?;
    let rate = output.rate();
    let mut status = output.note.clone().unwrap_or_default();
    let mut song = match &path {
        Some(folder) if folder.join("song.ron").exists() => project::load_song(folder)?,
        Some(folder) => {
            status = format!("New project: {} is created on :w", folder.display());
            Song::new(DEFAULT_TRACKS)
        }
        None => Song::new(DEFAULT_TRACKS),
    };
    if let Some(first) = project::find_sources(path.as_deref(), &mut song).first() {
        status = first.clone();
    }
    let mut pool = SamplePool::default();
    let errors = project::load_samples(path.as_deref(), &song, rate, &mut pool);
    if let Some(first) = errors.first() {
        status = first.to_string();
    }
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::new(song.clone()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(rate, version));
    let stats = Arc::new(AudioStats::default());
    let device = output.name.clone();
    let (stream, buffer) = output.start(realtime, Arc::clone(&stats))?;
    let mut stream = Some(stream);
    let mut app = App::new(song, path, samples, handle, rate);
    app.use_analysis_cache(analysis::cache_folder());
    app.audio = crate::app::AudioState {
        device,
        buffer,
        running: true,
        choice,
        devices: audio::devices(),
        ..crate::app::AudioState::default()
    };
    app.library = crate::ableton::default_samples();
    if let Some(path) = Settings::path() {
        let (settings, note) = Settings::load(&path);
        app.settings = settings;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    if let Some(path) = CommandHistory::default_path() {
        let (history, note) = CommandHistory::load(&path);
        app.command_history = history;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    if let Some(path) = RecentPlaces::default_path() {
        let (places, note) = RecentPlaces::load(&path);
        app.recent_places = places;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    app.status = status;
    app.offer_restore();
    let mut graphics = Graphics::new();

    let mut terminal = ratatui::init();
    // Every key comes as a press, a repeat or a release from here to quitting. The terminal also
    // says when its window loses focus, as a key let go then is never reported. ratatui's own hook
    // puts the terminal back after a panic, and this lets go of the flags first.
    execute!(
        std::io::stdout(),
        PushKeyboardEnhancementFlags(KEYBOARD_FLAGS),
        EnableFocusChange
    )?;
    let restore = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(
            std::io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableFocusChange
        );
        restore(info);
    }));
    let result = main_loop(&mut terminal, &mut app, &stats, &mut graphics, &mut stream);
    execute!(
        std::io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableFocusChange
    )?;
    graphics.forget(&mut std::io::stdout())?;
    ratatui::restore();
    result
}

fn main_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    stats: &Arc<AudioStats>,
    graphics: &mut Graphics,
    stream: &mut Option<cpal::Stream>,
) -> Result<(), RunError> {
    let frame_time = Duration::from_millis(FRAME_MS);
    // Set at start, and again when opening, saving as or starting a new project changes it.
    let mut title = String::new();
    while !app.should_quit {
        if app.window_title() != title {
            title = app.window_title();
            execute!(std::io::stdout(), SetTitle(&title))?;
        }
        // A device that changes or goes away stops audio, and the audio page chooses again.
        if stream.is_some() && stats.lost.load(Ordering::Acquire) {
            *stream = None;
            app.audio.running = false;
            app.audio.devices = audio::devices();
            app.status =
                "The audio device changed or went away, so audio stopped. Space 7 chooses one"
                    .to_owned();
        }
        app.audio.latency_us = stats.latency.load(Ordering::Relaxed);
        app.audio.late = stats.late.load(Ordering::Relaxed);
        app.audio.xruns = stats.errors.load(Ordering::Relaxed);
        if std::mem::take(&mut app.audio.refresh) {
            app.audio.devices = audio::devices();
        }
        if let Some(choice) = app.audio_request.take() {
            // The old stream lets go of its device first.
            *stream = None;
            match switch_audio(app, &choice, stats) {
                Ok(playing) => *stream = Some(playing),
                Err(error) => {
                    app.audio.running = false;
                    app.status = format!("Couldn't play through that: {error}");
                }
            }
        }
        app.tick();
        app.cpu
            .sample(stats.load.load(Ordering::Relaxed), Instant::now());
        app.autosave_if_due(Instant::now());
        graphics.sync(app, &mut std::io::stdout())?;
        terminal.draw(|frame| draw_screen(frame, app, Some(stats), Instant::now()))?;
        if event::poll(frame_time)? {
            while event::poll(Duration::ZERO)? {
                match event::read()? {
                    Event::Key(key) => {
                        app.key_event(Key::from_event(key), key.kind, Instant::now())
                    }
                    Event::FocusLost => app.focus_lost(Instant::now()),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Open what the audio page chose and play through a new engine at its rate, with the samples
/// loaded again at that rate. The song, undo, mutes and solos stay, and it's kept for next time.
fn switch_audio(
    app: &mut App,
    choice: &AudioChoice,
    stats: &Arc<AudioStats>,
) -> Result<cpal::Stream, AudioError> {
    let output = Output::open(choice)?;
    let rate = output.rate();
    let mut pool = SamplePool::default();
    let errors = project::load_samples(app.folder.as_deref(), app.song(), rate, &mut pool);
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::clone(app.song()), Arc::clone(&samples)));
    let (handle, realtime) = connect(Engine::new(rate, version));
    let device = output.name.clone();
    let note = output.note.clone();
    let (stream, buffer) = output.start(realtime, Arc::clone(stats))?;
    app.reconnect(handle, samples, rate);
    app.audio.device = device;
    app.audio.buffer = buffer;
    app.audio.running = true;
    app.audio.choice = choice.clone();
    app.audio.devices = audio::devices();
    app.status = match (note, errors.first()) {
        (Some(note), _) => note,
        (None, Some(error)) => error.to_string(),
        (None, None) => format!("Playing through {} at {rate} Hz", app.audio.device),
    };
    if let Err(error) = choice.save() {
        app.status = format!("Playing, but the choice couldn't be kept: {error}");
    }
    Ok(stream)
}

/// Draw the whole screen: the view, the meter, the status line, and whatever is open over them.
/// Without audio stats, as for a screenshot, the system line shows no trouble.
pub fn draw_screen(
    frame: &mut ratatui::Frame,
    app: &mut App,
    stats: Option<&AudioStats>,
    now: Instant,
) {
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
        View::Mixer => views::mixer::draw(frame, view, app),
        View::Audio => views::audio::draw(frame, view, app),
        View::Project => views::project::draw(frame, view, app),
        View::Song => views::song::draw(frame, view, app),
    }
    // Inside the box the track page has while the keyboard plays it.
    let free = if app.mode == Mode::Keys {
        view.inner(Margin::new(1, 1))
    } else {
        view
    };
    views::system::draw(frame, free, app, stats);
    app.scope_shown = false;
    if let Some(column) = meter {
        let (scope, meter) = views::meter::split_scope(column);
        app.scope_shown = scope.is_some();
        if let (Some(area), Some(id)) = (scope, app.scope_image) {
            views::meter::draw_scope(frame, area, id);
        }
        views::meter::draw(frame, meter, &app.meter, app.song().limiter);
    }
    views::status::draw(frame, status, app);
    views::help::draw_popup(frame, body, app, now);
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
    if let Some(picker) = &mut app.picker {
        views::picker::draw(frame, body, picker);
    }
    if let Some(menu) = &mut app.command_menu
        && app.mode == Mode::Command
    {
        // Under the names it finishes, after the `:`.
        let column = 1 + menu.column(&app.command_line);
        views::picker::draw_menu(frame, status, column, &mut menu.picker);
    }
}
