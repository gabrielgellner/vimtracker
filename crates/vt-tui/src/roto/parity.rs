//! A temporary check that the Roto's knobs read from the settings' views feel and turn as the
//! old per-setting code had them (#784): the old `feel`, `gen_feel` and `gen_turned`, kept here
//! word for word, against the new, on every knob of every kind of device and generator.

#![allow(clippy::all)]

use vt_model::accumulator::{MOST_DEGREES, Reset};
use vt_model::action::Action;
use vt_model::arpeggio::MOST_OCTAVES;
use vt_model::constants::{MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::Hold;
use vt_model::insert::Insert;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{Fade, Rate, note_rates};
use vt_model::listed::{Listed, choices_of};
use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot, Polarity};
use vt_model::settings::{Kind, SettingId};
use vt_model::shape::GenParam;
use vt_model::shape::params::Units as GenUnits;
use vt_model::song::{Song, Track};
use vt_model::steps::{Loop, StepRate};
use vt_roto::Label;
use vt_roto::live::{Feel, LiveControl, position_at, position_of, share_of, step_at};
use vt_roto::sysex::MAX_STEPS;

use super::devices::{family, hold_steps, name, rate_position, time_position, time_steps};
use crate::shaping::with_live;
use crate::track_page::tuning::{tuning, tuning_most};
use crate::track_page::{AccumulatorField, ArpField, Control, ModField, ParamView, set, view};
use crate::views::track::toggle_on;
use crate::views::widgets::param::edit;

/// The old knob.
pub fn knob(control: Control, track: &Track, song: &Song) -> Option<LiveControl> {
    let (feel, position) = feel(control, track, song)?;
    Some(LiveControl {
        name: name(control, track),
        feel,
        position,
        colour: family(control, track).scheme(),
    })
}

/// A knob for one of a list of choices, the `at`th: stepped and named for up to 24, smooth past
/// that, as a list that long is turned through rather than clicked.
fn list(names: impl IntoIterator<Item = String>, at: usize) -> (Feel, u16) {
    let names: Vec<Label> = names.into_iter().map(|name| Label::new(&name)).collect();
    let count = names.len();
    let position = position_of(at, count);
    if (2..=usize::from(MAX_STEPS)).contains(&count) {
        (Feel::Steps(names), position)
    } else {
        (Feel::Smooth, position)
    }
}

/// A knob for one of a [`Listed`] type's values, on the one it has.
fn listed<X: Listed>(current: X) -> (Feel, u16) {
    let at = X::ALL.iter().position(|each| *each == current).unwrap_or(0);
    list(choices_of::<X>().map(str::to_owned), at)
}

/// A knob for one of a list that changes with the track or the song, such as a destination: it
/// turns smoothly, as its steps would move under it.
fn changing(at: usize, count: usize) -> (Feel, u16) {
    (Feel::Smooth, position_of(at, count))
}

/// A knob for a value from 0 to a top, centred or not.
fn value(value: u16, top: u16, centred: bool) -> (Feel, u16) {
    let feel = if centred {
        Feel::CentreNotch
    } else {
        Feel::Smooth
    };
    (feel, position_at(f64::from(value) / f64::from(top.max(1))))
}

/// A knob for a setting in the registry, with its coarse value and fine part together.
fn registry(id: SettingId, coarse: u8, fine: u8) -> (Feel, u16) {
    match id.entry().kind {
        Kind::Continuous { top, middle, .. } => {
            let whole = u16::from(coarse.min(top)) << 8 | u16::from(fine);
            value(whole, u16::from(top) << 8, middle)
        }
        Kind::Stepped { choices, .. } => list(
            choices.iter().map(|choice| (*choice).to_owned()),
            usize::from(coarse),
        ),
    }
}
/// How far along an AHD's holds one is, as [`hold_steps`] has them.
fn hold_position(hold: Hold) -> u16 {
    let steps = hold_steps();
    let at = match hold {
        Hold::Note => 0,
        Hold::Ms(ms) => steps
            .iter()
            .position(|step| matches!(step, Hold::Ms(at) if *at >= ms))
            .unwrap_or(steps.len() - 1),
    };
    position_at(at as f64 / (steps.len() - 1) as f64)
}

