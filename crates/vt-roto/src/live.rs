//! Devices shown on the Roto, as Bitwig Studio's script shows a device's remote controls. Most
//! are macro plugins sent live, of as many pages of eight knobs as they need; the Roto asks for
//! each knob of the page it shows with `CONTROL MAPPED`, and is answered with the knob's name,
//! feel and position in `LEARN PARAM` and its name and colour in `SET PLUGIN CONTROL DETAILS`. A
//! device with buttons is one of the program's own stored setups ([`crate::store`]), sent as a
//! normal plugin under its setup's hash: the Roto asks for its knobs, answered the same way with
//! the macro flag off, and its buttons, whose names are the setup's and whose lights the program
//! sends as CCs (`docs/designs/roto-control.md` §4.3).
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

use std::collections::BTreeMap;

use crate::bytes::{Label, ParamHash, PluginHash};
use crate::sysex::{
    CONTROLS, ControlType, FromRoto, LearnParam, MAX_STEPS, PluginDetails, PluginKind, ToRoto,
    TrackDetails,
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
    /// `index` with `hash`: a macro device's, or with the macro flag off a stored device's, which
    /// Ableton's script answers so, and whose knob turns nothing until it's answered.
    pub fn learn(&self, index: u16, hash: ParamHash, macro_param: bool) -> LearnParam {
        LearnParam {
            index,
            hash,
            macro_param,
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

    /// Whether it's on, as a button is: past the middle of its range.
    pub fn is_on(&self) -> bool {
        self.position > KNOB_TOP / 2
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

/// Which of `count` places a knob at a position is nearest, as it clicks through its steps: the
/// way back from [`position_of`].
pub fn step_at(position: u16, count: usize) -> usize {
    if count < 2 {
        return 0;
    }
    let last = count - 1;
    let top = usize::from(KNOB_TOP);
    ((usize::from(position.min(KNOB_TOP)) * last + top / 2) / top).min(last)
}

/// How far along a knob is, from 0 to 1: the way back from [`position_at`].
pub fn share_of(position: u16) -> f64 {
    f64::from(position.min(KNOB_TOP)) / f64::from(KNOB_TOP)
}

/// One device in the Roto's plugin list, its knobs in order, a page of eight after another, with
/// a hole wherever the device has nothing. A device the Roto stores (`stored`) has buttons too,
/// as only a stored setup has them, and is found by its key's hash among its setups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveDevice {
    /// What it's known by, such as `vimtracker/fx1/delay`, which its hash is made from.
    pub key: String,
    pub name: Label,
    /// Whether it's switched in, as an FX slot or a modulator can be switched out.
    pub enabled: bool,
    /// Whether it's one of vimtracker's setups stored on the Roto, rather than a macro device
    /// sent live.
    pub stored: bool,
    pub controls: Vec<Option<LiveControl>>,
    /// A stored device's buttons, a page of eight after another, each off or on.
    pub switches: Vec<Option<LiveControl>>,
}

impl LiveDevice {
    /// How many pages of eight knobs and buttons it has: at least one, and at most
    /// [`MAX_PAGES`].
    pub fn pages(&self) -> u8 {
        self.controls
            .len()
            .max(self.switches.len())
            .div_ceil(CONTROLS)
            .clamp(1, usize::from(MAX_PAGES)) as u8
    }

    /// The button the Roto numbers `index`, from 1 across the pages, if it holds a setting.
    pub fn switch(&self, index: u16) -> Option<&LiveControl> {
        let at = usize::from(index.checked_sub(1)?);
        self.switches.get(at)?.as_ref()
    }

    pub fn hash(&self) -> PluginHash {
        PluginHash::of(&self.key)
    }

    /// The knob the Roto numbers `index`, from 1 across the pages, if it holds a setting.
    pub fn control(&self, index: u16) -> Option<&LiveControl> {
        let at = usize::from(index.checked_sub(1)?);
        self.controls.get(at)?.as_ref()
    }

    /// Whether it shows in the plugin list as `other` does: the same name, state and pages, under
    /// the same key.
    fn listed_as(&self, other: &Self) -> bool {
        self.key == other.key
            && self.name == other.name
            && self.enabled == other.enabled
            && self.stored == other.stored
            && self.pages() == other.pages()
    }

    fn details(&self, index: u8) -> PluginDetails {
        PluginDetails {
            index,
            hash: self.hash(),
            enabled: self.enabled,
            name: self.name.clone(),
            // A stored device's pages are its setup's, and the API has 0 for one that isn't a
            // macro.
            kind: if self.stored {
                PluginKind::Normal
            } else {
                PluginKind::Macro
            },
            macro_pages: if self.stored { 0 } else { self.pages() },
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
    /// The hash the Roto gave each knob of the chosen device it asked for, by its number, so a
    /// knob whose name or feel changes can be learned again without its asking.
    mapped: BTreeMap<u16, ParamHash>,
    /// Buttons of the page shown to light or put out, each by its place on the page, waiting to
    /// be sent as CCs: a stored device's lights are the program's to draw.
    lights: Vec<(u8, bool)>,
}

/// What to send the Roto after the devices changed: SysEx, and knobs to move, each by its place
/// on the page shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refresh {
    pub messages: Vec<ToRoto>,
    pub positions: Vec<(u8, u16)>,
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

    /// The first device of the bank shown, a multiple of 8.
    pub fn first(&self) -> u8 {
        self.first
    }

    /// Shows these devices, keeping the one chosen where it can, and says what to send.
    pub fn show(&mut self, devices: Vec<LiveDevice>) -> Vec<ToRoto> {
        let before = self.selected().map(|device| device.key.clone());
        self.take(devices);
        if self.selected().map(|device| &device.key) != before.as_ref() {
            self.mapped.clear();
        }
        self.push()
    }

    /// Holds these devices, keeping the one chosen, its page and the bank shown where they still
    /// are. The bank is the Roto's to page through, so it stays where SEL's arrows left it
    /// whichever device is chosen, as Bitwig's script keeps it: sent from the chosen device's
    /// bank, toggling a device of the second bank on or off sent the Roto back to the first.
    fn take(&mut self, mut devices: Vec<LiveDevice>) {
        devices.truncate(0x7F);
        let count = devices.len();
        self.devices = devices;
        if usize::from(self.selected) >= count {
            self.selected = 0;
        }
        let pages = self.selected().map_or(1, LiveDevice::pages);
        self.page = self.page.min(pages - 1);
        if usize::from(self.first) >= count {
            let last = count.saturating_sub(1) as u8;
            self.first = last - last % CONTROLS as u8;
        }
    }

    /// Takes a device, by its place in the whole list, as the one the Roto shows, from its first
    /// page if it's another, in its bank.
    pub fn choose(&mut self, index: u8) {
        if usize::from(index) >= self.devices.len() {
            return;
        }
        if index != self.selected {
            self.selected = index;
            self.page = 0;
            self.mapped.clear();
        }
        self.first = self.selected - self.selected % CONTROLS as u8;
    }

    /// Shows the bank the chosen device is in, as on connecting, when the Roto has no bank of its
    /// own yet.
    pub fn bank_to_chosen(&mut self) {
        self.first = self.selected - self.selected % CONTROLS as u8;
    }

    /// Whether the chosen device is in the bank shown.
    fn selected_in_bank(&self) -> bool {
        (self.first..self.first.saturating_add(CONTROLS as u8)).contains(&self.selected)
    }

    /// The number the Roto gives a knob of the page shown, from 1 across the device's pages.
    pub fn index_of(&self, knob: u8) -> u16 {
        u16::from(self.page) * CONTROLS as u16 + u16::from(knob % CONTROLS as u8) + 1
    }

    /// The device the Roto means by an index in `ROTO SELECT PLUGIN` or `SET PLUGIN ENABLED`. The
    /// real Roto counts it from the bank shown (firmware 3.3.0: holding SEL on the second bank and
    /// pressing its second button sent 1), as both vendor scripts read it. A place in the whole
    /// list is taken as it is, as the two can't be mistaken: a place in the bank is below 8, and
    /// a later bank's first device is at 8 or more.
    pub fn device_for(&self, index: u8) -> u8 {
        if index < self.first {
            self.first.saturating_add(index)
        } else {
            index
        }
    }

    /// Shows the devices as they are after an edit, sending only what changed: the list again if
    /// a device's name, state or pages did, a knob of the page shown learned again if its name,
    /// feel or colour did, and a knob moved if only its position did. A knob `touched` isn't
    /// moved under the hand, and keeps the position last sent, so it goes where it should once
    /// it's let go if something else moved its setting meanwhile. A knob just `turned` on the
    /// Roto takes its setting's position without being sent it, as it's there already.
    pub fn refresh(
        &mut self,
        mut devices: Vec<LiveDevice>,
        touched: [bool; CONTROLS],
        turned: [bool; CONTROLS],
    ) -> Refresh {
        devices.truncate(0x7F);
        let listed = devices.len() == self.devices.len()
            && devices
                .iter()
                .zip(&self.devices)
                .all(|(new, old)| new.listed_as(old));
        let before = self.selected().cloned();
        let indices: Vec<u16> = (0..CONTROLS as u8)
            .map(|knob| self.index_of(knob))
            .collect();
        if let (Some(old), Some(new)) = (&before, devices.get_mut(usize::from(self.selected))) {
            for (knob, index) in indices.iter().enumerate() {
                let at = usize::from(index - 1);
                if touched[knob]
                    && !turned[knob]
                    && let (Some(Some(was)), Some(Some(now))) =
                        (old.controls.get(at), new.controls.get_mut(at))
                {
                    now.position = was.position;
                }
            }
        }
        let page = self.page;
        self.take(devices);
        let mut refresh = Refresh::default();
        if !listed {
            refresh.messages = self.push();
        }
        let (Some(old), Some(new)) = (before, self.selected()) else {
            return refresh;
        };
        if self.page != page {
            return refresh;
        }
        let same_device = old.key == new.key;
        let mut lights = Vec::new();
        for (knob, index) in indices.into_iter().enumerate() {
            let was = old.control(index).filter(|_| same_device);
            match (was, new.control(index)) {
                (Some(_), None) => refresh.messages.push(ToRoto::UnmapControl {
                    control: ControlType::Knob,
                    index: knob as u8,
                }),
                (None, None) => {}
                (Some(was), Some(now))
                    if was.name == now.name && was.feel == now.feel && was.colour == now.colour =>
                {
                    if was.position != now.position && !touched[knob] && !turned[knob] {
                        refresh.positions.push((knob as u8, now.position));
                    }
                }
                (_, Some(now)) => {
                    if let Some(hash) = self.mapped.get(&index) {
                        refresh.messages.push(ToRoto::LearnParam(now.learn(
                            index,
                            *hash,
                            !new.stored,
                        )));
                        refresh.messages.push(now.details(index));
                    }
                }
            }
            // A stored device's button whose setting changed is lit as it now is; a device
            // newly shown has its lights sent as the Roto asks for its buttons.
            if same_device
                && let Some(now) = new.switch(index)
                && old.switch(index).map(LiveControl::is_on) != Some(now.is_on())
            {
                lights.push((knob as u8, now.is_on()));
            }
        }
        self.lights.extend(lights);
        refresh
    }

    /// The buttons of the page shown to light or put out since this was last asked, each by its
    /// place on the page.
    pub fn take_lights(&mut self) -> Vec<(u8, bool)> {
        // Each button once, as it was last asked to be.
        let mut lights: Vec<(u8, bool)> = Vec::new();
        for (button, lit) in std::mem::take(&mut self.lights) {
            lights.retain(|(other, _)| *other != button);
            lights.push((button, lit));
        }
        lights
    }

    /// Lights a button of the page shown as its setting is, as after it's pressed: the Roto
    /// draws a press itself, and the light shows the setting only once it's sent.
    pub fn relight(&mut self, button: u8) {
        if let Some(lit) = self.button(button).map(LiveControl::is_on) {
            self.lights.push((button, lit));
        }
    }

    /// The setting on a button of the page shown, if the device chosen is stored and has one
    /// there.
    pub fn button(&self, button: u8) -> Option<&LiveControl> {
        let index = self.index_of(button);
        self.selected().and_then(|device| device.switch(index))
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
        // Only from the bank shown, as Bitwig's script sends it, so the Roto stays on its bank.
        if self.selected().is_some() && self.selected_in_bank() {
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
            FromRoto::RotoSelectPlugin(index)
                if usize::from(self.device_for(index)) < self.devices.len() =>
            {
                self.choose(self.device_for(index));
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
                self.mapped.insert(index, hash);
                self.answer_mapped(index, hash, control_index)
            }
            // A stored device's button, numbered from 1 across its pages as its knobs are: it
            // has its name and colours from the setup, and its light is the program's to send.
            FromRoto::ControlMapped {
                index,
                control: ControlType::Switch,
                control_index,
                ..
            } => {
                if let Some(at) = index.checked_sub(1) {
                    self.turn_to((at / CONTROLS as u16).min(0x7F) as u8);
                }
                if let Some(switch) = self.selected().and_then(|device| device.switch(index)) {
                    let lit = switch.is_on();
                    self.lights.push((control_index % CONTROLS as u8, lit));
                }
                Vec::new()
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
        vec![ToRoto::ShowControlValue {
            control: ControlType::Knob,
            index: knob,
            value: names[step_at(position, names.len())].clone(),
        }]
    }

    /// The answer to `CONTROL MAPPED` for the knob numbered `index`.
    fn answer_mapped(&self, index: u16, hash: ParamHash, control_index: u8) -> Vec<ToRoto> {
        let device = self.selected();
        let stored = device.is_some_and(|device| device.stored);
        match device.and_then(|device| device.control(index)) {
            Some(control) => vec![
                ToRoto::LearnParam(control.learn(index, hash, !stored)),
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

/// One track as the Roto's track list shows it, on the buttons PLUGIN mode's FUNC shows, to
/// choose which track's devices it shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LiveTrack {
    pub name: Label,
    /// An index into [`crate::colour::SCHEMES`].
    pub colour: u8,
    /// Whether it's a group.
    pub grouped: bool,
}

/// The Roto's track list: every track, the bank of eight shown, and the one chosen, whose devices
/// PLUGIN mode shows. The Roto's FUNC shows the bank on its buttons, its arrows page through the
/// banks (`SET FIRST TRACK`), and a button chooses a track (`ROTO SELECT TRACK`), as with Ableton
/// Live's and Bitwig Studio's scripts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackList {
    tracks: Vec<LiveTrack>,
    /// The first track of the bank shown, a multiple of 8.
    first: u16,
    selected: u16,
}

impl TrackList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tracks(&self) -> &[LiveTrack] {
        &self.tracks
    }

    pub fn selected(&self) -> u16 {
        self.selected
    }

    /// Shows these tracks with one chosen, and says what to send: nothing if it's all as shown.
    pub fn update(&mut self, tracks: Vec<LiveTrack>, selected: u16) -> Vec<ToRoto> {
        if tracks == self.tracks && selected == self.selected {
            return Vec::new();
        }
        self.show(tracks, selected)
    }

    /// Shows these tracks with one chosen, in the bank it's in, and says what to send.
    pub fn show(&mut self, mut tracks: Vec<LiveTrack>, selected: u16) -> Vec<ToRoto> {
        tracks.truncate(usize::from(crate::bytes::PAIR_MAX));
        self.tracks = tracks;
        self.selected = selected.min(self.last());
        self.first = self.selected - self.selected % CONTROLS as u16;
        self.push()
    }

    fn last(&self) -> u16 {
        self.tracks.len().saturating_sub(1) as u16
    }

    fn details(&self, index: u16) -> Option<TrackDetails> {
        let track = self.tracks.get(usize::from(index))?;
        Some(TrackDetails {
            index,
            name: track.name.clone(),
            colour: track.colour,
            grouped: track.grouped,
        })
    }

    /// Everything the Roto needs to show the list: how many tracks, the bank, each track in it,
    /// and the one chosen, as Ableton's script sends them.
    pub fn push(&self) -> Vec<ToRoto> {
        let count = self.tracks.len() as u16;
        let mut messages = vec![ToRoto::NumTracks(count), ToRoto::FirstTrack(self.first)];
        let bank = self.first..(self.first + CONTROLS as u16).min(count);
        messages.extend(bank.filter_map(|index| self.details(index).map(ToRoto::TrackDetails)));
        messages.push(ToRoto::TrackDetailsEnd);
        messages.extend(self.details(self.selected).map(ToRoto::DawSelectTrack));
        messages
    }

    /// What to send for a message from the Roto, and the track chosen on it, if one was.
    pub fn receive(&mut self, message: &FromRoto) -> (Vec<ToRoto>, Option<u16>) {
        match *message {
            FromRoto::SetFirstTrack(first) if first <= self.last() => {
                self.first = first - first % CONTROLS as u16;
                (self.push(), None)
            }
            FromRoto::RotoSelectTrack(index) if usize::from(index) < self.tracks.len() => {
                self.selected = index;
                self.first = index - index % CONTROLS as u16;
                (Vec::new(), Some(index))
            }
            _ => (Vec::new(), None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_found_back_from_its_position() {
        for count in 2..=24 {
            for at in 0..count {
                assert_eq!(step_at(position_of(at, count), count), at);
            }
        }
        assert_eq!(step_at(KNOB_TOP, 1), 0);
        assert!((share_of(position_at(0.3)) - 0.3).abs() < 1e-4);
    }

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
