//! vimtracker's own setups stored on the Roto, for the devices with off/on settings: only a
//! stored setup has buttons, with their names, colours and lights (`docs/designs/roto-control.md`
//! §4.3). The Roto keeps 64 PLUGIN setups shared by every program that drives it, so this
//! touches only vimtracker's:
//!
//! *   Each is found by the hash of a key in vimtracker's own namespace, `vimtracker:` and the
//!     kind of device, such as `vimtracker:fx/delay`, and named with [`NAME_PREFIX`].
//! *   Before anything is written, the store is listed. A setup under one of vimtracker's hashes
//!     whose name isn't one of vimtracker's is someone else's, and isn't touched; nor is a write
//!     made that would need more room than the store has.
//! *   Every change goes through [`Guarded`], which refuses any request that isn't to one of
//!     vimtracker's own hashes: no other setup, no MIDI setup and never a factory reset.
//! *   A setup is written only when it isn't there or its version, a hash of everything written,
//!     has changed since it was, so plugging in is quick and the Roto's flash isn't worn.
//!
//! [`clear_own`] removes vimtracker's setups and nothing else.

use std::collections::{BTreeMap, BTreeSet};

use xxhash_rust::xxh3::xxh3_64;

use crate::bytes::{Label, ParamHash, PluginHash};
use crate::fake::{FakePorts, FakeRoto};
use crate::live::{Feel, KNOB_TOP, LiveControl, LiveDevice};
use crate::serial::{
    Code, KnobHaptic, MAX_PLUGIN_SETUPS, MAX_STORED_STEPS, PluginKnobConfig, PluginSwitchConfig,
    Reply, Request, StoredPlugin, SwitchHaptic,
};
use crate::transport::{LinkError, SerialLink};

/// What every key of vimtracker's stored setups starts with.
pub const NAMESPACE: &str = "vimtracker:";

/// What every name of vimtracker's stored setups starts with, so they're told apart in the
/// vendor's Roto-Setup app and here.
pub const NAME_PREFIX: &str = "vt ";

/// A button's light while its setting is off: the dark scheme Ableton's stored switches use.
pub const LIGHT_OFF: u8 = 70;

/// A way to send the Roto serial requests and read their replies: its port, or the fake Roto.
pub trait Serial {
    fn exchange(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError>;
}

impl Serial for SerialLink {
    fn exchange(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        if request.is_read_only() {
            self.ask(request)
        } else {
            self.change(request)
        }
    }
}

impl Serial for FakeRoto {
    fn exchange(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        let reply = self.serial_in(&request.encode());
        Ok(request.decode_reply(&reply)?)
    }
}

impl Serial for FakePorts {
    fn exchange(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        self.with(|roto| roto.exchange(request))
    }
}

/// Why vimtracker's setups weren't written.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error("the Roto said {0:?} to {1}")]
    Refused(Code, String),
    #[error(
        "the Roto stores {stored} setups, and {needed} more don't fit in its {MAX_PLUGIN_SETUPS}"
    )]
    Full { stored: usize, needed: usize },
    #[error("refused: {0} isn't one of vimtracker's own setups")]
    NotOurs(String),
}

/// One of vimtracker's stored setups: a device's knobs and buttons as the Roto keeps them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnSetup {
    /// Its key in vimtracker's namespace, which its hash is made from.
    pub key: String,
    pub name: Label,
    pub knobs: Vec<PluginKnobConfig>,
    pub switches: Vec<PluginSwitchConfig>,
}

impl OwnSetup {
    /// The setup for a stored device, from its knobs and buttons as they'd be shown, each
    /// numbered from 1 across its pages as the Roto gives them back in `CONTROL MAPPED`.
    pub fn of(device: &LiveDevice, name: &str) -> Self {
        let hash = device.hash();
        let param =
            |kind: &str, at: usize| ParamHash::of(&format!("{}/{kind}/{}", device.key, at + 1));
        let knobs = device
            .controls
            .iter()
            .enumerate()
            .filter_map(|(at, control)| Some((at, control.as_ref()?)))
            .map(|(at, control)| knob(hash, at, param("knob", at), control))
            .collect();
        let switches = device
            .switches
            .iter()
            .enumerate()
            .filter_map(|(at, control)| Some((at, control.as_ref()?)))
            .map(|(at, control)| switch(hash, at, param("switch", at), control))
            .collect();
        Self {
            key: device.key.clone(),
            name: Label::new(&format!("{NAME_PREFIX}{name}")),
            knobs,
            switches,
        }
    }

