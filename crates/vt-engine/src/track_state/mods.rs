//! The mod slots as they run: their envelopes, LFOs and Steps, worked out as the song moves,
//! started and released with the track's notes, and left resting while they'd change nothing.

use vt_model::attractor::{Attractor, System, view};
use vt_model::envelope::{Envelope, Lpg};
use vt_model::lfo::{LfoMode, MAX_DEFORM, Rate, deformed_wave, phase_of};
use vt_model::macros::MACROS;
use vt_model::modulation::{
    MAX_DEPTH, MOD_SLOTS, ModKind, ModSlot, Polarity, depth_of, note_shape_slot,
};
use vt_model::scale::Scale;
use vt_model::settings::{MOD_OWN_SETTINGS, SettingId};
use vt_model::song::Track;
use vt_model::steps::{MAX_STEP, STEPS, StepRate, Steps, StepsMode, smoothed};

use crate::dsp::filter::Channel;
use crate::dsp::memory::{MemoryPool, SongPlace};
use crate::dsp::slot::{BYPASS_FADE_MS, InsertState};
use crate::rng::Pcg32;

use super::TrackState;
use super::live::LfoNow;
use super::pending::NotePitch;

/// The most settings a track's envelopes can move at once: one for each slot, and `Cut` besides,
/// which a low-pass gate's `Colour` closes whatever its destination.
pub(super) const MOVED: usize = MOD_SLOTS + 1;

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

/// Where an LFO's cycles count from.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct LfoStart {
    pub(super) tick: f64,
    pub(super) clock: u64,
}

impl LfoStart {
    /// Where the song is now.
    fn now(now: SongTime) -> Self {
        Self {
            tick: now.tick,
            clock: now.clock,
        }
    }
}

/// How far a slot's LFO, or its Steps, has gone since its start, at the rate `R` it's had: a
/// running phase, so moving the rate speeds it up or slows it down from where it is rather than
/// jumping, as Vital's and Surge's LFOs do (decision 301). It's counted from its start at the rate
/// it has there, and from where it was at each change of rate on at the new one, always by the
/// song's ticks and the playing clock rather than the blocks, so a render repeats whatever their
/// size and a synced rate stays on the bar. A rate that never moves leaves it counting from its
/// start alone, which is where the song is, exactly as before.
#[derive(Debug, Clone, Copy)]
struct Lap<R> {
    /// Where its rate last changed, or its start.
    from: LfoStart,
    /// How far it had gone there, in cycles or steps.
    gone: f64,
    /// The rate it's gone at since, none from a start until it's next worked out, so a start
    /// counts at the rate it has then.
    rate: Option<R>,
}

impl<R> Default for Lap<R> {
    fn default() -> Self {
        Self {
            from: LfoStart::default(),
            gone: 0.0,
            rate: None,
        }
    }
}

impl<R> Lap<R> {
    /// Count from a start again, at the rate it has when it's next worked out.
    fn start(&mut self, start: LfoStart) {
        *self = Self {
            from: start,
            ..Self::default()
        };
    }
}

impl Lap<Rate> {
    /// How many cycles an LFO at `rate` since its last change has gone by `until`.
    #[inline]
    fn cycles(&self, rate: Rate, until: LfoStart, sample_rate: u32) -> f64 {
        let seconds = until.clock.wrapping_sub(self.from.clock) as f64 / f64::from(sample_rate);
        self.gone + rate.cycles(until.tick - self.from.tick, seconds)
    }

    /// Go on at `rate` from now, counting what it went at the last up to here.
    #[inline]
    fn keep(&mut self, rate: Rate, now: SongTime) {
        if let Some(was) = self.rate.filter(|was| *was != rate) {
            let here = LfoStart::now(now);
            self.gone = self.cycles(was, here, now.rate);
            self.from = here;
        }
        self.rate = Some(rate);
    }
}

impl Lap<StepRate> {
    /// How many steps it's gone at `ticks` a step since its last change, by `now`.
    #[inline]
    fn steps(&self, ticks: f64, now: SongTime) -> f64 {
        (self.gone + (now.tick - self.from.tick) / ticks).max(0.0)
    }

