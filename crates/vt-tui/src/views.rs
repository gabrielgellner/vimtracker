//! What is drawn on screen.

use ratatui::widgets::{Block, BorderType, Borders};

pub mod audio;
pub mod chain;
pub mod curve;
pub mod guide;
pub mod help;
pub mod meter;
pub mod mixer;
pub mod phrase;
pub mod picker;
pub mod project;
pub mod sample;
pub mod shape;
pub mod song;
pub mod status;
pub mod system;
pub mod track;

/// A window's border, all round, with rounded corners: `╭`, `╮`, `╰` and `╯`, U+256D to U+2570,
/// which kitty and Ghostty draw themselves, so they join the lines cleanly. Every window has one:
/// the picker and the explorer, the command line's list of matches, the guides, the keys, the
/// shape panel, and the box round the track page while its keyboard plays.
pub fn window_border() -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}
