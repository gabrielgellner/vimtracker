//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::overlays::picker::draw::{PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH};
use crate::overlays::picker::exploring::Explorer;
use crate::pages::audio::AudioPage;
use crate::pages::chain::ChainView;
use crate::screen::Readouts;
use crate::services::audio::{AudioChoice, DeviceInfo};
use crate::services::loading::Loader;
use crate::widgets::glyphs;
use vt_engine::engine::{PLAYHEADS, TrackPlace, Transport};
use vt_engine::link::{Command, Handle, JamState};
use vt_model::constants::{DEFAULT_NOTE_VELOCITY, MAX_TRACKS, TICKS_PER_STEP};
use vt_model::names::Names;
use vt_model::samples::SamplePool;
use vt_model::selection::{Field, Scope, StepRange};
use vt_model::song::Song;
use vt_model::step::{FxCommand, MAX_VELOCITY};

use crate::app::grammar::Grammar;
use crate::app::jumps::Jumps;
use crate::app::keys::HeldKeys;
use crate::app::recording::Recording;
use crate::input::bindings::Table;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::typed::TypedValue;
use crate::overlays::Overlays;
use crate::overlays::field_jump::FieldJump;
use crate::pages::mixer::MixerPage;
use crate::pages::phrase::PhraseView;
use crate::pages::project::ProjectPage;
use crate::pages::song::SongView;
use crate::pages::track::TrackPage;
use crate::services::Services;
use crate::services::autosave::Autosave;

pub(crate) mod accent_slide;
mod commands;
mod cursor;
mod entry;
pub(crate) mod grammar;
mod jumps;
mod keys;
mod loops;
pub mod recording;
pub(crate) mod tracks;
pub(crate) mod tree;

/// A run of steps and columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub range: StepRange,
    pub scope: Scope,
    pub tracks: TrackSpan,
}

/// An inclusive run of tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackSpan {
    pub first: usize,
    pub last: usize,
}

impl TrackSpan {
    /// A span covering two tracks, in either order.
    pub fn between(a: usize, b: usize) -> Self {
        Self {
            first: a.min(b),
            last: a.max(b),
        }
    }

    /// How many tracks it covers.
    pub fn count(self) -> usize {
        self.last - self.first + 1
    }

    /// Whether it covers a track.
    pub fn contains(self, track: usize) -> bool {
        (self.first..=self.last).contains(&track)
    }
}

/// How far `c` and `v` move the note velocity in Insert: its top digit.
const NOTE_VELOCITY_STEP: u8 = 0x10;

/// Typed characters not yet written, in Insert mode.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Entry {
    pub text: TypedValue,
    /// A command name waiting for its value, so the command doesn't take effect until then.
    pub command: Option<FxCommand>,
}

/// What spans the pages as keys are typed: the view on screen, the mode the keys are in, what
/// Insert has typed, a field jump under way and the count typed before a command. A page's keys
/// read them.
#[derive(Debug, Clone)]
pub struct Modes {
    pub view: View,
    pub mode: Mode,
    pub entry: Entry,
    /// The search for a setting under way.
    pub field_jump: Option<FieldJump>,
    pub(crate) count: Option<usize>,
}

impl Default for Modes {
    fn default() -> Self {
        Self {
            view: View::Phrase,
            mode: Mode::Normal,
            entry: Entry::default(),
            field_jump: None,
            count: None,
        }
    }
}

/// Where the cursor is: a step in a chain slot on a track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// The section being edited, which needn't be the one playing.
    pub section: usize,
    pub track: usize,
    pub slot: usize,
    pub step: usize,
    pub field: Field,
}

/// A track's live controls.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrackLive {
    pub mute: bool,
    pub solo: bool,
}

/// How long a key has to be down to count as held rather than tapped, for the keys that act
/// differently held: a held mute or solo goes back when it's let go, and a held audition stops.
/// Polyend's Tracker takes a button as held after 333 ms.
pub(crate) const HOLD_TIME: Duration = Duration::from_millis(300);

/// How long the playheads stay without a report from the audio thread, after which audio has
/// stopped and nothing is playing.
const PLAYHEADS_KEPT: Duration = Duration::from_millis(250);

/// What `:w` says to a read-only song in its own folder.
const READ_ONLY: &str = "This song is read-only: :w path saves a copy, :w! writes over it";

