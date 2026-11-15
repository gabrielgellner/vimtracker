//! Small drawings in pixels, using kitty's graphics protocol, which kitty and Ghostty both have. An
//! image is sent ahead of time, and placeholder characters say where it goes. As they're text, a
//! window can be drawn over them.
//!
//! - [`protocol`]: the placeholders and the commands.
//! - [`slots`]: what an FX slot's drawing shows.
//! - [`sender`]: [`Graphics`], which sends the pictures the pages ask for and keeps track of
//!   which images the terminal has. A page says which it wants as [`PanelWant`]s, and is given
//!   back their ids as [`PanelIds`].

use ratatui::style::Color;

pub(crate) mod curve;
mod encoding;
pub(crate) mod generators;
mod panels;
pub(crate) mod plot;
mod protocol;
mod sender;
pub(crate) mod slots;
mod switches;

pub use encoding::Picture;
pub(crate) use encoding::waveform_spans;
pub use panels::{PanelIds, PanelKey};
pub(crate) use panels::{PanelWant, Shape};
pub use protocol::{DEFAULT_CELL, id_color, placeholder_lines, store};
pub(crate) use protocol::{PLACEHOLDER, POSITIONS};
pub(crate) use protocol::{Placement, supports_graphics};
// The screenshots' tests check what they place against the commands themselves.
#[cfg(test)]
pub(crate) use protocol::{Image, place_at, transmit, unplace};
pub use sender::{Graphics, dot_pixels};
pub(crate) use sender::{Placements, SampleLines, dot_placement, placed_dot};

/// The grid of dots a waveform or the scope is drawn on, for each character.
const CELL_DOTS_ACROSS: usize = 6;
const CELL_DOTS_DOWN: usize = 12;
/// The pixels across and down each dot. Kitty smooths an image it scales, so a dot is a solid block
/// that's already near its final size, which keeps its edges crisp.
const DOT_PIXELS: usize = 3;

/// A theme colour as the bytes of a picture's palette.
pub(crate) fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(red, green, blue) => [red, green, blue],
        _ => [255; 3],
    }
}

#[cfg(test)]
mod tests;
