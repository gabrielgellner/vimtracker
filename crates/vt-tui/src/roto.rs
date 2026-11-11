//! The Roto-Control, as vimtracker drives it (`docs/designs/roto-control.md`): the setting that
//! turns it on, kept for this machine in `roto.ron` beside `audio.ron`; the link to it while it's
//! on; and what its PLUGIN mode shows, a track's devices sent live with nothing stored on the
//! Roto ([`devices`]). Off, as it starts, nothing is opened and the Roto is left to Live.
//!
//! Only one program may drive the Roto at a time (§6 of the note), so turning it on takes the
//! Roto, and turning it off, or quitting, says `DAW STARTED` and closes the link, so a running
//! Live with the Roto's script answers its ping and takes it back.
//!
//! For now PLUGIN mode shows the devices of the track the cursor was on when the Roto connected
//! or was put in PLUGIN mode, so each device can be checked on the Roto; its knobs don't edit the
//! song yet. Choosing the track from the Roto, and its knobs' edits, are #413's; MIX mode is
//! #412's.

pub mod devices;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use vt_roto::cc::ControlEvent;
use vt_roto::link::{Event, Link, Options};
use vt_roto::live::PluginView;
use vt_roto::session::Change;
use vt_roto::sysex::{FromRoto, ToRoto};
use vt_roto::transport::{Ports, SystemPorts};

use crate::app::App;

/// The setting's file, in the config folder beside `audio.ron`.
pub const SETTING_FILE: &str = "roto.ron";

/// Whether vimtracker drives the Roto, remembered for this machine rather than the song.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RotoSetting {
    /// Off unless turned on, so opening vimtracker with Live running never takes the Roto from it
    /// (the author's decision of 2026-11-11).
    pub on: bool,
}

impl RotoSetting {
    /// Where it's kept: `roto.ron` in the config folder.
    pub fn path() -> Option<PathBuf> {
        crate::config::config_folder().map(|folder| folder.join(SETTING_FILE))
    }

    /// The setting kept at `path`, or off if there's none or it won't read.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| ron::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Keep it at `path` for next time.
    pub fn save_to(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let text = ron::to_string(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// How the link to the Roto is, for the audio page to say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Connection {
    /// Roto support is off.
    #[default]
    Off,
    /// On, and waiting for its ports to appear.
    Looking,
    /// Its ports are open, and the handshake has started.
    Connecting,
    /// Connected, with its firmware's version once it's said.
    Connected { firmware: Option<String> },
    /// The ports went away or couldn't be opened, and the link is looking for them again.
    Lost(String),
}

impl Connection {
    /// What the audio page says of it, after the switch.
    pub fn describe(&self) -> String {
        match self {
            Self::Off => "Off, so Live can have it".to_owned(),
            Self::Looking => "On, looking for it".to_owned(),
            Self::Connecting => "On, connecting".to_owned(),
            Self::Connected {
                firmware: Some(firmware),
            } => format!("On, connected, firmware {firmware}"),
            Self::Connected { firmware: None } => "On, connected".to_owned(),
            Self::Lost(why) => format!("On, looking for it again: {why}"),
        }
    }
}

/// Which mode the Roto last said it was in. It doesn't say on connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RotoMode {
    #[default]
    Unknown,
    Plugin,
    Mix,
}

/// Makes the ports the link opens: the system's, or a test's fake Roto.
pub type MakePorts = Box<dyn Fn() -> Box<dyn Ports>>;

/// The Roto as the app holds it.
pub struct Roto {
    pub setting: RotoSetting,
    /// Where the setting is saved when it changes, or nowhere, as in tests.
    pub setting_path: Option<PathBuf>,
    pub connection: Connection,
    pub mode: RotoMode,
    /// What PLUGIN mode shows.
    pub view: PluginView,
    /// The track whose devices PLUGIN mode shows.
    pub track: usize,
    /// How the link behaves, such as how often it looks for the ports.
    pub options: Options,
    ports: MakePorts,
    link: Option<Link>,
}

impl Default for Roto {
    fn default() -> Self {
        Self {
            setting: RotoSetting::default(),
            setting_path: None,
            connection: Connection::Off,
            mode: RotoMode::Unknown,
            view: PluginView::new(),
            track: 0,
            options: Options::default(),
            ports: Box::new(|| Box::new(SystemPorts::new())),
            link: None,
        }
    }
}

impl Roto {
    /// Opens the link with these ports from now on, as a test puts its fake Roto in.
    pub fn use_ports(&mut self, make: impl Fn() -> Box<dyn Ports> + 'static) {
        self.ports = Box::new(make);
    }

    /// Whether the link is open.
    pub fn is_linked(&self) -> bool {
        self.link.is_some()
    }

    /// Starts the link, which looks for the Roto and connects when it's there.
    fn start(&mut self) {
        if self.link.is_some() {
            return;
        }
        match Link::spawn((self.ports)(), self.options) {
            Ok(link) => {
                self.link = Some(link);
                self.connection = Connection::Looking;
            }
            Err(error) => self.connection = Connection::Lost(error.to_string()),
        }
        self.mode = RotoMode::Unknown;
    }

