//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::accumulator::{Accumulation, Moves, Walk};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::constants::{FX_SLOTS, MAX_TRACKS, TRACK_ENVELOPES, UNITY_LEVEL};
use vt_model::envelope::Envelope;
use vt_model::groove::Groove;
use vt_model::insert::{INSERTS, Insert, MAX_PARAMS};
use vt_model::lfo::{
    LFOS_PER_TRACK, MAX_LFO_DEPTH, depth_of, depth_value, phase_of, phase_value, wave,
};
use vt_model::settings::{
    Effect, MAX_MACHINE_SETTINGS, Owner, SettingId, gate_delta, gate_share, glide_ms,
};
use vt_model::song::{DEFAULT_ACCENT, FilterMode, FilterType, SynthVoice, Track, level_gain};
use vt_model::step::{FxSlot, Mark};

use crate::dsp::distort::InsertState;
use crate::dsp::filter::{Channel, Coefficients, Trim};
use crate::dsp::memory::MemoryPool;
use crate::dsp::smoother::Smoothed;
use crate::dsp::virtual_analog::Settings;
use crate::mixer::Reading;
use crate::rng::Pcg32;
use crate::voice::{NoteOn, Voice};

/// How many envelopes and LFOs a track has, each moving at most one setting.
const MODULATORS: usize = TRACK_ENVELOPES + LFOS_PER_TRACK;

/// What a track's envelopes move, by setting, kept as a short list as there are only a few.
#[derive(Debug, Clone, Copy, Default)]
struct Moved {
    entries: [(usize, f32); TRACK_ENVELOPES],
    count: usize,
}

impl Moved {
    /// Add to what a setting is moved by.
    fn add(&mut self, index: usize, by: f32) {
        let count = self.count;
        match self.entries[..count]
            .iter_mut()
            .find(|(at, _)| *at == index)
        {
            Some((_, value)) => *value += by,
            None => {
                self.entries[count] = (index, 0.0 + by);
                self.count += 1;
            }
        }
    }

    /// What a setting is moved by.
    fn at(&self, index: usize) -> f32 {
        self.entries()
            .iter()
            .find(|(at, _)| *at == index)
            .map_or(0.0, |(_, value)| *value)
    }

    fn entries(&self) -> &[(usize, f32)] {
        &self.entries[..self.count]
    }
}

/// How often filter coefficients are recalculated while the cutoff moves, in frames.
const FILTER_UPDATE_FRAMES: u32 = 32;
/// How close to wide open the utility filter counts as out of the way, on `0..=1`.
const TRIM_OPEN: f32 = 0.001;

/// Where an LFO's cycles count from.
#[derive(Debug, Clone, Copy, Default)]
pub struct LfoStart {
    pub tick: f64,
    pub clock: u64,
}

/// Where one of a track's envelopes has got to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum EnvelopeRun {
    #[default]
    Idle,
    Held {
        at: u64,
        from: f32,
    },
    Released {
        at: u64,
        from: f32,
    },
}

impl EnvelopeRun {
    fn level(self, envelope: &Envelope, clock: u64, rate: u32) -> f32 {
        match self {
            Self::Idle => 0.0,
            Self::Held { at, from } => envelope.held(clock.saturating_sub(at), from, rate),
            Self::Released { at, from } => envelope.released(clock.saturating_sub(at), from, rate),
        }
    }
}

/// Where the song is, for working out LFOs.
#[derive(Debug, Clone, Copy)]
pub struct SongTime {
    pub tick: f64,
    pub clock: u64,
    pub rate: u32,
    pub seed: u64,
    /// How many frames a tick takes at this tempo.
    pub frames_per_tick: f64,
    /// Frames rendered in all, playing or not.
    pub rendered: u64,
}

/// What a track is playing with right now, after step commands and modulation, on the same `00` to
/// `FF` scale as its settings.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LiveValues {
    pub level: u8,
    pub pan: u8,
    pub cutoff: u8,
    pub resonance: u8,
    pub send_a: u8,
    pub send_b: u8,
    /// The machine's settings, in its order.
    pub machine: [u8; MAX_MACHINE_SETTINGS],
    /// What each slot's settings are playing with, in the effect's order.
    pub inserts: [[u8; MAX_PARAMS]; INSERTS],
    /// How far each slot's compressor or limiter is turning down, in dB, for a meter.
    pub reduction: [f32; INSERTS],
    /// Whether each slot is switched out as it's heard, a step's lock of it included.
    pub bypassed: [bool; INSERTS],
}

/// A value moving in a straight line, by song position, so it keeps up with tempo changes.
#[derive(Debug, Clone, Copy)]
struct Glide {
    from: u16,
    to: FxSlot,
    start_tick: f64,
    ticks: f64,
}

/// What can set one value. What's heard is the glide if there is one, then the lock, then the base.
#[derive(Debug, Clone, Copy)]
/// A value's layers, each with a coarse byte and a fine one, as steps give them.
struct ValueLayers {
    base: u16,
    lock: Option<u16>,
    glide: Option<Glide>,
}

impl ValueLayers {
    fn at(base: u16) -> Self {
        Self {
            base,
            lock: None,
            glide: None,
        }
    }
}

/// A note waiting for its delayed start.
#[derive(Debug, Clone, Copy)]
pub struct DelayedNote {
    pub at: u64,
    pub note: NoteOn,
}

/// Which of a step's parts lands. A nudge moves the row's note part off the grid, while the FX
/// columns with lanes of their own stay on it, so the two land apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// All of it at once, as every step lands without a nudge.
    Whole,
    /// The lanes' values, where the groove lands the step.
    Lanes,
    /// The note, its own FX columns, and what times the note from any column: `DEL`, `KIL`,
    /// `OFF`, `RET` and `CHA`.
    Row,
}

impl Part {
    /// Whether it plays the row's note part.
    pub fn plays_row(self) -> bool {
        self != Self::Lanes
    }

    /// Whether it runs the lanes' values.
    pub fn runs_lanes(self) -> bool {
        self != Self::Row
    }
}

/// How many parts of steps a track can have waiting. At one speed it's two steps' lanes and rows,
/// as a nudged row can land after the next step is worked out, but never after the one after. A
/// slow track's early groove works every track's steps out further ahead, so a fast track can
/// have several steps worked out before the one heard has landed: at most ten of a 32nd's steps
/// against a quarter's earliest groove and nudge.
pub const PENDING_PARTS: usize = 24;

/// How many of the steps a track has worked out it keeps, to find the one heard while it works
/// some out ahead: as many as a 32nd's steps worked out ahead of a quarter's earliest groove and
/// nudge, and the one heard.
const WORKED_KEPT: usize = 16;

/// Where a track's own steps fall on the grid. Each track counts its own steps, so one at an
/// eighth counts its eighths, and its groove swings them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Clock {
    /// Where its next step falls on the grid, in ticks on the engine's count of steps.
    pub beat: u64,
    /// The running count at that step: the track's own steps since the song's start or the last
    /// reset.
    pub count: i64,
    /// Where its steps are counted from on the grid: where it last lined up, at a reset or as its
    /// chain started. A step at a new speed lasts until the next of that speed's steps from here,
    /// so a track keeps to its speed's grid however it changes.
    pub anchor: i64,
}

impl Clock {
    /// Have the next step fall here, and count the steps from here.
    pub fn line_up(&mut self, at: u64) {
        self.beat = at;
        self.anchor = at as i64;
    }

    /// Where the step after the next falls if the next is this many ticks long: the next of its
    /// speed's steps from the anchor, so a step that doesn't start on one is cut short.
    pub fn after(&self, ticks: u64) -> u64 {
        let ticks = ticks.max(1) as i64;
        let from = self.beat as i64 - self.anchor;
        (self.anchor + (from.div_euclid(ticks) + 1) * ticks) as u64
    }

    /// Move on to the step after the next, which is this many ticks long.
    pub fn advance(&mut self, ticks: u64) {
        self.beat = self.after(ticks);
        self.count += 1;
    }

    /// Count on, as if it had played steps this many ticks long, to the first at or after a place.
    pub fn skip_to(&mut self, at: u64, ticks: u64) {
        if at <= self.beat {
            return;
        }
        let steps = (at - self.beat).div_ceil(ticks.max(1));
        self.beat += steps * ticks;
        self.count += steps as i64;
    }

