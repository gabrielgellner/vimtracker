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

pub mod curve;
mod encoding;
pub mod generators;
mod panels;
pub mod plot;
mod protocol;
mod sender;
pub(crate) mod slots;
mod switches;

pub use encoding::{Picture, scope_png, spans_png, waveform_spans};
pub use panels::{PanelIds, PanelKey};
pub(crate) use panels::{PanelWant, Shape};
pub use protocol::{
    DEFAULT_CELL, Image, MOST_CELLS, Placement, id_color, place_at, placeholder_lines, store,
    supports_graphics, transmit, unplace, unplace_all,
};
pub(crate) use protocol::{PLACEHOLDER, POSITIONS};
pub use sender::{
    Graphics, Placements, SampleLines, dot_pixels, dot_placement, mark_png, placed_dot,
};

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
