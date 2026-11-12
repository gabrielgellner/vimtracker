//! Changing the setting under the cursor: `=` and `-` and their large steps, the fine digits, and
//! `x` back to its default.

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::{Action, TrackControl};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES};
use vt_model::constants::{BAR_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::{Curve, Hold, MAX_HOLD_MS, MAX_RATCHET};
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{LfoMode, MAX_DEFORM, Shape};
use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot, default_mod};
use vt_model::settings::{SettingId, gate_percent, gate_value};
use vt_model::song::{FilterType, Machine, Track};
use vt_model::speed::Speed;

use crate::app::App;
use crate::input::command::Line;

use super::controls::no_modes;
use super::{AccumulatorField, ArpField, Control, ModField};

/// How far a small and a large step move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;

const PHASE_JUMP: i32 = 90;

/// How far a large step moves `Gate`, in percent.
const GATE_JUMP: i32 = 10;

/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i64 = 0x10;

/// How many small steps a large one is for an envelope time.
const TIME_JUMP: u16 = 10;

/// How far a large step moves a sustain, in percent.
const SUSTAIN_JUMP: i16 = 10;

/// The next item in a list, going round.
fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
    let position = items.iter().position(|item| *item == current).unwrap_or(0);
    let step = if up { 1 } else { items.len() - 1 };
    items[(position + step) % items.len()]
}

/// How far a small step moves an envelope time. Short times need finer steps than long ones.
fn time_step(value: u16) -> u16 {
    match value {
        0..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}

/// Move one setting of a mod slot one notch, under its type. The type itself is chosen with `K`
/// alone, as it changes what the device is.
fn adjust_mod(
    modulator: &mut ModSlot,
    slot: usize,
    field: ModField,
    up: bool,
    large: bool,
    track: &Track,
) {
    let time = |value: u16| {
        let size = if large { TIME_JUMP } else { 1 };
        if up {
            (value + time_step(value) * size).min(MAX_ENVELOPE_MS)
        } else {
            value.saturating_sub(time_step(value.saturating_sub(1)) * size)
        }
    };
    let kind = modulator.kind;
    let (adsr, ahd, drum, lfo) = (
        &mut modulator.adsr,
        &mut modulator.ahd,
        &mut modulator.drum,
        &mut modulator.lfo,
    );
    let ahd_kind = kind == ModKind::Ahd;
    match field {
        ModField::Type => {}
        ModField::Mode => match kind {
            ModKind::Adsr => adsr.curve = next_in(&Curve::ALL, adsr.curve, up),
            ModKind::Ahd => ahd.curve = next_in(&Curve::ALL, ahd.curve, up),
            ModKind::Drum => drum.curve = next_in(&Curve::ALL, drum.curve, up),
            ModKind::Lfo => lfo.mode = next_in(&LfoMode::ALL, lfo.mode, up),
        },
        ModField::Polarity => modulator.polarity = Some(modulator.polarity().other()),
        ModField::Destination => {
            let mut choices = vec![None];
            choices.extend(SettingId::for_mod(track, slot).into_iter().map(Some));
            modulator.destination = next_in(&choices, modulator.destination, up);
        }
        ModField::Depth => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = modulator.depth + if up { size } else { -size };
            modulator.depth = moved.clamp(-MAX_DEPTH, MAX_DEPTH);
        }
        ModField::Attack if ahd_kind => ahd.attack_ms = time(ahd.attack_ms),
        ModField::Attack => adsr.attack_ms = time(adsr.attack_ms),
        ModField::Decay if ahd_kind => ahd.decay_ms = time(ahd.decay_ms),
        ModField::Decay if kind == ModKind::Drum => drum.decay_ms = time(drum.decay_ms),
        ModField::Decay => adsr.decay_ms = time(adsr.decay_ms),
        ModField::Peak => drum.peak_ms = time(drum.peak_ms),
        ModField::Body => {
            let size = if large { SUSTAIN_JUMP } else { 1 };
            let by = if up { size } else { -size };
            drum.body = (i16::from(drum.body) + by).clamp(0, 100) as u8;
        }
        ModField::Ratchet => {
            let by = if up { 1 } else { -1 };
            drum.ratchet = (i16::from(drum.ratchet) + by).clamp(1, i16::from(MAX_RATCHET)) as u8;
        }
        // `Note` is below no time, so `-` from 0 ms holds for the note.
        ModField::Hold => {
            ahd.hold = match (ahd.hold, up) {
                (Hold::Note, true) => Hold::Ms(0),
                (Hold::Note, false) | (Hold::Ms(0), false) => Hold::Note,
                (Hold::Ms(ms), _) => Hold::Ms(time(ms).min(MAX_HOLD_MS)),
            };
        }
        ModField::Release => adsr.release_ms = time(adsr.release_ms),
        ModField::Sustain => {
            let size = if large { SUSTAIN_JUMP } else { 1 };
            let by = if up { size } else { -size };
            adsr.sustain = (i16::from(adsr.sustain) + by).clamp(0, 100) as u8;
        }
        ModField::Reset if ahd_kind => ahd.reset = !ahd.reset,
        ModField::Reset => adsr.reset = !adsr.reset,
        ModField::Shape => lfo.shape = next_in(&Shape::ALL, lfo.shape, up),
        ModField::Rate if large => lfo.rate = lfo.rate.jumped(up),
        ModField::Rate => lfo.rate = lfo.rate.nudged(up),
        ModField::Phase => {
            let size = if large { PHASE_JUMP } else { PHASE_NUDGE };
            let by = if up { size } else { -size };
            lfo.phase = (i32::from(lfo.phase) + by).rem_euclid(360) as u16;
        }
        ModField::Deform => {
            let size = if large { HEX_JUMP as i16 } else { 1 };
            let moved = lfo.deform + if up { size } else { -size };
            lfo.deform = moved.clamp(-MAX_DEFORM, MAX_DEFORM);
        }
    }
}

