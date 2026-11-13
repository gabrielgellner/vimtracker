//! The Roto-Control, as vimtracker drives it (`docs/designs/roto-control.md`): the setting that
//! turns it on, kept for this machine in `roto.ron` beside `audio.ron`; the link to it while it's
//! on; and what its PLUGIN mode shows, a track's devices, each laid out by its kind's layout
//! ([`layouts`], [`devices`]): live, or for a device with off/on settings one of vimtracker's own
//! setups stored on the Roto, written over its serial port on a thread of its own when they're
//! missing or changed, so those settings are on its buttons. Off, as it starts, nothing is opened
//! and the Roto is left to Live.
//!
//! Only one program may drive the Roto at a time (§6 of the note), so turning it on takes the
//! Roto, and turning it off, or quitting, says `DAW STARTED` and closes the link, so a running
//! Live with the Roto's script answers its ping and takes it back.
//!
//! **The Roto's own view.** PLUGIN mode shows one track's devices, the track the Roto chose
//! with FUNC, from its track list, or at first the cursor's; the Roto's SEL chooses the device
//! and switches it in and out, and its arrows turn its pages, as the vendor's scripts have them
//! for Live and Bitwig. Nothing on screen changes what it shows, and it changes nothing on
//! screen (the design note's rule).
//!
//! **Its knobs edit the song** through [`App::apply`], as keys do ([`edits`]): each knob's last
//! move of a frame is one edit, and everything from touching a knob to letting go is one undo
//! step. Each value is shown on the knob's screen as the app writes it, at most 12 times a
//! second. After any edit, from the Roto, the keys or an undo, what changed is sent back, but a
//! knob being touched is never moved under the hand. A stored device's button switches its
//! setting as it's pressed, and its light is sent back from the song.
//!
//! **MIX mode is the mixer** ([`mix`]), as the vendor's manual has it: Level on the knobs and
//! mutes on the buttons, SEL held for Pan, a send, or solos, FUNC for one track's faders, the
//! arrows paging the banks of eight and both together for the returns and the master. Its knobs
//! edit the song as PLUGIN mode's do, its buttons mute and solo as the keys do, and its meters
//! show the engine's levels 12 times a second. The Roto's transport buttons play, stop and
//! record in either mode.

pub mod devices;
pub mod edits;
pub mod layouts;
pub mod mix;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use vt_engine::engine::Transport;
use vt_engine::link::Command;
use vt_model::action::Action;
use vt_model::settings::FADER_COUNT;
use vt_model::song::{Song, Track};
use vt_roto::Label;
use vt_roto::cc::{self, ControlEvent};
use vt_roto::link::{Event, Link, Options};
use vt_roto::live::{KNOB_TOP, LiveDevice, PluginView, TrackList};
use vt_roto::mix::MixView;
use vt_roto::session::Change;
use vt_roto::store::{self, Serial, Synced, Versions};
use vt_roto::sysex::{CONTROLS, ControlType, FromRoto, SwitchMode, ToRoto, TransportStatus};
use vt_roto::transport::{LinkError, Ports, SerialLink, SystemPorts};

use crate::track_page::Control;

use crate::app::{App, TrackLive};
use crate::automating::as_heard;

/// The setting's file, in the config folder beside `audio.ron`.
pub const SETTING_FILE: &str = "roto.ron";

/// Which version of each of vimtracker's stored setups is on the Roto, in the program's cache
/// folder, so a setup is written again only when it changes.
pub const VERSIONS_FILE: &str = "roto-setups.ron";

/// How many times the serial port is tried, and how long apart, before the devices with buttons
/// are left live.
const SERIAL_TRIES: usize = 3;
const SERIAL_RETRY: Duration = Duration::from_secs(1);

/// Opens the Roto's serial port, or the fake Roto's, for writing vimtracker's setups.
pub type OpenSerial = Arc<dyn Fn() -> Result<Box<dyn Serial + Send>, LinkError> + Send + Sync>;

/// The versions of vimtracker's setups last written, kept at `path`, or none if there's no file.
pub fn load_versions(path: &Path) -> Versions {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| ron::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keeps the versions of vimtracker's setups at `path`.
pub fn save_versions(versions: &Versions, path: &Path) -> std::io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let text = ron::to_string(versions).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

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
    /// On, but another vimtracker on this machine has the Roto, so this one leaves it alone and
    /// looks again every second.
    Taken,
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
            Self::Taken => "On, but another vimtracker has it: turn it off there".to_owned(),
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

/// The file only the vimtracker that has the Roto holds locked, in the cache folder, so a
/// second one on the same machine never answers it too.
pub const LOCK_FILE: &str = "roto.lock";

/// How often a vimtracker whose Roto is on, while another has it, looks again.
const TAKEN_RETRY: Duration = Duration::from_secs(1);

/// Makes the ports the link opens: the system's, or a test's fake Roto.
pub type MakePorts = Box<dyn Fn() -> Box<dyn Ports>>;

/// The least time between two values shown on one knob's screen: 12 a second, as Ableton's
/// script has it (`VALUE_UPDATE_FPS`). The last is always shown.
pub const VALUE_EVERY: Duration = Duration::from_millis(1000 / 12);

/// How long after the last move, with no knob touched, a gesture's undo step closes: long enough
/// for a knob's last move to arrive after it's let go, or for a knob turned without a touch, as
/// a hard twist can be, to be one step.
pub const GESTURE_PAUSE: Duration = Duration::from_millis(300);

/// The least time between two of the meters' readings sent to the Roto: 12 a second, as Ableton's
/// script has it (`AUDIO_METER_FPS`).
pub const METERS_EVERY: Duration = Duration::from_millis(1000 / 12);

/// The Roto's transport buttons, in the order it numbers them (CC 28 to 37), as the vendor's
/// scripts use them: play, stop and record are vimtracker's; session record, loop, punch in and
/// out, automation and the arrows have nothing yet (decision 159's answer 4).
const PLAY_BUTTON: u8 = 0;
const STOP_BUTTON: u8 = 1;
const RECORD_BUTTON: u8 = 2;

/// Edits on the Roto's knobs being made one undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Gesture {
    /// The undo depth before its first edit.
    depth: usize,
    /// When a knob last moved or was let go.
    last: Instant,
}

