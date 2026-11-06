//! What keys do in the instrument view.

use ratatui::crossterm::event::KeyCode;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::selection::Motion;
use vt_model::song::Instrument;
use vt_model::step::MAX_VELOCITY;

use crate::app::{App, Entry};
use crate::input::bindings::Cmd;
use crate::input::mode::Mode;

/// A row of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Sample,
    Volume,
}

impl Row {
    /// Every row, top to bottom.
    pub const ALL: [Self; 2] = [Self::Sample, Self::Volume];

    /// The row's name on screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Sample => "Sample",
            Self::Volume => "Volume",
        }
    }

    fn position(self) -> usize {
        Self::ALL.iter().position(|row| *row == self).unwrap_or(0)
    }
}

/// How many small steps a large one moves a volume, which is `10` in hex.
const LARGE_STEP: i32 = 0x10;

impl App {
    fn instrument_volume(&self) -> u8 {
        self.song()
            .instruments
            .get(&self.instrument)
            .map_or(Instrument::default().volume, |i| i.volume)
    }

    fn set_instrument_volume(&mut self, volume: u8) {
        self.apply(&Action::SetInstrumentVolume {
            id: self.instrument,
            volume,
        });
    }

    fn move_row(&mut self, to: usize) {
        self.instrument_row = Row::ALL[to.min(Row::ALL.len() - 1)];
    }

    fn adjust_instrument(&mut self, up: bool, count: usize, large: bool) {
        if self.instrument_row != Row::Volume {
            return;
        }
        let size = if large { LARGE_STEP } else { 1 };
        let by = size.saturating_mul(count.min(0x100) as i32);
        let moved = i32::from(self.instrument_volume()) + if up { by } else { -by };
        self.set_instrument_volume(moved.clamp(0, 0xFF) as u8);
    }

    /// Put this row back to its default. An instrument nothing has been set on stays that way, and
    /// a row already there leaves no undo step.
    fn reset_instrument_row(&mut self) {
        let Some(current) = self.song().instruments.get(&self.instrument).cloned() else {
            return;
        };
        match self.instrument_row {
            Row::Sample => {
                if current.sample.is_some() {
                    self.apply(&Action::ClearSample {
                        instrument: self.instrument,
                    });
                }
            }
            Row::Volume => {
                let volume = Instrument::default().volume;
                if current.volume != volume {
                    self.set_instrument_volume(volume);
                }
            }
        }
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
                for _ in 0..times {
                    self.instrument = self.song().next_instrument(self.instrument, true);
                }
            }
            Cmd::Move(Motion::Left) => {
                for _ in 0..times {
                    self.instrument = self.song().next_instrument(self.instrument, false);
                }
            }
            Cmd::Increase => self.adjust_instrument(true, times, false),
            Cmd::Decrease => self.adjust_instrument(false, times, false),
            Cmd::IncreaseLarge => self.adjust_instrument(true, times, true),
            Cmd::DecreaseLarge => self.adjust_instrument(false, times, true),
            Cmd::ClearField => self.reset_instrument_row(),
            Cmd::PickValue if self.instrument_row == Row::Sample => {
                let roots = self.sample_roots();
                self.open_sample_picker(&roots);
            }
            Cmd::PickValue => {
                self.status = "K chooses a sample on the Sample row".to_owned();
            }
            Cmd::Audition => {
                self.handle.send(Command::Preview {
                    instrument: self.instrument,
                    note: 60,
                    velocity: MAX_VELOCITY,
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

    /// Type a volume: two hex digits.
    pub(crate) fn insert_instrument_row(&mut self, key: KeyCode) {
        if self.instrument_row != Row::Volume {
            self.mode = Mode::Normal;
            return;
        }
        match key {
            KeyCode::Esc => {
                self.entry = Entry::default();
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.entry.text.pop();
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() => {
                self.entry.text.push(c.to_ascii_uppercase());
                if self.entry.text.len() == 2 {
                    let text = std::mem::take(&mut self.entry.text);
                    if let Ok(volume) = u8::from_str_radix(&text, 16) {
                        self.set_instrument_volume(volume);
                    }
                    self.mode = Mode::Normal;
                }
            }
            _ => {}
        }
    }
}
