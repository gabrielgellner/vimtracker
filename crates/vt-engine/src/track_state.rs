//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::accumulator::{Accumulation, Moves, Walk};
use vt_model::arpeggio::{ArpMode, Cycle};
use vt_model::constants::{FX_SLOTS, MAX_TRACKS, UNITY_LEVEL};
use vt_model::envelope::{Envelope, Lpg};
use vt_model::groove::Groove;
use vt_model::insert::{INSERTS, Insert, MAX_KEPT_PARAMS, MAX_PARAMS, Setting};
use vt_model::lfo::{LfoMode, MAX_DEFORM, deformed_wave, phase_of, phase_value};
use vt_model::modulation::{
    MAX_DEPTH, MOD_SLOTS, ModKind, ModSlot, Polarity, depth_of, depth_value, note_shape_slot,
};
use vt_model::playable::Playable;
use vt_model::scale::Scale;
use vt_model::settings::{
    Effect, FADER_COUNT, FADERS, MAX_MACHINE_SETTINGS, Owner, PlayMode, SAMPLER_SETTINGS,
    SettingId, gate_delta, gate_share, glide_ms, moved_velocity, quantised_nudge,
};
use vt_model::song::{DEFAULT_ACCENT, FilterMode, FilterType, SynthVoice, Track, level_gain};
use vt_model::step::{FxSlot, Mark, slice_letter};
use vt_model::steps::{MAX_STEP, STEPS, Steps, StepsMode, smoothed};

use crate::dsp::blocks::sides::same_sides;
use crate::dsp::distort::{BYPASS_FADE_MS, InsertState};
use crate::dsp::filter::{Channel, Coefficients, Ramp, Trim, safe_resonance};
use crate::dsp::memory::MemoryPool;
use crate::dsp::smoother::Smoothed;
use crate::dsp::virtual_analog::Settings;
use crate::mixer::Reading;
use crate::rng::Pcg32;
use crate::timing::SlotTimes;
use crate::voice::{Cut, NoteOn, Voice};

/// The most settings a track's envelopes can move at once: one for each slot, and `Cut` besides,
/// which a low-pass gate's `Colour` closes whatever its destination.
const MOVED: usize = MOD_SLOTS + 1;

/// What a track's envelopes move, by setting, kept as a short list as there are only a few.
#[derive(Debug, Clone, Copy, Default)]
struct Moved {
    entries: [(usize, f32); MOVED],
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

/// How often filter coefficients are recalculated while the cutoff moves, in frames. The track's
/// filter ramps to each new set over as many frames.
const FILTER_UPDATE_FRAMES: u32 = 32;
/// How close to wide open the utility filter counts as out of the way, on `0..=1`.
const TRIM_OPEN: f32 = 0.001;

/// Where an LFO's cycles count from.
#[derive(Debug, Clone, Copy, Default)]
struct LfoStart {
    tick: f64,
    clock: u64,
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

/// One mod slot as it runs, whatever its type: every type's state is kept going underneath, so a
/// slot whose type changes picks the new one up where it would be had it always been there, and
/// what it adds glides from the old type's to the new one's rather than jumping.
#[derive(Debug, Clone, Copy)]
struct SlotRun {
    /// The type it last ran as, to notice a change: none until it first runs.
    seen: Option<ModKind>,
    /// Where its envelope has got to, which each note starts and ends.
    envelope: EnvelopeRun,
    /// Where its cycles count from.
    start: LfoStart,
    /// Its held depth, phase, polarity and deform, on the `00` to `FF` scale, as steps and the
    /// page set them. The polarity is below zero until it's first set, which is the slot's own.
    values: [f32; 4],
    /// How much of what it moves is heard, from 1 while it's in to 0 while it's switched out,
    /// gliding between them over [`BYPASS_FADE_MS`] as a slot's bypass does, so switching one
    /// doesn't jump.
    share: f32,
    /// What it added last time, in its destination's units over 255.
    out: f32,
    /// What it added before its type changed less what the new type first added, which fades out
    /// as `glide_left` goes from 1 to 0 over [`BYPASS_FADE_MS`].
    glide: f32,
    glide_left: f32,
    /// Where it was in its wave when the modulation was last worked out, if it ran as an LFO.
    now: Option<LfoNow>,
    /// The track's notes since playing started, which a Steps slot in `Note` mode moves on by,
    /// and in `Trig` waits for.
    notes: i64,
    /// The pass of the track's chain its last note was in, which a Steps slot at `Pass` in `Trig`
    /// counts from.
    pass_at: i64,
    /// The step it played when the modulation was last worked out, if it ran as Steps.
    playing: Option<u8>,
    /// The values Steps' `Mutate` has drawn again in place of the written ones, by step, and which
    /// of them are drawn, a bit a step: kept while it plays, so a loop drifts rather than
    /// starting afresh each time round, and forgotten when `Mutate` goes back to none.
    drift: [i16; STEPS],
    drawn: u16,
    /// Where it was counted to when `Mutate` last rolled, so each step coming round rolls once.
    counted: Option<i64>,
    /// What `Mutate` rolls, a stream of the slot's own from the project's seed, after every
    /// track's others, so a render repeats and `CHA`'s rolls are as they were.
    dice: Pcg32,
    /// What a Track slot following `Random` rolled for the last note, and the stream of the slot's
    /// own it rolls from, after every slot's `Mutate`. It rolls at every note, whatever the slot's
    /// type, so one that becomes Track has a roll for the note sounding.
    roll: u8,
    rolls: Pcg32,
}

impl Default for SlotRun {
    fn default() -> Self {
        Self {
            seen: None,
            envelope: EnvelopeRun::Idle,
            start: LfoStart::default(),
            values: [128.0, 0.0, -1.0, 128.0],
            share: 1.0,
            out: 0.0,
            glide: 0.0,
            glide_left: 0.0,
            now: None,
            notes: 0,
            pass_at: 0,
            playing: None,
            drift: [0; STEPS],
            drawn: 0,
            counted: None,
            dice: Pcg32::new(0, 0),
            roll: 0,
            rolls: Pcg32::new(0, 0),
        }
    }
}

impl SlotRun {
    /// Draw the value of the step that's come round again, with `Mutate`'s chance, once each time
    /// it comes round, within the full `-FF` to `+FF`, which the slot's depth then scales. With no
    /// `Mutate`, the values are as written and what was drawn is forgotten.
    fn mutate(&mut self, steps: &Steps, step: usize, count: i64) {
        if steps.mutate == 0 {
            self.drawn = 0;
            self.counted = None;
            return;
        }
        if self.counted == Some(count) {
            return;
        }
        self.counted = Some(count);
        if self.dice.next_u32() % 100 < u32::from(steps.mutate) {
            self.drift[step] = (self.dice.next_u32() % (2 * MAX_STEP as u32 + 1)) as i16 - MAX_STEP;
            self.drawn |= 1 << step;
        }
    }

    /// A step's value as it's heard, from -1 to 1: drawn again by `Mutate` while it's on, or as
    /// written.
    fn heard(&self, steps: &Steps, step: usize) -> f32 {
        let value = if steps.mutate > 0 && self.drawn & (1 << step) != 0 {
            self.drift[step]
        } else {
            steps.values[step]
        };
        f32::from(value) / f32::from(MAX_STEP)
    }

    /// Which way it moves what it reaches: as a step or the page last set it, or the slot's own.
    fn polarity(&self, slot: &ModSlot) -> Polarity {
        match self.values[2] {
            held if held < 0.0 => slot.polarity(),
            held if held >= 0.5 => Polarity::Bi,
            _ => Polarity::Uni,
        }
    }

    /// What it adds this time, `out` with what's left of a type change's glide, kept as what it
    /// added. A type it's just changed to starts its glide from what the old one last added.
    fn add(&mut self, kind: ModKind, out: f32) -> f32 {
        if self.seen.is_some_and(|seen| seen != kind) {
            self.glide = self.out - out;
            self.glide_left = 1.0;
        }
        self.seen = Some(kind);
        let added = if self.glide_left > 0.0 {
            out + self.glide * self.glide_left
        } else {
            out
        };
        self.out = added;
        added
    }

