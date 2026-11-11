//! The audio page: which output plays, at what rate, and with what buffer. A change takes effect
//! as it's made, as Ableton's audio preferences do: the main loop opens the new output.

use vt_model::selection::Motion;

use crate::app::App;
use crate::audio::{AudioChoice, DeviceInfo};
use crate::glyphs;
use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::input::picker::{Item, Pick, Picker};

/// The page's rows, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRow {
    Device,
    Rate,
    Buffer,
}

impl AudioRow {
    pub const ALL: [Self; 3] = [Self::Device, Self::Rate, Self::Buffer];

    pub fn name(self) -> &'static str {
        match self {
            Self::Device => "Device",
            Self::Rate => "Rate",
            Self::Buffer => "Buffer",
        }
    }
}

/// How long a buffer lasts, in milliseconds, at a rate.
pub fn buffer_ms(frames: u32, rate: u32) -> f64 {
    f64::from(frames) * 1000.0 / f64::from(rate.max(1))
}

impl App {
    /// What the status line says on the audio page while nothing else is said there: what the
    /// leaf before the CPU figure means, while it shows, as `󰌪 CPU slowed ×3.6 by power saving,
    /// about 8% at full speed`.
    pub fn audio_page_hint(&self) -> Option<String> {
        if self.view != View::Audio {
            return None;
        }
        let meaning = self.slowdown.explanation(self.cpu.percent())?;
        Some(format!("{} CPU {meaning}", glyphs::LEAF))
    }

    /// The row the cursor is on.
    pub fn audio_row_here(&self) -> AudioRow {
        AudioRow::ALL[self.audio_row.min(AudioRow::ALL.len() - 1)]
    }

    /// The device playing, or chosen, as the page lists it.
    pub fn audio_device_here(&self) -> Option<&DeviceInfo> {
        let devices = &self.audio.devices;
        match &self.audio.choice.device {
            Some(id) => devices.iter().find(|device| device.id == *id),
            None => devices.iter().find(|device| device.default),
        }
    }

    /// The choices for a row, with the one now playing, and each as the choice it makes.
    fn audio_options(&self, row: AudioRow) -> (Vec<(String, AudioChoice)>, Option<usize>) {
        let choice = &self.audio.choice;
        let device = self.audio_device_here();
        let options: Vec<(String, AudioChoice)> = match row {
            AudioRow::Device => self
                .audio
                .devices
                .iter()
                .map(|found| {
                    // A rate or buffer the new device doesn't take goes back to its own.
                    let keeps = |value: Option<u32>, offered: &[u32]| {
                        value.filter(|value| offered.contains(value))
                    };
                    let name = if found.default {
                        format!("{}, the system's default", found.name)
                    } else {
                        found.name.clone()
                    };
                    (
                        name,
                        AudioChoice {
                            device: Some(found.id.clone()),
                            rate: keeps(choice.rate, &found.rates),
                            buffer: keeps(choice.buffer, &found.buffers),
                        },
                    )
                })
                .collect(),
            AudioRow::Rate => device.map_or_else(Vec::new, |device| {
                device
                    .rates
                    .iter()
                    .map(|rate| {
                        let rate = *rate;
                        (
                            format!("{rate} Hz"),
                            AudioChoice {
                                rate: Some(rate),
                                ..choice.clone()
                            },
                        )
                    })
                    .collect()
            }),
            AudioRow::Buffer => device.map_or_else(Vec::new, |device| {
                device
                    .buffers
                    .iter()
                    .map(|size| {
                        let size = *size;
                        (
                            format!("{size} frames, {:.1} ms", buffer_ms(size, self.rate)),
                            AudioChoice {
                                buffer: Some(size),
                                ..choice.clone()
                            },
                        )
                    })
                    .collect()
            }),
        };
        let current = match row {
            AudioRow::Device => device.and_then(|device| {
                self.audio
                    .devices
                    .iter()
                    .position(|found| found.id == device.id)
            }),
            AudioRow::Rate => {
                device.and_then(|device| device.rates.iter().position(|r| *r == self.rate))
            }
            AudioRow::Buffer => device.and_then(|device| {
                let size = self.audio.buffer.or(choice.buffer)?;
                device.buffers.iter().position(|b| *b == size)
            }),
        };
        (options, current)
    }

    /// Ask for another output, which the main loop opens.
    fn choose_audio(&mut self, choice: AudioChoice) {
        if choice != self.audio.choice || !self.audio.running {
            self.audio_request = Some(choice);
        }
    }

    /// Run a command on the audio page. Returns false if it means the same as anywhere.
    pub(crate) fn run_audio_page(&mut self, cmd: Cmd) -> bool {
        let count = self.count.take();
        let times = count.unwrap_or(1).max(1);
        let last = AudioRow::ALL.len() - 1;
        match cmd {
            Cmd::Move(Motion::Down) => self.audio_row = (self.audio_row + times).min(last),
            Cmd::Move(Motion::Up) => self.audio_row = self.audio_row.saturating_sub(times),
            Cmd::Move(Motion::First) => self.audio_row = 0,
            Cmd::Move(Motion::Last) => self.audio_row = last,
            Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                let up = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
                let (options, current) = self.audio_options(self.audio_row_here());
                if options.is_empty() {
                    self.status = "This device doesn't say what it takes".to_owned();
                    return true;
                }
                let at = match (current, up) {
                    (Some(at), true) => (at + times).min(options.len() - 1),
                    (Some(at), false) => at.saturating_sub(times),
                    (None, _) => 0,
                };
                if Some(at) != current {
                    self.choose_audio(options[at].1.clone());
                }
            }
            // Back to the system's own choice for this row.
            Cmd::ClearField => {
                let choice = match self.audio_row_here() {
                    AudioRow::Device => AudioChoice::default(),
                    AudioRow::Rate => AudioChoice {
                        rate: None,
                        ..self.audio.choice.clone()
                    },
                    AudioRow::Buffer => AudioChoice {
                        buffer: None,
                        ..self.audio.choice.clone()
                    },
                };
                self.choose_audio(choice);
            }
            Cmd::PickValue | Cmd::EnterInsert => self.open_audio_picker(),
            _ => {
                self.count = count;
                return false;
            }
        }
        true
    }

    /// List the row's choices, with the one playing highlighted.
    fn open_audio_picker(&mut self) {
        let row = self.audio_row_here();
        let (options, current) = self.audio_options(row);
        if options.is_empty() {
            self.status = "There's nothing to choose here".to_owned();
            return;
        }
        let items = options
            .into_iter()
            .map(|(label, choice)| Item::new(label, Pick::Audio(choice)))
            .collect();
        self.show_picker(Picker::new(row.name(), items).opened_on(current));
    }

    /// A choice from the audio page's list.
    pub(crate) fn pick_audio(&mut self, choice: AudioChoice) {
        self.choose_audio(choice);
    }
}