    /// Move it along the grid, as working out a place to play from does once it's known where it
    /// starts.
    pub fn shift(&mut self, by: i64) {
        self.beat = (self.beat as i64 + by).max(0) as u64;
        self.anchor += by;
    }
}

/// A step a track has worked out, for where it's heard: where it falls on the grid, its running
/// count and how long it lasts.
#[derive(Debug, Clone, Copy)]
pub struct Worked {
    pub grid: u64,
    pub count: i64,
    pub ticks: u64,
}

/// A step, or one part of it, worked out but not yet played, as its groove or nudge lands it later
/// than now.
#[derive(Debug, Clone, Copy)]
pub struct PendingStep {
    /// When it lands, on the track's clock.
    pub at: u64,
    /// Its place in the track's running count, which its groove counts.
    pub running: usize,
    /// The groove it was worked out with, which its velocity and length follow.
    pub groove: Groove,
    /// How long one of the track's steps is at its speed, in ticks, which its pushes, nudge,
    /// timing commands and length are shares of.
    pub ticks: u64,
    /// Which part of the step it is.
    pub part: Part,
    /// Which step it is: where it falls on the grid, in ticks, which never wraps.
    pub step: u64,
    /// Whether it's the step's first part to land, which rolls its `CHA` and ends the last note's
    /// locks.
    pub leads: bool,
    /// The chain the track plays it from and the place in it, found when it's worked out, as a
    /// part landing after the next step is worked out mustn't look again.
    pub place: Option<(u32, usize)>,
    /// The count its lanes read at.
    pub count: i64,
    /// What its row's `CHA` and `CND` decided, once the step's first part has decided them.
    pub plays: Option<Decided>,
    /// Which of its track's chain starts it was worked out after, which its accumulators count
    /// from.
    pub start: u64,
}

/// What a row's chance and conditions decided, once, on its first pulse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decided {
    /// Whether its note plays, past its `CHA` and `CND`.
    pub note: bool,
    /// Whether its own FX run, past its `CND`. A failed `CHA` skips only the note.
    pub fx: bool,
}

impl Decided {
    /// A row with nothing against it.
    pub const PLAYS: Self = Self {
        note: true,
        fx: true,
    };
}

/// Retriggering the current note.
#[derive(Debug, Clone, Copy)]
pub struct Retrigger {
    pub next_at: u64,
    pub interval: u64,
    /// Multiplies the level on each retrigger.
    pub factor: f32,
    /// The row's accumulator, where each hit moves its note on.
    pub accumulates: Option<Accumulating>,
}

/// A note's arpeggio, going round its notes at its rate for as long as the note lasts.
#[derive(Debug, Clone, Copy)]
pub struct Arpeggiating {
    pub cycle: Cycle,
    /// Its `ARP`'s value, which a `HLD` carries into the note it ties into, unless that row has
    /// one of its own.
    pub value: u8,
    pub mode: ArpMode,
    /// Whether each note starts the envelopes again, rather than only changing the pitch.
    pub retrig: bool,
    /// Where its first note started, which each later one is counted from, so they don't drift.
    pub from: u64,
    /// How many of its notes have started, the first among them.
    pub played: u64,
    /// Which of the cycle's notes is sounding.
    pub place: usize,
    /// How long each note is, in ticks of a step 240 long, and how long the track's step is, in
    /// ticks, so a note is that share of it.
    pub note_ticks: u64,
    pub step_ticks: u64,
    pub next_at: u64,
}

/// A row whose accumulator moves its note on each time its retrigger hits.
#[derive(Debug, Clone, Copy)]
pub struct Accumulating {
    pub row: RowKey,
    pub amount: i8,
    /// The note as written, which it's moved from.
    pub written: u8,
    /// Its chain's transpose, added after.
    pub transpose: i8,
    pub epoch: Epoch,
}

/// The most rows on a track that keep an accumulator of their own at once. Past it, a row with an
/// `ACU` plays its note as written, though in Track mode it still moves with the track.
pub const ACCUMULATED_ROWS: usize = 64;

/// A row of a phrase, which an accumulator belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowKey {
    pub phrase: u32,
    pub row: usize,
}

/// Which start of a track's chain, and which group of loops round it, a track's accumulators are
/// counting in. They start again from the notes as written once it moves on.
pub type Epoch = (u64, i64);

/// One row's accumulator, and whether its row has played since they started again, as its first
/// play is as written.
#[derive(Debug, Clone, Copy)]
struct RowWalk {
    key: RowKey,
    played: bool,
    walk: Walk,
}

/// A track's accumulators: one for each row with an `ACU`, and the track's own for Track mode, in a
/// fixed table so the callback never allocates.
#[derive(Debug, Clone, Copy)]
pub struct Accumulators {
    rows: [Option<RowWalk>; ACCUMULATED_ROWS],
    track: Walk,
    epoch: Epoch,
}

impl Default for Accumulators {
    fn default() -> Self {
        Self {
            rows: [None; ACCUMULATED_ROWS],
            track: Walk::default(),
            epoch: (0, 0),
        }
    }
}

impl Accumulators {
    /// Go back to the notes as written.
    pub fn forget(&mut self) {
        self.rows = [None; ACCUMULATED_ROWS];
        self.track = Walk::default();
    }

    /// Count in this epoch, starting again if it's a new one.
    pub fn enter(&mut self, epoch: Epoch) {
        if epoch != self.epoch {
            self.forget();
            self.epoch = epoch;
        }
    }

    /// Add a row's amount as it plays, to its own accumulator or the track's. Its first play since
    /// they started again is as written, and adds nothing.
    pub fn add(&mut self, key: RowKey, amount: i8, settings: Accumulation) {
        let found = self
            .rows
            .iter()
            .position(|row| row.is_some_and(|r| r.key == key));
        let at = match found.or_else(|| self.rows.iter().position(Option::is_none)) {
            Some(at) => at,
            None => return,
        };
        let row = self.rows[at].get_or_insert(RowWalk {
            key,
            played: false,
            walk: Walk::default(),
        });
        if !row.played {
            row.played = true;
            return;
        }
        match settings.moves {
            Moves::Row => row.walk.add(amount, settings),
            Moves::Track => self.track.add(amount, settings),
        }
    }

    /// How far a row's note is moved: by its own accumulator, or the track's in Track mode.
    pub fn offset(&self, key: RowKey, moves: Moves) -> i16 {
        match moves {
            Moves::Track => self.track.offset,
            Moves::Row => self
                .rows
                .iter()
                .flatten()
                .find(|row| row.key == key)
                .map_or(0, |row| row.walk.offset),
        }
    }
}

