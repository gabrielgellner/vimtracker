//! `i` on a setting typed as a value: its digits or words, and what they set.

use ratatui::crossterm::event::KeyCode;

use vt_model::lfo::{Fade, Lfo, Rate};
use vt_model::modulation::{ModField, ModKind, ModSlot};
use vt_model::shape::params::Typing;
use vt_model::steps::{StepRate, Steps};

use crate::app::{App, Entry};
use crate::input::mode::Mode;
use crate::input::typed::{HEX, Rule, Typed};
use crate::services::Services;
use crate::views::widgets::param::edit;

use super::Control;
use super::editing::{set_fine_here, set_mod, view_on, write};
use super::tuning::type_tuning;

/// The longest a generator's value can be typed: a list of eight notes and the spaces between.
const GEN_TEXT: usize = 40;

impl App {
    /// Type a value: two hex digits for levels, and anything else then `Enter`.
    pub(crate) fn insert_track_page(&mut self, key: KeyCode) {
        let control = self.track_control();
        // A generator's ends on a column of hex values enter themselves, as levels do, and a
        // list of notes takes more room.
        let (hex, longest) = match control {
            Control::Gen { .. } => (self.gen_typing() == Some(Typing::Hex), GEN_TEXT),
            _ => (control.is_hex(), 12),
        };
        let rule = if hex { HEX } else { Rule::Text { longest } };
        match self.modes.entry.text.key(key, rule) {
            Typed::Editing => {}
            Typed::Cancel => {
                self.modes.entry = Entry::default();
                self.modes.mode = Mode::Normal;
            }
            Typed::Commit(text) => {
                self.modes.mode = Mode::Normal;
                if hex && self.track_page.fine {
                    if let Ok(fine) = u8::from_str_radix(&text, 16) {
                        set_fine_here(&mut self.services, control, fine);
                    }
                } else if !text.is_empty() {
                    self.commit_typed(control, &text);
                }
            }
        }
    }

    /// Write what was typed into the setting: a generator's into its shape, and the rest as the
    /// track page's settings take it.
    fn commit_typed(&mut self, control: Control, text: &str) {
        match control {
            Control::Gen { .. } => self.type_gen(text),
            _ => commit_control(&mut self.services, control, text),
        }
    }
}

/// Write what was typed into one of the track's settings, as its view reads it, saying on the
/// status line why where it doesn't.
fn commit_control(services: &mut Services, control: Control, text: &str) {
    let track = services.page_track();
    match control {
        // A generator's are typed into its shape, by `App::commit_typed`.
        Control::Gen { .. } | Control::NewGenerator(_) => {}
        Control::Tune | Control::Cents => type_tuning(services, control, text),
        Control::Mod(slot, field @ (ModField::Rate | ModField::Fade)) => {
            type_kind(services, slot, field, text);
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
