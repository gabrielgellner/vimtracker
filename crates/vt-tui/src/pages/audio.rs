//! The audio page, the project page's Audio sub-page (`Space 9`, then `L`): which output this
//! machine plays through, at what rate, and with what buffer, and which input the recorder hears. A change takes effect
//! as it's made, as Ableton's audio preferences do: the main loop opens the new output. Its last
//! rows turn the mouse wheel on and off as a knob on the cursor's setting (`wheel`), and say
//! which way it turns, and turn on the Roto-Control (`roto`), as Live's MIDI preferences choose
//! its control surfaces. The last shows the line describing the setting under the cursor, as
//! `Space d` does, kept in `settings.ron` (decision 476).
//!
//! `AudioPage` is the page as a component: the row its cursor is on, its moves, and its drawing, in
//! `draw`, given a [`Ctx`]. It's a [`Page`], with its rows of the binding table, [`Page::KEYS`],
//! here beside it.

pub mod draw;

use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::Rect;
use vt_model::selection::Motion;

use crate::input::bindings::Cmd;
use crate::input::mode::View;
use crate::input::wheel::{Places, Wheel, WheelSetting};
use crate::overlays::picker::{Item, Pick, Picker};
use crate::pages::project::editing::page_beside;
use crate::pages::{Ctx, CtxMut, Outcome, Page};
use crate::services::Services;
use crate::services::audio::{AudioChoice, DeviceInfo};
use crate::widgets::glyphs;

/// The page's rows, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioRow {
    Device,
    Rate,
    Buffer,
    /// The input the recorder hears, or none.
    Input,
    /// Whether the mouse wheel turns the cursor's setting.
    Wheel,
    /// Whether the value it turns follows the fingers or goes against them.
    WheelDirection,
    /// Whether vimtracker drives the Roto-Control, and how its link is.
    Roto,
    /// Whether the line describing the setting under the cursor shows, as `Space d` switches it.
    Describe,
}

impl AudioRow {
    pub const ALL: [Self; 8] = [
        Self::Device,
        Self::Rate,
        Self::Buffer,
        Self::Input,
        Self::Wheel,
        Self::WheelDirection,
        Self::Roto,
        Self::Describe,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Device => "Device",
            Self::Rate => "Rate",
            Self::Buffer => "Buffer",
            Self::Input => "Input",
            Self::Roto => "Roto",
            Self::Wheel => "Wheel",
            Self::WheelDirection => "Direction",
            Self::Describe => "Describe",
        }
    }
}

/// The audio output as it plays, and what the audio page offers.
#[derive(Debug, Clone, Default)]
pub struct AudioState {
    pub device: String,
    /// The buffer it plays with, in frames, if the device says.
    pub buffer: Option<u32>,
    /// How long from the callback to the sound leaving the device, in microseconds.
    pub latency_us: u64,
    /// Buffers filled too late, and dropouts the device reported, since it started.
    pub late: u64,
    pub xruns: u64,
    /// Whether the main loop should list the devices again, as the page has just opened.
    pub refresh: bool,
    /// Whether it's playing. It stops when the device changes or goes away.
    pub running: bool,
    /// What's chosen, which the system's own choices fill in.
    pub choice: AudioChoice,
    pub devices: Vec<DeviceInfo>,
    /// The inputs the recorder can hear, the one open by name, if one is, and how long from the
    /// sound reaching it to its callback, in microseconds.
    pub(crate) inputs: Vec<DeviceInfo>,
    pub input: Option<String>,
    pub(crate) input_latency_us: u64,
    /// The slowdown check's time at full speed, measured as the program starts, for each stream
    /// it opens.
    pub(crate) full_speed: Duration,
}

/// The audio page as a component: the row its cursor is on, kept while other views are shown,
/// its moves, and its changes. It keeps the output as it plays, which the main loop fills in and
/// opens again when the page asks for another, and which the status line and the recorder read
/// through `App`. The wheel's setting is lent to it, in its [`CtxMut`], as the mouse's ticks are
/// `App`'s, and the Roto's is a service. It's drawn with [`Page::draw`], given the rest of the
/// program as a [`Ctx`].
#[derive(Debug, Clone, Default)]
pub struct AudioPage {
    /// The row the cursor is on, by its place in [`AudioRow::ALL`].
    pub row: usize,
    /// The audio output as it plays, and what the page chooses from, which the main loop keeps
    /// up to date.
    pub audio: AudioState,
    /// A different output the page has chosen, for the main loop to open.
    pub request: Option<AudioChoice>,
}

