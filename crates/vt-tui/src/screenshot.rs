//! Screenshots of the interface for the manual: a drawn screen written as an SVG, with each cell's
//! text and colours, so it's exact and stays sharp at any size. The images the terminal is sent,
//! like the filter curve, are put in where their placeholders are, as the terminal shows them.

use std::collections::HashMap;
use std::fmt::Write;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

use crate::kitty::{PLACEHOLDER, POSITIONS};
use crate::theme;

/// A cell's size in the picture, in pixels, and the size of its text.
const CELL_WIDTH: f32 = 9.0;
const CELL_HEIGHT: f32 = 19.0;
const FONT_SIZE: f32 = 15.0;
/// Where the text sits in a cell, from its top.
const BASELINE: f32 = 14.0;

/// A colour as SVG writes it, or the screen's own for the terminal's default.
fn colour(color: Color, default: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Reset => colour(default, theme::TEXT),
        // The interface only uses exact colours, so these are only a fallback.
        _ => colour(default, theme::TEXT),
    }
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// How one cell looks: its text colour, background, and whether it's bold.
#[derive(PartialEq)]
struct Look {
    fg: String,
    bg: String,
    bold: bool,
}

fn look(buffer: &Buffer, x: u16, y: u16) -> Look {
    let cell = &buffer[(x, y)];
    let (mut fg, mut bg) = (colour(cell.fg, theme::TEXT), colour(cell.bg, theme::BASE));
    if cell.modifier.contains(Modifier::REVERSED) {
        std::mem::swap(&mut fg, &mut bg);
    }
    Look {
        fg,
        bg,
        bold: cell.modifier.contains(Modifier::BOLD),
    }
}

/// The PNGs in what was sent to the terminal, by their ids.
pub fn sent_images(sent: &[u8]) -> HashMap<u32, Vec<u8>> {
    let text = String::from_utf8_lossy(sent);
    let mut images = HashMap::new();
    let (mut id, mut data) = (None, String::new());
    for command in text.split("\x1b_G").skip(1) {
        let command = command.split("\x1b\\").next().unwrap_or_default();
        let (controls, chunk) = command.split_once(';').unwrap_or((command, ""));
        let value = |key: &str| {
            controls
                .split(',')
                .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
        };
        if value("a") == Some("T") {
            id = value("i").and_then(|id| id.parse::<u32>().ok());
            data.clear();
        } else if value("a").is_some() {
            continue;
        }
        data.push_str(chunk);
        if value("m") == Some("0")
            && let Some(id) = id.take()
            && let Ok(png) = STANDARD.decode(&data)
        {
            images.insert(id, png);
        }
    }
    images
}

/// Whether a cell is part of an image, and if so where its top left is: the image it's part of,
/// read from its colour.
fn image_corner(buffer: &Buffer, x: u16, y: u16) -> Option<u32> {
    let cell = &buffer[(x, y)];
    let mut marks = cell.symbol().chars();
    let corner = marks.next() == Some(PLACEHOLDER)
        && marks.next() == Some(POSITIONS[0])
        && marks.next() == Some(POSITIONS[0]);
    match cell.fg {
        Color::Rgb(red, green, blue) if corner => {
            Some(u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue))
        }
        _ => None,
    }
}

/// How many cells of placeholders run on from this one, across or down.
fn placeholders_from(buffer: &Buffer, x: u16, y: u16, across: bool) -> u16 {
    let fg = buffer[(x, y)].fg;
    let area = buffer.area;
    let mut count = 0;
    loop {
        let (at_x, at_y) = if across {
            (x + count, y)
        } else {
            (x, y + count)
        };
        if at_x >= area.width || at_y >= area.height {
            return count;
        }
        let cell = &buffer[(at_x, at_y)];
        if cell.fg != fg || !cell.symbol().starts_with(PLACEHOLDER) {
            return count;
        }
        count += 1;
    }
}

