//! TEMPORARY (U8, #739): the track page's edits as they were made before `param::edit`, kept
//! only to check the new path sets every setting as `=`, `-`, `+`, `_`, `x`, `i` and `K` did,
//! with the same lists, the same chosen item and the same status line. Removed with the old path
//! in the next commit.

#![allow(clippy::all, dead_code, unused_imports)]

mod old_editing {

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

    use crate::track_page::controls::no_modes;
    use crate::track_page::fx::set_insert;
    use crate::track_page::tuning::{adjust_tuning, set_tuning};
    use crate::track_page::{AccumulatorField, ArpField, Control, ModField};

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
    pub(crate) fn next_in<T: Copy + PartialEq>(items: &[T], current: T, up: bool) -> T {
        let position = items.iter().position(|item| *item == current).unwrap_or(0);
        let step = if up { 1 } else { items.len() - 1 };
        items[(position + step) % items.len()]
    }

    /// How far a small step moves an envelope time. Short times need finer steps than long ones.
    pub(crate) fn time_step(value: u16) -> u16 {
        match value {
            0..=9 => 1,
            10..=99 => 5,
            _ => 25,
        }
    }

    /// Move one setting of a mod slot one notch, under its type. The type itself is chosen with `K`
    /// alone, as it changes what the device is. A list or a switch goes round, as a phase does, and a
    /// number stops at its ends.
    pub(crate) fn adjust_mod(
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
        let (Some(value), Some((lowest, highest))) =
            (modulator.field(field), modulator.bounds(field))
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
            | ModField::Loop => {
                lowest + (value - lowest + by(1, 1)).rem_euclid(highest - lowest + 1)
            }
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
    pub(crate) fn mod_at_default(modulator: ModSlot, slot: usize, field: ModField) -> ModSlot {
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
    pub(crate) fn adjust_accumulation(
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
            AccumulatorField::Adds => {
                accumulation.adds = next_in(&Adds::ALL, accumulation.adds, up)
            }
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
    pub(crate) fn adjust_arpeggio(arpeggio: &mut Arpeggio, field: ArpField, up: bool) {
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
    pub(crate) fn set_group_here(services: &mut Services, group: Option<u8>) {
        services.apply(&Action::SetTrackGroup {
            first: services.cursor.track,
            last: services.cursor.track,
            group,
        });
    }

    /// Replace one of the track's mod slots, as one undo step.
    pub(crate) fn set_mod(services: &mut Services, slot: usize, modulator: ModSlot) {
        services.apply(&Action::SetMod {
            track: services.cursor.track,
            slot,
            modulator,
        });
    }

    /// Set how many of the song's steps the track's chain plays before starting again, or none.
    pub(crate) fn set_reset(services: &mut Services, reset: Option<u16>) {
        services.apply(&Action::SetTrackReset {
            track: services.cursor.track,
            reset,
        });
    }

    /// Set how long the track's ties glide into the next note.
    pub(crate) fn set_glide(services: &mut Services, glide: u8) {
        services.apply(&Action::SetTrackGlide {
            track: services.cursor.track,
            glide,
        });
    }

    pub(crate) fn set_hex(services: &mut Services, control: Control, value: u8) {
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

    pub(crate) fn adjust_control(
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

    pub(crate) fn set_accumulation(services: &mut Services, accumulation: Accumulation) {
        if accumulation != services.page_track().accumulation {
            services.apply(&Action::SetAccumulation {
                track: services.cursor.track,
                accumulation,
            });
        }
    }

    pub(crate) fn set_arpeggio(services: &mut Services, arpeggio: Arpeggio) {
        if arpeggio != services.page_track().arpeggio {
            services.apply(&Action::SetArpeggio {
                track: services.cursor.track,
                arpeggio,
            });
        }
    }

    /// Move the fine part of the setting under the cursor, carrying into its value past `FF` or
    /// below `00`, as one undo step.
    pub(crate) fn adjust_fine(
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
    pub(crate) fn set_fine_here(services: &mut Services, control: Control, fine: u8) {
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
    pub(crate) fn reset_control(services: &mut Services, control: Control) {
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
                if (control == Control::Mute && live.mute)
                    || (control == Control::Solo && live.solo)
                {
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
    pub(crate) fn next_sample(services: &Services, from: u8, up: bool, count: usize) -> u8 {
        let numbers =
            std::iter::once(0).chain(services.song().bank.iter().map(|(number, _)| number));
        let found = if up {
            numbers.filter(|number| *number > from).take(count).last()
        } else {
            let below: Vec<u8> = numbers.filter(|number| *number < from).collect();
            below.get(below.len().saturating_sub(count)).copied()
        };
        found.unwrap_or(from)
    }

    /// Switch the track's mute, solo or filter's `Safe`, or the song's limiter.
    pub(crate) fn toggle_control(services: &mut Services, control: Control) {
        match control {
            Control::Mute => services.toggle_mute(),
            Control::Limiter => set_limiter(services, !services.song().limiter),
            Control::Safe => set_safe(services, !services.page_track().safe),
            _ => services.toggle_solo(),
        }
    }

    /// Switch the track filter's `Safe` on or off.
    pub(crate) fn set_safe(services: &mut Services, on: bool) {
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
}

mod old_typing {

    use vt_model::accumulator::{Accumulation, MOST_DEGREES};
    use vt_model::action::Action;
    use vt_model::arpeggio::{Arpeggio, MOST_OCTAVES};
    use vt_model::constants::{MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
    use vt_model::envelope::{MAX_HOLD_MS, MAX_RATCHET};
    use vt_model::kit::CHOKE_GROUPS;
    use vt_model::lfo::{Fade, Lfo, Rate};
    use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot};
    use vt_model::settings::{LONGEST_GLIDE_MS, gate_value, glide_value};
    use vt_model::song::reset_named;
    use vt_model::step::Note;
    use vt_model::steps::{MAX_MUTATE, STEPS, StepRate, Steps};
    use vt_model::tracking::Source;

    use crate::services::Services;

    use super::old_editing::{
        set_accumulation, set_arpeggio, set_fine_here, set_glide, set_group_here, set_hex,
        set_macro, set_mod, set_reset,
    };
    use crate::track_page::fx::set_insert;
    use crate::track_page::tuning::type_tuning;
    use crate::track_page::{AccumulatorField, ArpField, Control, ModField};

    /// Read a depth, with an optional sign, in hex.
    fn parse_depth(text: &str) -> Option<i16> {
        let (sign, digits) = match text.strip_prefix('-') {
            Some(digits) => (-1, digits),
            None => (1, text.strip_prefix('+').unwrap_or(text)),
        };
        let depth = i16::from_str_radix(digits, 16).ok()?;
        (0..=MAX_DEPTH).contains(&depth).then_some(sign * depth)
    }

    /// Write what was typed into one of the track's settings.
    pub(crate) fn commit_control(services: &mut Services, control: Control, text: &str) {
        let track = services.page_track();
        match control {
            // A generator's are typed into its shape, by `App::commit_typed`.
            Control::Gen { .. } | Control::NewGenerator(_) => {}
            Control::Mute | Control::Solo | Control::Limiter | Control::Safe => {}
            Control::Sample => {}
            Control::Tune | Control::Cents => type_tuning(services, control, text),
            Control::Insert(_) => {}
            Control::InsertParam(slot, param) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    let insert = track.inserts[slot];
                    set_insert(services, slot, insert.with_value(param, value));
                }
            }
            Control::Base(_) | Control::Level | Control::Trim | Control::Setting(_) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    set_hex(services, control, value);
                }
            }
            Control::Choke if text.eq_ignore_ascii_case("off") => {
                services.apply(&Action::SetTrackChoke {
                    track: services.cursor.track,
                    choke: None,
                });
            }
            Control::Choke => match text.parse::<u8>() {
                Ok(group) if (1..=CHOKE_GROUPS).contains(&group) => {
                    services.apply(&Action::SetTrackChoke {
                        track: services.cursor.track,
                        choke: Some(group),
                    });
                }
                _ => services.status = format!("Choke is a group from 1 to {CHOKE_GROUPS}, or off"),
            },
            Control::Group if text.eq_ignore_ascii_case("off") => set_group_here(services, None),
            Control::Group => {
                let number = text.trim_start_matches(['G', 'g']).parse::<u8>();
                match number {
                    Ok(group) if (1..=MAX_GROUPS).contains(&usize::from(group)) => {
                        set_group_here(services, Some(group));
                    }
                    _ => {
                        services.status = format!("Group is a group from 1 to {MAX_GROUPS}, or off")
                    }
                }
            }
            Control::GateLength => match text.trim_end_matches('%').trim().parse::<u8>() {
                Ok(percent) if (1..=100).contains(&percent) => {
                    services.apply(&Action::SetTrackGateLength {
                        track: services.cursor.track,
                        gate_length: Some(gate_value(percent)),
                    });
                }
                _ => services.status = "Gate is a percent from 1 to 100".to_owned(),
            },
            Control::Glide if text.eq_ignore_ascii_case("off") => set_glide(services, 0),
            Control::Glide => match text
                .trim_end_matches("ms")
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|ms| (0.0..=LONGEST_GLIDE_MS).contains(ms))
            {
                Some(ms) => set_glide(services, glide_value(ms)),
                None => {
                    services.status =
                        format!("Glide is a time in ms up to {LONGEST_GLIDE_MS}, or off");
                }
            },
            Control::Reset => match reset_named(text) {
                Some(reset) => set_reset(services, reset),
                None => {
                    services.status = format!(
                        "Reset is a number of steps from 1 to {MAX_TRACK_RESET}, of bars like 2bar, or off"
                    );
                }
            },
            Control::Groove | Control::Speed | Control::Role => {}
            Control::FilterType | Control::FilterMode | Control::Machine => {}
            Control::Accumulator(field @ (AccumulatorField::Up | AccumulatorField::Down)) => {
                match text.trim().parse::<u8>() {
                    Ok(limit) if (1..=MOST_DEGREES).contains(&limit) => {
                        let now = track.accumulation;
                        set_accumulation(
                            services,
                            if field == AccumulatorField::Up {
                                Accumulation { up: limit, ..now }
                            } else {
                                Accumulation { down: limit, ..now }
                            },
                        );
                    }
                    _ => {
                        services.status =
                            format!("A limit is a number of degrees from 1 to {MOST_DEGREES}");
                    }
                }
            }
            Control::Accumulator(_) => {}
            Control::Arpeggio(ArpField::Octaves) => match text.trim().parse::<u8>() {
                Ok(octaves) if (1..=MOST_OCTAVES).contains(&octaves) => {
                    set_arpeggio(
                        services,
                        Arpeggio {
                            octaves,
                            ..track.arpeggio
                        },
                    );
                }
                _ => services.status = format!("Octaves is a number from 1 to {MOST_OCTAVES}"),
            },
            Control::Arpeggio(_) => {}
            Control::Mod(slot, field) => type_mod(services, slot, field, text),
            Control::Macro(at) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    set_macro(services, at, u16::from(value) << 8);
                }
            }
        }
    }

    /// Set a mod slot's setting from what was typed: a depth in signed hex, an envelope's time,
    /// sustain or hold, an LFO's rate or phase, Steps' own, or where Track's map starts and ends, a
    /// note's name or hex as its source has them. The rest are lists, chosen with `K`.
    fn type_mod(services: &mut Services, slot: usize, field: ModField, text: &str) {
        let current = services.page_track().mods[slot];
        // A rate and a fade are read as they're written, as they aren't one number.
        let written = match field {
            ModField::Rate if current.kind == ModKind::Steps => Some(
                text.parse::<StepRate>()
                    .map(|rate| {
                        let steps = Steps {
                            rate,
                            ..current.steps
                        };
                        ModSlot { steps, ..current }
                    })
                    .map_err(|error| error.to_string()),
            ),
            ModField::Rate => Some(
                text.parse::<Rate>()
                    .map(|rate| {
                        let lfo = Lfo {
                            rate,
                            ..current.lfo
                        };
                        ModSlot { lfo, ..current }
                    })
                    .map_err(|error| error.to_string()),
            ),
            ModField::Fade => Some(
                text.parse::<Fade>()
                    .map(|fade| {
                        let lfo = Lfo {
                            fade,
                            ..current.lfo
                        };
                        ModSlot { lfo, ..current }
                    })
                    .map_err(|error| error.to_string()),
            ),
            _ => None,
        };
        if let Some(written) = written {
            match written {
                Ok(modulator) => set_mod(services, slot, modulator),
                Err(why) => services.status = why,
            }
            return;
        }
        // The rest are numbers, read as the slot keeps them.
        let percent = |text: &str| text.trim_end_matches('%').parse::<u8>().ok().map(i32::from);
        let count = |text: &str| text.trim().parse::<u8>().ok().map(i32::from);
        let signed = |text: &str| parse_depth(text).map(i32::from);
        let times = format!("Times are milliseconds from 0 to {MAX_ENVELOPE_MS}");
        let (value, why) = match field {
            ModField::Depth => (signed(text), "Depth is hex from -FF to +FF".to_owned()),
            ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => {
                (text.parse::<u16>().ok().map(i32::from), times)
            }
            ModField::Colour => (
                u8::from_str_radix(text.trim(), 16).ok().map(i32::from),
                "Colour is hex from 00 to FF".to_owned(),
            ),
            ModField::Body => (percent(text), "Body is a percent from 0 to 100".to_owned()),
            ModField::Sustain => (
                percent(text),
                "Sustain is a percent from 0 to 100".to_owned(),
            ),
            ModField::Ratchet => (
                count(text),
                format!("Ratchet is how many strikes, from 1 to {MAX_RATCHET}"),
            ),
            // `Note` is below no time.
            ModField::Hold => {
                let hold = if text.trim().eq_ignore_ascii_case("note") {
                    Some(-1)
                } else {
                    let ms = text.trim().trim_end_matches("ms").trim();
                    ms.parse::<u16>().ok().map(i32::from)
                };
                (
                    hold,
                    format!("Hold is milliseconds from 0 to {MAX_HOLD_MS}, or note"),
                )
            }
            ModField::Length => (
                count(text),
                format!("Length is how many steps, from 1 to {STEPS}"),
            ),
            // Typed from 1, as the page shows it.
            ModField::Start => (
                count(text).map(|start| start - 1),
                format!("Start is a step, from 1 to {STEPS}"),
            ),
            ModField::Mutate => (
                percent(text.trim()),
                format!("Mutate is a percent from 0 to {MAX_MUTATE}"),
            ),
            ModField::Smooth => (signed(text), "Smooth is hex from -FF to +FF".to_owned()),
            ModField::Step(_) => (signed(text), "A step is hex from -FF to +FF".to_owned()),
            ModField::Phase => (
                text.parse::<u16>().ok().map(i32::from),
                "Phase is degrees from 0 to 359".to_owned(),
            ),
            ModField::Deform => (signed(text), "Deform is hex from -FF to +FF".to_owned()),
            ModField::Low | ModField::High => {
                let source = current.track.source;
                let value = match source {
                    Source::Note => text
                        .trim()
                        .parse::<Note>()
                        .ok()
                        .and_then(|note| match note {
                            Note::On(midi) => Some(midi),
                            _ => None,
                        }),
                    Source::Velocity | Source::Random => u8::from_str_radix(text.trim(), 16).ok(),
                };
                let what = Control::Mod(slot, field).label();
                let (lowest, highest) = source.span();
                let (lowest, highest) = (source.text(lowest), source.text(highest));
                let why = match source {
                    Source::Note => format!("{what} is a note from {lowest} to {highest}"),
                    _ => format!("{what} is hex from {lowest} to {highest}"),
                };
                (value.map(i32::from), why)
            }
            ModField::Type
            | ModField::Mode
            | ModField::Polarity
            | ModField::Destination
            | ModField::Reset
            | ModField::Loop
            | ModField::Shape
            | ModField::Rate
            | ModField::Fade => return,
        };
        let within = |value: &i32| {
            current
                .bounds(field)
                .is_some_and(|(lowest, highest)| (lowest..=highest).contains(value))
        };
        match value.filter(within) {
            Some(value) => {
                let mut modulator = current;
                modulator.set_field(field, value);
                set_mod(services, slot, modulator);
            }
            None => services.status = why,
        }
    }
}

mod old_pickers {

    use ratatui::style::Style;
    use vt_model::accumulator::{Accumulation, MOST_DEGREES, Reset};
    use vt_model::arpeggio::{Arpeggio, MOST_OCTAVES, retrig_meaning};
    use vt_model::insert::Insert;
    use vt_model::lfo::{Fade, Lfo, Shape, note_rates};
    use vt_model::listed::Listed;
    use vt_model::modulation::{MOD_SLOTS, ModKind, ModSlot, Polarity};
    use vt_model::settings::{Kind, SettingId};
    use vt_model::song::{FilterType, Machine, Role};
    use vt_model::speed::Speed;
    use vt_model::steps::{Loop, StepRate, Steps};

    use crate::app::App;
    use crate::glyphs;
    use crate::input::command::{Line, PatchLine, PresetLine};
    use crate::input::picker::{Item, Pick, Picker};
    use crate::theme;
    use crate::track_page::view::IN_SCALE_PITCH;
    use vt_model::preset::PresetKind;

    use super::old_editing::toggle_control;
    use crate::track_page::controls::no_modes;

    /// The titles of each slot's lists, as a list's title is fixed text.
    const TYPE_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 type",
        "Mod 2 type",
        "Mod 3 type",
        "Mod 4 type",
        "Mod 5 type",
        "Mod 6 type",
    ];
    const MODE_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 mode",
        "Mod 2 mode",
        "Mod 3 mode",
        "Mod 4 mode",
        "Mod 5 mode",
        "Mod 6 mode",
    ];
    const SOURCE_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 source",
        "Mod 2 source",
        "Mod 3 source",
        "Mod 4 source",
        "Mod 5 source",
        "Mod 6 source",
    ];
    const CURVE_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 curve",
        "Mod 2 curve",
        "Mod 3 curve",
        "Mod 4 curve",
        "Mod 5 curve",
        "Mod 6 curve",
    ];
    const POLARITY_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 polarity",
        "Mod 2 polarity",
        "Mod 3 polarity",
        "Mod 4 polarity",
        "Mod 5 polarity",
        "Mod 6 polarity",
    ];
    const DESTINATION_TITLES: [&str; MOD_SLOTS] = [
        "Mod 1 destination",
        "Mod 2 destination",
        "Mod 3 destination",
        "Mod 4 destination",
        "Mod 5 destination",
        "Mod 6 destination",
    ];
    use crate::track_page::{AccumulatorField, ArpField, Control, ModField};

    /// A picker's items for every one of a [`Listed`] type's values, in order, each with what it does
    /// and picking what `pick` makes of it.
    fn items_of<X: Listed>(pick: impl Fn(X) -> Pick) -> Vec<Item> {
        X::ALL
            .iter()
            .map(|each| Item::new(each.name(), pick(*each)).meaning(each.meaning()))
            .collect()
    }

    /// A list of what a machine or a slot can be, with loading and saving a preset of it after them,
    /// as Ableton's hot-swap is on a device's title (decision 310).
    /// The machines' list draws each with its icon, so these have the presets folder's, which also
    /// keeps a search for a machine, as `sa` for `Sampler`, from finding them first.
    fn with_presets(mut items: Vec<Item>, kind: PresetKind) -> Vec<Item> {
        if kind == PresetKind::Patch {
            return with_patches(items);
        }
        let (what, icon) = match kind {
            PresetKind::Sound => ("sound preset", format!("{} ", glyphs::FOLDER)),
            _ => ("effect preset", String::new()),
        };
        let (load, into) = match kind {
            PresetKind::Sound => ("a", "keeping the track's phrases, level and sends"),
            _ => ("an", "into this slot"),
        };
        items.push(
            Item::new(
                format!("{icon}Load {load} {what}"),
                Pick::Line(Line::Preset(PresetLine::Browse)),
            )
            .meaning(format!(":preset, {into}"))
            .category(PRESETS),
        );
        items.push(
            Item::new(
                format!("{icon}Save as {load} {what}"),
                Pick::Prompt("preset save ".to_owned()),
            )
            .meaning(":preset save, then its name")
            .category(PRESETS),
        );
        items
    }

    /// A list on the Mod page or the macros, with adding a patch and saving the track's as one after
    /// it (decision 311).
    fn with_patches(mut items: Vec<Item>) -> Vec<Item> {
        items.push(
            Item::new(
                "Add a patch",
                Pick::Line(Line::Patch(PatchLine::Browse { over: false })),
            )
            .meaning(":patch, into free mod slots and macros, keeping the sound")
            .category(PATCHES),
        );
        items.push(
            Item::new("Save as a patch", Pick::Prompt("patch save ".to_owned()))
                .meaning(":patch save, the macros and what they move, then its name")
                .category(PATCHES),
        );
        items
    }

    /// The heading the patch items are listed under.
    const PATCHES: &str = "Patches";

    /// The heading the preset items are listed under.
    const PRESETS: &str = "Presets";

    impl App {
        pub(crate) fn old_open_control_picker(&mut self) {
            let control = self.track_control();
            // Something that's only on or off switches, as there's nothing to choose between.
            if matches!(
                control,
                Control::Mute | Control::Solo | Control::Limiter | Control::Safe
            ) {
                toggle_control(&mut self.services, control);
                return;
            }
            if self.track_control() == Control::Sample {
                self.open_bank_picker();
                return;
            }
            if matches!(control, Control::Tune | Control::Cents) {
                self.open_tuning_picker(control);
                return;
            }
            if self.track_control().setting_on(&self.services.page_track())
                == Some(SettingId::SLICE)
            {
                self.open_slice_picker();
                return;
            }
            if let Control::Insert(slot) = control {
                self.old_open_insert_picker(slot);
                return;
            }
            if let Control::InsertParam(slot, param) = control {
                self.old_open_insert_param_picker(slot, param);
                return;
            }
            if self.track_control() == Control::FilterType {
                self.old_open_filter_picker();
                return;
            }
            if self.track_control() == Control::FilterMode {
                self.old_open_filter_mode_picker();
                return;
            }
            if control == Control::Role {
                self.old_open_role_picker();
                return;
            }
            if self.track_control() == Control::Speed {
                self.old_open_speed_picker();
                return;
            }
            if self.track_control() == Control::Machine {
                self.old_open_machine_picker();
                return;
            }
            if self.track_control() == Control::Groove {
                self.open_groove_picker(true);
                return;
            }
            if let Some(id) = self.track_control().setting_on(&self.services.page_track())
                && let Kind::Stepped { choices, .. } = id.entry().kind
            {
                self.old_open_setting_picker(id, choices);
                return;
            }
            if let Control::Accumulator(field) = self.track_control() {
                self.old_open_accumulator_picker(field);
                return;
            }
            if let Control::Arpeggio(field) = self.track_control() {
                self.old_open_arpeggio_picker(field);
                return;
            }
            // A macro is the patch's knob, so its list is adding a patch and saving one.
            if let Control::Macro(_) = control {
                self.show_picker(Picker::new("Patches", with_patches(Vec::new())));
                return;
            }
            let Control::Mod(slot, field) = self.track_control() else {
                self.services.status =
                "K chooses from a list on Machine, Voice, Patch, File, Type, Mode, Groove, Speed, Dest, Shape, Rate and Reset"
                    .to_owned();
                return;
            };
            self.old_open_mod_picker(slot, field);
        }

        /// List the filter types, with what each does and the current one highlighted.
        /// List the effects a slot can hold, with the current one highlighted.
        fn old_open_insert_picker(&mut self, slot: usize) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().inserts[slot];
            let items = Insert::ALL
                .map(|insert| {
                    Item::new(
                        insert.name(),
                        Pick::Insert {
                            track,
                            slot,
                            // Choosing what's already there keeps its settings.
                            insert: if insert.name() == current.name() {
                                current
                            } else {
                                insert
                            },
                        },
                    )
                    .meaning(insert.meaning())
                })
                .to_vec();
            let items = with_presets(items, PresetKind::Effect);
            let position = Insert::ALL
                .iter()
                .position(|insert| insert.name() == current.name());
            self.show_picker(Picker::new("Effect", items).opened_on(position));
        }

        /// List the choices of a slot's setting, with the current one highlighted.
        fn old_open_insert_param_picker(&mut self, slot: usize, param: usize) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().inserts[slot];
            let Some(found) = current
                .params()
                .get(param)
                .filter(|p| !p.choices().is_empty())
            else {
                self.services.status =
                    "This is a value: = and - change it, and i types it".to_owned();
                return;
            };
            let items = found
                .choices()
                .iter()
                .enumerate()
                .map(|(index, choice)| {
                    Item::new(
                        *choice,
                        Pick::Insert {
                            track,
                            slot,
                            insert: current.with_value(param, index as u8),
                        },
                    )
                })
                .collect();
            let position = usize::from(current.value(param));
            self.show_picker(Picker::new(found.name, items).opened_on(Some(position)));
        }

        fn old_open_filter_picker(&mut self) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().filter;
            let items = items_of(|filter| Pick::FilterType { track, filter });
            let position = FilterType::ALL.iter().position(|filter| *filter == current);
            self.show_picker(Picker::new("Filter type", items).opened_on(position));
        }

        /// List what one of the accumulator's settings can be, with what each does and the one it has
        /// highlighted. Its limits are numbers, typed or moved instead.
        fn old_open_accumulator_picker(&mut self, field: AccumulatorField) {
            let track = self.services.cursor.track;
            let now = self.services.page_track().accumulation;
            let pick = |accumulation| Pick::Accumulation {
                track,
                accumulation,
            };
            let (title, items): (&str, Vec<Item>) = match field {
                AccumulatorField::Up | AccumulatorField::Down => {
                    self.services.status = format!(
                        "A limit is a number of degrees from 1 to {MOST_DEGREES}: i types it, = and - move it"
                    );
                    return;
                }
                AccumulatorField::Moves => (
                    "Accumulator moves",
                    items_of(|moves| pick(Accumulation { moves, ..now })),
                ),
                AccumulatorField::AtLimit => (
                    "Accumulator at its limit",
                    items_of(|at_limit| pick(Accumulation { at_limit, ..now })),
                ),
                AccumulatorField::Adds => (
                    "Accumulator adds",
                    items_of(|adds| pick(Accumulation { adds, ..now })),
                ),
                AccumulatorField::Reset => (
                    "Accumulator starts again",
                    Reset::all()
                        .map(|reset| {
                            Item::new(reset.name(), pick(Accumulation { reset, ..now }))
                                .meaning(reset.meaning())
                        })
                        .collect(),
                ),
            };
            let position = items.iter().position(|item| item.pick == pick(now));
            self.show_picker(Picker::new(title, items).opened_on(position));
        }

        /// List what one of the arpeggio's settings can be, with what each does and the one it has
        /// highlighted. Its octaves are a number, typed or moved instead.
        fn old_open_arpeggio_picker(&mut self, field: ArpField) {
            let track = self.services.cursor.track;
            let now = self.services.page_track().arpeggio;
            let pick = |arpeggio| Pick::Arpeggio { track, arpeggio };
            let (title, items): (&str, Vec<Item>) = match field {
                ArpField::Octaves => {
                    self.services.status = format!(
                        "Octaves is a number from 1 to {MOST_OCTAVES}: i types it, = and - move it"
                    );
                    return;
                }
                ArpField::Rate => ("Arp rate", items_of(|rate| pick(Arpeggio { rate, ..now }))),
                ArpField::Mode => ("Arp mode", items_of(|mode| pick(Arpeggio { mode, ..now }))),
                ArpField::Retrig => (
                    "Arp retrig",
                    [false, true]
                        .map(|retrig| {
                            let name = if retrig { "on" } else { "off" };
                            Item::new(name, pick(Arpeggio { retrig, ..now }))
                                .meaning(retrig_meaning(retrig))
                        })
                        .to_vec(),
                ),
            };
            let position = items.iter().position(|item| item.pick == pick(now));
            self.show_picker(Picker::new(title, items).opened_on(position));
        }

        /// List the speeds a track's steps can go at, with the one it plays highlighted.
        fn old_open_speed_picker(&mut self) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().speed;
            let items = items_of(|speed| Pick::Speed { track, speed });
            let position = Speed::ALL.iter().position(|speed| *speed == current);
            self.show_picker(Picker::new("Speed", items).opened_on(position));
        }

        /// List the roles, each in its colour with what it's for, `Auto` saying what the machine
        /// makes it, and the track's highlighted.
        fn old_open_role_picker(&mut self) {
            let track = self.services.cursor.track;
            let page = self.services.page_track();
            let items = Role::ALL
                .map(|role| {
                    let meaning = match role {
                        Role::Auto => {
                            format!("As its machine is for: {}", page.machine.role().name())
                        }
                        _ => role.meaning().to_owned(),
                    };
                    let style =
                        theme::role(role).map_or_else(Style::new, |colour| Style::new().fg(colour));
                    Item::new(role.name(), Pick::Role { track, role })
                        .meaning(meaning)
                        .styled(style)
                })
                .to_vec();
            let position = Role::ALL.iter().position(|role| *role == page.role);
            self.show_picker(Picker::new("Role", items).opened_on(position));
        }

        /// List the modes the track's filter type offers, with what each does and the one it plays
        /// highlighted.
        fn old_open_filter_mode_picker(&mut self) {
            let track = self.services.cursor.track;
            let page = self.services.page_track();
            let modes = page.filter.modes();
            if modes.is_empty() {
                self.services.status = no_modes(page.filter);
                return;
            }
            let items = modes
                .iter()
                .map(|mode| {
                    Item::new(mode.name(), Pick::FilterMode { track, mode: *mode })
                        .meaning(mode.meaning())
                })
                .collect();
            let playing = page.filter.plays(page.filter_mode);
            let position = modes.iter().position(|mode| *mode == playing);
            self.show_picker(Picker::new("Filter mode", items).opened_on(position));
        }

        /// List the machines, each with its glyph, a sampler, a synth or a drum, and what it does,
        /// the sampler at the top and each voice under its kind, and the current one highlighted.
        fn old_open_machine_picker(&mut self) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().machine;
            let items = Machine::ALL
                .map(|machine| {
                    let label = format!("{} {}", glyphs::machine(machine), machine.name());
                    Item::new(label, Pick::Machine { track, machine })
                        .meaning(machine.meaning())
                        .category(machine.category())
                })
                .to_vec();
            let items = with_presets(items, PresetKind::Sound);
            let position = Machine::ALL.iter().position(|machine| *machine == current);
            self.show_picker(Picker::new("Machine", items).opened_on(position));
        }

        /// List what a machine's setting can be, such as 6-op's patches, with the current one
        /// highlighted.
        fn old_open_setting_picker(
            &mut self,
            setting: SettingId,
            choices: &'static [&'static str],
        ) {
            let track = self.services.cursor.track;
            let current = self.services.page_track().setting(setting);
            let items = (0..choices.len().min(0x100))
                .map(|at| {
                    Item::new(
                        format!("{at:02X}  {}", choices[at]),
                        Pick::Setting {
                            track,
                            setting,
                            value: at as u8,
                        },
                    )
                })
                .collect();
            self.show_picker(
                Picker::new(setting.name(), items).opened_on(Some(usize::from(current))),
            );
        }

        /// List what a mod slot's setting can be, with the current one highlighted: its type, its
        /// type's mode, where it goes, or an envelope's or an LFO's own that are lists.
        fn old_open_mod_picker(&mut self, slot: usize, field: ModField) {
            let track = self.services.cursor.track;
            let page = self.services.page_track();
            let modulator = page.mods[slot];
            let pick = |modulator| Pick::Mod {
                track,
                slot,
                modulator,
            };
            // The slot with the setting at one of its choices, as the slot counts them.
            let chosen = |at: usize| {
                let mut chosen = modulator;
                chosen.set_field(field, at as i32);
                pick(chosen)
            };
            let (lfo, steps) = (modulator.lfo, modulator.steps);
            let (title, items): (&'static str, Vec<Item>) = match field {
                ModField::Type => {
                    let kinds = ModKind::ALL.map(|kind| {
                        Item::new(kind.name(), pick(ModSlot { kind, ..modulator }))
                            .meaning(kind.meaning())
                    });
                    (TYPE_TITLES[slot], with_patches(kinds.to_vec()))
                }
                // An envelope's curve, how notes start an LFO or Steps, or what Track follows.
                ModField::Mode if modulator.kind.has_mode() => {
                    let titles = match modulator.kind {
                        ModKind::Lfo | ModKind::Steps => MODE_TITLES,
                        ModKind::Track => SOURCE_TITLES,
                        _ => CURVE_TITLES,
                    };
                    let modes = modulator
                        .kind
                        .modes()
                        .into_iter()
                        .enumerate()
                        .map(|(at, (name, meaning))| Item::new(name, chosen(at)).meaning(meaning))
                        .collect();
                    (titles[slot], modes)
                }
                ModField::Rate if modulator.kind == ModKind::Steps => {
                    let rates = StepRate::ALL.map(|rate| {
                        let steps = Steps { rate, ..steps };
                        Item::new(rate.to_string(), pick(ModSlot { steps, ..modulator }))
                    });
                    ("Steps rate", rates.to_vec())
                }
                ModField::Loop => {
                    let loops = Loop::ALL.map(|looping| {
                        let steps = Steps { looping, ..steps };
                        Item::new(looping.to_string(), pick(ModSlot { steps, ..modulator }))
                            .meaning(looping.meaning())
                    });
                    ("Steps loop", loops.to_vec())
                }
                ModField::Polarity => {
                    let both = Polarity::ALL.map(|polarity| {
                        let chosen = ModSlot {
                            polarity: Some(polarity),
                            ..modulator
                        };
                        Item::new(polarity.name(), pick(chosen)).meaning(polarity.meaning())
                    });
                    (POLARITY_TITLES[slot], both.to_vec())
                }
                ModField::Destination => {
                    let off = Item::new(
                        "---",
                        pick(ModSlot {
                            destination: None,
                            in_scale: false,
                            ..modulator
                        }),
                    )
                    .meaning("Off");
                    // `Pitch` twice: as it is, and moving the note along the song's scale.
                    let on = SettingId::for_mod(&page, slot)
                        .into_iter()
                        .flat_map(|destination| {
                            let scaled = (destination == SettingId::PITCH).then_some(true);
                            [Some(false), scaled]
                                .into_iter()
                                .flatten()
                                .map(move |in_scale| (destination, in_scale))
                        })
                        .map(|(destination, in_scale)| {
                            let (name, meaning) = if in_scale {
                                (
                                    IN_SCALE_PITCH.to_owned(),
                                    "Moves the note along the song's scale".to_owned(),
                                )
                            } else {
                                (destination.name_on(&page), destination.meaning())
                            };
                            Item::new(
                                name,
                                pick(ModSlot {
                                    destination: Some(destination),
                                    in_scale,
                                    ..modulator
                                }),
                            )
                            .meaning(meaning)
                            .category(destination.group())
                        });
                    (
                        DESTINATION_TITLES[slot],
                        std::iter::once(off).chain(on).collect(),
                    )
                }
                ModField::Reset => {
                    let choices = [true, false].map(|reset| {
                        let (name, meaning) = if reset {
                            ("on", "Each note starts it from zero")
                        } else {
                            ("off", "Each note goes on from where it is")
                        };
                        Item::new(name, chosen(usize::from(reset))).meaning(meaning)
                    });
                    ("Envelope reset", choices.to_vec())
                }
                ModField::Shape => {
                    let shapes = Shape::ALL.map(|shape| {
                        let lfo = Lfo { shape, ..lfo };
                        let item = Item::new(shape.name(), pick(ModSlot { lfo, ..modulator }));
                        // The attractors say what they are, and Rössler's name is found without its
                        // umlaut.
                        match shape {
                            Shape::Lorenz => {
                                item.meaning("chaos round two wings, never quite repeating")
                            }
                            Shape::Rossler => item.meaning("Rossler's chaos round one loop"),
                            _ => item,
                        }
                    });
                    ("LFO shape", shapes.to_vec())
                }
                ModField::Rate => {
                    let rates = note_rates()
                        .into_iter()
                        .map(|rate| {
                            let lfo = Lfo { rate, ..lfo };
                            Item::new(rate.to_string(), pick(ModSlot { lfo, ..modulator }))
                        })
                        .collect();
                    ("LFO rate. i types any other", rates)
                }
                ModField::Fade => {
                    let fades = Fade::ALL.map(|fade| {
                        let lfo = Lfo { fade, ..lfo };
                        let meaning = match fade {
                            Fade::Off => "the whole wave from each note",
                            Fade::In(_) => "in from nothing after each note",
                            Fade::Out(_) => "out to nothing after each note",
                        };
                        Item::new(fade.to_string(), pick(ModSlot { lfo, ..modulator }))
                            .meaning(meaning)
                    });
                    ("LFO fade", fades.to_vec())
                }
                _ => {
                    self.services.status =
                    "K chooses from a list on Type, Mode, Loop, Polarity, Dest, Shape, Rate, Fade and Reset"
                        .to_owned();
                    return;
                }
            };
            // A polarity that follows its type opens on the one it plays.
            let current = match field {
                ModField::Polarity => ModSlot {
                    polarity: Some(modulator.polarity()),
                    ..modulator
                },
                _ => modulator,
            };
            let position = items.iter().position(|item| item.pick == pick(current));
            self.show_picker(Picker::new(title, items).opened_on(position));
        }
    }
}

