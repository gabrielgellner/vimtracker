//! Fixed-size per-track playback state, made ahead of time so the callback never allocates.

use vt_model::constants::{FX_SLOTS, UNITY_LEVEL};
use vt_model::insert::{EffectKind, INSERTS, Insert, MAX_KEPT_PARAMS};
use vt_model::macros::MACROS;
use vt_model::modulation::MOD_SLOTS;
use vt_model::order::{MOST_ORDER_NUMBER, WalkMove};
use vt_model::playable::Playable;
use vt_model::settings::{
    FADER_COUNT, FADERS, LANE_SETTINGS, MAX_MACHINE_SETTINGS, MOD_OWN_SETTINGS, PLAYED_SETTINGS,
    PlayMode, SAMPLER_SETTINGS, SettingId, gate_delta, gate_share, glide_ms, moved_velocity,
    quantised_nudge,
};
use vt_model::song::{DEFAULT_ACCENT, FilterType, SynthVoice, Track};
use vt_model::step::slice_letter;

use crate::dsp::blocks::smoother::Smoothed;
use crate::dsp::effects::memory::MemoryPool;
use crate::dsp::effects::slot::InsertState;
use crate::dsp::filters::filter::{Channel, Coefficients, Ramp, Trim};
use crate::gens::GenHolds;
use crate::mixer::Reading;
use crate::rng::Pcg32;
use crate::timing::SlotTimes;
use crate::voice::{Carry, Cut, NoteOn, Voice};

mod lanes;
mod live;
mod mods;
mod pending;
mod seeds;
mod shaping;
mod values;

#[cfg(test)]
mod tests;

pub use lanes::LaneRead;
pub use live::{Drawn, Faders, LfoNow, LiveValues};
pub use pending::NotePitch;

pub(crate) use mods::SongTime;
pub(crate) use pending::{
    Accumulating, Accumulators, Arpeggiating, Clock, Decided, DelayedNote, Epoch, JumpAt,
    PENDING_PARTS, Part, PendingStep, Retrigger, RowKey, Worked,
};
pub(crate) use shaping::{MasterRest, RunContext, ShapedFrame};
#[cfg(test)]
pub(crate) use values::LOOKUPS;
pub(crate) use values::base_of;

use lanes::{LaneRun, lanes_at_rest};
use live::FaderRun;
use mods::{EnvelopeRun, LfoStart, MOVED, Rest, SlotRun};
use pending::WORKED_KEPT;
use seeds::{
    accumulator_rng, arp_rng, drift_rng, effect_rng, groove_rng, jump_rng, mutate_rng, roll_rng,
};
use shaping::{LeavingFilter, pan_gains};
use values::{ValueLayers, played_id, scaled};

