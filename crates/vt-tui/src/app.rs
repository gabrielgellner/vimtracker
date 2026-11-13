//! Application state, and what each key does. It runs without a terminal, so tests can drive it.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::accent_slide::{HeldFlag, Written};
use crate::analysing::Analyst;
use crate::audio::{AudioChoice, DeviceInfo, LoadReadout, SlowdownReadout};
use crate::config::Settings;
use crate::exploring::Naming;
use crate::folding::Folds;
use crate::glyphs;
use crate::loading::Loader;
use crate::meter::Meter;
use crate::places::RecentPlaces;
use crate::views::picker::{PREVIEW_SAMPLE_HEIGHT, PREVIEW_SAMPLE_WIDTH};
use ratatui::crossterm::event::KeyCode;
use vt_engine::engine::{PLAYHEADS, Playhead, SectionLoop, TrackPlace, Transport};
use vt_engine::link::{Command, Handle, JamState};
use vt_engine::published::Published;
use vt_engine::track_state::{Faders, LiveValues};
use vt_model::action::{Action, edit};
use vt_model::constants::{DEFAULT_NOTE_VELOCITY, MAX_TRACKS, TICKS_PER_STEP};
use vt_model::groove::GrooveId;
use vt_model::history::History;
use vt_model::kit::KitEntry;
use vt_model::modulation::MOD_SLOTS;
use vt_model::names::Names;
use vt_model::order::Dealer;
use vt_model::samples::SamplePool;
use vt_model::scale::Scale;
pub use vt_model::selection::Field;
use vt_model::selection::{Block, Extent, Motion, Scope, StepRange};
use vt_model::song::{ChainEntry, Phrase, Section, Song, Track};
use vt_model::step::{FxCommand, MAX_VELOCITY};

use crate::grammar::{Adjustments, Change, SelectionPlace};
use crate::guides::Guide;
use crate::input::bindings::{Op, Table};
use crate::input::command_history::CommandHistory;
use crate::input::completion::CommandMenu;
use crate::input::keys::Key;
use crate::input::mode::{Mode, View};
use crate::input::picker::Picker;
use crate::jumping::FieldJump;
use crate::recording::Recording;
use crate::shaping::ShapePanel;
use crate::track_page::{MIXER_START, TakenInsert, TrackPage};
use crate::tree::{ChainColumn, Place};

mod commands;
mod cursor;
mod entry;
mod keys;
mod loops;
mod tracks;

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

/// How long the playheads stay without a report from the audio thread, after which audio has
/// stopped and nothing is playing.
const PLAYHEADS_KEPT: Duration = Duration::from_millis(250);

/// How many versions sent are kept for a report to be of. The callback takes each within a block,
/// so only a few are ever waiting; these are for when it has stopped.
const SENT_KEPT: usize = 64;

/// Why `:e` and `:new` refused, and the ways on.
pub(crate) const UNSAVED_OPEN: &str = "Unsaved changes: :w to save, or :e! to discard them";
const UNSAVED_NEW: &str = "Unsaved changes: :w to save, or :new! to discard them";

/// What a key started that lasts while it's down, and is undone or ended when it's let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Momentary {
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
    /// The slowdown check's time at full speed, measured as the program starts, for each stream
    /// it opens.
    pub full_speed: Duration,
}