/// One track's voices, held FX values and pending events.
#[derive(Debug, Clone)]
pub struct TrackState {
    /// A chain launched in place of its section's, while jamming, or none inside if stopped.
    pub launched: Option<Option<u32>>,
    /// What to launch when the track's launch timing next allows. Inside is a chain, or none to
    /// stop it.
    pub cued: Option<Option<u32>>,
    /// The chain this track played last.
    pub playing: Option<u32>,
    /// The step of its own running count its chain began at. A cursor partway through a chain can
    /// put it below 0.
    pub started: i64,
    /// What the row playing's `CHA` and `CND` decided, once for all its pulses.
    pub row: Decided,
    /// Whether the last condition decided on this track passed, `CHA` included, which `PRE`
    /// reads here and `NEI` on the track to the right.
    pub passed: bool,
    /// Whether the note sounding is a `Hold`, to be tied into the next row's note.
    pub tied: bool,
    /// How many times its chain has started, which its accumulators count from.
    pub chain_starts: u64,
    /// Where its `ACU`s have moved its notes.
    pub accumulators: Accumulators,
    /// The note the row playing was heard at, before its chain's transpose, when an accumulator
    /// moved it.
    pub accumulated: Option<u8>,
    /// What's added to the count for each FX column's lane, so a lane changed while playing stays
    /// on its step. Anything that sets `started` again sets these back to 0.
    pub lane_offsets: [i64; FX_SLOTS],
    pub voice: Voice,
    pub tail: Voice,
    /// What this track has played since the meters last asked, after its level and pan.
    pub reading: Reading,
    /// Live controls, kept here rather than in the song so undo can't change them.
    pub mute: bool,
    pub solo: bool,
    /// On a return, whether it's heard, so muting fades.
    pub heard: Smoothed,
    pub rng: Pcg32,
    pub last_note: Option<NoteOn>,
    pub delayed: Option<DelayedNote>,
    pub kill_at: Option<u64>,
    pub off_at: Option<u64>,
    /// When the note's gate closes, `Gate` of the way through its last pulse.
    pub gate_at: Option<u64>,
    pub retrigger: Option<Retrigger>,
    /// The sounding note's arpeggio, if it has one.
    pub arp: Option<Arpeggiating>,
    /// The rolls an arpeggio in `Random` takes, apart from `rng` so `CHA` rolls as it did.
    pub arp_rng: Pcg32,
    /// Steps, or parts of them, waiting for where their groove and nudge land them, and when the
    /// soonest lands, so a track with none waiting isn't looked through.
    pub pending: [Option<PendingStep>; PENDING_PARTS],
    soonest: u64,
    /// Where its own steps fall.
    pub clock: Clock,
    /// The steps it has worked out lately, newest last, going round.
    worked: [Option<Worked>; WORKED_KEPT],
    worked_next: usize,
    /// The random timing a groove's humanise adds, apart from `rng` so `CHA` rolls as it did.
    pub groove_rng: Pcg32,
    /// What each setting is set to, by its place in the registry. The fields below it are what's
    /// heard: a voice's or an effect's settings only reach them while it's the one loaded.
    values: [ValueLayers; SettingId::COUNT],
    /// How many of `values` have a glide running, so moving them along is skipped when none do.
    gliding: usize,
    /// The voice and effects whose settings are heard now.
    loaded_voice: Option<SynthVoice>,
    loaded_effects: [Option<Effect>; INSERTS],
    /// Held `VOL`, `SNA` and `SNB`, as level values, so `E0` is exactly 0 dB. What's smoothed is the gain they come to, so the
    /// curve is only worked out when they or an LFO change.
    vol: f32,
    send_a: f32,
    send_b: f32,
    gain: Smoothed,
    /// The track's level from the song, which playback going back to the top leaves alone.
    pub level: Smoothed,
    pan: Smoothed,
    cutoff: Smoothed,
    resonance: Smoothed,
    send_a_gain: Smoothed,
    send_b_gain: Smoothed,
    filter: [Channel; 2],
    filter_type: FilterType,
    /// Held `MODE`, a place in the list of modes, which the filter's type may not offer.
    filter_mode: f32,
    drive: Smoothed,
    trim: [Trim; 2],
    low_cut: Smoothed,
    /// What each of its effects keeps.
    inserts: [InsertState; INSERTS],
    /// What's heard for each setting of each slot, before modulation, as a step command gives it.
    slot_values: [[f32; MAX_PARAMS]; INSERTS],
    /// What each slot's effect is playing with now.
    resolved: [Insert; INSERTS],
    /// Whether modulation is moving each slot.
    slot_moving: [bool; INSERTS],
    high_cut: Smoothed,
    /// The coefficients in use, and the value each was worked out for.
    low_cut_coefficients: Option<(f32, Coefficients)>,
    high_cut_coefficients: Option<(f32, Coefficients)>,
    trim_countdown: u32,
    coefficients: Option<Coefficients>,
    filter_countdown: u32,
    pub lfo_starts: [LfoStart; LFOS_PER_TRACK],
    /// What the LFOs add to each held value, by setting. Separate from the held values, so a
    /// step's FX moves the centre and the LFO carries on around it.
    offsets: [f32; SettingId::COUNT],
    /// The settings `offsets` has anything on, at most one for each envelope and LFO, so only those
    /// are cleared next time rather than every setting.
    offset_marks: [Option<usize>; MODULATORS],
    cut_offset: Smoothed,
    res_offset: Smoothed,
    pan_offset: Smoothed,
    /// What envelopes add, which moves fast and needs to keep its snap.
    cut_envelope: Smoothed,
    res_envelope: Smoothed,
    pan_envelope: Smoothed,
    /// The last pan position and its gains, as a settled pan stays put and `cos` and `sin` cost.
    panned: (f32, (f32, f32)),
    envelopes: [EnvelopeRun; TRACK_ENVELOPES],
    /// What modulation multiplies a note's speed by.
    pitch: f64,
    /// Each LFO's held depth and phase, and the held pitch, on the `00` to `FF` scale, as steps
    /// and the page set them.
    lfo_values: [[f32; 2]; LFOS_PER_TRACK],
    held_pitch: f32,
    /// Held `LEN`, the track's `Gate`, on the `00` to `FF` scale.
    held_gate: f32,
    /// Held `LDLT`, which moves each note's length up or down from its own, with `80` as none.
    held_gate_delta: f32,
    /// Held `GLD`, how long a tie glides into the next note, on the `00` to `FF` scale.
    held_glide: f32,
    /// Held `SMP`, the bank entry a sampler's next note plays, by number.
    held_sample: f32,
    /// Whether each slot is switched out as it's heard: its own bypass, or a step's lock of it.
    bypassed: [bool; INSERTS],
    /// The loaded voice's held settings, in its order, on `0..=1`.
    synth: [f32; MAX_MACHINE_SETTINGS],
    /// Whether an LFO moves the filter, so it stays running at the top of its range.
    filter_moving: bool,
    /// Its sound after its slots in the latest frame, for a compressor on another track to hear.
    pub key: [f32; 2],
}

/// The pan gains for a `0..=1` position. The centre is unity, and it keeps equal power.
#[inline]
fn pan_gains(position: f32) -> (f32, f32) {
    let angle = position * std::f32::consts::FRAC_PI_2;
    (
        angle.cos() * std::f32::consts::SQRT_2,
        angle.sin() * std::f32::consts::SQRT_2,
    )
}

/// Map a `PAN` value to a `0..=1` position, with `80` as exact centre.
fn pan_position(value: f32) -> f32 {
    if value >= 128.0 {
        0.5 + (value - 128.0) / 127.0 * 0.5
    } else {
        value / 128.0 * 0.5
    }
}

/// Where a setting starts on this track, before any step sets it, with a coarse byte and a fine
/// one. A voice keeps its own, an effect's setting is what's in its slot, or its default while
/// another effect is there, and an LFO's depth and phase are what its page has.
pub fn base_of(id: SettingId, track: &Track) -> u16 {
    let lfo = |lfo: usize, at: usize| match at {
        0 => depth_value(track.lfos[lfo].depth),
        _ => phase_value(track.lfos[lfo].phase),
    };
    let whole = match id.owner() {
        (Owner::Lfo(n), at) => return lfo(n, at),
        (Owner::Track, _) => match id {
            SettingId::VOL => UNITY_LEVEL,
            SettingId::PAN => track.pan,
            SettingId::CUT => track.cutoff,
            SettingId::RES => track.resonance,
            SettingId::SEND_A => track.send_a,
            SettingId::SEND_B => track.send_b,
            SettingId::PITCH => track.pitch,
            SettingId::FILTER_MODE => track.filter_mode.index(),
            SettingId::GATE_LENGTH => track.gate_length(),
            SettingId::GROOVE => track
                .groove
                .map_or(0, |groove| (groove.index() + 1).min(0xFF) as u8),
            SettingId::SPEED => track.speed.index(),
            SettingId::GLIDE => track.glide,
            _ => id.entry().default,
        },
        (Owner::Bypass(slot), _) => u8::from(track.bypassed[slot]),
        (Owner::Voice(_) | Owner::Sampler, _) => track.setting(id),
        (Owner::Slot { slot, effect }, at) => {
            let insert = track.inserts[slot];
            // A distortion keeps every mode's settings, so one its mode doesn't show still reads.
            if Effect::of(insert) == Some(effect) {
                insert.read(effect.params()[at].setting).unwrap_or(0)
            } else {
                id.entry().default
            }
        }
    };
    u16::from(whole) << 8 | u16::from(track.fine(id))
}

/// The generator for a track's humanise, on streams after every track's own for `CHA`.
fn groove_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (MAX_TRACKS + index) as u64)
}