/// One track's voices, held FX values and pending events.
#[derive(Debug, Clone)]
pub(crate) struct TrackState {
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
    /// or an `RND` moved it.
    pub accumulated: Option<u8>,
    /// What its `Jump` decided for the visit it's on, which its later pulses keep.
    pub(crate) jump: Option<JumpAt>,
    /// How its settings move its phrase's walk at the step it last worked out, or is working out
    /// (decision 400).
    pub(crate) walk: WalkMove,
    /// The rolls a `Random` accumulator takes, apart from `rng` so `CHA` rolls as it did.
    pub accumulator_rng: Pcg32,
    /// What's added to the count for each FX column's lane, so a lane changed while playing stays
    /// on its step, where it's placed from the count. Anything that sets `started` again sets
    /// these back to 0.
    pub lane_offsets: [i64; FX_SLOTS],
    /// Where each FX column's lane is as it plays, counted along a row at a time.
    lane_runs: [LaneRun; FX_SLOTS],
    /// The held values of each FX column's `Length` and `Rate`, on the `00` to `FF` scale, which a
    /// lane reads as each of its rows starts.
    lane_held: [[f32; LANE_SETTINGS]; FX_SLOTS],
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
    /// Where each of the loaded voice's settings is in the registry, by its place among the
    /// voice's, looked up as it's loaded rather than at every update.
    voice_settings: [Option<usize>; MAX_MACHINE_SETTINGS],
    loaded_effects: [Option<EffectKind>; INSERTS],
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
    /// Held `FTYP`.
    filter_type: FilterType,
    /// The filter a change of type left, heard fading out under the new one, so a lock of
    /// `FTYP` doesn't click (decision 398).
    leaving: Option<LeavingFilter>,
    /// Whether nothing has gone into the filter since it last settled, so a new type can take
    /// over at once, with nothing to fade from.
    filter_quiet: bool,
    /// Held `MODE`, a place in the list of modes, which the filter's type may not offer.
    filter_mode: f32,
    /// Held `FDRV`, `FPCH`, `LCUT` and `HCUT`, on `0..=1`, which what modulation adds goes on
    /// top of in their smoothers' targets.
    held_drive: f32,
    held_punch: f32,
    held_low_cut: f32,
    held_high_cut: f32,
    drive: Smoothed,
    /// `Punch` as it's heard, which only the clean types hear.
    punch: Smoothed,
    /// Held `Safe`, 1 for on, which glides so switching it doesn't jump `Res`.
    safe: Smoothed,
    /// Held `Key track`, a place in `Off` and `On`.
    held_key_track: f32,
    /// What `Key track` adds to the cutoff, on its `0..=1` scale: the voice's note in octaves
    /// from `C-3`, smoothed as the cutoff is, so a new note's moves it as a change of `CUT` does
    /// and a glide's moves it as the glide goes. Nothing while it's off (decision 381).
    key_track: Smoothed,
    /// Whether `key_track` jumps to the next note heard rather than moving there, as the filter
    /// has settled while nothing sounded.
    key_settles: bool,
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
    /// The numbers the engine holds its generators' settings at, rather than their shapes'.
    pub gen_holds: GenHolds,
    /// Where the track's last note started its modulation, for an LFO's fade and its hold, or
    /// none since playing started.
    noted: Option<LfoStart>,
    /// Whether a note has started or been tied into since the modulation was last worked out,
    /// which a Track slot follows, so the engine works it out again before the note is heard.
    pub new_note: bool,
    /// Whether the voice was started afresh since the modulation was last worked out, so one that
    /// follows the note can give it its pitch from its first frame rather than slew to it.
    pub fresh_voice: bool,
    /// The row of a note just started, on a track with a slot on `Pitch` or a row with an `RND` on
    /// its note, whose moves the next working out of the modulation reports, as that's the first
    /// to hear the note.
    pub pitch_row: Option<RowKey>,
    /// How far that row's own `RND`s moved its note, in semitones, for the report.
    pub pitch_varied: i8,
    /// What each slot moved that note's pitch by, until the report takes it.
    pitched: Option<NotePitch>,
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
    /// The same in octaves, and the semitones both were worked out from, bit for bit, as `powf`
    /// and `log2` are only worth working out again when it moves.
    pitch_octaves: f32,
    pitch_from: Option<u32>,
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
    /// What `Phase` holds, as a place in `Phase::ALL`.
    held_phase: f32,
    /// What `Mode` holds, as a place in `VoiceMode::ALL`.
    held_voice_mode: f32,
    /// What `Glide mode` holds, as a place in `GlideMode::ALL`.
    held_glide_mode: f32,
    /// Held `SMP`, `STA`, `SLEN` and `PLY`, the sampler's settings, which its next note reads as it
    /// starts, on the `00` to `FF` scale: the bank entry it plays by number, and how it cuts it.
    held_sampler: [f32; SAMPLER_SETTINGS],
    /// Whether each slot is switched out as it's heard: its own bypass, or a step's lock of it.
    bypassed: [bool; INSERTS],
    /// The frames rendered when the shares last moved.
    shared_at: u64,
    /// What its modulation was last worked out with, if that left nothing moving, so working it
    /// out again changes nothing until something it reads does, which wakes it.
    rest: Option<Rest>,
    /// The loaded voice's held settings, in its order, on `0..=1`.
    synth: [f32; MAX_MACHINE_SETTINGS],
    /// Whether an LFO moves the filter, so it stays running at the top of its range.
    filter_moving: bool,
    /// Its sound after its slots in the latest frame, for a compressor on another track to hear.
    pub sidechain: [f32; 2],
    /// How long each slot has taken, while `vimtracker bench --tracks` times the engine.
    pub slot_times: SlotTimes,
    /// Each macro's held value, from its turn, a step's lock or a glide, before modulation, on
    /// the `00` to `FF` scale.
    macro_held: [f32; MACROS],
    /// Where each macro last put the settings it holds, with modulation, or none to put them
    /// there again, as after a new version, whose maps may differ.
    macro_heard: [Option<f32>; MACROS],
    /// On a track in a group, where each of the group's macros last put the settings it holds on
    /// this track, with modulation, or none to put them there again (decision 408).
    group_macro_heard: [Option<f32>; MACROS],
    /// The held values of each mod slot's type's own settings, and of the accumulator's and the
    /// arpeggio's, which a macro sets as a hand does, and a step's lock or hold of a labelled one.
    /// What's heard is read through a copy with them put in, [`heard_slot`](mods::heard_slot),
    /// wherever they differ from the song's.
    own_held: [[f32; MOD_OWN_SETTINGS]; MOD_SLOTS],
    played_held: [f32; PLAYED_SETTINGS],
    /// Whether a step has set one of the mod slots' own settings since the track was made, so its
    /// slots are read through a copy from then on; a song that never locks one reads them as
    /// they are, as before (decision 399).
    own_stepped: bool,
}

