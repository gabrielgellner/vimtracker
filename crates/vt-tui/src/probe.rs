//! `vimtracker probe-keys`: print the key events the terminal really delivers.
//!
//! No keyboard enhancement flags are pushed, as the program runs without them, except with
//! `--kitty`, which asks for what the track page's keyboard asks for, so presses, repeats and
//! releases show. Times show how a held key repeats, which the transport key guard depends on.

use std::io::{self, Write};
use std::time::Instant;

use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyModifiers, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::{execute, terminal};

use crate::input::keys::RELEASE_REPORTING;

/// Restores the terminal even if printing fails part way through.
struct RawMode {
    kitty: bool,
}

impl RawMode {
    fn enter(kitty: bool) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        if kitty {
            execute!(
                io::stdout(),
                PushKeyboardEnhancementFlags(RELEASE_REPORTING)
            )?;
        }
        Ok(Self { kitty })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if self.kitty {
            let _ = execute!(io::stdout(), PopKeyboardEnhancementFlags);
        }
        let _ = terminal::disable_raw_mode();
    }
}

/// Print each key event until `Ctrl-C`.
pub fn run(kitty: bool) -> io::Result<()> {
    let _raw = RawMode::enter(kitty)?;
    let mut out = io::stdout();
    if kitty {
        let supported = terminal::supports_keyboard_enhancement().unwrap_or(false);
        write!(
            out,
            "The kitty keyboard protocol is {}.\r\n",
            if supported {
                "supported"
            } else {
                "not supported here"
            }
        )?;
    }
    write!(out, "Press keys to see their events. Ctrl-C exits.\r\n")?;
    let mut last = Instant::now();
    loop {
        if let Event::Key(key) = event::read()? {
            let now = Instant::now();
            let gap = now.duration_since(last).as_millis();
            last = now;
            write!(
                out,
                "+{gap:>5} ms  {:?}  modifiers={:?}  kind={:?}\r\n",
                key.code, key.modifiers, key.kind
            )?;
            out.flush()?;
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                return Ok(());
            }
        }
    }
}
