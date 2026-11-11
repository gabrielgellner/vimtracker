//! The Roto's ports on this computer: finding them by name, opening its MIDI ports through
//! `midir` and its serial port through `serialport`, and the traits the link opens them by, so a
//! test can put the fake Roto in their place.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::bytes::DecodeError;
use crate::serial::{self, Code, Notice, Reply, Request};

/// What the Roto's MIDI ports are called: `Roto-Control` on macOS, and `Roto-Control MIDI 1` among
/// ALSA's names on Linux, as Bitwig's extension finds them.
pub const MIDI_NAME: &str = "Roto-Control";

/// Who made the Roto, as its USB serial port says, and what it calls itself there.
pub const USB_MANUFACTURER: &str = "Melbourne Instruments";
pub const USB_PRODUCT: &str = "Roto";

/// Why the link couldn't do something.
#[derive(Debug, Error)]
pub enum LinkError {
    #[error("no Roto-Control found")]
    NotFound,
    #[error("MIDI: {0}")]
    Midi(String),
    #[error("serial port: {0}")]
    Serial(String),
    #[error("the Roto didn't answer in time")]
    Timeout,
    #[error("couldn't read the Roto's answer: {0}")]
    Decode(#[from] DecodeError),
    #[error("refused: {0} would change what the Roto stores")]
    NotReadOnly(String),
}

impl From<std::io::Error> for LinkError {
    fn from(error: std::io::Error) -> Self {
        Self::Serial(error.to_string())
    }
}

/// Whether a MIDI port's name is the Roto's.
pub fn is_roto_port(name: &str) -> bool {
    name.to_ascii_lowercase()
        .contains(&MIDI_NAME.to_ascii_lowercase())
}

/// The MIDI ports this computer has, inputs then outputs, by name.
pub fn midi_port_names() -> Result<(Vec<String>, Vec<String>), LinkError> {
    let input =
        midir::MidiInput::new("vimtracker ports").map_err(|e| LinkError::Midi(e.to_string()))?;
    let output =
        midir::MidiOutput::new("vimtracker ports").map_err(|e| LinkError::Midi(e.to_string()))?;
    let inputs = input
        .ports()
        .iter()
        .filter_map(|port| input.port_name(port).ok())
        .collect();
    let outputs = output
        .ports()
        .iter()
        .filter_map(|port| output.port_name(port).ok())
        .collect();
    Ok((inputs, outputs))
}

/// One serial port, and whether it's the Roto's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortName {
    pub path: String,
    /// The USB device's maker and name, if it's on USB.
    pub usb: Option<String>,
    pub is_roto: bool,
}

/// Every serial port, the Roto's marked.
pub fn serial_ports() -> Result<Vec<SerialPortName>, LinkError> {
    let ports = serialport::available_ports().map_err(|e| LinkError::Serial(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|port| {
            let (usb, is_roto) = match &port.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    let manufacturer = info.manufacturer.clone().unwrap_or_default();
                    let product = info.product.clone().unwrap_or_default();
                    let is_roto = manufacturer.contains(USB_MANUFACTURER)
                        || product.to_ascii_lowercase().contains("roto");
                    (
                        Some(format!(
                            "{manufacturer} {product} ({:04x}:{:04x})",
                            info.vid, info.pid
                        )),
                        is_roto,
                    )
                }
                _ => (None, false),
            };
            SerialPortName {
                path: port.port_name,
                usb,
                is_roto,
            }
        })
        .collect())
}

/// Where the link's incoming MIDI goes: each message the port hands over, as it's handed.
pub trait Deliver: Send {
    fn deliver(&self, bytes: &[u8]);
}

/// An open MIDI connection's sending half. Dropping it closes the connection.
pub trait MidiSend: Send {
    fn send(&mut self, message: &[u8]) -> Result<(), LinkError>;
}

/// Finds and opens the Roto's MIDI ports: the system's, or the fake's in a test.
pub trait Ports: Send {
    /// Whether the Roto's ports are there now.
    fn present(&mut self) -> bool;

    /// Opens both, its incoming messages going to `incoming`.
    fn open(&mut self, incoming: Box<dyn Deliver>) -> Result<Box<dyn MidiSend>, LinkError>;
}

