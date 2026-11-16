//! What sets each of a track's values and how it's heard: a step's lock, hold or glide over the
//! track page's value, a hand on a fader, and a macro's turn.

use vt_model::accumulator::Accumulation;
use vt_model::arpeggio::Arpeggio;
use vt_model::insert::INSERTS;
use vt_model::macros::{MACROS, MacroTarget};
use vt_model::settings::{
    FADER_COUNT, FADERS, MAX_MACHINE_SETTINGS, Owner, PLAYED_SETTINGS, SettingId,
};
use vt_model::song::{Track, level_gain};
use vt_model::step::{FxSlot, Mark};

use crate::dsp::voices::Settings;

use super::TrackState;
use super::mods::SongTime;
use super::shaping::pan_position;

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
pub(super) struct ValueLayers {
    pub(super) base: u16,
    pub(super) lock: Option<u16>,
    glide: Option<Glide>,
}

impl ValueLayers {
    pub(super) fn at(base: u16) -> Self {
        Self {
            base,
            lock: None,
            glide: None,
        }
    }
}

/// Where a setting starts on this track, before any step sets it, with a coarse byte and a fine
/// one. A voice keeps its own, an effect's setting is what's in its slot, or its default while
/// another effect is there, and a mod slot's depth and phase are what its page has.
pub(crate) fn base_of(id: SettingId, track: &Track) -> u16 {
    #[cfg(test)]
    LOOKUPS.with(|count| count.set(count.get() + 1));
    track.value_of(id)
}

