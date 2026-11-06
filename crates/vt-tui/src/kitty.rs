//! An experiment in drawing with pixels, using kitty's graphics protocol. An image is sent ahead of
//! time, and placeholder characters say where it goes. As they're text, they stay inside a tmux
//! pane, and a window can be drawn over them.

use std::io::{self, Write};
use std::process::Command;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ratatui::style::Color;
use vt_model::constants::TRACK_ENVELOPES;
use vt_model::envelope::Envelope;

use crate::app::App;
use crate::input::mode::View;
use crate::theme;
use crate::track_page::Page;
use crate::views::braille::envelope_columns;
use crate::views::track::{DIAGRAM_HEIGHT, DIAGRAM_WIDTH, envelope_being_edited};

/// The character that stands for a piece of an image.
const PLACEHOLDER: char = '\u{10EEEE}';
/// The marks that give a placeholder's row and column, from kitty's `rowcolumn-diacritics.txt`.
const POSITIONS: [char; 28] = [
    '\u{305}', '\u{30D}', '\u{30E}', '\u{310}', '\u{312}', '\u{33D}', '\u{33E}', '\u{33F}',
    '\u{346}', '\u{34A}', '\u{34B}', '\u{34C}', '\u{350}', '\u{351}', '\u{352}', '\u{357}',
    '\u{35B}', '\u{363}', '\u{364}', '\u{365}', '\u{366}', '\u{367}', '\u{368}', '\u{369}',
    '\u{36A}', '\u{36B}', '\u{36C}', '\u{36D}',
];
/// The most base64 the protocol takes in one command.
const CHUNK: usize = 4096;
/// The grid of dots an envelope is drawn on, for each character.
const CELL_DOTS_ACROSS: usize = 6;
const CELL_DOTS_DOWN: usize = 12;
/// The pixels across and down each dot. Kitty smooths an image it scales, so a dot is a solid block
/// that's already near its final size, which keeps its edges crisp.
const DOT_PIXELS: usize = 3;

/// Lines of placeholders for an image of this size.
pub fn placeholder_lines(width: usize, height: usize) -> Vec<String> {
    (0..height)
        .map(|row| {
            (0..width)
                .flat_map(|column| [PLACEHOLDER, POSITIONS[row], POSITIONS[column]])
                .collect()
        })
        .collect()
}