    /// Gives the Roto back and closes the link.
    fn stop(&mut self) {
        if let Some(link) = self.link.take() {
            link.hand_back();
        }
        self.connection = Connection::Off;
        self.mode = RotoMode::Unknown;
    }

    fn send(&self, messages: Vec<ToRoto>) {
        if let Some(link) = &self.link {
            for message in messages {
                link.send(message);
            }
        }
    }
}

impl Drop for Roto {
    /// Quitting gives the Roto back, as turning it off does.
    fn drop(&mut self) {
        self.stop();
    }
}

impl App {
    /// Starts driving the Roto as the program opens, if its setting is on.
    pub fn start_roto_if_on(&mut self) {
        if self.roto.setting.on {
            self.roto.start();
        }
    }

    /// Turns Roto support on or off, keeping the choice for this machine. On takes the Roto; off
    /// gives it back, so a running Live takes it.
    pub fn set_roto(&mut self, on: bool) {
        if self.roto.setting.on == on && self.roto.is_linked() == on {
            return;
        }
        self.roto.setting.on = on;
        if on {
            self.roto.start();
            self.status =
                "Roto-Control on: it's vimtracker's while this is on, so set Live's control \
                 surface to None meanwhile"
                    .to_owned();
        } else {
            self.roto.stop();
            self.status =
                "Roto-Control off: it's given back, and a running Live takes it".to_owned();
        }
        if let Some(path) = self.roto.setting_path.clone()
            && let Err(error) = self.roto.setting.save_to(&path)
        {
            self.status = format!("{}: couldn't be saved: {error}", path.display());
        }
    }

    /// The devices PLUGIN mode shows, from the song as it is.
    fn roto_devices(&self) -> Vec<vt_roto::live::LiveDevice> {
        let tracks = &self.song().tracks;
        let Some(track) = tracks.get(self.roto.track).or_else(|| tracks.first()) else {
            return Vec::new();
        };
        devices::live_devices(track, self.song())
    }

    /// Shows the cursor's track's devices from now on.
    fn roto_follow_cursor_once(&mut self) {
        self.roto.track = self.cursor.track;
    }

    /// Takes what the Roto said since the last frame, and answers it.
    pub(crate) fn tick_roto(&mut self) {
        let events = match &self.roto.link {
            Some(link) => link.poll(),
            None => return,
        };
        for event in events {
            match event {
                Event::Opened => self.roto.connection = Connection::Connecting,
                Event::Lost(why) => self.roto.connection = Connection::Lost(why),
                Event::Session(Change::Connected) => {
                    self.roto.connection = Connection::Connected { firmware: None };
                    self.roto.mode = RotoMode::Unknown;
                    // It doesn't say its mode on connecting, so the plugin list goes now, which
                    // shows at once in PLUGIN mode and is sent again when PLUGIN mode is chosen.
                    self.roto_follow_cursor_once();
                    let messages = self.roto.view.show(self.roto_devices());
                    self.roto.send(messages);
                }
                Event::Session(Change::FirmwareVersion(version)) => {
                    self.roto.connection = Connection::Connected {
                        firmware: Some(format!(
                            "{}.{}.{}",
                            version.major, version.minor, version.patch
                        )),
                    };
                }
                Event::Session(Change::ApiVersion { .. }) => {}
                Event::Session(Change::Message(message)) => self.roto_message(&message),
                // A stepped knob shows its step's name as it turns, which the Roto shows only
                // when told. Knobs editing the song are #413's and #412's.
                Event::Control(ControlEvent::Knob { index, value })
                    if self.roto.mode != RotoMode::Mix =>
                {
                    let messages = self.roto.view.knob_turned(index, value);
                    self.roto.send(messages);
                }
                Event::Control(_) => {}
            }
        }
    }

    fn roto_message(&mut self, message: &FromRoto) {
        match message {
            FromRoto::SetPluginMode => {
                if self.roto.mode != RotoMode::Plugin {
                    self.roto_follow_cursor_once();
                }
                self.roto.mode = RotoMode::Plugin;
                // The devices as the song has them now; the Roto answers only what changed.
                let messages = self.roto.view.show(self.roto_devices());
                self.roto.send(messages);
            }
            FromRoto::SetMixAllTracksMode { .. }
            | FromRoto::SetMixTrackMode(_)
            | FromRoto::SetAllTracksMode(_) => self.roto.mode = RotoMode::Mix,
            message => {
                let messages = self.roto.view.receive(message);
                self.roto.send(messages);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_is_off_until_turned_on_and_reads_back_as_saved() {
        let folder = std::env::temp_dir().join(format!("vt-roto-setting-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join("vimtracker").join(SETTING_FILE);
        assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: false });
        RotoSetting { on: true }.save_to(&path).unwrap();
        assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: true });
        RotoSetting { on: false }.save_to(&path).unwrap();
        assert_eq!(RotoSetting::load_from(&path), RotoSetting { on: false });
        std::fs::write(&path, "(on: maybe)").unwrap();
        assert_eq!(
            RotoSetting::load_from(&path),
            RotoSetting { on: false },
            "a file that won't read leaves it off"
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