impl AudioPage {
    /// The row the cursor is on.
    pub fn row_here(&self) -> AudioRow {
        AudioRow::ALL[self.row.min(AudioRow::ALL.len() - 1)]
    }

    /// Run one of the page's own keys, `j` and `k` up and down its rows and `gg` and `G` to its
    /// ends. `times` is its count, at least 1.
    pub fn key_on(&mut self, cmd: Cmd, times: usize) -> Outcome {
        let last = AudioRow::ALL.len() - 1;
        match cmd {
            Cmd::Move(Motion::Down) => self.row = (self.row + times).min(last),
            Cmd::Move(Motion::Up) => self.row = self.row.saturating_sub(times),
            Cmd::Move(Motion::First) => self.row = 0,
            Cmd::Move(Motion::Last) => self.row = last,
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }
}

impl Page for AudioPage {
    const KEYS: &'static [(&'static str, Cmd, &'static str)] = &[
        ("H", Cmd::PreviousPage, "Previous page: Song, Audio"),
        ("L", Cmd::NextPage, "Next page: Song, Audio"),
        ("j", Cmd::Move(Motion::Down), "Next setting"),
        ("k", Cmd::Move(Motion::Up), "Previous setting"),
        ("g g", Cmd::Move(Motion::First), "First setting"),
        ("G", Cmd::Move(Motion::Last), "Last setting"),
        ("=", Cmd::Increase, "The next choice"),
        ("-", Cmd::Decrease, "The previous choice"),
        // There's no large step through a list of choices, so `+` and `_` aren't bound, but Cmd-k
        // and Cmd-j stepped through it before the one rule for Cmd keys, and still do (decision
        // 148).
        ("D-k", Cmd::IncreaseLarge, "The next choice, as Cmd-l"),
        ("D-j", Cmd::DecreaseLarge, "The previous choice, as Cmd-h"),
        ("x", Cmd::ClearField, "Back to the system's own choice"),
        ("K", Cmd::PickValue, "List the choices"),
    ];

    type Drawn = ();

    /// Run a key as the program gives it to the page: one of its moves, [`AudioPage::key_on`],
    /// `H` and `L`, which show the sub-page beside it, a change to the setting under the cursor,
    /// or passed back, for what means the same anywhere.
    fn key(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        match cmd {
            Cmd::PreviousPage | Cmd::NextPage => {
                Outcome::Show(page_beside(View::Audio, cmd == Cmd::NextPage))
            }
            _ => match self.key_on(cmd, times) {
                Outcome::Passed => self.change(cmd, times, ctx),
                outcome => outcome,
            },
        }
    }

    /// Draw the Audio sub-page.
    fn draw(&self, frame: &mut Frame, area: Rect, ctx: &Ctx) {
        crate::pages::audio::draw::draw(frame, area, self, ctx);
    }

    fn column(&self, _ctx: &Ctx) -> String {
        self.row_here().name().to_owned()
    }

    /// What the leaf before the CPU figure means, while it shows, as `󰌪 CPU slowed ×3.6 by power
    /// saving, about 8% at full speed`.
    fn hint(&self, ctx: &Ctx) -> Option<String> {
        let meaning = ctx.slowed_meaning()?;
        Some(format!("{} CPU {meaning}", glyphs::LEAF))
    }
}

/// How long a buffer lasts, in milliseconds, at a rate.
pub(crate) fn buffer_ms(frames: u32, rate: u32) -> f64 {
    f64::from(frames) * 1000.0 / f64::from(rate.max(1))
}

impl AudioPage {
    /// The device playing, or chosen, as the page lists it.
    pub(crate) fn device_here(&self) -> Option<&DeviceInfo> {
        let devices = &self.audio.devices;
        match &self.audio.choice.device {
            Some(id) => devices.iter().find(|device| device.id == *id),
            None => devices.iter().find(|device| device.default),
        }
    }