/// The whole buffer as an SVG, with the images sent to the terminal where their placeholders are.
/// Runs of cells that look the same are drawn together.
pub fn to_svg(buffer: &Buffer, images: &HashMap<u32, Vec<u8>>) -> String {
    let area = buffer.area;
    let (width, height) = (
        f32::from(area.width) * CELL_WIDTH,
        f32::from(area.height) * CELL_HEIGHT,
    );
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" font-family="'JetBrains Mono', 'SF Mono', Menlo, Consolas, 'Symbols Nerd Font Mono', monospace" font-size="{FONT_SIZE}">"#
    );
    let _ = writeln!(
        svg,
        r#"<rect width="100%" height="100%" fill="{}"/>"#,
        colour(theme::BASE, theme::BASE)
    );
    for y in 0..area.height {
        let mut x = 0;
        while x < area.width {
            let here = look(buffer, x, y);
            let start = x;
            let mut text = String::new();
            while x < area.width && look(buffer, x, y) == here {
                let symbol = buffer[(x, y)].symbol();
                // A placeholder is drawn as its image, below.
                text.push_str(if symbol.starts_with(PLACEHOLDER) {
                    " "
                } else {
                    symbol
                });
                x += 1;
            }
            let (left, top) = (f32::from(start) * CELL_WIDTH, f32::from(y) * CELL_HEIGHT);
            let run = f32::from(x - start) * CELL_WIDTH;
            if here.bg != colour(theme::BASE, theme::BASE) {
                let _ = writeln!(
                    svg,
                    r#"<rect x="{left}" y="{top}" width="{run}" height="{CELL_HEIGHT}" fill="{}"/>"#,
                    here.bg
                );
            }
            if !text.trim().is_empty() {
                let weight = if here.bold {
                    r#" font-weight="bold""#
                } else {
                    ""
                };
                let _ = writeln!(
                    svg,
                    r#"<text x="{left}" y="{}" fill="{}"{weight} textLength="{run}" lengthAdjust="spacingAndGlyphs" xml:space="preserve">{}</text>"#,
                    top + BASELINE,
                    here.fg,
                    escaped(&text)
                );
            }
        }
    }
    for y in 0..area.height {
        for x in 0..area.width {
            let Some(png) = image_corner(buffer, x, y).and_then(|id| images.get(&id)) else {
                continue;
            };
            let (columns, rows) = (
                placeholders_from(buffer, x, y, true),
                placeholders_from(buffer, x, y, false),
            );
            // The terminal stretches an image over its cells, as this does.
            let _ = writeln!(
                svg,
                r#"<image x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="none" href="data:image/png;base64,{}"/>"#,
                f32::from(x) * CELL_WIDTH,
                f32::from(y) * CELL_HEIGHT,
                f32::from(columns) * CELL_WIDTH,
                f32::from(rows) * CELL_HEIGHT,
                STANDARD.encode(png)
            );
        }
    }
    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use crate::kitty::{Image, id_color, placeholder_lines, transmit};

    #[test]
    fn a_screen_becomes_an_svg_with_its_text_and_colours() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));
        buffer.set_string(0, 0, "C-4 <&>", Style::new().fg(theme::YELLOW));
        buffer.set_string(
            0,
            1,
            "HLD3",
            Style::new().fg(theme::TEXT).bg(theme::ROSEWATER),
        );
        let svg = to_svg(&buffer, &HashMap::new());
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(svg.contains(">C-4 &lt;&amp;&gt;</text>"), "{svg}");
        assert!(svg.contains("#e5c890"), "the yellow text");
        assert!(
            svg.contains(r##"fill="#f2d5cf""##),
            "the cursor's background"
        );
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn an_image_sent_to_the_terminal_is_put_in_over_its_placeholders() {
        let png = vec![7; 5000];
        let mut sent = Vec::new();
        let image = Image {
            id: 0x01_02_03,
            png: &png,
            columns: 3,
            rows: 2,
        };
        transmit(&mut sent, &image).unwrap();
        sent.extend(b"\x1b_Ga=d,d=I,i=9,q=2;\x1b\\");
        let images = sent_images(&sent);
        assert_eq!(images.len(), 1);
        assert_eq!(images[&0x01_02_03], png);

        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 3));
        let style = Style::new().fg(id_color(0x01_02_03));
        for (row, line) in placeholder_lines(3, 2).iter().enumerate() {
            buffer.set_string(2, row as u16 + 1, line, style);
        }
        let svg = to_svg(&buffer, &images);
        assert_eq!(svg.matches("<image ").count(), 1, "{svg}");
        assert!(
            svg.contains(&format!(
                r#"<image x="18" y="19" width="27" height="38" preserveAspectRatio="none" href="data:image/png;base64,{}"/>"#,
                STANDARD.encode(&png)
            )),
            "{svg}"
        );
        assert!(!svg.contains(PLACEHOLDER), "the placeholders aren't text");
    }
}