    /// Add nothing this time, as it moves nothing, which ends any glide.
    fn rest(&mut self, kind: ModKind) {
        self.seen = Some(kind);
        self.out = 0.0;
        self.glide_left = 0.0;
    }
}

/// Start one slot's envelope for a note, from nothing or from where it is, and its cycles again if
/// its mode follows the notes: as Steps, its own mode's, and otherwise its LFO's, which every other
/// type keeps running underneath. It counts the note, and the pass of the chain it's in.
fn trigger_slot(slot: &ModSlot, run: &mut SlotRun, now: SongTime, pass: i64) {
    let envelope = slot.envelope();
    let from = if envelope.reset() {
        0.0
    } else {
        run.envelope.level(&envelope, now.clock, now.rate)
    };
    run.envelope = EnvelopeRun::Held {
        at: now.clock,
        from,
    };
    run.notes += 1;
    run.pass_at = pass;
    run.roll = (run.rolls.next_u32() >> 24) as u8;
    let restarts = match slot.kind {
        ModKind::Steps => slot.steps.mode == StepsMode::Trig,
        _ => slot.lfo.mode.restarts(),
    };
    if restarts {
        run.start = LfoStart {
            tick: now.tick,
            clock: now.clock,
        };
    }
}

/// How many steps a Steps slot has moved on from its start now, and how far through the step it
/// is, from 0 to 1, or none before the note it waits for: one a note in `Note` mode, and otherwise
/// by its rate, from its start, which each note moves in `Trig`, or by passes of the track's chain.
/// Only a step in time has a way through it, for `Smooth` to follow.
#[inline]
fn steps_count(
    slot: &ModSlot,
    run: &SlotRun,
    now: SongTime,
    step_ticks: u64,
    pass: i64,
) -> Option<(i64, Option<f32>)> {
    let steps = &slot.steps;
    match steps.mode {
        StepsMode::Note => (run.notes > 0).then(|| (run.notes - 1, None)),
        StepsMode::Trig if run.notes == 0 => None,
        mode => Some(match steps.rate.ticks(step_ticks) {
            Some(ticks) => {
                let along = ((now.tick - run.start.tick) / ticks).max(0.0);
                (along.floor() as i64, Some(along.fract() as f32))
            }
            None if mode == StepsMode::Trig => (pass - run.pass_at, None),
            None => (pass, None),
        }),
    }
}

/// What a slot in scale adds to the pitch, in semitones over 255: the note it moves to from the
/// one playing, the nearest of the scale's, so its moves land on the scale's notes. A move of no
/// whole semitone leaves the note as written, even one off the scale, as the scale is only ever
/// an aid for what's written (decision 24). With the scale off, or no note, it's as it was.
#[inline]
fn in_scale(added: f32, note: Option<f32>, scale: Scale) -> f32 {
    let Some(note) = note.filter(|_| scale.on) else {
        return added;
    };
    let semitones = (added * 255.0).round();
    if semitones == 0.0 {
        return 0.0;
    }
    let target = (note + semitones).round().clamp(0.0, 127.0) as u8;
    (f32::from(scale.fitted(target)) - note) / 255.0
}

/// How many cycles an LFO slot is through its wave now, before its phase, under its mode: from
/// its start, which a note that restarts it moves, and for `Hold` only as far as the track's last
/// note, or its start if none has played. A one-shot stays at the end of what it plays, just short
/// of it so a saw is still at its bottom, and before any note it's already there.
#[inline]
fn lfo_cycles(slot: &ModSlot, start: LfoStart, noted: Option<LfoStart>, now: SongTime) -> f64 {
    let mode = slot.lfo.mode;
    let until = match (mode, noted) {
        (LfoMode::Hold, Some(note)) => note,
        (LfoMode::Hold, None) => start,
        _ => LfoStart {
            tick: now.tick,
            clock: now.clock,
        },
    };
    let seconds = until.clock.wrapping_sub(start.clock) as f64 / f64::from(now.rate);
    let cycles = slot.lfo.rate.cycles(until.tick - start.tick, seconds);
    match mode.plays_for() {
        Some(most) if noted.is_some() => cycles.min(most - ONE_SHOT_END),
        Some(most) => most - ONE_SHOT_END,
        None => cycles,
    }
}

/// How far short of its end a one-shot LFO stays: far enough that its place is still in the cycle
/// it played, and too little to hear.
const ONE_SHOT_END: f64 = 1e-6;

/// The key a slot's random waves are picked by: slots 4 to 6 have the keys the three LFOs had
/// before mod slots, so their `Random` and `Smooth` waves are as they were, and slots 1 to 3 the
/// next three.
fn wave_key(slot: usize) -> u64 {
    ((slot + MOD_SLOTS - 3) % MOD_SLOTS) as u64
}

/// Where the song is, for working out LFOs.
#[derive(Debug, Clone, Copy)]
pub struct SongTime {
    /// The song's scale, which slots in scale move the pitch along.
    pub scale: Scale,
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
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// The highest level of each side since the last report, right after the trim and before the
    /// filter, as a gain: what the Output device's meter shows, so the trim can be set by eye.
    /// The report fills it in from `Engine::take_after_trim`.
    pub after_trim: [f32; 2],
    /// Where each mod slot that's an LFO is in its wave now, or a Track slot's last note on its
    /// map, for one that's running: none while the song is stopped, before a Track slot's first
    /// note, or for one with nowhere to go, no depth, or switched out once it has faded.
    pub mods: [Option<LfoNow>; MOD_SLOTS],
    /// Which step each mod slot that's Steps is playing now, counted from 0, for the Mod page to
    /// light: none for one that isn't running, as an LFO's place is.
    pub steps: [Option<u8>; MOD_SLOTS],
}

/// Written out, as an array as long as `4-op`'s settings has no `Default` of its own.
impl Default for LiveValues {
    fn default() -> Self {
        Self {
            level: 0,
            pan: 0,
            cutoff: 0,
            resonance: 0,
            send_a: 0,
            send_b: 0,
            machine: [0; MAX_MACHINE_SETTINGS],
            inserts: Default::default(),
            reduction: Default::default(),
            bypassed: Default::default(),
            after_trim: Default::default(),
            mods: Default::default(),
            steps: Default::default(),
        }
    }
}

/// A track's faders as they're heard, `LEV`, `PAN`, `SNA` and `SNB` in [`FADERS`]' order, for the
/// mixer, the Output device and the Roto to show (decision 174).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Faders {
    /// Where each is: a step's value while the steps drive it, or the track page's, with a coarse
    /// byte and a fine one.
    pub at: [u16; FADER_COUNT],
    /// What's heard with `VOL` and modulation on top, on the `00` to `FF` scale, for the thin
    /// mark beside a fader.
    pub moved: [u8; FADER_COUNT],
    /// Whether a step has set each since playing last started from the top, so the steps drive it.
    pub automated: [bool; FADER_COUNT],
    /// Whether a hand has taken each over from the steps, until it's given back.
    pub taken: [bool; FADER_COUNT],
}

impl Faders {
    /// Whether a fader follows the steps now: they drive it, and no hand has taken it over.
    pub fn follows_steps(&self, fader: usize) -> bool {
        self.automated[fader] && !self.taken[fader]
    }
}

/// One of a track's faders as the steps and a hand move it.
#[derive(Debug, Clone, Copy)]
struct FaderRun {
    /// The track page's value, which a hand sets.
    song: u16,
    /// Where it's heard, on the `00` to `FF` scale with any fine part as the fraction.
    at: f32,
    /// Whether a step has set it since playing last started from the top.
    automated: bool,
    /// The last value a step held it at since then, if one has: what's under a lock when it's
    /// given back, whatever the track page's value did meanwhile, as an edit replaces a hold.
    hold: Option<u16>,
    /// Whether a hand has taken it over: the steps' values are kept underneath, unheard.
    taken: bool,
}

impl FaderRun {
    fn at(id: SettingId) -> Self {
        let value = u16::from(id.entry().default) << 8;
        Self {
            song: value,
            at: scaled(value),
            automated: false,
            hold: None,
            taken: false,
        }
    }
}

/// Where an LFO is in its wave and what the wave gives there, before its depth, for the Mod page's
/// moving dot, or where a Track slot's last note is on its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LfoNow {
    /// How far round its wave it is, its `Phase` included, from `0` to a whole turn at
    /// `u16::MAX + 1`.
    pub phase: u16,
    /// The wave's value there, from `-i16::MAX` at its bottom to `i16::MAX` at its top.
    pub value: i16,
}

