//! Changing the setting under the cursor: `=` and `-` and their large steps, the fine digits, and
//! `x` back to its default. Each is a function over the [`Services`], given the setting, as the
//! track page and the mixer both turn them.

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::Action;
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES};
use vt_model::constants::{BAR_STEPS, MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::modulation::{ModKind, ModSlot, default_mod};
use vt_model::settings::{SettingId, gate_percent, gate_value};
use vt_model::song::{FilterType, Machine, Role, Track};
use vt_model::speed::Speed;
use vt_model::tracking::Source;

use crate::services::Services;

use super::controls::no_modes;
use super::fx::set_insert;
use super::tuning::{adjust_tuning, set_tuning};
use super::{AccumulatorField, ArpField, Control, ModField};

/// How far a small and a large step move a phase, in degrees.
const PHASE_NUDGE: i32 = 15;

const PHASE_JUMP: i32 = 90;
/// An attractor's large step, from one axis to the next: `x` at 0°, `y` at 120° and `z` at 240°.
const AXIS_JUMP: i32 = 120;

/// How far a large step moves `Gate`, in percent.
const GATE_JUMP: i32 = 10;

/// How many small steps a large one is for a value in hex.
const HEX_JUMP: i64 = 0x10;

/// How many small steps a large one is for an envelope time.
const TIME_JUMP: u16 = 10;

/// How far a large step moves a sustain, in percent.
const SUSTAIN_JUMP: i32 = 10;

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
/// alone, as it changes what the device is. A list or a switch goes round, as a phase does, and a
/// number stops at its ends.
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
    match field {
        ModField::Type => return,
        // `Pitch` comes twice, as it is and in scale.
        ModField::Destination => {
            let mut choices = vec![(None, false)];
            for destination in SettingId::for_mod(track, slot) {
                choices.push((Some(destination), false));
                if destination == SettingId::PITCH {
                    choices.push((Some(destination), true));
                }
            }
            let here = (modulator.destination, modulator.goes_in_scale());
            (modulator.destination, modulator.in_scale) = next_in(&choices, here, up);
            return;
        }
        // A rate and a fade step through their own kinds.
        ModField::Rate => {
            let (steps, lfo) = (&mut modulator.steps, &mut modulator.lfo);
            match (modulator.kind, large) {
                (ModKind::Steps, _) => steps.rate = steps.rate.nudged(up, large),
                (_, true) => lfo.rate = lfo.rate.jumped(up),
                (_, false) => lfo.rate = lfo.rate.nudged(up),
            }
            return;
        }
        ModField::Fade => {
            modulator.lfo.fade = modulator.lfo.fade.nudged(up, large);
            return;
        }
        _ => {}
    }
    let (Some(value), Some((lowest, highest))) = (modulator.field(field), modulator.bounds(field))
    else {
        return;
    };
    let by = |small: i32, jump: i32| {
        let size = if large { jump } else { small };
        if up { size } else { -size }
    };
    let to = match field {
        ModField::Mode
        | ModField::Polarity
        | ModField::Reset
        | ModField::Shape
        | ModField::Loop => lowest + (value - lowest + by(1, 1)).rem_euclid(highest - lowest + 1),
        ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => {
            i32::from(time(value as u16))
        }
        // `Note` is below no time, so `-` from 0 ms holds for the note.
        ModField::Hold => match value {
            -1 if up => 0,
            -1 | 0 if !up => -1,
            ms => i32::from(time(ms as u16)),
        },
        // An attractor's large step goes from one axis to the next.
        ModField::Phase => {
            let jump = if modulator.lfo.shape.is_attractor() {
                AXIS_JUMP
            } else {
                PHASE_JUMP
            };
            value + by(PHASE_NUDGE, jump)
        }
        ModField::Body | ModField::Sustain => value + by(1, SUSTAIN_JUMP),
        ModField::Mutate => value + by(1, MUTATE_JUMP),
        ModField::Ratchet | ModField::Length | ModField::Start => value + by(1, 1),
        // A large step is an octave, as a step's value on `Pitch` is semitones.
        ModField::Step(_) => value + by(1, OCTAVE),
        // A large step on a map of notes is an octave.
        ModField::Low | ModField::High if modulator.track.source == Source::Note => {
            value + by(1, OCTAVE)
        }
        // The rest are in hex, a large step `10`: a depth, a colour, a deform, Smooth, and a map
        // of velocities or rolls.
        _ => value + by(1, HEX_JUMP as i32),
    };
    modulator.set_field(field, to);
}

