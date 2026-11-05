//! The colours: Catppuccin Frappé. They are exact values rather than the terminal's named colours,
//! since a theme can make two named colours nearly the same.

use ratatui::style::{Color, Modifier, Style};

use crate::input::mode::Mode;

pub const ROSEWATER: Color = Color::Rgb(0xf2, 0xd5, 0xcf);
pub const MAUVE: Color = Color::Rgb(0xca, 0x9e, 0xe6);
pub const RED: Color = Color::Rgb(0xe7, 0x82, 0x84);
pub const PEACH: Color = Color::Rgb(0xef, 0x9f, 0x76);
pub const YELLOW: Color = Color::Rgb(0xe5, 0xc8, 0x90);
pub const GREEN: Color = Color::Rgb(0xa6, 0xd1, 0x89);
pub const TEAL: Color = Color::Rgb(0x81, 0xc8, 0xbe);
pub const BLUE: Color = Color::Rgb(0x8c, 0xaa, 0xee);
pub const TEXT: Color = Color::Rgb(0xc6, 0xd0, 0xf5);
pub const OVERLAY2: Color = Color::Rgb(0x94, 0x9c, 0xbb);
pub const SURFACE1: Color = Color::Rgb(0x51, 0x57, 0x6d);
pub const BASE: Color = Color::Rgb(0x30, 0x34, 0x46);
pub const MANTLE: Color = Color::Rgb(0x29, 0x2c, 0x3c);

/// The whole screen.
pub fn screen() -> Style {
    Style::new().fg(TEXT).bg(BASE)
}

/// A window drawn over the screen.
pub fn window() -> Style {
    Style::new().fg(TEXT).bg(MANTLE)
}

/// The cursor, which turns the Insert colour while typing writes to the song.
pub fn cursor(mode: Mode) -> Style {
    let color = if mode == Mode::Insert {
        GREEN
    } else {
        ROSEWATER
    };
    Style::new().fg(BASE).bg(color)
}

/// A selection, and the picker's highlighted row. The text stays readable on it.
pub fn selected() -> Style {
    Style::new()
        .fg(TEXT)
        .bg(SURFACE1)
        .add_modifier(Modifier::BOLD)
}

/// Text that matters less than what is around it.
pub fn dim() -> Style {
    Style::new().fg(OVERLAY2)
}

/// The playhead, and whatever is current in a list.
pub fn active() -> Style {
    Style::new().fg(GREEN)
}
