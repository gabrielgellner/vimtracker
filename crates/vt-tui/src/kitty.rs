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
use crate::views::braille::level_at;
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
/// The pixels drawn for each character. The terminal scales them to fit.
const CELL_WIDTH: usize = 16;
const CELL_HEIGHT: usize = 32;
/// Room above and below a curve, so its line isn't cut off.
const MARGIN: f32 = 3.0;
/// How wide a curve's line is, in pixels.
const LINE: f32 = 3.0;
/// How strong the fill under a curve is just below it, and at the bottom.
const FILL_TOP: f32 = 0.35;
const FILL_BOTTOM: f32 = 0.06;

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

/// How far a point is from a line between two others.
fn distance(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> f32 {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx * dx + dy * dy;
    let along = if length > 0.0 {
        (((point.0 - from.0) * dx + (point.1 - from.1) * dy) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (point.0 - from.0 - along * dx).hypot(point.1 - from.1 - along * dy)
}

/// An envelope's shape as RGBA pixels: a smooth line with a soft fill under it.
pub fn envelope_pixels(
    envelope: &Envelope,
    color: [u8; 3],
    width: usize,
    height: usize,
) -> Vec<u8> {
    let (top, bottom) = (MARGIN, height as f32 - 1.0 - MARGIN);
    let last = (width - 1) as f32;
    let curve: Vec<f32> = (0..width)
        .map(|x| bottom - level_at(envelope, x as f32 / last) * (bottom - top))
        .collect();
    // It rises from silence and returns to it, even if its first or last stage takes no time.
    let mut points = vec![(0.0, bottom)];
    points.extend(curve.iter().enumerate().map(|(x, y)| (x as f32, *y)));
    points.push((last, bottom));

    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for (x, curve_y) in curve.iter().enumerate() {
            let point = (x as f32, y as f32);
            // Only the parts of the line close to this column can reach it.
            let near = x.saturating_sub(2)..(x + 4).min(points.len() - 1);
            let closest = near.fold(f32::MAX, |closest, index| {
                closest.min(distance(point, points[index], points[index + 1]))
            });
            let line = (LINE / 2.0 + 0.5 - closest).clamp(0.0, 1.0);
            let fill = if point.1 >= *curve_y && point.1 <= bottom {
                let depth = (point.1 - curve_y) / (bottom - top);
                FILL_TOP + (FILL_BOTTOM - FILL_TOP) * depth
            } else {
                0.0
            };
            let alpha = line.max(fill);
            pixels.extend([color[0], color[1], color[2], (alpha * 255.0) as u8]);
        }
    }
    pixels
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

/// Where an image's pixels are from, and how many characters it covers.
pub struct Image<'a> {
    pub id: u32,
    pub pixels: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub columns: usize,
    pub rows: usize,
}

/// Send an image, in pieces. An image already sent with this id is replaced. `q=2` keeps the
/// terminal from answering, as its answers would arrive as key presses.
pub fn transmit(out: &mut impl Write, image: &Image, in_tmux: bool) -> io::Result<()> {
    let encoded = STANDARD.encode(image.pixels);
    let mut chunks = encoded.as_bytes().chunks(CHUNK).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = u8::from(chunks.peek().is_some());
        let controls = if first {
            let Image {
                id,
                width,
                height,
                columns,
                rows,
                ..
            } = image;
            format!("a=T,U=1,f=32,s={width},v={height},i={id},c={columns},r={rows},q=2,m={more}")
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
        let (width, height) = (DIAGRAM_WIDTH * CELL_WIDTH, DIAGRAM_HEIGHT * CELL_HEIGHT);
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
                    let pixels = envelope_pixels(envelope, color, width, height);
                    let image = Image {
                        id,
                        pixels: &pixels,
                        width,
                        height,
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

    const WIDTH: usize = 112;
    const HEIGHT: usize = 64;

    fn alpha(pixels: &[u8], x: usize, y: usize) -> u8 {
        pixels[(y * WIDTH + x) * 4 + 3]
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

    fn sent(in_tmux: bool, pixels: &[u8]) -> Vec<u8> {
        let image = Image {
            id: 7,
            pixels,
            width: 2,
            height: 2,
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
    fn the_pixels_follow_the_envelope() {
        let pixels = envelope_pixels(&adsr(0, 0, 50, 0), [1, 2, 3], WIDTH, HEIGHT);
        assert_eq!(pixels.len(), WIDTH * HEIGHT * 4);
        assert_eq!(&pixels[..3], &[1, 2, 3]);
        let middle = HEIGHT / 2;
        // A held half: its line is halfway up, with nothing above and a fill below.
        assert_eq!(alpha(&pixels, WIDTH / 2, middle), 255);
        assert_eq!(alpha(&pixels, WIDTH / 2, middle - 10), 0);
        let fill = alpha(&pixels, WIDTH / 2, middle + 10);
        assert!(fill > 0 && fill < 255, "{fill}");
    }

    #[test]
    fn the_line_has_no_gap_where_it_rises_quickly() {
        let pixels = envelope_pixels(&adsr(0, 200, 0, 0), [255; 3], WIDTH, HEIGHT);
        for y in 4..HEIGHT - 4 {
            let widest = (0..WIDTH).map(|x| alpha(&pixels, x, y)).max().unwrap();
            assert!(widest > 100, "row {y} is at {widest}");
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
        assert!(commands[0].starts_with("\x1b_Ga=T,U=1,f=32,s=2,v=2,i=7,c=28,r=4,q=2,m=1;"));
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