/// Ports chosen as the program runs, the system's or a test's fake.
impl<P: Ports + ?Sized> Ports for Box<P> {
    fn present(&mut self) -> bool {
        (**self).present()
    }

    fn open(&mut self, incoming: Box<dyn Deliver>) -> Result<Box<dyn MidiSend>, LinkError> {
        (**self).open(incoming)
    }
}

/// The system's MIDI ports, through `midir`: CoreMIDI on macOS and ALSA on Linux.
pub struct SystemPorts {
    lister: Option<midir::MidiInput>,
}

impl SystemPorts {
    pub fn new() -> Self {
        Self { lister: None }
    }
}

impl Default for SystemPorts {
    fn default() -> Self {
        Self::new()
    }
}

struct SystemSend {
    output: midir::MidiOutputConnection,
    // Kept so its callback keeps running; dropping it closes the input.
    _input: midir::MidiInputConnection<()>,
}

impl MidiSend for SystemSend {
    fn send(&mut self, message: &[u8]) -> Result<(), LinkError> {
        self.output
            .send(message)
            .map_err(|e| LinkError::Midi(e.to_string()))
    }
}

impl Ports for SystemPorts {
    fn present(&mut self) -> bool {
        if self.lister.is_none() {
            self.lister = midir::MidiInput::new("vimtracker roto watch").ok();
        }
        let Some(lister) = &self.lister else {
            return false;
        };
        lister
            .ports()
            .iter()
            .any(|port| lister.port_name(port).is_ok_and(|name| is_roto_port(&name)))
    }

    fn open(&mut self, incoming: Box<dyn Deliver>) -> Result<Box<dyn MidiSend>, LinkError> {
        let midi = |e: &dyn std::fmt::Display| LinkError::Midi(e.to_string());
        let mut input = midir::MidiInput::new("vimtracker roto").map_err(|e| midi(&e))?;
        // SysEx is what the Roto speaks; clock and active sensing it never sends.
        input.ignore(midir::Ignore::TimeAndActiveSense);
        let input_port = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|name| is_roto_port(&name)))
            .ok_or(LinkError::NotFound)?;
        let output = midir::MidiOutput::new("vimtracker roto").map_err(|e| midi(&e))?;
        let output_port = output
            .ports()
            .into_iter()
            .find(|port| output.port_name(port).is_ok_and(|name| is_roto_port(&name)))
            .ok_or(LinkError::NotFound)?;
        let input = input
            .connect(
                &input_port,
                "vimtracker roto in",
                move |_, bytes, _| incoming.deliver(bytes),
                (),
            )
            .map_err(|e| midi(&e))?;
        let output = output
            .connect(&output_port, "vimtracker roto out")
            .map_err(|e| midi(&e))?;
        Ok(Box::new(SystemSend {
            output,
            _input: input,
        }))
    }
}

/// How long to wait for a serial reply.
pub const SERIAL_TIMEOUT: Duration = Duration::from_millis(1000);

/// The Roto's serial port, open, for one request at a time. It's opened only while it's needed,
/// so the vendor's Roto-Setup app can open it the rest of the time.
pub struct SerialLink {
    port: Box<dyn serialport::SerialPort>,
    /// Notices the Roto sent while a reply was awaited.
    pub notices: Vec<Notice>,
}