/// The generator for a track's random arpeggios, on streams after every track's humanise.
fn arp_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (2 * MAX_TRACKS + index) as u64)
}

/// What's heard of a value: its lock over its base. A glide is moved along by the song.
/// On the `00` to `FF` scale, with the fine part as the fraction.
fn heard(layers: &ValueLayers) -> f32 {
    scaled(layers.lock.unwrap_or(layers.base))
}

/// A value with a fine part on the `00` to `FF` scale, so `4080` is 64.5.
fn scaled(value: u16) -> f32 {
    f32::from(value) / 256.0
}

impl TrackState {
    /// A silent track with default held values.
    pub fn new(seed: u64, index: usize) -> Self {
        Self {
            launched: None,
            cued: None,
            playing: None,
            started: 0,
            lane_offsets: [0; FX_SLOTS],
            row: Decided::PLAYS,
            passed: false,
            tied: false,
            chain_starts: 0,
            accumulators: Accumulators::default(),
            accumulated: None,
            voice: Voice::default(),
            tail: Voice::default(),
            reading: Reading::default(),
            mute: false,
            solo: false,
            heard: Smoothed::at(1.0),
            rng: Pcg32::new(seed, index as u64),
            groove_rng: groove_rng(seed, index),
            last_note: None,
            delayed: None,
            kill_at: None,
            off_at: None,
            gate_at: None,
            retrigger: None,
            arp: None,
            arp_rng: arp_rng(seed, index),
            values: {
                let mut values = [ValueLayers::at(0); SettingId::COUNT];
                for id in SettingId::all() {
                    values[id.index()] = ValueLayers::at(u16::from(id.entry().default) << 8);
                }
                values
            },
            loaded_voice: None,
            loaded_effects: [None; INSERTS],
            pending: [None; PENDING_PARTS],
            soonest: u64::MAX,
            clock: Clock::default(),
            worked: [None; WORKED_KEPT],
            worked_next: 0,
            vol: f32::from(UNITY_LEVEL),
            send_a: 0.0,
            send_b: 0.0,
            gain: Smoothed::at(1.0),
            level: Smoothed::at(1.0),
            pan: Smoothed::at(0.5),
            cutoff: Smoothed::at(1.0),
            resonance: Smoothed::at(0.0),
            send_a_gain: Smoothed::at(0.0),
            send_b_gain: Smoothed::at(0.0),
            filter: [Channel::default(); 2],
            filter_type: FilterType::LowPass,
            filter_mode: 0.0,
            drive: Smoothed::at(0.0),
            trim: [Trim::default(); 2],
            low_cut: Smoothed::at(0.0),
            inserts: std::array::from_fn(|_| InsertState::default()),
            gliding: 0,
            slot_values: [[0.0; MAX_PARAMS]; INSERTS],
            resolved: [Insert::Empty; INSERTS],
            slot_moving: [false; INSERTS],
            high_cut: Smoothed::at(1.0),
            low_cut_coefficients: None,
            high_cut_coefficients: None,
            trim_countdown: 0,
            coefficients: None,
            filter_countdown: 0,
            lfo_starts: [LfoStart::default(); LFOS_PER_TRACK],
            offsets: [0.0; SettingId::COUNT],
            offset_marks: [None; MODULATORS],
            synth: {
                let mut synth = [0.0; MAX_MACHINE_SETTINGS];
                synth[..4].copy_from_slice(&[0.5, 0.0, 0.5, f32::from(DEFAULT_ACCENT) / 255.0]);
                synth
            },
            cut_offset: Smoothed::at(0.0),
            res_offset: Smoothed::at(0.0),
            pan_offset: Smoothed::at(0.0),
            cut_envelope: Smoothed::at(0.0),
            res_envelope: Smoothed::at(0.0),
            pan_envelope: Smoothed::at(0.0),
            panned: (0.5, pan_gains(0.5)),
            envelopes: [EnvelopeRun::Idle; TRACK_ENVELOPES],
            pitch: 1.0,
            lfo_values: [[128.0, 0.0]; LFOS_PER_TRACK],
            held_pitch: 128.0,
            held_gate: f32::from(SettingId::GATE_LENGTH.entry().default),
            held_gate_delta: f32::from(SettingId::GATE_DELTA.entry().default),
            held_glide: f32::from(SettingId::GLIDE.entry().default),
            held_sample: 0.0,
            bypassed: [false; INSERTS],
            filter_moving: false,
            key: [0.0; 2],
        }
    }

    /// Start its chain at its next step, which falls here, with its steps counted from here.
    pub fn restart_at(&mut self, at: u64) {
        self.clock.line_up(at);
        self.restart(self.clock.count);
    }

    /// Start counting the track's chain from this running count, with its lanes in line again.
    pub fn restart(&mut self, started: i64) {
        self.started = started;
        self.lane_offsets = [0; FX_SLOTS];
        self.tied = false;
        self.chain_starts += 1;
    }

    /// Forget what the last conditions decided, as playing starts, so `PRE` and `NEI` start from
    /// none having passed.
    pub fn forget_conditions(&mut self) {
        self.row = Decided::PLAYS;
        self.passed = false;
    }

    /// Put its accumulators back to the notes as written, as playing starts.
    pub fn forget_accumulators(&mut self) {
        self.accumulators.forget();
        self.accumulated = None;
    }

    /// Carry the sounding note on as this one, with no new attack, gliding there for as long as
    /// `GLD` says as it's heard now.
    pub fn tie(&mut self, note: NoteOn) {
        self.last_note = Some(note);
        self.voice.tie(note, glide_ms(self.held_glide));
        self.tell_slots_the_note();
    }

    /// Tell the slots what note is playing, with the track's `Pitch` and its modulation, for an
    /// oscillator of their own to follow.
    fn tell_slots_the_note(&mut self) {
        let played = self.last_note.map_or(60.0, |note| f32::from(note.note));
        let note = played + 12.0 * self.pitch.log2() as f32;
        for insert in &mut self.inserts {
            insert.follow_note(note);
        }
    }

    /// Reset held values, the random generator and LFOs, when playback goes back to the top. Values
    /// glide back, so anything still sounding doesn't click.
    pub fn reset_held(&mut self, seed: u64, index: usize, track: &Track, clock: u64) {
        self.rng = Pcg32::new(seed, index as u64);
        self.forget_conditions();
        self.forget_accumulators();
        self.groove_rng = groove_rng(seed, index);
        self.arp_rng = arp_rng(seed, index);
        self.arp = None;
        self.last_note = None;
        self.load(track);
        for id in SettingId::all() {
            let base = base_of(id, track);
            self.values[id.index()] = ValueLayers::at(base);
            self.write_value(id, scaled(base));
        }
        self.gliding = 0;
        self.lfo_starts = [LfoStart { tick: 0.0, clock }; LFOS_PER_TRACK];
    }

    /// Hear the voice and effects the track has now. One that's changed is heard from its own
    /// values, as steps and the track page left them.
    fn load(&mut self, track: &Track) {
        let voice = track.voice();
        if voice != self.loaded_voice {
            self.loaded_voice = voice;
            if let Some(voice) = voice {
                for at in 0..MAX_MACHINE_SETTINGS {
                    if let Some(id) = SettingId::voice(voice, at) {
                        self.synth[at] = heard(&self.values[id.index()]) / 255.0;
                    }
                }
            }
        }
        for slot in 0..INSERTS {
            let effect = Effect::of(track.inserts[slot]);
            if effect == self.loaded_effects[slot] {
                continue;
            }
            self.loaded_effects[slot] = effect;
            let Some(effect) = effect else {
                continue;
            };
            for at in 0..effect.params().len() {
                if let Some(id) = SettingId::slot(slot, effect, at) {
                    self.slot_values[slot][at] = heard(&self.values[id.index()]);
                }
            }
        }
    }