    /// The choices for a row, with the one now playing, and each as the choice it makes, at the
    /// program's sample rate.
    fn options(&self, row: AudioRow, rate: u32) -> (Vec<(String, AudioChoice)>, Option<usize>) {
        let choice = &self.audio.choice;
        let device = self.device_here();
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
                            input: choice.input.clone(),
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
                            format!("{size} frames, {:.1} ms", buffer_ms(size, rate)),
                            AudioChoice {
                                buffer: Some(size),
                                ..choice.clone()
                            },
                        )
                    })
                    .collect()
            }),
            // Off, then each input, which must run at the output's rate.
            AudioRow::Input => {
                let off = (
                    "Off".to_owned(),
                    AudioChoice {
                        input: None,
                        ..choice.clone()
                    },
                );
                let inputs = self.audio.inputs.iter().map(|found| {
                    let at = if found.rates.contains(&rate) {
                        String::new()
                    } else {
                        format!(", not at {rate} Hz")
                    };
                    let default = if found.default {
                        ", the system's default"
                    } else {
                        ""
                    };
                    (
                        format!("{}{default}{at}", found.name),
                        AudioChoice {
                            input: Some(found.id.clone()),
                            ..choice.clone()
                        },
                    )
                });
                std::iter::once(off).chain(inputs).collect()
            }
            // Off or on, or one way or the other, which their own keys switch.
            AudioRow::Roto | AudioRow::Wheel | AudioRow::WheelDirection | AudioRow::Describe => {
                Vec::new()
            }
        };
        let current = match row {
            AudioRow::Device => device.and_then(|device| {
                self.audio
                    .devices
                    .iter()
                    .position(|found| found.id == device.id)
            }),
            AudioRow::Rate => {
                device.and_then(|device| device.rates.iter().position(|r| *r == rate))
            }
            AudioRow::Buffer => device.and_then(|device| {
                let size = self.audio.buffer.or(choice.buffer)?;
                device.buffers.iter().position(|b| *b == size)
            }),
            AudioRow::Input => match &choice.input {
                None => Some(0),
                Some(id) => self
                    .audio
                    .inputs
                    .iter()
                    .position(|found| found.id == *id)
                    .map(|at| at + 1),
            },
            AudioRow::Roto | AudioRow::Wheel | AudioRow::WheelDirection | AudioRow::Describe => {
                None
            }
        };
        (options, current)
    }

    /// The row's places for the mouse wheel: a list of choices, or off and on. The wheel's own
    /// `Direction` has none, as the wheel leaves it alone.
    pub(crate) fn wheel_places(&self, wheel: WheelSetting, services: &Services) -> Option<Places> {
        match self.row_here() {
            AudioRow::Roto => Some(Places::switch(services.roto.setting.on)),
            AudioRow::Wheel => Some(Places::switch(wheel.on)),
            AudioRow::Describe => Some(Places::switch(services.settings.describe)),
            AudioRow::WheelDirection => None,
            row @ (AudioRow::Device | AudioRow::Rate | AudioRow::Buffer | AudioRow::Input) => {
                let (options, current) = self.options(row, services.rate);
                Some(Places::of(current, options.len()))
            }
        }
    }

    /// What a key does on the Roto's row, as on the master's `Limiter`: `=` turns it on and `-`
    /// off, `K`, `i` and Enter switch it, and `x` puts it back off, as it starts. Returns false
    /// for a key that means nothing there.
    fn roto_row(cmd: Cmd, services: &mut Services) -> bool {
        let on = services.roto.setting.on;
        let wanted = match cmd {
            Cmd::Increase | Cmd::IncreaseLarge => true,
            Cmd::Decrease | Cmd::DecreaseLarge | Cmd::ClearField => false,
            Cmd::PickValue | Cmd::EnterInsert => !on,
            _ => return false,
        };
        services.set_roto(wanted);
        true
    }

    /// What a key does on the wheel's row, as on the Roto's.
    fn wheel_row(cmd: Cmd, wheel: &mut Wheel, services: &mut Services) -> bool {
        let on = wheel.setting.on;
        let wanted = match cmd {
            Cmd::Increase | Cmd::IncreaseLarge => true,
            Cmd::Decrease | Cmd::DecreaseLarge | Cmd::ClearField => false,
            Cmd::PickValue | Cmd::EnterInsert => !on,
            _ => return false,
        };
        wheel.switch(wanted, &mut services.status);
        true
    }

    /// What a key does on the description's row, as on the Roto's: off, as it starts, for `x`.
    fn describe_row(cmd: Cmd, services: &mut Services) -> bool {
        let on = services.settings.describe;
        let wanted = match cmd {
            Cmd::Increase | Cmd::IncreaseLarge => true,
            Cmd::Decrease | Cmd::DecreaseLarge | Cmd::ClearField => false,
            Cmd::PickValue | Cmd::EnterInsert => !on,
            _ => return false,
        };
        if wanted != on {
            services.set_describing(wanted);
        }
        true
    }

    /// What a key does on the wheel's direction row: `=`, `-`, `K`, `i` and Enter switch it, and
    /// `x` puts it back with the fingers. Returns false for a key that means nothing there.
    fn wheel_direction_row(cmd: Cmd, wheel: &mut Wheel, services: &mut Services) -> bool {
        let reversed = match cmd {
            Cmd::Increase
            | Cmd::IncreaseLarge
            | Cmd::Decrease
            | Cmd::DecreaseLarge
            | Cmd::PickValue
            | Cmd::EnterInsert => !wheel.setting.reversed,
            Cmd::ClearField => false,
            _ => return false,
        };
        wheel.reverse(reversed, &mut services.status);
        true
    }

    /// Ask for another output, which the main loop opens: one chosen with the page's keys, or
    /// from its list.
    pub(crate) fn choose(&mut self, choice: AudioChoice) {
        if choice != self.audio.choice || !self.audio.running {
            self.request = Some(choice);
        }
    }

    /// A change to the setting under the cursor, `times` steps for `=` and `-`, or passed back.
    fn change(&mut self, cmd: Cmd, times: usize, ctx: &mut CtxMut) -> Outcome {
        let services = &mut *ctx.services;
        let row = self.row_here();
        let done = match row {
            AudioRow::Roto => Self::roto_row(cmd, services),
            AudioRow::Wheel => Self::wheel_row(cmd, ctx.wheel, services),
            AudioRow::WheelDirection => Self::wheel_direction_row(cmd, ctx.wheel, services),
            AudioRow::Describe => Self::describe_row(cmd, services),
            _ => false,
        };
        if done {
            return Outcome::Done;
        }
        match cmd {
            Cmd::Increase | Cmd::Decrease | Cmd::IncreaseLarge | Cmd::DecreaseLarge => {
                let up = matches!(cmd, Cmd::Increase | Cmd::IncreaseLarge);
                let (options, current) = self.options(row, services.rate);
                if options.is_empty() {
                    return Outcome::Refused("This device doesn't say what it takes");
                }
                let at = match (current, up) {
                    (Some(at), true) => (at + times).min(options.len() - 1),
                    (Some(at), false) => at.saturating_sub(times),
                    (None, _) => 0,
                };
                if Some(at) != current {
                    self.choose(options[at].1.clone());
                }
            }
            // Back to the system's own choice for this row.
            Cmd::ClearField => {
                let choice = match row {
                    AudioRow::Device => AudioChoice {
                        input: self.audio.choice.input.clone(),
                        ..AudioChoice::default()
                    },
                    AudioRow::Input => AudioChoice {
                        input: None,
                        ..self.audio.choice.clone()
                    },
                    AudioRow::Rate => AudioChoice {
                        rate: None,
                        ..self.audio.choice.clone()
                    },
                    AudioRow::Buffer => AudioChoice {
                        buffer: None,
                        ..self.audio.choice.clone()
                    },
                    // The Roto's, the wheel's and the description's rows have their own keys,
                    // above.
                    AudioRow::Roto
                    | AudioRow::Wheel
                    | AudioRow::WheelDirection
                    | AudioRow::Describe => {
                        return Outcome::Done;
                    }
                };
                self.choose(choice);
            }
            Cmd::PickValue | Cmd::EnterInsert => return self.picker(row, services.rate),
            _ => return Outcome::Passed,
        }
        Outcome::Done
    }

    /// The row's choices as a list, with the one playing highlighted.
    fn picker(&self, row: AudioRow, rate: u32) -> Outcome {
        let (options, current) = self.options(row, rate);
        if options.is_empty() {
            return Outcome::Refused("There's nothing to choose here");
        }
        let items = options
            .into_iter()
            .map(|(label, choice)| Item::new(label, Pick::Audio(choice)))
            .collect();
        Outcome::Open(Box::new(Picker::new(row.name(), items).opened_on(current)))
    }
}
