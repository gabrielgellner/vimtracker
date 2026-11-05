//! `vimtracker probe-keys`: print the key events the terminal really delivers.
//!
//! No keyboard enhancement flags are pushed, because that is how the program runs.
//! Times show how a held key repeats, which the transport key guard depends on.

use std::io::{self, Write};
use std::time::Instant;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::crossterm::terminal;

/// Restores the terminal even if printing fails part way through.
struct RawMode;

impl RawMode {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Print each key event until `Ctrl-C`.
pub fn run() -> io::Result<()> {
    let _raw = RawMode::enter()?;
    let mut out = io::stdout();
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