    /// Work out what the LFOs add now. A stopped track sits at its held values, so an audition
    /// hears those rather than wherever the LFO stopped.
    pub fn modulate(
        &mut self,
        track: &Track,
        index: usize,
        now: SongTime,
        stopped: bool,
        memory: &mut MemoryPool,
    ) {
        self.load(track);
        if !stopped {
            self.advance_glides(now);
        }
        self.filter_moving = false;
        // Envelopes, then each LFO, so each reads what's before it, and there's no way back round.
        // What the envelopes move is kept by setting, as there are only a few.
        let mut moved = Moved::default();
        for (slot, (envelope, run)) in track.envelopes.iter().zip(self.envelopes).enumerate() {
            let Some(destination) = envelope.destination else {
                continue;
            };
            // The first one on `Vol` is each note's own shape, which the voice follows itself.
            if envelope.depth == 0
                || (slot == 0 && destination == SettingId::VOL)
                || !destination.exists_on(track)
            {
                continue;
            }
            self.filter_moving |= matches!(destination, SettingId::CUT | SettingId::RES);
            if stopped {
                continue;
            }
            let level = run.level(envelope, now.clock, now.rate);
            moved.add(
                destination.index(),
                level * f32::from(envelope.depth) / 255.0,
            );
        }
        // Only what was moved last time needs clearing.
        for index in self.offset_marks.iter_mut().filter_map(Option::take) {
            self.offsets[index] = 0.0;
        }
        let mut marks = 0;
        let offsets = &mut self.offsets;
        for (slot, (lfo, start)) in track.lfos.iter().zip(&self.lfo_starts).enumerate() {
            let Some(destination) = lfo.destination else {
                continue;
            };
            let moved_by = |setting: usize| {
                SettingId::lfo(slot, setting)
                    .map_or(0.0, |id| offsets[id.index()] + moved.at(id.index()))
            };
            let [depth, phase] = self.lfo_values[slot];
            let depth = depth_of(depth) + moved_by(0) * 255.0;
            // A target that isn't on this track does no work.
            if depth == 0.0 || !destination.exists_on(track) {
                continue;
            }
            self.filter_moving |= matches!(destination, SettingId::CUT | SettingId::RES);
            if stopped {
                continue;
            }
            let seconds = now.clock.wrapping_sub(start.clock) as f64 / f64::from(now.rate);
            let cycles = lfo.rate.cycles(now.tick - start.tick, seconds);
            let key = now.seed ^ ((index as u64) << 8 | slot as u64);
            let phase = f64::from(phase_of(phase) + moved_by(1));
            let value = wave(lfo.shape, cycles + phase, key);
            let depth = depth.clamp(-f32::from(MAX_LFO_DEPTH), f32::from(MAX_LFO_DEPTH));
            offsets[destination.index()] += value * depth / 255.0;
            self.offset_marks[marks] = Some(destination.index());
            marks += 1;
        }
        // A different filter starts from silence rather than from another's state.
        if self.filter_type != track.filter {
            self.filter_type = track.filter;
            self.filter = [Channel::default(); 2];
            self.coefficients = None;
        }
        self.drive.target = f32::from(track.drive) / 255.0;
        self.low_cut.target = f32::from(track.low_cut) / 255.0;
        self.high_cut.target = f32::from(track.high_cut) / 255.0;
        self.cut_offset.target = offsets[SettingId::CUT.index()];
        self.res_offset.target = offsets[SettingId::RES.index()];
        self.pan_offset.target = offsets[SettingId::PAN.index()];
        self.cut_envelope.target = moved.at(SettingId::CUT.index());
        self.res_envelope.target = moved.at(SettingId::RES.index());
        self.pan_envelope.target = moved.at(SettingId::PAN.index());
        for &(index, envelope) in moved.entries() {
            offsets[index] += envelope;
            self.offset_marks[marks] = Some(index);
            marks += 1;
        }
        // Each slot's effect gets finished values, and doesn't know where they came from.
        for (slot, insert) in track.inserts.into_iter().enumerate() {
            let mut resolved = insert;
            self.slot_moving[slot] = false;
            let effect = Effect::of(insert);
            // A mode shows some of its effect's settings, so each is found by what it is.
            for found in insert.params() {
                let setting = found.setting;
                let Some((at, id)) = effect.and_then(|effect| {
                    let at = effect
                        .params()
                        .iter()
                        .position(|param| param.setting == setting)?;
                    Some((at, SettingId::slot(slot, effect, at)?))
                }) else {
                    continue;
                };
                let added = offsets[id.index()] * setting.span();
                self.slot_moving[slot] |= added != 0.0;
                let value = self.slot_values[slot][at] + added;
                resolved = resolved.write(setting, value);
            }
            self.resolved[slot] = resolved;
            self.inserts[slot].hold(resolved, memory, now.frames_per_tick, now.rendered);
            self.inserts[slot].bypass(self.bypassed[slot]);
        }
        let semitones = self.held_pitch - 128.0 + offsets[SettingId::PITCH.index()] * 255.0;
        self.pitch = 2_f64.powf(f64::from(semitones.clamp(-96.0, 96.0)) / 12.0);
        self.voice.set_pitch(self.pitch);
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
        self.tell_slots_the_note();
    }

    /// The pan gains for a position, worked out again only when it moves.
    #[inline]
    fn pan_gains(&mut self, position: f32) -> (f32, f32) {
        if position.to_bits() != self.panned.0.to_bits() {
            self.panned = (position, pan_gains(position));
        }
        self.panned.1
    }