/// What a key started that lasts while it's down, and is undone or ended when it's let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Momentary {
    /// A track's mute or solo was switched from this.
    Mute {
        track: usize,
        was: bool,
    },
    Solo {
        track: usize,
        was: bool,
    },
    /// Fill was switched from this.
    Fill {
        was: bool,
    },
    /// A row is sounding on every track.
    Audition,
    /// A row outside the loop is sounding this note through a track on a preview voice.
    AuditionNote {
        track: usize,
        note: u8,
    },
    /// The sample page is playing a slice or from its cursor, its count of what it's played then.
    SampleAudition {
        hearing: u64,
    },
}

/// The audio output as it plays, and what the audio page offers.
#[derive(Debug, Clone, Default)]
pub struct AudioState {
    pub device: String,
    /// The buffer it plays with, in frames, if the device says.
    pub buffer: Option<u32>,
    /// How long from the callback to the sound leaving the device, in microseconds.
    pub latency_us: u64,
    /// Buffers filled too late, and dropouts the device reported, since it started.
    pub late: u64,
    pub xruns: u64,
    /// Whether the main loop should list the devices again, as the page has just opened.
    pub refresh: bool,
    /// Whether it's playing. It stops when the device changes or goes away.
    pub running: bool,
    /// What's chosen, which the system's own choices fill in.
    pub choice: AudioChoice,
    pub devices: Vec<DeviceInfo>,
    /// The inputs the recorder can hear, the one open by name, if one is, and how long from the
    /// sound reaching it to its callback, in microseconds.
    pub(crate) inputs: Vec<DeviceInfo>,
    pub input: Option<String>,
    pub(crate) input_latency_us: u64,
    /// The slowdown check's time at full speed, measured as the program starts, for each stream
    /// it opens.
    pub(crate) full_speed: Duration,
}

/// The whole program's state.
pub struct App {
    /// What every page shares: the song and the cursor, the engine, the samples, the status line.
    pub services: Services,
    /// The view and the mode the keys are in, and what spans the pages as they're typed.
    pub modes: Modes,
    /// The audio output as it plays, and what the audio page chooses from.
    pub audio: AudioState,
    /// A different output the audio page has chosen, for the main loop to open.
    pub audio_request: Option<AudioChoice>,
    /// The audio page: the row its cursor is on.
    pub audio_page: AudioPage,
    /// The mouse wheel as a knob on the cursor's setting, while its setting is on.
    pub wheel: crate::input::wheel::Wheel,
    /// The mixer: the row its cursor is on.
    pub mixer_page: MixerPage,
    /// The song view: the column its cursor is on, and where `V` started selecting sections.
    pub song_view: SongView,
    /// The phrase view: where it's scrolled to, how it's laid out, and the selection on it.
    pub phrase_view: PhraseView,
    /// The track page: which of its pages shows, where its cursor is, and the images it shows.
    pub track_page: TrackPage,
    /// The sample page: where it is, and the pictures of its waveform and its overview.
    pub sample_page: crate::pages::sample::SamplePage,
    /// The project page's Song sub-page: where its cursor is, and the groove it draws.
    pub project_page: ProjectPage,
    /// Most recently used first, starting with the view on screen.
    pub recent_views: Vec<View>,
    /// The chain view: the column its cursor is on, and the row last copied.
    pub chain_view: ChainView,
    /// What's drawn over the page: the picker, the `:` line, the help and the guides, the shape
    /// panel and a macro's mapping view.
    pub overlays: Overlays,
    /// The jump list, which `Ctrl-O`, `Ctrl-I` and `''` go through.
    pub jumps: Jumps,
    /// When the autosave was last due, and what it last wrote.
    pub autosave: Autosave,
    /// Live recording, while it's armed.
    pub(crate) recording: Option<Recording>,
    /// The recorder: what `Space R` records, and the recording going on.
    pub sampling: crate::services::sampling::Sampling,
    /// The CPU and slowdown figures, and the scope's image.
    pub readouts: Readouts,
    pub should_quit: bool,
    /// The keys that are down, and what they hold while they are.
    pub held: HeldKeys,
    pub(crate) table: Table,
    /// The grammar: keys typed so far, the operator and register, and the change `.` repeats.
    pub grammar: Grammar,
    /// The file explorer: the kit in use, the files it reads and plays, its places and tags, and
    /// what a folder chosen in it goes on to.
    pub explorer: Explorer,
    /// When the loader or the analyst started on what they're doing, while they are, for the
    /// spinner on the status line.
    pub(crate) work_since: Option<Instant>,
    /// Makes the random names that name lines start with and `C-r` rolls, seeded from the clock.
    pub(crate) names: Names,
}

/// The keys that play while the track page's keyboard is on, from C to F. The rest stay the
/// page's own.
const KEYS_NOTES: &str = "awsedf";

