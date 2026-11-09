//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accent_slide::{HeldFlag, Written, slid};
use crate::audio::{AudioChoice, DeviceInfo, LoadReadout};
use crate::config::Settings;
use crate::explorer::Purpose;
use crate::exploring::Naming;
use crate::loading::Loader;
use crate::meter::Meter;
use crate::places::RecentPlaces;
use crate::views::picker::{PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH};
use ratatui::crossterm::event::{KeyCode, KeyEventKind};
use vt_engine::engine::{TrackPlace, Transport};
use vt_engine::link::{Command, Handle, JamState};
use vt_engine::published::Published;
use vt_engine::render::{RenderOptions, render_loop, write_wav};
use vt_engine::track_state::LiveValues;
use vt_model::accumulator::{MOST_DEGREES, amount};
use vt_model::action::{Action, FieldValue, edit};
use vt_model::condition::Condition;
use vt_model::constants::{
    DEFAULT_NOTE_VELOCITY, DEFAULT_TRACKS, FIXED_TRACKS, MAX_PHRASE_STEPS, MAX_TRACKS,
    TICKS_PER_STEP, TRACK_ENVELOPES,
};
use vt_model::groove::GrooveId;
use vt_model::history::History;
use vt_model::kit::KitEntry;
use vt_model::lfo::LFOS_PER_TRACK;
use vt_model::names::Names;
use vt_model::order::Dealer;
use vt_model::project::{self, ProjectError};
use vt_model::sample::load_sample;
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
pub use vt_model::selection::Field;
use vt_model::selection::{Block, Extent, Motion, Scope, StepRange};
use vt_model::song::{ChainEntry, Lane, Phrase, Section, Song, Track, TrackKind};
use vt_model::step::{
    FxCommand, FxSlot, Gate, GateKind, MAX_NOTE, MAX_NUDGE, MAX_PULSES, MAX_VELOCITY, MIN_NOTE,
    Mark, Note, Step,
};

use crate::grammar::Change;
use crate::guides::Guide;
use crate::input::bindings::{Cmd, Op, Resolved, Table, degree_key, note_key};
use crate::input::command::Line;
use crate::input::command_history::CommandHistory;
use crate::input::completion::CommandMenu;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::Picker;
use crate::jumping::FieldJump;
use crate::recording::Recording;
use crate::shaping::ShapePanel;
use crate::track_page::{Page, TakenInsert};
use crate::tree::{ChainColumn, Place};

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
    pub text: String,
    /// A command name waiting for its value, so the command doesn't take effect until then.
    pub command: Option<FxCommand>,
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
pub const HOLD_TIME: Duration = Duration::from_millis(300);

/// What a key started that lasts while it's down, and is undone or ended when it's let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Momentary {
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
}

/// A key holding a note, on the keyboard or in Insert, and the note it started.
#[derive(Debug, Clone, Copy)]
struct HeldNote {
    key: KeyCode,
    track: usize,
    note: u8,
}

/// A momentary change, the key it's held by, and when that key went down.
#[derive(Debug, Clone, Copy)]
struct Held {
    key: KeyCode,
    since: Instant,
    what: Momentary,
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
}

/// The whole program's state.
pub struct App {
    pub history: History,
    pub folder: Option<PathBuf>,
    pub dirty: bool,
    pub samples: Arc<SamplePool>,
    pub handle: Handle,
    pub rate: u32,
    /// The audio output as it plays, and what the audio page chooses from.
    pub audio: AudioState,
    /// A different output the audio page has chosen, for the main loop to open.
    pub audio_request: Option<AudioChoice>,
    /// The audio page's row: device, rate or buffer.
    pub audio_row: usize,
    /// Where this machine keeps its kits and samples.
    pub settings: Settings,
    pub mode: Mode,
    pub view: View,
    /// The track page's row and setting.
    pub mixer_row: usize,
    pub song_column: usize,
    /// The section the engine is on.
    pub playing_section: usize,
    /// A section being kept playing, set with `:loop`.
    pub looping: Option<usize>,
    /// What each track is launched onto and cued to, as the engine last said.
    pub jam: Vec<JamState>,
    pub track_page: Page,
    /// The ids of the envelopes' images, when they're drawn with kitty's graphics protocol.
    pub envelope_images: Option<[u32; TRACK_ENVELOPES]>,
    /// The ids of the images of the LFOs' shapes, likewise.
    pub lfo_images: Option<[u32; LFOS_PER_TRACK]>,
    /// The id of the image of the filter's curve, likewise.
    pub filter_image: Option<u32>,
    /// The id of the image of the waveform of the sample the cursor's track plays, if it has one.
    pub waveform_image: Option<u32>,
    /// The ids of the images of the FX page's slots, for those that have a drawing.
    pub slot_images: [Option<u32>; vt_model::insert::INSERTS],
    /// What the cursor's track is playing with right now, and the track the engine reports.
    pub live: Option<LiveValues>,
    watched: usize,
    pub track_row: usize,
    pub track_field: usize,
    /// Where `j` and `k` last left the cursor, as its row and field, and the field it was on before
    /// a shorter row moved it in. Moving up or down from there goes back to that field where a row
    /// has it, as Vim keeps its column.
    pub(crate) track_wanted: Option<(usize, usize, usize)>,
    /// Whether the cursor is on the fine digits of the track page's setting.
    pub track_fine: bool,
    /// The project page's row and setting, whether the cursor is on the tempo's fine part, and
    /// the field `j` and `k` aim for, as on the track page.
    pub project_row: usize,
    pub project_field: usize,
    pub project_fine: bool,
    pub(crate) project_wanted: Option<(usize, usize, usize)>,
    /// The groove the project page draws and changes, or the project's if none was chosen.
    pub groove_shown: Option<GrooveId>,
    /// Most recently used first, starting with the view on screen.
    pub recent_views: Vec<View>,
    pub picker: Option<Picker>,
    pub chain_column: ChainColumn,
    pub(crate) yanked_row: Option<ChainEntry>,
    pub(crate) jumps: Vec<Place>,
    pub(crate) jump_index: usize,
    pub(crate) last_autosave: Option<Instant>,
    pub(crate) autosaved: Option<Arc<Song>>,
    pub cursor: Cursor,
    pub octave: u8,
    /// The velocity new notes are written with, like the octave.
    pub note_velocity: u8,
    /// The field a replace started on, since typing an FX command moves on to its value.
    pub(crate) replacing: Option<Field>,
    pub command_line: String,
    /// The list of matches above the command line, while there's more than one.
    pub command_menu: Option<CommandMenu>,
    /// The command lines run before, which `Up` and `Down` go through.
    pub command_history: CommandHistory,
    pub status: String,
    pub transport: Transport,
    pub playhead: usize,
    /// How far through the playhead's step the engine last said it was, and when that was read.
    pub progress: f32,
    /// Where each track is heard, as its own steps count, which a track at another speed than the
    /// song's needs.
    pub places: [TrackPlace; MAX_TRACKS],
    pub(crate) report_at: Instant,
    /// Live recording, while it's armed.
    pub(crate) recording: Option<Recording>,
    /// The main output's level.
    pub meter: Meter,
    /// The main output's recent sound, for the scope above the meter.
    pub scope: crate::scope::Scope,
    /// The id of the scope's image, and whether the last screen drawn had room for it.
    pub scope_image: Option<u32>,
    pub scope_shown: bool,
    /// Each track's level, for the mixer.
    pub track_meters: Vec<Meter>,
    /// The CPU figure on the status line.
    pub cpu: LoadReadout,
    last_tick: Instant,
    pub should_quit: bool,
    /// Live mute and solo per track, kept outside undo and written to the song on save.
    pub levels: Vec<TrackLive>,
    /// The scale, likewise, as it's for entering notes and isn't part of what's undone.
    pub scale: Scale,
    /// Whether entering in Insert moves on by the grid, likewise.
    pub auto_jump: bool,
    /// Whether fill is on, for `CND`'s `FILL` and `!FILL`. It's a live control, never saved.
    pub fill: bool,
    /// The key last pressed, as it is on the keyboard, until it's let go.
    key_down: Option<KeyCode>,
    /// Whether the key that's down ignores its repeats, as a transport key and a held note do.
    pub(crate) repeats_ignored: bool,
    /// The keys holding notes, on the keyboard or in Insert, in the order they went down, so
    /// they play a chord.
    held_notes: Vec<HeldNote>,
    /// The keys holding an accent or slide in Insert, for the notes played while they're down.
    pub(crate) held_flags: Vec<HeldFlag>,
    /// Where the last note played in Insert went, for an accent or slide tapped after it.
    pub(crate) last_written: Option<Written>,
    /// What the key that's down started that lasts while it's held.
    momentary: Option<Held>,
    /// What the press being handled started, for [`App::press`] to tie to its key.
    started: Option<Momentary>,
    pub entry: Entry,
    /// The first step on screen, and how many fit.
    pub top: usize,
    pub view_height: usize,
    pub help_scroll: usize,
    /// The open guide, how far it has scrolled, and how many of its rows fit.
    pub guide: Option<Guide>,
    pub guide_scroll: usize,
    pub guide_page: usize,
    pub guide_width: usize,
    pub(crate) table: Table,
    pub(crate) pending: Vec<Key>,
    pub(crate) pending_since: Option<Instant>,
    pub(crate) count: Option<usize>,
    pub(crate) operator: Option<Op>,
    pub(crate) operator_count: Option<usize>,
    pub(crate) register: Option<char>,
    pub(crate) awaiting_register: bool,
    pub(crate) registers: HashMap<char, Block>,
    pub(crate) anchor: Option<(usize, Scope)>,
    /// How far a selection reaches across tracks from the cursor's.
    pub(crate) anchor_track: Option<usize>,
    /// The open shape panel.
    pub shape: Option<ShapePanel>,
    /// The search for a setting under way.
    pub field_jump: Option<FieldJump>,
    /// An effect taken out of a slot with `dd`, for `p` to put somewhere.
    pub(crate) taken_insert: Option<TakenInsert>,
    /// Changes each time random values are wanted anew.
    pub(crate) shape_seed: u64,
    /// The sounds of the kit in use, to choose from for a track.
    pub kit: Vec<KitEntry>,
    /// The file of the kit in use, which the kit picker opens on.
    pub kit_file: Option<PathBuf>,
    /// Reads the files the explorer previews and plays, off this thread.
    pub loader: Loader,
    /// Whether the explorer plays the sample its cursor moves onto, which `A` switches and the
    /// next one opened keeps.
    pub audition_files: bool,
    /// The sample the explorer's cursor is on, to play once it's read, and the one playing.
    pub file_wanted: Option<PathBuf>,
    pub file_playing: Option<PathBuf>,
    /// The folders the explorer was last used in.
    pub recent_places: RecentPlaces,
    /// The command line a folder chosen in the explorer goes on to.
    pub naming: Option<Naming>,
    /// Makes the random names that name lines start with and `C-r` rolls, seeded from the clock.
    pub(crate) names: Names,
    /// The selection a replace will fill, and the undo depth from before it started.
    pub(crate) replacing_selection: Option<(Selection, usize)>,
    pub(crate) selection_scope: Option<Scope>,
    pub(crate) last_selection: Option<Selection>,
    pub(crate) last_change: Option<Change>,
}