impl SerialLink {
    /// Opens a port at 115200 baud, 8 data bits, no parity and one stop bit.
    pub fn open(path: &str) -> Result<Self, LinkError> {
        let port = serialport::new(path, serial::BAUD)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(50))
            .open()
            .map_err(|e| LinkError::Serial(e.to_string()))?;
        Ok(Self {
            port,
            notices: Vec::new(),
        })
    }

    /// Opens the Roto's API port. The Roto has two USB serial ports and only one answers the API
    /// (on the author's Mac, the second of the two, `usbmodem1101` rather than `usbmodem1103`),
    /// so each is asked its firmware version. On macOS only each one's `cu.` port is tried.
    pub fn find() -> Result<Self, LinkError> {
        let candidates = serial_ports()?
            .into_iter()
            .filter(|port| port.is_roto && !port.path.contains("/tty."));
        for port in candidates {
            let Ok(mut link) = Self::open(&port.path) else {
                continue;
            };
            if matches!(link.ask(&Request::GetFirmwareVersion), Ok(Ok(_))) {
                return Ok(link);
            }
        }
        Err(LinkError::NotFound)
    }

    /// Sends a request that only asks, and reads its reply. Anything that would change the Roto
    /// is refused here; changes go through [`SerialLink::change`].
    pub fn ask(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        if !request.is_read_only() {
            return Err(LinkError::NotReadOnly(format!("{request:?}")));
        }
        self.exchange(request)
    }

    /// Sends a request that changes what the Roto stores or shows, and reads its reply.
    pub fn change(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        self.exchange(request)
    }

    fn exchange(&mut self, request: &Request) -> Result<Result<Reply, Code>, LinkError> {
        let mut reply = self.exchange_raw(request)?;
        // A notice can come straight after the reply, as when a device is chosen on the Roto
        // while its setups are written (firmware 3.3.0: `PLUGIN SELECTED`, 13 bytes): it's a
        // notice, not part of the reply.
        let wanted = if reply.get(1) == Some(&0) {
            2 + request.reply_len()
        } else {
            2
        };
        if reply.len() > wanted {
            let surplus = reply.split_off(wanted);
            self.keep_notices(&surplus);
        }
        Ok(request.decode_reply(&reply)?)
    }

    /// Keeps the notices in bytes that came after a reply.
    fn keep_notices(&mut self, mut bytes: &[u8]) {
        while let Some(length) = serial::frame_len(bytes).filter(|length| *length <= bytes.len()) {
            if let Ok(notice) = Notice::decode(&bytes[..length]) {
                self.notices.push(notice);
            }
            bytes = &bytes[length..];
        }
    }

    /// A read-only request's reply as the bytes that came, `A5` first, for checking what the
    /// PDF leaves unsaid against the real Roto.
    pub fn ask_raw(&mut self, request: &Request) -> Result<Vec<u8>, LinkError> {
        if !request.is_read_only() {
            return Err(LinkError::NotReadOnly(format!("{request:?}")));
        }
        self.exchange_raw(request)
    }

    fn exchange_raw(&mut self, request: &Request) -> Result<Vec<u8>, LinkError> {
        self.port.write_all(&request.encode())?;
        self.port.flush()?;
        let deadline = Instant::now() + SERIAL_TIMEOUT;
        loop {
            let first = self.read_exact(1, deadline)?[0];
            match first {
                serial::REQUEST => {
                    let mut frame = vec![first];
                    frame.extend(self.read_exact(4, deadline)?);
                    let length = serial::frame_len(&frame).unwrap_or(5) - 5;
                    frame.extend(self.read_exact(length, deadline)?);
                    if let Ok(notice) = Notice::decode(&frame) {
                        self.notices.push(notice);
                    }
                }
                serial::REPLY => {
                    let mut reply = vec![first];
                    reply.extend(self.read_exact(1, deadline)?);
                    if reply[1] == 0 {
                        reply.extend(self.read_exact(request.reply_len(), deadline)?);
                    }
                    // Anything more that's already come is kept, so a reply longer than the PDF
                    // says shows as one rather than as the next reply's start.
                    reply.extend(self.read_waiting());
                    return Ok(reply);
                }
                // A byte out of step is dropped, so the next frame can be found.
                _ => {}
            }
        }
    }

    /// Whatever bytes are waiting, after a short pause for any still on their way.
    fn read_waiting(&mut self) -> Vec<u8> {
        std::thread::sleep(Duration::from_millis(20));
        let waiting = self.port.bytes_to_read().unwrap_or(0) as usize;
        let mut bytes = vec![0; waiting];
        match self.port.read(&mut bytes) {
            Ok(read) => bytes.truncate(read),
            Err(_) => bytes.clear(),
        }
        bytes
    }

    fn read_exact(&mut self, count: usize, deadline: Instant) -> Result<Vec<u8>, LinkError> {
        let mut bytes = vec![0; count];
        let mut filled = 0;
        while filled < count {
            if Instant::now() > deadline {
                return Err(LinkError::Timeout);
            }
            match self.port.read(&mut bytes[filled..]) {
                Ok(read) => filled += read,
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(bytes)
    }
}
