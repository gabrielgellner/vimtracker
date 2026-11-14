//! Setting up audio and the terminal, and the main loop.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableFocusChange, EnableFocusChange, Event, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::terminal::SetTitle;
use ratatui::crossterm::{execute, terminal};
use ratatui::layout::{Constraint, Layout, Margin};
use ratatui::widgets::Block;
use thiserror::Error;
use vt_engine::engine::{Engine, Transport};
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
use crate::wheel;

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
         and let go. vimtracker runs in kitty and Ghostty, and not inside tmux. `render`, `bench`, \
         `probe` and `probe-keys` run anywhere"
    )]
    NoKeyboardProtocol,
    #[error(
        "this terminal doesn't draw images with the kitty graphics protocol, which shows the \
         envelopes and the filter's curve. vimtracker runs in kitty and Ghostty, and not inside \
         tmux. `render`, `bench`, `probe` and `probe-keys` run anywhere"
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
    /// Write one file per group, and per track in none, into `out` as a folder.
    pub stems: bool,
    /// Write one file per track instead, those in groups too.
    pub split: bool,
}

/// What a stem render reports.
pub fn stems_written(count: usize, folder: &Path) -> String {
    let stems = if count == 1 { "stem" } else { "stems" };
    format!("Rendered {count} {stems} to {}", folder.display())
}

/// Write one file per stem that would be heard, as `01-BD.wav` or `02.wav` for a track and
/// `G1.wav` or `G1-DR.wav` for a group, and return how many.
pub fn write_stems(
    version: &Published,
    folder: &Path,
    rate: u32,
    options: RenderOptions,
) -> Result<usize, RunError> {
    std::fs::create_dir_all(folder)?;
    let stems = render_stems(version, rate, options);
    for stem in &stems {
        let track = &version.song.tracks[stem.track];
        let name = &track.name;
        // A group's named for its letter, and for its own name if it's been given one.
        let number = if track.is_sound() {
            format!("{:02}", stem.track + 1)
        } else {
            track.kind.letter()
        };
        let file = if name.is_empty() || *name == number {
            format!("{number}.wav")
        } else {
            format!("{number}-{name}.wav")
        };
        write_wav(&folder.join(file), rate, &stem.samples)?;
    }
    Ok(stems.len())
}

