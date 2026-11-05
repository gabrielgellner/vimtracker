//! What keys do in the instrument view.

use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Command;
use vt_model::action::{Action, InstrumentField};
use vt_model::constants::MAX_ENVELOPE_MS;
use vt_model::selection::Motion;
use vt_model::song::Instrument;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::Mode;

/// A row of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Sample,
    Volume,
    Attack,
    Release,
}

impl Row {
    /// Every row, top to bottom.
    pub const ALL: [Self; 4] = [Self::Sample, Self::Volume, Self::Attack, Self::Release];

    /// The row's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sample => "Sample",
            Self::Volume => "Volume",
            Self::Attack => "Attack",
            Self::Release => "Release",
        }
    }

    fn field(self) -> Option<InstrumentField> {
        match self {
            Self::Sample => None,
            Self::Volume => Some(InstrumentField::Volume),
            Self::Attack => Some(InstrumentField::Attack),
            Self::Release => Some(InstrumentField::Release),
        }
    }

    fn position(self) -> usize {
        Self::ALL.iter().position(|row| *row == self).unwrap_or(0)
    }
}

/// How far `+` and `-` move an envelope time. Short times need finer steps than long ones.
fn envelope_step(value: u16) -> u16 {
    match value {
        0..=9 => 1,
        10..=99 => 5,
        _ => 25,
    }
}

impl App {
    fn instrument_value(&self, field: InstrumentField) -> u16 {
        let fallback = Instrument::default();
        let current = self
            .song()
            .instruments
            .get(&self.instrument)
            .map_or(&fallback, |i| i.as_ref());
        match field {
            InstrumentField::Volume => u16::from(current.volume),
            InstrumentField::Attack => current.attack_ms,
            InstrumentField::Release => current.release_ms,
        }
    }

    fn set_instrument_value(&mut self, field: InstrumentField, value: u16) {
        self.apply(&Action::SetInstrument {
            id: self.instrument,
            field,
            value,
        });
    }

    fn move_row(&mut self, to: usize) {
        self.instrument_row = Row::ALL[to.min(Row::ALL.len() - 1)];
    }

    fn adjust_instrument(&mut self, up: bool, count: usize) {
        let Some(field) = self.instrument_row.field() else {
            return;
        };
        let mut value = self.instrument_value(field);
        for _ in 0..count {
            value = match (field, up) {
                (InstrumentField::Volume, true) => (value + 1).min(0xFF),
                (InstrumentField::Volume, false) => value.saturating_sub(1),
                (_, true) => (value + envelope_step(value)).min(MAX_ENVELOPE_MS),
                (_, false) => value.saturating_sub(envelope_step(value.saturating_sub(1))),
            };
        }
        self.set_instrument_value(field, value);
    }

    /// Run a command in the instrument view. Returns false if it means the same as anywhere.
    pub(crate) fn run_instrument(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        match cmd {
            Cmd::Move(Motion::Down) => self.move_row(self.instrument_row.position() + times),
            Cmd::Move(Motion::Up) => {
                self.move_row(self.instrument_row.position().saturating_sub(times));
            }
            Cmd::Move(Motion::First) => self.move_row(0),
            Cmd::Move(Motion::Last) => self.move_row(Row::ALL.len() - 1),
            Cmd::Move(Motion::Right) => {
                self.instrument = self.instrument.saturating_add(times as u32);
            }
            Cmd::Move(Motion::Left) => {
                self.instrument = self.instrument.saturating_sub(times as u32);
            }
            Cmd::Increase => self.adjust_instrument(true, times),
            Cmd::Decrease => self.adjust_instrument(false, times),
            Cmd::Hear => {
                self.handle.send(Command::Preview {
                    instrument: self.instrument,
                    note: 60,
                });
            }
            Cmd::EnterInsert if self.instrument_row == Row::Sample => {
                let roots = self.sample_roots();
                self.open_sample_picker(&roots);
            }
            Cmd::EnterInsert => {
                self.entry = Entry::default();
                self.mode = Mode::Insert;
            }
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// Type a value: two hex digits for volume, and milliseconds then `Enter` for envelope times.
    pub(crate) fn insert_instrument_row(&mut self, key: KeyCode) {
        let Some(field) = self.instrument_row.field() else {
            self.mode = Mode::Normal;
            return;
        };
        let volume = field == InstrumentField::Volume;
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Enter if !volume => self.commit_instrument_row(field),
            KeyCode::Char(c) if volume && c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    self.commit_instrument_row(field);
                }
            }
            KeyCode::Char(c) if !volume && c.is_ascii_digit() && self.entry.text.len() < 5 => {
                self.entry.text.push(c);
            }
            _ => {}
        }
    }

    fn commit_instrument_row(&mut self, field: InstrumentField) {
        let text = std::mem::take(&mut self.entry.text);
        let radix = if field == InstrumentField::Volume {
            16
        } else {
            10
        };
        // Five digits can pass a u16's limit, and that still has to reach the range check.
        if let Ok(value) = u32::from_str_radix(&text, radix) {
            self.set_instrument_value(field, value.min(u32::from(u16::MAX)) as u16);
        }
        self.mode = Mode::Normal;
    }
}