impl TrackState {
    /// A silent track with default held values.
    pub(crate) fn new(seed: u64, index: usize) -> Self {
        Self {
            launched: None,
            cued: None,
            playing: None,
            started: 0,
            lane_offsets: [0; FX_SLOTS],
            lane_runs: [LaneRun::default(); FX_SLOTS],
            lane_held: [lanes_at_rest(); FX_SLOTS],
            row: Decided::PLAYS,
            passed: false,
            tied: false,
            chain_starts: 0,
            realign_waiting: false,
            accumulators: Accumulators::default(),
            accumulated: None,
            jump: None,
            walk: WalkMove::default(),
            accumulator_rng: accumulator_rng(seed, index),
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
            voice_settings: [None; MAX_MACHINE_SETTINGS],
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
            leaving: None,
            filter_quiet: true,
            filter_mode: 0.0,
            held_drive: 0.0,
            held_punch: 0.0,
            held_low_cut: 0.0,
            held_high_cut: 1.0,
            drive: Smoothed::at(0.0),
            punch: Smoothed::at(0.0),
            safe: Smoothed::at(0.0),
            held_key_track: f32::from(SettingId::KEY_TRACK.entry().default),
            key_track: Smoothed::at(0.0),
            key_settles: false,
            trim: [Trim::default(); 2],
            low_cut: Smoothed::at(0.0),
            inserts: std::array::from_fn(|slot| {
                let mut insert = InsertState::default();
                insert.reseed(effect_rng(seed, index, slot));
                insert
            }),
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
            gen_holds: GenHolds::default(),
            noted: None,
            new_note: false,
            fresh_voice: false,
            pitch_row: None,
            pitch_varied: 0,
            pitched: None,
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
            pitch_octaves: 0.0,
            pitch_from: None,
            held_pitch: 128.0,
            held_gate: f32::from(SettingId::GATE_LENGTH.entry().default),
            held_gate_delta: f32::from(SettingId::GATE_DELTA.entry().default),
            held_velocity_delta: f32::from(SettingId::VELOCITY_DELTA.entry().default),
            held_glide: f32::from(SettingId::GLIDE.entry().default),
            held_phase: f32::from(SettingId::PHASE.entry().default),
            held_voice_mode: f32::from(SettingId::VOICE_MODE.entry().default),
            held_glide_mode: f32::from(SettingId::GLIDE_MODE.entry().default),
            held_sampler: std::array::from_fn(|at| {
                SettingId::sampler(at).map_or(0.0, |id| f32::from(id.entry().default))
            }),
            bypassed: [false; INSERTS],
            shared_at: 0,
            rest: None,
            filter_moving: false,
            sidechain: [0.0; 2],
            slot_times: SlotTimes::default(),
            macro_held: [0.0; MACROS],
            macro_heard: [None; MACROS],
            group_macro_heard: [None; MACROS],
            own_held: std::array::from_fn(|slot| {
                std::array::from_fn(|at| {
                    SettingId::mod_own(slot, at).map_or(0.0, |id| f32::from(id.entry().default))
                })
            }),
            played_held: std::array::from_fn(|at| {
                played_id(at).map_or(0.0, |id| f32::from(id.entry().default))
            }),
            own_stepped: false,
        }
    }

