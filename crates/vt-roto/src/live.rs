//! Devices shown live on the Roto, with nothing stored on it, as Bitwig Studio's script shows a
//! device's remote controls. Each device is a macro plugin of as many pages of eight knobs as it
//! needs; the Roto asks for each knob of the page it shows with `CONTROL MAPPED`, and is answered
//! with the knob's name, feel and position in `LEARN PARAM` and its name and colour in `SET PLUGIN
//! CONTROL DETAILS`. None of this touches the Roto's 64 stored PLUGIN setups, which the serial API
//! writes and this never does, so a user's Ableton setups stay as they are.
//!
//! What the real Roto does (firmware 3.3.0, checked on 2026-11-11, `docs/designs/roto-control.md`
//! §4): after `DAW SELECT PLUGIN` it says `SET PLUGIN MODE`, then sends `CONTROL MAPPED` for each
//! knob of the page shown, numbered from 1 across the device's pages (9 to 16 on the second) with
//! a hash of its own, which the answer gives back. `LEARN PARAM` sent before it asks is dropped,
//! and so is one for a knob of a page it isn't showing. `SET PLUGIN CONTROL DETAILS` takes the
//! same number. The serial API's `GET CURRENT PLUGIN` reads such a device back as a
//! `MacroAutoConfig` that isn't among the stored setups.
//!
//! [`PluginView`] is what the Roto's PLUGIN mode shows, with no I/O of its own: it's given the
//! devices, and says what to send for each message from the Roto, so the link and the tests drive
//! the same code.

use crate::bytes::{Label, ParamHash, PluginHash};
use crate::sysex::{
    CONTROLS, ControlType, FromRoto, LearnParam, MAX_STEPS, PluginDetails, PluginKind, ToRoto,
};

/// The most pages of eight knobs a device can have, as the Roto keeps 64 controls a plugin.
pub const MAX_PAGES: u8 = 8;

/// The top of a knob's range.
pub const KNOB_TOP: u16 = 16383;

/// How a knob feels as it turns.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Feel {
    /// Turns freely.
    Smooth,
    /// Turns freely, with a notch at the top for the middle, as pan's centre.
    CentreNotch,
    /// Clicks through its steps, 2 to 24, the first 16 each with a name the Roto shows.
    Steps(Vec<Label>),
    /// Off or on: two steps, named `Off` and `On`.
    Switch,
}

impl Feel {
    /// How many steps it clicks through, or 0 for a smooth knob.
    pub fn steps(&self) -> u8 {
        match self {
            Self::Smooth | Self::CentreNotch => 0,
            Self::Steps(names) => names.len().clamp(2, usize::from(MAX_STEPS)) as u8,
            Self::Switch => 2,
        }
    }

    /// The names of its steps, as many as it has.
    pub fn step_names(&self) -> Vec<Label> {
        match self {
            Self::Smooth | Self::CentreNotch => Vec::new(),
            Self::Steps(names) => names.iter().take(usize::from(MAX_STEPS)).cloned().collect(),
            Self::Switch => vec![Label::new("Off"), Label::new("On")],
        }
    }
}

/// One knob of a device, as the Roto shows it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveControl {
    /// Up to 12 characters, as [`Label`] cuts it.
    pub name: Label,
    pub feel: Feel,
    /// Where it's set, from 0 to [`KNOB_TOP`].
    pub position: u16,
    /// An index into [`crate::colour::SCHEMES`].
    pub colour: u8,
}

impl LiveControl {
    /// Its `LEARN PARAM`, as the answer to the Roto's `CONTROL MAPPED` for the knob numbered
    /// `index` with `hash`.
    pub fn learn(&self, index: u16, hash: ParamHash) -> LearnParam {
        LearnParam {
            index,
            hash,
            macro_param: true,
            centre_indent: self.feel == Feel::CentreNotch,
            steps: self.feel.steps(),
            position: self.position.min(KNOB_TOP),
            name: self.name.clone(),
            step_names: self.feel.step_names(),
        }
    }

    /// Its name and colour, for the knob numbered `index`.
    pub fn details(&self, index: u16) -> ToRoto {
        ToRoto::SetPluginControlDetails {
            control: ControlType::Knob,
            index: index.min(0x7F) as u8,
            name: self.name.clone(),
            colour: self.colour,
        }
    }
}

