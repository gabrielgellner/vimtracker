//! The handshake with the Roto, with no I/O of its own: what to send when the link opens, and
//! what to answer each message with, so the real link and the tests drive the same code.
//!
//! The vendor's scripts all go the same way. The program sends `DAW STARTED`; the Roto answers
//! with `PING DAW`, as it also does every second for a minute after it's plugged in; the program
//! answers with `DAW PING RESPONSE` and the program it is; the Roto says `ROTO-DAW CONNECTED`, and
//! only then may the program send anything else. Bitwig's script then asks for the firmware
//! version, and Ableton's starts sending its tracks; this asks for the firmware and API versions,
//! then waits for the Roto to say which mode it's in.

use crate::sysex::{Daw, FirmwareVersion, FromRoto, ToRoto};

/// The API version this was written against.
pub const API_MAJOR: u8 = 1;
pub const API_MINOR: u8 = 6;

/// Where the handshake is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing sent yet, or the Roto went away.
    Idle,
    /// `DAW STARTED` sent, or a ping answered, and waiting for `ROTO-DAW CONNECTED`.
    Waiting,
    /// Connected: anything may be sent.
    Connected,
}

/// What a message from the Roto changed, for the program to act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The handshake finished.
    Connected,
    FirmwareVersion(FirmwareVersion),
    ApiVersion {
        major: u8,
        minor: u8,
    },
    /// Anything else, once connected: a mode, a page, a track or a plugin chosen on the Roto.
    Message(FromRoto),
}

/// One session with the Roto.
#[derive(Debug, Clone)]
pub struct Session {
    daw: Daw,
    state: State,
    firmware: Option<FirmwareVersion>,
    api: Option<(u8, u8)>,
}

impl Session {
    /// A session that answers the Roto as the given program.
    pub fn new(daw: Daw) -> Self {
        Self {
            daw,
            state: State::Idle,
            firmware: None,
            api: None,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == State::Connected
    }

    pub fn firmware(&self) -> Option<&FirmwareVersion> {
        self.firmware.as_ref()
    }

    pub fn api_version(&self) -> Option<(u8, u8)> {
        self.api
    }

    /// The link is open: say so, and wait for the Roto's ping.
    pub fn start(&mut self) -> Vec<ToRoto> {
        self.state = State::Waiting;
        self.firmware = None;
        self.api = None;
        vec![ToRoto::DawStarted]
    }

    /// The Roto went away, unplugged or its port closed.
    pub fn lost(&mut self) {
        self.state = State::Idle;
    }

    /// What to send back for a message from the Roto, and what it changed.
    pub fn receive(&mut self, message: FromRoto) -> (Vec<ToRoto>, Option<Change>) {
        match message {
            FromRoto::PingDaw => {
                // A ping while connected means the Roto restarted its search, as after another
                // program's `DAW STARTED`: answer, and wait to be connected again.
                self.state = State::Waiting;
                (vec![ToRoto::PingResponse(self.daw)], None)
            }
            FromRoto::Connected => {
                self.state = State::Connected;
                (
                    vec![ToRoto::RequestFirmwareVersion, ToRoto::RequestApiVersion],
                    Some(Change::Connected),
                )
            }
            FromRoto::FirmwareVersion(version) => {
                self.firmware = Some(version.clone());
                (Vec::new(), Some(Change::FirmwareVersion(version)))
            }
            FromRoto::ApiVersion { major, minor } => {
                self.api = Some((major, minor));
                (Vec::new(), Some(Change::ApiVersion { major, minor }))
            }
            message if self.is_connected() => (Vec::new(), Some(Change::Message(message))),
            // Before the handshake is done nothing else is acted on.
            _ => (Vec::new(), None),
        }
    }

    /// Whether a message may be sent now: before the Roto says it's connected, only the
    /// handshake's own.
    pub fn may_send(&self, message: &ToRoto) -> bool {
        self.is_connected() || matches!(message, ToRoto::DawStarted | ToRoto::PingResponse(_))
    }
}