/// Read or work out the bank's analyses before playing, with no terminal, so a normalised sample
/// plays at its level from the first frame. A file that can't be analysed is said on stderr, and
/// plays as it is.
pub(crate) fn analyse_now(folder: Option<&Path>, song: &Song, pool: &mut SamplePool) {
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
        each_track: request.split,
    };
    if request.stems || request.split {
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

/// Open a project, or start an unsaved one, and run until the user quits.
pub fn run(path: Option<PathBuf>) -> Result<(), RunError> {
    // Before anything opens, so a terminal that can't run it is told so straight away.
    if !terminal::supports_keyboard_enhancement().unwrap_or(false) {
        return Err(RunError::NoKeyboardProtocol);
    }
    if !kitty::supports_graphics()? {
        return Err(RunError::NoGraphicsProtocol);
    }
    // While the song and its samples load.
    let full_speed = audio::measure_full_speed();
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
    let full_speed = full_speed.join().unwrap_or(Duration::MAX);
    let (stream, buffer) = output.start(realtime, Arc::clone(&stats), full_speed)?;
    let mut stream = Some(stream);
    let mut app = App::new(song, path, samples, handle, rate);
    app.use_analysis_cache(analysis::cache_folder());
    app.audio = crate::app::AudioState {
        device,
        buffer,
        running: true,
        choice,
        devices: audio::devices(),
        full_speed,
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
    if let Some(folder) = crate::config::config_folder() {
        let path = folder.join(vt_model::tag::TAGS_FILE);
        let (library, note) = vt_model::tag::TagLibrary::load(&path);
        app.tag_library = std::sync::Arc::new(library);
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
    // Off unless it was turned on here before, so Live keeps the Roto otherwise.
    if let Some(path) = crate::roto::RotoSetting::path() {
        app.roto.setting = crate::roto::RotoSetting::load_from(&path);
        app.roto.setting_path = Some(path);
    }
    // Its devices with buttons are vimtracker's own setups stored on it, written over its serial
    // port when they're missing or changed, with their versions kept in the cache folder.
    app.roto.serial = Some(crate::roto::system_serial());
    app.roto.versions_path = vt_model::analysis::program_cache_folder()
        .map(|folder| folder.join(crate::roto::VERSIONS_FILE));
    // Only one vimtracker on this machine has the Roto at a time: the one holding this lock.
    app.roto.lock_path = vt_model::analysis::program_cache_folder()
        .map(|folder| folder.join(crate::roto::LOCK_FILE));
    app.start_roto_if_on();
    // Off unless it was turned on here before, so the terminal keeps its mouse otherwise.
    if let Some(path) = crate::wheel::WheelSetting::path() {
        app.wheel.setting = crate::wheel::WheelSetting::load_from(&path);
        app.wheel.setting_path = Some(path);
    }
    // So the value follows the fingers whichever way the system scrolls.
    app.wheel.natural = crate::wheel::natural_scrolling();
    app.status = status;
    app.offer_restore();
    let mut graphics = Graphics::new();

    let mut terminal = ratatui::init();
    // Every key comes as a press, a repeat or a release from here to quitting. The terminal also
    // says when its window loses focus, as a key let go then is never reported. ratatui's own hook
    // puts the terminal back after a panic, and this lets go of the flags, and the mouse, first.
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
        let _ = wheel::capture(&mut std::io::stdout(), false);
        restore(info);
    }));
    let result = main_loop(&mut terminal, &mut app, &stats, &mut graphics, &mut stream);
    if app.wants_mouse() {
        wheel::capture(&mut std::io::stdout(), false)?;
    }
    execute!(
        std::io::stdout(),
        PopKeyboardEnhancementFlags,
        DisableFocusChange
    )?;
    graphics.forget(&mut std::io::stdout())?;
    ratatui::restore();
    result
}

/// How often the main loop asks whether Low Power Mode is on, or the Mac is hot.
const POWER_EVERY: Duration = Duration::from_secs(2);

/// Frames in a row drawn the same, about half a second, before the screen counts as still.
const STILL_AFTER: u32 = 15;
/// While the screen is still, it's drawn every this many frames, about four times a second, for
/// a change that no key, mouse or window event brought.
const STILL_EVERY: u32 = 8;

/// When the screen is drawn. Drawing it the same as last time was most of what the interface did
/// while nothing happened, so once it has come out the same for a while it's drawn only now and
/// then, and at once again when a key, the mouse or the window brings something, the song
/// changes, or the song plays (#547). The loop still runs every frame, so nothing else waits.
#[derive(Debug, Default)]
struct Redraws {
    /// Frames in a row drawn the same as the one before.
    same: u32,
    /// Frames gone by since it was last drawn.
    since: u32,
}

impl Redraws {
    /// Whether to draw this frame, given whether something woke the screen.
    fn due(&mut self, woken: bool) -> bool {
        self.since += 1;
        woken || self.same < STILL_AFTER || self.since >= STILL_EVERY
    }

    /// It was drawn, the same as the time before or not.
    fn drawn(&mut self, changed: bool) {
        self.same = if changed {
            0
        } else {
            self.same.saturating_add(1)
        };
        self.since = 0;
    }
}

/// A writer that notes whether anything went through it.
struct Noted<W> {
    inner: W,
    wrote: bool,
}