    /// Start its chain at its next step, which falls here, with its steps counted from here.
    pub(crate) fn restart_at(&mut self, at: u64) {
        self.clock.line_up(at);
        self.restart(self.clock.count);
    }

    /// Start counting the track's chain from this running count, with its lanes in line again.
    pub(crate) fn restart(&mut self, started: i64) {
        self.started = started;
        self.lane_offsets = [0; FX_SLOTS];
        self.unplace_lanes();
        self.tied = false;
        self.chain_starts += 1;
    }

    /// Forget what the last conditions decided, as playing starts, so `PRE` and `NEI` start from
    /// none having passed.
    pub(crate) fn forget_conditions(&mut self) {
        self.row = Decided::PLAYS;
        self.passed = false;
    }

    /// Put its accumulators back to the notes as written, as playing starts.
    pub(crate) fn forget_accumulators(&mut self) {
        self.accumulators.forget();
        self.accumulated = None;
    }

    /// Carry the sounding note on as this one, with no new attack, gliding there for as long as
    /// `GLD` says as it's heard now.
    pub(crate) fn tie(&mut self, note: NoteOn) {
        self.wake();
        self.last_note = Some(note);
        self.new_note = true;
        self.voice.tie(note, glide_ms(self.held_glide));
        self.tell_slots_the_note();
    }

    /// Whether a note starting now ties into the one sounding rather than striking: in `Legato`,
    /// while that note's gate is still open on the same synth or sample (decision 380).
    pub(crate) fn ties_legato(&self, note: &NoteOn) -> bool {
        self.legato() && self.voice.gate_open_on(note)
    }

    /// Whether its `Mode` is `Legato`, as held now, with a lock on the row starting a note.
    pub(crate) fn legato(&self) -> bool {
        self.held_voice_mode >= 0.5
    }

    /// Tie a note into the one sounding, as `Legato` does, as a new row's note: what was timed for
    /// the note before is dropped, as a struck note drops it, and the row times its own.
    pub(crate) fn tie_legato(&mut self, note: NoteOn) {
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.gate_at = None;
        self.tie(note);
    }

    /// What each slot moved the pitch of the last note started by, once, for the report.
    pub(crate) fn take_note_pitch(&mut self) -> Option<NotePitch> {
        self.pitched.take()
    }

    /// Tell the slots what note is playing, with the track's `Pitch` and its modulation, for an
    /// oscillator of their own to follow.
    fn tell_slots_the_note(&mut self) {
        let played = self.last_note.map_or(60.0, |note| f32::from(note.note));
        let note = played + 12.0 * self.pitch_octaves;
        for insert in &mut self.inserts {
            insert.follow_note(note);
        }
    }

    /// Reset held values, the random generator and LFOs, when playback goes back to the top. Values
    /// glide back, so anything still sounding doesn't click.
    pub(crate) fn reset_held(&mut self, seed: u64, index: usize, track: &Track, clock: u64) {
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
        self.put_macros_again(track);
    }

