//! `vimtracker roto-probe`: lists the MIDI and serial ports, does the handshake with the Roto if
//! it's there, and asks it only what changes nothing: its firmware and API versions, its mode, and
//! the PLUGIN setups it stores. It writes nothing to the Roto's setups and doesn't change its
//! mode, so it's safe to run beside Ableton Live, though the handshake takes the Roto from Live
//! until Live's control surface is chosen again.

use std::io::Write;
use std::time::{Duration, Instant};

use crate::bytes::PluginHash;
use crate::link::{Event, Link, Options};
use crate::serial::{self, Reply, Request};
use crate::session::Change;
use crate::sysex::Daw;
use crate::transport::{self, SerialLink, SystemPorts};

/// How long to wait for the handshake and the versions.
const HANDSHAKE_WAIT: Duration = Duration::from_secs(5);

/// How long to keep listening once connected, for what the Roto says of its mode.
const LISTEN: Duration = Duration::from_millis(1500);

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs the probe, writing what it finds.
pub fn run(out: &mut impl Write, daw: Daw) -> std::io::Result<()> {
    writeln!(out, "MIDI ports:")?;
    let mut roto_midi = false;
    match transport::midi_port_names() {
        Ok((inputs, outputs)) => {
            for (kind, names) in [("in ", inputs), ("out", outputs)] {
                for name in names {
                    let roto = transport::is_roto_port(&name);
                    roto_midi |= roto;
                    let mark = if roto { "  <- Roto-Control" } else { "" };
                    writeln!(out, "  {kind} {name}{mark}")?;
                }
            }
        }
        Err(error) => writeln!(out, "  couldn't list them: {error}")?,
    }

    writeln!(out, "Serial ports:")?;
    let mut roto_serial = Vec::new();
    match transport::serial_ports() {
        Ok(ports) => {
            for port in ports {
                let usb = port.usb.as_deref().unwrap_or("not USB");
                let mark = if port.is_roto {
                    "  <- Roto-Control"
                } else {
                    ""
                };
                writeln!(out, "  {}  {usb}{mark}", port.path)?;
                if port.is_roto {
                    roto_serial.push(port.path);
                }
            }
        }
        Err(error) => writeln!(out, "  couldn't list them: {error}")?,
    }

    if roto_midi {
        handshake(out, daw)?;
    } else {
        writeln!(out, "No Roto-Control MIDI port, so no handshake.")?;
    }
    // On macOS each serial device has a `cu.` port and a `tty.` one; the `cu.` is for calling out.
    for path in roto_serial.iter().filter(|path| !path.contains("/tty.")) {
        ask_serial(out, path)?;
    }
    Ok(())
}

fn handshake(out: &mut impl Write, daw: Daw) -> std::io::Result<()> {
    writeln!(out, "Handshake over MIDI, as {daw:?}:")?;
    let options = Options {
        daw,
        watch_every: Duration::from_millis(200),
        ..Options::default()
    };
    let link = match Link::spawn(SystemPorts::new(), options) {
        Ok(link) => link,
        Err(error) => return writeln!(out, "  couldn't start the link: {error}"),
    };
    let started = Instant::now();
    let mut connected_at = None;
    let mut versions = 0;
    loop {
        let deadline = match connected_at {
            Some(at) => at + LISTEN,
            None => started + HANDSHAKE_WAIT,
        };
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        let Some(event) = link.wait(left) else {
            break;
        };
        let elapsed = started.elapsed().as_millis();
        match event {
            Event::Opened => writeln!(out, "  {elapsed:>5} ms  ports open, DAW STARTED sent")?,
            Event::Session(Change::Connected) => {
                connected_at = Some(Instant::now());
                writeln!(out, "  {elapsed:>5} ms  ROTO-DAW CONNECTED")?;
            }
            Event::Session(Change::FirmwareVersion(version)) => {
                versions += 1;
                writeln!(out, "  {elapsed:>5} ms  firmware {version}")?;
            }
            Event::Session(Change::ApiVersion { major, minor }) => {
                versions += 1;
                writeln!(out, "  {elapsed:>5} ms  SysEx API {major}.{minor}")?;
            }
            Event::Session(Change::Message(message)) => {
                writeln!(out, "  {elapsed:>5} ms  the Roto says {message:?}")?;
            }
            Event::Control(control) => writeln!(out, "  {elapsed:>5} ms  {control:?}")?,
            Event::Lost(why) => writeln!(out, "  {elapsed:>5} ms  lost: {why}")?,
        }
    }
    if connected_at.is_none() {
        writeln!(
            out,
            "  no ROTO-DAW CONNECTED in {} s: is another program answering it?",
            HANDSHAKE_WAIT.as_secs()
        )?;
    } else if versions < 2 {
        writeln!(out, "  connected, but the versions didn't both come")?;
    }
    Ok(())
}

