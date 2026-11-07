//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::constants::{FX_SLOTS, TRACK_ENVELOPES, UNITY_LEVEL};
use vt_model::envelope::Envelope;
use vt_model::insert::{INSERTS, Insert, MAX_PARAMS};
use vt_model::lfo::{LFOS_PER_TRACK, MAX_LFO_DEPTH, wave};
use vt_model::song::{FilterType, Track, level_gain};
use vt_model::step::{FxCommand, FxSlot, Mark};
use vt_model::target::Target;

use crate::dsp::distort::InsertState;
use crate::dsp::filter::{Channel, Coefficients, Trim};
use crate::dsp::memory::MemoryPool;
use crate::dsp::smoother::Smoothed;
use crate::dsp::virtual_analog::Settings;
use crate::mixer::Reading;
use crate::rng::Pcg32;
use crate::voice::{NoteOn, Voice};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LiveValues {
    pub level: u8,
    pub pan: u8,
    pub cutoff: u8,
    pub resonance: u8,
    pub send_a: u8,
    pub send_b: u8,
    pub harmonics: u8,
    pub timbre: u8,
    pub morph: u8,
    /// What each slot's settings are playing with, in the effect's order.
    pub inserts: [[u8; MAX_PARAMS]; INSERTS],
}

/// The commands whose values last, in the order `TrackState::values` keeps them.
const VALUE_COMMANDS: [FxCommand; 9 + INSERTS * MAX_PARAMS] = {
    let slots = FxCommand::ALL;
    let mut commands = [FxCommand::Vol; 9 + INSERTS * MAX_PARAMS];
    let first = [
        FxCommand::Vol,
        FxCommand::Pan,
        FxCommand::Cut,
        FxCommand::Res,
        FxCommand::SendA,
        FxCommand::SendB,
        FxCommand::Har,
        FxCommand::Tim,
        FxCommand::Mor,
    ];
    let mut at = 0;
    while at < first.len() {
        commands[at] = first[at];
        at += 1;
    }
    // The commands for slots' settings come last in the full list, and they all hold.
    let mut slot = 0;
    while slot < INSERTS * MAX_PARAMS {
        commands[first.len() + slot] = slots[slots.len() - INSERTS * MAX_PARAMS + slot];
        slot += 1;
    }
    commands
};

/// A value moving in a straight line, by song position, so it keeps up with tempo changes.
#[derive(Debug, Clone, Copy)]
struct Glide {
    from: u8,
    to: FxSlot,
    start_tick: f64,
    ticks: f64,
}

/// What can set one value. What's heard is the glide if there is one, then the lock, then the base.
#[derive(Debug, Clone, Copy)]
struct ValueLayers {
    base: u8,
    lock: Option<u8>,
    glide: Option<Glide>,
}