impl LfoNow {
    /// Where a wave is from its turns, `cycles` with its phase added, and its value from -1 to 1.
    pub fn of(cycles: f64, value: f32) -> Self {
        let turn = f64::from(u16::MAX) + 1.0;
        Self {
            phase: (cycles.rem_euclid(1.0) * turn) as u32 as u16,
            value: (value.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        }
    }

    /// Where a Track slot's last note is on its map: `along` its source's span, from 0 to 1, as
    /// the phase, and the map's level there, from 0 to 1, as the value.
    pub fn along(along: f32, level: f32) -> Self {
        Self {
            phase: (along.clamp(0.0, 1.0) * f32::from(u16::MAX)).round() as u16,
            value: (level.clamp(0.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        }
    }
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
    /// Whether its chain starts again at the next line of the realign grid, as `Space a` asks
    /// while playing.
    pub realign_waiting: bool,
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
    /// The highest level of each side since it was last taken, right after the trim and before
    /// the filter or the slots, for the Output device's meter, so the trim can be set by eye.
    pub after_trim: [f32; 2],
    /// Live controls, kept here rather than in the song so undo can't change them.
    pub mute: bool,
    pub solo: bool,
    /// Whether its mute or its solo switches at the next line of the launch grid, as `Space g m`
    /// and `Space g s` ask while playing.
    pub mute_waiting: bool,
    pub solo_waiting: bool,
    /// How many switches waiting for the grid have landed, so the edit thread can tell a landing
    /// from its own switches and follow it.
    pub switches_landed: u8,
    /// On a return, whether it's heard, so muting fades.
    pub heard: Smoothed,
    /// On a sound track in a group, whether it plays into the group. It's what's heard, but while a
    /// compressor, Warps or a gate hears the group, the group's own mute and the solos elsewhere
    /// are left to the group, so the group still has its tracks to hear while it's silent.
    pub feeds: Smoothed,
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
    /// The seeds of each note's drift, for a synth that drifts as an analog one does, apart from
    /// `rng` so `CHA` rolls as it did.
    drift_rng: Pcg32,
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
    /// The gain of held `LEV`, the fader, whose base is the track's level.
    pub level: Smoothed,
    /// Its faders, as the steps automate them and a hand takes them over.
    faders: [FaderRun; FADER_COUNT],
    /// The track's trim from the song, the gain at its input, which playback leaves alone too.
    pub input_trim: Smoothed,
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
    /// Held `Punch`, which only the clean types hear.
    punch: Smoothed,
    /// Held `Safe`, 1 for on, which glides so switching it doesn't jump `Res`.
    safe: Smoothed,
    trim: [Trim; 2],
    low_cut: Smoothed,
    /// What each of its effects keeps.
    inserts: [InsertState; INSERTS],
    /// What's heard for each setting of each slot, before modulation, as a step command gives it.
    slot_values: [[f32; MAX_KEPT_PARAMS]; INSERTS],
    /// What each slot's effect is playing with now.
    resolved: [Insert; INSERTS],
    /// Whether modulation is moving each slot.
    slot_moving: [bool; INSERTS],
    /// Whether a step has switched each slot's trigger on since the slots were last given their
    /// settings, which starts a stutter again even if it was on already.
    struck: [bool; INSERTS],
    high_cut: Smoothed,
    /// The coefficients in use, and the value each was worked out for.
    low_cut_coefficients: Option<(f32, Coefficients)>,
    high_cut_coefficients: Option<(f32, Coefficients)>,
    trim_countdown: u32,
    /// The filter's coefficients, ramping toward the set last worked out.
    coefficients: Option<Ramp>,
    filter_countdown: u32,
    /// Each mod slot as it runs.
    slots: [SlotRun; MOD_SLOTS],
    /// How many times round its chain the track is, as the step it last played counts, which a
    /// Steps slot at `Pass` moves on by.
    pub chain_pass: i64,
    /// Where the track's last note started its modulation, for an LFO's fade and its hold, or
    /// none since playing started.
    noted: Option<LfoStart>,
    /// Whether a note has started or been tied into since the modulation was last worked out,
    /// which a Track slot follows, so the engine works it out again before the note is heard.
    pub new_note: bool,
    /// What the LFOs add to each held value, by setting. Separate from the held values, so a
    /// step's FX moves the centre and the LFO carries on around it.
    offsets: [f32; SettingId::COUNT],
    /// The settings `offsets` has anything on, at most one for each envelope and LFO, so only those
    /// are cleared next time rather than every setting.
    offset_marks: [Option<usize>; MOVED],
    cut_offset: Smoothed,
    res_offset: Smoothed,
    pan_offset: Smoothed,
    /// What envelopes add, which moves fast and needs to keep its snap.
    cut_envelope: Smoothed,
    res_envelope: Smoothed,
    pan_envelope: Smoothed,
    /// The last pan position and its gains, as a settled pan stays put and `cos` and `sin` cost.
    panned: (f32, (f32, f32)),
    /// What modulation multiplies a note's speed by.
    pitch: f64,
    /// The held pitch, on the `00` to `FF` scale, as steps and the page set it.
    held_pitch: f32,
    /// Held `LEN`, the track's `Gate`, on the `00` to `FF` scale.
    held_gate: f32,
    /// Held `LDLT`, which moves each note's length up or down from its own, with `80` as none.
    held_gate_delta: f32,
    /// Held `VDLT`, which moves each note's velocity up or down from its own, with `80` as none.
    held_velocity_delta: f32,
    /// Held `GLD`, how long a tie glides into the next note, on the `00` to `FF` scale.
    held_glide: f32,
    /// Held `SMP`, `STA`, `SLEN` and `PLY`, the sampler's settings, which its next note reads as it
    /// starts, on the `00` to `FF` scale: the bank entry it plays by number, and how it cuts it.
    held_sampler: [f32; SAMPLER_SETTINGS],
    /// Whether each slot is switched out as it's heard: its own bypass, or a step's lock of it.
    bypassed: [bool; INSERTS],
    /// The frames rendered when the shares last moved.
    shared_at: u64,
    /// The loaded voice's held settings, in its order, on `0..=1`.
    synth: [f32; MAX_MACHINE_SETTINGS],
    /// Whether an LFO moves the filter, so it stays running at the top of its range.
    filter_moving: bool,
    /// Its sound after its slots in the latest frame, for a compressor on another track to hear.
    pub key: [f32; 2],
    /// How long each slot has taken, while `vimtracker bench --tracks` times the engine.
    pub slot_times: SlotTimes,
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
/// another effect is there, and a mod slot's depth and phase are what its page has.
pub fn base_of(id: SettingId, track: &Track) -> u16 {
    let modulator = |slot: usize, at: usize| match at {
        0 => depth_value(track.mods[slot].depth),
        1 if track.mods[slot].kind == ModKind::Steps => track.mods[slot].steps.start_value(),
        1 => phase_value(track.mods[slot].lfo.phase),
        2 => u16::from(track.mods[slot].polarity().index()) << 8,
        _ => depth_value(track.mods[slot].lfo.deform),
    };
    let whole = match id.owner() {
        (Owner::Mod(n), at) => return modulator(n, at),
        (Owner::Track, _) => match id {
            SettingId::VOL => UNITY_LEVEL,
            SettingId::LEVEL => track.level,
            SettingId::PAN => track.pan,
            SettingId::CUT => track.cutoff,
            SettingId::RES => track.resonance,
            SettingId::SEND_A => track.send_a,
            SettingId::SEND_B => track.send_b,
            SettingId::PITCH => track.pitch,
            SettingId::VELOCITY_DELTA => track.velocity_delta,
            SettingId::QUANTISE => track.quantise,
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

/// The generator for a mod slot's `Mutate`, on streams after every track's drift.
fn mutate_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(seed, (4 * MAX_TRACKS + index * MOD_SLOTS + slot) as u64)
}

/// The generator for a Track mod slot's `Random`, on streams after every slot's `Mutate`.
fn roll_rng(seed: u64, index: usize, slot: usize) -> Pcg32 {
    Pcg32::new(
        seed,
        (4 * MAX_TRACKS + (MAX_TRACKS + index) * MOD_SLOTS + slot) as u64,
    )
}

/// The generator for a track's notes' drift, on streams after every track's arpeggios.
fn drift_rng(seed: u64, index: usize) -> Pcg32 {
    Pcg32::new(seed, (3 * MAX_TRACKS + index) as u64)
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
            realign_waiting: false,
            accumulators: Accumulators::default(),
            accumulated: None,
            voice: Voice::default(),
            tail: Voice::default(),
            reading: Reading::default(),
            mute: false,
            solo: false,
            mute_waiting: false,
            solo_waiting: false,
            switches_landed: 0,
            heard: Smoothed::at(1.0),
            feeds: Smoothed::at(1.0),
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
            drift_rng: drift_rng(seed, index),
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
            faders: FADERS.map(FaderRun::at),
            input_trim: Smoothed::at(1.0),
            after_trim: [0.0; 2],
            pan: Smoothed::at(0.5),
            cutoff: Smoothed::at(1.0),
            resonance: Smoothed::at(0.0),
            send_a_gain: Smoothed::at(0.0),
            send_b_gain: Smoothed::at(0.0),
            filter: [Channel::default(); 2],
            filter_type: FilterType::LowPass,
            filter_mode: 0.0,
            drive: Smoothed::at(0.0),
            punch: Smoothed::at(0.0),
            safe: Smoothed::at(0.0),
            trim: [Trim::default(); 2],
            low_cut: Smoothed::at(0.0),
            inserts: std::array::from_fn(|_| InsertState::default()),
            gliding: 0,
            slot_values: [[0.0; MAX_KEPT_PARAMS]; INSERTS],
            resolved: [Insert::Empty; INSERTS],
            slot_moving: [false; INSERTS],
            struck: [false; INSERTS],
            high_cut: Smoothed::at(1.0),
            low_cut_coefficients: None,
            high_cut_coefficients: None,
            trim_countdown: 0,
            coefficients: None,
            filter_countdown: 0,
            slots: [SlotRun::default(); MOD_SLOTS],
            chain_pass: 0,
            noted: None,
            new_note: false,
            offsets: [0.0; SettingId::COUNT],
            offset_marks: [None; MOVED],
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
            pitch: 1.0,
            held_pitch: 128.0,
            held_gate: f32::from(SettingId::GATE_LENGTH.entry().default),
            held_gate_delta: f32::from(SettingId::GATE_DELTA.entry().default),
            held_velocity_delta: f32::from(SettingId::VELOCITY_DELTA.entry().default),
            held_glide: f32::from(SettingId::GLIDE.entry().default),
            held_sampler: std::array::from_fn(|at| {
                SettingId::sampler(at).map_or(0.0, |id| f32::from(id.entry().default))
            }),
            bypassed: [false; INSERTS],
            shared_at: 0,
            filter_moving: false,
            key: [0.0; 2],
            slot_times: SlotTimes::default(),
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
        self.new_note = true;
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
        self.reset_all_but_settings(seed, index, clock);
        self.load(track);
        self.settle_shares(track);
        for (run, id) in self.faders.iter_mut().zip(FADERS) {
            run.song = base_of(id, track);
            run.automated = false;
            run.hold = None;
        }
        for id in SettingId::all() {
            let base = base_of(id, track);
            self.values[id.index()] = ValueLayers::at(base);
            self.write_value(id, scaled(base));
        }
        self.gliding = 0;
    }

    /// Reset what playing from the top does apart from the settings: the random generators,
    /// conditions, accumulators, arpeggio, last note and LFOs. Enough on its own for a state no
    /// track has had since its settings were last reset to an empty track's, as nothing else
    /// changes them, and going through every setting is most of a reset's time.
    pub fn reset_all_but_settings(&mut self, seed: u64, index: usize, clock: u64) {
        self.rng = Pcg32::new(seed, index as u64);
        self.forget_conditions();
        self.forget_accumulators();
        self.groove_rng = groove_rng(seed, index);
        self.arp_rng = arp_rng(seed, index);
        self.drift_rng = drift_rng(seed, index);
        self.arp = None;
        self.last_note = None;
        self.chain_pass = 0;
        for (at, run) in self.slots.iter_mut().enumerate() {
            run.start = LfoStart { tick: 0.0, clock };
            run.notes = 0;
            run.dice = mutate_rng(seed, index, at);
            run.rolls = roll_rng(seed, index, at);
            run.roll = 0;
            run.drawn = 0;
            run.counted = None;
        }
        self.noted = None;
    }

    /// Count every slot's cycles from the top of the song, as playing starts there, keeping the
    /// clock they count real time from.
    pub fn count_cycles_from_the_top(&mut self) {
        for run in &mut self.slots {
            run.start.tick = 0.0;
            run.notes = 0;
            run.counted = None;
        }
        self.noted = None;
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

    /// Work out what the mod slots add now. A stopped track sits at its held values, so an
    /// audition hears those rather than wherever an LFO stopped.
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
        self.move_shares(track, now.rendered, now.rate);
        self.filter_moving = false;
        // Only what was moved last time needs clearing.
        for index in self.offset_marks.iter_mut().filter_map(Option::take) {
            self.offsets[index] = 0.0;
        }
        // Each slot in order, so each reads what the slots before it add to its own depth and
        // phase, and there's no way back round. What the envelopes move is kept by setting, as
        // there are only a few, and goes through the fast smoothers; what the LFOs move goes
        // straight into the offsets, through the slow ones.
        let mut moved = Moved::default();
        let mut marks = 0;
        let mut filter_moving = false;
        let shaping = note_shape_slot(&track.mods);
        let has_cut = SettingId::CUT.exists_on(track);
        let offsets = &mut self.offsets;
        let (step_ticks, pass) = (track.speed.ticks(), self.chain_pass);
        let noted = self.noted;
        let last_note = self.last_note;
        // The note as it's heard before modulation, for slots in scale to move from.
        let note = self
            .last_note
            .map(|played| f32::from(played.note) + self.held_pitch - 128.0);
        for (at, (slot, run)) in track.mods.iter().zip(&mut self.slots).enumerate() {
            run.now = None;
            run.playing = None;
            let Some(destination) = slot.destination else {
                run.rest(slot.kind);
                continue;
            };
            // A low-pass gate's fall closes the track's filter too, by its `Colour`, wherever it
            // goes and whatever its depth, the notes' shape included, through the fast smoothers
            // as an envelope's moves are (#45). One going nowhere does nothing, as any slot.
            if slot.kind == ModKind::Lpg && slot.lpg.colour > 0 && has_cut && !stopped {
                let level = run.envelope.level(&slot.envelope(), now.clock, now.rate);
                let closed = f32::from(slot.lpg.colour) / 255.0 * Lpg::closing(level);
                moved.add(SettingId::CUT.index(), -closed * run.share);
                filter_moving = true;
            }
            let moved_by = |setting: usize| {
                SettingId::modulator(at, setting)
                    .map_or(0.0, |id| offsets[id.index()] + moved.at(id.index()))
            };
            let [depth, phase, _, deform] = run.values;
            let polarity = run.polarity(slot);
            // An envelope's depth is whole, as its page sets it, so it moves by exactly that.
            let held = match slot.kind {
                ModKind::Adsr
                | ModKind::Ahd
                | ModKind::Drum
                | ModKind::Lpg
                | ModKind::Steps
                | ModKind::Track => depth_of(depth).round(),
                ModKind::Lfo => depth_of(depth),
            };
            let depth = held + moved_by(0) * 255.0;
            let share = run.share;
            // The note-shaping slot is each note's own shape, which the voice follows itself. A
            // target that isn't on this track, or is a slot before this one, does no work, and
            // nor does a slot switched out, once it has faded.
            if depth == 0.0
                || share == 0.0
                || shaping == Some(at)
                || !destination.reached_from(at, track)
            {
                run.rest(slot.kind);
                continue;
            }
            filter_moving |= matches!(destination, SettingId::CUT | SettingId::RES);
            if stopped {
                run.rest(slot.kind);
                continue;
            }
            let depth = depth.clamp(-f32::from(MAX_DEPTH), f32::from(MAX_DEPTH));
            // A slot in scale on `Pitch` moves the note to the scale's notes.
            let tuned = |added: f32| {
                if slot.in_scale && destination == SettingId::PITCH {
                    in_scale(added, note, now.scale)
                } else {
                    added
                }
            };
            match slot.kind {
                ModKind::Adsr | ModKind::Ahd | ModKind::Drum | ModKind::Lpg => {
                    let level = run.envelope.level(&slot.envelope(), now.clock, now.rate);
                    let level = polarity.of(Polarity::Uni, level);
                    let added = run.add(slot.kind, tuned(level * depth / 255.0) * share);
                    moved.add(destination.index(), added);
                }
                ModKind::Lfo => {
                    let cycles = lfo_cycles(slot, run.start, noted, now);
                    let key = now.seed ^ ((index as u64) << 8 | wave_key(at));
                    let phase = f64::from(phase_of(phase) + moved_by(1));
                    let deform = ((depth_of(deform) + moved_by(3) * 255.0) / f32::from(MAX_DEFORM))
                        .clamp(-1.0, 1.0);
                    let value = deformed_wave(slot.lfo.shape, cycles + phase, key, deform);
                    run.now = Some(LfoNow::of(cycles + phase, value));
                    let since_note = noted.map(|note| now.tick - note.tick);
                    let value = polarity.of(Polarity::Bi, value) * slot.lfo.fade.gain(since_note);
                    offsets[destination.index()] +=
                        run.add(slot.kind, tuned(value * depth / 255.0) * share);
                    self.offset_marks[marks] = Some(destination.index());
                    marks += 1;
                }
                // A step's value, heard as written at a depth of `+FF`, through the fast
                // smoothers as an envelope's is, as it moves in steps.
                ModKind::Steps => {
                    let steps = &slot.steps;
                    let start = steps.start_of(phase + moved_by(1) * 256.0);
                    let key = now.seed ^ ((index as u64) << 8 | wave_key(at));
                    let level = match steps_count(slot, run, now, step_ticks, pass) {
                        None => 0.0,
                        Some((count, through)) => {
                            let step = steps.step_at(start, count, key);
                            run.playing = Some(step as u8);
                            run.mutate(steps, step, count);
                            let heard =
                                |count: i64| run.heard(steps, steps.step_at(start, count, key));
                            let smooth = f32::from(steps.smooth) / f32::from(MAX_STEP);
                            match through {
                                Some(through) if smooth != 0.0 => smoothed(
                                    [
                                        heard(count - 2),
                                        heard(count - 1),
                                        heard(count),
                                        heard(count + 1),
                                    ],
                                    through,
                                    smooth,
                                ),
                                _ => heard(count),
                            }
                        }
                    };
                    let added = run.add(slot.kind, tuned(level * depth / 255.0) * share);
                    moved.add(destination.index(), added);
                }
                // The last note's value on the map, held to the next note, through the fast
                // smoothers as a step's value is. Before any note it moves nothing.
                ModKind::Track => {
                    let Some(played) = last_note else {
                        run.rest(slot.kind);
                        continue;
                    };
                    let tracking = &slot.track;
                    let value = tracking.value(played.note, played.velocity, run.roll);
                    let level = tracking.bounds().level(f32::from(value));
                    run.now = Some(LfoNow::along(
                        tracking.source.along(f32::from(value)),
                        level,
                    ));
                    let level = polarity.of(Polarity::Uni, level);
                    let added = run.add(slot.kind, tuned(level * depth / 255.0) * share);
                    moved.add(destination.index(), added);
                }
            }
        }
        self.filter_moving = filter_moving;
        // A different filter starts from silence rather than from another's state.
        if self.filter_type != track.filter {
            self.filter_type = track.filter;
            self.filter = [Channel::default(); 2];
            self.coefficients = None;
        }
        self.drive.target = f32::from(track.drive) / 255.0;
        self.punch.target = f32::from(track.punch) / 255.0;
        self.safe.target = f32::from(u8::from(track.safe));
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
            if std::mem::take(&mut self.struck[slot]) {
                self.inserts[slot].strike();
            }
            self.inserts[slot].bypass(self.bypassed[slot]);
        }
        let semitones = self.held_pitch - 128.0 + offsets[SettingId::PITCH.index()] * 255.0;
        self.pitch = 2_f64.powf(f64::from(semitones.clamp(-96.0, 96.0)) / 12.0);
        self.voice.set_pitch(self.pitch);
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
        self.tell_slots_the_note();
    }

    /// Move each mod slot's share toward 1 if it's in or 0 if it's switched out, and what's left
    /// of a type change's glide toward 0, over [`BYPASS_FADE_MS`]. It goes by the frames rendered
    /// since last time, so a note's extra update between the regular ones moves them no further.
    fn move_shares(&mut self, track: &Track, rendered: u64, rate: u32) {
        let frames = rendered.saturating_sub(self.shared_at);
        self.shared_at = rendered;
        let by = frames as f32 * 1000.0 / (BYPASS_FADE_MS * rate as f32);
        for (run, slot) in self.slots.iter_mut().zip(&track.mods) {
            let to = if slot.bypassed { 0.0 } else { 1.0 };
            run.share = if to > run.share {
                (run.share + by).min(to)
            } else {
                (run.share - by).max(to)
            };
            run.glide_left = (run.glide_left - by).max(0.0);
        }
    }

    /// Put each mod slot's share where its switch is, and take each one's type as it is, with no
    /// glide, for a song that starts playing with some switched out.
    fn settle_shares(&mut self, track: &Track) {
        for (run, slot) in self.slots.iter_mut().zip(&track.mods) {
            run.share = if slot.bypassed { 0.0 } else { 1.0 };
            run.seen = Some(slot.kind);
            run.glide_left = 0.0;
        }
    }

    /// The pan gains for a position, worked out again only when it moves.
    #[inline]
    fn pan_gains(&mut self, position: f32) -> (f32, f32) {
        if position.to_bits() != self.panned.0.to_bits() {
            self.panned = (position, pan_gains(position));
        }
        self.panned.1
    }

    /// Its faders as they're heard: where each is, and what's heard with `VOL` and modulation on
    /// top, which is shown against the fader's `Level`, as they both set how loud it is. The first
    /// envelope on `Vol` shapes notes, and isn't a movement of the level.
    pub fn faders(&self) -> Faders {
        let offset = |destination: SettingId| self.offsets[destination.index()];
        let on_level = |held: f32, destination| {
            (held + offset(destination) * 255.0)
                .clamp(0.0, 255.0)
                .round() as u8
        };
        let vol = self.vol + offset(SettingId::VOL) * 255.0 - f32::from(UNITY_LEVEL);
        let level = self.faders[0].at;
        Faders {
            at: std::array::from_fn(|fader| {
                let top = f32::from(FADERS[fader].entry().kind.top());
                (self.faders[fader].at.clamp(0.0, top) * 256.0).round() as u16
            }),
            moved: [
                (level + vol).clamp(0.0, 255.0).round() as u8,
                ((self.pan.target + offset(SettingId::PAN)).clamp(0.0, 1.0) * 255.0).round() as u8,
                on_level(self.send_a, SettingId::SEND_A),
                on_level(self.send_b, SettingId::SEND_B),
            ],
            automated: self.faders.map(|run| run.automated),
            taken: self.faders.map(|run| run.taken),
        }
    }

    /// What this track is playing with right now, its faders as [`TrackState::faders`] hears
    /// them.
    pub fn live(&self) -> LiveValues {
        let offset = |destination: SettingId| self.offsets[destination.index()];
        let moved = |held: f32, destination| {
            ((held + offset(destination)).clamp(0.0, 1.0) * 255.0).round() as u8
        };
        let synth = self.synth_settings();
        let shown = |value: f32| (value * 255.0).round() as u8;
        let [level, pan, send_a, send_b] = self.faders().moved;
        LiveValues {
            level,
            pan,
            cutoff: moved(self.cutoff.target, SettingId::CUT),
            resonance: moved(self.resonance.target, SettingId::RES),
            send_a,
            send_b,
            machine: match self.loaded_voice {
                Some(_) => std::array::from_fn(|place| shown(synth.at(place))),
                None => std::array::from_fn(|place| {
                    SettingId::sampler(place).map_or(0, |id| self.sampler_value(id).round() as u8)
                }),
            },
            inserts: self.resolved.map(|insert| {
                let mut values = [0; MAX_PARAMS];
                for (param, value) in values.iter_mut().enumerate() {
                    *value = insert.value(param);
                }
                values
            }),
            reduction: std::array::from_fn(|slot| self.inserts[slot].reduction()),
            bypassed: self.bypassed,
            // Taken, as it's the highest since it last was, by `Engine::take_after_trim`, which the
            // report fills this in from.
            after_trim: [0.0; 2],
            mods: self.slots.map(|run| run.now),
            steps: self.slots.map(|run| run.playing),
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

    /// Start a note's modulation: every slot's envelope, and the cycles of each that a note
    /// starts, whatever type each is now, so one that changes type picks the new one up as though
    /// it had always been it.
    pub fn trigger_mods(&mut self, track: &Track, now: SongTime) {
        for (slot, run) in track.mods.iter().zip(&mut self.slots) {
            trigger_slot(slot, run, now, self.chain_pass);
        }
        self.noted = Some(LfoStart {
            tick: now.tick,
            clock: now.clock,
        });
    }

    /// Start the release of every slot's envelope, when a note ends, but a one-shot's, which plays
    /// through whatever the note's length.
    pub fn release_mods(&mut self, track: &Track, now: SongTime) {
        for (slot, run) in track.mods.iter().zip(&mut self.slots) {
            let envelope = slot.envelope();
            if matches!(run.envelope, EnvelopeRun::Held { .. }) && !envelope.one_shot() {
                let from = run.envelope.level(&envelope, now.clock, now.rate);
                run.envelope = EnvelopeRun::Released {
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
        let from = if note.shape.reset() {
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
        self.new_note = true;
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
        // A note carrying on keeps the drift it has, and rolls its own offsets as it starts.
        if !carries_on && self.voice.drifts() {
            self.voice.seed_drift(self.drift_rng.next_u32());
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

    /// A note's velocity as it starts, its own moved up or down by held `VDLT` and by what
    /// modulation adds to it, kept to the velocity's range. It's read once, so a sounding note
    /// keeps it.
    pub fn velocity(&self, own: u8) -> u8 {
        let moved =
            self.held_velocity_delta + self.offsets[SettingId::VELOCITY_DELTA.index()] * 255.0;
        moved_velocity(own, moved)
    }

    /// A row's nudge as it plays, in ticks, from the nudge written and the `QNT` the row chose
    /// before it played, moved by what modulation adds.
    pub fn quantised(&self, written: i64, chosen: u8) -> i64 {
        let value = f32::from(chosen) + self.offsets[SettingId::QUANTISE.index()] * 255.0;
        quantised_nudge(written, value)
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
        self.sampler_value(SettingId::SAMPLE).round() as u8
    }

    /// The slice of its entry a sampler's note starting now plays, by number: the held `SLI`, or
    /// a lock over it, moved by what modulation adds, in whole slices. The entry clamps it to its
    /// last. An `SLI` by a letter plays the slice the entry's mark names, or, with no such mark,
    /// the track's own: the held `SLI` under the lock, or `own`, the track's setting, if what's
    /// held is a letter too.
    pub fn slice_number(&self, playable: &Playable, own: u8) -> u8 {
        let layers = &self.values[SettingId::SLICE.index()];
        let Some(named) = slice_letter(layers.lock.unwrap_or(layers.base)) else {
            return self.sampler_value(SettingId::SLICE).round() as u8;
        };
        let slice = playable
            .marked(named)
            .unwrap_or_else(|| match slice_letter(layers.base) {
                Some(_) => own,
                None => (layers.base >> 8) as u8,
            });
        let moved = f32::from(slice) + self.offsets[SettingId::SLICE.index()] * 255.0;
        moved.clamp(0.0, 255.0).round() as u8
    }

    /// What the sampler's `STA`, `SLEN` and `PLY` make of the span a note starting now plays:
    /// where in it the note starts and how much of it plays, as shares, and how. Modulation moves
    /// the start and the length, and the play mode is chosen.
    pub fn sample_cut(&self) -> Cut {
        let share = |id| f64::from(self.sampler_value(id)) / 255.0;
        Cut {
            start: share(SettingId::SAMPLE_START),
            length: share(SettingId::SAMPLE_LENGTH),
            mode: PlayMode::from_index(self.held_sampler[SettingId::PLAY.owner().1].round() as u8),
        }
    }

    /// One of the sampler's held settings, with what modulation adds, on the `00` to `FF` scale.
    fn sampler_value(&self, id: SettingId) -> f32 {
        let held = self.held_sampler[id.owner().1];
        (held + self.offsets[id.index()] * 255.0).clamp(0.0, 255.0)
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
        for run in &mut self.slots {
            run.envelope = EnvelopeRun::Idle;
        }
    }

    /// Run a lock or hold value from a step. Anything else on that value gives way to it. A fader
    /// a hand has taken over keeps it underneath, unheard, for when it's given back.
    pub fn set_value(&mut self, id: SettingId, value: u16, mark: Mark) {
        if let Some(fader) = id.fader() {
            let run = &mut self.faders[fader];
            run.automated = true;
            if mark == Mark::Hold {
                run.hold = Some(value);
            }
        }
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
        // A step switching a trigger on strikes it, even where it's on already, so two steps in a
        // row each start a stutter.
        if let (Owner::Slot { slot, effect }, at) = id.owner()
            && scaled(value) >= 0.5
            && self.loaded_effects[slot] == Some(effect)
            && effect
                .params()
                .get(at)
                .is_some_and(|param| param.setting == Setting::Trig)
        {
            self.struck[slot] = true;
        }
        self.hear(id, scaled(value));
    }

    /// Hear a value the steps give, unless it's for a fader a hand has taken over.
    fn hear(&mut self, id: SettingId, value: f32) {
        if !id.fader().is_some_and(|fader| self.faders[fader].taken) {
            self.write_value(id, value);
        }
    }

    /// A hand moved a fader the steps drive: from now its track page's value is heard, and the
    /// steps' values for it are kept underneath, unheard, until it's given back.
    pub fn take_over(&mut self, fader: usize) {
        let Some(run) = self.faders.get_mut(fader) else {
            return;
        };
        run.taken = true;
        let song = run.song;
        self.write_value(FADERS[fader], scaled(song));
    }

    /// Give a fader back to the steps, as Live's Re-Enable Automation does: it's heard where they
    /// have it now, a lock's or a hold's value or, at the next update, a glide's place.
    pub fn give_back(&mut self, fader: usize) {
        let Some(run) = self.faders.get_mut(fader) else {
            return;
        };
        if !std::mem::take(&mut run.taken) {
            return;
        }
        let base = run.hold.unwrap_or(run.song);
        let id = FADERS[fader];
        let layers = &mut self.values[id.index()];
        // Under a lock is the steps' last hold, or the track page's value, which the hand set.
        layers.base = base;
        if layers.glide.is_none() {
            let value = heard(layers);
            self.write_value(id, value);
        }
    }

    /// Give every fader back to the steps.
    pub fn give_back_all(&mut self) {
        for fader in 0..FADER_COUNT {
            self.give_back(fader);
        }
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
                self.hear(id, scaled(base));
            }
        }
    }

    /// The track page changed a value. A lock or glide on top stays until it ends, and a hold
    /// gives way to it. On a fader a hand has taken over it's heard at once, whatever the steps do,
    /// which keep their hold for when it's given back.
    pub fn set_base(&mut self, id: SettingId, value: u16) {
        if let Some(fader) = id.fader() {
            let run = &mut self.faders[fader];
            run.song = value;
            if run.taken {
                self.write_value(id, scaled(value));
                return;
            }
        }
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
                self.hear(id, from + (to - from) * progress.max(0.0) as f32);
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
            (Owner::Mod(slot), at) => {
                self.slots[slot].values[at] = value;
                return;
            }
            (Owner::Track, _) => match id {
                SettingId::PITCH => {
                    self.held_pitch = value;
                    return;
                }
                SettingId::LEVEL => {
                    self.level.target = level_gain(value);
                    self.faders[0].at = value;
                    return;
                }
                SettingId::VOL => self.vol = value,
                SettingId::PAN => {
                    self.pan.target = pan_position(value);
                    self.faders[1].at = value;
                }
                SettingId::CUT => self.cutoff.target = value / 255.0,
                SettingId::RES => self.resonance.target = value / 255.0,
                SettingId::SEND_A => {
                    self.send_a = value;
                    self.faders[2].at = value;
                }
                SettingId::SEND_B => {
                    self.send_b = value;
                    self.faders[3].at = value;
                }
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
                SettingId::VELOCITY_DELTA => {
                    self.held_velocity_delta = value;
                    return;
                }
                SettingId::GLIDE => {
                    self.held_glide = value;
                    return;
                }
                _ => return,
            },
            // Read as a note starts, which keeps what it chose until it ends.
            (Owner::Sampler, at) => {
                self.held_sampler[at] = value;
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
        self.input_trim.settle();
        self.pan.settle();
        self.cutoff.settle();
        self.resonance.settle();
        self.send_a_gain.settle();
        self.send_b_gain.settle();
        self.heard.settle();
        self.feeds.settle();
        self.cut_offset.settle();
        self.res_offset.settle();
        self.pan_offset.settle();
        self.cut_envelope.settle();
        self.res_envelope.settle();
        self.pan_envelope.settle();
        self.inserts.iter_mut().for_each(InsertState::settle);
        self.drive.settle();
        self.punch.settle();
        self.safe.settle();
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

    /// Trim, filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub fn shape(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> ShapedFrame {
        // The trim comes first, as a console's input gain does, so it sets how hard the filter,
        // its drive and the slots are hit. At 0 dB it's exactly 1, which leaves the voice as it is.
        let trim = self.input_trim.next(smoothing);
        // A NaN or an infinity from a voice would stay in the filter, and the utility filter
        // after it, for good, so it's heard as a frame of silence instead, as the slots do (#461).
        // Level, pan and the sends only scale the frame and keep nothing of it.
        let input = [input[0] * trim, input[1] * trim]
            .map(|sample| if sample.is_finite() { sample } else { 0.0 });
        self.take_in_after_trim(input);
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
        let punch = self.punch.next(smoothing);
        let safe = self.safe.next(smoothing);
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
                // So little `Safe` is none, so the filter is as it was without it.
                let resonance = if safe >= 0.001 {
                    safe_resonance(self.filter_type, resonance, cutoff, drive, safe)
                } else {
                    resonance
                };
                let mut c = Coefficients::with_mode(cutoff, resonance, mode, rate);
                // A punch this small is none, so the filter is as it was without it.
                if punch >= 0.001 && self.filter_type.has_punch() {
                    c = c.punched(punch, rate);
                }
                match &mut self.coefficients {
                    Some(ramp) => ramp.toward(&c, FILTER_UPDATE_FRAMES),
                    None => self.coefficients = Some(Ramp::at(c)),
                }
                self.filter_countdown = FILTER_UPDATE_FRAMES;
            }
            self.filter_countdown -= 1;
            // A drive this small takes the clean path rather than a very slight clip.
            let drive = if drive < 0.001 { 0.0 } else { drive };
            match &mut self.coefficients {
                Some(ramp) => {
                    let c = ramp.now();
                    let filtered = [
                        self.filter[0].process(input[0], self.filter_type, drive, c),
                        self.filter[1].process(input[1], self.filter_type, drive, c),
                    ];
                    ramp.advance();
                    filtered
                }
                None => input,
            }
        };
        let filtered = self.trimmed(filtered, smoothing, rate);
        let filtered = self.through_slots(filtered, smoothing, fast, rate, keys);
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

    /// Keep the highest level of each side right after the trim, for the Output device's meter.
    /// It's a comparison a side, and a sample that isn't a number leaves it as it was.
    #[inline]
    fn take_in_after_trim(&mut self, frame: [f32; 2]) {
        for (highest, sample) in self.after_trim.iter_mut().zip(frame) {
            *highest = highest.max(sample.abs());
        }
    }

    /// Run a frame through the slots in order. Whether its two sides are the same is looked at
    /// once, and each slot passes it on to the next, so a chain of effects that keep a mono sound
    /// mono works out one side all the way through (decision 147).
    #[inline]
    fn through_slots(
        &mut self,
        mut frame: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        let mut same = same_sides(frame);
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            // A setting being modulated keeps up, and a change on its own doesn't jump.
            let keeping_up = if self.slot_moving[slot] {
                fast
            } else {
                smoothing
            };
            (frame, same) = self.slot_times.time(slot, !insert.is_empty(), || {
                insert.process_sides(frame, same, keeping_up, rate, keys)
            });
        }
        frame
    }

    /// Run a frame through the trim, slots and level of a track with no voice of its own.
    #[inline]
    fn slots_and_level(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> [f32; 2] {
        let trim = self.input_trim.next(smoothing);
        let frame = [input[0] * trim, input[1] * trim];
        self.take_in_after_trim(frame);
        let frame = self.through_slots(frame, smoothing, fast, rate, keys);
        self.key = frame;
        let vol = if self.offsets[SettingId::VOL.index()] == 0.0 {
            smoothing
        } else {
            fast
        };
        let gain = self.gain.next(vol) * self.level.next(smoothing);
        [frame[0] * gain, frame[1] * gain]
    }

    /// What a return adds to the mix for what's sent to it: its trim, slots, level and pan, then
    /// whether it's heard, faded so muting doesn't click. With nothing going in and nothing left in
    /// its slots, it does no work.
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
            // With nothing to fade, a mute is heard at once when something comes in.
            self.heard.settle();
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

    /// What a group adds to the mix for the sum of its tracks, as a return does, and what it sends
    /// to the returns from there, after its level and pan.
    #[inline]
    pub fn process_group(
        &mut self,
        input: [f32; 2],
        smoothing: f32,
        fast: f32,
        rate: u32,
        keys: &Keys,
    ) -> ShapedFrame {
        let dry = self.process_return(input, smoothing, fast, rate, keys);
        let keeping_up = |target: SettingId| {
            if self.offsets[target.index()] == 0.0 {
                smoothing
            } else {
                fast
            }
        };
        let (to_a, to_b) = (keeping_up(SettingId::SEND_A), keeping_up(SettingId::SEND_B));
        let send_a = self.send_a_gain.next(to_a);
        let send_b = self.send_b_gain.next(to_b);
        ShapedFrame {
            dry,
            send_a: [dry[0] * send_a, dry[1] * send_a],
            send_b: [dry[0] * send_b, dry[1] * send_b],
        }
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

    /// A track's state with its values from a track, as playing from the top leaves them.
    fn from_the_top(track: &Track) -> TrackState {
        let mut state = TrackState::new(0, 0);
        state.reset_held(0, 0, track, 0);
        state
    }

    #[test]
    fn a_lev_lock_sets_the_fader_and_a_note_puts_it_back() {
        let track = Track {
            level: 0xC0,
            ..Track::default()
        };
        let mut state = from_the_top(&track);
        assert_eq!(state.level.target, level_gain(f32::from(0xC0_u8)));
        assert_eq!(state.faders().at[0], 0xC000);
        assert!(!state.faders().automated[0]);
        state.set_value(SettingId::LEVEL, 0xE000, Mark::Lock);
        assert_eq!(state.level.target, 1.0, "E0 is 0 dB");
        assert_eq!(state.faders().at[0], 0xE000);
        assert!(state.faders().follows_steps(0));
        state.end_locks();
        assert_eq!(state.level.target, level_gain(f32::from(0xC0_u8)));
        assert!(
            state.faders().automated[0],
            "still the steps' until playing from the top"
        );
        state.reset_held(0, 0, &track, 0);
        assert!(!state.faders().automated[0]);
    }

    #[test]
    fn a_lev_hold_lasts_through_a_note_until_playing_from_the_top() {
        let mut state = from_the_top(&Track::default());
        state.set_value(SettingId::LEVEL, 0x8000, Mark::Hold);
        state.end_locks();
        assert_eq!(state.faders().at[0], 0x8000, "the hold stays");
        state.reset_held(0, 0, &Track::default(), 0);
        assert_eq!(state.faders().at[0], 0xE000, "back to the set value");
    }

    #[test]
    fn a_takeover_keeps_the_steps_hold_whichever_comes_first_of_it_and_the_hands_edit() {
        for edit_first in [false, true] {
            let mut state = from_the_top(&Track::default());
            state.set_value(SettingId::LEVEL, 0x8000, Mark::Hold);
            if edit_first {
                state.set_base(SettingId::LEVEL, 0xD000);
                state.take_over(0);
            } else {
                state.take_over(0);
                state.set_base(SettingId::LEVEL, 0xD000);
            }
            assert_eq!(state.faders().at[0], 0xD000, "the hand's");
            state.give_back(0);
            assert_eq!(state.faders().at[0], 0x8000, "the steps' hold");
        }
    }

    #[test]
    fn vol_moves_what_is_heard_around_the_fader_and_not_the_fader() {
        let mut state = from_the_top(&Track::default());
        state.set_value(SettingId::LEVEL, 0xC000, Mark::Hold);
        state.set_value(SettingId::VOL, 0xD000, Mark::Hold);
        let faders = state.faders();
        assert_eq!(faders.at[0], 0xC000);
        assert_eq!(faders.moved[0], 0xB0, "16 steps under the fader");
        assert_eq!(state.live().level, 0xB0);
    }

    #[test]
    fn a_hand_takes_a_fader_over_until_it_is_given_back() {
        let mut state = from_the_top(&Track::default());
        state.set_value(SettingId::PAN, 0x2000, Mark::Hold);
        assert_eq!(state.faders().at[1], 0x2000);
        state.take_over(1);
        assert_eq!(state.faders().at[1], 0x8000, "the track page's value");
        assert!(state.faders().taken[1] && !state.faders().follows_steps(1));
        state.set_base(SettingId::PAN, 0x9000);
        assert_eq!(state.faders().at[1], 0x9000, "the hand's move is heard");
        state.set_value(SettingId::PAN, 0x3000, Mark::Lock);
        assert_eq!(state.faders().at[1], 0x9000, "the steps' aren't");
        state.set_value(SettingId::SEND_A, 0x4000, Mark::Lock);
        assert_eq!(
            state.faders().at[2],
            0x4000,
            "the other faders still follow"
        );
        state.end_locks();
        state.give_back(1);
        assert_eq!(state.faders().at[1], 0x2000, "back to the hold underneath");
        assert!(state.faders().follows_steps(1));
    }

    #[test]
    fn a_fader_given_back_after_a_lock_ends_hears_the_hands_value() {
        let mut state = from_the_top(&Track::default());
        state.set_value(SettingId::LEVEL, 0x8000, Mark::Lock);
        state.take_over(0);
        state.set_base(SettingId::LEVEL, 0xD000);
        state.end_locks();
        state.give_back(0);
        assert_eq!(state.faders().at[0], 0xD000);
        state.set_value(SettingId::LEVEL, 0x9000, Mark::Lock);
        assert_eq!(state.faders().at[0], 0x9000, "the steps drive it again");
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

    /// A track's state as it plays it: its values from the top, then its filter, drive, punch
    /// and utility filter, which modulating takes from the track, all settled.
    fn playing(track: &Track, rate: u32) -> TrackState {
        let mut state = from_the_top(track);
        let mut memory = MemoryPool::for_song(&vt_model::song::Song::new(1), rate);
        let now = SongTime {
            tick: 0.0,
            clock: 0,
            rate,
            seed: 0,
            frames_per_tick: 100.0,
            rendered: 0,
            scale: Default::default(),
        };
        state.modulate(track, 0, now, true, &mut memory);
        state.settle();
        state
    }

    #[test]
    fn a_slot_in_scale_moves_the_note_to_the_scales_nearest() {
        use vt_model::scale::{Scale, ScaleKind};
        let major = Scale {
            on: true,
            root: 0,
            kind: ScaleKind::Major,
        };
        let semitones = |added: f32| (added * 255.0).round();
        // From C, three up is D sharp, between D and E, so the lower, D.
        assert_eq!(semitones(in_scale(3.0 / 255.0, Some(60.0), major)), 2.0);
        assert_eq!(semitones(in_scale(4.0 / 255.0, Some(60.0), major)), 4.0);
        assert_eq!(semitones(in_scale(-1.0 / 255.0, Some(60.0), major)), -1.0);
        // From E, one up is F, in the scale.
        assert_eq!(semitones(in_scale(1.0 / 255.0, Some(64.0), major)), 1.0);
        // No whole move leaves a note off the scale as written; and off or with no note it's as
        // it was.
        assert_eq!(in_scale(0.3 / 255.0, Some(61.0), major), 0.0);
        let off = Scale { on: false, ..major };
        assert_eq!(in_scale(3.0 / 255.0, Some(60.0), off), 3.0 / 255.0);
        assert_eq!(in_scale(3.0 / 255.0, None, major), 3.0 / 255.0);
    }

    #[test]
    fn a_broken_sample_from_a_voice_is_a_frame_of_silence_to_the_filter() {
        use crate::dsp::blocks::smoother;
        let rate = 48_000;
        let (smoothing, fast) = (
            smoother::coefficient(rate),
            smoother::fast_coefficient(rate),
        );
        let keys = Keys::default();
        for filter in FilterType::ALL {
            for broken in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                // Resonant and driven, with the utility filter cutting both ends, so every stage
                // with state hears it.
                let track = Track {
                    filter,
                    cutoff: 0x60,
                    resonance: 0xC0,
                    drive: 0x40,
                    low_cut: 0x20,
                    high_cut: 0xE0,
                    ..Track::default()
                };
                let (mut hit, mut silent) = (playing(&track, rate), playing(&track, rate));
                for frame in 0..4_000 {
                    let sound = (frame as f32 * 0.05).sin() * 0.5;
                    let (to_hit, to_silent) = if frame == 1_000 {
                        ([broken, sound], [0.0, sound])
                    } else {
                        ([sound; 2], [sound; 2])
                    };
                    let out = hit.shape(to_hit, smoothing, fast, rate, &keys);
                    let heard = silent.shape(to_silent, smoothing, fast, rate, &keys);
                    // Within the same frame it's as if the voice had gone silent for one.
                    assert_eq!(out.dry, heard.dry, "{filter:?} {broken} at {frame}");
                    assert_eq!(out.send_a, heard.send_a);
                    assert!(out.dry.iter().all(|s| s.is_finite()));
                }
                assert!(
                    hit.after_trim.iter().all(|s| s.is_finite()),
                    "the meter too"
                );
            }
        }
    }

    #[test]
    fn punch_is_heard_only_on_the_clean_filter_types() {
        use crate::dsp::blocks::smoother;
        let rate = 48_000;
        let (smoothing, fast) = (
            smoother::coefficient(rate),
            smoother::fast_coefficient(rate),
        );
        let keys = Keys::default();
        let render = |filter, punch| {
            let track = Track {
                filter,
                cutoff: 0x20,
                resonance: 0xFF,
                punch,
                ..Track::default()
            };
            let mut state = playing(&track, rate);
            (0..4_800)
                .map(|frame| {
                    let strike = if frame < 48 { 1.0 } else { 0.0 };
                    state.shape([strike; 2], smoothing, fast, rate, &keys).dry[0]
                })
                .collect::<Vec<f32>>()
        };
        for filter in FilterType::ALL {
            let (plain, punched) = (render(filter, 0), render(filter, 0xFF));
            assert_eq!(plain != punched, filter.has_punch(), "{filter:?}");
        }
    }

    #[test]
    fn safe_is_heard_only_on_the_types_that_ring_by_themselves() {
        use crate::dsp::blocks::smoother;
        let rate = 48_000;
        let (smoothing, fast) = (
            smoother::coefficient(rate),
            smoother::fast_coefficient(rate),
        );
        let keys = Keys::default();
        let render = |filter, safe| {
            let track = Track {
                filter,
                cutoff: 0x80,
                resonance: 0xFF,
                safe,
                ..Track::default()
            };
            let mut state = playing(&track, rate);
            (0..4_800)
                .map(|frame| {
                    let strike = if frame < 48 { 1.0 } else { 0.0 };
                    state.shape([strike; 2], smoothing, fast, rate, &keys).dry[0]
                })
                .collect::<Vec<f32>>()
        };
        for filter in FilterType::ALL {
            let (plain, safe) = (render(filter, false), render(filter, true));
            assert_eq!(plain != safe, filter.has_safe(), "{filter:?}");
        }
    }
}