/// A generator's setting as a knob: a list or a switch stepped through its choices, named as the
/// page names them, and a number smooth from its lowest to its highest, a bend or an interval
/// with a notch at its middle. A `List`'s values have none.
fn gen_feel(
    song: &Song,
    track: &Track,
    phrase: u32,
    index: usize,
    param: GenParam,
) -> Option<(Feel, u16)> {
    with_live(song, track, phrase, index, |generator, context| {
        let shape = &generator.shape;
        let (low, high) = param.bounds(context)?;
        let number = param.number(shape, context)?;
        if param.units() == GenUnits::Named {
            let names = (low..=high).map(|at| {
                let mut named = *shape;
                param.set_number(&mut named, context, at);
                param.shown(&named, context)
            });
            return Some(list(names, (number - low) as usize));
        }
        let centred = matches!(param.units(), GenUnits::Bend | GenUnits::Interval);
        Some(value((number - low) as u16, (high - low) as u16, centred))
    })
    .flatten()
}
/// A depth either side of none, as a mod slot's.
fn depth(depth: i16) -> (Feel, u16) {
    let top = MAX_DEPTH;
    value(
        (depth.clamp(-top, top) + top) as u16,
        (2 * top) as u16,
        true,
    )
}

/// Whether a mod slot's two-way choice is on, as its button shows it: any slot's polarity is `Bi`
/// (the button under its `Depth`).
fn mod_switch_on(control: Control, track: &Track) -> Option<bool> {
    match control {
        Control::Mod(slot, ModField::Polarity) => Some(track.mods[slot].polarity() == Polarity::Bi),
        _ => None,
    }
}

