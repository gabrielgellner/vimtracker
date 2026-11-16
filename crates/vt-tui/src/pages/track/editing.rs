//! Changing the setting under the cursor: `=` and `-` and their large steps, the fine digits, and
//! `x` back to its default, and `dd` to nothing. Each is a function over the [`Services`], given the setting, as the
//! track page and the mixer both turn them. Every setting the page describes goes one way: its
//! [`view`], `param::edit` for where a key takes it, and [`set`] to write it. Only what the view
//! doesn't write keeps its own arm: the live mute and solo, the song's limiter, a sampler's
//! sample and tune, the groove a track follows, a mod slot's type, rate and fade, and the Gen
//! page's own.

use vt_model::action::Action;
use vt_model::groove::GrooveId;
use vt_model::modulation::{ModField, ModKind, ModSlot, default_mod};
use vt_model::song::Track;

use crate::services::Services;
use crate::widgets::param::edit;

use super::tuning::{adjust_tuning, set_tuning};
use crate::controls::param::ParamView;
use crate::controls::{Control, no_modes, set, view};

/// Write a value into a setting, as its view counts it. A macro's, the accumulator's, the
/// arpeggio's and `Safe` are only written when they change. The rest are written even where they
/// are, as a hand on a fader the steps drive takes it over there.
pub(super) fn write(
    services: &mut Services,
    control: Control,
    track: &Track,
    view: &ParamView,
    value: i32,
) {
    let only_changes = matches!(
        control,
        Control::Macro(_)
            | Control::Accumulator(_)
            | Control::Arpeggio(_)
            | Control::Safe
            | Control::Lane(..)
    );
    if only_changes && value == view.value {
        return;
    }
    let at = services.cursor.track;
    if let Some(action) = set(control, at, track, services.song(), value, None) {
        services.apply(&action);
    }
}

/// The setting's view on this track, where the track has it: on the mixer, the cursor can be on
/// a setting this track doesn't have.
pub(super) fn view_on(services: &Services, control: Control, track: &Track) -> Option<ParamView> {
    control
        .exists_on(track)
        .then(|| view(control, track, services.song(), false))?
}

/// Replace one of the track's mod slots, as one undo step.
pub(super) fn set_mod(services: &mut Services, slot: usize, modulator: ModSlot) {
    services.apply(&Action::SetMod {
        track: services.cursor.track,
        slot,
        modulator,
    });
}

/// Move a mod slot's rate or fade one notch through its own kinds: a division, steps or hertz,
/// and a fade in or out.
fn step_kinds(modulator: &mut ModSlot, field: ModField, up: bool, large: bool) {
    let (steps, lfo) = (&mut modulator.steps, &mut modulator.lfo);
    match (field, modulator.kind, large) {
        (ModField::Fade, ..) => lfo.fade = lfo.fade.nudged(up, large),
        (_, ModKind::Steps, _) => steps.rate = steps.rate.nudged(up, large),
        (_, _, true) => lfo.rate = lfo.rate.jumped(up),
        (_, _, false) => lfo.rate = lfo.rate.nudged(up),
    }
}