fn ask_serial(out: &mut impl Write, path: &str) -> std::io::Result<()> {
    writeln!(out, "Serial API on {path}, read-only:")?;
    let mut link = match SerialLink::open(path) {
        Ok(link) => link,
        Err(error) => return writeln!(out, "  couldn't open it (busy, or refused): {error}"),
    };
    match link.ask(&Request::GetFirmwareVersion) {
        Ok(Ok(Reply::FirmwareVersion(version))) => writeln!(out, "  firmware {version}")?,
        Ok(other) => writeln!(out, "  GET FW VERSION: {other:?}")?,
        Err(error) => return writeln!(out, "  GET FW VERSION: {error}"),
    }
    match link.ask(&Request::GetMode) {
        Ok(Ok(Reply::Mode { mode, page })) => writeln!(out, "  mode {mode:?}, page {page}")?,
        Ok(other) => writeln!(out, "  GET MODE: {other:?}")?,
        Err(error) => writeln!(out, "  GET MODE: {error}")?,
    }
    match link.ask(&Request::GetCurrentPlugin) {
        Ok(Ok(Reply::CurrentPlugin { plugin, daw })) => writeln!(
            out,
            "  current plugin {} ({:?}, for {daw:?})",
            plugin.name, plugin.kind
        )?,
        Ok(Err(code)) => writeln!(out, "  no current plugin ({code:?})")?,
        Ok(other) => writeln!(out, "  GET CURRENT PLUGIN: {other:?}")?,
        Err(error) => writeln!(out, "  GET CURRENT PLUGIN: {error}")?,
    }

    let mut stored: Vec<(PluginHash, String)> = Vec::new();
    let mut request = Request::GetFirstPlugin;
    while stored.len() <= serial::MAX_PLUGIN_SETUPS {
        match link.ask(&request) {
            Ok(Ok(Reply::Plugin(plugin))) => {
                stored.push((plugin.hash, format!("{} ({:?})", plugin.name, plugin.kind)));
            }
            Ok(_) => break,
            Err(error) => {
                writeln!(out, "  listing the setups: {error}")?;
                break;
            }
        }
        request = Request::GetNextPlugin;
    }
    writeln!(
        out,
        "  {} of {} PLUGIN setups stored:",
        stored.len(),
        serial::MAX_PLUGIN_SETUPS
    )?;
    for (_, name) in &stored {
        writeln!(out, "    {name}")?;
    }

    // One stored knob's reply as it comes, to check the widths the PDF doesn't give.
    for (hash, name) in &stored {
        let request = Request::GetPluginKnobConfig {
            plugin: *hash,
            control: 0,
        };
        match link.ask_raw(&request) {
            Ok(raw) if raw.get(1) == Some(&0) => {
                writeln!(
                    out,
                    "  {name}'s first knob: {} bytes after the code, {} expected",
                    raw.len() - 2,
                    request.reply_len()
                )?;
                writeln!(out, "    {}", hex(&raw[..raw.len().min(60)]))?;
                match request.decode_reply(&raw) {
                    Ok(Ok(Reply::PluginKnob(knob))) => writeln!(
                        out,
                        "    {} index {} min {} max {} {:?}",
                        knob.name, knob.param_index, knob.min, knob.max, knob.haptic
                    )?,
                    other => writeln!(out, "    read as {other:?}")?,
                }
                break;
            }
            Ok(_) => {}
            Err(error) => {
                writeln!(out, "  reading a knob: {error}")?;
                break;
            }
        }
    }
    for notice in &link.notices {
        writeln!(out, "  the Roto says {notice:?}")?;
    }
    Ok(())
}