    /// Go on at `rate` from now, counting what it went at the last up to here. From `Pass`,
    /// which isn't a speed, it goes on from where it last was in time.
    #[inline]
    fn keep(&mut self, rate: StepRate, now: SongTime, step_ticks: u64) {
        let was = self.rate.filter(|was| *was != rate);
        if let Some(ticks) = was.and_then(|was| was.ticks(step_ticks)) {
            self.gone = self.steps(ticks, now);
            self.from = LfoStart::now(now);
        }
        self.rate = Some(rate);
    }
}

/// Where one of a track's envelopes has got to.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) enum EnvelopeRun {
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
pub(super) struct SlotRun {
    /// The type it last ran as, to notice a change: none until it first runs.
    seen: Option<ModKind>,
    /// Where its envelope has got to, which each note starts and ends.
    pub(super) envelope: EnvelopeRun,
    /// Where its cycles count from: a note's start, or the top of the song.
    start: LfoStart,
    /// How far its LFO and its Steps have gone since then, each at its own rate.
    lfo_lap: Lap<Rate>,
    steps_lap: Lap<StepRate>,
    /// Where its LFO was at the track's last note, which `Hold` holds.
    held: f64,
    /// Its LFO's attractor while its shape is one (#592), which can't be worked out from where the
    /// song is as a wave can, so it's stepped on in the LFO's cycles: none until it's first heard
    /// after a start, so it's seeded there.
    chaos: Option<Attractor>,
    /// Its held depth, phase, polarity and deform, on the `00` to `FF` scale, as steps and the
    /// page set them. The polarity is below zero until it's first set, which is the slot's own.
    pub(super) values: [f32; 4],
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
    pub(super) now: Option<LfoNow>,
    /// The track's notes since playing started, which a Steps slot in `Note` mode moves on by,
    /// and in `Trig` waits for.
    pub(super) notes: i64,
    /// The pass of the track's chain its last note was in, which a Steps slot at `Pass` in `Trig`
    /// counts from.
    pass_at: i64,
    /// The step it played when the modulation was last worked out, if it ran as Steps.
    pub(super) playing: Option<u8>,
    /// The values Steps' `Mutate` has drawn again in place of the written ones, by step, and which
    /// of them are drawn, a bit a step: kept while it plays, so a loop drifts rather than
    /// starting afresh each time round, and forgotten when `Mutate` goes back to none.
    pub(super) drift: [i16; STEPS],
    pub(super) drawn: u16,
    /// Where it was counted to when `Mutate` last rolled, so each step coming round rolls once.
    pub(super) counted: Option<i64>,
    /// What `Mutate` rolls, a stream of the slot's own from the project's seed, after every
    /// track's others, so a render repeats and `CHA`'s rolls are as they were.
    pub(super) dice: Pcg32,
    /// What a Track slot following `Random` rolled for the last note, and the stream of the slot's
    /// own it rolls from, after every slot's `Mutate`. It rolls at every note, whatever the slot's
    /// type, so one that becomes Track has a roll for the note sounding.
    pub(super) roll: u8,
    pub(super) rolls: Pcg32,
}