#[allow(
    clippy::match_same_arms,
    reason = "an arm for each control, in the page's order"
)]
pub(super) fn adjust_control(
    services: &mut Services,
    control: Control,
    up: bool,
    count: usize,
    large: bool,
) {
    // A fader the steps drive moves from where it's heard, as a hand takes it over there.
    let track = services.page_track_as_heard();
    match control {
        // The Gen page's own keys reshape its generators (`TrackPage::gen_edit`).
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Mute | Control::Solo => {
            let live = services
                .live
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
        Control::Sample => {
            let size = if large { 0x10 } else { 1 };
            let sample = next_sample(services, track.sample(), up, size * count);
            services.apply(&Action::SetTrackSample {
                track: services.cursor.track,
                sample,
            });
        }
        Control::Tune | Control::Cents => adjust_tuning(services, control, up, count, large),
        Control::FilterMode if track.filter.modes().is_empty() => {
            services.status = no_modes(track.filter);
        }
        Control::Groove => {
            // From what it plays, one choice or sixteen, as many times as the count, stopping at
            // either end.
            let from = track.groove.unwrap_or(services.song().groove).index();
            let last = GrooveId::all().count() - 1;
            let by = count.saturating_mul(if large { 0x10 } else { 1 });
            let to = if up {
                from.saturating_add(by).min(last)
            } else {
                from.saturating_sub(by)
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
        Control::Mod(slot, field @ (ModField::Rate | ModField::Fade)) => {
            let mut modulator = track.mods[slot];
            for _ in 0..count.min(0x200) {
                step_kinds(&mut modulator, field, up, large);
            }
            set_mod(services, slot, modulator);
        }
        _ => {
            if let Some(shown) = view_on(services, control, &track) {
                let value = edit::stepped(&shown, up, large, count);
                write(services, control, &track, &shown, value);
            }
        }
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
    // Only a setting the track keeps a coarse byte of has a fine part to move.
    let Some(setting) = control
        .fine_on(&track)
        .filter(|id| track.coarse(*id).is_some())
    else {
        return;
    };
    let Some(shown) = view_on(services, control, &track) else {
        return;
    };
    let (coarse, fine) = edit::stepped_fine(&shown, up, large, count);
    // What's set, which a fader the steps drive may not be at.
    let set_now =
        view_on(services, control, &services.page_track()).map_or(shown.value, |v| v.value);
    let at = services.cursor.track;
    let mut actions = Vec::new();
    if coarse != set_now
        && let Some(action) = set(control, at, &track, services.song(), coarse, None)
    {
        actions.push(action);
    }
    actions.push(Action::SetTrackFine {
        track: at,
        setting,
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

/// A mod slot with one of the settings its view doesn't write back at its default. `Type` goes
/// back to the slot's first type, with that type's settings as they start, keeping where it goes,
/// how far and its switch, as choosing a type does; a polarity goes back to following its type;
/// and a rate and a fade, which aren't one number, to the slot's own.
fn mod_at_default(modulator: ModSlot, slot: usize, field: ModField) -> ModSlot {
    let fresh = default_mod(slot);
    let mut reset = modulator;
    match field {
        ModField::Type => {
            reset.kind = fresh.kind;
            if fresh.kind == ModKind::Adsr {
                reset.adsr = fresh.adsr;
            }
            if fresh.kind == ModKind::Lfo {
                reset.lfo = fresh.lfo;
            }
        }
        ModField::Polarity => reset.polarity = None,
        ModField::Rate if modulator.kind == ModKind::Steps => reset.steps.rate = fresh.steps.rate,
        ModField::Rate => reset.lfo.rate = fresh.lfo.rate,
        ModField::Fade => reset.lfo.fade = fresh.lfo.fade,
        _ => {}
    }
    reset
}

/// Put the setting under the cursor back to its default. If it's there already, nothing changes
/// and no undo step is left.
#[allow(
    clippy::match_same_arms,
    reason = "an arm for each control, in the page's order"
)]
pub(super) fn reset_control(services: &mut Services, control: Control) {
    let track = services.page_track();
    match control {
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Mute | Control::Solo => {
            let live = services
                .live
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
        Control::Sample => {
            services.apply(&Action::SetTrackSample {
                track: services.cursor.track,
                sample: 0,
            });
        }
        Control::Tune | Control::Cents => set_tuning(services, control, None),
        // Back to following the project's.
        Control::Groove if track.groove.is_some() => set_track_groove(services, None),
        Control::Groove => {}
        // Back to the track's default, which isn't one of its percentages.
        Control::GateLength => {
            services.apply(&Action::SetTrackGateLength {
                track: services.cursor.track,
                gate_length: None,
            });
        }
        // The mode it keeps, which its type may not offer.
        Control::FilterMode => {
            services.apply(&Action::SetFilterMode {
                track: services.cursor.track,
                mode: Track::default().filter_mode,
            });
        }
        Control::Mod(
            slot,
            field @ (ModField::Type | ModField::Polarity | ModField::Rate | ModField::Fade),
        ) => set_mod(
            services,
            slot,
            mod_at_default(track.mods[slot], slot, field),
        ),
        _ => {
            if let Some(shown) = view_on(services, control, &track) {
                let default = edit::reset(&shown);
                if shown.value != default {
                    write(services, control, &track, &shown, default);
                }
            }
        }
    }
}

/// `dd`: set the setting under the cursor to nothing, as [`edit::nothing`] reads it from its
/// view, with any fine part at `00`, as one undo step, and say what it set. An effect's setting
/// whose entry is drawn from its middle goes to its middle, though its bar isn't. The live mute
/// and solo and the song's limiter go off, a sampler's sample to `Off`, and its tune and cents to
/// none. What a machine or a modulator is, and a modulator's rate, which isn't one number, have no
/// nothing, and the status line says so.
pub(super) fn to_nothing(services: &mut Services, control: Control) {
    let track = services.page_track();
    match control {
        Control::Gen { .. } | Control::NewGenerator(_) => return,
        Control::Machine => {
            services.status = "dd doesn't change the machine: K picks another".to_owned();
            return;
        }
        Control::Mod(_, ModField::Type) => {
            services.status =
                "dd doesn't change a modulator's type: b switches it out, K picks another"
                    .to_owned();
            return;
        }
        Control::Mod(_, ModField::Rate) => {
            services.status = "A rate has no nothing: x puts it back".to_owned();
            return;
        }
        Control::Mute | Control::Solo => {
            let live = services
                .live
                .levels
                .get(services.cursor.track)
                .copied()
                .unwrap_or_default();
            if (control == Control::Mute && live.mute) || (control == Control::Solo && live.solo) {
                toggle_control(services, control);
            }
        }
        Control::Limiter => {
            if services.song().limiter {
                set_limiter(services, false);
            }
            return;
        }
        Control::Sample => {
            services.apply(&Action::SetTrackSample {
                track: services.cursor.track,
                sample: 0,
            });
        }
        // It says what it set itself.
        Control::Tune | Control::Cents => return set_tuning(services, control, Some(0)),
        _ => {
            let Some(shown) = view_on(services, control, &track) else {
                return;
            };
            let centred = shown.centred
                || matches!(control, Control::InsertParam(slot, param)
                    if track.inserts[slot].params().get(param)
                        .is_some_and(|entry| entry.described().centred));
            let value = edit::nothing_within(shown.bounds, centred);
            let fine = control.fine_on(&track).map(|_| 0);
            let changed = value != shown.value || shown.fine.is_some_and(|fine| fine != 0);
            // Already nothing, it leaves no undo step, as `x` at the default doesn't.
            if changed
                && let Some(action) = set(
                    control,
                    services.cursor.track,
                    &track,
                    services.song(),
                    value,
                    fine,
                )
            {
                services.apply(&action);
            }
        }
    }
    let track = services.page_track();
    if let Some(now) = view_on(services, control, &track) {
        services.status = format!("{} to {}", now.name, now.cell.trim());
    }
}

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
        Control::Safe => services.apply(&Action::SetTrackSafe {
            track: services.cursor.track,
            on: !services.page_track().safe,
        }),
        _ => services.toggle_solo(),
    }
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