/// The longest a typed label is: a slot's digit and 4 letters.
const LONGEST_LABEL: usize = 5;
impl App {
    /// Why `:e`, `:new` or `:q` refused unsaved changes, and the ways on: `or` is the forced
    /// command and what it does. A read-only song's `:w` refuses too, so it's offered `:w path`.
    pub(crate) fn unsaved(&self, or: &str) -> String {
        if self.services.live.read_only {
            format!("Unsaved changes to a read-only song: :w path to save a copy, or {or}")
        } else {
            format!("Unsaved changes: :w to save, or {or}")
        }
    }

    /// Start with a song, its project folder if it has one, and a connection to the engine.
    pub fn new(
        song: Song,
        folder: Option<PathBuf>,
        samples: Arc<SamplePool>,
        handle: Handle,
        rate: u32,
    ) -> Self {
        let mut names = Names::from_clock();
        let mut song = song;
        name_song(&mut song, folder.as_deref(), &mut names);
        let mut app = Self {
            services: Services::new(song, folder, samples, handle, rate),
            modes: Modes::default(),
            mixer_page: MixerPage::default(),
            song_view: SongView::default(),
            phrase_view: PhraseView::default(),
            track_page: TrackPage::default(),
            sample_page: crate::pages::sample::SamplePage::default(),
            project_page: ProjectPage::default(),
            audio: AudioState::default(),
            audio_request: None,
            audio_page: AudioPage::default(),
            wheel: crate::input::wheel::Wheel::default(),
            recent_views: View::ALL.to_vec(),
            chain_view: ChainView::default(),
            overlays: Overlays::default(),
            jumps: Jumps::default(),
            autosave: Autosave::default(),
            recording: None,
            sampling: crate::services::sampling::Sampling::default(),
            readouts: Readouts::default(),
            should_quit: false,
            held: HeldKeys::default(),
            table: Table::default(),
            grammar: Grammar::default(),
            explorer: Explorer::new(rate),
            work_since: None,
            names,
        };
        app.services.live_song = Some(Arc::clone(app.song()));
        app.send_live();
        app
    }