/// The colour that tells the terminal which image a placeholder belongs to.
pub fn id_color(id: u32) -> Color {
    Color::Rgb((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// The CRC that PNG chunks end with.
fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = u32::MAX;
    for byte in parts.iter().copied().flatten() {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// The checksum that zlib data ends with.
fn adler32(data: &[u8]) -> u32 {
    let (a, b) = data.iter().fold((1_u32, 0_u32), |(a, b), byte| {
        let a = (a + u32::from(*byte)) % 65_521;
        (a, (b + a) % 65_521)
    });
    (b << 16) | a
}

/// A PNG of solid dots in one colour on nothing. With one bit for each pixel it's small enough
/// without compressing it, so its data is stored as it is.
fn png(dots: &[Vec<bool>], color: [u8; 3]) -> Vec<u8> {
    let (width, height) = (dots[0].len(), dots.len());
    let mut lines = Vec::with_capacity((width.div_ceil(8) + 1) * height);
    for row in dots {
        lines.push(0);
        lines.extend(row.chunks(8).map(|bits| {
            bits.iter().enumerate().fold(0_u8, |byte, (index, bit)| {
                byte | (u8::from(*bit) << (7 - index))
            })
        }));
    }
    let mut stored = vec![0x78, 0x01];
    let mut blocks = lines.chunks(usize::from(u16::MAX)).peekable();
    while let Some(block) = blocks.next() {
        let length = block.len() as u16;
        stored.push(u8::from(blocks.peek().is_none()));
        stored.extend(length.to_le_bytes());
        stored.extend((!length).to_le_bytes());
        stored.extend(block);
    }
    stored.extend(adler32(&lines).to_be_bytes());

    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    // One bit deep, with colours from a palette, and the usual settings for the rest.
    header.extend([1, 3, 0, 0, 0]);
    let mut palette = vec![0; 3];
    palette.extend(color);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunks: [(&[u8; 4], &[u8]); 5] = [
        (b"IHDR", &header),
        (b"PLTE", &palette),
        // The first colour is see-through.
        (b"tRNS", &[0]),
        (b"IDAT", &stored),
        (b"IEND", &[]),
    ];
    for (name, data) in chunks {
        out.extend((data.len() as u32).to_be_bytes());
        out.extend(*name);
        out.extend(data);
        out.extend(crc32(&[name, data]).to_be_bytes());
    }
    out
}

/// An envelope's shape as a PNG: a plain line of solid dots, as on a small LCD.
pub fn envelope_png(envelope: &Envelope, color: [u8; 3], columns: usize, rows: usize) -> Vec<u8> {
    let (across, down) = (columns * CELL_DOTS_ACROSS, rows * CELL_DOTS_DOWN);
    let mut dots = vec![vec![false; across * DOT_PIXELS]; down * DOT_PIXELS];
    for (column, (high, low)) in envelope_columns(envelope, across, down)
        .into_iter()
        .enumerate()
    {
        for row in high..=low {
            for line in &mut dots[row * DOT_PIXELS..(row + 1) * DOT_PIXELS] {
                line[column * DOT_PIXELS..(column + 1) * DOT_PIXELS].fill(true);
            }
        }
    }
    png(&dots, color)
}

/// Write one command, wrapped so tmux passes it on.
fn command(out: &mut impl Write, controls: &str, data: &[u8], in_tmux: bool) -> io::Result<()> {
    if in_tmux {
        out.write_all(b"\x1bPtmux;\x1b\x1b_G")?;
    } else {
        out.write_all(b"\x1b_G")?;
    }
    out.write_all(controls.as_bytes())?;
    out.write_all(b";")?;
    out.write_all(data)?;
    if in_tmux {
        out.write_all(b"\x1b\x1b\\\x1b\\")
    } else {
        out.write_all(b"\x1b\\")
    }
}

/// A PNG to send, and how many characters it covers.
pub struct Image<'a> {
    pub id: u32,
    pub png: &'a [u8],
    pub columns: usize,
    pub rows: usize,
}

/// Send an image, in pieces. `q=2` keeps the terminal from answering, as its answers would arrive as key presses.
pub fn transmit(out: &mut impl Write, image: &Image, in_tmux: bool) -> io::Result<()> {
    let encoded = STANDARD.encode(image.png);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        let controls = if first {
            let Image {
                id, columns, rows, ..
            } = image;
            format!("a=T,U=1,f=100,i={id},c={columns},r={rows},q=2,m={more}")
        } else {
            format!("q=2,m={more}")
        };
        command(out, &controls, chunk, in_tmux)?;
        first = false;
    }
    Ok(())
}

/// Why the images can't reach the terminal, if that can be told.
pub fn problem() -> Option<&'static str> {
    std::env::var_os("TMUX")?;
    let output = Command::new("tmux")
        .args(["show", "-Apv", "allow-passthrough"])
        .output()
        .ok()?;
    let setting = String::from_utf8_lossy(&output.stdout);
    (!matches!(setting.trim(), "on" | "all")).then_some(
        "--kitty-graphics needs tmux to pass images on: run `tmux set -g allow-passthrough on`",
    )
}

/// The envelope images the terminal has. Each envelope keeps one with the cursor on it and one
/// without, so moving the cursor sends nothing.
///
/// A changed image gets a new id, rather than replacing the one with the old id. Kitty only looks at
/// placeholders on lines that have changed, and replacing an image disconnects them, so it would stay
/// blank. A new id changes the placeholders' colour, which makes kitty look again.
pub struct Graphics {
    base: u32,
    next: u32,
    images: [[Option<(Envelope, u32)>; 2]; TRACK_ENVELOPES],
    /// Images to forget once the screen has moved on from them.
    retired: Vec<u32>,
    in_tmux: bool,
}

impl Graphics {
    /// Ready to send images, with ids another running copy won't be using.
    pub fn new() -> Self {
        Self {
            base: (std::process::id() & 0xFFFF) << 8,
            next: 0,
            images: [[None; 2]; TRACK_ENVELOPES],
            retired: Vec::new(),
            in_tmux: std::env::var_os("TMUX").is_some(),
        }
    }

    /// An id no image has at the moment.
    fn new_id(&mut self) -> u32 {
        loop {
            self.next = self.next % 0xFF + 1;
            let id = self.base | self.next;
            let in_use = self
                .images
                .iter()
                .flatten()
                .flatten()
                .any(|image| image.1 == id)
                || self.retired.contains(&id);
            if !in_use {
                return id;
            }
        }
    }

    /// Send the images the screen is about to show, if the terminal lacks them, and say which to use.
    pub fn sync(&mut self, app: &mut App, out: &mut impl Write) -> io::Result<()> {
        for id in self.retired.drain(..) {
            command(out, &format!("a=d,d=I,i={id},q=2"), &[], self.in_tmux)?;
        }
        let Some(track) = app.song().tracks.get(app.cursor.track) else {
            return out.flush();
        };
        if app.view != View::Track || app.track_page != Page::Mod {
            return out.flush();
        }
        let envelopes = track.envelopes;
        let edited = envelope_being_edited(app);
        let mut ids = [0; TRACK_ENVELOPES];
        for (slot, envelope) in envelopes.iter().enumerate() {
            let active = edited == Some(slot);
            let held = self.images[slot][usize::from(active)];
            ids[slot] = match held {
                Some((drawn, id)) if drawn == *envelope => id,
                _ => {
                    let style = if active {
                        theme::active()
                    } else {
                        theme::dim()
                    };
                    let color = match style.fg {
                        Some(Color::Rgb(red, green, blue)) => [red, green, blue],
                        _ => [255; 3],
                    };
                    let id = self.new_id();
                    let png = envelope_png(envelope, color, DIAGRAM_WIDTH, DIAGRAM_HEIGHT);
                    let image = Image {
                        id,
                        png: &png,
                        columns: DIAGRAM_WIDTH,
                        rows: DIAGRAM_HEIGHT,
                    };
                    transmit(out, &image, self.in_tmux)?;
                    self.retired.extend(held.map(|(_, old)| old));
                    self.images[slot][usize::from(active)] = Some((*envelope, id));
                    id
                }
            };
        }
        app.envelope_images = Some(ids);
        out.flush()
    }

    /// Have the terminal forget the images.
    pub fn forget(&self, out: &mut impl Write) -> io::Result<()> {
        let held = self.images.iter().flatten().flatten().map(|image| &image.1);
        for id in held.chain(&self.retired) {
            command(out, &format!("a=d,d=I,i={id},q=2"), &[], self.in_tmux)?;
        }
        out.flush()
    }
}

impl Default for Graphics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Chunks = Vec<(String, Vec<u8>)>;

    /// A PNG's chunks, having checked their CRCs, and its pixels.
    fn decode(png: &[u8]) -> (Chunks, Vec<Vec<bool>>) {
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let mut chunks = Vec::new();
        let mut at = 8;
        while at < png.len() {
            let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let (name, data) = (&png[at + 4..at + 8], &png[at + 8..at + 8 + length]);
            let crc =
                u32::from_be_bytes(png[at + 8 + length..at + 12 + length].try_into().unwrap());
            assert_eq!(crc32(&[name, data]), crc);
            chunks.push((String::from_utf8(name.to_vec()).unwrap(), data.to_vec()));
            at += 12 + length;
        }
        let header = &chunks[0].1;
        let width = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let idat = &chunks.iter().find(|(name, _)| name == "IDAT").unwrap().1;
        let mut lines = Vec::new();
        let mut at = 2;
        loop {
            let last = idat[at] == 1;
            let length = usize::from(u16::from_le_bytes([idat[at + 1], idat[at + 2]]));
            let check = u16::from_le_bytes([idat[at + 3], idat[at + 4]]);
            assert_eq!(check, !(length as u16));
            lines.extend(&idat[at + 5..at + 5 + length]);
            at += 5 + length;
            if last {
                break;
            }
        }
        assert_eq!(&idat[at..], adler32(&lines).to_be_bytes());
        let stride = width.div_ceil(8) + 1;
        let pixels = lines
            .chunks(stride)
            .map(|line| {
                assert_eq!(line[0], 0);
                (0..width)
                    .map(|x| line[1 + x / 8] & (0x80 >> (x % 8)) != 0)
                    .collect()
            })
            .collect();
        (chunks, pixels)
    }

    fn adsr(attack_ms: u16, decay_ms: u16, sustain: u8, release_ms: u16) -> Envelope {
        Envelope {
            attack_ms,
            decay_ms,
            sustain,
            release_ms,
            ..Envelope::default()
        }
    }

    fn sent(in_tmux: bool, png: &[u8]) -> Vec<u8> {
        let image = Image {
            id: 7,
            png,
            columns: 28,
            rows: 4,
        };
        let mut out = Vec::new();
        transmit(&mut out, &image, in_tmux).unwrap();
        out
    }

    #[test]
    fn placeholders_are_one_column_each_and_say_where_they_are() {
        let lines = placeholder_lines(28, 4);
        assert_eq!(lines.len(), 4);
        for (row, line) in lines.iter().enumerate() {
            let chars: Vec<char> = line.chars().collect();
            assert_eq!(chars.len(), 28 * 3);
            for (column, cell) in chars.chunks(3).enumerate() {
                assert_eq!(cell, [PLACEHOLDER, POSITIONS[row], POSITIONS[column]]);
            }
            assert_eq!(ratatui::text::Line::raw(line.as_str()).width(), 28);
        }
    }

    #[test]
    fn a_color_holds_an_images_id() {
        assert_eq!(id_color(0x12_34_56), Color::Rgb(0x12, 0x34, 0x56));
    }

    #[test]
    fn the_checksums_match_known_values() {
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn an_envelope_is_a_small_png_of_one_colour_on_nothing() {
        let png = envelope_png(&adsr(1, 200, 0, 50), [1, 2, 3], 28, 4);
        assert!(png.len() < 10_000, "{}", png.len());
        let (chunks, _) = decode(&png);
        let names: Vec<&str> = chunks.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["IHDR", "PLTE", "tRNS", "IDAT", "IEND"]);
        assert_eq!(chunks[1].1, [0, 0, 0, 1, 2, 3]);
        assert_eq!(chunks[2].1, [0]);
    }

    #[test]
    fn the_pixels_are_the_envelopes_line_as_solid_dots() {
        // A held half has one line part way up, with nothing above or below it.
        let envelope = adsr(0, 0, 50, 0);
        let (_, pixels) = decode(&envelope_png(&envelope, [255; 3], 28, 4));
        let (across, down) = (28 * CELL_DOTS_ACROSS, 4 * CELL_DOTS_DOWN);
        assert_eq!(
            (pixels[0].len(), pixels.len()),
            (across * DOT_PIXELS, down * DOT_PIXELS)
        );
        let spans = envelope_columns(&envelope, across, down);
        for (y, line) in pixels.iter().enumerate() {
            for (x, pixel) in line.iter().enumerate() {
                let (column, row) = (x / DOT_PIXELS, y / DOT_PIXELS);
                let (high, low) = spans[column];
                assert_eq!(*pixel, (high..=low).contains(&row), "dot {column}, {row}");
            }
        }
    }

    #[test]
    fn an_image_is_sent_in_pieces_the_protocol_takes() {
        let pixels = vec![9; 10_000];
        let out = String::from_utf8(sent(false, &pixels)).unwrap();
        let commands: Vec<&str> = out
            .split("\x1b\\")
            .filter(|command| !command.is_empty())
            .collect();
        assert!(commands.len() > 1);
        assert!(commands[0].starts_with("\x1b_Ga=T,U=1,f=100,i=7,c=28,r=4,q=2,m=1;"));
        let mut data = String::new();
        for (index, command) in commands.iter().enumerate() {
            let (controls, chunk) = command.split_once(';').unwrap();
            let last = index == commands.len() - 1;
            assert!(
                controls.ends_with(if last { "m=0" } else { "m=1" }),
                "{controls}"
            );
            assert!(controls.contains("q=2"), "{controls}");
            assert!(chunk.len() <= CHUNK);
            if !last {
                assert_eq!(chunk.len() % 4, 0);
            }
            data.push_str(chunk);
        }
        assert_eq!(STANDARD.decode(data).unwrap(), pixels);
    }

    #[test]
    fn tmux_gets_each_piece_wrapped_with_its_escapes_doubled() {
        let out = String::from_utf8(sent(true, &[1, 2, 3, 4])).unwrap();
        assert!(out.starts_with("\x1bPtmux;\x1b\x1b_Ga=T"), "{out:?}");
        assert!(out.ends_with("\x1b\x1b\\\x1b\\"), "{out:?}");
    }
}