/// The whole program's state.
pub struct App {
    pub history: History,
    pub folder: Option<PathBuf>,
    pub dirty: bool,
    pub samples: Arc<SamplePool>,
    pub handle: Handle,
    /// The version last sent to the audio thread, whose bank entries the next keeps where they're
    /// the same.
    published: Option<Arc<Published>>,
    /// The songs of the versions sent that a report could still be of, by their ids, oldest
    /// first, and the song the live values were worked out from, so a bar draws what's playing
    /// against the setting it was reported with (#421). The engine starts on the song the app
    /// starts with, before any version is sent.
    sent: VecDeque<(u64, Arc<Song>)>,
    live_song: Option<Arc<Song>>,
    pub rate: u32,
    /// The audio output as it plays, and what the audio page chooses from.
    pub audio: AudioState,
    /// A different output the audio page has chosen, for the main loop to open.
    pub audio_request: Option<AudioChoice>,
    /// The audio page's row: device, rate or buffer.
    pub audio_row: usize,
    /// The Roto-Control: whether it's on, the link to it, and what it shows.
    pub roto: crate::roto::Roto,
    /// The mouse wheel as a knob on the cursor's setting, while its setting is on.
    pub wheel: crate::wheel::Wheel,
    /// Where this machine keeps its kits and samples.
    pub settings: Settings,
    /// The sample library the sample explorer lists, such as Ableton Live's, if this machine has
    /// one: found as the program starts, and none in tests.
    pub library: Option<PathBuf>,
    pub mode: Mode,
    pub view: View,
    /// The mixer's row.
    pub mixer_row: usize,
    pub song_column: usize,
    /// The section the engine is on.
    pub playing_section: usize,
    /// The sections being kept playing, set with `:loop`.
    pub looping: Option<SectionLoop>,
    /// Whether that loop is waiting for the launch grid's next line, as the engine last said.
    pub loop_waiting: bool,
    /// Where a selection of sections in the song view started, while `V` selects them.
    pub section_anchor: Option<usize>,
    /// What each track is launched onto and cued to, as the engine last said.
    pub jam: Vec<JamState>,
    /// The track page: which of its pages shows, and where its cursor is.
    pub track_page: TrackPage,
    /// The terminal's images of the Mod page's slots, an envelope's shape or an LFO's cycle each,
    /// by slot.
    pub mod_images: Option<[u32; MOD_SLOTS]>,
    /// Where each LFO's cycle was last drawn, for the terminal to place its dot on.
    pub lfo_cycles_at: crate::views::track::CyclesAt,
    /// The id of the image of the filter's curve, likewise.
    pub filter_image: Option<u32>,
    /// The id of the image of the waveform of the sample the cursor's track plays, if it has one.
    pub waveform_image: Option<u32>,
    /// The ids of the images of the FX page's slots, for those that have a drawing.
    pub slot_images: [Option<u32>; vt_model::insert::INSERTS],
    /// The images of the Gen page's previews the terminal has, by each generator's place in its
    /// phrase's list.
    pub gen_images: Vec<Option<u32>>,
    /// Where each preview the Gen page drew is, its generator's place and its picture's top left,
    /// for the terminal to place the row playing on.
    pub gen_panels_at: Vec<(usize, (u16, u16))>,
    /// Where the sample page is, and the ids of the images of its waveform and its overview.
    pub sample_page: crate::sample_page::SamplePage,
    pub sample_images: Option<(u32, u32)>,
    /// What the cursor's track is playing with right now, and the track the engine reports.
    pub live: Option<LiveValues>,
    /// What each mod slot moved each row's note by on `Pitch` the last time it was heard, by
    /// track, phrase and row, in semitones, as the engine reports them, for `Space g A` (#569).
    pub(crate) heard_pitches: HashMap<(usize, u32, usize), [f32; MOD_SLOTS]>,
    /// Every track's faders as they're heard: where the steps have them, and which a hand has
    /// taken over, as the engine last said, with what's been asked of it since (decision 174).
    pub(crate) faders: [Faders; MAX_TRACKS],
    pub(crate) fader_wishes: Vec<crate::automating::Wish>,
    /// Where each voice playing a sample through the cursor's track is in it, for the sample
    /// page's playheads.
    pub playheads: [Option<Playhead>; PLAYHEADS],
    watched: usize,
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
    /// Where the latest jump left from, for `''` to go back to, as Vim's previous context mark.
    pub(crate) jumped_from: Option<Place>,
    pub(crate) last_autosave: Option<Instant>,
    pub(crate) autosaved: Option<Arc<Song>>,
    pub cursor: Cursor,
    /// The octave the keys play and enter notes in, as note names number it: 3, where `a` is
    /// `C-3`, middle C.
    pub octave: i8,
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
    /// The time of the frame or key being handled, which the playhead is carried on to from the
    /// engine's last report, so what's recorded goes by the times the app is given and never
    /// reads the clock itself.
    pub(crate) now: Instant,
    /// Live recording, while it's armed.
    pub(crate) recording: Option<Recording>,
    /// The main output's level.
    pub meter: Meter,
    /// How far each FX slot of the cursor's track has turned down lately, in dB, for the mark its
    /// gain reduction meter holds at the deepest.
    pub slot_reduction: [crate::meter::Held; vt_model::insert::INSERTS],
    /// The cursor's track's level right after its trim, each side's in dB, for the meter in its
    /// Output device: where it is now and the highest it was lately.
    pub after_trim: [crate::meter::Held; 2],
    /// The main output's recent sound, for the scope above the meter.
    pub scope: crate::scope::Scope,
    /// The id of the scope's image, and whether the last screen drawn had room for it.
    pub scope_image: Option<u32>,
    pub scope_shown: bool,
    /// Each track's level, for the mixer.
    pub track_meters: Vec<Meter>,
    /// The CPU figure on the status line.
    pub cpu: LoadReadout,
    /// How slowed the audio thread is, for the leaf beside it.
    pub slowdown: SlowdownReadout,
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
    /// Whether a realign is waiting for the launch grid, as the engine last said.
    pub realigning: bool,
    /// Whether each track's own realign is waiting for the launch grid, as the engine last said.
    pub realigning_tracks: [bool; MAX_TRACKS],
    /// The key last pressed, as it is on the keyboard, until it's let go.
    key_down: Option<KeyCode>,
    /// Whether the key that's down ignores its repeats, as a transport key and a held note do.
    pub(crate) repeats_ignored: bool,
    /// Whether the key being acted on is the terminal repeating one that's held, for the sample
    /// page's `h` and `l`, which speed up as they're held.
    pub(crate) repeating: bool,
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
    pub(crate) started: Option<Momentary>,
    pub entry: Entry,
    /// The first step on screen, and how many lines fit.
    pub top: usize,
    pub view_height: usize,
    /// The phrase view is collapsed to a line a row, rather than laid out in time (decision 122).
    pub rows_collapsed: bool,
    /// The phrase view shows the row inspector right of the overview, for this session
    /// (decision 255).
    pub inspector: bool,
    /// Which groups are folded in the song view, the mixer and the phrase view's overview, for
    /// this song and this session (decision 128).
    pub folds: Folds,
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
    /// Whether `'` or `m` on the sample page waits for a letter: of the slice to go to, or to
    /// name the slice under the cursor with.
    pub(crate) awaiting_mark: Option<crate::marking::MarkWait>,
    pub(crate) registers: HashMap<char, Block>,
    pub(crate) anchor: Option<(usize, Scope)>,
    /// How far a selection reaches across tracks from the cursor's.
    pub(crate) anchor_track: Option<usize>,
    /// What the changes kept on the selection have moved its values by, for the status line.
    pub(crate) adjustments: Adjustments,
    /// The selection a replace across it comes back to, once its value is in or it's cancelled.
    pub(crate) replace_returns_to: Option<SelectionPlace>,
    /// The open shape panel.
    pub shape: Option<ShapePanel>,
    /// The open mapping view of a macro.
    pub mapping: Option<crate::macroing::MappingView>,
    /// A selection to shape once the command for its empty FX column is chosen, and its field.
    pub(crate) shape_command: Option<(Selection, Field)>,
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
    /// Analyses the bank's files, off this thread, for normalising, `:trim` and the waveform.
    pub analyst: Analyst,
    /// When the loader or the analyst started on what they're doing, while they are, for the
    /// spinner on the status line.
    pub(crate) work_since: Option<Instant>,
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
    /// Whether the project the explorer opens throws away unsaved changes, as after `:e!`.
    pub(crate) opening_discards: bool,
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
            published: None,
            sent: VecDeque::new(),
            live_song: None,
            rate,
            mode: Mode::Normal,
            view: View::Phrase,
            mixer_row: MIXER_START,
            song_column: 0,
            playing_section: 0,
            looping: None,
            loop_waiting: false,
            section_anchor: None,
            jam: vec![JamState::default(); MAX_TRACKS],
            track_page: TrackPage::default(),
            mod_images: None,
            lfo_cycles_at: Default::default(),
            filter_image: None,
            waveform_image: None,
            slot_images: [None; vt_model::insert::INSERTS],
            gen_images: Vec::new(),
            gen_panels_at: Vec::new(),
            sample_page: crate::sample_page::SamplePage::default(),
            sample_images: None,
            live: None,
            heard_pitches: HashMap::new(),
            faders: [Faders::default(); MAX_TRACKS],
            fader_wishes: Vec::new(),
            playheads: [None; PLAYHEADS],
            watched: 0,
            project_row: 0,
            project_field: 0,
            project_fine: false,
            project_wanted: None,
            groove_shown: None,
            audio: AudioState::default(),
            audio_request: None,
            settings: Settings::none(),
            library: None,
            audio_row: 0,
            roto: crate::roto::Roto::default(),
            wheel: crate::wheel::Wheel::default(),
            recent_views: View::ALL.to_vec(),
            picker: None,
            chain_column: ChainColumn::Phrase,
            yanked_row: None,
            jumps: Vec::new(),
            jump_index: 0,
            jumped_from: None,
            last_autosave: None,
            autosaved: None,
            cursor: Cursor {
                section: 0,
                track: 0,
                slot: 0,
                step: 0,
                field: Field::Note,
            },
            octave: 3,
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
            now: Instant::now(),
            recording: None,
            meter: Meter::default(),
            slot_reduction: [crate::meter::Held::new(0.0); vt_model::insert::INSERTS],
            after_trim: [crate::meter::Held::new(crate::meter::FLOOR_DB); 2],
            scope: crate::scope::Scope::new(rate),
            scope_image: None,
            scope_shown: true,
            track_meters: vec![Meter::default(); MAX_TRACKS],
            cpu: LoadReadout::default(),
            slowdown: SlowdownReadout::default(),
            last_tick: Instant::now(),
            should_quit: false,
            levels,
            scale,
            auto_jump,
            fill: false,
            realigning: false,
            realigning_tracks: [false; MAX_TRACKS],
            key_down: None,
            repeats_ignored: false,
            repeating: false,
            held_notes: Vec::new(),
            held_flags: Vec::new(),
            last_written: None,
            momentary: None,
            started: None,
            entry: Entry::default(),
            top: 0,
            view_height: 16,
            rows_collapsed: false,
            inspector: false,
            folds: Folds::default(),
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
            awaiting_mark: None,
            registers: HashMap::new(),
            anchor: None,
            anchor_track: None,
            adjustments: Adjustments::default(),
            replace_returns_to: None,
            shape: None,
            mapping: None,
            shape_command: None,
            field_jump: None,
            taken_insert: None,
            shape_seed: 0,
            kit: Vec::new(),
            kit_file: None,
            loader: Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT)),
            analyst: Analyst::new(None),
            work_since: None,
            audition_files: true,
            file_wanted: None,
            file_playing: None,
            recent_places: RecentPlaces::default(),
            naming: None,
            opening_discards: false,
            names,
            replacing_selection: None,
            selection_scope: None,
            last_selection: None,
            last_change: None,
        };
        app.live_song = Some(Arc::clone(app.song()));
        app.send_live();
        app
    }

    /// Play through a new engine, on another output or at another rate. The song, undo, mutes and
    /// solos stay, and the transport is stopped, as the new engine starts that way.
    pub fn reconnect(&mut self, handle: Handle, samples: Arc<SamplePool>, rate: u32) {
        self.handle = handle;
        // An analysis is of the file at its own rate, so it's the same at the new one.
        let mut samples = SamplePool::clone(&samples);
        samples.keep_analyses(&self.samples);
        self.samples = Arc::new(samples);
        self.rate = rate;
        self.loader = Loader::new(rate, (PREVIEW_SAMPLE_WIDTH, PREVIEW_SAMPLE_HEIGHT));
        self.file_playing = None;
        self.scope = crate::scope::Scope::new(rate);
        self.transport = Transport::Stopped;
        self.playhead = 0;
        self.places = [TrackPlace::default(); MAX_TRACKS];
        self.live = None;
        // The new engine starts on the song as it is.
        self.sent.clear();
        self.live_song = Some(Arc::clone(self.song()));
        self.jam = vec![JamState::default(); MAX_TRACKS];
        // Sent again when the next frame finds it differs.
        self.watched = usize::MAX;
        self.send_live();
        // The new engine's first version was built before the analyses were kept.
        self.publish();
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
                Momentary::Fill { .. }
                | Momentary::Audition
                | Momentary::AuditionNote { .. }
                | Momentary::SampleAudition { .. } => {}
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
        // Rows above it last their pulses. In another order, the walk is wherever it is at that
        // step, worked out from the count as everything else is.
        let time = self.phrase_here().map_or(row, |phrase| phrase.time_to(row));
        self.slot_start() + time
    }

    /// Put the cursor on the first step that plays, at the top of the screen.
    pub(crate) fn go_to_loop_start(&mut self) {
        let start = self.extent().start;
        self.cursor.step = start;
        self.top = start;
    }

    /// Send the current version to the audio thread. Only the bank entries that changed are
    /// worked out again.
    pub fn publish(&mut self) {
        let version = Arc::new(Published::after(
            Arc::clone(self.song()),
            Arc::clone(&self.samples),
            self.published.as_deref(),
        ));
        self.published = Some(Arc::clone(&version));
        self.sent.push_back((version.id, Arc::clone(&version.song)));
        if self.sent.len() > SENT_KEPT {
            self.sent.pop_front();
        }
        self.handle.publish(version);
        self.analyse_bank();
    }

    /// Note which version a report is of: the versions before it are done with, as the callback
    /// never goes back to one, and its song is what the live values were worked out from. A
    /// report of a version before any still kept, such as the one the engine started on, keeps
    /// the song it was last of.
    fn follow_version(&mut self, version: u64) {
        while self.sent.front().is_some_and(|(id, _)| *id < version) {
            self.sent.pop_front();
        }
        if let Some((_, song)) = self.sent.front().filter(|(id, _)| *id == version) {
            self.live_song = Some(Arc::clone(song));
        }
    }

    /// The cursor's track as the latest report's live values were worked out from, which is a
    /// frame or so behind the song while it's being changed.
    pub fn reported_track(&self) -> Option<&Track> {
        let song = self.live_song.as_ref().unwrap_or_else(|| self.song());
        song.tracks.get(self.cursor.track)
    }

    /// Do the work due each frame: send waiting versions and read the callback's report.
    pub fn tick(&mut self) {
        self.tick_at(Instant::now());
    }

    /// Do a frame's work as if it's `now`, so the screenshots can step the meters by a fixed time.
    pub fn tick_at(&mut self, now: Instant) {
        self.now = now;
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
            self.realigning = report.realigning;
            self.loop_waiting = report.loop_waiting;
            self.realigning_tracks = report.realigning_tracks;
            self.live = (report.watched == track).then_some(report.live);
            self.follow_version(report.version);
            self.follow_faders(&report.faders, now);
            self.playheads = if report.watched == track {
                report.playheads
            } else {
                [None; PLAYHEADS]
            };
        } else if now.saturating_duration_since(self.report_at) > PLAYHEADS_KEPT {
            // Audio has stopped, so nothing is playing.
            self.playheads = [None; PLAYHEADS];
        }
        for (slot, held) in self.slot_reduction.iter_mut().enumerate() {
            held.update(self.live.map_or(0.0, |live| live.reduction[slot]), seconds);
        }
        // Only what was reported this frame, for this track, so it falls once nothing is.
        let trimmed = report
            .filter(|report| report.watched == track)
            .map_or([0.0; 2], |report| report.live.after_trim);
        for (held, level) in self.after_trim.iter_mut().zip(trimmed) {
            held.update(crate::meter::to_db(level), seconds);
        }
        while let Some((track, pitch)) = self.handle.note_pitch() {
            let row = (track, pitch.row.phrase, pitch.row.row);
            self.heard_pitches.insert(row, pitch.moves);
        }
        while let Some((track, state)) = self.handle.jam_change() {
            // A mute or solo waiting for the grid has landed, so the live controls follow it.
            if state.switches_landed != self.jam[track].switches_landed
                && let Some(live) = self.levels.get_mut(track)
            {
                live.mute = state.mute;
                live.solo = state.solo;
                self.dirty = true;
            }
            self.jam[track] = state;
        }
        self.take_loaded();
        self.take_analyses();
        self.tick_roto();
        self.wheel_frame(now);
        self.follow_recording();
        let working = self.loader.is_waiting() || self.analyst.is_waiting();
        self.work_since = working.then(|| self.work_since.unwrap_or(now));
    }

    /// The work going on in the background, by name, and the spinner's turn for it. Only work
    /// that has taken a moment shows, so a file read as the explorer's cursor passes doesn't flash
    /// on the status line, and nothing shows once it's done, so an idle screen stays still.
    pub fn background_work(&self) -> Option<(&'static str, char)> {
        let since = self.work_since?;
        let elapsed = self.now.saturating_duration_since(since);
        if elapsed < glyphs::SPINNER_AFTER {
            return None;
        }
        let work = if self.analyst.is_waiting() {
            "Analysing"
        } else {
            "Loading"
        };
        Some((work, glyphs::spinner(elapsed)))
    }

    /// The terminal window's title: `vimtracker` and the project's name. kitty's
    /// `--when-focus-on title:vimtracker` matches it, so its `Cmd` shortcuts can be left to this
    /// window alone.
    pub fn window_title(&self) -> String {
        format!("vimtracker: {}", self.project_name())
    }

    /// Run an action and make the result current. A fader it moves that the steps drive is
    /// taken over by the hand that moved it.
    pub fn apply(&mut self, action: &Action) {
        match edit(self.song(), action) {
            Ok(next) => {
                self.say_applied(&next);
                self.take_over_moved(action);
                self.history.commit(next);
                self.dirty = true;
                self.publish();
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