/// A position for the `at`th of `count` places, the first at 0 and the last at the top, as the
/// Roto spreads a stepped knob's steps.
pub fn position_of(at: usize, count: usize) -> u16 {
    if count < 2 {
        return 0;
    }
    let at = at.min(count - 1) as u64;
    ((at * u64::from(KNOB_TOP) + (count as u64 - 1) / 2) / (count as u64 - 1)) as u16
}

/// A position for a share of the way along, from 0 to 1.
pub fn position_at(share: f64) -> u16 {
    (share.clamp(0.0, 1.0) * f64::from(KNOB_TOP)).round() as u16
}

/// One device in the Roto's plugin list, its knobs in order, a page of eight after another, with
/// a hole wherever the device has nothing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveDevice {
    /// What it's known by, such as `vimtracker/fx1/delay`, which its hash is made from.
    pub key: String,
    pub name: Label,
    /// Whether it's switched in, as an FX slot or a modulator can be switched out.
    pub enabled: bool,
    pub controls: Vec<Option<LiveControl>>,
}

impl LiveDevice {
    /// How many pages of eight knobs it has: at least one, and at most [`MAX_PAGES`].
    pub fn pages(&self) -> u8 {
        self.controls
            .len()
            .div_ceil(CONTROLS)
            .clamp(1, usize::from(MAX_PAGES)) as u8
    }

    pub fn hash(&self) -> PluginHash {
        PluginHash::of(&self.key)
    }

    /// The knob the Roto numbers `index`, from 1 across the pages, if it holds a setting.
    pub fn control(&self, index: u16) -> Option<&LiveControl> {
        let at = usize::from(index.checked_sub(1)?);
        self.controls.get(at)?.as_ref()
    }

    fn details(&self, index: u8) -> PluginDetails {
        PluginDetails {
            index,
            hash: self.hash(),
            enabled: self.enabled,
            name: self.name.clone(),
            kind: PluginKind::Macro,
            macro_pages: self.pages(),
        }
    }
}

/// What the Roto's PLUGIN mode shows: a list of devices, the bank of eight it shows, the device
/// chosen and the page of it shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginView {
    devices: Vec<LiveDevice>,
    /// The first device of the bank shown, a multiple of 8.
    first: u8,
    selected: u8,
    page: u8,
}

