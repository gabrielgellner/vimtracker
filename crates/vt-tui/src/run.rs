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
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::widgets::Block;
use thiserror::Error;
use vt_engine::engine::{Engine, Transport};
use vt_engine::link::{Realtime, connect};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, render_stems, write_wav};
use vt_model::analysis;
use vt_model::constants::DEFAULT_TRACKS;
use vt_model::modulation::MOD_SLOTS;
use vt_model::project::{self, ProjectError};
use vt_model::samples::SamplePool;
use vt_model::song::Song;

use crate::app::App;
use crate::graphics::{self, Graphics, PanelIds, Placements, SampleLines};
use crate::input::keys::{KEYBOARD_FLAGS, Key};
use crate::input::mode::{Mode, View};
use crate::input::wheel;
use crate::overlays::Overlay as _;
use crate::overlays::command::history::CommandHistory;
use crate::overlays::macro_panel::STRIP_HEIGHT;
use crate::overlays::picker::places::RecentPlaces;
use crate::pages::Page;
use crate::pages::sample::draw::WaveOnScreen;
use crate::services::audio::{self, AudioChoice, AudioError, AudioStats, Input, Output};
use crate::services::config::Settings;
use crate::widgets::theme;

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
pub(crate) fn stems_written(count: usize, folder: &Path) -> String {
    let stems = if count == 1 { "stem" } else { "stems" };
    format!("Rendered {count} {stems} to {}", folder.display())
}

