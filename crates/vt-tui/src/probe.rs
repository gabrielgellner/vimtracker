//! `vimtracker probe-keys`: print the key events the terminal really delivers.
//!
//! It asks for the kitty keyboard protocol as the program does, so presses, repeats and releases
//! show, and says whether the terminal offers it and the kitty graphics protocol. It runs in any
//! terminal, so it can show why the program won't start in one. Times show how a held key repeats,
//! and each event shows the key the program reads it as.

use std::io::{self, Write};
use std::time::Instant;

use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyModifiers, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use crate::input::keys::{KEYBOARD_FLAGS, Key};

/// Restores the terminal even if printing fails part way through.
struct RawMode;

impl RawMode {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), PushKeyboardEnhancementFlags(KEYBOARD_FLAGS))?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        let _ = terminal::disable_raw_mode();
    }
}

/// Print each key event until `Ctrl-C`.
pub fn run() -> io::Result<()> {
    // Asked first, as it sets the terminal's mode itself and puts it back.
    let graphics = crate::kitty::supports_graphics().unwrap_or(false);
    let _raw = RawMode::enter()?;
    let mut out = io::stdout();
    let supported = terminal::supports_keyboard_enhancement().unwrap_or(false);
    let said = |supported: bool| {
        if supported {
            "supported"
        } else {
            "not supported here, so vimtracker won't start"
        }
    };
    write!(
        out,
        "The kitty keyboard protocol is {}.\r\n",
        said(supported)
    )?;
    write!(
        out,
        "The kitty graphics protocol is {}.\r\n",
        said(graphics)
    )?;
    write!(out, "Press keys to see their events. Ctrl-C exits.\r\n")?;
    let mut last = Instant::now();
    loop {
        if let Event::Key(key) = event::read()? {
            let now = Instant::now();
            let gap = now.duration_since(last).as_millis();
            last = now;
            let read = Key::from_event(key);
            let read = if read.ignored() {
                "ignored".to_owned()
            } else if read.command {
                // Nothing is bound to Cmd yet, so the program drops these.
                format!("{read}, ignored")
            } else {
                read.to_string()
            };
            write!(
                out,
                "+{gap:>5} ms  {:?}  modifiers={:?}  kind={:?}  reads as {read}\r\n",
                key.code, key.modifiers, key.kind
            )?;
            out.flush()?;
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
        }
    }
}