impl PluginView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn devices(&self) -> &[LiveDevice] {
        &self.devices
    }

    /// The device chosen, if there's one.
    pub fn selected(&self) -> Option<&LiveDevice> {
        self.devices.get(usize::from(self.selected))
    }

    pub fn selected_index(&self) -> u8 {
        self.selected
    }

    /// The page of the chosen device shown, from 0.
    pub fn page(&self) -> u8 {
        self.page
    }

    /// Shows these devices, keeping the one chosen where it can, and says what to send.
    pub fn show(&mut self, devices: Vec<LiveDevice>) -> Vec<ToRoto> {
        let count = devices.len().min(0x7F);
        self.devices = devices;
        self.devices.truncate(count);
        if usize::from(self.selected) >= count {
            self.selected = 0;
        }
        let pages = self.selected().map_or(1, LiveDevice::pages);
        self.page = self.page.min(pages - 1);
        self.first = self.selected - self.selected % CONTROLS as u8;
        self.push()
    }

    /// Everything the Roto needs to show the view: how many devices, the bank, each device in it,
    /// and which is chosen on which page. The Roto then asks for each knob of that page.
    pub fn push(&self) -> Vec<ToRoto> {
        let mut messages = vec![
            ToRoto::NumPlugins(self.devices.len() as u8),
            ToRoto::FirstPlugin(self.first),
        ];
        let bank = self
            .devices
            .iter()
            .enumerate()
            .skip(usize::from(self.first))
            .take(CONTROLS);
        for (index, device) in bank {
            messages.push(ToRoto::PluginDetails(device.details(index as u8)));
        }
        messages.push(ToRoto::PluginDetailsEnd);
        if self.selected().is_some() {
            messages.push(ToRoto::DawSelectPlugin {
                index: self.selected,
                macro_page: self.page,
                // A device locked on the Roto stays, as Bitwig's script leaves it.
                force: false,
            });
        }
        messages
    }

    /// What to send for a message from the Roto.
    pub fn receive(&mut self, message: &FromRoto) -> Vec<ToRoto> {
        match *message {
            // PLUGIN mode chosen, or the view changed: the whole of it, which the Roto doesn't
            // answer again when nothing has changed.
            FromRoto::SetPluginMode => self.push(),
            FromRoto::SetFirstPlugin(first) => {
                self.first = first - first % CONTROLS as u8;
                self.push()
            }
            FromRoto::RotoSelectPlugin(index) if usize::from(index) < self.devices.len() => {
                if index != self.selected {
                    self.selected = index;
                    self.page = 0;
                }
                self.first = self.selected - self.selected % CONTROLS as u8;
                self.push()
            }
            FromRoto::MacroPluginPageIndex(page) => {
                self.turn_to(page);
                Vec::new()
            }
            // The arrows on a macro plugin, as the real Roto sends them (firmware 3.3.0), rather
            // than the page's index the PDF has. It then asks for the page's knobs itself.
            FromRoto::PageLeft => {
                self.turn_to(self.page.saturating_sub(1));
                Vec::new()
            }
            FromRoto::PageRight => {
                self.turn_to(self.page.saturating_add(1));
                Vec::new()
            }
            FromRoto::ControlMapped {
                index,
                hash,
                control: ControlType::Knob,
                control_index,
                ..
            } => {
                // The knobs it asks for are the page it shows, whatever moved it there.
                if let Some(at) = index.checked_sub(1) {
                    self.turn_to((at / CONTROLS as u16).min(0x7F) as u8);
                }
                self.answer_mapped(index, hash, control_index)
            }
            _ => Vec::new(),
        }
    }

    /// Shows a page of the chosen device, if it has that many.
    fn turn_to(&mut self, page: u8) {
        let pages = self.selected().map_or(1, LiveDevice::pages);
        self.page = page.min(pages - 1);
    }

    /// What a knob of the page shown says as it's turned to a position: a stepped knob's step,
    /// by its name, which the Roto shows only when it's told, as Ableton's script tells it each
    /// value. A smooth knob's value is the app's to say, as it edits the song (#414).
    pub fn knob_turned(&self, knob: u8, position: u16) -> Vec<ToRoto> {
        let index = u16::from(self.page) * CONTROLS as u16 + u16::from(knob) + 1;
        let Some(control) = self.selected().and_then(|device| device.control(index)) else {
            return Vec::new();
        };
        let names = control.feel.step_names();
        if names.len() < 2 {
            return Vec::new();
        }
        let last = names.len() - 1;
        let step = (usize::from(position.min(KNOB_TOP)) * last + usize::from(KNOB_TOP) / 2)
            / usize::from(KNOB_TOP);
        vec![ToRoto::ShowControlValue {
            control: ControlType::Knob,
            index: knob,
            value: names[step.min(last)].clone(),
        }]
    }

    /// The answer to `CONTROL MAPPED` for the knob numbered `index`.
    fn answer_mapped(&self, index: u16, hash: ParamHash, control_index: u8) -> Vec<ToRoto> {
        match self.selected().and_then(|device| device.control(index)) {
            Some(control) => vec![
                ToRoto::LearnParam(control.learn(index, hash)),
                control.details(index),
            ],
            // A hole: nothing on that knob.
            None => vec![ToRoto::UnmapControl {
                control: ControlType::Knob,
                index: control_index,
            }],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_spread_steps_from_the_bottom_to_the_top() {
        assert_eq!(position_of(0, 4), 0);
        assert_eq!(position_of(3, 4), KNOB_TOP);
        assert_eq!(position_of(1, 2), KNOB_TOP);
        assert_eq!(position_of(0, 1), 0);
        assert_eq!(position_at(0.5), 8192);
    }

    #[test]
    fn a_switch_is_two_named_steps() {
        assert_eq!(Feel::Switch.steps(), 2);
        assert_eq!(
            Feel::Switch.step_names(),
            [Label::new("Off"), Label::new("On")]
        );
    }
}