/// A knob's feel and where it's set, for a control on this track, or nothing if the track
/// hasn't the setting, as a fixed track hasn't `Pitch`, or it has nothing to choose, as a filter
/// type without modes has for `Mode`.
fn feel(control: Control, track: &Track, song: &Song) -> Option<(Feel, u16)> {
    if !control.exists_on(track) {
        return None;
    }
    if let Some(on) = toggle_on(control, track, song).or_else(|| mod_switch_on(control, track)) {
        return Some((Feel::Switch, if on { position_of(1, 2) } else { 0 }));
    }
    let names = |items: &[&str]| {
        items
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    let found = match control {
        Control::Gen {
            phrase,
            generator,
            param,
        } => return gen_feel(song, track, phrase, generator, param),
        Control::NewGenerator(_) => return None,
        Control::Machine => listed(track.machine),
        Control::Sample => {
            let numbers: Vec<u8> = std::iter::once(0)
                .chain(song.bank.iter().map(|(number, _)| number))
                .collect();
            let at = numbers
                .iter()
                .position(|number| *number == track.sample())
                .unwrap_or(0);
            changing(at, numbers.len())
        }
        Control::Setting(_) => {
            let id = control.setting_on(track)?;
            registry(id, track.setting(id), track.fine(id))
        }
        Control::Macro(at) => {
            let [coarse, fine] = track.macros[at].value.to_be_bytes();
            registry(SettingId::macro_at(at)?, coarse, fine)
        }
        // The sample's own, either side of none with a notch there, and nothing with no sample.
        Control::Tune | Control::Cents => {
            let set = tuning(song, track)?.value(control);
            let most = i16::from(tuning_most(control));
            value((i16::from(set) + most) as u16, (2 * most) as u16, true)
        }
        Control::Base(which) => {
            let coarse = track.control(which);
            match which.setting() {
                Some(id) => registry(id, coarse, track.fine(id)),
                None => value(u16::from(coarse), 0xFF, false),
            }
        }
        Control::Level => registry(SettingId::LEVEL, track.level, 0),
        // `80` is 0 dB, so the notch is where it neither cuts nor boosts.
        Control::Trim => value(u16::from(track.trim), 0xFF, true),
        Control::Groove => {
            let grooves: Vec<_> = vt_model::groove::GrooveId::all().collect();
            let playing = track.groove.unwrap_or(song.groove);
            list(
                grooves.iter().map(|groove| groove.name().to_owned()),
                grooves
                    .iter()
                    .position(|groove| *groove == playing)
                    .unwrap_or(0),
            )
        }
        Control::Choke => list(
            std::iter::once("Off".to_owned()).chain((1..=CHOKE_GROUPS).map(|g| g.to_string())),
            usize::from(track.choke.unwrap_or(0)),
        ),
        Control::Group => list(
            std::iter::once("Off".to_owned()).chain((1..=MAX_GROUPS).map(|g| format!("G{g}"))),
            usize::from(track.group.unwrap_or(0)),
        ),
        Control::Role => listed(track.role),
        Control::GateLength => value(
            u16::from(vt_model::settings::gate_percent(track.gate_length())) - 1,
            99,
            false,
        ),
        Control::Speed => listed(track.speed),
        Control::Reset => value(track.reset.unwrap_or(0), MAX_TRACK_RESET, false),
        Control::Glide => value(u16::from(track.glide), 0xFF, false),
        Control::FilterType => listed(track.filter),
        Control::FilterMode => {
            let modes = track.filter.modes();
            if modes.is_empty() {
                return None;
            }
            let playing = track.filter.plays(track.filter_mode);
            list(
                modes.iter().map(|mode| mode.name().to_owned()),
                modes.iter().position(|mode| *mode == playing).unwrap_or(0),
            )
        }
        Control::Mod(slot, field) => {
            let modulator = &track.mods[slot];
            match field {
                // What a slot is, the app's alone.
                ModField::Type | ModField::Polarity | ModField::Reset => return None,
                ModField::Destination => {
                    let choices = SettingId::for_mod(track, slot);
                    let at = modulator
                        .destination
                        .and_then(|to| choices.iter().position(|choice| *choice == to))
                        .map_or(0, |at| at + 1);
                    return Some(changing(at, choices.len() + 1));
                }
                // Steps' rate is a musical list, which leaves the layout as it is.
                ModField::Rate if modulator.kind == ModKind::Steps => {
                    let steps = &modulator.steps;
                    return Some(list(
                        StepRate::ALL.iter().map(ToString::to_string),
                        StepRate::ALL
                            .iter()
                            .position(|rate| *rate == steps.rate)
                            .unwrap_or(0),
                    ));
                }
                // Smooth over every kind of rate, its value shown as it turns: a stored knob keeps
                // at most 16 steps, and a note value has 24 (the author's choice, 2026-11-11).
                ModField::Rate => return Some((Feel::Smooth, rate_position(modulator.lfo.rate))),
                // Smooth, its value shown as it turns, with `Note` at the bottom.
                ModField::Hold => return Some((Feel::Smooth, hold_position(modulator.ahd.hold))),
                _ => {}
            }
            let at = modulator.field(field)?;
            let (lowest, highest) = modulator.bounds(field)?;
            let place = (at.clamp(lowest, highest) - lowest) as usize;
            let top = (highest - lowest) as u16;
            match field {
                // An envelope's curve, how notes start an LFO or Steps, and what Track follows are
                // musical lists, which leave the layout as it is.
                ModField::Mode => list(
                    modulator
                        .kind
                        .modes()
                        .into_iter()
                        .map(|(name, _)| name.to_owned()),
                    place,
                ),
                ModField::Shape => listed(modulator.lfo.shape),
                // Smooth through every fade, out, none and in, its value shown as it turns.
                ModField::Fade => list(Fade::ALL.iter().map(ToString::to_string), place),
                ModField::Loop => list(Loop::ALL.iter().map(ToString::to_string), place),
                // Counts and places, numbered from 1.
                ModField::Length | ModField::Start | ModField::Ratchet => list(
                    (1..=usize::from(top) + 1).map(|count| count.to_string()),
                    place,
                ),
                ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak => {
                    (Feel::Smooth, time_position(at as u16))
                }
                // Each either way of none, with the depth's centre notch.
                ModField::Depth | ModField::Deform | ModField::Smooth | ModField::Step(_) => {
                    depth(at as i16)
                }
                // Smooth from its lowest to its highest: a level, a percentage, degrees, or Track's
                // map across its source's values, its note or hex shown as it turns.
                _ => value(place as u16, top, false),
            }
        }
        Control::Accumulator(field) => {
            let accumulation = track.accumulation;
            let degrees = |at: u8| {
                list(
                    (1..=MOST_DEGREES).map(|degrees| degrees.to_string()),
                    usize::from(at.saturating_sub(1)),
                )
            };
            match field {
                AccumulatorField::Up => degrees(accumulation.up),
                AccumulatorField::Down => degrees(accumulation.down),
                AccumulatorField::Moves => listed(accumulation.moves),
                AccumulatorField::AtLimit => listed(accumulation.at_limit),
                AccumulatorField::Adds => listed(accumulation.adds),
                AccumulatorField::Reset => {
                    let resets: Vec<Reset> = Reset::all().collect();
                    list(
                        resets.iter().map(|reset| reset.name()),
                        resets
                            .iter()
                            .position(|reset| *reset == accumulation.reset)
                            .unwrap_or(0),
                    )
                }
            }
        }
        Control::Arpeggio(field) => {
            let arpeggio = track.arpeggio;
            match field {
                ArpField::Rate => listed(arpeggio.rate),
                ArpField::Mode => listed(arpeggio.mode),
                ArpField::Octaves => list(
                    (1..=MOST_OCTAVES).map(|octaves| octaves.to_string()),
                    usize::from(arpeggio.octaves.saturating_sub(1)),
                ),
                ArpField::Retrig => return None,
            }
        }
        Control::Insert(slot) => list(
            Insert::ALL.iter().map(|insert| insert.name().to_owned()),
            Insert::ALL
                .iter()
                .position(|insert| insert.name() == track.inserts[slot].name())
                .unwrap_or(0),
        ),
        Control::InsertParam(slot, param) => {
            let insert = track.inserts[slot];
            let found = insert.params().get(param)?;
            let set = insert.value(param);
            if found.choices().is_empty() {
                let (id, _) = control.value_on(track)?;
                registry(id, set, 0)
            } else {
                list(names(found.choices()), usize::from(set))
            }
        }
        // The mixer's, which no device has, and the off/on settings, which are switches.
        Control::Mute | Control::Solo | Control::Limiter | Control::Safe => return None,
    };
    Some(found)
}

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

/// A destination among a list of choices with none first, at a position.
fn destination_at(choices: Vec<SettingId>, position: u16) -> Option<SettingId> {
    let at = step_at(position, choices.len() + 1);
    at.checked_sub(1).map(|at| choices[at])
}

/// What the knob for a control on a track, the `at`th of the song's, does turned to a position:
/// the action that puts its setting there, or nothing if it's there already or the knob is a
/// hole. A knob takes the keys' way: it finds the value its position stands for in the setting's
/// view, by `param::edit`, and writes it with [`set`], so only the sample, its tune, a mod slot's
/// destination and rate and a generator's settings have edits of their own here.
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
    let share = share_of(position);
    match control {
        Control::Gen {
            phrase,
            generator,
            param,
        } => return gen_turned(song, at, phrase, generator, param, position),
        Control::NewGenerator(_) | Control::Mute | Control::Solo => return None,
        Control::Sample => {
            let numbers: Vec<u8> = std::iter::once(0)
                .chain(song.bank.iter().map(|(number, _)| number))
                .collect();
            return changed(pick(&numbers, position), track.sample(), |sample| {
                Action::SetTrackSample { track: at, sample }
            });
        }
        // The sample's own, as an edit of its bank entry, as the keys make.
        Control::Tune | Control::Cents => {
            let tuning = tuning(song, track)?;
            let most = i32::from(tuning_most(control));
            let to = whole(position, (2 * most) as u32) as i32 - most;
            let entry = tuning.with(control, Some(to as i8));
            return (entry != *tuning.entry).then_some(Action::SetBankEntry {
                number: tuning.number,
                entry: Some(entry),
            });
        }
        // The knob's list of destinations has `Pitch` once, keeping whether it's in scale.
        Control::Mod(slot, ModField::Destination) => {
            let was = track.mods[slot];
            let modulator = ModSlot {
                destination: destination_at(SettingId::for_mod(track, slot), position),
                ..was
            };
            return changed(modulator, was, |modulator| Action::SetMod {
                track: at,
                slot,
                modulator,
            });
        }
        Control::Mod(slot, ModField::Rate) => {
            let was = track.mods[slot];
            let mut modulator = was;
            match was.kind {
                ModKind::Steps => modulator.steps.rate = pick(&StepRate::ALL, position),
                _ => modulator.lfo.rate = rate_at(was.lfo.rate, position),
            }
            return changed(modulator, was, |modulator| Action::SetMod {
                track: at,
                slot,
                modulator,
            });
        }
        // What a slot is, the app's alone.
        Control::Mod(_, ModField::Type) => return None,
        _ => {}
    }
    let shown = view(control, track, song, false)?;
    let steps = || edit::at_place(&shown, step_at(position, edit::places(&shown)));
    // A switch by a two-step knob.
    if toggle_on(control, track, song).is_some() {
        return write(
            control,
            at,
            track,
            song,
            &shown,
            i32::from(step_at(position, 2) == 1),
        );
    }
    let value = match control {
        Control::Setting(_) | Control::Base(_) => {
            // The registry's, which a setting of the track's own may not be.
            let kind = control
                .setting_on(track)
                .or_else(|| match control {
                    Control::Base(which) => which.setting(),
                    _ => None,
                })
                .map(|id| id.entry().kind);
            // A knob goes as far as the registry's top, which on the level scale is short of
            // where the keys stop.
            let knob = match kind {
                Some(Kind::Stepped { choices, .. }) => {
                    return write(
                        control,
                        at,
                        track,
                        song,
                        &shown,
                        edit::at_place(&shown, step_at(position, choices.len())),
                    );
                }
                Some(Kind::Continuous { top, .. }) => ParamView {
                    bounds: (0, i32::from(top)),
                    ..shown.clone()
                },
                None => shown.clone(),
            };
            match shown.fine {
                Some(_) => {
                    let (value, fine) = edit::shared_fine(&knob, share);
                    return with_fine(control, at, track, song, &shown, value, fine);
                }
                None => edit::shared(&knob, share),
            }
        }
        // The level scale stops short of the fader's top.
        Control::Level => {
            let top = SettingId::LEVEL.entry().kind.top();
            whole(position, u32::from(top)) as i32
        }
        Control::Mod(
            _,
            ModField::Attack | ModField::Decay | ModField::Release | ModField::Peak,
        ) => i32::from(time_at(position)),
        Control::Mod(_, ModField::Hold) => {
            let steps = hold_steps();
            let at = whole(position, steps.len() as u32 - 1) as usize;
            match steps[at.min(steps.len() - 1)] {
                Hold::Note => -1,
                Hold::Ms(ms) => i32::from(ms),
            }
        }
        // One of a list, or a count or a place, by the step the knob is on.
        Control::Mod(
            _,
            ModField::Mode
            | ModField::Polarity
            | ModField::Shape
            | ModField::Fade
            | ModField::Loop
            | ModField::Length
            | ModField::Start
            | ModField::Ratchet,
        ) => steps(),
        // An effect's value by its registry's, as a step locks it.
        Control::InsertParam(..) if shown.choices.is_empty() => {
            let (id, _) = control.value_on(track)?;
            match id.entry().kind {
                Kind::Continuous { top, .. } => whole(position, u32::from(top)) as i32,
                Kind::Stepped { choices, .. } => step_at(position, choices.len()) as i32,
            }
        }
        // A number from its lowest to its highest.
        Control::Macro(_)
        | Control::Trim
        | Control::GateLength
        | Control::Reset
        | Control::Glide
        | Control::Mod(..) => edit::shared(&shown, share),
        _ => steps(),
    };
    write(control, at, track, song, &shown, value)
}