impl ValueLayers {
    fn at(base: u8) -> Self {
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

/// Retriggering the current note.
#[derive(Debug, Clone, Copy)]
pub struct Retrigger {
    pub next_at: u64,
    pub interval: u64,
    /// Multiplies the level on each retrigger.
    pub factor: f32,
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
    /// The running count its chain began at. A cursor partway through a chain can put it below 0.
    pub started: i64,
    /// Whether the row playing passed its `CHA`, which is rolled once for all its pulses.
    pub row_plays: bool,
    /// Whether the note sounding is a `Hold`, to be tied into the next row's note.
    pub tied: bool,
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
    pub retrigger: Option<Retrigger>,
    /// A step waiting for its swing delay.
    pub swung: Option<(u64, usize)>,
    /// What each value is set to, in `VALUE_COMMANDS`' order. The fields below it are what's heard.
    values: [ValueLayers; VALUE_COMMANDS.len()],
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
    /// What the LFOs add to each held value, in `Target::ALL`'s order. Separate from the held
    /// values, so a step's FX moves the centre and the LFO carries on around it.
    offsets: [f32; Target::COUNT],
    cut_offset: Smoothed,
    res_offset: Smoothed,
    pan_offset: Smoothed,
    /// What envelopes add, which moves fast and needs to keep its snap.
    cut_envelope: Smoothed,
    res_envelope: Smoothed,
    pan_envelope: Smoothed,
    envelopes: [EnvelopeRun; TRACK_ENVELOPES],
    /// What modulation multiplies a note's speed by.
    pitch: f64,
    /// Held `HAR`, `TIM` and `MOR`, on `0..=1`.
    synth: [f32; 3],
    /// Whether an LFO moves the filter, so it stays running at the top of its range.
    filter_moving: bool,
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

/// Where `TrackState::values` keeps a command's value. Other commands have none, and share the
/// first place, as nothing that calls this passes them.
/// Where a slot's setting starts, from what's on the track. Nothing if the slot's empty.
pub fn slot_base(command: FxCommand, track: &Track) -> u8 {
    let FxCommand::Slot { slot, param } = command else {
        return 0;
    };
    let insert = track.inserts[usize::from(slot)];
    insert
        .params()
        .get(usize::from(param))
        .and_then(|found| insert.read(found.setting))
        .unwrap_or(0)
}

fn value_index(command: FxCommand) -> usize {
    VALUE_COMMANDS
        .iter()
        .position(|value| *value == command)
        .unwrap_or(0)
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
            row_plays: true,
            tied: false,
            voice: Voice::default(),
            tail: Voice::default(),
            reading: Reading::default(),
            mute: false,
            solo: false,
            heard: Smoothed::at(1.0),
            rng: Pcg32::new(seed, index as u64),
            last_note: None,
            delayed: None,
            kill_at: None,
            off_at: None,
            retrigger: None,
            values: {
                let first = [UNITY_LEVEL, 0x80, 0xFF, 0, 0, 0, 0x80, 0, 0x80];
                std::array::from_fn(|at| ValueLayers::at(first.get(at).copied().unwrap_or(0)))
            },
            swung: None,
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
            drive: Smoothed::at(0.0),
            trim: [Trim::default(); 2],
            low_cut: Smoothed::at(0.0),
            inserts: std::array::from_fn(|_| InsertState::default()),
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
            offsets: [0.0; Target::COUNT],
            synth: [0.5, 0.0, 0.5],
            cut_offset: Smoothed::at(0.0),
            res_offset: Smoothed::at(0.0),
            pan_offset: Smoothed::at(0.0),
            cut_envelope: Smoothed::at(0.0),
            res_envelope: Smoothed::at(0.0),
            pan_envelope: Smoothed::at(0.0),
            envelopes: [EnvelopeRun::Idle; TRACK_ENVELOPES],
            pitch: 1.0,
            filter_moving: false,
        }
    }

    /// Start counting the track's chain from this running count, with its lanes in line again.
    pub fn restart(&mut self, started: i64) {
        self.started = started;
        self.lane_offsets = [0; FX_SLOTS];
        self.tied = false;
    }

    /// Carry the sounding note on as this one, with no new attack.
    pub fn tie(&mut self, note: NoteOn) {
        self.last_note = Some(note);
        self.voice.tie(note.note, note.gain);
    }

    /// Reset held values, the random generator and LFOs, when playback goes back to the top. Values
    /// glide back, so anything still sounding doesn't click.
    pub fn reset_held(&mut self, seed: u64, index: usize, track: &Track, clock: u64) {
        self.rng = Pcg32::new(seed, index as u64);
        self.last_note = None;
        let bases = [
            UNITY_LEVEL,
            track.pan,
            track.cutoff,
            track.resonance,
            track.send_a,
            track.send_b,
            track.harmonics,
            track.timbre,
            track.morph,
        ];
        for (command, base) in VALUE_COMMANDS.into_iter().zip(bases) {
            self.values[value_index(command)] = ValueLayers::at(base);
            self.write_value(command, f32::from(base));
        }
        for command in VALUE_COMMANDS.into_iter().skip(bases.len()) {
            let base = slot_base(command, track);
            self.values[value_index(command)] = ValueLayers::at(base);
            self.write_value(command, f32::from(base));
        }
        self.lfo_starts = [LfoStart { tick: 0.0, clock }; LFOS_PER_TRACK];
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
        if !stopped {
            self.advance_glides(now);
        }
        self.filter_moving = false;
        // Envelopes, then each LFO, so each reads what's before it, and there's no way back round.
        let mut moved = [0.0_f32; Target::COUNT];
        for (slot, (envelope, run)) in track.envelopes.iter().zip(self.envelopes).enumerate() {
            let Some(destination) = envelope.destination else {
                continue;
            };
            // The first one on `Vol` is each note's own shape, which the voice follows itself.
            if envelope.depth == 0
                || (slot == 0 && destination == Target::Vol)
                || !destination.exists_on(track)
            {
                continue;
            }
            self.filter_moving |= matches!(destination, Target::Cut | Target::Res);
            if stopped {
                continue;
            }
            let level = run.level(envelope, now.clock, now.rate);
            moved[destination.index()] += level * f32::from(envelope.depth) / 255.0;
        }
        let mut offsets = [0.0_f32; Target::COUNT];
        for (slot, (lfo, start)) in track.lfos.iter().zip(&self.lfo_starts).enumerate() {
            let Some(destination) = lfo.destination else {
                continue;
            };
            let moved_by = |target: Target| offsets[target.index()] + moved[target.index()];
            let at = slot as u8;
            let depth = f32::from(lfo.depth) + moved_by(Target::LfoDepth { lfo: at }) * 255.0;
            // A target that isn't on this track does no work.
            if depth == 0.0 || !destination.exists_on(track) {
                continue;
            }
            self.filter_moving |= matches!(destination, Target::Cut | Target::Res);
            if stopped {
                continue;
            }
            let seconds = now.clock.wrapping_sub(start.clock) as f64 / f64::from(now.rate);
            let cycles = lfo.rate.cycles(now.tick - start.tick, seconds);
            let key = now.seed ^ ((index as u64) << 8 | slot as u64);
            let phase =
                f64::from(lfo.phase) / 360.0 + f64::from(moved_by(Target::LfoPhase { lfo: at }));
            let value = wave(lfo.shape, cycles + phase, key);
            let depth = depth.clamp(-f32::from(MAX_LFO_DEPTH), f32::from(MAX_LFO_DEPTH));
            offsets[destination.index()] += value * depth / 255.0;
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
        self.cut_offset.target = offsets[Target::Cut.index()];
        self.res_offset.target = offsets[Target::Res.index()];
        self.pan_offset.target = offsets[Target::Pan.index()];
        self.cut_envelope.target = moved[Target::Cut.index()];
        self.res_envelope.target = moved[Target::Res.index()];
        self.pan_envelope.target = moved[Target::Pan.index()];
        for (offset, envelope) in offsets.iter_mut().zip(moved) {
            *offset += envelope;
        }
        // Each slot's effect gets finished values, and doesn't know where they came from.
        for (slot, insert) in track.inserts.into_iter().enumerate() {
            let mut resolved = insert;
            self.slot_moving[slot] = false;
            for (param, found) in insert.params().iter().enumerate() {
                let target = Target::Slot {
                    slot: slot as u8,
                    param: param as u8,
                };
                let setting = found.setting;
                let added = offsets[target.index()] * setting.span();
                self.slot_moving[slot] |= added != 0.0;
                let value = self.slot_values[slot][param] + added;
                resolved = resolved.write(setting, value);
            }
            self.resolved[slot] = resolved;
            self.inserts[slot].hold(resolved, memory, now.frames_per_tick, now.rendered);
        }
        self.offsets = offsets;
        let semitones = offsets[Target::Pitch.index()] * 255.0;
        self.pitch = 2_f64.powf(f64::from(semitones.clamp(-96.0, 96.0)) / 12.0);
        self.voice.set_pitch(self.pitch);
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
    }

    /// What this track is playing with right now. `VOL` and its modulation are shown on the track's
    /// `level`, as they both set how loud it is. The first envelope on `Vol` shapes notes, and isn't
    /// a movement of the level.
    pub fn live(&self, level: u8) -> LiveValues {
        let offset = |destination: Target| self.offsets[destination.index()];
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
        let vol = self.vol + offset(Target::Vol) * 255.0 - f32::from(UNITY_LEVEL);
        LiveValues {
            level: (f32::from(level) + vol).clamp(0.0, 255.0).round() as u8,
            pan: moved(self.pan.target, Target::Pan),
            cutoff: moved(self.cutoff.target, Target::Cut),
            resonance: moved(self.resonance.target, Target::Res),
            send_a: on_level(self.send_a, Target::SendA),
            send_b: on_level(self.send_b, Target::SendB),
            harmonics: shown(synth.harmonics),
            timbre: shown(synth.timbre),
            morph: shown(synth.morph),
            inserts: self.resolved.map(|insert| {
                let mut values = [0; MAX_PARAMS];
                for (param, value) in values.iter_mut().enumerate() {
                    *value = insert.value(param);
                }
                values
            }),
        }
    }

    /// What a synth on this track plays with: the held values and what modulation adds.
    pub fn synth_settings(&self) -> Settings {
        let at = |held: f32, destination: Target| {
            (held + self.offsets[destination.index()]).clamp(0.0, 1.0)
        };
        Settings {
            harmonics: at(self.synth[0], Target::Har),
            timbre: at(self.synth[1], Target::Tim),
            morph: at(self.synth[2], Target::Mor),
        }
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
        let gain = |held: f32, destination: Target| {
            level_gain((held + self.offsets[destination.index()] * 255.0).clamp(0.0, 255.0))
        };
        let (vol, send_a, send_b) = (
            gain(self.vol, Target::Vol),
            gain(self.send_a, Target::SendA),
            gain(self.send_b, Target::SendB),
        );
        self.gain.target = vol;
        self.send_a_gain.target = send_a;
        self.send_b_gain.target = send_b;
    }

    /// Start a new note, cancelling anything pending for the old one.
    pub fn start(&mut self, note: NoteOn, rate: u32) {
        let from = if note.shape.reset {
            0.0
        } else {
            self.voice.level()
        };
        self.cut_to_tail();
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.last_note = Some(note);
        self.voice = Voice::start(note, rate, from, self.pitch);
        self.voice.set_synth(self.synth_settings());
        if self.voice.uses_noise() {
            self.voice.seed_noise(self.rng.next_u32());
        }
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
        self.retrigger = None;
        self.swung = None;
        self.tied = false;
        self.envelopes = [EnvelopeRun::Idle; TRACK_ENVELOPES];
    }

    /// Run a lock or hold value from a step. Anything else on that value gives way to it.
    pub fn set_value(&mut self, command: FxCommand, value: u8, mark: Mark) {
        let layers = &mut self.values[value_index(command)];
        layers.glide = None;
        if mark == Mark::Hold {
            layers.base = value;
            layers.lock = None;
        } else {
            layers.lock = Some(value);
        }
        self.write_value(command, f32::from(value));
    }

    /// Run a glide value from a step, toward the next value of its parameter if there is one.
    /// `to` is that value, with how many ticks away it is.
    pub fn start_glide(
        &mut self,
        command: FxCommand,
        value: u8,
        to: Option<(FxSlot, f64)>,
        now: SongTime,
    ) {
        self.set_value(command, value, Mark::Lock);
        if let Some((to, ticks)) = to {
            let layers = &mut self.values[value_index(command)];
            layers.lock = None;
            layers.glide = Some(Glide {
                from: value,
                to,
                start_tick: now.tick,
                ticks,
            });
        }
    }

    /// A note has started, so the locks it would have outlived are over. Glides carry on.
    pub fn end_locks(&mut self) {
        for command in VALUE_COMMANDS {
            let layers = &mut self.values[value_index(command)];
            if layers.lock.take().is_some() && layers.glide.is_none() {
                let base = layers.base;
                self.write_value(command, f32::from(base));
            }
        }
    }

    /// The track page changed a value. A lock or glide on top stays until it ends.
    pub fn set_base(&mut self, command: FxCommand, value: u8) {
        let layers = &mut self.values[value_index(command)];
        layers.base = value;
        if layers.lock.is_none() && layers.glide.is_none() {
            self.write_value(command, f32::from(value));
        }
    }

    /// Move glides along to where the song is.
    fn advance_glides(&mut self, now: SongTime) {
        for command in VALUE_COMMANDS {
            let index = value_index(command);
            let Some(glide) = self.values[index].glide else {
                continue;
            };
            let progress = (now.tick - glide.start_tick) / glide.ticks;
            if progress < 1.0 {
                let (from, to) = (f32::from(glide.from), f32::from(glide.to.value));
                self.write_value(command, from + (to - from) * progress.max(0.0) as f32);
            } else {
                // Its row hasn't run, as a chain moved on, so this takes that row's place.
                let mark = match glide.to.mark {
                    Mark::Hold => Mark::Hold,
                    Mark::Lock | Mark::Glide => Mark::Lock,
                };
                self.set_value(command, glide.to.value, mark);
            }
        }
    }

    /// Set what's heard for a value, on the `00` to `FF` scale.
    fn write_value(&mut self, command: FxCommand, value: f32) {
        match command {
            FxCommand::Vol => self.vol = value,
            FxCommand::Pan => self.pan.target = pan_position(value),
            FxCommand::Cut => self.cutoff.target = value / 255.0,
            FxCommand::Res => self.resonance.target = value / 255.0,
            FxCommand::SendA => self.send_a = value,
            FxCommand::SendB => self.send_b = value,
            FxCommand::Har => self.synth[0] = value / 255.0,
            FxCommand::Tim => self.synth[1] = value / 255.0,
            FxCommand::Mor => self.synth[2] = value / 255.0,
            FxCommand::Slot { slot, param } => {
                self.slot_values[usize::from(slot)][usize::from(param)] = value;
                return;
            }
            _ => {}
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

    /// Filter, level and pan one frame of voice output, and work out what it sends.
    #[inline]
    pub fn shape(&mut self, input: [f32; 2], smoothing: f32, fast: f32, rate: u32) -> ShapedFrame {
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
                self.coefficients = Some(Coefficients::new(cutoff, resonance, rate));
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
            filtered = insert.process(filtered, keeping_up, rate);
        }
        // What's moved by modulation keeps up, and a change on its own doesn't jump.
        let keeping_up = |target: Target| {
            if self.offsets[target.index()] == 0.0 {
                smoothing
            } else {
                fast
            }
        };
        let (vol, to_a, to_b) = (
            keeping_up(Target::Vol),
            keeping_up(Target::SendA),
            keeping_up(Target::SendB),
        );
        let gain = self.gain.next(vol) * self.level.next(smoothing);
        let (left, right) = pan_gains(position);
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
    ) -> [f32; 2] {
        let mut frame = input;
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            let keeping_up = if self.slot_moving[slot] {
                fast
            } else {
                smoothing
            };
            frame = insert.process(frame, keeping_up, rate);
        }
        let vol = if self.offsets[Target::Vol.index()] == 0.0 {
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
    ) -> [f32; 2] {
        if input == [0.0, 0.0] && !self.ringing(rate) {
            return [0.0, 0.0];
        }
        let frame = self.slots_and_level(input, smoothing, fast, rate);
        let position =
            (self.pan.next(smoothing) + self.pan_offset.next(fast) + self.pan_envelope.next(fast))
                .clamp(0.0, 1.0);
        // At the centre, what the effect gave comes through untouched, where `pan_gains` is only close.
        let (left, right) = if position == 0.5 {
            (1.0, 1.0)
        } else {
            pan_gains(position)
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
    ) -> [f32; 2] {
        self.slots_and_level(input, smoothing, fast, rate)
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
