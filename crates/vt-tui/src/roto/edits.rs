//! A knob turned on the Roto as an edit of the song: the way back from [`super::devices`], a
//! knob's position to the setting's value it stands for, as the same `Action` a key would give,
//! and the value as the app writes it, for the Roto to show. Pure, like the mapping: a track and
//! its song in, an action out, so it's tested with no Roto at all.
//!
//! **Where a position lands** is where the mapping puts that value, so a knob left alone sends
//! back what it was sent: a list's step by [`step_at`], a value by its share of the way from
//! the bottom to its top, a setting with fine digits to its fine part, an envelope's time by the
//! steps `=` takes, and an LFO's rate the nearest of its own kind, a note division staying a
//! note division. A knob that lands where the setting already is gives no action, so it leaves
//! no undo step.

use vt_model::accumulator::{Accumulation, Adds, AtLimit, MOST_DEGREES, Moves, Reset};
use vt_model::action::{Action, TrackControl};
use vt_model::arpeggio::{ArpMode, ArpRate, Arpeggio, MOST_OCTAVES};
use vt_model::constants::{MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::{Adsr, Ahd, Curve};
use vt_model::groove::GrooveId;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{Lfo, LfoMode, Rate, Shape, note_rates};
use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot, Polarity};
use vt_model::settings::{Kind, SettingId, gate_percent, gate_value};
use vt_model::song::{FilterType, Machine, Song, Track, trim_text};
use vt_model::speed::Speed;
use vt_roto::live::{share_of, step_at};

use crate::track_page::{AccumulatorField, ArpField, Control, ModField};
use crate::views::track::{toggle_on, value};

use super::devices::{hold_steps, knob, rate_position, time_steps};

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
        Control::Base(which) => {
            let coarse = match which {
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
        Control::GateLength => {
            let percent = whole(position, 99) as u8 + 1;
            (percent != gate_percent(track.gate_length())).then_some(Action::SetTrackGateLength {
                track: at,
                gate_length: Some(gate_value(percent)),
            })
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
        Control::FilterType => changed(pick(&FilterType::ALL, position), track.filter, |filter| {
            Action::SetFilterType { track: at, filter }
        }),
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
            let (adsr, ahd, lfo) = (&mut modulator.adsr, &mut modulator.ahd, &mut modulator.lfo);
            let ahd_kind = was.kind == ModKind::Ahd;
            match field {
                // What a slot is, the app's alone.
                ModField::Type => return None,
                ModField::Mode => match was.kind {
                    ModKind::Adsr => adsr.curve = pick(&Curve::ALL, position),
                    ModKind::Ahd => ahd.curve = pick(&Curve::ALL, position),
                    ModKind::Lfo => lfo.mode = pick(&LfoMode::ALL, position),
                },
                ModField::Polarity => modulator.polarity = Some(pick(&Polarity::ALL, position)),
                ModField::Destination => {
                    modulator.destination =
                        destination_at(SettingId::for_mod(track, slot), position);
                }
                ModField::Depth => modulator.depth = depth_at(position),
                ModField::Attack if ahd_kind => ahd.attack_ms = time_at(position),
                ModField::Attack => adsr.attack_ms = time_at(position),
                ModField::Decay if ahd_kind => ahd.decay_ms = time_at(position),
                ModField::Decay => adsr.decay_ms = time_at(position),
                ModField::Hold => {
                    let steps = hold_steps();
                    let at = whole(position, steps.len() as u32 - 1) as usize;
                    ahd.hold = steps[at.min(steps.len() - 1)];
                }
                ModField::Release => adsr.release_ms = time_at(position),
                ModField::Sustain => adsr.sustain = whole(position, 100) as u8,
                ModField::Reset => return None,
                ModField::Shape => lfo.shape = pick(&Shape::ALL, position),
                ModField::Rate => lfo.rate = rate_at(lfo.rate, position),
                ModField::Phase => lfo.phase = whole(position, 359) as u16,
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
                AccumulatorField::AtLimit => accumulation.at_limit = pick(&AtLimit::ALL, position),
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
            let to = if found.choices.is_empty() {
                let (id, _) = control.value_on(track)?;
                match id.entry().kind {
                    Kind::Continuous { top, .. } => whole(position, u32::from(top)) as u8,
                    Kind::Stepped { choices, .. } => step_at(position, choices.len()) as u8,
                }
            } else {
                step_at(position, found.choices.len()) as u8
            };
            changed(to, set, |to| Action::SetInsert {
                track: at,
                slot,
                insert: insert.with_value(param, to),
            })
        }
        // The mixer's, which no device has.
        Control::Mute | Control::Solo | Control::Limiter => None,
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
        Control::Mod(slot, ModField::Reset) if track.mods[slot].kind == ModKind::Ahd => {
            Some(Action::SetMod {
                track: at,
                slot,
                modulator: ModSlot {
                    ahd: Ahd {
                        reset: on,
                        ..track.mods[slot].ahd
                    },
                    ..track.mods[slot]
                },
            })
        }
        Control::Mod(slot, ModField::Reset) => Some(Action::SetMod {
            track: at,
            slot,
            modulator: ModSlot {
                adsr: Adsr {
                    reset: on,
                    ..track.mods[slot].adsr
                },
                ..track.mods[slot]
            },
        }),
        // An LFO's mode on its button while it has two, `Trig` lit, until #86's five make it a
        // knob.
        Control::Mod(slot, ModField::Mode) => Some(Action::SetMod {
            track: at,
            slot,
            modulator: ModSlot {
                lfo: Lfo {
                    mode: if on { LfoMode::Trig } else { LfoMode::Free },
                    ..track.mods[slot].lfo
                },
                ..track.mods[slot]
            },
        }),
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
        _ => None,
    }
}

/// A setting's value as the app writes it, for the Roto to show as its knob turns: in its units
/// where it has them, as the status line says it (`-14.8 dB`, `1.21 kHz`), and otherwise as the
/// track page shows it (`1/16`, `Overdrive`, `40 ms`). The Roto cuts it to 12 characters.
pub fn value_text(control: Control, track: &Track, song: &Song) -> String {
    if control == Control::Trim {
        return trim_text(track.trim);
    }
    match control.value_on(track) {
        Some((id, set)) if id.exists_on(track) => id.entry().kind.shown(set),
        _ => value(control, track, song),
    }
}