#[cfg(test)]
thread_local! {
    /// How many settings [`base_of`] has looked up on this thread, for a test to count.
    pub(crate) static LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many of the played settings are the accumulator's, before the arpeggio's.
const ACCUMULATION_SETTINGS: usize = 2;

/// The accumulator's or arpeggio's setting at a place among both.
pub(super) fn played_id(place: usize) -> Option<SettingId> {
    match place.checked_sub(ACCUMULATION_SETTINGS) {
        None => SettingId::accumulation(place),
        Some(at) => SettingId::arpeggio(at),
    }
}

/// What's heard of a value: its lock over its base. A glide is moved along by the song.
/// On the `00` to `FF` scale, with the fine part as the fraction.
fn heard(layers: &ValueLayers) -> f32 {
    scaled(layers.lock.unwrap_or(layers.base))
}

/// A value with a fine part on the `00` to `FF` scale, so `4080` is 64.5.
pub(super) fn scaled(value: u16) -> f32 {
    f32::from(value) / 256.0
}

impl TrackState {
    /// Have each macro put the settings it holds where it has them, now, with what modulation
    /// last added to it, as a new version's maps may differ or the settings have just been set
    /// from the song's.
    pub(crate) fn put_macros_again(&mut self, track: &Track) {
        self.wake();
        self.macro_heard = [None; MACROS];
        // A generator's setting no macro holds now goes back to its shape.
        self.gen_holds.forget_macros();
        let moves = std::array::from_fn(|at| {
            SettingId::macro_at(at).map_or(0.0, |id| self.offsets[id.index()])
        });
        self.put_macros(track, &moves);
    }

    /// Put each macro that has moved since the block began where it has its settings now, as a
    /// row's `MAC` lock has just moved it, so the row is heard with it rather than from the next
    /// block. Whether that moved a generator's setting, so the row is to be read again.
    pub(crate) fn put_macros_now(&mut self, track: &Track) -> bool {
        let before = self.gen_holds.changes();
        let moves = std::array::from_fn(|at| {
            SettingId::macro_at(at).map_or(0.0, |id| self.offsets[id.index()])
        });
        self.put_macros(track, &moves);
        self.gen_holds.changes() != before
    }

    /// Put each macro's settings where it has them now, its held value and what modulation adds
    /// to it, `moves` on the `00` to `FF` scale over 255, as a hand's turn of each would: a step's
    /// lock of one still wins on its row, and modulation of one goes on top. A macro that hasn't
    /// moved since does nothing, so one at rest costs a comparison.
    pub(super) fn put_macros(&mut self, track: &Track, moves: &[f32; MACROS]) {
        for (at, held) in track.macros.iter().enumerate() {
            if held.maps.is_empty() {
                self.macro_heard[at] = None;
                continue;
            }
            let value = (self.macro_held[at] + moves[at] * 255.0).clamp(0.0, 255.0);
            if self.macro_heard[at] == Some(value) {
                continue;
            }
            self.macro_heard[at] = Some(value);
            for map in &held.maps {
                match map.target {
                    MacroTarget::Setting(id) => self.set_base(id, map.at(value)),
                    // A generator's is held for the engine to render with as it reads each row,
                    // so a lock, a glide or modulation reaches it; at the macro's turn, it's what
                    // the phrase holds, so nothing renders.
                    MacroTarget::Gen { column, param } => {
                        self.gen_holds
                            .hold_by_macro(column, param, map.number_at(value));
                    }
                }
            }
        }
    }

    /// The accumulator's settings as they're heard, its `Up` and `Down` as macros and modulation
    /// have them.
    pub(crate) fn heard_accumulation(&self, track: &Track) -> Accumulation {
        let mut heard = track.accumulation;
        for at in 0..ACCUMULATION_SETTINGS {
            if let Some(value) = self.played(at, track.accumulation.own_value(at)) {
                heard.set_own(at, value);
            }
        }
        heard
    }

    /// The arpeggio's settings as they're heard, its `Rate` and `Octaves` as macros and
    /// modulation have them.
    pub(crate) fn heard_arpeggio(&self, track: &Track) -> Arpeggio {
        let mut heard = track.arpeggio;
        for at in 0..PLAYED_SETTINGS - ACCUMULATION_SETTINGS {
            let place = ACCUMULATION_SETTINGS + at;
            if let Some(value) = self.played(place, track.arpeggio.own_value(at)) {
                heard.set_own(at, value);
            }
        }
        heard
    }

    /// One of the accumulator's or arpeggio's settings as it's heard, by its place among both, if
    /// it isn't what the song has written.
    fn played(&self, place: usize, written: u16) -> Option<f32> {
        let held = self.played_held[place];
        let by = played_id(place).map_or(0.0, |id| self.offsets[id.index()]);
        (by != 0.0 || (held * 256.0).round() as u16 != written).then_some(held + by * 255.0)
    }

    /// Hear the voice and effects the track has now. One that's changed is heard from its own
    /// values, as steps and the track page left them.
    pub(super) fn load(&mut self, track: &Track) {
        let voice = track.voice();
        if voice != self.loaded_voice {
            self.loaded_voice = voice;
            self.voice_settings = std::array::from_fn(|at| {
                voice.and_then(|voice| SettingId::voice(voice, at).map(SettingId::index))
            });
            if let Some(voice) = voice {
                for at in 0..MAX_MACHINE_SETTINGS {
                    if let Some(id) = SettingId::voice(voice, at) {
                        self.synth[at] = heard(&self.values[id.index()]) / 255.0;
                    }
                }
            }
        }
        for slot in 0..INSERTS {
            let effect = track.inserts[slot].kind();
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

    /// What a synth on this track plays with: the held values and what modulation adds.
    pub(crate) fn synth_settings(&self) -> Settings {
        let at = |setting: usize| {
            let offset = self.voice_settings[setting].map_or(0.0, |index| self.offsets[index]);
            (self.synth[setting] + offset).clamp(0.0, 1.0)
        };
        Settings::from_places(std::array::from_fn(at))
    }

    /// Work out the gains of the held values that are heard on a level scale.
    pub(super) fn update_gains(&mut self) {
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

    /// Run a lock or hold value from a step. Anything else on that value gives way to it. A fader
    /// a hand has taken over keeps it underneath, unheard, for when it's given back.
    pub(crate) fn set_value(&mut self, id: SettingId, value: u16, mark: Mark) {
        self.wake();
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
                .is_some_and(|param| param.key == "trig")
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
    pub(crate) fn take_over(&mut self, fader: usize) {
        self.wake();
        let Some(run) = self.faders.get_mut(fader) else {
            return;
        };
        run.taken = true;
        let song = run.song;
        self.write_value(FADERS[fader], scaled(song));
    }

    /// Give a fader back to the steps, as Live's Re-Enable Automation does: it's heard where they
    /// have it now, a lock's or a hold's value or, at the next update, a glide's place.
    pub(crate) fn give_back(&mut self, fader: usize) {
        self.wake();
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
    pub(crate) fn give_back_all(&mut self) {
        for fader in 0..FADER_COUNT {
            self.give_back(fader);
        }
    }

    /// Run a glide value from a step, toward the next value of its parameter if there is one.
    /// `to` is that value, with how many ticks away it is.
    pub(crate) fn start_glide(
        &mut self,
        id: SettingId,
        value: u16,
        to: Option<(FxSlot, f64)>,
        now: SongTime,
    ) {
        self.wake();
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
    pub(crate) fn end_locks(&mut self) {
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
    pub(crate) fn set_base(&mut self, id: SettingId, value: u16) {
        self.wake();
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
    pub(super) fn advance_glides(&mut self, now: SongTime) {
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
    pub(super) fn write_value(&mut self, id: SettingId, value: f32) {
        self.wake();
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
                SettingId::PHASE => {
                    self.held_phase = value;
                    return;
                }
                SettingId::VOICE_MODE => {
                    self.held_voice_mode = value;
                    return;
                }
                SettingId::GLIDE_MODE => {
                    self.held_glide_mode = value;
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
            // Put onto what it holds as the modulation is next worked out.
            (Owner::Macro(at), _) => {
                self.macro_held[at] = value;
                return;
            }
            // Read through a copy of the slot, or of the track's settings, as they're next used.
            (Owner::ModOwn(slot), at) => {
                self.own_held[slot][at] = value;
                return;
            }
            (Owner::Accumulation, at) => {
                self.played_held[at] = value;
                return;
            }
            (Owner::Arpeggio, at) => {
                self.played_held[ACCUMULATION_SETTINGS + at] = value;
                return;
            }
            // Read as each of the lane's rows starts.
            (Owner::Lane(slot), at) => {
                self.lane_held[slot][at] = value;
                return;
            }
            _ => return,
        }
        self.voice.set_synth(self.synth_settings());
        self.update_gains();
    }
}