    /// Play through a new engine, on another output or at another rate. The song, undo, mutes and
    /// solos stay, and the transport is stopped, as the new engine starts that way.
    pub fn reconnect(&mut self, handle: Handle, samples: Arc<SamplePool>, rate: u32) {
        self.services.handle = handle;
        // The engine recording is gone, and its recording with it.
        self.lose_sampling();
        // An analysis is of the file at its own rate, so it's the same at the new one.
        let mut samples = SamplePool::clone(&samples);
        samples.keep_analyses(&self.services.samples);
        self.services.samples = Arc::new(samples);
        self.services.rate = rate;
        self.explorer.loader = Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT));
        self.explorer.file_playing = None;
        self.services.scope = crate::services::scope::Scope::new(rate);
        self.services.reported.transport = Transport::Stopped;
        self.services.reported.playhead = 0;
        self.services.reported.places = [TrackPlace::default(); MAX_TRACKS];
        self.services.reported.live = None;
        // The new engine starts on the song as it is.
        self.services.sent.clear();
        self.services.live_song = Some(Arc::clone(self.song()));
        self.services.reported.jam = vec![JamState::default(); MAX_TRACKS];
        // Sent again when the next frame finds it differs.
        self.services.watched = usize::MAX;
        self.send_live();
        // The new engine's first version was built before the analyses were kept.
        self.services.publish();
    }

    /// Tell the engine every track's live controls, and the scale `ACU` moves notes through, after
    /// loading a song.
    fn send_live(&mut self) {
        self.services.handle.send(Command::SetScale {
            scale: self.services.live.scale,
        });
        for (track, live) in self.services.live.levels.clone().into_iter().enumerate() {
            self.services.handle.send(Command::SetMute {
                track,
                mute: live.mute,
            });
            self.services.handle.send(Command::SetSolo {
                track,
                solo: live.solo,
            });
        }
    }

    /// The song as saved: with live mute, solo, the scale and the folds written into it.
    pub(crate) fn song_to_save(&self) -> Arc<Song> {
        let mut song = Song::clone(self.song());
        song.scale = self.services.live.scale;
        song.auto_jump = self.services.live.auto_jump;
        song.read_only = self.services.live.read_only;
        song.view = self.services.folds.view();
        // Only a track whose mute or solo differs is written to, as writing to one copies it.
        for (index, live) in self.services.live.levels.iter().enumerate() {
            let differs =
                |track: &vt_model::song::Track| (track.mute, track.solo) != (live.mute, live.solo);
            if song.tracks.get(index).is_some_and(differs) {
                let track = &mut song.tracks[index];
                track.mute = live.mute;
                track.solo = live.solo;
            }
        }
        // While a mute or solo key is down it may yet be a hold, which goes back when it's let go,
        // so what's saved is how it was. A tap marks the song changed again when it's let go.
        if let Some(held) = self.held.momentary {
            match held.what {
                Momentary::Mute { track, was } => {
                    if let Some(track) = song.tracks.get_mut(track) {
                        track.mute = was;
                    }
                }
                Momentary::Solo { track, was } => {
                    if let Some(track) = song.tracks.get_mut(track) {
                        track.solo = was;
                    }
                }
                Momentary::Fill { .. }
                | Momentary::Audition
                | Momentary::AuditionNote { .. }
                | Momentary::SampleAudition { .. } => {}
            }
        }
        Arc::new(song)
    }

    /// The current song, as the services hold it.
    pub fn song(&self) -> &Arc<Song> {
        self.services.song()
    }

    /// How far a track is through this chain, if it's playing it.
    pub fn chain_playhead(&self, track: usize, chain: u32) -> Option<usize> {
        let length = (self.song().tracks.get(track)?.chain_steps(chain)).max(1);
        Some(self.services.playing_place(track, chain)? % length)
    }

    /// Put the playhead at a step of the song's, with each track at the step of its own playing
    /// then, as the engine reports them, for drawing without playing.
    pub fn set_playhead(&mut self, step: usize) {
        self.services.reported.playhead = step;
        let tracks = self.song().tracks.len().min(MAX_TRACKS);
        for track in 0..tracks {
            let speed = self.song().tracks[track].speed;
            let ticks = speed.ticks();
            self.services.reported.places[track] = TrackPlace {
                count: (step as u64 * TICKS_PER_STEP / ticks) as i64,
                progress: 0.0,
                ticks: ticks as u32,
                speed,
                ..TrackPlace::default()
            };
            self.services.reported.places[track].lanes = self.services.placed_lanes(track);
        }
    }

    /// The keys typed so far in a sequence.
    pub fn pending(&self) -> &[Key] {
        &self.grammar.pending
    }

    /// Put the cursor on the first step that plays, at the top of the screen.
    pub(crate) fn go_to_loop_start(&mut self) {
        self.services.go_to_loop_start();
        self.phrase_view.top = self.services.cursor.step;
    }

    /// Do the work due each frame: send waiting versions and read the callback's report.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    /// Do a frame's work as if it's `now`, so the screenshots can step the meters by a fixed time.
    pub fn tick_at(&mut self, now: Instant) {
        self.services.now = now;
        self.services.handle.pump();
        let seconds = now
            .saturating_duration_since(self.services.last_tick)
            .as_secs_f32();
        self.services.last_tick = now;
        let track = self.services.cursor.track;
        if self.services.watched != track && self.services.handle.send(Command::Watch { track }) {
            self.services.watched = track;
        }
        let report = self.services.handle.latest();
        self.services.handle.read_scope(&mut self.services.scope);
        self.services
            .meter
            .update(report.map(|report| report.reading), seconds);
        let tracks = self.song().tracks.len();
        for (index, meter) in self
            .services
            .track_meters
            .iter_mut()
            .take(tracks)
            .enumerate()
        {
            meter.update(report.map(|report| report.tracks[index]), seconds);
        }
        if let Some(report) = report {
            self.services.reported.playhead = report.position;
            self.services.reported.progress = report.progress;
            self.services.reported.places = report.places;
            self.services.reported.report_at = now;
            self.services.reported.playing_section = report.section;
            self.services.reported.transport = report.transport;
            self.services.reported.realigning = report.realigning;
            self.services.reported.loop_waiting = report.loop_waiting;
            self.services.reported.realigning_tracks = report.realigning_tracks;
            self.services.reported.live = (report.watched == track).then_some(report.live);
            self.services.follow_version(report.version);
            self.services.follow_faders(&report.faders, now);
            self.services.reported.playheads = if report.watched == track {
                report.playheads
            } else {
                [None; PLAYHEADS]
            };
        } else if now.saturating_duration_since(self.services.reported.report_at) > PLAYHEADS_KEPT {
            // Audio has stopped, so nothing is playing.
            self.services.reported.playheads = [None; PLAYHEADS];
        }
        let reduction = self
            .services
            .reported
            .live
            .map_or([0.0; vt_model::insert::INSERTS], |live| live.reduction);
        for (held, now) in self.track_page.slot_reduction.iter_mut().zip(reduction) {
            held.update(now, seconds);
        }
        let bpm = f64::from(self.song().millibpm) / 1000.0;
        self.track_page
            .reduction_history
            .update(track, reduction, seconds, bpm);
        // Only what was reported this frame, for this track, so it falls once nothing is.
        let trimmed = report
            .filter(|report| report.watched == track)
            .map_or([0.0; 2], |report| report.live.after_trim);
        for (held, level) in self.track_page.after_trim.iter_mut().zip(trimmed) {
            held.update(crate::services::meter::to_db(level), seconds);
        }
        while let Some((track, pitch)) = self.services.handle.note_pitch() {
            let row = (track, pitch.row.phrase, pitch.row.row);
            self.services.reported.heard_pitches.insert(row, pitch);
        }
        while let Some((track, report)) = self.services.handle.gen_heard() {
            self.hear_generator(track, report);
        }
        while let Some((track, state)) = self.services.handle.jam_change() {
            // A mute or solo waiting for the grid has landed, so the live controls follow it.
            if state.switches_landed != self.services.reported.jam[track].switches_landed
                && let Some(live) = self.services.live.levels.get_mut(track)
            {
                live.mute = state.mute;
                live.solo = state.solo;
                self.services.dirty = true;
            }
            self.services.reported.jam[track] = state;
        }
        self.take_loaded();
        self.take_analyses();
        self.tick_roto();
        self.wheel_frame(now);
        self.follow_recording();
        self.follow_sampling();
        let working = self.explorer.loader.is_waiting() || self.services.analyst.is_waiting();
        self.work_since = working.then(|| self.work_since.unwrap_or(now));
        self.settle_sample_page();
    }

    /// The work going on in the background, by name, and the spinner's turn for it. Only work
    /// that has taken a moment shows, so a file read as the explorer's cursor passes doesn't flash
    /// on the status line, and nothing shows once it's done, so an idle screen stays still.
    pub fn background_work(&self) -> Option<(&'static str, char)> {
        let since = self.work_since?;
        let elapsed = self.services.now.saturating_duration_since(since);
        if elapsed < glyphs::SPINNER_AFTER {
            return None;
        }
        let work = if self.services.analyst.is_waiting() {
            "Analysing"
        } else {
            "Loading"
        };
        Some((work, glyphs::spinner(elapsed)))
    }

    /// The terminal window's title: `vimtracker` and the project's name, then `[RO]` if it's
    /// read-only, as Vim marks a read-only buffer. kitty's
    /// `--when-focus-on title:vimtracker` matches it, so its `Cmd` shortcuts can be left to this
    /// window alone.
    pub fn window_title(&self) -> String {
        let read_only = if self.services.live.read_only {
            " [RO]"
        } else {
            ""
        };
        format!("vimtracker: {}{read_only}", self.services.project_name())
    }
}