/// The keys that play while the track page's keyboard is on, from C to F. The rest stay the
/// page's own.
const KEYS_NOTES: &str = "awsedf";

/// The longest a typed label is: a slot's digit and 4 letters.
const LONGEST_LABEL: usize = 5;
impl App {
    /// Start with a song, its project folder if it has one, and a connection to the engine.
    pub fn new(
        song: Song,
        folder: Option<PathBuf>,
        samples: Arc<SamplePool>,
        handle: Handle,
        rate: u32,
    ) -> Self {
        let levels = live_from(&song);
        let scale = song.scale;
        let auto_jump = song.auto_jump;
        let mut names = Names::from_clock();
        let mut song = song;
        name_song(&mut song, folder.as_deref(), &mut names);
        let mut app = Self {
            history: History::new(Arc::new(song)),
            folder,
            dirty: false,
            samples,
            handle,
            rate,
            mode: Mode::Normal,
            view: View::Phrase,
            mixer_row: 0,
            song_column: 0,
            playing_section: 0,
            looping: None,
            jam: vec![JamState::default(); MAX_TRACKS],
            track_page: Page::Sound,
            envelope_images: None,
            lfo_images: None,
            filter_image: None,
            waveform_image: None,
            slot_images: [None; vt_model::insert::INSERTS],
            live: None,
            watched: 0,
            track_row: 0,
            track_field: 0,
            track_wanted: None,
            project_row: 0,
            project_field: 0,
            project_fine: false,
            project_wanted: None,
            groove_shown: None,
            audio: AudioState::default(),
            audio_request: None,
            settings: Settings::none(),
            audio_row: 0,
            track_fine: false,
            recent_views: View::ALL.to_vec(),
            picker: None,
            chain_column: ChainColumn::Phrase,
            yanked_row: None,
            jumps: Vec::new(),
            jump_index: 0,
            last_autosave: None,
            autosaved: None,
            cursor: Cursor {
                section: 0,
                track: 0,
                slot: 0,
                step: 0,
                field: Field::Note,
            },
            octave: 4,
            note_velocity: DEFAULT_NOTE_VELOCITY,
            replacing: None,
            command_line: String::new(),
            command_menu: None,
            command_history: CommandHistory::default(),
            status: String::new(),
            transport: Transport::Stopped,
            playhead: 0,
            progress: 0.0,
            places: [TrackPlace::default(); MAX_TRACKS],
            report_at: Instant::now(),
            recording: None,
            meter: Meter::default(),
            scope: crate::scope::Scope::new(rate),
            scope_image: None,
            scope_shown: true,
            track_meters: vec![Meter::default(); MAX_TRACKS],
            cpu: LoadReadout::default(),
            last_tick: Instant::now(),
            should_quit: false,
            levels,
            scale,
            auto_jump,
            fill: false,
            key_down: None,
            repeats_ignored: false,
            held_notes: Vec::new(),
            held_flags: Vec::new(),
            last_written: None,
            momentary: None,
            started: None,
            entry: Entry::default(),
            top: 0,
            view_height: 16,
            help_scroll: 0,
            guide: None,
            guide_scroll: 0,
            guide_page: 16,
            guide_width: 76,
            table: Table::default(),
            pending: Vec::new(),
            pending_since: None,
            count: None,
            operator: None,
            operator_count: None,
            register: None,
            awaiting_register: false,
            registers: HashMap::new(),
            anchor: None,
            anchor_track: None,
            shape: None,
            field_jump: None,
            taken_insert: None,
            shape_seed: 0,
            kit: Vec::new(),
            kit_file: None,
            loader: Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT)),
            audition_files: true,
            file_wanted: None,
            file_playing: None,
            recent_places: RecentPlaces::default(),
            naming: None,
            names,
            replacing_selection: None,
            selection_scope: None,
            last_selection: None,
            last_change: None,
        };
        app.send_live();
        app
    }

    /// Play through a new engine, on another output or at another rate. The song, undo, mutes and
    /// solos stay, and the transport is stopped, as the new engine starts that way.
    pub fn reconnect(&mut self, handle: Handle, samples: Arc<SamplePool>, rate: u32) {
        self.handle = handle;
        self.samples = samples;
        self.rate = rate;
        self.loader = Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT));
        self.file_playing = None;
        self.scope = crate::scope::Scope::new(rate);
        self.transport = Transport::Stopped;
        self.playhead = 0;
        self.places = [TrackPlace::default(); MAX_TRACKS];
        self.live = None;
        self.jam = vec![JamState::default(); MAX_TRACKS];
        // Sent again when the next frame finds it differs.
        self.watched = usize::MAX;
        self.send_live();
    }

    /// Tell the engine every track's live controls, and the scale `ACU` moves notes through, after
    /// loading a song.
    fn send_live(&mut self) {
        self.handle.send(Command::SetScale { scale: self.scale });
        for (track, live) in self.levels.clone().into_iter().enumerate() {
            self.handle.send(Command::SetMute {
                track,
                mute: live.mute,
            });
            self.handle.send(Command::SetSolo {
                track,
                solo: live.solo,
            });
        }
    }

    /// The song as saved: with live mute, solo and the scale written into it.
    pub(crate) fn song_to_save(&self) -> Arc<Song> {
        let mut song = Song::clone(self.song());
        song.scale = self.scale;
        song.auto_jump = self.auto_jump;
        for (track, live) in song.tracks.iter_mut().zip(&self.levels) {
            track.mute = live.mute;
            track.solo = live.solo;
        }
        // While a mute or solo key is down it may yet be a hold, which goes back when it's let go,
        // so what's saved is how it was. A tap marks the song changed again when it's let go.
        if let Some(held) = self.momentary {
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
                Momentary::Fill { .. } | Momentary::Audition | Momentary::AuditionNote { .. } => {}
            }
        }
        Arc::new(song)
    }

    /// The current song.
    pub fn song(&self) -> &Arc<Song> {
        self.history.current()
    }

    /// The section being edited.
    pub fn section_here(&self) -> Option<&Section> {
        let sections = &self.song().sections;
        sections.get(self.cursor.section).or(sections.last())
    }

    /// How far a track is through this chain, if it's playing it.
    pub fn chain_playhead(&self, track: usize, chain: u32) -> Option<usize> {
        let length = (self.song().tracks.get(track)?.chain_steps(chain)).max(1);
        Some(self.playing_place(track, chain)? % length)
    }

    /// How far a track is through this chain, if it's playing it, counting every time round it
    /// since it started, as the engine does to deal a dealt order, for finding the row playing.
    pub fn playing_place(&self, track: usize, chain: u32) -> Option<usize> {
        let moving = matches!(self.transport, Transport::Playing | Transport::Paused);
        let jam = self.jam.get(track)?;
        if !moving || jam.chain != Some(chain) {
            return None;
        }
        let length = (self.song().tracks.get(track)?.chain_steps(chain) as i64).max(1);
        let place = self.places.get(track)?.count - jam.started;
        let laps = place.div_euclid(length).max(0);
        Some((laps * length + place.rem_euclid(length)) as usize)
    }

    /// What a track's dealt orders are dealt from, as the engine deals them.
    pub fn dealer(&self, track: usize) -> Dealer {
        Dealer::new(self.song().seed, track)
    }

    /// Put the playhead at a step of the song's, with each track at the step of its own playing
    /// then, as the engine reports them, for drawing without playing.
    pub fn set_playhead(&mut self, step: usize) {
        self.playhead = step;
        let tracks = self.song().tracks.len().min(MAX_TRACKS);
        for track in 0..tracks {
            let ticks = self.song().tracks[track].speed.ticks();
            self.places[track] = TrackPlace {
                count: (step as u64 * TICKS_PER_STEP / ticks) as i64,
                progress: 0.0,
                ticks: ticks as u32,
                skipped: false,
                accumulated: None,
            };
        }
    }

    /// The step of its own that one track is at when another is at this one of its own, at their
    /// own speeds: the step playing then.
    pub fn step_on(&self, track: usize, from: usize, step: usize) -> usize {
        let ticks = |index: usize| {
            self.song()
                .tracks
                .get(index)
                .map_or(TICKS_PER_STEP, |track| track.speed.ticks())
        };
        (step as u64 * ticks(from) / ticks(track)) as usize
    }

    /// The track under the cursor.
    pub fn track_here(&self) -> Option<&Track> {
        self.song().tracks.get(self.cursor.track)
    }

    /// The phrase under the cursor.
    pub fn phrase_id(&self) -> Option<u32> {
        let chain = (*self.section_here()?.chains.get(self.cursor.track)?)?;
        Some(
            self.track_here()?
                .chains
                .get(&chain)?
                .entries
                .get(self.cursor.slot)?
                .phrase,
        )
    }

    /// The contents of the phrase under the cursor.
    pub fn phrase_here(&self) -> Option<&Arc<Phrase>> {
        self.track_here()?.phrases.get(&self.phrase_id()?)
    }

    /// The keys typed so far in a sequence.
    pub fn pending(&self) -> &[Key] {
        &self.pending
    }

    /// What `gg` and `G` go by: the cursor's FX column's lane if it has one, and the phrase's loop
    /// otherwise. Beats always go by the phrase's loop.
    pub(crate) fn motion_extent(&self, motion: Motion) -> Extent {
        let extent = self.extent();
        let lane = match (motion, self.cursor.field) {
            (
                Motion::First | Motion::Last,
                Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot),
            ) => self.phrase_here().and_then(|phrase| phrase.lane(slot)),
            _ => None,
        };
        lane.map_or(extent, |lane| Extent {
            start: lane.start,
            length: lane.length,
            ..extent
        })
    }

    /// Where the cursor's phrase's loop sits.
    pub fn extent(&self) -> Extent {
        self.phrase_here()
            .map_or(Extent::whole(0), |phrase| phrase.extent())
    }

    /// The position in the section where the cursor's phrase starts playing.
    pub fn slot_start(&self) -> usize {
        let Some(track) = self.track_here() else {
            return 0;
        };
        let Some(chain) = self
            .section_here()
            .and_then(|s| s.chains.get(self.cursor.track).copied().flatten())
            .and_then(|id| track.chains.get(&id))
        else {
            return 0;
        };
        chain
            .entries
            .iter()
            .take(self.cursor.slot)
            .filter_map(|e| track.phrases.get(&e.phrase))
            .map(|p| p.time())
            .sum()
    }

    /// The cursor's position within the section. Only the loop has one, so a step outside it
    /// counts as the loop's first.
    pub fn section_step(&self) -> usize {
        if self.view == View::Song {
            return 0;
        }
        let extent = self.extent();
        // The chain view has no step of its own, so it plays from the row's first.
        let row = if self.view != View::Chain && extent.plays(self.cursor.step) {
            self.cursor.step - extent.start
        } else {
            0
        };
        // Rows before it last their pulses, in the phrase's order, up to where its first time
        // round first plays the row.
        let deal = self.dealer(self.cursor.track).deal(0, self.cursor.slot);
        let time = self
            .phrase_here()
            .map_or(row, |phrase| phrase.time_to(row, deal));
        self.slot_start() + time
    }

    /// Put the cursor on the first step that plays, at the top of the screen.
    pub(crate) fn go_to_loop_start(&mut self) {
        let start = self.extent().start;
        self.cursor.step = start;
        self.top = start;
    }

    /// Send the current version to the audio thread.
    pub fn publish(&mut self) {
        let version = Published::new(Arc::clone(self.song()), Arc::clone(&self.samples));
        self.handle.publish(Arc::new(version));
    }

    /// Do the work due each frame: send waiting versions and read the callback's report.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    /// Do a frame's work as if it's `now`, so the screenshots can step the meters by a fixed time.
    pub fn tick_at(&mut self, now: Instant) {
        self.handle.pump();
        let seconds = now.saturating_duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        let track = self.cursor.track;
        if self.watched != track && self.handle.send(Command::Watch { track }) {
            self.watched = track;
        }
        let report = self.handle.latest();
        self.handle.read_scope(&mut self.scope);
        self.meter
            .update(report.map(|report| report.reading), seconds);
        let tracks = self.song().tracks.len();
        for (index, meter) in self.track_meters.iter_mut().take(tracks).enumerate() {
            meter.update(report.map(|report| report.tracks[index]), seconds);
        }
        if let Some(report) = report {
            self.playhead = report.position;
            self.progress = report.progress;
            self.places = report.places;
            self.report_at = now;
            self.playing_section = report.section;
            self.transport = report.transport;
            self.live = (report.watched == track).then_some(report.live);
        }
        while let Some((track, state)) = self.handle.jam_change() {
            self.jam[track] = state;
        }
        self.take_loaded();
        self.follow_recording();
    }

    /// A key from the terminal, which the kitty keyboard protocol says is a press, a repeat or a
    /// release. What a held key does, in every mode, is decided here and in the three below.
    pub fn key_event(&mut self, key: Key, kind: KeyEventKind, now: Instant) {
        if key.ignored() {
            return;
        }
        // Only a key bound with Cmd, such as `Cmd-l`, goes through, so `Cmd-a` isn't read as `a`.
        // Its release still counts, as `a` can be held before Cmd goes down.
        if key.command && kind != KeyEventKind::Release && !self.binds(key) {
            return;
        }
        match kind {
            KeyEventKind::Press => self.key(key, now),
            KeyEventKind::Repeat => self.key_repeated(key, now),
            KeyEventKind::Release => self.key_released(key, now),
        }
    }

    /// The terminal window's title: `vimtracker` and the project's name. kitty's
    /// `--when-focus-on title:vimtracker` matches it, so its `Cmd` shortcuts can be left to this
    /// window alone.
    pub fn window_title(&self) -> String {
        format!("vimtracker: {}", self.project_name())
    }

    /// Whether a key, after any typed before it, means something here.
    fn binds(&self, key: Key) -> bool {
        let mut keys = self.pending.clone();
        keys.push(key);
        self.table.resolve(self.key_scope(), &keys) != Resolved::Unbound
    }

    /// A key pressed.
    pub fn key(&mut self, key: Key, now: Instant) {
        // A real second press comes after a release, so a press of the key that's down is the
        // terminal repeating it. kitty on macOS has been seen sending a held `a`'s repeats as
        // presses. A key holding a note is down too, even after another key has gone down.
        if self.key_down == Some(key.physical()) || self.holds_note(key) || self.holds_flag(key) {
            self.key_repeated(key, now);
            return;
        }
        self.key_down = Some(key.physical());
        self.repeats_ignored = false;
        self.press(key, now);
    }

    /// A key the terminal repeats while it's held. A transport key acts once however long it's
    /// held, and a note on the keyboard is already held; anything else repeats as a press.
    pub fn key_repeated(&mut self, key: Key, now: Instant) {
        if (self.repeats_ignored && self.key_down == Some(key.physical()))
            || self.holds_note(key)
            || self.holds_flag(key)
        {
            return;
        }
        self.press(key, now);
    }

    /// A key let go: its held note ends, leaving any others held, and so does what it started
    /// that lasts while it's held.
    pub fn key_released(&mut self, key: Key, now: Instant) {
        if self.key_down == Some(key.physical()) {
            self.key_down = None;
            self.repeats_ignored = false;
        }
        if let Some(at) = self
            .held_notes
            .iter()
            .position(|held| held.key == key.physical())
        {
            let held = self.held_notes.remove(at);
            self.let_go_of(held);
        }
        self.let_go_of_flag(key);
        if self
            .momentary
            .is_some_and(|held| held.key == key.physical())
        {
            self.end_momentary(now);
        }
    }

    /// End what a key started that lasts while it's held. Down for less than [`HOLD_TIME`], it was
    /// a tap, and stays as it is: a mute or solo stays switched, and an audition plays out. Held
    /// longer, a mute or solo goes back to how it was, and an audition stops.
    fn end_momentary(&mut self, now: Instant) {
        let Some(held) = self.momentary.take() else {
            return;
        };
        let long = now.saturating_duration_since(held.since) >= HOLD_TIME;
        match held.what {
            Momentary::Mute { track, was } | Momentary::Solo { track, was } => {
                let mute = matches!(held.what, Momentary::Mute { .. });
                match self.levels.get_mut(track) {
                    Some(live) if long => {
                        if mute {
                            live.mute = was;
                            self.handle.send(Command::SetMute { track, mute: was });
                        } else {
                            live.solo = was;
                            self.handle.send(Command::SetSolo { track, solo: was });
                        }
                    }
                    // A save while it was down kept how it was before.
                    Some(_) => self.dirty = true,
                    None => {}
                }
            }
            Momentary::Fill { was } if long => self.set_fill(was),
            Momentary::Fill { .. } => {}
            Momentary::Audition => {
                self.handle.send(Command::EndAudition { held: long });
            }
            Momentary::AuditionNote { track, note } if long => {
                self.handle.send(Command::ReleasePreview { track, note });
            }
            Momentary::AuditionNote { track, note } => {
                self.handle.send(Command::FinishPreview { track, note });
            }
        }
    }

    /// Whether this key is holding a note.
    fn holds_note(&self, key: Key) -> bool {
        self.held_notes
            .iter()
            .any(|held| held.key == key.physical())
    }

    /// End a held key's note, unless another key still down is holding the same one, as a key an
    /// octave up and one an octave down can.
    fn let_go_of(&mut self, held: HeldNote) {
        self.end_take(held.key);
        let shared = self
            .held_notes
            .iter()
            .any(|other| other.track == held.track && other.note == held.note);
        if !shared {
            self.handle.send(Command::ReleasePreview {
                track: held.track,
                note: held.note,
            });
        }
    }

    /// End every held note.
    fn let_go_of_held_notes(&mut self) {
        for held in std::mem::take(&mut self.held_notes) {
            self.let_go_of(held);
        }
    }

    /// The terminal window lost focus, so a key that's down may never be reported let go. Its
    /// note ends now, what it started that lasts while it's held ends as if it was let go now,
    /// and the next press of it is a press.
    pub fn focus_lost(&mut self, now: Instant) {
        self.key_down = None;
        self.repeats_ignored = false;
        self.let_go_of_held_notes();
        self.drop_flags();
        self.end_momentary(now);
    }

    /// Sound a note for as long as this key is down, ignoring the key's repeats. Each key held
    /// sounds its own note, so several play a chord.
    fn hold_note(&mut self, key: Key, note: u8, velocity: u8) {
        let track = self.cursor.track;
        self.held_notes.push(HeldNote {
            key: key.physical(),
            track,
            note,
        });
        self.repeats_ignored = true;
        self.handle.send(Command::HoldPreview {
            track,
            note,
            velocity,
        });
    }

    /// Act on a press, or a repeat that acts as one.
    fn press(&mut self, key: Key, now: Instant) {
        let was = self.mode;
        self.started = None;
        self.handle_key(key, now);
        // Something that lasts while its key is down is tied to that key, and doesn't repeat.
        // One already held ends as if let go now, as only one is kept.
        if let Some(what) = self.started.take() {
            self.end_momentary(now);
            self.momentary = Some(Held {
                key: key.physical(),
                since: now,
                what,
            });
            self.repeats_ignored = true;
        }
        // Leaving the keyboard, or Insert, lets go of what it's holding.
        if matches!(was, Mode::Keys | Mode::Insert) && self.mode != was {
            self.let_go_of_held_notes();
            self.drop_flags();
            self.last_written = None;
        }
    }

    fn handle_key(&mut self, key: Key, now: Instant) {
        match self.mode {
            Mode::Normal
            | Mode::OperatorPending
            | Mode::VisualField
            | Mode::VisualStep
            | Mode::Chain
            | Mode::Track
            | Mode::Mixer
            | Mode::Song
            | Mode::Audio
            | Mode::Project => {
                self.command_key_normal(key, now);
            }
            Mode::Insert => self.insert_key(key),
            Mode::Replace => self.replace_key(key),
            Mode::Command => self.command_key(key),
            Mode::Help => self.help_key(key),
            Mode::Guide => self.guide_key(key),
            Mode::Picker => self.picker_key(key),
            Mode::Shape => self.shape_key(key),
            Mode::Jump => self.jump_key(key),
            Mode::Restore => self.restore_key(key),
            Mode::Keys => self.keys_key(key, now),
        }
    }

    /// Play the track's sound from a few keys, `a` `w` `s` `e` `d` `f`, without writing anything,
    /// while every other key works on the page as it does, so settings change as it plays. A note
    /// lasts as long as its key is down.
    fn keys_key(&mut self, key: Key, now: Instant) {
        let plain = self.pending.is_empty() && !key.ctrl && !key.alt;
        match key.code {
            KeyCode::Esc | KeyCode::Char(';') if self.pending.is_empty() => {
                self.mode = Mode::Normal;
                self.status.clear();
            }
            KeyCode::Char('<') if plain => self.octave = self.octave.saturating_sub(1),
            KeyCode::Char('>') if plain => self.octave = (self.octave + 1).min(9),
            KeyCode::Char(c) if plain && KEYS_NOTES.contains(c) => {
                if let Some(note) = self.played_key(c) {
                    self.hold_note(key, note, self.note_velocity);
                }
            }
            _ => {
                self.command_key_normal(key, now);
                // The keys only play on the track page.
                if self.mode == Mode::Keys && self.view != View::Track {
                    self.mode = Mode::Normal;
                }
            }
        }
    }

    /// The note a key plays in the current octave. With a scale, the home row goes up through it
    /// from the root, and the row above is silent, so every key is in key.
    fn played_key(&self, c: char) -> Option<u8> {
        if self.scale.on {
            degree_key(c).map(|degree| self.scale.degree(degree, self.octave))
        } else {
            note_key(c).map(|semitones| {
                let midi = (u16::from(self.octave) + 1) * 12 + u16::from(semitones);
                midi.clamp(u16::from(MIN_NOTE), u16::from(MAX_NOTE)) as u8
            })
        }
    }

    /// What the popup should list, once the delay has passed.
    pub fn popup_rows(&self, now: Instant, delay: Duration) -> Vec<(String, &'static str)> {
        if self.pending.is_empty() || !which_key_due(self.pending_since, now, delay) {
            return Vec::new();
        }
        self.table.continuations(self.key_scope(), &self.pending)
    }

    /// Which rows of the binding table apply now. Normal mode's keys depend on the view.
    pub fn key_scope(&self) -> Mode {
        match (self.mode, self.view) {
            (Mode::Keys, View::Track) => Mode::Track,
            (Mode::Normal, View::Chain) => Mode::Chain,
            (Mode::Normal, View::Track) => Mode::Track,
            (Mode::Normal, View::Mixer) => Mode::Mixer,
            (Mode::Normal, View::Audio) => Mode::Audio,
            (Mode::Normal, View::Project) => Mode::Project,
            (Mode::Normal, View::Song) => Mode::Song,
            (mode, _) => mode,
        }
    }

    pub(crate) fn show_view(&mut self, view: View) {
        self.count = None;
        // The audio page lists the devices afresh each time it opens.
        self.audio.refresh |= view == View::Audio && self.view != View::Audio;
        self.view = view;
        self.note_view(view);
        self.clamp_cursor();
        if view != View::Phrase {
            self.stop_recording();
        }
    }

    /// Show a view, as a jump that `Ctrl-O` can undo.
    pub(crate) fn jump_to_view(&mut self, view: View) {
        if view != self.view {
            self.record_jump();
        }
        self.show_view(view);
    }

    /// Every key, for `Space k`.
    pub fn help_rows(&self) -> Vec<String> {
        let mut rows = Vec::new();
        let mut last = None;
        for binding in self.table.rows() {
            if last != Some(binding.mode) {
                rows.push(String::new());
                rows.push(format!("{:?}", binding.mode));
                last = Some(binding.mode);
            }
            rows.push(format!("  {:<12} {}", binding.keys, binding.description));
        }
        rows
    }

    fn help_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.help_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            _ => {
                self.help_scroll = 0;
                self.mode = Mode::Normal;
            }
        }
    }

    /// Hear a row, unless playback is using the tracks, for as long as the key is down if it's
    /// held.
    pub(crate) fn audition(&mut self, section_step: usize) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
        } else {
            self.audition_track(self.cursor.section, section_step);
        }
    }

    /// Hear a row of a section, for as long as the key is down if it's held.
    pub(crate) fn audition_section(&mut self, section: usize, section_step: usize) {
        self.handle.send(Command::Audition {
            section,
            section_step,
            track: None,
        });
        self.started = Some(Momentary::Audition);
    }

    /// Hear the row the cursor's track is on at a step of its own, with the row each other track
    /// plays then, for as long as the key is down if it's held.
    pub(crate) fn audition_track(&mut self, section: usize, step: usize) {
        self.handle.send(Command::Audition {
            section,
            section_step: step,
            track: Some(self.cursor.track),
        });
        self.started = Some(Momentary::Audition);
    }

    /// Scroll an open guide, stopping when its last row reaches the bottom of the window.
    fn guide_key(&mut self, key: Key) {
        let Some(guide) = self.guide else {
            self.mode = Mode::Normal;
            return;
        };
        let last = guide
            .lines(self.guide_width)
            .len()
            .saturating_sub(self.guide_page);
        let half = (self.guide_page / 2).max(1);
        self.guide_scroll = match key.code {
            KeyCode::Char('d') if key.ctrl => self.guide_scroll + half,
            KeyCode::Char('u') if key.ctrl => self.guide_scroll.saturating_sub(half),
            KeyCode::Char('j') | KeyCode::Down => self.guide_scroll + 1,
            KeyCode::Char('k') | KeyCode::Up => self.guide_scroll.saturating_sub(1),
            KeyCode::Char('g') => 0,
            KeyCode::Char('G') => last,
            KeyCode::Char('?') => {
                self.guide = None;
                self.open_guide_picker();
                return;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.guide = None;
                self.mode = Mode::Normal;
                return;
            }
            _ => self.guide_scroll,
        }
        .min(last);
    }

    pub(crate) fn run(&mut self, cmd: Cmd) {
        let handled = match self.view {
            View::Chain => self.run_chain(cmd),
            View::Track => self.run_track_page(cmd),
            View::Mixer => self.run_mixer(cmd),
            View::Audio => self.run_audio_page(cmd),
            View::Project => self.run_project_page(cmd),
            View::Song => self.run_song(cmd),
            View::Phrase => false,
        };
        if handled || self.run_tree(cmd) || self.run_grammar(cmd) {
            return;
        }
        match cmd {
            Cmd::ViewTop | Cmd::ViewMiddle | Cmd::ViewBottom => self.view_cursor(cmd),
            Cmd::HalfPageDown => self.half_page(1),
            Cmd::HalfPageUp => self.half_page(-1),
            Cmd::ScrollDown => self.scroll(3),
            Cmd::ScrollUp => self.scroll(-3),
            Cmd::Double => self.double(),
            Cmd::SlideLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.slide_loop(by * times as isize);
            }
            Cmd::RotateLoop(by) => {
                let times = self.count.take().unwrap_or(1).min(MAX_PHRASE_STEPS);
                self.rotate_loop(by * times as isize);
            }
            Cmd::StartLoopHere => self.start_loop_here(),
            Cmd::EndLoopHere => self.end_loop_here(),
            Cmd::ShowView(view) => self.jump_to_view(view),
            Cmd::Picker => self.open_view_picker(),
            Cmd::TrackPicker => self.open_track_picker(),
            Cmd::Help => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }
            Cmd::Guides => self.open_guide_picker(),
            Cmd::PickValue => self.open_field_picker(),
            Cmd::Shape => self.open_shape(),
            Cmd::PickSample => self.open_default_sample_picker(),
            Cmd::PickKit => self.open_kit_picker(),
            Cmd::Replace => self.begin_replace(),
            Cmd::Substitute => self.substitute(),
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.last_written = None;
                self.mode = Mode::Insert;
            }
            Cmd::EnterCommand => {
                self.command_line.clear();
                self.mode = Mode::Command;
            }
            Cmd::PlayFromCursor => {
                let step = self.section_step();
                // The song view plays from the row, so tracks stand where the song has them.
                let track = (self.view != View::Song).then_some(self.cursor.track);
                self.handle.send(Command::PlaySection {
                    section: self.cursor.section,
                    step,
                    track,
                });
                self.repeats_ignored = true;
            }
            Cmd::PlayFromStart => {
                self.handle.send(Command::PlayFrom { step: 0 });
                self.repeats_ignored = true;
            }
            Cmd::Audition => {
                if self.extent().plays(self.cursor.step) {
                    let section_step = self.section_step();
                    self.audition(section_step);
                } else {
                    self.audition_outside_loop();
                }
            }
            Cmd::PauseOrResume => {
                self.handle.send(Command::PauseOrResume);
                self.repeats_ignored = true;
            }
            Cmd::Quit => self.quit(false),
            Cmd::ToggleScale => self.toggle_scale(),
            Cmd::NarrowGrid => self.change_grid(false),
            Cmd::WidenGrid => self.change_grid(true),
            Cmd::NextTrack => self.move_track(1),
            Cmd::PreviousTrack => self.move_track(-1),
            Cmd::ToggleMute => {
                let track = self.cursor.track;
                if let Some(was) = self.levels.get(track).map(|live| live.mute) {
                    self.toggle_mute();
                    if self.levels[track].mute != was {
                        self.started = Some(Momentary::Mute { track, was });
                    }
                }
            }
            Cmd::ToggleSolo => {
                let track = self.cursor.track;
                if let Some(was) = self.levels.get(track).map(|live| live.solo) {
                    self.toggle_solo();
                    if self.levels[track].solo != was {
                        self.started = Some(Momentary::Solo { track, was });
                    }
                }
            }
            Cmd::ToggleFill => {
                let was = self.fill;
                self.set_fill(!was);
                self.started = Some(Momentary::Fill { was });
            }
            Cmd::ClearMutes => {
                // A held mute or solo stays cleared when it's let go.
                if self.momentary.is_some_and(|held| {
                    matches!(held.what, Momentary::Mute { .. } | Momentary::Solo { .. })
                }) {
                    self.momentary = None;
                }
                self.clear_live();
            }
            Cmd::ToggleRecord => self.toggle_recording(),
            Cmd::Undo => {
                let before = self.loop_place();
                if self.history.undo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                } else {
                    self.status = "Nothing to undo".to_owned();
                }
            }
            Cmd::Redo => {
                let before = self.loop_place();
                if self.history.redo().is_some() {
                    self.keep_row(before);
                    self.after_history();
                } else {
                    self.status = "Nothing to redo".to_owned();
                }
            }
            _ => {}
        }
    }

    fn double(&mut self) {
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::Double {
                track: self.cursor.track,
                phrase,
            });
        }
    }

    /// The phrase under the cursor and where its loop starts.
    fn loop_place(&self) -> (Option<u32>, usize) {
        (self.phrase_id(), self.extent().start)
    }

    /// After the loop has moved, keep the cursor and the view on the same row numbers, so the
    /// notes scroll under them.
    fn keep_row(&mut self, before: (Option<u32>, usize)) {
        let (phrase, old) = before;
        if phrase != self.phrase_id() {
            return;
        }
        let by = self.extent().start as isize - old as isize;
        self.cursor.step = self.cursor.step.saturating_add_signed(by);
        self.top = self.top.saturating_add_signed(by);
    }

    fn slide_loop(&mut self, by: isize) {
        if let Some(phrase) = self.phrase_id() {
            let before = self.loop_place();
            self.apply(&Action::SlideLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
            self.keep_row(before);
            self.clamp_cursor();
        }
    }

    /// Rotate the loop's steps. The cursor stays on its row. Going all the way round changes
    /// nothing, so it doesn't leave an undo step.
    fn rotate_loop(&mut self, by: isize) {
        let length = self.extent().length as isize;
        if length > 0
            && by.rem_euclid(length) != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::RotateLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
        }
    }

    /// The FX column the cursor is on.
    fn fx_column(&self) -> Option<usize> {
        match self.cursor.field {
            Field::FxCommand(slot) | Field::FxValue(slot) | Field::FxFine(slot) => Some(slot),
            Field::Note | Field::Velocity | Field::Nudge | Field::Gate | Field::GateLength => None,
        }
    }

    /// The FX column the cursor is on, if it already has a lane, which the loop keys then set. A
    /// column without one follows the notes, and the loop keys set the phrase's loop as they do
    /// on the notes, so a column only comes apart from them with `:lane`.
    fn lane_slot(&self) -> Option<usize> {
        let slot = self.fx_column()?;
        self.phrase_here()?.lane(slot).map(|_| slot)
    }

    /// The cursor's FX column's lane as it is, or the phrase's loop if it has none.
    fn lane_here(&self, slot: usize) -> Lane {
        let extent = self.extent();
        self.phrase_here()
            .and_then(|phrase| phrase.lane(slot))
            .unwrap_or(Lane {
                start: extent.start,
                length: extent.length,
            })
    }

    /// Give the cursor's FX column a lane, or none to follow the phrase's loop again.
    fn set_lane(&mut self, slot: usize, lane: Option<Lane>) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        if self.phrase_here().and_then(|p| p.lane(slot)) == lane {
            return;
        }
        self.apply(&Action::SetLane {
            track: self.cursor.track,
            phrase,
            slot,
            lane,
        });
        self.status = match lane {
            Some(lane) => format!("FX {} loops on its own, {} steps", slot + 1, lane.length),
            None => format!("FX {} follows the phrase's loop", slot + 1),
        };
    }

    /// Start the loop on the cursor's step. The notes stay where they are and the numbers change.
    /// On an FX column, its lane starts there instead, keeping its length.
    fn start_loop_here(&mut self) {
        if let Some(slot) = self.lane_slot() {
            let length = self.lane_here(slot).length;
            let start = self.cursor.step;
            self.set_lane(slot, Some(Lane { start, length }));
            return;
        }
        let by = self.extent().row(self.cursor.step);
        if by != 0
            && let Some(phrase) = self.phrase_id()
        {
            self.apply(&Action::SlideLoop {
                track: self.cursor.track,
                phrase,
                by,
            });
        }
    }

    /// End the loop on the cursor's step, so its row is the last that plays. Steps after it stay
    /// stored, and a dim step below the loop brings the steps before it in.
    fn end_loop_here(&mut self) {
        if let Some(slot) = self.lane_slot() {
            let start = self.lane_here(slot).start;
            let Some(length) = (self.cursor.step + 1).checked_sub(start).filter(|l| *l > 0) else {
                self.status = "A lane can't end above its start".to_owned();
                return;
            };
            self.set_lane(slot, Some(Lane { start, length }));
            return;
        }
        let extent = self.extent();
        let Ok(row) = usize::try_from(extent.row(self.cursor.step)) else {
            self.status = "The loop can't end above its start".to_owned();
            return;
        };
        let length = row + 1;
        if let Some(phrase) = self.phrase_id()
            && length != extent.length
        {
            self.apply(&Action::SetLength {
                track: self.cursor.track,
                phrase,
                length,
            });
        }
    }

    /// Hear the note on a row outside the loop. Other tracks have nothing to play with it, since
    /// only the loop has a place in the section.
    fn audition_outside_loop(&mut self) {
        if self.transport == Transport::Playing {
            self.status = "Audition works while stopped or paused".to_owned();
            return;
        }
        let Some(step) = self
            .phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step))
        else {
            return;
        };
        let Note::On(midi) = step.note else {
            return;
        };
        let velocity = step.velocity.unwrap_or(MAX_VELOCITY);
        let transpose = self
            .chain_entries()
            .get(self.cursor.slot)
            .map_or(0, |entry| entry.transpose);
        let note = (i16::from(midi) + i16::from(transpose))
            .clamp(i16::from(MIN_NOTE), i16::from(MAX_NOTE)) as u8;
        self.handle.send(Command::HoldPreview {
            track: self.cursor.track,
            note,
            velocity,
        });
        self.started = Some(Momentary::AuditionNote {
            track: self.cursor.track,
            note,
        });
    }

    fn stored_steps(&self) -> usize {
        self.extent().stored
    }

    /// The furthest the view can scroll: to the end of what is stored, or with the loop's start at
    /// the top, so a slide can keep row `00` still on a screen with room to spare.
    fn last_top(&self) -> usize {
        let extent = self.extent();
        extent
            .stored
            .saturating_sub(self.view_height)
            .max(extent.start)
    }

    /// Keep the cursor on screen, moving the view rather than the cursor.
    pub fn follow_cursor(&mut self, height: usize) {
        self.view_height = height.max(1);
        let last_top = self.last_top();
        if self.cursor.step < self.top {
            self.top = self.cursor.step;
        } else if self.cursor.step >= self.top + self.view_height {
            self.top = self.cursor.step + 1 - self.view_height;
        }
        self.top = self.top.min(last_top);
    }

    fn view_cursor(&mut self, cmd: Cmd) {
        let height = self.view_height.min(self.stored_steps().max(1));
        self.cursor.step = self.top
            + match cmd {
                Cmd::ViewTop => 0,
                Cmd::ViewMiddle => height / 2,
                _ => height - 1,
            };
        self.clamp_cursor();
    }

    fn half_page(&mut self, sign: isize) {
        let half = (self.view_height / 2).max(1) as isize;
        self.move_step(sign * half);
        self.top = self.cursor.step.saturating_sub(self.view_height / 2);
    }

    /// Scroll the view. The cursor only moves if it would leave the screen.
    fn scroll(&mut self, by: isize) {
        self.top = self.top.saturating_add_signed(by).min(self.last_top());
        let bottom = self.top + self.view_height.saturating_sub(1);
        self.cursor.step = self.cursor.step.clamp(self.top, bottom);
        self.clamp_cursor();
    }

    fn after_history(&mut self) {
        self.dirty = true;
        self.fit_levels();
        self.cursor.track = self.cursor.track.min(self.song().tracks.len() - 1);
        self.clamp_cursor();
        self.publish();
    }

    /// Keep one set of live controls per track as tracks come and go.
    pub(crate) fn fit_levels(&mut self) {
        // Those of the fixed tracks stay with them, and the engine clears any no longer used.
        let tracks = self.song().tracks.len();
        let was = self.levels.len();
        let (from, to) = (
            was.saturating_sub(FIXED_TRACKS),
            tracks.saturating_sub(FIXED_TRACKS),
        );
        if tracks > was {
            let added = std::iter::repeat_n(TrackLive::default(), tracks - was);
            self.levels.splice(from..from, added);
        } else {
            self.levels.drain(to..from);
        }
    }

    /// Move to another track, staying at the same position in the section.
    pub(crate) fn move_track(&mut self, by: isize) {
        let last = self.song().tracks.len() as isize - 1;
        let track = (self.cursor.track as isize + by).clamp(0, last) as usize;
        if track == self.cursor.track {
            return;
        }
        let section_step = self.section_step();
        let extent = self.extent();
        // A row outside the loop has no place in the section, so it keeps its number instead.
        let outside = (!extent.plays(self.cursor.step)).then(|| extent.row(self.cursor.step));
        let from = self.cursor.track;
        self.cursor.track = track;
        let song = Arc::clone(self.song());
        let position = song
            .sections
            .get(self.cursor.section)
            .and_then(|s| s.chains.get(track).copied().flatten())
            .and_then(|chain| {
                let place = self.step_on(track, from, section_step);
                song.tracks[track].chain_position(chain, place, self.dealer(track))
            });
        match position {
            Some(position) => {
                self.cursor.slot = position.slot;
                let row = outside.unwrap_or(position.step as isize);
                self.cursor.step = self.extent().step_of(row);
            }
            None => self.cursor.slot = 0,
        }
    }

    /// Whether this track can be muted and soloed, saying so if not.
    fn can_silence(&mut self) -> bool {
        let master = self
            .song()
            .tracks
            .get(self.cursor.track)
            .is_some_and(|track| track.kind == TrackKind::Master);
        if master {
            self.status = "The master can't be muted or soloed".to_owned();
        }
        !master
    }

    pub(crate) fn toggle_mute(&mut self) {
        if !self.can_silence() {
            return;
        }
        let track = self.cursor.track;
        if let Some(live) = self.levels.get_mut(track) {
            live.mute = !live.mute;
            let mute = live.mute;
            self.handle.send(Command::SetMute { track, mute });
            self.dirty = true;
        }
    }

    /// Turn fill on or off, saying which.
    pub(crate) fn set_fill(&mut self, on: bool) {
        self.fill = on;
        self.handle.send(Command::SetFill { on });
        self.status = if on { "Fill on" } else { "Fill off" }.to_owned();
    }

    pub(crate) fn toggle_solo(&mut self) {
        if !self.can_silence() {
            return;
        }
        let track = self.cursor.track;
        if let Some(live) = self.levels.get_mut(track) {
            live.solo = !live.solo;
            let solo = live.solo;
            self.handle.send(Command::SetSolo { track, solo });
            self.dirty = true;
        }
    }

    fn clear_live(&mut self) {
        self.levels.fill(TrackLive::default());
        self.send_live();
        self.dirty = true;
    }

    pub(crate) fn slots(&self) -> usize {
        self.phrase_here()
            .and_then(|p| p.steps().get(self.cursor.step))
            .map_or(0, |step| step.fx.len())
    }

    pub(crate) fn move_step(&mut self, by: isize) {
        let Some(phrase) = self.phrase_here().cloned() else {
            return;
        };
        let last = phrase.steps().len() as isize - 1;
        self.cursor.step = (self.cursor.step as isize + by).clamp(0, last) as usize;
    }

    /// Keep the cursor on something that exists. Only the chain view has a row past the end.
    pub(crate) fn clamp_cursor(&mut self) {
        let sections = self.song().sections.len();
        self.cursor.section = self.cursor.section.min(sections.saturating_sub(1));
        let rows = self.chain_entries().len();
        let last = if self.view == View::Chain {
            rows
        } else {
            rows.saturating_sub(1)
        };
        self.cursor.slot = self.cursor.slot.min(last);
        self.move_step(0);
    }

    /// Typing here means what it does in Insert, for this one field, and `advance` ends it.
    fn replace_key(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => {
                self.cancel_entry();
                if let Some(field) = self.replacing.take() {
                    self.cursor.field = field;
                }
                if let Some((selection, _)) = self.replacing_selection.take() {
                    self.cursor.step = selection.range.first;
                }
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if self.entry.text.is_empty() => {}
            KeyCode::Char('c' | 'v') if self.cursor.field == Field::Note => {}
            _ => match self.cursor.field {
                Field::Note => self.insert_note(key),
                Field::Velocity | Field::FxValue(_) => self.insert_value(key),
                Field::Nudge => self.insert_nudge(key),
                Field::Gate => self.insert_gate(key),
                Field::GateLength => self.insert_length(key),
                Field::FxCommand(slot) => self.insert_command(key, slot),
                Field::FxFine(slot) => self.insert_fine(key, slot),
            },
        }
    }

    fn insert_key(&mut self, key: Key) {
        // You can start and pause while writing, as `Tab` and `Shift-Tab` are never typed.
        match key.code {
            KeyCode::Tab => return self.run(Cmd::PauseOrResume),
            KeyCode::BackTab => return self.run(Cmd::PlayFromStart),
            _ => {}
        }
        if self.view == View::Phrase && key.alt {
            match key.code {
                KeyCode::Char('1') => self.change_grid(false),
                KeyCode::Char('2') => self.change_grid(true),
                KeyCode::Char('s') => self.toggle_scale(),
                _ => {}
            }
            return;
        }
        let skip = self.song().skip as usize;
        let arrow = match (key.code, key.shift) {
            (KeyCode::Up, _) => Some(-1),
            (KeyCode::Down, _) => Some(1),
            (KeyCode::Left, true) => Some(snap_by(self.current_row(), skip, false)),
            (KeyCode::Right, true) => Some(snap_by(self.current_row(), skip, true)),
            (KeyCode::Left, false) => Some(-(skip as isize)),
            (KeyCode::Right, false) => Some(skip as isize),
            _ => None,
        };
        if let Some(by) = arrow {
            if self.view == View::Phrase {
                self.insert_move(by);
            }
            return;
        }
        if self.view == View::Project {
            self.insert_project_page(key);
            return;
        }
        if self.view != View::Phrase {
            if !key.ctrl {
                match self.view {
                    View::Chain => self.insert_chain_row(key.code),
                    View::Song => self.insert_song(key.code),
                    _ => self.insert_track_page(key.code),
                }
            }
            return;
        }
        if key.code == KeyCode::Char(';') && !key.ctrl && !key.alt {
            self.toggle_auto_jump();
            return;
        }
        match self.cursor.field {
            Field::Note => self.insert_note(key),
            Field::Velocity | Field::FxValue(_) => self.insert_value(key),
            Field::Nudge => self.insert_nudge(key),
            Field::Gate => self.insert_gate(key),
            Field::GateLength => self.insert_length(key),
            Field::FxCommand(slot) => self.insert_command(key, slot),
            Field::FxFine(slot) => self.insert_fine(key, slot),
        }
    }

    /// Set the scale, which isn't an edit to undo, and say what it now does.
    pub(crate) fn set_scale(&mut self, scale: Scale) {
        self.scale = scale;
        self.handle.send(Command::SetScale { scale });
        self.dirty = true;
        self.status = if scale.on {
            format!("Scale {} {}", scale.root_name(), scale.kind.name())
        } else {
            "Scale off".to_owned()
        };
    }

    fn toggle_scale(&mut self) {
        self.count = None;
        self.set_scale(Scale {
            on: !self.scale.on,
            ..self.scale
        });
    }

    /// Move this phrase's notes to the nearest in the scale.
    fn fit_to_scale(&mut self) {
        if !self.scale.on {
            self.status = "The scale is off".to_owned();
            return;
        }
        if let Some(phrase) = self.phrase_id() {
            self.apply(&Action::FitToScale {
                track: self.cursor.track,
                phrase,
                scale: self.scale,
            });
        }
    }

    /// Halve or double the skip setting, as a grid narrows or widens in Ableton. A size between
    /// these goes to the next one along, and one past them only comes back.
    fn change_grid(&mut self, wider: bool) {
        self.count = None;
        let skip = self.song().skip;
        let next = if wider {
            GRID_SIZES.iter().find(|size| **size > skip)
        } else {
            GRID_SIZES.iter().rfind(|size| **size < skip)
        };
        if let Some(next) = next {
            self.apply(&Action::SetSkipSize(*next));
        }
    }

    fn insert_note(&mut self, key: Key) {
        if self.mode == Mode::Insert
            && let Some(flag) = self.flag_key(key)
        {
            self.hold_flag(key, flag);
            return;
        }
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                self.back_for_clearing();
                self.clear_note();
            }
            KeyCode::Char('z') => self.octave = self.octave.saturating_sub(1),
            KeyCode::Char('x') => self.octave = (self.octave + 1).min(9),
            KeyCode::Char('c') => {
                self.note_velocity = next_note_velocity(self.note_velocity, false)
            }
            KeyCode::Char('v') => self.note_velocity = next_note_velocity(self.note_velocity, true),
            // A note, and an OFF, is written once however long its key is held.
            KeyCode::Char('1') => {
                self.repeats_ignored = true;
                self.write_note(Note::Off);
                self.advance();
            }
            KeyCode::Char(c) if !key.ctrl => {
                // While recording, a note goes in where the track is playing, and the cursor
                // stays.
                if let Some(midi) = self.played_key(c)
                    && self.recording.is_some()
                    && self.mode == Mode::Insert
                {
                    self.record_note(key, midi);
                    let velocity = self.flagged_velocity(self.note_velocity);
                    self.hold_note(key, midi, velocity);
                } else if let Some(midi) = self.played_key(c) {
                    let on_step = self
                        .phrase_here()
                        .cloned()
                        .and_then(|phrase| phrase.steps().get(self.cursor.step).cloned());
                    let velocity = on_step
                        .as_ref()
                        .map_or(self.note_velocity, |step| self.played_with(step));
                    let velocity = self.flagged_velocity(velocity);
                    let (track, step) = (self.cursor.track, self.cursor.step);
                    let phrase = self.write_played_note(midi);
                    self.hold_note(key, midi, velocity);
                    self.advance();
                    self.last_written = phrase.map(|phrase| Written {
                        track,
                        phrase,
                        step,
                        cursor: self.cursor,
                    });
                }
            }
            _ => {}
        }
    }

    fn current_row(&self) -> isize {
        self.extent().row(self.cursor.step)
    }

    /// Move without writing, staying in Insert. Anything half typed is dropped.
    fn insert_move(&mut self, by: isize) {
        if !self.entry.text.is_empty() || self.entry.command.is_some() {
            self.cancel_entry();
            self.mode = Mode::Insert;
        }
        self.move_step(by);
    }

    /// Write a played note with its velocity, and an accent or slide whose key is down, as one
    /// change, and say which phrase it went in.
    fn write_played_note(&mut self, midi: u8) -> Option<u32> {
        let Some(id) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return None;
        };
        let current = self.phrase_here();
        let step = current.and_then(|p| p.steps().get(self.cursor.step).cloned())?;
        let velocity = self.flagged_velocity(self.played_with(&step));
        let gate = if self.flagged_slide() {
            slid(step.gate, true)
        } else {
            step.gate
        };
        self.apply(&Action::SetStep {
            track: self.cursor.track,
            phrase: id,
            step: self.cursor.step,
            value: Step {
                note: Note::On(midi),
                velocity: Some(velocity),
                gate,
                ..step
            },
        });
        self.use_flags();
        Some(id)
    }

    /// The velocity a played note takes. Replacing keeps what the step has.
    fn played_with(&self, step: &Step) -> u8 {
        if self.mode == Mode::Replace {
            step.velocity.unwrap_or(self.note_velocity)
        } else {
            self.note_velocity
        }
    }

    /// Clear a note along with the velocity it was written with.
    fn clear_note(&mut self) {
        let Some(id) = self.phrase_id() else {
            return;
        };
        let current = self.phrase_here();
        let Some(step) = current.and_then(|p| p.steps().get(self.cursor.step).cloned()) else {
            return;
        };
        self.apply(&Action::SetStep {
            track: self.cursor.track,
            phrase: id,
            step: self.cursor.step,
            value: Step {
                note: Note::Empty,
                velocity: None,
                ..step
            },
        });
    }

    /// Leave Insert, dropping anything typed but not written.
    fn cancel_entry(&mut self) {
        if let Some(command) = self.entry.command
            && let Field::FxValue(slot) = self.cursor.field
        {
            self.status = format!("{} needs a value, so nothing was written", command.name());
            self.cursor.field = Field::FxCommand(slot);
        }
        self.entry = Entry::default();
        self.mode = Mode::Normal;
    }

    pub(crate) fn set_field(&mut self, value: FieldValue) {
        let Some(phrase) = self.phrase_id() else {
            return;
        };
        self.apply(&Action::SetField {
            track: self.cursor.track,
            phrase,
            step: self.cursor.step,
            value,
        });
    }

    /// Two hex digits commit, and auto-jump moves on by the grid. A command typed just before
    /// takes effect here, so typing `CUT` during playback doesn't close the filter.
    fn insert_value(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    match self.cursor.field {
                        Field::Velocity => self.set_field(FieldValue::Velocity(None)),
                        Field::FxValue(slot) if self.entry.command.is_none() => {
                            self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    self.commit_value();
                }
            }
            _ => {}
        }
    }

    fn commit_value(&mut self) {
        let text = std::mem::take(&mut self.entry.text);
        let Ok(value) = u8::from_str_radix(&text, 16) else {
            return;
        };
        match self.cursor.field {
            Field::Velocity => {
                self.set_field(FieldValue::Velocity(Some(value.min(MAX_VELOCITY))));
            }
            Field::FxValue(slot) => {
                let current = self.current_slot(slot).unwrap_or_default();
                // A condition's value is one of a list, and anything else is refused.
                let command = self.entry.command.or(current.command);
                if command == Some(FxCommand::Cnd) && Condition::from_value(value).is_none() {
                    self.status = format!("CND {value:02X} isn't a condition: K lists them");
                    return;
                }
                if command == Some(FxCommand::Acu) && amount(value).is_none() {
                    self.status = format!(
                        "ACU {value:02X} is past {MOST_DEGREES}: 01 to 1C go up, FF to E4 down, and K lists them"
                    );
                    return;
                }
                let Some(command) = self.entry.command.take().or(current.command) else {
                    self.status = "Type an FX command first".to_owned();
                    return;
                };
                // Typed over the same setting, the coarse digits change and the fine part stays.
                let fine = if current.command == Some(command) {
                    current.fine()
                } else {
                    0
                };
                self.set_field(FieldValue::Slot(
                    slot,
                    FxSlot {
                        command: Some(command),
                        value: u16::from(value) << 8 | u16::from(fine),
                        mark: self.mark_for_typed_value(slot),
                    },
                ));
            }
            _ => {}
        }
        self.advance();
    }

    /// Two hex digits set a value's fine part, keeping its coarse digits, and auto-jump moves on
    /// by the grid.
    fn insert_fine(&mut self, key: Key, slot: usize) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_fine(slot, 0);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                let current = self.current_slot(slot).unwrap_or_default();
                if !current.command.is_some_and(FxCommand::has_fine) {
                    self.status = "Only a setting that sweeps has a fine part".to_owned();
                    return;
                }
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    if let Ok(fine) = u8::from_str_radix(&text, 16) {
                        self.set_fine(slot, fine);
                    }
                    self.advance();
                }
            }
            _ => {}
        }
    }

    /// Set a value's fine part, where it has one.
    pub(crate) fn set_fine(&mut self, slot: usize, fine: u8) {
        let current = self.current_slot(slot).unwrap_or_default();
        if current.command.is_some_and(FxCommand::has_fine) {
            self.set_field(FieldValue::Slot(
                slot,
                FxSlot {
                    value: current.value & 0xFF00 | u16::from(fine),
                    ..current
                },
            ));
        }
    }

    /// The mark a value typed here gets: the one it's typed over, or else the nearest value's
    /// above it in this column.
    pub(crate) fn mark_for_typed_value(&self, slot: usize) -> Mark {
        let phrase = self.phrase_here();
        let Some(steps) = phrase.map(|phrase| phrase.steps()) else {
            return Mark::default();
        };
        let upward = steps[..=self.cursor.step.min(steps.len() - 1)].iter().rev();
        upward
            .filter_map(|step| step.fx.get(slot))
            .find(|fx| fx.is_value())
            .map_or(Mark::default(), |fx| fx.mark)
    }

    pub(crate) fn current_slot(&self, slot: usize) -> Option<FxSlot> {
        let phrase = self.phrase_here()?;
        phrase.steps().get(self.cursor.step)?.fx.get(slot).copied()
    }

    /// A label, then `Enter`, moves to the value. One that isn't on the track opens the list.
    fn insert_command(&mut self, key: Key, slot: usize) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_field(FieldValue::Slot(slot, FxSlot::default()));
                }
            }
            // A label is 3 or 4 letters, after a slot's digit for an effect's setting.
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_alphanumeric() => {
                if self.entry.text.len() < LONGEST_LABEL {
                    self.entry.text.push(c.to_ascii_uppercase());
                }
            }
            // A label on this track goes to its value. Anything else opens the list, searching
            // for what was typed, so a name half remembered still gets there.
            KeyCode::Enter if !self.entry.text.is_empty() => {
                let name = std::mem::take(&mut self.entry.text);
                let track = self.song().tracks.get(self.cursor.track);
                match track.and_then(|track| FxCommand::typed(&name, track)) {
                    Some(command) => {
                        self.entry.command = Some(command);
                        self.cursor.field = Field::FxValue(slot);
                    }
                    None => {
                        self.status = format!("{name} isn't a label on this track");
                        self.open_fx_picker(slot, &name.to_lowercase());
                    }
                }
            }
            _ => {}
        }
    }

    /// A gate's three letters then its pulses, like `HLD3`. A digit on its own keeps the gate type
    /// that's there, or `SGL` in an empty cell, and changes only the pulses. The note's length
    /// stays.
    fn insert_gate(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_field(FieldValue::Gate(None));
                }
            }
            KeyCode::Char(c @ '0'..='9') if !key.ctrl && self.entry.text.len() != 1 => {
                let pulses = c as u8 - b'0';
                if pulses > MAX_PULSES {
                    self.status = format!("A row lasts 0 to {MAX_PULSES} pulses");
                    self.entry.text.clear();
                    return;
                }
                let name = std::mem::take(&mut self.entry.text);
                let kind = match name.as_str() {
                    "" => self
                        .current_gate()
                        .map_or(GateKind::Single, |gate| gate.kind),
                    name => match GateKind::from_name(name) {
                        Some(kind) => kind,
                        None => {
                            self.status = format!("{name} isn't a gate: SGL, MLT, HLD or RST");
                            return;
                        }
                    },
                };
                let length = self.current_gate().and_then(|gate| gate.length);
                self.set_field(FieldValue::Gate(Some(Gate {
                    kind,
                    pulses,
                    length,
                })));
                self.advance();
            }
            KeyCode::Char(c)
                if !key.ctrl && c.is_ascii_alphabetic() && self.entry.text.len() < 3 =>
            {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 3 && GateKind::from_name(&self.entry.text).is_none() {
                    self.status = format!("{} isn't a gate: SGL, MLT, HLD or RST", self.entry.text);
                    self.entry.text.clear();
                }
            }
            _ => {}
        }
    }

    /// Two hex digits set how much of its last pulse the row's note lasts, and auto-jump moves on
    /// by the grid.
    fn insert_length(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_length(None);
                }
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    if let Ok(length) = u8::from_str_radix(&text, 16) {
                        self.set_length(Some(length));
                    }
                    self.advance();
                }
            }
            _ => {}
        }
    }

    /// Give the gate cell under the cursor a length of its own, keeping its gate and pulses, or
    /// take it away so the note plays at the track's `Gate`. An empty cell gets one pulse of `SGL`
    /// to hold a length, which is how it played.
    pub(crate) fn set_length(&mut self, length: Option<u8>) {
        let gate = match self.current_gate() {
            Some(gate) => Some(Gate { length, ..gate }),
            None if length.is_some() => Some(Gate {
                length,
                ..Gate::default()
            }),
            None => None,
        };
        self.set_field(FieldValue::Gate(gate));
    }

    /// A nudge's 24ths as one hex digit, `0` to `B`, late unless `-` comes first, and `+` or `=`
    /// first says late again. `0` puts the row back on the grid, and auto-jump moves on by the
    /// grid once the digit's in.
    fn insert_nudge(&mut self, key: Key) {
        match key.code {
            KeyCode::Esc => self.cancel_entry(),
            KeyCode::Backspace => {
                if self.entry.text.pop().is_none() {
                    self.back_for_clearing();
                    self.set_nudge(0);
                }
            }
            KeyCode::Char(sign @ ('-' | '+' | '=')) if !key.ctrl => {
                self.entry.text = if sign == '-' { "-" } else { "+" }.to_owned();
            }
            KeyCode::Char(c) if !key.ctrl && c.is_ascii_hexdigit() => {
                let amount = c.to_digit(16).unwrap_or(0) as i8;
                if amount > MAX_NUDGE {
                    self.status =
                        format!("A nudge goes up to {MAX_NUDGE:X}, just under half a step");
                    return;
                }
                let early = std::mem::take(&mut self.entry.text) == "-";
                self.set_nudge(if early { -amount } else { amount });
                self.advance();
            }
            _ => {}
        }
    }

    /// Nudge the row under the cursor, or put it back on the grid with 0.
    pub(crate) fn set_nudge(&mut self, nudge: i8) {
        self.set_field(FieldValue::Nudge(nudge));
    }

    /// How far the row under the cursor is nudged, in 24ths of a step.
    pub(crate) fn current_nudge(&self) -> i8 {
        self.phrase_here()
            .and_then(|phrase| phrase.steps().get(self.cursor.step))
            .map_or(0, |step| step.nudge)
    }

    /// The gate cell under the cursor.
    pub(crate) fn current_gate(&self) -> Option<Gate> {
        let phrase = self.phrase_here()?;
        phrase.steps().get(self.cursor.step)?.gate
    }

    fn advance(&mut self) {
        if self.mode == Mode::Replace {
            self.finish_replace();
            return;
        }
        if self.auto_jump {
            self.move_step(self.song().skip as isize);
        }
    }

    /// Before `Backspace` clears: with auto-jump, back one grid cell to the last entry, and
    /// without, it clears where it is.
    fn back_for_clearing(&mut self) {
        if self.auto_jump {
            self.move_step(-(self.song().skip as isize));
        }
    }

    /// `;` in Insert: whether entering moves on by the grid. Like the scale, it isn't an edit to
    /// undo.
    pub(crate) fn toggle_auto_jump(&mut self) {
        self.auto_jump = !self.auto_jump;
        self.dirty = true;
        self.status = if self.auto_jump {
            "Auto-jump on: each entry moves on by the grid".to_owned()
        } else {
            "Auto-jump off: entering stays put".to_owned()
        };
    }

    fn write_note(&mut self, note: Note) {
        let Some(phrase) = self.phrase_id() else {
            self.status = "This track has no phrase here".to_owned();
            return;
        };
        self.apply(&Action::SetNote {
            track: self.cursor.track,
            phrase,
            step: self.cursor.step,
            note,
        });
    }

    /// Run an action and make the result current.
    pub fn apply(&mut self, action: &Action) {
        match edit(self.song(), action) {
            Ok(next) => {
                self.history.commit(next);
                self.dirty = true;
                self.publish();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Run a command line.
    pub fn line(&mut self, line: Line) {
        self.status.clear();
        match line {
            Line::Quit { force } => self.quit(force),
            Line::Write(None) if self.folder.is_none() => {
                self.open_naming_explorer(Purpose::Save, "w", String::new());
            }
            Line::Write(path) => self.write(path),
            Line::Edit(Some(path)) => self.open(&path),
            Line::Edit(None) => self.open_project_explorer(),
            Line::New(Some(path)) => self.new_project(&path),
            Line::New(None) if self.dirty => {
                self.status = "Unsaved changes: use :w to save first".to_owned();
            }
            Line::New(None) => self.open_naming_explorer(Purpose::Save, "new", String::new()),
            Line::Sample(Some(path)) => self.sample(&path),
            Line::Sample(None) => self.open_default_sample_picker(),
            Line::Kit(Some(name)) => self.use_kit(&self.kit_path(&name)),
            Line::KitTracks(name) => self.load_kit(&self.kit_path(&name)),
            Line::Kit(None) => self.open_kit_picker(),
            Line::Name(name) => self.apply(&Action::SetTrackName {
                track: self.cursor.track,
                name: name.unwrap_or_default(),
            }),
            Line::Bpm(millibpm) => self.apply(&Action::SetMilliBpm(millibpm)),
            Line::TrackAdd(count) => {
                let was = self.song().sound_tracks();
                self.apply(&Action::AddTracks(count));
                self.fit_levels();
                // A cursor on a fixed track stays on it.
                if self.cursor.track >= was {
                    self.cursor.track += self.song().sound_tracks() - was;
                }
            }
            Line::TrackDel => {
                let was = self.song().sound_tracks();
                self.apply(&Action::RemoveLastTrack);
                self.fit_levels();
                let now = self.song().sound_tracks();
                if self.cursor.track >= was {
                    self.cursor.track -= was - now;
                } else if self.cursor.track >= now {
                    self.cursor.track = now - 1;
                    self.cursor.slot = 0;
                    self.clamp_cursor();
                }
            }
            Line::Chain(chain) => self.set_track_chain(chain),
            Line::Render {
                path: None,
                rows,
                tail,
                stems,
            } => {
                // The flags are kept, after the name.
                let mut after = String::new();
                if let Some(rows) = rows {
                    after.push_str(&format!(" --rows {rows}"));
                }
                if tail {
                    after.push_str(" --tail");
                }
                if stems {
                    after.push_str(" --stems");
                }
                self.open_naming_explorer(Purpose::Render, "render", after);
            }
            Line::Render {
                path: Some(path),
                rows,
                tail,
                stems: true,
            } => self.render_stems(&path, rows, tail),
            Line::Render {
                path: Some(path),
                rows,
                tail,
                ..
            } => self.render(&path, rows, tail),
            Line::Double => self.double(),
            Line::SetSkip(size) => self.apply(&Action::SetSkipSize(size)),
            Line::SetScale(kind) => self.set_scale(Scale {
                on: true,
                kind,
                ..self.scale
            }),
            Line::SetRoot(root) => self.set_scale(Scale {
                on: true,
                root,
                ..self.scale
            }),
            Line::ScaleOn(on) => self.set_scale(Scale { on, ..self.scale }),
            Line::PickScale => self.open_scale_picker(),
            Line::FitToScale => self.fit_to_scale(),
            Line::SetLimiter(on) => {
                self.apply(&Action::SetLimiter(on));
                self.status = if on {
                    "Limiter on".to_owned()
                } else {
                    "Limiter off: renders keep levels over 0, and the output still clips at 0"
                        .to_owned()
                };
            }
            Line::Seed(seed) => self.apply(&Action::SetSeed(seed)),
            Line::Groove(groove) => {
                self.apply(&Action::SetGroove(groove));
                self.status = format!("The project plays with {groove}");
            }
            Line::TrackGroove(groove) => self.set_track_groove(groove),
            Line::PickGroove { track } => self.open_groove_picker(track),
            Line::Reset(reset) => self.apply(&Action::SetReset {
                section: self.cursor.section,
                reset,
            }),
            Line::Loop => self.toggle_loop(),
            Line::Launch(Some(launch)) => {
                self.apply(&Action::SetLaunch(launch));
                self.status = format!("Launches start {}", launch.when());
            }
            Line::Launch(None) => {
                let launch = self.song().launch;
                self.status = format!("Launches start {}", launch.when());
            }
            Line::Label(label) => self.label_chain(label),
            Line::TrackLevel(level) => self.apply(&Action::SetTrackLevel {
                track: self.cursor.track,
                level,
            }),
            Line::Mute => self.toggle_mute(),
            Line::Solo => self.toggle_solo(),
            Line::Len(length) if self.lane_slot().is_some() => {
                let slot = self.lane_slot().unwrap_or(0);
                let start = self.lane_here(slot).start;
                self.set_lane(slot, Some(Lane { start, length }));
            }
            Line::Lane(length) => match self.fx_column() {
                Some(slot) => {
                    let start = self.cursor.step;
                    self.set_lane(slot, length.map(|length| Lane { start, length }));
                }
                None => self.status = ":lane works on an FX column".to_owned(),
            },
            Line::Order(Some(order)) => self.set_order(order),
            Line::Order(None) => self.open_order_picker(),
            Line::Len(length) => {
                if let Some(phrase) = self.phrase_id() {
                    self.apply(&Action::SetLength {
                        track: self.cursor.track,
                        phrase,
                        length,
                    });
                    self.clamp_cursor();
                }
            }
        }
    }

    /// Render what is in memory, with the live mutes and at the device's rate. The screen waits,
    /// and audio keeps playing on its own thread.
    fn render(&mut self, path: &Path, rows: Option<usize>, tail: bool) {
        let version = Published::new(self.song_to_save(), Arc::clone(&self.samples));
        let samples = render_loop(Arc::new(version), self.rate, RenderOptions { rows, tail });
        self.status = match write_wav(path, self.rate, &samples) {
            Ok(()) => format!(
                "Rendered {} ({:.2} s)",
                path.display(),
                samples.len() as f64 / 2.0 / f64::from(self.rate)
            ),
            Err(error) => format!("Couldn't write {}: {error}", path.display()),
        };
    }

    /// Render one file per track that would be heard, into a folder.
    fn render_stems(&mut self, folder: &Path, rows: Option<usize>, tail: bool) {
        let version = Published::new(self.song_to_save(), Arc::clone(&self.samples));
        let options = RenderOptions { rows, tail };
        self.status = match crate::run::write_stems(&version, folder, self.rate, options) {
            Ok(count) => crate::run::stems_written(count, folder),
            Err(error) => format!("Couldn't write {}: {error}", folder.display()),
        };
    }

    fn quit(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = "Unsaved changes: use :w to save, or :q! to quit anyway".to_owned();
        } else {
            self.forget_autosave();
            self.should_quit = true;
        }
    }

    fn write(&mut self, path: Option<PathBuf>) {
        let same = path
            .as_deref()
            .is_none_or(|p| Some(p) == self.folder.as_deref());
        let result = if same {
            match self.folder.clone() {
                Some(folder) => {
                    project::save_song(&folder, &self.song_to_save()).map(|()| (folder, None))
                }
                None => {
                    self.status = "This project has no folder yet: use :w path".to_owned();
                    return;
                }
            }
        } else {
            // A project saved to a new folder is named after it, which the line saving it starts
            // with, so the two only differ if another name is typed there.
            let target = path.expect("a different path is present");
            let mut song = Song::clone(&self.song_to_save());
            song.name = folder_name(&target);
            project::save_as(self.folder.as_deref(), &target, &song)
                .map(|saved| (target, Some(saved)))
        };
        match result {
            Ok((folder, saved)) => {
                if let Some(saved) = saved {
                    self.replace_song(saved, Some(folder.clone()));
                }
                self.folder = Some(folder.clone());
                self.dirty = false;
                self.forget_autosave();
                self.status = format!("Saved {}", folder.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn open(&mut self, path: &Path) {
        if self.dirty {
            self.status = "Unsaved changes: use :w to save first".to_owned();
            return;
        }
        match project::load_song(path) {
            Ok(song) => {
                self.handle.send(Command::Return);
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                if self.status.is_empty() {
                    self.status = format!("Opened {}", path.display());
                }
                self.offer_restore();
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn new_project(&mut self, path: &Path) {
        if self.dirty {
            self.status = "Unsaved changes: use :w to save first".to_owned();
            return;
        }
        let song = Song {
            name: folder_name(path),
            ..Song::new(DEFAULT_TRACKS)
        };
        match project::create_project(path, &song) {
            Ok(()) => {
                self.handle.send(Command::Return);
                self.samples = Arc::new(SamplePool::default());
                self.replace_song(song, Some(path.to_path_buf()));
                self.dirty = false;
                self.status = format!("Created {}", path.display());
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    /// Replace the song and undo history, loading any samples it needs.
    pub(crate) fn replace_song(&mut self, mut song: Song, folder: Option<PathBuf>) {
        name_song(&mut song, folder.as_deref(), &mut self.names);
        let mut pool = SamplePool::clone(&self.samples);
        let errors = project::load_samples(folder.as_deref(), &song, self.rate, &mut pool);
        self.samples = Arc::new(pool);
        self.levels = live_from(&song);
        self.scale = song.scale;
        self.auto_jump = song.auto_jump;
        self.send_live();
        self.history = History::new(Arc::new(song));
        self.folder = folder;
        self.cursor = Cursor {
            step: self.cursor.step,
            ..self.cursor
        };
        if self.phrase_id().is_none() {
            self.cursor.track = 0;
            self.cursor.slot = 0;
        }
        self.clamp_cursor();
        if let Some(first) = errors.first() {
            self.status = first.to_string();
        }
        self.publish();
    }

    /// How a song refers to a sample: where it is, or relative to the project if it's inside,
    /// so a bundled project can be moved. `:w` to a new folder is what copies samples.
    pub(crate) fn store_sample(&self, source: &Path) -> Result<String, ProjectError> {
        let full = source.canonicalize().map_err(|error| ProjectError::Io {
            path: source.display().to_string(),
            source: error,
        })?;
        let project = self.folder.as_deref().and_then(|f| f.canonicalize().ok());
        let inside = project.as_deref().and_then(|f| full.strip_prefix(f).ok());
        Ok(inside.unwrap_or(&full).display().to_string())
    }

    pub(crate) fn sample(&mut self, source: &Path) {
        self.sample_named(source, None);
    }

    /// Put a sample on the current track and name the track after it, if there's a name, as one
    /// undo step.
    pub(crate) fn sample_named(&mut self, source: &Path, name: Option<&str>) {
        let track = self.cursor.track;
        let more = name
            .map(|name| Action::SetTrackName {
                track,
                name: name.to_owned(),
            })
            .into_iter()
            .collect();
        self.sample_with(source, more);
    }

    /// Put a sample on the current track, with other changes to the track, as one undo step.
    pub(crate) fn sample_with(&mut self, source: &Path, more: Vec<Action>) {
        if source.is_dir() {
            self.open_sample_folder(source);
            return;
        }
        let stored = match self.store_sample(source) {
            Ok(stored) => stored,
            Err(error) => {
                self.status = error.to_string();
                return;
            }
        };
        let resolved = project::resolve_sample(self.folder.as_deref(), &stored);
        match load_sample(&resolved, self.rate) {
            Ok(data) => {
                let mut pool = SamplePool::clone(&self.samples);
                pool.insert(&stored, data);
                self.samples = Arc::new(pool);
                let track = self.cursor.track;
                let mut actions = vec![Action::SetTrackSample {
                    track,
                    sample: Some(stored),
                }];
                actions.extend(more);
                self.apply(&Action::Batch(actions));
                if self.status.is_empty() {
                    self.status = format!("Track {:02} sample loaded", self.cursor.track + 1);
                }
            }
            Err(error) => self.status = error.to_string(),
        }
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
fn which_key_due(since: Option<Instant>, now: Instant, delay: Duration) -> bool {
    match since {
        Some(started) => now.saturating_duration_since(started) >= delay,
        None => false,
    }
}

fn live_from(song: &Song) -> Vec<TrackLive> {
    song.tracks
        .iter()
        .map(|t| TrackLive {
            mute: t.mute,
            solo: t.solo,
        })
        .collect()
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