    /// What this track is playing with right now. `VOL` and its modulation are shown on the track's
    /// `level`, as they both set how loud it is. The first envelope on `Vol` shapes notes, and isn't
    /// a movement of the level.
    pub fn live(&self, level: u8) -> LiveValues {
        let offset = |destination: SettingId| self.offsets[destination.index()];
        let moved = |held: f32, destination| {
            ((held + offset(destination)).clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let on_level = |held: f32, destination| {
            (held + offset(destination) * 255.0)
                .clamp(0.0, 255.0)
                .round() as u8
        };
        let synth = self.synth_settings();
        let shown = |value: f32| (value * 255.0).round() as u8;
        let vol = self.vol + offset(SettingId::VOL) * 255.0 - f32::from(UNITY_LEVEL);
        LiveValues {
            level: (f32::from(level) + vol).clamp(0.0, 255.0).round() as u8,
            pan: moved(self.pan.target, SettingId::PAN),
            cutoff: moved(self.cutoff.target, SettingId::CUT),
            resonance: moved(self.resonance.target, SettingId::RES),
            send_a: on_level(self.send_a, SettingId::SEND_A),
            send_b: on_level(self.send_b, SettingId::SEND_B),
            machine: std::array::from_fn(|place| shown(synth.at(place))),
            inserts: self.resolved.map(|insert| {
                let mut values = [0; MAX_PARAMS];
                for (param, value) in values.iter_mut().enumerate() {
                    *value = insert.value(param);
                }
                values
            }),
            reduction: std::array::from_fn(|slot| self.inserts[slot].reduction()),
            bypassed: self.bypassed,
        }
    }

    /// What a synth on this track plays with: the held values and what modulation adds.
    pub fn synth_settings(&self) -> Settings {
        let at = |setting: usize| {
            let offset = self
                .loaded_voice
                .and_then(|voice| SettingId::voice(voice, setting))
                .map_or(0.0, |id| self.offsets[id.index()]);
            (self.synth[setting] + offset).clamp(0.0, 1.0)
        };
        Settings::from_places(std::array::from_fn(at))
    }

    /// Start the track's envelopes for a note.
    pub fn trigger_envelopes(&mut self, track: &Track, now: SongTime) {
        for (envelope, run) in track.envelopes.iter().zip(&mut self.envelopes) {
            let from = if envelope.reset {
                0.0
            } else {
                run.level(envelope, now.clock, now.rate)
            };
            *run = EnvelopeRun::Held {
                at: now.clock,
                from,
            };
        }
    }

    /// Start the release of the track's envelopes, when a note ends.
    pub fn release_envelopes(&mut self, track: &Track, now: SongTime) {
        for (envelope, run) in track.envelopes.iter().zip(&mut self.envelopes) {
            if matches!(run, EnvelopeRun::Held { .. }) {
                let from = run.level(envelope, now.clock, now.rate);
                *run = EnvelopeRun::Released {
                    at: now.clock,
                    from,
                };
            }
        }
    }

    /// Work out the gains of the held values that are heard on a level scale.
    fn update_gains(&mut self) {
        let gain = |held: f32, destination: SettingId| {
            level_gain((held + self.offsets[destination.index()] * 255.0).clamp(0.0, 255.0))
        };
        let (vol, send_a, send_b) = (
            gain(self.vol, SettingId::VOL),
            gain(self.send_a, SettingId::SEND_A),
            gain(self.send_b, SettingId::SEND_B),
        );
        self.gain.target = vol;
        self.send_a_gain.target = send_a;
        self.send_b_gain.target = send_b;
    }

    /// Start a new note, cancelling anything pending for the old one.
    pub fn start(&mut self, note: NoteOn, rate: u32) {
        self.begin(note, rate, false);
    }

    /// Start an arpeggio's next note again, as `start` does, with any noise its synth draws taken
    /// from the arpeggio's own random stream, so `CHA` rolls as it would without it.
    pub fn restrike(&mut self, note: NoteOn, rate: u32) {
        self.begin(note, rate, true);
    }

    fn begin(&mut self, note: NoteOn, rate: u32, arpeggio: bool) {
        let from = if note.shape.reset {
            0.0
        } else {
            self.voice.level()
        };
        // `Acid` plays one note at a time, and the next carries on from its filter and accent.
        let carries_on = note.synth == Some(SynthVoice::Acid) && self.voice.plays_acid();
        if !carries_on {
            self.cut_to_tail();
        }
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.gate_at = None;
        self.last_note = Some(note);
        self.tell_slots_the_note();
        if carries_on {
            self.voice.retrigger(note);
        } else {
            self.voice = Voice::start(note, rate, from, self.pitch);
        }
        self.voice.set_synth(self.synth_settings());
        if self.voice.uses_noise() {
            let rng = if arpeggio {
                &mut self.arp_rng
            } else {
                &mut self.rng
            };
            self.voice.seed_noise(rng.next_u32());
        }
    }

    /// How much of its last pulse a note starting now lasts: its row's own length, or the held
    /// `Gate` if it has none, moved up or down by `LDLT` and by what modulation adds to `Gate`, as
    /// an LFO moves any setting around where a step put it. 1 holds it to the next note.
    pub fn gate_share(&self, own: Option<u8>) -> f32 {
        let base = own.map_or(self.held_gate, f32::from);
        let moved =
            gate_delta(self.held_gate_delta) + self.offsets[SettingId::GATE_LENGTH.index()] * 255.0;
        gate_share(base + moved)
    }

    /// What a step chooses of a setting read before it plays, as `GRV` and `RATE` are, on its
    /// scale: a lock over the held value, or the held value alone once a new note has ended the
    /// locks.
    pub fn held_choice(&self, id: SettingId, locks_end: bool) -> u8 {
        let layers = &self.values[id.index()];
        let value = match layers.lock {
            Some(lock) if !locks_end => lock,
            _ => layers.base,
        };
        (value >> 8) as u8
    }

    /// The bank entry a sampler's note starting now plays, by number: the held `SMP`, or a lock
    /// over it, moved by what modulation adds, in whole entries.
    pub fn sample_number(&self) -> u8 {
        let moved = self.held_sample + self.offsets[SettingId::SAMPLE.index()] * 255.0;
        moved.round().clamp(0.0, 255.0) as u8
    }

    /// How far either way a groove's humanise moves a step, at random, in ticks.
    pub fn humanise(&mut self, ticks: u8) -> i64 {
        if ticks == 0 {
            return 0;
        }
        let spread = u32::from(ticks) * 2 + 1;
        i64::from(self.groove_rng.next_u32() % spread) - i64::from(ticks)
    }

    /// Take the part that lands first, if it lands by this time. Parts that land together play in
    /// the order their steps were worked out, a step's first part before its other.
    pub fn take_due(&mut self, by: u64) -> Option<PendingStep> {
        if self.soonest > by {
            return None;
        }
        let next = self
            .pending
            .iter()
            .enumerate()
            .filter_map(|(at, part)| part.map(|part| (at, part)))
            .filter(|(_, part)| part.at <= by)
            .min_by_key(|(_, part)| (part.at, part.step, !part.leads))?;
        self.pending[next.0] = None;
        self.find_soonest();
        Some(next.1)
    }

    /// When the soonest part waiting lands, or `u64::MAX` with none.
    pub fn soonest(&self) -> u64 {
        self.soonest
    }

    fn find_soonest(&mut self) {
        self.soonest = self
            .pending
            .iter()
            .flatten()
            .map(|part| part.at)
            .min()
            .unwrap_or(u64::MAX);
    }

    /// Wait for a part to land. With no room, which only a big change of tempo could bring
    /// about, the earliest waiting is given back to be played now.
    pub fn wait_for(&mut self, part: PendingStep) -> Option<PendingStep> {
        let full = self.pending.iter().all(Option::is_some);
        let earliest = if full { self.take_due(u64::MAX) } else { None };
        if let Some(free) = self.pending.iter_mut().find(|part| part.is_none()) {
            *free = Some(part);
            self.soonest = self.soonest.min(part.at);
        }
        earliest
    }

    /// Keep a step it has worked out, for where it's heard.
    pub fn note_worked(&mut self, worked: Worked) {
        self.worked[self.worked_next] = Some(worked);
        self.worked_next = (self.worked_next + 1) % WORKED_KEPT;
    }

    /// The latest step it has worked out that falls by this tick on the grid, the one heard.
    pub fn heard_at(&self, tick: f64) -> Option<Worked> {
        self.worked
            .iter()
            .flatten()
            .filter(|worked| worked.grid as f64 <= tick)
            .max_by_key(|worked| worked.grid)
            .copied()
    }

    /// Forget the steps it has worked out from this tick on the grid, as they'll be worked out
    /// again, or all of them, when playing starts somewhere else.
    pub fn forget_worked(&mut self, from: u64) {
        for worked in &mut self.worked {
            if worked.is_some_and(|worked| worked.grid >= from) {
                *worked = None;
            }
        }
    }

    /// The latest any waiting part that plays a row's note lands, as the next row's can't land
    /// before it.
    pub fn last_row_at(&self) -> Option<u64> {
        self.pending
            .iter()
            .flatten()
            .filter(|part| part.part.plays_row())
            .map(|part| part.at)
            .max()
    }

    /// Tell a step's other part, still waiting, what its row's chance and conditions decided.
    pub fn decide(&mut self, step: u64, plays: Decided) {
        for part in self.pending.iter_mut().flatten() {
            if part.step == step {
                part.plays = Some(plays);
            }
        }
    }

    /// Forget whatever is waiting of the steps from this tick on the grid, as they're to be worked
    /// out again.
    pub fn forget_steps(&mut self, from: u64) {
        for part in &mut self.pending {
            if part.is_some_and(|part| part.step >= from) {
                *part = None;
            }
        }
        self.find_soonest();
        self.forget_worked(from);
    }

    /// Fade the playing note.
    pub fn cut_to_tail(&mut self) {
        if self.voice.active() {
            self.tail = self.voice;
            self.tail.fade();
            self.voice = Voice::default();
        }
    }

    /// Stop sounding and drop anything pending, when the transport halts.
    pub fn stop(&mut self) {
        self.cut_to_tail();
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.gate_at = None;
        self.retrigger = None;
        self.arp = None;
        self.pending = [None; PENDING_PARTS];
        self.soonest = u64::MAX;
        self.tied = false;
        self.envelopes = [EnvelopeRun::Idle; TRACK_ENVELOPES];
    }

    /// Run a lock or hold value from a step. Anything else on that value gives way to it.
    pub fn set_value(&mut self, id: SettingId, value: u16, mark: Mark) {
        let layers = &mut self.values[id.index()];
        if layers.glide.take().is_some() {
            self.gliding -= 1;
        }
        if mark == Mark::Hold {
            layers.base = value;
            layers.lock = None;
        } else {
            layers.lock = Some(value);
        }
        self.write_value(id, scaled(value));
    }

    /// Run a glide value from a step, toward the next value of its parameter if there is one.
    /// `to` is that value, with how many ticks away it is.
    pub fn start_glide(
        &mut self,
        id: SettingId,
        value: u16,
        to: Option<(FxSlot, f64)>,
        now: SongTime,
    ) {
        self.set_value(id, value, Mark::Lock);
        if let Some((to, ticks)) = to {
            let layers = &mut self.values[id.index()];
            layers.lock = None;
            layers.glide = Some(Glide {
                from: value,
                to,
                start_tick: now.tick,
                ticks,
            });
            self.gliding += 1;
        }
    }

    /// A note has started, so the locks it would have outlived are over. Glides carry on.
    pub fn end_locks(&mut self) {
        for id in SettingId::all() {
            let layers = &mut self.values[id.index()];
            if layers.lock.take().is_some() && layers.glide.is_none() {
                let base = layers.base;
                self.write_value(id, scaled(base));
            }
        }
    }

    /// The track page changed a value. A lock or glide on top stays until it ends.
    pub fn set_base(&mut self, id: SettingId, value: u16) {
        let layers = &mut self.values[id.index()];
        layers.base = value;
        if layers.lock.is_none() && layers.glide.is_none() {
            self.write_value(id, scaled(value));
        }
    }

    /// Move glides along to where the song is.
    fn advance_glides(&mut self, now: SongTime) {
        debug_assert_eq!(
            self.gliding,
            self.values
                .iter()
                .filter(|layers| layers.glide.is_some())
                .count(),
            "the count of glides is out"
        );
        if self.gliding == 0 {
            return;
        }
        for id in SettingId::all() {
            let Some(glide) = self.values[id.index()].glide else {
                continue;
            };
            let progress = (now.tick - glide.start_tick) / glide.ticks;
            if progress < 1.0 {
                let (from, to) = (scaled(glide.from), scaled(glide.to.value));
                self.write_value(id, from + (to - from) * progress.max(0.0) as f32);
            } else {
                // Its row hasn't run, as a chain moved on, so this takes that row's place.
                let mark = match glide.to.mark {
                    Mark::Hold => Mark::Hold,
                    Mark::Lock | Mark::Glide => Mark::Lock,
                };
                self.set_value(id, glide.to.value, mark);
            }
        }
    }

    /// Set what's heard for a value, on the `00` to `FF` scale, with any fine part as the
    /// fraction.
    /// A setting of a voice or an effect that isn't loaded keeps its value for when it is, and
    /// isn't heard.
    fn write_value(&mut self, id: SettingId, value: f32) {
        match id.owner() {
            (Owner::Lfo(lfo), at) => {
                self.lfo_values[lfo][at] = value;
                return;
            }
            (Owner::Track, _) => match id {
                SettingId::PITCH => {
                    self.held_pitch = value;
                    return;
                }
                SettingId::VOL => self.vol = value,
                SettingId::PAN => self.pan.target = pan_position(value),
                SettingId::CUT => self.cutoff.target = value / 255.0,
                SettingId::RES => self.resonance.target = value / 255.0,
                SettingId::SEND_A => self.send_a = value,
                SettingId::SEND_B => self.send_b = value,
                SettingId::FILTER_MODE => {
                    self.filter_mode = value;
                    return;
                }
                SettingId::GATE_LENGTH => {
                    self.held_gate = value;
                    return;
                }
                SettingId::GATE_DELTA => {
                    self.held_gate_delta = value;
                    return;
                }
                SettingId::GLIDE => {
                    self.held_glide = value;
                    return;
                }
                _ => return,
            },
            // Read as a note starts, which keeps what it chose until it ends.
            (Owner::Sampler, _) => {
                self.held_sample = value;
                return;
            }
            // Heard as the slots are next given their settings, where it fades in or out.
            (Owner::Bypass(slot), _) => {
                self.bypassed[slot] = value >= 0.5;
                return;
            }
            (Owner::Voice(voice), at) if self.loaded_voice == Some(voice) => {
                self.synth[at] = value / 255.0;
            }
            (Owner::Slot { slot, effect }, at) => {
                if self.loaded_effects[slot] == Some(effect) {
                    self.slot_values[slot][at] = value;
                }
                return;
            }
            _ => return,
        }
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
    }

    /// Give the long buffers its slots hold back to the pool, as the track's gone.
    pub fn give_back_memory(&mut self, memory: &mut MemoryPool) {
        for insert in &mut self.inserts {
            insert.give_back(memory);
        }
    }

    /// Whether a slot still has something to play, with nothing going in.
    #[inline]
    pub fn ringing(&self, rate: u32) -> bool {
        self.inserts.iter().any(|insert| insert.ringing(rate))
    }

    /// Whether anything is sounding.
    #[inline]
    pub fn sounding(&self) -> bool {
        self.voice.active() || self.tail.active()
    }

    /// Settle held values, so a new sound doesn't ramp from an old setting while idle.
    pub fn settle(&mut self) {
        self.gain.settle();
        self.level.settle();
        self.pan.settle();
        self.cutoff.settle();
        self.resonance.settle();
        self.send_a_gain.settle();
        self.send_b_gain.settle();
        self.heard.settle();
        self.cut_offset.settle();
        self.res_offset.settle();
        self.pan_offset.settle();
        self.cut_envelope.settle();
        self.res_envelope.settle();
        self.pan_envelope.settle();
        self.inserts.iter_mut().for_each(InsertState::settle);
        self.drive.settle();
        self.low_cut.settle();
        self.high_cut.settle();
        self.filter.iter_mut().for_each(Channel::settle);
        self.coefficients = None;
        self.filter_countdown = 0;
    }

    /// The utility filter. It's out of the way when wide open, and coefficients are only worked out
    /// while it moves.
    #[inline]
    fn trimmed(&mut self, input: [f32; 2], smoothing: f32, rate: u32) -> [f32; 2] {
        let low_cut = self.low_cut.next(smoothing);
        let high_cut = self.high_cut.next(smoothing);
        let (low_on, high_on) = (low_cut > TRIM_OPEN, high_cut < 1.0 - TRIM_OPEN);
        if !low_on && !high_on {
            self.low_cut_coefficients = None;
            self.high_cut_coefficients = None;
            return input;
        }
        if self.trim_countdown == 0 {
            self.trim_countdown = FILTER_UPDATE_FRAMES;
            let update = |slot: &mut Option<(f32, Coefficients)>, value: f32| {
                let current = slot.is_some_and(|(at, _)| (at - value).abs() < TRIM_OPEN);
                if !current {
                    *slot = Some((value, Coefficients::new(value, 0.0, rate)));
                }
            };
            if low_on {
                if self.low_cut_coefficients.is_none() {
                    self.trim[0].clear_low_cut();
                    self.trim[1].clear_low_cut();
                }
                update(&mut self.low_cut_coefficients, low_cut);
            } else {
                self.low_cut_coefficients = None;
            }
            if high_on {
                if self.high_cut_coefficients.is_none() {
                    self.trim[0].prime_high_cut(input[0]);
                    self.trim[1].prime_high_cut(input[1]);
                }
                update(&mut self.high_cut_coefficients, high_cut);
            } else {
                self.high_cut_coefficients = None;
            }
        }
        self.trim_countdown -= 1;
        let mut output = input;
        for (sample, trim) in output.iter_mut().zip(&mut self.trim) {
            if let Some((_, c)) = &self.low_cut_coefficients {
                *sample = trim.low_cut(*sample, c);
            }
            if let Some((_, c)) = &self.high_cut_coefficients {
                *sample = trim.high_cut(*sample, c);
            }
        }
        output
    }

    /// The mode the filter plays: held `MODE` and what modulation adds, as a whole place in the
    /// list, or its type's first if its type doesn't offer that.
    fn playing_mode(&self) -> FilterMode {
        let moved = self.filter_mode + self.offsets[SettingId::FILTER_MODE.index()] * 255.0;
        let place = moved.round().clamp(0.0, f32::from(u8::MAX)) as u8;
        self.filter_type
            .plays(FilterMode::at(place).unwrap_or_default())
    }

    /// Filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub fn shape(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> ShapedFrame {
        let cutoff = (self.cutoff.next(smoothing)
            + self.cut_offset.next(fast)
            + self.cut_envelope.next(fast))
        .clamp(0.0, 1.0);
        let resonance = (self.resonance.next(smoothing)
            + self.res_offset.next(fast)
            + self.res_envelope.next(fast))
        .clamp(0.0, 1.0);
        let position =
            (self.pan.next(smoothing) + self.pan_offset.next(fast) + self.pan_envelope.next(fast))
                .clamp(0.0, 1.0);
        let drive = self.drive.next(smoothing);
        let open = self.filter_type == FilterType::LowPass
            && cutoff > 0.999
            && resonance < 0.001
            && drive < 0.001
            && !self.filter_moving;
        let filtered = if open {
            self.coefficients = None;
            input
        } else {
            if self.filter_countdown == 0 || self.coefficients.is_none() {
                let mode = self.playing_mode();
                self.coefficients = Some(Coefficients::with_mode(cutoff, resonance, mode, rate));
                self.filter_countdown = FILTER_UPDATE_FRAMES;
            }
            self.filter_countdown -= 1;
            // A drive this small takes the clean path rather than a very slight clip.
            let drive = if drive < 0.001 { 0.0 } else { drive };
            match &self.coefficients {
                Some(c) => [
                    self.filter[0].process(input[0], self.filter_type, drive, c),
                    self.filter[1].process(input[1], self.filter_type, drive, c),
                ],
                None => input,
            }
        };
        let mut filtered = self.trimmed(filtered, smoothing, rate);
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            // A setting being modulated keeps up, and a change on its own doesn't jump.
            let keeping_up = if self.slot_moving[slot] {
                fast
            } else {
                smoothing
            };
            filtered = insert.process(filtered, keeping_up, rate, keys);
        }
        self.key = filtered;
        // What's moved by modulation keeps up, and a change on its own doesn't jump.
        let keeping_up = |target: SettingId| {
            if self.offsets[target.index()] == 0.0 {
                smoothing
            } else {
                fast
            }
        };
        let (vol, to_a, to_b) = (
            keeping_up(SettingId::VOL),
            keeping_up(SettingId::SEND_A),
            keeping_up(SettingId::SEND_B),
        );
        let gain = self.gain.next(vol) * self.level.next(smoothing);
        let (left, right) = self.pan_gains(position);
        let dry = [filtered[0] * gain * left, filtered[1] * gain * right];
        let send_a = self.send_a_gain.next(to_a);
        let send_b = self.send_b_gain.next(to_b);
        ShapedFrame {
            dry,
            send_a: [dry[0] * send_a, dry[1] * send_a],
            send_b: [dry[0] * send_b, dry[1] * send_b],
        }
    }

