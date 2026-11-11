//! Melbourne Instruments' Roto-Control, for vimtracker: a second, separate way of working beside
//! the keyboard, with its own view of the song (`docs/designs/roto-control.md`).
//!
//! This crate is the protocol and the link, with nothing of vimtracker's in it:
//!
//! *   [`sysex`]: every SysEx command of the API's version 1.6, typed, each way.
//! *   [`cc`]: the knobs, switches, touches, transport buttons and meters as CCs on channel 16.
//! *   [`serial`]: the serial API's version 1.3, which reads and writes the Roto's stored setups.
//! *   [`session`]: the handshake, with no I/O, so the link and the tests drive the same code.
//! *   [`link`]: the Roto's own thread, which finds its ports by name, opens them, does the
//!     handshake, and reconnects when it's plugged back in.
//! *   [`live`]: devices shown live in PLUGIN mode, their knobs' names, feel and colours sent as
//!     the Roto asks for them, with nothing stored on it, what changes sent back as it changes,
//!     and the track list FUNC chooses from.
//! *   [`transport`]: the ports, through `midir` and `serialport`.
//! *   [`fake`]: a Roto in memory that speaks both protocols, for tests.
//! *   [`probe`]: `vimtracker roto-probe`.
//!
//! The command tables, the CC numbers, the colour schemes and the handshake's order are adapted
//! from Melbourne Instruments' API documents and its MIT-licensed Ableton, Bitwig and Logic
//! scripts, whose notice is in this crate's `NOTICE`.

pub mod bytes;
pub mod cc;
pub mod colour;
pub mod fake;
pub mod link;
pub mod live;
pub mod midi;
pub mod probe;
pub mod serial;
pub mod session;
pub mod sysex;
pub mod transport;

pub use bytes::{DecodeError, Label, ParamHash, PluginHash, Rgb};
