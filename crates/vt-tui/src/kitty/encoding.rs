//! PNGs written by hand: zlib's stored blocks and their checksums, a picture in a few colours
//! with its palette, and the waveform's and the scope's lines of dots.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;

use crate::views::meter::SCOPE_HEIGHT;
use crate::waveform;

use super::{CELL_DOTS_ACROSS, CELL_DOTS_DOWN, DOT_PIXELS};

/// The CRC that PNG chunks end with.
pub(super) fn crc32(parts: &[&[u8]]) -> u32 {
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
pub(super) fn adler32(data: &[u8]) -> u32 {
    let (a, b) = data.iter().fold((1_u32, 0_u32), |(a, b), byte| {
        let a = (a + u32::from(*byte)) % 65_521;
        (a, (b + a) % 65_521)
    });
    (b << 16) | a
}

/// The most a stored block of zlib data holds.
const STORED_BLOCK: usize = u16::MAX as usize;

/// Data as zlib data that isn't compressed, in stored blocks.
fn stored(data: &[u8]) -> Vec<u8> {
    let mut stored = vec![0x78, 0x01];
    let mut blocks = data.chunks(STORED_BLOCK).peekable();
    while let Some(block) = blocks.next() {
        let length = block.len() as u16;
        stored.push(u8::from(blocks.peek().is_none()));
        stored.extend(length.to_le_bytes());
        stored.extend((!length).to_le_bytes());
        stored.extend(block);
    }
    stored.extend(adler32(data).to_be_bytes());
    stored
}

/// A PNG of solid dots in one colour on nothing.
pub(super) fn png(dots: &[Vec<bool>], color: [u8; 3]) -> Vec<u8> {
    Picture {
        width: dots[0].len(),
        height: dots.len(),
        pixels: dots.iter().flatten().map(|dot| u8::from(*dot)).collect(),
        colors: vec![color],
    }
    .png()
}

/// A drawing in a few colours, each with a meaning. A pixel is `0` for nothing, which lets the
/// cell's background show through, or `n` for the nth of its colours.
pub struct Picture {
    pub width: usize,
    pub height: usize,
    /// Row by row, from the top left.
    pub pixels: Vec<u8>,
    pub colors: Vec<[u8; 3]>,
}

impl Picture {
    /// Nothing but nothing, ready to draw on.
    pub fn new(width: usize, height: usize, colors: Vec<[u8; 3]>) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
            colors,
        }
    }

    /// As a PNG with a palette, using as few bits for each pixel as its colours need: one for a
    /// single colour, two for up to 3, and four for up to 15.
    ///
    /// Pixels that fit in one stored block, as every small drawing's do, are stored as they are,
    /// which costs a few kilobytes at most. Bigger ones are compressed: a waveform the width of the
    /// screen, 2400 by 546 pixels in four colours, goes from 876 KB sent to 16 KB, so writing it
    /// never waits on the terminal to read it.
    pub fn png(&self) -> Vec<u8> {
        indexed_png(self.width, &self.pixels, &self.colors, &[0], false)
    }
}

/// Pixels as a PNG with a palette, each pixel the number of a colour, with `0` for nothing, which
/// is see-through. `alphas` is how see-through each palette entry is, from that first one, and the
/// entries after it are solid. It uses as few bits for each pixel as its colours need: one for a
/// single colour, two for up to 3, and four for up to 15. The pixels are compressed if `compress`
/// says to, or if they're more than one stored block.
pub(super) fn indexed_png(
    width: usize,
    pixels: &[u8],
    colors: &[[u8; 3]],
    alphas: &[u8],
    compress: bool,
) -> Vec<u8> {
    assert!(colors.len() < 16, "a picture has a few colours");
    let height = pixels.len() / width.max(1);
    let depth = match colors.len() {
        1 => 1,
        2..=3 => 2,
        _ => 4,
    };
    let per_byte = 8 / depth;
    let mut lines = Vec::with_capacity((width.div_ceil(per_byte) + 1) * height);
    for row in pixels.chunks(width) {
        lines.push(0);
        lines.extend(row.chunks(per_byte).map(|pixels| {
            pixels
                .iter()
                .enumerate()
                .fold(0_u8, |byte, (index, pixel)| {
                    byte | (pixel << (8 - depth * (index + 1)))
                })
        }));
    }
    let data = if lines.len() <= STORED_BLOCK && !compress {
        stored(&lines)
    } else {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&lines).expect("writing to memory");
        encoder.finish().expect("writing to memory")
    };

    let mut header = Vec::new();
    header.extend((width as u32).to_be_bytes());
    header.extend((height as u32).to_be_bytes());
    // Colours from a palette, and the usual settings for the rest.
    header.extend([depth as u8, 3, 0, 0, 0]);
    let mut palette = vec![0; 3];
    palette.extend(colors.iter().flatten());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let chunks: [(&[u8; 4], &[u8]); 5] = [
        (b"IHDR", &header),
        (b"PLTE", &palette),
        (b"tRNS", alphas),
        (b"IDAT", &data),
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

/// A line as a PNG of solid dots, with each dot a block of pixels.
fn line_png(spans: &[(usize, usize)], rows: usize, color: [u8; 3]) -> Vec<u8> {
    let mut dots = vec![vec![false; spans.len() * DOT_PIXELS]; rows * DOT_PIXELS];
    for (column, (high, low)) in spans.iter().enumerate() {
        for row in *high..=*low {
            for line in &mut dots[row * DOT_PIXELS..(row + 1) * DOT_PIXELS] {
                line[column * DOT_PIXELS..(column + 1) * DOT_PIXELS].fill(true);
            }
        }
    }
    png(&dots, color)
}

/// The scope's picture as a PNG, from the rows it covers in each column of dots.
pub fn scope_png(spans: &[(usize, usize)], color: [u8; 3]) -> Vec<u8> {
    line_png(spans, SCOPE_HEIGHT * CELL_DOTS_DOWN, color)
}

/// The rows a waveform covers in each column of dots, for a drawing this many characters across
/// and down, to keep in place of the sample and draw later with [`spans_png`].
pub fn waveform_spans(frames: &[[f32; 2]], columns: usize, rows: usize) -> Vec<(usize, usize)> {
    waveform::spans(
        frames,
        &[],
        columns * CELL_DOTS_ACROSS,
        rows * CELL_DOTS_DOWN,
    )
}

/// A drawing worked out by [`waveform_spans`] as a PNG, `rows` characters down.
pub fn spans_png(spans: &[(usize, usize)], rows: usize, color: [u8; 3]) -> Vec<u8> {
    line_png(spans, rows * CELL_DOTS_DOWN, color)
}