/// A mod slot with one setting back at its default: the slot's own for what every type shares,
/// and for a type's own the default that type has in this slot. `Type` goes back to the slot's
/// first type, with that type's settings as they start, keeping where it goes, how far and its
/// switch, as choosing a type does.
fn mod_at_default(modulator: ModSlot, slot: usize, field: ModField) -> ModSlot {
    let fresh = default_mod(slot);
    let (mut adsr, mut ahd, mut drum, mut lfo) =
        (modulator.adsr, modulator.ahd, modulator.drum, modulator.lfo);
    let ahd_kind = modulator.kind == ModKind::Ahd;
    match field {
        ModField::Type => {
            return ModSlot {
                kind: fresh.kind,
                adsr: if fresh.kind == ModKind::Adsr {
                    fresh.adsr
                } else {
                    adsr
                },
                lfo: if fresh.kind == ModKind::Lfo {
                    fresh.lfo
                } else {
                    lfo
                },
                ..modulator
            };
        }
        ModField::Destination => {
            return ModSlot {
                destination: fresh.destination,
                ..modulator
            };
        }
        // Back to following its type.
        ModField::Polarity => {
            return ModSlot {
                polarity: None,
                ..modulator
            };
        }
        ModField::Depth => {
            return ModSlot {
                depth: fresh.depth,
                ..modulator
            };
        }
        ModField::Mode => match modulator.kind {
            ModKind::Adsr => adsr.curve = fresh.adsr.curve,
            ModKind::Ahd => ahd.curve = fresh.ahd.curve,
            ModKind::Drum => drum.curve = fresh.drum.curve,
            ModKind::Lfo => lfo.mode = fresh.lfo.mode,
        },
        ModField::Attack if ahd_kind => ahd.attack_ms = fresh.ahd.attack_ms,
        ModField::Attack => adsr.attack_ms = fresh.adsr.attack_ms,
        ModField::Decay if ahd_kind => ahd.decay_ms = fresh.ahd.decay_ms,
        ModField::Decay if modulator.kind == ModKind::Drum => drum.decay_ms = fresh.drum.decay_ms,
        ModField::Decay => adsr.decay_ms = fresh.adsr.decay_ms,
        ModField::Peak => drum.peak_ms = fresh.drum.peak_ms,
        ModField::Body => drum.body = fresh.drum.body,
        ModField::Ratchet => drum.ratchet = fresh.drum.ratchet,
        ModField::Hold => ahd.hold = fresh.ahd.hold,
        ModField::Reset if ahd_kind => ahd.reset = fresh.ahd.reset,
        ModField::Sustain => adsr.sustain = fresh.adsr.sustain,
        ModField::Release => adsr.release_ms = fresh.adsr.release_ms,
        ModField::Reset => adsr.reset = fresh.adsr.reset,
        ModField::Shape => lfo.shape = fresh.lfo.shape,
        ModField::Rate => lfo.rate = fresh.lfo.rate,
        ModField::Phase => lfo.phase = fresh.lfo.phase,
        ModField::Deform => lfo.deform = fresh.lfo.deform,
    }
    ModSlot {
        adsr,
        ahd,
        drum,
        lfo,
        ..modulator
    }
}