/// How long after SEL switches a device the Roto choosing it takes the switch back. Holding SEL
/// while its switches are up, and pressing a device to choose it, makes the real Roto (firmware
/// 3.3.0) switch it as the button goes down, and show it as SEL is let go, asking for its knobs
/// and sometimes saying `ROTO SELECT PLUGIN`: 3 to 4 seconds apart when the author tried it.
pub const SELECT_TAKES_BACK: Duration = Duration::from_secs(10);

/// A device SEL just switched in or out, which choosing it straight after takes back.
#[derive(Debug, Clone)]
struct Switched {
    device: u8,
    /// What it was switched to.
    enabled: bool,
    at: Instant,
    /// The song the switch made, which is taken back as if it was never made if nothing has
    /// changed it since.
    made: Arc<Song>,
}

/// A knob's value waiting to be shown on its screen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Value {
    waiting: Option<Label>,
    shown: Option<Instant>,
}

/// The Roto as the app holds it.
pub struct Roto {
    pub setting: RotoSetting,
    /// Where the setting is saved when it changes, or nowhere, as in tests.
    pub setting_path: Option<PathBuf>,
    pub connection: Connection,
    pub mode: RotoMode,
    /// What PLUGIN mode shows.
    pub view: PluginView,
    /// The track list FUNC chooses from.
    pub tracks: TrackList,
    /// What MIX mode shows.
    pub mix: MixView,
    /// The track whose devices PLUGIN mode shows.
    pub track: usize,
    /// How long a gesture's undo step stays open after the last move with no knob touched.
    pub gesture_pause: Duration,
    /// How the link behaves, such as how often it looks for the ports.
    pub options: Options,
    ports: MakePorts,
    link: Option<Link>,
    /// Whether the Roto has been given a track yet: the cursor's when it first connects, and
    /// after that its own choice, kept while it's unplugged.
    has_track: bool,
    /// Which knobs of the page shown are being touched.
    touched: [bool; CONTROLS],
    gesture: Option<Gesture>,
    values: [Value; CONTROLS],
    /// The song as the Roto was last sent it, to tell when it changed.
    sent_song: Option<Arc<Song>>,
    /// The mutes and solos as MIX mode was last sent them, which aren't the song's.
    sent_levels: Vec<TrackLive>,
    /// The faders the steps drive as they were last sent, and when that was.
    sent_faders: Vec<[Option<u16>; FADER_COUNT]>,
    faders_sent: Option<Instant>,
    /// When the meters were last sent.
    meters_sent: Option<Instant>,
    /// Whether the Roto's buttons are its transport's, from its asking for the transport's state
    /// until it says which mode it's in, and the play and record lights last sent then.
    transport_buttons: bool,
    transport_lights: Option<(bool, bool)>,
    switched: Option<Switched>,
    /// Opens the serial port that writes vimtracker's stored setups, or nothing, as in a test
    /// that doesn't give one: then every device is shown live.
    pub serial: Option<OpenSerial>,
    /// Where the stored setups' versions are kept between runs, or nowhere.
    pub versions_path: Option<PathBuf>,
    /// Where the file is that says which vimtracker has the Roto, or nowhere, as in most tests.
    pub lock_path: Option<PathBuf>,
    /// That file, held locked while this one has the Roto; the system lets go of it if the
    /// program ends however it ends.
    lock: Option<std::fs::File>,
    /// When it last looked whether the other vimtracker has let the Roto go.
    taken_at: Option<Instant>,
    /// Whether vimtracker's setups are on the Roto, so the devices with buttons are shown as
    /// them.
    pub stored_on_roto: bool,
    /// The setups being checked and written, on a thread of their own.
    syncing: Option<Receiver<Result<(Synced, Versions), String>>>,
}

impl Default for Roto {
    fn default() -> Self {
        Self {
            setting: RotoSetting::default(),
            setting_path: None,
            connection: Connection::Off,
            mode: RotoMode::Unknown,
            view: PluginView::new(),
            tracks: TrackList::new(),
            mix: MixView::new(),
            track: 0,
            gesture_pause: GESTURE_PAUSE,
            options: Options::default(),
            ports: Box::new(|| Box::new(SystemPorts::new())),
            link: None,
            has_track: false,
            touched: [false; CONTROLS],
            gesture: None,
            values: Default::default(),
            sent_song: None,
            sent_levels: Vec::new(),
            sent_faders: Vec::new(),
            faders_sent: None,
            meters_sent: None,
            transport_buttons: false,
            transport_lights: None,
            switched: None,
            serial: None,
            versions_path: None,
            lock_path: None,
            lock: None,
            taken_at: None,
            stored_on_roto: false,
            syncing: None,
        }
    }
}