/// Write one file per stem that would be heard, as `01-BD.wav` or `02.wav` for a track and
/// `G1.wav` or `G1-DR.wav` for a group, and return how many.
pub(crate) fn write_stems(
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
    if !graphics::supports_graphics()? {
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
    let (handle, mut realtime) = connect(Engine::new(rate, version));
    let stats = Arc::new(AudioStats::default());
    let device = output.name.clone();
    let full_speed = full_speed.join().unwrap_or(Duration::MAX);
    let heard = hear_input(&choice, rate, &mut realtime, &stats);
    let (stream, buffer) = output.start(realtime, Arc::clone(&stats), full_speed)?;
    let mut stream = Some(Streams {
        output: stream,
        input: heard.stream,
    });
    let mut app = App::new(song, path, samples, handle, rate);
    app.services.use_analysis_cache(analysis::cache_folder());
    app.audio_page.audio = crate::pages::audio::AudioState {
        device,
        buffer,
        running: true,
        choice,
        devices: audio::devices(),
        inputs: audio::input_devices(),
        input: heard.name,
        full_speed,
        ..crate::pages::audio::AudioState::default()
    };
    if let Some(note) = heard.note
        && status.is_empty()
    {
        status = note;
    }
    app.overlays.explorer.library = crate::offline::ableton::default_samples();
    if let Some(path) = Settings::path() {
        let (settings, note) = Settings::load(&path);
        app.services.settings = settings;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    if let Some(path) = CommandHistory::default_path() {
        let (history, note) = CommandHistory::load(&path);
        app.overlays.command.history = history;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    if let Some(folder) = crate::services::config::config_folder() {
        let path = folder.join(vt_model::tag::TAGS_FILE);
        let (library, note) = vt_model::tag::TagLibrary::load(&path);
        app.overlays.explorer.tag_library = std::sync::Arc::new(library);
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    if let Some(path) = RecentPlaces::default_path() {
        let (places, note) = RecentPlaces::load(&path);
        app.overlays.explorer.recent_places = places;
        if let Some(note) = note
            && status.is_empty()
        {
            status = note;
        }
    }
    // Off unless it was turned on here before, so Live keeps the Roto otherwise.
    if let Some(path) = crate::roto::RotoSetting::path() {
        app.services.roto.setting = crate::roto::RotoSetting::load_from(&path);
        app.services.roto.setting_path = Some(path);
    }
    // Its devices with buttons are vimtracker's own setups stored on it, written over its serial
    // port when they're missing or changed, with their versions kept in the cache folder.
    app.services.roto.serial = Some(crate::roto::system_serial());
    app.services.roto.versions_path = vt_model::analysis::program_cache_folder()
        .map(|folder| folder.join(crate::roto::VERSIONS_FILE));
    // Only one vimtracker on this machine has the Roto at a time: the one holding this lock.
    app.services.roto.lock_path = vt_model::analysis::program_cache_folder()
        .map(|folder| folder.join(crate::roto::LOCK_FILE));
    app.services.start_roto_if_on();
    // Off unless it was turned on here before, so the terminal keeps its mouse otherwise.
    if let Some(path) = crate::input::wheel::WheelSetting::path() {
        app.wheel.setting = crate::input::wheel::WheelSetting::load_from(&path);
        app.wheel.setting_path = Some(path);
    }
    // So the value follows the fingers whichever way the system scrolls.
    app.wheel.natural = crate::input::wheel::natural_scrolling();
    app.services.status = status;
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
/// changes, or the song plays. The loop still runs every frame, so nothing else waits.
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

/// Tuning: time between redraws.
pub const FRAME_MS: u64 = 33;

fn main_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    stats: &Arc<AudioStats>,
    graphics: &mut Graphics,
    stream: &mut Option<Streams>,
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
            app.audio_page.audio.running = false;
            app.audio_page.audio.devices = audio::devices();
            app.services.status =
                "The audio device changed or went away, so audio stopped. Space 9 L chooses one"
                    .to_owned();
        }
        app.audio_page.audio.latency_us = stats.latency.load(Ordering::Relaxed);
        app.audio_page.audio.input_latency_us = stats.input_latency.load(Ordering::Relaxed);
        app.audio_page.audio.late = stats.late.load(Ordering::Relaxed);
        app.audio_page.audio.xruns = stats.errors.load(Ordering::Relaxed);
        if std::mem::take(&mut app.audio_page.audio.refresh) {
            app.audio_page.audio.devices = audio::devices();
            app.audio_page.audio.inputs = audio::input_devices();
        }
        if let Some(choice) = app.audio_page.request.take() {
            // The old stream lets go of its device first.
            *stream = None;
            match switch_audio(app, &choice, stats) {
                Ok(playing) => *stream = Some(playing),
                Err(error) => {
                    app.audio_page.audio.running = false;
                    app.services.status = format!("Couldn't play through that: {error}");
                }
            }
        }
        app.tick();
        app.readouts
            .cpu
            .sample(stats.load.load(Ordering::Relaxed), Instant::now());
        app.readouts
            .slowdown
            .sample(stats.slowdown.load(Ordering::Relaxed), Instant::now());
        if power_asked.is_none_or(|asked| asked.elapsed() >= POWER_EVERY) {
            power_asked = Some(Instant::now());
            app.readouts.slowdown.power = audio::PowerState::now();
        }
        app.autosave_if_due(Instant::now());
        woken |= app.services.reported.transport == Transport::Playing
            || !Arc::ptr_eq(app.song(), &shown_song);
        if redraws.due(woken) {
            // The sample page's waveform is drawn at one pixel for each of the screen's.
            if let Ok(size) = terminal::window_size() {
                graphics.use_window(size);
                // So an up tick of the wheel counts for as much of the fingers' way as sideways ones.
                app.wheel.cell = Some(graphics.cell());
            }
            let mut out = Noted {
                inner: std::io::stdout(),
                wrote: false,
            };
            app.send_pictures(graphics, &mut out)?;
            let drawn = terminal.draw(|frame| {
                draw_screen(frame, app, Some(stats), Instant::now());
                // The switches are pictures placed over their glyphs, which are hidden first.
                graphics.find_switches(frame.buffer_mut());
            })?;
            let changed = *drawn.buffer != shown;
            if changed {
                shown.clone_from(drawn.buffer);
            }
            app.place_pictures(graphics, &mut out)?;
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
) -> Result<Streams, AudioError> {
    let output = Output::open(choice)?;
    let rate = output.rate();
    let mut pool = SamplePool::default();
    let errors = project::load_samples(app.services.folder.as_deref(), app.song(), rate, &mut pool);
    let samples = Arc::new(pool);
    let version = Arc::new(Published::new(Arc::clone(app.song()), Arc::clone(&samples)));
    let (handle, mut realtime) = connect(Engine::new(rate, version));
    let device = output.name.clone();
    let note = output.note.clone();
    let heard = hear_input(choice, rate, &mut realtime, stats);
    let note = note.or(heard.note);
    let (stream, buffer) =
        output.start(realtime, Arc::clone(stats), app.audio_page.audio.full_speed)?;
    app.reconnect(handle, samples, rate);
    app.audio_page.audio.input = heard.name;
    app.audio_page.audio.inputs = audio::input_devices();
    app.audio_page.audio.device = device;
    app.audio_page.audio.buffer = buffer;
    app.audio_page.audio.running = true;
    app.audio_page.audio.choice = choice.clone();
    app.audio_page.audio.devices = audio::devices();
    app.services.status = match (note, errors.first()) {
        (Some(note), _) => note,
        (None, Some(error)) => error.to_string(),
        (None, None) => format!(
            "Playing through {} at {rate} Hz",
            app.audio_page.audio.device
        ),
    };
    if let Err(error) = choice.save() {
        app.services.status = format!("Playing, but the choice couldn't be kept: {error}");
    }
    Ok(Streams {
        output: stream,
        input: heard.stream,
    })
}

/// The output's stream, and the input's if one is open. Each stops when it's dropped.
struct Streams {
    #[allow(dead_code, reason = "held so it keeps playing")]
    output: cpal::Stream,
    #[allow(dead_code, reason = "held so it keeps listening")]
    input: Option<cpal::Stream>,
}

/// The input chosen, if one is and it opens: its stream, its name, and what's to be said if it
/// didn't open.
struct Heard {
    stream: Option<cpal::Stream>,
    name: Option<String>,
    note: Option<String>,
}

/// Open the input chosen at the output's rate, and have the callback hear it, so the recorder can
/// record it. None is opened unless one's chosen.
fn hear_input(
    choice: &AudioChoice,
    rate: u32,
    realtime: &mut Realtime,
    stats: &Arc<AudioStats>,
) -> Heard {
    let Some(id) = &choice.input else {
        return Heard {
            stream: None,
            name: None,
            note: None,
        };
    };
    let opened = Input::open(id, rate).and_then(|input| {
        let name = input.name.clone();
        let (into, heard) = rtrb::RingBuffer::new(vt_engine::link::INPUT_QUEUE);
        let stream = input.start(into, Arc::clone(stats))?;
        realtime.listen(heard);
        Ok((stream, name))
    });
    match opened {
        Ok((stream, name)) => Heard {
            stream: Some(stream),
            name: Some(name),
            note: None,
        },
        Err(error) => Heard {
            stream: None,
            name: None,
            note: Some(format!("The input isn't heard: {error}")),
        },
    }
}

impl App {
    /// Send the pictures the screen is about to show, if the terminal lacks them, and give each
    /// page and overlay the ids of its own: the track page's panels and the mapping view's
    /// response, the sample page's waveform and overview, the picker's preview and the scope.
    /// Before ratatui draws, as the drawing puts their placeholders down.
    pub fn send_pictures(
        &mut self,
        graphics: &mut Graphics,
        out: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        graphics.begin(out)?;
        let rate = self.services.rate;
        if self.modes.view == View::Track && self.services.track_here().is_some() {
            let wants = self.track_page.panels(&self.ctx());
            let ids = graphics.panels(&wants, rate, out)?;
            self.track_page.take_panels(ids, &wants, rate);
            if let (Some(view), Some(track)) =
                (self.overlays.mapping.as_mut(), self.services.track_here())
            {
                view.images = graphics.panels(&view.panels(track), rate, out)?;
            }
        }
        if self.modes.view == View::Sample {
            // So `h` and `l` move the cursor's line by a pixel of these.
            self.sample_page.cell_width = Some(graphics.cell().0);
            let wants = self.sample_page.panels(&self.ctx());
            let ids = graphics.sample_panels(&wants, rate, out)?;
            self.sample_page.take_panels(ids, &wants, rate);
        } else {
            self.sample_page.images = PanelIds::default();
        }
        let shown = self
            .overlays
            .picker
            .as_ref()
            .and_then(|picker| picker.preview.clone());
        let preview = graphics.preview(shown, out)?;
        if let (Some(picker), Some((ids, wave))) = (self.overlays.picker.as_mut(), preview) {
            picker.preview_images.clone_from(ids);
            picker.preview_wave = *wave;
        }
        if self.readouts.scope_shown
            && let Some(id) = graphics.scope(&self.services.scope, out)?
        {
            self.readouts.scope_image = Some(id);
        }
        graphics.finish(out)
    }

    /// Place what the terminal places itself, once the screen is drawn: the dots on the track
    /// page's pictures, and the sample page's ruler marks, playheads and cursor as lines, each
    /// only while nothing is drawn over the page.
    pub fn place_pictures(
        &self,
        graphics: &mut Graphics,
        out: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        let now = Instant::now();
        let dots = if self.modes.view == View::Track {
            self.track_page.dots(&self.ctx(), graphics.cell(), now)
        } else {
            [None; MOD_SLOTS]
        };
        let page = &self.sample_page;
        let ctx = self.ctx();
        let sample = SampleLines {
            page,
            marks: crate::pages::sample::draw::marks_placed(page, &ctx, now),
            playheads: crate::pages::sample::draw::lines_placed(page, &ctx, now),
            cursor: self.modes.view == View::Sample
                && page.cursor_line
                && page.wave_images().is_some()
                && !ctx.overlaid(now),
            // With the focus on the settings rows, `h` and `l` go along them, so the line is set
            // back to show it isn't what moves.
            dimmed: page.setting.is_some() && self.modes.mode != Mode::Keys,
        };
        graphics.place(&Placements { dots, sample }, out)
    }
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
    // The macro panel, while it shows, takes its rows from the page, above the status line.
    let strip = if app.overlays.macro_panel.open {
        STRIP_HEIGHT
    } else {
        0
    };
    // The line describing the setting under the cursor, while it's on and there's one, takes its
    // rows from the page too, above the macro panel.
    let described = crate::screen::describe::rows(app);
    let [body, describing, panel, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(described),
        Constraint::Length(strip),
        Constraint::Length(2),
    ])
    .areas(frame.area());
    app.follow_cursor(usize::from(
        body.height
            .saturating_sub(crate::pages::phrase::draw::ROWS_AROUND_STEPS),
    ));
    // The sample page's waveform has the whole width, as a sample editor's does.
    let (view, meter) = if app.modes.view == View::Sample {
        (body, None)
    } else {
        crate::screen::meter::split(body)
    };
    // Only the Mod page's LFO cycles and the Gen page's previews have dots for the terminal to
    // place.
    app.track_page.lfo_cycles_at = Default::default();
    app.track_page.gen_panels_at = Vec::new();
    // Laid out for the room first, so what it shows settles in the columns it's drawn with.
    if app.modes.view == View::Sample {
        app.sample_page.lay_out(view);
    }
    match draw_view_noting(frame, view, app, app.modes.view) {
        PageNotes::Nothing => {}
        PageNotes::Track(drawn) => {
            app.track_page.lfo_cycles_at = drawn.cycles;
            app.track_page.gen_panels_at = drawn.previews;
        }
        PageNotes::Sample(on_screen) => app.sample_page.on_screen = on_screen,
    }
    // Inside the box the track page has while the keyboard plays it.
    let free = if app.modes.mode == Mode::Keys && app.modes.view == View::Track {
        view.inner(Margin::new(1, 1))
    } else {
        view
    };
    crate::screen::system::draw(frame, free, app, stats);
    app.readouts.scope_shown = false;
    if let Some(column) = meter {
        let (scope, meter) = crate::screen::meter::split_scope(column);
        app.readouts.scope_shown = scope.is_some();
        if let (Some(area), Some(id)) = (scope, app.readouts.scope_image) {
            crate::screen::meter::draw_scope(frame, area, id);
        }
        crate::screen::meter::draw(frame, meter, &app.services.meter, app.song().limiter);
    }
    crate::screen::status::draw(frame, status, app);
    crate::screen::describe::draw(frame, describing, app);
    if app.overlays.macro_panel.open {
        let mut macros = std::mem::take(&mut app.overlays.macro_panel);
        macros.draw(frame, panel, &app.ctx());
        app.overlays.macro_panel = macros;
    }
    // An overlay may keep something of how it was drawn, so each is lent apart from the rest.
    // A guide keeps how many of its rows fit, which its keys go by, open or not.
    let mut guide = app.overlays.guide;
    guide.draw(frame, body, &app.ctx());
    app.overlays.guide = guide;
    if let Some(mut panel) = app.overlays.shape.take() {
        panel.draw(frame, view, &app.ctx());
        app.overlays.shape = Some(panel);
    }
    if let Some(mut mapping) = app.overlays.mapping.take() {
        mapping.draw(frame, view, &app.ctx());
        app.overlays.mapping = Some(mapping);
    }
    if let Some(mut picker) = app.overlays.picker.take() {
        picker.draw(frame, body, &app.ctx());
        app.overlays.picker = Some(picker);
    }
    // Over the picker, which `?` opens it from.
    if app.modes.mode == Mode::Help {
        let mut help = std::mem::take(&mut app.overlays.help);
        help.draw(frame, body, &app.ctx());
        app.overlays.help = help;
    }
    if app.modes.mode == Mode::Command {
        let mut command = std::mem::take(&mut app.overlays.command);
        command.draw(frame, status, &app.ctx());
        app.overlays.command = command;
    }
    // Over everything, as what follows a key can be asked for in an overlay too.
    crate::overlays::help::draw_popup(frame, body, app, now);
}

/// What a view's drawing notes for after the frame, for [`draw_screen`] to keep.
enum PageNotes {
    Nothing,
    /// Where the track page drew the pictures that carry a dot.
    Track(crate::pages::track::draw::Drawn),
    /// Where the sample page's waveform is on screen, if it's drawn.
    Sample(Option<WaveOnScreen>),
}

/// Draw a view's page in an area as the screen draws it, without the meter, the status line or
/// anything drawn over it, as the tests draw one page.
pub fn draw_view(frame: &mut ratatui::Frame, area: Rect, app: &App, view: View) {
    draw_view_noting(frame, area, app, view);
}

/// Draw a view's page, giving back what its drawing notes for after the frame.
fn draw_view_noting(frame: &mut ratatui::Frame, area: Rect, app: &App, view: View) -> PageNotes {
    let ctx = app.ctx();
    match view {
        View::Chain => app.chain_view.draw(frame, area, &ctx),
        View::Phrase => app.phrase_view.draw(frame, area, &ctx),
        View::Track => return PageNotes::Track(app.track_page.draw(frame, area, &ctx)),
        View::Mixer => app.mixer_page.draw(frame, area, &ctx),
        View::Audio => app.audio_page.draw(frame, area, &ctx),
        View::Project => app.project_page.draw(frame, area, &ctx),
        View::Sample => return PageNotes::Sample(app.sample_page.draw(frame, area, &ctx)),
        View::Song => app.song_view.draw(frame, area, &ctx),
    }
    PageNotes::Nothing
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
