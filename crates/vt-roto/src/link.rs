//! The Roto's own thread: it watches for the Roto's ports, opens them when they appear, does the
//! handshake, reads what the Roto sends into [`Event`]s for the edit thread to take each frame,
//! and sends what the edit thread asks, paced. When the ports go away it says so and watches for
//! them again, so unplugging and plugging back in needs nothing from the user. It never touches
//! the audio thread, and the edit thread never waits on it.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::cc::{ControlDecoder, ControlEvent};
use crate::midi::Splitter;
use crate::session::{Change, Session};
use crate::sysex::{self, Daw, FromRoto, ToRoto};
use crate::transport::{Deliver, MidiSend, Ports};

/// How the link behaves.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Which program to say this is.
    pub daw: Daw,
    /// How often to look for the ports while they're closed, and to check they're still there.
    pub watch_every: Duration,
    /// The least time between two SysEx messages sent, as the Ableton script waits 5 ms after
    /// each, so the Roto keeps up.
    pub sysex_gap: Duration,
    /// How long to wait for `ROTO-DAW CONNECTED` before starting the handshake again.
    pub handshake_timeout: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            daw: Daw::Bitwig,
            watch_every: Duration::from_secs(1),
            sysex_gap: Duration::from_millis(5),
            handshake_timeout: Duration::from_secs(3),
        }
    }
}

/// What the link tells the edit thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The Roto's ports opened, and the handshake started.
    Opened,
    /// The handshake, the firmware's and the API's versions, and what the Roto says once
    /// connected.
    Session(Change),
    /// A knob, a switch, a touch or a transport button.
    Control(ControlEvent),
    /// The ports went away, or couldn't be opened.
    Lost(String),
}

/// What reaches the link's thread: what the Roto sent, or what the edit thread asks.
enum Input {
    Midi(Vec<u8>),
    Send(ToRoto),
    Ccs(Vec<[u8; 3]>),
    Stop,
}

struct Incoming(Sender<Input>);

impl Deliver for Incoming {
    fn deliver(&self, bytes: &[u8]) {
        // The thread may have stopped, in which case there's no one to tell.
        let _ = self.0.send(Input::Midi(bytes.to_vec()));
    }
}

/// The edit thread's end of the link.
pub struct Link {
    input: Sender<Input>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Link {
    /// Starts the link's thread, watching for the Roto's ports.
    pub fn spawn(ports: impl Ports + 'static, options: Options) -> std::io::Result<Self> {
        let (input, inbox) = mpsc::channel();
        let (tell, events) = mpsc::channel();
        let incoming = input.clone();
        let thread = std::thread::Builder::new()
            .name("roto".to_owned())
            .spawn(move || {
                Worker {
                    ports: Box::new(ports),
                    options,
                    inbox,
                    incoming,
                    tell,
                    open: None,
                    session: Session::new(options.daw),
                    splitter: Splitter::new(),
                    controls: ControlDecoder::new(),
                    last_sysex: None,
                    started: None,
                }
                .run();
            })?;
        Ok(Self {
            input,
            events,
            thread: Some(thread),
        })
    }

    /// Sends a SysEx message, once the handshake is done; before that, it's dropped, as the
    /// edit thread sends the whole state again on [`Change::Connected`].
    pub fn send(&self, message: ToRoto) {
        let _ = self.input.send(Input::Send(message));
    }

    /// Sends CCs, such as knob positions, switch lights and meters.
    pub fn send_ccs(&self, ccs: Vec<[u8; 3]>) {
        let _ = self.input.send(Input::Ccs(ccs));
    }

    /// Everything that's happened since the last call, without waiting.
    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// Waits up to a time for the next event, for tests and the probe.
    pub fn wait(&self, timeout: Duration) -> Option<Event> {
        self.events.recv_timeout(timeout).ok()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        let _ = self.input.send(Input::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Worker {
    ports: Box<dyn Ports>,
    options: Options,
    inbox: Receiver<Input>,
    incoming: Sender<Input>,
    tell: Sender<Event>,
    open: Option<Box<dyn MidiSend>>,
    session: Session,
    splitter: Splitter,
    controls: ControlDecoder,
    last_sysex: Option<Instant>,
    /// When the handshake last started, to try again if the Roto never answers.
    started: Option<Instant>,
}

impl Worker {
    fn run(mut self) {
        let mut next_watch = Instant::now();
        loop {
            let now = Instant::now();
            if now >= next_watch {
                self.watch();
                next_watch = now + self.options.watch_every;
            }
            let wait = next_watch.saturating_duration_since(Instant::now());
            match self.inbox.recv_timeout(wait) {
                Ok(Input::Midi(bytes)) => self.receive(&bytes),
                Ok(Input::Send(message)) => {
                    if self.session.may_send(&message) {
                        self.send_sysex(&message);
                    }
                }
                Ok(Input::Ccs(ccs)) => {
                    if self.session.is_connected() {
                        for cc in ccs {
                            self.send_raw(&cc);
                        }
                    }
                }
                Ok(Input::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Opens the ports if they've appeared, closes them if they've gone, and starts the
    /// handshake again if it stalled.
    fn watch(&mut self) {
        let present = self.ports.present();
        match (&self.open, present) {
            (None, true) => self.connect(),
            (Some(_), false) => self.lose("the Roto-Control was unplugged".to_owned()),
            (Some(_), true) if !self.session.is_connected() => {
                let stalled = self
                    .started
                    .is_some_and(|at| at.elapsed() >= self.options.handshake_timeout);
                if stalled {
                    self.start_handshake();
                }
            }
            _ => {}
        }
    }

    fn connect(&mut self) {
        match self.ports.open(Box::new(Incoming(self.incoming.clone()))) {
            Ok(open) => {
                self.open = Some(open);
                self.splitter = Splitter::new();
                self.controls = ControlDecoder::new();
                let _ = self.tell.send(Event::Opened);
                self.start_handshake();
            }
            Err(error) => {
                let _ = self.tell.send(Event::Lost(error.to_string()));
            }
        }
    }

    fn start_handshake(&mut self) {
        self.started = Some(Instant::now());
        for message in self.session.start() {
            self.send_sysex(&message);
        }
    }

    fn lose(&mut self, why: String) {
        self.open = None;
        self.session.lost();
        self.started = None;
        let _ = self.tell.send(Event::Lost(why));
    }

    fn receive(&mut self, bytes: &[u8]) {
        for message in self.splitter.feed(bytes) {
            if sysex::is_roto_sysex(&message) {
                let Ok(message) = FromRoto::decode(&message) else {
                    continue;
                };
                let (replies, change) = self.session.receive(message);
                for reply in replies {
                    self.send_sysex(&reply);
                }
                if let Some(change) = change {
                    let _ = self.tell.send(Event::Session(change));
                }
            } else if let Some(event) = self.controls.feed(&message) {
                let _ = self.tell.send(Event::Control(event));
            }
        }
    }

    fn send_sysex(&mut self, message: &ToRoto) {
        if let Some(last) = self.last_sysex {
            let since = last.elapsed();
            if since < self.options.sysex_gap {
                std::thread::sleep(self.options.sysex_gap - since);
            }
        }
        self.send_raw(&message.encode());
        self.last_sysex = Some(Instant::now());
    }

    fn send_raw(&mut self, bytes: &[u8]) {
        let Some(open) = &mut self.open else {
            return;
        };
        if let Err(error) = open.send(bytes) {
            self.lose(error.to_string());
        }
    }
}