/// How far a large step moves an accumulator's limit: an octave of a seven-note scale.
const LIMIT_JUMP: u8 = 7;

/// Move one of the accumulator's settings one notch. The limits stop at either end, as does the
/// list of resets, and the short lists go round.
fn adjust_accumulation(
    accumulation: &mut Accumulation,
    field: AccumulatorField,
    up: bool,
    large: bool,
) {
    let limit = |value: u8| {
        let size = if large { LIMIT_JUMP } else { 1 };
        let moved = if up {
            value.saturating_add(size)
        } else {
            value.saturating_sub(size)
        };
        moved.clamp(1, MOST_DEGREES)
    };
    match field {
        AccumulatorField::Up => accumulation.up = limit(accumulation.up),
        AccumulatorField::Down => accumulation.down = limit(accumulation.down),
        AccumulatorField::Moves => {
            accumulation.moves = next_in(&Moves::ALL, accumulation.moves, up)
        }
        AccumulatorField::AtLimit => {
            accumulation.at_limit = next_in(&AtLimit::ALL, accumulation.at_limit, up);
        }
        AccumulatorField::Adds => accumulation.adds = next_in(&Adds::ALL, accumulation.adds, up),
        AccumulatorField::Reset => {
            let resets: Vec<Reset> = Reset::all().collect();
            let at = resets
                .iter()
                .position(|reset| *reset == accumulation.reset)
                .unwrap_or(0);
            let to = if up {
                (at + 1).min(resets.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            accumulation.reset = resets[to];
        }
    }
}

/// Move one of the arpeggio's settings one notch. The rates stop at either end, as they run from
/// the shortest note to the longest, and so do the octaves, while the modes go round.
fn adjust_arpeggio(arpeggio: &mut Arpeggio, field: ArpField, up: bool) {
    match field {
        ArpField::Rate => {
            let rates = ArpRate::ALL;
            let at = rates
                .iter()
                .position(|rate| *rate == arpeggio.rate)
                .unwrap_or(0);
            let to = if up {
                (at + 1).min(rates.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            arpeggio.rate = rates[to];
        }
        ArpField::Mode => arpeggio.mode = next_in(&ArpMode::ALL, arpeggio.mode, up),
        ArpField::Octaves => {
            let moved = if up {
                arpeggio.octaves.saturating_add(1)
            } else {
                arpeggio.octaves.saturating_sub(1)
            };
            arpeggio.octaves = moved.clamp(1, MOST_OCTAVES);
        }
        ArpField::Retrig => arpeggio.retrig = !arpeggio.retrig,
    }
}

impl App {
    pub(super) fn page_track(&self) -> Track {
        self.song()
            .tracks
            .get(self.cursor.track)
            .cloned()
            .unwrap_or_default()
    }

    /// The cursor's track as it's heard, with its faders where the steps have them while they
    /// drive them.
    pub(super) fn page_track_as_heard(&self) -> Track {
        self.track_as_heard(self.cursor.track)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default()
    }

    /// Put the track in a group, or take it out of any.
    pub(super) fn set_group_here(&mut self, group: Option<u8>) {
        self.apply(&Action::SetTrackGroup {
            first: self.cursor.track,
            last: self.cursor.track,
            group,
        });
    }

    /// Replace one of the track's mod slots, as one undo step.
    pub(super) fn set_mod(&mut self, slot: usize, modulator: ModSlot) {
        self.apply(&Action::SetMod {
            track: self.cursor.track,
            slot,
            modulator,
        });
    }

    /// Set how many of the song's steps the track's chain plays before starting again, or none.
    pub(super) fn set_reset(&mut self, reset: Option<u16>) {
        self.apply(&Action::SetTrackReset {
            track: self.cursor.track,
            reset,
        });
    }

    /// Set how long the track's ties glide into the next note.
    pub(super) fn set_glide(&mut self, glide: u8) {
        self.apply(&Action::SetTrackGlide {
            track: self.cursor.track,
            glide,
        });
    }

    pub(super) fn set_hex(&mut self, control: Control, value: u8) {
        let track = self.cursor.track;
        // On the mixer, the cursor can be on a setting this track doesn't have.
        let tracks = &self.song().tracks;
        if tracks.get(track).is_some_and(|t| !control.exists_on(t)) {
            return;
        }
        if let Some(setting) = tracks.get(track).and_then(|t| control.setting_on(t)) {
            self.apply(&Action::SetTrackSetting {
                track,
                setting,
                value,
            });
            return;
        }
        match control {
            Control::Base(control) => self.apply(&Action::SetTrackBase {
                track,
                control,
                value,
            }),
            Control::Level => self.apply(&Action::SetTrackLevel {
                track,
                level: value,
            }),
            Control::Trim => self.apply(&Action::SetTrackTrim { track, trim: value }),
            _ => {}
        }
    }

    pub(super) fn adjust_control(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        // A fader the steps drive moves from where it's heard, as a hand takes it over there.
        let track = self.page_track_as_heard();
        // A machine's setting stops at its own top, as a list stops at its last choice.
        let top = control
            .setting_on(&track)
            .map_or(0xFF, |id| id.entry().kind.top());
        let change = |value: u8| {
            let size = if large { HEX_JUMP } else { 1 };
            let moved = i64::from(value.min(top)) + if up { size } else { -size } * count as i64;
            moved.clamp(0, i64::from(top)) as u8
        };
        match control {
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                let on = if control == Control::Mute {
                    live.mute
                } else {
                    live.solo
                };
                if on != up {
                    self.toggle_control(control);
                }
            }
            // `=` puts it on and `-` takes it off, as on the project page it was on.
            Control::Limiter if self.song().limiter != up => self.set_limiter(up),
            Control::Limiter => {}
            Control::Setting(_) => {
                if let Some(id) = control.setting_on(&track) {
                    self.set_hex(control, change(track.setting(id)));
                }
            }
            Control::Machine => {
                let machine = next_in(&Machine::ALL, track.machine, up);
                self.apply(&Action::SetTrackMachine {
                    track: self.cursor.track,
                    machine,
                });
            }
            Control::Sample => {
                let size = if large { HEX_JUMP as usize } else { 1 };
                let sample = self.next_sample(track.sample(), up, size * count);
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample,
                });
            }
            Control::Base(which) => {
                let value = match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::SendA => track.send_a,
                    TrackControl::SendB => track.send_b,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Pitch => track.pitch,
                    TrackControl::VelocityDelta => track.velocity_delta,
                };
                self.set_hex(control, change(value));
            }
            Control::Level => self.set_hex(control, change(track.level)),
            Control::Trim => self.set_hex(control, change(track.trim)),
            Control::Insert(slot) => self.set_insert(slot, track.inserts[slot].stepped(up)),
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                let Some(found) = insert.params().get(param) else {
                    return;
                };
                let value = insert.value(param);
                let count = found.choices.len() as u8;
                let moved = if found.choices.is_empty() {
                    change(value)
                } else if found.wraps {
                    (value + if up { 1 } else { count - 1 }) % count
                } else if up {
                    (value + 1).min(count - 1)
                } else {
                    value.saturating_sub(1)
                };
                self.set_insert(slot, insert.with_value(param, moved));
            }
            Control::FilterType => {
                // One way at a time, as a large step would skip through a short list.
                let filter = next_in(&FilterType::ALL, track.filter, up);
                self.apply(&Action::SetFilterType {
                    track: self.cursor.track,
                    filter,
                });
            }
            Control::FilterMode => {
                let modes = track.filter.modes();
                if modes.is_empty() {
                    self.status = no_modes(track.filter);
                    return;
                }
                let mode = next_in(modes, track.filter.plays(track.filter_mode), up);
                self.apply(&Action::SetFilterMode {
                    track: self.cursor.track,
                    mode,
                });
            }
            Control::Choke => {
                let from = i64::from(track.choke.unwrap_or(0));
                let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                    .clamp(0, i64::from(CHOKE_GROUPS)) as u8;
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: (moved > 0).then_some(moved),
                });
            }
            Control::Group => {
                let from = i64::from(track.group.unwrap_or(0));
                let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                    .clamp(0, MAX_GROUPS as i64) as u8;
                self.set_group_here((moved > 0).then_some(moved));
            }
            Control::GateLength => {
                let size = if large { GATE_JUMP } else { 1 };
                let by = size * count.min(100) as i32;
                let from = i32::from(gate_percent(track.gate_length()));
                let moved = from + if up { by } else { -by };
                self.apply(&Action::SetTrackGateLength {
                    track: self.cursor.track,
                    gate_length: Some(gate_value(moved.clamp(1, 100) as u8)),
                });
            }
            Control::Glide => {
                let size = if large { HEX_JUMP } else { 1 };
                let by = size * count.min(0x100) as i64;
                let moved = i64::from(track.glide) + if up { by } else { -by };
                self.set_glide(moved.clamp(0, 0xFF) as u8);
            }
            Control::Reset => {
                // A step at a time, or a bar to the next whole bar, with off below 1.
                let from = u32::from(track.reset.unwrap_or(0));
                let (bar, by) = (
                    BAR_STEPS as u32,
                    count.min(usize::from(MAX_TRACK_RESET)) as u32,
                );
                let moved = match (large, up) {
                    (false, true) => from + by,
                    (false, false) => from.saturating_sub(by),
                    (true, true) => (from / bar + by) * bar,
                    (true, false) => from.div_ceil(bar).saturating_sub(by) * bar,
                };
                let moved = moved.min(u32::from(MAX_TRACK_RESET)) as u16;
                self.set_reset((moved > 0).then_some(moved));
            }
            Control::Speed => {
                // One way at a time, stopping at either end, as the list runs short to long.
                let at = usize::from(track.speed.index());
                let speed = Speed::from_index(if up { at + 1 } else { at.saturating_sub(1) });
                self.apply(&Action::SetTrackSpeed {
                    track: self.cursor.track,
                    speed,
                });
            }
            Control::Groove => {
                // One at a time, from what it plays, as a large step would skip through the list.
                let from = track.groove.unwrap_or(self.song().groove).index();
                let last = GrooveId::all().count() - 1;
                let to = if up {
                    (from + 1).min(last)
                } else {
                    from.saturating_sub(1)
                };
                if track.groove.is_none() || to != from {
                    self.set_track_groove(GrooveId::from_index(to));
                }
            }
            Control::Mod(slot, ModField::Type) => {
                self.status = format!(
                    "K picks Mod {}'s type, as a type changes what the device is",
                    slot + 1
                );
            }
            Control::Mod(slot, field) => {
                let mut modulator = track.mods[slot];
                for _ in 0..count.min(0x200) {
                    adjust_mod(&mut modulator, slot, field, up, large, &track);
                }
                self.set_mod(slot, modulator);
            }
            Control::Accumulator(field) => {
                let mut accumulation = track.accumulation;
                for _ in 0..count.min(0x100) {
                    adjust_accumulation(&mut accumulation, field, up, large);
                }
                self.set_accumulation(accumulation);
            }
            Control::Arpeggio(field) => {
                let mut arpeggio = track.arpeggio;
                for _ in 0..count.min(0x10) {
                    adjust_arpeggio(&mut arpeggio, field, up);
                }
                self.set_arpeggio(arpeggio);
            }
        }
    }

    pub(super) fn set_accumulation(&mut self, accumulation: Accumulation) {
        if accumulation != self.page_track().accumulation {
            self.apply(&Action::SetAccumulation {
                track: self.cursor.track,
                accumulation,
            });
        }
    }

    pub(super) fn set_arpeggio(&mut self, arpeggio: Arpeggio) {
        if arpeggio != self.page_track().arpeggio {
            self.apply(&Action::SetArpeggio {
                track: self.cursor.track,
                arpeggio,
            });
        }
    }

    /// Move the fine part of the setting under the cursor, carrying into its value past `FF` or
    /// below `00`, as one undo step.
    pub(super) fn adjust_fine(&mut self, up: bool, count: usize, large: bool) {
        let control = self.track_control();
        let track = self.page_track_as_heard();
        let Some(id) = control.fine_on(&track) else {
            return;
        };
        let Some(coarse) = track.coarse(id) else {
            return;
        };
        // What's set, which a fader the steps drive may not be at.
        let set = self.page_track().coarse(id).unwrap_or(coarse);
        let size = if large { HEX_JUMP } else { 1 } * count as i64;
        let value = i64::from(coarse) << 8 | i64::from(track.fine(id));
        let top = i64::from(id.entry().kind.top()) << 8;
        let moved = (value + if up { size } else { -size }).clamp(0, top);
        let (coarse_now, fine) = ((moved >> 8) as u8, (moved & 0xFF) as u8);
        let index = self.cursor.track;
        let mut actions = Vec::new();
        if coarse_now != set {
            actions.push(match control {
                Control::Base(control) => Action::SetTrackBase {
                    track: index,
                    control,
                    value: coarse_now,
                },
                _ => Action::SetTrackSetting {
                    track: index,
                    setting: id,
                    value: coarse_now,
                },
            });
        }
        actions.push(Action::SetTrackFine {
            track: index,
            setting: id,
            fine,
        });
        self.apply(&Action::Batch(actions));
    }

    /// Set the fine part of the setting under the cursor.
    pub(super) fn set_fine_here(&mut self, fine: u8) {
        if let Some(setting) = self.track_control().fine_on(&self.page_track()) {
            self.apply(&Action::SetTrackFine {
                track: self.cursor.track,
                setting,
                fine,
            });
        }
    }

    /// Put the setting under the cursor back to its default. If it's there already, nothing changes
    /// and no undo step is left.
    pub(super) fn reset_control(&mut self) {
        let control = self.track_control();
        let track = self.page_track();
        let fresh = Track::default();
        match control {
            Control::Base(which) => {
                let value = |track: &Track| match which {
                    TrackControl::Cutoff => track.cutoff,
                    TrackControl::Resonance => track.resonance,
                    TrackControl::Pan => track.pan,
                    TrackControl::SendA => track.send_a,
                    TrackControl::SendB => track.send_b,
                    TrackControl::Drive => track.drive,
                    TrackControl::LowCut => track.low_cut,
                    TrackControl::HighCut => track.high_cut,
                    TrackControl::Pitch => track.pitch,
                    TrackControl::VelocityDelta => track.velocity_delta,
                };
                if value(&track) != value(&fresh) {
                    self.set_hex(control, value(&fresh));
                }
            }
            Control::Mute | Control::Solo => {
                let live = self
                    .levels
                    .get(self.cursor.track)
                    .copied()
                    .unwrap_or_default();
                if (control == Control::Mute && live.mute)
                    || (control == Control::Solo && live.solo)
                {
                    self.toggle_control(control);
                }
            }
            // A new song's is on.
            Control::Limiter if !self.song().limiter => self.set_limiter(true),
            Control::Limiter => {}
            Control::Setting(_) => {
                if let Some(id) = control.setting_on(&track)
                    && track.setting(id) != id.entry().default
                {
                    self.set_hex(control, id.entry().default);
                }
            }
            Control::Machine => {
                if track.machine != fresh.machine {
                    self.apply(&Action::SetTrackMachine {
                        track: self.cursor.track,
                        machine: fresh.machine,
                    });
                }
            }
            Control::Sample if track.sample() != 0 => {
                self.apply(&Action::SetTrackSample {
                    track: self.cursor.track,
                    sample: 0,
                });
            }
            Control::Sample => {}
            Control::Insert(slot) => {
                if track.inserts[slot] != Insert::Empty {
                    self.set_insert(slot, Insert::Empty);
                }
            }
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                if let Some(found) = insert.params().get(param)
                    && insert.value(param) != found.default
                {
                    self.set_insert(slot, insert.with_value(param, found.default));
                }
            }
            Control::Level if track.level != fresh.level => self.set_hex(control, fresh.level),
            Control::Level => {}
            Control::Trim if track.trim != fresh.trim => self.set_hex(control, fresh.trim),
            Control::Trim => {}
            Control::Groove if track.groove.is_some() => self.set_track_groove(None),
            Control::Groove => {}
            Control::Choke if track.choke.is_some() => {
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: None,
                });
            }
            Control::Choke => {}
            Control::Group if track.group.is_some() => self.set_group_here(None),
            Control::Group => {}
            Control::GateLength if track.gate_length.is_some() => {
                self.apply(&Action::SetTrackGateLength {
                    track: self.cursor.track,
                    gate_length: None,
                });
            }
            Control::GateLength => {}
            Control::Glide if track.glide != fresh.glide => self.set_glide(fresh.glide),
            Control::Glide => {}
            Control::Speed if track.speed != fresh.speed => {
                self.apply(&Action::SetTrackSpeed {
                    track: self.cursor.track,
                    speed: fresh.speed,
                });
            }
            Control::Speed => {}
            Control::Reset if track.reset.is_some() => self.set_reset(None),
            Control::Reset => {}
            Control::FilterType => {
                if track.filter != fresh.filter {
                    self.apply(&Action::SetFilterType {
                        track: self.cursor.track,
                        filter: fresh.filter,
                    });
                }
            }
            Control::FilterMode => {
                if track.filter_mode != fresh.filter_mode {
                    self.apply(&Action::SetFilterMode {
                        track: self.cursor.track,
                        mode: fresh.filter_mode,
                    });
                }
            }
            Control::Mod(slot, field) => {
                let modulator = track.mods[slot];
                let reset = mod_at_default(modulator, slot, field);
                if reset != modulator {
                    self.set_mod(slot, reset);
                }
            }
            Control::Accumulator(field) => {
                let (now, default) = (track.accumulation, Accumulation::default());
                let reset = match field {
                    AccumulatorField::Up => Accumulation {
                        up: default.up,
                        ..now
                    },
                    AccumulatorField::Down => Accumulation {
                        down: default.down,
                        ..now
                    },
                    AccumulatorField::Moves => Accumulation {
                        moves: default.moves,
                        ..now
                    },
                    AccumulatorField::AtLimit => Accumulation {
                        at_limit: default.at_limit,
                        ..now
                    },
                    AccumulatorField::Adds => Accumulation {
                        adds: default.adds,
                        ..now
                    },
                    AccumulatorField::Reset => Accumulation {
                        reset: default.reset,
                        ..now
                    },
                };
                self.set_accumulation(reset);
            }
            Control::Arpeggio(field) => {
                let (now, default) = (track.arpeggio, Arpeggio::default());
                let reset = match field {
                    ArpField::Rate => Arpeggio {
                        rate: default.rate,
                        ..now
                    },
                    ArpField::Mode => Arpeggio {
                        mode: default.mode,
                        ..now
                    },
                    ArpField::Octaves => Arpeggio {
                        octaves: default.octaves,
                        ..now
                    },
                    ArpField::Retrig => Arpeggio {
                        retrig: default.retrig,
                        ..now
                    },
                };
                self.set_arpeggio(reset);
            }
        }
    }

    /// List what an LFO's setting can be, with the current one highlighted.
    /// Switch this track's mute or solo.
    /// The bank entry so many on from this one, up or down, through those the bank has, with
    /// `Off` below the first. It stops at either end.
    fn next_sample(&self, from: u8, up: bool, count: usize) -> u8 {
        let numbers = std::iter::once(0).chain(self.song().bank.iter().map(|(number, _)| number));
        let found = if up {
            numbers.filter(|number| *number > from).take(count).last()
        } else {
            let below: Vec<u8> = numbers.filter(|number| *number < from).collect();
            below.get(below.len().saturating_sub(count)).copied()
        };
        found.unwrap_or(from)
    }

    /// Switch the track's mute or solo, or the song's limiter.
    pub(super) fn toggle_control(&mut self, control: Control) {
        match control {
            Control::Mute => self.toggle_mute(),
            Control::Limiter => self.set_limiter(!self.song().limiter),
            _ => self.toggle_solo(),
        }
    }

    /// Switch the song's limiter on or off, as `:set limiter` does, saying what that means.
    fn set_limiter(&mut self, on: bool) {
        self.line(Line::SetLimiter(on));
    }
}
