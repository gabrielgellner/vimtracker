//! A toggle: the switch glyph for anything that's on or off (decision 101). In a device's title it
//! says whether the device is in, in a setting's place whether a two-state setting is on, and in a
//! window's hint whether its key's switch is, so one idea has one look everywhere. It's green when
//! on and faint when off; the words `On` and `Off` are left to the status line.

use ratatui::style::Style;
use ratatui::text::Span;

use crate::glyphs;
use crate::theme;

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

/// A toggle in its own colour, a single cell, for a place that keeps the cell after it for
/// [`glyphs::ICON_SPACE`], as a device's title does.
pub fn span(on: bool) -> Span<'static> {
    Span::styled(glyph(on).to_string(), style(on))
}

/// A toggle as a setting's value: its glyph, then the no-break space that keeps it to its cell,
/// so it's as big as a title's or a hint's.
pub fn text(on: bool) -> String {
    format!("{}{}", glyph(on), glyphs::ICON_SPACE)
}