/// The grid sizes `Option-1` and `Option-2` go through, in steps: `1/16` up to a bar.
pub(crate) const GRID_SIZES: [u32; 5] = [1, 2, 4, 8, 16];

/// How far to move from a row to reach the next multiple of the skip, so a jump lands on the grid
/// wherever it starts.
fn snap_by(row: isize, skip: usize, forward: bool) -> isize {
    if skip == 0 {
        return 0;
    }
    let into = row.rem_euclid(skip as isize) as usize;
    match (forward, into) {
        (true, _) => (skip - into) as isize,
        (false, 0) => -(skip as isize),
        (false, _) => -(into as isize),
    }
}

/// The note velocity one press up or down from this one, changing only its top digit so that
/// pressing back always returns to the default. `7F` is the one value above that ladder, and going
/// down from it steps back on.
fn next_note_velocity(velocity: u8, up: bool) -> u8 {
    let step = NOTE_VELOCITY_STEP;
    let lowest = DEFAULT_NOTE_VELOCITY % step;
    let highest = MAX_VELOCITY - (MAX_VELOCITY - lowest) % step;
    if up {
        (velocity + step).min(MAX_VELOCITY)
    } else if velocity > highest {
        highest
    } else {
        velocity.saturating_sub(step).max(lowest)
    }
}

/// Whether a delay since a time has passed. Copied from gmuse's `src/ui/mod.rs`.
pub(crate) fn which_key_due(since: Option<Instant>, now: Instant, delay: Duration) -> bool {
    match since {
        Some(started) => now.saturating_duration_since(started) >= delay,
        None => false,
    }
}

/// A folder's own name, which a project saved in it is called.
fn folder_name(folder: &Path) -> String {
    folder
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Give a song with no name its folder's, or a random one before it has a folder.
fn name_song(song: &mut Song, folder: Option<&Path>, names: &mut Names) {
    if song.name.is_empty() {
        song.name = folder.map(folder_name).unwrap_or_default();
    }
    if song.name.is_empty() {
        song.name = names.name();
    }
}