use old_editing::{adjust_control, adjust_fine, reset_control};
use old_typing::commit_control;

mod check {
    use std::sync::Arc;

    use vt_model::accumulator::MOST_DEGREES;
    use vt_model::arpeggio::MOST_OCTAVES;
    use vt_model::envelope::Hold;
    use vt_model::groove::GrooveId;
    use vt_model::insert::Insert;
    use vt_model::lfo::Shape;
    use vt_model::modulation::{ModKind, Polarity};
    use vt_model::settings::SettingId;
    use vt_model::song::{FilterMode, FilterType, Machine, Role, Song, TrackKind};
    use vt_model::speed::Speed;
    use vt_model::tracking::Source;

    use crate::app::App;
    use crate::input::mode::{Mode, View};
    use crate::input::picker::Pick;
    use crate::mixer_page::MIXER;
    use crate::track_page::{Control, Page};
    use crate::views::widgets::pin::app_with;

    /// A song with every machine, effect and mod type, each of the track's own away from its
    /// default on some tracks, and values where a step's size depends on where it is.
    pub(super) fn varied() -> Song {
        let mut song = Song::new(Machine::ALL.len());
        song.limiter = false;
        let sound = song
            .tracks
            .iter_mut()
            .filter(|track| track.kind == TrackKind::Sound);
        for (at, track) in sound.enumerate() {
            track.machine = Machine::ALL[at % Machine::ALL.len()];
            let count = track.inserts.len();
            for (slot, insert) in track.inserts.iter_mut().enumerate() {
                *insert = Insert::ALL[(at * count + slot + 1) % Insert::ALL.len()];
                if at % 3 == 1 {
                    *insert = insert.with_value(0, 0xFF).with_value(1, 1);
                }
            }
            let count = track.mods.len();
            for (slot, modulator) in track.mods.iter_mut().enumerate() {
                modulator.kind = ModKind::ALL[(at * count + slot) % ModKind::ALL.len()];
                let ms = [9, 10, 100, 0, 8000, 1][(at + slot) % 6];
                (modulator.adsr.attack_ms, modulator.ahd.attack_ms) = (ms, ms);
                (modulator.adsr.decay_ms, modulator.lpg.decay_ms) = (ms + 1, ms);
                modulator.drum.peak_ms = ms;
                modulator.ahd.hold = [
                    Hold::Note,
                    Hold::Ms(0),
                    Hold::Ms(10),
                    Hold::Ms(99),
                    Hold::Ms(5),
                ][at % 5];
                modulator.lfo.shape = Shape::ALL[(at + slot) % Shape::ALL.len()];
                modulator.lfo.phase = [0, 120, 350, 45][slot % 4];
                modulator.polarity =
                    [None, Some(Polarity::ALL[0]), Some(Polarity::ALL[1])][(at + slot) % 3];
                modulator.track.source = Source::ALL[(at + slot) % Source::ALL.len()];
                modulator.depth = [0, 0xFF, -0xFF, 0x40][(at + slot) % 4];
                modulator.adsr.sustain = [0, 100, 50][slot % 3];
                modulator.steps.length = [1, 16, 8][(at + slot) % 3];
                modulator.steps.start = [0, 15, 3][(at + slot) % 3];
                if (at + slot) % 4 == 2 {
                    modulator.destination = Some(SettingId::PITCH);
                    modulator.in_scale = true;
                }
            }
            track.reset = [None, Some(7), Some(16), Some(17)][at % 4];
            track.glide = [0, 0x08, 0xF8, 0xFF][at % 4];
            track.accumulation.up = [1, 3, MOST_DEGREES][at % 3];
            track.arpeggio.octaves = 1 + at as u8 % MOST_OCTAVES;
            track.arpeggio.retrig = at % 2 == 0;
            track.groove = [
                None,
                Some(GrooveId::from_index(0)),
                Some(GrooveId::from_index(3)),
            ][at % 3]
                .flatten();
            track.gate_length = [None, Some(1), Some(0xFF), Some(0x80)][at % 4];
            track.choke = [None, Some(1), Some(8)][at % 3];
            track.role = Role::ALL[at % Role::ALL.len()];
            track.speed = Speed::ALL[at % Speed::ALL.len()];
            track.filter = FilterType::ALL[at % FilterType::ALL.len()];
            track.filter_mode = FilterMode::ALL[(at / 2) % FilterMode::ALL.len()];
            track.safe = at % 2 == 1;
            track.level = [0, 0xFF, 0x80][at % 3];
            track.pan = [0, 0xFF, 0x80][(at + 1) % 3];
            track.macros[at % 8].value = [0, 0xFF00, 0x1234][at % 3];
        }
        song
    }