    /// Reset what playing from the top does apart from the settings: the random generators,
    /// conditions, accumulators, arpeggio, last note and LFOs. Enough on its own for a state no
    /// track has had since its settings were last reset to an empty track's, as nothing else
    /// changes them, and going through every setting is most of a reset's time.
    pub(crate) fn reset_all_but_settings(&mut self, seed: u64, index: usize, clock: u64) {
        self.wake();
        self.rng = Pcg32::new(seed, index as u64);
        self.forget_conditions();
        self.forget_accumulators();
        self.groove_rng = groove_rng(seed, index);
        self.arp_rng = arp_rng(seed, index);
        self.drift_rng = drift_rng(seed, index);
        self.accumulator_rng = accumulator_rng(seed, index);
        for (slot, insert) in self.inserts.iter_mut().enumerate() {
            insert.reseed(effect_rng(seed, index, slot));
        }
        self.arp = None;
        self.last_note = None;
        self.chain_pass = 0;
        self.jump = None;
        for (at, run) in self.slots.iter_mut().enumerate() {
            run.restart(LfoStart { tick: 0.0, clock });
            run.notes = 0;
            run.dice = mutate_rng(seed, index, at);
            run.rolls = roll_rng(seed, index, at);
            run.jumps = jump_rng(seed, index, at);
            run.roll = 0;
            run.drawn = 0;
            run.counted = None;
        }
        self.noted = None;
    }

    /// Start a new note, cancelling anything pending for the old one.
    pub(crate) fn start(&mut self, note: NoteOn, rate: u32) {
        self.begin(note, rate, false);
    }

    /// Start an arpeggio's next note again, as `start` does, with any noise its synth draws taken
    /// from the arpeggio's own random stream, so `CHA` rolls as it would without it.
    pub(crate) fn restrike(&mut self, note: NoteOn, rate: u32) {
        self.begin(note, rate, true);
    }

    fn begin(&mut self, note: NoteOn, rate: u32, arpeggio: bool) {
        self.wake();
        // `Acid` plays one note at a time, and the next carries on from its filter and accent.
        let acid = note.synth == Some(SynthVoice::Acid) && self.voice.plays_acid();
        // With `Phase` at `Reset`, a synth whose sound has a phase starts again at the bottom of
        // the dip, which it always takes (decision 370).
        let restart = self.held_phase >= 0.5 && note.synth.is_some_and(SynthVoice::has_phase);
        // Any other synth that holds its notes carries on too, its oscillators running on into
        // the next note rather than a new voice starting under the old one's fade. At `Free`, so
        // does one gone quiet on its own since its last note, from where its oscillators stopped
        // (decision 374).
        let resumes = !acid && !restart && self.voice.resumes(note.synth);
        let carries = !acid && (resumes || self.voice.carries(note.synth));
        // A note on a voice of its own cuts the one sounding, and starts under its fade.
        let cuts = !acid && !carries && self.voice.active();
        // With `Glide mode` at `Always`, a struck note's pitch glides from the note still sounding
        // on the same synth or sample, as a tie's does, though it strikes. `Acid` slides by its
        // own `Slide` and a 303's slide flag alone (decision 382).
        let glide = (self.held_glide_mode >= 0.5 && note.synth != Some(SynthVoice::Acid))
            .then(|| glide_ms(self.held_glide))
            .flatten()
            .and_then(|ms| Some((self.voice.sounding_speed(&note)?, ms)));
        if !acid && !carries {
            self.cut_to_tail();
        }
        self.delayed = None;
        self.kill_at = None;
        self.off_at = None;
        self.gate_at = None;
        self.last_note = Some(note);
        self.new_note = true;
        self.tell_slots_the_note();
        if acid {
            self.voice.retrigger(note);
        } else if cuts {
            self.voice = Voice::start_under_cut(note, rate, self.pitch);
            self.fresh_voice = true;
        } else if !carries {
            self.voice = Voice::start(note, rate, self.pitch);
            self.fresh_voice = true;
        }
        if let Some((from, ms)) = glide.filter(|_| !acid && !carries) {
            self.voice.glide_from(from, ms);
        }
        self.voice.set_synth(self.synth_settings());
        // Each note draws its seeds as it's played, whether it has a voice of its own or not, so
        // the streams go on as they would and each note rolls what it would on a new voice.
        let noise = self.voice.uses_noise().then(|| {
            let rng = if arpeggio {
                &mut self.arp_rng
            } else {
                &mut self.rng
            };
            rng.next_u32()
        });
        // `Acid` carrying on keeps the drift it has, and rolls its own offsets as it starts.
        let drift = (!acid && self.voice.drifts()).then(|| self.drift_rng.next_u32());
        if resumes {
            // It has played nothing to slew from, as a new voice hasn't.
            self.voice.set_pitch(self.pitch);
            self.fresh_voice = true;
        }
        if carries {
            self.voice.carry(Carry {
                note,
                noise,
                drift,
                dip: note.shape.reset() || restart,
                restart,
                glide: glide.map(|(_, ms)| ms),
            });
            return;
        }
        if let Some(seed) = noise {
            self.voice.seed_noise(seed);
        }
        if let Some(seed) = drift {
            self.voice.seed_drift(seed);
        }
    }

