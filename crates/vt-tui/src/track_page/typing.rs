//! `i` on a setting typed as a value: its digits or words, and what they set.

use ratatui::crossterm::event::KeyCode;

use vt_model::accumulator::{Accumulation, MOST_DEGREES};
use vt_model::action::Action;
use vt_model::arpeggio::{Arpeggio, MOST_OCTAVES};
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::{MAX_HOLD_MS, MAX_RATCHET};
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{Fade, Lfo, Rate};
use vt_model::modulation::{MAX_DEPTH, ModKind, ModSlot};
use vt_model::settings::{LONGEST_GLIDE_MS, gate_value, glide_value};
use vt_model::shape::params::Typing;
use vt_model::song::reset_named;
use vt_model::step::Note;
use vt_model::steps::{MAX_MUTATE, STEPS, StepRate, Steps};
use vt_model::tracking::Source;

use crate::app::{App, Entry};
use crate::input::mode::Mode;
use crate::input::typed::{HEX, Rule, Typed};
use crate::services::Services;

use super::editing::{
    set_accumulation, set_arpeggio, set_fine_here, set_glide, set_group_here, set_hex, set_macro,
    set_mod, set_reset,
};
use super::fx::set_insert;
use super::tuning::type_tuning;
use super::{AccumulatorField, ArpField, Control, ModField};

/// The longest a generator's value can be typed: a list of eight notes and the spaces between.
const GEN_TEXT: usize = 40;

/// Read a depth, with an optional sign, in hex.
fn parse_depth(text: &str) -> Option<i16> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let depth = i16::from_str_radix(digits, 16).ok()?;
    (0..=MAX_DEPTH).contains(&depth).then_some(sign * depth)
}

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

/// Write what was typed into one of the track's settings.
fn commit_control(services: &mut Services, control: Control, text: &str) {
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
                _ => services.status = format!("Group is a group from 1 to {MAX_GROUPS}, or off"),
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
                services.status = format!("Glide is a time in ms up to {LONGEST_GLIDE_MS}, or off");
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
