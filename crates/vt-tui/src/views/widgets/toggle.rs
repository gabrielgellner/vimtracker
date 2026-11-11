//! A toggle: the switch glyph for anything that's on or off (decision 101). In a device's title it
//! says whether the device is in, in a setting's place whether a two-state setting is on, and in a
//! window's hint whether its key's switch is, so one idea has one look everywhere. It's green when
//! on and faint when off; the words `On` and `Off` are left to the status line.
//!
//! A toggle is two cells wide: its glyph, then a plain space, which kitty and Ghostty draw the
//! glyph over, so the switch is as wide as it's drawn to be rather than squeezed into one cell as
//! a tiny pill (decision 156). Both cells are always written, on or off, so nothing after a
//! toggle moves when it flips.

use ratatui::style::Style;
use ratatui::text::Span;

use crate::glyphs;
use crate::theme;

/// How many cells a toggle takes: its glyph and the space it's drawn over.
pub const WIDTH: usize = 2;

/// The glyph for on or off.
pub fn glyph(on: bool) -> char {
    if on {
        glyphs::SWITCH_ON
    } else {
        glyphs::SWITCH_OFF
    }
}

/// A toggle's own colour: green when on, faint when off.
pub fn style(on: bool) -> Style {
    if on { theme::active() } else { theme::faint() }
}

/// A toggle as text: its glyph, then the plain space the terminal draws it over, two cells.
pub fn text(on: bool) -> String {
    format!("{} ", glyph(on))
}

/// A toggle in its own colour, its two cells together, so a place that draws one can't leave its
/// second cell to something the glyph would cover or that would keep it small.
pub fn span(on: bool) -> Span<'static> {
    Span::styled(text(on), style(on))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toggle_is_two_cells_on_or_off_and_ends_in_a_plain_space() {
        for on in [false, true] {
            assert_eq!(span(on).width(), WIDTH);
            assert!(
                text(on).ends_with(' '),
                "a plain space, which the glyph is drawn over"
            );
        }
    }
}
