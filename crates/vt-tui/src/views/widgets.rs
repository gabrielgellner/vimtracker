//! The pieces pages are put together from, so a new page uses them rather than working out its
//! layout again: a grid of columns, a value's bar, a setting's name and value, a meter's cells, an
//! image's placeholders and the box round one, a window's border, a hint with its switches, how a
//! group and its tracks are marked, what a track is for, a toggle for whatever is on or off, and a rack's devices: a
//! device's frame and title, its settings grid and its graphic panel.

use ratatui::widgets::{Block, BorderType, Borders};

pub mod bar;
pub mod device;
pub mod grid;
pub mod group;
pub mod hint;
pub mod image;
pub mod meter;
pub mod panel;
pub mod role;
pub mod setting;
pub mod table;
pub mod toggle;

/// A window's border, all round, with rounded corners: `╭`, `╮`, `╰` and `╯`, U+256D to U+2570,
/// which kitty and Ghostty draw themselves, so they join the lines cleanly. Every window has one:
/// the picker and the explorer, the command line's list of matches, the guides, the keys, the
/// shape panel, the box round the track page while its keyboard plays, and each device in a rack.
pub fn window_border() -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}