    /// What an edit leaves: the song, the status line, how many undo steps, and any list open,
    /// with its title, its items and the one it's on.
    fn left(app: &App) -> String {
        let picker = app.picker.as_ref().map(|picker| {
            let items: Vec<_> = picker
                .items()
                .iter()
                .map(|item| {
                    (
                        &item.label,
                        &item.meaning,
                        &item.category,
                        item.style,
                        &item.wider,
                        &item.preview,
                    )
                })
                .collect();
            format!(
                "{} {:?} {:?} {}",
                picker.title,
                items,
                picker.current_index(),
                picker.selected
            )
        });
        format!(
            "{:?}\n{}\n{}\n{:?}\n{:?}",
            Song {
                name: String::new(),
                ..Song::clone(app.services.song())
            },
            app.services.status,
            app.services.history.undo_depth(),
            picker,
            app.modes.mode
        )
    }

    /// Back to the song as it was, with no list and the status line empty.
    fn restore(app: &mut App, depth: usize, mode: Mode) {
        while app.services.history.undo_depth() > depth {
            app.services.history.undo();
        }
        app.picker = None;
        app.modes.mode = mode;
        app.services.status.clear();
    }

    /// Where the first difference is, with some of each around it.
    fn differs(old: &str, new: &str) -> String {
        let from = old
            .chars()
            .zip(new.chars())
            .position(|(a, b)| a != b)
            .unwrap_or(0)
            .saturating_sub(60);
        let end = |text: &str| text.len().min(from + 240);
        format!(
            "old: {}\nnew: {}",
            old.get(from..end(old)).unwrap_or(old),
            new.get(from..end(new)).unwrap_or(new)
        )
    }