/// How far a large step moves one of Steps' values: an octave, `0C`.
const OCTAVE: i32 = 12;

/// How far a large step moves `Mutate`, in percent.
const MUTATE_JUMP: i32 = 10;

/// A mod slot with one setting back at its default: the slot's own for what every type shares,
/// and for a type's own the default that type has in this slot. `Type` goes back to the slot's
/// first type, with that type's settings as they start, keeping where it goes, how far and its
/// switch, as choosing a type does.
fn mod_at_default(modulator: ModSlot, slot: usize, field: ModField) -> ModSlot {
    let fresh = default_mod(slot);
    match field {
        ModField::Type => {
            return ModSlot {
                kind: fresh.kind,
                adsr: if fresh.kind == ModKind::Adsr {
                    fresh.adsr
                } else {
                    modulator.adsr
                },
                lfo: if fresh.kind == ModKind::Lfo {
                    fresh.lfo
                } else {
                    modulator.lfo
                },
                ..modulator
            };
        }
        ModField::Destination => {
            return ModSlot {
                destination: fresh.destination,
                in_scale: false,
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
        // A rate isn't one number, as it's a division, steps or hertz.
        ModField::Rate => {
            let mut reset = modulator;
            if modulator.kind == ModKind::Steps {
                reset.steps.rate = fresh.steps.rate;
            } else {
                reset.lfo.rate = fresh.lfo.rate;
            }
            return reset;
        }
        _ => {}
    }
    // The slot as it starts under this type, with Track's map of the source it follows.
    let mut default = ModSlot {
        kind: modulator.kind,
        ..fresh
    };
    if matches!(field, ModField::Low | ModField::High) {
        default.track.source = modulator.track.source;
    }
    let mut reset = modulator;
    if let Some(value) = default.field(field) {
        reset.set_field(field, value);
    }
    reset
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

/// Put the track in a group, or take it out of any.
pub(super) fn set_group_here(services: &mut Services, group: Option<u8>) {
    services.apply(&Action::SetTrackGroup {
        first: services.cursor.track,
        last: services.cursor.track,
        group,
    });
}

/// Replace one of the track's mod slots, as one undo step.
pub(super) fn set_mod(services: &mut Services, slot: usize, modulator: ModSlot) {
    services.apply(&Action::SetMod {
        track: services.cursor.track,
        slot,
        modulator,
    });
}

/// Set how many of the song's steps the track's chain plays before starting again, or none.
pub(super) fn set_reset(services: &mut Services, reset: Option<u16>) {
    services.apply(&Action::SetTrackReset {
        track: services.cursor.track,
        reset,
    });
}

/// Set how long the track's ties glide into the next note.
pub(super) fn set_glide(services: &mut Services, glide: u8) {
    services.apply(&Action::SetTrackGlide {
        track: services.cursor.track,
        glide,
    });
}

pub(super) fn set_hex(services: &mut Services, control: Control, value: u8) {
    let track = services.cursor.track;
    // On the mixer, the cursor can be on a setting this track doesn't have.
    let tracks = &services.song().tracks;
    if tracks.get(track).is_some_and(|t| !control.exists_on(t)) {
        return;
    }
    if let Some(setting) = tracks.get(track).and_then(|t| control.setting_on(t)) {
        services.apply(&Action::SetTrackSetting {
            track,
            setting,
            value,
        });
        return;
    }
    match control {
        Control::Base(control) => services.apply(&Action::SetTrackBase {
            track,
            control,
            value,
        }),
        Control::Level => services.apply(&Action::SetTrackLevel {
            track,
            level: value,
        }),
        Control::Trim => services.apply(&Action::SetTrackTrim { track, trim: value }),
        _ => {}
    }
}

pub(super) fn adjust_control(
    services: &mut Services,
    control: Control,
    up: bool,
    count: usize,
    large: bool,
) {
    // A fader the steps drive moves from where it's heard, as a hand takes it over there.
    let track = services.page_track_as_heard();
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
        // The Gen page's own keys reshape its generators (`generators::run_gen_page`).
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Mute | Control::Solo => {
            let live = services
                .levels
                .get(services.cursor.track)
                .copied()
                .unwrap_or_default();
            let on = if control == Control::Mute {
                live.mute
            } else {
                live.solo
            };
            if on != up {
                toggle_control(services, control);
            }
        }
        // `=` puts it on and `-` takes it off, as on the project page it was on.
        Control::Limiter if services.song().limiter != up => set_limiter(services, up),
        Control::Limiter => {}
        Control::Safe if track.safe != up => set_safe(services, up),
        Control::Safe => {}
        Control::Setting(_) => {
            if let Some(id) = control.setting_on(&track) {
                set_hex(services, control, change(track.setting(id)));
            }
        }
        Control::Machine => {
            let machine = next_in(&Machine::ALL, track.machine, up);
            services.apply(&Action::SetTrackMachine {
                track: services.cursor.track,
                machine,
            });
        }
        Control::Sample => {
            let size = if large { HEX_JUMP as usize } else { 1 };
            let sample = next_sample(services, track.sample(), up, size * count);
            services.apply(&Action::SetTrackSample {
                track: services.cursor.track,
                sample,
            });
        }
        Control::Tune | Control::Cents => adjust_tuning(services, control, up, count, large),
        Control::Base(which) => set_hex(services, control, change(track.control(which))),
        Control::Level => set_hex(services, control, change(track.level)),
        Control::Trim => set_hex(services, control, change(track.trim)),
        Control::Insert(slot) => set_insert(services, slot, track.inserts[slot].stepped(up)),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let Some(found) = insert.params().get(param) else {
                return;
            };
            let value = insert.value(param);
            let count = found.choices().len() as u8;
            let moved = if found.choices().is_empty() {
                change(value)
            } else if found.wraps() {
                (value + if up { 1 } else { count - 1 }) % count
            } else if up {
                (value + 1).min(count - 1)
            } else {
                value.saturating_sub(1)
            };
            set_insert(services, slot, insert.with_value(param, moved));
        }
        Control::FilterType => {
            // One way at a time, as a large step would skip through a short list.
            let filter = next_in(&FilterType::ALL, track.filter, up);
            services.apply(&Action::SetFilterType {
                track: services.cursor.track,
                filter,
            });
        }
        Control::FilterMode => {
            let modes = track.filter.modes();
            if modes.is_empty() {
                services.status = no_modes(track.filter);
                return;
            }
            let mode = next_in(modes, track.filter.plays(track.filter_mode), up);
            services.apply(&Action::SetFilterMode {
                track: services.cursor.track,
                mode,
            });
        }
        Control::Choke => {
            let from = i64::from(track.choke.unwrap_or(0));
            let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                .clamp(0, i64::from(CHOKE_GROUPS)) as u8;
            services.apply(&Action::SetTrackChoke {
                track: services.cursor.track,
                choke: (moved > 0).then_some(moved),
            });
        }
        Control::Group => {
            let from = i64::from(track.group.unwrap_or(0));
            let moved = (from + if up { 1 } else { -1 } * count.min(0x100) as i64)
                .clamp(0, MAX_GROUPS as i64) as u8;
            set_group_here(services, (moved > 0).then_some(moved));
        }
        Control::GateLength => {
            let size = if large { GATE_JUMP } else { 1 };
            let by = size * count.min(100) as i32;
            let from = i32::from(gate_percent(track.gate_length()));
            let moved = from + if up { by } else { -by };
            services.apply(&Action::SetTrackGateLength {
                track: services.cursor.track,
                gate_length: Some(gate_value(moved.clamp(1, 100) as u8)),
            });
        }
        Control::Glide => {
            let size = if large { HEX_JUMP } else { 1 };
            let by = size * count.min(0x100) as i64;
            let moved = i64::from(track.glide) + if up { by } else { -by };
            set_glide(services, moved.clamp(0, 0xFF) as u8);
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
            set_reset(services, (moved > 0).then_some(moved));
        }
        Control::Role => {
            let role = next_in(&Role::ALL, track.role, up);
            services.apply(&Action::SetTrackRole {
                track: services.cursor.track,
                role,
            });
        }
        Control::Speed => {
            // One way at a time, stopping at either end, as the list runs short to long.
            let at = usize::from(track.speed.index());
            let speed = Speed::from_index(if up { at + 1 } else { at.saturating_sub(1) });
            services.apply(&Action::SetTrackSpeed {
                track: services.cursor.track,
                speed,
            });
        }
        Control::Groove => {
            // One at a time, from what it plays, as a large step would skip through the list.
            let from = track.groove.unwrap_or(services.song().groove).index();
            let last = GrooveId::all().count() - 1;
            let to = if up {
                (from + 1).min(last)
            } else {
                from.saturating_sub(1)
            };
            if track.groove.is_none() || to != from {
                set_track_groove(services, GrooveId::from_index(to));
            }
        }
        Control::Mod(slot, ModField::Type) => {
            services.status = format!(
                "K picks Mod {}'s type, as a type changes what the device is",
                slot + 1
            );
        }
        Control::Mod(slot, field) => {
            let mut modulator = track.mods[slot];
            for _ in 0..count.min(0x200) {
                adjust_mod(&mut modulator, slot, field, up, large, &track);
            }
            set_mod(services, slot, modulator);
        }
        Control::Accumulator(field) => {
            let mut accumulation = track.accumulation;
            for _ in 0..count.min(0x100) {
                adjust_accumulation(&mut accumulation, field, up, large);
            }
            set_accumulation(services, accumulation);
        }
        Control::Arpeggio(field) => {
            let mut arpeggio = track.arpeggio;
            for _ in 0..count.min(0x10) {
                adjust_arpeggio(&mut arpeggio, field, up);
            }
            set_arpeggio(services, arpeggio);
        }
        // A coarse step a turn, as a cutoff's, keeping any fine part the Roto left.
        Control::Macro(at) => {
            let size = if large { HEX_JUMP } else { 1 } * count.min(0x100) as i64;
            let from = i64::from(track.macros[at].value);
            let by = if up { size } else { -size } * 0x100;
            let moved = (from + by).clamp(0, 0xFF00);
            set_macro(services, at, moved as u16);
        }
    }
}

/// Turn one of the track's macros, and with it everything it holds.
pub(crate) fn set_macro(services: &mut Services, at: usize, value: u16) {
    if services.page_track().macros[at].value != value {
        services.apply(&Action::SetMacro {
            track: services.cursor.track,
            at,
            value,
        });
    }
}

pub(super) fn set_accumulation(services: &mut Services, accumulation: Accumulation) {
    if accumulation != services.page_track().accumulation {
        services.apply(&Action::SetAccumulation {
            track: services.cursor.track,
            accumulation,
        });
    }
}

pub(super) fn set_arpeggio(services: &mut Services, arpeggio: Arpeggio) {
    if arpeggio != services.page_track().arpeggio {
        services.apply(&Action::SetArpeggio {
            track: services.cursor.track,
            arpeggio,
        });
    }
}

/// Move the fine part of the setting under the cursor, carrying into its value past `FF` or
/// below `00`, as one undo step.
pub(super) fn adjust_fine(
    services: &mut Services,
    control: Control,
    up: bool,
    count: usize,
    large: bool,
) {
    let track = services.page_track_as_heard();
    let Some(id) = control.fine_on(&track) else {
        return;
    };
    let Some(coarse) = track.coarse(id) else {
        return;
    };
    // What's set, which a fader the steps drive may not be at.
    let set = services.page_track().coarse(id).unwrap_or(coarse);
    let size = if large { HEX_JUMP } else { 1 } * count as i64;
    let value = i64::from(coarse) << 8 | i64::from(track.fine(id));
    let top = i64::from(id.entry().kind.top()) << 8;
    let moved = (value + if up { size } else { -size }).clamp(0, top);
    let (coarse_now, fine) = ((moved >> 8) as u8, (moved & 0xFF) as u8);
    let index = services.cursor.track;
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
    services.apply(&Action::Batch(actions));
}

/// Set the fine part of the setting under the cursor.
pub(super) fn set_fine_here(services: &mut Services, control: Control, fine: u8) {
    if let Some(setting) = control.fine_on(&services.page_track()) {
        services.apply(&Action::SetTrackFine {
            track: services.cursor.track,
            setting,
            fine,
        });
    }
}

/// Put the setting under the cursor back to its default. If it's there already, nothing changes
/// and no undo step is left.
pub(super) fn reset_control(services: &mut Services, control: Control) {
    let track = services.page_track();
    let fresh = Track::default();
    match control {
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Base(which) => {
            if track.control(which) != fresh.control(which) {
                set_hex(services, control, fresh.control(which));
            }
        }
        Control::Macro(at) => set_macro(services, at, 0),
        Control::Mute | Control::Solo => {
            let live = services
                .levels
                .get(services.cursor.track)
                .copied()
                .unwrap_or_default();
            if (control == Control::Mute && live.mute) || (control == Control::Solo && live.solo) {
                toggle_control(services, control);
            }
        }
        // A new song's is on.
        Control::Limiter if !services.song().limiter => set_limiter(services, true),
        Control::Limiter => {}
        Control::Safe if track.safe => set_safe(services, false),
        Control::Safe => {}
        Control::Setting(_) => {
            if let Some(id) = control.setting_on(&track)
                && track.setting(id) != id.entry().default
            {
                set_hex(services, control, id.entry().default);
            }
        }
        Control::Machine => {
            if track.machine != fresh.machine {
                services.apply(&Action::SetTrackMachine {
                    track: services.cursor.track,
                    machine: fresh.machine,
                });
            }
        }
        Control::Sample if track.sample() != 0 => {
            services.apply(&Action::SetTrackSample {
                track: services.cursor.track,
                sample: 0,
            });
        }
        Control::Sample => {}
        Control::Tune | Control::Cents => set_tuning(services, control, None),
        Control::Insert(slot) => {
            if track.inserts[slot] != Insert::Empty {
                set_insert(services, slot, Insert::Empty);
            }
        }
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            if let Some(found) = insert.params().get(param)
                && insert.value(param) != found.default
            {
                set_insert(services, slot, insert.with_value(param, found.default));
            }
        }
        Control::Level if track.level != fresh.level => set_hex(services, control, fresh.level),
        Control::Level => {}
        Control::Trim if track.trim != fresh.trim => set_hex(services, control, fresh.trim),
        Control::Trim => {}
        Control::Groove if track.groove.is_some() => set_track_groove(services, None),
        Control::Groove => {}
        Control::Choke if track.choke.is_some() => {
            services.apply(&Action::SetTrackChoke {
                track: services.cursor.track,
                choke: None,
            });
        }
        Control::Choke => {}
        Control::Group if track.group.is_some() => set_group_here(services, None),
        Control::Group => {}
        Control::Role if !track.role.is_auto() => {
            services.apply(&Action::SetTrackRole {
                track: services.cursor.track,
                role: Role::Auto,
            });
        }
        Control::Role => {}
        Control::GateLength if track.gate_length.is_some() => {
            services.apply(&Action::SetTrackGateLength {
                track: services.cursor.track,
                gate_length: None,
            });
        }
        Control::GateLength => {}
        Control::Glide if track.glide != fresh.glide => set_glide(services, fresh.glide),
        Control::Glide => {}
        Control::Speed if track.speed != fresh.speed => {
            services.apply(&Action::SetTrackSpeed {
                track: services.cursor.track,
                speed: fresh.speed,
            });
        }
        Control::Speed => {}
        Control::Reset if track.reset.is_some() => set_reset(services, None),
        Control::Reset => {}
        Control::FilterType => {
            if track.filter != fresh.filter {
                services.apply(&Action::SetFilterType {
                    track: services.cursor.track,
                    filter: fresh.filter,
                });
            }
        }
        Control::FilterMode => {
            if track.filter_mode != fresh.filter_mode {
                services.apply(&Action::SetFilterMode {
                    track: services.cursor.track,
                    mode: fresh.filter_mode,
                });
            }
        }
        Control::Mod(slot, field) => {
            let modulator = track.mods[slot];
            let reset = mod_at_default(modulator, slot, field);
            if reset != modulator {
                set_mod(services, slot, reset);
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
            set_accumulation(services, reset);
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
            set_arpeggio(services, reset);
        }
    }
}

/// List what an LFO's setting can be, with the current one highlighted.
/// Switch this track's mute or solo.
/// The bank entry so many on from this one, up or down, through those the bank has, with
/// `Off` below the first. It stops at either end.
fn next_sample(services: &Services, from: u8, up: bool, count: usize) -> u8 {
    let numbers = std::iter::once(0).chain(services.song().bank.iter().map(|(number, _)| number));
    let found = if up {
        numbers.filter(|number| *number > from).take(count).last()
    } else {
        let below: Vec<u8> = numbers.filter(|number| *number < from).collect();
        below.get(below.len().saturating_sub(count)).copied()
    };
    found.unwrap_or(from)
}

/// Switch the track's mute, solo or filter's `Safe`, or the song's limiter.
pub(super) fn toggle_control(services: &mut Services, control: Control) {
    match control {
        Control::Mute => services.toggle_mute(),
        Control::Limiter => set_limiter(services, !services.song().limiter),
        Control::Safe => set_safe(services, !services.page_track().safe),
        _ => services.toggle_solo(),
    }
}

/// Switch the track filter's `Safe` on or off.
fn set_safe(services: &mut Services, on: bool) {
    services.apply(&Action::SetTrackSafe {
        track: services.cursor.track,
        on,
    });
}

/// Switch the song's limiter on or off, as `:set limiter` does, saying what that means.
pub(crate) fn set_limiter(services: &mut Services, on: bool) {
    services.apply(&Action::SetLimiter(on));
    services.status = if on {
        "Limiter on".to_owned()
    } else {
        "Limiter off: renders keep levels over 0, and the output still clips at 0".to_owned()
    };
}

/// Give the cursor's track its own groove, or have it follow the project's again.
pub(crate) fn set_track_groove(services: &mut Services, groove: Option<GrooveId>) {
    let track = services.cursor.track;
    if !services
        .song()
        .tracks
        .get(track)
        .is_some_and(|t| t.is_sound())
    {
        services.status = "Only a track with notes has a groove of its own".to_owned();
        return;
    }
    services.apply(&Action::SetTrackGroove { track, groove });
    services.status = match groove {
        Some(groove) => format!("This track plays with {groove}"),
        None => format!(
            "This track follows the project's {}",
            services.song().groove
        ),
    };
}
