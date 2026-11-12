//! The on/off switches as small pictures (decision 256): a rounded pill two characters wide and one
//! high, at the terminal's own pixels, with a round knob right when on and left when off, so a
//! switch is as big whatever the font draws its glyph as.
//!
//! The views still write each switch as its glyph and a plain space (`widgets::toggle`), which
//! tests and screenshots read. Once the screen is drawn, [`find`] finds them there, in the colour
//! each was written in, and hides each glyph by drawing it in its background's colour, so it stays
//! in the cell, and is what the terminal gives as text, while the picture placed over it is all
//! that shows. The picture is see-through around the pill, so the cursor's background shows round
//! it, as round the glyph.

use std::collections::BTreeMap;

use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use super::plot::{Plot, STEPS};
use super::rgb;
use crate::glyphs;
use crate::theme;

/// How a switch looks: on or off, and its colour, which is the colour its glyph was written in:
/// green on and faint off as `toggle::style` has them, dim where it does nothing, or the cursor's.
/// Each look is one picture, sent once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Look {
    pub on: bool,
    pub color: [u8; 3],
}

/// How solid the inside of a switch that's on is, under its knob, out of 1: enough to tell on
/// from off at a glance, as a lit switch is filled and an unlit one hollow, while the knob still
/// stands out of it.
const FILL: f32 = 0.4;

/// Every switch on a drawn screen, by the cell its glyph is in, the first of its two: a switch's
/// glyph followed by a plain space on the same background, as `toggle::span` writes it. Each glyph
/// found is hidden in its cell, drawn in its background's colour, for the picture placed over it.
/// A glyph whose second cell has gone, as under the edge of a window drawn over it, is left as it
/// is.
pub fn find(buffer: &mut Buffer) -> BTreeMap<(u16, u16), Look> {
    let area = buffer.area;
    let mut found = BTreeMap::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right().saturating_sub(1) {
            let cell = &buffer[(x, y)];
            let on = match cell.symbol().chars().next() {
                Some(glyphs::SWITCH_ON) => true,
                Some(glyphs::SWITCH_OFF) => false,
                _ => continue,
            };
            let next = &buffer[(x + 1, y)];
            if cell.symbol().chars().count() != 1
                || next.symbol() != " "
                || next.bg != cell.bg
                || cell.modifier.contains(Modifier::REVERSED)
            {
                continue;
            }
            let color = match cell.fg {
                Color::Reset => theme::TEXT,
                color => color,
            };
            let hidden = match cell.bg {
                Color::Reset => theme::BASE,
                color => color,
            };
            found.insert(
                (x, y),
                Look {
                    on,
                    color: rgb(color),
                },
            );
            buffer[(x, y)].set_fg(hidden);
        }
    }
    found
}

/// A switch's picture for characters of this size, before it's a PNG: two characters across and
/// one down, with the pill in the middle of them, on whole pixels so its edges are crisp. The
/// pill is as wide as the two characters less a tenth of one at each end, and six tenths as tall
/// as it's wide, but no more than six tenths of the character's height, so in a short, wide
/// character it keeps its shape. Its outline, an eighth of its height, and its knob are solid,
/// with a gap between them, and on, its inside is filled part way.
pub fn picture(look: Look, cell: (usize, usize)) -> Plot {
    let (width, height) = (2 * cell.0.max(1), cell.1.max(1));
    let mut plot = Plot::new(width, height);
    let end = (cell.0 as f32 * 0.1).round().max(1.0);
    let tall = ((width as f32 - 2.0 * end) * 0.6)
        .min(height as f32 * 0.6)
        .round();
    let radius = tall / 2.0;
    let middle = ((height as f32 - tall) / 2.0).floor() + radius;
    let (left, right) = (end + radius, width as f32 - end - radius);
    let line = (tall / 8.0).round().max(1.0);
    let knob = radius - line - (line / 2.0).max(1.0);
    let knob_x = if look.on { right } else { left };
    for y in 0..plot.height {
        for x in 0..plot.width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            // How far the pixel's middle is outside the pill's edge and the knob's.
            let along = px.clamp(left, right);
            let pill = (px - along).hypot(py - middle) - radius;
            let round = (px - knob_x).hypot(py - middle) - knob;
            let inside = (0.5 - pill).clamp(0.0, 1.0);
            let outline = inside - (0.5 - pill - line).clamp(0.0, 1.0);
            let knob = (0.5 - round).clamp(0.0, 1.0);
            let fill = if look.on { FILL * inside } else { 0.0 };
            let share = outline.max(knob).max(fill);
            plot.cover[y * plot.width + x] = (share * f32::from(STEPS)).round() as u8;
        }
    }
    plot
}

