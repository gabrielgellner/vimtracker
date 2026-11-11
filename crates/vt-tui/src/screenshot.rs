//! Screenshots of the interface for the manual: a drawn screen written as an SVG, with each cell's
//! text and colours, so it's exact and stays sharp at any size. The images the terminal is sent,
//! like the filter curve, are put in where their placeholders are, as the terminal shows them, and
//! those the program places itself, like the sample page's ruler marks, where they're placed.

use std::collections::{BTreeMap, BTreeSet, HashMap};
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
/// Where an underline is drawn, from a cell's top: a little under the text, as a font puts it.
const UNDERLINE: f32 = BASELINE + 2.0;
/// The fonts the picture asks for, for its text. Its Nerd Font symbols are drawn as paths.
const FONTS: &str = "'JetBrains Mono', 'SF Mono', Menlo, Consolas, monospace";

// Made by a script, with a path a line.
#[rustfmt::skip]
mod symbols;

/// A Nerd Font symbol's outline, as an SVG path in the font's units, if it's one the program
/// draws. A picture shown on a page with `<img>` can't use the page's fonts, and a font carried
/// in the picture can be drawn before it has loaded, so each symbol is drawn as its outline,
/// from the manual's cut-down Symbols Nerd Font Mono (`scripts/symbols-font.sh`).
fn symbol_path(c: char) -> Option<&'static str> {
    let at = symbols::PATHS
        .binary_search_by_key(&c, |&(symbol, _)| symbol)
        .ok()?;
    Some(symbols::PATHS[at].1)
}

/// A cell's symbol, if it's a single Nerd Font symbol drawn as a path.
fn cell_symbol(symbol: &str) -> Option<char> {
    let mut chars = symbol.chars();
    let c = chars.next()?;
    (chars.next().is_none() && symbol_path(c).is_some()).then_some(c)
}

/// Where a symbol's outline is put to fill a cell's width, as kitty draws a Nerd Font Mono
/// symbol, upright and in the middle of the cell's height: an SVG transform from the font's
/// units, y up, to the picture's, from the cell's top left.
fn symbol_transform(left: f32, top: f32) -> String {
    let scale = CELL_WIDTH / symbols::ADVANCE;
    let middle = (symbols::ASCENT + symbols::DESCENT) / 2.0;
    format!(
        "matrix({scale} 0 0 {} {left} {})",
        -scale,
        top + CELL_HEIGHT / 2.0 + middle * scale
    )
}

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

/// How one cell looks: its text colour, background, whether it's bold, and its underline's colour
/// if it has one.
#[derive(PartialEq)]
struct Look {
    fg: String,
    bg: String,
    bold: bool,
    underline: Option<String>,
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
        underline: cell
            .modifier
            .contains(Modifier::UNDERLINED)
            .then(|| colour(cell.underline_color, cell.fg)),
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

/// An image the program places itself rather than through placeholders, as it was last placed: at
/// a cell, this many of the image's pixels into it across and down, with this many of its rows
/// shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub id: u32,
    pub column: u16,
    pub row: u16,
    pub across: usize,
    pub down: usize,
    pub height: usize,
}

/// The images the program places itself, from what was sent to the terminal: each stored PNG by
/// its id, where each placement of them is now, and the character size in pixels they were drawn
/// for, which they're scaled from.
#[derive(Debug, Default)]
pub struct Placements {
    pub images: HashMap<u32, Vec<u8>>,
    pub placed: Vec<Placed>,
    pub cell: (usize, usize),
}

/// The images placed directly in what was sent to the terminal, drawn for characters of this
/// many pixels, as they are once everything sent has been done: placing again under the same
/// placement id moves an image, and taking one off takes it away.
pub fn sent_placements(sent: &[u8], cell: (usize, usize)) -> Placements {
    let text = String::from_utf8_lossy(sent);
    let mut images = HashMap::new();
    let mut placed = BTreeMap::new();
    let (mut id, mut data) = (None, String::new());
    let pieces: Vec<&str> = text.split("\x1b_G").collect();
    for (index, command) in pieces.iter().enumerate().skip(1) {
        let command = command.split("\x1b\\").next().unwrap_or_default();
        let (controls, chunk) = command.split_once(';').unwrap_or((command, ""));
        let value = |key: &str| {
            controls
                .split(',')
                .find_map(|pair| pair.strip_prefix(key)?.strip_prefix('='))
        };
        let number = |key: &str| value(key).and_then(|value| value.parse::<u32>().ok());
        match value("a") {
            Some("t") => {
                id = number("i");
                data.clear();
            }
            Some("p") => {
                // Moved to its cell just before, as `CSI row;column H`, counted from 1.
                let before = pieces[index - 1];
                let jump = before
                    .rfind("\x1b[")
                    .and_then(|at| before[at + 2..].strip_suffix('H')?.split_once(';'));
                if let (Some(image), Some((row, column))) = (number("i"), jump)
                    && let (Ok(row), Ok(column)) = (row.parse::<u16>(), column.parse::<u16>())
                {
                    placed.insert(
                        (image, number("p").unwrap_or(0)),
                        Placed {
                            id: image,
                            column: column.saturating_sub(1),
                            row: row.saturating_sub(1),
                            across: number("X").unwrap_or(0) as usize,
                            down: number("Y").unwrap_or(0) as usize,
                            height: number("h").unwrap_or(0) as usize,
                        },
                    );
                }
                continue;
            }
            Some("d") => {
                if let Some(image) = number("i") {
                    match number("p").filter(|_| value("d") == Some("i")) {
                        Some(placement) => {
                            placed.remove(&(image, placement));
                        }
                        None => placed.retain(|(placed, _), _| *placed != image),
                    }
                    if value("d") == Some("I") {
                        images.remove(&image);
                    }
                }
                continue;
            }
            Some(_) => continue,
            None => {}
        }
        data.push_str(chunk);
        if value("m") == Some("0")
            && let Some(id) = id.take()
            && let Ok(png) = STANDARD.decode(&data)
        {
            images.insert(id, png);
        }
    }
    Placements {
        images,
        placed: placed.into_values().collect(),
        cell,
    }
}