    /// What's typed on every setting: hex, numbers in and out of bounds, words and units.
    const TYPED: [&str; 24] = [
        "00", "7F", "FF", "1", "0", "5", "16", "100", "300", "-10", "+20", "-FF", "off", "G2",
        "50%", "120ms", "0.5", "note", "2bar", "C-3", "G-8", "x", "1/8", "4",
    ];

    struct Pair {
        old: App,
        new: App,
        count: usize,
    }

    impl Pair {
        /// Run the same edit both ways, check they leave the same, and put both back.
        fn same(&mut self, what: &str, old: impl FnOnce(&mut App), new: impl FnOnce(&mut App)) {
            let depth = self.old.services.history.undo_depth();
            assert_eq!(depth, self.new.services.history.undo_depth());
            let mode = self.old.modes.mode;
            old(&mut self.old);
            new(&mut self.new);
            let (was, now) = (left(&self.old), left(&self.new));
            assert!(was == now, "{what}\n{}", differs(&was, &now));
            self.count += 1;
            restore(&mut self.old, depth, mode);
            restore(&mut self.new, depth, mode);
        }

        /// Every key on one setting: `= - + _` with and without a count, the fine digits, `x`,
        /// what's typed, and `K` with each of its choices picked.
        fn every_key(&mut self, control: Control, listed: bool) {
            for count in [1, 3, 15] {
                for up in [true, false] {
                    for large in [false, true] {
                        let what = format!("{control:?} up {up} large {large} x{count}");
                        self.same(
                            &what,
                            |app| {
                                super::adjust_control(&mut app.services, control, up, count, large)
                            },
                            |app| {
                                crate::track_page::editing::adjust_control(
                                    &mut app.services,
                                    control,
                                    up,
                                    count,
                                    large,
                                )
                            },
                        );
                        self.same(
                            &format!("fine {what}"),
                            |app| super::adjust_fine(&mut app.services, control, up, count, large),
                            |app| {
                                crate::track_page::editing::adjust_fine(
                                    &mut app.services,
                                    control,
                                    up,
                                    count,
                                    large,
                                )
                            },
                        );
                    }
                }
            }
            self.same(
                &format!("x on {control:?}"),
                |app| super::reset_control(&mut app.services, control),
                |app| crate::track_page::editing::reset_control(&mut app.services, control),
            );
            // `i` on a list opens it, as `K` does.
            let typed = match control.is_choice(&self.old.services.page_track()) {
                true => &[][..],
                false => &TYPED[..],
            };
            for &text in typed {
                self.same(
                    &format!("i {text} on {control:?}"),
                    |app| super::commit_control(&mut app.services, control, text),
                    |app| {
                        crate::track_page::typing::commit_control(&mut app.services, control, text)
                    },
                );
            }
            if !listed {
                return;
            }
            self.same(
                &format!("K on {control:?}"),
                |app| app.old_open_control_picker(),
                |app| app.open_control_picker(),
            );
            // Each choice, picked.
            self.old.old_open_control_picker();
            self.new.open_control_picker();
            let picks = |app: &App| -> Vec<Pick> {
                app.picker
                    .as_ref()
                    .map(|picker| {
                        picker
                            .items()
                            .iter()
                            .map(|item| item.pick.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            let (old_picks, new_picks) = (picks(&self.old), picks(&self.new));
            let depth = self.old.services.history.undo_depth();
            let mode = self.old.modes.mode;
            restore(&mut self.old, depth, mode);
            restore(&mut self.new, depth, mode);
            for (at, (old, new)) in old_picks.into_iter().zip(new_picks).enumerate() {
                // Loading and saving a preset or a patch open the command line or the explorer.
                if matches!(old, Pick::Line(_) | Pick::Prompt(_)) {
                    assert_eq!(old, new);
                    continue;
                }
                self.same(
                    &format!("pick {at} on {control:?}: {old:?} against {new:?}"),
                    |app| app.choose(old),
                    |app| app.choose(new),
                );
            }
        }
    }

    #[test]
    fn every_key_on_every_setting_does_what_it_did() {
        let song = varied();
        let tracks = song.tracks.len();
        let (old, _old_realtime) = app_with(song.clone());
        let (new, _new_realtime) = app_with(song);
        let mut pair = Pair { old, new, count: 0 };
        for track in 0..tracks {
            for app in [&mut pair.old, &mut pair.new] {
                app.services.cursor.track = track;
                app.modes.view = View::Track;
                app.modes.mode = Mode::Normal;
            }
            for page in [Page::Sound, Page::Mod, Page::Fx] {
                pair.old.track_page.page = page;
                pair.new.track_page.page = page;
                let rows = pair.old.track_page.rows(pair.old.ctx().track_here());
                for (row, places) in rows.iter().enumerate() {
                    for (field, place) in places.iter().enumerate() {
                        let Some(control) = *place else {
                            continue;
                        };
                        if matches!(control, Control::Mute | Control::Solo) {
                            continue;
                        }
                        for app in [&mut pair.old, &mut pair.new] {
                            app.track_page.row = row;
                            app.track_page.field = field;
                            app.track_page.fine = false;
                        }
                        assert_eq!(pair.old.track_control(), control);
                        pair.every_key(control, true);
                    }
                }
            }
            // The mixer's, which a track may not have.
            for control in MIXER {
                if matches!(control, Control::Mute | Control::Solo) {
                    continue;
                }
                pair.every_key(control, false);
            }
        }
        assert!(pair.count > 10_000, "{}", pair.count);
    }
}

mod old_roto {

    use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
    use vt_model::action::Action;
    use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES};
    use vt_model::constants::{MAX_GROUPS, MAX_TRACK_RESET};
    use vt_model::groove::GrooveId;
    use vt_model::insert::Insert;
    use vt_model::kit::CHOKE_GROUPS;
    use vt_model::lfo::{Rate, note_rates};
    use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot, Polarity};
    use vt_model::settings::{Kind, SettingId, gate_percent, gate_value};
    use vt_model::shape::GenParam;
    use vt_model::shape::params::Units as GenUnits;
    use vt_model::song::{FilterType, Machine, Role, Song, Track, trim_text};
    use vt_model::speed::Speed;
    use vt_model::steps::StepRate;
    use vt_roto::live::{share_of, step_at};

    use crate::shaping::{live_shown, with_live};
    use crate::track_page::tuning::{tuning, tuning_most};
    use crate::track_page::{AccumulatorField, ArpField, Control, ModField, view};
    use crate::views::track::toggle_on;

    use crate::roto::devices::{hold_steps, knob, rate_position, time_steps};

    /// A value from 0 to a top, for a knob's position.
    fn whole(position: u16, top: u32) -> u32 {
        (share_of(position) * f64::from(top)).round() as u32
    }

    /// The item of a list a knob is on.
    fn pick<T: Copy>(items: &[T], position: u16) -> T {
        items[step_at(position, items.len())]
    }

    /// The action for a value, if it's a change.
    fn changed<T: PartialEq>(now: T, was: T, action: impl FnOnce(T) -> Action) -> Option<Action> {
        (now != was).then(|| action(now))
    }

    /// A setting of the registry at a position, as its coarse value and, if it has them, its fine
    /// part, with the actions that set what changed: `coarse` makes the action for the coarse value.
    fn registry(
        id: SettingId,
        position: u16,
        coarse: u8,
        fine: Option<u8>,
        set_coarse: impl FnOnce(u8) -> Action,
        set_fine: impl FnOnce(u8) -> Action,
    ) -> Option<Action> {
        match id.entry().kind {
            Kind::Continuous { top, .. } => match fine {
                Some(was_fine) => {
                    let at = whole(position, u32::from(top) << 8);
                    let (to, to_fine) = ((at >> 8) as u8, (at & 0xFF) as u8);
                    let mut actions = Vec::new();
                    if to != coarse {
                        actions.push(set_coarse(to));
                    }
                    // There's no fine part at the top.
                    let to_fine = if to == top { 0 } else { to_fine };
                    if to_fine != was_fine {
                        actions.push(set_fine(to_fine));
                    }
                    match actions.len() {
                        0 => None,
                        1 => actions.pop(),
                        _ => Some(Action::Batch(actions)),
                    }
                }
                None => changed(whole(position, u32::from(top)) as u8, coarse, set_coarse),
            },
            Kind::Stepped { choices, .. } => {
                changed(step_at(position, choices.len()) as u8, coarse, set_coarse)
            }
        }
    }

    /// An envelope's time at a position, by the steps `=` takes.
    fn time_at(position: u16) -> u16 {
        let steps = time_steps();
        let at = whole(position, u32::from(steps.len() as u16 - 1)) as usize;
        steps[at.min(steps.len() - 1)]
    }

    /// The rate of the same kind as `rate` nearest a position: a note division stays one, and so do
    /// steps and hertz.
    fn rate_at(rate: Rate, position: u16) -> Rate {
        let walk = |from: Rate, faster: bool| {
            let mut rates = vec![from];
            loop {
                let next = rates[rates.len() - 1].nudged(faster);
                if next == rates[rates.len() - 1] {
                    break rates;
                }
                rates.push(next);
            }
        };
        let rates = match rate {
            Rate::Note(_) => note_rates(),
            Rate::Steps(_) => walk(Rate::Steps(1), false),
            Rate::Hertz(_) => walk(Rate::Hertz(1), true),
        };
        rates
            .into_iter()
            .min_by_key(|candidate| rate_position(*candidate).abs_diff(position))
            .unwrap_or(rate)
    }

    /// A depth either side of none at a position.
    fn depth_at(position: u16) -> i16 {
        let top = MAX_DEPTH;
        whole(position, (2 * top) as u32) as i16 - top
    }

    /// A destination among a list of choices with none first, at a position.
    fn destination_at(choices: Vec<SettingId>, position: u16) -> Option<SettingId> {
        let at = step_at(position, choices.len() + 1);
        at.checked_sub(1).map(|at| choices[at])
    }

    /// What the knob for a control on a track, the `at`th of the song's, does turned to a position:
    /// the action that puts its setting there, or nothing if it's there already or the knob is a
    /// hole.
    pub fn turned(control: Control, at: usize, song: &Song, position: u16) -> Option<Action> {
        let track = song.tracks.get(at)?;
        if !control.exists_on(track) {
            return None;
        }
        // Where the setting already puts it: a knob has fewer places than a setting with fine
        // digits has values, so one left alone would otherwise round to the next.
        if knob(control, track, song).is_some_and(|shown| shown.position == position) {
            return None;
        }
        if let Some(on) = toggle_on(control, track, song) {
            let now = step_at(position, 2) == 1;
            if now == on {
                return None;
            }
            return switched(control, at, track, now);
        }
        match control {
            Control::Gen {
                phrase,
                generator,
                param,
            } => gen_turned(song, at, phrase, generator, param, position),
            Control::NewGenerator(_) => None,
            Control::Machine => changed(pick(&Machine::ALL, position), track.machine, |machine| {
                Action::SetTrackMachine { track: at, machine }
            }),
            Control::Sample => {
                let numbers: Vec<u8> = std::iter::once(0)
                    .chain(song.bank.iter().map(|(number, _)| number))
                    .collect();
                changed(pick(&numbers, position), track.sample(), |sample| {
                    Action::SetTrackSample { track: at, sample }
                })
            }
            Control::Setting(_) => {
                let id = control.setting_on(track)?;
                registry(
                    id,
                    position,
                    track.setting(id),
                    control.fine_on(track).map(|id| track.fine(id)),
                    |value| Action::SetTrackSetting {
                        track: at,
                        setting: id,
                        value,
                    },
                    |fine| Action::SetTrackFine {
                        track: at,
                        setting: id,
                        fine,
                    },
                )
            }
            // A turn of a macro, coarse and fine together, one undo step.
            Control::Macro(macro_at) => {
                let to = whole(position, 0xFF00) as u16;
                changed(to, track.macros[macro_at].value, |value| Action::SetMacro {
                    track: at,
                    at: macro_at,
                    value,
                })
            }
            // The sample's own, as an edit of its bank entry, as the keys make.
            Control::Tune | Control::Cents => {
                let tuning = tuning(song, track)?;
                let most = i32::from(tuning_most(control));
                let to = whole(position, (2 * most) as u32) as i32 - most;
                let entry = tuning.with(control, Some(to as i8));
                (entry != *tuning.entry).then_some(Action::SetBankEntry {
                    number: tuning.number,
                    entry: Some(entry),
                })
            }
            Control::Base(which) => {
                let coarse = track.control(which);
                let set = |value| Action::SetTrackBase {
                    track: at,
                    control: which,
                    value,
                };
                match which.setting() {
                    Some(id) => registry(
                        id,
                        position,
                        coarse,
                        control.fine_on(track).map(|id| track.fine(id)),
                        set,
                        |fine| Action::SetTrackFine {
                            track: at,
                            setting: id,
                            fine,
                        },
                    ),
                    None => changed(whole(position, 0xFF) as u8, coarse, set),
                }
            }
            Control::Level => {
                let top = SettingId::LEVEL.entry().kind.top();
                changed(
                    whole(position, u32::from(top)) as u8,
                    track.level,
                    |level| Action::SetTrackLevel { track: at, level },
                )
            }
            Control::Trim => changed(whole(position, 0xFF) as u8, track.trim, |trim| {
                Action::SetTrackTrim { track: at, trim }
            }),
            Control::Groove => {
                let grooves: Vec<GrooveId> = GrooveId::all().collect();
                let chosen = pick(&grooves, position);
                let playing = track.groove.unwrap_or(song.groove);
                (chosen != playing).then_some(Action::SetTrackGroove {
                    track: at,
                    groove: Some(chosen),
                })
            }
            Control::Choke => {
                let choke = step_at(position, usize::from(CHOKE_GROUPS) + 1) as u8;
                changed((choke > 0).then_some(choke), track.choke, |choke| {
                    Action::SetTrackChoke { track: at, choke }
                })
            }
            Control::Group => {
                let group = step_at(position, MAX_GROUPS + 1) as u8;
                changed((group > 0).then_some(group), track.group, |group| {
                    Action::SetTrackGroup {
                        first: at,
                        last: at,
                        group,
                    }
                })
            }
            Control::Role => changed(pick(&Role::ALL, position), track.role, |role| {
                Action::SetTrackRole { track: at, role }
            }),
            Control::GateLength => {
                let percent = whole(position, 99) as u8 + 1;
                (percent != gate_percent(track.gate_length())).then_some(
                    Action::SetTrackGateLength {
                        track: at,
                        gate_length: Some(gate_value(percent)),
                    },
                )
            }
            Control::Speed => changed(pick(&Speed::ALL, position), track.speed, |speed| {
                Action::SetTrackSpeed { track: at, speed }
            }),
            Control::Reset => {
                let reset = whole(position, u32::from(MAX_TRACK_RESET)) as u16;
                changed((reset > 0).then_some(reset), track.reset, |reset| {
                    Action::SetTrackReset { track: at, reset }
                })
            }
            Control::Glide => changed(whole(position, 0xFF) as u8, track.glide, |glide| {
                Action::SetTrackGlide { track: at, glide }
            }),
            Control::FilterType => {
                changed(pick(&FilterType::ALL, position), track.filter, |filter| {
                    Action::SetFilterType { track: at, filter }
                })
            }
            Control::FilterMode => {
                let modes = track.filter.modes();
                if modes.is_empty() {
                    return None;
                }
                let playing = track.filter.plays(track.filter_mode);
                changed(pick(modes, position), playing, |mode| {
                    Action::SetFilterMode { track: at, mode }
                })
            }
            Control::Mod(slot, field) => {
                let was = track.mods[slot];
                let mut modulator = was;
                match field {
                    // What a slot is, the app's alone.
                    ModField::Type | ModField::Reset => return None,
                    ModField::Destination => {
                        modulator.destination =
                            destination_at(SettingId::for_mod(track, slot), position);
                    }
                    ModField::Rate if was.kind == ModKind::Steps => {
                        modulator.steps.rate = pick(&StepRate::ALL, position);
                    }
                    ModField::Rate => modulator.lfo.rate = rate_at(was.lfo.rate, position),
                    ModField::Hold => {
                        let steps = hold_steps();
                        let at = whole(position, steps.len() as u32 - 1) as usize;
                        modulator.ahd.hold = steps[at.min(steps.len() - 1)];
                    }
                    _ => {
                        let (lowest, highest) = was.bounds(field)?;
                        let top = (highest - lowest) as u32;
                        let to = match field {
                            ModField::Attack
                            | ModField::Decay
                            | ModField::Release
                            | ModField::Peak => i32::from(time_at(position)),
                            ModField::Depth
                            | ModField::Deform
                            | ModField::Smooth
                            | ModField::Step(_) => i32::from(depth_at(position)),
                            // One of a list, or a count or a place, by the step the knob is on.
                            ModField::Mode
                            | ModField::Polarity
                            | ModField::Shape
                            | ModField::Fade
                            | ModField::Loop
                            | ModField::Length
                            | ModField::Start
                            | ModField::Ratchet => {
                                lowest + step_at(position, top as usize + 1) as i32
                            }
                            // A number from its lowest to its highest.
                            _ => lowest + whole(position, top) as i32,
                        };
                        modulator.set_field(field, to);
                    }
                }
                changed(modulator, was, |modulator| Action::SetMod {
                    track: at,
                    slot,
                    modulator,
                })
            }
            Control::Accumulator(field) => {
                let was = track.accumulation;
                let mut accumulation = was;
                let degrees: Vec<u8> = (1..=MOST_DEGREES).collect();
                match field {
                    AccumulatorField::Up => accumulation.up = pick(&degrees, position),
                    AccumulatorField::Down => accumulation.down = pick(&degrees, position),
                    AccumulatorField::Moves => accumulation.moves = pick(&Moves::ALL, position),
                    AccumulatorField::AtLimit => {
                        accumulation.at_limit = pick(&AtLimit::ALL, position)
                    }
                    AccumulatorField::Adds => accumulation.adds = pick(&Adds::ALL, position),
                    AccumulatorField::Reset => {
                        let resets: Vec<Reset> = Reset::all().collect();
                        accumulation.reset = pick(&resets, position);
                    }
                }
                changed(accumulation, was, |accumulation: Accumulation| {
                    Action::SetAccumulation {
                        track: at,
                        accumulation,
                    }
                })
            }
            Control::Arpeggio(field) => {
                let was = track.arpeggio;
                let mut arpeggio = was;
                let octaves: Vec<u8> = (1..=MOST_OCTAVES).collect();
                match field {
                    ArpField::Rate => arpeggio.rate = pick(&ArpRate::ALL, position),
                    ArpField::Mode => arpeggio.mode = pick(&ArpMode::ALL, position),
                    ArpField::Octaves => arpeggio.octaves = pick(&octaves, position),
                    ArpField::Retrig => return None,
                }
                changed(arpeggio, was, |arpeggio: Arpeggio| Action::SetArpeggio {
                    track: at,
                    arpeggio,
                })
            }
            Control::Insert(slot) => {
                let chosen = pick(&Insert::ALL, position);
                (chosen.name() != track.inserts[slot].name()).then_some(Action::SetInsert {
                    track: at,
                    slot,
                    insert: chosen,
                })
            }
            Control::InsertParam(slot, param) => {
                let insert = track.inserts[slot];
                let found = insert.params().get(param)?;
                let set = insert.value(param);
                let to = if found.choices().is_empty() {
                    let (id, _) = control.value_on(track)?;
                    match id.entry().kind {
                        Kind::Continuous { top, .. } => whole(position, u32::from(top)) as u8,
                        Kind::Stepped { choices, .. } => step_at(position, choices.len()) as u8,
                    }
                } else {
                    step_at(position, found.choices().len()) as u8
                };
                changed(to, set, |to| Action::SetInsert {
                    track: at,
                    slot,
                    insert: insert.with_value(param, to),
                })
            }
            // The mixer's, which no device has.
            Control::Mute | Control::Solo | Control::Limiter | Control::Safe => None,
        }
    }

    /// An off/on setting switched, by a two-step knob.
    fn switched(control: Control, at: usize, track: &Track, on: bool) -> Option<Action> {
        match control {
            Control::InsertParam(slot, param) => Some(Action::SetInsert {
                track: at,
                slot,
                insert: track.inserts[slot].with_value(param, u8::from(on)),
            }),
            Control::Mod(slot, ModField::Reset) => {
                let mut modulator = track.mods[slot];
                modulator.set_field(ModField::Reset, i32::from(on));
                Some(Action::SetMod {
                    track: at,
                    slot,
                    modulator,
                })
            }
            // A slot's polarity on its button, `Bi` lit.
            Control::Mod(slot, ModField::Polarity) => Some(Action::SetMod {
                track: at,
                slot,
                modulator: ModSlot {
                    polarity: Some(if on { Polarity::Bi } else { Polarity::Uni }),
                    ..track.mods[slot]
                },
            }),
            Control::Arpeggio(ArpField::Retrig) => Some(Action::SetArpeggio {
                track: at,
                arpeggio: Arpeggio {
                    retrig: on,
                    ..track.arpeggio
                },
            }),
            // The song's limiter, on the master's output.
            Control::Limiter => Some(Action::SetLimiter(on)),
            Control::Safe => Some(Action::SetTrackSafe { track: at, on }),
            _ => None,
        }
    }

    /// A generator's setting at a knob's position, as the edit the page's would be: a list's or a
    /// switch's choice by its step, a number by its share of the way from its lowest to its
    /// highest, its column written again through `Reshape`.
    fn gen_turned(
        song: &Song,
        at: usize,
        phrase: u32,
        generator: usize,
        param: GenParam,
        position: u16,
    ) -> Option<Action> {
        let track = song.tracks.get(at)?;
        let shape = with_live(song, track, phrase, generator, |found, context| {
            let (low, high) = param.bounds(context)?;
            let number = if param.units() == GenUnits::Named {
                low + step_at(position, (high - low + 1) as usize) as i32
            } else {
                low + whole(position, (high - low) as u32) as i32
            };
            let mut shape = found.shape;
            param.set_number(&mut shape, context, number);
            (shape != found.shape).then_some(shape)
        })??;
        Some(Action::Reshape {
            track: at,
            phrase,
            generator,
            shape,
        })
    }

    /// A setting's value as the app writes it, for the Roto to show as its knob turns: in its units
    /// where it has them, as the status line says it (`-14.8 dB`, `1.21 kHz`), and otherwise as the
    /// track page shows it (`1/16`, `Overdrive`, `40 ms`). The Roto cuts it to 12 characters.
    pub fn value_text(control: Control, track: &Track, song: &Song) -> String {
        if control == Control::Trim {
            return trim_text(track.trim);
        }
        match (control, control.value_on(track)) {
            (_, Some((id, set))) if id.exists_on(track) => id.entry().kind.shown(set),
            // As it's set, rather than as a macro holding it puts it.
            (
                Control::Gen {
                    phrase,
                    generator,
                    param,
                },
                _,
            ) => live_shown(song, track, phrase, generator, param),
            _ => view(control, track, song, false)
                .map(|view| view.cell)
                .unwrap_or_default(),
        }
    }
}

mod roto_check {
    use std::sync::Arc;

