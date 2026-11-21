//! `i` on a setting typed as a value: its digits or words, and what they set.

use ratatui::crossterm::event::KeyCode;

use vt_model::lfo::{Fade, Lfo, Rate};
use vt_model::modulation::{ModField, ModKind, ModSlot};
use vt_model::shape::params::Typing;
use vt_model::steps::{STEPS, StepRate, Steps};

use crate::app::Entry;
use crate::input::mode::View;
use crate::input::typed::{HEX, Rule, Typed};
use crate::services::Services;
use crate::widgets::param::edit;

use super::TrackPage;
use super::editing::{set_fine_here, set_mod, view_on, write};
use super::generators::{gen_typing, type_gen};
use super::tuning::type_tuning;
use crate::controls::Control;

/// The longest a generator's value can be typed: a list of eight notes and the spaces between.
const GEN_TEXT: usize = 40;
/// The longest a Steps slot's values can be typed, from the cursor's step: all of them, each a
/// sign, two digits and a space (decision 459).
const STEPS_TEXT: usize = 4 * STEPS;

impl TrackPage {
    /// Type a value into `entry` on the setting `control`: two hex digits for levels, and anything
    /// else then `Enter`. Returns false once Insert is done with, written or let go.
    pub(crate) fn insert(
        &self,
        key: KeyCode,
        control: Control,
        view: View,
        entry: &mut Entry,
        services: &mut Services,
    ) -> bool {
        // A generator's ends on a column of hex values enter themselves, as levels do, and a
        // list of notes takes more room.
        let (hex, longest) = match control {
            Control::Gen { .. } => (gen_typing(services, control) == Some(Typing::Hex), GEN_TEXT),
            Control::Mod(_, ModField::Step(_)) => (false, STEPS_TEXT),
            _ => (control.is_hex(), 12),
        };
        let rule = if hex { HEX } else { Rule::Text { longest } };
        match entry.text.key(key, rule) {
            Typed::Editing => true,
            Typed::Cancel => {
                *entry = Entry::default();
                false
            }
            Typed::Commit(text) => {
                if hex && self.fine {
                    if let Ok(fine) = u8::from_str_radix(&text, 16) {
                        set_fine_here(services, control, fine);
                    }
                } else if !text.is_empty() {
                    commit_typed(services, view, control, &text);
                }
                false
            }
        }
    }
}

/// Write what was typed into the setting: a generator's into its shape, and the rest as the
/// track page's settings take it.
fn commit_typed(services: &mut Services, view: View, control: Control, text: &str) {
    match control {
        Control::Gen { .. } => type_gen(services, view, control, text),
        _ => commit_control(services, control, text),
    }
}

/// Write what was typed into one of the track's settings, as its view reads it, saying on the
/// status line why where it doesn't.
pub(crate) fn commit_control(services: &mut Services, control: Control, text: &str) {
    let track = services.page_track();
    match control {
        // A generator's are typed into its shape, by `commit_typed`.
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Tune | Control::Cents => type_tuning(services, control, text),
        Control::Mod(slot, field @ (ModField::Rate | ModField::Fade)) => {
            type_kind(services, slot, field, text);
        }
        Control::Mod(slot, ModField::Step(step)) if text.split_whitespace().count() > 1 => {
            type_steps(services, slot, step, text);
        }
        _ => {
            let Some(shown) = view_on(services, control, &track) else {
                return;
            };
            match edit::typed(&shown, text) {
                Ok(value) => write(services, control, &track, &shown, value),
                Err(why) if !why.is_empty() => services.status = why,
                Err(_) => {}
            }
        }
    }
}

/// A Steps slot's values typed as a list, `0C 00 -07 -05`, from the cursor's step on, as one undo
/// step: each value signed hex, as one step's is typed, and `Length` taken out to the last if it
/// stopped short of it (decision 459). Refused whole if any value doesn't read, or the list runs
/// past the last step.
fn type_steps(services: &mut Services, slot: usize, step: u8, text: &str) {
    let track = services.page_track();
    let mut modulator = track.mods[slot];
    let first = usize::from(step);
    let values: Vec<&str> = text.split_whitespace().collect();
    if first + values.len() > STEPS {
        services.status = format!(
            "{} values from step {} run past the last of {STEPS}",
            values.len(),
            first + 1
        );
        return;
    }
    for (place, value) in (first..).zip(&values) {
        let control = Control::Mod(slot, ModField::Step(place as u8));
        let Some(shown) = view_on(services, control, &track) else {
            return;
        };
        match edit::typed(&shown, value) {
            Ok(typed) => modulator.steps.values[place] = typed as i16,
            Err(why) => {
                services.status = if why.is_empty() {
                    format!("{value} isn't a step's value, signed hex such as -0C")
                } else {
                    format!("{value}: {why}")
                };
                return;
            }
        }
    }
    let last = first + values.len();
    if modulator.steps.played_count() < last {
        modulator.steps.length = last as u8;
    }
    set_mod(services, slot, modulator);
    services.status = format!("{} steps typed from step {}", values.len(), first + 1);
}

/// Set a mod slot's rate or fade from what was typed, read as it's written, as neither is one
/// number.
fn type_kind(services: &mut Services, slot: usize, field: ModField, text: &str) {
    let current = services.page_track().mods[slot];
    let written = match field {
        ModField::Rate if current.kind == ModKind::Steps => text
            .parse::<StepRate>()
            .map(|rate| {
                let steps = Steps {
                    rate,
                    ..current.steps
                };
                ModSlot { steps, ..current }
            })
            .map_err(|error| error.to_string()),
        ModField::Rate => text
            .parse::<Rate>()
            .map(|rate| {
                let lfo = Lfo {
                    rate,
                    ..current.lfo
                };
                ModSlot { lfo, ..current }
            })
            .map_err(|error| error.to_string()),
        _ => text
            .parse::<Fade>()
            .map(|fade| {
                let lfo = Lfo {
                    fade,
                    ..current.lfo
                };
                ModSlot { lfo, ..current }
            })
            .map_err(|error| error.to_string()),
    };
    match written {
        Ok(modulator) => set_mod(services, slot, modulator),
        Err(why) => services.status = why,
    }
}