    pub fn hash(&self) -> PluginHash {
        PluginHash::of(&self.key)
    }

    /// Everything written to store it, in order, inside a config update.
    fn writes(&self) -> Vec<Request> {
        let mut writes = vec![Request::AddPlugin {
            hash: self.hash(),
            name: self.name.clone(),
        }];
        writes.extend(self.knobs.iter().cloned().map(Request::SetPluginKnobConfig));
        writes.extend(
            self.switches
                .iter()
                .cloned()
                .map(Request::SetPluginSwitchConfig),
        );
        writes
    }

    /// A hash of everything written to store it, which changes when anything in it does.
    pub fn version(&self) -> u64 {
        let bytes: Vec<u8> = self.writes().iter().flat_map(Request::encode).collect();
        xxh3_64(&bytes)
    }
}

/// A knob of a stored setup. The Roto takes at most 16 steps a stored knob, so a longer list
/// is stored smooth; its steps are sent live as the Roto asks for the knob, as for any knob.
fn knob(
    plugin: PluginHash,
    at: usize,
    param: ParamHash,
    control: &LiveControl,
) -> PluginKnobConfig {
    let (haptic, step_names) = match &control.feel {
        Feel::Smooth => (KnobHaptic::Smooth { indents: [None; 2] }, Vec::new()),
        Feel::CentreNotch => (KnobHaptic::CentreIndent, Vec::new()),
        Feel::Steps(names) if names.len() <= usize::from(MAX_STORED_STEPS) => {
            (KnobHaptic::Steps(names.len() as u8), names.clone())
        }
        Feel::Steps(_) => (KnobHaptic::Smooth { indents: [None; 2] }, Vec::new()),
        Feel::Switch => (KnobHaptic::Steps(2), control.feel.step_names()),
    };
    PluginKnobConfig {
        plugin,
        control: at as u8,
        param_index: at as u16 + 1,
        param_hash: param,
        macro_param: false,
        min: 0,
        max: KNOB_TOP,
        name: control.name.clone(),
        colour: control.colour,
        haptic,
        step_names,
    }
}

/// A button of a stored setup: a push button, lit in its setting's colour while it's on. The
/// program flips the setting on each press and sends the light back, so the song is the one
/// place it's kept (the Roto's own toggle mode keeps a state of its own, which drifts from it).
fn switch(
    plugin: PluginHash,
    at: usize,
    param: ParamHash,
    control: &LiveControl,
) -> PluginSwitchConfig {
    PluginSwitchConfig {
        plugin,
        control: at as u8,
        param_index: at as u16 + 1,
        param_hash: param,
        min: 0,
        max: 0x7F,
        name: control.name.clone(),
        colour: control.colour,
        light_on: control.colour,
        light_off: LIGHT_OFF,
        haptic: SwitchHaptic::Push,
        steps: 0,
        step_names: Vec::new(),
    }
}

/// The version of each setup last written, by its key, which the program keeps between runs.
pub type Versions = BTreeMap<String, u64>;

/// What a sync did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Synced {
    /// The setups written, by key.
    pub written: Vec<String>,
    /// How many were there already as they should be.
    pub unchanged: usize,
}

/// Changes to the Roto's store, each refused unless it's to one of vimtracker's own setups.
pub struct Guarded<'a> {
    serial: &'a mut dyn Serial,
    ours: BTreeSet<PluginHash>,
}

impl<'a> Guarded<'a> {
    /// Changes allowed only to these setups.
    pub fn new(serial: &'a mut dyn Serial, ours: impl IntoIterator<Item = PluginHash>) -> Self {
        Self {
            serial,
            ours: ours.into_iter().collect(),
        }
    }

    /// Sends a request: anything that only asks, the start and end of a config update, and a
    /// change to one of vimtracker's own setups. Anything else is refused, unsent.
    pub fn send(&mut self, request: &Request) -> Result<Reply, StoreError> {
        let allowed = request.is_read_only()
            || matches!(
                request,
                Request::StartConfigUpdate | Request::EndConfigUpdate
            )
            || request
                .plugin_changed()
                .is_some_and(|hash| self.ours.contains(&hash));
        if !allowed {
            return Err(StoreError::NotOurs(format!("{request:?}")));
        }
        self.serial
            .exchange(request)?
            .map_err(|code| StoreError::Refused(code, format!("{request:?}")))
    }