    /// Run a frame through the slots and level of a track with no voice of its own.
    #[inline]
    fn slots_and_level(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        let mut frame = input;
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            let keeping_up = if self.slot_moving[slot] {
                fast
            } else {
                smoothing
            };
            frame = insert.process(frame, keeping_up, rate, keys);
        }
        self.key = frame;
        let vol = if self.offsets[SettingId::VOL.index()] == 0.0 {
            smoothing
        } else {
            fast
        };
        let gain = self.gain.next(vol) * self.level.next(smoothing);
        [frame[0] * gain, frame[1] * gain]
    }

    /// What a return adds to the mix for what's sent to it: its slots, level and pan, then whether
    /// it's heard, faded so muting doesn't click. With nothing going in and nothing left in its
    /// slots, it does no work.
    #[inline]
    pub fn process_return(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        if input == [0.0, 0.0] && !self.ringing(rate) {
            self.key = [0.0, 0.0];
            return [0.0, 0.0];
        }
        let frame = self.slots_and_level(input, smoothing, fast, rate, keys);
        let position =
            (self.pan.next(smoothing) + self.pan_offset.next(fast) + self.pan_envelope.next(fast))
                .clamp(0.0, 1.0);
        // At the centre, what the effect gave comes through untouched, where `pan_gains` is only close.
        let (left, right) = if position == 0.5 {
            (1.0, 1.0)
        } else {
            self.pan_gains(position)
        };
        let heard = self.heard.next(smoothing);
        let out = [frame[0] * left * heard, frame[1] * right * heard];
        self.reading.add(out, out);
        out
    }

    /// The master's slots and level, for the whole mix.
    #[inline]
    pub fn process_master(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        self.slots_and_level(input, smoothing, fast, rate, keys)
    }
}