/// `vimtracker roto-probe --clear-own`: lists vimtracker's own setups on the Roto, asks, and
/// removes them and nothing else, then forgets their versions so they're written afresh next
/// time. The Roto's other setups, such as Ableton Live's, are listed as kept.
pub fn clear_own_setups(
    out: &mut impl std::io::Write,
    answer: &mut impl std::io::BufRead,
) -> Result<(), Box<dyn std::error::Error>> {
    let ours: Vec<_> = devices::stored_setups()
        .iter()
        .map(store::OwnSetup::hash)
        .collect();
    let mut serial = SerialLink::find()?;
    let own = store::own_stored(&mut serial, ours.iter().copied())?;
    if own.is_empty() {
        writeln!(out, "The Roto stores none of vimtracker's setups.")?;
        return Ok(());
    }
    writeln!(out, "vimtracker's setups on the Roto:")?;
    for setup in &own {
        writeln!(out, "  {}", setup.name.as_str())?;
    }
    write!(
        out,
        "Remove these {}, and nothing else? Type yes to remove them: ",
        own.len()
    )?;
    out.flush()?;
    let mut reply = String::new();
    answer.read_line(&mut reply)?;
    if reply.trim() != "yes" {
        writeln!(out, "Nothing removed.")?;
        return Ok(());
    }
    let removed = store::clear_own(&mut serial, ours)?;
    writeln!(out, "Removed {} of vimtracker's setups.", removed.len())?;
    if let Some(path) =
        vt_model::analysis::program_cache_folder().map(|folder| folder.join(VERSIONS_FILE))
    {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// The system's serial port to the Roto, found as `roto-probe` finds it.
pub fn system_serial() -> OpenSerial {
    Arc::new(|| SerialLink::find().map(|link| Box::new(link) as Box<dyn Serial + Send>))
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
        if !self.take_lock() {
            self.connection = Connection::Taken;
            self.taken_at = Some(Instant::now());
            self.mode = RotoMode::Unknown;
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
    /// Takes the lock that says this vimtracker has the Roto, unless another holds it. A lock
    /// that can't be made at all, such as in a folder that can't be written, doesn't stop it.
    fn take_lock(&mut self) -> bool {
        let Some(path) = &self.lock_path else {
            return true;
        };
        if self.lock.is_some() {
            return true;
        }
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(path)
        else {
            return true;
        };
        match file.try_lock() {
            Ok(()) => {
                self.lock = Some(file);
                true
            }
            Err(std::fs::TryLockError::WouldBlock) => false,
            Err(std::fs::TryLockError::Error(_)) => true,
        }
    }

    fn stop(&mut self) {
        // Let go, so another vimtracker can have it.
        self.lock = None;
        self.taken_at = None;
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

    /// Sends CCs, such as lights and meters, if there are any.
    fn send_ccs(&self, ccs: Vec<[u8; 3]>) {
        if let Some(link) = &self.link
            && !ccs.is_empty()
        {
            link.send_ccs(ccs);
        }
    }

    /// Moves knobs of the page shown, each by its place on the page.
    fn move_knobs(&self, positions: &[(u8, u16)]) {
        if let Some(link) = &self.link
            && !positions.is_empty()
        {
            let ccs = positions
                .iter()
                .flat_map(|(knob, position)| cc::knob_position(*knob, *position))
                .collect();
            link.send_ccs(ccs);
        }
    }

    /// Puts a knob's value up to be shown on its screen, as soon as it may be.
    fn show_value(&mut self, knob: u8, text: &str) {
        if let Some(value) = self.values.get_mut(usize::from(knob)) {
            value.waiting = Some(Label::new(text));
        }
    }

    /// Shows the values waiting whose knobs haven't shown one too lately.
    fn send_values(&mut self, now: Instant) {
        let mut messages = Vec::new();
        for (knob, value) in self.values.iter_mut().enumerate() {
            let due = value
                .shown
                .is_none_or(|shown| now.duration_since(shown) >= VALUE_EVERY);
            if due && let Some(text) = value.waiting.take() {
                value.shown = Some(now);
                messages.push(ToRoto::ShowControlValue {
                    control: ControlType::Knob,
                    index: knob as u8,
                    value: text,
                });
            }
        }
        self.send(messages);
    }

    /// Lights or puts out the buttons whose settings changed, as a stored device's lights are
    /// the program's to draw.
    fn send_lights(&mut self) {
        let lights = self.view.take_lights();
        if let Some(link) = &self.link
            && !lights.is_empty()
        {
            let ccs = lights
                .into_iter()
                .map(|(button, lit)| cc::switch_light(button, lit))
                .collect();
            link.send_ccs(ccs);
        }
    }

    /// Checks vimtracker's stored setups on the Roto, and writes those it hasn't or that have
    /// changed, on a thread of its own, as the serial port is slow: a setup is about 15 requests
    /// of 80 ms each. Until it's done the devices are shown as before.
    fn start_sync(&mut self) {
        let Some(open) = self.serial.clone() else {
            return;
        };
        if self.syncing.is_some() {
            return;
        }
        let setups = devices::stored_setups();
        let mut versions = self
            .versions_path
            .as_deref()
            .map(load_versions)
            .unwrap_or_default();
        let (tell, hear) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("roto setups".to_owned())
            .spawn(move || {
                // The serial port can be slow to answer just after the Roto connects, so it's
                // tried a few times before the devices are left live.
                let mut opened = open();
                for _ in 1..SERIAL_TRIES {
                    if opened.is_ok() {
                        break;
                    }
                    std::thread::sleep(SERIAL_RETRY);
                    opened = open();
                }
                let result = opened
                    .map_err(|error| error.to_string())
                    .and_then(|mut serial| {
                        store::sync(serial.as_mut(), &setups, &mut versions)
                            .map(|synced| (synced, versions))
                            .map_err(|error| error.to_string())
                    });
                let _ = tell.send(result);
            });
        if spawned.is_ok() {
            self.syncing = Some(hear);
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
            self.status = if self.roto.connection == Connection::Taken {
                "Roto-Control on, but another vimtracker has it: turn it off on that one's audio \
                 page, and this one takes it"
                    .to_owned()
            } else {
                "Roto-Control on: it's vimtracker's while this is on, so set Live's control \
                 surface to None meanwhile"
                    .to_owned()
            };
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

    /// The phrase whose live generators are the Roto's track's last devices: the one level with
    /// the cursor on that track, as the Gen page shows the cursor's own.
    fn roto_phrase(&self, at: usize) -> Option<u32> {
        self.phrase_beside(at).map(|(phrase, _)| phrase)
    }

    /// The devices PLUGIN mode shows, from the song as it is.
    fn roto_devices(&self) -> Vec<LiveDevice> {
        let tracks = &self.song().tracks;
        let at = if self.roto.track < tracks.len() {
            self.roto.track
        } else {
            0
        };
        let Some(track) = tracks.get(at) else {
            return Vec::new();
        };
        // The Output device's faders follow the steps while they drive them.
        let heard = as_heard(track, &self.faders(at));
        devices::live_devices(
            &heard,
            self.roto_phrase(at),
            self.song(),
            self.roto.stored_on_roto,
        )
    }

    /// A track as the Roto shows it, with its faders where the steps have them while they drive
    /// them.
    fn roto_heard(&self, at: usize) -> Track {
        self.track_as_heard(at)
            .map(std::borrow::Cow::into_owned)
            .unwrap_or_default()
    }

    /// Whether the steps have moved a fader since the Roto was last sent them, at most as often
    /// as the meters are sent, so a glide's motor moves smoothly without flooding the Roto.
    fn roto_faders_moved(&self, now: Instant) -> bool {
        self.roto.sent_faders != self.roto_faders_shown()
            && self
                .roto
                .faders_sent
                .is_none_or(|sent| now.duration_since(sent) >= METERS_EVERY)
    }

    /// Note the faders as the Roto has just been sent them.
    fn roto_sent_faders(&mut self, now: Instant) {
        self.roto.sent_faders = self.roto_faders_shown();
        self.roto.faders_sent = Some(now);
    }

    /// Each track's faders where the steps have them, for those they drive: what the Roto's knobs
    /// show that the song doesn't.
    fn roto_faders_shown(&self) -> Vec<[Option<u16>; FADER_COUNT]> {
        let tracks = self.song().tracks.len().min(self.faders.len());
        self.faders[..tracks]
            .iter()
            .map(|faders| {
                std::array::from_fn(|fader| faders.follows_steps(fader).then_some(faders.at[fader]))
            })
            .collect()
    }

    /// Sends the Roto everything PLUGIN mode shows: the track list and the track's devices.
    fn roto_push(&mut self) {
        let tracks = devices::live_tracks(self.song());
        let mut messages = self.roto.tracks.show(tracks, self.roto.track as u16);
        messages.extend(self.roto.view.show(self.roto_devices()));
        self.roto.send(messages);
        self.roto.sent_song = Some(Arc::clone(self.song()));
    }

    /// Takes what the Roto said since the last frame, and answers it: knobs' moves become edits,
    /// one for each knob's last move, and what the song now shows goes back.
    pub(crate) fn tick_roto(&mut self) {
        self.roto_poll_sync();
        // Another vimtracker had the Roto: it's this one's once that one lets it go.
        if self.roto.connection == Connection::Taken
            && self
                .roto
                .taken_at
                .is_none_or(|at| at.elapsed() >= TAKEN_RETRY)
        {
            self.roto.start();
            if self.roto.connection != Connection::Taken {
                self.status =
                    "Roto-Control: the other vimtracker let it go, so it's this one's".to_owned();
            }
        }
        let events = match &self.roto.link {
            Some(link) => link.poll(),
            None => return,
        };
        let now = Instant::now();
        let mut moves: [Option<u16>; CONTROLS] = [None; CONTROLS];
        let mut presses = Vec::new();
        let mut let_go = false;
        for event in events {
            match event {
                Event::Opened => self.roto.connection = Connection::Connecting,
                Event::Lost(why) => {
                    self.roto.connection = Connection::Lost(why);
                    self.roto.touched = [false; CONTROLS];
                }
                Event::Session(Change::Connected) => {
                    self.roto.connection = Connection::Connected { firmware: None };
                    self.roto.mode = RotoMode::Unknown;
                    if !self.roto.has_track {
                        self.roto.track = self.cursor.track;
                        self.roto.has_track = true;
                    }
                    // It doesn't say its mode on connecting, so the lists go now, which show at
                    // once in PLUGIN mode and are sent again when PLUGIN mode is chosen, from
                    // the bank the chosen device is in, as a new session has no bank of its own.
                    self.roto.view.bank_to_chosen();
                    self.roto_push();
                    // MIX mode's meters turn yellow and red where the app's own do, as
                    // Ableton's script sets its thresholds as it connects.
                    self.roto.send(vec![ToRoto::SetVuMeterThresholds {
                        yellow: mix::meter_level(mix::METER_YELLOW_DB),
                        red: mix::meter_level(mix::METER_RED_DB),
                    }]);
                    // And how many sends there are, with their names, for SEL's menu.
                    let mut sends = self.roto.mix.sends(mix::send_names());
                    // A Roto already in MIX mode says so again, and is shown the mixer.
                    sends.push(ToRoto::RefreshMixMode);
                    self.roto.send(sends);
                    self.roto.transport_buttons = false;
                    // Its devices with buttons are vimtracker's stored setups, checked each time
                    // it connects, as another program may have changed its store meanwhile.
                    self.roto.start_sync();
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
                Event::Control(ControlEvent::Knob { index, value }) => {
                    if let Some(slot) = moves.get_mut(usize::from(index)) {
                        *slot = Some(value);
                    }
                }
                Event::Control(ControlEvent::Touch { index, touched }) => {
                    if let Some(held) = self.roto.touched.get_mut(usize::from(index)) {
                        *held = touched;
                    }
                    if touched {
                        // Touching a knob shows what it's set to, as turning it does.
                        if let Some((control, at)) = self.roto_knob(index) {
                            let text =
                                edits::value_text(control, &self.roto_heard(at), self.song());
                            self.roto.show_value(index, &text);
                        }
                    } else {
                        let_go = true;
                        if let Some(gesture) = &mut self.roto.gesture {
                            gesture.last = now;
                        }
                    }
                }
                // A stored device's button, or a mute or solo in MIX mode, a push button:
                // pressed, it says 127, and let go 0.
                Event::Control(ControlEvent::Switch { index, value }) => {
                    if value >= 0x40 {
                        presses.push(index);
                    }
                }
                Event::Control(ControlEvent::Transport { index, value }) => {
                    if value >= 0x40 {
                        self.roto_transport(index);
                    }
                }
                Event::Control(ControlEvent::Default { index }) => self.roto_default(index),
            }
        }
        let mixing = self.roto.mode == RotoMode::Mix;
        for button in &presses {
            if mixing {
                self.roto_mix_press(*button);
            } else {
                self.roto_press(*button);
            }
        }
        let turned = self.roto_turn(moves, now);
        self.roto_close_gesture(now);
        let faders_moved = self.roto_faders_moved(now);
        self.roto_send_back(turned, let_go || faders_moved);
        if faders_moved {
            self.roto_sent_faders(now);
        }
        self.roto.send_values(now);
        // A button pressed is lit as its setting now is, whatever the Roto drew as it went down.
        if mixing {
            let lights = presses
                .into_iter()
                .filter_map(|button| self.roto.mix.relight(button))
                .collect();
            self.roto.send_ccs(lights);
            self.roto_meters(now);
        } else {
            for button in presses {
                self.roto.view.relight(button);
            }
            self.roto.send_lights();
        }
        self.roto_transport_lights();
    }

    /// A button of a stored device pressed: its off/on setting switched, as its two-step knob
    /// on a live device does, on the Roto's track whatever the cursor is on. Its light is sent back
    /// from the song, so it's lit only once the song has it on. One press is one undo step.
    fn roto_press(&mut self, button: u8) {
        let Some(on) = self.roto.view.button(button).map(|switch| switch.is_on()) else {
            return;
        };
        let at = self.roto.track;
        let index = self.roto.view.index_of(button);
        let device = usize::from(self.roto.view.selected_index());
        let phrase = self.roto_phrase(at);
        let control = self.song().tracks.get(at).and_then(|track| {
            devices::button_at(track, phrase, device, index, self.roto.stored_on_roto)
        });
        if let Some(control) = control {
            let position = if on { 0 } else { KNOB_TOP };
            if let Some(action) = edits::turned(control, at, self.song(), position) {
                self.roto_apply(&action);
            }
        }
    }

    /// Takes the stored setups' check when it's done: the devices with buttons are shown as
    /// vimtracker's stored setups from then on, or live if they couldn't be written.
    fn roto_poll_sync(&mut self) {
        let Some(hear) = &self.roto.syncing else {
            return;
        };
        let result = match hear.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("its thread stopped".to_owned()),
        };
        self.roto.syncing = None;
        let was = self.roto.stored_on_roto;
        match result {
            Ok((synced, versions)) => {
                if let Some(path) = self.roto.versions_path.clone()
                    && let Err(error) = save_versions(&versions, &path)
                {
                    self.status = format!("{}: couldn't be saved: {error}", path.display());
                }
                if !synced.written.is_empty() {
                    self.status = format!(
                        "Roto-Control: wrote {} of vimtracker's setups to it, for its devices' \
                         buttons",
                        synced.written.len()
                    );
                }
                self.roto.stored_on_roto = true;
                if (!was || !synced.written.is_empty())
                    && matches!(self.roto.connection, Connection::Connected { .. })
                {
                    self.roto_push();
                }
            }
            Err(why) => {
                self.status = format!(
                    "Roto-Control: buttons need vimtracker's setups on it, which couldn't be \
                     written, so they're knobs for now: {why}"
                );
                self.roto.stored_on_roto = false;
                if was && matches!(self.roto.connection, Connection::Connected { .. }) {
                    self.roto_push();
                }
            }
        }
    }

    /// The setting on a knob of the page shown, and the track it's on: in MIX mode a fader of a
    /// track of the bank, or of track focus's track.
    fn roto_knob(&self, knob: u8) -> Option<(Control, usize)> {
        if self.roto.mode == RotoMode::Mix {
            let (place, fader) = self.roto.mix.knob(knob)?;
            let at = mix::track_of(self.song(), place)?;
            return mix::control_of(fader).map(|control| (control, at));
        }
        let at = self.roto.track;
        let track = self.song().tracks.get(at)?;
        let index = self.roto.view.index_of(knob);
        let device = usize::from(self.roto.view.selected_index());
        devices::control_at(
            track,
            self.roto_phrase(at),
            device,
            index,
            self.roto.stored_on_roto,
        )
        .map(|control| (control, at))
    }

    /// Each knob's last move of the frame as an edit of its own setting, on the Roto's track
    /// whatever the cursor is on, and its value put up to be shown. Says which knobs moved.
    fn roto_turn(&mut self, moves: [Option<u16>; CONTROLS], now: Instant) -> [bool; CONTROLS] {
        let mut turned = [false; CONTROLS];
        for (knob, position) in moves.into_iter().enumerate() {
            let Some(position) = position else {
                continue;
            };
            let Some((control, at)) = self.roto_knob(knob as u8) else {
                continue;
            };
            let depth = self.history.undo_depth();
            let gesture = self
                .roto
                .gesture
                .get_or_insert(Gesture { depth, last: now });
            gesture.last = now;
            // MIX mode's Level knob has a notch of its own, at 0 dB.
            let action = if self.roto.mode == RotoMode::Mix {
                mix::turned(control, at, self.song(), position)
            } else {
                edits::turned(control, at, self.song(), position)
            };
            if let Some(action) = action {
                self.roto_apply(&action);
            }
            turned[knob] = true;
            let text = edits::value_text(control, &self.roto_heard(at), self.song());
            self.roto.show_value(knob as u8, &text);
        }
        turned
    }

    /// An edit from the Roto, which keeps the status line as it was unless it fails.
    fn roto_apply(&mut self, action: &Action) {
        let status = std::mem::take(&mut self.status);
        self.apply(action);
        if self.status.is_empty() {
            self.status = status;
        }
    }

    /// Makes a gesture's edits one undo step, once no knob is touched and the last has settled.
    fn roto_close_gesture(&mut self, now: Instant) {
        let Some(gesture) = self.roto.gesture else {
            return;
        };
        if self.roto.touched.iter().any(|held| *held)
            || now.duration_since(gesture.last) < self.roto.gesture_pause
        {
            return;
        }
        if self.history.undo_depth() > gesture.depth + 1 {
            self.history.squash_to(gesture.depth);
        }
        self.roto.gesture = None;
    }

    /// Sends the Roto what changed since it was last sent the song, after an edit from it, the
    /// keys or an undo: the track list if a track's name or group did, and the devices, with no
    /// knob moved that's touched or that moved itself. A knob let go goes where its setting is now,
    /// if the keys moved it meanwhile.
    fn roto_send_back(&mut self, turned: [bool; CONTROLS], let_go: bool) {
        if !matches!(self.roto.connection, Connection::Connected { .. }) {
            return;
        }
        if self.roto.mode == RotoMode::Mix {
            self.roto_mix_send_back(turned, let_go);
            return;
        }
        let same = self
            .roto
            .sent_song
            .as_ref()
            .is_some_and(|sent| Arc::ptr_eq(sent, self.song()));
        if same && !let_go && !turned.iter().any(|moved| *moved) {
            return;
        }
        let tracks = devices::live_tracks(self.song());
        let mut messages = self.roto.tracks.update(tracks, self.roto.track as u16);
        let devices = self.roto_devices();
        let refresh = self.roto.view.refresh(devices, self.roto.touched, turned);
        messages.extend(refresh.messages);
        self.roto.send(messages);
        self.roto.move_knobs(&refresh.positions);
        // A knob moved by the keys, an undo or a song shows its new value as it goes, as one
        // turned by hand does.
        for (knob, _) in &refresh.positions {
            if let Some((control, at)) = self.roto_knob(*knob) {
                let text = edits::value_text(control, &self.roto_heard(at), self.song());
                self.roto.show_value(*knob, &text);
            }
        }
        self.roto.sent_song = Some(Arc::clone(self.song()));
    }

    fn roto_message(&mut self, message: &FromRoto) {
        // `SET PLUGIN MODE` between a switch and its choice leaves the switch to be taken back;
        // anything else ends it.
        let switched = if matches!(message, FromRoto::SetPluginMode) {
            None
        } else {
            self.roto.switched.take()
        };
        let recent = |switched: &Switched| switched.at.elapsed() < SELECT_TAKES_BACK;
        match message {
            // Holding SEL with its switches up and pressing a device switches it, and then, as
            // SEL is let go, the real Roto shows that device and asks for its knobs, sometimes
            // with `ROTO SELECT PLUGIN` and sometimes without. Asked for knobs with no choice of
            // ours sent, after a switch, it has chosen the device it switched: that's the device
            // whose knobs it's given, and the switch is taken back.
            FromRoto::ControlMapped { .. } if switched.as_ref().is_some_and(recent) => {
                let switched = switched.expect("checked above");
                self.roto_take_back(&switched);
                self.roto.view.choose(switched.device);
                let mut messages = self.roto.view.push();
                messages.extend(self.roto.view.receive(message));
                self.roto.send(messages);
                self.roto.sent_song = Some(Arc::clone(self.song()));
            }
            FromRoto::SetPluginMode => {
                self.roto.mode = RotoMode::Plugin;
                self.roto.transport_buttons = false;
                // As the song has it now; the Roto answers only what changed.
                self.roto_push();
            }
            FromRoto::SetMixAllTracksMode { .. }
            | FromRoto::SetMixTrackMode(_)
            | FromRoto::SetAllTracksMode(_) => {
                self.roto.mode = RotoMode::Mix;
                self.roto.transport_buttons = false;
                self.roto_mix_message(message);
            }
            // In MIX mode the arrows page its banks, and touching a knob chooses its track.
            FromRoto::SetFirstTrack(_) | FromRoto::RotoSelectTrack(_)
                if self.roto.mode == RotoMode::Mix =>
            {
                self.roto_mix_message(message);
            }
            FromRoto::RequestSendTrackNames(_) | FromRoto::ToggleTrackGrouping(_) => {
                self.roto_mix_message(message);
            }
            // The transport button: the eight buttons are the transport's until a mode is
            // chosen, lit as it plays and records.
            FromRoto::RequestTransportStatus => {
                self.roto.transport_buttons = true;
                self.roto.transport_lights = None;
                let status = self.roto_transport_status();
                self.roto.send(vec![ToRoto::TransportStatus(status)]);
            }
            FromRoto::SetFirstTrack(_) | FromRoto::RotoSelectTrack(_) => {
                let (messages, chosen) = self.roto.tracks.receive(message);
                self.roto.send(messages);
                if let Some(track) = chosen {
                    self.roto_choose_track(usize::from(track));
                }
            }
            FromRoto::SetPluginEnabled { index, enabled } => {
                let device = self.roto.view.device_for(*index);
                self.roto_switch_device(usize::from(device), *enabled);
            }
            FromRoto::RotoSelectPlugin(index) => {
                // Choosing a device never switches it too: the switch the Roto sent as its
                // button went down is taken back.
                let device = self.roto.view.device_for(*index);
                if let Some(switched) = switched
                    && switched.device == device
                    && recent(&switched)
                {
                    self.roto_take_back(&switched);
                }
                let messages = self.roto.view.receive(message);
                self.roto.send(messages);
                self.roto.sent_song = Some(Arc::clone(self.song()));
            }
            message => {
                let messages = self.roto.view.receive(message);
                self.roto.send(messages);
            }
        }
    }

    /// Takes back a switch SEL made, leaving no undo step if nothing has changed the song since,
    /// or switching it back if something has.
    fn roto_take_back(&mut self, switched: &Switched) {
        if Arc::ptr_eq(&switched.made, self.song()) && self.history.take_back().is_some() {
            self.publish();
        } else {
            self.roto_switch_device(usize::from(switched.device), !switched.enabled);
            self.roto.switched = None;
        }
        // The devices as they are again, which the choice then sends.
        let _ = self.roto.view.show(self.roto_devices());
    }

    /// Shows a track's devices on the Roto, chosen on it with FUNC, keeping the device and page
    /// where they are.
    fn roto_choose_track(&mut self, track: usize) {
        if track >= self.song().tracks.len() {
            return;
        }
        self.roto.track = track;
        self.roto.gesture = None;
        let messages = self.roto.view.show(self.roto_devices());
        self.roto.send(messages);
        self.roto.sent_song = Some(Arc::clone(self.song()));
    }

    /// SEL on the Roto switching a device in or out: an FX slot's bypass, or a mod slot's switch,
    /// as `b` does, on the Roto's track. A device with no switch, or an empty
    /// slot, stays as it is, and the Roto is shown so again.
    fn roto_switch_device(&mut self, device: usize, enabled: bool) {
        let at = self.roto.track;
        let Some(track) = self.song().tracks.get(at).cloned() else {
            return;
        };
        let action = match devices::device_switch(&track, device) {
            Some(Control::Insert(slot))
                if track.inserts[slot] != vt_model::insert::Insert::Empty
                    && track.bypassed[slot] == enabled =>
            {
                Some(Action::SetBypass {
                    track: at,
                    slot,
                    bypassed: !enabled,
                })
            }
            Some(Control::Mod(slot, _)) if track.mods[slot].bypassed == enabled => {
                let mut modulator = track.mods[slot];
                modulator.bypassed = !enabled;
                Some(Action::SetMod {
                    track: at,
                    slot,
                    modulator,
                })
            }
            _ => None,
        };
        if let Some(action) = action {
            self.roto_apply(&action);
            self.roto.switched = Some(Switched {
                device: device as u8,
                enabled,
                at: Instant::now(),
                made: Arc::clone(self.song()),
            });
        }
        // Its state as it is, which a device that can't be switched, or an empty slot, keeps. The
        // real Roto (firmware 3.3.0) flips the light itself as the button's pressed, and shows
        // it as sent the next time the list comes up.
        let messages = self.roto.view.show(self.roto_devices());
        self.roto.send(messages);
        self.roto.sent_song = Some(Arc::clone(self.song()));
    }

    /// Answers one of MIX mode's messages: what its new view shows, and the track the Roto chose
    /// by touching a knob, which PLUGIN mode then shows the devices of, as Live's selected track
    /// is both modes'.
    fn roto_mix_message(&mut self, message: &FromRoto) {
        let place = mix::place_of(self.song(), self.roto.track);
        self.roto.mix.select(place);
        let lists = mix::strips(self.song(), &self.levels, &self.faders);
        let (refresh, chosen) = self.roto.mix.receive(message, lists, mix::send_names());
        self.roto.send(refresh.messages.clone());
        self.roto.send_ccs(refresh.ccs());
        if let Some(place) = chosen
            && let Some(track) = mix::track_of(self.song(), place)
        {
            self.roto.track = track;
        }
        self.roto.sent_song = Some(Arc::clone(self.song()));
        self.roto.sent_levels = self.levels.clone();
    }

    /// Sends MIX mode what changed since it was last sent the song and the mutes and solos: the
    /// bank's names and colours, the knobs' positions, the buttons' lights and the meters' states,
    /// with no knob moved that's touched or that moved itself.
    fn roto_mix_send_back(&mut self, turned: [bool; CONTROLS], let_go: bool) {
        let same = self
            .roto
            .sent_song
            .as_ref()
            .is_some_and(|sent| Arc::ptr_eq(sent, self.song()))
            && self.roto.sent_levels == self.levels;
        if same && !let_go && !turned.iter().any(|moved| *moved) {
            return;
        }
        let place = mix::place_of(self.song(), self.roto.track);
        self.roto.mix.select(place);
        let lists = mix::strips(self.song(), &self.levels, &self.faders);
        let refresh = self
            .roto
            .mix
            .refresh(lists, mix::send_names(), self.roto.touched, turned);
        self.roto.send(refresh.messages.clone());
        self.roto.send_ccs(refresh.ccs());
        // A knob moved by the keys, an undo or a song shows its new value as it goes, as one
        // turned by hand does.
        for (knob, _) in &refresh.positions {
            if let Some((control, at)) = self.roto_knob(*knob) {
                let text = edits::value_text(control, &self.roto_heard(at), self.song());
                self.roto.show_value(*knob, &text);
            }
        }
        self.roto.sent_song = Some(Arc::clone(self.song()));
        self.roto.sent_levels = self.levels.clone();
    }

    /// A button pressed in MIX mode: its track muted or unmuted, or soloed or not, as the keys
    /// do, on its track whatever the cursor is on. The master can't be either, and arming has no
    /// meaning yet, so those do nothing.
    fn roto_mix_press(&mut self, button: u8) {
        let Some((place, switches)) = self.roto.mix.button(button) else {
            return;
        };
        let Some(track) = mix::track_of(self.song(), place) else {
            return;
        };
        if !self
            .song()
            .tracks
            .get(track)
            .is_some_and(|owner| Control::Mute.exists_on(owner))
        {
            return;
        }
        match switches {
            SwitchMode::Mute => self.switch_mute(track),
            SwitchMode::Solo => self.switch_solo(track),
            SwitchMode::Arm => {}
        }
    }

    /// The Roto asking for a knob's fader back at its default, as Ableton's script answers it: a
    /// new track's level, pan or send, as one edit.
    fn roto_default(&mut self, knob: u8) {
        if self.roto.mode != RotoMode::Mix {
            return;
        }
        let Some((control, at)) = self.roto_knob(knob) else {
            return;
        };
        let Some(position) = mix::default_position(control, &self.song().tracks[at], self.song())
        else {
            return;
        };
        if let Some(action) = mix::turned(control, at, self.song(), position) {
            self.roto_apply(&action);
            let text = edits::value_text(control, &self.roto_heard(at), self.song());
            self.roto.show_value(knob, &text);
        }
    }

    /// Sends the meters of the tracks MIX mode shows, 12 times a second, each as the mixer page
    /// draws its peak, and only those that moved.
    fn roto_meters(&mut self, now: Instant) {
        if !matches!(self.roto.connection, Connection::Connected { .. })
            || self
                .roto
                .meters_sent
                .is_some_and(|sent| now.duration_since(sent) < METERS_EVERY)
        {
            return;
        }
        self.roto.meters_sent = Some(now);
        let levels: Vec<(u8, [u8; 2])> = self
            .roto
            .mix
            .metered()
            .into_iter()
            .filter_map(|(meter, place)| {
                let track = mix::track_of(self.song(), place)?;
                let reading = self.track_meters.get(track)?;
                Some((meter, mix::meter_levels(reading)))
            })
            .collect();
        let ccs = self.roto.mix.meter_ccs(&levels);
        self.roto.send_ccs(ccs);
    }

    /// One of the Roto's transport buttons pressed: PLAY plays from where it stopped, STOP pauses
    /// in place and, pressed again, goes back to the start, as Live's does, and RECORD arms
    /// recording as the keys do. The others have nothing yet.
    fn roto_transport(&mut self, button: u8) {
        match button {
            PLAY_BUTTON if self.transport != Transport::Playing => {
                self.handle.send(Command::PauseOrResume);
            }
            STOP_BUTTON if self.transport == Transport::Playing => {
                self.handle.send(Command::PauseOrResume);
            }
            STOP_BUTTON => {
                self.handle.send(Command::Return);
            }
            RECORD_BUTTON => self.toggle_recording(),
            _ => {}
        }
    }

    /// The transport's state for its buttons' lights.
    fn roto_transport_status(&self) -> TransportStatus {
        TransportStatus {
            playing: self.transport == Transport::Playing,
            recording: self.recording(),
            ..TransportStatus::default()
        }
    }

    /// While the Roto's buttons are its transport's, lights PLAY while playing and RECORD while
    /// recording, sending only a change.
    fn roto_transport_lights(&mut self) {
        if !self.roto.transport_buttons {
            return;
        }
        let status = self.roto_transport_status();
        let lights = (status.playing, status.recording);
        if self.roto.transport_lights == Some(lights) {
            return;
        }
        self.roto.transport_lights = Some(lights);
        let light = |button: u8, lit: bool| {
            cc::encode(ControlEvent::Transport {
                index: button,
                value: if lit { cc::ON } else { cc::OFF },
            })
        };
        let mut ccs = light(PLAY_BUTTON, status.playing);
        ccs.extend(light(RECORD_BUTTON, status.recording));
        self.roto.send_ccs(ccs);
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
