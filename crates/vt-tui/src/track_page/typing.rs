//! `i` on a setting typed as a value: its digits or words, and what they set.

use ratatui::crossterm::event::KeyCode;

use vt_model::accumulator::{Accumulation, MOST_DEGREES};
use vt_model::action::Action;
use vt_model::arpeggio::{Arpeggio, MOST_OCTAVES};
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::{Adsr, Ahd, Drum, Hold, Lpg, MAX_HOLD_MS, MAX_RATCHET};
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
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if !hex => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                if !text.is_empty() {
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if hex && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    self.mode = Mode::Normal;
                    if self.track_page.fine {
                        if let Ok(fine) = u8::from_str_radix(&text, 16) {
                            self.set_fine_here(fine);
                        }
                    } else {
                        self.commit_control(control, &text);
                    }
                }
            }
            KeyCode::Char(c) if !hex && self.entry.text.len() < longest => {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_control(&mut self, control: Control, text: &str) {
        let track = self.page_track();
        match control {
            Control::Gen { .. } => self.type_gen(text),
            Control::NewGenerator(_) => {}
            Control::Mute | Control::Solo | Control::Limiter | Control::Safe => {}
            Control::Sample => {}
            Control::Tune | Control::Cents => self.type_tuning(control, text),
            Control::Insert(_) => {}
            Control::InsertParam(slot, param) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    let insert = track.inserts[slot];
                    self.set_insert(slot, insert.with_value(param, value));
                }
            }
            Control::Base(_) | Control::Level | Control::Trim | Control::Setting(_) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    self.set_hex(control, value);
                }
            }
            Control::Choke if text.eq_ignore_ascii_case("off") => {
                self.apply(&Action::SetTrackChoke {
                    track: self.cursor.track,
                    choke: None,
                });
            }
            Control::Choke => match text.parse::<u8>() {
                Ok(group) if (1..=CHOKE_GROUPS).contains(&group) => {
                    self.apply(&Action::SetTrackChoke {
                        track: self.cursor.track,
                        choke: Some(group),
                    });
                }
                _ => self.status = format!("Choke is a group from 1 to {CHOKE_GROUPS}, or off"),
            },
            Control::Group if text.eq_ignore_ascii_case("off") => self.set_group_here(None),
            Control::Group => {
                let number = text.trim_start_matches(['G', 'g']).parse::<u8>();
                match number {
                    Ok(group) if (1..=MAX_GROUPS).contains(&usize::from(group)) => {
                        self.set_group_here(Some(group));
                    }
                    _ => self.status = format!("Group is a group from 1 to {MAX_GROUPS}, or off"),
                }
            }
            Control::GateLength => match text.trim_end_matches('%').trim().parse::<u8>() {
                Ok(percent) if (1..=100).contains(&percent) => {
                    self.apply(&Action::SetTrackGateLength {
                        track: self.cursor.track,
                        gate_length: Some(gate_value(percent)),
                    });
                }
                _ => self.status = "Gate is a percent from 1 to 100".to_owned(),
            },
            Control::Glide if text.eq_ignore_ascii_case("off") => self.set_glide(0),
            Control::Glide => match text
                .trim_end_matches("ms")
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|ms| (0.0..=LONGEST_GLIDE_MS).contains(ms))
            {
                Some(ms) => self.set_glide(glide_value(ms)),
                None => {
                    self.status = format!("Glide is a time in ms up to {LONGEST_GLIDE_MS}, or off");
                }
            },
            Control::Reset => match reset_named(text) {
                Some(reset) => self.set_reset(reset),
                None => {
                    self.status = format!(
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
                        self.set_accumulation(if field == AccumulatorField::Up {
                            Accumulation { up: limit, ..now }
                        } else {
                            Accumulation { down: limit, ..now }
                        });
                    }
                    _ => {
                        self.status =
                            format!("A limit is a number of degrees from 1 to {MOST_DEGREES}");
                    }
                }
            }
            Control::Accumulator(_) => {}
            Control::Arpeggio(ArpField::Octaves) => match text.trim().parse::<u8>() {
                Ok(octaves) if (1..=MOST_OCTAVES).contains(&octaves) => {
                    self.set_arpeggio(Arpeggio {
                        octaves,
                        ..track.arpeggio
                    });
                }
                _ => self.status = format!("Octaves is a number from 1 to {MOST_OCTAVES}"),
            },
            Control::Arpeggio(_) => {}
            Control::Mod(slot, field) => self.type_mod(slot, field, text),
            Control::Macro(at) => {
                if let Ok(value) = u8::from_str_radix(text, 16) {
                    self.set_macro(at, u16::from(value) << 8);
                }
            }
        }
    }

    /// Set a mod slot's setting from what was typed: a depth in signed hex, an envelope's time,
    /// sustain or hold, an LFO's rate or phase, Steps' own, or where Track's map starts and ends, a
    /// note's name or hex as its source has them. The rest are lists, chosen with `K`.
    fn type_mod(&mut self, slot: usize, field: ModField, text: &str) {
        let current = self.page_track().mods[slot];
        let (adsr, ahd, drum, lpg, lfo) = (
            current.adsr,
            current.ahd,
            current.drum,
            current.lpg,
            current.lfo,
        );
        let ms = text.parse::<u16>().ok().filter(|ms| *ms <= MAX_ENVELOPE_MS);
        let with_adsr = |adsr: Adsr| ModSlot { adsr, ..current };
        let with_ahd = |ahd: Ahd| ModSlot { ahd, ..current };
        let with_drum = |drum: Drum| ModSlot { drum, ..current };
        let with_lpg = |lpg: Lpg| ModSlot { lpg, ..current };
        let lpg_kind = current.kind == ModKind::Lpg;
        let with_lfo = |lfo: Lfo| ModSlot { lfo, ..current };
        let steps = current.steps;
        let with_steps = |steps: Steps| ModSlot { steps, ..current };
        let ahd_kind = current.kind == ModKind::Ahd;
        let times = format!("Times are milliseconds from 0 to {MAX_ENVELOPE_MS}");
        let (changed, why) = match field {
            ModField::Depth => (
                parse_depth(text).map(|depth| ModSlot { depth, ..current }),
                "Depth is hex from -FF to +FF".to_owned(),
            ),
            ModField::Attack if lpg_kind => (
                ms.map(|attack_ms| with_lpg(Lpg { attack_ms, ..lpg })),
                times,
            ),
            ModField::Decay if lpg_kind => {
                (ms.map(|decay_ms| with_lpg(Lpg { decay_ms, ..lpg })), times)
            }
            ModField::Colour => (
                u8::from_str_radix(text.trim(), 16)
                    .ok()
                    .map(|colour| with_lpg(Lpg { colour, ..lpg })),
                "Colour is hex from 00 to FF".to_owned(),
            ),
            ModField::Attack if ahd_kind => (
                ms.map(|attack_ms| with_ahd(Ahd { attack_ms, ..ahd })),
                times,
            ),
            ModField::Attack => (
                ms.map(|attack_ms| with_adsr(Adsr { attack_ms, ..adsr })),
                times,
            ),
            ModField::Decay if ahd_kind => {
                (ms.map(|decay_ms| with_ahd(Ahd { decay_ms, ..ahd })), times)
            }
            ModField::Decay if current.kind == ModKind::Drum => (
                ms.map(|decay_ms| with_drum(Drum { decay_ms, ..drum })),
                times,
            ),
            ModField::Peak => (ms.map(|peak_ms| with_drum(Drum { peak_ms, ..drum })), times),
            ModField::Body => (
                text.trim_end_matches('%')
                    .parse::<u8>()
                    .ok()
                    .filter(|body| *body <= 100)
                    .map(|body| with_drum(Drum { body, ..drum })),
                "Body is a percent from 0 to 100".to_owned(),
            ),
            ModField::Ratchet => (
                text.trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|ratchet| (1..=MAX_RATCHET).contains(ratchet))
                    .map(|ratchet| with_drum(Drum { ratchet, ..drum })),
                format!("Ratchet is how many strikes, from 1 to {MAX_RATCHET}"),
            ),
            ModField::Decay => (
                ms.map(|decay_ms| with_adsr(Adsr { decay_ms, ..adsr })),
                times,
            ),
            ModField::Hold => {
                let hold = if text.trim().eq_ignore_ascii_case("note") {
                    Some(Hold::Note)
                } else {
                    let ms = text
                        .trim()
                        .trim_end_matches("ms")
                        .trim()
                        .parse::<u16>()
                        .ok();
                    ms.filter(|ms| *ms <= MAX_HOLD_MS).map(Hold::Ms)
                };
                (
                    hold.map(|hold| with_ahd(Ahd { hold, ..ahd })),
                    format!("Hold is milliseconds from 0 to {MAX_HOLD_MS}, or note"),
                )
            }
            ModField::Release => (
                ms.map(|release_ms| with_adsr(Adsr { release_ms, ..adsr })),
                format!("Times are milliseconds from 0 to {MAX_ENVELOPE_MS}"),
            ),
            ModField::Sustain => (
                text.trim_end_matches('%')
                    .parse::<u8>()
                    .ok()
                    .filter(|sustain| *sustain <= 100)
                    .map(|sustain| with_adsr(Adsr { sustain, ..adsr })),
                "Sustain is a percent from 0 to 100".to_owned(),
            ),
            ModField::Rate if current.kind == ModKind::Steps => match text.parse::<StepRate>() {
                Ok(rate) => (Some(with_steps(Steps { rate, ..steps })), String::new()),
                Err(error) => (None, error.to_string()),
            },
            ModField::Rate => match text.parse::<Rate>() {
                Ok(rate) => (Some(with_lfo(Lfo { rate, ..lfo })), String::new()),
                Err(error) => (None, error.to_string()),
            },
            ModField::Length => (
                text.trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|length| (1..=STEPS as u8).contains(length))
                    .map(|length| with_steps(Steps { length, ..steps })),
                format!("Length is how many steps, from 1 to {STEPS}"),
            ),
            ModField::Start => (
                text.trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|start| (1..=STEPS as u8).contains(start))
                    .map(|start| {
                        with_steps(Steps {
                            start: start - 1,
                            ..steps
                        })
                    }),
                format!("Start is a step, from 1 to {STEPS}"),
            ),
            ModField::Mutate => (
                text.trim()
                    .trim_end_matches('%')
                    .parse::<u8>()
                    .ok()
                    .filter(|mutate| *mutate <= MAX_MUTATE)
                    .map(|mutate| with_steps(Steps { mutate, ..steps })),
                format!("Mutate is a percent from 0 to {MAX_MUTATE}"),
            ),
            ModField::Smooth => (
                parse_depth(text).map(|smooth| with_steps(Steps { smooth, ..steps })),
                "Smooth is hex from -FF to +FF".to_owned(),
            ),
            ModField::Step(at) => (
                parse_depth(text).map(|value| {
                    let mut values = steps.values;
                    values[usize::from(at)] = value;
                    with_steps(Steps { values, ..steps })
                }),
                "A step is hex from -FF to +FF".to_owned(),
            ),
            ModField::Phase => (
                text.parse::<u16>()
                    .ok()
                    .filter(|phase| *phase < 360)
                    .map(|phase| with_lfo(Lfo { phase, ..lfo })),
                "Phase is degrees from 0 to 359".to_owned(),
            ),
            ModField::Deform => (
                parse_depth(text).map(|deform| with_lfo(Lfo { deform, ..lfo })),
                "Deform is hex from -FF to +FF".to_owned(),
            ),
            ModField::Fade => match text.parse::<Fade>() {
                Ok(fade) => (Some(with_lfo(Lfo { fade, ..lfo })), String::new()),
                Err(error) => (None, error.to_string()),
            },
            ModField::Low | ModField::High => {
                let source = current.track.source;
                let (lowest, highest) = source.span();
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
                let changed = value
                    .filter(|value| (lowest..=highest).contains(value))
                    .map(|value| {
                        let mut track = current.track;
                        let bounds = track.bounds_mut();
                        if field == ModField::Low {
                            bounds.low = value;
                        } else {
                            bounds.high = value;
                        }
                        ModSlot { track, ..current }
                    });
                let what = Control::Mod(slot, field).label();
                let (lowest, highest) = (source.text(lowest), source.text(highest));
                let why = match source {
                    Source::Note => format!("{what} is a note from {lowest} to {highest}"),
                    _ => format!("{what} is hex from {lowest} to {highest}"),
                };
                (changed, why)
            }
            ModField::Type
            | ModField::Mode
            | ModField::Polarity
            | ModField::Destination
            | ModField::Reset
            | ModField::Loop
            | ModField::Shape => return,
        };
        match changed {
            Some(modulator) => self.set_mod(slot, modulator),
            None => self.status = why,
        }
    }
}