impl<W: std::io::Write> std::io::Write for Noted<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.wrote |= !bytes.is_empty();
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
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
    // Asked about now and then, as it changes seldom.
    let mut power_asked: Option<Instant> = None;
    // Whether the terminal is sending the mouse, which the wheel's setting turns on and off.
    let mut mouse = false;
    let mut redraws = Redraws::default();
    // The screen as last drawn, and the song it showed, to tell whether anything changed.
    let mut shown = Buffer::empty(ratatui::layout::Rect::ZERO);
    let mut shown_song = Arc::clone(app.song());
    let mut woken = true;
    while !app.should_quit {
        if app.wants_mouse() != mouse {
            mouse = app.wants_mouse();
            wheel::capture(&mut std::io::stdout(), mouse)?;
        }
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
                "The audio device changed or went away, so audio stopped. Space 9 L chooses one"
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
        app.slowdown
            .sample(stats.slowdown.load(Ordering::Relaxed), Instant::now());
        if power_asked.is_none_or(|asked| asked.elapsed() >= POWER_EVERY) {
            power_asked = Some(Instant::now());
            app.slowdown.power = audio::PowerState::now();
        }
        app.autosave_if_due(Instant::now());
        woken |= app.transport == Transport::Playing || !Arc::ptr_eq(app.song(), &shown_song);
        if redraws.due(woken) {
            // The sample page's waveform is drawn at one pixel for each of the screen's.
            if let Ok(size) = terminal::window_size() {
                graphics.use_window(size);
            }
            let mut out = Noted {
                inner: std::io::stdout(),
                wrote: false,
            };
            graphics.sync(app, &mut out)?;
            let drawn = terminal.draw(|frame| {
                draw_screen(frame, app, Some(stats), Instant::now());
                // The switches are pictures placed over their glyphs, which are hidden first.
                graphics.find_switches(frame.buffer_mut());
            })?;
            let changed = *drawn.buffer != shown;
            if changed {
                shown.clone_from(drawn.buffer);
            }
            graphics.place(app, &mut out)?;
            redraws.drawn(changed || out.wrote);
            shown_song = Arc::clone(app.song());
        }
        woken = false;
        if event::poll(frame_time)? {
            while event::poll(Duration::ZERO)? {
                woken = true;
                match event::read()? {
                    Event::Key(key) => {
                        app.key_event(Key::from_event(key), key.kind, Instant::now())
                    }
                    Event::Mouse(report) => app.mouse_event(report, Instant::now()),
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
    let (stream, buffer) = output.start(realtime, Arc::clone(stats), app.audio.full_speed)?;
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
    // The sample page's waveform has the whole width, as a sample editor's does.
    let (view, meter) = if app.view == View::Sample {
        (body, None)
    } else {
        views::meter::split(body)
    };
    // Only the Mod page's LFO cycles and the Gen page's previews have dots for the terminal to
    // place.
    app.lfo_cycles_at = Default::default();
    app.gen_panels_at = Vec::new();
    match app.view {
        View::Chain => views::chain::draw(frame, view, app),
        View::Phrase => views::phrase::draw(frame, view, app),
        View::Track => {
            let drawn = views::track::draw_finding_cycles(frame, view, app);
            app.lfo_cycles_at = drawn.cycles;
            app.gen_panels_at = drawn.previews;
        }
        View::Mixer => views::mixer::draw(frame, view, app),
        View::Audio => views::audio::draw(frame, view, app),
        View::Project => views::project::draw(frame, view, app),
        View::Sample => views::sample::draw(frame, view, app),
        View::Song => views::song::draw(frame, view, app),
    }
    // Inside the box the track page has while the keyboard plays it.
    let free = if app.mode == Mode::Keys && app.view == View::Track {
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
    if let Some(mapping) = &app.mapping {
        views::mapping::draw(frame, view, app, mapping);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Frames until the next draw, from now.
    fn wait(redraws: &mut Redraws) -> u32 {
        (1..100).find(|_| redraws.due(false)).unwrap()
    }

    #[test]
    fn a_still_screen_is_drawn_now_and_then_and_at_once_when_woken() {
        let mut redraws = Redraws::default();
        for _ in 0..STILL_AFTER {
            assert!(redraws.due(false), "drawn every frame until it's still");
            redraws.drawn(false);
        }
        assert_eq!(wait(&mut redraws), STILL_EVERY, "then now and then");
        redraws.drawn(false);
        assert!(!redraws.due(false));
        assert!(redraws.due(true), "a key draws it at once");
        redraws.drawn(true);
        for _ in 0..STILL_AFTER {
            assert!(redraws.due(false), "and every frame while it changes");
            redraws.drawn(false);
        }
        assert!(!redraws.due(false));
    }
}