    use vt_model::action::edit;
    use vt_model::song::Song;
    use vt_roto::live::KNOB_TOP;

    use crate::mixer_page::MIXER;
    use crate::track_page::{Control, Page};
    use crate::views::widgets::pin::app_with;

    #[test]
    fn every_knob_at_every_place_does_what_it_did() {
        let song = super::check::varied();
        let (mut app, _realtime) = app_with(song.clone());
        let song = Arc::new(song);
        let mut count = 0;
        let positions: Vec<u16> = (0..=KNOB_TOP)
            .step_by(127)
            .chain([KNOB_TOP, 1, 8191, 8192])
            .collect();
        for track in 0..song.tracks.len() {
            app.services.cursor.track = track;
            let mut controls: Vec<Control> = MIXER.to_vec();
            for page in [Page::Sound, Page::Mod, Page::Fx] {
                app.track_page.page = page;
                let rows = app.track_page.rows(Some(&song.tracks[track]));
                controls.extend(rows.into_iter().flatten().flatten());
            }
            for control in controls {
                for &position in &positions {
                    let old = super::old_roto::turned(control, track, &song, position);
                    let new = crate::roto::edits::turned(control, track, &song, position);
                    assert_eq!(
                        old.is_some(),
                        new.is_some(),
                        "{control:?} at {position}: {old:?} against {new:?}"
                    );
                    let after = |action: Option<vt_model::action::Action>| {
                        action.map(|action| {
                            format!("{:?}", edit(&song, &action).map(|next| (next.tracks[track].clone(), next.limiter, next.bank.clone())))
                        })
                    };
                    let (was, now) = (after(old.clone()), after(new.clone()));
                    assert!(
                        was == now,
                        "{control:?} at {position}: {old:?} against {new:?}"
                    );
                    count += 1;
                }
            }
        }
        assert!(count > 10_000, "{count}");
    }
}