/// Each track's sound after its slots and before its level and mute, from the latest frame, for a
/// compressor that hears another track. A track renders in order, so one later than the listener
/// is heard a frame late, which is far too little to hear.
#[derive(Debug, Clone)]
pub struct Keys {
    pub frames: [[f32; 2]; MAX_TRACKS],
    /// How many sound tracks there are, so the returns' places are known.
    pub sounds: usize,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            frames: [[0.0; 2]; MAX_TRACKS],
            sounds: 0,
        }
    }
}

impl Keys {
    /// A track's latest frame, or silence for one that isn't there.
    pub fn of(&self, track: usize) -> [f32; 2] {
        self.frames.get(track).copied().unwrap_or_default()
    }
}

/// One track's frame, and what it sends to the returns.
#[derive(Debug, Clone, Copy)]
pub struct ShapedFrame {
    pub dry: [f32; 2],
    pub send_a: [f32; 2],
    pub send_b: [f32; 2],
}

#[cfg(test)]
mod tests {
    use super::*;
    use vt_model::song::Machine;

    #[test]
    fn a_value_for_a_voice_not_playing_waits_for_it() {
        let mut state = TrackState::new(0, 0);
        let on = |voice| Track {
            machine: Machine::Plaits(voice),
            ..Track::default()
        };
        state.load(&on(SynthVoice::Kick));
        let kick_decay = state.synth_settings().morph;
        let string_decay = SettingId::voice(SynthVoice::String, 2).unwrap();
        state.set_value(string_decay, 0x1000, Mark::Hold);
        assert_eq!(
            state.synth_settings().morph,
            kick_decay,
            "the Kick doesn't hear it"
        );
        state.load(&on(SynthVoice::String));
        assert_eq!(state.synth_settings().morph, 0x10 as f32 / 255.0);
    }

    #[test]
    fn a_tracks_own_fine_part_is_in_its_base() {
        let mut track = Track {
            cutoff: 0x40,
            ..Track::default()
        };
        track.set_fine(SettingId::CUT, 0x80);
        assert_eq!(base_of(SettingId::CUT, &track), 0x4080);
        track.cutoff = 0xFF;
        track.set_fine(SettingId::CUT, 0x80);
        assert_eq!(base_of(SettingId::CUT, &track), 0xFF00, "none at the top");
    }

    #[test]
    fn a_values_fine_part_is_heard_between_its_coarse_steps() {
        let mut state = TrackState::new(0, 0);
        state.load(&Track::default());
        state.set_value(SettingId::CUT, 0x4080, Mark::Lock);
        assert_eq!(state.cutoff.target, 64.5 / 255.0);
        state.set_value(SettingId::CUT, 0x4000, Mark::Lock);
        assert_eq!(
            state.cutoff.target,
            0x40 as f32 / 255.0,
            "the same as before fine parts"
        );
    }

    #[test]
    fn a_value_for_an_effect_not_in_its_slot_waits_for_it() {
        use vt_model::insert::Insert;
        let mut state = TrackState::new(0, 0);
        let mut track = Track::default();
        track.inserts[1] = Insert::ALL[3];
        state.load(&track);
        let feedback = SettingId::slot(1, Effect::Delay, 1).unwrap();
        state.set_value(feedback, 0x3000, Mark::Hold);
        assert_ne!(
            state.slot_values[1][1], 0x30 as f32,
            "the Reverb doesn't hear it"
        );
        track.inserts[1] = Insert::ALL[2];
        state.load(&track);
        assert_eq!(state.slot_values[1][1], 0x30 as f32);
    }

    #[test]
    fn pan_is_exactly_centred_at_80_and_keeps_power() {
        assert_eq!(pan_position(0x80 as f32), 0.5);
        assert_eq!(pan_position(0x00 as f32), 0.0);
        assert_eq!(pan_position(0xFF as f32), 1.0);
        let (l, r) = pan_gains(0.5);
        assert!((l - 1.0).abs() < 1e-5 && (r - 1.0).abs() < 1e-5);
        let (l, r) = pan_gains(0.2);
        assert!((l * l + r * r - 2.0).abs() < 1e-5);
    }
}
