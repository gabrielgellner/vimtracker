//! The colours: Catppuccin Frappé. They are exact values rather than the terminal's named colours,
//! since a theme can make two named colours nearly the same.

use ratatui::style::{Color, Modifier, Style};

use vt_model::song::Role;

use crate::input::mode::Mode;

pub const ROSEWATER: Color = Color::Rgb(0xf2, 0xd5, 0xcf);
pub const PINK: Color = Color::Rgb(0xf4, 0xb8, 0xe4);
pub const MAUVE: Color = Color::Rgb(0xca, 0x9e, 0xe6);
pub const MAROON: Color = Color::Rgb(0xea, 0x99, 0x9c);
pub const RED: Color = Color::Rgb(0xe7, 0x82, 0x84);
pub const PEACH: Color = Color::Rgb(0xef, 0x9f, 0x76);
pub const YELLOW: Color = Color::Rgb(0xe5, 0xc8, 0x90);
pub const GREEN: Color = Color::Rgb(0xa6, 0xd1, 0x89);
pub const TEAL: Color = Color::Rgb(0x81, 0xc8, 0xbe);
pub const SKY: Color = Color::Rgb(0x99, 0xd1, 0xdb);
pub const BLUE: Color = Color::Rgb(0x8c, 0xaa, 0xee);
pub const LAVENDER: Color = Color::Rgb(0xba, 0xbb, 0xf1);
pub const TEXT: Color = Color::Rgb(0xc6, 0xd0, 0xf5);
pub const SUBTEXT1: Color = Color::Rgb(0xb5, 0xbf, 0xe2);
pub const OVERLAY2: Color = Color::Rgb(0x94, 0x9c, 0xbb);
pub const OVERLAY0: Color = Color::Rgb(0x73, 0x79, 0x94);
pub const SURFACE1: Color = Color::Rgb(0x51, 0x57, 0x6d);
pub const SURFACE0: Color = Color::Rgb(0x41, 0x45, 0x59);
pub const BASE: Color = Color::Rgb(0x30, 0x34, 0x46);
pub const MANTLE: Color = Color::Rgb(0x29, 0x2c, 0x3c);
/// Under a step's FX, a shade darker than the screen, so they sit back from the notes.
pub const FX_BAND: Color = MANTLE;

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
    let color = match mode {
        Mode::Insert => GREEN,
        Mode::Replace => RED,
        _ => ROSEWATER,
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

/// A note outside the scale.
pub fn out_of_scale() -> Style {
    Style::new().fg(PEACH)
}

/// A label to press to jump somewhere.
pub fn jump_label() -> Style {
    Style::new().fg(BASE).bg(PEACH).add_modifier(Modifier::BOLD)
}

/// Text that matters less than what is around it.
pub fn dim() -> Style {
    Style::new().fg(OVERLAY2)
}

/// A fader's value while the steps drive it, in the colour of what's playing, as a bar's fill is.
pub fn automated() -> Style {
    Style::new().fg(MAUVE)
}

/// The mark of a fader a hand has taken over from the steps, in the colour of a button lit to
/// give it back, as Live's Re-Enable Automation is lit.
pub fn taken() -> Style {
    Style::new().fg(PEACH)
}

/// Text that helps without asking to be read.
pub fn faint() -> Style {
    Style::new().fg(OVERLAY0)
}

/// Something has gone wrong and needs noticing.
pub fn warning() -> Style {
    Style::new().fg(RED).add_modifier(Modifier::BOLD)
}

/// A meter's colour at a level in dB: green, turning yellow and then orange toward full level, and
/// red above that, where a level is over.
pub fn level(db: f32) -> Color {
    const STOPS: [(f32, Color); 3] = [(-18.0, GREEN), (-6.0, YELLOW), (0.0, PEACH)];
    if db <= STOPS[0].0 {
        return STOPS[0].1;
    }
    for pair in STOPS.windows(2) {
        let ((low_db, low), (high_db, high)) = (pair[0], pair[1]);
        if db <= high_db {
            return mix(low, high, (db - low_db) / (high_db - low_db));
        }
    }
    RED
}

/// A colour pushed back toward the screen, for the part of a bar that isn't solid.
pub fn dimmed(color: Color) -> Color {
    mix(color, BASE, 0.55)
}

/// A value's fine digits: its colour set back toward the FX band, but still brighter than
/// what's faint.
pub fn fine(color: Color) -> Color {
    mix(color, FX_BAND, 0.3)
}

/// A colour some share of the way from one to another.
fn mix(from: Color, to: Color, share: f32) -> Color {
    let (Color::Rgb(r0, g0, b0), Color::Rgb(r1, g1, b1)) = (from, to) else {
        return from;
    };
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * share) as u8;
    Color::Rgb(channel(r0, r1), channel(g0, g1), channel(b0, b1))
}

/// The playhead, and whatever is current in a list.
pub fn active() -> Style {
    Style::new().fg(GREEN)
}

/// Each group's colour, `G1` first, which its strip's name and the mark on each of its tracks
/// share, so a track is seen to be in it. Neighbours are far apart on the wheel, and none is the
/// green of what's active.
const GROUP_COLORS: [Color; 4] = [BLUE, PEACH, TEAL, MAUVE];

/// A group's colour, by its number from 1. Past the fourth they come round again.
pub fn group(number: u8) -> Color {
    GROUP_COLORS[usize::from(number.saturating_sub(1)) % GROUP_COLORS.len()]
}

/// The colour of what a sound track is for, the rule under its name wherever it's named
/// (decision 316): five of the theme's calm shades, each a hue apart from the others and none a
/// group's, the green of what's active, or the red of a warning, so a rule never reads as one of
/// those. A track with no role has none.
pub fn role(role: Role) -> Option<Color> {
    Some(match role {
        Role::Drums => MAROON,
        Role::Bass => YELLOW,
        Role::Synth => SKY,
        Role::Pad => LAVENDER,
        Role::Texture => PINK,
        Role::Auto | Role::None => return None,
    })
}

/// The rule under a bus's name, a return's, the master's, or a group's whose tracks are for
/// different things: the grey of dim text, as a bus plays nothing of its own.
pub const BUS: Color = OVERLAY2;