    /// How much of its last pulse a note starting now lasts: its row's own length, or the held
    /// `Gate` if it has none, moved up or down by `LDLT` and by what modulation adds to `Gate`, as
    /// an LFO moves any setting around where a step put it. 1 holds it to the next note.
    pub(crate) fn gate_share(&self, own: Option<u8>) -> f32 {
        let base = own.map_or(self.held_gate, f32::from);
        let moved =
            gate_delta(self.held_gate_delta) + self.offsets[SettingId::GATE_LENGTH.index()] * 255.0;
        gate_share(base + moved)
    }

    /// A note's velocity as it starts, its own moved up or down by held `VDLT` and by what
    /// modulation adds to it, kept to the velocity's range. It's read once, so a sounding note
    /// keeps it.
    pub(crate) fn velocity(&self, own: u8) -> u8 {
        let moved =
            self.held_velocity_delta + self.offsets[SettingId::VELOCITY_DELTA.index()] * 255.0;
        moved_velocity(own, moved)
    }

    /// What modulation adds to the track's `Jump` now, on the `00` to `FF` scale.
    pub(crate) fn jump_moved(&self) -> f32 {
        self.offsets[SettingId::JUMP.index()] * 255.0
    }

    /// What modulation adds to one of the track's own settings now, on the scale a step gives it:
    /// the whole of a list, or `FF` for a value, at full depth.
    pub(crate) fn moved_by(&self, id: SettingId) -> f32 {
        self.offsets[id.index()] * id.entry().span()
    }

    /// One of the settings that move its phrase's walk, as chosen before a row plays (`chosen`,
    /// what's held or a lock), with what modulation adds (decision 400).
    pub(crate) fn walk_value(&self, id: SettingId, chosen: u8) -> u8 {
        (f32::from(chosen) + self.moved_by(id))
            .round()
            .clamp(0.0, 255.0) as u8
    }

    /// Whether a step's lock holds a setting now, over what the track holds.
    pub(crate) fn locked(&self, id: SettingId) -> bool {
        self.values[id.index()].lock.is_some()
    }

    /// How its settings move its phrase's walk with no lock read on the row: what it holds, with
    /// what modulation adds (decision 400).
    pub(crate) fn held_walk(&self) -> WalkMove {
        self.walk_of(|id| self.held_choice(id, false))
    }

    /// How its settings move its phrase's walk, from what's chosen of each before a row plays,
    /// with what modulation adds.
    pub(crate) fn walk_of(&self, mut chosen: impl FnMut(SettingId) -> u8) -> WalkMove {
        let mut value = |id: SettingId| self.walk_value(id, chosen(id));
        WalkMove {
            order: value(SettingId::ORDER),
            reverse: value(SettingId::REVERSE) >= 1,
            x: value(SettingId::ORDER_X).min(MOST_ORDER_NUMBER),
            y: value(SettingId::ORDER_Y).min(MOST_ORDER_NUMBER),
        }
    }

    /// A row's nudge as it plays, in ticks, from the nudge written and the `QNT` the row chose
    /// before it played, moved by what modulation adds.
    pub(crate) fn quantised(&self, written: i64, chosen: u8) -> i64 {
        let value = f32::from(chosen) + self.offsets[SettingId::QUANTISE.index()] * 255.0;
        quantised_nudge(written, value)
    }