/// The action that puts a setting at a value, if that's a change. A mod slot's is a change if it
/// changes the slot, as a polarity that follows its type becomes its own.
fn write(
    control: Control,
    at: usize,
    track: &Track,
    song: &Song,
    shown: &ParamView,
    value: i32,
) -> Option<Action> {
    let action = set(control, at, track, song, value, None)?;
    let changes = match (&action, control) {
        (Action::SetMod { modulator, .. }, Control::Mod(slot, _)) => *modulator != track.mods[slot],
        _ => value != shown.value,
    };
    changes.then_some(action)
}

/// The actions that put a setting with a fine part at a value and a fine byte, each only where
/// it's a change.
fn with_fine(
    control: Control,
    at: usize,
    track: &Track,
    song: &Song,
    shown: &ParamView,
    value: i32,
    fine: u8,
) -> Option<Action> {
    let mut actions = Vec::new();
    actions.extend(write(control, at, track, song, shown, value));
    if Some(fine) != shown.fine {
        actions.push(Action::SetTrackFine {
            track: at,
            setting: control.fine_on(track)?,
            fine,
        });
    }
    match actions.len() {
        0 => None,
        1 => actions.pop(),
        _ => Some(Action::Batch(actions)),
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

mod check {
    use std::sync::Arc;

    use vt_model::accumulator::MOST_DEGREES;
    use vt_model::action::{Action, edit as apply};
    use vt_model::arpeggio::MOST_OCTAVES;
    use vt_model::bank::{BankEntry, ContentHash};
    use vt_model::envelope::Hold;
    use vt_model::generator::{Generator, Span};
    use vt_model::groove::GrooveId;
    use vt_model::insert::Insert;
    use vt_model::lfo::Shape;
    use vt_model::macros::{GEN_ZERO, GenColumn, MacroMap, MacroTarget};
    use vt_model::modulation::{ModKind, Polarity};
    use vt_model::scale::Scale;
    use vt_model::settings::SettingId;
    use vt_model::shape::{
        Acid, Bits, DEFAULT_RATE, FillKind, Follow, GenParam, Pink, Random, ShapeColumn, Values,
        Walk, Where, World,
    };
    use vt_model::song::{FilterMode, FilterType, Machine, Phrase, Role, Song, TrackKind};
    use vt_model::speed::Speed;
    use vt_model::step::Step;
    use vt_model::tracking::Source;
    use vt_roto::live::KNOB_TOP;

    use crate::roto::devices::{every_kind_of_device, placement, track_devices};
    use crate::track_page::{Control, DeviceSpec, Page};

    fn shape(kind: FillKind, place: Where) -> vt_model::shape::Shape {
        vt_model::shape::Shape {
            from: 0x30,
            to: 0x48,
            curve: 0,
            s: 0,
            kind,
            wave: vt_model::lfo::Shape::Sine,
            rate: DEFAULT_RATE,
            phase: 0,
            run: 1,
            values: Values::new(&[0x30, 0x40]),
            place,
            seed: 1,
            acid: Acid::default(),
            follow: Follow::default(),
            walk: Walk::default(),
            pink: Pink::default(),
            bits: Bits::default(),
            grids: vt_model::shape::Grids::default(),
            random: Random::default(),
            vary: 0,
            variation: vt_model::shape::Variation::default(),
            divisions: vt_model::shape::Divisions::default(),
            ring: vt_model::shape::Ring::default(),
            tonnetz: vt_model::shape::Tonnetz::default(),
        }
    }

    /// A song whose first track's phrase `00` is live with this shape on its Note column, and
    /// its macros holding some of its settings when `held`.
    fn song_with(shape: vt_model::shape::Shape, held: bool) -> Song {
        let mut song = Song::new(1);
        let steps: Vec<Step> = (0..16)
            .map(|_| "C-3 40 --- --- ---".parse().unwrap())
            .collect();
        let mut phrase = Phrase::from_steps(steps);
        let column = ShapeColumn::Note {
            scale: Scale::default(),
            writes: None,
        };
        let generator = Generator::new(column, Span::Rows { first: 0, last: 15 }, shape);
        assert!(phrase.add_generator(generator, None));
        song.tracks[0].phrases.insert(0, Arc::new(phrase));
        if !held {
            return song;
        }
        let maps = [
            GenParam::To,
            GenParam::Curve,
            GenParam::Hits,
            GenParam::Wave,
        ];
        for (at, param) in maps.into_iter().enumerate() {
            song.tracks[0].macros[at].maps.push(MacroMap {
                target: MacroTarget::Gen {
                    column: GenColumn::Note,
                    param,
                },
                min: (GEN_ZERO as u16),
                max: (GEN_ZERO as u16).wrapping_add(3),
                curve: 0,
                s: 0,
            });
        }
        let song = Arc::new(song);
        let action = Action::SetMacro {
            track: 0,
            at: 0,
            value: 0x8000,
        };
        let next = apply(&song, &action).unwrap();
        (*next).clone()
    }

    fn generator_songs() -> Vec<Song> {
        let wheres = [
            Where::Notes,
            Where::Every(4),
            Where::Chance(50),
            Where::Euclid { hits: 3, rotate: 0 },
            Where::Clusters(50),
            Where::Markov(80),
            Where::States(25),
            Where::Complementary {
                share: 50,
                second: true,
            },
            Where::Rhythm {
                rhythm: World::Bembe,
                part: vt_model::shape::Role::High,
                child: 70,
            },
        ];
        let mut songs = Vec::new();
        for kind in FillKind::ALL {
            for (at, place) in wheres.into_iter().enumerate() {
                let mut shape = shape(kind, place);
                shape.follow.mutation = 2;
                songs.push(song_with(shape, at % 3 == 1));
            }
        }
        let held = songs
            .iter()
            .filter(|song| song.tracks[0].phrases[&0].generators()[0].is_held())
            .count();
        assert!(held > 20, "{held} held");
        songs
    }

    /// A song of many tracks, each with its settings off their defaults in its own way.
    fn varied() -> Song {
        let mut song = Song::new(Machine::ALL.len());
        song.limiter = false;
        song.bank
            .set(1, Some(BankEntry::of_file("kick.wav", ContentHash(1))));
        song.bank
            .set(5, Some(BankEntry::of_file("snare.wav", ContentHash(2))));
        let sound = song
            .tracks
            .iter_mut()
            .filter(|track| track.kind == TrackKind::Sound);
        for (at, track) in sound.enumerate() {
            track.machine = Machine::ALL[at % Machine::ALL.len()];
            if track.machine == Machine::Sampler {
                track.set_setting(SettingId::SAMPLE, [1, 5, 0][at % 3]);
            }
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
            track.group = [None, Some(1), Some(3)][(at + 1) % 3];
            track.role = Role::ALL[at % Role::ALL.len()];
            track.speed = Speed::ALL[at % Speed::ALL.len()];
            track.filter = FilterType::ALL[at % FilterType::ALL.len()];
            track.filter_mode = FilterMode::ALL[(at / 2) % FilterMode::ALL.len()];
            track.safe = at % 2 == 1;
            track.level = [0, 0xFF, 0x80][at % 3];
            track.pan = [0, 0xFF, 0x80][(at + 1) % 3];
            track.trim = [0, 0xFF, 0x80][(at + 2) % 3];
            track.macros[at % 8].value = [0, 0xFF00, 0x1234][at % 3];
        }
        song
    }

    /// Every control a device has: on its grid, and on its knobs and buttons stored and live.
    fn controls_of(spec: &DeviceSpec, song: &Song, at: usize) -> Vec<Control> {
        let track = &song.tracks[at];
        let mut controls: Vec<Control> = spec.rows().flatten().flatten().collect();
        for stored in [true, false] {
            let placed = placement(spec, track, stored);
            controls.extend(
                placed
                    .knobs
                    .iter()
                    .chain(&placed.buttons)
                    .flatten()
                    .map(|(control, _)| *control),
            );
        }
        let mut unique = Vec::new();
        for control in controls {
            if !unique.contains(&control) {
                unique.push(control);
            }
        }
        unique
    }

    struct Count {
        knobs: usize,
        turns: usize,
        feels: usize,
    }

    /// One control's knob, and its turns at every position with the knob as each leaves it.
    fn same(song: &Arc<Song>, at: usize, control: Control, positions: &[u16], count: &mut Count) {
        let track = &song.tracks[at];
        let old = super::knob(control, track, song);
        let new = crate::roto::devices::knob(control, track, song);
        assert_eq!(old, new, "{control:?} on track {at}");
        count.knobs += 1;
        for &position in positions {
            let old = super::turned(control, at, song, position);
            let new = crate::roto::edits::turned(control, at, song, position);
            count.turns += 1;
            if old == new {
                // Not one a song refuses, as a `Follow` of its own phrase.
                if let Some(Ok(next)) = new.as_ref().map(|action| apply(song, action)) {
                    let action = new.unwrap();
                    let track = &next.tracks[at];
                    let old = super::knob(control, track, &next);
                    let new = crate::roto::devices::knob(control, track, &next);
                    assert_eq!(old, new, "{control:?} on track {at} after {action:?}");
                    count.feels += 1;
                }
                continue;
            }
            let after = |action: &Option<Action>| {
                action.as_ref().map(|action| {
                    apply(song, action).map(|next| {
                        format!(
                            "{:?}",
                            (next.tracks[at].clone(), next.limiter, next.bank.clone())
                        )
                    })
                })
            };
            assert!(
                after(&old) == after(&new),
                "{control:?} on track {at} at {position}: {old:?} against {new:?}"
            );
        }
    }

    #[test]
    fn every_knob_feels_and_turns_as_it_did() {
        let positions: Vec<u16> = (0..=KNOB_TOP)
            .step_by(127)
            .chain([KNOB_TOP, 1, 8191, 8192])
            .collect();
        let mut count = Count {
            knobs: 0,
            turns: 0,
            feels: 0,
        };
        for (song, at, page, slot) in every_kind_of_device() {
            let spec = page.devices(&song.tracks[at])[slot].clone();
            let song = Arc::new(song);
            for control in controls_of(&spec, &song, at) {
                same(&song, at, control, &positions, &mut count);
            }
        }
        for song in generator_songs() {
            let devices = track_devices(&song.tracks[0], Some(0));
            let song = Arc::new(song);
            for (page, _, spec) in devices {
                if page != Page::Gen {
                    continue;
                }
                for control in controls_of(&spec, &song, 0) {
                    same(&song, 0, control, &positions, &mut count);
                }
            }
        }
        let song = Arc::new(varied());
        for at in 0..song.tracks.len() {
            for (_, _, spec) in track_devices(&song.tracks[at], None) {
                for control in controls_of(&spec, &song, at) {
                    same(&song, at, control, &positions, &mut count);
                }
            }
        }
        assert!(count.knobs > 3_000, "{}", count.knobs);
        assert!(count.turns > 300_000, "{}", count.turns);
        assert!(count.feels > 50_000, "{}", count.feels);
        eprintln!(
            "{} knobs, {} turns, {} feels after a turn",
            count.knobs, count.turns, count.feels
        );
    }
}
