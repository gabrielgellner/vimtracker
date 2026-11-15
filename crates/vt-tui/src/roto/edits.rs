//! A knob turned on the Roto as an edit of the song: the way back from [`super::devices`], a
//! knob's position to the setting's value it stands for, written as a key writes it, by the
//! setting's view and `track_page::set` (#739), and the value as the app writes it, for the Roto to show. Pure, like the mapping: a track and
//! its song in, an action out, so it's tested with no Roto at all.
//!
//! **Where a position lands** is where the mapping puts that value, so a knob left alone sends
//! back what it was sent: a list's step by [`step_at`], a value by its share of the way from
//! the bottom to its top, a setting with fine digits to its fine part, an envelope's time by the
//! steps `=` takes, and an LFO's rate the nearest of its own kind, a note division staying a
//! note division. A knob that lands where the setting already is gives no action, so it leaves
//! no undo step.

use vt_model::action::Action;
use vt_model::envelope::Hold;
use vt_model::lfo::{Rate, note_rates};
use vt_model::modulation::{ModField, ModKind, ModSlot};
use vt_model::settings::{Kind, SettingId};
use vt_model::shape::GenParam;
use vt_model::shape::params::Units as GenUnits;
use vt_model::song::{Song, Track, trim_text};
use vt_model::steps::StepRate;
use vt_roto::live::{share_of, step_at};

use crate::shaping::{live_shown, with_live};
use crate::track_page::tuning::{tuning, tuning_most};
use crate::track_page::{Control, ParamView, set, view};
use crate::views::track::toggle_on;
use crate::views::widgets::param::edit;

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