/// A switch's picture as a PNG, for characters of this size.
pub fn png(look: Look, cell: (usize, usize)) -> Vec<u8> {
    picture(look, cell).png(look.color)
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    use ratatui::text::Span;

    use super::*;
    use crate::input::mode::Mode;
    use crate::views::widgets::toggle;

    #[test]
    fn a_switch_is_found_in_its_colour_and_its_glyph_hidden_in_its_cell() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 12, 2));
        buffer.set_style(buffer.area, theme::screen());
        buffer.set_span(1, 0, &toggle::span(true), 2);
        buffer.set_span(5, 0, &toggle::span(false), 2);
        let cursor = theme::cursor(Mode::Normal);
        buffer.set_span(8, 1, &Span::styled(toggle::text(true), cursor), 2);
        // A glyph without its space, as under a window's edge, stays a glyph.
        buffer.set_string(10, 0, glyphs::SWITCH_OFF.to_string(), Style::new());
        buffer.set_string(11, 0, "│", Style::new());
        let found = find(&mut buffer);
        let look = |on, style: Style| Look {
            on,
            color: rgb(style.fg.unwrap_or(theme::TEXT)),
        };
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            [
                ((1, 0), look(true, theme::active())),
                ((5, 0), look(false, theme::faint())),
                ((8, 1), look(true, cursor)),
            ]
        );
        for (x, y) in [(1, 0), (5, 0), (8, 1)] {
            let cell = &buffer[(x, y)];
            assert!(
                [glyphs::SWITCH_ON, glyphs::SWITCH_OFF]
                    .map(String::from)
                    .contains(&cell.symbol().to_owned()),
                "the glyph stays in its cell"
            );
            assert_eq!(cell.fg, cell.bg, "drawn in its background's colour");
            assert_eq!(buffer[(x + 1, y)].symbol(), " ");
        }
        assert_ne!(buffer[(10, 0)].fg, buffer[(10, 0)].bg);
    }

    /// How much of a picture's left and right halves are covered.
    fn halves(plot: &Plot) -> (u32, u32) {
        let half = plot.width / 2;
        plot.cover
            .chunks(plot.width)
            .fold((0, 0), |(left, right), row| {
                let sum = |cover: &[u8]| cover.iter().map(|&c| u32::from(c)).sum::<u32>();
                (left + sum(&row[..half]), right + sum(&row[half..]))
            })
    }

    #[test]
    fn a_switch_is_a_pill_two_characters_wide_with_its_knob_right_when_on() {
        for cell in [(15, 39), (8, 17), (20, 24)] {
            let on = picture(
                Look {
                    on: true,
                    color: [0; 3],
                },
                cell,
            );
            let off = picture(
                Look {
                    on: false,
                    color: [0; 3],
                },
                cell,
            );
            for plot in [&on, &off] {
                assert_eq!((plot.width, plot.height), (2 * cell.0, cell.1));
                // See-through round the pill, so the cell's background shows, and clear at its
                // ends, so a switch never touches what's beside it.
                for row in plot.cover.chunks(plot.width) {
                    assert_eq!((row[0], row[plot.width - 1]), (0, 0));
                }
                assert!(plot.cover[..plot.width].iter().all(|&c| c == 0));
            }
            let (left, right) = halves(&on);
            assert!(right > left, "on, the knob is right: {left} {right}");
            let (left, right) = halves(&off);
            assert!(left > right, "off, the knob is left: {left} {right}");
            // Across its middle, on is filled part way, and off is hollow between its ends.
            let middle = |plot: &Plot| {
                let row = plot.height / 2 * plot.width;
                plot.cover[row + 2..row + plot.width - 2].to_vec()
            };
            assert!(middle(&on).iter().any(|c| (1..STEPS).contains(c)));
            assert!(middle(&on).iter().all(|&c| c > 0));
            assert!(middle(&off).contains(&0));
        }
    }
}