impl Default for SlotRun {
    fn default() -> Self {
        Self {
            seen: None,
            envelope: EnvelopeRun::Idle,
            start: LfoStart::default(),
            lfo_lap: Lap::default(),
            steps_lap: Lap::default(),
            held: 0.0,
            chaos: None,
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
    /// Count its cycles and steps from a start again, a note's or the top of the song's, from
    /// where the song is at the rates they have then: so playing from anywhere, or a note that
    /// restarts it, finds it where it would be had its rate never moved.
    pub(super) fn restart(&mut self, start: LfoStart) {
        self.start = start;
        self.lfo_lap.start(start);
        self.steps_lap.start(start);
        self.chaos = None;
    }

    /// Where its LFO's attractor is `cycles` into its count, each axis from -1 to 1, stepped on
    /// from where it was. It's seeded there, from `key`, when it has none since its start, was
    /// another attractor, or is behind or too far behind to step on to it, as after a long rest
    /// or between notes far apart in `Hold`: always at a time the song fixes, so a render repeats.
    fn attractor_at(&mut self, system: System, cycles: f64, key: u64, deform: f32) -> [f32; 3] {
        let reached = self
            .chaos
            .as_mut()
            .is_some_and(|chaos| chaos.system == system && chaos.reach(cycles, deform));
        if !reached {
            self.chaos = Some(Attractor::seeded(system, key, deform, cycles));
        }
        self.chaos
            .as_ref()
            .map_or([0.0; 3], |chaos| chaos.point(cycles, deform))
    }

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
    // Where its LFO is at the note, for `Hold`, at the rate it's going at.
    let rate = run.lfo_lap.rate.unwrap_or(slot.lfo.rate);
    run.held = run.lfo_lap.cycles(rate, LfoStart::now(now), now.rate);
    let restarts = match slot.kind {
        ModKind::Steps => slot.steps.mode == StepsMode::Trig,
        _ => slot.lfo.mode.restarts(),
    };
    if restarts {
        run.restart(LfoStart::now(now));
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
                let along = run.steps_lap.steps(ticks, now);
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
pub(super) fn in_scale(added: f32, note: Option<f32>, scale: Scale) -> f32 {
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
/// its start, which a note that restarts it moves, at the rates it's had since, and for `Hold`
/// only as far as the track's last note, or its start if none has played. A one-shot stays at the
/// end of what it plays, just short of it so a saw is still at its bottom, and before any note
/// it's already there.
#[inline]
fn lfo_cycles(slot: &ModSlot, run: &SlotRun, noted: Option<LfoStart>, now: SongTime) -> f64 {
    let mode = slot.lfo.mode;
    let cycles = match (mode, noted) {
        (LfoMode::Hold, Some(_)) => run.held,
        (LfoMode::Hold, None) => 0.0,
        _ => run
            .lfo_lap
            .cycles(slot.lfo.rate, LfoStart::now(now), now.rate),
    };
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

impl SongTime {
    /// Where the song is, for the slots' effects that keep time with it.
    pub fn place(&self, stopped: bool) -> SongPlace {
        SongPlace {
            tick: self.tick,
            playing: !stopped,
            frames_per_tick: self.frames_per_tick,
        }
    }
}

/// What a track's modulation was worked out with when it left nothing moving: whether the song
/// was stopped, and its tempo, which some slots' effects keep time with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Rest {
    stopped: bool,
    frames_per_tick: f64,
}

/// Whether a track's mod slots' own settings may be heard other than as the song has them: a
/// macro holds something, or a slot goes to one of them. Otherwise the slots are read as they
/// are, with no copy.
fn reshapes(track: &Track) -> bool {
    track.has_macro_maps()
        || track
            .mods
            .iter()
            .any(|slot| slot.destination.is_some_and(SettingId::is_mod_own))
}

/// A mod slot as it's heard, where its own settings may be other than the song's, `reshaped`
/// ([`reshapes`]): a copy with each of its type's own settings put in where it isn't the song's,
/// from its held value, `held`, and what modulation adds, `moved` of a setting's place, so the
/// slot's own code reads it as it reads the song's and allocates nothing. None where they can't
/// be, as the song's slot is heard as it is.
fn heard_slot(
    slot: &ModSlot,
    at: usize,
    reshaped: bool,
    held: &[f32; MOD_OWN_SETTINGS],
    moved: impl Fn(usize) -> f32,
) -> Option<ModSlot> {
    if !reshaped {
        return None;
    }
    let mut heard = *slot;
    for (own, held) in held.iter().enumerate() {
        let (Some(written), Some(id)) = (slot.own_value(own), SettingId::mod_own(at, own)) else {
            continue;
        };
        let by = moved(id.index());
        if by != 0.0 || (held * 256.0).round() as u16 != written {
            heard.set_own(own, held + by * 255.0);
        }
    }
    Some(heard)
}

impl TrackState {
    /// A mod slot as it's heard: the song's, with its own settings as macros and modulation have
    /// them, for what reads a slot outside its working out, a note's start and end and its shape.
    pub fn heard_mod(&self, track: &Track, at: usize) -> ModSlot {
        let slot = &track.mods[at];
        heard_slot(slot, at, reshapes(track), &self.own_held[at], |index| {
            self.offsets[index]
        })
        .unwrap_or(*slot)
    }

    /// Count every slot's cycles from the top of the song, as playing starts there, keeping the
    /// clock they count real time from.
    pub fn count_cycles_from_the_top(&mut self) {
        self.wake();
        for run in &mut self.slots {
            run.restart(LfoStart {
                tick: 0.0,
                clock: run.start.clock,
            });
            run.notes = 0;
            run.counted = None;
        }
        self.noted = None;
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
        // Whether any slot moves anything, or might.
        let mut working = false;
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
        // Whether any slot's own settings may be moved from the song's, so each is read through
        // a copy with them put in, from what the slots before it have added this time.
        let reshaped = reshapes(track);
        let own_held = &self.own_held;
        for (at, (slot, run)) in track.mods.iter().zip(&mut self.slots).enumerate() {
            let heard = heard_slot(slot, at, reshaped, &own_held[at], |index| {
                offsets[index] + moved.at(index)
            });
            let slot = heard.as_ref().unwrap_or(slot);
            run.now = None;
            run.playing = None;
            // Each rate as it's heard now goes on from where its last left off, whatever the
            // slot's type, as every type's state is kept going underneath.
            run.lfo_lap.keep(slot.lfo.rate, now);
            run.steps_lap.keep(slot.steps.rate, now, step_ticks);
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
                working = true;
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
            working = true;
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
                    let cycles = lfo_cycles(slot, run, noted, now);
                    let key = now.seed ^ ((index as u64) << 8 | wave_key(at));
                    let phase = f64::from(phase_of(phase) + moved_by(1));
                    let deform = ((depth_of(deform) + moved_by(3) * 255.0) / f32::from(MAX_DEFORM))
                        .clamp(-1.0, 1.0);
                    // An attractor's phase is the way it's looked at, which axis is heard, as it has
                    // no cycle to start part way through; the Mod page draws it from the next
                    // axis round, so its dot goes across by that one.
                    let value = if let Some(system) = System::of(slot.lfo.shape) {
                        let point = run.attractor_at(system, cycles, key, deform);
                        let value = view(point, phase);
                        let across = view(point, phase + 1.0 / 3.0);
                        run.now = Some(LfoNow::across(across, value));
                        value
                    } else {
                        let value = deformed_wave(slot.lfo.shape, cycles + phase, key, deform);
                        run.now = Some(LfoNow::of(cycles + phase, value));
                        value
                    };
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
        // The note just started is heard from here, so what each slot moves its pitch by is
        // reported, every path above having left what it added in `out`.
        if let Some(row) = self.pitch_row.take() {
            let on_pitch = |at: usize| track.mods[at].destination == Some(SettingId::PITCH);
            self.pitched = Some(NotePitch {
                row,
                varied: self.pitch_varied,
                moves: std::array::from_fn(|at| {
                    if on_pitch(at) {
                        self.slots[at].out * 255.0
                    } else {
                        0.0
                    }
                }),
            });
        }
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
        // Each macro puts what it holds where it has it now, before the voice and the slots' effects
        // are given their settings, so they hear it this time; a mod slot's, the next.
        // With none mapped, that comes to forgetting where each was heard (#187).
        if track.has_macro_maps() {
            let moves = std::array::from_fn(|at| {
                SettingId::macro_at(at).map_or(0.0, |id| offsets[id.index()])
            });
            self.put_macros(track, &moves);
        } else {
            self.macro_heard = [None; MACROS];
        }
        let offsets = &mut self.offsets;
        // Each slot's effect gets finished values, and doesn't know where they came from.
        for (slot, insert) in track.inserts.into_iter().enumerate() {
            let mut resolved = insert;
            self.slot_moving[slot] = false;
            let effect = insert.kind();
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
                let added = offsets[id.index()] * found.span();
                self.slot_moving[slot] |= added != 0.0;
                let value = self.slot_values[slot][at] + added;
                resolved = resolved.write(setting, value);
            }
            self.resolved[slot] = resolved;
            self.inserts[slot].hold(resolved, memory, now.place(stopped), now.rendered);
            if std::mem::take(&mut self.struck[slot]) {
                self.inserts[slot].strike();
            }
            self.inserts[slot].bypass(self.bypassed[slot]);
        }
        let semitones = self.held_pitch - 128.0 + offsets[SettingId::PITCH.index()] * 255.0;
        if self.pitch_from != Some(semitones.to_bits()) {
            self.pitch_from = Some(semitones.to_bits());
            self.pitch = 2_f64.powf(f64::from(semitones.clamp(-96.0, 96.0)) / 12.0);
            self.pitch_octaves = self.pitch.log2() as f32;
        }
        self.voice.set_pitch(self.pitch);
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
        self.tell_slots_the_note();
        // Working it out again would change nothing while no slot moves anything, no glide runs,
        // each mod slot's share is where its switch is, and each slot's effect has what it keeps.
        let still = !working
            && (stopped || self.gliding == 0)
            && self.slots.iter().zip(&track.mods).all(|(run, slot)| {
                run.glide_left == 0.0 && run.share == if slot.bypassed { 0.0 } else { 1.0 }
            })
            && self.inserts.iter().all(InsertState::holding);
        self.rest = still.then_some(Rest {
            stopped,
            frames_per_tick: now.frames_per_tick,
        });
    }

    /// Whether working its modulation out now would change nothing, as nothing it reads has
    /// changed since it was last worked out and that left nothing moving.
    pub fn rests(&self, now: SongTime, stopped: bool) -> bool {
        self.rest
            == Some(Rest {
                stopped,
                frames_per_tick: now.frames_per_tick,
            })
    }

    /// Whether its modulation, as last worked out, left nothing moving.
    pub fn at_rest(&self) -> bool {
        self.rest.is_some()
    }

    /// Something its modulation reads has changed, so it's worked out again next time.
    #[inline]
    pub fn wake(&mut self) {
        self.rest = None;
    }

    /// What working its modulation out again would still do while it rests: the shares' clock
    /// moves on, and so do a reverb's or a plate's wobbles, which go by the frame.
    pub fn keep_resting(&mut self, now: SongTime) {
        self.shared_at = now.rendered;
        let stopped = self.rest.is_none_or(|rest| rest.stopped);
        for insert in &mut self.inserts {
            insert.keep_time(now.rendered, now.place(stopped));
        }
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
    pub(super) fn settle_shares(&mut self, track: &Track) {
        for (run, slot) in self.slots.iter_mut().zip(&track.mods) {
            run.share = if slot.bypassed { 0.0 } else { 1.0 };
            run.seen = Some(slot.kind);
            run.glide_left = 0.0;
        }
    }

    /// Start a note's modulation: every slot's envelope, and the cycles of each that a note
    /// starts, whatever type each is now, so one that changes type picks the new one up as though
    /// it had always been it.
    pub fn trigger_mods(&mut self, track: &Track, now: SongTime) {
        self.wake();
        let reshaped = reshapes(track);
        let (own_held, offsets) = (&self.own_held, &self.offsets);
        for (at, (slot, run)) in track.mods.iter().zip(&mut self.slots).enumerate() {
            let slot = heard_slot(slot, at, reshaped, &own_held[at], |index| offsets[index])
                .unwrap_or(*slot);
            trigger_slot(&slot, run, now, self.chain_pass);
        }
        self.noted = Some(LfoStart {
            tick: now.tick,
            clock: now.clock,
        });
    }

    /// Start the release of every slot's envelope, when a note ends, but a one-shot's, which plays
    /// through whatever the note's length.
    pub fn release_mods(&mut self, track: &Track, now: SongTime) {
        self.wake();
        let reshaped = reshapes(track);
        let (own_held, offsets) = (&self.own_held, &self.offsets);
        for (at, (slot, run)) in track.mods.iter().zip(&mut self.slots).enumerate() {
            let slot = heard_slot(slot, at, reshaped, &own_held[at], |index| offsets[index])
                .unwrap_or(*slot);
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
}
