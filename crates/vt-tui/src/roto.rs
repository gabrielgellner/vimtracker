//! The Roto-Control, as vimtracker drives it (`docs/designs/roto-control.md`): the setting that
//! turns it on, kept for this machine in `roto.ron` beside `audio.ron`; the link to it while it's
//! on; and what its PLUGIN mode shows, a track's devices sent live with nothing stored on the
//! Roto ([`devices`]). Off, as it starts, nothing is opened and the Roto is left to Live.
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
//! knob being touched is never moved under the hand. MIX mode is #412's.

pub mod devices;
pub mod edits;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use vt_model::action::Action;
use vt_model::song::Song;
use vt_roto::Label;
use vt_roto::cc::{self, ControlEvent};
use vt_roto::link::{Event, Link, Options};
use vt_roto::live::{LiveDevice, PluginView, TrackList};
use vt_roto::session::Change;
use vt_roto::sysex::{CONTROLS, ControlType, FromRoto, ToRoto};
use vt_roto::transport::{Ports, SystemPorts};

use crate::track_page::Control;

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

/// The least time between two values shown on one knob's screen: 12 a second, as Ableton's
/// script has it (`VALUE_UPDATE_FPS`). The last is always shown.
pub const VALUE_EVERY: Duration = Duration::from_millis(1000 / 12);

/// How long after the last move, with no knob touched, a gesture's undo step closes: long enough
/// for a knob's last move to arrive after it's let go, or for a knob turned without a touch, as
/// a hard twist can be, to be one step.
pub const GESTURE_PAUSE: Duration = Duration::from_millis(300);

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
    switched: Option<Switched>,
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
            switched: None,
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
    fn roto_devices(&self) -> Vec<LiveDevice> {
        let tracks = &self.song().tracks;
        let Some(track) = tracks.get(self.roto.track).or_else(|| tracks.first()) else {
            return Vec::new();
        };
        devices::live_devices(track, self.song())
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
        let events = match &self.roto.link {
            Some(link) => link.poll(),
            None => return,
        };
        let now = Instant::now();
        let mut moves: [Option<u16>; CONTROLS] = [None; CONTROLS];
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
                Event::Control(_) if self.roto.mode == RotoMode::Mix => {}
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
                                edits::value_text(control, &self.song().tracks[at], self.song());
                            self.roto.show_value(index, &text);
                        }
                    } else {
                        let_go = true;
                        if let Some(gesture) = &mut self.roto.gesture {
                            gesture.last = now;
                        }
                    }
                }
                Event::Control(_) => {}
            }
        }
        let turned = self.roto_turn(moves, now);
        self.roto_close_gesture(now);
        self.roto_send_back(turned, let_go);
        self.roto.send_values(now);
    }

    /// The setting on a knob of the page shown, and the track it's on.
    fn roto_knob(&self, knob: u8) -> Option<(Control, usize)> {
        let at = self.roto.track;
        let track = self.song().tracks.get(at)?;
        let index = self.roto.view.index_of(knob);
        let device = usize::from(self.roto.view.selected_index());
        devices::control_at(track, device, index).map(|control| (control, at))
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
            if let Some(action) = edits::turned(control, at, self.song(), position) {
                self.roto_apply(&action);
            }
            turned[knob] = true;
            let text = edits::value_text(control, &self.song().tracks[at], self.song());
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
        if self.roto.mode == RotoMode::Mix
            || !matches!(self.roto.connection, Connection::Connected { .. })
        {
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
                let text = edits::value_text(control, &self.song().tracks[at], self.song());
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
                // As the song has it now; the Roto answers only what changed.
                self.roto_push();
            }
            FromRoto::SetMixAllTracksMode { .. }
            | FromRoto::SetMixTrackMode(_)
            | FromRoto::SetAllTracksMode(_) => self.roto.mode = RotoMode::Mix,
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

    /// SEL on the Roto switching a device in or out: an FX slot's bypass, or an envelope's or an
    /// LFO's switch, as `b` does, on the Roto's track. A device with no switch, or an empty
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
            Some(Control::Envelope(slot, _)) if track.envelopes[slot].bypassed == enabled => {
                let mut envelope = track.envelopes[slot];
                envelope.bypassed = !enabled;
                Some(Action::SetEnvelope {
                    track: at,
                    slot,
                    envelope,
                })
            }
            Some(Control::Lfo(slot, _)) if track.lfos[slot].bypassed == enabled => {
                let mut lfo = track.lfos[slot];
                lfo.bypassed = !enabled;
                Some(Action::SetLfo {
                    track: at,
                    slot,
                    lfo,
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