/// A PNG's width and height in pixels, from its header.
fn png_size(png: &[u8]) -> Option<(usize, usize)> {
    let number = |at: usize| {
        let bytes = png.get(at..at + 4)?;
        Some(u32::from_be_bytes(bytes.try_into().ok()?) as usize)
    };
    Some((number(16)?, number(20)?))
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
    to_svg_placed(buffer, images, &Placements::default())
}

/// The whole buffer as [`to_svg`] draws it, with the images the program places itself over it,
/// where they're placed, each scaled from the character size it was drawn for to the picture's.
pub fn to_svg_placed(
    buffer: &Buffer,
    images: &HashMap<u32, Vec<u8>>,
    placements: &Placements,
) -> String {
    let area = buffer.area;
    let (width, height) = (
        f32::from(area.width) * CELL_WIDTH,
        f32::from(area.height) * CELL_HEIGHT,
    );
    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" font-family="{FONTS}" font-size="{FONT_SIZE}">"#
    );
    // Each symbol shown, once, for the cells that show it to use.
    let shown: BTreeSet<char> = buffer
        .content
        .iter()
        .filter_map(|cell| cell_symbol(cell.symbol()))
        .collect();
    if !shown.is_empty() {
        svg.push_str("<defs>");
        for symbol in &shown {
            let path = symbol_path(*symbol).unwrap_or_default();
            let _ = write!(svg, r#"<path id="s{:x}" d="{path}"/>"#, u32::from(*symbol));
        }
        svg.push_str("</defs>\n");
    }
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
            let mut drawn = Vec::new();
            while x < area.width && look(buffer, x, y) == here {
                let symbol = buffer[(x, y)].symbol();
                // A placeholder is drawn as its image, below, and a Nerd Font symbol as its path.
                if let Some(symbol) = cell_symbol(symbol) {
                    drawn.push((x, symbol));
                    text.push(' ');
                } else if symbol.starts_with(PLACEHOLDER) {
                    text.push(' ');
                } else {
                    text.push_str(symbol);
                }
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
            if let Some(line) = &here.underline {
                let _ = writeln!(
                    svg,
                    r#"<rect x="{left}" y="{}" width="{run}" height="1" fill="{line}"/>"#,
                    top + UNDERLINE
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
            for (at, symbol) in drawn {
                let _ = writeln!(
                    svg,
                    r##"<use href="#s{:x}" transform="{}" fill="{}"/>"##,
                    u32::from(symbol),
                    symbol_transform(f32::from(at) * CELL_WIDTH, top),
                    here.fg
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
    let (across, down) = placements.cell;
    if across > 0 && down > 0 {
        let (scale_x, scale_y) = (CELL_WIDTH / across as f32, CELL_HEIGHT / down as f32);
        for placed in &placements.placed {
            let Some(png) = placements.images.get(&placed.id) else {
                continue;
            };
            let Some((width, height)) = png_size(png) else {
                continue;
            };
            // Only its top rows, as many as are shown.
            let shown = if placed.height == 0 {
                height
            } else {
                placed.height.min(height)
            };
            let _ = writeln!(
                svg,
                r#"<svg x="{}" y="{}" width="{}" height="{}" overflow="hidden"><image width="{}" height="{}" preserveAspectRatio="none" href="data:image/png;base64,{}"/></svg>"#,
                f32::from(placed.column) * CELL_WIDTH + placed.across as f32 * scale_x,
                f32::from(placed.row) * CELL_HEIGHT + placed.down as f32 * scale_y,
                width as f32 * scale_x,
                shown as f32 * scale_y,
                width as f32 * scale_x,
                height as f32 * scale_y,
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
        buffer.set_string(0, 0, "C-3 <&>", Style::new().fg(theme::YELLOW));
        buffer.set_string(
            0,
            1,
            "HLD3",
            Style::new().fg(theme::TEXT).bg(theme::ROSEWATER),
        );
        let svg = to_svg(&buffer, &HashMap::new());
        assert!(svg.starts_with("<svg"), "{svg}");
        assert!(svg.contains(">C-3 &lt;&amp;&gt;</text>"), "{svg}");
        assert!(svg.contains("#e5c890"), "the yellow text");
        assert!(
            svg.contains(r##"fill="#f2d5cf""##),
            "the cursor's background"
        );
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn a_nerd_font_symbol_is_drawn_as_its_outline_in_its_cells_colour() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));
        buffer.set_string(0, 0, "C-3", Style::new());
        let plain = to_svg(&buffer, &HashMap::new());
        assert!(!plain.contains("<defs>"), "{plain}");

        let play = crate::glyphs::PLAY;
        buffer.set_string(4, 1, play.to_string(), Style::new().fg(theme::GREEN));
        buffer.set_string(6, 1, play.to_string(), Style::new().fg(theme::GREEN));
        let svg = to_svg(&buffer, &HashMap::new());
        assert!(!svg.contains(play), "the symbol isn't text: {svg}");
        assert_eq!(svg.matches(r#"<path id="sf040a""#).count(), 1, "{svg}");
        let path = symbol_path(play).unwrap();
        assert!(svg.contains(&format!(r#"d="{path}""#)));
        assert_eq!(svg.matches(r##"<use href="#sf040a""##).count(), 2, "{svg}");
        assert!(
            svg.contains(&format!(
                r##"<use href="#sf040a" transform="{}" fill="{}"/>"##,
                symbol_transform(36.0, 19.0),
                colour(theme::GREEN, theme::TEXT)
            )),
            "{svg}"
        );
    }

    /// The paths have every symbol `glyphs.rs` names: run `scripts/symbols-font.sh` after
    /// adding one.
    #[test]
    fn every_symbol_the_program_draws_has_a_path() {
        let glyphs = include_str!("glyphs.rs");
        let mut rest = glyphs;
        while let Some(at) = rest.find("\\u{") {
            rest = &rest[at + 3..];
            let hex = &rest[..rest.find('}').unwrap()];
            let symbol = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
            let private = matches!(symbol, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}');
            assert!(
                !private || symbol_path(symbol).is_some(),
                "U+{:04X} has no path: run scripts/symbols-font.sh",
                u32::from(symbol)
            );
        }
        assert!(symbols::PATHS.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn an_underline_is_drawn_in_its_own_colour_under_its_cells() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 10, 2));
        let ruled = Style::new()
            .add_modifier(Modifier::UNDERLINED)
            .underline_color(theme::SURFACE1);
        buffer.set_string(2, 1, "C-3", ruled);
        let svg = to_svg(&buffer, &HashMap::new());
        assert!(
            svg.contains(r##"<rect x="18" y="35" width="27" height="1" fill="#51576d"/>"##),
            "{svg}"
        );
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

    #[test]
    fn an_image_the_program_places_is_put_in_where_it_was_last_placed() {
        use crate::kitty::{Picture, Placement, place_at, store, unplace};

        let mut png = Picture::new(4, 6, vec![[255; 3]]);
        png.pixels.fill(1);
        let png = png.png();
        let mut sent = Vec::new();
        store(&mut sent, 5, &png).unwrap();
        let at = |column, across| Placement {
            column,
            row: 2,
            across,
            down: 0,
            height: 6,
        };
        place_at(&mut sent, 5, 1, at(1, 0)).unwrap();
        place_at(&mut sent, 5, 2, at(3, 1)).unwrap();
        // Placing again under the same id moves it, and taking one off takes it away.
        place_at(&mut sent, 5, 1, at(4, 2)).unwrap();
        place_at(&mut sent, 5, 3, at(6, 0)).unwrap();
        unplace(&mut sent, 5, 3).unwrap();
        let placements = sent_placements(&sent, (3, 6));
        assert_eq!(placements.images[&5], png);
        let columns: Vec<_> = placements
            .placed
            .iter()
            .map(|placed| (placed.column, placed.across))
            .collect();
        assert_eq!(columns, [(4, 2), (3, 1)]);

        let buffer = Buffer::empty(Rect::new(0, 0, 10, 3));
        let svg = to_svg_placed(&buffer, &HashMap::new(), &placements);
        // Characters 3 pixels by 6 are 9 by 19 in the picture: 4 columns and 2 pixels in is 42.
        assert!(
            svg.contains(r#"<svg x="42" y="38" width="12" height="19" overflow="hidden">"#),
            "{svg}"
        );
        assert_eq!(svg.matches("<image ").count(), 2);
    }
}