    /// Every setup the Roto stores, in its order.
    pub fn list(&mut self) -> Result<Vec<StoredPlugin>, StoreError> {
        let mut stored = Vec::new();
        let mut request = Request::GetFirstPlugin;
        while stored.len() <= MAX_PLUGIN_SETUPS {
            match self.serial.exchange(&request)? {
                Ok(Reply::Plugin(plugin)) => stored.push(plugin),
                _ => break,
            }
            request = Request::GetNextPlugin;
        }
        Ok(stored)
    }

    /// Changes inside one config update, which is ended whatever happens.
    fn update(&mut self, writes: &[Request]) -> Result<(), StoreError> {
        self.send(&Request::StartConfigUpdate)?;
        let result = writes
            .iter()
            .try_for_each(|write| self.send(write).map(|_| ()));
        let ended = self.send(&Request::EndConfigUpdate);
        result.and(ended.map(|_| ()))
    }
}

/// Whether a stored setup is one of vimtracker's: under one of its hashes, with its name.
fn is_ours(stored: &StoredPlugin, ours: &BTreeSet<PluginHash>) -> bool {
    ours.contains(&stored.hash) && stored.name.as_str().starts_with(NAME_PREFIX)
}

/// Stores vimtracker's setups that aren't there or have changed since they were written, and
/// notes each one's version. A setup under one of vimtracker's hashes that isn't vimtracker's
/// stops it, with nothing written.
pub fn sync(
    serial: &mut dyn Serial,
    setups: &[OwnSetup],
    versions: &mut Versions,
) -> Result<Synced, StoreError> {
    let ours: BTreeSet<PluginHash> = setups.iter().map(OwnSetup::hash).collect();
    let mut guarded = Guarded::new(serial, ours.iter().copied());
    let stored = guarded.list()?;
    let mut writes = Vec::new();
    let mut written = Vec::new();
    let mut new = 0;
    let mut synced = Synced::default();
    for setup in setups {
        let found = stored.iter().find(|plugin| plugin.hash == setup.hash());
        if let Some(found) = found
            && !is_ours(found, &ours)
        {
            return Err(StoreError::NotOurs(found.name.as_str().to_owned()));
        }
        let version = setup.version();
        if found.is_some() && versions.get(&setup.key) == Some(&version) {
            synced.unchanged += 1;
            continue;
        }
        if found.is_some() {
            writes.push(Request::ClearPlugin(setup.hash()));
        } else {
            new += 1;
        }
        writes.extend(setup.writes());
        written.push((setup.key.clone(), version));
    }
    if stored.len() + new > MAX_PLUGIN_SETUPS {
        return Err(StoreError::Full {
            stored: stored.len(),
            needed: new,
        });
    }
    if !writes.is_empty() {
        guarded.update(&writes)?;
    }
    for (key, version) in written {
        synced.written.push(key.clone());
        versions.insert(key, version);
    }
    Ok(synced)
}

/// The names of vimtracker's own setups the Roto stores, among those under these hashes.
pub fn own_stored(
    serial: &mut dyn Serial,
    ours: impl IntoIterator<Item = PluginHash>,
) -> Result<Vec<StoredPlugin>, StoreError> {
    let ours: BTreeSet<PluginHash> = ours.into_iter().collect();
    let mut guarded = Guarded::new(serial, ours.iter().copied());
    Ok(guarded
        .list()?
        .into_iter()
        .filter(|stored| is_ours(stored, &ours))
        .collect())
}

/// Removes vimtracker's own setups, among those under these hashes, and nothing else. Says
/// which it removed.
pub fn clear_own(
    serial: &mut dyn Serial,
    ours: impl IntoIterator<Item = PluginHash>,
) -> Result<Vec<StoredPlugin>, StoreError> {
    let ours: BTreeSet<PluginHash> = ours.into_iter().collect();
    let found = own_stored(serial, ours.iter().copied())?;
    if !found.is_empty() {
        let mut guarded = Guarded::new(serial, ours.iter().copied());
        let clears: Vec<Request> = found
            .iter()
            .map(|stored| Request::ClearPlugin(stored.hash))
            .collect();
        guarded.update(&clears)?;
    }
    Ok(found)
}