    /// What a step chooses of a setting read before it plays, as `GRV` and `RATE` are, on its
    /// scale: a lock over the held value, or the held value alone once a new note has ended the
    /// locks.
    pub(crate) fn held_choice(&self, id: SettingId, locks_end: bool) -> u8 {
        let layers = &self.values[id.index()];
        let value = match layers.lock {
            Some(lock) if !locks_end => lock,
            _ => layers.base,
        };
        (value >> 8) as u8
    }

    /// The bank entry a sampler's note starting now plays, by number: the held `SMP`, or a lock
    /// over it, moved by what modulation adds, in whole entries.
    pub(crate) fn sample_number(&self) -> u8 {
        self.sampler_value(SettingId::SAMPLE).round() as u8
    }

    /// The slice of its entry a sampler's note starting now plays, by number: the held `SLI`, or
    /// a lock over it, moved by what modulation adds, in whole slices. The entry clamps it to its
    /// last. An `SLI` by a letter plays the slice the entry's mark names, or, with no such mark,
    /// the track's own: the held `SLI` under the lock, or `own`, the track's setting, if what's
    /// held is a letter too.
    pub(crate) fn slice_number(&self, playable: &Playable, own: u8) -> u8 {
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
    pub(crate) fn sample_cut(&self) -> Cut {
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

    /// Fade the playing note.
    pub(crate) fn cut_to_tail(&mut self) {
        self.wake();
        if self.voice.active() {
            self.tail = self.voice;
            self.tail.fade();
            self.voice = Voice::default();
        }
    }

    /// Stop sounding and drop anything pending, when the transport halts.
    pub(crate) fn stop(&mut self) {
        self.wake();
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

    /// Give the long buffers its slots hold back to the pool, as the track's gone.
    pub(crate) fn give_back_memory(&mut self, memory: &mut MemoryPool) {
        self.wake();
        for insert in &mut self.inserts {
            insert.give_back(memory);
        }
    }

    /// Whether a slot still has something to play, with nothing going in.
    #[inline]
    pub(crate) fn ringing(&self, rate: u32) -> bool {
        self.inserts.iter().any(|insert| insert.ringing(rate))
    }

    /// Whether anything is sounding.
    #[inline]
    pub(crate) fn sounding(&self) -> bool {
        self.voice.active() || self.tail.active()
    }

    /// Jump held values to what's set, so a new sound doesn't ramp from an old setting. It's
    /// called on every row read, and every preview started, while nothing's [`sounding`]: a
    /// jump is safe only with nothing passing through. What's before the slots hears only the
    /// voice, so it jumps then. The slots, and the level, pan and sends after them, jump only
    /// once no slot's tail is [`ringing`] either, as a tail passes through them and a jump there
    /// is a step (#923).
    ///
    /// [`sounding`]: Self::sounding
    /// [`ringing`]: Self::ringing
    pub(crate) fn settle(&mut self, rate: u32) {
        self.input_trim.settle();
        self.cutoff.settle();
        self.resonance.settle();
        self.cut_offset.settle();
        self.res_offset.settle();
        self.cut_envelope.settle();
        self.key_track.settle();
        self.key_settles = true;
        self.res_envelope.settle();
        self.drive.settle();
        self.punch.settle();
        self.safe.settle();
        self.low_cut.settle();
        self.high_cut.settle();
        self.filter.iter_mut().for_each(Channel::settle);
        self.leaving = None;
        self.filter_quiet = true;
        self.coefficients = None;
        self.filter_countdown = 0;
        if self.ringing(rate) {
            return;
        }
        self.inserts.iter_mut().for_each(InsertState::settle);
        self.gain.settle();
        self.level.settle();
        self.pan.settle();
        self.pan_offset.settle();
        self.pan_envelope.settle();
        self.send_a_gain.settle();
        self.send_b_gain.settle();
        self.heard.settle();
        self.feeds.settle();
    }
}
