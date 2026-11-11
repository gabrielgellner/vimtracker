//! `i` on a setting typed as a value: its digits or words, and what they set.

use ratatui::crossterm::event::KeyCode;

use vt_model::accumulator::{Accumulation, MOST_DEGREES};
use vt_model::action::Action;
use vt_model::arpeggio::{Arpeggio, MOST_OCTAVES};
use vt_model::constants::{MAX_ENVELOPE_MS, MAX_GROUPS, MAX_TRACK_RESET};
use vt_model::envelope::Envelope;
use vt_model::kit::CHOKE_GROUPS;
use vt_model::lfo::{Lfo, MAX_LFO_DEPTH, Rate};
use vt_model::settings::{LONGEST_GLIDE_MS, gate_value, glide_value};
use vt_model::song::reset_named;

use crate::app::{App, Entry};
use crate::input::mode::Mode;

use super::{AccumulatorField, ArpField, Control, EnvelopeField, LfoField};

/// Read a depth, with an optional sign, in hex.
fn parse_depth(text: &str) -> Option<i16> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1, digits),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    let depth = i16::from_str_radix(digits, 16).ok()?;
    (0..=MAX_LFO_DEPTH).contains(&depth).then_some(sign * depth)
}

impl App {
    /// Type a value: two hex digits for levels, and anything else then `Enter`.
    pub(crate) fn insert_track_page(&mut self, key: KeyCode) {
        let control = self.track_control();
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if !control.is_hex() => {
                let text = std::mem::take(&mut self.entry.text);
                self.mode = Mode::Normal;
                if !text.is_empty() {
                    self.commit_control(control, &text);
                }
            }
            KeyCode::Char(c) if control.is_hex() && c.is_ascii_hexdigit() => {
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
            KeyCode::Char(c) if !control.is_hex() && self.entry.text.len() < 12 => {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_control(&mut self, control: Control, text: &str) {
        let track = self.page_track();
        match control {
            Control::Mute | Control::Solo => {}
            Control::Sample => {}
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
            Control::Groove | Control::Speed => {}
            Control::Lfo(slot, LfoField::Rate) => match text.parse::<Rate>() {
                Ok(rate) => {
                    let lfo = Lfo {
                        rate,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                Err(error) => self.status = error.to_string(),
            },
            Control::Lfo(slot, LfoField::Depth) => match parse_depth(text) {
                Some(depth) => {
                    let lfo = Lfo {
                        depth,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                None => self.status = "Depth is hex from -FF to +FF".to_owned(),
            },
            Control::Lfo(slot, LfoField::Phase) => match text.parse::<u16>() {
                Ok(phase) if phase < 360 => {
                    let lfo = Lfo {
                        phase,
                        ..track.lfos[slot]
                    };
                    self.set_lfo(slot, lfo);
                }
                _ => self.status = "Phase is degrees from 0 to 359".to_owned(),
            },
            Control::Lfo(..) | Control::FilterType | Control::FilterMode | Control::Machine => {}
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
            Control::Envelope(slot, field) => {
                let current = track.envelopes[slot];
                let ms = text.parse::<u16>().ok().filter(|ms| *ms <= MAX_ENVELOPE_MS);
                let changed = match field {
                    EnvelopeField::Attack => ms.map(|attack_ms| Envelope {
                        attack_ms,
                        ..current
                    }),
                    EnvelopeField::Decay => ms.map(|decay_ms| Envelope {
                        decay_ms,
                        ..current
                    }),
                    EnvelopeField::Release => ms.map(|release_ms| Envelope {
                        release_ms,
                        ..current
                    }),
                    EnvelopeField::Sustain => text
                        .trim_end_matches('%')
                        .parse::<u8>()
                        .ok()
                        .filter(|sustain| *sustain <= 100)
                        .map(|sustain| Envelope { sustain, ..current }),
                    EnvelopeField::Depth => {
                        parse_depth(text).map(|depth| Envelope { depth, ..current })
                    }
                    EnvelopeField::Destination | EnvelopeField::Reset => None,
                };
                match changed {
                    Some(envelope) => self.set_envelope(slot, envelope),
                    None => {
                        self.status = match field {
                            EnvelopeField::Sustain => "Sustain is a percent from 0 to 100",
                            EnvelopeField::Depth => "Depth is hex from -FF to +FF",
                            _ => "Times are milliseconds from 0 to 8000",
                        }
                        .to_owned();
                    }
                }
            }
        }
    }
}
